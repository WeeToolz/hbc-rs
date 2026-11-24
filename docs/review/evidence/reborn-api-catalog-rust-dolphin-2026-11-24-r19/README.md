# Production catalog sorting in Dolphin

The production `catalog` acceptance case passed in Dolphin with the x86_64
build running under Rosetta. In addition to the existing scan, filter, and
custom-sort checks, this run checks that UTF-8 metadata names `Ä` and `ä` sort
by the byte behavior used by Reborn's `strcasecmp` after it selects
`C-ISO-8859-1` in `config_language`.

The DOL SHA-256 is
`b2dfaf6bf7ef0bf377fc5f51545df2f6893087d44f43ae14797a2579d88819e4`; its
source and SDK revisions are recorded in `build.json`. The exact run command
was:

```sh
arch -arm64 python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-catalog-acceptance-latin1.dol \
  --elf /Volumes/SSD/WiiDev/WeeToolz/libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-api-catalog-rust-dolphin-2026-11-24-r19 \
  --cases catalog --dolphin-arch x86_64
```

This is Dolphin evidence for the production Rust catalog API. It does not
prove physical FAT ordering or Wii libc behavior.
