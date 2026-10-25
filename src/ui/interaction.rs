//! Pointer and keyboard decisions for the stage and the disconnect
//! confirmation, kept apart from the GPUI listeners so they can be tested.

use gpui_kit::{Pixels, Point};

/// Pointer travel (Manhattan, px) that turns a press into a drag.
pub const DRAG_THRESHOLD: f32 = 4.;

/// Mouse press on the 3D stage (window coordinates).
pub struct Drag {
    start: Point<Pixels>,
    last: Point<Pixels>,
    moved: bool,
}

impl Drag {
    pub fn new(at: Point<Pixels>) -> Self {
        Self {
            start: at,
            last: at,
            moved: false,
        }
    }

    pub fn moved(&self) -> bool {
        self.moved
    }

    /// Records pointer movement; returns the delta to rotate by once the press
    /// has become a drag.
    pub fn update(&mut self, at: Point<Pixels>) -> Option<Point<Pixels>> {
        let delta = at - self.last;
        let total = at - self.start;
        self.last = at;
        if !self.moved && f32::from(total.x).abs() + f32::from(total.y).abs() > DRAG_THRESHOLD {
            self.moved = true;
        }
        self.moved.then_some(delta)
    }
}

/// Release evidence survives until the next press, because GPUI can dispatch
/// the stage's mouse-up and a child's click in either order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Release {
    Click,
    Dragged,
    #[default]
    Canceled,
}

#[derive(Default)]
pub struct StageGesture {
    drag: Option<Drag>,
    released: Release,
}
impl StageGesture {
    pub fn begin(&mut self, at: Point<Pixels>) {
        self.drag = Some(Drag::new(at));
        self.released = Release::Canceled;
    }
    pub fn motion(&mut self, at: Point<Pixels>) -> Option<Point<Pixels>> {
        self.drag.as_mut().and_then(|drag| drag.update(at))
    }
    pub fn active(&self) -> bool {
        self.drag.is_some()
    }
    pub fn moved(&self) -> bool {
        self.drag.as_ref().is_some_and(Drag::moved)
    }
    pub fn release(&mut self) -> Release {
        if let Some(drag) = self.drag.take() {
            self.released = if drag.moved() {
                Release::Dragged
            } else {
                Release::Click
            };
            self.released
        } else {
            Release::Canceled
        }
    }
    pub fn label_selects(&self) -> bool {
        self.drag
            .as_ref()
            .map_or(self.released == Release::Click, |drag| !drag.moved())
    }
    pub fn cancel(&mut self) {
        self.drag = None;
        self.released = Release::Canceled;
    }
}

/// Choice highlighted in the disconnect confirmation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Choice {
    /// The safe default: Enter right after opening does not disconnect.
    #[default]
    Cancel,
    Disconnect,
}

/// Keyboard input while the confirmation is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalKey {
    Next,
    Previous,
    Activate,
    Dismiss,
    /// Any other key, including text. It must not reach the window behind.
    Other,
}

