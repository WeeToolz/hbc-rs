//! Hardware-independent frame orchestration for HBC application state.

use alloc::{string::String, vec::Vec};

use crate::{
    app::{Action, AppController, Input, NoticeKind, PointerTarget},
    catalog::AppEntry,
    metadata::{ThemeMetadata, UserSettings},
    platform::input::InputFrame,
    settings::SettingsState,
};

/// Owns the current catalog and applies platform input to the app controller.
/// Storage, drawing, and service actions remain at the platform edge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppRuntime {
    controller: AppController,
    catalog: Vec<AppEntry>,
    settings: SettingsState,
    settings_persistence_enabled: bool,
    theme: Option<ThemeMetadata>,
}

impl Default for AppRuntime {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SettingsSaveError<E> {
    PreviousLoadFailed,
    Backend(E),
}

impl AppRuntime {
    pub fn new(catalog: Vec<AppEntry>) -> Self {
        Self {
            controller: AppController::new(),
            catalog,
            settings: SettingsState::default(),
            settings_persistence_enabled: true,
            theme: None,
        }
    }

    pub fn controller(&self) -> &AppController {
        &self.controller
    }

    pub fn set_measured_memo(&mut self, max_scroll: usize, line_height: u16) {
        self.controller.set_measured_memo(max_scroll, line_height);
    }

    pub fn request_wiiload_zip_confirmation(&mut self) {
        self.controller.request_wiiload_zip_confirmation();
    }

    pub fn set_wiiload_confirmation_text(&mut self, text: String) {
        self.controller.set_wiiload_confirmation_text(text);
    }

    pub fn show_wiiload_installing(&mut self) {
        self.controller.show_wiiload_installing();
    }

    pub fn show_wiiload_receiving(&mut self) {
        self.controller.show_wiiload_receiving();
    }

    pub fn finish_wiiload_receiving(&mut self) {
        self.controller.finish_wiiload_receiving();
    }

    pub fn finish_wiiload_installing(&mut self) {
        self.controller.finish_wiiload_installing();
    }

    pub fn set_wiiload_install_progress(&mut self, percent: u8) {
        self.controller.set_wiiload_install_progress(percent);
    }

    pub fn set_network_config(&mut self, config: crate::network_config::NetworkConfig) {
        self.controller.set_network_config(config);
    }

    pub fn set_main_menu_status(&mut self, ios_label: String, network_label: String) {
        self.controller
            .set_main_menu_status(ios_label, network_label);
    }

    pub fn set_scanned_networks(&mut self, networks: crate::network_config::ScannedNetworks) {
        self.controller.set_scanned_networks(networks);
    }

    pub fn network_config_for_save(
        &self,
    ) -> Result<crate::network_config::NetworkConfig, crate::app::NetworkConfigEditError> {
        self.controller.network_config_for_save()
    }

    pub fn network_config_snapshot(&self) -> Option<&crate::network_config::NetworkConfig> {
        self.controller.network_config_snapshot()
    }

    pub fn clear_network_config(&mut self) {
        self.controller.clear_network_config();
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
        self.controller.set_about_theme_description(
            theme.description.as_deref().is_some_and(|s| !s.is_empty()),
        );
        self.theme = Some(theme);
    }

    /// Show a platform action failure in the shared notice dialog.
    pub fn show_notice(&mut self, message: &str) {
        self.controller.show_notice(String::from(message));
    }

    pub fn show_notice_with_kind(&mut self, message: &str, kind: NoticeKind) {
        self.controller
            .show_notice_with_kind(String::from(message), kind);
    }

    pub fn set_bootmii_available(&mut self, available: bool) {
        self.controller.set_bootmii_available(available);
    }

    pub fn set_active_device(&mut self, device: Option<i32>) {
        self.controller.set_active_device(device);
    }

    pub fn set_device_availability(&mut self, availability: [bool; 4]) {
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
    ) -> Result<bool, SettingsSaveError<E>> {
        if !self.settings_persistence_enabled && self.settings.is_dirty() {
            return Err(SettingsSaveError::PreviousLoadFailed);
        }
        self.settings
            .save_if_dirty(save)
            .map_err(SettingsSaveError::Backend)
    }

