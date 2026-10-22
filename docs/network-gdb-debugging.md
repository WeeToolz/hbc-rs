# Network GDB debugging status

## Current scope

The normal build selects `off`. It does not initialize `libdb-sys` and does
not open TCP port 2828. Use `--features debugger-usb-gecko` for the existing
USB Gecko debugger. The older `debugger` feature remains an alias for this
mode. Use `--features debugger-network` to enable the network transport and
cooperative HBC control.

In network mode, the Wiiload worker owns both listeners: TCP 4299 for Wiiload
and TCP 2828 for GDB. It opens the GDB listener after it initializes the shared
Wii network stack. The worker polls both listeners. It closes the GDB listener
before the Wiiload listener and the network stack. A GDB listener error is
retained for read-only queries without stopping Wiiload.

The transport accepts one GDB client. It uses nonblocking socket operations,
bounded 4 KiB packet buffers, and checks `$packet#checksum` frames. It supports
`qHbcStatus`, `qHbcArm`, `qHbcPause`, and `qHbcResume`. It rejects unsupported
requests with an empty GDB reply. It does not read memory, change registers,
set breakpoints, handle processor exceptions, or implement standard GDB
execution commands.

`qHbcStatus` reports live transport and cooperative state. For example:

```text
transport=connected;client=present;cooperative=enabled;pause=running;authorization=disarmed;exception=disabled;execution=disabled;reset=normal;result=none
```

A new client does not arm or pause HBC. The client must send `qHbcArm`. The HBC
main thread applies pause and resume requests at the normal menu safe point.
While paused, the main thread checks Reset first, checks local escape input,
services queued debugger events, and sleeps to let the Wiiload worker run. A
disconnect disarms the current client and resumes HBC. A new client does not
inherit the earlier authorization.

USB Gecko behavior remains separate. Normal and USB Gecko builds do not include
the network transport. Network Reset keeps the normal return-to-menu path in
the source, but the Dolphin Reset test has not confirmed that path. The current
Dolphin test sent a Reset hotkey while HBC was cooperatively paused. The GDB
socket stayed open, and the log did not show HBC entering Reset cleanup. The
captured screen still showed HBC. This does not establish whether HBC received
the reset request. Keep Reset and menu return qualification open.

## Development entry and recovery evidence

Use the shared `build-dol --dev` wrapper for development recovery. Its
`OGC_DEV_ENTRY_STAGE` symbol records the last completed entry stage: 0 has not
entered, 1 entered the wrapper, 2 system initialization returned, 3 arguments
were accepted, 4 the watchdog thread was created, 5 recovery was armed, and
6 the reporter thread was created and application entry was ready.
`OGC_DEV_ENTRY_RESULT` starts at `i32::MIN`, retains the original entry error,
and becomes zero only after all entry work succeeds. These are development
symbols, not new GDB commands. Stage 2 alone does not prove that every system
initialization operation succeeded. The existing runtime diagnostics retain
those operation results.

`OGC_DEV_STATE` keeps its existing recovery meanings: 1 armed, 2 deadline
expired, and 3 finish restored the saved caller stub when one was available.
State 3 does not prove that shutdown or the caller stub completed. Require the
expected HBC screen and upload readiness to qualify a physical return.

`tools/test_dev_recovery_dolphin.py` tests the actual development DOL and ELF.
Supply `--dol`, `--elf`, a fresh `--output` directory, the current Mac `--host`,
and `--mode probe`, `network`, or `invalid-args`. The probe mode stops application
progress after arming. Network mode pauses HBC with an active network worker.
Both keep the report endpoint closed and require deadline exit through the
restored caller stub. Invalid-argument mode requires the original error and
no application entry. The emulator debugger is separate from the cooperative
HBC transport. These tests do not upload to a Wii or qualify physical recovery.

## Worker stack budget

The network build uses a 64 KiB Wiiload worker stack, aligned to 32 bytes.
Normal and USB Gecko builds retain 32 KiB. The 2026-10-22 PowerPC release binary
has a measured direct compressed-upload call path of 36,272 bytes. The remaining
29,264 bytes provide margin for deeper calls, thread entry, and interrupts.
This measurement is a lower bound, not a worst-case interrupt analysis.

`tools/check_wiiload_stack.py <unstripped-PowerPC-ELF>` reads the actual assembly
and stack symbol. It requires 32-byte alignment and at least 16 KiB above the
measured direct call path. It returns failure if the symbols or call path cannot
be measured. Run this audit after a compiler, decoder, transport, or worker change.

