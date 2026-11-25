//! Host-testable decoding and bounds checks for the HBC Wiiload TCP header.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicI32, Ordering};

const WIILOAD_STREAM_CHUNK: usize = 1024;

#[derive(Debug)]
pub(crate) enum StreamDecodeError<E> {
    Source(E),
    Allocation,
    SizeMismatch,
    Invalid,
}

/// Decode a zlib payload while the caller supplies bounded input chunks.
/// The output allocation is the only payload-sized allocation.
pub(crate) fn decode_zlib_stream<E>(
    output_len: usize,
    input_len: usize,
    mut read_chunk: impl FnMut(&mut [u8]) -> Result<(), E>,
) -> Result<Vec<u8>, StreamDecodeError<E>> {
    use miniz_oxide::inflate::{
        TINFLStatus,
        core::{DecompressorOxide, decompress, inflate_flags},
    };

    let output_capacity = output_len
        .checked_add(1)
        .ok_or(StreamDecodeError::Allocation)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(output_capacity)
        .map_err(|_| StreamDecodeError::Allocation)?;
    output.resize(output_capacity, 0);
    let mut decompressor = Vec::new();
    decompressor
        .try_reserve_exact(1)
        .map_err(|_| StreamDecodeError::Allocation)?;
    decompressor.push(DecompressorOxide::new());
    let mut input = [0; WIILOAD_STREAM_CHUNK];
    let mut input_remaining = input_len;
    let mut output_written = 0;
    let mut stream_done = false;

    while input_remaining > 0 {
        let chunk_len = input_remaining.min(input.len());
        read_chunk(&mut input[..chunk_len]).map_err(StreamDecodeError::Source)?;
        input_remaining -= chunk_len;
        if stream_done {
            continue;
        }

        let mut flags = inflate_flags::TINFL_FLAG_PARSE_ZLIB_HEADER
            | inflate_flags::TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF;
        if input_remaining > 0 {
            flags |= inflate_flags::TINFL_FLAG_HAS_MORE_INPUT;
        }
        let (status, consumed, written) = decompress(
            &mut decompressor[0],
            &input[..chunk_len],
            &mut output,
            output_written,
            flags,
        );
        output_written = output_written
            .checked_add(written)
            .ok_or(StreamDecodeError::Invalid)?;
        if output_written > output_len {
            return Err(StreamDecodeError::Invalid);
        }
        match status {
            TINFLStatus::Done => stream_done = true,
            TINFLStatus::NeedsMoreInput if consumed == chunk_len => {}
            _ => return Err(StreamDecodeError::Invalid),
        }
    }

    if !stream_done {
        return Err(StreamDecodeError::Invalid);
    }
    if output_written != output_len {
        return Err(StreamDecodeError::SizeMismatch);
    }
    output.truncate(output_len);
    Ok(output)
}

pub const WIILOAD_HEADER_LEN: usize = 16;
pub const REBORN_UPLOAD_NAME_CAPACITY: usize = 63;
pub const REBORN_UPLOAD_METADATA_TTL_MS: u64 = 120_000;
pub const REBORN_UPLOAD_INSTALL_YES: u16 = 1;
pub const WIILOAD_TCP_PORT: u16 = 4299;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RebornTransferStyle {
    None = 0,
    Scroll = 1,
    Bounce = 2,
    Truncate = 3,
}

impl RebornTransferStyle {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Scroll => "scroll",
            Self::Bounce => "bounce",
            Self::Truncate => "truncate",
        }
    }

    pub const fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Scroll,
            2 => Self::Bounce,
            3 => Self::Truncate,
            _ => Self::None,
        }
    }

    /// Decode the twelve style bytes in an HBCT header as Reborn's C string.
    pub fn decode_hbct(header: &[u8; WIILOAD_HEADER_LEN]) -> Option<Self> {
        if &header[..4] != b"HBCT" {
            return None;
        }
        let bytes = &header[4..];
        let end = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len());
        match &bytes[..end] {
            b"scroll" => Some(Self::Scroll),
            b"bounce" => Some(Self::Bounce),
            b"truncate" => Some(Self::Truncate),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RebornTransferLine {
    pub message: alloc::string::String,
    pub percent: alloc::string::String,
    pub split_percent: bool,
}

pub struct RebornTransferProgress<'a> {
    pub operation: u8,
    pub name: &'a str,
    pub completed: u32,
    pub total: u32,
    pub style: RebornTransferStyle,
    pub elapsed_ms: u32,
    pub max_chars: usize,
    pub receiving: &'a str,
    pub sending: &'a str,
}

