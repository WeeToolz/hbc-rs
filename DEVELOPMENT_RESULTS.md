# Development results

## Independent USB volumes and DRY review — 2026-10-23

The shared SDK now owns two independent USB drive contexts. HBC adds `usb2:`
at saved device index 4 and uses one catalog and DOL launch path for all volumes.
No fixed physical port mapping is claimed. Failed close retains device ownership;
removal and cleanup affect only the selected drive.

All six supplied DRY findings are addressed. Network persistence tests now use
the production transaction engines; a shared recovery accumulator owns precedence.
Network work shares receiver stop and restart ordering, both capture tools use
one checked debugger parser, rectangle validity has one owner, and startup uses
the parser's theme archive size limit. Host coverage includes real failure
boundaries and retained recovery bytes.

Clippy passes for default and network host/PowerPC configurations and for both
valid diagnostic/acceptance debugger configurations with warnings denied.
The intentionally invalid all-features combination is rejected because network
and USB Gecko debugger modes are mutually exclusive. That guard was preserved.

The final host/Python, SDK, and Dolphin results and artifact identities are in
[USB and DRY evidence](docs/review/evidence/usb2-storage-2026-10-22/README.md).
The detailed [source review](docs/review/usb2-storage-and-dry-2026-10-23.md)
records the six fixes and remaining hardware checks.

Fresh Dolphin HID probes pass descriptor checks (8/8) and backend checks (7/7).
They do not prove physical HID reports or actual HID enumeration. Physical
SD/USB, simultaneous two-media operation, live HID input with both drives, and
return-stub recovery remain pending. No physical Wii or original NAND was changed.

## Reborn storage loading and Clippy — 2026-10-22

- All four Clippy configurations pass with warnings denied: host default,
  host network debugger, PowerPC default, and PowerPC network debugger.
  No lint was suppressed. All 387 host tests pass; none are ignored.
- Reborn metadata keeps `ahb_access` and `no_ios_reload` separate. The shared
  storage launch policy now preserves both flags. Tests cover all four pairs.
- A real Dolphin SD launch exposed a child listener bind failure. Checked
  `SO_REUSEADDR` setup fixes the restart without enabling port sharing.
- Scripted Dolphin launches read the DOL from the emulated SD card. The final
  default and no-reload cases both start a new HBC session. Load-to-ready times
  are 15.225 s and 14.923 s. Actual IOS58 launch counts are one and zero.
- The existing parity ledger now has 41 validated contracts and references the
  authoritative Reborn source. Two Python regression tests pass.
- USB port 0 and port 1 were not runtime-tested. Reborn's two independent USB
  drive contexts and `usb2:` volume remain open parity work. Physical SD,
  USB, AHB-enabled launch, and return-stub recovery remain unverified.
- The SDK source audit passes. Its completeness audit retains 2,371 existing
  findings; no SDK source or audit rule was changed.

Evidence: [checks, screenshots, logs, and limits](docs/review/evidence/reborn-storage-parity-2026-10-22/README.md).
Source comparison: [Reborn storage loading](docs/review/reborn-storage-loading-2026-10-22.md).

## Loader and cleanup DRY refactor — 2026-10-22

The loader now owns the executable, argument, and runtime-clear boundaries.
Startup and the PowerPC trampoline consume the shared definitions. Application
launch, BootMii, and system exit use one service cleanup adapter with typed
backend errors. Existing shutdown orders and public APIs are preserved.

All 385 host library tests pass. The PowerPC build passes, and its 524-byte
trampoline is byte-for-byte identical to the previous tested build. The normal
WAD and matching Wiiload DOL pass the Dolphin regression: 2.083 seconds to WAD
readiness, then 2.070 seconds from upload start to a new HBC session. Screenshots
and runtime logs confirm both instances. Formatting, parity-ledger validation,
and the SDK source audit pass. Clippy still fails on seven existing findings
outside the changed files. SDK completeness still fails on 2,371 existing
findings. Full evidence and scope limits:
`docs/review/evidence/load-path-dry-2026-10-22/README.md`.

