# HBC public API verification

The [generated API inventory](generated/api-inventory.csv) lists the public
functions and public trait methods in the default-feature `hbc-rs` library for
the Wii target. The [summary](generated/api-status.md) reports the evidence
state for each entry. It does not include binary-only entry points or APIs that
appear only with optional features. The inventory generator rejects an unknown
rustdoc JSON format and checks that every `HBC-API-*` ledger reference resolves
to one public API.

Generate or check the inventory with:

```sh
python3 tools/api_inventory.py refresh
python3 tools/api_inventory.py check
```

Each API entry can link to an `HBC-API-*` row in
[`config/parity-ledger.csv`](../config/parity-ledger.csv). Add a row only when a
test or probe calls that exact production API and checks observable behavior.
`partial` means the named check passes but it does not cover the full API
contract. `unverified` means no exact passing check is linked; broad host,
target, or Dolphin tests may still exercise the API. `verified` requires all
required contract evidence. `blocked` requires evidence that the remaining
requirement depends on an unavailable prerequisite. The generator does not
infer either state from a build, source review, or a broad subsystem test.

API-specific runtime evidence includes 17 `parse_app_xml` date cases and 8
`parse_update_xml` date cases compared with an independent C-runtime probe in
Dolphin. This is partial API evidence. It does not prove all C runtime timezone
behavior or integer overflow behavior. See [the combined Rust Dolphin evidence](review/evidence/reborn-metadata-rust-dolphin-2026-11-23-r1/README.md)
and [the combined C probe evidence](review/evidence/reborn-metadata-c-dolphin-2026-11-23-r2/README.md).
No physical Wii API result is implied by Dolphin. Hardware checks remain
separate in the parity ledger.

The `start_stop` and `shutdown` APIs now have a named Dolphin acceptance case
for Reborn's `/dev/di` callback chain. It checks open, ioctl `0xe3`, close,
callback completion, and the drain result. The [direct API record](review/evidence/disc-stop-rust-dolphin-2026-11-22-r8/README.md)
and [standard HBC startup capture](review/evidence/disc-startup-rust-async-2026-11-22-r2/README.md)
are separate evidence. They do not prove physical Wii behavior.

The update API comparison records that Rust keeps `None` for absent or invalid
dates, while C leaves its zero-initialized field at zero. It does not verify the
disabled update service.

The `parse_settings_xml` API has Dolphin cases for Reborn's `atoi` prefix rules,
present invalid values, and missing-field defaults. The `parse_theme_xml` API
checks the same numeric conversion for theme colors and font sizes, supported
font targets, and field-wise updates across repeated font entries. These are
partial parser checks; they do not verify the full theme renderer or physical
Wii behavior. C `atoi` overflow is undefined, and Rust keeps its checked
conversion.

To expand coverage, select related entries from the generated CSV, review each
contract against the HBC Reborn C source, then add API-specific production
checks and named evidence. Run the relevant host tests, target build, and
Dolphin probe before linking evidence. Preserve any failed or incomplete run
with its limits; do not change it into a passing result.

The `parse_dol` and `parse_elf32_ppc` APIs also have named Dolphin launch cases.
Each case sends a synthetic executable through the production parser, staging
path, and handoff to a test child. These cases prove the selected load paths
run in Dolphin; they do not prove full format parity or Broadway cache behavior.
See [the loader acceptance record](review/evidence/reborn-loader-rust-dolphin-2026-11-23/README.md).
