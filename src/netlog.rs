//! Reborn's shared app network-log target record.

pub const BLOCK_LEN: usize = 32;
pub const LOW_MEMORY_ADDRESS: usize = 0x8000_2f20;
pub const KEEP_ADDRESS: usize = 0x9180_0000;
pub const CRASH_BLOCK_ADDRESS: usize = KEEP_ADDRESS + BLOCK_LEN;
pub const LAST_LOG_ADDRESS: usize = 0x9180_0100;
pub const LAST_LOG_BLOCK_LEN: usize = 4 * 5 + 20 + 4096 + 4;
pub const RESERVED_END: usize = LAST_LOG_ADDRESS + LAST_LOG_BLOCK_LEN;
const MAGIC: &[u8; 4] = b"HBCN";
const VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Target {
    pub address: [u8; 4],
    pub port: u16,
}

impl Target {
    pub fn encode(self) -> [u8; BLOCK_LEN] {
        let mut block = [0; BLOCK_LEN];
        if self.port == 0 {
            return block;
        }
        block[..4].copy_from_slice(MAGIC);
        block[4..8].copy_from_slice(&VERSION.to_be_bytes());
        block[8..12].copy_from_slice(&self.address);
        block[12..14].copy_from_slice(&self.port.to_be_bytes());
        let check = u32::from_be_bytes(*MAGIC)
            ^ VERSION
            ^ u32::from_be_bytes(self.address)
            ^ u32::from(self.port);
        block[16..20].copy_from_slice(&check.to_be_bytes());
        block
    }

    pub fn decode(block: &[u8; BLOCK_LEN]) -> Option<Self> {
        if &block[..4] != MAGIC
            || u32::from_be_bytes(block[4..8].try_into().ok()?) != VERSION
            || block[14..16] != [0, 0]
            || block[20..] != [0; 12]
        {
            return None;
        }
        let address: [u8; 4] = block[8..12].try_into().ok()?;
        let port = u16::from_be_bytes(block[12..14].try_into().ok()?);
        if port == 0 {
            return None;
        }
        let check = u32::from_be_bytes(block[16..20].try_into().ok()?);
        let expected =
            u32::from_be_bytes(*MAGIC) ^ VERSION ^ u32::from_be_bytes(address) ^ u32::from(port);
        (check == expected).then_some(Self { address, port })
    }
}

/// Keep Rust's MEM2 heap below Reborn's persistent target, crash, and log records.
pub const fn heap_limit_before_reserved(arena_low: usize, requested: usize) -> usize {
    if arena_low >= RESERVED_END {
        requested
    } else if arena_low >= KEEP_ADDRESS {
        0
    } else {
        let available = KEEP_ADDRESS - arena_low;
        if requested < available {
            requested
        } else {
            available
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_record_matches_the_reborn_c_layout_and_checksum() {
        let target = Target {
            address: [192, 168, 1, 82],
            port: 4405,
        };
        let block = target.encode();
        assert_eq!(&block[..16], b"HBCN\0\0\0\x01\xc0\xa8\x01\x52\x11\x35\0\0");
        assert_eq!(Target::decode(&block), Some(target));
        assert_eq!(&block[20..], &[0; 12]);
    }

    #[test]
    fn empty_and_corrupt_records_do_not_restore_a_target() {
        assert_eq!(Target::decode(&[0; BLOCK_LEN]), None);
        let mut block = Target {
            address: [10, 0, 0, 4],
            port: 4405,
        }
        .encode();
        block[16] ^= 1;
        assert_eq!(Target::decode(&block), None);
    }

    #[test]
    fn allocator_stops_before_the_fixed_mem2_record() {
        assert_eq!(
            heap_limit_before_reserved(0x9000_0000, 16 * 1024 * 1024),
            16 * 1024 * 1024
        );
        assert_eq!(
            heap_limit_before_reserved(0x9100_0000, 16 * 1024 * 1024),
            8 * 1024 * 1024
        );
        assert_eq!(heap_limit_before_reserved(KEEP_ADDRESS, 1024), 0);
        assert_eq!(heap_limit_before_reserved(CRASH_BLOCK_ADDRESS, 1024), 0);
        assert_eq!(heap_limit_before_reserved(RESERVED_END - 1, 1024), 0);
        assert_eq!(heap_limit_before_reserved(RESERVED_END, 1024), 1024);
    }
}