Physical Wii launch/return, SD/USB launch, and BootMii runtime remain untested
by this change. No physical fix is claimed. Existing worktree edits are kept.

## Dolphin WAD to Wiiload DOL launch — 2026-10-22

The normal debug WAD became ready on TCP 4299 in 2.111 seconds from Dolphin
process launch. Its read-only HBC command response reported session 1028.
The matching 2,454,208-byte HBC-RS DOL was then sent through Wiiload once.
The sender completed in 0.129 seconds. A new HBC instance answered with session
3990 at 2.102 seconds from sender start, or 1.973 seconds after sender completion.
The log proves transfer of executable control, IOS58 reload, a second startup,
and the child Wiiload listener. Screenshots show HBC before and after upload.
Evidence: `docs/review/evidence/wad-wiiload-selfload-2026-10-22/`.

This confirms that the packaged HBC-RS can load its matching DOL through Wiiload
in Dolphin. The previously stated general DOL-load failure does not apply to
this tested path and artifact. This run does not test other DOLs, physical Wii
launch, or return-stub execution. It used a disposable NAND copy and stopped
cleanly. Startup had a 180-second allowance for each stage; readiness required
a valid protocol response, and a new HBC session was required after upload.

Screenshot capture now searches Dolphin's title subdirectories and gives
images unique filenames. The nested-directory regression test passes. Python
syntax validation and saved launch-log checks pass. Correction: title
`000000010000003a` in the earlier logs is IOS58, not the System Menu. Those
entries do not prove Priiloader return. The earlier 30/120-second recovery
waits began at test resume, before the recovery deadline; they were not full
post-return HBC startup allowances.

## Debug channel WAD — 2026-10-22

Prepared `dist/hbc-rs-debug-2026-10-22/hbc-rs-debug-OHBC.wad` with the native Rust `gc-wad` tool in libogc-rs. Size: 3,096,896 bytes. SHA-256: `157c9609a72c3d7535e0c6f6aaf5947d31cd94da7a59b9e7a2811aa0209a755e`. The package contains the matching DOL and source-debug ELF. See `docs/debug-channel-wad.md` and the package manifest for inputs, build command, source state, hashes, and limits.

The independent Reborn packer produced exactly the same WAD bytes. Rust decryption verified both content SHA-1 records. The metadata is legacy fakesigned, with root trust unchecked. The package uses the existing OHBC title ID and IOS58.

Dolphin initially failed its Qt CPU feature check inside the sandbox. It ran with the required execution permission. A new NAND copy under WiiDev/tmp was used for each run. The test reached `_start`, `main`, and the first render call. HBC preserved the caller's pre-existing STUBHAXX bytes. It did not install the bundled stub in that run. The emulator process stopped cleanly and the temporary NAND copy was removed. Evidence: `docs/review/evidence/channel-wad-2026-10-22-r2/`.

Host validation: 18 gc-wad/gc-elf tests passed, with no ignored tests. Clippy passed with warnings denied. The WAD ledger validator test passed. The ledger's source and evidence references and generated report are current. Source-structure, crate-role, dependency-direction, and inventory checks passed. Three existing keyboard queue audit-debt notes remain visible.

Repository-wide checks still fail: review-status reports missing entries for gdb-transport and ogc-report-protocol; runtime completeness reports 2,371 disallowed API implementations. These are outside the new host WAD tool. No audit requirement was suppressed. The new gc-elf channel API needs independent review, and gc-wad remains marked review_incomplete.

No physical Wii was modified. Physical channel boot, Priiloader dispatch, and the physical return failure remain open. The corrected bundled stub remains a confirmed prior fix; this packaging task does not establish a physical return fix. The WAD's network listener and cooperative control were not exercised in this WAD run. The physical real-mode vector path is also unverified.


## Dolphin channel install — 2026-10-22

