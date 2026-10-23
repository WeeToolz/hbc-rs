//! FAT volume access through the vendored `ogc-fat` Rust API.

#[cfg(target_arch = "powerpc")]
use alloc::{ffi::CString, string::String, vec::Vec};

#[cfg(target_arch = "powerpc")]
use crate::catalog::{CatalogStorage, DirectoryEntry};

/// Report removal only after storage was available.
#[cfg(any(test, target_arch = "powerpc"))]
#[derive(Default)]
pub(crate) struct StoragePresence {
    last_scan_had_storage: Option<bool>,
}

#[cfg(any(test, target_arch = "powerpc"))]
impl StoragePresence {
    pub(crate) fn report_missing(&mut self, has_storage: bool) -> bool {
        let report = !has_storage && self.last_scan_had_storage == Some(true);
        self.last_scan_had_storage = Some(has_storage);
        report
    }
}

/// An empty successful enumeration proves absence. A probe failure does not.
#[cfg(any(test, target_arch = "powerpc"))]
fn usb_mount_is_absent<E: PartialEq>(
    error: E,
    unavailable: E,
    enumeration_succeeded: bool,
    candidates: u32,
) -> bool {
    error == unavailable && enumeration_succeeded && candidates == 0
}

/// Keep retries active, but show the same failure only once until recovery.
#[cfg(any(test, target_arch = "powerpc"))]
pub(crate) struct StorageFailureNotice<E> {
    last_error: Option<E>,
}

#[cfg(any(test, target_arch = "powerpc"))]
impl<E> Default for StorageFailureNotice<E> {
    fn default() -> Self {
        Self { last_error: None }
    }
}

#[cfg(any(test, target_arch = "powerpc"))]
impl<E: Copy + PartialEq> StorageFailureNotice<E> {
    pub(crate) fn report(&mut self, error: Option<E>) -> bool {
        let show = error.is_some() && error != self.last_error;
        self.last_error = error;
        show
    }
}

