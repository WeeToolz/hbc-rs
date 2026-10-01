//! NAND-backed HBC settings storage through ES and ISFS Rust APIs.

use alloc::ffi::CString;
use core::cell::UnsafeCell;
use core::str;
use core::sync::atomic::{AtomicI32, AtomicU8, Ordering};

use crate::metadata::UserSettings;
use crate::settings::{
    MAX_SETTINGS_XML_SIZE, SETTINGS_PATH, decode_settings_xml, encode_settings_xml,
};

use ogc_es_sys as es;
use ogc_filesystem as isfs;

const DATA_DIR_BUFFER_LEN: usize = 30;

const TITLE_DIR_UNINITIALIZED: u8 = 0;
const TITLE_DIR_INITIALIZING: u8 = 1;
const TITLE_DIR_READY: u8 = 2;
const TITLE_DIR_FAILED: u8 = 3;
const INVALID_TITLE_DIR_ERROR: i32 = i32::MIN;

struct TitleDataDirectoryCache {
    state: AtomicU8,
    error: AtomicI32,
    directory: UnsafeCell<[u8; DATA_DIR_BUFFER_LEN]>,
}

// The state transition publishes the directory before any reader can access it.
unsafe impl Sync for TitleDataDirectoryCache {}

impl TitleDataDirectoryCache {
    const fn new() -> Self {
        Self {
            state: AtomicU8::new(TITLE_DIR_UNINITIALIZED),
            error: AtomicI32::new(INVALID_TITLE_DIR_ERROR),
            directory: UnsafeCell::new([0; DATA_DIR_BUFFER_LEN]),
        }
    }

    fn get_or_init(
        &self,
        resolve: impl FnOnce() -> Result<TitleDataDirectory, SettingsIoError>,
    ) -> Result<TitleDataDirectory, SettingsIoError> {
        let mut resolve = Some(resolve);
        loop {
            match self.state.load(Ordering::Acquire) {
                TITLE_DIR_READY => {
                    let bytes = unsafe { *self.directory.get() };
                    return TitleDataDirectory::from_es_buffer(bytes);
                }
                TITLE_DIR_FAILED => {
                    let error = self.error.load(Ordering::Relaxed);
                    return if error == INVALID_TITLE_DIR_ERROR {
                        Err(SettingsIoError::InvalidPath)
                    } else {
                        Err(SettingsIoError::Es(error))
                    };
                }
                TITLE_DIR_INITIALIZING => core::hint::spin_loop(),
                TITLE_DIR_UNINITIALIZED => {
                    if self
                        .state
                        .compare_exchange(
                            TITLE_DIR_UNINITIALIZED,
                            TITLE_DIR_INITIALIZING,
                            Ordering::Acquire,
                            Ordering::Relaxed,
                        )
                        .is_err()
                    {
                        continue;
                    }
                    match resolve.take().expect("cache resolver used once")() {
                        Ok(directory) => {
                            unsafe { *self.directory.get() = directory.bytes };
                            self.state.store(TITLE_DIR_READY, Ordering::Release);
                        }
                        Err(SettingsIoError::Es(error)) => {
                            self.error.store(error, Ordering::Relaxed);
                            self.state.store(TITLE_DIR_FAILED, Ordering::Release);
                        }
                        Err(_) => {
                            self.error.store(INVALID_TITLE_DIR_ERROR, Ordering::Relaxed);
                            self.state.store(TITLE_DIR_FAILED, Ordering::Release);
                        }
                    }
                }
                _ => unreachable!(),
            }
        }
    }
}

static TITLE_DIR_CACHE: TitleDataDirectoryCache = TitleDataDirectoryCache::new();

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

#[derive(Clone, Copy)]
struct TitleDataDirectory {
    bytes: [u8; DATA_DIR_BUFFER_LEN],
    len: usize,
}

