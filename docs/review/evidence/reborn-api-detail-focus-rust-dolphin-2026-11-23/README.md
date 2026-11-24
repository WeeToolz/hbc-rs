# App detail focus API check

The `detail-focus-api` production acceptance case passed in Dolphin 2609a. It
checks that Load is selected when app details open, that focus follows Delete,
Load, Back order, and that focus stops at both ends.

Reborn's `dialog_app` sets focus to its Load button. Its view focus helpers stop
at the first and last usable widgets. Rust previously focused Back by default
and wrapped between the action buttons; this check follows the fix.

The production DOL hash and result are in `result.json`. This does not prove
rendered focus styling or physical controller behavior.
