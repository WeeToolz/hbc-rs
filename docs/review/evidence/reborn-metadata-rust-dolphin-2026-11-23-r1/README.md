# Rust metadata APIs compared with Reborn in Dolphin

The production `hbc_rs::metadata::parse_app_xml` and `parse_update_xml` APIs passed their Dolphin acceptance cases. The date case checks 17 inputs. The update case checks 8 inputs against the Reborn C scanner: decimal prefix parsing, ASCII whitespace, both signs, trailing text, invalid input, zero, empty content, a non-ASCII space, and a missing element.

`comparison.json` checks the C outputs against the Rust acceptance result. C stores zero for missing or invalid update dates. Rust preserves its safer `Option<u64>` result and returns `None` for those values. Integer overflow is not included because C `scanf` overflow behavior is implementation-specific. The full update service remains disabled in Reborn.

The acceptance DOL SHA-256 is `6f30da7dd9ddc2d1235ba94712a351a2ca184193f3d328d7087db540e8a52028`. `build.manifest.json` records the target build. The Dolphin run used a minimal private profile under `/private/tmp/hbc-rs` and did not copy the user's NAND.

Run both production API cases with:

```sh
python3 tools/test_production_acceptance_dolphin.py \
  --dol target/reborn-date-c/rust-metadata.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-metadata-rust-dolphin-2026-11-23-r1 \
  --cases metadata-date metadata-update
```

Dolphin results do not prove physical Wii or timezone behavior.
