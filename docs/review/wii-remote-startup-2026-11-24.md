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
data format for a slot before a remote connects. The Rust PowerPC backend
currently returns without storing the format when a channel is disconnected.
This can affect the reports requested after connection. It does not establish
why button input failed on the user's Wii. Test both implementations with the
same Dolphin input setup before changing the library contract, then verify the
result on a Wii.

The startup delay also needs measurement. Do not treat the setup change as a
confirmed fix for either reported symptom.
