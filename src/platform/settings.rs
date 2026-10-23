//! NAND-backed HBC settings storage through ES and ISFS Rust APIs.

use alloc::ffi::CString;
use core::ffi::CStr;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::metadata::UserSettings;
use crate::settings::{
    MAX_SETTINGS_XML_SIZE, SETTINGS_PATH, decode_settings_xml, encode_settings_xml,
};

use ogc_es_sys as es;
use ogc_filesystem as isfs;

use crate::title_path::{
    CachedTitleDataDirectory, TITLE_DATA_DIR_BUFFER_LEN, TitleDataDirectory, TitlePathError,
};

static TITLE_DIR_CACHE: CachedTitleDataDirectory = CachedTitleDataDirectory::new();
static BANNER_SAVE_NEEDED: AtomicBool = AtomicBool::new(true);

const BANNER_PATH: &str = "banner.bin";
const SAVE_BANNER: &[u8] = include_bytes!("../../assets/save_banner.bin");
const PLAYTIME_PATH: &CStr = c"/title/00000001/00000002/data/play_rec.dat";
const PLAYTIME_RECORD_SIZE: u32 = 4;

const IPC_EQUEUEFULL: i32 = -8;
const IPC_QUEUE_RETRY_LIMIT: usize = 32;

#[cfg(target_arch = "powerpc")]
fn report_settings_event(message: &str) {
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::event(message);
    ogc_system::report(format_args!("HBC: {message}\n"));
}

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
    StaleSnapshot,
    InvalidPath,
    InvalidFileSize,
    Codec,
    TitlePathBusy,
    PlaytimeVerificationMismatch,
    NandRead(super::nand::ReadError),
    NandReadCleanup {
        operation: super::nand::ReadError,
        cleanup: i32,
    },
}

impl SettingsIoError {
    pub(crate) fn from_read(error: super::nand::ReadError) -> Self {
        use super::nand::{ReadError, ReadFailure};
        match error {
            ReadError::Operation(ReadFailure::Backend(code)) => Self::Isfs(code),
            ReadError::Operation(ReadFailure::InvalidSize) => Self::InvalidFileSize,
            ReadError::Operation(ReadFailure::Allocation) => Self::Isfs(isfs::ISFS_ENOMEM),
            ReadError::Close {
                operation: Some(ReadFailure::Backend(operation)),
                cleanup,
            } => Self::IsfsCleanup {
                operation: Some(operation),
                cleanup,
            },
            ReadError::Close {
                operation: None,
                cleanup,
            } => Self::Isfs(cleanup),
            other => Self::NandRead(other),
        }
    }

