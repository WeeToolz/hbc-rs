# Interrupted compressed Wiiload requests in Dolphin

The production application ran in Dolphin 2609a with a private NAND copy.
The test sent a truncated zlib stream and, in a separate connection, a complete
zlib stream with truncated arguments. Each client closed its write side. The
receiver closed each connection within three seconds and answered a new HBCK
client after each. The same run passed uncompressed interruptions, interrupted
HBCK, normal HBCK, and existing HBCI checks. The result records the DOL hash.

The first run passed its request checks but required Dolphin to be forcibly
terminated; its result is marked unsuccessful and is included. The SSD was
nearly full. The repeated run used temporary space on the system volume and
completed with normal cleanup. Its result and output are included.

This checks the stated interruption points in Dolphin. Other compressed stream
errors and physical Wii networking remain open.
