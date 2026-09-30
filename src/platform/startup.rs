//! Boot-time HBC service and application catalog setup.
//!
//! The FAT mount and initial catalog scan run on a worker. The app thread owns
//! the mounted adapter after the worker returns and performs later file access.

use alloc::vec::Vec;
use core::{
    ffi::c_void,
    mem::MaybeUninit,
    ptr::{addr_of_mut, null_mut},
    sync::atomic::{AtomicBool, AtomicI32, AtomicU8, Ordering},
};

use crate::{
    catalog::{scan_apps, AppEntry, AppKind, CatalogStorage},
    loader::{build_launch_arguments, parse_dol, parse_elf32_ppc, LoadPlan, ParseError},
    metadata::{ThemeMetadata, UserSettings},
    platform::{
        input, settings,
        storage::{FatStorage, StorageError, Volume},
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
}

pub struct IconTexture {
    pub catalog_index: usize,
    pub texture: crate::png::GxRgba8Texture,
}

impl WiiStartup {
    /// Install an app ZIP or save and apply a validated theme ZIP received by Wiiload.
    pub fn apply_received_zip(&mut self, bytes: &[u8]) -> Result<(), StartupError> {
        use crate::wiiload_zip::{classify_received_zip, WiiloadZipClassification};
        match classify_received_zip(bytes).map_err(|error| match error {
            crate::wiiload_zip::WiiloadZipError::InvalidTheme(error) => StartupError::Theme(error),
            crate::wiiload_zip::WiiloadZipError::InvalidArchive(error) => {
                StartupError::Storage(StorageError::Archive(error))
            }
            crate::wiiload_zip::WiiloadZipError::InvalidApp => StartupError::NotExecutable,
        })? {
            WiiloadZipClassification::Theme { package } => {
                let textures = super::theme::ThemeTextures::load_for_viewport(
                    Some((&package, bytes)),
                    self.widescreen,
                );
                super::theme::save(bytes).map_err(StartupError::ThemeStorage)?;
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
                let archive = crate::archive::ZipArchive::parse_limited(
                    bytes,
                    crate::wiiload_zip::WIILOAD_ZIP_MAX_ENTRIES,
                )
                .map_err(|error| StartupError::Storage(StorageError::Archive(error)))?;
                let storage = self.storage.as_mut().ok_or(StartupError::NoStorage)?;
                storage
                    .ensure_directory("apps")
                    .map_err(StartupError::Storage)?;
                let destination = alloc::format!("apps/{directory}");
                storage
                    .ensure_directory(&destination)
                    .map_err(StartupError::Storage)?;
                storage
                    .extract_archive(bytes, &archive, &extraction, &destination)
                    .map_err(StartupError::Storage)?;
                let storage = self.storage.as_mut().ok_or(StartupError::NoStorage)?;
                let catalog = scan_apps(storage, "apps", EXECUTABLE_SIZE_LIMIT, THEME_SIZE_LIMIT)
                    .map_err(|_| StartupError::CatalogScan)?;
                self.runtime.replace_catalog(catalog);
                self.icon_textures.clear();
                self.attempted_icon_indices.clear();
            }
        }
        Ok(())
    }

    /// Initialize controller services, load NAND settings, and scan the
    /// preferred SD or USB volume once. Missing settings and volumes use the
    /// same empty/default state as HBC.
    pub fn initialize(view_width: u16, view_height: u16) -> Self {
        input::initialize(view_width, view_height);

        let loaded_settings = settings::load().unwrap_or_default();
        let preference = loaded_settings.device;
        let mut runtime = AppRuntime::new(Vec::new());
        let widescreen = view_width > crate::render::LOGICAL_WIDTH;
        runtime.set_widescreen(widescreen);
        let (loaded_theme, theme_textures) = match super::theme::load() {
            Ok(Some(archive)) => match crate::theme::ThemePackage::parse(&archive) {
                Ok(package) => {
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
            },
            _ => (
                None,
                super::theme::ThemeTextures::load_for_viewport(None, widescreen),
            ),
        };
        if let Some(theme) = loaded_theme {
            runtime.apply_theme(theme);
        }
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
        };
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
        if self.scan_thread.is_some() {
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

    pub fn icon_textures(&self) -> &[IconTexture] {
        &self.icon_textures
    }

    pub fn theme_textures(&self) -> &super::theme::ThemeTextures {
        &self.theme_textures
    }

    /// Decode visible HBC-sized icons once and retain their aligned GX data.
    /// Bad, missing, and oversized optional icons stay on the placeholder path.
    pub fn refresh_visible_icons(&mut self) {
        if self.storage_scan_pending() {
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
        if self.storage_scan_pending() {
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
        let preferred = self.runtime.settings().current().device;
        let wanted = if preferred == 1 {
            Volume::Usb
        } else {
            Volume::Sd
        };
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
        if self.storage_scan_pending() {
            return Err(StartupError::StorageScanPending);
        }
        ogc_fat::shutdown_all().map_err(|error| StartupError::Storage(StorageError::Fat(error)))?;
        self.storage = None;
        self.volume = None;
        Ok(())
    }
}

impl Volume {
    fn settings_index(self) -> i32 {
        match self {
            Self::Sd => 0,
            Self::Usb => 1,
        }
    }

    fn mount_name(self) -> &'static core::ffi::CStr {
        match self {
            Self::Sd => c"sd",
            Self::Usb => c"usb",
        }
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
    let volumes = match preference {
        1 => [Volume::Usb, Volume::Sd],
        _ => [Volume::Sd, Volume::Usb],
    };

    for volume in volumes {
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
