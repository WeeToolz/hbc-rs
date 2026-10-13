//! Wii-only code that writes and starts a validated executable image.

use super::{LoadPlan, ParseError, MAX_LAUNCH_ARGUMENT_BYTES, MAX_LOAD_SEGMENTS};
use alloc::vec::Vec;
use core::ffi::c_void;
use core::ops::Range;
use core::ptr;

const MEM2_START: usize = 0x9000_0000;
const MEM2_END: usize = 0x9400_0000;
const LAUNCH_ARGUMENT_ADDRESS: u32 = 0x8132_fc00;

#[repr(C)]
#[derive(Clone, Copy)]
struct LaunchOperation {
    source: u32,
    destination: u32,
    file_size: u32,
    memory_size: u32,
    executable: u32,
}

#[repr(C)]
struct LaunchContext {
    operation_count: u32,
    entry: u32,
    operations: *const LaunchOperation,
    crt0_patch: u32,
    argv_entry: u32,
    argv_source: *const u8,
    argv_len: u32,
}

const _: () = assert!(core::mem::size_of::<LaunchOperation>() == 20);
const _: () = assert!(core::mem::size_of::<LaunchContext>() == 28);
const _: () = assert!(core::mem::offset_of!(LaunchContext, operations) == 8);
const _: () = assert!(core::mem::offset_of!(LaunchContext, crt0_patch) == 12);
const _: () = assert!(core::mem::offset_of!(LaunchContext, argv_entry) == 16);
const _: () = assert!(core::mem::offset_of!(LaunchContext, argv_source) == 20);
const _: () = assert!(core::mem::offset_of!(LaunchContext, argv_len) == 24);

/// All launch data and the final copy routine stay allocated in MEM2 until
/// control leaves HBC. The final routine does not use this Rust object's stack.
pub struct LaunchImage {
    trampoline: Vec<u32>,
    bytes: super::LaunchSource,
    arguments: Vec<u8>,
    operations: Vec<LaunchOperation>,
    context: Vec<LaunchContext>,
}

core::arch::global_asm!(
    r#"
    .section .text.hbc_launch_trampoline,"ax",@progbits
    .balign 32
    .globl __hbc_launch_trampoline_start
__hbc_launch_trampoline_start:
    # r3 points to LaunchContext. This leaf uses no stack and calls no code.
    lwz 4,0(3)
    lwz 5,8(3)
    .Lcopy_operation:
    cmpwi 4,0
    beq .Lapply_patches
    lwz 6,0(5)
    lwz 7,4(5)
    lwz 8,8(5)
    lwz 9,12(5)
    lwz 10,16(5)
    mr 11,7
    .Lcopy_file:
    cmpwi 8,0
    beq .Lzero_tail_setup
    lbz 12,0(6)
    stb 12,0(7)
    addi 6,6,1
    addi 7,7,1
    addi 8,8,-1
    b .Lcopy_file
    .Lzero_tail_setup:
    lwz 0,8(5)
    subf 9,0,9
    li 0,0
    .Lzero_tail:
    cmpwi 9,0
    beq .Lcache_data_setup
    stb 0,0(7)
    addi 7,7,1
    addi 9,9,-1
    b .Lzero_tail
    .Lcache_data_setup:
    lwz 9,12(5)
    cmpwi 9,0
    beq .Lnext_operation
    rlwinm 12,11,0,0,26
    add 8,11,9
    addi 8,8,31
    rlwinm 8,8,0,0,26
    .Lflush_data:
    dcbf 0,12
    addi 12,12,32
    cmplw 12,8
    blt .Lflush_data
    sync
    andi. 10,10,1
    beq .Lnext_operation
    rlwinm 12,11,0,0,26
    add 8,11,9
    addi 8,8,31
    rlwinm 8,8,0,0,26
    .Lflush_instruction:
    icbi 0,12
    addi 12,12,32
    cmplw 12,8
    blt .Lflush_instruction
    sync
    isync
    .Lnext_operation:
    addi 5,5,20
    addi 4,4,-1
    b .Lcopy_operation

    .Lapply_patches:
    lwz 5,12(3)
    cmpwi 5,0
    beq .Largv_patch_check
    li 0,0x40
    stb 0,0(5)
    rlwinm 12,5,0,0,26
    dcbf 0,12
    sync
    icbi 0,12
    sync
    isync

    .Largv_patch_check:
    lwz 5,16(3)
    cmpwi 5,0
    beq .Lclear_runtime
    lwz 6,20(3)
    lwz 8,24(3)
    lis 7,0x8132
    ori 7,7,0xfc00
    .Lcopy_arguments:
    cmpwi 8,0
    beq .Lflush_arguments_setup
    lbz 9,0(6)
    stb 9,0(7)
    addi 6,6,1
    addi 7,7,1
    addi 8,8,-1
    b .Lcopy_arguments
    .Lflush_arguments_setup:
    lis 7,0x8132
    ori 7,7,0xfc00
    lwz 8,24(3)
    add 8,7,8
    addi 8,8,31
    rlwinm 8,8,0,0,26
    rlwinm 12,7,0,0,26
    .Lflush_arguments:
    cmplw 12,8
    bge .Lwrite_argv_record
    dcbf 0,12
    addi 12,12,32
    b .Lflush_arguments
    .Lwrite_argv_record:
    lis 9,0x5f61
    ori 9,9,0x7267
    stw 9,8(5)
    lis 9,0x8132
    ori 9,9,0xfc00
    stw 9,12(5)
    lwz 8,24(3)
    stw 8,16(5)
    addi 12,5,8
    addi 8,5,20
    addi 8,8,31
    rlwinm 8,8,0,0,26
    rlwinm 12,12,0,0,26
    .Lflush_argv_record:
    cmplw 12,8
    bge .Lsync_argv_record
    dcbf 0,12
    icbi 0,12
    addi 12,12,32
    b .Lflush_argv_record
    .Lsync_argv_record:
    sync
    isync

    .Lclear_runtime:
    li 0,0
    lis 12,0x8133
    lis 11,0x8180
    .Lclear_runtime_loop:
    stw 0,0(12)
    addi 12,12,4
    cmplw 12,11
    blt .Lclear_runtime_loop
    lis 12,0x8133
    lis 11,0x8180
    .Lflush_cleared_runtime:
    dcbf 0,12
    addi 12,12,32
    cmplw 12,11
    blt .Lflush_cleared_runtime
    sync
    lwz 12,4(3)
    mr 3,12
    mtctr 12
    bctr
    .globl __hbc_launch_trampoline_end
__hbc_launch_trampoline_end:
    "#
);

