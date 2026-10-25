//! Interactive 3D remote: scene, callouts beside their buttons, leaders,
//! rotation by drag or keys, click-to-select and the view controls.

use super::{
    StudioApp,
    callouts::{self, Placed},
    index_of,
    interaction::Release,
    remote_scene::{self, Camera, Highlight},
    theme::*,
};
use gpui_kit::{
    component::{
        Sizable,
        button::{Button, ButtonVariants},
    },
    prelude::FluentBuilder,
    *,
};
use ready_for_air::studio::{ButtonId, MouseMode};
use std::hash::{Hash, Hasher};

const DRAG_RAD_PER_PX: f32 = 0.008;
/// Label text styles; measurement and rendering share them.
const TITLE_SIZE: f32 = 13.5;
const TITLE_WEIGHT: FontWeight = FontWeight::SEMIBOLD;
const SUBTITLE_SIZE: f32 = 11.5;
const LEARNING: &str = "Press it on the remote…";

/// Placement memory plus the last layout, reused while its inputs are unchanged.
#[derive(Default)]
pub(super) struct Callouts {
    memory: callouts::Memory,
    cache: Option<(u64, Vec<Placed>)>,
    /// Shaped label widths for the current texts.
    widths: Option<(u64, Vec<(f32, f32)>)>,
}

