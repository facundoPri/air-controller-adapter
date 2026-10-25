//! ACA Studio window: integrated header, navigation, shared state and the
//! 50 ms engine poll.
//!
//! Views live in `src/ui/*_view.rs`. The Buttons view composes
//! `remote_stage.rs` (3D remote and callouts, placed by `callouts.rs`) and
//! `button_inspector.rs`. Connection state is presented through `status.rs`,
//! style tokens and English labels live in `theme.rs`, and the 3D remote is
//! painted by `src/remote_scene.rs`.

#[path = "remote_scene.rs"]
mod remote_scene;

mod activity_view;
mod button_inspector;
mod buttons_view;
mod callouts;
mod catalog;
mod connection_view;
mod follow_movement;
mod interaction;
#[cfg(feature = "virtual-keyboard")]
mod keyboard_panel;
mod remote_stage;
mod status;
mod theme;

pub use theme::install as install_theme;

use gpui_kit::{
    base::FocusTrapElement,
    component::{
        Icon, IconName, Sizable,
        button::{Button, ButtonVariants},
        input::TextareaState,
    },
    prelude::FluentBuilder,
    *,
};
use ready_for_air::{
    lifecycle::ConnectionCommand,
    studio::{ButtonId, MouseMode, Profile, Studio},
};
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

/// Origin of the native window controls inside the integrated header.
pub const TRAFFIC_LIGHTS: Point<Pixels> = point(px(20.), px(25.));
const HEADER_H: f32 = 64.;
/// Header content starts clear of the traffic lights.
const HEADER_LEFT: f32 = 92.;

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    #[cfg(feature = "virtual-keyboard")]
    Keyboard,
    Buttons,
    Activity,
    Connection,
}

