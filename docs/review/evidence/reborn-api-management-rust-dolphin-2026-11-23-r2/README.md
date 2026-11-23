# Archive management APIs in Dolphin

The `management-api` production case passed in Dolphin. It called the public
archive planning APIs with the fixed Wiiload app ZIP. It checked the planned
directory, both output files, the ten-byte total, and staged directory commit
and rollback rename order, including restoration after a failed stage rename.

The build manifest records the source and SDK revisions and the DOL hash.
Reborn checks the app ZIP in `source/manage.c#manage_check_zip_app` and writes
files in `manage_extract_zip`. Rust validates the complete plan before writes
and keeps the old app folder during replacement. The staged commit and rollback
are Rust safety extensions.

This case tests the production policy helpers with a controlled rename
callback. It does not execute FAT renames or ZIP writes. Full archive edge
cases, actual install recovery, and physical Wii storage behavior remain
pending.
