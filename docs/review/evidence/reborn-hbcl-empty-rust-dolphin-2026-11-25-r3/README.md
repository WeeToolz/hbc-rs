# HBCL empty-record Dolphin evidence

The Rust HBC and the matching Reborn C control passed the empty retained-log
check in Dolphin on 2026-11-25. Each run checked HBCS `lastlog: null`, then
sent HBCL and verified status `-ENOENT` with a zero-byte response body.

The final Rust result is in `result.json`. It records DOL SHA-256
`47ab7b49f39dd9515b3e615f5e7e6f2069052933876d189df33ba62916169131`.
The matching C result is in
[`../reborn-hbcl-empty-c-control-dolphin-2026-11-25/result.json`](../reborn-hbcl-empty-c-control-dolphin-2026-11-25/result.json).
Its DOL SHA-256 is
`a32c35c20e2a18ba3004aefbcea9ac10af555f0d068241e9332d6840b28fe5f6`.

Host tests check the C record layout and checksum, response text, corrupt
checksums, oversized text length, and app-name sanitation. A positive
retained-log response and HBCN app launch/return handoff still need an
end-to-end run. Physical Wii memory and cache behavior also remain unverified.
