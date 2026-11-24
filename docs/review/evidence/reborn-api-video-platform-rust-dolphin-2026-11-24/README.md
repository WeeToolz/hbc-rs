# Production VI and GX video APIs in Dolphin

The production `video-platform-api` case checks VI/GX initialization,
framebuffer information, solid and textured 2D setup, frame setup, a solid
rectangle, a fixed 4 by 4 RGBA8 texture, EFB capture, presentation, retrace
wait, framebuffer show, and shutdown. Run r7 also draws the HOME strip chrome
and tests the screenshot PNG pipeline.

The first acceptance DOL failed a combined EFB dimensions or pixel check with
result -5. That check did not identify which part failed. The acceptance case
was changed to report the parts separately. Two runs of the revised solid-only
case passed. The expanded solid-and-textured case then passed twice in fresh
Dolphin 2609a profiles. The solid and textured center pixels matched their
expected RGBA values in both runs. The initial failure remains in the evidence
subdirectory.

Run r7 adds known panel and selected-item EFB checks after the HOME strip draws.
It also checks that the screenshot API refuses to capture when storage is
unavailable, then captures, encodes, saves through a callback, and decodes an
image with the known panel pixel. The result and build manifest are in the
[r7 evidence directory](../reborn-api-video-platform-rust-dolphin-2026-11-24-r7/README.md).

The latest build manifest records the DOL and source revisions. The API
inventory records tested methods as partial, since Dolphin does not prove
physical Wii VI timing, GX cache behavior, scanout, or SD writes. EFB-to-RGBA
capture is a Rust diagnostic extension; Reborn has no matching public API. The
attempted repeat using the old DOL with a different custom ELF was rejected
before launch and is recorded separately.
