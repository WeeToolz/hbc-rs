# Local `libogc-rs` changes

The `libogc-rs/` directory is a complete local copy and is ignored by this
repository. Changes inside it are not included in HBC commits. Record required
changes here so a new checkout can reproduce them in its own local copy.

## HBC application handoff exception restoration

The local libogc-rs baseline must include commit `dfe5b3e` or equivalent.
`ogc-sys` snapshots the original decrementer vector before LWP installs its
Rust deadline handler, and `ogc-irq::restore_runtime_exception_vectors()`
restores both the external IRQ vector and that decrementer vector before HBC
branches to a loaded application. This matches the core runtime portion of
C HBC's `__exception_closeall()` handoff without making HBC write exception
slots directly. Debugger-owned general exception vectors remain owned by the
debugger rather than by the core runtime restore API.

## PowerPC floating-point startup

In `crates/abi/ogc-sys/src/lib.rs`, native `_start` must set `MSR_FP` while
preserving the other MSR bits before it calls Rust code. DOL entry does not
guarantee that the loader enabled the FPU, and normal Rust `f32` operations
can otherwise raise a program exception. Use a nonzero GPR such as `r12` to
read, update, and write the MSR. The `ori` instruction uses `r0` as a normal
source register; instructions such as `addi` have special behavior for `r0`.

For startup tracing, `ogc-gx::configure_frame_with_progress` marks viewport
calculation before and after its floating-point operations, then marks the
viewport opcode, address, and each data word as it writes the GX FIFO. These
labels distinguish a floating-point exception from a stuck FIFO write.

## Restore the floating point enable bit after IPC and cache locks

In `crates/core/ogc-ipc/src/lib.rs` and `crates/core/ogc-cache/src/lib.rs`,
`lock_by_msr` must save both `MSR_EE` and `MSR_FP`, because it clears both bits.
`unlock_by_msr` must clear both bits from the current MSR and restore only the
saved values. The old code restored `MSR_EE` but left `MSR_FP` disabled after
each lock. HBC configures video aspect through IPC before it uses GX floating
point commands, so that bug stopped the startup diagnostic in GX frame setup.
Both crates use the shared mask helpers in `crates/abi/ogc-sys/src/lib.rs`;
the host regression test checks enabled and disabled states and preserves all
unrelated MSR bits.

`crates/core/ogc-system-sys/src/lib.rs` must also preserve both `MSR_EE` and
`MSR_FP` in its SRAM lock. The lock masks both bits, so the unlock must restore
both through the shared MSR helpers. The previous unlock restored only
`MSR_EE`; `sys_init` then returned with floating point disabled after its GBS
mode SRAM access. This stopped HBC at its first GX viewport calculation.

## Tetris example video startup

`examples/tetris.rs` must call `system::init()` before it allocates framebuffers.
It must check for a missing video mode before entering the game loop. It follows
the working `snake-gc` example by enabling USB HID without starting Bluetooth
HID. Its neutral YCbYCr colors use a dark gray background and lighter panels,
walls, and text for clear contrast.

## ELF-to-DOL conversion

Use `cargo xtask elf-to-dol <input.elf> <output.dol>` from the copied
`libogc-rs/` workspace. Its `Libogc` DOL profile omits the ELF `PT_LOAD`
metadata segment below the executable image. The separate `elf2dol` command
uses the legacy profile. That profile included the HBC ELF metadata segment at
`0x80000000` as a DOL data section. A Wii homebrew loader rejected that DOL as
an invalid Wii application. The regenerated HBC and Tetris DOLs have no section
at that address; their first load section begins at `0x80004000`.

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

GX vertex setup must write `INVTXSPEC` at XF register `0x1008` to match the CP
vertex descriptor. Color 0 uses value `0x01`; color 0 plus direct TEX0 uses
`0x11`. `XF_NUMCOLORS` at register `0x1009` is a separate output setting.
Dolphin reported that the prior setup sent one CP color and zero XF colors, and
the textured path sent one CP texture coordinate and zero XF coordinates. It
warns that this mismatch can hang real hardware. A regression test checks both
vertex-spec values.

The solid renderer must disable vertex lighting and use the direct vertex
color. Its channel-control word now matches libogc's unlit `GX_SetChanCtrl`
setup. A regression test checks this word. This correction did not resolve the
black XFB result in Dolphin, so it is a confirmed register-packing fix but not
the confirmed cause of the black screen.

GX FIFO startup now writes the basic revision, indirect-mask, and Wii TMEM
register defaults that libogc sends from `GX_Init`. HBC's mode-specific AA
sample pattern, vertical filter, field-rendering mode, and half-aspect setting
now flow through `FrameConfig` to the GX setup. It also uses HBC's two initial
clearing copies and its 0.05 viewport adjustment. Display-copy control enables
the Y-scale bit for any non-1:1 EFB-to-XFB scale, and the GX flush ends with a
PowerPC `sync` instruction like libogc's `GX_Flush`. These changes match
specific HBC/libogc commands; the startup diagnostic still reports
`GX XFB BLACK` in Dolphin, so the GX rendering failure is not resolved.

