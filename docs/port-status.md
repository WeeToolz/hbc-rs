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
  input, updates the controller state, draws the initial browser layout with a
  native 5x7 bitmap font, and waits for VI retrace. Return-to-menu and power-off
  actions save settings and call the system reset service.
- Boot setup loads NAND settings, tries the preferred SD or USB volume followed
  by the other volume, scans `apps`, and keeps the mounted volume for later
  operations. Wii Remote setup matches HBC's 640x480 default IR range and
  pointer-owner button priority.

## Remaining runtime work

- The display scene uses solid-color panels and a native 5x7 font for basic
  app labels, descriptions, menus, About content, and confirmation dialogs.
  Startup reports missing SD or USB storage; launch, delete, settings-save,
  and storage-stop failures use a dismissible notice. App icons remain
  placeholders. Theme textures, gradients, localized strings, and HBC widget
  behavior are not ported.
- A pure Rust theme ZIP parser now enforces the HBC archive size, entry count,
  root filename allowlist, duplicate-name policy, and bounded extraction plan.
  It parses `theme.xml` with the existing metadata parser. The Details action
  can apply the theme's default font color for the current session. Theme
  images and fonts are not decoded or shown, progress gradients are not drawn,
  and theme ZIP persistence to NAND is not connected to the UI.
- Device discovery scans one preferred volume synchronously at startup and
  retries SD/USB selection every 30 frames. It unmounts an offline volume,
  remounts available media, and replaces the catalog while preserving the
  selected app where possible. It does not use HBC's background mount worker
  or show per-device status. A missing `apps` directory is treated as an empty
  catalog; other top-level scan errors show a notice. GameCube memory cards are
  not supported. USB storage candidates exclude HID devices, and v5 watcher
  startup has a 100 ms per-host
  bound in the ignored `libogc-rs` copy; the reported NAND/HID hang still needs
  hardware confirmation.
- BootMii IOS availability now uses HBC's bounded TMD-view `BM` marker check.
  Its menu item appears only when the check succeeds; selecting it saves
  settings, stops storage and Wii Remote services, then requests IOS 254 before
  returning to the System Menu if the IOS launch returns. This path is not
  hardware-tested. App deletion asks for an explicit yes response and uses the
  safe folder remover. A canceled delete returns to app details.
- The GX FIFO path has no interrupt-based flow control, overflow recovery, or
  draw-completion wait. Its register setup and display output are unverified on
  hardware.
- ELF/DOL launch now reads the selected file, validates and applies its load
  plan, patches the HBC crt0 byte when present, and writes argv data into the
  reserved area. It saves settings, unmounts storage, shuts down WPAD and system
  services, disables IRQ masks and external interrupts, clears HBC's runtime
  memory through an Arena2 trampoline, and branches to the entry point. It does
  not restore an earlier IRQ exception vector, reload IOS for AHB access, or
  receive real hardware verification. The loaded program's crt0 must set the
  stack and install its own interrupt handlers before enabling interrupts.
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
