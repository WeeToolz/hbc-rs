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

USB v5 watcher startup now waits cooperatively for at most 100 ms per detected
host. It polls IPC and sleeps in 1 ms LWP timebase increments instead of busy
spinning. A missing watcher snapshot does not fail USB initialization. With
both vendor and HID hosts detected, this wait can add up to 200 ms. This timeout
does not cover synchronous IOS open/ioctl calls made while starting a host, and
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
The local copy also provides create, truncate, write, flush, stat, directory
creation, file removal, and empty-directory removal. Mutating FAT operations
flush metadata and transport buffers. The Wii target crate check passes with
warnings denied; target unit tests cannot run because the custom no-std target
does not provide Rust's `test` crate.

## GX state and command emission

The HBC renderer needs Rust APIs for GX FIFO command emission and state setup.
The local implementation now provides GX opcode constants, primitive and
vertex FIFO writes, viewport and scissor setup, projection matrix loading, and
display copy setup/presentation (`FrameConfig`, `configure_frame`,
`initialize_display`, `present_frame`, and `finish_frame`). These APIs emit
commands to GX registers in Rust. `GxFifo::initialize_default_mem2` reserves a
bounded 256 KiB command ring from MEM2 and sets the PI FIFO, CP FIFO, WPAR, and
HID2 write-gather state. `configure_solid_2d` sets direct XYZ/F32 and RGBA8
vertex inputs, one vertex color channel, a pass-color TEV stage, and alpha
blending. `draw_solid_rect` emits a four-vertex solid rectangle. The target
crate passes `cargo check` with warnings denied. Hardware output still needs
validation.

The GX work is incomplete. The FIFO setup is a minimal single-owner path. It
does not install CP or PE interrupt handlers, provide overflow/underflow
recovery, reset and abort handling, or wait for draw completion. HBC sprite and
text rendering also needs vertex descriptor and format commands, texture
coordinate generation, TEV and blend state, texture and palette upload, and
texture-cache invalidation. Treat FIFO flow control as unverified until it has
hardware testing. Do not use the display functions before FIFO initialization.
The rectangle path does not load textures or support HBC sprite, icon, or text
rendering.

`crates/hardware/ogc-gx/src/lib.rs` `init_tex_obj` must return without changing
the texture object when either input dimension is zero. GX texture dimensions
are encoded as `dimension - 1`; without this guard, zero underflows and creates
an invalid packed size. A focused regression test checks zero width and zero
height and confirms the initialized object remains unchanged. This test cannot
run on the host because the crate and its dependencies require the Wii target.
