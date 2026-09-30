# Local `libogc-rs` changes

The `libogc-rs/` directory is a complete local copy and is ignored by this
repository. Changes inside it are not included in HBC commits. Record required
changes here so a new checkout can reproduce them in its own local copy.

## USB storage must ignore HID devices

In `crates/services/ogc-usbstorage-sys/src/wii_backend.rs`, USB storage
enumeration must request only `USB_CLASS_MASS_STORAGE`. Do not include
`USB_CLASS_ANY` in the storage candidate query. Filter candidate interfaces for
mass storage before opening a candidate. A HID-only device must not enter USB
mass storage setup. This addresses a likely interaction with the reported HBC
hang when HBC runs from NAND with a HID controller attached and no USB drive.
The target code compiles and a pure selection test covers the class filter, but
the behavior still needs hardware verification.

## Empty network poll must accept a null descriptor pointer

In `crates/services/ogc-network`, `poll` must accept `nfds == 0` with a null
descriptor pointer. It must not form a Rust slice from a null pointer, even for
an empty slice. Use a non-null aligned dangling pointer for the empty slice.
This follows Rust's slice pointer requirements and avoids undefined behavior.

## FAT file and directory access

In `crates/services/ogc-fat`, the HBC port uses Rust-owned `FatFile` and
`FatDirectory` wrappers. They must close their handles on drop and update the
existing open-handle accounting. HBC catalog scanning uses these wrappers.

## GX state and command emission

The HBC renderer needs Rust APIs for GX FIFO command emission and state setup.
The local implementation now provides GX opcode constants, primitive and
vertex FIFO writes, viewport and scissor setup, projection matrix loading, and
display copy setup/presentation (`FrameConfig`, `configure_frame`,
`initialize_display`, `present_frame`, and `finish_frame`). These APIs emit
commands to GX registers in Rust. The target crate passes `cargo check` with
warnings denied. Hardware output still needs validation.

The GX work is incomplete. The display API requires a live GP FIFO and enabled
CPU write-gather pipe. `ogc-gx` does not yet provide the equivalent of
`GX_Init`/`GX_InitFifoBase`/`GX_SetGPFifo`, reset and abort handling, CP and PE
interrupts, or a blocking draw-completion wait. It also does not configure
vertex descriptors and formats, texture coordinate generators, TEV and blend
state, texture and palette upload, or texture-cache invalidation in the form
needed by the HBC sprite renderer. Implement and test these in Rust before HBC
uses the display functions on hardware.
