# Reborn C received-theme control in Dolphin

The Reborn C HBC DOL identified in `result.json` received the same
`tests/fixtures/theme-valid.zip` as the Rust test over Wiiload in Dolphin
2609a. Its status response reported upload sequence 1, result `theme`, and no
error without controller input. The DOL hash matches the prior
[reviewed C input control](../ui-input-c-reference-dolphin-2026-10-23-r4/result.json).
The Rust run and fixture hash are in the [Rust record](../reborn-wiiload-theme-rust-dolphin-2026-11-24/README.md).

Both runs used the same minimal NAND source and separate private copies. SD
emulation was disabled; the unused cloned SD images were different. Both
runs used x86_64 Dolphin. This control checks Reborn's reported upload result;
it does not compare rendered pixels or persistent theme bytes between C and
Rust. Physical Wii behavior remains open.

The first control attempt failed because the runner treated a null upload
status as an object. The second received a `theme` result but could not write
its result file when temporary storage filled. Their terminal outputs are
included. The final run completed and wrote `result.json` after cleanup.
