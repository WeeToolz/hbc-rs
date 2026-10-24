# Combined Wii startup audit: games and examples

Original source: `libogc-rs copy.zip`. The archive hash and its source revision were not supplied with the original report. The original findings below are a static source comparison. The separate runtime evidence section records later Dolphin tests; it does not establish a root cause.

Reviewed on 2026-10-23 against library revision `cc56567d1e0c2bf1610c0af19264f6fea64d119f` and the separate Tetris repository at `5d7342ca713eef7944f5678bf7b0c01220a4ca51`. The build used a temporary copy of the current local source. Its library build manifest reports no source changes. The Tetris manifest reports only the untracked `tetris-audio-buffer-check.dol`; that file was not used as a build input.

Line numbers in the startup matrix refer to the reviewed source snapshot. Record a new revision and recheck them after source changes.

## Startup matrix

| Program | Source entry point | Explicit `system::init()` before video | `system::check_argv()` | Initial application startup order |
|---|---|---|---|---|
| Breakout | `games/breakout/src/breakout.rs:105` | No | No | Display/video → framebuffer → controllers |
| Capsule (normal, non-std) | `games/capsule/src/capsule.rs:252` | No | No | `run()` → display/video → framebuffer → controllers |
| Pitfall | `games/pitfall/src/pitfall.rs:58` | Yes | No | System → display/video → power callback → PCM → controller manager |
| Tetris | `games/tetris/src/tetris.rs:521` | Yes | Yes | Capture arguments → system → check arguments → video → power callback → controllers |
| Autoplay 2048 | `examples/autoplay_2048/autoplay_2048.rs:392` | No | No | Display/video → framebuffer → controllers → audio → memory card |
| Sudoku | `examples/sudoku/sudoku.rs:113` | No | No | Display/video → framebuffer → VSYNC → controllers |
| Snake GC | `examples/snake-gc/snake-gc.rs:109` | Yes | No | System → video → audio → controllers |
| Letter Flip | `examples/letter-flip/letter-flip.rs:56` | Yes | No | System → video → controllers → storage → loop |
| Letter Flip hardware probe (separate diagnostic binary) | `examples/letter-flip/hardware_probe.rs:30` | Yes | No | System → probe display → controllers |

The four normal `games/` programs and four normal example games split evenly: four call `system::init()` before video and four do not. The hardware probe is a fifth **example executable**, not a fifth normal example game.

**Important:** Absence of an explicit call in application `main` does not alone establish that system initialization is absent everywhere; confirm the runtime and any library initialization paths when testing.

## Shared loader and runtime startup

1. `crates/abi/ogc-sys/src/lib.rs`: assembly `_start` includes an HBC-compatible `_arg` structure and reserves its data.
2. `_start` sets processor/stack state and branches to `_start_rust`.
3. `_start_rust` runs `shelter_startup_argv` before clearing BSS so loader-provided arguments are not overwritten.
4. Runtime clears BSS and installs the IRQ exception vector; `wii-std` builds take an additional standard-runtime initialization path.
5. Control enters the program entry point, which follows one of the differing initialization sequences above.
6. `system::check_argv()` delegates to `_check_argv`; when argument setup has already completed, it exits without repeating the work. Thus Tetris's explicit call is not evidence that other programs lack HBC argument preservation.

Do **not** compare Capsule's optional `capsule-wii-std` binary as though it had the same startup contract as the normal non-std Capsule build.

## Findings, ranked for the Tetris failure

1. **High priority — locate the first difference between direct launch and HBC launch.** Use the same DOL bytes in both paths. Record the entry state, argument preservation, system initialization, video setup, controller startup, first menu draw, and audio startup. A screenshot timeout alone does not identify which stage failed.
2. **High priority — verify system and video results.** Tetris already calls `system::init()`. Its `init_display()` checks the selected mode and both framebuffer allocations, and `main` checks the display result. Record the actual arena bounds, initialization status, video mode, framebuffer pointers, and first displayed buffer. The existing checks do not prove that initialization completed correctly.
3. **High priority — trace controller and first-frame work.** Record entry and completion for controller setup, the first controller scan, the first menu draw and buffer swap, and the first audio initialization. Record errors at each boundary. A spinner alone does not prove that input, menu drawing, or audio startup completed.
4. **Lower priority — explicit argument recheck.** Tetris's `check_argv()` returns without repeating the work when the runtime has already sheltered and parsed valid arguments. Do not remove it as a proposed fix without evidence of a failure in that path.

These priorities are test hypotheses. They are not measured failure rates or a confirmed diagnosis.

## Wider game audit

- Breakout, Capsule (normal), Autoplay 2048, and Sudoku enter display setup without an explicit application call to `system::init()`. Pitfall, Tetris, Snake GC, and Letter Flip call it first. Snake GC includes a comment about arena initialization before framebuffer allocation. Trace the complete library call path and actual allocation results before concluding that initialization is missing.
- Sudoku does not check the `init_display()` return for null at its call site; Autoplay 2048 does check it. Check the complete error path before any pointer dereference.
- Keep std and non-std binaries and their feature flags distinct, especially Capsule.
- Controller, audio, IPC, IOS, storage, and Bluetooth startup orders differ. Compare these after system and video startup have been measured.

