//! BUTTONS view: interactive 3D remote with callouts + inspector card.

use super::{
    Drag, StudioApp, index_of,
    remote_scene::{self, Camera, Highlight, Side},
    theme::*,
};
use gpui_kit::{
    base::Disableable,
    component::{
        IconName, Sizable,
        button::{Button, ButtonVariants},
        input::Textarea,
    },
    prelude::FluentBuilder,
    *,
};
use ready_for_air::studio::{Action, ButtonId};

const CALLOUT_H: f32 = 38.;
const CALLOUT_GAP: f32 = 6.;
const CALLOUT_STRIDE: f32 = CALLOUT_H + CALLOUT_GAP;
/// Clearance between the remote's silhouette and a callout column, when it fits.
const COLUMN_GAP: f32 = 30.;
/// Widest expected callout; columns are laid out at this width.
const CALLOUT_MAX_W: f32 = 150.;
/// Stage padding kept clear of callouts.
const STAGE_MARGIN: f32 = 8.;
/// Narrowest strip between the two columns. Stages too narrow for two full
/// columns plus this strip only label focused buttons.
const CENTRE_MIN: f32 = 60.;
/// Callouts keep this far from hotspot ring centres (ring outer radius 7.75).
const RING_CLEAR: f32 = 10.;
/// How far into the box a leader may attach when its anchor sits under the column.
const LEADER_HOOK: f32 = 44.;
const DRAG_RAD_PER_PX: f32 = 0.008;

struct Placed {
    index: usize,
    anchor: Point<Pixels>,
    side: Side,
    top: f32,
    /// Column edge facing the remote: right edge for Left, left edge for Right.
    edge: f32,
}

impl Placed {
    /// Horizontal extent of the widest possible box in this column.
    fn span(&self) -> (f32, f32) {
        match self.side {
            Side::Left => (self.edge - CALLOUT_MAX_W, self.edge),
            Side::Right => (self.edge, self.edge + CALLOUT_MAX_W),
        }
    }

    /// Where the leader line meets the box: the inner edge's midpoint, or the
    /// nearer top/bottom edge when the anchor sits under the column.
    fn leader_end(&self) -> Point<Pixels> {
        let (ax, ay) = (f32::from(self.anchor.x), f32::from(self.anchor.y));
        let mid = self.top + CALLOUT_H / 2.;
        let x = match self.side {
            Side::Left if ax < self.edge => ax.max(self.edge - LEADER_HOOK),
            Side::Right if ax > self.edge => ax.min(self.edge + LEADER_HOOK),
            _ => return point(px(self.edge), px(mid)),
        };
        point(
            px(x),
            px(if ay < mid {
                self.top
            } else {
                self.top + CALLOUT_H
            }),
        )
    }
}

/// Lays callouts out as an overlay in two columns: outside the remote's
/// silhouette when the stage allows, otherwise over the model, clamped to the
/// stage. Each column keeps its labels in anchor-height order, never overlaps,
/// and steers boxes off hotspot rings where the height allows. Only `focus`
/// buttons get callouts when the stage is too narrow for both columns.
fn place_callouts(camera: Camera, size: Size<Pixels>, focus: &[usize]) -> Vec<Placed> {
    let (width, height) = (f32::from(size.width), f32::from(size.height));
    let (lo, hi) = (STAGE_MARGIN, height - 48. - CALLOUT_H);
    if width <= 0. || hi < lo {
        return vec![];
    }
    let body = remote_scene::body_bounds(camera, size);
    let half_strip = CENTRE_MIN / 2.;
    let left_edge = (f32::from(body.left()) - COLUMN_GAP)
        .min(width / 2. - half_strip)
        .max(CALLOUT_MAX_W + STAGE_MARGIN);
    let right_edge = (f32::from(body.right()) + COLUMN_GAP)
        .max(width / 2. + half_strip)
        .min(width - CALLOUT_MAX_W - STAGE_MARGIN);
    let full = width >= 2. * (CALLOUT_MAX_W + STAGE_MARGIN) + CENTRE_MIN;
    let spots = remote_scene::hotspots(camera, size);
    let rings: Vec<Point<Pixels>> = spots
        .iter()
        .filter(|h| h.visible)
        .map(|h| h.anchor)
        .collect();
    let (left, right): (Vec<Placed>, Vec<Placed>) = spots
        .iter()
        .filter(|h| full || focus.contains(&h.index))
        .map(|h| Placed {
            index: h.index,
            anchor: h.anchor,
            side: h.side,
            top: f32::from(h.anchor.y) - CALLOUT_H / 2.,
            edge: if h.side == Side::Left {
                left_edge
            } else {
                right_edge
            },
        })
        .partition(|p| p.side == Side::Left);
    if left_edge + CALLOUT_GAP > right_edge {
        // Columns overlap horizontally (narrow stage): share one vertical stack.
        stack(
            left.into_iter().chain(right).collect(),
            &rings,
            focus,
            lo,
            hi,
        )
    } else {
        let mut placed = stack(left, &rings, focus, lo, hi);
        placed.extend(stack(right, &rings, focus, lo, hi));
        placed
    }
}

