use alloc::{format, string::String, vec::Vec};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

pub const RECORD_LEN: usize = 192;
#[cfg(target_arch = "powerpc")]
const ADDRESS: usize = crate::netlog::CRASH_BLOCK_ADDRESS;
#[cfg(any(target_arch = "powerpc", test))]
const LEGACY_CHECK_WORDS: usize = 29;
static AVAILABLE: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CrashReport {
    exception: u32,
    pc: u32,
    msr: u32,
    lr: u32,
    cr: u32,
    ctr: u32,
    dar: u32,
    dsisr: u32,
    sp: u32,
    uptime_ms: u32,
    frames: [u32; 12],
    app: [u8; 20],
    kind: u32,
    code: u32,
    reason: [u8; 64],
}

impl CrashReport {
    const EMPTY: Self = Self {
        exception: 0,
        pc: 0,
        msr: 0,
        lr: 0,
        cr: 0,
        ctr: 0,
        dar: 0,
        dsisr: 0,
        sp: 0,
        uptime_ms: 0,
        frames: [0; 12],
        app: [0; 20],
        kind: 0,
        code: 0,
        reason: [0; 64],
    };

    fn json(self) -> String {
        let app = clean_text(&self.app);
        let reason = clean_text(&self.reason);
        let mut frames = String::new();
        for address in self.frames.into_iter().take_while(|address| *address != 0) {
            if !frames.is_empty() {
                frames.push(',');
            }
            frames.push_str(&format!("\"{address:08x}\""));
        }
        format!(
            "{{\"kind\":\"{}\",\"code\":{},\"reason\":\"{}\",\"app\":\"{}\",\"exception\":{},\"name\":\"{}\",\"pc\":\"{:08x}\",\"lr\":\"{:08x}\",\"msr\":\"{:08x}\",\"cr\":\"{:08x}\",\"ctr\":\"{:08x}\",\"dar\":\"{:08x}\",\"dsisr\":\"{:08x}\",\"sp\":\"{:08x}\",\"uptime_ms\":{},\"frames\":[{}]}}",
            kind_name(self.kind),
            self.code,
            reason,
            app,
            self.exception,
            exception_name(self.exception),
            self.pc,
            self.lr,
            self.msr,
            self.cr,
            self.ctr,
            self.dar,
            self.dsisr,
            self.sp,
            self.uptime_ms,
            frames
        )
    }
}

struct StoredCrash(UnsafeCell<CrashReport>);

// Startup writes the crash record before worker start. Only the main loop can clear it.
unsafe impl Sync for StoredCrash {}

static CRASH: StoredCrash = StoredCrash(UnsafeCell::new(CrashReport::EMPTY));

fn kind_name(kind: u32) -> &'static str {
    match kind {
        0 => "exception",
        1 => "fatal",
        2 => "hang",
        3 => "assert",
        4 => "abort",
        5 => "stack",
        _ => "exception",
    }
}

fn exception_name(exception: u32) -> &'static str {
    match exception {
        1 => "reset",
        2 => "machine check",
        3 => "DSI",
        4 => "ISI",
        5 => "interrupt",
        6 => "alignment",
        7 => "program",
        8 => "floating point",
        9 => "decrementer",
        12 => "system call",
        13 => "trace",
        15 => "performance monitor",
        19 => "breakpoint",
        _ => "unknown",
    }
}

fn clean_text(bytes: &[u8]) -> String {
    let mut output = Vec::new();
    for &byte in bytes {
        if byte == 0 {
            break;
        }
        output.push(
            if (0x20..0x7f).contains(&byte) && byte != b'"' && byte != b'\\' {
                byte
            } else {
                b'?'
            },
        );
    }
    String::from_utf8(output).expect("sanitized crash text is ASCII")
}

#[cfg(any(target_arch = "powerpc", test))]
fn checksum(bytes: &[u8]) -> u32 {
    bytes.chunks_exact(4).fold(0x5a17_c0de, |value, word| {
        value.rotate_left(5) ^ u32::from_be_bytes(word.try_into().expect("four-byte chunk"))
    })
}

#[cfg(any(target_arch = "powerpc", test))]
fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(bytes[offset..offset + 4].try_into().expect("word offset"))
}

#[cfg(any(target_arch = "powerpc", test))]
fn parse_record(bytes: &[u8; RECORD_LEN]) -> Option<CrashReport> {
    if &bytes[..4] != b"HBCC" {
        return None;
    }
    let version = word(bytes, 4);
    let (kind, code, reason) = match version {
        2 if word(bytes, 188) == checksum(&bytes[..188]) => {
            let mut reason = [0; 64];
            reason.copy_from_slice(&bytes[124..188]);
            (word(bytes, 116), word(bytes, 120), reason)
        }
        1 if word(bytes, LEGACY_CHECK_WORDS * 4) == checksum(&bytes[..LEGACY_CHECK_WORDS * 4]) => {
            (0, 0, [0; 64])
        }
        _ => return None,
    };
    let mut frames = [0; 12];
    for (index, frame) in frames.iter_mut().enumerate() {
        *frame = word(bytes, 48 + index * 4);
    }
    let mut app = [0; 20];
    app.copy_from_slice(&bytes[96..116]);
    Some(CrashReport {
        exception: word(bytes, 8),
        pc: word(bytes, 12),
        msr: word(bytes, 16),
        lr: word(bytes, 20),
        cr: word(bytes, 24),
        ctr: word(bytes, 28),
        dar: word(bytes, 32),
        dsisr: word(bytes, 36),
        sp: word(bytes, 40),
        uptime_ms: word(bytes, 44),
        frames,
        app,
        kind,
        code,
        reason,
    })
}

