//! Wii TCP receiver for the HBC Wiiload protocol.
//!
//! The listener owns its network handles on an LWP worker. The main loop can
//! read status or take a completed payload without waiting for network I/O.

use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicU32, Ordering};

pub use crate::wiiload::ReceiverStatus;
use crate::wiiload::{
    IndicatorState, WIILOAD_CLIENT_TIMEOUT_TICKS, WIILOAD_HEADER_LEN, WIILOAD_TCP_PORT,
    WiiloadError, WiiloadHeader, WiiloadPayload, client_deadline_expired,
    indicator_state as map_indicator_state, same_local_subnet_16, socket_would_block,
};
#[cfg(feature = "debugger-network")]
use libdb_sys::wii_network::CooperativeControl;
use ogc_lwp::{ThreadHandle, sys as lwp_sys};
use ogc_network::sys::{F_GETFL, F_SETFL};
use ogc_network::{self as network, AF_INET, O_NONBLOCK, PF_INET, SOCK_STREAM, SockaddrIn};
use ogc_usbgecko as usbgecko;

const LISTEN_BACKLOG: u32 = 3;
const THREAD_STACK_SIZE: usize = crate::wiiload::WIILOAD_WORKER_STACK_BYTES;
const THREAD_PRIORITY: u8 = 48;
const NETWORK_INIT_RETRIES: usize = 32;
const READ_IDLE_POLLS: usize = 500;
const POLL_TICKS: u64 = lwp_sys::TICKS_PER_SECOND / 50;
const EAGAIN: i32 = -11;
const ETIMEDOUT: i32 = -110;
const USBGECKO_CHANNEL: i32 = 1;
const USBGECKO_RETRIES: i32 = 1000;
const GECKO_IDLE_POLLS: usize = 6;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiverError {
    AlreadyStarted,
    ThreadCreation,
    WorkerJoin(i32),
    Cleanup(i32),
}

struct Shared {
    stopped: AtomicBool,
    status: AtomicU8,
    gecko_ready: AtomicBool,
    receive_completed: AtomicU32,
    receive_total: AtomicU32,
    cleanup_error: AtomicI32,
    payload_lock: AtomicBool,
    payload: UnsafeCell<Option<WiiloadPayload>>,
}

// `payload` is accessed only while `payload_lock` is held.
unsafe impl Sync for Shared {}

impl Shared {
    const fn new() -> Self {
        Self {
            stopped: AtomicBool::new(false),
            status: AtomicU8::new(ReceiverStatus::Stopped as u8),
            gecko_ready: AtomicBool::new(false),
            receive_completed: AtomicU32::new(0),
            receive_total: AtomicU32::new(0),
            cleanup_error: AtomicI32::new(0),
            payload_lock: AtomicBool::new(false),
            payload: UnsafeCell::new(None),
        }
    }

    fn lock_payload(&self) -> PayloadGuard<'_> {
        while !crate::wiiload::try_claim_payload(&self.payload_lock) {
            core::hint::spin_loop();
        }
        PayloadGuard { shared: self }
    }

    fn set_status(&self, status: ReceiverStatus) {
        self.status.store(status as u8, Ordering::Release);
    }

    fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }
}

struct PayloadGuard<'a> {
    shared: &'a Shared,
}

impl Drop for PayloadGuard<'_> {
    fn drop(&mut self) {
        self.shared.payload_lock.store(false, Ordering::Release);
    }
}

static SHARED: Shared = Shared::new();
pub static REMOTE_CONTROL: crate::remote_control::Mailbox = crate::remote_control::Mailbox::new();
static CONTROL_SESSION: AtomicU32 = AtomicU32::new(0);

static REMOTE_INPUT: crate::remote_input::Mailbox = crate::remote_input::Mailbox::new();

#[cfg(feature = "debugger-network")]
pub static COOPERATIVE_CONTROL: CooperativeControl = CooperativeControl::new();

pub fn merge_remote_input(
    state: &mut crate::remote_input::RemoteState,
    frame: &mut crate::platform::input::InputFrame,
) {
    REMOTE_INPUT.consume(state, remote_now_ms(), frame);
}

fn remote_now_ms() -> u64 {
    lwp_sys::now_ticks() / (lwp_sys::TICKS_PER_SECOND / 1000)
}

