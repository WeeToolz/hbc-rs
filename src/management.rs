//! Hardware-independent policy for app and theme archive management.

use crate::archive::{validate_archive_path, ArchivePathError, ZipArchive, ZipEntry};
use alloc::{collections::BTreeMap, string::String, vec::Vec};

/// Resource limits for one archive extraction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExtractionLimits {
    pub max_entries: usize,
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
}

impl Default for ExtractionLimits {
    fn default() -> Self {
        Self {
            max_entries: 4096,
            max_file_bytes: 64 * 1024 * 1024,
            max_total_bytes: 128 * 1024 * 1024,
        }
    }
}

/// A filesystem operation required to extract an archive.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExtractionOperation {
    CreateDirectory {
        path: String,
    },
    WriteFile {
        path: String,
        archive_name: String,
        size: u64,
    },
}

/// A validated, bounded extraction plan. It does not access storage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtractionPlan {
    operations: Vec<ExtractionOperation>,
    total_bytes: u64,
}

impl ExtractionPlan {
    pub(crate) fn from_operations(operations: Vec<ExtractionOperation>, total_bytes: u64) -> Self {
        Self {
            operations,
            total_bytes,
        }
    }

    pub fn operations(&self) -> &[ExtractionOperation] {
        &self.operations
    }

    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagementError {
    TooManyEntries,
    UnsafePath(ArchivePathError),
    UnsupportedEntry,
    DuplicatePath,
    PathCollision,
    FileTooLarge,
    TotalOutputTooLarge,
    SizeOverflow,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DirectoryCommitError<E> {
    Install(E),
    Restore { install: E, restore: E },
}

/// Move a fully prepared app folder into place while retaining the prior
/// folder until the caller validates the refreshed catalog.
pub fn commit_staged_directory<E>(
    had_previous: bool,
    target: &str,
    stage: &str,
    backup: &str,
    mut rename: impl FnMut(&str, &str) -> Result<(), E>,
) -> Result<(), DirectoryCommitError<E>> {
    if had_previous {
        rename(target, backup).map_err(DirectoryCommitError::Install)?;
    }
    if let Err(install) = rename(stage, target) {
        if had_previous {
            if let Err(restore) = rename(backup, target) {
                return Err(DirectoryCommitError::Restore { install, restore });
            }
        }
        return Err(DirectoryCommitError::Install(install));
    }
    Ok(())
}

/// Restore the old app folder after the new folder fails catalog validation.
pub fn rollback_staged_directory<E>(
    had_previous: bool,
    target: &str,
    stage: &str,
    backup: &str,
    mut rename: impl FnMut(&str, &str) -> Result<(), E>,
) -> Result<(), E> {
    rename(target, stage)?;
    if had_previous {
        rename(backup, target)?;
    }
    Ok(())
}

/// A validated immediate child of the HBC apps directory for recursive removal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppDeletionPlan {
    pub directory: String,
}

/// Plan HBC's recursive app-folder deletion after validating the folder name.
///
/// The C implementation joins the selected `dirname` to `app_path` and then
/// recursively unlinks its contents. This check keeps that operation inside
/// the apps root when a catalog name is stale or comes from an invalid source.
pub fn plan_app_deletion(directory: &str) -> Result<AppDeletionPlan, ArchivePathError> {
    validate_archive_path(directory)?;
    if directory.contains('/') || directory.starts_with('.') {
        return Err(ArchivePathError::InvalidComponent);
    }
    Ok(AppDeletionPlan {
        directory: String::from(directory),
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NodeKind {
    Directory,
    File,
}

#[derive(Clone, Copy, Debug)]
struct Node {
    kind: NodeKind,
    explicit: bool,
}

/// Build a safe extraction plan for an app or theme archive.
///
/// ZIP entries must use stored or DEFLATE compression. Encrypted entries are
/// rejected. Explicit directories and implied parent directories are created
/// before files. Duplicate paths and file/directory prefix collisions fail the
/// whole plan, so callers can validate the complete archive before writing.
pub fn plan_extraction(
    archive: &ZipArchive,
    limits: ExtractionLimits,
) -> Result<ExtractionPlan, ManagementError> {
    let entries = archive.entries();
    if entries.len() > limits.max_entries {
        return Err(ManagementError::TooManyEntries);
    }

    let mut nodes = BTreeMap::<String, Node>::new();
    let mut files = Vec::new();
    let mut total_bytes = 0u64;

    for entry in entries {
        validate_entry(entry)?;
        let normalized = entry.name.strip_suffix('/').unwrap_or(&entry.name);
        let kind = if entry.is_directory {
            NodeKind::Directory
        } else {
            NodeKind::File
        };

        add_parent_directories(normalized, &mut nodes)?;
        match nodes.get_mut(normalized) {
            Some(node) if node.kind != kind => return Err(ManagementError::PathCollision),
            Some(node) if node.explicit => return Err(ManagementError::DuplicatePath),
            Some(node) => node.explicit = true,
            None => {
                nodes.insert(
                    String::from(normalized),
                    Node {
                        kind,
                        explicit: true,
                    },
                );
            }
        }

        if kind == NodeKind::File {
            let size = u64::from(entry.uncompressed_size);
            if size > limits.max_file_bytes {
                return Err(ManagementError::FileTooLarge);
            }
            total_bytes = total_bytes
                .checked_add(size)
                .ok_or(ManagementError::SizeOverflow)?;
            if total_bytes > limits.max_total_bytes {
                return Err(ManagementError::TotalOutputTooLarge);
            }
            files.push(ExtractionOperation::WriteFile {
                path: String::from(normalized),
                archive_name: entry.name.clone(),
                size,
            });
        }
    }

    let mut directories: Vec<String> = nodes
        .into_iter()
        .filter_map(|(path, node)| (node.kind == NodeKind::Directory).then_some(path))
        .collect();
    directories.sort_by(|a, b| {
        component_depth(a)
            .cmp(&component_depth(b))
            .then_with(|| a.cmp(b))
    });

    let mut operations = Vec::with_capacity(directories.len() + files.len());
    operations.extend(
        directories
            .into_iter()
            .map(|path| ExtractionOperation::CreateDirectory { path }),
    );
    operations.extend(files);
    Ok(ExtractionPlan {
        operations,
        total_bytes,
    })
}

fn validate_entry(entry: &ZipEntry) -> Result<(), ManagementError> {
    validate_archive_path(&entry.name).map_err(ManagementError::UnsafePath)?;
    if !entry.is_supported_type
        || entry.flags & (0x0001 | 0x0040 | 0x2000) != 0
        || !matches!(entry.compression_method, 0 | 8)
    {
        return Err(ManagementError::UnsupportedEntry);
    }
    if entry.is_directory != entry.name.ends_with('/')
        || (entry.is_directory && (entry.uncompressed_size != 0 || entry.compressed_size != 0))
    {
        return Err(ManagementError::UnsupportedEntry);
    }
    Ok(())
}

fn add_parent_directories(
    path: &str,
    nodes: &mut BTreeMap<String, Node>,
) -> Result<(), ManagementError> {
    let mut parent_end = 0;
    while let Some(separator) = path[parent_end..].find('/') {
        parent_end += separator;
        let parent = &path[..parent_end];
        match nodes.get(parent) {
            Some(node) if node.kind == NodeKind::File => {
                return Err(ManagementError::PathCollision)
            }
            Some(_) => {}
            None => {
                nodes.insert(
                    String::from(parent),
                    Node {
                        kind: NodeKind::Directory,
                        explicit: false,
                    },
                );
            }
        }
        parent_end += 1;
    }
    Ok(())
}

fn component_depth(path: &str) -> usize {
    path.bytes().filter(|byte| *byte == b'/').count() + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::ZipEntry;

    fn entry(name: &str, is_directory: bool, size: u32) -> ZipEntry {
        ZipEntry {
            name: String::from(name),
            flags: 0,
            compression_method: 8,
            crc32: 0,
            compressed_size: size,
            uncompressed_size: size,
            local_header_offset: 0,
            is_directory,
            is_supported_type: true,
        }
    }

    fn archive(entries: &[ZipEntry]) -> ZipArchive {
        ZipArchive::from_entries_for_test(entries.to_vec())
    }

    #[test]
    fn plans_parent_directories_before_files_and_counts_output() {
        let zip = archive(&[
            entry("apps/demo/meta.xml", false, 5),
            entry("apps/demo/assets/icon.bin", false, 7),
            entry("apps/demo/", true, 0),
        ]);
        let plan = plan_extraction(&zip, ExtractionLimits::default()).unwrap();
        assert_eq!(plan.total_bytes(), 12);
        assert_eq!(
            plan.operations(),
            &[
                ExtractionOperation::CreateDirectory {
                    path: String::from("apps")
                },
                ExtractionOperation::CreateDirectory {
                    path: String::from("apps/demo")
                },
                ExtractionOperation::CreateDirectory {
                    path: String::from("apps/demo/assets")
                },
                ExtractionOperation::WriteFile {
                    path: String::from("apps/demo/meta.xml"),
                    archive_name: String::from("apps/demo/meta.xml"),
                    size: 5
                },
                ExtractionOperation::WriteFile {
                    path: String::from("apps/demo/assets/icon.bin"),
                    archive_name: String::from("apps/demo/assets/icon.bin"),
                    size: 7
                }
            ]
        );
    }

    #[test]
    fn rejects_duplicate_entries_and_file_directory_collisions() {
        let duplicate = archive(&[
            entry("apps/demo/file", false, 1),
            entry("apps/demo/file", false, 1),
        ]);
        assert_eq!(
            plan_extraction(&duplicate, ExtractionLimits::default()),
            Err(ManagementError::DuplicatePath)
        );

        let collision = archive(&[
            entry("apps/demo", false, 1),
            entry("apps/demo/file", false, 1),
        ]);
        assert_eq!(
            plan_extraction(&collision, ExtractionLimits::default()),
            Err(ManagementError::PathCollision)
        );
    }

    #[test]
    fn enforces_per_file_total_and_entry_limits() {
        let zip = archive(&[entry("a", false, 6), entry("b", false, 5)]);
        let limits = ExtractionLimits {
            max_file_bytes: 6,
            max_total_bytes: 10,
            ..ExtractionLimits::default()
        };
        assert_eq!(
            plan_extraction(&zip, limits),
            Err(ManagementError::TotalOutputTooLarge)
        );
        assert_eq!(
            plan_extraction(
                &zip,
                ExtractionLimits {
                    max_entries: 1,
                    ..ExtractionLimits::default()
                }
            ),
            Err(ManagementError::TooManyEntries)
        );
    }

    #[test]
    fn rejects_unsafe_encrypted_and_unsupported_entries() {
        let unsafe_path = archive(&[entry("../escape", false, 1)]);
        assert!(matches!(
            plan_extraction(&unsafe_path, ExtractionLimits::default()),
            Err(ManagementError::UnsafePath(_))
        ));

        let mut encrypted_entry = entry("secret", false, 1);
        encrypted_entry.flags = 1;
        let encrypted = archive(&[encrypted_entry]);
        assert_eq!(
            plan_extraction(&encrypted, ExtractionLimits::default()),
            Err(ManagementError::UnsupportedEntry)
        );

        let mut unknown_method_entry = entry("file", false, 1);
        unknown_method_entry.compression_method = 12;
        let unknown_method = archive(&[unknown_method_entry]);
        assert_eq!(
            plan_extraction(&unknown_method, ExtractionLimits::default()),
            Err(ManagementError::UnsupportedEntry)
        );

        let mut symlink = entry("link", false, 1);
        symlink.is_supported_type = false;
        let special_entry = archive(&[symlink]);
        assert_eq!(
            plan_extraction(&special_entry, ExtractionLimits::default()),
            Err(ManagementError::UnsupportedEntry)
        );
    }

    #[test]
    fn app_deletion_accepts_only_one_safe_folder_name() {
        assert_eq!(
            plan_app_deletion("demo"),
            Ok(AppDeletionPlan {
                directory: String::from("demo")
            })
        );
        assert!(plan_app_deletion("../other").is_err());
        assert!(plan_app_deletion("demo/nested").is_err());
        assert!(plan_app_deletion(".hidden").is_err());
    }

    #[test]
    fn staged_app_commit_restores_the_old_folder_when_install_rename_fails() {
        let mut folders = BTreeMap::new();
        folders.insert(String::from("apps/demo"), b"old executable".to_vec());
        folders.insert(
            String::from("apps/.demo.hbc-stage"),
            b"new executable".to_vec(),
        );
        let mut rename_count = 0;
        let result = commit_staged_directory(
            true,
            "apps/demo",
            "apps/.demo.hbc-stage",
            "apps/.demo.hbc-backup",
            |old, new| {
                rename_count += 1;
                if rename_count == 2 {
                    return Err(-8);
                }
                let bytes = folders.remove(old).ok_or(-6)?;
                if folders.contains_key(new) {
                    folders.insert(String::from(old), bytes);
                    return Err(-9);
                }
                folders.insert(String::from(new), bytes);
                Ok(())
            },
        );
        assert_eq!(result, Err(DirectoryCommitError::Install(-8)));
        assert_eq!(folders.get("apps/demo"), Some(&b"old executable".to_vec()));
        assert_eq!(
            folders.get("apps/.demo.hbc-stage"),
            Some(&b"new executable".to_vec())
        );
        assert!(!folders.contains_key("apps/.demo.hbc-backup"));
    }

    #[test]
    fn staged_app_commit_keeps_old_data_in_backup_if_restore_fails() {
        let mut folders = BTreeMap::new();
        folders.insert(String::from("apps/demo"), b"old executable".to_vec());
        folders.insert(
            String::from("apps/.demo.hbc-stage"),
            b"new executable".to_vec(),
        );
        let mut rename_count = 0;
        let result = commit_staged_directory(
            true,
            "apps/demo",
            "apps/.demo.hbc-stage",
            "apps/.demo.hbc-backup",
            |old, new| {
                rename_count += 1;
                if rename_count >= 2 {
                    return Err(-8);
                }
                let bytes = folders.remove(old).ok_or(-6)?;
                folders.insert(String::from(new), bytes);
                Ok(())
            },
        );
        assert_eq!(
            result,
            Err(DirectoryCommitError::Restore {
                install: -8,
                restore: -8
            })
        );
        assert!(!folders.contains_key("apps/demo"));
        assert_eq!(
            folders.get("apps/.demo.hbc-backup"),
            Some(&b"old executable".to_vec())
        );
    }
}
