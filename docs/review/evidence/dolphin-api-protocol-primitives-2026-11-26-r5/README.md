# Protocol helper APIs in Dolphin

The `protocol-primitives-api` case calls the production PowerPC APIs for HBCF
operation decoding, header decoding, path checks and changed app names; HBCN
target encoding, decoding and heap limits; HBCH page selection, JSON building
and hardware response; and Reborn Wiiload style, metadata, timing, footer and
stack helpers. The case returned 0 in Dolphin with a private copy of the Wii
NAND.

The DOL SHA-256 is recorded in `result.json` and `build-manifest.json`. The
manifest records source revision `3e401b9b4c5a66fb2b79fad445b5509f17b2eaab`
and the acceptance source changes.

The first three attempts exposed errors in the test fixture and assertions.
Their logs and results remain in the sibling `...-r1`, `...-r2`, and `...-r3`
directories. The `...-r4` run passed the helper checks. This `...-r5` run also
calls `hardware_response` against the private Dolphin NAND and is the current
passing record.

Run command:

```sh
arch -arm64 python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-protocol-api-acceptance-r5.dol \
  --elf /Volumes/SSD/WiiDev/libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/dolphin-api-protocol-primitives-2026-11-26-r5 \
  --cases protocol-primitives-api
```

This verifies the named API inputs in Dolphin. It does not prove physical Wii
memory placement, IOS behavior, or complete protocol parity.
