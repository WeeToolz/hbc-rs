# Reborn to Rust: SD and USB DOL loading

Reference source is `C-sources/hbc-reborn/channel/channelapp`, not the older
`C-sources/hbc` baseline. The existing parity ledger remains the single status
record. Explicit `reborn:` references select this source tree in its validator.

## Compared contracts

| Contract | Reborn | Rust production path | Finding |
| --- | --- | --- | --- |
| SD discovery and FAT access | `appentry.c`, `__io_wiisd`, mount and scan worker | `FatStorage::mount`, `ogc_fat::mount_sd`, `WiiStartup::poll_storage_scan` | Shared SD backend and catalog path; emulator acceptance recorded separately. |
| USB discovery | `usbmsd.c::is_inserted`, IOS mass-storage device list | `ogc-usbstorage-sys::wii_backend::storage_device_candidates` and `is_inserted` | Rust iterates IOS devices without a physical-port filter. Either physical port can supply a candidate. Real ports are not verified by an SD test. |
| Two active USB drives | `usbmsd_disc[0]` and `[1]`, `usb:` and `usb2:` | One `USB_STORAGE_INTERFACE` and `Volume::Usb` | Missing independent second drive state and volume. Reborn slots follow discovery order; they are not fixed physical port names. Keep this open. |
| Read an application | `loader.c::ld_load_func`, `LDC_FILE`, bounded read loop | `prepare_selected_executable`, `LaunchSource`, `read_file_into` | Rust reads the catalogued size into an owned Arena2 buffer, checks the returned size, and passes the shared validated plan to the launch adapter. |
| DOL sections and entry | `loader_reloc.c::reloc_dol` | `loader::parse_dol`, `stage_launch`, MEM2 trampoline | Both accept the 0xe4-byte DOL header and copy declared sections. Rust validates the entire plan and executable entry before modifying application memory. |
| Arguments | `loader_reloc.c::set_argv` and metadata argument block | `build_launch_arguments`, trampoline argv record | Shared path-first, NUL-delimited argument block. Rust rejects oversized arguments instead of writing past its reservation. |
| IOS policy | `xml.c::meta_parse`, `main.c::main_real` | `parse_app_xml`, `app_ios_launch_policy`, `transfer_control` | Fixed: `no_ios_reload` is independent of `ahb_access`; it disables both IOS reload and the patch used only for that reload. |
| Shutdown and branch | Reborn app, network, storage, input, video shutdown and `loader_exec` | Shared handoff cleanup and `transfer_control` | Existing Rust order is preserved; each failure stops the handoff. Physical equivalence still needs a real-device probe. |

## Confirmed fixes

The Rust parser previously treated `<no_ios_reload/>` as `ahb_access=true` and
the launch policy always reloaded IOS. Reborn parses these flags separately.
The parser now stores both flags. One loader-owned policy maps all four flag
combinations. SD and USB launches use the same policy and transfer function.
The no-reload branch skips the AHB-reset patch, IOS launch, and post-reload DVD
register write, as Reborn does.

The first browser-driven SD run read the DOL, completed service shutdown,
reloaded IOS58, and reached child startup. Its new Wiiload bind then failed
with host error 48, Address already in use. The script had sent read-only
status/input traffic during the card read. The listener now checks
`SO_REUSEADDR` setup before bind so connections in TIME_WAIT do not prevent
the new instance from starting its service. Failure to set the option closes
the socket and is returned with cleanup details. `SO_REUSEPORT` is not enabled.
This is a Rust service-restart improvement; Reborn's `tcp_listen` does not
enable this option. Real IOS acceptance remains unverified.

## Deliberate safety differences

Reborn relocates sections while it checks them. Rust validates and stages all
sections before the final copy. This prevents a later invalid section from
leaving a partially overwritten HBC. Rust also validates the entry and keeps
the final copy routine outside the application destination region. These
differences retain the actual load operation and reject malformed executables.

Reborn logs an AHB patch error and continues. Rust stops the handoff through
its System Menu failure route when a requested AHB patch or IOS reload fails.
This avoids starting an application after a failed transition with different
hardware access. These failure routes still need physical qualification.

## Verification and remaining work

Run evidence is under `docs/review/evidence/reborn-storage-parity-2026-10-22/`.
The first failed SD run is retained. Final results are recorded in that
directory's README and the generated parity report after verification.

The SD test uses Dolphin's emulated SD card, real FAT/catalog APIs, production
controller input, the selected-executable path, and the matching HBC child DOL.
It requires a new HBC session and checks the actual IOS policy in runtime logs.
It does not substitute Wiiload for the storage load.

Physical SD, one drive in each physical USB port, two simultaneous USB drives,
hot removal, AHB-enabled launch, and recovery from failed IOS transitions remain
unverified. No physical NAND or firmware write is part of this task. Independent
USB drive contexts must be implemented before claiming Reborn dual-drive parity.
