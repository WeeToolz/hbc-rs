//! Wii TCP receiver for the HBC Wiiload protocol.
//!
//! The listener owns its network handles on an LWP worker. The main loop can
//! read status or take a completed payload without waiting for network I/O.

use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::ffi::c_void;
use core::sync::atomic::{
    AtomicBool, AtomicI32, AtomicU8, AtomicU16, AtomicU32, AtomicUsize, Ordering,
};

pub use crate::wiiload::ReceiverStatus;
use crate::wiiload::{
    IndicatorState, RebornUploadMetadata, WIILOAD_CLIENT_TIMEOUT_TICKS, WIILOAD_HEADER_LEN,
    WIILOAD_TCP_PORT, WiiloadError, WiiloadHeader, WiiloadPayload, client_deadline_expired,
    decode_reborn_upload_metadata, indicator_state as map_indicator_state, same_local_subnet_16,
    socket_would_block,
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

fn devnet_status_reply(status: i32) -> [u8; 8] {
    let mut reply = [0; 8];
    reply[..4].copy_from_slice(&status.to_be_bytes());
    reply
}

#[cfg(test)]
mod devnet_reply_tests {
    use super::devnet_status_reply;

    #[test]
    fn devnet_reply_uses_signed_status_and_empty_body_length() {
        assert_eq!(devnet_status_reply(0), [0; 8]);
        assert_eq!(
            devnet_status_reply(-22),
            [0xff, 0xff, 0xff, 0xea, 0, 0, 0, 0]
        );
    }
}

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
    payload: UnsafeCell<Option<ReceivedUpload>>,
    // The network worker owns pending metadata. The main thread only sees it
    // after the worker publishes the completed upload under payload_lock.
    pending_upload_metadata: UnsafeCell<Option<RebornUploadMetadata>>,
}

