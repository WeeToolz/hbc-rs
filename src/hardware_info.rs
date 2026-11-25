//! Reborn `HBCH` hardware information request support.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{self, Write as _};

pub const PAGE_NAMES: [&str; 5] = ["System", "Video", "Storage", "USB", "Network"];
const PAGE_MASK: u16 = (1 << PAGE_NAMES.len()) - 1;
const RESPONSE_CAPACITY: usize = 8192;

pub const fn selected_pages(mask: u16) -> [bool; PAGE_NAMES.len()] {
    let mask = if mask == 0 { PAGE_MASK } else { mask };
    let mut selected = [false; PAGE_NAMES.len()];
    let mut index = 0;
    while index < PAGE_NAMES.len() {
        selected[index] = mask & (1 << index) != 0;
        index += 1;
    }
    selected
}

#[derive(Debug, Eq, PartialEq)]
pub enum BuildError {
    NoMemory,
    TooLarge,
}

#[cfg(target_arch = "powerpc")]
static USB_INFO_STARTED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

#[cfg(target_arch = "powerpc")]
#[repr(align(32))]
struct AlignedMac([u8; 6]);

#[cfg(target_arch = "powerpc")]
#[repr(align(32))]
struct AlignedSetting([u8; 256]);

/// Gather the hardware fields exposed by the current libogc-rs wrappers.
/// Unsupported Reborn fields are left out instead of reporting invented values.
#[cfg(target_arch = "powerpc")]
pub fn hardware_response(mask: u16) -> Result<Vec<u8>, BuildError> {
    build_response(mask, |index, object| match index {
        0 => system_page(object),
        1 => video_page(object),
        2 => storage_page(object),
        3 => usb_page(object),
        4 => network_page(object),
        _ => {}
    })
}

#[cfg(target_arch = "powerpc")]
fn system_page(page: &mut JsonObject<'_>) {
    let _ = ogc_conf::init();
    let mut model = [0; 32];
    let model = ogc_conf::model_name(&mut model)
        .ok()
        .and_then(|len| core::str::from_utf8(model.get(..len)?).ok());
    let wii_u = ogc_system::has_ahbprot()
        // SAFETY: C Reborn checks AHBPROT before reading the Wii U hardware
        // register; this guarded volatile read follows the same condition.
        && unsafe { core::ptr::read_volatile(0x0d80_05a0 as *const u32) >> 16 == 0xcafe };
    if wii_u {
        page.formatted(
            "Console",
            format_args!(
                "Wii U, in Wii mode{}{}",
                if model.is_some() { " " } else { "" },
                model.unwrap_or("")
            ),
        );
    } else {
        page.formatted(
            "Console",
            format_args!("Wii {}", model.unwrap_or("(model unknown)")),
        );
    }
    if let Some(serial) = read_console_serial() {
        page.string("Serial", &serial);
    }
    let region = match ogc_conf::get_region() {
        value if value == ogc_conf::CONF_REGION_JP as i32 => "Japan",
        value if value == ogc_conf::CONF_REGION_US as i32 => "USA",
        value if value == ogc_conf::CONF_REGION_EU as i32 => "Europe",
        value if value == ogc_conf::CONF_REGION_KR as i32 => "Korea",
        value if value == ogc_conf::CONF_REGION_CN as i32 => "China",
        _ => "unknown",
    };
    page.string("Region", region);
    page.formatted(
        "IOS",
        format_args!(
            "IOS{} v{}, AHBPROT {}",
            ogc_ios::get_version(),
            ogc_ios::get_revision(),
            if ogc_system::has_ahbprot() {
                "open"
            } else {
                "closed"
            }
        ),
    );
    let mut console_id = 0u32;
    // SAFETY: ES writes one u32 to this live stack value.
    if unsafe { ogc_es_sys::get_device_id(&mut console_id) } >= 0 {
        page.formatted(
            "Console ID",
            format_args!("{console_id} ({console_id:08x})"),
        );
    }
    if let Some(version) = system_menu_version() {
        page.formatted("Wii Menu", format_args!("{version}"));
    }
    let mut boot2 = 0u32;
    // SAFETY: ES writes one u32 to this live stack value.
    if unsafe { ogc_es_sys::get_boot2_version(&mut boot2) } >= 0 {
        page.formatted("boot2", format_args!("v{boot2}"));
    }
    let pvr: u32;
    // SAFETY: Reading the PowerPC PVR special-purpose register is read-only.
    unsafe { core::arch::asm!("mfspr {0}, 287", out(reg) pvr, options(nostack, nomem)) };
    page.formatted(
        "CPU",
        format_args!(
            "{}, 486 MHz (PVR {pvr:08x})",
            if pvr >> 16 == 0x7001 {
                "Espresso"
            } else {
                "Broadway"
            }
        ),
    );
    page.formatted(
        "Hollywood",
        format_args!("revision {:08x}", ogc_system::get_hollywood_revision()),
    );
    const LANGUAGES: [&str; 10] = [
        "Japanese",
        "English",
        "German",
        "French",
        "Spanish",
        "Italian",
        "Dutch",
        "Chinese (simplified)",
        "Chinese (traditional)",
        "Korean",
    ];
    let language = ogc_conf::get_language();
    page.string(
        "Language",
        usize::try_from(language)
            .ok()
            .and_then(|index| LANGUAGES.get(index))
            .copied()
            .unwrap_or("unknown"),
    );
}

