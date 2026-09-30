//! Bounded validation and metadata parsing for HBC theme ZIP files.

use alloc::{collections::BTreeSet, string::String, vec::Vec};

use crate::{
    archive::{ZipArchive, ZipError},
    management::{plan_extraction, ExtractionLimits, ManagementError},
    metadata::{parse_theme_xml, ThemeMetadata},
};

pub const MAX_THEME_ARCHIVE_BYTES: usize = 20 * 1024 * 1024;
pub const MAX_THEME_ENTRIES: usize = 64;
pub const MAX_THEME_XML_BYTES: usize = 1024 * 1024;
const MAX_THEME_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_THEME_TOTAL_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThemePackageError {
    TooLarge,
    InvalidArchive(ZipError),
    InvalidEntryCount,
    InvalidEntry,
    MissingThemeXml,
    ThemeXmlTooLarge,
    InvalidThemeXml,
    InvalidMetadata(String),
    UnsafeArchive(ManagementError),
}

/// A validated HBC theme archive and its parsed `theme.xml` settings.
///
/// The package owns the archive index and metadata, but not the ZIP bytes.
/// Callers keep the original bytes and use [`extract_file`](Self::extract_file)
/// when they need one validated file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemePackage {
    archive: ZipArchive,
    theme_xml_name: String,
    pub metadata: ThemeMetadata,
}

impl ThemePackage {
    /// Parse an HBC theme ZIP with 20 MiB and 64-entry archive limits.
    /// All paths and extraction resources are validated before metadata is read.
    pub fn parse(data: &[u8]) -> Result<Self, ThemePackageError> {
        if data.len() > MAX_THEME_ARCHIVE_BYTES {
            return Err(ThemePackageError::TooLarge);
        }
        let archive = ZipArchive::parse_limited(data, MAX_THEME_ENTRIES)
            .map_err(ThemePackageError::InvalidArchive)?;
        let entries = archive.entries();
        if entries.is_empty() {
            return Err(ThemePackageError::InvalidEntryCount);
        }
        plan_extraction(
            &archive,
            ExtractionLimits {
                max_entries: MAX_THEME_ENTRIES,
                max_file_bytes: MAX_THEME_FILE_BYTES,
                max_total_bytes: MAX_THEME_TOTAL_BYTES,
            },
        )
        .map_err(ThemePackageError::UnsafeArchive)?;

        let mut folded_names = BTreeSet::new();
        let mut theme_xml_name = None;
        for entry in entries {
            if !folded_names.insert(entry.name.to_ascii_lowercase()) {
                return Err(ThemePackageError::InvalidEntry);
            }
            if entry.is_directory || entry.uncompressed_size == 0 {
                continue;
            }
            if !entry.name.contains('/') && entry.name.eq_ignore_ascii_case("theme.xml") {
                theme_xml_name = Some(entry.name.clone());
            }
        }
        let theme_xml_name = theme_xml_name.ok_or(ThemePackageError::MissingThemeXml)?;
        let xml_entry = archive
            .entry(&theme_xml_name)
            .ok_or(ThemePackageError::MissingThemeXml)?;
        if xml_entry.uncompressed_size as usize > MAX_THEME_XML_BYTES {
            return Err(ThemePackageError::ThemeXmlTooLarge);
        }
        let xml = archive
            .extract(data, &theme_xml_name, MAX_THEME_XML_BYTES)
            .map_err(ThemePackageError::InvalidArchive)?;
        let xml = core::str::from_utf8(&xml).map_err(|_| ThemePackageError::InvalidThemeXml)?;
        let metadata = parse_theme_xml(xml).map_err(ThemePackageError::InvalidMetadata)?;
        Ok(Self {
            archive,
            theme_xml_name,
            metadata,
        })
    }

    pub fn theme_xml_name(&self) -> &str {
        &self.theme_xml_name
    }

