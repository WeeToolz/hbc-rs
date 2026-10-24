# HBC launch error hidden by controller warning — 2026-10-24

## Report

The user launched HBC from Dolphin's SD folder. The hand pointer and click
worked. Selecting Tetris led directly to HBC's "Controller input failed"
warning. The Tetris board did not appear.

## Source finding

The main loop reports an executable staging or cleanup error in a notice.
Later in that same loop, a controller scan failure replaces that notice with
a generic warning. Each later failed scan replaces the notice again.
This can hide the original launch error. The screenshot does not identify
the failed operation or establish that executable control reached Tetris.

## Change

Keep an existing notice when a controller scan fails. Continue to log the real
scan error. When no notice exists, include the scan error in the visible
warning. No loader validation or cleanup requirement was removed.

## Build and SD update

The release PowerPC build and DOL conversion passed with `-D warnings` and no
compiler or Cargo warnings. No tests or Dolphin run were performed for this
change. The underlying launch failure remains open pending the original
error message or a run log.

Updated file:
`/Users/user/Library/Application Support/Dolphin/Load/WiiSD/apps/hbc/boot.dol`

SHA-256:
`9a0960d26c7a199e67bbce054f6597c3bb0bb10b38284f6477c9522532801602`

Build output, log, and the previous SD HBC DOL are in
`dist/launch-error-details-2026-10-24/`. The directory is ignored by Git.
The destination DOL was checked against the new build by SHA-256. Tetris and
the other SD applications were not changed.