The user opened the prepared Dolphin profile and showed the HBC screen. Its OHBC title is installed under `000100014f484243`. The installed ticket, TMD, and boot content match the WAD's decrypted payload; content hashes also pass. Dolphin can therefore boot the packaged HBC as a channel in this profile. The older LULZ title `000100014c554c5a` remains installed. The displayed `Load Wii System Menu 4.3U` item is disabled while Dolphin runs the channel; the System Menu TMD is present in this profile. The original Dolphin NAND was not changed. Details: `docs/review/evidence/channel-install-2026-10-22/session.json`.

This confirms Dolphin channel import and launch. It does not verify Priiloader dispatch or physical return on a Wii. The WAD is legacy fakesigned. Dolphin's successful boot does not show that a retail IOS accepts its metadata signatures.

## Dolphin recovery-validation failure — 2026-10-22

The focused `recovery-validation` DOL (2,497,312 bytes; SHA-256 `e928937a845b3890864831adcff316a55662b4f0a187ebafd9715b1c7b3a6e9b`) passed Wiiload readiness at `192.168.1.52`, uploaded successfully, and reached the visible `RECOVERY TEST HOLD`. It did not return to the HBC loader after its 30-second deadline. The post-test readiness check found port 4299 refusing connections.

The user supplied a macOS hang report for Dolphin 2609a (PID 60570), unresponsive for about 103 seconds. The UI main thread waited in `CPU::CPUManager::SetStepping(bool)` and the CPU-GPU thread was waiting on an atomic semaphore, with its last scheduled time about 160 seconds earlier. This supports a Dolphin emulation/UI hang during guest recovery. It does not identify the exact guest instruction or prove the specific reset stage that blocked.

Source tracing shows the recovery worker calls `ogc_system::exit(0)`. When the reload stub is present, `sys_exit_to_loader` calls `sys_reset_system(SYS_SHUTDOWN, 0, 0)` before jumping to `0x80001800`. The hang report has no guest reset-stage trace. Exact cause is unconfirmed; recovery remains failed and open. Do not repeat the upload until instrumentation or a reviewed route change can distinguish the guest reset stages. No physical Wii or NAND was modified. Evidence: `docs/review/evidence/recovery-validation-dolphin-2026-10-22/`.

## DRY cleanup — 2026-10-22

The six approved cleanup items are implemented. Existing worktree changes were
committed first, in groups for executable loading, localization, runtime
integration, terminal tools, and integration documentation. Cleanup commits follow
those groups. The sibling `libogc-rs` repository was inspected but not changed.

### Changes

- Local and remote analog scroll use the same axis selection, deadzone, and scale.
  The Right Joy-Con uses right Y. Other remote kinds use left Y. The calculation
  divides the normalized axis by 256, applies the deadzone of 10, then divides by
  eight. Explicit remote scroll remains a separate pixel value.
- NAND settings, theme, and replacement reads share size checks and close handling.
  Settings retain their fixed buffer. Themes retain bounded allocation. The shared
  read engine keeps every IPC destination aligned after short reads. Operation,
  close, and later temporary-file cleanup failures remain visible together.
  An oversized NAND read now has an explicit invalid-size error.
- Menu restart, connection tests, and terminal restart share upload guards and
  connection checks. Worker submission is pending. Completion requires a listening
  receiver with an IP address. Receiver failures are returned. The menu timeout is
  60 seconds of elapsed time, rather than 3,600 rendered frames.
- App ZIP classification uses the extraction planner for common entry validation.
  Theme parsing uses that planner for duplicate-name checks. App and theme rules
  remain separate. Direct archive extraction retains its validation checks.
- ZIP tests use one fixture encoder. A fixed ZIP from Python's independent
  `zipfile` implementation verifies parser output and the known CRC32 value
  `0xcbf43926` for `123456789` without the shared fixture encoder.
- Terminal clients share exact TCP reads. Fragmented reads, EOF, timeout, and
  transport errors retain their error results. Hardware commands are still sent
  only once.