// `payload` is accessed only while `payload_lock` is held. The pending upload
// metadata is accessed only by the single network worker.
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
            pending_upload_metadata: UnsafeCell::new(None),
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

    fn set_upload_metadata(&self, metadata: RebornUploadMetadata) {
        // SAFETY: Only the network worker sets or takes this pending value.
        unsafe { *self.pending_upload_metadata.get() = Some(metadata) };
    }

    fn take_upload_metadata(&self, now_ms: u64) -> Option<RebornUploadMetadata> {
        // SAFETY: Only the network worker sets or takes this pending value.
        unsafe { (&mut *self.pending_upload_metadata.get()).take() }
            .filter(|metadata| metadata.is_current(now_ms))
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceivedUpload {
    pub payload: WiiloadPayload,
    pub metadata: Option<RebornUploadMetadata>,
}
static MENU_APP_COUNT: AtomicU32 = AtomicU32::new(0);
static MENU_DEVICE: AtomicU8 = AtomicU8::new(u8::MAX);
static MENU_INSERTED: AtomicU8 = AtomicU8::new(0);
static TRANSFER_STYLE: AtomicU8 = AtomicU8::new(crate::wiiload::RebornTransferStyle::None as u8);
static DEVFILE_TRANSFER_OP: AtomicU8 = AtomicU8::new(0);
static DEVFILE_TRANSFER_NAME: [AtomicU8; 64] = [const { AtomicU8::new(0) }; 64];
static DEVFILE_REQUEST_ACTIVE: AtomicBool = AtomicBool::new(false);
static DEVFILE_CHANGE_GENERATION: AtomicU32 = AtomicU32::new(0);
static DEVFILE_CHANGE_TICKS: AtomicU32 = AtomicU32::new(0);
static MENU_INIT_MS: AtomicU32 = AtomicU32::new(0);
static MENU_SCAN_MS: AtomicU32 = AtomicU32::new(0);
static NETWORK_STARTS: AtomicU32 = AtomicU32::new(0);
pub static REMOTE_CONTROL: crate::remote_control::Mailbox = crate::remote_control::Mailbox::new();
static CONTROL_SESSION: AtomicU32 = AtomicU32::new(0);
static SCREEN_FRAMEBUFFER: AtomicUsize = AtomicUsize::new(0);
static SCREEN_WIDTH: AtomicU16 = AtomicU16::new(0);
static SCREEN_HEIGHT: AtomicU16 = AtomicU16::new(0);
static SCREEN_BYTE_LEN: AtomicU32 = AtomicU32::new(0);
static LOG_TARGET_IP: AtomicU32 = AtomicU32::new(0);
static LOG_TARGET_PORT: AtomicU16 = AtomicU16::new(0);

static REMOTE_INPUT: crate::remote_input::Mailbox = crate::remote_input::Mailbox::new();

/// Publish the live scanout buffer for HBCP requests.
pub fn publish_screenshot_framebuffer(pointer: usize, width: u16, height: u16, byte_len: u32) {
    SCREEN_FRAMEBUFFER.store(pointer, Ordering::Relaxed);
    SCREEN_WIDTH.store(width, Ordering::Relaxed);
    SCREEN_HEIGHT.store(height, Ordering::Relaxed);
    SCREEN_BYTE_LEN.store(byte_len, Ordering::Release);
}

/// Restore the previous app log target before the MEM2 heap can use its slot.
pub fn initialize_log_target() {
    let target = read_log_target(crate::netlog::LOW_MEMORY_ADDRESS)
        .or_else(|| read_log_target(crate::netlog::KEEP_ADDRESS));
    set_log_target(target);
}

fn log_target() -> Option<crate::netlog::Target> {
    let port = LOG_TARGET_PORT.load(Ordering::Acquire);
    (port != 0).then(|| crate::netlog::Target {
        address: LOG_TARGET_IP.load(Ordering::Relaxed).to_be_bytes(),
        port,
    })
}

fn set_log_target(target: Option<crate::netlog::Target>) {
    let block = target.map_or([0; crate::netlog::BLOCK_LEN], crate::netlog::Target::encode);
    write_log_target(crate::netlog::LOW_MEMORY_ADDRESS, &block);
    write_log_target(crate::netlog::KEEP_ADDRESS, &block);
    match target {
        Some(target) => {
            LOG_TARGET_IP.store(u32::from_be_bytes(target.address), Ordering::Relaxed);
            LOG_TARGET_PORT.store(target.port, Ordering::Release);
        }
        None => {
            LOG_TARGET_IP.store(0, Ordering::Relaxed);
            LOG_TARGET_PORT.store(0, Ordering::Release);
        }
    }
}

fn read_log_target(address: usize) -> Option<crate::netlog::Target> {
    let pointer = address as *const u8;
    let mut block = [0; crate::netlog::BLOCK_LEN];
    // SAFETY: Reborn reserves both fixed addresses for this 32-byte shared
    // record. Cache invalidation makes the previous title's bytes visible.
    unsafe {
        ogc_cache::dc_invalidate_range(pointer.cast(), block.len() as u32);
        for (offset, byte) in block.iter_mut().enumerate() {
            *byte = core::ptr::read_volatile(pointer.add(offset));
        }
    }
    crate::netlog::Target::decode(&block)
}

fn write_log_target(address: usize, block: &[u8; crate::netlog::BLOCK_LEN]) {
    let pointer = address as *mut u8;
    // SAFETY: The shared record is reserved by Reborn. The Rust heap is kept
    // below the MEM2 copy, and low memory is reserved by the loader ABI.
    unsafe {
        for (offset, byte) in block.iter().copied().enumerate() {
            core::ptr::write_volatile(pointer.add(offset), byte);
        }
        ogc_cache::dc_flush_range(pointer.cast(), block.len() as u32);
    }
}

/// Publish the current app and storage view for nonblocking HBCS requests.
pub fn publish_menu_status(
    app_count: usize,
    active_device: Option<crate::platform::storage::Volume>,
    inserted_mask: u8,
    init_ms: u32,
    scan_ms: u32,
) {
    MENU_APP_COUNT.store(app_count.min(u32::MAX as usize) as u32, Ordering::Release);
    MENU_DEVICE.store(
        active_device.map_or(u8::MAX, |volume| volume.settings_index() as u8),
        Ordering::Release,
    );
    MENU_INSERTED.store(
        inserted_mask & ((1 << crate::platform::storage::DEVICE_COUNT) - 1),
        Ordering::Release,
    );
    MENU_INIT_MS.store(init_ms, Ordering::Release);
    MENU_SCAN_MS.store(scan_ms, Ordering::Release);
}

pub fn successful_network_starts() -> u32 {
    NETWORK_STARTS.load(Ordering::Acquire)
}

struct RebornKeys {
    lock: AtomicBool,
    queue: UnsafeCell<crate::reborn_keys::KeyQueue>,
}

// Access to queue is serialized by lock.
unsafe impl Sync for RebornKeys {}

static REBORN_KEYS: RebornKeys = RebornKeys {
    lock: AtomicBool::new(false),
    queue: UnsafeCell::new(crate::reborn_keys::KeyQueue::new()),
};

fn with_reborn_keys<R>(f: impl FnOnce(&mut crate::reborn_keys::KeyQueue) -> R) -> Option<R> {
    crate::wiiload::try_with_payload(&REBORN_KEYS.lock, || {
        // SAFETY: the lock provides exclusive access to the queue.
        f(unsafe { &mut *REBORN_KEYS.queue.get() })
    })
}

pub fn merge_reborn_keys(
    pacer: &mut crate::reborn_keys::KeyPacer,
    overlay_open: bool,
    frame: &mut crate::platform::input::InputFrame,
) {
    let Some(Some(key)) = with_reborn_keys(|queue| pacer.next(queue, overlay_open)) else {
        return;
    };
    let Some(input) = crate::reborn_keys::input_for_key(key) else {
        return;
    };
    if !frame.pressed.contains(&input) {
        frame.pressed.push(input);
    }
}

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
    // The previous worker is joined before STARTED is cleared. Reset its
    // high-water area before giving the stack to a new worker.
    unsafe {
        core::ptr::write_bytes(
            core::ptr::addr_of_mut!(THREAD_STACK.0).cast::<u8>(),
            0,
            THREAD_STACK_SIZE,
        )
    };
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

/// Return the active Reborn developer file transfer for the footer line.
pub fn devfile_transfer_progress() -> Option<(u8, alloc::string::String, u32, u32)> {
    let operation = DEVFILE_TRANSFER_OP.load(Ordering::Acquire);
    if !matches!(operation, b'p' | b'P' | b'g' | b'G') {
        return None;
    }
    let mut name = alloc::vec::Vec::new();
    for byte in &DEVFILE_TRANSFER_NAME {
        let byte = byte.load(Ordering::Acquire);
        if byte == 0 {
            break;
        }
        name.push(byte);
    }
    Some((
        operation,
        alloc::string::String::from_utf8_lossy(&name).into_owned(),
        SHARED.receive_completed.load(Ordering::Acquire),
        SHARED.receive_total.load(Ordering::Acquire),
    ))
}

pub fn transfer_style() -> crate::wiiload::RebornTransferStyle {
    crate::wiiload::RebornTransferStyle::from_u8(TRANSFER_STYLE.load(Ordering::Acquire))
}

pub fn initialize_transfer_style(seed: u32) {
    let selected = (seed % 3 + 1) as u8;
    let _ = TRANSFER_STYLE.compare_exchange(
        crate::wiiload::RebornTransferStyle::None as u8,
        selected,
        Ordering::AcqRel,
        Ordering::Acquire,
    );
}

pub fn devfile_request_active() -> bool {
    DEVFILE_REQUEST_ACTIVE.load(Ordering::Acquire)
}

pub fn devfile_app_change() -> Option<(u32, u32)> {
    let generation = DEVFILE_CHANGE_GENERATION.load(Ordering::Acquire);
    (generation != 0).then(|| (generation, DEVFILE_CHANGE_TICKS.load(Ordering::Acquire)))
}

fn publish_devfile_transfer(operation: u8, path: &[u8]) {
    let name = path.rsplit(|byte| *byte == b'/').next().unwrap_or(path);
    for (index, slot) in DEVFILE_TRANSFER_NAME.iter().enumerate() {
        slot.store(name.get(index).copied().unwrap_or(0), Ordering::Relaxed);
    }
    DEVFILE_TRANSFER_OP.store(operation, Ordering::Release);
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

/// Take ownership of one completed upload. The worker can publish another after this call.
pub fn take_payload() -> Option<ReceivedUpload> {
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
                    *slot = Some(ReceivedUpload {
                        payload,
                        metadata: None,
                    });
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
            peer.sin_addr.s_addr,
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
                let payload = ReceivedUpload {
                    payload,
                    metadata: shared.take_upload_metadata(remote_now_ms()),
                };
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
        let submitted = unsafe { network::init_async(None, core::ptr::null_mut()) };
        if submitted != 0 {
            ogc_system::report(format_args!(
                "HBC: Wiiload IOS network startup submission failed: {submitted}\n"
            ));
            return false;
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
            if sockets < 0 || shared.is_stopped() {
                return false;
            }
            NETWORK_STARTS.fetch_add(1, Ordering::AcqRel);
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
    peer_ip: u32,
    #[cfg(feature = "debugger-network")]
    debugger_diagnostics: &libdb_sys::wii_network::ListenerDiagnostics,
) -> Result<Option<WiiloadPayload>, ReceiveError> {
    let started_at = ogc_core::powerpc::read_timebase();
    let mut header_bytes = [0; WIILOAD_HEADER_LEN];
    read_exact(shared, client, &mut header_bytes, false, started_at)?;
    if &header_bytes[..4] == b"HBCK" {
        let Some(len) = crate::reborn_keys::body_length(&header_bytes) else {
            let mut reply = [0u8; 8];
            reply[..4].copy_from_slice(&(-22i32).to_be_bytes());
            return write_response(shared, client, &reply, started_at).map(|()| None);
        };
        let mut keys = [0u8; crate::reborn_keys::CAPACITY];
        if len != 0 && read_exact(shared, client, &mut keys[..len], false, started_at).is_err() {
            let mut reply = [0u8; 8];
            reply[..4].copy_from_slice(&(-22i32).to_be_bytes());
            return write_response(shared, client, &reply, started_at).map(|()| None);
        }
        while with_reborn_keys(|queue| queue.push(&keys[..len])).is_none() {
            if shared.is_stopped() {
                return Err(ReceiveError::Stopped);
            }
            sleep_poll();
        }
        return write_response(shared, client, &[0; 8], started_at).map(|()| None);
    }
    if &header_bytes[..4] == b"HBCP" {
        return send_screenshot(shared, client, started_at).map(|()| None);
    }
    if &header_bytes[..4] == b"HBCN" {
        let port = u16::from_be_bytes([header_bytes[4], header_bytes[5]]);
        let target = (port != 0).then_some(crate::netlog::Target {
            address: peer_ip.to_be_bytes(),
            port,
        });
        set_log_target(target);
        return write_response(shared, client, &[0; 8], started_at).map(|()| None);
    }
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
    if &header_bytes[..4] == b"HBCA" {
        let name_len = usize::from(u16::from_be_bytes([header_bytes[4], header_bytes[5]]));
        let flags = u16::from_be_bytes([header_bytes[6], header_bytes[7]]);
        if name_len > crate::wiiload::REBORN_UPLOAD_NAME_CAPACITY || (name_len == 0 && flags == 0) {
            let reply = devnet_status_reply(-22);
            return write_response(shared, client, &reply, started_at).map(|()| None);
        }
        let mut name = [0; crate::wiiload::REBORN_UPLOAD_NAME_CAPACITY];
        if name_len != 0 {
            match read_exact(shared, client, &mut name[..name_len], false, started_at) {
                Ok(()) => {}
                Err(ReceiveError::Stopped) => return Err(ReceiveError::Stopped),
                Err(_) => {
                    let reply = devnet_status_reply(-22);
                    return write_response(shared, client, &reply, started_at).map(|()| None);
                }
            }
        }
        let metadata = match decode_reborn_upload_metadata(
            &header_bytes,
            &name[..name_len],
            remote_now_ms(),
        ) {
            Ok(metadata) => metadata,
            Err(_) => {
                let reply = devnet_status_reply(-22);
                return write_response(shared, client, &reply, started_at).map(|()| None);
            }
        };
        shared.set_upload_metadata(metadata);
        return write_response(shared, client, &[0; 8], started_at).map(|()| None);
    }
    if &header_bytes[..4] == b"HBCT" {
        let Some(style) = crate::wiiload::RebornTransferStyle::decode_hbct(&header_bytes) else {
            let reply = devnet_status_reply(-22);
            return write_response(shared, client, &reply, started_at).map(|()| None);
        };
        TRANSFER_STYLE.store(style as u8, Ordering::Release);
        return write_response(shared, client, &[0; 8], started_at).map(|()| None);
    }
    if &header_bytes[..4] == b"HBCF" {
        let header = match crate::devfile::RequestHeader::decode(&header_bytes) {
            Ok(header) => header,
            Err(_) => {
                return write_devfile_reply(
                    shared,
                    client,
                    -crate::devfile::errno::EINVAL,
                    &[],
                    started_at,
                )
                .map(|()| None);
            }
        };
        let mut path = Vec::new();
        if path.try_reserve_exact(header.path_len).is_err() {
            return write_devfile_reply(
                shared,
                client,
                -crate::devfile::errno::ENOMEM,
                &[],
                started_at,
            )
            .map(|()| None);
        }
        path.resize(header.path_len, 0);
        if read_exact(shared, client, &mut path, false, started_at).is_err() {
            return write_devfile_reply(
                shared,
                client,
                -crate::devfile::errno::EINVAL,
                &[],
                started_at,
            )
            .map(|()| None);
        }
        return handle_devfile(shared, client, &header, &path, started_at).map(|()| None);
    }
    if &header_bytes[..4] == b"HBCS" {
        let body = status_json(shared);
        let mut response = Vec::with_capacity(8 + body.len());
        response.extend_from_slice(&0i32.to_be_bytes());
        response.extend_from_slice(&(body.len() as u32).to_be_bytes());
        response.extend_from_slice(body.as_bytes());
        write_response(shared, client, &response, started_at)?;
        return Ok(None);
    }
    let header = WiiloadHeader::decode(&header_bytes).map_err(|_| ReceiveError::Invalid)?;
    shared.set_status(ReceiverStatus::Receiving);
    let body_len = header.body_len().map_err(|_| ReceiveError::Invalid)?;
    shared.receive_completed.store(0, Ordering::Release);
    shared
        .receive_total
        .store(body_len as u32, Ordering::Release);
    let data = if header.is_compressed() {
        let mut compressed_read = 0usize;
        crate::wiiload::decode_zlib_stream(
            header.data_len_uncompressed as usize,
            header.data_len as usize,
            |chunk| {
                read_exact(shared, client, chunk, false, started_at)?;
                compressed_read += chunk.len();
                shared
                    .receive_completed
                    .store(compressed_read as u32, Ordering::Release);
                Ok::<_, ReceiveError>(())
            },
        )
        .map_err(|error| match error {
            crate::wiiload::StreamDecodeError::Source(source) => source,
            crate::wiiload::StreamDecodeError::Allocation => ReceiveError::Memory,
            crate::wiiload::StreamDecodeError::SizeMismatch => ReceiveError::Invalid,
            crate::wiiload::StreamDecodeError::Invalid => ReceiveError::Invalid,
        })?
    } else {
        let data_len = header.data_len as usize;
        let mut data = Vec::new();
        data.try_reserve_exact(data_len)
            .map_err(|_| ReceiveError::Memory)?;
        data.resize(data_len, 0);
        read_exact(shared, client, &mut data, true, started_at)?;
        data
    };
    let mut args = Vec::new();
    args.try_reserve_exact(header.args_len as usize)
        .map_err(|_| ReceiveError::Memory)?;
    args.resize(header.args_len as usize, 0);
    read_exact(shared, client, &mut args, false, started_at)?;
    shared
        .receive_completed
        .store(body_len as u32, Ordering::Release);
    ogc_system::report(format_args!(
        "HBC: Wiiload payload received; data={} args={} bytes\n",
        data.len(),
        args.len()
    ));
    Ok(Some(WiiloadPayload { data, args }))
}

fn send_screenshot(shared: &Shared, client: i32, started_at: u64) -> Result<(), ReceiveError> {
    let byte_len = SCREEN_BYTE_LEN.load(Ordering::Acquire) as usize;
    let pointer = SCREEN_FRAMEBUFFER.load(Ordering::Relaxed);
    let width = SCREEN_WIDTH.load(Ordering::Relaxed);
    let height = SCREEN_HEIGHT.load(Ordering::Relaxed);
    let pixels_len = usize::from(width)
        .checked_mul(usize::from(height))
        .and_then(|pixels| pixels.checked_mul(2));
    let Some(pixels_len) = pixels_len
        .filter(|length| pointer != 0 && width != 0 && height != 0 && *length <= byte_len)
    else {
        return write_response(shared, client, &devnet_status_reply(-19), started_at);
    };
    let Some(payload_len) = pixels_len
        .checked_add(8)
        .filter(|length| *length <= u32::MAX as usize)
    else {
        return write_response(shared, client, &devnet_status_reply(-22), started_at);
    };

    // Reborn flushes the scanout range before its network worker reads it.
    // VideoSystem owns this framebuffer for the full process lifetime.
    unsafe { ogc_cache::dc_flush_range(pointer as *const c_void, pixels_len as u32) };
    let mut header = [0; 8];
    header[4..].copy_from_slice(&(payload_len as u32).to_be_bytes());
    write_response(shared, client, &header, started_at)?;
    let mut dimensions = [0; 8];
    dimensions[..4].copy_from_slice(&u32::from(width).to_be_bytes());
    dimensions[4..].copy_from_slice(&u32::from(height).to_be_bytes());
    write_response(shared, client, &dimensions, started_at)?;

    // SAFETY: VideoSystem owns this buffer until process exit. Its reported
    // length covers the YUYV frame. Reborn sends the live scanout in the same
    // way, so GX may update it while this network response is in progress.
    let pixels = unsafe { core::slice::from_raw_parts(pointer as *const u8, pixels_len) };
    write_response(shared, client, pixels, started_at)
}

fn write_devfile_reply(
    shared: &Shared,
    client: i32,
    status: i32,
    payload: &[u8],
    started_at: u64,
) -> Result<(), ReceiveError> {
    let Ok(length) = u32::try_from(payload.len()) else {
        return write_response(
            shared,
            client,
            &devnet_status_reply(-crate::devfile::errno::EFBIG),
            started_at,
        );
    };
    write_devfile_header(shared, client, status, length, started_at)?;
    write_response(shared, client, payload, started_at)
}

fn write_devfile_header(
    shared: &Shared,
    client: i32,
    status: i32,
    length: u32,
    started_at: u64,
) -> Result<(), ReceiveError> {
    let mut header = [0; 8];
    header[..4].copy_from_slice(&status.to_be_bytes());
    header[4..].copy_from_slice(&length.to_be_bytes());
    write_response(shared, client, &header, started_at)
}

fn devfile_errno(error: ogc_fat::FatError) -> i32 {
    -ogc_fat::fat_errno(error)
}

struct DevfileProgressGuard<'a>(&'a Shared);

impl Drop for DevfileProgressGuard<'_> {
    fn drop(&mut self) {
        self.0.receive_completed.store(0, Ordering::Release);
        self.0.receive_total.store(0, Ordering::Release);
        DEVFILE_TRANSFER_OP.store(0, Ordering::Release);
    }
}

struct DevfileRequestGuard;

impl Drop for DevfileRequestGuard {
    fn drop(&mut self) {
        DEVFILE_REQUEST_ACTIVE.store(false, Ordering::Release);
    }
}

fn devfile_volume(device: &[u8]) -> Option<crate::platform::storage::Volume> {
    match device {
        b"sd" => Some(crate::platform::storage::Volume::Sd),
        b"usb" => Some(crate::platform::storage::Volume::Usb),
        b"usb2" => Some(crate::platform::storage::Volume::Usb2),
        b"carda" => Some(crate::platform::storage::Volume::CardA),
        b"cardb" => Some(crate::platform::storage::Volume::CardB),
        _ => None,
    }
}

fn devfile_c_path(path: &[u8]) -> Result<alloc::ffi::CString, i32> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(path.len())
        .map_err(|_| -crate::devfile::errno::ENOMEM)?;
    bytes.extend_from_slice(path);
    alloc::ffi::CString::new(bytes).map_err(|_| -crate::devfile::errno::EINVAL)
}

