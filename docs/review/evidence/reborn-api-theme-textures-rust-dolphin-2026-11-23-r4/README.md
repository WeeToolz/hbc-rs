# Theme texture loading APIs with package override

The `theme-textures-api` acceptance case checks the 640 by 480 standard
fallback background, the 852 by 480 widescreen fallback background, and the
required fallback textures. It parses a ZIP fixture with a 432 by 64
`apps_list.png`, loads the package through `ThemeTextures::load`, and confirms
that this list card replaces the bundled fallback.

The production acceptance case passed in Dolphin. The result JSON and raw case
log are archived in this directory. The acceptance DOL SHA-256 is
`1205aeb98254d4a674bb73271bab22242fedcec14c35a251a470a56c686c8489`. The
fixture SHA-256 is
`4f926b77461d273d50b4fe47f16ada2ab9d5c3c9433d0a3f1f51ff63633224e7`.

This tests one packaged image override. Other image assets, rendered GX output,
and physical Wii behavior remain open.
