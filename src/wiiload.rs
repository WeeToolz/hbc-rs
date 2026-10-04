//! Host-testable decoding and bounds checks for the HBC Wiiload TCP header.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use miniz_oxide::inflate::decompress_to_vec_zlib_with_limit;

pub const WIILOAD_HEADER_LEN: usize = 16;
pub const WIILOAD_MIN_VERSION: u16 = 0x0005;
pub const WIILOAD_MAX_ARGS: usize = 1024;
/// This matches HBC's launch-memory bound: `0x81330000 - 1 - args - 0x80003400`.
pub const WIILOAD_MAX_DATA: usize = 0x0132_c7ff;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ReceiverStatus {
    Stopped = 0,
    Starting = 1,
    NetworkUnavailable = 2,
    Listening = 3,
    Receiving = 4,
    PayloadReady = 5,
    InvalidUpload = 6,
    ListenerError = 7,
    MemoryError = 8,
    CleanupError = 9,
}

/// Clear worker ownership only after the join confirms thread completion.
#[cfg_attr(not(any(target_arch = "powerpc", test)), allow(dead_code))]
pub(crate) fn complete_worker_join(
    started: &AtomicBool,
    result: Result<(), i32>,
) -> Result<(), i32> {
    result?;
    started.store(false, Ordering::Release);
    Ok(())
}

#[cfg_attr(not(any(target_arch = "powerpc", test)), allow(dead_code))]
pub(crate) fn remember_cleanup_error(slot: &AtomicI32, error: i32) {
    if error >= 0 {
        return;
    }
    loop {
        let current = slot.load(Ordering::Acquire);
        if current < 0 {
            return;
        }
        match slot.compare_exchange(current, error, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return,
            Err(_) => continue,
        }
    }
}

