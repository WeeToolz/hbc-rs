# Production API acceptance sweep in Dolphin

The fresh `emulator-acceptance` build passed 22 named production cases in
Dolphin 2609a. The run used a private NAND profile. The result file records
each case. The build manifest records source revision `c72b90d` and DOL SHA-256
`61c9fdd578d26f1d6b9af0c67ee4f4afc21d7dd719e0133d0720ba3ffc6f1f75`.

The cases cover DOL and ELF launch, settings state, Wiiload header and policy,
ZIP classification and controller state, metadata parsers, catalog scanning,
theme package validation, loader helpers, app controller state, storage volume
policy, safe app deletion, input policy and mapping, render hit testing, and the
disc stop callback chain.

These results give passing evidence for the named paths only. The API inventory
still reports 88 partial and 347 unverified public APIs. The sweep does not
complete every API contract, prove physical Wii behavior, or change the
hardware-pending states in the parity ledger.
