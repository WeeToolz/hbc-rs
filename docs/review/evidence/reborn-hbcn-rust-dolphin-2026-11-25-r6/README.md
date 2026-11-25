# HBCN target Dolphin evidence on the crash-report build

The final Rust DOL passed the HBCN target checks in Dolphin on 2026-11-25. The
test set port 4407, checked HBCS `log`, then cleared the target and checked for
`null`. `result.json` records the SHA-256, target `192.168.1.52:4407`, and a
successful named `reborn-netlog` case. The DOL SHA-256 is
`47ab7b49f39dd9515b3e615f5e7e6f2069052933876d189df33ba62916169131`.

The matching Reborn C control passed HBCN set, HBCS report, and clear in the
prior run at
[`../reborn-hbcn-c-control-dolphin-2026-11-25-r2/result.json`](../reborn-hbcn-c-control-dolphin-2026-11-25-r2/result.json).

Rust restores the target before heap setup. Its heap remains below Reborn's
persistent target, crash, and retained-log records. App log connection and
return remain unverified, as do physical Wii cache and memory behavior.
