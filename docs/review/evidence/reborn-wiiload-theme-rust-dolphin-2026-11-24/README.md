# Received theme ZIP in Dolphin

Reborn `main.c` handles `LT_ZIP_THEME` by calling `refresh_theme` directly.
Rust now applies a validated received theme through its production theme path
without the app-ZIP confirmation. Both selected and received themes save the
archive before releasing the old texture set and loading replacements.

The production Rust application received `tests/fixtures/theme-valid.zip` over
TCP in Dolphin 2609a. The final run used a minimal private NAND fixture and a
cloned private SD image. It reported successful theme application and the test
read the saved archive back for exact byte equality. Existing HBCI checks also
passed. The fixture SHA-256 is
`69079a286de6852be700cf6e19271218ccb7ee71f366763ea61e28f6ec140d26`.
The DOL hash is in `result.json`.

The baseline run received the archive but did not apply it before the test
deadline. A first implementation then failed to allocate replacement textures;
its result and raw log are included. Another attempt ran out of disk space
before network startup; its terminal log is included. The final run passed
after both theme paths used the existing release-before-replacement order.

The Reborn behavior was reviewed in source. A matching C HBC control run was
not performed, so this is not a complete C-versus-Rust runtime comparison.
Physical Wii NAND behavior, memory headroom, and rendered theme parity remain
open. All writes in this test used private Dolphin data.