/// Format Reborn's transfer footer with a bounded name window.
pub fn reborn_transfer_line(progress: RebornTransferProgress<'_>) -> Option<RebornTransferLine> {
    use alloc::format;
    use alloc::string::String;

    let RebornTransferProgress {
        operation,
        name,
        completed,
        total,
        style,
        elapsed_ms,
        max_chars,
        receiving,
        sending,
    } = progress;
    let verb = match operation {
        b'p' | b'P' => receiving,
        b'g' | b'G' => sending,
        _ => return None,
    };
    let name: String = name.chars().take(63).collect();
    let percent = if total == 0 {
        String::new()
    } else {
        format!(
            "  {}%",
            (u64::from(completed) * 100 / u64::from(total)).min(100)
        )
    };
    let chars: Vec<char> = name.chars().collect();
    let room = max_chars
        .saturating_sub(verb.chars().count() + 1 + percent.chars().count())
        .max(1);
    if chars.len() <= room {
        return Some(RebornTransferLine {
            message: format!("{verb} {name}{percent}"),
            percent: String::new(),
            split_percent: false,
        });
    }
    let window = match style {
        RebornTransferStyle::Truncate => {
            let visible = room.saturating_sub(3);
            let mut text: String = chars.iter().take(visible).collect();
            text.push_str("...");
            text
        }
        RebornTransferStyle::Scroll => {
            let offset = if elapsed_ms < 1000 {
                0
            } else {
                ((elapsed_ms - 1000) / 133) as usize % (chars.len() + 3)
            };
            (0..room)
                .map(|index| {
                    let index = (offset + index) % (chars.len() + 3);
                    if index < chars.len() {
                        chars[index]
                    } else {
                        ' '
                    }
                })
                .collect()
        }
        RebornTransferStyle::Bounce | RebornTransferStyle::None => {
            let span = chars.len().saturating_sub(room);
            let period = 2 * (1000 + span as u32 * 133);
            let phase = if period == 0 { 0 } else { elapsed_ms % period };
            let offset = if phase < 1000 {
                0
            } else if phase < 1000 + span as u32 * 133 {
                ((phase - 1000) / 133) as usize
            } else if phase < 2000 + span as u32 * 133 {
                span
            } else {
                span.saturating_sub(((phase - 2000 - span as u32 * 133) / 133) as usize)
            };
            chars.iter().skip(offset).take(room).collect()
        }
    };
    Some(RebornTransferLine {
        message: format!("{verb} {window}"),
        percent,
        split_percent: total != 0,
    })
}
/// The network-debugger release worker and compressed-upload call chain require
/// at least 36,320 bytes on PowerPC. Use 64 KiB in that mode, leaving more than
/// 27 KiB for deeper calls, thread entry, and interrupt frames. The other modes
/// retain their 32 KiB budget (including miniz's roughly 10.5 KiB decoder).
pub const WIILOAD_WORKER_STACK_BYTES: usize = if cfg!(feature = "debugger-network") {
    64 * 1024
} else {
    32 * 1024
};
const _: () = assert!(
    WIILOAD_WORKER_STACK_BYTES
        >= 2 * core::mem::size_of::<miniz_oxide::inflate::core::DecompressorOxide>() + 8 * 1024
);
pub const WIILOAD_MIN_VERSION: u16 = 0x0005;
pub const WIILOAD_MAX_ARGS: usize = 1024;
/// This matches HBC's launch-memory bound: `0x81330000 - 1 - args - 0x80003400`.
pub const WIILOAD_MAX_DATA: usize = 0x0132_c7ff;
/// Maximum time allowed for one TCP upload, including the header and body.
pub const WIILOAD_CLIENT_TIMEOUT_TICKS: u64 = 120 * 60_750_000;

