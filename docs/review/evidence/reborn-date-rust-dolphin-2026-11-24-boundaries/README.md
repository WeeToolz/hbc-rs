# Reborn date parser boundary comparison

The independent Reborn C runtime probe and the production Rust metadata APIs
passed in Dolphin 2609a. The comparison covers 27 release-date cases and eight
update-date cases. The release-date cases include year 0, pre-epoch dates, the
year-2038 boundary, and year 9999. The C runtime returned epoch values beyond
the signed 32-bit range and for year 9999. This confirms the configured
64-bit `time_t` behavior in this Dolphin environment.

The C probe copied Reborn's `sscanf` and `mktime` sequence from
`source/xml.c#meta_parse`. Its source SHA-256 is
`cad61bf33fae24cad38297af44a17f1815fff404f431b10dcd7c8f102cab90da`; its DOL
SHA-256 is `1b512d19ae8cbde201114f8bb6b9c6aad25b5ea4ed44d0a64d3ff70c39da998e`.
The C result and unmodified console log are in
`../reborn-date-c-dolphin-2026-11-24-boundaries-r3/`.

The Rust run used a fresh production `emulator-acceptance` DOL. The API result
is in `result.json`; `comparison.json` checks all C normalized values against
the production Rust `metadata-date` case and checks the update API results.
The Rust DOL SHA-256 is
`b18cfe1d22d07184322b6487dd645889f6b1b0d8998b15e8e87098b7396fe920`.

These runs use Dolphin's C runtime and time-zone state. Physical Wii `mktime`
and time-zone behavior remain unverified.
