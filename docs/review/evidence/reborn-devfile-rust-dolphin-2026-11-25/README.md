# HBCF Rust Dolphin evidence

The final DOL passed live HBCF checks in Dolphin on 2026-11-25. The test used
private copies of the local Dolphin NAND and SD image. The test wrote and
removed files only in the isolated profile; it did not change the source image.

The test checked:

- `M` and `S` for a temporary app folder;
- raw upload (`P`) and framed upload (`p`);
- checksum (`C`) and raw download (`G`);
- compressed framed download (`g`), including frame size and CRC checks;
- directory listing (`L`) and file/directory deletion (`D`);
- app catalog refresh after upload and delete of `boot.dol` under the active
  SD app folder, after the one second quiet period;
- cleanup of all temporary files and the app folder.

The original test result is in `result.json`. It reports 5,301 bytes checked,
both catalog refreshes passing, and cleanup complete. `parity-result.json` is a
small named-case index for the parity ledger; it points to the original result
and records the named `reborn-file` case. `dolphin.log` is the copied
runtime log. `process.log` records the Dolphin process output. Both logs were
copied without text normalization. `wiiload-stack-audit.json` records the
release ELF stack audit: 65,536 bytes allocated, 27,344-byte direct-call lower
bound, and a passing 16,384-byte required margin.

The tested DOL SHA-256 is:

`088d65fcc3cdcf23a522e86446a595fb2cc693420f888767c0bc411458882c1f`

The test command was:

```sh
python3 tools/test_remote_input_dolphin.py \
  --host 192.168.1.52 \
  --dol target/hbc-rs.dol \
  --nand '/Users/user/Library/Application Support/Dolphin/Wii' \
  --sd-image '/Users/user/Library/Application Support/Dolphin/Load/WiiSD.raw' \
  --dolphin /Applications/Dolphin.app/Contents/MacOS/Dolphin \
  --output /private/tmp/hbc-rs-hbcf-dolphin-20261125-h \
  --reborn-file
```

This verifies functional protocol and catalog behavior in Dolphin. It does not
verify physical Wii FAT behavior, cache or DMA behavior, or visual parity of
all three transfer line styles.
