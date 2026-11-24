# Network restart millisecond clock API

The production `network-restart-api` acceptance case passed in Dolphin 2609a
on 2026-11-24. It checks that `platform::network_restart::now_ms()` advances
after a 10 ms LWP sleep, then runs the existing connection result and restart
ordering checks. The runner used a private Dolphin profile. The build manifest
records the target DOL and source revisions.

This proves a basic elapsed-time property in Dolphin. Reborn uses `gettime()`
and tick conversion for elapsed timeouts. This test does not establish exact
timer parity on Wii, IOS network restart, or IP acquisition.
