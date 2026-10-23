# Loader memory and service cleanup refactor — 2026-10-22

The loader owns the executable range, argument address, and runtime-clear
boundaries. Rust validation and the PowerPC trampoline use these definitions.
The startup executable size limit is derived from the same range. Its previous
one-byte margin is preserved. The public startup constant remains available
as a re-export. Compile-time checks enforce the 1024-byte argument reservation
and the alignment required by the assembly boundary loads.

Application launch, BootMii, and system exit call the same service cleanup
adapter in `src/main.rs`. The adapter calls the actual storage, disc, receiver,
and controller backends. Typed errors retain the backend failure, and callers
add the operation context. Controller failure diagnostics are retained.
BootMii errors now also include the backend failure details.

`src/platform/lifecycle.rs` owns cleanup order and stops at the first failure.
Application and BootMii order remains storage, disc, network, controllers.
System exit order remains disc, network, storage, controllers. The previous
`run_exit_cleanup` API remains available through a compatibility wrapper.
Settings-save policies, IOS policy, return-stub selection, and final transfer
behavior were not changed by this refactor.

## Verification

- Host library tests with `debugger-network`: **385 passed**, none failed or ignored.
- New tests check fixed Wii memory addresses, executable boundary acceptance
  and rejection, application cleanup order, and failure at each of the four
  service boundaries for each of the two orders. A failure must preserve its
  code and prevent all later cleanup operations.
- Release PowerPC DOL build with source debugging and `debugger-network`: passed.
- Compared the compiled trampoline against the previously tested ELF:
  **524 identical bytes**, SHA-256
  `5e941d5dfb59c7db9102136ced229a966d88dd92d2af3db0b0bc83291ce14481`.
- Formatting of all five changed Rust files and `git diff --check`: passed.
- HBC parity ledger and generated report check: passed, 39 areas.
- SDK source-structure audit: passed, with its three existing keyboard audit
  debt notes visible.
- SDK runtime completeness audit: failed with **2,371 existing findings**.
  See `audit-stubs.log`. SDK sources were not modified in this task.
- Clippy with warnings denied: failed with seven existing findings in
  `home_overlay_app.rs`, `management.rs`, `remote_control.rs`, `render.rs`,
  `runtime.rs`, and `wiiload_zip.rs`. See `clippy.log`. No finding points to a
  changed file. These checks were not suppressed or reported as passes.

## Dolphin regression

The normal debug WAD started and returned a valid read-only HBC command
response in **2.083 seconds**, session **1028**. Its matching DOL was sent
through Wiiload once. The new instance returned a valid response in
**2.070 seconds from sender start**, session **3956**. The host sender took
0.130 seconds. Each startup had a 180-second allowance.

The emulator log proves the real cleanup order, transfer of control, IOS58
reload, runtime shutdown completion, a second startup, and the new listener.
Both ready-stage screenshots were inspected and show HBC running. The
emulator stopped cleanly. All NAND writes were in a disposable copy.

See `dolphin/result.json`, `dolphin/dolphin.log`, `dolphin/upload.json`, and
the PNG images. The package is `dist/hbc-rs-dry-2026-10-22-r2/`.
The text log uses LF line endings. Its exact original bytes are retained in
`dolphin/dolphin-raw.log.gz`, with their hash in `dolphin/log-encoding.json`.

WAD SHA-256:
`0f5287e96ecaa34e2f578cf2b56031b6ec658e24a4986b4f6bbcd08918eb7737`.
DOL SHA-256:
`e7f6f3300aaec8d1e1c839efd4568c0b45ce5eb18bb8e69b502d8fff530f2f43`.
The WAD tool read back both contents and verified their SHA-1 hashes. It uses
legacy fakesigned metadata; retail trust is not established by this run.

No physical Wii test, SD/USB executable launch test, BootMii runtime test, or
return-stub recovery test was performed. The physical failure remains open.
This change removes duplicated rules and adds cleanup-stage evidence; it
does not establish a physical fix. Existing worktree changes were preserved
and excluded from these implementation commits. `worktree-status.txt` is a
snapshot taken after the refactor and before its commits.
