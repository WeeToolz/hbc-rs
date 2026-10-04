//! Wii TCP receiver for the HBC Wiiload protocol.
//!
//! The listener owns its network handles on an LWP worker. The main loop can
//! read status or take a completed payload without waiting for network I/O.

use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU8, Ordering};

pub use crate::wiiload::ReceiverStatus;
use crate::wiiload::{
    indicator_state as map_indicator_state, same_local_subnet_16, IndicatorState, WiiloadHeader,
    WiiloadPayload, WIILOAD_HEADER_LEN,
};
use ogc_lwp::{sys as lwp_sys, ThreadHandle};
use ogc_network::sys::{F_GETFL, F_SETFL};
use ogc_network::{self as network, SockaddrIn, AF_INET, O_NONBLOCK, PF_INET, SOCK_STREAM};
use ogc_usbgecko as usbgecko;

const WIILOAD_PORT: u16 = 4299;
const LISTEN_BACKLOG: u32 = 3;
const THREAD_STACK_SIZE: usize = 8 * 1024;
const THREAD_PRIORITY: u8 = 48;
const NETWORK_INIT_RETRIES: usize = 32;
const NETWORK_BUSY_POLLS: usize = 200;
const READ_IDLE_POLLS: usize = 500;
const POLL_TICKS: u64 = lwp_sys::TICKS_PER_SECOND / 50;
const EBUSY: i32 = -16;
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
        while self
            .payload_lock
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
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

/// Take ownership of one completed payload. The worker can publish another after this call.
pub fn take_payload() -> Option<WiiloadPayload> {
    let _guard = SHARED.lock_payload();
    // SAFETY: PayloadGuard provides exclusive access to the Option.
    let payload = unsafe { (&mut *SHARED.payload.get()).take() };
    if payload.is_some() && !SHARED.is_stopped() {
        SHARED.set_status(ReceiverStatus::Listening);
    }
    payload
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

unsafe fn worker(arg: *mut c_void) -> *mut c_void {
    // SAFETY: start passes the address of the static SHARED value.
    let shared = unsafe { &*arg.cast::<Shared>() };
    let result = worker_main(shared);
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
            if let Some(cleanup) = error.cleanup {
                record_cleanup_error(shared, cleanup);
            }
            return finish_worker(shared, None, ReceiverStatus::ListenerError);
        }
    };
    let local_ip = network::get_host_ip();
    let _ = usbgecko::flush(USBGECKO_CHANNEL);
    // The inline poller is ready here. C HBC enables its Gecko icon after the
    // Gecko receiver worker reports initialized.
    shared.gecko_ready.store(true, Ordering::Release);
    shared.set_status(ReceiverStatus::Listening);
    let mut gecko = GeckoReceive::new();
    let mut terminal_status = ReceiverStatus::Stopped;

    while !shared.is_stopped() {
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
        if client == EAGAIN {
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
        shared.set_status(ReceiverStatus::Receiving);
        let received = receive_client(shared, client);
        shared.receive_completed.store(0, Ordering::Release);
        shared.receive_total.store(0, Ordering::Release);
        if let Err(error) = close_socket(client) {
            record_cleanup_error(shared, error);
            terminal_status = ReceiverStatus::CleanupError;
            break;
        }
        match received {
            Ok(payload) => {
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
    finish_worker(shared, Some(listener), terminal_status)
}

struct ListenerCreateError {
    cleanup: Option<i32>,
}

fn create_listener() -> Result<i32, ListenerCreateError> {
    let socket = network::socket(PF_INET, SOCK_STREAM, 0);
    if socket < 0 {
        return Err(ListenerCreateError { cleanup: None });
    }
    let flags = network::fcntl(socket, F_GETFL, 0);
    if flags < 0 || network::fcntl(socket, F_SETFL, (flags as u32) | O_NONBLOCK) < 0 {
        return Err(ListenerCreateError {
            cleanup: close_socket(socket).err(),
        });
    }
    let mut address = SockaddrIn::default();
    address.sin_len = core::mem::size_of::<SockaddrIn>() as u8;
    address.sin_family = AF_INET as u8;
    address.sin_port = WIILOAD_PORT.to_be();
    address.sin_addr.s_addr = network::get_host_ip();
    if network::bind_addr(socket, &address) < 0 || network::listen(socket, LISTEN_BACKLOG) < 0 {
        return Err(ListenerCreateError {
            cleanup: close_socket(socket).err(),
        });
    }
    Ok(socket)
}

fn close_socket(socket: i32) -> Result<(), i32> {
    let result = network::close(socket);
    if result < 0 {
        Err(result)
    } else {
        Ok(())
    }
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
    for _ in 0..NETWORK_INIT_RETRIES {
        if shared.is_stopped() {
            return false;
        }
        // No callback is needed: status polling is bounded below.
        if unsafe { network::init_async(None, core::ptr::null_mut()) } != 0 {
            sleep_poll();
            continue;
        }
        let mut busy_polls = 0;
        loop {
            if busy_polls == NETWORK_BUSY_POLLS {
                return false;
            }
            let result = network::get_status();
            if result == EBUSY {
                busy_polls += 1;
                sleep_poll();
                continue;
            }
            if result == EAGAIN || result == ETIMEDOUT {
                break;
            }
            if shared.is_stopped() || result < 0 || network::soc_init() < 0 {
                return false;
            }
            return true;
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
}

impl GeckoReceive {
    const fn new() -> Self {
        Self {
            header_bytes: [0; WIILOAD_HEADER_LEN],
            header_len: 0,
            body: None,
            body_len: 0,
            idle_polls: 0,
        }
    }

    fn reset(&mut self, shared: &Shared) {
        self.header_len = 0;
        self.body = None;
        self.body_len = 0;
        self.idle_polls = 0;
        shared.receive_completed.store(0, Ordering::Release);
        shared.receive_total.store(0, Ordering::Release);
    }

    fn poll(&mut self, shared: &Shared) -> GeckoPoll {
        if usbgecko::is_gecko_alive(USBGECKO_CHANNEL) == 0 {
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
        let result = header.decode_body(body).map_err(|_| GeckoPoll::Invalid);
        self.reset(shared);
        match result {
            Ok(payload) => GeckoPoll::Complete(payload),
            Err(error) => error,
        }
    }
}

fn receive_client(shared: &Shared, client: i32) -> Result<WiiloadPayload, ReceiveError> {
    let mut header_bytes = [0; WIILOAD_HEADER_LEN];
    read_exact(shared, client, &mut header_bytes, false)?;
    let header = WiiloadHeader::decode(&header_bytes).map_err(|_| ReceiveError::Invalid)?;
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
    read_exact(shared, client, &mut body, true)?;
    header.decode_body(&body).map_err(|_| ReceiveError::Invalid)
}

fn read_exact(
    shared: &Shared,
    client: i32,
    buffer: &mut [u8],
    track_progress: bool,
) -> Result<(), ReceiveError> {
    let mut offset = 0;
    let mut idle_polls = 0;
    while offset < buffer.len() {
        if shared.is_stopped() {
            return Err(ReceiveError::Stopped);
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
        } else if read < 0 && read != EAGAIN {
            return Err(ReceiveError::Invalid);
        } else {
            idle_polls += 1;
            if idle_polls >= READ_IDLE_POLLS {
                return Err(ReceiveError::Invalid);
            }
            sleep_poll();
        }
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
