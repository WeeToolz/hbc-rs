//! Failure-safe replacement of files in the running channel's NAND data dir.

#[cfg(any(target_arch = "powerpc", test))]
use alloc::{format, string::String, vec::Vec};

/// The operations needed to replace a file without deleting its last good copy.
#[cfg(any(target_arch = "powerpc", test))]
pub(crate) trait ReplaceBackend {
    fn ensure_directory(&mut self, path: &str) -> Result<(), i32>;
    fn exists(&mut self, path: &str) -> Result<bool, i32>;
    fn read_bounded(&mut self, path: &str, max_size: usize) -> Result<Option<Vec<u8>>, i32>;
    fn write_new(&mut self, path: &str, bytes: &[u8]) -> Result<(), i32>;
    fn write_new_with_attributes(
        &mut self,
        path: &str,
        bytes: &[u8],
        _attributes: Option<FileAttributes>,
    ) -> Result<(), i32> {
        self.write_new(path, bytes)
    }
    fn rename(&mut self, old_path: &str, new_path: &str) -> i32;
    fn delete(&mut self, path: &str) -> i32;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(any(target_arch = "powerpc", test))]
pub(crate) struct FileAttributes {
    pub owner_id: u32,
    pub group_id: u16,
    pub attributes: u8,
    pub owner_perm: u8,
    pub group_perm: u8,
    pub other_perm: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(any(target_arch = "powerpc", test))]
pub(crate) enum ReplaceError {
    Backend(i32),
    Cleanup {
        operation: Option<i32>,
        cleanup: i32,
    },
    CommittedCleanup(i32),
    BackupNeedsValidation,
    TemporaryMismatch,
    StaleSnapshot,
    InvalidPath,
}

#[cfg(any(target_arch = "powerpc", test))]
pub(crate) fn is_missing_isfs_error(error: i32, missing_error: i32) -> bool {
    error == missing_error
}

#[cfg(any(target_arch = "powerpc", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DirectoryEnsureError {
    Read(i32),
    Create(i32),
    Verify(i32),
}

/// Check for an existing directory before asking IOS to create it. Some IOS
/// permissions report access denied for CreateDir on an existing system
/// directory, so CreateDir cannot serve as an existence check.
#[cfg(any(target_arch = "powerpc", test))]
fn ensure_directory_with(
    mut check: impl FnMut() -> Result<bool, i32>,
    mut create: impl FnMut() -> i32,
    mut verify: impl FnMut() -> i32,
    exists_error: i32,
) -> Result<(), DirectoryEnsureError> {
    if check().map_err(DirectoryEnsureError::Read)? {
        return Ok(());
    }

    let result = create();
    if result < 0 && result != exists_error {
        return Err(DirectoryEnsureError::Create(result));
    }

    let result = verify();
    if result < 0 {
        return Err(DirectoryEnsureError::Verify(result));
    }
    Ok(())
}

#[cfg(target_arch = "powerpc")]
const ISFS_DIRECTORY_ENTRY_SIZE: usize = 13;

#[cfg(any(target_arch = "powerpc", test))]
fn directory_contains_entry(entries: &[u8], count: usize, name: &str) -> Option<bool> {
    if name.is_empty() || name.len() > 12 {
        return None;
    }
    let mut remaining = entries;
    let mut found = false;
    for _ in 0..count {
        let name_end = remaining.iter().position(|byte| *byte == 0)?;
        if name_end == 0 || name_end > 12 {
            return None;
        }
        found |= &remaining[..name_end] == name.as_bytes();
        remaining = &remaining[name_end + 1..];
    }
    Some(found)
}

#[cfg(any(target_arch = "powerpc", test))]
impl From<i32> for ReplaceError {
    fn from(error: i32) -> Self {
        Self::Backend(error)
    }
}

/// Write to a temporary directory, then move the old version to a backup directory
/// before installing the new version. A later call recovers either an
/// interrupted rollback or a completed install with leftover backup data.
#[cfg(any(target_arch = "powerpc", test))]
pub(crate) fn replace_file(
    backend: &mut impl ReplaceBackend,
    path: &str,
    bytes: &[u8],
) -> Result<(), ReplaceError> {
    replace_file_with_attributes(backend, path, bytes, None, None)
}

