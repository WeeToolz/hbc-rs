//! NAND-backed HBC settings storage through ES and ISFS Rust APIs.

use alloc::ffi::CString;
use core::ffi::CStr;
use core::sync::atomic::{AtomicBool, Ordering};

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
static BANNER_SAVE_NEEDED: AtomicBool = AtomicBool::new(true);

const BANNER_PATH: &str = "banner.bin";
const SAVE_BANNER: &[u8] = include_bytes!("../../assets/save_banner.bin");
const PLAYTIME_PATH: &CStr = c"/title/00000001/00000002/data/play_rec.dat";
const PLAYTIME_RECORD_SIZE: u32 = 4;

const IPC_EQUEUEFULL: i32 = -8;
const IPC_QUEUE_RETRY_LIMIT: usize = 32;

fn retry_ipc_queue_full(mut request: impl FnMut() -> i32) -> i32 {
    let mut last = IPC_EQUEUEFULL;
    for _ in 0..IPC_QUEUE_RETRY_LIMIT {
        let rc = request();
        if rc != IPC_EQUEUEFULL {
            return rc;
        }
        last = rc;
        // Drain completed IPC replies. poll() also gives the deferred IPC
        // completion worker a scheduling point when it consumes a reply. Do
        // not sleep here: this path runs before video startup, and blocking the
        // main LWP here can prevent the first frame on Dolphin/Wii runtimes.
        let _ = ogc_ipc::poll();
        ogc_lwp::sys::yield_thread();
    }
    last
}

#[repr(align(32))]
struct AlignedTitleId(u64);
#[repr(align(32))]
struct AlignedDataDir([u8; TITLE_DATA_DIR_BUFFER_LEN]);
#[repr(align(32))]
struct AlignedFstats(isfs::Fstats);
#[repr(align(32))]
struct AlignedXml([u8; MAX_SETTINGS_XML_SIZE]);
#[repr(align(32))]
struct AlignedPlaytimeRecord([u8; 32]);

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
    PlaytimeVerificationMismatch,
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
    let rc = retry_ipc_queue_full(es::init);
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
    let rc = retry_ipc_queue_full(|| unsafe { es::get_title_id(&mut title_id.0) });
    if rc < 0 {
        #[cfg(feature = "startup-diagnostics")]
        super::diagnostics::mark("ES TITLE ID FAILED");
        return Err(TitlePathError::Es(rc));
    }
    let mut data_dir = AlignedDataDir([0; TITLE_DATA_DIR_BUFFER_LEN]);
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("ES DATA DIR REQUEST");
    let rc =
        retry_ipc_queue_full(|| unsafe { es::get_data_dir(title_id.0, data_dir.0.as_mut_ptr()) });
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

fn banner_path() -> Result<CString, SettingsIoError> {
    title_data_file_path(BANNER_PATH)
}

fn validate_save_banner() -> Result<(), SettingsIoError> {
    let path = banner_path()?;
    let rc = isfs::initialize();
    if rc < 0 {
        return Err(SettingsIoError::Isfs(rc));
    }
    match super::nand::read_file_wii(&path, SAVE_BANNER.len())? {
        Some(bytes) if bytes == SAVE_BANNER => {
            BANNER_SAVE_NEEDED.store(false, Ordering::Release);
            Ok(())
        }
        Some(_) | None => {
            BANNER_SAVE_NEEDED.store(true, Ordering::Release);
            Ok(())
        }
    }
}

fn save_banner_if_needed() -> Result<(), SettingsIoError> {
    if !BANNER_SAVE_NEEDED.load(Ordering::Acquire) {
        return Ok(());
    }
    let path = banner_path()?;
    super::nand::save_wii(&path, SAVE_BANNER)?;
    BANNER_SAVE_NEEDED.store(false, Ordering::Release);
    Ok(())
}

