//! Failure-safe replacement of files in the running channel's NAND data dir.

#[cfg(any(target_arch = "powerpc", test))]
use alloc::{format, string::String};

/// The operations needed to replace a file without deleting its last good copy.
#[cfg(any(target_arch = "powerpc", test))]
pub(crate) trait ReplaceBackend {
    fn exists(&mut self, path: &str) -> Result<bool, i32>;
    fn write_new(&mut self, path: &str, bytes: &[u8]) -> Result<(), i32>;
    fn rename(&mut self, old_path: &str, new_path: &str) -> i32;
    fn delete(&mut self, path: &str) -> i32;
}

/// Write to a sibling temporary file, then move the old version to a backup
/// before installing the new version. A later call recovers either an
/// interrupted rollback or a completed install with leftover backup data.
#[cfg(any(target_arch = "powerpc", test))]
pub(crate) fn replace_file(
    backend: &mut impl ReplaceBackend,
    path: &str,
    bytes: &[u8],
) -> Result<(), i32> {
    let temporary = sibling_path(path, ".tmp");
    let backup = sibling_path(path, ".bak");

    recover_interrupted_replace(backend, path, &temporary, &backup)?;
    if let Err(error) = backend.write_new(&temporary, bytes) {
        let _ = backend.delete(&temporary);
        return Err(error);
    }

    let original_exists = match backend.exists(path) {
        Ok(exists) => exists,
        Err(error) => {
            let _ = backend.delete(&temporary);
            return Err(error);
        }
    };

    if !original_exists {
        if let Err(error) = rename(backend, &temporary, path) {
            let _ = backend.delete(&temporary);
            return Err(error);
        }
        return Ok(());
    }

    if let Err(error) = rename(backend, path, &backup) {
        let _ = backend.delete(&temporary);
        return Err(error);
    }
    if let Err(error) = rename(backend, &temporary, path) {
        if let Err(restore_error) = rename(backend, &backup, path) {
            return Err(restore_error);
        }
        let _ = backend.delete(&temporary);
        return Err(error);
    }

    let _ = backend.delete(&backup);
    Ok(())
}

/// Restore a backup when a prior save stopped after moving the primary file.
#[cfg(any(target_arch = "powerpc", test))]
pub(crate) fn recover_file_for_load(
    backend: &mut impl ReplaceBackend,
    path: &str,
) -> Result<(), i32> {
    let backup = sibling_path(path, ".bak");
    let temporary = sibling_path(path, ".tmp");
    let path_exists = backend.exists(path)?;
    let backup_exists = backend.exists(&backup)?;
    if !path_exists && backup_exists {
        rename(backend, &backup, path)?;
    } else if path_exists && backup_exists {
        let _ = backend.delete(&backup);
    }
    if backend.exists(&temporary)? {
        let _ = backend.delete(&temporary);
    }
    Ok(())
}

#[cfg(any(target_arch = "powerpc", test))]
fn recover_interrupted_replace(
    backend: &mut impl ReplaceBackend,
    path: &str,
    temporary: &str,
    backup: &str,
) -> Result<(), i32> {
    let path_exists = backend.exists(path)?;
    let backup_exists = backend.exists(backup)?;

    if !path_exists && backup_exists {
        rename(backend, backup, path)?;
    } else if path_exists && backup_exists {
        check(backend.delete(backup))?;
    }

    if backend.exists(temporary)? {
        check(backend.delete(temporary))?;
    }
    Ok(())
}

#[cfg(any(target_arch = "powerpc", test))]
fn rename(backend: &mut impl ReplaceBackend, old_path: &str, new_path: &str) -> Result<(), i32> {
    check(backend.rename(old_path, new_path))
}

#[cfg(any(target_arch = "powerpc", test))]
fn check(result: i32) -> Result<(), i32> {
    if result < 0 {
        Err(result)
    } else {
        Ok(())
    }
}

#[cfg(any(target_arch = "powerpc", test))]
fn sibling_path(path: &str, suffix: &str) -> String {
    format!("{path}{suffix}")
}

#[cfg(target_arch = "powerpc")]
mod wii {
    use super::{recover_file_for_load, replace_file, ReplaceBackend};
    use crate::platform::settings::SettingsIoError;
    use alloc::ffi::CString;
    use ogc_filesystem as isfs;

