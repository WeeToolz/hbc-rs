//! GameCube PAD and Wii Remote input adapter.
//!
//! Inputs retain their press, hold, and release state. Callers can use press
//! events for one-shot actions and hold events for repeatable navigation.

use alloc::vec::Vec;

use crate::app::Input;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InputFrame {
    pub pressed: Vec<Input>,
    pub held: Vec<Input>,
    pub released: Vec<Input>,
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
    const FILTER: u16 = 1 << 7;
    const SORT: u16 = 1 << 8;
    const PREVIOUS: u16 = 1 << 9;
    const NEXT: u16 = 1 << 10;
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
    Classic,
    GuitarHero3,
}

/// Convert supported extension buttons to the matching Remote buttons.
/// Nunchuk buttons do not map to HBC actions in the original controls layer.
pub fn map_extension_buttons(mask: u32, extension: Extension) -> u32 {
    let mut result = mask;
    match extension {
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
        out.0 |= Buttons::SORT;
    }
    if mask & 0x0800 != 0 {
        out.0 |= Buttons::FILTER;
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
        out.0 |= Buttons::SORT;
    }
    if mask & 0x0001 != 0 {
        out.0 |= Buttons::FILTER;
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
        Input::CycleFilter => Buttons::FILTER,
        Input::CycleSort => Buttons::SORT,
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
        Input::CycleFilter,
        Input::CycleSort,
        Input::PreviousPage,
        Input::NextPage,
    ]
    .into_iter()
    .filter(|input| mask.0 & bit_for(*input) != 0)
    .collect()
}

#[cfg(target_arch = "powerpc")]
pub fn initialize() {
    ogc_input::init();
    wpad_sys::init();
    wpad_sys::set_idle_timeout(120);
}

/// Scan all four GameCube ports and Wii Remote channels, combining buttons.
#[cfg(target_arch = "powerpc")]
pub fn scan() -> InputFrame {
    use ogc_input as pad;
    use wpad_sys as wpad;

    pad::scan_pads();
    let mut down = Buttons::default();
    let mut held = Buttons::default();
    let mut up = Buttons::default();
    for channel in 0..4 {
        let channel = channel as i32;
        let pressed_pad = pad::buttons_down(channel);
        let held_pad = pad::buttons_held(channel);
        let released_pad = pad::buttons_up(channel);
        map_pad(pressed_pad, &mut down);
        map_pad(held_pad, &mut held);
        map_pad(released_pad, &mut up);
    }
    wpad::scan_pads();
    for channel in 0..4 {
        let channel = channel as i32;
        map_wpad_channel(channel, wpad::buttons_down(channel), &mut down);
        map_wpad_channel(channel, wpad::buttons_held(channel), &mut held);
        map_wpad_channel(channel, wpad::buttons_up(channel), &mut up);
    }
    InputFrame {
        pressed: inputs(down),
        held: inputs(held),
        released: inputs(up),
    }
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
        let pad = map_buttons(0x0100 | 0x0008 | 0x0020, 0);
        assert!(pad.contains(Input::Confirm));
        assert!(pad.contains(Input::Up));
        assert!(pad.contains(Input::NextPage));
        let remote = map_buttons(0, 0x0008 | 0x0800 | 0x1000);
        assert!(remote.contains(Input::Confirm));
        assert!(remote.contains(Input::Up));
        assert!(remote.contains(Input::NextPage));
        let classic_mask =
            map_extension_buttons((0x0010 << 16) | (0x8000 << 16), Extension::Classic);
        let classic = map_buttons(0, classic_mask);
        assert!(classic.contains(Input::Confirm));
        assert!(classic.contains(Input::Right));
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
