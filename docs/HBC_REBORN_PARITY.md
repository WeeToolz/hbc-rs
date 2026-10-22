# HBC Reborn parity


## HOME overlay integration checkpoint

- Shared overlay geometry now drives both the software RGBA compositor and the Wii GX chrome renderer.
- Screenshot actions execute before overlay presentation, so the capture path sees the underlying application frame.
- Host action failures are returned to callers and leave the overlay open for recovery.
- Restart and power-off require a second confirm press as an intentional safety difference from C HBC Reborn. Navigation or closing the menu cancels confirmation.
- Wii GX text still comes from the application font backend; no second font rasterizer was added.
- Real launched-application integration still requires the application to call the host bridge from its own input/render loop. HBC cannot inject this into unrelated applications after transfer of control.

- Added a synchronous `HomeOverlaySession::run` entry point matching Reborn's app-thread `hbc_agent_home()` execution model.
- Added a one-shot `OpenSignal` for remote/developer-triggered overlay requests without fabricating controller input.
- HBC's normal screenshot chord and the overlay now share `capture_and_save_screenshot`, including the early no-storage check.
