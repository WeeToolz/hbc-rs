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
    #[cfg(feature = "debugger-usb-gecko")]
    hbc_rs::platform::lifecycle::initialize_debugger();
    #[cfg(feature = "startup-diagnostics")]
    let msr_after_system_init: u32;
    #[cfg(feature = "startup-diagnostics")]
    unsafe {
        core::arch::asm!(
            "mfmsr {0}",
            out(reg) msr_after_system_init,
            options(nostack, preserves_flags)
        );
    }
    #[cfg(feature = "startup-diagnostics")]
    if hbc_rs::platform::diagnostics::initialize(_argc, _argv).is_err() {
        if hbc_rs::platform::diagnostics::fail("CPU VIDEO INIT ERROR") {
            reset_after_diagnostic_report();
        }
        return -4;
    }
    #[cfg(feature = "startup-diagnostics")]
    {
        let fp_disabled_stage = ogc_system::runtime_fp_disabled_stage();
        let disabled_label = match fp_disabled_stage {
            13 => Some("FP OFF BEFORE START"),
            1 => Some("FP OFF LOW MEMORY"),
            2 => Some("FP OFF IPC BUFFERS"),
            3 => Some("FP OFF LWP SETUP"),
            4 => Some("FP OFF RESET QUEUE"),
            5 => Some("FP OFF EXI"),
            6 => Some("FP OFF SRAM"),
            7 => Some("FP OFF SI"),
            8 => Some("FP OFF DSP"),
            9 => Some("FP OFF MEM PROTECT"),
            10 => Some("FP OFF CALLBACKS"),
            11 => Some("FP OFF IPC CLIENT"),
            12 => Some("FP OFF GBS MODE"),
            _ => None,
        };
        if let Some(label) = disabled_label {
            hbc_rs::platform::diagnostics::mark(label);
            loop {
                core::hint::spin_loop();
            }
        }
        hbc_rs::platform::diagnostics::mark_fp_value(
            msr_after_system_init,
            "FP AFTER SYSTEM INIT ON",
            "FP AFTER SYSTEM INIT OFF",
        );
    }
    #[cfg(feature = "startup-diagnostics")]
    hbc_rs::platform::diagnostics::mark_fp_state(
        "FP AFTER TRACE INIT ON",
        "FP AFTER TRACE INIT OFF",
    );
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
    if hbc_rs::platform::diagnostics::arm_recovery().is_err() {
        hbc_rs::platform::diagnostics::mark("RECOVERY WORKER FAILED");
        return -5;
    }
    #[cfg(all(target_arch = "powerpc", feature = "recovery-validation"))]
    if hbc_rs::platform::diagnostics::hold_for_recovery_validation().is_err() {
        return -6;
    }
    install_return_stub();
    let wiilaunch_init = ogc_wiilaunch::initialize();
    if wiilaunch_init < 0 {
        ogc_system::report(format_args!(
            "HBC: Wii return-to-menu initialization failed: {wiilaunch_init}\n"
        ));
    }
    #[cfg(feature = "startup-diagnostics")]
    hbc_rs::platform::diagnostics::mark_fp_state("FP AFTER HEAP ON", "FP AFTER HEAP OFF");

    #[cfg(feature = "startup-diagnostics")]
    {
        hbc_rs::platform::diagnostics::begin_reporting();
        if hbc_rs::platform::diagnostics::startup_checkpoint() == Some(false) {
            hbc_rs::platform::diagnostics::mark("STARTUP CHECKPOINT SEND FAILED");
        }
    }

    #[cfg(feature = "startup-diagnostics")]
    hbc_rs::platform::diagnostics::mark("CONF START");
    let _ = ogc_conf::init();
    #[cfg(feature = "startup-diagnostics")]
    hbc_rs::platform::diagnostics::mark("CONF READY");
    #[cfg(feature = "startup-diagnostics")]
    hbc_rs::platform::diagnostics::mark_fp_state("FP AFTER CONF ON", "FP AFTER CONF OFF");
    let view_width = if ogc_conf::get_aspect_ratio() == ogc_conf::CONF_ASPECT_16_9 as i32 {
        hbc_rs::render::WIDE_LOGICAL_WIDTH
    } else {
        hbc_rs::render::LOGICAL_WIDTH
    };
    ogc_system::report(format_args!("HBC: initializing video\n"));
    let video =
        match hbc_rs::platform::video::initialize(view_width > hbc_rs::render::LOGICAL_WIDTH) {
            Ok(video) => video,
            Err(error) => {
                ogc_system::report(format_args!(
                    "HBC: video initialization failed: {error:?}\n"
                ));
                #[cfg(feature = "startup-diagnostics")]
                if hbc_rs::platform::diagnostics::fail("VIDEO OR GX FIFO ERROR") {
                    reset_after_diagnostic_report();
                }
                return -2;
            }
        };
    ogc_system::report(format_args!("HBC: video initialized\n"));
    #[cfg(feature = "startup-diagnostics")]
    hbc_rs::platform::diagnostics::mark("GX 2D SETUP");
    video.configure_2d();
    #[cfg(feature = "startup-diagnostics")]
    hbc_rs::platform::diagnostics::mark("GX 2D READY");

    // C HBC submits its startup disc motor stop asynchronously. ogc-di's
    // stop_motor() is synchronous, so calling it here can block first-frame
    // startup when IOS or an emulator does not complete the DI request. Keep
    // the shutdown/handoff stop below, but do not substitute a synchronous
    // operation for C's non-blocking startup request.

    ogc_system::report(format_args!("HBC: loading settings and theme\n"));
    let playtime_reset_error = hbc_rs::platform::settings::reset_playtime_record().err();
    let mut startup = hbc_rs::platform::startup::WiiStartup::initialize(view_width, 480);
    ogc_system::report(format_args!("HBC: settings and theme loaded\n"));
    if let Some(error) = playtime_reset_error {
        ogc_system::report(format_args!("HBC: playtime reset failed: {error:?}\n"));
    }
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
    let mut wiiload_cleanup_notice_shown = false;
    let mut network_check: Option<hbc_rs::platform::network_restart::ConnectionCheck> = None;
    let mut network_indicator_frame = 0u16;
    let mut reported_receiver_status = None;
    let mut remote_input = hbc_rs::remote_input::RemoteState::default();
    let mut remote_operation: Option<(u32, u64, hbc_rs::platform::control::Operation)> = None;
    let mut remote_result: Option<(u32, Result<String, String>)> = None;
    ogc_system::report(format_args!(
        "HBC: startup complete; entering render loop\n"
    ));

    loop {
        #[cfg(feature = "startup-diagnostics")]
        if display_ready {
            hbc_rs::platform::diagnostics::begin_runtime_trace();
        }
        #[cfg(feature = "startup-diagnostics")]
        hbc_rs::platform::diagnostics::runtime_stage(1, "RUNTIME LOOP ENTER");
        let first_frame = !display_ready;
        if let Some(request) = hbc_rs::platform::lifecycle::take_request() {
            hbc_rs::platform::lifecycle::defer_exit(&mut deferred_exit, request);
        }
        #[cfg(feature = "debugger-network")]
        {
            let screen = startup.runtime().controller().screen;
            let normal_menu = display_ready
                && matches!(
                    screen,
                    hbc_rs::app::Screen::Browser | hbc_rs::app::Screen::MainMenu
                );
            let no_install_pending = pending_zip.is_none()
                && !startup.received_zip_install_pending()
                && !matches!(
                    hbc_rs::platform::wiiload::status(),
                    hbc_rs::platform::wiiload::ReceiverStatus::Receiving
                        | hbc_rs::platform::wiiload::ReceiverStatus::PayloadReady
                );
            let conditions = libdb_sys::wii_network::PauseConditions {
                normal_menu,
                no_exit_pending: deferred_exit.is_none(),
                no_install_pending,
                no_storage_operation: !startup.storage_scan_pending()
                    && !startup.device_probe_pending(),
                no_remote_operation: remote_operation.is_none(),
                no_network_transition: !matches!(
                    screen,
                    hbc_rs::app::Screen::NetworkSettings
                        | hbc_rs::app::Screen::NetworkScan
                        | hbc_rs::app::Screen::NetworkKeyboard
                ),
                no_modal_workflow: normal_menu,
            };
            let result = hbc_rs::platform::wiiload::COOPERATIVE_CONTROL
                .service(deferred_exit.is_some(), conditions.is_eligible());
            if result == libdb_sys::wii_network::MainControlResult::EnteredPause {
                if let Some(request) = cooperative_pause_loop() {
                    hbc_rs::platform::lifecycle::defer_exit(&mut deferred_exit, request);
                }
            }
        }
        #[cfg(feature = "debugger-usb-gecko")]
        if hbc_rs::platform::lifecycle::take_debugger_break() {
            libdb_sys::_break();
        }
        if first_frame {
            ogc_system::report(format_args!("HBC: first lifecycle poll complete\n"));
        }

        startup.poll_storage_scan();
        #[cfg(feature = "startup-diagnostics")]
        hbc_rs::platform::diagnostics::runtime_stage(2, "STORAGE RESULT POLL DONE");
        if first_frame {
            ogc_system::report(format_args!("HBC: first storage poll complete\n"));
        }
        if let Err(error) = startup.poll_device_availability() {
            ogc_system::report(format_args!(
                "HBC: storage availability probe failed: {error:?}\n"
            ));
            startup.runtime_mut().show_notice_with_kind(
                "Could not refresh storage devices.",
                hbc_rs::app::NoticeKind::Error,
            );
        }
        #[cfg(feature = "startup-diagnostics")]
        hbc_rs::platform::diagnostics::runtime_stage(4, "DEVICE AVAILABILITY POLL DONE");
        if first_frame {
            ogc_system::report(format_args!("HBC: first device poll complete\n"));
        }
        let receiver_status = hbc_rs::platform::wiiload::status();
        if reported_receiver_status != Some(receiver_status) {
            #[cfg(feature = "startup-diagnostics")]
            hbc_rs::platform::diagnostics::event(&format!(
                "WIILOAD RECEIVER STATE {receiver_status:?}"
            ));
            match hbc_rs::platform::wiiload::cleanup_error() {
                Some(error) => ogc_system::report(format_args!(
                    "HBC: Wiiload receiver state changed to {receiver_status:?}; cleanup error={error}\n"
                )),
                None => ogc_system::report(format_args!(
                    "HBC: Wiiload receiver state changed to {receiver_status:?}\n"
                )),
            }
            reported_receiver_status = Some(receiver_status);
        }
        #[cfg(feature = "startup-diagnostics")]
        hbc_rs::platform::diagnostics::runtime_stage(8, "WIILOAD STATUS POLL DONE");
        if first_frame {
            ogc_system::report(format_args!("HBC: first receiver poll complete\n"));
        }
        if let Some(result) = network_check.as_ref().and_then(|check| {
            check.poll(
                receiver_status,
                hbc_rs::platform::wiiload::local_ip(),
                hbc_rs::platform::network_restart::now_ms(),
            )
        }) {
            network_check = None;
            let (message, kind) = match result {
                Ok(address) => (
                    format!(
                        "Network connection succeeded. Wii address: {}.{}.{}.{}. Wiiload is listening.",
                        address[0], address[1], address[2], address[3]
                    ),
                    hbc_rs::app::NoticeKind::Information,
                ),
                Err(error) => (
                    format!("Network connection failed: {error:?}."),
                    hbc_rs::app::NoticeKind::Error,
                ),
            };
            startup.runtime_mut().show_notice_with_kind(&message, kind);
        }
        if receiver_status == hbc_rs::platform::wiiload::ReceiverStatus::CleanupError
            && !wiiload_cleanup_notice_shown
        {
            if let Some(error) = hbc_rs::platform::wiiload::cleanup_error() {
                startup.runtime_mut().show_notice_with_kind(
                    &format!("Wiiload cleanup failed ({error})."),
                    hbc_rs::app::NoticeKind::Error,
                );
                wiiload_cleanup_notice_shown = true;
            }
        }
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
            let (message, kind) = match (
                completion.operation,
                completion.cleanup_error,
                completion.worker_join,
            ) {
                (Ok(()), None, Ok(())) => (
                    String::from("Received ZIP is complete."),
                    hbc_rs::app::NoticeKind::Information,
                ),
                (Ok(()), Some(cleanup), Ok(())) => (
                    format!("The app was installed, but its previous copy remains: {cleanup:?}."),
                    hbc_rs::app::NoticeKind::Warning,
                ),
                (Err(operation), _, Ok(())) => (operation.notice(), hbc_rs::app::NoticeKind::Error),
                (Ok(()), _, Err(join)) => (join.notice(), hbc_rs::app::NoticeKind::Error),
                (Err(operation), _, Err(join)) => (
                    format!("{} {}", operation.notice(), join.notice()),
                    hbc_rs::app::NoticeKind::Error,
                ),
            };
            startup.runtime_mut().show_notice_with_kind(&message, kind);
        }
        if let Some(progress) = startup.received_zip_install_progress() {
            startup.runtime_mut().set_wiiload_install_progress(progress);
        }
        if let Some(request) = hbc_rs::platform::lifecycle::take_ready_exit(
            &mut deferred_exit,
            startup.received_zip_install_pending()
                || startup.storage_scan_pending()
                || startup.device_probe_pending(),
        ) {
            let reset = match request {
                hbc_rs::platform::lifecycle::ExitRequest::ReturnToMenu => {
                    ogc_system::SYS_RETURNTOMENU
                }
                hbc_rs::platform::lifecycle::ExitRequest::PowerOff => ogc_system::SYS_POWEROFF,
            };
            if let Err(message) = exit_to_system(&mut startup, reset) {
                startup
                    .runtime_mut()
                    .show_notice_with_kind(&message, hbc_rs::app::NoticeKind::Error);
            }
        }

        if deferred_exit.is_none() && !startup.storage_scan_pending() {
            if pending_zip.is_none() && !startup.received_zip_install_pending() {
                if let Some(payload) = hbc_rs::platform::wiiload::take_payload() {
                    ogc_system::report(format_args!(
                        "HBC: received payload ready: {} bytes\n",
                        payload.data.len()
                    ));
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
                                    hbc_rs::loader::LaunchSource::from_vec(payload.data),
                                    plan,
                                    payload.args,
                                    false,
                                    "Could not prepare the received application.",
                                ) {
                                    startup.runtime_mut().show_notice(&notice);
                                }
                            }
                            Err(error) => {
                                ogc_system::report(format_args!(
                                    "HBC: received executable validation failed: {error:?}\n"
                                ));
                                startup.runtime_mut().show_notice(&format!(
                                    "Received data is not a valid Wii application: {error:?}."
                                ));
                            }
                        }
                    }
                }
            }
        }
        frames_since_storage_scan = frames_since_storage_scan.saturating_add(1);
        if frames_since_storage_scan >= 30 && deferred_exit.is_none() {
            frames_since_storage_scan = 0;
            if startup.runtime().controller().screen == hbc_rs::app::Screen::Options {
                if let Err(error) = startup.refresh_device_availability() {
                    ogc_system::report(format_args!(
                        "HBC: could not start storage availability probe: {error:?}\n"
                    ));
                    startup.runtime_mut().show_notice_with_kind(
                        "Could not refresh storage devices.",
                        hbc_rs::app::NoticeKind::Error,
                    );
                }
            }
            if let Err(error) = startup.refresh_storage() {
                ogc_system::report(format_args!("HBC: storage refresh failed: {error:?}\n"));
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
        if first_frame {
            ogc_system::report(format_args!("HBC: first controller scan starting\n"));
        }
        let mut frame = match hbc_rs::platform::input::scan() {
            Ok(frame) => frame,
            Err(error) => {
                ogc_system::report(format_args!(
                    "HBC: controller input scan failed: {error:?}\n"
                ));
                startup.runtime_mut().show_notice_with_kind(
                    "Controller input failed. See the system log for details.",
                    hbc_rs::app::NoticeKind::Warning,
                );
                hbc_rs::platform::input::InputFrame::default()
            }
        };
        #[cfg(feature = "startup-diagnostics")]
        hbc_rs::platform::diagnostics::runtime_stage(16, "RUNTIME INPUT SCAN DONE");
        if first_frame {
            ogc_system::report(format_args!("HBC: first controller scan complete\n"));
        }
        hbc_rs::platform::wiiload::merge_remote_input(&mut remote_input, &mut frame);
        let storage_scan_pending = startup.storage_scan_pending();
        // Device probes own storage in their worker. Menu input stays active;
        // operations that need storage validate worker ownership at the backend.
        let pointer = frame.pointer;
        let pointer_pressed = frame.pointer_pressed;
        let pointer_held = frame.pointer_held;
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
        let screenshot_requested = frame.screenshot_requested;
        let actions = startup.runtime_mut().update_frame_with_pointer(
            &mut frame,
            pointer_target,
            pointer.map(|pointer| pointer.y),
            pointer_pressed,
            pointer_held,
        );
        #[cfg(feature = "startup-diagnostics")]
        for action in &actions {
            hbc_rs::platform::diagnostics::event(&format!("HBC input action: {action:?}"));
        }
        #[cfg(feature = "startup-diagnostics")]
        hbc_rs::platform::diagnostics::runtime_stage(32, "RUNTIME INPUT UPDATE DONE");
        if actions.contains(&hbc_rs::app::Action::DeleteSelected) {
            background_animation.pop_all();
        }
        #[cfg(feature = "startup-diagnostics")]
        if !display_ready {
            hbc_rs::platform::diagnostics::mark("FIRST ICON LOAD");
        }
        startup.refresh_visible_icons();
        let ios_label = format!(
            "IOS{} v{}.{}",
            ogc_ios::get_version(),
            ogc_ios::get_revision_major(),
            ogc_ios::get_revision_minor()
        );
        let network_label = match hbc_rs::platform::wiiload::local_ip() {
            Some(address) => format!(
                "Your Wii's IP is {}.{}.{}.{}",
                address[0], address[1], address[2], address[3]
            ),
            None => String::from("Network not initialized"),
        };
        startup
            .runtime_mut()
            .set_main_menu_status(ios_label, network_label);
        let mut scene = hbc_rs::render::browser_frame_with_language(
            startup.runtime().controller(),
            startup.runtime().catalog(),
            startup.runtime().theme(),
            startup.logical_width(),
            language,
        );
        if first_frame {
            ogc_system::report(format_args!("HBC: first scene construction complete\n"));
        }
        if first_frame {
            ogc_system::report(format_args!("HBC: first memo layout starting\n"));
        }
        match hbc_rs::platform::renderer::layout_memos(&mut scene, startup.theme_textures()) {
            Ok(Some((max_scroll, line_height))) => {
                startup
                    .runtime_mut()
                    .set_measured_memo(max_scroll, line_height);
            }
            Ok(None) => {}
            Err(error) => {
                ogc_system::report(format_args!("HBC: memo layout failed: {error:?}\n"));
                #[cfg(feature = "startup-diagnostics")]
                hbc_rs::platform::diagnostics::fail("TEXT LAYOUT ERROR");
                return -3;
            }
        }
        if first_frame {
            ogc_system::report(format_args!("HBC: first memo layout complete\n"));
        }
        if startup.runtime().controller().screen == hbc_rs::app::Screen::Browser {
            let indicators = hbc_rs::platform::wiiload::indicator_state();
            let network_active = hbc_rs::wiiload::network_indicator_visible(
                receiver_status,
                network_indicator_frame,
            );
            if receiver_status == hbc_rs::wiiload::ReceiverStatus::Starting {
                network_indicator_frame = (network_indicator_frame + 1) % 40;
            } else {
                network_indicator_frame = 0;
            }
            scene.status_indicators(network_active, indicators.gecko_active);
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
            scene.pointer_cursor(f32::from(pointer.x), f32::from(pointer.y), pointer_held);
        }
        if storage_scan_pending {
            scene.storage_loading();
        }
        if first_frame {
            ogc_system::report(format_args!("HBC: first GX scene drawing starting\n"));
        }
        #[cfg(feature = "startup-diagnostics")]
        if !display_ready {
            hbc_rs::platform::diagnostics::mark("FIRST GX FRAME");
        }
        let draw_result = unsafe {
            hbc_rs::platform::renderer::draw_frame(
                &video,
                &scene,
                startup.icon_textures(),
                startup.theme_textures(),
            )
        };
        #[cfg(feature = "startup-diagnostics")]
        hbc_rs::platform::diagnostics::runtime_stage(64, "RUNTIME GX DRAW DONE");
        if let Err(error) = draw_result {
            ogc_system::report(format_args!("HBC: frame rendering failed: {error:?}\n"));
            #[cfg(feature = "startup-diagnostics")]
            if hbc_rs::platform::diagnostics::fail("GX RENDER ERROR") {
                if let Err(message) = exit_to_system(&mut startup, ogc_system::SYS_RESTART) {
                    hbc_rs::platform::diagnostics::fail(&message);
                }
            }
            return -3;
        }
        if first_frame {
            ogc_system::report(format_args!("HBC: first GX scene drawing complete\n"));
        }
        #[cfg(feature = "startup-diagnostics")]
        {
            let info = video.info();
            let Some((square_x, square_y)) =
                hbc_rs::platform::diagnostics::test_square_origin(info.width, info.height)
            else {
                if hbc_rs::platform::diagnostics::fail("GX TEST RECT BOUNDS") {
                    if let Err(message) = exit_to_system(&mut startup, ogc_system::SYS_RESTART) {
                        hbc_rs::platform::diagnostics::fail(&message);
                    }
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
                    if let Err(message) = exit_to_system(&mut startup, ogc_system::SYS_RESTART) {
                        hbc_rs::platform::diagnostics::fail(&message);
                    }
                }
                loop {
                    hbc_rs::platform::diagnostics::hold();
                }
            }
        }
        #[cfg(feature = "startup-diagnostics")]
        let first_efb_square = if !display_ready {
            match video.efb_has_test_square() {
                Ok(found) => {
                    hbc_rs::platform::diagnostics::mark(if found {
                        "GX EFB SQUARE PRESENT"
                    } else {
                        "GX EFB SQUARE MISSING"
                    });
                    found
                }
                Err(_) => {
                    if hbc_rs::platform::diagnostics::fail("GX EFB READBACK TIMEOUT") {
                        if let Err(message) = exit_to_system(&mut startup, ogc_system::SYS_RESTART)
                        {
                            hbc_rs::platform::diagnostics::fail(&message);
                        }
                    }
                    loop {
                        hbc_rs::platform::diagnostics::hold();
                    }
                }
            }
        } else {
            false
        };
        #[cfg(feature = "startup-diagnostics")]
        if !display_ready {
            hbc_rs::platform::diagnostics::mark("GX COPY START");
        }
        // Hardware commands run on this thread, after input polling services BT
        // completions. The socket worker only queues commands and returns status.
        let control_now = ogc_lwp::sys::now_ticks() / (ogc_lwp::sys::TICKS_PER_SECOND / 1000);
        if let Some((id, started, operation)) = remote_operation.as_ref() {
            let result = operation.poll().or_else(|| {
                (control_now.saturating_sub(*started) >= 60_000).then(|| {
                    Err(String::from(
                        "Hardware operation did not complete within 60 seconds",
                    ))
                })
            });
            if let Some(result) = result {
                remote_result = Some((*id, result));
                remote_operation = None;
            }
        }
        if let Some((id, result)) = remote_result.as_ref() {
            if hbc_rs::platform::wiiload::REMOTE_CONTROL.finish(*id, result) {
                ogc_system::report(format_args!(
                    "HBC: terminal request {id} result: {result:?}\n"
                ));
                remote_result = None;
            }
        }
        if display_ready
            && remote_operation.is_none()
            && remote_result.is_none()
            && !startup.storage_scan_pending()
            && !startup.device_probe_pending()
            && !startup.received_zip_install_pending()
            && !matches!(
                hbc_rs::platform::wiiload::status(),
                hbc_rs::wiiload::ReceiverStatus::Receiving
                    | hbc_rs::wiiload::ReceiverStatus::PayloadReady
            )
        {
            if let Some((id, command)) = hbc_rs::platform::wiiload::REMOTE_CONTROL.take(control_now)
            {
                use hbc_rs::remote_control::Command;
                ogc_system::report(format_args!(
                    "HBC: terminal request {id} executing {command:?}\n"
                ));
                let reset = match command {
                    Command::Restart => Some(ogc_system::SYS_RESTART),
                    Command::Reset => Some(ogc_system::SYS_RETURNTOMENU),
                    Command::PowerOff => Some(ogc_system::SYS_POWEROFF),
                    _ => None,
                };
                if let Some(reset) = reset {
                    // A successful reset never returns. Never publish completed
                    // before the hardware has actually changed execution state.
                    if let Err(mut message) = exit_to_system(&mut startup, reset) {
                        if hbc_rs::platform::wiiload::status()
                            == hbc_rs::wiiload::ReceiverStatus::Stopped
                        {
                            if let Err(error) = hbc_rs::platform::input::initialize(
                                startup.logical_width(),
                                hbc_rs::render::LOGICAL_HEIGHT,
                            ) {
                                message.push_str(&format!(" Controller recovery failed: {error}."));
                            }
                            if let Err(error) = hbc_rs::platform::wiiload::start() {
                                message.push_str(&format!(" Receiver recovery failed: {error:?}."));
                            }
                        }
                        startup
                            .runtime_mut()
                            .show_notice_with_kind(&message, hbc_rs::app::NoticeKind::Error);
                        remote_result = Some((id, Err(message)));
                    }
                } else {
                    match hbc_rs::platform::control::Operation::begin(command, &mut startup, &video)
                    {
                        Ok(operation) => remote_operation = Some((id, control_now, operation)),
                        Err(error) => remote_result = Some((id, Err(error))),
                    }
                }
            }
        }
        if display_ready && screenshot_requested {
            // Do not allocate or read back the framebuffer when no storage is
            // mounted. save_screenshot_png still rechecks storage at save time.
            let saved = hbc_rs::platform::home_overlay::capture_and_save_screenshot(
                &video,
                startup.has_storage(),
                |bytes| startup.save_screenshot_png(bytes),
            )
            .map_err(|error| format!("{error:?}"));
            if let Err(error) = saved {
                startup.runtime_mut().show_notice_with_kind(
                    &format!("Screenshot failed ({error})."),
                    hbc_rs::app::NoticeKind::Error,
                );
            }
        }
        if first_frame {
            ogc_system::report(format_args!(
                "HBC: first framebuffer presentation starting\n"
            ));
        }
        if let Err(_error) = unsafe { video.present() } {
            ogc_system::report(format_args!("HBC: frame presentation failed: {_error:?}\n"));
            #[cfg(feature = "startup-diagnostics")]
            {
                let message = match _error {
                    hbc_rs::platform::video::VideoError::ViRetraceTimeout => "VI IRQ COUNT STALLED",
                    _ => "GX DRAW DONE TIMEOUT",
                };
                if hbc_rs::platform::diagnostics::fail(message) {
                    if let Err(message) = exit_to_system(&mut startup, ogc_system::SYS_RESTART) {
                        hbc_rs::platform::diagnostics::fail(&message);
                    }
                }
            }
            return -4;
        }
        #[cfg(feature = "startup-diagnostics")]
        hbc_rs::platform::diagnostics::runtime_stage(128, "RUNTIME GX PRESENT DONE");
        if first_frame {
            ogc_system::report(format_args!(
                "HBC: first scene rendered and copied to XFB\n"
            ));
        }
        if !display_ready {
            #[cfg(feature = "startup-diagnostics")]
            {
                hbc_rs::platform::diagnostics::mark("GX COPY QUEUED");
                hbc_rs::platform::diagnostics::hold();
                if !video.framebuffer_has_visible_luma() {
                    let label = if first_efb_square {
                        "GX XFB COPY BLACK"
                    } else {
                        "GX EFB DRAW BLACK"
                    };
                    hbc_rs::platform::diagnostics::mark(label);
                    video.paint_scanout_probe(label);
                    video.show();
                    video.wait_vsync();
                    loop {
                        hbc_rs::platform::diagnostics::hold();
                    }
                }
                if !video.framebuffer_has_test_square() {
                    if hbc_rs::platform::diagnostics::fail("GX TEST SQUARE MISSING") {
                        if let Err(message) = exit_to_system(&mut startup, ogc_system::SYS_RESTART)
                        {
                            hbc_rs::platform::diagnostics::fail(&message);
                        }
                    }
                    // Replace the failed GX image with a CPU-written control pattern.
                    // This separates VI scanout from GX drawing and copy behavior.
                    let label = if first_efb_square {
                        "GX XFB SQUARE FAIL"
                    } else {
                        "GX EFB SQUARE FAIL"
                    };
                    video.paint_scanout_probe(label);
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
            ogc_system::report(format_args!(
                "HBC: display enabled after first rendered frame\n"
            ));
            #[cfg(feature = "startup-diagnostics")]
            hbc_rs::platform::diagnostics::complete();
        }
        #[cfg(feature = "startup-diagnostics")]
        if !ogc_video::wait_vsync_timeout_us(1_000_000) {
            if hbc_rs::platform::diagnostics::fail("VI FRAME RETRACE TIMEOUT") {
                if let Err(message) = exit_to_system(&mut startup, ogc_system::SYS_RESTART) {
                    hbc_rs::platform::diagnostics::fail(&message);
                }
            }
            loop {
                hbc_rs::platform::diagnostics::hold();
            }
        }
        #[cfg(feature = "startup-diagnostics")]
        hbc_rs::platform::diagnostics::runtime_stage(256, "RUNTIME VI RETRACE DONE");
        #[cfg(not(feature = "startup-diagnostics"))]
        video.wait_vsync();
        // Give the lower-priority storage worker a bounded slice after each
        // visible frame. Sleeping the main thread for 1 ms lets the worker run,
        // then the timer wake makes priority-64 main preempt priority-62 storage.
        // A plain yield is unsafe here: an equal-priority worker can retain the
        // CPU inside a long FAT/IOS operation and freeze animation and input.
        if startup.storage_scan_pending() || startup.device_probe_pending() {
            ogc_lwp::sys::sleep_ticks((ogc_lwp::sys::TICKS_PER_SECOND / 1_000).max(1));
        }
        #[cfg(feature = "startup-diagnostics")]
        hbc_rs::platform::diagnostics::runtime_stage(512, "RUNTIME WORKER SLICE DONE");
        #[cfg(feature = "startup-diagnostics")]
        if hbc_rs::platform::diagnostics::sample_frame() {
            if let Err(message) = exit_to_system(&mut startup, ogc_system::SYS_RESTART) {
                hbc_rs::platform::diagnostics::fail(&message);
            }
        }
        #[cfg(feature = "startup-diagnostics")]
        hbc_rs::platform::diagnostics::runtime_stage(1024, "RUNTIME REPORT SAMPLE DONE");

        for action in actions {
            let exit = match action {
                hbc_rs::app::Action::Exit => {
                    Some(hbc_rs::platform::lifecycle::ExitRequest::ReturnToMenu)
                }
                hbc_rs::app::Action::Shutdown => {
                    Some(hbc_rs::platform::lifecycle::ExitRequest::PowerOff)
                }
                _ => None,
            };
            if let Some(request) = exit {
                hbc_rs::platform::lifecycle::defer_exit(&mut deferred_exit, request);
                if startup.storage_scan_pending()
                    || startup.device_probe_pending()
                    || startup.received_zip_install_pending()
                {
                    startup.runtime_mut().show_notice_with_kind(
                        "Waiting for storage operations to finish before exiting.",
                        hbc_rs::app::NoticeKind::Information,
                    );
                }
                continue;
            }
            if deferred_exit.is_some() {
                continue;
            }
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
            if action == hbc_rs::app::Action::LoadNetworkConfig {
                match hbc_rs::platform::network_config::load(startup.storage_prefix()) {
                    Ok(config) => startup.runtime_mut().set_network_config(config),
                    Err(error) => startup.runtime_mut().show_notice_with_kind(
                        &format!("Could not read Wii network settings: {error:?}."),
                        hbc_rs::app::NoticeKind::Error,
                    ),
                }
                continue;
            }
            if action == hbc_rs::app::Action::SaveNetworkConfig
                || action == hbc_rs::app::Action::SaveNetworkConfigWithoutBackup
            {
                let make_backup = action == hbc_rs::app::Action::SaveNetworkConfig;
                ogc_system::report(format_args!("HBC: network Save action received\n"));
                let expected = startup.runtime().network_config_snapshot().cloned();
                let prepared = startup.runtime().network_config_for_save();
                match (expected, prepared) {
                    (Some(expected), Ok(config)) => match hbc_rs::platform::wiiload::stop() {
                        Err(error) => startup.runtime_mut().show_notice_with_kind(
                            &format!("Could not stop Wiiload; network settings were not saved: {error:?}."),
                            hbc_rs::app::NoticeKind::Error,
                        ),
                        Ok(()) => {
                            let save = if make_backup {
                                hbc_rs::platform::network_config::save(
                                    &config,
                                    &expected,
                                    startup.storage_prefix(),
                                )
                            } else {
                                hbc_rs::platform::network_config::save_without_backup(
                                    &config,
                                    &expected,
                                )
                            };
                            ogc_system::report(format_args!("HBC: network save result: {save:?}\n"));
                            let restart = hbc_rs::platform::network_restart::start();
                            match (save, restart) {
                                (Ok(()), Ok(check)) => match hbc_rs::platform::network_config::load(startup.storage_prefix()) {
                                    Ok(saved) => {
                                        startup.runtime_mut().set_network_config(saved);
                                        network_check = Some(check);
                                        startup.runtime_mut().show_notice_with_kind(
                                            "Settings saved. Connecting to the network...",
                                            hbc_rs::app::NoticeKind::Information,
                                        );
                                    }
                                    Err(error) => {
                                        startup.runtime_mut().clear_network_config();
                                        startup.runtime_mut().show_notice_with_kind(
                                            &format!("Settings were saved, but the committed file could not be reloaded: {error:?}."),
                                            hbc_rs::app::NoticeKind::Error,
                                        );
                                    }
                                },
                                (Err(save_error), Ok(_)) => startup.runtime_mut().show_notice_with_kind(
                                    &hbc_rs::platform::network_config::save_error_message(save_error),
                                    hbc_rs::app::NoticeKind::Error,
                                ),
                                (Ok(()), Err(restart_error)) => startup.runtime_mut().show_notice_with_kind(
                                    &format!("Settings were saved, but Wiiload could not restart: {restart_error:?}."),
                                    hbc_rs::app::NoticeKind::Error,
                                ),
                                (Err(save_error), Err(restart_error)) => startup.runtime_mut().show_notice_with_kind(
                                    &format!("{}. Wiiload restart also failed: {restart_error:?}.", hbc_rs::platform::network_config::save_error_message(save_error)),
                                    hbc_rs::app::NoticeKind::Error,
                                ),
                            }
                        }
                    },
                    (_, Err(error)) => startup.runtime_mut().show_notice_with_kind(
                        &format!("WiFi settings are invalid: {error:?}."),
                        hbc_rs::app::NoticeKind::Error,
                    ),
                    (None, Ok(_)) => startup.runtime_mut().show_notice_with_kind(
                        "Could not save Wii network settings: no loaded configuration snapshot.",
                        hbc_rs::app::NoticeKind::Error,
                    ),
                }
                continue;
            }
            if action == hbc_rs::app::Action::RestartNetwork {
                match hbc_rs::platform::network_restart::restart() {
                    Ok(check) => {
                        network_check = Some(check);
                        startup.runtime_mut().show_notice_with_kind(
                            "Restarting network. Waiting for an IP address...",
                            hbc_rs::app::NoticeKind::Information,
                        );
                    }
                    Err(error) => startup.runtime_mut().show_notice_with_kind(
                        &format!("Could not restart the network receiver: {error:?}."),
                        hbc_rs::app::NoticeKind::Error,
                    ),
                }
                continue;
            }
            if action == hbc_rs::app::Action::TestNetworkConnection {
                let saved = startup.runtime().network_config_snapshot().cloned();
                match startup.runtime().network_config_for_save() {
                    Err(error) => startup.runtime_mut().show_notice_with_kind(
                        &format!("WiFi settings are invalid: {error:?}."),
                        hbc_rs::app::NoticeKind::Error,
                    ),
                    Ok(_) if saved.is_none() => startup.runtime_mut().show_notice_with_kind(
                        "Load the saved Wii network settings before testing the connection.",
                        hbc_rs::app::NoticeKind::Error,
                    ),
                    Ok(config) if saved.as_ref() != Some(&config) => {
                        startup.runtime_mut().show_notice_with_kind(
                            "Save the edited network settings before testing the connection.",
                            hbc_rs::app::NoticeKind::Warning,
                        );
                    }
                    Ok(_) => match hbc_rs::platform::network_restart::restart() {
                        Ok(check) => {
                            network_check = Some(check);
                            startup.runtime_mut().show_notice_with_kind(
                                "Testing network. Waiting for an IP address...",
                                hbc_rs::app::NoticeKind::Information);
                        }
                        Err(error) => startup.runtime_mut().show_notice_with_kind(
                            &format!("Could not restart Wiiload before testing the connection: {error:?}."),
                            hbc_rs::app::NoticeKind::Error),
                    },
                }
                continue;
            }
            if action == hbc_rs::app::Action::ScanNetworks {
                let stopped = hbc_rs::platform::wiiload::stop();
                if let Err(error) = stopped {
                    startup.runtime_mut().show_notice_with_kind(
                        &format!("Could not stop Wiiload before scanning: {error:?}."),
                        hbc_rs::app::NoticeKind::Error,
                    );
                    continue;
                }
                let scan = hbc_rs::platform::network_scan::scan();
                let restart = hbc_rs::platform::wiiload::start();
                match (scan, restart) {
                    (Ok(networks), Ok(())) => startup.runtime_mut().set_scanned_networks(networks),
                    (Ok(networks), Err(error)) => {
                        startup.runtime_mut().set_scanned_networks(networks);
                        startup.runtime_mut().show_notice_with_kind(
                            &format!("Scan finished, but Wiiload could not restart: {error:?}."),
                            hbc_rs::app::NoticeKind::Warning,
                        );
                    }
                    (Err(error), Ok(())) => startup.runtime_mut().show_notice_with_kind(
                        &hbc_rs::platform::network_scan::ScanError::user_message(error),
                        hbc_rs::app::NoticeKind::Error,
                    ),
                    (Err(error), Err(restart_error)) => {
                        startup.runtime_mut().show_notice_with_kind(
                            &format!(
                                "{} Wiiload restart also failed: {restart_error:?}.",
                                hbc_rs::platform::network_scan::ScanError::user_message(error)
                            ),
                            hbc_rs::app::NoticeKind::Error,
                        )
                    }
                }
                continue;
            }
            if action == hbc_rs::app::Action::ApplyOptions {
                let (persist, mut notice) = match startup.apply_options() {
                    Ok(true) => (true, None),
                    Ok(false) => (
                        true,
                        Some(String::from(
                            "The selected storage device is not available.",
                        )),
                    ),
                    Err(hbc_rs::platform::startup::StartupError::CatalogScan(error)) => (
                        true,
                        Some(alloc::format!("Could not scan apps: {error:?}.")),
                    ),
                    Err(_) => (
                        false,
                        Some(String::from("Could not change the storage device.")),
                    ),
                };
                if persist
                    && startup
                        .runtime_mut()
                        .save_settings_if_dirty(hbc_rs::platform::settings::save)
                        .is_err()
                {
                    notice = Some(String::from("Could not save the new settings to NAND."));
                }
                if let Some(notice) = notice {
                    startup
                        .runtime_mut()
                        .show_notice_with_kind(&notice, hbc_rs::app::NoticeKind::Error);
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
                ogc_system::report(format_args!("HBC: reading selected executable\n"));
                let image = match startup.prepare_selected_executable() {
                    Ok(image) => image,
                    Err(error) => {
                        startup.runtime_mut().show_notice_with_kind(
                            &format!("Could not load this application: {error:?}."),
                            hbc_rs::app::NoticeKind::Error,
                        );
                        continue;
                    }
                };
                ogc_system::report(format_args!(
                    "HBC: selected executable read and validated\n"
                ));
                let ahb_access = image.ahb_access;
                if let Err(notice) = transfer_executable(
                    &mut startup,
                    image.bytes,
                    image.plan,
                    image.arguments,
                    ahb_access,
                    "Could not prepare this application to run.",
                ) {
                    startup
                        .runtime_mut()
                        .show_notice_with_kind(&notice, hbc_rs::app::NoticeKind::Error);
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
                    if startup
                        .runtime_mut()
                        .save_settings_if_dirty(hbc_rs::platform::settings::save)
                        .is_err()
                    {
                        startup.runtime_mut().show_notice_with_kind(
                            "Could not save settings. BootMii launch stopped.",
                            hbc_rs::app::NoticeKind::Error,
                        );
                        continue;
                    }
                    if startup.unmount_storage().is_err() {
                        startup.runtime_mut().show_notice_with_kind(
                            "Could not stop storage. BootMii launch stopped.",
                            hbc_rs::app::NoticeKind::Error,
                        );
                        continue;
                    }
                    if shutdown_disc_services().is_err() {
                        startup.runtime_mut().show_notice_with_kind(
                            "Could not stop disc services. BootMii launch stopped.",
                            hbc_rs::app::NoticeKind::Error,
                        );
                        continue;
                    }
                    if hbc_rs::platform::wiiload::stop().is_err() {
                        startup.runtime_mut().show_notice_with_kind(
                            "Could not stop the network receiver. BootMii launch stopped.",
                            hbc_rs::app::NoticeKind::Error,
                        );
                        continue;
                    }
                    if stop_wpad_services().is_err() {
                        startup.runtime_mut().show_notice_with_kind(
                            "Could not finish Wii Remote shutdown. BootMii launch stopped.",
                            hbc_rs::app::NoticeKind::Error,
                        );
                        continue;
                    }
                    if ogc_ios::launch_new_ios(254) < 0 {
                        // WPAD and storage are already stopped. Return to the
                        // System Menu instead of leaving HBC without input.
                        hbc_rs::platform::video::shutdown();
                        reset_system(ogc_system::SYS_RETURNTOMENU);
                    }
                    hbc_rs::platform::video::shutdown();
                    reset_system(reset);
                }
                if let Err(message) = exit_to_system(&mut startup, reset) {
                    startup
                        .runtime_mut()
                        .show_notice_with_kind(&message, hbc_rs::app::NoticeKind::Error);
                }
            }
        }
    }
}

#[cfg(all(target_arch = "powerpc", feature = "debugger-network"))]
fn cooperative_pause_loop() -> Option<hbc_rs::platform::lifecycle::ExitRequest> {
    loop {
        if let Some(request) = hbc_rs::platform::lifecycle::take_request() {
            hbc_rs::platform::wiiload::COOPERATIVE_CONTROL.service(true, false);
            return Some(request);
        }

        match hbc_rs::platform::input::scan() {
            Ok(frame)
                if frame.pressed.contains(&hbc_rs::app::Input::Home)
                    && frame.pressed.contains(&hbc_rs::app::Input::Back) =>
            {
                hbc_rs::platform::wiiload::COOPERATIVE_CONTROL.local_resume();
            }
            Ok(_) => {}
            Err(error) => ogc_system::report(format_args!(
                "HBC: paused debugger input scan failed: {error:?}\n"
            )),
        }

        hbc_rs::platform::wiiload::COOPERATIVE_CONTROL.service(false, true);
        if hbc_rs::platform::wiiload::COOPERATIVE_CONTROL.state()
            != libdb_sys::wii_network::CooperativeState::Paused
        {
            return None;
        }
        ogc_lwp::sys::sleep_ticks((ogc_lwp::sys::TICKS_PER_SECOND / 1_000).max(1));
    }
}

#[cfg(target_arch = "powerpc")]
fn install_return_stub() {
    // A DOL launched by HBC must retain that channel's configured return
    // target. The bundled stub is only needed when no loader stub exists.
    if ogc_system::sys::sys_hbc_reload_stub_present() {
        ogc_system::report(format_args!(
            "HBC: keeping the launching channel's return stub\n"
        ));
        return;
    }
    const RETURN_STUB_ADDRESS: *mut u8 = 0x8000_1800 as *mut u8;
    const RETURN_STUB: &[u8] = include_bytes!("../assets/return_stub.bin");
    if RETURN_STUB.len() > 0x1800 {
        return;
    }
    unsafe {
        core::ptr::copy_nonoverlapping(
            RETURN_STUB.as_ptr(),
            RETURN_STUB_ADDRESS,
            RETURN_STUB.len(),
        );
        ogc_cache::dc_flush_range(RETURN_STUB_ADDRESS.cast(), RETURN_STUB.len() as u32);
        ogc_cache::ic_invalidate_range(RETURN_STUB_ADDRESS.cast(), RETURN_STUB.len() as u32);
    }
}

#[cfg(target_arch = "powerpc")]
fn shutdown_disc_services() -> Result<(), i32> {
    // C WiiDVD_ShutDown closes an existing request. It does not open /dev/di
    // or issue another motor command while handing control to the next app.
    let close_result = ogc_di::close();
    if close_result < 0 {
        ogc_system::report(format_args!("HBC: DI close failed: {close_result}\n"));
        return Err(close_result);
    }
    Ok(())
}

#[cfg(target_arch = "powerpc")]
#[derive(Debug)]
enum WpadShutdownError {
    Start(i32),
    Poll(i32),
    Timeout,
}

#[cfg(target_arch = "powerpc")]
impl core::fmt::Display for WpadShutdownError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Start(code) => write!(formatter, "shutdown submission failed ({code})"),
            Self::Poll(code) => write!(formatter, "shutdown poll failed ({code})"),
            Self::Timeout => {
                formatter.write_str("Bluetooth close did not complete within 5 seconds")
            }
        }
    }
}

