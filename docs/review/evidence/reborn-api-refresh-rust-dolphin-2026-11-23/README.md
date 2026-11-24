# HBC public API acceptance refresh

Eight production API acceptance cases passed in Dolphin 2609a. The target DOL
SHA-256 is recorded in `result.json`; it was built from source revision
`fe3cdabdb74bff9b3334f88e6ab947b80d614e5d`.

Cases: localization, HOME overlay control, application notice, About scrolling,
application controller, rendered hit testing, app metadata, and catalog.

The restricted Dolphin launch attempt failed before debugger startup. The same
cases then passed with the normal host Dolphin process. This archive contains
the passing run logs and result record. These checks prove only their named
production API behavior. They do not complete full API contracts or physical
Wii validation.
