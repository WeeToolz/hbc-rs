# Wii Remote startup review

The user reported that two HBC Rust DOLs did not accept Wii Remote input on a
physical Wii. GameCube input worked. Startup also appeared slow without the
GameCube controller. These observations do not identify a cause.

The local HBC Reborn C source configures the data format and pointer resolution
for each of four channels, then sets the idle timeout. Rust now attempts each of
these calls in the same order. It records failures and returns the first error.
A host test checks the call order and the failure path. A Wii target build
checks that the production path compiles. Dolphin and physical Wii tests of
this change remain open.

There is a separate library difference that needs a controlled test. The local
C libogc allocates Wii Remote slots during initialization and stores a requested
data format for a slot before a remote connects. The Rust PowerPC backend had
discarded the format while disconnected. The library now stores it for the
later connection. A Dolphin probe checked format API results without a remote;
it did not check connected reports. This difference does not establish why
button input failed on the user's Wii. Check a later connection in Dolphin and
verify the result on a Wii.

The startup delay also needs measurement. Do not treat the setup change as a
confirmed fix for either reported symptom.

## Physical Wii report for the updated debug DOL

On 2026-11-25, the user reported that the updated DOL ran on the Wii, but the
Wii Remote did not work and the app did not connect to the network. The sent
DOL SHA-256 was
`45be2cc4b15f6ff923a5bcad431bcf5afe07bf6028f4b0abc034a56612450307`.
Its build manifest records HBC `50908712672227da30d0dba3def4fd6369ae1824`,
libogc-rs `e28e6af5fd9992ce822c200fab8ae4c26b71a5eb`, and the
`debugger-network` feature. Wiiload reported that the host wrote the complete
transfer; the user's observation establishes the subsequent behavior.

The Bluetooth and network causes are not yet known. Dolphin's successful
emulated Remote test does not establish physical adapter, pairing, or network
startup behavior. Preserve the Wii state and obtain the Remote light behavior,
controller availability, network indicator, and startup diagnostics before
attributing either failure to a specific code path.

## 2026-11-25 input parity correction

The supplied parity audit says C HBC reads all four GameCube ports. Direct
inspection of `channel/channelapp/source/controls.c#controls_scan` shows that
it reads `PAD_ButtonsDown(0)`, `PAD_ButtonsHeld(0)`, and `PAD_ButtonsUp(0)`.
`controls_sticky` also reads `PAD_StickY(0)`. Rust's port-zero mapping matches
this source, so no four-port mapping change was made.

The same C source review confirms that a valid pointer owner supplies all
three button transition masks. If no Remote owns the pointer, C combines masks
from all probed Remotes. Rust previously filtered non-owner down and held
masks, but still exposed non-owner releases. The production mapper now applies
the owner rule to releases as well. Its host regression checks both owner and
no-owner cases. The HBC runtime has no local button latch that needs a
non-owner release for cleanup; Rust's separately queued remote-input path
keeps its own release handling. Wii Remote ownership handoff remains unverified
on hardware.

Rust returned an empty input frame when the controller manager reported a Wii
Remote poll error. C HBC continues its input scan, and the manager already
records the source error. Rust now keeps valid PAD and USB input for that frame
and retains the recorded error. Host tests and a Wii PowerPC build pass. These
checks do not establish physical Remote connection or button behavior.

The rebuilt debug-network DOL is
`target/hbc-rs.dol` (2,490,368 bytes; SHA-256
`0f996fbfeb459a91968170773331a67e639d330bc348b21f6bb2e88eff5da0a8`). It was
built from HBC revision `c504fb3` with the WPAD poll and pointer-owner fixes
in this report, and libogc-rs `e28e6af5fd9992ce822c200fab8ae4c26b71a5eb`,
using `debugger-network`. It has not passed the required automatic recovery
qualification in Dolphin, so it is not cleared for Wii upload. The physical
Wii Remote and network reports remain unresolved. The current DOL also did not
pass the existing Dolphin Wiiload/TCP input script: Dolphin logged IOS network
startup result `0`, socket startup result `0`, and receiver state `Listening`,
but the host connection to port 4299 was refused before the HBCI input check.
The run used a private copy of the local Dolphin NAND. This is an incomplete
network test, not evidence that the input change passed at runtime. Raw output
is in `docs/review/evidence/hbc-input-parity-rust-dolphin-2026-11-25-r2/`.