/// Assigns tops in `[lo, hi]` to one column (each `top` starts as its target),
/// preferring positions clear of `rings`. Drops unfocused labels only if the
/// stage is too short for all of them.
fn stack(
    mut group: Vec<Placed>,
    rings: &[Point<Pixels>],
    focus: &[usize],
    lo: f32,
    hi: f32,
) -> Vec<Placed> {
    group.sort_by(|a, b| a.top.total_cmp(&b.top));
    let (x0, x1) = group
        .iter()
        .map(Placed::span)
        .fold((f32::MAX, f32::MIN), |(a, b), (l, r)| (a.min(l), b.max(r)));
    // A box whose top falls inside one of these ranges would cover a ring.
    let mut blocked: Vec<(f32, f32)> = rings
        .iter()
        .map(|p| (f32::from(p.x), f32::from(p.y)))
        .filter(|&(x, _)| x > x0 - RING_CLEAR && x < x1 + RING_CLEAR)
        .map(|(_, y)| (y - RING_CLEAR - CALLOUT_H, y + RING_CLEAR))
        .collect();
    blocked.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut free = Vec::new();
    let mut from = lo;
    for (a, b) in blocked {
        if a.min(hi) >= from {
            free.push((from, a.min(hi)));
        }
        from = from.max(b);
    }
    if from <= hi {
        free.push((from, hi));
    }
    loop {
        let targets: Vec<f32> = group.iter().map(|p| p.top).collect();
        if let Some(tops) = fit(&targets, &free).or_else(|| fit(&targets, &[(lo, hi)])) {
            for (p, top) in group.iter_mut().zip(tops) {
                p.top = top;
            }
            return group;
        }
        let drop = group
            .iter()
            .rposition(|p| !focus.contains(&p.index))
            .unwrap_or(group.len() - 1);
        group.remove(drop);
    }
}

/// Cheapest order-preserving placement of sorted `targets` into the `free` top
/// ranges (each separated by more than one stride), or `None` if they don't fit.
fn fit(targets: &[f32], free: &[(f32, f32)]) -> Option<Vec<f32>> {
    let n = targets.len();
    // best[i]: cheapest (cost, tops) for the first i labels in the ranges seen so far.
    let mut best: Vec<Option<(f32, Vec<f32>)>> = vec![None; n + 1];
    best[0] = Some((0., vec![]));
    for &(a, b) in free {
        let prev = best.clone();
        for (s, entry) in prev.iter().enumerate() {
            let Some((base, tops)) = entry else { continue };
            for e in s + 1..=n {
                let Some(run) = stack_run(&targets[s..e], a, b) else {
                    break;
                };
                let cost = base
                    + run
                        .iter()
                        .zip(&targets[s..e])
                        .map(|(t, g)| (t - g).abs())
                        .sum::<f32>();
                if best[e].as_ref().is_none_or(|(c, _)| cost < *c) {
                    best[e] = Some((cost, [tops.as_slice(), &run].concat()));
                }
            }
        }
    }
    best.pop().flatten().map(|(_, tops)| tops)
}

