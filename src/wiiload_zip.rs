//! Host-testable validation policy for ZIP files received by Wiiload.

use alloc::{string::String, vec::Vec};

use crate::{
    archive::{ZipArchive, ZipError},
    management::{ExtractionLimits, ExtractionOperation, ExtractionPlan, plan_extraction},
    theme::{ThemePackage, ThemePackageError},
};

pub const WIILOAD_ZIP_MAX_ENTRIES: usize = 1024;
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
    let full_plan = plan_extraction(
        &archive,
        ExtractionLimits {
            max_entries: WIILOAD_ZIP_MAX_ENTRIES,
            max_file_bytes: MAX_FILE_BYTES,
            max_total_bytes: MAX_TOTAL_BYTES,
        },
    )
    .map_err(|_| WiiloadZipError::InvalidApp)?;
    let mut directory: Option<&str> = None;
    let mut executable: Option<(&str, WiiloadZipKind)> = None;
    for entry in entries {
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
            if let Some(kind) = kind
                && executable.replace((entry.name.as_str(), kind)).is_some()
            {
                return Err(WiiloadZipError::InvalidApp);
            }
        }
    }
    let directory = directory.ok_or(WiiloadZipError::InvalidApp)?;
    crate::management::validate_app_install_directory(directory)
        .map_err(|_| WiiloadZipError::InvalidApp)?;
    let (_, kind) = executable.ok_or(WiiloadZipError::InvalidApp)?;
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

    use crate::test_support::zip;

    #[test]
    fn app_plan_rejects_aliases_and_file_directory_collisions() {
        for files in [
            vec![("demo/boot.elf", &b"elf"[..]), ("Demo/meta.xml", &b"x"[..])],
            vec![("demo/boot.elf", &b"elf"[..]), ("demo/data.", &b"x"[..])],
            vec![
                ("demo/boot.elf", &b"elf"[..]),
                ("demo/data", &b"x"[..]),
                ("demo/data/file", &b"y"[..]),
            ],
            vec![
                ("demo/boot.elf", &b"elf"[..]),
                ("demo/boot.elf", &b"elf"[..]),
            ],
        ] {
            assert_eq!(classify_app(&zip(&files)), Err(WiiloadZipError::InvalidApp));
        }
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
    fn classification_enforces_install_folder_byte_bounds_and_keeps_relative_paths() {
        for directory in ["a".repeat(243), alloc::format!("{}a", "é".repeat(121))] {
            let executable = alloc::format!("{directory}/boot.elf");
            let metadata = alloc::format!("{directory}/meta.xml");
            let data = zip(&[(&executable, b"elf"), (&metadata, b"<app/>")]);
            let WiiloadZipClassification::App {
                directory: actual,
                kind,
                extraction,
            } = classify_received_zip(&data).unwrap()
            else {
                panic!("valid app rejected")
            };
            assert_eq!(actual, directory);
            assert_eq!(kind, WiiloadZipKind::Elf);
            assert_eq!(extraction.total_bytes(), 9);
            assert_eq!(
                extraction.operations(),
                &[
                    ExtractionOperation::WriteFile {
                        path: String::from("boot.elf"),
                        archive_name: executable,
                        size: 3
                    },
                    ExtractionOperation::WriteFile {
                        path: String::from("meta.xml"),
                        archive_name: metadata,
                        size: 6
                    },
                ]
            );
            let paths = crate::management::AppInstallPaths::new(&actual).unwrap();
            assert_eq!(paths.stage.strip_prefix("apps/").unwrap().len(), 254);
            assert_eq!(paths.backup.strip_prefix("apps/").unwrap().len(), 255);
        }
        for directory in ["a".repeat(244), "é".repeat(122)] {
            let executable = alloc::format!("{directory}/boot.elf");
            let metadata = alloc::format!("{directory}/meta.xml");
            let data = zip(&[(&executable, b"elf"), (&metadata, b"<app/>")]);
            assert_eq!(classify_app(&data), Err(WiiloadZipError::InvalidApp));
            assert!(classify_received_zip(&data).is_err());
        }
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
