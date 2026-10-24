//! Wii video initialization.
//!
//! This adapter sets the VI mode and framebuffer through `ogc-video`, then
//! owns the GX FIFO and the HBC 2D rendering state.

use super::draw_state::{DrawFormat, DrawState};
use crate::png::GxRgba8Texture;

static TRACE_FIRST_PRESENT: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(true);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FramebufferInfo {
    pub width: u16,
    pub height: u16,
    pub byte_len: u32,
}

/// Owns the VI framebuffer and the GX FIFO for the lifetime of the display.
pub struct VideoSystem {
    _fifo: ogc_gx::GxFifo,
    draw_state: DrawState,
    config: ogc_gx::FrameConfig,
    framebuffer: *mut core::ffi::c_void,
    info: FramebufferInfo,
    efb_height: u16,
}

impl VideoSystem {
    pub fn info(&self) -> FramebufferInfo {
        self.info
    }

    /// Read the completed EFB into a top-to-bottom RGBA8 image.
    pub fn capture_efb_rgba(&self) -> Result<crate::png::RgbaImage, VideoError> {
        let width = self.info.width;
        let height = self.efb_height;
        let byte_len = usize::from(width)
            .checked_mul(usize::from(height))
            .and_then(|pixels| pixels.checked_mul(4))
            .filter(|_| width != 0 && height != 0 && width <= 1024 && height <= 1024)
            .ok_or(VideoError::InvalidCaptureDimensions)?;
        let mut pixels = alloc::vec::Vec::new();
        pixels
            .try_reserve_exact(byte_len)
            .map_err(|_| VideoError::CaptureAllocation)?;
        ogc_gx::draw_done();
        for y in 0..height {
            for x in 0..width {
                let mut color = ogc_gx::GxColor::default();
                unsafe { ogc_gx::peek_argb(x, y, &mut color) };
                pixels.extend_from_slice(&[color.r, color.g, color.b, color.a]);
            }
        }
        Ok(crate::png::RgbaImage {
            width: u32::from(width),
            height: u32::from(height),
            pixels,
        })
    }

    #[cfg(feature = "startup-diagnostics")]
    pub fn framebuffer_has_visible_luma(&self) -> bool {
        let framebuffer = ogc_core::mem_k0_to_k1(self.framebuffer as usize) as *const u8;
        (0..self.info.byte_len as usize)
            .step_by(2)
            .any(|offset| unsafe {
                super::diagnostics::has_visible_luma(core::ptr::read_volatile(
                    framebuffer.add(offset),
                ))
            })
    }

    #[cfg(feature = "startup-diagnostics")]
    pub fn framebuffer_has_test_square(&self) -> bool {
        let framebuffer = ogc_core::mem_k0_to_k1(self.framebuffer as usize) as *const u8;
        let stride = (usize::from(self.info.width) + 15) & !15;
        super::diagnostics::test_square_is_white(self.info.width, self.info.height, |x, y| unsafe {
            core::ptr::read_volatile(
                framebuffer.add((usize::from(y) * stride + usize::from(x)) * 2),
            )
        })
    }

    #[cfg(feature = "startup-diagnostics")]
    pub fn efb_has_test_square(&self) -> Result<bool, VideoError> {
        if !unsafe { ogc_gx::finish_frame_and_wait() } {
            return Err(VideoError::GxDrawTimeout);
        }
        let found = super::diagnostics::test_square_is_white_rgb(
            self.info.width,
            self.efb_height,
            |x, y| {
                let mut color = ogc_gx::GxColor::default();
                unsafe { ogc_gx::peek_argb(x, y, &mut color) };
                (color.r, color.g, color.b)
            },
        );
        if !found {
            let mut color = ogc_gx::GxColor::default();
            unsafe { ogc_gx::peek_argb(self.info.width / 2, self.efb_height / 2, &mut color) };
            super::diagnostics::mark(&alloc::format!(
                "EFB RGB {:02X}{:02X}{:02X}",
                color.r,
                color.g,
                color.b,
            ));
            super::diagnostics::hold();
        }
        Ok(found)
    }

    #[cfg(feature = "startup-diagnostics")]
    pub fn paint_scanout_probe(&self, label: &str) {
        let framebuffer = ogc_core::mem_k0_to_k1(self.framebuffer as usize) as *mut u32;
        let words = unsafe {
            core::slice::from_raw_parts_mut(framebuffer, self.info.byte_len as usize / 4)
        };
        super::diagnostics::paint_scanout_probe(
            words,
            usize::from(self.info.width),
            usize::from(self.info.height),
            label,
        );
        unsafe { core::arch::asm!("sync", options(nostack, preserves_flags)) };
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
        self.draw_state.configure(DrawFormat::Solid, || {
            self.configure_format(DrawFormat::Solid)
        });
    }

