# Reborn catalog API acceptance in Dolphin

The `catalog` production acceptance case passed with result `0` in Dolphin
2609a. It exercised
`scan_apps` and `visible_entries` with a controlled storage fixture. Checks
covered executable priority, size limits, hidden and invalid folders, optional
metadata, metadata filtering, custom sort IDs, and icon filtering.

The acceptance image was built from source revision `e4c0c9191678340072f03f3e8aec1244240c5f65` with the `emulator-acceptance` feature. Its SHA-256 is
`0fd2eeed0abbc1a373fa1b92ea6cd0bc3586357c8f714b758a1ea0cef4afe1ae`.

Run from the `hbc-rs` repository root:

```sh
python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-catalog-acceptance-r4.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-catalog-rust-dolphin-2026-11-23-r5 \
  --cases catalog
```

The passing result is in `result.json`; `catalog.log` records Dolphin shutdown.
The runner used a private Dolphin profile. A sandboxed attempt failed before
the test began because Qt could not detect NEON. The passing run used normal
tool escalation. This evidence does not prove FAT behavior or physical Wii
behavior. Real storage and hardware checks remain pending.
