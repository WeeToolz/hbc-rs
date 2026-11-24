//! Emulator acceptance entry. This module is absent from normal HBC builds.
use crate::{loader, metadata::UserSettings, platform::settings};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::{vec, vec::Vec};

fn expected_settings() -> UserSettings {
    UserSettings {
        device: 1,
        sort_order: 2,
        browse_mode: 1,
        selected_app: Some(String::from("sd:/apps/acceptance/boot.dol")),
    }
}

/// # Safety
/// The runtime must supply a valid argc/argv vector. The caller must establish
/// that this process uses an isolated emulator NAND before it enables entry.
pub unsafe fn run(argc: i32, argv: *mut *mut u8) -> i32 {
    if argc != 2 || argv.is_null() {
        return -22;
    }
    let pointer = unsafe { *argv.add(1) };
    if pointer.is_null() {
        return -22;
    }
    let case = unsafe { core::ffi::CStr::from_ptr(pointer.cast()) }.to_bytes();
    if case == b"metadata-date" {
        return metadata_date();
    }
    if case == b"metadata-update" {
        return metadata_update();
    }
    if case == b"metadata-settings" {
        return metadata_settings();
    }
    if case == b"settings-state" {
        return settings_state();
    }
    if case == b"wiiload-header" {
        return wiiload_header();
    }
    if case == b"wiiload-policy" {
        return wiiload_policy();
    }
    if case == b"wiiload-zip" {
        return wiiload_zip();
    }
    if case == b"wiiload-controller" {
        return wiiload_controller();
    }
    if case == b"runtime-state-api" {
        return runtime_state_api();
    }
    if case == b"runtime-pointer-api" {
        return runtime_pointer_api();
    }
    if case == b"runtime-measured-memo-api" {
        return runtime_measured_memo_api();
    }
    if case == b"metadata-theme" {
        return metadata_theme();
    }
    if case == b"metadata-app" {
        return metadata_app();
    }
    if case == b"catalog" {
        return catalog();
    }
    if case == b"theme-package" {
        return theme_package();
    }
    if case == b"theme-textures-api" {
        return theme_textures_api();
    }
    if case == b"theme-storage-api" {
        return theme_storage_api();
    }
    if case == b"archive-api" {
        return archive_api();
    }
    if case == b"png-api" {
        return png_api();
    }
    if case == b"management-api" {
        return management_api();
    }
    if case == b"font-api" {
        return font_api();
    }
    if case == b"localization-api" {
        return localization_api();
    }
    if case == b"overlay-control-api" {
        return overlay_control_api();
    }
    if case == b"home-overlay-app-api" {
        return home_overlay_app_api();
    }
    if case == b"network-restart-api" {
        return network_restart_api();
    }
    if case == b"input-platform-api" {
        return input_platform_api();
    }
    if case == b"app-notice-api" {
        return app_notice_api();
    }
    if case == b"about-api" {
        return about_api();
    }
    if case == b"loader-helpers" {
        return loader_helpers();
    }
    if case == b"app-controller" {
        return app_controller();
    }
    if case == b"main-menu-api" {
        return main_menu_api();
    }
    if case == b"executable-loading-api" {
        return executable_loading_api();
    }
    if case == b"detail-focus-api" {
        return detail_focus_api();
    }
    if case == b"catalog-reconcile-api" {
        return catalog_reconcile_api();
    }
    if case == b"storage-volumes" {
        return storage_volumes();
    }
    if case == b"device-availability-api" {
        return device_availability_api();
    }
    if case == b"playtime-reset-api" {
        return match settings::reset_playtime_record() {
            Ok(()) => {
                ogc_system::report(format_args!("PASS playtime reset API\n"));
                0
            }
            Err(error) => {
                ogc_system::report(format_args!("FAIL playtime reset API {error:?}\n"));
                -5
            }
        };
    }
    if case == b"app-deletion" {
        return app_deletion();
    }
    if case == b"input-policy" {
        return input_policy();
    }
    if case == b"input-mapping" {
        return input_mapping();
    }
    if case == b"render-hit-test" {
        return render_hit_test();
    }
    if case == b"disc-stop" {
        return disc_stop();
    }
    let result = match case {
        b"settings-save" => settings::save(&expected_settings()),
        b"settings-load" | b"settings-load-recovery" => settings::load().and_then(|actual| {
            if actual == expected_settings() {
                Ok(())
            } else {
                Err(settings::SettingsIoError::Codec)
            }
        }),
        b"settings-banner-error" => expect_banner_error(),
        b"settings-banner-retry" => {
            expect_banner_error().and_then(|()| settings::save(&expected_settings()))
        }
        b"settings-direct" => settings::persistence_available().and_then(|available| {
            if !available {
                Ok(())
            } else {
                Err(settings::SettingsIoError::InvalidPath)
            }
        }),
        b"launch-dol" | b"launch-elf" => return launch(case == b"launch-elf"),
        _ => return -22,
    };
    match result {
        Ok(()) => 0,
        Err(error) => {
            ogc_system::report(format_args!("FAIL acceptance {error:?}\n"));
            -5
        }
    }
}

fn storage_volumes() -> i32 {
    use crate::platform::storage::{DEVICE_COUNT, VOLUMES, Volume, device_scan_order};

    let names = [c"sd", c"usb", c"carda", c"cardb", c"usb2"];
    if DEVICE_COUNT != names.len() {
        return -1;
    }
    for (index, volume) in VOLUMES.into_iter().enumerate() {
        if Volume::from_settings_index(index as i32) != Some(volume)
            || volume.settings_index() != index as i32
            || volume.mount_name() != names[index]
        {
            ogc_system::report(format_args!(
                "FAIL storage-volumes settings index {index}\n"
            ));
            return -1;
        }
        let order = device_scan_order(index as i32);
        if order[0] != volume
            || order
                .iter()
                .filter(|candidate| **candidate == volume)
                .count()
                != 1
            || order.iter().any(|candidate| !VOLUMES.contains(candidate))
        {
            ogc_system::report(format_args!("FAIL storage-volumes scan order {index}\n"));
            return -1;
        }
    }
    if Volume::from_settings_index(-1).is_some()
        || Volume::from_settings_index(DEVICE_COUNT as i32).is_some()
        || device_scan_order(-1)[0] != Volume::Sd
        || device_scan_order(DEVICE_COUNT as i32)[0] != Volume::Sd
    {
        ogc_system::report(format_args!(
            "FAIL storage-volumes invalid preference fallback\n"
        ));
        return -1;
    }
    ogc_system::report(format_args!(
        "PASS storage-volumes indices, names and scan order\n"
    ));
    0
}

fn device_availability_api() -> i32 {
    use crate::{app::device_availability_snapshot, runtime::AppRuntime};

    let cases = [
        [false; crate::platform::storage::DEVICE_COUNT],
        [true, false, true, false, true],
        [true; crate::platform::storage::DEVICE_COUNT],
    ];
    for (case_index, detected) in cases.into_iter().enumerate() {
        let snapshot = device_availability_snapshot(detected);
        if snapshot != detected {
            ogc_system::report(format_args!(
                "FAIL device availability snapshot {case_index}\n"
            ));
            return -1;
        }

        let mut runtime = AppRuntime::default();
        runtime.set_device_availability(snapshot);
        for (device, available) in detected.into_iter().enumerate() {
            if runtime.controller().device_available(device) != Some(available)
                || runtime.controller().option_enabled(device) != available
            {
                ogc_system::report(format_args!(
                    "FAIL device availability forwarding {case_index}:{device}\n"
                ));
                return -1;
            }
        }
    }
    let mut runtime = AppRuntime::default();
    runtime.set_preferred_device(1);
    if runtime.settings().current().device != 1 || !runtime.settings().is_dirty() {
        ogc_system::report(format_args!("FAIL preferred device settings sync\n"));
        return -2;
    }
    runtime.set_active_device(Some(4));
    runtime.update(&crate::platform::input::InputFrame {
        pressed: alloc::vec![crate::app::Input::OpenOptions],
        ..Default::default()
    });
    if runtime.controller().pending_device() != 4 {
        ogc_system::report(format_args!("FAIL active device priority\n"));
        return -3;
    }
    ogc_system::report(format_args!(
        "PASS device availability, active and preferred device forwarding\n"
    ));
    0
}

fn app_deletion() -> i32 {
    use crate::{
        app::{Action, AppController, Input, PointerTarget, Screen},
        catalog::{AppEntry, AppKind},
        management::plan_app_deletion,
    };

    let Ok(plan) = plan_app_deletion("demo") else {
        ogc_system::report(format_args!("FAIL app-deletion valid folder\n"));
        return -1;
    };
    if plan.directory != "demo" {
        ogc_system::report(format_args!("FAIL app-deletion folder result\n"));
        return -1;
    }
    for name in [
        "../other",
        "demo/nested",
        ".hidden",
        "demo.",
        "demo ",
        "demo\\nested",
        "C:",
    ] {
        if plan_app_deletion(name).is_ok() {
            ogc_system::report(format_args!("FAIL app-deletion accepted unsafe name\n"));
            return -1;
        }
    }

    let catalog = [AppEntry {
        directory: String::from("demo"),
        kind: AppKind::Dol,
        size: 1,
        has_icon: false,
        metadata: None,
    }];
    let mut controller = AppController::new();
    if controller.delete_confirmation_yes()
        || controller.handle(Input::Confirm, &catalog) != Action::None
    {
        ogc_system::report(format_args!("FAIL app-deletion initial selection\n"));
        return -1;
    }
    controller.handle(Input::Left, &catalog);
    controller.handle(Input::Left, &catalog);
    controller.handle(Input::Confirm, &catalog);
    if controller.screen != Screen::DeleteConfirm || controller.delete_confirmation_yes() {
        ogc_system::report(format_args!("FAIL app-deletion default No state\n"));
        return -1;
    }
    if controller.handle(Input::Confirm, &catalog) != Action::None
        || controller.screen != Screen::AppDetails
    {
        ogc_system::report(format_args!("FAIL app-deletion No choice\n"));
        return -1;
    }
    controller.handle(Input::Confirm, &catalog);
    controller.handle(Input::Right, &catalog);
    if !controller.delete_confirmation_yes()
        || controller.handle(Input::Confirm, &catalog) != Action::DeleteSelected
        || controller.screen != Screen::Browser
    {
        ogc_system::report(format_args!("FAIL app-deletion Yes choice\n"));
        return -1;
    }

    let mut pointer_controller = AppController::new();
    pointer_controller.handle(Input::Confirm, &catalog);
    pointer_controller.handle(Input::Left, &catalog);
    pointer_controller.handle(Input::Left, &catalog);
    pointer_controller.handle(Input::Confirm, &catalog);
    if pointer_controller.handle_pointer(
        Some(PointerTarget::DeleteChoice { yes: false }),
        true,
        &catalog,
    ) != Action::None
        || pointer_controller.delete_confirmation_yes()
    {
        ogc_system::report(format_args!("FAIL app-deletion pointer No choice\n"));
        return -1;
    }
    pointer_controller.handle(Input::Confirm, &catalog);
    pointer_controller.handle(Input::Left, &catalog);
    if pointer_controller.handle_pointer(
        Some(PointerTarget::DeleteChoice { yes: true }),
        true,
        &catalog,
    ) != Action::DeleteSelected
    {
        ogc_system::report(format_args!("FAIL app-deletion pointer Yes choice\n"));
        return -1;
    }
    ogc_system::report(format_args!(
        "PASS app-deletion path safety and Yes/No confirmation API\n"
    ));
    0
}

fn input_policy() -> i32 {
    let timeout = crate::platform::input::WPAD_IDLE_TIMEOUT_SECONDS;
    if timeout != 300 {
        ogc_system::report(format_args!(
            "FAIL input-policy WPAD idle timeout {timeout}\n"
        ));
        return -1;
    }
    ogc_system::report(format_args!(
        "PASS input-policy WPAD idle timeout {timeout}\n"
    ));
    0
}

fn input_mapping() -> i32 {
    use crate::{
        app::Input,
        platform::input::{
            Extension, logical_pointer_position, map_buttons, map_extension_buttons,
        },
    };

    let all_inputs = [
        Input::Home,
        Input::Up,
        Input::Down,
        Input::Left,
        Input::Right,
        Input::Confirm,
        Input::Back,
        Input::ToggleBrowseMode,
        Input::OpenOptions,
        Input::PreviousPage,
        Input::NextPage,
    ];
    let pad = map_buttons(
        0x0001
            | 0x0002
            | 0x0004
            | 0x0008
            | 0x0010
            | 0x0020
            | 0x0040
            | 0x0100
            | 0x0200
            | 0x0400
            | 0x0800
            | 0x1000,
        0,
    );
    let remote = map_buttons(
        0,
        0x0001
            | 0x0002
            | 0x0004
            | 0x0008
            | 0x0010
            | 0x0080
            | 0x0100
            | 0x0200
            | 0x0400
            | 0x0800
            | 0x1000,
    );
    let classic = map_extension_buttons(
        (0x0001
            | 0x0002
            | 0x0008
            | 0x0010
            | 0x0020
            | 0x0040
            | 0x0080
            | 0x0200
            | 0x0400
            | 0x0800
            | 0x1000
            | 0x2000
            | 0x4000
            | 0x8000)
            << 16,
        Extension::Classic,
    );
    if all_inputs.iter().any(|input| !pad.contains(*input))
        || all_inputs.iter().any(|input| !remote.contains(*input))
        || all_inputs
            .iter()
            .any(|input| !map_buttons(0, classic).contains(*input))
        || logical_pointer_position(64.0, 64.0)
            != Some(crate::platform::input::PointerPosition { x: 0, y: 0 })
        || logical_pointer_position(384.0, 304.0)
            != Some(crate::platform::input::PointerPosition { x: 320, y: 240 })
        || logical_pointer_position(f32::NAN, 64.0).is_some()
    {
        ogc_system::report(format_args!(
            "FAIL input-mapping button or pointer values\n"
        ));
        return -1;
    }
    ogc_system::report(format_args!(
        "PASS input-mapping PAD, WPAD, Classic and IR\n"
    ));
    0
}

