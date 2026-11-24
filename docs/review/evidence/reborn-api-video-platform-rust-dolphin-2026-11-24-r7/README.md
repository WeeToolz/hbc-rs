# Video, HOME overlay, and screenshot APIs in Dolphin

The `video-platform-api` production acceptance case now calls the Wii HOME
overlay GX chrome and screenshot helper APIs. The overlay opens through the
production model, draws its strip through `draw_chrome`, and checks the known
panel and selected-item EFB pixels. The screenshot helper rejects the no-storage
path without calling the save closure. With storage enabled, it reads the EFB,
encodes PNG bytes, calls the save closure, decodes the PNG, and checks its
dimensions and a known panel pixel.

Dolphin 2609a passed the case with result 0. The DOL SHA-256 in `result.json`
matches `build-manifest.json`. This verifies rendering and image conversion in
Dolphin. It does not verify writing to an SD card, Wii GX cache coherency, or
physical framebuffer capture. Reborn's screenshot writes a BMP to SD; the Rust
helper retains PNG encoding and a caller-provided save backend.

Run command:

```sh
python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-video-overlay-acceptance.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-api-video-platform-rust-dolphin-2026-11-24-r7 \
  --cases video-platform-api
```
