# Final HBC-RS DOL input smoke check

The final release DOL completed all six Pipe-backed Wii Remote input captures
in Dolphin 2609a and exited with code 0. Its SHA-256 is
`519fd7b006a37184ac98154625bac1e83bfe62885bf3c621baeeadd37231103f`.
Screenshot review confirms pointer movement and app selection.

The DOL uses Reborn HBC's GameCube port-0 rule for button edges, stick input,
and sample validity. This Dolphin run covers emulated Wii Remote input only.
Host tests cover GameCube port selection. Physical Wii input, Remote pairing,
and startup timing remain unverified.

The raw `console.log` and `dolphin.log` files are preserved without
normalization. The successful run used the x86_64 Dolphin process with normal
macOS app access after a sandboxed launch failed on pasteboard access.
