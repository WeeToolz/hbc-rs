//! Application-owned HOME strip. Policy is independent of Wii GX and the host's
//! actual controller, save, and system operations.
//!
//! A launched application must opt in and provide an `OverlayHost`; HBC cannot
//! draw into an unrelated application's display after transferring control.
use crate::home_overlay::{OperationResult, OverlayControl, OverlayError, Request};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Key {
    Home,
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameAction {
    None,
    Perform(Request),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RectSpec {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub color: [u8; 4],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Page {
    Strip,
    Dev,
    Exit,
    WiiMote,
}

/// The application must opt in to optional functionality. An absent callback
/// is never represented by an enabled button.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Capabilities {
    pub can_restart_app: bool,
    pub can_save: bool,
    pub can_find_remotes: bool,
    pub can_disconnect_remotes: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Slot {
    pub label: &'static str,
    pub request: Request,
}

const STRIP_LABELS: [&str; 5] = ["DEV", "", "Exit", "Shot", "WiiMote"];
const DEV_LABELS: [&str; 2] = ["Restart app", "Save"];
const EXIT_LABELS: [&str; 4] = [
    "The Homebrew Channel",
    "System Menu",
    "Restart Wii",
    "Power off",
];
const WM_LABELS: [&str; 2] = ["Find remotes", "Disconnect remotes"];
const EXIT_REQUESTS: [Request; 4] = [
    Request::ReturnToHbc,
    Request::ReturnToSystemMenu,
    Request::RestartConsole,
    Request::PowerOff,
];
const DEV_REQUESTS: [Request; 2] = [Request::RestartApp, Request::Save];
const WM_REQUESTS: [Request; 2] = [Request::FindRemotes, Request::DisconnectRemotes];

pub fn key_from_input(input: crate::app::Input) -> Option<Key> {
    use crate::app::Input;
    match input {
        Input::Home => Some(Key::Home),
        Input::Up => Some(Key::Up),
        Input::Down => Some(Key::Down),
        Input::Left => Some(Key::Left),
        Input::Right => Some(Key::Right),
        Input::Confirm => Some(Key::Confirm),
        Input::Back => Some(Key::Back),
        _ => None,
    }
}

pub trait OverlayText {
    fn draw_text(&mut self, label: &str, x: usize, y: usize, selected: bool);
}

pub struct AppHomeOverlay {
    control: OverlayControl,
    page: Page,
    selected: usize,
    wait_home_release: bool,
    confirm: Option<Request>,
    capabilities: Capabilities,
    slots: [Option<Slot>; 2],
}

impl Default for AppHomeOverlay {
    fn default() -> Self {
        Self {
            control: OverlayControl::default(),
            page: Page::Strip,
            selected: 3,
            wait_home_release: false,
            confirm: None,
            capabilities: Capabilities::default(),
            slots: [
                None,
                Some(Slot {
                    label: "Shot",
                    request: Request::Screenshot,
                }),
            ],
        }
    }
}

impl AppHomeOverlay {
    pub fn is_open(&self) -> bool {
        self.control.is_open()
    }
    pub fn page(&self) -> Page {
        self.page
    }
    pub fn selected(&self) -> usize {
        self.selected
    }
    pub fn pending_confirmation(&self) -> Option<Request> {
        self.confirm
    }
    pub fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    pub fn set_capabilities(&mut self, capabilities: Capabilities) {
        self.capabilities = capabilities;
        self.confirm = None;
        self.reselect_enabled();
    }

    /// Slot 0 (left of Exit) is blank by default; slot 1 defaults to Shot.
    /// Hosts can supply an app-specific action, but they must implement it.
    pub fn set_slot(&mut self, index: usize, slot: Option<Slot>) -> bool {
        if let Some(place) = self.slots.get_mut(index) {
            *place = slot.filter(|s| !s.label.is_empty());
            self.confirm = None;
            self.reselect_enabled();
            true
        } else {
            false
        }
    }

    fn open(&mut self, require_release: bool) -> Result<(), OverlayError> {
        self.control.open()?;
        self.page = Page::Strip;
        self.selected = 3;
        self.confirm = None;
        self.wait_home_release = require_release;
        self.reselect_enabled();
        Ok(())
    }

    pub fn open_programmatically(&mut self) -> Result<(), OverlayError> {
        self.open(false)
    }

    pub fn count(&self) -> usize {
        match self.page {
            Page::Strip => 5,
            Page::Dev | Page::WiiMote => 2,
            Page::Exit => 4,
        }
    }

    fn enabled(&self, index: usize) -> bool {
        match self.page {
            Page::Strip => match index {
                0 | 2 | 4 => true,
                1 => self.slots[0].is_some(),
                3 => self.slots[1].is_some(),
                _ => false,
            },
            Page::Dev => match index {
                0 => self.capabilities.can_restart_app,
                1 => self.capabilities.can_save,
                _ => false,
            },
            Page::Exit => index < EXIT_REQUESTS.len(),
            Page::WiiMote => match index {
                0 => self.capabilities.can_find_remotes,
                1 => self.capabilities.can_disconnect_remotes,
                _ => false,
            },
        }
    }

    pub fn item_enabled(&self, index: usize) -> bool {
        self.enabled(index)
    }

    fn strip_label(&self, index: usize) -> &'static str {
        match index {
            1 => self.slots[0].map_or("", |slot| slot.label),
            3 => self.slots[1].map_or("", |slot| slot.label),
            _ => STRIP_LABELS.get(index).copied().unwrap_or(""),
        }
    }

    pub fn item_label(&self, index: usize) -> &'static str {
        if self.confirm.is_some() && index == self.selected {
            return match self.confirm {
                Some(Request::RestartConsole) => "Confirm Restart Wii",
                Some(Request::PowerOff) => "Confirm Power off",
                Some(Request::RestartApp) => "Confirm Restart app",
                _ => "Confirm",
            };
        }
        match self.page {
            Page::Strip => self.strip_label(index),
            Page::Dev => DEV_LABELS.get(index).copied().unwrap_or(""),
            Page::Exit => EXIT_LABELS.get(index).copied().unwrap_or(""),
            Page::WiiMote => WM_LABELS.get(index).copied().unwrap_or(""),
        }
    }

    fn reselect_enabled(&mut self) {
        if self.enabled(self.selected) {
            return;
        }
        if let Some(index) = (0..self.count()).find(|&i| self.enabled(i)) {
            self.selected = index;
        } else {
            self.selected = 0; // empty menu: nothing can be confirmed
        }
    }

    fn navigate(&mut self, step: isize) {
        self.confirm = None;
        let count = self.count();
        let next = (1..=count)
            .map(|n| {
                (self.selected as isize + step * n as isize).rem_euclid(count as isize) as usize
            })
            .find(|&i| self.enabled(i));
        if let Some(index) = next {
            self.selected = index;
        }
    }

    fn request_for_selection(&self) -> Option<Request> {
        if !self.enabled(self.selected) {
            return None;
        }
        match self.page {
            Page::Strip => match self.selected {
                1 => self.slots[0].map(|s| s.request),
                3 => self.slots[1].map(|s| s.request),
                _ => None,
            },
            Page::Dev => DEV_REQUESTS.get(self.selected).copied(),
            Page::Exit => EXIT_REQUESTS.get(self.selected).copied(),
            Page::WiiMote => WM_REQUESTS.get(self.selected).copied(),
        }
    }

    pub fn tick(
        &mut self,
        pressed: Option<Key>,
        home_held: bool,
    ) -> Result<FrameAction, OverlayError> {
        if self.wait_home_release {
            if !home_held {
                self.wait_home_release = false;
            }
            return Ok(FrameAction::None);
        }
        if !self.control.is_open() {
            if pressed == Some(Key::Home) {
                self.open(true)?;
            }
            return Ok(FrameAction::None);
        }
        match pressed {
            Some(Key::Home) => {
                self.confirm = None;
                self.control.close()?;
            }
            Some(Key::Back) => {
                self.confirm = None;
                if self.page == Page::Strip {
                    self.control.close()?;
                } else {
                    self.page = Page::Strip;
                    self.selected = 2;
                }
            }
            Some(Key::Up | Key::Left) => self.navigate(-1),
            Some(Key::Down | Key::Right) => self.navigate(1),
            Some(Key::Confirm) => {
                if !self.enabled(self.selected) {
                    return Ok(FrameAction::None);
                }
                if self.page == Page::Strip {
                    let page = match self.selected {
                        0 => Some(Page::Dev),
                        2 => Some(Page::Exit),
                        4 => Some(Page::WiiMote),
                        _ => None,
                    };
                    if let Some(page) = page {
                        self.page = page;
                        self.selected = 0;
                        self.confirm = None;
                        self.reselect_enabled();
                        return Ok(FrameAction::None);
                    }
                }
                if let Some(request) = self.request_for_selection() {
                    if matches!(
                        request,
                        Request::RestartApp | Request::RestartConsole | Request::PowerOff
                    ) && self.confirm != Some(request)
                    {
                        self.confirm = Some(request);
                        return Ok(FrameAction::None);
                    }
                    self.confirm = None;
                    return self.control.request(request).map(FrameAction::Perform);
                }
            }
            None => {}
        }
        Ok(FrameAction::None)
    }

    pub fn complete(
        &mut self,
        request: Request,
        result: OperationResult,
    ) -> Result<(), OverlayError> {
        self.control.complete(request, result)
    }

    pub fn tick_input_frame(
        &mut self,
        frame: &crate::platform::input::InputFrame,
    ) -> Result<FrameAction, OverlayError> {
        let held = frame.held.contains(&crate::app::Input::Home);
        let key = frame
            .pressed
            .iter()
            .find_map(|&input| key_from_input(input));
        self.tick(key, held)
    }

    // Both software and GX paths share this exact placement and labels.
    fn strip_geometry(width: usize, height: usize) -> Result<(usize, usize, usize), DrawError> {
        if width < 320 || height < 240 {
            return Err(DrawError::InvalidBuffer);
        }
        Ok((16, height - 76, (width - 32 - 4 * 6) / 5))
    }

    fn popup_geometry(&self, width: usize, height: usize) -> Result<(usize, usize), DrawError> {
        if width < 320 || height < 240 {
            return Err(DrawError::InvalidBuffer);
        }
        let panel_height = 28 + self.count() * 27;
        let strip_top = height - 76;
        Ok((
            (width - 288) / 2,
            strip_top.saturating_sub(panel_height + 8),
        ))
    }

    pub fn draw_labels(&self, width: usize, height: usize, text: &mut impl OverlayText) {
        if !self.is_open() {
            return;
        }
        let Ok((strip_x, strip_y, cell_w)) = Self::strip_geometry(width, height) else {
            return;
        };
        text.draw_text("HOME", strip_x + 8, strip_y + 4, false);
        for i in 0..5 {
            // Source the label from one policy, including pending confirmation.
            let label = if self.page == Page::Strip {
                self.item_label(i)
            } else {
                self.strip_label(i)
            };
            text.draw_text(
                label,
                strip_x + i * (cell_w + 6) + 4,
                strip_y + 39,
                self.page == Page::Strip && i == self.selected,
            );
        }
        if self.page != Page::Strip
            && let Ok((x, y)) = self.popup_geometry(width, height)
        {
            let title = match self.page {
                Page::Dev => "DEV",
                Page::Exit => "Exit",
                Page::WiiMote => "WiiMote",
                Page::Strip => "",
            };
            text.draw_text(title, x + 12, y + 5, false);
            for index in 0..self.count() {
                text.draw_text(
                    self.item_label(index),
                    x + 16,
                    y + 31 + index * 27,
                    index == self.selected && self.enabled(index),
                );
            }
        }
    }

    pub fn draw(
        &self,
        pixels: &mut [u8],
        width: usize,
        height: usize,
        stride: usize,
    ) -> Result<(), DrawError> {
        if !self.is_open() {
            return Ok(());
        }
        let row_bytes = width.checked_mul(4).ok_or(DrawError::InvalidBuffer)?;
        let total = stride.checked_mul(height).ok_or(DrawError::InvalidBuffer)?;
        if stride < row_bytes || pixels.len() < total {
            return Err(DrawError::InvalidBuffer);
        }
        self.visit_rects(width, height, |rect| {
            rectangle(
                pixels,
                stride,
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                rect.color,
            );
        })
    }

    pub fn visit_rects(
        &self,
        width: usize,
        height: usize,
        mut visit: impl FnMut(RectSpec),
    ) -> Result<(), DrawError> {
        if !self.is_open() {
            return Ok(());
        }
        let (x, y, cw) = Self::strip_geometry(width, height)?;
        visit(RectSpec {
            x,
            y,
            width: width - 32,
            height: 70,
            color: [16, 22, 35, 255],
        });
        for i in 0..5 {
            let active = self.page == Page::Strip && self.selected == i;
            let empty_slot = match i {
                1 => self.slots[0].is_none(),
                3 => self.slots[1].is_none(),
                _ => false,
            };
            let color = if empty_slot {
                [29, 35, 44, 255]
            } else if active {
                [92, 153, 218, 255]
            } else {
                [43, 56, 75, 255]
            };
            visit(RectSpec {
                x: x + i * (cw + 6),
                y: y + 28,
                width: cw,
                height: 34,
                color,
            });
        }
        if self.page != Page::Strip {
            let (px, py) = self.popup_geometry(width, height)?;
            let ph = 28 + self.count() * 27;
            visit(RectSpec {
                x: px,
                y: py,
                width: 288,
                height: ph,
                color: [16, 22, 35, 255],
            });
            for i in 0..self.count() {
                let color = if !self.enabled(i) {
                    [29, 35, 44, 255]
                } else if self.selected == i {
                    [92, 153, 218, 255]
                } else {
                    [43, 56, 75, 255]
                };
                visit(RectSpec {
                    x: px + 8,
                    y: py + 27 + i * 27,
                    width: 272,
                    height: 24,
                    color,
                });
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DrawError {
    InvalidBuffer,
}

fn rectangle(
    pixels: &mut [u8],
    stride: usize,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    color: [u8; 4],
) {
    for row in y..y + h {
        for col in x..x + w {
            let offset = row * stride + col * 4;
            pixels[offset..offset + 4].copy_from_slice(&color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn open(ui: &mut AppHomeOverlay) {
        ui.tick(Some(Key::Home), true).unwrap();
        ui.tick(None, false).unwrap();
    }
    #[test]
    fn strip_has_reborn_slot_order_and_shot_by_default() {
        let mut ui = AppHomeOverlay::default();
        open(&mut ui);
        assert_eq!(ui.page(), Page::Strip);
        assert_eq!(ui.item_label(0), "DEV");
        assert_eq!(ui.item_label(1), "");
        assert_eq!(ui.item_label(2), "Exit");
        assert_eq!(ui.item_label(3), "Shot");
        assert_eq!(ui.item_label(4), "WiiMote");
        assert_eq!(
            ui.tick(Some(Key::Confirm), false),
            Ok(FrameAction::Perform(Request::Screenshot))
        );
    }
    #[test]
    fn page_navigation_and_confirmation_keep_actions_safe() {
        let mut ui = AppHomeOverlay::default();
        open(&mut ui);
        ui.tick(Some(Key::Left), false).unwrap(); // Exit
        ui.tick(Some(Key::Confirm), false).unwrap();
        assert_eq!(ui.page(), Page::Exit);
        assert_eq!(ui.item_label(0), "The Homebrew Channel");
        ui.tick(Some(Key::Down), false).unwrap();
        ui.tick(Some(Key::Down), false).unwrap();
        assert_eq!(ui.item_label(ui.selected()), "Restart Wii");
        assert_eq!(ui.tick(Some(Key::Confirm), false), Ok(FrameAction::None));
        assert_eq!(ui.pending_confirmation(), Some(Request::RestartConsole));
        assert_eq!(ui.item_label(ui.selected()), "Confirm Restart Wii");
        ui.tick(Some(Key::Back), false).unwrap();
        assert_eq!(ui.pending_confirmation(), None);
        assert_eq!(ui.page(), Page::Strip);
        assert!(ui.is_open());
    }
    #[test]
    fn unavailable_dev_and_remote_items_do_nothing() {
        let mut ui = AppHomeOverlay::default();
        open(&mut ui);
        ui.tick(Some(Key::Left), false).unwrap();
        ui.tick(Some(Key::Left), false).unwrap(); // DEV
        ui.tick(Some(Key::Confirm), false).unwrap();
        assert_eq!(ui.page(), Page::Dev);
        assert!(!ui.item_enabled(0));
        assert!(!ui.item_enabled(1));
        assert_eq!(ui.tick(Some(Key::Confirm), false), Ok(FrameAction::None));
        ui.set_capabilities(Capabilities {
            can_save: true,
            ..Capabilities::default()
        });
        assert_eq!(ui.selected(), 1);
        assert_eq!(
            ui.tick(Some(Key::Confirm), false),
            Ok(FrameAction::Perform(Request::Save))
        );
    }
    #[test]
    fn host_custom_slots_do_not_override_default_shot_without_opt_in() {
        let mut ui = AppHomeOverlay::default();
        assert!(!ui.set_slot(2, None));
        assert!(ui.set_slot(
            0,
            Some(Slot {
                label: "Quick Save",
                request: Request::Save
            })
        ));
        open(&mut ui);
        assert_eq!(ui.item_label(1), "Quick Save");
        assert_eq!(ui.item_label(3), "Shot");
    }
    #[test]
    fn wiimote_controls_are_explicitly_opt_in() {
        let mut ui = AppHomeOverlay::default();
        open(&mut ui);
        ui.tick(Some(Key::Right), false).unwrap(); // WiiMote
        ui.tick(Some(Key::Confirm), false).unwrap();
        assert_eq!(ui.page(), Page::WiiMote);
        assert_eq!(ui.item_label(0), "Find remotes");
        assert!(!ui.item_enabled(0));
        assert_eq!(ui.tick(Some(Key::Confirm), false), Ok(FrameAction::None));
        ui.set_capabilities(Capabilities {
            can_find_remotes: true,
            ..Capabilities::default()
        });
        assert_eq!(
            ui.tick(Some(Key::Confirm), false),
            Ok(FrameAction::Perform(Request::FindRemotes))
        );
        ui.complete(Request::FindRemotes, OperationResult::Failed)
            .unwrap();
        assert!(ui.is_open());
    }
    #[test]
    fn save_is_one_action_and_restart_app_requires_confirmation() {
        let mut ui = AppHomeOverlay::default();
        ui.set_capabilities(Capabilities {
            can_save: true,
            can_restart_app: true,
            ..Capabilities::default()
        });
        open(&mut ui);
        ui.tick(Some(Key::Left), false).unwrap(); // Exit
        ui.tick(Some(Key::Left), false).unwrap(); // DEV
        ui.tick(Some(Key::Confirm), false).unwrap();
        assert_eq!(ui.page(), Page::Dev);
        assert_eq!(ui.tick(Some(Key::Confirm), false), Ok(FrameAction::None));
        assert_eq!(
            ui.tick(Some(Key::Confirm), false),
            Ok(FrameAction::Perform(Request::RestartApp))
        );
        ui.complete(Request::RestartApp, OperationResult::Failed)
            .unwrap();
        assert!(ui.is_open());
        ui.tick(Some(Key::Down), false).unwrap();
        assert_eq!(
            ui.tick(Some(Key::Confirm), false),
            Ok(FrameAction::Perform(Request::Save))
        );
    }
    #[test]
    fn no_overflow_on_invalid_rgba_buffer() {
        let mut ui = AppHomeOverlay::default();
        open(&mut ui);
        let mut pixels = [12u8; 4];
        assert_eq!(
            ui.draw(&mut pixels, 320, 240, 1280),
            Err(DrawError::InvalidBuffer)
        );
        assert_eq!(pixels, [12; 4]);
    }
    #[test]
    fn home_release_and_no_repetition() {
        let mut ui = AppHomeOverlay::default();
        ui.tick(Some(Key::Home), true).unwrap();
        assert!(ui.is_open());
        ui.tick(Some(Key::Home), true).unwrap();
        assert!(ui.is_open());
        ui.tick(None, false).unwrap();
        ui.tick(Some(Key::Home), false).unwrap();
        assert!(!ui.is_open());
    }
}
