# HBC text layout API check

The `text-layout-api` production acceptance case passed in Dolphin. It calls
the public measured memo layout, character-bounded wrapping, and memo paragraph
wrapping APIs. It checks proportional width measurement, visible line
selection, scroll clamping, invalid width rejection, long-word splitting,
zero-width handling, and preservation of empty paragraphs.

The compared C behavior is in Reborn's `font_wrap_string` and `widget_memo`
paths. This checks returned Rust layout data in Dolphin. It does not prove
theme font measurement, rendered line breaks, or physical Wii layout.

The acceptance DOL SHA-256 is recorded in `result.json`. The successful test
command was:

```sh
python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-text-layout-acceptance.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-api-text-layout-rust-dolphin-2026-11-23-r2 \
  --cases text-layout-api
```
