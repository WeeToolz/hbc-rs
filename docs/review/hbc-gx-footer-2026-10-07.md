# HBC GX footer command fix

Date: 2026-10-07
Ledger entry: `HBC-GX-01`
Production fix: `49782bd`
Failing source revision: `aafde21`

## Report and cause

Dolphin reported invalid GX FIFO opcodes after the no-storage notice was dismissed. The C application continued to show the empty browser.

The Rust renderer configured solid drawing when it reached `FrameCommand::Fill(0)`, even though that background fill was already drawn and skipped. The later `FrameCommand::StatusIndicator` branch submitted the Gecko and network textures without restoring textured drawing. The solid descriptor consumed 16 bytes per vertex, while `draw_textured_rect` submitted 24 bytes per vertex. The texture-coordinate bytes were then read as further commands.

C `source/gfx.c:148–156` configures direct position, color, and texture coordinates. Both `plot_vert_c` (`:310`) and `plot_vert` (`:354`) supply those attributes. C therefore keeps the submitted vertex bytes consistent with its descriptor.

The address after `@` in Dolphin's error is a host buffer pointer, not a Wii physical address. Dolphin uses `fmt::ptr(buffer)` in [its error handler](https://github.com/dolphin-emu/dolphin/blob/master/Source/Core/VideoCommon/CommandProcessor.cpp). The approximate CPU PC and LR can also lag the command that failed. Neither address identifies the root cause on its own.

## Production change

`VideoSystem::draw_solid_rect` and `VideoSystem::draw_texture` now use one `DrawState` helper to configure GX before a format transition. The first draw also configures its format. Explicit configuration remains available after an external GX state change. Repeated draws with the same format reuse the active configuration.

The renderer no longer keeps a separate `textured_state` flag. Footer, image, glyph, and fallback-fill paths all use the same production draw methods. Draw errors retain their original `VideoError` values.

Host regressions check descriptor selection before vertex submission, consecutive footer textures, transitions back to solid fallback glyphs, draw-error propagation, and explicit configuration refresh. The host sink is explicit test evidence for shared state logic; it does not replace a hardware probe.

## Dolphin evidence

The run used an isolated Dolphin 2603-415 profile, an emulated GameCube controller through Dolphin's named-pipe input backend, the production DOL, and direct XFB reads through Dolphin's GDB endpoint. Controller input passed through HBC's normal input and application paths. The test did not patch application state or replace drawing code.

- The failing revision produced **2,360** invalid-opcode reports after A dismissed the notice. See [the baseline result](evidence/gx-footer-2026-10-07/baseline-result.json), [run record](evidence/gx-footer-2026-10-07/baseline-run.log), and [first error messages](evidence/gx-footer-2026-10-07/baseline-first-errors.log).
- The fixed DOL produced **zero** invalid-opcode reports during an 86.86-second run. A dismissed the notice, Start opened the Home menu, B returned to the browser, and the browser remained visible for a further 60 seconds. See [the fixed result](evidence/gx-footer-2026-10-07/fixed-result.json) and [run record](evidence/gx-footer-2026-10-07/fixed-run.log).
- The captured [notice](evidence/gx-footer-2026-10-07/notice.png), [browser](evidence/gx-footer-2026-10-07/browser.png), [Home menu](evidence/gx-footer-2026-10-07/menu.png), [browser return](evidence/gx-footer-2026-10-07/browser-return.png), and [idle browser](evidence/gx-footer-2026-10-07/browser-idle.png) were visually reviewed. Text was readable, the footer icons were present, and the no-storage notice stayed dismissed.
- The final capture utility was checked in a separate 26.83-second run with zero FIFO faults. Its [result](evidence/gx-footer-2026-10-07/tool-result.json) and [run record](evidence/gx-footer-2026-10-07/tool-run.log) are retained.

The tested DOL SHA-256 is `73e3a3a62cf6d8d375c6d63188e3e5eae96fe433e52f54e0c824c5efa8213a33`.

## Repeat the capture

Build the production Wii ELF and convert it to `target/hbc-rs.dol`. Then run:

```sh
python3 tools/verify-gx-footer.py --label fixed
```

The utility uses the installed macOS Dolphin executable and Homebrew LLVM by default. `--dol`, `--elf`, and `--dolphin` select other files. The DOL and ELF must come from the same build. The capture uses the default 640×480 mode and writes PNG screens, raw XFB data, the Dolphin log, and a JSON result into `target/gx-footer-fixed/`. Review the screens to verify the requested transitions; a zero error count alone does not prove visual parity.

For a previously built failing revision, use its matching ELF and DOL with `--expect-fifo-errors`. That option explicitly requires a failing baseline and records the expectation. It does not count that run as a passing implementation. Dolphin's popup is disabled in the isolated profile so an error cannot block capture; every invalid opcode remains logged and causes a normal fixed-run check to fail.

## Checks and remaining work

- `cargo test --locked`: **215 passed**, 0 failed, 0 ignored.
- `cargo fmt --check` and `git diff --check`: passed.
- Strict Wii release build and `xtask elf-to-dol`: passed. No lint override was added.
- `python3 tools/parity.py check`: passed after report refresh.
- `libogc-rs` `xtask audit-source`: passed with its existing tracked keyboard audit debt.
- `libogc-rs` `xtask audit-stubs`: source structure passed and the 2,854-entry implementation inventory was current, but runtime completeness failed with **2,238 disallowed API implementations/evidence findings**. No `libogc-rs` source, evidence requirements, or audit rules were changed by this fix.

Real Wii verification remains pending. This run covers the no-storage browser and menu transitions in Dolphin; it does not establish parity for every scene, video mode, controller, or hardware operation.
