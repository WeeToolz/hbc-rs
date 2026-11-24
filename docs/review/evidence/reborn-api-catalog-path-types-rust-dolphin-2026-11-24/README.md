# Reborn catalog path type behavior

The `catalog` and `fat-storage-api` production acceptance cases passed in
Dolphin 2609a. The controlled catalog case places a directory at
`boot.elf` and a valid `boot.dol` in the same app folder. The directory's
zero size takes ELF priority, so the app is skipped without trying the DOL.
It also checks that directory paths for `meta.xml` and `icon.png` are treated
as absent optional files.

The FAT API case creates directories in the Dolphin SD fixture and checks
them through the production `CatalogStorage` implementation. A directory
candidate returns `Some(0)` from `file_size`; directory metadata and icon
paths return `None` from `read_file`. This matches Reborn's `stat` and later
failed file opens. The acceptance DOL SHA-256 is
`86ddc446348c5854410f45564271bc0d3aef4ae34a5284701c21133c0f248232`.

Command:

```sh
python3 tools/test_production_acceptance_dolphin.py \
  --dol /private/tmp/hbc-rs/catalog-path-types-acceptance.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-api-catalog-path-types-rust-dolphin-2026-11-24 \
  --dolphin-arch x86_64 \
  --cases catalog fat-storage-api
```

This checks the production Rust paths on Dolphin's SD fixture. Physical Wii
FAT behavior and I/O errors remain pending.
