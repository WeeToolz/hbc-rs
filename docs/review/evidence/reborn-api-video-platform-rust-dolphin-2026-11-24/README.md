# Production VI and GX video APIs in Dolphin

The production `video-platform-api` case checks initialization, framebuffer
information, solid 2D configuration, frame setup, rectangle drawing, EFB
capture, presentation, retrace wait, show, and shutdown. Dolphin 2609a ran the
case three times on 2026-11-24. The first DOL failed its combined framebuffer
dimension or pixel check with result -5. That check did not show which part
failed. The acceptance code was updated to report those cases separately. Two
fresh runs using the revised DOL passed. Their DOL hashes match.

The first failed result and both later results are kept in the child evidence
directories. The repeated pass is useful evidence, but the unresolved first
result keeps EFB pixel stability partial. Repeat this check and verify physical
Wii GX cache behavior, VI scanout, and retrace timing before claiming hardware
parity. The EFB capture helper is a Rust diagnostic extension; Reborn has no
matching public API.
