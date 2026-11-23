# Reborn release-date API Dolphin rerun

Dolphin 2609a passed the production `parse_app_xml` date case on the current
HBC source revision `08f1bdcf98d6241f3fadc54073be805b5de932d7`. The acceptance
DOL SHA-256 is `8653edfe9333eafb60f6e12d4b7dcd7faa9f9cdce13c37ae99ea487d86a5619f`.
The matching linker ELF and generated manifest are included. The result JSON
records `metadata-date` as successful with result zero. The raw Dolphin output
is in `metadata-date.log`.

Build command, run from the SDK checkout:

```sh
cargo xtask build-dol hbc-rs --bin emulator-acceptance \
  --manifest-path ../hbc-rs/Cargo.toml \
  --probe-features emulator-acceptance \
  --output ../hbc-rs/target/reborn-date-api-rerun/rust-probe.dol
```

The API test command, run from the HBC checkout:

```sh
python3 tools/test_production_acceptance_dolphin.py \
  --dol target/reborn-date-api-rerun/rust-probe.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-date-rust-dolphin-2026-11-22-rerun2 \
  --cases metadata-date
```

The eight tested date inputs match the independent C runtime probe documented
in the earlier [C and Rust comparison](../reborn-date-rust-dolphin-2026-11-22/README.md).
This run does not test every metadata API input, timezone variation, or
physical Wii behavior. The inventory therefore keeps `parse_app_xml` partial.
