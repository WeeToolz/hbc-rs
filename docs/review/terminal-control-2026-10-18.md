# Terminal controller and front-button commands, 2026-10-18

## Issue and execution path

The running Rust HBC accepted `HBCI` controller input and `HAXX` uploads, but had
no direct system-command endpoint. The existing root reset command uploaded a
diagnostic DOL. It was not a direct terminal restart of the running HBC.

The receiver now accepts validated `HBCC` requests and `HBCQ` result queries.
It shares the existing listener and peer restriction. A bounded, nonblocking
mailbox transfers work to the normal HBC main thread. System exits use the same
settings, cleanup, and production reset path as HBC's local actions. The worker
does not execute reset, power, file, disc, or pairing operations itself.

Request IDs make submission idempotent while the result is retained. An active
job cannot be overwritten. Expired queued jobs never execute. Final backend
errors are retained for polling. Clients do not repeat a hardware operation
after an uncertain connection. Reset calls that unexpectedly return now produce
an error. Terminal exits attempt to recover controller/network service and show
the error after failed cleanup; recovery errors remain in the result.

Eject waits for a real disc-presence read to confirm absence. Disconnect waits
for actual controller disconnect events. Search waits for search completion,
a new connection count, and a connected controller in an available slot.
Screenshot and settings use the normal production writers. Network restart
waits for Listening state and a real IP address. GameCube and Classic analog
trigger input uses their local L/R bindings. Guitar fret/strum names and Xbox
button aliases use the existing masks.

## Supported errors and open capability work

`restart-app` and `return-hbc` return unsupported in the HBC browser. An unrelated
launched app must integrate its own command service. No app-side transport was
implemented or claimed by this change.

`stop-sync` returns unsupported before submission because the SDK does not expose
hardware completion of inquiry cancellation. Reading the SDK's desired search
state is not a hardware acknowledgement. Adding an acknowledged SDK cancellation
operation remains open work.

Power uses normal controlled shutdown. A forced electrical Power-button hold
is not emulated. The long SYNC hold that removes saved pairings is not exposed.
The input transport supplies HBC's used input fields, not a virtual controller
or arbitrary raw motion sensor data for a different application.

## Verification

- `cargo test --locked --lib`: 355 passed, 0 failed, 0 ignored.
- Terminal client tests: 12 passed. The controlled socket peers validate the
  real Python sender, fragmented responses, malformed/short replies, backend
  failure, uncertain delivery, no duplicate hardware submission, and read-only
  preflight failure. These tests are not hardware acceptance evidence.
- Final native Wii build completed without warnings:
  `target/hbc-rs-terminal-control.dol`, SHA-256
  `c97235583d2eba039fe67bdff9011956d0c76446832cfe37cdf9dc0d90e9afef`.
- Isolated Dolphin run of that final DOL passed. The production socket and main
  loop consumed HOME, release, analog, pointer, and stick packets, rejected an
  invalid version and malformed active-release packets for each controller
  family, returned the actual unsupported-app and no-storage screenshot errors,
  and retained results for duplicate request IDs. This does not verify menu
  selection effects, mechanical eject, pairing, power, or console restart.
  Profile: `/private/var/folders/lj/9_lkf2111h7g0310hnkh5m2w0000gn/T/hbc-remote-input-al7z_ruk`.
- Sender/test scripts compile, `.command` files pass `zsh -n`, and `git diff
  --check` passes. New command paths were inspected for discarded results,
  unconditional success, empty error handlers, ignored inputs, and hidden no-ops.
- HBC parity check passes for the 39 existing areas. SDK `audit-source` passes
  with the three existing keyboard notes. SDK `audit-stubs` stops at the existing
  stale `src/lib.rs:840 mp3player-sys::init` evidence; no completeness pass is
  claimed and no evidence rule was weakened.

## Real Wii attempt and limits

The user returned the Wii to the working C Reborn listener. The reference
Wiiload sender ran through the signed-in Terminal `.command` workflow. Upload
succeeded for the earlier command build, SHA-256
`7b8f8dc4bcb6b12d456e85ff2fa390e21a04544c0f22b447d894d997c9bd30fb`.
Its new command service answered a read-only query with session `599774`.
Evidence: `target/terminal-control-upload.log`, `terminal-control-upload.status`,
`terminal-control-ready.log`, and `terminal-control-ready.status`.

The direct restart launcher then timed out during its read-only session preflight.
The client sends the restart operation only after that read succeeds. Therefore
this attempt submitted no hardware restart command. A subsequent read-only status
request also timed out. This is not a successful restart and does not identify the
cause of the lost response. The Wii screen/input state was requested from the user.
The final build above has further completion checks and error recovery; that exact
final hash has passed Dolphin but has not been uploaded to the Wii yet.

Actual front-button operations and console restart remain hardware-pending.
The software cannot execute a queued command when its main loop is fully stalled.

The final read-only status retry through Terminal returned `Connection refused`
on `10.0.0.242:4299`, exit status 1. No operation was submitted. Its unique log is
`target/terminal-control-20261018-135627-19550.log`. The current listener state
must be restored before further real-Wii tests. Command launchers now preserve
one timestamped log and exit-status file per run, as well as the latest result.
