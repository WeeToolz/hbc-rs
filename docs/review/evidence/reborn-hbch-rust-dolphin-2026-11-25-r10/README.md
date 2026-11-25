# Reborn HBCH Rust Dolphin check

The isolated Dolphin check passed for Rust DOL SHA-256
`533b535c33e01a82193eb484d982f6afdc55dc6651bce06312aa0b273c5e9e36`.
The machine result is in [result.json](result.json). It checked the zero mask,
each page bit, a combined Video and Network mask, and an unknown-only mask.

The Storage page reported the mounted SD image as `SD, 0.6 GB, 0.5 GB free`.
It reported memory card A as `2043 blocks (128 Mbit)` when selected alone and
memory cards A and B as `None` for the no-card state. These checks confirm the
Rust formatting and Dolphin API values. They do not replace checks on a Wii.

Wii U mode behavior, a matched C/Rust System-page comparison, and real Wii
device values remain open. The local C channel DOL did not start its listener
in the same Dolphin check. The raw Dolphin process log is preserved in
[process.log](process.log).
