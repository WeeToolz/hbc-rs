# Reborn frame hit target API Dolphin evidence

The production `render-hit-test` acceptance case passed in Dolphin 2609a. It
calls `Frame::hit_target` to register overlapping app and menu targets, then
calls `Frame::hit_test` at the shared boundary. The topmost registered menu
target is returned. It also checks the right and bottom edges, an out-of-bounds
point, non-finite coordinates, and a browser app target from a production
frame.

The result, build manifest, and raw case log are archived beside this file.
The DOL hash is in `result.json` and `build-manifest.json`.

This is partial API evidence. It does not prove rendered GX hit-map geometry,
live IR coordinates, or physical Wii pointer behavior. The Rust API produces
hit targets for the existing Reborn pointer selection behavior in
`source/view.c`.
