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

The current API-specific runtime evidence is the `parse_app_xml` date case in
Dolphin. It compares eight production Rust parser cases with an independent
C-runtime probe. Its result is partial API evidence, not proof of the entire
metadata API. See [the Rust Dolphin evidence](review/evidence/reborn-date-rust-dolphin-2026-11-22/README.md)
and [the C probe evidence](review/evidence/reborn-date-c-dolphin-2026-11-22/).
No physical Wii API result is implied by Dolphin. Hardware checks remain
separate in the parity ledger.

To expand coverage, select related entries from the generated CSV, review each
contract against the HBC Reborn C source, then add API-specific production
checks and named evidence. Run the relevant host tests, target build, and
Dolphin probe before linking evidence. Preserve any failed or incomplete run
with its limits; do not change it into a passing result.
