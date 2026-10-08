# HBC Rust port status

This file tracks behavior coverage against the published C HBC channel app.
The user has confirmed that the C source runs on a Wii. The Rust port still
needs its own real-hardware verification; a target check proves compilation,
not device behavior.

The machine-checked source map is [the parity ledger](generated/parity-status.md),
backed by `config/parity-ledger.csv`. Run `python3 tools/parity.py check` to
validate its C and Rust source references and generated status report.

The parity target is the running channel application, including the C HBC
support for SD Gecko adapters in GameCube ports A and B. This is FAT storage
on an SD Gecko, not support for proprietary GameCube Memory Cards. The former
exclusions for the network editor, wireless scan, and optional updater are now
under completeness review. Their open findings are recorded in
[the exclusion review](review/former-exclusions-2026-10-07.md). The editor and
scan are Rust extensions without C HBC counterparts. C disables the updater,
uses a placeholder endpoint and signing key, and omits the installer source;
Rust has only the update XML parser. These features remain partial. The reload
stub, banner builder, WiiPAX, host-side wiiload tool, and WAD packaging are
separate targets.

## Implemented in `hbc-rs`

- Host-testable metadata, settings XML, archive path checks, classic ZIP listing
  and stored/DEFLATE extraction, catalog filtering and sorting, browser input
  policy, and ELF/DOL load-plan validation.
- A FAT catalog adapter, settings persistence through ES/ISFS, app extraction
  planning, and app-folder deletion through the local FAT API.
- A Wii-only Wiiload receiver worker starts asynchronous networking with bounded
  retries, listens on port 4299, filters clients to HBC's local IPv4 `/16`, and
  validates complete uploads through the host-testable protocol decoder. The
  decoder checks exact body length and bounded decompression. It accepts uploads
  over TCP and USB Gecko. USB Gecko reads the HAXX header and then the advertised
  data and argument body through the local `ogc-usbgecko` receive API. The
  receiver publishes atomic body byte counts, and the main loop shows a live
  receive dialog. The worker queues
  one payload for the main thread. The entry point starts the worker and sends
  validated ELF or DOL uploads through the existing launch path. ZIP uploads
  use a confirmation dialog, then install an app archive or apply a theme.
- Input scanning for PAD and Wii Remotes, a frame-level application state
  driver, a bounded MEM2 allocator, and a Wii executable segment copier with
  cache maintenance and a one-way branch.
- The animated bubble background, pointer bubble popping, network and USB Gecko
  connection indicators, memo scroll arrows, and theme description in About are
  implemented. Rust imports all six C PO catalogs through
  `tools/generate-localization.py`; the generator checks every message key.
- VI/framebuffer setup and GX FIFO/display-copy rendering in the local
  `libogc-rs` copy. The Wii entry point starts a frame loop that scans input,
  updates controller state, draws the browser with HBC’s bundled Droid fonts,
  and waits for VI retrace. The logical viewport now
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
- Wii Remote 1 opens an Options dialog for SD, USB, and both SD Gecko slots,
  plus Name or Date sorting. Changes stay pending until OK. Confirmed device
  changes remount the selected volume and refresh the catalog. HBC also updates
  device availability while its dialog is open; Rust probes SD, USB, and both
  SD Gecko slots on a worker while Options is open, then displays the completed
  availability snapshot without blocking the main loop. The worker
  leaves existing mounts registered and unmounts only a volume it mounted itself.
- Screenshot capture matches the enabled C controller chord: hold Nunchuk Z or
  Classic ZR and press Nunchuk C or Classic ZL. Rust reads the rendered EFB,
  writes an RGBA PNG as the next unused `hbc-000.png` to `hbc-999.png` on the
  selected mounted volume, and reports capture, encode, write, or cleanup errors.
  Host tests and the Wii target build pass; capture and file output still need
  verification in Dolphin and on Wii hardware.

## Remaining runtime work

- HBC resets the first four bytes of the System Menu `play_rec.dat` file
  before settings load. Rust now checks the file length, writes through an aligned ISFS buffer,
  closes the handle, and reads the value back. A reset failure appears as a
  warning. A file shorter than four bytes stops the write.
- The display scene uses HBC’s full-screen background without Rust-only
  header/footer bands. HBC’s Droid TrueType faces
  render labels, descriptions, menus, About content, and confirmation dialogs.
  Startup reports missing SD or USB storage; launch, delete, settings-save,
  and storage-stop failures use a dismissible notice. Missing, invalid,
  oversized, or out-of-memory app icons are omitted, as in C.
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
  active themes. The storage scan state uses the theme throbber image, and the
  Wiiload install dialog uses the theme progress frame. Both use bundled C
  artwork when an active theme image is missing or has invalid dimensions.
  Long notices also wrap and scroll by line with Up and Down buttons, controller
  sticks, or pointer dragging, matching the C message-dialog memo. App details,
  About, notices, and Wiiload confirmation use Rust memo handling, including memo
  scroll arrows. Per-file Wiiload app extraction progress is drawn from bytes
  written. Dolphin and Wii visual checks remain pending. The 5x7 glyph renderer
  is a fallback when a TrueType face is unavailable.
