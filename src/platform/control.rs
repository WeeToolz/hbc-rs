//! Hardware work for terminal commands. Called only by the HBC main thread.
use super::{startup::WiiStartup, video::VideoSystem};
use crate::remote_control::Command;
use alloc::{format, string::String};

pub enum Operation {
    Done(String),
    Eject,
    Search {
        connections: u32,
        failures: u32,
        authentication_failures: u32,
        hci_failures: u32,
        before_slots: u32,
    },
    Disconnect {
        slots: u32,
    },
    Network,
}
fn checked(code: i32, operation: &str) -> Result<(), String> {
    if code != 0 {
        Err(format!("{operation} failed ({code})"))
    } else {
        Ok(())
    }
}
fn connected_slots() -> u32 {
    (0..wpad_sys::WPAD_MAX_DEVICES).fold(0, |mask, slot| {
        if wpad_sys::bluetooth_hid_state(slot).connected {
            mask | (1 << slot)
        } else {
            mask
        }
    })
}
impl Operation {
    pub fn begin(
        command: Command,
        startup: &mut WiiStartup,
        video: &VideoSystem,
    ) -> Result<Self, String> {
        match command {
            Command::Save => {
                let outcome = startup.runtime_mut().save_settings_if_dirty(super::settings::save)
                    .map_err(|error| format!("Settings save failed: {error:?}"))?;
                Ok(Self::Done(if outcome { "Settings saved" } else { "Settings already match the loaded state; no write was required" }.into()))
            }
            Command::Screenshot => {
                let path = super::home_overlay::capture_and_save_screenshot(video, startup.has_storage(), |bytes| startup.save_screenshot_png(bytes))
                    .map_err(|error| format!("Screenshot failed: {error:?}"))?;
                Ok(Self::Done(format!("Screenshot saved: {path}")))
            }
            Command::Eject => {
                let initialized = ogc_di::init();
                if initialized < 0 { return Err(format!("Disc interface initialization failed ({initialized})")); }
                let mut cover = 0;
                checked(ogc_di::get_cover_register(&mut cover), "Disc presence read")?;
                if cover & ogc_di::DVD_COVER_DISC_INSERTED == 0 { return Err("No disc is inserted".into()); }
                checked(ogc_di::eject(), "Disc eject")?;
                Ok(Self::Eject)
            }
            Command::Sync | Command::FindRemotes => {
                let before = wpad_sys::bluetooth_diagnostics();
                if before.searching { return Err("A controller search is already active".into()); }
                checked(if command == Command::Sync { wpad_sys::start_pairing() } else { wpad_sys::search() }, "Controller search")?;
                Ok(Self::Search { connections: before.connections, failures: before.connection_failures,
                    authentication_failures: before.authentication_failures, hci_failures: before.hci_command_failures,
                    before_slots: connected_slots() })
            }
            Command::StopSync => Err("Unsupported: the SDK can request inquiry cancellation but does not expose its hardware completion. No cancellation was submitted".into()),
            Command::DisconnectRemotes => {
                let slots = connected_slots();
                if slots == 0 { return Err("No Bluetooth controllers are connected".into()); }
                for slot in 0..wpad_sys::WPAD_MAX_DEVICES {
                    if slots & (1 << slot) != 0 { checked(wpad_sys::disconnect(slot as i32), &format!("Disconnect controller {slot}"))?; }
                }
                Ok(Self::Disconnect { slots })
            }
            Command::RestartNetwork => {
                super::wiiload::stop().map_err(|error| format!("Network stop failed: {error:?}"))?;
                super::wiiload::start().map_err(|error| format!("Network start failed: {error:?}"))?;
                Ok(Self::Network)
            }
            Command::RestartApp | Command::ReturnToHbc => Err("Unsupported in the HBC browser. A running app must integrate its own control service; HBC cannot control an unrelated DOL after handoff".into()),
            Command::Restart | Command::Reset | Command::PowerOff => Err("System commands require the shared HBC exit path".into()),
        }
    }
    pub fn poll(&self) -> Option<Result<String, String>> {
        match self {
            Self::Done(message) => Some(Ok(message.clone())),
            Self::Eject => {
                let mut cover = 0;
                let result = ogc_di::get_cover_register(&mut cover);
                crate::remote_control::observe_eject(
                    result,
                    cover & ogc_di::DVD_COVER_DISC_INSERTED != 0,
                )
            }
            Self::Search {
                connections,
                failures,
                authentication_failures,
                hci_failures,
                before_slots,
            } => {
                let status = wpad_sys::bluetooth_diagnostics();
                if status.connection_failures != *failures
                    || status.authentication_failures != *authentication_failures
                    || status.hci_command_failures != *hci_failures
                    || status.failure.is_some()
                {
                    return Some(Err(format!("Controller search failed: {status:?}")));
                }
                if status.searching {
                    return None;
                }
                let added = status.connections.wrapping_sub(*connections);
                Some(if added > 0 && connected_slots() & !before_slots != 0 {
                    Ok(format!(
                        "Search completed; {added} controller connection(s) completed"
                    ))
                } else {
                    Err(
                        "Search ended without a new connected controller in an available slot"
                            .into(),
                    )
                })
            }
            Self::Disconnect { slots } => {
                crate::remote_control::observe_disconnect(*slots, connected_slots())
            }
            Self::Network => match super::wiiload::status() {
                crate::wiiload::ReceiverStatus::Listening
                    if super::wiiload::local_ip().is_some() =>
                {
                    Some(Ok(
                        "Network restarted; HBC is listening with an IP address".into()
                    ))
                }
                crate::wiiload::ReceiverStatus::NetworkUnavailable
                | crate::wiiload::ReceiverStatus::ListenerError
                | crate::wiiload::ReceiverStatus::CleanupError => Some(Err(format!(
                    "Network restart failed: {:?}",
                    super::wiiload::status()
                ))),
                _ => None,
            },
        }
    }
}
