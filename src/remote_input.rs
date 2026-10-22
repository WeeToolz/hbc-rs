//! Bounded network input commands and main-thread input transitions.
use crate::{
    app::Input,
    platform::input::{InputFrame, PointerPosition},
};
use alloc::vec::Vec;
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, AtomicU8, Ordering},
};

pub const PACKET_LEN: usize = 32;
pub const MAX_HOLD_MS: u16 = 2000;
pub const APPLIED: u8 = 0;
pub const INVALID: u8 = 1;
pub const BUSY: u8 = 2;
pub const EXPIRED: u8 = 3;
pub const PENDING: u8 = 4;
pub const STOPPED: u8 = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Command {
    pub kind: u8,
    pub buttons: u32,
    pub duration_ms: u16,
    pub pointer: Option<PointerPosition>,
    pub scroll: i16,
    pub sticks: [i16; 4],
    pub triggers: [u8; 2],
    pub screenshot: bool,
}
impl Command {
    pub fn decode(bytes: &[u8]) -> Result<Self, u8> {
        if bytes.len() != PACKET_LEN
            || &bytes[..4] != b"HBCI"
            || bytes[4] != 1
            || bytes[5] > 11
            || bytes[6] != 0
            || bytes[7] & !3 != 0
            || bytes[30..32] != [0, 0]
        {
            return Err(INVALID);
        }
        let word = |offset| i16::from_be_bytes([bytes[offset], bytes[offset + 1]]);
        let buttons = u32::from_be_bytes(bytes[8..12].try_into().map_err(|_| INVALID)?);
        let valid_buttons = match bytes[5] {
            0 => 0x0eff, // All eleven HBC logical actions; bit 8 is reserved.
            1 => 0x1f7f,
            2 => 0x1f9f,
            3 => 0xffff_1f9f,
            4 => 0x0003_1f9f,
            5 => 0xffff_1f9f,
            _ => 0x001f_ffff,
        };
        let duration_ms = u16::from_be_bytes([bytes[12], bytes[13]]);
        let x = word(14);
        let y = word(16);
        if buttons & !valid_buttons != 0
            || duration_ms > MAX_HOLD_MS
            || (bytes[7] & 1 != 0 && (!(0..=4095).contains(&x) || !(0..=4095).contains(&y)))
            || word(18).unsigned_abs() > 512
            || [word(20), word(22), word(24), word(26)].contains(&i16::MIN)
            || (bytes[7] & 1 == 0 && (x != 0 || y != 0))
            || (matches!(bytes[5], 0 | 2 | 4 | 5) && bytes[28..30] != [0, 0])
            || (duration_ms == 0
                && bytes[7..12]
                    .iter()
                    .chain(bytes[14..30].iter())
                    .any(|b| *b != 0))
        {
            return Err(INVALID);
        }
        Ok(Self {
            kind: bytes[5],
            buttons,
            duration_ms,
            pointer: (bytes[7] & 1 != 0).then_some(PointerPosition { x, y }),
            scroll: word(18),
            sticks: [word(20), word(22), word(24), word(26)],
            triggers: [bytes[28], bytes[29]],
            screenshot: bytes[7] & 2 != 0,
        })
    }
}