fn render_hit_test() -> i32 {
    use crate::{
        app::{
            Action, AppController, Input, OPTION_BACK, OPTION_OK, OPTION_SORT_DATE, PointerTarget,
        },
        catalog::{AppEntry, AppKind},
        render::{Frame, Rect, browser_frame},
    };

    let mut frame = Frame::default();
    frame.hit_target(
        PointerTarget::App(0),
        Rect {
            x: 10.0,
            y: 20.0,
            width: 30.0,
            height: 40.0,
        },
    );
    frame.hit_target(
        PointerTarget::Menu(1),
        Rect {
            x: 20.0,
            y: 30.0,
            width: 30.0,
            height: 40.0,
        },
    );
    if frame.hit_test(20.0, 30.0) != Some(PointerTarget::Menu(1))
        || frame.hit_test(50.0, 70.0) != Some(PointerTarget::Menu(1))
        || frame.hit_test(50.1, 70.0).is_some()
        || frame.hit_test(f32::NAN, 40.0).is_some()
    {
        ogc_system::report(format_args!("FAIL render-hit-test overlap and bounds\n"));
        return -1;
    }

    let catalog = [AppEntry {
        directory: String::from("demo"),
        kind: AppKind::Dol,
        size: 1,
        has_icon: false,
        metadata: None,
    }];
    let rendered = browser_frame(&AppController::new(), &catalog);
    let Some(target) = rendered
        .hit_targets
        .iter()
        .find(|hit| hit.target == PointerTarget::App(0))
    else {
        ogc_system::report(format_args!("FAIL render-hit-test browser app target\n"));
        return -1;
    };
    let center_x = target.bounds.x + target.bounds.width / 2.0;
    let center_y = target.bounds.y + target.bounds.height / 2.0;
    if rendered.hit_test(center_x, center_y) != Some(PointerTarget::App(0)) {
        ogc_system::report(format_args!(
            "FAIL render-hit-test rendered browser target\n"
        ));
        return -1;
    }

    let mut options = AppController::new();
    options.set_device_availability([true; crate::platform::storage::DEVICE_COUNT]);
    options.set_preferred_device(2);
    options.set_active_device(Some(0));
    options.handle(Input::OpenOptions, &catalog);
    if options.device_available(4) != Some(true)
        || options.options_focus() != OPTION_BACK
        || options.pending_device() != 0
        || options.preferred_device() != 2
        || options.pending_sort() != crate::catalog::SortOrder::Name
    {
        ogc_system::report(format_args!("FAIL render-hit-test initial device state\n"));
        return -1;
    }
    let options_frame = browser_frame(&options, &catalog);
    let Some(usb2) = options_frame
        .hit_targets
        .iter()
        .find(|hit| hit.target == PointerTarget::Option(4))
    else {
        ogc_system::report(format_args!(
            "FAIL render-hit-test available USB 2 option\n"
        ));
        return -1;
    };
    let usb2_target = options_frame.hit_test(
        usb2.bounds.x + usb2.bounds.width / 2.0,
        usb2.bounds.y + usb2.bounds.height / 2.0,
    );
    if usb2_target != Some(PointerTarget::Option(4))
        || !options.option_enabled(4)
        || options.handle_pointer(usb2_target, true, &catalog) != Action::None
        || options.options_focus() != 4
        || options.pending_device() != 4
        || options.preferred_device() != 2
        || options.handle_pointer(
            Some(PointerTarget::Option(OPTION_SORT_DATE)),
            true,
            &catalog,
        ) != Action::None
        || options.options_focus() != OPTION_SORT_DATE
        || options.pending_sort() != crate::catalog::SortOrder::ReleaseDate
        || options.handle_pointer(Some(PointerTarget::Option(OPTION_OK)), true, &catalog)
            != Action::ApplyOptions
        || options.preferred_device() != 4
    {
        ogc_system::report(format_args!(
            "FAIL render-hit-test USB 2 selection and apply\n"
        ));
        return -1;
    }
    let mut unavailable = AppController::new();
    unavailable.set_device_availability([true, false, false, false, false]);
    unavailable.handle(Input::OpenOptions, &catalog);
    if unavailable.device_available(4) != Some(false)
        || unavailable.option_enabled(4)
        || unavailable.handle_pointer(Some(PointerTarget::Option(4)), true, &catalog)
            != Action::None
        || unavailable.pending_device() != 0
        || unavailable.preferred_device() != 0
    {
        ogc_system::report(format_args!(
            "FAIL render-hit-test unavailable device state\n"
        ));
        return -1;
    }
    ogc_system::report(format_args!(
        "PASS render-hit-test ordering, bounds, browser and device targets\n"
    ));
    0
}

fn disc_stop() -> i32 {
    if let Err(error) = crate::platform::disc::start_stop() {
        ogc_system::report(format_args!("FAIL disc-stop submit {error}\n"));
        return -5;
    }
    let service_result = match crate::platform::disc::shutdown() {
        Ok(result) => result,
        Err(error) => {
            ogc_system::report(format_args!("FAIL disc-stop drain {error}\n"));
            return -5;
        }
    };
    let (callbacks, open_result, stop_result) = crate::platform::disc::acceptance_report();
    ogc_system::report(format_args!(
        "HBC_DISC callbacks={callbacks} open={open_result} stop={stop_result} result={service_result}\n"
    ));
    if callbacks & 1 == 0 {
        return -5;
    }
    if open_result < 0 {
        ogc_system::report(format_args!("PASS disc-stop open-error-path\n"));
        if callbacks != 1 || service_result != open_result {
            return -5;
        }
        return (0x4400_0000 | (callbacks << 16)) as i32;
    }
    if callbacks != 7 || stop_result < 0 || service_result != 0 {
        ogc_system::report(format_args!("FAIL disc-stop callback-chain\n"));
        return -5;
    }
    ogc_system::report(format_args!("PASS disc-stop callback-chain\n"));
    (0x4400_0000 | (callbacks << 16) | 0x101) as i32
}

fn metadata_date() -> i32 {
    let cases = [
        ("20241332", Some(20250304010000)),
        ("20250000", Some(20241231010000)),
        ("20250101999999", Some(20250205044039)),
        ("2025-01-02", Some(20250101010000)),
        ("20250101-1-1-1", Some(20250131225859)),
        ("19691201000000", None),
        ("-0010101", Some(-9_798_990_000)),
        ("2025010112", Some(20250201010000)),
        ("2025 1 1", Some(20250201010000)),
        ("2025x0101", None),
        ("20250101123456tail", Some(20250201123456)),
        ("202501011234", Some(20250201010000)),
        ("20240131", Some(20240302010000)),
        ("20250131", Some(20250303010000)),
        ("20241231", Some(20250131010000)),
        ("20250131235960", Some(20250304000000)),
        ("20250102", Some(20250202010000)),
    ];
    for (index, (input, expected)) in cases.iter().enumerate() {
        let xml = alloc::format!("<app><release_date>{input}</release_date></app>");
        let actual = crate::metadata::parse_app_xml(&xml).map(|entry| entry.release_date);
        if actual != Ok(*expected) {
            ogc_system::report(format_args!("FAIL metadata-date {index}: {actual:?}\n"));
            return -1;
        }
        ogc_system::report(format_args!("PASS metadata-date {index}\n"));
    }
    0
}

fn metadata_update() -> i32 {
    let cases = [
        (Some("123tail"), Some(123)),
        (Some("  +456rest"), Some(456)),
        (Some("-7tail"), Some(u64::MAX - 6)),
        (Some("junk"), None),
        (Some("0"), Some(0)),
        (Some(""), None),
        (Some("\u{a0}123"), None),
        (None, None),
    ];
    for (index, (input, expected)) in cases.iter().enumerate() {
        let xml = match input {
            Some(input) => alloc::format!("<update><app><date>{input}</date></app></update>"),
            None => String::from("<update><app></app></update>"),
        };
        let actual = crate::metadata::parse_update_xml(&xml).map(|entry| entry.date);
        if actual != Ok(*expected) {
            ogc_system::report(format_args!("FAIL metadata-update {index}: {actual:?}\n"));
            return -1;
        }
    }
    0
}

fn metadata_settings() -> i32 {
    let cases = [
        (
            "<hbc><device>  +12tail</device><sort_order>-3.5</sort_order><browse_mode>words</browse_mode></hbc>",
            12,
            -3,
            0,
        ),
        ("<hbc/>", -1, 0, 0),
        ("<hbc><device>\u{a0}4</device></hbc>", 0, 0, 0),
    ];
    for (index, (xml, device, sort_order, browse_mode)) in cases.iter().enumerate() {
        let actual = crate::metadata::parse_settings_xml(xml);
        if !matches!(actual, Ok(ref settings) if (settings.device, settings.sort_order, settings.browse_mode) == (*device, *sort_order, *browse_mode))
        {
            ogc_system::report(format_args!("FAIL metadata-settings {index}: {actual:?}\n"));
            return -1;
        }
        ogc_system::report(format_args!("PASS metadata-settings {index}\n"));
    }
    0
}

fn settings_state() -> i32 {
    use crate::settings::{SettingsInitializationError, decode_settings_xml, encode_settings_xml};

    let expected_xml = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><hbc><device>1</device><sort_order>2</sort_order><browse_mode>1</browse_mode><app_sel>sd:/apps/acceptance/boot.dol</app_sel></hbc>";
    let expected = expected_settings();
    if encode_settings_xml(&expected).as_deref().map(str::as_bytes) != Ok(expected_xml)
        || decode_settings_xml(expected_xml) != Ok(expected.clone())
    {
        ogc_system::report(format_args!("FAIL settings-state XML codec\n"));
        return -1;
    }

    let defaults = UserSettings::default();
    let loaded = crate::settings::load_or_create(
        defaults.clone(),
        || Ok::<_, u8>(expected.clone()),
        |_| false,
        |_| Err(1),
    );
    if loaded != Ok(expected.clone()) {
        ogc_system::report(format_args!("FAIL settings-state load existing\n"));
        return -1;
    }
    let mut created = false;
    let missing = crate::settings::load_or_create(
        defaults.clone(),
        || Err::<UserSettings, _>(0u8),
        |error| *error == 0,
        |settings| {
            created = settings == &defaults;
            Ok::<_, u8>(())
        },
    );
    if missing != Ok(defaults.clone()) || !created {
        ogc_system::report(format_args!("FAIL settings-state create missing\n"));
        return -1;
    }
    let unreadable = crate::settings::load_or_create(
        defaults,
        || Err::<UserSettings, _>(1u8),
        |_| false,
        |_| panic!("save must not run after a read error"),
    );
    if unreadable != Err(SettingsInitializationError::Load(1)) {
        ogc_system::report(format_args!("FAIL settings-state preserve read error\n"));
        return -1;
    }

    let mut state = crate::settings::SettingsState::default();
    if state.current() != &UserSettings::default() || state.is_dirty() {
        ogc_system::report(format_args!("FAIL settings-state defaults\n"));
        return -1;
    }
    state.current_mut().device = 1;
    if state.current().device != 1 || !state.is_dirty() {
        ogc_system::report(format_args!("FAIL settings-state edit tracking\n"));
        return -1;
    }
    let mut saved = false;
    if state.save_if_dirty(|settings| {
        saved = settings.device == 1;
        Ok::<(), ()>(())
    }) != Ok(true)
        || !saved
        || state.is_dirty()
    {
        ogc_system::report(format_args!("FAIL settings-state successful save\n"));
        return -1;
    }
    let mut save_called = false;
    if state.save_if_dirty(|_| {
        save_called = true;
        Ok::<(), ()>(())
    }) != Ok(false)
        || save_called
    {
        ogc_system::report(format_args!("FAIL settings-state unchanged save\n"));
        return -1;
    }
    state.current_mut().sort_order = 2;
    if state.save_if_dirty(|_| Err("write")) != Err("write") || !state.is_dirty() {
        ogc_system::report(format_args!("FAIL settings-state failed save tracking\n"));
        return -1;
    }
    state.mark_persisted();
    if state.is_dirty() {
        ogc_system::report(format_args!("FAIL settings-state mark persisted\n"));
        return -1;
    }
    let loaded = expected_settings();
    state.load(loaded.clone());
    if state.current() != &loaded || state.is_dirty() {
        ogc_system::report(format_args!("FAIL settings-state load\n"));
        return -1;
    }
    let catalog = alloc::vec![crate::catalog::AppEntry {
        directory: String::from("selected"),
        kind: crate::catalog::AppKind::Dol,
        size: 1,
        has_icon: false,
        metadata: Some(crate::metadata::AppMetadata::default()),
    }];
    let mut runtime = crate::runtime::AppRuntime::new(catalog);
    runtime.load_settings(UserSettings {
        device: 4,
        sort_order: 1,
        browse_mode: 1,
        selected_app: Some(String::from("selected")),
    });
    if runtime.catalog().len() != 1
        || runtime.controller().preferred_device() != 4
        || runtime.controller().sort != crate::catalog::SortOrder::ReleaseDate
        || runtime.controller().filter != crate::catalog::Filter::MetadataOnly
        || runtime.controller().browser_layout().columns != 3
        || runtime.controller().selected_app.as_deref() != Some("selected")
        || runtime.settings().is_dirty()
    {
        ogc_system::report(format_args!(
            "FAIL settings-state runtime load forwarding\n"
        ));
        return -2;
    }
    ogc_system::report(format_args!("PASS settings-state dirty, save and load\n"));
    0
}