    const WRITE_CHUNK_SIZE: usize = 16 * 1024;

    #[repr(align(32))]
    struct AlignedChunk([u8; WRITE_CHUNK_SIZE]);

    struct IsfsBackend;

    impl ReplaceBackend for IsfsBackend {
        fn exists(&mut self, path: &str) -> Result<bool, i32> {
            let path = CString::new(path).map_err(|_| isfs::ISFS_EINVAL)?;
            match isfs::open(&path, isfs::ISFS_OPEN_READ) {
                Ok(file) => {
                    let result = file.close();
                    if result < 0 {
                        Err(result)
                    } else {
                        Ok(true)
                    }
                }
                Err(ogc_ipc::IPC_ENOENT) => Ok(false),
                Err(error) => Err(error),
            }
        }

        fn write_new(&mut self, path: &str, bytes: &[u8]) -> Result<(), i32> {
            let path = CString::new(path).map_err(|_| isfs::ISFS_EINVAL)?;
            let result = isfs::create_file(&path, 0, 3, 3, 0);
            if result < 0 {
                return Err(result);
            }
            let file = isfs::open(&path, isfs::ISFS_OPEN_RW)?;
            let mut chunk = AlignedChunk([0; WRITE_CHUNK_SIZE]);
            let mut write_result = Ok(());
            for part in bytes.chunks(WRITE_CHUNK_SIZE) {
                chunk.0[..part.len()].copy_from_slice(part);
                if let Err(error) = file.write_all(&chunk.0[..part.len()]) {
                    write_result = Err(error);
                    break;
                }
            }
            let close_result = file.close();
            write_result?;
            if close_result < 0 {
                return Err(close_result);
            }
            Ok(())
        }

        fn rename(&mut self, old_path: &str, new_path: &str) -> i32 {
            let (Ok(old_path), Ok(new_path)) = (CString::new(old_path), CString::new(new_path))
            else {
                return isfs::ISFS_EINVAL;
            };
            isfs::rename(&old_path, &new_path)
        }

        fn delete(&mut self, path: &str) -> i32 {
            let Ok(path) = CString::new(path) else {
                return isfs::ISFS_EINVAL;
            };
            isfs::delete(&path)
        }
    }

    pub(crate) fn save(path: &CString, bytes: &[u8]) -> Result<(), SettingsIoError> {
        let path = path.to_str().map_err(|_| SettingsIoError::InvalidPath)?;
        replace_file(&mut IsfsBackend, path, bytes).map_err(SettingsIoError::Isfs)
    }

    pub(crate) fn recover(path: &CString) -> Result<(), SettingsIoError> {
        let path = path.to_str().map_err(|_| SettingsIoError::InvalidPath)?;
        recover_file_for_load(&mut IsfsBackend, path).map_err(SettingsIoError::Isfs)
    }
}

#[cfg(target_arch = "powerpc")]
pub(crate) use wii::save as save_wii;

