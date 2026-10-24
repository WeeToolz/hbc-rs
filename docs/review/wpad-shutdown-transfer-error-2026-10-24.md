# WPAD shutdown transfer error — 2026-10-24

## Report

The user selected Tetris through HBC in Dolphin. The hand pointer and click
worked. Tetris did not appear. After the notice-preservation fix, HBC showed:
"Could not finish Wii Remote shutdown (shutdown poll failed (-3)).
Application launch stopped."

`-3` is `WPAD_ERR_TRANSFER`. HBC stopped before executable control transfer.

## Source cause

The SDK WPAD shutdown function starts adapter shutdown, publishes generic HID
disconnections, clears HID snapshots, and clears controller slots. The host
event queue can still contain input accepted before shutdown.

`scan_pads` processes those queued events during close polling. After the Wii
Remote slot is cleared, a queued Remote report no longer selects the Wii
Remote parser. It enters the generic HID branch, which requires a live input
generation. That generation does not exist, so the branch returns `-3`.
A queued connection can also recreate a slot after shutdown cleared it.

## Fix

In SDK `wpad-sys/src/powerpc_backend.rs`, do not deliver queued connection or
report events while the real host state is `Closing`. These events cannot
produce input after shutdown has published disconnection. The consumer still
removes them from the queue. Disconnect, failure, and key-storage events retain
their existing handling. Normal operation retains its validation and event
delivery. HBC still polls for adapter shutdown completion and retains its
shutdown error and timeout checks.

## Checks and evidence

- HBC release PowerPC build passed with `-D warnings`; no compiler or Cargo
  warnings were emitted.
- DOL conversion and header checks passed.
- SDK source-structure audit passed. The implementation inventory was refreshed
  to account for shifted source lines; it contains the same 3,296 entries.
- No automated tests or new Dolphin run were performed. The source fix still
  needs confirmation of HBC-to-Tetris launch in Dolphin. Physical Wii behavior
  remains unverified.

The user screenshot, build command, build log, and result are in
`docs/review/evidence/wpad-shutdown-fix-2026-10-24/`.

The new DOL is `dist/wpad-shutdown-fix-2026-10-24/boot.dol`.
SHA-256:
`6b72a89ee420981e4f2ca0fa5f68af2728f61068ee9de3107af898c82643b699`.

SDK commit: `4271745`. The rebuilt DOL was copied to Dolphin's
`Load/WiiSD/apps/hbc/boot.dol` and checked by SHA-256. The previous SD HBC
was saved as `dist/wpad-shutdown-fix-2026-10-24/previous-sd-hbc.dol`.
