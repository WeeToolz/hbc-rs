# HBCL empty-record Dolphin evidence

The Rust HBC and the matching Reborn C control passed the empty retained-log
check in Dolphin on 2026-11-25. Each run checked HBCS `lastlog: null`, then
sent HBCL and verified status `-ENOENT` with a zero-byte response body.

The Rust result is in `result.json`. It records DOL SHA-256
`07acc1cdf10bd118b9b03b5591573aee2db93c2fc92e7eaf5b547122f47335a0`.
The C result is in
[`../reborn-hbcl-empty-c-control-dolphin-2026-11-25/result.json`](../reborn-hbcl-empty-c-control-dolphin-2026-11-25/result.json).
Its DOL SHA-256 is
`a32c35c20e2a18ba3004aefbcea9ac10af555f0d068241e9332d6840b28fe5f6`.

Rust host tests check the C record layout and checksum, response text, corrupt
checksums, oversized text length, and app-name sanitation. This Dolphin run
has no valid retained record. A positive retained-log response and HBCN app
launch/return handoff still need an end-to-end run. Physical Wii memory and
cache behavior also remain unverified.
