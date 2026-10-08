# Review of the former scope exclusions

Date: 2026-10-07

This review covers the three ledger entries that were previously marked
`out_of_scope`. It traces production callers and backends. It does not enable
updates, execute an installer, or change Wii system network settings.

The network editor and scan are Rust extensions. C HBC has no matching editor
or scan UI, so they need a completeness review rather than a claim of C HBC
parity. The optional updater has C source, but is disabled in the supplied C
configuration. All three entries now record partial coverage and required work
in the existing ledger.

## HBC-EXT-01: system network profile editor

Production path: `app.rs::network_config_for_save` ->
`main.rs::SaveNetworkConfig` -> `platform/network_config.rs::save` ->
`platform/nand.rs::replace_file` and `IsfsBackend`.

The editor reads all profile bytes and edits the active profile. It refuses to
save when the active profile is wired or when profile flags are invalid. Load
and recovery check file size, reserved profile flags, and the active-profile
count. Save compares the loaded bytes again after staging and before commit, so
it does not replace a changed configuration. The replacement copies the
system file's owner, group, attributes, and permissions to its staged file.
Wiiload stops before the save and restarts after it. The editor reloads saved
bytes after a successful save and reports both save and restart errors when
both operations fail.

The save-path source findings are closed. `close_after_error` still reports
only the close error when both a load operation and close fail. It is a separate
read-path error and remains open. No live NAND write or IOS network activation
was performed. Verify active wired and wireless profiles, preserved system
file attributes, persisted reload, and network activation on Wii hardware
before treating the editor as hardware verified.

## HBC-EXT-02: wireless network scan

Production path: `main.rs::ScanNetworks` stops the receiver, calls
`platform/network_scan.rs::scan`, then restarts the receiver and reports both
scan and restart failures. `ogc-wd::scan_once` locks the wireless driver, starts
scan mode, submits the real IOS scan, deinitializes, and unlocks. Compared with
C `WD_ScanOnce`/`WD_Scan`, Rust checks results that C discards. Returning the
real driver/cleanup failure is a valid safety difference.

Open findings:

- `parse_security_ie` uses the big-endian descriptor helper for the WPA/RSN
  pairwise count. The information element uses little-endian fields. A normal
  count encoded as `01 00` becomes 256, causing the parser to reject the suite
  list. `detect_security` then falls back to WEP when the privacy capability is
  set. This can label WPA/WPA2 access points as WEP. The primary
  [hostap parser](https://w1.fi/hostapd/devel/wpa__common_8c_source.html)
  uses `WPA_GET_LE16` for version, pairwise count, and authentication count.
- The parser skips version validation and authentication/key-management
  suites. Cipher selection alone cannot distinguish a supported personal
  network from an enterprise or otherwise unsupported network. Malformed or
  unsupported WPA/RSN data must not fall back to an actionable WEP/open label.
- Selecting `Unknown` security replaces the SSID and clears the key but keeps
  the previous encryption value in `app.rs::handle_network_scan`. Reject that
  selection or require explicit supported security before saving.
- Results are merged by SSID only. Access points with the same SSID but
  different security modes can replace one another based on signal strength.
  The UI needs an explicit policy for such conflicts.
- The parser and its security helper are PowerPC-only and have no host
  regression coverage in the current HBC test suite. Extract the shared parser
  for tests with fixed descriptor and WPA/RSN bytes; retain the real IOS backend
  for production. A Wii scan remains required for driver acceptance evidence.

## HBC-EXCLUDE-01: optional updater and installer handoff

C path: `main.c` schedules `update_signal`; `update.c` fetches at most 8 KiB of
metadata, checks the `SIG0` envelope and ECDSA signature over the XML SHA-1,
parses metadata, and compares the release date. After confirmation it downloads
at most 4 MiB, checks the signed metadata's SHA-1, and relocates the installer
with the `-updatehbc` argument.

The supplied `channelapp/config.h` comments out `ENABLE_UPDATES`, sets the URL
to `http://example.com/update.sxml`, and supplies an all-zero public key. The
source README explicitly says the installer is not included. The supplied C
build therefore does not provide a working trusted updater or installer.

Rust implements `metadata.rs::parse_update_xml`, but has no update request
state machine, authenticated metadata verifier, version/availability flow,
download/progress path, or update-specific installer handoff. Existing XML
tests do not prove those missing operations.

This is partial implementation, not a completed feature or safety-equivalent
replacement. Before implementation can be accepted, define the Rust update
authority and version identity, supply the real signing key and service,
specify supported installer/device revisions, and provide the installer and
its backup/commit/recovery contract. Preserve signed input authentication;
do not enable placeholder keys or endpoints. Network transport changes may
improve safety only if the authenticated update operation remains complete.

## Verification limits

The review uses the local C and Rust source and the primary WPA/RSN parser
reference above. Host checks cannot establish wireless scanning, system-file
permissions, IOS network activation, or installer safety on Wii hardware.
Those items remain open in the ledger.

- `cargo test --locked --features startup-diagnostics`: 228 passed, none
  failed or ignored. These tests do not exercise the PowerPC-only scan parser
  or real IOS network configuration writes.
- `python3 tools/parity.py refresh` and `check`: passed after recording all
  three former exclusions as partial work.
- `libogc-rs` source-structure audit: passed, with its three tracked keyboard
  queue notes.
- The full `audit-stubs` completeness check failed with 2,238 API findings,
  including missing exact test/probe evidence. The generated inventory's
  classifications and evidence did not change during this review; only source
  line references were refreshed after the earlier warning cleanup. This audit
  is not a pass and does not establish library completeness.
