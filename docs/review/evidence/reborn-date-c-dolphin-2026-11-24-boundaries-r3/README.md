# Reborn C date boundary probe

The standalone devkitPPC probe copied Reborn `meta_parse`'s `sscanf` and
`mktime` sequence. It emitted 27 date records and eight update records before
the runner stopped its wait loop. The runner checked the final completion
marker and all record counts. The run used Dolphin 2609a. Dolphin was then
stopped after the probe had finished its work.

The probe includes year 0, dates before the epoch, the year-2038 boundary,
and year 9999. Newlib config selects `__int_least64_t` for `time_t` unless
`_USE_LONG_TIME_T` is defined. This build does not define it. The probe reports
epochs beyond the signed 32-bit range, including year 9999. This confirms
64-bit `time_t` behavior for this Dolphin runtime.
See `result.json` and the unmodified `console.log`. Build flags and artifact
hashes are in `c-build.manifest.json`.

This is Dolphin evidence only. Physical Wii date and time-zone behavior remain
unverified.
