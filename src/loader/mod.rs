//! Safe parsers for executable formats used by HBC.
//!
//! These functions only inspect input bytes. A caller must apply the returned
//! plan through a platform-specific memory adapter.

use alloc::{string::String, vec::Vec};
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

pub const MAX_LAUNCH_ARGUMENT_BYTES: usize = 1024;

/// Decide whether launch must preserve the current IOS state for AHB access.
///
/// HBC patches ES after an IOS reload to restore AHB access. libogc-rs does not
/// expose that patch, so apps that request AHB access must keep the current IOS.
pub const fn should_reload_ios(ahb_access: bool) -> bool {
    !ahb_access
}

/// Build the Wii `__argv.commandLine` byte sequence used by HBC.
///
/// The executable path is the first NUL-terminated argument. Metadata values
/// follow as separate NUL-terminated arguments, with one final NUL. The
/// complete byte sequence fits in HBC's reserved 1024-byte argument area.
pub fn build_launch_arguments(
    executable_path: &str,
    arguments: &[String],
) -> Result<Vec<u8>, ParseError> {
    if executable_path.as_bytes().contains(&0)
        || arguments.iter().any(|arg| arg.as_bytes().contains(&0))
    {
        return Err(ParseError::InvalidFormat("launch arguments contain NUL"));
    }
    let mut length = executable_path
        .len()
        .checked_add(2)
        .ok_or(ParseError::OutOfBounds)?;
    for argument in arguments {
        length = length
            .checked_add(argument.len())
            .and_then(|length| length.checked_add(1))
            .ok_or(ParseError::OutOfBounds)?;
    }
    if length > MAX_LAUNCH_ARGUMENT_BYTES {
        return Err(ParseError::InvalidFormat("launch arguments exceed limit"));
    }

    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| ParseError::OutOfBounds)?;
    bytes.extend_from_slice(executable_path.as_bytes());
    bytes.push(0);
    for argument in arguments {
        bytes.extend_from_slice(argument.as_bytes());
        bytes.push(0);
    }
    bytes.push(0);
    Ok(bytes)
}

/// Check every source and destination range before a platform writes a plan.
///
/// A parsed plan can still be created by callers, so platform code must not
/// trust its public fields without checking them again.
pub fn validate_load_plan(
    bytes: &[u8],
    plan: &LoadPlan,
    allowed_memory: &Range<u32>,
) -> Result<(), ParseError> {
    if allowed_memory.start >= allowed_memory.end {
        return Err(ParseError::InvalidSegment("empty allowed memory range"));
    }
    if plan.segments.is_empty() {
        return Err(ParseError::InvalidFormat("load plan has no segments"));
    }

    let mut entry_is_executable = false;
    for segment in &plan.segments {
        let source_len = segment
            .file_range
            .end
            .checked_sub(segment.file_range.start)
            .ok_or(ParseError::OutOfBounds)?;
        if segment.file_range.end > bytes.len()
            || source_len != segment.file_size as usize
            || segment.file_size > segment.memory_size
        {
            return Err(ParseError::InvalidSegment("invalid segment file range"));
        }
        let destination = memory_range(segment.address, segment.memory_size)?
            .ok_or(ParseError::InvalidSegment("empty load segment"))?;
        check_bounds(destination, allowed_memory)?;
        entry_is_executable |=
            segment.executable && contains(&segment.address, &segment.memory_size, plan.entry);
    }

    if !entry_is_executable {
        return Err(ParseError::InvalidFormat(
            "entry point is outside executable segments",
        ));
    }
    if let Some(bss) = &plan.bss {
        if bss.start > bss.end {
            return Err(ParseError::InvalidSegment("reversed BSS range"));
        }
        check_bounds(bss.clone(), allowed_memory)?;
    }
    Ok(())
}

#[cfg(target_arch = "powerpc")]
mod wii;

#[cfg(target_arch = "powerpc")]
pub use wii::{apply_load_plan, prepare_launch, transfer_control};

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

    #[test]
    fn launch_ios_policy_preserves_ahb_access() {
        assert!(should_reload_ios(false));
        assert!(!should_reload_ios(true));
    }
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
    fn builds_hbc_command_line_with_nul_separated_arguments() {
        let arguments = alloc::vec![String::from("--safe"), String::from("two words")];
        assert_eq!(
            build_launch_arguments("sd:/apps/demo/boot.elf", &arguments).unwrap(),
            b"sd:/apps/demo/boot.elf\0--safe\0two words\0\0"
        );
        assert_eq!(
            build_launch_arguments("sd:/apps/demo/boot.dol", &[]).unwrap(),
            b"sd:/apps/demo/boot.dol\0\0"
        );
    }

    #[test]
    fn rejects_invalid_or_oversized_launch_arguments() {
        assert!(matches!(
            build_launch_arguments("bad\0path", &[]),
            Err(ParseError::InvalidFormat(_))
        ));
        let long = String::from("x").repeat(MAX_LAUNCH_ARGUMENT_BYTES);
        assert!(matches!(
            build_launch_arguments("boot.elf", &[long]),
            Err(ParseError::InvalidFormat(_))
        ));
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

    #[test]
    fn validates_plan_sources_destinations_and_entry() {
        let bytes = [1, 2, 3, 4];
        let plan = LoadPlan {
            segments: vec![LoadSegment {
                file_range: 0..4,
                address: 0x8000_3400,
                file_size: 4,
                memory_size: 8,
                executable: true,
            }],
            bss: Some(0x8000_3408..0x8000_3410),
            entry: 0x8000_3400,
        };
        assert_eq!(
            validate_load_plan(&bytes, &plan, &(0x8000_3400..0x8000_4000)),
            Ok(())
        );

        let mut invalid = plan.clone();
        invalid.segments[0].file_range = 1..4;
        assert_eq!(
            validate_load_plan(&bytes, &invalid, &(0x8000_3400..0x8000_4000)),
            Err(ParseError::InvalidSegment("invalid segment file range"))
        );

        let mut invalid = plan;
        invalid.entry = 0x8000_3408;
        assert_eq!(
            validate_load_plan(&bytes, &invalid, &(0x8000_3400..0x8000_4000)),
            Err(ParseError::InvalidFormat(
                "entry point is outside executable segments"
            ))
        );

        let mut invalid = LoadPlan {
            segments: vec![LoadSegment {
                file_range: 0..4,
                address: 0x8000_3400,
                file_size: 4,
                memory_size: 8,
                executable: true,
            }],
            bss: Some(0x8000_3410..0x8000_340f),
            entry: 0x8000_3400,
        };
        assert_eq!(
            validate_load_plan(&bytes, &invalid, &(0x8000_3400..0x8000_4000)),
            Err(ParseError::InvalidSegment("reversed BSS range"))
        );
        invalid.bss = Some(0x8000_3410..0x8000_3410);
        assert!(validate_load_plan(&bytes, &invalid, &(0x8000_3400..0x8000_4000)).is_ok());
    }
    fn put16(b: &mut [u8], p: usize, v: u16) {
        b[p..p + 2].copy_from_slice(&v.to_be_bytes());
    }
    fn put32(b: &mut [u8], p: usize, v: u32) {
        b[p..p + 4].copy_from_slice(&v.to_be_bytes());
    }
}