impl ModalKey {
    pub fn from_keystroke(key: &str, shift: bool) -> Self {
        match key {
            "tab" if shift => Self::Previous,
            "tab" | "right" => Self::Next,
            "left" => Self::Previous,
            "enter" | "space" => Self::Activate,
            "escape" => Self::Dismiss,
            _ => Self::Other,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Keep the confirmation open; the key is consumed.
    Stay,
    Cancel,
    Disconnect,
}

/// Applies a key to the confirmation. Focus cycles between its two choices
/// and every key is consumed, so nothing behind it can be typed into or
/// activated while it is open.
pub fn confirmation_key(choice: &mut Choice, key: ModalKey) -> Outcome {
    match key {
        ModalKey::Next | ModalKey::Previous => {
            *choice = match choice {
                Choice::Cancel => Choice::Disconnect,
                Choice::Disconnect => Choice::Cancel,
            };
            Outcome::Stay
        }
        ModalKey::Activate => match choice {
            Choice::Cancel => Outcome::Cancel,
            Choice::Disconnect => Outcome::Disconnect,
        },
        ModalKey::Dismiss => Outcome::Cancel,
        ModalKey::Other => Outcome::Stay,
    }
}

/// Owns the confirmation's actual GPUI focus targets. The host renders the
/// two choices with these handles and applies the returned outcome.
pub struct Confirmation {
    open: bool,
    choice: Choice,
    container: gpui_kit::FocusHandle,
    cancel: gpui_kit::FocusHandle,
    disconnect: gpui_kit::FocusHandle,
    previous: Option<gpui_kit::FocusHandle>,
}
impl Confirmation {
    pub fn new(cx: &mut gpui_kit::App) -> Self {
        Self {
            open: false,
            choice: Choice::Cancel,
            container: cx.focus_handle(),
            // With track_focus, GPUI uses the handle's tab flags; element-level
            // tab_stop alone does not add these explicit handles to traversal.
            cancel: cx.focus_handle().tab_index(0).tab_stop(true),
            disconnect: cx.focus_handle().tab_index(0).tab_stop(true),
            previous: None,
        }
    }
    pub fn is_open(&self) -> bool {
        self.open
    }
    pub fn container(&self) -> &gpui_kit::FocusHandle {
        &self.container
    }
    pub fn focus(&self, choice: Choice) -> &gpui_kit::FocusHandle {
        match choice {
            Choice::Cancel => &self.cancel,
            Choice::Disconnect => &self.disconnect,
        }
    }
    pub fn open(&mut self, window: &mut gpui_kit::Window, cx: &mut gpui_kit::App) {
        if self.open {
            return;
        }
        self.previous = window.focused(cx);
        self.open = true;
        self.choice = Choice::Cancel;
        window.focus(&self.cancel, cx);
    }
    pub fn dismiss(
        &mut self,
        fallback: &gpui_kit::FocusHandle,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::App,
    ) {
        self.open = false;
        window.focus(
            &self.previous.take().unwrap_or_else(|| fallback.clone()),
            cx,
        );
    }
    pub fn key_down(
        &mut self,
        event: &gpui_kit::KeyDownEvent,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::App,
    ) -> Option<Outcome> {
        if !self.open {
            return None;
        }
        // Capture phase: prevent both descendant activation and native text input.
        window.prevent_default();
        cx.stop_propagation();
        let k = &event.keystroke;
        let m = k.modifiers;
        let key = if event.is_held || m.platform || m.control || m.alt || m.function {
            ModalKey::Other
        } else {
            ModalKey::from_keystroke(&k.key, m.shift)
        };
        // Accessibility or native focus traversal may have focused a choice.
        if self.disconnect.is_focused(window) {
            self.choice = Choice::Disconnect;
        } else if self.cancel.is_focused(window) {
            self.choice = Choice::Cancel;
        }
        let outcome = confirmation_key(&mut self.choice, key);
        if outcome == Outcome::Stay {
            window.focus(self.focus(self.choice), cx);
        }
        Some(outcome)
    }
    pub fn key_up(&self, window: &mut gpui_kit::Window, cx: &mut gpui_kit::App) {
        if self.open {
            window.prevent_default();
            cx.stop_propagation();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{point, px};

    fn at(x: f32, y: f32) -> Point<Pixels> {
        point(px(x), px(y))
    }

    #[::core::prelude::v1::test]
    fn release_order_cannot_turn_a_label_drag_into_a_click() {
        for label_first in [false, true] {
            let mut gesture = StageGesture::default();
            gesture.begin(at(100., 100.));
            gesture.motion(at(108., 100.));
            if label_first {
                assert!(!gesture.label_selects());
            }
            assert_eq!(gesture.release(), Release::Dragged);
            assert!(
                !gesture.label_selects(),
                "a later label callback must see the drag"
            );
            // The next genuine click must not inherit the suppression.
            gesture.begin(at(110., 110.));
            assert_eq!(gesture.release(), Release::Click);
            assert!(gesture.label_selects());
        }
    }

    #[::core::prelude::v1::test]
    fn release_without_a_new_stage_press_cannot_reuse_an_old_click() {
        let mut gesture = StageGesture::default();
        gesture.begin(at(100., 100.));
        assert_eq!(gesture.release(), Release::Click);
        assert_eq!(
            gesture.release(),
            Release::Canceled,
            "a release from outside the stage must not select"
        );
        assert!(
            gesture.label_selects(),
            "release evidence still serves the label callback"
        );
    }

    #[::core::prelude::v1::test]
    fn controls_cancel_stage_gestures_and_never_select_the_model() {
        let mut gesture = StageGesture::default();
        gesture.begin(at(100., 100.));
        gesture.cancel();
        assert_eq!(gesture.motion(at(130., 130.)), None);
        assert_eq!(gesture.release(), Release::Canceled);
        assert!(!gesture.label_selects());
        assert!(!gesture.active());
    }

    // gpui_kit's glob import shadows the std `test` attribute.
    #[::core::prelude::v1::test]
    fn dragging_from_a_label_rotates_without_selecting() {
        let mut gesture = StageGesture::default();
        gesture.begin(at(100., 100.));
        assert_eq!(gesture.motion(at(102., 101.)), None, "jitter is not a drag");
        assert!(
            gesture.motion(at(110., 104.)).is_some(),
            "past the threshold it rotates"
        );
        assert!(!gesture.label_selects());
        assert_eq!(gesture.release(), Release::Dragged);
    }

    #[::core::prelude::v1::test]
    fn plain_clicks_still_select() {
        let mut gesture = StageGesture::default();
        assert_eq!(
            gesture.release(),
            Release::Canceled,
            "release without a press does not select"
        );
        gesture.begin(at(100., 100.));
        gesture.motion(at(101., 102.));
        assert!(gesture.label_selects());
        assert_eq!(gesture.release(), Release::Click);
    }

    #[::core::prelude::v1::test]
    fn confirmation_consumes_every_key_and_defaults_to_cancel() {
        let mut choice = Choice::default();
        for key in ["a", "1", "backspace", "up", "down", "cmd-v"] {
            assert_eq!(
                confirmation_key(&mut choice, ModalKey::from_keystroke(key, false)),
                Outcome::Stay,
                "{key} escaped the confirmation"
            );
        }
        assert_eq!(choice, Choice::Cancel);
        // Enter right away keeps the remote connected.
        assert_eq!(
            confirmation_key(&mut choice, ModalKey::Activate),
            Outcome::Cancel
        );
    }

    #[::core::prelude::v1::test]
    fn tab_moves_between_the_two_choices_only() {
        let mut choice = Choice::default();
        let key = |k, shift| ModalKey::from_keystroke(k, shift);
        assert_eq!(
            confirmation_key(&mut choice, key("tab", false)),
            Outcome::Stay
        );
        assert_eq!(choice, Choice::Disconnect);
        assert_eq!(
            confirmation_key(&mut choice, key("tab", false)),
            Outcome::Stay
        );
        assert_eq!(choice, Choice::Cancel, "focus wraps instead of leaving");
        assert_eq!(
            confirmation_key(&mut choice, key("tab", true)),
            Outcome::Stay
        );
        assert_eq!(choice, Choice::Disconnect);
        assert_eq!(
            confirmation_key(&mut choice, key("space", false)),
            Outcome::Disconnect
        );
        let mut choice = Choice::Disconnect;
        assert_eq!(
            confirmation_key(&mut choice, key("escape", false)),
            Outcome::Cancel
        );
    }
}
