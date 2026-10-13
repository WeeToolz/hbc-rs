//! GameCube PAD and Wii Remote input adapter.
//!
//! Inputs retain their press, hold, and release state. Callers can use press
//! events for one-shot actions and hold events for repeatable navigation.

use alloc::vec::Vec;
#[cfg(target_arch = "powerpc")]
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};

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
    #[cfg(any(test, target_arch = "powerpc"))]
    const NETWORK_INIT: u16 = 1 << 12;
    #[cfg(any(test, target_arch = "powerpc"))]
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

#[cfg(any(target_arch = "powerpc", test))]
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

#[cfg(any(target_arch = "powerpc", test))]
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
    unsafe { (&mut *core::ptr::addr_of_mut!(CONTROLLER_MANAGER)).init() };
    POINTER_OWNER.store(-1, Ordering::Release);
    WPAD_VIEW_WIDTH.store(u32::from(view_width), Ordering::Release);
    WPAD_VIEW_HEIGHT.store(u32::from(view_height), Ordering::Release);
    WPAD_STARTED.store(false, Ordering::Release);

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
pub fn scan() -> Result<InputFrame, i32> {
    use wpad_sys as wpad;

    diagnostic_scan_mark("WPAD SCAN START");
    if !WPAD_STARTED.load(Ordering::Acquire) {
        // SAFETY: HBC scans from its main thread only.
        let start = unsafe { (&mut *core::ptr::addr_of_mut!(CONTROLLER_MANAGER)).enable_wpad() };
        if start == wpad::WPAD_ERR_NONE as i32 {
            configure_wpad(
                WPAD_VIEW_WIDTH.load(Ordering::Acquire),
                WPAD_VIEW_HEIGHT.load(Ordering::Acquire),
            )?;
        } else if start != wpad::WPAD_ERR_NOT_READY as i32 {
            return Err(start);
        }
    }
    diagnostic_scan_mark("PAD AND WPAD SCAN START");
    // SAFETY: one main-thread scan owns the shared production manager.
    let raw = unsafe { (&mut *core::ptr::addr_of_mut!(CONTROLLER_MANAGER)).scan_with_raw() };
    diagnostic_scan_mark("WPAD SCAN DONE");

    if let Some(error) = raw
        .snapshot
        .errors()
        .find(|error| error.operation == "WPAD poll")
    {
        if let ogc_controller::InputFailure::Source(code) = error.failure {
            return Err(code);
        }
    }

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
    stick_scroll /= 8;
    let mut wpad_transitions = [RawWpadTransitions::default(); 4];
    for (channel, transitions) in wpad_transitions.iter_mut().enumerate() {
        transitions.extension = extension_from_raw_type(raw.wpad_extension_type[channel]);
        transitions.down = raw.wpad_buttons_down[channel];
        transitions.held = raw.wpad_buttons_held[channel];
        transitions.up = raw.wpad_buttons_up[channel];
    }
    let mapped = map_input_masks(
        raw.pad_buttons_down[0],
        raw.pad_buttons_held[0],
        raw.pad_buttons_up[0],
        wpad_transitions,
        owner,
        raw.pad_status[0].err == ogc_input::PAD_ERR_NONE as i8,
    );
    if let Some(owner) = owner {
        if let Some(state) = raw.wpad_data[owner]
            .as_ref()
            .filter(|state| state.err == wpad::WPAD_ERR_NONE && state.data_present != 0)
        {
            pointer = logical_pointer_position(state.ir.x, state.ir.y);
            pointer_pressed = pointer_confirm_pressed(true, mapped.down);
        }
    }
    let pointer_held = pointer_confirm_held(owner.is_some(), mapped.held);
    Ok(InputFrame {
        pressed: inputs(mapped.down),
        held: inputs(mapped.held),
        released: inputs(mapped.up),
        pointer,
        pointer_pressed,
        pointer_held,
        stick_scroll_pixels: stick_scroll.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
        screenshot_requested: screenshot_chord(mapped.down, mapped.held),
    })
}

#[cfg(target_arch = "powerpc")]
#[inline(always)]
fn diagnostic_scan_mark(label: &'static str) {
    #[cfg(feature = "startup-diagnostics")]
    super::diagnostics::mark(label);
    #[cfg(not(feature = "startup-diagnostics"))]
    let _ = label;
}

#[cfg(any(test, target_arch = "powerpc"))]
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

#[cfg(any(target_arch = "powerpc", test))]
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
