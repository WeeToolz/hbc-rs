# Rust release-date API comparison in Dolphin

The production `hbc_rs::metadata::parse_app_xml` API passed its `metadata-date` acceptance case in Dolphin. The original run checked all 17 inputs listed by the independent C probe. The current comparison table also includes ten calendar and `time_t` boundary inputs. See the 2026-11-24 boundary evidence for the fresh 27-case C and Rust runs.

The acceptance DOL SHA-256 is `e21902d7ef29c24537d31f61cfa160f48c2db5d915aa462d9c2ed014faa68ba6`. `build.manifest.json` records its target build. The Rust run used a small private profile under `/private/tmp/hbc-rs`; it did not copy or modify the user's Dolphin NAND.

Run the production API case with:

```sh
python3 tools/test_production_acceptance_dolphin.py \
  --dol target/reborn-date-c/rust-expanded.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-date-rust-dolphin-2026-11-23-expanded \
  --cases metadata-date
```

The run verifies Dolphin behavior only. Physical Wii date and timezone behavior remain unverified.