- Setup instructions now use the sibling `../libogc-rs` directory. The localization
  generator uses the crate's Rust 2024 edition for formatting. Host builds exclude
  hardware-only controller imports.

### Checks

| Check | Result |
| --- | --- |
| Initial `cargo test --lib` | 359 passed |
| Final `cargo test --quiet` | 371 passed |
| `cargo test --features startup-diagnostics,debugger-network --quiet` | 392 passed |
| `cargo test --features legacy-main-menu --quiet` | 371 passed |
| Terminal client and transport tests, `test_hbc_*.py` | 21 passed; real loopback sockets used |
| XFB capture tests | 3 passed |
| Dolphin capture configuration tests | 2 passed |
| `python3 tools/parity.py check` | Passed; 39 areas |
| `python3 tools/generate-localization.py --check` | Passed; all six C PO catalogs match |
| Normal PowerPC target check | Passed |
| PowerPC check with network debugger, startup diagnostics, and both recovery features | Passed |
| PowerPC check with USB Gecko debugger, startup diagnostics, and legacy menu | Passed; three existing unused-function warnings |
| `cargo clippy --lib --tests` | Completed; seven existing warnings |
| Complete committed diff whitespace check | Passed |

PowerPC checks used the nightly compiler, the sibling
`libogc-rs/targets/powerpc-nintendo-none.json` target, and
`-Z build-std=core,alloc -Z json-target-spec`. The installed `cargo` executable is
not a rustup proxy. The checks used `rustup run nightly cargo` with `RUSTC` set to
that toolchain's compiler.

The seven Clippy warnings are six `collapsible_if` suggestions in existing code
and the existing public `Request::decode` API that returns `Result<_, ()>`.
The stricter Clippy command with `-D warnings` therefore fails. No lint was
suppressed. The localization check initially found an edition-format difference;
the generator edition correction resolves it without changing translation data.

The terminal tests initially failed because the sandbox blocked loopback binds.
The complete suite passed with the required local socket permission.

### Verification limits

No Wii upload, persistent device write, Dolphin run, or real Wii probe was
performed. Host tests and target checks do not prove real hardware behavior.
Real Wii checks remain open for NAND read failures, network restart completion,
and controller scroll behavior. Follow the mandatory hardware procedure before
those checks. The existing binary stack audit was not run because this task did
not produce a linked PowerPC ELF.

## Follow-up DRY cleanup — 2026-10-22

The five items from the repository cleanup audit are implemented. The worktree
was clean before this work. Each item has a separate code commit. This section
records the checks for this follow-up, separately from the earlier six-item work.

### Changes

- App ZIP validation, installation, and startup recovery share the install path
  rules in `management`. Stage and backup names retain their existing format.
  The 243-byte app folder limit is derived from the 255-byte component limit,
  the leading dot, and the longest artifact suffix. Recovery checks the shared
  bounds before it can rename or remove an artifact. ZIP validation uses the
  shared validator without allocating target, stage, and backup path strings.
- Local and remote Right Joy-Con input use the same stick projection and the
  same threshold. Remote input is now neutral at exactly -8192 and 8192, as
  local input already was. Local connection tracking and remote hold/release
  handling retain their separate state.
- Main menu count, labels, and action selection use one typed item sequence.
  The renderer retains the normal and legacy layout coordinates. BootMii
  availability still controls whether its item appears.
- Details, notices, and Wiiload confirmation share partial-line accumulation
  and scroll bounds. Each memo retains its own pixel remainder. Detail input
  retains its opposite sign. Sign reversal uses a widened integer.
- Remote replies use the existing UTF-8 byte truncation helper. Reply size
  fields and bytes are checked at ASCII and multibyte boundaries.

Five new regression tests cover install names and bounds, both stick axes at
threshold edges, all menu labels and actions, memo direction changes and limits,
and encoded UTF-8 reply bytes. The existing persistence, rollback, input release,
menu pointer, memo drag, and independent memo remainder tests also pass.

### Checks

