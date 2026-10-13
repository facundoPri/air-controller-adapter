//! 3D Motorola MR-01 ("Ready For Air Remote") scene, owned by the 3D agent.
//!
//! Rendering approach: a procedural triangle mesh in model space (millimetres,
//! laid out from the button diagram in `docs/reference/mr-01-motorola-es.pdf`)
//! is transformed on the CPU by an orbit camera, projected with a perspective
//! projection, back-face culled, shaded per face (key + fill + rim + Blinn
//! specular) and painter-sorted far to near. Every resulting screen triangle is
//! handed to GPUI as a `Path` and rasterized on the GPU via `paint_path`.
//!
//! Button indices follow `studio::ButtonId::ALL` order:
//! 0 Home, 1 Experiences, 2 Apps, 3 Back, 4 Up, 5 Down, 6 Left, 7 Right,
//! 8 Ok, 9 Capture, 10 MouseMode, 11 VolumeDown, 12 VolumeUp, 13 Mute.

use std::f32::consts::{FRAC_PI_2, PI, TAU};
use std::sync::LazyLock;

use gpui_kit::*;

pub const BUTTON_COUNT: usize = 14;

/// Orbit camera around the remote. Angles in radians.
///
/// The remote lies on a table. `yaw` spins it around the table normal and
/// `pitch` tilts the view away from straight top-down (0 = top-down,
/// larger = lower, more oblique camera).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            yaw: -0.2,
            pitch: 0.62,
        }
    }
}

impl Camera {
    /// Returns a copy rotated by the given deltas, clamped to the supported range.
    pub fn rotated(self, d_yaw: f32, d_pitch: f32) -> Self {
        Self {
            yaw: (self.yaw + d_yaw).clamp(-1.3, 1.3),
            pitch: (self.pitch + d_pitch).clamp(0.05, 1.2),
        }
    }
}

/// Which side of the anchor a callout label should sit on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// Projected hotspot for one physical button, relative to the canvas origin.
#[derive(Clone, Copy, Debug)]
pub struct Hotspot {
    pub index: usize,
    pub anchor: Point<Pixels>,
    /// False when the button faces away from the camera (hide or dim the callout).
    pub visible: bool,
    pub side: Side,
}

/// Visual state passed to the painter.
#[derive(Clone, Copy, Debug, Default)]
pub struct Highlight {
    pub selected: Option<usize>,
    pub hovered: Option<usize>,
    /// Button that was just observed from hardware (brief glow).
    pub pressed: Option<usize>,
}

/// Canvas element that paints the remote; fills its parent.
pub fn scene(camera: Camera, highlight: Highlight) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let ox = f32::from(bounds.origin.x);
            let oy = f32::from(bounds.origin.y);
            let tris = shaded_triangles(
                camera,
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
                highlight,
            );
            let pt = |p: [f32; 2]| point(px(p[0] + ox), px(p[1] + oy));
            let st = (point(0., 1.), point(0., 1.), point(0., 1.));
            let mut i = 0;
            while i < tris.len() {
                let first = tris[i];
                let mut path = Path::new(pt(first.pts[0]));
                let mut j = i;
                while j < tris.len() && tris[j].rgba == first.rgba && tris[j].key == first.key {
                    let s = tris[j].pts;
                    path.push_triangle((pt(s[0]), pt(s[1]), pt(s[2])), st);
                    j += 1;
                }
                let [r, g, b, a] = first.rgba;
                window.paint_path(path, Rgba { r, g, b, a });
                i = j;
            }
        },
    )
    .size_full()
}

/// Hotspot anchors for a canvas of `size`, using the same projection as `scene`.
pub fn hotspots(camera: Camera, size: Size<Pixels>) -> Vec<Hotspot> {
    let w = f32::from(size.width);
    let h = f32::from(size.height);
    projected_hotspots(&View::new(camera, w, h), w, h)
        .into_iter()
        .map(|s| Hotspot {
            index: s.index,
            anchor: point(px(s.pos[0]), px(s.pos[1])),
            visible: s.visible,
            side: s.side,
        })
        .collect()
}

/// Screen-space box around the button-bearing part of the body (top to just
/// below the volume pill) for a canvas of `size`, canvas-relative, so callout
/// columns can sit outside the silhouette next to the buttons they label.
pub fn body_bounds(camera: Camera, size: Size<Pixels>) -> Bounds<Pixels> {
    let view = View::new(camera, f32::from(size.width), f32::from(size.height));
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for x in [-HALF_W, HALF_W] {
        for y in [VOLUME[1] - 2.0 * VOLUME_R, HALF_L] {
            for z in [0.0, THICK] {
                let p = view.project_model([x, y, z]);
                lo = [lo[0].min(p[0]), lo[1].min(p[1])];
                hi = [hi[0].max(p[0]), hi[1].max(p[1])];
            }
        }
    }
    Bounds::from_corners(point(px(lo[0]), px(lo[1])), point(px(hi[0]), px(hi[1])))
}

/// Button geometry under `position` (canvas-relative), if any.
pub fn hit_test(camera: Camera, size: Size<Pixels>, position: Point<Pixels>) -> Option<usize> {
    let view = View::new(camera, f32::from(size.width), f32::from(size.height));
    let q = [f32::from(position.x), f32::from(position.y)];
    let mut best: Option<(f32, usize)> = None;
    for t in MODEL.tris.iter().filter(|t| t.hit) {
        let Some(button) = t.button else { continue };
        let v = t.v.map(|p| view.rot(p));
        let c = centroid(v[0], v[1], v[2]);
        if dot(view.rot_dir(t.n), view.to_cam(c)) <= 0.0 {
            continue;
        }
        let p = v.map(|p| view.project(p));
        if point_in_tri(p, q) {
            let depth = -c[2];
            if best.is_none_or(|(d, _)| depth < d) {
                best = Some((depth, button as usize));
            }
        }
    }
    best.map(|(_, b)| b)
}

// ---------------------------------------------------------------------------
// Geometry constants (millimetres). Layout traced from the MR-01 manual diagram.

const HALF_W: f32 = 19.0;
const HALF_L: f32 = 77.5;
const THICK: f32 = 14.0;
const CORNER: f32 = 10.0;
const TOP_FILLET: f32 = 3.5;
const BOTTOM_FILLET: f32 = 5.0;
const SEAM_Z: f32 = 5.6;
const DOME: f32 = 0.6;
const N_LONG: usize = 12;
const N_SHORT: usize = 4;
const GRID_X: usize = 6;
const GRID_Y: usize = 24;
const CORNER_SEGS: usize = 5;
const BUTTON_SEGS: usize = 18;

const DPAD: [f32; 2] = [0.0, 20.7];
const DPAD_RING_IN: f32 = 7.0;
const DPAD_RING_OUT: f32 = 15.0;
const DPAD_ARROW_R: f32 = 11.2;
const OK_R: f32 = 6.0;
const VOLUME: [f32; 2] = [0.0, -12.9];
const VOLUME_HALF: f32 = 9.5;
const VOLUME_R: f32 = 3.3;
const LOGO: [f32; 2] = [0.0, -54.6];

