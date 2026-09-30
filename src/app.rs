//! Hardware-independent application navigation state.

use alloc::{string::String, vec::Vec};

use crate::catalog::{visible_entries, AppEntry, Filter, SortOrder};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Screen {
    #[default]
    Browser,
    MainMenu,
    AppDetails,
    About,
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AppController {
    pub screen: Screen,
    pub selected_app: Option<String>,
    pub filter: Filter,
    pub sort: SortOrder,
    selected_index: usize,
    menu_focus: usize,
    detail_focus: usize,
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
                    8 => Action::DeleteSelected,
                    9 => Action::LaunchSelected,
                    10 => {
                        self.screen = Screen::Browser;
                        Action::None
                    }
                    _ => Action::None,
                },
                _ => Action::None,
            },
            Screen::About => match input {
                Input::Back | Input::Confirm => {
                    self.screen = Screen::MainMenu;
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
            Input::Up | Input::Left => {
                self.move_selection(catalog, false);
                Action::None
            }
            Input::Down | Input::Right => {
                self.move_selection(catalog, true);
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
            Input::PreviousPage => Action::PreviousPage,
            Input::NextPage => Action::NextPage,
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

    fn move_selection(&mut self, catalog: &[AppEntry], forward: bool) {
        let visible = self.visible_apps(catalog);
        if visible.is_empty() {
            self.selected_index = 0;
            self.selected_app = None;
            return;
        }
        self.selected_index = if forward {
            (self.selected_index + 1).min(visible.len() - 1)
        } else {
            self.selected_index.saturating_sub(1)
        };
        self.selected_app = Some(visible[self.selected_index].directory.clone());
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
            self.selected_index = self.selected_index.min(visible.len() - 1);
            self.selected_app = Some(visible[self.selected_index].directory.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::AppKind;
    use crate::metadata::AppMetadata;
    use alloc::vec;

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
        assert_eq!(
            controller.handle(Input::Confirm, &catalog),
            Action::DeleteSelected
        );
        controller.handle(Input::Back, &catalog);
        assert_eq!(controller.screen, Screen::Browser);
        assert_eq!(controller.selected_app.as_deref(), Some("two"));
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
    fn browser_page_inputs_return_actions() {
        let mut controller = AppController::new();
        assert_eq!(
            controller.handle(Input::NextPage, &vec![]),
            Action::NextPage
        );
        assert_eq!(
            controller.handle(Input::PreviousPage, &[]),
            Action::PreviousPage
        );
    }
}
