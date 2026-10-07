//! Wii GX submission for hardware-independent UI frames.

use alloc::vec::Vec;

use crate::{
    platform::{
        startup::IconTexture,
        theme::ThemeTextures,
        video::{VideoError, VideoSystem},
    },
    render::{
        map_rect, ButtonSize, DialogIcon, Frame, FrameCommand, StatusIndicator, ThemeArt,
        LOGICAL_HEIGHT,
    },
};

/// Submit the theme background and logo, solid fills, and app icons through GX.
///
/// Text uses cached TrueType glyph textures. App icons use the RGBA8 texture path.
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
    if let Some(background_fill) = frame.fills.first() {
        if let Some(bounds) = map_rect(
            background_fill.bounds,
            frame.logical_width(),
            LOGICAL_HEIGHT,
            info.width,
            info.height,
        ) {
            video.configure_2d();
            unsafe { draw_solid_fill(video, bounds, *background_fill)? };
        }
    }
    if let Some(background) = theme.background.as_ref() {
        if let Some(bounds) = map_rect(
            crate::render::Rect {
                x: f32::from(
                    frame
                        .logical_width()
                        .saturating_sub(theme.background_width as u16),
                ) / 2.0,
                y: 0.0,
                width: theme.background_width as f32,
                height: f32::from(LOGICAL_HEIGHT),
            },
            frame.logical_width(),
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
    for command in &frame.commands {
        if let FrameCommand::Bubble { texture, bounds } = *command {
            let Some(bounds) = map_rect(
                bounds,
                frame.logical_width(),
                LOGICAL_HEIGHT,
                info.width,
                info.height,
            ) else {
                continue;
            };
            if let Some(texture) = theme.bubbles[texture as usize].as_ref() {
                if !textured_state {
                    video.configure_textured_2d();
                    textured_state = true;
                }
                unsafe {
                    video.draw_texture(texture, bounds.x, bounds.y, bounds.width, bounds.height)?;
                }
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
            frame.logical_width(),
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
                    ThemeArt::Cursor { dragging, shade } => {
                        theme.cursor[usize::from(dragging) * 2 + usize::from(shade)].as_ref()
                    }
                    ThemeArt::Dialog => theme.dialog.as_ref(),
                    ThemeArt::Progress => theme.progress.as_ref(),
                    ThemeArt::Throbber => theme.throbber.as_ref(),
                    ThemeArt::MemoArrowUp => theme.memo_arrow_up.as_ref(),
                    ThemeArt::MemoArrowDown => theme.memo_arrow_down.as_ref(),
                    ThemeArt::DialogIcon(DialogIcon::Info) => theme.dialog_info.as_ref(),
                    ThemeArt::DialogIcon(DialogIcon::Confirm) => theme.dialog_confirm.as_ref(),
                    ThemeArt::DialogIcon(DialogIcon::Warning) => theme.dialog_warning.as_ref(),
                    ThemeArt::DialogIcon(DialogIcon::Error) => theme.dialog_error.as_ref(),
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
                    frame.logical_width(),
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
            FrameCommand::Bubble { .. } => {}
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
                    frame.logical_width(),
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
                    frame.logical_width(),
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
                if index == 0 {
                    continue;
                }
                let Some(fill) = frame.fills.get(index) else {
                    continue;
                };
                let Some(bounds) = map_rect(
                    fill.bounds,
                    frame.logical_width(),
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
            } => {
                let icon = icons
                    .iter()
                    .find(|icon| icon.catalog_index == catalog_index);
                let Some(icon) = icon else {
                    continue;
                };
                let Some(bounds) = map_rect(
                    bounds,
                    frame.logical_width(),
                    LOGICAL_HEIGHT,
                    info.width,
                    info.height,
                ) else {
                    continue;
                };
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
            }
            FrameCommand::StatusIndicator { indicator, bounds } => {
                let texture = match indicator {
                    StatusIndicator::Network { active: true } => theme.network_icon_active.as_ref(),
                    StatusIndicator::Network { active: false } => theme.network_icon.as_ref(),
                    StatusIndicator::Gecko { active: true } => theme.gecko_icon_active.as_ref(),
                    StatusIndicator::Gecko { active: false } => theme.gecko_icon.as_ref(),
                };
                let Some(bounds) = map_rect(
                    bounds,
                    frame.logical_width(),
                    LOGICAL_HEIGHT,
                    info.width,
                    info.height,
                ) else {
                    continue;
                };
                if let Some(texture) = texture {
                    unsafe {
                        video.draw_texture(
                            texture,
                            bounds.x,
                            bounds.y,
                            bounds.width,
                            bounds.height,
                        )?;
                    }
                }
            }
        }
    }
    for run in &frame.text_runs {
        let Some(file) = run.file.as_deref() else {
            continue;
        };
        let requested_face = theme
            .fonts
            .iter()
            .find(|face| face.borrow().file.eq_ignore_ascii_case(file))
            .or_else(|| {
                run.fallback_file.as_deref().and_then(|fallback| {
                    theme
                        .fonts
                        .iter()
                        .find(|face| face.borrow().file.eq_ignore_ascii_case(fallback))
                })
            });
        let Some(requested_face) = requested_face else {
            unsafe { draw_fallback_text(video, frame, run, &info)? };
            continue;
        };
        let file = requested_face.borrow().file.clone();
        let mut face = requested_face.borrow_mut();
        let Some((ascent, _, _)) = face.horizontal_line_metrics(run.size) else {
            unsafe { draw_fallback_text(video, frame, run, &info)? };
            continue;
        };
        let mut cursor_x = run.x;
        let mut previous_character = None;
        for character in run.value.chars() {
            if let Some(previous) = previous_character {
                cursor_x += face
                    .horizontal_kern(previous, character, run.size)
                    .unwrap_or(0.0)
                    * run.scale;
            }
            let glyph = face
                .rasterize(character, run.size)
                .map_err(VideoError::Font)?;
            if glyph.width != 0 && glyph.height != 0 {
                let mut textures = theme.font_textures.borrow_mut();
                let cached = textures.iter().position(|item| {
                    item.file.eq_ignore_ascii_case(&file)
                        && item.character == character
                        && item.size == run.size
                        && item.color == run.color
                });
                let cached = if let Some(index) = cached {
                    index
                } else {
                    let mut pixels = Vec::new();
                    pixels
                        .try_reserve_exact(glyph.coverage.len() * 4)
                        .map_err(|_| VideoError::RenderAllocation)?;
                    for coverage in &glyph.coverage {
                        pixels.extend_from_slice(&[
                            run.color.red,
                            run.color.green,
                            run.color.blue,
                            ((u16::from(*coverage) * u16::from(run.color.alpha)) / 255) as u8,
                        ]);
                    }
                    let image = crate::png::RgbaImage {
                        width: u32::from(glyph.width),
                        height: u32::from(glyph.height),
                        pixels,
                    };
                    let texture = crate::png::GxRgba8Texture::from_image(&image)
                        .map_err(VideoError::Texture)?;
                    textures
                        .try_reserve(1)
                        .map_err(|_| VideoError::RenderAllocation)?;
                    textures.push(super::theme::CachedFontTexture {
                        file: file.clone(),
                        character,
                        size: run.size,
                        color: run.color,
                        texture,
                    });
                    textures.len() - 1
                };
                let texture = &textures[cached].texture;
                let baseline = run.y + ascent * run.scale;
                let x = cursor_x + glyph.xmin as f32 * run.scale;
                let y = baseline - (glyph.ymin + i32::from(glyph.height)) as f32 * run.scale;
                if let Some(bounds) = map_rect(
                    crate::render::Rect {
                        x,
                        y,
                        width: f32::from(glyph.width) * run.scale,
                        height: f32::from(glyph.height) * run.scale,
                    },
                    frame.logical_width(),
                    LOGICAL_HEIGHT,
                    info.width,
                    info.height,
                ) {
                    video.configure_textured_2d();
                    unsafe {
                        video.draw_texture(
                            texture,
                            bounds.x,
                            bounds.y,
                            bounds.width,
                            bounds.height,
                        )?;
                    }
                }
            }
            cursor_x += glyph.advance_width * run.scale;
            previous_character = Some(character);
        }
    }
    Ok(())
}

unsafe fn draw_fallback_text(
    video: &VideoSystem,
    frame: &Frame,
    run: &crate::render::TextRun,
    info: &crate::platform::video::FramebufferInfo,
) -> Result<(), VideoError> {
    video.configure_2d();
    let mut x = run.x;
    for character in run.value.chars() {
        for (row, bits) in crate::render::glyph_rows(character).into_iter().enumerate() {
            let mut column = 0;
            while column < 5 {
                if bits & (1 << (4 - column)) == 0 {
                    column += 1;
                    continue;
                }
                let start = column;
                while column < 5 && bits & (1 << (4 - column)) != 0 {
                    column += 1;
                }
                let Some(bounds) = map_rect(
                    crate::render::Rect {
                        x: x + start as f32 * run.scale,
                        y: run.y + row as f32 * run.scale,
                        width: (column - start) as f32 * run.scale,
                        height: run.scale,
                    },
                    frame.logical_width(),
                    LOGICAL_HEIGHT,
                    info.width,
                    info.height,
                ) else {
                    continue;
                };
                let fill = crate::render::FillRect {
                    bounds,
                    color: run.color,
                };
                unsafe {
                    draw_solid_fill(video, bounds, fill)?;
                }
            }
        }
        x += 6.0 * run.scale;
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
