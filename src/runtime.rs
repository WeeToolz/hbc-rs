//! Hardware-independent frame orchestration for HBC application state.

use alloc::{string::String, vec::Vec};

use crate::{
    app::{Action, AppController, Input, NoticeKind, PointerTarget},
    catalog::AppEntry,
    metadata::{ThemeMetadata, UserSettings},
    platform::input::InputFrame,
    settings::SettingsState,
};
use ogc_ui::KeyRepeat;

/// Owns the current catalog and applies platform input to the app controller.
/// Storage, drawing, and service actions remain at the platform edge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppRuntime {
    controller: AppController,
    catalog: Vec<AppEntry>,
    settings: SettingsState,
    settings_persistence_enabled: bool,
    theme: Option<ThemeMetadata>,
    page_repeat: KeyRepeat<Input>,
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

/// Result of preparing settings for an application or system exit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExitSettingsOutcome {
    Unchanged,
    Saved,
    PreservedAfterLoadFailure,
}

impl AppRuntime {
    pub fn new(catalog: Vec<AppEntry>) -> Self {
        Self {
            controller: AppController::new(),
            catalog,
            settings: SettingsState::default(),
            settings_persistence_enabled: true,
            theme: None,
            // C HBC applies held page buttons once per frame.
            page_repeat: KeyRepeat::new(0, 1),
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

    pub fn show_executable_loading(&mut self) {
        self.controller.show_executable_loading();
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

    pub fn set_operation_progress(&mut self, percent: u8) {
        self.controller.set_operation_progress(percent);
    }

    pub fn operation_progress(&self) -> u8 {
        self.controller.operation_progress()
    }

    pub fn finish_executable_loading(&mut self) {
        self.controller.finish_executable_loading();
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

    pub fn set_device_availability(
        &mut self,
        availability: [bool; crate::platform::storage::DEVICE_COUNT],
    ) {
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

    /// Preserve unreadable settings without making their save a launch requirement.
    /// A real backend write failure still prevents departure.
    pub fn prepare_settings_for_exit<E>(
        &mut self,
        save: impl FnOnce(&UserSettings) -> Result<(), E>,
    ) -> Result<ExitSettingsOutcome, E> {
        match self.save_settings_if_dirty(save) {
            Ok(false) => Ok(ExitSettingsOutcome::Unchanged),
            Ok(true) => Ok(ExitSettingsOutcome::Saved),
            Err(SettingsSaveError::PreviousLoadFailed) => {
                Ok(ExitSettingsOutcome::PreservedAfterLoadFailure)
            }
            Err(SettingsSaveError::Backend(error)) => Err(error),
        }
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
            // The button that started a synchronous operation can still be held
            // when its result arrives. Only a new press can dismiss that result.
            let input = frame.pressed.first().copied();
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
        let memo_scroll_input = self
            .controller
            .scroll_active_memo_by_stick(frame.stick_scroll_pixels);
        let mut actions = Vec::new();
        let mut screen_changed = false;
        for &input in &frame.pressed {
            if memo_scroll_input == Some(input) && frame.stick_navigation.contains(&input) {
                continue;
            }
            let screen = self.controller.screen;
            push_action(&mut actions, self.controller.handle(input, &self.catalog));
            if self.controller.screen != screen {
                self.page_repeat.reset();
                screen_changed = true;
                break;
            }
        }
        let held = frame
            .held
            .iter()
            .copied()
            .find(|input| is_repeatable(*input));
        let pressed = frame
            .pressed
            .iter()
            .copied()
            .find(|input| is_repeatable(*input));
        if !screen_changed
            && self.page_repeat.sample(held, pressed)
            && let Some(input) = held.filter(|input| !frame.pressed.contains(input))
        {
            let screen = self.controller.screen;
            push_action(&mut actions, self.controller.handle(input, &self.catalog));
            if self.controller.screen != screen {
                self.page_repeat.reset();
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

    /// Apply controller input and pointer input in the same frame.
    ///
    /// HBC handles a pointer activation before Minus/Plus page navigation. A
    /// screen transition from controller input also ends the C frame, so a
    /// pointer action cannot run against the newly opened screen.
    pub fn update_frame_with_pointer(
        &mut self,
        frame: &mut InputFrame,
        target: Option<PointerTarget>,
        pointer_y: Option<i16>,
        pointer_pressed: bool,
        pointer_held: bool,
    ) -> Vec<Action> {
        if pointer_pressed && target.is_some() {
            frame.pressed.retain(|input| {
                !matches!(
                    input,
                    Input::Confirm | Input::PreviousPage | Input::NextPage
                )
            });
            frame.held.retain(|input| !is_repeatable(*input));
        }
        let screen_before = self.controller.screen;
        let mut actions = self.update(frame);
        if self.controller.screen != screen_before {
            return actions;
        }
        actions.extend(self.update_pointer_frame(target, pointer_y, pointer_pressed, pointer_held));
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
    fn right_stick_focus_input_is_suppressed_only_when_memo_scroll_can_consume_it() {
        let mut runtime = AppRuntime::new(vec![app("one"), app("two")]);
        runtime.update(&InputFrame {
            pressed: vec![Input::Confirm],
            ..InputFrame::default()
        });
        runtime.set_measured_memo(4, 1);
        runtime.update(&InputFrame {
            stick_scroll_pixels: -2,
            ..InputFrame::default()
        });
        assert_eq!(runtime.controller().detail_description_scroll(), 2);

        runtime.update(&InputFrame {
            pressed: vec![Input::Up],
            stick_navigation: vec![Input::Up],
            stick_scroll_pixels: 2,
            ..InputFrame::default()
        });
        assert_eq!(runtime.controller().detail_description_scroll(), 0);
        runtime.update(&InputFrame {
            pressed: vec![Input::Up],
            stick_navigation: vec![Input::Up],
            stick_scroll_pixels: 2,
            ..InputFrame::default()
        });
        assert_eq!(runtime.controller().detail_description_scroll(), 0);
        runtime.update(&InputFrame {
            pressed: vec![Input::Down],
            stick_navigation: vec![Input::Down],
            stick_scroll_pixels: -2,
            ..InputFrame::default()
        });
        assert_eq!(runtime.controller().detail_description_scroll(), 2);

        runtime.update(&InputFrame {
            pressed: vec![Input::Up],
            stick_navigation: vec![Input::Up],
            stick_scroll_pixels: 1,
            ..InputFrame::default()
        });
        assert_eq!(runtime.controller().detail_description_scroll(), 1);

        runtime.update(&InputFrame {
            pressed: vec![Input::Down],
            stick_navigation: vec![Input::Down],
            stick_scroll_pixels: -1,
            ..InputFrame::default()
        });
        assert_eq!(runtime.controller().detail_description_scroll(), 2);

        runtime.update(&InputFrame {
            pressed: vec![Input::Back],
            ..InputFrame::default()
        });
        runtime.update(&InputFrame {
            pressed: vec![Input::Down],
            stick_navigation: vec![Input::Down],
            stick_scroll_pixels: 10,
            ..InputFrame::default()
        });
        assert_eq!(runtime.controller().selected_app.as_deref(), Some("two"));
        runtime.update(&InputFrame {
            pressed: vec![Input::Up],
            stick_scroll_pixels: 10,
            ..InputFrame::default()
        });
        assert_eq!(runtime.controller().selected_app.as_deref(), Some("one"));
    }

    #[test]
    fn the_input_that_opens_a_screen_consumes_the_rest_of_its_frame() {
        let mut runtime = AppRuntime::new(vec![app("one")]);
        runtime.update(&InputFrame {
            pressed: vec![Input::Home, Input::Confirm],
            ..InputFrame::default()
        });
        assert_eq!(runtime.controller().screen, crate::app::Screen::MainMenu);
    }

    #[test]
    fn pointer_activation_consumes_simultaneous_page_navigation() {
        let catalog = (0..10)
            .map(|index| app(&alloc::format!("app-{index}")))
            .collect();
        let mut runtime = AppRuntime::new(catalog);
        let mut frame = InputFrame {
            pressed: vec![Input::NextPage, Input::Confirm],
            held: vec![Input::NextPage],
            ..InputFrame::default()
        };

        let actions = runtime.update_frame_with_pointer(
            &mut frame,
            Some(PointerTarget::App(0)),
            None,
            true,
            false,
        );

        assert!(actions.is_empty());
        assert_eq!(runtime.controller().screen, crate::app::Screen::AppDetails);
        assert_eq!(runtime.controller().selected_app.as_deref(), Some("app-0"));
    }

    #[test]
    fn pointer_press_without_a_hit_target_keeps_page_navigation() {
        let catalog = (0..10)
            .map(|index| app(&alloc::format!("app-{index}")))
            .collect();
        let mut runtime = AppRuntime::new(catalog);
        let mut frame = InputFrame {
            pressed: vec![Input::NextPage],
            held: vec![Input::NextPage],
            ..InputFrame::default()
        };

        runtime.update_frame_with_pointer(&mut frame, None, None, true, false);

        assert_eq!(runtime.controller().selected_app.as_deref(), Some("app-5"));
    }

    #[test]
    fn controller_screen_transition_consumes_pointer_activation() {
        let mut runtime = AppRuntime::new(vec![app("one")]);
        let mut frame = InputFrame {
            pressed: vec![Input::Home],
            ..InputFrame::default()
        };

        runtime.update_frame_with_pointer(
            &mut frame,
            Some(PointerTarget::App(0)),
            None,
            true,
            false,
        );

        assert_eq!(runtime.controller().screen, crate::app::Screen::MainMenu);
        assert_eq!(runtime.controller().pointer_target(), None);
    }

    #[test]
    fn back_transition_takes_priority_over_pointer_activation() {
        let mut runtime = AppRuntime::new(vec![app("one")]);
        runtime.update(&InputFrame {
            pressed: vec![Input::Confirm],
            ..InputFrame::default()
        });
        assert_eq!(runtime.controller().screen, crate::app::Screen::AppDetails);

        let mut frame = InputFrame {
            pressed: vec![Input::Back],
            ..InputFrame::default()
        };
        runtime.update_frame_with_pointer(
            &mut frame,
            Some(PointerTarget::App(0)),
            None,
            true,
            false,
        );

        assert_eq!(runtime.controller().screen, crate::app::Screen::Browser);
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
    fn save_error_remains_visible_while_the_save_button_is_held() {
        let mut runtime = AppRuntime::new(vec![app("one")]);
        runtime.controller.screen = crate::app::Screen::NetworkSettings;
        assert_eq!(
            runtime.update_pointer(Some(PointerTarget::NetworkOption(3)), true),
            vec![Action::SaveNetworkConfig]
        );
        runtime.show_notice_with_kind("NAND write failed", crate::app::NoticeKind::Error);
        for _ in 0..60 {
            assert!(
                runtime
                    .update(&InputFrame {
                        held: vec![Input::Confirm],
                        ..InputFrame::default()
                    })
                    .is_empty()
            );
        }
        assert_eq!(runtime.controller().screen, crate::app::Screen::Notice);
        assert_eq!(runtime.controller().notice(), Some("NAND write failed"));
        runtime.update(&InputFrame {
            pressed: vec![Input::Confirm],
            ..InputFrame::default()
        });
        assert_eq!(
            runtime.controller().screen,
            crate::app::Screen::NetworkSettings
        );
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
        runtime.update(&InputFrame {
            pressed: vec![Input::OpenOptions],
            ..InputFrame::default()
        });
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
    fn options_dialog_commits_sort_without_selecting_device_indicators() {
        let mut runtime = AppRuntime::new(Vec::new());
        runtime.load_settings(UserSettings {
            device: 0,
            sort_order: 0,
            ..UserSettings::default()
        });
        runtime.set_device_availability([true, true, false, false, false]);
        runtime.update(&InputFrame {
            pressed: vec![Input::OpenOptions],
            ..InputFrame::default()
        });
        let actions = runtime.update(&InputFrame {
            pressed: vec![
                Input::Left, // Back -> OK
                Input::Left, // OK -> Date
                Input::Confirm,
                Input::Right, // Date -> OK
                Input::Confirm,
            ],
            ..InputFrame::default()
        });
        assert_eq!(actions, vec![Action::ApplyOptions]);
        assert_eq!(runtime.settings().current().device, 0);
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
    #[test]
    fn exit_preserves_unreadable_settings_without_blocking_launch() {
        let mut runtime = AppRuntime::default();
        runtime.set_preferred_device(1);
        runtime.set_settings_persistence_enabled(false);
        assert_eq!(
            runtime.prepare_settings_for_exit(|_| -> Result<(), i32> {
                panic!("unreadable settings must not be overwritten")
            }),
            Ok(ExitSettingsOutcome::PreservedAfterLoadFailure)
        );
        assert!(runtime.settings().is_dirty());
    }

    #[test]
    fn exit_returns_write_failure_and_retries_dirty_settings() {
        let mut runtime = AppRuntime::default();
        runtime.set_preferred_device(1);
        assert_eq!(runtime.prepare_settings_for_exit(|_| Err(-42)), Err(-42));
        assert!(runtime.settings().is_dirty());
        let mut written = None;
        assert_eq!(
            runtime.prepare_settings_for_exit(|settings| {
                written = Some(settings.clone());
                Ok::<(), i32>(())
            }),
            Ok(ExitSettingsOutcome::Saved)
        );
        assert_eq!(written.unwrap().device, 1);
        assert!(!runtime.settings().is_dirty());
        assert_eq!(
            runtime.prepare_settings_for_exit(|_| -> Result<(), i32> {
                panic!("clean settings do not require a write")
            }),
            Ok(ExitSettingsOutcome::Unchanged)
        );
    }
}
