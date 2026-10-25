use super::*;
use ready_for_air::{
    keyboard::{Command, Phase, Request},
    keyboard_model::{
        self, Key, KeyboardAction, KeyboardLanguage, KeyboardLayout, KeyboardPosition,
        KeyboardPreferences,
    },
    native_keyboard::{self, Target},
};

const TITLE: &str = "ACA keyboard";
struct PointerPress {
    id: u64,
    point: (f64, f64),
    key: Option<(usize, usize)>,
    origin: Point<Pixels>,
    header: bool,
    moved: bool,
}
struct MousePress {
    point: Point<Pixels>,
    key: Key,
    moved: bool,
}
impl MousePress {
    fn move_to(&mut self, point: Point<Pixels>) {
        self.moved |=
            f32::from(point.x - self.point.x).hypot(f32::from(point.y - self.point.y)) > 4.;
    }
    fn selects(mut self, key: Key, point: Point<Pixels>) -> bool {
        self.move_to(point);
        !self.moved && self.key == key
    }
}
pub(super) struct Panel {
    control: ready_for_air::keyboard::Control,
    generation: u64,
    target: Option<Target>,
    preview: bool,
    preferences: KeyboardPreferences,
    rows: Vec<Vec<Key>>,
    selected: (usize, usize),
    shift: bool,
    bounds: Bounds<Pixels>,
    drag: bool,
    mouse_down: Option<MousePress>,
    pointer: Option<PointerPress>,
}
impl Panel {
    fn key_bounds(&self, row: usize, col: usize) -> Bounds<Pixels> {
        let scale = self.preferences.scale;
        let width = (640.0 - 24.) / self.rows[row].len() as f32;
        Bounds::new(
            point(
                px((12. + col as f32 * width) * scale),
                px((42. + row as f32 * 48.) * scale),
            ),
            size(px((width - 5.) * scale), px(42. * scale)),
        )
    }
    fn hit(&self, p: (f64, f64)) -> Option<(usize, usize)> {
        let local = point(
            px(p.0 as f32) - self.bounds.origin.x,
            px(p.1 as f32) - self.bounds.origin.y,
        );
        for (r, row) in self.rows.iter().enumerate() {
            for c in 0..row.len() {
                if self.key_bounds(r, c).contains(&local) {
                    return Some((r, c));
                }
            }
        }
        None
    }
    fn activate(&mut self, key: Key, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview {
            return;
        }
        if !self.control.current(self.generation) {
            window.remove_window();
            return;
        }
        let Some(target) = &self.target else {
            return;
        };
        if !target.valid() {
            self.close(
                Some("Keyboard closed because the focused field changed".into()),
                window,
            );
            return;
        }
        if matches!(key, Key::Return | Key::Tab) {
            self.control.cancel(None);
        }
        let result = match key {
            Key::Shift => {
                self.shift = !self.shift;
                cx.notify();
                return;
            }
            Key::Letter(c) => {
                let text = if self.shift {
                    c.to_uppercase().collect::<String>()
                } else {
                    c.to_string()
                };
                let result = target.text(&text);
                if result.is_ok() {
                    self.shift = false;
                }
                result
            }
            Key::Space => target.text(" "),
            Key::Backspace => target.control(51),
            Key::Return => target.control(36),
            Key::Tab => target.control(48),
        };
        if let Err(error) = result {
            self.close(Some(error), window);
        } else if matches!(key, Key::Return | Key::Tab) {
            let reason = if key == Key::Tab {
                Some("Tab sent once. Keyboard closed because this target cannot attribute a safe field transition. Reopen in the next field.".into())
            } else {
                None
            };
            self.close(reason, window);
        }
        cx.notify();
    }
    fn close(&mut self, reason: Option<String>, window: &mut Window) {
        self.target = None;
        self.control.cancel(reason);
        window.remove_window();
    }
    fn command(&mut self, request: Request, window: &mut Window, cx: &mut Context<Self>) {
        if request.generation != self.generation || !self.control.current(self.generation) {
            return;
        }
        if let Command::Repeat { press, .. } = request.command {
            if !self.control.held(press) {
                return;
            }
        }
        match request.command {
            Command::Action { action, .. } | Command::Repeat { action, .. } => match action {
                KeyboardAction::Up => {
                    self.selected.0 = self.selected.0.saturating_sub(1);
                }
                KeyboardAction::Down => {
                    self.selected.0 = (self.selected.0 + 1).min(self.rows.len() - 1);
                }
                KeyboardAction::Left => {
                    self.selected.1 = self.selected.1.saturating_sub(1);
                }
                KeyboardAction::Right => {
                    self.selected.1 =
                        (self.selected.1 + 1).min(self.rows[self.selected.0].len() - 1);
                }
                KeyboardAction::Select => {
                    self.activate(self.rows[self.selected.0][self.selected.1], window, cx);
                }
                KeyboardAction::Delete => {
                    self.activate(Key::Backspace, window, cx);
                }
                KeyboardAction::Toggle => {}
            },
            Command::Pointer { press, down, point } => {
                if down {
                    let header = point.0 >= f64::from(self.bounds.origin.x)
                        && point.0 < f64::from(self.bounds.right())
                        && point.1 >= f64::from(self.bounds.origin.y)
                        && point.1
                            < f64::from(self.bounds.origin.y + px(38. * self.preferences.scale));
                    self.pointer = Some(PointerPress {
                        id: press,
                        point,
                        key: self.hit(point),
                        origin: self.bounds.origin,
                        header,
                        moved: false,
                    });
                } else if let Some(start) = self.pointer.take().filter(|p| p.id == press) {
                    if !start.header
                        && !start.moved
                        && (point.0 - start.point.0).hypot(point.1 - start.point.1) <= 4.
                    {
                        if let Some(key) = start.key.filter(|key| Some(*key) == self.hit(point)) {
                            self.activate(self.rows[key.0][key.1], window, cx);
                        }
                    }
                }
            }
            _ => {}
        }
        self.selected.1 = self.selected.1.min(self.rows[self.selected.0].len() - 1);
        cx.notify();
    }
    fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.bounds = window.bounds();
        if !self.preview
            && (!self.control.current(self.generation)
                || self.target.as_ref().is_none_or(|target| !target.valid()))
        {
            if self.control.current(self.generation) {
                self.control.cancel(Some(
                    "Keyboard closed because the destination is no longer verified".into(),
                ));
            }
            self.target = None;
            window.remove_window();
            return;
        }
        if let Some(press) = self.pointer.as_mut() {
            let point = native_keyboard::pointer_position();
            let dx = point.0 - press.point.0;
            let dy = point.1 - press.point.1;
            press.moved |= dx.hypot(dy) > 4.;
            if press.header && press.moved {
                native_keyboard::move_panel(
                    TITLE,
                    f64::from(press.origin.x) + dx,
                    f64::from(press.origin.y) + dy,
                );
                cx.notify();
            }
        }
    }
}
impl Render for Panel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.bounds = window.bounds();
        let mut children = Vec::new();
        for (r, row) in self.rows.iter().enumerate() {
            for (c, key) in row.iter().copied().enumerate() {
                let b = self.key_bounds(r, c);
                let label = match key {
                    Key::Letter(ch) => {
                        if self.shift {
                            ch.to_uppercase().collect()
                        } else {
                            ch.to_string()
                        }
                    }
                    Key::Space => "Space".into(),
                    Key::Backspace => "⌫".into(),
                    Key::Return => "Return".into(),
                    Key::Tab => "Tab".into(),
                    Key::Shift => "Shift".into(),
                };
                children.push(
                    div()
                        .id(("keyboard-key", r * 20 + c))
                        .absolute()
                        .left(b.origin.x)
                        .top(b.origin.y)
                        .w(b.size.width)
                        .h(b.size.height)
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(7.))
                        .bg(rgb(
                            if self.selected == (r, c) || key == Key::Shift && self.shift {
                                0x526987
                            } else {
                                PANEL
                            },
                        ))
                        .text_size(px(16. * self.preferences.scale))
                        .cursor_pointer()
                        .child(label)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |s, e: &MouseDownEvent, _, _| {
                                s.mouse_down = Some(MousePress {
                                    point: e.position,
                                    key,
                                    moved: false,
                                });
                            }),
                        )
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(move |s, e: &MouseUpEvent, w, cx| {
                                if s.mouse_down
                                    .take()
                                    .is_some_and(|p| p.selects(key, e.position))
                                {
                                    s.activate(key, w, cx);
                                }
                            }),
                        ),
                );
            }
        }
        div()
            .size_full()
            .relative()
            .on_mouse_move(cx.listener(|s, e: &MouseMoveEvent, _, _| {
                if let Some(press) = &mut s.mouse_down {
                    press.move_to(e.position);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|s, _, _, _| {
                    s.mouse_down = None;
                }),
            )
            .bg(rgba((BG << 8) | ((self.preferences.opacity * 255.) as u32)))
            .text_color(rgb(TEXT))
            .font_family(UI_FONT)
            .child(
                div()
                    .absolute()
                    .left(px(12.))
                    .top(px(6.))
                    .h(px(30. * self.preferences.scale))
                    .w_full()
                    .cursor_move()
                    .text_sm()
                    .child(if self.preview {
                        "Keyboard preview · does not type"
                    } else {
                        "ACA keyboard · drag to move"
                    })
                    .on_mouse_down(MouseButton::Left, cx.listener(|s, _, _, _| s.drag = true))
                    .on_mouse_move(cx.listener(|s, _, w, _| {
                        if s.drag {
                            s.drag = false;
                            w.start_window_move();
                        }
                    }))
                    .on_mouse_up(MouseButton::Left, cx.listener(|s, _, _, _| s.drag = false)),
            )
            .children(children)
    }
}
impl StudioApp {
    pub(super) fn keyboard_tick(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        if let Some((due, generation)) = self.keyboard_test_due {
            if self.studio.keyboard_control.snapshot().generation != generation {
                self.keyboard_test_due = None;
            } else if now >= due {
                self.keyboard_test_due = None;
                let _ = self.studio.keyboard_control.toggle(None, now);
            }
        }
        if let Some((handle, entity)) = self.keyboard_panel.as_ref() {
            let snapshot = self.studio.keyboard_control.snapshot();
            let stale = !entity.read(cx).preview
                && (entity.read(cx).generation != snapshot.generation
                    || snapshot.phase == Phase::Hidden);
            if stale {
                let position = entity.read(cx).bounds.origin;
                let moved =
                    entity.read(cx).preferences.position.is_none_or(|p| {
                        p.x != f32::from(position.x) || p.y != f32::from(position.y)
                    });
                let _ = handle.update(cx, |_, w, _| w.remove_window());
                self.keyboard_panel = None;
                if moved {
                    self.keyboard_saved_position = Some(KeyboardPosition {
                        x: f32::from(position.x),
                        y: f32::from(position.y),
                    });
                }
            }
        }
        if self.studio.keyboard_control.snapshot().phase == Phase::Hidden {
            if let Some(position) = self.keyboard_saved_position.take() {
                if let Err(error) = self.studio.save_keyboard_position(position) {
                    self.report("keyboard", Err(error));
                }
            }
        }
        for request in self.studio.keyboard_control.drain(now) {
            match request.command {
                Command::Open => self.keyboard_open = Some(request),
                Command::Close => {
                    self.dismiss_keyboard(cx);
                    self.studio
                        .keyboard_control
                        .acknowledge_close(request.generation);
                }
                _ => {
                    if let Some((handle, entity)) = &self.keyboard_panel {
                        let _ = handle.update(cx, |_, w, cx| {
                            entity.update(cx, |panel, cx| panel.command(request, w, cx))
                        });
                    }
                }
            }
        }
        if let Some(request) = self.keyboard_open {
            if now.saturating_duration_since(request.at) >= Duration::from_millis(500) {
                self.keyboard_open = None;
                self.studio
                    .keyboard_control
                    .cancel(Some("Keyboard opening timed out".into()));
            } else if self
                .studio
                .keyboard_control
                .ready_to_open(request.generation)
            {
                self.keyboard_open = None;
                match self.open_keyboard(false, request.generation, cx) {
                    Ok(()) => {}
                    Err(error) => self.studio.keyboard_control.cancel(Some(error)),
                }
            } else if self.studio.keyboard_control.snapshot().generation != request.generation {
                self.keyboard_open = None;
            }
        }
        if let Some((handle, entity)) = &self.keyboard_panel {
            let _ = handle.update(cx, |_, w, cx| {
                entity.update(cx, |panel, cx| panel.refresh(w, cx))
            });
        }
        if let Some(error) = self.studio.keyboard_control.take_error() {
            self.report("keyboard", Err(error));
            cx.notify();
        }
    }
    fn dismiss_keyboard(&mut self, cx: &mut Context<Self>) {
        if let Some((handle, _)) = self.keyboard_panel.take() {
            let _ = handle.update(cx, |_, w, _| w.remove_window());
        }
    }
    fn open_keyboard(
        &mut self,
        preview: bool,
        generation: u64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.dismiss_keyboard(cx);
        let target = if preview {
            None
        } else {
            Some(Target::capture()?)
        };
        let preferences = self.studio.config.keyboard.clone();
        let rows = keyboard_model::keys(&preferences);
        let dimensions = size(
            px(640. * preferences.scale),
            px((52. + rows.len() as f32 * 48.) * preferences.scale),
        );
        let centered = Bounds::centered(None, dimensions, cx);
        let origin = preferences
            .position
            .map(|p| point(px(p.x), px(p.y)))
            .filter(|p| {
                cx.displays().iter().any(|d| {
                    d.bounds().contains(p)
                        && d.bounds()
                            .contains(&point(p.x + dimensions.width, p.y + dimensions.height))
                })
            })
            .unwrap_or(centered.origin);
        let bounds = Bounds::new(origin, dimensions);
        let control = self.studio.keyboard_control.clone();
        let (handle, entity) = gpui_kit::open_window(
            WindowOptions {
                kind: WindowKind::PopUp,
                focus: false,
                show: true,
                titlebar: Some(TitlebarOptions {
                    title: Some(TITLE.into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_background: WindowBackgroundAppearance::Transparent,
                is_resizable: false,
                is_minimizable: false,
                app_owns_titlebar_drag: true,
                ..Default::default()
            },
            cx,
            |_, cx| {
                cx.new(|_| Panel {
                    control,
                    generation,
                    target,
                    preview,
                    preferences,
                    rows,
                    selected: (0, 0),
                    shift: false,
                    bounds,
                    drag: false,
                    mouse_down: None,
                    pointer: None,
                })
            },
        )
        .map_err(|e| e.to_string())?;
        if !native_keyboard::prepare_panel(TITLE)
            || (!preview && !self.studio.keyboard_control.acknowledge_open(generation))
        {
            let _ = handle.update(cx, |_, w, _| w.remove_window());
            return Err("Keyboard panel could not acknowledge its session".into());
        }
        self.keyboard_panel = Some((handle, entity));
        Ok(())
    }
    fn save_keyboard_preferences(&mut self, preferences: KeyboardPreferences) {
        self.keyboard_saved_position = None;
        self.keyboard_test_due = None;
        let mut next = self.studio.config.clone();
        next.keyboard = preferences;
        let result = self
            .studio
            .save_config(next)
            .map(|_| "Keyboard preferences saved".into());
        self.report("keyboard", result);
    }
    pub(super) fn render_keyboard_settings(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let p = &self.studio.config.keyboard;
        let layout = |label: &'static str, value: KeyboardLayout, cx: &mut Context<Self>| {
            Button::new(label)
                .label(label)
                .map(|b| {
                    if p.layout == value {
                        b.primary()
                    } else {
                        b.outline()
                    }
                })
                .on_click(cx.listener(move |s, _, _, cx| {
                    let mut p = s.studio.config.keyboard.clone();
                    p.layout = value;
                    s.save_keyboard_preferences(p);
                    s.dismiss_keyboard(cx);
                    cx.notify();
                }))
        };
        let language = |label: &'static str, value: KeyboardLanguage, cx: &mut Context<Self>| {
            Button::new(label)
                .label(label)
                .map(|b| {
                    if p.language == value {
                        b.primary()
                    } else {
                        b.outline()
                    }
                })
                .on_click(cx.listener(move |s, _, _, cx| {
                    let mut p = s.studio.config.keyboard.clone();
                    p.language = value;
                    s.save_keyboard_preferences(p);
                    s.dismiss_keyboard(cx);
                    cx.notify();
                }))
        };
        div().size_full().p_8().flex().flex_col().gap_4().child(div().text_2xl().child("Keyboard"))
            .child(caption("Compact input for searches. Assign Toggle keyboard to Home or Capture in Normal or Air mouse. The opener always closes the panel, even on OK or an arrow."))
            .child(div().flex().gap_2().child(layout("QWERTY",KeyboardLayout::Qwerty,cx)).child(layout("Alphabetical",KeyboardLayout::Alphabetical,cx)))
            .child(div().flex().gap_2().child(language("English",KeyboardLanguage::English,cx)).child(language("Spanish",KeyboardLanguage::Spanish,cx)))
            .child(div().flex().items_center().gap_3().child(format!("Size {:.0}%",p.scale*100.)).children([("Smaller",-0.1f32),("Larger",0.1)].into_iter().map(|(label,delta)|Button::new(label).label(label).outline().on_click(cx.listener(move|s,_,_,cx|{let mut p=s.studio.config.keyboard.clone();p.scale=(p.scale+delta).clamp(0.7,1.5);s.save_keyboard_preferences(p);s.dismiss_keyboard(cx);cx.notify();})))))
            .child(div().flex().items_center().gap_3().child(format!("Opacity {:.0}%",p.opacity*100.)).children([("More transparent",-0.1f32),("More opaque",0.1)].into_iter().map(|(label,delta)|Button::new(label).label(label).outline().on_click(cx.listener(move|s,_,_,cx|{let mut p=s.studio.config.keyboard.clone();p.opacity=(p.opacity+delta).clamp(0.4,1.);s.save_keyboard_preferences(p);s.dismiss_keyboard(cx);cx.notify();})))))
            .child(div().flex().gap_2()
                .child(Button::new("keyboard-preview").label("Toggle preview").primary().on_click(cx.listener(|s,_,_,cx|{if s.keyboard_panel.is_some(){s.dismiss_keyboard(cx);}else{let result=s.open_keyboard(true,0,cx).map(|_|"Preview does not type".into());s.report("keyboard",result);}cx.notify();})))
                .child(Button::new("keyboard-reset-position").label("Reset position").outline().on_click(cx.listener(|s,_,_,cx|{let mut p=s.studio.config.keyboard.clone();p.position=None;s.save_keyboard_preferences(p);s.dismiss_keyboard(cx);cx.notify();})))
                .child(Button::new("keyboard-reset-preferences").label("Reset keyboard preferences").outline().on_click(cx.listener(|s,_,_,cx|{s.save_keyboard_preferences(Default::default());s.dismiss_keyboard(cx);cx.notify();}))))
            .child(Button::new("keyboard-test-target").label("Test in another app in 5 seconds").outline().on_click(cx.listener(|s,_,_,cx| {
                s.dismiss_keyboard(cx);
                s.studio.keyboard_control.cancel(None);
                s.keyboard_test_due=Some((Instant::now()+Duration::from_secs(5),s.studio.keyboard_control.snapshot().generation));
                s.report("keyboard",Ok("Focus a disposable text field in another app. The test keyboard opens in 5 seconds and can type there.".into()));cx.notify();
            })))
            .child(caption("Return sends once and closes. Tab sends once and closes until safe field-transition attribution is available. Changing the target closes the keyboard. Accessibility is required for typing. Preview never inserts text."))
            .child(caption(self.message("keyboard").map(|m|m.0.clone()).unwrap_or_default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn dragging_away_and_back_does_not_select_a_key() {
        let start = point(px(20.), px(30.));
        let press = || MousePress {
            point: start,
            key: Key::Letter('a'),
            moved: false,
        };
        assert!(press().selects(Key::Letter('a'), start));
        assert!(!press().selects(Key::Letter('b'), start));
        let mut dragged = press();
        dragged.move_to(point(px(40.), px(30.)));
        dragged.move_to(start);
        assert!(!dragged.selects(Key::Letter('a'), start));
    }
}
