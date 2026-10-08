# HBC Rust port

This crate is a native Rust port of the HBC channel application in `../hbc/channel/channelapp`.
It does not call or wrap the C implementation. Port each behavior in Rust, and use
the vendored `libogc-rs/` workspace for Wii services and hardware access where its
Rust APIs provide the required operation. The vendored checkout is ignored by this
repository so library changes remain local and do not enlarge HBC commits.

## Scope

The first target is the running HBC application: device discovery, the app browser,
metadata and theme loading, input and graphics, app management, executable loading,
and application launch. Treat the reload stub as a separate later target because it
has a different startup address, linker setup, and low-level boot contract. The
banner builder, WiiPAX tool, wiiload receiver, channel/WAD packaging, and translations
are separate tools or assets. Reuse data and formats where useful, but do not make
them part of the runtime crate by default.

## Runtime architecture

Keep hardware access at the edge. The application core owns state and policies;
platform modules adapt `libogc-rs` services to small Rust interfaces. This makes
metadata parsing, sorting, filtering, archive validation, and loader validation
independent of Wii hardware and easier to test.

```text
main / startup
  platform (video, input, storage, network, IOS, timing, cache)
  app (application state and frame loop)
    browser + catalog       views + widgets + dialogs
    metadata + settings     themes + graphics resources
    management + archives   updates (optional)
    executable loader       launch / handoff
```

Suggested ownership boundaries:

- `platform`: initialize and shut down Wii services. Keep unsafe hardware access
  here where possible. Prefer safe `libogc-rs` crates and avoid direct C calls.
- `catalog`: mount-state snapshots, app directory scanning, metadata/icon loading,
  filtering, sorting, and selected-app identity. Avoid global mutable arrays.
- `metadata`: parse `meta.xml`, settings, update metadata, and theme XML. Return
  typed values and explicit parse errors.
- `browser`, `views`, `widgets`, `dialogs`: UI state and rendering. Keep app
  management and device I/O out of draw routines.
- `graphics`, `theme`, `localization`: texture/resource ownership, layout/theme
  data, and language selection. Decode image/font formats through Rust libraries
  or native Rust code.
- `management`: safe app/theme archive inspection, extraction, deletion, and
  directory creation. Validate archive paths before writing any file.
- `loader`: parse ELF and DOL bytes into validated load plans. Keep parsing and
  range checks separate from the unsafe memory copy/cache-flush step.
- `launch`: perform the final application handoff after all validation succeeds.
  Keep this small and isolated because it changes machine state.
- `network`: wiiload receive and optional update fetch. Keep protocol parsing
  separate from transport and UI progress state.
- `settings`, `playtime`, `title`: persistent settings and channel/NAND state.

## Porting order

1. Establish the Wii target and startup path, with a minimal Rust executable and
   a documented dependency on the local `libogc-rs` workspace.
2. Port pure data logic first: metadata, settings formats, app sorting/filtering,
   XML/color handling, archive path checks, hashes, and ELF/DOL structural parsing.
3. Add platform services: input, video/GX, filesystem mounts, NAND access, timing,
   and network. Keep each adapter narrow and avoid duplicating shared conversion
   or error handling.
4. Port the browser and views, then app/theme management and wiiload.
5. Add validated executable loading and launch last. Exercise it in Dolphin and,
   where available, on hardware.
6. Port the reload stub only after the application path works. Give it its own
   target/configuration if its linker and startup requirements do not fit the app.

Each step should replace a complete behavior boundary. Do not preserve C as a
runtime dependency or add a C shim around code being ported. For repeated logic,
use shared Rust helpers (for example, one checked path validator and one binary
range-checking helper) instead of parallel copies.

## Initial Rust layout

```text
hbc-rs/
  Cargo.toml
  README.md
  src/
    main.rs
    lib.rs
    app.rs
    platform/
      mod.rs
      storage.rs
      input.rs
      video.rs
      network.rs
      system.rs
    catalog.rs
    metadata.rs
    settings.rs
    browser.rs
    views.rs
    widgets.rs
    dialogs.rs
    graphics.rs
    theme.rs
    localization.rs
    management.rs
    archive.rs
    loader/
      mod.rs
      elf.rs
      dol.rs
      plan.rs
    launch.rs
    update.rs
    playtime.rs
    title.rs
```

