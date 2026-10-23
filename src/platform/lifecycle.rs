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

#[cfg(all(target_arch = "powerpc", feature = "debugger-usb-gecko"))]
static DEBUGGER_ARMED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
#[cfg(all(target_arch = "powerpc", feature = "debugger-usb-gecko"))]
static DEBUGGER_BREAK: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

#[cfg(all(feature = "debugger-usb-gecko", feature = "debugger-network"))]
compile_error!("Select only one debugger mode: USB Gecko or network");

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DebuggerMode {
    #[default]
    Off,
    UsbGecko,
    Network,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(any(all(target_arch = "powerpc", feature = "debugger-usb-gecko"), test))]
enum ResetAction {
    ReturnToMenu,
    BreakIntoDebugger,
}

#[cfg(any(all(target_arch = "powerpc", feature = "debugger-usb-gecko"), test))]
fn reset_action(mode: DebuggerMode, debugger_armed: bool) -> ResetAction {
    if mode == DebuggerMode::UsbGecko && debugger_armed {
        ResetAction::BreakIntoDebugger
    } else {
        ResetAction::ReturnToMenu
    }
}

/// Return the debugger mode selected by Cargo features. No debugger is active
/// in the normal build.
pub const fn debugger_mode() -> DebuggerMode {
    #[cfg(feature = "debugger-usb-gecko")]
    return DebuggerMode::UsbGecko;

    #[cfg(all(not(feature = "debugger-usb-gecko"), feature = "debugger-network"))]
    return DebuggerMode::Network;

    #[cfg(not(any(feature = "debugger-usb-gecko", feature = "debugger-network")))]
    DebuggerMode::Off
}

/// Start the USB Gecko GDB stub when the adapter is present.
#[cfg(all(target_arch = "powerpc", feature = "debugger-usb-gecko"))]
pub fn initialize_debugger() -> bool {
    const USB_GECKO_CHANNEL: i32 = 1;
    if ogc_usbgecko::is_gecko_alive(USB_GECKO_CHANNEL) == 0 {
        ogc_system::report(format_args!(
            "HBC: debugger not started; no USB Gecko on channel 1\n"
        ));
        return false;
    }
    libdb_sys::debug_init(libdb_sys::GDBSTUB_DEVICE_USB, USB_GECKO_CHANNEL);
    DEBUGGER_ARMED.store(true, Ordering::Release);
    ogc_system::report(format_args!(
        "HBC: USB Gecko debugger ready; press Wii Reset to attach\n"
    ));
    true
}

#[cfg(not(all(target_arch = "powerpc", feature = "debugger-usb-gecko")))]
pub fn initialize_debugger() -> bool {
    false
}

#[cfg(all(target_arch = "powerpc", feature = "debugger-usb-gecko"))]
pub fn take_debugger_break() -> bool {
    DEBUGGER_BREAK.swap(false, Ordering::AcqRel)
}

#[cfg(not(all(target_arch = "powerpc", feature = "debugger-usb-gecko")))]
pub fn take_debugger_break() -> bool {
    false
}

pub fn defer_exit(pending: &mut Option<ExitRequest>, request: ExitRequest) {
    if *pending != Some(ExitRequest::PowerOff) {
        *pending = Some(request);
    }
}

pub fn take_ready_exit(
    pending: &mut Option<ExitRequest>,
    storage_busy: bool,
) -> Option<ExitRequest> {
    if storage_busy { None } else { pending.take() }
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

const APPLICATION_CLEANUP_ORDER: [ExitCleanupStep; 4] = [
    ExitCleanupStep::Storage,
    ExitCleanupStep::Disc,
    ExitCleanupStep::NetworkReceiver,
    ExitCleanupStep::Controllers,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Handoff {
    Application,
    SystemExit,
}

/// Stop services in the route's existing order, returning the first failure.
pub fn run_handoff_cleanup<E>(
    handoff: Handoff,
    mut cleanup: impl FnMut(ExitCleanupStep) -> Result<(), E>,
) -> Result<(), (ExitCleanupStep, E)> {
    let order = match handoff {
        Handoff::Application => APPLICATION_CLEANUP_ORDER,
        Handoff::SystemExit => EXIT_CLEANUP_ORDER,
    };
    for step in order {
        cleanup(step).map_err(|error| (step, error))?;
    }
    Ok(())
}

/// Preserve the system-exit API for callers that do not launch applications.
pub fn run_exit_cleanup<E>(
    cleanup: impl FnMut(ExitCleanupStep) -> Result<(), E>,
) -> Result<(), (ExitCleanupStep, E)> {
    run_handoff_cleanup(Handoff::SystemExit, cleanup)
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
    #[cfg(feature = "debugger-usb-gecko")]
    {
        if reset_action(debugger_mode(), DEBUGGER_ARMED.load(Ordering::Acquire))
            == ResetAction::BreakIntoDebugger
        {
            DEBUGGER_BREAK.store(true, Ordering::Release);
            return;
        }
    }
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
    fn debugger_mode_matches_the_build_features() {
        assert_eq!(
            debugger_mode(),
            if cfg!(feature = "debugger-usb-gecko") {
                DebuggerMode::UsbGecko
            } else if cfg!(feature = "debugger-network") {
                DebuggerMode::Network
            } else {
                DebuggerMode::Off
            }
        );
    }

    #[test]
    fn reset_returns_to_menu_until_the_selected_debugger_is_armed() {
        assert_eq!(
            reset_action(DebuggerMode::Network, false),
            ResetAction::ReturnToMenu
        );
        assert_eq!(
            reset_action(DebuggerMode::Network, true),
            ResetAction::ReturnToMenu
        );
        assert_eq!(
            reset_action(DebuggerMode::UsbGecko, true),
            ResetAction::BreakIntoDebugger
        );
        assert_eq!(
            reset_action(DebuggerMode::Off, true),
            ResetAction::ReturnToMenu
        );
    }

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
    fn application_cleanup_preserves_the_launch_order() {
        let mut events = alloc::vec::Vec::new();
        let result = run_handoff_cleanup(Handoff::Application, |step| {
            events.push(step);
            Ok::<(), ()>(())
        });
        assert_eq!(result, Ok(()));
        assert_eq!(
            events,
            [
                ExitCleanupStep::Storage,
                ExitCleanupStep::Disc,
                ExitCleanupStep::NetworkReceiver,
                ExitCleanupStep::Controllers,
            ]
        );
    }

    #[test]
    fn each_handoff_returns_each_backend_failure_without_running_later_steps() {
        let routes = [
            (
                Handoff::Application,
                [
                    ExitCleanupStep::Storage,
                    ExitCleanupStep::Disc,
                    ExitCleanupStep::NetworkReceiver,
                    ExitCleanupStep::Controllers,
                ],
            ),
            (
                Handoff::SystemExit,
                [
                    ExitCleanupStep::Disc,
                    ExitCleanupStep::NetworkReceiver,
                    ExitCleanupStep::Storage,
                    ExitCleanupStep::Controllers,
                ],
            ),
        ];
        for (handoff, expected_order) in routes {
            for (index, failed_step) in expected_order.iter().copied().enumerate() {
                let mut events = alloc::vec::Vec::new();
                let result = run_handoff_cleanup(handoff, |step| {
                    events.push(step);
                    if step == failed_step {
                        Err(-37)
                    } else {
                        Ok(())
                    }
                });
                assert_eq!(result, Err((failed_step, -37)));
                assert_eq!(events, expected_order[..=index]);
            }
        }
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
