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

/// Owns the VI framebuffer and the GX FIFO for the lifetime of the display.
pub struct VideoSystem {
    _fifo: ogc_gx::GxFifo,
    config: ogc_gx::FrameConfig,
    framebuffer: *mut core::ffi::c_void,
    info: FramebufferInfo,
}

impl VideoSystem {
    pub fn info(&self) -> FramebufferInfo {
        self.info
    }

    /// Show the cleared framebuffer after the renderer has drawn its first frame.
    pub fn show(&self) {
        ogc_video::set_black(false);
        ogc_video::flush();
    }

    /// Configure direct-color 2D drawing in pixel coordinates with a y-down axis.
    ///
    /// Call this after GX initialization and again after any code changes GX
    /// vertex, channel, TEV, or blend state.
    pub fn configure_2d(&self) {
        let width = f32::from(self.info.width);
        let height = f32::from(self.info.height);
        let projection: ogc_gx::Mtx44 = [
            [2.0 / width, 0.0, 0.0, -1.0],
            [0.0, -2.0 / height, 0.0, 1.0],
            [0.0, 0.0, -1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        unsafe { ogc_gx::configure_solid_2d(&projection) };
    }

    /// Draw one filled rectangle in framebuffer pixel coordinates.
    ///
    /// # Safety
    /// The solid 2D GX state must be configured and no other code may change
    /// its vertex state until this call returns.
    pub unsafe fn draw_solid_rect(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: ogc_gx::GxColor,
    ) -> Result<(), VideoError> {
        unsafe { ogc_gx::draw_solid_rect(x, y, width, height, 0.0, color) }
            .map_err(VideoError::GxRect)
    }

    /// Queue an EFB-to-XFB copy and schedule the framebuffer for VI scanout.
    ///
    /// # Safety
    /// The caller must finish drawing the frame and keep all referenced graphics
    /// resources alive until GX has completed the copy.
    pub unsafe fn present(&self) {
        unsafe { ogc_gx::present_frame(self.config) };
        unsafe { ogc_video::set_next_framebuffer(self.framebuffer) };
        ogc_video::flush();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VideoError {
    NoPreferredMode,
    FramebufferAllocationFailed,
    InvalidFrameConfig,
    GxFifo(ogc_gx::GxFifoError),
    GxRect(ogc_gx::GxRectError),
}

/// Initialize VI and select a cleared framebuffer.
///
/// Width and height describe the selected XFB mode. HBC's separate 640x480
/// or 852x480 logical UI dimensions require aspect-ratio configuration and
/// remain caller policy.
pub fn initialize() -> Result<VideoSystem, VideoError> {
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
    }

    // GX uses the low arena2 boundary for its FIFO. The HBC allocator reserves
    // its heap first, so this FIFO cannot overlap Rust allocations.
    let fifo = unsafe { ogc_gx::GxFifo::initialize_default_mem2() }.map_err(VideoError::GxFifo)?;
    let config =
        ogc_gx::FrameConfig::new(framebuffer, mode.fb_width, mode.efb_height, mode.xfb_height)
            .ok_or(VideoError::InvalidFrameConfig)?;
    unsafe {
        ogc_gx::initialize_display(config, ogc_gx::GxColor::default());
        ogc_gx::present_frame(config);
        ogc_video::set_next_framebuffer(framebuffer);
    }
    ogc_video::flush();
    for _ in 0..4 {
        ogc_video::wait_vsync();
    }

    Ok(VideoSystem {
        _fifo: fifo,
        config,
        framebuffer,
        info: FramebufferInfo {
            width: mode.fb_width,
            height: mode.xfb_height,
            byte_len,
        },
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
