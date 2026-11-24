# Reborn catalog directory error check

The production `catalog` acceptance case passed in Dolphin 2609a. Its
controlled storage fixture returns one valid app, then reports a directory
iteration error. Rust returns `ScanError::AppDirectory`, closes the directory,
and does not return the partial app list. The result and production DOL build
manifest are in this folder. The DOL SHA-256 is
`f23a4611eddc44d77d392287246beddd4b56703edf8dabffa4d93afa4f1a8ea4`.

Reborn's `app_entry_load_all` loop treats a null `readdir` result as end of
directory. It then filters and keeps entries loaded before that result. The
Rust behavior is a deliberate fail-closed extension: it reports an incomplete
scan instead of presenting a partial list as complete. This check validates
Rust's policy in the production scan function; it does not inject a fault into
the real FAT driver.

Command:

```sh
python3 tools/test_production_acceptance_dolphin.py \
  --dol /private/tmp/hbc-rs/catalog-errors-acceptance.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-api-catalog-errors-rust-dolphin-2026-11-24-r2 \
  --dolphin-arch x86_64 \
  --cases catalog
```

This is Dolphin evidence only. Real FAT directory errors and removable-media
behavior remain pending on Wii hardware.
