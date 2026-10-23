# Dolphin recovery-validation failure — 2026-10-22

The focused `recovery-validation` DOL was uploaded to Dolphin at `192.168.1.52` after both Wiiload readiness checks passed before upload. The HBC screen showed `RECOVERY TEST HOLD`, which proves the validation hold started. The expected 30-second return to the loader did not occur. After the hang, port 4299 refused connections. No repeat upload was made and no physical Wii was involved.

The user-provided macOS hang report identifies Dolphin 2609a, PID 60570, as unresponsive for about 103 seconds. Dolphin's UI main thread waited in `CPU::CPUManager::SetStepping(bool)` on a condition variable. The `CPU-GPU thread` was last scheduled about 160 seconds earlier and waited in an atomic semaphore acquire. This supports a Dolphin guest-CPU stop/step hang during recovery; it does not identify the exact guest PPC instruction or prove which libogc reset substep initiated it.

The source route sets recovery stage 2, displays `RECOVERY RETURN TO LOADER`, then calls `ogc_system::exit(0)`. On the loader-stub route, `sys_exit_to_loader` calls `sys_reset_system(SYS_SHUTDOWN, 0, 0)` before branching to address `0x80001800`. The report lacks guest reset-stage telemetry, so the exact failure point remains unknown. Preserve this as an open failure and do not repeat the upload until the route is instrumented or corrected.

Artifacts in this directory include the DOL build manifest, upload summary and logs, both readiness logs, and a short extract from the user-provided Dolphin hang report. The original full report remains at the user attachment path and is not duplicated here.
