#![cfg_attr(target_arch = "powerpc", no_std)]
#![cfg_attr(target_arch = "powerpc", no_main)]

extern crate alloc;

#[cfg(target_arch = "powerpc")]
use alloc::{format, string::String, vec::Vec};

#[cfg(not(target_arch = "powerpc"))]
fn main() {
    // Wii startup and service initialization will be added with the first
    // platform port. This binary currently provides the package entry point.
}

#[cfg(target_arch = "powerpc")]
#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *mut *mut u8) -> i32 {
    // Seed the libogc-rs arena bounds and initialize runtime services before
    // reserving the Rust heap from MEM2 or registering hardware callbacks.
    ogc_system::init();
    #[cfg(feature = "startup-diagnostics")]
    if hbc_rs::platform::diagnostics::initialize(_argc, _argv).is_err() {
        if hbc_rs::platform::diagnostics::fail("CPU VIDEO INIT ERROR") {
            reset_after_diagnostic_report();
        }
        return -4;
    }
    hbc_rs::platform::lifecycle::register_callbacks();
    #[cfg(feature = "startup-diagnostics")]
    hbc_rs::platform::diagnostics::mark("HEAP START");
    let heap_result = unsafe {
        hbc_rs::platform::allocator::initialize_wii(
            hbc_rs::platform::allocator::DEFAULT_MAX_MEM2_HEAP_BYTES,
            hbc_rs::platform::allocator::DEFAULT_MEM2_RESERVE_BYTES,
        )
    };
    if heap_result.is_err() {
        #[cfg(feature = "startup-diagnostics")]
        if hbc_rs::platform::diagnostics::fail("HEAP ERROR") {
            reset_after_diagnostic_report();
        }
        return -1;
    }

    #[cfg(feature = "startup-diagnostics")]
    hbc_rs::platform::diagnostics::mark("CONF START");
    let _ = ogc_conf::init();
    let view_width = if ogc_conf::get_aspect_ratio() == ogc_conf::CONF_ASPECT_16_9 as i32 {
        hbc_rs::render::WIDE_LOGICAL_WIDTH
    } else {
        hbc_rs::render::LOGICAL_WIDTH
    };
    let video =
        match hbc_rs::platform::video::initialize(view_width > hbc_rs::render::LOGICAL_WIDTH) {
            Ok(video) => video,
            Err(_) => {
                #[cfg(feature = "startup-diagnostics")]
                if hbc_rs::platform::diagnostics::fail("VIDEO OR GX FIFO ERROR") {
                    reset_after_diagnostic_report();
                }
                return -2;
            }
        };
    #[cfg(feature = "startup-diagnostics")]
    hbc_rs::platform::diagnostics::mark("GX 2D SETUP");
    video.configure_2d();

    #[cfg(feature = "startup-diagnostics")]
    hbc_rs::platform::diagnostics::begin_reporting();

    let mut startup = hbc_rs::platform::startup::WiiStartup::initialize(view_width, 480);
    let now = ogc_system::time();
    let now_minutes = now / 60;
    let minute_cycle = ((now_minutes % 1440 + 1200) % 1440) as u16;
    let mut bubble_clock_frames = 0u16;
    let mut bubble_minute_cycle = minute_cycle;
    let mut background_animation =
        hbc_rs::render::BackgroundAnimation::new(view_width, now as u32, minute_cycle);
    #[cfg(feature = "startup-diagnostics")]
    hbc_rs::platform::diagnostics::mark("NETWORK WORKER START");
    if hbc_rs::platform::wiiload::start().is_err() {
        startup.runtime_mut().show_notice_with_kind(
            "Could not start the network receiver.",
            hbc_rs::app::NoticeKind::Error,
        );
    }
    let mut display_ready = false;
    let mut frames_since_storage_scan = 0u8;
    let mut pending_zip: Option<Vec<u8>> = None;
    let mut deferred_exit: Option<hbc_rs::platform::lifecycle::ExitRequest> = None;

    loop {
        if let Some(request) = hbc_rs::platform::lifecycle::take_request() {
            if startup.received_zip_install_pending() {
                deferred_exit = Some(request);
            } else {
                let reset = match request {
                    hbc_rs::platform::lifecycle::ExitRequest::ReturnToMenu => {
                        ogc_system::SYS_RETURNTOMENU
                    }
                    hbc_rs::platform::lifecycle::ExitRequest::PowerOff => ogc_system::SYS_POWEROFF,
                };
                exit_to_system(&mut startup, reset);
            }
        }

        startup.poll_storage_scan();
        if startup.poll_device_availability().is_err() {
            startup.runtime_mut().show_notice_with_kind(
                "Could not refresh storage devices.",
                hbc_rs::app::NoticeKind::Error,
            );
        }
        let receiver_status = hbc_rs::platform::wiiload::status();
        if receiver_status == hbc_rs::platform::wiiload::ReceiverStatus::Receiving {
            if startup.runtime().controller().screen != hbc_rs::app::Screen::WiiloadReceiving {
                startup.runtime_mut().show_wiiload_receiving();
            }
            let (completed, total) = hbc_rs::platform::wiiload::receive_progress();
            startup
                .runtime_mut()
                .set_wiiload_install_progress(hbc_rs::wiiload::progress_percent(completed, total));
        } else if startup.runtime().controller().screen == hbc_rs::app::Screen::WiiloadReceiving {
            startup.runtime_mut().finish_wiiload_receiving();
        }
        if let Some(completion) = startup.poll_received_zip_install() {
            let (message, kind) = match (completion.operation, completion.worker_join) {
                (Ok(()), Ok(())) => (
                    String::from("Received ZIP is complete."),
                    hbc_rs::app::NoticeKind::Information,
                ),
                (Err(operation), Ok(())) => (operation.notice(), hbc_rs::app::NoticeKind::Error),
                (Ok(()), Err(join)) => (join.notice(), hbc_rs::app::NoticeKind::Error),
                (Err(operation), Err(join)) => (
                    format!("{} {}", operation.notice(), join.notice()),
                    hbc_rs::app::NoticeKind::Error,
                ),
            };
            startup.runtime_mut().show_notice_with_kind(&message, kind);
        }
        if let Some(progress) = startup.received_zip_install_progress() {
            startup.runtime_mut().set_wiiload_install_progress(progress);
        }
        if !startup.received_zip_install_pending() {
            if let Some(request) = deferred_exit.take() {
                let reset = match request {
                    hbc_rs::platform::lifecycle::ExitRequest::ReturnToMenu => {
                        ogc_system::SYS_RETURNTOMENU
                    }
                    hbc_rs::platform::lifecycle::ExitRequest::PowerOff => ogc_system::SYS_POWEROFF,
                };
                exit_to_system(&mut startup, reset);
            }
        }
        if !startup.storage_scan_pending() {
            if pending_zip.is_none() && !startup.received_zip_install_pending() {
                if let Some(payload) = hbc_rs::platform::wiiload::take_payload() {
                    if payload.data.starts_with(b"PK\x03\x04") {
                        match hbc_rs::wiiload_zip::classify_received_zip(&payload.data) {
                            Ok(hbc_rs::wiiload_zip::WiiloadZipClassification::App {
                                directory,
                                extraction,
                                ..
                            }) => match startup.received_zip_destination_exists(&directory) {
                                Ok(overwrite) => {
                                    let language = hbc_rs::localization::Language::from_console(
                                        ogc_conf::get_language(),
                                        startup.runtime().theme(),
                                    );
                                    let confirmation = language.received_zip_confirmation(
                                        &directory,
                                        extraction.total_bytes(),
                                        overwrite,
                                    );
                                    pending_zip = Some(payload.data);
                                    startup.runtime_mut().request_wiiload_zip_confirmation();
                                    startup
                                        .runtime_mut()
                                        .set_wiiload_confirmation_text(confirmation);
                                }
                                Err(_) => startup
                                    .runtime_mut()
                                    .show_notice("Could not check the target storage folder."),
                            },
                            Ok(hbc_rs::wiiload_zip::WiiloadZipClassification::Theme { .. }) => {
                                pending_zip = Some(payload.data);
                                startup.runtime_mut().request_wiiload_zip_confirmation();
                            }
                            Err(_) => startup
                                .runtime_mut()
                                .show_notice("Received ZIP is not a valid app or theme."),
                        }
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
                                    false,
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
        }
        frames_since_storage_scan += 1;
        if frames_since_storage_scan == 30 {
            frames_since_storage_scan = 0;
            if startup.runtime().controller().screen == hbc_rs::app::Screen::Options {
                if startup.refresh_device_availability().is_err() {
                    startup.runtime_mut().show_notice_with_kind(
                        "Could not refresh storage devices.",
                        hbc_rs::app::NoticeKind::Error,
                    );
                }
            }
            if startup.refresh_storage().is_err() {
                startup.runtime_mut().show_notice_with_kind(
                    "Could not refresh storage devices.",
                    hbc_rs::app::NoticeKind::Error,
                );
            }
        }
        #[cfg(feature = "startup-diagnostics")]
        if !display_ready {
            hbc_rs::platform::diagnostics::mark("FIRST INPUT SCAN");
        }
        let mut frame = hbc_rs::platform::input::scan();
        let storage_scan_pending = startup.storage_scan_pending();
        if storage_scan_pending || startup.device_probe_pending() {
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
        let language = hbc_rs::localization::Language::from_console(
            ogc_conf::get_language(),
            startup.runtime().theme(),
        );
        let hit_frame = hbc_rs::render::browser_frame_with_language(
            startup.runtime().controller(),
            startup.runtime().catalog(),
            startup.runtime().theme(),
            startup.logical_width(),
            language,
        );
        let pointer_target = pointer
            .and_then(|position| hit_frame.hit_test(f32::from(position.x), f32::from(position.y)));
        actions.extend(startup.runtime_mut().update_pointer_frame(
            pointer_target,
            pointer.map(|pointer| pointer.y),
            pointer_pressed,
            pointer_held,
        ));
        if actions.contains(&hbc_rs::app::Action::DeleteSelected) {
            background_animation.pop_all();
        }
        #[cfg(feature = "startup-diagnostics")]
        if !display_ready {
            hbc_rs::platform::diagnostics::mark("FIRST ICON LOAD");
        }
        startup.refresh_visible_icons();
        let mut scene = hbc_rs::render::browser_frame_with_language(
            startup.runtime().controller(),
            startup.runtime().catalog(),
            startup.runtime().theme(),
            startup.logical_width(),
            language,
        );
        if startup.runtime().controller().screen == hbc_rs::app::Screen::Browser {
            let indicators = hbc_rs::platform::wiiload::indicator_state();
            scene.status_indicators(indicators.network_active, indicators.gecko_active);
        }
        bubble_clock_frames += 1;
        if bubble_clock_frames >= 600 {
            bubble_clock_frames = 0;
            let now_minutes = ogc_system::time() / 60;
            bubble_minute_cycle = ((now_minutes % 1440 + 1200) % 1440) as u16;
        }
        scene.set_background_bubbles(&background_animation.tick(
            view_width,
            bubble_minute_cycle,
            pointer.map(|point| (f32::from(point.x), f32::from(point.y))),
        ));
        if let Some(pointer) = pointer {
            scene.pointer_cursor(f32::from(pointer.x), f32::from(pointer.y));
        }
        if storage_scan_pending {
            scene.storage_loading_with_language(language);
        }
        #[cfg(feature = "startup-diagnostics")]
        if !display_ready {
            hbc_rs::platform::diagnostics::mark("FIRST GX FRAME");
        }
        #[cfg(not(feature = "startup-diagnostics"))]
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
        #[cfg(feature = "startup-diagnostics")]
        if display_ready
            && unsafe {
                hbc_rs::platform::renderer::draw_frame(
                    &video,
                    &scene,
                    startup.icon_textures(),
                    startup.theme_textures(),
                )
            }
            .is_err()
        {
            if hbc_rs::platform::diagnostics::fail("GX RENDER ERROR") {
                exit_to_system(&mut startup, ogc_system::SYS_RESTART);
            }
            return -3;
        }
        #[cfg(feature = "startup-diagnostics")]
        {
            let info = video.info();
            let Some((square_x, square_y)) =
                hbc_rs::platform::diagnostics::test_square_origin(info.width, info.height)
            else {
                if hbc_rs::platform::diagnostics::fail("GX TEST RECT BOUNDS") {
                    exit_to_system(&mut startup, ogc_system::SYS_RESTART);
                }
                loop {
                    hbc_rs::platform::diagnostics::hold();
                }
            };
            video.configure_2d();
            if unsafe {
                video.draw_solid_rect(
                    f32::from(square_x),
                    f32::from(square_y),
                    f32::from(hbc_rs::platform::diagnostics::TEST_SQUARE_SIZE),
                    f32::from(hbc_rs::platform::diagnostics::TEST_SQUARE_SIZE),
                    ogc_gx::GxColor {
                        r: 255,
                        g: 255,
                        b: 255,
                        a: 255,
                    },
                )
            }
            .is_err()
            {
                if hbc_rs::platform::diagnostics::fail("GX TEST RECT ERROR") {
                    exit_to_system(&mut startup, ogc_system::SYS_RESTART);
                }
                loop {
                    hbc_rs::platform::diagnostics::hold();
                }
            }
        }
        #[cfg(feature = "startup-diagnostics")]
        if !display_ready {
            hbc_rs::platform::diagnostics::mark("GX COPY START");
        }
        if let Err(_error) = unsafe { video.present() } {
            #[cfg(feature = "startup-diagnostics")]
            {
                let message = match _error {
                    hbc_rs::platform::video::VideoError::ViRetraceTimeout => "VI IRQ COUNT STALLED",
                    _ => "GX DRAW DONE TIMEOUT",
                };
                if hbc_rs::platform::diagnostics::fail(message) {
                    exit_to_system(&mut startup, ogc_system::SYS_RESTART);
                }
            }
            return -4;
        }
        if !display_ready {
            #[cfg(feature = "startup-diagnostics")]
            {
                hbc_rs::platform::diagnostics::mark("GX COPY QUEUED");
                hbc_rs::platform::diagnostics::hold();
                if !video.framebuffer_has_visible_luma() {
                    if hbc_rs::platform::diagnostics::fail("GX XFB BLACK") {
                        exit_to_system(&mut startup, ogc_system::SYS_RESTART);
                    }
                    loop {
                        hbc_rs::platform::diagnostics::hold();
                    }
                }
                if !video.framebuffer_has_test_square() {
                    if hbc_rs::platform::diagnostics::fail("GX TEST SQUARE MISSING") {
                        exit_to_system(&mut startup, ogc_system::SYS_RESTART);
                    }
                    // Replace the failed GX image with a CPU-written control pattern.
                    // This separates VI scanout from GX drawing and copy behavior.
                    video.paint_scanout_probe();
                    video.show();
                    video.wait_vsync();
                    loop {
                        hbc_rs::platform::diagnostics::hold();
                    }
                }
                hbc_rs::platform::diagnostics::mark("GX XFB HAS IMAGE");
                hbc_rs::platform::diagnostics::hold();
            }
            video.show();
            display_ready = true;
            #[cfg(feature = "startup-diagnostics")]
            hbc_rs::platform::diagnostics::complete();
        }
        video.wait_vsync();
        #[cfg(feature = "startup-diagnostics")]
        if hbc_rs::platform::diagnostics::sample_frame() {
            exit_to_system(&mut startup, ogc_system::SYS_RESTART);
        }

        for action in actions {
            if action == hbc_rs::app::Action::AcceptWiiloadZip {
                if let Some(bytes) = pending_zip.take() {
                    if startup.start_received_zip(bytes).is_err() {
                        startup.runtime_mut().show_notice_with_kind(
                            "Could not install the app or apply the theme.",
                            hbc_rs::app::NoticeKind::Error,
                        );
                    }
                }
                continue;
            }
            if action == hbc_rs::app::Action::RejectWiiloadZip {
                pending_zip = None;
                continue;
            }
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
                    startup
                        .runtime_mut()
                        .show_notice_with_kind(notice, hbc_rs::app::NoticeKind::Error);
                }
                continue;
            }
            if action == hbc_rs::app::Action::ApplyTheme {
                match startup.prepare_selected_theme() {
                    Ok(theme) => startup.apply_theme(theme),
                    Err(hbc_rs::platform::startup::StartupError::ThemeStorage(_)) => {
                        startup.runtime_mut().show_notice_with_kind(
                            "Could not save this theme to NAND.",
                            hbc_rs::app::NoticeKind::Error,
                        )
                    }
                    Err(_) => startup.runtime_mut().show_notice_with_kind(
                        "Could not read or validate this theme.",
                        hbc_rs::app::NoticeKind::Error,
                    ),
                }
                continue;
            }
            if action == hbc_rs::app::Action::LaunchSelected {
                let Ok(image) = startup.prepare_selected_executable() else {
                    startup.runtime_mut().show_notice_with_kind(
                        "Could not read or validate this application.",
                        hbc_rs::app::NoticeKind::Error,
                    );
                    continue;
                };
                if let Err(notice) = transfer_executable(
                    &mut startup,
                    &image.bytes,
                    &image.plan,
                    &image.arguments,
                    image.ahb_access,
                    "Could not prepare this application to run.",
                ) {
                    startup
                        .runtime_mut()
                        .show_notice_with_kind(notice, hbc_rs::app::NoticeKind::Error);
                    continue;
                }
            }
            if action == hbc_rs::app::Action::DeleteSelected {
                if startup.delete_selected_app().is_err() {
                    startup.runtime_mut().show_notice_with_kind(
                        "Could not delete this application.",
                        hbc_rs::app::NoticeKind::Error,
                    );
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
                        startup.runtime_mut().show_notice_with_kind(
                            "Could not stop storage. BootMii launch stopped.",
                            hbc_rs::app::NoticeKind::Error,
                        );
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
    ahb_access: bool,
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
    unsafe {
        hbc_rs::loader::transfer_control(plan.entry, hbc_rs::loader::ios_launch_policy(ahb_access))
    }
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
fn reset_after_diagnostic_report() -> ! {
    unsafe { ogc_system::reset_system(ogc_system::SYS_RESTART as i32, 0, 0) };
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(target_arch = "powerpc")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    #[cfg(feature = "startup-diagnostics")]
    if hbc_rs::platform::diagnostics::fail("RUST PANIC") {
        reset_after_diagnostic_report();
    }
    loop {
        core::hint::spin_loop();
    }
}
