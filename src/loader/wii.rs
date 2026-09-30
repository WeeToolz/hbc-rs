//! Wii-only code that writes and starts a validated executable image.

use super::{validate_load_plan, LoadPlan, ParseError, MAX_LAUNCH_ARGUMENT_BYTES};
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

/// Apply a load plan, patch HBC-compatible crt0 state, and write `__argv` data.
///
/// # Safety
/// The caller must stop all code that can access `allowed_memory`. The image
/// and argument buffer must stay readable until this function returns.
pub unsafe fn prepare_launch(
    bytes: &[u8],
    plan: &LoadPlan,
    arguments: &[u8],
    allowed_memory: Range<u32>,
) -> Result<(), ParseError> {
    if arguments.len() > MAX_LAUNCH_ARGUMENT_BYTES {
        return Err(ParseError::InvalidFormat("launch arguments exceed limit"));
    }
    if arguments.last() != Some(&0) {
        return Err(ParseError::InvalidFormat(
            "invalid launch argument terminator",
        ));
    }
    unsafe { apply_load_plan(bytes, plan, allowed_memory.clone())? };

    let entry = plan.entry;
    let entry_record_end = entry.checked_add(20).ok_or(ParseError::AddressOverflow)?;
    let entry_record_is_loaded = plan.segments.iter().any(|segment| {
        segment.executable
            && entry >= segment.address
            && entry_record_end <= segment.address.saturating_add(segment.file_size)
    });

    let entry_ptr = entry as usize as *mut u8;
    // HBC patches one known crt0 branch byte when this exact instruction is
    // present. Do not change arbitrary entry code.
    let crt0_offset = entry.checked_add(0x20).ok_or(ParseError::AddressOverflow)?;
    let crt0_byte_is_loaded = plan.segments.iter().any(|segment| {
        segment.executable
            && crt0_offset >= segment.address
            && crt0_offset < segment.address.saturating_add(segment.file_size)
    });
    if crt0_byte_is_loaded && crt0_offset < allowed_memory.end {
        let crt0 = crt0_offset as usize as *mut u8;
        // SAFETY: The address is in a validated executable file-backed range.
        if unsafe { ptr::read_volatile(crt0) } == 0x41 {
            unsafe { ptr::write_volatile(crt0, 0x40) };
            unsafe { ogc_cache::dc_flush_range(crt0.cast::<c_void>(), 1) };
        }
    }

    if !entry_record_is_loaded || entry & 3 != 0 {
        return Ok(());
    }

    // The Wii is big-endian, so volatile native u32 loads match the ABI words.
    // SAFETY: `argv_record_is_loaded` proves the first 20 entry bytes came from
    // an executable segment, and the plan has already been applied.
    let argv_magic = unsafe { ptr::read_volatile(entry_ptr.add(4).cast::<u32>()) };
    if argv_magic != ARGV_MAGIC {
        return Ok(());
    }

    const LAUNCH_ARGUMENT_ADDRESS: u32 = 0x8132_fc00;
    if LAUNCH_ARGUMENT_ADDRESS < allowed_memory.end
        || arguments.len() > 0x8133_0000usize.saturating_sub(LAUNCH_ARGUMENT_ADDRESS as usize)
    {
        return Err(ParseError::InvalidFormat(
            "launch arguments overlap the executable range",
        ));
    }
    let argument_ptr = LAUNCH_ARGUMENT_ADDRESS as usize as *mut u8;
    // SAFETY: The caller reserves this HBC argument area outside the loader's
    // executable range, and the size check keeps the copy inside 1024 bytes.
    unsafe {
        ptr::copy_nonoverlapping(arguments.as_ptr(), argument_ptr, arguments.len());
        ptr::write_volatile(entry_ptr.add(8).cast::<u32>(), ARGV_MAGIC);
        ptr::write_volatile(entry_ptr.add(12).cast::<u32>(), LAUNCH_ARGUMENT_ADDRESS);
        ptr::write_volatile(entry_ptr.add(16).cast::<u32>(), arguments.len() as u32);
        ogc_cache::dc_flush_range(entry_ptr.add(8).cast::<c_void>(), 12);
        ogc_cache::dc_flush_range(argument_ptr.cast::<c_void>(), arguments.len() as u32);
    }
    Ok(())
}

/// Stop the libogc system services, clear HBC's runtime region, then branch to
/// a validated executable entry point through an Arena2 trampoline.
///
/// # Safety
///
/// `entry` must point to initialized executable Wii RAM. The caller must stop
/// HBC-owned worker threads and unmount storage first. The current Rust IRQ
/// vector remains installed because libogc-rs has no safe close operation; the
/// executable's crt0 must install its own handlers and set its stack before it
/// enables interrupts. Reload preferred IOS only when the app does not request
/// AHB access, because the required HBC ES patch is not available here.
pub unsafe fn transfer_control(entry: u32, reload_ios: bool) -> ! {
    const EXEC_STUB: [u32; 8] = [
        0x7c69_03a6, // mtctr r3
        0x3d20_8133, // lis r9, 0x8133
        0x3d40_8180, // lis r10, 0x8180
        0x9009_0000, // stw r0, 0(r9)
        0x3929_0004, // addi r9, r9, 4
        0x7c29_5000, // cmpd r9, r10
        0x4180_fff4, // blt 1b
        0x4e80_0420, // bctr
    ];
    let arena2_hi = ogc_system::get_arena2_hi() as usize;
    let stub_len = core::mem::size_of_val(&EXEC_STUB);
    if arena2_hi < 0x9000_0000 + stub_len {
        loop {
            core::hint::spin_loop();
        }
    }
    let stub_address = (arena2_hi - stub_len) & !31;
    let stub = stub_address as *mut u32;

    // Match HBC's SDK compatibility clock values before entering an app.
    unsafe {
        ptr::write_volatile(0x8000_00f8usize as *mut u32, 0x0e7b_e2c0);
        ptr::write_volatile(0x8000_00fcusize as *mut u32, 0x2b73_a840);
        ptr::copy_nonoverlapping(EXEC_STUB.as_ptr(), stub, EXEC_STUB.len());
        ogc_cache::dc_flush_range(stub.cast::<c_void>(), stub_len as u32);
        ogc_cache::ic_invalidate_range(stub.cast::<c_void>(), stub_len as u32);
        ogc_system::reset_system(ogc_system::SYS_SHUTDOWN as i32, 0, 0);
        if reload_ios {
            let _ = ogc_ios::launch_new_ios(ogc_ios::get_preferred_version());
        }
        ogc_irq::disable(u32::MAX);
        core::arch::asm!(
            "mfmsr 3",
            "lis 4, 0xffff",
            "ori 4, 4, 0x7fff",
            "and 3, 3, 4",
            "mtmsr 3",
            "isync",
            out("r3") _,
            out("r4") _,
            options(nostack)
        );
    }

    // SAFETY: The Arena2 code is initialized and instruction-cache coherent.
    unsafe {
        let call: unsafe extern "C" fn(u32) -> ! = core::mem::transmute(stub);
        call(entry)
    }
}

const ARGV_MAGIC: u32 = 0x5f61_7267;

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