fn devfile_entry_line(
    metadata: ogc_fat::FatMetadata,
    modified: u64,
    name: &[u8],
) -> Result<Vec<u8>, i32> {
    let kind = if metadata.is_directory { 'd' } else { 'f' };
    let size = if metadata.is_directory {
        0
    } else {
        metadata.size
    };
    let readonly = if metadata.readonly { "r" } else { "-" };
    let mut line = alloc::string::String::new();
    line.try_reserve_exact(40 + name.len())
        .map_err(|_| -crate::devfile::errno::ENOMEM)?;
    use core::fmt::Write as _;
    let _ = write!(line, "{kind} {size} {modified} {readonly} ");
    let mut bytes = line.into_bytes();
    bytes.extend_from_slice(name);
    bytes.push(b'\n');
    Ok(bytes)
}

fn handle_devfile(
    shared: &Shared,
    client: i32,
    header: &crate::devfile::RequestHeader,
    raw_path: &[u8],
    started_at: u64,
) -> Result<(), ReceiveError> {
    use crate::devfile::Operation;

    DEVFILE_REQUEST_ACTIVE.store(true, Ordering::Release);
    let _request = DevfileRequestGuard;

    let validated = match crate::devfile::validate_path(raw_path) {
        Ok(path) => path,
        Err(_) => {
            return write_devfile_reply(
                shared,
                client,
                -crate::devfile::errno::EINVAL,
                &[],
                started_at,
            );
        }
    };
    if let Some(operation @ (b'p' | b'P' | b'g' | b'G')) =
        header.operation.map(|operation| match operation {
            Operation::PutFramed => b'p',
            Operation::PutRaw => b'P',
            Operation::GetFramed => b'g',
            Operation::GetRaw => b'G',
            _ => 0,
        })
    {
        publish_devfile_transfer(operation, validated.bytes);
    }
    let _transfer_progress = matches!(
        header.operation,
        Some(Operation::PutFramed | Operation::PutRaw | Operation::GetFramed | Operation::GetRaw)
    )
    .then(|| DevfileProgressGuard(shared));
    let Some(volume) = devfile_volume(validated.device) else {
        return write_devfile_reply(
            shared,
            client,
            -crate::devfile::errno::EINVAL,
            &[],
            started_at,
        );
    };
    if let Err(error) = crate::platform::storage::FatStorage::mount(volume) {
        let errno = match error {
            crate::platform::storage::StorageError::Fat(error)
            | crate::platform::storage::StorageError::UsbMount { error, .. } => {
                devfile_errno(error)
            }
            _ => -crate::devfile::errno::EIO,
        };
        return write_devfile_reply(shared, client, errno, &[], started_at);
    }

    let path = crate::devfile::strip_directory_slash(validated.bytes);
    let c_path = match devfile_c_path(path) {
        Ok(path) => path,
        Err(errno) => return write_devfile_reply(shared, client, errno, &[], started_at),
    };
    let response = match header.operation {
        Some(Operation::Stat) => devfile_stat(path, &c_path),
        Some(Operation::List) => devfile_list(path, &c_path),
        Some(Operation::Checksum) => devfile_checksum(&c_path),
        Some(Operation::Delete) => devfile_delete(&c_path),
        Some(Operation::MakeDirectory) => devfile_mkdir(path),
        Some(Operation::PutFramed | Operation::PutRaw)
            if validated.trailing_slash || header.size > crate::devfile::MAX_UPLOAD_SIZE =>
        {
            Err(if validated.trailing_slash {
                -crate::devfile::errno::EISDIR
            } else {
                -crate::devfile::errno::EFBIG
            })
        }
        Some(Operation::PutFramed | Operation::PutRaw) => {
            devfile_put(shared, client, path, header, started_at).map(|()| Vec::new())
        }
        Some(Operation::GetRaw | Operation::GetFramed) => {
            return devfile_get(shared, client, &c_path, header, started_at);
        }
        None => Err(-crate::devfile::errno::ENOSYS),
    };
    match response {
        Ok(payload) => {
            if matches!(
                header.operation,
                Some(Operation::PutFramed | Operation::PutRaw)
            ) {
                note_devfile_app_change(validated.device, path);
            }
            if matches!(header.operation, Some(Operation::Delete)) {
                note_devfile_app_change(validated.device, path);
            }
            write_devfile_reply(shared, client, 0, &payload, started_at)
        }
        Err(errno) => write_devfile_reply(shared, client, errno, &[], started_at),
    }
}