/// Read and consume Reborn's crash record before the MEM2 heap is allocated.
pub fn initialize() {
    AVAILABLE.store(false, Ordering::Release);
    #[cfg(target_arch = "powerpc")]
    {
        let source = ADDRESS as *const u8;
        let mut bytes = [0; RECORD_LEN];
        // SAFETY: Reborn reserves this MEM2 block for the crash record.
        unsafe {
            ogc_cache::dc_invalidate_range(source.cast(), RECORD_LEN as u32);
            for (offset, byte) in bytes.iter_mut().enumerate() {
                *byte = core::ptr::read_volatile(source.add(offset));
            }
        }
        if let Some(report) = parse_record(&bytes) {
            // SAFETY: Startup writes before the network worker starts.
            unsafe { *CRASH.0.get() = report };
            AVAILABLE.store(true, Ordering::Release);
        }
        let record = ADDRESS as *mut u8;
        // The C HBC clears this record after it copies a valid report.
        unsafe {
            for offset in 0..RECORD_LEN {
                core::ptr::write_volatile(record.add(offset), 0);
            }
            ogc_cache::dc_flush_range(record.cast(), RECORD_LEN as u32);
        }
    }
}

pub fn clear() {
    AVAILABLE.store(false, Ordering::Release);
}

pub fn json() -> Option<String> {
    if !AVAILABLE.load(Ordering::Acquire) {
        return None;
    }
    // SAFETY: initialized before worker start; main loop only changes availability.
    let report = unsafe { *CRASH.0.get() };
    Some(report.json())
}

#[cfg(test)]
mod tests {
    use super::{RECORD_LEN, checksum, exception_name, parse_record};

    fn v2_record() -> [u8; RECORD_LEN] {
        let mut bytes = [0; RECORD_LEN];
        bytes[..4].copy_from_slice(b"HBCC");
        bytes[4..8].copy_from_slice(&2u32.to_be_bytes());
        bytes[8..12].copy_from_slice(&3u32.to_be_bytes());
        bytes[12..16].copy_from_slice(&0x8000_1234u32.to_be_bytes());
        bytes[16..20].copy_from_slice(&0x8000_5678u32.to_be_bytes());
        bytes[48..52].copy_from_slice(&0x8000_beefu32.to_be_bytes());
        bytes[116..120].copy_from_slice(&1u32.to_be_bytes());
        bytes[120..124].copy_from_slice(&7u32.to_be_bytes());
        bytes[96..104].copy_from_slice(b"bad\napp\0");
        let check = checksum(&bytes[..188]);
        bytes[188..192].copy_from_slice(&check.to_be_bytes());
        bytes
    }

    #[test]
    fn test_parse_record_reads_v2_and_formats_reborn_status() {
        let bytes = v2_record();
        assert_eq!(checksum(&bytes[..188]), 0x660a_0454);
        let report = parse_record(&bytes).expect("valid v2 crash record");
        assert_eq!(report.kind, 1);
        assert_eq!(report.code, 7);
        assert_eq!(report.exception, 3);
        assert_eq!(&report.app[..8], b"bad\napp\0");
        let json = report.json();
        assert!(json.contains("\"kind\":\"fatal\""));
        assert!(json.contains("\"app\":\"bad?app\""));
        assert!(json.contains("\"name\":\"DSI\""));
        assert!(json.contains("\"frames\":[\"8000beef\"]"));
    }

    #[test]
    fn test_parse_record_rejects_invalid_full_and_legacy_checksums() {
        let mut bad = v2_record();
        bad[191] ^= 1;
        assert!(parse_record(&bad).is_none());

        let mut legacy = [0; RECORD_LEN];
        legacy[..4].copy_from_slice(b"HBCC");
        legacy[4..8].copy_from_slice(&1u32.to_be_bytes());
        legacy[8..12].copy_from_slice(&3u32.to_be_bytes());
        let check = checksum(&legacy[..116]);
        assert_eq!(check, 0xa588_8027);
        legacy[116..120].copy_from_slice(&check.to_be_bytes());
        let report = parse_record(&legacy).expect("valid v1 crash record");
        assert_eq!(report.kind, 0);
        assert_eq!(exception_name(report.exception), "DSI");
    }
}