unsafe extern "C" {
    static __hbc_launch_trampoline_start: u8;
    static __hbc_launch_trampoline_end: u8;
}

/// Prepare all launch data in MEM2 without writing to application memory.
///
/// The returned object owns every source and descriptor used by the final
/// transfer. Its allocations must stay alive until the Arena2 trampoline
/// branches to the application. The input buffers must come from the active
/// MEM2. Arena2Hi-backed storage and the managed Rust heap are both valid
/// sources; the source range is checked before any service is stopped.
pub fn stage_launch(
    bytes: super::LaunchSource,
    plan: &LoadPlan,
    arguments: Vec<u8>,
    allowed_memory: Range<u32>,
) -> Result<LaunchImage, ParseError> {
    if arguments.len() > MAX_LAUNCH_ARGUMENT_BYTES {
        return Err(ParseError::InvalidFormat("launch arguments exceed limit"));
    }
    if arguments.last() != Some(&0) {
        return Err(ParseError::InvalidFormat(
            "invalid launch argument terminator",
        ));
    }
    if plan.segments.len() > MAX_LOAD_SEGMENTS {
        return Err(ParseError::InvalidFormat("too many executable segments"));
    }
    if allowed_memory.start < 0x8000_3400 || allowed_memory.end > LAUNCH_ARGUMENT_ADDRESS {
        return Err(ParseError::InvalidSegment(
            "launch range is outside HBC's reserved executable area",
        ));
    }
    let bytes_slice = bytes.as_slice();
    super::validate_load_plan(bytes_slice, plan, &allowed_memory)?;
    if !mem2_range(bytes_slice.as_ptr() as usize, bytes_slice.len())
        || !mem2_range(arguments.as_ptr() as usize, arguments.len())
    {
        return Err(ParseError::InvalidFormat(
            "launch source is outside the reserved MEM2 heap",
        ));
    }

    let entry = plan.entry;
    let entry_record_end = entry.checked_add(20).ok_or(ParseError::AddressOverflow)?;
    let entry_record_source = plan.segments.iter().find_map(|segment| {
        (segment.executable
            && entry >= segment.address
            && entry_record_end <= segment.address.saturating_add(segment.file_size))
        .then(|| segment.file_range.start + (entry - segment.address) as usize)
    });
    let entry_record_is_loaded = entry_record_source.is_some();

    // HBC patches one known crt0 branch byte when this exact instruction is
    // present. Do not change arbitrary entry code.
    let crt0_offset = entry.checked_add(0x20).ok_or(ParseError::AddressOverflow)?;
    let crt0_source = plan.segments.iter().find_map(|segment| {
        (segment.executable
            && crt0_offset >= segment.address
            && crt0_offset < segment.address.saturating_add(segment.file_size))
        .then(|| segment.file_range.start + (crt0_offset - segment.address) as usize)
    });
    let patch_crt0 = crt0_source
        .and_then(|offset| bytes_slice.get(offset))
        .is_some_and(|byte| *byte == 0x41);

    let argv_record_is_loaded = entry_record_source.is_some_and(|offset| {
        bytes_slice
            .get(offset + 4..offset + 8)
            .is_some_and(|magic| magic == ARGV_MAGIC.to_be_bytes())
    });
    if entry_record_is_loaded
        && entry & 3 == 0
        && argv_record_is_loaded
        && (LAUNCH_ARGUMENT_ADDRESS < allowed_memory.end
            || arguments.len() > 0x8133_0000usize.saturating_sub(LAUNCH_ARGUMENT_ADDRESS as usize))
    {
        return Err(ParseError::InvalidFormat(
            "launch arguments overlap the executable range",
        ));
    }

    let operation_capacity = plan
        .segments
        .len()
        .checked_add(usize::from(plan.bss.is_some()))
        .ok_or(ParseError::OutOfBounds)?;
    let mut operations = Vec::new();
    operations
        .try_reserve_exact(operation_capacity)
        .map_err(|_| ParseError::OutOfBounds)?;
    for segment in &plan.segments {
        let source = bytes_slice.as_ptr().wrapping_add(segment.file_range.start) as usize;
        let source = u32::try_from(source).map_err(|_| ParseError::OutOfBounds)?;
        operations.push(LaunchOperation {
            source,
            destination: segment.address,
            file_size: segment.file_size,
            memory_size: segment.memory_size,
            executable: u32::from(segment.executable),
        });
    }
    if let Some(bss) = &plan.bss {
        operations.push(LaunchOperation {
            source: 0,
            destination: bss.start,
            file_size: 0,
            memory_size: bss.end - bss.start,
            executable: 0,
        });
    }
    let operations_ptr = operations.as_ptr();
    if !mem2_range(
        operations_ptr as usize,
        core::mem::size_of_val(operations.as_slice()),
    ) {
        return Err(ParseError::InvalidFormat(
            "launch descriptors are outside the reserved MEM2 heap",
        ));
    }

    let mut context = Vec::new();
    context
        .try_reserve_exact(1)
        .map_err(|_| ParseError::OutOfBounds)?;
    context.push(LaunchContext {
        operation_count: u32::try_from(operations.len()).map_err(|_| ParseError::OutOfBounds)?,
        entry,
        operations: operations_ptr,
        crt0_patch: if patch_crt0 { crt0_offset } else { 0 },
        argv_entry: if argv_record_is_loaded && entry & 3 == 0 {
            entry
        } else {
            0
        },
        argv_source: arguments.as_ptr(),
        argv_len: u32::try_from(arguments.len()).map_err(|_| ParseError::OutOfBounds)?,
    });
    if !mem2_range(
        context.as_ptr() as usize,
        core::mem::size_of::<LaunchContext>(),
    ) {
        return Err(ParseError::InvalidFormat(
            "launch context is outside the reserved MEM2 heap",
        ));
    }

    let start = core::ptr::addr_of!(__hbc_launch_trampoline_start) as usize;
    let end = core::ptr::addr_of!(__hbc_launch_trampoline_end) as usize;
    let code_len = end.checked_sub(start).ok_or(ParseError::OutOfBounds)?;
    if code_len == 0 || code_len % core::mem::size_of::<u32>() != 0 {
        return Err(ParseError::InvalidFormat("invalid launch trampoline image"));
    }
    let code_words = code_len / core::mem::size_of::<u32>();
    let mut trampoline = Vec::new();
    trampoline
        .try_reserve_exact(code_words)
        .map_err(|_| ParseError::OutOfBounds)?;
    trampoline.resize(code_words, 0);
    // SAFETY: The assembly labels delimit the immutable, linked trampoline
    // image. The destination has exactly enough reserved aligned storage.
    unsafe { ptr::copy_nonoverlapping(start as *const u32, trampoline.as_mut_ptr(), code_words) };
    if !mem2_range(
        trampoline.as_ptr() as usize,
        core::mem::size_of_val(trampoline.as_slice()),
    ) {
        return Err(ParseError::InvalidFormat(
            "launch trampoline is outside the reserved MEM2 heap",
        ));
    }

    Ok(LaunchImage {
        trampoline,
        bytes,
        arguments,
        operations,
        context,
    })
}

