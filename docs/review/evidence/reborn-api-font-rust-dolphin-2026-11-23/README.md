# Reborn font API Dolphin evidence

This run calls the production `ThemeFontFace` APIs in the `font-api`
acceptance case. Dolphin reports success in `result.json`; `build-manifest.json`
identifies the tested DOL.

The case checks that the bundled Droid font parses and invalid bytes fail. It
checks ascent, descent, and line gap; compares measured widths for `WWW` and
`iii`; checks empty text width; checks the `A/V` kerning result when available;
checks non-empty raster coverage and glyph cache reuse; and checks measured
memo line layout and scrolling.

This is partial behavior evidence. HBC Reborn uses FreeType. Rust uses
fontdue. The run does not establish raster, metrics, kerning, or text layout
equivalence for all supported theme fonts, font sizes, or text inputs. It does
not prove physical Wii font rendering or performance. Those comparisons remain
open in the font rows of `config/parity-ledger.csv`.

The source revision for this run is recorded in `build-manifest.json`. The raw
acceptance output is in `font-api.log`.
