# Source and asset notices

This project is a Rust port based on the published Homebrew Channel source and
[HBC Reborn](https://github.com/Monsterray/hbc-reborn), a maintained fork of
[The Homebrew Channel](https://github.com/fail0verflow/hbc). The upstream
projects retain their copyright notices. HBC Reborn includes the GNU General
Public License version 2 in its `COPYING` file; a copy is included here.
Individual upstream files can specify version 2 or a later version. Consult
their notices for their exact terms.

The bundled HBC artwork and Droid font files in `assets/` come from the HBC
Reborn channel application. Their individual provenance and redistribution
terms have not yet been fully documented. Do not assume that the GPL text alone
settles the terms for every asset.

`assets/return_stub.bin` and `assets/return_stub.elf` were built from HBC
Reborn's C return stub. The manifest records source hashes, the source revision,
and the build command. The source revision is
`90025e8fe503795f5246392e044e010527879cc6` in HBC Reborn. The stub source
contains GPL version 2 or later notices. The build also used the upstream
channel configuration and toolchain; see the manifest for details.

WeeToolz is a separate dependency with its own license and source notices.
Third-party Rust packages have their own licenses. A complete review of the
license terms for the Rust port, bundled assets, and all dependencies remains
open; this notice does not claim that review is complete.

## Dependency review

At the lockfile revision reviewed for publication, `cargo metadata --locked`
listed these registry package license expressions:

| Packages | Declared license expression |
| --- | --- |
| adler2 | 0BSD OR MIT OR Apache-2.0 |
| equivalent | Apache-2.0 OR MIT |
| core_maths, libm | MIT |
| allocator-api2, cfg-if, crc32fast, hashbrown, ttf-parser | MIT OR Apache-2.0 |
| fontdue | MIT OR Apache-2.0 OR Zlib |
| miniz_oxide | MIT OR Zlib OR Apache-2.0 |
| foldhash | Zlib |

These are package metadata, not an independent verification of every license
file or binary distribution requirement. See `Cargo.lock` for exact versions.
WeeToolz contains components with separate notices; consult its `LICENSE` and
`docs/source-licenses.md` and `docs/third-party-notices.md` in the checkout used for the build.

The bundled fonts identify themselves as Droid Sans. The Android Open Source
Project distributes Droid fonts under Apache-2.0, but the exact bundled files
have not yet been matched to a documented upstream release. The copyright and
license terms for the artwork also need confirmation.
