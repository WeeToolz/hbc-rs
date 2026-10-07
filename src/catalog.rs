//! Scan-independent app catalog policy.

use crate::metadata::AppMetadata;
use alloc::{format, string::String, vec::Vec};
use core::cmp::Ordering;

pub const MAX_APP_ENTRIES: usize = 1024;
pub const MAX_APP_DIRECTORY_ENTRIES: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppKind {
    Elf,
    Dol,
    Theme,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppEntry {
    pub directory: String,
    pub kind: AppKind,
    pub size: u64,
    pub has_icon: bool,
    pub metadata: Option<AppMetadata>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Filter {
    #[default]
    All,
    IconsOnly,
    MetadataOnly,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SortOrder {
    #[default]
    Name,
    ReleaseDate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryEntry {
    pub name: String,
    pub is_directory: bool,
}

/// File operations needed to build the app catalog. Keep the scan policy
/// independent from FAT and NAND device details.
pub trait CatalogStorage {
    type Error;

    fn read_dir(&mut self, path: &str) -> Result<Vec<DirectoryEntry>, Self::Error>;
    fn file_size(&mut self, path: &str) -> Result<Option<u64>, Self::Error>;
    fn read_file(&mut self, path: &str, max_bytes: usize) -> Result<Option<Vec<u8>>, Self::Error>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScanError<E> {
    AppDirectory(E),
    Storage { path: String, source: E },
    TooManyAppDirectoryEntries,
}

/// Read app folders below `apps_root` and create catalog entries.
///
/// Missing or invalid optional metadata and icons do not hide a valid app.
pub fn scan_apps<S: CatalogStorage>(
    storage: &mut S,
    apps_root: &str,
    max_executable_size: u64,
    max_theme_size: u64,
) -> Result<Vec<AppEntry>, ScanError<S::Error>> {
    let folders = storage
        .read_dir(apps_root)
        .map_err(ScanError::AppDirectory)?;
    if folders.len() > MAX_APP_DIRECTORY_ENTRIES {
        return Err(ScanError::TooManyAppDirectoryEntries);
    }
    let mut apps = Vec::new();
    for folder in folders {
        if !folder.is_directory || !valid_folder_name(&folder.name) {
            continue;
        }
        let folder_path = join_path(apps_root, &folder.name);
        let candidates = [
            ("boot.elf", AppKind::Elf, max_executable_size),
            ("boot.dol", AppKind::Dol, max_executable_size),
            (
                "theme.zip",
                AppKind::Theme,
                core::cmp::min(max_executable_size, max_theme_size),
            ),
        ];
        let mut selected = None;
        for (name, kind, limit) in &candidates {
            let path = join_path(&folder_path, name);
            if let Some(size) = storage
                .file_size(&path)
                .map_err(|source| ScanError::Storage {
                    path: path.clone(),
                    source,
                })?
            {
                selected = Some((*kind, size, *limit));
                break;
            }
        }
        let Some((kind, size, limit)) = selected else {
            continue;
        };
        if size == 0 || size > limit {
            continue;
        }

        let meta_path = join_path(&folder_path, "meta.xml");
        let metadata = storage
            .read_file(&meta_path, 1024 * 1024)
            .map_err(|source| ScanError::Storage {
                path: meta_path,
                source,
            })?
            .and_then(|bytes| {
                core::str::from_utf8(&bytes)
                    .ok()
                    .and_then(|xml| crate::metadata::parse_app_xml(xml).ok())
            });
        let icon_path = join_path(&folder_path, "icon.png");
        let has_icon = storage
            .read_file(&icon_path, crate::png::HBC_APP_ICON_MAX_PNG_BYTES)
            .map_err(|source| ScanError::Storage {
                path: icon_path,
                source,
            })?
            .is_some_and(|bytes| crate::png::decode_hbc_app_icon(&bytes).is_ok());

        apps.push(AppEntry {
            directory: folder.name,
            kind,
            size,
            has_icon,
            metadata,
        });
        if apps.len() == MAX_APP_ENTRIES {
            break;
        }
    }
    Ok(apps)
}

fn valid_folder_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.starts_with('.')
        && !name.chars().any(|c| matches!(c, '/' | '\\' | ':' | '\0'))
}

fn join_path(parent: &str, child: &str) -> String {
    format!("{}/{}", parent.trim_end_matches('/'), child)
}

/// Return borrowed entries in display order. Date order is newest first.
pub fn visible_entries(entries: &[AppEntry], filter: Filter, sort: SortOrder) -> Vec<&AppEntry> {
    let mut visible: Vec<_> = entries
        .iter()
        .filter(|entry| match filter {
            Filter::All => true,
            Filter::IconsOnly => entry.has_icon,
            Filter::MetadataOnly => entry.metadata.is_some(),
        })
        .collect();
    visible.sort_by(|a, b| compare(a, b, sort));
    visible
}

fn compare(a: &AppEntry, b: &AppEntry, sort: SortOrder) -> Ordering {
    match sort {
        SortOrder::Name => match (&a.metadata, &b.metadata) {
            (Some(ma), Some(mb)) => cmp_optional_text(ma.name.as_deref(), mb.name.as_deref())
                .then_with(|| cmp_text(&a.directory, &b.directory)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => cmp_text(&a.directory, &b.directory),
        },
        SortOrder::ReleaseDate => match (&a.metadata, &b.metadata) {
            (Some(ma), Some(mb)) => mb
                .release_date
                .cmp(&ma.release_date)
                .then_with(|| cmp_optional_text(ma.name.as_deref(), mb.name.as_deref()))
                .then_with(|| cmp_text(&a.directory, &b.directory)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => cmp_text(&a.directory, &b.directory),
        },
    }
}

fn cmp_optional_text(a: Option<&str>, b: Option<&str>) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => cmp_text(a, b),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn cmp_text(a: &str, b: &str) -> Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{collections::BTreeMap, vec, vec::Vec};

    fn valid_hbc_icon_png() -> Vec<u8> {
        fn chunk(kind: &[u8; 4], data: &[u8], output: &mut Vec<u8>) {
            output.extend_from_slice(&(data.len() as u32).to_be_bytes());
            let crc_start = output.len();
            output.extend_from_slice(kind);
            output.extend_from_slice(data);
            let crc = crc32fast::hash(&output[crc_start..]);
            output.extend_from_slice(&crc.to_be_bytes());
        }

        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut header = Vec::new();
        header.extend_from_slice(&128u32.to_be_bytes());
        header.extend_from_slice(&48u32.to_be_bytes());
        header.extend_from_slice(&[8, 0, 0, 0, 0]);
        chunk(b"IHDR", &header, &mut png);

        let raw = vec![0; (128 + 1) * 48];
        let mut zlib = vec![0x78, 0x01];
        let mut offset = 0;
        while offset < raw.len() {
            let count = (raw.len() - offset).min(65535);
            zlib.push(u8::from(offset + count == raw.len()));
            zlib.extend_from_slice(&(count as u16).to_le_bytes());
            zlib.extend_from_slice(&(!(count as u16)).to_le_bytes());
            zlib.extend_from_slice(&raw[offset..offset + count]);
            offset += count;
        }
        let mut a = 1u32;
        let mut b = 0u32;
        for byte in &raw {
            a = (a + u32::from(*byte)) % 65521;
            b = (b + a) % 65521;
        }
        zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());
        chunk(b"IDAT", &zlib, &mut png);
        chunk(b"IEND", &[], &mut png);
        png
    }

    #[derive(Default)]
    struct MemoryStorage {
        directories: BTreeMap<String, Vec<DirectoryEntry>>,
        sizes: BTreeMap<String, u64>,
        files: BTreeMap<String, Vec<u8>>,
        failures: BTreeMap<String, &'static str>,
    }

    impl CatalogStorage for MemoryStorage {
        type Error = &'static str;

        fn read_dir(&mut self, path: &str) -> Result<Vec<DirectoryEntry>, Self::Error> {
            if let Some(error) = self.failures.get(path) {
                return Err(error);
            }
            self.directories
                .get(path)
                .cloned()
                .ok_or("missing directory")
        }

        fn file_size(&mut self, path: &str) -> Result<Option<u64>, Self::Error> {
            if let Some(error) = self.failures.get(path) {
                return Err(error);
            }
            Ok(self.sizes.get(path).copied())
        }

        fn read_file(
            &mut self,
            path: &str,
            max_bytes: usize,
        ) -> Result<Option<Vec<u8>>, Self::Error> {
            if let Some(error) = self.failures.get(path) {
                return Err(error);
            }
            Ok(self
                .files
                .get(path)
                .filter(|bytes| bytes.len() <= max_bytes)
                .cloned())
        }
    }

    fn app(dir: &str, name: Option<&str>, date: Option<u64>, icon: bool) -> AppEntry {
        AppEntry {
            directory: dir.into(),
            kind: AppKind::Elf,
            size: 1,
            has_icon: icon,
            metadata: name.map(|name| AppMetadata {
                name: Some(name.into()),
                author: None,
                version: None,
                short_description: None,
                long_description: None,
                release_date: date,
                arguments: vec![],
                ahb_access: false,
            }),
        }
    }
    #[test]
    fn filters_and_sorts_deterministically() {
        let entries = [
            app("z", Some("Zulu"), Some(20240101), false),
            app("a", Some("alpha"), Some(20250101), true),
            app("b", None, None, true),
        ];
        assert_eq!(
            visible_entries(&entries, Filter::IconsOnly, SortOrder::Name)
                .iter()
                .map(|e| e.directory.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(
            visible_entries(&entries, Filter::MetadataOnly, SortOrder::ReleaseDate)
                .iter()
                .map(|e| e.directory.as_str())
                .collect::<Vec<_>>(),
            ["a", "z"]
        );
        assert_eq!(
            visible_entries(&entries, Filter::All, SortOrder::Name)
                .iter()
                .map(|e| e.directory.as_str())
                .collect::<Vec<_>>(),
            ["a", "z", "b"]
        );
    }

    #[test]
    fn scan_uses_hbc_artifact_priority_and_skips_invalid_folders() {
        let root = "sd:/apps";
        let mut fs = MemoryStorage::default();
        fs.directories.insert(
            root.into(),
            vec![
                DirectoryEntry {
                    name: "elf-first".into(),
                    is_directory: true,
                },
                DirectoryEntry {
                    name: "dol".into(),
                    is_directory: true,
                },
                DirectoryEntry {
                    name: "theme".into(),
                    is_directory: true,
                },
                DirectoryEntry {
                    name: "bad-elf-and-dol".into(),
                    is_directory: true,
                },
                DirectoryEntry {
                    name: ".hidden".into(),
                    is_directory: true,
                },
                DirectoryEntry {
                    name: "../escape".into(),
                    is_directory: true,
                },
                DirectoryEntry {
                    name: "plain-file".into(),
                    is_directory: false,
                },
            ],
        );
        fs.sizes.insert(format!("{root}/elf-first/boot.elf"), 10);
        fs.sizes.insert(format!("{root}/elf-first/boot.dol"), 12);
        fs.sizes.insert(format!("{root}/dol/boot.dol"), 12);
        fs.sizes.insert(format!("{root}/theme/theme.zip"), 30);
        fs.sizes
            .insert(format!("{root}/bad-elf-and-dol/boot.elf"), 101);
        fs.sizes
            .insert(format!("{root}/bad-elf-and-dol/boot.dol"), 12);
        fs.sizes.insert(
            format!("{root}/elf-first/icon.png"),
            valid_hbc_icon_png().len() as u64,
        );
        fs.files
            .insert(format!("{root}/elf-first/icon.png"), valid_hbc_icon_png());
        fs.sizes.insert(format!("{root}/dol/icon.png"), 3);
        fs.files
            .insert(format!("{root}/dol/icon.png"), b"bad".to_vec());
        fs.files.insert(
            format!("{root}/elf-first/meta.xml"),
            b"<app><name>ELF</name></app>".to_vec(),
        );

        let apps = scan_apps(&mut fs, root, 100, 50).unwrap();
        assert_eq!(apps.len(), 3);
        assert_eq!(apps[0].kind, AppKind::Elf);
        assert_eq!(
            apps[0].metadata.as_ref().unwrap().name.as_deref(),
            Some("ELF")
        );
        assert!(apps[0].has_icon);
        assert_eq!(apps[1].kind, AppKind::Dol);
        assert!(!apps[1].has_icon);
        assert_eq!(apps[2].kind, AppKind::Theme);
    }

    #[test]
    fn scan_rejects_theme_above_executable_size_limit() {
        let root = "sd:/apps";
        let mut fs = MemoryStorage::default();
        fs.directories.insert(
            root.into(),
            vec![DirectoryEntry {
                name: "large-theme".into(),
                is_directory: true,
            }],
        );
        fs.sizes.insert(format!("{root}/large-theme/theme.zip"), 51);

        let apps = scan_apps(&mut fs, root, 50, 100).unwrap();
        assert!(apps.is_empty());
    }

    #[test]
    fn scan_rejects_an_unbounded_backend_directory_result() {
        let root = "sd:/apps";
        let mut fs = MemoryStorage::default();
        fs.directories.insert(
            root.into(),
            (0..=MAX_APP_DIRECTORY_ENTRIES)
                .map(|index| DirectoryEntry {
                    name: format!("app-{index}"),
                    is_directory: true,
                })
                .collect(),
        );
        assert_eq!(
            scan_apps(&mut fs, root, 100, 50),
            Err(ScanError::TooManyAppDirectoryEntries)
        );
    }

    #[test]
    fn scan_caps_catalog_entries_at_the_c_hbc_limit() {
        let root = "sd:/apps";
        let mut fs = MemoryStorage::default();
        fs.directories.insert(
            root.into(),
            (0..=MAX_APP_ENTRIES)
                .map(|index| DirectoryEntry {
                    name: format!("app-{index}"),
                    is_directory: true,
                })
                .collect(),
        );
        for index in 0..=MAX_APP_ENTRIES {
            fs.sizes.insert(format!("{root}/app-{index}/boot.dol"), 10);
        }

        let apps = scan_apps(&mut fs, root, 100, 50).unwrap();
        assert_eq!(apps.len(), MAX_APP_ENTRIES);
        assert_eq!(apps[0].directory, "app-0");
    }

    #[test]
    fn scan_returns_candidate_storage_failures() {
        let root = "sd:/apps";
        let mut fs = MemoryStorage::default();
        fs.directories.insert(
            root.into(),
            vec![DirectoryEntry {
                name: "demo".into(),
                is_directory: true,
            }],
        );
        let path = format!("{root}/demo/boot.elf");
        fs.failures.insert(path.clone(), "io failure");

        assert_eq!(
            scan_apps(&mut fs, root, 100, 50),
            Err(ScanError::Storage {
                path,
                source: "io failure",
            })
        );
    }

    #[test]
    fn scan_returns_optional_file_read_failures() {
        let root = "sd:/apps";
        let mut fs = MemoryStorage::default();
        fs.directories.insert(
            root.into(),
            vec![DirectoryEntry {
                name: "demo".into(),
                is_directory: true,
            }],
        );
        fs.sizes.insert(format!("{root}/demo/boot.dol"), 10);
        let path = format!("{root}/demo/meta.xml");
        fs.failures.insert(path.clone(), "io failure");

        assert_eq!(
            scan_apps(&mut fs, root, 100, 50),
            Err(ScanError::Storage {
                path,
                source: "io failure",
            })
        );
    }
}
