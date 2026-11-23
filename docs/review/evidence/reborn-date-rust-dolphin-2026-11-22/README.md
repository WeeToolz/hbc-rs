# Reborn release-date API comparison

Dolphin 2609a ran a standalone C probe and the Rust `emulator-acceptance` target. Neither run used a physical Wii.

The C probe in `probes/reborn-date-c/main.c` copies Reborn `meta_parse`'s `sscanf` and `mktime` sequence. It was built with devkitPPC GCC 16.1.0 and the installed libogc/newlib. Its source SHA-256 is `dc7c083848e2e12b8fbe8f6b7ccf027fe9d304ffdaf03c7bb857a6db0c3b1129`; the DOL SHA-256 is `479334cad73385224eb7c51343a2b19131cef8759aeef013a8d5a0d2d6c2615a`. Build:

```sh
/opt/devkitpro/devkitPPC/bin/powerpc-eabi-gcc -DGEKKO -mrvl -mcpu=750 -meabi -mhard-float -O2 -Wall -Wextra -I/opt/devkitpro/libogc/include -specs=/opt/devkitpro/libogc/share/rvl.specs -L/opt/devkitpro/libogc/lib/wii probes/reborn-date-c/main.c -logc -lm -o target/reborn-date-c/probe.elf
/opt/devkitpro/tools/bin/elf2dol target/reborn-date-c/probe.elf target/reborn-date-c/probe.dol
```

The C run used `python3 tools/probe_hbc_reference_input_dolphin.py --image target/reborn-date-c/probe.dol --output docs/review/evidence/reborn-date-c-dolphin-2026-11-22`. Its console log contains all eight `HBC_DATE` results and the final `done cases=8` marker. The general screenshot script timed out and forced Dolphin to terminate; it does not establish a screenshot or UI pass. This standalone probe tests the copied parsing sequence and installed C runtime, not the previously built Reborn binary.

The Rust run used `python3 tools/test_production_acceptance_dolphin.py --dol target/reborn-date-c/rust-probe.dol --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance --output docs/review/evidence/reborn-date-rust-dolphin-2026-11-22 --cases metadata-date`. The debugger reached `hbc_acceptance_complete` with result zero after the production `parse_app_xml` API checked all eight cases against the C results. The result JSON is the automated pass record. The build manifest records the DOL hash, source revision, source changes, SDK revision, and feature set; `source.patch` preserves the relevant uncommitted source changes used by the build.

The tested cases cover out-of-range month, day, and time; signs; partial time; a negative year; and a zero epoch. They do not exhaust the input space, compare different timezone settings, prove physical Wii behavior, or establish the behavior of another C library build. A zero epoch is represented as an absent date because the Reborn comparator treats zero as missing.
