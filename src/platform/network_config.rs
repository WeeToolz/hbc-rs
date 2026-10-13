//! Read and safely replace the Wii system network configuration on NAND.

use alloc::ffi::CString;
use core::{ffi::CStr, mem::MaybeUninit};

use crate::network_config::{self, NetworkConfig, CONFIG_LEN};
use crate::platform::{nand, settings::SettingsIoError};
use ogc_es_sys as es;
use ogc_fat::{FatError, FatFile};
use ogc_filesystem as isfs;

const NAND_PATH: &CStr = c"/shared2/sys/net/02/config.dat";
const BACKUP_FILE_NAME: &str = "netcfg.dat";
const CURRENT_BACKUP_FILE_NAME: &str = "netcur.dat";
const ISFS_EACCES: i32 = -102;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageBackupStage {
    Open,
    Read,
    Write,
    Flush,
    RemoveTemporary,
    Rename,
}

#[repr(align(32))]
struct AlignedConfig([u8; CONFIG_LEN]);
#[repr(align(32))]
#[cfg(target_arch = "powerpc")]
struct AlignedWriteConfig([u8; CONFIG_LEN]);

#[repr(align(32))]
struct AlignedFstats(isfs::Fstats);

#[repr(align(32))]
struct AlignedDeviceId(u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkConfigError {
    Isfs(i32),
    ReadAndClose {
        read: i32,
        close: i32,
    },
    InvalidPath,
    InvalidFileSize,
    InvalidFormat(crate::network_config::NetworkFormatError),
    StaleSnapshot,
    Recovery(SettingsIoError),
    StorageBackup(FatError),
    StorageBackupAt {
        stage: StorageBackupStage,
        error: FatError,
    },
    StorageBackupExists,
    StorageBackupAllocation,
    BackupStorageUnavailable,
    BackupUnavailable,
    DirectWrite(i32),
    DirectWriteAndClose {
        write: i32,
        close: i32,
    },
    InvalidWifi(crate::network_config::WifiValidationError),
    WriteAndClose {
        write: i32,
        close: i32,
    },
    VerificationMismatch,
    BackupInvalid,
    RestoreFailed,
    VerificationFailed,
}

/// Explain the common ISFS invalid-argument result without presenting it as a bad password.
pub fn save_error_message(error: NetworkConfigError) -> alloc::string::String {
    use crate::platform::settings::SettingsIoError;
    match error {
        NetworkConfigError::StorageBackupExists => return alloc::string::String::from(
            "The network settings backup already exists and could not be replaced. Settings were not written.",
        ),
        NetworkConfigError::StorageBackup(fat_error) => return alloc::format!(
            "Could not create or verify the versioned network settings backup on the selected storage device: {fat_error:?}. Settings were not written."
        ),
        NetworkConfigError::StorageBackupAt { stage, error } => return alloc::format!(
            "The selected storage device failed during network backup {stage:?}: {error:?}. Settings were not written."
        ),
        NetworkConfigError::StorageBackupAllocation => return alloc::string::String::from(
            "Could not allocate memory to verify the network settings backup. Settings were not written.",
        ),
        NetworkConfigError::BackupStorageUnavailable => return alloc::string::String::from(
            "No storage device is mounted for the network settings backup. Settings were not written.",
        ),
        NetworkConfigError::BackupUnavailable => return alloc::string::String::from(
            "No verified recovery backup is available. The Wii network settings were not changed.",
        ),
        NetworkConfigError::BackupInvalid => return alloc::string::String::from(
            "A network settings backup is damaged or belongs to another Wii. The Wii network settings were not changed.",
        ),
        NetworkConfigError::RestoreFailed => return alloc::string::String::from(
            "A verified network settings backup could not be restored. Keep the Wii powered on and retry recovery.",
        ),
        NetworkConfigError::VerificationFailed => return alloc::string::String::from(
            "The restored network settings did not match the verified backup. The recovery copy was retained.",
        ),
        NetworkConfigError::DirectWrite(code) => return alloc::format!(
            "IOS could not write the existing Wii network settings file directly (ISFS {code}). The original file is backed up on the selected storage device."
        ),
        NetworkConfigError::DirectWriteAndClose { write, close } => return alloc::format!(
            "IOS failed while writing the existing Wii network settings file directly (write {write}, close {close}). The original file is backed up on the selected storage device."
        ),
        _ => {}
    }
    let isfs_access_denied = match error {
        NetworkConfigError::Isfs(code) => code == ISFS_EACCES,
        NetworkConfigError::WriteAndClose { write, close } => {
            write == ISFS_EACCES || close == ISFS_EACCES
        }
        NetworkConfigError::ReadAndClose { read, close } => {
            read == ISFS_EACCES || close == ISFS_EACCES
        }
        NetworkConfigError::Recovery(SettingsIoError::Isfs(code))
        | NetworkConfigError::Recovery(SettingsIoError::CommittedCleanup(code)) => {
            code == ISFS_EACCES
        }
        NetworkConfigError::Recovery(SettingsIoError::IsfsCleanup { operation, cleanup }) => {
            operation == Some(ISFS_EACCES) || cleanup == ISFS_EACCES
        }
        _ => false,
    };
    let isfs_invalid_argument = match error {
        NetworkConfigError::Isfs(code) => code == isfs::ISFS_EINVAL,
        NetworkConfigError::WriteAndClose { write, close } => {
            write == isfs::ISFS_EINVAL || close == isfs::ISFS_EINVAL
        }
        NetworkConfigError::Recovery(SettingsIoError::Isfs(code)) => code == isfs::ISFS_EINVAL,
        NetworkConfigError::Recovery(SettingsIoError::IsfsCleanup { operation, cleanup }) => {
            operation == Some(isfs::ISFS_EINVAL) || cleanup == isfs::ISFS_EINVAL
        }
        NetworkConfigError::Recovery(SettingsIoError::CommittedCleanup(code)) => {
            code == isfs::ISFS_EINVAL
        }
        _ => false,
    };
    if isfs_access_denied {
        alloc::format!(
            "IOS denied access to a Wii network settings file (ISFS -102). Details: {error:?}."
        )
    } else if isfs_invalid_argument {
        alloc::format!("The Wii file system rejected a request (ISFS -101). Details: {error:?}. The password passed local validation.")
    } else {
        alloc::format!("Could not save Wii network settings: {error:?}.")
    }
}

/// Load the current Wii network profiles without changing the saved bytes.
pub fn load(storage_prefix: Option<&str>) -> Result<NetworkConfig, NetworkConfigError> {
    initialize_isfs()?;
    let path = nand_path()?;
    recover_invalid_config(&path, storage_prefix)?;

    let file = isfs::open(&path, isfs::ISFS_OPEN_READ).map_err(NetworkConfigError::Isfs)?;
    let mut stats = AlignedFstats(isfs::Fstats::default());
    let stat_result = isfs::get_file_stats(&file, &mut stats.0);
    if stat_result < 0 {
        return close_after_error(file, stat_result);
    }
    if usize::try_from(stats.0.file_length).ok() != Some(CONFIG_LEN) {
        return close_after_error(file, isfs::ISFS_EINVAL);
    }

    let mut buffer = MaybeUninit::<AlignedConfig>::zeroed();
    // SAFETY: AlignedConfig is a byte array. It is valid to write each byte
    // before assuming the initialized buffer below.
    let bytes = unsafe { &mut (*buffer.as_mut_ptr()).0 };
    let read_result = read_all(&file, bytes);
    let close_result = file.close();
    if close_result < 0 {
        if let Err(NetworkConfigError::Isfs(read)) = read_result {
            return Err(NetworkConfigError::ReadAndClose {
                read,
                close: close_result,
            });
        }
        if let Err(NetworkConfigError::InvalidFileSize) = read_result {
            return Err(NetworkConfigError::ReadAndClose {
                read: isfs::ISFS_EINVAL,
                close: close_result,
            });
        }
        return Err(NetworkConfigError::Isfs(close_result));
    }
    read_result?;
    // SAFETY: read_all returned success only after filling the full array.
    let config = unsafe { buffer.assume_init() }.0;
    let config = NetworkConfig::from_bytes(config);
    config
        .validate_format()
        .map_err(NetworkConfigError::InvalidFormat)?;
    Ok(config)
}

/// Replace the system network config after validating the WiFi fields.
/// Unknown and unrelated profile bytes remain unchanged in `config`.
pub fn save(
    config: &NetworkConfig,
    expected: &NetworkConfig,
    storage_prefix: Option<&str>,
) -> Result<(), NetworkConfigError> {
    save_impl(config, expected, storage_prefix, true)
}

/// Replace the Wii network config without a recovery backup.
///
/// The UI must require a separate confirmation before it calls this function.
/// Input validation, stale-snapshot checks, and read-back verification still
/// run. A failed write can leave the system file damaged because no backup is
/// available to restore it.
pub fn save_without_backup(
    config: &NetworkConfig,
    expected: &NetworkConfig,
) -> Result<(), NetworkConfigError> {
    save_impl(config, expected, None, false)
}

fn save_impl(
    config: &NetworkConfig,
    expected: &NetworkConfig,
    storage_prefix: Option<&str>,
    create_recovery_backups: bool,
) -> Result<(), NetworkConfigError> {
    let current = load(storage_prefix)?;
    if current != *expected {
        return Err(NetworkConfigError::StaleSnapshot);
    }
    config
        .validate_format()
        .map_err(NetworkConfigError::InvalidFormat)?;
    let profile = config.active_wifi_profile().map_err(|error| {
        NetworkConfigError::InvalidFormat(crate::network_config::NetworkFormatError::Profile(error))
    })?;
    crate::network_config::validate_wifi(
        config
            .profile(profile)
            .ok_or(NetworkConfigError::InvalidFileSize)?
            .wifi(),
    )
    .map_err(NetworkConfigError::InvalidWifi)?;

    initialize_isfs()?;

    if !create_recovery_backups {
        let path = nand_path()?;
        return network_config::commit_unbacked_config(
            expected.as_bytes(),
            config.as_bytes(),
            valid_config_bytes,
            || read_raw_config(&path),
            |bytes| write_existing_config(&path, bytes),
        )
        .map_err(|error| match error {
            network_config::UnbackedSaveError::PrimaryRead(error)
            | network_config::UnbackedSaveError::PrimaryWrite(error)
            | network_config::UnbackedSaveError::VerificationRead(error) => error,
            network_config::UnbackedSaveError::PrimaryChanged => NetworkConfigError::StaleSnapshot,
            network_config::UnbackedSaveError::VerificationFailed => {
                NetworkConfigError::VerificationFailed
            }
        });
    }

    let app_original = crate::platform::settings::title_data_file_path(BACKUP_FILE_NAME).ok();
    let mut app_original_ready = false;
    if let Some(backup) = app_original.as_ref() {
        match nand::read_file_wii(backup, CONFIG_LEN) {
            Ok(Some(bytes)) => app_original_ready = valid_config_bytes(&bytes),
            Ok(None) => match nand::write_new_verified_wii(backup, expected.as_bytes()) {
                Ok(()) => app_original_ready = true,
                Err(error @ SettingsIoError::IsfsCleanup { .. })
                | Err(error @ SettingsIoError::CommittedCleanup(_)) => {
                    return Err(NetworkConfigError::Recovery(error));
                }
                Err(error) => ogc_system::report(format_args!(
                    "HBC: app NAND network backup unavailable ({error:?}); trying active storage\n"
                )),
            },
            Err(error) => ogc_system::report(format_args!(
                "HBC: app NAND network backup could not be read ({error:?}); trying active storage\n"
            )),
        }
    }

    let external = if app_original_ready {
        None
    } else {
        let prefix = storage_prefix.ok_or(NetworkConfigError::BackupUnavailable)?;
        let device_id = wii_device_id()?;
        let original_path = ensure_external_original(prefix, device_id, expected.as_bytes())?;
        Some((prefix, device_id, original_path))
    };

    if let Ok(current_path) =
        crate::platform::settings::title_data_file_path(CURRENT_BACKUP_FILE_NAME)
    {
        let commit = if app_original_ready {
            match app_original.as_ref() {
                Some(original) => Some(commit_with_nand_backup(
                    &current_path,
                    config,
                    expected,
                    || verify_nand_original(original),
                )),
                None => return Err(NetworkConfigError::BackupUnavailable),
            }
        } else if let Some((_, device_id, original)) = external.as_ref() {
            Some(commit_with_nand_backup(
                &current_path,
                config,
                expected,
                || verify_external_original(original, *device_id),
            ))
        } else {
            None
        };
        if let Some(result) = commit {
            match result {
                Ok(()) => return Ok(()),
                Err(error @ network_config::ConfigSaveError::OriginalBackup(_))
                | Err(error @ network_config::ConfigSaveError::CurrentBackupWrite(_))
                | Err(error @ network_config::ConfigSaveError::CurrentBackupRead(_))
                | Err(error @ network_config::ConfigSaveError::CurrentBackupInvalid) => {
                    ogc_system::report(format_args!(
                        "HBC: app NAND recovery copy unavailable ({error:?}); trying active storage\n"
                    ));
                }
                Err(error) => return map_save_error(Err(error)),
            }
        }
    }

    let (prefix, device_id, original_path) = match external {
        Some(external) => external,
        None => {
            let prefix = storage_prefix.ok_or(NetworkConfigError::BackupUnavailable)?;
            let device_id = wii_device_id()?;
            let original_path = ensure_external_original(prefix, device_id, expected.as_bytes())?;
            (prefix, device_id, original_path)
        }
    };
    let current_path = current_backup_path(prefix, device_id)?;
    commit_with_external_backup(&current_path, &original_path, device_id, config, expected)
}

#[cfg(target_arch = "powerpc")]
fn commit_with_nand_backup(
    current_backup: &CString,
    config: &NetworkConfig,
    expected: &NetworkConfig,
    mut verify_original: impl FnMut() -> Result<(), NetworkConfigError>,
) -> Result<(), network_config::ConfigSaveError<NetworkConfigError>> {
    network_config::save_with_recovery_backups(
        expected.as_bytes(),
        config.as_bytes(),
        valid_config_bytes,
        || verify_original(),
        |bytes| save_nand_current_backup(current_backup, bytes),
        || nand::read_file_wii(current_backup, CONFIG_LEN).map_err(NetworkConfigError::Recovery),
        || {
            let path = nand_path()?;
            read_raw_config(&path)
        },
        |bytes| {
            let path = nand_path()?;
            write_existing_config(&path, bytes)
        },
    )
}

#[cfg(target_arch = "powerpc")]
fn commit_with_external_backup(
    current_backup: &CStr,
    original_backup: &CStr,
    device_id: u32,
    config: &NetworkConfig,
    expected: &NetworkConfig,
) -> Result<(), NetworkConfigError> {
    let path = nand_path()?;
    let result = network_config::save_with_recovery_backups(
        expected.as_bytes(),
        config.as_bytes(),
        valid_config_bytes,
        || verify_external_original(original_backup, device_id),
        |bytes| {
            write_storage_backup(
                current_backup,
                bytes,
                device_id,
                network_config::CURRENT_BACKUP_KIND,
                false,
            )
        },
        || {
            read_storage_backup(
                current_backup,
                device_id,
                network_config::CURRENT_BACKUP_KIND,
            )
        },
        || read_raw_config(&path),
        |bytes| write_existing_config(&path, bytes),
    );
    map_save_error(result)
}

#[cfg(target_arch = "powerpc")]
fn map_save_error(
    result: Result<(), network_config::ConfigSaveError<NetworkConfigError>>,
) -> Result<(), NetworkConfigError> {
    result.map_err(|error| match error {
        network_config::ConfigSaveError::OriginalBackup(error)
        | network_config::ConfigSaveError::CurrentBackupWrite(error)
        | network_config::ConfigSaveError::CurrentBackupRead(error)
        | network_config::ConfigSaveError::PrimaryRead(error)
        | network_config::ConfigSaveError::PrimaryWrite(error)
        | network_config::ConfigSaveError::PrimaryVerificationRead(error) => error,
        network_config::ConfigSaveError::CurrentBackupInvalid => NetworkConfigError::BackupInvalid,
        network_config::ConfigSaveError::PrimaryChanged => NetworkConfigError::StaleSnapshot,
        network_config::ConfigSaveError::PrimaryVerificationFailed => {
            NetworkConfigError::VerificationFailed
        }
    })
}

#[cfg(target_arch = "powerpc")]
fn verify_nand_original(path: &CString) -> Result<(), NetworkConfigError> {
    match nand::read_file_wii(path, CONFIG_LEN).map_err(NetworkConfigError::Recovery)? {
        Some(bytes) if valid_config_bytes(&bytes) => Ok(()),
        Some(_) => Err(NetworkConfigError::BackupInvalid),
        None => Err(NetworkConfigError::BackupUnavailable),
    }
}

#[cfg(target_arch = "powerpc")]
fn verify_external_original(path: &CStr, device_id: u32) -> Result<(), NetworkConfigError> {
    match read_storage_backup(path, device_id, network_config::ORIGINAL_BACKUP_KIND)? {
        Some(_) => Ok(()),
        None => Err(NetworkConfigError::BackupUnavailable),
    }
}

#[cfg(target_arch = "powerpc")]
fn save_nand_current_backup(path: &CString, current: &[u8]) -> Result<(), NetworkConfigError> {
    match nand::read_file_wii(path, CONFIG_LEN) {
        Ok(Some(existing)) if existing == current => Ok(()),
        Ok(Some(_)) => {
            write_existing_config(path, current)?;
            match nand::read_file_wii(path, CONFIG_LEN) {
                Ok(Some(saved)) if saved == current => Ok(()),
                Ok(_) => Err(NetworkConfigError::VerificationFailed),
                Err(error) => Err(NetworkConfigError::Recovery(error)),
            }
        }
        Ok(None) => {
            nand::write_new_verified_wii(path, current).map_err(NetworkConfigError::Recovery)
        }
        Err(error) => Err(NetworkConfigError::Recovery(error)),
    }
}

#[cfg(target_arch = "powerpc")]
fn ensure_external_original(
    prefix: &str,
    device_id: u32,
    current: &[u8],
) -> Result<CString, NetworkConfigError> {
    let path = backup_path(prefix, device_id)?;
    match read_storage_backup(&path, device_id, network_config::ORIGINAL_BACKUP_KIND) {
        Ok(Some(_)) => return Ok(path),
        Ok(None) | Err(NetworkConfigError::BackupInvalid) => {}
        Err(error) => return Err(error),
    }

    // Reuse a valid same-console backup from the earlier shared filename.
    // Keep an invalid or foreign-console file in place and use the new
    // device-specific name instead.
    let legacy = legacy_storage_backup_path(prefix, network_config::ORIGINAL_BACKUP_KIND)?;
    match read_storage_backup(&legacy, device_id, network_config::ORIGINAL_BACKUP_KIND) {
        Ok(Some(_)) => return Ok(legacy),
        Ok(None) | Err(NetworkConfigError::BackupInvalid) => {}
        Err(error) => return Err(error),
    }

    create_external_original(&path, current, device_id)?;
    Ok(path)
}

#[cfg(target_arch = "powerpc")]
fn create_external_original(
    path: &CStr,
    current: &[u8],
    device_id: u32,
) -> Result<(), NetworkConfigError> {
    let temporary = CString::new(alloc::format!("{}.tmp", path.to_string_lossy()))
        .map_err(|_| NetworkConfigError::InvalidPath)?;
    match read_storage_backup(&temporary, device_id, network_config::ORIGINAL_BACKUP_KIND) {
        Ok(Some(_)) => {}
        Ok(None) => {
            // A missing temporary file needs no cleanup. Some FAT backends
            // report a failed unlink of a missing path as a generic I/O error.
            write_storage_backup(
                &temporary,
                current,
                device_id,
                network_config::ORIGINAL_BACKUP_KIND,
                true,
            )?;
        }
        Err(NetworkConfigError::BackupInvalid) => {
            match ogc_fat::remove_file(&temporary) {
                Ok(()) | Err(FatError::NotFound) => {}
                Err(error) => {
                    return Err(NetworkConfigError::StorageBackupAt {
                        stage: StorageBackupStage::RemoveTemporary,
                        error,
                    })
                }
            }
            if let Err(error) = write_storage_backup(
                &temporary,
                current,
                device_id,
                network_config::ORIGINAL_BACKUP_KIND,
                true,
            ) {
                match ogc_fat::remove_file(&temporary) {
                    Ok(()) | Err(FatError::NotFound) => return Err(error),
                    Err(error) => {
                        return Err(NetworkConfigError::StorageBackupAt {
                            stage: StorageBackupStage::RemoveTemporary,
                            error,
                        })
                    }
                }
            }
            match read_storage_backup(&temporary, device_id, network_config::ORIGINAL_BACKUP_KIND) {
                Ok(Some(bytes)) if bytes == current => {}
                Ok(_) | Err(NetworkConfigError::BackupInvalid) => {
                    match ogc_fat::remove_file(&temporary) {
                        Ok(()) | Err(FatError::NotFound) => {
                            return Err(NetworkConfigError::VerificationFailed)
                        }
                        Err(error) => {
                            return Err(NetworkConfigError::StorageBackupAt {
                                stage: StorageBackupStage::RemoveTemporary,
                                error,
                            })
                        }
                    }
                }
                Err(error) => return Err(error),
            }
        }
        Err(error) => return Err(error),
    }

    ogc_fat::rename(&temporary, path).map_err(|error| match error {
        FatError::AlreadyExists => NetworkConfigError::StorageBackupExists,
        other => NetworkConfigError::StorageBackupAt {
            stage: StorageBackupStage::Rename,
            error: other,
        },
    })?;
    verify_external_original(path, device_id)
}

#[cfg(target_arch = "powerpc")]
#[cfg(target_arch = "powerpc")]
fn wii_device_id() -> Result<u32, NetworkConfigError> {
    let rc = es::init();
    if rc < 0 {
        return Err(NetworkConfigError::BackupUnavailable);
    }
    let mut device_id = AlignedDeviceId(0);
    let rc = unsafe { es::get_device_id(&mut device_id.0) };
    if rc < 0 {
        Err(NetworkConfigError::BackupUnavailable)
    } else {
        Ok(device_id.0)
    }
}

fn backup_path(prefix: &str, device_id: u32) -> Result<CString, NetworkConfigError> {
    let path = crate::network_config::active_storage_backup_path(prefix, device_id)
        .ok_or(NetworkConfigError::InvalidPath)?;
    CString::new(path).map_err(|_| NetworkConfigError::InvalidPath)
}

#[cfg(target_arch = "powerpc")]
fn current_backup_path(prefix: &str, device_id: u32) -> Result<CString, NetworkConfigError> {
    let path = crate::network_config::active_storage_current_backup_path(prefix, device_id)
        .ok_or(NetworkConfigError::InvalidPath)?;
    CString::new(path).map_err(|_| NetworkConfigError::InvalidPath)
}

#[cfg(target_arch = "powerpc")]
fn legacy_storage_backup_path(prefix: &str, kind: u8) -> Result<CString, NetworkConfigError> {
    if !matches!(prefix, "sd:" | "usb:" | "carda:" | "cardb:") {
        return Err(NetworkConfigError::InvalidPath);
    }
    let name = match kind {
        network_config::ORIGINAL_BACKUP_KIND => "netcfg.v1",
        network_config::CURRENT_BACKUP_KIND => "netcur.v1",
        _ => return Err(NetworkConfigError::InvalidPath),
    };
    CString::new(alloc::format!("{prefix}/apps/hbc-rs/{name}"))
        .map_err(|_| NetworkConfigError::InvalidPath)
}

fn write_storage_backup(
    path: &CStr,
    bytes: &[u8],
    device_id: u32,
    kind: u8,
    create_new: bool,
) -> Result<(), NetworkConfigError> {
    let record = network_config::encode_external_backup(device_id, kind, bytes)
        .ok_or(NetworkConfigError::InvalidFileSize)?;
    let mut file = if create_new {
        FatFile::create_new(path)
    } else {
        FatFile::create_truncate(path)
    }
    .map_err(|error| match error {
        FatError::AlreadyExists => NetworkConfigError::StorageBackupExists,
        other => NetworkConfigError::StorageBackupAt {
            stage: StorageBackupStage::Open,
            error: other,
        },
    })?;
    let mut offset = 0;
    while offset < record.len() {
        let written =
            file.write(&record[offset..])
                .map_err(|error| NetworkConfigError::StorageBackupAt {
                    stage: StorageBackupStage::Write,
                    error,
                })?;
        if written == 0 {
            return Err(NetworkConfigError::StorageBackupAt {
                stage: StorageBackupStage::Write,
                error: FatError::Io,
            });
        }
        offset += written;
    }
    file.flush()
        .map_err(|error| NetworkConfigError::StorageBackupAt {
            stage: StorageBackupStage::Flush,
            error,
        })
}

fn read_storage_backup(
    path: &CStr,
    device_id: u32,
    kind: u8,
) -> Result<Option<alloc::vec::Vec<u8>>, NetworkConfigError> {
    let mut file = match FatFile::open(path) {
        Ok(file) => file,
        Err(FatError::NotFound) => return Ok(None),
        Err(error) => {
            return Err(NetworkConfigError::StorageBackupAt {
                stage: StorageBackupStage::Open,
                error,
            })
        }
    };
    let record_size = CONFIG_LEN + 20;
    if usize::try_from(file.len()).ok() != Some(record_size) {
        return Err(NetworkConfigError::BackupInvalid);
    }
    let mut bytes = alloc::vec::Vec::new();
    bytes
        .try_reserve_exact(record_size)
        .map_err(|_| NetworkConfigError::StorageBackupAllocation)?;
    bytes.resize(record_size, 0);
    let mut offset = 0;
    while offset < bytes.len() {
        let read = file.read(&mut bytes[offset..]).map_err(|error| {
            NetworkConfigError::StorageBackupAt {
                stage: StorageBackupStage::Read,
                error,
            }
        })?;
        if read == 0 {
            return Err(NetworkConfigError::BackupInvalid);
        }
        offset += read;
    }
    let config = network_config::decode_external_backup(&bytes, device_id, kind)
        .ok_or(NetworkConfigError::BackupInvalid)?;
    if !valid_config_bytes(config) {
        return Err(NetworkConfigError::BackupInvalid);
    }
    Ok(Some(config.to_vec()))
}

fn recover_invalid_config(
    path: &CString,
    storage_prefix: Option<&str>,
) -> Result<(), NetworkConfigError> {
    let primary = read_primary_for_recovery(path)?;
    if primary.as_deref().is_some_and(valid_config_bytes) {
        // A good primary always wins. Leave both recovery copies untouched.
        return Ok(());
    }

    let mut saw_invalid_backup = false;
    let mut saw_restore_failure = false;
    let mut saw_verification_failure = false;
    for name in [CURRENT_BACKUP_FILE_NAME, BACKUP_FILE_NAME] {
        let backup = match crate::platform::settings::title_data_file_path(name) {
            Ok(backup) => backup,
            Err(error) => {
                ogc_system::report(format_args!(
                    "HBC: could not resolve {name} recovery path ({error:?})\n"
                ));
                continue;
            }
        };
        let bytes = match nand::read_file_wii(&backup, CONFIG_LEN) {
            Ok(bytes) => bytes,
            Err(SettingsIoError::InvalidFileSize) => {
                saw_invalid_backup = true;
                continue;
            }
            Err(error) => {
                ogc_system::report(format_args!(
                    "HBC: could not read {name} recovery backup ({error:?})\n"
                ));
                continue;
            }
        };
        match try_restore_backup(path, bytes) {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(NetworkConfigError::BackupInvalid) => saw_invalid_backup = true,
            Err(NetworkConfigError::RestoreFailed) => saw_restore_failure = true,
            Err(NetworkConfigError::VerificationFailed) => saw_verification_failure = true,
            Err(error) => ogc_system::report(format_args!(
                "HBC: local network recovery attempt failed ({error:?})\n"
            )),
        }
    }

    if let Some(storage_prefix) = storage_prefix {
        match wii_device_id() {
            Ok(device_id) => {
                for (kind, candidate_path) in [
                    (
                        network_config::CURRENT_BACKUP_KIND,
                        current_backup_path(storage_prefix, device_id),
                    ),
                    (
                        network_config::ORIGINAL_BACKUP_KIND,
                        backup_path(storage_prefix, device_id),
                    ),
                ] {
                    for backup_path in [
                        candidate_path,
                        legacy_storage_backup_path(storage_prefix, kind),
                    ] {
                        let backup_path = match backup_path {
                            Ok(path) => path,
                            Err(error) => {
                                ogc_system::report(format_args!(
                                    "HBC: could not resolve removable recovery path ({error:?})\n"
                                ));
                                continue;
                            }
                        };
                        let bytes = match read_storage_backup(&backup_path, device_id, kind) {
                            Ok(bytes) => bytes,
                            Err(NetworkConfigError::BackupInvalid) => {
                                saw_invalid_backup = true;
                                continue;
                            }
                            Err(error) => {
                                ogc_system::report(format_args!(
                                    "HBC: could not read removable recovery backup ({error:?})\n"
                                ));
                                continue;
                            }
                        };
                        match try_restore_backup(path, bytes) {
                            Ok(true) => return Ok(()),
                            Ok(false) => {}
                            Err(NetworkConfigError::BackupInvalid) => saw_invalid_backup = true,
                            Err(NetworkConfigError::RestoreFailed) => saw_restore_failure = true,
                            Err(NetworkConfigError::VerificationFailed) => {
                                saw_verification_failure = true
                            }
                            Err(error) => ogc_system::report(format_args!(
                                "HBC: removable network recovery attempt failed ({error:?})\n"
                            )),
                        }
                    }
                }
            }
            Err(error) => ogc_system::report(format_args!(
                "HBC: Wii identity unavailable for removable recovery ({error:?})\n"
            )),
        }
    }

    if saw_restore_failure {
        Err(NetworkConfigError::RestoreFailed)
    } else if saw_verification_failure {
        Err(NetworkConfigError::VerificationFailed)
    } else if saw_invalid_backup {
        Err(NetworkConfigError::BackupInvalid)
    } else {
        Err(NetworkConfigError::BackupUnavailable)
    }
}

#[cfg(target_arch = "powerpc")]
fn try_restore_backup(
    path: &CStr,
    backup: Option<alloc::vec::Vec<u8>>,
) -> Result<bool, NetworkConfigError> {
    if backup
        .as_deref()
        .is_some_and(|bytes| !valid_config_bytes(bytes))
    {
        return Err(NetworkConfigError::BackupInvalid);
    }
    match recover_from_backup(path, backup) {
        Ok(restored) => Ok(restored),
        Err(error) => Err(error),
    }
}

#[cfg(target_arch = "powerpc")]
fn recover_from_backup(
    path: &CStr,
    backup: Option<alloc::vec::Vec<u8>>,
) -> Result<bool, NetworkConfigError> {
    network_config::recover_from_retained_backup(
        backup,
        valid_config_bytes,
        |bytes| write_existing_config(path, bytes),
        || read_primary_for_recovery(path),
    )
    .map_err(|error| match error {
        network_config::RetainedBackupRecoveryError::InvalidBackup => {
            NetworkConfigError::BackupInvalid
        }
        network_config::RetainedBackupRecoveryError::PrimaryRead(error) => error,
        network_config::RetainedBackupRecoveryError::Restore(error) => {
            ogc_system::report(format_args!(
                "HBC: network backup restore failed ({error:?})\n"
            ));
            NetworkConfigError::RestoreFailed
        }
        network_config::RetainedBackupRecoveryError::Verify(error) => {
            ogc_system::report(format_args!(
                "HBC: restored network config could not be read ({error:?})\n"
            ));
            NetworkConfigError::VerificationFailed
        }
        network_config::RetainedBackupRecoveryError::Mismatch => {
            NetworkConfigError::VerificationFailed
        }
    })
}

fn read_raw_config(path: &CStr) -> Result<Option<alloc::vec::Vec<u8>>, NetworkConfigError> {
    let path = CString::new(path.to_bytes()).map_err(|_| NetworkConfigError::InvalidPath)?;
    nand::read_file_wii(&path, CONFIG_LEN).map_err(NetworkConfigError::Recovery)
}

/// Missing or malformed-length NAND data is recoverable. Other IOS errors
/// stop recovery because they do not prove that the primary is invalid.
fn read_primary_for_recovery(
    path: &CStr,
) -> Result<Option<alloc::vec::Vec<u8>>, NetworkConfigError> {
    let path = CString::new(path.to_bytes()).map_err(|_| NetworkConfigError::InvalidPath)?;
    match nand::read_file_wii(&path, CONFIG_LEN) {
        Ok(bytes) => Ok(bytes),
        Err(SettingsIoError::Missing | SettingsIoError::InvalidFileSize) => Ok(None),
        Err(error) => Err(NetworkConfigError::Recovery(error)),
    }
}

#[cfg(target_arch = "powerpc")]
fn write_existing_config(path: &CStr, bytes: &[u8]) -> Result<(), NetworkConfigError> {
    if bytes.len() != CONFIG_LEN {
        return Err(NetworkConfigError::InvalidFileSize);
    }
    let mut aligned = AlignedWriteConfig([0; CONFIG_LEN]);
    aligned.0.copy_from_slice(bytes);
    let file = isfs::open(path, isfs::ISFS_OPEN_WRITE).map_err(NetworkConfigError::Isfs)?;
    let write_result = file.write_all(&aligned.0);
    let close_result = file.close();
    match (write_result, close_result) {
        (Ok(()), 0..=i32::MAX) => Ok(()),
        (Err(write), close) if close < 0 => Err(NetworkConfigError::WriteAndClose { write, close }),
        (Err(write), _) => Err(NetworkConfigError::Isfs(write)),
        (Ok(()), close) => Err(NetworkConfigError::Isfs(close)),
    }
}

fn valid_config_bytes(bytes: &[u8]) -> bool {
    let Ok(array) = <&[u8; CONFIG_LEN]>::try_from(bytes) else {
        return false;
    };
    NetworkConfig::from_bytes(*array).validate_format().is_ok()
}

fn nand_path() -> Result<CString, NetworkConfigError> {
    CString::new(NAND_PATH.to_bytes()).map_err(|_| NetworkConfigError::InvalidPath)
}

fn initialize_isfs() -> Result<(), NetworkConfigError> {
    let result = isfs::initialize();
    if result < 0 {
        Err(NetworkConfigError::Isfs(result))
    } else {
        Ok(())
    }
}

fn read_all(file: &isfs::IsfsFile, output: &mut [u8]) -> Result<(), NetworkConfigError> {
    nand::read_exact_wii(file, output).map_err(NetworkConfigError::Isfs)
}

fn close_after_error<T>(file: isfs::IsfsFile, error: i32) -> Result<T, NetworkConfigError> {
    let close_result = file.close();
    if close_result < 0 {
        Err(NetworkConfigError::Isfs(close_result))
    } else {
        Err(NetworkConfigError::Isfs(error))
    }
}

#[cfg(test)]
mod save_message_tests {
    use super::*;

    #[test]
    fn isfs_101_is_reported_as_a_file_operation_error() {
        let error = NetworkConfigError::Isfs(isfs::ISFS_EINVAL);
        let message = save_error_message(error);
        assert!(message.contains("ISFS -101"));
        assert!(message.contains("password passed local validation"));
    }

    #[test]
    fn isfs_102_names_the_access_denial_and_keeps_the_operation() {
        let error =
            NetworkConfigError::Recovery(crate::platform::settings::SettingsIoError::Isfs(-102));
        let message = save_error_message(error);
        assert!(message.contains("denied access"));
        assert!(message.contains("ISFS -102"));
        assert!(message.contains("Recovery(Isfs(-102))"));
    }
}