- A pure Rust theme ZIP parser enforces HBC's `LD_MAX_SIZE` and 20 MiB archive
  limits, a 64-entry
  limit, safe paths, supported ZIP entry types and compression, duplicate-name
  checks, a 64 MiB per-file limit, and a 128 MiB total extracted-size limit.
  Non-empty files must be root-level HBC assets, TTF fonts, or TXT files.
  Theme metadata comes from root `theme.xml`,
  and texture loading reads named root assets. Applying a theme saves its
  validated ZIP to NAND. It updates the background, app cards, page arrows,
  throbber, and progress images for the current session. The 320x64 logo always
  uses the bundled image, as in HBC. The Rust text renderer uses HBC's bundled Droid regular and bold fonts by
  default. Theme TTF files and target-specific font choices are rasterized and
  cached through `fontdue` for the `label`, `dlgtitle`, `memo`, `appname`,
  `appdesc`, `button`, and `button_desel` roles. The ZIP install meter uses the four parsed progress-gradient stops,
  with a blue default when a theme has no gradient.
  Startup loads and validates that archive again. Theme PNG resources use the bounded pure
  Rust decoder for grayscale, RGB, indexed, gray-alpha, and RGBA images, including
  Adam7 interlacing. The decoder can convert RGBA output to GX RGBA8 tile order.
- Device discovery mounts and scans one preferred volume on a worker at startup
  and retries SD/USB selection every 30 frames. The main loop pauses browser
  input during a scan and displays a loading panel. It unmounts an offline volume,
  remounts available media, and replaces the catalog while preserving the
  selected app where possible. Options displays SD and USB availability. A missing
  `apps` directory is treated as an empty
  catalog; other top-level scan errors show a notice. SD Gecko adapters in both
  GameCube ports use the FAT SDGecko backend. USB storage candidates exclude HID
  devices. USB storage now
  starts and stops only the vendor watcher. Its IOS open and ioctl calls remain
  synchronous. The reported NAND/HID hang still needs hardware confirmation.
- BootMii IOS availability now uses HBC's bounded TMD-view `BM` marker check.
  Its menu item appears only when the check succeeds; selecting it saves
  settings, stops storage and Wii Remote services, then requests IOS 254 before
  returning to the System Menu if the IOS launch returns. This path is not
  hardware-tested. App deletion asks for an explicit yes response and uses the
  safe folder remover. A canceled delete returns to app details.
- The local GX FIFO uses interrupt-based draw completion and overflow handling,
  with a bounded completion wait. Register setup and display output remain
  unverified on hardware. A native GX RGBA8 texture path configures textured 2D state,
  loads texture-map registers, invalidates texture cache, and draws textured
  quads. Icon allocations remain alive for the current catalog and are released
  after catalog refresh or app deletion. This output is also unverified on
  hardware.
- ELF/DOL launch now reads the selected file, validates and applies its load
  plan, patches the HBC crt0 byte when present, and writes argv data into the
  reserved area. It saves settings, unmounts storage, shuts down WPAD and system
  services, applies the AHB patch for apps that request AHB access, reloads the
  preferred IOS, disables IRQ masks and external interrupts, clears HBC's
  runtime memory through an Arena2 trampoline, and branches to the entry point.
  The Rust runtime's external-interrupt and LWP decrementer exception vectors
  are restored before handoff. AHB patch and IOS reload failures return to the
  System Menu as narrow safety divergences from C. AHB launches re-enable DVD
  access after the IOS reload, matching the C handoff sequence. The loaded crt0
  must set
  the stack and install its own interrupt handlers before enabling interrupts.
  This path is not verified on Rust hardware.
