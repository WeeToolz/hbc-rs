# HBC render frame API check

The `render-frame-api` production acceptance case passed in Dolphin. It calls
the public browser frame APIs and checks standard and widescreen logical
widths, app hit targets, a theme font color, a German Options label, and
clipped rectangle mapping with a rejected zero target width.

The compared C paths include Reborn's video viewport setup, theme and
localization paths, and GX coordinate mapping. This checks returned frame data
in Dolphin. It does not prove physical video mode detection, GX scanout, theme
asset loading, or rendered glyph behavior on Wii hardware.

The acceptance DOL SHA-256 is recorded in `result.json`. The successful test
command was:

```sh
python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-render-frame-acceptance.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-api-render-frame-rust-dolphin-2026-11-23 \
  --cases render-frame-api
```
