# Reborn HBCK reply checks

On 2026-11-24 the production HBC application DOL ran in Dolphin 2609a using
a private copy of the test NAND. The existing remote input script sent HBCK
requests over TCP in split writes. Empty, filtered, HOME, Back, and 64-byte
payloads returned the eight-byte success reply. A 65-byte length returned
status -22 and zero payload length. The existing HBCI input checks also passed.
The captured script result and terminal output are included here.

The DOL SHA-256 is recorded in `result.json`. Host tests check queue filtering,
capacity, pacing, and the shared input mapping through the menu controller.
The Dolphin test does not observe the resulting menu transition. The Rust HOME
menu differs from Reborn's overlay; keys 1, 2, and w have no equivalent action.
Full HBCK behavior and physical Wii network and input behavior remain open.
