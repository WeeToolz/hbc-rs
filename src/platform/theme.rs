//! NAND storage for the HBC installed theme ZIP.

use alloc::{string::String, vec::Vec};
use core::cell::RefCell;

use crate::{
    platform::settings::{title_data_file_path, SettingsIoError},
    theme::MAX_THEME_ARCHIVE_BYTES,
};

use ogc_filesystem as isfs;

const THEME_FILENAME: &str = "theme.zip";
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
const THROBBER_SIZE: u32 = 64;
const BUBBLE_SIZE: u32 = 64;
const PROGRESS_WIDTH: u32 = 400;
const PROGRESS_HEIGHT: u32 = 112;
const MEMO_ARROW_WIDTH: u32 = 32;
const MEMO_ARROW_HEIGHT: u32 = 8;
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
const DEFAULT_THROBBER: &[u8] = include_bytes!("../../assets/throbber.png");
const DEFAULT_BUBBLES: [&[u8]; 3] = [
    include_bytes!("../../assets/bubble1.png"),
    include_bytes!("../../assets/bubble2.png"),
    include_bytes!("../../assets/bubble3.png"),
];
const DEFAULT_PROGRESS: &[u8] = include_bytes!("../../assets/progress.png");
const DEFAULT_MEMO_ARROW_UP: &[u8] = include_bytes!("../../assets/content_arrow_up.png");
const DEFAULT_MEMO_ARROW_DOWN: &[u8] = include_bytes!("../../assets/content_arrow_down.png");
const DEFAULT_LARGE_BUTTON: &[u8] = include_bytes!("../../assets/button.png");
const DEFAULT_LARGE_BUTTON_FOCUS: &[u8] = include_bytes!("../../assets/button_focus.png");
const DEFAULT_SMALL_BUTTON: &[u8] = include_bytes!("../../assets/button_small.png");
const DEFAULT_SMALL_BUTTON_FOCUS: &[u8] = include_bytes!("../../assets/button_small_focus.png");
const DEFAULT_TINY_BUTTON: &[u8] = include_bytes!("../../assets/button_tiny.png");
const DEFAULT_TINY_BUTTON_FOCUS: &[u8] = include_bytes!("../../assets/button_tiny_focus.png");
const DEFAULT_NETWORK_ICON: &[u8] = include_bytes!("../../assets/icon_network.png");
const DEFAULT_NETWORK_ICON_ACTIVE: &[u8] = include_bytes!("../../assets/icon_network_active.png");
const DEFAULT_GECKO_ICON: &[u8] = include_bytes!("../../assets/icon_usbgecko.png");
const DEFAULT_GECKO_ICON_ACTIVE: &[u8] = include_bytes!("../../assets/icon_usbgecko_active.png");
const DEFAULT_CURSOR: &[u8] = include_bytes!("../../assets/cursor_pic.png");
const DEFAULT_CURSOR_SHADE: &[u8] = include_bytes!("../../assets/cursor_shade.png");
const DEFAULT_CURSOR_DRAG: &[u8] = include_bytes!("../../assets/cursor_drag.png");
const DEFAULT_CURSOR_DRAG_SHADE: &[u8] = include_bytes!("../../assets/cursor_drag_shade.png");
const DEFAULT_REGULAR_FONT: &[u8] = include_bytes!("../../assets/droid.ttf");
const DEFAULT_BOLD_FONT: &[u8] = include_bytes!("../../assets/droidbold.ttf");

