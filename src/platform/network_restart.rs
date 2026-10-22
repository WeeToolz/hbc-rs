//! Shared receiver restart guards and final connection results.
use crate::wiiload::ReceiverStatus;

const CONNECTION_TIMEOUT_MS: u64 = 60_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RestartError<E> {
    UploadPending,
    Stop(E),
    Start(E),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionError {
    Receiver(ReceiverStatus),
    Timeout,
}

#[derive(Clone, Copy, Debug)]
pub struct ConnectionCheck {
    started_ms: u64,
}

impl ConnectionCheck {
    pub fn new(started_ms: u64) -> Self {
        Self { started_ms }
    }

    /// A submitted worker is pending until it listens with an IP address.
    pub fn poll(
        &self,
        status: ReceiverStatus,
        address: Option<[u8; 4]>,
        now_ms: u64,
    ) -> Option<Result<[u8; 4], ConnectionError>> {
        match status {
            ReceiverStatus::NetworkUnavailable
            | ReceiverStatus::ListenerError
            | ReceiverStatus::CleanupError
            | ReceiverStatus::Stopped => return Some(Err(ConnectionError::Receiver(status))),
            ReceiverStatus::Listening => {
                if let Some(address) = address {
                    return Some(Ok(address));
                }
            }
            _ => {}
        }
        (now_ms.saturating_sub(self.started_ms) >= CONNECTION_TIMEOUT_MS)
            .then_some(Err(ConnectionError::Timeout))
    }
}

#[cfg(any(target_arch = "powerpc", test))]
fn restart_with<E>(
    status: ReceiverStatus,
    stop: impl FnOnce() -> Result<(), E>,
    start: impl FnOnce() -> Result<(), E>,
) -> Result<(), RestartError<E>> {
    if matches!(
        status,
        ReceiverStatus::Receiving | ReceiverStatus::PayloadReady
    ) {
        return Err(RestartError::UploadPending);
    }
    stop().map_err(RestartError::Stop)?;
    start().map_err(RestartError::Start)
}

#[cfg(target_arch = "powerpc")]
pub fn now_ms() -> u64 {
    ogc_lwp::sys::now_ticks() / (ogc_lwp::sys::TICKS_PER_SECOND / 1000)
}

#[cfg(target_arch = "powerpc")]
pub fn start() -> Result<ConnectionCheck, super::wiiload::ReceiverError> {
    super::wiiload::start()?;
    Ok(ConnectionCheck::new(now_ms()))
}

#[cfg(target_arch = "powerpc")]
pub fn restart() -> Result<ConnectionCheck, RestartError<super::wiiload::ReceiverError>> {
    restart_with(
        super::wiiload::status(),
        super::wiiload::stop,
        super::wiiload::start,
    )?;
    Ok(ConnectionCheck::new(now_ms()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::RefCell;

    #[test]
    fn upload_guard_preserves_active_and_retained_payloads() {
        for status in [ReceiverStatus::Receiving, ReceiverStatus::PayloadReady] {
            assert_eq!(
                restart_with(
                    status,
                    || -> Result<(), i32> { panic!("must preserve upload") },
                    || -> Result<(), i32> { panic!("must not start") }
                ),
                Err(RestartError::UploadPending)
            );
        }
    }

    #[test]
    fn restart_returns_each_backend_failure_and_stops_in_order() {
        let calls = RefCell::new(alloc::vec::Vec::new());
        for (stop, start, expected) in [
            (Err(-7), Ok(()), Err(RestartError::Stop(-7))),
            (Ok(()), Err(-9), Err(RestartError::Start(-9))),
            (Ok(()), Ok(()), Ok(())),
        ] {
            calls.borrow_mut().clear();
            let result = restart_with(
                ReceiverStatus::Listening,
                || {
                    calls.borrow_mut().push("stop");
                    stop
                },
                || {
                    calls.borrow_mut().push("start");
                    start
                },
            );
            assert_eq!(result, expected);
            assert_eq!(
                *calls.borrow(),
                if stop.is_err() {
                    alloc::vec!["stop"]
                } else {
                    alloc::vec!["stop", "start"]
                }
            );
        }
    }

    #[test]
    fn submission_stale_ip_and_timeout_are_not_connection_success() {
        let check = ConnectionCheck::new(100);
        assert_eq!(
            check.poll(ReceiverStatus::Starting, Some([10, 0, 0, 2]), 100),
            None
        );
        assert_eq!(check.poll(ReceiverStatus::Listening, None, 60_099), None);
        assert_eq!(
            check.poll(ReceiverStatus::Listening, None, 60_100),
            Some(Err(ConnectionError::Timeout))
        );
        assert_eq!(
            check.poll(ReceiverStatus::Listening, Some([10, 0, 0, 3]), 101),
            Some(Ok([10, 0, 0, 3]))
        );
        for status in [
            ReceiverStatus::NetworkUnavailable,
            ReceiverStatus::ListenerError,
            ReceiverStatus::CleanupError,
            ReceiverStatus::Stopped,
        ] {
            assert_eq!(
                check.poll(status, Some([10, 0, 0, 2]), 102),
                Some(Err(ConnectionError::Receiver(status)))
            );
        }
    }
}