static STARTED: AtomicBool = AtomicBool::new(false);
static mut THREAD: lwp_sys::LwpThread = lwp_sys::LWP_THREAD_NULL;

#[repr(align(32))]
struct ThreadStack([u8; THREAD_STACK_SIZE]);
static mut THREAD_STACK: ThreadStack = ThreadStack([0; THREAD_STACK_SIZE]);

/// Start the singleton listener. Call this after the LWP system is initialized.
pub fn start() -> Result<(), ReceiverError> {
    if STARTED.load(Ordering::Acquire) {
        match status() {
            ReceiverStatus::NetworkUnavailable | ReceiverStatus::ListenerError => {
                // A terminal worker can be joined and restarted by this control thread.
                // SAFETY: the terminal status is published only after worker_main exits.
                let joined = unsafe { ThreadHandle::new(THREAD) }.join().map(|_| ());
                crate::wiiload::complete_worker_join(&STARTED, joined)
                    .map_err(ReceiverError::WorkerJoin)?;
            }
            ReceiverStatus::CleanupError => {
                return Err(ReceiverError::Cleanup(cleanup_error().unwrap_or(-1)));
            }
            _ => return Err(ReceiverError::AlreadyStarted),
        }
    }
    if let Some(error) = cleanup_error() {
        return Err(ReceiverError::Cleanup(error));
    }
    if STARTED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err(ReceiverError::AlreadyStarted);
    }
    #[cfg(feature = "debugger-network")]
    COOPERATIVE_CONTROL.reset_for_worker_start();
    if CONTROL_SESSION.load(Ordering::Acquire) == 0 {
        CONTROL_SESSION.store((remote_now_ms() as u32).max(1), Ordering::Release);
    }
    SHARED.stopped.store(false, Ordering::Release);
    SHARED.set_status(ReceiverStatus::Starting);
    SHARED.gecko_ready.store(false, Ordering::Release);
    SHARED.cleanup_error.store(0, Ordering::Release);
    // SAFETY: SHARED and the aligned static stack have process lifetime.
    let result = unsafe {
        ThreadHandle::create(
            &mut *core::ptr::addr_of_mut!(THREAD),
            Some(worker),
            (&SHARED as *const Shared).cast_mut().cast::<c_void>(),
            core::ptr::addr_of_mut!(THREAD_STACK.0).cast::<c_void>(),
            THREAD_STACK_SIZE as u32,
            THREAD_PRIORITY,
        )
    };
    if result.is_err() {
        SHARED.set_status(ReceiverStatus::Stopped);
        STARTED.store(false, Ordering::Release);
        #[cfg(feature = "debugger-network")]
        COOPERATIVE_CONTROL.worker_stopped();
        return Err(ReceiverError::ThreadCreation);
    }
    Ok(())
}

/// Read the listener state without blocking.
pub fn status() -> ReceiverStatus {
    match SHARED.status.load(Ordering::Acquire) {
        1 => ReceiverStatus::Starting,
        2 => ReceiverStatus::NetworkUnavailable,
        3 => ReceiverStatus::Listening,
        4 => ReceiverStatus::Receiving,
        5 => ReceiverStatus::PayloadReady,
        6 => ReceiverStatus::InvalidUpload,
        7 => ReceiverStatus::ListenerError,
        8 => ReceiverStatus::MemoryError,
        9 => ReceiverStatus::CleanupError,
        _ => ReceiverStatus::Stopped,
    }
}

/// Return browser indicator states with atomic reads only.
pub fn indicator_state() -> IndicatorState {
    map_indicator_state(status(), SHARED.gecko_ready.load(Ordering::Acquire))
}

/// Return the current TCP upload receive counts. The total is zero until the
/// header has been decoded and after the upload body has been received.
pub fn receive_progress() -> (u32, u32) {
    (
        SHARED.receive_completed.load(Ordering::Acquire),
        SHARED.receive_total.load(Ordering::Acquire),
    )
}

pub fn cleanup_error() -> Option<i32> {
    let error = SHARED.cleanup_error.load(Ordering::Acquire);
    (error < 0).then_some(error)
}