fn note_devfile_app_change(device: &[u8], path: &[u8]) {
    let device_index = MENU_DEVICE.load(Ordering::Acquire);
    let active_device = match device_index {
        0 => b"sd:".as_slice(),
        1 => b"usb:",
        2 => b"carda:",
        3 => b"cardb:",
        4 => b"usb2:",
        _ => return,
    };
    let active_device_name = &active_device[..active_device.len() - 1];
    if device != active_device_name
        || crate::devfile::app_change_name(active_device_name, path).is_none()
    {
        return;
    }
    DEVFILE_CHANGE_TICKS.store(ogc_lwp::sys::now_ticks() as u32, Ordering::Release);
    DEVFILE_CHANGE_GENERATION.fetch_add(1, Ordering::AcqRel);
}

fn devfile_put(
    shared: &Shared,
    client: i32,
    path: &[u8],
    header: &crate::devfile::RequestHeader,
    started_at: u64,
) -> Result<(), i32> {
    let mut part_path = Vec::new();
    part_path
        .try_reserve_exact(path.len() + 5)
        .map_err(|_| -crate::devfile::errno::ENOMEM)?;
    part_path.extend_from_slice(path);
    part_path.extend_from_slice(b".part");
    devfile_mkdir_parents(path);
    let target = devfile_c_path(path)?;
    let part = devfile_c_path(&part_path)?;
    let framed = header.operation == Some(crate::devfile::Operation::PutFramed);
    let write_result = (|| {
        let mut file = ogc_fat::FatFile::create_truncate(&part).map_err(devfile_errno)?;
        let mut remaining = header.size;
        let mut completed = 0u32;
        shared.receive_completed.store(0, Ordering::Release);
        shared.receive_total.store(header.size, Ordering::Release);
        while remaining != 0 {
            let raw = if framed {
                let mut frame_header = [0; 12];
                read_exact(shared, client, &mut frame_header, false, started_at)
                    .map_err(receive_devfile_errno)?;
                let raw_len = u32::from_be_bytes(frame_header[..4].try_into().unwrap()) as usize;
                let wire_len = u32::from_be_bytes(frame_header[4..8].try_into().unwrap()) as usize;
                let expected_crc = u32::from_be_bytes(frame_header[8..12].try_into().unwrap());
                if raw_len == 0
                    || raw_len > crate::devfile::MAX_FRAME_SIZE
                    || raw_len > remaining as usize
                    || wire_len == 0
                    || wire_len > raw_len
                {
                    return Err(-crate::devfile::errno::EPROTO);
                }
                let mut wire = Vec::new();
                wire.try_reserve_exact(wire_len)
                    .map_err(|_| -crate::devfile::errno::ENOMEM)?;
                wire.resize(wire_len, 0);
                read_exact(shared, client, &mut wire, false, started_at)
                    .map_err(receive_devfile_errno)?;
                let raw = if wire_len < raw_len {
                    let mut offset = 0;
                    crate::wiiload::decode_zlib_stream(raw_len, wire_len, |chunk| {
                        let end = offset + chunk.len();
                        chunk.copy_from_slice(&wire[offset..end]);
                        offset = end;
                        Ok::<(), ()>(())
                    })
                    .map_err(|error| match error {
                        crate::wiiload::StreamDecodeError::Allocation => {
                            -crate::devfile::errno::ENOMEM
                        }
                        _ => -crate::devfile::errno::EBADMSG,
                    })?
                } else {
                    wire
                };
                if crc32fast::hash(&raw) != expected_crc {
                    return Err(-crate::devfile::errno::EBADMSG);
                }
                remaining -= raw_len as u32;
                completed = completed.saturating_add(raw_len as u32);
                raw
            } else {
                let count = (remaining as usize).min(32 * 1024);
                let mut bytes = Vec::new();
                bytes
                    .try_reserve_exact(count)
                    .map_err(|_| -crate::devfile::errno::ENOMEM)?;
                bytes.resize(count, 0);
                read_exact(shared, client, &mut bytes, false, started_at)
                    .map_err(receive_devfile_errno)?;
                remaining -= count as u32;
                completed = completed.saturating_add(count as u32);
                bytes
            };
            write_devfile_file(&mut file, &raw)?;
            shared.receive_completed.store(completed, Ordering::Release);
        }
        file.flush().map_err(devfile_errno)?;
        file.close().map_err(devfile_errno)
    })();
    shared.receive_completed.store(0, Ordering::Release);
    shared.receive_total.store(0, Ordering::Release);
    if let Err(errno) = write_result {
        let _ = ogc_fat::remove_file(&part);
        return Err(errno);
    }
    ogc_fat::rename_replace(&part, &target).map_err(devfile_errno)
}