    /// Extract one file with a caller-supplied output bound.
    pub fn extract_file(
        &self,
        data: &[u8],
        name: &str,
        max_output_bytes: usize,
    ) -> Result<Vec<u8>, ThemePackageError> {
        let entry = self
            .archive
            .entries()
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(name))
            .ok_or(ThemePackageError::InvalidEntry)?;
        self.archive
            .extract(data, &entry.name, max_output_bytes)
            .map_err(ThemePackageError::InvalidArchive)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        zip_with_flags(files, 0)
    }

    fn zip_with_flags(files: &[(&str, &[u8])], flags: u16) -> Vec<u8> {
        let mut output = Vec::new();
        let mut central = Vec::new();
        for (name, payload) in files {
            let offset = output.len() as u32;
            let name_bytes = name.as_bytes();
            let size = payload.len() as u32;
            let crc = crc32fast::hash(payload);
            output.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            output.extend_from_slice(&20u16.to_le_bytes());
            output.extend_from_slice(&flags.to_le_bytes());
            output.extend_from_slice(&0u16.to_le_bytes());
            output.extend_from_slice(&[0; 4]);
            output.extend_from_slice(&crc.to_le_bytes());
            output.extend_from_slice(&size.to_le_bytes());
            output.extend_from_slice(&size.to_le_bytes());
            output.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            output.extend_from_slice(&0u16.to_le_bytes());
            output.extend_from_slice(name_bytes);
            output.extend_from_slice(payload);

            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&flags.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&[0; 4]);
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&size.to_le_bytes());
            central.extend_from_slice(&size.to_le_bytes());
            central.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            central.extend_from_slice(&[0; 6]);
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name_bytes);
        }
        let central_offset = output.len() as u32;
        output.extend_from_slice(&central);
        output.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        output.extend_from_slice(&[0; 4]);
        output.extend_from_slice(&(files.len() as u16).to_le_bytes());
        output.extend_from_slice(&(files.len() as u16).to_le_bytes());
        output.extend_from_slice(&(central.len() as u32).to_le_bytes());
        output.extend_from_slice(&central_offset.to_le_bytes());
        output.extend_from_slice(&0u16.to_le_bytes());
        output
    }

    const THEME_XML: &[u8] = b"<theme><description>Water</description></theme>";

    #[test]
    fn accepts_theme_xml_and_extracts_case_insensitive_file_names() {
        let data = zip(&[("Theme.XML", THEME_XML), ("BUTTON.PNG", b"image")]);
        let package = ThemePackage::parse(&data).unwrap();
        assert_eq!(package.theme_xml_name(), "Theme.XML");
        assert_eq!(package.metadata.description.as_deref(), Some("Water"));
        assert_eq!(
            package.extract_file(&data, "button.png", 8).unwrap(),
            b"image"
        );
    }

    #[test]
    fn requires_theme_xml_and_ignores_safe_unrelated_files() {
        assert_eq!(
            ThemePackage::parse(&zip(&[("button.png", b"image")])),
            Err(ThemePackageError::MissingThemeXml)
        );
        assert_eq!(
            ThemePackage::parse(&zip(&[("folder/theme.xml", THEME_XML)])),
            Err(ThemePackageError::MissingThemeXml)
        );
        let package = ThemePackage::parse(&zip(&[
            ("theme.xml", THEME_XML),
            ("other.bin", b"x"),
            ("folder/unrelated.dat", b"y"),
        ]))
        .unwrap();
        assert_eq!(package.theme_xml_name(), "theme.xml");
        assert!(matches!(
            ThemePackage::parse(&zip(&[("theme.xml", THEME_XML), ("../evil.txt", b"x")])),
            Err(ThemePackageError::InvalidArchive(_))
        ));
        assert_eq!(
            ThemePackage::parse(&zip(&[("theme.xml", THEME_XML), ("THEME.XML", THEME_XML)])),
            Err(ThemePackageError::InvalidEntry)
        );
    }

    #[test]
    fn caps_entry_count_and_rejects_bad_theme_xml() {
        let many: Vec<_> = (0..=MAX_THEME_ENTRIES)
            .map(|index| (format!("readme{index}.txt"), b"x".as_slice()))
            .collect();
        let borrowed: Vec<_> = many
            .iter()
            .map(|(name, bytes)| (name.as_str(), *bytes))
            .collect();
        assert_eq!(
            ThemePackage::parse(&zip(&borrowed)),
            Err(ThemePackageError::InvalidArchive(ZipError::TooManyEntries))
        );
        assert!(matches!(
            ThemePackage::parse(&zip(&[("theme.xml", b"<theme>")])),
            Err(ThemePackageError::InvalidMetadata(_))
        ));
    }

    #[test]
    fn rejects_encrypted_theme_files_and_bad_theme_xml_crc() {
        assert_eq!(
            ThemePackage::parse(&zip_with_flags(&[("theme.xml", THEME_XML)], 1)),
            Err(ThemePackageError::UnsafeArchive(
                ManagementError::UnsupportedEntry
            ))
        );
        let mut bad_crc = zip(&[("theme.xml", THEME_XML)]);
        bad_crc[30 + "theme.xml".len()] ^= 1;
        assert_eq!(
            ThemePackage::parse(&bad_crc),
            Err(ThemePackageError::InvalidArchive(ZipError::CrcMismatch))
        );
    }

    #[test]
    fn accepts_hbc_assets_and_rejects_unsafe_unrelated_paths() {
        let package = ThemePackage::parse(&zip(&[
            ("theme.xml", THEME_XML),
            ("throbber.png", b"image"),
            ("progress.png", b"image"),
            ("other.bin", b"x"),
        ]))
        .unwrap();
        assert_eq!(package.theme_xml_name(), "theme.xml");
        assert!(matches!(
            ThemePackage::parse(&zip(&[("theme.xml", THEME_XML), ("../outside.bin", b"x")])),
            Err(ThemePackageError::InvalidArchive(_))
        ));
    }
}
