//! ACA Studio window: shell, shared state and the 50 ms engine poll.
//!
//! Views live in `src/ui/*_view.rs`; style tokens and English labels in
//! `src/ui/theme.rs`. The 3D remote is painted by `src/remote_scene.rs`.

#[path = "remote_scene.rs"]
mod remote_scene;

mod activity_view;
mod buttons_view;
mod connection_view;
mod theme;

use gpui_kit::{
    base::Disableable,
    component::{
        Icon, IconName, Sizable,
        button::{Button, ButtonVariants},
        input::TextareaState,
    },
    prelude::FluentBuilder,
    *,
};
use ready_for_air::studio::{ButtonId, MouseMode, Profile, Studio};
use remote_scene::Camera;
use std::{
    cell::Cell,
    collections::HashMap,
    hash::{Hash, Hasher},
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};
use theme::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Buttons,
    Activity,
    Connection,
}

/// Mouse drag on the 3D scene (window coordinates).
struct Drag {
    start: Point<Pixels>,
    last: Point<Pixels>,
    moved: bool,
}

pub struct StudioApp {
    studio: Studio,
    view: View,
    selected: ButtonId,
    profile: Profile,
    camera: Camera,
    hovered: Option<usize>,
    /// Button observed from hardware, glowing until the instant expires.
    pressed: Option<(usize, Instant)>,
    last_seen: Option<ButtonId>,
    drag: Option<Drag>,
    /// Scene container bounds, written during prepaint by a measuring canvas.
    canvas_size: Rc<Cell<Size<Pixels>>>,
    canvas_origin: Rc<Cell<Point<Pixels>>>,
    known_size: Size<Pixels>,
    focus: FocusHandle,
    focused_once: bool,
    action_editor: Option<Entity<TextareaState>>,
    action_editor_key: String,
    drafts: ready_for_air::drafts::Drafts,
    config_editor: Option<Entity<TextareaState>>,
    config_editor_key: String,
    /// Inline results per area ("action", "json", "rules", "logs", ...): (text, is_error).
    messages: HashMap<&'static str, (String, bool)>,
    toast: Option<(String, bool, Instant)>,
    fingerprint: u64,
    log_scroll: ScrollHandle,
    log_marker: (usize, String),
}