fn wiiload_header() -> i32 {
    use crate::wiiload::{WIILOAD_MAX_ARGS, WIILOAD_MAX_DATA, WiiloadError, WiiloadHeader};

    let wire_header = [b'H', b'A', b'X', b'X', 0, 5, 0, 3, 0, 0, 0, 4, 0, 0, 0, 0];
    let header = match WiiloadHeader::decode(&wire_header) {
        Ok(header) => header,
        Err(error) => {
            ogc_system::report(format_args!("FAIL wiiload-header decode {error:?}\n"));
            return -1;
        }
    };
    if header.version != 5
        || header.args_len != 3
        || header.data_len != 4
        || header.validate() != Ok(())
        || header.body_len() != Ok(7)
        || header.is_compressed()
        || header.decode_body(b"ELF!arg")
            != Ok(crate::wiiload::WiiloadPayload {
                data: b"ELF!".to_vec(),
                args: b"arg".to_vec(),
            })
    {
        ogc_system::report(format_args!("FAIL wiiload-header decoded fields or body\n"));
        return -1;
    }
    let mut bad_magic = wire_header;
    bad_magic[0] = b'X';
    if WiiloadHeader::decode(&bad_magic) != Err(WiiloadError::InvalidHeader)
        || WiiloadHeader::decode(&wire_header[..15]) != Err(WiiloadError::InvalidHeader)
        || WiiloadHeader::decode(&[b'H', b'A', b'X', b'X', 0, 4, 0, 3, 0, 0, 0, 4, 0, 0, 0, 0])
            != Err(WiiloadError::UnsupportedVersion)
        || header.decode_body(b"ELF!argx") != Err(WiiloadError::IncompleteBody)
    {
        ogc_system::report(format_args!("FAIL wiiload-header rejection bounds\n"));
        return -1;
    }
    let compressed = [
        0x78, 0x9c, 0x4b, 0x54, 0x28, 0xce, 0x4d, 0xcc, 0xc9, 0x51, 0x08, 0xcf, 0xcc, 0x54, 0x48,
        0xad, 0x48, 0x4d, 0x2e, 0x2d, 0x49, 0x4c, 0xca, 0x49, 0x55, 0x28, 0x48, 0xac, 0xcc, 0xc9,
        0x4f, 0x4c, 0x01, 0x00, 0xa8, 0x16, 0x0b, 0x30,
    ];
    let compressed_header =
        WiiloadHeader::decode(&[b'H', b'A', b'X', b'X', 0, 5, 0, 2, 0, 0, 0, 38, 0, 0, 0, 30]);
    let Ok(compressed_header) = compressed_header else {
        ogc_system::report(format_args!("FAIL wiiload-header compressed header\n"));
        return -1;
    };
    let mut compressed_body = compressed.to_vec();
    compressed_body.extend_from_slice(b"xy");
    if !compressed_header.is_compressed()
        || compressed_header.body_len() != Ok(40)
        || compressed_header.decode_body(&compressed_body)
            != Ok(crate::wiiload::WiiloadPayload {
                data: b"a small Wii executable payload".to_vec(),
                args: b"xy".to_vec(),
            })
    {
        ogc_system::report(format_args!("FAIL wiiload-header compressed body\n"));
        return -1;
    }
    let large_payload: Vec<u8> = (0..96 * 1024)
        .scan(0x1234_5678u32, |state, _| {
            *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            Some((*state >> 24) as u8)
        })
        .collect();
    let large_compressed = miniz_oxide::deflate::compress_to_vec_zlib(&large_payload, 6);
    let mut large_wire_header = [b'H', b'A', b'X', b'X', 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    large_wire_header[8..12].copy_from_slice(&(large_compressed.len() as u32).to_be_bytes());
    large_wire_header[12..16].copy_from_slice(&(large_payload.len() as u32).to_be_bytes());
    let large_header = WiiloadHeader::decode(&large_wire_header);
    if large_compressed.len() <= 8 * 1024
        || !matches!(large_header, Ok(header)
        if header.decode_body(&large_compressed) == Ok(crate::wiiload::WiiloadPayload {
            data: large_payload.clone(),
            args: Vec::new(),
        }))
    {
        ogc_system::report(format_args!("FAIL wiiload-header multi-chunk zlib body\n"));
        return -1;
    }
    let wrong_size =
        WiiloadHeader::decode(&[b'H', b'A', b'X', b'X', 0, 5, 0, 0, 0, 0, 0, 38, 0, 0, 0, 31]);
    if !matches!(wrong_size, Ok(header) if header.decode_body(&compressed) == Err(WiiloadError::DecompressedSizeMismatch))
    {
        ogc_system::report(format_args!(
            "FAIL wiiload-header decompressed size check\n"
        ));
        return -1;
    }
    let max_data = WIILOAD_MAX_DATA as u32;
    let mut max_header = [b'H', b'A', b'X', b'X', 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    max_header[6..8].copy_from_slice(&(WIILOAD_MAX_ARGS as u16).to_be_bytes());
    max_header[8..12].copy_from_slice(&max_data.to_be_bytes());
    max_header[12..16].copy_from_slice(&max_data.to_be_bytes());
    let expected_body_len = WIILOAD_MAX_DATA + WIILOAD_MAX_ARGS;
    if !matches!(WiiloadHeader::decode(&max_header), Ok(header)
        if header.validate() == Ok(()) && header.body_len() == Ok(expected_body_len))
    {
        ogc_system::report(format_args!(
            "FAIL wiiload-header maximum accepted bounds\n"
        ));
        return -1;
    }
    let mut too_much_data = max_header;
    too_much_data[8..12].copy_from_slice(&(max_data + 1).to_be_bytes());
    let mut too_much_output = max_header;
    too_much_output[12..16].copy_from_slice(&(max_data + 1).to_be_bytes());
    let mut too_many_args = max_header;
    too_many_args[6..8].copy_from_slice(&((WIILOAD_MAX_ARGS as u16) + 1).to_be_bytes());
    if WiiloadHeader::decode(&too_much_data) != Err(WiiloadError::PayloadTooLarge)
        || WiiloadHeader::decode(&too_much_output) != Err(WiiloadError::PayloadTooLarge)
        || WiiloadHeader::decode(&too_many_args) != Err(WiiloadError::ArgumentsTooLarge)
    {
        ogc_system::report(format_args!(
            "FAIL wiiload-header maximum rejected bounds\n"
        ));
        return -1;
    }
    ogc_system::report(format_args!(
        "PASS wiiload-header decode, body bounds and multi-chunk zlib\n"
    ));
    0
}

fn wiiload_policy() -> i32 {
    use crate::wiiload::{
        ReceiverStatus, client_deadline_expired, indicator_state, network_indicator_visible,
        progress_percent, same_local_subnet_16, socket_would_block,
    };

    let local = u32::from_be_bytes([192, 168, 4, 2]);
    if !same_local_subnet_16(u32::from_be_bytes([192, 168, 4, 200]), local)
        || same_local_subnet_16(u32::from_be_bytes([192, 169, 4, 2]), local)
        || progress_percent(50, 100) != 50
        || progress_percent(101, 100) != 100
        || progress_percent(1, 0) != 0
        || !indicator_state(ReceiverStatus::Listening, true).network_active
        || !indicator_state(ReceiverStatus::Listening, true).gecko_active
        || indicator_state(ReceiverStatus::Stopped, false).network_active
        || network_indicator_visible(ReceiverStatus::Starting, 19)
        || !network_indicator_visible(ReceiverStatus::Starting, 20)
        || !network_indicator_visible(ReceiverStatus::Listening, 0)
        || network_indicator_visible(ReceiverStatus::Stopped, 20)
        || !client_deadline_expired(100, 110, 10)
        || client_deadline_expired(100, 109, 10)
        || !client_deadline_expired(u64::MAX - 5, 4, 10)
        || !socket_would_block(-6)
        || socket_would_block(-11)
        || socket_would_block(0)
    {
        ogc_system::report(format_args!("FAIL wiiload-policy outputs\n"));
        return -1;
    }
    ogc_system::report(format_args!("PASS wiiload-policy helpers\n"));
    0
}

fn wiiload_zip() -> i32 {
    use crate::{
        management::ExtractionOperation,
        wiiload_zip::{WiiloadZipClassification, WiiloadZipKind, classify_received_zip},
    };

    let app = include_bytes!("../tests/fixtures/wiiload-app.zip");
    let classification = match classify_received_zip(app) {
        Ok(classification) => classification,
        Err(error) => {
            ogc_system::report(format_args!(
                "FAIL wiiload-zip app classification {error:?}\n"
            ));
            return -1;
        }
    };
    let WiiloadZipClassification::App {
        directory,
        kind,
        extraction,
    } = classification
    else {
        ogc_system::report(format_args!("FAIL wiiload-zip app branch\n"));
        return -1;
    };
    if directory != "Demo"
        || kind != WiiloadZipKind::Elf
        || extraction.total_bytes() != 10
        || extraction.operations().len() != 2
        || !extraction.operations().iter().any(|operation| {
            matches!(
                operation,
                ExtractionOperation::WriteFile { path, archive_name, size: 4 }
                    if path == "boot.elf" && archive_name == "Demo/boot.elf"
            )
        })
        || !extraction.operations().iter().any(|operation| {
            matches!(
                operation,
                ExtractionOperation::WriteFile { path, archive_name, size: 6 }
                    if path == "meta.xml" && archive_name == "Demo/meta.xml"
            )
        })
    {
        ogc_system::report(format_args!("FAIL wiiload-zip app extraction plan\n"));
        return -1;
    }

    let theme = include_bytes!("../tests/fixtures/theme-valid.zip");
    if !matches!(
        classify_received_zip(theme),
        Ok(WiiloadZipClassification::Theme { package })
            if package.metadata.description.as_deref() == Some("Dolphin")
    ) || classify_received_zip(b"not a zip").is_ok()
    {
        ogc_system::report(format_args!("FAIL wiiload-zip theme or invalid branch\n"));
        return -1;
    }
    ogc_system::report(format_args!(
        "PASS wiiload-zip app, theme and invalid inputs\n"
    ));
    0
}

fn wiiload_controller() -> i32 {
    use crate::{
        app::{Action, Input, Screen},
        runtime::AppRuntime,
    };

    let mut runtime = AppRuntime::default();
    runtime.request_wiiload_zip_confirmation();
    if runtime.controller().screen != Screen::WiiloadConfirm
        || runtime.controller().wiiload_confirmation_text() != "Extract the received ZIP file?"
        || runtime.controller().wiiload_confirmation_scroll() != 0
        || runtime.controller().wiiload_confirmation_max_scroll() != 0
        || runtime.controller().wiiload_confirmation_yes()
    {
        ogc_system::report(format_args!(
            "FAIL wiiload-controller initial confirmation\n"
        ));
        return -1;
    }
    let details = "ZIP detail ".repeat(240);
    runtime.set_wiiload_confirmation_text(details.clone());
    if runtime.controller().wiiload_confirmation_text() != details
        || runtime.controller().wiiload_confirmation_max_scroll() == 0
        || runtime.controller().wiiload_confirmation_scroll() != 0
    {
        ogc_system::report(format_args!("FAIL wiiload-controller confirmation text\n"));
        return -1;
    }
    runtime.update(&crate::platform::input::InputFrame {
        pressed: vec![Input::Down],
        ..Default::default()
    });
    runtime.update(&crate::platform::input::InputFrame {
        pressed: vec![Input::Left],
        ..Default::default()
    });
    if runtime.controller().wiiload_confirmation_scroll() != 1
        || !runtime.controller().wiiload_confirmation_yes()
        || runtime.update(&crate::platform::input::InputFrame {
            pressed: vec![Input::Confirm],
            ..Default::default()
        }) != [Action::AcceptWiiloadZip]
        || runtime.controller().screen != Screen::Browser
    {
        ogc_system::report(format_args!("FAIL wiiload-controller accept ZIP\n"));
        return -1;
    }

    runtime.show_wiiload_receiving();
    if runtime.controller().screen != Screen::WiiloadReceiving
        || runtime.operation_progress() != 0
        || runtime.controller().wiiload_install_progress() != 0
    {
        ogc_system::report(format_args!("FAIL wiiload-controller receive start\n"));
        return -1;
    }
    runtime.set_wiiload_install_progress(35);
    if runtime.controller().wiiload_install_progress() != 35 || runtime.operation_progress() != 35 {
        ogc_system::report(format_args!("FAIL wiiload-controller receive progress\n"));
        return -1;
    }
    runtime.set_operation_progress(255);
    if runtime.operation_progress() != 100 {
        ogc_system::report(format_args!("FAIL wiiload-controller bounded progress\n"));
        return -1;
    }
    runtime.finish_wiiload_receiving();
    runtime.set_wiiload_install_progress(22);
    if runtime.controller().screen != Screen::Browser
        || runtime.controller().wiiload_install_progress() != 100
    {
        ogc_system::report(format_args!("FAIL wiiload-controller receive finish\n"));
        return -1;
    }

    runtime.show_wiiload_installing();
    runtime.set_wiiload_install_progress(48);
    runtime.finish_wiiload_installing();
    if runtime.controller().screen != Screen::Browser
        || runtime.controller().wiiload_install_progress() != 48
    {
        ogc_system::report(format_args!("FAIL wiiload-controller install finish\n"));
        return -1;
    }
    runtime.show_wiiload_receiving();
    runtime.finish_wiiload_receiving();
    if runtime.controller().screen != Screen::Browser {
        ogc_system::report(format_args!("FAIL wiiload-controller repeat receive\n"));
        return -1;
    }
    ogc_system::report(format_args!(
        "PASS wiiload-controller confirmation and progress\n"
    ));
    0
}

fn runtime_state_api() -> i32 {
    use crate::{
        app::{NoticeKind, Screen},
        catalog::{AppEntry, AppKind},
        metadata::{ThemeMetadata, UserSettings},
        runtime::{AppRuntime, ExitSettingsOutcome, SettingsSaveError},
    };

    let make_entry = |directory: &str| AppEntry {
        directory: String::from(directory),
        kind: AppKind::Dol,
        size: 1,
        has_icon: false,
        metadata: None,
    };
    let mut runtime = AppRuntime::new(alloc::vec![make_entry("first"), make_entry("selected")]);
    runtime.load_settings(UserSettings {
        selected_app: Some(String::from("selected")),
        ..UserSettings::default()
    });
    runtime.replace_catalog(alloc::vec![make_entry("selected"), make_entry("new")]);
    if runtime.catalog().len() != 2
        || runtime.controller().selected_app.as_deref() != Some("selected")
    {
        ogc_system::report(format_args!("FAIL runtime catalog replacement selection\n"));
        return -1;
    }

    runtime.set_widescreen(true);
    runtime.update(&crate::platform::input::InputFrame {
        pressed: alloc::vec![crate::app::Input::ToggleBrowseMode],
        ..Default::default()
    });
    if runtime.controller().browser_layout().columns != 4 {
        ogc_system::report(format_args!("FAIL runtime widescreen columns\n"));
        return -2;
    }
    runtime.set_widescreen(false);
    if runtime.controller().browser_layout().columns != 3 {
        ogc_system::report(format_args!("FAIL runtime standard columns\n"));
        return -3;
    }

    let theme = ThemeMetadata {
        description: Some(String::from("Runtime theme description")),
        ..ThemeMetadata::default()
    };
    runtime.apply_theme(theme.clone());
    if runtime.theme() != Some(&theme) {
        ogc_system::report(format_args!("FAIL runtime theme application\n"));
        return -4;
    }

    runtime.show_notice("Storage unavailable");
    if runtime.controller().screen != Screen::Notice
        || runtime.controller().notice() != Some("Storage unavailable")
        || runtime.controller().notice_kind() != NoticeKind::Generic
    {
        ogc_system::report(format_args!("FAIL runtime generic notice\n"));
        return -5;
    }
    runtime.show_notice_with_kind("NAND write failed", NoticeKind::Error);
    if runtime.controller().notice() != Some("NAND write failed")
        || runtime.controller().notice_kind() != NoticeKind::Error
        || !runtime.controller().timed_notice_active()
    {
        ogc_system::report(format_args!("FAIL runtime error notice\n"));
        return -6;
    }

    runtime.set_preferred_device(2);
    if runtime.save_settings_if_dirty(|settings| {
        if settings.device == 2 {
            Err(-22)
        } else {
            Ok(())
        }
    }) != Err(SettingsSaveError::Backend(-22))
        || !runtime.settings().is_dirty()
    {
        ogc_system::report(format_args!("FAIL runtime settings save error\n"));
        return -7;
    }
    let mut saved_device = None;
    if runtime.save_settings_if_dirty(|settings| {
        saved_device = Some(settings.device);
        Ok::<(), i32>(())
    }) != Ok(true)
        || saved_device != Some(2)
        || runtime.settings().is_dirty()
    {
        ogc_system::report(format_args!("FAIL runtime settings save success\n"));
        return -8;
    }

    runtime.set_preferred_device(3);
    runtime.set_settings_persistence_enabled(false);
    let mut save_called = false;
    if runtime.prepare_settings_for_exit(|_| {
        save_called = true;
        Ok::<(), i32>(())
    }) != Ok(ExitSettingsOutcome::PreservedAfterLoadFailure)
        || save_called
        || !runtime.settings().is_dirty()
    {
        ogc_system::report(format_args!("FAIL runtime preserve unreadable settings\n"));
        return -9;
    }

    ogc_system::report(format_args!(
        "PASS runtime catalog, display, theme, notice and settings APIs\n"
    ));
    0
}

fn runtime_pointer_api() -> i32 {
    use crate::{
        app::{Input, PointerTarget, Screen},
        catalog::{AppEntry, AppKind},
        platform::input::InputFrame,
        runtime::AppRuntime,
    };

    let catalog = (0..10)
        .map(|index| AppEntry {
            directory: alloc::format!("pointer-{index}"),
            kind: AppKind::Dol,
            size: 1,
            has_icon: false,
            metadata: None,
        })
        .collect();
    let mut runtime = AppRuntime::new(catalog);

    if !runtime
        .update_pointer(Some(PointerTarget::App(2)), false)
        .is_empty()
        || runtime.controller().selected_app.as_deref() != Some("pointer-2")
        || runtime.controller().screen != Screen::Browser
    {
        ogc_system::report(format_args!("FAIL runtime pointer focus\n"));
        return -1;
    }

    if !runtime
        .update_pointer_frame(Some(PointerTarget::App(3)), Some(120), true, false)
        .is_empty()
        || runtime.controller().selected_app.as_deref() != Some("pointer-3")
        || runtime.controller().screen != Screen::AppDetails
    {
        ogc_system::report(format_args!("FAIL runtime pointer frame activation\n"));
        return -2;
    }

    runtime.update(&InputFrame {
        pressed: alloc::vec![Input::Back],
        ..Default::default()
    });
    let mut frame = InputFrame {
        pressed: alloc::vec![Input::NextPage, Input::Confirm],
        held: alloc::vec![Input::NextPage],
        ..Default::default()
    };
    if !runtime
        .update_frame_with_pointer(&mut frame, Some(PointerTarget::App(0)), None, true, false)
        .is_empty()
        || runtime.controller().selected_app.as_deref() != Some("pointer-0")
        || runtime.controller().screen != Screen::AppDetails
    {
        ogc_system::report(format_args!("FAIL runtime combined pointer frame\n"));
        return -3;
    }

    ogc_system::report(format_args!(
        "PASS runtime pointer focus, activation and frame ordering\n"
    ));
    0
}

fn runtime_measured_memo_api() -> i32 {
    use crate::{app::Screen, platform::input::InputFrame, runtime::AppRuntime};

    let mut runtime = AppRuntime::default();
    runtime.show_notice("Measured memo");
    runtime.set_measured_memo(2, 8);
    if runtime.controller().notice_scroll_max() != 2 {
        ogc_system::report(format_args!("FAIL runtime measured memo scroll bound\n"));
        return -1;
    }

    runtime.update(&InputFrame {
        stick_scroll_pixels: 7,
        ..Default::default()
    });
    if runtime.controller().screen != Screen::Notice || runtime.controller().notice_scroll() != 0 {
        ogc_system::report(format_args!("FAIL runtime measured memo partial line\n"));
        return -2;
    }
    runtime.update(&InputFrame {
        stick_scroll_pixels: 1,
        ..Default::default()
    });
    if runtime.controller().notice_scroll() != 1 {
        ogc_system::report(format_args!("FAIL runtime measured memo line height\n"));
        return -3;
    }
    runtime.update(&InputFrame {
        stick_scroll_pixels: 32,
        ..Default::default()
    });
    runtime.update(&InputFrame {
        stick_scroll_pixels: 8,
        ..Default::default()
    });
    if runtime.controller().notice_scroll() != 2 {
        ogc_system::report(format_args!("FAIL runtime measured memo clamp\n"));
        return -4;
    }

    ogc_system::report(format_args!(
        "PASS runtime measured memo line height and scroll bound\n"
    ));
    0
}

fn metadata_theme() -> i32 {
    let xml = "<theme><font_color><red>  +4tail</red><green>5</green><blue>6</blue><alpha>255</alpha></font_color><font target='appname'><size>12px</size></font><font target='appname'><color><red>255tail</red><green>0</green><blue>7x</blue><alpha>255</alpha></color></font><font target='APPNAME'><size>20</size></font></theme>";
    let Ok(theme) = crate::metadata::parse_theme_xml(xml) else {
        return -1;
    };
    if theme.default_font.color != Some(crate::metadata::Rgba(4, 5, 6, 255))
        || theme.fonts.len() != 1
        || theme.fonts[0].0 != ["appname"]
        || theme.fonts[0].1.size != Some(12)
        || theme.fonts[0].1.color != Some(crate::metadata::Rgba(255, 0, 7, 255))
    {
        ogc_system::report(format_args!("FAIL metadata-theme parsed fields\n"));
        return -1;
    }
    ogc_system::report(format_args!("PASS metadata-theme atoi and target merge\n"));
    0
}

fn metadata_app() -> i32 {
    let xml = "<app><name> Demo </name><author> </author><coder>fallback</coder><version> 1 </version><short_description> note </short_description><sort_id>  +12tail</sort_id><arguments><arg> x </arg><arg>y</arg></arguments><ahb_access/><no_ios_reload/></app>";
    let Ok(metadata) = crate::metadata::parse_app_xml(xml) else {
        return -1;
    };
    if metadata.name.as_deref() != Some(" Demo ")
        || metadata.author.as_deref() != Some(" ")
        || metadata.version.as_deref() != Some(" 1 ")
        || metadata.short_description.as_deref() != Some(" note ")
        || metadata.sort_id != Some(12)
        || metadata.arguments != [" x ", "y"]
        || !metadata.ahb_access
        || !metadata.no_ios_reload
    {
        ogc_system::report(format_args!("FAIL metadata-app parsed fields\n"));
        return -1;
    }
    ogc_system::report(format_args!("PASS metadata-app CData and sort prefix\n"));
    0
}

struct MemoryCatalogStorage {
    folders: Vec<crate::catalog::DirectoryEntry>,
    next_folder: usize,
    files: BTreeMap<String, Vec<u8>>,
}

impl crate::catalog::CatalogStorage for MemoryCatalogStorage {
    type Error = ();

    fn open_dir(&mut self, path: &str) -> Result<(), Self::Error> {
        (path == "apps").then_some(()).ok_or(())
    }

    fn next_dir_entry(&mut self) -> Result<Option<crate::catalog::DirectoryEntry>, Self::Error> {
        let folder = self.folders.get(self.next_folder).cloned();
        self.next_folder += usize::from(folder.is_some());
        Ok(folder)
    }

    fn close_dir(&mut self) {}

    fn file_size(&mut self, path: &str) -> Result<Option<u64>, Self::Error> {
        Ok(self.files.get(path).map(|bytes| bytes.len() as u64))
    }

    fn read_file(&mut self, path: &str, max_bytes: usize) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self
            .files
            .get(path)
            .filter(|bytes| bytes.len() <= max_bytes)
            .cloned())
    }
}