fn write_devfile_file(file: &mut ogc_fat::FatFile, mut bytes: &[u8]) -> Result<(), i32> {
    while !bytes.is_empty() {
        let written = file.write(bytes).map_err(devfile_errno)?;
        if written == 0 {
            return Err(-ogc_fat::fat_errno(ogc_fat::FatError::NoSpace));
        }
        bytes = &bytes[written..];
    }
    Ok(())
}

fn receive_devfile_errno(error: ReceiveError) -> i32 {
    match error {
        ReceiveError::Stopped => -crate::devfile::errno::EINTR,
        ReceiveError::Memory => -crate::devfile::errno::ENOMEM,
        ReceiveError::Invalid => -crate::devfile::errno::EIO,
    }
}

fn devfile_mkdir_parents(path: &[u8]) {
    let Some(colon) = path.iter().position(|byte| *byte == b':') else {
        return;
    };
    let mut at = colon + 2;
    while at < path.len() {
        let Some(relative) = path[at..].iter().position(|byte| *byte == b'/') else {
            break;
        };
        at += relative;
        if at > colon + 1
            && let Ok(parent) = devfile_c_path(&path[..at])
        {
            let _ = ogc_fat::create_directory(&parent);
        }
        at += 1;
    }
}

fn devfile_get(
    shared: &Shared,
    client: i32,
    c_path: &core::ffi::CStr,
    header: &crate::devfile::RequestHeader,
    started_at: u64,
) -> Result<(), ReceiveError> {
    let metadata = match ogc_fat::stat(c_path) {
        Ok(metadata) if !metadata.is_directory => metadata,
        _ => {
            return write_devfile_reply(
                shared,
                client,
                -crate::devfile::errno::ENOENT,
                &[],
                started_at,
            );
        }
    };
    let mut file = match ogc_fat::FatFile::open(c_path) {
        Ok(file) => file,
        Err(error) => {
            return write_devfile_reply(shared, client, devfile_errno(error), &[], started_at);
        }
    };
    let framed = header.operation == Some(crate::devfile::Operation::GetFramed);
    let compress = framed && header.flags & crate::devfile::FLAG_COMPRESS != 0;
    write_devfile_header(shared, client, 0, metadata.size, started_at)?;
    let mut remaining = metadata.size;
    shared.receive_completed.store(0, Ordering::Release);
    shared.receive_total.store(metadata.size, Ordering::Release);
    while remaining != 0 {
        let count = (remaining as usize).min(if framed {
            crate::devfile::MAX_FRAME_SIZE
        } else {
            32 * 1024
        });
        let mut bytes = Vec::new();
        if bytes.try_reserve_exact(count).is_err() {
            if framed {
                write_devfile_terminal(shared, client, -crate::devfile::errno::ENOMEM, started_at)?;
            }
            return Err(ReceiveError::Memory);
        }
        bytes.resize(count, 0);
        let mut read = 0;
        while read < count {
            match file.read(&mut bytes[read..]) {
                Ok(0) => break,
                Ok(length) => read += length,
                Err(_) => {
                    if framed {
                        write_devfile_terminal(
                            shared,
                            client,
                            -crate::devfile::errno::EIO,
                            started_at,
                        )?;
                    }
                    return Err(ReceiveError::Invalid);
                }
            }
        }
        if framed {
            let checksum = crc32fast::hash(&bytes);
            let compressed = if compress {
                miniz_oxide::deflate::compress_to_vec_zlib(&bytes, 1)
            } else {
                Vec::new()
            };
            let wire = if compress && compressed.len() < bytes.len() {
                compressed.as_slice()
            } else {
                bytes.as_slice()
            };
            let mut frame = Vec::new();
            if frame.try_reserve_exact(12 + wire.len()).is_err() {
                write_devfile_terminal(shared, client, -crate::devfile::errno::ENOMEM, started_at)?;
                return Err(ReceiveError::Memory);
            }
            frame.extend_from_slice(&(count as u32).to_be_bytes());
            frame.extend_from_slice(&(wire.len() as u32).to_be_bytes());
            frame.extend_from_slice(&checksum.to_be_bytes());
            frame.extend_from_slice(wire);
            write_response(shared, client, &frame, started_at)?;
        } else {
            write_response(shared, client, &bytes, started_at)?;
        }
        remaining -= count as u32;
        shared
            .receive_completed
            .store(metadata.size - remaining, Ordering::Release);
    }
    if framed {
        write_devfile_terminal(shared, client, 0, started_at)?;
    }
    shared.receive_completed.store(0, Ordering::Release);
    shared.receive_total.store(0, Ordering::Release);
    Ok(())
}

