# Rust app metadata API CDATA behavior in Dolphin

The production `metadata-app` acceptance case passed in Dolphin. It checks the
app metadata fields, `sort_id` prefix parsing, and both launch flags. It also
checks Reborn Mini-XML text selection: the first direct opaque text node wins,
text after comments or nested elements is ignored, nested-only field content
returns no value, and CDATA is ignored. Empty author data falls back to `coder`.

The independent C probe in
[`reborn-mxml-cdata-c-dolphin-2026-11-24-r4`](../reborn-mxml-cdata-c-dolphin-2026-11-24-r4/README.md)
uses Reborn's `_get_cdata` traversal and confirms the same outcomes in Dolphin.
The Rust acceptance DOL hash and source revisions are in `build.json`;
`result.json` records result zero. Dolphin ran as x86_64 under Rosetta with build
ID `409881c34357d0f105fd473167d15ab0bd9c1628`.

Run command:

```sh
python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-metadata-cdata-acceptance.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-api-metadata-app-rust-dolphin-2026-11-24-r1 \
  --cases metadata-app --dolphin-arch x86_64
```

This test checks the Rust parser in Dolphin and C's Mini-XML behavior in a
separate probe. It does not prove all XML error handling, all date and timezone
behavior, or physical Wii behavior.
