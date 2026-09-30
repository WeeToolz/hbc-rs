//! NAND storage for the HBC installed theme ZIP.

use alloc::vec::Vec;

use crate::{
    platform::settings::{title_data_file_path, SettingsIoError},
    theme::MAX_THEME_ARCHIVE_BYTES,
};

use ogc_cache::dc_flush_range;
use ogc_filesystem as isfs;

const THEME_FILENAME: &str = "theme.zip";
const IO_CHUNK_SIZE: usize = 16 * 1024;
const BACKGROUND_WIDTH: u32 = 640;
const BACKGROUND_HEIGHT: u32 = 480;
const THEME_IMAGE_MAX_BYTES: usize = 8 * 1024 * 1024;

const DEFAULT_BACKGROUND: &[u8] = include_bytes!("../../assets/background.png");

#[repr(align(32))]
struct AlignedChunk([u8; IO_CHUNK_SIZE]);

/// Read the installed theme archive from this channel's NAND data directory.
pub fn load() -> Result<Option<Vec<u8>>, SettingsIoError> {
    let path = title_data_file_path(THEME_FILENAME)?;
    let rc = isfs::initialize();
    if rc < 0 {
        return Err(SettingsIoError::Isfs(rc));
    }
    let file = isfs::open(&path, isfs::ISFS_OPEN_READ).map_err(SettingsIoError::Isfs)?;
    let mut stats = AlignedStats(isfs::Fstats::default());
    let rc = isfs::get_file_stats(&file, &mut stats.0);
    if rc < 0 {
        let _ = file.close();
        return Err(SettingsIoError::Isfs(rc));
    }
    let size = stats.0.file_length as usize;
    if size == 0 || size > MAX_THEME_ARCHIVE_BYTES {
        let _ = file.close();
        return Err(SettingsIoError::InvalidFileSize);
    }

    let mut archive = Vec::new();
    if archive.try_reserve_exact(size).is_err() {
        let _ = file.close();
        return Err(SettingsIoError::InvalidFileSize);
    }
    let mut chunk = AlignedChunk([0; IO_CHUNK_SIZE]);
    let mut remaining = size;
    while remaining > 0 {
        let requested = remaining.min(IO_CHUNK_SIZE);
        match file.read(&mut chunk.0[..requested]) {
            Ok(0) => {
                let _ = file.close();
                return Err(SettingsIoError::InvalidFileSize);
            }
            Ok(count) if count <= requested => {
                archive.extend_from_slice(&chunk.0[..count]);
                remaining -= count;
            }
            Ok(_) => {
                let _ = file.close();
                return Err(SettingsIoError::InvalidFileSize);
            }
            Err(rc) => {
                let _ = file.close();
                return Err(SettingsIoError::Isfs(rc));
            }
        }
    }
    let rc = file.close();
    if rc < 0 {
        return Err(SettingsIoError::Isfs(rc));
    }
    Ok(Some(archive))
}

/// Replace the installed theme archive after validating it in the caller.
pub fn save(archive: &[u8]) -> Result<(), SettingsIoError> {
    if archive.is_empty() || archive.len() > MAX_THEME_ARCHIVE_BYTES {
        return Err(SettingsIoError::InvalidFileSize);
    }
    let path = title_data_file_path(THEME_FILENAME)?;
    let rc = isfs::initialize();
    if rc < 0 {
        return Err(SettingsIoError::Isfs(rc));
    }

    // ISFS has no truncate open mode. Match HBC's delete, create, then write sequence.
    let _ = isfs::delete(&path);
    let rc = isfs::create_file(&path, 0, 3, 3, 0);
    if rc < 0 {
        return Err(SettingsIoError::Isfs(rc));
    }
    let file = match isfs::open(&path, isfs::ISFS_OPEN_RW) {
        Ok(file) => file,
        Err(rc) => {
            let _ = isfs::delete(&path);
            return Err(SettingsIoError::Isfs(rc));
        }
    };

    let mut chunk = AlignedChunk([0; IO_CHUNK_SIZE]);
    let mut result = Ok(());
    for bytes in archive.chunks(IO_CHUNK_SIZE) {
        chunk.0[..bytes.len()].copy_from_slice(bytes);
        unsafe { dc_flush_range(chunk.0.as_ptr().cast(), bytes.len() as u32) };
        if let Err(rc) = file.write_all(&chunk.0[..bytes.len()]) {
            result = Err(rc);
            break;
        }
    }
    let close_rc = file.close();
    if let Err(rc) = result {
        let _ = isfs::delete(&path);
        return Err(SettingsIoError::Isfs(rc));
    }
    if close_rc < 0 {
        let _ = isfs::delete(&path);
        return Err(SettingsIoError::Isfs(close_rc));
    }
    Ok(())
}

#[repr(align(32))]
struct AlignedStats(isfs::Fstats);

/// Decode a theme background, or use HBC's built-in background when the theme
/// does not contain a valid image.
pub fn background_texture(
    package: Option<(&crate::theme::ThemePackage, &[u8])>,
) -> Option<crate::png::GxRgba8Texture> {
    package
        .and_then(|(package, archive)| {
            package
                .extract_file(archive, "background.png", THEME_IMAGE_MAX_BYTES)
                .ok()
                .and_then(|bytes| decode_background(&bytes))
        })
        .or_else(|| decode_background(DEFAULT_BACKGROUND))
}

fn decode_background(data: &[u8]) -> Option<crate::png::GxRgba8Texture> {
    let image = crate::png::decode_png(
        data,
        crate::png::PngLimits {
            max_width: BACKGROUND_WIDTH,
            max_height: BACKGROUND_HEIGHT,
            max_pixels: (BACKGROUND_WIDTH * BACKGROUND_HEIGHT) as usize,
            max_input_bytes: crate::theme::MAX_THEME_ARCHIVE_BYTES,
        },
    )
    .ok()?;
    if image.width != BACKGROUND_WIDTH || image.height != BACKGROUND_HEIGHT {
        return None;
    }
    let texture = crate::png::GxRgba8Texture::from_image(&image).ok()?;
    if let Ok(byte_len) = u32::try_from(texture.len()) {
        unsafe { ogc_cache::dc_flush_range(texture.as_ptr().cast(), byte_len) };
        Some(texture)
    } else {
        None
    }
}
