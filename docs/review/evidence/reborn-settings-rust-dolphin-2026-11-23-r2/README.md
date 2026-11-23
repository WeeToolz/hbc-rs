# Reborn settings numeric parsing in Dolphin

The production `hbc_rs::metadata::parse_settings_xml` API passed three acceptance
cases in Dolphin. They check signed decimal prefixes with trailing text,
missing-field defaults, and present text without a decimal prefix. The expected
behavior follows Reborn's `source/xml.c#_get_elem_int`, which calls `atoi` when
content exists and uses a field-specific fallback when it does not.

Build the test image and run it with:

```sh
cd ../libogc-rs
cargo xtask build-dol hbc-rs --bin emulator-acceptance \
  --features emulator-acceptance --manifest-path ../hbc-rs/Cargo.toml \
  --output ../hbc-rs/target/hbc-rs-settings-acceptance.dol
cd ../hbc-rs
python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-settings-acceptance.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-settings-rust-dolphin-2026-11-23-r2 \
  --cases metadata-settings
```

The result is a partial parser check, not proof of the complete settings API.
The Rust scanner uses checked arithmetic for safety. `atoi` overflow in C is
undefined, so overflow behavior is not part of this comparison. Dolphin does
not prove physical Wii behavior.