/// Return the Wii's assigned IPv4 address while the receiver has a live socket stack.
pub fn local_ip() -> Option<[u8; 4]> {
    if !matches!(
        status(),
        ReceiverStatus::Listening | ReceiverStatus::Receiving
    ) {
        return None;
    }
    let address = network::get_host_ip();
    if address == 0 {
        return None;
    }
    Some([
        network::ip4_addr1(address) as u8,
        network::ip4_addr2(address) as u8,
        network::ip4_addr3(address) as u8,
        network::ip4_addr4(address) as u8,
    ])
}

/// Take ownership of one completed payload. The worker can publish another after this call.
pub fn take_payload() -> Option<WiiloadPayload> {
    // A busy producer runs at lower priority. Return to the frame loop so it
    // can finish publishing instead of spinning and preventing it from running.
    crate::wiiload::try_with_payload(&SHARED.payload_lock, || {
        // SAFETY: try_with_payload holds the same lock as the producer.
        let payload = unsafe { (&mut *SHARED.payload.get()).take() };
        if payload.is_some() && !SHARED.is_stopped() {
            SHARED.set_status(ReceiverStatus::Listening);
        }
        payload
    })?
}

/// Ask the worker to stop and wait for it to close its listener and network stack.
pub fn stop() -> Result<(), ReceiverError> {
    if !STARTED.load(Ordering::Acquire) {
        return cleanup_error().map_or(Ok(()), |error| Err(ReceiverError::Cleanup(error)));
    }
    SHARED.stopped.store(true, Ordering::Release);
    // SAFETY: THREAD is written once by start and read only for this join.
    let joined = unsafe { ThreadHandle::new(THREAD) }.join().map(|_| ());
    crate::wiiload::complete_worker_join(&STARTED, joined).map_err(ReceiverError::WorkerJoin)?;
    SHARED.gecko_ready.store(false, Ordering::Release);
    if let Some(error) = cleanup_error() {
        SHARED.set_status(ReceiverStatus::CleanupError);
        return Err(ReceiverError::Cleanup(error));
    }
    SHARED.set_status(ReceiverStatus::Stopped);
    Ok(())
}

unsafe extern "C" fn worker(arg: *mut c_void) -> *mut c_void {
    // SAFETY: start passes the address of the static SHARED value.
    let shared = unsafe { &*arg.cast::<Shared>() };
    let result = worker_main(shared);
    #[cfg(feature = "debugger-network")]
    COOPERATIVE_CONTROL.worker_stopped();
    shared.gecko_ready.store(false, Ordering::Release);
    shared.set_status(result);
    core::ptr::null_mut()
}

