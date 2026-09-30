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
  `libogc-rs` copy.

## Remaining runtime work

- `main` does not yet start the full application loop. It initializes the Rust
  heap and returns.
- There is no HBC browser renderer, theme image/font decoder, widget system,
  dialog rendering, or localized text output.
- Device discovery and refresh policy are not connected to runtime startup.
  USB storage candidates now exclude HID devices in the ignored local
  `libogc-rs` copy, but the reported NAND/HID hang needs hardware confirmation.
- The GX FIFO path has no interrupt-based flow control, overflow recovery, or
  draw-completion wait. Its register setup and display output are unverified on
  hardware.
- Executable launch does not yet stop services, restore exception state, set a
  target stack, clear the HBC runtime area, or use HBC's launch trampoline.
- Network receive/update behavior, theme installation UI, and persistent
  settings save timing still need integration.

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
