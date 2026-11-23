# Wiiload bounded decode acceptance in Dolphin

The `wiiload-header` acceptance case passed in Dolphin 2609a. Its DOL SHA-256
is `523a5a43e5094c0a8d468f7a903d35ef9c5471b2eb76758245e102622b97a624`.
The case used a deterministic 96 KiB payload. It checked zlib decoding through
multiple bounded input chunks and exact output length through the production
`WiiloadHeader::decode_body` API.

This run did not open a TCP socket or exercise the receiver worker. It also
preceded the final reduction of the input chunk size from 8 KiB to 1 KiB and
the move of decompressor state to a fallibly allocated heap buffer. Host tests
pass for that final implementation. Its normal 32 KiB PowerPC stack audit and
a fresh Dolphin DOL run remain pending because the build volume ran out of
space during the final target link. No physical Wii test was run.
