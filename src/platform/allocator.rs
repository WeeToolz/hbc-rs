//! Coalescing allocator for a reserved range of MEM2.
//!
//! The allocator owns only the range passed to `initialize_region`. The Wii
//! initializer reserves that range by advancing arena2's low boundary. It
//! leaves the requested space at the high end for other runtime users.

use core::cell::UnsafeCell;
use core::cmp;
use core::ptr::{self, null_mut};
use core::sync::atomic::{AtomicBool, Ordering};

const BLOCK_ALIGNMENT: usize = 16;
const BLOCK_ALIGN_MASK: usize = BLOCK_ALIGNMENT - 1;

#[repr(C, align(16))]
struct Block {
    size: usize,
    previous_size: usize,
    next_free: *mut Block,
    previous_free: *mut Block,
    is_free: bool,
}

const HEADER_SIZE: usize = core::mem::size_of::<Block>();
const MIN_BLOCK_SIZE: usize = HEADER_SIZE + BLOCK_ALIGNMENT;

struct State {
    start: usize,
    end: usize,
    free_head: *mut Block,
    initialized: bool,
}

impl State {
    const fn new() -> Self {
        Self {
            start: 0,
            end: 0,
            free_head: null_mut(),
            initialized: false,
        }
    }
}

/// A bounded free-list allocator. It coalesces adjacent free blocks.
pub struct ArenaAllocator {
    locked: AtomicBool,
    state: UnsafeCell<State>,
}

// State access is serialized by `locked`.
unsafe impl Sync for ArenaAllocator {}