pub struct ThemeTextures {
    pub fonts: Vec<RefCell<crate::font::ThemeFontFace>>,
    pub font_textures: RefCell<Vec<CachedFontTexture>>,
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
    pub throbber: Option<crate::png::GxRgba8Texture>,
    pub bubbles: [Option<crate::png::GxRgba8Texture>; 3],
    pub progress: Option<crate::png::GxRgba8Texture>,
    pub memo_arrow_up: Option<crate::png::GxRgba8Texture>,
    pub memo_arrow_down: Option<crate::png::GxRgba8Texture>,
    pub network_icon: Option<crate::png::GxRgba8Texture>,
    pub network_icon_active: Option<crate::png::GxRgba8Texture>,
    pub gecko_icon: Option<crate::png::GxRgba8Texture>,
    pub gecko_icon_active: Option<crate::png::GxRgba8Texture>,
    pub cursor: [Option<crate::png::GxRgba8Texture>; 4],
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
            fonts: load_theme_fonts(package),
            font_textures: RefCell::new(Vec::new()),
            background,
            background_width,
            // HBC reads its logo from the bundled data and does not load it from themes.
            logo: decode_fixed_size_image(DEFAULT_LOGO, LOGO_WIDTH, LOGO_HEIGHT),
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
            throbber: theme_image_texture(
                package,
                "throbber.png",
                THROBBER_SIZE,
                THROBBER_SIZE,
                DEFAULT_THROBBER,
            ),
            bubbles: [
                theme_image_texture(
                    package,
                    "bubble1.png",
                    BUBBLE_SIZE,
                    BUBBLE_SIZE,
                    DEFAULT_BUBBLES[0],
                ),
                theme_image_texture(
                    package,
                    "bubble2.png",
                    BUBBLE_SIZE,
                    BUBBLE_SIZE,
                    DEFAULT_BUBBLES[1],
                ),
                theme_image_texture(
                    package,
                    "bubble3.png",
                    BUBBLE_SIZE,
                    BUBBLE_SIZE,
                    DEFAULT_BUBBLES[2],
                ),
            ],
            progress: theme_image_texture(
                package,
                "progress.png",
                PROGRESS_WIDTH,
                PROGRESS_HEIGHT,
                DEFAULT_PROGRESS,
            ),
            memo_arrow_up: theme_image_texture(
                package,
                "content_arrow_up.png",
                MEMO_ARROW_WIDTH,
                MEMO_ARROW_HEIGHT,
                DEFAULT_MEMO_ARROW_UP,
            ),
            memo_arrow_down: theme_image_texture(
                package,
                "content_arrow_down.png",
                MEMO_ARROW_WIDTH,
                MEMO_ARROW_HEIGHT,
                DEFAULT_MEMO_ARROW_DOWN,
            ),
            network_icon: theme_image_texture(
                package,
                "icon_network.png",
                32,
                32,
                DEFAULT_NETWORK_ICON,
            ),
            network_icon_active: theme_image_texture(
                package,
                "icon_network_active.png",
                32,
                32,
                DEFAULT_NETWORK_ICON_ACTIVE,
            ),
            gecko_icon: theme_image_texture(
                package,
                "icon_usbgecko.png",
                32,
                32,
                DEFAULT_GECKO_ICON,
            ),
            gecko_icon_active: theme_image_texture(
                package,
                "icon_usbgecko_active.png",
                32,
                32,
                DEFAULT_GECKO_ICON_ACTIVE,
            ),
            cursor: [
                decode_fixed_size_image(DEFAULT_CURSOR, 96, 96),
                decode_fixed_size_image(DEFAULT_CURSOR_SHADE, 96, 96),
                decode_fixed_size_image(DEFAULT_CURSOR_DRAG, 96, 96),
                decode_fixed_size_image(DEFAULT_CURSOR_DRAG_SHADE, 96, 96),
            ],
        }
    }
}

pub struct CachedFontTexture {
    pub file: String,
    pub character: char,
    pub size: u16,
    pub color: crate::render::Color,
    pub texture: crate::png::GxRgba8Texture,
}

fn load_theme_fonts(
    package: Option<(&crate::theme::ThemePackage, &[u8])>,
) -> Vec<RefCell<crate::font::ThemeFontFace>> {
    let mut faces = Vec::new();
    for (name, bytes) in [
        (crate::font::DEFAULT_REGULAR_FONT_KEY, DEFAULT_REGULAR_FONT),
        (crate::font::DEFAULT_BOLD_FONT_KEY, DEFAULT_BOLD_FONT),
    ] {
        if let Ok(face) = crate::font::ThemeFontFace::parse(name.into(), bytes.to_vec()) {
            faces.push(RefCell::new(face));
        }
    }
    let Some((package, archive)) = package else {
        return faces;
    };
    let mut paths = Vec::new();
    if let Some(file) = package.metadata.default_font.file.as_ref() {
        paths.push(file.clone());
    }
    for (_, font) in &package.metadata.fonts {
        if let Some(file) = font.file.as_ref() {
            if !paths
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(file))
            {
                paths.push(file.clone());
            }
        }
    }
    for path in paths {
        if faces
            .iter()
            .any(|face: &RefCell<crate::font::ThemeFontFace>| {
                face.borrow().file.eq_ignore_ascii_case(&path)
            })
        {
            continue;
        }
        let Ok(data) = package.extract_file(archive, &path, THEME_FONT_MAX_BYTES) else {
            continue;
        };
        if let Ok(face) = crate::font::ThemeFontFace::parse(path, data) {
            faces.push(RefCell::new(face));
        }
    }
    faces
}

const THEME_FONT_MAX_BYTES: usize = 8 * 1024 * 1024;

fn load_background(
    package: Option<(&crate::theme::ThemePackage, &[u8])>,
    widescreen: bool,
) -> (Option<crate::png::GxRgba8Texture>, u32) {
    let (filename, width, fallback) = if widescreen {
        (
            crate::theme::background_asset(true),
            WIDE_BACKGROUND_WIDTH,
            DEFAULT_WIDE_BACKGROUND,
        )
    } else {
        (
            crate::theme::background_asset(false),
            BACKGROUND_WIDTH,
            DEFAULT_BACKGROUND,
        )
    };
    let texture = package_image(package, filename, width, BACKGROUND_HEIGHT)
        .or_else(|| decode_fixed_size_image(fallback, width, BACKGROUND_HEIGHT));
    (texture, width)
}

