# Wii defects reported on 2026-10-23

Status: open investigation. These are user reports from a physical Wii, plus
review of the supplied screenshots and current source. No code fix or new Wii
test is claimed by this report.

The user confirmed that the attached MP4 and text-only recovery-probe image
are stale. Exclude both from this diagnosis. Only the four recent error-dialog
screenshots support the channel failure report below.

## Evidence

Four screenshots from `~/Movies/EasyCap Studio` are saved with their original
names and SHA-256 values in
[wii-channel-report-2026-10-23](evidence/wii-channel-report-2026-10-23/files.json).
They show these errors in the installed channel:

- Could not stop the network receiver (`Cleanup(-1)`). System exit stopped.
- Could not restart the network receiver: `Stop(Cleanup(-1))`.
- Could not stop Wiiload before scanning: `Cleanup(-1)`.

Two screenshots show the same system-exit error. The images show a running HBC
UI with errors during service shutdown. They do not identify the installed WAD
or establish that channel packaging is the cause.

An older transfer summary was preserved before clearing the temporary folder.
It is archival material only. It does not identify the installed WAD or establish
the current device address, installation result, or running HBC binary.

Two later screenshots identify the SD-load defect. Tetris details open and
the selected Load action returns
`Loader(DolHeaderOverlap { section: 0, offset: 48, size: 48 })`.
The user identified the file as `libogc-rs/apps/tetris/boot.dol` and confirmed
that C HBC Reborn loads that DOL successfully on the Wii. The local file is
5,153,056 bytes with SHA-256
`b5792032f947f4529bfcf934e75be82d328394f136b69e6db663b24cb5e9e1ba`.
Its first text section starts at byte 256 and has 649,176 bytes. The production
host parser and load-plan checks accept it. Its header and identity record are
saved alongside the screenshots. Treat the reported failure as a Rust SD-load
defect; do not weaken the header validation to accept the reported byte 48.

This error occurs while preparing the executable, before service cleanup or
IOS handoff. It is separate from the receiver `Cleanup(-1)` report.

## Fix order and acceptance requirements

| Priority | Defect | Current finding | Required verification |
| --- | --- | --- | --- |
| 1 | Wiiload cleanup blocks exit, reset, and network scanning | The four receiver error screens name `Cleanup(-1)`. Receiver shutdown stores the first cleanup integer without its failing stage. The socket wrapper can reduce a backend close error to `-1`. The exact failed cleanup operation is unknown. | Preserve the operation and backend error; test failure handling at each close/deinit boundary; verify receiver stop/restart and each affected user action. Never clear the error or report success to bypass the failed cleanup. |
| 2 | Startup looks frozen on Wii | `WiiStartup::initialize` performs settings/theme reads and controller initialization before the normal frame loop. Storage discovery already has a worker, so moving storage again does not resolve all startup pauses. | Present an early loading screen; keep animation and stage text changing while slow initialization runs; apply initialized results on the drawing thread; verify both success and failure. Measure physical startup as well as Dolphin startup. |
| 3 | Network scan freezes and does not reach a usable picker | The Scan action calls receiver stop, wireless scan, and receiver restart synchronously from `main`. `Screen::NetworkScan` exists and shows successful results, but a stop error prevents the scan and screen transition. | Show a scan-in-progress state before work starts; run the slow work outside the drawing thread; show a selectable network list, an empty result, or the real error. Verify Back, selection, retry, receiver restart, and pointer/controller operation. Do not claim immediate cancellation of an IOS request unless the backend supports it. |
| 4 | DOL launch from SD fails on the Wii | Tetris from `libogc-rs/apps` reports `DolHeaderOverlap { section: 0, offset: 48, size: 48 }`. The user confirms that C Reborn loads it on the Wii. The local DOL passes the production host parser. The failure precedes receiver cleanup. | Compare C and Rust loading with the same fixture. Trace the Rust SD read and launch buffer, and preserve the header seen by the parser. Add a test for the confirmed cause. Verify the exact candidate and route in Dolphin and on hardware under the required procedure. |
| 5 | UI design is inconsistent | User requests one design across all screens. The supplied images show long implementation errors in modal dialogs. The design review uses the `libogc-ui-design` skill and the existing HBC theme. | Share spacing, type sizes, button geometry, focus, Back behavior, notice layout, and loading states. Check 4:3 and widescreen, long text, visible focus, and Wii Remote/GameCube input. Inspect actual rendered frames. Keep useful technical details available without making them the main user message. |
| 6 | Installed WAD may be wrong or stale | The installed WAD identity is unknown. The existing `dist/hbc-ui-acceptance-2026-10-23` package is older than current main and differs from the r28 UI acceptance package. | Match the installed WAD to its SHA-256 and manifest. Build one named candidate from known source revisions; test its WAD boot and DOL launch routes. Keep DOL, WAD, ELF, manifest, and evidence together. A successful Dolphin WAD boot does not qualify physical installation or recovery. |

## UI ownership

Keep GX drawing and theme/UI state under one owner. Move blocking service work
to workers and pass progress and final results back to that owner. Adding a
second drawing thread would require sharing GX state and theme font caches; it
does not by itself remove the blocking work. Startup order must still preserve
the current requirement that synchronous NAND discovery precedes WPAD setup.

## Packaging and test limits

- The current candidate package contains no app-level `icon.png` or `meta.xml`.
  The earlier icon question concerns an app bundle as well as the embedded
  channel artwork. Resolve that bundle as part of candidate packaging.
- The existing UI acceptance run proves pointer, bubbles, sorting, and the
  network-driver error path in Dolphin. It does not prove a successful wireless
  scan, a physical network picker, or the reported Wii shutdown behavior.
- Automatic recovery remains unqualified. Follow `WiiDev/DEVELOPER.md` before
  physical capture, Wiiload, recovery, or diagnostic work. Do not use the older
  passing emulator launch as a recovery qualification.
- The earlier example sender command used `192.168.1.52`, which was the Mac's
  address in the emulator test. It must not be described as the Wii's address.
  Future physical sends must use the current address shown by HBC and the
  mandatory readiness and capture procedure.

## Temporary-folder cleanup

The user authorized clearing `/Volumes/SSD/WiiDev/tmp`. The folder contained
about 1 GB and 79 top-level entries. No `.git` data or open files were found.
The four screenshots and the recent transfer summary were preserved at the
evidence path above before deletion. The folder was removed and recreated
empty. Existing source, `dist` packages, and retained review evidence were not
part of this cleanup.
