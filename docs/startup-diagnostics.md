# HBC startup diagnostic DOL

Build the diagnostic DOL from the repository root:

```sh
RUSTC=/Users/user/.rustup/toolchains/nightly-aarch64-apple-darwin/bin/rustc \
RUSTFLAGS='-C linker=/opt/homebrew/Caskroom/miniforge/base/envs/hummingbot/lib/rustlib/aarch64-apple-darwin/bin/gcc-ld/ld.lld -C panic=abort -C link-arg=-T/Volumes/SSD/projects/Wii/wiidev/hbc-rs/libogc-rs/targets/powerpc-unknown-wii.ld' \
/Users/user/.rustup/toolchains/nightly-aarch64-apple-darwin/bin/cargo build \
  --release --features startup-diagnostics \
  --target libogc-rs/targets/powerpc-unknown-wii.json \
  -Z build-std=core,alloc -Z json-target-spec

cd libogc-rs
cargo xtask elf-to-dol \
  /Volumes/SSD/projects/Wii/wiidev/hbc-rs/target/powerpc-unknown-wii/release/hbc-rs \
  /Volumes/SSD/projects/Wii/wiidev/hbc-rs/target/hbc-rs-startup.dol
```

The diagnostic starts with a CPU-rendered framebuffer. Its label changes as
startup reaches each stage. Read the last label if it stops. Red means HBC
reported an error or panic. The early labels include heap setup, video setup,
GX setup, network startup, NAND settings, theme loading, controller input, and
the first rendered frame.

The diagnostic framebuffer stays visible while GX writes to a separate
framebuffer. Its first GX frame draws only a white square over the initial gray
clear. This isolates the solid-shape and copy path from HBC's textured scene.
After that check passes, later frames draw the HBC screen. `GX COPY QUEUED`
means the GX commands were submitted; it does not confirm that the GPU
completed them. After a short hold, the program checks Y luma bytes in the
YUYV output. If the buffer has no visible luma, the diagnostic stays on a red
`GX XFB BLACK` label. Otherwise, it checks the square before it selects the
GX framebuffer.
If the fixed white-square sample check fails, it writes a gray field with a
centered white square and red outline through the CPU to the GX XFB, then holds
that image. This checks VI scanout separately from GX drawing and copying.

The diagnostic does not implement the Wiiload report protocol. A Wiiload
`report_timeout` can therefore occur even when the DOL continues to run. Use
the screen label and the emulator or console behavior to check progress.

The automated Dolphin GDB capture helper is `target/capture-startup.py`; its
profile and logs are under the ignored `target/` directory. The diagnostic
feature is disabled in normal builds.
