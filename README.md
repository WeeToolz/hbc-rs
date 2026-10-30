# HBC Rust port

An in-progress Rust port of the Wii Homebrew Channel application. It uses the
Rust Wii services in [WeeToolz](https://github.com/WeeToolz/libogc-rs).
The project is based on the published [Homebrew Channel source](https://github.com/fail0verflow/hbc)
and [HBC Reborn](https://github.com/Monsterray/hbc-reborn).

This repository is available for development and review. It is not a release.
Some behavior and hardware paths remain incomplete or need verification on a
Wii. A successful build or Dolphin test does not prove hardware behavior. See
[port status](docs/port-status.md), the [parity report](docs/generated/parity-status.md),
and [development results](DEVELOPMENT_RESULTS.md) for evidence and open work.
The original HBC installer is not included.

## Setup

Install Rust, Cargo, Python 3, and a nightly Rust toolchain with `rust-src`.
Clone WeeToolz beside this repository using the directory name `libogc-rs`:

```sh
git clone https://github.com/WeeToolz/libogc-rs.git libogc-rs
git clone https://github.com/WeeToolz/hbc-rs.git hbc-rs
cd hbc-rs
rustup toolchain install nightly
rustup component add rust-src --toolchain nightly
cargo test
```

The path dependencies in `Cargo.toml` use the shared `../libogc-rs` checkout.
Maintain libogc changes in that repository, and record the exact revision and
local changes for each test build. Older HBC results apply to their recorded
builds. The host tests do not exercise Wii hardware.

## Wii build

From `hbc-rs`, build the ELF with the target and linker script from WeeToolz:

```sh
RUSTC="$(rustup which --toolchain nightly rustc)" \
RUSTFLAGS='-C linker=rust-lld -C panic=abort -C link-arg=-T../libogc-rs/targets/powerpc-nintendo-none.ld --check-cfg=cfg(gamecube)' \
rustup run nightly cargo build --release \
  --target ../libogc-rs/targets/powerpc-nintendo-none.json \
  -Z build-std=core,alloc \
  -Z build-std-features=compiler-builtins-mem \
  -Z json-target-spec

cd ../libogc-rs
cargo xtask elf-to-dol \
  ../hbc-rs/target/powerpc-nintendo-none/release/hbc-rs \
  ../hbc-rs/target/hbc-rs.dol
```

See [startup diagnostics](docs/startup-diagnostics.md) for the diagnostic
build and [Wii packaging](tools/package-wii.py) for SD application packages.
The packages are applications, not channel installers. Do not install a channel
package without understanding its inputs, risks, and recovery requirements.

## Development and reference material

- [Port status](docs/port-status.md) records implemented behavior and open work.
- [Parity report](docs/generated/parity-status.md) maps C behavior to Rust and
  separates implementation, review, and verification.
- [Terminal control](docs/terminal-control.md) describes the development tools.
- [Source and asset notices](SOURCE_NOTICES.md) records upstream material and
  known licensing limits.

The parity and localization tools require separate C source checkouts in the
workspace layout described by their scripts. Historical test reports can contain
paths and commands from the original development machine; they are evidence,
not portable setup instructions.
