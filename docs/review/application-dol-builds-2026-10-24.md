# Application DOL build results — 2026-10-24

All 13 selected Wii targets compiled and linked in release mode. Each ELF was
converted to a DOL. File section bounds, address overflow, BSS overflow, and
the entry point in a text section passed validation for each DOL.

| Target | Result | Warning messages in build log |
| --- | --- | ---: |
| `breakout` | PASS | 3 |
| `capsule` | PASS | 2 |
| `pitfall` | PASS | 2 |
| `tetris` | PASS | 5 |
| `autoplay_2048` | PASS | 2 |
| `sudoku` | PASS | 4 |
| `letter-flip` | PASS | 0 |
| `snake-gc` | PASS | 2 |
| `hbc-rs` | PASS | 0 |
| `letter-flip-hardware-probe` | PASS | 41 |
| `tetris_host_probe` | PASS | 5 |
| `tetris_mp3_probe` | PASS | 2 |
| `emulator-acceptance` | PASS | 0 |

Builds use the current PowerPC target and linker script, nightly Rust,
`build-std=core,alloc`, default features, and each diagnostic target's required
feature. The output directory was changed because the SDK root does not permit
creation of its default target directory. The existing xtask converter produced
the DOL files.

This check covers four current games, four playable examples, HBC, and four
additional diagnostic/helper targets. Historical migration copies, host test
crates, GameCube variants, and optional runtime/feature combinations were not
built. This check does not establish Dolphin or physical Wii operation.
Warnings remain in game and example builds. These results do not establish
warning-free application builds.

DOL files, commands, SHA-256 hashes, source commit IDs, and full logs:
`/Volumes/SSD/WiiDev/Devtools/reports/evidence/2026-10-24-all-app-dols`.

The existing Dolphin records in HBC report that Dolphin exited before the
debugger started. They do not establish an HBC-to-Tetris launch result.

Capsule was rebuilt after review of the input changes in commit `698a292d4b31cd7be65cc6dfe5e8453be4b97633`. The release link, DOL conversion, and header checks passed. The rebuild log is in `/Volumes/SSD/WiiDev/Devtools/reports/evidence/2026-10-24-capsule-input-dol`.
