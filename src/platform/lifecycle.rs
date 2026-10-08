//! Deferred system exit requests from Wii hardware callbacks.

use core::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExitRequest {
    ReturnToMenu,
    PowerOff,
}

#[derive(Debug, Default)]
pub struct ExitSignal(AtomicU8);

impl ExitSignal {
    const RESET: u8 = 1;
    const POWER: u8 = 2;

    pub const fn new() -> Self {
        Self(AtomicU8::new(0))
    }

    pub fn request_reset(&self) {
        self.0.fetch_or(Self::RESET, Ordering::Release);
    }

    pub fn request_power_off(&self) {
        self.0.fetch_or(Self::POWER, Ordering::Release);
    }

    pub fn take(&self) -> Option<ExitRequest> {
        match self.0.swap(0, Ordering::AcqRel) {
            value if value & Self::POWER != 0 => Some(ExitRequest::PowerOff),
            value if value & Self::RESET != 0 => Some(ExitRequest::ReturnToMenu),
            _ => None,
        }
    }
}

static EXIT_SIGNAL: ExitSignal = ExitSignal::new();

pub fn defer_exit(pending: &mut Option<ExitRequest>, request: ExitRequest) {
    if *pending != Some(ExitRequest::PowerOff) {
        *pending = Some(request);
    }
}

pub fn take_ready_exit(
    pending: &mut Option<ExitRequest>,
    storage_busy: bool,
) -> Option<ExitRequest> {
    if storage_busy {
        None
    } else {
        pending.take()
    }
}

/// C HBC records controller cleanup errors and still performs the requested
/// system reset. Call only after persistent writes, storage and network cleanup
/// have completed. This policy does not apply to an application handoff.
pub fn reset_after_controller_cleanup<E, R>(
    cleanup: Result<(), E>,
    report: impl FnOnce(E),
    reset: impl FnOnce() -> R,
) -> R {
    if let Err(error) = cleanup {
        report(error);
    }
    reset()
}

#[cfg(target_arch = "powerpc")]
pub fn register_callbacks() {
    ogc_system::set_reset_callback(Some(reset_callback));
    ogc_system::set_power_callback(Some(power_callback));
}

#[cfg(not(target_arch = "powerpc"))]
pub fn register_callbacks() {}

pub fn take_request() -> Option<ExitRequest> {
    EXIT_SIGNAL.take()
}

#[cfg(target_arch = "powerpc")]
unsafe extern "C" fn reset_callback(_irq: u32, _context: *mut core::ffi::c_void) {
    EXIT_SIGNAL.request_reset();
}

#[cfg(target_arch = "powerpc")]
unsafe extern "C" fn power_callback() {
    EXIT_SIGNAL.request_power_off();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_waits_for_storage_completion_and_preserves_power_priority() {
        let mut pending = None;
        defer_exit(&mut pending, ExitRequest::PowerOff);
        defer_exit(&mut pending, ExitRequest::ReturnToMenu);
        assert_eq!(take_ready_exit(&mut pending, true), None);
        assert_eq!(pending, Some(ExitRequest::PowerOff));
        assert_eq!(
            take_ready_exit(&mut pending, false),
            Some(ExitRequest::PowerOff)
        );
        assert_eq!(pending, None);
    }

    #[test]
    fn failed_controller_close_is_recorded_before_the_real_reset_boundary() {
        let events = core::cell::RefCell::new(alloc::vec::Vec::new());
        let outcome = reset_after_controller_cleanup(
            Err(-116),
            |error| events.borrow_mut().push(error),
            || {
                events.borrow_mut().push(1);
                "reset completed"
            },
        );
        assert_eq!(*events.borrow(), [-116, 1]);
        assert_eq!(outcome, "reset completed");
    }

    #[test]
    fn exit_callbacks_only_queue_work_for_the_main_loop() {
        let signal = ExitSignal::new();
        assert_eq!(signal.take(), None);

        signal.request_reset();
        assert_eq!(signal.take(), Some(ExitRequest::ReturnToMenu));
        assert_eq!(signal.take(), None);

        signal.request_power_off();
        assert_eq!(signal.take(), Some(ExitRequest::PowerOff));
        assert_eq!(signal.take(), None);
    }

    #[test]
    fn power_off_takes_priority_when_both_callbacks_arrive() {
        let signal = ExitSignal::new();
        signal.request_reset();
        signal.request_power_off();
        assert_eq!(signal.take(), Some(ExitRequest::PowerOff));
    }
}
