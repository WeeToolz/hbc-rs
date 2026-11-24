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
Use `path#Type::method` for a method name that appears more than once in the
same source file; the unqualified `path#pub fn name` form is for unique names.
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
The network profile extension scope includes its storage APIs, virtual keyboard
accessors, and the controller operation that clears an edited profile. The
wireless scan extension scope includes the platform scan API, its error message,
and controller state. Reborn has no network editor, virtual keyboard, or
wireless scan API. Physical network profile storage and wireless scanning remain
pending as recorded in HBC-EXT-01 and HBC-EXT-02.

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
The standard and widescreen `ThemeTextures` APIs also have a direct Dolphin
case. It checks the bundled 640 by 480 and 852 by 480 backgrounds, required
fallback textures, and that a valid packaged list card replaces its fallback.
Other installed theme images and physical GX output remain open; see the
[theme texture API record](review/evidence/reborn-api-theme-textures-rust-dolphin-2026-11-23-r4/README.md).
The theme archive storage APIs have a separate Dolphin round trip in a private
NAND copy. It checks save, replacement, exact readback, and that empty input
does not replace the saved archive. Wii NAND commit and recovery remain
pending; see the [theme storage record](review/evidence/reborn-api-theme-storage-rust-dolphin-2026-11-23/README.md).
The public settings NAND `load` and `save` APIs now link to the isolated
settings acceptance sequence. It checks saved XML bytes, loaded values, and
loading from the recovery backup path. Physical Wii storage behavior remains
pending; see the [settings storage record](review/evidence/reborn-api-settings-storage-rust-dolphin-2026-11-23/README.md).
The lower-level `ZipArchive` APIs now also have a direct production Dolphin
case for parsing, entry listing and lookup, bounded extraction, entry limits,
and path validation. The [archive API record](review/evidence/reborn-api-archive-rust-dolphin-2026-11-23/README.md)
records the tested behaviors and the remaining safety and compatibility gaps.
The PNG encoder, decoder, GX tile conversion, and owned texture accessors have
a direct production case with fixed pixel and tile results. See the
[PNG API record](review/evidence/reborn-api-png-rust-dolphin-2026-11-23/README.md)
for the tested contract and the remaining image and hardware checks.

The frame hit target API has a direct production case for overlapping target
registration, topmost selection, bounds, and non-finite coordinates. See the
[frame API record](review/evidence/reborn-api-render-rust-dolphin-2026-11-23/README.md).
Rendered hit-map geometry and physical pointer behavior remain pending.

The HOME overlay control APIs have a direct production case for one-shot
requests, open and close state, pending actions, failure retention, and exit
completion. See the [overlay control record](review/evidence/reborn-api-overlay-rust-dolphin-2026-11-23/README.md).
This checks the Rust control state machine in Dolphin; real frame capture,
controller input, and Wii system actions remain pending.

The application notice APIs have a direct production case for message kind,
long-text scrolling, timeout boundaries, and one-shot pointer-input handling.
See the [notice API record](review/evidence/reborn-api-notice-rust-dolphin-2026-11-23-r2/README.md).
C button countdown timing and rendered dialog behavior remain open.

The About APIs have a direct production case for the 120-tick delay,
three-tick scroll step, manual movement, end clamp, and theme-description
bound. See the [About API record](review/evidence/reborn-api-about-rust-dolphin-2026-11-23/README.md).
Rendered font metrics and physical Wii timing remain pending.

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
The latest direct API acceptance record is
[`reborn-api-controller-rust-dolphin-2026-11-23-r8`](review/evidence/reborn-api-controller-rust-dolphin-2026-11-23-r8/README.md).
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

A fresh eight-case Dolphin run on 2026-11-23 passed the localization, HOME
control, notice, About, application controller, render hit-test, metadata, and
catalog production API checks. The archived logs and result record are in
[`reborn-api-refresh-rust-dolphin-2026-11-23`](review/evidence/reborn-api-refresh-rust-dolphin-2026-11-23/README.md).
These results refresh only APIs named by those cases; they do not reduce the
remaining source-review, full-contract, or physical Wii requirements.

The main menu API case checks conditional BootMii item state, launch selection,
focus clamping, status labels, and the matching `AppRuntime` forwarding methods
in Dolphin. See [`reborn-api-main-menu-rust-dolphin-2026-11-23`](review/evidence/reborn-api-main-menu-rust-dolphin-2026-11-23/README.md).
The case supplies IOS254 availability as input. It does not prove BootMii title
detection or launch on a physical Wii.

The main menu API case also caught and fixed a parity gap: Rust removed the
BootMii item when IOS254 was unavailable, while Reborn keeps it visible and
disables activation. Rust now keeps the item in the list, prevents keyboard and
pointer activation, and skips it during focus movement. Dolphin verifies the
controller behavior. Actual IOS254 detection, rendered disabled styling, and
BootMii launch on Wii hardware remain pending.

The app-deletion production case now also checks the controller's public delete
confirmation state and keyboard and pointer choices. Reborn starts its Yes/No
dialog with No selected. See the [app deletion evidence](review/evidence/reborn-app-delete-rust-dolphin-2026-11-23/README.md).
Rendered dialog behavior and FAT deletion remain separate checks.

The executable-loading API case checks controller and runtime entry, bounded
progress, and completion behavior in Dolphin. See the [executable loading
evidence](review/evidence/reborn-api-executable-loading-rust-dolphin-2026-11-23/README.md).
Reborn uses its loader caption and activity indicator. This does not prove
rendered or physical Wii loading behavior.

The app detail focus API now has a direct Dolphin case for the Load default,
action order, and focus boundaries. Source comparison found that Rust had
focused Back by default and wrapped between actions; Reborn focuses Load and
keeps focus within the first and last usable widgets. See the [detail focus
evidence](review/evidence/reborn-api-detail-focus-rust-dolphin-2026-11-23/README.md).

The detail memo maximum-scroll accessor now has an exact Dolphin check. The
case sets the measured bound to three lines and verifies that the public API
returns three. Rendered text layout and physical Wii pointer behavior remain
open. See the [maximum scroll evidence](review/evidence/reborn-api-detail-max-scroll-rust-dolphin-2026-11-23/README.md).

The catalog reconciliation API has a direct production Dolphin case for
selection retention after insertion, page position, removal fallback, and an
empty catalog. See the [catalog reconciliation evidence](review/evidence/reborn-api-catalog-reconcile-rust-dolphin-2026-11-23/README.md).
Physical catalog refresh timing and rendered transitions remain separate.

The storage-volume Dolphin case checks all five settings-index mappings, the
matching mount names, and invalid-index handling through the `Volume` APIs.
Physical device mounts and hot-plug behavior remain pending. See the [storage
volume API record](review/evidence/reborn-storage-volume-rust-dolphin-2026-11-23/README.md).

The device availability API case checks all-absent, mixed, and all-present
probe results. It verifies runtime forwarding and Options eligibility in
Dolphin. Live Wii media probes and hot-plug behavior remain pending; see the
[device availability record](review/evidence/reborn-api-device-availability-rust-dolphin-2026-11-23/README.md).

The playtime reset API now has a Dolphin check against a private copy of the
existing NAND. It clears the nonzero four-byte prefix and preserves the rest
of the record. The original NAND remains unchanged; physical Wii ISFS behavior
is pending. See the [playtime reset record](review/evidence/reborn-api-playtime-reset-rust-dolphin-2026-11-23/README.md).
