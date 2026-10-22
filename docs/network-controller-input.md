# Network controller input

Rust HBC accepts controller commands on TCP port 4299. This is the same
listener that accepts Wiiload uploads. A command does not require a DOL upload.
The new DOL must be running before these commands can work. An older build
cannot receive this feature through a command.

The network input goes into the normal HBC input frame. It controls HBC,
including its menus and notices. It does not send input to an app after HBC
has transferred control to that app. Button bindings are the same as local
controllers. A physical button with no HBC binding has no menu action.

## Use the sender

Run these commands from `RustPorts/hbc-rs`:

```sh
# Open HOME with a Wii Remote HOME button.
python3 tools/hbc-input.py --wii 10.0.0.242 --kind wiimote home

# Press a logical HBC direction or confirm button.
python3 tools/hbc-input.py --wii 10.0.0.242 down
python3 tools/hbc-input.py --wii 10.0.0.242 a

# Hold a combination for one second, then release it automatically.
python3 tools/hbc-input.py --wii 10.0.0.242 --kind gamecube --hold-ms 1000 l+r

# Move the pointer to logical screen coordinates and click.
python3 tools/hbc-input.py --wii 10.0.0.242 --pointer 320 240 a

# Send normalized sticks and analog triggers.
python3 tools/hbc-input.py --wii 10.0.0.242 --kind generic \
  --left-stick 0 20000 --right-stick 0 -16000 --triggers 0 200

# Release all network input immediately. Local controls remain active.
python3 tools/hbc-input.py --wii 10.0.0.242 --release

# Request the normal HBC screenshot operation.
python3 tools/hbc-input.py --wii 10.0.0.242 --screenshot
```

The default hold is 120 ms. A hold can be 1 through 2000 ms. Refresh the
command before it expires to keep buttons held. Send `--release` between two
commands if they must produce separate presses before the first hold expires.
A connection loss cannot leave a network button held without a time limit.
A release command uses the same queue as other commands.

Supported kinds are `logical`, `gamecube`, `wiimote`, `classic`, `nunchuk`,
`guitar`, `generic`, `joycon-left`, `joycon-right`, `joycon-pair`, `switch-pro`,
and `xbox`. Generic uses the normalized USB/Bluetooth HBC bindings.
Use `--mask 0x...` to send raw button combinations. For Classic and extension
buttons, use the full WPAD mask, with extension bits in the upper 16 bits.
The sender lists valid named buttons if a name is invalid. Logical names are
`up down left right a b home 1 2 minus plus`.

Stick values are -32767 through 32767. Positive X is right and positive Y is
up. HBC scrolls memo text with left-stick Y. The Right Joy-Con right stick also
uses the local 8192 threshold for directional navigation, in addition to memo
scrolling from its Y axis. Other normalized right-stick directions do not create
menu actions; the stick values remain valid in the command.
`--scroll` adds -512 through 512 logical pixels per frame. Analog triggers
are 0 through 255; 128 or greater presses the mapped L or R button on
GameCube, Classic, and normalized controller kinds. Pointer coordinates are logical screen coordinates, 0 through 4095.
HBC applies its normal hit testing. Screenshot completion uses the existing
storage error and notice path; an input reply does not confirm a saved image.

Local input remains active. The adapter merges held states, suppresses duplicate
presses, and prevents a release from one source from releasing the other source.
A remote pointer owns pointer input only while that command is active.

The reply confirms that the main loop merged the input into its frame. It does
not confirm that a menu action, reset, storage write, or launch completed.
Invalid, busy, expired, pending, stopped, incomplete, and socket-error results
are not reported as completed input. A pending result means that the main loop
has not confirmed consumption; it does not prove that the command was cancelled.

This service uses the Wiiload listener's existing peer subnet restriction.
It has no separate authentication. Use it on a trusted development network.
It cannot work when that listener is stopped or its port refuses connections.

## Protocol version 1

Send exactly 32 bytes, in network byte order. The `HBCI` prefix selects input;
`HAXX` still selects a normal Wiiload upload. The worker does not show an upload
progress dialog for an input command.

| Offset | Bytes | Value |
| --- | --- | --- |
| 0 | 4 | ASCII `HBCI` |
| 4 | 1 | Version 1 |
| 5 | 1 | Kind: logical 0, GameCube 1, Wii Remote 2, Classic 3, Nunchuk 4, guitar 5, generic 6, left Joy-Con 7, right Joy-Con 8, Joy-Con pair 9, Switch Pro 10, Xbox 11 |
| 6 | 2 | Flags: bit 0 pointer valid, bit 1 screenshot; all other bits zero |
| 8 | 4 | Button mask |
| 12 | 2 | Hold duration in ms; zero releases all network input |
| 14 | 2 | Signed pointer X |
| 16 | 2 | Signed pointer Y |
| 18 | 2 | Signed scroll pixels per frame |
| 20 | 2 | Signed left-stick X |
| 22 | 2 | Signed left-stick Y |
| 24 | 2 | Signed right-stick X |
| 26 | 2 | Signed right-stick Y |
| 28 | 1 | Left trigger; GameCube, Classic, or normalized kinds |
| 29 | 1 | Right trigger; GameCube, Classic, or normalized kinds |
| 30 | 2 | Reserved, zero |

Logical button bits are 0 up, 1 down, 2 left, 3 right, 4 confirm, 5 back,
6 HOME, 7 browse mode, 9 previous page, 10 next page, and 11 options.
Normalized kinds use `ogc_controller::Buttons`. Other kinds use the local
PAD or WPAD masks. An all-release packet has no flags, buttons, pointer,
scroll, sticks, or triggers. Pointer X/Y must be zero when its flag is absent.

The eight-byte reply is `HBCI`, version byte 1, a status byte, then two zero
bytes. Status 0 means merged into the main-loop frame; 1 invalid; 2 busy;
3 expired before consumption; 4 no completion within 2500 ms; 5 stopped.
A queue accepts one pending command. It does not overwrite an unread command.
The main thread never waits for the worker to release a lock. Expired unread
commands do not produce a press. Input holds expire in the main loop even
when the worker is busy with another connection.

## Verification

```sh
cargo test --locked --lib
python3 tools/test_hbc_input.py
python3 tools/test_remote_input_dolphin.py --host MAC_LAN_IP
```

The Rust tests use the production parser, mailbox, mappings, and transitions.
They include the real runtime menu transition for a network HOME command.
The Python tests use a controlled socket peer to check the production sender,
including fragmented replies, errors, and EOF. The Dolphin test uses the
production DOL socket and requires main-loop completion for actual packets.
These checks do not replace a real Wii input check.


The current normal DOL used for the host checks has SHA-256
`4f4984563818fc31d48d2426afc37fe5310a9e3b6f59dc55449369d1fa0521e0`.
The isolated Dolphin check sent fragmented HOME, invalid-version, release,
pointer, and stick packets to the production receiver. It received main-loop
completion for valid commands and an invalid status for the malformed one.
The Wii has not been tested with this version of the input protocol.

## Direct system commands

See `terminal-control.md` for restart, RESET, Power, Eject, SYNC, settings,
screenshots, and result polling. The `hbc-control.py input` command forwards
these same controller packets without a DOL upload. GameCube and Classic
controllers also accept analog trigger values. Guitar input has fret and
strum names; normalized controllers have Xbox button aliases.
