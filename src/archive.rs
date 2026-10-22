//! Bounds-checked ZIP central-directory parsing and archive path validation.

use alloc::string::String;
use alloc::vec::Vec;
use core::str;
use miniz_oxide::inflate::decompress_to_vec_with_limit;

const MAX_ARCHIVE_PATH_BYTES: usize = 1024;
const MAX_ARCHIVE_ENTRIES: usize = 4096;
const END_OF_CENTRAL_DIRECTORY: u32 = 0x0605_4b50;
const CENTRAL_DIRECTORY_FILE_HEADER: u32 = 0x0201_4b50;
const LOCAL_FILE_HEADER: u32 = 0x0403_4b50;
const END_OF_CENTRAL_DIRECTORY_SIZE: usize = 22;
const CENTRAL_DIRECTORY_FILE_HEADER_SIZE: usize = 46;
const MAX_ZIP_COMMENT_BYTES: usize = u16::MAX as usize;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchivePathError {
    Empty,
    TooLong,
    Absolute,
    UnsupportedSeparator,
    InvalidComponent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ZipError {
    TooShort,
    EndRecordNotFound,
    MultiDiskArchive,
    Zip64Unsupported,
    TooManyEntries,
    InvalidDirectoryRange,
    InvalidEntry,
    InvalidPath(ArchivePathError),
    InvalidUtf8Path,
    UnsupportedEncryption,
    UnsupportedEntryType,
    UnsupportedCompression(u16),
    OutputTooLarge,
    AllocationFailed,
    DecompressionFailed,
    SizeMismatch,
    CrcMismatch,
}

/// Metadata from one ZIP central-directory entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ZipEntry {
    pub name: String,
    pub flags: u16,
    pub compression_method: u16,
    pub crc32: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    pub local_header_offset: u32,
    pub is_directory: bool,
    /// False for Unix symbolic links and other special file types.
    pub is_supported_type: bool,
}

/// A parsed ZIP archive. The parser owns the names and does not retain `data`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ZipArchive {
    entries: Vec<ZipEntry>,
    directory_start: usize,
}

impl ZipArchive {
    #[cfg(test)]
    pub(crate) fn from_entries_for_test(entries: Vec<ZipEntry>) -> Self {
        Self {
            entries,
            directory_start: 0,
        }
    }

    /// Read classic ZIP central-directory metadata from a complete archive.
    ///
    /// It rejects split archives, ZIP64 sentinels, malformed central-directory
    /// ranges, unsafe paths, and names that are not UTF-8.
    pub fn parse(data: &[u8]) -> Result<Self, ZipError> {
        Self::parse_limited(data, MAX_ARCHIVE_ENTRIES)
    }

    /// Read an archive while enforcing a caller-specific entry limit before
    /// allocating the central-directory index.
    pub fn parse_limited(data: &[u8], max_entries: usize) -> Result<Self, ZipError> {
        let (eocd, entry_count, directory_offset, directory_size) = parse_end_record(data)?;
        if entry_count as usize > MAX_ARCHIVE_ENTRIES.min(max_entries) {
            return Err(ZipError::TooManyEntries);
        }

        let directory_start = directory_offset as usize;
        let directory_end = directory_start
            .checked_add(directory_size as usize)
            .ok_or(ZipError::InvalidDirectoryRange)?;
        if directory_end > eocd || directory_end > data.len() {
            return Err(ZipError::InvalidDirectoryRange);
        }

        let mut entries = Vec::with_capacity(entry_count as usize);
        let mut cursor = directory_start;
        for _ in 0..entry_count {
            let fixed_end = cursor
                .checked_add(CENTRAL_DIRECTORY_FILE_HEADER_SIZE)
                .ok_or(ZipError::InvalidEntry)?;
            let fixed = data.get(cursor..fixed_end).ok_or(ZipError::InvalidEntry)?;
            if read_u32(fixed, 0)? != CENTRAL_DIRECTORY_FILE_HEADER {
                return Err(ZipError::InvalidEntry);
            }

            let flags = read_u16(fixed, 8)?;
            let version_made_by = read_u16(fixed, 4)?;
            let compression_method = read_u16(fixed, 10)?;
            let crc32 = read_u32(fixed, 16)?;
            let compressed_size = read_u32(fixed, 20)?;
            let uncompressed_size = read_u32(fixed, 24)?;
            let filename_length = read_u16(fixed, 28)? as usize;
            let extra_length = read_u16(fixed, 30)? as usize;
            let comment_length = read_u16(fixed, 32)? as usize;
            let disk_start = read_u16(fixed, 34)?;
            let external_attributes = read_u32(fixed, 38)?;
            let local_header_offset = read_u32(fixed, 42)?;

            if disk_start != 0 {
                return Err(ZipError::MultiDiskArchive);
            }
            if compressed_size == u32::MAX
                || uncompressed_size == u32::MAX
                || local_header_offset == u32::MAX
            {
                return Err(ZipError::Zip64Unsupported);
            }

            let variable_end = fixed_end
                .checked_add(filename_length)
                .and_then(|end| end.checked_add(extra_length))
                .and_then(|end| end.checked_add(comment_length))
                .ok_or(ZipError::InvalidEntry)?;
            if variable_end > directory_end {
                return Err(ZipError::InvalidEntry);
            }
            let filename_start = fixed_end;
            let filename_end = filename_start
                .checked_add(filename_length)
                .ok_or(ZipError::InvalidEntry)?;
            let name = str::from_utf8(
                data.get(filename_start..filename_end)
                    .ok_or(ZipError::InvalidEntry)?,
            )
            .map_err(|_| ZipError::InvalidUtf8Path)?;
            validate_archive_path(name).map_err(ZipError::InvalidPath)?;
            let is_directory = name.ends_with('/');
            let is_supported_type =
                zip_entry_type_supported(version_made_by, external_attributes, is_directory);
            let _ = local_compressed_range(
                data,
                directory_start,
                local_header_offset,
                flags,
                compression_method,
                compressed_size,
                name.as_bytes(),
            )?;

            entries.push(ZipEntry {
                name: String::from(name),
                flags,
                compression_method,
                crc32,
                compressed_size,
                uncompressed_size,
                local_header_offset,
                is_directory,
                is_supported_type,
            });
            cursor = variable_end;
        }

        if cursor != directory_end {
            return Err(ZipError::InvalidDirectoryRange);
        }

        Ok(Self {
            entries,
            directory_start,
        })
    }

