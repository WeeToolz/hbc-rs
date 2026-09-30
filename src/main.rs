#![cfg_attr(target_arch = "powerpc", no_std)]
#![cfg_attr(target_arch = "powerpc", no_main)]

#[cfg(not(target_arch = "powerpc"))]
fn main() {
    // Wii startup and service initialization will be added with the first
    // platform port. This binary currently provides the package entry point.
}

#[cfg(target_arch = "powerpc")]
#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *mut *mut u8) -> i32 {
    match unsafe {
        hbc_rs::platform::allocator::initialize_wii(
            hbc_rs::platform::allocator::DEFAULT_MAX_MEM2_HEAP_BYTES,
            hbc_rs::platform::allocator::DEFAULT_MEM2_RESERVE_BYTES,
        )
    } {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

#[cfg(target_arch = "powerpc")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
