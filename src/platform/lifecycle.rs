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
