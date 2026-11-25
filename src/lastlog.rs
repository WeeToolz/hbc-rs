use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

pub const RECORD_LEN: usize = crate::netlog::LAST_LOG_BLOCK_LEN;
#[cfg(target_arch = "powerpc")]
const ADDRESS: usize = crate::netlog::LAST_LOG_ADDRESS;
const TEXT_OFFSET: usize = 40;
const TEXT_LEN: usize = 4096;
const APP_OFFSET: usize = 20;
const APP_LEN: usize = 20;
static AVAILABLE: AtomicBool = AtomicBool::new(false);

struct RetainedRecord(UnsafeCell<[u8; RECORD_LEN]>);

// The record is written once at startup, before the network worker starts.
unsafe impl Sync for RetainedRecord {}

static RETAINED: RetainedRecord = RetainedRecord(UnsafeCell::new([0; RECORD_LEN]));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Record<'a> {
    why: u32,
    uptime_ms: u32,
    app: [u8; APP_LEN],
    app_len: usize,
    text: &'a [u8],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Status {
    pub why: &'static str,
    pub bytes: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResponseError {
    OutOfMemory,
}

impl Record<'_> {
    fn status(self) -> Status {
        Status {
            why: why_name(self.why),
            bytes: self.text.len() as u32,
        }
    }
}

fn why_name(why: u32) -> &'static str {
    match why {
        0 => "exit",
        1 => "exception",
        2 => "fatal",
        3 => "hang",
        4 => "abort",
        5 => "stack",
        6 => "power",
        _ => "unknown",
    }
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes.chunks_exact(4).fold(0x5a17_c0de, |value, word| {
        value.rotate_left(5) ^ u32::from_be_bytes(word.try_into().expect("four-byte chunk"))
    })
}

fn parse_record(bytes: &[u8; RECORD_LEN]) -> Option<Record<'_>> {
    if &bytes[..4] != b"HBCL"
        || u32::from_be_bytes(bytes[4..8].try_into().ok()?) != 1
        || u32::from_be_bytes(bytes[12..16].try_into().ok()?) as usize > TEXT_LEN
        || checksum(&bytes[..RECORD_LEN - 4])
            != u32::from_be_bytes(bytes[RECORD_LEN - 4..].try_into().ok()?)
    {
        return None;
    }

    let mut app = [0; APP_LEN];
    let app_source = &bytes[APP_OFFSET..APP_OFFSET + APP_LEN];
    let mut app_len = 0;
    for &byte in app_source {
        if byte == 0 {
            break;
        }
        app[app_len] = if (0x20..0x7f).contains(&byte) && byte != b'"' && byte != b'\\' {
            byte
        } else {
            b'?'
        };
        app_len += 1;
    }
    let text_len = u32::from_be_bytes(bytes[12..16].try_into().ok()?) as usize;
    Some(Record {
        why: u32::from_be_bytes(bytes[8..12].try_into().ok()?),
        uptime_ms: u32::from_be_bytes(bytes[16..20].try_into().ok()?),
        app,
        app_len,
        text: &bytes[TEXT_OFFSET..TEXT_OFFSET + text_len],
    })
}

/// Read and consume Reborn's retained log before the MEM2 heap is allocated.
pub fn initialize() {
    AVAILABLE.store(false, Ordering::Release);
    #[cfg(target_arch = "powerpc")]
    {
        let source = ADDRESS as *const u8;
        let mut bytes = [0; RECORD_LEN];
        // SAFETY: Reborn reserves this aligned MEM2 block for the kept log.
        // The cache invalidation makes the previous title's record visible.
        unsafe {
            ogc_cache::dc_invalidate_range(source.cast(), RECORD_LEN as u32);
            for (offset, byte) in bytes.iter_mut().enumerate() {
                *byte = core::ptr::read_volatile(source.add(offset));
            }
        }
        if parse_record(&bytes).is_some() {
            // SAFETY: Startup calls this before any worker can read RETAINED.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    bytes.as_ptr(),
                    (*RETAINED.0.get()).as_mut_ptr(),
                    RECORD_LEN,
                )
            };
            AVAILABLE.store(true, Ordering::Release);
        }
        let record = ADDRESS as *mut u8;
        // Reborn consumes the log once by clearing its 64-byte header.
        unsafe {
            for offset in 0..64 {
                core::ptr::write_volatile(record.add(offset), 0);
            }
            ogc_cache::dc_flush_range(record.cast(), 64);
        }
    }
}

