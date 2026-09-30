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
    hbc_rs::platform::lifecycle::register_callbacks();
    let heap_result = unsafe {
        hbc_rs::platform::allocator::initialize_wii(
            hbc_rs::platform::allocator::DEFAULT_MAX_MEM2_HEAP_BYTES,
            hbc_rs::platform::allocator::DEFAULT_MEM2_RESERVE_BYTES,
        )
    };
    if heap_result.is_err() {
        return -1;
    }

    let _ = ogc_conf::init();
    let view_width = if ogc_conf::get_aspect_ratio() == ogc_conf::CONF_ASPECT_16_9 as i32 {
        hbc_rs::render::WIDE_LOGICAL_WIDTH
    } else {
        hbc_rs::render::LOGICAL_WIDTH
    };
    let video =
        match hbc_rs::platform::video::initialize(view_width > hbc_rs::render::LOGICAL_WIDTH) {
            Ok(video) => video,
            Err(_) => return -2,
        };
    video.configure_2d();

    let mut startup = hbc_rs::platform::startup::WiiStartup::initialize(view_width, 480);
    if hbc_rs::platform::wiiload::start().is_err() {
        startup
            .runtime_mut()
            .show_notice("Could not start the network receiver.");
    }
    let mut display_ready = false;
    let mut frames_since_storage_scan = 0u8;

    loop {
        if let Some(request) = hbc_rs::platform::lifecycle::take_request() {
            let reset = match request {
                hbc_rs::platform::lifecycle::ExitRequest::ReturnToMenu => {
                    ogc_system::SYS_RETURNTOMENU
                }
                hbc_rs::platform::lifecycle::ExitRequest::PowerOff => ogc_system::SYS_POWEROFF,
            };
            exit_to_system(&mut startup, reset);
        }

        startup.poll_storage_scan();
        if !startup.storage_scan_pending() {
            if let Some(payload) = hbc_rs::platform::wiiload::take_payload() {
                if payload.data.starts_with(b"PK\x03\x04") {
                    startup
                        .runtime_mut()
                        .show_notice("Received ZIP installation is not available yet.");
                } else {
                    let plan = if payload.data.starts_with(b"\x7fELF") {
                        hbc_rs::loader::parse_elf32_ppc(
                            &payload.data,
                            Some(hbc_rs::platform::startup::EXECUTABLE_MEMORY.clone()),
                        )
                    } else {
                        hbc_rs::loader::parse_dol(
                            &payload.data,
                            Some(hbc_rs::platform::startup::EXECUTABLE_MEMORY.clone()),
                        )
                    };
                    match plan {
                        Ok(plan) => {
                            if let Err(notice) = transfer_executable(
                                &mut startup,
                                &payload.data,
                                &plan,
                                &payload.args,
                                "Could not prepare the received application.",
                            ) {
                                startup.runtime_mut().show_notice(notice);
                            }
                        }
                        Err(_) => startup
                            .runtime_mut()
                            .show_notice("Received data is not a valid Wii application."),
                    }
                }
            }
        }
        frames_since_storage_scan += 1;
        if frames_since_storage_scan == 30 {
            frames_since_storage_scan = 0;
            if startup.refresh_storage().is_err() {
                startup
                    .runtime_mut()
                    .show_notice("Could not refresh storage devices.");
            }
        }
        let mut frame = hbc_rs::platform::input::scan();
        let storage_scan_pending = startup.storage_scan_pending();
        if storage_scan_pending {
            frame.pressed.clear();
            frame.held.clear();
            frame.released.clear();
            frame.pointer = None;
            frame.pointer_pressed = false;
            frame.pointer_held = false;
            frame.stick_scroll_pixels = 0;
        }
        let pointer = frame.pointer;
        let pointer_pressed = frame.pointer_pressed;
        let pointer_held = frame.pointer_held;
        if pointer_pressed {
            // HBC applies Confirm from the pointer owner or GameCube pad to
            // the widget under the pointer. Delay it until after hit-testing.
            frame
                .pressed
                .retain(|input| *input != hbc_rs::app::Input::Confirm);
        }
        let mut actions = startup.runtime_mut().update(&frame);
        let hit_frame = hbc_rs::render::browser_frame_with_viewport(
            startup.runtime().controller(),
            startup.runtime().catalog(),
            startup.runtime().theme(),
            startup.logical_width(),
        );
        let pointer_target = pointer
            .and_then(|position| hit_frame.hit_test(f32::from(position.x), f32::from(position.y)));
        actions.extend(startup.runtime_mut().update_pointer_frame(
            pointer_target,
            pointer.map(|pointer| pointer.y),
            pointer_pressed,
            pointer_held,
        ));
        startup.refresh_visible_icons();
        let mut scene = hbc_rs::render::browser_frame_with_viewport(
            startup.runtime().controller(),
            startup.runtime().catalog(),
            startup.runtime().theme(),
            startup.logical_width(),
        );
        if let Some(pointer) = pointer {
            scene.pointer_cursor(f32::from(pointer.x), f32::from(pointer.y));
        }
        if storage_scan_pending {
            scene.storage_loading();
        }
        if unsafe {
            hbc_rs::platform::renderer::draw_frame(
                &video,
                &scene,
                startup.icon_textures(),
                startup.theme_textures(),
            )
        }
        .is_err()
        {
            return -3;
        }
        unsafe { video.present() };
        if !display_ready {
            video.show();
            display_ready = true;
        }
        video.wait_vsync();

        for action in actions {
            if action == hbc_rs::app::Action::ApplyOptions {
                let (persist, mut notice) = match startup.apply_options() {
                    Ok(true) => (true, None),
                    Ok(false) => (true, Some("The selected storage device is not available.")),
                    Err(hbc_rs::platform::startup::StartupError::CatalogScan) => {
                        (true, Some("Could not read the apps folder on this device."))
                    }
                    Err(_) => (false, Some("Could not change the storage device.")),
                };
                if persist
                    && startup
                        .runtime_mut()
                        .save_settings_if_dirty(hbc_rs::platform::settings::save)
                        .is_err()
                {
                    notice = Some("Could not save the new settings to NAND.");
                }
                if let Some(notice) = notice {
                    startup.runtime_mut().show_notice(notice);
                }
                continue;
            }
            if action == hbc_rs::app::Action::ApplyTheme {
                match startup.prepare_selected_theme() {
                    Ok(theme) => startup.apply_theme(theme),
                    Err(hbc_rs::platform::startup::StartupError::ThemeStorage(_)) => startup
                        .runtime_mut()
                        .show_notice("Could not save this theme to NAND."),
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
                if let Err(notice) = transfer_executable(
                    &mut startup,
                    &image.bytes,
                    &image.plan,
                    &image.arguments,
                    "Could not prepare this application to run.",
                ) {
                    startup.runtime_mut().show_notice(notice);
                    continue;
                }
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
                if action == hbc_rs::app::Action::LaunchBootMii {
                    let _ = startup
                        .runtime_mut()
                        .save_settings_if_dirty(hbc_rs::platform::settings::save);
                    if startup.unmount_storage().is_err() {
                        startup
                            .runtime_mut()
                            .show_notice("Could not stop storage. BootMii launch stopped.");
                        continue;
                    }
                    hbc_rs::platform::wiiload::stop();
                    let _ = wpad_sys::shutdown();
                    let _ = ogc_ios::launch_new_ios(254);
                }
                exit_to_system(&mut startup, reset);
            }
        }
    }
}

