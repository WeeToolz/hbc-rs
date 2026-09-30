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
    InvalidPath,
    Allocation,
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

    pub fn unmount(self) -> Result<(), StorageError> {
        let name = match self.prefix {
            "sd:" => c"sd",
            "usb:" => c"usb",
            _ => return Err(StorageError::InvalidPath),
        };
        ogc_fat::unmount(name).map_err(StorageError::Fat)
    }

    fn full_path(&self, path: &str) -> Result<CString, StorageError> {
        crate::archive::validate_archive_path(path).map_err(|_| StorageError::InvalidPath)?;
        CString::new(alloc::format!("{}{path}", self.prefix)).map_err(|_| StorageError::InvalidPath)
    }
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
