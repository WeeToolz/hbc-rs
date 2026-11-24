# HBC runtime state API check

The `runtime-state-api` production acceptance case passed in Dolphin. It calls
the public `AppRuntime` methods for catalog replacement, widescreen grid
policy, theme application and lookup, generic and typed notices, dirty
settings saves, and exit preparation. It checks selected app identity,
three- and four-column layouts, applied theme metadata, notice state, save
failure retention, successful save state, and preservation after a settings
load failure.

These checks cover the Rust runtime state paths. They do not prove live storage
scans, theme asset loading or GX output, NAND file I/O, or physical Wii display
and input behavior. The settings persistence switch is a Rust safety control
that prevents overwriting unreadable settings; it has no matching Reborn public
runtime API.

The acceptance DOL SHA-256 is recorded in `result.json`. The successful test
command was:

```sh
python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-runtime-state-acceptance.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-api-runtime-state-rust-dolphin-2026-11-23-r2 \
  --cases runtime-state-api
```
