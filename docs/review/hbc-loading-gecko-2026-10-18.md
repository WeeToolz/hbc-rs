# HBC upload, DOL validation, and Gecko review (2026-10-18)

Authoritative C comparison: `C-sources/hbc-reborn/channel/channelapp/source/loader.c`
and `loader_reloc.c`. The existing ledger IDs remain the status source.

## Confirmed findings and production changes

- **HBC-WIILOAD-01:** The receiver had an 8,192-byte stack. Disassembly of the
  matching PowerPC release ELF shows the miniz decompression helper starts with
  `stwu 1, -10560(1)`. This frame alone exceeds the worker stack. An actual
  compressed DOL upload in isolated Dolphin reached decompression, then panicked
  with a 1,088-byte allocation failure. The stack overflow makes that allocator
  error unsuitable as evidence of normal heap exhaustion. The worker now has
  32 KiB, with a compile-time size check against the decoder state and caller
  reserve. A repeat upload finished decoding, published the payload, validated
  it, staged it, unmounted storage, and reached the IOS handoff.
- **HBC-WIILOAD-01:** The priority-64 main thread could spin while the priority-48
  producer held the upload slot. The consumer now tries the lock once and leaves
  the payload untouched while busy. A regression holds the production lock over
  120 consumer attempts, verifies the bytes remain intact, then verifies their
  ownership is transferred once the producer releases the lock. This is a
  confirmed scheduler hazard; it was not isolated as the sole hardware stall.
- **HBC-WIILOAD-01 (Gecko):** Rust previously marked the Gecko receiver ready
  unconditionally and displayed an active icon without a device. C starts its
  Gecko worker only after `usb_isgeckoalive`. Rust now calls the production
  `is_gecko_alive` probe at startup and on each Gecko poll, and only flushes a
  detected device. Tests verify the indicator remains off with no detected
  device for all relevant receiver states. Real hardware removal/insertion
  remains pending.
- **HBC-LOAD-01:** C's `dolheader` ends at offset `0xe4`; Rust required `0x100`
  bytes and rejected a section starting at `0xe4`. The parser now treats `0xe4`
  as the required header length. Regressions accept compact and padded layouts
  with independent byte offsets, and reject an offset inside the actual header
  and a truncated header. The existing padded-header bounds fixture still uses
  offset `0x100` and its original expected range. The user's photo reports
  `section overlaps DOL header` while selecting C HBC. That exact card file's
  header bytes have not been collected, so compact layout is not yet proven
  to be the explanation for that particular file.

## Runtime and verification limits

The isolated Dolphin upload tests call the actual Wiiload receiver and production
loader. The baseline panicked during decompression. With the larger stack, the
same normal-HBC DOL reaches `HBC: transferring executable control`. A later
trace shows preferred-IOS lookup returns `-12544` in the empty isolated NAND;
the existing error path returns toward the System Menu. This is not an
end-to-end application-entry pass. C also selects `IOS_GetPreferredVersion()`
through `APPS_IOS_VERSION`; no fixed-IOS replacement was retained.

All 339 HBC host tests pass. The normal PowerPC DOL builds. `audit-source`
passes with the three existing keyboard-queue notes. The completeness audit is
blocked by the existing stale `mp3player-sys::init` evidence at `src/lib.rs:840`.
No audit expectations were weakened. The source diff has no whitespace errors.

The user confirmed the earlier network ordering fix now connects quickly on
Wii. Rust had tried to attach and wait for an IP for about ten seconds before
starting NWC24. Its normal receiver now follows C's initialization order. The
existing cancellation-aware IOS wait remains; it does not time out simply
because a connection takes more than four seconds.

## Hardware build sent

Normal DOL `target/hbc-rs.dol`, SHA-256
`b10b97ed599e4211ebac799bfb12b0c84330cbc8bc729adb4eeba1b90027010a`,
2,415,808 bytes, includes the stack, handoff-lock, hardware-presence, and DOL
header fixes. It was sent through the devkitPro Wiiload sender in Terminal;
`target/send-dol-header-fix.log` ends with `done.` and the status is zero.
That confirms transfer, not application entry. The user's follow-up on local
C-HBC launch and the Gecko icon is pending.


## Exact C-HBC file remains rejected; detailed build sent

The user's next photo still shows the original `section overlaps DOL header`
error while selecting C HBC. The compact-header change has therefore not yet
been shown to resolve this exact file; the running build identity was not
independently captured. The parser now returns `DolHeaderOverlap` with the
section number, source offset, and source size instead of discarding those
values. The bounds check remains enforced.

The new normal build has SHA-256
`1bad3886384a2cd6dd70ae2e022face4389a735f89bc87772c55b47ca49f6f33`
and is 2,416,512 bytes. All 339 host tests pass. It was sent from the confirmed
C HBC listener with the reference sender through Terminal;
`target/send-dol-header-details.log` ends in `done.` and the exit status is zero.
The next required evidence is the exact on-Wii section/offset/size or the actual
C-HBC DOL bytes. Do not classify the local DOL loading issue as fixed yet.


The next Wii photo shows `PreviousLoadFailed` from the shared executable
launch path. DOL staging completes before this check. Rust had treated the
protection against overwriting unreadable settings as a reason to stop app
launch. C Reborn calls settings_save without making its result a launch
condition; direct DOL launch can lack the installed channel NAND identity.
The shared Rust exit preparation now reports an explicit preserved outcome
when an earlier load failed. It does not call the writer or clear dirty state.
Actual backend write errors still stop departure. Both system exit and app
launch use this policy. Two production-engine regression tests cover preserved
settings, write failure, retry, and clean settings. All 341 host tests pass;
real Wii app entry remains pending.

The settings-launch fix normal DOL SHA-256 is
`b8ca1071550d4230899fca7ecc096fe98eebbdd387af3075fc51b18764751aae`.
The reference Wiiload sender completed with `done.` and exit status 0 from
the user-confirmed C HBC listener. This proves transfer only.