#[cfg(target_arch = "powerpc")]
pub(crate) use wii::recover as recover_wii;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{collections::BTreeMap, vec};

    #[derive(Default)]
    struct MemoryBackend {
        files: BTreeMap<String, Vec<u8>>,
        fail_write: Option<WriteFailure>,
        fail_rename_call: Option<usize>,
        fail_rollback: bool,
        rename_calls: usize,
    }

    #[derive(Clone, Copy)]
    enum WriteFailure {
        Create,
        Open,
        Write,
        Close,
    }

    impl ReplaceBackend for MemoryBackend {
        fn exists(&mut self, path: &str) -> Result<bool, i32> {
            Ok(self.files.contains_key(path))
        }

        fn write_new(&mut self, path: &str, bytes: &[u8]) -> Result<(), i32> {
            match self.fail_write {
                Some(WriteFailure::Create | WriteFailure::Open) => return Err(-7),
                Some(WriteFailure::Write) => {
                    self.files
                        .insert(String::from(path), bytes[..bytes.len() / 2].into());
                    return Err(-7);
                }
                Some(WriteFailure::Close) => {
                    self.files.insert(String::from(path), bytes.into());
                    return Err(-7);
                }
                None => {}
            }
            self.files.insert(String::from(path), bytes.into());
            Ok(())
        }

        fn rename(&mut self, old_path: &str, new_path: &str) -> i32 {
            self.rename_calls += 1;
            if self.fail_rename_call == Some(self.rename_calls)
                || (self.fail_rollback && self.rename_calls == 3)
            {
                return -8;
            }
            let Some(bytes) = self.files.remove(old_path) else {
                return -6;
            };
            if self.files.contains_key(new_path) {
                self.files.insert(String::from(old_path), bytes);
                return -9;
            }
            self.files.insert(String::from(new_path), bytes);
            0
        }

        fn delete(&mut self, path: &str) -> i32 {
            self.files.remove(path);
            0
        }
    }

    #[test]
    fn replace_writes_temp_then_atomically_installs_new_content() {
        let mut backend = MemoryBackend::default();
        backend.files.insert(String::from("settings.xml"), vec![1]);
        assert_eq!(replace_file(&mut backend, "settings.xml", &[2, 3]), Ok(()));
        assert_eq!(backend.files.get("settings.xml"), Some(&vec![2, 3]));
        assert!(!backend.files.contains_key("settings.xml.bak"));
        assert!(!backend.files.contains_key("settings.xml.tmp"));
    }

    #[test]
    fn failed_temp_file_phases_keep_existing_content_unchanged() {
        for failure in [
            WriteFailure::Create,
            WriteFailure::Open,
            WriteFailure::Write,
            WriteFailure::Close,
        ] {
            let mut backend = MemoryBackend::default();
            backend.files.insert(String::from("settings.xml"), vec![1]);
            backend.fail_write = Some(failure);
            assert_eq!(replace_file(&mut backend, "settings.xml", &[2, 3]), Err(-7));
            assert_eq!(backend.files.get("settings.xml"), Some(&vec![1]));
            assert!(!backend.files.contains_key("settings.xml.tmp"));
        }
    }

    #[test]
    fn failed_install_rename_restores_the_previous_file() {
        let mut backend = MemoryBackend::default();
        backend.files.insert(String::from("settings.xml"), vec![1]);
        backend.fail_rename_call = Some(2);
        assert_eq!(replace_file(&mut backend, "settings.xml", &[2]), Err(-8));
        assert_eq!(backend.files.get("settings.xml"), Some(&vec![1]));
        assert!(!backend.files.contains_key("settings.xml.bak"));
        assert!(!backend.files.contains_key("settings.xml.tmp"));
    }

    #[test]
    fn interrupted_backup_move_is_restored_before_the_next_save() {
        let mut backend = MemoryBackend::default();
        backend
            .files
            .insert(String::from("settings.xml.bak"), vec![1]);
        backend
            .files
            .insert(String::from("settings.xml.tmp"), vec![2]);
        assert_eq!(replace_file(&mut backend, "settings.xml", &[3]), Ok(()));
        assert_eq!(backend.files.get("settings.xml"), Some(&vec![3]));
        assert!(!backend.files.contains_key("settings.xml.bak"));
        assert!(!backend.files.contains_key("settings.xml.tmp"));
    }

    #[test]
    fn load_recovery_restores_backup_when_primary_is_missing() {
        let mut backend = MemoryBackend::default();
        backend
            .files
            .insert(String::from("settings.xml.bak"), vec![1]);
        backend
            .files
            .insert(String::from("settings.xml.tmp"), vec![2]);
        assert_eq!(recover_file_for_load(&mut backend, "settings.xml"), Ok(()));
        assert_eq!(backend.files.get("settings.xml"), Some(&vec![1]));
        assert!(!backend.files.contains_key("settings.xml.tmp"));
    }

    #[test]
    fn load_recovery_restores_backup_after_install_and_rollback_rename_failures() {
        let mut backend = MemoryBackend::default();
        backend.files.insert(String::from("settings.xml"), vec![1]);
        backend.fail_rename_call = Some(2);
        backend.fail_rollback = true;
        assert_eq!(replace_file(&mut backend, "settings.xml", &[2]), Err(-8));
        assert!(!backend.files.contains_key("settings.xml"));
        assert_eq!(backend.files.get("settings.xml.bak"), Some(&vec![1]));

        assert_eq!(recover_file_for_load(&mut backend, "settings.xml"), Ok(()));
        assert_eq!(backend.files.get("settings.xml"), Some(&vec![1]));
        assert!(!backend.files.contains_key("settings.xml.bak"));
        assert!(!backend.files.contains_key("settings.xml.tmp"));
    }
}
