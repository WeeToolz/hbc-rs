//! Boot-time HBC service and application catalog setup.
//!
//! The FAT mount and initial catalog scan run on a worker. The app thread owns
//! the mounted adapter after the worker returns and performs later file access.

use alloc::{collections::BTreeSet, format, string::String, vec::Vec};
use core::{
    ffi::c_void,
    mem::MaybeUninit,
    ptr::{addr_of_mut, null_mut},
    sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU8, Ordering},
};

use crate::{
    catalog::{scan_apps, AppEntry, AppKind, CatalogStorage},
    loader::{build_launch_arguments, parse_dol, parse_elf32_ppc, LoadPlan, ParseError},
    metadata::{ThemeMetadata, UserSettings},
    platform::{
        input, settings,
        storage::{classify_device_presence, device_scan_order, FatStorage, StorageError, Volume},
    },
    runtime::AppRuntime,
};

/// HBC reserves this address range for loading an application executable.
const EXECUTABLE_SIZE_LIMIT: u64 = (0x8133_0000 - 1 - 1024) - 0x8000_3400;
const THEME_SIZE_LIMIT: u64 = 20 * 1024 * 1024;
const BOOTMII_TITLE_ID: u64 = 0x0000_0001_0000_00fe;
const STORAGE_SCAN_STACK_SIZE: usize = 16 * 1024;
// Keep storage work below the application main thread. The Rust LWP scheduler
// preempts a running lower-priority thread when a higher-priority sleeper wakes.
// The main loop gives this worker a bounded 1 ms slice each frame while a scan
// is pending, instead of yielding indefinitely to an equal-priority worker.
const STORAGE_SCAN_PRIORITY: u8 = 62;
const STORAGE_SCAN_IDLE: u8 = 0;
const STORAGE_SCAN_RUNNING: u8 = 1;
const STORAGE_SCAN_READY: u8 = 2;
const STORAGE_STAGE_START: u8 = 0;
const STORAGE_STAGE_SHUTDOWN: u8 = 1;
const STORAGE_STAGE_MOUNT_SD: u8 = 2;
const STORAGE_STAGE_MOUNT_USB: u8 = 3;
const STORAGE_STAGE_MOUNT_CARDA: u8 = 4;
const STORAGE_STAGE_MOUNT_CARDB: u8 = 5;
const STORAGE_STAGE_RECOVER: u8 = 6;
const STORAGE_STAGE_SCAN_APPS: u8 = 7;
const STORAGE_STAGE_FINISH: u8 = 8;
const ZIP_INSTALL_IDLE: u8 = 0;
const ZIP_INSTALL_REQUESTED: u8 = 1;
const ZIP_INSTALL_RUNNING: u8 = 2;
const ZIP_INSTALL_READY: u8 = 3;
const DEVICE_PROBE_IDLE: u8 = 0;
const DEVICE_PROBE_RUNNING: u8 = 1;
const DEVICE_PROBE_READY: u8 = 2;
pub const EXECUTABLE_MEMORY: core::ops::Range<u32> = 0x8000_3400..0x8132_fc00;

struct StorageScanResult {
    volume: Option<Volume>,
    storage: Option<FatStorage>,
    catalog: Vec<AppEntry>,
    scan_error: Option<crate::catalog::ScanError<StorageError>>,
    mount_errors: [(Volume, Option<StorageError>); 4],
    recovery_error: Option<ZipInstallError>,
    shutdown_error: Option<ogc_fat::FatError>,
}

struct MountScanResult {
    volume: Option<Volume>,
    storage: Option<FatStorage>,
    catalog: Vec<AppEntry>,
    scan_error: Option<crate::catalog::ScanError<StorageError>>,
    mount_errors: [(Volume, Option<StorageError>); 4],
    recovery_error: Option<ZipInstallError>,
}

#[derive(Clone, Copy)]
struct DeviceProbeResult {
    availability: [bool; 4],
    error: Option<ogc_fat::FatError>,
}

fn load_default_theme_textures(widescreen: bool) -> super::theme::ThemeTextures {
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("DEFAULT THEME BUILD");
    let textures = super::theme::ThemeTextures::load_for_viewport(None, widescreen);
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("DEFAULT THEME READY");
    textures
}

#[repr(align(32))]
struct StorageScanStack([u8; STORAGE_SCAN_STACK_SIZE]);

static STORAGE_SCAN_STATE: AtomicU8 = AtomicU8::new(STORAGE_SCAN_IDLE);
static STORAGE_SCAN_PREFERENCE: AtomicI32 = AtomicI32::new(0);
static STORAGE_SCAN_SHUTDOWN_OLD: AtomicBool = AtomicBool::new(false);
static STORAGE_SCAN_STAGE: AtomicU8 = AtomicU8::new(STORAGE_STAGE_START);
static mut STORAGE_SCAN_RESULT: MaybeUninit<StorageScanResult> = MaybeUninit::uninit();
static mut STORAGE_SCAN_STACK: StorageScanStack = StorageScanStack([0; STORAGE_SCAN_STACK_SIZE]);
static DEVICE_PROBE_STATE: AtomicU8 = AtomicU8::new(DEVICE_PROBE_IDLE);
static DEVICE_PROBE_ACTIVE: AtomicU8 = AtomicU8::new(2);
static mut DEVICE_PROBE_RESULT: MaybeUninit<DeviceProbeResult> = MaybeUninit::uninit();
static mut DEVICE_PROBE_STACK: StorageScanStack = StorageScanStack([0; STORAGE_SCAN_STACK_SIZE]);

struct ZipInstallRequest {
    storage: FatStorage,
    bytes: Vec<u8>,
    directory: alloc::string::String,
    kind: crate::wiiload_zip::WiiloadZipKind,
    plan: crate::management::ExtractionPlan,
}

