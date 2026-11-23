# Reborn browser controller API acceptance in Dolphin

The `app-controller` production acceptance case passed with result `0` in
Dolphin 2609a. It checked seven-entry list and grid navigation, pointer app
selection and paging, and the public `AppController::handle_pointer_frame`
detail memo drag path. The drag moved the long description by one measured line
and retained the scroll position after release.

The acceptance image was built from the current `hbc-rs` worktree with the
`emulator-acceptance` feature. Its SHA-256 is
`9a80ce98d5b7f69b1791809d6a2131acd6324ea2ab60ce620114e4568de46547`.

Run from the `hbc-rs` repository root:

```sh
python3 tools/test_production_acceptance_dolphin.py \
  --dol target/hbc-rs-app-controller-acceptance-r6.dol \
  --elf ../libogc-rs/target/xtask/custom-dol-build/powerpc-nintendo-none/release/emulator-acceptance \
  --output docs/review/evidence/reborn-app-controller-rust-dolphin-2026-11-23-r7 \
  --cases app-controller
```

The passing result is in `result.json`; `app-controller.log` records Dolphin
shutdown. The runner used a private profile. This checks controller semantics
with fixed pointer targets. It does not verify rendered hit-map geometry, live
IR input, or physical Wii behavior.
