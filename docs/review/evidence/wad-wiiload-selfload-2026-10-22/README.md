# Dolphin WAD startup and Wiiload self-load — 2026-10-22

Result: passed in one Dolphin run. This run used the normal `debugger-network`
package, not the recovery-validation package. Dolphin booted the WAD. HBC-RS
then received and launched the matching DOL through its real Wiiload receiver.

| Measurement | Result |
| --- | --- |
| Dolphin process launch to first valid HBC command response | 2.111 seconds |
| Initial HBC session | 1028 |
| Wiiload sender process duration, including compression and transfer | 0.129 seconds |
| Start of sender process to first valid response from a new HBC session | 2.102 seconds |
| Sender completion to first valid response from the new session | 1.973 seconds |
| New HBC session | 3990 |

Readiness used the read-only HBC command-status protocol on TCP 4299. A valid
response with a new session was required after upload. TCP acceptance or sender
completion alone was not treated as a successful DOL launch. The poll interval
was 0.25 seconds, with a one-second per-query timeout. Each startup had a
180-second limit. These are measurements from one run, not guaranteed limits.

The Dolphin log records a received 2,454,208-byte executable, staging in MEM2,
settings save, service shutdown, transfer of control, IOS58 reload, and a second
HBC startup and Wiiload listener. There was no second OHBC title launch through
ES after upload. The child ran as a direct DOL and used its documented fallback
settings path because ES could not resolve a channel title ID.

Artifacts:

- `result.json`: package identity, timings, sessions, screenshots, and process cleanup.
- `upload.json`: sender identity and actual host-written byte count.
- `upload.log`: sender output.
- `dolphin.log`: emulator and HBC runtime evidence.
- `checks.json`: independent checks of the saved launch log and session change.
- `dolphin-20261022-225146.png`: initial WAD HBC screen.
- `dolphin-20261022-225149.png`: HBC screen after the DOL started and answered.

Both images were inspected. They show HBC's browser background and network
indicator. The second image is the final capture: its filename shared the
second-level timestamp of the child-ready capture, so the final capture
replaced that image. The script now gives each image a unique filename.
Screenshot capture searches title subdirectories below `ScreenShots`; the
previous direct-directory search could miss saved Dolphin images.

Package: `dist/hbc-rs-debug-2026-10-22-rebuild/`. WAD SHA-256:
`157c9609a72c3d7535e0c6f6aaf5947d31cd94da7a59b9e7a2811aa0209a755e`.
DOL SHA-256:
`47904a3c3718641644bc5fe9002197a59ed7ae71c7b4c5400d0519670e999ca5`.
The script verified the package manifest hashes before launch.

The test ran from a Terminal `.command` wrapper and used a disposable copy of
the prepared NAND. The source NAND and physical Wii were not changed. The
emulator stopped cleanly. This proves this DOL's Wiiload launch in Dolphin;
physical Wii DOL launch and return-stub recovery remain unverified by this run.

Correction to earlier interpretation: title `000000010000003a` is IOS58. Its
presence in the older recovery logs does not prove a System Menu or Priiloader
return.
