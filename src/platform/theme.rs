//! NAND storage for the HBC installed theme ZIP.

use alloc::vec::Vec;

use crate::{
    platform::settings::{title_data_file_path, SettingsIoError},
    theme::MAX_THEME_ARCHIVE_BYTES,
};

use ogc_filesystem as isfs;

const THEME_FILENAME: &str = "theme.zip";
const IO_CHUNK_SIZE: usize = 16 * 1024;
const BACKGROUND_WIDTH: u32 = 640;
const WIDE_BACKGROUND_WIDTH: u32 = 852;
const BACKGROUND_HEIGHT: u32 = 480;
const LOGO_WIDTH: u32 = 320;
const LOGO_HEIGHT: u32 = 64;
const LIST_CARD_WIDTH: u32 = 432;
const LIST_CARD_HEIGHT: u32 = 64;
const GRID_CARD_WIDTH: u32 = 144;
const GRID_CARD_HEIGHT: u32 = 64;
const PAGE_ARROW_WIDTH: u32 = 64;
const PAGE_ARROW_HEIGHT: u32 = 64;
const DIALOG_WIDTH: u32 = 520;
const DIALOG_HEIGHT: u32 = 360;
const DIALOG_ICON_SIZE: u32 = 32;
const LARGE_BUTTON_WIDTH: u32 = 340;
const SMALL_BUTTON_WIDTH: u32 = 200;
const TINY_BUTTON_WIDTH: u32 = 132;
const BUTTON_HEIGHT: u32 = 48;
const ABOUT_WIDTH: u32 = 236;
const ABOUT_HEIGHT: u32 = 104;
const THEME_IMAGE_MAX_BYTES: usize = 8 * 1024 * 1024;

const DEFAULT_BACKGROUND: &[u8] = include_bytes!("../../assets/background.png");
const DEFAULT_WIDE_BACKGROUND: &[u8] = include_bytes!("../../assets/background_wide.png");
const DEFAULT_LOGO: &[u8] = include_bytes!("../../assets/logo.png");
const DEFAULT_LIST_CARD: &[u8] = include_bytes!("../../assets/apps_list.png");
const DEFAULT_LIST_CARD_FOCUS: &[u8] = include_bytes!("../../assets/apps_list_hover.png");
const DEFAULT_GRID_CARD: &[u8] = include_bytes!("../../assets/apps_grid.png");
const DEFAULT_GRID_CARD_FOCUS: &[u8] = include_bytes!("../../assets/apps_grid_hover.png");
const DEFAULT_PREVIOUS_ARROW: &[u8] = include_bytes!("../../assets/apps_previous.png");
const DEFAULT_PREVIOUS_ARROW_FOCUS: &[u8] = include_bytes!("../../assets/apps_previous_hover.png");
const DEFAULT_NEXT_ARROW: &[u8] = include_bytes!("../../assets/apps_next.png");
const DEFAULT_NEXT_ARROW_FOCUS: &[u8] = include_bytes!("../../assets/apps_next_hover.png");
const DEFAULT_DIALOG: &[u8] = include_bytes!("../../assets/dialog_background.png");
const DEFAULT_DIALOG_INFO: &[u8] = include_bytes!("../../assets/dlg_info.png");
const DEFAULT_DIALOG_CONFIRM: &[u8] = include_bytes!("../../assets/dlg_confirm.png");
const DEFAULT_DIALOG_WARNING: &[u8] = include_bytes!("../../assets/dlg_warning.png");
const DEFAULT_DIALOG_ERROR: &[u8] = include_bytes!("../../assets/dlg_error.png");
const DEFAULT_ABOUT: &[u8] = include_bytes!("../../assets/about.png");
const DEFAULT_LARGE_BUTTON: &[u8] = include_bytes!("../../assets/button.png");
const DEFAULT_LARGE_BUTTON_FOCUS: &[u8] = include_bytes!("../../assets/button_focus.png");
const DEFAULT_SMALL_BUTTON: &[u8] = include_bytes!("../../assets/button_small.png");
const DEFAULT_SMALL_BUTTON_FOCUS: &[u8] = include_bytes!("../../assets/button_small_focus.png");
const DEFAULT_TINY_BUTTON: &[u8] = include_bytes!("../../assets/button_tiny.png");
const DEFAULT_TINY_BUTTON_FOCUS: &[u8] = include_bytes!("../../assets/button_tiny_focus.png");

