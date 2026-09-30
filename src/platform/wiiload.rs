//! Wii TCP receiver for the HBC Wiiload protocol.
//!
//! The listener owns its network handles on an LWP worker. The main loop can
//! read status or take a completed payload without waiting for network I/O.

use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use crate::wiiload::{same_local_subnet_16, WiiloadHeader, WiiloadPayload, WIILOAD_HEADER_LEN};
use ogc_lwp::{sys as lwp_sys, ThreadHandle};
use ogc_network::sys::{F_GETFL, F_SETFL};
use ogc_network::{self as network, SockaddrIn, AF_INET, O_NONBLOCK, PF_INET, SOCK_STREAM};

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ReceiverStatus {
    Stopped = 0,
    Starting = 1,
    NetworkUnavailable = 2,
    Listening = 3,
    Receiving = 4,
    PayloadReady = 5,
    InvalidUpload = 6,
    ListenerError = 7,
    MemoryError = 8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiverError {
    AlreadyStarted,
    ThreadCreation,
}

struct Shared {
    stopped: AtomicBool,
    status: AtomicU8,
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
                let _ = unsafe { ThreadHandle::new(THREAD) }.join();
                STARTED.store(false, Ordering::Release);
            }
            _ => return Err(ReceiverError::AlreadyStarted),
        }
    }
    if STARTED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err(ReceiverError::AlreadyStarted);
    }
    SHARED.stopped.store(false, Ordering::Release);
    SHARED.set_status(ReceiverStatus::Starting);
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
        _ => ReceiverStatus::Stopped,
    }
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
pub fn stop() {
    if !STARTED.load(Ordering::Acquire) {
        return;
    }
    SHARED.stopped.store(true, Ordering::Release);
    // SAFETY: THREAD is written once by start and read only for this join.
    let _ = unsafe { ThreadHandle::new(THREAD) }.join();
    STARTED.store(false, Ordering::Release);
    SHARED.set_status(ReceiverStatus::Stopped);
}

unsafe fn worker(arg: *mut c_void) -> *mut c_void {
    // SAFETY: start passes the address of the static SHARED value.
    let shared = unsafe { &*arg.cast::<Shared>() };
    let result = worker_main(shared);
    shared.set_status(result);
    core::ptr::null_mut()
}

fn worker_main(shared: &Shared) -> ReceiverStatus {
    if !initialize_network(shared) {
        // init_async can leave IOS network setup in progress if stop or the
        // bounded status loop ends early. Deinit also handles an uninitialized
        // SOC wrapper state.
        let _ = network::deinit();
        return if shared.is_stopped() {
            ReceiverStatus::Stopped
        } else {
            ReceiverStatus::NetworkUnavailable
        };
    }
    let listener = create_listener();
    if listener < 0 {
        let _ = network::deinit();
        return ReceiverStatus::ListenerError;
    }
    let local_ip = network::get_host_ip();
    shared.set_status(ReceiverStatus::Listening);

    while !shared.is_stopped() {
        if has_pending_payload(shared) {
            sleep_poll();
            continue;
        }
        let mut peer = SockaddrIn::default();
        let client = network::accept_addr(listener, &mut peer);
        if client == EAGAIN {
            sleep_poll();
            continue;
        }
        if client < 0 {
            let _ = network::close(listener);
            let _ = network::deinit();
            return ReceiverStatus::ListenerError;
        }
        if !same_local_subnet_16(peer.sin_addr.s_addr, local_ip) {
            let _ = network::close(client);
            continue;
        }
        let flags = network::fcntl(client, F_GETFL, 0);
        if flags < 0 || network::fcntl(client, F_SETFL, (flags as u32) | O_NONBLOCK) < 0 {
            let _ = network::close(client);
            continue;
        }
        shared.set_status(ReceiverStatus::Receiving);
        let received = receive_client(shared, client);
        let _ = network::close(client);
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
    let _ = network::close(listener);
    let _ = network::deinit();
    ReceiverStatus::Stopped
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

fn create_listener() -> i32 {
    let socket = network::socket(PF_INET, SOCK_STREAM, 0);
    if socket < 0 {
        return socket;
    }
    let flags = network::fcntl(socket, F_GETFL, 0);
    if flags < 0 || network::fcntl(socket, F_SETFL, (flags as u32) | O_NONBLOCK) < 0 {
        let _ = network::close(socket);
        return -1;
    }
    let mut address = SockaddrIn::default();
    address.sin_len = core::mem::size_of::<SockaddrIn>() as u8;
    address.sin_family = AF_INET as u8;
    address.sin_port = WIILOAD_PORT.to_be();
    address.sin_addr.s_addr = network::get_host_ip();
    if network::bind_addr(socket, &address) < 0 || network::listen(socket, LISTEN_BACKLOG) < 0 {
        let _ = network::close(socket);
        return -1;
    }
    socket
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReceiveError {
    Invalid,
    Memory,
    Stopped,
}

fn receive_client(shared: &Shared, client: i32) -> Result<WiiloadPayload, ReceiveError> {
    let mut header_bytes = [0; WIILOAD_HEADER_LEN];
    read_exact(shared, client, &mut header_bytes)?;
    let header = WiiloadHeader::decode(&header_bytes).map_err(|_| ReceiveError::Invalid)?;
    let body_len = header.body_len().map_err(|_| ReceiveError::Invalid)?;
    // Peak receive memory can approach the wire body plus a second maximum-size
    // decompressed payload (about 38 MiB, plus allocator overhead).
    let mut body = Vec::new();
    body.try_reserve_exact(body_len)
        .map_err(|_| ReceiveError::Memory)?;
    body.resize(body_len, 0);
    read_exact(shared, client, &mut body)?;
    header.decode_body(&body).map_err(|_| ReceiveError::Invalid)
}

fn read_exact(shared: &Shared, client: i32, buffer: &mut [u8]) -> Result<(), ReceiveError> {
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