/// Width of `text` as GPUI lays it out in the given label style.
fn text_width(window: &Window, text: &str, size: f32, weight: FontWeight) -> f32 {
    let run = TextRun {
        len: text.len(),
        font: Font {
            weight,
            ..font(UI_FONT)
        },
        color: rgb(TEXT).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    f32::from(
        window
            .text_system()
            .layout_line(text, px(size), &[run], None)
            .width,
    )
}

impl Callouts {
    fn layout(
        &mut self,
        camera: Camera,
        size: Size<Pixels>,
        texts: &[(&'static str, String)],
        focus: &[usize],
        window: &Window,
    ) -> Vec<Placed> {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        texts.hash(&mut h);
        let texts_key = h.finish();
        if self.widths.as_ref().is_none_or(|(k, _)| *k != texts_key) {
            let widths = texts
                .iter()
                .map(|(name, action)| {
                    callouts::label_size(
                        text_width(window, name, TITLE_SIZE, TITLE_WEIGHT),
                        text_width(window, action, SUBTITLE_SIZE, FontWeight::NORMAL),
                    )
                })
                .collect();
            self.widths = Some((texts_key, widths));
        }
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (camera.yaw.to_bits(), camera.pitch.to_bits()).hash(&mut h);
        (
            f32::from(size.width).to_bits(),
            f32::from(size.height).to_bits(),
        )
            .hash(&mut h);
        (texts, focus).hash(&mut h);
        let key = h.finish();
        if let Some((k, placed)) = &self.cache {
            if *k == key {
                return placed.clone();
            }
        }
        let widths = self.widths.as_ref().map_or(&[][..], |(_, w)| w.as_slice());
        let placed = callouts::place(camera, size, widths, focus, &mut self.memory);
        self.cache = Some((key, placed.clone()));
        placed
    }

    /// True when `local` lies on a label of the current layout.
    fn covers(&self, local: Point<Pixels>) -> bool {
        let (x, y) = (f32::from(local.x), f32::from(local.y));
        self.cache.as_ref().is_some_and(|(_, placed)| {
            placed.iter().any(|p| {
                x >= p.rect.x
                    && x <= p.rect.x + p.rect.w
                    && y >= p.rect.y
                    && y <= p.rect.y + p.rect.h
            })
        })
    }
}

impl StudioApp {
    fn local(&self, position: Point<Pixels>) -> Point<Pixels> {
        position - self.canvas_origin.get()
    }

    pub(super) fn render_stage(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let size = self.known_size;
        let selected = index_of(self.selected);
        let pressed = self.pressed.map(|(i, _)| i);
        let learning = self
            .studio
            .learning
            .filter(|(_, p)| *p == self.profile)
            .map(|(b, _)| index_of(b));
        let highlight = Highlight {
            selected: Some(selected),
            hovered: self.hovered,
            pressed,
        };
        let focus: Vec<usize> = [Some(selected), learning, self.hovered, pressed]
            .into_iter()
            .flatten()
            .collect();
        let texts: Vec<(&'static str, String)> = ButtonId::ALL
            .iter()
            .map(|&b| {
                // The Air mouse button has no assignable press; say what it does.
                let action = if learning == Some(index_of(b)) {
                    LEARNING.to_string()
                } else if b == ButtonId::MouseMode {
                    "Toggles Air mouse".to_string()
                } else {
                    action_name(self.studio.action(b, self.profile))
                };
                (button_name(b), action)
            })
            .collect();
        let placed = self
            .callouts
            .layout(self.camera, size, &texts, &focus, window);
        let lines: Vec<(Point<Pixels>, Point<Pixels>, bool)> = placed
            .iter()
            .map(|p| (p.anchor, p.leader_end(), p.index == selected))
            .collect();
        let size_cell = self.canvas_size.clone();
        let origin_cell = self.canvas_origin.clone();

        let labels = placed.into_iter().map(|p| {
            let b = ButtonId::ALL[p.index];
            let is_selected = p.index == selected;
            let is_pressed = pressed == Some(p.index);
            let is_learning = learning == Some(p.index);
            let border = if is_selected {
                ACCENT
            } else if is_learning {
                WARN
            } else {
                BORDER_STRONG
            };
            let subtitle = (!p.compact).then(|| texts[p.index].1.clone());
            div()
                .id(("callout", p.index))
                .absolute()
                .left(px(p.rect.x))
                .top(px(p.rect.y))
                .w(px(p.rect.w))
                .h(px(p.rect.h))
                .flex()
                .flex_col()
                .justify_center()
                .when(p.leftward(), |d| d.items_end())
                .px(px(callouts::PAD_X))
                .bg(rgb(if is_pressed || is_selected {
                    ACCENT_SOFT
                } else {
                    PANEL_DEEP
                }))
                .border_1()
                .border_color(rgb(border))
                .rounded(px(5.))
                .cursor_pointer()
                .hover(|s| s.border_color(rgb(if is_selected { ACCENT } else { MUTED })))
                .child(
                    div()
                        .max_w_full()
                        .truncate()
                        .text_size(px(TITLE_SIZE))
                        .line_height(px(17.))
                        .font_weight(TITLE_WEIGHT)
                        .text_color(rgb(TEXT))
                        .child(button_name(b)),
                )
                .when_some(subtitle, |d, text| {
                    d.child(
                        div()
                            .max_w_full()
                            .truncate()
                            .text_size(px(SUBTITLE_SIZE))
                            .line_height(px(15.))
                            .text_color(rgb(if is_learning {
                                WARN
                            } else if is_selected {
                                ACCENT
                            } else {
                                MUTED
                            }))
                            .child(text),
                    )
                })
                .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                    if !matches!(ev, ClickEvent::Keyboard(_)) && !this.gesture.label_selects() {
                        return;
                    }
                    this.select(b);
                    window.focus(&this.focus, cx);
                    cx.notify();
                }))
        });

        let following = self.follow_movement.enabled();
        let air_mouse = self.studio.mode == MouseMode::Active;
        div()
            .id("stage")
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .cursor(if self.gesture.moved() {
                CursorStyle::ClosedHand
            } else if self.hovered.is_some() {
                CursorStyle::PointingHand
            } else {
                CursorStyle::OpenHand
            })
            .child(remote_scene::scene(self.camera, highlight))
            // Leaders from ring markers to the nearest point of each label.
            .child(
                canvas(
                    move |bounds, _, _| {
                        size_cell.set(bounds.size);
                        origin_cell.set(bounds.origin);
                        bounds.origin
                    },
                    move |_, origin, window, _| {
                        for (from, to, sel) in &lines {
                            let mut path = PathBuilder::stroke(px(1.));
                            path.move_to(origin + *from);
                            path.line_to(origin + *to);
                            if let Ok(path) = path.build() {
                                window.paint_path(path, rgb(if *sel { ACCENT } else { 0x5c5c5c }));
                            }
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            .children(labels)
            // View controls (keyboard equivalents: [ ] or ⌥←/→ rotate, ⌥↑/↓ tilt, 0 resets).
            .child(
                div()
                    .id("stage-controls")
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _: &MouseDownEvent, _, cx| {
                        this.gesture.cancel();
                        cx.stop_propagation();
                    }))
                    .on_mouse_up(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, cx| {
                        this.gesture.cancel();
                        cx.stop_propagation();
                    }))
                    .absolute()
                    .bottom(px(6.))
                    .left(px(0.))
                    .right(px(0.))
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        Button::new("reset-view")
                            .label("Reset view")
                            .ghost()
                            .small()
                            .tooltip("Reset camera  ( 0 )")
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.camera = this.follow_movement.reset();
                                cx.notify();
                            })),
                    )
                    .when(air_mouse || following, |row| {
                        row.child(
                            Button::new("follow-movement")
                                .label(match (following, air_mouse) {
                                    (true, true) => "Stop following",
                                    (true, false) => "Stop following (paused)",
                                    _ => "Follow movement",
                                })
                                .ghost()
                                .small()
                                .tooltip("Approximate rotation from Air mouse movement, not absolute orientation. Drag to return to manual control.")
                                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.follow_movement.toggle(this.camera);
                                    this.gesture.cancel();
                                    cx.notify();
                                })),
                        )
                    })
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .pl_2()
                            .text_size(px(11.5))
                            .text_color(rgb(FAINT))
                            .child("Drag to rotate · click a button or label · ↑/↓ select"),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus, cx);
                    this.gesture.begin(ev.position);
                }),
            )
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                if ev.pressed_button == Some(MouseButton::Left) {
                    if this.gesture.active() {
                        if let Some(delta) = this.gesture.motion(ev.position) {
                            this.rotate(
                                f32::from(delta.x) * DRAG_RAD_PER_PX,
                                f32::from(delta.y) * DRAG_RAD_PER_PX,
                            );
                            cx.notify();
                        }
                        return;
                    }
                } else if this.gesture.active() {
                    this.gesture.cancel();
                    cx.notify();
                }
                // Labels hover on their own; the model underneath stays unlit
                // so pointing at a label never re-flows the layout.
                let local = this.local(ev.position);
                let hovered = if this.callouts.covers(local) {
                    None
                } else {
                    remote_scene::hit_test(this.camera, this.known_size, local)
                };
                if hovered != this.hovered {
                    this.hovered = hovered;
                    cx.notify();
                }
            }))
            .on_hover(cx.listener(|this, hovering: &bool, _, cx| {
                if !*hovering && this.hovered.take().is_some() {
                    cx.notify();
                }
            }))
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    this.gesture.cancel();
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseUpEvent, _, cx| {
                    let released = this.gesture.release();
                    let local = this.local(ev.position);
                    // The label owns its click; never select the model beneath it.
                    if released == Release::Click && !this.callouts.covers(local) {
                        if let Some(i) = remote_scene::hit_test(this.camera, this.known_size, local) {
                            this.select(ButtonId::ALL[i]);
                        }
                    }
                    cx.notify();
                }),
            )
    }
}
