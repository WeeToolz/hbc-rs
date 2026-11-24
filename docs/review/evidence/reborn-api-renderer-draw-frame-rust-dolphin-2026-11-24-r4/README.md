# Production GX scene renderer in Dolphin

The `renderer-draw-frame-api` production acceptance case builds a browser frame
with the bundled theme and calls the real Wii GX `draw_frame` API. It captures
the EFB and checks dimensions of 640 by 480. At pixel (2, 2), the rendered
background must match `[22, 112, 157, 255]`, decoded independently from the
bundled `assets/background.png` file. The same case then presents, waits for
retrace, shows, and shuts down the frame.

Dolphin 2609a passed with result 0. The DOL SHA-256 in `result.json` matches
`build-manifest.json`. Two earlier source-pixel checks failed their combined
dimension or pixel condition. Those results remain in the parent and r2
evidence directories. Run r3 passed with a fixed RGBA8 fixture; r4 now passes
with the bundled background and a known source pixel.

This checks one point in one production scene. It does not prove the full HBC
scene, theme variation, Broadway cache behavior, or physical Wii scanout.

Run command:

```sh
python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-renderer-acceptance.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-api-renderer-draw-frame-rust-dolphin-2026-11-24-r4 \
  --cases renderer-draw-frame-api
```
