//! Access point scan through the Wii wireless driver.

use crate::network_config::ScannedNetworks;
use crate::network_scan::parse_scan_results;

const SCAN_BUFFER_SIZE: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScanError {
    Driver(ogc_wd::ScanFailure),
    InvalidDescriptor,
}

impl ScanError {
    pub fn user_message(self) -> alloc::string::String {
        match self {
            Self::Driver(failure) if failure.code == ogc_ipc::IPC_ENOENT => alloc::format!(
                "IOS could not find the Wii wireless-driver service during {}. The network scan could not start. Error: {}.",
                stage_name(failure.stage),
                ogc_wd::ErrorCode::new(failure.code)
            ),
            Self::Driver(failure) => alloc::format!(
                "The Wii wireless-driver scan failed during {}: {}.",
                stage_name(failure.stage),
                ogc_wd::ErrorCode::new(failure.code)
            ),
            Self::InvalidDescriptor => alloc::string::String::from(
                "The Wii wireless driver returned an invalid scan result.",
            ),
        }
    }
}

fn stage_name(stage: ogc_wd::ScanStage) -> &'static str {
    match stage {
        ogc_wd::ScanStage::Platform => "platform check",
        ogc_wd::ScanStage::NetworkDriverLock => "network-driver lock",
        ogc_wd::ScanStage::DriverOpen => "wireless-driver open (/dev/net/wd/command)",
        ogc_wd::ScanStage::Scan => "wireless scan",
        ogc_wd::ScanStage::DriverClose => "wireless-driver close",
        ogc_wd::ScanStage::NetworkDriverUnlock => "network-driver unlock",
    }
}

/// Scan nearby access points and return validated SSIDs and security modes.
/// The caller must stop the active socket receiver before calling this API.
pub fn scan() -> Result<ScannedNetworks, ScanError> {
    let mut params = ogc_wd::default_scan_parameters();
    let mut bytes = alloc::vec![0; SCAN_BUFFER_SIZE];
    ogc_wd::scan_once_detailed_result(&mut params, &mut bytes).map_err(ScanError::Driver)?;
    parse_scan_results(&bytes).map_err(|_| ScanError::InvalidDescriptor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_wireless_driver_error_explains_the_failed_service() {
        let message = ScanError::Driver(ogc_wd::ScanFailure {
            stage: ogc_wd::ScanStage::DriverOpen,
            code: ogc_ipc::IPC_ENOENT,
        })
        .user_message();
        assert!(message.contains("/dev/net/wd/command"));
        assert!(message.contains("-6"));
    }
}
