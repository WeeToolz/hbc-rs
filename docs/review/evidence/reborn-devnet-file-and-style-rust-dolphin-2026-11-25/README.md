# Reborn developer protocol Dolphin evidence

The final debug DOL passed HBCF and HBCT checks in one isolated Dolphin session
on 2026-11-25. The test used private copies of the local Dolphin NAND and SD
image. The operations changed only the isolated profile.

The session checked:

- HBCT accepts `scroll`, `bounce`, and `truncate` and rejects an invalid style;
- HBCS reports each selected transfer style;
- HBCF mkdir, stat, raw and framed upload, checksum, raw and compressed framed
download, listing, file deletion, and directory deletion;
- HBCF upload and delete of `boot.dol` refresh the active SD app catalog after
  the one second quiet period;
- all temporary files and the app folder are removed.

`result.json` contains the named `reborn-transfer-style` and `reborn-file`
cases. It reports 5,301 bytes checked, both catalog refreshes passing, and
cleanup complete. `dolphin.log` and `process.log` were copied without text
normalization. The ELF stack audit for this same DOL is in
`wiiload-stack-audit.json`.

The tested DOL SHA-256 is:

`088d65fcc3cdcf23a522e86446a595fb2cc693420f888767c0bc411458882c1f`

This verifies the protocol replies, file operations, and catalog behavior in
Dolphin. It does not verify physical Wii FAT behavior, cache or DMA behavior,
or visual parity of all three transfer line styles. The display formatter has
host tests, but rendered style comparisons remain open.