pub struct StudioApp {
    #[cfg(feature = "virtual-keyboard")]
    keyboard_panel: Option<(AnyWindowHandle, Entity<keyboard_panel::Panel>)>,
    #[cfg(feature = "virtual-keyboard")]
    keyboard_open: Option<ready_for_air::keyboard::Request>,
    #[cfg(feature = "virtual-keyboard")]
    keyboard_test_due: Option<(Instant, u64)>,
    #[cfg(feature = "virtual-keyboard")]
    keyboard_saved_position: Option<ready_for_air::keyboard_model::KeyboardPosition>,
    studio: Studio,
    view: View,
    selected: ButtonId,
    /// Profile being edited; independent of the remote's observed mode.
    profile: Profile,
    camera: Camera,
    follow_movement: follow_movement::FollowMovement,
    hovered: Option<usize>,
    /// Button observed from hardware, glowing until the instant expires.
    pressed: Option<(usize, Instant)>,
    gesture: interaction::StageGesture,
    /// Scene container bounds, written during prepaint by a measuring canvas.
    canvas_size: Rc<Cell<Size<Pixels>>>,
    canvas_origin: Rc<Cell<Point<Pixels>>>,
    known_size: Size<Pixels>,
    callouts: remote_stage::Callouts,
    focus: FocusHandle,
    focused_once: bool,
    drafts: ready_for_air::drafts::Drafts,
    config_editor: Option<Entity<TextareaState>>,
    config_editor_key: String,
    /// Inline results per area ("action", "test", "config", "connection", "logs"): (text, is_error).
    messages: HashMap<&'static str, (String, bool)>,
    fingerprint: u64,
    log_scroll: ScrollHandle,
    log_marker: (usize, String),
    connection_scroll: ScrollHandle,
    /// Inspector's collapsed advanced options.
    advanced_open: bool,
    /// The user chose to edit mappings without a connected remote.
    offline_editing: bool,
    confirmation: interaction::Confirmation,
    configuration_open: bool,
    diagnostics_open: bool,
    /// Scroll to and focus the configuration editor on the next render.
    reveal_editor: bool,
    migration_dismissed: bool,
    /// Left button went down on empty header space; moving starts a window drag.
    header_press: bool,
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
            #[cfg(feature = "virtual-keyboard")]
            keyboard_panel: None,
            #[cfg(feature = "virtual-keyboard")]
            keyboard_open: None,
            #[cfg(feature = "virtual-keyboard")]
            keyboard_test_due: None,
            #[cfg(feature = "virtual-keyboard")]
            keyboard_saved_position: None,
            fingerprint: fingerprint(&studio),
            studio,
            view: View::Buttons,
            selected: ButtonId::Home,
            profile: Profile::Normal,
            camera: Camera::default(),
            follow_movement: Default::default(),
            hovered: None,
            pressed: None,
            gesture: Default::default(),
            canvas_size: Rc::new(Cell::new(Size::default())),
            canvas_origin: Rc::new(Cell::new(Point::default())),
            known_size: Size::default(),
            callouts: Default::default(),
            focus: cx.focus_handle(),
            focused_once: false,
            drafts: Default::default(),
            config_editor: None,
            config_editor_key: String::new(),
            messages: HashMap::new(),
            log_scroll: ScrollHandle::new(),
            log_marker: (0, String::new()),
            connection_scroll: ScrollHandle::new(),
            advanced_open: false,
            offline_editing: false,
            confirmation: interaction::Confirmation::new(cx),
            configuration_open: false,
            diagnostics_open: false,
            reveal_editor: false,
            migration_dismissed: false,
            header_press: false,
        }
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        self.studio.poll();
        #[cfg(feature = "virtual-keyboard")]
        self.keyboard_tick(cx);
        let motion = self.studio.take_view_motion();
        let camera = self.follow_movement.step(
            self.camera,
            motion,
            self.studio.mode == MouseMode::Active
                && self.studio.listening
                && self.view == View::Buttons
                && !self.gesture.active(),
        );
        let mut changed = camera != self.camera;
        self.camera = camera;
        if let Some(result) = self.studio.learn_result.take() {
            self.report("action", result);
            changed = true;
        }
        // `last_button` is consumed so repeated presses of the same button glow again.
        if let Some(b) = self.studio.last_button.take() {
            self.pressed = Some((index_of(b), Instant::now()));
            changed = true;
        }
        if self
            .pressed
            .is_some_and(|(_, t)| t.elapsed() > Duration::from_millis(600))
        {
            self.pressed = None;
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

    /// Records an inline result for `area`.
    fn report(&mut self, area: &'static str, result: Result<String, String>) {
        let (text, error) = match result {
            Ok(t) => (t, false),
            Err(e) => (e, true),
        };
        self.messages.insert(area, (text, error));
    }

    fn message(&self, area: &str) -> Option<&(String, bool)> {
        self.messages.get(area)
    }

    fn select(&mut self, b: ButtonId) {
        if self.selected != b {
            self.selected = b;
            self.messages.remove("action");
            self.messages.remove("test");
        }
    }

    fn step_selection(&mut self, delta: isize) {
        let n = ButtonId::ALL.len() as isize;
        let i = (index_of(self.selected) as isize + delta).rem_euclid(n) as usize;
        self.select(ButtonId::ALL[i]);
    }

    fn set_profile(&mut self, profile: Profile) {
        if self.profile != profile {
            self.profile = profile;
            self.messages.remove("action");
            self.messages.remove("test");
        }
    }

    fn rotate(&mut self, d_yaw: f32, d_pitch: f32) {
        self.follow_movement.stop();
        self.camera = self.camera.rotated(d_yaw, d_pitch);
    }

    fn show(&mut self, view: View) {
        self.view = view;
        if view == View::Activity {
            self.log_scroll.scroll_to_bottom();
        }
    }

    /// Opens Connection with the full configuration editor in view.
    fn open_configuration(&mut self) {
        self.show(View::Connection);
        self.configuration_open = true;
        self.reveal_editor = true;
    }

    fn run_command(&mut self, command: ConnectionCommand) {
        if command == ConnectionCommand::Disconnect {
            self.offline_editing = false;
        }
        let result = self.studio.connection_command(command);
        self.report("connection", result);
    }

    fn run_step(&mut self, step: status::Step) {
        match step {
            status::Step::Run(command) => self.run_command(command),
            status::Step::ChooseDevice => self.show(View::Connection),
            status::Step::OpenConfiguration => self.open_configuration(),
        }
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
        [&self.config_editor]
            .into_iter()
            .flatten()
            .any(|e| e.read(cx).focus_handle(cx).contains_focused(window, cx))
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        if self.editor_focused(window, cx) {
            return;
        }
        let alt = ks.modifiers.alt;
        let step = 0.15;
        match ks.key.as_str() {
            "escape" if self.studio.learning.is_some() => {
                self.studio.cancel_learning();
                self.report("action", Ok("Identification canceled".into()));
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
            "0" => self.camera = self.follow_movement.reset(),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// Creates the full JSON editor lazily and preserves its draft across changes.
    fn sync_editors(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    /// Button for a setup step, shared by the header, landing, strip and Connection.
    fn step_button(
        &self,
        id: impl Into<ElementId>,
        next: status::NextStep,
        primary: bool,
        cx: &mut Context<Self>,
    ) -> Button {
        let resolving = next.step == status::Step::Run(ConnectionCommand::ResolveSetup)
            && self.studio.capture_starting();
        Button::new(id)
            .label(next.label)
            .small()
            .map(|b| if primary { b.primary() } else { b.outline() })
            .loading(resolving)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.run_step(next.step.clone());
                cx.notify();
            }))
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
            .h(px(40.))
            .map(|b| if selected { b.primary() } else { b.ghost() })
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.show(view);
                cx.notify();
            }))
    }

    fn profile_selector(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let segment = |profile: Profile, tip: &'static str, this: &Self, cx: &mut Context<Self>| {
            let selected = this.profile == profile;
            Button::new(profile.id())
                .label(profile_name(profile))
                .small()
                .flex_1()
                .map(|b| if selected { b.primary() } else { b.ghost() })
                .tooltip(tip)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.set_profile(profile);
                    cx.notify();
                }))
        };
        no_drag(div())
            .w(px(INSPECTOR_W))
            .flex_none()
            .flex()
            .gap_1()
            .p(px(3.))
            .rounded(px(9.))
            .bg(rgb(PANEL_DEEP))
            .border_1()
            .border_color(rgb(BORDER))
            .child(segment(
                Profile::Normal,
                "Edit actions for when the remote is in Normal mode",
                self,
                cx,
            ))
            .child(segment(
                Profile::AirMouse,
                "Edit actions for when the remote's Air mouse is on. Does not switch the remote's mode.",
                self,
                cx,
            ))
            .map(|d| {
                #[cfg(feature = "virtual-keyboard")]
                let d = d.child(segment(Profile::Keyboard, "Edit actions while ACA keyboard is visible", self, cx));
                d
            })
    }

    fn render_header(
        &self,
        status: &ready_for_air::lifecycle::ConnectionStatus,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let (headline, color) = status::headline(status);
        let mode = status::observed_mode(status);
        let device_action =
            if self.view == View::Buttons && status::landing(status) && !self.offline_editing {
                // The landing page leads with its own Connect.
                None
            } else {
                status::primary(status)
                    .filter(|n| n.step == status::Step::Run(ConnectionCommand::Connect))
                    .map(|n| self.step_button("header-connect", n, true, cx))
            };
        div()
            .id("header")
            .flex_none()
            .h(px(HEADER_H))
            .flex()
            .items_center()
            .gap_4()
            .pl(px(HEADER_LEFT))
            .pr(px(PAGE_X))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, window, _| {
                    if ev.click_count >= 2 {
                        this.header_press = false;
                        window.titlebar_double_click();
                    } else {
                        this.header_press = true;
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, _| this.header_press = false),
            )
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, window, _| {
                if std::mem::take(&mut this.header_press)
                    && ev.pressed_button == Some(MouseButton::Left)
                {
                    window.start_window_move();
                }
            }))
            .child(
                div()
                    .text_size(px(22.))
                    .line_height(px(28.))
                    .font_weight(FontWeight::BOLD)
                    .child("MR-01"),
            )
            // One truthful indicator: readiness from the core plus observed mode.
            .child(
                no_drag(div())
                    .id("status-pill")
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .h(px(30.))
                    .rounded_full()
                    .border_1()
                    .border_color(rgb(BORDER_STRONG))
                    .text_size(px(12.5))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(HOVER)))
                    .child(dot(color))
                    .child(div().text_color(rgb(TEXT)).child(headline))
                    .when_some(mode, |d, mode| {
                        d.child(div().text_color(rgb(FAINT)).child("·"))
                            .child(div().text_color(rgb(MUTED)).child(mode))
                    })
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.show(View::Connection);
                        cx.notify();
                    })),
            )
            .when_some(device_action, |d, b| d.child(no_drag(div()).child(b)))
            .child(div().flex_1())
            .when(self.view == View::Buttons, |d| {
                d.child(self.profile_selector(cx))
            })
    }

    fn render_migration_banner(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        div()
            .mx(px(PAGE_X))
            .mb_3()
            .flex()
            .items_center()
            .gap_3()
            .px_4()
            .py_2()
            .rounded(px(10.))
            .bg(rgb(ACCENT_SOFT))
            .border_1()
            .border_color(rgb(0x1d5a54))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.5))
                    .text_color(rgb(TEXT))
                    .child("Your configuration was migrated. Review what changed before relying on affected buttons."),
            )
            .child(
                Button::new("migration-review")
                    .label("Review")
                    .small()
                    .outline()
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.open_configuration();
                        cx.notify();
                    })),
            )
            .child(
                Button::new("migration-dismiss")
                    .icon(Icon::new(IconName::Close))
                    .small()
                    .ghost()
                    .tooltip("Hide for this session; details stay in Connection")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.migration_dismissed = true;
                        cx.notify();
                    })),
            )
    }

    fn open_confirmation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.gesture.cancel();
        self.header_press = false;
        self.confirmation.open(window, cx);
        cx.notify();
    }

    fn finish_confirmation(
        &mut self,
        choice: interaction::Choice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.confirmation.dismiss(&self.focus, window, cx);
        if choice == interaction::Choice::Disconnect {
            self.run_command(ConnectionCommand::Disconnect);
        }
        cx.notify();
    }

    fn confirmation_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use interaction::{Choice, Outcome};
        match self.confirmation.key_down(event, window, cx) {
            Some(Outcome::Cancel) => self.finish_confirmation(Choice::Cancel, window, cx),
            Some(Outcome::Disconnect) => self.finish_confirmation(Choice::Disconnect, window, cx),
            Some(Outcome::Stay) => cx.notify(),
            None => {}
        }
    }

    fn render_disconnect_confirmation(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        use interaction::Choice;
        // Explicit focus handles give Tab, accessibility and restoration the
        // same two real targets. The parent captures keys before either target.
        let choice = |value: Choice, label: &'static str, cx: &mut Context<Self>| {
            div()
                .id(label)
                .track_focus(self.confirmation.focus(value))
                .tab_index(0)
                .tab_stop(true)
                .role(accesskit::Role::Button)
                .aria_label(label)
                .cursor_pointer()
                .px_3()
                .py_2()
                .rounded(px(6.))
                .border_2()
                .border_color(rgb(if self.confirmation.focus(value).is_focused(window) {
                    ACCENT
                } else {
                    BORDER_STRONG
                }))
                .bg(rgb(if value == Choice::Disconnect {
                    DANGER
                } else {
                    PANEL
                }))
                .text_color(rgb(if value == Choice::Disconnect {
                    BG
                } else {
                    TEXT
                }))
                .text_size(px(13.))
                .child(label)
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.finish_confirmation(value, window, cx);
                }))
        };
        div()
            .id("disconnect-confirmation")
            .absolute().inset_0().flex().items_center().justify_center()
            .bg(rgba(0x000000b0))
            .occlude()
            .role(accesskit::Role::AlertDialog)
            .aria_label("Disconnect MR-01?")
            .focus_trap("disconnect-focus", self.confirmation.container())
            .child(
                card().w(px(440.)).gap_4().p_6().bg(rgb(RAISED)).border_color(rgb(BORDER_STRONG))
                    .child(heading("Disconnect MR-01?"))
                    .child(caption("ACA stops reading the remote and running its actions, and will not reconnect until you choose Connect."))
                    .child(caption("macOS may keep the remote's Bluetooth link open. To disconnect it completely, use Bluetooth Settings afterwards; ACA cannot confirm that step."))
                    .child(div().flex().justify_end().gap_2()
                        .child(choice(Choice::Cancel, "Cancel", cx))
                        .child(choice(Choice::Disconnect, "Disconnect", cx)))
            )
    }
}