#[cfg(target_arch = "powerpc")]
fn stop_wpad_services() -> Result<(), WpadShutdownError> {
    let result = wpad_sys::shutdown();
    if result < 0 {
        return Err(WpadShutdownError::Start(result));
    }
    // shutdown() only submits the Bluetooth close chain. Keep polling the
    // backend until IOS reports the close completion, including the retry path
    // used after a full IPC queue prevents the first close submission.
    const SHUTDOWN_POLL_FRAMES: usize = 300;
    const SHUTDOWN_POLL_TICKS: u64 = ogc_lwp::sys::TICKS_PER_SECOND / 60;
    for _ in 0..SHUTDOWN_POLL_FRAMES {
        let scan_result = wpad_sys::scan_pads();
        if scan_result < 0 {
            return Err(WpadShutdownError::Poll(scan_result));
        }
        if wpad_sys::bluetooth_shutdown_complete() {
            return Ok(());
        }
        ogc_lwp::sys::sleep_ticks(SHUTDOWN_POLL_TICKS.max(1));
    }
    Err(WpadShutdownError::Timeout)
}

#[cfg(target_arch = "powerpc")]
fn prepare_settings_for_exit(
    startup: &mut hbc_rs::platform::startup::WiiStartup,
) -> Result<(), hbc_rs::platform::settings::SettingsIoError> {
    let outcome = startup
        .runtime_mut()
        .prepare_settings_for_exit(hbc_rs::platform::settings::save)?;
    ogc_system::report(format_args!("HBC: exit settings: {outcome:?}\n"));
    Ok(())
}

