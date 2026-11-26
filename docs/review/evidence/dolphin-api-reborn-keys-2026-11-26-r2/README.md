# Reborn HBCK key APIs in Dolphin

The `reborn-keys-api` acceptance case calls the production HBCK APIs in the
PowerPC build. It checks header length, supported and unsupported key mapping,
queue filtering and wraparound, HOME handling, twelve-frame pacing, and the
production `merge_reborn_keys` empty-queue path. The direct case returned 0 in
Dolphin. The result and matching DOL manifest are in this directory.

The positive network-to-menu path has separate Dolphin evidence in
[`reborn-hbck-replies-rust-dolphin-2026-11-24`](../reborn-hbck-replies-rust-dolphin-2026-11-24/README.md).
That run reported `reborn_keys: true` and covered filtered, HOME, Back, empty,
and capacity-boundary HBC TCP requests. The host tests continue to cover the
full queue contract. Physical Wii behavior and Reborn's unsupported `1`, `2`,
and `w` actions remain open.
