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
    CycleFilter,
    CycleSort,
    PreviousPage,
    NextPage,
}

/// Work for a hardware or service layer to perform after a state transition.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Action {
    #[default]
    None,
    LaunchSelected,
    DeleteSelected,
    LaunchBootMii,
    Exit,
    Shutdown,
    PreviousPage,
    NextPage,
}

/// Main menu focus follows HBC's main view order: browser, about, BootMii,
/// exit, shutdown.
const MENU_ITEMS: usize = 5;
const DEFAULT_BROWSER_ROWS: usize = 5;

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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AppController {
    pub screen: Screen,
    pub selected_app: Option<String>,
    pub filter: Filter,
    pub sort: SortOrder,
    selected_index: usize,
    layout: BrowserLayout,
    menu_focus: usize,
    detail_focus: usize,
    delete_confirmation_yes: bool,
    notice: Option<String>,
    notice_return_screen: Screen,
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

    /// Keep the selected app identity valid after a catalog refresh.
    pub fn reconcile_catalog(&mut self, catalog: &[AppEntry]) {
        self.reconcile_selection(catalog);
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

    /// Index of the first entry on the selected app's page.
    pub fn page_start(&self) -> usize {
        self.selected_index / self.layout.page_capacity() * self.layout.page_capacity()
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
                Input::Confirm if self.selected_app.is_some() => match self.detail_focus {
                    8 => {
                        self.delete_confirmation_yes = false;
                        self.screen = Screen::DeleteConfirm;
                        Action::None
                    }
                    9 => Action::LaunchSelected,
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
                Action::None
            }
            Input::CycleFilter => {
                self.filter = match self.filter {
                    Filter::All => Filter::IconsOnly,
                    Filter::IconsOnly => Filter::MetadataOnly,
                    Filter::MetadataOnly => Filter::All,
                };
                self.reconcile_selection(catalog);
                Action::None
            }
            Input::CycleSort => {
                self.sort = match self.sort {
                    SortOrder::Name => SortOrder::ReleaseDate,
                    SortOrder::ReleaseDate => SortOrder::Name,
                };
                self.reconcile_selection(catalog);
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
                2 => Action::LaunchBootMii,
                3 => Action::Exit,
                4 => Action::Shutdown,
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
        let capacity = self.layout.page_capacity();
        let current_start = self.page_start();
        let next_start = if forward {
            current_start.saturating_add(capacity)
        } else {
            current_start.saturating_sub(capacity)
        };
        if next_start == current_start || next_start >= visible.len() {
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
            (self.menu_focus + 1).min(MENU_ITEMS - 1)
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
    fn filtered_entries_keep_identity_when_possible_and_select_visible_fallback() {
        let catalog = [app("with-icon", "A", true), app("without-icon", "B", false)];
        let mut controller = AppController::new();
        controller.handle(Input::Down, &catalog);
        assert_eq!(controller.selected_app.as_deref(), Some("without-icon"));
        controller.handle(Input::CycleFilter, &catalog);
        assert_eq!(controller.filter, Filter::IconsOnly);
        assert_eq!(controller.selected_app.as_deref(), Some("with-icon"));
        assert_eq!(controller.selected_index(), 0);
        assert_eq!(controller.visible_apps(&catalog).len(), 1);
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
        controller.handle(Input::CycleSort, &catalog);
        assert_eq!(controller.selected_app.as_deref(), Some("e"));
        assert_eq!(controller.selected_index(), 4);
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
