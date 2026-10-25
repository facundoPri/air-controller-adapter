//! Callout placement for the 3D remote: each label sits beside its projected
//! button with a short leader instead of in a fixed column.
//!
//! Pure geometry so placement can be tested across stage sizes and camera
//! angles. Labels are placed greedily (focused buttons first) into the
//! cheapest free slot around their anchor. A slot is cheap when it is close to
//! the button, on the button's preferred side and off the remote's body. A
//! per-button memory keeps the previous slot while it stays valid, so labels
//! glide with the model instead of jumping between sides during rotation.

use super::remote_scene::{self, Camera, Side};
use gpui_kit::{Pixels, Point, Size, point, px};

/// Two-line label: button name and current action.
pub const FULL_H: f32 = 38.;
/// One-line label (button name only), used when a full label has no room.
pub const COMPACT_H: f32 = 24.;
pub const MAX_W: f32 = 172.;
/// Smallest label, keeping a comfortable click target for short names.
const MIN_W: f32 = 44.;
pub const PAD_X: f32 = 10.;
/// Clear space between two labels.
const GAP: f32 = 4.;
/// Stage padding kept clear of labels.
const MARGIN: f32 = 8.;
/// Strip at the bottom of the stage reserved for the view controls.
pub const CONTROLS_H: f32 = 44.;
/// Labels keep this far from hotspot ring centres (ring outer radius 7.75).
const RING_CLEAR: f32 = 11.;
/// Distance between successive candidate rings around an anchor.
const STEP: f32 = 14.;
const LEVELS: u8 = 12;
/// Cost advantage of the slot a label used in the previous frame.
const STICKY: f32 = 60.;
/// A one-line label must save at least this much leader to replace a full one.
const COMPACT_COST: f32 = 40.;
/// Stages narrower than this favour one-line labels over long leaders.
const SMALL_W: f32 = 520.;
/// Below this stage width only focused buttons are labelled.
const DENSE_MIN_W: f32 = 380.;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    fn right(&self) -> f32 {
        self.x + self.w
    }
    fn bottom(&self) -> f32 {
        self.y + self.h
    }
    fn overlaps(&self, o: &Rect, gap: f32) -> bool {
        self.x < o.right() + gap
            && o.x < self.right() + gap
            && self.y < o.bottom() + gap
            && o.y < self.bottom() + gap
    }
    fn inside(&self, b: &Rect) -> bool {
        self.x >= b.x && self.y >= b.y && self.right() <= b.right() && self.bottom() <= b.bottom()
    }
    /// Closest point of the rectangle to `(x, y)`.
    fn nearest(&self, x: f32, y: f32) -> (f32, f32) {
        (
            x.clamp(self.x, self.right()),
            y.clamp(self.y, self.bottom()),
        )
    }
    fn distance(&self, x: f32, y: f32) -> f32 {
        let (nx, ny) = self.nearest(x, y);
        (nx - x).hypot(ny - y)
    }
    fn overlap_width(&self, o: &Rect) -> f32 {
        let w = (self.right().min(o.right()) - self.x.max(o.x)).max(0.);
        let h = (self.bottom().min(o.bottom()) - self.y.max(o.y)).max(0.);
        w * h / self.h
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dir {
    East,
    West,
    North,
    South,
}

/// Identity of a candidate position relative to its anchor, remembered
/// between frames for stability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    dir: Dir,
    level: u8,
    shift: i8,
    compact: bool,
}

/// Previous slot per button, indexed like `ButtonId::ALL`.
pub type Memory = [Option<Slot>; remote_scene::BUTTON_COUNT];

#[derive(Clone, Debug)]
pub struct Placed {
    pub index: usize,
    pub anchor: Point<Pixels>,
    pub rect: Rect,
    pub compact: bool,
    slot: Slot,
    cost: f32,
}