fn catalog() -> i32 {
    use crate::catalog::{AppKind, DirectoryEntry, Filter, SortOrder, scan_apps, visible_entries};

    let folders = [
        ("alpha", true),
        ("beta", true),
        ("badmeta", true),
        (".hidden", true),
        ("unsafe/name", true),
        ("large", true),
        ("theme-large", true),
        ("file", false),
    ];
    let mut storage = MemoryCatalogStorage {
        folders: folders
            .iter()
            .map(|(name, is_directory)| DirectoryEntry {
                name: String::from(*name),
                is_directory: *is_directory,
            })
            .collect(),
        next_folder: 0,
        files: BTreeMap::new(),
    };
    for (path, bytes) in [
        ("apps/alpha/boot.elf", b"elf".as_slice()),
        ("apps/alpha/boot.dol", b"dol".as_slice()),
        ("apps/alpha/theme.zip", b"zip".as_slice()),
        ("apps/beta/boot.dol", b"dol".as_slice()),
        ("apps/badmeta/boot.dol", b"dol".as_slice()),
        ("apps/large/boot.elf", b"123456".as_slice()),
        ("apps/theme-large/theme.zip", b"1234".as_slice()),
        (
            "apps/alpha/meta.xml",
            b"<app><name>Zeta</name><sort_id>2</sort_id></app>".as_slice(),
        ),
        (
            "apps/beta/meta.xml",
            b"<app><name>Alpha</name><sort_id>1</sort_id></app>".as_slice(),
        ),
        ("apps/badmeta/meta.xml", b"<app>".as_slice()),
    ] {
        storage.files.insert(String::from(path), bytes.to_vec());
    }
    storage.files.insert(
        String::from("apps/alpha/icon.png"),
        include_bytes!("../tests/fixtures/hbc-app-icon.png").to_vec(),
    );
    storage.files.insert(
        String::from("apps/beta/icon.png"),
        include_bytes!("../tests/fixtures/hbc-app-icon-wrong-size.png").to_vec(),
    );
    storage
        .files
        .insert(String::from("apps/badmeta/icon.png"), b"not a PNG".to_vec());
    let entries = match scan_apps(&mut storage, "apps", 5, 3) {
        Ok(entries) => entries,
        Err(error) => {
            ogc_system::report(format_args!("FAIL catalog scan {error:?}\n"));
            return -1;
        }
    };
    if entries.len() != 3
        || entries[0].directory != "alpha"
        || entries[0].kind != AppKind::Elf
        || !entries[0].has_icon
        || entries[1].directory != "beta"
        || entries[1].kind != AppKind::Dol
        || entries[1].has_icon
        || entries[2].directory != "badmeta"
        || entries[2].metadata.is_some()
    {
        ogc_system::report(format_args!("FAIL catalog entries {entries:?}\n"));
        return -1;
    }
    let custom = visible_entries(&entries, Filter::MetadataOnly, SortOrder::Custom);
    if custom
        .iter()
        .map(|entry| entry.directory.as_str())
        .collect::<Vec<_>>()
        != ["beta", "alpha"]
        || visible_entries(&entries, Filter::IconsOnly, SortOrder::Name)
            .iter()
            .map(|entry| entry.directory.as_str())
            .collect::<Vec<_>>()
            != ["alpha"]
        || crate::png::decode_hbc_app_icon(include_bytes!("../tests/fixtures/hbc-app-icon.png"))
            .is_err()
        || crate::png::decode_hbc_app_icon(include_bytes!(
            "../tests/fixtures/hbc-app-icon-wrong-size.png"
        ))
        .is_ok()
        || crate::png::decode_hbc_app_icon(b"not a PNG").is_ok()
    {
        ogc_system::report(format_args!("FAIL catalog filter or sort\n"));
        return -1;
    }
    ogc_system::report(format_args!("PASS catalog scan and sort\n"));
    0
}

