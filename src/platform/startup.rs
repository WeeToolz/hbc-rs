//! Boot-time HBC service and application catalog setup.
//!
//! Device access is synchronous in the current FAT adapter. This module scans
//! one preferred volume and retains it for later browser actions. It does not
//! start a background rescan worker.

use alloc::vec::Vec;

use crate::{
    catalog::{scan_apps, AppEntry, AppKind, CatalogStorage},
    loader::{build_launch_arguments, parse_dol, parse_elf32_ppc, LoadPlan, ParseError},
    platform::{
        input, settings,
        storage::{FatStorage, StorageError, Volume},
    },
    runtime::AppRuntime,
};

/// HBC reserves this address range for loading an application executable.
const EXECUTABLE_SIZE_LIMIT: u64 = (0x8133_0000 - 1 - 1024) - 0x8000_3400;
const THEME_SIZE_LIMIT: u64 = 20 * 1024 * 1024;
pub const EXECUTABLE_MEMORY: core::ops::Range<u32> = 0x8000_3400..0x8132_fc00;

/// The result of native Wii startup, ready for the browser loop.
pub struct WiiStartup {
    runtime: AppRuntime,
    volume: Option<Volume>,
    storage: Option<FatStorage>,
}

impl WiiStartup {
    /// Initialize controller services, load NAND settings, and scan the
    /// preferred SD or USB volume once. Missing settings and volumes use the
    /// same empty/default state as HBC.
    pub fn initialize(view_width: u16, view_height: u16) -> Self {
        input::initialize(view_width, view_height);

        let loaded_settings = settings::load().unwrap_or_default();
        let (volume, storage, catalog) = mount_preferred_volume(loaded_settings.device);
        let mut runtime = AppRuntime::new(catalog);
        runtime.load_settings(loaded_settings);

        Self {
            runtime,
            volume,
            storage,
        }
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

    /// Unmount the selected volume before the one-way executable handoff.
    pub fn unmount_storage(&mut self) -> Result<(), StartupError> {
        ogc_fat::shutdown_all().map_err(|error| StartupError::Storage(StorageError::Fat(error)))?;
        self.storage = None;
        self.volume = None;
        Ok(())
    }
}

pub struct LaunchImage {
    pub bytes: Vec<u8>,
    pub plan: LoadPlan,
    pub arguments: Vec<u8>,
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
}

fn mount_preferred_volume(preference: i32) -> (Option<Volume>, Option<FatStorage>, Vec<AppEntry>) {
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
        let catalog = scan_apps(
            &mut storage,
            "apps",
            EXECUTABLE_SIZE_LIMIT,
            THEME_SIZE_LIMIT,
        )
        .unwrap_or_default();
        return (Some(volume), Some(storage), catalog);
    }

    (None, None, Vec::new())
}
