# Reborn memo edge wrapping in Dolphin

Reborn's `channel/channelapp/source/font.c#font_wrap_string` starts a new line
when the current glyph reaches the memo width (`cx + w >= max_width`). Rust's
measured memo layout used to keep a candidate whose measured width equaled the
limit. It now uses a strict interior bound and keeps a single glyph that fills
the full width, as the C routine does.

The host suite passed 488 tests. The PowerPC `text-layout-api` acceptance case
passed in Dolphin and checks the exact-width line break. A host test also checks
that one glyph can fill the full width. The result JSON records the DOL
SHA-256. This case verifies the Rust production API behavior in Dolphin; it does
not compare FreeType and fontdue glyph metrics. Glyph ink bounds, theme-font
rendering, scale behavior, and physical Wii layout remain open.

Build command, run from `/Volumes/SSD/WiiDev/libogc-rs`:

```sh
cargo xtask build-dol hbc-rs --bin emulator-acceptance \
  --features emulator-acceptance \
  --manifest-path ../WeeToolz/hbc-rs-origin/Cargo.toml \
  --output ../WeeToolz/hbc-rs-origin/target/hbc-rs-text-layout-edge-acceptance.dol
```

Dolphin command, run from the hbc-rs checkout:

```sh
arch -arm64 python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-text-layout-edge-acceptance.dol \
  --elf /Volumes/SSD/WiiDev/libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-api-text-layout-edge-rust-dolphin-2026-11-25-r2 \
  --cases text-layout-api
```