#[cfg(target_arch = "powerpc")]
fn transfer_executable(
    startup: &mut hbc_rs::platform::startup::WiiStartup,
    bytes: &[u8],
    plan: &hbc_rs::loader::LoadPlan,
    arguments: &[u8],
    prepare_error: &'static str,
) -> Result<(), &'static str> {
    if unsafe {
        hbc_rs::loader::prepare_launch(
            bytes,
            plan,
            arguments,
            hbc_rs::platform::startup::EXECUTABLE_MEMORY.clone(),
        )
    }
    .is_err()
    {
        return Err(prepare_error);
    }
    if startup
        .runtime_mut()
        .save_settings_if_dirty(hbc_rs::platform::settings::save)
        .is_err()
    {
        return Err("Could not save settings. Application launch stopped.");
    }
    if startup.unmount_storage().is_err() {
        return Err("Could not stop storage. Application launch stopped.");
    }
    hbc_rs::platform::wiiload::stop();
    let _ = wpad_sys::shutdown();
    hbc_rs::platform::video::shutdown();
    unsafe { hbc_rs::loader::transfer_control(plan.entry) }
}

#[cfg(target_arch = "powerpc")]
fn exit_to_system(startup: &mut hbc_rs::platform::startup::WiiStartup, reset: u32) -> ! {
    let _ = startup
        .runtime_mut()
        .save_settings_if_dirty(hbc_rs::platform::settings::save);
    let _ = startup.unmount_storage();
    hbc_rs::platform::wiiload::stop();
    let _ = wpad_sys::shutdown();
    hbc_rs::platform::video::shutdown();
    unsafe { ogc_system::reset_system(reset as i32, 0, 0) };
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(target_arch = "powerpc")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