/// Packs sorted targets into `[a, b]` one stride apart, as close to target as possible.
fn stack_run(targets: &[f32], a: f32, b: f32) -> Option<Vec<f32>> {
    if a + (targets.len() - 1) as f32 * CALLOUT_STRIDE > b {
        return None;
    }
    let mut tops: Vec<f32> = Vec::with_capacity(targets.len());
    for &t in targets {
        tops.push(t.max(tops.last().map_or(a, |p| p + CALLOUT_STRIDE)));
    }
    let mut max = b;
    for t in tops.iter_mut().rev() {
        *t = t.min(max);
        max = *t - CALLOUT_STRIDE;
    }
    Some(tops)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn rotation_never_drops_or_overlaps_labels_at_desktop_sizes() {
        for (w, h) in [(610., 620.), (500., 500.), (900., 800.)] {
            for yi in 0..=26 {
                for pi in 0..=12 {
                    let camera = Camera {
                        yaw: -1.3 + yi as f32 * 0.1,
                        pitch: 0.05 + pi as f32 * (1.15 / 12.),
                    };
                    let labels = place_callouts(camera, gpui_kit::size(px(w), px(h)), &[9]);
                    assert_eq!(
                        labels.len(),
                        14,
                        "missing labels at {w}×{h}, yaw {} pitch {}",
                        camera.yaw,
                        camera.pitch
                    );
                    for p in &labels {
                        let (l, r) = p.span();
                        assert!(l >= 0. && r <= w && p.top >= 0. && p.top + CALLOUT_H <= h - 40.);
                        for q in &labels {
                            if p.index == q.index {
                                continue;
                            }
                            let (ql, qr) = q.span();
                            assert!(
                                r <= ql
                                    || l >= qr
                                    || p.top + CALLOUT_H <= q.top
                                    || q.top + CALLOUT_H <= p.top,
                                "label overlap"
                            );
                        }
                    }
                }
            }
        }
    }
    #[::core::prelude::v1::test]
    fn selected_label_survives_narrow_rotated_stages() {
        for selected in 0..14 {
            let labels = place_callouts(
                Camera::default().rotated(1.3, 0.58),
                gpui_kit::size(px(306.), px(480.)),
                &[selected],
            );
            assert_eq!(labels.len(), 1);
            assert_eq!(labels[0].index, selected);
            let (l, r) = labels[0].span();
            assert!(l >= 0. && r <= 306.);
        }
    }

    // gpui_kit's glob import shadows the std `test` attribute.
    #[::core::prelude::v1::test]
    fn callouts_fit_default_stage_without_overlap() {
        // Stage size at the default 1320x860 window.
        let size = gpui_kit::size(px(610.), px(620.));
        let placed = place_callouts(Camera::default(), size, &[8]);
        assert_eq!(placed.len(), ButtonId::ALL.len());
        let body = remote_scene::body_bounds(Camera::default(), size);
        for p in &placed {
            let (l, r) = match p.side {
                Side::Left => (p.edge - CALLOUT_MAX_W, p.edge),
                Side::Right => (p.edge, p.edge + CALLOUT_MAX_W),
            };
            assert!(l >= 0. && r <= 610., "callout {} leaves the stage", p.index);
            assert!(r <= f32::from(body.left()) || l >= f32::from(body.right()));
            assert!(p.top >= 0. && p.top + CALLOUT_H <= 620.);
            for q in placed
                .iter()
                .filter(|q| q.side == p.side && q.index != p.index)
            {
                assert!(
                    (p.top - q.top).abs() >= CALLOUT_H,
                    "{} overlaps {}",
                    p.index,
                    q.index
                );
            }
        }
        // Minimum window: only focused buttons keep a callout.
        let narrow = place_callouts(Camera::default(), gpui_kit::size(px(306.), px(480.)), &[8]);
        assert_eq!(narrow.iter().map(|p| p.index).collect::<Vec<_>>(), [8]);
    }
}

impl StudioApp {
    fn local(&self, position: Point<Pixels>) -> Point<Pixels> {
        position - self.canvas_origin.get()
    }

    pub(super) fn render_buttons(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        div()
            .size_full()
            .flex()
            .gap_4()
            .pr(px(44.))
            .child(self.render_stage(cx))
            .child(self.render_inspector(cx))
    }