impl Placed {
    /// Where the leader meets the label: the label point nearest the anchor.
    pub fn leader_end(&self) -> Point<Pixels> {
        let (x, y) = self
            .rect
            .nearest(f32::from(self.anchor.x), f32::from(self.anchor.y));
        point(px(x), px(y))
    }

    /// True when the label sits left of its button; its text then aligns
    /// toward the button.
    pub fn leftward(&self) -> bool {
        self.slot.dir == Dir::West
    }

    #[cfg(test)]
    fn leader_len(&self) -> f32 {
        self.rect
            .distance(f32::from(self.anchor.x), f32::from(self.anchor.y))
    }
}

/// Horizontal space around label text: padding on both sides, the 1 px
/// border on both sides, and 2 px for subpixel rounding in text layout.
pub const CHROME_W: f32 = 2. * PAD_X + 2. + 2.;

/// Label widths `(full, compact)` from measured text widths in the label's
/// title and subtitle styles. Both always fit the whole button name; only an
/// action name longer than `MAX_W` allows is truncated.
pub fn label_size(title: f32, subtitle: f32) -> (f32, f32) {
    let compact = (title + CHROME_W).max(MIN_W);
    let full = (title.max(subtitle) + CHROME_W).min(MAX_W).max(compact);
    (full, compact)
}

fn candidates(
    ax: f32,
    ay: f32,
    w: f32,
    h: f32,
    side: Side,
    compact: bool,
) -> Vec<(Slot, Rect, f32)> {
    let mut out = Vec::with_capacity(LEVELS as usize * 24);
    let preferred = if side == Side::Left {
        Dir::West
    } else {
        Dir::East
    };
    for level in 0..LEVELS {
        let d = RING_CLEAR + level as f32 * STEP;
        for shift in [0i8, 1, -1, 2, -2, 3, -3] {
            let y = ay - h / 2. + shift as f32 * (h / 2. + GAP);
            for (dir, x) in [(Dir::East, ax + d), (Dir::West, ax - d - w)] {
                let slot = Slot {
                    dir,
                    level,
                    shift,
                    compact,
                };
                let penalty =
                    if dir == preferred { 0. } else { 30. } + shift.unsigned_abs() as f32 * 3.;
                out.push((slot, Rect { x, y, w, h }, penalty));
            }
        }
        for shift in [0i8, 1, -1, 2, -2] {
            let x = ax - w / 2. + shift as f32 * w / 4.;
            for (dir, y) in [(Dir::North, ay - d - h), (Dir::South, ay + d)] {
                let slot = Slot {
                    dir,
                    level,
                    shift,
                    compact,
                };
                let penalty = 14. + shift.unsigned_abs() as f32 * 3.;
                out.push((slot, Rect { x, y, w, h }, penalty));
            }
        }
    }
    out
}

/// Fixed inputs for one layout pass.
struct Stage<'a> {
    bounds: Rect,
    body: Rect,
    /// Visible ring centres as `(button, x, y)`.
    rings: Vec<(usize, f32, f32)>,
    spots: Vec<remote_scene::Hotspot>,
    widths: &'a [(f32, f32)],
    memory: &'a Memory,
    /// Extra cost of a one-line label; lower on small stages.
    compact_cost: f32,
}