impl StudioApp {
    pub fn new(root: PathBuf, cx: &mut Context<Self>) -> Self {
        let studio = Studio::new(root);
        cx.spawn(async move |view, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                if view.update(cx, |this, cx| this.tick(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        Self {
            fingerprint: fingerprint(&studio),
            studio,
            view: View::Buttons,
            selected: ButtonId::Home,
            profile: Profile::Normal,
            camera: Camera::default(),
            hovered: None,
            pressed: None,
            last_seen: None,
            drag: None,
            canvas_size: Rc::new(Cell::new(Size::default())),
            canvas_origin: Rc::new(Cell::new(Point::default())),
            known_size: Size::default(),
            focus: cx.focus_handle(),
            focused_once: false,
            action_editor: None,
            action_editor_key: String::new(),
            drafts: Default::default(),
            config_editor: None,
            config_editor_key: String::new(),
            messages: HashMap::new(),
            toast: None,
            log_scroll: ScrollHandle::new(),
            log_marker: (0, String::new()),
        }
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        self.studio.poll();
        let mut changed = false;
        if let Some(result) = self.studio.learn_result.take() {
            self.report("action", result);
            changed = true;
        }
        if self
            .messages
            .get("rules")
            .is_some_and(|(text, _)| text == "Starting native actions…")
        {
            if self.studio.native_active {
                self.messages
                    .insert("rules", ("Native actions active".into(), false));
                changed = true;
            } else if !self.studio.actions_enabled {
                self.messages
                    .insert("rules", (self.studio.status.clone(), true));
                changed = true;
            }
        }
        if !self.studio.actions_enabled
            && self
                .messages
                .get("rules")
                .is_some_and(|(text, _)| text == "Native actions active")
        {
            self.messages.remove("rules");
            changed = true;
        }
        // `last_button` is consumed so repeated presses of the same button glow again.
        if let Some(b) = self.studio.last_button.take() {
            self.pressed = Some((index_of(b), Instant::now()));
            self.last_seen = Some(b);
            changed = true;
        }
        if self
            .pressed
            .is_some_and(|(_, t)| t.elapsed() > Duration::from_millis(600))
        {
            self.pressed = None;
            changed = true;
        }
        if self
            .toast
            .as_ref()
            .is_some_and(|(_, _, t)| t.elapsed() > Duration::from_secs(8))
        {
            self.toast = None;
            changed = true;
        }
        let size = self.canvas_size.get();
        if size != self.known_size {
            self.known_size = size;
            changed = true;
        }
        let fp = fingerprint(&self.studio);
        if fp != self.fingerprint {
            self.fingerprint = fp;
            changed = true;
        }
        let marker = (
            self.studio.logs.len(),
            self.studio
                .logs
                .last()
                .map(|l| format!("{}{}", l.time, l.message))
                .unwrap_or_default(),
        );
        if marker != self.log_marker {
            self.log_marker = marker;
            if self.view == View::Activity {
                self.log_scroll.scroll_to_bottom();
            }
        }
        if changed {
            cx.notify();
        }
    }

    /// Records an inline result for `area` and mirrors it in the shell toast line.
    fn report(&mut self, area: &'static str, result: Result<String, String>) {
        let (text, error) = match result {
            Ok(t) => (t, false),
            Err(e) => (e, true),
        };
        self.toast = Some((text.clone(), error, Instant::now()));
        self.messages.insert(area, (text, error));
    }

    fn message(&self, area: &str) -> Option<&(String, bool)> {
        self.messages.get(area)
    }

    fn select(&mut self, b: ButtonId) {
        if self.selected != b {
            self.selected = b;
            self.messages.remove("json");
            self.messages.remove("action");
        }
    }

    fn step_selection(&mut self, delta: isize) {
        let n = ButtonId::ALL.len() as isize;
        let i = (index_of(self.selected) as isize + delta).rem_euclid(n) as usize;
        self.select(ButtonId::ALL[i]);
    }

    fn rotate(&mut self, d_yaw: f32, d_pitch: f32) {
        self.camera = self.camera.rotated(d_yaw, d_pitch);
    }

    fn reveal_config(&mut self) {
        let path = self.studio.config_path();
        let mut command = std::process::Command::new("open");
        if path.exists() {
            command.arg("-R").arg(&path);
        } else {
            command.arg(path.parent().unwrap_or(&path));
        }
        let result = command
            .spawn()
            .map(|_| {
                if path.exists() {
                    format!("Revealed {}", path.display())
                } else {
                    format!(
                        "{} does not exist yet (save once to create it); opened its folder",
                        path.display()
                    )
                }
            })
            .map_err(|e| format!("Could not open Finder: {e}"));
        self.report("config", result);
    }

    fn editor_focused(&self, window: &Window, cx: &App) -> bool {
        [&self.action_editor, &self.config_editor]
            .into_iter()
            .flatten()
            .any(|e| e.read(cx).focus_handle(cx).contains_focused(window, cx))
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor_focused(window, cx) {
            return;
        }
        let ks = &ev.keystroke;
        let alt = ks.modifiers.alt;
        let step = 0.15;
        match ks.key.as_str() {
            "escape" if self.studio.learning.is_some() => {
                self.studio.cancel_learning();
                self.report("action", Ok("Learning cancelled".into()));
            }
            _ if self.view != View::Buttons => return,
            "[" => self.rotate(-step, 0.),
            "]" => self.rotate(step, 0.),
            "left" if alt => self.rotate(-step, 0.),
            "right" if alt => self.rotate(step, 0.),
            "up" if alt => self.rotate(0., -step),
            "down" if alt => self.rotate(0., step),
            "up" => self.step_selection(-1),
            "down" => self.step_selection(1),
            "0" => self.camera = Camera::default(),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// Creates the JSON editors lazily (they need a `Window`) and refreshes their
    /// text whenever the underlying action/config changes.
    fn sync_editors(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let action = self.studio.action(self.selected, self.profile);
        let action_json = serde_json::to_string_pretty(action).unwrap_or_default();
        let key = format!("{}/{}", self.selected.id(), self.profile.id());
        let editor = self
            .action_editor
            .get_or_insert_with(|| cx.new(|cx| TextareaState::new(window, cx)))
            .clone();
        let current = editor.read(cx).value().to_string();
        let text = self.drafts.sync(
            &key,
            &action_json,
            (!self.action_editor_key.is_empty())
                .then_some((self.action_editor_key.as_str(), current.as_str())),
        );
        self.action_editor_key = key;
        if current != text {
            editor.update(cx, |s, cx| s.set_value(text, window, cx));
        }
        let config_json = serde_json::to_string_pretty(&self.studio.config).unwrap_or_default();
        let editor = self
            .config_editor
            .get_or_insert_with(|| cx.new(|cx| TextareaState::new(window, cx)))
            .clone();
        let current = editor.read(cx).value().to_string();
        let text = self.drafts.sync(
            "config",
            &config_json,
            (!self.config_editor_key.is_empty()).then_some(("config", current.as_str())),
        );
        self.config_editor_key = config_json;
        if current != text {
            editor.update(cx, |s, cx| s.set_value(text, window, cx));
        }
    }

    fn engine_chip(&self) -> (String, u32) {
        let s = &self.studio;
        let link = match (s.connecting, s.connected, s.listening) {
            (true, _, _) => "CONNECTING…",
            (_, true, true) => "CONNECTED · LISTENING",
            (_, false, true) => "LISTENING",
            (_, true, false) => "CONNECTED · NOT LISTENING",
            _ => "NOT CONNECTED",
        };
        let color = if s.listening {
            OK
        } else if s.connecting || s.connected {
            WARN
        } else {
            FAINT
        };
        let mode = if s.listening {
            mode_name(s.mode).to_uppercase()
        } else {
            "INACTIVE".into()
        };
        (format!("{link}  ·  {mode}"), color)
    }

    fn nav_item(
        &self,
        view: View,
        icon: IconName,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let selected = self.view == view;
        Button::new(label)
            .label(label)
            .icon(icon)
            .ghost()
            .h(px(42.))
            .when(selected, |b| b.primary())
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.view = view;
                if view == View::Activity {
                    this.log_scroll.scroll_to_bottom();
                }
                cx.notify();
            }))
    }

    fn profile_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let tab = |profile: Profile, label: &'static str, this: &Self, cx: &mut Context<Self>| {
            let selected = this.profile == profile;
            Button::new(label)
                .label(label)
                .ghost()
                .when(selected, |b| b.primary())
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.profile = profile;
                    this.messages.remove("json");
                    this.messages.remove("action");
                    cx.notify();
                }))
        };
        div()
            .flex()
            .items_end()
            .gap_4()
            .child(
                div()
                    .pb(px(13.))
                    .text_size(px(11.))
                    .text_color(rgb(FAINT))
                    .child("EDITING PROFILE"),
            )
            .child(tab(Profile::Normal, "NORMAL", self, cx))
            .child(tab(Profile::AirMouse, "AIR MOUSE", self, cx))
    }

    fn connection_pill(&self) -> impl IntoElement + use<> {
        let s = &self.studio;
        let (text, color) = if s.connecting {
            ("Connecting…", WARN)
        } else if s.listening {
            (
                if s.actions_enabled && s.native_active {
                    "Listo"
                } else {
                    "Conectado · solo lectura"
                },
                OK,
            )
        } else if s.connected {
            ("Connected · not listening", WARN)
        } else {
            ("Not connected", FAINT)
        };
        div()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .h(px(30.))
            .rounded_full()
            .border_1()
            .border_color(rgb(BORDER))
            .text_size(px(12.))
            .text_color(rgb(TEXT))
            .child(dot(color))
            .child(text)
            .when(s.actions_enabled, |d| {
                d.child(div().text_color(rgb(ACCENT)).child("· actions on"))
            })
    }
}

