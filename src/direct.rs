//! Single-owner report routing. No other keyboard/mouse is intercepted.
use crate::report::{Report, decode};
use crate::studio::{Action, ButtonId, Config, InputSource, MouseMode, Profile, sources};
#[cfg(feature = "virtual-keyboard")]
use crate::{
    keyboard::{Control, Phase},
    keyboard_model::KeyboardAction,
};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, PartialEq)]
pub enum Output {
    Key {
        usage: u8,
        down: bool,
    },
    Consumer(u16),
    MouseButton {
        button: u8,
        down: bool,
    },
    Motion {
        x: i8,
        y: i8,
        wheel: i8,
    },
    Action(ButtonId, Profile),
    #[cfg(feature = "virtual-keyboard")]
    SessionAction {
        button: ButtonId,
        generation: u64,
    },
    #[cfg(feature = "virtual-keyboard")]
    KeyboardPointer {
        generation: u64,
        press: u64,
        down: bool,
    },
}
#[derive(Clone, Copy)]
struct Hold {
    forwarded: bool,
    #[cfg(feature = "virtual-keyboard")]
    press: u64,
    #[cfg(feature = "virtual-keyboard")]
    button: Option<ButtonId>,
    #[cfg(feature = "virtual-keyboard")]
    route: Option<KeyboardRoute>,
}
#[cfg(feature = "virtual-keyboard")]
#[derive(Clone, Copy)]
enum KeyboardRoute {
    Command,
    Pointer { generation: u64 },
}