/// Treat absent interfaces and removed media as unavailable devices. Preserve
/// other probe failures so Options can report real storage errors.
#[cfg(any(test, target_arch = "powerpc"))]
pub(crate) fn classify_device_presence<E: PartialEq>(
    result: Result<bool, E>,
    device_unavailable: E,
    media_removed: E,
) -> Result<bool, E> {
    match result {
        Err(error) if error == device_unavailable || error == media_removed => Ok(false),
        result => result,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Volume {
    Sd,
    Usb,
    CardA,
    CardB,
}

impl Volume {
    pub fn from_settings_index(index: i32) -> Option<Self> {
        match index {
            0 => Some(Self::Sd),
            1 => Some(Self::Usb),
            2 => Some(Self::CardA),
            3 => Some(Self::CardB),
            _ => None,
        }
    }

    pub fn settings_index(self) -> i32 {
        match self {
            Self::Sd => 0,
            Self::Usb => 1,
            Self::CardA => 2,
            Self::CardB => 3,
        }
    }

    pub fn mount_name(self) -> &'static core::ffi::CStr {
        match self {
            Self::Sd => c"sd",
            Self::Usb => c"usb",
            Self::CardA => c"carda",
            Self::CardB => c"cardb",
        }
    }
}

pub fn device_scan_order(preference: i32) -> [Volume; 4] {
    let volumes = [Volume::Sd, Volume::Usb, Volume::CardA, Volume::CardB];
    let preferred = Volume::from_settings_index(preference).unwrap_or(Volume::Sd);
    let mut order = [preferred; 4];
    let mut next = 1;
    for volume in volumes {
        if volume != preferred {
            order[next] = volume;
            next += 1;
        }
    }
    order
}

#[cfg(test)]
mod device_selection_tests {
    use super::{
        StorageFailureNotice, StoragePresence, Volume, classify_device_presence, device_scan_order,
    };

    #[test]
    fn repeated_storage_failure_is_reported_once_and_recovery_rearms_it() {
        let mut notice = StorageFailureNotice::default();
        assert!(notice.report(Some((Volume::Usb, -4))));
        assert!(!notice.report(Some((Volume::Usb, -4))));
        assert!(notice.report(Some((Volume::Usb, -22))));
        assert!(!notice.report(None));
        assert!(notice.report(Some((Volume::Usb, -4))));
    }

    #[test]
    fn no_storage_notice_is_not_reopened_by_repeated_scans() {
        let mut presence = StoragePresence::default();
        assert!(!presence.report_missing(false));
        assert!(!presence.report_missing(false));
        assert!(!presence.report_missing(false));
        assert!(!presence.report_missing(true));
        assert!(presence.report_missing(false));
        assert!(!presence.report_missing(false));
    }

    #[test]
    fn empty_usb_scan_is_absence_only_when_the_probe_succeeded() {
        use super::usb_mount_is_absent;
        assert!(usb_mount_is_absent(-1, -1, true, 0));
        assert!(!usb_mount_is_absent(-1, -1, false, 0));
        assert!(!usb_mount_is_absent(-1, -1, true, 1));
        assert!(!usb_mount_is_absent(-5, -1, true, 0));
    }

    #[test]
    fn absent_or_removed_device_is_unavailable_without_hiding_other_errors() {
        #[derive(Debug, Eq, PartialEq)]
        enum ProbeError {
            DeviceUnavailable,
            MediaRemoved,
            Io,
        }

        use ProbeError::{DeviceUnavailable, Io, MediaRemoved};
        let classify = |result| classify_device_presence(result, DeviceUnavailable, MediaRemoved);

        assert_eq!(classify(Ok(true)), Ok(true));
        assert_eq!(classify(Ok(false)), Ok(false));
        assert_eq!(classify(Err(DeviceUnavailable)), Ok(false));
        assert_eq!(classify(Err(MediaRemoved)), Ok(false));
        assert_eq!(classify(Err(Io)), Err(Io));
    }

    #[test]
    fn settings_indices_cover_all_four_storage_devices() {
        let volumes = [Volume::Sd, Volume::Usb, Volume::CardA, Volume::CardB];
        for (index, volume) in volumes.into_iter().enumerate() {
            assert_eq!(Volume::from_settings_index(index as i32), Some(volume));
            assert_eq!(volume.settings_index(), index as i32);
        }
        assert_eq!(Volume::from_settings_index(-1), None);
        assert_eq!(Volume::from_settings_index(4), None);
    }

    #[test]
    fn preferred_storage_is_scanned_before_the_other_devices() {
        assert_eq!(
            device_scan_order(2),
            [Volume::CardA, Volume::Sd, Volume::Usb, Volume::CardB]
        );
        assert_eq!(
            device_scan_order(3),
            [Volume::CardB, Volume::Sd, Volume::Usb, Volume::CardA]
        );
    }
}

#[cfg(target_arch = "powerpc")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageError {
    Fat(ogc_fat::FatError),
    UsbMount {
        error: ogc_fat::FatError,
        probe: ogc_usbstorage_sys::UsbProbeStatus,
    },
    Archive(crate::archive::ZipError),
    Management(crate::management::ManagementError),
    InvalidPath,
    Allocation,
    WriteZero,
    ScreenshotLimit,
    ScreenshotCleanup {
        operation: ogc_fat::FatError,
        cleanup: ogc_fat::FatError,
    },
    DeleteDepth,
    DeleteEntryLimit,
    DirectoryNotOpen,
    DirectoryEntryLimit,
}

#[cfg(target_arch = "powerpc")]
const MAX_RECOVERY_DIRECTORY_ENTRIES: usize = 4096;

#[cfg(target_arch = "powerpc")]
pub struct FatStorage {
    prefix: &'static str,
    catalog_directory: Option<ogc_fat::FatDirectory>,
}

