# Bundled font Latin-1 glyph widths

The C FreeType probe loaded single Unicode code points U+00A0 through U+00FF
at sizes 8, 12, 16, 18, 24, and 32 pixels in Dolphin 2609a. It emitted 576
widths and a completion marker. The Rust production font API matched all 576
on the host and in the Wii acceptance case in Dolphin. The fixture stores the
C widths in size and code point order.

The C DOL SHA-256 is `dfb8ffee46c3e08d31957cec169ee407ed04ca0cffec189cd94bd29355f30370`.
The fixture SHA-256 is `990813d502a86945b6cca2e5de5315797abb07117aad84bb9bff47b66ba2184d`.
The raw C log SHA-256 is `bceb68e4fd26c65773e39b95565e1b74a6b82644807d8a6b17089bc5c767f44c`.
The Rust DOL SHA-256 and result are in `result.json`.

The date-probe runner reports failure because it expects date markers. The raw
C log contains all font records and the completion marker before the runner
stopped Dolphin. Both runs used private profiles. HBC itself was not under
test. No physical Wii test was performed.

The C probe calls FreeType with decoded code points. It does not check C UTF-8
decoding, multi-character layout, rendering, other fonts, or other sizes.