pub fn status() -> Option<Status> {
    if !AVAILABLE.load(Ordering::Acquire) {
        return None;
    }
    // SAFETY: initialize writes RETAINED once before worker start. It is read-only after publication.
    let bytes = unsafe { &*RETAINED.0.get() };
    parse_record(bytes).map(Record::status)
}

/// Build the HBCL response body. `try_reserve_exact` lets the caller return ENOMEM.
pub fn response_body() -> Result<Option<Vec<u8>>, ResponseError> {
    if !AVAILABLE.load(Ordering::Acquire) {
        return Ok(None);
    }
    // SAFETY: initialize writes RETAINED once before worker start. It is read-only after publication.
    let bytes = unsafe { &*RETAINED.0.get() };
    let Some(record) = parse_record(bytes) else {
        return Ok(None);
    };
    build_reply(record).map(Some)
}

fn build_reply(record: Record<'_>) -> Result<Vec<u8>, ResponseError> {
    let mut body = Vec::new();
    body.try_reserve_exact(64 + record.text.len())
        .map_err(|_| ResponseError::OutOfMemory)?;
    body.extend_from_slice(b"HBCL 1 ");
    body.extend_from_slice(why_name(record.why).as_bytes());
    body.push(b' ');
    push_decimal(&mut body, record.uptime_ms);
    body.push(b' ');
    body.extend_from_slice(&record.app[..record.app_len]);
    body.push(b'\n');
    body.extend_from_slice(record.text);
    Ok(body)
}

fn push_decimal(output: &mut Vec<u8>, value: u32) {
    let mut digits = [0; 10];
    let mut count = 0;
    let mut value = value;
    loop {
        digits[count] = b'0' + (value % 10) as u8;
        count += 1;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    output.extend(digits[..count].iter().rev());
}

#[cfg(test)]
mod tests {
    use super::{RECORD_LEN, build_reply, checksum, parse_record};

    fn valid_record() -> [u8; RECORD_LEN] {
        let mut bytes = [0; RECORD_LEN];
        bytes[..4].copy_from_slice(b"HBCL");
        bytes[4..8].copy_from_slice(&1u32.to_be_bytes());
        bytes[8..12].copy_from_slice(&2u32.to_be_bytes());
        bytes[12..16].copy_from_slice(&5u32.to_be_bytes());
        bytes[16..20].copy_from_slice(&1234u32.to_be_bytes());
        bytes[20..26].copy_from_slice(b"probe\0");
        bytes[40..45].copy_from_slice(b"hello");
        let check = checksum(&bytes[..RECORD_LEN - 4]);
        bytes[RECORD_LEN - 4..].copy_from_slice(&check.to_be_bytes());
        bytes
    }

    #[test]
    fn test_parse_record_validates_c_layout_and_builds_reborn_reply() {
        let bytes = valid_record();
        assert_eq!(checksum(&bytes[..RECORD_LEN - 4]), 0x3a6f_3e8a);
        let record = parse_record(&bytes).expect("valid C record");

        assert_eq!(record.why, 2);
        assert_eq!(record.uptime_ms, 1234);
        assert_eq!(&record.app[..record.app_len], b"probe");
        assert_eq!(record.text, b"hello");
        assert_eq!(
            build_reply(record).unwrap(),
            b"HBCL 1 fatal 1234 probe\nhello"
        );
    }

    #[test]
    fn test_parse_record_rejects_bad_checksum_and_oversize_text() {
        let mut bad_checksum = valid_record();
        bad_checksum[RECORD_LEN - 1] ^= 1;
        assert!(parse_record(&bad_checksum).is_none());

        let mut long_text = valid_record();
        long_text[12..16].copy_from_slice(&((RECORD_LEN as u32) + 1).to_be_bytes());
        let check = checksum(&long_text[..RECORD_LEN - 4]);
        long_text[RECORD_LEN - 4..].copy_from_slice(&check.to_be_bytes());
        assert!(parse_record(&long_text).is_none());
    }

    #[test]
    fn test_parse_record_sanitizes_header_text_like_reborn() {
        let mut bytes = valid_record();
        bytes[20..26].copy_from_slice(b"p\n\"x\0\0");
        let check = checksum(&bytes[..RECORD_LEN - 4]);
        bytes[RECORD_LEN - 4..].copy_from_slice(&check.to_be_bytes());
        let record = parse_record(&bytes).expect("record with text fields");
        assert_eq!(&record.app[..record.app_len], b"p??x");
    }
}