HBC's `initialize_display` now waits for the two initial clear copies to finish
before it schedules the XFB for VI scanout. The TEV stage 0 order is also set
explicitly at BP register `0x28`, matching libogc's `GX_SetTevOrder` register
mapping and HBC's `GX_Init` default (`TEXCOORD0`, `TEXMAP0`, `COLOR0A0`). The
startup DOL still reaches `GX XFB BLACK` in Dolphin after these changes, so
neither change is a confirmed fix for the black XFB. A further comparison found
that the renderer set biased scissor coordinates but did not set BP register
`0x59` with libogc's `GX_SetScissorBoxOffset(0, 0)` value (`0x5902acab`). It
also did not explicitly enable XF clipping as `__GX_InitGX` does. Both states
are now set in the 2D setup path and the scissor offset has a register-packing
regression test. The startup diagnostic still reports `GX CLEAR XFB BLACK`
after both copies, so the new state setup alone does not fix the copy. HBC also
now waits for the first clearing copy before it queues the second, matching the
`GX_DrawDone` sequence in HBC's C renderer. This change still needs a PowerPC
build and Dolphin or hardware verification before it is treated as a confirmed
fix.

The GX work is incomplete. The FIFO setup is a minimal single-owner path. It
installs a PE finish handler and waits for completion before it schedules a
frame for VI scanout. It does not install CP token/overflow handlers or provide
FIFO recovery, reset, and abort handling. HBC sprite and text rendering also
needs vertex descriptor and format commands, texture-coordinate generation,
TEV and blend state, texture and palette upload, and texture-cache
invalidation. Treat FIFO flow control as unverified until it has hardware
testing. Do not use the display functions before FIFO initialization.
The rectangle path does not load textures or support HBC sprite, icon, or text
rendering.

`crates/hardware/ogc-gx/src/lib.rs` `init_tex_obj` must return without changing
the texture object when either input dimension is zero. GX texture dimensions
are encoded as `dimension - 1`; without this guard, zero underflows and creates
an invalid packed size. A focused regression test checks zero width and zero
height and confirms the initialized object remains unchanged. This test cannot
run on the host because the crate and its dependencies require the Wii target.

The HBC icon renderer also needs native Rust RGBA8 texture support in
`ogc-gx`. `configure_textured_2d` sets direct position, color, and texture
coordinates, one texture generator, a texture-modulate TEV stage, and alpha
blending. `load_tex_obj` emits the Wii RGBA8 texture-map registers for the
default texture-cache regions and invalidates the texture cache. The quad path
emits position, white color, and UV coordinates through the GX FIFO. HBC owns
32-byte-aligned tiled bytes and flushes them before GX reads them. Target checks
and register-packer tests cover the code, but test binaries cannot run on the
host and the GX output needs real hardware verification.


## GX 2D and texture state corrections from the `libogc` comparison

The ignored `libogc-rs` copy needs the changes in `docs/libogc-rs-gx-repair.diff`.
Apply this patch from the `hbc-rs` directory with `patch -p1` when you create a
new local copy. The renderer must write the same matrix index to CP and XF,
update `XF_NUMTEX` when it switches between textured and solid draws, set the
M2x4 TEX0 generator, load its post identity matrix, and set TEX0 size registers
from each texture's dimensions. Its solid pass-color TEV inputs must select the
zero constants instead of previous TEV output. It must load an identity TEV
swap table. The GX depth baseline from `GX_Init` is `0x17` (`GX_ENABLE`,
`GX_LEQUAL`, depth update); `GX_CopyDisp(clear=true)` uses `0x1f` while clearing
and restores `0x17` afterward.

The same comparison found four additional register/API mismatches. `GX_Init`
enables dither and sets Z-compare location to early mode, so frame setup and
solid draws now preserve dither (`0x1c` in color mode and bit 2 in the blend
word) and set the pixel-engine control bit (`0x40`). Texture-map registers for
slots 4–7 use the separate `0xa0`–`0xb7` register range. Texture-coordinate
scale registers now use the selected map's matching coordinate slot. Finally,
framebuffer width must be divisible by 16 because the GX copy destination
register stores width in 16-pixel units. Host tests cover these encodings and
the width check. These fixes align the Rust implementation with libogc; a
PowerPC build does not confirm the black-XFB issue is resolved, and HBC still
needs runtime testing on Dolphin and Wii hardware.

A later comparison found that libogc disables pixel-engine bit `0x40` while
`GX_CopyDisp(clear=true)` clears the EFB, then restores it. Rust now follows
that sequence. The same update preserves negative texture LOD bias bits, and
makes the CI texture initializer leave an object unchanged when its dimensions
are zero. Host tests cover the PE bit sequence and signed LOD field packing.

The standalone register tests run on the host without the PowerPC-only crate
or its dependency graph:

```sh
rustc --test libogc-rs/crates/hardware/ogc-gx/src/register_state.rs -o /tmp/gx-register-tests
/tmp/gx-register-tests
```

Dolphin defaults to keeping XFB copies in its texture cache and leaves
`01 FE 01 FE` in emulated RAM. This value is Dolphin's XFB placeholder, not a
readout of the GX copy destination. `tools/capture-startup.py` creates a
Dolphin profile with XFB-to-RAM copies enabled and deferred EFB copies disabled
before it captures the separate GX framebuffer. Its config behavior is covered
by `python3 -m unittest discover -s tools -p 'test_*.py'`.

The startup diagnostic draws and checks a 32 × 32 white test square at the
center of the framebuffer. Its first GX frame draws only this square over the
initial clear. If the check fails, it writes a centered white square with a red
outline through the CPU, then displays it. This checks VI scanout separately
from GX draw and copy behavior. The latest Dolphin run proceeds to HBC's
storage-scan screen. The Wii run showed the CPU-written square in the
upper-left corner; the updated probe places it at the center and adds the red
outline.
