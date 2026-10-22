# Wii exit, USB, and menu input review — 2026-10-07

## Issue and cause

The user reports that System Menu exit stops with “Could not finish Wii Remote shutdown. System exit stopped.” USB remains unavailable. Network Settings and other buttons do not respond.

The main loop cleared all controller input while either storage worker was active. Options starts a device availability check every 30 frames. A slow USB check therefore blocked menu input. The storage APIs already check worker ownership before operations that need exclusive access.

System exit stopped when asynchronous Wii Remote shutdown did not complete within its deadline. The C controls cleanup does not make that result a condition for SYS_ResetSystem. Returning to the menu after clearing controller state also leaves controls unavailable. The underlying Bluetooth close failure is not proved or fixed by this change.

## Production changes

- Keep controller input active during storage checks. Give both scan and device probe workers a bounded execution slice.
- Queue System Menu and power requests until storage scans, device probes, and received ZIP installation finish. Preserve power request priority. Stop scheduling new probes while an exit is pending.
- After settings, storage, disc, and network cleanup complete, record the actual controller cleanup error and Bluetooth diagnostics, then perform the requested system reset. This does not return a false controller cleanup success. Application and BootMii handoff still require controller cleanup completion because an old IOS callback can refer to the old application memory.
- Publish the actual USB probe stage, backend result, and candidate count. A USB FAT mount failure includes that snapshot in the existing error notice. These values come from production host initialization, enumeration, device open, LUN discovery, mount, and reset operations. This read-only diagnostic API does not prove device presence or successful I/O.

USB detection remains open. The next Wii report must identify the failed stage and result before another backend change is selected. Network configuration load and save behavior also requires hardware verification.

## Verification

- HBC host suite: 238 passed, 0 failed, 0 ignored. New shared production regressions check deferred exit, power priority, failure reporting before reset, and Network Settings selection with an empty catalog.
- USB startup shared engine: 1 standalone host test passed with edition 2021. It checks operation failures and the public diagnostic snapshot. This is state publication evidence, not USB hardware evidence.
- Normal and startup diagnostic PowerPC release builds passed with no compiler warnings.
- Both Homebrew Channel packages passed ZIP integrity, exact DOL bytes, and launch metadata checks.
- SDK inventory: 2,855 API entries. Source structure audit passed with three existing keyboard queue debt notes. Completeness audit still fails with 2,238 existing findings. No finding was suppressed. The new read-only diagnostic API has explicit shared host evidence; hardware backend claims remain unverified.

## Hardware limits and next checks

A desktop build or Dolphin capture does not prove Wii USB operation, controller shutdown completion, network persistence, or app handoff. On the Wii, check System Menu exit and Network Settings selection. Record the USB error notice, including stage, result, and candidate count. Preserve the working C reference for comparison. No NAND or firmware write was performed.

## CARROT deployment

The normal app was copied to `/Volumes/CARROT/apps/hbc-rs/boot.dol` at 19:37:38 EDT on 2026-10-07. Its SHA-256 is `c2297ed053fd2a85e774551850948944da3f8d245ce1f8a6ff1c8e83bd385077`.

The diagnostic app was copied to `/Volumes/CARROT/apps/hbc-rs-diagnostic/boot.dol` at 19:37:42 EDT. Its SHA-256 is `c8187473a5471f2628cf760c2f90dd924ddb06c78057383a182499e1b8308292`.

Both copied DOLs and metadata files match the prepared packages byte for byte. Files were flushed and storage was synchronized. Previous Rust app files were backed up under `target/wii-followup-previous/apps`. The C reference hash remained `93f6c6826e534986dcc3045f0eb3fa04fcc3bbfa803d348897843e6cbc79737d`.

Dolphin startup capture completed after 25 seconds. The rendered notice shows the real diagnostic snapshot: `Enumerate`, result `-22`, candidates `0`. The text is inside the notice box. This is a desktop result and must not be used as the Wii USB diagnosis. Capture: `target/wii-followup-dolphin.png`.

## Follow-up: USB watcher order and controller selection

The next Wii report showed `DeviceChange`, result `-1`, candidates `0`. The user reports that the same drive works with C HBC. Source comparison found that C starts both USB v5 host watchers before returning from `USB_Initialize`, while Rust waited for the vendor watcher before starting the HID watcher. Rust now starts both watchers before waiting for either one. This restores the C startup order. The change is in `libogc-rs/crates/services/ogc-usb-sys/src/lib.rs::initialize_hosts`.

