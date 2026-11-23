# Reborn theme metadata parsing in Dolphin

The production `hbc_rs::metadata::parse_theme_xml` API passed its
`metadata-theme` acceptance case in Dolphin. The case checks decimal-prefix
conversion for theme colors and font sizes, ignores an unsupported case-mismatched
target, and merges the size and color from two entries for `appname`.

Reborn's `source/xml.c#load_theme_xml` calls `_get_color` and `_get_font`.
`_get_color` uses `_get_elem_int`, which calls `atoi` for each channel. `_get_font`
uses the same conversion for size and applies each nonzero field to the named
font target. Its target list is case-sensitive and contains seven fixed names.

Build and run the production API case with:

```sh
cd ../libogc-rs
cargo xtask build-dol hbc-rs --bin emulator-acceptance \
  --features emulator-acceptance --manifest-path ../hbc-rs/Cargo.toml \
  --output ../hbc-rs/target/hbc-rs-theme-acceptance-final.dol
cd ../hbc-rs
python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-theme-acceptance-final.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-theme-rust-dolphin-2026-11-23-r2 \
  --cases metadata-theme
```

This is partial parser evidence. Rust ignores negative font sizes, and checked
integer conversion rejects overflow safely. C `atoi` overflow is undefined, so
neither behavior is claimed as parity. Dolphin does not prove physical Wii
font rendering.