#[cfg(target_arch = "powerpc")]
fn read_console_serial() -> Option<String> {
    if ogc_filesystem::initialize() < 0 {
        return None;
    }
    let file = ogc_filesystem::open(
        c"/title/00000001/00000002/data/setting.txt",
        ogc_filesystem::ISFS_OPEN_READ,
    )
    .ok()?;
    let mut contents = AlignedSetting([0; 256]);
    let read = file.read(&mut contents.0);
    let _ = file.close();
    if read.ok()? != contents.0.len() {
        return None;
    }
    let mut key = 0x73b5_dbfau32;
    for byte in &mut contents.0 {
        *byte ^= key as u8;
        key = key.rotate_left(1);
    }
    contents.0[255] = 0;
    let text_len = contents.0.iter().position(|byte| *byte == 0).unwrap_or(255);
    let code = setting_value(&contents.0[..text_len], b"CODE").unwrap_or("");
    let serial = setting_value(&contents.0[..text_len], b"SERNO").unwrap_or("");
    (!code.is_empty() || !serial.is_empty()).then(|| alloc::format!("{code}{serial}"))
}

#[cfg(any(target_arch = "powerpc", test))]
fn setting_value<'a>(text: &'a [u8], key: &[u8]) -> Option<&'a str> {
    let text = text.split(|byte| *byte == 0).next()?;
    text.split(|byte| *byte == b'\n').find_map(|line| {
        let split = line.iter().position(|byte| *byte == b'=')?;
        let (name, value) = line.split_at(split);
        let value = value.get(1..)?.strip_suffix(b"\r").unwrap_or(&value[1..]);
        (name == key)
            .then(|| core::str::from_utf8(value).ok())
            .flatten()
    })
}

#[cfg(target_arch = "powerpc")]
#[repr(align(32))]
struct AlignedTmd([u8; 0x4000]);

#[cfg(target_arch = "powerpc")]
fn system_menu_version() -> Option<String> {
    const SYSTEM_MENU: u64 = 0x0000_0001_0000_0002;
    let mut size = 0u32;
    // SAFETY: ES writes a u32 size value to this stack location.
    if unsafe { ogc_es_sys::get_stored_tmd_size(SYSTEM_MENU, &mut size) } < 0
        || !(0x1e0..=0x4000).contains(&size)
    {
        return None;
    }
    let mut tmd = AlignedTmd([0; 0x4000]);
    // SAFETY: The aligned buffer exceeds the validated TMD size.
    if unsafe { ogc_es_sys::get_stored_tmd(SYSTEM_MENU, tmd.0.as_mut_ptr().cast(), size) } < 0 {
        return None;
    }
    let version = u16::from_be_bytes([tmd.0[0x1dc], tmd.0[0x1dd]]);
    let name = match version {
        512 => Some("4.3J"),
        513 => Some("4.3U"),
        514 => Some("4.3E"),
        518 => Some("4.3K"),
        480 => Some("4.2J"),
        481 => Some("4.2U"),
        482 => Some("4.2E"),
        486 => Some("4.2K"),
        448 => Some("4.1J"),
        449 => Some("4.1U"),
        450 => Some("4.1E"),
        454 => Some("4.1K"),
        416 => Some("4.0J"),
        417 => Some("4.0U"),
        418 => Some("4.0E"),
        384 => Some("3.4J"),
        385 => Some("3.4U"),
        386 => Some("3.4E"),
        390 => Some("3.5K"),
        608 => Some("4.3J (vWii)"),
        609 => Some("4.3U (vWii)"),
        610 => Some("4.3E (vWii)"),
        4609 => Some("4.3U (Wii mini)"),
        4610 => Some("4.3E (Wii mini)"),
        _ => None,
    };
    Some(match name {
        Some(name) => alloc::format!("{name} (v{version})"),
        None => alloc::format!("v{version}"),
    })
}

