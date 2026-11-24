# Remote-input production API extension

The production `remote-input-api` case passed in Dolphin 2609a on 2026-11-24.
It maps a synthetic Wii Remote confirmation and pointer command through the
public `remote_frame` API and checks the confirmation, logical pointer, and
combined explicit and analog scroll. It also checks Right Joy-Con right-stick
navigation through the same production API.

The build manifest records the DOL and source revisions. Remote controller
input has no matching Reborn C API, so this stays a Rust extension. The case
does not verify network packet delivery, hold expiry, simultaneous local input,
or physical Wii Remote behavior.