impl Stage<'_> {
    fn anchor(&self, index: usize) -> (f32, f32) {
        let a = self.spots[index].anchor;
        (f32::from(a.x), f32::from(a.y))
    }

    fn cost(&self, index: usize, slot: Slot, rect: &Rect, penalty: f32) -> f32 {
        let (ax, ay) = self.anchor(index);
        let sticky = if self.memory[index] == Some(slot) {
            STICKY
        } else {
            0.
        };
        rect.distance(ax, ay) + penalty + rect.overlap_width(&self.body) * 0.15 - sticky
    }

    /// Cheapest free slot for `index` among `others`, preferring a full label.
    fn best(
        &self,
        index: usize,
        others: &[Placed],
        allow_compact: bool,
    ) -> Option<(f32, Slot, Rect)> {
        let (ax, ay) = self.anchor(index);
        let side = self.spots[index].side;
        let (full_w, compact_w) = self.widths.get(index).copied().unwrap_or((MAX_W, MAX_W));
        let variants = [(full_w, FULL_H, false), (compact_w, COMPACT_H, true)];
        let mut best: Option<(f32, Slot, Rect)> = None;
        for &(w, h, compact) in &variants[..if allow_compact { 2 } else { 1 }] {
            for (slot, rect, mut penalty) in candidates(ax, ay, w, h, side, compact) {
                if compact {
                    penalty += self.compact_cost;
                }
                if !rect.inside(&self.bounds)
                    || others
                        .iter()
                        .any(|p| p.index != index && p.rect.overlaps(&rect, GAP))
                    || self
                        .rings
                        .iter()
                        .any(|&(i, x, y)| i != index && rect.distance(x, y) < RING_CLEAR)
                {
                    continue;
                }
                let cost = self.cost(index, slot, &rect, penalty);
                if best.is_none_or(|(c, _, _)| cost < c) {
                    best = Some((cost, slot, rect));
                }
            }
        }
        best
    }

    /// A focused label that found no free slot still goes beside its button,
    /// ignoring unfocused labels and foreign rings; only other focused labels
    /// and the stage bounds constrain it.
    fn forced(&self, index: usize, focused: &[Placed]) -> (Slot, Rect) {
        let (ax, ay) = self.anchor(index);
        let side = self.spots[index].side;
        let w = self.widths.get(index).map_or(MAX_W, |w| w.0);
        let all = candidates(ax, ay, w, FULL_H, side, false);
        all.iter()
            .filter(|(_, r, _)| {
                r.inside(&self.bounds) && !focused.iter().any(|p| p.rect.overlaps(r, GAP))
            })
            .min_by(|a, b| (a.1.distance(ax, ay) + a.2).total_cmp(&(b.1.distance(ax, ay) + b.2)))
            .map(|&(s, r, _)| (s, r))
            .unwrap_or_else(|| {
                let (slot, mut rect, _) = all[0];
                rect.w = rect.w.min(self.bounds.w);
                rect.x = rect.x.clamp(self.bounds.x, self.bounds.right() - rect.w);
                rect.y = rect.y.clamp(self.bounds.y, self.bounds.bottom() - rect.h);
                (slot, rect)
            })
    }
}

