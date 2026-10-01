//! CPU framebuffer startup labels. GX never writes to this framebuffer.

const BACKGROUND: u32 = 0x3880_3880;
const WHITE: u32 = 0xeb80_eb80;
const GREEN: u32 = 0x9136_9122;
const RED: u32 = 0x525a_52f0;

fn paint(words: &mut [u32], width: usize, height: usize, label: &str, failed: bool) {
    let Some((words_per_row, length)) = framebuffer_layout(width, height) else {
        return;
    };
    let Some(words) = words.get_mut(..length) else {
        return;
    };
    words.fill(BACKGROUND);
    let status_color = if failed { RED } else { GREEN };
    for y in 16..32.min(height) {
        words[y * words_per_row..(y + 1) * words_per_row].fill(status_color);
    }
    // Even glyph scale keeps each YCbYCr pair in a single solid color.
    for (y, text) in [(56, "HBC RUST STARTUP"), (104, label)] {
        for (index, character) in text.chars().take(width.saturating_sub(48) / 24).enumerate() {
            for (row, bits) in crate::render::glyph_rows(character).into_iter().enumerate() {
                for column in 0..5 {
                    if bits & (1 << (4 - column)) == 0 {
                        continue;
                    }
                    let x = 24 + index * 24 + column * 4;
                    for py in y + row * 4..(y + row * 4 + 4).min(height) {
                        for word_x in x / 2..((x + 4).min(width)) / 2 {
                            words[py * words_per_row + word_x] = WHITE;
                        }
                    }
                }
            }
        }
    }
}

fn framebuffer_layout(width: usize, height: usize) -> Option<(usize, usize)> {
    let stride = width.checked_add(15)? & !15;
    let words_per_row = stride / 2;
    if width == 0 || height == 0 || words_per_row == 0 {
        return None;
    }
    Some((words_per_row, words_per_row.checked_mul(height)?))
}

pub(super) fn paint_scanout_probe(words: &mut [u32], width: usize, height: usize) {
    let Some((words_per_row, length)) = framebuffer_layout(width, height) else {
        return;
    };
    let Some(words) = words.get_mut(..length) else {
        return;
    };
    words.fill(BACKGROUND);
    let Some((left, top)) = test_square_origin(width as u16, height as u16) else {
        return;
    };
    let left = usize::from(left);
    let top = usize::from(top);
    let right = left + usize::from(TEST_SQUARE_SIZE);
    let bottom = top + usize::from(TEST_SQUARE_SIZE);
    let border = 8;
    if left < border || top < border || right + border > width || bottom + border > height {
        return;
    }
    for y in top - border..bottom + border {
        for word_x in (left - border) / 2..(right + border) / 2 {
            words[y * words_per_row + word_x] = RED;
        }
    }
    for y in top..bottom {
        for word_x in left / 2..right / 2 {
            words[y * words_per_row + word_x] = WHITE;
        }
    }
}

pub(super) fn has_visible_luma(value: u8) -> bool {
    value > 24
}

pub(super) fn has_bright_luma(value: u8) -> bool {
    value > 200
}

pub const TEST_SQUARE_SIZE: u16 = 32;
const REPORT_NETWORK_INIT_ATTEMPTS: usize = 32;

