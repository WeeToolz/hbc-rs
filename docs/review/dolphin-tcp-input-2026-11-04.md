# HBC TCP input and control verification

The existing production HBC DOL with SHA-256
`320e6fa9bb43ea55b82005b32d986f511eff2179f70c7169c97471b4e321d8d1`
passed the remote-input test and its extended control mode in Dolphin.
The DOL was an existing artifact; its build-source revision was not established
in this run. These results do not prove the current SDK build's TCP APIs or
physical Wii behavior.

The script split request writes, checked exact protocol reply bytes, opened
new connections, rejected invalid versions and active release packets,
confirmed main-loop completion, and verified operation errors and duplicate
command results. The input/control extensions have no C HBC protocol
counterpart; no independent C implementation result is claimed.

The runner now takes NAND, Dolphin, and output paths from the command line.
It validates that the host address belongs to the Mac and that the port is free before
launching a private Dolphin profile. It copies the source NAND, keeps the
process handle for cleanup, and saves result.json without machine-specific
paths. Raw process and emulator logs remain in the chosen output directory.

```sh
python3 tools/test_remote_input_dolphin.py --host "$HOST_IPV4" \
  --dol "$HBC_DOL" --nand "$NAND_DIR" --dolphin "$DOLPHIN_BIN" \
  --output "$RUN_DIR" --control
```

Use a new result directory. Set the host address to the Mac's address, and use
the same patched macOS Dolphin network build used by the SDK probes. Never
publish the private NAND profile or its network configuration. A successful
run reports both protocol completion and clean process termination; a forced
termination is recorded as a failure.