/// Places labels for `camera` on a stage of `size`. `widths[i]` holds the
/// `(full, compact)` widths for button `i`. Focused buttons (selected first)
/// always get a full label inside the stage; others are labelled when room
/// allows, dropping to a compact label before being left out. Narrow stages
/// only label focused buttons; the inspector stays the accessible alternative.
pub fn place(
    camera: Camera,
    size: Size<Pixels>,
    widths: &[(f32, f32)],
    focus: &[usize],
    memory: &mut Memory,
) -> Vec<Placed> {
    let (width, height) = (f32::from(size.width), f32::from(size.height));
    let bounds = Rect {
        x: MARGIN,
        y: MARGIN,
        w: width - 2. * MARGIN,
        h: height - MARGIN - CONTROLS_H,
    };
    if bounds.w < MIN_W || bounds.h < FULL_H {
        *memory = Default::default();
        return vec![];
    }
    let spots = remote_scene::hotspots(camera, size);
    let b = remote_scene::body_bounds(camera, size);
    let stage = Stage {
        bounds,
        body: Rect {
            x: f32::from(b.origin.x),
            y: f32::from(b.origin.y),
            w: f32::from(b.size.width),
            h: f32::from(b.size.height),
        },
        rings: spots
            .iter()
            .filter(|s| s.visible)
            .map(|s| (s.index, f32::from(s.anchor.x), f32::from(s.anchor.y)))
            .collect(),
        widths,
        memory,
        spots,
        compact_cost: if width < SMALL_W {
            COMPACT_COST / 3.
        } else {
            COMPACT_COST
        },
    };
    let mut focused: Vec<usize> = Vec::new();
    for &i in focus {
        if i < stage.spots.len() && !focused.contains(&i) {
            focused.push(i);
        }
    }

    let mut placed: Vec<Placed> = Vec::with_capacity(stage.spots.len());
    let label = |index: usize, (cost, slot, rect): (f32, Slot, Rect)| Placed {
        index,
        anchor: stage.spots[index].anchor,
        rect,
        compact: slot.compact,
        slot,
        cost,
    };
    for &index in &focused {
        let best = stage.best(index, &placed, false).unwrap_or_else(|| {
            let (slot, rect) = stage.forced(index, &placed);
            (f32::MAX, slot, rect)
        });
        placed.push(label(index, best));
    }
    if width >= DENSE_MIN_W {
        // Outer buttons first: their labels fit just outside the body, and
        // inner buttons then reach past them instead of being boxed in.
        let (cx, cy) = (
            stage.body.x + stage.body.w / 2.,
            stage.body.y + stage.body.h / 2.,
        );
        let mut rest: Vec<usize> = (0..stage.spots.len())
            .filter(|i| !focused.contains(i) && stage.spots[*i].visible)
            .collect();
        rest.sort_by(|&a, &b| {
            let outer = |i: usize| {
                let (x, y) = stage.anchor(i);
                ((x - cx) / stage.body.w.max(1.)).hypot((y - cy) / stage.body.h.max(1.))
            };
            outer(b).total_cmp(&outer(a)).then(a.cmp(&b))
        });
        for &index in &rest {
            if let Some(best) = stage.best(index, &placed, true) {
                placed.push(label(index, best));
            }
        }
        // Greedy order can leave a label boxed in by earlier neighbours. Let a
        // long-leader label take a closer neighbour's room when that neighbour
        // can move somewhere that lowers their combined cost.
        for _ in 0..3 {
            let mut improved = false;
            for &i in &rest {
                let (ax, ay) = stage.anchor(i);
                // A missing label may displace any neighbour within reach.
                let (current, reach) = match placed.iter().find(|p| p.index == i) {
                    Some(p) => (p.cost, p.rect.distance(ax, ay)),
                    None => match stage.best(i, &placed, true) {
                        Some(best) => {
                            placed.push(label(i, best));
                            improved = true;
                            continue;
                        }
                        None => (1e5, 160.),
                    },
                };
                if current < 30. {
                    continue;
                }
                let blockers: Vec<usize> = placed
                    .iter()
                    .filter(|p| p.index != i && !focused.contains(&p.index))
                    .filter(|p| p.rect.distance(ax, ay) < reach)
                    .map(|p| p.index)
                    .collect();
                for j in blockers {
                    let mut others: Vec<Placed> = placed
                        .iter()
                        .filter(|p| p.index != i && p.index != j)
                        .cloned()
                        .collect();
                    let Some(bi) = stage.best(i, &others, true).filter(|b| b.0 < current - 1.)
                    else {
                        continue;
                    };
                    others.push(label(i, bi));
                    let Some(bj) = stage.best(j, &others, true) else {
                        continue;
                    };
                    let old_j = placed.iter().find(|p| p.index == j).map_or(0., |p| p.cost);
                    if bi.0 + bj.0 < current + old_j - 1. {
                        others.push(label(j, bj));
                        placed = others;
                        improved = true;
                        break;
                    }
                }
            }
            if !improved {
                break;
            }
        }
        placed.sort_by_key(|p| {
            (
                !focused.contains(&p.index),
                focused.iter().position(|&f| f == p.index),
                p.index,
            )
        });
    }
    let mut next = Memory::default();
    for p in &placed {
        next[p.index] = Some(p.slot);
    }
    *memory = next;
    placed
}