#[cfg_attr(not(any(target_arch = "powerpc", test)), allow(dead_code))]
pub(crate) fn first_cleanup_failure(results: &[i32]) -> Option<i32> {
    results.iter().copied().find(|result| *result < 0)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndicatorState {
    pub network_active: bool,
    pub gecko_active: bool,
}

pub fn indicator_state(status: ReceiverStatus, gecko_ready: bool) -> IndicatorState {
    IndicatorState {
        network_active: matches!(
            status,
            ReceiverStatus::Listening
                | ReceiverStatus::Receiving
                | ReceiverStatus::PayloadReady
                | ReceiverStatus::InvalidUpload
                | ReceiverStatus::MemoryError
        ),
        gecko_active: gecko_ready,
    }
}

/// HBC accepts Wiiload clients from the same IPv4 `/16` as the console.
pub fn same_local_subnet_16(peer_ip: u32, local_ip: u32) -> bool {
    (peer_ip & 0xffff_0000) == (local_ip & 0xffff_0000)
}

/// Convert completed and total byte counts to a bounded display percentage.
pub fn progress_percent(completed: u32, total: u32) -> u8 {
    if total == 0 {
        return 0;
    }
    ((u64::from(completed.min(total)) * 100) / u64::from(total)) as u8
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WiiloadError {
    InvalidHeader,
    UnsupportedVersion,
    ArgumentsTooLarge,
    EmptyPayload,
    PayloadTooLarge,
    LengthOverflow,
    IncompleteBody,
    DecompressionFailed,
    DecompressedSizeMismatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WiiloadHeader {
    pub version: u16,
    pub args_len: u16,
    pub data_len: u32,
    /// Zero or a value equal to `data_len` means that the payload is not compressed.
    pub data_len_uncompressed: u32,
}

impl WiiloadHeader {
    pub fn decode(bytes: &[u8]) -> Result<Self, WiiloadError> {
        if bytes.len() != WIILOAD_HEADER_LEN || &bytes[..4] != b"HAXX" {
            return Err(WiiloadError::InvalidHeader);
        }
        let header = Self {
            version: u16::from_be_bytes([bytes[4], bytes[5]]),
            args_len: u16::from_be_bytes([bytes[6], bytes[7]]),
            data_len: u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            data_len_uncompressed: u32::from_be_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
        };
        header.validate()?;
        Ok(header)
    }

    pub fn validate(&self) -> Result<(), WiiloadError> {
        if self.version < WIILOAD_MIN_VERSION {
            return Err(WiiloadError::UnsupportedVersion);
        }
        if self.args_len as usize > WIILOAD_MAX_ARGS {
            return Err(WiiloadError::ArgumentsTooLarge);
        }
        if self.data_len == 0 {
            return Err(WiiloadError::EmptyPayload);
        }
        if self.data_len as usize > WIILOAD_MAX_DATA
            || self.data_len_uncompressed as usize > WIILOAD_MAX_DATA
        {
            return Err(WiiloadError::PayloadTooLarge);
        }
        self.data_len
            .checked_add(self.args_len as u32)
            .ok_or(WiiloadError::LengthOverflow)?;
        Ok(())
    }

    pub fn body_len(&self) -> Result<usize, WiiloadError> {
        self.validate()?;
        (self.data_len as usize)
            .checked_add(self.args_len as usize)
            .ok_or(WiiloadError::LengthOverflow)
    }

    pub fn is_compressed(&self) -> bool {
        self.data_len_uncompressed != 0 && self.data_len_uncompressed != self.data_len
    }

    /// Validate a complete wire body, decompress its data when needed, and split off arguments.
    pub fn decode_body(&self, body: &[u8]) -> Result<WiiloadPayload, WiiloadError> {
        let expected_body_len = self.body_len()?;
        if body.len() != expected_body_len {
            return Err(WiiloadError::IncompleteBody);
        }
        let wire_data_len = self.data_len as usize;
        let wire_data = &body[..wire_data_len];
        let args = body[wire_data_len..].to_vec();
        let data = if self.is_compressed() {
            let expected = self.data_len_uncompressed as usize;
            let decoded = decompress_to_vec_zlib_with_limit(wire_data, expected)
                .map_err(|_| WiiloadError::DecompressionFailed)?;
            if decoded.len() != expected {
                return Err(WiiloadError::DecompressedSizeMismatch);
            }
            decoded
        } else {
            wire_data.to_vec()
        };
        Ok(WiiloadPayload { data, args })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WiiloadPayload {
    pub data: Vec<u8>,
    pub args: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::AtomicBool;

    #[test]
    fn failed_worker_join_keeps_worker_marked_as_started() {
        let started = AtomicBool::new(true);
        assert_eq!(complete_worker_join(&started, Err(-8)), Err(-8));
        assert!(started.load(Ordering::Acquire));
    }

    #[test]
    fn successful_worker_join_clears_worker_started_state() {
        let started = AtomicBool::new(true);
        assert_eq!(complete_worker_join(&started, Ok(())), Ok(()));
        assert!(!started.load(Ordering::Acquire));
    }

    #[test]
    fn remembers_the_first_cleanup_failure() {
        let error = AtomicI32::new(0);
        remember_cleanup_error(&error, -9);
        remember_cleanup_error(&error, -11);
        remember_cleanup_error(&error, 0);
        assert_eq!(error.load(Ordering::Acquire), -9);
    }

    #[test]
    fn reports_socket_or_network_cleanup_failures() {
        assert_eq!(first_cleanup_failure(&[0, 0]), None);
        assert_eq!(first_cleanup_failure(&[-9, 0]), Some(-9));
        assert_eq!(first_cleanup_failure(&[0, -11]), Some(-11));
        assert_eq!(first_cleanup_failure(&[-9, -11]), Some(-9));
    }

    #[test]
    fn maps_receiver_status_and_gecko_readiness_to_indicators() {
        for status in [
            ReceiverStatus::Stopped,
            ReceiverStatus::Starting,
            ReceiverStatus::NetworkUnavailable,
            ReceiverStatus::ListenerError,
            ReceiverStatus::CleanupError,
        ] {
            assert!(!indicator_state(status, false).network_active);
        }
        for status in [
            ReceiverStatus::Listening,
            ReceiverStatus::Receiving,
            ReceiverStatus::PayloadReady,
            ReceiverStatus::InvalidUpload,
            ReceiverStatus::MemoryError,
        ] {
            assert!(indicator_state(status, false).network_active);
        }
        assert!(!indicator_state(ReceiverStatus::Stopped, false).gecko_active);
        assert!(indicator_state(ReceiverStatus::Listening, true).gecko_active);
    }
    use miniz_oxide::deflate::compress_to_vec_zlib;

    fn header(version: u16, args_len: u16, data_len: u32, data_un: u32) -> [u8; 16] {
        let mut bytes = [0; 16];
        bytes[..4].copy_from_slice(b"HAXX");
        bytes[4..6].copy_from_slice(&version.to_be_bytes());
        bytes[6..8].copy_from_slice(&args_len.to_be_bytes());
        bytes[8..12].copy_from_slice(&data_len.to_be_bytes());
        bytes[12..16].copy_from_slice(&data_un.to_be_bytes());
        bytes
    }

    #[test]
    fn decodes_big_endian_header_and_accepts_minimum_version() {
        let decoded = WiiloadHeader::decode(&header(5, 3, 4, 0)).unwrap();
        assert_eq!(decoded.args_len, 3);
        assert_eq!(decoded.body_len(), Ok(7));
        assert!(!decoded.is_compressed());
    }

    #[test]
    fn rejects_bad_magic_length_version_and_wire_bounds() {
        let mut bad_magic = header(5, 0, 1, 0);
        bad_magic[0] = b'X';
        assert_eq!(
            WiiloadHeader::decode(&bad_magic),
            Err(WiiloadError::InvalidHeader)
        );
        assert_eq!(
            WiiloadHeader::decode(&[0; 15]),
            Err(WiiloadError::InvalidHeader)
        );
        assert_eq!(
            WiiloadHeader::decode(&header(4, 0, 1, 0)),
            Err(WiiloadError::UnsupportedVersion)
        );
        assert_eq!(
            WiiloadHeader::decode(&header(5, 1025, 1, 0)),
            Err(WiiloadError::ArgumentsTooLarge)
        );
        assert_eq!(
            WiiloadHeader::decode(&header(5, 0, 0, 0)),
            Err(WiiloadError::EmptyPayload)
        );
        assert_eq!(
            WiiloadHeader::decode(&header(5, 0, WIILOAD_MAX_DATA as u32 + 1, 0)),
            Err(WiiloadError::PayloadTooLarge)
        );
        assert_eq!(
            WiiloadHeader::decode(&header(5, 0, 1, WIILOAD_MAX_DATA as u32 + 1)),
            Err(WiiloadError::PayloadTooLarge)
        );
    }

    #[test]
    fn payload_limit_matches_hbc_launch_buffer() {
        assert_eq!(
            WIILOAD_MAX_DATA,
            (0x8133_0000usize - 1 - WIILOAD_MAX_ARGS) - 0x8000_3400usize
        );
    }

    #[test]
    fn validates_body_length_and_splits_arguments_after_data() {
        let h = WiiloadHeader::decode(&header(5, 3, 4, 0)).unwrap();
        assert_eq!(h.decode_body(b"abc"), Err(WiiloadError::IncompleteBody));
        assert_eq!(
            h.decode_body(b"abcdezzz"),
            Err(WiiloadError::IncompleteBody)
        );
        assert_eq!(
            h.decode_body(b"ELF!arg"),
            Ok(WiiloadPayload {
                data: b"ELF!".to_vec(),
                args: b"arg".to_vec(),
            })
        );
    }

    #[test]
    fn bounded_zlib_decode_requires_exact_advertised_output_size() {
        let raw = b"a small Wii executable payload";
        let compressed = compress_to_vec_zlib(raw, 6);
        let h = WiiloadHeader::decode(&header(5, 2, compressed.len() as u32, raw.len() as u32))
            .unwrap();
        let mut body = compressed.clone();
        body.extend_from_slice(b"xy");
        assert_eq!(
            h.decode_body(&body),
            Ok(WiiloadPayload {
                data: raw.to_vec(),
                args: b"xy".to_vec()
            })
        );

        let wrong_size =
            WiiloadHeader::decode(&header(5, 0, compressed.len() as u32, raw.len() as u32 + 1))
                .unwrap();
        assert_eq!(
            wrong_size.decode_body(&compressed),
            Err(WiiloadError::DecompressedSizeMismatch)
        );

        let malformed = WiiloadHeader::decode(&header(5, 0, 3, 20)).unwrap();
        assert_eq!(
            malformed.decode_body(b"bad"),
            Err(WiiloadError::DecompressionFailed)
        );
    }

    #[test]
    fn peer_filter_accepts_only_the_local_ipv4_slash_16() {
        let local = u32::from_be_bytes([192, 168, 4, 2]);
        assert!(same_local_subnet_16(
            u32::from_be_bytes([192, 168, 4, 200]),
            local
        ));
        assert!(!same_local_subnet_16(
            u32::from_be_bytes([192, 169, 5, 200]),
            local
        ));
        assert!(!same_local_subnet_16(
            u32::from_be_bytes([10, 0, 4, 200]),
            local
        ));
    }

    #[test]
    fn install_progress_percentage_is_bounded_and_handles_empty_totals() {
        assert_eq!(progress_percent(0, 0), 0);
        assert_eq!(progress_percent(0, 100), 0);
        assert_eq!(progress_percent(37, 100), 37);
        assert_eq!(progress_percent(200, 100), 100);
    }
}
