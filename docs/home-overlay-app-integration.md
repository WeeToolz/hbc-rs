# HOME overlay: app-side integration checkpoint

The reusable `AppHomeOverlay` has a frame-based input adapter and an RGBA8888
software menu compositor. Use `tick` on controller press edges and pass the HOME
held state. Release the HOME key before dispatching further commands. Call
`complete` after each requested operation; report `Failed` when storage is
missing, screenshot capture fails, or an exit request fails.

**Wii-specific integration is not finished.** A launched application must link
this module or a shared SDK and explicitly give it access to a safely copied
framebuffer and decoded controller input. HBC itself cannot force the overlay
into an independent application after launch. This checkpoint does not modify
GX state or register Wii system hooks. The renderer draws geometry only; it
does not render labeled text. The existing UI font renderer should provide the
labels rather than introducing a second font engine.

A live Wii adapter must preserve the application's framebuffer, video mode,
controller state, and interrupts, and restore them on overlay close. Do not
assume RGBA8888 is the native GX/XFB format: convert to RGBA8888 in an
application-owned buffer before calling `draw`. Do not overwrite live video
memory and do not issue a real exit/power request without application approval.

Host-side tests were added for HOME edge suppression, duplicate actions,
failed screenshot operations and invalid buffers. Rust tooling was unavailable
in this environment, so these tests have not run. No Wii DOL was produced.
