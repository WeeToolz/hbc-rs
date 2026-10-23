//! GameCube PAD and Wii Remote input adapter.
//!
//! Inputs retain their press, hold, and release state. Callers can use press
//! events for one-shot actions and hold events for repeatable navigation.

use alloc::vec::Vec;
#[cfg(target_arch = "powerpc")]
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use ogc_controller::{Buttons as ControllerButtons, ControllerKind, project_stick_directions};

const RIGHT_STICK_THRESHOLD: i16 = 8192;
#[cfg(any(target_arch = "powerpc", test))]
use ogc_controller::{
    ControllerSnapshot, ControllerSource, DigitalStickMapper, MAX_CONTROLLERS, StickDirectionError,
    StickDirectionFrame, StickDirectionSample,
};

use crate::app::Input;
#[cfg(target_arch = "powerpc")]
use ogc_controller::ControllerManager;

#[cfg(target_arch = "powerpc")]
static WPAD_STARTED: AtomicBool = AtomicBool::new(false);
#[cfg(target_arch = "powerpc")]
static WPAD_VIEW_WIDTH: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "powerpc")]
static WPAD_VIEW_HEIGHT: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "powerpc")]
static mut CONTROLLER_MANAGER: ControllerManager = ControllerManager::new();
#[cfg(target_arch = "powerpc")]
static mut RIGHT_STICK_MAPPER: DigitalStickMapper<MAX_CONTROLLERS> = DigitalStickMapper::new();

#[cfg(target_arch = "powerpc")]
fn report_bluetooth_startup(stage: &'static str) {
    ogc_system::report(format_args!("HBC: Bluetooth startup: {stage}\n"));
}

#[cfg(target_arch = "powerpc")]
fn report_bluetooth_event(event: &'static str) {
    ogc_system::report(format_args!("HBC: Bluetooth event: {event}\n"));
}

