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
`extension` identifies APIs that an `HBC-EXT-*` or `HBC-REMOTE-INPUT-*` ledger
row explicitly marks as Rust-only with one or more exact `api_prefix=` scope
entries. These APIs remain listed, but they do not count as C parity work. Do
not add a prefix unless the named APIs have no matching C HBC contract.

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

The `scan_apps` and `visible_entries` APIs have a Dolphin acceptance case with a
controlled storage fixture. It checks file priority, size limits, folder
filtering, optional metadata, catalog filters, and custom sort IDs. The latest
run is recorded in
[`reborn-catalog-rust-dolphin-2026-11-23-r5`](review/evidence/reborn-catalog-rust-dolphin-2026-11-23-r5/README.md).
It does not replace a Wii scan over FAT or verify device failure behavior. Rust
keeps stable tie ordering where Reborn's `qsort` does not define it.

The archive management APIs have a production Dolphin case for the extraction
plan, output operations, byte total, staged commit, and rollback policy. The
[management API record](review/evidence/reborn-api-management-rust-dolphin-2026-11-23-r2/README.md)
states that commit and rollback use a controlled rename callback; it does not
prove physical FAT writes or recovery.

The theme package APIs have one Dolphin case for validation, root XML selection,
and file extraction. It checks two fixed archive fixtures. It does not compare
every ZIP edge case with Reborn's minizip path or prove the full theme install
flow. Rust's extra path and integrity checks are retained for safety.
The lower-level `ZipArchive` APIs now also have a direct production Dolphin
case for parsing, entry listing and lookup, bounded extraction, entry limits,
and path validation. The [archive API record](review/evidence/reborn-api-archive-rust-dolphin-2026-11-23/README.md)
records the tested behaviors and the remaining safety and compatibility gaps.
The PNG encoder, decoder, GX tile conversion, and owned texture accessors have
a direct production case with fixed pixel and tile results. See the
[PNG API record](review/evidence/reborn-api-png-rust-dolphin-2026-11-23/README.md)
for the tested contract and the remaining image and hardware checks.

The `ThemeFontFace` APIs have a direct Dolphin case for parsing the bundled
Droid face, rasterizing a glyph, line metrics, kerning, text width, and measured
memo layout. See the [font API record](review/evidence/reborn-api-font-rust-dolphin-2026-11-23/README.md).
This is partial evidence: it does not establish FreeType equivalence across all
theme fonts or physical Wii rendering behavior.

The localization API case checks supported and fallback language selection,
Japanese theme gating, representative translated strings, dynamic-string
fallback, and received ZIP confirmation formatting. See the
[localization API record](review/evidence/reborn-api-localization-rust-dolphin-2026-11-23-r2/README.md).
The case does not compare every generated PO catalog entry or the rendered
language glyphs on Wii hardware.

The loader helper case calls the policy, argument-building, and plan-validation
APIs directly in Dolphin. IOS and AHB hardware effects remain pending. Rust
keeps NUL-separated arguments and complete preflight validation as safety and
behavior extensions; Reborn uses one space-joined command string and validates
while relocating.

The browser controller case checks list-page movement, boundary stops, grid
mode changes, selection identity, grid row movement, and detail memo dragging
with a seven-entry catalog. It calls the production controller APIs in Dolphin.
The latest controller acceptance record is
[`reborn-app-controller-rust-dolphin-2026-11-23-r7`](review/evidence/reborn-app-controller-rust-dolphin-2026-11-23-r7/README.md).
Rendered widget geometry, transition animation, and physical controller
behavior remain separate checks.

A fresh production acceptance sweep passed 22 named Dolphin cases for launch,
settings state, Wiiload, metadata, catalog, themes, loader helpers, app
controller state, storage policy, input, rendering, and disc control. See
[`reborn-api-sweep-rust-dolphin-2026-11-23-r2`](review/evidence/reborn-api-sweep-rust-dolphin-2026-11-23-r2/README.md).
The sweep adds current run evidence for those named cases; it does not promote
unrelated APIs or complete the full contracts for the APIs that it calls.
The five `CatalogStorage` trait methods also have exact inventory entries tied
to a current `catalog` case; see the [trait API record](review/evidence/reborn-api-catalog-trait-rust-dolphin-2026-11-23/README.md).
