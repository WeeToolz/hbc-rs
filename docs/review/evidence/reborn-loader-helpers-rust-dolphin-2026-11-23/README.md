# Reborn loader policy and helper APIs in Dolphin

The `loader-helpers` production acceptance case passed in Dolphin. It calls
these public APIs directly:

- `ios_launch_policy` and `app_ios_launch_policy` with both independent
  `no_ios_reload` and `ahb_access` flags;
- `build_launch_arguments` with an executable path, two arguments, and an
  embedded-NUL rejection case; and
- `validate_load_plan` with a valid load plan and an invalid entry point.

Reborn reads the two flags in `main.c` before IOS reload and AHB handling. It
copies the command string in `loader_reloc.c#set_argv`. Rust encodes separate
NUL-terminated arguments to preserve argument boundaries; this is an intentional
Rust extension from Reborn's space-joined argument string. Rust also validates
the full load plan before it writes memory, which strengthens the checks in
Reborn's relocation path.

This is partial helper evidence. It does not exercise actual IOS reload, AHB
protection, or application-specific `__argv` parsing on a physical Wii. The DOL
and ELF parser and handoff checks are recorded separately in
[the loader smoke evidence](../reborn-loader-rust-dolphin-2026-11-23/README.md).