type V3 = [f32; 3];
type P2 = [f32; 2];
/// Outline sample: base point `C` and outward unit normal `N`; a ring at
/// offset `r` places the vertex at `C + r * N`.
type OutlinePt = (P2, P2);

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn normalize(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    if l > 1e-9 { mul(a, 1.0 / l) } else { a }
}
fn centroid(a: V3, b: V3, c: V3) -> V3 {
    mul(add(add(a, b), c), 1.0 / 3.0)
}
fn mix3(a: V3, b: V3, t: f32) -> V3 {
    add(mul(a, 1.0 - t), mul(b, t))
}

fn dome(x: f32, y: f32) -> f32 {
    let u = x / (HALF_W - TOP_FILLET);
    let v = y / (HALF_L - TOP_FILLET);
    DOME * (1.0 - u * u).max(0.0) * (1.0 - v.powi(6)).max(0.0)
}

/// Height of the body's top surface at (x, y).
fn top_z(c: P2) -> f32 {
    THICK + dome(c[0], c[1])
}

// ---------------------------------------------------------------------------
// Mesh

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mat {
    Body,
    Seam,
    Button,
    Glyph,
    GlyphDim,
    Logo,
    Led,
}

#[derive(Clone, Copy, Debug)]
struct Tri {
    v: [V3; 3],
    /// Outward unit normal (model space).
    n: V3,
    mat: Mat,
    /// Draw group: 0 = body (+ its decals); every button solid has its own group.
    group: u8,
    /// Paint order inside a group (solid, secondary solid, decals).
    layer: u8,
    button: Option<u8>,
    /// Part of a button's top face (used by `hit_test`).
    hit: bool,
}

struct Glow {
    button: u8,
    /// Painted just before this (group, layer) so the button covers its inside.
    group: u8,
    layer: u8,
    outline: Vec<OutlinePt>,
    r: f32,
    z: f32,
}

struct Model {
    tris: Vec<Tri>,
    group_centres: Vec<V3>,
    anchors: [V3; BUTTON_COUNT],
    glows: Vec<Glow>,
    body_outline: Vec<OutlinePt>,
}

static MODEL: LazyLock<Model> = LazyLock::new(build_model);

#[derive(Clone, Copy, PartialEq)]
enum Split {
    None,
    Dpad,
    Volume,
}

struct Builder {
    tris: Vec<Tri>,
    group: u8,
    layer: u8,
    mat: Mat,
    button: Option<u8>,
    hit: bool,
    split: Split,
}

impl Builder {
    fn tri(&mut self, a: V3, b: V3, c: V3, interior: V3) {
        let n = cross(sub(b, a), sub(c, a));
        if dot(n, n) < 1e-10 {
            return;
        }
        let mut n = normalize(n);
        let cen = centroid(a, b, c);
        if dot(n, sub(cen, interior)) < 0.0 {
            n = mul(n, -1.0);
        }
        let button = match self.split {
            Split::None => self.button,
            Split::Dpad => Some(dpad_direction(cen[0] - DPAD[0], cen[1] - DPAD[1])),
            Split::Volume => Some(if cen[0] < VOLUME[0] { 11 } else { 12 }),
        };
        self.tris.push(Tri {
            v: [a, b, c],
            n,
            mat: self.mat,
            group: self.group,
            layer: self.layer,
            button,
            hit: self.hit,
        });
    }

    // ---- flat decals (normal +z), painted after their base surface ----

    fn decal_tri(&mut self, a: P2, b: P2, c: P2, z: f32) {
        self.tri(
            [a[0], a[1], z],
            [b[0], b[1], z],
            [c[0], c[1], z],
            [a[0], a[1], z - 1.0],
        );
    }

    fn decal_poly(&mut self, pts: &[P2], z: f32) {
        for i in 1..pts.len() - 1 {
            self.decal_tri(pts[0], pts[i], pts[i + 1], z);
        }
    }

    fn decal_disc(&mut self, c: P2, r: f32, segs: usize, z: f32) {
        let pts: Vec<P2> = (0..segs)
            .map(|i| {
                let a = TAU * i as f32 / segs as f32;
                [c[0] + r * a.cos(), c[1] + r * a.sin()]
            })
            .collect();
        self.decal_poly(&pts, z);
    }

    fn decal_ring(&mut self, c: P2, r0: f32, r1: f32, segs: usize, z: f32) {
        for i in 0..segs {
            let a0 = TAU * i as f32 / segs as f32;
            let a1 = TAU * (i + 1) as f32 / segs as f32;
            let p = |r: f32, a: f32| [c[0] + r * a.cos(), c[1] + r * a.sin()];
            self.decal_tri(p(r0, a0), p(r1, a0), p(r1, a1), z);
            self.decal_tri(p(r0, a0), p(r1, a1), p(r0, a1), z);
        }
    }

    fn decal_seg(&mut self, a: P2, b: P2, w: f32, z: f32) {
        let d = [b[0] - a[0], b[1] - a[1]];
        let l = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-6);
        let n = [-d[1] / l * w * 0.5, d[0] / l * w * 0.5];
        self.decal_poly(
            &[
                [a[0] + n[0], a[1] + n[1]],
                [b[0] + n[0], b[1] + n[1]],
                [b[0] - n[0], b[1] - n[1]],
                [a[0] - n[0], a[1] - n[1]],
            ],
            z,
        );
    }

    fn decal_polyline(&mut self, pts: &[P2], w: f32, closed: bool, z: f32) {
        let n = pts.len();
        let segs = if closed { n } else { n - 1 };
        for i in 0..segs {
            self.decal_seg(pts[i], pts[(i + 1) % n], w, z);
        }
        let joints = if closed { 0..n } else { 1..n - 1 };
        for i in joints {
            self.decal_disc(pts[i], w * 0.5, 8, z);
        }
    }
}

fn dpad_direction(dx: f32, dy: f32) -> u8 {
    if dy.abs() >= dx.abs() {
        if dy > 0.0 { 4 } else { 5 }
    } else if dx < 0.0 {
        6
    } else {
        7
    }
}

#[derive(Clone, Copy)]
struct Ring {
    r: f32,
    z: f32,
    /// Weight of the body dome added to `z`.
    dome: f32,
    /// Material / hit flag of the band from this ring to the next.
    mat: Mat,
    hit: bool,
}

fn ring(r: f32, z: f32, mat: Mat) -> Ring {
    Ring {
        r,
        z,
        dome: 0.0,
        mat,
        hit: false,
    }
}

fn ring_point(ring: &Ring, (c, n): OutlinePt) -> V3 {
    let x = c[0] + ring.r * n[0];
    let y = c[1] + ring.r * n[1];
    [x, y, ring.z + ring.dome * dome(x, y)]
}