impl TitleDataDirectory {
    fn from_es_buffer(bytes: [u8; DATA_DIR_BUFFER_LEN]) -> Result<Self, SettingsIoError> {
        let len = bytes
            .iter()
            .position(|byte| *byte == 0)
            .ok_or(SettingsIoError::InvalidPath)?;
        str::from_utf8(&bytes[..len]).map_err(|_| SettingsIoError::InvalidPath)?;
        Ok(Self { bytes, len })
    }

    fn file_path(self, name: &str) -> Result<CString, SettingsIoError> {
        if name.is_empty() || name.contains('/') || name.contains('\\') {
            return Err(SettingsIoError::InvalidPath);
        }
        let directory =
            str::from_utf8(&self.bytes[..self.len]).map_err(|_| SettingsIoError::InvalidPath)?;
        CString::new(alloc::format!("{directory}/{name}")).map_err(|_| SettingsIoError::InvalidPath)
    }
}

fn resolve_title_data_directory() -> Result<TitleDataDirectory, SettingsIoError> {
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
    TitleDataDirectory::from_es_buffer(data_dir.0)
}

fn title_data_directory() -> Result<TitleDataDirectory, SettingsIoError> {
    TITLE_DIR_CACHE.get_or_init(resolve_title_data_directory)
}

pub(crate) fn title_data_file_path(name: &str) -> Result<CString, SettingsIoError> {
    title_data_directory()?.file_path(name)
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
    super::nand::recover_wii(&path)?;
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

/// Replace the settings XML file without deleting the previous valid copy.
pub fn save(settings: &UserSettings) -> Result<(), SettingsIoError> {
    let path = settings_path()?;
    let xml = encode_settings_xml(settings).map_err(|_| SettingsIoError::Codec)?;
    let rc = isfs::initialize();
    if rc < 0 {
        return Err(SettingsIoError::Isfs(rc));
    }
    super::nand::save_wii(&path, xml.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::{SettingsIoError, TitleDataDirectory, TitleDataDirectoryCache};
    use core::cell::Cell;

    fn title_data_directory() -> TitleDataDirectory {
        let mut bytes = [0; 30];
        bytes[..29].copy_from_slice(b"/title/00010001/48424345/data");
        TitleDataDirectory::from_es_buffer(bytes).unwrap()
    }

    #[test]
    fn settings_and_theme_paths_share_one_cached_title_directory() {
        let cache = TitleDataDirectoryCache::new();
        let calls = Cell::new(0);
        let first = cache
            .get_or_init(|| {
                calls.set(calls.get() + 1);
                Ok(title_data_directory())
            })
            .unwrap();
        let second = cache
            .get_or_init(|| {
                calls.set(calls.get() + 1);
                Ok(title_data_directory())
            })
            .unwrap();

        assert_eq!(calls.get(), 1);
        assert_eq!(
            first.file_path("settings.xml").unwrap().as_bytes(),
            b"/title/00010001/48424345/data/settings.xml"
        );
        assert_eq!(
            second.file_path("theme.zip").unwrap().as_bytes(),
            b"/title/00010001/48424345/data/theme.zip"
        );
    }

    #[test]
    fn title_directory_errors_are_cached() {
        let cache = TitleDataDirectoryCache::new();
        let calls = Cell::new(0);
        for _ in 0..2 {
            assert_eq!(
                cache.get_or_init(|| {
                    calls.set(calls.get() + 1);
                    Err(SettingsIoError::Es(-7))
                }),
                Err(SettingsIoError::Es(-7))
            );
        }
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn invalid_title_data_paths_are_rejected() {
        let directory = title_data_directory();
        for name in [
            "",
            "../settings.xml",
            "folder/theme.zip",
            "folder\\theme.zip",
        ] {
            assert_eq!(directory.file_path(name), Err(SettingsIoError::InvalidPath));
        }
        assert_eq!(
            TitleDataDirectory::from_es_buffer([b'a'; 30]),
            Err(SettingsIoError::InvalidPath)
        );
    }
}