/// IOS socket errors use a different namespace from the network init API.
/// IOS SO_EAGAIN is 6; network initialization uses POSIX EAGAIN (11).
pub fn socket_would_block(result: i32) -> bool {
    result == -6
}

#[cfg_attr(not(any(target_arch = "powerpc", test)), allow(dead_code))]
pub(crate) fn try_claim_payload(lock: &AtomicBool) -> bool {
    lock.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_ok()
}

/// A higher-priority consumer must never spin while a producer owns the slot.
#[cfg_attr(not(any(target_arch = "powerpc", test)), allow(dead_code))]
pub(crate) fn try_with_payload<R>(lock: &AtomicBool, operation: impl FnOnce() -> R) -> Option<R> {
    if !try_claim_payload(lock) {
        return None;
    }
    struct Guard<'a>(&'a AtomicBool);
    impl Drop for Guard<'_> {
        fn drop(&mut self) {
            self.0.store(false, Ordering::Release);
        }
    }
    let _guard = Guard(lock);
    Some(operation())
}

/// Wait for IOS network initialization without treating a pending DHCP request
/// as a failure. Cancellation remains available on every status poll.
#[cfg_attr(not(any(target_arch = "powerpc", test)), allow(dead_code))]
pub(crate) fn wait_network_initialization(
    mut status: impl FnMut() -> i32,
    mut stopped: impl FnMut() -> bool,
    mut wait: impl FnMut(),
) -> Option<i32> {
    loop {
        if stopped() {
            return None;
        }
        let result = status();
        if result != -16 {
            return Some(result);
        }
        wait();
    }
}

/// Return true when the elapsed time reaches a fixed client-wide deadline.
/// Wrapping subtraction keeps this valid across the timebase counter rollover.
pub fn client_deadline_expired(started_at: u64, now: u64, timeout_ticks: u64) -> bool {
    now.wrapping_sub(started_at) >= timeout_ticks
}

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

pub fn elapsed_ticks_to_milliseconds(
    started_at: u64,
    finished_at: u64,
    ticks_per_second: u64,
) -> u32 {
    if ticks_per_second == 0 {
        return 0;
    }
    let elapsed = finished_at.wrapping_sub(started_at);
    let milliseconds = (elapsed / ticks_per_second)
        .saturating_mul(1000)
        .saturating_add((elapsed % ticks_per_second).saturating_mul(1000) / ticks_per_second);
    milliseconds.min(u32::MAX as u64) as u32
}

pub fn stack_high_water_used_bytes(
    low: usize,
    high: usize,
    mut read_byte: impl FnMut(usize) -> u8,
) -> usize {
    let mut untouched = low;
    while untouched < high && read_byte(untouched) == 0 {
        untouched += 1;
    }
    high.saturating_sub(untouched)
}

/// Live HBCS fields that the Rust receiver and system APIs can report.
/// Storage, startup, and retained-log fields need more production state.
pub(crate) struct RebornStatusSnapshot<'a> {
    pub version: &'a str,
    pub network: &'a str,
    pub ios: i32,
    pub ios_revision: i32,
    pub ahbprot: bool,
    pub arena1_free: u32,
    pub arena2_free: u32,
    pub heap_free: u32,
    pub apps: u32,
    pub device: Option<crate::platform::storage::Volume>,
    pub inserted_mask: u8,
    pub init_ms: u32,
    pub scan_ms: u32,
    pub tcp_stack_size: u32,
    pub tcp_stack_used: u32,
    pub net_starts: u32,
    pub time: u64,
    pub ip: [u8; 4],
    pub completed: u32,
    pub total: u32,
    pub transfer_style: &'a str,
    pub log: Option<crate::netlog::Target>,
}