/// A single pending command. Main-thread acquisition never waits for the worker.
pub struct Mailbox {
    locked: AtomicBool,
    slot: UnsafeCell<Option<(Command, u64)>>,
    completion: AtomicU8,
}
// All accesses to slot hold locked; completion has release/acquire ordering.
unsafe impl Sync for Mailbox {}
impl Default for Mailbox {
    fn default() -> Self {
        Self::new()
    }
}
impl Mailbox {
    pub const fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
            slot: UnsafeCell::new(None),
            completion: AtomicU8::new(STOPPED),
        }
    }
    fn access<R>(&self, operation: impl FnOnce(&mut Option<(Command, u64)>) -> R) -> Option<R> {
        crate::wiiload::try_with_payload(&self.locked, || {
            // SAFETY: the guard gives exclusive access, including on unwind.
            operation(unsafe { &mut *self.slot.get() })
        })
    }
    pub fn submit(&self, command: Command, now_ms: u64) -> Result<(), u8> {
        self.access(|slot| {
            if slot.is_some() {
                return Err(BUSY);
            }
            self.completion.store(PENDING, Ordering::Release);
            *slot = Some((command, now_ms));
            Ok(())
        })
        .unwrap_or(Err(BUSY))
    }
    pub fn completion(&self) -> u8 {
        self.completion.load(Ordering::Acquire)
    }
    pub fn consume(&self, state: &mut RemoteState, now_ms: u64, frame: &mut InputFrame) {
        let consumed = self.access(|slot| {
            let mut outcome = None;
            if let Some((command, received_at)) = slot.take() {
                if command.duration_ms != 0
                    && now_ms.saturating_sub(received_at) >= u64::from(command.duration_ms)
                {
                    outcome = Some(EXPIRED);
                } else {
                    state.active = (command.duration_ms != 0).then_some((command, received_at));
                    outcome = Some(APPLIED);
                }
            }
            state.merge(now_ms, frame);
            if let Some(outcome) = outcome {
                self.completion.store(outcome, Ordering::Release);
            }
        });
        if consumed.is_none() {
            state.merge(now_ms, frame);
        }
    }
}

