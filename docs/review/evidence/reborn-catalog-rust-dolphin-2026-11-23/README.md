# Reborn catalog scan policy in Dolphin

The production `scan_apps` and `visible_entries` APIs passed the `catalog`
acceptance case in Dolphin. A controlled `CatalogStorage` fixture checks:

- ELF priority over DOL and theme files in one folder;
- executable and theme size limits;
- hidden, unsafe, and non-directory entries;
- valid apps with malformed optional metadata;
- metadata and icon filters; and
- custom sort IDs.

Reborn's corresponding paths are `appentry.c#app_entry_load_single`,
`appentry.c#app_entry_filter`, and `appentry.c#cmp_app_entry`. The case checks
Rust policy against those source rules. It does not test real FAT directory
ordering, media errors, or Wii behavior. Rust uses stable ordering for ties;
Reborn uses `qsort`, which leaves equal-key ordering unspecified.

Build and run the acceptance image with:

```sh
cd ../libogc-rs
cargo xtask build-dol hbc-rs --bin emulator-acceptance \
  --features emulator-acceptance --manifest-path ../hbc-rs/Cargo.toml \
  --output ../hbc-rs/target/hbc-rs-catalog-acceptance.dol
cd ../hbc-rs
python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-catalog-acceptance.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-catalog-rust-dolphin-2026-11-23 \
  --cases catalog
```
