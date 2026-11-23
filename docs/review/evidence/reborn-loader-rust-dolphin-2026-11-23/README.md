# Reborn executable loader smoke cases in Dolphin

The production `launch-dol` and `launch-elf` acceptance cases passed in
Dolphin. Each case builds a small executable in memory, calls the production
`parse_dol` or `parse_elf32_ppc` API, stages the load plan, checks initialized
text and data plus cleared BSS, and transfers control to a linked test child.
The machine result and DOL hash are in `result.json`; each case has its own log.

Reborn performs DOL and ELF relocation in `source/loader_reloc.c` in
`reloc_dol` and `reloc_elf`. These smoke cases prove the selected production
paths can load and hand off the chosen fixtures. They do not compare the full
accepted format space, all error paths, or the full IOS launch contract.
Physical Broadway cache, memory visibility, and handoff behavior remain pending.
