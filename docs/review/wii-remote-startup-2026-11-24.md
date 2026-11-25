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
