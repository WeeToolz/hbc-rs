# Protocol and retained-record APIs in Dolphin

The `protocol-primitives-api` case calls the production PowerPC APIs for HBCF
operation and header decoding, path checks and app names; HBCN target encoding,
decoding and heap bounds; HBCH page selection, JSON construction and a
read-only hardware response; and Reborn Wiiload style, upload metadata, timing,
footer formatting and stack high water. It also writes valid synthetic HBCC
and HBCL records to their reserved MEM2 addresses in the Dolphin process, then
calls the production crash-report and last-log initialization, query,
serialization, and clear APIs. The case returned 0 in Dolphin with a private
copy of the Wii NAND.

The synthetic records test parsing and consumption. They are not evidence of a
real crash or retained log from a Wii application. Hardware memory placement
and IOS behavior remain pending. The DOL SHA-256 and build details are in
`result.json` and `build-manifest.json`.

Earlier attempts with the same API case are preserved in the sibling `...-r1`
through `...-r5` directories. This `...-r6` run adds the retained-record API
checks and is the current passing record.

Run command:

```sh
arch -arm64 python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-protocol-api-acceptance-r6.dol \
  --elf /Volumes/SSD/WiiDev/libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/dolphin-api-protocol-primitives-2026-11-26-r6 \
  --cases protocol-primitives-api
```
