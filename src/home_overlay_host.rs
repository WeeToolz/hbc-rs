//! Application-owned integration point for the HOME overlay.
//!
//! The host must provide its own RGBA frame copy, text drawing, presentation,
//! and system actions. No implicit GX/XFB state changes or global hooks occur.
use crate::home_overlay::{OperationResult, OverlayError, Request};
use crate::home_overlay_app::{
    AppHomeOverlay, Capabilities, DrawError, FrameAction, OverlayText, Slot,
};
use crate::platform::input::InputFrame;

/// A host must freeze/copy the displayed frame before allowing any overlay
/// painting. A failed presentation must not mark an operation successful.
pub trait OverlayHost {
    type Error;
    fn with_frame<R>(
        &mut self,
        draw: impl FnOnce(&mut [u8], usize, usize, usize, &mut dyn OverlayText) -> R,
    ) -> Result<R, Self::Error>;
    fn present(&mut self) -> Result<(), Self::Error>;
    fn execute(&mut self, request: Request) -> Result<(), Self::Error>;
}

/// Host for a synchronous application-side HOME loop, matching HBC Reborn's
/// `hbc_agent_home()` call pattern. All polling and waits remain app-owned.
pub trait OverlayLoopHost: OverlayHost {
    fn poll_overlay_input(&mut self) -> Result<InputFrame, Self::Error>;
    fn wait_overlay_frame(&mut self) -> Result<(), Self::Error>;
}

#[derive(Debug, PartialEq)]
pub enum HostFrameError<E> {
    Control(OverlayError),
    Buffer(DrawError),
    Host(E),
    Action { request: Request, error: E },
}

/// This owns only UI policy. Application code retains GX, framebuffer,
/// controller, filesystem and shutdown ownership. Do not invoke from IRQ code.
#[derive(Default)]
pub struct HomeOverlaySession {
    ui: AppHomeOverlay,
}

impl HomeOverlaySession {
    pub fn is_open(&self) -> bool {
        self.ui.is_open()
    }

    /// Application-supplied capabilities only; missing callbacks stay disabled.
    pub fn set_capabilities(&mut self, capabilities: Capabilities) {
        self.ui.set_capabilities(capabilities);
    }

    pub fn set_slot(&mut self, index: usize, slot: Option<Slot>) -> bool {
        self.ui.set_slot(index, slot)
    }

    pub fn open_programmatically(&mut self) -> Result<(), OverlayError> {
        self.ui.open_programmatically()
    }

    /// Run the overlay synchronously in the calling application's thread.
    /// A successful return means the menu closed normally; host failures are
    /// returned and never converted into success.
    pub fn run<H: OverlayLoopHost>(
        &mut self,
        host: &mut H,
    ) -> Result<(), HostFrameError<H::Error>> {
        self.open_programmatically()
            .map_err(HostFrameError::Control)?;
        while self.is_open() {
            let input = host.poll_overlay_input().map_err(HostFrameError::Host)?;
            self.frame(host, &input)?;
            if self.is_open() {
                host.wait_overlay_frame().map_err(HostFrameError::Host)?;
            }
        }
        Ok(())
    }

    /// Handle a controller frame and paint the overlay. Actions are completed
    /// only after the host confirms the operation. Failures leave the menu open.
    pub fn frame<H: OverlayHost>(
        &mut self,
        host: &mut H,
        input: &InputFrame,
    ) -> Result<(), HostFrameError<H::Error>> {
        let action = self
            .ui
            .tick_input_frame(input)
            .map_err(HostFrameError::Control)?;

        // Execute against the application's unmodified frame. In particular,
        // Screenshot must not capture the HOME overlay itself.
        if let FrameAction::Perform(request) = action {
            match host.execute(request) {
                Ok(()) => {
                    self.ui
                        .complete(request, OperationResult::Succeeded)
                        .map_err(HostFrameError::Control)?;
                }
                Err(error) => {
                    self.ui
                        .complete(request, OperationResult::Failed)
                        .map_err(HostFrameError::Control)?;
                    return Err(HostFrameError::Action { request, error });
                }
            }
        }

        if self.ui.is_open() {
            let draw_result = host
                .with_frame(|pixels, width, height, stride, text| {
                    self.ui.draw(pixels, width, height, stride)?;
                    struct TextRef<'a>(&'a mut dyn OverlayText);
                    impl OverlayText for TextRef<'_> {
                        fn draw_text(&mut self, label: &str, x: usize, y: usize, selected: bool) {
                            self.0.draw_text(label, x, y, selected);
                        }
                    }
                    self.ui.draw_labels(width, height, &mut TextRef(text));
                    Ok::<(), DrawError>(())
                })
                .map_err(HostFrameError::Host)?;
            draw_result.map_err(HostFrameError::Buffer)?;
            host.present().map_err(HostFrameError::Host)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::home_overlay_app::Key;

