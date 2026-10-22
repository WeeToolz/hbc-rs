# HBC Reborn menu parity: October 17 checkpoint

Reference: `hbc-reborn-master/channel/channelapp/source/home.c` and
`hbc-reborn-master/sdk/hbc_agent/ov_ui.c` (the user-provided source archive).

## HBC main HOME menu

- Adds `Restart network` as the final menu action, with translations.
- Uses the existing `wiiload::stop`/`wiiload::start` path; no duplicate network worker.
- Refuses restart while an upload is active or its payload is waiting to be processed.
- Shows a result notice and reuses the existing network-check timer.
- Keeps BootMii conditional and spaces six buttons within the Wii viewport.

## Application-side HOME strip (opt-in)

- Draws five entries in the order: `DEV`, custom slot 0, `Exit`, custom slot 1
  (`Shot` by default), and `WiiMote`.
- `Exit` has four choices: HBC, System Menu, Restart Wii, Power off.
- `DEV` exposes conditional `Restart app` and `Save` actions.
- `WiiMote` exposes conditional `Find remotes` and `Disconnect remotes` actions.
- Hosts may configure single-button custom slots using `set_slot`.
- Missing host operations are disabled by default. No fake system or controller
  success is reported, and restart/power-off/restart-app require confirmation.
- Controller mapping, rectangle layout, font hook, and request completion are
  shared between software and Wii GX overlay adapters.

## Hardware integration boundary

The app-side strip is an **opt-in library UI**. An application must implement
`home_overlay_host::OverlayHost` and connect it to its own Wii GX/XFB loop,
controller and storage services. The HBC process cannot inject the overlay into
arbitrary independently launched applications. The current host interface is
not yet connected to a production Wii application's presentation lifecycle.

The full Reborn DEV diagnostics, WiiMote IR/battery/configuration pages, app
slot *submenus*, pointer hit testing, and animated transitions have **not** been
ported. Only the basic strip, submenus and request hooks above are present.
Do not claim full UI parity or hardware validation.

## Validation required

1. Build with Rust nightly and the `libogc-rs` sibling checkout.
2. Run Rust unit tests for `home_overlay_app` and `home_overlay_host`.
3. In Dolphin verify six main-menu buttons with/without BootMii, including a
   network restart during active upload (should refuse).
4. In an app that opts into the overlay, verify HOME opens/closes, Exit submenu,
   Shot without SD (should fail safely), action confirmation, and no GX state
   corruption after closing.
5. Test on real Wii only after Dolphin validation; do not install as a system
   channel without separate recovery and installation testing.

This environment does not have Rust/Cargo, so the Rust build/tests are not
claimed to have passed.
