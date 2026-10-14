# HBC-RS Wii Remote and DOL loading report

## User report

The user reports that a previously paired Wii Remote keeps blinking in
HBC-RS. The user can navigate with a GameCube controller. When the user selects
Tetris from a USB drive, HBC-RS displays `Could not load this application:
Storage(Allocation)`. The same Tetris DOL loads in the C reference HBC. The
Tetris DOL size is not known.

The dialog text comes from the error returned by
`WiiStartup::prepare_selected_executable`. Before the fix,
`FatStorage::read_file` allocated a full-file `Vec` from the global Rust heap
and returned `StorageError::Allocation` when `try_reserve_exact` failed. The
failure occurs before DOL parsing or executable handoff.

## DOL allocation change

Rust now reserves an aligned executable buffer from the top of Arena2 and reads
the USB file directly into it. The reservation checks that at least 512 KiB
remains above Arena2Lo and restores Arena2Hi when preparation fails or the
source is dropped. A successful handoff consumes the launch image, so the
buffer remains reserved while the trampoline copies the executable.

This follows C HBC's top-down `blob_alloc` path and `BLOB_MINSLACK` rule in
`C-sources/hbc/channel/channelapp/source/blob.c`. This can help when a smaller
file cannot fit in the fragmented Rust heap. It also avoids the Rust heap's
16 MiB cap for this source buffer, subject to available Arena2 space. The
source comparison does not prove that the reported Tetris DOL loads on the
user's Wii.

## Wii Remote key capacity change

C WPAD reads up to ten registered controller link keys and uses up to four
guest records from SYSCONF. Rust already loads guest keys and requests stored
controller keys from the Bluetooth host, but its key cache held only six
records. When earlier records filled that cache, later registered keys were
dropped, so a later Wii Remote key could not answer the authentication request.
Rust now holds ten registered keys plus four guest keys. A host regression
loads all four guest records and ten stored controller keys, then verifies the
HCI reply for the tenth registered key.

This is a confirmed support gap. It does not prove that it caused this user's
blinking remote because the Wii's stored key count and order were not captured.
The user reports that the remote was paired before the test.

HBC-RS now writes Bluetooth startup stages, connection events, and changed
`HostDiagnostics` values to the Wii system log. The diagnostic record includes
link-key requests, matches and misses, connection attempts and failures, and
authentication status. This uses the existing `ControllerManager` hooks. It
does not change pairing or connection behavior.

## Verification

- `ogc-bt-sync`: 92 host tests pass; strict Clippy passes.
- HBC-RS: 316 host tests pass; strict host Clippy passes; the PowerPC release
  build passes.
- `cargo fmt --check` and `git diff --check` pass for the HBC-RS change.
- No real-Wii acceptance test was run.
- The rebuilt DOL is `target/hbc-rs.dol`, 2.3 MiB, SHA-256
  `861cfcc01b856c1be05e669779c10e732713c242fb544db3e3a8289d365250d1`.

## Wii follow-up

Run the rebuilt HBC-RS DOL with the same USB Tetris directory and compare it
with C HBC. Record the Tetris `boot.dol` size and whether the error dialog still
appears. For the Wii Remote, capture Bluetooth startup stage, key request,
key match/miss, connection attempt/failure, and authentication failure counts.
The Wii system log now reports these values after boot. Then verify buttons and
pointer input. Keep both results marked hardware-pending until those checks
pass on the Wii.