fn extrude(b: &mut Builder, outline: &[OutlinePt], rings: &[Ring], interior: V3) {
    for k in 0..rings.len() - 1 {
        b.mat = rings[k].mat;
        b.hit = rings[k].hit;
        for i in 0..outline.len() {
            let o0 = outline[i];
            let o1 = outline[(i + 1) % outline.len()];
            let p00 = ring_point(&rings[k], o0);
            let p01 = ring_point(&rings[k], o1);
            let p11 = ring_point(&rings[k + 1], o1);
            let p10 = ring_point(&rings[k + 1], o0);
            b.tri(p00, p01, p11, interior);
            b.tri(p00, p11, p10, interior);
        }
    }
}

fn cap_fan(b: &mut Builder, outline: &[OutlinePt], last: &Ring, centre: P2, interior: V3) {
    b.mat = last.mat;
    b.hit = last.hit;
    let c = [centre[0], centre[1], last.z];
    for i in 0..outline.len() {
        let p0 = ring_point(last, outline[i]);
        let p1 = ring_point(last, outline[(i + 1) % outline.len()]);
        b.tri(c, p0, p1, interior);
    }
}

fn circle_outline(c: P2, segs: usize) -> Vec<OutlinePt> {
    (0..segs)
        .map(|i| {
            let a = TAU * i as f32 / segs as f32;
            (c, [a.cos(), a.sin()])
        })
        .collect()
}

fn stadium_outline(c: P2, half: f32, straight_segs: usize, arc_segs: usize) -> Vec<OutlinePt> {
    let mut out = Vec::new();
    for (sx, a0) in [(1.0, -FRAC_PI_2), (-1.0, FRAC_PI_2)] {
        for i in 0..=arc_segs {
            let a = a0 + PI * i as f32 / arc_segs as f32;
            out.push(([c[0] + sx * half, c[1]], [a.cos(), a.sin()]));
        }
        for i in 1..straight_segs {
            let x = sx * half * (1.0 - 2.0 * i as f32 / straight_segs as f32);
            out.push(([c[0] + x, c[1]], [0.0, sx]));
        }
    }
    out
}

/// Rounded rectangle outline around the inner rectangle [-a, a] x [-b, b].
fn rrect_outline(a: f32, b: f32) -> Vec<OutlinePt> {
    let mut out = Vec::new();
    let corner = |out: &mut Vec<OutlinePt>, c: P2, a0: f32| {
        for i in 1..CORNER_SEGS {
            let ang = a0 + FRAC_PI_2 * i as f32 / CORNER_SEGS as f32;
            out.push((c, [ang.cos(), ang.sin()]));
        }
    };
    for j in 0..=N_LONG {
        out.push(([a, -b + 2.0 * b * j as f32 / N_LONG as f32], [1.0, 0.0]));
    }
    corner(&mut out, [a, b], 0.0);
    for k in 0..=N_SHORT {
        out.push(([a - 2.0 * a * k as f32 / N_SHORT as f32, b], [0.0, 1.0]));
    }
    corner(&mut out, [-a, b], FRAC_PI_2);
    for j in 0..=N_LONG {
        out.push(([-a, b - 2.0 * b * j as f32 / N_LONG as f32], [-1.0, 0.0]));
    }
    corner(&mut out, [-a, -b], PI);
    for k in 0..=N_SHORT {
        out.push(([-a + 2.0 * a * k as f32 / N_SHORT as f32, -b], [0.0, -1.0]));
    }
    corner(&mut out, [a, -b], 1.5 * PI);
    out
}

fn build_body(b: &mut Builder, outline: &[OutlinePt]) {
    let interior = [0.0, 0.0, THICK * 0.5];
    let mut rings = Vec::new();
    for i in 0..=3 {
        let phi = FRAC_PI_2 * i as f32 / 3.0;
        rings.push(ring(
            CORNER - BOTTOM_FILLET + BOTTOM_FILLET * phi.sin(),
            BOTTOM_FILLET - BOTTOM_FILLET * phi.cos(),
            Mat::Body,
        ));
    }
    rings.push(ring(CORNER, SEAM_Z - 0.18, Mat::Seam));
    rings.push(ring(CORNER, SEAM_Z + 0.18, Mat::Body));
    for i in 0..=5 {
        let phi = FRAC_PI_2 * i as f32 / 5.0;
        let mut r = ring(
            CORNER - TOP_FILLET + TOP_FILLET * phi.cos(),
            THICK - TOP_FILLET + TOP_FILLET * phi.sin(),
            Mat::Body,
        );
        r.dome = phi.sin();
        rings.push(r);
    }
    for r in [(CORNER - TOP_FILLET) * 0.5, 0.0] {
        let mut r = ring(r, THICK, Mat::Body);
        r.dome = 1.0;
        rings.push(r);
    }
    extrude(b, outline, &rings, interior);

    // Inner top rectangle as a grid so the dome shades smoothly.
    b.mat = Mat::Body;
    b.hit = false;
    let a = HALF_W - CORNER;
    let bb = HALF_L - CORNER;
    // Finer than the outline so the softbox sheen reads as a gradient; the
    // T-junctions on its border are hidden by the triangle expansion.
    let p = |k: usize, j: usize| {
        let x = -a + 2.0 * a * k as f32 / GRID_X as f32;
        let y = -bb + 2.0 * bb * j as f32 / GRID_Y as f32;
        [x, y, THICK + dome(x, y)]
    };
    for k in 0..GRID_X {
        for j in 0..GRID_Y {
            b.tri(p(k, j), p(k + 1, j), p(k + 1, j + 1), interior);
            b.tri(p(k, j), p(k + 1, j + 1), p(k, j + 1), interior);
        }
    }
}

/// Raised button with a rounded top edge. Returns the z of its top face.
fn button_solid(
    b: &mut Builder,
    outline: &[OutlinePt],
    centre: P2,
    radius: f32,
    base: f32,
    height: f32,
) -> f32 {
    let bevel = (radius * 0.22).min(0.7);
    let top = base + height;
    let s = std::f32::consts::FRAC_1_SQRT_2;
    let mut rings = [
        ring(radius, base, Mat::Button),
        ring(radius, top - bevel, Mat::Button),
        ring(
            radius - bevel + bevel * s,
            top - bevel + bevel * s,
            Mat::Button,
        ),
        ring(radius - bevel, top, Mat::Button),
    ];
    for r in &mut rings[1..] {
        r.hit = true;
    }
    let interior = [centre[0], centre[1], base + height * 0.3];
    extrude(b, outline, &rings, interior);
    cap_fan(b, outline, &rings[3], centre, interior);
    top
}

