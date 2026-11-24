# HBC runtime Wiiload API check

The `wiiload-controller` production acceptance case passed in Dolphin. It now
calls the public `AppRuntime` methods for ZIP confirmation, receive and install
screens, progress updates, and progress reads. It uses `AppRuntime::update` for
the confirmation input. The case checks the resulting screen, prompt, scroll
state, progress bounds, and completion transitions.

The check compares these runtime forwarding paths with Reborn's ZIP handling in
`channel/channelapp/source/loader.c` and progress updates in
`channel/channelapp/source/dialogs.c`. It checks runtime state in Dolphin only.
It does not test a live upload, ZIP extraction, physical storage writes, theme
font rendering, or physical Wii behavior.

The acceptance DOL SHA-256 is recorded in `result.json`. The test command was:

```sh
python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-runtime-wiiload-acceptance.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-api-runtime-wiiload-rust-dolphin-2026-11-23-r2 \
  --cases wiiload-controller
```
