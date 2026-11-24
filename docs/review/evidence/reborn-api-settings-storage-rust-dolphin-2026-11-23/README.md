# Settings NAND storage APIs

The production `settings` acceptance sequence runs in a private copy of the
Dolphin NAND. `settings-save` calls the public settings save API. The runner
compares the persisted XML with independent expected bytes. `settings-load`
calls the public load API and checks the expected values. The runner then
moves the file into its backup directory and calls `settings-load-recovery`,
which checks the production recovery read path. The banner error and retry
cases also pass with debugger-injected ISFS permission errors.

All six named cases passed in Dolphin. The result JSON and raw case logs are
archived in this directory. The acceptance DOL SHA-256 is
`df8f2c6729eacea00d0804af8576fde772a150cca3240c562104af1d3b7d7e8d`.

The run writes only to the runner's private copy of the Dolphin NAND. It does
not prove physical Wii IOS, ISFS, or NAND behavior.
