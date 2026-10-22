# HOME overlay controller and renderer integration checkpoint

The shared HOME menu now consumes the existing `platform::input::InputFrame` event model, so it does not decode Wii Remote or GameCube controller bit masks a second time. `tick_input_frame` only reports a `FrameAction`; the caller must execute any requested filesystem or system operation and report success or failure through `complete`.

`draw` composites bounded RGBA pixels onto a caller-owned buffer. `draw_labels` calls an application-supplied `OverlayText` font renderer; this avoids a duplicate glyph rasterizer. Only a caller that owns a stable RGBA frame should use `draw`. Wii XFB frames use a different pixel format. Do not treat the XFB as RGBA or write directly to GX-owned display memory.

## Still required for Wii application-side use

- Integrate the adapter **inside the launched application**, not only in HBC. HBC cannot intercept controller input or display frames after control transfers to another executable.
- Provide a supported controller polling loop and feed its mapped `InputFrame` to `tick_input_frame`.
- Copy or capture a completed application-owned frame, render the overlay via a supported GX drawing path or correctly convert RGBA to the application's framebuffer format, and restore drawing state.
- Route screenshot and save requests through the application's existing storage code, including no-volume and failed-write handling. Do not invoke irreversible system actions without validating them.
- Integrate `OverlayText` with the existing cached theme font renderer, and verify layout on the actual console.
- Compile and test host unit tests and Wii target before claiming complete runtime parity.

No hardware hook, framebuffer ownership transfer, or overlay injection has been added by this checkpoint. No runnable Wii DOL is claimed.
