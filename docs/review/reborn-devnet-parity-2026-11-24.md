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
| HBCS | Return HBC status as JSON | Rust now returns version, protocol, IOS version and revision, AHBPROT state, free system arenas, free bytes in its managed MEM2 heap, measured TCP stack use and size, measured initialization and app-scan times, local IP as a dotted-quad string (including `0.0.0.0` when unavailable), successful network starts, app count, active and inserted devices, live receiver state, receive progress, and system time. The response remains partial until detailed startup marks, transfer, crash, upload, and retained-log state are available and checked against the client. |
| HBCT | Select a transfer display style | Implement the supported style names and view update. |
| HBCF | Serve developer file operations | Implement the request protocol and bounded storage operations. |
| HBCA | Record the next upload name and flags | Implement the one-shot name, flags, expiry, and install policy. |
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