impl Render for StudioApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focused_once {
            self.focused_once = true;
            window.focus(&self.focus, cx);
        }
        self.sync_editors(window, cx);

        let (chip_text, chip_color) = self.engine_chip();
        let content = match self.view {
            View::Buttons => self.render_buttons(window, cx).into_any_element(),
            View::Activity => self.render_activity(cx).into_any_element(),
            View::Connection => self.render_connection(cx).into_any_element(),
        };
        let toast = self.toast.clone();
        let header_right = div()
            .flex()
            .items_center()
            .gap_6()
            .when(self.view == View::Buttons, |d| {
                d.child(self.profile_tabs(cx))
                    .child(div().w(px(1.)).h(px(28.)).bg(rgb(BORDER)))
            })
            .child(self.connection_pill())
            .child(
                Button::new("global-pause")
                    .small()
                    .ghost()
                    .label(if self.studio.actions_requested {
                        "Pausar control"
                    } else {
                        "Reanudar control"
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.studio
                            .set_actions_enabled(!this.studio.actions_requested);
                        cx.notify();
                    })),
            )
            .when(self.studio.needs_retry(), |d| {
                d.child(
                    Button::new("retry-control")
                        .small()
                        .label("Reanudar lector")
                        .disabled(self.studio.capture_starting())
                        .on_click(cx.listener(|this, _, _, cx| {
                            let result = this.studio.start_capture_service();
                            this.report("connection", result);
                            cx.notify();
                        })),
                )
            });

        div()
            .id("studio-root")
            .track_focus(&self.focus)
            .key_context("StudioApp")
            .on_key_down(cx.listener(Self::on_key))
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .font_family(".SystemUIFont")
            // Header: title left, profile tabs + connection right.
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_start()
                    .justify_between()
                    .px(px(44.))
                    .pt(px(30.))
                    .pb(px(10.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(34.))
                                    .line_height(px(40.))
                                    .font_weight(FontWeight::BOLD)
                                    .child("MR-01"),
                            )
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(rgb(MUTED))
                                    .child("Motorola remote · Ready For Air"),
                            ),
                    )
                    .child(header_right),
            )
            // Body: navigation column + active view.
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(
                        div()
                            .w(px(250.))
                            .flex_none()
                            .flex()
                            .flex_col()
                            .justify_center()
                            .items_start()
                            .gap(px(14.))
                            .pl(px(44.))
                            .child(self.nav_item(
                                View::Buttons,
                                IconName::LayoutDashboard,
                                "BUTTONS",
                                cx,
                            ))
                            .child(self.nav_item(
                                View::Activity,
                                IconName::SquareTerminal,
                                "ACTIVITY",
                                cx,
                            ))
                            .child(self.nav_item(
                                View::Connection,
                                IconName::Network,
                                "CONNECTION",
                                cx,
                            )),
                    )
                    .child(div().flex_1().min_w_0().min_h_0().flex().child(content)),
            )
            // Footer: engine chip, last result, reveal-config tool button.
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_6()
                    .px(px(44.))
                    .pt(px(12.))
                    .pb(px(26.))
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_4()
                            .h(px(40.))
                            .border_1()
                            .border_color(rgb(0x3a3a3a))
                            .rounded(px(3.))
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(dot(chip_color))
                            .child(chip_text),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(12.5))
                            .map(|d| match toast {
                                Some((text, error, _)) => d
                                    .text_color(rgb(if error { DANGER } else { OK }))
                                    .child(text),
                                None => d
                                    .text_color(rgb(FAINT))
                                    .child(format!("Engine: {}", self.studio.status)),
                            }),
                    )
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(FAINT))
                                    .child("CONFIG FILE"),
                            )
                            .child(
                                div()
                                    .id("reveal-config")
                                    .size(px(48.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .border_1()
                                    .border_color(rgb(0x3a3a3a))
                                    .rounded(px(3.))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(HOVER)).border_color(rgb(MUTED)))
                                    .child(Icon::new(IconName::FolderOpen).size_5())
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.reveal_config();
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
    }
}