fn write_devfile_terminal(
    shared: &Shared,
    client: i32,
    status: i32,
    started_at: u64,
) -> Result<(), ReceiveError> {
    let mut frame = [0; 12];
    frame[8..12].copy_from_slice(&status.to_be_bytes());
    write_response(shared, client, &frame, started_at)
}

fn devfile_stat(path: &[u8], c_path: &core::ffi::CStr) -> Result<Vec<u8>, i32> {
    let metadata = ogc_fat::stat(c_path).map_err(devfile_errno)?;
    let modified = ogc_fat::get_times(c_path)
        .map_err(devfile_errno)?
        .modified
        .unwrap_or(0);
    let basename = path.rsplit(|byte| *byte == b'/').next().unwrap_or(path);
    devfile_entry_line(metadata, modified, basename)
}

fn devfile_list(path: &[u8], c_path: &core::ffi::CStr) -> Result<Vec<u8>, i32> {
    let mut directory = ogc_fat::FatDirectory::open(c_path).map_err(devfile_errno)?;
    let mut listing = Vec::new();
    listing
        .try_reserve_exact(crate::devfile::MAX_LIST_SIZE)
        .map_err(|_| -crate::devfile::errno::ENOMEM)?;
    listing.extend_from_slice(b"#list 2\n");
    while let Some(entry) = directory.next_entry().map_err(devfile_errno)? {
        if entry.name == b"." || entry.name == b".." {
            continue;
        }
        if listing.len() >= crate::devfile::MAX_LIST_SIZE - crate::devfile::PATH_CAPACITY - 64 {
            listing.extend_from_slice(b"! truncated\n");
            break;
        }
        let mut child = Vec::new();
        child
            .try_reserve_exact(path.len() + 1 + entry.name.len())
            .map_err(|_| -crate::devfile::errno::ENOMEM)?;
        child.extend_from_slice(path);
        if child.last() != Some(&b'/') {
            child.push(b'/');
        }
        child.extend_from_slice(&entry.name);
        let Ok(child_path) = devfile_c_path(&child) else {
            continue;
        };
        let Ok(metadata) = ogc_fat::stat(&child_path) else {
            continue;
        };
        let modified = ogc_fat::get_times(&child_path)
            .ok()
            .and_then(|times| times.modified)
            .unwrap_or(0);
        let line = devfile_entry_line(metadata, modified, &entry.name)?;
        listing.extend_from_slice(&line);
    }
    Ok(listing)
}

