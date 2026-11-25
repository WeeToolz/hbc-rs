# HBCC empty-state Dolphin evidence

The Rust HBC and Reborn C control passed the empty crash-state check in
Dolphin on 2026-11-25. Both HBCS replies reported `crash: null`. Both accepted
the all-zero `HBCC` clear request with an empty success reply, and HBCS still
reported `crash: null` afterwards.

The Rust result is in `result.json`; its DOL SHA-256 is
`47ab7b49f39dd9515b3e615f5e7e6f2069052933876d189df33ba62916169131`.
The C result is in
[`../reborn-hbcc-empty-c-control-dolphin-2026-11-25/result.json`](../reborn-hbcc-empty-c-control-dolphin-2026-11-25/result.json).
Its DOL SHA-256 is
`a32c35c20e2a18ba3004aefbcea9ac10af555f0d068241e9332d6840b28fe5f6`.

Host tests check the v2 and legacy v1 record layouts and checksums, JSON field
format, and the wire distinction between Reborn's all-zero clear request and
the Rust versioned HBCC remote-command extension. A valid crash record has
not been supplied to HBC in Dolphin or on a Wii. Real Wii memory and cache
behavior remain unverified.
