# HBC Rust port status

This file tracks behavior coverage. A target check proves that code compiles for
the Wii target. It does not prove behavior on a Wii. Real hardware testing is
deferred.

## Implemented in `hbc-rs`

- Host-testable metadata, settings XML, archive path checks, classic ZIP listing
  and stored/DEFLATE extraction, catalog filtering and sorting, browser input
  policy, and ELF/DOL load-plan validation.
- A FAT catalog adapter, settings persistence through ES/ISFS, app extraction
  planning, and app-folder deletion through the local FAT API.
- Input scanning for PAD and Wii Remotes, a frame-level application state
  driver, a bounded MEM2 allocator, and a Wii executable segment copier with
  cache maintenance and a one-way branch.
- VI/framebuffer setup and a minimal GX FIFO/display-copy path in the local
  `libogc-rs` copy. The Wii entry point now starts a frame loop that scans
  input, updates the controller state, draws a solid-fill browser scene, and
  waits for VI retrace. Return-to-menu and power-off actions save settings and
  call the system reset service.
- Boot setup loads NAND settings, tries the preferred SD or USB volume followed
  by the other volume, scans `apps`, and keeps the mounted volume for later
  operations. Wii Remote setup matches HBC's 640x480 default IR range and
  pointer-owner button priority.

## Remaining runtime work

- The display scene is an initial solid-color layout with placeholder bars.
  It has no text or glyph rasterization, icon and theme textures, gradients,
  localized strings, or HBC widget and dialog behavior.
- Device discovery scans one preferred volume synchronously at startup. There
  is no background mount worker, hotplug handling, 30-frame refresh, or SD and
  USB device status UI. GameCube memory cards are not supported. USB storage
  candidates exclude HID devices, and v5 watcher startup has a 100 ms per-host
  bound in the ignored `libogc-rs` copy; the reported NAND/HID hang still needs
  hardware confirmation.
- Browser launch and delete actions are not connected to the runtime loop.
  The loader still lacks HBC's complete cleanup and launch trampoline.
- The GX FIFO path has no interrupt-based flow control, overflow recovery, or
  draw-completion wait. Its register setup and display output are unverified on
  hardware.
- Executable launch does not yet stop services, restore exception state, set a
  target stack, clear the HBC runtime area, or use HBC's launch trampoline.
- Network receive/update behavior, theme installation UI, and periodic dirty
  settings save timing still need integration. Settings currently save on the
  return-to-menu and power-off actions.

## Local checks

The current host suite runs with `cargo test`. The Wii code has passed a
nightly `cargo check` using `libogc-rs/targets/powerpc-unknown-wii.json` and
`-Z build-std=core,alloc -Z json-target-spec`, with warnings denied. Unit tests
inside target-only `libogc-rs` crates cannot run because the custom bare-metal
target has no `test` crate.

## Out of scope for the first runtime target

The reload stub, banner builder, WiiPAX tool, wiiload host tool, WAD packaging,
and translations remain separate targets or assets. Revisit the reload stub
after the application runtime works.