/// Read the installed theme archive from this channel's NAND data directory.
pub fn load() -> Result<Option<Vec<u8>>, SettingsIoError> {
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("THEME PATH LOOKUP");
    let path = match title_data_file_path(THEME_FILENAME) {
        Ok(path) => path,
        Err(error) => {
            #[cfg(feature = "startup-diagnostics")]
            super::diagnostics::mark("THEME TITLE PATH ERROR");
            return Err(error);
        }
    };
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("THEME ISFS INIT");
    let rc = isfs::initialize();
    if rc < 0 {
        return Err(SettingsIoError::Isfs(rc));
    }
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("THEME FILE RECOVERY");
    super::nand::recover_wii(&path, MAX_THEME_ARCHIVE_BYTES, |bytes| {
        crate::theme::ThemePackage::parse(bytes).is_ok()
    })?;
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("THEME FILE OPEN");
    let file = match isfs::open(&path, isfs::ISFS_OPEN_READ) {
        Ok(file) => file,
        Err(error) if super::nand::is_missing_isfs_error(error, ogc_filesystem::ISFS_ENOENT) => {
            return Ok(None)
        }
        Err(error) => return Err(SettingsIoError::Isfs(error)),
    };
    let read_result = (|| {
        #[cfg(feature = "startup-diagnostics")]
        super::diagnostics::mark("THEME FILE STATS");
        let mut stats = AlignedStats(isfs::Fstats::default());
        let rc = isfs::get_file_stats(&file, &mut stats.0);
        if rc < 0 {
            return Err(SettingsIoError::Isfs(rc));
        }
        let size = stats.0.file_length as usize;
        if size == 0 || size > MAX_THEME_ARCHIVE_BYTES {
            return Err(SettingsIoError::InvalidFileSize);
        }

        #[cfg(feature = "startup-diagnostics")]
        super::diagnostics::mark("THEME FILE READ");
        let mut archive = Vec::new();
        if archive.try_reserve_exact(size).is_err() {
            return Err(SettingsIoError::InvalidFileSize);
        }
        archive.resize(size, 0);
        super::nand::read_exact_wii(&file, &mut archive).map_err(SettingsIoError::Isfs)?;
        Ok(archive)
    })();
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("THEME FILE CLOSE");
    let close_rc = file.close();
    if close_rc < 0 {
        return match read_result {
            Err(SettingsIoError::Isfs(operation)) => Err(SettingsIoError::IsfsCleanup {
                operation: Some(operation),
                cleanup: close_rc,
            }),
            Err(_) => Err(SettingsIoError::Isfs(close_rc)),
            Ok(_) => Err(SettingsIoError::Isfs(close_rc)),
        };
    }
    let archive = read_result?;
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("THEME ZIP READY");
    Ok(Some(archive))
}

/// Replace the installed theme archive after validating it in the caller.
pub fn save(archive: &[u8]) -> Result<(), SettingsIoError> {
    if archive.is_empty() || archive.len() > MAX_THEME_ARCHIVE_BYTES {
        return Err(SettingsIoError::InvalidFileSize);
    }
    let path = super::settings::prepare_title_data_file_path(THEME_FILENAME)?;
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

#[cfg(test)]
mod tests {
    use super::load_theme_fonts;

    #[test]
    fn default_hbc_regular_and_bold_fonts_are_available_without_a_theme() {
        let fonts = load_theme_fonts(None);
        let names: alloc::vec::Vec<_> = fonts
            .iter()
            .map(|face| face.borrow().file.clone())
            .collect();
        assert!(names
            .iter()
            .any(|name| name == crate::font::DEFAULT_REGULAR_FONT_KEY));
        assert!(names
            .iter()
            .any(|name| name == crate::font::DEFAULT_BOLD_FONT_KEY));
    }
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
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("PNG DECODE START");
    let image = match crate::png::decode_png(
        data,
        crate::png::PngLimits {
            max_width: width,
            max_height: height,
            max_pixels: (width * height) as usize,
            max_input_bytes: crate::theme::MAX_THEME_ARCHIVE_BYTES,
        },
    ) {
        Ok(image) => image,
        Err(_) => {
            #[cfg(feature = "startup-diagnostics")]
            super::diagnostics::mark("PNG DECODE FAILED");
            return None;
        }
    };
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("PNG DECODE DONE");
    if image.width != width || image.height != height {
        #[cfg(feature = "startup-diagnostics")]
        super::diagnostics::mark("PNG SIZE MISMATCH");
        return None;
    }
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("PNG GX TILE START");
    let texture = match crate::png::GxRgba8Texture::from_image(&image) {
        Ok(texture) => texture,
        Err(_) => {
            #[cfg(feature = "startup-diagnostics")]
            super::diagnostics::mark("PNG GX TILE FAILED");
            return None;
        }
    };
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("PNG GX TILE DONE");
    Some(texture)
}
