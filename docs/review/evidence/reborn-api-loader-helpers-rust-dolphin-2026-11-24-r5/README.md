# Reborn loader helper APIs in Dolphin

The production `loader-helpers` acceptance case passed in Dolphin. It calls
`build_launch_arguments` and checks normal arguments, an empty argument, an
exactly 1,024-byte command line, metadata arguments that exceed Reborn’s
1,024-byte metadata limit, and arguments that fit that limit but do not fit
with the executable path. The exact-boundary case keeps the metadata argument.
Both overflow cases produce only the executable path followed by two NUL bytes,
as Reborn does when it drops the complete metadata argument list. An embedded
NUL is rejected as a Rust safety check. The case also checks IOS policy and
load-plan validation.

The test boots the emulator acceptance DOL directly. HBC is not the program
under test, so a C HBC control does not apply. Dolphin ran as x86_64 under
Rosetta. The DOL SHA-256, source revision and source changes are in `build.json`;
`result.json` records result zero. The Dolphin build identifies as
`409881c34357d0f105fd473167d15ab0bd9c1628`.

Run command:

```sh
python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-argv-parity-acceptance.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-api-loader-helpers-rust-dolphin-2026-11-24-r5 \
  --cases loader-helpers --dolphin-arch x86_64
```

This Dolphin check does not prove how a particular homebrew app parses `__argv`
on a physical Wii. IOS reload, AHB behavior, and hardware cache and DMA effects
remain pending.
