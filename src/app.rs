//! Hardware-independent application navigation state.

use alloc::{string::String, vec::Vec};

use crate::catalog::{visible_entries, AppEntry, Filter, SortOrder};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Screen {
    #[default]
    Browser,
    MainMenu,
    AppDetails,
    DeleteConfirm,
    WiiloadConfirm,
    WiiloadInstalling,
    Options,
    About,
    Notice,
}

/// Input after the platform layer maps controller buttons to logical actions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Input {
    Home,
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
    ToggleBrowseMode,
    OpenOptions,
    PreviousPage,
    NextPage,
}

/// An interactive item under the Wii Remote pointer. Targets are semantic;
/// the renderer supplies their bounds from the same layout it draws.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerTarget {
    App(usize),
    Page { forward: bool },
    Menu(usize),
    Detail(usize),
    DetailMemo,
    DeleteChoice { yes: bool },
    WiiloadChoice { yes: bool },
    Option(usize),
    NoticeConfirm,
}

/// Work for a hardware or service layer to perform after a state transition.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Action {
    #[default]
    None,
    LaunchSelected,
    ApplyTheme,
    DeleteSelected,
    LaunchBootMii,
    Exit,
    Shutdown,
    PreviousPage,
    NextPage,
    ApplyOptions,
    AcceptWiiloadZip,
    RejectWiiloadZip,
}

/// Main menu focus follows the options available on this console.
const MENU_ITEMS_WITHOUT_BOOTMII: usize = 4;
const MENU_ITEMS_WITH_BOOTMII: usize = 5;
const DEFAULT_BROWSER_ROWS: usize = 5;
pub const DETAIL_DESCRIPTION_LINE_WIDTH: usize = 76;
pub const DETAIL_DESCRIPTION_VISIBLE_LINES: usize = 20;

/// Number of app columns and rows shown by the browser.
///
/// HBC uses five rows in both list and grid modes. A one-column layout is a
/// list; wider layouts use directional grid movement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrowserLayout {
    pub columns: usize,
    pub rows: usize,
}

impl Default for BrowserLayout {
    fn default() -> Self {
        Self {
            columns: 1,
            rows: DEFAULT_BROWSER_ROWS,
        }
    }
}

