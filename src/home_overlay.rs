//! Shared, hardware-independent control policy for the in-app HOME overlay.
//!
//! This module does not open a Wii overlay by itself. The application-side
//! video/input adapter must freeze the frame, render the menu and restore
//! application state before this policy can be used on the console.

use core::sync::atomic::{AtomicBool, Ordering};

/// One-shot request used by remote/developer transports to ask the app's main
/// loop to open the HOME overlay. Hardware callbacks and network workers only
/// set the flag; the application thread consumes it.
#[derive(Debug, Default)]
pub struct OpenSignal(AtomicBool);

impl OpenSignal {
    pub const fn new() -> Self {
        Self(AtomicBool::new(false))
    }
    pub fn request(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn take(&self) -> bool {
        self.0.swap(false, Ordering::AcqRel)
    }
}

/// User actions that the overlay may request from its application adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Request {
    Screenshot,
    Save,
    RestartApp,
    FindRemotes,
    DisconnectRemotes,
    Close,
    ReturnToHbc,
    ReturnToSystemMenu,
    RestartConsole,
    PowerOff,
}

/// The adapter must report the result of each operation, including failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationResult {
    Succeeded,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverlayError {
    AlreadyOpen,
    NotOpen,
    Busy,
    NoPendingOperation,
    MismatchedOperation,
}

/// A small control state machine shared by the overlay user interfaces.
/// All hardware actions remain the responsibility of the calling adapter.
#[derive(Default, Debug)]
pub struct OverlayControl {
    open: bool,
    pending: Option<Request>,
}

impl OverlayControl {
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open(&mut self) -> Result<(), OverlayError> {
        if self.open {
            return Err(OverlayError::AlreadyOpen);
        }
        self.open = true;
        self.pending = None;
        Ok(())
    }

    pub fn request(&mut self, request: Request) -> Result<Request, OverlayError> {
        if !self.open {
            return Err(OverlayError::NotOpen);
        }
        if self.pending.is_some() {
            return Err(OverlayError::Busy);
        }
        if request == Request::Close {
            self.open = false;
            return Ok(request);
        }
        self.pending = Some(request);
        Ok(request)
    }

    pub fn complete(
        &mut self,
        request: Request,
        result: OperationResult,
    ) -> Result<(), OverlayError> {
        match self.pending {
            None => Err(OverlayError::NoPendingOperation),
            Some(pending) if pending != request => Err(OverlayError::MismatchedOperation),
            Some(_) => {
                self.pending = None;
                if result == OperationResult::Succeeded
                    && matches!(
                        request,
                        Request::ReturnToHbc
                            | Request::ReturnToSystemMenu
                            | Request::RestartConsole
                            | Request::PowerOff
                            | Request::RestartApp
                    )
                {
                    self.open = false;
                }
                Ok(())
            }
        }
    }

    pub fn close(&mut self) -> Result<(), OverlayError> {
        if !self.open {
            return Err(OverlayError::NotOpen);
        }
        if self.pending.is_some() {
            return Err(OverlayError::Busy);
        }
        self.open = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn screenshot_failure_keeps_overlay_open_and_allows_retry() {
        let mut ui = OverlayControl::default();
        ui.open().unwrap();
        assert_eq!(ui.request(Request::Screenshot), Ok(Request::Screenshot));
        assert_eq!(ui.request(Request::Screenshot), Err(OverlayError::Busy));
        ui.complete(Request::Screenshot, OperationResult::Failed)
            .unwrap();
        assert!(ui.is_open());
        assert_eq!(ui.request(Request::Screenshot), Ok(Request::Screenshot));
        ui.complete(Request::Screenshot, OperationResult::Succeeded)
            .unwrap();
        assert!(ui.is_open());
    }
    #[test]
    fn exit_failure_cannot_claim_closed_overlay() {
        let mut ui = OverlayControl::default();
        ui.open().unwrap();
        ui.request(Request::ReturnToSystemMenu).unwrap();
        assert_eq!(
            ui.complete(Request::PowerOff, OperationResult::Succeeded),
            Err(OverlayError::MismatchedOperation)
        );
        assert_eq!(ui.close(), Err(OverlayError::Busy));
        ui.complete(Request::ReturnToSystemMenu, OperationResult::Failed)
            .unwrap();
        assert!(ui.is_open());
        ui.request(Request::ReturnToHbc).unwrap();
        ui.complete(Request::ReturnToHbc, OperationResult::Succeeded)
            .unwrap();
        assert!(!ui.is_open());
    }
    #[test]
    fn rejects_reentry_and_invalid_completion() {
        let mut ui = OverlayControl::default();
        assert_eq!(ui.request(Request::Save), Err(OverlayError::NotOpen));
        ui.open().unwrap();
        assert_eq!(ui.open(), Err(OverlayError::AlreadyOpen));
        assert_eq!(
            ui.complete(Request::Save, OperationResult::Succeeded),
            Err(OverlayError::NoPendingOperation)
        );
        ui.close().unwrap();
        assert_eq!(ui.close(), Err(OverlayError::NotOpen));
    }
    #[test]
    fn open_signal_is_one_shot() {
        let signal = OpenSignal::new();
        assert!(!signal.take());
        signal.request();
        signal.request();
        assert!(signal.take());
        assert!(!signal.take());
    }
}
