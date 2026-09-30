//! Wii GX submission for hardware-independent UI frames.

use crate::{
    platform::{
        startup::IconTexture,
        video::{VideoError, VideoSystem},
    },
    render::{map_rect, Frame, FrameCommand, LOGICAL_HEIGHT, LOGICAL_WIDTH},
};

/// Submit the frame's solid fills and app icons through GX.
///
/// Text remains native bitmap-font rectangles. App icons use the RGBA8 texture path.
///
/// # Safety
/// The caller must keep GX initialized and keep all supplied textures alive until GX has
/// completed its reads from this frame.
pub unsafe fn draw_frame(
    video: &VideoSystem,
    frame: &Frame,
    icons: &[IconTexture],
) -> Result<(), VideoError> {
    let info = video.info();
    let mut textured_state = false;
    for command in &frame.commands {
        match *command {
            FrameCommand::Fill(index) => {
                if textured_state {
                    video.configure_2d();
                    textured_state = false;
                }
                let Some(fill) = frame.fills.get(index) else {
                    continue;
                };
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
                unsafe {
                    video.draw_solid_rect(bounds.x, bounds.y, bounds.width, bounds.height, color)?
                };
            }
            FrameCommand::Icon {
                catalog_index,
                bounds,
                fallback,
            } => {
                let icon = icons
                    .iter()
                    .find(|icon| icon.catalog_index == catalog_index);
                let Some(bounds) = map_rect(
                    bounds,
                    LOGICAL_WIDTH,
                    LOGICAL_HEIGHT,
                    info.width,
                    info.height,
                ) else {
                    continue;
                };
                if let Some(icon) = icon {
                    if !textured_state {
                        video.configure_textured_2d();
                        textured_state = true;
                    }
                    unsafe {
                        video.draw_texture(
                            &icon.texture,
                            bounds.x,
                            bounds.y,
                            bounds.width,
                            bounds.height,
                        )?;
                    }
                } else {
                    if textured_state {
                        video.configure_2d();
                        textured_state = false;
                    }
                    let color = ogc_gx::GxColor {
                        r: fallback.color.red,
                        g: fallback.color.green,
                        b: fallback.color.blue,
                        a: fallback.color.alpha,
                    };
                    unsafe {
                        video.draw_solid_rect(
                            bounds.x,
                            bounds.y,
                            bounds.width,
                            bounds.height,
                            color,
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}
