# Bundled font width rounding in Dolphin

The [ASCII comparison](../reborn-font-ascii-comparison-2026-11-24/README.md)
found five widths that differed by one pixel. A C FreeType diagnostic in
Dolphin reported unhinted advances of 544, 1120, 736, 544, and 544 in 26.6
units for those cases. These are half-pixel boundaries. Its hinted advances
were 576, 1152, 768, 576, and 576. Fontdue's unrounded values are slightly below
the half-pixel boundaries. Rounding to 26.6 units before pixel alignment makes
Rust match these C results.

The production Rust `font-api` case passed in Dolphin 2609a with all 570
printable ASCII expected widths from the C probe, plus the existing bundled
and custom-font width cases. The original fixture contained the C results in size and character order.
It was later expanded in the [ASCII sweep](../reborn-font-ascii-sweep-rust-dolphin-2026-11-24/README.md). A host comparison also matched all 570 values.
The run result and raw log are included. The C diagnostic DOL SHA-256 is
`77811c3c5176e3db40d6654a7410431db994367e6cf54e7829690ff5b0888ceb`.
Its source and raw output are included. The date runner reports failure because
it expects date markers; the font diagnostic completed before it stopped
Dolphin. Both runs used private profiles. HBC itself was not under test.

FreeType hinting is not implemented by fontdue. Other fonts, characters,
sizes, kerning values, and physical Wii behavior remain open.
