//! Boot-time HBC service and application catalog setup.
//!
//! The FAT mount and initial catalog scan run on a worker. The app thread owns
//! the mounted adapter after the worker returns and performs later file access.

use alloc::vec::Vec;
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
        storage::{device_scan_order, FatStorage, StorageError, Volume},
    },
    runtime::AppRuntime,
};

/// HBC reserves this address range for loading an application executable.
const EXECUTABLE_SIZE_LIMIT: u64 = (0x8133_0000 - 1 - 1024) - 0x8000_3400;
const THEME_SIZE_LIMIT: u64 = 20 * 1024 * 1024;
const BOOTMII_TITLE_ID: u64 = 0x0000_0001_0000_00fe;
const STORAGE_SCAN_STACK_SIZE: usize = 12 * 1024;
const STORAGE_SCAN_PRIORITY: u8 = 30;
const STORAGE_SCAN_IDLE: u8 = 0;
const STORAGE_SCAN_RUNNING: u8 = 1;
const STORAGE_SCAN_READY: u8 = 2;
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
    scan_failed: bool,
}

#[repr(align(32))]
struct StorageScanStack([u8; STORAGE_SCAN_STACK_SIZE]);

static STORAGE_SCAN_STATE: AtomicU8 = AtomicU8::new(STORAGE_SCAN_IDLE);
static STORAGE_SCAN_PREFERENCE: AtomicI32 = AtomicI32::new(0);
static STORAGE_SCAN_SHUTDOWN_OLD: AtomicBool = AtomicBool::new(false);
static mut STORAGE_SCAN_RESULT: MaybeUninit<StorageScanResult> = MaybeUninit::uninit();
static mut STORAGE_SCAN_STACK: StorageScanStack = StorageScanStack([0; STORAGE_SCAN_STACK_SIZE]);
static DEVICE_PROBE_STATE: AtomicU8 = AtomicU8::new(DEVICE_PROBE_IDLE);
static DEVICE_PROBE_ACTIVE: AtomicU8 = AtomicU8::new(2);
static mut DEVICE_PROBE_RESULT: MaybeUninit<[bool; 4]> = MaybeUninit::uninit();
static mut DEVICE_PROBE_STACK: StorageScanStack = StorageScanStack([0; STORAGE_SCAN_STACK_SIZE]);

struct ZipInstallRequest {
    storage: FatStorage,
    bytes: Vec<u8>,
    directory: alloc::string::String,
    plan: crate::management::ExtractionPlan,
}

struct ZipInstallResult {
    storage: FatStorage,
    catalog: Option<Vec<AppEntry>>,
}

static ZIP_INSTALL_STATE: AtomicU8 = AtomicU8::new(ZIP_INSTALL_IDLE);
static ZIP_INSTALL_PROGRESS_BYTES: AtomicU32 = AtomicU32::new(0);
static ZIP_INSTALL_TOTAL_BYTES: AtomicU32 = AtomicU32::new(0);
static mut ZIP_INSTALL_REQUEST: MaybeUninit<ZipInstallRequest> = MaybeUninit::uninit();
static mut ZIP_INSTALL_RESULT: MaybeUninit<ZipInstallResult> = MaybeUninit::uninit();
static mut ZIP_INSTALL_THREAD: ogc_lwp::sys::LwpThread = ogc_lwp::sys::LWP_THREAD_NULL;
static mut ZIP_INSTALL_STACK: StorageScanStack = StorageScanStack([0; STORAGE_SCAN_STACK_SIZE]);

