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