    /// Configure textured RGBA8 drawing with the same framebuffer projection.
    pub fn configure_textured_2d(&self) {
        self.draw_state.configure(DrawFormat::Textured, || {
            self.configure_format(DrawFormat::Textured)
        });
    }

    /// Start an HBC frame with the same cache invalidation and default 2D state as C HBC.
    pub fn begin_frame(&self) {
        ogc_gx::inv_vtx_cache();
        unsafe { ogc_gx::invalidate_tex_all() };
        self.draw_state.configure(DrawFormat::Textured, || {
            self.configure_format(DrawFormat::Textured)
        });
    }

    fn configure_format(&self, format: DrawFormat) {
        let projection = self.projection();
        unsafe {
            match format {
                DrawFormat::Solid => ogc_gx::configure_solid_2d(&projection),
                DrawFormat::Textured => ogc_gx::configure_textured_2d(&projection),
            }
        }
    }

    fn projection(&self) -> ogc_gx::Mtx44 {
        super::projection::projection(self.info.width, self.info.height)
    }

    /// Draw one filled rectangle in framebuffer pixel coordinates.
    ///
    /// # Safety
    /// No other code may change GX state during this call. After an external GX
    /// state change, call `configure_2d` or `configure_textured_2d` to refresh it.
    pub unsafe fn draw_solid_rect(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: ogc_gx::GxColor,
    ) -> Result<(), VideoError> {
        self.draw_state.submit(
            DrawFormat::Solid,
            || self.configure_format(DrawFormat::Solid),
            || {
                unsafe {
                    ogc_gx::draw_solid_rect(x, y, width, height, super::projection::SCENE_Z, color)
                }
                .map_err(VideoError::GxRect)
            },
        )
    }

    /// Draw an aligned GX RGBA8 texture in framebuffer pixel coordinates.
    ///
    /// # Safety
    /// The texture must remain alive until GX has completed its queued reads.
    /// No other code may change GX state during this call. After an external GX
    /// state change, call `configure_2d` or `configure_textured_2d` to refresh it.
    pub unsafe fn draw_texture(
        &self,
        texture: &GxRgba8Texture,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> Result<(), VideoError> {
        self.draw_state.submit(
            DrawFormat::Textured,
            || self.configure_format(DrawFormat::Textured),
            || {
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
                    ogc_gx::draw_textured_rect(
                        &mut object,
                        x,
                        y,
                        width,
                        height,
                        super::projection::SCENE_Z,
                    )
                }
                .map_err(VideoError::GxTexture)
            },
        )
    }