#[cfg(target_arch = "powerpc")]
impl FatStorage {
    /// Mount a device and create a storage adapter for its root.
    pub fn mount(volume: Volume) -> Result<Self, StorageError> {
        let (prefix, result) = match volume {
            Volume::Sd => ("sd:", ogc_fat::mount_sd()),
            Volume::Usb => ("usb:", ogc_fat::mount_usb()),
            Volume::CardA => ("carda:", ogc_fat::mount_card_a()),
            Volume::CardB => ("cardb:", ogc_fat::mount_card_b()),
        };
        result.map_err(|error| {
            if volume == Volume::Usb {
                let probe = ogc_usbstorage_sys::probe_status();
                if usb_mount_is_absent(
                    error,
                    ogc_fat::FatError::DeviceUnavailable,
                    probe.stage == ogc_usbstorage_sys::ProbeStage::Enumerate && probe.result == 0,
                    probe.candidates,
                ) {
                    StorageError::Fat(error)
                } else {
                    StorageError::UsbMount { error, probe }
                }
            } else {
                StorageError::Fat(error)
            }
        })?;
        Ok(Self {
            prefix,
            catalog_directory: None,
        })
    }

    pub fn prefix(&self) -> &'static str {
        self.prefix
    }

    /// Check whether a validated path is an existing directory on this volume.
    pub fn path_is_directory(&self, path: &str) -> Result<bool, StorageError> {
        let full = self.full_path(path)?;
        match ogc_fat::stat(&full) {
            Ok(metadata) => Ok(metadata.is_directory),
            Err(ogc_fat::FatError::NotFound) => Ok(false),
            Err(error) => Err(StorageError::Fat(error)),
        }
    }

    /// Read the bounded app root while recovery checks hidden install entries.
    pub fn read_dir(&mut self, path: &str) -> Result<Vec<DirectoryEntry>, StorageError> {
        let path = self.full_path(path)?;
        let mut directory = ogc_fat::FatDirectory::open(&path).map_err(StorageError::Fat)?;
        let mut entries = Vec::new();
        while let Some(entry) = directory.next_entry().map_err(StorageError::Fat)? {
            if entries.len() == MAX_RECOVERY_DIRECTORY_ENTRIES {
                return Err(StorageError::DirectoryEntryLimit);
            }
            entries
                .try_reserve(1)
                .map_err(|_| StorageError::Allocation)?;
            entries.push(DirectoryEntry {
                name: String::from_utf8_lossy(&entry.name).into_owned(),
                is_directory: entry.is_directory,
            });
        }
        Ok(entries)
    }

    pub fn path_exists(&self, path: &str) -> Result<bool, StorageError> {
        let full = self.full_path(path)?;
        match ogc_fat::stat(&full) {
            Ok(_) => Ok(true),
            Err(ogc_fat::FatError::NotFound) => Ok(false),
            Err(error) => Err(StorageError::Fat(error)),
        }
    }

    pub fn file_size(&self, path: &str) -> Result<Option<u64>, StorageError> {
        let full = self.full_path(path)?;
        match ogc_fat::stat(&full) {
            Ok(metadata) if metadata.is_directory => Err(StorageError::InvalidPath),
            Ok(metadata) => Ok(Some(u64::from(metadata.size))),
            Err(ogc_fat::FatError::NotFound) => Ok(None),
            Err(error) => Err(StorageError::Fat(error)),
        }
    }

    /// Read an executable directly into its reserved launch buffer.
    pub fn read_file_into(
        &mut self,
        path: &str,
        bytes: &mut [u8],
    ) -> Result<Option<usize>, StorageError> {
        let path = self.full_path(path)?;
        let mut file = match ogc_fat::FatFile::open(&path) {
            Ok(file) => file,
            Err(ogc_fat::FatError::NotFound) => return Ok(None),
            Err(error) => return Err(StorageError::Fat(error)),
        };
        let len = file.len() as usize;
        if len > bytes.len() {
            return Ok(None);
        }
        read_exact_file(&mut file, &mut bytes[..len]).map_err(StorageError::Fat)?;
        Ok(Some(len))
    }

    /// Save a screenshot to the active volume without replacing an existing capture.
    pub fn save_screenshot_png(&self, bytes: &[u8]) -> Result<String, StorageError> {
        match crate::screenshot::save_png_with(
            bytes,
            |path| self.path_exists(path),
            |path, bytes| self.write_screenshot_file(path, bytes),
        ) {
            Ok(path) => Ok(path),
            Err(crate::screenshot::SaveError::Storage(error)) => Err(error),
            Err(crate::screenshot::SaveError::IndexExhausted) => Err(StorageError::ScreenshotLimit),
            Err(crate::screenshot::SaveError::AllocationFailed) => Err(StorageError::Allocation),
        }
    }

    fn write_screenshot_file(&self, path: &str, bytes: &[u8]) -> Result<bool, StorageError> {
        let full = self.full_path(path)?;
        let mut file = match ogc_fat::FatFile::create_new(&full) {
            Ok(file) => file,
            Err(ogc_fat::FatError::AlreadyExists) => return Ok(false),
            Err(error) => return Err(StorageError::Fat(error)),
        };
        let operation = (|| {
            let mut offset = 0;
            while offset < bytes.len() {
                let written = file.write(&bytes[offset..])?;
                if written == 0 {
                    return Err(ogc_fat::FatError::Io);
                }
                offset += written;
            }
            file.flush()
        })();
        drop(file);
        if let Err(operation) = operation {
            if let Err(cleanup) = ogc_fat::remove_file(&full) {
                return Err(StorageError::ScreenshotCleanup { operation, cleanup });
            }
            return Err(StorageError::Fat(operation));
        }
        Ok(true)
    }

    pub fn rename_path(&self, old_path: &str, new_path: &str) -> Result<(), StorageError> {
        let old_path = self.full_path(old_path)?;
        let new_path = self.full_path(new_path)?;
        ogc_fat::rename(&old_path, &new_path).map_err(StorageError::Fat)
    }

    pub fn repair_interrupted_directory_rename(
        &self,
        primary_path: &str,
        alias_path: &str,
    ) -> Result<bool, StorageError> {
        let primary_path = self.full_path(primary_path)?;
        let alias_path = self.full_path(alias_path)?;
        ogc_fat::repair_interrupted_directory_rename(&primary_path, &alias_path)
            .map_err(StorageError::Fat)
    }

    /// Create a directory, accepting an existing directory at the same path.
    pub fn ensure_directory(&self, path: &str) -> Result<(), StorageError> {
        let full = self.full_path(path)?;
        match ogc_fat::create_directory(&full) {
            Ok(()) => Ok(()),
            Err(ogc_fat::FatError::AlreadyExists)
                if ogc_fat::stat(&full)
                    .map(|stat| stat.is_directory)
                    .unwrap_or(false) =>
            {
                Ok(())
            }
            Err(ogc_fat::FatError::AlreadyExists) => {
                Err(StorageError::Fat(ogc_fat::FatError::PathError))
            }
            Err(error) => Err(StorageError::Fat(error)),
        }
    }

    pub fn unmount(self) -> Result<(), StorageError> {
        let name = match self.prefix {
            "sd:" => c"sd",
            "usb:" => c"usb",
            "carda:" => c"carda",
            "cardb:" => c"cardb",
            _ => return Err(StorageError::InvalidPath),
        };
        ogc_fat::unmount(name).map_err(StorageError::Fat)
    }

    /// Extract a fully validated archive plan below a mounted-volume path.
    ///
    /// The complete plan must be built before this method is called. Writes
    /// follow that plan in order, so parent directories exist before files.
    pub fn extract_archive(
        &self,
        archive_bytes: &[u8],
        archive: &crate::archive::ZipArchive,
        plan: &crate::management::ExtractionPlan,
        destination: &str,
    ) -> Result<(), StorageError> {
        self.extract_archive_with_progress(archive_bytes, archive, plan, destination, |_, _| {})
    }

    /// Extract a validated plan and report cumulative completed output bytes.
    pub fn extract_archive_with_progress<F>(
        &self,
        archive_bytes: &[u8],
        archive: &crate::archive::ZipArchive,
        plan: &crate::management::ExtractionPlan,
        destination: &str,
        mut progress: F,
    ) -> Result<(), StorageError>
    where
        F: FnMut(u64, u64),
    {
        crate::archive::validate_archive_path(destination)
            .map_err(|_| StorageError::InvalidPath)?;
        let mut completed_bytes = 0u64;
        let total_bytes = plan.total_bytes();
        progress(completed_bytes, total_bytes);
        for operation in plan.operations() {
            match operation {
                crate::management::ExtractionOperation::CreateDirectory { path } => {
                    let relative = join_safe_path(destination, path)?;
                    self.ensure_directory(&relative)?;
                }
                crate::management::ExtractionOperation::WriteFile {
                    path,
                    archive_name,
                    size,
                } => {
                    let output_limit =
                        usize::try_from(*size).map_err(|_| StorageError::Allocation)?;
                    let bytes = archive
                        .extract(archive_bytes, archive_name, output_limit)
                        .map_err(StorageError::Archive)?;
                    if bytes.len() as u64 != *size {
                        return Err(StorageError::Archive(
                            crate::archive::ZipError::SizeMismatch,
                        ));
                    }
                    let relative = join_safe_path(destination, path)?;
                    let full = self.full_path(&relative)?;
                    let mut file =
                        ogc_fat::FatFile::create_truncate(&full).map_err(StorageError::Fat)?;
                    let mut offset = 0;
                    while offset < bytes.len() {
                        let written = file.write(&bytes[offset..]).map_err(StorageError::Fat)?;
                        if written == 0 {
                            return Err(StorageError::WriteZero);
                        }
                        offset += written;
                        completed_bytes = completed_bytes.saturating_add(written as u64);
                        progress(completed_bytes, total_bytes);
                    }
                    file.flush().map_err(StorageError::Fat)?;
                }
            }
        }
        Ok(())
    }

    /// Remove an app folder recursively after checking its catalog name.
    pub fn remove_app_folder(&self, directory: &str) -> Result<(), StorageError> {
        let plan = crate::management::plan_app_deletion(directory)
            .map_err(|_| StorageError::InvalidPath)?;
        let path = alloc::format!("apps/{}", plan.directory);
        self.remove_directory_tree(&path)
    }

    pub fn remove_directory_tree(&self, path: &str) -> Result<(), StorageError> {
        crate::archive::validate_archive_path(path).map_err(|_| StorageError::InvalidPath)?;
        let mut remaining_entries = MAX_RECOVERY_DIRECTORY_ENTRIES * 4;
        let mut deletion_plan = Vec::new();
        self.collect_tree(path, 0, &mut remaining_entries, &mut deletion_plan)?;
        for (child, is_directory) in deletion_plan {
            let full_child = self.full_path(&child)?;
            if is_directory {
                ogc_fat::remove_directory(&full_child).map_err(StorageError::Fat)?;
            } else {
                ogc_fat::remove_file(&full_child).map_err(StorageError::Fat)?;
            }
        }
        let full = self.full_path(path)?;
        ogc_fat::remove_directory(&full).map_err(StorageError::Fat)
    }

    fn collect_tree(
        &self,
        path: &str,
        depth: usize,
        remaining_entries: &mut usize,
        deletion_plan: &mut Vec<(String, bool)>,
    ) -> Result<(), StorageError> {
        const MAX_DELETE_DEPTH: usize = 64;
        if depth > MAX_DELETE_DEPTH {
            return Err(StorageError::DeleteDepth);
        }
        let full = self.full_path(path)?;
        let mut directory = ogc_fat::FatDirectory::open(&full).map_err(StorageError::Fat)?;
        let mut children = alloc::vec::Vec::new();
        while let Some(entry) = directory.next_entry().map_err(StorageError::Fat)? {
            let name = alloc::string::String::from_utf8_lossy(&entry.name).into_owned();
            if name == "." || name == ".." {
                continue;
            }
            crate::archive::validate_archive_path(&name).map_err(|_| StorageError::InvalidPath)?;
            if *remaining_entries == 0 {
                return Err(StorageError::DeleteEntryLimit);
            }
            *remaining_entries -= 1;
            children
                .try_reserve(1)
                .map_err(|_| StorageError::Allocation)?;
            children.push((name, entry.is_directory));
        }
        drop(directory);

        for (name, is_directory) in children {
            let child = join_safe_path(path, &name)?;
            if is_directory {
                self.collect_tree(&child, depth + 1, remaining_entries, deletion_plan)?;
                deletion_plan
                    .try_reserve(1)
                    .map_err(|_| StorageError::Allocation)?;
                deletion_plan.push((child, true));
            } else {
                deletion_plan
                    .try_reserve(1)
                    .map_err(|_| StorageError::Allocation)?;
                deletion_plan.push((child, false));
            }
        }
        Ok(())
    }

    fn full_path(&self, path: &str) -> Result<CString, StorageError> {
        crate::archive::validate_archive_path(path).map_err(|_| StorageError::InvalidPath)?;
        CString::new(alloc::format!("{}{path}", self.prefix)).map_err(|_| StorageError::InvalidPath)
    }
}

