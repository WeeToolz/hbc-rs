# Production API acceptance sweep in Dolphin

A fresh production `emulator-acceptance` DOL passed 22 named API and behavior
cases in Dolphin 2609a on 2026-11-24. The run used the existing production
acceptance runner and a private Dolphin profile. The result records all cases;
the build manifest records the DOL and source revisions.

The cases covered DOL and ELF launch, settings state, Wiiload header and policy,
ZIP classification and controller state, metadata parsers, catalog scanning,
theme package validation, loader helpers, app controller state, storage volume
policy, safe app deletion, input policy and mapping, render hit testing, and
the disc stop callback chain. All 22 returned success. The disc stop result
reported all seven callbacks, successful open, and successful stop.

This is Dolphin evidence for the named paths. It does not establish complete
C parity or physical Wii behavior. See the parity ledger for remaining checks.
