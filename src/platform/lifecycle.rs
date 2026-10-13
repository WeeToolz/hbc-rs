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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExitCleanupStep {
    Storage,
    Disc,
    NetworkReceiver,
    Controllers,
}

const EXIT_CLEANUP_ORDER: [ExitCleanupStep; 4] = [
    ExitCleanupStep::Disc,
    ExitCleanupStep::NetworkReceiver,
    ExitCleanupStep::Storage,
    ExitCleanupStep::Controllers,
];

/// Run shared shutdown work in order and stop when a required step fails.
pub fn run_exit_cleanup<E>(
    mut cleanup: impl FnMut(ExitCleanupStep) -> Result<(), E>,
) -> Result<(), (ExitCleanupStep, E)> {
    for step in EXIT_CLEANUP_ORDER {
        cleanup(step).map_err(|error| (step, error))?;
    }
    Ok(())
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
    fn system_menu_and_power_off_share_ordered_cleanup() {
        let events = core::cell::RefCell::new(alloc::vec::Vec::new());
        let result = run_exit_cleanup(|step| {
            events.borrow_mut().push(step);
            Ok::<(), ()>(())
        });
        assert_eq!(result, Ok(()));
        assert_eq!(
            *events.borrow(),
            [
                ExitCleanupStep::Disc,
                ExitCleanupStep::NetworkReceiver,
                ExitCleanupStep::Storage,
                ExitCleanupStep::Controllers,
            ]
        );
    }

    #[test]
    fn system_exit_cleanup_stops_at_the_first_failed_step() {
        let events = core::cell::RefCell::new(alloc::vec::Vec::new());
        let result = run_exit_cleanup(|step| {
            events.borrow_mut().push(step);
            if step == ExitCleanupStep::Controllers {
                Err(-9)
            } else {
                Ok(())
            }
        });
        assert_eq!(result, Err((ExitCleanupStep::Controllers, -9)));
        assert_eq!(
            *events.borrow(),
            [
                ExitCleanupStep::Disc,
                ExitCleanupStep::NetworkReceiver,
                ExitCleanupStep::Storage,
                ExitCleanupStep::Controllers,
            ]
        );
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
