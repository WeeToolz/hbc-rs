//! Safe parsers for executable formats used by HBC.
//!
//! These functions only inspect input bytes. A caller must apply the returned
//! plan through a platform-specific memory adapter.

use alloc::vec::Vec;
use core::ops::Range;

const DOL_HEADER_SIZE: usize = 0x100;
const ELF_HEADER_SIZE: usize = 52;
const ELF_PHDR_SIZE: usize = 32;
const PT_LOAD: u32 = 1;
const PF_X: u32 = 1;
const ELFDATA2MSB: u8 = 2;
const ELFCLASS32: u8 = 1;
const ET_EXEC: u16 = 2;
const EM_PPC: u16 = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadSegment {
    /// Source byte range in the executable. It is empty for zero-fill segments.
    pub file_range: Range<usize>,
    /// Destination address in Wii memory.
    pub address: u32,
    /// Number of bytes to initialize from the file.
    pub file_size: u32,
    /// Total memory size. The trailing `memory_size - file_size` bytes must be zeroed.
    pub memory_size: u32,
    pub executable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadPlan {
    pub segments: Vec<LoadSegment>,
    pub bss: Option<Range<u32>>,
    pub entry: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Truncated,
    InvalidFormat(&'static str),
    Unsupported(&'static str),
    OutOfBounds,
    AddressOverflow,
    InvalidSegment(&'static str),
}

/// Parse a DOL file. `allowed_memory` optionally limits each section to a
/// half-open destination range, as HBC's overlap check does.
pub fn parse_dol(bytes: &[u8], allowed_memory: Option<Range<u32>>) -> Result<LoadPlan, ParseError> {
    if bytes.len() < DOL_HEADER_SIZE {
        return Err(ParseError::Truncated);
    }
    let mut segments = Vec::new();
    for i in 0..7 {
        push_dol_segment(
            bytes,
            &mut segments,
            i,
            0,
            0x48,
            0x90,
            allowed_memory.as_ref(),
        )?;
    }
    for i in 0..11 {
        push_dol_segment(
            bytes,
            &mut segments,
            i,
            0x1c,
            0x64,
            0xac,
            allowed_memory.as_ref(),
        )?;
    }
    let bss_start = be32(bytes, 0xd8)?;
    let bss_size = be32(bytes, 0xdc)?;
    let bss = memory_range(bss_start, bss_size)?;
    if let (Some(bounds), Some(range)) = (allowed_memory.as_ref(), bss.as_ref()) {
        check_bounds(range.clone(), bounds)?;
    }
    let entry = be32(bytes, 0xe0)?;
    if segments.is_empty() {
        return Err(ParseError::InvalidFormat("DOL has no sections"));
    }
    if !segments
        .iter()
        .any(|s| contains(&s.address, &s.memory_size, entry))
    {
        return Err(ParseError::InvalidFormat(
            "entry point is outside loaded sections",
        ));
    }
    Ok(LoadPlan {
        segments,
        bss,
        entry,
    })
}

fn push_dol_segment(
    bytes: &[u8],
    out: &mut Vec<LoadSegment>,
    i: usize,
    base: usize,
    addr_base: usize,
    size_base: usize,
    bounds: Option<&Range<u32>>,
) -> Result<(), ParseError> {
    let offset = be32(bytes, base + 4 * i)?;
    let address = be32(bytes, addr_base + 4 * i)?;
    let size = be32(bytes, size_base + 4 * i)?;
    if size == 0 {
        return Ok(());
    }
    if offset < DOL_HEADER_SIZE as u32 {
        return Err(ParseError::InvalidSegment("section overlaps DOL header"));
    }
    let end = offset.checked_add(size).ok_or(ParseError::OutOfBounds)?;
    if end as usize > bytes.len() {
        return Err(ParseError::OutOfBounds);
    }
    let mem = memory_range(address, size)?.unwrap();
    if let Some(bounds) = bounds {
        check_bounds(mem.clone(), bounds)?;
    }
    out.push(LoadSegment {
        file_range: offset as usize..end as usize,
        address,
        file_size: size,
        memory_size: size,
        executable: base == 0,
    });
    Ok(())
}

/// Parse a big-endian ELF32 PowerPC executable. HBC maps physical addresses
/// through `0x3fff_ffff | 0x8000_0000`, matching its loader.
pub fn parse_elf32_ppc(
    bytes: &[u8],
    allowed_memory: Option<Range<u32>>,
) -> Result<LoadPlan, ParseError> {
    if bytes.len() < ELF_HEADER_SIZE {
        return Err(ParseError::Truncated);
    }
    if &bytes[..4] != b"\x7fELF" {
        return Err(ParseError::InvalidFormat("missing ELF magic"));
    }
    if bytes[4] != ELFCLASS32 {
        return Err(ParseError::Unsupported("ELF class"));
    }
    if bytes[5] != ELFDATA2MSB {
        return Err(ParseError::Unsupported("ELF byte order"));
    }
    if bytes[6] != 1 {
        return Err(ParseError::Unsupported("ELF version"));
    }
    if be16(bytes, 16)? != ET_EXEC {
        return Err(ParseError::Unsupported("ELF type"));
    }
    if be16(bytes, 18)? != EM_PPC {
        return Err(ParseError::Unsupported("ELF machine"));
    }
    if be32(bytes, 20)? != 1 {
        return Err(ParseError::Unsupported("ELF version"));
    }
    let entry = map_wii_address(be32(bytes, 24)?);
    let phoff = be32(bytes, 28)? as usize;
    let ehsize = be16(bytes, 40)? as usize;
    let phentsize = be16(bytes, 42)? as usize;
    let phnum = be16(bytes, 44)? as usize;
    if ehsize != ELF_HEADER_SIZE {
        return Err(ParseError::InvalidFormat("invalid ELF header size"));
    }
    if phoff == 0 || phnum == 0 {
        return Err(ParseError::InvalidFormat("ELF has no program headers"));
    }
    if phentsize != ELF_PHDR_SIZE {
        return Err(ParseError::InvalidFormat("invalid program header size"));
    }
    let table_size = phentsize
        .checked_mul(phnum)
        .ok_or(ParseError::OutOfBounds)?;
    let table_end = phoff
        .checked_add(table_size)
        .ok_or(ParseError::OutOfBounds)?;
    if table_end > bytes.len() {
        return Err(ParseError::OutOfBounds);
    }
    let mut segments = Vec::new();
    for i in 0..phnum {
        let p = phoff + i * phentsize;
        if be32(bytes, p)? != PT_LOAD {
            continue;
        }
        let offset = be32(bytes, p + 4)?;
        let paddr = be32(bytes, p + 12)?;
        let filesz = be32(bytes, p + 16)?;
        let memsz = be32(bytes, p + 20)?;
        if filesz > memsz {
            return Err(ParseError::InvalidSegment("file size exceeds memory size"));
        }
        if memsz == 0 {
            continue;
        }
        let end = offset.checked_add(filesz).ok_or(ParseError::OutOfBounds)? as usize;
        if end > bytes.len() {
            return Err(ParseError::OutOfBounds);
        }
        let address = map_wii_address(paddr);
        let mem = memory_range(address, memsz)?
            .ok_or(ParseError::InvalidSegment("empty load segment"))?;
        if let Some(bounds) = allowed_memory.as_ref() {
            check_bounds(mem.clone(), bounds)?;
        }
        let flags = be32(bytes, p + 24)?;
        segments.push(LoadSegment {
            file_range: offset as usize..end,
            address,
            file_size: filesz,
            memory_size: memsz,
            executable: flags & PF_X != 0,
        });
    }
    if segments.is_empty() {
        return Err(ParseError::InvalidFormat("ELF has no loadable segments"));
    }
    if !segments
        .iter()
        .any(|s| contains(&s.address, &s.memory_size, entry))
    {
        return Err(ParseError::InvalidFormat(
            "entry point is outside loaded sections",
        ));
    }
    Ok(LoadPlan {
        segments,
        bss: None,
        entry,
    })
}

fn map_wii_address(address: u32) -> u32 {
    (address & 0x3fff_ffff) | 0x8000_0000
}
fn memory_range(start: u32, size: u32) -> Result<Option<Range<u32>>, ParseError> {
    if size == 0 {
        return Ok(None);
    }
    let end = start.checked_add(size).ok_or(ParseError::AddressOverflow)?;
    Ok(Some(start..end))
}
fn check_bounds(range: Range<u32>, bounds: &Range<u32>) -> Result<(), ParseError> {
    if range.start < bounds.start || range.end > bounds.end {
        return Err(ParseError::InvalidSegment(
            "destination outside allowed memory",
        ));
    }
    Ok(())
}
fn contains(start: &u32, size: &u32, address: u32) -> bool {
    start
        .checked_add(*size)
        .is_some_and(|end| *start <= address && address < end)
}
fn be16(bytes: &[u8], offset: usize) -> Result<u16, ParseError> {
    let b = bytes
        .get(offset..offset.checked_add(2).ok_or(ParseError::Truncated)?)
        .ok_or(ParseError::Truncated)?;
    Ok(u16::from_be_bytes([b[0], b[1]]))
}
fn be32(bytes: &[u8], offset: usize) -> Result<u32, ParseError> {
    let b = bytes
        .get(offset..offset.checked_add(4).ok_or(ParseError::Truncated)?)
        .ok_or(ParseError::Truncated)?;
    Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    #[test]
    fn rejects_short_dol_and_bad_section_ranges() {
        assert_eq!(parse_dol(&[], None), Err(ParseError::Truncated));
        let mut dol = vec![0u8; DOL_HEADER_SIZE];
        put32(&mut dol, 0, DOL_HEADER_SIZE as u32);
        put32(&mut dol, 0x48, 0x8000_3400);
        put32(&mut dol, 0x90, 8);
        put32(&mut dol, 0xe0, 0x8000_3400);
        assert_eq!(parse_dol(&dol, None), Err(ParseError::OutOfBounds));
    }
    #[test]
    fn parses_dol_section_and_enforces_memory_bounds() {
        let mut dol = vec![0u8; DOL_HEADER_SIZE + 4];
        put32(&mut dol, 0, DOL_HEADER_SIZE as u32);
        put32(&mut dol, 0x48, 0x8000_3400);
        put32(&mut dol, 0x90, 4);
        put32(&mut dol, 0xe0, 0x8000_3400);
        let plan = parse_dol(&dol, Some(0x8000_3400..0x8000_4000)).unwrap();
        assert_eq!(plan.segments[0].file_range, 0x100..0x104);
        assert!(plan.segments[0].executable);
        assert!(parse_dol(&dol, Some(0x8000_3401..0x8000_4000)).is_err());
    }
    #[test]
    fn parses_elf_and_rejects_truncated_program_table() {
        let mut elf = vec![0u8; 0x104];
        elf[..7].copy_from_slice(b"\x7fELF\x01\x02\x01");
        put16(&mut elf, 16, ET_EXEC);
        put16(&mut elf, 18, EM_PPC);
        put32(&mut elf, 20, 1);
        put32(&mut elf, 24, 0x1000);
        put32(&mut elf, 28, 52);
        put16(&mut elf, 40, 52);
        put16(&mut elf, 42, 32);
        put16(&mut elf, 44, 1);
        put32(&mut elf, 52, PT_LOAD);
        put32(&mut elf, 56, 0x100);
        put32(&mut elf, 64, 0x1000);
        put32(&mut elf, 68, 4);
        put32(&mut elf, 72, 8);
        put32(&mut elf, 76, PF_X);
        let plan = parse_elf32_ppc(&elf, None).unwrap();
        assert_eq!(plan.entry, 0x8000_1000);
        assert_eq!(plan.segments[0].memory_size, 8);
        assert_eq!(plan.segments[0].file_size, 4);
        assert_eq!(
            parse_elf32_ppc(&elf[..70], None),
            Err(ParseError::OutOfBounds)
        );
    }
    fn put16(b: &mut [u8], p: usize, v: u16) {
        b[p..p + 2].copy_from_slice(&v.to_be_bytes());
    }
    fn put32(b: &mut [u8], p: usize, v: u32) {
        b[p..p + 4].copy_from_slice(&v.to_be_bytes());
    }
}
