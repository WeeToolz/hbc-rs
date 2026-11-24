# HBC main menu API and parity check

The `main-menu-api` production acceptance case passed in Dolphin 2609a. It
checks that BootMii stays visible but disabled when unavailable, that pointer
and keyboard input cannot activate it, that focus skips it and recovers if
availability changes while it is selected, and that the enabled item returns
the BootMii launch action. It also checks the item count, status labels, and
both `AppRuntime` forwarding methods.

The target DOL hash and result are in `result.json`. The test injects the
availability state. It does not probe IOS254 on hardware, prove actual BootMii
launch, or verify Wii display behavior. Those requirements remain open.