- Wiiload now accepts ELF and DOL uploads through the launch path. It checks
  received app and theme ZIPs, asks before applying them, extracts app files
  below `apps/<folder>`, and saves and applies theme ZIPs through NAND. App
  extraction runs on a worker and displays per-file progress based on bytes
  written. Archive, storage, catalog refresh, and worker-join failures reach the
  UI with stage-specific error notices. App files are written below a hidden
  staging folder. HBC checks the staged executable, moves the prior app folder
  to a hidden backup, and swaps in the completed folder. If catalog refresh
  fails, HBC restores the prior folder. Startup recovers interrupted stage and
  backup moves. Recovery checks whether two names point to the same FAT
  directory cluster before removing an interrupted-rename alias; it unlinks only
  that duplicate entry and keeps the shared data. A backup remains in place if
  cleanup fails, and HBC reports it.
  Directory rename and interrupted-install recovery still need Wii testing.
  The browser displays the network and USB Gecko status indicators. Settings
  save when HBC exits, as in the original. Settings and theme replacement now
  write a temporary NAND file, rename with a backup, and recover an interrupted
  replacement at the next load. Hardware behavior of ISFS rename remains
  unverified. Staged settings and theme bytes are read back and compared before
  the old file moves. Recovery validates the primary before removing its backup;
  it restores a valid backup when the primary fails validation and keeps both
  files if neither validates. A settings load error blocks later saves so HBC
  does not overwrite unreadable settings with defaults. Theme replacement needs
  enough free NAND space for both the old and new archive during the write.
- Rust-only extension: Options includes a themed WiFi profile editor with a full
  on-screen keyboard. This is not part of the C HBC parity claim.
  It edits the active wireless profile in `/shared2/sys/net/02/config.dat`,
  validates profile flags and the SSID and security key, preserves other config
  bytes and file attributes, and checks for a stale snapshot before commit.
  HBC stops and restarts the Wiiload receiver around the save, then reloads the
  committed config. HBC polls for the
  receiver's assigned IPv4 address for up to 60 seconds without blocking the UI.
  It reports the address only when Wiiload is listening. Read, save, restart,
  and connection timeout failures are shown in the shared notice dialog. This
  NAND config and connection check still need real Wii validation.
- Rust-only extension: Network setup can scan visible access points through the
  `ogc-wd` Wii
  wireless driver API. It shows detected SSIDs and security modes, then fills
  the selected SSID and recognized mode in the editor. Hidden SSIDs need
  manual entry. Unknown security modes do not change the selected mode.
  Scanning stops and restarts the Wiiload receiver and reports restart errors.
  Scan output, router security modes, and receiver recovery need Wii hardware
  validation.
- System exit and app or BootMii handoff now stop if required save, unmount,
  WPAD shutdown, or Wiiload worker join steps fail. A failed Wiiload join keeps
  the worker marked as started. If BootMii IOS launch returns an error after
  input services stop, HBC returns to the System Menu.

## Local checks

The current host suite runs with `cargo test`. The Wii code has passed a
nightly `cargo check` using `libogc-rs/targets/powerpc-nintendo-none.json` and
`-Z build-std=core,alloc -Z json-target-spec`. One existing diagnostic-build
function is unused in this configuration. Unit tests
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

## Open parity and verification work

- The source review is complete for the tracked HBC paths. Screenshot capture
  is implemented; controller, EFB and PNG file output remain pending runtime
  verification. HBC scene artwork is
  implemented; visual comparison on Dolphin and Wii remains pending. App
  scanning now streams directory entries and stops
  after C’s 1024 valid-app limit, so many unrelated files do not cause a false
  scan failure. The former scope exclusions are now reviewed as partial work;
  see the exclusion review for network and updater findings.
- Verify SD Gecko A/B insertion, mounting, scanning, selection, app loading, hot
  removal, and reinsertion on Wii hardware with actual adapters. Rust probes
  physical presence and routes both choices through `ogc-fat`'s SDGecko backend;
  source and API inspection do not prove hardware behavior.
- App launch source parity now includes HBC's AHBPROT ES patch before the IOS
  reload, DVD-access re-enable after reload, and restoration of the Rust runtime's
  external-interrupt and LWP decrementer vectors before handoff. The Rust port
  deliberately returns to the System Menu if the requested AHB patch or IOS
  reload fails instead of continuing with a partially prepared launch; this is a
  narrow safety divergence from C, which logs or ignores those failures and
  continues. The launch path still needs real-Wii verification.
- Verify GX display and texture output, storage discovery, app launch, BootMii
  launch, NAND replacement and recovery on a real Wii. Confirm whether
  startup hangs with a HID controller and no USB drive.
- Verify staged Wiiload app replacement and recovery after write, rename, and
  catalog-scan failures on Wii storage. The implementation uses the local FAT
  directory-rename operation; host tests cover the shared commit and rollback
  policy, but they do not verify FAT behavior on the console.
- Verify Wiiload socket-close and network-deinit failures on Wii hardware. These
  failures now stop the receiver, set a visible cleanup error, and block a later
  app or system handoff.
- Verify reading, saving, backup recovery, and the assigned IP check after
  changing the Wii network profile on hardware. Confirm that Wiiload returns
  without a console restart after a profile change.

## Other scope notes

The C translation catalogs remain the source for the generated Rust catalog.
The reload stub remains a separate target.