/// Keeps a header control's mouse-down from starting a window drag.
fn no_drag(d: Div) -> Div {
    d.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

impl Render for StudioApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focused_once {
            self.focused_once = true;
            window.focus(&self.focus, cx);
        }
        self.sync_editors(window, cx);

        let status = self.studio.connection_status();
        let content = match self.view {
            #[cfg(feature = "virtual-keyboard")]
            View::Keyboard => self.render_keyboard_settings(cx).into_any_element(),
            View::Buttons => self.render_buttons(&status, window, cx).into_any_element(),
            View::Activity => self.render_activity(&status, cx).into_any_element(),
            View::Connection => self
                .render_connection(&status, window, cx)
                .into_any_element(),
        };

        div()
            .id("studio-root")
            .track_focus(&self.focus)
            .key_context("StudioApp")
            .capture_key_down(cx.listener(Self::confirmation_key))
            .capture_key_up(cx.listener(|this, _: &KeyUpEvent, window, cx| {
                this.confirmation.key_up(window, cx);
            }))
            .on_key_down(cx.listener(Self::on_key))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .font_family(UI_FONT)
            .child(self.render_header(&status, cx))
            .when(
                self.studio.migration_notice.is_some() && !self.migration_dismissed,
                |d| d.child(self.render_migration_banner(cx)),
            )
            // Body: navigation column + active view.
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(
                        div()
                            .w(px(NAV_W))
                            .flex_none()
                            .flex()
                            .flex_col()
                            .justify_center()
                            .items_start()
                            .gap(px(12.))
                            .pl(px(PAGE_X))
                            .pb(px(HEADER_H))
                            .child(self.nav_item(
                                View::Buttons,
                                IconName::LayoutDashboard,
                                "BUTTONS",
                                cx,
                            ))
                            .map(|d| {
                                #[cfg(feature = "virtual-keyboard")]
                                let d = d.child(self.nav_item(
                                    View::Keyboard,
                                    IconName::SquareTerminal,
                                    "KEYBOARD",
                                    cx,
                                ));
                                d
                            })
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
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .flex()
                            .pr(px(PAGE_X))
                            .pb(px(24.))
                            .child(content),
                    ),
            )
            .when(self.confirmation.is_open(), |d| {
                d.child(self.render_disconnect_confirmation(window, cx))
            })
    }
}

fn index_of(b: ButtonId) -> usize {
    ButtonId::ALL.iter().position(|x| *x == b).unwrap_or(0)
}

/// Cheap change detector so the poll loop only re-renders when engine state moved.
fn fingerprint(s: &Studio) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.connection_status().hash(&mut h);
    s.logs.len().hash(&mut h);
    if let Some(l) = s.logs.last() {
        (&l.time, &l.message).hash(&mut h);
    }
    (
        s.packet_count,
        &s.last_packet,
        &s.status,
        &s.migration_notice,
    )
        .hash(&mut h);
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
        s.actions_enabled,
        s.native_active,
        s.capture_service,
    )
        .hash(&mut h);
    match s.mode {
        MouseMode::Normal => 1,
        MouseMode::Active => 2,
    }
    .hash(&mut h);
    s.learning.map(|(b, p)| (b.id(), p.id())).hash(&mut h);
    s.last_action_result.hash(&mut h);
    h.finish()
}
