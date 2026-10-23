# Normal launch trace — 2026-10-22

The exact DOL from the large-text run is
`/Volumes/SSD/WiiDev/Devtools/reports/evidence/2026-10-22-dev-large-text/dev-large-text.dol`.
Its SHA-256 is `d3eb281901a8772ac97c3e77c603038505f2c55b6ebb3492b0bbe382bb0c17fb`.

The launch used Dolphin's `-e` option with this DOL and no application
arguments. The trace writes no arguments, caller stub, or other DOL runtime
state into emulated memory. `result.json` records the observed entry registers,
entry state, and stops.

Dolphin entered `__ogc_dev_begin` with `argc=0`. The development wrapper
requires a valid argument vector. It returned `-22` before it reached `main`.
The observed loader-exit argument was `0xFFFFFFEA` (`-22`), and recovery was
not armed. Therefore this launch did not reach the probe's video setup. This
explains the black screen for a direct Dolphin launch without HBC arguments.

The earlier controlled Dolphin run used the same DOL hash and reported
`entry_stage=6`, `entry_result=0`, `large_text_pixels_verified=true`, and
`stub_entry_observed=true`. That run supplied report and probe arguments and a
controlled caller stub. It shows the probe's text and caller-stub path after a
valid development launch setup. It does not validate a direct Dolphin launch
without those inputs or prove a full return to HBC.

The trace script is `trace_normal_dol_launch.py`. It runs Dolphin with an
isolated profile and local GDB port. The sandbox blocked Dolphin before startup
because its Qt build required NEON; the same read-only emulation trace then ran
outside the sandbox. No Wii upload, persistent device write, or WAD operation
was performed.

The physical report remains separate: the Wii displayed the text and reached
the caller-stub entry observer, then failed to return to HBC. This Dolphin trace
does not resolve that return failure.
