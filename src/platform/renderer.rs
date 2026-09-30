//! Wii GX submission for hardware-independent UI frames.

use crate::{
    platform::video::{VideoError, VideoSystem},
    render::{map_rect, Frame, LOGICAL_HEIGHT, LOGICAL_WIDTH},
};

/// Submit the frame's solid fills through the configured 2D GX state.
///
/// Configure `video.configure_2d()` before calling this function. Draw text
/// and textures only after GX texture state is available; this adapter does
/// not pretend that a solid-color rectangle can display an icon or glyph.
///
/// # Safety
/// The caller must keep GX initialized, configure the solid 2D state, and call
/// this before presenting the same frame.
pub unsafe fn draw_frame(video: &VideoSystem, frame: &Frame) -> Result<(), VideoError> {
    let info = video.info();
    for fill in &frame.fills {
        let Some(bounds) = map_rect(
            fill.bounds,
            LOGICAL_WIDTH,
            LOGICAL_HEIGHT,
            info.width,
            info.height,
        ) else {
            continue;
        };
        let color = ogc_gx::GxColor {
            r: fill.color.red,
            g: fill.color.green,
            b: fill.color.blue,
            a: fill.color.alpha,
        };
        unsafe { video.draw_solid_rect(bounds.x, bounds.y, bounds.width, bounds.height, color)? };
    }
    Ok(())
}
