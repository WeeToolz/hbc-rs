# Dolphin API checks

This run passed 20 production PowerPC acceptance cases in an isolated Dolphin
profile. It covers the C parity APIs and the Rust-only extension APIs except
for the FAT storage group, which needs an SD fixture and has its own run.

The result JSON and build manifest record the DOL hash. The runner copied a
private NAND for the retained-record and network configuration cases. It did
not attach an SD fixture, so the startup no-storage case also ran in its
required environment. Physical Wii behavior remains pending.

Run command:

```sh
arch -arm64 python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-api-completion-2026-11-26.dol \
  --elf /Volumes/SSD/WiiDev/libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/dolphin-all-apis-2026-11-26-r2 \
  --cases protocol-primitives-api reborn-keys-api wiiload-publication-api \
    video-platform-api input-platform-api wii-startup-state-api render-frame-api \
    allocator-init-api network-config-extensions-api rust-control-extensions-api \
    render-primitives-extensions-api home-overlay-host-extensions-api \
    network-platform-extensions-api font-api zip-install-error-api \
    network-restart-api remote-input-api home-overlay-app-api runtime-state-api \
    wiiload-start-api
```