unsafe fn zip_install_entry(_argument: *mut c_void) -> *mut c_void {
    ZIP_INSTALL_STATE.store(ZIP_INSTALL_RUNNING, Ordering::Release);
    let request = unsafe { addr_of_mut!(ZIP_INSTALL_REQUEST).read().assume_init_read() };
    let mut storage = request.storage;
    let archive = crate::archive::ZipArchive::parse_limited(
        &request.bytes,
        crate::wiiload_zip::WIILOAD_ZIP_MAX_ENTRIES,
    );
    let catalog = archive.ok().and_then(|archive| {
        storage
            .ensure_directory("apps")
            .and_then(|()| {
                let destination = alloc::format!("apps/{}", request.directory);
                storage.ensure_directory(&destination)?;
                storage.extract_archive_with_progress(
                    &request.bytes,
                    &archive,
                    &request.plan,
                    &destination,
                    |completed, total| {
                        ZIP_INSTALL_TOTAL_BYTES
                            .store(total.min(u32::MAX as u64) as u32, Ordering::Relaxed);
                        ZIP_INSTALL_PROGRESS_BYTES
                            .store(completed.min(u32::MAX as u64) as u32, Ordering::Release);
                    },
                )
            })
            .ok()
            .and_then(|()| {
                scan_apps(
                    &mut storage,
                    "apps",
                    EXECUTABLE_SIZE_LIMIT,
                    THEME_SIZE_LIMIT,
                )
                .ok()
            })
    });
    let result = ZipInstallResult { storage, catalog };
    unsafe { addr_of_mut!(ZIP_INSTALL_RESULT).write(MaybeUninit::new(result)) };
    ZIP_INSTALL_STATE.store(ZIP_INSTALL_READY, Ordering::Release);
    null_mut()
}

fn probe_one_device(volume: Volume) -> bool {
    let name = volume.mount_name();
    if ogc_fat::is_mounted(name) {
        return true;
    }
    let mounted = FatStorage::mount(volume).is_ok();
    if !mounted {
        return false;
    }
    let available = ogc_fat::is_mounted(name);
    let _ = ogc_fat::unmount(name);
    available
}

unsafe fn device_probe_entry(_argument: *mut c_void) -> *mut c_void {
    DEVICE_PROBE_STATE.store(DEVICE_PROBE_RUNNING, Ordering::Release);
    let active = match DEVICE_PROBE_ACTIVE.load(Ordering::Acquire) {
        index @ 0..=3 => Volume::from_settings_index(i32::from(index))
            .filter(|volume| ogc_fat::is_mounted(volume.mount_name()))
            .map(|_| index as usize),
        _ => None,
    };
    let availability = crate::app::device_availability_snapshot(
        active,
        [
            probe_one_device(Volume::Sd),
            probe_one_device(Volume::Usb),
            probe_one_device(Volume::CardA),
            probe_one_device(Volume::CardB),
        ],
    );
    unsafe { addr_of_mut!(DEVICE_PROBE_RESULT).write(MaybeUninit::new(availability)) };
    DEVICE_PROBE_STATE.store(DEVICE_PROBE_READY, Ordering::Release);
    null_mut()
}

