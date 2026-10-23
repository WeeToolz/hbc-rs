# Reborn storage loading and Clippy — 2026-10-22

## Final checks

All four Clippy commands pass with warnings denied. No lint was suppressed.

| Configuration | Check | Result |
| --- | --- | --- |
| Host, default features | `cargo clippy --all-targets -- -D warnings` | Pass |
| Host, network debugger | `cargo clippy --all-targets --features debugger-network -- -D warnings` | Pass |
| PowerPC, default features | Nightly target Clippy, release `hbc-rs` binary, `-D warnings` | Pass |
| PowerPC, network debugger | Same target command with `--features debugger-network` | Pass |

The exact target commands, including target JSON and build-std options, are in
`clippy-powerpc-default.log` and `clippy-powerpc-network.log`. The target checks
use nightly's cargo-clippy and clippy-driver, not the host stable driver.

All **387 host tests** pass, with none failed or ignored. The Reborn reference
root validator and screenshot regression tests also pass. Python syntax and
the existing parity ledger/report checks pass. The ledger now has 41 contracts;
the coverage and hardware status remain separate from these check results.

The SDK source audit passes. The SDK completeness audit still fails on its
existing 2,371 findings. Its raw output is retained in `audit-stubs.log`. No SDK
source was changed in this task. This failure is not a Clippy result.

## Production fixes

- App metadata retains separate `ahb_access` and `no_ios_reload` flags. One
  loader-owned policy is used by storage launches. Four independent flag
  combinations are tested. The no-reload branch skips the IOS reload and the
  associated AHB reset patch and DVD-register write.
- The Wiiload listener checks `SO_REUSEADDR` setup before binding. A setup
  failure closes the socket and returns both operation and cleanup details.
  Address reuse prevents a child instance's bind from failing because prior
  connections remain in TIME_WAIT. Port sharing was not enabled.
- Clippy findings were fixed in both host and PowerPC code. The command packet
  decoder now returns a typed error; its wire reply is unchanged. Tests check
  malformed headers, command values, missing command IDs, and valid status
  queries with ID zero. Other lint fixes preserve operation and error ordering.

The detailed source comparison and safety differences are in
`docs/review/reborn-storage-loading-2026-10-22.md`. The CSV ledger is the only
implementation status record; it retains the open USB drive-state gap.

## Dolphin SD acceptance

The script boots the normal debug WAD in a private NAND copy. It uses Dolphin's
SD folder synchronization to create a real emulated FAT card containing only
`apps/selfload/boot.dol` and its metadata. It waits for the production SD mount
and catalog, opens the app details, selects Load, and starts the DOL. Network
traffic supplies controller input and read-only status queries; it does not
upload the DOL. Required screenshots show the app's Load button and the child
HBC screen. All four details/child screenshots from the final runs were inspected.

| Final case | WAD ready | Load action to child ready | Parent session | Child session | Actual IOS58 launches after handoff |
| --- | --- | --- | --- | --- | --- |
| `sd-default-r3` | 3.102 s | 15.225 s | 1028 | 17987 | 1 |
| `sd-no-reload-r3` | 2.822 s | 14.923 s | 1028 | 17311 | 0 |

Each child readiness check requires a valid HBC protocol reply with a new
session. The logs prove selected-file read and validation, shared cleanup,
transfer, runtime shutdown, child startup, and the new listener. Independent
checks count Dolphin's actual IOS58 launch events, rather than using only the
guest policy message. Both emulator processes stopped cleanly.

The final launch input acknowledgment can be lost while the main thread reads
the card or closes the receiver. The script sends the launch once, retains any
acknowledgment error, and requires the new child and its runtime evidence.
An input submission or missing reply alone is not treated as launch success.

## Failures retained

`sd-default` records the first failure: the DOL loaded and the child started,
but the child's bind failed with host error 48, Address already in use.
`sd-default-r2` passed after the listener fix. The following `sd-no-reload-r2`
did not launch because the host preflight bind rejected TIME_WAIT. Its log is
retained. The preflight now uses address reuse while still rejecting an active
listener. Both final r3 cases pass. Failures were not relabeled as passes.

Text Dolphin logs have CRLF normalized to LF. Original bytes are preserved in
adjacent `dolphin-raw.log.gz` files, with hashes in `raw-log-hashes.json`.

## Build identity and limits

Final package: `dist/hbc-reborn-storage-parity-2026-10-22-r3/`.

- DOL: 2,454,496 bytes; SHA-256
  `3b08c28e1f411f26160ffa02dd7a0216c3931bb6a266862c2ff6d07a9adc283e`.
- WAD: 3,097,216 bytes; SHA-256
  `9191cb9aa7294330f73360c9ed1894a8ff60e49476bd82c71c4ac9becda8f8dc`.
- Build: normal release with source debugging and `debugger-network`.

The WAD uses legacy fakesigned metadata and was verified by reading back both
contents. The package manifests record source revisions and dirty worktrees.
The tests do not establish retail signature trust or physical Wii behavior.

USB port 0 and port 1 were not runtime-tested. Rust's USB enumeration has no
physical-port filter, but the app exposes only one USB volume. Reborn's two
independent drive contexts and `usb2:` volume remain a specific open parity
gap. A successful SD launch exercises their shared loader, not the missing
second USB transport or physical port behavior. Physical SD, both USB ports,
AHB-enabled launch, return-stub recovery, and BootMii remain unverified here.
No physical Wii or original NAND was modified.
