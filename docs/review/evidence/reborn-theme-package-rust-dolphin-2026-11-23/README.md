# Reborn theme archive APIs in Dolphin

The production `ThemePackage::parse`, `theme_xml_name`, and `extract_file`
APIs passed the `theme-package` acceptance case in Dolphin. It uses two fixed
ZIP fixtures:

- `tests/fixtures/theme-valid.zip` contains root `theme.xml` and `button.png`.
  Parsing returns the expected description, the XML accessor returns the root
  filename, and extraction returns the exact image payload. A missing file is
  rejected.
- `tests/fixtures/theme-invalid-entry.zip` contains an unsupported `unknown.bin`
  entry and is rejected.

Fixture SHA-256 values:

- `theme-valid.zip`: `69079a286de6852be700cf6e19271218ccb7ee71f366763ea61e28f6ec140d26`
- `theme-invalid-entry.zip`: `68d2636bfb2f5e985a5e7ab30ef9dc64072843f82c446c403754f2d27b55c299`

Reborn checks the archive with `manage.c#manage_check_zip_theme`, finds theme
assets with `theme.c#theme_is_valid_fn`, then extracts entries in
`manage.c#manage_extract_zip`. These tests cover only selected valid and invalid
cases. They do not cover all ZIP library differences, installation failures,
or physical Wii behavior. Rust keeps stricter path, duplicate-entry,
decompression, and integrity checks for safety.

Run the production case with:

```sh
cd ../libogc-rs
cargo xtask build-dol hbc-rs --bin emulator-acceptance \
  --features emulator-acceptance --manifest-path ../hbc-rs/Cargo.toml \
  --output ../hbc-rs/target/hbc-rs-theme-package-acceptance.dol
cd ../hbc-rs
python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-theme-package-acceptance.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-theme-package-rust-dolphin-2026-11-23 \
  --cases theme-package
```