#[cfg(any(target_arch = "powerpc", test))]
pub(crate) fn replace_file_with_attributes(
    backend: &mut impl ReplaceBackend,
    path: &str,
    bytes: &[u8],
    attributes: Option<FileAttributes>,
    expected: Option<&[u8]>,
) -> Result<(), ReplaceError> {
    let temporary = staging_path(path, "_tmp")?;
    let backup = staging_path(path, "_bak")?;

    recover_interrupted_replace(backend, &temporary, &backup)?;
    for staged in [&temporary, &backup] {
        let (directory, _) = staged.rsplit_once('/').ok_or(ReplaceError::InvalidPath)?;
        backend.ensure_directory(directory)?;
    }
    if let Err(error) = backend.write_new_with_attributes(&temporary, bytes, attributes) {
        return Err(cleanup_temporary(backend, &temporary, Some(error)));
    }
    let staged = match backend.read_bounded(&temporary, bytes.len()) {
        Ok(Some(staged)) => staged,
        Ok(None) => {
            return Err(cleanup_temporary(backend, &temporary, None));
        }
        Err(error) => return Err(cleanup_temporary(backend, &temporary, Some(error))),
    };
    if staged != bytes {
        return Err(cleanup_temporary(backend, &temporary, None));
    }

    let original_exists = match backend.exists(path) {
        Ok(exists) => exists,
        Err(error) => {
            return Err(cleanup_temporary(backend, &temporary, Some(error)));
        }
    };

    if let Some(expected) = expected {
        match backend.read_bounded(path, expected.len()) {
            Ok(Some(current)) if current == expected => {}
            Ok(_) => {
                return match cleanup_temporary(backend, &temporary, None) {
                    ReplaceError::TemporaryMismatch => Err(ReplaceError::StaleSnapshot),
                    error => Err(error),
                };
            }
            Err(error) => {
                return Err(cleanup_temporary(backend, &temporary, Some(error)));
            }
        }
    }

    if !original_exists {
        if let Err(error) = rename(backend, &temporary, path) {
            return Err(cleanup_temporary(
                backend,
                &temporary,
                Some(backend_error(error)),
            ));
        }
        return Ok(());
    }

    if let Err(error) = rename(backend, path, &backup) {
        return Err(cleanup_temporary(
            backend,
            &temporary,
            Some(backend_error(error)),
        ));
    }
    if let Err(error) = rename(backend, &temporary, path) {
        rename(backend, &backup, path)?;
        return Err(cleanup_temporary(
            backend,
            &temporary,
            Some(backend_error(error)),
        ));
    }

    if let Err(error) = check(backend.delete(&backup)) {
        // The new primary is already installed and verified. Keep it in place;
        // report that only removal of the old recovery copy failed.
        return Err(ReplaceError::CommittedCleanup(backend_error(error)));
    }
    Ok(())
}

/// Create a recovery copy in its parent directory without requiring temporary
/// subdirectories. IOS permissions can allow files in a title data directory
/// while denying new child directories.
#[cfg(any(target_arch = "powerpc", test))]
pub(crate) fn write_new_verified_file(
    backend: &mut impl ReplaceBackend,
    path: &str,
    bytes: &[u8],
) -> Result<(), ReplaceError> {
    if backend.exists(path)? {
        return Err(ReplaceError::BackupNeedsValidation);
    }
    if let Err(error) = backend.write_new(path, bytes) {
        return Err(cleanup_temporary(backend, path, Some(error)));
    }
    match backend.read_bounded(path, bytes.len()) {
        Ok(Some(staged)) if staged == bytes => Ok(()),
        Ok(_) => Err(cleanup_temporary(backend, path, None)),
        Err(error) => Err(cleanup_temporary(backend, path, Some(error))),
    }
}

/// Save a new file directly in its existing parent directory. Use the staged
/// replacement path only when the primary already exists. Some title data
/// directories allow file creation but deny creation of `_tmp` and `_bak` dirs.
#[cfg(any(target_arch = "powerpc", test))]
fn save_file(
    backend: &mut impl ReplaceBackend,
    path: &str,
    bytes: &[u8],
) -> Result<(), ReplaceError> {
    if backend.exists(path)? {
        replace_file(backend, path, bytes)
    } else {
        write_new_verified_file(backend, path, bytes)
    }
}

#[cfg(any(target_arch = "powerpc", test))]
fn cleanup_temporary(
    backend: &mut impl ReplaceBackend,
    temporary: &str,
    operation: Option<i32>,
) -> ReplaceError {
    match backend.exists(temporary) {
        Ok(false) => {
            return operation.map_or(ReplaceError::TemporaryMismatch, ReplaceError::Backend);
        }
        Ok(true) => {}
        Err(cleanup) => return ReplaceError::Cleanup { operation, cleanup },
    }
    let cleanup = backend.delete(temporary);
    if cleanup < 0 {
        ReplaceError::Cleanup { operation, cleanup }
    } else if let Some(operation) = operation {
        ReplaceError::Backend(operation)
    } else {
        ReplaceError::TemporaryMismatch
    }
}

#[cfg(any(target_arch = "powerpc", test))]
fn backend_error(error: ReplaceError) -> i32 {
    match error {
        ReplaceError::Backend(error) => error,
        _ => unreachable!("rename only returns backend errors"),
    }
}

/// Restore a backup when a prior save stopped after moving the primary file.
#[cfg(any(target_arch = "powerpc", test))]
pub(crate) fn recover_file_for_load(
    backend: &mut impl ReplaceBackend,
    path: &str,
    max_size: usize,
    mut validate: impl FnMut(&[u8]) -> bool,
) -> Result<(), ReplaceError> {
    let backup = staging_path(path, "_bak")?;
    let temporary = staging_path(path, "_tmp")?;
    let path_exists = backend.exists(path)?;
    let backup_exists = backend.exists(&backup)?;
    if backup_exists {
        let primary_valid = if path_exists {
            backend
                .read_bounded(path, max_size)?
                .is_some_and(|bytes| validate(&bytes))
        } else {
            false
        };
        let backup_valid = backend
            .read_bounded(&backup, max_size)?
            .is_some_and(|bytes| validate(&bytes));

        if primary_valid {
            check(backend.delete(&backup))?;
        } else if backup_valid {
            if path_exists {
                check(backend.delete(path))?;
            }
            rename(backend, &backup, path)?;
        } else if !path_exists {
            return Err(ReplaceError::BackupNeedsValidation);
        }
    }
    if backend.exists(&temporary)? {
        check(backend.delete(&temporary))?;
    }
    Ok(())
}

