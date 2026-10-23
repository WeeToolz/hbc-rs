# Latest DOL normal launch — 2026-10-22

This trace uses the latest DOL from the return-stub fix build:
`/Volumes/SSD/WiiDev/Devtools/reports/evidence/2026-10-22-return-stub-address-fix/fresh-build/dev-stub-address-fix-fresh.dol`.
Its SHA-256 is `102d0aae30b4d21dda938304920fdbaf994ce69688298db945943f055444a8c1`.
This differs from the earlier large-text DOL, whose SHA-256 is
`d3eb281901a8772ac97c3e77c603038505f2c55b6ebb3492b0bbe382bb0c17fb`.

The normal launch used Dolphin's `-e` option with the latest DOL and no
application arguments. The trace did not write the argument vector, caller stub,
or other runtime state into emulated memory. See `result.json` and
`trace_latest_dol_launch.py` for the observations and the exact run procedure.

Dolphin entered `__ogc_dev_begin` with `argc=0`. The entry result became
`-22`, and execution reached `sys_exit_to_loader` without reaching `main`.
Recovery did not arm. The DOL therefore did not reach application video setup.
This is the first observed failure in the direct Dolphin launch.

The controlled Dolphin run for this same latest DOL reports the same DOL hash,
`entry_stage=6`, `entry_result=0`, visible text verification, and caller-stub
entry observation. That test provided development arguments and a controlled
caller stub. The two runs have different launch conditions. The controlled run
shows that the DOL can display text and reach its caller-stub observer after a
valid development launch setup. It does not validate a normal launch without
those inputs or prove a complete return to HBC.

The earlier physical large-text finding remains separate. The Wii displayed
that earlier probe and reached its caller-stub entry observer, but did not
return to HBC. This trace does not resolve that failure. No Wii upload,
persistent device write, or WAD operation was performed.
