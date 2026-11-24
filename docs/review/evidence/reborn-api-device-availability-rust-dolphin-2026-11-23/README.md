# Device availability APIs

The `device-availability-api` production case passes all-absent, mixed, and
all-present device states through `device_availability_snapshot` and
`AppRuntime::set_device_availability`. It checks the resulting per-device
state and confirms that the Options eligibility for each device matches the
availability result.

The case passed in Dolphin. The result JSON and raw case log are archived in
this directory. The acceptance DOL SHA-256 is
`1ac420767095f232c8dd3b4f99634f5497d4bff8273c654d6b7e2f0ac18b1161`.

This checks state conversion and forwarding only. Live device probing,
physical mount state, and hot-plug behavior remain pending on Wii hardware.