unsafe fn storage_scan_entry(_argument: *mut c_void) -> *mut c_void {
    if STORAGE_SCAN_SHUTDOWN_OLD.load(Ordering::Acquire) {
        let _ = ogc_fat::shutdown_all();
    }
    let preference = STORAGE_SCAN_PREFERENCE.load(Ordering::Acquire);
    let (volume, storage, catalog, scan_failed) = mount_preferred_volume(preference);
    let result = StorageScanResult {
        volume,
        storage,
        catalog,
        scan_failed,
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
                kind: _,
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
    pub fn poll_received_zip_install(&mut self) -> Option<bool> {
        if ZIP_INSTALL_STATE.load(Ordering::Acquire) != ZIP_INSTALL_READY {
            return None;
        }
        if let Some(thread) = self.zip_install_thread.take() {
            let _ = thread.join();
        }
        let result = unsafe { addr_of_mut!(ZIP_INSTALL_RESULT).read().assume_init_read() };
        self.storage = Some(result.storage);
        let succeeded = if let Some(catalog) = result.catalog {
            self.runtime.replace_catalog(catalog);
            self.icon_textures.clear();
            self.attempted_icon_indices.clear();
            true
        } else {
            false
        };
        ZIP_INSTALL_STATE.store(ZIP_INSTALL_IDLE, Ordering::Release);
        self.runtime.finish_wiiload_installing();
        Some(succeeded)
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
        input::initialize(view_width, view_height);

        #[cfg(feature = "startup-diagnostics")]
        super::diagnostics::mark("NAND SETTINGS LOAD");
        let loaded_settings = settings::load().unwrap_or_default();
        let preference = loaded_settings.device;
        let mut runtime = AppRuntime::new(Vec::new());
        let widescreen = view_width > crate::render::LOGICAL_WIDTH;
        runtime.set_widescreen(widescreen);
        #[cfg(feature = "startup-diagnostics")]
        super::diagnostics::mark("NAND THEME LOAD");
        #[cfg(feature = "startup-diagnostics")]
        super::diagnostics::mark("THEME FILE LOAD");
        let (loaded_theme, theme_textures) = match super::theme::load() {
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
                    Err(_) => (
                        None,
                        super::theme::ThemeTextures::load_for_viewport(None, widescreen),
                    ),
                }
            }
            _ => (
                None,
                super::theme::ThemeTextures::load_for_viewport(None, widescreen),
            ),
        };
        #[cfg(feature = "startup-diagnostics")]
        super::diagnostics::mark("THEME TEXTURES READY");
        if let Some(theme) = loaded_theme {
            runtime.apply_theme(theme);
        }
        #[cfg(feature = "startup-diagnostics")]
        super::diagnostics::mark("BOOTMII TITLE CHECK");
        runtime.set_bootmii_available(bootmii_ios_available());

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
            zip_install_thread: None,
            device_probe_thread: None,
        };
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

    pub fn storage_scan_pending(&self) -> bool {
        self.scan_thread.is_some()
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
        if let Some(thread) = self.scan_thread.take() {
            let _ = thread.join();
        }
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

        if result.scan_failed {
            self.runtime
                .show_notice("Could not read the apps folder on this device.");
        } else if requested_volume.is_some() && self.volume != requested_volume {
            self.runtime
                .show_notice("The selected storage device is not available.");
        } else if previous_volume.is_some() && self.volume.is_none() {
            self.runtime
                .show_notice("Storage was removed. Connect an SD or USB device.");
        } else if self.volume.is_none() {
            self.runtime
                .show_notice("No SD or USB storage device is available.");
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

    pub fn poll_device_availability(&mut self) -> bool {
        if DEVICE_PROBE_STATE.load(Ordering::Acquire) != DEVICE_PROBE_READY {
            return false;
        }
        if let Some(thread) = self.device_probe_thread.take() {
            let _ = thread.join();
        }
        let availability = unsafe { addr_of_mut!(DEVICE_PROBE_RESULT).read().assume_init_read() };
        self.runtime.set_device_availability(availability);
        DEVICE_PROBE_STATE.store(DEVICE_PROBE_IDLE, Ordering::Release);
        true
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
    /// Bad, missing, and oversized optional icons stay on the placeholder path.
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
            if let Ok(byte_len) = u32::try_from(texture.len()) {
                unsafe { ogc_cache::dc_flush_range(texture.as_ptr().cast(), byte_len) };
            } else {
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
        let (volume, storage, catalog, scan_failed) = mount_preferred_volume(preferred);
        self.volume = volume;
        self.storage = storage;
        self.runtime
            .set_active_device(volume.map(Volume::settings_index));
        self.runtime.replace_catalog(catalog);
        self.icon_textures.clear();
        self.attempted_icon_indices.clear();
        let selected = volume == Some(wanted);
        if !selected {
            if let Some(active) = volume {
                self.runtime.set_preferred_device(active.settings_index());
            }
        }
        if scan_failed {
            return Err(StartupError::CatalogScan);
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
    CatalogScan,
    MissingExecutable,
    NotExecutable,
    TooLarge,
    Storage(StorageError),
    ScanWorker(i32),
    StorageScanPending,
    Loader(ParseError),
    Theme(crate::theme::ThemePackageError),
    ThemeStorage(super::settings::SettingsIoError),
}

fn mount_preferred_volume(
    preference: i32,
) -> (Option<Volume>, Option<FatStorage>, Vec<AppEntry>, bool) {
    for volume in device_scan_order(preference) {
        let Ok(mut storage) = FatStorage::mount(volume) else {
            continue;
        };
        // A missing apps directory is a valid empty catalog. Keep the mounted
        // volume so later install, refresh, and delete actions can use it.
        let (catalog, scan_failed) = match scan_apps(
            &mut storage,
            "apps",
            EXECUTABLE_SIZE_LIMIT,
            THEME_SIZE_LIMIT,
        ) {
            Ok(catalog) => (catalog, false),
            Err(crate::catalog::ScanError::AppDirectory(StorageError::Fat(
                ogc_fat::FatError::NotFound,
            ))) => (Vec::new(), false),
            Err(_) => (Vec::new(), true),
        };
        return (Some(volume), Some(storage), catalog, scan_failed);
    }

    (None, None, Vec::new(), false)
}
