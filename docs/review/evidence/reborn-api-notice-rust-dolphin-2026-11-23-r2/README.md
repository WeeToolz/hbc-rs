# Reborn application notice API Dolphin evidence

The `app-notice-api` production acceptance case passed in Dolphin 2609a. It
calls the production `AppController` notice APIs. The result, build manifest,
and raw case log are archived beside this file.

The case checks generic notice text and kind, long-message scroll bounds and
movement, warning and error timeout classification, warning retention at 299
ticks and dismissal at 300 ticks, and the one-shot pointer-input guard. It also
checks that a new pointer event dismisses an error notice after the already
consumed event has been ignored.

Reborn uses `show_message_timed` in `source/dialogs.c`. Its C dialog waits for
elapsed seconds and presents a live button countdown. Rust uses controller
state and a 300-frame timeout for warning and error notices. This run checks
the Rust production state in Dolphin. It does not compare wall-clock timing,
font layout, rendered GX dialog geometry, or physical Wii button behavior.

The first run is preserved in the sibling
`reborn-api-notice-rust-dolphin-2026-11-23` directory. It failed because the
test expected a timed warning to scroll on Down input. The production
controller closes timed notices on input. The corrected case checks scrolling
on a generic notice and checks timeout behavior separately.