#[cfg(target_arch = "powerpc")]
fn video_page(page: &mut JsonObject<'_>) {
    let mut mode = ogc_video::GxrModeObj {
        vi_tv_mode: 0,
        fb_width: 0,
        efb_height: 0,
        xfb_height: 0,
        vi_x_origin: 0,
        vi_y_origin: 0,
        vi_width: 0,
        vi_height: 0,
        xfb_mode: 0,
        field_rendering: 0,
        aa: 0,
        sample_pattern: [[0; 2]; 12],
        vfilter: [0; 7],
    };
    // SAFETY: `mode` is a writable mode object for the wrapper to fill.
    if !unsafe { ogc_video::get_preferred_mode(&mut mode) }.is_null() {
        let format = match mode.vi_tv_mode >> 2 {
            0 => "NTSC",
            1 => "PAL",
            2 => "MPAL",
            3 => "debug",
            4 => "debug PAL",
            5 => "PAL 60",
            _ => "?",
        };
        let scan = match mode.vi_tv_mode & 3 {
            0 => "interlaced",
            2 => "progressive",
            _ => "240p",
        };
        page.formatted(
            "Mode",
            format_args!(
                "{}x{}, {scan}, {} Hz ({format})",
                mode.fb_width,
                mode.xfb_height,
                if mode.vi_tv_mode >> 2 == 1 { 50 } else { 60 }
            ),
        );
    }
    page.string(
        "Cable",
        if ogc_video::have_component_cable() != 0 {
            "Component"
        } else {
            "Composite (or none)"
        },
    );
    let video = ogc_conf::get_video();
    page.string(
        "TV type",
        if video == ogc_conf::CONF_VIDEO_PAL as i32 {
            "PAL"
        } else if video == ogc_conf::CONF_VIDEO_MPAL as i32 {
            "MPAL"
        } else {
            "NTSC"
        },
    );
    page.string(
        "Screen",
        if ogc_conf::get_aspect_ratio() == ogc_conf::CONF_ASPECT_16_9 as i32 {
            "16:9"
        } else {
            "4:3"
        },
    );
    page.string(
        "480p",
        if ogc_conf::get_progressive_scan() > 0 {
            "On"
        } else {
            "Off"
        },
    );
    if video == ogc_conf::CONF_VIDEO_PAL as i32 {
        page.string(
            "PAL 60",
            if ogc_conf::get_eu_rgb60() > 0 {
                "On"
            } else {
                "Off"
            },
        );
    }
    const SOUNDS: [&str; 3] = ["Mono", "Stereo", "Surround"];
    let sound = ogc_conf::get_sound_mode();
    page.string(
        "Sound",
        usize::try_from(sound)
            .ok()
            .and_then(|index| SOUNDS.get(index))
            .copied()
            .unwrap_or("unknown"),
    );
    let position = if ogc_conf::get_sensor_bar_position() == ogc_conf::CONF_SENSORBAR_TOP as i32 {
        "Above the TV"
    } else {
        "Below the TV"
    };
    page.formatted(
        "Sensor bar",
        format_args!(
            "{position}, sensitivity {} of 5",
            ogc_conf::get_ir_sensitivity() + 1
        ),
    );
    let mut pads = ogc_conf::ConfPads::default();
    if ogc_conf::get_pad_devices(&mut pads) >= 0 {
        // SAFETY: ConfPads is packed; an unaligned read preserves its C layout.
        let registered = unsafe { core::ptr::addr_of!(pads.num_registered).read_unaligned() };
        page.formatted("Paired remotes", format_args!("{registered}"));
    }
}

#[cfg(target_arch = "powerpc")]
fn storage_page(page: &mut JsonObject<'_>) {
    for (label, mount) in [
        ("SD card", c"sd:"),
        ("USB drive", c"usb:"),
        ("USB drive 2", c"usb2:"),
    ] {
        if mount == c"usb2:" && !ogc_fat::is_mounted(mount) {
            continue;
        }
        page.string(
            label,
            if ogc_fat::is_mounted(mount) {
                "Mounted by the app; size unavailable"
            } else {
                "Not mounted by the app"
            },
        );
    }
    let mut stats = ogc_filesystem::IsfsStats::default();
    if ogc_filesystem::initialize() >= 0 && ogc_filesystem::get_stats(&mut stats) >= 0 {
        let cluster = if stats.a == 0 { 0x4000 } else { stats.a };
        let free = u64::from(stats.b);
        let used = u64::from(stats.c);
        page.formatted(
            "NAND",
            format_args!(
                "{:.1} MB free of {:.1} MB",
                free as f64 * f64::from(cluster) / 1_048_576.0,
                (free + used) as f64 * f64::from(cluster) / 1_048_576.0
            ),
        );
        page.formatted(
            "NAND blocks",
            format_args!(
                "{} free (the Wii Menu's 128 KiB)",
                free * u64::from(cluster) / 131_072
            ),
        );
        page.formatted(
            "NAND files",
            format_args!("{} free of {}", stats.f, stats.f + stats.g),
        );
    }
}

