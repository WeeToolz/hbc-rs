//! Emulator acceptance entry. This module is absent from normal HBC builds.
use crate::{loader, metadata::UserSettings, platform::settings};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

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
    if case == b"loader-helpers" {
        return loader_helpers();
    }
    if case == b"app-controller" {
        return app_controller();
    }
    if case == b"storage-volumes" {
        return storage_volumes();
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
    if case == b"disc-stop" {
        return disc_stop();
    }
    let result = match case {
        b"settings-save" => settings::save(&expected_settings()),
        b"settings-load" => settings::load().and_then(|actual| {
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

fn app_deletion() -> i32 {
    use crate::management::plan_app_deletion;

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
    ogc_system::report(format_args!("PASS app-deletion safe folder validation\n"));
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
        || entries[1].directory != "beta"
        || entries[1].kind != AppKind::Dol
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
        || !visible_entries(&entries, Filter::IconsOnly, SortOrder::Name).is_empty()
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
    if navigation.capacity != 5
        || navigation.start != 0
        || !navigation.has_next()
        || navigation.has_previous()
        || controller.handle(Input::NextPage, &catalog) != Action::NextPage
        || controller.selected_app.as_deref() != Some("app-5")
        || controller.page_start() != 5
        || controller.handle(Input::NextPage, &catalog) != Action::None
        || controller.handle(Input::PreviousPage, &catalog) != Action::PreviousPage
        || controller.selected_app.as_deref() != Some("app-0")
    {
        ogc_system::report(format_args!("FAIL app controller page navigation\n"));
        return -1;
    }
    controller.handle(Input::ToggleBrowseMode, &catalog);
    controller.handle(Input::Down, &catalog);
    if controller.browser_layout().columns != 3
        || controller.selected_app.as_deref() != Some("app-3")
        || controller.visible_page(&catalog).len() != 7
    {
        ogc_system::report(format_args!("FAIL app controller grid navigation\n"));
        return -1;
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
        return -1;
    }
    ogc_system::report(format_args!(
        "PASS app controller page, grid and pointer navigation\n"
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