fn devfile_checksum(c_path: &core::ffi::CStr) -> Result<Vec<u8>, i32> {
    let metadata = ogc_fat::stat(c_path).map_err(devfile_errno)?;
    if metadata.is_directory {
        return Err(-crate::devfile::errno::EISDIR);
    }
    let mut file = ogc_fat::FatFile::open(c_path).map_err(devfile_errno)?;
    let mut buffer = Vec::new();
    buffer
        .try_reserve_exact(32 * 1024)
        .map_err(|_| -crate::devfile::errno::ENOMEM)?;
    buffer.resize(32 * 1024, 0);
    let mut checksum = crc32fast::Hasher::new();
    loop {
        let length = file.read(&mut buffer).map_err(devfile_errno)?;
        if length == 0 {
            break;
        }
        checksum.update(&buffer[..length]);
    }
    let mut result = Vec::new();
    result
        .try_reserve_exact(8)
        .map_err(|_| -crate::devfile::errno::ENOMEM)?;
    result.extend_from_slice(&metadata.size.to_be_bytes());
    result.extend_from_slice(&checksum.finalize().to_be_bytes());
    Ok(result)
}

fn devfile_delete(c_path: &core::ffi::CStr) -> Result<Vec<u8>, i32> {
    let metadata = ogc_fat::stat(c_path).map_err(|_| -crate::devfile::errno::ENOENT)?;
    if metadata.is_directory {
        ogc_fat::remove_directory(c_path).map_err(devfile_errno)?;
    } else {
        ogc_fat::remove_file(c_path).map_err(devfile_errno)?;
    }
    Ok(Vec::new())
}