fn retry_report_network(mut initialize: impl FnMut() -> bool, mut wait: impl FnMut()) -> bool {
    for attempt in 0..REPORT_NETWORK_INIT_ATTEMPTS {
        if initialize() {
            return true;
        }
        if attempt + 1 < REPORT_NETWORK_INIT_ATTEMPTS {
            wait();
        }
    }
    false
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReportIdentity {
    host: [u8; 4],
    port: u16,
    run_id: [u8; 64],
    run_id_len: u8,
    build_id: [u8; 64],
    restart_after_report: bool,
    warmup_frames: u32,
    sample_frames: u32,
}

const PROGRESS_TEXT_LIMIT: usize = 256;

fn progress_packet(identity: &ReportIdentity, sequence: u32, text: &str) -> ([u8; 408], usize) {
    let mut packet = [0u8; 408];
    let text_len = text.len().min(PROGRESS_TEXT_LIMIT);
    packet[..4].copy_from_slice(b"WIP1");
    packet[4..6].copy_from_slice(&1u16.to_be_bytes());
    packet[6..8].copy_from_slice(&u16::from(text.len() > text_len).to_be_bytes());
    packet[8..12].copy_from_slice(&sequence.to_be_bytes());
    packet[12..16].copy_from_slice(&8u32.to_be_bytes());
    packet[16..18].copy_from_slice(&u16::from(identity.run_id_len).to_be_bytes());
    packet[18..20].copy_from_slice(&64u16.to_be_bytes());
    packet[20..24].copy_from_slice(&(text_len as u32).to_be_bytes());
    let run_end = 24 + usize::from(identity.run_id_len);
    packet[24..run_end].copy_from_slice(&identity.run_id[..usize::from(identity.run_id_len)]);
    let build_end = run_end + 64;
    packet[run_end..build_end].copy_from_slice(&identity.build_id);
    let end = build_end + text_len;
    packet[build_end..end].copy_from_slice(&text.as_bytes()[..text_len]);
    (packet, end)
}

fn parse_report_identity(args: &[&str]) -> Option<ReportIdentity> {
    let mut endpoint = None;
    let mut run_id = None;
    let mut build_id = None;
    let mut restart_after_report = false;
    let mut warmup_frames = 0;
    let mut sample_frames = 1;
    for arg in args {
        if let Some(value) = arg.strip_prefix("--report=") {
            endpoint = Some(value);
        } else if let Some(value) = arg.strip_prefix("--run-id=") {
            run_id = Some(value);
        } else if let Some(value) = arg.strip_prefix("--build-id=") {
            build_id = Some(value);
        } else if *arg == "--restart-after-probe" {
            restart_after_report = true;
        } else if let Some(value) = arg.strip_prefix("--warmup-frames=") {
            warmup_frames = value.parse().ok()?;
        } else if let Some(value) = arg.strip_prefix("--sample-frames=") {
            sample_frames = value.parse().ok()?;
        }
    }
    let (host, port) = parse_report_endpoint(endpoint?)?;
    let run_id = run_id?;
    let build_id = build_id?;
    if run_id.is_empty()
        || run_id.len() > 64
        || !run_id.is_ascii()
        || build_id.len() != 64
        || !build_id.bytes().all(|byte| byte.is_ascii_hexdigit())
        || warmup_frames > 10_000
        || sample_frames == 0
        || sample_frames > 10_000
    {
        return None;
    }
    let mut run_id_bytes = [0; 64];
    run_id_bytes[..run_id.len()].copy_from_slice(run_id.as_bytes());
    let mut build_id_bytes = [0; 64];
    build_id_bytes.copy_from_slice(build_id.as_bytes());
    Some(ReportIdentity {
        host,
        port,
        run_id: run_id_bytes,
        run_id_len: run_id.len() as u8,
        build_id: build_id_bytes,
        restart_after_report,
        warmup_frames,
        sample_frames,
    })
}

fn parse_report_endpoint(value: &str) -> Option<([u8; 4], u16)> {
    let (address, port) = value.split_once(':')?;
    let mut octets = [0; 4];
    let mut parts = address.split('.');
    for octet in &mut octets {
        *octet = parts.next()?.parse().ok()?;
    }
    if parts.next().is_some() {
        return None;
    }
    let port = port.parse().ok()?;
    (port != 0).then_some((octets, port))
}

pub const fn test_square_origin(width: u16, height: u16) -> Option<(u16, u16)> {
    if width < TEST_SQUARE_SIZE || height < TEST_SQUARE_SIZE {
        return None;
    }
    Some((
        (width - TEST_SQUARE_SIZE) / 2,
        (height - TEST_SQUARE_SIZE) / 2,
    ))
}

pub(super) fn test_square_is_white(
    width: u16,
    height: u16,
    mut read_luma: impl FnMut(u16, u16) -> u8,
) -> bool {
    // Check the interior. White text or a logo elsewhere must not pass this check.
    let Some((left, top)) = test_square_origin(width, height) else {
        return false;
    };
    let step = TEST_SQUARE_SIZE / 4;
    (1..4).all(|row| {
        (1..4).all(|column| has_bright_luma(read_luma(left + column * step, top + row * step)))
    })
}

#[cfg(target_arch = "powerpc")]
mod wii {
    use super::ReportIdentity;
    use core::{
        ffi::c_void,
        sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    static FRAMEBUFFER: AtomicUsize = AtomicUsize::new(0);
    #[no_mangle]
    pub static GX_FRAMEBUFFER: AtomicUsize = AtomicUsize::new(0);
    static WIDTH: AtomicUsize = AtomicUsize::new(0);
    static HEIGHT: AtomicUsize = AtomicUsize::new(0);
    static LENGTH: AtomicUsize = AtomicUsize::new(0);
    static DRAWING: AtomicBool = AtomicBool::new(false);

    /// Call on the main thread after system initialization and before GX setup.
    pub fn initialize(
        argc: i32,
        argv: *mut *mut u8,
    ) -> Result<(), crate::platform::video::VideoError> {
        use crate::platform::video::VideoError;
        unsafe { initialize_report_identity(argc, argv) };
        ogc_video::init();
        let preferred = unsafe { ogc_video::get_preferred_mode(core::ptr::null_mut()) };
        if preferred.is_null() {
            return Err(VideoError::NoPreferredMode);
        }
        let mut mode = unsafe { *preferred };
        let framebuffer = unsafe { ogc_system::allocate_framebuffer(&mut mode) };
        if framebuffer.is_null() {
            return Err(VideoError::FramebufferAllocationFailed);
        }
        let bytes = unsafe { ogc_video::get_frame_buffer_size(&mut mode) };
        unsafe { ogc_cache::dc_invalidate_range(framebuffer.cast_const(), bytes) };
        WIDTH.store(usize::from(mode.fb_width), Ordering::Relaxed);
        HEIGHT.store(usize::from(mode.xfb_height), Ordering::Relaxed);
        LENGTH.store(bytes as usize / 4, Ordering::Relaxed);
        FRAMEBUFFER.store(
            ogc_core::mem_k0_to_k1(framebuffer as usize),
            Ordering::Release,
        );
        unsafe { ogc_video::configure(&mut mode) };
        mark("CPU VIDEO READY");
        hold();
        Ok(())
    }

    /// Startup labels are written by the main thread. Panic reporting uses a
    /// nonblocking guard so it cannot re-enter a framebuffer write.
    fn show(label: &str, failed: bool) {
        record_stage(label, failed);
        let framebuffer = FRAMEBUFFER.load(Ordering::Acquire);
        if framebuffer == 0 || DRAWING.swap(true, Ordering::Acquire) {
            return;
        }
        // This allocation is reserved for diagnostics for the entire run.
        // Only this guarded CPU writer accesses it; GX uses a different XFB.
        let words = unsafe {
            core::slice::from_raw_parts_mut(framebuffer as *mut u32, LENGTH.load(Ordering::Relaxed))
        };
        super::paint(
            words,
            WIDTH.load(Ordering::Relaxed),
            HEIGHT.load(Ordering::Relaxed),
            label,
            failed,
        );
        unsafe {
            core::arch::asm!("sync", options(nostack, preserves_flags));
            ogc_video::set_next_framebuffer(framebuffer as *mut c_void);
        }
        ogc_video::set_black(false);
        ogc_video::flush();
        DRAWING.store(false, Ordering::Release);
    }

    pub fn mark(label: &str) {
        show(label, false);
        send_progress(label);
    }

    /// Establish reporting before NAND/theme loading can block startup.
    pub fn begin_reporting() {
        if !REPORT_READY.load(Ordering::Acquire) {
            return;
        }
        show("REPORT NETWORK START", false);
        let identity = unsafe { core::ptr::addr_of!(REPORT_IDENTITY).read() };
        let socket = connect_report_socket(&identity);
        if socket >= 0 {
            PROGRESS_SOCKET.store(socket, Ordering::Release);
            PROGRESS_ENABLED.store(true, Ordering::Release);
            mark("REPORT CHANNEL READY");
        } else {
            show(report_error_label(), true);
        }
    }

    pub fn fail(label: &str) -> bool {
        show(label, true);
        if !send_report(false) && REPORT_READY.load(Ordering::Acquire) {
            show(report_error_label(), true);
            hold_for_report_error();
        }
        report_restart_requested()
    }

    pub fn complete() {
        record_stage("FIRST HBC FRAME DISPLAYED", false);
        unsafe {
            let now = ogc_core::powerpc::read_timebase();
            REPORT_START_TICKS = now;
            LAST_FRAME_TICKS = now;
            FRAME_INDEX = 0;
            SAMPLE_COUNT = 0;
            SAMPLE_TOTAL_TICKS = 0;
            SAMPLE_MIN_TICKS = u64::MAX;
            SAMPLE_MAX_TICKS = 0;
        }
    }

    /// Collect frame intervals after the first HBC frame. Returns true after
    /// the report is sent when the caller requested a restart.
    pub fn sample_frame() -> bool {
        if !REPORT_READY.load(Ordering::Acquire) || REPORT_SENT.load(Ordering::Acquire) {
            return false;
        }
        let now = ogc_core::powerpc::read_timebase();
        let done = unsafe {
            let delta = now.wrapping_sub(LAST_FRAME_TICKS);
            LAST_FRAME_TICKS = now;
            FRAME_INDEX = FRAME_INDEX.saturating_add(1);
            let identity = core::ptr::addr_of!(REPORT_IDENTITY).read();
            if FRAME_INDEX > identity.warmup_frames {
                SAMPLE_COUNT = SAMPLE_COUNT.saturating_add(1);
                SAMPLE_TOTAL_TICKS = SAMPLE_TOTAL_TICKS.saturating_add(delta);
                SAMPLE_MIN_TICKS = SAMPLE_MIN_TICKS.min(delta);
                SAMPLE_MAX_TICKS = SAMPLE_MAX_TICKS.max(delta);
                SAMPLE_COUNT >= identity.sample_frames
            } else {
                false
            }
        };
        if done {
            if !send_report(true) {
                show(report_error_label(), true);
                hold_for_report_error();
            }
            report_restart_requested()
        } else {
            false
        }
    }

    /// Keep a completed stage visible before switching to the GX framebuffer.
    pub fn hold() {
        for _ in 0..90 {
            ogc_video::wait_vsync();
        }
    }

    const REPORT_LOG_CAPACITY: usize = 4096;
    static REPORT_LOG: [core::sync::atomic::AtomicU8; REPORT_LOG_CAPACITY] =
        [const { core::sync::atomic::AtomicU8::new(0) }; REPORT_LOG_CAPACITY];
    static REPORT_LOG_LEN: AtomicUsize = AtomicUsize::new(0);
    static REPORT_SENT: AtomicBool = AtomicBool::new(false);
    static REPORT_READY: AtomicBool = AtomicBool::new(false);
    static REPORT_ERROR: AtomicUsize = AtomicUsize::new(0);
    static REPORT_NETWORK_READY: AtomicBool = AtomicBool::new(false);
    static PROGRESS_ENABLED: AtomicBool = AtomicBool::new(false);
    static PROGRESS_SEQUENCE: AtomicUsize = AtomicUsize::new(0);
    static PROGRESS_SOCKET: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(-1);
    static mut REPORT_IDENTITY: ReportIdentity = ReportIdentity {
        host: [0; 4],
        port: 0,
        run_id: [0; 64],
        run_id_len: 0,
        build_id: [0; 64],
        restart_after_report: false,
        warmup_frames: 0,
        sample_frames: 1,
    };
    static mut REPORT_START_TICKS: u64 = 0;
    static mut LAST_FRAME_TICKS: u64 = 0;
    static mut FRAME_INDEX: u32 = 0;
    static mut SAMPLE_COUNT: u32 = 0;
    static mut SAMPLE_TOTAL_TICKS: u64 = 0;
    static mut SAMPLE_MIN_TICKS: u64 = u64::MAX;
    static mut SAMPLE_MAX_TICKS: u64 = 0;
    static GX_FRAMEBUFFER_BYTES: AtomicUsize = AtomicUsize::new(0);
    static GX_FRAMEBUFFER_WIDTH: AtomicUsize = AtomicUsize::new(0);
    static GX_FRAMEBUFFER_HEIGHT: AtomicUsize = AtomicUsize::new(0);

    const REPORT_ERROR_NONE: usize = 0;
    const REPORT_ERROR_NET_INIT: usize = 1;
    const REPORT_ERROR_SOCKET: usize = 2;
    const REPORT_ERROR_CONNECT: usize = 3;
    const REPORT_ERROR_WRITE: usize = 4;

    fn report_error_label() -> &'static str {
        match REPORT_ERROR.load(Ordering::Acquire) {
            REPORT_ERROR_NET_INIT => "REPORT NET INIT FAIL",
            REPORT_ERROR_SOCKET => "REPORT SOCKET FAIL",
            REPORT_ERROR_CONNECT => "REPORT CONNECT FAIL",
            REPORT_ERROR_WRITE => "REPORT WRITE FAIL",
            _ => "REPORT SEND FAIL",
        }
    }

    fn hold_for_report_error() {
        for _ in 0..180 {
            ogc_video::wait_vsync();
        }
    }

    unsafe fn initialize_report_identity(argc: i32, argv: *mut *mut u8) {
        if argc <= 0 || argv.is_null() {
            return;
        }
        let mut args = [&""[..]; 64];
        let mut arg_count = 0;
        for index in 0..(argc as usize).min(args.len()) {
            let arg = unsafe { *argv.add(index) };
            if arg.is_null() {
                break;
            }
            let bytes = unsafe { core::ffi::CStr::from_ptr(arg.cast()) }.to_bytes();
            let Ok(value) = core::str::from_utf8(bytes) else {
                continue;
            };
            args[arg_count] = value;
            arg_count += 1;
        }
        if let Some(identity) = super::parse_report_identity(&args[..arg_count]) {
            unsafe { core::ptr::addr_of_mut!(REPORT_IDENTITY).write(identity) };
            unsafe {
                let now = ogc_core::powerpc::read_timebase();
                REPORT_START_TICKS = now;
                LAST_FRAME_TICKS = now;
            }
            REPORT_READY.store(true, Ordering::Release);
        }
    }

    fn record_stage(label: &str, failed: bool) {
        let prefix = if failed { b"FAIL " } else { b"INFO " };
        let current = REPORT_LOG_LEN.load(Ordering::Relaxed);
        let end = current.saturating_add(prefix.len() + label.len() + 1);
        if end > REPORT_LOG_CAPACITY {
            return;
        }
        for (offset, byte) in prefix
            .iter()
            .chain(label.as_bytes())
            .chain(b"\n")
            .enumerate()
        {
            REPORT_LOG[current + offset].store(*byte, Ordering::Relaxed);
        }
        REPORT_LOG_LEN.store(end, Ordering::Release);
    }

    fn send_report(passed: bool) -> bool {
        use ogc_network as network;

        if !REPORT_READY.load(Ordering::Acquire) || REPORT_SENT.swap(true, Ordering::AcqRel) {
            return false;
        }
        REPORT_ERROR.store(REPORT_ERROR_NONE, Ordering::Release);
        let identity = unsafe { core::ptr::addr_of!(REPORT_IDENTITY).read() };
        let framebuffer = if GX_FRAMEBUFFER.load(Ordering::Acquire) != 0 {
            ogc_core::mem_k0_to_k1(GX_FRAMEBUFFER.load(Ordering::Acquire))
        } else {
            FRAMEBUFFER.load(Ordering::Acquire)
        };
        let (width, height, framebuffer_bytes) = if GX_FRAMEBUFFER.load(Ordering::Acquire) != 0 {
            (
                GX_FRAMEBUFFER_WIDTH.load(Ordering::Acquire),
                GX_FRAMEBUFFER_HEIGHT.load(Ordering::Acquire),
                GX_FRAMEBUFFER_BYTES.load(Ordering::Acquire),
            )
        } else {
            (
                WIDTH.load(Ordering::Acquire),
                HEIGHT.load(Ordering::Acquire),
                LENGTH.load(Ordering::Acquire).saturating_mul(4),
            )
        };
        let log_len = REPORT_LOG_LEN
            .load(Ordering::Acquire)
            .min(REPORT_LOG_CAPACITY);
        let mut header = [0u8; 156];
        header[..4].copy_from_slice(b"WIR2");
        header[4..6].copy_from_slice(&2u16.to_be_bytes());
        header[8..12].copy_from_slice(&(log_len as u32).to_be_bytes());
        header[12..16].copy_from_slice(&(framebuffer_bytes as u32).to_be_bytes());
        header[16..20].copy_from_slice(&(width as u32).to_be_bytes());
        header[20..24].copy_from_slice(&(height as u32).to_be_bytes());
        header[24..28].copy_from_slice(&36u32.to_be_bytes());
        header[28..28 + identity.run_id_len as usize]
            .copy_from_slice(&identity.run_id[..identity.run_id_len as usize]);
        header[92..156].copy_from_slice(&identity.build_id);

        let summary = if passed {
            b"PASS summary passes=1 fails=0\n".as_slice()
        } else {
            b"FAIL summary passes=0 fails=1\n".as_slice()
        };
        let summary_start = log_len.min(REPORT_LOG_CAPACITY.saturating_sub(summary.len()));
        let report_log_len = summary_start + summary.len();
        header[8..12].copy_from_slice(&(report_log_len as u32).to_be_bytes());
        let mut log = [0u8; REPORT_LOG_CAPACITY];
        for (index, byte) in log[..log_len].iter_mut().enumerate() {
            *byte = REPORT_LOG[index].load(Ordering::Relaxed);
        }
        log[summary_start..report_log_len].copy_from_slice(summary);
        let timer_clock_hz = ogc_video::timer_clock_hz();
        let ticks_to_us = |ticks| ogc_video::ticks_to_microseconds(ticks, timer_clock_hz);
        let (elapsed_ticks, sample_count, total_ticks, min_ticks, max_ticks) = unsafe {
            (
                ogc_core::powerpc::read_timebase().wrapping_sub(REPORT_START_TICKS),
                SAMPLE_COUNT,
                SAMPLE_TOTAL_TICKS,
                SAMPLE_MIN_TICKS,
                SAMPLE_MAX_TICKS,
            )
        };
        let average_ticks = if sample_count == 0 {
            0
        } else {
            total_ticks / u64::from(sample_count)
        };
        let heap_used_bytes = crate::platform::allocator::WII_ALLOCATOR
            .used_bytes()
            .min(u32::MAX as usize);
        let mut performance = [0u8; 36];
        performance[..8].copy_from_slice(&ticks_to_us(elapsed_ticks).to_be_bytes());
        for (index, value) in [
            sample_count,
            heap_used_bytes.min(u32::MAX as usize) as u32,
            identity.warmup_frames,
            identity.sample_frames,
            ticks_to_us(average_ticks).min(u64::from(u32::MAX)) as u32,
            ticks_to_us(if min_ticks == u64::MAX { 0 } else { min_ticks }).min(u64::from(u32::MAX))
                as u32,
            ticks_to_us(max_ticks).min(u64::from(u32::MAX)) as u32,
        ]
        .into_iter()
        .enumerate()
        {
            let start = 8 + index * 4;
            performance[start..start + 4].copy_from_slice(&value.to_be_bytes());
        }

        let progress_socket = PROGRESS_SOCKET.swap(-1, Ordering::AcqRel);
        let socket = if progress_socket >= 0 {
            progress_socket
        } else {
            connect_report_socket(&identity)
        };
        if socket < 0 {
            return false;
        }
        let framebuffer = if framebuffer == 0 || framebuffer_bytes == 0 {
            &[][..]
        } else {
            unsafe { core::slice::from_raw_parts(framebuffer as *const u8, framebuffer_bytes) }
        };
        let sent = write_all(socket, &header)
            && write_all(socket, &log[..report_log_len])
            && write_all(socket, framebuffer)
            && write_all(socket, &performance);
        if !sent {
            REPORT_ERROR.store(REPORT_ERROR_WRITE, Ordering::Release);
        }
        PROGRESS_ENABLED.store(false, Ordering::Release);
        network::close(socket);
        sent
    }

    fn initialize_report_network() -> bool {
        use ogc_network as network;
        if REPORT_NETWORK_READY.load(Ordering::Acquire) {
            return true;
        }
        let network_ready = super::retry_report_network(
            || {
                // Prefer the regular libogc startup path. Attach to an active IOS
                // stack when NWC24 or SO_STARTUP reports that it is already open.
                network::init() >= 0 || network::attach_active() >= 0
            },
            || {
                for _ in 0..12 {
                    ogc_video::wait_vsync();
                }
            },
        );
        if !network_ready {
            REPORT_ERROR.store(REPORT_ERROR_NET_INIT, Ordering::Release);
            return false;
        }
        REPORT_NETWORK_READY.store(true, Ordering::Release);
        true
    }

    fn connect_report_socket(identity: &ReportIdentity) -> i32 {
        use ogc_network as network;
        if !initialize_report_network() {
            return -1;
        }
        let socket = network::socket(network::PF_INET, network::SOCK_STREAM, 0);
        if socket < 0 {
            REPORT_ERROR.store(REPORT_ERROR_SOCKET, Ordering::Release);
            return socket;
        }
        let timeout = network::Timeval {
            tv_sec: 2,
            tv_usec: 0,
        };
        unsafe {
            network::setsockopt(
                socket,
                network::SOL_SOCKET,
                network::SO_SNDTIMEO,
                (&timeout as *const network::Timeval).cast(),
                core::mem::size_of::<network::Timeval>() as network::SockLenT,
            );
        }
        let address = network::SockaddrIn {
            sin_len: core::mem::size_of::<network::SockaddrIn>() as u8,
            sin_family: network::AF_INET as u8,
            sin_port: identity.port.to_be(),
            sin_addr: network::InAddr {
                s_addr: network::ip4_addr(
                    identity.host[0],
                    identity.host[1],
                    identity.host[2],
                    identity.host[3],
                ),
            },
            sin_zero: [0; 8],
        };
        if network::connect_addr(socket, &address) != 0 {
            REPORT_ERROR.store(REPORT_ERROR_CONNECT, Ordering::Release);
            network::close(socket);
            return -1;
        }
        socket
    }

    fn send_progress(label: &str) {
        if !PROGRESS_ENABLED.load(Ordering::Acquire) || REPORT_SENT.load(Ordering::Acquire) {
            return;
        }
        let identity = unsafe { core::ptr::addr_of!(REPORT_IDENTITY).read() };
        let sequence = PROGRESS_SEQUENCE.fetch_add(1, Ordering::Relaxed) as u32 + 1;
        let (packet, length) = super::progress_packet(&identity, sequence, label);
        let socket = PROGRESS_SOCKET.load(Ordering::Acquire);
        if socket < 0 {
            PROGRESS_ENABLED.store(false, Ordering::Release);
            return;
        }
        let sent = socket >= 0 && write_all(socket, &packet[..length]);
        if !sent {
            PROGRESS_ENABLED.store(false, Ordering::Release);
            if PROGRESS_SOCKET
                .compare_exchange(socket, -1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                ogc_network::close(socket);
            }
            show("REPORT PROGRESS SEND FAIL", true);
        }
    }

    fn report_restart_requested() -> bool {
        if !REPORT_READY.load(Ordering::Acquire) {
            return false;
        }
        unsafe {
            core::ptr::addr_of!(REPORT_IDENTITY)
                .read()
                .restart_after_report
        }
    }

    fn write_all(socket: i32, mut bytes: &[u8]) -> bool {
        while !bytes.is_empty() {
            let written = ogc_network::write_slice(socket, bytes);
            if written <= 0 {
                return false;
            }
            bytes = &bytes[written as usize..];
        }
        true
    }

    pub fn set_gx_framebuffer_info(framebuffer: usize, width: u16, height: u16, bytes: u32) {
        GX_FRAMEBUFFER.store(framebuffer, Ordering::Release);
        GX_FRAMEBUFFER_WIDTH.store(usize::from(width), Ordering::Release);
        GX_FRAMEBUFFER_HEIGHT.store(usize::from(height), Ordering::Release);
        GX_FRAMEBUFFER_BYTES.store(bytes as usize, Ordering::Release);
    }
}

#[cfg(target_arch = "powerpc")]
pub use wii::{
    begin_reporting, complete, fail, hold, initialize, mark, sample_frame, set_gx_framebuffer_info,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_progress_uses_the_host_wip1_identity_and_length_fields() {
        let identity = parse_report_identity(&[
            "--report=10.0.0.238:4298",
            "--run-id=freeze-test",
            "--build-id=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ])
        .unwrap();
        let (packet, length) = progress_packet(&identity, 3, "THEME PATH LOOKUP");
        assert_eq!(&packet[..8], b"WIP1\0\x01\0\0");
        assert_eq!(u32::from_be_bytes(packet[8..12].try_into().unwrap()), 3);
        assert_eq!(u32::from_be_bytes(packet[12..16].try_into().unwrap()), 8);
        assert_eq!(u16::from_be_bytes(packet[16..18].try_into().unwrap()), 11);
        assert_eq!(u16::from_be_bytes(packet[18..20].try_into().unwrap()), 64);
        assert_eq!(&packet[24..35], b"freeze-test");
        assert_eq!(&packet[35..99], &identity.build_id);
        assert_eq!(&packet[99..length], b"THEME PATH LOOKUP");
        let (truncated, length) = progress_packet(&identity, 4, &"A".repeat(300));
        assert_eq!(u16::from_be_bytes(truncated[6..8].try_into().unwrap()), 1);
        assert_eq!(length, 99 + 256);
    }

    #[test]
    fn status_screen_respects_padded_rows_and_allocation_bounds() {
        let mut words = [0xdead_beef; 80 * 128 + 4];
        paint(&mut words, 145, 128, "A", false);
        assert_eq!(words[16 * 80], 0x9136_9122);
        // A's first bitmap row is 01110, at x=24 with scale four.
        assert_eq!(words[104 * 80 + 12], 0x3880_3880);
        assert_eq!(words[104 * 80 + 14], 0xeb80_eb80);
        assert_eq!(&words[80 * 128..], &[0xdead_beef; 4]);
    }

    #[test]
    fn scanout_probe_paints_gray_frame_and_white_square() {
        let mut words = [0xdead_beef; 80 * 128 + 4];
        paint_scanout_probe(&mut words, 145, 128);
        assert_eq!(words[0], BACKGROUND);
        assert_eq!(words[40 * 80 + 24], RED);
        assert_eq!(words[48 * 80 + 28], WHITE);
        assert_eq!(words[16 * 80 + 28], BACKGROUND);
        assert_eq!(&words[80 * 128..], &[0xdead_beef; 4]);
    }

    #[test]
    fn failure_screen_is_red_and_short_buffers_are_unchanged() {
        let mut words = [0; 32 * 40];
        paint(&mut words, 64, 40, "ERROR", true);
        assert_eq!(words[16 * 32], 0x525a_52f0);
        let mut short = [0xdead_beef; 10];
        paint(&mut short, 64, 40, "ERROR", true);
        assert_eq!(short, [0xdead_beef; 10]);
    }

    #[test]
    fn ycb_ycr_black_chroma_does_not_count_as_visible_luma() {
        let black = [0x00, 0x80, 0x00, 0x80];
        assert!(!black.iter().step_by(2).copied().any(has_visible_luma));

        let dark_gray = [0x38, 0x80, 0x38, 0x80];
        assert!(dark_gray.iter().step_by(2).copied().any(has_visible_luma));
    }

    #[test]
    fn bright_luma_check_distinguishes_white_from_black() {
        assert!(!has_bright_luma(0));
        assert!(!has_bright_luma(200));
        assert!(has_bright_luma(235));
    }

    #[test]
    fn parses_wiiload_report_identity() {
        let identity = parse_report_identity(&[
            "--report=10.0.0.238:4298",
            "--run-id=1234-5678",
            &format!("--build-id={}", "a".repeat(64)),
            "--restart-after-probe",
            "--warmup-frames=2",
            "--sample-frames=3",
        ])
        .unwrap();
        assert_eq!(identity.host, [10, 0, 0, 238]);
        assert_eq!(identity.port, 4298);
        assert!(identity.restart_after_report);
        assert_eq!(identity.warmup_frames, 2);
        assert_eq!(identity.sample_frames, 3);
        assert_eq!(
            &identity.run_id[..identity.run_id_len as usize],
            b"1234-5678"
        );
    }

    #[test]
    fn rejects_invalid_wiiload_report_identity() {
        assert!(parse_report_identity(&[
            "--report=999.0.0.1:4298",
            "--run-id=test",
            &format!("--build-id={}", "a".repeat(64)),
        ])
        .is_none());
        assert!(parse_report_identity(&[
            "--report=10.0.0.1:0",
            "--run-id=test",
            &format!("--build-id={}", "a".repeat(64)),
        ])
        .is_none());
    }

    #[test]
    fn report_network_startup_retries_until_ready_and_bounds_waits() {
        let mut attempts = 0;
        let mut waits = 0;
        assert!(retry_report_network(
            || {
                attempts += 1;
                attempts == 3
            },
            || waits += 1,
        ));
        assert_eq!(attempts, 3);
        assert_eq!(waits, 2);

        attempts = 0;
        waits = 0;
        assert!(!retry_report_network(
            || {
                attempts += 1;
                false
            },
            || waits += 1,
        ));
        assert_eq!(attempts, REPORT_NETWORK_INIT_ATTEMPTS);
        assert_eq!(waits, REPORT_NETWORK_INIT_ATTEMPTS - 1);
    }

    #[test]
    fn test_square_check_rejects_white_elsewhere_and_partial_drawing() {
        assert_eq!(test_square_origin(640, 480), Some((304, 224)));
        assert_eq!(test_square_origin(852, 480), Some((410, 224)));
        assert_eq!(test_square_origin(31, 480), None);
        assert!(!test_square_is_white(640, 480, |_, y| if y >= 416 {
            235
        } else {
            63
        }));
        assert!(!test_square_is_white(640, 480, |x, y| {
            if x == 320 && y == 240 {
                63
            } else {
                235
            }
        }));
        assert!(test_square_is_white(640, 480, |_, _| 235));
    }
}
