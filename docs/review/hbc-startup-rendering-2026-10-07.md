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

## 2026-10-07 rebuilt-DOL framebuffer capture

I rebuilt `target/hbc-rs-startup.dol` from the current diagnostic source and
captured its framebuffer through Dolphin's GDB interface after 20 seconds. The
GX framebuffer contains the HBC Home menu and the white probe square. The
diagnostic holds on a failure screen when its EFB or XFB square check fails. By
reaching the normal menu, this run passed the XFB square check in Dolphin.
The EFB result was not required for this success path. The
user then provided a Dolphin screenshot of the same visible state, which
confirms the capture. This does not resolve the Wii photo's `GX EFB SQUARE
FAIL`.

Keep HBC-GX-01 partial. Compare the EFB probe samples and GX/XFB memory on both
systems before changing rendering behavior. Do not treat the Dolphin capture as
proof of Wii parity.

## C frame-state parity correction

Source comparison found that C HBC invalidates the vertex and texture caches
and establishes its 2D state at each frame start. Rust previously performed
cache invalidation only during GX configuration and did not refresh that state
for every rendered frame. Rust now begins every frame with the same cache
invalidation and textured baseline. The Rust `GX_LoadTexObj` path also no longer
invalidates the full texture cache for each object; C leaves that operation to
`GX_InvalidateTexAll` at frame start.

C HBC restores Z mode and color updates before retrace, then copies and clears
the EFB. Rust now follows that order. The libogc-rs copy path now preserves and
restores the active Z, color, and pixel-engine shadow words around a clear, as
C `GX_CopyDisp` does, instead of restoring fixed GX initialization values.
The Rust copy still waits for GX completion before scheduling VI scanout; this
extra wait is a deliberate synchronization safeguard and does not change the
copied frame contents.

The Wii EFB/XFB probe has not been rerun after these changes. Keep HBC-GX-01
partial until the updated DOL passes on Wii hardware.

## Updated-DOL Dolphin run

After the C frame-state parity changes and warning cleanup, I rebuilt the
startup diagnostic DOL and ran it in Dolphin for 25 seconds. It reached the
no-storage notice. The GX framebuffer capture at
`target/hbc-startup-final-gx.png` shows readable notice text and the white
probe square. This proves the XFB square check passed. The EFB result
was used only to select a failure label; this run did not prove that the EFB
readback check passed. The Dolphin log
contains no error or invalid-opcode lines. The CPU progress framebuffer still
shows `WIILOAD PROBE 3 START`; that label is separate from the GX scene capture
and did not prevent the visible application frame.

This verifies the updated renderer path in Dolphin. The Wii EFB/XFB result is
still pending; keep HBC-GX-01 partial until the same diagnostic passes on Wii.

## Default Dolphin settings reproduce the failure

The user ran the rebuilt startup DOL and reported `GX EFB DRAW BLACK` again.
On 2026-10-07, the same DOL reproduced that exact screen in an isolated
Dolphin profile with default graphics settings. The captured scanout is
`target/hbc-default-settings-failure.png`. Enabling immediate XFB RAM copies
and CPU EFB access in the capture profile let the same DOL reach the
no-storage notice (`target/hbc-readback-enabled.png`).

The startup diagnostic reads EFB pixels and XFB RAM. Dolphin can skip those
operations under its default graphics settings. The default-profile failure
does not independently prove a GX draw failure. C HBC does not stop startup
on these CPU readback tests. A normal release DOL, `target/hbc-rs.dol`, was
built without startup diagnostics; actual GX rendering and presentation still
use the same production implementation. No GX implementation was changed in
this follow-up. Both builds and conversion had no warnings.

The capture helper now explicitly enables `EFBAccessEnable` as well as
disabling `XFBToTextureEnable` and `DeferEFBCopies`. Two regression tests
check new and existing profiles, preserve unrelated options, and check
repeatability. Dolphin's primary configuration source defines these options:
https://github.com/dolphin-emu/dolphin/blob/master/Source/Core/Core/Config/GraphicsSettings.cpp

Real Wii EFB/XFB results remain unverified. Earlier statements that a visible
notice proved both readback checks passed were incorrect and are corrected
above. Do not close HBC-GX-01 based on the emulator result.

The normal DOL was also run for 25 seconds with an isolated capture profile.
Its production VI scanout pointer (`ogc_video_sys::LAST_NEXT_FRAMEBUFFER`)
was read through Dolphin GDB. The 640x480 XFB capture at
`target/hbc-normal-gx.png` shows the no-storage notice, readable text, and
background. It has no startup test square. The capture profile enables RAM
readback; this result is not a separate default-settings visual test.
The computer-use runtime could not load, so direct UI inspection was unavailable.

## Wii normal build: black screen reported

The user reports a black screen on a real Wii after launching the normal
DOL copied to CARROT. This is a failed hardware result. The console model,
launch method, video-signal state, and last completed startup stage are not
yet known. A Dolphin pass does not resolve this failure.

A separate `apps/hbc-rs-diagnostic` package was prepared from the existing
startup-diagnostics production build. It displays CPU-written stage labels
and checks EFB drawing and XFB copying. No rendering fix is claimed from
this preparation. CARROT was not mounted when the issue was investigated;
the diagnostic package is prepared locally pending access to the drive.

CARROT became available during preparation. The diagnostic DOL and metadata
were copied to `/Volumes/CARROT/apps/hbc-rs-diagnostic`, flushed, and
verified against their source hashes. The normal app DOL was also verified
to match `target/hbc-rs.dol`. The real-Wii diagnostic result is pending.

The user confirms launch through the Homebrew Channel; the screen stays black.
On request, the existing C `channelapp-channel.dol` build was copied to
CARROT at `apps/hbc-c/boot.dol`, verified byte-for-byte, and named
HBC C Reference. This is the C build identified in the earlier working
Dolphin reference review. Its metadata retains the no-IOS-reload and AHB
access tags used by the existing C app metadata on CARROT.
The Rust packages currently lack those tags; launch policy differs and is
an open comparison item, not an established cause of the black screen.
The existing `apps/hbc` entry was left intact. Hardware comparison of
C HBC and Rust diagnostic results remains pending.

## Real Wii EFB square failure and C depth parity correction

The user supplied a Wii photo showing `GX EFB SQUARE FAIL` and confirms
that HBC C Reference works flawlessly on the same Wii through the Homebrew
Channel. The diagnostic reaches frame presentation, but the EFB square read
and XFB square check both fail. The displayed red/white square is the
CPU-written scanout control, not proof that GX drew the test square.

A source comparison found a concrete projection/depth difference:
C `gfx_init` uses `guOrtho` with near=100 and far=1000, and `config.h`
defines `VIEW_Z_ORIGIN=-420`. C applies that depth to its scene geometry.
Rust used projection depth [-1, 0] and submitted solid and textured vertices
at Z=0, which places the scene on the far clipping boundary.

The new shared `platform/projection.rs` preserves the C depth coefficients
and scene Z for both production draw paths. The existing pixel-coordinate
X/Y mapping is unchanged. The normalized scene depth is -580/900, inside
the clip volume. This fixes a verified source parity gap. It is a candidate
for the hardware fault; the Wii root cause is not yet proven.

Host tests passed: 230, including two regressions for the production
projection helper, independent expected C depth, and both aspect ratios.
Normal and diagnostic Wii builds completed without warnings. The source
structure audit passed with three existing keyboard-queue debt notes.
The full libogc-rs completeness audit still fails with the same 2238 open
API findings. No audit conditions or evidence classifications were weakened.
The updated normal and diagnostic packages were verified against their
source DOL bytes. A new real-Wii run is required before reporting a fix.

After CARROT was reconnected, both updated Rust app DOLs and metadata
were copied, flushed, and verified against source hashes. Previous card
files were saved under `target/wii-depth-previous`. The C reference was
left in place. The new Wii result remains pending.

The new diagnostic DOL was run in Dolphin with the readback-enabled
capture profile for 25 seconds. `target/hbc-depth-diagnostic-gx.png` shows
the no-storage notice and white GX test square. This is an XFB rendering
regression check in the emulator, not proof of a real-Wii fix or a separate
EFB readback success.

## Wii rendering confirmed by the user

The user confirms the depth-corrected build now works on Wii. New reports
concern disc cleanup, USB detection, text layout, and directional input;
see `wii-runtime-fixes-2026-10-07.md`. This is confirmation of visible
rendering, not a separately reported EFB readback or full hardware probe pass.

## 2026-10-08 startup diagnostic frame transition

Run `1791492295-180497000` uploaded successfully and received 486 progress packets through `GX XFB HAS IMAGE`, but no final report. Input startup labels were still being painted and synchronously sent on every runtime frame. The latest photo showing `PAD SCAN START` therefore does not establish a blocked first PAD scan; that scan completed in the captured trace.

Startup completion now publishes `FIRST HBC FRAME DISPLAYED` without replacing the GX framebuffer. Later startup marks no longer paint or send labels. A production-helper regression test verifies suppression over 120 subsequent input labels. The diagnostic frame loop polls for an actual VI retrace with a one-second bound and reports `VI FRAME RETRACE TIMEOUT` on failure. It does not report a passing frame when that wait fails. The normal queue wait also includes the libogc-rs critical-section correction recorded in `libvideo.md`. Wii confirmation remains pending.

## 2026-10-08 second-frame hardware trace

Run `1791493961-872597000` sent build `d02e255183f858c4712b285f6b0bd087182c1fa5ea7b2611ff9bc35d0095c67e` through Terminal and received 494 progress packets. After `FIRST HBC FRAME DISPLAYED`, the next loop completed storage/result and device polling, Wiiload status polling, input scan/update, and GX scene drawing. The final received marker was `RUNTIME GX DRAW DONE`; framebuffer presentation did not report completion and no final report arrived within 45 seconds. This narrows the observed failure interval but does not prove the exact blocked call.

Runtime progress markers are emitted once per phase after startup completion and do not repaint the diagnostic framebuffer. The next diagnostic build bounds the VI wait inside `VideoSystem::present` on every frame, rather than only its first invocation. It also records GX completion wait, VI wait, and XFB copy boundaries. Normal builds retain the queue wait so this diagnostic change does not establish a production scheduler fix. Verification: 309 HBC host tests and the diagnostic PowerPC release build passed. The latest presentation markers and bounded subsequent-frame VI path still require Wii validation.

## Early snapshot before the HBC screen

The diagnostic DOL now sends a WIR2 startup checkpoint after heap setup and report-network setup, before CONF/GX initialization, playtime/settings/theme operations, controller setup, storage workers and the first HBC frame. It sends the CPU diagnostic framebuffer, actual logs, heap use and build/run identity. Frame samples remain zero; no runtime passing summary is added. Host status is `checkpoint_received`, with `probe_passed: false`. HBC continues normally after transmission and does not request restart for this snapshot. Subsequent runtime failures retain their own error screen instead of presenting the completed checkpoint as a report failure. The normal build does not use this checkpoint path. Hardware transmission remains pending until the Wii is listening again.

## Live stage capture extension

Diagnostic HBC now keeps its report socket open after the early snapshot. It sends CPU framebuffer snapshots at major completed startup milestones and GX framebuffer snapshots after the first displayed HBC frame and at each one-time runtime/presentation boundary. Fine-grained PNG parsing progress remains text-only to avoid transferring hundreds of full frames during theme decode. Each actual completed snapshot carries WIR2 flags `0x0006`; final reports retain flags zero. A failed capture is recorded and prevents a later passing runtime report. The host persists each frame and live progress before waiting for the next frame. Its gallery indexes captured images and refreshes automatically. Timeout regression coverage verifies that two distinct saved images and logs survive a later stall. Real Wii streaming remains pending.
