# Independent USB volumes and DRY review — 2026-10-23

## Contract and source comparison

Reborn uses two independent drive contexts in
`channel/channelapp/source/usbmsd.c` and five entries in `source/appentry.c`:
`sd`, `usb`, `carda`, `cardb`, `usb2`. The slots follow IOS discovery order.
They are not fixed physical USB port numbers. Existing saved indices 0–3 must
stay valid; the second USB volume uses index 4.

Rust now implements that structure in the shared SDK. Each drive has a stable
handle address and its own LUN, identity, removal state, idle clock, transfer
mode, command buffer, mutex, and alarm. A shared disc lock serializes selection
and transport use. A device ID cannot belong to both slots, including two
devices with the same VID/PID. A failed close keeps its ownership claim.
Closing one drive resets only that drive. A partial failed-open descriptor can
be closed without transfer buffers that were never allocated.

One const-generic disc-interface definition supplies both callback sets. One
transfer engine validates and transfers both reads and writes. It returns an
idle remount failure before issuing I/O against stale capacity. FAT maps
`MountSource::Usb` and `MountSource::Usb2` to separate interfaces, including
transport flush routing. HBC scans all five volumes through its existing
catalog and DOL launch path. `VOLUMES` owns scan membership and array size.
There is no second DOL loader and no alias of the first USB handle.

The storage discovery path still requests only mass-storage interfaces. It
does not query HID devices as storage candidates. Shared USB host selection
and HID message layouts remain in the existing USB implementation.

## Six DRY findings

| Finding | Change | Verification |
| --- | --- | --- |
| Separate test transaction engines | Removed the obsolete test-only commit/recovery engines and cleanup error types. Tests now call `save_with_recovery_backups` and `recover_from_retained_backup`. | Fixed bytes, partial-write recovery, retained backups, operation order, and injected failures at backup and primary boundaries. |
| Three emulator packet parsers | Startup and footer capture use `dolphin_test_support.Debugger`. Async stops are queued; checksums and memory lengths are checked. | Seven parser tests include empty responses, timeout bounds and timeout restoration. |
| Receiver stop/work/restart copies | `stop_work_restart` owns work and restart ordering. `stop_for_work` shares the active-upload guard with ordinary restart. Work and restart errors remain separate. | Stop failure, work failure, restart failure, and both failures. PowerPC Clippy checks actual callers. |
| Recovery classification copies | `RecoveryOutcomes` owns classification and precedence in the host-testable production module. Hardware adapters retain candidate order and identity checks. | The shared production policy is tested on the host; target callers are compiled. |
| Repeated rectangle validity | `Rect::is_valid` owns finite inputs and positive size for intersection and drawing paths. | All five drawing paths first accept valid bounds and then reject invalid bounds without new commands. |
| Two theme archive limits | Startup derives its `u64` limit from `theme::MAX_THEME_ARCHIVE_BYTES`. | Accepted size remains 20 MiB; source and target checks. |

The obsolete tests expected backup deletion. Production retains recovery
copies. Their replacement tests check the production policy and preserve the
relevant failure boundaries; the old policy was not added to production.

## Runtime and hardware limits

Scripted Dolphin tests use a separate NAND copy and an actual emulated FAT SD
card. They select `boot.dol` in the HBC browser, require a new child session,
and count actual IOS58 launch events. The Options capture shows independent
USB status entries. Final results are in
[evidence](evidence/usb2-storage-2026-10-22/README.md).

Fresh registered HID probes check descriptor conversion and emulated VEN/HID
backend startup. They do not prove physical HID reports. The backend probe
keeps device listing disabled; it is not an enumeration acceptance test.

The SDK provides `usb_volumes_probe` for two real media. It uses actual FAT/USB
APIs, reads distinct fixture bytes in alternating order, unmounts `usb:`, and
reads `usb2:` again. It fails for absent media or invalid fixtures. It does not
contain a replacement USB model. No explicit cache-flush test was added to
this read-only probe. It has not run on physical hardware.

Required open acceptance: one drive in each physical port; two drives together;
removal and reconnect without invalidating the other drive; failed transport
close; HID reports while both drives are attached. Physical return-stub
recovery also remains open. No physical Wii or original NAND was changed.