/// Build the Reborn HBCS JSON from a production status snapshot.
#[cfg_attr(not(target_arch = "powerpc"), allow(dead_code))]
pub(crate) fn reborn_status_json(snapshot: RebornStatusSnapshot<'_>) -> alloc::string::String {
    let RebornStatusSnapshot {
        version,
        network,
        ios,
        ios_revision,
        ahbprot,
        arena1_free,
        arena2_free,
        heap_free,
        apps,
        device,
        inserted_mask,
        init_ms,
        scan_ms,
        tcp_stack_size,
        tcp_stack_used,
        net_starts,
        time,
        ip,
        completed,
        total,
        transfer_style,
        log,
    } = snapshot;
    let [ip_a, ip_b, ip_c, ip_d] = ip;
    let ip = alloc::format!("\"{ip_a}.{ip_b}.{ip_c}.{ip_d}\"");
    let device = device.map_or_else(
        || alloc::string::String::from("null"),
        |volume| alloc::format!("\"{}\"", status_device_name(volume)),
    );
    let log = log.map_or_else(
        || alloc::string::String::from("null"),
        |target| {
            let [a, b, c, d] = target.address;
            alloc::format!("\"{a}.{b}.{c}.{d}:{}\"", target.port)
        },
    );
    let mut inserted = alloc::string::String::from("[");
    let mut first_inserted = true;
    for (index, volume) in crate::platform::storage::VOLUMES.iter().enumerate() {
        if inserted_mask & (1 << index) == 0 {
            continue;
        }
        if !first_inserted {
            inserted.push(',');
        }
        first_inserted = false;
        inserted.push('"');
        inserted.push_str(status_device_name(*volume));
        inserted.push('"');
    }
    inserted.push(']');
    alloc::format!(
        "{{\"version\":\"{version}\",\"proto\":6,\"ios\":{ios},\"ios_revision\":{ios_revision},\"ahbprot\":{ahbprot},\"arena1_free\":{arena1_free},\"arena2_free\":{arena2_free},\"heap_free\":{heap_free},\"tcp_stack_used\":{tcp_stack_used},\"tcp_stack_size\":{tcp_stack_size},\"init_ms\":{init_ms},\"scan_ms\":{scan_ms},\"ip\":{ip},\"net_starts\":{net_starts},\"apps\":{apps},\"device\":{device},\"inserted\":{inserted},\"log\":{log},\"network\":\"{network}\",\"receive\":{{\"completed\":{completed},\"total\":{total}}},\"transfer_style\":\"{transfer_style}\",\"time\":{time}}}"
    )
}

fn status_device_name(volume: crate::platform::storage::Volume) -> &'static str {
    use crate::platform::storage::Volume;
    match volume {
        Volume::Sd => "sd",
        Volume::Usb => "usb",
        Volume::CardA => "carda",
        Volume::CardB => "cardb",
        Volume::Usb2 => "usb2",
    }
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

/// Match C HBC's network status light: it is disabled for the first 20
/// initialization frames, enabled for the next 20, and repeats until ready.
pub fn network_indicator_visible(status: ReceiverStatus, frame: u16) -> bool {
    if status == ReceiverStatus::Starting {
        (frame % 40) >= 20
    } else {
        indicator_state(status, false).network_active
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
    AllocationFailed,
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
        let args = copy_bytes(&body[wire_data_len..])?;
        let data = if self.is_compressed() {
            let expected = self.data_len_uncompressed as usize;
            let mut offset = 0;
            decode_zlib_stream(expected, wire_data.len(), |chunk| {
                let end = offset + chunk.len();
                chunk.copy_from_slice(&wire_data[offset..end]);
                offset = end;
                Ok::<_, core::convert::Infallible>(())
            })
            .map_err(|error| match error {
                StreamDecodeError::Allocation => WiiloadError::AllocationFailed,
                StreamDecodeError::Source(never) => match never {},
                StreamDecodeError::SizeMismatch => WiiloadError::DecompressedSizeMismatch,
                StreamDecodeError::Invalid => WiiloadError::DecompressionFailed,
            })?
        } else {
            copy_bytes(wire_data)?
        };
        Ok(WiiloadPayload { data, args })
    }
}

