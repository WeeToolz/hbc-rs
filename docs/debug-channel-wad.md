# HBC Rust debug channel WAD

The prepared package is in `dist/hbc-rs-debug-2026-10-22/`. This directory is ignored by Git. Keep the WAD, matching ELF, and manifest together.

| Field | Value |
| --- | --- |
| Title | OHBC, `000100014f484243` |
| IOS | 58 |
| Version | 2375 |
| Boot content index | 1 |
| WAD size | 3,096,896 bytes |
| WAD SHA-256 | `157c9609a72c3d7535e0c6f6aaf5947d31cd94da7a59b9e7a2811aa0209a755e` |
| Build | Release, `debug=2`, `debugger-network` |
| Metadata | Legacy fakesigned; certificate root trust unchecked |

The WAD uses Reborn's banner, footer, and retail metadata as inputs. Its boot content is the Rust HBC DOL with the channel entry vector added by `gc-elf`. The production WAD implementation is `../libogc-rs/crates/tools/gc-wad`. It does not call the reference packer. The independent reference test uses the actual Reborn packer and gives identical output bytes.

This title ID is the existing OHBC ID. An installation can replace an existing OHBC channel. No physical installation was done in this task. Follow the workspace firmware preparation and recovery rules before a physical write. The physical return failure remains open.

## Debugging

The package includes the complete ELF with source debug information. The `debugger-network` feature provides cooperative status and arm, pause, and resume commands on TCP 2828. Wiiload uses TCP 4299. Use `tools/hbc-gdb-status.py` or the existing cooperative debugger clients. This build does not provide register access, memory reads, breakpoints, or exception debugging through the HBC network endpoint. Dolphin's separate emulator debugger was used to check startup. Network listener operation was not tested in this WAD run.

This is not an automatically qualified hardware recovery build. No physical upload or channel-install acceptance is claimed.

## Rebuild

Build the host tools from the SDK:

```sh
cd /Volumes/SSD/WiiDev/Devtools/libogc-rs
cargo build -p gc-wad -p xtask
```

Build HBC from its repository root:

```sh
CARGO_PROFILE_RELEASE_DEBUG=2 \
RUSTC="$(rustup which --toolchain nightly rustc)" \
RUSTFLAGS='-C linker=rust-lld -C panic=abort -C link-arg=-T../libogc-rs/targets/powerpc-nintendo-none.ld --check-cfg=cfg(gamecube)' \
rustup run nightly cargo build --release --features debugger-network \
  --target ../libogc-rs/targets/powerpc-nintendo-none.json \
  -Z build-std=core,alloc -Z build-std-features=compiler-builtins-mem -Z json-target-spec
../libogc-rs/target/debug/xtask elf-to-dol \
  target/powerpc-nintendo-none/release/hbc-rs \
  target/hbc-rs-debug.dol
python3 tools/package_debug_wad.py \
  --elf target/powerpc-nintendo-none/release/hbc-rs \
  --dol target/hbc-rs-debug.dol \
  --reborn-root /Volumes/SSD/WiiDev/C-sources/hbc-reborn \
  --wad-tool ../libogc-rs/target/debug/gc-wad \
  --xtask ../libogc-rs/target/debug/xtask \
  --key-file /Users/user/.wii/common-key \
  --output dist/NEW-PACKAGE-DIRECTORY
```

The packager refuses an existing output directory. It checks the ELF-to-DOL match, source debug sections, and network transport symbol. It then calls the native Rust channel-entry and WAD tools. The WAD tool reads back the file and checks every decrypted content hash. Keys are supplied as file paths and are not printed.

## Verification evidence

- `docs/review/evidence/channel-wad-2026-10-22-r2/result.json`: Dolphin reached `_start`, `main`, and the first `draw_frame` call in a disposable NAND copy. The process stopped cleanly.
- Dolphin supplied `STUBHAXX` before HBC startup. HBC preserved the supplied bytes. This proves the caller-stub preserve path. The supplied area is not the bundled executable stub, so this does not validate execution of that stub.
- The first run expected the bundled stub unconditionally and failed that test assumption. Its result is retained under `channel-wad-2026-10-22/`. The second run checks the actual production install-or-preserve rule.
- Dolphin can launch the DOL entry directly. The added physical real-mode entry vector still needs a Wii test.
- `../libogc-rs/config/wad-parity.csv` and its generated report record the WAD scope and open capabilities. Private-key signing, EC verification, root trust, and physical acceptance remain open.