/// Load the settings file from the running channel's ES data directory.
pub fn load() -> Result<UserSettings, SettingsIoError> {
    let path = settings_path()?;
    let rc = isfs::initialize();
    if rc < 0 {
        return Err(SettingsIoError::Isfs(rc));
    }
    validate_save_banner()?;
    super::nand::recover_wii(&path, MAX_SETTINGS_XML_SIZE, |bytes| {
        decode_settings_xml(bytes).is_ok()
    })?;
    let file = match isfs::open(&path, isfs::ISFS_OPEN_READ) {
        Ok(file) => file,
        Err(error) if super::nand::is_missing_isfs_error(error, ogc_filesystem::ISFS_ENOENT) => {
            return Err(SettingsIoError::Missing);
        }
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
        super::nand::read_exact_wii(&file, &mut xml.0[..size]).map_err(SettingsIoError::Isfs)?;
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
    save_banner_if_needed()?;
    super::nand::save_wii(&path, xml.as_bytes())
}

/// Reset HBC's four-byte System Menu playtime record, as the C application does.
///
/// The file must exist and have the exact expected size. The reset reads the
/// record again after closing the write handle so success means ISFS committed
/// the zero value.
pub fn reset_playtime_record() -> Result<(), SettingsIoError> {
    let rc = isfs::initialize();
    if rc < 0 {
        return Err(SettingsIoError::Isfs(rc));
    }
    match crate::playtime::reset_record_with(read_playtime_record, write_playtime_record) {
        Ok(()) => Ok(()),
        Err(crate::playtime::ResetError::Operation(error)) => Err(error),
        Err(crate::playtime::ResetError::VerificationMismatch) => {
            Err(SettingsIoError::PlaytimeVerificationMismatch)
        }
    }
}

fn read_playtime_record() -> Result<[u8; 4], SettingsIoError> {
    let file = isfs::open(PLAYTIME_PATH, isfs::ISFS_OPEN_READ).map_err(SettingsIoError::Isfs)?;
    let result = (|| {
        let mut stats = AlignedFstats(isfs::Fstats::default());
        let rc = isfs::get_file_stats(&file, &mut stats.0);
        if rc < 0 {
            return Err(SettingsIoError::Isfs(rc));
        }
        if stats.0.file_length != PLAYTIME_RECORD_SIZE {
            return Err(SettingsIoError::InvalidFileSize);
        }
        let mut record = AlignedPlaytimeRecord([0; 32]);
        match file.read(&mut record.0[..PLAYTIME_RECORD_SIZE as usize]) {
            Ok(count) if count == PLAYTIME_RECORD_SIZE as usize => {
                Ok([record.0[0], record.0[1], record.0[2], record.0[3]])
            }
            Ok(_) => Err(SettingsIoError::Isfs(isfs::ISFS_EINVAL)),
            Err(error) => Err(SettingsIoError::Isfs(error)),
        }
    })();
    close_playtime_file(file, result)
}

fn write_playtime_record(record: [u8; 4]) -> Result<(), SettingsIoError> {
    let file = isfs::open(PLAYTIME_PATH, isfs::ISFS_OPEN_RW).map_err(SettingsIoError::Isfs)?;
    let result = (|| {
        let mut stats = AlignedFstats(isfs::Fstats::default());
        let rc = isfs::get_file_stats(&file, &mut stats.0);
        if rc < 0 {
            return Err(SettingsIoError::Isfs(rc));
        }
        if stats.0.file_length != PLAYTIME_RECORD_SIZE {
            return Err(SettingsIoError::InvalidFileSize);
        }
        let mut aligned = AlignedPlaytimeRecord([0; 32]);
        aligned.0[..4].copy_from_slice(&record);
        file.write_all(&aligned.0[..4])
            .map_err(SettingsIoError::Isfs)
    })();
    close_playtime_file(file, result)
}

fn close_playtime_file<T>(
    file: isfs::IsfsFile,
    operation: Result<T, SettingsIoError>,
) -> Result<T, SettingsIoError> {
    let close_rc = file.close();
    if close_rc < 0 {
        return match operation {
            Err(SettingsIoError::Isfs(operation)) => Err(SettingsIoError::IsfsCleanup {
                operation: Some(operation),
                cleanup: close_rc,
            }),
            Err(_) | Ok(_) => Err(SettingsIoError::Isfs(close_rc)),
        };
    }
    operation
}
