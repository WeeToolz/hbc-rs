# HBCP framebuffer Dolphin evidence

The Rust HBC answered a live `HBCP` request in Dolphin 2609a. The response
contained status 0, a payload length of 614408 bytes, 32-bit dimensions of
640 by 480, and 614400 nonzero YUYV framebuffer bytes. The test checked the
declared length against the dimensions and read the complete frame.

The matched HBC Reborn C control passed the same request and returned the same
dimensions and frame length. Its result is in
[`../reborn-hbcp-c-control-dolphin-2026-11-25-r2/result.json`](../reborn-hbcp-c-control-dolphin-2026-11-25-r2/result.json).
The C DOL SHA-256 is `a32c35c20e2a18ba3004aefbcea9ac10af555f0d068241e9332d6840b28fe5f6`;
it matches the known C control DOL used in the 2026-11-24 Reborn theme control.

The Rust DOL SHA-256 is
`8d14e16ce3d563ca4301a9b66c871e068623c6279ccf95e73551689ccdace66a`.
The harness used a private copy of the local Dolphin NAND and SD image. The
command was:

```sh
python3 tools/test_remote_input_dolphin.py \
  --host 192.168.1.52 \
  --dol target/hbc-rs.dol \
  --reborn-screen \
  --nand '/Users/user/Library/Application Support/Dolphin/Wii' \
  --sd-image '/Users/user/Library/Application Support/Dolphin/Load/WiiSD.raw' \
  --dolphin /Applications/Dolphin.app/Contents/MacOS/Dolphin \
  --output docs/review/evidence/reborn-hbcp-rust-dolphin-2026-11-25
```

`result.json` records the checked dimensions, payload size, DOL identity, and
passing case. `dolphin.log` is the captured Rust guest log. The C and Rust
process logs are kept in their matching evidence directories.

This verifies the wire response and live framebuffer read in Dolphin. It does
not verify Broadway cache behavior or IOS/network DMA coherency on a physical
Wii. Those checks must cover concurrent frame updates and remain pending.