The missing-explicit-init finding is relevant to the wider audit. It does not explain Tetris's failure by itself because Tetris explicitly calls `system::init()`.

## Unified direct-launch / HBC trace

For each exact DOL build, record the game and library revisions, source changes, DOL SHA-256, build manifest, default and additional feature flags, compiler, and build command. Also record the launcher binary SHA-256, Dolphin version and configuration, NAND and SD fixture identity, input configuration, IOS, and video mode. Use separate private profiles made from the same starting data. Then check these stages in both paths:

| Stage | Checkpoint | Record |
|---|---|---|
| 00 | Assembly `_start` | Earliest possible trace and initial stack |
| 01 | Loader `_arg` before modification | Raw descriptor bytes, magic, command-line pointer and length, argc/argv pointers; record whether the loader populated each field |
| 02 | After `shelter_startup_argv` | Argument-copy status and resulting pointers |
| 03 | After BSS clear / IRQ installation | BSS and stack bounds, sheltered argument bounds, arena reservation, installed vectors |
| 04 | Enter application `main` | Arguments, arena bounds, startup state |
| 05 | Before and after `system::init()` when used | Arena bounds, `snapshot_startup()` completion and failure stage, changed state |
| 06 | Video mode and framebuffer setup | `video::init`, selected mode, framebuffer pointers, allocation failures |
| 07 | First VSYNC / visible frame | Reached/not reached and display output |
| 08 | Controllers and other subsystems | Begin/end/error for controller setup, first input scan, IPC, Bluetooth, first menu draw and buffer swap, audio, and memory card when used |
| 09 | First game update/input | First playable frame and controller sample |

Record the HBC reload stub separately from `_arg`. The current runtime transfers to `0x80001800` when the reload route is selected. `ogc_sys::hbc_reload_stub_present()` reads the two signature words at `0x80001804` and `0x80001808` and expects `0x53545542` and `0x48415858` (`STUBHAXX`). Record the bytes and validation result without executing the stub. These are checks of the current runtime contract; they do not prove that return to HBC works.

Avoid relying on IPC/USB/filesystem logging before those services initialize. Find the **first missing or different checkpoint**, rather than inferring failure from a black screen alone.

## Suggested diagnostic sequence

1. Start with Tetris. Preserve the failing DOL and its hash. Test the same exact DOL directly, through the known C HBC reference, and through Rust HBC. Record each result separately. A direct-launch pass does not establish HBC compatibility.
2. Trace the checkpoints above in the direct and C HBC paths. Use debugger memory reads or another validated early trace that does not depend on services that are still starting. Record normal differences, such as absent direct-launch arguments, separately from failed checks.
3. Find the first failed or incomplete operation. If both paths reach `main`, trace system setup, display setup, controller setup, the first scan, menu drawing, and audio startup. If the difference appears before `main`, inspect `_start`, argument shielding, BSS clearing, stack bounds, exception vectors, and the handoff state.
4. Make the smallest change supported by the trace. Repeat the affected same-DOL comparisons and verify visible menu output and controller input. Verify return to HBC separately if the change affects the return path.
5. After the Tetris problem is resolved, extend the audit to Sudoku and Snake GC, then the other normal games. Use the Letter Flip hardware probe only for its stated diagnostic purpose. Follow the hardware procedure before any physical Wii test.

## Runtime evidence added on 2026-10-23

The current-library builds used game revision `5d7342ca713eef7944f5678bf7b0c01220a4ca51` and library revision `cc56567d1e0c2bf1610c0af19264f6fea64d119f`.

| DOL | SHA-256 | Direct Dolphin launch | C HBC SD launch |
|---|---|---|---|
| Current library, default features plus `agent-report` | `eeed701568bd2ff36d086a02117bd161decf1f61940cf419ffc010db89e05701` | Capture runner reached menu and gameplay; zero unknown GX opcodes; PNG review remains required | Browser and app details captured; post-load screenshot request timed out; app startup result is unconfirmed |
| Current library, default features | `dce4ddca56f5be1226192be82f775b6e8f4b2294a155bf9ab89d34145c0779f3` | Not tested in this build pair | Browser and app details captured; post-load screenshot request timed out; app startup result is unconfirmed |

The two builds have different feature sets and hashes. Do not combine their results as though they were the same DOL. Default features include `wii-controllers`; an empty manifest list of additional features does not disable defaults.

The C HBC reference hash was `a32c35c20e2a18ba3004aefbcea9ac10af555f0d068241e9332d6840b28fe5f6`. The observed Dolphin window identified version 2609a with JITARM64 SC, Metal, and HLE. Machine-readable Dolphin executable identity and exact version output remain to be recorded. Both C HBC tests used private profiles, an SD app fixture, and Dolphin Pipe input. Their copied configuration is in each run directory. Starting NAND and SD fixture hashes were not recorded, so these runs do not provide a complete fixture identity.

