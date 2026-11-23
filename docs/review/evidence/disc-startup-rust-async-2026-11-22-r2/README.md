# Rust HBC startup with the async DI callback chain

The standard Rust HBC DOL reached `HBC: startup complete; entering render
loop` in Dolphin 2609a. The existing Pipe input probe then completed all six
stages, including pointer movement, button input, and opening app details. The
result JSON records `capture_complete: true`, and the Dolphin process exited
with code zero. The first browser capture was reviewed and showed the app list.

The DOL SHA-256 is `4f11587a41fc181753bfd8bc528f2ac97ecc89a4ca40a5f083d6267e300c58e9`.
`build.manifest.json` records the target, source revision, and linker ELF.
The C HBC setup control used the same Pipe input script and a fresh copy of the
same starting Dolphin NAND. See
[`disc-startup-c-control-2026-11-22/result.json`](../disc-startup-c-control-2026-11-22/result.json)
and the [direct async callback acceptance result](../disc-stop-rust-dolphin-2026-11-22-r5/README.md).

Run command, from the HBC checkout:

```sh
python3 tools/probe_hbc_reference_input_dolphin.py \
  --image target/hbc-rs-disc-async-current.dol \
  --output docs/review/evidence/disc-startup-rust-async-2026-11-22-r2
```

This capture proves that the startup path renders and accepts input in Dolphin.
The app labels differ from the C control screenshot, so this run does not claim
catalog presentation parity. Physical Wii behavior remains unverified.
