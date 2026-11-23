# ZIP archive APIs in Dolphin

The `archive-api` production acceptance case passed in Dolphin. It called the
public ZIP APIs with the fixed `theme-valid.zip` fixture. It checked central
directory order, exact and missing-name lookup, extraction bytes, output-size
limits, a caller entry limit, truncated input, safe nested paths, and parent
directory traversal rejection.

The build manifest records the source and SDK revisions and the DOL hash. The
case compares Rust ZIP behavior with Reborn's minizip path in
`source/manage.c#manage_check_zip_theme` and `manage_extract_zip`.

This is partial API evidence. It does not compare every malformed ZIP, data
descriptor, encryption, compression, or filesystem failure case. Rust rejects
parent-directory components but accepts harmless names with two consecutive
dots; Reborn's inline check rejects those names too. Theme installation applies
its separate root-only rule. Dolphin does not prove physical Wii storage
behavior.
