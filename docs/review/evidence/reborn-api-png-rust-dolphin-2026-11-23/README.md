# PNG APIs in Dolphin

The `png-api` production acceptance case passed in Dolphin. It called the
public encoder with fixed RGBA pixels, decoded the result, and checked the
exact pixel values. It also decoded the fixed HBC icon fixture, checked a
one-pixel RGBA to GX conversion against fixed tile bytes, and checked the
owned texture dimensions, bytes, pointer alignment, length, and empty state.

The build manifest records the source and SDK revisions and the DOL hash. The
PNG implementation follows Reborn's `source/tex.c#tex_from_png` and
`save_rgba_png` paths. Rust writes uncompressed DEFLATE blocks while Reborn
uses best-compression libpng; the check compares decoded pixels, not identical
PNG files.

This is partial API evidence. It does not check every PNG color mode,
interlacing case, malformed chunk, or GX image size. Dolphin does not prove
physical cache behavior, GX sampling, or screenshot file writes on Wii hardware.