#[derive(Default)]
pub struct RemoteState {
    active: Option<(Command, u64)>,
    held: Vec<Input>,
    screenshot_held: bool,
    pointer_confirm_held: bool,
}
impl RemoteState {
    fn merge(&mut self, now_ms: u64, frame: &mut InputFrame) {
        if self.active.is_some_and(|(command, start)| {
            now_ms.saturating_sub(start) >= u64::from(command.duration_ms)
        }) {
            self.active = None;
        }
        let remote = self
            .active
            .map_or_else(InputFrame::default, |(command, _)| {
                crate::platform::input::remote_frame(&command)
            });
        for input in &remote.held {
            if !self.held.contains(input)
                && !frame.held.contains(input)
                && !frame.pressed.contains(input)
            {
                frame.pressed.push(*input);
            }
        }
        for input in &self.held {
            if !remote.held.contains(input)
                && !frame.held.contains(input)
                && !frame.released.contains(input)
            {
                frame.released.push(*input);
            }
        }
        frame
            .pressed
            .retain(|input| !self.held.contains(input) || !remote.held.contains(input));
        frame.released.retain(|input| !remote.held.contains(input));
        if let Some(pointer) = remote.pointer {
            frame.pointer = Some(pointer);
            let remote_pointer_down = remote.held.contains(&Input::Confirm);
            frame.pointer_pressed |= remote_pointer_down && !self.pointer_confirm_held;
            frame.pointer_held |= remote_pointer_down;
            self.pointer_confirm_held = remote_pointer_down;
        } else {
            self.pointer_confirm_held = false;
        }
        frame.screenshot_requested |= remote.screenshot_requested && !self.screenshot_held;
        frame.stick_scroll_pixels = frame
            .stick_scroll_pixels
            .saturating_add(remote.stick_scroll_pixels);
        for input in remote.stick_navigation {
            if !frame.stick_navigation.contains(&input) {
                frame.stick_navigation.push(input);
            }
        }
        self.screenshot_held = remote.screenshot_requested;
        self.held = remote.held;
        for input in &self.held {
            if !frame.held.contains(input) {
                frame.held.push(*input);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn packet() -> [u8; 32] {
        // Wii Remote HOME held for 120 ms. Independent protocol fixture.
        [
            0x48, 0x42, 0x43, 0x49, 1, 2, 0, 0, 0, 0, 0, 0x80, 0, 0x78, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0,
        ]
    }
    fn frame(mailbox: &Mailbox, state: &mut RemoteState, now: u64) -> InputFrame {
        let mut frame = InputFrame::default();
        mailbox.consume(state, now, &mut frame);
        frame
    }
    #[test]
    fn wire_home_changes_real_runtime_menu_then_releases_at_deadline() {
        let mailbox = Mailbox::new();
        let mut state = RemoteState::default();
        let command = Command::decode(&packet()).unwrap();
        assert_eq!(command.buttons, 128);
        assert_eq!(command.duration_ms, 120);
        mailbox.submit(command, 100).unwrap();
        assert_eq!(mailbox.completion(), PENDING);
        let first = frame(&mailbox, &mut state, 100);
        assert_eq!(first.pressed, [Input::Home]);
        assert_eq!(first.held, [Input::Home]);
        assert_eq!(mailbox.completion(), APPLIED);
        let mut runtime = crate::runtime::AppRuntime::default();
        runtime.update(&first);
        assert_eq!(runtime.controller().screen, crate::app::Screen::MainMenu);
        let next = frame(&mailbox, &mut state, 219);
        assert!(next.pressed.is_empty());
        assert_eq!(next.held, [Input::Home]);
        let released = frame(&mailbox, &mut state, 220);
        assert_eq!(released.released, [Input::Home]);
        assert!(released.held.is_empty());
        assert!(frame(&mailbox, &mut state, 221).released.is_empty());
    }
    #[test]
    fn rejects_invalid_packets_without_changing_pending_input() {
        let good = packet();
        assert_eq!(Command::decode(&good[..31]), Err(INVALID));
        for (offset, value) in [
            (0, 0),
            (4, 2),
            (5, 12),
            (6, 1),
            (7, 4),
            (8, 128),
            (30, 1),
            (31, 1),
        ] {
            let mut bad = good;
            bad[offset] = value;
            assert_eq!(Command::decode(&bad), Err(INVALID), "offset {offset}");
        }
        let mut too_long = good;
        too_long[12..14].copy_from_slice(&2001u16.to_be_bytes());
        assert_eq!(Command::decode(&too_long), Err(INVALID));
        let mut release_with_buttons = good;
        release_with_buttons[13] = 0;
        assert_eq!(Command::decode(&release_with_buttons), Err(INVALID));
        let mut invalid_axis = good;
        invalid_axis[20] = 128;
        assert_eq!(Command::decode(&invalid_axis), Err(INVALID));
    }
    #[test]
    fn busy_and_expired_commands_do_not_claim_application_or_press_buttons() {
        let mailbox = Mailbox::new();
        let mut state = RemoteState::default();
        let command = Command::decode(&packet()).unwrap();
        mailbox.submit(command, 0).unwrap();
        assert_eq!(mailbox.submit(command, 1), Err(BUSY));
        assert!(frame(&mailbox, &mut state, 120).pressed.is_empty());
        assert_eq!(mailbox.completion(), EXPIRED);
        // Simulate a producer holding the mailbox; the main thread must not spin.
        mailbox.locked.store(true, Ordering::Release);
        assert!(frame(&mailbox, &mut state, 121).held.is_empty());
        assert_eq!(mailbox.submit(command, 121), Err(BUSY));
        mailbox.locked.store(false, Ordering::Release);
        mailbox.submit(command, 122).unwrap();
        assert_eq!(frame(&mailbox, &mut state, 123).pressed, [Input::Home]);
    }
    #[test]
    fn release_command_preserves_local_held_buttons_and_pointer() {
        let mailbox = Mailbox::new();
        let mut state = RemoteState::default();
        mailbox
            .submit(Command::decode(&packet()).unwrap(), 0)
            .unwrap();
        frame(&mailbox, &mut state, 0);
        let mut release = packet();
        release[11] = 0;
        release[13] = 0;
        mailbox
            .submit(Command::decode(&release).unwrap(), 10)
            .unwrap();
        let mut local = InputFrame {
            held: alloc::vec![Input::Home],
            pointer: Some(PointerPosition { x: 50, y: 60 }),
            ..InputFrame::default()
        };
        mailbox.consume(&mut state, 10, &mut local);
        assert!(local.released.is_empty());
        assert_eq!(local.held, [Input::Home]);
        assert_eq!(local.pointer, Some(PointerPosition { x: 50, y: 60 }));
        assert_eq!(mailbox.completion(), APPLIED);
    }
    #[test]
    fn controller_families_use_real_hbc_button_mappings() {
        for (kind, mask, expected) in [
            (0, 0x10, Input::Confirm),
            (1, 0x100, Input::Confirm),
            (2, 8, Input::Confirm),
            (3, 0x0010_0000, Input::Confirm),
            (4, 8, Input::Confirm),
            (5, 8, Input::Confirm),
            (6, 1, Input::Confirm),
            (7, 1 << 18, Input::Confirm),
            (8, 1, Input::Confirm),
            (9, 1 << 10, Input::Home),
            (10, 1 << 10, Input::Home),
            (11, 1 << 8, Input::Home),
        ] {
            let mut bytes = packet();
            bytes[5] = kind;
            bytes[8..12].copy_from_slice(&u32::to_be_bytes(mask));
            let command = Command::decode(&bytes).unwrap();
            assert!(
                crate::platform::input::remote_frame(&command)
                    .held
                    .contains(&expected),
                "kind {kind}"
            );
        }
    }
    #[test]
    fn pointer_sticks_scroll_triggers_and_screenshot_reach_input_frame() {
        let mut bytes = packet();
        bytes[5] = 8;
        bytes[7] = 3;
        bytes[8..12].copy_from_slice(&((1 | (1 << 8) | (1 << 12)) as u32).to_be_bytes());
        bytes[14..18].copy_from_slice(&[1, 0x40, 0, 0xf0]); // 320,240
        bytes[18..20].copy_from_slice(&(-5i16).to_be_bytes());
        bytes[20..22].copy_from_slice(&20000i16.to_be_bytes());
        bytes[24..26].copy_from_slice(&(-20000i16).to_be_bytes());
        bytes[26..28].copy_from_slice(&16384i16.to_be_bytes());
        bytes[28] = 255;
        bytes[29] = 128;
        let mailbox = Mailbox::new();
        let mut state = RemoteState::default();
        mailbox.submit(Command::decode(&bytes).unwrap(), 0).unwrap();
        let first = frame(&mailbox, &mut state, 0);
        assert_eq!(first.pointer, Some(PointerPosition { x: 320, y: 240 }));
        assert!(first.pointer_pressed && first.pointer_held && first.screenshot_requested);
        assert_eq!(first.stick_scroll_pixels, 49);
        assert!(first.stick_navigation.contains(&Input::Left));
        assert!(first.stick_navigation.contains(&Input::Up));
        for input in [
            Input::Confirm,
            Input::Left,
            Input::Up,
            Input::PreviousPage,
            Input::NextPage,
        ] {
            assert!(first.pressed.contains(&input));
        }
        let held = frame(&mailbox, &mut state, 20);
        assert!(!held.pointer_pressed && !held.screenshot_requested);
        assert!(held.pointer_held);
        let released = frame(&mailbox, &mut state, 120);
        assert!(!released.pointer_held && !released.screenshot_requested);
        assert_eq!(released.stick_scroll_pixels, 0);
    }
    #[test]
    fn pointer_press_has_its_own_edge_and_merges_local_pointer_state() {
        let mailbox = Mailbox::new();
        let mut state = RemoteState::default();
        let mut logical_confirm = packet();
        logical_confirm[5] = 0;
        logical_confirm[8..12].copy_from_slice(&16u32.to_be_bytes());
        mailbox
            .submit(Command::decode(&logical_confirm).unwrap(), 0)
            .unwrap();
        frame(&mailbox, &mut state, 0);
        let mut pointer_confirm = logical_confirm;
        pointer_confirm[7] = 1;
        pointer_confirm[14..18].copy_from_slice(&[0, 20, 0, 30]);
        mailbox
            .submit(Command::decode(&pointer_confirm).unwrap(), 20)
            .unwrap();
        let mut local = InputFrame {
            pointer_pressed: true,
            pointer_held: true,
            pointer: Some(PointerPosition { x: 5, y: 6 }),
            ..InputFrame::default()
        };
        mailbox.consume(&mut state, 20, &mut local);
        assert!(local.pointer_pressed);
        assert!(local.pointer_held);
        assert_eq!(local.pointer, Some(PointerPosition { x: 20, y: 30 }));
        assert!(local.pressed.is_empty());
    }
    #[test]
    fn remote_hold_suppresses_duplicate_local_edges_until_release() {
        let mailbox = Mailbox::new();
        let mut state = RemoteState::default();
        mailbox
            .submit(Command::decode(&packet()).unwrap(), 0)
            .unwrap();
        frame(&mailbox, &mut state, 0);
        let mut local = InputFrame {
            pressed: alloc::vec![Input::Home],
            held: alloc::vec![Input::Home],
            ..InputFrame::default()
        };
        mailbox.consume(&mut state, 10, &mut local);
        assert!(local.pressed.is_empty());
        assert_eq!(local.held, [Input::Home]);
        let mut local_release = InputFrame {
            released: alloc::vec![Input::Home],
            ..InputFrame::default()
        };
        mailbox.consume(&mut state, 20, &mut local_release);
        assert!(local_release.released.is_empty());
        assert_eq!(local_release.held, [Input::Home]);
        let mut release = packet();
        release[11] = 0;
        release[13] = 0;
        mailbox
            .submit(Command::decode(&release).unwrap(), 21)
            .unwrap();
        assert_eq!(frame(&mailbox, &mut state, 21).released, [Input::Home]);
    }
    #[test]
    fn gamecube_and_classic_analog_triggers_use_local_page_bindings() {
        for kind in [1, 3] {
            for (triggers, expected) in [
                ([127, 127], alloc::vec![]),
                ([128, 0], alloc::vec![Input::PreviousPage]),
                ([0, 255], alloc::vec![Input::NextPage]),
            ] {
                let mut bytes = packet();
                bytes[5] = kind;
                bytes[8..12].fill(0);
                bytes[28..30].copy_from_slice(&triggers);
                let command = Command::decode(&bytes).unwrap();
                assert_eq!(
                    crate::platform::input::remote_frame(&command).held,
                    expected
                );
            }
        }
        for kind in [0, 2, 4, 5] {
            let mut bytes = packet();
            bytes[5] = kind;
            bytes[28] = 1;
            assert_eq!(Command::decode(&bytes), Err(INVALID));
        }
    }
    #[test]
    fn every_logical_button_reaches_the_shared_input_path() {
        for (bit, input) in [
            (0, Input::Up),
            (1, Input::Down),
            (2, Input::Left),
            (3, Input::Right),
            (4, Input::Confirm),
            (5, Input::Back),
            (6, Input::Home),
            (7, Input::ToggleBrowseMode),
            (9, Input::PreviousPage),
            (10, Input::NextPage),
            (11, Input::OpenOptions),
        ] {
            let mut bytes = packet();
            bytes[5] = 0;
            bytes[8..12].copy_from_slice(&(1u32 << bit).to_be_bytes());
            let mailbox = Mailbox::new();
            let mut state = RemoteState::default();
            mailbox.submit(Command::decode(&bytes).unwrap(), 0).unwrap();
            assert_eq!(frame(&mailbox, &mut state, 0).pressed, [input]);
        }
    }
}
