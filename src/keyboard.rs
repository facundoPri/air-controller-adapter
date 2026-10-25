use crate::{keyboard_model::KeyboardAction, studio::ButtonId};
use std::{
    collections::{BTreeSet, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const CAPACITY: usize = 64;
const EXPIRY: Duration = Duration::from_millis(500);
pub const REPEAT_DELAY: Duration = Duration::from_millis(400);
pub const REPEAT_INTERVAL: Duration = Duration::from_millis(70);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Hidden,
    Opening,
    Visible,
    Closing,
}
#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
    pub phase: Phase,
    pub generation: u64,
    pub opener: Option<ButtonId>,
}
#[derive(Clone, Copy, Debug)]
pub enum Command {
    Open,
    Close,
    Action {
        action: KeyboardAction,
        press: u64,
    },
    Repeat {
        action: KeyboardAction,
        press: u64,
    },
    Pointer {
        press: u64,
        down: bool,
        point: (f64, f64),
    },
}
#[derive(Clone, Copy, Debug)]
pub struct Request {
    pub generation: u64,
    pub command: Command,
    pub at: Instant,
}
struct State {
    snapshot: Snapshot,
    queue: VecDeque<Request>,
    held: BTreeSet<u64>,
    repeat: Option<(u64, KeyboardAction, Instant)>,
    router_online: bool,
    fenced: u64,
    error: Option<String>,
}
#[derive(Clone)]
pub struct Control(Arc<Mutex<State>>);
impl Default for Control {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(State {
            snapshot: Snapshot {
                phase: Phase::Hidden,
                generation: 0,
                opener: None,
            },
            queue: VecDeque::new(),
            held: BTreeSet::new(),
            repeat: None,
            router_online: false,
            fenced: 0,
            error: None,
        })))
    }
}
impl Control {
    pub fn snapshot(&self) -> Snapshot {
        self.0.lock().unwrap().snapshot
    }
    pub fn set_router_online(&self, online: bool) {
        let mut s = self.0.lock().unwrap();
        s.router_online = online;
        if !online {
            Self::cancel_locked(&mut s, None);
        }
    }
    pub fn acknowledge_fence(&self, generation: u64) {
        let mut s = self.0.lock().unwrap();
        if s.snapshot.generation == generation {
            s.fenced = generation;
        }
    }
    pub fn ready_to_open(&self, generation: u64) -> bool {
        let s = self.0.lock().unwrap();
        s.snapshot.generation == generation
            && s.snapshot.phase == Phase::Opening
            && (!s.router_online || s.fenced == generation)
    }
    pub fn toggle(&self, opener: Option<ButtonId>, at: Instant) -> Result<u64, String> {
        let mut s = self.0.lock().unwrap();
        Ok(Self::toggle_locked(&mut s, opener, at))
    }
    pub fn toggle_for(
        &self,
        generation: u64,
        opener: Option<ButtonId>,
        at: Instant,
    ) -> Result<u64, String> {
        let mut s = self.0.lock().unwrap();
        if s.snapshot.generation != generation {
            return Err("Keyboard session changed before toggle".into());
        }
        Ok(Self::toggle_locked(&mut s, opener, at))
    }
    fn toggle_locked(s: &mut State, opener: Option<ButtonId>, at: Instant) -> u64 {
        let opening = s.snapshot.phase == Phase::Hidden;
        s.snapshot.generation = s.snapshot.generation.wrapping_add(1);
        s.snapshot.phase = if opening {
            Phase::Opening
        } else {
            Phase::Closing
        };
        s.snapshot.opener = if opening { opener } else { None };
        s.queue.clear();
        s.repeat = None;
        s.held.clear();
        s.error = None;
        let generation = s.snapshot.generation;
        s.queue.push_back(Request {
            generation,
            command: if opening {
                Command::Open
            } else {
                Command::Close
            },
            at,
        });
        generation
    }
    pub fn acknowledge_open(&self, generation: u64) -> bool {
        let mut s = self.0.lock().unwrap();
        if s.snapshot.generation != generation
            || s.snapshot.phase != Phase::Opening
            || (s.router_online && s.fenced != generation)
        {
            return false;
        }
        s.snapshot.phase = Phase::Visible;
        true
    }
    pub fn acknowledge_close(&self, generation: u64) {
        let mut s = self.0.lock().unwrap();
        if s.snapshot.generation == generation && s.snapshot.phase == Phase::Closing {
            s.snapshot.phase = Phase::Hidden;
        }
    }
    pub fn cancel(&self, reason: Option<String>) {
        Self::cancel_locked(&mut self.0.lock().unwrap(), reason);
    }
    fn cancel_locked(s: &mut State, reason: Option<String>) {
        s.snapshot.generation = s.snapshot.generation.wrapping_add(1);
        s.snapshot.phase = Phase::Hidden;
        s.snapshot.opener = None;
        s.queue.clear();
        s.held.clear();
        s.repeat = None;
        s.error = reason;
    }
    pub fn take_error(&self) -> Option<String> {
        self.0.lock().unwrap().error.take()
    }
    pub fn press_for(&self, generation: u64, action: KeyboardAction, press: u64, at: Instant) {
        let mut s = self.0.lock().unwrap();
        if s.snapshot.generation != generation
            || s.snapshot.phase != Phase::Visible
            || !s.held.insert(press)
        {
            return;
        }
        if matches!(
            action,
            KeyboardAction::Up
                | KeyboardAction::Down
                | KeyboardAction::Left
                | KeyboardAction::Right
                | KeyboardAction::Delete
        ) {
            s.repeat = Some((press, action, at + REPEAT_DELAY));
        }
        Self::enqueue(&mut s, Command::Action { action, press }, at);
    }
    pub fn pointer_for(
        &self,
        generation: u64,
        press: u64,
        down: bool,
        point: (f64, f64),
        at: Instant,
    ) {
        let mut s = self.0.lock().unwrap();
        if s.snapshot.phase == Phase::Visible && s.snapshot.generation == generation {
            Self::enqueue(&mut s, Command::Pointer { press, down, point }, at);
        }
    }
    pub fn held(&self, press: u64) -> bool {
        self.0.lock().unwrap().held.contains(&press)
    }
    pub fn release(&self, press: u64) {
        let mut s = self.0.lock().unwrap();
        s.held.remove(&press);
        if s.repeat.is_some_and(|(p, _, _)| p == press) {
            s.repeat = None;
        }
    }
    fn enqueue(s: &mut State, command: Command, at: Instant) {
        if s.queue.len() >= CAPACITY {
            Self::cancel_locked(s, Some("Keyboard queue overflow; session canceled".into()));
            return;
        }
        s.queue.push_back(Request {
            generation: s.snapshot.generation,
            command,
            at,
        });
    }
    pub fn drain(&self, now: Instant) -> Vec<Request> {
        let mut s = self.0.lock().unwrap();
        if s.queue
            .front()
            .is_some_and(|r| now.saturating_duration_since(r.at) >= EXPIRY)
        {
            Self::cancel_locked(
                &mut s,
                Some("Keyboard command expired; session canceled".into()),
            );
            return vec![];
        }
        if let Some((press, action, due)) = s.repeat {
            if now >= due && s.snapshot.phase == Phase::Visible && s.held.contains(&press) {
                Self::enqueue(&mut s, Command::Repeat { action, press }, now);
                s.repeat = Some((press, action, now + REPEAT_INTERVAL));
            }
        }
        s.queue.drain(..).collect()
    }
    pub fn current(&self, generation: u64) -> bool {
        let s = self.snapshot();
        s.generation == generation && s.phase == Phase::Visible
    }
    pub fn wait(
        &self,
        generation: u64,
        deadline: Instant,
        canceled: impl Fn() -> bool,
    ) -> Result<(), String> {
        loop {
            if canceled() || Instant::now() >= deadline {
                if self.snapshot().generation == generation {
                    self.cancel(Some("Keyboard step canceled or timed out".into()));
                }
                return Err("Keyboard step canceled or timed out".into());
            }
            let s = self.snapshot();
            if s.generation != generation {
                return Err("Keyboard session changed during macro execution".into());
            }
            match s.phase {
                Phase::Visible | Phase::Hidden => return Ok(()),
                _ => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn visible(c: &Control, t: Instant) -> u64 {
        let g = c.toggle(Some(ButtonId::Home), t).unwrap();
        c.drain(t);
        assert!(c.acknowledge_open(g));
        g
    }
    #[test]
    fn opening_waits_for_router_fence_and_stale_ack_is_inert() {
        let c = Control::default();
        c.set_router_online(true);
        let g = c.toggle(None, Instant::now()).unwrap();
        assert!(!c.acknowledge_open(g));
        c.acknowledge_fence(g);
        assert!(c.acknowledge_open(g));
        c.cancel(None);
        assert!(!c.acknowledge_open(g));
    }
    #[test]
    fn repeat_waits_then_stops_on_release_without_catchup() {
        let c = Control::default();
        let t = Instant::now();
        let g = visible(&c, t);
        c.press_for(g, KeyboardAction::Delete, 1, t);
        assert_eq!(c.drain(t).len(), 1);
        assert!(
            c.drain(t + REPEAT_DELAY - Duration::from_millis(1))
                .is_empty()
        );
        assert_eq!(c.drain(t + REPEAT_DELAY).len(), 1);
        c.release(1);
        assert!(c.drain(t + Duration::from_secs(2)).is_empty());
    }
    #[test]
    fn select_never_repeats_and_distinct_rapid_presses_survive() {
        let c = Control::default();
        let t = Instant::now();
        let g = visible(&c, t);
        c.press_for(g, KeyboardAction::Select, 1, t);
        c.press_for(g, KeyboardAction::Select, 1, t);
        c.release(1);
        c.press_for(g, KeyboardAction::Select, 2, t);
        assert_eq!(c.drain(t).len(), 2);
        assert!(c.drain(t + Duration::from_secs(1)).is_empty());
    }
    #[test]
    fn overflow_and_expiry_revoke_session() {
        let c = Control::default();
        let t = Instant::now();
        let g = visible(&c, t);
        for p in 0..65 {
            c.press_for(g, KeyboardAction::Select, p, t);
        }
        assert_eq!(c.snapshot().phase, Phase::Hidden);
        assert!(c.drain(t).is_empty());
        let g = visible(&c, t);
        c.press_for(g, KeyboardAction::Delete, 2, t);
        assert!(c.drain(t + EXPIRY).is_empty());
        assert_eq!(c.snapshot().phase, Phase::Hidden);
    }
    #[test]
    fn stale_press_cannot_enter_reopened_session_or_restart_repeat() {
        let c = Control::default();
        let t = Instant::now();
        let old = visible(&c, t);
        c.press_for(old, KeyboardAction::Delete, 1, t);
        assert_eq!(c.drain(t).len(), 1);
        c.cancel(None);
        let current = visible(&c, t);

        c.press_for(old, KeyboardAction::Delete, 2, t);
        assert!(c.drain(t).is_empty());
        assert!(!c.held(1));
        assert!(!c.held(2));
        assert!(c.drain(t + REPEAT_DELAY).is_empty());

        c.press_for(current, KeyboardAction::Delete, 2, t + REPEAT_DELAY);
        let requests = c.drain(t + REPEAT_DELAY);
        assert!(matches!(
            requests.as_slice(),
            [Request { generation, command: Command::Action { action: KeyboardAction::Delete, press: 2 }, .. }]
                if *generation == current
        ));
        c.cancel(None);
        assert!(c.drain(t + REPEAT_DELAY + REPEAT_DELAY).is_empty());
    }
    #[test]
    fn stale_toggle_cannot_reopen_canceled_session() {
        let c = Control::default();
        let t = Instant::now();
        let old = visible(&c, t);
        c.cancel(None);
        let canceled = c.snapshot().generation;

        assert!(c.toggle_for(old, Some(ButtonId::Home), t).is_err());
        assert_eq!(c.snapshot().generation, canceled);
        assert_eq!(c.snapshot().phase, Phase::Hidden);
        assert!(c.drain(t).is_empty());

        let current = c.toggle_for(canceled, Some(ButtonId::Home), t).unwrap();
        assert!(c.acknowledge_open(current));
        assert!(matches!(
            c.drain(t).as_slice(),
            [Request {
                command: Command::Open,
                ..
            }]
        ));
    }
}