#[derive(Default)]
pub struct Router {
    held: BTreeMap<InputSource, Hold>,
    pub last_mouse: Option<Instant>,
    #[cfg(feature = "virtual-keyboard")]
    keyboard: Option<Control>,
    #[cfg(feature = "virtual-keyboard")]
    generation: u64,
    #[cfg(feature = "virtual-keyboard")]
    pending_fence: Option<u64>,
    #[cfg(feature = "virtual-keyboard")]
    pending_releases: Vec<Output>,
    #[cfg(feature = "virtual-keyboard")]
    next_press: u64,
}
impl Router {
    #[cfg(feature = "virtual-keyboard")]
    pub fn with_keyboard(mut self, keyboard: Control) -> Self {
        self.generation = keyboard.snapshot().generation;
        self.pending_fence = Some(self.generation);
        keyboard.set_router_online(true);
        self.keyboard = Some(keyboard);
        self
    }
    /// Return synthetic releases before acknowledging this generation in the sender.
    #[cfg(feature = "virtual-keyboard")]
    pub fn poll_keyboard(&mut self) -> Vec<Output> {
        let mut out = vec![];
        let Some(control) = &self.keyboard else {
            return out;
        };
        let snapshot = control.snapshot();
        if snapshot.generation != self.generation {
            self.generation = snapshot.generation;
            self.pending_fence = Some(snapshot.generation);
            for (source, hold) in &mut self.held {
                if hold.forwarded {
                    Self::release(source, &mut out);
                }
                hold.forwarded = false;
                hold.route = None;
            }
        }
        self.pending_releases.extend(out.iter().cloned());
        out
    }
    /// The caller must have successfully sent all outputs from process/poll first.
    #[cfg(feature = "virtual-keyboard")]
    pub fn acknowledge_keyboard_fence(&mut self) {
        self.pending_releases.clear();
        if let Some(generation) = self.pending_fence.take() {
            if let Some(control) = &self.keyboard {
                control.acknowledge_fence(generation);
            }
        }
    }
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
        #[cfg(feature = "virtual-keyboard")]
        let mut out = self.poll_keyboard();
        #[cfg(not(feature = "virtual-keyboard"))]
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
        for source in released {
            let hold = self.held.remove(&source).unwrap();
            if hold.forwarded {
                Self::release(&source, &mut out);
            }
            #[cfg(feature = "virtual-keyboard")]
            if !self.held.values().any(|other| other.press == hold.press) {
                if let Some(control) = &self.keyboard {
                    if let Some(KeyboardRoute::Pointer { generation }) = hold.route {
                        out.push(Output::KeyboardPointer {
                            generation,
                            press: hold.press,
                            down: false,
                        });
                    }
                    control.release(hold.press);
                }
            }
        }
        for source in current {
            if self.held.contains_key(&source) {
                continue;
            }
            let mapping = config
                .buttons
                .iter()
                .find(|b| sources(b, profile).contains(&source));
            let action = mapping.map(|b| {
                if profile == Profile::Normal {
                    &b.normal
                } else {
                    &b.air_mouse
                }
            });
            #[cfg(feature = "virtual-keyboard")]
            if let Some(control) = self.keyboard.clone() {
                let snapshot = control.snapshot();
                let mapping = if snapshot.phase != Phase::Hidden {
                    config
                        .buttons
                        .iter()
                        .find(|b| {
                            Some(b.button) == snapshot.opener
                                && (sources(b, Profile::Normal).contains(&source)
                                    || sources(b, Profile::AirMouse).contains(&source))
                        })
                        .or(mapping)
                } else {
                    mapping
                };
                let button = mapping.map(|b| b.button);
                if let Some(existing) = button
                    .and_then(|button| self.held.values().find(|h| h.button == Some(button)))
                    .copied()
                {
                    self.held.insert(
                        source,
                        Hold {
                            forwarded: false,
                            ..existing
                        },
                    );
                    continue;
                }
                self.next_press = self.next_press.wrapping_add(1);
                let mut hold = Hold {
                    forwarded: false,
                    press: self.next_press,
                    button,
                    route: None,
                };
                let opener = button.is_some()
                    && button == snapshot.opener
                    && snapshot.phase != Phase::Hidden;
                let effective = if snapshot.phase == Phase::Visible {
                    mapping.map(|b| &b.keyboard)
                } else {
                    action
                };
                if opener {
                    let _ = control.toggle_for(snapshot.generation, button, now);
                } else if matches!(snapshot.phase, Phase::Opening | Phase::Closing) {
                } else if let Some(Action::Keyboard {
                    command: KeyboardAction::Toggle,
                }) = effective
                {
                    let _ = control.toggle_for(snapshot.generation, button, now);
                } else if snapshot.phase == Phase::Visible
                    && source == InputSource::Mouse(1)
                    && matches!(
                        effective,
                        Some(Action::Keyboard {
                            command: KeyboardAction::Select
                        })
                    )
                {
                    hold.route = Some(KeyboardRoute::Pointer {
                        generation: snapshot.generation,
                    });
                    out.push(Output::KeyboardPointer {
                        generation: snapshot.generation,
                        press: hold.press,
                        down: true,
                    });
                } else if let Some(Action::Keyboard { command }) = effective {
                    hold.route = Some(KeyboardRoute::Command);
                    control.press_for(snapshot.generation, *command, hold.press, now);
                } else if snapshot.phase == Phase::Visible {
                    match effective {
                        Some(Action::Default) => {
                            hold.forwarded = Self::dispatch(
                                &source,
                                effective,
                                button,
                                Profile::Keyboard,
                                &mut out,
                            );
                        }
                        Some(Action::Disabled) | None => {}
                        Some(_) => out.push(Output::SessionAction {
                            button: button.unwrap(),
                            generation: snapshot.generation,
                        }),
                    }
                } else {
                    hold.forwarded = Self::dispatch(&source, effective, button, profile, &mut out);
                }
                self.held.insert(source, hold);
                out.extend(self.poll_keyboard());
                continue;
            }
            let forwarded = Self::dispatch(
                &source,
                action,
                mapping.map(|b| b.button),
                profile,
                &mut out,
            );
            self.held.insert(
                source,
                Hold {
                    forwarded,
                    #[cfg(feature = "virtual-keyboard")]
                    press: 0,
                    #[cfg(feature = "virtual-keyboard")]
                    button: mapping.map(|b| b.button),
                    #[cfg(feature = "virtual-keyboard")]
                    route: None,
                },
            );
        }
        out
    }
    fn dispatch(
        source: &InputSource,
        action: Option<&Action>,
        button: Option<ButtonId>,
        profile: Profile,
        out: &mut Vec<Output>,
    ) -> bool {
        let forward = action.is_none_or(|a| *a == Action::Default);
        if forward {
            match *source {
                InputSource::Keyboard(usage) => out.push(Output::Key { usage, down: true }),
                InputSource::Mouse(button) => out.push(Output::MouseButton { button, down: true }),
                InputSource::Consumer(usage) => out.push(Output::Consumer(usage)),
            }
        } else if action != Some(&Action::Disabled) {
            out.push(Output::Action(button.unwrap(), profile));
        }
        forward
    }
    fn release(source: &InputSource, out: &mut Vec<Output>) {
        match *source {
            InputSource::Keyboard(usage) => out.push(Output::Key { usage, down: false }),
            InputSource::Mouse(button) => out.push(Output::MouseButton {
                button,
                down: false,
            }),
            InputSource::Consumer(_) => {}
        }
    }
    pub fn release_all(&mut self) -> Vec<Output> {
        #[cfg(feature = "virtual-keyboard")]
        let mut out = std::mem::take(&mut self.pending_releases);
        #[cfg(not(feature = "virtual-keyboard"))]
        let mut out = vec![];
        for (source, hold) in std::mem::take(&mut self.held) {
            if hold.forwarded {
                Self::release(&source, &mut out);
            }
            #[cfg(feature = "virtual-keyboard")]
            if let Some(control) = &self.keyboard {
                control.release(hold.press);
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

#[cfg(all(test, feature = "virtual-keyboard"))]
mod keyboard_tests {
    use super::*;
    use crate::keyboard::Command;
    fn key(usage: u8) -> [u8; 9] {
        [1, 0, 0, usage, 0, 0, 0, 0, 0]
    }
    fn config(opener: ButtonId) -> Config {
        let mut c = Config::default();
        c.buttons
            .iter_mut()
            .find(|b| b.button == opener)
            .unwrap()
            .normal = Action::Keyboard {
            command: KeyboardAction::Toggle,
        };
        c
    }
    fn visible(control: &Control, router: &mut Router, now: Instant) {
        let g = control.toggle(None, now).unwrap();
        assert!(router.poll_keyboard().is_empty());
        assert!(!control.acknowledge_open(g));
        router.acknowledge_keyboard_fence();
        assert!(control.acknowledge_open(g));
        control.drain(now);
    }
    fn actions(control: &Control, now: Instant) -> Vec<KeyboardAction> {
        control
            .drain(now)
            .into_iter()
            .filter_map(|r| match r.command {
                Command::Action { action, .. } | Command::Repeat { action, .. } => Some(action),
                _ => None,
            })
            .collect()
    }
    #[test]
    fn every_observable_opener_consumes_down_hold_release_and_close() {
        for (button, down, up) in [
            (ButtonId::Home, vec![3, 0x6a, 0], vec![3, 0, 0]),
            (ButtonId::Capture, vec![3, 0x6c, 0], vec![3, 0, 0]),
            (ButtonId::Ok, key(0x28).to_vec(), key(0).to_vec()),
            (ButtonId::Up, key(0x52).to_vec(), key(0).to_vec()),
        ] {
            let control = Control::default();
            let mut router = Router::default().with_keyboard(control.clone());
            let c = config(button);
            let t = Instant::now();
            assert!(router.process(&down, t, &c).is_empty());
            let g = control.snapshot().generation;
            assert_eq!(control.snapshot().phase, Phase::Opening);
            assert!(!control.acknowledge_open(g));
            router.acknowledge_keyboard_fence();
            assert!(control.acknowledge_open(g));
            control.drain(t);
            assert!(router.process(&down, t, &c).is_empty());
            assert!(router.process(&up, t, &c).is_empty());
            assert!(router.process(&down, t, &c).is_empty());
            assert_eq!(control.snapshot().phase, Phase::Closing);
            assert!(actions(&control, t).is_empty());
        }
    }
    #[test]
    fn normal_opener_closes_after_mouse_mode_changes() {
        let control = Control::default();
        let mut router = Router::default().with_keyboard(control.clone());
        let mut c = config(ButtonId::Home);
        c.buttons
            .iter_mut()
            .find(|b| b.button == ButtonId::Home)
            .unwrap()
            .mouse_sources = vec![InputSource::Consumer(0x100)];
        c.validate().unwrap();
        let t = Instant::now();
        assert!(router.process(&[3, 0x6a, 0], t, &c).is_empty());
        router.acknowledge_keyboard_fence();
        assert!(control.acknowledge_open(control.snapshot().generation));
        control.drain(t);
        assert!(router.process(&[3, 0, 0], t, &c).is_empty());
        assert_eq!(
            router.process(&[2, 0, 1, 0, 0], t, &c),
            vec![Output::Motion {
                x: 1,
                y: 0,
                wheel: 0
            }]
        );
        assert_eq!(router.mode(t, c.mouse_idle_ms), MouseMode::Active);
        assert!(router.process(&[3, 0x6a, 0], t, &c).is_empty());
        assert_eq!(control.snapshot().phase, Phase::Closing);
        assert!(actions(&control, t).is_empty());
        router.acknowledge_keyboard_fence();
        control.acknowledge_close(control.snapshot().generation);
        assert!(router.process(&[3, 0x6a, 0], t, &c).is_empty());
        assert!(router.process(&[3, 0, 0], t, &c).is_empty());
        assert_eq!(
            router.process(&[3, 0x6a, 0], t, &c),
            vec![Output::Consumer(0x6a)]
        );
    }
    #[test]
    fn air_mouse_opener_closes_after_mouse_mode_expires() {
        let control = Control::default();
        let mut router = Router::default().with_keyboard(control.clone());
        let mut c = Config::default();
        let home = c
            .buttons
            .iter_mut()
            .find(|b| b.button == ButtonId::Home)
            .unwrap();
        home.mouse_sources = vec![InputSource::Consumer(0x100)];
        home.air_mouse = Action::Keyboard {
            command: KeyboardAction::Toggle,
        };
        c.validate().unwrap();
        let t = Instant::now();
        router.process(&[2, 0, 1, 0, 0], t, &c);
        assert!(router.process(&[3, 0, 1], t, &c).is_empty());
        router.acknowledge_keyboard_fence();
        assert!(control.acknowledge_open(control.snapshot().generation));
        control.drain(t);
        assert!(router.process(&[3, 0, 0], t, &c).is_empty());
        let idle = t + Duration::from_millis(c.mouse_idle_ms);
        assert_eq!(router.mode(idle, c.mouse_idle_ms), MouseMode::Normal);
        assert!(router.process(&[3, 0, 1], idle, &c).is_empty());
        assert_eq!(control.snapshot().phase, Phase::Closing);
        assert!(actions(&control, idle).is_empty());
        router.acknowledge_keyboard_fence();
        control.acknowledge_close(control.snapshot().generation);
        assert!(router.process(&[3, 0, 1], idle, &c).is_empty());
        assert!(router.process(&[3, 0, 0], idle, &c).is_empty());
        assert_eq!(
            router.process(&[3, 0, 1], idle, &c),
            vec![Output::Consumer(0x100)]
        );
    }
    #[test]
    fn transition_releases_forwarded_holds_and_waits_for_sender_ack() {
        let control = Control::default();
        let mut router = Router::default().with_keyboard(control.clone());
        let c = Config::default();
        let t = Instant::now();
        assert_eq!(
            router.process(&key(0x52), t, &c),
            vec![Output::Key {
                usage: 0x52,
                down: true
            }]
        );
        let g = control.toggle(None, t).unwrap();
        assert_eq!(
            router.poll_keyboard(),
            vec![Output::Key {
                usage: 0x52,
                down: false
            }]
        );
        assert!(
            !control.acknowledge_open(g),
            "generated releases have not reached the sender yet"
        );
        router.acknowledge_keyboard_fence();
        assert!(control.acknowledge_open(g));
        control.drain(t);
        assert!(router.process(&key(0x52), t, &c).is_empty());
        assert!(router.process(&key(0), t, &c).is_empty());
        assert!(actions(&control, t).is_empty());
        assert!(router.process(&key(0x52), t, &c).is_empty());
        assert_eq!(actions(&control, t), vec![KeyboardAction::Up]);
        assert!(router.release_all().is_empty());
    }
    #[test]
    fn unsent_transition_releases_remain_available_for_failure_cleanup() {
        let control = Control::default();
        let mut router = Router::default().with_keyboard(control.clone());
        let c = Config::default();
        let t = Instant::now();
        router.process(&key(0x52), t, &c);
        let generation = control.toggle(None, t).unwrap();
        assert_eq!(
            router.poll_keyboard(),
            vec![Output::Key {
                usage: 0x52,
                down: false
            }]
        );
        assert!(!control.ready_to_open(generation));
        assert_eq!(
            router.release_all(),
            vec![Output::Key {
                usage: 0x52,
                down: false
            }]
        );
        assert!(!control.ready_to_open(generation));
    }
    #[test]
    fn transitional_presses_remain_fenced_after_visible() {
        let control = Control::default();
        let mut router = Router::default().with_keyboard(control.clone());
        let c = config(ButtonId::Home);
        let t = Instant::now();
        router.process(&[3, 0x6a, 0], t, &c);
        assert!(router.process(&key(0x28), t, &c).is_empty());
        router.acknowledge_keyboard_fence();
        assert!(control.acknowledge_open(control.snapshot().generation));
        control.drain(t);
        assert!(router.process(&key(0x28), t, &c).is_empty());
        assert!(router.process(&key(0), t, &c).is_empty());
        assert!(actions(&control, t).is_empty());
        router.process(&key(0x28), t, &c);
        assert_eq!(actions(&control, t), vec![KeyboardAction::Select]);
    }
    #[test]
    fn overlapping_ok_aliases_share_one_activation_in_either_order() {
        for pointer_first in [false, true] {
            let control = Control::default();
            let mut router = Router::default().with_keyboard(control.clone());
            let c = Config::default();
            let t = Instant::now();
            visible(&control, &mut router, t);
            let reports = if pointer_first {
                vec![
                    vec![2, 1, 0, 0, 0],
                    key(0x28).to_vec(),
                    vec![2, 0, 0, 0, 0],
                    key(0).to_vec(),
                ]
            } else {
                vec![
                    key(0x28).to_vec(),
                    vec![2, 1, 0, 0, 0],
                    key(0).to_vec(),
                    vec![2, 0, 0, 0, 0],
                ]
            };
            let mut pointer_outputs = vec![];
            for bytes in reports {
                for output in router.process(&bytes, t, &c) {
                    match output {
                        Output::Motion { .. } => {}
                        Output::KeyboardPointer { .. } => pointer_outputs.push(output),
                        _ => panic!("remote alias leaked an original or custom action"),
                    }
                }
            }
            let commands: Vec<_> = control.drain(t).into_iter().map(|r| r.command).collect();
            if pointer_first {
                assert!(commands.is_empty());
                assert!(
                    matches!(pointer_outputs.as_slice(), [Output::KeyboardPointer { generation: a, press: p, down: true }, Output::KeyboardPointer { generation: b, press: q, down: false }] if a == b && p == q)
                );
            } else {
                assert!(pointer_outputs.is_empty());
                assert!(matches!(
                    commands.as_slice(),
                    [Command::Action {
                        action: KeyboardAction::Select,
                        ..
                    }]
                ));
            }
        }
    }
    #[test]
    fn ok_opener_mixed_aliases_never_select_return_or_click() {
        let control = Control::default();
        let mut router = Router::default().with_keyboard(control.clone());
        let c = config(ButtonId::Ok);
        let t = Instant::now();
        assert!(router.process(&key(0x28), t, &c).is_empty());
        assert_eq!(
            router.process(&[2, 1, 0, 0, 0], t, &c),
            vec![Output::Motion {
                x: 0,
                y: 0,
                wheel: 0
            }]
        );
        router.acknowledge_keyboard_fence();
        assert!(control.acknowledge_open(control.snapshot().generation));
        control.drain(t);
        router.process(&key(0), t, &c);
        router.process(&[2, 0, 0, 0, 0], t, &c);
        assert!(control.drain(t).is_empty());
        assert_eq!(
            router.process(&[2, 1, 0, 0, 0], t, &c),
            vec![Output::Motion {
                x: 0,
                y: 0,
                wheel: 0
            }]
        );
        router.process(&key(0x28), t, &c);
        assert_eq!(control.snapshot().phase, Phase::Closing);
        assert!(actions(&control, t).is_empty());
    }
    #[test]
    fn pointer_edges_follow_native_motion_before_sampling_coordinates() {
        let control = Control::default();
        let mut router = Router::default().with_keyboard(control.clone());
        let c = Config::default();
        let t = Instant::now();
        visible(&control, &mut router, t);
        for (buttons, down) in [(1, true), (0, false)] {
            let outputs = router.process(&[2, buttons, 5, 3, 0], t, &c);
            assert!(
                matches!(outputs.as_slice(), [Output::Motion { x: 5, y: 3, wheel: 0 }, Output::KeyboardPointer { down: edge, .. }] if *edge == down)
            );
            for output in outputs {
                if let Output::KeyboardPointer {
                    generation,
                    press,
                    down,
                } = output
                {
                    control.pointer_for(generation, press, down, (15.0, 23.0), t);
                }
            }
            assert!(
                matches!(control.drain(t).as_slice(), [crate::keyboard::Request { command: Command::Pointer { down: edge, point: (15.0, 23.0), .. }, .. }] if *edge == down)
            );
        }
    }
    #[test]
    fn visible_mouse_select_honors_disabled_and_custom_keyboard_mapping() {
        for action in [
            Action::Disabled,
            Action::System {
                command: crate::studio::SystemAction::MissionControl,
            },
        ] {
            let control = Control::default();
            let mut router = Router::default().with_keyboard(control.clone());
            let mut c = Config::default();
            c.buttons
                .iter_mut()
                .find(|b| b.button == ButtonId::Ok)
                .unwrap()
                .keyboard = action.clone();
            let t = Instant::now();
            visible(&control, &mut router, t);
            let outputs = router.process(&[2, 1, 0, 0, 0], t, &c);
            if action == Action::Disabled {
                assert_eq!(
                    outputs,
                    vec![Output::Motion {
                        x: 0,
                        y: 0,
                        wheel: 0
                    }]
                );
            } else {
                assert_eq!(
                    outputs,
                    vec![
                        Output::Motion {
                            x: 0,
                            y: 0,
                            wheel: 0
                        },
                        Output::SessionAction {
                            button: ButtonId::Ok,
                            generation: control.snapshot().generation,
                        }
                    ]
                );
            }
            router.process(&[2, 0, 0, 0, 0], t, &c);
            assert!(control.drain(t).is_empty());
        }
    }
    #[test]
    fn unknown_visible_sources_are_consumed_without_native_input() {
        let control = Control::default();
        let mut router = Router::default().with_keyboard(control.clone());
        let c = Config::default();
        let t = Instant::now();
        visible(&control, &mut router, t);
        for report in [
            key(0x04).to_vec(),
            key(0).to_vec(),
            vec![3, 0xff, 0],
            vec![3, 0, 0],
        ] {
            assert!(router.process(&report, t, &c).is_empty());
        }
        assert!(control.drain(t).is_empty());
        assert!(router.release_all().is_empty());
    }
    #[test]
    fn rapid_independent_presses_survive_and_release_stops_repeat() {
        let control = Control::default();
        let mut router = Router::default().with_keyboard(control.clone());
        let c = Config::default();
        let t = Instant::now();
        visible(&control, &mut router, t);
        for _ in 0..3 {
            router.process(&key(0x28), t, &c);
            router.process(&key(0), t, &c);
        }
        assert_eq!(actions(&control, t), vec![KeyboardAction::Select; 3]);
        router.process(&key(0x52), t, &c);
        assert_eq!(actions(&control, t), vec![KeyboardAction::Up]);
        assert!(
            actions(
                &control,
                t + crate::keyboard::REPEAT_DELAY - Duration::from_millis(1)
            )
            .is_empty()
        );
        assert_eq!(
            actions(&control, t + crate::keyboard::REPEAT_DELAY),
            vec![KeyboardAction::Up]
        );
        router.process(&key(0), t + crate::keyboard::REPEAT_DELAY, &c);
        assert!(actions(&control, t + Duration::from_secs(2)).is_empty());
    }
    #[test]
    fn keyboard_custom_action_uses_queue_and_close_restores_current_mode() {
        let control = Control::default();
        let mut router = Router::default().with_keyboard(control.clone());
        let mut c = Config::default();
        c.buttons
            .iter_mut()
            .find(|b| b.button == ButtonId::Capture)
            .unwrap()
            .keyboard = Action::System {
            command: crate::studio::SystemAction::MissionControl,
        };
        c.buttons
            .iter_mut()
            .find(|b| b.button == ButtonId::Up)
            .unwrap()
            .air_mouse = Action::Scroll { x: 0, y: 3 };
        let t = Instant::now();
        visible(&control, &mut router, t);
        assert_eq!(
            router.process(&[3, 0x6c, 0], t, &c),
            vec![Output::SessionAction {
                button: ButtonId::Capture,
                generation: control.snapshot().generation,
            }]
        );
        router.process(&[2, 0, 1, 0, 0], t, &c);
        router.process(&key(0x52), t, &c);
        assert_eq!(actions(&control, t), vec![KeyboardAction::Up]);
        router.process(&key(0), t, &c);
        let g = control.toggle(None, t).unwrap();
        router.poll_keyboard();
        router.acknowledge_keyboard_fence();
        control.acknowledge_close(g);
        assert_eq!(
            router.process(&key(0x52), t, &c),
            vec![Output::Key {
                usage: 0x52,
                down: true
            }]
        );
    }
}
