# Reborn catalog optional-file size limits

The production `catalog` acceptance case passed in Dolphin 2609a with a
controlled storage reader. It supplies a valid executable, valid XML metadata
larger than 1 MiB, and an icon file larger than 1 MiB. Rust applies the 1 MiB
read limit to both optional files, keeps the executable in the catalog, and
reports no metadata or icon for that entry. The acceptance fixture also checks
the exact limits passed to the storage reader.

Reborn calls `meta_parse` after `stat` and loads the XML with `mxmlLoadFd`
without an equivalent input size cap. Reborn's `tex_from_png_file` allocates
`st_size` bytes and reads the full icon before decoding. Rust's cap is a safety
extension that limits scan allocations. It can omit valid optional metadata
or icons above 1 MiB; it does not hide the app executable.

The production DOL SHA-256 is
`e01dffa92b9b0742beb1cce8db54b2825afcc420bc0b52d081f6963da56f6332`. The
acceptance result and build manifest record the run. The exact command was:

```sh
python3 tools/test_production_acceptance_dolphin.py \
  --dol /private/tmp/hbc-rs/catalog-optional-size-acceptance.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-api-catalog-optional-size-rust-dolphin-2026-11-24 \
  --dolphin-arch x86_64 \
  --cases catalog
```

This checks Rust's bounded policy in Dolphin. It does not measure memory use
or optional-file parsing on a physical Wii. Actual FAT reads remain pending.
