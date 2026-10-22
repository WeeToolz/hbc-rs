# Terminal control of Rust HBC

The running HBC build accepts direct system commands and controller input on
its existing TCP port 4299. A command does not upload a DOL. Load the updated
`target/hbc-rs-terminal-control.dol` once before using this service. An older
build cannot gain these commands through a network request.

## Run through Terminal on this Mac

Open `tools/hbc-terminal.command` in the signed-in Terminal app. It prints the
commands, then asks for one command and its arguments. For example, enter
`restart`, `eject`, or `input --kind wiimote home`.

From an agent, open the file through Terminal:

```sh
/usr/bin/open -a /System/Applications/Utilities/Terminal.app \
  /Volumes/SSD/projects/Wii/wiidev/RustWii/RustPorts/hbc-rs/tools/hbc-terminal.command
```

`tools/hbc-restart.command` sends one direct restart request without a DOL.
`tools/send-terminal-control.command` is the separate, one-time upload of the
updated HBC build. It uses reference Wiiload and then waits for the new command
service. Do not use the old `run-reset-to-menu.command` to test this service;
that file uploads a diagnostic application.

In a normal Terminal session, run from `RustPorts/hbc-rs`:

```sh
python3 tools/hbc-control.py --wii 10.0.0.242 status
python3 tools/hbc-control.py --wii 10.0.0.242 restart
python3 tools/hbc-control.py --wii 10.0.0.242 reset
python3 tools/hbc-control.py --wii 10.0.0.242 power-off
python3 tools/hbc-control.py --wii 10.0.0.242 eject
python3 tools/hbc-control.py --wii 10.0.0.242 sync
```

The default Wii address is `10.0.0.242`. `HBC_WII`, `WII_IP`, or `--wii`
can supply another address. `--wii`, `--port`, and `--timeout` go before the
command. `status` reads the last retained request and its result. `wait` waits
for this Rust command service; it does not submit hardware work.

## Commands and Wii front buttons

| Terminal command | Production operation and completion check |
| --- | --- |
| `restart` | Normal HBC cleanup, then `SYS_RESTART`. A new HBC session confirms that HBC restarted. A disconnected socket alone does not confirm a console reset. |
| `reset`, `system-menu` | The physical RESET callback's return-to-menu action, through HBC cleanup and `SYS_RETURNTOMENU`. |
| `power-off`, `power` | The normal Power action, through HBC cleanup and `SYS_POWEROFF`. Also covers the Wii Remote Power action. |
| `eject` | Read disc presence, call the real DI eject operation, then wait until the drive reports disc absence. An empty drive is reported as an error. |
| `sync` | Call the SDK's short SYNC/pairing operation and wait for search completion and a new controller connection. |
| `find-remotes` | Start Wii Remote discovery and wait for search completion and a new connection. |
| `stop-sync` | Returns an unsupported error: the SDK does not expose hardware completion of inquiry cancellation. No cancellation is submitted. |
| `disconnect-remotes` | Request disconnection of connected Bluetooth controllers and wait for their actual disconnect events. |
| `save`, `save-settings` | Run the real dirty-settings writer. Report write errors. Clean settings report that no write was needed. |
| `screenshot` | Capture the real framebuffer, encode PNG, and save through the normal storage backend. Return the saved path or the actual error. |
| `restart-network` | Stop and start the production receiver. Wait for Listening state and an assigned IP address. |
| `home`, `close` | Send HOME or B through the normal input path. These are button presses; their reply confirms main-loop consumption. |
| `input ...` | All controller input options from `hbc-input.py`. |

The software Power command is the normal shutdown action. It does not emulate
holding the electrical Power button to force the console off. SYNC does not
perform the long-press operation that deletes saved controller pairings.

`restart-app` and `return-hbc` are recognized, but return an explicit unsupported
error in the HBC browser. These actions belong to a running application's host
integration. HBC cannot control an unrelated DOL after transferring control to
it. Implementing an app-side command service remains open work. No command
claims that these operations ran.

## Controller input

The `input` command uses the existing HBC input packet and the same mappings
as local controllers. Every button can be named or supplied with `--mask`.
The supported families are logical HBC input, GameCube, Wii Remote, Classic,
Nunchuk, Guitar Hero, generic USB/Bluetooth, left/right/paired Joy-Con,
Switch Pro, and Xbox.