struct ZipInstallResult {
    storage: FatStorage,
    catalog: Result<Vec<AppEntry>, ZipInstallError>,
    cleanup_error: Option<StorageError>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ZipInstallError {
    Archive(crate::archive::ZipError),
    Storage(StorageError),
    StorageCleanup {
        operation: StorageError,
        cleanup: StorageError,
    },
    StorageRollback {
        operation: StorageError,
        rollback: StorageError,
    },
    CatalogRefresh(crate::catalog::ScanError<StorageError>),
    CatalogRollback {
        operation: crate::catalog::ScanError<StorageError>,
        rollback: StorageError,
    },
}

impl ZipInstallError {
    pub fn notice(&self) -> String {
        match self {
            Self::Archive(error) => {
                format!("The received ZIP archive failed validation: {error:?}.")
            }
            Self::Storage(error) => {
                format!("Could not write the received app files: {error:?}.")
            }
            Self::StorageCleanup {
                operation,
                cleanup,
            } => format!(
                "Could not write the received app files ({operation:?}); cleanup also failed ({cleanup:?})."
            ),
            Self::StorageRollback {
                operation,
                rollback,
            } => format!(
                "Could not install the app ({operation:?}) or restore its previous files ({rollback:?})."
            ),
            Self::CatalogRefresh(error) => format!(
                "The app files were written, but HBC could not refresh its app list: {error:?}."
            ),
            Self::CatalogRollback {
                operation,
                rollback,
            } => format!(
                "Could not refresh HBC's app list ({operation:?}) or restore the previous app ({rollback:?})."
            ),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ZipInstallJoinError {
    MissingWorker,
    Failed(i32),
}

impl ZipInstallJoinError {
    pub fn notice(&self) -> String {
        match self {
            Self::MissingWorker => String::from("The ZIP install worker handle is missing."),
            Self::Failed(code) => format!("Could not join the ZIP install worker ({code})."),
        }
    }
}

pub struct ZipInstallCompletion {
    pub operation: Result<(), ZipInstallError>,
    pub cleanup_error: Option<StorageError>,
    pub worker_join: Result<(), ZipInstallJoinError>,
}

struct InstalledApp {
    catalog: Vec<AppEntry>,
    cleanup_error: Option<StorageError>,
}

fn fail_stage_install(
    storage: &FatStorage,
    stage: &str,
    operation: StorageError,
) -> ZipInstallError {
    match storage.path_exists(stage) {
        Ok(true) => match storage.remove_directory_tree(stage) {
            Ok(()) => ZipInstallError::Storage(operation),
            Err(cleanup) => ZipInstallError::StorageCleanup { operation, cleanup },
        },
        Ok(false) => ZipInstallError::Storage(operation),
        Err(cleanup) => ZipInstallError::StorageCleanup { operation, cleanup },
    }
}

fn install_received_app(
    storage: &mut FatStorage,
    bytes: &[u8],
    directory: &str,
    kind: crate::wiiload_zip::WiiloadZipKind,
    plan: &crate::management::ExtractionPlan,
    archive: &crate::archive::ZipArchive,
) -> Result<InstalledApp, ZipInstallError> {
    storage
        .ensure_directory("apps")
        .map_err(ZipInstallError::Storage)?;
    let target = alloc::format!("apps/{directory}");
    let stage = alloc::format!("apps/.{directory}.hbc-stage");
    let backup = alloc::format!("apps/.{directory}.hbc-backup");

    recover_app_install(storage, &target, &stage, &backup, false)?;
    storage
        .ensure_directory(&stage)
        .map_err(ZipInstallError::Storage)?;
    if let Err(operation) =
        storage.extract_archive_with_progress(bytes, archive, plan, &stage, |completed, total| {
            ZIP_INSTALL_TOTAL_BYTES.store(total.min(u32::MAX as u64) as u32, Ordering::Relaxed);
            ZIP_INSTALL_PROGRESS_BYTES
                .store(completed.min(u32::MAX as u64) as u32, Ordering::Release);
        })
    {
        return match storage.remove_directory_tree(&stage) {
            Ok(()) => Err(ZipInstallError::Storage(operation)),
            Err(cleanup) => Err(ZipInstallError::StorageCleanup { operation, cleanup }),
        };
    }

    let executable = match kind {
        crate::wiiload_zip::WiiloadZipKind::Elf => "boot.elf",
        crate::wiiload_zip::WiiloadZipKind::Dol => "boot.dol",
        crate::wiiload_zip::WiiloadZipKind::Theme => "theme.zip",
    };
    let executable_path = alloc::format!("{stage}/{executable}");
    let executable_size = match storage.file_size(&executable_path) {
        Ok(size) => size,
        Err(operation) => return Err(fail_stage_install(storage, &stage, operation)),
    }
    .filter(|size| {
        *size > 0
            && *size <= EXECUTABLE_SIZE_LIMIT
            && (kind != crate::wiiload_zip::WiiloadZipKind::Theme || *size <= THEME_SIZE_LIMIT)
    });
    if executable_size.is_none() {
        return Err(fail_stage_install(
            storage,
            &stage,
            StorageError::InvalidPath,
        ));
    }

    let had_previous = match storage.path_exists(&target) {
        Ok(exists) => exists,
        Err(operation) => return Err(fail_stage_install(storage, &stage, operation)),
    };
    if had_previous {
        let target_is_directory = match storage.path_is_directory(&target) {
            Ok(is_directory) => is_directory,
            Err(operation) => return Err(fail_stage_install(storage, &stage, operation)),
        };
        if !target_is_directory {
            return Err(fail_stage_install(
                storage,
                &stage,
                StorageError::InvalidPath,
            ));
        }
    }

    match crate::management::commit_staged_directory(
        had_previous,
        &target,
        &stage,
        &backup,
        |old, new| storage.rename_path(old, new),
    ) {
        Ok(()) => {}
        Err(crate::management::DirectoryCommitError::Install(operation)) => {
            return Err(fail_stage_install(storage, &stage, operation));
        }
        Err(crate::management::DirectoryCommitError::Restore { install, restore }) => {
            return Err(ZipInstallError::StorageRollback {
                operation: install,
                rollback: restore,
            });
        }
    }

    let catalog = match scan_apps(storage, "apps", EXECUTABLE_SIZE_LIMIT, THEME_SIZE_LIMIT) {
        Ok(catalog) => catalog,
        Err(operation) => {
            let rollback = crate::management::rollback_staged_directory(
                had_previous,
                &target,
                &stage,
                &backup,
                |old, new| storage.rename_path(old, new),
            )
            .and_then(|()| storage.remove_directory_tree(&stage));
            return match rollback {
                Ok(()) => Err(ZipInstallError::CatalogRefresh(operation)),
                Err(rollback) => Err(ZipInstallError::CatalogRollback {
                    operation,
                    rollback,
                }),
            };
        }
    };

    let cleanup_error = if had_previous {
        storage.remove_directory_tree(&backup).err()
    } else {
        None
    };
    Ok(InstalledApp {
        catalog,
        cleanup_error,
    })
}

fn recover_app_install(
    storage: &mut FatStorage,
    target: &str,
    stage: &str,
    backup: &str,
    preserve_valid_backup: bool,
) -> Result<(), ZipInstallError> {
    let target_exists = storage
        .path_exists(target)
        .map_err(ZipInstallError::Storage)?;
    let mut stage_exists = storage
        .path_exists(stage)
        .map_err(ZipInstallError::Storage)?;
    let mut backup_exists = storage
        .path_exists(backup)
        .map_err(ZipInstallError::Storage)?;

    if target_exists && stage_exists {
        stage_exists = !storage
            .repair_interrupted_directory_rename(target, stage)
            .map_err(ZipInstallError::Storage)?;
    }
    if target_exists && backup_exists {
        backup_exists = !storage
            .repair_interrupted_directory_rename(target, backup)
            .map_err(ZipInstallError::Storage)?;
    }

    if backup_exists && !target_exists {
        storage
            .rename_path(backup, target)
            .map_err(ZipInstallError::Storage)?;
    } else if backup_exists {
        let catalog = scan_apps(storage, "apps", EXECUTABLE_SIZE_LIMIT, THEME_SIZE_LIMIT)
            .map_err(ZipInstallError::CatalogRefresh)?;
        if catalog
            .iter()
            .any(|entry| entry.directory == target.rsplit('/').next().unwrap_or(""))
        {
            if !preserve_valid_backup {
                storage
                    .remove_directory_tree(backup)
                    .map_err(ZipInstallError::Storage)?;
            }
        } else {
            if stage_exists {
                storage
                    .remove_directory_tree(stage)
                    .map_err(ZipInstallError::Storage)?;
            }
            storage
                .rename_path(target, stage)
                .map_err(ZipInstallError::Storage)?;
            storage
                .rename_path(backup, target)
                .map_err(ZipInstallError::Storage)?;
            storage
                .remove_directory_tree(stage)
                .map_err(ZipInstallError::Storage)?;
        }
    }
    if stage_exists
        && storage
            .path_exists(stage)
            .map_err(ZipInstallError::Storage)?
    {
        storage
            .remove_directory_tree(stage)
            .map_err(ZipInstallError::Storage)?;
    }
    Ok(())
}

fn recover_app_install_artifacts(storage: &mut FatStorage) -> Result<(), ZipInstallError> {
    let entries = match storage.read_dir("apps") {
        Ok(entries) => entries,
        Err(StorageError::Fat(ogc_fat::FatError::NotFound)) => return Ok(()),
        Err(error) => return Err(ZipInstallError::Storage(error)),
    };
    let mut directories = BTreeSet::new();
    for entry in entries.into_iter().filter(|entry| entry.is_directory) {
        let Some(name) = entry.name.strip_prefix('.') else {
            continue;
        };
        let directory = [".hbc-stage", ".hbc-backup"]
            .into_iter()
            .find_map(|suffix| name.strip_suffix(suffix));
        if let Some(directory) = directory {
            if crate::management::plan_app_deletion(directory).is_ok() {
                directories.insert(String::from(directory));
            }
        }
    }
    for directory in directories {
        let target = alloc::format!("apps/{directory}");
        let stage = alloc::format!("apps/.{directory}.hbc-stage");
        let backup = alloc::format!("apps/.{directory}.hbc-backup");
        recover_app_install(storage, &target, &stage, &backup, true)?;
    }
    Ok(())
}

static ZIP_INSTALL_STATE: AtomicU8 = AtomicU8::new(ZIP_INSTALL_IDLE);
static ZIP_INSTALL_PROGRESS_BYTES: AtomicU32 = AtomicU32::new(0);
static ZIP_INSTALL_TOTAL_BYTES: AtomicU32 = AtomicU32::new(0);
static mut ZIP_INSTALL_REQUEST: MaybeUninit<ZipInstallRequest> = MaybeUninit::uninit();
static mut ZIP_INSTALL_RESULT: MaybeUninit<ZipInstallResult> = MaybeUninit::uninit();
static mut ZIP_INSTALL_THREAD: ogc_lwp::sys::LwpThread = ogc_lwp::sys::LWP_THREAD_NULL;
static mut ZIP_INSTALL_STACK: StorageScanStack = StorageScanStack([0; STORAGE_SCAN_STACK_SIZE]);

unsafe extern "C" fn zip_install_entry(_argument: *mut c_void) -> *mut c_void {
    ZIP_INSTALL_STATE.store(ZIP_INSTALL_RUNNING, Ordering::Release);
    let ZipInstallRequest {
        mut storage,
        bytes,
        directory,
        kind,
        plan,
    } = unsafe { addr_of_mut!(ZIP_INSTALL_REQUEST).read().assume_init_read() };
    let result = crate::archive::ZipArchive::parse_limited(
        &bytes,
        crate::wiiload_zip::WIILOAD_ZIP_MAX_ENTRIES,
    )
    .map_err(ZipInstallError::Archive)
    .and_then(|archive| {
        install_received_app(&mut storage, &bytes, &directory, kind, &plan, &archive)
    });
    let (catalog, cleanup_error) = match result {
        Ok(result) => (Ok(result.catalog), result.cleanup_error),
        Err(error) => (Err(error), None),
    };
    let result = ZipInstallResult {
        storage,
        catalog,
        cleanup_error,
    };
    unsafe { addr_of_mut!(ZIP_INSTALL_RESULT).write(MaybeUninit::new(result)) };
    ZIP_INSTALL_STATE.store(ZIP_INSTALL_READY, Ordering::Release);
    null_mut()
}

fn probe_one_device(volume: Volume) -> Result<bool, ogc_fat::FatError> {
    let source = match volume {
        Volume::Sd => ogc_fat::MountSource::Sd,
        Volume::Usb => ogc_fat::MountSource::Usb,
        Volume::CardA => ogc_fat::MountSource::CardA,
        Volume::CardB => ogc_fat::MountSource::CardB,
    };
    classify_device_presence(
        ogc_fat::device_is_inserted(source),
        ogc_fat::FatError::DeviceUnavailable,
        ogc_fat::FatError::MediaRemoved,
    )
}

unsafe extern "C" fn device_probe_entry(_argument: *mut c_void) -> *mut c_void {
    DEVICE_PROBE_STATE.store(DEVICE_PROBE_RUNNING, Ordering::Release);
    let active = match DEVICE_PROBE_ACTIVE.load(Ordering::Acquire) {
        index @ 0..=3 => Volume::from_settings_index(i32::from(index))
            .filter(|volume| ogc_fat::is_mounted(volume.mount_name()))
            .map(|_| index as usize),
        _ => None,
    };
    let probes = [
        probe_one_device(Volume::Sd),
        probe_one_device(Volume::Usb),
        probe_one_device(Volume::CardA),
        probe_one_device(Volume::CardB),
    ];
    let availability = crate::app::device_availability_snapshot(
        active,
        probes.map(|result| matches!(result, Ok(true))),
    );
    let error = probes.into_iter().find_map(Result::err);
    unsafe {
        addr_of_mut!(DEVICE_PROBE_RESULT).write(MaybeUninit::new(DeviceProbeResult {
            availability,
            error,
        }))
    };
    DEVICE_PROBE_STATE.store(DEVICE_PROBE_READY, Ordering::Release);
    null_mut()
}

unsafe extern "C" fn storage_scan_entry(_argument: *mut c_void) -> *mut c_void {
    STORAGE_SCAN_STAGE.store(STORAGE_STAGE_SHUTDOWN, Ordering::Release);
    let shutdown_error = STORAGE_SCAN_SHUTDOWN_OLD
        .load(Ordering::Acquire)
        .then(ogc_fat::shutdown_all)
        .transpose()
        .err();
    if shutdown_error.is_some() {
        unsafe {
            addr_of_mut!(STORAGE_SCAN_RESULT).write(MaybeUninit::new(StorageScanResult {
                volume: None,
                storage: None,
                catalog: Vec::new(),
                scan_error: None,
                mount_errors: device_scan_order(0).map(|volume| (volume, None)),
                recovery_error: None,
                shutdown_error,
            }))
        };
        STORAGE_SCAN_STATE.store(STORAGE_SCAN_READY, Ordering::Release);
        return null_mut();
    }
    let preference = STORAGE_SCAN_PREFERENCE.load(Ordering::Acquire);
    let scan = mount_preferred_volume(preference);
    STORAGE_SCAN_STAGE.store(STORAGE_STAGE_FINISH, Ordering::Release);
    let result = StorageScanResult {
        volume: scan.volume,
        storage: scan.storage,
        catalog: scan.catalog,
        scan_error: scan.scan_error,
        mount_errors: scan.mount_errors,
        recovery_error: scan.recovery_error,
        shutdown_error,
    };
    unsafe {
        addr_of_mut!(STORAGE_SCAN_RESULT).write(MaybeUninit::new(result));
    }
    STORAGE_SCAN_STATE.store(STORAGE_SCAN_READY, Ordering::Release);
    null_mut()
}

/// The result of native Wii startup, ready for the browser loop.
pub struct WiiStartup {
    runtime: AppRuntime,
    volume: Option<Volume>,
    storage: Option<FatStorage>,
    icon_textures: Vec<IconTexture>,
    attempted_icon_indices: Vec<usize>,
    theme_textures: super::theme::ThemeTextures,
    widescreen: bool,
    pending_settings: Option<UserSettings>,
    scan_thread: Option<ogc_lwp::ThreadHandle>,
    scan_previous_volume: Option<Volume>,
    scan_requested_volume: Option<Volume>,
    storage_presence: super::storage::StoragePresence,
    zip_install_thread: Option<ogc_lwp::ThreadHandle>,
    device_probe_thread: Option<ogc_lwp::ThreadHandle>,
}

pub struct IconTexture {
    pub catalog_index: usize,
    pub texture: crate::png::GxRgba8Texture,
}

impl WiiStartup {
    /// Start a validated Wiiload app install on a worker, or apply a theme ZIP.
    pub fn start_received_zip(&mut self, bytes: Vec<u8>) -> Result<(), StartupError> {
        use crate::wiiload_zip::{classify_received_zip, WiiloadZipClassification};
        match classify_received_zip(&bytes).map_err(|error| match error {
            crate::wiiload_zip::WiiloadZipError::InvalidTheme(error) => StartupError::Theme(error),
            crate::wiiload_zip::WiiloadZipError::InvalidArchive(error) => {
                StartupError::Storage(StorageError::Archive(error))
            }
            crate::wiiload_zip::WiiloadZipError::InvalidApp => StartupError::NotExecutable,
        })? {
            WiiloadZipClassification::Theme { package } => {
                let textures = super::theme::ThemeTextures::load_for_viewport(
                    Some((&package, &bytes)),
                    self.widescreen,
                );
                super::theme::save(&bytes).map_err(StartupError::ThemeStorage)?;
                self.apply_theme(PreparedTheme {
                    metadata: package.metadata,
                    textures,
                });
            }
            WiiloadZipClassification::App {
                directory,
                kind,
                extraction,
            } => {
                if self.storage_scan_pending()
                    || self.received_zip_install_pending()
                    || self.device_probe_pending()
                {
                    return Err(StartupError::StorageScanPending);
                }
                if ZIP_INSTALL_STATE.load(Ordering::Acquire) != ZIP_INSTALL_IDLE {
                    return Err(StartupError::ScanWorker(-1));
                }
                let storage = self.storage.take().ok_or(StartupError::NoStorage)?;
                let request = ZipInstallRequest {
                    storage,
                    bytes,
                    directory,
                    kind,
                    plan: extraction,
                };
                ZIP_INSTALL_PROGRESS_BYTES.store(0, Ordering::Relaxed);
                ZIP_INSTALL_TOTAL_BYTES.store(
                    request.plan.total_bytes().min(u32::MAX as u64) as u32,
                    Ordering::Release,
                );
                unsafe { addr_of_mut!(ZIP_INSTALL_REQUEST).write(MaybeUninit::new(request)) };
                ZIP_INSTALL_STATE.store(ZIP_INSTALL_REQUESTED, Ordering::Release);
                let created = unsafe {
                    ogc_lwp::ThreadHandle::create(
                        &mut *addr_of_mut!(ZIP_INSTALL_THREAD),
                        Some(zip_install_entry),
                        null_mut(),
                        addr_of_mut!(ZIP_INSTALL_STACK.0).cast(),
                        STORAGE_SCAN_STACK_SIZE as u32,
                        STORAGE_SCAN_PRIORITY,
                    )
                };
                match created {
                    Ok(thread) => {
                        self.zip_install_thread = Some(thread);
                        self.runtime.show_wiiload_installing();
                    }
                    Err(error) => {
                        let request =
                            unsafe { addr_of_mut!(ZIP_INSTALL_REQUEST).read().assume_init_read() };
                        self.storage = Some(request.storage);
                        ZIP_INSTALL_STATE.store(ZIP_INSTALL_IDLE, Ordering::Release);
                        return Err(StartupError::ScanWorker(error));
                    }
                }
            }
        }
        Ok(())
    }

    /// Join a completed ZIP install, restore storage ownership, and report success.
    pub fn poll_received_zip_install(&mut self) -> Option<ZipInstallCompletion> {
        if ZIP_INSTALL_STATE.load(Ordering::Acquire) != ZIP_INSTALL_READY {
            return None;
        }
        let worker_join = self
            .zip_install_thread
            .take()
            .ok_or(ZipInstallJoinError::MissingWorker)
            .and_then(|thread| {
                thread
                    .join()
                    .map(|_| ())
                    .map_err(ZipInstallJoinError::Failed)
            });
        let result = unsafe { addr_of_mut!(ZIP_INSTALL_RESULT).read().assume_init_read() };
        self.storage = Some(result.storage);
        let operation = result.catalog.map(|catalog| {
            self.runtime.replace_catalog(catalog);
            self.icon_textures.clear();
            self.attempted_icon_indices.clear();
        });
        ZIP_INSTALL_STATE.store(ZIP_INSTALL_IDLE, Ordering::Release);
        self.runtime.finish_wiiload_installing();
        Some(ZipInstallCompletion {
            operation,
            cleanup_error: result.cleanup_error,
            worker_join,
        })
    }

    pub fn received_zip_install_pending(&self) -> bool {
        self.zip_install_thread.is_some()
    }

    pub fn received_zip_install_progress(&self) -> Option<u8> {
        if !self.received_zip_install_pending() {
            return None;
        }
        let total = ZIP_INSTALL_TOTAL_BYTES.load(Ordering::Acquire);
        let completed = ZIP_INSTALL_PROGRESS_BYTES.load(Ordering::Acquire);
        Some(crate::wiiload::progress_percent(completed, total))
    }

    /// Initialize controller services, load NAND settings, and scan the
    /// preferred SD or USB volume once. Missing settings and volumes use the
    /// same empty/default state as HBC.
    pub fn initialize(view_width: u16, view_height: u16) -> Self {
        // C HBC resolves its title data directory and reads the installed theme
        // before controls_init()/WPAD_Init(). Keep all synchronous ES/ISFS
        // startup reads ahead of WPAD as well. WPAD can queue IOS work, and a
        // later synchronous ES or ISFS request made behind a full IPC queue can
        // return IPC_EQUEUEFULL (-8).
        #[cfg(feature = "startup-diagnostics")]
        super::diagnostics::mark("NAND SETTINGS LOAD");
        let (loaded_settings, settings_load_error) = match settings::load() {
            Ok(settings) => (settings, None),
            Err(settings::SettingsIoError::Missing) => (UserSettings::default(), None),
            Err(error) => (UserSettings::default(), Some(error)),
        };
        let widescreen = view_width > crate::render::LOGICAL_WIDTH;
        #[cfg(feature = "startup-diagnostics")]
        super::diagnostics::mark("NAND THEME LOAD");
        let theme_archive = match super::theme::load() {
            Ok(theme) => Ok(theme),
            Err(error) => Err(error),
        };
        #[cfg(feature = "startup-diagnostics")]
        super::diagnostics::mark("BOOTMII TITLE CHECK");
        let bootmii_available = bootmii_ios_available();
        // Match C startup ordering for NAND theme reads: title/NAND discovery
        // and the initial theme file read precede WPAD setup.
        let input_error = input::initialize(view_width, view_height).err();

        let preference = loaded_settings.device;
        let mut runtime = AppRuntime::new(Vec::new());
        runtime.set_settings_persistence_enabled(settings_load_error.is_none());
        if let Some(error) = settings_load_error {
            runtime.show_notice_with_kind(
                &format!("Could not load saved settings ({error:?}). HBC will not overwrite them."),
                crate::app::NoticeKind::Warning,
            );
        }
        if input_error.is_some() {
            runtime.show_notice_with_kind(
                "Could not start Wii Remote services. GameCube controller input remains available.",
                crate::app::NoticeKind::Warning,
            );
        }
        runtime.set_widescreen(widescreen);
        #[cfg(feature = "startup-diagnostics")]
        super::diagnostics::mark("THEME FILE LOAD");
        let (loaded_theme, theme_textures) = match theme_archive {
            Ok(Some(archive)) => {
                #[cfg(feature = "startup-diagnostics")]
                super::diagnostics::mark("THEME ZIP PARSE");
                match crate::theme::ThemePackage::parse(&archive) {
                    Ok(package) => {
                        #[cfg(feature = "startup-diagnostics")]
                        super::diagnostics::mark("THEME TEXTURE BUILD");
                        let textures = super::theme::ThemeTextures::load_for_viewport(
                            Some((&package, &archive)),
                            widescreen,
                        );
                        (Some(package.metadata), textures)
                    }
                    Err(error) => {
                        runtime.show_notice_with_kind(
                            &format!("Could not read the installed theme archive ({error:?}). Using the default theme."),
                            crate::app::NoticeKind::Warning,
                        );
                        (None, load_default_theme_textures(widescreen))
                    }
                }
            }
            Ok(None) => (None, load_default_theme_textures(widescreen)),
            Err(error) => {
                runtime.show_notice_with_kind(
                    &format!(
                        "Could not load the installed theme ({error:?}). Using the default theme."
                    ),
                    crate::app::NoticeKind::Warning,
                );
                (None, load_default_theme_textures(widescreen))
            }
        };
        #[cfg(feature = "startup-diagnostics")]
        super::diagnostics::mark("THEME TEXTURES READY");
        if let Some(theme) = loaded_theme {
            runtime.apply_theme(theme);
        }
        runtime.set_bootmii_available(bootmii_available);

        let mut startup = Self {
            runtime,
            volume: None,
            storage: None,
            icon_textures: Vec::new(),
            attempted_icon_indices: Vec::new(),
            theme_textures,
            widescreen,
            pending_settings: Some(loaded_settings),
            scan_thread: None,
            scan_previous_volume: None,
            scan_requested_volume: None,
            storage_presence: super::storage::StoragePresence::default(),
            zip_install_thread: None,
            device_probe_thread: None,
        };
        // Start the first mount/catalog pass on a lower-priority storage worker.
        // It is not scheduled here. The browser loop gives it a bounded slice
        // only after a frame has reached VI.
        #[cfg(feature = "startup-diagnostics")]
        super::diagnostics::mark("STORAGE WORKER START");
        if startup.start_storage_scan(preference, false, None).is_err() {
            startup
                .runtime
                .show_notice("Could not start storage discovery.");
        }
        startup
    }

    pub fn runtime(&self) -> &AppRuntime {
        &self.runtime
    }

    pub fn runtime_mut(&mut self) -> &mut AppRuntime {
        &mut self.runtime
    }

    pub fn mounted_volume(&self) -> Option<Volume> {
        self.volume
    }

    /// Check whether a received app ZIP would replace an existing app folder.
    pub fn received_zip_destination_exists(&self, directory: &str) -> Result<bool, StartupError> {
        let storage = self.storage.as_ref().ok_or(StartupError::NoStorage)?;
        let path = alloc::format!("apps/{directory}");
        storage
            .path_is_directory(&path)
            .map_err(StartupError::Storage)
    }

    pub fn logical_width(&self) -> u16 {
        if self.widescreen {
            crate::render::WIDE_LOGICAL_WIDTH
        } else {
            crate::render::LOGICAL_WIDTH
        }
    }

    pub fn has_storage(&self) -> bool {
        self.storage.is_some()
    }

    /// Save a PNG screenshot to the currently selected mounted volume.
    pub fn save_screenshot_png(&self, bytes: &[u8]) -> Result<String, StartupError> {
        self.storage
            .as_ref()
            .ok_or(StartupError::NoStorage)?
            .save_screenshot_png(bytes)
            .map_err(StartupError::Storage)
    }

    pub fn storage_scan_pending(&self) -> bool {
        self.scan_thread.is_some()
    }

    /// Current storage-worker stage for runtime diagnostics.
    pub fn storage_scan_stage_label(&self) -> &'static str {
        match STORAGE_SCAN_STAGE.load(Ordering::Acquire) {
            STORAGE_STAGE_START => "Starting storage scan...",
            STORAGE_STAGE_SHUTDOWN => "Stopping previous storage...",
            STORAGE_STAGE_MOUNT_SD => "Mounting SD...",
            STORAGE_STAGE_MOUNT_USB => "Mounting USB...",
            STORAGE_STAGE_MOUNT_CARDA => "Mounting Card A...",
            STORAGE_STAGE_MOUNT_CARDB => "Mounting Card B...",
            STORAGE_STAGE_RECOVER => "Checking app install state...",
            STORAGE_STAGE_SCAN_APPS => "Scanning apps...",
            STORAGE_STAGE_FINISH => "Finishing storage scan...",
            _ => "Scanning storage...",
        }
    }

    fn start_storage_scan(
        &mut self,
        preference: i32,
        shutdown_previous_mount: bool,
        requested_volume: Option<Volume>,
    ) -> Result<(), StartupError> {
        if self.scan_thread.is_some() || self.device_probe_thread.is_some() {
            return Ok(());
        }
        if STORAGE_SCAN_STATE.load(Ordering::Acquire) != STORAGE_SCAN_IDLE {
            return Err(StartupError::ScanWorker(-1));
        }

        STORAGE_SCAN_PREFERENCE.store(preference, Ordering::Release);
        STORAGE_SCAN_STAGE.store(STORAGE_STAGE_START, Ordering::Release);
        STORAGE_SCAN_SHUTDOWN_OLD.store(shutdown_previous_mount, Ordering::Release);
        self.scan_requested_volume = requested_volume;
        STORAGE_SCAN_STATE.store(STORAGE_SCAN_RUNNING, Ordering::Release);
        let mut raw_thread = ogc_lwp::sys::LWP_THREAD_NULL;
        let created = unsafe {
            ogc_lwp::ThreadHandle::create(
                &mut raw_thread,
                Some(storage_scan_entry),
                null_mut(),
                addr_of_mut!(STORAGE_SCAN_STACK.0).cast(),
                STORAGE_SCAN_STACK_SIZE as u32,
                STORAGE_SCAN_PRIORITY,
            )
        };
        match created {
            Ok(thread) => {
                self.scan_thread = Some(thread);
                // Do not schedule the storage worker before the first GX frame.
                // The main loop gives this lower-priority worker a bounded time
                // slice after each presented frame while scanning is pending.
                Ok(())
            }
            Err(error) => {
                STORAGE_SCAN_STATE.store(STORAGE_SCAN_IDLE, Ordering::Release);
                Err(StartupError::ScanWorker(error))
            }
        }
    }

    /// Join a completed mount-and-scan task and install its owned result.
    pub fn poll_storage_scan(&mut self) -> bool {
        if STORAGE_SCAN_STATE.load(Ordering::Acquire) != STORAGE_SCAN_READY {
            return false;
        }
        let join_failed = self
            .scan_thread
            .take()
            .is_some_and(|thread| thread.join().is_err());
        let result = unsafe { addr_of_mut!(STORAGE_SCAN_RESULT).read().assume_init_read() };
        STORAGE_SCAN_STATE.store(STORAGE_SCAN_IDLE, Ordering::Release);

        let previous_volume = self.scan_previous_volume.take();
        let requested_volume = self.scan_requested_volume.take();
        self.volume = result.volume;
        self.storage = result.storage;
        self.runtime.replace_catalog(result.catalog);
        if let Some(settings) = self.pending_settings.take() {
            self.runtime.load_settings(settings);
        }
        self.runtime.set_widescreen(self.widescreen);
        self.runtime
            .set_active_device(self.volume.map(Volume::settings_index));
        self.icon_textures.clear();
        self.attempted_icon_indices.clear();

        for (volume, error) in result.mount_errors {
            if let Some(error) = error
                .filter(|error| *error != StorageError::Fat(ogc_fat::FatError::DeviceUnavailable))
            {
                ogc_system::report(format_args!("HBC: {volume:?} mount failed: {error:?}\n"));
            }
        }
        if let Some(error) = result.shutdown_error {
            self.runtime.show_notice(&alloc::format!(
                "Could not stop the previous storage mount ({error:?}). No new device was mounted."
            ));
        } else if join_failed {
            self.runtime.show_notice(
                "The storage scan worker stopped unexpectedly. Its result was discarded.",
            );
            self.storage = None;
            self.volume = None;
            self.runtime.replace_catalog(Vec::new());
        } else if let Some(error) = result.scan_error {
            self.runtime.show_notice(&alloc::format!(
                "Could not scan apps on this device ({error:?})."
            ));
        } else if let Some(error) = result.recovery_error {
            self.runtime.show_notice(&alloc::format!(
                "Could not recover app storage state ({error:?})."
            ));
        } else if self.volume.is_none() {
            if let Some((volume, error)) = result.mount_errors.iter().find_map(|(volume, error)| {
                error
                    .filter(|error| {
                        *error != StorageError::Fat(ogc_fat::FatError::DeviceUnavailable)
                    })
                    .map(|error| (volume, error))
            }) {
                self.runtime.show_notice(&alloc::format!(
                    "Could not mount {volume:?} storage ({error:?})."
                ));
            } else if previous_volume.is_some() {
                self.storage_presence.report_missing(false);
                self.runtime
                    .show_notice("Storage was removed. Connect an SD or USB device.");
            } else if self.storage_presence.report_missing(false) {
                self.runtime
                    .show_notice("No SD or USB storage device is available.");
            }
        } else if requested_volume.is_some() && self.volume != requested_volume {
            self.runtime
                .show_notice("The selected storage device is not available.");
        }
        if self.volume.is_some() {
            self.storage_presence.report_missing(true);
        }
        true
    }

    pub fn refresh_device_availability(&mut self) -> Result<bool, StartupError> {
        if self.device_probe_thread.is_some()
            || self.storage_scan_pending()
            || self.received_zip_install_pending()
        {
            return Ok(false);
        }
        if DEVICE_PROBE_STATE.load(Ordering::Acquire) != DEVICE_PROBE_IDLE {
            return Ok(false);
        }
        DEVICE_PROBE_ACTIVE.store(
            self.volume.map_or(2, Volume::settings_index) as u8,
            Ordering::Release,
        );
        DEVICE_PROBE_STATE.store(DEVICE_PROBE_RUNNING, Ordering::Release);
        let mut raw_thread = ogc_lwp::sys::LWP_THREAD_NULL;
        let created = unsafe {
            ogc_lwp::ThreadHandle::create(
                &mut raw_thread,
                Some(device_probe_entry),
                null_mut(),
                addr_of_mut!(DEVICE_PROBE_STACK.0).cast(),
                STORAGE_SCAN_STACK_SIZE as u32,
                STORAGE_SCAN_PRIORITY,
            )
        };
        match created {
            Ok(thread) => {
                self.device_probe_thread = Some(thread);
                Ok(true)
            }
            Err(error) => {
                DEVICE_PROBE_STATE.store(DEVICE_PROBE_IDLE, Ordering::Release);
                Err(StartupError::ScanWorker(error))
            }
        }
    }

    pub fn poll_device_availability(&mut self) -> Result<bool, StartupError> {
        if DEVICE_PROBE_STATE.load(Ordering::Acquire) != DEVICE_PROBE_READY {
            return Ok(false);
        }
        let join_result = self
            .device_probe_thread
            .take()
            .map(|thread| thread.join().map(|_| ()))
            .transpose();
        let result = unsafe { addr_of_mut!(DEVICE_PROBE_RESULT).read().assume_init_read() };
        self.runtime.set_device_availability(result.availability);
        DEVICE_PROBE_STATE.store(DEVICE_PROBE_IDLE, Ordering::Release);
        join_result.map_err(StartupError::ScanWorker)?;
        if let Some(error) = result.error {
            return Err(StartupError::Storage(StorageError::Fat(error)));
        }
        Ok(true)
    }

    pub fn device_probe_pending(&self) -> bool {
        self.device_probe_thread.is_some()
    }

    pub fn icon_textures(&self) -> &[IconTexture] {
        &self.icon_textures
    }

    pub fn theme_textures(&self) -> &super::theme::ThemeTextures {
        &self.theme_textures
    }

    /// Decode visible HBC-sized icons once and retain their aligned GX data.
    /// Bad, missing, and oversized optional icons remain omitted from the frame.
    pub fn refresh_visible_icons(&mut self) {
        if self.storage_scan_pending()
            || self.received_zip_install_pending()
            || self.device_probe_pending()
        {
            return;
        }
        let visible: Vec<(usize, alloc::string::String)> = {
            let catalog = self.runtime.catalog();
            self.runtime
                .controller()
                .visible_page(catalog)
                .into_iter()
                .filter(|entry| entry.has_icon)
                .filter_map(|entry| {
                    catalog
                        .iter()
                        .position(|candidate| candidate.directory == entry.directory)
                        .map(|index| (index, entry.directory.clone()))
                })
                .collect()
        };

        let Some(storage) = self.storage.as_mut() else {
            return;
        };
        for (catalog_index, directory) in visible {
            if self.attempted_icon_indices.contains(&catalog_index) {
                continue;
            }
            if self.attempted_icon_indices.try_reserve(1).is_err() {
                continue;
            }
            self.attempted_icon_indices.push(catalog_index);

            let path = alloc::format!("apps/{directory}/icon.png");
            let Ok(Some(bytes)) = storage.read_file(&path, crate::png::HBC_APP_ICON_MAX_PNG_BYTES)
            else {
                continue;
            };
            let Ok(texture) = crate::png::decode_hbc_app_icon(&bytes) else {
                continue;
            };
            if self.icon_textures.try_reserve(1).is_err() {
                continue;
            }
            self.icon_textures.push(IconTexture {
                catalog_index,
                texture,
            });
        }
    }

    /// Retry device selection and catalog scanning after media removal or
    /// insertion. Call this every 30 frames, as HBC does.
    pub fn refresh_storage(&mut self) -> Result<bool, StartupError> {
        if self.storage_scan_pending()
            || self.received_zip_install_pending()
            || self.device_probe_pending()
        {
            return Ok(false);
        }
        if self
            .volume
            .is_some_and(|volume| ogc_fat::is_mounted(volume.mount_name()))
        {
            return Ok(false);
        }

        let previous_volume = self.volume;
        let preference = self.runtime.settings().current().device;
        self.scan_previous_volume = previous_volume;
        self.start_storage_scan(preference, previous_volume.is_some(), None)?;
        Ok(true)
    }

    /// Delete the selected app folder and remove it from the live catalog.
    pub fn delete_selected_app(&mut self) -> Result<(), StartupError> {
        let directory = self
            .runtime
            .controller()
            .selected_app
            .as_deref()
            .ok_or(StartupError::NoSelection)?;
        let storage = self.storage.as_ref().ok_or(StartupError::NoStorage)?;
        storage
            .remove_app_folder(directory)
            .map_err(StartupError::Storage)?;
        let catalog = self
            .runtime
            .catalog()
            .iter()
            .filter(|entry| entry.directory != directory)
            .cloned()
            .collect();
        self.runtime.replace_catalog(catalog);
        self.icon_textures.clear();
        self.attempted_icon_indices.clear();
        Ok(())
    }

    /// Read and validate the selected ELF or DOL without writing Wii memory.
    pub fn prepare_selected_executable(&mut self) -> Result<LaunchImage, StartupError> {
        let entry = self
            .runtime
            .catalog()
            .iter()
            .find(|entry| {
                self.runtime.controller().selected_app.as_deref() == Some(&entry.directory)
            })
            .cloned()
            .ok_or(StartupError::NoSelection)?;
        let filename = match entry.kind {
            AppKind::Elf => "boot.elf",
            AppKind::Dol => "boot.dol",
            AppKind::Theme => return Err(StartupError::NotExecutable),
        };
        let relative_path = alloc::format!("apps/{}/{}", entry.directory, filename);
        let storage = self.storage.as_mut().ok_or(StartupError::NoStorage)?;
        let bytes = storage
            .read_file(
                &relative_path,
                usize::try_from(entry.size).map_err(|_| StartupError::TooLarge)?,
            )
            .map_err(StartupError::Storage)?
            .ok_or(StartupError::MissingExecutable)?;
        if bytes.len() as u64 != entry.size {
            return Err(StartupError::MissingExecutable);
        }
        let prefix = storage.prefix();
        let executable_path = alloc::format!("{prefix}/{relative_path}");
        let arguments = entry
            .metadata
            .as_ref()
            .map(|metadata| metadata.arguments.as_slice())
            .unwrap_or(&[]);
        let launch_arguments =
            build_launch_arguments(&executable_path, arguments).map_err(StartupError::Loader)?;
        let plan = match entry.kind {
            AppKind::Elf => parse_elf32_ppc(&bytes, Some(EXECUTABLE_MEMORY.clone())),
            AppKind::Dol => parse_dol(&bytes, Some(EXECUTABLE_MEMORY.clone())),
            AppKind::Theme => return Err(StartupError::NotExecutable),
        }
        .map_err(StartupError::Loader)?;

        Ok(LaunchImage {
            bytes,
            plan,
            arguments: launch_arguments,
            ahb_access: entry
                .metadata
                .as_ref()
                .is_some_and(|metadata| metadata.ahb_access),
        })
    }

    /// Read a selected theme ZIP and parse its bounded metadata.
    pub fn prepare_selected_theme(&mut self) -> Result<PreparedTheme, StartupError> {
        let entry = self
            .runtime
            .catalog()
            .iter()
            .find(|entry| {
                self.runtime.controller().selected_app.as_deref() == Some(&entry.directory)
            })
            .cloned()
            .ok_or(StartupError::NoSelection)?;
        if entry.kind != AppKind::Theme {
            return Err(StartupError::NotExecutable);
        }
        let size = usize::try_from(entry.size).map_err(|_| StartupError::TooLarge)?;
        if size > crate::theme::MAX_THEME_ARCHIVE_BYTES {
            return Err(StartupError::TooLarge);
        }
        let relative_path = alloc::format!("apps/{}/theme.zip", entry.directory);
        let storage = self.storage.as_mut().ok_or(StartupError::NoStorage)?;
        let bytes = storage
            .read_file(&relative_path, size)
            .map_err(StartupError::Storage)?
            .ok_or(StartupError::MissingExecutable)?;
        if bytes.len() != size {
            return Err(StartupError::MissingExecutable);
        }
        let package = crate::theme::ThemePackage::parse(&bytes).map_err(StartupError::Theme)?;
        let textures = super::theme::ThemeTextures::load_for_viewport(
            Some((&package, &bytes)),
            self.widescreen,
        );
        super::theme::save(&bytes).map_err(StartupError::ThemeStorage)?;
        Ok(PreparedTheme {
            metadata: package.metadata,
            textures,
        })
    }

    pub fn apply_theme(&mut self, theme: PreparedTheme) {
        self.runtime.apply_theme(theme.metadata);
        self.theme_textures = theme.textures;
    }

    /// Apply the confirmed storage preference and refresh the app catalog.
    pub fn apply_options(&mut self) -> Result<bool, StartupError> {
        if self.device_probe_pending() {
            return Err(StartupError::StorageScanPending);
        }
        let preferred = self.runtime.settings().current().device;
        let wanted = Volume::from_settings_index(preferred).unwrap_or(Volume::Sd);
        if self.volume == Some(wanted) && ogc_fat::is_mounted(wanted.mount_name()) {
            return Ok(true);
        }
        if self.storage.is_some() {
            ogc_fat::shutdown_all()
                .map_err(|error| StartupError::Storage(StorageError::Fat(error)))?;
            self.storage = None;
            self.volume = None;
        }
        let scan = mount_preferred_volume(preferred);
        for (volume, error) in scan.mount_errors {
            if let Some(error) = error
                .filter(|error| *error != StorageError::Fat(ogc_fat::FatError::DeviceUnavailable))
            {
                ogc_system::report(format_args!("HBC: {volume:?} mount failed: {error:?}\n"));
            }
        }
        self.volume = scan.volume;
        self.storage = scan.storage;
        self.runtime
            .set_active_device(scan.volume.map(Volume::settings_index));
        self.runtime.replace_catalog(scan.catalog);
        self.icon_textures.clear();
        self.attempted_icon_indices.clear();
        let selected = scan.volume == Some(wanted);
        if !selected {
            if let Some(active) = scan.volume {
                self.runtime.set_preferred_device(active.settings_index());
            }
        }
        if let Some(error) = scan.scan_error {
            return Err(StartupError::CatalogScan(error));
        }
        if let Some(error) = scan.recovery_error {
            return Err(StartupError::Recovery(error));
        }
        if scan.volume.is_none() {
            if let Some((volume, error)) =
                scan.mount_errors.into_iter().find_map(|(volume, error)| {
                    error
                        .filter(|error| {
                            *error != StorageError::Fat(ogc_fat::FatError::DeviceUnavailable)
                        })
                        .map(|error| (volume, error))
                })
            {
                return Err(StartupError::StorageMount { volume, error });
            }
        }
        Ok(selected)
    }

    /// Unmount the selected volume before the one-way executable handoff.
    pub fn unmount_storage(&mut self) -> Result<(), StartupError> {
        if self.storage_scan_pending() || self.device_probe_pending() {
            return Err(StartupError::StorageScanPending);
        }
        ogc_fat::shutdown_all().map_err(|error| StartupError::Storage(StorageError::Fat(error)))?;
        self.storage = None;
        self.volume = None;
        Ok(())
    }
}

#[repr(align(32))]
struct AlignedTmdBuffer([u8; 1024]);

/// Match HBC's BootMii marker check with an aligned, bounded TMD view buffer.
fn bootmii_ios_available() -> bool {
    let mut size = 0u32;
    if unsafe { ogc_es_sys::get_tmd_view_size(BOOTMII_TITLE_ID, &mut size) } < 0
        || !(90..=1024).contains(&size)
    {
        return false;
    }

    let mut buffer = AlignedTmdBuffer([0; 1024]);
    if unsafe {
        ogc_es_sys::get_tmd_view(
            BOOTMII_TITLE_ID,
            buffer.0.as_mut_ptr().cast::<ogc_es_sys::TmdView>(),
            size,
        )
    } < 0
    {
        return false;
    }
    buffer.0[50] == b'B' && buffer.0[51] == b'M'
}

pub struct LaunchImage {
    pub bytes: Vec<u8>,
    pub plan: LoadPlan,
    pub arguments: Vec<u8>,
    /// The app metadata requests access to hardware registers after launch.
    pub ahb_access: bool,
}

pub struct PreparedTheme {
    pub metadata: ThemeMetadata,
    textures: super::theme::ThemeTextures,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StartupError {
    NoSelection,
    NoStorage,
    CatalogScan(crate::catalog::ScanError<StorageError>),
    MissingExecutable,
    NotExecutable,
    TooLarge,
    Storage(StorageError),
    ScanWorker(i32),
    StorageScanPending,
    Loader(ParseError),
    Theme(crate::theme::ThemePackageError),
    ThemeStorage(super::settings::SettingsIoError),
    StorageMount { volume: Volume, error: StorageError },
    Recovery(ZipInstallError),
}

fn mount_preferred_volume(preference: i32) -> MountScanResult {
    let order = device_scan_order(preference);
    let mut mount_errors = order.map(|volume| (volume, None));
    for (index, volume) in order.into_iter().enumerate() {
        let mount_stage = match volume {
            Volume::Sd => STORAGE_STAGE_MOUNT_SD,
            Volume::Usb => STORAGE_STAGE_MOUNT_USB,
            Volume::CardA => STORAGE_STAGE_MOUNT_CARDA,
            Volume::CardB => STORAGE_STAGE_MOUNT_CARDB,
        };
        STORAGE_SCAN_STAGE.store(mount_stage, Ordering::Release);
        let mut storage = match FatStorage::mount(volume) {
            Ok(storage) => storage,
            Err(error) => {
                mount_errors[index].1 = Some(error);
                continue;
            }
        };
        STORAGE_SCAN_STAGE.store(STORAGE_STAGE_RECOVER, Ordering::Release);
        let recovery_error = recover_app_install_artifacts(&mut storage).err();
        if recovery_error.is_some() {
            return MountScanResult {
                volume: Some(volume),
                storage: Some(storage),
                catalog: Vec::new(),
                scan_error: None,
                mount_errors,
                recovery_error,
            };
        }
        STORAGE_SCAN_STAGE.store(STORAGE_STAGE_SCAN_APPS, Ordering::Release);
        // A missing apps directory is a valid empty catalog. Keep the mounted
        // volume so later install, refresh, and delete actions can use it.
        let (catalog, scan_error) = match scan_apps(
            &mut storage,
            "apps",
            EXECUTABLE_SIZE_LIMIT,
            THEME_SIZE_LIMIT,
        ) {
            Ok(catalog) => (catalog, None),
            Err(crate::catalog::ScanError::AppDirectory(StorageError::Fat(
                ogc_fat::FatError::NotFound,
            ))) => (Vec::new(), None),
            Err(error) => (Vec::new(), Some(error)),
        };
        return MountScanResult {
            volume: Some(volume),
            storage: Some(storage),
            catalog,
            scan_error,
            mount_errors,
            recovery_error: None,
        };
    }

    MountScanResult {
        volume: None,
        storage: None,
        catalog: Vec::new(),
        scan_error: None,
        mount_errors,
        recovery_error: None,
    }
}