    /// Return all entries in central-directory order.
    pub fn entries(&self) -> &[ZipEntry] {
        &self.entries
    }

    /// Find one entry by its exact archive path.
    pub fn entry(&self, name: &str) -> Option<&ZipEntry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    /// Decompress one file entry, up to `max_output_size` bytes.
    ///
    /// ZIP methods 0 (stored) and 8 (raw DEFLATE) are supported. Encryption,
    /// directories, unsupported methods, size mismatches, and CRC errors fail.
    pub fn extract(
        &self,
        data: &[u8],
        name: &str,
        max_output_size: usize,
    ) -> Result<Vec<u8>, ZipError> {
        let entry = self.entry(name).ok_or(ZipError::InvalidEntry)?;
        if entry.is_directory {
            return Err(ZipError::InvalidEntry);
        }
        if !entry.is_supported_type {
            return Err(ZipError::UnsupportedEntryType);
        }
        if entry.flags & (0x0001 | 0x0040 | 0x2000) != 0 {
            return Err(ZipError::UnsupportedEncryption);
        }

        let expected_size = entry.uncompressed_size as usize;
        if expected_size > max_output_size {
            return Err(ZipError::OutputTooLarge);
        }
        let (compressed_start, compressed_end) = local_compressed_range(
            data,
            self.directory_start,
            entry.local_header_offset,
            entry.flags,
            entry.compression_method,
            entry.compressed_size,
            entry.name.as_bytes(),
        )?;
        let compressed = data
            .get(compressed_start..compressed_end)
            .ok_or(ZipError::InvalidEntry)?;

        let output = match entry.compression_method {
            0 => {
                if entry.compressed_size != entry.uncompressed_size {
                    return Err(ZipError::SizeMismatch);
                }
                let mut output = Vec::new();
                output
                    .try_reserve_exact(expected_size)
                    .map_err(|_| ZipError::AllocationFailed)?;
                output.extend_from_slice(compressed);
                output
            }
            8 => decompress_to_vec_with_limit(compressed, expected_size.max(1))
                .map_err(|_| ZipError::DecompressionFailed)?,
            method => return Err(ZipError::UnsupportedCompression(method)),
        };

        if output.len() != expected_size {
            return Err(ZipError::SizeMismatch);
        }
        if crc32fast::hash(&output) != entry.crc32 {
            return Err(ZipError::CrcMismatch);
        }
        Ok(output)
    }
}

fn zip_entry_type_supported(
    version_made_by: u16,
    external_attributes: u32,
    is_directory: bool,
) -> bool {
    let host_system = version_made_by >> 8;
    if host_system == 3 || host_system == 19 {
        let unix_type = (external_attributes >> 16) & 0o170000;
        return match unix_type {
            0 => true,
            0o100000 => !is_directory,
            0o040000 => is_directory,
            _ => false,
        };
    }
    true
}

