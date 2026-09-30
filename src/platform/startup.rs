//! Boot-time HBC service and application catalog setup.
//!
//! Device access is synchronous in the current FAT adapter. This module scans
//! one preferred volume and retains it for later browser actions. It does not
//! start a background rescan worker.

use alloc::vec::Vec;

use crate::{
    catalog::{scan_apps, AppEntry},
    platform::{
        input, settings,
        storage::{FatStorage, Volume},
    },
    runtime::AppRuntime,
};

/// HBC reserves this address range for loading an application executable.
const EXECUTABLE_SIZE_LIMIT: u64 = (0x8133_0000 - 1 - 1024) - 0x8000_3400;
const THEME_SIZE_LIMIT: u64 = 20 * 1024 * 1024;

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
