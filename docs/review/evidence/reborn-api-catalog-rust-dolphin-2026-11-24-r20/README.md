# Production catalog sorting in Dolphin

The production `catalog` acceptance case passed in Dolphin with the x86_64
build running under Rosetta. It checks the existing scan, filters, and custom
sort IDs. It also checks the ordering expected from the independent C runtime
probe in [the companion evidence](../reborn-c-collation-dolphin-2026-11-24-r3/README.md):
ASCII `Alpha` and `alpha` compare equal, while UTF-8 `Ā` sorts before U+0800
because `strcasecmp` leaves high bytes unchanged.

The DOL SHA-256 and source and SDK revisions are recorded in `build.json`.
The exact run command was:

```sh
arch -arm64 python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-catalog-acceptance-strcasecmp.dol \
  --elf /Volumes/SSD/WiiDev/WeeToolz/libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-api-catalog-rust-dolphin-2026-11-24-r20 \
  --cases catalog --dolphin-arch x86_64
```

This verifies the production Rust API against the observed devkitPPC runtime
under Dolphin. Physical Wii libc and FAT catalog behavior remain pending.
