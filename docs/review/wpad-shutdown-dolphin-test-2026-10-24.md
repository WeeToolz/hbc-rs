# Dolphin SD launch test after WPAD shutdown fix — 2026-10-24

## Result

The reported shutdown error no longer occurs in the tested Rust HBC run.
HBC completed controller cleanup, reloaded IOS58, and reached the selected
Tetris DOL entry. The debugger compared the entry bytes outside the argv
patch with the selected file. This check passed for Rust HBC and C HBC.

The complete visible-game acceptance check did not pass. Dolphin's screenshot
request timed out after Tetris entry in both loader runs. Direct Tetris boot
also failed to produce the requested screenshot in the private profile,
including a repeat with MMU enabled to match the normal user profile.
This is not evidence that the Tetris file is corrupt. The display/startup
result remains unresolved. No physical Wii run was performed.

## Artifacts and setup

Dolphin: 2609a, bundle version 2609.1, at
`/Applications/Dolphin.app/Contents/MacOS/Dolphin`.

| Artifact | SHA-256 |
| --- | --- |
| Rust HBC in Dolphin SD | `6b72a89ee420981e4f2ca0fa5f68af2728f61068ee9de3107af898c82643b699` |
| Tetris in Dolphin SD | `f7b51ee24af743117d932173749d37bc8f3ee3da52de92342ab69103542f3b21` |
| C HBC Reborn control | `a32c35c20e2a18ba3004aefbcea9ac10af555f0d068241e9332d6840b28fe5f6` |

Each run used a separate copy of the same user NAND, the same Tetris app
folder and metadata at `sd:/apps/tetris/boot.dol`, Pipe input, and the shared
capture configuration. The user's normal Dolphin data and SD DOLs were not
changed by these runs. Profiles were created and removed under WiiDev/tmp.
All run logs, configuration files, screenshots, and result JSON are retained
below `docs/review/evidence/`.

## Run history

| Evidence directory suffix | Result |
| --- | --- |
| `wpad-shutdown-rust-dolphin-2026-10-24-r1` | The original C sequence selected Back in Rust HBC. No launch pass. |
| `wpad-shutdown-rust-dolphin-2026-10-24-r2` | Correct Load selection; capture after 20 seconds showed loading at 52 percent. No launch pass. |
| `wpad-shutdown-c-dolphin-2026-10-24-r2` | Post-launch screenshot timeout. |
| `wpad-shutdown-rust-dolphin-2026-10-24-r3` | Controller cleanup and IOS reload completed. Tetris entry check passed. Game screenshot timed out. |
| `wpad-shutdown-c-dolphin-2026-10-24-r3` | Same Tetris entry check passed. Game screenshot timed out. |
| `wpad-shutdown-direct-dolphin-2026-10-24-r4` | Direct Tetris boot; initial screenshot timed out. |
| `wpad-shutdown-rust-dolphin-2026-10-24-r4` | Repeated entry pass. After screenshot timeout, sampled PC/stack and framebuffer. |
| `wpad-shutdown-direct-dolphin-2026-10-24-r5` | Direct boot with MMU enabled; initial screenshot timed out. |

## Runtime diagnosis

The Rust r3 log records executable validation, staging, Storage/Disc/NetworkReceiver/
Controllers cleanup, control transfer, and an IOS58 reload result of 58.
The debugger then stopped at Tetris entry `0x80004000`. The r4 run repeated
that entry check.

After the r4 screenshot timeout, the sampled PC and stack resolve to the SDK
idle thread and timeout processing. The framebuffer check reported 7,532
active pixels and five luma levels; the shared validation rejected it as blank
or nearly blank. This sample does not identify why the main thread was not
producing a usable game capture. It does not establish a game-start pass.

The matching Tetris ELF was converted with the existing xtask converter and
produced the exact SD DOL SHA-256. Its symbol table identifies framebuffer
pointer `tetris::wii::DRAW_FRAMEBUFFER` at `0x804ee9b4`. This address was used
for the read-only diagnostic. Matching Tetris and HBC ELFs are retained outside
`target` in `dist/wpad-shutdown-fix-2026-10-24/`.

## Script changes and checks

Extended the existing `tools/probe_hbc_reference_input_dolphin.py` runner with:

- Explicit Rust HBC Load focus selection.
- Optional debugger entry verification and loaded entry-byte comparison.
- Failure PC, LR, stack, and framebuffer inspection.
- An explicit MMU option for profile comparisons.

No production code was changed during this test task. Seven existing screenshot,
capture-configuration, and framebuffer validation tests passed. `git diff
--check` passed. Capture completion and a clean process exit were not treated
as a game-start pass.

Example decisive Rust command:

```sh
python3 tools/probe_hbc_reference_input_dolphin.py \
  --image '/Users/user/Library/Application Support/Dolphin/Load/WiiSD/apps/hbc/boot.dol' \
  --app-dir '/Users/user/Library/Application Support/Dolphin/Load/WiiSD/apps/tetris' \
  --load-focus-left --verify-app-entry --post-launch-seconds 20 \
  --output docs/review/evidence/wpad-shutdown-rust-dolphin-2026-10-24-r3
```

The C command used the identified C HBC image, the same app folder and entry
verification, and omitted `--load-focus-left` because C focuses Load by default.

Runner commit: `8a057d0`. Saved configuration copies have trailing
empty lines removed; settings and values are retained.

Readable log copies normalize trailing whitespace and CRLF line endings.
The exact original bytes are preserved in adjacent `.raw.log.gz` files.
