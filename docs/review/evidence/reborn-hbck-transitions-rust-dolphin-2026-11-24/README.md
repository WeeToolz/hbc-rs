# HBCK menu transitions in Dolphin

The focused diagnostic HBC build passed the remote input test in Dolphin 2609a
with a private NAND copy. The test sent `dh` over HBCK; the closed menu dropped
`d` and the application reported Browser to MainMenu. It then sent `da` and
reported MainMenu to About. Existing HBCI input checks also passed. The screen
change messages, result, and test output are included. The DOL SHA-256 is in
`result.json`. The diagnostic feature only adds a report on screen changes.

An earlier build using the broader startup diagnostic feature reached its first
frame and initialized networking, but the listener did not become ready before
the deadline. Its result and raw Dolphin log are also included. This run did
not test key delivery. Two earlier attempts ran out of temporary storage during
NAND copying and did not produce a test result.

This proves the recorded transitions in Dolphin. It does not prove Reborn's
full overlay behavior, the 1, 2, and w keys, rendering parity, or physical Wii
network and input behavior.