#[cfg(test)]
mod tests {
    use super::*;

    /// macOS system-font text widths in the label styles (names 13.5 px
    /// semibold, actions 11.5 px regular), measured once with AppKit's
    /// `NSFont.systemFont`, the face behind GPUI's `.SystemUIFont`.
    pub(super) const NAMES: [(&str, f32); 14] = [
        ("Home", 38.39),
        ("Experiences", 79.80),
        ("Apps", 33.69),
        ("Back", 32.13),
        ("Up", 18.48),
        ("Down", 36.80),
        ("Left", 25.73),
        ("Right", 34.46),
        ("OK", 19.64),
        ("Capture", 52.39),
        ("Air mouse", 65.62),
        ("Volume −", 60.81),
        ("Volume +", 60.81),
        ("Mute", 33.10),
    ];
    const ACTIONS: [(&str, f32); 7] = [
        ("Original function", 90.37),
        ("Disabled", 47.44),
        ("Mission Control", 84.73),
        ("Apps / Launchpad", 98.24),
        ("Mute", 27.56),
        ("Toggles R-Mouse", 95.30),
        ("Press it on the remote…", 129.26),
    ];
    const ORIGINAL_FUNCTION: f32 = 90.37;

    pub(super) fn widths() -> Vec<(f32, f32)> {
        NAMES
            .iter()
            .map(|&(_, w)| label_size(w, ORIGINAL_FUNCTION))
            .collect()
    }

    fn sweep() -> impl Iterator<Item = Camera> {
        (0..=26).flat_map(|yi| {
            (0..=12).map(move |pi| Camera {
                yaw: -1.3 + yi as f32 * 0.1,
                pitch: 0.05 + pi as f32 * (1.15 / 12.),
            })
        })
    }

    fn assert_valid(labels: &[Placed], w: f32, h: f32, context: &str) {
        let widths = widths();
        for p in labels {
            let r = p.rect;
            // Placement never narrows a label below its whole button name.
            let (full, compact) = widths[p.index];
            assert_eq!(
                r.w,
                if p.compact { compact } else { full },
                "label {} resized {context}",
                p.index
            );
            assert!(
                r.w - 2. * PAD_X - 2. > NAMES[p.index].1,
                "label {} truncates {context}",
                p.index
            );
            assert!(
                r.x >= 0. && r.y >= 0. && r.right() <= w && r.bottom() <= h - CONTROLS_H + MARGIN,
                "label {} leaves the stage {context}",
                p.index
            );
            // The label never hides its own button, so clicking it still hits the model.
            let (ax, ay) = (f32::from(p.anchor.x), f32::from(p.anchor.y));
            assert!(
                r.distance(ax, ay) >= RING_CLEAR - 0.01,
                "label {} covers its ring {context}",
                p.index
            );
            for q in labels.iter().filter(|q| q.index != p.index) {
                assert!(
                    !r.overlaps(&q.rect, 0.),
                    "labels {} and {} overlap {context}",
                    p.index,
                    q.index
                );
            }
        }
    }

