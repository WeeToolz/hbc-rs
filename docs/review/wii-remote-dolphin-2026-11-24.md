# Wii Remote Dolphin comparison, 2026-11-24

The current HBC Rust build did not pass the emulated Wii Remote UI test.
The C control showed pointer movement, Options, app details, and grid view in
the six captured images. The C run booted a DOL; the Rust run booted a WAD.
Both used the shared Dolphin Pipe remote setup and a private NAND copy. The
fixtures and boot methods are not identical, so this comparison does not prove
the exact cause of the Rust failure.

The Rust test reached the Wiiload-ready state in 3.108 seconds. The prior
October 23 Rust run recorded 2.834 seconds; these are individual measurements
from different builds and runs, not a controlled performance comparison. In
the current run, Bluetooth reported one connection and one disconnect. Dolphin
logged an HCI disconnect and an unknown L2CAP channel. No pointer-ready event
arrived before the timeout. The script exited with status 1 and saved its
result and log. It did not reach the menu interaction checks.

The C DOL SHA-256 was
`a32c35c20e2a18ba3004aefbcea9ac10af555f0d068241e9332d6840b28fe5f6`.
The Rust DOL SHA-256 was
`a86f83c5afb83b56c47306867744f8b08884beec7058e8006e41502aa85c859c`.
The Rust WAD SHA-256 was
`f2e1cbf22d084e219e002acd2c2116b7a2de6a332b187aad5a8738524f98664f`.
The Rust build used HBC `36c435fe8a7d32671c8d68e4adb80b1029b12786` and
libogc-rs `aa54a9afd2d465af6ab65b74d3bf2644fca798f8`, with the
`debugger-network` feature. Neither the new format change nor a physical Wii
fix is established by this run.

- [C control result](evidence/wpad-format-c-control-dolphin-2026-11-24-r2/result.json)
  and its six images: reviewed; the script's unmodified result still says that
  screenshot review is required.
- [Rust result](evidence/wpad-format-rust-ui-dolphin-2026-11-24/result.json)
  and [raw log](evidence/wpad-format-rust-ui-dolphin-2026-11-24/dolphin.log).
- [Previous passing Rust result](evidence/ui-acceptance-dolphin-2026-10-23-r28/result.json).

Next, identify why the connection closed. Compare the current and earlier
working Bluetooth paths with controlled builds, then repeat the same Dolphin
input test. Physical Wii input and startup timing remain open.

## Disconnect trace

A controlled run with HBC `36c435f` and the library before the data-format
change (`b25827a6`) also disconnected the emulated Remote. A temporary
instrumented copy recorded the caller of the Bluetooth disconnect request:
the guest-search branch in `hid_host.rs`. The controller manager started that
search automatically after the Remote connected. Guest search disconnects
existing Wii Remotes before inquiry. The trace source changes are saved with
the [caller trace](evidence/wpad-caller-trace-rust-ui-dolphin-2026-11-24/result.json).

The manager now starts only general HID discovery automatically. An explicit
guest search is still available. In the [follow-up run](evidence/wpad-no-auto-guest-rust-ui-dolphin-2026-11-24/result.json),
the emulated Remote stayed connected, reached pointer readiness, moved the
pointer, and produced a pointer press. The full UI test failed later: it did
not observe the expected Options target for a click at the requested position.
Its screenshot shows the Options screen. Investigate that failure separately;
do not mark the full UI test or the physical Wii test as passed. The follow-up
WAD reached the network-ready check in 2.832 seconds in this single Dolphin run.
