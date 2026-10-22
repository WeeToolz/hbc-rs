# HOME overlay host bridge checkpoint

`src/home_overlay_host.rs` adds an application-owned `HomeOverlaySession` that receives the existing `InputFrame`. The host supplies a safe copied RGBA frame, shared text rendering, frame presentation, and result-checked operations. UI control stays in `AppHomeOverlay`; no new controller mapper, font rasterizer, or filesystem implementation was added.

## Host integration requirements

1. Only call `frame` on an application-owned render thread, not an interrupt or hardware callback.
2. Copy the finished frame into a separate RGBA8888 buffer with valid stride; do not mutate a live GX-owned XFB/EFB.
3. Use the application's existing font renderer and present routine. Restore prior GX state in the host, including failure paths.
4. Never call `execute` for power/reset or filesystem operations from inside rendering. Confirm safe cleanup and completion before returning success.
5. Screenshot and save operations must check usable storage immediately before opening a file; failures must not crash or leave partial files.
6. An unrelated launched application cannot receive this overlay automatically. That app must explicitly integrate the host bridge.

## Validation

- `git diff --check` passed.
- Rust compiler and Wii integration were not available in this environment; no successful build or Wii hardware test is claimed.
- The host adapter is a contract, **not** a finished Wii video/GX driver.
