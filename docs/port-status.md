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
- A Wii-only Wiiload receiver worker starts asynchronous networking with bounded
  retries, listens on port 4299, filters clients to HBC's local IPv4 `/16`, and
  validates complete uploads through the host-testable protocol decoder. The
  decoder checks exact body length and bounded decompression. The worker queues
  one payload for the main thread. The entry point starts the worker and sends
  validated ELF or DOL uploads through the existing launch path. ZIP uploads
  use a confirmation dialog, then install an app archive or apply a theme.
- Input scanning for PAD and Wii Remotes, a frame-level application state
  driver, a bounded MEM2 allocator, and a Wii executable segment copier with
  cache maintenance and a one-way branch.
- VI/framebuffer setup and a minimal GX FIFO/display-copy path in the local
  `libogc-rs` copy. The Wii entry point now starts a frame loop that scans
  input, updates the controller state, draws the initial browser layout with a
  native 5x7 bitmap font, and waits for VI retrace. The logical viewport now
  follows the Wii 4:3 or 16:9 setting (640x480 or 852x480), with the matching
  bundled or theme background and four grid columns in widescreen mode. The
  video setup also applies HBC's vWii pillarbox register settings.
  Return-to-menu and power-off actions save settings and call the system reset
  service. Physical Reset and Power callbacks queue exit requests. The main loop
  saves settings, unmounts storage, stops Wii Remote services, and requests the
  matching system action.
- Boot setup loads NAND settings, tries the preferred SD or USB volume followed
  by the other volume, scans `apps`, and keeps the mounted volume for later
  operations. Wii Remote setup matches the active HBC viewport's padded IR
  range and pointer-owner button priority.
- Wii Remote 1 opens an Options dialog for SD or USB preference and Name or
  Date sorting. Changes stay pending until OK. Confirmed device changes remount
  the selected volume and refresh the catalog. The two GameCube memory card
  choices are shown as disabled because the current FAT backends do not support
  them. HBC also updates device availability while its dialog is open; Rust
  reports an unavailable SD or USB choice after the user confirms it.

## Remaining runtime work

- The display scene uses solid-color panels and a native 5x7 font for basic
  app labels, descriptions, menus, About content, and confirmation dialogs.
  Startup reports missing SD or USB storage; launch, delete, settings-save,
  and storage-stop failures use a dismissible notice. Missing, invalid,
  oversized, or out-of-memory app icons use placeholders.
  HBC-sized 128x48 app icons load through the native PNG decoder and draw as GX
  RGBA8 textures. The 640x480 background and focused or normal list and grid app
  cards draw from built-in HBC images or matching active-theme PNG assets. The
  320x64 logo always uses the bundled image, as in HBC. Page arrows use HBC's
  64x64 images and appear when a previous or next page exists. Main-menu rows
  use HBC large button images. About and modal
  screens use the HBC dialog background, confirmation icon, About artwork, and
  size-specific normal and focused buttons. Wii Remote IR pointer input now selects and
  activates app cards, page arrows, menus, and dialog buttons using renderer
  hit bounds. App detail descriptions wrap and scroll by line with the Up and
  Down buttons, controller sticks, or pointer dragging. This artwork uses
  bundled defaults. HBC bundles the About image instead of loading it from
  active themes. The storage scan panel uses the theme throbber image, and the
  Wiiload install dialog uses the theme progress frame. Both use bundled C
  artwork when an active theme image is missing or has invalid dimensions.
  Long notices also wrap and scroll by line with Up and Down buttons, controller
  sticks, or pointer dragging, matching the C message-dialog memo. Animated
  bubble backgrounds, network and USBGecko status indicators, memo scroll arrows,
  other memo contexts, localized strings, and complete HBC widget behavior are
  not ported.
- A pure Rust theme ZIP parser enforces a 20 MiB archive limit, a 64-entry
  limit, safe paths, supported ZIP entry types and compression, duplicate-name
  checks, a 64 MiB per-file limit, and a 128 MiB total extracted-size limit.
  Safe unrelated files are ignored. Theme metadata comes from root `theme.xml`,
  and texture loading reads named root assets. Applying a theme saves its
  validated ZIP to NAND. It updates the background, app cards, page arrows,
  throbber, and progress images for the current session. The 320x64 logo always
  uses the bundled image, as in HBC. The native Rust bitmap glyph renderer
  applies default and target-specific font colors and sizes to HBC's
  `label`, `dlgtitle`, `memo`, `appname`, `appdesc`, `button`, and `button_desel`
  text roles. It uses scaled built-in glyphs; custom TTF files are parsed as
  metadata only and are not rasterized. Theme controls and custom fonts are not
  yet drawn. The ZIP install meter uses the four parsed progress-gradient stops,
  with a blue default when a theme has no gradient.
  Startup loads and validates that archive again. Theme PNG resources use the bounded pure
  Rust decoder for grayscale, RGB, indexed, gray-alpha, and RGBA images, including
  Adam7 interlacing. The decoder can convert RGBA output to GX RGBA8 tile order.