#[cfg(target_arch = "powerpc")]
fn usb_page(page: &mut JsonObject<'_>) {
    const KNOWN: &[(u16, u16, &str)] = &[
        (0x057e, 0x0305, "Wii's own Bluetooth"),
        (0x057e, 0x0337, "GameCube controller adapter"),
        (0x057e, 0x0306, "Wii Remote"),
        (0x0b05, 0x1786, "Wi-Fi adapter"),
        (0x0d8c, 0x000c, "Audio adapter"),
        (0x046d, 0x0a03, "Logitech microphone"),
        (0x1430, 0x0150, "Guitar Hero dongle"),
        (0x12ba, 0x0100, "Rock Band guitar"),
        (0x1bad, 0x0004, "Rock Band guitar"),
        (0x057e, 0x0308, "Wii Speak"),
    ];
    use core::sync::atomic::Ordering;

    if !USB_INFO_STARTED.swap(true, Ordering::AcqRel) {
        if ogc_usb::initialize() >= 0 {
            ogc_lwp::sys::sleep_ticks(ogc_lwp::sys::TICKS_PER_SECOND * 3 / 10);
        }
    }
    let mut entries = [ogc_usb::UsbDeviceEntry::default(); 32];
    let mut hid_entries = [ogc_usb::UsbDeviceEntry::default(); 32];
    let hid_count =
        ogc_usb::device_list(&mut hid_entries, ogc_usb::USB_CLASS_HID).unwrap_or_default();
    match ogc_usb::device_list(&mut entries, 0) {
        Ok(count) if count == 0 => page.string("USB", "Nothing plugged in"),
        Ok(count) => {
            for entry in entries.iter().take(count) {
                let hid = hid_entries
                    .iter()
                    .take(hid_count)
                    .any(|item| item.device_id == entry.device_id);
                let name = KNOWN
                    .iter()
                    .find(|(vid, pid, _)| *vid == entry.vid && *pid == entry.pid)
                    .map(|(_, _, name)| *name);
                page.formatted(
                    &alloc::format!("{:04x}:{:04x}", entry.vid, entry.pid),
                    format_args!(
                        "{}{}{}",
                        if hid { "HID" } else { "Device" },
                        if name.is_some() { ", " } else { "" },
                        name.unwrap_or("")
                    ),
                );
            }
        }
        Err(_) => page.formatted(
            "USB",
            format_args!("Not available on IOS{}", ogc_ios::get_version()),
        ),
    }
}

#[cfg(target_arch = "powerpc")]
fn network_page(page: &mut JsonObject<'_>) {
    let status = ogc_network::get_status();
    if status == 0 {
        let ip = ogc_network::get_host_ip();
        page.formatted(
            "IP",
            format_args!(
                "{}.{}.{}.{}",
                ogc_network::ip4_addr1(ip),
                ogc_network::ip4_addr2(ip),
                ogc_network::ip4_addr3(ip),
                ogc_network::ip4_addr4(ip)
            ),
        );
    } else {
        page.string("IP", "Not connected");
    }
    let mut mac = AlignedMac([0; 6]);
    // SAFETY: `mac` is a writable six-byte output buffer for the wrapper.
    if unsafe { ogc_network::get_mac_address(mac.0.as_mut_ptr().cast()) } >= 0 {
        page.formatted(
            "MAC",
            format_args!(
                "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                mac.0[0], mac.0[1], mac.0[2], mac.0[3], mac.0[4], mac.0[5]
            ),
        );
    }
}

