# Reborn metadata API boundary check

The production `metadata-app` acceptance case passed in Dolphin. It used the
fresh `emulator-acceptance` DOL built from source revision `46ab19d`, with the
metadata API checks added in that working tree. The DOL SHA-256 is
`1fc9427d5fa9c57459bd4eda72600afc13162a5ef4304e555ecb16e43ef97970`.

The case checks raw `long_description` text, entity decoding in an argument,
signed 32-bit `sort_id` limits, overflow saturation, a sign without digits,
and non-ASCII UTF-8 whitespace. The C reference uses `strtol` for `sort_id`;
the Wii target has 32-bit `long`, so `LONG_MIN` and `LONG_MAX` provide the
independent boundary values. Earlier production checks and C probes cover
`strtol` prefixes, nested argument traversal, and Mini-XML opaque text rules.

Command:

```sh
python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-metadata-app-parity.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-metadata-app-boundaries-rust-dolphin-2026-11-24-r2 \
  --dolphin-arch x86_64 \
  --cases metadata-app
```

`result.json` records a successful result. This is Dolphin evidence. Physical
Wii parser and storage behavior remain unverified.
