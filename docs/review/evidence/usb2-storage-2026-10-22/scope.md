# Independent USB volume scope

Compare Reborn's two `usbmsd` drive contexts with the production Rust USB
backend, FAT mount sources, HBC volume scan, settings, and status display.
The shared DOL loader stays unchanged. All existing settings indices stay valid.
The added USB slot is discovery order, not a fixed physical port number.

No physical Wii upload, NAND write, USB passthrough, or media write is part of
this run. Dolphin SD tests cover startup and the common DOL launch path.
A real-media probe is provided, but physical USB acceptance stays pending.