fn build_model() -> Model {
    let mut b = Builder {
        tris: Vec::with_capacity(4096),
        group: 0,
        layer: 0,
        mat: Mat::Body,
        button: None,
        hit: false,
        split: Split::None,
    };
    let mut group_centres = vec![[0.0, 0.0, THICK * 0.5]];
    let mut anchors = [[0.0; 3]; BUTTON_COUNT];
    let mut glows = Vec::new();

    // Body.
    let body_outline = rrect_outline(HALF_W - CORNER, HALF_L - CORNER);
    build_body(&mut b, &body_outline);

    // Body decals: LED pin-holes and the Motorola batwing logo.
    b.layer = 1;
    b.hit = false;
    b.mat = Mat::Led;
    for y in [69.9, 66.6] {
        b.decal_disc([0.0, y], 0.65, 10, top_z([0.0, y]) + 0.04);
    }
    b.mat = Mat::Logo;
    let lz = top_z(LOGO) + 0.05;
    b.decal_ring(LOGO, 4.75, 5.35, 32, lz);
    let m: Vec<P2> = [
        [-0.62, -0.46],
        [-0.47, 0.18],
        [-0.33, 0.42],
        [-0.17, 0.30],
        [0.0, -0.22],
        [0.17, 0.30],
        [0.33, 0.42],
        [0.47, 0.18],
        [0.62, -0.46],
    ]
    .iter()
    .map(|p| [LOGO[0] + p[0] * 5.6, LOGO[1] + p[1] * 5.6])
    .collect();
    b.decal_polyline(&m, 0.62, false, lz);

    // Round buttons: (index, centre, radius, height).
    let rounds: [(u8, P2, f32, f32); 7] = [
        (0, [-12.0, 68.8], 4.1, 1.5),
        (1, [12.0, 68.8], 4.1, 1.5),
        (13, [0.0, 54.6], 5.9, 1.7),
        (2, [-12.0, 40.8], 4.1, 1.5),
        (9, [12.0, 40.8], 4.1, 1.5),
        (3, [-11.6, 1.4], 4.1, 1.5),
        (10, [11.6, 1.4], 4.1, 1.5),
    ];
    for (idx, c, r, h) in rounds {
        b.group = group_centres.len() as u8;
        b.layer = 0;
        b.button = Some(idx);
        b.split = Split::None;
        let base = top_z(c) - 0.35;
        let top = button_solid(&mut b, &circle_outline(c, BUTTON_SEGS), c, r, base, h);
        group_centres.push([c[0], c[1], top]);
        anchors[idx as usize] = [c[0], c[1], top];
        glows.push(Glow {
            button: idx,
            group: b.group,
            layer: 0,
            outline: circle_outline(c, 36),
            r,
            z: top_z(c) + 0.06,
        });
        b.layer = 2;
        b.hit = false;
        b.mat = Mat::Glyph;
        button_glyph(&mut b, idx, c, top + 0.04);
    }

    // D-pad: raised annulus split in four directions + raised OK cylinder.
    b.group = group_centres.len() as u8;
    b.layer = 0;
    b.split = Split::Dpad;
    let base = top_z(DPAD) - 0.35;
    let ring_h = 1.4;
    let bv = 0.6;
    let s = std::f32::consts::FRAC_1_SQRT_2;
    let profile: [(f32, f32, bool); 8] = [
        (DPAD_RING_IN, base, false),
        (DPAD_RING_IN, base + ring_h - bv, true),
        (
            DPAD_RING_IN + bv - bv * s,
            base + ring_h - bv + bv * s,
            true,
        ),
        (DPAD_RING_IN + bv, base + ring_h, true),
        (DPAD_RING_OUT - bv, base + ring_h, true),
        (
            DPAD_RING_OUT - bv + bv * s,
            base + ring_h - bv + bv * s,
            true,
        ),
        (DPAD_RING_OUT, base + ring_h - bv, false),
        (DPAD_RING_OUT, base, false),
    ];
    let segs = 40;
    b.mat = Mat::Button;
    for i in 0..segs {
        let a0 = TAU * i as f32 / segs as f32;
        let a1 = TAU * (i + 1) as f32 / segs as f32;
        let am = (a0 + a1) * 0.5;
        let mid = (DPAD_RING_IN + DPAD_RING_OUT) * 0.5;
        let interior = [
            DPAD[0] + mid * am.cos(),
            DPAD[1] + mid * am.sin(),
            base + ring_h * 0.4,
        ];
        let p = |r: f32, z: f32, a: f32| [DPAD[0] + r * a.cos(), DPAD[1] + r * a.sin(), z];
        for k in 0..profile.len() - 1 {
            let (r0, z0, hit) = profile[k];
            let (r1, z1, _) = profile[k + 1];
            b.hit = hit;
            b.tri(p(r0, z0, a0), p(r0, z0, a1), p(r1, z1, a1), interior);
            b.tri(p(r0, z0, a0), p(r1, z1, a1), p(r1, z1, a0), interior);
        }
    }
    let ring_top = base + ring_h;
    for (idx, ang) in [(4u8, FRAC_PI_2), (5, -FRAC_PI_2), (6, PI), (7, 0.0)] {
        anchors[idx as usize] = [
            DPAD[0] + DPAD_ARROW_R * ang.cos(),
            DPAD[1] + DPAD_ARROW_R * ang.sin(),
            ring_top,
        ];
    }
    // OK cylinder floats slightly above the body inside the ring's hole so its
    // wall never paints over the ring's near top edge.
    b.layer = 1;
    b.split = Split::None;
    b.button = Some(8);
    let ok_top = button_solid(
        &mut b,
        &circle_outline(DPAD, 28),
        DPAD,
        OK_R,
        base + 0.9,
        1.5,
    );
    anchors[8] = [DPAD[0], DPAD[1], ok_top];
    group_centres.push([DPAD[0], DPAD[1], ring_top]);
    glows.push(Glow {
        button: 8,
        group: b.group,
        layer: 1,
        outline: circle_outline(DPAD, 36),
        r: OK_R,
        z: ring_top + 0.05,
    });
    b.layer = 2;
    b.hit = false;
    b.split = Split::Dpad;
    b.mat = Mat::Glyph;
    for ang in [FRAC_PI_2, -FRAC_PI_2, PI, 0.0] {
        let (c, sn) = (ang.cos(), ang.sin());
        let at = |r: f32, t: f32| [DPAD[0] + r * c - t * sn, DPAD[1] + r * sn + t * c];
        b.decal_poly(
            &[
                at(DPAD_ARROW_R + 0.75, 0.0),
                at(DPAD_ARROW_R - 0.55, 0.9),
                at(DPAD_ARROW_R - 0.55, -0.9),
            ],
            ring_top + 0.04,
        );
    }
    b.split = Split::None;
    b.button = Some(8);
    button_glyph(&mut b, 8, DPAD, ok_top + 0.04);

    // Volume rocker pill, split left (down) / right (up).
    b.group = group_centres.len() as u8;
    b.layer = 0;
    b.split = Split::Volume;
    let vbase = top_z(VOLUME) - 0.35;
    let vtop = button_solid(
        &mut b,
        &stadium_outline(VOLUME, VOLUME_HALF, 4, 10),
        VOLUME,
        VOLUME_R,
        vbase,
        1.4,
    );
    group_centres.push([VOLUME[0], VOLUME[1], vtop]);
    anchors[11] = [VOLUME[0] - 7.6, VOLUME[1], vtop];
    anchors[12] = [VOLUME[0] + 7.6, VOLUME[1], vtop];
    b.layer = 2;
    b.hit = false;
    b.mat = Mat::Glyph;
    let gz = vtop + 0.04;
    let (lx, rx, y) = (VOLUME[0] - 8.6, VOLUME[0] + 8.6, VOLUME[1]);
    b.decal_seg([lx - 1.0, y], [lx + 1.0, y], 0.42, gz);
    b.decal_seg([rx - 1.0, y], [rx + 1.0, y], 0.42, gz);
    b.decal_seg([rx, y - 1.0], [rx, y + 1.0], 0.42, gz);
    b.split = Split::None;

    Model {
        tris: b.tris,
        group_centres,
        anchors,
        glows,
        body_outline,
    }
}

