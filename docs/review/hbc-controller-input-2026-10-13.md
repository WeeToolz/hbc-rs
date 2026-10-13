# HBC controller input update — 2026-10-13

HBC now polls GameCube PAD and Wii Remote input through the production
`ControllerManager::scan_with_raw` path. The manager performs the hardware scan
once and returns copied raw button masks, raw WPAD samples, retained expansion
types for disconnect releases, and the normalized snapshot. HBC uses the raw
masks to keep its C-specific Classic, Nunchuk, Guitar Hero 3, pointer, and
screenshot mappings.

HBC enables USB HID polling through the same manager. USB adapters and USB or
Bluetooth HID sources use the manager's normalized buttons and axes. These are
new HBC defaults, not C parity claims: GameCube uses A Confirm, B Back, X
Options, Y Browse Mode, Start/Home Main Menu, and L/R Previous/Next Page.
Switch Pro and paired Joy-Cons use A Confirm, B Back, X Options, Y Browse Mode,
Minus Previous Page, Plus Next Page, and Home Main Menu. A right Joy-Con uses
A/B for Confirm/Back, X/Y for Options/Browse Mode, Plus for Next Page, and
Home for Main Menu. A left Joy-Con uses SR Confirm, SL Back, Minus/L for
Previous Page, R for Next Page, and Home for Main Menu. The normalized left
stick scrolls vertically; the right Joy-Con right stick supplies its vertical
scroll. HBC logs every reported USB or Bluetooth source error with its full
error detail.

The manager gates raw press edges when a controller connects or reconnects
while a button is held. It keeps the held masks and reports release masks on
disconnect. HBC maps those per-source results before combining them, so a
neutral WPAD cannot clear a GameCube action and pointer ownership still selects
which connected Wii Remote supplies down and held input.

The shared `ogc-ui::KeyRepeat` drives HBC page navigation with the C behavior:
the pressed edge acts immediately, then a held page button acts each frame.
When an input opens or closes a screen, HBC consumes the rest of that frame and
blocks repeat until release and a new press.

The Wii runtime computes the pointer target from the current screen before it
applies input. It applies pointer activation in the same runtime call as
controller input. A pointer click consumes simultaneous Confirm and Minus/Plus
page input, including held page repeat. If controller input changes the screen
first, the runtime skips pointer activation for that frame. This matches the C
main loop, where screen transitions continue to the next frame and browser
pointer activation occurs before Minus/Plus page handling.

Host tests cover the production HBC mask mapper for GameCube and Wii Remote
combination, Classic X/Y and Remote Minus mappings, multiple Wii Remotes,
pointer-owner arbitration, disconnect releases, and screenshot chords. Runtime
regression tests also check simultaneous pointer/page activation and
screen-transition input consumption. Host tests also cover each normalized
controller kind, adapter input, stick scroll, and a quick press plus release
preserved by a reset. When `startup-diagnostics` is enabled, HBC writes each
action returned by the production runtime to the existing diagnostic event
stream before platform dispatch. This records the action result; it does not
record whether a later hardware or storage operation succeeds. The full HBC
host suite and PowerPC custom-target check are required before claiming build
verification. Real Wii behavior remains unverified.
