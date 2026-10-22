# Development results

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
