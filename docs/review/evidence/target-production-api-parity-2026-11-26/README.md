# Normal Wii target build

The normal `hbc-rs` production binary built as a PowerPC DOL without the
`emulator-acceptance` feature. The build manifest records the target, DOL hash,
and SDK revision. This checks that acceptance-only API probes do not enter the
normal build.

Build command:

```sh
cargo xtask build-dol hbc-rs --bin hbc-rs \
  --manifest-path ../WeeToolz/hbc-rs-origin/Cargo.toml \
  --output ../WeeToolz/hbc-rs-origin/target/hbc-rs-production-api-parity-2026-11-26.dol
```

This is a target compile and link check. It does not prove physical Wii
behavior; hardware evidence remains pending.
