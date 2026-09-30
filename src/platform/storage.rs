//! FAT volume access through the vendored `ogc-fat` Rust API.

#[cfg(target_arch = "powerpc")]
use alloc::{ffi::CString, string::String, vec::Vec};

#[cfg(target_arch = "powerpc")]
use crate::catalog::{CatalogStorage, DirectoryEntry};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Volume {
    Sd,
    Usb,
}

#[cfg(target_arch = "powerpc")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageError {
    Fat(ogc_fat::FatError),
    Archive(crate::archive::ZipError),
    Management(crate::management::ManagementError),
    InvalidPath,
    Allocation,
    WriteZero,
    DeleteDepth,
}

#[cfg(target_arch = "powerpc")]
pub struct FatStorage {
    prefix: &'static str,
}

#[cfg(target_arch = "powerpc")]
impl FatStorage {
    /// Mount a device and create a storage adapter for its root.
    pub fn mount(volume: Volume) -> Result<Self, StorageError> {
        let (prefix, result) = match volume {
            Volume::Sd => ("sd:", ogc_fat::mount_sd()),
            Volume::Usb => ("usb:", ogc_fat::mount_usb()),
        };
        result.map_err(StorageError::Fat)?;
        Ok(Self { prefix })
    }

    pub fn prefix(&self) -> &'static str {
        self.prefix
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
        self.remove_tree(&path, 0)?;
        let full = self.full_path(&path)?;
        ogc_fat::remove_directory(&full).map_err(StorageError::Fat)
    }

    fn remove_tree(&self, path: &str, depth: usize) -> Result<(), StorageError> {
        const MAX_DELETE_DEPTH: usize = 64;
        if depth > MAX_DELETE_DEPTH {
            return Err(StorageError::DeleteDepth);
        }
        let full = self.full_path(path)?;
        let mut directory = ogc_fat::FatDirectory::open(&full).map_err(StorageError::Fat)?;
        let mut children = alloc::vec::Vec::new();
        loop {
            let Some(entry) = directory.next_entry().map_err(StorageError::Fat)? else {
                break;
            };
            let name = alloc::string::String::from_utf8_lossy(&entry.name).into_owned();
            if name == "." || name == ".." {
                continue;
            }
            crate::archive::validate_archive_path(&name).map_err(|_| StorageError::InvalidPath)?;
            children.push((name, entry.is_directory));
        }
        drop(directory);

        for (name, is_directory) in children {
            let child = join_safe_path(path, &name)?;
            if is_directory {
                self.remove_tree(&child, depth + 1)?;
                let full_child = self.full_path(&child)?;
                ogc_fat::remove_directory(&full_child).map_err(StorageError::Fat)?;
            } else {
                let full_child = self.full_path(&child)?;
                ogc_fat::remove_file(&full_child).map_err(StorageError::Fat)?;
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

    fn read_dir(&mut self, path: &str) -> Result<Vec<DirectoryEntry>, Self::Error> {
        let path = self.full_path(path)?;
        let mut directory = ogc_fat::FatDirectory::open(&path).map_err(StorageError::Fat)?;
        let mut entries = Vec::new();
        loop {
            let Some(entry) = directory.next_entry().map_err(StorageError::Fat)? else {
                break;
            };
            let name = String::from_utf8_lossy(&entry.name).into_owned();
            entries.push(DirectoryEntry {
                name,
                is_directory: entry.is_directory,
            });
        }
        Ok(entries)
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
        let mut offset = 0;
        while offset < len {
            let read = file.read(&mut bytes[offset..]).map_err(StorageError::Fat)?;
            if read == 0 {
                return Err(StorageError::Fat(ogc_fat::FatError::Io));
            }
            offset += read;
        }
        Ok(Some(bytes))
    }
}