fn theme_package() -> i32 {
    let bytes = include_bytes!("../tests/fixtures/theme-valid.zip");
    let package = match crate::theme::ThemePackage::parse(bytes) {
        Ok(package) => package,
        Err(error) => {
            ogc_system::report(format_args!("FAIL theme package parse {error:?}\n"));
            return -1;
        }
    };
    if package.theme_xml_name() != "theme.xml"
        || package.metadata.description.as_deref() != Some("Dolphin")
        || package.extract_file(bytes, "BUTTON.PNG", 32).as_deref()
            != Ok(b"theme-button-bytes".as_slice())
        || package.extract_file(bytes, "missing.png", 32).is_ok()
    {
        ogc_system::report(format_args!("FAIL theme package outputs\n"));
        return -1;
    }
    let invalid = include_bytes!("../tests/fixtures/theme-invalid-entry.zip");
    if !matches!(
        crate::theme::ThemePackage::parse(invalid),
        Err(crate::theme::ThemePackageError::InvalidEntry)
    ) {
        ogc_system::report(format_args!("FAIL theme package entry validation\n"));
        return -1;
    }
    ogc_system::report(format_args!(
        "PASS theme package validation and extraction\n"
    ));
    0
}

fn theme_textures_api() -> i32 {
    let standard = crate::platform::theme::ThemeTextures::load(None);
    if standard.background_width != 640
        || !matches!(standard.background.as_ref(), Some(texture) if texture.width == 640 && texture.height == 480)
        || standard.logo.is_none()
        || standard.list_card.is_none()
    {
        ogc_system::report(format_args!("FAIL standard theme texture defaults\n"));
        return -1;
    }
    let fallback_card = standard.list_card.as_ref().unwrap().as_bytes().to_vec();
    drop(standard);

    let archive = include_bytes!("../tests/fixtures/theme-texture-override.zip");
    let package = match crate::theme::ThemePackage::parse(archive) {
        Ok(package) => package,
        Err(error) => {
            ogc_system::report(format_args!("FAIL theme texture package {error:?}\n"));
            return -1;
        }
    };
    let themed = crate::platform::theme::ThemeTextures::load(Some((&package, archive)));
    if !matches!(themed.list_card.as_ref(), Some(texture) if texture.width == 432 && texture.height == 64 && texture.as_bytes() != fallback_card.as_slice())
    {
        ogc_system::report(format_args!("FAIL themed list card overrides fallback\n"));
        return -1;
    }
    drop(themed);

    let widescreen = crate::platform::theme::ThemeTextures::load_for_viewport(None, true);
    if widescreen.background_width != 852
        || !matches!(widescreen.background.as_ref(), Some(texture) if texture.width == 852 && texture.height == 480)
        || widescreen.logo.is_none()
        || widescreen.list_card.is_none()
    {
        ogc_system::report(format_args!("FAIL widescreen theme texture defaults\n"));
        return -1;
    }

    ogc_system::report(format_args!(
        "PASS theme texture defaults and widescreen background APIs\n"
    ));
    0
}

fn theme_storage_api() -> i32 {
    let archive = include_bytes!("../tests/fixtures/theme-texture-override.zip");
    let archive_is_installed = || matches!(crate::platform::theme::load(), Ok(Some(bytes)) if bytes.as_slice() == archive.as_slice());
    if crate::theme::ThemePackage::parse(archive).is_err() {
        ogc_system::report(format_args!("FAIL theme storage fixture validation\n"));
        return -1;
    }

    if let Err(error) = crate::platform::theme::save(archive) {
        ogc_system::report(format_args!("FAIL theme storage save {error:?}\n"));
        return -1;
    }
    if !archive_is_installed() {
        ogc_system::report(format_args!("FAIL theme storage saved archive readback\n"));
        return -1;
    }
    if crate::platform::theme::save(archive).is_err() || !archive_is_installed() {
        ogc_system::report(format_args!("FAIL theme storage replacement readback\n"));
        return -1;
    }

    if crate::platform::theme::save(&[])
        != Err(crate::platform::settings::SettingsIoError::InvalidFileSize)
        || !archive_is_installed()
    {
        ogc_system::report(format_args!(
            "FAIL theme storage invalid-save preservation\n"
        ));
        return -1;
    }

    ogc_system::report(format_args!(
        "PASS theme storage save, load and invalid-input preservation APIs\n"
    ));
    0
}

fn archive_api() -> i32 {
    use crate::archive::{ArchivePathError, ZipArchive, ZipError, validate_archive_path};

    let bytes = include_bytes!("../tests/fixtures/theme-valid.zip");
    let archive = match ZipArchive::parse(bytes) {
        Ok(archive) => archive,
        Err(error) => {
            ogc_system::report(format_args!("FAIL archive API parse {error:?}\n"));
            return -1;
        }
    };
    let entries = archive.entries();
    if entries.len() != 2
        || entries[0].name != "theme.xml"
        || entries[1].name != "button.png"
        || archive.entry("theme.xml").map(|entry| entry.name.as_str()) != Some("theme.xml")
        || archive.entry("missing.png").is_some()
        || archive.extract(bytes, "button.png", 32).as_deref()
            != Ok(b"theme-button-bytes".as_slice())
        || archive.extract(bytes, "button.png", 1) != Err(ZipError::OutputTooLarge)
        || ZipArchive::parse_limited(bytes, 1) != Err(ZipError::TooManyEntries)
        || ZipArchive::parse(b"not a zip") != Err(ZipError::TooShort)
        || validate_archive_path("theme.xml").is_err()
        || validate_archive_path("folder/theme.xml").is_err()
        || validate_archive_path("../escape").is_ok()
        || validate_archive_path("folder/../escape").is_ok()
    {
        ogc_system::report(format_args!("FAIL archive API observable results\n"));
        return -1;
    }
    if validate_archive_path("../escape") != Err(ArchivePathError::InvalidComponent) {
        ogc_system::report(format_args!("FAIL archive API traversal rejection\n"));
        return -1;
    }
    ogc_system::report(format_args!(
        "PASS archive API parse, lookup, extraction and paths\n"
    ));
    0
}

fn png_api() -> i32 {
    use crate::png::{
        GxRgba8Texture, PngError, PngLimits, RgbaImage, decode_png, encode_rgba_png, to_gx_rgba8,
    };

    let pixels = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0];
    let encoded = match encode_rgba_png(2, 1, &pixels) {
        Ok(encoded) => encoded,
        Err(error) => {
            ogc_system::report(format_args!("FAIL PNG API encode {error:?}\n"));
            return -1;
        }
    };
    let limits = PngLimits {
        max_width: 2,
        max_height: 1,
        max_pixels: 2,
        max_input_bytes: 1024,
    };
    let decoded = match decode_png(&encoded, limits) {
        Ok(decoded) => decoded,
        Err(error) => {
            ogc_system::report(format_args!("FAIL PNG API decode {error:?}\n"));
            return -1;
        }
    };
    if encoded.get(..8) != Some(b"\x89PNG\r\n\x1a\n")
        || encoded.get(16..24) != Some(&[0, 0, 0, 2, 0, 0, 0, 1])
        || decoded.width != 2
        || decoded.height != 1
        || decoded.pixels != pixels
        || decode_png(
            include_bytes!("../tests/fixtures/hbc-app-icon.png"),
            PngLimits {
                max_width: 128,
                max_height: 48,
                max_pixels: 128 * 48,
                max_input_bytes: crate::png::HBC_APP_ICON_MAX_PNG_BYTES,
            },
        )
        .map(|image| (image.width, image.height, image.pixels.len()))
            != Ok((128, 48, 128 * 48 * 4))
    {
        ogc_system::report(format_args!("FAIL PNG API encoded or decoded pixels\n"));
        return -1;
    }

    let image = RgbaImage {
        width: 1,
        height: 1,
        pixels: alloc::vec![1, 2, 3, 4],
    };
    let tiled = match to_gx_rgba8(&image) {
        Ok(tiled) => tiled,
        Err(error) => {
            ogc_system::report(format_args!("FAIL PNG API GX conversion {error:?}\n"));
            return -1;
        }
    };
    let texture = match GxRgba8Texture::from_image(&image) {
        Ok(texture) => texture,
        Err(error) => {
            ogc_system::report(format_args!("FAIL PNG API texture {error:?}\n"));
            return -1;
        }
    };
    if tiled.len() != 64
        || tiled[..2] != [4, 1]
        || tiled[2..32].iter().any(|byte| *byte != 0)
        || tiled[32..34] != [2, 3]
        || tiled[34..].iter().any(|byte| *byte != 0)
        || texture.width != 1
        || texture.height != 1
        || texture.as_bytes() != tiled
        || texture.as_ptr() != texture.as_bytes().as_ptr()
        || texture.as_ptr() as usize & 31 != 0
        || texture.len() != 64
        || texture.is_empty()
    {
        ogc_system::report(format_args!(
            "FAIL PNG API GX texture layout or ownership\n"
        ));
        return -1;
    }
    if decode_png(b"not a PNG", limits) != Err(PngError::InvalidSignature) {
        ogc_system::report(format_args!("FAIL PNG API invalid signature\n"));
        return -1;
    }
    ogc_system::report(format_args!("PASS PNG API encode, decode and GX texture\n"));
    0
}

fn management_api() -> i32 {
    use crate::{
        archive::ZipArchive,
        management::{
            DirectoryCommitError, ExtractionLimits, ExtractionOperation, commit_staged_directory,
            plan_extraction, rollback_staged_directory,
        },
    };

    let bytes = include_bytes!("../tests/fixtures/wiiload-app.zip");
    let archive = match ZipArchive::parse(bytes) {
        Ok(archive) => archive,
        Err(error) => {
            ogc_system::report(format_args!("FAIL management API archive {error:?}\n"));
            return -1;
        }
    };
    let plan = match plan_extraction(
        &archive,
        ExtractionLimits {
            max_entries: 1024,
            max_file_bytes: 1024,
            max_total_bytes: 2048,
        },
    ) {
        Ok(plan) => plan,
        Err(error) => {
            ogc_system::report(format_args!("FAIL management API plan {error:?}\n"));
            return -1;
        }
    };
    if plan.total_bytes() != 10
        || plan.operations().len() != 3
        || !plan
            .operations()
            .contains(&ExtractionOperation::CreateDirectory {
                path: String::from("Demo"),
            })
        || !plan.operations().contains(&ExtractionOperation::WriteFile {
            path: String::from("Demo/boot.elf"),
            archive_name: String::from("Demo/boot.elf"),
            size: 4,
        })
        || !plan.operations().contains(&ExtractionOperation::WriteFile {
            path: String::from("Demo/meta.xml"),
            archive_name: String::from("Demo/meta.xml"),
            size: 6,
        })
    {
        ogc_system::report(format_args!("FAIL management API extraction plan\n"));
        return -1;
    }

    let target = "apps/Demo";
    let stage = "apps/.Demo.hbc-stage";
    let backup = "apps/.Demo.hbc-backup";
    let mut commit_calls = Vec::new();
    if !matches!(
        commit_staged_directory(true, target, stage, backup, |from, to| {
            commit_calls.push((String::from(from), String::from(to)));
            Ok::<(), ()>(())
        }),
        Ok(())
    ) || commit_calls
        != [
            (String::from(target), String::from(backup)),
            (String::from(stage), String::from(target)),
        ]
    {
        ogc_system::report(format_args!("FAIL management API commit order\n"));
        return -1;
    }

    let mut failure_calls = Vec::new();
    let failure = commit_staged_directory(true, target, stage, backup, |from, to| {
        failure_calls.push((String::from(from), String::from(to)));
        if from == stage {
            Err("stage rename failed")
        } else {
            Ok(())
        }
    });
    if failure != Err(DirectoryCommitError::Install("stage rename failed"))
        || failure_calls
            != [
                (String::from(target), String::from(backup)),
                (String::from(stage), String::from(target)),
                (String::from(backup), String::from(target)),
            ]
    {
        ogc_system::report(format_args!("FAIL management API commit rollback\n"));
        return -1;
    }

    let mut rollback_calls = Vec::new();
    if rollback_staged_directory(true, target, stage, backup, |from, to| {
        rollback_calls.push((String::from(from), String::from(to)));
        Ok::<(), ()>(())
    })
    .is_err()
        || rollback_calls
            != [
                (String::from(target), String::from(stage)),
                (String::from(backup), String::from(target)),
            ]
    {
        ogc_system::report(format_args!("FAIL management API rollback order\n"));
        return -1;
    }

    ogc_system::report(format_args!(
        "PASS management API planning and staged rename policy\n"
    ));
    0
}

