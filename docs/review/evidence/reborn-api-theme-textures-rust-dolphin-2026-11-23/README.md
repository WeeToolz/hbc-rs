# Theme texture loading APIs

The `theme-textures-api` production acceptance case calls
`ThemeTextures::load(None)` and checks the 640 by 480 bundled background,
logo, and list card. It then calls `ThemeTextures::load_for_viewport(None, true)`
and checks the 852 by 480 widescreen background and the same required fallback
textures. It drops the standard textures before loading the
widescreen set so the test does not hold both allocations at once.

The case passed in Dolphin. The result JSON and raw case log are archived in
this directory. The acceptance DOL SHA-256 is
`def881e930511f589561b44c9b9dbb4074ec1eab46bc8a4e88c05feeb3014d2f`.

Reborn's `theme_init` loads its embedded background and theme resources. This
test covers Rust's bundled fallback assets only. It does not prove installed
theme image selection, GX presentation, or physical Wii behavior.
