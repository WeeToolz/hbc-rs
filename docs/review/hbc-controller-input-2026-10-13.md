# HBC controller input update — 2026-10-13

HBC now polls GameCube PAD and Wii Remote input through the production
`ControllerManager::scan_with_raw` path. The manager performs the hardware scan
once and returns copied raw button masks, raw WPAD samples, retained expansion
types for disconnect releases, and the normalized snapshot. HBC uses the raw
masks to keep its C-specific Classic, Nunchuk, Guitar Hero 3, pointer, and
screenshot mappings.

The manager gates raw press edges when a controller connects or reconnects
while a button is held. It keeps the held masks and reports release masks on
disconnect. HBC maps those per-source results before combining them, so a
neutral WPAD cannot clear a GameCube action and pointer ownership still selects
which connected Wii Remote supplies down and held input.

The shared `ogc-ui::KeyRepeat` drives HBC page navigation with the C behavior:
the pressed edge acts immediately, then a held page button acts each frame.
When an input opens or closes a screen, HBC consumes the rest of that frame and
blocks repeat until release and a new press.

Host tests cover the production HBC mask mapper for GameCube and Wii Remote
combination, Classic X/Y and Remote Minus mappings, multiple Wii Remotes,
pointer-owner arbitration, disconnect releases, and screenshot chords. The
full HBC host suite, host Clippy, and the PowerPC custom-target check pass.
Real Wii behavior remains unverified.
