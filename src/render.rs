//! Hardware-independent layout for the first HBC browser renderer.
//!
//! The legacy renderer combined layout, widget state, texture drawing, and
//! GX state changes in one queue. This module keeps layout as data. The Wii
//! adapter submits solid rectangles, HBC textures, app icons, and rasterized
//! TrueType text through `ogc-gx`.

use alloc::{format, string::String, vec::Vec};

use crate::{
    app::{
        ABOUT_CREDITS, ABOUT_MEMO_LINE_WIDTH, ABOUT_MEMO_VISIBLE_LINES, AppController,
        DETAIL_DESCRIPTION_LINE_WIDTH, DETAIL_DESCRIPTION_VISIBLE_LINES,
        MESSAGE_MEMO_VISIBLE_LINES, NoticeKind, PointerTarget, Screen,
    },
    catalog::AppEntry,
    metadata::{Rgba, ThemeMetadata},
    text_layout::wrap_memo_lines,
};

pub const LOGICAL_WIDTH: u16 = 640;
pub const WIDE_LOGICAL_WIDTH: u16 = 852;
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ButtonSize {
    Large,
    Small,
    Tiny,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogIcon {
    Info,
    Confirm,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeArt {
    Cursor { dragging: bool, shade: bool },
    Dialog,
    Progress,
    Throbber,
    MemoArrowUp,
    MemoArrowDown,
    DialogIcon(DialogIcon),
    About,
    Button(ButtonSize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusIndicator {
    Network { active: bool },
    Gecko { active: bool },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
    pub fills: Vec<FillRect>,
    pub commands: Vec<FrameCommand>,
    pub hit_targets: Vec<HitTarget>,
    /// Decorative bubbles from the animated HBC background.
    pub bubbles: Vec<BackgroundBubble>,
    pub text_runs: Vec<TextRun>,
    logical_width: u16,
    text_scale: f32,
    text_color: Option<Color>,
    text_file: Option<String>,
    text_fallback_file: Option<String>,
    text_size: Option<u32>,
    focused_button_style: TextStyle,
    unfocused_button_style: TextStyle,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BackgroundBubble {
    pub texture: u8,
    pub bounds: Rect,
}

/// Frame based port of HBC's rising background bubbles.
#[derive(Clone, Debug)]
pub struct BackgroundAnimation {
    bubbles: Vec<BackgroundBubbleState>,
    particles: Vec<BackgroundParticle>,
    seed: u32,
    update_calls: u16,
    desired_count: usize,
}

#[derive(Clone, Copy, Debug)]
struct BackgroundBubbleState {
    texture: u8,
    x: f32,
    y: f32,
    speed: f32,
    sway: f32,
    phase: f32,
    step: f32,
    size: f32,
    popped: bool,
}

#[derive(Clone, Copy, Debug)]
struct BackgroundParticle {
    parent: usize,
    state: BackgroundBubbleState,
}

impl BackgroundAnimation {
    pub fn new(logical_width: u16, seed: u32, minute_cycle: u16) -> Self {
        let mut state = Self {
            bubbles: Vec::new(),
            particles: Vec::new(),
            seed: seed.max(1),
            update_calls: 1,
            desired_count: bubble_count_for_minute(minute_cycle),
        };
        for _ in 0..state.desired_count {
            state.reset_bubble(logical_width, 0.0);
        }
        state
    }

    fn random(&mut self) -> f32 {
        self.seed = self.seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.seed >> 8) as f32 / 16_777_216.0
    }

    fn reset_bubble(&mut self, width: u16, y: f32) {
        let bubble = self.create_bubble(width, y);
        self.bubbles.push(bubble);
    }

    fn create_bubble(&mut self, width: u16, y: f32) -> BackgroundBubbleState {
        let texture = (self.random() * 3.0) as u8;
        let speed = 1.2 + self.random() * (4 - texture) as f32;
        BackgroundBubbleState {
            texture,
            x: self.random() * f32::from(width),
            y: if y == 0.0 {
                480.0 + self.random() * 200.0
            } else {
                y
            },
            speed,
            sway: 3.0 + self.random() * (texture + 1) as f32 * speed,
            phase: 0.0,
            step: core::f32::consts::TAU / (64.0 + self.random() * 64.0),
            size: 0.4 + self.random() * 0.6,
            popped: false,
        }
    }

    /// Advance one frame. `minute_cycle` is the current minute modulo 24 hours.
    /// The time-based count changes every 600 frames, as it does in HBC.
    pub fn tick(
        &mut self,
        logical_width: u16,
        minute_cycle: u16,
        pointer: Option<(f32, f32)>,
    ) -> Vec<BackgroundBubble> {
        if self.update_calls >= 600 {
            self.update_calls = 0;
            self.desired_count = bubble_count_for_minute(minute_cycle);
        } else {
            self.update_calls += 1;
        }
        if self.update_calls.is_multiple_of(6) {
            if self.bubbles.len() < self.desired_count {
                self.reset_bubble(logical_width, 0.0);
            } else if self.bubbles.len() > self.desired_count {
                self.bubbles.pop();
                self.particles
                    .retain(|particle| particle.parent < self.bubbles.len());
            }
        }

        if let Some((x, y)) = pointer {
            for i in 0..self.bubbles.len() {
                if !self.bubbles[i].popped && self.contains(i, x, y, BUBBLE_POP_RADIUS) {
                    self.pop(i);
                }
            }
            for particle in &mut self.particles {
                if !particle.state.popped
                    && contains_state(&particle.state, x, y, BUBBLE_POP_RADIUS)
                {
                    particle.state.popped = true;
                }
            }
        }

        for i in 0..self.bubbles.len() {
            if !self.bubbles[i].popped {
                self.move_bubble(i, logical_width);
            }
        }
        for particle in &mut self.particles {
            if !particle.state.popped {
                particle.state.y -= particle.state.speed;
                particle.state.phase += particle.state.step;
                if particle.state.y < -100.0 {
                    particle.state.popped = true;
                }
            }
        }
        self.particles.retain(|particle| !particle.state.popped);
        for i in 0..self.bubbles.len() {
            if self.bubbles[i].popped && !self.particles.iter().any(|particle| particle.parent == i)
            {
                self.bubbles[i] = self.create_bubble(logical_width, 0.0);
            }
        }

        let mut output = Vec::new();
        for (i, bubble) in self.bubbles.iter().enumerate() {
            if !bubble.popped {
                output.push(drawable_bubble(bubble));
            }
            for particle in self
                .particles
                .iter()
                .filter(|particle| particle.parent == i)
            {
                output.push(drawable_bubble(&particle.state));
            }
        }
        output
    }

    pub fn pop_all(&mut self) {
        for i in 0..self.bubbles.len() {
            if !self.bubbles[i].popped {
                self.pop(i);
            }
        }
    }

    fn contains(&self, index: usize, x: f32, y: f32, radius_scale: f32) -> bool {
        contains_state(&self.bubbles[index], x, y, radius_scale)
    }

    fn pop(&mut self, index: usize) {
        let bubble = self.bubbles[index];
        self.bubbles[index].popped = true;
        let count = 5 + (self.random() * 5.0) as usize;
        for _ in 0..count {
            let angle = self.random() * core::f32::consts::TAU;
            let dx = libm::sinf(angle) * self.random() * 40.0;
            let dy = (libm::cosf(angle) - 1.5) * self.random() * 30.0;
            let mut particle = bubble;
            particle.x += dx;
            particle.y += dy;
            particle.speed = bubble.speed - 0.5 + self.random() * (4 - bubble.texture) as f32;
            particle.sway = bubble.sway * (0.8 + self.random());
            particle.step = bubble.step * (0.8 + self.random() * 0.4);
            particle.size = bubble.size * (0.2 + self.random() * 0.2);
            particle.phase = bubble.phase;
            particle.popped = false;
            self.particles.push(BackgroundParticle {
                parent: index,
                state: particle,
            });
        }
    }

    fn move_bubble(&mut self, index: usize, logical_width: u16) {
        self.bubbles[index].y -= self.bubbles[index].speed;
        if self.bubbles[index].y < -100.0 {
            let y = 480.0 + self.random() * 200.0;
            self.bubbles[index] = self.create_bubble(logical_width, y);
        } else {
            self.bubbles[index].phase += self.bubbles[index].step;
        }
    }
}

const BUBBLE_POP_RADIUS: f32 = 0.8;

fn bubble_count_for_minute(minute: u16) -> usize {
    let minute = minute % 1440;
    let delta = 16usize;
    let count = if minute <= 720 {
        delta * usize::from(minute) / 720 + 4
    } else {
        delta * usize::from(1440 - minute) / 720 + 4
    };
    count.clamp(4, 20)
}

fn contains_state(bubble: &BackgroundBubbleState, x: f32, y: f32, radius_scale: f32) -> bool {
    let radius = 64.0 * bubble.size / 2.0 * radius_scale;
    let cx = bubble.x + libm::roundf(bubble.sway * libm::sinf(bubble.phase)) + 32.0;
    let cy = bubble.y - 32.0;
    (x - cx).abs() < radius && (y - cy).abs() < radius
}

fn drawable_bubble(bubble: &BackgroundBubbleState) -> BackgroundBubble {
    let size = 64.0 * bubble.size;
    BackgroundBubble {
        texture: bubble.texture,
        bounds: Rect {
            x: bubble.x + libm::roundf(bubble.sway * libm::sinf(bubble.phase)),
            y: bubble.y - size,
            width: size,
            height: size,
        },
    }
}

#[derive(Clone, Debug, PartialEq)]
struct TextStyle {
    scale: f32,
    color: Option<Color>,
    file: Option<String>,
    fallback_file: Option<String>,
    size: Option<u32>,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            scale: 1.0,
            color: None,
            file: None,
            fallback_file: None,
            size: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub x: f32,
    pub y: f32,
    pub value: String,
    pub scale: f32,
    pub color: Color,
    pub file: Option<String>,
    pub fallback_file: Option<String>,
    pub size: u16,
    pub memo: Option<crate::text_layout::MemoSpec>,
    pub right_align: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitTarget {
    pub target: PointerTarget,
    pub bounds: Rect,
}

impl Frame {
    /// Resolve pending memos with the font face used by the production renderer.
    pub fn resolve_memos(
        &mut self,
        mut layout: impl FnMut(
            &TextRun,
            crate::text_layout::MemoSpec,
        )
            -> Result<crate::text_layout::MemoLayout, crate::font::FontError>,
    ) -> Result<Option<(usize, u16)>, crate::font::FontError> {
        let mut resolved = Vec::new();
        let mut bounds = None;
        for run in &self.text_runs {
            if let Some(spec) = run.memo {
                let memo = layout(run, spec)?;
                bounds = Some((memo.max_scroll, memo.line_height as u16));
                for (index, line) in memo.lines.into_iter().enumerate() {
                    let mut line_run = run.clone();
                    line_run.value = line;
                    line_run.y += index as f32 * memo.line_height;
                    line_run.memo = None;
                    resolved.push(line_run);
                }
            } else {
                resolved.push(run.clone());
            }
        }
        self.text_runs = resolved;
        Ok(bounds)
    }

    fn set_text_style(&mut self, style: TextStyle) {
        self.text_scale = style.scale;
        self.text_color = style.color;
        self.text_file = style.file;
        self.text_fallback_file = style.fallback_file;
        self.text_size = style.size;
    }
    pub fn logical_width(&self) -> u16 {
        if self.logical_width == 0 {
            LOGICAL_WIDTH
        } else {
            self.logical_width
        }
    }

    /// Return the topmost interactive item at this logical UI coordinate.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<PointerTarget> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        self.hit_targets.iter().rev().find_map(|hit| {
            (x >= hit.bounds.x
                && y >= hit.bounds.y
                && x <= hit.bounds.right()
                && y <= hit.bounds.bottom())
            .then_some(hit.target)
        })
    }

    pub fn hit_target(&mut self, target: PointerTarget, bounds: Rect) {
        self.hit_targets.push(HitTarget { target, bounds });
    }

    /// Add a compact native cursor above the frame's existing artwork.
    pub fn pointer_cursor(&mut self, x: f32, y: f32, dragging: bool) {
        let bounds = rect(x - 48.0, y - 48.0, 96.0, 96.0);
        self.theme_art(
            rect(bounds.x + 2.0, bounds.y + 4.0, bounds.width, bounds.height),
            ThemeArt::Cursor {
                dragging,
                shade: true,
            },
            false,
        );
        self.theme_art(
            bounds,
            ThemeArt::Cursor {
                dragging,
                shade: false,
            },
            false,
        );
    }

    pub fn storage_loading(&mut self) {
        let x = (f32::from(self.logical_width()) - 64.0) / 2.0;
        self.theme_art(rect(x, 208.0, 64.0, 64.0), ThemeArt::Throbber, false);
    }

    fn widen_canvas(&mut self) {
        // Existing layout constants use a centered 640-pixel design canvas.
        // Extend full-width fills and move its contents into the 852-pixel view.
        let offset = f32::from(WIDE_LOGICAL_WIDTH - LOGICAL_WIDTH) / 2.0;
        for fill in &mut self.fills {
            if fill.bounds.x == 0.0
                && fill.bounds.width == f32::from(LOGICAL_WIDTH)
                && (fill.bounds.height == 64.0 || fill.bounds.height == 480.0)
            {
                fill.bounds.width = f32::from(WIDE_LOGICAL_WIDTH);
            } else {
                fill.bounds.x += offset;
            }
        }
        for command in &mut self.commands {
            match command {
                FrameCommand::Fill(_) => {}
                FrameCommand::PageArrow {
                    previous,
                    bounds,
                    fallback,
                    ..
                } => {
                    bounds.x = if *previous {
                        24.0
                    } else {
                        f32::from(WIDE_LOGICAL_WIDTH) - 88.0
                    };
                    fallback.bounds.x = bounds.x;
                }
                FrameCommand::AppCard {
                    bounds, fallback, ..
                }
                | FrameCommand::ThemeArt {
                    bounds, fallback, ..
                } => {
                    bounds.x += offset;
                    fallback.bounds.x += offset;
                }
                FrameCommand::Bubble { bounds, .. } => bounds.x += offset,
                FrameCommand::Icon { bounds, .. } => bounds.x += offset,
                FrameCommand::StatusIndicator { bounds, .. } => bounds.x += offset,
            }
        }
        for text in &mut self.text_runs {
            text.x += offset;
        }
        for hit in &mut self.hit_targets {
            match hit.target {
                PointerTarget::Page { forward: false } => hit.bounds.x = 24.0,
                PointerTarget::Page { forward: true } => {
                    hit.bounds.x = f32::from(WIDE_LOGICAL_WIDTH) - 88.0
                }
                _ => hit.bounds.x += offset,
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrameCommand {
    Fill(usize),
    Bubble {
        texture: u8,
        bounds: Rect,
    },
    AppCard {
        grid: bool,
        focused: bool,
        bounds: Rect,
        fallback: FillRect,
    },
    PageArrow {
        previous: bool,
        focused: bool,
        bounds: Rect,
        fallback: FillRect,
    },
    ThemeArt {
        art: ThemeArt,
        focused: bool,
        bounds: Rect,
        fallback: FillRect,
    },
    Icon {
        catalog_index: usize,
        bounds: Rect,
    },
    StatusIndicator {
        indicator: StatusIndicator,
        bounds: Rect,
    },
}

impl Frame {
    /// Add the two HBC connection indicators to the browser footer.
    pub fn status_indicators(&mut self, network_active: bool, gecko_active: bool) {
        let y = 412.0;
        let right = f32::from(self.logical_width()) - 32.0;
        self.commands.push(FrameCommand::StatusIndicator {
            indicator: StatusIndicator::Gecko {
                active: gecko_active,
            },
            bounds: rect(right - 64.0, y, 32.0, 32.0),
        });
        self.commands.push(FrameCommand::StatusIndicator {
            indicator: StatusIndicator::Network {
                active: network_active,
            },
            bounds: rect(right - 32.0, y, 32.0, 32.0),
        });
    }

    /// Add rising bubbles before foreground UI elements.
    pub fn set_background_bubbles(&mut self, bubbles: &[BackgroundBubble]) {
        self.bubbles.clear();
        self.bubbles.extend_from_slice(bubbles);
        let insert_at = self
            .commands
            .iter()
            .position(|c| !matches!(c, FrameCommand::Fill(_)))
            .unwrap_or(self.commands.len());
        for (offset, bubble) in bubbles.iter().enumerate() {
            self.commands.insert(
                insert_at + offset,
                FrameCommand::Bubble {
                    texture: bubble.texture.min(2),
                    bounds: bubble.bounds,
                },
            );
        }
    }
}

impl Frame {
    fn app_card(&mut self, bounds: Rect, grid: bool, focused: bool) {
        if ![bounds.x, bounds.y, bounds.width, bounds.height]
            .iter()
            .all(|value| value.is_finite())
            || bounds.width <= 0.0
            || bounds.height <= 0.0
        {
            return;
        }
        let color = if focused { ACCENT } else { ROW };
        self.commands.push(FrameCommand::AppCard {
            grid,
            focused,
            bounds,
            fallback: FillRect { bounds, color },
        });
    }

    fn page_arrow(&mut self, bounds: Rect, previous: bool, focused: bool) {
        if ![bounds.x, bounds.y, bounds.width, bounds.height]
            .iter()
            .all(|value| value.is_finite())
            || bounds.width <= 0.0
            || bounds.height <= 0.0
        {
            return;
        }
        self.commands.push(FrameCommand::PageArrow {
            previous,
            focused,
            bounds,
            fallback: FillRect {
                bounds,
                color: MUTED,
            },
        });
    }

    fn theme_art(&mut self, bounds: Rect, art: ThemeArt, focused: bool) {
        if ![bounds.x, bounds.y, bounds.width, bounds.height]
            .iter()
            .all(|value| value.is_finite())
            || bounds.width <= 0.0
            || bounds.height <= 0.0
        {
            return;
        }
        let color = if matches!(art, ThemeArt::Cursor { .. }) {
            Color::default()
        } else if focused {
            ACCENT
        } else {
            PANEL
        };
        self.commands.push(FrameCommand::ThemeArt {
            art,
            focused,
            bounds,
            fallback: FillRect { bounds, color },
        });
    }

    fn button(
        &mut self,
        x: f32,
        y: f32,
        size: ButtonSize,
        focused: bool,
        label: &str,
        color: Color,
    ) -> Rect {
        let width = match size {
            ButtonSize::Large => 340.0,
            ButtonSize::Small => 200.0,
            ButtonSize::Tiny => 132.0,
        };
        self.button_with_geometry(
            rect(x, y, width, 48.0),
            y + 20.0,
            size,
            focused,
            label,
            color,
        )
    }

    fn compact_button(
        &mut self,
        x: f32,
        y: f32,
        size: ButtonSize,
        focused: bool,
        label: &str,
        color: Color,
    ) -> Rect {
        self.button_with_geometry(
            rect(x, y, 200.0, 28.0),
            y + 10.0,
            size,
            focused,
            label,
            color,
        )
    }

    fn button_with_geometry(
        &mut self,
        bounds: Rect,
        text_y: f32,
        size: ButtonSize,
        focused: bool,
        label: &str,
        color: Color,
    ) -> Rect {
        let width = bounds.width;
        let x = bounds.x;
        self.theme_art(bounds, ThemeArt::Button(size), focused);
        let style = if focused {
            self.focused_button_style.clone()
        } else {
            self.unfocused_button_style.clone()
        };
        self.set_text_style(style);
        let char_advance = self
            .text_size
            .map_or(6.0 * self.text_scale, |size| size as f32 * 0.55);
        let text_width = label.chars().count() as f32 * char_advance;
        self.text(
            x + ((width - text_width).max(0.0) / 2.0),
            text_y,
            label,
            1.0,
            color,
            (width / char_advance).max(1.0) as usize,
        );
        bounds
    }

    fn fill(&mut self, bounds: Rect, color: Color) {
        if bounds.width.is_finite()
            && bounds.height.is_finite()
            && bounds.x.is_finite()
            && bounds.y.is_finite()
            && bounds.width > 0.0
            && bounds.height > 0.0
        {
            let index = self.fills.len();
            self.fills.push(FillRect { bounds, color });
            self.commands.push(FrameCommand::Fill(index));
        }
    }

    fn icon(&mut self, catalog_index: usize, bounds: Rect) {
        if ![bounds.x, bounds.y, bounds.width, bounds.height]
            .iter()
            .all(|value| value.is_finite())
            || bounds.width <= 0.0
            || bounds.height <= 0.0
        {
            return;
        }
        self.commands.push(FrameCommand::Icon {
            catalog_index,
            bounds,
        });
    }

    fn text(&mut self, x: f32, y: f32, value: &str, scale: f32, color: Color, max_chars: usize) {
        let glyph_scale = scale;
        let style_scale = if self.text_scale == 0.0 {
            1.0
        } else {
            self.text_scale
        };
        let scale = scale * style_scale;
        let color = self.text_color.unwrap_or(color);
        if !scale.is_finite() || scale <= 0.0 {
            return;
        }
        if let Some(file) = self.text_file.clone() {
            self.text_runs.push(TextRun {
                x,
                y,
                value: value.chars().take(max_chars).collect(),
                scale: glyph_scale,
                color,
                file: Some(file),
                fallback_file: self.text_fallback_file.clone(),
                size: self.text_size.unwrap_or(16).clamp(1, 128) as u16,
                memo: None,
                right_align: false,
            });
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

    fn text_right(&mut self, right: f32, y: f32, value: &str, color: Color, max_chars: usize) {
        let value: String = value.chars().take(max_chars).collect();
        let style_scale = if self.text_scale == 0.0 {
            1.0
        } else {
            self.text_scale
        };
        let color = self.text_color.unwrap_or(color);
        if !style_scale.is_finite() || style_scale <= 0.0 {
            return;
        }
        if self.text_file.is_some() {
            self.text_runs.push(TextRun {
                x: right,
                y,
                value,
                scale: style_scale,
                color,
                file: self.text_file.clone(),
                fallback_file: self.text_fallback_file.clone(),
                size: self.text_size.unwrap_or(16).clamp(1, 128) as u16,
                memo: None,
                right_align: true,
            });
        } else {
            self.text(
                right - value.chars().count() as f32 * 6.0 * style_scale,
                y,
                &value,
                1.0,
                color,
                max_chars,
            );
        }
    }

    fn text_wrapped_scrolled(
        &mut self,
        x: f32,
        y: f32,
        value: &str,
        scale: f32,
        color: Color,
        layout: (usize, usize, usize),
    ) {
        let (max_chars, max_lines, scroll_lines) = layout;
        if max_chars == 0 || max_lines == 0 || !scale.is_finite() || scale <= 0.0 {
            return;
        }
        if self.text_file.is_some() {
            self.text(x, y, value, scale, color, usize::MAX);
            if let Some(run) = self.text_runs.last_mut() {
                run.memo = Some(crate::text_layout::MemoSpec {
                    width: max_chars as f32 * 6.0 * scale,
                    height: max_lines as f32 * 9.0 * scale,
                    scroll: scroll_lines,
                });
            }
            return;
        }
        let style_scale = if self.text_scale == 0.0 {
            1.0
        } else {
            self.text_scale
        };
        let line_scale = scale * style_scale;
        let styled_max_chars = ((max_chars as f32) / style_scale).max(1.0) as usize;
        let lines = wrap_memo_lines(value, styled_max_chars);
        let visible_lines = wrapped_line_window(&lines, max_lines, scroll_lines);
        for (line_index, line) in visible_lines.iter().enumerate() {
            self.text(
                x,
                y + line_index as f32 * 9.0 * line_scale,
                line,
                scale,
                color,
                styled_max_chars,
            );
        }
    }
}

fn wrapped_line_window(lines: &[String], visible_lines: usize, scroll_lines: usize) -> &[String] {
    let start = scroll_lines.min(lines.len().saturating_sub(visible_lines));
    &lines[start..lines.len().min(start.saturating_add(visible_lines))]
}

pub use ogc_ui::glyph_rows;

const BACKGROUND: Color = Color::rgb(15, 24, 35);
const PANEL: Color = Color::rgb(29, 44, 61);
const ROW: Color = Color::rgb(37, 54, 72);
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
    browser_frame_with_theme(controller, catalog, None)
}

/// Build a browser frame with metadata colors from an active HBC theme.
pub fn browser_frame_with_theme(
    controller: &AppController,
    catalog: &[AppEntry],
    theme: Option<&ThemeMetadata>,
) -> Frame {
    browser_frame_with_viewport(controller, catalog, theme, LOGICAL_WIDTH)
}

/// Build a browser frame for the HBC 4:3 or 16:9 logical viewport.
pub fn browser_frame_with_viewport(
    controller: &AppController,
    catalog: &[AppEntry],
    theme: Option<&ThemeMetadata>,
    logical_width: u16,
) -> Frame {
    browser_frame_with_language(
        controller,
        catalog,
        theme,
        logical_width,
        crate::localization::Language::English,
    )
}

/// Build a frame using the active Wii language and HBC catalog fallback rules.
pub fn browser_frame_with_language(
    controller: &AppController,
    catalog: &[AppEntry],
    theme: Option<&ThemeMetadata>,
    logical_width: u16,
    language: crate::localization::Language,
) -> Frame {
    let tr = |source| language.text(source);
    let logical_width = if logical_width > LOGICAL_WIDTH {
        WIDE_LOGICAL_WIDTH
    } else {
        LOGICAL_WIDTH
    };
    let mut frame = Frame {
        logical_width,
        ..Frame::default()
    };
    let text_color = theme
        .and_then(|theme| theme.default_font.color)
        .map(color_from_rgba)
        .unwrap_or(Color::rgb(213, 224, 233));
    frame.set_text_style(theme_text_style(theme, "label", text_color));
    frame.focused_button_style = theme_text_style(theme, "button", text_color);
    frame.unfocused_button_style = theme_text_style(theme, "button_desel", text_color);
    frame.fill(rect(0, 0, 640, 480), BACKGROUND);

    let layout = controller.browser_layout();
    let page = controller.visible_page(catalog);
    let navigation = controller.page_navigation(catalog);
    let page_start = navigation.start;
    let selected = controller.selected_index().saturating_sub(page_start);

    match controller.screen {
        Screen::Browser
        | Screen::AppDetails
        | Screen::DeleteConfirm
        | Screen::WiiloadConfirm
        | Screen::WiiloadInstalling
        | Screen::WiiloadReceiving => {
            if navigation.has_previous() {
                frame.page_arrow(
                    rect(24, 192, 64, 64),
                    true,
                    controller.pointer_target() == Some(PointerTarget::Page { forward: false }),
                );
                if controller.screen == Screen::Browser {
                    frame.hit_target(
                        PointerTarget::Page { forward: false },
                        rect(24, 192, 64, 64),
                    );
                }
            }
            if navigation.has_next() {
                frame.page_arrow(
                    rect(552, 192, 64, 64),
                    false,
                    controller.pointer_target() == Some(PointerTarget::Page { forward: true }),
                );
                if controller.screen == Screen::Browser {
                    frame.hit_target(
                        PointerTarget::Page { forward: true },
                        rect(552, 192, 64, 64),
                    );
                }
            }
            let columns = layout.columns.max(1);
            let rows = layout.rows.max(1);
            let grid = columns > 1;
            let cell_w = if grid { 144.0 } else { 432.0 };
            let cell_h = 64.0;
            let start_x = (640.0 - cell_w * columns as f32) / 2.0;
            for (index, app) in page.iter().enumerate() {
                let col = index % columns;
                let row = index / columns;
                if row >= rows {
                    break;
                }
                let x = start_x + col as f32 * cell_w;
                let y = 64.0 + row as f32 * cell_h;
                let is_selected = index == selected
                    && controller.selected_app.as_deref() == Some(app.directory.as_str());
                frame.app_card(rect(x, y, cell_w, cell_h), grid, is_selected);
                if controller.screen == Screen::Browser {
                    frame.hit_target(
                        PointerTarget::App(page_start + index),
                        rect(x, y, cell_w, cell_h),
                    );
                }
                let icon_bounds = rect(x + 8.0, y + 8.0, 128.0, 48.0);
                if app.has_icon {
                    if let Some(catalog_index) = catalog
                        .iter()
                        .position(|entry| entry.directory == app.directory)
                    {
                        frame.icon(catalog_index, icon_bounds);
                    }
                }
                // The GX renderer submits text after all artwork. Catalog
                // labels would otherwise cover a modal dialog drawn later.
                if controller.screen != Screen::Browser {
                    continue;
                }
                let name = app
                    .metadata
                    .as_ref()
                    .and_then(|meta| meta.name.as_deref())
                    .unwrap_or(app.directory.as_str());
                if grid {
                    continue;
                }
                let (text_x, name_y, description_y, text_width) =
                    (x + 156.0, y + 8.0, y + 36.0, cell_w - 168.0);
                frame.set_text_style(theme_text_style(theme, "appname", text_color));
                let max_chars = (text_width.max(6.0) / (6.0 * frame.text_scale)) as usize;
                frame.text(text_x, name_y, name, 1.0, text_color, max_chars);
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
                    frame.set_text_style(theme_text_style(theme, "appdesc", MUTED));
                    let desc_chars = (text_width.max(6.0) / (6.0 * frame.text_scale)) as usize;
                    frame.text(text_x, description_y, description, 1.0, MUTED, desc_chars);
                }
            }
        }
        Screen::MainMenu => {
            // A/B diagnosis of a first-frame regression: isolate only the
            // ordinary HOME menu layout while preserving the newer overlay code.
            for (index, item) in controller.menu_items().enumerate() {
                #[cfg(feature = "legacy-main-menu")]
                let y = 80.0
                    + index as f32
                        * if controller.bootmii_available() {
                            64.0
                        } else {
                            80.0
                        };
                #[cfg(not(feature = "legacy-main-menu"))]
                let y = 64.0 + index as f32 * 60.0;
                let bounds = frame.button(
                    150.0,
                    y,
                    ButtonSize::Large,
                    index == controller.menu_focus(),
                    tr(item.label()),
                    text_color,
                );
                frame.hit_target(PointerTarget::Menu(index), bounds);
            }
            let (ios_label, network_label) = controller.main_menu_status();
            frame.set_text_style(theme_text_style(theme, "label", text_color));
            frame.text(48.0, 32.0, network_label, 1.0, text_color, 64);
            let wide_offset = f32::from(logical_width.saturating_sub(LOGICAL_WIDTH)) / 2.0;
            let status_right = f32::from((logical_width / 3) * 3 - 48) - wide_offset;
            // C uses FreeType's face height. Droid Sans Bold at 16 px rounds
            // to 19 px; keep that spacing proportional for a theme label size.
            let status_line = libm::ceilf(frame.text_size.unwrap_or(16) as f32 * 1.1875);
            frame.text_right(status_right, 32.0, "1.1.3", text_color, 20);
            frame.text_right(status_right, 32.0 + status_line, ios_label, text_color, 24);
        }
        Screen::About => {
            frame.theme_art(rect(60, 44, 520, 360), ThemeArt::Dialog, false);
            frame.theme_art(rect(202, 60, 236, 104), ThemeArt::About, false);
            frame.set_text_style(theme_text_style(theme, "memo", text_color));
            let mut credits = String::new();
            if let Some((before_post, post)) = ABOUT_CREDITS.split_once("Powered by devkitPPC") {
                credits.push_str(before_post);
                let translation = tr("<YourLanguageHere> translation by <YourNickNameHere>");
                if translation != "<YourLanguageHere> translation by <YourNickNameHere>" {
                    credits.push_str("\n\n");
                    credits.push_str(translation);
                }
                if let Some(description) = theme
                    .and_then(|metadata| metadata.description.as_deref())
                    .filter(|description| !description.is_empty())
                {
                    credits.push_str(&format!("\n\n{} {}", tr("Theme:"), description));
                }
                credits.push_str("\n\nPowered by devkitPPC");
                credits.push_str(post);
            } else {
                credits.push_str(ABOUT_CREDITS);
            }
            frame.text_wrapped_scrolled(
                92.0,
                176.0,
                &credits,
                1.0,
                text_color,
                (
                    ABOUT_MEMO_LINE_WIDTH,
                    ABOUT_MEMO_VISIBLE_LINES,
                    controller.about_scroll(),
                ),
            );
        }
        Screen::Options
        | Screen::NetworkSettings
        | Screen::NetworkKeyboard
        | Screen::NetworkScan => {}
        Screen::Notice => {}
    }

    if matches!(
        controller.screen,
        Screen::AppDetails
            | Screen::DeleteConfirm
            | Screen::WiiloadConfirm
            | Screen::WiiloadInstalling
            | Screen::WiiloadReceiving
            | Screen::Options
            | Screen::NetworkSettings
            | Screen::NetworkKeyboard
            | Screen::NetworkScan
    ) {
        frame.fill(rect(0, 0, 640, 480), MODAL_SHADE);
        let dialog_bounds = if controller.screen == Screen::NetworkSettings {
            rect(20, 24, 600, 432)
        } else if controller.screen == Screen::NetworkKeyboard {
            let extra = f32::from(logical_width - LOGICAL_WIDTH);
            rect(24.0 - extra / 2.0, 28.0, 592.0 + extra, 388.0)
        } else {
            rect(60, 44, 520, 360)
        };
        frame.theme_art(dialog_bounds, ThemeArt::Dialog, false);
        if controller.screen == Screen::Options {
            frame.set_text_style(theme_text_style(theme, "dlgtitle", text_color));
            frame.text(299.0, 60.0, tr("Options"), 1.0, text_color, 24);
            frame.set_text_style(theme_text_style(theme, "label", text_color));
            frame.text(92.0, 80.0, tr("Device:"), 1.0, text_color, 32);
            for (index, (x, y, label)) in [
                (112.0, 108.0, tr("Internal SD Slot")),
                (328.0, 108.0, tr("USB device")),
                (112.0, 172.0, tr("SDGecko Slot A")),
                (328.0, 172.0, tr("SDGecko Slot B")),
            ]
            .into_iter()
            .enumerate()
            {
                let available = controller.device_available(index) == Some(true);
                let color = if available { ACCENT } else { MUTED };
                frame.fill(rect(x, y + 19.0, 10.0, 10.0), color);
                frame.text(x + 18.0, y + 20.0, label, 1.0, color, 27);
            }
            frame.set_text_style(theme_text_style(theme, "label", text_color));
            frame.text(
                92.0,
                224.0,
                tr("Sort applications by:"),
                1.0,
                text_color,
                32,
            );
            let bounds = frame.button(
                112.0,
                248.0,
                ButtonSize::Small,
                controller.options_focus() == 4,
                tr("Name"),
                if controller.pending_sort() == crate::catalog::SortOrder::Name {
                    ACCENT
                } else {
                    text_color
                },
            );
            frame.hit_target(PointerTarget::Option(4), bounds);
            let bounds = frame.button(
                328.0,
                248.0,
                ButtonSize::Small,
                controller.options_focus() == 5,
                tr("Date"),
                if controller.pending_sort() == crate::catalog::SortOrder::ReleaseDate {
                    ACCENT
                } else {
                    text_color
                },
            );
            frame.hit_target(PointerTarget::Option(5), bounds);
            let bounds = frame.compact_button(
                224.0,
                310.0,
                ButtonSize::Small,
                controller.options_focus() == 8,
                tr("Network setup"),
                text_color,
            );
            frame.hit_target(PointerTarget::Option(8), bounds);
            let bounds = frame.button(
                92.0,
                340.0,
                ButtonSize::Small,
                controller.options_focus() == 6,
                tr("Ok"),
                text_color,
            );
            frame.hit_target(PointerTarget::Option(6), bounds);
            let bounds = frame.button(
                348.0,
                340.0,
                ButtonSize::Small,
                controller.options_focus() == 7,
                tr("Back"),
                text_color,
            );
            frame.hit_target(PointerTarget::Option(7), bounds);
        } else if controller.screen == Screen::NetworkSettings {
            frame.set_text_style(theme_text_style(theme, "dlgtitle", text_color));
            frame.text(235.0, 48.0, tr("Network setup"), 1.0, text_color, 32);
            let ssid = core::str::from_utf8(controller.network_ssid()).unwrap_or("(invalid text)");
            let key = "*".repeat(controller.network_key().len());
            for (index, (label, value)) in [
                (tr("Network name"), ssid),
                (
                    tr("Security"),
                    match controller.network_encryption() {
                        0 => "Open",
                        1 => "WEP64",
                        2 => "WEP128",
                        4 => "WPA TKIP",
                        5 => "WPA2 AES",
                        6 => "WPA AES",
                        _ => "Unknown",
                    },
                ),
                (tr("Password"), &key),
            ]
            .into_iter()
            .enumerate()
            {
                let y = 102.0 + index as f32 * 72.0;
                frame.set_text_style(theme_text_style(theme, "label", text_color));
                frame.text(92.0, y + 14.0, label, 1.0, text_color, 24);
                let shown = if value.is_empty() { "(empty)" } else { value };
                frame.text(244.0, y + 14.0, shown, 1.0, text_color, 46);
                let bounds = frame.button(
                    420.0,
                    y,
                    ButtonSize::Tiny,
                    controller.network_focus() == index,
                    if index == 1 { tr("Change") } else { tr("Edit") },
                    text_color,
                );
                frame.hit_target(PointerTarget::NetworkOption(index), bounds);
            }
            for (index, x, label) in [
                (3, 32.0, tr("Save")),
                (4, 228.0, tr("Test connection")),
                (5, 424.0, tr("Back")),
            ] {
                let bounds = frame.button_with_geometry(
                    rect(x, 340.0, 184.0, 48.0),
                    360.0,
                    ButtonSize::Small,
                    controller.network_focus() == index,
                    label,
                    text_color,
                );
                frame.hit_target(PointerTarget::NetworkOption(index), bounds);
            }
            let bounds = frame.button_with_geometry(
                rect(32.0, 396.0, 184.0, 48.0),
                416.0,
                ButtonSize::Small,
                controller.network_focus() == 6,
                tr("Scan networks"),
                text_color,
            );
            frame.hit_target(PointerTarget::NetworkOption(6), bounds);
            let bounds = frame.button_with_geometry(
                rect(228.0, 396.0, 364.0, 48.0),
                416.0,
                ButtonSize::Small,
                controller.network_focus() == 7,
                tr("Write without backup"),
                text_color,
            );
            frame.hit_target(PointerTarget::NetworkOption(7), bounds);
        } else if controller.screen == Screen::NetworkScan {
            frame.set_text_style(theme_text_style(theme, "dlgtitle", text_color));
            frame.text(235.0, 60.0, tr("Choose a network"), 1.0, text_color, 32);
            for (index, network) in controller.scanned_networks().iter().take(8).enumerate() {
                let y = 104.0 + index as f32 * 32.0;
                let ssid =
                    core::str::from_utf8(network.name_bytes()).unwrap_or("(unsupported SSID)");
                frame.text(92.0, y, ssid, 0.9, text_color, 28);
                frame.text(352.0, y, network.security.label(), 0.8, MUTED, 18);
                frame.text(494.0, y, network.signal_label(), 0.75, MUTED, 10);
                let bounds = rect(84, y as i32 - 10, 472, 28);
                frame.hit_target(PointerTarget::NetworkOption(index), bounds);
            }
            if controller.scanned_networks().is_empty() {
                frame.text(
                    100.0,
                    180.0,
                    tr("No visible networks found."),
                    1.0,
                    text_color,
                    48,
                );
            }
            frame.text(
                112.0,
                374.0,
                tr("Confirm: select  Back: return"),
                0.9,
                MUTED,
                44,
            );
        } else if controller.screen == Screen::NetworkKeyboard {
            let extra_width = f32::from(logical_width - LOGICAL_WIDTH);
            let keyboard_left = 44.0 - extra_width / 2.0;
            let keyboard_width = 552.0 + extra_width;
            let key_pitch = keyboard_width / 10.0;
            let command_count = if controller.keyboard_target_key() {
                7.0
            } else {
                6.0
            };
            let command_pitch = keyboard_width / command_count;
            frame.set_text_style(theme_text_style(theme, "dlgtitle", text_color));
            let title = if controller.keyboard_target_key() {
                tr("Enter network password")
            } else {
                tr("Enter network name")
            };
            frame.text(198.0, 58.0, title, 1.0, text_color, 46);
            let value = network_keyboard_value(controller);
            frame.theme_art(
                rect(keyboard_left, 86.0, keyboard_width, 34.0),
                ThemeArt::Button(ButtonSize::Small),
                false,
            );
            frame.set_text_style(theme_text_style(theme, "label", text_color));
            frame.text(keyboard_left + 12.0, 98.0, &value, 1.0, text_color, 72);
            for index in 0..40 {
                let x = keyboard_left + (index % 10) as f32 * key_pitch;
                let y = 132.0 + (index / 10) as f32 * 38.0;
                let bounds = rect(x, y, key_pitch - 4.0, 34.0);
                frame.theme_art(
                    bounds,
                    ThemeArt::Button(ButtonSize::Tiny),
                    controller.keyboard_focus() == index,
                );
                let character = controller.keyboard_char(index).unwrap_or('?');
                let label = [character as u8];
                frame.text(
                    x + (key_pitch - 4.0) / 2.0 - 4.0,
                    y + 12.0,
                    core::str::from_utf8(&label).unwrap_or("?"),
                    1.0,
                    text_color,
                    4,
                );
                frame.hit_target(PointerTarget::KeyboardKey(index), bounds);
            }
            let command_end = if controller.keyboard_target_key() {
                47
            } else {
                46
            };
            for index in 40..command_end {
                let x = keyboard_left + (index - 40) as f32 * command_pitch;
                let y = 294.0;
                let bounds = rect(x, y, command_pitch - 6.0, 34.0);
                frame.theme_art(
                    bounds,
                    ThemeArt::Button(ButtonSize::Tiny),
                    controller.keyboard_focus() == index,
                );
                let label = controller.keyboard_command_label(index);
                frame.text(x + 8.0, y + 12.0, label, 0.9, text_color, 12);
                frame.hit_target(PointerTarget::KeyboardKey(index), bounds);
            }
            frame.text(
                keyboard_left + 8.0,
                352.0,
                tr("Select a key, then choose Done."),
                0.85,
                MUTED,
                64,
            );
            frame.text(
                keyboard_left + 8.0,
                374.0,
                tr("B returns without closing the editor."),
                0.85,
                MUTED,
                64,
            );
        } else if controller.screen == Screen::AppDetails {
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
                frame.set_text_style(theme_text_style(theme, "appname", text_color));
                let max_chars = (76.0 / frame.text_scale).max(1.0) as usize;
                let name_width =
                    name.chars().count().min(max_chars) as f32 * 6.0 * frame.text_scale;
                frame.text(
                    320.0 - name_width / 2.0,
                    60.0,
                    name,
                    1.0,
                    text_color,
                    max_chars,
                );
                if app.has_icon {
                    if let Some(catalog_index) = catalog
                        .iter()
                        .position(|entry| entry.directory == app.directory)
                    {
                        frame.icon(catalog_index, rect(92, 92, 128, 48));
                    }
                }
                if let Some(version) = app
                    .metadata
                    .as_ref()
                    .and_then(|meta| meta.version.as_deref())
                {
                    frame.set_text_style(theme_text_style(theme, "label", text_color));
                    frame.text(268.0, 116.0, version, 1.0, text_color, 42);
                }
                if let Some(author) = app
                    .metadata
                    .as_ref()
                    .and_then(|meta| meta.author.as_deref())
                {
                    frame.set_text_style(theme_text_style(theme, "label", text_color));
                    frame.text(268.0, 128.0, author, 1.0, text_color, 42);
                }
                let description = app
                    .metadata
                    .as_ref()
                    .and_then(|meta| meta.long_description.as_deref())
                    .map_or_else(|| tr("no description available"), |description| description);
                frame.set_text_style(theme_text_style(theme, "memo", text_color));
                frame.text_wrapped_scrolled(
                    92.0,
                    164.0,
                    description,
                    1.0,
                    text_color,
                    (
                        DETAIL_DESCRIPTION_LINE_WIDTH,
                        DETAIL_DESCRIPTION_VISIBLE_LINES,
                        controller.detail_description_scroll(),
                    ),
                );
                draw_memo_scroll_arrows(
                    &mut frame,
                    controller.detail_description_scroll(),
                    controller.detail_description_max_scroll(),
                    148.0,
                    324.0,
                );
                frame.hit_target(PointerTarget::DetailMemo, rect(88, 144, 464, 184));
            }
            let load_label = controller
                .selected_app
                .as_deref()
                .and_then(|selected| catalog.iter().find(|entry| entry.directory == selected))
                .filter(|app| app.kind == crate::catalog::AppKind::Theme)
                .map_or(tr("Load"), |_| tr("Apply"));
            for (index, action) in [8usize, 9, 10].iter().enumerate() {
                let bounds = frame.button(
                    91.0 + index as f32 * 163.0,
                    340.0,
                    ButtonSize::Tiny,
                    controller.detail_focus() == *action,
                    [tr("Delete"), load_label, tr("Back")][index],
                    text_color,
                );
                frame.hit_target(PointerTarget::Detail(*action), bounds);
            }
        } else if controller.screen == Screen::DeleteConfirm {
            frame.theme_art(
                rect(188, 68, 32, 32),
                ThemeArt::DialogIcon(DialogIcon::Confirm),
                false,
            );
            frame.set_text_style(theme_text_style(theme, "dlgtitle", text_color));
            frame.text(284.0, 76.0, tr("Confirmation"), 1.0, text_color, 12);
            frame.set_text_style(theme_text_style(theme, "memo", text_color));
            frame.text_wrapped_scrolled(
                92.0,
                120.0,
                tr("Do you really want to delete this application?"),
                1.0,
                text_color,
                (74, MESSAGE_MEMO_VISIBLE_LINES, 0),
            );
            for (index, yes) in [true, false].iter().enumerate() {
                let bounds = frame.button(
                    100.0 + index as f32 * 240.0,
                    324.0,
                    ButtonSize::Small,
                    controller.delete_confirmation_yes() == *yes,
                    if *yes { tr("Yes") } else { tr("No") },
                    text_color,
                );
                frame.hit_target(PointerTarget::DeleteChoice { yes: *yes }, bounds);
            }
        } else if controller.screen == Screen::WiiloadConfirm {
            frame.theme_art(
                rect(188, 68, 32, 32),
                ThemeArt::DialogIcon(DialogIcon::Confirm),
                false,
            );
            frame.set_text_style(theme_text_style(theme, "dlgtitle", text_color));
            frame.text(284.0, 76.0, tr("Confirmation"), 1.0, text_color, 12);
            frame.set_text_style(theme_text_style(theme, "memo", text_color));
            frame.text_wrapped_scrolled(
                92.0,
                120.0,
                tr(controller.wiiload_confirmation_text()),
                1.0,
                text_color,
                (
                    74,
                    MESSAGE_MEMO_VISIBLE_LINES,
                    controller.wiiload_confirmation_scroll(),
                ),
            );
            draw_memo_scroll_arrows(
                &mut frame,
                controller.wiiload_confirmation_scroll(),
                controller.wiiload_confirmation_max_scroll(),
                104.0,
                308.0,
            );
            frame.hit_target(PointerTarget::WiiloadMemo, rect(88, 116, 464, 200));
            for (index, yes) in [true, false].iter().enumerate() {
                let bounds = frame.button(
                    100.0 + index as f32 * 240.0,
                    324.0,
                    ButtonSize::Small,
                    controller.wiiload_confirmation_yes() == *yes,
                    if *yes { tr("Yes") } else { tr("No") },
                    text_color,
                );
                frame.hit_target(PointerTarget::WiiloadChoice { yes: *yes }, bounds);
            }
        } else {
            let dialog_bounds = if controller.screen == Screen::NetworkSettings {
                rect(40, 24, 560, 432)
            } else {
                rect(60, 44, 520, 360)
            };
            frame.theme_art(dialog_bounds, ThemeArt::Dialog, false);
            frame.set_text_style(theme_text_style(theme, "dlgtitle", text_color));
            let receiving = controller.screen == Screen::WiiloadReceiving;
            frame.text(
                if receiving { 250.0 } else { 252.0 },
                124.0,
                if receiving {
                    tr("Receiving upload")
                } else {
                    tr("Installing ZIP")
                },
                1.0,
                text_color,
                40,
            );
            frame.set_text_style(theme_text_style(theme, "label", text_color));
            let percent = controller.wiiload_install_progress();
            let label = [
                b'0' + percent / 100,
                b'0' + (percent / 10) % 10,
                b'0' + percent % 10,
                b'%',
            ];
            let label = core::str::from_utf8(&label).unwrap_or("000%");
            if percent > 0 {
                let stops = theme
                    .and_then(|metadata| metadata.progress_gradient)
                    .unwrap_or([Rgba(63, 155, 211, 255); 4]);
                let x = 155.0;
                let y = 255.0;
                let width = 330.0;
                // Four theme colors are gradient stops. Render short solid
                // spans so this path works with the GX rectangle renderer.
                const SPANS: usize = 32;
                for span in 0..SPANS {
                    let start = span as f32 / SPANS as f32;
                    if start >= f32::from(percent) / 100.0 {
                        break;
                    }
                    let end = ((span + 1) as f32 / SPANS as f32).min(f32::from(percent) / 100.0);
                    frame.fill(
                        rect(x + width * start, y, width * (end - start), 42),
                        gradient_color(stops, start),
                    );
                }
            }
            frame.theme_art(rect(120, 220, 400, 112), ThemeArt::Progress, false);
            frame.text(192.0, 190.0, tr("Please wait..."), 1.0, text_color, 40);
            frame.text(430.0, 292.0, label, 1.0, text_color, 5);
        }
    }
    if controller.screen == Screen::Notice {
        frame.fill(rect(0, 0, 640, 480), MODAL_SHADE);
        let dialog_bounds = if controller.screen == Screen::NetworkSettings {
            rect(40, 24, 560, 432)
        } else if controller.screen == Screen::NetworkKeyboard {
            let extra = f32::from(logical_width - LOGICAL_WIDTH);
            rect(24.0 - extra / 2.0, 28.0, 592.0 + extra, 388.0)
        } else {
            rect(60, 44, 520, 360)
        };
        frame.theme_art(dialog_bounds, ThemeArt::Dialog, false);
        frame.set_text_style(theme_text_style(theme, "dlgtitle", text_color));
        let (title, icon) = notice_presentation(controller.notice_kind());
        let title = tr(title);
        frame.theme_art(rect(188, 68, 32, 32), ThemeArt::DialogIcon(icon), false);
        let title_width = title.chars().count() as f32 * 6.0 * frame.text_scale;
        frame.text(
            (320.0 - title_width / 2.0).max(92.0),
            76.0,
            title,
            1.0,
            text_color,
            52,
        );
        if let Some(message) = controller.notice() {
            frame.set_text_style(theme_text_style(theme, "memo", text_color));
            frame.text_wrapped_scrolled(
                92.0,
                120.0,
                language.text(message),
                1.0,
                text_color,
                (74, MESSAGE_MEMO_VISIBLE_LINES, controller.notice_scroll()),
            );
            draw_memo_scroll_arrows(
                &mut frame,
                controller.notice_scroll(),
                controller.notice_scroll_max(),
                104.0,
                308.0,
            );
            frame.hit_target(PointerTarget::NoticeMemo, rect(88, 116, 464, 200));
        }
        let bounds = frame.button(
            220.0,
            324.0,
            ButtonSize::Small,
            controller.pointer_target() == Some(PointerTarget::NoticeConfirm),
            tr("OK"),
            text_color,
        );
        frame.hit_target(PointerTarget::NoticeConfirm, bounds);
    }
    if logical_width > LOGICAL_WIDTH {
        frame.widen_canvas();
    }
    frame
}

fn network_keyboard_value(controller: &AppController) -> String {
    if controller.keyboard_target_key() {
        if controller.network_key_visible() {
            core::str::from_utf8(controller.network_key())
                .unwrap_or("(invalid text)")
                .into()
        } else {
            "*".repeat(controller.network_key().len())
        }
    } else {
        core::str::from_utf8(controller.network_ssid())
            .unwrap_or("(invalid text)")
            .into()
    }
}

fn notice_presentation(kind: NoticeKind) -> (&'static str, DialogIcon) {
    match kind {
        NoticeKind::Generic => ("Notice", DialogIcon::Info),
        NoticeKind::Information => ("Information", DialogIcon::Info),
        NoticeKind::Warning => ("Warning", DialogIcon::Warning),
        NoticeKind::Error => ("Error", DialogIcon::Error),
    }
}

/// Show the same directional scroll cues as HBC's decorated memo widget.
/// The renderer resolves each cue through the active theme's image set.
fn draw_memo_scroll_arrows(
    frame: &mut Frame,
    offset: usize,
    max_offset: usize,
    top: f32,
    bottom: f32,
) {
    if offset < max_offset {
        frame.theme_art(rect(304.0, top, 32.0, 8.0), ThemeArt::MemoArrowUp, false);
    }
    if offset > 0 {
        frame.theme_art(
            rect(304.0, bottom, 32.0, 8.0),
            ThemeArt::MemoArrowDown,
            false,
        );
    }
}

fn gradient_color(stops: [Rgba; 4], position: f32) -> Color {
    // Convert the bounded position to 8-bit fixed point. Avoid float floor and
    // round, which are unavailable in the Wii no_std target.
    let scaled = (position * 768.0) as u16;
    let index = ((scaled / 256) as usize).min(2);
    let amount = scaled - index as u16 * 256;
    let a = stops[index];
    let b = stops[index + 1];
    let mix = |left: u8, right: u8| {
        let left = u16::from(left);
        let right = u16::from(right);
        ((left * (256 - amount) + right * amount + 128) / 256) as u8
    };
    Color {
        red: mix(a.0, b.0),
        green: mix(a.1, b.1),
        blue: mix(a.2, b.2),
        alpha: mix(a.3, b.3),
    }
}

fn color_from_rgba(Rgba(red, green, blue, alpha): Rgba) -> Color {
    Color {
        red,
        green,
        blue,
        alpha,
    }
}

/// Resolve one HBC font target. Target values override only fields they define;
/// missing values inherit from `default_font`, then from the renderer defaults.
fn theme_text_style(theme: Option<&ThemeMetadata>, target: &str, fallback: Color) -> TextStyle {
    let (default_file, default_size) = match target {
        "memo" => (crate::font::DEFAULT_REGULAR_FONT_KEY, 16),
        "button_desel" => (crate::font::DEFAULT_REGULAR_FONT_KEY, 20),
        "dlgtitle" | "appname" | "button" => (crate::font::DEFAULT_BOLD_FONT_KEY, 20),
        _ => (crate::font::DEFAULT_BOLD_FONT_KEY, 16),
    };
    let default = theme.map(|theme| &theme.default_font);
    let specific = theme.and_then(|theme| {
        theme.fonts.iter().find_map(|(targets, font)| {
            targets
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(target))
                .then_some(font)
        })
    });
    let size = specific
        .and_then(|font| font.size)
        .or_else(|| default.and_then(|font| font.size))
        .or(Some(default_size));
    // Keep theme sizes inside the fixed HBC layout and the font rasterizer's
    // supported range.
    let scale = size.map_or(1.0, |size| (size as f32 / 7.0).clamp(0.5, 2.5));
    let color = specific
        .and_then(|font| font.color)
        .or_else(|| default.and_then(|font| font.color))
        .map(color_from_rgba)
        .unwrap_or(fallback);
    let file = specific
        .and_then(|font| font.file.clone())
        .or_else(|| default.and_then(|font| font.file.clone()))
        .or_else(|| Some(alloc::string::String::from(default_file)));
    TextStyle {
        scale,
        color: Some(color),
        fallback_file: Some(alloc::string::String::from(default_file)),
        file,
        size,
    }
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

    #[test]
    fn status_indicators_use_hbc_footer_positions_and_states() {
        let mut frame = Frame {
            logical_width: LOGICAL_WIDTH,
            ..Frame::default()
        };
        frame.status_indicators(true, false);
        assert!(matches!(
            frame.commands[0],
            FrameCommand::StatusIndicator {
                indicator: StatusIndicator::Gecko { active: false },
                bounds: Rect {
                    x: 544.0,
                    y: 412.0,
                    width: 32.0,
                    height: 32.0
                },
            }
        ));
        assert!(matches!(
            frame.commands[1],
            FrameCommand::StatusIndicator {
                indicator: StatusIndicator::Network { active: true },
                bounds: Rect {
                    x: 576.0,
                    y: 412.0,
                    width: 32.0,
                    height: 32.0
                },
            }
        ));
    }

    #[test]
    fn rendered_menu_labels_and_hit_targets_execute_the_displayed_actions() {
        use crate::app::{Action, Input};
        let theme = ThemeMetadata {
            default_font: crate::metadata::ThemeFont {
                file: Some(String::from("droid.ttf")),
                ..Default::default()
            },
            ..Default::default()
        };
        for width in [LOGICAL_WIDTH, WIDE_LOGICAL_WIDTH] {
            for bootmii in [false, true] {
                let mut expected = vec![
                    ("Back", Action::None, Screen::Browser),
                    ("About", Action::None, Screen::About),
                ];
                if bootmii {
                    expected.push(("Launch BootMii", Action::LaunchBootMii, Screen::MainMenu));
                }
                expected.extend([
                    ("Exit to System Menu", Action::Exit, Screen::MainMenu),
                    ("Shutdown", Action::Shutdown, Screen::MainMenu),
                ]);
                #[cfg(not(feature = "legacy-main-menu"))]
                expected.push(("Restart network", Action::RestartNetwork, Screen::MainMenu));
                let mut controller = AppController::new();
                controller.set_bootmii_available(bootmii);
                controller.handle(Input::Home, &[]);
                let frame = browser_frame_with_viewport(&controller, &[], Some(&theme), width);
                let hits: Vec<_> = frame
                    .hit_targets
                    .iter()
                    .filter(|hit| matches!(hit.target, PointerTarget::Menu(_)))
                    .collect();
                assert_eq!(hits.len(), expected.len());
                for (index, (hit, (label, action, screen))) in hits.iter().zip(expected).enumerate()
                {
                    assert_eq!(hit.target, PointerTarget::Menu(index));
                    assert!(
                        frame.text_runs.iter().any(|run| run.value == label
                            && run.y >= hit.bounds.y
                            && run.y < hit.bounds.bottom()),
                        "missing row label {label}"
                    );
                    let target = frame.hit_test(
                        hit.bounds.x + hit.bounds.width / 2.0,
                        hit.bounds.y + hit.bounds.height / 2.0,
                    );
                    assert_eq!(target, Some(PointerTarget::Menu(index)));
                    controller.screen = Screen::MainMenu;
                    assert_eq!(
                        controller.handle_pointer(target, true, &[]),
                        action,
                        "{label}"
                    );
                    assert_eq!(controller.screen, screen);
                }
            }
        }
    }

    #[test]
    fn main_menu_status_text_stays_inside_narrow_and_wide_viewports() {
        let theme = ThemeMetadata {
            default_font: crate::metadata::ThemeFont {
                file: Some(String::from("droid.ttf")),
                ..Default::default()
            },
            ..Default::default()
        };
        for (width, expected_right) in [(LOGICAL_WIDTH, 591.0), (WIDE_LOGICAL_WIDTH, 804.0)] {
            let mut controller = AppController::new();
            controller.screen = Screen::MainMenu;
            controller.set_main_menu_status(
                String::from("IOS58 v24.0"),
                String::from("Network not initialized"),
            );
            let frame = browser_frame_with_viewport(&controller, &[], Some(&theme), width);
            let version = frame
                .text_runs
                .iter()
                .find(|run| run.value == "IOS58 v24.0")
                .expect("main menu must render the IOS status label");
            assert!(version.right_align);
            assert_eq!(version.x, expected_right);
            assert!(version.x <= f32::from(width) - 48.0);
        }
    }
    use crate::catalog::{AppEntry, AppKind};
    use alloc::{string::String, vec};

    #[test]
    fn memo_theme_arrows_follow_scroll_bounds() {
        let mut frame = Frame::default();
        draw_memo_scroll_arrows(&mut frame, 0, 4, 140.0, 300.0);
        assert!(matches!(
            frame.commands.as_slice(),
            [FrameCommand::ThemeArt {
                art: ThemeArt::MemoArrowUp,
                ..
            }]
        ));
        frame.commands.clear();
        draw_memo_scroll_arrows(&mut frame, 2, 4, 140.0, 300.0);
        assert!(matches!(
            frame.commands.as_slice(),
            [
                FrameCommand::ThemeArt {
                    art: ThemeArt::MemoArrowUp,
                    ..
                },
                FrameCommand::ThemeArt {
                    art: ThemeArt::MemoArrowDown,
                    ..
                }
            ]
        ));
        frame.commands.clear();
        draw_memo_scroll_arrows(&mut frame, 4, 4, 140.0, 300.0);
        assert!(matches!(
            frame.commands.as_slice(),
            [FrameCommand::ThemeArt {
                art: ThemeArt::MemoArrowDown,
                ..
            }]
        ));
        frame.commands.clear();
        draw_memo_scroll_arrows(&mut frame, 0, 0, 140.0, 300.0);
        assert!(frame.commands.is_empty(), "no arrows for short content");
    }

    #[test]
    fn storage_loading_frame_uses_theme_throbber_art() {
        let mut frame = Frame::default();
        frame.storage_loading();
        assert!(frame.commands.iter().any(|command| matches!(
            command,
            FrameCommand::ThemeArt {
                art: ThemeArt::Throbber,
                bounds,
                ..
            } if *bounds == rect(288, 208, 64, 64)
        )));
    }

    #[test]
    fn pointer_cursor_uses_hbc_standard_and_drag_art_with_c_offsets() {
        let mut frame = Frame::default();
        frame.pointer_cursor(100.0, 120.0, true);
        let cursors: Vec<_> = frame
            .commands
            .iter()
            .filter_map(|command| match command {
                FrameCommand::ThemeArt { art, bounds, .. } => Some((*art, *bounds)),
                _ => None,
            })
            .collect();
        assert_eq!(
            cursors,
            [
                (
                    ThemeArt::Cursor {
                        dragging: true,
                        shade: true,
                    },
                    rect(54.0, 76.0, 96.0, 96.0),
                ),
                (
                    ThemeArt::Cursor {
                        dragging: true,
                        shade: false,
                    },
                    rect(52.0, 72.0, 96.0, 96.0),
                ),
            ]
        );
    }

    #[test]
    fn target_font_fields_override_default_fields_independently() {
        let theme = ThemeMetadata {
            default_font: crate::metadata::ThemeFont {
                size: Some(14),
                color: Some(Rgba(1, 2, 3, 255)),
                ..Default::default()
            },
            fonts: vec![(
                vec![String::from("APPNAME")],
                crate::metadata::ThemeFont {
                    size: Some(7),
                    ..Default::default()
                },
            )],
            ..Default::default()
        };
        let style = theme_text_style(Some(&theme), "appname", Color::rgb(9, 8, 7));
        assert_eq!(style.scale, 1.0);
        assert_eq!(style.color, Some(Color::rgb(1, 2, 3)));
        let fallback = theme_text_style(Some(&theme), "memo", Color::rgb(9, 8, 7));
        assert_eq!(fallback.scale, 2.0);
        assert_eq!(fallback.color, Some(Color::rgb(1, 2, 3)));
    }

    #[test]
    fn custom_font_role_records_a_ttf_run_and_keeps_english_fallback_glyphs() {
        let theme = ThemeMetadata {
            default_font: crate::metadata::ThemeFont {
                file: Some(String::from("droid.ttf")),
                size: Some(18),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut frame = Frame::default();
        frame.set_text_style(theme_text_style(Some(&theme), "label", Color::rgb(1, 2, 3)));
        frame.text(4.0, 5.0, "A", 1.0, Color::rgb(0, 0, 0), 1);
        assert!(frame.fills.is_empty());
        assert_eq!(frame.text_runs.len(), 1);
        assert_eq!(frame.text_runs[0].file.as_deref(), Some("droid.ttf"));
        assert_eq!(
            frame.text_runs[0].fallback_file.as_deref(),
            Some(crate::font::DEFAULT_BOLD_FONT_KEY)
        );
        assert_eq!(frame.text_runs[0].size, 18);
        assert_eq!(frame.text_runs[0].value, "A");
        assert!(glyph_rows('A').iter().any(|row| *row != 0));
    }

    #[test]
    fn target_matching_is_case_insensitive_and_does_not_match_other_roles() {
        let theme = ThemeMetadata {
            fonts: vec![(
                vec![String::from("button_desel")],
                crate::metadata::ThemeFont {
                    color: Some(Rgba(30, 20, 10, 255)),
                    ..Default::default()
                },
            )],
            ..Default::default()
        };
        let style = theme_text_style(Some(&theme), "BUTTON_DESEL", Color::rgb(0, 0, 0));
        assert_eq!(style.color, Some(Color::rgb(30, 20, 10)));
        let other = theme_text_style(Some(&theme), "button", Color::rgb(0, 0, 0));
        assert_eq!(other.color, Some(Color::rgb(0, 0, 0)));
    }

    #[test]
    fn font_size_scales_native_seven_pixel_glyphs_with_safe_bounds() {
        let make_theme = |size| ThemeMetadata {
            default_font: crate::metadata::ThemeFont {
                size: Some(size),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            theme_text_style(Some(&make_theme(14)), "label", Color::rgb(1, 1, 1)).scale,
            2.0
        );
        assert_eq!(
            theme_text_style(Some(&make_theme(1)), "label", Color::rgb(1, 1, 1)).scale,
            0.5
        );
        assert_eq!(
            theme_text_style(Some(&make_theme(100)), "label", Color::rgb(1, 1, 1)).scale,
            2.5
        );
    }

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
        assert!(!frame.fills.iter().any(|fill| {
            fill.bounds == rect(0, 0, 640, 64) || fill.bounds == rect(28, 436, 584, 20)
        }));
        assert!(frame.commands.iter().any(|command| matches!(
            command,
            FrameCommand::AppCard {
                focused: true,
                bounds,
                ..
            } if *bounds == rect(104, 128, 432, 64)
        )));
    }

    #[test]
    fn browser_frame_does_not_add_a_block_for_an_app_without_an_icon() {
        let catalog = vec![app("no-icon")];
        let frame = browser_frame(&AppController::new(), &catalog);

        assert!(
            !frame
                .commands
                .iter()
                .any(|command| matches!(command, FrameCommand::Icon { .. }))
        );
        assert!(
            !frame
                .fills
                .iter()
                .any(|fill| fill.bounds == rect(112.0, 72.0, 128.0, 48.0))
        );
    }

    #[test]
    fn browser_description_stays_inside_its_app_row() {
        let mut entry = app("one");
        entry.metadata = Some(crate::metadata::AppMetadata {
            name: Some(String::from("One")),
            short_description: Some(String::from("A short description")),
            ..Default::default()
        });
        let theme = ThemeMetadata {
            default_font: crate::metadata::ThemeFont {
                file: Some(String::from("droid.ttf")),
                ..Default::default()
            },
            ..Default::default()
        };
        let frame = browser_frame_with_theme(&AppController::new(), &[entry], Some(&theme));
        let name = frame
            .text_runs
            .iter()
            .find(|run| run.value == "One")
            .unwrap();
        let description = frame
            .text_runs
            .iter()
            .find(|run| run.value == "A short description")
            .unwrap();
        assert_eq!(name.y, 72.0);
        assert_eq!(description.y, 100.0);
        assert!(description.y < 64.0 + 64.0);
    }

    #[test]
    fn widescreen_frame_uses_the_852_viewport_and_wide_edge_arrow() {
        let catalog: Vec<_> = (0..21).map(|index| app(&format!("app{index}"))).collect();
        let mut controller = AppController::new();
        controller.set_browser_layout(
            crate::app::BrowserLayout {
                columns: 4,
                rows: 5,
            },
            &catalog,
        );

        let frame = browser_frame_with_viewport(&controller, &catalog, None, WIDE_LOGICAL_WIDTH);
        assert_eq!(frame.logical_width(), WIDE_LOGICAL_WIDTH);
        assert_eq!(frame.fills[0].bounds, rect(0, 0, 852, 480));
        assert!(frame.commands.iter().any(|command| matches!(
            command,
            FrameCommand::AppCard {
                grid: true,
                bounds,
                ..
            } if *bounds == rect(138, 64, 144, 64)
        )));
        assert!(frame.commands.iter().any(|command| matches!(
            command,
            FrameCommand::PageArrow {
                previous: false,
                bounds,
                ..
            } if *bounds == rect(764, 192, 64, 64)
        )));
        assert_eq!(
            frame.hit_test(770.0, 200.0),
            Some(PointerTarget::Page { forward: true })
        );
    }

    #[test]
    fn frame_hit_test_uses_the_drawn_app_bounds_and_topmost_target() {
        let catalog = vec![app("one"), app("two")];
        let frame = browser_frame(&AppController::new(), &catalog);
        assert_eq!(frame.hit_test(110.0, 70.0), Some(PointerTarget::App(0)));
        assert_eq!(frame.hit_test(110.0, 130.0), Some(PointerTarget::App(1)));
        assert_eq!(frame.hit_test(100.0, 100.0), None);

        let mut frame = Frame::default();
        frame.hit_target(PointerTarget::Menu(0), rect(0, 0, 20, 20));
        frame.hit_target(PointerTarget::Menu(1), rect(0, 0, 20, 20));
        assert_eq!(frame.hit_test(10.0, 10.0), Some(PointerTarget::Menu(1)));
        assert_eq!(frame.hit_test(20.0, 20.0), Some(PointerTarget::Menu(1)));
    }

    #[test]
    fn page_arrows_follow_visible_page_bounds() {
        let catalog = vec![
            app("one"),
            app("two"),
            app("three"),
            app("four"),
            app("five"),
            app("six"),
        ];
        let mut controller = AppController::new();
        let first_page = browser_frame(&controller, &catalog);
        assert!(first_page.commands.iter().any(|command| matches!(
            command,
            FrameCommand::PageArrow {
                previous: false,
                bounds,
                ..
            } if *bounds == rect(552, 192, 64, 64)
        )));
        assert!(
            !first_page
                .commands
                .iter()
                .any(|command| matches!(command, FrameCommand::PageArrow { previous: true, .. }))
        );

        for _ in 0..5 {
            controller.handle(crate::app::Input::Down, &catalog);
        }
        let last_page = browser_frame(&controller, &catalog);
        assert!(last_page.commands.iter().any(|command| matches!(
            command,
            FrameCommand::PageArrow {
                previous: true,
                bounds,
                ..
            } if *bounds == rect(24, 192, 64, 64)
        )));
        assert!(!last_page.commands.iter().any(|command| matches!(
            command,
            FrameCommand::PageArrow {
                previous: false,
                ..
            }
        )));

        let target = last_page.hit_test(24.0, 256.0);
        assert_eq!(target, Some(PointerTarget::Page { forward: false }));
        controller.handle_pointer(target, false, &catalog);
        let focused_page = browser_frame(&controller, &catalog);
        assert!(focused_page.commands.iter().any(|command| matches!(
            command,
            FrameCommand::PageArrow {
                previous: true,
                focused: true,
                ..
            }
        )));
    }

    #[test]
    fn browser_frame_places_icon_data_before_app_text() {
        let mut entry = app("icon-app");
        entry.has_icon = true;
        let catalog = vec![entry];
        let frame = browser_frame(&AppController::new(), &catalog);
        let icon_command = frame
            .commands
            .iter()
            .position(|command| {
                matches!(
                    command,
                    FrameCommand::Icon {
                        catalog_index: 0,
                        ..
                    }
                )
            })
            .unwrap();
        let card_command = frame
            .commands
            .iter()
            .position(|command| matches!(command, FrameCommand::AppCard { .. }))
            .unwrap();
        let icon_bounds = frame
            .commands
            .iter()
            .find_map(|command| {
                if let FrameCommand::Icon { bounds, .. } = command {
                    Some(*bounds)
                } else {
                    None
                }
            })
            .unwrap();
        let has_app_text = frame
            .text_runs
            .iter()
            .any(|run| run.x >= 260.0 && run.y >= 72.0);

        assert_eq!(icon_bounds.width, 128.0);
        assert_eq!(icon_bounds.height, 48.0);
        assert!(card_command < icon_command);
        assert!(has_app_text);
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

        let catalog = vec![app("one"), app("two")];
        let grid_frame = browser_frame(&controller, &catalog);
        assert!(grid_frame.commands.iter().any(|command| matches!(
            command,
            FrameCommand::AppCard {
                grid: true,
                bounds,
                ..
            } if *bounds == rect(176, 64, 144, 64)
        )));
    }

    #[test]
    fn about_screen_uses_hbc_dialog_and_about_artwork() {
        let mut controller = AppController::new();
        controller.screen = Screen::About;
        let frame = browser_frame(&controller, &[]);
        assert!(frame.commands.iter().any(|command| matches!(
            command,
            FrameCommand::ThemeArt { art: ThemeArt::Dialog, bounds, .. }
                if *bounds == rect(60, 44, 520, 360)
        )));
        assert!(frame.commands.iter().any(|command| matches!(
            command,
            FrameCommand::ThemeArt { art: ThemeArt::About, bounds, .. }
                if *bounds == rect(202, 60, 236, 104)
        )));
        assert!(ABOUT_CREDITS.starts_with("Credits\n\n"));
        assert!(ABOUT_CREDITS.contains("dhewg (EWRONGCHAN) - application code, geckoloader code"));
        assert!(ABOUT_CREDITS.contains("FreeType - the free TrueType/OpenType font renderer"));
        assert!(!frame.commands.iter().any(|command| matches!(
            command,
            FrameCommand::ThemeArt {
                art: ThemeArt::Button(_),
                ..
            }
        )));
    }

    #[test]
    fn about_screen_draws_the_active_theme_description() {
        let mut controller = AppController::new();
        controller.screen = Screen::About;
        let theme = ThemeMetadata {
            description: Some(String::from("Water")),
            default_font: crate::metadata::ThemeFont {
                size: Some(16),
                ..crate::metadata::ThemeFont::default()
            },
            ..ThemeMetadata::default()
        };
        controller.set_about_theme_description(true);
        for _ in 0..10_000 {
            controller.tick_about_scroll();
        }
        let frame = browser_frame_with_language(
            &controller,
            &[],
            Some(&theme),
            LOGICAL_WIDTH,
            crate::localization::Language::English,
        );
        let without_description = browser_frame_with_language(
            &controller,
            &[],
            None,
            LOGICAL_WIDTH,
            crate::localization::Language::English,
        );
        assert!(
            frame
                .text_runs
                .iter()
                .any(|run| run.value.contains("Water"))
        );
        assert!(
            !without_description
                .text_runs
                .iter()
                .any(|run| run.value.contains("Water"))
        );
    }

    #[test]
    fn about_memo_resolves_with_the_draw_font_inside_the_dialog() {
        let mut controller = AppController::new();
        controller.screen = Screen::About;
        let mut frame = browser_frame(&controller, &[]);
        let face = crate::font::ThemeFontFace::parse(
            String::from("droid.ttf"),
            include_bytes!("../assets/droid.ttf").to_vec(),
        )
        .unwrap();
        let (max_scroll, line_height) = frame
            .resolve_memos(|run, spec| face.layout_memo(&run.value, spec, run.size, run.scale))
            .unwrap()
            .unwrap();
        assert!(max_scroll > 0);
        let rows: Vec<_> = frame
            .text_runs
            .iter()
            .filter(|run| run.y >= 176.0 && run.y < 356.0)
            .collect();
        assert!(rows.len() > 1);
        for row in rows {
            assert!(row.memo.is_none());
            assert!(row.y + f32::from(line_height) <= 356.0);
            assert!(row.x + face.measure_width(&row.value, row.size) * row.scale <= 548.0);
        }
        controller.set_measured_memo(max_scroll, line_height);
        for _ in 0..max_scroll + 1 {
            controller.handle(crate::app::Input::Down, &[]);
        }
        assert_eq!(controller.about_scroll(), max_scroll);
    }

    #[test]
    fn failed_memo_layout_preserves_the_unresolved_frame() {
        let mut controller = AppController::new();
        controller.screen = Screen::About;
        let mut frame = browser_frame(&controller, &[]);
        let original = frame.clone();
        assert_eq!(
            frame.resolve_memos(|_, _| Err(crate::font::FontError::InvalidFont)),
            Err(crate::font::FontError::InvalidFont)
        );
        assert_eq!(frame, original);
    }

    #[test]
    fn notice_kinds_select_the_hbc_title_and_icon_pair() {
        assert_eq!(
            notice_presentation(NoticeKind::Information),
            ("Information", DialogIcon::Info)
        );
        assert_eq!(
            notice_presentation(NoticeKind::Warning),
            ("Warning", DialogIcon::Warning)
        );
        assert_eq!(
            notice_presentation(NoticeKind::Error),
            ("Error", DialogIcon::Error)
        );
        assert_eq!(
            notice_presentation(NoticeKind::Generic),
            ("Notice", DialogIcon::Info)
        );
        for (kind, expected) in [
            (NoticeKind::Information, DialogIcon::Info),
            (NoticeKind::Warning, DialogIcon::Warning),
            (NoticeKind::Error, DialogIcon::Error),
        ] {
            let mut controller = AppController::new();
            controller.show_notice_with_kind(String::from("Message"), kind);
            let frame = browser_frame(&controller, &[]);
            assert!(frame.commands.iter().any(|command| matches!(
                command,
                FrameCommand::ThemeArt {
                    art: ThemeArt::DialogIcon(icon),
                    bounds,
                    ..
                } if *icon == expected && *bounds == rect(188, 68, 32, 32)
            )));
        }
    }

    #[test]
    fn delete_confirmation_uses_hbc_confirm_icon() {
        let mut controller = AppController::new();
        controller.screen = Screen::DeleteConfirm;
        let frame = browser_frame(&controller, &[]);
        assert!(frame.commands.iter().any(|command| matches!(
            command,
            FrameCommand::ThemeArt {
                art: ThemeArt::DialogIcon(DialogIcon::Confirm),
                bounds,
                ..
            } if *bounds == rect(188, 68, 32, 32)
        )));
    }

    #[test]
    fn wiiload_install_dialog_draws_progress_percentage_and_meter() {
        let mut controller = AppController::new();
        controller.show_wiiload_installing();
        controller.set_wiiload_install_progress(25);
        let frame = browser_frame(&controller, &[]);
        assert!(frame.commands.iter().any(|command| matches!(
            command,
            FrameCommand::ThemeArt { art: ThemeArt::Progress, bounds, .. }
                if *bounds == rect(120, 220, 400, 112)
        )));
        let progress: Vec<_> = frame
            .fills
            .iter()
            .filter(|fill| fill.bounds.y == 255.0 && fill.bounds.x >= 155.0)
            .collect();
        assert!(!progress.is_empty());
        assert!(progress.iter().all(|fill| fill.color == ACCENT));
        assert!(
            (progress.iter().map(|fill| fill.bounds.width).sum::<f32>() - 82.5).abs() < 0.01,
            "{progress:?}"
        );
    }

    #[test]
    fn wiiload_install_progress_uses_theme_gradient_stops() {
        let mut controller = AppController::new();
        controller.show_wiiload_installing();
        controller.set_wiiload_install_progress(100);
        let theme = ThemeMetadata {
            progress_gradient: Some([
                Rgba(1, 2, 3, 255),
                Rgba(30, 40, 50, 255),
                Rgba(70, 80, 90, 255),
                Rgba(110, 120, 130, 255),
            ]),
            ..ThemeMetadata::default()
        };
        let frame = browser_frame_with_theme(&controller, &[], Some(&theme));
        let progress: Vec<_> = frame
            .fills
            .iter()
            .filter(|fill| fill.bounds.y == 255.0 && fill.bounds.x >= 155.0)
            .collect();
        assert_eq!(
            progress.first().unwrap().color,
            Color {
                red: 1,
                green: 2,
                blue: 3,
                alpha: 255
            }
        );
        assert!(
            progress
                .iter()
                .any(|fill| fill.color.red > 1 && fill.color.red < 30)
        );
        assert!(
            progress
                .iter()
                .any(|fill| fill.color.red > 70 && fill.color.red < 110)
        );
        assert!((progress.iter().map(|fill| fill.bounds.width).sum::<f32>() - 330.0).abs() < 0.01);
        assert_eq!(
            gradient_color(theme.progress_gradient.unwrap(), 1.0),
            Color {
                red: 110,
                green: 120,
                blue: 130,
                alpha: 255
            }
        );
    }

    #[test]
    fn wiiload_receive_dialog_shows_live_progress_meter() {
        let mut controller = AppController::new();
        controller.show_wiiload_receiving();
        controller.set_wiiload_install_progress(35);
        let frame = browser_frame(&controller, &[]);
        assert!(frame.commands.iter().any(|command| matches!(
            command,
            FrameCommand::ThemeArt {
                art: ThemeArt::Progress,
                ..
            }
        )));
        let progress: Vec<_> = frame
            .fills
            .iter()
            .filter(|fill| fill.bounds.y == 255.0 && fill.bounds.x >= 155.0)
            .collect();
        assert!(progress.iter().all(|fill| fill.color == ACCENT));
        assert!((progress.iter().map(|fill| fill.bounds.width).sum::<f32>() - 115.5).abs() < 0.01);
    }

    #[test]
    fn wide_keyboard_uses_available_width_and_keeps_every_key_selectable() {
        let mut controller = AppController::new();
        controller.screen = Screen::NetworkKeyboard;
        for width in [LOGICAL_WIDTH, WIDE_LOGICAL_WIDTH] {
            let frame = browser_frame_with_viewport(&controller, &[], None, width);
            let keys: Vec<_> = frame
                .hit_targets
                .iter()
                .filter(|hit| matches!(hit.target, PointerTarget::KeyboardKey(_)))
                .collect();
            assert_eq!(keys.len(), 46);
            let first = keys[0].bounds;
            let last = keys[9].bounds;
            assert!(first.width > if width == LOGICAL_WIDTH { 50.0 } else { 70.0 });
            assert!((first.x - 44.0).abs() < 0.01);
            assert!((f32::from(width) - (last.x + last.width) - 48.0).abs() < 0.01);
            for key in keys {
                let bounds = key.bounds;
                assert!(bounds.x >= 24.0 && bounds.x + bounds.width <= f32::from(width) - 24.0);
                assert_eq!(
                    frame.hit_test(
                        bounds.x + bounds.width / 2.0,
                        bounds.y + bounds.height / 2.0
                    ),
                    Some(key.target)
                );
            }
        }
    }

    #[test]
    fn password_keyboard_renders_masked_and_visible_password_values() {
        let mut config = crate::network_config::NetworkConfig::from_bytes(
            [0; crate::network_config::CONFIG_LEN],
        );
        {
            let mut profile = config.profile_mut(0).unwrap();
            let mut wifi = profile.wifi_mut();
            assert!(wifi.set_ssid(b"HomeNet"));
            assert!(wifi.set_key(b"secret"));
        }
        let mut controller = AppController::new();
        controller.set_network_config(config);
        controller.screen = Screen::NetworkSettings;
        controller.handle_pointer(Some(PointerTarget::NetworkOption(2)), true, &[]);
        assert_eq!(network_keyboard_value(&controller), "******");
        let masked = browser_frame(&controller, &[]);
        assert!(masked.text_runs.iter().any(|run| run.value == "******"));
        assert!(!masked.text_runs.iter().any(|run| run.value == "secret"));

        controller.handle_pointer(Some(PointerTarget::KeyboardKey(46)), true, &[]);
        assert_eq!(network_keyboard_value(&controller), "secret");
        let visible = browser_frame(&controller, &[]);
        assert!(visible.text_runs.iter().any(|run| run.value == "secret"));
        assert!(!visible.text_runs.iter().any(|run| run.value == "******"));
    }

    #[test]
    fn network_settings_rows_and_actions_have_separate_pointer_bounds() {
        let mut controller = AppController::new();
        controller.screen = Screen::NetworkSettings;
        let frame = browser_frame(&controller, &[]);
        let row_edits: Vec<_> = frame
            .hit_targets
            .iter()
            .filter(|target| matches!(target.target, PointerTarget::NetworkOption(0..=2)))
            .map(|target| target.bounds.y)
            .collect();
        let save_y = frame
            .hit_targets
            .iter()
            .find(|target| target.target == PointerTarget::NetworkOption(3))
            .unwrap()
            .bounds
            .y;
        let test_y = frame
            .hit_targets
            .iter()
            .find(|target| target.target == PointerTarget::NetworkOption(4))
            .unwrap()
            .bounds
            .y;
        let action_bounds: Vec<_> = frame
            .hit_targets
            .iter()
            .filter(|target| matches!(target.target, PointerTarget::NetworkOption(3..=5)))
            .map(|target| target.bounds)
            .collect();
        let scan_y = frame
            .hit_targets
            .iter()
            .find(|target| target.target == PointerTarget::NetworkOption(6))
            .unwrap()
            .bounds
            .y;
        let bypass_bounds = frame
            .hit_targets
            .iter()
            .find(|target| target.target == PointerTarget::NetworkOption(7))
            .unwrap()
            .bounds;
        let label_y = |label: &str| {
            frame
                .text_runs
                .iter()
                .find(|run| run.value == label)
                .unwrap()
                .y
        };
        assert_eq!(row_edits, [102.0, 174.0, 246.0]);
        assert!(save_y - row_edits[2] >= 60.0);
        assert_eq!(test_y, save_y);
        assert!(scan_y - save_y >= 48.0);
        assert!(scan_y + 24.0 <= 456.0);
        assert_eq!(bypass_bounds.y, scan_y);
        assert_eq!(bypass_bounds.x, 228.0);
        assert!(bypass_bounds.width >= 360.0);
        assert!(bypass_bounds.right() <= 608.0);
        assert_eq!(label_y("Test connection"), 360.0);
        assert_eq!(label_y("Scan networks"), 416.0);
        assert_eq!(label_y("Write without backup"), 416.0);
        assert_eq!(action_bounds.len(), 3);
        for bounds in &action_bounds {
            assert!(bounds.width >= 180.0);
            assert!(bounds.x >= 32.0);
            assert!(bounds.right() <= 608.0);
        }
        assert!(action_bounds[0].right() < action_bounds[1].x);
        assert!(action_bounds[1].right() < action_bounds[2].x);
    }

    #[test]
    fn network_scan_help_uses_logical_labels_for_all_controller_kinds() {
        let mut controller = AppController::new();
        controller.screen = Screen::NetworkScan;
        let frame = browser_frame(&controller, &[]);
        let help = frame
            .text_runs
            .iter()
            .find(|run| run.value.contains("Confirm: select"))
            .unwrap();
        assert_eq!(help.value, "Confirm: select  Back: return");
        assert!(!help.value.contains("B:"));
    }

    #[test]
    fn options_dialog_does_not_show_a_message_icon() {
        let mut controller = AppController::new();
        controller.screen = Screen::Options;
        let frame = browser_frame(&controller, &[]);
        assert!(!frame.commands.iter().any(|command| matches!(
            command,
            FrameCommand::ThemeArt {
                art: ThemeArt::DialogIcon(_),
                ..
            }
        )));
    }

    #[test]
    fn options_device_labels_do_not_overlap_with_availability_text() {
        let mut controller = AppController::new();
        controller.screen = Screen::Options;
        controller.set_device_availability([false; 4]);
        let theme = ThemeMetadata {
            default_font: crate::metadata::ThemeFont {
                file: Some(String::from("droid.ttf")),
                ..Default::default()
            },
            ..Default::default()
        };

        let frame = browser_frame_with_theme(&controller, &[], Some(&theme));
        let labels: Vec<_> = frame
            .text_runs
            .iter()
            .map(|run| run.value.as_str())
            .collect();
        for label in [
            "Internal SD Slot",
            "USB device",
            "SDGecko Slot A",
            "SDGecko Slot B",
            "Network setup",
        ] {
            assert!(
                labels.contains(&label),
                "missing option label {label:?}; rendered {labels:?}"
            );
        }
        assert!(
            !labels
                .iter()
                .any(|label| { matches!(*label, "Available" | "Unavailable" | "Checking") })
        );
    }

    #[test]
    fn app_details_exposes_the_description_as_a_drag_target() {
        let catalog = vec![app("one")];
        let mut controller = AppController::new();
        controller.selected_app = Some(String::from("one"));
        controller.screen = Screen::AppDetails;
        let frame = browser_frame(&controller, &catalog);
        assert_eq!(
            frame.hit_test(100.0, 200.0),
            Some(PointerTarget::DetailMemo)
        );
        assert!(matches!(
            frame.hit_test(100.0, 350.0),
            Some(PointerTarget::Detail(8))
        ));
    }

    #[test]
    fn app_details_do_not_draw_catalog_labels_over_the_dialog() {
        let catalog = vec![app("one"), app("two")];
        let mut controller = AppController::new();
        controller.selected_app = Some(String::from("one"));
        controller.screen = Screen::AppDetails;
        let frame = browser_frame(&controller, &catalog);
        assert!(frame.text_runs.iter().any(|run| run.value == "one"));
        assert!(!frame.text_runs.iter().any(|run| run.value == "two"));
    }

    #[test]
    fn notice_exposes_its_wrapped_message_as_a_drag_target() {
        let mut controller = AppController::new();
        controller.show_notice("Long message ".repeat(80));
        let frame = browser_frame(&controller, &[]);
        assert_eq!(
            frame.hit_test(100.0, 200.0),
            Some(PointerTarget::NoticeMemo)
        );
        assert_eq!(
            frame.hit_test(230.0, 332.0),
            Some(PointerTarget::NoticeConfirm)
        );
    }

    #[test]
    fn wrapped_text_breaks_lines_and_obeys_the_visible_line_limit() {
        let mut frame = Frame::default();
        frame.text_wrapped_scrolled(
            10.0,
            20.0,
            "ONE TWO THREE FOUR FIVE",
            1.0,
            ACCENT,
            (7, 2, 0),
        );
        let mut line_ys: Vec<f32> = frame.fills.iter().map(|fill| fill.bounds.y).collect();
        line_ys.sort_by(f32::total_cmp);
        line_ys.dedup();
        assert!(line_ys.contains(&20.0));
        assert!(line_ys.contains(&29.0));
        assert!(line_ys.iter().all(|y| *y < 36.0));
    }

    #[test]
    fn wrapped_text_can_select_a_clamped_scrolled_window() {
        let lines = crate::text_layout::wrap_lines("ONE TWO THREE FOUR", 7);
        assert_eq!(lines, vec!["ONE TWO", "THREE", "FOUR"]);
        assert_eq!(wrapped_line_window(&lines, 2, 99), &["THREE", "FOUR"]);

        let mut frame = Frame::default();
        frame.text_wrapped_scrolled(10.0, 20.0, "ONE TWO THREE FOUR", 1.0, ACCENT, (7, 2, 99));
        let mut line_ys: Vec<f32> = frame.fills.iter().map(|fill| fill.bounds.y).collect();
        line_ys.sort_by(f32::total_cmp);
        line_ys.dedup();
        assert!(line_ys.contains(&20.0));
        assert!(line_ys.contains(&29.0));
        assert!(line_ys.iter().all(|y| *y < 36.0));
    }

    #[test]
    fn options_device_statuses_are_lights_and_only_actions_are_buttons() {
        let mut controller = AppController::new();
        controller.set_device_availability([true, false, true, false]);
        controller.handle(crate::app::Input::OpenOptions, &[]);
        let frame = browser_frame(&controller, &[]);
        assert!(frame.commands.iter().any(|command| matches!(
            command,
            FrameCommand::ThemeArt { art: ThemeArt::Dialog, bounds, .. }
                if *bounds == rect(60, 44, 520, 360)
        )));
        for expected in [
            rect(112, 248, 200, 48),
            rect(328, 248, 200, 48),
            rect(92, 340, 200, 48),
            rect(348, 340, 200, 48),
        ] {
            assert!(frame.commands.iter().any(|command| matches!(
                command,
                FrameCommand::ThemeArt { art: ThemeArt::Button(ButtonSize::Small), bounds, .. }
                    if *bounds == expected
            )));
        }
        assert!(frame.commands.iter().any(|command| matches!(
            command,
                FrameCommand::ThemeArt { art: ThemeArt::Button(ButtonSize::Small), bounds, .. }
                    if *bounds == rect(224, 310, 200, 28)
        )));
        for device_bounds in [
            rect(112, 108, 200, 48),
            rect(328, 108, 200, 48),
            rect(112, 172, 200, 48),
            rect(328, 172, 200, 48),
        ] {
            assert!(!frame.commands.iter().any(|command| matches!(
                command,
                FrameCommand::ThemeArt { art: ThemeArt::Button(_), bounds, .. }
                    if *bounds == device_bounds
            )));
        }
        assert!(frame.fills.contains(&FillRect {
            bounds: rect(112, 127, 10, 10),
            color: ACCENT,
        }));
        assert!(frame.fills.contains(&FillRect {
            bounds: rect(328, 127, 10, 10),
            color: MUTED,
        }));
        assert!((0..4).all(|index| {
            !frame
                .hit_targets
                .iter()
                .any(|target| target.target == PointerTarget::Option(index))
        }));
        assert_eq!(frame.hit_test(120.0, 120.0), None);
        assert_eq!(frame.hit_test(120.0, 80.0), None);
    }

    #[test]
    fn applies_theme_default_font_color_to_rendered_text() {
        let mut controller = AppController::new();
        controller.screen = Screen::About;
        let theme = ThemeMetadata {
            default_font: crate::metadata::ThemeFont {
                color: Some(Rgba(240, 180, 40, 255)),
                ..crate::metadata::ThemeFont::default()
            },
            ..ThemeMetadata::default()
        };
        let frame = browser_frame_with_theme(&controller, &[], Some(&theme));
        assert!(
            frame
                .text_runs
                .iter()
                .any(|run| run.color == Color::rgb(240, 180, 40))
        );
    }

    #[test]
    fn default_font_styles_match_hbc_regular_and_bold_faces() {
        let label = theme_text_style(None, "label", Color::rgb(255, 255, 255));
        let memo = theme_text_style(None, "memo", Color::rgb(255, 255, 255));
        let deselected = theme_text_style(None, "button_desel", Color::rgb(255, 255, 255));

        assert_eq!(
            label.file.as_deref(),
            Some(crate::font::DEFAULT_BOLD_FONT_KEY)
        );
        assert_eq!(label.size, Some(16));
        assert_eq!(
            memo.file.as_deref(),
            Some(crate::font::DEFAULT_REGULAR_FONT_KEY)
        );
        assert_eq!(memo.size, Some(16));
        assert_eq!(
            deselected.file.as_deref(),
            Some(crate::font::DEFAULT_REGULAR_FONT_KEY)
        );
        assert_eq!(deselected.size, Some(20));
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
    #[test]
    fn animated_background_bubbles_rise_at_hbc_dimensions() {
        let mut animation = BackgroundAnimation::new(LOGICAL_WIDTH, 7, 720);
        let first = animation.tick(LOGICAL_WIDTH, 720, None);
        let second = animation.tick(LOGICAL_WIDTH, 720, None);
        assert_eq!(first.len(), 20);
        assert_eq!(second.len(), 20);
        assert!(
            second
                .iter()
                .zip(first.iter())
                .all(|(next, prev)| next.bounds.y < prev.bounds.y || next.bounds.y > 480.0)
        );
        assert!(second.iter().all(|bubble| bubble.texture < 3));
    }

    #[test]
    fn animated_background_bubble_count_tracks_the_daily_cycle() {
        assert_eq!(bubble_count_for_minute(0), 4);
        assert_eq!(bubble_count_for_minute(720), 20);
        assert_eq!(bubble_count_for_minute(1439), 4);

        let mut animation = BackgroundAnimation::new(LOGICAL_WIDTH, 11, 0);
        for _ in 0..600 {
            animation.tick(LOGICAL_WIDTH, 720, None);
        }
        assert_eq!(animation.desired_count, 20);
        assert_eq!(animation.bubbles.len(), 5);
    }

    #[test]
    fn pointer_and_pop_all_create_bubble_fragments() {
        let mut animation = BackgroundAnimation::new(LOGICAL_WIDTH, 23, 720);
        let bubble = animation.tick(LOGICAL_WIDTH, 720, None)[0];
        let size = bubble.bounds.width;
        let pointer = (bubble.bounds.x + 32.0, bubble.bounds.y + size - 32.0);
        let fragments = animation.tick(LOGICAL_WIDTH, 720, Some(pointer));
        assert!(animation.bubbles[0].popped);
        assert!(!animation.particles.is_empty());
        assert!(fragments.len() > 4);
        assert!(fragments.len() < 30);

        animation.pop_all();
        assert!(animation.bubbles.iter().all(|bubble| bubble.popped));
        let all_fragments = animation.tick(LOGICAL_WIDTH, 720, None);
        assert!(all_fragments.len() >= 20);
        assert!(all_fragments.len() <= 180);
    }
}
