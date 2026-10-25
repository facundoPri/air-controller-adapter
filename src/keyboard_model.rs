use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyboardAction {
    Toggle,
    Up,
    Down,
    Left,
    Right,
    Select,
    Delete,
}
impl KeyboardAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::Toggle => "Toggle keyboard",
            Self::Up => "Keyboard up",
            Self::Down => "Keyboard down",
            Self::Left => "Keyboard left",
            Self::Right => "Keyboard right",
            Self::Select => "Select keyboard key",
            Self::Delete => "Delete character",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum KeyboardLayout {
    #[default]
    Qwerty,
    Alphabetical,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum KeyboardLanguage {
    #[default]
    English,
    Spanish,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyboardPosition {
    pub x: f32,
    pub y: f32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyboardPreferences {
    pub layout: KeyboardLayout,
    pub language: KeyboardLanguage,
    pub scale: f32,
    pub opacity: f32,
    pub position: Option<KeyboardPosition>,
}
impl Default for KeyboardPreferences {
    fn default() -> Self {
        Self {
            layout: KeyboardLayout::Qwerty,
            language: KeyboardLanguage::English,
            scale: 1.0,
            opacity: 0.95,
            position: None,
        }
    }
}
impl KeyboardPreferences {
    pub fn validate(&self) -> Result<(), String> {
        if !self.scale.is_finite() || !(0.7..=1.5).contains(&self.scale) {
            return Err("Keyboard scale must be between 0.7 and 1.5".into());
        }
        if !self.opacity.is_finite() || !(0.4..=1.0).contains(&self.opacity) {
            return Err("Keyboard opacity must be between 0.4 and 1.0".into());
        }
        if self.position.is_some_and(|p| {
            !p.x.is_finite() || !p.y.is_finite() || p.x.abs() > 100_000.0 || p.y.abs() > 100_000.0
        }) {
            return Err("Keyboard position must have finite coordinates within ±100000".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Letter(char),
    Space,
    Backspace,
    Return,
    Tab,
    Shift,
}
pub fn keys(preferences: &KeyboardPreferences) -> Vec<Vec<Key>> {
    let spanish = preferences.language == KeyboardLanguage::Spanish;
    let letters: &[&str] = match (preferences.layout, spanish) {
        (KeyboardLayout::Qwerty, false) => &["qwertyuiop", "asdfghjkl", "zxcvbnm"],
        (KeyboardLayout::Qwerty, true) => &["qwertyuiop", "asdfghjklñ", "zxcvbnm", "áéíóúü"],
        (KeyboardLayout::Alphabetical, false) => &["abcdefghi", "jklmnopqr", "stuvwxyz"],
        (KeyboardLayout::Alphabetical, true) => &["abcdefghi", "jklmnñopq", "rstuvwxyz", "áéíóúü"],
    };
    let mut rows: Vec<_> = letters
        .iter()
        .map(|row| row.chars().map(Key::Letter).collect())
        .collect();
    rows.push(vec![
        Key::Shift,
        Key::Tab,
        Key::Space,
        Key::Backspace,
        Key::Return,
    ]);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layouts_contain_each_language_character_once_and_all_controls() {
        for layout in [KeyboardLayout::Qwerty, KeyboardLayout::Alphabetical] {
            for language in [KeyboardLanguage::English, KeyboardLanguage::Spanish] {
                let preferences = KeyboardPreferences {
                    layout,
                    language,
                    ..Default::default()
                };
                let keys: Vec<_> = keys(&preferences).into_iter().flatten().collect();
                let letters: std::collections::BTreeSet<_> = keys
                    .iter()
                    .filter_map(|key| match key {
                        Key::Letter(c) => Some(*c),
                        _ => None,
                    })
                    .collect();
                let expected: std::collections::BTreeSet<_> =
                    if language == KeyboardLanguage::Spanish {
                        "abcdefghijklmnopqrstuvwxyzñáéíóúü"
                    } else {
                        "abcdefghijklmnopqrstuvwxyz"
                    }
                    .chars()
                    .collect();
                assert_eq!(letters, expected);
                assert_eq!(keys.len(), letters.len() + 5);
                for control in [
                    Key::Space,
                    Key::Backspace,
                    Key::Return,
                    Key::Tab,
                    Key::Shift,
                ] {
                    assert!(keys.contains(&control));
                }
            }
        }
    }
    #[test]
    fn preferences_reject_nonfinite_and_out_of_bounds_values() {
        for scale in [f32::NAN, f32::INFINITY, 0.69, 1.51] {
            assert!(
                KeyboardPreferences {
                    scale,
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        for opacity in [f32::NAN, f32::NEG_INFINITY, 0.39, 1.01] {
            assert!(
                KeyboardPreferences {
                    opacity,
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        for x in [f32::NAN, f32::INFINITY, 100_001.0, -100_001.0] {
            assert!(
                KeyboardPreferences {
                    position: Some(KeyboardPosition { x, y: 0.0 }),
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        assert!(KeyboardPreferences::default().validate().is_ok());
    }
}