#[cfg(any(target_arch = "powerpc", test))]
fn recover_interrupted_replace(
    backend: &mut impl ReplaceBackend,
    temporary: &str,
    backup: &str,
) -> Result<(), ReplaceError> {
    let backup_exists = backend.exists(backup)?;

    if backup_exists {
        // Keep the backup until the caller validates the primary on load.
        // A missing primary is also ambiguous until load has checked the copy.
        return Err(ReplaceError::BackupNeedsValidation);
    }

    if backend.exists(temporary)? {
        check(backend.delete(temporary))?;
    }
    Ok(())
}

#[cfg(any(target_arch = "powerpc", test))]
fn rename(
    backend: &mut impl ReplaceBackend,
    old_path: &str,
    new_path: &str,
) -> Result<(), ReplaceError> {
    check(backend.rename(old_path, new_path))
}

#[cfg(any(target_arch = "powerpc", test))]
fn check(result: i32) -> Result<(), ReplaceError> {
    if result < 0 {
        Err(ReplaceError::Backend(result))
    } else {
        Ok(())
    }
}

#[cfg(any(target_arch = "powerpc", test))]
fn staging_path(path: &str, directory_name: &str) -> Result<String, ReplaceError> {
    let (directory, filename) = path
        .rsplit_once('/')
        .map_or(("", path), |(directory, filename)| {
            (&path[..directory.len() + 1], filename)
        });
    if filename.is_empty()
        || filename.len() > 12
        || path.as_bytes().contains(&0)
        || matches!(filename, "." | "..")
        || !matches!(directory_name, "_tmp" | "_bak")
    {
        return Err(ReplaceError::InvalidPath);
    }
    // IOS moves files between directories but rejects a change to the basename.
    // Both staging directories persist across a reboot in the title's data dir.
    let staged = format!("{directory}{directory_name}/{filename}");
    if staged.len() >= 64 {
        return Err(ReplaceError::InvalidPath);
    }
    Ok(staged)
}

#[cfg(target_arch = "powerpc")]
mod wii {
    use super::{
        directory_contains_entry, ensure_directory_with, is_missing_isfs_error,
        recover_file_for_load, save_file, write_new_verified_file, DirectoryEnsureError,
        FileAttributes, ReplaceBackend,
    };
    use crate::platform::settings::SettingsIoError;
    use alloc::ffi::CString;
    use alloc::vec::Vec;
    use ogc_filesystem as isfs;

    const ISFS_EEXIST: i32 = -105;
    const WRITE_CHUNK_SIZE: usize = 16 * 1024;
    const MAX_DIRECTORY_ENTRIES: usize = 64;

    #[repr(align(32))]
    struct AlignedChunk([u8; WRITE_CHUNK_SIZE]);

    #[repr(align(32))]
    struct AlignedDirectoryEntries([u8; MAX_DIRECTORY_ENTRIES * super::ISFS_DIRECTORY_ENTRY_SIZE]);

    #[repr(align(32))]
    struct AlignedFstats(isfs::Fstats);

    struct IsfsBackend;

    fn report_isfs_failure(operation: &str, path: &core::ffi::CStr, error: i32) {
        let message = alloc::format!(
            "ISFS {operation} failed path={:?} rc={error}",
            path.to_bytes()
        );
        #[cfg(feature = "startup-diagnostics")]
        super::super::diagnostics::event(&message);
        ogc_system::report(format_args!("HBC: {message}\n"));
    }

