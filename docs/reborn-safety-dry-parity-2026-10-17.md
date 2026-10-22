# HBC Reborn → Rust HBC safety and DRY parity review

Reference: uploaded patched `HBC_Reborn_screenshot_SD_safety_source.zip` (not just older upstream C). Rust sources: uploaded `hbc-rs-oct-rs.zip` and `libogc-rsoct17.zip`.

## Scope

Match observable HBC Reborn channel functionality while allowing intentional divergences for stronger input validation, hardware safety, explicit error reporting, and reusable Rust implementations. No C runtime implementation is linked into the Rust port. No destructive device action is added.

## Verified source findings

| Feature | Reborn C source | Rust source | Status and next evidence |
| --- | --- | --- | --- |
| Screenshot from channel | `channel/channelapp/source/tex.c` | `src/platform/video.rs`, `src/png.rs`, `src/screenshot.rs`, `src/platform/storage.rs`, `src/main.rs` | Rust EFB-to-PNG save exists. The new preflight skips capture when storage is not mounted, while the save path still checks storage. Test real Wii missing SD, removed SD, USB, and write failure. |
| HOME overlay / Shot | `sdk/hbc_agent/overlay.c`, `channel/channelapp/source/home.c` | `src/platform/input.rs` and browser flow | **Not equivalent**: Reborn's in-app agent HOME overlay has a dedicated Shot action for loaded apps and writes BMP to `sd:/screenshots`. Rust's existing screenshot chord is in the channel and writes PNG to active volume. Track this separately; do not mark agent-overlay parity complete. |
| Screenshot file integrity | `sdk/hbc_agent/overlay.c` | `src/screenshot.rs`, `src/platform/storage.rs` | Rust uses exclusive create, retries collisions, checks writes, flushes, and removes incomplete files. Retain one shared screenshot filename/save engine. Physical removal and write-failure tests are needed. |
| Storage absence | patched C agent overlay checks SD insertion and mount | `src/platform/startup.rs`, `src/platform/storage.rs`, `src/main.rs` | Rust already returns NoStorage when absent. New guard avoids GPU capture/PNG encoding before failure. Confirm live device removal. |
| App browser and launch | `channel/channelapp` | `src/app.rs`, `src/runtime.rs`, `src/loader` | Broad functionality documented in `docs/port-status.md`; boot, IOS, SD Gecko and unsafe device operations still need real-hardware validation. |
| Network and updater | Reborn network handling, optional updater | `src/wiiload.rs`, `src/network_scan.rs` | Exclusions are already described as partial in `docs/port-status.md`. Do not claim full parity. |

## Implementation in this checkpoint

- `src/main.rs`: skip expensive EFB capture and PNG allocation if `Startup::has_storage()` is false, and show the existing error notice.
- Keep `Startup::save_screenshot_png`'s independent NoStorage error and FatStorage's exclusive-write error handling. This covers storage becoming unavailable between the initial check and the write. Do not duplicate screenshot filename or file-writing policy.

## Validation

- Read and compared source paths listed above.
- `python3 tools/parity.py check`: fails on a pre-existing invalid source reference `source/main.c#main_pre` (ledger needs synchronization with the chosen Reborn baseline). Not marked passing.
- No `rustc` or `cargo` executable is available in this environment. This patch has not been Rust-compiled or run on Dolphin / a Wii.

## Next work, in order

1. Add a scoped Rust HOME overlay architecture and map C overlay lifecycle and input operations before implementation. Avoid porting the C overlay code as a second parallel UI engine.
2. Reuse a common screenshot capture/save policy for channel and HOME overlay; keep format/path differences explicit where needed for parity.
3. Validate no-SD, hot-unplug, full-card, and partial-write failure on real Wii. Confirm error notice and unchanged existing files.
4. Update the parity ledger's source mapping to the patched Reborn baseline without falsifying status; then run its check and available hardware/Dolphin probes.

## HOME agent overlay control checkpoint

- `src/home_overlay.rs` introduces a shared, hardware-independent state machine for overlay operation requests and completion. It prevents re-entry and concurrent commands. It does not report a successful exit when the hardware operation failed, and it keeps the overlay available after a failed screenshot.
- The module is exported from `src/lib.rs`. It has focused host-side unit tests for failure/retry and exit-state consistency.
- **Not full parity:** the loaded-application-side framebuffer capture, controller input, overlay renderer, callback bridge, SD-backed BMP capture and restore of rumble/IR/app framebuffer are not implemented here. No UI claims to provide the HOME agent overlay until these adapters are implemented and tested on a Wii.
- Next production integration: implement a real application-side adapter using libogc-rs (framebuffer ownership and cleanup first), then use this state machine for requests; implement a BMP output adapter sharing the existing safe screenshot naming and exclusive-write policy rather than copy that engine.
