# Wiiload bounded decode in Dolphin

The final `emulator-acceptance` DOL passed the `wiiload-header` case in
Dolphin 2609a with result 0. This case decodes a deterministic 96 KiB payload
whose zlib stream spans many 1 KiB input chunks. It exercises the production
`WiiloadHeader::decode_body` API, exact output-size checks, and the final
bounded decoder implementation. The matching DOL SHA-256 is
`69b2f7f7b812cda2bb4185d8e8c64cf6ac46d6a552820af6737df98a5e79fac3`.

The normal PowerPC release ELF also passes `tools/check_wiiload_stack.py`.
The worker stack is 32 KiB; the measured direct path uses 2,080 bytes and
leaves 30,688 bytes. See `wiiload-stack-audit.json` for the frame report and
`build-manifest.json` for source and SDK revisions.

The Dolphin acceptance checks the decoder API. It does not exercise socket
receive, transfer progress, IOS cache or DMA behavior, or application entry
from a live upload. Those checks, including physical Wii validation, remain
pending.
