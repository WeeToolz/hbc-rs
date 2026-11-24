# HBC app deletion checks

The `app-deletion` production acceptance case passed in Dolphin 2609a. It
checks safe app-folder path validation and the app controller's deletion
confirmation state: No is the default, keyboard and pointer input can choose
No or Yes, and only Yes returns the delete action.

The result and production DOL hash are in `result.json`. Reborn also starts its
Yes/No dialog with No selected. Dolphin does not prove rendered dialog behavior,
FAT deletion, or physical storage error handling; those remain pending.