    #[test]
    fn state_is_closed_by_default() {
        assert!(!HomeOverlaySession::default().is_open());
        assert_eq!(
            crate::home_overlay_app::key_from_input(crate::app::Input::Home),
            Some(Key::Home)
        );
    }
    #[derive(Default)]
    struct FakeText;
    impl OverlayText for FakeText {
        fn draw_text(&mut self, _label: &str, _x: usize, _y: usize, _selected: bool) {}
    }

    struct FakeHost {
        events: alloc::vec::Vec<&'static str>,
        fail_action: bool,
        pixels: alloc::vec::Vec<u8>,
        text: FakeText,
    }
    impl FakeHost {
        fn new() -> Self {
            Self {
                events: alloc::vec::Vec::new(),
                fail_action: false,
                pixels: alloc::vec![0; 320*240*4],
                text: FakeText,
            }
        }
    }
    impl OverlayHost for FakeHost {
        type Error = &'static str;
        fn with_frame<R>(
            &mut self,
            draw: impl FnOnce(&mut [u8], usize, usize, usize, &mut dyn OverlayText) -> R,
        ) -> Result<R, Self::Error> {
            self.events.push("draw");
            Ok(draw(&mut self.pixels, 320, 240, 320 * 4, &mut self.text))
        }
        fn present(&mut self) -> Result<(), Self::Error> {
            self.events.push("present");
            Ok(())
        }
        fn execute(&mut self, _request: Request) -> Result<(), Self::Error> {
            self.events.push("execute");
            if self.fail_action {
                Err("action failed")
            } else {
                Ok(())
            }
        }
    }

    impl OverlayLoopHost for FakeHost {
        fn poll_overlay_input(&mut self) -> Result<InputFrame, Self::Error> {
            self.events.push("poll");
            Ok(frame_with(&[crate::app::Input::Back], &[]))
        }
        fn wait_overlay_frame(&mut self) -> Result<(), Self::Error> {
            self.events.push("wait");
            Ok(())
        }
    }

    fn frame_with(pressed: &[crate::app::Input], held: &[crate::app::Input]) -> InputFrame {
        InputFrame {
            pressed: pressed.to_vec(),
            held: held.to_vec(),
            ..InputFrame::default()
        }
    }

    #[test]
    fn screenshot_executes_before_overlay_is_drawn() {
        use crate::app::Input;
        let mut session = HomeOverlaySession::default();
        let mut host = FakeHost::new();
        session
            .frame(&mut host, &frame_with(&[Input::Home], &[Input::Home]))
            .unwrap();
        session.frame(&mut host, &frame_with(&[], &[])).unwrap();
        host.events.clear();
        session
            .frame(&mut host, &frame_with(&[Input::Confirm], &[]))
            .unwrap();
        assert_eq!(host.events, ["execute", "draw", "present"]);
    }

    #[test]
    fn action_failure_is_returned_and_menu_stays_open() {
        use crate::app::Input;
        let mut session = HomeOverlaySession::default();
        let mut host = FakeHost::new();
        session
            .frame(&mut host, &frame_with(&[Input::Home], &[Input::Home]))
            .unwrap();
        session.frame(&mut host, &frame_with(&[], &[])).unwrap();
        host.fail_action = true;
        let error = session
            .frame(&mut host, &frame_with(&[Input::Confirm], &[]))
            .unwrap_err();
        assert_eq!(
            error,
            HostFrameError::Action {
                request: Request::Screenshot,
                error: "action failed"
            }
        );
        assert!(session.is_open());
    }

    #[test]
    fn synchronous_loop_can_open_without_fake_home_input() {
        let mut session = HomeOverlaySession::default();
        let mut host = FakeHost::new();
        session.run(&mut host).unwrap();
        assert!(!session.is_open());
        assert!(host.events.contains(&"poll"));
    }
}