fn font_api() -> i32 {
    use crate::{
        font::{FontError, ThemeFontFace},
        text_layout::MemoSpec,
    };

    if ThemeFontFace::parse(String::from("bad.ttf"), alloc::vec![0; 8]).is_ok() {
        ogc_system::report(format_args!("FAIL font API invalid font accepted\n"));
        return -1;
    }
    let mut face = match ThemeFontFace::parse(
        String::from("droid.ttf"),
        include_bytes!("../assets/droid.ttf").to_vec(),
    ) {
        Ok(face) => face,
        Err(error) => {
            ogc_system::report(format_args!("FAIL font API parse {error:?}\n"));
            return -1;
        }
    };
    let Some((ascent, descent, line_gap)) = face.horizontal_line_metrics(18) else {
        ogc_system::report(format_args!("FAIL font API line metrics missing\n"));
        return -1;
    };
    if ascent <= 0.0 || descent >= 0.0 || !line_gap.is_finite() {
        ogc_system::report(format_args!("FAIL font API line metrics values\n"));
        return -1;
    }
    let empty_width = face.measure_width("", 18);
    let narrow_width = face.measure_width("iii", 18);
    let wide_width = face.measure_width("WWW", 18);
    if empty_width != 0.0 || wide_width <= narrow_width * 2.0 {
        ogc_system::report(format_args!("FAIL font API measured widths\n"));
        return -1;
    }
    if face
        .horizontal_kern('A', 'V', 18)
        .is_some_and(|value| !value.is_finite())
    {
        ogc_system::report(format_args!("FAIL font API kerning value\n"));
        return -1;
    }
    if face.cached_glyph_count() != 0 {
        ogc_system::report(format_args!("FAIL font API initial cache count\n"));
        return -1;
    }
    let glyph = match face.rasterize('A', 18) {
        Ok(glyph) => glyph.clone(),
        Err(error) => {
            ogc_system::report(format_args!("FAIL font API rasterize {error:?}\n"));
            return -1;
        }
    };
    if glyph.width == 0
        || glyph.height == 0
        || glyph.coverage.len() != usize::from(glyph.width) * usize::from(glyph.height)
        || !glyph.coverage.iter().any(|coverage| *coverage != 0)
        || face.rasterize('A', 18).as_deref() != Ok(&glyph)
        || face.cached_glyph_count() != 1
        || face.rasterize('A', 22).is_err()
        || face.cached_glyph_count() != 2
        || face.rasterize('A', 0) != Err(FontError::InvalidSize)
    {
        ogc_system::report(format_args!("FAIL font API glyph cache or bounds\n"));
        return -1;
    }

    let text = "Dolphin theme fonts wrap text across several memo lines.";
    let memo = match face.layout_memo(
        text,
        MemoSpec {
            width: 80.0,
            height: 32.0,
            scroll: 0,
        },
        16,
        1.0,
    ) {
        Ok(memo) => memo,
        Err(error) => {
            ogc_system::report(format_args!("FAIL font API layout {error:?}\n"));
            return -1;
        }
    };
    if memo.max_scroll == 0
        || memo.lines.is_empty()
        || memo
            .lines
            .iter()
            .any(|line| face.measure_width(line, 16) > 80.0)
        || face.layout_memo(
            text,
            MemoSpec {
                width: 0.0,
                height: 32.0,
                scroll: 0,
            },
            16,
            1.0,
        ) != Err(FontError::InvalidSize)
    {
        ogc_system::report(format_args!("FAIL font API memo layout bounds\n"));
        return -1;
    }
    ogc_system::report(format_args!(
        "PASS font API metrics, glyphs and memo layout\n"
    ));
    0
}

fn localization_api() -> i32 {
    use crate::{localization::Language, metadata::ThemeMetadata};

    let mut japanese_theme = ThemeMetadata::default();
    japanese_theme.languages = alloc::vec![String::from("ja")];
    if Language::from_console(-1, None) != Language::English
        || Language::from_console(0, None) != Language::English
        || Language::from_console(0, Some(&japanese_theme)) != Language::Japanese
        || Language::from_console(2, None) != Language::German
        || Language::from_console(3, None) != Language::French
        || Language::from_console(4, None) != Language::Spanish
        || Language::from_console(5, None) != Language::Italian
        || Language::from_console(6, None) != Language::Dutch
        || Language::from_console(99, None) != Language::English
    {
        ogc_system::report(format_args!("FAIL localization API language selection\n"));
        return -1;
    }
    if Language::Spanish.text("Back") != "Atrás"
        || Language::French.text("Device:") != "Dispositif:"
        || Language::German.text("Options") != "Einstellungen"
        || Language::Dutch.text("Options") != "Opties"
        || Language::Italian.text("Please wait...") != "Attendere..."
        || Language::Japanese.text("Yes") != "はい"
        || Language::French.text("dynamic text") != "dynamic text"
    {
        ogc_system::report(format_args!("FAIL localization API catalog lookup\n"));
        return -1;
    }
    let plain = Language::English.received_zip_confirmation("demo", 2048, false);
    let overwrite = Language::German.received_zip_confirmation("demo", 1024 * 1024, true);
    if !plain.contains("2 KB of free space")
        || plain.contains("overwritten")
        || !overwrite.contains("1.00 MB")
        || !overwrite.contains("'demo'")
        || !overwrite.contains("überschrieben")
    {
        ogc_system::report(format_args!("FAIL localization API ZIP confirmation\n"));
        return -1;
    }
    ogc_system::report(format_args!(
        "PASS localization API selection, catalog and ZIP text\n"
    ));
    0
}

fn overlay_control_api() -> i32 {
    use crate::home_overlay::{OpenSignal, OperationResult, OverlayControl, OverlayError, Request};

    let signal = OpenSignal::new();
    if signal.take() {
        return -1;
    }
    signal.request();
    signal.request();
    if !signal.take() || signal.take() {
        return -1;
    }

    let mut control = OverlayControl::default();
    if control.is_open()
        || control.request(Request::Save) != Err(OverlayError::NotOpen)
        || control.close() != Err(OverlayError::NotOpen)
        || control.open().is_err()
        || !control.is_open()
        || control.open() != Err(OverlayError::AlreadyOpen)
        || control.request(Request::Save) != Ok(Request::Save)
        || control.request(Request::Screenshot) != Err(OverlayError::Busy)
        || control.close() != Err(OverlayError::Busy)
        || control.complete(Request::PowerOff, OperationResult::Succeeded)
            != Err(OverlayError::MismatchedOperation)
        || control
            .complete(Request::Save, OperationResult::Failed)
            .is_err()
        || !control.is_open()
        || control.complete(Request::Save, OperationResult::Succeeded)
            != Err(OverlayError::NoPendingOperation)
        || control.request(Request::Close) != Ok(Request::Close)
        || control.is_open()
    {
        ogc_system::report(format_args!("FAIL overlay control API pending lifecycle\n"));
        return -1;
    }
    if control.open().is_err()
        || control.request(Request::ReturnToSystemMenu) != Ok(Request::ReturnToSystemMenu)
        || control
            .complete(Request::ReturnToSystemMenu, OperationResult::Failed)
            .is_err()
        || !control.is_open()
        || control.request(Request::PowerOff) != Ok(Request::PowerOff)
        || control
            .complete(Request::PowerOff, OperationResult::Succeeded)
            .is_err()
        || control.is_open()
    {
        ogc_system::report(format_args!("FAIL overlay control API exit lifecycle\n"));
        return -1;
    }
    ogc_system::report(format_args!(
        "PASS overlay control API signal, retry, close and exit\n"
    ));
    0
}

fn home_overlay_app_api() -> i32 {
    use crate::{
        app::Input,
        home_overlay::{OperationResult, OverlayError, Request},
        home_overlay_app::{
            AppHomeOverlay, Capabilities, FrameAction, Key, OverlayText, Page, Slot, key_from_input,
        },
        platform::input::InputFrame,
    };

    struct TextCapture(Vec<String>);
    impl OverlayText for TextCapture {
        fn draw_text(&mut self, label: &str, _x: usize, _y: usize, _selected: bool) {
            self.0.push(String::from(label));
        }
    }

    let mut overlay = AppHomeOverlay::default();
    if overlay.is_open()
        || overlay.page() != Page::Strip
        || overlay.count() != 5
        || overlay.selected() != 3
        || overlay.pending_confirmation().is_some()
        || overlay.capabilities() != Capabilities::default()
        || overlay.item_label(3) != "Shot"
        || !overlay.item_enabled(2)
        || overlay.item_enabled(1)
        || overlay.set_slot(2, None)
        || key_from_input(Input::Home) != Some(Key::Home)
        || key_from_input(Input::ToggleBrowseMode).is_some()
    {
        ogc_system::report(format_args!("FAIL app HOME overlay defaults and inputs\n"));
        return -1;
    }
    if !overlay.set_slot(
        0,
        Some(Slot {
            label: "Quick Save",
            request: Request::Save,
        }),
    ) || !overlay.set_slot(
        1,
        Some(Slot {
            label: "Shot",
            request: Request::Screenshot,
        }),
    ) {
        ogc_system::report(format_args!("FAIL app HOME overlay custom slots\n"));
        return -2;
    }
    let capabilities = Capabilities {
        can_restart_app: true,
        can_save: true,
        can_find_remotes: true,
        can_disconnect_remotes: true,
    };
    overlay.set_capabilities(capabilities);
    if overlay.capabilities() != capabilities
        || overlay.open_programmatically().is_err()
        || !overlay.is_open()
        || overlay.open_programmatically() != Err(OverlayError::AlreadyOpen)
        || overlay.selected() != 3
        || overlay.item_label(1) != "Quick Save"
        || !overlay.item_enabled(1)
    {
        ogc_system::report(format_args!("FAIL app HOME overlay setup\n"));
        return -3;
    }

    let mut text = TextCapture(Vec::new());
    overlay.draw_labels(320, 240, &mut text);
    let mut rect_count = 0;
    if overlay.visit_rects(320, 240, |_| rect_count += 1).is_err()
        || rect_count < 6
        || !text.0.iter().any(|label| label == "HOME")
        || !text.0.iter().any(|label| label == "Quick Save")
    {
        ogc_system::report(format_args!("FAIL app HOME overlay labels and geometry\n"));
        return -4;
    }
    let mut pixels = alloc::vec![0; 320 * 240 * 4];
    if overlay.draw(&mut pixels, 320, 240, 320 * 4).is_err()
        || pixels[(164 * 320 + 16) * 4..(164 * 320 + 16) * 4 + 4] != [16, 22, 35, 255]
        || overlay.draw(&mut pixels[..4], 320, 240, 320 * 4).is_ok()
    {
        ogc_system::report(format_args!("FAIL app HOME overlay software drawing\n"));
        return -5;
    }

    let action = overlay.tick_input_frame(&InputFrame {
        pressed: alloc::vec![Input::Right],
        ..InputFrame::default()
    });
    if action != Ok(FrameAction::None)
        || overlay.selected() != 4
        || overlay.tick(Some(Key::Confirm), false) != Ok(FrameAction::None)
        || overlay.page() != Page::WiiMote
        || overlay.item_label(0) != "Find remotes"
        || !overlay.item_enabled(1)
        || overlay.tick(Some(Key::Confirm), false) != Ok(FrameAction::Perform(Request::FindRemotes))
        || overlay
            .complete(Request::FindRemotes, OperationResult::Failed)
            .is_err()
        || !overlay.is_open()
    {
        ogc_system::report(format_args!("FAIL app HOME overlay remote action\n"));
        return -6;
    }
    if overlay.tick(Some(Key::Back), false) != Ok(FrameAction::None)
        || overlay.page() != Page::Strip
        || overlay.tick(Some(Key::Confirm), false) != Ok(FrameAction::None)
        || overlay.page() != Page::Exit
        || overlay.count() != 4
    {
        ogc_system::report(format_args!("FAIL app HOME overlay exit page\n"));
        return -7;
    }
    overlay.tick(Some(Key::Down), false).ok();
    overlay.tick(Some(Key::Down), false).ok();
    if overlay.tick(Some(Key::Confirm), false) != Ok(FrameAction::None)
        || overlay.pending_confirmation() != Some(Request::RestartConsole)
        || overlay.item_label(overlay.selected()) != "Confirm Restart Wii"
        || overlay.tick(Some(Key::Confirm), false)
            != Ok(FrameAction::Perform(Request::RestartConsole))
        || overlay
            .complete(Request::RestartConsole, OperationResult::Failed)
            .is_err()
        || !overlay.is_open()
        || overlay.tick(Some(Key::Back), false) != Ok(FrameAction::None)
        || overlay.tick(Some(Key::Back), false) != Ok(FrameAction::None)
        || overlay.is_open()
    {
        ogc_system::report(format_args!(
            "FAIL app HOME overlay safe confirmation and close\n"
        ));
        return -8;
    }
    ogc_system::report(format_args!(
        "PASS app HOME overlay API defaults, slots, input, drawing, actions and confirmation\n"
    ));
    0
}