    pub fn set_settings_persistence_enabled(&mut self, enabled: bool) {
        self.settings_persistence_enabled = enabled;
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
    /// Directional selection uses new presses, as in C HBC. Held page inputs repeat.
    /// A newly pressed input is not
    /// applied twice when the platform reports it as both pressed and held.
    pub fn update(&mut self, frame: &InputFrame) -> Vec<Action> {
        self.controller.reconcile_catalog(&self.catalog);
        if self.controller.timed_notice_active() {
            let input = frame
                .pressed
                .first()
                .or_else(|| frame.held.first())
                .copied();
            if let Some(input) = input {
                let mut actions = Vec::new();
                push_action(&mut actions, self.controller.handle(input, &self.catalog));
                self.controller.mark_timed_notice_input_consumed();
                return actions;
            }
            self.controller.tick_timed_notice();
            return Vec::new();
        }
        self.controller.tick_about_scroll();
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
    matches!(input, Input::PreviousPage | Input::NextPage)
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
    fn directional_selection_requires_a_new_press() {
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
        assert_eq!(runtime.controller().selected_app.as_deref(), Some("b"));
        for _ in 0..120 {
            runtime.update(&held);
        }
        assert_eq!(runtime.controller().selected_app.as_deref(), Some("b"));
        runtime.update(&InputFrame::default());
        runtime.update(&first);
        assert_eq!(runtime.controller().selected_app.as_deref(), Some("c"));
    }

    #[test]
    fn error_notice_dismissal_consumes_timeout_frame_input() {
        let mut runtime = AppRuntime::new(vec![app("one")]);
        runtime.show_notice_with_kind("Storage error", crate::app::NoticeKind::Error);
        for _ in 0..299 {
            runtime.update(&InputFrame::default());
        }
        assert_eq!(runtime.controller().screen, crate::app::Screen::Notice);
        runtime.update(&InputFrame {
            pressed: vec![Input::Confirm, Input::Down],
            ..InputFrame::default()
        });
        assert_eq!(runtime.controller().screen, crate::app::Screen::Browser);
    }

    #[test]
    fn warning_notice_closes_after_five_seconds_without_input() {
        let mut runtime = AppRuntime::new(vec![app("one")]);
        runtime.show_notice_with_kind("Theme warning", crate::app::NoticeKind::Warning);
        for _ in 0..299 {
            runtime.update(&InputFrame::default());
        }
        assert_eq!(runtime.controller().screen, crate::app::Screen::Notice);
        runtime.update(&InputFrame::default());
        assert_eq!(runtime.controller().screen, crate::app::Screen::Browser);
    }

    #[test]
    fn options_can_open_network_settings_without_any_mounted_apps() {
        let mut runtime = AppRuntime::new(Vec::new());
        runtime.update(&InputFrame {
            pressed: vec![Input::OpenOptions],
            ..Default::default()
        });
        assert_eq!(runtime.controller().screen, crate::app::Screen::Options);
        runtime.update(&InputFrame {
            pressed: vec![Input::Right],
            ..Default::default()
        });
        let actions = runtime.update(&InputFrame {
            pressed: vec![Input::Confirm],
            ..Default::default()
        });
        assert_eq!(actions, [Action::LoadNetworkConfig]);
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
                Input::Up,
                Input::Left,
                Input::Confirm,
                Input::Down,
                Input::Down,
                Input::Left,
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
        runtime.set_device_availability([true, true, false, false]);
        let actions = runtime.update(&InputFrame {
            pressed: vec![
                Input::OpenOptions,
                Input::Left, // Back -> OK
                Input::Left, // OK -> Date
                Input::Left, // Date -> Name
                Input::Left, // Skip unavailable SD Gecko slots -> USB
                Input::Confirm,
                Input::Right, // USB -> Name
                Input::Right, // Name -> Date
                Input::Confirm,
                Input::Right, // Date -> OK
                Input::Confirm,
            ],
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

    #[test]
    fn blocked_settings_persistence_does_not_overwrite_unreadable_data() {
        let mut runtime = AppRuntime::new(vec![app("a")]);
        runtime.set_preferred_device(1);
        runtime.set_settings_persistence_enabled(false);
        let mut save_called = false;

        assert_eq!(
            runtime.save_settings_if_dirty(|_| {
                save_called = true;
                Ok::<(), i32>(())
            }),
            Err(SettingsSaveError::PreviousLoadFailed)
        );
        assert!(!save_called);
        assert!(runtime.settings().is_dirty());
    }
}
