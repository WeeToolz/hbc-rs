# HBC C source review

Review date: 2026-10-07
C source: `C-sources/hbc/channel/channelapp/source`
Rust source: this `hbc-rs` crate

This review compares each in-scope ledger reference with its Rust counterpart. `config/parity-ledger.csv` remains the status source; this file records review evidence and findings. `source_reviewed` means the comparison is recorded. It does not mean hardware behavior is verified or that partial behavior is complete.

## Findings

- **HBC-DATA-02 — playtime record reset:** C calls `playtime_destroy()` before settings load. `playtime.c` opens `/title/00000001/00000002/data/play_rec.dat` read/write and writes four zero bytes. Rust has no matching operation. This is a real parity gap. It remains open because it writes persistent system NAND data; review the file contract and safe failure behavior before implementing it. Do not count this path as implemented.
- **HBC-INPUT-01 — extension actions:** C maps Nunchuk Z and Classic ZR to the network-init action; the C main loop starts its TCP receiver on that action. The Rust receiver starts during startup, so the action is not required for the receiver's normal availability. C screenshot handling is behind `ENABLE_SCREENSHOTS`; the Rust port has no equivalent screenshot action. This is not a current required behavior unless screenshot support is enabled in the C build configuration.
- **HBC-THEME-01 / HBC-ZIP-02:** the source requires a root `theme.xml` and accepts only recognized root theme assets. Rust validation now enforces those rules. Existing bounds and supported compression checks remain explicit in Rust.
- **HBC-CATALOG-01 — catalog scan errors:** the review found that Rust previously discarded storage errors while checking executable, metadata, and icon files, and silently stopped at the raw directory-entry limit. Rust now returns file-operation errors with their paths and reports an oversized directory instead of presenting a partial scan as complete. Missing or malformed optional metadata and invalid icons still follow C HBC behavior and do not hide an otherwise valid app. The catalog limit remains a documented bounded behavior.
- **HBC-THEME-02 / HBC-THEME-03 — widescreen background:** C loads only `background_wide.png` for a theme in widescreen mode, then falls back to the built-in wide image. Rust previously used a theme `background.png` when the wide image was absent. Rust now selects the wide theme asset only and falls back to the built-in wide image, with a host regression test for the production asset-selection helper. Other texture dimensions and hardware output remain under review.
- **HBC-SETTINGS-01 / HBC-SETTINGS-02:** C settings I/O uses direct ISFS reads and delete-then-create writes. Rust validates XML, checks exact read/write results, and uses staged replacement and recovery. These are deliberate safety improvements; they preserve the settings contract while avoiding deletion of the last valid copy on a failed write.
- **HBC-DATA-01:** C obtains the running title's ES data directory. Rust obtains the same directory, validates the returned buffer, caches it, and builds bounded file paths.
- **HBC-BOOT-01 / HBC-BOOT-02 / HBC-SYSTEM-01:** startup order, the frame loop, and exit paths were compared. Rust owns failures and cleanup explicitly. Remaining screen and event differences are recorded as partial coverage; system calls still require Wii verification.
- **HBC-STORAGE-01 / HBC-STORAGE-02 / HBC-STORAGE-03 / HBC-CATALOG-01 / HBC-DELETE-01:** device startup, scan/refresh, SD Gecko call order, app metadata scan, and deletion were compared. The port covers the tracked paths; asynchronous scheduling and adapter, FAT rename, and hot-removal behavior remain subject to the ledger's follow-up or hardware checks.
- **HBC-INPUT-02 / HBC-VIDEO-01 / HBC-GX-01 / HBC-THEME-02 / HBC-THEME-03 / HBC-UI-01 / HBC-UI-02 / HBC-ANIMATION-01 / HBC-LOCALE-01:** C coordinate, display, scene, theme, widget, dialog, animation, and language behavior was compared with the corresponding Rust paths. Partial visual, interaction, font/catalog, and hardware gaps remain recorded in the port status and ledger. These rows are source-reviewed, not declared fully equivalent.
- **HBC-BOOTMII-01 / HBC-BOOTMII-02:** title marker detection and the IOS 254 action path were compared. Detection is implemented; launch and return behavior remains partial and requires Wii verification.
- **HBC-WIILOAD-01 / HBC-WIILOAD-02 / HBC-ZIP-01 / HBC-INSTALL-01 / HBC-INSTALL-02:** C receive, header, archive classification, extraction, and replacement paths were compared. Rust validates inputs and reports operation and cleanup failures. Socket/USB transport, FAT rename and recovery behavior remain subject to Wii verification.
- **HBC-LOAD-01 / HBC-LOAD-02 / HBC-LAUNCH-01:** C DOL/ELF relocation and launch handoff were compared with Rust validation, staging, and transfer. Rust checks ranges and reports recoverable failures. The launch path is partial until verified on Wii hardware.

The three ledger rows marked `out_of_scope` are intentionally excluded from this C parity review: the Rust-only network profile editor and scan, and the C update/installer feature.

## Review status

All 34 in-scope ledger rows have `review=source_reviewed`. One has `coverage=open` (the playtime record reset), and existing partial rows retain their recorded follow-up. The three out-of-scope rows remain excluded. No hardware-pending item is represented as hardware-verified.
