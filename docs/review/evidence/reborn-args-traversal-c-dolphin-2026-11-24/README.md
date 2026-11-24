# Reborn argument XML traversal in Dolphin

This C probe uses the same `mxmlFindElement` loop as Reborn's
`source/xml.c#_get_args`. It checks direct arguments, nested arguments before a
direct argument, and a nested argument between direct arguments.

Dolphin reports counts `2`, `1`, and `2`. It ignores nested `<arg>` elements
and collects only direct children of `<arguments>`. Rust uses the same direct
child rule in `parse_app_xml`. The production Rust `metadata-app` acceptance
case also passes in Dolphin with a nested argument before its direct arguments.
The C console records, parsed result, and probe source are stored in this
directory. The DOL SHA-256 is recorded in `run1/result.json`.

Build the C probe from
`/Volumes/SSD/WiiDev/C-sources/hbc-reborn/channel/channelapp`:

```sh
/opt/devkitpro/devkitPPC/bin/powerpc-eabi-gcc \
  -DGEKKO -mrvl -mcpu=750 -meabi -mhard-float -Os -Wall \
  -I/opt/devkitpro/libogc/include -I/opt/devkitpro/portlibs/ppc/include \
  -L/opt/devkitpro/libogc/lib/wii -L/opt/devkitpro/portlibs/ppc/lib \
  /Volumes/SSD/WiiDev/WeeToolz/hbc-rs/docs/review/evidence/reborn-args-traversal-c-dolphin-2026-11-24/probe.c \
  -specs=newrvl.spec -lmxml -logc -lm -o probe.elf
/opt/devkitpro/tools/bin/elf2dol probe.elf probe.dol
```

This verifies Mini-XML traversal in Dolphin and the Rust production parser in
Dolphin. It does not verify the parser on physical Wii hardware.
