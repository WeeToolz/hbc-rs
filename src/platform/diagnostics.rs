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

pub const fn test_square_origin(width: u16, height: u16) -> Option<(u16, u16)> {
    if width < TEST_SQUARE_SIZE || height < TEST_SQUARE_SIZE {
        return None;
    }
    Some(((width - TEST_SQUARE_SIZE) / 2, (height - TEST_SQUARE_SIZE) / 2))
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
        (1..4).all(|column| {
            has_bright_luma(read_luma(left + column * step, top + row * step))
        })
    })
}

#[cfg(target_arch = "powerpc")]
mod wii {
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

    pub fn set_gx_framebuffer(framebuffer: usize) {
        GX_FRAMEBUFFER.store(framebuffer, Ordering::Release);
    }

    /// Call on the main thread after system initialization and before GX setup.
    pub fn initialize() -> Result<(), crate::platform::video::VideoError> {
        use crate::platform::video::VideoError;
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
    }

    pub fn fail(label: &str) {
        show(label, true);
    }

    /// Keep a completed stage visible before switching to the GX framebuffer.
    pub fn hold() {
        for _ in 0..90 {
            ogc_video::wait_vsync();
        }
    }
}

#[cfg(target_arch = "powerpc")]
pub use wii::{fail, hold, initialize, mark, set_gx_framebuffer};

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_square_check_rejects_white_elsewhere_and_partial_drawing() {
        assert_eq!(test_square_origin(640, 480), Some((304, 224)));
        assert_eq!(test_square_origin(852, 480), Some((410, 224)));
        assert_eq!(test_square_origin(31, 480), None);
        assert!(!test_square_is_white(640, 480, |_, y| if y >= 416 {
            235
        } else {
            63
        }));
        assert!(!test_square_is_white(640, 480, |x, y| if x == 320 && y == 240 {
            63
        } else {
            235
        }));
        assert!(test_square_is_white(640, 480, |_, _| 235));
    }
}
