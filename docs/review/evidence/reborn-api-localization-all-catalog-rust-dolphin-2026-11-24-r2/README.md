# Reborn localization catalog Dolphin evidence

The production `localization-api` acceptance case passed in Dolphin. It called
`Language::text` for all 318 translated entries across the six Reborn PO
catalogs: Spanish, French, German, Dutch, Italian, and Japanese. It also checked
language selection, Japanese theme gating, and fallback for an unknown dynamic
string.

`python3 tools/generate-localization.py --check` passed. This confirms that the
Rust table matches the source PO catalogs. The Dolphin case checks each generated
entry through the production lookup API. It does not prove PO file loading in the
full startup path or glyph rendering on Wii hardware.

The test used Dolphin 2609a with the x86_64 build and a private profile. HBC was
not the program under test; the Rust acceptance DOL called the production API
through the existing debugger runner. The runner stopped Dolphin after the
acceptance breakpoint returned success. The result file records success and the DOL hash. The runner log contains no
text output for this successful case.

The DOL SHA-256 is
`b81cc2e6e35e6eb214d70a6cf09eb66a3f78f4285595c74fbf90501b09700001`.
`build.json` records the compiler inputs and artifact hashes. The DOL was built
with the existing PowerPC Wii target settings in a temporary target directory,
then converted with `cargo xtask elf-to-dol`, because the SSD target folder had
no free space.

The acceptance DOL is built from production `hbc-rs` code with the
`emulator-acceptance` feature. No C HBC control was needed because this check
runs the Rust API directly and does not test HBC startup, input, storage, or app
loading.
