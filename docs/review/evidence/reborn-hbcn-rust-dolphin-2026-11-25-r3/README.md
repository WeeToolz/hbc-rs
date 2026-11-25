# HBCN target Dolphin evidence

The rebuilt Rust DOL passed the HBCN target checks in Dolphin on 2026-11-25.
The test set port 4407, checked the `log` field in HBCS, then cleared the
setting and checked for `null`. `result.json` records the SHA-256, the target
address `192.168.1.52:4407`, and a successful named `reborn-netlog` case. The
DOL SHA-256 is `8dc0e98544d26856c32afd7b049b4ae9478211596684a7061f8fb9fc6292a02d`.

The matching HBC Reborn C control passed the same set, status, and clear
checks. Its result is in
[`../reborn-hbcn-c-control-dolphin-2026-11-25-r2/result.json`](../reborn-hbcn-c-control-dolphin-2026-11-25-r2/result.json).
The C DOL SHA-256 is
`a32c35c20e2a18ba3004aefbcea9ac10af555f0d068241e9332d6840b28fe5f6`.

Rust restores the target record before heap setup. The heap stays below the
Reborn target, crash, and retained-log records in MEM2. Unit tests check the
record layout, checksum, invalid records, and heap boundary.

An app handoff probe did not complete. A prior Rust run reached the app staging
step but the app did not connect to the host listener. A matching C run ended
before HBC finished receiving the probe. These runs do not prove or disprove
target restoration across an app return.

This evidence covers only HBCN set, HBCS reporting, and clear in Dolphin. It
does not cover HBCL retained-log replies or physical Wii cache and memory
behavior. Physical Wii validation remains pending.
