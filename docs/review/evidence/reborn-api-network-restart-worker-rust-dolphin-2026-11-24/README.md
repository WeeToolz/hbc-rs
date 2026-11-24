# Network restart worker APIs

The production `network-restart-worker-api` case passed in Dolphin 2609a on
2026-11-24. It calls the public network restart `start` API, waits for the
Wiiload worker to leave `Starting`, and checks the returned connection check
against the worker status and local address. It then calls `restart`, waits
for the replacement worker, checks the new connection result, and stops the
worker. The final check requires stopped indicators and either clean shutdown
or a cleanup error that matches the reported production state.

The runner used a private Dolphin profile. The build manifest records the
target DOL and source revisions. This verifies the worker lifecycle in Dolphin;
IOS network startup, physical Wii IP assignment, and live upload behavior
remain pending.
