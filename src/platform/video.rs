//! Wii video initialization.
//!
//! This adapter sets the VI mode and framebuffer through `ogc-video`. The
//! current `ogc-gx` API does not provide GX initialization or draw-state calls,
//! so this module does not initialize GX. HBC's GX viewport, copy, projection,
//! vertex descriptor, and draw setup remain blocked on Rust GX APIs.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FramebufferInfo {
    pub width: u16,
    pub height: u16,
    pub byte_len: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VideoError {
    NoPreferredMode,
    FramebufferAllocationFailed,
}

/// Initialize VI and select a cleared framebuffer.
///
/// Width and height describe the selected XFB mode. HBC's separate 640x480
/// or 852x480 logical UI dimensions require aspect-ratio configuration and
/// remain caller policy.
pub fn initialize() -> Result<FramebufferInfo, VideoError> {
    ogc_video::init();
    ogc_video::set_black(true);

    let preferred = unsafe { ogc_video::get_preferred_mode(core::ptr::null_mut()) };
    if preferred.is_null() {
        return Err(VideoError::NoPreferredMode);
    }
    let mut mode = unsafe { *preferred };

    unsafe { ogc_video::configure(&mut mode) };
    ogc_video::flush();
    ogc_video::wait_vsync();

    let framebuffer = unsafe { ogc_system::allocate_framebuffer(&mut mode) };
    if framebuffer.is_null() {
        return Err(VideoError::FramebufferAllocationFailed);
    }

    let byte_len = unsafe { ogc_video::get_frame_buffer_size(&mut mode) };
    unsafe {
        ogc_cache::dc_invalidate_range(framebuffer.cast_const(), byte_len);
        // COLOR_BLACK from HBC is a zero-valued YUYV framebuffer color.
        ogc_video::clear_frame_buffer(&mut mode, framebuffer, 0);
        ogc_video::set_next_framebuffer(framebuffer);
    }
    ogc_video::flush();
    for _ in 0..4 {
        ogc_video::wait_vsync();
    }

    Ok(FramebufferInfo {
        width: mode.fb_width,
        height: mode.xfb_height,
        byte_len,
    })
}

/// Hide the display before shutdown, following HBC's video teardown sequence.
pub fn shutdown() {
    ogc_video::set_black(true);
    ogc_video::flush();
    for _ in 0..4 {
        ogc_video::wait_vsync();
    }
}

/// Show the configured framebuffer after the renderer has drawn its first frame.
pub fn show() {
    ogc_video::set_black(false);
    ogc_video::flush();
}
