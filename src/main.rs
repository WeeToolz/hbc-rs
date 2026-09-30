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
    let heap_result = unsafe {
        hbc_rs::platform::allocator::initialize_wii(
            hbc_rs::platform::allocator::DEFAULT_MAX_MEM2_HEAP_BYTES,
            hbc_rs::platform::allocator::DEFAULT_MEM2_RESERVE_BYTES,
        )
    };
    if heap_result.is_err() {
        return -1;
    }

    let video = match hbc_rs::platform::video::initialize() {
        Ok(video) => video,
        Err(_) => return -2,
    };
    video.configure_2d();

    // HBC's default logical UI space is 640 by 480. Widescreen mode and its
    // aspect setting will select an 852 by 480 space when that policy lands.
    let mut startup = hbc_rs::platform::startup::WiiStartup::initialize(640, 480);
    let mut display_ready = false;
    let mut frames_since_storage_scan = 0u8;

    loop {
        frames_since_storage_scan += 1;
        if frames_since_storage_scan == 30 {
            frames_since_storage_scan = 0;
            if startup.refresh_storage().is_err() {
                startup
                    .runtime_mut()
                    .show_notice("Could not refresh storage devices.");
            }
        }
        let frame = hbc_rs::platform::input::scan();
        let actions = startup.runtime_mut().update(&frame);
        let scene = hbc_rs::render::browser_frame_with_theme(
            startup.runtime().controller(),
            startup.runtime().catalog(),
            startup.runtime().theme(),
        );
        if unsafe { hbc_rs::platform::renderer::draw_frame(&video, &scene) }.is_err() {
            return -3;
        }
        unsafe { video.present() };
        if !display_ready {
            video.show();
            display_ready = true;
        }
        video.wait_vsync();

        for action in actions {
            if action == hbc_rs::app::Action::ApplyTheme {
                match startup.prepare_selected_theme() {
                    Ok(theme) => startup.runtime_mut().apply_theme(theme),
                    Err(_) => startup
                        .runtime_mut()
                        .show_notice("Could not read or validate this theme."),
                }
                continue;
            }
            if action == hbc_rs::app::Action::LaunchSelected {
                let Ok(image) = startup.prepare_selected_executable() else {
                    startup
                        .runtime_mut()
                        .show_notice("Could not read or validate this application.");
                    continue;
                };
                if unsafe {
                    hbc_rs::loader::prepare_launch(
                        &image.bytes,
                        &image.plan,
                        &image.arguments,
                        hbc_rs::platform::startup::EXECUTABLE_MEMORY,
                    )
                }
                .is_err()
                {
                    startup
                        .runtime_mut()
                        .show_notice("Could not prepare this application to run.");
                    continue;
                }
                let settings = startup.runtime().settings().current();
                if hbc_rs::platform::settings::save(settings).is_err() {
                    startup
                        .runtime_mut()
                        .show_notice("Could not save settings. Application launch stopped.");
                    continue;
                }
                if startup.unmount_storage().is_err() {
                    startup
                        .runtime_mut()
                        .show_notice("Could not stop storage. Application launch stopped.");
                    continue;
                }
                let _ = wpad_sys::shutdown();
                hbc_rs::platform::video::shutdown();
                unsafe { hbc_rs::loader::transfer_control(image.plan.entry) };
            }
            if action == hbc_rs::app::Action::DeleteSelected {
                if startup.delete_selected_app().is_err() {
                    startup
                        .runtime_mut()
                        .show_notice("Could not delete this application.");
                }
                continue;
            }
            let reset = match action {
                hbc_rs::app::Action::LaunchBootMii => Some(ogc_system::SYS_RETURNTOMENU),
                hbc_rs::app::Action::Exit => Some(ogc_system::SYS_RETURNTOMENU),
                hbc_rs::app::Action::Shutdown => Some(ogc_system::SYS_POWEROFF),
                _ => None,
            };
            if let Some(reset) = reset {
                let settings = startup.runtime().settings().current();
                let _ = hbc_rs::platform::settings::save(settings);
                if action == hbc_rs::app::Action::LaunchBootMii {
                    if startup.unmount_storage().is_err() {
                        startup
                            .runtime_mut()
                            .show_notice("Could not stop storage. BootMii launch stopped.");
                        continue;
                    }
                    let _ = wpad_sys::shutdown();
                    let _ = ogc_ios::launch_new_ios(254);
                }
                hbc_rs::platform::video::shutdown();
                unsafe { ogc_system::reset_system(reset as i32, 0, 0) };
                loop {
                    core::hint::spin_loop();
                }
            }
        }
    }
}

#[cfg(target_arch = "powerpc")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