#[cfg(target_arch = "powerpc")]
fn report_bluetooth_diagnostics(diagnostics: ogc_bt_sync::HostDiagnostics) {
    ogc_system::report(format_args!(
        "HBC: Bluetooth diagnostics: {diagnostics:?}\n"
    ));
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InputFrame {
    pub pressed: Vec<Input>,
    pub held: Vec<Input>,
    pub released: Vec<Input>,
    pub pointer: Option<PointerPosition>,
    pub pointer_pressed: bool,
    pub pointer_held: bool,
    /// Signed HBC-style vertical stick scroll in logical pixels for this frame.
    pub stick_scroll_pixels: i16,
    /// Directions pressed by a Right Joy-Con stick. Runtime suppresses only a
    /// matching direction when memo scrolling consumes that same movement.
    pub stick_navigation: Vec<Input>,
    /// Capture the rendered frame when the HBC network/screenshot chord is pressed.
    pub screenshot_requested: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PointerPosition {
    pub x: i16,
    pub y: i16,
}

/// Convert padded Wii Remote IR coordinates to the logical HBC viewport.
/// HBC sets the sensor resolution to the viewport plus 128 pixels and removes
/// 64 pixels from each axis when it reads the pointer.
pub fn logical_pointer_position(ir_x: f32, ir_y: f32) -> Option<PointerPosition> {
    let x = ir_x - 64.0;
    let y = ir_y - 64.0;
    (x.is_finite() && y.is_finite()).then_some(PointerPosition {
        x: x as i16,
        y: y as i16,
    })
}

#[cfg(any(target_arch = "powerpc", test))]
fn pointer_confirm_pressed(owner_present: bool, pressed: Buttons) -> bool {
    owner_present && pressed.0 & Buttons::CONFIRM != 0
}

#[cfg(any(target_arch = "powerpc", test))]
fn pointer_confirm_held(owner_present: bool, held: Buttons) -> bool {
    owner_present && held.0 & Buttons::CONFIRM != 0
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Buttons(u16);

#[cfg(any(target_arch = "powerpc", test))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct RawWpadTransitions {
    extension: Option<Extension>,
    down: u32,
    held: u32,
    up: u32,
}

#[cfg(any(target_arch = "powerpc", test))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct MappedInput {
    down: Buttons,
    held: Buttons,
    up: Buttons,
}

impl Buttons {
    const UP: u16 = 1 << 0;
    const DOWN: u16 = 1 << 1;
    const LEFT: u16 = 1 << 2;
    const RIGHT: u16 = 1 << 3;
    const CONFIRM: u16 = 1 << 4;
    const BACK: u16 = 1 << 5;
    const HOME: u16 = 1 << 6;
    const BROWSE_MODE: u16 = 1 << 7;
    const PREVIOUS: u16 = 1 << 9;
    const NEXT: u16 = 1 << 10;
    const OPTIONS: u16 = 1 << 11;
    const NETWORK_INIT: u16 = 1 << 12;
    const SCREENSHOT: u16 = 1 << 13;
}

/// Map button masks to HBC logical actions. This is separate from scanning so
/// host tests can check the mapping without a Wii or GameCube controller.
pub fn map_buttons(gamecube: u16, wiimote: u32) -> ButtonsForTest {
    let mut mapped = Buttons::default();
    map_pad(gamecube, &mut mapped);
    map_wiimote(wiimote, &mut mapped);
    ButtonsForTest(mapped.0)
}

/// Public mask result for host-side mapping checks and platform diagnostics.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ButtonsForTest(u16);

impl ButtonsForTest {
    pub fn contains(self, input: Input) -> bool {
        self.0 & bit_for(input) != 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Extension {
    Nunchuk,
    Classic,
    GuitarHero3,
}

#[cfg(target_arch = "powerpc")]
static POINTER_OWNER: AtomicI32 = AtomicI32::new(-1);

/// Convert supported Classic Controller buttons to the matching Remote buttons.
pub fn map_extension_buttons(mask: u32, extension: Extension) -> u32 {
    let mut result = mask;
    match extension {
        Extension::Nunchuk => {}
        Extension::Classic => {
            for (source, target) in [
                (0x0001 << 16, 0x0800),            // up
                (0x0002 << 16, 0x0100),            // left
                (0x4000 << 16, 0x0400),            // down
                (0x8000 << 16, 0x0200),            // right
                (0x0010 << 16, 0x0008),            // A
                (0x0040 << 16, 0x0004),            // B
                (0x0008 << 16, 0x0002),            // X -> Remote 1
                (0x0020 << 16, 0x0001),            // Y -> Remote 2
                ((0x2000 | 0x1000) << 16, 0x0010), // L/minus
                ((0x0200 | 0x0400) << 16, 0x1000), // R/plus
                (0x0800 << 16, 0x0080),            // Home
            ] {
                if mask & source != 0 {
                    result |= target;
                }
            }
        }
        Extension::GuitarHero3 => {
            for (source, target) in [
                (0x0001 << 16, 0x0800), // strum up
                (0x4000 << 16, 0x0400), // strum down
                (0x0010 << 16, 0x0008), // green -> A
                (0x0040 << 16, 0x0004), // red -> B
                (0x0400 << 16, 0x1000), // plus
                (0x1000 << 16, 0x0010), // minus
                (0x0008 << 16, 0x0100), // yellow -> left
                (0x0020 << 16, 0x0200), // blue -> right
                (0x0080 << 16, 0x0002), // orange -> 1
            ] {
                if mask & source != 0 {
                    result |= target;
                }
            }
        }
    }
    result
}

fn map_pad(mask: u16, out: &mut Buttons) {
    // Values match the public PAD masks in ogc-input-sys.
    map_directions(mask, 0x0008, 0x0004, 0x0001, 0x0002, out);
    if mask & 0x0100 != 0 {
        out.0 |= Buttons::CONFIRM;
    }
    if mask & 0x0200 != 0 {
        out.0 |= Buttons::BACK;
    }
    if mask & 0x0400 != 0 {
        out.0 |= Buttons::OPTIONS;
    }
    if mask & 0x0800 != 0 {
        out.0 |= Buttons::BROWSE_MODE;
    }
    if mask & 0x1000 != 0 {
        out.0 |= Buttons::HOME;
    }
    if mask & 0x0040 != 0 {
        out.0 |= Buttons::PREVIOUS;
    }
    if mask & 0x0020 != 0 {
        out.0 |= Buttons::NEXT;
    }
    #[cfg(any(target_arch = "powerpc", test))]
    if mask & 0x0010 != 0 {
        out.0 |= Buttons::NETWORK_INIT;
    }
}

fn map_wiimote(mask: u32, out: &mut Buttons) {
    // Base Wii Remote and Classic Controller masks from wpad-sys.
    map_directions(mask as u16, 0x0800, 0x0400, 0x0100, 0x0200, out);
    if mask & 0x0008 != 0 {
        out.0 |= Buttons::CONFIRM;
    }
    if mask & 0x0004 != 0 {
        out.0 |= Buttons::BACK;
    }
    if mask & 0x0080 != 0 {
        out.0 |= Buttons::HOME;
    }
    if mask & 0x0002 != 0 {
        out.0 |= Buttons::OPTIONS;
    }
    if mask & 0x0001 != 0 {
        out.0 |= Buttons::BROWSE_MODE;
    }
    if mask & 0x0010 != 0 {
        out.0 |= Buttons::PREVIOUS;
    }
    if mask & 0x1000 != 0 {
        out.0 |= Buttons::NEXT;
    }
}

fn map_directions(mask: u16, up: u16, down: u16, left: u16, right: u16, out: &mut Buttons) {
    if mask & up != 0 {
        out.0 |= Buttons::UP;
    }
    if mask & down != 0 {
        out.0 |= Buttons::DOWN;
    }
    if mask & left != 0 {
        out.0 |= Buttons::LEFT;
    }
    if mask & right != 0 {
        out.0 |= Buttons::RIGHT;
    }
}

#[cfg(any(target_arch = "powerpc", test))]
fn map_input_masks(
    pad_down: u16,
    pad_held: u16,
    pad_up: u16,
    wpad: [RawWpadTransitions; 4],
    pointer_owner: Option<usize>,
    pad_sample_valid: bool,
) -> MappedInput {
    let mut mapped = MappedInput::default();
    if pad_sample_valid {
        map_pad(pad_down, &mut mapped.down);
        map_pad(pad_held, &mut mapped.held);
    }
    map_pad(pad_up, &mut mapped.up);
    for (channel, transitions) in wpad.into_iter().enumerate() {
        // Releases from disconnected controllers must reach the input frame,
        // even when there is no current raw WPAD sample or pointer owner.
        map_wpad_buttons(transitions.extension, transitions.up, &mut mapped.up);
        if pointer_owner.is_some_and(|owner| owner != channel) {
            continue;
        }
        map_wpad_buttons(transitions.extension, transitions.down, &mut mapped.down);
        map_wpad_buttons(transitions.extension, transitions.held, &mut mapped.held);
    }
    mapped
}

fn map_normalized_controller_buttons(kind: ControllerKind, mask: u32, out: &mut Buttons) {
    // These are HBC application defaults for normalized USB and Bluetooth
    // controllers. The C HBC mappings above remain authoritative for PAD and
    // WPAD sources.
    map_directions(
        mask as u16,
        ControllerButtons::UP.bits() as u16,
        ControllerButtons::DOWN.bits() as u16,
        ControllerButtons::LEFT.bits() as u16,
        ControllerButtons::RIGHT.bits() as u16,
        out,
    );
    let bindings: &[(ControllerButtons, u16)] = match kind {
        ControllerKind::GameCube => &[
            (ControllerButtons::A, Buttons::CONFIRM),
            (ControllerButtons::B, Buttons::BACK),
            (ControllerButtons::X, Buttons::OPTIONS),
            (ControllerButtons::Y, Buttons::BROWSE_MODE),
            (ControllerButtons::START, Buttons::HOME),
            (ControllerButtons::L, Buttons::PREVIOUS),
            (ControllerButtons::R, Buttons::NEXT),
        ],
        ControllerKind::JoyConLeft => &[
            (ControllerButtons::SR, Buttons::CONFIRM),
            (ControllerButtons::SL, Buttons::BACK),
            (ControllerButtons::SELECT, Buttons::HOME),
            (ControllerButtons::L, Buttons::PREVIOUS),
            (ControllerButtons::ZL, Buttons::NEXT),
            (ControllerButtons::L_STICK, Buttons::OPTIONS),
            (ControllerButtons::CAPTURE, Buttons::BROWSE_MODE),
        ],
        ControllerKind::JoyConRight => &[
            (ControllerButtons::A, Buttons::CONFIRM),
            (ControllerButtons::B, Buttons::BACK),
            (ControllerButtons::X, Buttons::OPTIONS),
            (ControllerButtons::Y, Buttons::BROWSE_MODE),
            (ControllerButtons::START, Buttons::NEXT),
            (ControllerButtons::R, Buttons::PREVIOUS),
            (ControllerButtons::HOME, Buttons::HOME),
        ],
        ControllerKind::JoyConPair | ControllerKind::SwitchPro => &[
            (ControllerButtons::A, Buttons::CONFIRM),
            (ControllerButtons::B, Buttons::BACK),
            (ControllerButtons::X, Buttons::OPTIONS),
            (ControllerButtons::Y, Buttons::BROWSE_MODE),
            (ControllerButtons::SELECT, Buttons::PREVIOUS),
            (ControllerButtons::START, Buttons::NEXT),
        ],
        _ => &[
            (ControllerButtons::A, Buttons::CONFIRM),
            (ControllerButtons::B, Buttons::BACK),
            (ControllerButtons::X, Buttons::OPTIONS),
            (ControllerButtons::Y, Buttons::BROWSE_MODE),
            (ControllerButtons::START, Buttons::HOME),
            (ControllerButtons::SELECT, Buttons::OPTIONS),
            (ControllerButtons::L, Buttons::PREVIOUS),
            (ControllerButtons::R, Buttons::NEXT),
        ],
    };
    for (source, target) in bindings {
        if mask & source.bits() != 0 {
            out.0 |= target;
        }
    }
    if matches!(
        kind,
        ControllerKind::JoyConLeft
            | ControllerKind::JoyConRight
            | ControllerKind::JoyConPair
            | ControllerKind::SwitchPro
    ) && mask & ControllerButtons::HOME.bits() != 0
    {
        out.0 |= Buttons::HOME;
    }
}

#[cfg(any(target_arch = "powerpc", test))]
fn is_additional_source(source: ControllerSource) -> bool {
    matches!(
        source,
        ControllerSource::UsbGameCube { .. }
            | ControllerSource::UsbHid(_)
            | ControllerSource::BluetoothHid(_)
            | ControllerSource::BluetoothHidPair(_, _)
    )
}

fn normalized_stick_scroll(kind: ControllerKind, axes: ogc_controller::Axes) -> i32 {
    let axis = match kind {
        ControllerKind::JoyConRight => axes.right_y,
        _ => axes.left_y,
    };
    deadzone(i32::from(axis) / 256)
}

#[cfg(any(target_arch = "powerpc", test))]
fn map_additional_controllers<const N: usize>(
    snapshot: &ControllerSnapshot,
    mapped: &mut MappedInput,
    stick_mapper: &mut DigitalStickMapper<N>,
) -> Result<(i32, Buttons), InputScanError> {
    let mut stick_scroll = 0;
    let mut samples =
        [const { core::mem::MaybeUninit::<StickDirectionSample>::uninit() }; MAX_CONTROLLERS];
    let mut sample_count = 0;
    for controller in snapshot.controllers() {
        if !is_additional_source(controller.source) {
            continue;
        }
        map_controller_edges(controller, mapped);
        stick_scroll += normalized_stick_scroll(controller.kind, controller.axes);
        if controller.kind == ControllerKind::JoyConRight && controller.connected {
            let id = snapshot
                .controller_id(controller.source)
                .ok_or(InputScanError::MissingControllerIdentity(controller.source))?;
            samples[sample_count].write(StickDirectionSample {
                id,
                x: controller.axes.right_x,
                y: controller.axes.right_y,
                threshold: RIGHT_STICK_THRESHOLD,
            });
            sample_count += 1;
        }
    }
    for reset in snapshot.resets() {
        if is_additional_source(reset.state.source) {
            map_controller_edges(&reset.state, mapped);
        }
    }
    // SAFETY: each entry before sample_count was initialized above. The
    // snapshot is bounded by MAX_CONTROLLERS and the mapper validates capacity.
    let samples = unsafe {
        core::slice::from_raw_parts(
            samples.as_ptr().cast::<StickDirectionSample>(),
            sample_count,
        )
    };
    let frame = stick_mapper
        .map(samples)
        .map_err(InputScanError::StickDirection)?;
    let navigation = map_stick_direction_frame(&frame, mapped);
    Ok((stick_scroll, navigation))
}

#[cfg(any(target_arch = "powerpc", test))]
fn map_stick_direction_frame<const N: usize>(
    frame: &StickDirectionFrame<N>,
    mapped: &mut MappedInput,
) -> Buttons {
    let mut navigation = Buttons::default();
    for state in frame.states() {
        let previous_down = mapped.down;
        map_normalized_controller_buttons(
            ControllerKind::JoyConRight,
            state.action_down.to_buttons().bits(),
            &mut mapped.down,
        );
        navigation.0 |= mapped.down.0 & !previous_down.0;
        map_normalized_controller_buttons(
            ControllerKind::JoyConRight,
            state.action_held.to_buttons().bits(),
            &mut mapped.held,
        );
        map_normalized_controller_buttons(
            ControllerKind::JoyConRight,
            state.action_up.to_buttons().bits(),
            &mut mapped.up,
        );
    }
    for reset in frame.resets() {
        map_normalized_controller_buttons(
            ControllerKind::JoyConRight,
            reset.action_released.to_buttons().bits(),
            &mut mapped.up,
        );
    }
    navigation
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(any(target_arch = "powerpc", test))]
pub enum InputScanError {
    Wpad(i32),
    MissingControllerIdentity(ControllerSource),
    StickDirection(StickDirectionError),
}

#[cfg(any(target_arch = "powerpc", test))]
fn map_controller_edges(state: &ogc_controller::ControllerState, mapped: &mut MappedInput) {
    map_normalized_controller_buttons(state.kind, state.buttons_down.bits(), &mut mapped.down);
    map_normalized_controller_buttons(state.kind, state.buttons.bits(), &mut mapped.held);
    map_normalized_controller_buttons(state.kind, state.buttons_up.bits(), &mut mapped.up);
}

#[cfg(target_arch = "powerpc")]
fn report_controller_input_error(snapshot: &ControllerSnapshot) {
    for error in snapshot.errors() {
        ogc_system::report(format_args!("HBC: controller input error: {error:?}\n"));
    }
}

fn bit_for(input: Input) -> u16 {
    match input {
        Input::Up => Buttons::UP,
        Input::Down => Buttons::DOWN,
        Input::Left => Buttons::LEFT,
        Input::Right => Buttons::RIGHT,
        Input::Confirm => Buttons::CONFIRM,
        Input::Back => Buttons::BACK,
        Input::Home => Buttons::HOME,
        Input::ToggleBrowseMode => Buttons::BROWSE_MODE,
        Input::OpenOptions => Buttons::OPTIONS,
        Input::PreviousPage => Buttons::PREVIOUS,
        Input::NextPage => Buttons::NEXT,
    }
}

fn inputs(mask: Buttons) -> Vec<Input> {
    [
        Input::Home,
        Input::Up,
        Input::Down,
        Input::Left,
        Input::Right,
        Input::Confirm,
        Input::Back,
        Input::ToggleBrowseMode,
        Input::OpenOptions,
        Input::PreviousPage,
        Input::NextPage,
    ]
    .into_iter()
    .filter(|input| mask.0 & bit_for(*input) != 0)
    .collect()
}

#[cfg(any(target_arch = "powerpc", test))]
fn screenshot_chord(pressed: Buttons, held: Buttons) -> bool {
    pressed.0 & Buttons::SCREENSHOT != 0 && held.0 & Buttons::NETWORK_INIT != 0
}

fn map_screenshot_extension_buttons(mask: u32, extension: Extension, out: &mut Buttons) {
    let (network_init, screenshot) = match extension {
        Extension::Nunchuk => (0x0001 << 16, 0x0002 << 16),
        Extension::Classic => (0x0004 << 16, 0x0080 << 16),
        Extension::GuitarHero3 => return,
    };
    if mask & network_init != 0 {
        out.0 |= Buttons::NETWORK_INIT;
    }
    if mask & screenshot != 0 {
        out.0 |= Buttons::SCREENSHOT;
    }
}

#[cfg(target_arch = "powerpc")]
fn configure_wpad(view_width: u32, view_height: u32) -> Result<(), i32> {
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("WPAD SETTINGS");
    wpad_sys::set_idle_timeout(120);
    let ir_width = view_width.saturating_add(128);
    let ir_height = view_height.saturating_add(128);
    for channel in 0..4 {
        let rc = wpad_sys::set_data_format(channel, wpad_sys::WPAD_FMT_BTNS_ACC_IR);
        if rc < wpad_sys::WPAD_ERR_NONE as i32 {
            return Err(rc);
        }
        wpad_sys::set_v_res(channel, ir_width, ir_height);
    }
    WPAD_STARTED.store(true, Ordering::Release);
    Ok(())
}

#[cfg(target_arch = "powerpc")]
pub fn initialize(view_width: u16, view_height: u16) -> Result<(), i32> {
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("PAD INIT");
    // SAFETY: HBC owns this manager from startup on its single main thread.
    let manager = unsafe { &mut *core::ptr::addr_of_mut!(CONTROLLER_MANAGER) };
    manager.set_bluetooth_startup_hook(Some(report_bluetooth_startup));
    manager.set_bluetooth_event_hook(Some(report_bluetooth_event));
    manager.set_bluetooth_diagnostics_hook(Some(report_bluetooth_diagnostics));
    #[cfg(feature = "startup-diagnostics")]
    manager.set_input_scan_hook(Some(diagnostic_scan_mark));
    #[cfg(feature = "startup-diagnostics")]
    manager.set_usb_startup_hook(Some(diagnostic_scan_mark));
    manager.init();
    manager.enable_usb_hid();
    POINTER_OWNER.store(-1, Ordering::Release);
    WPAD_VIEW_WIDTH.store(u32::from(view_width), Ordering::Release);
    WPAD_VIEW_HEIGHT.store(u32::from(view_height), Ordering::Release);
    WPAD_STARTED.store(false, Ordering::Release);

    #[cfg(feature = "startup-diagnostics")]
    wpad_sys::set_bluetooth_poll_hook(Some(diagnostic_scan_mark));

    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark("WPAD INIT");
    // SAFETY: the same main-thread-owned manager is used for every scan.
    let wpad_result = unsafe { (&mut *core::ptr::addr_of_mut!(CONTROLLER_MANAGER)).enable_wpad() };
    if wpad_result == wpad_sys::WPAD_ERR_NONE as i32 {
        return configure_wpad(u32::from(view_width), u32::from(view_height));
    }
    if wpad_result == wpad_sys::WPAD_ERR_NOT_READY as i32 {
        // The adapter-open submission was not accepted yet. Do not issue WPAD
        // configuration calls against an unstarted backend. scan() retries init
        // from the normal frame loop while GameCube input remains available.
        return Ok(());
    }
    Err(wpad_result)
}

/// Scan GameCube port zero and Wii Remote channels using HBC pointer ownership.
#[cfg(target_arch = "powerpc")]
pub fn scan() -> Result<InputFrame, InputScanError> {
    use wpad_sys as wpad;

    diagnostic_scan_mark("WPAD SCAN START");
    if !WPAD_STARTED.load(Ordering::Acquire) {
        // SAFETY: HBC scans from its main thread only.
        let start = unsafe { (&mut *core::ptr::addr_of_mut!(CONTROLLER_MANAGER)).enable_wpad() };
        if start == wpad::WPAD_ERR_NONE as i32 {
            configure_wpad(
                WPAD_VIEW_WIDTH.load(Ordering::Acquire),
                WPAD_VIEW_HEIGHT.load(Ordering::Acquire),
            )
            .map_err(InputScanError::Wpad)?;
        } else if start != wpad::WPAD_ERR_NOT_READY as i32 {
            return Err(InputScanError::Wpad(start));
        }
    }
    diagnostic_scan_mark("PAD AND WPAD SCAN START");
    // SAFETY: one main-thread scan owns the shared production manager.
    let raw = unsafe { (&mut *core::ptr::addr_of_mut!(CONTROLLER_MANAGER)).scan_with_raw() };
    diagnostic_scan_mark("WPAD SCAN DONE");
    diagnostic_scan_mark("ERROR REPORT START");
    report_controller_input_error(&raw.snapshot);
    diagnostic_scan_mark("ERROR REPORT DONE");

    diagnostic_scan_mark("WPAD ERROR START");
    if let Some(error) = raw
        .snapshot
        .errors()
        .find(|error| error.operation == "WPAD poll")
        && let ogc_controller::InputFailure::Source(code) = error.failure
    {
        return Err(InputScanError::Wpad(code));
    }
    diagnostic_scan_mark("WPAD ERROR DONE");

    let mut ir_valid = [false; 4];
    for (channel, sample) in raw.wpad_data.iter().enumerate() {
        diagnostic_scan_mark(match channel {
            0 => "WPAD PROBE 0 START",
            1 => "WPAD PROBE 1 START",
            2 => "WPAD PROBE 2 START",
            _ => "WPAD PROBE 3 START",
        });
        if let Some(sample) =
            sample.filter(|sample| sample.err == wpad::WPAD_ERR_NONE && sample.data_present != 0)
        {
            ir_valid[channel] = sample.ir.valid != 0;
        }
    }
    let owner = select_pointer_owner(&ir_valid, POINTER_OWNER.load(Ordering::Acquire));
    POINTER_OWNER.store(
        owner.map_or(-1, |channel| channel as i32),
        Ordering::Release,
    );

    let mut pointer = None;
    let mut pointer_pressed = false;
    let mut stick_scroll = if raw.pad_status[0].err == ogc_input::PAD_ERR_NONE as i8 {
        deadzone(i32::from(raw.pad_status[0].stick_y))
    } else {
        0
    };
    if let Some(owner) = owner {
        if let Some(state) = raw.wpad_data[owner].as_ref() {
            stick_scroll += extension_stick_y(state);
        }
    } else {
        for state in raw.wpad_data.iter().flatten() {
            stick_scroll += extension_stick_y(state);
        }
    }
    let mut wpad_transitions = [RawWpadTransitions::default(); 4];
    for (channel, transitions) in wpad_transitions.iter_mut().enumerate() {
        transitions.extension = extension_from_raw_type(raw.wpad_extension_type[channel]);
        transitions.down = raw.wpad_buttons_down[channel];
        transitions.held = raw.wpad_buttons_held[channel];
        transitions.up = raw.wpad_buttons_up[channel];
    }
    let mut mapped = map_input_masks(
        raw.pad_buttons_down[0],
        raw.pad_buttons_held[0],
        raw.pad_buttons_up[0],
        wpad_transitions,
        owner,
        raw.pad_status[0].err == ogc_input::PAD_ERR_NONE as i8,
    );
    // SAFETY: HBC scans on one main thread and owns this mapper for its life.
    let (additional_scroll, stick_navigation) =
        map_additional_controllers(&raw.snapshot, &mut mapped, unsafe {
            &mut *core::ptr::addr_of_mut!(RIGHT_STICK_MAPPER)
        })?;
    stick_scroll += additional_scroll;
    let stick_scroll = scroll_pixels(stick_scroll);
    if let Some(owner) = owner
        && let Some(state) = raw.wpad_data[owner]
            .as_ref()
            .filter(|state| state.err == wpad::WPAD_ERR_NONE && state.data_present != 0)
    {
        pointer = logical_pointer_position(state.ir.x, state.ir.y);
        pointer_pressed = pointer_confirm_pressed(true, mapped.down);
    }
    let pointer_held = pointer_confirm_held(owner.is_some(), mapped.held);
    Ok(InputFrame {
        pressed: inputs(mapped.down),
        held: inputs(mapped.held),
        released: inputs(mapped.up),
        pointer,
        pointer_pressed,
        pointer_held,
        stick_scroll_pixels: stick_scroll,
        stick_navigation: inputs(stick_navigation),
        screenshot_requested: screenshot_chord(mapped.down, mapped.held),
    })
}

#[cfg(target_arch = "powerpc")]
#[inline(always)]
fn diagnostic_scan_mark(label: &'static str) {
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark(label);
    #[cfg(feature = "recovery-scan-validation")]
    super::diagnostics::hold_for_recovery_scan_validation(label);
    #[cfg(not(feature = "startup-diagnostics"))]
    let _ = label;
}

/// Convert the sum of HBC stick units to logical pixels once per frame.
fn scroll_pixels(units: i32) -> i16 {
    (units / 8).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

fn deadzone(value: i32) -> i32 {
    if value > 10 {
        value - 10
    } else if value < -10 {
        value + 10
    } else {
        0
    }
}

#[cfg(target_arch = "powerpc")]
fn extension_stick_y(state: &wpad_sys::WpadData) -> i32 {
    match state.exp.type_ {
        value if value == wpad_sys::WPAD_EXP_NUNCHUK as i32 => {
            let stick = unsafe { state.exp.u.nunchuk.js };
            deadzone(i32::from(stick.pos.y) - i32::from(stick.center.y))
        }
        value if value == wpad_sys::WPAD_EXP_CLASSIC as i32 => {
            let stick = unsafe { state.exp.u.classic.ljs };
            deadzone((i32::from(stick.pos.y) - i32::from(stick.center.y)) * 4)
        }
        value if value == wpad_sys::WPAD_EXP_GUITARHERO3 as i32 => {
            let stick = unsafe { state.exp.u.gh3.js };
            deadzone((i32::from(stick.pos.y) - i32::from(stick.center.y)) * 4)
        }
        _ => 0,
    }
}

#[cfg(any(target_arch = "powerpc", test))]
fn select_pointer_owner(ir_valid: &[bool; 4], previous: i32) -> Option<usize> {
    if previous >= 0 {
        let previous = previous as usize;
        if ir_valid.get(previous).copied().unwrap_or(false) {
            return Some(previous);
        }
    }
    ir_valid.iter().position(|valid| *valid)
}

fn map_wpad_buttons(extension: Option<Extension>, buttons: u32, out: &mut Buttons) {
    if let Some(extension) = extension {
        map_screenshot_extension_buttons(buttons, extension, out);
    }
    let transformed = extension
        .filter(|extension| {
            *extension == Extension::Classic || *extension == Extension::GuitarHero3
        })
        .map_or(buttons, |extension| {
            map_extension_buttons(buttons, extension)
        });
    // HBC promotes extension buttons into the base Remote mask before use.
    map_wiimote(transformed, out);
}

#[cfg(target_arch = "powerpc")]
fn extension_from_raw_type(expansion: i32) -> Option<Extension> {
    match expansion {
        value if value == wpad_sys::WPAD_EXP_NUNCHUK as i32 => Some(Extension::Nunchuk),
        value if value == wpad_sys::WPAD_EXP_CLASSIC as i32 => Some(Extension::Classic),
        value if value == wpad_sys::WPAD_EXP_GUITARHERO3 as i32 => Some(Extension::GuitarHero3),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_gamecube_and_wiimote_buttons_to_actions() {
        let pad = map_buttons(0x0100 | 0x0004 | 0x0008 | 0x0020 | 0x0400 | 0x0800, 0);
        assert!(pad.contains(Input::Confirm));
        assert!(pad.contains(Input::OpenOptions));
        assert!(pad.contains(Input::ToggleBrowseMode));
        assert!(pad.contains(Input::Up));
        assert!(pad.contains(Input::Down));
        assert!(pad.contains(Input::NextPage));
        let remote = map_buttons(0, 0x0001 | 0x0002 | 0x0008 | 0x0400 | 0x0800 | 0x1000);
        assert!(remote.contains(Input::Confirm));
        assert!(remote.contains(Input::OpenOptions));
        assert!(remote.contains(Input::ToggleBrowseMode));
        assert!(remote.contains(Input::Up));
        assert!(remote.contains(Input::Down));
        assert!(remote.contains(Input::NextPage));
        let classic_mask =
            map_extension_buttons((0x0010 << 16) | (0x8000 << 16), Extension::Classic);
        let classic = map_buttons(0, classic_mask);
        assert!(classic.contains(Input::Confirm));
        assert!(classic.contains(Input::Right));

        let classic_x = map_extension_buttons(0x0008 << 16, Extension::Classic);
        assert!(map_buttons(0, classic_x).contains(Input::OpenOptions));
        let classic_y = map_extension_buttons(0x0020 << 16, Extension::Classic);
        assert!(map_buttons(0, classic_y).contains(Input::ToggleBrowseMode));
    }

    #[test]
    fn normalized_controller_defaults_map_buttons_to_hbc_actions() {
        let mut gamecube = Buttons::default();
        map_normalized_controller_buttons(
            ControllerKind::GameCube,
            ControllerButtons::A.bits()
                | ControllerButtons::Y.bits()
                | ControllerButtons::UP.bits()
                | ControllerButtons::START.bits()
                | ControllerButtons::L.bits()
                | ControllerButtons::R.bits(),
            &mut gamecube,
        );
        assert_eq!(
            gamecube.0,
            Buttons::CONFIRM
                | Buttons::BROWSE_MODE
                | Buttons::UP
                | Buttons::HOME
                | Buttons::PREVIOUS
                | Buttons::NEXT
        );

        let mut joycon_left = Buttons::default();
        map_normalized_controller_buttons(
            ControllerKind::JoyConLeft,
            ControllerButtons::A.bits()
                | ControllerButtons::B.bits()
                | ControllerButtons::SR.bits()
                | ControllerButtons::SL.bits()
                | ControllerButtons::SELECT.bits()
                | ControllerButtons::L.bits()
                | ControllerButtons::ZL.bits()
                | ControllerButtons::L_STICK.bits()
                | ControllerButtons::CAPTURE.bits(),
            &mut joycon_left,
        );
        assert_eq!(
            joycon_left.0,
            Buttons::CONFIRM
                | Buttons::BACK
                | Buttons::HOME
                | Buttons::PREVIOUS
                | Buttons::NEXT
                | Buttons::OPTIONS
                | Buttons::BROWSE_MODE
        );

        let mut joycon_right = Buttons::default();
        map_normalized_controller_buttons(
            ControllerKind::JoyConRight,
            ControllerButtons::A.bits()
                | ControllerButtons::B.bits()
                | ControllerButtons::START.bits()
                | ControllerButtons::R.bits()
                | ControllerButtons::HOME.bits(),
            &mut joycon_right,
        );
        assert_eq!(
            joycon_right.0,
            Buttons::CONFIRM | Buttons::BACK | Buttons::NEXT | Buttons::PREVIOUS | Buttons::HOME
        );

        let mut pro = Buttons::default();
        map_normalized_controller_buttons(
            ControllerKind::SwitchPro,
            ControllerButtons::SELECT.bits()
                | ControllerButtons::START.bits()
                | ControllerButtons::HOME.bits()
                | ControllerButtons::L.bits(),
            &mut pro,
        );
        assert_eq!(pro.0, Buttons::PREVIOUS | Buttons::NEXT | Buttons::HOME);

        let mut pair = Buttons::default();
        map_normalized_controller_buttons(
            ControllerKind::JoyConPair,
            ControllerButtons::X.bits() | ControllerButtons::Y.bits(),
            &mut pair,
        );
        assert_eq!(pair.0, Buttons::OPTIONS | Buttons::BROWSE_MODE);
    }

    #[test]
    fn normalized_snapshot_maps_adapter_and_stick_scroll() {
        let mut snapshot = ControllerSnapshot::empty();
        snapshot.push(ogc_controller::ControllerState {
            source: ControllerSource::UsbGameCube {
                adapter_slot: 0,
                port: 2,
            },
            kind: ControllerKind::GameCube,
            connected: true,
            buttons: ControllerButtons::A,
            buttons_down: ControllerButtons::A,
            axes: ogc_controller::Axes {
                left_y: 8192,
                ..Default::default()
            },
            ..ogc_controller::ControllerState::empty()
        });
        let mut mapped = MappedInput::default();
        let mut mapper = DigitalStickMapper::<MAX_CONTROLLERS>::new();
        let (scroll, _) = map_additional_controllers(&snapshot, &mut mapped, &mut mapper).unwrap();
        assert_eq!(mapped.down.0, Buttons::CONFIRM);
        assert_eq!(mapped.held.0, Buttons::CONFIRM);
        assert_eq!(scroll, 22);
    }

    #[test]
    fn right_stick_actions_rearm_after_reconnect_and_do_not_duplicate_pad_direction() {
        let source = ControllerSource::BluetoothHid(0);
        let id = ogc_controller::ControllerId {
            source,
            generation: 1,
        };
        let mut mapper = DigitalStickMapper::<2>::new();
        let sample = |id, y| StickDirectionSample {
            id,
            x: 0,
            y,
            threshold: RIGHT_STICK_THRESHOLD,
        };
        assert!(
            mapper
                .map(&[sample(id, 0)])
                .unwrap()
                .states()
                .next()
                .unwrap()
                .action_down
                .bits()
                == 0
        );
        let pressed = mapper.map(&[sample(id, 9000)]).unwrap();
        let mut mapped = MappedInput::default();
        let navigation = map_stick_direction_frame(&pressed, &mut mapped);
        assert_eq!(navigation.0, Buttons::UP);
        assert_eq!(mapped.down.0, Buttons::UP);

        let mut pad_already_pressed = MappedInput {
            down: Buttons(Buttons::UP),
            ..MappedInput::default()
        };
        assert_eq!(
            map_stick_direction_frame(&pressed, &mut pad_already_pressed).0,
            0
        );

        let reset = mapper.map(&[]).unwrap();
        let mut released = MappedInput::default();
        map_stick_direction_frame(&reset, &mut released);
        assert_eq!(released.up.0, Buttons::UP);
        let replacement = ogc_controller::ControllerId {
            generation: 2,
            ..id
        };
        let held_after_reconnect = mapper.map(&[sample(replacement, 9000)]).unwrap();
        assert!(
            held_after_reconnect
                .states()
                .next()
                .unwrap()
                .action_down
                .bits()
                == 0
        );
        assert!(
            mapper
                .map(&[sample(replacement, 0)])
                .unwrap()
                .states()
                .next()
                .unwrap()
                .action_down
                .bits()
                == 0
        );
        let new_press = mapper.map(&[sample(replacement, 9000)]).unwrap();
        assert_eq!(
            new_press.states().next().unwrap().action_down.to_buttons(),
            ControllerButtons::UP
        );
    }

    #[test]
    fn right_stick_input_without_manager_identity_is_an_error() {
        let mut snapshot = ControllerSnapshot::empty();
        snapshot.push(ogc_controller::ControllerState {
            source: ControllerSource::BluetoothHid(0),
            kind: ControllerKind::JoyConRight,
            connected: true,
            axes: ogc_controller::Axes {
                right_y: 9000,
                ..Default::default()
            },
            ..ogc_controller::ControllerState::empty()
        });
        let mut mapper = DigitalStickMapper::<MAX_CONTROLLERS>::new();
        let error = map_additional_controllers(&snapshot, &mut MappedInput::default(), &mut mapper)
            .unwrap_err();
        assert_eq!(
            error,
            InputScanError::MissingControllerIdentity(ControllerSource::BluetoothHid(0))
        );
    }

    #[test]
    fn reset_edges_keep_quick_press_and_release_visible() {
        let state = ogc_controller::ControllerState {
            source: ControllerSource::BluetoothHid(0),
            kind: ControllerKind::SwitchPro,
            buttons_down: ControllerButtons::A,
            buttons_up: ControllerButtons::A,
            ..ogc_controller::ControllerState::empty()
        };
        let mut mapped = MappedInput::default();
        map_controller_edges(&state, &mut mapped);
        assert_eq!(mapped.down.0, Buttons::CONFIRM);
        assert_eq!(mapped.up.0, Buttons::CONFIRM);
    }

    #[test]
    fn production_mapper_keeps_pad_actions_when_wpad_is_neutral() {
        let mapped = map_input_masks(0x0100, 0, 0, [RawWpadTransitions::default(); 4], None, true);
        assert!(mapped.down.0 & Buttons::CONFIRM != 0);
        assert_eq!(mapped.held, Buttons::default());
    }

    #[test]
    fn production_mapper_preserves_classic_x_y_and_remote_minus_mappings() {
        let mut classic = RawWpadTransitions {
            extension: Some(Extension::Classic),
            down: (0x0008 << 16) | (0x0020 << 16),
            ..RawWpadTransitions::default()
        };
        let mut wpad = [RawWpadTransitions::default(); 4];
        wpad[0] = classic;
        let mapped = map_input_masks(0, 0, 0, wpad, None, true);
        assert!(mapped.down.0 & Buttons::OPTIONS != 0);
        assert!(mapped.down.0 & Buttons::BROWSE_MODE != 0);

        classic.extension = None;
        classic.down = 0x0010;
        wpad[0] = classic;
        let mapped = map_input_masks(0, 0, 0, wpad, None, true);
        assert!(mapped.down.0 & Buttons::PREVIOUS != 0);
        assert_eq!(mapped.down.0 & Buttons::OPTIONS, 0);
    }

    #[test]
    fn production_mapper_combines_wpad_channels_and_keeps_releases_on_disconnect() {
        let mut wpad = [RawWpadTransitions::default(); 4];
        wpad[0].down = 0x0008;
        wpad[1].down = 0x0004;
        wpad[1].up = 0x0001;
        let mapped = map_input_masks(0, 0, 0, wpad, None, true);
        assert!(mapped.down.0 & Buttons::CONFIRM != 0);
        assert!(mapped.down.0 & Buttons::BACK != 0);
        assert!(mapped.up.0 & Buttons::BROWSE_MODE != 0);

        let owner = map_input_masks(0, 0, 0, wpad, Some(0), true);
        assert!(owner.down.0 & Buttons::CONFIRM != 0);
        assert_eq!(owner.down.0 & Buttons::BACK, 0);
        assert!(owner.up.0 & Buttons::BROWSE_MODE != 0);
    }

    #[test]
    fn production_mapper_preserves_extension_screenshot_chords() {
        let mut wpad = [RawWpadTransitions::default(); 4];
        wpad[0] = RawWpadTransitions {
            extension: Some(Extension::Classic),
            down: 0x0080 << 16,
            held: 0x0004 << 16,
            up: 0,
        };
        let mapped = map_input_masks(0, 0, 0, wpad, None, true);
        assert!(screenshot_chord(mapped.down, mapped.held));
    }

    #[test]
    fn maps_hbc_screenshot_chord_buttons_for_nunchuk_and_classic() {
        assert_ne!(map_buttons(0x0010, 0).0 & Buttons::NETWORK_INIT, 0);
        for (mask, extension) in [
            (0x0001 << 16, Extension::Nunchuk),
            (0x0004 << 16, Extension::Classic),
        ] {
            let mut buttons = Buttons::default();
            map_screenshot_extension_buttons(mask, extension, &mut buttons);
            assert_ne!(buttons.0 & Buttons::NETWORK_INIT, 0);
        }
        for (mask, extension) in [
            (0x0002 << 16, Extension::Nunchuk),
            (0x0080 << 16, Extension::Classic),
        ] {
            let mut buttons = Buttons::default();
            map_screenshot_extension_buttons(mask, extension, &mut buttons);
            assert_ne!(buttons.0 & Buttons::SCREENSHOT, 0);
        }
        let mut buttons = Buttons::default();
        map_screenshot_extension_buttons(0x8000 << 16, Extension::Classic, &mut buttons);
        assert_eq!(buttons, Buttons::default());
    }

    #[test]
    fn screenshot_requires_a_new_screenshot_press_while_network_init_is_held() {
        assert!(screenshot_chord(
            Buttons(Buttons::SCREENSHOT),
            Buttons(Buttons::NETWORK_INIT)
        ));
        assert!(!screenshot_chord(
            Buttons(Buttons::SCREENSHOT),
            Buttons::default()
        ));
        assert!(!screenshot_chord(
            Buttons::default(),
            Buttons(Buttons::NETWORK_INIT)
        ));
    }

    #[test]
    fn pointer_owner_is_stable_until_ir_is_lost() {
        assert_eq!(
            select_pointer_owner(&[true, true, false, false], 1),
            Some(1)
        );
        assert_eq!(
            select_pointer_owner(&[true, false, false, false], 1),
            Some(0)
        );
        assert_eq!(select_pointer_owner(&[false, false, false, false], 1), None);
        assert_eq!(
            select_pointer_owner(&[true, false, false, false], -1),
            Some(0)
        );
    }

    #[test]
    fn ir_coordinates_remove_hbc_padding_and_reject_non_finite_values() {
        assert_eq!(
            logical_pointer_position(64.0, 64.0),
            Some(PointerPosition { x: 0, y: 0 })
        );
        assert_eq!(
            logical_pointer_position(384.0, 304.0),
            Some(PointerPosition { x: 320, y: 240 })
        );
        assert_eq!(logical_pointer_position(f32::NAN, 64.0), None);
        assert_eq!(
            logical_pointer_position(63.5, 64.0),
            Some(PointerPosition { x: 0, y: 0 })
        );
    }

    #[test]
    fn analog_scroll_matches_hbc_deadzone_and_scale() {
        assert_eq!(deadzone(10), 0);
        assert_eq!(deadzone(-10), 0);
        assert_eq!(deadzone(18) / 8, 1);
        assert_eq!(deadzone(-18) / 8, -1);
    }

    #[test]
    fn pointer_confirmation_uses_the_combined_pad_and_pointer_owner_buttons() {
        let mut pressed = Buttons::default();
        map_pad(0x0100, &mut pressed);
        map_wiimote(0x0008, &mut pressed);
        assert!(pointer_confirm_pressed(true, pressed));
        assert!(!pointer_confirm_pressed(false, pressed));
    }

    #[test]
    fn pointer_hold_requires_a_pointer_owner_and_confirm() {
        let held = Buttons(Buttons::CONFIRM);
        assert!(pointer_confirm_held(true, held));
        assert!(!pointer_confirm_held(false, held));
        assert!(!pointer_confirm_held(true, Buttons::default()));
    }

    #[test]
    fn separates_button_states_and_avoids_duplicate_actions() {
        let down = map_buttons(0x0100, 0);
        let held = map_buttons(0x0100, 0);
        assert!(down.contains(Input::Confirm));
        assert!(held.contains(Input::Confirm));
        assert_eq!(
            inputs(Buttons(down.0))
                .iter()
                .filter(|i| **i == Input::Confirm)
                .count(),
            1
        );
    }
}

/// Map a validated network sample with the same bindings as local controllers.
pub fn remote_frame(command: &crate::remote_input::Command) -> InputFrame {
    let mut buttons = Buttons::default();
    match command.kind {
        0 => buttons.0 = command.buttons as u16,
        1 => {
            let mut mask = command.buttons as u16;
            if command.triggers[0] >= 128 {
                mask |= 0x40;
            }
            if command.triggers[1] >= 128 {
                mask |= 0x20;
            }
            map_pad(mask, &mut buttons);
        }
        2 => map_wiimote(command.buttons, &mut buttons),
        3 => {
            let mut mask = command.buttons;
            if command.triggers[0] >= 128 {
                mask |= 0x2000 << 16;
            }
            if command.triggers[1] >= 128 {
                mask |= 0x0200 << 16;
            }
            map_wpad_buttons(Some(Extension::Classic), mask, &mut buttons);
        }
        4 => map_wpad_buttons(Some(Extension::Nunchuk), command.buttons, &mut buttons),
        5 => map_wpad_buttons(Some(Extension::GuitarHero3), command.buttons, &mut buttons),
        kind => {
            let kind = match kind {
                7 => ControllerKind::JoyConLeft,
                8 => ControllerKind::JoyConRight,
                9 => ControllerKind::JoyConPair,
                10 => ControllerKind::SwitchPro,
                11 => ControllerKind::XboxOne,
                _ => ControllerKind::GenericUsbHid,
            };
            let mut mask = command.buttons;
            if command.triggers[0] >= 128 {
                mask |= ControllerButtons::L.bits();
            }
            if command.triggers[1] >= 128 {
                mask |= ControllerButtons::R.bits();
            }
            map_normalized_controller_buttons(kind, mask, &mut buttons);
        }
    }
    // HBC scrolls with the left stick. It also gives the Right Joy-Con right
    // stick the same per-controller directional transitions as local input.
    let axes = ogc_controller::Axes {
        left_y: command.sticks[1],
        right_y: command.sticks[3],
        ..ogc_controller::Axes::default()
    };
    let kind = if command.kind == 8 {
        ControllerKind::JoyConRight
    } else {
        ControllerKind::GenericUsbHid
    };
    let analog_scroll = scroll_pixels(normalized_stick_scroll(kind, axes));
    let mut stick_navigation = Buttons::default();
    if command.kind == 8 {
        // The fixed threshold is inside the projection API's valid range.
        let directions =
            project_stick_directions(command.sticks[2], command.sticks[3], RIGHT_STICK_THRESHOLD)
                .expect("valid Right Joy-Con threshold");
        map_normalized_controller_buttons(
            ControllerKind::JoyConRight,
            directions.to_buttons().bits(),
            &mut stick_navigation,
        );
        buttons.0 |= stick_navigation.0;
    }
    InputFrame {
        held: inputs(buttons),
        pointer: command.pointer,
        stick_scroll_pixels: command.scroll.saturating_add(analog_scroll),
        stick_navigation: inputs(stick_navigation),
        screenshot_requested: command.screenshot
            || (buttons.0 & Buttons::SCREENSHOT != 0 && buttons.0 & Buttons::NETWORK_INIT != 0),
        ..InputFrame::default()
    }
}

#[cfg(test)]
mod remote_scroll_tests {
    use super::*;

    #[test]
    fn remote_and_local_right_sticks_match_at_threshold_edges() {
        let id = ogc_controller::ControllerId {
            source: ControllerSource::BluetoothHid(0),
            generation: 1,
        };
        for axis in [0, 1] {
            for (value, expected) in [
                (
                    -8193,
                    Some(if axis == 0 { Input::Left } else { Input::Down }),
                ),
                (-8192, None),
                (-8191, None),
                (8191, None),
                (8192, None),
                (8193, Some(if axis == 0 { Input::Right } else { Input::Up })),
            ] {
                let mut command = crate::remote_input::Command {
                    kind: 8,
                    buttons: 0,
                    duration_ms: 120,
                    pointer: None,
                    scroll: 0,
                    sticks: [0; 4],
                    triggers: [0; 2],
                    screenshot: false,
                };
                command.sticks[axis + 2] = value;
                let remote = remote_frame(&command);
                let expected: Vec<_> = expected.into_iter().collect();
                assert_eq!(
                    remote.stick_navigation, expected,
                    "axis {axis}, value {value}"
                );
                assert_eq!(remote.held, expected);
                let mut mapper = DigitalStickMapper::<1>::new();
                let mut sample = StickDirectionSample {
                    id,
                    x: 0,
                    y: 0,
                    threshold: RIGHT_STICK_THRESHOLD,
                };
                mapper.map(&[sample]).unwrap();
                if axis == 0 {
                    sample.x = value;
                } else {
                    sample.y = value;
                }
                let frame = mapper.map(&[sample]).unwrap();
                let mut mapped = MappedInput::default();
                let navigation = map_stick_direction_frame(&frame, &mut mapped);
                assert_eq!(inputs(navigation), expected);
                assert_eq!(inputs(mapped.held), expected);
            }
        }
    }

    #[test]
    fn remote_scroll_uses_local_axis_deadzone_and_scale_for_every_kind() {
        for kind in 0..=11 {
            let mut command = crate::remote_input::Command {
                kind,
                buttons: 0,
                duration_ms: 120,
                pointer: None,
                scroll: 0,
                sticks: [0, 16384, 0, -16384],
                triggers: [0; 2],
                screenshot: false,
            };
            assert_eq!(
                remote_frame(&command).stick_scroll_pixels,
                if kind == 8 { -6 } else { 6 },
                "kind {kind}"
            );
            command.scroll = 20;
            assert_eq!(
                remote_frame(&command).stick_scroll_pixels,
                if kind == 8 { 14 } else { 26 }
            );
            for (axis, expected) in [
                (2560, 0),
                (-2560, 0),
                (4608, 1),
                (-4608, -1),
                (32767, 14),
                (-32767, -14),
            ] {
                command.scroll = 0;
                command.sticks = [0, axis, 0, axis];
                assert_eq!(remote_frame(&command).stick_scroll_pixels, expected);
            }
        }
    }
}
