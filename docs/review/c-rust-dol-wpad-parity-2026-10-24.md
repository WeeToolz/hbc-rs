# DOL and Wii Remote C-to-Rust parity review

Date: 2026-10-24

## DOL validation and SD read path

The physical Wii report showed
`DolHeaderOverlap { section: 0, offset: 48, size: 48 }` for a DOL whose local
header records the first text file offset as `0x100`. The Rust check in
`src/loader/mod.rs#push_dol_segment` reads the DOL header correctly when given
the complete bytes. Relaxing that parser check would hide an input corruption
and could load bytes from the header as code.

The host parser test and the production `loader-helpers` Dolphin case include
the reported values (section offset 48, size 48) and confirm that `parse_dol`
rejects them. This protects the safety check; it does not show where the Wii
read path changed the header bytes. See the
[Dolphin evidence](evidence/reborn-api-loader-helpers-rust-dolphin-2026-11-24-r6/README.md).

A second production Dolphin case reads valid and malformed DOL files through
the real `WiiStartup` FAT-backed loading path. It confirms bounded reads,
successful validation for the valid fixture, and the reported overlap error for
the malformed fixture. This checks the Rust FAT and parsing path under Dolphin;
the same capture is still needed from a physical Wii. See the
[FAT-backed loader evidence](evidence/reborn-api-wii-startup-dol-load-rust-dolphin-2026-11-24/README.md).

C Reborn's `source/loader_reloc.c#reloc_dol` checks section file bounds before
copying each section. Rust first reads the whole selected file, validates the
complete plan, and only then stages memory writes. That ordering is a deliberate
safety difference. It does not explain why the Wii parser received `0x30` where
the local file contains `0x100`.

The shared SD read path is `WiiStartup::begin_selected_executable_load` →
`advance_selected_executable_load` → `FatFile::read` → `read_file_bytes` →
`FatMount::read_sector` → the selected `DiscInterface::read_sectors` backend.
Before this change, FAT passed byte-aligned stack arrays directly to the disc
backend. The C Wii SD and Rust Wii SD backends both accept unaligned buffers by
using their own aligned bounce buffers. The Rust FAT driver now also uses a
32-byte aligned sector buffer for reads and writes, then copies at the FAT
boundary. This gives other `DiscInterface` backends an aligned transfer buffer,
but source review does not prove this change caused or fixes the reported bad
header. The DOL failure remains open. Capture the file size, hash, and first
`0xe4` bytes immediately before `parse_dol` on the Wii, then compare them with
the same file on the card.

## Wii Remote player lights

C HBC calls `WPAD_Init` and configures WPAD channels in
`source/controls.c#controls_init`. libogc assigns the connected Remote's WPAD
channel and handles the `WIIUSE_CONNECT` event in
`C-sources/libogc/wiiuse/wpad.c`; it sends `wiiuse_set_leds` with
`WIIMOTE_LED_1 << (channel % WPAD_BALANCE_BOARD)`. `wiiuse_set_leds` sends
output report `0x11` in `C-sources/libogc/wiiuse/wiiuse.c`.

Rust assigns the Bluetooth HID slot to the corresponding WPAD slot in
`wpad-sys/src/powerpc_backend.rs#process_hid_event`, then calls
`initialize_wii_remote`. That path sends output report `0x11` before requesting
status. C's `wiiuse_sendcmd` sets bit `0x02` in the LED parameter, and its send
path adds bit `0x01` when rumble is active. Rust now builds the same base LED
parameter (`LED bit | 0x02`) and its common sender adds `0x01` for active rumble.
A failed report submission returns the transfer error through the connection
event path. Regression checks cover the six SDK slots and the C-compatible
report bytes. Real player lights remain unverified until tested on the Wii.

## Wii Remote idle timeout

C HBC calls `WPAD_SetIdleTimeout(300)` in `source/controls.c#controls_init`,
which sets a five-minute timeout. Rust previously called
`wpad_sys::set_idle_timeout(120)`. Rust now uses the same 300-second timeout,
and a host regression checks the production configuration constant. A Wii
target build checks the WPAD configuration call. These checks do not prove the
timeout's effect on a connected physical Remote; that check remains pending.

## Verification and limits

- `cargo check` for `ogc-fat` and `wpad-sys` passed for the Wii PowerPC target
  with warnings denied.
- The SDK regression tests are in the source. The target-only SDK crates do not
  provide a host test executable. Attempting host `cargo test` fails because
  dependencies reject non-PowerPC targets. Attempting `cargo check --tests` for
  the bare-metal PowerPC target cannot load Rust's `std` and `test` crates.
- Rust formatting was checked with `rustfmt --emit stdout` for both changed SDK
  files.
- Physical Wii checks are still required for DOL byte integrity and player
  lights. Dolphin does not prove SD controller DMA or physical Remote LEDs.
