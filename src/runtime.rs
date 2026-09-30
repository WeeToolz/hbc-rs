//! Hardware-independent frame orchestration for HBC application state.

use alloc::{string::String, vec::Vec};

use crate::{
    app::{Action, AppController, Input},
    catalog::AppEntry,
    metadata::UserSettings,
    platform::input::InputFrame,
    settings::SettingsState,
};

/// Owns the current catalog and applies platform input to the app controller.
/// Storage, drawing, and service actions remain at the platform edge.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AppRuntime {
    controller: AppController,
    catalog: Vec<AppEntry>,
    settings: SettingsState,
}

impl AppRuntime {
    pub fn new(catalog: Vec<AppEntry>) -> Self {
        Self {
            controller: AppController::new(),
            catalog,
            settings: SettingsState::default(),
        }
    }

    pub fn controller(&self) -> &AppController {
        &self.controller
    }

    pub fn catalog(&self) -> &[AppEntry] {
        &self.catalog
    }

    pub fn settings(&self) -> &SettingsState {
        &self.settings
    }

    /// Show a platform action failure in the shared notice dialog.
    pub fn show_notice(&mut self, message: &str) {
        self.controller.show_notice(String::from(message));
    }

    pub fn set_bootmii_available(&mut self, available: bool) {
        self.controller.set_bootmii_available(available);
    }

    /// Apply settings loaded from NAND before the first rendered frame.
    pub fn load_settings(&mut self, settings: UserSettings) {
        self.controller.sort = if settings.sort_order == 1 {
            crate::catalog::SortOrder::ReleaseDate
        } else {
            crate::catalog::SortOrder::Name
        };
        self.controller.selected_app = settings.selected_app.clone();
        self.controller.set_browser_layout(
            crate::app::BrowserLayout {
                columns: if settings.browse_mode == 1 { 3 } else { 1 },
                rows: 5,
            },
            &self.catalog,
        );
        self.settings.load(settings);
    }

    /// Replace a device-scan snapshot and keep selection inside the new list.
    pub fn replace_catalog(&mut self, catalog: Vec<AppEntry>) {
        self.catalog = catalog;
        self.controller.reconcile_catalog(&self.catalog);
    }

    /// Apply one input frame and return actions for storage or platform code.
    /// Held directional and page inputs repeat. A newly pressed input is not
    /// applied twice when the platform reports it as both pressed and held.
    pub fn update(&mut self, frame: &InputFrame) -> Vec<Action> {
        self.controller.reconcile_catalog(&self.catalog);
        let mut actions = Vec::new();
        for &input in &frame.pressed {
            push_action(&mut actions, self.controller.handle(input, &self.catalog));
        }
        for &input in &frame.held {
            if is_repeatable(input) && !frame.pressed.contains(&input) {
                push_action(&mut actions, self.controller.handle(input, &self.catalog));
            }
        }
        self.sync_settings();
        actions
    }

    fn sync_settings(&mut self) {
        let current = self.settings.current_mut();
        current.sort_order = match self.controller.sort {
            crate::catalog::SortOrder::Name => 0,
            crate::catalog::SortOrder::ReleaseDate => 1,
        };
        current.selected_app = self.controller.selected_app.clone();
        current.browse_mode = if self.controller.browser_layout().columns > 1 {
            1
        } else {
            0
        };
    }
}

fn push_action(actions: &mut Vec<Action>, action: Action) {
    if action != Action::None {
        actions.push(action);
    }
}

fn is_repeatable(input: Input) -> bool {
    matches!(
        input,
        Input::Up
            | Input::Down
            | Input::Left
            | Input::Right
            | Input::PreviousPage
            | Input::NextPage
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::AppKind;
    use alloc::vec;

    fn app(directory: &str) -> AppEntry {
        AppEntry {
            directory: directory.into(),
            kind: AppKind::Elf,
            size: 1,
            has_icon: false,
            metadata: None,
        }
    }

    #[test]
    fn applies_press_once_and_repeats_held_navigation() {
        let mut runtime = AppRuntime::new(vec![app("a"), app("b"), app("c")]);
        let first = InputFrame {
            pressed: vec![Input::Down],
            held: vec![Input::Down],
            released: vec![],
        };
        assert!(runtime.update(&first).is_empty());
        assert_eq!(runtime.controller().selected_app.as_deref(), Some("b"));

        let held = InputFrame {
            pressed: vec![],
            held: vec![Input::Down],
            released: vec![],
        };
        runtime.update(&held);
        assert_eq!(runtime.controller().selected_app.as_deref(), Some("c"));
    }

    #[test]
    fn one_shot_actions_are_not_repeated_while_held() {
        let mut runtime = AppRuntime::new(vec![app("a")]);
        let frame = InputFrame {
            pressed: vec![],
            held: vec![Input::Home, Input::Confirm],
            released: vec![],
        };
        assert!(runtime.update(&frame).is_empty());
    }

    #[test]
    fn settings_load_and_frame_updates_keep_sort_and_selection_persistent() {
        let mut runtime = AppRuntime::new(vec![app("a"), app("b")]);
        runtime.load_settings(UserSettings {
            sort_order: 1,
            browse_mode: 1,
            selected_app: Some("b".into()),
            ..UserSettings::default()
        });
        assert_eq!(runtime.controller().browser_layout().columns, 3);
        assert!(!runtime.settings().is_dirty());

        let frame = InputFrame {
            pressed: vec![Input::CycleSort],
            held: vec![],
            released: vec![],
        };
        runtime.update(&frame);
        assert!(runtime.settings().is_dirty());
        assert_eq!(runtime.settings().current().sort_order, 0);
        assert_eq!(
            runtime.settings().current().selected_app.as_deref(),
            Some("b")
        );
    }

    #[test]
    fn catalog_refresh_reconciles_selection_before_settings_sync() {
        let mut runtime = AppRuntime::new(vec![app("a"), app("b")]);
        runtime.load_settings(UserSettings {
            selected_app: Some("b".into()),
            ..UserSettings::default()
        });
        runtime.replace_catalog(vec![app("a")]);
        let idle = InputFrame::default();
        runtime.update(&idle);
        assert_eq!(runtime.controller().selected_app.as_deref(), Some("a"));
        assert_eq!(
            runtime.settings().current().selected_app.as_deref(),
            Some("a")
        );
    }
}
