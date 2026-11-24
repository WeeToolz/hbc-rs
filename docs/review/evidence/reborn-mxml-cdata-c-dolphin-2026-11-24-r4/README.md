# Reborn Mini-XML field text in Dolphin

This read-only PPC probe uses Mini-XML 3.3.1 with `MXML_OPAQUE_CALLBACK` and
the same `_get_cdata` child traversal as Reborn `source/xml.c`. Its five cases
show the field value returned by that traversal:

- Text before a nested element wins; nested and trailing text are ignored.
- Text before a comment wins; text after it is ignored.
- Nested text without direct text returns no value.
- CDATA is not opaque text; later ordinary text is returned.
- CDATA without ordinary text returns no value.

Dolphin reports all five expected values in `result.json`. This probes Reborn's
Mini-XML behavior, not physical Wii behavior. The matching Rust production API
case is recorded in [`reborn-api-metadata-app-rust-dolphin-2026-11-24-r1`](../reborn-api-metadata-app-rust-dolphin-2026-11-24-r1/README.md).

The C probe source is `probe.c`. Build it from
`/Volumes/SSD/WiiDev/C-sources/hbc-reborn/channel/channelapp` with:

```sh
/opt/devkitpro/devkitPPC/bin/powerpc-eabi-gcc \
  -DGEKKO -mrvl -mcpu=750 -meabi -mhard-float -Os -Wall \
  -I/opt/devkitpro/libogc/include -I/opt/devkitpro/portlibs/ppc/include \
  -L/opt/devkitpro/libogc/lib/wii -L/opt/devkitpro/portlibs/ppc/lib \
  /Volumes/SSD/WiiDev/WeeToolz/hbc-rs/docs/review/evidence/reborn-mxml-cdata-c-dolphin-2026-11-24-r4/probe.c \
  -specs=newrvl.spec -lmxml -logc -lm -o probe.elf
/opt/devkitpro/tools/bin/elf2dol probe.elf probe.dol
```

The source and DOL SHA-256 values are in `result.json`. The matching Dolphin
build ID is `409881c34357d0f105fd473167d15ab0bd9c1628`.
