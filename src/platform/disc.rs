//! Non-blocking startup disc motor stop, matching Reborn's DI callback chain.

use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};

const IDLE: u32 = 0;
const OPENING: u32 = 1;
const SUBMITTING_STOP: u32 = 2;
const SUBMITTING_CLOSE: u32 = 3;
const COMPLETE: u32 = 4;
const FENCING: u32 = 5;

struct DiscStopState {
    phase: AtomicU32,
    descriptor: AtomicI32,
    error: AtomicI32,
}

impl DiscStopState {
    const fn new() -> Self {
        Self {
            phase: AtomicU32::new(IDLE),
            descriptor: AtomicI32::new(-1),
            error: AtomicI32::new(0),
        }
    }

    fn begin(&self) -> bool {
        self.phase
            .compare_exchange(IDLE, OPENING, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    fn opened(&self, result: i32) -> Option<i32> {
        if result < 0 {
            self.record_error(result);
            self.phase.store(FENCING, Ordering::Release);
            None
        } else {
            self.descriptor.store(result, Ordering::Relaxed);
            self.phase.store(SUBMITTING_STOP, Ordering::Release);
            Some(result)
        }
    }

    fn stop_submitted(&self, result: i32) -> Option<i32> {
        if result < 0 {
            self.record_error(result);
            self.phase.store(SUBMITTING_CLOSE, Ordering::Release);
            Some(self.descriptor.load(Ordering::Relaxed))
        } else {
            None
        }
    }

    fn stopped(&self, result: i32) -> i32 {
        if result < 0 {
            self.record_error(result);
        }
        self.phase.store(SUBMITTING_CLOSE, Ordering::Release);
        self.descriptor.load(Ordering::Relaxed)
    }

    fn close_submitted(&self, result: i32) {
        if result < 0 {
            self.record_error(result);
            self.phase.store(FENCING, Ordering::Release);
        }
    }

    fn closed(&self, result: i32) {
        if result < 0 {
            self.record_error(result);
        }
        if result >= 0 {
            self.descriptor.store(-1, Ordering::Relaxed);
        }
        self.phase.store(FENCING, Ordering::Release);
    }

    fn phase(&self) -> u32 {
        self.phase.load(Ordering::Acquire)
    }

    fn complete(&self) {
        self.phase.store(COMPLETE, Ordering::Release);
    }

    fn pending(&self) -> bool {
        matches!(
            self.phase(),
            OPENING | SUBMITTING_STOP | SUBMITTING_CLOSE | FENCING
        )
    }

    fn error(&self) -> i32 {
        self.error.load(Ordering::Acquire)
    }

    fn record_error(&self, result: i32) {
        if result < 0 {
            let _ = self
                .error
                .compare_exchange(0, result, Ordering::AcqRel, Ordering::Acquire);
        }
    }
}

#[cfg(target_arch = "powerpc")]
mod wii {
    use super::{COMPLETE, DiscStopState};
    use core::ffi::c_void;
    use core::sync::atomic::Ordering;
    #[cfg(feature = "emulator-acceptance")]
    use core::sync::atomic::{AtomicI32, AtomicU32};
    use ogc_ipc::{FileDescriptor, IoctlBuffers, open_file_async};
    use ogc_lwp::sys::{self, TICKS_PER_SECOND};

    const PATH: &core::ffi::CStr = c"/dev/di";
    const TIMEOUT_TICKS: u64 = 5 * TICKS_PER_SECOND;
    const TIMEOUT_ERROR: i32 = -110;

    #[repr(align(32))]
    struct AlignedWords(core::cell::UnsafeCell<[u32; 8]>);
    unsafe impl Sync for AlignedWords {}

    static INPUT: AlignedWords = AlignedWords(core::cell::UnsafeCell::new([0; 8]));
    static OUTPUT: AlignedWords = AlignedWords(core::cell::UnsafeCell::new([0; 8]));
    static STATE: DiscStopState = DiscStopState::new();
    #[cfg(feature = "emulator-acceptance")]
    static CALLBACKS: AtomicU32 = AtomicU32::new(0);
    #[cfg(feature = "emulator-acceptance")]
    static OPEN_RESULT: AtomicI32 = AtomicI32::new(i32::MIN);
    #[cfg(feature = "emulator-acceptance")]
    static STOP_RESULT: AtomicI32 = AtomicI32::new(i32::MIN);

    pub fn start_stop() -> Result<(), i32> {
        if !STATE.begin() {
            return Ok(());
        }
        let result = open_file_async(PATH, 0, Some(opened), core::ptr::null_mut());
        if result < 0 {
            STATE.record_error(result);
            STATE.phase.store(COMPLETE, Ordering::Release);
            return Err(result);
        }
        Ok(())
    }

    unsafe extern "C" fn opened(result: i32, _userdata: *mut c_void) -> i32 {
        #[cfg(feature = "emulator-acceptance")]
        CALLBACKS.fetch_or(1, Ordering::Release);
        #[cfg(feature = "emulator-acceptance")]
        OPEN_RESULT.store(result, Ordering::Release);
        let Some(fd) = STATE.opened(result) else {
            enqueue_completion_fence();
            return 0;
        };

        // IOS reads these aligned, static buffers until the callback returns.
        let input = unsafe { &mut *INPUT.0.get() };
        let output = unsafe { &mut *OUTPUT.0.get() };
        input.fill(0);
        output.fill(0);
        input[0] = 0xe300_0000;
        let buffers = IoctlBuffers::from_slices(
            Some(unsafe { core::slice::from_raw_parts_mut(input.as_mut_ptr().cast(), 0x20) }),
            Some(unsafe { core::slice::from_raw_parts_mut(output.as_mut_ptr().cast(), 0x20) }),
        );
        let result = unsafe {
            FileDescriptor::new(fd).ioctl_async(0xe3, buffers, Some(stopped), core::ptr::null_mut())
        };
        if let Some(fd) = STATE.stop_submitted(result) {
            submit_close(fd);
        }
        result
    }

    unsafe extern "C" fn stopped(result: i32, _userdata: *mut c_void) -> i32 {
        #[cfg(feature = "emulator-acceptance")]
        CALLBACKS.fetch_or(2, Ordering::Release);
        #[cfg(feature = "emulator-acceptance")]
        STOP_RESULT.store(result, Ordering::Release);
        let fd = STATE.stopped(result);
        submit_close(fd)
    }

    fn submit_close(fd: i32) -> i32 {
        let result =
            unsafe { FileDescriptor::new(fd).close_async(Some(closed), core::ptr::null_mut()) };
        STATE.close_submitted(result);
        if result < 0 {
            enqueue_completion_fence();
        }
        result
    }

    unsafe extern "C" fn closed(result: i32, _userdata: *mut c_void) -> i32 {
        #[cfg(feature = "emulator-acceptance")]
        CALLBACKS.fetch_or(4, Ordering::Release);
        STATE.closed(result);
        enqueue_completion_fence();
        0
    }

    fn enqueue_completion_fence() {
        // The deferred worker runs this after the current callback returns.
        // IPC frees its request after the callback and before the next item.
        let _ = unsafe { sys::deferred_call(Some(completion_fence), core::ptr::null_mut()) };
    }

    unsafe fn completion_fence(_userdata: *mut c_void) {
        STATE.complete();
    }

    pub fn shutdown() -> Result<i32, i32> {
        let start = sys::now_ticks();
        while STATE.pending() {
            ogc_ipc::poll();
            sys::yield_thread();
            if sys::now_ticks().wrapping_sub(start) >= TIMEOUT_TICKS {
                return Err(TIMEOUT_ERROR);
            }
        }

        let fd = STATE.descriptor.load(Ordering::Acquire);
        if fd >= 0 {
            FileDescriptor::new(fd).close()?;
            STATE.descriptor.store(-1, Ordering::Release);
        }
        if STATE.phase() != COMPLETE {
            return Err(-5);
        }
        Ok(STATE.error())
    }

    #[cfg(feature = "emulator-acceptance")]
    pub(crate) fn acceptance_report() -> (u32, i32, i32) {
        (
            CALLBACKS.load(Ordering::Acquire),
            OPEN_RESULT.load(Ordering::Acquire),
            STOP_RESULT.load(Ordering::Acquire),
        )
    }
}

#[cfg(target_arch = "powerpc")]
pub use wii::{shutdown, start_stop};

#[cfg(all(target_arch = "powerpc", feature = "emulator-acceptance"))]
pub(crate) use wii::acceptance_report;

#[cfg(test)]
mod tests {
    use super::{
        COMPLETE, DiscStopState, FENCING, IDLE, OPENING, SUBMITTING_CLOSE, SUBMITTING_STOP,
    };

    #[test]
    fn accepted_request_runs_open_stop_and_close_in_order() {
        let state = DiscStopState::new();
        assert!(state.begin());
        assert_eq!(state.phase(), OPENING);
        let fd = state.opened(7).unwrap();
        assert_eq!(fd, 7);
        assert_eq!(state.phase(), SUBMITTING_STOP);
        assert_eq!(state.stop_submitted(0), None);
        assert_eq!(state.stopped(0), 7);
        assert_eq!(state.phase(), SUBMITTING_CLOSE);
        state.close_submitted(0);
        assert_eq!(state.phase(), SUBMITTING_CLOSE);
        state.closed(0);
        assert_eq!(state.phase(), FENCING);
        state.complete();
        assert_eq!(state.phase(), COMPLETE);
        assert_eq!(state.error(), 0);
    }

    #[test]
    fn open_error_finishes_without_a_descriptor() {
        let state = DiscStopState::new();
        assert!(state.begin());
        assert_eq!(state.opened(-6), None);
        assert_eq!(state.phase(), FENCING);
        state.complete();
        assert_eq!(state.phase(), COMPLETE);
        assert_eq!(state.error(), -6);
    }

    #[test]
    fn stop_error_still_closes_the_open_descriptor() {
        let state = DiscStopState::new();
        assert!(state.begin());
        assert_eq!(state.opened(4), Some(4));
        assert_eq!(state.stop_submitted(-12), Some(4));
        state.close_submitted(0);
        assert_eq!(state.phase(), SUBMITTING_CLOSE);
        state.closed(0);
        state.complete();
        assert_eq!(state.phase(), COMPLETE);
        assert_eq!(state.error(), -12);
    }

    #[test]
    fn failed_close_submission_keeps_descriptor_for_sync_cleanup() {
        let state = DiscStopState::new();
        assert!(state.begin());
        assert_eq!(state.opened(9), Some(9));
        assert_eq!(state.stop_submitted(0), None);
        assert_eq!(state.stopped(0), 9);
        state.close_submitted(-12);
        assert_eq!(state.phase(), FENCING);
        assert_eq!(
            state.descriptor.load(core::sync::atomic::Ordering::Acquire),
            9
        );
        assert_eq!(state.error(), -12);
    }

    #[test]
    fn failed_close_callback_keeps_descriptor_for_sync_cleanup() {
        let state = DiscStopState::new();
        assert!(state.begin());
        assert_eq!(state.opened(11), Some(11));
        state.closed(-12);
        assert_eq!(state.phase(), FENCING);
        assert_eq!(
            state.descriptor.load(core::sync::atomic::Ordering::Acquire),
            11
        );
        assert_eq!(state.error(), -12);
    }

    #[test]
    fn startup_request_cannot_be_submitted_twice() {
        let state = DiscStopState::new();
        assert!(state.begin());
        assert!(!state.begin());
        assert_eq!(state.phase(), OPENING);
        assert_eq!(IDLE, 0);
    }

    #[test]
    fn synchronous_callback_completion_is_not_overwritten_by_submit_return() {
        let state = DiscStopState::new();
        assert!(state.begin());
        assert_eq!(state.opened(3), Some(3));
        state.stopped(0);
        state.closed(0);
        state.close_submitted(0);
        state.complete();
        assert_eq!(state.phase(), COMPLETE);
        assert!(!state.pending());
    }
}
