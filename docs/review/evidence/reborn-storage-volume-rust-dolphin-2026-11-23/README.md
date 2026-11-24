# Storage volume APIs

The `storage-volumes` production acceptance case passed in Dolphin. It checks
all five `Volume::from_settings_index` and `Volume::settings_index` results,
the five mount names, and negative and out-of-range preference handling. It
also checks that `device_scan_order` places the selected device first and
includes each volume exactly once.

The result JSON and raw case log are archived here. The acceptance DOL
SHA-256 is
`0fb2b87446624d2e9b33e752b04405db8f698365b3b322c5e0d6c1bb7f3c3116`.

This checks the volume policy APIs only. Physical mounts, removable media, and
hot-plug behavior remain pending on Wii hardware.
