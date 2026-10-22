//! Wii-specific helpers for the shared HOME overlay engine.
//!
//! Geometry, selection, input, and operation policy live in the hardware-
//! independent modules. This module contains only real Wii video integration.

#[cfg(target_arch = "powerpc")]
use super::video::{VideoError, VideoSystem};
#[cfg(target_arch = "powerpc")]
use crate::home_overlay_app::{AppHomeOverlay, DrawError};

#[cfg(target_arch = "powerpc")]
#[derive(Debug)]
pub enum GxOverlayError {
    Layout(DrawError),
    Video(VideoError),
}

#[cfg(target_arch = "powerpc")]
#[derive(Debug)]
pub enum ScreenshotActionError<E> {
    NoStorage,
    Capture(VideoError),
    Encode(crate::png::PngError),
    Save(E),
}

/// One production screenshot pipeline shared by HBC and application hosts.
/// Storage is checked before the expensive EFB readback and rechecked by the
/// caller's save backend when the bytes are committed.
#[cfg(target_arch = "powerpc")]
pub fn capture_and_save_screenshot<E>(
    video: &VideoSystem,
    storage_ready: bool,
    save: impl FnOnce(&[u8]) -> Result<alloc::string::String, E>,
) -> Result<alloc::string::String, ScreenshotActionError<E>> {
    if !storage_ready {
        return Err(ScreenshotActionError::NoStorage);
    }
    let image = video
        .capture_efb_rgba()
        .map_err(ScreenshotActionError::Capture)?;
    let bytes = crate::png::encode_rgba_png(image.width, image.height, &image.pixels)
        .map_err(ScreenshotActionError::Encode)?;
    save(&bytes).map_err(ScreenshotActionError::Save)
}

/// Draw the overlay chrome with HBC's existing GX 2D renderer. Text remains
/// owned by the host application's font backend through `OverlayText`.
#[cfg(target_arch = "powerpc")]
pub fn draw_chrome(video: &VideoSystem, overlay: &AppHomeOverlay) -> Result<(), GxOverlayError> {
    let info = video.info();
    let mut first_error = None;
    overlay
        .visit_rects(usize::from(info.width), usize::from(info.height), |rect| {
            if first_error.is_some() {
                return;
            }
            let [r, g, b, a] = rect.color;
            let result = unsafe {
                video.draw_solid_rect(
                    rect.x as f32,
                    rect.y as f32,
                    rect.width as f32,
                    rect.height as f32,
                    ogc_gx::GxColor { r, g, b, a },
                )
            };
            if let Err(error) = result {
                first_error = Some(error);
            }
        })
        .map_err(GxOverlayError::Layout)?;
    if let Some(error) = first_error {
        return Err(GxOverlayError::Video(error));
    }
    Ok(())
}