#[cfg(target_arch = "powerpc")]
fn join_safe_path(parent: &str, child: &str) -> Result<String, StorageError> {
    crate::archive::validate_archive_path(parent).map_err(|_| StorageError::InvalidPath)?;
    crate::archive::validate_archive_path(child).map_err(|_| StorageError::InvalidPath)?;
    let joined = alloc::format!("{}/{child}", parent.trim_end_matches('/'));
    crate::archive::validate_archive_path(&joined).map_err(|_| StorageError::InvalidPath)?;
    Ok(joined)
}

#[cfg(target_arch = "powerpc")]
impl CatalogStorage for FatStorage {
    type Error = StorageError;

    fn open_dir(&mut self, path: &str) -> Result<(), Self::Error> {
        let path = self.full_path(path)?;
        let directory = ogc_fat::FatDirectory::open(&path).map_err(StorageError::Fat)?;
        self.catalog_directory = Some(directory);
        Ok(())
    }

    fn next_dir_entry(&mut self) -> Result<Option<DirectoryEntry>, Self::Error> {
        let directory = self
            .catalog_directory
            .as_mut()
            .ok_or(StorageError::DirectoryNotOpen)?;
        directory
            .next_entry()
            .map(|entry| {
                entry.map(|entry| DirectoryEntry {
                    name: String::from_utf8_lossy(&entry.name).into_owned(),
                    is_directory: entry.is_directory,
                })
            })
            .map_err(StorageError::Fat)
    }

