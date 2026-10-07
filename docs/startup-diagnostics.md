# HBC startup diagnostic DOL

Build the diagnostic ELF with the Wii target from `libogc-rs`:

```sh
RUSTC="$(rustup which --toolchain nightly rustc)" \
RUSTFLAGS='-C linker=rust-lld -C panic=abort -C link-arg=-T../libogc-rs/targets/powerpc-nintendo-none.ld --check-cfg=cfg(gamecube)' \
rustup run nightly cargo build \
  --release --features startup-diagnostics \
  --target ../libogc-rs/targets/powerpc-nintendo-none.json \
  -Z build-std=core,alloc \
  -Z build-std-features=compiler-builtins-mem \
  -Z json-target-spec

cd ../libogc-rs
cargo xtask elf-to-dol \
  ../hbc-rs/target/powerpc-nintendo-none/release/hbc-rs \
  ../hbc-rs/target/hbc-rs-startup.dol
```

The diagnostic starts with a CPU-rendered framebuffer. Its label changes as
startup reaches each stage. Read the last label if it stops. Red means HBC
reported an error or panic. The early labels include heap setup, video setup,
GX setup, network startup, NAND settings, theme loading, controller input, and
the first rendered frame.

Theme loading has separate progress labels for title-path lookup, ISFS setup,
NAND recovery, file open and stats, file read and close, ZIP parsing, and theme
texture creation. The last visible label identifies the operation that did not
finish.

The diagnostic framebuffer stays visible while GX writes to a separate
framebuffer. Its first GX frame uses HBC's production renderer and adds a white
square over the scene. Before the display copy, it waits for GX and reads the
square from the EFB with `GX_PeekARGB`. After the copy, it checks Y luma bytes
and the same square in the YUYV XFB. `GX COPY QUEUED` means the GX commands were
submitted; it does not confirm that the GPU completed them. If a check fails,
the diagnostic paints a gray field with a white square and red outline through
the CPU into the GX XFB, selects that buffer, and writes the failure stage above
the square. This checks VI scanout separately from EFB drawing and EFB-to-XFB
copying.

The diagnostic accepts the report arguments added by Wiiload-RS and sends a
report to the host after it runs the requested probes. It then resets to HBC
when `--restart-after-probe` is set. The DOL attaches to an IOS network stack
that is already running; it does not call IOS network startup again. If report
setup fails, the screen shows the failing stage (`REPORT NET INIT FAIL`,
`REPORT SOCKET FAIL`, `REPORT CONNECT FAIL`, or `REPORT WRITE FAIL`) before it
resets.

Wiiload-RS always saves `result.json` in `target/wii-runs/<run-id>-<wii-ip>/`.
It saves `report.wir`, `probe.log`, framebuffer captures, and progress data
only when the Wii connects and sends those bytes. A timeout with zero accepted
connections therefore still has a `result.json`, but no report or probe files.

The automated Dolphin GDB capture helper is `target/capture-startup.py`; its
profile and logs are under the ignored `target/` directory. The diagnostic
feature is disabled in normal builds.
