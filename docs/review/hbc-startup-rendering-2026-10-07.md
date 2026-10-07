# HBC startup rendering and storage scan fix

Date: 2026-10-07

## Report

A Dolphin capture showed an information dialog with unreadable text and a storage throbber that remained visible. The scan worker was expected to finish and report whether media was mounted.

## Findings and fixes

- `src/png.rs::to_gx_rgba8` rejected images whose width or height was not a multiple of four. `src/platform/renderer.rs::draw_frame` converted texture creation failures to `None`, then omitted those glyphs. TrueType glyphs commonly have partial 4x4 GX tiles. The converter now rounds storage to complete GX tiles, copies in-range pixels, and writes transparent pixels into each padded area. Allocation and texture errors propagate through `VideoError`.
- GX rectangle vertices use a y-down coordinate system and start at the lower-left corner. `ogc-gx::draw_textured_rect` assigned top-row texture coordinates to those lower vertices, which flipped textures vertically. The textured rectangle now maps the top texture row to the upper vertices. A regression test checks all four vertex and texture-coordinate pairs against independent expected values.
- The storage scan worker called `ogc_fat::mount`, which created its scheduler mutex through `ogc_lwp_driver::init_mutex`. That function used the thread-handle encoder (complemented thread-record pointer) for a mutex handle. Mutex operations decode small one-based object handles. This mismatch sent mutex operations to an invalid object and left the worker in a panic loop. Thread handles and synchronization-object handles now use separate encoders. A regression test checks every synchronization-object pool, its bounds, and its null handle.
- Glyphs are created after theme setup and also need a data-cache flush before GX reads them. `GxRgba8Texture::from_image` now publishes its own aligned immutable buffer to GX. Theme, icon, and glyph paths share this constructor, so callers no longer maintain separate flush logic.

## Verification

- `cargo test --locked` in `hbc-rs`: 209 passed, 0 failed.
- `cargo run -p xtask -- audit-source` and `cargo run -p xtask -- audit-stubs` in `libogc-rs`: both source-structure audits passed. They reported only the repository's existing tracked audit debt.
- `cargo xtask check-abi-rust-target` in `libogc-rs`: passed after the unsafe-operation fixes. The check keeps `unsafe_op_in_unsafe_fn` denied.
- `cargo run -p xtask -- build-probes`: built the probe DOLs. A prior `cargo xtask build-probes` invocation failed after its build because this environment has no installed `cargo-xtask` subcommand; running the workspace package directly succeeded.
- The strict Wii-target HBC release build completed, and `xtask elf-to-dol` rebuilt `target/hbc-rs.dol`.
- Dolphin ran that DOL with an isolated user profile. At 12 seconds, the worker stack showed it had progressed through SD, USB, and both GameCube memory-card checks. The worker state was idle, its stage was complete, and its result displayed “No SD or USB storage device is available.” A direct XFB read produced a 640x480 image with readable notice text, correctly oriented assets, and logo. No emulated storage media was configured.
- Real Wii verification remains pending. Dolphin verifies the emulator rendering and startup path, not Wii hardware behavior.

## GX first-frame failure follow-up

The user reports that the CPU-written gray frame, white square, and red outline
also appear on a real Wii. This confirms that VI scans out the selected GX XFB
on hardware. It does not confirm that GX drew into the EFB or copied that EFB
to the XFB.

The startup diagnostic now waits for GX after it draws the known square and
reads a 3-by-3 sample from the square with `GX_PeekARGB`. It then checks the
copied XFB. If a later check fails, the CPU control pattern includes the
failure stage. `GX EFB SQUARE FAIL` points to drawing; `GX XFB SQUARE FAIL`
points to the EFB-to-XFB copy. This narrows the hardware failure without
replacing the production renderer.

Verification for this diagnostic change:

- `cargo test --locked --features startup-diagnostics`: 228 passed, 0 failed.
- Strict PowerPC `startup-diagnostics` release build and DOL conversion passed.
- The real Wii result for the new EFB/XFB labels is pending. The current
  evidence confirms the CPU scanout pattern only.

## C/Rust vertex-stream correction

The next Dolphin capture showed `GX EFB DRAW BLACK`. A source comparison found
that HBC C enables direct POS, CLR0, and TEX0 attributes for every quad, and
`plot_vert_c` writes a zero TEX0 pair for each solid vertex. The Rust solid
vertex path omitted TEX0 and disabled its texture generator. The Rust GX helper
now keeps the same TEX0 descriptor, VAT format, identity texgen, and per-vertex
coordinates for solid and textured quads. The solid TEV operation still uses
the raster color and does not sample a texture.

The strict PowerPC diagnostic DOL was rebuilt with this change. A normal-profile
Dolphin run of that DOL on 2026-10-07 reaches `GX EFB DRAW BLACK` and displays
the CPU-written fallback square with its red border. This reproduces the reported
failure; it does not confirm the GX fix.

An earlier automated capture appeared to show the HBC no-storage scene and white
test square. That capture used a separate Dolphin profile with
`XFBToTextureEnable=False` and `DeferEFBCopies=False`, and the helper stopped
after eight seconds without checking the startup result. It is not valid pass
evidence. The profile difference may affect XFB readback, but it does not yet
explain the EFB test failure. Compare the EFB sample values and the XFB buffer
under both profiles, then rerun the rebuilt DOL on Wii. Keep HBC-GX-01 partial
until the black-screen cause is resolved and the Wii result is verified.

## TEV order correction

The solid-color TEV helper returned three BP words. Its third word targeted
register `0x28` and cleared the TEV order that `configure_2d_state` had just
set to select raster color. C `GX_SetTevOp(GX_PASSCLR)` writes the color and
alpha environment registers; it does not change TEV order. Rust now emits only
those two environment words for solid and textured setup. Regression checks
keep the expected order word and both environment values.

The strict diagnostic DOL was rebuilt and run in the normal Dolphin profile
again. It still reports `GX EFB DRAW BLACK`; this correction has not resolved
the observed rendering failure. The Wii photo appears to report
`GX EFB SQUARE FAIL`, which also leaves the EFB draw path open. HBC-GX-01 stays
partial, and the corrected DOL still needs another Wii run.