    #[cfg(any(target_arch = "powerpc", test))]
    pub(crate) fn from_replace(error: super::nand::ReplaceError) -> Self {
        match error {
            super::nand::ReplaceError::Read(error) => Self::from_read(error),
            super::nand::ReplaceError::ReadCleanup { operation, cleanup } => {
                Self::NandReadCleanup { operation, cleanup }
            }
            super::nand::ReplaceError::Backend(error) => Self::Isfs(error),
            super::nand::ReplaceError::Cleanup { operation, cleanup } => {
                Self::IsfsCleanup { operation, cleanup }
            }
            super::nand::ReplaceError::CommittedCleanup(error) => Self::CommittedCleanup(error),
            super::nand::ReplaceError::BackupNeedsValidation => Self::RecoveryConflict,
            super::nand::ReplaceError::TemporaryMismatch => Self::TemporaryMismatch,
            super::nand::ReplaceError::StaleSnapshot => Self::StaleSnapshot,
            super::nand::ReplaceError::InvalidPath => Self::InvalidPath,
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
        if rc == -1017 {
            ogc_system::report(format_args!(
                "HBC: ES could not resolve direct-DOL title ID; using /title/00010001/48424345/data\n"
            ));
        }
    }
    TitleDataDirectory::for_title_id_result((rc >= 0).then_some(title_id.0).ok_or(rc), |title_id| {
        let mut data_dir = AlignedDataDir([0; TITLE_DATA_DIR_BUFFER_LEN]);
        #[cfg(feature = "startup-diagnostics")]
        super::diagnostics::mark("ES DATA DIR REQUEST");
        let rc =
            retry_ipc_queue_full(|| unsafe { es::get_data_dir(title_id, data_dir.0.as_mut_ptr()) });
        if rc < 0 {
            #[cfg(feature = "startup-diagnostics")]
            super::diagnostics::mark("ES DATA DIR FAILED");
            if rc == -1017 {
                ogc_system::report(format_args!(
                    "HBC: ES has no direct-DOL title data directory; using /title/00010001/48424345/data\n"
                ));
            }
            return Err(TitlePathError::Es(rc));
        }
        TitleDataDirectory::from_es_buffer(data_dir.0)
    })
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

/// Report whether the running title owns a writable NAND settings directory.
/// A direct DOL has no installed ES data directory, and its development path is
/// only a lookup fallback. Do not try to create that system path on NAND.
pub(crate) fn persistence_available() -> Result<bool, SettingsIoError> {
    title_data_directory().map(TitleDataDirectory::persistence_available)
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

/// Prepare the explicit development directory only when ES selected that path.
/// Installed channels keep their ES directory and its existing permissions.
pub(crate) fn prepare_title_data_file_path(name: &str) -> Result<CString, SettingsIoError> {
    let directory = title_data_directory()?;
    let path = directory.file_path(name).map_err(map_title_path_error)?;
    let result = isfs::initialize();
    if result < 0 {
        return Err(SettingsIoError::Isfs(result));
    }
    if let Some(parents) = directory.development_parents() {
        for parent in parents {
            super::nand::ensure_directory_wii(parent)?;
        }
    }
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
    let path = prepare_title_data_file_path(BANNER_PATH)?;
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
    if let Err(error) = validate_save_banner() {
        ogc_system::report(format_args!(
            "HBC: could not validate saved banner ({error:?}); settings read will continue\n"
        ));
    }
    match read_settings_file(&path) {
        Ok(settings) => Ok(settings),
        Err(
            SettingsIoError::Missing | SettingsIoError::InvalidFileSize | SettingsIoError::Codec,
        ) => {
            super::nand::recover_wii(&path, MAX_SETTINGS_XML_SIZE, |bytes| {
                decode_settings_xml(bytes).is_ok()
            })?;
            read_settings_file(&path)
        }
        Err(error) => Err(error),
    }
}

fn read_settings_file(path: &CString) -> Result<UserSettings, SettingsIoError> {
    let file = match isfs::open(path, isfs::ISFS_OPEN_READ) {
        Ok(file) => file,
        Err(error) if super::nand::is_missing_isfs_error(error, ogc_filesystem::ISFS_ENOENT) => {
            return Err(SettingsIoError::Missing);
        }
        Err(error) => return Err(SettingsIoError::Isfs(error)),
    };
    let (xml, size) = super::nand::read_open_file(file, 1, MAX_SETTINGS_XML_SIZE, |file, size| {
        use super::nand::{ReadFailure, ReadFile};
        let mut xml = AlignedXml([0; MAX_SETTINGS_XML_SIZE]);
        file.read_exact(&mut xml.0[..size])
            .map_err(ReadFailure::Backend)?;
        Ok((xml, size))
    })
    .map_err(SettingsIoError::from_read)?;
    decode_settings_xml(&xml.0[..size]).map_err(|_| SettingsIoError::Codec)
}

/// Replace the settings XML file without deleting the previous valid copy.
pub fn save(settings: &UserSettings) -> Result<(), SettingsIoError> {
    let xml = encode_settings_xml(settings).map_err(|_| SettingsIoError::Codec)?;
    let path = prepare_title_data_file_path(SETTINGS_PATH)?;
    report_settings_event(&alloc::format!(
        "settings target path={:?}",
        path.to_bytes()
    ));
    let rc = isfs::initialize();
    if rc < 0 {
        report_settings_event(&alloc::format!(
            "ISFS Initialize before settings save failed rc={rc}"
        ));
        return Err(SettingsIoError::Isfs(rc));
    }
    if let Err(error) = save_banner_if_needed() {
        report_settings_event(&alloc::format!(
            "settings save stopped while saving banner: {error:?}"
        ));
        return Err(error);
    }
    match super::nand::save_wii(&path, xml.as_bytes()) {
        Ok(()) => Ok(()),
        Err(error) => {
            report_settings_event(&alloc::format!("settings XML save failed: {error:?}"));
            Err(error)
        }
    }
}

/// Reset the first four bytes of the System Menu playtime file, as C does.
///
/// The file must exist and contain at least four bytes. The reset reads the
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
        if !crate::playtime::record_has_reset_prefix(stats.0.file_length) {
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
        if !crate::playtime::record_has_reset_prefix(stats.0.file_length) {
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
