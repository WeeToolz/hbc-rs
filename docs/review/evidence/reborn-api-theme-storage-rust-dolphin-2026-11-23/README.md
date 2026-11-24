# Theme archive storage APIs

The `theme-storage-api` acceptance case uses a private copy of the user's
Dolphin NAND. It validates the fixed theme ZIP fixture, calls the production
theme save API, and checks that the load API returns the exact archive bytes.
It repeats the save to check replacement, then submits an empty archive and
checks that the API rejects it without changing the installed bytes.

The production acceptance case passed in Dolphin. The result JSON and raw case
log are archived in this directory. The acceptance DOL SHA-256 is
`46d8ee6887482aa356c1646f55125d9139d8046ccdac7ea53ce8e5f0143e3f0a`. The
theme fixture SHA-256 is
`4f926b77461d273d50b4fe47f16ada2ab9d5c3c9433d0a3f1f51ff63633224e7`.

This verifies the API round trip and invalid-input preservation in Dolphin's
copied NAND. Physical Wii IOS permissions, NAND commit behavior, and recovery
remain pending.
