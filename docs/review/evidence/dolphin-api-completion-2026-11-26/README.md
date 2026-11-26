# Remaining public APIs in Dolphin

This acceptance run passed seven production PowerPC API cases in Dolphin with
an isolated profile. It checks the protocol and retained-record helpers, HBCK
key queue and pacing, Wiiload publication state, video screenshot publication,
PAD initialization and three scans, startup no-media refresh and scan duration,
and rendered Wiiload transfer status.

`wiiload-publication-api` verifies its state publication calls through
acceptance-only readers that are compiled only with the
`emulator-acceptance` feature. The normal HBC build has no such readers. The
screenshot framebuffer pointer is checked for presence and its dimensions and
length are checked against the live VideoSystem information.

The result JSON contains one successful record for each case. The DOL hash and
source revision are in `result.json` and `build-manifest.json`. Physical Wii
controller, storage, GX, IOS, and reserved-memory behavior remain pending.

Run command:

```sh
arch -arm64 python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-api-completion-2026-11-26.dol \
  --elf /Volumes/SSD/WiiDev/libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/dolphin-api-completion-2026-11-26 \
  --cases protocol-primitives-api reborn-keys-api wiiload-publication-api \
    video-platform-api input-platform-api wii-startup-state-api render-frame-api
```