impl BrowserLayout {
    fn page_capacity(self) -> usize {
        self.columns.max(1).saturating_mul(self.rows.max(1))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageNavigation {
    pub start: usize,
    pub capacity: usize,
    pub visible_count: usize,
}

impl PageNavigation {
    pub fn has_previous(self) -> bool {
        self.start > 0
    }

    pub fn has_next(self) -> bool {
        self.start.saturating_add(self.capacity) < self.visible_count
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AppController {
    pub screen: Screen,
    pub selected_app: Option<String>,
    pub filter: Filter,
    pub sort: SortOrder,
    selected_index: usize,
    layout: BrowserLayout,
    grid_columns: usize,
    menu_focus: usize,
    detail_focus: usize,
    detail_description_scroll: usize,
    detail_description_max_scroll: usize,
    detail_scroll_app: Option<String>,
    detail_scroll_pixel_remainder: i16,
    detail_drag_pixel_remainder: i16,
    detail_drag_last_y: Option<i16>,
    detail_dragging: bool,
    delete_confirmation_yes: bool,
    wiiload_confirmation_yes: bool,
    notice: Option<String>,
    notice_return_screen: Screen,
    bootmii_available: bool,
    preferred_device: i32,
    active_device: Option<i32>,
    pending_device: i32,
    pending_sort: SortOrder,
    options_focus: usize,
    pointer_target: Option<PointerTarget>,
}

impl AppController {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the catalog in the active filter and sort order.
    pub fn visible_apps<'a>(&self, catalog: &'a [AppEntry]) -> Vec<&'a AppEntry> {
        visible_entries(catalog, self.filter, self.sort)
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn menu_focus(&self) -> usize {
        self.menu_focus
    }

    pub fn detail_focus(&self) -> usize {
        self.detail_focus
    }

    pub fn request_wiiload_zip_confirmation(&mut self) {
        self.wiiload_confirmation_yes = false;
        self.screen = Screen::WiiloadConfirm;
    }

    pub fn wiiload_confirmation_yes(&self) -> bool {
        self.wiiload_confirmation_yes
    }

    pub fn show_wiiload_installing(&mut self) {
        self.screen = Screen::WiiloadInstalling;
    }

    pub fn finish_wiiload_installing(&mut self) {
        if self.screen == Screen::WiiloadInstalling {
            self.screen = Screen::Browser;
        }
    }

    pub fn detail_description_scroll(&self) -> usize {
        self.detail_description_scroll
    }

    pub fn scroll_detail_by_stick(&mut self, pixels: i16) {
        self.scroll_detail_by_pixels(pixels);
    }

    fn scroll_detail_by_pixels(&mut self, pixels: i16) {
        if self.screen != Screen::AppDetails {
            return;
        }
        self.detail_scroll_pixel_remainder =
            self.detail_scroll_pixel_remainder.saturating_add(pixels);
        let lines = self.detail_scroll_pixel_remainder / 9;
        self.detail_scroll_pixel_remainder %= 9;
        if lines > 0 {
            self.detail_description_scroll = self
                .detail_description_scroll
                .saturating_sub(lines as usize)
                .min(self.detail_description_max_scroll);
        } else if lines < 0 {
            self.detail_description_scroll = self
                .detail_description_scroll
                .saturating_add(lines.unsigned_abs() as usize)
                .min(self.detail_description_max_scroll);
        }
    }

    pub fn delete_confirmation_yes(&self) -> bool {
        self.delete_confirmation_yes
    }

    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// Show a message and return to the current screen after dismissal.
    pub fn show_notice(&mut self, message: String) {
        self.notice_return_screen = self.screen;
        self.notice = Some(message);
        self.screen = Screen::Notice;
    }

    pub fn set_bootmii_available(&mut self, available: bool) {
        self.bootmii_available = available;
        let last_focus = self.menu_item_count().saturating_sub(1);
        self.menu_focus = self.menu_focus.min(last_focus);
    }

    pub fn bootmii_available(&self) -> bool {
        self.bootmii_available
    }

    pub fn preferred_device(&self) -> i32 {
        self.preferred_device
    }

    pub fn options_focus(&self) -> usize {
        self.options_focus
    }

    pub fn pointer_target(&self) -> Option<PointerTarget> {
        self.pointer_target
    }

    pub fn pending_device(&self) -> i32 {
        self.pending_device
    }

    pub fn pending_sort(&self) -> SortOrder {
        self.pending_sort
    }

    pub fn set_preferred_device(&mut self, device: i32) {
        self.preferred_device = device;
    }

    pub fn set_active_device(&mut self, device: Option<i32>) {
        self.active_device = device;
    }

    pub fn menu_item_count(&self) -> usize {
        if self.bootmii_available {
            MENU_ITEMS_WITH_BOOTMII
        } else {
            MENU_ITEMS_WITHOUT_BOOTMII
        }
    }

    /// Keep the selected app identity valid after a catalog refresh.
    pub fn reconcile_catalog(&mut self, catalog: &[AppEntry]) {
        self.reconcile_selection(catalog);
        self.refresh_detail_scroll_limit(catalog);
    }

    pub fn browser_layout(&self) -> BrowserLayout {
        self.layout
    }

    /// Set list or grid dimensions without losing the selected app identity.
    pub fn set_browser_layout(&mut self, layout: BrowserLayout, catalog: &[AppEntry]) {
        self.layout = BrowserLayout {
            columns: layout.columns.max(1),
            rows: layout.rows.max(1),
        };
        self.reconcile_selection(catalog);
    }

    /// Set the grid width used when the user toggles from list mode.
    pub fn set_grid_columns(&mut self, columns: usize, catalog: &[AppEntry]) {
        self.grid_columns = columns.max(3);
        if self.layout.columns > 1 {
            self.set_browser_layout(
                BrowserLayout {
                    columns: self.grid_columns,
                    rows: self.layout.rows,
                },
                catalog,
            );
        }
    }

    /// Index of the first entry on the selected app's page.
    pub fn page_start(&self) -> usize {
        self.selected_index / self.layout.page_capacity() * self.layout.page_capacity()
    }

    pub fn page_navigation(&self, catalog: &[AppEntry]) -> PageNavigation {
        self.page_navigation_for_count(self.visible_apps(catalog).len())
    }

    fn page_navigation_for_count(&self, visible_count: usize) -> PageNavigation {
        PageNavigation {
            start: self.page_start(),
            capacity: self.layout.page_capacity(),
            visible_count,
        }
    }

    /// Return the current page in the active filter and sort order.
    pub fn visible_page<'a>(&self, catalog: &'a [AppEntry]) -> Vec<&'a AppEntry> {
        let visible = self.visible_apps(catalog);
        let start = self.page_start().min(visible.len());
        let end = start
            .saturating_add(self.layout.page_capacity())
            .min(visible.len());
        visible[start..end].to_vec()
    }

    /// Apply one logical input and keep the selected identity valid for the
    /// current visible catalog. Selection does not wrap at either end.
    pub fn handle(&mut self, input: Input, catalog: &[AppEntry]) -> Action {
        self.reconcile_selection(catalog);
        self.refresh_detail_scroll_limit(catalog);

        match self.screen {
            Screen::Browser => self.handle_browser(input, catalog),
            Screen::MainMenu => self.handle_menu(input),
            Screen::AppDetails => match input {
                Input::Back => {
                    self.screen = Screen::Browser;
                    Action::None
                }
                Input::Left => {
                    self.move_detail_focus(false);
                    Action::None
                }
                Input::Right => {
                    self.move_detail_focus(true);
                    Action::None
                }
                Input::Up => {
                    self.detail_description_scroll =
                        self.detail_description_scroll.saturating_sub(1);
                    Action::None
                }
                Input::Down => {
                    self.detail_description_scroll = self
                        .detail_description_scroll
                        .saturating_add(1)
                        .min(self.detail_description_max_scroll);
                    Action::None
                }
                Input::Confirm if self.selected_app.is_some() => match self.detail_focus {
                    8 => {
                        self.delete_confirmation_yes = false;
                        self.screen = Screen::DeleteConfirm;
                        Action::None
                    }
                    9 => {
                        let is_theme = self
                            .selected_app
                            .as_deref()
                            .and_then(|selected| {
                                catalog.iter().find(|entry| entry.directory == selected)
                            })
                            .is_some_and(|entry| entry.kind == crate::catalog::AppKind::Theme);
                        if is_theme {
                            self.screen = Screen::Browser;
                            Action::ApplyTheme
                        } else {
                            Action::LaunchSelected
                        }
                    }
                    10 => {
                        self.screen = Screen::Browser;
                        Action::None
                    }
                    _ => Action::None,
                },
                _ => Action::None,
            },
            Screen::DeleteConfirm => match input {
                Input::Left | Input::Right => {
                    self.delete_confirmation_yes = !self.delete_confirmation_yes;
                    Action::None
                }
                Input::Back => {
                    self.screen = Screen::AppDetails;
                    Action::None
                }
                Input::Confirm if self.delete_confirmation_yes => {
                    self.screen = Screen::Browser;
                    Action::DeleteSelected
                }
                Input::Confirm => {
                    self.screen = Screen::AppDetails;
                    Action::None
                }
                _ => Action::None,
            },
            Screen::WiiloadConfirm => match input {
                Input::Left | Input::Right => {
                    self.wiiload_confirmation_yes = !self.wiiload_confirmation_yes;
                    Action::None
                }
                Input::Back => {
                    self.screen = Screen::Browser;
                    Action::RejectWiiloadZip
                }
                Input::Home => {
                    self.screen = Screen::MainMenu;
                    self.menu_focus = 0;
                    Action::RejectWiiloadZip
                }
                Input::Confirm if self.wiiload_confirmation_yes => {
                    self.screen = Screen::Browser;
                    Action::AcceptWiiloadZip
                }
                Input::Confirm => {
                    self.screen = Screen::Browser;
                    Action::RejectWiiloadZip
                }
                _ => Action::None,
            },
            Screen::WiiloadInstalling => Action::None,
            Screen::Options => self.handle_options(input, catalog),
            Screen::About => match input {
                Input::Back | Input::Confirm => {
                    self.screen = Screen::MainMenu;
                    Action::None
                }
                _ => Action::None,
            },
            Screen::Notice => match input {
                Input::Back | Input::Confirm | Input::Home => {
                    self.notice = None;
                    self.screen = self.notice_return_screen;
                    Action::None
                }
                _ => Action::None,
            },
        }
    }

    /// Move focus to an item selected by the renderer's hit map. A click then
    /// reuses the same Confirm and page actions as physical buttons.
    pub fn handle_pointer(
        &mut self,
        target: Option<PointerTarget>,
        activate: bool,
        catalog: &[AppEntry],
    ) -> Action {
        self.handle_pointer_frame(target, None, activate, false, catalog)
    }

    pub fn handle_pointer_frame(
        &mut self,
        target: Option<PointerTarget>,
        pointer_y: Option<i16>,
        pressed: bool,
        held: bool,
        catalog: &[AppEntry],
    ) -> Action {
        self.pointer_target = target;
        self.update_detail_drag(target, pointer_y, pressed, held);
        let Some(target) = target else {
            return Action::None;
        };
        self.reconcile_selection(catalog);
        match (self.screen, target) {
            (Screen::Browser, PointerTarget::App(index)) => {
                let visible = self.visible_apps(catalog);
                if let Some(entry) = visible.get(index) {
                    self.selected_app = Some(entry.directory.clone());
                    self.reconcile_selection(catalog);
                    if pressed {
                        return self.handle(Input::Confirm, catalog);
                    }
                }
                Action::None
            }
            (Screen::Browser, PointerTarget::Page { forward }) => {
                if pressed {
                    self.move_page(catalog, forward)
                } else {
                    Action::None
                }
            }
            (Screen::MainMenu, PointerTarget::Menu(index)) if index < self.menu_item_count() => {
                self.menu_focus = index;
                if pressed {
                    self.handle(Input::Confirm, catalog)
                } else {
                    Action::None
                }
            }
            (Screen::AppDetails, PointerTarget::Detail(index)) if (8..=10).contains(&index) => {
                self.detail_focus = index;
                if pressed {
                    self.handle(Input::Confirm, catalog)
                } else {
                    Action::None
                }
            }
            (Screen::AppDetails, PointerTarget::DetailMemo) => Action::None,
            (Screen::DeleteConfirm, PointerTarget::DeleteChoice { yes }) => {
                self.delete_confirmation_yes = yes;
                if pressed {
                    self.handle(Input::Confirm, catalog)
                } else {
                    Action::None
                }
            }
            (Screen::WiiloadConfirm, PointerTarget::WiiloadChoice { yes }) => {
                self.wiiload_confirmation_yes = yes;
                if pressed {
                    self.handle(Input::Confirm, catalog)
                } else {
                    Action::None
                }
            }
            (Screen::WiiloadInstalling, _) => Action::None,
            (Screen::Options, PointerTarget::Option(index)) if index < 6 => {
                self.options_focus = index;
                if pressed {
                    self.handle(Input::Confirm, catalog)
                } else {
                    Action::None
                }
            }
            (Screen::Notice, PointerTarget::NoticeConfirm) if pressed => {
                self.handle(Input::Confirm, catalog)
            }
            _ => Action::None,
        }
    }

    fn update_detail_drag(
        &mut self,
        target: Option<PointerTarget>,
        pointer_y: Option<i16>,
        pressed: bool,
        held: bool,
    ) {
        if self.screen != Screen::AppDetails {
            self.detail_dragging = false;
            self.detail_drag_last_y = None;
            self.detail_drag_pixel_remainder = 0;
            return;
        }
        if !held && !pressed {
            self.detail_dragging = false;
            self.detail_drag_last_y = None;
            self.detail_drag_pixel_remainder = 0;
            return;
        }
        let Some(pointer_y) = pointer_y else {
            self.detail_dragging = false;
            self.detail_drag_last_y = None;
            self.detail_drag_pixel_remainder = 0;
            return;
        };
        if pressed && target == Some(PointerTarget::DetailMemo) {
            self.detail_dragging = true;
            self.detail_drag_last_y = Some(pointer_y);
            self.detail_drag_pixel_remainder = 0;
            return;
        }
        if !held {
            return;
        }
        if !self.detail_dragging {
            return;
        }
        if let Some(previous_y) = self.detail_drag_last_y.replace(pointer_y) {
            let delta_y = i32::from(pointer_y) - i32::from(previous_y);
            let scaled = i32::from(self.detail_drag_pixel_remainder) - delta_y;
            let scroll_pixels = scaled / 32;
            self.detail_drag_pixel_remainder = (scaled % 32) as i16;
            self.scroll_detail_by_pixels(scroll_pixels.clamp(-32_768, 32_767) as i16);
        }
    }

    fn handle_browser(&mut self, input: Input, catalog: &[AppEntry]) -> Action {
        match input {
            Input::Home => {
                self.screen = Screen::MainMenu;
                self.menu_focus = 0;
                Action::None
            }
            Input::Up => {
                let step = if self.layout.columns == 1 {
                    1
                } else {
                    self.layout.columns
                };
                self.move_selection(catalog, false, step);
                Action::None
            }
            Input::Down => {
                let step = if self.layout.columns == 1 {
                    1
                } else {
                    self.layout.columns
                };
                self.move_selection(catalog, true, step);
                Action::None
            }
            Input::Left if self.layout.columns > 1 => {
                self.move_selection(catalog, false, 1);
                Action::None
            }
            Input::Right if self.layout.columns > 1 => {
                self.move_selection(catalog, true, 1);
                Action::None
            }
            Input::Confirm if self.selected_app.is_some() => {
                self.screen = Screen::AppDetails;
                self.detail_focus = 10;
                self.detail_description_scroll = 0;
                self.detail_description_max_scroll = 0;
                self.detail_scroll_app = None;
                self.detail_scroll_pixel_remainder = 0;
                self.refresh_detail_scroll_limit(catalog);
                Action::None
            }
            Input::ToggleBrowseMode => {
                self.layout.columns = if self.layout.columns == 1 {
                    self.grid_columns.max(3)
                } else {
                    1
                };
                self.reconcile_selection(catalog);
                Action::None
            }
            Input::OpenOptions => {
                self.pending_device = match self.active_device {
                    Some(device @ (0 | 1)) => device,
                    _ => i32::from(self.preferred_device == 1),
                };
                self.pending_sort = self.sort;
                self.options_focus = 5;
                self.screen = Screen::Options;
                Action::None
            }
            Input::PreviousPage => self.move_page(catalog, false),
            Input::NextPage => self.move_page(catalog, true),
            _ => Action::None,
        }
    }

    fn handle_menu(&mut self, input: Input) -> Action {
        match input {
            Input::Home | Input::Back => {
                self.screen = Screen::Browser;
                Action::None
            }
            Input::Up | Input::Left => {
                self.move_menu_focus(false);
                Action::None
            }
            Input::Down | Input::Right => {
                self.move_menu_focus(true);
                Action::None
            }
            Input::Confirm => match self.menu_focus {
                0 => {
                    self.screen = Screen::Browser;
                    Action::None
                }
                1 => {
                    self.screen = Screen::About;
                    Action::None
                }
                2 if self.bootmii_available => Action::LaunchBootMii,
                2 => Action::Exit,
                3 if self.bootmii_available => Action::Exit,
                3 => Action::Shutdown,
                4 => Action::Shutdown,
                _ => Action::None,
            },
            _ => Action::None,
        }
    }

    fn handle_options(&mut self, input: Input, catalog: &[AppEntry]) -> Action {
        match input {
            Input::Back | Input::OpenOptions => {
                self.screen = Screen::Browser;
                Action::None
            }
            Input::Left => {
                self.options_focus &= !1;
                Action::None
            }
            Input::Right => {
                self.options_focus |= 1;
                Action::None
            }
            Input::Up => {
                self.options_focus = self.options_focus.saturating_sub(2);
                Action::None
            }
            Input::Down => {
                self.options_focus = (self.options_focus + 2).min(5);
                Action::None
            }
            Input::Confirm => match self.options_focus {
                0 => {
                    self.pending_device = 0;
                    Action::None
                }
                1 => {
                    self.pending_device = 1;
                    Action::None
                }
                2 => {
                    self.pending_sort = SortOrder::Name;
                    Action::None
                }
                3 => {
                    self.pending_sort = SortOrder::ReleaseDate;
                    Action::None
                }
                4 => {
                    self.preferred_device = self.pending_device;
                    self.sort = self.pending_sort;
                    self.filter = match self.pending_sort {
                        SortOrder::Name => Filter::All,
                        SortOrder::ReleaseDate => Filter::MetadataOnly,
                    };
                    self.reconcile_selection(catalog);
                    self.screen = Screen::Browser;
                    Action::ApplyOptions
                }
                5 => {
                    self.screen = Screen::Browser;
                    Action::None
                }
                _ => Action::None,
            },
            _ => Action::None,
        }
    }

    fn move_selection(&mut self, catalog: &[AppEntry], forward: bool, step: usize) {
        let visible = self.visible_apps(catalog);
        if visible.is_empty() {
            self.selected_index = 0;
            self.selected_app = None;
            return;
        }
        self.selected_index = if forward {
            self.selected_index
                .saturating_add(step)
                .min(visible.len() - 1)
        } else {
            self.selected_index.saturating_sub(step)
        };
        self.selected_app = Some(visible[self.selected_index].directory.clone());
    }

    fn move_page(&mut self, catalog: &[AppEntry], forward: bool) -> Action {
        let visible = self.visible_apps(catalog);
        if visible.is_empty() {
            return Action::None;
        }
        let navigation = self.page_navigation_for_count(visible.len());
        if (forward && !navigation.has_next()) || (!forward && !navigation.has_previous()) {
            return Action::None;
        }
        let capacity = navigation.capacity;
        let current_start = navigation.start;
        let next_start = if forward {
            current_start.saturating_add(capacity)
        } else {
            current_start.saturating_sub(capacity)
        };
        if next_start >= visible.len() {
            return Action::None;
        }
        self.selected_index = next_start;
        self.selected_app = Some(visible[next_start].directory.clone());
        if forward {
            Action::NextPage
        } else {
            Action::PreviousPage
        }
    }

    fn move_menu_focus(&mut self, forward: bool) {
        self.menu_focus = if forward {
            (self.menu_focus + 1).min(self.menu_item_count() - 1)
        } else {
            self.menu_focus.saturating_sub(1)
        };
    }

    fn move_detail_focus(&mut self, forward: bool) {
        self.detail_focus = match (self.detail_focus, forward) {
            (8, false) => 10,
            (9, false) => 8,
            (10, false) => 9,
            (8, true) => 9,
            (9, true) => 10,
            (10, true) => 8,
            _ => 10,
        };
    }

    fn refresh_detail_scroll_limit(&mut self, catalog: &[AppEntry]) {
        if self.screen != Screen::AppDetails
            || self.detail_scroll_app.as_deref() == self.selected_app.as_deref()
        {
            return;
        }
        let description = self
            .selected_app
            .as_deref()
            .and_then(|selected| catalog.iter().find(|app| app.directory == selected))
            .and_then(|app| app.metadata.as_ref())
            .and_then(|metadata| metadata.long_description.as_deref())
            .unwrap_or("No description available.");
        self.detail_description_max_scroll =
            crate::text_layout::wrap_lines(description, DETAIL_DESCRIPTION_LINE_WIDTH)
                .len()
                .saturating_sub(DETAIL_DESCRIPTION_VISIBLE_LINES);
        self.detail_description_scroll = self
            .detail_description_scroll
            .min(self.detail_description_max_scroll);
        self.detail_scroll_app = self.selected_app.clone();
    }

    fn reconcile_selection(&mut self, catalog: &[AppEntry]) {
        let visible = self.visible_apps(catalog);
        if visible.is_empty() {
            self.selected_index = 0;
            self.selected_app = None;
            return;
        }
        if let Some(index) = self
            .selected_app
            .as_deref()
            .and_then(|identity| visible.iter().position(|entry| entry.directory == identity))
        {
            self.selected_index = index;
        } else {
            // HBC focuses the first entry when the selected app is not in the
            // current filtered view or has disappeared from the catalog.
            self.selected_index = 0;
            self.selected_app = Some(visible[self.selected_index].directory.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::AppKind;
    use crate::metadata::AppMetadata;

    fn app(directory: &str, name: &str, icon: bool) -> AppEntry {
        AppEntry {
            directory: directory.into(),
            kind: AppKind::Elf,
            size: 1,
            has_icon: icon,
            metadata: Some(AppMetadata {
                name: Some(name.into()),
                ..AppMetadata::default()
            }),
        }
    }

    #[test]
    fn browser_navigation_and_detail_back_keep_identity() {
        let catalog = [app("one", "One", true), app("two", "Two", true)];
        let mut controller = AppController::new();
        assert_eq!(controller.selected_app.as_deref(), None);
        controller.handle(Input::Down, &catalog);
        assert_eq!(controller.selected_app.as_deref(), Some("two"));
        controller.handle(Input::Confirm, &catalog);
        assert_eq!(controller.screen, Screen::AppDetails);
        assert_eq!(controller.detail_focus(), 10);
        assert_eq!(controller.handle(Input::Confirm, &catalog), Action::None);
        assert_eq!(controller.screen, Screen::Browser);
        controller.handle(Input::Confirm, &catalog);
        assert_eq!(controller.handle(Input::Left, &catalog), Action::None);
        assert_eq!(controller.detail_focus(), 9);
        assert_eq!(
            controller.handle(Input::Confirm, &catalog),
            Action::LaunchSelected
        );
        controller.handle(Input::Left, &catalog);
        assert_eq!(controller.detail_focus(), 8);
        assert_eq!(controller.handle(Input::Confirm, &catalog), Action::None);
        assert_eq!(controller.screen, Screen::DeleteConfirm);
        controller.handle(Input::Back, &catalog);
        assert_eq!(controller.screen, Screen::AppDetails);
        assert_eq!(controller.detail_focus(), 8);
        assert_eq!(controller.selected_app.as_deref(), Some("two"));
    }

    #[test]
    fn detail_up_and_down_scroll_the_description_without_moving_focus() {
        let mut controller = AppController::new();
        let mut entry = app("a", "A", false);
        entry.metadata.as_mut().unwrap().long_description = Some("A ".repeat(1_000));
        let catalog = vec![entry];
        controller.handle(Input::Confirm, &catalog);
        assert_eq!(controller.detail_description_scroll(), 0);

        controller.handle(Input::Down, &catalog);
        controller.handle(Input::Down, &catalog);
        assert_eq!(controller.detail_description_scroll(), 2);
        assert_eq!(controller.detail_focus(), 10);

        controller.handle(Input::Up, &catalog);
        controller.handle(Input::Up, &catalog);
        controller.handle(Input::Up, &catalog);
        assert_eq!(controller.detail_description_scroll(), 0);
        assert_eq!(controller.detail_focus(), 10);

        for _ in 0..100 {
            controller.handle(Input::Down, &catalog);
        }
        assert!(controller.detail_description_scroll() > 0);
        let max_scroll = controller.detail_description_max_scroll;
        assert_eq!(controller.detail_description_scroll(), max_scroll);
        controller.handle(Input::Up, &catalog);
        assert_eq!(controller.detail_description_scroll(), max_scroll - 1);
    }

    #[test]
    fn detail_stick_scroll_accumulates_pixels_into_lines() {
        let mut controller = AppController::new();
        let mut entry = app("a", "A", false);
        entry.metadata.as_mut().unwrap().long_description = Some("A ".repeat(1_000));
        let catalog = vec![entry];
        controller.handle(Input::Confirm, &catalog);
        controller.handle(Input::Down, &catalog);
        controller.scroll_detail_by_stick(8);
        assert_eq!(controller.detail_description_scroll(), 1);
        controller.scroll_detail_by_stick(1);
        assert_eq!(controller.detail_description_scroll(), 0);
        controller.scroll_detail_by_stick(-18);
        assert_eq!(controller.detail_description_scroll(), 2);
    }

    #[test]
    fn dragging_detail_memo_scrolls_by_pointer_delta_and_stops_on_release() {
        let mut controller = AppController::new();
        let mut entry = app("a", "A", false);
        entry.metadata.as_mut().unwrap().long_description = Some("A ".repeat(1_000));
        let catalog = vec![entry];
        controller.handle(Input::Confirm, &catalog);
        controller.handle(Input::Down, &catalog);
        controller.handle(Input::Down, &catalog);

        controller.handle_pointer_frame(
            Some(PointerTarget::DetailMemo),
            Some(200),
            true,
            false,
            &catalog,
        );
        controller.handle_pointer_frame(None, Some(-88), false, true, &catalog);
        assert_eq!(controller.detail_description_scroll(), 1);

        controller.handle_pointer_frame(
            Some(PointerTarget::DetailMemo),
            Some(200),
            true,
            false,
            &catalog,
        );
        controller.handle_pointer_frame(None, None, false, true, &catalog);
        controller.handle_pointer_frame(None, Some(-88), false, true, &catalog);
        assert_eq!(controller.detail_description_scroll(), 1);

        controller.handle_pointer_frame(None, None, false, false, &catalog);
        assert_eq!(controller.detail_description_scroll(), 1);
    }

    #[test]
    fn pointer_selects_and_activates_the_same_items_as_confirm() {
        let catalog = [app("one", "One", false), app("two", "Two", false)];
        let mut controller = AppController::new();
        assert_eq!(
            controller.handle_pointer(Some(PointerTarget::App(1)), false, &catalog),
            Action::None
        );
        assert_eq!(controller.selected_app.as_deref(), Some("two"));
        assert_eq!(
            controller.handle_pointer(Some(PointerTarget::App(1)), true, &catalog),
            Action::None
        );
        assert_eq!(controller.screen, Screen::AppDetails);
        assert_eq!(controller.selected_app.as_deref(), Some("two"));
    }

    #[test]
    fn pointer_app_indexes_follow_the_filtered_visible_list() {
        let mut hidden = app("hidden", "Hidden", false);
        hidden.metadata = None;
        let catalog = [hidden, app("visible", "Visible", false)];
        let mut controller = AppController::new();
        controller.filter = crate::catalog::Filter::MetadataOnly;
        controller.sort = SortOrder::ReleaseDate;
        controller.handle_pointer(Some(PointerTarget::App(0)), false, &catalog);
        assert_eq!(controller.selected_app.as_deref(), Some("visible"));
    }

    #[test]
    fn pointer_focuses_modal_buttons_and_uses_normal_actions() {
        let catalog = [app("one", "One", false)];
        let mut controller = AppController::new();
        controller.handle(Input::OpenOptions, &catalog);
        assert_eq!(
            controller.handle_pointer(Some(PointerTarget::Option(2)), true, &catalog),
            Action::None
        );
        assert_eq!(controller.pending_sort(), SortOrder::Name);
        assert_eq!(controller.options_focus(), 2);
        assert_eq!(
            controller.handle_pointer(Some(PointerTarget::Option(4)), true, &catalog),
            Action::ApplyOptions
        );
        assert_eq!(controller.screen, Screen::Browser);
    }

    #[test]
    fn pointer_activates_menu_details_delete_notice_and_page_targets() {
        let catalog = [
            app("one", "One", false),
            app("two", "Two", false),
            app("three", "Three", false),
            app("four", "Four", false),
            app("five", "Five", false),
            app("six", "Six", false),
        ];
        let mut controller = AppController::new();

        assert_eq!(
            controller.handle_pointer(Some(PointerTarget::Page { forward: true }), true, &catalog),
            Action::NextPage
        );
        assert_eq!(controller.selected_app.as_deref(), Some("two"));
        controller.handle(Input::Home, &catalog);
        assert_eq!(
            controller.handle_pointer(Some(PointerTarget::Menu(1)), true, &catalog),
            Action::None
        );
        assert_eq!(controller.screen, Screen::About);
        controller.handle(Input::Back, &catalog);
        controller.handle_pointer(Some(PointerTarget::Menu(0)), true, &catalog);
        controller.handle_pointer(Some(PointerTarget::App(5)), true, &catalog);
        assert_eq!(
            controller.handle_pointer(Some(PointerTarget::Detail(8)), true, &catalog),
            Action::None
        );
        assert_eq!(controller.screen, Screen::DeleteConfirm);
        assert_eq!(
            controller.handle_pointer(
                Some(PointerTarget::DeleteChoice { yes: true }),
                true,
                &catalog
            ),
            Action::DeleteSelected
        );
        controller.show_notice("Issue".into());
        assert_eq!(
            controller.handle_pointer(Some(PointerTarget::NoticeConfirm), true, &catalog),
            Action::None
        );
        assert_eq!(controller.screen, Screen::Browser);
    }

    #[test]
    fn deleting_an_app_requires_an_explicit_yes_confirmation() {
        let catalog = [app("one", "One", true)];
        let mut controller = AppController::new();
        controller.handle(Input::Confirm, &catalog);
        controller.handle(Input::Left, &catalog);
        controller.handle(Input::Left, &catalog);
        assert_eq!(controller.detail_focus(), 8);
        assert_eq!(controller.handle(Input::Confirm, &catalog), Action::None);
        assert_eq!(controller.screen, Screen::DeleteConfirm);
        assert_eq!(controller.handle(Input::Confirm, &catalog), Action::None);
        assert_eq!(controller.screen, Screen::AppDetails);
        assert_eq!(controller.detail_focus(), 8);
        controller.handle(Input::Confirm, &catalog);
        assert_eq!(controller.screen, Screen::DeleteConfirm);
        controller.handle(Input::Back, &catalog);
        assert_eq!(controller.screen, Screen::AppDetails);
        assert_eq!(controller.detail_focus(), 8);

        controller.handle(Input::Confirm, &catalog);
        controller.handle(Input::Right, &catalog);
        assert_eq!(
            controller.handle(Input::Confirm, &catalog),
            Action::DeleteSelected
        );
        assert_eq!(controller.screen, Screen::Browser);
    }

    #[test]
    fn received_zip_requires_yes_and_no_or_back_discards_it() {
        let mut controller = AppController::new();
        controller.request_wiiload_zip_confirmation();
        assert_eq!(controller.screen, Screen::WiiloadConfirm);
        assert_eq!(
            controller.handle(Input::Confirm, &[]),
            Action::RejectWiiloadZip
        );
        controller.request_wiiload_zip_confirmation();
        controller.handle(Input::Right, &[]);
        assert_eq!(controller.wiiload_confirmation_yes(), true);
        assert_eq!(
            controller.handle(Input::Confirm, &[]),
            Action::AcceptWiiloadZip
        );
        controller.request_wiiload_zip_confirmation();
        assert_eq!(
            controller.handle(Input::Back, &[]),
            Action::RejectWiiloadZip
        );
        controller.request_wiiload_zip_confirmation();
        assert_eq!(
            controller.handle(Input::Home, &[]),
            Action::RejectWiiloadZip
        );
        assert_eq!(controller.screen, Screen::MainMenu);
    }

    #[test]
    fn received_zip_pointer_choices_use_the_same_yes_no_actions() {
        let mut controller = AppController::new();
        controller.request_wiiload_zip_confirmation();
        assert_eq!(
            controller.handle_pointer(Some(PointerTarget::WiiloadChoice { yes: true }), true, &[]),
            Action::AcceptWiiloadZip
        );
        controller.request_wiiload_zip_confirmation();
        assert_eq!(
            controller.handle_pointer(Some(PointerTarget::WiiloadChoice { yes: false }), true, &[]),
            Action::RejectWiiloadZip
        );
    }

    #[test]
    fn zip_install_screen_ignores_controller_and_pointer_input() {
        let mut controller = AppController::new();
        controller.show_wiiload_installing();
        assert_eq!(controller.screen, Screen::WiiloadInstalling);
        assert_eq!(controller.handle(Input::Home, &[]), Action::None);
        assert_eq!(
            controller.handle_pointer(Some(PointerTarget::NoticeConfirm), true, &[]),
            Action::None
        );
        controller.finish_wiiload_installing();
        assert_eq!(controller.screen, Screen::Browser);
    }

    #[test]
    fn theme_detail_action_applies_the_theme_instead_of_launching_it() {
        let mut theme = app("ocean", "Ocean", true);
        theme.kind = AppKind::Theme;
        let catalog = [theme];
        let mut controller = AppController::new();
        controller.handle(Input::Confirm, &catalog);
        assert_eq!(controller.screen, Screen::AppDetails);
        controller.handle(Input::Left, &catalog);
        assert_eq!(controller.detail_focus(), 9);
        assert_eq!(
            controller.handle(Input::Confirm, &catalog),
            Action::ApplyTheme
        );
        assert_eq!(controller.screen, Screen::Browser);
    }

    #[test]
    fn browse_mode_toggle_keeps_selected_app_identity() {
        let catalog = [app("with-icon", "A", true), app("without-icon", "B", false)];
        let mut controller = AppController::new();
        controller.handle(Input::Down, &catalog);
        assert_eq!(controller.selected_app.as_deref(), Some("without-icon"));
        controller.handle(Input::ToggleBrowseMode, &catalog);
        assert_eq!(controller.browser_layout().columns, 3);
        assert_eq!(controller.selected_app.as_deref(), Some("without-icon"));
        assert_eq!(controller.selected_index(), 1);
        controller.handle(Input::ToggleBrowseMode, &catalog);
        assert_eq!(controller.browser_layout().columns, 1);
    }

    #[test]
    fn empty_catalog_clears_selection_and_confirmation_does_nothing() {
        let mut controller = AppController::new();
        controller.selected_app = Some("stale-entry".into());
        assert_eq!(controller.handle(Input::Down, &[]), Action::None);
        assert_eq!(controller.selected_app, None);
        assert_eq!(controller.handle(Input::Confirm, &[]), Action::None);
        assert_eq!(controller.screen, Screen::Browser);
    }

    #[test]
    fn home_menu_navigation_matches_main_view_actions() {
        let mut controller = AppController::new();
        controller.set_bootmii_available(true);
        controller.handle(Input::Home, &[]);
        assert_eq!(controller.screen, Screen::MainMenu);
        controller.handle(Input::Down, &[]);
        assert_eq!(controller.handle(Input::Confirm, &[]), Action::None);
        assert_eq!(controller.screen, Screen::About);
        controller.handle(Input::Back, &[]);
        assert_eq!(controller.screen, Screen::MainMenu);
        controller.handle(Input::Down, &[]);
        assert_eq!(
            controller.handle(Input::Confirm, &[]),
            Action::LaunchBootMii
        );
        controller.handle(Input::Down, &[]);
        assert_eq!(controller.handle(Input::Confirm, &[]), Action::Exit);
    }

    #[test]
    fn options_changes_are_pending_until_ok_and_back_discards_them() {
        let catalog = [app("one", "One", true)];
        let mut controller = AppController::new();
        controller.set_preferred_device(1);
        controller.sort = SortOrder::ReleaseDate;

        assert_eq!(
            controller.handle(Input::OpenOptions, &catalog),
            Action::None
        );
        assert_eq!(controller.screen, Screen::Options);
        assert_eq!(controller.pending_device(), 1);
        assert_eq!(controller.pending_sort(), SortOrder::ReleaseDate);
        controller.handle(Input::Up, &catalog);
        controller.handle(Input::Left, &catalog);
        assert_eq!(controller.options_focus(), 2);
        controller.handle(Input::Confirm, &catalog);
        assert_eq!(controller.pending_sort(), SortOrder::Name);
        assert_eq!(controller.handle(Input::Back, &catalog), Action::None);
        assert_eq!(controller.screen, Screen::Browser);
        assert_eq!(controller.preferred_device(), 1);
        assert_eq!(controller.sort, SortOrder::ReleaseDate);

        controller.handle(Input::OpenOptions, &catalog);
        controller.handle(Input::Up, &catalog);
        controller.handle(Input::Left, &catalog);
        controller.handle(Input::Confirm, &catalog);
        controller.handle(Input::Down, &catalog);
        assert_eq!(controller.options_focus(), 4);
        assert_eq!(
            controller.handle(Input::Confirm, &catalog),
            Action::ApplyOptions
        );
        assert_eq!(controller.screen, Screen::Browser);
        assert_eq!(controller.preferred_device(), 1);
        assert_eq!(controller.sort, SortOrder::Name);
        assert_eq!(controller.filter, Filter::All);
    }

    #[test]
    fn options_starts_with_the_active_device_after_preference_fallback() {
        let mut controller = AppController::new();
        controller.set_preferred_device(1);
        controller.set_active_device(Some(0));
        controller.handle(Input::OpenOptions, &[]);
        assert_eq!(controller.pending_device(), 0);
    }

    #[test]
    fn main_menu_hides_and_remaps_bootmii_when_ios_is_unavailable() {
        let mut controller = AppController::new();
        controller.handle(Input::Home, &[]);
        assert_eq!(controller.menu_item_count(), 4);
        controller.handle(Input::Down, &[]);
        controller.handle(Input::Down, &[]);
        assert_eq!(controller.handle(Input::Confirm, &[]), Action::Exit);
        controller.handle(Input::Down, &[]);
        assert_eq!(controller.handle(Input::Confirm, &[]), Action::Shutdown);

        let mut available = AppController::new();
        available.set_bootmii_available(true);
        available.handle(Input::Home, &[]);
        available.handle(Input::Down, &[]);
        available.handle(Input::Down, &[]);
        assert_eq!(available.menu_item_count(), 5);
        assert_eq!(available.handle(Input::Confirm, &[]), Action::LaunchBootMii);
    }

    #[test]
    fn notice_dismissal_restores_the_screen_that_opened_it() {
        let catalog = [app("one", "One", true)];
        let mut controller = AppController::new();
        controller.handle(Input::Confirm, &catalog);
        controller.show_notice("Load failed".into());
        assert_eq!(controller.screen, Screen::Notice);
        assert_eq!(controller.notice(), Some("Load failed"));
        assert_eq!(controller.handle(Input::Confirm, &catalog), Action::None);
        assert_eq!(controller.screen, Screen::AppDetails);
        assert_eq!(controller.notice(), None);
    }

    #[test]
    fn page_inputs_move_selection_and_stop_at_page_boundaries() {
        let catalog = [
            app("a", "A", true),
            app("b", "B", true),
            app("c", "C", true),
            app("d", "D", true),
            app("e", "E", true),
            app("f", "F", true),
            app("g", "G", true),
        ];
        let mut controller = AppController::new();
        assert_eq!(
            controller.handle(Input::NextPage, &catalog),
            Action::NextPage
        );
        assert_eq!(controller.selected_index(), 5);
        assert_eq!(controller.selected_app.as_deref(), Some("f"));
        assert_eq!(controller.page_start(), 5);
        assert_eq!(controller.visible_page(&catalog).len(), 2);
        assert_eq!(controller.handle(Input::NextPage, &catalog), Action::None);
        assert_eq!(
            controller.handle(Input::PreviousPage, &catalog),
            Action::PreviousPage
        );
        assert_eq!(controller.selected_index(), 0);
        assert_eq!(controller.selected_app.as_deref(), Some("a"));
        assert_eq!(
            controller.handle(Input::PreviousPage, &catalog),
            Action::None
        );
    }

    #[test]
    fn grid_controls_move_by_row_and_keep_identity_after_resort() {
        let catalog = [
            app("a", "A", true),
            app("b", "B", true),
            app("c", "C", true),
            app("d", "D", true),
            app("e", "E", true),
            app("f", "F", true),
        ];
        let mut controller = AppController::new();
        controller.set_browser_layout(
            BrowserLayout {
                columns: 3,
                rows: 2,
            },
            &catalog,
        );
        controller.handle(Input::Right, &catalog);
        assert_eq!(controller.selected_app.as_deref(), Some("b"));
        controller.handle(Input::Down, &catalog);
        assert_eq!(controller.selected_app.as_deref(), Some("e"));
        controller.sort = SortOrder::ReleaseDate;
        controller.filter = Filter::MetadataOnly;
        controller.handle(Input::OpenOptions, &catalog);
        controller.handle(Input::Up, &catalog);
        controller.handle(Input::Left, &catalog);
        controller.handle(Input::Confirm, &catalog);
        controller.handle(Input::Down, &catalog);
        controller.handle(Input::Confirm, &catalog);
        assert_eq!(controller.selected_app.as_deref(), Some("e"));
        assert_eq!(controller.selected_index(), 4);
        assert_eq!(controller.sort, SortOrder::Name);
    }

    #[test]
    fn missing_selection_uses_first_visible_app() {
        let catalog = [
            app("a", "A", true),
            app("b", "B", true),
            app("c", "C", true),
        ];
        let mut controller = AppController::new();
        controller.handle(Input::Down, &catalog);
        controller.handle(Input::Down, &catalog);
        let catalog = [app("a", "A", true), app("c", "C", true)];
        controller.handle(Input::Up, &catalog);
        assert_eq!(controller.selected_app.as_deref(), Some("a"));
        assert_eq!(controller.selected_index(), 0);
    }
}
