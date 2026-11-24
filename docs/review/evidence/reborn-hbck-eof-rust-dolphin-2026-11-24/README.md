# Interrupted HBCK request in Dolphin

The production application ran in Dolphin 2609a with a private NAND copy.
The test sent an HBCK header declaring two bytes, sent one byte, and closed
the client write side while waiting for a reply. Before the change, the client
timed out after three seconds. The captured failed result is included.

After the change, the receiver returned status -22 and a zero payload length
within the same deadline. The same run then passed empty, filtered, HOME,
Back, 64-byte, and oversized HBCK reply checks and existing HBCI checks. The
result and test output are included. DOL hashes are in the result files.

This checks the socket path in Dolphin. It does not prove every interrupted
Wiiload case or physical Wii networking. The test output and the result files
are preserved as captured.