    // gpui_kit's glob import shadows the std `test` attribute.
    #[::core::prelude::v1::test]
    fn desktop_stages_label_every_button_close_to_it() {
        let widths = widths();
        // The minimum window leaves a stage of about 390×640; the default 710×780.
        // Budgets: (width, height, median of per-view mean leaders, worst mean).
        for (w, h, typical, worst) in [
            (390., 640., 45., 75.),
            (500., 500., 40., 60.),
            (610., 620., 30., 60.),
            (710., 780., 30., 60.),
            (900., 800., 30., 60.),
        ] {
            let mut means = vec![];
            for camera in sweep() {
                let mut memory = Memory::default();
                let labels = place(
                    camera,
                    gpui_kit::size(px(w), px(h)),
                    &widths,
                    &[9],
                    &mut memory,
                );
                let context = format!("at {w}×{h}, {camera:?}");
                assert_eq!(labels.len(), 14, "missing labels {context}");
                assert_valid(&labels, w, h, &context);
                assert_eq!(labels[0].index, 9);
                assert!(!labels[0].compact, "selected label is compact {context}");
                for p in &labels {
                    assert!(
                        p.leader_len() <= 170.,
                        "leader of {} is {} {context}",
                        p.index,
                        p.leader_len()
                    );
                }
                let mean = labels.iter().map(Placed::leader_len).sum::<f32>() / 14.;
                assert!(mean <= worst, "mean leader {mean} {context}");
                means.push(mean);
            }
            // Typical views keep labels right beside their buttons.
            means.sort_by(f32::total_cmp);
            let median = means[means.len() / 2];
            assert!(median <= typical, "median leader {median} at {w}×{h}");
        }
    }

    #[::core::prelude::v1::test]
    fn crowded_stages_keep_selected_label_and_never_overlap() {
        let widths = widths();
        for (w, h) in [(306., 480.), (420., 420.), (500., 500.), (240., 260.)] {
            for camera in sweep() {
                for selected in [0, 8, 10, 13] {
                    let mut memory = Memory::default();
                    let labels = place(
                        camera,
                        gpui_kit::size(px(w), px(h)),
                        &widths,
                        &[selected, 4],
                        &mut memory,
                    );
                    let context = format!("at {w}×{h}, {camera:?}, selected {selected}");
                    assert_valid(&labels, w, h, &context);
                    assert_eq!(
                        labels[0].index, selected,
                        "selected label missing {context}"
                    );
                    assert!(!labels[0].compact);
                }
            }
        }
    }

    #[::core::prelude::v1::test]
    fn narrow_stage_labels_only_focused_buttons() {
        let labels = place(
            Camera::default().rotated(1.3, 0.58),
            gpui_kit::size(px(306.), px(480.)),
            &widths(),
            &[5],
            &mut Memory::default(),
        );
        assert_eq!(labels.iter().map(|p| p.index).collect::<Vec<_>>(), [5]);
    }

    /// Continuous rotation (dragging or Follow movement) moves labels with
    /// their buttons. A jump is a label moving more than 24 px relative to its
    /// anchor between frames, or switching between full and compact.
    #[::core::prelude::v1::test]
    fn labels_do_not_jump_during_continuous_rotation() {
        let widths = widths();
        let size = gpui_kit::size(px(610.), px(620.));
        let count = |sticky: bool| {
            let mut memory = Memory::default();
            let mut previous: Option<Vec<Placed>> = None;
            let (mut jumps, mut frames) = (0, 0);
            for k in 0..=520 {
                let camera = Camera {
                    yaw: -1.3 + k as f32 * 0.005,
                    pitch: 0.62 + 0.3 * (k as f32 * 0.02).sin(),
                };
                if !sticky {
                    memory = Memory::default();
                }
                let labels = place(camera, size, &widths, &[8], &mut memory);
                if let Some(prev) = &previous {
                    for p in &labels {
                        frames += 1;
                        let jumped = prev.iter().find(|q| q.index == p.index).is_none_or(|q| {
                            let dx = (p.rect.x - q.rect.x) - f32::from(p.anchor.x - q.anchor.x);
                            let dy = (p.rect.y - q.rect.y) - f32::from(p.anchor.y - q.anchor.y);
                            dx.hypot(dy) > 24. || p.compact != q.compact
                        });
                        jumps += jumped as usize;
                    }
                }
                previous = Some(labels);
            }
            (jumps, frames)
        };
        let (sticky, frames) = count(true);
        let (stateless, _) = count(false);
        assert!(
            sticky * 50 <= frames,
            "{sticky} jumps in {frames} label frames"
        );
        assert!(
            sticky < stateless,
            "memory does not reduce jumps: {sticky} vs {stateless}"
        );
    }

