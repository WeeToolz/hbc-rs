# Reborn memo whitespace and clipping in Dolphin

Reborn's `channel/channelapp/source/font.c#font_wrap_string` uses ASCII space
as its word break, preserves spaces in each returned line, ignores carriage
returns, and keeps a glyph that reaches or exceeds the line width. Its widget
scissor clips the drawing. Rust's measured layout now follows those rules. It
also preserves tabs as glyphs instead of treating all Unicode whitespace as a
break.

The production PowerPC `text-layout-api` acceptance case passed in Dolphin. It
checks repeated spaces, tabs, carriage returns, edge wrapping, exact-fit and
over-width glyphs, and the trailing empty row after a full-width glyph. The
result JSON records the acceptance DOL SHA-256. The normal production DOL also
built successfully for PowerPC with SHA-256
`3da4c6a558abaaeea00047c44ec7338797057a4a1ff76ce6cbbb6ea544b5c425`; its
manifest is in `production-build-manifest.json`.

The host suite passed 489 tests. The parity ledger and generated API inventory
record the remaining font metric, theme rendering, and physical Wii limits.
This acceptance case does not compare FreeType and fontdue raster output.

Build and run commands:

```sh
# From /Volumes/SSD/WiiDev/libogc-rs
cargo xtask build-dol hbc-rs --bin emulator-acceptance \
  --features emulator-acceptance \
  --manifest-path ../WeeToolz/hbc-rs-origin/Cargo.toml \
  --output ../WeeToolz/hbc-rs-origin/target/hbc-rs-text-layout-whitespace-acceptance.dol

cargo xtask build-dol hbc-rs --bin hbc-rs \
  --manifest-path ../WeeToolz/hbc-rs-origin/Cargo.toml \
  --output ../WeeToolz/hbc-rs-origin/target/hbc-rs-production-text-layout-whitespace-2026-11-25.dol

# From the hbc-rs checkout
arch -arm64 python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-text-layout-whitespace-acceptance.dol \
  --elf /Volumes/SSD/WiiDev/libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-api-text-layout-whitespace-rust-dolphin-2026-11-25-r4 \
  --cases text-layout-api
```
