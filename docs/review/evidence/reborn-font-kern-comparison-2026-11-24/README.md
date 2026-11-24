# Custom font kerning comparison

A standalone C probe in Dolphin 2609a compared the Reborn FreeType width
algorithm with the production Rust font API running on the host. Both used a
temporary copy of `assets/droid.ttf` with one legacy `kern` table entry: A/V
at -200 font units. `make-fixture.py` records how the fixture was made. Its
SHA-256 is `112915874db9ef06fa7e75648b7a8f3c876d476f55b060e60c62819638d9e016`.
The probe source is `probes/reborn-font-width/main.c`; the C DOL SHA-256 is
`7f012d0265a293d55020eea6c9e8a548206da648db3d3736f3a5c676f336c00a`.

FreeType reported `has_kerning=1` and completed 60 measurements. Fifty-eight
matched Rust. At 24 pixels, C reported width 27 for `AV` and `A\rV`; Rust
reported 26 for both. The captured Rust values use the production API on the
host, so repeat the Rust side in Dolphin and trace FreeType and fontdue rounding
before choosing a production change. The raw C log and Rust output are here.

The existing date-probe runner reports failure because it expects date markers.
It stopped Dolphin after the font completion marker and all 60 results appeared.
The C run used a private profile; HBC was not under test. No physical Wii test
was performed. Other custom fonts and kerning pairs remain open.