    /// Regression: the compact "Down" label rendered as "Do…" because its
    /// box was estimated narrower than the shaped text plus padding.
    #[::core::prelude::v1::test]
    fn labels_fit_whole_measured_names() {
        for (name, title) in NAMES {
            for (action, subtitle) in ACTIONS {
                let (full, compact) = label_size(title, subtitle);
                let content = |w: f32| w - 2. * PAD_X - 2.;
                assert!(content(compact) > title, "compact {name} truncates");
                assert!(content(full) > title, "full {name} truncates with {action}");
                assert!(
                    content(full) > subtitle || full == MAX_W,
                    "{action} truncates below the maximum width"
                );
                assert!(compact >= MIN_W && full <= MAX_W && compact <= full);
            }
        }
        // The selected button's identification prompt fits as well.
        let (full, _) = label_size(36.80, 129.26);
        assert!(full - 2. * PAD_X - 2. > 129.26);
    }
}

#[cfg(test)]
mod previews {
    use super::*;
    use crate::ui::remote_scene::{Highlight, tests as scene};

    /// Paints label boxes and leaders over the software scene render. The
    /// boxes carry no text: this checks proximity, overlap and bounds.
    fn preview(camera: Camera, w: usize, h: usize, focus: &[usize], name: &str) {
        let widths = super::tests::widths();
        let size = gpui_kit::size(px(w as f32), px(h as f32));
        let labels = place(camera, size, &widths, focus, &mut Memory::default());
        let highlight = Highlight {
            selected: focus.first().copied(),
            ..Default::default()
        };
        let mut img = scene::rasterize(camera, highlight, w, h);
        let mut put = |x: f32, y: f32, c: [f32; 3]| {
            let (x, y) = (x.round() as isize, y.round() as isize);
            if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                img[y as usize * w + x as usize] = c;
            }
        };
        for p in &labels {
            let selected = focus.first() == Some(&p.index);
            let edge = if selected {
                [0.1, 0.9, 0.83]
            } else {
                [0.45, 0.45, 0.45]
            };
            let r = p.rect;
            for yy in 0..r.h as usize {
                for xx in 0..r.w as usize {
                    let border =
                        xx == 0 || yy == 0 || xx + 1 >= r.w as usize || yy + 1 >= r.h as usize;
                    put(
                        r.x + xx as f32,
                        r.y + yy as f32,
                        if border { edge } else { [0.06; 3] },
                    );
                }
            }
            let (ax, ay) = (f32::from(p.anchor.x), f32::from(p.anchor.y));
            let end = p.leader_end();
            let (ex, ey) = (f32::from(end.x), f32::from(end.y));
            let steps = (ex - ax).hypot(ey - ay).ceil().max(1.) as usize;
            for k in 0..=steps {
                let t = k as f32 / steps as f32;
                put(ax + (ex - ax) * t, ay + (ey - ay) * t, edge);
            }
        }
        scene::write_bmp(
            &scene::preview_dir().join(format!("callouts-{name}.bmp")),
            w,
            h,
            &img,
        );
    }

    #[::core::prelude::v1::test]
    #[ignore = "writes preview images to target/scene-renders"]
    fn render_callout_previews() {
        let d = Camera::default();
        preview(d, 710, 780, &[9], "default");
        preview(d.rotated(0.9, 0.3), 710, 780, &[8], "rotated");
        preview(
            Camera {
                yaw: 1.2,
                pitch: 0.62,
            },
            610,
            620,
            &[0],
            "sideways",
        );
        preview(
            Camera {
                yaw: 0.0,
                pitch: 1.15,
            },
            710,
            780,
            &[4],
            "oblique",
        );
        preview(d, 390, 640, &[9], "minimum-window");
        preview(d.rotated(1.3, 0.58), 306, 480, &[5], "narrow");
    }
}