/// Light printed icon on top of a button, in button-local millimetres.
fn button_glyph(b: &mut Builder, idx: u8, c: P2, z: f32) {
    let o = |p: P2| [c[0] + p[0], c[1] + p[1]];
    let w = 0.4;
    match idx {
        // Home: house outline.
        0 => {
            let pts: Vec<P2> = [
                [-1.3, -1.4],
                [-1.3, 0.2],
                [0.0, 1.45],
                [1.3, 0.2],
                [1.3, -1.4],
            ]
            .into_iter()
            .map(o)
            .collect();
            b.decal_polyline(&pts, w, true, z);
        }
        // Experiences: 2x2 tiles.
        1 => {
            for (x, y) in [(-0.75, -0.75), (0.75, -0.75), (-0.75, 0.75), (0.75, 0.75)] {
                let h = 0.55;
                b.decal_poly(
                    &[
                        o([x - h, y - h]),
                        o([x + h, y - h]),
                        o([x + h, y + h]),
                        o([x - h, y + h]),
                    ],
                    z,
                );
            }
        }
        // Apps tray: three bars.
        2 => {
            for y in [-0.85, 0.0, 0.85] {
                b.decal_seg(o([-1.25, y]), o([1.25, y]), w, z);
            }
        }
        // Back: left arrow.
        3 => {
            b.decal_seg(o([-1.5, 0.0]), o([1.6, 0.0]), w, z);
            b.decal_polyline(
                &[o([-0.3, 1.2]), o([-1.5, 0.0]), o([-0.3, -1.2])],
                w,
                false,
                z,
            );
        }
        // OK: inner ring + "OK" lettering.
        8 => {
            b.mat = Mat::GlyphDim;
            b.decal_ring(c, 4.7, 5.0, 28, z);
            b.mat = Mat::Glyph;
            b.decal_ring(o([-1.05, 0.0]), 0.68, 1.06, 18, z);
            b.decal_seg(o([0.45, -1.1]), o([0.45, 1.1]), 0.36, z);
            b.decal_seg(o([0.55, -0.05]), o([1.7, 1.1]), 0.36, z);
            b.decal_seg(o([0.85, 0.25]), o([1.75, -1.1]), 0.36, z);
        }
        // Capture: thick ring.
        9 => b.decal_ring(c, 1.2, 1.75, 20, z),
        // Mouse mode: mouse outline with wheel split.
        10 => {
            let pts: Vec<P2> = (0..16)
                .map(|i| {
                    let a = TAU * i as f32 / 16.0;
                    o([1.05 * a.cos(), 1.55 * a.sin()])
                })
                .collect();
            b.decal_polyline(&pts, 0.34, true, z);
            b.decal_seg(o([0.0, 0.35]), o([0.0, 1.5]), 0.34, z);
            b.decal_seg(o([-1.0, 0.35]), o([1.0, 0.35]), 0.3, z);
        }
        // Mute: speaker + cross.
        13 => {
            b.decal_poly(
                &[
                    o([-2.3, -0.7]),
                    o([-1.35, -0.7]),
                    o([-1.35, 0.7]),
                    o([-2.3, 0.7]),
                ],
                z,
            );
            b.decal_poly(
                &[
                    o([-1.35, -0.7]),
                    o([-0.2, -1.75]),
                    o([-0.2, 1.75]),
                    o([-1.35, 0.7]),
                ],
                z,
            );
            b.decal_seg(o([0.6, -0.95]), o([2.3, 0.95]), w, z);
            b.decal_seg(o([0.6, 0.95]), o([2.3, -0.95]), w, z);
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Camera / projection

const CAMERA_DIST: f32 = 520.0;

struct View {
    cy: f32,
    sy: f32,
    cp: f32,
    sp: f32,
    scale: f32,
    cx_px: f32,
    cy_px: f32,
}

impl View {
    fn new(camera: Camera, w: f32, h: f32) -> Self {
        // 168 mm covers the widest yaw (the body turned nearly sideways), so
        // rotating never pushes the remote or its hotspots off the canvas.
        let scale = (h / 172.0).min(w / 168.0).max(0.01);
        Self {
            cy: camera.yaw.cos(),
            sy: camera.yaw.sin(),
            cp: camera.pitch.cos(),
            sp: camera.pitch.sin(),
            scale,
            cx_px: w * 0.5,
            cy_px: h * 0.49,
        }
    }

    fn rot_dir(&self, p: V3) -> V3 {
        let x1 = p[0] * self.cy - p[1] * self.sy;
        let y1 = p[0] * self.sy + p[1] * self.cy;
        [
            x1,
            y1 * self.cp + p[2] * self.sp,
            -y1 * self.sp + p[2] * self.cp,
        ]
    }

    /// Model space -> view space (x right, y up, z toward the camera).
    fn rot(&self, p: V3) -> V3 {
        self.rot_dir([p[0], p[1], p[2] - THICK * 0.5])
    }

    fn to_cam(&self, v: V3) -> V3 {
        normalize([-v[0], -v[1], CAMERA_DIST - v[2]])
    }

    fn project(&self, v: V3) -> P2 {
        let k = self.scale * CAMERA_DIST / (CAMERA_DIST - v[2]);
        [self.cx_px + v[0] * k, self.cy_px - v[1] * k]
    }

    fn project_model(&self, p: V3) -> P2 {
        self.project(self.rot(p))
    }
}

struct ScreenHotspot {
    index: usize,
    pos: P2,
    visible: bool,
    side: Side,
}

fn projected_hotspots(view: &View, w: f32, h: f32) -> Vec<ScreenHotspot> {
    let up = view.rot_dir([0.0, 0.0, 1.0]);
    // How upright the remote's long axis looks on screen: 1 within 30° of
    // vertical, 0 within 30° of horizontal, smoothstep in between.
    let o = view.project_model([0.0, 0.0, THICK]);
    let t = view.project_model([0.0, HALF_L, THICK]);
    let (dx, dy) = (t[0] - o[0], t[1] - o[1]);
    let k = ((dy.abs() / dx.hypot(dy).max(1e-3) - 0.5) / 0.366).clamp(0.0, 1.0);
    let upright = k * k * (3.0 - 2.0 * k);
    let mut spots: Vec<(f32, ScreenHotspot)> = MODEL
        .anchors
        .iter()
        .enumerate()
        .map(|(index, &a)| {
            let v = view.rot(a);
            let pos = view.project(v);
            let facing = dot(up, view.to_cam(v));
            let inside = pos[0] >= 0.0 && pos[1] >= 0.0 && pos[0] <= w && pos[1] <= h;
            // Upright, measure against the body's projected centre line at the
            // same height so a tilted remote keeps its left buttons on the
            // left; lying sideways, plain screen x gives the shortest leaders.
            // Centre-line buttons get a nudge so the upright split is stable.
            let centre_x = view.project_model([0.0, a[1], a[2]])[0];
            let nudge = match index {
                13 | 8 => -0.5,
                4 | 5 => 0.5,
                _ => 0.0,
            };
            let score = pos[0] - upright * centre_x - (1.0 - upright) * view.cx_px + nudge;
            let spot = ScreenHotspot {
                index,
                pos,
                visible: facing > 0.2 && inside,
                side: Side::Right,
            };
            (score, spot)
        })
        .collect();
    // Balanced split: the lower-scoring half labels on the left.
    spots.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.index.cmp(&b.1.index)));
    for (_, s) in &mut spots[..BUTTON_COUNT / 2] {
        s.side = Side::Left;
    }
    let mut spots: Vec<ScreenHotspot> = spots.into_iter().map(|(_, s)| s).collect();
    spots.sort_by_key(|s| s.index);
    spots
}

// ---------------------------------------------------------------------------
// Shading

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}
fn lin(hex: u32) -> V3 {
    [
        srgb_to_linear(((hex >> 16) & 0xff) as f32 / 255.0),
        srgb_to_linear(((hex >> 8) & 0xff) as f32 / 255.0),
        srgb_to_linear((hex & 0xff) as f32 / 255.0),
    ]
}

