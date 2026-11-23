# Reborn localization API Dolphin evidence

The `localization-api` production acceptance case passed in Dolphin 2609a. It calls
the `Language` API directly in the acceptance program. The result and DOL build
manifest are archived beside this file; the raw case output is in
`localization-api.log`.

The case checks the supported console language values, negative and unknown
fallback values, Japanese selection only when the theme declares `ja`, six
representative translated strings, unknown dynamic-string fallback, and
received ZIP confirmation text for 2 KB and 1.00 MB. The ZIP checks include
localized wording, directory substitution, and the overwrite warning.

This is partial API evidence. It does not compare every generated translation
with Reborn's PO catalog, load the startup PO assets through the complete HBC
startup path, test all ZIP size boundaries, or prove language glyph rendering
on physical Wii hardware.

The first isolated launch exited before the Dolphin debugger started because
the sandboxed Qt process could not use the required NEON feature. That failed
log is preserved in the sibling
`reborn-api-localization-rust-dolphin-2026-11-23` directory. The rerun used the
same DOL with the system access needed by the existing Dolphin runner and
passed. No HBC launch control was needed because the test runs the Rust
acceptance program directly; HBC is not the program under test.

Run command:

```sh
python3 tools/test_production_acceptance_dolphin.py \
  --dol ../libogc-rs/target/xtask/custom-dols/hbc_rs-emulator-acceptance.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-api-localization-rust-dolphin-2026-11-23-r2 \
  --cases localization-api
```