The normal PowerPC release DOL was rebuilt and copied to `/Volumes/CARROT/apps/hbc-rs/boot.dol` on 2026-10-07 at 23:18 EDT. The local and CARROT files match byte for byte. SHA-256: `c07d2c8ca889ff131e474c86a22ee20934a6eb8bb0ace97f2ea633d09d6b8392`. The Wii must still confirm whether the USB watcher change fixes enumeration.

Controller checks passed: the runtime path selects Network setup after Options, Down, Confirm, and the input mapping check covers Down for both Wii Remote and GameCube controllers. These checks do not replace a Wii controller test.


## 2026-10-08 loading freeze follow-up

The user reports that the loading circle stops and HOME does not open the menu with DOL `a962d958c1acee48635b5e1fb3eb31ab5ce48460f27dcf6fc51d9842901fee51`. Source inspection found that the IPC heap lock can be held by a lower-priority storage worker when the priority-126 deferred completion worker preempts it. A yielding spin waiter cannot release the lower-priority owner. C heap allocation/free mask interrupts. The SDK now uses the same short critical-section protection for heap operations, with shared ordering regressions. See `../libogc-rs/docs/review/libipc-core.md` for the source evidence and checks. This identifies a real defect; the exact Wii stall location and recovery with the new DOL remain hardware-pending.

The rebuilt DOL was copied to `/Volumes/CARROT/apps/hbc-rs/boot.dol` and compared byte for byte with the local output. Both SHA-256 hashes are `1a11ad1ace6fae2c0e130b0037f811088da43a6d502fe112086b4aacc94ad1e9`. The completeness audit remains blocked by the existing stale evidence entry for `aesnd-sys::get_dsp_process_time` at line 914; no completeness pass is claimed.


## 2026-10-08 selected DOL launch follow-up

The user reports that a DOL launch freezes while the HBC image remains visible. The selected application and exact execution stage were not identified. `transfer_executable` stages the image, saves changed settings, unmounts storage, stops disc/network/controller services, then enters the IOS/trampoline handoff. The shared NAND filename fix also corrects the invalid staging names used by the settings-save step. SDK source inspection found the same priority-preemption defect in the short IPC request queue lock as in the previously fixed heap lock; request queue sections now use the shared interrupt-masked lock. Host lock ordering tests pass; final DOL launch on Wii remains pending. See libogc-rs `docs/review/libipc-core.md` for the bounded change and limits.

Final combined DOL: release build passed without warnings; copied to `/Volumes/CARROT/apps/hbc-rs/boot.dol`, flushed, and verified byte for byte. Local and CARROT SHA-256: `f7047b3d75b52ef1e661891a2d3e94f4d2338c36960b3e461c0808b6e4dccca3`. The source audit passed with existing keyboard debt; the SDK completeness audit remains blocked by stale AESND evidence at line 914. Network save, wider keyboard output, and selected-app launch require Wii verification.


## 2026-10-21 normal DOL loading freeze

The user reports that the normal HBC-RS DOL stops on the loading screen. The
reported copied file is 2,446,272 bytes with SHA-256
`557aec551755afe7a977872d6b35e6e9ed0f9e5af2a66a8b15cba23d89d2085f`. The
CARROT volume was not mounted in the build workspace, so these file values are
the user's report and were not read back here.

Source comparison found that the shared development wrapper called `sys_init`
and then enabled the PowerPC EE and RI bits before application `main`. A normal
DOL did not enable those bits after `sys_init`. A direct DOL launch can leave
interrupt delivery disabled. HBC-RS waits for video timing during startup, so
this can leave the loader screen visible. This is the likely cause; the current
Wii run did not provide a captured frame or an application report to identify
the exact stop point.

The shared `ogc-system-sys::sys_init` path now enables EE and RI after runtime
vectors and the scheduler are set up. The development wrapper uses this same
path and no longer has a separate MSR update. A focused `ogc-sys` regression
checks the independent expected EE/RI mask and preservation of other bits. The
normal PowerPC DOL builds with development mode disabled and contains no
development entry symbols. Its SHA-256 is
`c6c927a5031739bc698de5c5945b4c623c5ac2d458372efd654a23bb6df5ed51`; size is
2,446,304 bytes.

The signed-in Terminal Wiiload readiness check passed at `192.168.2.3:4299`.
The capture device enumerated as `1c88:003c`, but its capture worker stopped
before receiving a block and produced no frame. The capture result reports
`Other error`; a later device info query still found the device connected. No
DOL was uploaded. The read-only `hbc-control status` query was reset by the Wii.
This matches stock HBC rejecting that protocol. The successful Wiiload
readiness check and status reset indicate that stock HBC still handled the
endpoint, but do not identify the exact DOL stop point. The installed Dolphin
binary also refused to start because its processor feature check requires NEON.
Physical boot and the exact stop point remain unverified. Keep this behavior
hardware-pending.