| Check | Result |
| --- | --- |
| Baseline `cargo test --lib --quiet` | 371 passed |
| Final `cargo test --quiet` | 376 passed |
| `cargo test --features startup-diagnostics,debugger-network --quiet` | 397 passed |
| `cargo test --features legacy-main-menu --quiet` | 376 passed |
| `cargo clippy --lib --tests` | Completed; seven existing warnings |
| Normal PowerPC target check | Passed |
| PowerPC check with startup diagnostics, network debugger, and both recovery features | Passed |
| PowerPC check with startup diagnostics, USB Gecko debugger, and legacy menu | Passed; three existing unused-function warnings |
| `python3 tools/parity.py check` | Passed; 39 areas |
| `python3 tools/generate-localization.py --check` | Passed; all six C PO catalogs match |
| Rustfmt check of all seven changed Rust files | Passed |
| Repository `cargo fmt -- --check` | Failed on existing formatting in `src/font.rs`, `src/platform/mod.rs`, and `src/png.rs` |
| Complete change diff whitespace and file-mode review | Passed; no file-mode changes |

The formatting differences also occur in the source at the starting commit,
`b282e81`. Those files were not changed by this cleanup. The Clippy warnings
remain the six existing collapsible-if suggestions and the existing public
`Request::decode` API returning `Result<_, ()>`. No warning was suppressed.
Two new host dead-code warnings found during the first Clippy run were fixed
by compiling the path construction and recovery adapter for Wii and tests;
the shared validation remains available to normal host builds.

PowerPC checks used the nightly compiler, the sibling
`libogc-rs/targets/powerpc-nintendo-none.json` target, and
`-Z build-std=core,alloc -Z json-target-spec`, with `RUSTC` set to the nightly
compiler. The sibling repository was not changed.

### Review and verification limits

The complete source diff was reviewed for discarded failures, unconditional
success, unused arguments, validation bypasses, and changes to storage commit
order. The new shared rules do not replace backend operations. Existing backend
errors still reach their callers. The parity source-reference audit passed.

No Wii upload, persistent hardware write, Dolphin run, or real Wii probe was
performed. Host tests and target checks do not prove hardware behavior. Real Wii
acceptance remains open for interrupted-install recovery and controller input.
Follow the mandatory hardware procedure before those probes. A linked PowerPC
ELF was not produced, so the binary stack audit was not run.

## Follow-up integration tests — 2026-10-22

Added three tests for gaps found after the five-item DRY cleanup. Production
code is unchanged.

- ZIP classification accepts both ASCII and multibyte UTF-8 app folder names
  at 243 bytes and rejects them at 244 bytes. The test checks the actual relative
  extraction operations, total payload bytes, and stage and backup component
  lengths of 254 and 255 bytes.
- A decoded Right Joy-Con wire packet passes through the production mailbox.
  The test checks pending versus applied completion, press and release edges at
  both threshold boundaries, rearming, held input, expiry, and a single release.
- A production menu frame supplies the labels and hit targets used for pointer
  activation. The test checks each displayed action and resulting screen at
  both viewport widths, with BootMii available and unavailable. It runs in both
  normal and legacy menu builds. Expected labels and actions are independent
  of the shared menu definitions.

| Check | Result |
| --- | --- |
| `cargo test --quiet` | 379 passed; no ignored tests |
| `cargo test --features legacy-main-menu --quiet` | 379 passed; no ignored tests |
| `cargo test --features startup-diagnostics,debugger-network --quiet` | 400 passed; no ignored tests |
| `cargo clippy --lib --tests` | Completed; the same seven existing warnings |
| `python3 tools/parity.py check` | Passed; 39 areas |
| `python3 tools/generate-localization.py --check` | Passed; all six catalogs |
| Rustfmt check of the three changed Rust files | Passed |
| Complete diff whitespace and file-mode review | Passed; no file-mode changes |

These tests call the production parser, mailbox, renderer, and navigation code.
No hardware model or replacement backend was added. Real Wii input and recovery
verification remain open. No hardware operation, Dolphin run, or new PowerPC
build was performed for this test-only change.

