# Executable source and handoff APIs in Dolphin

The fresh `launch-dol` and `launch-elf` production acceptance cases passed in
Dolphin with result 0. Before staging either fixture, the cases check the
production Arena2 source allocator: zero and overflowing lengths fail, a
64-byte allocation is 32-byte aligned, mutable writes match the immutable
view, and dropping the source restores Arena2Hi.

The launch path keeps using `LaunchSource::from_vec`, `as_slice`,
`stage_launch`, and `transfer_control`. The test child checks initialized text
and data, cleared BSS, and entry transfer. The result and DOL hash are in
`result.json`; the build identity is in `build-manifest.json`.

This is Dolphin evidence. It does not establish Arena2 behavior, Broadway
cache coherency, IOS reload behavior, or application handoff on a physical
Wii. The complete DOL/ELF input format space also remains open.
