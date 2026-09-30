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

## GX command emission

The HBC renderer needs a safe boundary around GX FIFO command emission. The
current local implementation provides GX opcode constants and a `begin`
operation. Continue the GX port in Rust and keep direct command emission inside
`ogc-gx`.
