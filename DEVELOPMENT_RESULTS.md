# Development results

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