## Menu and memo state-change tests — 2026-10-22

Added two regression tests after reviewing the existing cleanup and integration
coverage. Production code is unchanged.

- BootMii availability changes while the menu is open: focus clamps when a row
  disappears, a stale pointer target cannot execute an action, and controller
  navigation reaches the correct last action after the row returns. The test
  runs with both normal and legacy menu builds.
- Memo remeasurement: details, notices, and Wiiload confirmation clamp their
  positions immediately when the measured scroll limit decreases. A zero limit
  prevents further scrolling. Zero and maximum unsigned line heights are handled
  without division by zero or a negative signed height. Returned scroll positions
  are checked through the production controller API.

| Check | Result |
| --- | --- |
| `cargo test --quiet` | 381 passed; no ignored tests |
| `cargo test --features legacy-main-menu --quiet` | 381 passed; no ignored tests |
| `cargo test --features startup-diagnostics,debugger-network --quiet` | 402 passed; no ignored tests |
| `cargo clippy --lib --tests --message-format=short` | Completed; the same seven existing warnings |
| `python3 tools/parity.py check` | Passed; 39 areas |
| `python3 tools/generate-localization.py --check` | Passed; all six catalogs |
| Rustfmt check of `src/app.rs` | Passed |
| Complete diff whitespace and file-mode review | Passed; no file-mode changes |

The worktree was clean at the start. The existing integration tests in commit
`6f246bc` were preserved and included in these runs. No hardware operation,
Dolphin run, or PowerPC build was performed for this test-only change. Real Wii
input and interrupted-install recovery verification remain open.


## Dolphin Wii Remote input diagnosis — 2026-10-23

The installed Dolphin reports version 2609a. Runs r6, r7, and r8 used the
existing hash-checked `dist/hbc-ui-acceptance-2026-10-23` package and a disposable
copy of the local NAND. They did not change the physical Wii or the source NAND.
Evidence is in `docs/review/evidence/ui-acceptance-dolphin-2026-10-23-r6/`,
`ui-acceptance-dolphin-2026-10-23-r7/`, and
`ui-acceptance-dolphin-2026-10-23-r8/` under the same evidence directory.

Dolphin discovers both Pipe devices. HBC reaches Bluetooth Ready, then reports
Bluetooth Failed about five seconds later. No pointer press is observed. The
harness now reports the guest Bluetooth failure instead of a pointer timeout.
The UI acceptance test remains failed; pointer accuracy, bubble pops, sorting,
and the network scan remain unverified.

Pipe X and Y now map to Wii Remote 1 and 2. The previous statement that Pipe
cannot provide these Wii buttons was incorrect: normal controller mappings
translate the generic Pipe buttons. The writer emits independent press and
release commands. This change has unit coverage but no successful guest button
acceptance evidence yet. Bluetooth and Wii Remote logging are enabled; UI runs
request debug verbosity, though the release binary did not emit the required
per-command debug details.

A source trace identifies a possible protocol incompatibility. ControllerManager
starts Wii guest search first. HidHost::search_wii_guest queues PERIODIC_INQUIRY
(opcode 0x0403). The examined Dolphin source dispatcher has no handler for that
opcode and its default branch creates a Command Complete without return bytes.
The Rust event parser requires a status byte and rejects that packet as malformed.
A pending command can then time out. This is a source-based hypothesis, not a
confirmed runtime opcode diagnosis. The package predates the current main commits;
its full build identity is preserved in each result.json. Confirm the failed
opcode from that actual runtime before changing the production protocol.

Dolphin reference source:
https://github.com/dolphin-emu/dolphin/blob/409881c34357d0f105fd473167d15ab0bd9c1628/Source/Core/Core/IOS/USB/Bluetooth/BTEmu.cpp

The harness also assumes Pipe axes map linearly to guest screen pixels. Dolphin
projects a virtual sensor bar through its camera, so that assumption still needs
measurement after guest controller delivery works. No coordinate tolerance or
acceptance requirement was reduced.

