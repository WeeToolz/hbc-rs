# Reborn font width API parity evidence

Reborn's `font_get_string_width` uses the current glyph advance, adds
`FT_KERNING_UNFITTED` to that advance, rounds the result with `(value + 32) >> 6`,
and adds the final glyph advance. It changes a newline to a space when more text
follows and skips a trailing newline and carriage returns.

The Rust implementation previously returned the rightmost fontdue glyph bound.
It did not use Reborn's advance-only width rule. It now sums advances and
kerning, rounds each step to the same whole-pixel result, and applies the same
newline and carriage return rules.

`probes/reborn-font-width/main.c` runs the Reborn FreeType width loop with the
same `assets/droid.ttf` bytes as Rust. DevkitPPC FreeType 2.14.1 and the Rust
production API agree at 16 px for all 10 cases. The C raw Dolphin log records:

| Text | Width |
| --- | ---: |
| `A` | 10 |
| `AV` | 19 |
| `WWW` | 42 |
| `iii` | 12 |
| `Hello, world!` | 89 |
| `gyp` | 25 |
| `M W` | 32 |
| `A\nV` | 23 |
| `A\n` | 10 |
| `A\rV` | 19 |

The Rust production `font-api` acceptance case checks those same values in
Dolphin and passes. The C probe emitted all 10 values and its completion marker.
The probe loops after reporting, so the runner force-stopped Dolphin after the
marker. This is recorded in `c-result.json`; it does not affect the completed
read-only measurements.

The C and Rust runs used Dolphin 2609a x86_64 with private profiles. Neither run
used HBC as the program under test. No HBC control was required. This evidence
covers one font, one size, and the listed strings. Other theme fonts, sizes, full
UTF-8 coverage, complete FreeType metrics, raster output, and Wii hardware remain
open.

See `c-build.manifest.json`, `rust-build.json`, and `result.json` for the source,
artifact hashes, target details, and runner result. The raw C Dolphin log is
preserved in `reborn-font-width-c.log`.
