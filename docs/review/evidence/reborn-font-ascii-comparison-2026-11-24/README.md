# Bundled font ASCII width comparison

The C probe uses Reborn's FreeType glyph loading and width calculation with
`assets/droid.ttf` in Dolphin 2609a. It emitted a completion marker and 570
records: printable ASCII characters 32 through 126 at 8, 12, 16, 18, 24, and
32 pixels. The Rust production API on the host matched 565 of the records.

| Size | Character | C width | Rust width |
| ---: | :---: | ---: | ---: |
| 16 | `a` | 9 | 8 |
| 24 | `N` | 18 | 17 |
| 24 | `v` | 12 | 11 |
| 24 | `{` | 9 | 8 |
| 24 | `}` | 9 | 8 |

A Rust Wii acceptance build with the diagnostic patch in this directory
confirmed all five Rust values in Dolphin. Its `font-api` result passed. The
patch is evidence of the diagnostic only; it is not a parity requirement.

C source, raw log, host probe and output, and both runner results are included.
The C DOL SHA-256 is `0fd5ffca73b835e2e72434d3e4b018c083d0c606470a10cfc7e04ba7bd5e058f`.
The Rust DOL SHA-256 is in `rust-result.json`. The font SHA-256 is
`f51b88945f4c1b236f44b8d55a2d304316869127e95248c435c23f1e4142a7db`.
The C date runner reports failure because it expects date markers; it stopped
Dolphin after the font completion marker. Both runs used private profiles.
HBC itself was not under test. No physical Wii test was performed.

FreeType's hinted glyph advances and fontdue's rounded advances differ for
these characters. Further work must preserve real custom font support and
compare more fonts and sizes. Do not treat the previous 120 matching samples
as complete width parity.