/// Finish the IOS transition, execute the copied MEM2 trampoline, and branch
/// to the staged executable. The trampoline copies and initializes all target
/// memory only after this Rust function has left the HBC runtime region.
///
/// # Safety
///
/// `image` must come from `stage_launch`. The caller must stop HBC-owned worker
/// threads and unmount storage first.
/// The prior external IRQ vector is restored before the executable starts.
/// The executable's crt0 must install any required handlers and set its stack
/// before it enables interrupts. The selected policy can restore AHB access
/// after reload.
pub unsafe fn transfer_control(image: LaunchImage, policy: super::IosLaunchPolicy) -> ! {
    let trampoline = image.trampoline.as_ptr();
    let trampoline_len = core::mem::size_of_val(image.trampoline.as_slice());
    let context = image.context.as_ptr();

    // Match C HBC: patch current IOS before reloading it, then stop libogc
    // after the reload and before the final branch.
    unsafe {
        ptr::write_volatile(0x8000_00f8usize as *mut u32, 0x0e7b_e2c0);
        ptr::write_volatile(0x8000_00fcusize as *mut u32, 0x2b73_a840);
        ogc_cache::dc_flush_range(
            image.bytes.as_slice().as_ptr().cast::<c_void>(),
            image.bytes.as_slice().len() as u32,
        );
        if !image.arguments.is_empty() {
            ogc_cache::dc_flush_range(
                image.arguments.as_ptr().cast::<c_void>(),
                image.arguments.len() as u32,
            );
        }
        ogc_cache::dc_flush_range(
            image.operations.as_ptr().cast::<c_void>(),
            core::mem::size_of_val(image.operations.as_slice()) as u32,
        );
        ogc_cache::dc_flush_range(
            context.cast::<c_void>(),
            core::mem::size_of::<LaunchContext>() as u32,
        );
        ogc_cache::dc_flush_range(trampoline.cast::<c_void>(), trampoline_len as u32);
        ogc_cache::ic_invalidate_range(trampoline.cast::<c_void>(), trampoline_len as u32);
        if policy.ahb_access && ogc_es_sys::patch_ahbprot_reset() != 0 {
            // HBC services are already stopped. Do not start the app without
            // its requested access; return to the System Menu instead.
            ogc_system::reset_system(ogc_system::SYS_RETURNTOMENU as i32, 0, 0);
            loop {
                core::hint::spin_loop();
            }
        }
        if policy.reload {
            let reload_result = ogc_ios::launch_new_ios(ogc_ios::get_preferred_version());
            if reload_result < 0 {
                // The HBC runtime is already stopped. Return to System Menu
                // while the prior IOS is still available instead of starting
                // the app after a failed IOS transition.
                ogc_system::reset_system(ogc_system::SYS_RETURNTOMENU as i32, 0, 0);
                loop {
                    core::hint::spin_loop();
                }
            }
            if policy.ahb_access {
                core::ptr::write_volatile(
                    0x0D80_0180usize as *mut u32,
                    core::ptr::read_volatile(0x0D80_0180usize as *const u32) & !(1 << 21),
                );
            }
        }
        ogc_system::reset_system(ogc_system::SYS_SHUTDOWN as i32, 0, 0);
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
        ogc_irq::restore_runtime_exception_vectors();
    }

    // SAFETY: The copied trampoline is a leaf routine in the reserved MEM2
    // heap. It uses no stack, Rust data, library calls, or return path after it
    // starts to overwrite application memory. `image` remains alive because
    // this call never returns.
    unsafe {
        let call: unsafe extern "C" fn(*const LaunchContext) -> ! =
            core::mem::transmute(trampoline);
        call(context)
    }
}

fn mem2_range(start: usize, len: usize) -> bool {
    start >= MEM2_START && start.checked_add(len).is_some_and(|end| end <= MEM2_END)
}

const ARGV_MAGIC: u32 = 0x5f61_7267;