- Device discovery mounts and scans one preferred volume on a worker at startup
  and retries SD/USB selection every 30 frames. The main loop pauses browser
  input during a scan and displays a loading panel. It unmounts an offline volume,
  remounts available media, and replaces the catalog while preserving the
  selected app where possible. It does not show per-device status. A missing
  `apps` directory is treated as an empty
  catalog; other top-level scan errors show a notice. GameCube memory cards are
  not supported. USB storage candidates exclude HID devices. USB storage now
  starts and stops only the vendor watcher. Its IOS open and ioctl calls remain
  synchronous. The reported NAND/HID hang still needs hardware confirmation.
- BootMii IOS availability now uses HBC's bounded TMD-view `BM` marker check.
  Its menu item appears only when the check succeeds; selecting it saves
  settings, stops storage and Wii Remote services, then requests IOS 254 before
  returning to the System Menu if the IOS launch returns. This path is not
  hardware-tested. App deletion asks for an explicit yes response and uses the
  safe folder remover. A canceled delete returns to app details.
- The GX FIFO path has no interrupt-based flow control, overflow recovery, or
  draw-completion wait. Its register setup and display output are unverified on
  hardware. A native GX RGBA8 texture path now configures textured 2D state,
  loads texture-map registers, invalidates texture cache, and draws textured
  quads. Icon allocations remain alive for the current catalog and are released
  after catalog refresh or app deletion. This output is also unverified on
  hardware.
- ELF/DOL launch now reads the selected file, validates and applies its load
  plan, patches the HBC crt0 byte when present, and writes argv data into the
  reserved area. It saves settings, unmounts storage, shuts down WPAD and system
  services, disables IRQ masks and external interrupts, clears HBC's runtime
  memory through an Arena2 trampoline, and branches to the entry point. It does
  not restore an earlier IRQ exception vector, reload IOS for AHB access, or
  receive real hardware verification. The loaded program's crt0 must set the
  stack and install its own interrupt handlers before enabling interrupts.
- Wiiload now accepts ELF and DOL uploads through the launch path. It checks
  received app and theme ZIPs, asks before applying them, extracts app files
  below `apps/<folder>`, and saves and applies theme ZIPs through NAND. App
  extraction runs on a worker and displays a loading dialog. It does not show
  per-file progress.
  There is no Rust per-device status UI yet, so the theme network and USBGecko
  status images do not have a runtime consumer. Settings
  save when HBC exits, as in the original. Settings and theme replacement now
  write a temporary NAND file, rename with a backup, and recover an interrupted
  replacement at the next load. Hardware behavior of ISFS rename remains
  unverified. Theme replacement needs enough free NAND space for both the old
  and new archive during the write.

## Local checks

The current host suite runs with `cargo test`. The Wii code has passed a
nightly `cargo check` using `libogc-rs/targets/powerpc-unknown-wii.json` and
`-Z build-std=core,alloc -Z json-target-spec`, with warnings denied. Unit tests
inside target-only `libogc-rs` crates cannot run because the custom bare-metal
target has no `test` crate.

## Bugs found in the original HBC source

- `channel/channelapp/source/tex.c` reads PNG callback data without checking
  the remaining input length. A truncated PNG can make the callback read past
  its buffer. The Rust decoder bounds every chunk and verifies its CRC.
- `channel/channelapp/source/main.c` applies the full pointer drag offset on
  every frame, even though `view.c` measures it from the drag start. This makes
  memo scrolling accelerate as the pointer moves. HBC-rs applies each frame's
  pointer delta once.
- The reported NAND startup hang with a HID controller and no USB drive enters
  HBC's USB storage startup path after SD probing fails. The local `libogc-rs`
  copy filters storage candidates by interface class and now starts only the
  vendor host for storage. HBC-rs now mounts and scans on an LWP worker so a
  stalled IOS storage call does not freeze drawing and input on the main loop.
  The vendor host still makes synchronous IOS calls, so storage discovery may
  still stall and the reported hardware hang needs confirmation.

## Out of scope for the first runtime target

The reload stub, banner builder, WiiPAX tool, wiiload host tool, WAD packaging,
and translations remain separate targets or assets. The C tree includes a POT
template and Dutch, French, German, Italian, Japanese, and Spanish PO catalogs.
HBC-rs has no localization loader or catalog build path yet; it still uses
English UI strings. Revisit the reload stub after the application runtime works.
