//! Bounds-checked ZIP central-directory parsing and archive path validation.

use alloc::string::String;
use alloc::vec::Vec;
use core::str;

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
}

/// A parsed ZIP archive. The parser owns the names and does not retain `data`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ZipArchive {
    entries: Vec<ZipEntry>,
}

impl ZipArchive {
    /// Read classic ZIP central-directory metadata from a complete archive.
    ///
    /// This parser does not decompress entry data. It rejects split archives,
    /// ZIP64 sentinels, malformed central-directory ranges, unsafe paths, and
    /// names that are not UTF-8.
    pub fn parse(data: &[u8]) -> Result<Self, ZipError> {
        let (eocd, entry_count, directory_offset, directory_size) = parse_end_record(data)?;
        if entry_count as usize > MAX_ARCHIVE_ENTRIES {
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
            let compression_method = read_u16(fixed, 10)?;
            let crc32 = read_u32(fixed, 16)?;
            let compressed_size = read_u32(fixed, 20)?;
            let uncompressed_size = read_u32(fixed, 24)?;
            let filename_length = read_u16(fixed, 28)? as usize;
            let extra_length = read_u16(fixed, 30)? as usize;
            let comment_length = read_u16(fixed, 32)? as usize;
            let disk_start = read_u16(fixed, 34)?;
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
            validate_local_entry(
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
                is_directory: name.ends_with('/'),
            });
            cursor = variable_end;
        }

        if cursor != directory_end {
            return Err(ZipError::InvalidDirectoryRange);
        }

        Ok(Self { entries })
    }

    /// Return all entries in central-directory order.
    pub fn entries(&self) -> &[ZipEntry] {
        &self.entries
    }

    /// Find one entry by its exact archive path.
    pub fn entry(&self, name: &str) -> Option<&ZipEntry> {
        self.entries.iter().find(|entry| entry.name == name)
    }
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

fn validate_local_entry(
    data: &[u8],
    directory_start: usize,
    local_header_offset: u32,
    flags: u16,
    compression_method: u16,
    compressed_size: u32,
    central_name: &[u8],
) -> Result<(), ZipError> {
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
    Ok(())
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

    fn zip_with_entries(names: &[&str]) -> Vec<u8> {
        let mut data = Vec::new();
        let mut central_records = Vec::new();
        for name in names {
            let name_bytes = name.as_bytes();
            let offset = data.len() as u32;
            data.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            data.extend_from_slice(&20u16.to_le_bytes());
            data.extend_from_slice(&0u16.to_le_bytes());
            data.extend_from_slice(&0u16.to_le_bytes());
            data.extend_from_slice(&0u16.to_le_bytes());
            data.extend_from_slice(&0u16.to_le_bytes());
            data.extend_from_slice(&0u32.to_le_bytes());
            data.extend_from_slice(&0u32.to_le_bytes());
            data.extend_from_slice(&0u32.to_le_bytes());
            data.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            data.extend_from_slice(&0u16.to_le_bytes());
            data.extend_from_slice(name_bytes);

            central_records.extend_from_slice(&CENTRAL_DIRECTORY_FILE_HEADER.to_le_bytes());
            central_records.extend_from_slice(&20u16.to_le_bytes());
            central_records.extend_from_slice(&20u16.to_le_bytes());
            central_records.extend_from_slice(&0u16.to_le_bytes());
            central_records.extend_from_slice(&0u16.to_le_bytes());
            central_records.extend_from_slice(&0u16.to_le_bytes());
            central_records.extend_from_slice(&0u16.to_le_bytes());
            central_records.extend_from_slice(&0u32.to_le_bytes());
            central_records.extend_from_slice(&0u32.to_le_bytes());
            central_records.extend_from_slice(&0u32.to_le_bytes());
            central_records.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            central_records.extend_from_slice(&0u16.to_le_bytes());
            central_records.extend_from_slice(&0u16.to_le_bytes());
            central_records.extend_from_slice(&0u16.to_le_bytes());
            central_records.extend_from_slice(&0u16.to_le_bytes());
            central_records.extend_from_slice(&0u32.to_le_bytes());
            central_records.extend_from_slice(&offset.to_le_bytes());
            central_records.extend_from_slice(name_bytes);
        }
        let directory_offset = data.len() as u32;
        data.extend_from_slice(&central_records);
        data.extend_from_slice(&END_OF_CENTRAL_DIRECTORY.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&(names.len() as u16).to_le_bytes());
        data.extend_from_slice(&(names.len() as u16).to_le_bytes());
        data.extend_from_slice(&(central_records.len() as u32).to_le_bytes());
        data.extend_from_slice(&directory_offset.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data
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
