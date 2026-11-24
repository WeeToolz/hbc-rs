# Production VI and GX video APIs in Dolphin

The production `video-platform-api` case checks VI/GX initialization,
framebuffer information, solid and textured 2D setup, frame setup, a solid
rectangle, a fixed 4 by 4 RGBA8 texture, EFB capture, presentation, retrace
wait, framebuffer show, and shutdown.

The first acceptance DOL failed a combined EFB dimensions or pixel check with
result -5. That check did not identify which part failed. The acceptance case
was changed to report the parts separately. Two runs of the revised solid-only
case passed. The expanded solid-and-textured case then passed twice in fresh
Dolphin 2609a profiles. The solid and textured center pixels matched their
expected RGBA values in both runs. The initial failure remains in the evidence
subdirectory.

The latest build manifest records the DOL and source revisions. The API
inventory records tested methods as partial, since Dolphin does not prove
physical Wii VI timing, GX cache behavior, or scanout. EFB-to-RGBA capture is a
Rust diagnostic extension; Reborn has no matching public API. The attempted
repeat using the old DOL with a different custom ELF was rejected before
launch and is recorded separately.
