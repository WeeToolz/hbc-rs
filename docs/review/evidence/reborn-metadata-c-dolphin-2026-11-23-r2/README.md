# Reborn metadata C probes in Dolphin

This standalone devkitPPC probe copies Reborn's release-date `sscanf` and `mktime` sequence and the update parser's mxml extraction plus `sscanf("%llu")` conversion. It ran in Dolphin with an empty, private NAND profile. The result records 17 ordered release-date outputs, 8 indexed update-date outputs, and the `HBC_META done dates=17 updates=8` marker. Dolphin was force-stopped after that marker because the probe waits forever after reporting its results.

The C source SHA-256 is `14497c484c2b59f274ea66785bf1328974dfb1a934924f8d9b81de10b3ee5a1e`. The DOL SHA-256 is `750af8d353ffe9b94515ed5666f7e91204ea3e1eae4260431b7d223cb1766d9d`. The unmodified console log and parsed result are in this directory.

Build with devkitPPC GCC, libogc, and the installed mxml portlib:

```sh
/opt/devkitpro/devkitPPC/bin/powerpc-eabi-gcc -DGEKKO -mrvl -mcpu=750 -meabi -mhard-float -O2 -Wall -Wextra -I/opt/devkitpro/libogc/include -I/opt/devkitpro/portlibs/ppc/include -specs=/opt/devkitpro/libogc/share/rvl.specs -L/opt/devkitpro/libogc/lib/wii -L/opt/devkitpro/portlibs/ppc/lib probes/reborn-date-c/main.c -logc -lmxml -lm -o target/reborn-date-c/probe-update.elf
/opt/devkitpro/tools/bin/elf2dol target/reborn-date-c/probe-update.elf target/reborn-date-c/probe-update.dol
```

This verifies the installed C runtime under Dolphin. It does not verify a physical Wii.
