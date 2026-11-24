# Reborn About API Dolphin evidence

The `about-api` production acceptance case passed in Dolphin 2609a. It calls
the production About scroll APIs. The result, DOL manifest, and raw case log
are archived beside this file.

The case checks that the credits stay at the start for 120 ticks, advance one
line at 123 ticks, respond to manual Down input, stop at the end, and reduce
the maximum scroll by one line when the theme description is removed.

Reborn builds the About memo in `source/dialogs.c` and advances it in
`source/main.c` after 120 frames, every third frame. This run checks the Rust
controller state in Dolphin. It does not compare live widget timing, font
metrics, GX rendering, or physical Wii input.