fn network_restart_api() -> i32 {
    use crate::{
        platform::network_restart::{
            ConnectionCheck, ConnectionError, StopError, StopWorkRestartResult, stop_work_restart,
        },
        wiiload::ReceiverStatus,
    };

    let check = ConnectionCheck::new(100);
    if check.poll(ReceiverStatus::Starting, None, 10_000).is_some()
        || check
            .poll(ReceiverStatus::Listening, None, 60_099)
            .is_some()
        || check.poll(ReceiverStatus::Listening, Some([192, 168, 1, 8]), 101)
            != Some(Ok([192, 168, 1, 8]))
        || check.poll(ReceiverStatus::Stopped, None, 101)
            != Some(Err(ConnectionError::Receiver(ReceiverStatus::Stopped)))
        || check.poll(ReceiverStatus::Starting, None, 60_100) != Some(Err(ConnectionError::Timeout))
    {
        ogc_system::report(format_args!("FAIL network restart connection check\n"));
        return -1;
    }

    let calls = core::cell::RefCell::new(Vec::new());
    let pending = stop_work_restart(
        ReceiverStatus::PayloadReady,
        || {
            calls.borrow_mut().push("stop");
            Ok::<(), i32>(())
        },
        || {
            calls.borrow_mut().push("work");
            Ok::<(), i32>(())
        },
        || {
            calls.borrow_mut().push("start");
            Ok::<(), i32>(())
        },
    );
    if pending != Err(StopError::UploadPending) || !calls.borrow().is_empty() {
        ogc_system::report(format_args!("FAIL network restart upload guard\n"));
        return -2;
    }

    let stopped = stop_work_restart(
        ReceiverStatus::Listening,
        || {
            calls.borrow_mut().push("stop");
            Ok::<(), i32>(())
        },
        || {
            calls.borrow_mut().push("work");
            Err::<(), i32>(-7)
        },
        || {
            calls.borrow_mut().push("start");
            Ok::<(), i32>(())
        },
    );
    if stopped
        != Ok(StopWorkRestartResult {
            work: Err(-7),
            restart: Ok(()),
        })
        || *calls.borrow() != ["stop", "work", "start"]
    {
        ogc_system::report(format_args!("FAIL network restart work ordering\n"));
        return -3;
    }
    ogc_system::report(format_args!(
        "PASS network restart connection, upload guard and work ordering\n"
    ));
    0
}

fn input_platform_api() -> i32 {
    use crate::platform::input::{initialize, scan};

    if let Err(error) = initialize(640, 480) {
        ogc_system::report(format_args!("FAIL input platform initialization {error}\n"));
        return -1;
    }
    for _ in 0..3 {
        let frame = match scan() {
            Ok(frame) => frame,
            Err(error) => {
                ogc_system::report(format_args!("FAIL input platform scan {error:?}\n"));
                return -2;
            }
        };
        if frame
            .pressed
            .iter()
            .any(|input| !frame.held.contains(input))
            || frame.pointer_pressed && frame.pointer.is_none()
        {
            ogc_system::report(format_args!("FAIL input platform frame state\n"));
            return -3;
        }
    }
    ogc_system::report(format_args!(
        "PASS input platform init and three production scans\n"
    ));
    0
}

fn app_notice_api() -> i32 {
    use crate::app::{AppController, Input, NoticeKind, PointerTarget};

    let mut controller = AppController::new();
    let long_message = String::from("Storage warning. ").repeat(180);
    controller.show_notice(long_message.clone());
    if controller.notice() != Some(long_message.as_str())
        || controller.notice_kind() != NoticeKind::Generic
        || controller.timed_notice_active()
        || controller.notice_scroll() != 0
        || controller.notice_scroll_max() == 0
        || controller.handle(Input::Down, &[]) != crate::app::Action::None
        || controller.notice_scroll() != 1
    {
        ogc_system::report(format_args!("FAIL app notice API state and scroll\n"));
        return -1;
    }
    controller.show_notice_with_kind(long_message.clone(), NoticeKind::Warning);
    if controller.notice_kind() != NoticeKind::Warning
        || !controller.timed_notice_active()
        || controller.notice_scroll() != 0
        || controller.notice_scroll_max() == 0
    {
        ogc_system::report(format_args!("FAIL app notice API warning state\n"));
        return -1;
    }
    for _ in 0..299 {
        controller.tick_timed_notice();
    }
    if controller.notice().is_none() || !controller.timed_notice_active() {
        ogc_system::report(format_args!("FAIL app notice API early timeout\n"));
        return -1;
    }
    controller.tick_timed_notice();
    if controller.notice().is_some() || controller.timed_notice_active() {
        ogc_system::report(format_args!("FAIL app notice API timeout\n"));
        return -1;
    }
    controller.handle_pointer(None, false, &[]);

    controller.show_notice(String::from("Complete."));
    if controller.notice_kind() != NoticeKind::Generic
        || controller.timed_notice_active()
        || controller.notice_scroll_max() != 0
    {
        ogc_system::report(format_args!("FAIL app notice API generic notice\n"));
        return -1;
    }
    controller.show_notice_with_kind(String::from("Retry."), NoticeKind::Error);
    controller.mark_timed_notice_input_consumed();
    if controller.handle_pointer(Some(PointerTarget::NoticeConfirm), true, &[])
        != crate::app::Action::None
        || controller.notice() != Some("Retry.")
        || controller.handle_pointer(Some(PointerTarget::NoticeConfirm), true, &[])
            != crate::app::Action::None
        || controller.notice().is_some()
    {
        ogc_system::report(format_args!("FAIL app notice API consumed input\n"));
        return -1;
    }
    ogc_system::report(format_args!(
        "PASS app notice API kind, scroll, timeout and dismissal\n"
    ));
    0
}

fn about_api() -> i32 {
    use crate::app::{AppController, Input, Screen};

    let mut controller = AppController::new();
    controller.screen = Screen::About;
    controller.set_about_theme_description(true);
    for _ in 0..120 {
        controller.tick_about_scroll();
    }
    if controller.about_scroll() != 0 {
        ogc_system::report(format_args!("FAIL about API delay\n"));
        return -1;
    }
    for _ in 0..3 {
        controller.tick_about_scroll();
    }
    if controller.about_scroll() != 1
        || controller.handle(Input::Down, &[]) != crate::app::Action::None
        || controller.about_scroll() != 2
    {
        ogc_system::report(format_args!("FAIL about API automatic and manual scroll\n"));
        return -1;
    }
    for _ in 0..10_000 {
        controller.tick_about_scroll();
    }
    let max_with_theme = controller.about_scroll();
    controller.tick_about_scroll();
    if max_with_theme == 0 || controller.about_scroll() != max_with_theme {
        ogc_system::report(format_args!("FAIL about API scroll clamp\n"));
        return -1;
    }
    controller.set_about_theme_description(false);
    let max_without_theme = controller.about_scroll();
    if max_without_theme + 1 != max_with_theme {
        ogc_system::report(format_args!("FAIL about API theme description bound\n"));
        return -1;
    }
    ogc_system::report(format_args!(
        "PASS about API delay, manual scroll and theme bound\n"
    ));
    0
}

fn loader_helpers() -> i32 {
    use crate::loader::{
        IosLaunchPolicy, LoadPlan, LoadSegment, ParseError, app_ios_launch_policy,
        build_launch_arguments, ios_launch_policy, validate_load_plan,
    };

    if ios_launch_policy(true)
        != (IosLaunchPolicy {
            reload: true,
            ahb_access: true,
        })
    {
        return -1;
    }
    for (xml, expected) in [
        (
            "<app/>",
            IosLaunchPolicy {
                reload: true,
                ahb_access: false,
            },
        ),
        (
            "<app><ahb_access/></app>",
            IosLaunchPolicy {
                reload: true,
                ahb_access: true,
            },
        ),
        (
            "<app><no_ios_reload/></app>",
            IosLaunchPolicy {
                reload: false,
                ahb_access: false,
            },
        ),
        (
            "<app><ahb_access/><no_ios_reload/></app>",
            IosLaunchPolicy {
                reload: false,
                ahb_access: true,
            },
        ),
    ] {
        let Ok(metadata) = crate::metadata::parse_app_xml(xml) else {
            return -1;
        };
        if app_ios_launch_policy(Some(&metadata)) != expected {
            return -1;
        }
    }
    if app_ios_launch_policy(None)
        != (IosLaunchPolicy {
            reload: true,
            ahb_access: false,
        })
    {
        return -1;
    }

    let arguments = alloc::vec![String::from("--safe"), String::from("two words")];
    if build_launch_arguments("sd:/apps/demo/boot.dol", &arguments)
        != Ok(b"sd:/apps/demo/boot.dol\0--safe\0two words\0\0".to_vec())
        || build_launch_arguments("bad\0path", &[]).is_ok()
    {
        return -1;
    }

    let bytes = [1, 2, 3, 4];
    let plan = LoadPlan {
        segments: alloc::vec![LoadSegment {
            file_range: 0..4,
            address: 0x8000_3400,
            file_size: 4,
            memory_size: 8,
            executable: true,
        }],
        bss: Some(0x8000_3408..0x8000_3410),
        entry: 0x8000_3400,
    };
    if validate_load_plan(&bytes, &plan, &(0x8000_3400..0x8000_4000)).is_err() {
        return -1;
    }
    let mut invalid = plan;
    invalid.entry = 0x8000_3410;
    if validate_load_plan(&bytes, &invalid, &(0x8000_3400..0x8000_4000))
        != Err(ParseError::InvalidFormat(
            "entry point is outside executable segments",
        ))
    {
        return -1;
    }
    ogc_system::report(format_args!(
        "PASS loader policies, arguments, and plan validation\n"
    ));
    0
}

fn app_controller() -> i32 {
    use crate::{
        app::{Action, AppController, Input, PointerTarget},
        catalog::{AppEntry, AppKind},
    };

    let catalog: Vec<_> = (0..7)
        .map(|index| AppEntry {
            directory: alloc::format!("app-{index}"),
            kind: AppKind::Dol,
            size: 1,
            has_icon: false,
            metadata: None,
        })
        .collect();
    let mut controller = AppController::new();
    let navigation = controller.page_navigation(&catalog);
    if controller.visible_apps(&catalog).len() != 7
        || controller.selected_index() != 0
        || controller.page_start() != 0
        || navigation.capacity != 5
        || navigation.start != 0
        || !navigation.has_next()
        || navigation.has_previous()
        || controller.handle(Input::NextPage, &catalog) != Action::NextPage
        || controller.selected_app.as_deref() != Some("app-5")
        || controller.selected_index() != 5
        || controller.page_start() != 5
        || controller.handle(Input::NextPage, &catalog) != Action::None
        || controller.handle(Input::PreviousPage, &catalog) != Action::PreviousPage
        || controller.selected_app.as_deref() != Some("app-0")
        || controller.selected_index() != 0
    {
        ogc_system::report(format_args!("FAIL app controller page navigation\n"));
        return -11;
    }
    controller.handle(Input::ToggleBrowseMode, &catalog);
    controller.handle(Input::Down, &catalog);
    if controller.browser_layout().columns != 3
        || controller.selected_app.as_deref() != Some("app-3")
        || controller.selected_index() != 3
        || controller.visible_page(&catalog).len() != 7
    {
        ogc_system::report(format_args!("FAIL app controller grid navigation\n"));
        return -12;
    }
    let selected_identity = controller.selected_app.clone();
    controller.set_grid_columns(4, &catalog);
    if controller.browser_layout().columns != 4 || controller.selected_app != selected_identity {
        ogc_system::report(format_args!("FAIL app controller grid column setting\n"));
        return -2;
    }
    controller.set_browser_layout(
        crate::app::BrowserLayout {
            columns: 0,
            rows: 0,
        },
        &catalog,
    );
    if controller.browser_layout()
        != (crate::app::BrowserLayout {
            columns: 1,
            rows: 1,
        })
        || controller.selected_app != selected_identity
    {
        ogc_system::report(format_args!("FAIL app controller layout bounds\n"));
        return -3;
    }
    let mut pointer_controller = AppController::new();
    if pointer_controller.handle_pointer(Some(PointerTarget::App(3)), false, &catalog)
        != Action::None
        || pointer_controller.selected_index() != 3
        || pointer_controller.pointer_target() != Some(PointerTarget::App(3))
        || pointer_controller.handle_pointer(
            Some(PointerTarget::Page { forward: true }),
            true,
            &catalog,
        ) != Action::NextPage
        || pointer_controller.selected_app.as_deref() != Some("app-5")
    {
        ogc_system::report(format_args!("FAIL app controller pointer actions\n"));
        return -13;
    }
    let mut detail_catalog = catalog.clone();
    detail_catalog[0].metadata = Some(crate::metadata::AppMetadata {
        long_description: Some("Scrollable description ".repeat(80)),
        ..crate::metadata::AppMetadata::default()
    });
    let mut detail_controller = AppController::new();
    detail_controller.handle(Input::Confirm, &detail_catalog);
    detail_controller.set_measured_memo(12, 9);
    detail_controller.handle(Input::Down, &detail_catalog);
    detail_controller.handle(Input::Down, &detail_catalog);
    if detail_controller.detail_description_scroll() != 2 {
        ogc_system::report(format_args!(
            "FAIL app controller detail memo initial scroll\n"
        ));
        return -14;
    }
    if detail_controller.scroll_active_memo_by_stick(1) != Some(Input::Up)
        || detail_controller.scroll_active_memo_by_stick(-1) != Some(Input::Down)
    {
        ogc_system::report(format_args!("FAIL app controller memo stick routing\n"));
        return -4;
    }
    let scroll_before_stick = detail_controller.detail_description_scroll();
    detail_controller.scroll_detail_by_stick(-9);
    if detail_controller.detail_description_scroll() <= scroll_before_stick {
        ogc_system::report(format_args!("FAIL app controller detail stick scroll\n"));
        return -5;
    }
    detail_controller.handle(Input::Up, &detail_catalog);
    detail_controller.handle_pointer_frame(
        Some(PointerTarget::DetailMemo),
        Some(200),
        true,
        false,
        &detail_catalog,
    );
    detail_controller.handle_pointer_frame(None, Some(-88), false, true, &detail_catalog);
    if detail_controller.detail_description_scroll() != 1 {
        ogc_system::report(format_args!("FAIL app controller detail memo drag\n"));
        return -15;
    }
    detail_controller.handle_pointer_frame(None, None, false, false, &detail_catalog);
    if detail_controller.detail_description_scroll() != 1 {
        ogc_system::report(format_args!(
            "FAIL app controller detail memo drag release\n"
        ));
        return -16;
    }
    ogc_system::report(format_args!(
        "PASS app controller page, grid layout setters, stick scroll, pointer navigation and detail memo drag\n"
    ));
    0
}