## Independent read-only listener diagnostics

Network builds accept a fixed 16-byte `HBCD` request on the existing Wiiload
service at TCP 4299. This path does not use port 2828 or the main-thread command
queue. It works while the main thread is cooperatively paused. Use
`tools/hbc-gdb-status.py --host <verified-address>` through Terminal to read it
when the target is running a build with this protocol.

The record is retained in worker memory after GDB startup or poll failure,
until the Wiiload worker is restarted. It is not written to NAND. It records
startup/poll/cleanup state, the failed socket operation, the original IOS code,
the first cleanup error, whether listen ever succeeded, and whether listener
closure subsequently succeeded. A transport invariant or unavailable host IP
can have no IOS code; the reply does not invent one.

The failure path updates a fixed record. It does not allocate, format messages,
wait for a reader, or send a report. Required socket cleanup still uses the
existing synchronous IOS close calls; this change does not claim a bound on
IOS completion time. The record is updated before cleanup is attempted.

The request is `HBCD`, version byte 1, then 11 zero bytes. All integers in the
32-byte reply use big-endian byte order:

| Offset | Length | Meaning |
| --- | --- | --- |
| 0 | 4 | `HBCD` |
| 4 | 1 | Version 1 |
| 5 | 1 | Query result: 0 accepted, 1 malformed request |
| 6 | 1 | State: 1 starting, 2 listening, 3 startup failed, 4 poll failed, 5 stopped, 6 cleanup failed |
| 7 | 1 | Flags: bit 0 ever opened, bit 1 listener closed, bit 2 IOS code present, bit 3 cleanup code present |
| 8 | 1 | Operation: 0 none, 1 host IP, 2 socket, 3 get flags, 4 set flags, 5 bind, 6 listen, 7 accept, 8 read, 9 write, 10 close, 11 transport invariant |
| 9 | 3 | Reserved, zero |
| 12 | 4 | Signed original IOS error, valid only when flag bit 2 is set |
| 16 | 4 | Signed cleanup error, valid only when flag bit 3 is set |
| 20 | 2 | GDB port 2828 |
| 22 | 10 | Reserved, zero |

Malformed requests return query result 1 with no snapshot. Normal and USB Gecko
builds do not expose this debugger diagnostic protocol. Existing Wiiload upload,
control, and input packet formats are unchanged.

## Qualification status

Host tests passed for normal, USB Gecko, and network feature builds. The GDB
transport's 37 unit tests passed; five diagnostic-client tests also passed. Dolphin tests passed for live status,
authorization, cooperative pause and resume, disconnect while paused, five
pause/resume cycles, and Wiiload main-loop completion after GDB disconnect and
reconnect.

The Reset test is not a pass. Its report and Dolphin log are in
`Devtools/reports/evidence/2026-10-21-network-gdb-continuation/`. The current
runner uses a copied NAND and never changes the source NAND. Inspect the result
and guest log before another trial.

Successful network GDB runtime qualification remains Dolphin-only. Earlier Wii
connection attempts timed out before GDB packets and are not qualification passes.
The revised DOL passed an actual Dolphin bind-failure test: holding port 2828
caused the production bind call to fail, and queries on 4299 retained operation
`bind`, IOS error `-3`, and `ever_opened=false`. The normal Dolphin run also
confirmed diagnostic queries while paused, unchanged authorization/pause state,
disconnect recovery, five cycles, and Wiiload main-loop completion. Evidence is
in `Devtools/reports/evidence/2026-10-22-network-gdb-stack-diagnostics/`.

The checked DOL was built with `--no-dev` and has no independent development
recovery deadline. The recovery procedure in the workspace `DEVELOPER.md` still
requires qualification of the exact independent recovery path before a live
hardware diagnostic. These listener tests do not qualify Reset or menu return. Do not upload a network GDB
DOL to a Wii until the required recovery process is qualified. Do not enable
exception based GDB debugging.

## Open work

1. Identify why the Dolphin Reset hotkey did not trigger HBC's Reset cleanup
   while HBC was paused. Confirm that the guest receives the reset event, then
   confirm cleanup, Wiiload worker closure, and the visible System Menu return.
2. Repeat normal and USB Gecko Dolphin regression checks after changes to shared
   lifecycle or transport code.
3. Validate network listener lifecycle and scheduler behavior on Wii only after
   the required recovery qualification passes.
4. Keep exception handlers, breakpoints, arbitrary memory access, register
   changes, and standard GDB execution control disabled. Source review has not
   established safe scheduling or socket use from an exception handler.
