//! Hardware-independent frame orchestration for HBC application state.

use alloc::{string::String, vec::Vec};

use crate::{
    app::{Action, AppController, Input, PointerTarget},
    catalog::AppEntry,
    metadata::{ThemeMetadata, UserSettings},
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
    theme: Option<ThemeMetadata>,
}

impl AppRuntime {
    pub fn new(catalog: Vec<AppEntry>) -> Self {
        Self {
            controller: AppController::new(),
            catalog,
            settings: SettingsState::default(),
            theme: None,
        }
    }

    pub fn controller(&self) -> &AppController {
        &self.controller
    }

    pub fn request_wiiload_zip_confirmation(&mut self) {
        self.controller.request_wiiload_zip_confirmation();
    }

    pub fn show_wiiload_installing(&mut self) {
        self.controller.show_wiiload_installing();
    }

    pub fn finish_wiiload_installing(&mut self) {
        self.controller.finish_wiiload_installing();
    }

    pub fn set_wiiload_install_progress(&mut self, percent: u8) {
        self.controller.set_wiiload_install_progress(percent);
    }

    pub fn catalog(&self) -> &[AppEntry] {
        &self.catalog
    }

    pub fn settings(&self) -> &SettingsState {
        &self.settings
    }

    pub fn theme(&self) -> Option<&ThemeMetadata> {
        self.theme.as_ref()
    }

    pub fn apply_theme(&mut self, theme: ThemeMetadata) {
        self.theme = Some(theme);
    }

    /// Show a platform action failure in the shared notice dialog.
    pub fn show_notice(&mut self, message: &str) {
        self.controller.show_notice(String::from(message));
    }

    pub fn set_bootmii_available(&mut self, available: bool) {
        self.controller.set_bootmii_available(available);
    }

    pub fn set_active_device(&mut self, device: Option<i32>) {
        self.controller.set_active_device(device);
    }

    pub fn set_device_availability(&mut self, availability: [bool; 2]) {
        self.controller.set_device_availability(availability);
    }

    pub fn set_widescreen(&mut self, widescreen: bool) {
        self.controller
            .set_grid_columns(if widescreen { 4 } else { 3 }, &self.catalog);
    }

    pub fn set_preferred_device(&mut self, device: i32) {
        self.controller.set_preferred_device(device);
        self.sync_settings();
    }

    pub fn save_settings_if_dirty<E>(
        &mut self,
        save: impl FnOnce(&UserSettings) -> Result<(), E>,
    ) -> Result<bool, E> {
        self.settings.save_if_dirty(save)
    }

    /// Apply settings loaded from NAND before the first rendered frame.
    pub fn load_settings(&mut self, settings: UserSettings) {
        self.controller.set_preferred_device(settings.device);
        self.controller.sort = if settings.sort_order == 1 {
            crate::catalog::SortOrder::ReleaseDate
        } else {
            crate::catalog::SortOrder::Name
        };
        self.controller.filter = match self.controller.sort {
            crate::catalog::SortOrder::Name => crate::catalog::Filter::All,
            crate::catalog::SortOrder::ReleaseDate => crate::catalog::Filter::MetadataOnly,
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
        self.controller
            .scroll_active_memo_by_stick(frame.stick_scroll_pixels);
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

    /// Apply pointer focus after the current screen's frame has supplied its
    /// hit target. Pointer activation uses the controller's normal Confirm
    /// transition, so it shares the same action behavior as button input.
    pub fn update_pointer(&mut self, target: Option<PointerTarget>, activate: bool) -> Vec<Action> {
        self.update_pointer_frame(target, None, activate, false)
    }

    pub fn update_pointer_frame(
        &mut self,
        target: Option<PointerTarget>,
        pointer_y: Option<i16>,
        pressed: bool,
        held: bool,
    ) -> Vec<Action> {
        let action =
            self.controller
                .handle_pointer_frame(target, pointer_y, pressed, held, &self.catalog);
        self.sync_settings();
        let mut actions = Vec::new();
        push_action(&mut actions, action);
        actions
    }

    fn sync_settings(&mut self) {
        let current = self.settings.current_mut();
        current.sort_order = match self.controller.sort {
            crate::catalog::SortOrder::Name => 0,
            crate::catalog::SortOrder::ReleaseDate => 1,
        };
        current.device = self.controller.preferred_device();
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
            ..InputFrame::default()
        };
        assert!(runtime.update(&first).is_empty());
        assert_eq!(runtime.controller().selected_app.as_deref(), Some("b"));

        let held = InputFrame {
            pressed: vec![],
            held: vec![Input::Down],
            released: vec![],
            ..InputFrame::default()
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
            ..InputFrame::default()
        };
        assert!(runtime.update(&frame).is_empty());
    }

    #[test]
    fn settings_load_and_frame_updates_keep_sort_and_selection_persistent() {
        let mut first = app("a");
        first.metadata = Some(crate::metadata::AppMetadata::default());
        let mut second = app("b");
        second.metadata = Some(crate::metadata::AppMetadata::default());
        let mut runtime = AppRuntime::new(vec![first, second]);
        runtime.load_settings(UserSettings {
            sort_order: 1,
            browse_mode: 1,
            selected_app: Some("b".into()),
            ..UserSettings::default()
        });
        assert_eq!(runtime.controller().browser_layout().columns, 3);
        assert!(!runtime.settings().is_dirty());

        let frame = InputFrame {
            pressed: vec![
                Input::OpenOptions,
                Input::Up,
                Input::Left,
                Input::Confirm,
                Input::Down,
                Input::Confirm,
            ],
            held: vec![],
            released: vec![],
            ..InputFrame::default()
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
    fn widescreen_policy_selects_four_grid_columns_and_tracks_mode_changes() {
        let mut runtime = AppRuntime::new(vec![app("a"), app("b"), app("c"), app("d")]);
        runtime.set_widescreen(true);
        runtime.update(&InputFrame {
            pressed: vec![Input::ToggleBrowseMode],
            ..InputFrame::default()
        });
        assert_eq!(runtime.controller().browser_layout().columns, 4);

        runtime.set_widescreen(false);
        assert_eq!(runtime.controller().browser_layout().columns, 3);
    }

    #[test]
    fn options_dialog_commits_device_and_sort_to_runtime_settings() {
        let mut runtime = AppRuntime::new(Vec::new());
        runtime.load_settings(UserSettings {
            device: 0,
            sort_order: 0,
            ..UserSettings::default()
        });
        runtime.update(&InputFrame {
            pressed: vec![Input::OpenOptions],
            ..InputFrame::default()
        });
        runtime.update(&InputFrame {
            pressed: vec![Input::Up, Input::Right, Input::Confirm],
            ..InputFrame::default()
        });
        runtime.update(&InputFrame {
            pressed: vec![Input::Up, Input::Right, Input::Confirm],
            ..InputFrame::default()
        });
        let actions = runtime.update(&InputFrame {
            pressed: vec![Input::Down, Input::Left, Input::Down, Input::Confirm],
            ..InputFrame::default()
        });
        assert_eq!(actions, vec![Action::ApplyOptions]);
        assert_eq!(runtime.settings().current().device, 1);
        assert_eq!(runtime.settings().current().sort_order, 1);
        assert_eq!(
            runtime.controller().filter,
            crate::catalog::Filter::MetadataOnly
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