/// Build the JSON reply while the caller supplies page contents in Reborn order.
/// A page may be empty when the current platform wrapper does not expose a field.
pub fn build_response(
    mask: u16,
    mut page: impl FnMut(usize, &mut JsonObject<'_>),
) -> Result<Vec<u8>, BuildError> {
    let selected = selected_pages(mask);
    let mut json = String::new();
    json.try_reserve_exact(RESPONSE_CAPACITY)
        .map_err(|_| BuildError::NoMemory)?;
    json.push('{');
    let mut first_page = true;
    for (index, name) in PAGE_NAMES.iter().enumerate() {
        if !selected[index] {
            continue;
        }
        if !first_page {
            json.push(',');
        }
        first_page = false;
        push_quoted(&mut json, name);
        json.push_str(":{");
        let mut object = JsonObject {
            json: &mut json,
            first: true,
        };
        page(index, &mut object);
        object.json.push('}');
    }
    json.push('}');
    if json.len() >= RESPONSE_CAPACITY {
        return Err(BuildError::TooLarge);
    }
    Ok(json.into_bytes())
}

pub struct JsonObject<'a> {
    json: &'a mut String,
    first: bool,
}

impl JsonObject<'_> {
    pub fn string(&mut self, key: &str, value: &str) {
        self.entry_prefix(key);
        push_quoted(self.json, value);
    }

    pub fn formatted(&mut self, key: &str, value: fmt::Arguments<'_>) {
        self.entry_prefix(key);
        self.json.push('"');
        let mut value_writer = JsonValueWriter(self.json);
        let _ = value_writer.write_fmt(value);
        value_writer.0.push('"');
    }

    fn entry_prefix(&mut self, key: &str) {
        if !self.first {
            self.json.push(',');
        }
        self.first = false;
        push_quoted(self.json, key);
        self.json.push(':');
    }
}

struct JsonValueWriter<'a>(&'a mut String);

impl fmt::Write for JsonValueWriter<'_> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        for character in value.chars() {
            match character {
                '"' => self.0.push('\''),
                '\\' => self.0.push('/'),
                character if character.is_control() => self.0.push(' '),
                character => self.0.push(character),
            }
        }
        Ok(())
    }
}

fn push_quoted(json: &mut String, value: &str) {
    json.push('"');
    for character in value.chars() {
        match character {
            '"' => json.push('\''),
            '\\' => json.push('/'),
            character if character.is_control() => json.push(' '),
            character => json.push(character),
        }
    }
    json.push('"');
}

#[cfg(test)]
mod tests {
    use super::{BuildError, PAGE_NAMES, build_response, selected_pages};

    #[test]
    fn zero_mask_selects_all_reborn_pages_in_order() {
        assert_eq!(selected_pages(0), [true; 5]);
        assert_eq!(
            build_response(0, |_, _| {}).unwrap(),
            br#"{"System":{},"Video":{},"Storage":{},"USB":{},"Network":{}}"#
        );
    }

    #[test]
    fn mask_selects_only_requested_pages_and_keeps_order() {
        assert_eq!(
            selected_pages((1 << 1) | (1 << 4)),
            [false, true, false, false, true]
        );
        assert_eq!(
            build_response((1 << 1) | (1 << 4), |index, page| {
                page.string("name", PAGE_NAMES[index]);
            })
            .unwrap(),
            br#"{"Video":{"name":"Video"},"Network":{"name":"Network"}}"#
        );
    }

    #[test]
    fn json_values_follow_reborn_quote_and_control_character_rules() {
        let result = build_response(1, |_, page| {
            page.string("text", "quote: \" slash: \\ control:\n");
        })
        .unwrap();
        assert_eq!(
            result,
            br#"{"System":{"text":"quote: ' slash: / control: "}}"#
        );
    }

    #[test]
    fn oversized_reply_is_rejected() {
        let result = build_response(1, |_, page| {
            page.string("value", &"x".repeat(8200));
        });
        assert_eq!(result, Err(BuildError::TooLarge));
    }

    #[test]
    fn setting_values_ignore_non_utf8_bytes_after_console_text() {
        let setting = b"CODE=LU\r\nSERNO=372730284\r\n\xf4\xe9\xd3";
        assert_eq!(super::setting_value(setting, b"CODE"), Some("LU"));
        assert_eq!(super::setting_value(setting, b"SERNO"), Some("372730284"));
    }

    #[test]
    fn setting_values_stop_at_the_c_string_terminator() {
        let setting = b"CODE=LU\0\nCODE=BAD\n";
        assert_eq!(super::setting_value(setting, b"CODE"), Some("LU"));
    }

    #[test]
    fn formatted_json_preserves_colons_and_zero_padded_hex_bytes() {
        let result = build_response(1, |_, page| {
            page.formatted("MAC", format_args!("{:02x}:{:02x}", 0x00u8, 0x17u8));
        })
        .unwrap();
        assert_eq!(result, br#"{"System":{"MAC":"00:17"}}"#);
    }
}