fn worker_main(shared: &Shared) -> ReceiverStatus {
    if !initialize_network(shared) {
        let status = if shared.is_stopped() {
            ReceiverStatus::Stopped
        } else {
            ReceiverStatus::NetworkUnavailable
        };
        return finish_worker(shared, None, status);
    }
    let listener = match create_listener() {
        Ok(listener) => listener,
        Err(error) => {
            ogc_system::report(format_args!(
                "HBC: Wiiload {} failed: {}\n",
                error.stage, error.code
            ));
            if let Some(cleanup) = error.cleanup {
                record_cleanup_error(shared, cleanup);
            }
            return finish_worker(shared, None, ReceiverStatus::ListenerError);
        }
    };
    #[cfg(feature = "debugger-network")]
    let mut debugger_diagnostics = libdb_sys::wii_network::ListenerDiagnostics::new();
    #[cfg(feature = "debugger-network")]
    let mut debugger_transport =
        match libdb_sys::wii_network::WiiNetworkTransport::bind_with_control(
            libdb_sys::wii_network::DEFAULT_PORT,
            &COOPERATIVE_CONTROL,
        ) {
            Ok(transport) => {
                COOPERATIVE_CONTROL.set_transport_ready(true);
                debugger_diagnostics.listening();
                Some(transport)
            }
            Err(error) => {
                COOPERATIVE_CONTROL.set_transport_ready(false);
                debugger_diagnostics
                    .startup_failed(libdb_sys::wii_network::diagnostic_failure(&error));
                None
            }
        };
    let local_ip = network::get_host_ip();
    let gecko_present = usbgecko::is_gecko_alive(USBGECKO_CHANNEL) != 0;
    if gecko_present {
        usbgecko::flush(USBGECKO_CHANNEL);
    }
    shared.gecko_ready.store(gecko_present, Ordering::Release);
    shared.set_status(ReceiverStatus::Listening);
    let mut gecko = GeckoReceive::new();
    let mut terminal_status = ReceiverStatus::Stopped;
    #[cfg(feature = "debugger-network")]
    let mut debugger_poll_enabled = true;

    while !shared.is_stopped() {
        #[cfg(feature = "debugger-network")]
        if debugger_poll_enabled && let Some(transport) = debugger_transport.as_mut() {
            let now_ms = lwp_sys::now_ticks() / (lwp_sys::TICKS_PER_SECOND / 1000);
            match transport.poll(now_ms) {
                Ok(_) => {}
                Err(error) => {
                    COOPERATIVE_CONTROL.set_transport_ready(false);
                    debugger_diagnostics
                        .poll_failed(libdb_sys::wii_network::diagnostic_failure(&error));
                    close_debugger_transport(shared, transport, &mut debugger_diagnostics);
                    debugger_poll_enabled = false;
                }
            }
        }
        if has_pending_payload(shared) {
            sleep_poll();
            continue;
        }
        match gecko.poll(shared) {
            GeckoPoll::Complete(payload) => {
                let _guard = shared.lock_payload();
                let slot = unsafe { &mut *shared.payload.get() };
                if slot.is_none() {
                    *slot = Some(payload);
                    shared.set_status(ReceiverStatus::PayloadReady);
                    continue;
                }
            }
            GeckoPoll::Invalid => shared.set_status(ReceiverStatus::InvalidUpload),
            GeckoPoll::Memory => shared.set_status(ReceiverStatus::MemoryError),
            GeckoPoll::Pending => {
                shared.set_status(ReceiverStatus::Receiving);
                sleep_poll();
                continue;
            }
            GeckoPoll::Idle => {}
        }
        let mut peer = SockaddrIn::default();
        let client = network::accept_addr(listener, &mut peer);
        if socket_would_block(client) {
            sleep_poll();
            continue;
        }
        if client < 0 {
            terminal_status = ReceiverStatus::ListenerError;
            break;
        }
        if !same_local_subnet_16(peer.sin_addr.s_addr, local_ip) {
            if let Err(error) = close_socket(client) {
                record_cleanup_error(shared, error);
                terminal_status = ReceiverStatus::CleanupError;
                break;
            }
            continue;
        }
        let flags = network::fcntl(client, F_GETFL, 0);
        if flags < 0 || network::fcntl(client, F_SETFL, (flags as u32) | O_NONBLOCK) < 0 {
            if let Err(error) = close_socket(client) {
                record_cleanup_error(shared, error);
                terminal_status = ReceiverStatus::CleanupError;
                break;
            }
            continue;
        }
        let received = receive_client(
            shared,
            client,
            #[cfg(feature = "debugger-network")]
            &debugger_diagnostics,
        );
        shared.receive_completed.store(0, Ordering::Release);
        shared.receive_total.store(0, Ordering::Release);
        if let Err(error) = close_socket(client) {
            record_cleanup_error(shared, error);
            terminal_status = ReceiverStatus::CleanupError;
            break;
        }
        match received {
            Ok(None) => {}
            Ok(Some(payload)) => {
                let _guard = shared.lock_payload();
                // SAFETY: PayloadGuard provides exclusive access to the slot.
                let slot = unsafe { &mut *shared.payload.get() };
                if slot.is_none() {
                    *slot = Some(payload);
                    shared.set_status(ReceiverStatus::PayloadReady);
                }
            }
            Err(ReceiveError::Memory) => shared.set_status(ReceiverStatus::MemoryError),
            Err(ReceiveError::Invalid) => shared.set_status(ReceiverStatus::InvalidUpload),
            Err(ReceiveError::Stopped) => break,
        }
        if !shared.is_stopped() && !has_pending_payload(shared) {
            shared.set_status(ReceiverStatus::Listening);
        }
    }
    #[cfg(feature = "debugger-network")]
    if let Some(transport) = debugger_transport.as_mut() {
        close_debugger_transport(shared, transport, &mut debugger_diagnostics);
    }
    finish_worker(shared, Some(listener), terminal_status)
}