fn main_menu_api() -> i32 {
    use crate::{app::AppController, runtime::AppRuntime};

    let mut controller = AppController::new();
    let menu_count = if cfg!(feature = "legacy-main-menu") {
        5
    } else {
        6
    };
    if controller.bootmii_available()
        || controller.menu_item_count() != menu_count
        || controller.menu_item_enabled(2)
    {
        ogc_system::report(format_args!("FAIL main menu initial BootMii state\n"));
        return -1;
    }

    controller.handle(crate::app::Input::Home, &[]);
    if controller.handle_pointer(Some(crate::app::PointerTarget::Menu(2)), true, &[])
        != crate::app::Action::None
        || controller.menu_focus() != 0
    {
        ogc_system::report(format_args!(
            "FAIL main menu disabled BootMii pointer action\n"
        ));
        return -1;
    }
    controller.set_bootmii_available(true);
    if !controller.bootmii_available()
        || controller.menu_item_count() != menu_count
        || !controller.menu_item_enabled(2)
    {
        ogc_system::report(format_args!("FAIL main menu BootMii item visibility\n"));
        return -1;
    }
    controller.handle(crate::app::Input::Down, &[]);
    controller.handle(crate::app::Input::Down, &[]);
    if controller.handle(crate::app::Input::Confirm, &[]) != crate::app::Action::LaunchBootMii {
        ogc_system::report(format_args!("FAIL main menu BootMii action\n"));
        return -1;
    }
    controller.set_bootmii_available(false);
    if controller.menu_focus() != 0 {
        ogc_system::report(format_args!("FAIL main menu BootMii focus fallback\n"));
        return -1;
    }
    controller.set_bootmii_available(true);
    for _ in 0..menu_count {
        controller.handle(crate::app::Input::Down, &[]);
    }
    if controller.menu_focus() != menu_count - 1 {
        ogc_system::report(format_args!("FAIL main menu navigation boundary\n"));
        return -1;
    }

    controller.set_main_menu_status(String::from("IOS 58 v6176"), String::from("No network"));
    if controller.main_menu_status() != ("IOS 58 v6176", "No network") {
        ogc_system::report(format_args!("FAIL main menu status labels\n"));
        return -1;
    }

    let mut runtime = AppRuntime::default();
    runtime.set_bootmii_available(true);
    runtime.set_main_menu_status(String::from("IOS 58 v6176"), String::from("Connected"));
    if !runtime.controller().bootmii_available()
        || runtime.controller().menu_item_count() != menu_count
        || runtime.controller().main_menu_status() != ("IOS 58 v6176", "Connected")
    {
        ogc_system::report(format_args!("FAIL main menu runtime forwarding\n"));
        return -1;
    }

    ogc_system::report(format_args!(
        "PASS main menu BootMii state, focus clamp and status labels\n"
    ));
    0
}

fn executable_loading_api() -> i32 {
    use crate::{app::AppController, runtime::AppRuntime};

    let mut controller = AppController::new();
    controller.show_executable_loading();
    if controller.screen != crate::app::Screen::ExecutableLoading
        || controller.operation_progress() != 0
    {
        ogc_system::report(format_args!("FAIL executable loading initial state\n"));
        return -1;
    }
    controller.set_operation_progress(43);
    if controller.operation_progress() != 43 {
        ogc_system::report(format_args!("FAIL executable loading progress\n"));
        return -1;
    }
    controller.set_operation_progress(255);
    if controller.operation_progress() != 100 {
        ogc_system::report(format_args!("FAIL executable loading progress bound\n"));
        return -1;
    }
    controller.finish_executable_loading();
    if controller.screen != crate::app::Screen::Browser {
        ogc_system::report(format_args!("FAIL executable loading finish\n"));
        return -1;
    }
    controller.finish_executable_loading();
    if controller.screen != crate::app::Screen::Browser {
        ogc_system::report(format_args!("FAIL executable loading repeated finish\n"));
        return -1;
    }

    let mut runtime = AppRuntime::default();
    runtime.show_executable_loading();
    if runtime.controller().screen != crate::app::Screen::ExecutableLoading {
        ogc_system::report(format_args!("FAIL executable loading runtime start\n"));
        return -1;
    }
    runtime.finish_executable_loading();
    if runtime.controller().screen != crate::app::Screen::Browser {
        ogc_system::report(format_args!("FAIL executable loading runtime finish\n"));
        return -1;
    }

    ogc_system::report(format_args!(
        "PASS executable loading start, progress and finish APIs\n"
    ));
    0
}

fn detail_focus_api() -> i32 {
    use crate::{app::AppController, catalog::AppEntry};

    let catalog = [AppEntry {
        directory: String::from("focus-demo"),
        kind: crate::catalog::AppKind::Dol,
        size: 1,
        has_icon: false,
        metadata: None,
    }];
    let mut controller = AppController::new();
    controller.handle(crate::app::Input::Confirm, &catalog);
    controller.set_measured_memo(3, 12);
    if controller.detail_description_max_scroll() != 3 {
        ogc_system::report(format_args!("FAIL detail description maximum scroll\n"));
        return -1;
    }
    if controller.detail_focus() != 9 {
        ogc_system::report(format_args!("FAIL detail focus default Load item\n"));
        return -1;
    }
    controller.handle(crate::app::Input::Left, &catalog);
    if controller.detail_focus() != 8 {
        ogc_system::report(format_args!("FAIL detail focus previous item\n"));
        return -1;
    }
    controller.handle(crate::app::Input::Left, &catalog);
    if controller.detail_focus() != 8 {
        ogc_system::report(format_args!("FAIL detail focus previous boundary\n"));
        return -1;
    }
    controller.handle(crate::app::Input::Right, &catalog);
    if controller.detail_focus() != 9 {
        ogc_system::report(format_args!("FAIL detail focus return to Load\n"));
        return -1;
    }
    controller.handle(crate::app::Input::Right, &catalog);
    if controller.detail_focus() != 10 {
        ogc_system::report(format_args!("FAIL detail focus Back item\n"));
        return -1;
    }
    controller.handle(crate::app::Input::Right, &catalog);
    if controller.detail_focus() != 10 {
        ogc_system::report(format_args!("FAIL detail focus next boundary\n"));
        return -1;
    }

    ogc_system::report(format_args!(
        "PASS detail focus default, order and boundaries\n"
    ));
    0
}

fn catalog_reconcile_api() -> i32 {
    use crate::{app::AppController, catalog::AppEntry};

    let make_entry = |directory: String| AppEntry {
        directory,
        kind: crate::catalog::AppKind::Dol,
        size: 1,
        has_icon: false,
        metadata: None,
    };
    let mut catalog: Vec<_> = (0..7)
        .map(|index| make_entry(alloc::format!("app-{index}")))
        .collect();
    let mut controller = AppController::new();
    controller.reconcile_catalog(&catalog);
    controller.handle(crate::app::Input::NextPage, &catalog);
    controller.handle(crate::app::Input::Down, &catalog);
    if controller.selected_app.as_deref() != Some("app-6")
        || controller.selected_index() != 6
        || controller.page_start() != 5
    {
        ogc_system::report(format_args!("FAIL catalog reconcile initial selection\n"));
        return -1;
    }

    catalog.insert(0, make_entry(String::from("000-new-app")));
    controller.reconcile_catalog(&catalog);
    if controller.selected_app.as_deref() != Some("app-6")
        || controller.selected_index() != 7
        || controller.page_start() != 5
    {
        ogc_system::report(format_args!("FAIL catalog reconcile retained selection\n"));
        return -1;
    }

    catalog.retain(|entry| entry.directory != "app-6");
    controller.reconcile_catalog(&catalog);
    if controller.selected_app.as_deref() != Some("000-new-app")
        || controller.selected_index() != 0
        || controller.page_start() != 0
    {
        ogc_system::report(format_args!("FAIL catalog reconcile removed selection\n"));
        return -1;
    }

    controller.reconcile_catalog(&[]);
    if controller.selected_app.is_some()
        || controller.selected_index() != 0
        || controller.page_start() != 0
    {
        ogc_system::report(format_args!("FAIL catalog reconcile empty catalog\n"));
        return -1;
    }
    ogc_system::report(format_args!(
        "PASS catalog reconcile identity, page and removal behavior\n"
    ));
    0
}

fn expect_banner_error() -> Result<(), settings::SettingsIoError> {
    match settings::save(&expected_settings()) {
        Err(settings::SettingsIoError::Isfs(-1022)) => Ok(()),
        Err(error) => Err(error),
        Ok(()) => Err(settings::SettingsIoError::Codec),
    }
}

fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn launch(elf: bool) -> i32 {
    unsafe extern "C" {
        static hbc_acceptance_child_start: u8;
        static hbc_acceptance_child_end: u8;
    }
    let start = core::ptr::addr_of!(hbc_acceptance_child_start);
    let length = core::ptr::addr_of!(hbc_acceptance_child_end) as usize - start as usize;
    if length != 0x100 {
        return -22;
    }
    // The linked child is position independent and runs only after the actual
    // production trampoline has copied its image and initialized its memory.
    let payload = unsafe { core::slice::from_raw_parts(start, length) };

    let arena_hi_before = ogc_system::get_arena2_hi();
    if crate::loader::LaunchSource::try_allocate(0).is_some()
        || crate::loader::LaunchSource::try_allocate(usize::MAX).is_some()
    {
        ogc_system::report(format_args!(
            "FAIL launch source invalid allocation bounds\n"
        ));
        return -23;
    }
    let mut source_probe = match crate::loader::LaunchSource::try_allocate(64) {
        Some(source) => source,
        None => {
            ogc_system::report(format_args!("FAIL launch source Arena2 allocation\n"));
            return -24;
        }
    };
    let probe_bytes = [0x5a; 64];
    let probe_address = source_probe.as_mut_slice().as_mut_ptr() as usize;
    source_probe.as_mut_slice().copy_from_slice(&probe_bytes);
    if probe_address & 31 != 0 || source_probe.as_slice() != probe_bytes {
        ogc_system::report(format_args!("FAIL launch source aligned mutable view\n"));
        return -25;
    }
    drop(source_probe);
    if ogc_system::get_arena2_hi() != arena_hi_before {
        ogc_system::report(format_args!("FAIL launch source Arena2 release\n"));
        return -26;
    }

    let mut bytes = alloc::vec![0; 0x204];
    if elf {
        bytes[..7].copy_from_slice(b"\x7fELF\x01\x02\x01");
        bytes[16..20].copy_from_slice(&[0, 2, 0, 20]);
        put32(&mut bytes, 20, 1);
        put32(&mut bytes, 24, 0x80010000);
        put32(&mut bytes, 28, 52);
        bytes[40..46].copy_from_slice(&[0, 52, 0, 32, 0, 3]);
        for (index, (offset, address, size, memory, flags)) in [
            (0x100, 0x80010000, 0x100, 0x100, 5),
            (0x200, 0x80012000, 4, 8, 6),
            (0x204, 0x80013000, 0, 4, 6),
        ]
        .iter()
        .enumerate()
        {
            let base = 52 + index * 32;
            for (field, value) in [1, *offset, *address, *address, *size, *memory, *flags, 4]
                .iter()
                .enumerate()
            {
                put32(&mut bytes, base + field * 4, *value);
            }
        }
    } else {
        put32(&mut bytes, 0, 0x100);
        put32(&mut bytes, 0x48, 0x80010000);
        put32(&mut bytes, 0x90, 0x100);
        put32(&mut bytes, 0x1c, 0x200);
        put32(&mut bytes, 0x64, 0x80012000);
        put32(&mut bytes, 0xac, 4);
        put32(&mut bytes, 0xd8, 0x80012004);
        put32(&mut bytes, 0xdc, 0x1000);
        put32(&mut bytes, 0xe0, 0x80010000);
    }
    bytes[0x100..0x200].copy_from_slice(payload);
    put32(&mut bytes, 0x200, 0x12345678);
    let plan = if elf {
        loader::parse_elf32_ppc(&bytes, None)
    } else {
        loader::parse_dol(&bytes, None)
    };
    let plan = match plan {
        Ok(plan) => plan,
        Err(_) => return -22,
    };
    let args = match loader::build_launch_arguments("boot.dol", &[]) {
        Ok(args) => args,
        Err(_) => return -22,
    };
    let image = match loader::stage_launch(
        loader::LaunchSource::from_vec(bytes),
        &plan,
        args,
        0x80003400..0x8132fc00,
    ) {
        Ok(image) => image,
        Err(error) => {
            ogc_system::report(format_args!("FAIL stage {error:?}\n"));
            return -5;
        }
    };
    // Poison both zero-fill destinations. A missing copy/clear must fail.
    unsafe {
        core::ptr::write_volatile(0x80012004 as *mut u32, 0xa5a5a5a5);
        core::ptr::write_volatile(0x80013000 as *mut u32, 0xa5a5a5a5);
        loader::transfer_control(
            image,
            loader::IosLaunchPolicy {
                reload: false,
                ahb_access: false,
            },
        )
    }
}

core::arch::global_asm!(include_str!("acceptance_child.S"));
