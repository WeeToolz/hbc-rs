# Application warning fixes — 2026-10-24

## Result

All 13 selected Wii DOL targets build in release mode with `-D warnings`.
The final compiler and Cargo logs contain no warnings. All seven action-trace
builds also pass with the same warning policy. Each of the 20 builds produced
a DOL and passed section bounds, address overflow, BSS overflow, and executable
entry-point checks.

| Build group | Targets | Result |
| --- | --- | --- |
| Normal games | Breakout, Capsule, Pitfall, Tetris | 4 passed; no warnings |
| Playable examples | autoplay_2048, Sudoku, Letter Flip, Snake GC | 4 passed; no warnings |
| HBC | hbc-rs | Passed; no warnings |
| Diagnostic/helper targets | letter-flip-hardware-probe, tetris_host_probe, tetris_mp3_probe, emulator-acceptance | 4 passed; no warnings |
| Action trace | Breakout, Capsule, Pitfall, Tetris, autoplay_2048, Sudoku, Snake GC | 7 passed; no warnings |

## Changes

- Trace-only fields and the axes lookup compile only for action tracing or host
  tests. Gameplay keeps the fields it uses. Tetris keeps its timing field in
  normal builds. Full trace records retain the same fields.
- Breakout and Sudoku imports follow their use. Sudoku's board-change helper
  compiles for tracing and tests.
- Letter Flip's game and hardware probe use one production library. The public
  audio entry points cross this library boundary. Storage, audio, and game
  behavior are unchanged; no backend is replaced.
- Tetris uses distinct Cargo entry files that include one shared game body.
  The shared body was checked against the previous source and matches exactly.
  The unused PCM stop wrapper was removed. Game shutdown code is unchanged.
- No lint allowances or warning-level reductions were added.

## Checks and limits

Source-structure, package-role, dependency-direction, and inventory checks pass.
The inventory remains current at 3,296 entries. The strict completeness audit
still reports the existing 2,366 unverified entries. No completeness requirement
was changed.

These are compile, link, conversion, and static checks. No Dolphin or physical
Wii run was performed. Host test suites were not run in this change. Optional
runtime combinations and GameCube variants were not included.

## Evidence

Normal builds, DOL files, hashes, source commit IDs, build script, and audit logs:
`/Volumes/SSD/WiiDev/Devtools/reports/evidence/2026-10-24-warning-free-app-dols`.

Action-trace builds, DOL files, hashes, and build script:
`/Volumes/SSD/WiiDev/Devtools/reports/evidence/2026-10-24-warning-free-trace-dols`.
