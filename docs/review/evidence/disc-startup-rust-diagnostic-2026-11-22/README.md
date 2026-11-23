# Disc startup experiment, 2026-11-22

Status: **unverified; the experimental code was removed**. Physical Wii: not tested.

The target was HBC Reborn startup parity: open `/dev/di`, send command `0xe3`, and close the descriptor without blocking the browser. The experiment put synchronous IOS calls on a new LWP, then waited for that worker before handoff. HBC source was based on commit `2887b09`; the SDK source was `b25827a6c49b8dbe2d6c2f901aadaaec670f86c3`. The diagnostic DOL SHA-256 was `3e50cac1bd96e5b1810f95e24ef874d3507fbfb587ed5091ee0e4f438cd620fc`. The experimental source diff was not archived, so the DOL hash does not provide a reproducible build by itself.

Dolphin 2609a ran with a disposable profile through `python3 tools/probe_hbc_reference_input_dolphin.py --image target/hbc-rs.dol --output docs/review/evidence/disc-startup-rust-diagnostic-2026-11-22`. See `result.json` and the unmodified logs for the command, fixture, and result. The log shows the first framebuffer was queued and then `HBC: startup disc opening`; it contains no open result. The script timed out waiting for a screenshot. This does not establish the cause of the screenshot timeout or whether IOS ever replied.

An earlier build with the same worker approach also reached its first rendered frame and timed out waiting for a screenshot; its evidence is in `../disc-startup-rust-dolphin-2026-11-22/`. The matched C setup control is in `../disc-startup-c-control-2026-11-22/`. Its first screenshot shows the browser, and its capture script completed. The C control does not directly report motor-stop completion.

The SDK synchronous submission path holds a global synchronous-wait guard until IOS replies. A pending synchronous open can therefore hold up other synchronous IOS operations, even when called from a separate LWP. This is a source-based risk; the available logs do not establish that it caused the observed timeout. The worker approach was removed. The next implementation needs asynchronous IOS requests and a proven callback/request drain before executable handoff, with bounded error handling. Do not treat either Dolphin run as a motor-stop pass.