struct MatProps {
    albedo: V3,
    /// Key-light Blinn specular.
    spec: f32,
    shin: f32,
    /// Broad sheen from the overhead softbox.
    sheen: f32,
    sheen_shin: f32,
    rim: f32,
}

fn mat_props(m: Mat) -> MatProps {
    let mp = |hex, spec, shin, sheen, sheen_shin, rim| MatProps {
        albedo: lin(hex),
        spec,
        shin,
        sheen,
        sheen_shin,
        rim,
    };
    match m {
        Mat::Body => mp(0x131517, 0.05, 30.0, 0.035, 12.0, 0.04),
        Mat::Seam => mp(0x060708, 0.01, 10.0, 0.0, 1.0, 0.0),
        Mat::Button => mp(0x0e0f11, 0.10, 45.0, 0.05, 16.0, 0.05),
        Mat::Glyph => mp(0xc4c8cc, 0.03, 20.0, 0.02, 10.0, 0.0),
        Mat::GlyphDim => mp(0x3a3e43, 0.06, 30.0, 0.03, 10.0, 0.0),
        Mat::Logo => mp(0x5a5f64, 0.25, 50.0, 0.10, 10.0, 0.0),
        Mat::Led => mp(0x040405, 0.20, 80.0, 0.05, 20.0, 0.0),
    }
}

const TURQUOISE: u32 = 0x19e6d4;

/// Studio lighting in view space (moves with the camera). Returns
/// (linear colour, key-light lambert term).
fn shade(m: Mat, n: V3, pos: V3, to_cam: V3) -> (V3, f32) {
    let p = mat_props(m);
    // Key: point light upper-left-front. Softbox: overhead, slightly behind.
    let key = normalize(sub([-260.0, 230.0, 320.0], pos));
    let soft = normalize(sub([-40.0, 330.0, 90.0], pos));
    let fill = normalize([0.8, -0.2, 0.5]);
    let rim = normalize([0.3, 0.6, -0.75]);
    let nk = dot(n, key).max(0.0);
    let nf = dot(n, fill).max(0.0);
    let diffuse = 0.18 + 0.80 * nk + 0.25 * nf;
    let blinn = |l: V3, shin: f32| dot(n, normalize(add(l, to_cam))).max(0.0).powf(shin);
    let spec = p.spec * blinn(key, p.shin) + p.sheen * blinn(soft, p.sheen_shin);
    let fres = (1.0 - dot(n, to_cam).max(0.0)).powi(3);
    let rim_term = p.rim * fres * (0.3 + dot(n, rim).max(0.0));
    let s = spec + rim_term;
    let c = add(mul(p.albedo, diffuse), [s, s * 1.02, s * 1.05]);
    (c, nk)
}

// ---------------------------------------------------------------------------
// Core: screen-space shaded triangles, shared by `scene` and the preview test.

#[derive(Clone, Copy, Debug)]
struct ShadedTri {
    pts: [P2; 3],
    /// Non-premultiplied sRGB.
    rgba: [f32; 4],
    /// Triangles may share a GPUI path only with equal `rgba` and `key`.
    /// Opaque triangles use 0; each translucent layer gets a unique key.
    key: u32,
}

fn to_rgba(c: V3, a: f32) -> [f32; 4] {
    let q = |v: f32| (linear_to_srgb(v) * 255.0).round() / 255.0;
    [q(c[0]), q(c[1]), q(c[2]), a]
}

fn expand_tri(p: [P2; 3], e: f32) -> [P2; 3] {
    let mut out = p;
    for i in 0..3 {
        let a = p[i];
        let b = p[(i + 1) % 3];
        let c = p[(i + 2) % 3];
        let n2 = |v: P2| {
            let l = (v[0] * v[0] + v[1] * v[1]).sqrt();
            if l > 1e-6 {
                [v[0] / l, v[1] / l]
            } else {
                [0.0, 0.0]
            }
        };
        let u = n2([b[0] - a[0], b[1] - a[1]]);
        let w = n2([c[0] - a[0], c[1] - a[1]]);
        let bis = n2([u[0] + w[0], u[1] + w[1]]);
        let sin_half = (u[0] * bis[1] - u[1] * bis[0]).abs();
        let d = (e / sin_half.max(0.25)).min(4.0 * e);
        out[i] = [a[0] - bis[0] * d, a[1] - bis[1] * d];
    }
    out
}

fn tri_area2(p: [P2; 3]) -> f32 {
    (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1])
}

fn point_in_tri(p: [P2; 3], q: P2) -> bool {
    let e = |a: P2, b: P2| (b[0] - a[0]) * (q[1] - a[1]) - (b[1] - a[1]) * (q[0] - a[0]);
    let (d0, d1, d2) = (e(p[0], p[1]), e(p[1], p[2]), e(p[2], p[0]));
    (d0 >= 0.0 && d1 >= 0.0 && d2 >= 0.0) || (d0 <= 0.0 && d1 <= 0.0 && d2 <= 0.0)
}