/// Validate a ZIP entry path before joining it to an installation directory.
///
/// ZIP uses `/` as its separator on every host. Backslashes, drive prefixes,
/// absolute paths, and parent/current directory components are rejected so
/// extraction cannot write outside the selected destination.
pub fn validate_archive_path(path: &str) -> Result<(), ArchivePathError> {
    if path.is_empty() {
        return Err(ArchivePathError::Empty);
    }
    if path.len() > MAX_ARCHIVE_PATH_BYTES {
        return Err(ArchivePathError::TooLong);
    }
    if path.starts_with('/') {
        return Err(ArchivePathError::Absolute);
    }
    if path.contains('\\') {
        return Err(ArchivePathError::UnsupportedSeparator);
    }
    if path.contains(':') || path.contains('\0') {
        return Err(ArchivePathError::InvalidComponent);
    }

    let components = path.strip_suffix('/').unwrap_or(path);
    if components.is_empty() {
        return Err(ArchivePathError::InvalidComponent);
    }
    for component in components.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            return Err(ArchivePathError::InvalidComponent);
        }
    }
    Ok(())
}

fn parse_end_record(data: &[u8]) -> Result<(usize, u16, u32, u32), ZipError> {
    if data.len() < END_OF_CENTRAL_DIRECTORY_SIZE {
        return Err(ZipError::TooShort);
    }
    let search_start = data
        .len()
        .saturating_sub(END_OF_CENTRAL_DIRECTORY_SIZE + MAX_ZIP_COMMENT_BYTES);
    let last_possible = data.len() - END_OF_CENTRAL_DIRECTORY_SIZE;
    for offset in (search_start..=last_possible).rev() {
        if read_u32(data, offset).ok() != Some(END_OF_CENTRAL_DIRECTORY) {
            continue;
        }
        let comment_length = match read_u16(data, offset + 20) {
            Ok(length) => length as usize,
            Err(_) => continue,
        };
        let record_end = match offset
            .checked_add(END_OF_CENTRAL_DIRECTORY_SIZE)
            .and_then(|end| end.checked_add(comment_length))
        {
            Some(end) => end,
            None => continue,
        };
        // Requiring the comment to end at EOF avoids matching a signature
        // embedded in the archive comment or accepting trailing corrupt data.
        if record_end != data.len() {
            continue;
        }

        let disk_number = read_u16(data, offset + 4)?;
        let directory_disk = read_u16(data, offset + 6)?;
        let disk_entries = read_u16(data, offset + 8)?;
        let total_entries = read_u16(data, offset + 10)?;
        let directory_size = read_u32(data, offset + 12)?;
        let directory_offset = read_u32(data, offset + 16)?;
        if disk_number != 0 || directory_disk != 0 || disk_entries != total_entries {
            return Err(ZipError::MultiDiskArchive);
        }
        if disk_entries == u16::MAX || directory_size == u32::MAX || directory_offset == u32::MAX {
            return Err(ZipError::Zip64Unsupported);
        }
        return Ok((offset, total_entries, directory_offset, directory_size));
    }
    Err(ZipError::EndRecordNotFound)
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16, ZipError> {
    let bytes = data
        .get(offset..offset.checked_add(2).ok_or(ZipError::InvalidEntry)?)
        .ok_or(ZipError::InvalidEntry)?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn local_compressed_range(
    data: &[u8],
    directory_start: usize,
    local_header_offset: u32,
    flags: u16,
    compression_method: u16,
    compressed_size: u32,
    central_name: &[u8],
) -> Result<(usize, usize), ZipError> {
    let start = local_header_offset as usize;
    let fixed_end = start.checked_add(30).ok_or(ZipError::InvalidEntry)?;
    if fixed_end > directory_start {
        return Err(ZipError::InvalidEntry);
    }
    let fixed = data.get(start..fixed_end).ok_or(ZipError::InvalidEntry)?;
    if read_u32(fixed, 0)? != LOCAL_FILE_HEADER
        || read_u16(fixed, 6)? != flags
        || read_u16(fixed, 8)? != compression_method
    {
        return Err(ZipError::InvalidEntry);
    }

    let name_length = read_u16(fixed, 26)? as usize;
    let extra_length = read_u16(fixed, 28)? as usize;
    let data_start = fixed_end
        .checked_add(name_length)
        .and_then(|end| end.checked_add(extra_length))
        .ok_or(ZipError::InvalidEntry)?;
    let name_end = fixed_end
        .checked_add(name_length)
        .ok_or(ZipError::InvalidEntry)?;
    let data_end = data_start
        .checked_add(compressed_size as usize)
        .ok_or(ZipError::InvalidEntry)?;
    if data_end > directory_start || data.get(fixed_end..name_end) != Some(central_name) {
        return Err(ZipError::InvalidEntry);
    }
    Ok((data_start, data_end))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, ZipError> {
    let bytes = data
        .get(offset..offset.checked_add(4).ok_or(ZipError::InvalidEntry)?)
        .ok_or(ZipError::InvalidEntry)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_unix_regular_files_and_directories_but_rejects_special_types() {
        assert!(zip_entry_type_supported(3 << 8, 0o100644 << 16, false));
        assert!(zip_entry_type_supported(3 << 8, 0o040755 << 16, true));
        assert!(!zip_entry_type_supported(3 << 8, 0o120777 << 16, false));
        assert!(!zip_entry_type_supported(3 << 8, 0o040755 << 16, false));
    }

    #[test]
    fn direct_extract_rejects_unsupported_file_types() {
        let data = zip_with_entries(&["link"]);
        let mut archive = ZipArchive::parse(&data).unwrap();
        archive.entries[0].is_supported_type = false;
        assert_eq!(
            archive.extract(&data, "link", 8),
            Err(ZipError::UnsupportedEntryType)
        );
    }

    use crate::test_support::zip_with_payload;

    fn zip_with_entries(names: &[&str]) -> Vec<u8> {
        crate::test_support::zip(
            &names
                .iter()
                .map(|&name| (name, &b""[..]))
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn independent_stored_zip_has_known_bytes_and_crc() {
        // Fixed archive from Python zipfile; the shared fixture builder is not used.
        const ZIP: &[u8] = &[
            0x50, 0x4b, 0x03, 0x04, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x21, 0x00,
            0x26, 0x39, 0xf4, 0xcb, 0x09, 0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x04, 0x00,
            0x00, 0x00, 0x64, 0x61, 0x74, 0x61, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38,
            0x39, 0x50, 0x4b, 0x01, 0x02, 0x14, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x21, 0x00, 0x26, 0x39, 0xf4, 0xcb, 0x09, 0x00, 0x00, 0x00, 0x09, 0x00, 0x00,
            0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x64, 0x61, 0x74, 0x61, 0x50, 0x4b, 0x05, 0x06, 0x00,
            0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x32, 0x00, 0x00, 0x00, 0x2b, 0x00, 0x00,
            0x00, 0x00, 0x00,
        ];
        let archive = ZipArchive::parse(ZIP).unwrap();
        assert_eq!(archive.entries()[0].crc32, 0xcbf4_3926);
        assert_eq!(archive.extract(ZIP, "data", 9), Ok(b"123456789".to_vec()));
    }

    #[test]
    fn lists_file_and_directory_entries() {
        let archive = ZipArchive::parse(&zip_with_entries(&["Demo/", "Demo/boot.elf"])).unwrap();
        assert_eq!(archive.entries().len(), 2);
        assert!(archive.entries()[0].is_directory);
        assert_eq!(
            archive.entry("Demo/boot.elf").unwrap().compression_method,
            0
        );
        assert!(!archive.entries()[1].is_directory);
    }

    #[test]
    fn rejects_traversal_paths_inside_zip() {
        for path in [
            "../escape",
            "Demo/../../escape",
            "/absolute",
            "Demo\\escape",
        ] {
            assert!(matches!(
                ZipArchive::parse(&zip_with_entries(&[path])),
                Err(ZipError::InvalidPath(_))
            ));
        }
    }

    #[test]
    fn rejects_truncated_and_inconsistent_directories() {
        assert_eq!(ZipArchive::parse(&[]), Err(ZipError::TooShort));
        let mut archive = zip_with_entries(&["app/boot.dol"]);
        archive.truncate(archive.len() - 1);
        assert_eq!(
            ZipArchive::parse(&archive),
            Err(ZipError::EndRecordNotFound)
        );

        let mut archive = zip_with_entries(&["app/boot.dol"]);
        let eocd_offset = archive.len() - END_OF_CENTRAL_DIRECTORY_SIZE;
        archive[eocd_offset + 16..eocd_offset + 20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(ZipArchive::parse(&archive), Err(ZipError::Zip64Unsupported));
    }

    #[test]
    fn rejects_invalid_utf8_and_malformed_entry_bounds() {
        let mut archive = zip_with_entries(&["app/boot.dol"]);
        let eocd_offset = archive.len() - END_OF_CENTRAL_DIRECTORY_SIZE;
        let name_offset = read_u32(&archive, eocd_offset + 16).unwrap() as usize
            + CENTRAL_DIRECTORY_FILE_HEADER_SIZE;
        archive[name_offset] = 0xff;
        assert_eq!(ZipArchive::parse(&archive), Err(ZipError::InvalidUtf8Path));

        let mut archive = zip_with_entries(&["app/boot.dol"]);
        let eocd_offset = archive.len() - END_OF_CENTRAL_DIRECTORY_SIZE;
        let directory_offset = read_u32(&archive, eocd_offset + 16).unwrap() as usize;
        archive[directory_offset + 28..directory_offset + 30]
            .copy_from_slice(&u16::MAX.to_le_bytes());
        assert_eq!(ZipArchive::parse(&archive), Err(ZipError::InvalidEntry));

        let mut archive = zip_with_entries(&["app/boot.dol"]);
        let eocd_offset = archive.len() - END_OF_CENTRAL_DIRECTORY_SIZE;
        let directory_offset = read_u32(&archive, eocd_offset + 16).unwrap() as usize;
        archive[directory_offset + 42..directory_offset + 46]
            .copy_from_slice(&(directory_offset as u32).to_le_bytes());
        assert_eq!(ZipArchive::parse(&archive), Err(ZipError::InvalidEntry));
    }

    #[test]
    fn extracts_stored_and_raw_deflate_entries() {
        let expected = b"HBC archive payload repeated for compression";
        for method in [0, 8] {
            let data = zip_with_payload("apps/demo/payload.bin", expected, method, 0);
            let archive = ZipArchive::parse(&data).unwrap();
            assert_eq!(
                archive
                    .extract(&data, "apps/demo/payload.bin", 1024)
                    .unwrap(),
                expected
            );
        }
    }

    #[test]
    fn extraction_enforces_limit_encryption_size_and_crc() {
        let stored = zip_with_payload("apps/demo/data", b"payload", 0, 0);
        let archive = ZipArchive::parse(&stored).unwrap();
        assert_eq!(
            archive.extract(&stored, "apps/demo/data", 2),
            Err(ZipError::OutputTooLarge)
        );

        let encrypted = zip_with_payload("apps/demo/data", b"payload", 0, 1);
        let archive = ZipArchive::parse(&encrypted).unwrap();
        assert_eq!(
            archive.extract(&encrypted, "apps/demo/data", 32),
            Err(ZipError::UnsupportedEncryption)
        );

        let mut bad_crc = zip_with_payload("apps/demo/data", b"payload", 0, 0);
        let eocd_offset = bad_crc.len() - END_OF_CENTRAL_DIRECTORY_SIZE;
        let directory_offset = read_u32(&bad_crc, eocd_offset + 16).unwrap() as usize;
        bad_crc[directory_offset + 16] ^= 0x80;
        let archive = ZipArchive::parse(&bad_crc).unwrap();
        assert_eq!(
            archive.extract(&bad_crc, "apps/demo/data", 32),
            Err(ZipError::CrcMismatch)
        );
    }

    #[test]
    fn rejects_compressed_payload_outside_local_data_range() {
        let mut data = zip_with_payload("apps/demo/data", b"abc", 0, 0);
        let eocd_offset = data.len() - END_OF_CENTRAL_DIRECTORY_SIZE;
        let directory_offset = read_u32(&data, eocd_offset + 16).unwrap() as usize;
        data[directory_offset + 20..directory_offset + 24].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(ZipArchive::parse(&data), Err(ZipError::Zip64Unsupported));

        data[directory_offset + 20..directory_offset + 24]
            .copy_from_slice(&u32::MAX.saturating_sub(1).to_le_bytes());
        assert_eq!(ZipArchive::parse(&data), Err(ZipError::InvalidEntry));
    }

    #[test]
    fn accepts_safe_file_and_directory_paths() {
        assert_eq!(validate_archive_path("Demo/boot.elf"), Ok(()));
        assert_eq!(validate_archive_path("Demo/assets/"), Ok(()));
    }

    #[test]
    fn rejects_paths_that_escape_or_change_platform_meaning() {
        for path in ["", "/boot.elf", "../boot.elf", "Demo/../../boot.elf"] {
            assert!(validate_archive_path(path).is_err(), "accepted {path:?}");
        }
        assert_eq!(
            validate_archive_path("Demo\\boot.elf"),
            Err(ArchivePathError::UnsupportedSeparator)
        );
        assert_eq!(
            validate_archive_path("C:/boot.elf"),
            Err(ArchivePathError::InvalidComponent)
        );
        assert!(validate_archive_path("Demo//boot.elf").is_err());
        assert!(validate_archive_path("Demo/./boot.elf").is_err());
    }
}