#[cfg(feature = "debugger-network")]
fn close_debugger_transport(
    shared: &Shared,
    transport: &mut libdb_sys::wii_network::WiiNetworkTransport,
    diagnostics: &mut libdb_sys::wii_network::ListenerDiagnostics,
) {
    match transport.close() {
        Ok(()) => diagnostics.listener_closed(),
        Err(error) => {
            if error.listener.is_none() {
                diagnostics.listener_closed();
            }
            for failure in [error.client, error.listener].into_iter().flatten() {
                diagnostics.cleanup_failed(libdb_sys::wii_network::failure_from_socket(failure));
                if let Some(code) = crate::wiiload::first_cleanup_failure(&[
                    failure.code,
                    failure.cleanup.unwrap_or(0),
                ]) {
                    record_cleanup_error(shared, code);
                }
            }
        }
    }
}

struct ListenerCreateError {
    stage: &'static str,
    code: i32,
    cleanup: Option<i32>,
}

fn create_listener() -> Result<i32, ListenerCreateError> {
    let socket = network::socket(PF_INET, SOCK_STREAM, 0);
    if socket < 0 {
        return Err(ListenerCreateError {
            stage: "socket",
            code: socket,
            cleanup: None,
        });
    }
    // A locally launched app can restart this service while connections from
    // the launching HBC are still in TIME_WAIT. Reuse that local address;
    // an active listener must still fail to bind (do not use SO_REUSEPORT).
    let reuse_address = 1u32;
    let reuse = unsafe {
        network::setsockopt(
            socket,
            network::SOL_SOCKET,
            network::SO_REUSEADDR,
            (&reuse_address as *const u32).cast(),
            core::mem::size_of_val(&reuse_address) as network::SockLenT,
        )
    };
    if reuse < 0 {
        return Err(ListenerCreateError {
            stage: "set address reuse",
            code: reuse,
            cleanup: close_socket(socket).err(),
        });
    }
    let flags = network::fcntl(socket, F_GETFL, 0);
    if flags < 0 {
        return Err(ListenerCreateError {
            stage: "get socket flags",
            code: flags,
            cleanup: close_socket(socket).err(),
        });
    }
    let set_flags = network::fcntl(socket, F_SETFL, (flags as u32) | O_NONBLOCK);
    if set_flags < 0 {
        return Err(ListenerCreateError {
            stage: "set nonblocking mode",
            code: set_flags,
            cleanup: close_socket(socket).err(),
        });
    }
    let address = SockaddrIn {
        sin_len: core::mem::size_of::<SockaddrIn>() as u8,
        sin_family: AF_INET as u8,
        sin_port: WIILOAD_TCP_PORT.to_be(),
        sin_addr: network::InAddr {
            s_addr: network::get_host_ip(),
        },
        ..SockaddrIn::default()
    };
    let bind = network::bind_addr(socket, &address);
    if bind < 0 {
        return Err(ListenerCreateError {
            stage: "bind",
            code: bind,
            cleanup: close_socket(socket).err(),
        });
    }
    let listen = network::listen(socket, LISTEN_BACKLOG);
    if listen < 0 {
        return Err(ListenerCreateError {
            stage: "listen",
            code: listen,
            cleanup: close_socket(socket).err(),
        });
    }
    Ok(socket)
}

fn close_socket(socket: i32) -> Result<(), i32> {
    let result = network::close(socket);
    if result < 0 { Err(result) } else { Ok(()) }
}

fn finish_worker(shared: &Shared, listener: Option<i32>, status: ReceiverStatus) -> ReceiverStatus {
    let close_result = listener.map_or(0, network::close);
    let deinit_result = network::deinit();
    if let Some(error) = crate::wiiload::first_cleanup_failure(&[close_result, deinit_result]) {
        record_cleanup_error(shared, error);
    }
    if shared.cleanup_error.load(Ordering::Acquire) < 0 {
        ReceiverStatus::CleanupError
    } else {
        status
    }
}

fn record_cleanup_error(shared: &Shared, error: i32) {
    crate::wiiload::remember_cleanup_error(&shared.cleanup_error, error);
}