```sh
python3 tools/hbc-control.py input --kind wiimote a
python3 tools/hbc-control.py input --kind wiimote home
python3 tools/hbc-control.py input --kind wiimote --pointer 320 240 a
python3 tools/hbc-control.py input --kind gamecube --left-stick 0 20000
python3 tools/hbc-control.py input --kind gamecube --right-stick 16000 0
python3 tools/hbc-control.py input --kind gamecube --triggers 0 255
python3 tools/hbc-control.py input --kind classic zl+zr
python3 tools/hbc-control.py input --kind nunchuk c+z
python3 tools/hbc-control.py input --kind guitar green+strum-down
python3 tools/hbc-control.py input --kind xbox lb+rb
python3 tools/hbc-control.py input --hold-ms 1000 down
python3 tools/hbc-control.py input --release
```

GameCube and Classic analog triggers use the same L/R page bindings at the
128 threshold. Normalized controller kinds retain their local mappings.
Xbox aliases include `back`, `menu`, `lb`, `rb`, `lt`, `rt`, `l3`, and `r3`.
Guitar input includes the five fret buttons and strum up/down. Base Remote
buttons remain available with Nunchuk and guitar extensions.

Input holds expire after at most two seconds. Use release between repeated
presses. Local controls remain active. The pointer uses HBC logical screen
coordinates. The C-stick and other sticks preserve HBC's existing behavior;
a stick direction that HBC does not bind does not gain a new action.

HBC uses the IR pointer position rather than raw sensor images. It does not
consume accelerometer, gyroscope, or arbitrary raw sensor reports in its menu.
This service controls the inputs HBC uses; it does not create a virtual hardware
controller for another game. Physical buttons with no HBC binding can be sent,
but have no HBC action. See `network-controller-input.md` for all masks and axes.

## Results and failure behavior

The worker validates and queues one command. The main thread executes it only
when storage and upload workers do not own required resources. Queued commands
expire after five seconds. The server retains the most recent result. A repeated
request ID returns the retained result without executing the command again;
using that ID for a different command is invalid. A second active request is busy.

A queued or running reply is not success. The client polls for the final result,
and does not resend a hardware action after a lost reply. Read, write, pairing,
disconnect, and storage errors reach the terminal. Hardware operations have a
60-second completion deadline. A command timeout is an error, not a pass.

Restart, System Menu, and power-off can remove the command server before a
final reply. The client reports completion as unconfirmed unless it observes
a new HBC session for `restart`. The physical boot sequence or a powered-off Wii
still needs console-side observation. The software cannot reset a fully stalled
main thread; a queued command expires rather than running later without notice.

The existing peer-subnet restriction applies. The service has no separate
authentication and is intended for the same trusted development network as
Wiiload. No internet port forwarding is configured by these tools.

## Control protocol version 1

Requests are exactly 16 bytes. `HBCC` submits work; `HBCQ` queries it.

| Offset | Bytes | Value |
| --- | --- | --- |
| 0 | 4 | `HBCC` or `HBCQ` |
| 4 | 1 | Version 1 |
| 5 | 1 | Command code, or zero for a query |
| 6 | 2 | Reserved, zero |
| 8 | 4 | Request ID, big endian. Submissions require a nonzero ID. Query ID zero reads the last result. |
| 12 | 4 | Reserved, zero |

Codes 1 through 13 follow the `COMMANDS` table in `tools/hbc-control.py` and
`Command` enum in `src/remote_control.rs`. Responses have a 20-byte header and
at most 512 UTF-8 message bytes: `HBCC`, version, state, command, reserved zero,
request ID, session ID, 16-bit message length, and two reserved zero bytes.
All multibyte values are big endian. States are queued 0, running 1, completed 2,
failed 3, busy 4, invalid 5, and unknown 6. Neither queued nor running is an
operation-completion claim.

## Current verification

See `review/terminal-control-2026-10-18.md` for exact build identities, tests,
and hardware limits. The final build passes host and Dolphin checks. An earlier
command build answered on the real Wii, but subsequent status requests timed
out before a restart request could be sent. Real front-button operations and
console restart are not yet verified.
