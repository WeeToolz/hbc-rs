# System Menu playtime reset API

The `playtime-reset-api` production case calls
`platform::settings::reset_playtime_record` in Dolphin. The runner copies the
existing Dolphin NAND to a private profile, requires the copied play record to
have at least four bytes and a nonzero prefix, then verifies that the API
clears exactly the first four bytes and preserves every remaining byte. The
result JSON confirms the post-write file check.

The case passed in Dolphin. Its result JSON and raw case log are archived
here. The acceptance DOL SHA-256 is
`f3cec9cf37de5474c054074bf93244076f226cfc927119a9c5f48b74acf1a215`. The
original Dolphin NAND file SHA-256 before the run was
`fb3b1a855d0aa44485e2a2bad9583ee8e025448438de161a0e0d05e102822efc`; it was
unchanged by the test, which wrote only to the private copy.

Reborn also clears four bytes from `play_rec.dat`; it does not check the
committed value. Rust reads the file, checks its size, writes the zero prefix,
closes it, and verifies the stored prefix. The read-back check is a safety
addition. Physical Wii ISFS behavior remains pending.
