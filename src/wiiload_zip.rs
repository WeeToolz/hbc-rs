//! Host-testable validation policy for ZIP files received by Wiiload.

use alloc::{string::String, vec::Vec};

use crate::{
    archive::{ZipArchive, ZipError, validate_archive_path},
    management::{ExtractionLimits, ExtractionOperation, ExtractionPlan, plan_extraction},
    theme::{ThemePackage, ThemePackageError},
};

pub const WIILOAD_ZIP_MAX_ENTRIES: usize = 1024;
const MAX_STAGED_APP_DIRECTORY_BYTES: usize = 243;
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WiiloadZipKind {
    Elf,
    Dol,
    Theme,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WiiloadZipClassification {
    App {
        directory: String,
        kind: WiiloadZipKind,
        extraction: ExtractionPlan,
    },
    Theme {
        package: ThemePackage,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WiiloadZipError {
    InvalidArchive(ZipError),
    InvalidApp,
    InvalidTheme(ThemePackageError),
}

/// Validate an incoming ZIP as an HBC app. If app policy fails, try the
/// existing HBC theme package parser before returning an error.
pub fn classify_received_zip(data: &[u8]) -> Result<WiiloadZipClassification, WiiloadZipError> {
    match classify_app(data) {
        Ok(app) => Ok(app),
        Err(_) => match ThemePackage::parse(data) {
            Ok(package) => Ok(WiiloadZipClassification::Theme { package }),
            Err(error) => Err(WiiloadZipError::InvalidTheme(error)),
        },
    }
}

fn classify_app(data: &[u8]) -> Result<WiiloadZipClassification, WiiloadZipError> {
    let archive = ZipArchive::parse_limited(data, WIILOAD_ZIP_MAX_ENTRIES)
        .map_err(WiiloadZipError::InvalidArchive)?;
    let entries = archive.entries();
    if !(2..=WIILOAD_ZIP_MAX_ENTRIES).contains(&entries.len()) {
        return Err(WiiloadZipError::InvalidApp);
    }
    let mut directory: Option<&str> = None;
    let mut executable: Option<(&str, WiiloadZipKind)> = None;
    for entry in entries {
        validate_archive_path(&entry.name).map_err(|_| WiiloadZipError::InvalidApp)?;
        if entry.flags & (0x0001 | 0x0040 | 0x2000) != 0
            || !entry.is_supported_type
            || !matches!(entry.compression_method, 0 | 8)
        {
            return Err(WiiloadZipError::InvalidApp);
        }
        if entry.uncompressed_size == 0 {
            continue;
        }
        let components: Vec<&str> = entry.name.trim_end_matches('/').split('/').collect();
        // HBC receives a folder such as `ScummVM/boot.elf`, then extracts it
        // below its own `apps/` directory. The ZIP must have one shared root.
        if components.len() < 2 || components[0].is_empty() {
            return Err(WiiloadZipError::InvalidApp);
        }
        match directory {
            Some(existing) if existing != components[0] => return Err(WiiloadZipError::InvalidApp),
            None => directory = Some(components[0]),
            _ => {}
        }
        if !entry.is_directory && components.len() == 2 {
            let kind = if components[1].eq_ignore_ascii_case("boot.elf") {
                Some(WiiloadZipKind::Elf)
            } else if components[1].eq_ignore_ascii_case("boot.dol") {
                Some(WiiloadZipKind::Dol)
            } else if components[1].eq_ignore_ascii_case("theme.zip") {
                Some(WiiloadZipKind::Theme)
            } else {
                None
            };
            if let Some(kind) = kind {
                if executable.replace((entry.name.as_str(), kind)).is_some() {
                    return Err(WiiloadZipError::InvalidApp);
                }
            }
        }
    }
    let directory = directory.ok_or(WiiloadZipError::InvalidApp)?;
    if directory.starts_with('.') || directory.len() > MAX_STAGED_APP_DIRECTORY_BYTES {
        return Err(WiiloadZipError::InvalidApp);
    }
    let (_, kind) = executable.ok_or(WiiloadZipError::InvalidApp)?;
    let full_plan = plan_extraction(
        &archive,
        ExtractionLimits {
            max_entries: WIILOAD_ZIP_MAX_ENTRIES,
            max_file_bytes: MAX_FILE_BYTES,
            max_total_bytes: MAX_TOTAL_BYTES,
        },
    )
    .map_err(|_| WiiloadZipError::InvalidApp)?;
    let prefix = alloc::format!("{directory}/");
    let mut operations = Vec::new();
    for operation in full_plan.operations() {
        match operation {
            ExtractionOperation::CreateDirectory { path } => {
                if path == directory {
                    continue;
                }
                let relative = path
                    .strip_prefix(&prefix)
                    .ok_or(WiiloadZipError::InvalidApp)?;
                if relative.is_empty() {
                    continue;
                }
                operations.push(ExtractionOperation::CreateDirectory {
                    path: String::from(relative),
                });
            }
            ExtractionOperation::WriteFile {
                path,
                archive_name,
                size,
            } => {
                let relative = path
                    .strip_prefix(&prefix)
                    .ok_or(WiiloadZipError::InvalidApp)?;
                operations.push(ExtractionOperation::WriteFile {
                    path: String::from(relative),
                    archive_name: archive_name.clone(),
                    size: *size,
                });
            }
        }
    }
    let extraction = ExtractionPlan::from_operations(operations, full_plan.total_bytes());
    Ok(WiiloadZipClassification::App {
        directory: String::from(directory),
        kind,
        extraction,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut output = Vec::new();
        let mut central = Vec::new();
        for (name, payload) in files {
            let offset = output.len() as u32;
            let name_bytes = name.as_bytes();
            let size = payload.len() as u32;
            let crc = crc32fast::hash(payload);
            output.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            output.extend_from_slice(&20u16.to_le_bytes());
            output.extend_from_slice(&0u16.to_le_bytes());
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
            central.extend_from_slice(&0u16.to_le_bytes());
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

    #[test]
    fn accepts_app_and_returns_relative_extraction_plan() {
        let data = zip(&[
            ("demo/", b""),
            ("demo/boot.elf", b"elf"),
            ("demo/data/file", b"x"),
        ]);
        let WiiloadZipClassification::App {
            directory,
            kind,
            extraction,
        } = classify_received_zip(&data).unwrap()
        else {
            panic!()
        };
        assert_eq!(directory, "demo");
        assert_eq!(kind, WiiloadZipKind::Elf);
        assert!(extraction.operations().iter().any(|op| matches!(op, ExtractionOperation::WriteFile { path, archive_name, .. } if path == "boot.elf" && archive_name == "demo/boot.elf")));
        assert_eq!(extraction.total_bytes(), 4);
    }

    #[test]
    fn matches_hbc_executable_names_without_case_sensitivity() {
        let data = zip(&[("Demo/BOOT.ELF", b"elf"), ("Demo/meta.xml", b"<app/>")]);
        let classification = classify_received_zip(&data).unwrap();
        assert!(matches!(
            classification,
            WiiloadZipClassification::App {
                kind: WiiloadZipKind::Elf,
                ..
            }
        ));
    }

    #[test]
    fn rejects_mixed_roots_and_duplicate_executables() {
        for data in [
            zip(&[("a/boot.elf", b"e"), ("b/data", b"x")]),
            zip(&[("a/boot.elf", b"e"), ("a/boot.dol", b"d")]),
        ] {
            assert!(matches!(
                classify_received_zip(&data),
                Err(WiiloadZipError::InvalidTheme(_))
            ));
        }
    }

    #[test]
    fn rejects_unsafe_path_and_invalid_zip() {
        assert!(classify_received_zip(&zip(&[("a/boot.elf", b"e"), ("/a/file", b"x")])).is_err());
        assert!(classify_received_zip(b"not a zip").is_err());
    }

    #[test]
    fn rejects_hidden_or_oversized_app_folder_names_for_staging() {
        let hidden = zip(&[
            (".hidden/boot.elf", b"elf"),
            (".hidden/meta.xml", b"<app/>"),
        ]);
        assert!(matches!(
            classify_received_zip(&hidden),
            Err(WiiloadZipError::InvalidTheme(_))
        ));

        let long_name = alloc::format!("{}/boot.elf", "a".repeat(244));
        let long_meta = alloc::format!("{}/meta.xml", "a".repeat(244));
        let oversized = zip(&[(&long_name, b"elf"), (&long_meta, b"<app/>")]);
        assert!(matches!(
            classify_received_zip(&oversized),
            Err(WiiloadZipError::InvalidTheme(_))
        ));
    }

    #[test]
    fn recognizes_valid_theme_package_after_app_policy_fails() {
        let xml = b"<theme><description>Water</description></theme>";
        assert!(matches!(
            classify_received_zip(&zip(&[("theme.xml", xml)])),
            Ok(WiiloadZipClassification::Theme { .. })
        ));
    }
}