Validation: all 51 Python tests pass outside the sandbox. The first sandbox run
could not use the shared temporary directory or local sockets; it was repeated
with the required access. The parity check passes for all 41 areas. The final
Dolphin run r8 fails with the explicit guest Bluetooth error and exits cleanly.
No physical Wii test was performed.


## C HBC Reborn control run — 2026-10-23

At the user's request, the built C HBC Reborn was run with the same Dolphin
2609a Pipe mappings, background input, three SD fixture entries, copied source
NAND, and screenshot hotkey as the Rust UI harness. The shared
`configure_profile`, `make_sd_fixture`, `set_initial_sort`, `RemotePipe`, and
`capture_screenshot` implementations were used. The C build has no Rust UI
markers, so the input results were checked from saved screenshots.

The C DOL passed the setup check. Its SHA-256 is
`a32c35c20e2a18ba3004aefbcea9ac10af555f0d068241e9332d6840b28fe5f6`.
The exact input is
`/Volumes/SSD/WiiDev/C-sources/hbc-reborn/channel/channelapp/channelapp-channel.dol`.
Final evidence is in
`docs/review/evidence/ui-input-c-reference-dolphin-2026-10-23-r4/`.
All six screenshots were inspected: pointer at center, upper-left and lower-right;
button 1 opens Options; pointer A opens Birch app details; button 2 changes the
browser to its grid view. Dolphin exits cleanly. This proves that the shared
Pipe setup delivers real Wii Remote pointer and button input to a working guest.
Exact pixel accuracy, bubble pops, Date/Name sorting, and UI network scanning
were not acceptance-tested in this C comparison. The C UI displays fixture
folder names and no descriptions; its fixture metadata interpretation is not
claimed to match the Rust parser.

The first C test used the retail WAD with SHA-256
`16a728d46766be2ee866acc76ffc1d80dc75aed633696e9a9dbc875713ed0176`.
It did not save a frame and its log repeatedly records PI FIFO resets. That run
is a failure, separately retained in `ui-input-c-reference-dolphin-2026-10-23-r1`.
Its large logs were compressed and checked byte-for-byte before the original
log files were removed. The successful DOL run does not establish WAD boot parity.
An early DOL run sent button 2 during dialog close and did not change view;
waiting one second after each B release made the full final sequence pass.

The comparison runner is `tools/probe_hbc_reference_input_dolphin.py`. It records
capture completion separately from screenshot review. No physical Wii operation
or C source modification was performed. All 51 Python tests and the 41-area
parity check pass. The new runner passes Python compilation and diff whitespace
checks.


## Rust HBC UI acceptance — 2026-10-23

Run r28 completed the Rust UI acceptance test in Dolphin 2609a. The run used an
isolated Dolphin profile and copied NAND. Its captured DOL SHA-256 is
`a218f1a76d14fdbb010738188a382c36e6ad82f55ffe2aebd341ea4cb2b01442`; its WAD
SHA-256 is `fbe35d3b859ea877d822ae76dac9955da487c97f759c9ed828822af43f216322`.
The result, Dolphin log, and seven screenshots are in
`docs/review/evidence/ui-acceptance-dolphin-2026-10-23-r28/`.

The guest reached Bluetooth Ready. The harness measured the Pipe-to-guest
pointer response, inverted that calibration, and reached the requested screen
targets within four pixels. It recorded two real bubble pops, Date order
Cedar/Birch/Amber, and Name order Amber/Birch/Cedar. It also opened Network
Settings, started the UI network scan, observed the supported-driver error, and
verified that the receiver returned to Listening. Dolphin exited with code 0.
The C comparison run separately confirmed that Pipe mapping and guest input
work with HBC Reborn.

The harness now accounts for Dolphin's sensor-bar projection instead of
assuming Pipe axes equal guest screen pixels. It measures both axes and uses
the guest's reported pointer position as feedback. Pipe X and Y map to Wii
Remote buttons 1 and 2. The result records the measured matrix and all pointer
events; acceptance checks require the targets and successful UI state changes.

