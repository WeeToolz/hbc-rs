//! Wii GX submission for hardware-independent UI frames.

use crate::{
    platform::{
        startup::IconTexture,
        theme::ThemeTextures,
        video::{VideoError, VideoSystem},
    },
    render::{map_rect, ButtonSize, Frame, FrameCommand, ThemeArt, LOGICAL_HEIGHT, LOGICAL_WIDTH},
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
    theme: &ThemeTextures,
) -> Result<(), VideoError> {
    let info = video.info();
    let mut textured_state = false;
    if let Some(background) = theme.background.as_ref() {
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
    if let Some(logo) = theme.logo.as_ref() {
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
            FrameCommand::ThemeArt {
                art,
                focused,
                bounds,
                fallback,
            } => {
                let texture = match art {
                    ThemeArt::Dialog => theme.dialog.as_ref(),
                    ThemeArt::About => theme.about.as_ref(),
                    ThemeArt::Button(ButtonSize::Large) if focused => {
                        theme.large_button_focused.as_ref()
                    }
                    ThemeArt::Button(ButtonSize::Large) => theme.large_button.as_ref(),
                    ThemeArt::Button(ButtonSize::Small) if focused => {
                        theme.small_button_focused.as_ref()
                    }
                    ThemeArt::Button(ButtonSize::Small) => theme.small_button.as_ref(),
                    ThemeArt::Button(ButtonSize::Tiny) if focused => {
                        theme.tiny_button_focused.as_ref()
                    }
                    ThemeArt::Button(ButtonSize::Tiny) => theme.tiny_button.as_ref(),
                };
                let Some(bounds) = map_rect(
                    bounds,
                    LOGICAL_WIDTH,
                    LOGICAL_HEIGHT,
                    info.width,
                    info.height,
                ) else {
                    continue;
                };
                unsafe {
                    draw_texture_or_fill(video, &mut textured_state, texture, bounds, fallback)?
                };
            }
            FrameCommand::PageArrow {
                previous,
                focused,
                bounds,
                fallback,
            } => {
                let texture = match (previous, focused) {
                    (true, false) => theme.previous_arrow.as_ref(),
                    (true, true) => theme.previous_arrow_focused.as_ref(),
                    (false, false) => theme.next_arrow.as_ref(),
                    (false, true) => theme.next_arrow_focused.as_ref(),
                };
                let Some(bounds) = map_rect(
                    bounds,
                    LOGICAL_WIDTH,
                    LOGICAL_HEIGHT,
                    info.width,
                    info.height,
                ) else {
                    continue;
                };
                unsafe {
                    draw_texture_or_fill(video, &mut textured_state, texture, bounds, fallback)?
                };
            }
            FrameCommand::AppCard {
                grid,
                focused,
                bounds,
                fallback,
            } => {
                let texture = match (grid, focused) {
                    (false, false) => theme.list_card.as_ref(),
                    (false, true) => theme.list_card_focused.as_ref(),
                    (true, false) => theme.grid_card.as_ref(),
                    (true, true) => theme.grid_card_focused.as_ref(),
                };
                let Some(bounds) = map_rect(
                    bounds,
                    LOGICAL_WIDTH,
                    LOGICAL_HEIGHT,
                    info.width,
                    info.height,
                ) else {
                    continue;
                };
                unsafe {
                    draw_texture_or_fill(video, &mut textured_state, texture, bounds, fallback)?
                };
            }
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
                unsafe { draw_solid_fill(video, bounds, *fill)? };
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
                unsafe {
                    draw_texture_or_fill(
                        video,
                        &mut textured_state,
                        icon.map(|icon| &icon.texture),
                        bounds,
                        fallback,
                    )?
                };
            }
        }
    }
    Ok(())
}

unsafe fn draw_solid_fill(
    video: &VideoSystem,
    bounds: crate::render::Rect,
    fill: crate::render::FillRect,
) -> Result<(), VideoError> {
    let color = ogc_gx::GxColor {
        r: fill.color.red,
        g: fill.color.green,
        b: fill.color.blue,
        a: fill.color.alpha,
    };
    unsafe { video.draw_solid_rect(bounds.x, bounds.y, bounds.width, bounds.height, color) }
}

unsafe fn draw_texture_or_fill(
    video: &VideoSystem,
    textured_state: &mut bool,
    texture: Option<&crate::png::GxRgba8Texture>,
    bounds: crate::render::Rect,
    fallback: crate::render::FillRect,
) -> Result<(), VideoError> {
    if let Some(texture) = texture {
        if !*textured_state {
            video.configure_textured_2d();
            *textured_state = true;
        }
        unsafe { video.draw_texture(texture, bounds.x, bounds.y, bounds.width, bounds.height) }
    } else {
        if *textured_state {
            video.configure_2d();
            *textured_state = false;
        }
        unsafe { draw_solid_fill(video, bounds, fallback) }
    }
}