    /// Read exactly `output.len()` bytes without ever passing an unaligned
    /// destination pointer to ISFS. IOS requires the IPC data buffer itself to
    /// be 32-byte aligned. Advancing a caller-owned slice after a short read can
    /// therefore make a later read invalid even when the original allocation
    /// was aligned.
    pub(crate) fn read_exact_aligned(file: &isfs::IsfsFile, output: &mut [u8]) -> Result<(), i32> {
        let mut chunk = AlignedChunk([0; WRITE_CHUNK_SIZE]);
        let mut offset = 0usize;
        while offset < output.len() {
            let requested = (output.len() - offset).min(WRITE_CHUNK_SIZE);
            match file.read(&mut chunk.0[..requested]) {
                Ok(0) => return Err(isfs::ISFS_EINVAL),
                Ok(count) if count <= requested => {
                    output[offset..offset + count].copy_from_slice(&chunk.0[..count]);
                    offset += count;
                }
                Ok(_) => return Err(isfs::ISFS_EINVAL),
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    impl ReplaceBackend for IsfsBackend {
        fn ensure_directory(&mut self, path: &str) -> Result<(), i32> {
            let path = CString::new(path).map_err(|_| isfs::ISFS_EINVAL)?;
            let result = ensure_directory_with(
                || {
                    let mut count = 0;
                    match isfs::read_dir(&path, None, &mut count) {
                        0 => Ok(true),
                        error if error == isfs::ISFS_ENOENT => Ok(false),
                        error => Err(error),
                    }
                },
                || isfs::create_dir(&path, 0, 3, 3, 0),
                || {
                    let mut count = 0;
                    isfs::read_dir(&path, None, &mut count)
                },
                ISFS_EEXIST,
            );
            match result {
                Ok(()) => Ok(()),
                Err(DirectoryEnsureError::Read(error)) => {
                    report_isfs_failure("ReadDir", &path, error);
                    Err(error)
                }
                Err(DirectoryEnsureError::Create(error)) => {
                    report_isfs_failure("CreateDir", &path, error);
                    Err(error)
                }
                Err(DirectoryEnsureError::Verify(error)) => {
                    report_isfs_failure("ReadDir", &path, error);
                    Err(error)
                }
            }
        }
        fn exists(&mut self, path: &str) -> Result<bool, i32> {
            let (directory, name) = path.rsplit_once('/').ok_or(isfs::ISFS_EINVAL)?;
            if name.is_empty() || name.len() > 12 {
                return Err(isfs::ISFS_EINVAL);
            }
            let directory = if directory.is_empty() { "/" } else { directory };
            let directory = CString::new(directory).map_err(|_| isfs::ISFS_EINVAL)?;
            let mut count = 0;
            let rc = isfs::read_dir(&directory, None, &mut count);
            if rc == isfs::ISFS_ENOENT {
                return Ok(false);
            }
            if rc < 0 {
                report_isfs_failure("ReadDir", &directory, rc);
                return Err(rc);
            }
            let count = usize::try_from(count).map_err(|_| isfs::ISFS_EINVAL)?;
            if count > MAX_DIRECTORY_ENTRIES {
                return Err(isfs::ISFS_EINVAL);
            }
            if count == 0 {
                return Ok(false);
            }
            let mut entries = AlignedDirectoryEntries(
                [0; MAX_DIRECTORY_ENTRIES * super::ISFS_DIRECTORY_ENTRY_SIZE],
            );
            let mut returned = count as u32;
            let rc = isfs::read_dir(
                &directory,
                Some(&mut entries.0[..count * super::ISFS_DIRECTORY_ENTRY_SIZE]),
                &mut returned,
            );
            if rc < 0 {
                report_isfs_failure("ReadDir entries", &directory, rc);
                return Err(rc);
            }
            let returned = usize::try_from(returned).map_err(|_| isfs::ISFS_EINVAL)?;
            directory_contains_entry(&entries.0, returned, name).ok_or(isfs::ISFS_EINVAL)
        }

        fn read_bounded(&mut self, path: &str, max_size: usize) -> Result<Option<Vec<u8>>, i32> {
            let path = CString::new(path).map_err(|_| isfs::ISFS_EINVAL)?;
            let file = match isfs::open(&path, isfs::ISFS_OPEN_READ) {
                Ok(file) => file,
                Err(error) if is_missing_isfs_error(error, ogc_filesystem::ISFS_ENOENT) => {
                    return Ok(None)
                }
                Err(error) => return Err(error),
            };
            let mut stats = AlignedFstats(isfs::Fstats::default());
            let read_result = (|| {
                let result = isfs::get_file_stats(&file, &mut stats.0);
                if result < 0 {
                    return Err(result);
                }
                let size = usize::try_from(stats.0.file_length).map_err(|_| isfs::ISFS_EINVAL)?;
                if size > max_size {
                    return Err(isfs::ISFS_EINVAL);
                }
                let mut bytes = Vec::new();
                bytes
                    .try_reserve_exact(size)
                    .map_err(|_| isfs::ISFS_ENOMEM)?;
                bytes.resize(size, 0);
                read_exact_aligned(&file, &mut bytes)?;
                Ok(bytes)
            })();
            let close_result = file.close();
            if close_result < 0 {
                return Err(close_result);
            }
            read_result.map(Some)
        }

        fn write_new(&mut self, path: &str, bytes: &[u8]) -> Result<(), i32> {
            self.write_new_with_attributes(path, bytes, None)
        }

        fn write_new_with_attributes(
            &mut self,
            path: &str,
            bytes: &[u8],
            attributes: Option<FileAttributes>,
        ) -> Result<(), i32> {
            let path = CString::new(path).map_err(|_| isfs::ISFS_EINVAL)?;
            let permissions = attributes.unwrap_or(FileAttributes {
                owner_id: 0,
                group_id: 0,
                attributes: 0,
                owner_perm: 3,
                group_perm: 3,
                other_perm: 0,
            });
            let result = isfs::create_file(
                &path,
                permissions.attributes,
                permissions.owner_perm,
                permissions.group_perm,
                permissions.other_perm,
            );
            if result < 0 {
                report_isfs_failure("CreateFile", &path, result);
                return Err(result);
            }
            if let Some(attributes) = attributes {
                let result = isfs::set_attr(
                    &path,
                    attributes.owner_id,
                    attributes.group_id,
                    attributes.attributes,
                    attributes.owner_perm,
                    attributes.group_perm,
                    attributes.other_perm,
                );
                if result < 0 {
                    report_isfs_failure("SetAttr", &path, result);
                    return Err(result);
                }
            }
            let file = match isfs::open(&path, isfs::ISFS_OPEN_RW) {
                Ok(file) => file,
                Err(error) => {
                    report_isfs_failure("Open(write)", &path, error);
                    return Err(error);
                }
            };
            let write_result: Result<(), i32> = (|| {
                let mut chunk = AlignedChunk([0; WRITE_CHUNK_SIZE]);
                for part in bytes.chunks(WRITE_CHUNK_SIZE) {
                    chunk.0[..part.len()].copy_from_slice(part);
                    if let Err(error) = file.write_all(&chunk.0[..part.len()]) {
                        report_isfs_failure("Write", &path, error);
                        return Err(error);
                    }
                }
                Ok(())
            })();
            let close_result = file.close();
            if close_result < 0 {
                report_isfs_failure("Close(write)", &path, close_result);
                return Err(close_result);
            }
            write_result?;
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

    pub(crate) fn ensure_directory(path: &core::ffi::CStr) -> Result<(), SettingsIoError> {
        let path = path.to_str().map_err(|_| SettingsIoError::InvalidPath)?;
        IsfsBackend
            .ensure_directory(path)
            .map_err(SettingsIoError::Isfs)
    }

    pub(crate) fn read_file(
        path: &CString,
        max_size: usize,
    ) -> Result<Option<Vec<u8>>, SettingsIoError> {
        let path = path.to_str().map_err(|_| SettingsIoError::InvalidPath)?;
        let mut backend = IsfsBackend;
        backend
            .read_bounded(path, max_size)
            .map_err(SettingsIoError::Isfs)
    }

    pub(crate) fn save(path: &CString, bytes: &[u8]) -> Result<(), SettingsIoError> {
        let path = path.to_str().map_err(|_| SettingsIoError::InvalidPath)?;
        save_file(&mut IsfsBackend, path, bytes).map_err(SettingsIoError::from_replace)
    }

    pub(crate) fn write_new_verified(path: &CString, bytes: &[u8]) -> Result<(), SettingsIoError> {
        let path = path.to_str().map_err(|_| SettingsIoError::InvalidPath)?;
        write_new_verified_file(&mut IsfsBackend, path, bytes)
            .map_err(SettingsIoError::from_replace)
    }

    pub(crate) fn recover(
        path: &CString,
        max_size: usize,
        validate: impl FnMut(&[u8]) -> bool,
    ) -> Result<(), SettingsIoError> {
        let path = path.to_str().map_err(|_| SettingsIoError::InvalidPath)?;
        recover_file_for_load(&mut IsfsBackend, path, max_size, validate)
            .map_err(SettingsIoError::from_replace)
    }
}

#[cfg(target_arch = "powerpc")]
pub(crate) use wii::ensure_directory as ensure_directory_wii;
#[cfg(target_arch = "powerpc")]
pub(crate) use wii::read_exact_aligned as read_exact_wii;
#[cfg(target_arch = "powerpc")]
pub(crate) use wii::read_file as read_file_wii;
#[cfg(target_arch = "powerpc")]
pub(crate) use wii::recover as recover_wii;
#[cfg(target_arch = "powerpc")]
pub(crate) use wii::save as save_wii;
#[cfg(target_arch = "powerpc")]
pub(crate) use wii::write_new_verified as write_new_verified_wii;
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{
        collections::{BTreeMap, BTreeSet},
        vec,
    };

    #[test]
    fn isfs_missing_uses_filesystem_error_namespace() {
        assert!(is_missing_isfs_error(-106, -106));
        assert!(!is_missing_isfs_error(-6, -106));
    }

    #[test]
    fn existing_directory_is_reused_without_create_request() {
        let mut create_calls = 0;
        let mut verify_calls = 0;
        let result = ensure_directory_with(
            || Ok(true),
            || {
                create_calls += 1;
                -102
            },
            || {
                verify_calls += 1;
                0
            },
            -105,
        );

        assert_eq!(result, Ok(()));
        assert_eq!(create_calls, 0);
        assert_eq!(verify_calls, 0);
    }

    #[test]
    fn missing_directory_is_created_and_verified() {
        let mut create_calls = 0;
        let mut verify_calls = 0;
        let result = ensure_directory_with(
            || Ok(false),
            || {
                create_calls += 1;
                0
            },
            || {
                verify_calls += 1;
                0
            },
            -105,
        );

        assert_eq!(result, Ok(()));
        assert_eq!(create_calls, 1);
        assert_eq!(verify_calls, 1);
    }

    #[test]
    fn directory_check_failure_stops_before_create() {
        let mut create_calls = 0;
        let result = ensure_directory_with(
            || Err(-102),
            || {
                create_calls += 1;
                0
            },
            || 0,
            -105,
        );

        assert_eq!(result, Err(DirectoryEnsureError::Read(-102)));
        assert_eq!(create_calls, 0);
    }

    #[test]
    fn directory_entry_lookup_reads_packed_nul_terminated_names() {
        // IOS uses variable length strings; 13 is the capacity per entry only.
        let entries = b"_tmp\0_bak\0settings.xml\0banner.bin\0";
        assert_eq!(
            directory_contains_entry(entries, 4, "settings.xml"),
            Some(true)
        );
        assert_eq!(
            directory_contains_entry(entries, 4, "banner.bin"),
            Some(true)
        );
        assert_eq!(
            directory_contains_entry(entries, 4, "netcfg.dat"),
            Some(false)
        );
        assert_eq!(directory_contains_entry(entries, 5, "banner.bin"), None);
        assert_eq!(directory_contains_entry(b"unterminated", 1, "x"), None);
        assert_eq!(
            directory_contains_entry(b"filename-too-long\0", 1, "x"),
            None
        );
        assert_eq!(directory_contains_entry(b"\0", 1, "x"), None);
    }

    #[test]
    fn staging_paths_preserve_the_ios_basename_and_fit_path_limits() {
        assert_eq!(
            staging_path("/title/00010001/48424345/data/banner.bin", "_tmp"),
            Ok(String::from(
                "/title/00010001/48424345/data/_tmp/banner.bin"
            ))
        );
        assert_eq!(
            staging_path("settings.xml", "_bak"),
            Ok(String::from("_bak/settings.xml"))
        );
        assert_eq!(
            staging_path("filename-too-long", "_tmp"),
            Err(ReplaceError::InvalidPath)
        );
        assert_eq!(
            staging_path(&format!("/{}/settings.xml", "x".repeat(50)), "_tmp"),
            Err(ReplaceError::InvalidPath)
        );
    }

    #[test]
    fn staging_directory_failure_does_not_change_the_primary() {
        let mut backend = MemoryBackend {
            fail_directory: true,
            ..MemoryBackend::default()
        };
        backend.files.insert(String::from("settings.xml"), vec![1]);
        assert_eq!(
            replace_file(&mut backend, "settings.xml", &[2]),
            Err(ReplaceError::Backend(-102))
        );
        assert_eq!(backend.files.len(), 1);
        assert_eq!(backend.files.get("settings.xml"), Some(&vec![1]));
    }

    #[test]
    fn verified_recovery_file_uses_existing_parent_and_reloads_exact_bytes() {
        let mut backend = MemoryBackend {
            fail_directory: true,
            ..MemoryBackend::default()
        };
        let expected = vec![0x5a; crate::network_config::CONFIG_LEN];
        assert_eq!(
            write_new_verified_file(&mut backend, "/title/data/netcfg.dat", &expected),
            Ok(())
        );
        assert_eq!(backend.directories, BTreeSet::new());
        assert_eq!(
            backend.read_bounded("/title/data/netcfg.dat", expected.len()),
            Ok(Some(expected))
        );
    }

    #[test]
    fn incomplete_recovery_file_is_removed_after_a_failed_write() {
        let mut backend = MemoryBackend {
            fail_write: Some(WriteFailure::Write),
            ..MemoryBackend::default()
        };
        assert_eq!(
            write_new_verified_file(&mut backend, "/title/data/netcfg.dat", &[1, 2, 3, 4]),
            Err(ReplaceError::Backend(-7))
        );
        assert!(!backend.files.contains_key("/title/data/netcfg.dat"));
    }

    #[test]
    fn recovery_file_cleanup_failure_preserves_both_errors() {
        let path = "/title/data/netcfg.dat";
        let mut backend = MemoryBackend {
            fail_write: Some(WriteFailure::Write),
            fail_delete_path: Some(String::from(path)),
            ..MemoryBackend::default()
        };
        assert_eq!(
            write_new_verified_file(&mut backend, path, &[1, 2, 3, 4]),
            Err(ReplaceError::Cleanup {
                operation: Some(-7),
                cleanup: -10,
            })
        );
        assert_eq!(backend.files.get(path), Some(&vec![1, 2]));
    }

    #[test]
    fn ios_basename_constraint_rejects_the_old_save_scheme() {
        let mut backend = MemoryBackend::default();
        backend.files.insert(String::from("banner.tmp"), vec![1]);
        assert_eq!(backend.rename("banner.tmp", "banner.bin"), -101);
        assert_eq!(backend.files.get("banner.tmp"), Some(&vec![1]));
        assert_eq!(replace_file(&mut backend, "banner.bin", &[2, 3]), Ok(()));
        assert_eq!(backend.read_bounded("banner.bin", 2), Ok(Some(vec![2, 3])));
    }

    #[test]
    fn network_save_commits_real_bytes_with_nand_filename_limits() {
        let path = "/shared2/sys/net/02/config.dat";
        let original = vec![1; crate::network_config::CONFIG_LEN];
        let replacement = vec![2; crate::network_config::CONFIG_LEN];
        let mut backend = MemoryBackend::default();
        backend.files.insert(String::from(path), original.clone());
        assert_eq!(
            replace_file_with_attributes(&mut backend, path, &replacement, None, Some(&original)),
            Ok(())
        );
        assert_eq!(
            backend.read_bounded(path, replacement.len()),
            Ok(Some(replacement))
        );
        assert!(!backend
            .files
            .contains_key("/shared2/sys/net/02/_tmp/config.dat"));
        assert!(!backend
            .files
            .contains_key("/shared2/sys/net/02/_bak/config.dat"));
    }

    #[derive(Default)]
    struct MemoryBackend {
        files: BTreeMap<String, Vec<u8>>,
        directories: BTreeSet<String>,
        fail_directory: bool,
        fail_write: Option<WriteFailure>,
        corrupt_staged_file: bool,
        fail_rename_call: Option<usize>,
        fail_rollback: bool,
        fail_delete_path: Option<String>,
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
        fn ensure_directory(&mut self, path: &str) -> Result<(), i32> {
            if self.fail_directory || self.files.contains_key(path) {
                return Err(-102);
            }
            self.directories.insert(String::from(path));
            Ok(())
        }
        fn exists(&mut self, path: &str) -> Result<bool, i32> {
            Ok(self.files.contains_key(path))
        }

        fn read_bounded(&mut self, path: &str, max_size: usize) -> Result<Option<Vec<u8>>, i32> {
            match self.files.get(path) {
                Some(bytes) if bytes.len() <= max_size => Ok(Some(bytes.clone())),
                Some(_) => Err(-7),
                None => Ok(None),
            }
        }

        fn write_new(&mut self, path: &str, bytes: &[u8]) -> Result<(), i32> {
            // Explicit filesystem constraint at the production engine boundary.
            if path.rsplit('/').next().is_none_or(|name| name.len() > 12) {
                return Err(-101);
            }
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
            let mut stored = bytes.to_vec();
            if self.corrupt_staged_file && !stored.is_empty() {
                stored[0] ^= 0xff;
            }
            self.files.insert(String::from(path), stored);
            Ok(())
        }

        fn rename(&mut self, old_path: &str, new_path: &str) -> i32 {
            if [old_path, new_path]
                .iter()
                .any(|path| path.rsplit('/').next().is_none_or(|name| name.len() > 12))
            {
                return -101;
            }
            if old_path.rsplit('/').next() != new_path.rsplit('/').next() {
                return -101;
            }
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
            if self.fail_delete_path.as_deref() == Some(path) {
                return -10;
            }
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
        assert!(!backend.files.contains_key("_bak/settings.xml"));
        assert!(!backend.files.contains_key("_tmp/settings.xml"));
    }

    #[test]
    fn first_save_creates_and_verifies_file_without_staging_directories() {
        let mut backend = MemoryBackend::default();
        assert_eq!(save_file(&mut backend, "settings.xml", &[1, 2, 3]), Ok(()));
        assert_eq!(backend.files.get("settings.xml"), Some(&vec![1, 2, 3]));
        assert!(backend.directories.is_empty());
        assert_eq!(backend.rename_calls, 0);
    }

    #[test]
    fn existing_file_save_keeps_the_staged_replacement_path() {
        let mut backend = MemoryBackend::default();
        backend.files.insert(String::from("settings.xml"), vec![1]);
        assert_eq!(save_file(&mut backend, "settings.xml", &[2, 3]), Ok(()));
        assert_eq!(backend.files.get("settings.xml"), Some(&vec![2, 3]));
        assert!(backend.directories.contains("_tmp"));
        assert!(backend.directories.contains("_bak"));
    }

    #[test]
    fn compare_and_replace_rejects_a_changed_primary_before_commit() {
        let mut backend = MemoryBackend::default();
        backend
            .files
            .insert(String::from("config.dat"), vec![1, 2, 3]);
        assert_eq!(
            replace_file_with_attributes(
                &mut backend,
                "config.dat",
                &[4, 5, 6],
                None,
                Some(&[1, 2, 4]),
            ),
            Err(ReplaceError::StaleSnapshot)
        );
        assert_eq!(backend.files.get("config.dat"), Some(&vec![1, 2, 3]));
        assert!(!backend.files.contains_key("_tmp/config.dat"));
    }

    #[test]
    fn backup_cleanup_failure_reports_committed_primary_without_removing_it() {
        let mut backend = MemoryBackend::default();
        backend.files.insert(String::from("settings.xml"), vec![1]);
        backend.fail_delete_path = Some(String::from("_bak/settings.xml"));

        assert_eq!(
            replace_file(&mut backend, "settings.xml", &[2, 3]),
            Err(ReplaceError::CommittedCleanup(-10))
        );
        assert_eq!(backend.files.get("settings.xml"), Some(&vec![2, 3]));
        assert_eq!(backend.files.get("_bak/settings.xml"), Some(&vec![1]));
        assert!(!backend.files.contains_key("_tmp/settings.xml"));
    }

    #[test]
    fn failed_operation_and_failed_temp_cleanup_are_both_reported() {
        let mut backend = MemoryBackend::default();
        backend.files.insert(String::from("settings.xml"), vec![1]);
        backend.fail_write = Some(WriteFailure::Write);
        backend.fail_delete_path = Some(String::from("_tmp/settings.xml"));

        assert_eq!(
            replace_file(&mut backend, "settings.xml", &[2, 3]),
            Err(ReplaceError::Cleanup {
                operation: Some(-7),
                cleanup: -10,
            })
        );
        assert_eq!(backend.files.get("settings.xml"), Some(&vec![1]));
        assert_eq!(backend.files.get("_tmp/settings.xml"), Some(&vec![2]));
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
            assert_eq!(
                replace_file(&mut backend, "settings.xml", &[2, 3]),
                Err(ReplaceError::Backend(-7))
            );
            assert_eq!(backend.files.get("settings.xml"), Some(&vec![1]));
            assert!(!backend.files.contains_key("_tmp/settings.xml"));
        }
    }

    #[test]
    fn staged_bytes_are_verified_before_replacing_the_original() {
        let mut backend = MemoryBackend {
            corrupt_staged_file: true,
            ..MemoryBackend::default()
        };
        backend.files.insert(String::from("settings.xml"), vec![1]);
        assert_eq!(
            replace_file(&mut backend, "settings.xml", &[2, 3]),
            Err(ReplaceError::TemporaryMismatch)
        );
        assert_eq!(backend.files.get("settings.xml"), Some(&vec![1]));
        assert!(!backend.files.contains_key("_bak/settings.xml"));
    }

    #[test]
    fn failed_install_rename_restores_the_previous_file() {
        let mut backend = MemoryBackend::default();
        backend.files.insert(String::from("settings.xml"), vec![1]);
        backend.fail_rename_call = Some(2);
        assert_eq!(
            replace_file(&mut backend, "settings.xml", &[2]),
            Err(ReplaceError::Backend(-8))
        );
        assert_eq!(backend.files.get("settings.xml"), Some(&vec![1]));
        assert!(!backend.files.contains_key("_bak/settings.xml"));
        assert!(!backend.files.contains_key("_tmp/settings.xml"));
    }

    #[test]
    fn interrupted_backup_move_is_restored_before_the_next_save() {
        let mut backend = MemoryBackend::default();
        backend
            .files
            .insert(String::from("_bak/settings.xml"), vec![1]);
        backend
            .files
            .insert(String::from("_tmp/settings.xml"), vec![2]);
        assert_eq!(
            recover_file_for_load(&mut backend, "settings.xml", 10, |bytes| bytes == [1]),
            Ok(())
        );
        assert_eq!(replace_file(&mut backend, "settings.xml", &[3]), Ok(()));
        assert_eq!(backend.files.get("settings.xml"), Some(&vec![3]));
        assert!(!backend.files.contains_key("_bak/settings.xml"));
        assert!(!backend.files.contains_key("_tmp/settings.xml"));
    }

    #[test]
    fn load_recovery_restores_backup_when_primary_is_missing() {
        let mut backend = MemoryBackend::default();
        backend
            .files
            .insert(String::from("_bak/settings.xml"), vec![1]);
        backend
            .files
            .insert(String::from("_tmp/settings.xml"), vec![2]);
        assert_eq!(
            recover_file_for_load(&mut backend, "settings.xml", 10, |bytes| bytes == [1]),
            Ok(())
        );
        assert_eq!(backend.files.get("settings.xml"), Some(&vec![1]));
        assert!(!backend.files.contains_key("_tmp/settings.xml"));
    }

    #[test]
    fn load_recovery_keeps_the_backup_when_primary_fails_validation() {
        let mut backend = MemoryBackend::default();
        backend.files.insert(String::from("settings.xml"), vec![2]);
        backend
            .files
            .insert(String::from("_bak/settings.xml"), vec![1]);

        assert_eq!(
            recover_file_for_load(&mut backend, "settings.xml", 10, |bytes| bytes == [1]),
            Ok(())
        );
        assert_eq!(backend.files.get("settings.xml"), Some(&vec![1]));
        assert!(!backend.files.contains_key("_bak/settings.xml"));
    }

    #[test]
    fn load_recovery_deletes_backup_only_after_primary_validation() {
        let mut backend = MemoryBackend::default();
        backend.files.insert(String::from("settings.xml"), vec![1]);
        backend
            .files
            .insert(String::from("_bak/settings.xml"), vec![2]);

        assert_eq!(
            recover_file_for_load(&mut backend, "settings.xml", 10, |bytes| bytes == [1]),
            Ok(())
        );
        assert_eq!(backend.files.get("settings.xml"), Some(&vec![1]));
        assert!(!backend.files.contains_key("_bak/settings.xml"));
    }

    #[test]
    fn invalid_primary_and_backup_are_both_preserved() {
        let mut backend = MemoryBackend::default();
        backend.files.insert(String::from("settings.xml"), vec![2]);
        backend
            .files
            .insert(String::from("_bak/settings.xml"), vec![3]);

        assert_eq!(
            recover_file_for_load(&mut backend, "settings.xml", 10, |bytes| bytes == [1]),
            Ok(())
        );
        assert_eq!(backend.files.get("settings.xml"), Some(&vec![2]));
        assert_eq!(backend.files.get("_bak/settings.xml"), Some(&vec![3]));
    }

    #[test]
    fn invalid_backup_without_primary_blocks_load_recovery() {
        let mut backend = MemoryBackend::default();
        backend
            .files
            .insert(String::from("_bak/settings.xml"), vec![3]);

        assert_eq!(
            recover_file_for_load(&mut backend, "settings.xml", 10, |bytes| bytes == [1]),
            Err(ReplaceError::BackupNeedsValidation)
        );
        assert_eq!(backend.files.get("_bak/settings.xml"), Some(&vec![3]));
        assert!(!backend.files.contains_key("settings.xml"));
    }

    #[test]
    fn save_refuses_to_replace_unvalidated_recovery_files() {
        let mut backend = MemoryBackend::default();
        backend.files.insert(String::from("settings.xml"), vec![1]);
        backend
            .files
            .insert(String::from("_bak/settings.xml"), vec![2]);

        assert_eq!(
            replace_file(&mut backend, "settings.xml", &[3]),
            Err(ReplaceError::BackupNeedsValidation)
        );
        assert_eq!(backend.files.get("settings.xml"), Some(&vec![1]));
        assert_eq!(backend.files.get("_bak/settings.xml"), Some(&vec![2]));
        assert!(!backend.files.contains_key("_tmp/settings.xml"));
    }

    #[test]
    fn load_recovery_restores_backup_after_install_and_rollback_rename_failures() {
        let mut backend = MemoryBackend::default();
        backend.files.insert(String::from("settings.xml"), vec![1]);
        backend.fail_rename_call = Some(2);
        backend.fail_rollback = true;
        assert_eq!(
            replace_file(&mut backend, "settings.xml", &[2]),
            Err(ReplaceError::Backend(-8))
        );
        assert!(!backend.files.contains_key("settings.xml"));
        assert_eq!(backend.files.get("_bak/settings.xml"), Some(&vec![1]));

        assert_eq!(
            recover_file_for_load(&mut backend, "settings.xml", 10, |bytes| bytes == [1]),
            Ok(())
        );
        assert_eq!(backend.files.get("settings.xml"), Some(&vec![1]));
        assert!(!backend.files.contains_key("_bak/settings.xml"));
        assert!(!backend.files.contains_key("_tmp/settings.xml"));
    }
}
