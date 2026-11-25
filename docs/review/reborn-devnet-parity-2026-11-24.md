# Reborn developer protocol parity review

Reborn's `devnet_handle` is called from its loader thread. It first forwards
requests to `hbc_agent_handle`, then handles developer commands on the same
listener as Wiiload. Rust's `receive_client` reads a 16-byte header, dispatches
its HBCD, HBCC/HBCQ, and HBCI extensions, handles HBCK, then attempts to decode other headers as Wiiload. The
following Reborn commands need further work:

| Reborn command | Reborn behavior | Remaining Rust work |
| --- | --- | --- |
| HBCK | Queue bounded remote HOME overlay keys and reply | The bounded queue and shared HOME menu controls are implemented. Verify visual delivery; Reborn-only overlay actions remain open. |
| HBCP | Reply with dimensions and the displayed YUYV framebuffer | Implement a safe framebuffer snapshot and reply, including cache handling. |
| HBCH | Return hardware and settings information as JSON | Implement the defined information response and its supported fields. |
| HBCS | Return HBC status as JSON | Rust now returns version, protocol, IOS version and revision, AHBPROT state, free system arenas, free bytes in its managed MEM2 heap, measured TCP stack use and size, measured initialization and app-scan times, local IP as a dotted-quad string (including `0.0.0.0` when unavailable), successful network starts, app count, active and inserted devices, live receiver state, receive progress, selected transfer style, and system time. The response remains partial until detailed startup marks, HBCF transfer state, crash, upload, and retained-log fields are available and checked against the client. |
| HBCT | Select a transfer display style | Rust accepts the exact `scroll`, `bounce`, and `truncate` names from the twelve-byte C-string field, rejects unknown names with `EINVAL`, stores the choice, and returns it in HBCS. Host tests cover names, case, NUL termination, and the HBCS field. The isolated Dolphin run checks all three valid replies, an invalid style reply, and HBCS state. Rust draws a localized HBCF transfer line and applies scroll, bounce, or truncate to long names. Host tests cover the formatter. Font-width fitting and rendered style comparisons remain unverified; real Wii text behavior also needs validation. See [combined file and style evidence](evidence/reborn-devnet-file-and-style-rust-dolphin-2026-11-25/README.md). |
| HBCF | Serve developer file operations | Rust parses bounded paths and implements FAT-backed stat, list, checksum, delete, mkdir, raw/framed upload, and raw/framed download. Framed downloads honor the compression flag. Successful changes to catalog-visible files on the active device queue a catalog rescan after one quiet second. The footer shows localized transfer direction, file name, progress, and the selected long-name style. Host tests cover protocol parsing, change filtering, and display formatting. `tools/test_remote_input_dolphin.py --reborn-file --sd-image <private-image>` passes live stat/list/checksum/delete, raw/framed transfers, and app catalog upload/delete refresh against a cloned SD image; see [combined file and style evidence](evidence/reborn-devnet-file-and-style-rust-dolphin-2026-11-25/README.md). Real Wii FAT behavior still needs validation. |
| HBCA | Record the next upload name and flags | Rust accepts names up to 63 bytes, stores flags for 120 seconds, and consumes the metadata once for the next valid Wiiload payload. `DEVNET_UPLOAD_YES` skips the app-ZIP confirmation while retaining ZIP validation and staged installation. The name is not yet used for Message Board playtime because Rust has no matching writer. Upload-result reporting and C/Rust Dolphin wire comparison remain open. |
| HBCL | Return a retained application log | Implement retained log validation and response. |
| HBCN | Set the network log target | Implement the target and the launch/return handoff contract. |
| HBCC | Clear retained crash state | Resolve its wire-format collision with Rust's existing HBCC extension and implement compatible crash clearing. |

The Rust HBCI remote input protocol and its HBCC/HBCQ command mailbox are
extensions. They do not establish compatibility with the Reborn commands.
`HBCC` needs special care because the same magic selects different operations;
keep the existing extension functional and compare the complete request and
response formats before adding a dispatcher. Do not claim support based only
on a recognized magic value.

This is a source review, not a runtime comparison. Each command needs tests for
its actual reply, errors, and effects, including Dolphin coverage. HBCP cache
behavior and persistent or device-specific behavior also need physical Wii
validation. Existing physical evidence remains pending.
