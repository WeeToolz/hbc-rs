//! Reborn HBCK bounded key queue.

pub const CAPACITY: usize = 64;

/// Map the shared HOME controls to HBC input actions.
pub fn input_for_key(key: u8) -> Option<crate::app::Input> {
    use crate::app::Input;
    match key {
        b'u' => Some(Input::Up),
        b'd' => Some(Input::Down),
        b'l' => Some(Input::Left),
        b'r' => Some(Input::Right),
        b'a' => Some(Input::Confirm),
        b'b' => Some(Input::Back),
        b'h' => Some(Input::Home),
        _ => None,
    }
}

/// Return the payload length for a valid Reborn key header.
pub fn body_length(header: &[u8; 16]) -> Option<usize> {
    if &header[..4] != b"HBCK" {
        return None;
    }
    let len = usize::from(u16::from_be_bytes([header[4], header[5]]));
    (len <= CAPACITY).then_some(len)
}

/// The caller serializes access from the network and UI threads.
pub struct KeyQueue {
    keys: [u8; CAPACITY],
    head: usize,
    len: usize,
}

impl Default for KeyQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyQueue {
    pub const fn new() -> Self {
        Self {
            keys: [0; CAPACITY],
            head: 0,
            len: 0,
        }
    }

    pub fn push(&mut self, keys: &[u8]) {
        for &key in keys {
            if self.len == CAPACITY {
                break;
            }
            // C's strchr also accepts NUL as the string terminator.
            if key == 0 || b"udlrabh12w".contains(&key) {
                self.keys[(self.head + self.len) % CAPACITY] = key;
                self.len += 1;
            }
        }
    }

    pub fn pop(&mut self) -> Option<u8> {
        if self.len == 0 {
            return None;
        }
        let key = self.keys[self.head];
        self.head = (self.head + 1) % CAPACITY;
        self.len -= 1;
        Some(key)
    }

    pub fn take_home(&mut self) -> bool {
        while let Some(key) = self.pop() {
            if key == b'h' {
                return true;
            }
        }
        false
    }
}

/// Main-thread pacing state for the Reborn overlay key stream.
#[derive(Default)]
pub struct KeyPacer {
    wait_frames: u8,
}

impl KeyPacer {
    pub fn next(&mut self, queue: &mut KeyQueue, overlay_open: bool) -> Option<u8> {
        if !overlay_open {
            return queue.take_home().then_some(b'h');
        }
        if self.wait_frames != 0 {
            self.wait_frames -= 1;
            return None;
        }
        let key = queue.pop()?;
        if key == 0 {
            return None;
        }
        self.wait_frames = 12;
        Some(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_filters_preserves_order_and_limits_capacity() {
        let mut queue = KeyQueue::new();
        queue.push(b"xudah1?2w");
        assert_eq!(queue.pop(), Some(b'u'));
        assert_eq!(queue.pop(), Some(b'd'));
        assert_eq!(queue.pop(), Some(b'a'));
        assert_eq!(queue.pop(), Some(b'h'));
        assert_eq!(queue.pop(), Some(b'1'));
        assert_eq!(queue.pop(), Some(b'2'));
        assert_eq!(queue.pop(), Some(b'w'));
        assert_eq!(queue.pop(), None);
        queue.push(&[b'a'; 64]);
        queue.push(b"h");
        for _ in 0..64 {
            assert_eq!(queue.pop(), Some(b'a'));
        }
        assert_eq!(queue.pop(), None);
    }

    #[test]
    fn closed_overlay_discards_keys_until_one_home() {
        let mut queue = KeyQueue::new();
        queue.push(b"udhhb");
        assert!(queue.take_home());
        assert!(queue.take_home());
        assert!(!queue.take_home());
        assert_eq!(queue.pop(), None);
    }

    #[test]
    fn overlay_keys_are_spaced_by_twelve_frames() {
        let mut queue = KeyQueue::new();
        let mut pacer = KeyPacer::default();
        queue.push(b"hahb");
        assert_eq!(pacer.next(&mut queue, false), Some(b'h'));
        assert_eq!(pacer.next(&mut queue, true), Some(b'a'));
        for _ in 0..12 {
            assert_eq!(pacer.next(&mut queue, true), None);
        }
        assert_eq!(pacer.next(&mut queue, true), Some(b'h'));
        assert_eq!(pacer.next(&mut queue, false), None);
    }

    #[test]
    fn queued_home_and_navigation_reach_the_real_controller() {
        use crate::app::{AppController, Screen};
        let mut controller = AppController::new();
        let mut queue = KeyQueue::new();
        let mut pacer = KeyPacer::default();
        queue.push(b"dhda");
        for _ in 0..16 {
            if let Some(input) = pacer
                .next(&mut queue, controller.screen == Screen::MainMenu)
                .and_then(input_for_key)
            {
                controller.handle(input, &[]);
            }
        }
        assert_eq!(controller.screen, Screen::About);
    }

    #[test]
    fn hbck_header_checks_length_without_changing_other_bytes() {
        let mut header = [0u8; 16];
        header[..4].copy_from_slice(b"HBCK");
        header[5] = 64;
        header[6] = 0xaa;
        assert_eq!(body_length(&header), Some(64));
        header[5] = 65;
        assert_eq!(body_length(&header), None);
        header[5] = 0;
        assert_eq!(body_length(&header), Some(0));
        header[0] = b'X';
        assert_eq!(body_length(&header), None);
    }
}
