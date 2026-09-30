//! Wii video initialization.
//!
//! This adapter sets the VI mode and framebuffer through `ogc-video`, then
//! owns the GX FIFO and the HBC 2D rendering state.

use crate::png::GxRgba8Texture;

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

    #[cfg(feature = "startup-diagnostics")]
    pub fn framebuffer_has_visible_luma(&self) -> bool {
        let framebuffer = ogc_core::mem_k0_to_k1(self.framebuffer as usize) as *const u8;
        (0..self.info.byte_len as usize)
            .step_by(2)
            .any(|offset| unsafe {
                super::diagnostics::has_visible_luma(core::ptr::read_volatile(framebuffer.add(offset)))
            })
    }

    #[cfg(feature = "startup-diagnostics")]
    pub fn framebuffer_has_bright_luma(&self) -> bool {
        let framebuffer = ogc_core::mem_k0_to_k1(self.framebuffer as usize) as *const u8;
        (0..self.info.byte_len as usize)
            .step_by(2)
            .any(|offset| unsafe {
                super::diagnostics::has_bright_luma(core::ptr::read_volatile(framebuffer.add(offset)))
            })
    }

    /// Show the cleared framebuffer after the renderer has drawn its first frame.
    pub fn show(&self) {
        unsafe { ogc_video::set_next_framebuffer(self.framebuffer) };
        ogc_video::set_black(false);
        ogc_video::flush();
    }

    /// Wait for one VI retrace before drawing the next frame.
    pub fn wait_vsync(&self) {
        ogc_video::wait_vsync();
    }

    /// Configure direct-color 2D drawing in pixel coordinates with a y-down axis.
    ///
    /// Call this after GX initialization and again after any code changes GX
    /// vertex, channel, TEV, or blend state.
    pub fn configure_2d(&self) {
        unsafe { ogc_gx::configure_solid_2d(&self.projection()) };
    }

    /// Configure textured RGBA8 drawing with the same framebuffer projection.
    pub fn configure_textured_2d(&self) {
        unsafe { ogc_gx::configure_textured_2d(&self.projection()) };
    }

    fn projection(&self) -> ogc_gx::Mtx44 {
        let width = f32::from(self.info.width);
        let height = f32::from(self.info.height);
        [
            [2.0 / width, 0.0, 0.0, -1.0],
            [0.0, -2.0 / height, 0.0, 1.0],
            [0.0, 0.0, -1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]
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

    /// Draw an aligned GX RGBA8 texture in framebuffer pixel coordinates.
    ///
    /// # Safety
    /// Textured 2D GX state must be configured. The texture must remain alive until GX has
    /// completed its queued reads.
    pub unsafe fn draw_texture(
        &self,
        texture: &GxRgba8Texture,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> Result<(), VideoError> {
        let mut object = ogc_gx::GxTexObj::default();
        unsafe {
            ogc_gx::init_tex_obj(
                &mut object,
                texture.as_ptr() as *mut core::ffi::c_void,
                ogc_gx::GxTexObjInit {
                    wd: texture.width,
                    ht: texture.height,
                    fmt: ogc_gx::GX_TF_RGBA8,
                    wrap_s: 0,
                    wrap_t: 0,
                    mipmap: 0,
                },
            );
            ogc_gx::init_tex_obj_filter_mode(&mut object, ogc_gx::GX_NEAR, ogc_gx::GX_NEAR);
            ogc_gx::draw_textured_rect(&object, x, y, width, height, 0.0)
        }
        .map_err(VideoError::GxTexture)
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
    GxTexture(ogc_gx::GxTextureError),
}

/// Initialize VI and select a cleared framebuffer.
///
/// Width and height describe the selected XFB mode. The caller chooses HBC's
/// 640x480 or 852x480 logical viewport and supplies its aspect ratio here.
pub fn initialize(widescreen: bool) -> Result<VideoSystem, VideoError> {
    ogc_video::init();
    #[cfg(not(feature = "startup-diagnostics"))]
    ogc_video::set_black(true);

    let preferred = unsafe { ogc_video::get_preferred_mode(core::ptr::null_mut()) };
    if preferred.is_null() {
        return Err(VideoError::NoPreferredMode);
    }
    let mut mode = unsafe { *preferred };
    mode.vi_width = 672;
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("VIDEO ASPECT CHECK");
    configure_vwii_aspect(widescreen);

    unsafe { ogc_video::configure(&mut mode) };
    ogc_video::flush();
    ogc_video::wait_vsync();

    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("GX FRAMEBUFFER ALLOCATE");
    let framebuffer = unsafe { ogc_system::allocate_framebuffer(&mut mode) };
    if framebuffer.is_null() {
        return Err(VideoError::FramebufferAllocationFailed);
    }

    let byte_len = unsafe { ogc_video::get_frame_buffer_size(&mut mode) };
    unsafe {
        ogc_cache::dc_invalidate_range(framebuffer.cast_const(), byte_len);
        // COLOR_BLACK from HBC is a zero-valued YUYV framebuffer color.
        // SYS_AllocateFramebuffer returns a cached K0 address. VI and GX use
        // the uncached alias, so clear through K1 to make the pixels visible.
        let framebuffer_k1 = ogc_core::mem_k0_to_k1(framebuffer as usize) as *mut _;
        ogc_video::clear_frame_buffer(&mut mode, framebuffer_k1, 0);
    }

    // GX uses the low arena2 boundary for its FIFO. The HBC allocator reserves
    // its heap first, so this FIFO cannot overlap Rust allocations.
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("GX FIFO START");
    let fifo = unsafe { ogc_gx::GxFifo::initialize_default_mem2() }.map_err(VideoError::GxFifo)?;
    let config =
        ogc_gx::FrameConfig::new(framebuffer, mode.fb_width, mode.efb_height, mode.xfb_height)
            .ok_or(VideoError::InvalidFrameConfig)?;
    unsafe {
        #[cfg(feature = "startup-diagnostics")]
        super::diagnostics::mark("GX DISPLAY COMMANDS");
        #[cfg(feature = "startup-diagnostics")]
        let initial_clear = ogc_gx::GxColor {
            r: 56,
            g: 56,
            b: 56,
            a: 255,
        };
        #[cfg(not(feature = "startup-diagnostics"))]
        let initial_clear = ogc_gx::GxColor::default();
        ogc_gx::initialize_display(config, initial_clear);
        ogc_gx::present_frame(config);
        #[cfg(not(feature = "startup-diagnostics"))]
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

fn configure_vwii_aspect(widescreen: bool) {
    const VWII_MENU_TITLE_ID: u64 = 0x0000_0001_0000_0200;
    let mut content_count = 0;
    let result =
        unsafe { ogc_es_sys::get_title_contents_count(VWII_MENU_TITLE_ID, &mut content_count) };
    if result < 0 || content_count == 0 {
        return;
    }

    unsafe {
        wii_mmio_write32(
            0x0d80_06a0,
            if widescreen { 0x3000_0004 } else { 0x1000_0002 },
        );
        let state = wii_mmio_read32(0x0d80_06a8);
        wii_mmio_write32(0x0d80_06a8, state | 2);
    }
}

/// Read a Wii MMIO register through the uncached hardware alias.
///
/// # Safety
/// `address` must identify a readable Wii register.
unsafe fn wii_mmio_read32(address: u32) -> u32 {
    let register = (0xc000_0000 | address) as *const u32;
    let value = unsafe { core::ptr::read_volatile(register) };
    unsafe { core::arch::asm!("sync", options(nostack, preserves_flags)) };
    value
}

/// Write a Wii MMIO register through the uncached hardware alias.
///
/// # Safety
/// `address` must identify a writable Wii register.
unsafe fn wii_mmio_write32(address: u32, value: u32) {
    let register = (0xc000_0000 | address) as *mut u32;
    unsafe { core::ptr::write_volatile(register, value) };
    unsafe { core::arch::asm!("eieio", options(nostack, preserves_flags)) };
}

/// Hide the display before shutdown, following HBC's video teardown sequence.
pub fn shutdown() {
    ogc_video::set_black(true);
    ogc_video::flush();
    for _ in 0..4 {
        ogc_video::wait_vsync();
    }
}