#[cfg(target_arch = "powerpc")]
fn transfer_executable(
    startup: &mut hbc_rs::platform::startup::WiiStartup,
    bytes: hbc_rs::loader::LaunchSource,
    plan: hbc_rs::loader::LoadPlan,
    arguments: Vec<u8>,
    ahb_access: bool,
    prepare_error: &str,
) -> Result<(), String> {
    ogc_system::report(format_args!("HBC: staging executable in MEM2\n"));
    let launch = hbc_rs::loader::stage_launch(
        bytes,
        &plan,
        arguments,
        hbc_rs::platform::startup::EXECUTABLE_MEMORY.clone(),
    )
    .map_err(|error| format!("{prepare_error} Loader error: {error:?}."))?;
    ogc_system::report(format_args!(
        "HBC: saving settings before executable launch\n"
    ));
    if let Err(error) = prepare_settings_for_exit(startup) {
        return Err(format!(
            "Could not save settings. Application launch stopped: {error:?}."
        ));
    }
    ogc_system::report(format_args!(
        "HBC: unmounting storage before executable launch\n"
    ));
    if let Err(error) = startup.unmount_storage() {
        return Err(format!(
            "Could not stop storage. Application launch stopped: {error:?}."
        ));
    }
    ogc_system::report(format_args!(
        "HBC: closing disc service before executable launch\n"
    ));
    shutdown_disc_services().map_err(|error| {
        format!("Could not stop disc services. Application launch stopped ({error}).")
    })?;
    ogc_system::report(format_args!(
        "HBC: stopping receiver before executable launch\n"
    ));
    hbc_rs::platform::wiiload::stop().map_err(|error| {
        format!("Could not stop the network receiver. Application launch stopped: {error:?}.")
    })?;
    ogc_system::report(format_args!(
        "HBC: stopping Bluetooth before executable launch\n"
    ));
    if let Err(error) = stop_wpad_services() {
        return Err(format!(
            "Could not finish Wii Remote shutdown. Application launch stopped: {error}."
        ));
    }
    ogc_system::report(format_args!("HBC: transferring executable control\n"));
    hbc_rs::platform::video::shutdown();
    unsafe {
        hbc_rs::loader::transfer_control(launch, hbc_rs::loader::ios_launch_policy(ahb_access))
    }
}

