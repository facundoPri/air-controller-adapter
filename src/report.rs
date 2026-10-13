//! One validation boundary shared by routing, learning and mode observation.
#[derive(Clone, Copy)]
pub enum Report<'a> {
    Keyboard {
        modifiers: u8,
        keys: &'a [u8],
    },
    Mouse {
        buttons: u8,
        x: i8,
        y: i8,
        wheel: i8,
    },
    Consumer(u16),
}
pub fn decode(bytes: &[u8]) -> Option<Report<'_>> {
    Some(match bytes {
        [1, modifiers, 0, keys @ ..]
            if keys.len() == 6 && !keys.iter().any(|k| (1..=3).contains(k)) =>
        {
            Report::Keyboard {
                modifiers: *modifiers,
                keys,
            }
        }
        [2, buttons, x, y, wheel] if buttons & 0xe0 == 0 => Report::Mouse {
            buttons: *buttons,
            x: *x as i8,
            y: *y as i8,
            wheel: *wheel as i8,
        },
        [3, lo, hi] if u16::from_le_bytes([*lo, *hi]) <= 0x3ff => {
            Report::Consumer(u16::from_le_bytes([*lo, *hi]))
        }
        _ => return None,
    })
}
