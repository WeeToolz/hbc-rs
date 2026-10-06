//! NAND-backed HBC settings storage through ES and ISFS Rust APIs.

use alloc::ffi::CString;

use crate::metadata::UserSettings;
use crate::settings::{
    decode_settings_xml, encode_settings_xml, MAX_SETTINGS_XML_SIZE, SETTINGS_PATH,
};

use ogc_es_sys as es;
use ogc_filesystem as isfs;

use crate::title_path::{
    CachedTitleDataDirectory, TitleDataDirectory, TitlePathError, TITLE_DATA_DIR_BUFFER_LEN,
};

static TITLE_DIR_CACHE: CachedTitleDataDirectory = CachedTitleDataDirectory::new();

#[repr(align(32))]
struct AlignedTitleId(u64);
#[repr(align(32))]
struct AlignedDataDir([u8; TITLE_DATA_DIR_BUFFER_LEN]);
#[repr(align(32))]
struct AlignedFstats(isfs::Fstats);
#[repr(align(32))]
struct AlignedXml([u8; MAX_SETTINGS_XML_SIZE]);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsIoError {
    Missing,
    Es(i32),
    Isfs(i32),
    IsfsCleanup {
        operation: Option<i32>,
        cleanup: i32,
    },
    CommittedCleanup(i32),
    RecoveryConflict,
    TemporaryMismatch,
    InvalidPath,
    InvalidFileSize,
    Codec,
    TitlePathBusy,
}

impl SettingsIoError {
    #[cfg(any(target_arch = "powerpc", test))]
    pub(crate) fn from_replace(error: super::nand::ReplaceError) -> Self {
        match error {
            super::nand::ReplaceError::Backend(error) => Self::Isfs(error),
            super::nand::ReplaceError::Cleanup { operation, cleanup } => {
                Self::IsfsCleanup { operation, cleanup }
            }
            super::nand::ReplaceError::CommittedCleanup(error) => Self::CommittedCleanup(error),
            super::nand::ReplaceError::BackupNeedsValidation => Self::RecoveryConflict,
            super::nand::ReplaceError::TemporaryMismatch => Self::TemporaryMismatch,
        }
    }
}

fn resolve_title_data_directory() -> Result<TitleDataDirectory, TitlePathError> {
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("ES INIT REQUEST");
    let rc = es::init();
    if rc < 0 {
        #[cfg(feature = "startup-diagnostics")]
        super::diagnostics::mark("ES INIT FAILED");
        return Err(TitlePathError::Es(rc));
    }
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("ES INIT READY");
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("ES TITLE ID REQUEST");
    let mut title_id = AlignedTitleId(0);
    let rc = unsafe { es::get_title_id(&mut title_id.0) };
    if rc < 0 {
        #[cfg(feature = "startup-diagnostics")]
        super::diagnostics::mark("ES TITLE ID FAILED");
        return Err(TitlePathError::Es(rc));
    }
    let mut data_dir = AlignedDataDir([0; TITLE_DATA_DIR_BUFFER_LEN]);
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("ES DATA DIR REQUEST");
    let rc = unsafe { es::get_data_dir(title_id.0, data_dir.0.as_mut_ptr()) };
    if rc < 0 {
        #[cfg(feature = "startup-diagnostics")]
        super::diagnostics::mark("ES DATA DIR FAILED");
        return Err(TitlePathError::Es(rc));
    }
    TitleDataDirectory::from_es_buffer(data_dir.0)
}

fn title_data_directory() -> Result<TitleDataDirectory, SettingsIoError> {
    let result = TITLE_DIR_CACHE
        .get_or_init(resolve_title_data_directory)
        .map_err(map_title_path_error);
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark(match result {
        Ok(_) => "TITLE DIR CACHE READY",
        Err(SettingsIoError::Es(_)) => "TITLE DIR CACHE ES ERROR",
        Err(SettingsIoError::TitlePathBusy) => "TITLE DIR CACHE BUSY",
        Err(_) => "TITLE DIR CACHE INVALID",
    });
    result
}

pub(crate) fn title_data_file_path(name: &str) -> Result<CString, SettingsIoError> {
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("TITLE DIR CACHE READ");
    let directory = title_data_directory()?;
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("TITLE FILE PATH BUILD");
    let path = directory.file_path(name).map_err(map_title_path_error)?;
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("TITLE FILE PATH READY");
    Ok(path)
}

fn map_title_path_error(error: TitlePathError) -> SettingsIoError {
    match error {
        TitlePathError::Es(rc) => SettingsIoError::Es(rc),
        TitlePathError::InvalidPath => SettingsIoError::InvalidPath,
        TitlePathError::Busy => SettingsIoError::TitlePathBusy,
    }
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
    super::nand::recover_wii(&path, MAX_SETTINGS_XML_SIZE, |bytes| {
        decode_settings_xml(bytes).is_ok()
    })?;
    let file = match isfs::open(&path, isfs::ISFS_OPEN_READ) {
        Ok(file) => file,
        Err(ogc_ipc::IPC_ENOENT) => return Err(SettingsIoError::Missing),
        Err(error) => return Err(SettingsIoError::Isfs(error)),
    };
    let mut stats = AlignedFstats(isfs::Fstats::default());
    let read_result = (|| {
        let rc = isfs::get_file_stats(&file, &mut stats.0);
        if rc < 0 {
            return Err(SettingsIoError::Isfs(rc));
        }
        let size =
            usize::try_from(stats.0.file_length).map_err(|_| SettingsIoError::InvalidFileSize)?;
        if size == 0 || size > MAX_SETTINGS_XML_SIZE {
            return Err(SettingsIoError::InvalidFileSize);
        }

        let mut xml = AlignedXml([0; MAX_SETTINGS_XML_SIZE]);
        let mut offset = 0;
        while offset < size {
            match file.read(&mut xml.0[offset..size]) {
                Ok(0) => return Err(SettingsIoError::InvalidFileSize),
                Ok(count) if count <= size - offset => offset += count,
                Ok(_) => return Err(SettingsIoError::InvalidFileSize),
                Err(rc) => return Err(SettingsIoError::Isfs(rc)),
            }
        }
        Ok((xml, size))
    })();
    let close_rc = file.close();
    if close_rc < 0 {
        return match read_result {
            Err(SettingsIoError::Isfs(operation)) => Err(SettingsIoError::IsfsCleanup {
                operation: Some(operation),
                cleanup: close_rc,
            }),
            Err(_) => Err(SettingsIoError::Isfs(close_rc)),
            Ok(_) => Err(SettingsIoError::Isfs(close_rc)),
        };
    }
    let (xml, size) = read_result?;
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