#[cfg(target_arch = "powerpc")]
fn exit_to_system(
    startup: &mut hbc_rs::platform::startup::WiiStartup,
    reset: u32,
) -> Result<(), String> {
    prepare_settings_for_exit(startup)
        .map_err(|error| format!("Could not save settings. System exit stopped: {error:?}."))?;
    hbc_rs::platform::lifecycle::run_exit_cleanup(|step| match step {
        hbc_rs::platform::lifecycle::ExitCleanupStep::Storage => startup
            .unmount_storage()
            .map_err(|error| format!("Could not stop storage. System exit stopped: {error:?}.")),
        hbc_rs::platform::lifecycle::ExitCleanupStep::Disc => shutdown_disc_services()
            .map_err(|error| format!("Could not stop disc services. System exit stopped ({error}).")),
        hbc_rs::platform::lifecycle::ExitCleanupStep::NetworkReceiver => {
            hbc_rs::platform::wiiload::stop().map_err(|error| {
                format!("Could not stop the network receiver. System exit stopped: {error:?}.")
            })
        }
        hbc_rs::platform::lifecycle::ExitCleanupStep::Controllers => {
            stop_wpad_services().map_err(|error| {
                ogc_system::report(format_args!(
                    "HBC: controller cleanup before system reset failed: {error}; Bluetooth: {:?}\n",
                    wpad_sys::bluetooth_diagnostics(),
                ));
                format!("Could not finish Wii Remote shutdown. System exit stopped: {error}.")
            })
        }
    })
    .map_err(|(_, error)| error)?;
    hbc_rs::platform::video::shutdown();
    // A reset that returns did not complete. Report it to the terminal caller.
    unsafe { ogc_system::reset_system(reset as i32, 0, 0) };
    Err(String::from(
        "The system reset call returned; reset completion is unconfirmed",
    ))
}

#[cfg(target_arch = "powerpc")]
fn reset_system(reset: u32) -> ! {
    unsafe { ogc_system::reset_system(reset as i32, 0, 0) };
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(target_arch = "powerpc")]
#[cfg(feature = "startup-diagnostics")]
fn reset_after_diagnostic_report() -> ! {
    hbc_rs::platform::diagnostics::recover()
}

#[cfg(target_arch = "powerpc")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    ogc_system::report(format_args!("HBC: Rust panic: {_info}\n"));
    #[cfg(feature = "startup-diagnostics")]
    if hbc_rs::platform::diagnostics::fail("RUST PANIC") {
        reset_after_diagnostic_report();
    }
    loop {
        core::hint::spin_loop();
    }
}