pub struct ThemeTextures {
    pub background: Option<crate::png::GxRgba8Texture>,
    pub background_width: u32,
    pub logo: Option<crate::png::GxRgba8Texture>,
    pub list_card: Option<crate::png::GxRgba8Texture>,
    pub list_card_focused: Option<crate::png::GxRgba8Texture>,
    pub grid_card: Option<crate::png::GxRgba8Texture>,
    pub grid_card_focused: Option<crate::png::GxRgba8Texture>,
    pub previous_arrow: Option<crate::png::GxRgba8Texture>,
    pub previous_arrow_focused: Option<crate::png::GxRgba8Texture>,
    pub next_arrow: Option<crate::png::GxRgba8Texture>,
    pub next_arrow_focused: Option<crate::png::GxRgba8Texture>,
    pub dialog: Option<crate::png::GxRgba8Texture>,
    pub dialog_info: Option<crate::png::GxRgba8Texture>,
    pub dialog_confirm: Option<crate::png::GxRgba8Texture>,
    pub dialog_warning: Option<crate::png::GxRgba8Texture>,
    pub dialog_error: Option<crate::png::GxRgba8Texture>,
    pub large_button: Option<crate::png::GxRgba8Texture>,
    pub large_button_focused: Option<crate::png::GxRgba8Texture>,
    pub small_button: Option<crate::png::GxRgba8Texture>,
    pub small_button_focused: Option<crate::png::GxRgba8Texture>,
    pub tiny_button: Option<crate::png::GxRgba8Texture>,
    pub tiny_button_focused: Option<crate::png::GxRgba8Texture>,
    pub about: Option<crate::png::GxRgba8Texture>,
}

impl ThemeTextures {
    pub fn load(package: Option<(&crate::theme::ThemePackage, &[u8])>) -> Self {
        Self::load_for_viewport(package, false)
    }

    pub fn load_for_viewport(
        package: Option<(&crate::theme::ThemePackage, &[u8])>,
        widescreen: bool,
    ) -> Self {
        let (background, background_width) = load_background(package, widescreen);
        Self {
            background,
            background_width,
            logo: theme_image_texture(package, "logo.png", LOGO_WIDTH, LOGO_HEIGHT, DEFAULT_LOGO),
            list_card: theme_image_texture(
                package,
                "apps_list.png",
                LIST_CARD_WIDTH,
                LIST_CARD_HEIGHT,
                DEFAULT_LIST_CARD,
            ),
            list_card_focused: theme_image_texture(
                package,
                "apps_list_hover.png",
                LIST_CARD_WIDTH,
                LIST_CARD_HEIGHT,
                DEFAULT_LIST_CARD_FOCUS,
            ),
            grid_card: theme_image_texture(
                package,
                "apps_grid.png",
                GRID_CARD_WIDTH,
                GRID_CARD_HEIGHT,
                DEFAULT_GRID_CARD,
            ),
            grid_card_focused: theme_image_texture(
                package,
                "apps_grid_hover.png",
                GRID_CARD_WIDTH,
                GRID_CARD_HEIGHT,
                DEFAULT_GRID_CARD_FOCUS,
            ),
            previous_arrow: theme_image_texture(
                package,
                "apps_previous.png",
                PAGE_ARROW_WIDTH,
                PAGE_ARROW_HEIGHT,
                DEFAULT_PREVIOUS_ARROW,
            ),
            previous_arrow_focused: theme_image_texture(
                package,
                "apps_previous_hover.png",
                PAGE_ARROW_WIDTH,
                PAGE_ARROW_HEIGHT,
                DEFAULT_PREVIOUS_ARROW_FOCUS,
            ),
            next_arrow: theme_image_texture(
                package,
                "apps_next.png",
                PAGE_ARROW_WIDTH,
                PAGE_ARROW_HEIGHT,
                DEFAULT_NEXT_ARROW,
            ),
            next_arrow_focused: theme_image_texture(
                package,
                "apps_next_hover.png",
                PAGE_ARROW_WIDTH,
                PAGE_ARROW_HEIGHT,
                DEFAULT_NEXT_ARROW_FOCUS,
            ),
            dialog: theme_image_texture(
                package,
                "dialog_background.png",
                DIALOG_WIDTH,
                DIALOG_HEIGHT,
                DEFAULT_DIALOG,
            ),
            dialog_info: theme_image_texture(
                package,
                "dlg_info.png",
                DIALOG_ICON_SIZE,
                DIALOG_ICON_SIZE,
                DEFAULT_DIALOG_INFO,
            ),
            dialog_confirm: theme_image_texture(
                package,
                "dlg_confirm.png",
                DIALOG_ICON_SIZE,
                DIALOG_ICON_SIZE,
                DEFAULT_DIALOG_CONFIRM,
            ),
            dialog_warning: theme_image_texture(
                package,
                "dlg_warning.png",
                DIALOG_ICON_SIZE,
                DIALOG_ICON_SIZE,
                DEFAULT_DIALOG_WARNING,
            ),
            dialog_error: theme_image_texture(
                package,
                "dlg_error.png",
                DIALOG_ICON_SIZE,
                DIALOG_ICON_SIZE,
                DEFAULT_DIALOG_ERROR,
            ),
            large_button: theme_image_texture(
                package,
                "button.png",
                LARGE_BUTTON_WIDTH,
                BUTTON_HEIGHT,
                DEFAULT_LARGE_BUTTON,
            ),
            large_button_focused: theme_image_texture(
                package,
                "button_focus.png",
                LARGE_BUTTON_WIDTH,
                BUTTON_HEIGHT,
                DEFAULT_LARGE_BUTTON_FOCUS,
            ),
            small_button: theme_image_texture(
                package,
                "button_small.png",
                SMALL_BUTTON_WIDTH,
                BUTTON_HEIGHT,
                DEFAULT_SMALL_BUTTON,
            ),
            small_button_focused: theme_image_texture(
                package,
                "button_small_focus.png",
                SMALL_BUTTON_WIDTH,
                BUTTON_HEIGHT,
                DEFAULT_SMALL_BUTTON_FOCUS,
            ),
            tiny_button: theme_image_texture(
                package,
                "button_tiny.png",
                TINY_BUTTON_WIDTH,
                BUTTON_HEIGHT,
                DEFAULT_TINY_BUTTON,
            ),
            tiny_button_focused: theme_image_texture(
                package,
                "button_tiny_focus.png",
                TINY_BUTTON_WIDTH,
                BUTTON_HEIGHT,
                DEFAULT_TINY_BUTTON_FOCUS,
            ),
            about: theme_image_texture(None, "about.png", ABOUT_WIDTH, ABOUT_HEIGHT, DEFAULT_ABOUT),
        }
    }
}

