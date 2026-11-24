# Custom font kerning size sweep

The C FreeType probe used the existing derived Droid fixture with one A/V
kerning pair at -200 font units. In Dolphin 2609a it measured `AV`, `AVA`,
`AVAV`, and `A\rV` for every size from 7 through 128 pixels. It emitted all
488 values and its completion marker. The production Rust font API matched
all values on the host and in the Wii acceptance case in Dolphin.

The C DOL SHA-256 is `76e160e69301daa73ea8b6e141d64967739b7f0714acc96a2b5dc19389b36a84`.
The expected-width fixture SHA-256 is `93e76cf7e60d3ca05a25397b8ef95d759ae95edb92b0e295e0637aa49755d7bb`.
The raw C log SHA-256 is `d2a17eba42c905f539ea94d5c751f1f22babfa2e4b713019f8c1c11e322b894a`.
The Rust DOL SHA-256 and result are in `result.json`. Widths are stored as
big-endian 16-bit values in size and sample order.

The date-probe runner reports failure because it expects date markers. The C
font completion marker and all results appeared before it stopped Dolphin.
Both runs used private profiles. HBC itself was not under test. No physical
Wii test was performed. Other kerning pairs, custom fonts, glyph rendering,
and FreeType hinting remain open.
