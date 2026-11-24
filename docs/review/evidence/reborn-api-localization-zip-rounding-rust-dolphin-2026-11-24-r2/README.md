# Reborn ZIP confirmation size formatting evidence

Reborn's `loader_handle_zip_app` in `channel/channelapp/source/loader.c`
selects kilobytes while `bytes / 1024u <= 999`. Above that bound it formats a
single-precision float with `%.02f MB`. Rust previously truncated the MB value
to hundredths. That gave `0.97 MB` for 1,024,000 bytes, while Reborn displays
`0.98 MB`.

Rust now formats the same single-precision value to two decimal places. Host
tests and the production `localization-api` acceptance case check:

- 1,023,999 bytes: `999 KB`
- 1,024,000 bytes: `0.98 MB`
- 1,100,000 bytes: `1.05 MB`

A small host C probe used the same branch and `%.02f` expression from Reborn and
returned those same three strings; its output is in `c-size-format.log`. The
Rust production API also passed these checks in Dolphin 2609a on x86_64, with a
private profile. The runner result and DOL hash are in `result.json`;
`build.json` records the target artifacts.

This tests the formatted API output. It does not prove full dialog rendering,
controller input, or the complete received ZIP extraction flow on Wii hardware.

The test called the Rust production acceptance DOL directly. HBC was not the
program under test, so a C HBC control was not needed.
