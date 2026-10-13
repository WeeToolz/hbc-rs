//! A contiguous executable buffer reserved from the top of Arena2.

use alloc::vec::Vec;
#[cfg(any(target_arch = "powerpc", test))]
use core::ops::Range;

#[cfg(any(target_arch = "powerpc", test))]
const ARENA2_MIN_SLACK: usize = 512 * 1024;
#[cfg(any(target_arch = "powerpc", test))]
const BLOB_ALIGNMENT: usize = 32;

/// Buffer for an executable read from storage.
///
/// On Wii, this uses the same top-down Arena2 allocation pattern as C HBC.
/// This keeps a large executable independent from fragmentation in HBC's
/// general-purpose Rust heap.
pub struct LaunchSource {
    #[cfg(target_arch = "powerpc")]
    pointer: Option<core::ptr::NonNull<u8>>,
    #[cfg(target_arch = "powerpc")]
    length: usize,
    #[cfg(target_arch = "powerpc")]
    previous_arena_hi: Option<*mut core::ffi::c_void>,
    #[cfg(target_arch = "powerpc")]
    bytes: Option<Vec<u8>>,
    #[cfg(not(target_arch = "powerpc"))]
    bytes: Vec<u8>,
}

impl LaunchSource {
    /// Reserve a contiguous buffer for `length` bytes.
    pub fn try_allocate(length: usize) -> Option<Self> {
        if length == 0 {
            return None;
        }

        #[cfg(target_arch = "powerpc")]
        {
            let irq_state = ogc_irq::irq_disable();
            let arena_low = ogc_system::get_arena2_lo() as usize;
            let previous_arena_hi = ogc_system::get_arena2_hi();
            let pointer = launch_blob_bounds(arena_low, previous_arena_hi as usize, length)
                .and_then(|bounds| core::ptr::NonNull::new(bounds.start as *mut u8));
            let Some(pointer) = pointer else {
                ogc_irq::irq_restore(irq_state);
                return None;
            };
            // SAFETY: The checked bounds reserve this range between Arena2Lo
            // and the old Arena2Hi. This function runs on the HBC app thread;
            // changing Arena2Hi makes the region unavailable to other users.
            unsafe { ogc_system::set_arena2_hi(pointer.as_ptr().cast()) };
            ogc_irq::irq_restore(irq_state);
            Some(Self {
                pointer: Some(pointer),
                length,
                previous_arena_hi: Some(previous_arena_hi),
                bytes: None,
            })
        }

        #[cfg(not(target_arch = "powerpc"))]
        {
            let mut bytes = Vec::new();
            bytes.try_reserve_exact(length).ok()?;
            bytes.resize(length, 0);
            Some(Self { bytes })
        }
    }

    /// Keep a payload that already owns its input bytes, such as WiiLoad data.
    #[cfg(target_arch = "powerpc")]
    pub fn from_vec(bytes: Vec<u8>) -> Self {
        Self {
            length: bytes.len(),
            pointer: None,
            previous_arena_hi: None,
            bytes: Some(bytes),
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        #[cfg(target_arch = "powerpc")]
        // SAFETY: The buffer is reserved until this object is dropped.
        unsafe {
            if let Some(pointer) = self.pointer {
                core::slice::from_raw_parts(pointer.as_ptr(), self.length)
            } else {
                self.bytes.as_deref().unwrap_or_default()
            }
        }
        #[cfg(not(target_arch = "powerpc"))]
        {
            &self.bytes
        }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        #[cfg(target_arch = "powerpc")]
        // SAFETY: This object uniquely owns its reserved buffer.
        unsafe {
            if let Some(pointer) = self.pointer {
                core::slice::from_raw_parts_mut(pointer.as_ptr(), self.length)
            } else {
                self.bytes.as_deref_mut().unwrap_or_default()
            }
        }
        #[cfg(not(target_arch = "powerpc"))]
        {
            &mut self.bytes
        }
    }
}

#[cfg(target_arch = "powerpc")]
impl Drop for LaunchSource {
    fn drop(&mut self) {
        // Match the stack allocation discipline used by C HBC's blob_free.
        // If another owner changed Arena2Hi, keep this reservation in place;
        // resetting it could expose memory that is still in use.
        let (Some(pointer), Some(previous_arena_hi)) = (self.pointer, self.previous_arena_hi)
        else {
            return;
        };
        let irq_state = ogc_irq::irq_disable();
        if ogc_system::get_arena2_hi() == pointer.as_ptr().cast() {
            // SAFETY: The current high boundary still marks this exact
            // reservation as the most recent top-down allocation.
            unsafe { ogc_system::set_arena2_hi(previous_arena_hi) };
        }
        ogc_irq::irq_restore(irq_state);
    }
}

#[cfg(any(target_arch = "powerpc", test))]
fn launch_blob_bounds(arena_low: usize, arena_high: usize, length: usize) -> Option<Range<usize>> {
    if length == 0 {
        return None;
    }
    let start = arena_high.checked_sub(length)? & !(BLOB_ALIGNMENT - 1);
    let minimum_start = arena_low.checked_add(ARENA2_MIN_SLACK)?;
    (start >= minimum_start).then_some(start..arena_high)
}

#[cfg(test)]
mod tests {
    use super::{launch_blob_bounds, LaunchSource, ARENA2_MIN_SLACK};

    #[test]
    fn launch_blob_leaves_required_arena_slack_and_aligns_start() {
        let low = 0x9000_0000;
        let high = low + 8 * 1024 * 1024;
        let bounds = launch_blob_bounds(low, high, 1024 * 1024).unwrap();
        assert_eq!(bounds.start & 31, 0);
        assert!(bounds.start >= low + ARENA2_MIN_SLACK);
        assert!(bounds.end <= high);
        assert!(bounds.len() >= 1024 * 1024);
    }

    #[test]
    fn launch_blob_rejects_zero_and_insufficient_space() {
        let low = 0x9000_0000;
        assert!(launch_blob_bounds(low, low + ARENA2_MIN_SLACK + 64, 0).is_none());
        assert!(launch_blob_bounds(low, low + ARENA2_MIN_SLACK + 64, 65).is_none());
    }

    #[test]
    fn launch_source_exposes_the_reserved_bytes_for_storage_reads() {
        let mut source = LaunchSource::try_allocate(4).unwrap();
        source.as_mut_slice().copy_from_slice(b"DOL!");
        assert_eq!(source.as_slice(), b"DOL!");
    }
}
