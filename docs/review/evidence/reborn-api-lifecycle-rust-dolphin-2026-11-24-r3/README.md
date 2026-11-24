# Lifecycle APIs in Dolphin

The production `lifecycle-api` acceptance case checks the reset and power
request signal behavior and cleanup order. It now also calls the production
`debugger_mode`, `initialize_debugger`, `take_debugger_break`,
`register_callbacks`, and `take_request` APIs. The normal acceptance build must
report debugger mode Off, must not start a debugger, must report no pending break,
and must leave the exit request queue empty after registering callbacks.

The Dolphin run passed with result 0. The DOL SHA-256 is recorded in
`result.json` and matches `build-manifest.json`. Two earlier attempts failed
before the test started because Dolphin reported an incompatible processor
requirement for NEON. Their logs and results are preserved in the sibling
`reborn-api-lifecycle-rust-dolphin-2026-11-24` and `...-r2` directories. The
passing retry ran with GUI access.

This checks the normal build only. USB Gecko and network debugger feature
builds, real adapter detection, and physical reset or power callback delivery
remain unverified. The Reborn reference registers its reset and power callbacks
in `channel/channelapp/source/main.c` and routes reset to the debugger only
when `GDBSTUB` is enabled.

Run command:

```sh
python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-lifecycle-acceptance.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-api-lifecycle-rust-dolphin-2026-11-24-r3 \
  --cases lifecycle-api
```