The C HBC command for the reporting build was:

```sh
python3 tools/probe_hbc_reference_input_dolphin.py \
  --image /Volumes/SSD/WiiDev/C-sources/hbc-reborn/channel/channelapp/channelapp-channel.dol \
  --app-dir /private/tmp/tetris-current-app \
  --output /private/tmp/tetris-current-libogc-hbc-test-r2 \
  --post-launch-seconds 15
```

The normal release build used:

```sh
cargo xtask build-dol tetris --bin tetris \
  --manifest-path games/tetris/Cargo.toml \
  --output /private/tmp/tetris-current-libogc-rs-default.dol
```

The reporting build added `--features agent-report` and used output `/private/tmp/tetris-current-libogc-rs.dol`.

Original run locations (retained in the copied JSON records):

- Reporting DOL manifest: `/private/tmp/tetris-current-libogc-rs.manifest.json`.
- Reporting DOL direct capture: `/private/tmp/libogc-rs-current-source/target/dolphin-example-tetris-current-sdk-direct/result.json` and adjacent PNG files.
- Reporting DOL C HBC capture: `/private/tmp/tetris-current-libogc-hbc-test-r2/result.json`, logs, and copied configuration.
- Default DOL manifest: `/private/tmp/tetris-current-libogc-rs-default.manifest.json`.
- Default DOL C HBC capture: `/private/tmp/tetris-current-default-hbc-test/result.json`, logs, and copied configuration.

The manifests, result records, logs, captured PNG files, and C HBC configuration are preserved in `docs/review/evidence/tetris-current-sdk-2026-10-23/`. Its `artifact-index.json` records the size and SHA-256 of each copied artifact. Original absolute paths inside those records still refer to the temporary run locations. DOL binaries, raw XFB captures, and NAND data are not included in that evidence copy. A screenshot timeout does not prove an application crash or locate a failed startup stage. A cleanup exit code of zero does not establish successful app startup.

## Validation and limitations

- The original archive review was static. The archive SHA-256 and exact source revision remain unknown.
- Current-source checks support the startup matrix and the argument-shielding description. They do not prove physical hardware behavior.
- Later Dolphin tests are recorded separately above. The current-library rebuild has not established successful HBC launch.
- No new physical Wii test was performed. No root cause or production fix is confirmed.


## Confirmed Tetris startup failure and fix (2026-10-23)

The same release DOL was traced through direct launch and both HBC launchers. In the failing HBC run, Tetris completed `system::init`, argument handling, display setup, and controller initialization. It entered `wait_for_console_reset_release()` before drawing its first menu frame. The loop treated an already asserted RESET signal at startup as a new RESET press, then waited for the signal to be released. No menu frame was drawn. This caused the black screen and the moving activity indicator. The trace does not identify which loader or emulator action asserted RESET, so this report does not assign that cause to libogc or to either HBC implementation.

The Tetris fix accepts RESET presses only after the application has sampled the signal released once. A signal held during startup is ignored; after a release is observed, normal new RESET presses still work. The change and regression tests are in the separate nested Tetris repository. Its host suite passed all 15 tests: four direction tests, eight pause tests, and three RESET tests. The exact fixed DOL was built in release mode from current libogc-rs revision `cc56567d1e0c2bf1610c0af19264f6fea64d119f`, with default features and no development features. The game source base revision was `5d7342ca713eef7944f5678bf7b0c01220a4ca51`; the only source changes used were the tracked RESET fix and its two new source/test files. The pre-existing untracked `tetris-audio-buffer-check.dol` was not used.

The fixed DOL SHA-256 is `b3362c7d9d79b5ff348cc13001178b56861d883efa6aa09cd2c3f0d0594404e0` (5,155,232 bytes). It reached the menu and active gameplay when launched directly, through the C HBC reference, and through the installed Rust HBC. Both HBC runs read `sd:/apps/tetris/boot.dol`; debugger checks confirmed that the child code matched the tested DOL. Screenshots from the Rust HBC run were reviewed and show the Tetris menu and active board after controller input. Evidence, screenshots, logs, build manifest, DOL, prior fake-SD DOL backup, and source patch are in `evidence/tetris-reset-fixed-rust-hbc-2026-10-23-r4/`. The installed fake-SD file is `/Users/user/Library/Application Support/Dolphin/Load/WiiSD/apps/tetris/boot.dol`; its hash matches the fixed DOL. Its previous DOL is preserved as `boot-before-fix.dol` in that evidence directory.

The direct, C HBC, and Rust HBC runs used Dolphin, not a physical Wii. They show that the tested DOL works in those emulator launch paths; they do not establish physical Wii behavior. The startup matrix and wider game audit above remain separate work and are not resolved by this Tetris fix.

## Updated validation status

The earlier runtime table records pre-fix or incomplete screenshot runs. Do not treat its screenshot timeouts as a failed application launch, or its incomplete traces as the final result. The later same-DOL trace above confirms the startup cause and successful fixed-DOL launch through both HBC implementations and direct Dolphin. No physical Wii test was run.
