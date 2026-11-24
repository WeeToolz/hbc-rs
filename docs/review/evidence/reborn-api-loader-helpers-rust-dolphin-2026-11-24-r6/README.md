# Loader helper acceptance in Dolphin

The production `loader-helpers` case passed in Dolphin on 2026-11-24. The
case checks the reported DOL header values: section offset 48 and size 48.
`parse_dol` returns `DolHeaderOverlap` for this malformed header.

The acceptance DOL SHA-256 is
`194233a9230f28a753c01aed82e34a277ae0d25ee98e997a5eb2600332d8b309`.
The result and Dolphin log are saved beside this file. Dolphin ran through
macOS Terminal because direct launches did not reach debugger startup.

This test confirms the parser guard. It does not show why the Wii read path
supplied these bytes. Capture the DOL bytes immediately before parsing on the
Wii to investigate that failure.
