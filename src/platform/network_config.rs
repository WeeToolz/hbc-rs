//! Read and safely replace the Wii system network configuration on NAND.

use alloc::ffi::CString;
use core::{ffi::CStr, mem::MaybeUninit};

use crate::network_config::{NetworkConfig, CONFIG_LEN};
use crate::platform::{nand, settings::SettingsIoError};
use ogc_filesystem as isfs;

const NAND_PATH: &CStr = c"/shared2/sys/net/02/config.dat";

#[repr(align(32))]
struct AlignedConfig([u8; CONFIG_LEN]);

#[repr(align(32))]
struct AlignedFstats(isfs::Fstats);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkConfigError {
    Isfs(i32),
    ReadAndClose { read: i32, close: i32 },
    InvalidPath,
    InvalidFileSize,
    Recovery(SettingsIoError),
    InvalidWifi(crate::network_config::WifiValidationError),
}

/// Load the current Wii network profiles without changing the saved bytes.
pub fn load() -> Result<NetworkConfig, NetworkConfigError> {
    initialize_isfs()?;
    let path = nand_path()?;
    nand::recover_wii(&path, CONFIG_LEN, |bytes| bytes.len() == CONFIG_LEN)
        .map_err(NetworkConfigError::Recovery)?;

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
    Ok(NetworkConfig::from_bytes(config))
}

/// Replace the system network config after validating the WiFi fields.
/// Unknown and unrelated profile bytes remain unchanged in `config`.
pub fn save(config: &NetworkConfig) -> Result<(), NetworkConfigError> {
    crate::network_config::validate_wifi(
        config
            .profile(0)
            .ok_or(NetworkConfigError::InvalidFileSize)?
            .wifi(),
    )
    .map_err(NetworkConfigError::InvalidWifi)?;

    initialize_isfs()?;
    let path = nand_path()?;
    nand::recover_wii(&path, CONFIG_LEN, |bytes| bytes.len() == CONFIG_LEN)
        .map_err(NetworkConfigError::Recovery)?;
    nand::save_wii(&path, config.as_bytes()).map_err(NetworkConfigError::Recovery)
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
