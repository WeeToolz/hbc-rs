# HOME overlay multi-priority checkpoint

Implemented in this checkpoint:

- one shared rectangle layout for the software RGBA compositor and Wii GX renderer;
- screenshot actions execute before overlay presentation so the overlay is not captured;
- host action failures are returned to the caller and keep the menu open;
- restart and power-off require a second confirmation press, and navigation cancels it;
- one-shot remote/developer open signaling without fabricated controller input;
- synchronous app-thread overlay loop matching the `hbc_agent_home()` execution pattern;
- one shared production screenshot pipeline for normal HBC screenshots and overlay hosts;
- early storage validation before EFB readback and PNG allocation.

Open hardware work:

- application-owned Wii framebuffer borrowing equivalent to `hbc_agent_home_fb()`;
- Wii Remote IR enable/restore and per-app controller state restoration;
- developer menu, custom slots, remote settings, speaker/rumble features and status reporting;
- real Wii test of overlay open/close, screenshot failure without SD, restart/power-off confirmation, and graphics-state restoration.

Validation in this environment:

- `git diff --check` passed;
- changed overlay files passed structural delimiter checks;
- no placeholder markers were found in production overlay code;
- Rust compilation was not run because this environment has no Rust toolchain.
