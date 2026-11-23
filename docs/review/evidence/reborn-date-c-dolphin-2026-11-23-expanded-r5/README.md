# Reborn C release-date probe in Dolphin

This standalone devkitPPC probe copies Reborn `meta_parse`'s `sscanf` and `mktime` sequence. It ran in Dolphin with an empty, private NAND profile. The result records 17 ordered input results and the `HBC_DATE done cases=17` marker. Dolphin was force-stopped after that marker because the probe waits forever after reporting its results. No probe case was running at that point.

The C source SHA-256 is `7946b35e094f1762dffe2915b80b1d94a567a5f33076d6cb1ac8356fe599563c`. The DOL SHA-256 is `7c51668675bfecf2a20a3eccbc2cff277019064e9628dec68f23b4c6ab3b34f5`. The unmodified console log and parsed result are in this directory.

Build with devkitPPC GCC and libogc:

```sh
/opt/devkitpro/devkitPPC/bin/powerpc-eabi-gcc -DGEKKO -mrvl -mcpu=750 -meabi -mhard-float -O2 -Wall -Wextra -I/opt/devkitpro/libogc/include -specs=/opt/devkitpro/libogc/share/rvl.specs -L/opt/devkitpro/libogc/lib/wii probes/reborn-date-c/main.c -logc -lm -o target/reborn-date-c/probe.elf
/opt/devkitpro/tools/bin/elf2dol target/reborn-date-c/probe.elf target/reborn-date-c/probe-expanded.dol
```

This verifies the installed C runtime under Dolphin. It does not verify a physical Wii.