    /// Copy EFB to XFB, wait for GX to finish, and schedule the framebuffer for VI scanout.
    ///
    /// # Safety
    /// The caller must finish drawing the frame and keep all referenced graphics
    /// resources alive until this function returns.
    pub unsafe fn present(&self) -> Result<(), VideoError> {
        let trace = TRACE_FIRST_PRESENT.swap(false, core::sync::atomic::Ordering::Relaxed);
        if trace {
            ogc_system::report(format_args!("HBC: first GX draw wait starting\n"));
        }

        // HBC finishes rendering, waits for the next retrace, and only then copies
        // the EFB to the XFB. This prevents GX from writing the framebuffer while
        // VI scans the same buffer.
        #[cfg(feature = "startup-diagnostics")]
        if trace {
            super::diagnostics::mark("GX DRAW WAIT START");
        }
        #[cfg(feature = "startup-diagnostics")]
        super::diagnostics::runtime_stage(2048, "PRESENT GX WAIT START");
        if !unsafe { ogc_gx::finish_frame_and_wait() } {
            if trace {
                ogc_system::report(format_args!("HBC: first GX draw wait timed out\n"));
            }
            #[cfg(feature = "startup-diagnostics")]
            if trace {
                super::diagnostics::mark("GX DRAW WAIT FAILED");
            }
            return Err(VideoError::GxDrawTimeout);
        }
        #[cfg(feature = "startup-diagnostics")]
        super::diagnostics::runtime_stage(4096, "PRESENT GX WAIT DONE");
        unsafe {
            ogc_gx::set_z_mode(ogc_gx::GX_ENABLE, ogc_gx::GX_LEQUAL, ogc_gx::GX_TRUE);
            ogc_gx::set_color_update(ogc_gx::GX_TRUE);
        }
        if trace {
            ogc_system::report(format_args!("HBC: first GX draw wait complete\n"));
            ogc_system::report(format_args!("HBC: first VI retrace wait starting\n"));
        }
        #[cfg(feature = "startup-diagnostics")]
        if trace {
            super::diagnostics::mark("GX DRAW WAIT DONE");
            super::diagnostics::mark("VI RETRACE WAIT START");
        }
        #[cfg(feature = "startup-diagnostics")]
        {
            if trace {
                super::diagnostics::mark("VI RETRACE POLL START");
            }
            super::diagnostics::runtime_stage(8192, "PRESENT VI WAIT START");
            if !ogc_video::wait_vsync_timeout_us(1_000_000) {
                return Err(VideoError::ViRetraceTimeout);
            }
            if trace {
                super::diagnostics::mark("VI POLL RETRACE SEEN");
            }
            super::diagnostics::runtime_stage(16384, "PRESENT VI WAIT DONE");
        }
        #[cfg(not(feature = "startup-diagnostics"))]
        self.wait_vsync();
        if trace {
            ogc_system::report(format_args!("HBC: first VI retrace wait complete\n"));
            ogc_system::report(format_args!("HBC: first XFB copy starting\n"));
        }
        #[cfg(feature = "startup-diagnostics")]
        if trace {
            super::diagnostics::mark("VI RETRACE WAIT DONE");
            super::diagnostics::mark("GX COPY WAIT START");
        }
        #[cfg(feature = "startup-diagnostics")]
        super::diagnostics::runtime_stage(32768, "PRESENT XFB COPY START");
        unsafe { ogc_gx::copy_display(self.config, true) };
        if trace {
            ogc_system::report(format_args!("HBC: first XFB copy queued\n"));
            ogc_system::report(format_args!("HBC: first XFB copy wait starting\n"));
        }
        if !unsafe { ogc_gx::finish_frame_and_wait() } {
            if trace {
                ogc_system::report(format_args!("HBC: first XFB copy wait timed out\n"));
            }
            #[cfg(feature = "startup-diagnostics")]
            if trace {
                super::diagnostics::mark("GX COPY WAIT FAILED");
            }
            return Err(VideoError::GxDrawTimeout);
        }
        if trace {
            ogc_system::report(format_args!("HBC: first XFB copy wait complete\n"));
        }
        #[cfg(feature = "startup-diagnostics")]
        super::diagnostics::runtime_stage(65536, "PRESENT XFB COPY DONE");
        #[cfg(feature = "startup-diagnostics")]
        if trace {
            super::diagnostics::mark("GX COPY WAIT DONE");
            super::diagnostics::mark("VI FRAMEBUFFER SET START");
        }
        unsafe { ogc_video::set_next_framebuffer(self.framebuffer) };
        #[cfg(feature = "startup-diagnostics")]
        if trace {
            super::diagnostics::mark("VI FRAMEBUFFER SET DONE");
            super::diagnostics::mark("VI FLUSH START");
        }
        ogc_video::flush();
        if trace {
            ogc_system::report(format_args!("HBC: first framebuffer queued for scanout\n"));
        }
        #[cfg(feature = "startup-diagnostics")]
        if trace {
            super::diagnostics::mark("VI FLUSH DONE");
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VideoError {
    NoPreferredMode,
    FramebufferAllocationFailed,
    InvalidFrameConfig,
    GxFifo(ogc_gx::GxFifoError),
    GxDrawTimeout,
    ViRetraceTimeout,
    InvalidCaptureDimensions,
    CaptureAllocation,
    GxRect(ogc_gx::GxRectError),
    GxTexture(ogc_gx::GxTextureError),
    Font(crate::font::FontError),
    Texture(crate::png::PngError),
    RenderAllocation,
}

/// Initialize VI and select a cleared framebuffer.
///
/// Width and height describe the selected XFB mode. The caller chooses HBC's
/// 640x480 or 852x480 logical viewport and supplies its aspect ratio here.
pub fn initialize(widescreen: bool) -> Result<VideoSystem, VideoError> {
    ogc_video::init();
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark_fp_state("FP AFTER VIDEO INIT ON", "FP AFTER VIDEO INIT OFF");
    #[cfg(not(feature = "startup-diagnostics"))]
    ogc_video::set_black(true);

    let preferred = unsafe { ogc_video::get_preferred_mode(core::ptr::null_mut()) };
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark_fp_state("FP AFTER MODE GET ON", "FP AFTER MODE GET OFF");
    if preferred.is_null() {
        return Err(VideoError::NoPreferredMode);
    }
    let mut mode = unsafe { *preferred };
    mode.vi_width = 672;
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("VIDEO ASPECT CHECK");
    configure_vwii_aspect(widescreen);

    unsafe { ogc_video::configure(&mut mode) };
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark_fp_state("FP AFTER VI CONFIG ON", "FP AFTER VI CONFIG OFF");
    ogc_video::flush();
    ogc_video::wait_vsync();
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark_fp_state("FP AFTER RETRACE ON", "FP AFTER RETRACE OFF");

    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("GX FRAMEBUFFER ALLOCATE");
    let framebuffer = unsafe { ogc_system::allocate_framebuffer(&mut mode) };
    if framebuffer.is_null() {
        return Err(VideoError::FramebufferAllocationFailed);
    }
    let byte_len = unsafe { ogc_video::get_frame_buffer_size(&mut mode) };
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::set_gx_framebuffer_info(
        framebuffer as usize,
        mode.fb_width,
        mode.xfb_height,
        byte_len,
    );
    unsafe {
        ogc_cache::dc_invalidate_range(framebuffer.cast_const(), byte_len);
        // COLOR_BLACK from HBC is a zero-valued YUYV framebuffer color.
        // SYS_AllocateFramebuffer returns a cached K0 address. VI and GX use
        // the uncached alias, so clear through K1 to make the pixels visible.
        let framebuffer_k1 = ogc_core::mem_k0_to_k1(framebuffer as usize) as *mut _;
        ogc_video::clear_frame_buffer(&mut mode, framebuffer_k1, 0);
    }
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark_fp_state("FP AFTER XFB CLEAR ON", "FP AFTER XFB CLEAR OFF");

    // GX uses the low arena2 boundary for its FIFO. The HBC allocator reserves
    // its heap first, so this FIFO cannot overlap Rust allocations.
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("GX FIFO START");
    // Reborn aborts the previous GX frame before GX_Init. HBC launches this
    // DOL with the Wii's GX command engine already in use.
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("GX ABORT START");
    ogc_gx::abort_frame();
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("GX ABORT COMPLETE");
    let fifo = unsafe { ogc_gx::GxFifo::initialize_default_mem2() }.map_err(VideoError::GxFifo)?;
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark_fp_state("FP AFTER GX FIFO ON", "FP AFTER GX FIFO OFF");
    let config =
        ogc_gx::FrameConfig::new(framebuffer, mode.fb_width, mode.efb_height, mode.xfb_height)
            .ok_or(VideoError::InvalidFrameConfig)?
            .with_copy_filter(ogc_gx::GxCopyFilter {
                antialias: mode.aa != 0,
                sample_pattern: mode.sample_pattern,
                vertical_enabled: true,
                vertical_coefficients: mode.vfilter,
            })
            .with_field_mode(
                mode.field_rendering != 0,
                u32::from(mode.vi_height) == u32::from(mode.xfb_height) * 2,
            );
    #[cfg(feature = "startup-diagnostics")]
    let initial_clear = ogc_gx::GxColor {
        r: 192,
        g: 192,
        b: 192,
        a: 255,
    };
    #[cfg(not(feature = "startup-diagnostics"))]
    let initial_clear = ogc_gx::GxColor::default();

    #[cfg(feature = "startup-diagnostics")]
    let display_initialized = unsafe {
        super::diagnostics::mark("GX FRAME CONFIG START");
        super::diagnostics::mark_fp_state("GX FP ENABLED", "GX FP DISABLED");
        ogc_gx::configure_frame_with_progress(config, |stage| {
            super::diagnostics::mark(stage);
        });

        ogc_gx::set_copy_clear(initial_clear, 0x00ff_ffff);
        super::diagnostics::mark("GX CLEAR STATE DONE");

        ogc_gx::copy_display(config, true);
        super::diagnostics::mark("GX FIRST CLEAR QUEUED");
        let first_clear_done = ogc_gx::finish_frame_and_wait();
        super::diagnostics::mark(if first_clear_done {
            "GX FIRST CLEAR DONE"
        } else {
            "GX FIRST CLEAR TIMEOUT"
        });

        if first_clear_done {
            ogc_gx::copy_display(config, true);
            super::diagnostics::mark("GX SECOND CLEAR QUEUED");
            let second_clear_done = ogc_gx::finish_frame_and_wait();
            super::diagnostics::mark(if second_clear_done {
                "GX SECOND CLEAR DONE"
            } else {
                "GX SECOND CLEAR TIMEOUT"
            });
            second_clear_done
        } else {
            false
        }
    };

    #[cfg(not(feature = "startup-diagnostics"))]
    let display_initialized = unsafe { ogc_gx::initialize_display(config, initial_clear) };

    if !display_initialized {
        return Err(VideoError::GxDrawTimeout);
    }

    #[cfg(not(feature = "startup-diagnostics"))]
    unsafe {
        ogc_video::set_next_framebuffer(framebuffer);
    }
    // The initial EFB/XFB clear is not the displayed scene. Continue to the
    // first rendered-frame probe, which checks GX output after drawing a known
    // white square. A low-luma clear alone does not prove that scene rendering
    // failed.
    ogc_video::flush();
    for _ in 0..4 {
        ogc_video::wait_vsync();
    }

    Ok(VideoSystem {
        _fifo: fifo,
        draw_state: DrawState::default(),
        config,
        framebuffer,
        info: FramebufferInfo {
            width: mode.fb_width,
            height: mode.xfb_height,
            byte_len,
        },
        efb_height: mode.efb_height,
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