Verification after the run: all 54 Python tests pass, including the focused
Dolphin harness tests; `cargo test --features startup-diagnostics,debugger-network
--quiet` passes (408 tests); `python3 tools/parity.py check` passes (41 areas);
Python compilation and `git diff --check` pass. The current
`dist/hbc-ui-acceptance` DOL has a different hash from the captured r28 DOL, so
the current dist artifact is not claimed as the binary tested in r28. No
physical Wii test was performed; real hardware input and recovery checks remain
open.


## Candidate DOL Wiiload launch in Dolphin — 2026-10-23

The current `dist/hbc-ui-acceptance-2026-10-23` candidate was tested through
the channel-to-Wiiload route in Dolphin 2609a. The emulator booted
`hbc-rs-debug-OHBC.wad`, HBC became ready in 2.100 seconds, the sender
transferred `hbc-rs-debug.dol` in 0.105 seconds, and a new HBC session became
ready 1.834 seconds after transfer began. Parent startup, executable handoff,
child startup, and the child Wiiload listener all passed their log checks.
Dolphin exited with code 0. Evidence is in
`docs/review/evidence/ui-candidate-wiiload-dolphin-2026-10-23-r1/`.

The tested DOL SHA-256 is
`c20ed3d888c2fb2609fdc206afa8c40b484eb9cb5508ac295b54bab2f9f968a4`.
The matching WAD SHA-256 is
`03d9da9fb00dc9d37a323cc5777d3b877a753e1371dacfc77b3c3d9c9e36155f`.
Package verification confirms the WAD content hashes. The manifest identifies
HBC revision `5dd9fa89cc8fda6ebd2b5ea24d6edcf0f2f94cd9` and libogc revision
`e716e77aa5b5ada0f2b7bb9fc66c697de15cff5d`, which are older than the current
main revisions. This run proves the launch route for this exact package; it
does not prove the latest main build or the separate r28 UI acceptance DOL.

The remaining package limits are material. The WAD uses legacy fakesigning;
private-key signing, EC verification, and certificate-root trust are not
verified. It installs under the existing OHBC title ID. The package manifest
does not qualify automatic recovery. The prior focused Dolphin recovery test
failed to return after its 30-second deadline, and the recovery path remains
unqualified. Per the mandatory hardware procedure, do not upload this candidate
to a Wii until recovery is reworked and validated. No physical Wii test was
performed.


## Wii defect report and temporary-folder cleanup — 2026-10-23

The user reported slow-looking startup, a frozen network scan, a failed DOL
launch from SD, inconsistent UI layout, and errors from the installed WAD
channel. Four recent EasyCap screenshots show receiver cleanup failures during
exit, restart, and network scan: `Cleanup(-1)`. The user confirmed that the MP4
and text-only recovery-probe image are stale; neither is evidence for these
defects. The exact SD-launch error and installed WAD identity remain open.

Current source review confirms that some startup work precedes normal drawing
and that the Scan action runs stop/scan/restart work on the drawing thread.
The existing results picker cannot be reached when receiver shutdown fails.
The cleanup report does not identify the failed operation, and the socket
wrapper can reduce a backend error to `-1`. The actual Wii cleanup cause is
not yet confirmed. These observations are not fixes or new hardware passes.

The user authorized clearing `/Volumes/SSD/WiiDev/tmp`. The folder held about
1 GB and 79 top-level entries. No Git data or open files were found. The four
recent screenshots and one archival transfer summary were saved with hashes
under `docs/review/evidence/wii-channel-report-2026-10-23/`. The temporary folder
was removed and recreated empty. Existing sources, dist packages, and retained
test evidence were preserved. No new DOL/WAD was built or sent to hardware.

The defect list, source findings, fix order, and required checks are recorded
in `docs/review/wii-follow-up-2026-10-23.md`. Document whitespace and saved-file
hashes were checked. No production code was changed in this cleanup and review.