    fn close_dir(&mut self) {
        self.catalog_directory = None;
    }

    fn file_size(&mut self, path: &str) -> Result<Option<u64>, Self::Error> {
        let path = self.full_path(path)?;
        match ogc_fat::FatFile::open(&path) {
            Ok(file) => Ok(Some(file.len() as u64)),
            Err(ogc_fat::FatError::NotFound) => Ok(None),
            Err(error) => Err(StorageError::Fat(error)),
        }
    }

    fn read_file(&mut self, path: &str, max_bytes: usize) -> Result<Option<Vec<u8>>, Self::Error> {
        let path = self.full_path(path)?;
        let mut file = match ogc_fat::FatFile::open(&path) {
            Ok(file) => file,
            Err(ogc_fat::FatError::NotFound) => return Ok(None),
            Err(error) => return Err(StorageError::Fat(error)),
        };
        let len = file.len() as usize;
        if len > max_bytes {
            return Ok(None);
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(len)
            .map_err(|_| StorageError::Allocation)?;
        bytes.resize(len, 0);
        read_exact_file(&mut file, &mut bytes).map_err(StorageError::Fat)?;
        Ok(Some(bytes))
    }
}

#[cfg(target_arch = "powerpc")]
fn read_exact_file(file: &mut ogc_fat::FatFile, bytes: &mut [u8]) -> Result<(), ogc_fat::FatError> {
    let mut offset = 0;
    while offset < bytes.len() {
        let read = file.read(&mut bytes[offset..])?;
        if read == 0 {
            return Err(ogc_fat::FatError::Io);
        }
        offset += read;
    }
    Ok(())
}
