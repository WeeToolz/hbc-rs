# Reborn browser controller API Dolphin evidence

The `app-controller` production acceptance case passed in Dolphin 2609a. It
uses a controlled seven-entry catalog. The result and DOL build manifest are
archived beside this file; `app-controller.log` records Dolphin shutdown.

The case checks initial controller selection and page state, forward and
backward list paging, page boundary stops, three-column grid movement, visible
page contents, pointer app selection and page activation, and the selected
index. It also opens a long description, sets measured memo bounds, drags one
line, and checks that the scroll remains after pointer release. The ledger
links each API that the case calls and observes.

This is partial behavior evidence. It does not prove the rendered GX hit map,
live Wii Remote input, theme-dependent text layout, or physical Wii behavior.
The compared Reborn paths are in `source/browser.c`, `source/view.c`, and the
memo drag path in `source/main.c`.

The DOL SHA-256 is recorded in `result.json` and `build-manifest.json`. The
private Dolphin profile does not change the normal Dolphin profile.
