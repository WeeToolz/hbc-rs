# Interrupted Wiiload reads in Dolphin

The production application ran in Dolphin 2609a with a private NAND copy.
The test sent incomplete Wiiload headers, data, and arguments in separate
connections, then closed each client write side. It required each connection
to close within three seconds and sent a valid empty HBCK request after each.

The earlier build timed out on the incomplete header. The fixed build closed
all three connections and answered each following request. It also passed the
interrupted HBCK, normal HBCK, and existing HBCI checks. The captured results
and terminal output are included; each result records its DOL hash.

This covers these uncompressed TCP interruptions in Dolphin. Compressed data,
other failure timing, and physical Wii networking remain open.