Start as one crate. Split crates only when an independent boundary has stable
ownership or a separate build target. Add dependencies by exact capability after
checking their current Rust API in `libogc-rs/`; do not assume a crate has the same
API or feature set as C libogc. The workspace currently
has Rust crates for GX/video, input, FAT/filesystem, network, IOS/ES, cache, and
system services. Some interfaces may still be incomplete for HBC, so record gaps
as Rust implementation work rather than solving them with C wrappers.

See [the current port status](docs/port-status.md) for implemented boundaries,
open runtime work, and local verification limits.

## Parity tracking

Track each in-scope behavior from its C source symbol to the Rust implementation
in `config/parity-ledger.csv`. The ledger keeps implementation coverage, source
review, and verification separate. Run `python3 tools/parity.py status` to read
the report, `python3 tools/parity.py refresh` after ledger changes, and
`python3 tools/parity.py check` to validate all source references and the
generated report. Run `python3 tools/generate-localization.py` after C PO changes
and `python3 tools/generate-localization.py --check` to verify the embedded six
language catalogs. `C-sources/hbc/` must be available beside `RustPorts/`.

Before building from a fresh HBC checkout, copy the local `libogc-rs` workspace to
`hbc-rs/libogc-rs/`. The dependency paths in `Cargo.toml` point to that location.

## Early bug candidates

These are findings to confirm and fix as the corresponding Rust behavior is
implemented:

- `channel/channelapp/source/http.c`: `http_split_url` calls `strchr` and then
  reads `c[0]` without checking whether the URL has a slash. A host-only URL can
  dereference a null pointer. The function also accepts an empty host/port forms
  without validating them.
- `channel/channelapp/source/isfs.c`: `isfs_get` returns after
  `ISFS_GetFileStats` fails without closing the opened descriptor. The allocation
  result from `pmemalign` is also used without a null check. Model file ownership
  with scoped Rust values and return errors on allocation failure.
- `channel/channelapp/source/loader_reloc.c`: ELF/DOL parsing uses offsets,
  section sizes, and address additions without consistently checking overflow or
  confirming the full header/table fits in the input. Validate every input range
  before reading or writing, including BSS/memory ranges and entry point.
- `channel/channelapp/source/appentry.c`: app directory scanning increments
  `entry_count_all` and then checks the limit. The current bound appears safe for
  the fixed array, but makes the capacity rule easy to break; use a collection
  with an explicit capacity or check before insertion in Rust.
- `channel/channelapp/source/tex.c`: PNG decoding assumes allocation succeeds
  and that dimensions/layout fit the hard-coded 4x4 tiled conversion. Keep strict
  dimension and length validation and handle allocation/decode errors explicitly.
- A reported hardware issue is that HBC can freeze when it runs from NAND with a
  USB HID controller connected and no USB storage device. The source scans device
  interfaces from a worker thread and attempts USB storage startup when no storage
  device is active (`channel/channelapp/source/main.c` and `appentry.c`). This is a
  plausible USB enumeration interaction, but it is not reproduced or confirmed.
  The Rust scan should avoid blocking the UI and should test that unrelated USB
  devices do not prevent NAND startup or input processing.

The repository README says the published source differs from the official build
and excludes the installer. Preserve that provenance context when comparing
behavior. The source README also says the original was tested only in Dolphin.

## Wii SD packages

After converting the normal Wii ELF to `target/hbc-rs.dol`, run
`python3 tools/package-wii.py`. For a startup-diagnostics build converted to
`target/hbc-rs-startup.dol`, run `python3 tools/package-wii.py --diagnostics`.
The ZIPs contain `apps/hbc-rs` and `apps/hbc-rs-diagnostic`, respectively.
Both metadata files use the same no-IOS-reload and AHB-access tags as the
working local C reference package. These are applications, not channel installers.
