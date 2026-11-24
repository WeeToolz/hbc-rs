# Corrected Reborn font width probe

On 2026-11-24, the standalone C probe ran in Dolphin 2609a under x86_64
with a private profile. It used the bundled Droid font and the FreeType
2.14.1 library from devkitPro. FreeType reported `has_kerning=0`. The probe
completed all 60 measurements. Each width matches the corresponding Rust
production result in the [width matrix](../reborn-api-font-width-matrix-rust-dolphin-2026-11-24/README.md).
The C probe adds the previous glyph advance for each character, including when
FreeType reports no kerning, as Reborn does.

The C source SHA-256 is `8920731590d380defe449dc0f85da235607561bc3f781cd72a94402b753f1448`.
The DOL SHA-256 is `9810f3f6b966e823a5a4576845eec8fd77dc2a75be78befda78b7c84f1267b90`.
The font SHA-256 is `f51b88945f4c1b236f44b8d55a2d304316869127e95248c435c23f1e4142a7db`.
The raw console log SHA-256 is `f4888954b9d633024d2cec2b56c8233ee892516b933e874a88b38b60253b7a19`.

The existing date-probe runner was used to capture the console. Its result
reports failure because it expects date markers, not font markers. After its
20-second timeout it force-stopped Dolphin. The raw log contains the font
completion marker and all 60 measurements before the stop. HBC itself was not
under test. No physical Wii test was performed. Custom fonts with kerning,
other strings and sizes, font raster output, and Wii behavior remain open.
