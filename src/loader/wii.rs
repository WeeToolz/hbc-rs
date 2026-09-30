//! Wii-only code that writes and starts a validated executable image.

use super::{validate_load_plan, LoadPlan, ParseError};
use core::ffi::c_void;
use core::ops::Range;
use core::ptr;

/// Copy executable sections to Wii memory, clear their zero-fill tails, and
/// prepare instruction and data caches.
///
/// # Safety
///
/// `allowed_memory` must describe only writable Wii RAM that this loader may
/// replace. No live Rust data, stack, allocator state, or runtime service may
/// use those addresses. The caller must stop concurrent code that can read or
/// write the destination ranges. The source bytes must remain readable until
/// this function returns.
pub unsafe fn apply_load_plan(
    bytes: &[u8],
    plan: &LoadPlan,
    allowed_memory: Range<u32>,
) -> Result<(), ParseError> {
    validate_load_plan(bytes, plan, &allowed_memory)?;

    // Reject source/destination aliasing. A destination write could otherwise
    // overwrite bytes for a later segment, even when each individual copy is
    // overlap-safe.
    let source_start = bytes.as_ptr() as usize;
    let source_end = source_start
        .checked_add(bytes.len())
        .ok_or(ParseError::OutOfBounds)?;
    for segment in &plan.segments {
        let destination_start = segment.address as usize;
        let destination_end = destination_start
            .checked_add(segment.memory_size as usize)
            .ok_or(ParseError::AddressOverflow)?;
        if address_ranges_alias(source_start..source_end, destination_start..destination_end) {
            return Err(ParseError::InvalidSegment(
                "source overlaps a destination segment",
            ));
        }
    }

    for segment in &plan.segments {
        let destination = segment.address as usize as *mut u8;
        let source = bytes[segment.file_range.clone()].as_ptr();
        let file_size = segment.file_size as usize;
        let memory_size = segment.memory_size as usize;

        // SAFETY: Validation checked the source range and the caller promises
        // that the full destination range is writable RAM.
        unsafe {
            if file_size != 0 {
                ptr::copy(source, destination, file_size);
            }
            if memory_size > file_size {
                ptr::write_bytes(destination.add(file_size), 0, memory_size - file_size);
            }
            ogc_cache::dc_flush_range(destination.cast::<c_void>(), segment.memory_size);
            if segment.executable {
                ogc_cache::ic_invalidate_range(destination.cast::<c_void>(), segment.memory_size);
            }
        }
    }

    if let Some(bss) = &plan.bss {
        let size = bss.end - bss.start;
        if size != 0 {
            let destination = bss.start as usize as *mut u8;
            // SAFETY: The caller promises writable memory for the validated
            // BSS range.
            unsafe {
                ptr::write_bytes(destination, 0, size as usize);
                ogc_cache::dc_flush_range(destination.cast::<c_void>(), size);
            }
        }
    }

    Ok(())
}

/// Transfer control to a validated executable entry point.
///
/// This is a one-way branch. It does not restore the current runtime or set a
/// new stack pointer.
///
/// # Safety
///
/// `entry` must point to initialized executable Wii RAM. The caller must stop
/// interrupts, threads, input devices, and other services before this call.
/// The active stack must be usable by the target program, and the current
/// program must no longer need to return.
pub unsafe fn transfer_control(entry: u32) -> ! {
    // SAFETY: The caller guarantees that this is an executable entry point
    // and accepts the one-way transfer of control.
    unsafe {
        core::arch::asm!(
            "mtctr {entry}",
            "bctr",
            entry = in(reg) entry as usize,
            options(noreturn)
        )
    }
}

fn address_ranges_alias(source: Range<usize>, destination: Range<usize>) -> bool {
    if source.start >= source.end || destination.start >= destination.end {
        return false;
    }
    let source = physical_range(source);
    let destination = physical_range(destination);
    source.start < destination.end && destination.start < source.end
}

fn physical_range(range: Range<usize>) -> Range<usize> {
    // Wii cached and uncached aliases share the lower 30-bit physical address.
    let start = range.start & 0x3fff_ffff;
    let end = start.saturating_add(range.end - range.start);
    start..end
}
