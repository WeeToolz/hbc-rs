# Wii startup state API in Dolphin

The production `wii-startup-state-api` acceptance case passed in Dolphin 2609a.
It calls `WiiStartup::initialize(640, 480)` and checks the no-media startup,
runtime state, storage guards, ZIP install rejection, icon refresh, storage
refresh, device probe worker, options handling, and unmount paths.

This case uses Dolphin with no mounted storage. It does not prove FAT-backed
storage scans, ZIP installation, theme loading, persistent options, or live Wii
device probing. Keep those checks pending for physical Wii validation.

The case result is in `result.json`. The DOL SHA-256 is
`403e1fa1bf8d0a8e8a107d01746844add59159c54a6c4d48844c138c2ac35f26`.
