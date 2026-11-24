# FAT-backed DOL loading in Dolphin

The production `wii-startup-dol-load-api` case passed in Dolphin on
2026-11-24. It mounts the controlled Wii SD folder, scans two DOL apps, and
loads the valid DOL through `WiiStartup::begin_selected_executable_load` and
`advance_selected_executable_load`. It then reads a second DOL with the
reported offset 48 and size 48 and confirms `DolHeaderOverlap`.

The Dolphin log shows the SD scan found both apps, the malformed DOL had the
expected header values at parse time, and the case passed. The raw run log and
result are saved beside this file. The Dolphin DOL SHA-256 is recorded in
`result.json`.

This verifies the FAT-backed production path in Dolphin. It does not prove
physical Wii SD or DMA behavior, or explain the reported bytes on the Wii.