fn initialize_network(shared: &Shared) -> bool {
    if shared.is_stopped() {
        return false;
    }

    // Match Reborn: start NWC24 before waiting for the socket interface and IP.
    // attach_active() can spend ten seconds waiting for an IP before the IOS
    // service has been started. init_async() also reuses an already initialized
    // stack, including the diagnostic reporter's connection.
    let started_at = lwp_sys::now_ticks();
    for _ in 0..NETWORK_INIT_RETRIES {
        if shared.is_stopped() {
            return false;
        }
        // No callback is needed. The status wait checks cancellation each poll.
        if unsafe { network::init_async(None, core::ptr::null_mut()) } != 0 {
            sleep_poll();
            continue;
        }
        let Some(result) = crate::wiiload::wait_network_initialization(
            network::get_status,
            || shared.is_stopped(),
            sleep_poll,
        ) else {
            return false;
        };
        ogc_system::report(format_args!(
            "HBC: Wiiload IOS network startup result: {result}\n"
        ));
        if result != EAGAIN && result != ETIMEDOUT {
            if result < 0 || shared.is_stopped() {
                return false;
            }
            let sockets = network::soc_init();
            ogc_system::report(format_args!(
                "HBC: Wiiload socket startup result: {sockets}\n"
            ));
            let elapsed_ms =
                lwp_sys::now_ticks().wrapping_sub(started_at) / (lwp_sys::TICKS_PER_SECOND / 1_000);
            ogc_system::report(format_args!(
                "HBC: Wiiload network startup took {elapsed_ms} ms\n"
            ));
            return sockets >= 0;
        }
        sleep_poll();
    }
    false
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReceiveError {
    Invalid,
    Memory,
    Stopped,
}

enum GeckoPoll {
    Idle,
    Pending,
    Complete(WiiloadPayload),
    Invalid,
    Memory,
}

struct GeckoReceive {
    header_bytes: [u8; WIILOAD_HEADER_LEN],
    header_len: usize,
    body: Option<(WiiloadHeader, Vec<u8>)>,
    body_len: usize,
    idle_polls: usize,
    started_at: Option<u64>,
}

impl GeckoReceive {
    const fn new() -> Self {
        Self {
            header_bytes: [0; WIILOAD_HEADER_LEN],
            header_len: 0,
            body: None,
            body_len: 0,
            idle_polls: 0,
            started_at: None,
        }
    }

    fn reset(&mut self, shared: &Shared) {
        self.header_len = 0;
        self.body = None;
        self.body_len = 0;
        self.idle_polls = 0;
        self.started_at = None;
        shared.receive_completed.store(0, Ordering::Release);
        shared.receive_total.store(0, Ordering::Release);
    }

    fn poll(&mut self, shared: &Shared) -> GeckoPoll {
        if self.started_at.is_some_and(|started| {
            client_deadline_expired(
                started,
                ogc_core::powerpc::read_timebase(),
                WIILOAD_CLIENT_TIMEOUT_TICKS,
            )
        }) {
            self.reset(shared);
            return GeckoPoll::Invalid;
        }
        let gecko_present = usbgecko::is_gecko_alive(USBGECKO_CHANNEL) != 0;
        shared.gecko_ready.store(gecko_present, Ordering::Release);
        if !gecko_present {
            self.reset(shared);
            return GeckoPoll::Idle;
        }
        if let Some((header, mut body)) = self.body.take() {
            let received = unsafe {
                usbgecko::recvbuffer_safe_ex(
                    USBGECKO_CHANNEL,
                    body[self.body_len..].as_mut_ptr().cast(),
                    (body.len() - self.body_len) as i32,
                    USBGECKO_RETRIES,
                )
            };
            return self.record_body_read(shared, header, &mut body, received);
        }

        let received = unsafe {
            usbgecko::recvbuffer_safe_ex(
                USBGECKO_CHANNEL,
                self.header_bytes[self.header_len..].as_mut_ptr().cast(),
                (WIILOAD_HEADER_LEN - self.header_len) as i32,
                USBGECKO_RETRIES,
            )
        };
        if received < 0 {
            self.reset(shared);
            return GeckoPoll::Invalid;
        }
        if received == 0 {
            self.idle_polls += 1;
            if self.idle_polls >= GECKO_IDLE_POLLS {
                self.reset(shared);
            }
            return GeckoPoll::Idle;
        }
        let received = received as usize;
        if received > WIILOAD_HEADER_LEN - self.header_len {
            self.reset(shared);
            return GeckoPoll::Invalid;
        }
        self.header_len += received;
        self.started_at
            .get_or_insert_with(ogc_core::powerpc::read_timebase);
        self.idle_polls = 0;
        if self.header_len < WIILOAD_HEADER_LEN {
            return GeckoPoll::Idle;
        }
        let header = match WiiloadHeader::decode(&self.header_bytes) {
            Ok(header) => header,
            Err(_) => {
                self.reset(shared);
                return GeckoPoll::Invalid;
            }
        };
        let body_len = match header.body_len() {
            Ok(len) => len,
            Err(_) => {
                self.reset(shared);
                return GeckoPoll::Invalid;
            }
        };
        let mut body = Vec::new();
        if body.try_reserve_exact(body_len).is_err() {
            self.reset(shared);
            return GeckoPoll::Memory;
        }
        body.resize(body_len, 0);
        self.body_len = 0;
        shared.receive_completed.store(0, Ordering::Release);
        shared
            .receive_total
            .store(body_len as u32, Ordering::Release);
        shared.set_status(ReceiverStatus::Receiving);
        self.body = Some((header, body));
        GeckoPoll::Pending
    }

    fn record_body_read(
        &mut self,
        shared: &Shared,
        header: WiiloadHeader,
        body: &mut Vec<u8>,
        received: i32,
    ) -> GeckoPoll {
        if received < 0 {
            self.reset(shared);
            return GeckoPoll::Invalid;
        }
        if received == 0 {
            self.idle_polls += 1;
            if self.idle_polls >= GECKO_IDLE_POLLS {
                self.reset(shared);
                return GeckoPoll::Invalid;
            }
            self.body = Some((header, core::mem::take(body)));
            return GeckoPoll::Pending;
        }
        let received = received as usize;
        if received > body.len().saturating_sub(self.body_len) {
            self.reset(shared);
            return GeckoPoll::Invalid;
        }
        self.body_len += received;
        self.idle_polls = 0;
        shared
            .receive_completed
            .store(self.body_len as u32, Ordering::Release);
        if self.body_len != body.len() {
            self.body = Some((header, core::mem::take(body)));
            return GeckoPoll::Pending;
        }
        let result = header.decode_body(body).map_err(|error| match error {
            WiiloadError::AllocationFailed => GeckoPoll::Memory,
            _ => GeckoPoll::Invalid,
        });
        self.reset(shared);
        match result {
            Ok(payload) => GeckoPoll::Complete(payload),
            Err(error) => error,
        }
    }
}

fn receive_client(
    shared: &Shared,
    client: i32,
    #[cfg(feature = "debugger-network")]
    debugger_diagnostics: &libdb_sys::wii_network::ListenerDiagnostics,
) -> Result<Option<WiiloadPayload>, ReceiveError> {
    let started_at = ogc_core::powerpc::read_timebase();
    let mut header_bytes = [0; WIILOAD_HEADER_LEN];
    read_exact(shared, client, &mut header_bytes, false, started_at)?;
    #[cfg(feature = "debugger-network")]
    if &header_bytes[..4] == b"HBCD" {
        let reply = debugger_diagnostics.reply(&header_bytes);
        write_response(shared, client, &reply, started_at)?;
        return Ok(None);
    }
    if matches!(&header_bytes[..4], b"HBCC" | b"HBCQ") {
        let reply = REMOTE_CONTROL.request(&header_bytes, remote_now_ms());
        let bytes = reply.encode(CONTROL_SESSION.load(Ordering::Acquire));
        write_response(shared, client, &bytes, started_at)?;
        return Ok(None);
    }
    if &header_bytes[..4] == b"HBCI" {
        let mut packet = [0; crate::remote_input::PACKET_LEN];
        packet[..WIILOAD_HEADER_LEN].copy_from_slice(&header_bytes);
        read_exact(
            shared,
            client,
            &mut packet[WIILOAD_HEADER_LEN..],
            false,
            started_at,
        )?;
        receive_remote_input(shared, client, &packet, started_at)?;
        return Ok(None);
    }
    let header = WiiloadHeader::decode(&header_bytes).map_err(|_| ReceiveError::Invalid)?;
    shared.set_status(ReceiverStatus::Receiving);
    let body_len = header.body_len().map_err(|_| ReceiveError::Invalid)?;
    shared.receive_completed.store(0, Ordering::Release);
    shared
        .receive_total
        .store(body_len as u32, Ordering::Release);
    // Peak receive memory can approach the wire body plus a second maximum-size
    // decompressed payload (about 38 MiB, plus allocator overhead).
    let mut body = Vec::new();
    body.try_reserve_exact(body_len)
        .map_err(|_| ReceiveError::Memory)?;
    body.resize(body_len, 0);
    read_exact(shared, client, &mut body, true, started_at)?;
    ogc_system::report(format_args!(
        "HBC: Wiiload body received; decoding {} bytes\n",
        body.len()
    ));
    header.decode_body(&body).map(Some).map_err(|error| {
        ogc_system::report(format_args!("HBC: Wiiload decoding failed: {error:?}\n"));
        match error {
            WiiloadError::AllocationFailed => ReceiveError::Memory,
            _ => ReceiveError::Invalid,
        }
    })
}

fn read_exact(
    shared: &Shared,
    client: i32,
    buffer: &mut [u8],
    track_progress: bool,
    started_at: u64,
) -> Result<(), ReceiveError> {
    let mut offset = 0;
    let mut idle_polls = 0;
    while offset < buffer.len() {
        if shared.is_stopped() {
            return Err(ReceiveError::Stopped);
        }
        if client_deadline_expired(
            started_at,
            ogc_core::powerpc::read_timebase(),
            WIILOAD_CLIENT_TIMEOUT_TICKS,
        ) {
            return Err(ReceiveError::Invalid);
        }
        let read = network::read_slice(client, &mut buffer[offset..]);
        if read > 0 {
            let read = read as usize;
            if read > buffer.len() - offset {
                return Err(ReceiveError::Invalid);
            }
            offset += read;
            if track_progress {
                shared
                    .receive_completed
                    .store(offset as u32, Ordering::Release);
            }
            idle_polls = 0;
        } else if read < 0 && !socket_would_block(read) {
            return Err(ReceiveError::Invalid);
        } else {
            idle_polls += 1;
            if idle_polls >= READ_IDLE_POLLS {
                return Err(ReceiveError::Invalid);
            }
            sleep_poll();
        }
    }
    if client_deadline_expired(
        started_at,
        ogc_core::powerpc::read_timebase(),
        WIILOAD_CLIENT_TIMEOUT_TICKS,
    ) {
        return Err(ReceiveError::Invalid);
    }
    Ok(())
}

fn has_pending_payload(shared: &Shared) -> bool {
    let _guard = shared.lock_payload();
    // SAFETY: PayloadGuard provides exclusive access to the slot.
    unsafe { (&*shared.payload.get()).is_some() }
}

fn sleep_poll() {
    lwp_sys::sleep_ticks(POLL_TICKS.max(1));
}

fn receive_remote_input(
    shared: &Shared,
    client: i32,
    packet: &[u8],
    started_at: u64,
) -> Result<(), ReceiveError> {
    let outcome = match crate::remote_input::Command::decode(packet) {
        Err(error) => error,
        Ok(command) => match REMOTE_INPUT.submit(command, remote_now_ms()) {
            Err(error) => error,
            Ok(()) => {
                let wait_started = remote_now_ms();
                loop {
                    let completion = REMOTE_INPUT.completion();
                    if completion != crate::remote_input::PENDING {
                        break completion;
                    }
                    if shared.is_stopped() {
                        break crate::remote_input::STOPPED;
                    }
                    if remote_now_ms().saturating_sub(wait_started) >= 2500 {
                        break crate::remote_input::PENDING;
                    }
                    sleep_poll();
                }
            }
        },
    };
    let response = [b'H', b'B', b'C', b'I', 1, outcome, 0, 0];
    write_response(shared, client, &response, started_at)
}

fn write_response(
    shared: &Shared,
    client: i32,
    response: &[u8],
    started_at: u64,
) -> Result<(), ReceiveError> {
    let mut offset = 0;
    while offset < response.len() {
        if shared.is_stopped() {
            return Err(ReceiveError::Stopped);
        }
        if client_deadline_expired(
            started_at,
            ogc_core::powerpc::read_timebase(),
            WIILOAD_CLIENT_TIMEOUT_TICKS,
        ) {
            return Err(ReceiveError::Invalid);
        }
        let written = network::write_slice(client, &response[offset..]);
        if written > 0 && written as usize <= response.len() - offset {
            offset += written as usize;
        } else if socket_would_block(written) {
            sleep_poll();
        } else {
            return Err(ReceiveError::Invalid);
        }
    }
    Ok(())
}
