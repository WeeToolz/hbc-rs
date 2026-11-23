# Wiiload TCP self-load in Dolphin

The test passed in Dolphin with the production `debugger-network` HBC WAD.
HBC booted from the WAD, then Wiiload-RS sent the matching 2,492,928-byte DOL
over TCP 4299. The sender compressed the transfer to 1,351,033 bytes and
returned success. HBC reached the child startup marker and reported its
Wiiload listener as listening. The child session changed from 1036 to 4482.

The automated result also verifies startup before and after upload, the
transfer-control marker, and clean Dolphin shutdown. `upload.json` records the
input DOL hash and compressed transfer. `dolphin.log.gz` and `upload.log.gz`
preserve the captured log bytes; `wad-startup.png` and `child-startup.png` show
the browser before and after the self-load. The profile used a disposable copy
of the Dolphin NAND. No physical Wii was involved.

WAD SHA-256:
`05f6ef8f59d678766ec3618bba75a8a7459668501332c3c7807798b9028db817`.
DOL SHA-256:
`c629626a88fb6a20c0677c8f5cea60261184916f1b5779e2887ce56e65a6e1c2`.
See `build-manifest.json` for file hashes and build identity, and `result.json`
for timings and test results.
