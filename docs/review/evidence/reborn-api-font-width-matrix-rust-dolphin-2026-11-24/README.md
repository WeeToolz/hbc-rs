# Reborn font width matrix evidence

This extends the 16 px comparison in the prior font width evidence. Reborn's
`font_get_string_width` uses FreeType glyph advances, adds `FT_KERNING_UNFITTED`
to each pending advance when `FT_HAS_KERNING` is true, rounds with
`(value + 32) >> 6`, and adds the final advance. It changes a newline to a space when more text follows and skips a
trailing newline and carriage returns.

The devkitPPC C probe in `probes/reborn-font-width/main.c` and the production
Rust `font-api` acceptance case use the same `assets/droid.ttf`. The captured devkitPPC FreeType probe and the production Rust API agree on 60
values: ten text cases at 8, 12, 16, 18, 24, and 32 pixels. The text cases are
`A`, `AV`, `WWW`, `iii`, `Hello, world!`, `gyp`, `M W`, `A\nV`, `A\n`, and `A\rV`.

| Pixels | Widths in the text case order |
| ---: | --- |
| 8 | 5, 10, 21, 6, 46, 13, 16, 12, 5, 10 |
| 12 | 7, 14, 33, 9, 67, 19, 24, 17, 7, 14 |
| 16 | 10, 19, 42, 12, 89, 25, 32, 23, 10, 19 |
| 18 | 11, 21, 48, 15, 104, 29, 37, 26, 11, 21 |
| 24 | 15, 29, 63, 18, 136, 38, 48, 35, 15, 29 |
| 32 | 19, 37, 84, 24, 180, 52, 64, 45, 19, 37 |

The C probe emitted all 60 values and its completion marker in Dolphin 2609a.
The runner then force-stopped the probe, which loops after reporting. The stop
happened after completion; see `c-result.json` and the raw C log. The Rust
production `font-api` acceptance case checks the same 60 expected values and
passed in a separate Dolphin run. Each run used a private profile. HBC was not
the program under test, so no C HBC control was needed.

The captured C run used an earlier probe. A [later corrected probe](../reborn-font-width-corrected-c-dolphin-2026-11-24/README.md) reports no kerning for the bundled font and matches all 60 widths. Kerning for custom fonts remains open.

This evidence covers the bundled Droid font at these six sizes and these ten
strings in the captured C and Rust runs. Other theme fonts, sizes, full UTF-8
coverage, FreeType line metrics, raster output, and physical Wii behavior remain
open.

The C and Rust artifact hashes and build settings are in
`c-build.manifest.json` and `rust-build.json`. `reborn-font-width-c.log` is the
raw C Dolphin log; it is preserved without edits.
