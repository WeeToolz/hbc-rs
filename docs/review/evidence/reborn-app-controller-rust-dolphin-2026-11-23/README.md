# Reborn browser controller APIs in Dolphin

The `app-controller` production acceptance case passed in Dolphin. It calls
`AppController::handle`, `page_navigation`, `visible_page`, and
`browser_layout` with a seven-entry catalog. It checks five-row list paging,
next and previous actions, stopping at the page edge, switching to a three-column
grid, and moving down one grid row while keeping the selected app.

Reborn's matching paths are `browser.c#browser_gen_view` for page creation and
`browser.c#browser_switch_mode` for list/grid changes. Grid focus movement uses
`browser.c#browser_set_focus` and the shared view focus functions. This case
checks controller state only; it does not inspect rendered widget geometry,
transition animation, real controller timing, or physical Wii behavior.

Build and run the acceptance case with:

```sh
cd ../libogc-rs
cargo xtask build-dol hbc-rs --bin emulator-acceptance \
  --features emulator-acceptance --manifest-path ../hbc-rs/Cargo.toml \
  --output ../hbc-rs/target/hbc-rs-app-controller-acceptance.dol
cd ../hbc-rs
python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-app-controller-acceptance.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-app-controller-rust-dolphin-2026-11-23 \
  --cases app-controller
```
