//! HBC's global allocator backed by the shared `ogc-alloc` MEM2 allocator.

pub use ogc_alloc::{
    ArenaAllocator, InitError, DEFAULT_MAX_MEM2_HEAP_BYTES, DEFAULT_MEM2_RESERVE_BYTES,
};

#[cfg(target_arch = "powerpc")]
#[global_allocator]
pub static WII_ALLOCATOR: ArenaAllocator = ArenaAllocator::new();

/// Reserve MEM2 for HBC's Rust allocations.
///
/// # Safety
/// Call after system startup and before any Rust allocation.
#[cfg(target_arch = "powerpc")]
pub unsafe fn initialize_wii(max_heap_bytes: usize, reserve_bytes: usize) -> Result<(), InitError> {
    let low = ogc_system::get_arena2_lo().cast::<u8>();
    let high = ogc_system::get_arena2_hi().cast::<u8>();
    unsafe {
        ogc_alloc::initialize_wii(
            &WII_ALLOCATOR,
            low,
            high,
            max_heap_bytes,
            reserve_bytes,
            |new_low| ogc_system::set_arena2_lo(new_low),
        )?
    };
    Ok(())
}

/// The Wii MEM2 arena is not available in host builds.
#[cfg(not(target_arch = "powerpc"))]
pub unsafe fn initialize_wii(
    _max_heap_bytes: usize,
    _reserve_bytes: usize,
) -> Result<(), InitError> {
    Err(InitError::NotEnoughArena)
}