fn copy_bytes(bytes: &[u8]) -> Result<Vec<u8>, WiiloadError> {
    let mut copy = Vec::new();
    copy.try_reserve_exact(bytes.len())
        .map_err(|_| WiiloadError::AllocationFailed)?;
    copy.extend_from_slice(bytes);
    Ok(copy)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WiiloadPayload {
    pub data: Vec<u8>,
    pub args: Vec<u8>,
}

/// One-shot metadata sent before a Reborn Wiiload transfer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RebornUploadMetadata {
    pub name: [u8; REBORN_UPLOAD_NAME_CAPACITY],
    pub name_len: u8,
    pub flags: u16,
    pub set_at_ms: u64,
}

impl RebornUploadMetadata {
    pub fn name(&self) -> &[u8] {
        &self.name[..usize::from(self.name_len)]
    }

    pub fn is_current(self, now_ms: u64) -> bool {
        self.set_at_ms != 0 && now_ms.saturating_sub(self.set_at_ms) < REBORN_UPLOAD_METADATA_TTL_MS
    }

    pub fn install_without_asking(self) -> bool {
        self.flags & REBORN_UPLOAD_INSTALL_YES != 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RebornUploadMetadataError {
    InvalidHeader,
    InvalidLength,
}

/// Decode the protocol 6 HBCA name and flags body.
pub fn decode_reborn_upload_metadata(
    header: &[u8; WIILOAD_HEADER_LEN],
    body: &[u8],
    set_at_ms: u64,
) -> Result<RebornUploadMetadata, RebornUploadMetadataError> {
    if &header[..4] != b"HBCA" {
        return Err(RebornUploadMetadataError::InvalidHeader);
    }
    let name_len = usize::from(u16::from_be_bytes([header[4], header[5]]));
    let flags = u16::from_be_bytes([header[6], header[7]]);
    if name_len > REBORN_UPLOAD_NAME_CAPACITY || body.len() != name_len {
        return Err(RebornUploadMetadataError::InvalidLength);
    }
    if name_len == 0 && flags == 0 {
        return Err(RebornUploadMetadataError::InvalidLength);
    }
    let mut name = [0; REBORN_UPLOAD_NAME_CAPACITY];
    name[..name_len.min(REBORN_UPLOAD_NAME_CAPACITY)].copy_from_slice(body);
    // Reborn stores this field as a C string. Keep flags even when the name
    // contains an embedded NUL, but expose only the prefix before that byte.
    let name_len = name[..name_len]
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(name_len);
    Ok(RebornUploadMetadata {
        name,
        name_len: name_len as u8,
        flags,
        set_at_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reborn_transfer_line_shows_direction_name_and_bounded_percentage() {
        let line = reborn_transfer_line(RebornTransferProgress {
            operation: b'p',
            name: "boot.dol",
            completed: 42,
            total: 100,
            style: RebornTransferStyle::Scroll,
            elapsed_ms: 0,
            max_chars: 44,
            receiving: "Receiving",
            sending: "Sending",
        })
        .unwrap();
        assert_eq!(line.message, "Receiving boot.dol  42%");
        assert!(!line.split_percent);
        assert_eq!(
            reborn_transfer_line(RebornTransferProgress {
                operation: b'G',
                name: "save.bin",
                completed: 0,
                total: 0,
                style: RebornTransferStyle::None,
                elapsed_ms: 0,
                max_chars: 44,
                receiving: "Receiving",
                sending: "Sending",
            },)
            .unwrap()
            .message,
            "Sending save.bin"
        );
        assert!(
            reborn_transfer_line(RebornTransferProgress {
                operation: b'?',
                name: "x",
                completed: 0,
                total: 0,
                style: RebornTransferStyle::Scroll,
                elapsed_ms: 0,
                max_chars: 44,
                receiving: "Receiving",
                sending: "Sending",
            })
            .is_none()
        );
    }

    #[test]
    fn reborn_transfer_line_applies_scroll_bounce_and_truncate_styles() {
        let line = |style, elapsed| {
            reborn_transfer_line(RebornTransferProgress {
                operation: b'g',
                name: "0123456789ABCDEFGHIJ",
                completed: 50,
                total: 100,
                style,
                elapsed_ms: elapsed,
                max_chars: 21,
                receiving: "Receiving",
                sending: "Sending",
            })
            .unwrap()
        };
        let scroll_start = line(RebornTransferStyle::Scroll, 0);
        let scroll_moved = line(RebornTransferStyle::Scroll, 1133);
        assert_ne!(scroll_start.message, scroll_moved.message);
        let bounce_start = line(RebornTransferStyle::Bounce, 0);
        let bounce_end = line(RebornTransferStyle::Bounce, 1000 + 8 * 133);
        assert_ne!(bounce_start.message, bounce_end.message);
        let truncated = line(RebornTransferStyle::Truncate, 5000);
        assert!(truncated.message.ends_with("..."));
        assert!(truncated.split_percent);
        assert_eq!(truncated.percent, "  50%");
    }

    #[test]
    fn reborn_upload_metadata_checks_length_flags_and_expiry() {
        let mut header = [0; WIILOAD_HEADER_LEN];
        header[..4].copy_from_slice(b"HBCA");
        header[5] = 4;
        header[7] = REBORN_UPLOAD_INSTALL_YES as u8;
        let metadata = decode_reborn_upload_metadata(&header, b"game", 100).unwrap();
        assert_eq!(metadata.name(), b"game");
        assert!(metadata.install_without_asking());
        assert!(metadata.is_current(100 + REBORN_UPLOAD_METADATA_TTL_MS - 1));
        assert!(!metadata.is_current(100 + REBORN_UPLOAD_METADATA_TTL_MS));

        let metadata =
            decode_reborn_upload_metadata(&header_with_name(b"ab\0cd"), b"ab\0cd", 100).unwrap();
        assert_eq!(metadata.name(), b"ab");
        assert!(metadata.install_without_asking());

        header[5] = (REBORN_UPLOAD_NAME_CAPACITY + 1) as u8;
        assert_eq!(
            decode_reborn_upload_metadata(&header, &[0; 64], 100),
            Err(RebornUploadMetadataError::InvalidLength)
        );
        header[5] = 0;
        header[7] = 0;
        assert_eq!(
            decode_reborn_upload_metadata(&header, &[], 100),
            Err(RebornUploadMetadataError::InvalidLength)
        );
    }

    #[test]
    fn hbct_accepts_only_reborn_style_names_from_c_string_field() {
        for (name, expected) in [
            (b"scroll".as_slice(), RebornTransferStyle::Scroll),
            (b"bounce".as_slice(), RebornTransferStyle::Bounce),
            (b"truncate".as_slice(), RebornTransferStyle::Truncate),
        ] {
            let mut header = [0; WIILOAD_HEADER_LEN];
            header[..4].copy_from_slice(b"HBCT");
            header[4..4 + name.len()].copy_from_slice(name);
            assert_eq!(RebornTransferStyle::decode_hbct(&header), Some(expected));
        }
        let mut header = [0; WIILOAD_HEADER_LEN];
        header[..4].copy_from_slice(b"HBCT");
        header[4..10].copy_from_slice(b"scroll");
        header[11] = b'x';
        assert_eq!(
            RebornTransferStyle::decode_hbct(&header),
            Some(RebornTransferStyle::Scroll)
        );
        header[4..10].copy_from_slice(b"SCROLL");
        assert_eq!(RebornTransferStyle::decode_hbct(&header), None);
        header[..4].copy_from_slice(b"HBCX");
        assert_eq!(RebornTransferStyle::decode_hbct(&header), None);
    }

    fn header_with_name(name: &[u8]) -> [u8; WIILOAD_HEADER_LEN] {
        let mut header = [0; WIILOAD_HEADER_LEN];
        header[..4].copy_from_slice(b"HBCA");
        header[4..6].copy_from_slice(&(name.len() as u16).to_be_bytes());
        header[7] = REBORN_UPLOAD_INSTALL_YES as u8;
        header
    }

    #[test]
    fn reborn_status_json_reports_live_receiver_fields_and_zero_ip_when_unavailable() {
        assert_eq!(
            reborn_status_json(RebornStatusSnapshot {
                version: "1.2.3",
                network: "receiving",
                ios: 58,
                ios_revision: 6432,
                ahbprot: true,
                arena1_free: 4096,
                arena2_free: 8192,
                heap_free: 1024,
                apps: 17,
                device: Some(crate::platform::storage::Volume::Usb2),
                inserted_mask: 0b10001,
                init_ms: 2510,
                scan_ms: 82,
                tcp_stack_size: 32768,
                tcp_stack_used: 736,
                net_starts: 2,
                time: 1790100000,
                ip: [192, 168, 1, 20],
                completed: 31,
                total: 64,
                transfer_style: "scroll",
                log: Some(crate::netlog::Target {
                    address: [192, 168, 1, 82],
                    port: 4405,
                }),
            }),
            r#"{"version":"1.2.3","proto":6,"ios":58,"ios_revision":6432,"ahbprot":true,"arena1_free":4096,"arena2_free":8192,"heap_free":1024,"tcp_stack_used":736,"tcp_stack_size":32768,"init_ms":2510,"scan_ms":82,"ip":"192.168.1.20","net_starts":2,"apps":17,"device":"usb2","inserted":["sd","usb2"],"log":"192.168.1.82:4405","network":"receiving","receive":{"completed":31,"total":64},"transfer_style":"scroll","time":1790100000}"#
        );
        assert_eq!(
            reborn_status_json(RebornStatusSnapshot {
                version: "1.2.3",
                network: "network_unavailable",
                ios: 0,
                ios_revision: 0,
                ahbprot: false,
                arena1_free: 0,
                arena2_free: 0,
                heap_free: 0,
                apps: 0,
                device: None,
                inserted_mask: 0,
                init_ms: 0,
                scan_ms: 0,
                tcp_stack_size: 0,
                tcp_stack_used: 0,
                net_starts: 0,
                time: 0,
                ip: [0, 0, 0, 0],
                completed: 0,
                total: 0,
                transfer_style: "none",
                log: None,
            }),
            r#"{"version":"1.2.3","proto":6,"ios":0,"ios_revision":0,"ahbprot":false,"arena1_free":0,"arena2_free":0,"heap_free":0,"tcp_stack_used":0,"tcp_stack_size":0,"init_ms":0,"scan_ms":0,"ip":"0.0.0.0","net_starts":0,"apps":0,"device":null,"inserted":[],"log":null,"network":"network_unavailable","receive":{"completed":0,"total":0},"transfer_style":"none","time":0}"#
        );
    }

    #[test]
    fn elapsed_ticks_to_milliseconds_handles_fraction_and_wrap() {
        assert_eq!(elapsed_ticks_to_milliseconds(10, 110, 100), 1000);
        assert_eq!(elapsed_ticks_to_milliseconds(10, 35, 100), 250);
        assert_eq!(elapsed_ticks_to_milliseconds(u64::MAX - 49, 50, 100), 1000);
        assert_eq!(elapsed_ticks_to_milliseconds(10, 20, 0), 0);
    }

    #[test]
    fn stack_high_water_counts_from_the_first_touched_byte() {
        let stack = [0, 0, 0, 4, 0, 8, 0, 0];
        assert_eq!(
            stack_high_water_used_bytes(0, stack.len(), |index| stack[index]),
            5
        );
        assert_eq!(stack_high_water_used_bytes(0, 4, |_| 0), 0);
        assert_eq!(stack_high_water_used_bytes(3, 3, |_| 1), 0);
    }

    #[test]
    fn zlib_stream_decoder_reads_bounded_chunks_without_buffering_the_wire_body() {
        let expected: Vec<u8> = (0..96 * 1024)
            .scan(0x1234_5678u32, |state, _| {
                *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                Some((*state >> 24) as u8)
            })
            .collect();
        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&expected, 6);
        assert!(compressed.len() > 8 * 1024);
        let mut offset = 0;
        let decoded = decode_zlib_stream(expected.len(), compressed.len(), |buffer| {
            let end = offset + buffer.len();
            buffer.copy_from_slice(&compressed[offset..end]);
            offset = end;
            Ok::<_, ()>(())
        })
        .unwrap();
        assert_eq!(decoded, expected);
        assert_eq!(offset, compressed.len());
    }

    #[test]
    fn payload_consumer_defers_until_the_producer_finishes() {
        let lock = AtomicBool::new(false);
        assert!(try_claim_payload(&lock));
        let mut slot = Some(alloc::vec![0x48, 0x41, 0x58, 0x58]);
        for _ in 0..120 {
            assert_eq!(try_with_payload(&lock, || slot.take()), None);
        }
        assert_eq!(slot.as_deref(), Some(&[0x48, 0x41, 0x58, 0x58][..]));
        lock.store(false, Ordering::Release);
        assert_eq!(
            try_with_payload(&lock, || slot.take()),
            Some(Some(alloc::vec![0x48, 0x41, 0x58, 0x58]))
        );
        assert!(slot.is_none());
        assert!(!lock.load(Ordering::Acquire));
    }

    #[test]
    fn gecko_indicator_stays_off_without_a_detected_device() {
        for status in [
            ReceiverStatus::Starting,
            ReceiverStatus::Listening,
            ReceiverStatus::Receiving,
            ReceiverStatus::PayloadReady,
            ReceiverStatus::NetworkUnavailable,
            ReceiverStatus::Stopped,
        ] {
            assert!(!indicator_state(status, false).gecko_active);
        }
        assert!(indicator_state(ReceiverStatus::Listening, true).gecko_active);
    }

    #[test]
    fn network_initialization_can_finish_after_more_than_four_seconds() {
        let polls = core::cell::Cell::new(0);
        let waits = core::cell::Cell::new(0);
        let result = wait_network_initialization(
            || {
                let poll = polls.get();
                polls.set(poll + 1);
                if poll < 250 { -16 } else { 0 }
            },
            || false,
            || waits.set(waits.get() + 1),
        );
        assert_eq!(result, Some(0));
        assert_eq!(polls.get(), 251);
        assert_eq!(waits.get(), 250);
    }

    #[test]
    fn network_initialization_can_be_cancelled_while_ios_is_busy() {
        let waits = core::cell::Cell::new(0);
        let result =
            wait_network_initialization(|| -16, || waits.get() == 3, || waits.set(waits.get() + 1));
        assert_eq!(result, None);
        assert_eq!(waits.get(), 3);
    }

    #[test]
    fn network_initialization_preserves_the_final_ios_error() {
        assert_eq!(
            wait_network_initialization(
                || -110,
                || false,
                || panic!("terminal error must not wait")
            ),
            Some(-110)
        );
    }
    use core::sync::atomic::AtomicBool;

    #[test]
    fn ios_accept_would_block_code_is_recognized_separately_from_net_init_eagain() {
        assert!(socket_would_block(-6));
        assert!(!socket_would_block(-11));
        assert!(!socket_would_block(-1));
    }

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
    fn wiiload_and_gdb_use_distinct_tcp_ports() {
        assert_ne!(WIILOAD_TCP_PORT, gdb_transport::DEFAULT_PORT);
        assert_eq!(WIILOAD_TCP_PORT, 4299);
        assert_eq!(gdb_transport::DEFAULT_PORT, 2828);
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

    #[test]
    fn network_indicator_blinks_every_twenty_frames_while_starting() {
        assert!(!network_indicator_visible(ReceiverStatus::Starting, 0));
        assert!(!network_indicator_visible(ReceiverStatus::Starting, 19));
        assert!(network_indicator_visible(ReceiverStatus::Starting, 20));
        assert!(network_indicator_visible(ReceiverStatus::Starting, 39));
        assert!(!network_indicator_visible(ReceiverStatus::Starting, 40));
        assert!(network_indicator_visible(ReceiverStatus::Listening, 0));
        assert!(!network_indicator_visible(
            ReceiverStatus::NetworkUnavailable,
            20
        ));
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

    #[test]
    fn client_deadline_is_absolute_and_handles_timebase_wrap() {
        assert!(!client_deadline_expired(100, 199, 100));
        assert!(client_deadline_expired(100, 200, 100));
        assert!(client_deadline_expired(u64::MAX - 4, 5, 10));
        assert!(!client_deadline_expired(u64::MAX - 4, 4, 10));
    }
}
