//! Button recognition for the observed MR-01 keyboard/consumer HID reports.
//! Mouse movement and vendor reports never generate keyboard or consumer button presses.
use crate::report::{Report, decode};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Button {
    Keyboard(u8),
    Consumer(u16),
}
impl std::fmt::Display for Button {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Keyboard(code) => write!(f, "HID key {code:02X}"),
            Self::Consumer(code) => write!(f, "Consumer HID usage {code:04X}"),
        }
    }
}

#[derive(Default)]
pub struct Presses {
    keyboard: BTreeSet<Button>,
    consumer: BTreeSet<Button>,
}
impl Presses {
    pub fn is_released(&self) -> bool {
        self.keyboard.is_empty() && self.consumer.is_empty()
    }

    pub fn update(&mut self, bytes: &[u8]) -> Vec<Button> {
        let (previous, current) = match decode(bytes) {
            Some(Report::Keyboard { modifiers, keys }) => {
                let mut keys: BTreeSet<_> = keys
                    .iter()
                    .copied()
                    .filter(|&k| k > 3)
                    .map(Button::Keyboard)
                    .collect();
                for bit in 0..8 {
                    if modifiers & (1 << bit) != 0 {
                        keys.insert(Button::Keyboard(0xe0 + bit));
                    }
                }
                (&mut self.keyboard, keys)
            }
            Some(Report::Consumer(usage)) => {
                let keys = if usage == 0 {
                    BTreeSet::new()
                } else {
                    BTreeSet::from([Button::Consumer(usage)])
                };
                (&mut self.consumer, keys)
            }
            _ => return vec![],
        };
        let pressed = current.difference(previous).copied().collect();
        *previous = current;
        pressed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn held_key_fires_once_and_mouse_reports_do_not_rearm_it() {
        let mut p = Presses::default();
        let down = [1, 0, 0, 0x28, 0, 0, 0, 0, 0];
        assert_eq!(p.update(&down), [Button::Keyboard(0x28)]);
        assert!(p.update(&down).is_empty());
        assert!(p.update(&[2, 0, 1, 0, 0]).is_empty());
        assert!(p.update(&down).is_empty());
        assert!(p.update(&[1, 0, 0, 0, 0, 0, 0, 0, 0]).is_empty());
        assert_eq!(p.update(&down), [Button::Keyboard(0x28)]);
    }
    #[test]
    fn consumer_release_is_independent_of_keyboard_and_uses_little_endian() {
        let mut p = Presses::default();
        assert_eq!(p.update(&[3, 0x21, 2]), [Button::Consumer(0x221)]);
        assert!(p.update(&[1, 0, 0, 0, 0, 0, 0, 0, 0]).is_empty());
        assert!(p.update(&[3, 0x21, 2]).is_empty());
        assert!(p.update(&[3, 0, 0]).is_empty());
        assert_eq!(p.update(&[3, 0x21, 2]), [Button::Consumer(0x221)]);
    }
    #[test]
    fn rejects_vendor_malformed_and_rollover_reports() {
        let mut p = Presses::default();
        for bytes in [
            vec![251, 1, 2],
            vec![252, 1],
            vec![1, 0, 0, 0x28],
            vec![3, 0, 4],
            vec![1, 0, 0, 1, 1, 1, 1, 1, 1],
        ] {
            assert!(p.update(&bytes).is_empty());
        }
    }
}
