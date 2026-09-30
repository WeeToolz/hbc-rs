//! Hardware-independent layout for the first HBC browser renderer.
//!
//! The legacy renderer combined layout, widget state, texture drawing, and
//! GX state changes in one queue. This module keeps layout as data. The Wii
//! adapter can submit the resulting solid rectangles through `ogc-gx`.
//! Text uses a small native bitmap font. Icons, gradients, and theme textures
//! need GX texture support and are not represented yet.

use alloc::vec::Vec;

use crate::{
    app::{AppController, Screen},
    catalog::AppEntry,
};

pub const LOGICAL_WIDTH: u16 = 640;
pub const LOGICAL_HEIGHT: u16 = 480;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Color {
    pub const fn rgb(red: u8, green: u8, blue: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha: 255,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn right(self) -> f32 {
        self.x + self.width
    }
    pub fn bottom(self) -> f32 {
        self.y + self.height
    }

    /// Return the visible intersection. Empty or non-finite rectangles vanish.
    pub fn intersect(self, other: Self) -> Option<Self> {
        if ![
            self.x,
            self.y,
            self.width,
            self.height,
            other.x,
            other.y,
            other.width,
            other.height,
        ]
        .iter()
        .all(|v| v.is_finite())
            || self.width <= 0.0
            || self.height <= 0.0
            || other.width <= 0.0
            || other.height <= 0.0
        {
            return None;
        }
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        (right > x && bottom > y).then_some(Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FillRect {
    pub bounds: Rect,
    pub color: Color,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
    pub fills: Vec<FillRect>,
}

impl Frame {
    fn fill(&mut self, bounds: Rect, color: Color) {
        if bounds.width.is_finite()
            && bounds.height.is_finite()
            && bounds.x.is_finite()
            && bounds.y.is_finite()
            && bounds.width > 0.0
            && bounds.height > 0.0
        {
            self.fills.push(FillRect { bounds, color });
        }
    }

    fn text(&mut self, x: f32, y: f32, value: &str, scale: f32, color: Color, max_chars: usize) {
        if !scale.is_finite() || scale <= 0.0 {
            return;
        }
        let mut cursor_x = x;
        for character in value.chars().take(max_chars) {
            let rows = glyph_rows(character);
            for (row_index, row_bits) in rows.into_iter().enumerate() {
                let mut column = 0;
                while column < 5 {
                    if row_bits & (1 << (4 - column)) == 0 {
                        column += 1;
                        continue;
                    }
                    let start = column;
                    while column < 5 && row_bits & (1 << (4 - column)) != 0 {
                        column += 1;
                    }
                    self.fill(
                        rect(
                            cursor_x + start as f32 * scale,
                            y + row_index as f32 * scale,
                            (column - start) as f32 * scale,
                            scale,
                        ),
                        color,
                    );
                }
            }
            cursor_x += 6.0 * scale;
        }
    }
}

fn glyph_rows(character: char) -> [u8; 7] {
    match character.to_ascii_uppercase() {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [14, 4, 4, 4, 4, 4, 14],
        'J' => [7, 2, 2, 2, 18, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 25, 21, 19, 17, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        ' ' => [0; 7],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        ',' => [0, 0, 0, 0, 4, 4, 8],
        ':' => [0, 12, 12, 0, 12, 12, 0],
        ';' => [0, 12, 12, 0, 4, 4, 8],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 31],
        '/' => [1, 1, 2, 4, 8, 16, 16],
        '(' => [2, 4, 8, 8, 8, 4, 2],
        ')' => [8, 4, 2, 2, 2, 4, 8],
        '!' => [4, 4, 4, 4, 4, 0, 4],
        '?' => [14, 17, 1, 2, 4, 0, 4],
        '\'' => [4, 4, 8, 0, 0, 0, 0],
        '+' => [0, 4, 4, 31, 4, 4, 0],
        _ => [14, 17, 1, 2, 4, 0, 4],
    }
}

const BACKGROUND: Color = Color::rgb(15, 24, 35);
const HEADER: Color = Color::rgb(25, 39, 55);
const PANEL: Color = Color::rgb(29, 44, 61);
const ROW: Color = Color::rgb(37, 54, 72);
const ROW_ALT: Color = Color::rgb(32, 48, 65);
const ACCENT: Color = Color::rgb(63, 155, 211);
const MUTED: Color = Color::rgb(87, 111, 132);
const MODAL_SHADE: Color = Color {
    red: 0,
    green: 0,
    blue: 0,
    alpha: 176,
};

/// Build a 640x480 logical browser frame from the current navigation state.
/// The caller clips/scales it to the active framebuffer. Catalog selection
/// uses controller identity and page policy, so rendering does not duplicate
/// filtering, sorting, or page calculations.
pub fn browser_frame(controller: &AppController, catalog: &[AppEntry]) -> Frame {
    let mut frame = Frame::default();
    frame.fill(rect(0, 0, 640, 480), BACKGROUND);
    frame.fill(rect(0, 0, 640, 64), HEADER);
    frame.fill(rect(28, 23, 5, 28), ACCENT);

    let panel = rect(28, 82, 584, 338);
    frame.fill(panel, PANEL);
    let layout = controller.browser_layout();
    let page = controller.visible_page(catalog);
    let page_start = controller.page_start();
    let selected = controller.selected_index().saturating_sub(page_start);

    match controller.screen {
        Screen::Browser | Screen::AppDetails | Screen::DeleteConfirm => {
            let columns = layout.columns.max(1);
            let rows = layout.rows.max(1);
            let gap = 8.0;
            let cell_w =
                (panel.width - 32.0 - gap * columns.saturating_sub(1) as f32) / columns as f32;
            let cell_h = (panel.height - 32.0 - gap * rows.saturating_sub(1) as f32) / rows as f32;
            for (index, app) in page.iter().enumerate() {
                let col = index % columns;
                let row = index / columns;
                if row >= rows {
                    break;
                }
                let x = panel.x + 16.0 + col as f32 * (cell_w + gap);
                let y = panel.y + 16.0 + row as f32 * (cell_h + gap);
                let is_selected = index == selected
                    && controller.selected_app.as_deref() == Some(app.directory.as_str());
                let fill = if is_selected {
                    ACCENT
                } else if row % 2 == 0 {
                    ROW
                } else {
                    ROW_ALT
                };
                frame.fill(rect(x, y, cell_w, cell_h), fill);
                // The marker stands in for the app icon until texture drawing
                // is available.
                frame.fill(rect(x + 10.0, y + 10.0, 38.0, 38.0), MUTED);
                let name = app
                    .metadata
                    .as_ref()
                    .and_then(|meta| meta.name.as_deref())
                    .unwrap_or(app.directory.as_str());
                frame.text(
                    x + 58.0,
                    y + 12.0,
                    name,
                    1.0,
                    Color::rgb(213, 224, 233),
                    ((cell_w - 70.0).max(6.0) / 6.0) as usize,
                );
                if app
                    .metadata
                    .as_ref()
                    .and_then(|meta| meta.short_description.as_deref())
                    .is_some_and(|description| !description.is_empty())
                {
                    let description = app
                        .metadata
                        .as_ref()
                        .and_then(|meta| meta.short_description.as_deref())
                        .unwrap_or_default();
                    frame.text(
                        x + 58.0,
                        y + 25.0,
                        description,
                        1.0,
                        MUTED,
                        ((cell_w - 70.0).max(6.0) / 6.0) as usize,
                    );
                }
            }
        }
        Screen::MainMenu => {
            frame.fill(rect(162, 122, 316, 248), ROW);
            frame.text(
                190.0,
                130.0,
                "Homebrew Channel",
                1.0,
                Color::rgb(213, 224, 233),
                32,
            );
            for index in 0..5 {
                let y = 145.0 + index as f32 * 40.0;
                let color = if index == controller.menu_focus() {
                    ACCENT
                } else {
                    ROW_ALT
                };
                frame.fill(rect(184, y, 272, 30), color);
                let label = [
                    "Back",
                    "About",
                    "Launch BootMii",
                    "Exit to System Menu",
                    "Shutdown",
                ][index];
                frame.text(198.0, y + 11.0, label, 1.0, Color::rgb(213, 224, 233), 36);
            }
        }
        Screen::About => {
            frame.fill(rect(108, 92, 424, 324), ROW);
            frame.fill(rect(136, 120, 180, 40), ACCENT);
            frame.text(142.0, 136.0, "HBC-RS", 2.0, Color::rgb(213, 224, 233), 12);
            frame.text(
                144.0,
                190.0,
                "Homebrew Channel",
                1.0,
                Color::rgb(213, 224, 233),
                36,
            );
            frame.text(
                144.0,
                218.0,
                "Launch applications from removable storage.",
                1.0,
                MUTED,
                56,
            );
            frame.text(
                144.0,
                246.0,
                "Native Rust port under development.",
                1.0,
                MUTED,
                56,
            );
            frame.text(
                144.0,
                386.0,
                "Press B to return",
                1.0,
                Color::rgb(213, 224, 233),
                32,
            );
        }
    }

    // Footer and page progress are common to the browser view and use only
    // controller-derived page data.
    frame.fill(rect(28, 436, 584, 20), HEADER);
    let visible_count = controller.visible_apps(catalog).len();
    let capacity = layout.columns.max(1).saturating_mul(layout.rows.max(1));
    let pages = visible_count.saturating_add(capacity - 1) / capacity;
    if pages > 0 {
        let page_index = page_start / capacity;
        let segment_w = 520.0 / pages as f32;
        frame.fill(
            rect(
                60.0 + page_index as f32 * segment_w,
                443.0,
                segment_w.min(520.0),
                6.0,
            ),
            ACCENT,
        );
    }
    if matches!(
        controller.screen,
        Screen::AppDetails | Screen::DeleteConfirm
    ) {
        frame.fill(rect(0, 0, 640, 480), MODAL_SHADE);
        if controller.screen == Screen::AppDetails {
            frame.fill(rect(120, 96, 400, 288), PANEL);
            frame.fill(rect(152, 132, 336, 22), HEADER);
            if let Some(app) = controller
                .selected_app
                .as_deref()
                .and_then(|selected| catalog.iter().find(|entry| entry.directory == selected))
            {
                let name = app
                    .metadata
                    .as_ref()
                    .and_then(|meta| meta.name.as_deref())
                    .unwrap_or(app.directory.as_str());
                frame.text(160.0, 140.0, name, 1.0, Color::rgb(213, 224, 233), 52);
                if let Some(description) = app
                    .metadata
                    .as_ref()
                    .and_then(|meta| meta.long_description.as_deref())
                {
                    frame.text(
                        152.0,
                        176.0,
                        description,
                        1.0,
                        Color::rgb(213, 224, 233),
                        54,
                    );
                }
            }
            for (index, action) in [8usize, 9, 10].iter().enumerate() {
                frame.fill(
                    rect(152.0 + index as f32 * 108.0, 326, 96, 34),
                    if controller.detail_focus() == *action {
                        ACCENT
                    } else {
                        ROW
                    },
                );
                frame.text(
                    174.0 + index as f32 * 108.0,
                    340.0,
                    ["Delete", "Load", "Return"][index],
                    1.0,
                    Color::rgb(213, 224, 233),
                    14,
                );
            }
        } else {
            frame.fill(rect(144, 168, 352, 144), PANEL);
            frame.fill(rect(168, 188, 304, 20), HEADER);
            frame.text(
                184.0,
                194.0,
                "Delete this application?",
                1.0,
                Color::rgb(213, 224, 233),
                48,
            );
            for (index, yes) in [true, false].iter().enumerate() {
                frame.fill(
                    rect(212.0 + index as f32 * 112.0, 260, 96, 32),
                    if controller.delete_confirmation_yes() == *yes {
                        ACCENT
                    } else {
                        ROW
                    },
                );
                frame.text(
                    244.0 + index as f32 * 112.0,
                    272.0,
                    if *yes { "Yes" } else { "No" },
                    1.0,
                    Color::rgb(213, 224, 233),
                    3,
                );
            }
        }
    }
    frame
}

/// Clip one logical rectangle to the viewport and scale it to framebuffer pixels.
pub fn map_rect(
    bounds: Rect,
    logical_width: u16,
    logical_height: u16,
    target_width: u16,
    target_height: u16,
) -> Option<Rect> {
    if logical_width == 0 || logical_height == 0 || target_width == 0 || target_height == 0 {
        return None;
    }
    let visible = bounds.intersect(rect(
        0,
        0,
        f32::from(logical_width),
        f32::from(logical_height),
    ))?;
    let sx = f32::from(target_width) / f32::from(logical_width);
    let sy = f32::from(target_height) / f32::from(logical_height);
    Some(Rect {
        x: visible.x * sx,
        y: visible.y * sy,
        width: visible.width * sx,
        height: visible.height * sy,
    })
}

trait IntoCoord {
    fn into_coord(self) -> f32;
}

impl IntoCoord for i32 {
    fn into_coord(self) -> f32 {
        self as f32
    }
}

impl IntoCoord for f32 {
    fn into_coord(self) -> f32 {
        self
    }
}

impl IntoCoord for f64 {
    fn into_coord(self) -> f32 {
        self as f32
    }
}

fn rect(
    x: impl IntoCoord,
    y: impl IntoCoord,
    width: impl IntoCoord,
    height: impl IntoCoord,
) -> Rect {
    Rect {
        x: x.into_coord(),
        y: y.into_coord(),
        width: width.into_coord(),
        height: height.into_coord(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{AppEntry, AppKind};
    use alloc::{string::String, vec};

    fn app(name: &str) -> AppEntry {
        AppEntry {
            directory: String::from(name),
            kind: AppKind::Elf,
            size: 1,
            has_icon: false,
            metadata: None,
        }
    }

    #[test]
    fn browser_frame_marks_the_selected_visible_row() {
        let catalog = vec![app("one"), app("two"), app("three")];
        let mut controller = AppController::new();
        controller.handle(crate::app::Input::Down, &catalog);
        let frame = browser_frame(&controller, &catalog);
        assert_eq!(frame.fills[0].bounds, rect(0, 0, 640, 480));
        assert_eq!(frame.fills[1].bounds, rect(0, 0, 640, 64));
        assert!(frame
            .fills
            .iter()
            .any(|fill| fill.color == ACCENT && fill.bounds.y > 90.0));
    }

    #[test]
    fn frame_respects_grid_capacity_and_empty_catalog() {
        let mut controller = AppController::new();
        let catalog: Vec<AppEntry> = Vec::new();
        let frame = browser_frame(&controller, &catalog);
        assert!(!frame.fills.is_empty());
        controller.set_browser_layout(
            crate::app::BrowserLayout {
                columns: 2,
                rows: 2,
            },
            &catalog,
        );
        assert_eq!(controller.visible_page(&catalog).len(), 0);
    }

    #[test]
    fn about_screen_uses_its_own_content_panel() {
        let mut controller = AppController::new();
        controller.screen = Screen::About;
        let frame = browser_frame(&controller, &[]);
        assert!(frame
            .fills
            .iter()
            .any(|fill| fill.bounds == rect(108, 92, 424, 324)));
        assert!(!frame
            .fills
            .iter()
            .any(|fill| fill.bounds == rect(162, 122, 316, 248)));
    }

    #[test]
    fn rect_mapping_clips_then_scales_for_widescreen() {
        assert_eq!(
            map_rect(rect(-10, 20, 40, 30), 640, 480, 852, 480),
            Some(rect(0, 20, 39.9375, 30))
        );
        assert_eq!(map_rect(rect(0, 0, 10, 10), 0, 480, 640, 480), None);
        assert_eq!(
            map_rect(rect(f32::NAN, 0, 10, 10), 640, 480, 640, 480),
            None
        );
    }
}
