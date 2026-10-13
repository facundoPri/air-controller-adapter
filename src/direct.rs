//! Single-owner report routing. No other keyboard/mouse is intercepted.
use crate::report::{Report, decode};
use crate::studio::{Action, ButtonId, Config, InputSource, MouseMode, Profile, sources};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, PartialEq)]
pub enum Output {
    Key { usage: u8, down: bool },
    Consumer(u16),
    MouseButton { button: u8, down: bool },
    Motion { x: i8, y: i8, wheel: i8 },
    Action(ButtonId, Profile),
}

#[derive(Default)]
pub struct Router {
    held: BTreeMap<InputSource, bool>, // whether the original down was forwarded
    pub last_mouse: Option<Instant>,
}
impl Router {
    pub fn mode(&self, now: Instant, idle_ms: u64) -> MouseMode {
        if self
            .last_mouse
            .is_some_and(|at| now.saturating_duration_since(at) < Duration::from_millis(idle_ms))
        {
            MouseMode::Active
        } else {
            MouseMode::Normal
        }
    }
    pub fn process(&mut self, bytes: &[u8], now: Instant, config: &Config) -> Vec<Output> {
        let mut out = vec![];
        let mut profile = if self.mode(now, config.mouse_idle_ms) == MouseMode::Active {
            Profile::AirMouse
        } else {
            Profile::Normal
        };
        let (kind, current): (u8, Vec<InputSource>) = match decode(bytes) {
            Some(Report::Keyboard { modifiers, keys }) => {
                if keys.iter().any(|k| (0x4f..=0x52).contains(k)) {
                    self.last_mouse = None;
                }
                let mut current: Vec<_> = keys
                    .iter()
                    .filter(|k| **k > 3)
                    .map(|k| InputSource::Keyboard(*k))
                    .collect();
                for bit in 0..8 {
                    if modifiers & (1 << bit) != 0 {
                        current.push(InputSource::Keyboard(0xe0 + bit));
                    }
                }
                current.sort_by_key(|s| !matches!(s, InputSource::Keyboard(0xe0..=0xe7)));
                (1, current)
            }
            Some(Report::Mouse {
                buttons,
                x,
                y,
                wheel,
            }) => {
                self.last_mouse = Some(now);
                profile = Profile::AirMouse;
                out.push(Output::Motion { x, y, wheel });
                (
                    2,
                    (0..5)
                        .filter(|b| buttons & (1 << b) != 0)
                        .map(|b| InputSource::Mouse(b + 1))
                        .collect(),
                )
            }
            Some(Report::Consumer(usage)) => (
                3,
                if usage == 0 {
                    vec![]
                } else {
                    vec![InputSource::Consumer(usage)]
                },
            ),
            _ => return out,
        };
        let same_kind = |s: &InputSource| {
            matches!(
                (kind, s),
                (1, InputSource::Keyboard(_))
                    | (2, InputSource::Mouse(_))
                    | (3, InputSource::Consumer(_))
            )
        };
        let released: Vec<_> = self
            .held
            .keys()
            .filter(|s| same_kind(s) && !current.contains(s))
            .cloned()
            .collect();
        for s in released {
            if self.held.remove(&s) == Some(true) {
                Self::release(&s, &mut out);
            }
        }
        for s in current {
            if self.held.contains_key(&s) {
                continue;
            }
            let mapping = config
                .buttons
                .iter()
                .find(|b| sources(b, profile).contains(&s));
            let action = mapping.map(|b| {
                if profile == Profile::Normal {
                    &b.normal
                } else {
                    &b.air_mouse
                }
            });
            let forward = action.is_none_or(|a| *a == Action::Default);
            if forward {
                match s {
                    InputSource::Keyboard(usage) => out.push(Output::Key { usage, down: true }),
                    InputSource::Mouse(button) => {
                        out.push(Output::MouseButton { button, down: true })
                    }
                    InputSource::Consumer(usage) => out.push(Output::Consumer(usage)),
                }
            } else if action != Some(&Action::Disabled) {
                out.push(Output::Action(mapping.unwrap().button, profile));
            }
            self.held.insert(s, forward);
        }
        out
    }
    fn release(s: &InputSource, out: &mut Vec<Output>) {
        match *s {
            InputSource::Keyboard(usage) => out.push(Output::Key { usage, down: false }),
            InputSource::Mouse(button) => out.push(Output::MouseButton {
                button,
                down: false,
            }),
            InputSource::Consumer(_) => {} // consumer output is an atomic tap
        }
    }
    pub fn release_all(&mut self) -> Vec<Output> {
        let mut out = vec![];
        for (s, forwarded) in std::mem::take(&mut self.held) {
            if forwarded {
                Self::release(&s, &mut out);
            }
        }
        self.last_mouse = None;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edits_take_effect_on_next_press_without_reopening_reader() {
        let mut config = Config::default();
        let i = config
            .buttons
            .iter()
            .position(|b| b.button == ButtonId::Capture)
            .unwrap();
        config.buttons[i].normal_sources = vec![InputSource::Consumer(0x6c)];
        config.buttons[i].normal = Action::Disabled;
        let mut router = Router::default();
        let t = Instant::now();
        assert!(router.process(&[3, 0x6c, 0], t, &config).is_empty());
        config.buttons[i].normal = Action::System {
            command: crate::studio::SystemAction::ShowDesktop,
        };
        assert!(
            router.process(&[3, 0x6c, 0], t, &config).is_empty(),
            "editing cannot execute an already-held key"
        );
        router.process(&[3, 0, 0], t, &config);
        assert_eq!(
            router.process(&[3, 0x6c, 0], t, &config),
            vec![Output::Action(ButtonId::Capture, Profile::Normal)]
        );
    }
    #[test]
    fn capture_dispatches_once_and_suppresses_original() {
        let mut c = Config::default();
        let b = c
            .buttons
            .iter_mut()
            .find(|b| b.button == ButtonId::Capture)
            .unwrap();
        b.normal_sources = vec![InputSource::Consumer(0x6c)];
        b.normal = Action::System {
            command: crate::studio::SystemAction::MissionControl,
        };
        let mut r = Router::default();
        let now = Instant::now();
        assert_eq!(
            r.process(&[3, 0x6c, 0], now, &c),
            vec![Output::Action(ButtonId::Capture, Profile::Normal)]
        );
        assert!(r.process(&[3, 0x6c, 0], now, &c).is_empty());
        r.process(&[3, 0, 0], now, &c);
        assert_eq!(r.process(&[3, 0x6c, 0], now, &c).len(), 1);
    }
    #[test]
    fn air_arrow_uses_air_action_then_normal_and_idle_returns_normal() {
        let mut c = Config::default();
        c.buttons
            .iter_mut()
            .find(|b| b.button == ButtonId::Up)
            .unwrap()
            .air_mouse = Action::Scroll { x: 0, y: 3 };
        let mut r = Router::default();
        let t = Instant::now();
        assert_eq!(r.mode(t, 1500), MouseMode::Normal);
        r.process(&[2, 0, 1, 0, 0], t, &c);
        assert_eq!(r.mode(t, 1500), MouseMode::Active);
        assert_eq!(r.mode(t + Duration::from_secs(2), 1500), MouseMode::Normal);
        assert_eq!(
            r.process(&[1, 0, 0, 0x52, 0, 0, 0, 0, 0], t, &c),
            vec![Output::Action(ButtonId::Up, Profile::AirMouse)]
        );
        assert_eq!(r.mode(t, 1500), MouseMode::Normal);
        assert!(r.process(&[1, 0, 0, 0, 0, 0, 0, 0, 0], t, &c).is_empty());
        assert_eq!(
            r.process(&[1, 0, 0, 0x52, 0, 0, 0, 0, 0], t, &c),
            vec![Output::Key {
                usage: 0x52,
                down: true
            }]
        );
    }
    #[test]
    fn default_click_is_released_even_after_mapping_changes() {
        let mut r = Router::default();
        let t = Instant::now();
        let mut c = Config::default();
        assert!(
            r.process(&[2, 1, 0, 0, 0], t, &c)
                .contains(&Output::MouseButton {
                    button: 1,
                    down: true
                })
        );
        c.buttons[8].mouse_sources = vec![InputSource::Mouse(1)];
        c.buttons[8].air_mouse = Action::Disabled;
        assert!(
            r.process(&[2, 0, 0, 0, 0], t, &c)
                .contains(&Output::MouseButton {
                    button: 1,
                    down: false
                })
        );
        r.process(&[1, 0, 0, 0x52, 0, 0, 0, 0, 0], t, &c);
        assert_eq!(
            r.release_all(),
            vec![Output::Key {
                usage: 0x52,
                down: false
            }]
        );
    }
}
