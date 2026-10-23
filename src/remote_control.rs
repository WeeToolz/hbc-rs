//! Main-thread system commands. Network workers never perform hardware actions.
use alloc::{string::String, vec::Vec};
use core::{cell::UnsafeCell, sync::atomic::AtomicBool};

pub const HEADER_LEN: usize = 16;
pub const RESPONSE_LEN: usize = 20;
pub const MAX_MESSAGE: usize = 512;
pub const QUEUE_LIFETIME_MS: u64 = 5000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Command {
    Restart = 1,
    Reset = 2,
    PowerOff = 3,
    Eject = 4,
    Sync = 5,
    Save = 6,
    Screenshot = 7,
    FindRemotes = 8,
    DisconnectRemotes = 9,
    StopSync = 10,
    RestartNetwork = 11,
    RestartApp = 12,
    ReturnToHbc = 13,
}
impl TryFrom<u8> for Command {
    type Error = ();
    fn try_from(value: u8) -> Result<Self, ()> {
        Ok(match value {
            1 => Self::Restart,
            2 => Self::Reset,
            3 => Self::PowerOff,
            4 => Self::Eject,
            5 => Self::Sync,
            6 => Self::Save,
            7 => Self::Screenshot,
            8 => Self::FindRemotes,
            9 => Self::DisconnectRemotes,
            10 => Self::StopSync,
            11 => Self::RestartNetwork,
            12 => Self::RestartApp,
            13 => Self::ReturnToHbc,
            _ => return Err(()),
        })
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum State {
    Queued = 0,
    Running = 1,
    Completed = 2,
    Failed = 3,
    Busy = 4,
    Invalid = 5,
    Unknown = 6,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Request {
    pub id: u32,
    pub command: Option<Command>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodeError {
    InvalidHeader,
    InvalidCommand,
    MissingRequestId,
}

impl Request {
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.len() != HEADER_LEN
            || bytes[4] != 1
            || bytes[6..8] != [0; 2]
            || bytes[12..16] != [0; 4]
        {
            return Err(DecodeError::InvalidHeader);
        }
        let command = match &bytes[..4] {
            b"HBCC" => Some(Command::try_from(bytes[5]).map_err(|_| DecodeError::InvalidCommand)?),
            b"HBCQ" if bytes[5] == 0 => None,
            _ => return Err(DecodeError::InvalidCommand),
        };
        let id = u32::from_be_bytes(
            bytes[8..12]
                .try_into()
                .map_err(|_| DecodeError::InvalidHeader)?,
        );
        if id == 0 && command.is_some() {
            return Err(DecodeError::MissingRequestId);
        }
        Ok(Self { id, command })
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Reply {
    pub id: u32,
    pub command: Option<Command>,
    pub state: State,
    pub message: String,
}
impl Reply {
    fn new(id: u32, command: Option<Command>, state: State, message: &str) -> Self {
        Self {
            id,
            command,
            state,
            message: crate::metadata::truncate_utf8_bytes(message, MAX_MESSAGE),
        }
    }
    pub fn encode(&self, session: u32) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(RESPONSE_LEN + self.message.len());
        bytes.extend_from_slice(b"HBCC");
        bytes.extend_from_slice(&[1, self.state as u8, self.command.map_or(0, |c| c as u8), 0]);
        bytes.extend_from_slice(&self.id.to_be_bytes());
        bytes.extend_from_slice(&session.to_be_bytes());
        bytes.extend_from_slice(&(self.message.len() as u16).to_be_bytes());
        bytes.extend_from_slice(&[0; 2]);
        bytes.extend_from_slice(self.message.as_bytes());
        bytes
    }
}
/// These observations come from production hardware reads. Submission alone
/// cannot satisfy a completion check. Shared with host failure-injection tests.
pub fn observe_eject(code: i32, disc_inserted: bool) -> Option<Result<String, String>> {
    if code != 0 {
        Some(Err(alloc::format!("Disc presence read failed ({code})")))
    } else if disc_inserted {
        None
    } else {
        Some(Ok(String::from("Disc absence confirmed by the drive")))
    }
}
pub fn observe_disconnect(targets: u32, connected: u32) -> Option<Result<String, String>> {
    if connected & targets == 0 {
        Some(Ok(String::from("Requested controllers are disconnected")))
    } else {
        None
    }
}
struct Job {
    reply: Reply,
    queued_ms: u64,
}
pub struct Mailbox {
    locked: AtomicBool,
    job: UnsafeCell<Option<Job>>,
}
// All job reads and writes hold the nonblocking lock.
unsafe impl Sync for Mailbox {}
impl Default for Mailbox {
    fn default() -> Self {
        Self::new()
    }
}
impl Mailbox {
    pub const fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
            job: UnsafeCell::new(None),
        }
    }
    fn access<R>(&self, f: impl FnOnce(&mut Option<Job>) -> R) -> Option<R> {
        crate::wiiload::try_with_payload(&self.locked, || {
            // SAFETY: the shared lock grants exclusive access.
            f(unsafe { &mut *self.job.get() })
        })
    }
    pub fn request(&self, bytes: &[u8], now_ms: u64) -> Reply {
        let request = match Request::decode(bytes) {
            Ok(request) => request,
            Err(_) => return Reply::new(0, None, State::Invalid, "Invalid command packet"),
        };
        self.access(|slot| {
            if let Some(job) = slot.as_mut() {
                if job.reply.state == State::Queued
                    && now_ms.saturating_sub(job.queued_ms) >= QUEUE_LIFETIME_MS
                {
                    job.reply.state = State::Failed;
                    job.reply.message = String::from("Command expired before execution");
                }
                if request.id == job.reply.id || (request.id == 0 && request.command.is_none()) {
                    if request.command.is_some() && request.command != job.reply.command {
                        return Reply::new(
                            request.id,
                            request.command,
                            State::Invalid,
                            "Request ID reused for a different command",
                        );
                    }
                    return job.reply.clone();
                }
                if request.command.is_some()
                    && matches!(job.reply.state, State::Queued | State::Running)
                {
                    return Reply::new(
                        request.id,
                        request.command,
                        State::Busy,
                        "Another command is active",
                    );
                }
            }
            match request.command {
                None => Reply::new(
                    request.id,
                    None,
                    State::Unknown,
                    "No retained result for this request",
                ),
                Some(command) => {
                    let reply = Reply::new(
                        request.id,
                        Some(command),
                        State::Queued,
                        "Queued; operation has not completed",
                    );
                    *slot = Some(Job {
                        reply: reply.clone(),
                        queued_ms: now_ms,
                    });
                    reply
                }
            }
        })
        .unwrap_or_else(|| {
            Reply::new(
                request.id,
                request.command,
                State::Busy,
                "Command state is busy",
            )
        })
    }
    pub fn take(&self, now_ms: u64) -> Option<(u32, Command)> {
        self.access(|slot| {
            let job = slot.as_mut()?;
            if job.reply.state != State::Queued {
                return None;
            }
            if now_ms.saturating_sub(job.queued_ms) >= QUEUE_LIFETIME_MS {
                job.reply.state = State::Failed;
                job.reply.message = String::from("Command expired before execution");
                return None;
            }
            job.reply.state = State::Running;
            job.reply.message = String::from("Running; operation has not completed");
            Some((
                job.reply.id,
                job.reply
                    .command
                    .expect("queued commands have an operation"),
            ))
        })
        .flatten()
    }
    /// Returns false if the main loop must retry publishing the result.
    pub fn finish(&self, id: u32, result: &Result<String, String>) -> bool {
        self.access(|slot| {
            let Some(job) = slot.as_mut() else {
                return false;
            };
            if job.reply.id != id || job.reply.state != State::Running {
                return false;
            }
            let (state, message) = match result {
                Ok(message) => (State::Completed, message.as_str()),
                Err(message) => (State::Failed, message.as_str()),
            };
            job.reply = Reply::new(id, job.reply.command, state, message);
            true
        })
        .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const RESTART: [u8; 16] = [72, 66, 67, 67, 1, 1, 0, 0, 0, 0, 0, 7, 0, 0, 0, 0];
    const QUERY: [u8; 16] = [72, 66, 67, 81, 1, 0, 0, 0, 0, 0, 0, 7, 0, 0, 0, 0];
    #[test]
    fn encoded_replies_keep_complete_utf8_characters_within_the_byte_limit() {
        for (message, expected) in [
            (String::new(), String::new()),
            ("a".repeat(512), "a".repeat(512)),
            (alloc::format!("{}é", "a".repeat(511)), "a".repeat(511)),
            (
                alloc::format!("{}é!", "a".repeat(510)),
                alloc::format!("{}é", "a".repeat(510)),
            ),
            ("界".repeat(171), "界".repeat(170)),
        ] {
            let reply = Reply::new(7, Some(Command::Restart), State::Failed, &message);
            assert_eq!(reply.message, expected);
            let encoded = reply.encode(9);
            assert_eq!(&encoded[20..], expected.as_bytes());
            assert_eq!(
                u16::from_be_bytes([encoded[16], encoded[17]]) as usize,
                expected.len()
            );
        }
    }

    #[test]
    fn hardware_submission_does_not_replace_completion_or_hide_read_failure() {
        assert_eq!(observe_eject(0, true), None);
        assert_eq!(
            observe_eject(-6, false),
            Some(Err(String::from("Disc presence read failed (-6)")))
        );
        assert_eq!(
            observe_eject(1, false),
            Some(Err(String::from("Disc presence read failed (1)")))
        );
        assert_eq!(
            observe_eject(0, false),
            Some(Ok(String::from("Disc absence confirmed by the drive")))
        );
        assert_eq!(observe_disconnect(0b101, 0b111), None);
        assert_eq!(observe_disconnect(0b101, 0b100), None);
        assert_eq!(
            observe_disconnect(0b101, 0b010),
            Some(Ok(String::from("Requested controllers are disconnected")))
        );
    }
    #[test]
    fn validates_fixed_wire_values_and_reserved_fields() {
        assert_eq!(
            Request::decode(&RESTART),
            Ok(Request {
                id: 7,
                command: Some(Command::Restart)
            })
        );
        for (offset, value) in [(4, 2), (5, 255), (6, 1), (12, 1)] {
            let mut bad = RESTART;
            bad[offset] = value;
            let expected = if offset == 5 {
                DecodeError::InvalidCommand
            } else {
                DecodeError::InvalidHeader
            };
            assert_eq!(Request::decode(&bad), Err(expected));
        }
        assert!(Request::decode(&RESTART[..15]).is_err());
        assert_eq!(
            Reply::new(7, Some(Command::Restart), State::Failed, "IOS -6").encode(9),
            b"HBCC\x01\x03\x01\x00\x00\x00\x00\x07\x00\x00\x00\x09\x00\x06\x00\x00IOS -6"
        );
    }
    #[test]
    fn command_requires_identity_but_status_query_accepts_zero() {
        assert_eq!(
            Request::decode(b"HBCC\x01\x01\0\0\0\0\0\0\0\0\0\0"),
            Err(DecodeError::MissingRequestId),
        );
        assert_eq!(
            Request::decode(b"HBCQ\x01\0\0\0\0\0\0\0\0\0\0\0"),
            Ok(Request {
                id: 0,
                command: None
            }),
        );
    }

    #[test]
    fn submission_is_not_completion_and_duplicate_does_not_run_twice() {
        let mailbox = Mailbox::new();
        assert_eq!(mailbox.request(&RESTART, 0).state, State::Queued);
        assert_eq!(mailbox.take(1), Some((7, Command::Restart)));
        assert_eq!(mailbox.request(&RESTART, 2).state, State::Running);
        assert_eq!(mailbox.take(3), None);
        assert!(mailbox.finish(7, &Err(String::from("settings write failed -102"))));
        let reply = mailbox.request(&QUERY, 4);
        assert_eq!(reply.state, State::Failed);
        assert_eq!(reply.message, "settings write failed -102");
        assert_eq!(mailbox.request(&RESTART, 5), reply);
        assert_eq!(mailbox.take(6), None);
    }
    #[test]
    fn busy_expired_and_wrong_completions_do_not_change_active_operation() {
        let mailbox = Mailbox::new();
        mailbox.request(&RESTART, 0);
        let mut other = RESTART;
        other[11] = 8;
        assert_eq!(mailbox.request(&other, 1).state, State::Busy);
        assert_eq!(mailbox.take(QUEUE_LIFETIME_MS), None);
        assert_eq!(
            mailbox.request(&QUERY, QUEUE_LIFETIME_MS).state,
            State::Failed
        );
        mailbox.request(&other, QUEUE_LIFETIME_MS + 1);
        assert_eq!(
            mailbox.take(QUEUE_LIFETIME_MS + 2),
            Some((8, Command::Restart))
        );
        assert!(!mailbox.finish(7, &Ok(String::from("wrong"))));
        assert!(mailbox.finish(8, &Ok(String::from("real operation result"))));
        let mut query = QUERY;
        query[11] = 8;
        assert_eq!(
            mailbox.request(&query, QUEUE_LIFETIME_MS + 3).message,
            "real operation result"
        );
    }
}
