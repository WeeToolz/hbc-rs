//! Wii GX submission for hardware-independent UI frames.

use crate::{
    platform::{
        startup::IconTexture,
        video::{VideoError, VideoSystem},
    },
    render::{map_rect, Frame, FrameCommand, LOGICAL_HEIGHT, LOGICAL_WIDTH},
};

/// Submit the theme background and logo, solid fills, and app icons through GX.
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
    background: Option<&crate::png::GxRgba8Texture>,
    logo: Option<&crate::png::GxRgba8Texture>,
) -> Result<(), VideoError> {
    let info = video.info();
    let mut textured_state = false;
    if let Some(background) = background {
        if let Some(bounds) = map_rect(
            crate::render::Rect {
                x: 0.0,
                y: 0.0,
                width: f32::from(LOGICAL_WIDTH),
                height: f32::from(LOGICAL_HEIGHT),
            },
            LOGICAL_WIDTH,
            LOGICAL_HEIGHT,
            info.width,
            info.height,
        ) {
            video.configure_textured_2d();
            textured_state = true;
            unsafe {
                video.draw_texture(background, bounds.x, bounds.y, bounds.width, bounds.height)?;
            }
        }
    }
    if let Some(logo) = logo {
        if !textured_state {
            video.configure_textured_2d();
            textured_state = true;
        }
        if let Some(bounds) = map_rect(
            crate::render::Rect {
                x: 0.0,
                y: 416.0,
                width: 320.0,
                height: 64.0,
            },
            LOGICAL_WIDTH,
            LOGICAL_HEIGHT,
            info.width,
            info.height,
        ) {
            unsafe {
                video.draw_texture(logo, bounds.x, bounds.y, bounds.width, bounds.height)?;
            }
        }
    }
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