impl ArenaAllocator {
    pub const fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
            state: UnsafeCell::new(State::new()),
        }
    }

    /// Start using an exclusively owned memory range.
    ///
    /// # Safety
    /// The caller must guarantee that `start..start + len` is writable memory,
    /// is not used by another allocator or runtime service, and remains valid
    /// for the lifetime of this allocator. No live allocations may exist when
    /// this method is called.
    pub unsafe fn initialize_region(&self, start: *mut u8, len: usize) -> Result<(), InitError> {
        let _guard = self.lock();
        let start_addr = start as usize;
        let aligned_start =
            align_up(start_addr, BLOCK_ALIGNMENT).ok_or(InitError::AddressOverflow)?;
        let end_addr = start_addr
            .checked_add(len)
            .ok_or(InitError::AddressOverflow)?;
        let end = end_addr & !BLOCK_ALIGN_MASK;
        let usable_len = end.checked_sub(aligned_start).ok_or(InitError::TooSmall)?;
        if start.is_null() || usable_len < MIN_BLOCK_SIZE {
            return Err(InitError::TooSmall);
        }

        let first = aligned_start as *mut Block;
        // SAFETY: The caller guarantees exclusive writable ownership of the
        // range. The header fits because usable_len >= MIN_BLOCK_SIZE.
        unsafe {
            ptr::write(
                first,
                Block {
                    size: usable_len,
                    previous_size: 0,
                    next_free: null_mut(),
                    previous_free: null_mut(),
                    is_free: true,
                },
            );
            let state = &mut *self.state.get();
            state.start = aligned_start;
            state.end = end;
            state.free_head = first;
            state.initialized = true;
        }
        Ok(())
    }

    /// Allocate memory with the requested alignment. This is exposed to test
    /// allocator behavior on hosts without creating a host global allocator.
    pub fn allocate(&self, size: usize, alignment: usize) -> *mut u8 {
        if alignment == 0 || !alignment.is_power_of_two() {
            return null_mut();
        }
        let _guard = self.lock();
        // SAFETY: All state changes are protected by the lock.
        unsafe { self.allocate_locked(size, alignment) }
    }

    /// Release a pointer returned by this allocator.
    ///
    /// # Safety
    /// `pointer` must be a live pointer returned by this allocator. `size` and
    /// `alignment` must match the original allocation request.
    pub unsafe fn deallocate(&self, pointer: *mut u8, size: usize, alignment: usize) {
        if pointer.is_null() || alignment == 0 || !alignment.is_power_of_two() {
            return;
        }
        let _guard = self.lock();
        // SAFETY: The caller promises pointer/layout validity. The checks in
        // release_locked also reject pointers outside the managed range.
        unsafe { self.release_locked(pointer, size, alignment) };
    }

    pub fn capacity(&self) -> usize {
        let _guard = self.lock();
        // SAFETY: State is protected by the lock.
        unsafe {
            let state = &*self.state.get();
            state.end.saturating_sub(state.start)
        }
    }

    unsafe fn allocate_locked(&self, size: usize, alignment: usize) -> *mut u8 {
        // SAFETY: Caller holds the lock.
        let state = unsafe { &mut *self.state.get() };
        if !state.initialized {
            return null_mut();
        }
        let requested = cmp::max(size, 1);
        let align = cmp::max(alignment, BLOCK_ALIGNMENT);
        let payload_bytes = match align_up(requested, BLOCK_ALIGNMENT) {
            Some(value) => value,
            None => return null_mut(),
        };
        let allocation_size = match HEADER_SIZE.checked_add(payload_bytes) {
            Some(value) => value,
            None => return null_mut(),
        };

        let mut block = state.free_head;
        while !block.is_null() {
            // SAFETY: Free-list nodes are initialized blocks inside the arena.
            let block_data = unsafe { &*block };
            let block_start = block as usize;
            let block_end = match block_start.checked_add(block_data.size) {
                Some(value) => value,
                None => return null_mut(),
            };
            let mut user = match align_up(block_start.saturating_add(HEADER_SIZE), align) {
                Some(value) => value,
                None => return null_mut(),
            };
            let mut allocation_start = user - HEADER_SIZE;
            let mut prefix = allocation_start - block_start;

            // Keep any leading fragment large enough to hold a block header
            // and a minimum aligned payload. Smaller padding is absorbed by
            // moving the user pointer forward by one requested alignment.
            if prefix != 0 && prefix < MIN_BLOCK_SIZE {
                user = match user.checked_add(align) {
                    Some(value) => value,
                    None => return null_mut(),
                };
                allocation_start = user - HEADER_SIZE;
                prefix = allocation_start - block_start;
            }

            let allocation_end = match allocation_start.checked_add(allocation_size) {
                Some(value) => value,
                None => return null_mut(),
            };
            if allocation_end > block_end {
                block = block_data.next_free;
                continue;
            }

            let mut allocated_size = allocation_size;
            let suffix = block_end - allocation_end;
            let split_suffix = suffix >= MIN_BLOCK_SIZE;
            if !split_suffix {
                allocated_size += suffix;
            }

            let previous_size = if prefix == 0 {
                block_data.previous_size
            } else {
                prefix
            };
            let old_next_free = block_data.next_free;
            let old_previous_free = block_data.previous_free;
            if prefix == 0 {
                unsafe { self.unlink_free(block, old_previous_free, old_next_free) };
            } else {
                // Retain the original header as the free prefix.
                unsafe {
                    (*block).size = prefix;
                    (*block).is_free = true;
                }
            }

            let allocated = allocation_start as *mut Block;
            // SAFETY: The allocation range was proven to fit the free block.
            unsafe {
                ptr::write(
                    allocated,
                    Block {
                        size: allocated_size,
                        previous_size,
                        next_free: null_mut(),
                        previous_free: null_mut(),
                        is_free: false,
                    },
                );
            }

            if split_suffix {
                let suffix_block = (allocation_start + allocated_size) as *mut Block;
                // SAFETY: suffix >= MIN_BLOCK_SIZE, so its header and payload
                // fit within the original free block.
                unsafe {
                    ptr::write(
                        suffix_block,
                        Block {
                            size: block_end - (suffix_block as usize),
                            previous_size: allocated_size,
                            next_free: null_mut(),
                            previous_free: null_mut(),
                            is_free: true,
                        },
                    );
                    let next = block_end as *mut Block;
                    if (next as usize) < state.end {
                        (*next).previous_size = (*suffix_block).size;
                    }
                    self.insert_free(suffix_block);
                }
            } else if block_end < state.end {
                // SAFETY: block_end is the start of the next physical block.
                unsafe { (*(block_end as *mut Block)).previous_size = allocated_size };
            }
            return user as *mut u8;
        }
        null_mut()
    }

    unsafe fn release_locked(&self, pointer: *mut u8, _size: usize, _alignment: usize) {
        // SAFETY: Caller holds lock and promises a valid allocation pointer.
        let state = unsafe { &mut *self.state.get() };
        if !state.initialized {
            return;
        }
        let user = pointer as usize;
        let header_addr = match user.checked_sub(HEADER_SIZE) {
            Some(value) => value,
            None => return,
        };
        if header_addr < state.start || header_addr >= state.end {
            return;
        }
        let mut block = header_addr as *mut Block;
        // SAFETY: A valid allocation points to its header. Bounds check above
        // rejects pointers outside the managed arena.
        unsafe {
            if (*block).is_free || (*block).size < HEADER_SIZE {
                return;
            }
            (*block).is_free = true;

            let block_end = (block as usize).checked_add((*block).size);
            if let Some(end) = block_end {
                if end < state.end {
                    let next = end as *mut Block;
                    if (*next).is_free {
                        self.unlink_free(next, (*next).previous_free, (*next).next_free);
                        (*block).size += (*next).size;
                    }
                }
            }

            let previous_size = (*block).previous_size;
            if previous_size != 0 {
                if let Some(previous_addr) = (block as usize).checked_sub(previous_size) {
                    if previous_addr >= state.start {
                        let previous = previous_addr as *mut Block;
                        if (*previous).is_free {
                            self.unlink_free(
                                previous,
                                (*previous).previous_free,
                                (*previous).next_free,
                            );
                            (*previous).size += (*block).size;
                            block = previous;
                        }
                    }
                }
            }

            let block_end = (block as usize) + (*block).size;
            if block_end < state.end {
                (*(block_end as *mut Block)).previous_size = (*block).size;
            }
            self.insert_free(block);
        }
    }

    unsafe fn unlink_free(&self, block: *mut Block, previous: *mut Block, next: *mut Block) {
        // SAFETY: Caller passes a member of this allocator's free list.
        unsafe {
            if previous.is_null() {
                (*self.state.get()).free_head = next;
            } else {
                (*previous).next_free = next;
            }
            if !next.is_null() {
                (*next).previous_free = previous;
            }
            (*block).next_free = null_mut();
            (*block).previous_free = null_mut();
        }
    }

    unsafe fn insert_free(&self, block: *mut Block) {
        // SAFETY: Caller passes a writable block inside the arena.
        unsafe {
            let head = (*self.state.get()).free_head;
            (*block).is_free = true;
            (*block).previous_free = null_mut();
            (*block).next_free = head;
            if !head.is_null() {
                (*head).previous_free = block;
            }
            (*self.state.get()).free_head = block;
        }
    }

    fn lock(&self) -> LockGuard<'_> {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        LockGuard { lock: &self.locked }
    }
}

