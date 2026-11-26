# Current catalog API acceptance in Dolphin

The current PowerPC `catalog` production acceptance case passed in Dolphin.
It called `scan_apps` and `visible_entries` with controlled in-memory storage.
The case checked candidate priority and size limits, invalid folders,
metadata and icon filtering, custom sort IDs, ASCII case folding, UTF-8 byte
ordering, newest-first date order, undated entries, equal-date tie ordering,
directory iteration errors, and reader cleanup.

The run used the current source revision recorded in `build-manifest.json`.
The result JSON and manifest contain the same DOL SHA-256. This is a direct
Rust API check; it does not exercise FAT or rendered browser sorting. Reborn's
equal-date comparator returns `-1` for equal nonzero dates, which violates the
`qsort` comparator contract. Rust keeps deterministic name and directory ties.
The C comparison for `strcasecmp` byte behavior is recorded in the parity
ledger. Real Wii libc and FAT behavior remain pending.

Build command, run from `/Volumes/SSD/WiiDev/libogc-rs`:

```sh
cargo xtask build-dol hbc-rs --bin emulator-acceptance \
  --features emulator-acceptance \
  --manifest-path ../WeeToolz/hbc-rs-origin/Cargo.toml \
  --output ../WeeToolz/hbc-rs-origin/target/hbc-rs-current-catalog-acceptance.dol
```

Dolphin command, run from the hbc-rs checkout:

```sh
arch -arm64 python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-current-catalog-acceptance.dol \
  --elf /Volumes/SSD/WiiDev/libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-catalog-current-api-rust-dolphin-2026-11-25 \
  --cases catalog
```
