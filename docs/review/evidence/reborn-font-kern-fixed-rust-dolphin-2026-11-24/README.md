# Custom font width adjustment

The prior [comparison](../reborn-font-kern-comparison-2026-11-24/README.md)
found a one-pixel difference for the custom A/V pair at 24 pixels. Before the
change, the Rust Wii acceptance case confirmed width 26 in Dolphin. The C
FreeType probe reported 27. Its diagnostic reports A advance 960, kerning
-150, and V advance 896 in 26.6 units. The raw C log and diagnostic source are
included here. The date runner stopped that probe after its completion marker;
its date-specific result did not pass.

Rust now rounds the fontdue glyph advance to a whole pixel before adding the
kerning adjustment. This approximates the pixel-aligned advance from Reborn's
FreeType default glyph load. The production Rust font API acceptance case
passed in Dolphin 2609a for the 60 custom-font widths from the C probe and the
existing 60 bundled-font widths. `result.json` and the raw acceptance log record
the run. The custom fixture SHA-256 is
`112915874db9ef06fa7e75648b7a8f3c876d476f55b060e60c62819638d9e016`.

Rounding fontdue advances does not implement FreeType hinting. Other fonts,
kerning pairs, glyphs, and sizes need further comparison. No physical Wii
test was performed. HBC itself was not under test.