impl Default for ArenaAllocator {
    fn default() -> Self {
        Self::new()
    }
}

struct LockGuard<'a> {
    lock: &'a AtomicBool,
}

impl Drop for LockGuard<'_> {
    fn drop(&mut self) {
        self.lock.store(false, Ordering::Release);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitError {
    TooSmall,
    AddressOverflow,
    NotEnoughArena,
}

/// Maximum managed heap size when the caller uses the recommended default.
pub const DEFAULT_MAX_MEM2_HEAP_BYTES: usize = 16 * 1024 * 1024;
/// Leave this many bytes at the top of MEM2 by default.
pub const DEFAULT_MEM2_RESERVE_BYTES: usize = 2 * 1024 * 1024;

/// Reserve a bounded low part of MEM2 for Rust allocations.
///
/// The current arena2 low boundary is the allocation start. The allocator
/// takes at most `max_heap_bytes` and leaves at least `reserve_bytes` above
/// the new low boundary for other runtime users.
#[cfg(target_arch = "powerpc")]
pub unsafe fn initialize_wii(max_heap_bytes: usize, reserve_bytes: usize) -> Result<(), InitError> {
    use core::ffi::c_void;
    use ogc_system::{get_arena2_hi, get_arena2_lo, set_arena2_lo};

    let low = get_arena2_lo() as usize;
    let high = get_arena2_hi() as usize;
    let available = high
        .checked_sub(low)
        .and_then(|size| size.checked_sub(reserve_bytes))
        .ok_or(InitError::NotEnoughArena)?;
    let heap_size = cmp::min(available, max_heap_bytes) & !BLOCK_ALIGN_MASK;
    let end = low
        .checked_add(heap_size)
        .ok_or(InitError::AddressOverflow)?;
    if low == 0 || heap_size < MIN_BLOCK_SIZE {
        return Err(InitError::NotEnoughArena);
    }
    // SAFETY: This initializer is unsafe because the caller must choose a
    // startup point before any other code uses the range being reserved.
    unsafe { WII_ALLOCATOR.initialize_region(low as *mut u8, end - low)? };
    // SAFETY: The allocator now owns [low, end), so advancing arena2's low
    // boundary prevents libogc arena users from allocating over it.
    unsafe { set_arena2_lo(end as *mut c_void) };
    Ok(())
}

#[cfg(target_arch = "powerpc")]
#[global_allocator]
pub static WII_ALLOCATOR: ArenaAllocator = ArenaAllocator::new();

#[cfg(target_arch = "powerpc")]
unsafe impl core::alloc::GlobalAlloc for ArenaAllocator {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        self.allocate(layout.size(), layout.align())
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: core::alloc::Layout) {
        // SAFETY: GlobalAlloc's caller must provide a valid pointer/layout pair.
        unsafe { self.deallocate(pointer, layout.size(), layout.align()) };
    }
}

