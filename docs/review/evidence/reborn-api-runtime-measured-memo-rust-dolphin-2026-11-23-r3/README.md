# HBC runtime measured memo API check

The `runtime-measured-memo-api` production acceptance case passed in Dolphin.
It calls `AppRuntime::set_measured_memo` for a notice and checks the scroll
bound, accumulation of less than one measured line, completion of a line, and
clamping at the maximum scroll position.

This checks the runtime forwarding path and memo state in Dolphin. It does not
prove measured theme font geometry, rendered notice layout, live pointer
movement, or physical Wii behavior.

The acceptance DOL SHA-256 is recorded in `result.json`. The successful test
command was:

```sh
python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-runtime-measured-memo-acceptance.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-api-runtime-measured-memo-rust-dolphin-2026-11-23-r3 \
  --cases runtime-measured-memo-api
```
