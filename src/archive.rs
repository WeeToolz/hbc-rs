//! Archive path validation shared by app and theme installation.

const MAX_ARCHIVE_PATH_BYTES: usize = 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchivePathError {
    Empty,
    TooLong,
    Absolute,
    UnsupportedSeparator,
    InvalidComponent,
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

#[cfg(test)]
mod tests {
    use super::*;

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