fn align_up(value: usize, alignment: usize) -> Option<usize> {
    value
        .checked_add(alignment - 1)
        .map(|value| value & !(alignment - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[repr(align(64))]
    struct TestMemory<const N: usize>([u8; N]);

    #[test]
    fn allocations_are_aligned_and_reused_after_free() {
        let allocator = ArenaAllocator::new();
        let mut memory = TestMemory([0; 16 * 1024]);
        unsafe { allocator.initialize_region(memory.0.as_mut_ptr(), memory.0.len()) }.unwrap();
        let first = allocator.allocate(17, 64);
        assert!(!first.is_null());
        assert_eq!(first as usize % 64, 0);
        unsafe { allocator.deallocate(first, 17, 64) };
        let again = allocator.allocate(17, 64);
        assert_eq!(again, first);
    }

    #[test]
    fn adjacent_frees_coalesce_to_fit_a_larger_request() {
        let allocator = ArenaAllocator::new();
        let mut memory = TestMemory([0; 1024]);
        unsafe { allocator.initialize_region(memory.0.as_mut_ptr(), memory.0.len()) }.unwrap();
        let a = allocator.allocate(200, 16);
        let b = allocator.allocate(200, 16);
        let c = allocator.allocate(200, 16);
        assert!(!a.is_null() && !b.is_null() && !c.is_null());
        unsafe {
            allocator.deallocate(a, 200, 16);
            allocator.deallocate(b, 200, 16);
        }
        let joined = allocator.allocate(400, 16);
        assert_eq!(joined, a);
    }

    #[test]
    fn allocation_fails_cleanly_when_capacity_is_exhausted() {
        let allocator = ArenaAllocator::new();
        let mut memory = TestMemory([0; 512]);
        unsafe { allocator.initialize_region(memory.0.as_mut_ptr(), memory.0.len()) }.unwrap();
        let allocation = allocator.allocate(4096, 16);
        assert!(allocation.is_null());
    }

    #[test]
    fn rejects_invalid_region_sizes_and_alignments() {
        let allocator = ArenaAllocator::new();
        let mut memory = TestMemory([0; 64]);
        assert_eq!(
            unsafe { allocator.initialize_region(memory.0.as_mut_ptr(), 8) },
            Err(InitError::TooSmall)
        );
        assert!(allocator.allocate(8, 3).is_null());
    }
}
