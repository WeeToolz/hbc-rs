# HBC runtime pointer API check

The `runtime-pointer-api` production acceptance case passed in Dolphin. It
calls the public `AppRuntime` pointer methods and checks focus without
activation, pointer activation through a pointer frame, and ordering when a
pointer activation arrives with page-navigation input.

The check compares runtime orchestration with Reborn's pointer hit-test and
frame paths in `channel/channelapp/source/view.c` and
`channel/channelapp/source/main.c`. It checks model state in Dolphin. Live
WPAD coordinates, rendered hit areas, physical Wii input timing, and display
behavior remain open.

The acceptance DOL SHA-256 is recorded in `result.json`. The successful test
command was:

```sh
python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-runtime-pointer-acceptance.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-api-runtime-pointer-rust-dolphin-2026-11-23-r2 \
  --cases runtime-pointer-api
```