fn index_of(b: ButtonId) -> usize {
    ButtonId::ALL.iter().position(|x| *x == b).unwrap_or(0)
}

/// Cheap change detector so the poll loop only re-renders when engine state moved.
fn fingerprint(s: &Studio) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.logs.len().hash(&mut h);
    if let Some(l) = s.logs.last() {
        (&l.time, &l.message).hash(&mut h);
    }
    (s.packet_count, &s.last_packet, &s.status, &s.audio_status).hash(&mut h);
    s.candidates.hash(&mut h);
    (
        &s.reader_error,
        s.capture_access,
        s.capture_verified,
        &s.bluetooth_state,
    )
        .hash(&mut h);
    (s.actions_requested, &s.engine_error, s.capture_starting()).hash(&mut h);
    (
        s.connected,
        s.connecting,
        s.listening,
        s.recording,
        s.actions_enabled,
        s.native_active,
        s.capture_service,
        s.audio_refreshing,
    )
        .hash(&mut h);
    match s.mode {
        MouseMode::Normal => 1,
        MouseMode::Active => 2,
    }
    .hash(&mut h);
    s.learning.map(|(b, p)| (b.id(), p.id())).hash(&mut h);
    s.audio_inputs.hash(&mut h);
    s.config.audio_input.hash(&mut h);
    h.finish()
}