/// Pushes a filled convex polygon (fan) as one translucent layer.
fn push_fan(out: &mut Vec<ShadedTri>, pts: &[P2], rgba: [f32; 4], key: u32) {
    for i in 1..pts.len().saturating_sub(1) {
        out.push(ShadedTri {
            pts: [pts[0], pts[i], pts[i + 1]],
            rgba,
            key,
        });
    }
}

fn push_band(out: &mut Vec<ShadedTri>, inner: &[P2], outer: &[P2], rgba: [f32; 4], key: u32) {
    let n = inner.len();
    for i in 0..n {
        let j = (i + 1) % n;
        out.push(ShadedTri {
            pts: [inner[i], outer[i], outer[j]],
            rgba,
            key,
        });
        out.push(ShadedTri {
            pts: [inner[i], outer[j], inner[j]],
            rgba,
            key,
        });
    }
}

fn shaded_triangles(
    camera: Camera,
    width: f32,
    height: f32,
    highlight: Highlight,
) -> Vec<ShadedTri> {
    let model = &*MODEL;
    let view = View::new(camera, width, height);
    let mut out = Vec::with_capacity(model.tris.len());
    let mut key = 1u32;

    // Soft ground contact shadow: the body footprint, grown in layers.
    let layers = 8;
    for k in (0..layers).rev() {
        let t = k as f32 / (layers - 1) as f32;
        let grow = -1.6 + 11.0 * t;
        let shift = [1.2 + 2.5 * t, -1.5 - 3.0 * t];
        let pts: Vec<P2> = model
            .body_outline
            .iter()
            .map(|&(c, n)| {
                let r = CORNER + grow;
                view.project_model([c[0] + r * n[0] + shift[0], c[1] + r * n[1] + shift[1], 0.0])
            })
            .collect();
        let alpha = if k == 0 { 0.55 } else { 0.075 };
        push_fan(&mut out, &pts, [0.0, 0.0, 0.0, alpha], key);
        key += 1;
    }

    // Transform, cull and shade the mesh.
    let rank = {
        let mut order: Vec<(f32, usize)> = model
            .group_centres
            .iter()
            .enumerate()
            .skip(1)
            .map(|(g, &c)| (view.rot(c)[2], g))
            .collect();
        order.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut rank = vec![0usize; model.group_centres.len()];
        for (i, &(_, g)) in order.iter().enumerate() {
            rank[g] = i + 1;
        }
        rank
    };
    let turq = lin(TURQUOISE);
    let mut visible: Vec<(usize, u8, f32, ShadedTri)> = Vec::with_capacity(model.tris.len());
    for t in &model.tris {
        let v = t.v.map(|p| view.rot(p));
        let c = centroid(v[0], v[1], v[2]);
        let n = view.rot_dir(t.n);
        let to_cam = view.to_cam(c);
        if dot(n, to_cam) <= 0.0 {
            continue;
        }
        let pts = v.map(|p| view.project(p));
        if tri_area2(pts).abs() < 0.02 {
            continue;
        }
        let (mut col, nk) = shade(t.mat, n, c, to_cam);
        if let Some(b) = t.button.map(usize::from) {
            let glyph = matches!(t.mat, Mat::Glyph);
            if highlight.pressed == Some(b) {
                col = if glyph {
                    lin(0xe6fffc)
                } else {
                    mix3(col, mul(turq, 0.55 + 0.6 * nk), 0.85)
                };
            } else if highlight.selected == Some(b) {
                col = if glyph {
                    turq
                } else {
                    mix3(col, mul(turq, 0.10 + 0.30 * nk), 0.55)
                };
            } else if highlight.hovered == Some(b) {
                col = add(mul(col, 1.7), [0.004; 3]);
            }
        }
        let rgba = to_rgba(col, 1.0);
        visible.push((
            rank[t.group as usize],
            t.layer,
            -c[2],
            ShadedTri {
                pts: expand_tri(pts, 0.3),
                rgba,
                key: 0,
            },
        ));
    }
    visible.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(b.2.total_cmp(&a.2)));

    // Glow rings around highlighted buttons, each painted right before the
    // (group, layer) it surrounds.
    let mut glows: Vec<((usize, u8), Vec<ShadedTri>)> = Vec::new();
    for g in &model.glows {
        let b = Some(g.button as usize);
        let (colour, alphas): (V3, [f32; 3]) = if highlight.pressed == b {
            (lin(0x7ff5ea), [0.75, 0.40, 0.16])
        } else if highlight.selected == b {
            (turq, [0.60, 0.30, 0.11])
        } else if highlight.hovered == b {
            (lin(0xffffff), [0.12, 0.06, 0.025])
        } else {
            continue;
        };
        let radii = [g.r + 0.15, g.r + 0.9, g.r + 1.8, g.r + 3.0];
        let ring_pts = |r: f32| -> Vec<P2> {
            g.outline
                .iter()
                .map(|&(c, n)| view.project_model([c[0] + r * n[0], c[1] + r * n[1], g.z]))
                .collect()
        };
        let mut tris = Vec::new();
        for i in 0..3 {
            push_band(
                &mut tris,
                &ring_pts(radii[i]),
                &ring_pts(radii[i + 1]),
                to_rgba(colour, alphas[i]),
                key,
            );
            key += 1;
        }
        glows.push(((rank[g.group as usize], g.layer), tris));
    }
    for e in &visible {
        for (at, tris) in &mut glows {
            if *at <= (e.0, e.1) && !tris.is_empty() {
                out.append(tris);
            }
        }
        out.push(e.3);
    }
    for (_, tris) in &mut glows {
        out.append(tris);
    }

    // Hotspot ring markers.
    let white = to_rgba(lin(0xf4f6f7), 1.0);
    let accent = to_rgba(turq, 1.0);
    for s in projected_hotspots(&view, width, height) {
        if !s.visible {
            continue;
        }
        let selected = highlight.selected == Some(s.index);
        let circle = |r: f32| -> Vec<P2> {
            (0..28)
                .map(|i| {
                    let a = TAU * i as f32 / 28.0;
                    [s.pos[0] + r * a.cos(), s.pos[1] + r * a.sin()]
                })
                .collect()
        };
        if selected {
            push_fan(&mut out, &circle(4.2), accent, 0);
        }
        push_band(
            &mut out,
            &circle(6.25),
            &circle(7.75),
            if selected { accent } else { white },
            0,
        );
    }
    out
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas_size(w: f32, h: f32) -> Size<Pixels> {
        size(px(w), px(h))
    }

    // `gpui_kit::*` exports gpui's own `test` macro; use the std one.
    #[::core::prelude::v1::test]
    fn hotspots_cover_every_button_and_hit_test_matches() {
        let n = MODEL.tris.len();
        assert!((1500..=4500).contains(&n), "triangle count {n}");
        let size = canvas_size(760.0, 680.0);
        let cam = Camera::default();
        let spots = hotspots(cam, size);
        assert_eq!(spots.len(), BUTTON_COUNT);
        for (i, s) in spots.iter().enumerate() {
            assert_eq!(s.index, i);
            assert!(s.visible, "button {i} not visible");
            assert_eq!(
                hit_test(cam, size, s.anchor),
                Some(i),
                "hit test for button {i}"
            );
        }
        let left = spots.iter().filter(|s| s.side == Side::Left).count();
        assert_eq!(left, 7);
        assert_eq!(hit_test(cam, size, spots[8].anchor), Some(8));
        assert_eq!(hit_test(cam, size, point(px(2.0), px(2.0))), None);
    }

    /// Every orbit the UI allows keeps the whole body and every hotspot on the
    /// canvas and splits callout sides 7 / 7.
    #[::core::prelude::v1::test]
    fn rotation_sweep_keeps_remote_on_canvas() {
        for (w, h) in [
            (290.0, 480.0),
            (490.0, 620.0),
            (610.0, 620.0),
            (1000.0, 800.0),
        ] {
            for yi in 0..=26 {
                for pi in 0..=23 {
                    let cam = Camera {
                        yaw: -1.3 + 0.1 * yi as f32,
                        pitch: 0.05 + 0.05 * pi as f32,
                    };
                    let view = View::new(cam, w, h);
                    for &(c, n) in &MODEL.body_outline {
                        for z in [0.0, THICK] {
                            let p =
                                view.project_model([c[0] + CORNER * n[0], c[1] + CORNER * n[1], z]);
                            assert!(
                                (0.0..=w).contains(&p[0]) && (0.0..=h).contains(&p[1]),
                                "body leaves {w}x{h} at {cam:?}"
                            );
                        }
                    }
                    let spots = hotspots(cam, canvas_size(w, h));
                    assert!(spots.iter().all(|s| s.visible), "hidden hotspot at {cam:?}");
                    assert_eq!(spots.iter().filter(|s| s.side == Side::Left).count(), 7);
                }
            }
        }
        // Upright views keep model-left buttons (Home, Apps, Back, Left, Vol-) left.
        for yaw in [-0.3, 0.0, 0.3] {
            let spots = hotspots(Camera { yaw, pitch: 0.62 }, canvas_size(610.0, 620.0));
            for i in [0, 2, 3, 6, 11] {
                assert_eq!(spots[i].side, Side::Left, "button {i} at yaw {yaw}");
            }
        }
    }

    fn write_bmp(path: &std::path::Path, w: usize, h: usize, rgb: &[[f32; 3]]) {
        let row = (w * 3 + 3) & !3;
        let size = 54 + row * h;
        let mut f = Vec::with_capacity(size);
        f.extend_from_slice(b"BM");
        f.extend_from_slice(&(size as u32).to_le_bytes());
        f.extend_from_slice(&0u32.to_le_bytes());
        f.extend_from_slice(&54u32.to_le_bytes());
        f.extend_from_slice(&40u32.to_le_bytes());
        f.extend_from_slice(&(w as i32).to_le_bytes());
        f.extend_from_slice(&(h as i32).to_le_bytes());
        f.extend_from_slice(&1u16.to_le_bytes());
        f.extend_from_slice(&24u16.to_le_bytes());
        f.extend_from_slice(&[0u8; 24]);
        for y in (0..h).rev() {
            for x in 0..w {
                let c = rgb[y * w + x];
                let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                f.extend_from_slice(&[q(c[2]), q(c[1]), q(c[0])]);
            }
            f.extend(std::iter::repeat_n(0u8, row - w * 3));
        }
        std::fs::write(path, f).unwrap();
    }

    /// Software rasterizer with 2x2 supersampling (per-sample alpha blend).
    fn render(camera: Camera, highlight: Highlight, w: usize, h: usize, name: &str) {
        let (sw, sh) = (w * 2, h * 2);
        let bg = 0x16 as f32 / 255.0;
        let mut buf = vec![[bg; 3]; sw * sh];
        for t in shaded_triangles(camera, w as f32, h as f32, highlight) {
            let p = t.pts.map(|q| [q[0] * 2.0, q[1] * 2.0]);
            let x0 = p
                .iter()
                .map(|q| q[0])
                .fold(f32::MAX, f32::min)
                .floor()
                .max(0.0) as usize;
            let x1 = (p
                .iter()
                .map(|q| q[0])
                .fold(f32::MIN, f32::max)
                .ceil()
                .max(0.0) as usize)
                .min(sw);
            let y0 = p
                .iter()
                .map(|q| q[1])
                .fold(f32::MAX, f32::min)
                .floor()
                .max(0.0) as usize;
            let y1 = (p
                .iter()
                .map(|q| q[1])
                .fold(f32::MIN, f32::max)
                .ceil()
                .max(0.0) as usize)
                .min(sh);
            let a = t.rgba[3];
            for y in y0..y1 {
                for x in x0..x1 {
                    if point_in_tri(p, [x as f32 + 0.5, y as f32 + 0.5]) {
                        let d = &mut buf[y * sw + x];
                        for i in 0..3 {
                            d[i] = d[i] * (1.0 - a) + t.rgba[i] * a;
                        }
                    }
                }
            }
        }
        let mut img = vec![[0.0; 3]; w * h];
        for y in 0..h {
            for x in 0..w {
                let mut c = [0.0; 3];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let s = buf[(y * 2 + dy) * sw + x * 2 + dx];
                    for i in 0..3 {
                        c[i] += s[i] * 0.25;
                    }
                }
                img[y * w + x] = c;
            }
        }
        // When run from the scratch copy in target/scene-lab, write into the
        // real repo's target/ directory.
        let manifest = env!("CARGO_MANIFEST_DIR");
        let root = manifest
            .split("/target/scene-lab/")
            .next()
            .unwrap_or(manifest);
        let dir = std::path::Path::new(root).join("target/scene-renders");
        std::fs::create_dir_all(&dir).unwrap();
        write_bmp(&dir.join(format!("{name}.bmp")), w, h, &img);
    }

    #[::core::prelude::v1::test]
    #[ignore = "writes preview images to target/scene-renders"]
    fn render_previews() {
        eprintln!("mesh triangles: {}", MODEL.tris.len());
        let d = Camera::default();
        let none = Highlight::default();
        render(d, none, 760, 680, "default");
        let sel = Highlight {
            selected: Some(8),
            ..Default::default()
        };
        render(d, sel, 760, 680, "selected-ok");
        let mix = Highlight {
            selected: Some(13),
            hovered: Some(9),
            pressed: Some(4),
        };
        render(d, mix, 760, 680, "states");
        render(d.rotated(0.8, 0.0), none, 760, 680, "yaw-plus");
        render(d.rotated(-0.8, 0.0), none, 760, 680, "yaw-minus");
        render(
            Camera {
                yaw: d.yaw,
                pitch: 1.15,
            },
            none,
            760,
            680,
            "pitch-low",
        );
        render(
            Camera {
                yaw: 0.0,
                pitch: 0.05,
            },
            none,
            1000,
            800,
            "top-down-wide",
        );
    }
}
