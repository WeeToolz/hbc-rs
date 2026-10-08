# Wii runtime fixes — 2026-10-07

## Hardware report

The user confirms that Rust HBC now renders on the Wii after the C depth
correction. The working C reference remains the comparison. Four runtime
faults were reported: disc-service errors when selecting actions, failure to
detect the USB medium used to launch the app, credits/text overflow, and
excessively fast D-pad navigation.

## Disc cleanup

C `source/dvd.c::WiiDVD_ShutDown` closes an existing startup request. It does
not open `/dev/di` or submit another motor-stop command during handoff.
Rust previously opened DI and submitted a synchronous stop on each exit or
launch. The main cleanup function now closes only the existing DI transport,
checks the result, preserves the IOS error in the log, and returns failure
when close fails. The shared `ogc-di/src/shutdown.rs` close engine preserves
handle ownership after a failed close and clears it only after successful
close. Two standalone host regressions test idle cleanup and failure/retry.
They exercise the same engine used by production `ogc_di::close`.

C's asynchronous startup motor-stop callback chain remains absent in Rust;
that omission is tracked separately as HBC-DISC-01 partial. This change fixes
the extra exit operation. It does not claim that the startup callback chain
has been implemented. Real IOS close and handoff verification is pending.

## USB initialization and application metadata

The USB storage interface uses C's `USB_Initialize` startup, which starts both
vendor and HID hosts. It still queries only mass-storage interfaces, so HID
devices do not enter storage setup. A shared startup engine checks the USB host
and storage initialization results. Its standalone host regression checks
success and both failure boundaries. Device enumeration, SCSI commands, FAT
mount, and file reads remain on the real existing backend.

The user later reported `DeviceUnavailable` with `stage: Enumerate`, result
`-22`, and zero candidates on Wii. The USB storage layer had replaced the
underlying V5 watcher failure with the generic `Enumerate` stage. It now keeps
the failing IOS operation (`DeviceChange` or `AttachFinish`) through the storage
diagnostic. A reported `Enumerate` stage still identifies an error from the
device-list call itself. Storage startup now matches C by initializing both
USB hosts; its storage query still filters to mass storage. The stage-mapping
regression compiles for PowerPC but was not run on the host. Nine existing USB
event tests passed. The normal DOL was rebuilt and copied to
`CARROT:/apps/hbc-rs/boot.dol`; its SHA-256 matched after readback. Dolphin
reached the notice, browser, menu, and Options screens with zero unknown GX
opcodes. Dolphin does not verify the Wii IOS USB watcher. The next Wii run is
needed to identify the failing operation and verify mass-storage detection.

The working C app on CARROT uses `<no_ios_reload/>` and `<ahb_access/>` in its
metadata. Rust packaging omitted both. The new `tools/package-wii.py` generates
normal and diagnostic packages with those same launch tags, verifies ZIP
integrity, checks exact DOL bytes, and checks the launch tags. This establishes
a matching package launch policy; the USB failure's hardware cause is not yet
proven. USB enumeration, mount, app catalog, and hot-plug checks remain pending
on the user's Wii. No live NAND write was performed during this work.

## Measured text layout

C `source/font.c::font_wrap_string` measures glyph width and kerning against a
pixel bound. Rust used character counts. Credits also used a fixed line count
with font-scaled row spacing, which could extend beyond the dialog.

Pending memo runs now retain the full text and the actual pixel box. The
production font face supplies glyph advance, ink width, kerning, and line
height. A shared measured wrapping engine preserves paragraph breaks, splits
long words, rejects invalid metrics and bounds, and selects only complete
visible rows. The renderer uses the same requested/fallback font for layout
and drawing. About, app descriptions, notices, and Wiiload memos use this path.
Scroll bounds and pointer/stick row steps use the measured line layout.

Tests exercise proportional widths with independent expected lines, paragraph
breaks, oversized glyph and invalid-metric failures, real Droid font sizes
16 and 24, the final theme credit, dialog bounds, real frame text-run resolution,
scroll limits, and preservation of the unresolved frame after a layout error.
Single-line labels retain their existing drawing behavior.

## D-pad selection

C main uses newly pressed directions (`bd & PADS_UP/DOWN/DPAD`) for selection.
Rust applied held directions every frame. Up/down/left/right now require new
press events; held page actions keep their existing behavior. The regression
holds Down for 120 frames, checks stable selection, then checks that another
press advances exactly once.

## Verification and limits

- HBC host suite: 235 passed, no failed or ignored tests.
- Shared DI close engine: 2 passed.
- Shared USB startup engine: 1 passed.
- Full USB crate host tests cannot run: its Wii-only dependencies deliberately
  reject a non-PowerPC target. This is not a pass or hardware acceptance evidence.
- Python capture profile tests: 2 passed.
- Both normal and diagnostic Wii release builds: successful, no warnings.
- Source structure audit: passed with three existing keyboard-queue debt notes.
- Implementation inventory: refreshed, 2854 API entries. Only affected source
  declaration line references changed in the evidence CSV; evidence classifications
  and verification claims were preserved.
- Full libogc-rs completeness audit: still fails with 2238 existing findings.
  No audit rule, classification, or expected result was weakened.
- Dolphin readback-enabled capture: the runtime rendering path reaches the
  no-storage notice; text now fits on one line. The final diagnostic DOL was captured again after all code changes; that
  image shows the Home menu with readable labels and the GX test square.
  The captures do not test the real USB device or prove Wii exit behavior.

The next acceptance check is on the Wii: open USB apps, use directional presses,
view/scroll all credits, and return to the System Menu. Do not close the hardware
follow-up merely because the build or host tests pass.

## CARROT deployment

Both final DOLs and metadata were copied to the existing Rust app directories
on CARROT, flushed, and verified against source SHA-256 hashes. Their file
timestamps are 2026-10-07 19:06:18 EDT. Previous card files are preserved
locally under `target/wii-runtime-previous`. C HBC Reference remains unchanged.

Normal DOL SHA-256:
`9b83c7c4de08c342eb99a6863a485b324daf5ae4194765cbcbc7e006024245e7`

Diagnostic DOL SHA-256:
`e0e8ebd4f8ce1d8e4d2c3972755840966fe93c2ed774dc36cfbcb432c2aa9e96`

The final Dolphin XFB image is `target/hbc-runtime-diagnostic-gx.png`.
