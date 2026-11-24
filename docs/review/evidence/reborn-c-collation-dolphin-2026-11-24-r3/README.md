# Reborn catalog collation in the devkitPPC C runtime

The C probe passed in Dolphin 2609a using the installed devkitPPC/libogc
runtime. It sets `LC_CTYPE` to `C-ISO-8859-1`, then calls `strcasecmp` on
UTF-8 U+0100 (`Ā`) and U+0800 (`ࠀ`), and on ASCII `Alpha` and `alpha`.
Dolphin reported:

```text
HBC_COLLATE locale=C-ISO-8859-1 utf8_cmp=-28 ascii_cmp=0
PASS HBC_COLLATE ASCII-only UTF-8 byte order
```

This shows that the installed `strcasecmp` folds ASCII case but leaves UTF-8
bytes unchanged, even with the selected locale. The C source and DOL hashes,
compiler, and build command are in `build.json`; `result.json` records the
successful marker and the isolated Dolphin profile. Dolphin was stopped after
the marker because the probe waits for retrace indefinitely.

The command to run the probe was:

```sh
python3 tools/run_reborn_collation_c_dolphin.py \
  --dol target/reborn-c-collation.dol \
  --output docs/review/evidence/reborn-c-collation-dolphin-2026-11-24-r3
```

This verifies the installed C runtime under Dolphin. It does not verify a
physical Wii libc build.
