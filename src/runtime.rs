//! Hardware-independent frame orchestration for HBC application state.

use alloc::vec::Vec;

use crate::{
    app::{Action, AppController, Input},
    catalog::AppEntry,
    platform::input::InputFrame,
};

/// Owns the current catalog and applies platform input to the app controller.
/// Storage, drawing, and service actions remain at the platform edge.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AppRuntime {
    controller: AppController,
    catalog: Vec<AppEntry>,
}

impl AppRuntime {
    pub fn new(catalog: Vec<AppEntry>) -> Self {
        Self {
            controller: AppController::new(),
            catalog,
        }
    }

    pub fn controller(&self) -> &AppController {
        &self.controller
    }

    pub fn catalog(&self) -> &[AppEntry] {
        &self.catalog
    }

    /// Replace a device-scan snapshot. Selection is reconciled on the next
    /// input update so the controller remains the single policy owner.
    pub fn replace_catalog(&mut self, catalog: Vec<AppEntry>) {
        self.catalog = catalog;
    }

    /// Apply one input frame and return actions for storage or platform code.
    /// Held directional and page inputs repeat. A newly pressed input is not
    /// applied twice when the platform reports it as both pressed and held.
    pub fn update(&mut self, frame: &InputFrame) -> Vec<Action> {
        let mut actions = Vec::new();
        for &input in &frame.pressed {
            push_action(&mut actions, self.controller.handle(input, &self.catalog));
        }
        for &input in &frame.held {
            if is_repeatable(input) && !frame.pressed.contains(&input) {
                push_action(&mut actions, self.controller.handle(input, &self.catalog));
            }
        }
        actions
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
}
