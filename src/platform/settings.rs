//! NAND-backed HBC settings storage through ES and ISFS Rust APIs.

use alloc::ffi::CString;
use core::str;

use crate::metadata::UserSettings;
use crate::settings::{
    decode_settings_xml, encode_settings_xml, MAX_SETTINGS_XML_SIZE, SETTINGS_PATH,
};

use ogc_es_sys as es;
use ogc_filesystem as isfs;

const DATA_DIR_BUFFER_LEN: usize = 32;

#[repr(align(32))]
struct AlignedTitleId(u64);
#[repr(align(32))]
struct AlignedDataDir([u8; DATA_DIR_BUFFER_LEN]);
#[repr(align(32))]
struct AlignedFstats(isfs::Fstats);
#[repr(align(32))]
struct AlignedXml([u8; MAX_SETTINGS_XML_SIZE]);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsIoError {
    Es(i32),
    Isfs(i32),
    InvalidPath,
    InvalidFileSize,
    Codec,
}

pub(crate) fn title_data_file_path(name: &str) -> Result<CString, SettingsIoError> {
    if name.is_empty() || name.contains('/') || name.contains('\\') {
        return Err(SettingsIoError::InvalidPath);
    }
    let mut title_id = AlignedTitleId(0);
    let rc = unsafe { es::get_title_id(&mut title_id.0) };
    if rc < 0 {
        return Err(SettingsIoError::Es(rc));
    }
    let mut data_dir = AlignedDataDir([0; DATA_DIR_BUFFER_LEN]);
    let rc = unsafe { es::get_data_dir(title_id.0, data_dir.0.as_mut_ptr()) };
    if rc < 0 {
        return Err(SettingsIoError::Es(rc));
    }
    let end = data_dir
        .0
        .iter()
        .position(|byte| *byte == 0)
        .ok_or(SettingsIoError::InvalidPath)?;
    let directory = str::from_utf8(&data_dir.0[..end]).map_err(|_| SettingsIoError::InvalidPath)?;
    CString::new(alloc::format!("{directory}/{name}")).map_err(|_| SettingsIoError::InvalidPath)
}

fn settings_path() -> Result<CString, SettingsIoError> {
    title_data_file_path(SETTINGS_PATH)
}

/// Load the settings file from the running channel's ES data directory.
pub fn load() -> Result<UserSettings, SettingsIoError> {
    let path = settings_path()?;
    let rc = isfs::initialize();
    if rc < 0 {
        return Err(SettingsIoError::Isfs(rc));
    }
    let file = isfs::open(&path, isfs::ISFS_OPEN_READ).map_err(SettingsIoError::Isfs)?;
    let mut stats = AlignedFstats(isfs::Fstats::default());
    let rc = isfs::get_file_stats(&file, &mut stats.0);
    if rc < 0 {
        let _ = file.close();
        return Err(SettingsIoError::Isfs(rc));
    }
    let size = stats.0.file_length as usize;
    if size == 0 || size > MAX_SETTINGS_XML_SIZE {
        let _ = file.close();
        return Err(SettingsIoError::InvalidFileSize);
    }

    let mut xml = AlignedXml([0; MAX_SETTINGS_XML_SIZE]);
    let mut offset = 0;
    while offset < size {
        match file.read(&mut xml.0[offset..size]) {
            Ok(0) => {
                let _ = file.close();
                return Err(SettingsIoError::InvalidFileSize);
            }
            Ok(count) => offset += count,
            Err(rc) => {
                let _ = file.close();
                return Err(SettingsIoError::Isfs(rc));
            }
        }
    }
    let close_rc = file.close();
    if close_rc < 0 {
        return Err(SettingsIoError::Isfs(close_rc));
    }
    decode_settings_xml(&xml.0[..size]).map_err(|_| SettingsIoError::Codec)
}

/// Replace the settings XML file. ISFS has no truncate open flag.
pub fn save(settings: &UserSettings) -> Result<(), SettingsIoError> {
    let path = settings_path()?;
    let xml = encode_settings_xml(settings).map_err(|_| SettingsIoError::Codec)?;
    let rc = isfs::initialize();
    if rc < 0 {
        return Err(SettingsIoError::Isfs(rc));
    }
    // Match HBC isfs_put: remove the old file before creating the new file.
    let _ = isfs::delete(&path);
    let rc = isfs::create_file(&path, 0, 3, 3, 0);
    if rc < 0 {
        return Err(SettingsIoError::Isfs(rc));
    }
    let file = match isfs::open(&path, isfs::ISFS_OPEN_RW) {
        Ok(file) => file,
        Err(rc) => {
            let _ = isfs::delete(&path);
            return Err(SettingsIoError::Isfs(rc));
        }
    };
    let mut buffer = AlignedXml([0; MAX_SETTINGS_XML_SIZE]);
    buffer.0[..xml.len()].copy_from_slice(xml.as_bytes());
    let write_result = file.write_all(&buffer.0[..xml.len()]);
    let close_rc = file.close();
    if let Err(rc) = write_result {
        let _ = isfs::delete(&path);
        return Err(SettingsIoError::Isfs(rc));
    }
    if close_rc < 0 {
        let _ = isfs::delete(&path);
        return Err(SettingsIoError::Isfs(close_rc));
    }
    Ok(())
}
