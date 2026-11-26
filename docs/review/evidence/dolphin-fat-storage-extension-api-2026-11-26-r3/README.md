# Dolphin FAT extension API check

This run passed the production `fat-storage-api` case in Dolphin. The runner
used a private NAND copy and an isolated SD fixture. The case directly calls
`FatStorage::rename_path` and `repair_interrupted_directory_rename`, along with
the other public FAT storage APIs. The fixture and private profile were
removed after the run to recover temporary disk space.

The result JSON and build manifest record the DOL hash. Physical Wii FAT
behavior remains pending.

Run command:

```sh
arch -arm64 python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-api-completion-2026-11-26.dol \
  --elf /Volumes/SSD/WiiDev/libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/dolphin-fat-storage-extension-api-2026-11-26-r3 \
  --cases fat-storage-api
```
