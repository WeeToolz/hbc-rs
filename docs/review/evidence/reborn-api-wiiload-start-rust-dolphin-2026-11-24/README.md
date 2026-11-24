# Wiiload worker start API

The production `wiiload-start-api` acceptance case passed in Dolphin 2609a
on 2026-11-24. It starts the production Wiiload worker, waits up to 15 seconds
for network startup to leave the `Starting` state, then stops and joins the
worker. It accepts either a clean stop or a reported cleanup error that agrees
with the production cleanup state. It also verifies that network and Gecko
indicators are inactive after stop.

The build manifest records the target DOL and source revisions. This case
checks worker lifecycle and cleanup in Dolphin. It does not prove physical Wii
IOS network startup, live uploads, or hardware timing.
