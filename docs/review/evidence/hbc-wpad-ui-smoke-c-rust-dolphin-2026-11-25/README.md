# HBC emulated Remote input smoke check

Both captures completed with Dolphin 2609a using the same isolated NAND copy
source and the same Pipe-backed emulated Wii Remote input sequence. The C
reference DOL SHA-256 is
`a32c35c20e2a18ba3004aefbcea9ac10af555f0d068241e9332d6840b28fe5f6`. The Rust
DOL SHA-256 is
`6d67360c4b579e787256d7c80a8f915a421fe4dcbce585f7570adea0f17ba8d9`.

The capture sequence moved the pointer to the center and both corners, pressed
Remote button 1, clicked an app, and pressed Remote button 2. The screenshots
show input reaching each application. The C and Rust screens differ in their
existing menu layout and content; this run does not claim full UI parity.

This is emulated input only. It does not prove physical Bluetooth pairing,
Remote reports, IR behavior, native GameCube multi-port input, or startup time
on Wii hardware. The GameCube port mapping change is covered by host tests and
the Wii target build; actual ports and simultaneous controller behavior still
need a physical Wii with GameCube ports.

The failed sandboxed Dolphin launches are not included. They exited before
input testing. The successful runs required normal macOS app access and used
the x86_64 Dolphin process because the default ARM process exited before the
first screenshot.
