//! GameCube PAD and Wii Remote input adapter.
//!
//! Inputs retain their press, hold, and release state. Callers can use press
//! events for one-shot actions and hold events for repeatable navigation.

use alloc::vec::Vec;
#[cfg(target_arch = "powerpc")]
use core::sync::atomic::{AtomicI32, Ordering};

use crate::app::Input;

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

/// Convert supported extension buttons to the matching Remote buttons.
/// Nunchuk buttons do not map to HBC actions in the original controls layer.
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

#[cfg(target_arch = "powerpc")]
pub fn initialize(view_width: u16, view_height: u16) {
    ogc_input::init();
    wpad_sys::init();
    wpad_sys::set_idle_timeout(120);
    let ir_width = u32::from(view_width).saturating_add(128);
    let ir_height = u32::from(view_height).saturating_add(128);
    for channel in 0..4 {
        wpad_sys::set_data_format(channel, wpad_sys::WPAD_FMT_BTNS_ACC_IR);
        wpad_sys::set_v_res(channel, ir_width, ir_height);
    }
    POINTER_OWNER.store(-1, Ordering::Release);
}

/// Scan GameCube port zero and Wii Remote channels using HBC pointer ownership.
#[cfg(target_arch = "powerpc")]
pub fn scan() -> InputFrame {
    use ogc_input as pad;
    use wpad_sys as wpad;

    pad::scan_pads();
    let mut down = Buttons::default();
    let mut held = Buttons::default();
    let mut up = Buttons::default();
    wpad::scan_pads();
    // HBC reads only PAD channel zero. Other controller buttons are accepted
    // only when no Wii Remote owns the IR pointer; then they are combined.
    let pressed_pad = pad::buttons_down(0);
    let held_pad = pad::buttons_held(0);
    let released_pad = pad::buttons_up(0);
    map_pad(pressed_pad, &mut down);
    map_pad(held_pad, &mut held);
    map_pad(released_pad, &mut up);

    let mut data = [core::ptr::null_mut(); 4];
    let mut ir_valid = [false; 4];
    for channel in 0..4 {
        let channel = channel as i32;
        let mut expansion = 0;
        if unsafe { wpad::probe(channel, &mut expansion) } != wpad::WPAD_ERR_NONE as i32 {
            continue;
        }
        let state = wpad::data(channel);
        if state.is_null() {
            continue;
        }
        data[channel as usize] = state;
        ir_valid[channel as usize] = unsafe { (*state).ir.valid != 0 };
    }
    let owner = select_pointer_owner(&ir_valid, POINTER_OWNER.load(Ordering::Acquire));
    POINTER_OWNER.store(
        owner.map_or(-1, |channel| channel as i32),
        Ordering::Release,
    );

    let mut pointer = None;
    let mut pointer_pressed = false;
    let mut stick_scroll = deadzone(i32::from(pad::stick_y(0)));
    if let Some(owner) = owner {
        if !data[owner].is_null() {
            stick_scroll += extension_stick_y(unsafe { &*data[owner] });
        }
    } else {
        for state in data.into_iter().filter(|state| !state.is_null()) {
            stick_scroll += extension_stick_y(unsafe { &*state });
        }
    }
    stick_scroll /= 8;
    for channel in 0..4 {
        if data[channel].is_null() || owner.is_some_and(|owner| owner != channel) {
            continue;
        }
        let channel_id = channel as i32;
        let pressed = wpad::buttons_down(channel_id);
        map_wpad_channel(channel_id, pressed, &mut down);
        if owner == Some(channel) {
            let state = unsafe { &*data[channel] };
            pointer = logical_pointer_position(state.ir.x, state.ir.y);
            pointer_pressed = pointer_confirm_pressed(owner.is_some(), down);
        }
        map_wpad_channel(channel_id, wpad::buttons_held(channel_id), &mut held);
        map_wpad_channel(channel_id, wpad::buttons_up(channel_id), &mut up);
    }
    let pointer_held = pointer_confirm_held(owner.is_some(), held);
    InputFrame {
        pressed: inputs(down),
        held: inputs(held),
        released: inputs(up),
        pointer,
        pointer_pressed,
        pointer_held,
        stick_scroll_pixels: stick_scroll.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
    }
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

#[cfg(target_arch = "powerpc")]
fn map_wpad_channel(channel: i32, buttons: u32, out: &mut Buttons) {
    let data = wpad_sys::data(channel);
    if data.is_null() {
        map_wiimote(buttons, out);
        return;
    }
    let expansion = unsafe { (*data).exp.type_ };
    let transformed = match expansion {
        value if value == wpad_sys::WPAD_EXP_CLASSIC as i32 => {
            map_extension_buttons(buttons, Extension::Classic)
        }
        value if value == wpad_sys::WPAD_EXP_GUITARHERO3 as i32 => {
            map_extension_buttons(buttons, Extension::GuitarHero3)
        }
        _ => buttons,
    };
    // HBC promotes extension buttons into the base Remote mask before use.
    map_wiimote(transformed, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_gamecube_and_wiimote_buttons_to_actions() {
        let pad = map_buttons(0x0100 | 0x0008 | 0x0020 | 0x0400 | 0x0800, 0);
        assert!(pad.contains(Input::Confirm));
        assert!(pad.contains(Input::OpenOptions));
        assert!(pad.contains(Input::ToggleBrowseMode));
        assert!(pad.contains(Input::Up));
        assert!(pad.contains(Input::NextPage));
        let remote = map_buttons(0, 0x0001 | 0x0002 | 0x0008 | 0x0800 | 0x1000);
        assert!(remote.contains(Input::Confirm));
        assert!(remote.contains(Input::OpenOptions));
        assert!(remote.contains(Input::ToggleBrowseMode));
        assert!(remote.contains(Input::Up));
        assert!(remote.contains(Input::NextPage));
        let classic_mask =
            map_extension_buttons((0x0010 << 16) | (0x8000 << 16), Extension::Classic);
        let classic = map_buttons(0, classic_mask);
        assert!(classic.contains(Input::Confirm));
        assert!(classic.contains(Input::Right));
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
