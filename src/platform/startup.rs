//! Boot-time HBC service and application catalog setup.
//!
//! Device access is synchronous in the current FAT adapter. This module scans
//! one preferred volume and retains it for later browser actions. It does not
//! start a background rescan worker.

use alloc::vec::Vec;

use crate::{
    catalog::{scan_apps, AppEntry, AppKind, CatalogStorage},
    loader::{build_launch_arguments, parse_dol, parse_elf32_ppc, LoadPlan, ParseError},
    metadata::ThemeMetadata,
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
pub const EXECUTABLE_MEMORY: core::ops::Range<u32> = 0x8000_3400..0x8132_fc00;

/// The result of native Wii startup, ready for the browser loop.
pub struct WiiStartup {
    runtime: AppRuntime,
    volume: Option<Volume>,
    storage: Option<FatStorage>,
    icon_textures: Vec<IconTexture>,
    attempted_icon_indices: Vec<usize>,
    background_texture: Option<crate::png::GxRgba8Texture>,
}

pub struct IconTexture {
    pub catalog_index: usize,
    pub texture: crate::png::GxRgba8Texture,
}

impl WiiStartup {
    /// Initialize controller services, load NAND settings, and scan the
    /// preferred SD or USB volume once. Missing settings and volumes use the
    /// same empty/default state as HBC.
    pub fn initialize(view_width: u16, view_height: u16) -> Self {
        input::initialize(view_width, view_height);

        let loaded_settings = settings::load().unwrap_or_default();
        let (volume, storage, catalog, scan_failed) =
            mount_preferred_volume(loaded_settings.device);
        let mut runtime = AppRuntime::new(catalog);
        runtime.load_settings(loaded_settings);
        let (loaded_theme, background_texture) = match super::theme::load() {
            Ok(Some(archive)) => match crate::theme::ThemePackage::parse(&archive) {
                Ok(package) => {
                    let background = super::theme::background_texture(Some((&package, &archive)));
                    (Some(package.metadata), background)
                }
                Err(_) => (None, super::theme::background_texture(None)),
            },
            _ => (None, super::theme::background_texture(None)),
        };
        if let Some(theme) = loaded_theme {
            runtime.apply_theme(theme);
        }
        runtime.set_bootmii_available(bootmii_ios_available());

        let mut startup = Self {
            runtime,
            volume,
            storage,
            icon_textures: Vec::new(),
            attempted_icon_indices: Vec::new(),
            background_texture,
        };
        if scan_failed {
            startup
                .runtime
                .show_notice("Could not read the apps folder on this device.");
        } else if !startup.has_storage() {
            startup
                .runtime
                .show_notice("No SD or USB storage device is available.");
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

    pub fn has_storage(&self) -> bool {
        self.storage.is_some()
    }

    pub fn icon_textures(&self) -> &[IconTexture] {
        &self.icon_textures
    }

    pub fn background_texture(&self) -> Option<&crate::png::GxRgba8Texture> {
        self.background_texture.as_ref()
    }

    /// Decode visible HBC-sized icons once and retain their aligned GX data.
    /// Bad, missing, and oversized optional icons stay on the placeholder path.
    pub fn refresh_visible_icons(&mut self) {
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
        if self
            .volume
            .is_some_and(|volume| ogc_fat::is_mounted(volume.mount_name()))
        {
            return Ok(false);
        }

        let previous_volume = self.volume;
        if previous_volume.is_some() {
            ogc_fat::shutdown_all()
                .map_err(|error| StartupError::Storage(StorageError::Fat(error)))?;
        }

        let preference = self.runtime.settings().current().device;
        let (volume, storage, catalog, scan_failed) = mount_preferred_volume(preference);
        self.volume = volume;
        self.storage = storage;
        self.runtime.replace_catalog(catalog);
        self.icon_textures.clear();
        self.attempted_icon_indices.clear();

        if scan_failed {
            self.runtime
                .show_notice("Could not read the apps folder on this device.");
        } else if previous_volume.is_some() && volume.is_none() {
            self.runtime
                .show_notice("Storage was removed. Connect an SD or USB device.");
        }
        Ok(previous_volume != volume)
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
        let background = super::theme::background_texture(Some((&package, &bytes)));
        super::theme::save(&bytes).map_err(StartupError::ThemeStorage)?;
        Ok(PreparedTheme {
            metadata: package.metadata,
            background,
        })
    }

    pub fn apply_theme(&mut self, theme: PreparedTheme) {
        self.runtime.apply_theme(theme.metadata);
        self.background_texture = theme.background;
    }

    /// Unmount the selected volume before the one-way executable handoff.
    pub fn unmount_storage(&mut self) -> Result<(), StartupError> {
        ogc_fat::shutdown_all().map_err(|error| StartupError::Storage(StorageError::Fat(error)))?;
        self.storage = None;
        self.volume = None;
        Ok(())
    }
}

impl Volume {
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
    background: Option<crate::png::GxRgba8Texture>,
}

impl PreparedTheme {
    pub fn background_texture(&self) -> Option<&crate::png::GxRgba8Texture> {
        self.background.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StartupError {
    NoSelection,
    NoStorage,
    MissingExecutable,
    NotExecutable,
    TooLarge,
    Storage(StorageError),
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
