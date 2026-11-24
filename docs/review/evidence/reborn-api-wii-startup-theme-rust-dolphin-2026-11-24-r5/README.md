# Wii startup theme preparation and apply in Dolphin

The production `wii-startup-theme-api` case passed in Dolphin 2609a. It uses a private folder-synced SD fixture with a `ThemeProbe` package and a minimal private NAND data directory for the direct-DOL title path. It does not copy or modify the normal Dolphin NAND.

The case waits for the production storage scan, confirms that `ThemeProbe` is cataloged as a theme, selects it through production settings state, prepares the theme from SD, applies the prepared metadata and texture set, and reads the persisted archive back from emulated NAND for byte equality with `tests/fixtures/theme-valid.zip`. The known theme description is `Dolphin`.

The case exposed a target allocation failure when replacement textures were built while the default theme textures remained allocated. Rust now saves the validated archive first, then drops the old textures before it builds the replacement set. This matches Reborn's `theme_deinit()` then `theme_init()` lifetime order and avoids the peak allocation that failed in Dolphin. The initial failed allocation run remains in the adjacent evidence directory.

The DOL identity is in `build.json`; `result.json` records the private Dolphin profile and result. This proves Dolphin behavior only. Physical Wii theme loading, NAND failure handling, and memory headroom on Broadway remain pending.