fn devfile_mkdir(path: &[u8]) -> Result<Vec<u8>, i32> {
    devfile_mkdir_parents(path);
    let target = devfile_c_path(path)?;
    match ogc_fat::create_directory(&target) {
        Ok(()) | Err(ogc_fat::FatError::AlreadyExists) => Ok(Vec::new()),
        Err(error) => Err(devfile_errno(error)),
    }
}

fn status_json(shared: &Shared) -> alloc::string::String {
    let status = match shared.status.load(Ordering::Acquire) {
        1 => "starting",
        2 => "network_unavailable",
        3 => "listening",
        4 => "receiving",
        5 => "payload_ready",
        6 => "invalid_upload",
        7 => "listener_error",
        8 => "memory_error",
        9 => "cleanup_error",
        _ => "stopped",
    };
    let (completed, total) = (
        shared.receive_completed.load(Ordering::Acquire),
        shared.receive_total.load(Ordering::Acquire),
    );
    let address = network::get_host_ip();
    let ip = [
        network::ip4_addr1(address) as u8,
        network::ip4_addr2(address) as u8,
        network::ip4_addr3(address) as u8,
        network::ip4_addr4(address) as u8,
    ];
    let device = crate::platform::storage::Volume::from_settings_index(
        MENU_DEVICE.load(Ordering::Acquire) as i32,
    );
    crate::wiiload::reborn_status_json(crate::wiiload::RebornStatusSnapshot {
        version: env!("CARGO_PKG_VERSION"),
        network: status,
        ios: ogc_ios::get_version(),
        ios_revision: ogc_ios::get_revision(),
        ahbprot: ogc_system::has_ahbprot(),
        arena1_free: ogc_system::get_arena1_size(),
        arena2_free: ogc_system::get_arena2_size(),
        heap_free: crate::platform::allocator::WII_ALLOCATOR
            .free_bytes()
            .min(u32::MAX as usize) as u32,
        apps: MENU_APP_COUNT.load(Ordering::Acquire),
        device,
        inserted_mask: MENU_INSERTED.load(Ordering::Acquire),
        init_ms: MENU_INIT_MS.load(Ordering::Acquire),
        scan_ms: MENU_SCAN_MS.load(Ordering::Acquire),
        tcp_stack_size: THREAD_STACK_SIZE.min(u32::MAX as usize) as u32,
        tcp_stack_used: tcp_stack_used(),
        net_starts: successful_network_starts(),
        time: ogc_system::time(),
        ip,
        completed,
        total,
        transfer_style: transfer_style().as_str(),
        log: log_target(),
    })
}

fn tcp_stack_used() -> u32 {
    // SAFETY: This worker owns its static stack until it exits. It reads the
    // stack as raw bytes and does not create references to active stack data.
    let stack = unsafe { core::ptr::addr_of!(THREAD_STACK.0).cast::<u8>() };
    let low = 32usize;
    let high = THREAD_STACK_SIZE.saturating_sub(8);
    crate::wiiload::stack_high_water_used_bytes(low, high, |offset| unsafe {
        stack.add(offset).read_volatile()
    })
    .min(u32::MAX as usize) as u32
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
        } else if !socket_would_block(read) {
            // Zero means the peer closed its write side before the body ended.
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
