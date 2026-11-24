# Bundled font ASCII width sweep

The C FreeType probe measured printable ASCII characters 32 through 126 at
all integer sizes from 7 through 40 pixels in Dolphin 2609a. It emitted all
3,230 values and its completion marker. The production Rust font API matched
all values on the host and in the Wii acceptance case in Dolphin. The fixture
stores C widths in ascending size and character order.

The C DOL SHA-256 is `04ad20d17c44b8ab8f4f39dd1e3f13a9046c2408855aaec797bc38259ccd84c9`.
The fixture SHA-256 is `be8b38b5ac453697ed3d24b26c3ce148e845224972d2cd04a8f7c313108aadaa`.
The raw C log SHA-256 is `8ec191ddd7201399fa7f9800d1ed37902f60dc4ef5570c33e0b4458012e94fff`.
The Rust DOL SHA-256 and acceptance result are in `result.json`.

The existing date-probe runner reports failure because it expects date
markers. The raw C log shows the font completion marker before the runner
stopped Dolphin. Both runs used private profiles. HBC itself was not under
test. No physical Wii test was performed.

This covers the bundled Droid font, individual printable ASCII characters,
and these sizes. Other fonts, multi-character kerning, Unicode, other sizes,
FreeType hinting, raster output, and physical Wii behavior remain open.
