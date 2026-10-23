#![cfg_attr(target_arch = "powerpc", no_std)]
#![cfg_attr(target_arch = "powerpc", no_main)]

#[cfg(not(target_arch = "powerpc"))]
fn main() {}

// The host test must set this before main. An ordinary launch cannot write NAND.
#[cfg(target_arch = "powerpc")]
#[unsafe(no_mangle)]
pub static HBC_ACCEPTANCE_ALLOW: core::sync::atomic::AtomicU32 =
    core::sync::atomic::AtomicU32::new(0);

#[cfg(target_arch = "powerpc")]
#[unsafe(no_mangle)]
pub static HBC_ACCEPTANCE_RESULT: core::sync::atomic::AtomicI32 =
    core::sync::atomic::AtomicI32::new(i32::MIN);

/// # Safety
/// The runtime must supply a valid argc/argv vector for this isolated test.
#[cfg(target_arch = "powerpc")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn main(argc: i32, argv: *mut *mut u8) -> i32 {
    use core::sync::atomic::Ordering;
    if HBC_ACCEPTANCE_ALLOW.load(Ordering::Acquire) != 0x454d554c {
        return -1;
    }
    ogc_system::init();
    if unsafe { hbc_rs::platform::allocator::initialize_wii(16 * 1024 * 1024, 1024 * 1024) }
        .is_err()
    {
        HBC_ACCEPTANCE_RESULT.store(-12, Ordering::Release);
        return -12;
    }
    let result = unsafe { hbc_rs::acceptance::run(argc, argv) };
    hbc_acceptance_complete(result)
}

#[cfg(target_arch = "powerpc")]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    ogc_system::report(format_args!("FAIL acceptance panic: {info}\n"));
    HBC_ACCEPTANCE_RESULT.store(-99, core::sync::atomic::Ordering::Release);
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(target_arch = "powerpc")]
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn hbc_acceptance_complete(result: i32) -> ! {
    HBC_ACCEPTANCE_RESULT.store(result, core::sync::atomic::Ordering::Release);
    loop {
        core::hint::spin_loop();
    }
}
