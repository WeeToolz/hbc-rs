# Reborn C and Rust startup/input smoke captures

Both DOLs completed the same six-stage Pipe-backed emulated Wii Remote input
sequence in Dolphin: center pointer, upper-left, lower-right, button 1, app
details selection, and button 2. Both results record `capture_complete: true`
and exit code 0. Screenshot review confirms pointer interaction and entry to
the app details screen.

The C reference DOL SHA-256 is
`a32c35c20e2a18ba3004aefbcea9ac10af555f0d068241e9332d6840b28fe5f6`. The Rust
release DOL SHA-256 is
`db07edb6aee77475cec552b864b75ca025eef3262481098e559ee38f37073938`.

The Rust console log records settings/theme loading from `30:44.912` to
`30:45.445` on Dolphin's log clock (533 ms). It records the first controller
scan from `30:45.454` to `30:45.523` (69 ms). The C DOL did not emit matching
startup markers in its captured console log, so this run does not compare full
startup duration between C and Rust.

The source correction moves Rust PAD initialization to immediately after its
video setup and before settings/theme reads. It starts WPAD after those reads
and before USB HID ownership, as Reborn does. Host tests check one-time PAD
initialization and WPAD-before-USB ordering. Dolphin confirms that the rebuilt
DOL reaches input and responds to emulated Remote events; it does not prove
physical Bluetooth pairing, real Wii startup time, or hardware WPAD behavior.

The raw console and Dolphin logs are copied without normalization. Each `c/`
and `rust/` folder contains its result JSON, logs, and six captured PNGs.
