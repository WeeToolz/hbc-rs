# FAT storage APIs in Dolphin

The production `fat-storage-api` case passed in Dolphin 2609a with the x86_64
build running under Rosetta. The arm64 app build exited before startup because
its Qt runtime rejected the available processor features. The acceptance runner
now records the selected architecture in `result.json`.

The case used a private folder-synced SD fixture and exercised the target-only
`FatStorage` implementation. It checked SD mount and unmount, file and
directory operations, bounded reads, ZIP extraction, rename behavior, PNG
write and readback, recursive removal, and cleanup. The pass log reports
`PASS FAT storage SD APIs, extraction and cleanup`.

The DOL SHA-256 is
`46a881e2f4c5c0474454ade545c7ce34c9ab00cc1963d4bdf679c41cb5cc8e9d`; its
source revision is recorded in `build.json`. The exact runner command was:

```sh
arch -arm64 python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-fat-storage-acceptance-r13.dol \
  --elf /Volumes/SSD/WiiDev/WeeToolz/libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-api-fat-storage-rust-dolphin-2026-11-24-r18 \
  --cases fat-storage-api --dolphin-arch x86_64
```

This proves folder-synced SD behavior in Dolphin. The separate host regression
checks the mount-prefix-to-unmount-name mapping for all five volumes. It does
not prove USB2 mounting, physical Wii FAT behavior, hot-plug, IOS DMA/cache
behavior, or write failures. These checks remain pending on hardware.