    fn render_stage(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
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
        let focus: Vec<usize> = [Some(selected), self.hovered, pressed, learning]
            .into_iter()
            .flatten()
            .collect();
        let placed = place_callouts(self.camera, size, &focus);
        let lines: Vec<(Point<Pixels>, Point<Pixels>, bool)> = placed
            .iter()
            .map(|p| (p.anchor, p.leader_end(), p.index == selected))
            .collect();
        let size_cell = self.canvas_size.clone();
        let origin_cell = self.canvas_origin.clone();

        let callouts = placed.into_iter().map(|p| {
            let b = ButtonId::ALL[p.index];
            let is_selected = p.index == selected;
            let is_pressed = pressed == Some(p.index);
            let is_learning = learning == Some(p.index);
            let action = action_name(self.studio.action(b, self.profile));
            let border = if is_selected {
                ACCENT
            } else if is_learning {
                WARN
            } else {
                CALLOUT_BORDER
            };
            div()
                .id(("callout", p.index))
                .absolute()
                .top(px(p.top))
                .h(px(CALLOUT_H))
                .map(|d| match p.side {
                    Side::Left => d.right(size.width - px(p.edge)).items_end(),
                    Side::Right => d.left(px(p.edge)),
                })
                .max_w(px(CALLOUT_MAX_W))
                .flex()
                .flex_col()
                .justify_center()
                .px(px(12.))
                .bg(rgb(if is_pressed { 0x12302d } else { PANEL_DEEP }))
                .border_1()
                .border_color(rgb(border))
                .rounded(px(3.))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(0x1a1a1a)))
                .child(
                    div()
                        .max_w_full()
                        .truncate()
                        .text_size(px(14.))
                        .line_height(px(18.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(TEXT))
                        .child(button_name(b)),
                )
                .child(
                    div()
                        .max_w_full()
                        .truncate()
                        .text_size(px(11.5))
                        .line_height(px(15.))
                        .text_color(rgb(if is_learning { WARN } else { MUTED }))
                        .child(if is_learning {
                            "Press it on the remote…".to_string()
                        } else {
                            action
                        }),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.drag = None;
                    this.select(b);
                    window.focus(&this.focus, cx);
                    cx.notify();
                }))
        });

        let small = |id: &'static str, icon: IconName, tip: &'static str| {
            Button::new(id).icon(icon).ghost().small().tooltip(tip)
        };

        div()
            .id("stage")
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .cursor(if self.drag.as_ref().is_some_and(|d| d.moved) {
                CursorStyle::ClosedHand
            } else if self.hovered.is_some() {
                CursorStyle::PointingHand
            } else {
                CursorStyle::OpenHand
            })
            .child(remote_scene::scene(self.camera, highlight))
            // Leader lines from ring markers to (possibly nudged) callouts.
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
                                window.paint_path(path, rgb(if *sel { ACCENT } else { 0x6a6a6a }));
                            }
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            .children(callouts)
            // View controls (keyboard equivalents: [ ] or ⌥←/→ rotate, ⌥↑/↓ tilt, 0 resets).
            .child(
                div()
                    .absolute()
                    .bottom(px(8.))
                    .left(px(0.))
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        small("rot-left", IconName::ChevronLeft, "Rotate left  ( [ )").on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.rotate(-0.2, 0.);
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        small("rot-right", IconName::ChevronRight, "Rotate right  ( ] )").on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.rotate(0.2, 0.);
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        small("tilt-up", IconName::ChevronUp, "Tilt up  ( ⌥↑ )").on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.rotate(0., -0.15);
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        small("tilt-down", IconName::ChevronDown, "Tilt down  ( ⌥↓ )").on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.rotate(0., 0.15);
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        Button::new("reset-view")
                            .label("Reset view")
                            .ghost()
                            .small()
                            .tooltip("Reset camera  ( 0 )")
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.camera = Camera::default();
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
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
                    this.drag = Some(Drag {
                        start: ev.position,
                        last: ev.position,
                        moved: false,
                    });
                }),
            )
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                if ev.pressed_button == Some(MouseButton::Left) {
                    if let Some(drag) = &mut this.drag {
                        let delta = ev.position - drag.last;
                        let total = ev.position - drag.start;
                        drag.last = ev.position;
                        if !drag.moved && f32::from(total.x).abs() + f32::from(total.y).abs() > 4. {
                            drag.moved = true;
                        }
                        if drag.moved {
                            this.camera = this.camera.rotated(
                                f32::from(delta.x) * DRAG_RAD_PER_PX,
                                f32::from(delta.y) * DRAG_RAD_PER_PX,
                            );
                            cx.notify();
                        }
                        return;
                    }
                } else if this.drag.take().is_some() {
                    cx.notify();
                }
                let hovered =
                    remote_scene::hit_test(this.camera, this.known_size, this.local(ev.position));
                if hovered != this.hovered {
                    this.hovered = hovered;
                    cx.notify();
                }
            }))
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    if this.drag.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseUpEvent, _, cx| {
                    if let Some(drag) = this.drag.take() {
                        if !drag.moved {
                            if let Some(i) = remote_scene::hit_test(
                                this.camera,
                                this.known_size,
                                this.local(ev.position),
                            ) {
                                this.select(ButtonId::ALL[i]);
                            }
                        }
                        cx.notify();
                    }
                }),
            )
    }

    fn render_inspector(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let b = self.selected;
        let profile = self.profile;
        let studio = &self.studio;
        let current = studio.action(b, profile).clone();
        let learning_this = studio.learning == Some((b, profile));
        let learning_any = studio.learning.is_some();
        let listening = studio.listening;
        let learn_block = if b == ButtonId::MouseMode {
            Some(
                "The Air mouse button sends no event of its own, so it cannot be learned. Its effect is inferred from mouse reports.",
            )
        } else if !listening && !learning_any {
            Some("Studio todavía no puede leer el control. Revisá el estado en Conexión.")
        } else {
            None
        };

        let presets = Action::presets()
            .into_iter()
            .enumerate()
            .map(|(i, (_, action))| {
                let sel = action == current;
                let label = action_name(&action);
                Button::new(("preset", i))
                    .label(label.clone())
                    .small()
                    .when(sel, |b| b.primary())
                    .disabled(b == ButtonId::MouseMode)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        let b = this.selected;
                        let p = this.profile;
                        let result = this.studio.assign_action(b, p, action.clone()).map(|_| {
                            format!(
                                "Guardado: {} · {} → {label}. {}",
                                button_name(b),
                                profile_name(p),
                                if this.studio.button_identified(b, p) {
                                    ""
                                } else {
                                    "La señal está asignada a otro botón; revisá las opciones avanzadas."
                                }
                            )
                        });
                        this.report("action", result);
                        cx.notify();
                    }))
            });

        let editor = self.action_editor.clone();
        let source = source_text(&studio.source_label(b, profile));
        let last_seen = self.last_seen;

        div()
            .id("inspector")
            .w(px(340.))
            .flex_none()
            .h_full()
            .overflow_y_scroll()
            .pb_4()
            .child(
                card()
                    .gap_3()
                    // Selection
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(eyebrow(format!(
                                "SELECTED BUTTON · {} PROFILE",
                                profile_name(profile).to_uppercase()
                            )))
                            .child(
                                div()
                                    .text_size(px(26.))
                                    .font_weight(FontWeight::BOLD)
                                    .child(button_name(b)),
                            )
                            .child(caption(format!("Action: {}", action_name(&current))))
                            .child(caption(format!("Detected as: {source}")))
                            .when(b == ButtonId::MouseMode, |d| {
                                d.child(caption(
                                    "Not directly detectable: this button switches the remote's air mouse in firmware and emits no event. The mode chip below is inferred from observed mouse reports.",
                                ))
                            })
                            .when(b.is_arrow(), |d| {
                                d.child(caption(
                                    "Arrows turn an active air mouse off. Measured for Up; Down, Left and Right are user-reported.",
                                ))
                            })
                            .when_some(last_seen, |d, seen| {
                                d.child(caption(format!(
                                    "Last pressed on the remote: {}",
                                    button_name(seen)
                                )))
                            }),
                    )
                    .when_some(self.message("action").cloned(), |d, m| d.child(notice(&m)))
                    .when(!studio.button_identified(b, profile) || learning_any, |d| d
                    .child(eyebrow("IDENTIFICAR ESTE BOTÓN UNA VEZ"))
                    .child(caption(if studio.button_identified(b, profile) {
                        "Identificado. Podés elegir otra función abajo."
                    } else {
                        "Pulsá Identificar botón y luego ese botón del control una vez."
                    }))
                    // Identify the physical source before choosing an action.
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .child(
                                Button::new("learn")
                                    .label(if learning_this {
                                        "Cancelar identificación"
                                    } else if learning_any {
                                        "Cancelar identificación"
                                    } else {
                                        if studio.button_identified(b, profile) { "Volver a identificar" } else { "Identificar botón" }
                                    })
                                    .small()
                                    .when(!learning_any, |b| b.primary())
                                    .disabled(learn_block.is_some() && !learning_any)
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        if this.studio.learning.is_some() {
                                            this.studio.cancel_learning();
                                            this.report("action", Ok("Identificación cancelada".into()));
                                        } else {
                                            this.messages.remove("rules");
                                            this.studio.learn(this.selected, this.profile);
                                            this.report(
                                                "action",
                                                Ok(format!(
                                                    "Pulsá {} una vez en el control (Esc cancela)",
                                                    button_name(this.selected)
                                                )),
                                            );
                                        }
                                        cx.notify();
                                    })),
                            )
                    )
                    .when(learning_this, |d| {
                        d.child(
                            div()
                                .text_size(px(12.5))
                                .text_color(rgb(WARN))
                                .child(format!(
                                    "Esperando {} en el control… (Esc cancela)",
                                    button_name(b)
                                )),
                        )
                    })
                    .when_some(learn_block.filter(|_| !learning_any), |d, why| {
                        d.child(caption(why))
                    })
                    )
                    .child(eyebrow("FUNCIÓN DEL BOTÓN"))
                    .child(caption("Elegí una función para este perfil. Se guarda automáticamente. El MR-01 ya incluye su mapa de botones."))
                    .child(div().flex().flex_wrap().gap(px(5.)).children(presets))
                    .child(caption(ready_for_air::actions::guidance(&current, studio.config.transcription.is_some())))
                    .when(ready_for_air::actions::uses_zoom(&current), |d| d.child(
                        Button::new("zoom-settings").label("Abrir ajustes de Zoom").small().ghost()
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.open_url("x-apple.systempreferences:com.apple.preference.universalaccess?Zoom");
                            }))
                    ))
                    .when(current == Action::VoiceToggle, |d| d.child(caption(studio.audio_status.clone())))
                    .when_some(studio.last_action_result.as_ref(), |d, result| d.child(notice(result)))
                    .child(caption(studio.engine_status()))
                    .child(eyebrow("OPCIONES AVANZADAS"))
                    .when(studio.button_identified(b, profile) && !learning_any, |d| d.child(
                        Button::new("relearn").label("Volver a identificar").small().ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.studio.learn(this.selected, this.profile); cx.notify();
                            }))
                    ))
                    .child(Button::new("test-action").label("Probar función ahora").small().ghost()
                        .disabled(matches!(current, Action::Default | Action::Disabled))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let r = this.studio.test_action(b, profile).map(|_| "Función solicitada · resultado en Activity".into());
                            this.report("rules", r); cx.notify();
                        })))
                    .when_some(self.message("rules").cloned(), |d,m|d.child(notice(&m)))
                    // Custom JSON
                    .child(Button::new("reload").label("Recargar archivo").small().ghost().on_click(
                        cx.listener(|this, _, _, cx| {
                            let r = this.studio.reload().map(|_| "Archivo recargado".into());
                            this.report("action",r); cx.notify();
                        })))
                    .child(eyebrow("CUSTOM ACTION (JSON)"))
                    .child(Button::new("discard-action-draft").label("Descartar borrador").small().ghost().on_click(cx.listener(|this, _, _, cx| {
                        this.drafts.discard(&this.action_editor_key); this.action_editor_key.clear(); cx.notify();
                    })))
                    .child(caption(
                        "Shortcut, AppleScript, shell command, scroll or macro. Example: {\"type\":\"shortcut\",\"key\":\"k\",\"modifiers\":[\"command\"]}",
                    ))
                    .when_some(editor, |d, editor| {
                        d.child(
                            div()
                                .font_family(MONO)
                                .text_size(px(12.))
                                .child(Textarea::new(&editor).h(px(170.))),
                        )
                    })
                    .child(
                        div().flex().gap_2().child(
                            Button::new("apply-json")
                                .label("Guardar acción JSON")
                                .disabled(b == ButtonId::MouseMode)
                                .small()
                                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                    let Some(editor) = this.action_editor.clone() else {
                                        return;
                                    };
                                    let text = editor.read(cx).value();
                                    let key = this.action_editor_key.clone();
                                    let model = serde_json::to_string_pretty(this.studio.action(this.selected, this.profile)).unwrap_or_default();
                                    this.drafts.sync(&key, &model, Some((&key, text.as_str())));
                                    let r = this.drafts.check_save(&key, &model).and_then(|_| match serde_json::from_str::<Action>(&text) {
                                        Ok(action) => {
                                            let label = action_name(&action);
                                            this.studio.assign_action(
                                                this.selected,
                                                this.profile,
                                                action,
                                            ).map(|_|format!("Guardado: {label}"))
                                        }
                                        Err(e) => Err(format!("Invalid action JSON: {e}")),
                                    });
                                    if r.is_ok() { this.drafts.discard(&key); this.action_editor_key.clear(); }
                                    this.report("json", r);
                                    cx.notify();
                                })),
                        ),
                    )
                    .when_some(self.message("json").cloned(), |d, m| d.child(notice(&m)))
                    .child(caption("El control funciona automáticamente al abrir Studio. Pausar control, arriba, pausa todas las funciones a la vez."))
                    .when_some(self.message("rules").cloned(), |d, m| d.child(notice(&m))),
            )
    }
}