fn load_background(
    package: Option<(&crate::theme::ThemePackage, &[u8])>,
    widescreen: bool,
) -> (Option<crate::png::GxRgba8Texture>, u32) {
    if widescreen {
        if let Some(texture) = package_image(
            package,
            "background_wide.png",
            WIDE_BACKGROUND_WIDTH,
            BACKGROUND_HEIGHT,
        ) {
            return (Some(texture), WIDE_BACKGROUND_WIDTH);
        }
        if let Some(texture) = package_image(
            package,
            "background.png",
            BACKGROUND_WIDTH,
            BACKGROUND_HEIGHT,
        ) {
            return (Some(texture), BACKGROUND_WIDTH);
        }
        return (
            decode_fixed_size_image(
                DEFAULT_WIDE_BACKGROUND,
                WIDE_BACKGROUND_WIDTH,
                BACKGROUND_HEIGHT,
            ),
            WIDE_BACKGROUND_WIDTH,
        );
    }

    (
        theme_image_texture(
            package,
            "background.png",
            BACKGROUND_WIDTH,
            BACKGROUND_HEIGHT,
            DEFAULT_BACKGROUND,
        ),
        BACKGROUND_WIDTH,
    )
}

#[repr(align(32))]
struct AlignedChunk([u8; IO_CHUNK_SIZE]);

/// Read the installed theme archive from this channel's NAND data directory.
pub fn load() -> Result<Option<Vec<u8>>, SettingsIoError> {
    let path = title_data_file_path(THEME_FILENAME)?;
    let rc = isfs::initialize();
    if rc < 0 {
        return Err(SettingsIoError::Isfs(rc));
    }
    super::nand::recover_wii(&path)?;
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

    super::nand::save_wii(&path, archive)
}

#[repr(align(32))]
struct AlignedStats(isfs::Fstats);

fn theme_image_texture(
    package: Option<(&crate::theme::ThemePackage, &[u8])>,
    filename: &str,
    width: u32,
    height: u32,
    fallback: &[u8],
) -> Option<crate::png::GxRgba8Texture> {
    package
        .and_then(|(package, archive)| {
            package
                .extract_file(archive, filename, THEME_IMAGE_MAX_BYTES)
                .ok()
                .and_then(|bytes| decode_fixed_size_image(&bytes, width, height))
        })
        .or_else(|| decode_fixed_size_image(fallback, width, height))
}

fn package_image(
    package: Option<(&crate::theme::ThemePackage, &[u8])>,
    filename: &str,
    width: u32,
    height: u32,
) -> Option<crate::png::GxRgba8Texture> {
    package.and_then(|(package, archive)| {
        package
            .extract_file(archive, filename, THEME_IMAGE_MAX_BYTES)
            .ok()
            .and_then(|bytes| decode_fixed_size_image(&bytes, width, height))
    })
}

fn decode_fixed_size_image(
    data: &[u8],
    width: u32,
    height: u32,
) -> Option<crate::png::GxRgba8Texture> {
    let image = crate::png::decode_png(
        data,
        crate::png::PngLimits {
            max_width: width,
            max_height: height,
            max_pixels: (width * height) as usize,
            max_input_bytes: crate::theme::MAX_THEME_ARCHIVE_BYTES,
        },
    )
    .ok()?;
    if image.width != width || image.height != height {
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
