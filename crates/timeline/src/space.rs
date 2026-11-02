//! 3D Track Motion (VEGAS's "3D compositing"): a track whose compositing
//! mode is 3D Source Alpha has its picture on a box in space — moved,
//! sized, oriented and rotated, keyframed on the timeline — seen by a
//! camera in front of the frame. Consecutive 3D tracks (no 2D track between
//! them) are drawn together, crossing where they meet, and composite over
//! what is below as one picture; a 2D track between them is a barrier.

use bv_core::Tick;
use serde::{Deserialize, Serialize};

use crate::fx::KeyCurve;

/// How far the camera stands in front of the frame, in frame widths: the
/// picture of a box at z = 0 facing it is the box's own size.
pub const CAMERA_DISTANCE: f32 = 1.5;

/// A track's box in space.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Box3d {
    /// Its center from the frame's: `x` and `z` in frame widths, `y` in
    /// frame heights (x to the right, y up, z away from the viewer).
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Its size: its width in frame widths, its height in frame heights.
    pub width: f32,
    pub height: f32,
    /// Its orientation, then its rotation: degrees about x, y and z.
    pub orientation: [f32; 3],
    pub rotation: [f32; 3],
}

impl Default for Box3d {
    fn default() -> Box3d {
        Box3d {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            width: 1.0,
            height: 1.0,
            orientation: [0.0; 3],
            rotation: [0.0; 3],
        }
    }
}

impl Box3d {
    pub fn is_default(&self) -> bool {
        *self == Box3d::default()
    }

    fn lerp(&self, b: &Box3d, f: f32) -> Box3d {
        let m = |a: f32, b: f32| a + (b - a) * f;
        Box3d {
            x: m(self.x, b.x),
            y: m(self.y, b.y),
            z: m(self.z, b.z),
            width: m(self.width, b.width),
            height: m(self.height, b.height),
            orientation: std::array::from_fn(|i| m(self.orientation[i], b.orientation[i])),
            rotation: std::array::from_fn(|i| m(self.rotation[i], b.rotation[i])),
        }
    }

    /// Where it is in a `w` × `h` frame's space, in pixels: its center and
    /// its half width and half height as vectors.
    pub fn plane(&self, w: f32, h: f32) -> Plane {
        let r = mul(&rotation(self.rotation), &rotation(self.orientation));
        let axis = |i: usize| [r[0][i], r[1][i], r[2][i]];
        Plane {
            center: [self.x * w, self.y * h, self.z * w],
            u: scale(axis(0), self.width * w / 2.0),
            v: scale(axis(1), self.height * h / 2.0),
        }
    }
}

type M3 = [[f32; 3]; 3];

/// The rotation by `deg` about x, then y, then z.
fn rotation(deg: [f32; 3]) -> M3 {
    let [ax, ay, az] = deg.map(f32::to_radians);
    let (sx, cx) = ax.sin_cos();
    let (sy, cy) = ay.sin_cos();
    let (sz, cz) = az.sin_cos();
    let rx = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
    let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
    let rz = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
    mul(&rz, &mul(&ry, &rx))
}

fn mul(a: &M3, b: &M3) -> M3 {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum()))
}

fn scale(v: [f32; 3], k: f32) -> [f32; 3] {
    v.map(|c| c * k)
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// A box's place in a frame's space (pixels from the frame's center; x to
/// the right, y up, z away from the viewer).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    pub center: [f32; 3],
    /// Half its width and half its height, along its own axes.
    pub u: [f32; 3],
    pub v: [f32; 3],
}

impl Plane {
    /// Its corners: top left, top right, bottom right, bottom left.
    pub fn corners(&self) -> [[f32; 3]; 4] {
        let c = self.center;
        let at = |a: f32, b: f32| std::array::from_fn(|i| c[i] + a * self.u[i] + b * self.v[i]);
        [at(-1.0, 1.0), at(1.0, 1.0), at(1.0, -1.0), at(-1.0, -1.0)]
    }

    /// What the camera of a `w`-wide frame sees of it through the pixel at
    /// (`x`, `y`) from the frame's center (y up): how far along the ray it
    /// is, and where on its picture (each from 0 to 1, the top left
    /// corner at 0, 0). `None` where the ray misses it.
    pub fn hit(&self, x: f32, y: f32, w: f32) -> Option<(f32, f32, f32)> {
        let d = CAMERA_DISTANCE * w;
        let eye = [0.0, 0.0, -d];
        let ray = [x, y, d];
        let n = cross(self.u, self.v);
        let along = dot(n, ray);
        if along.abs() < 1e-9 {
            return None;
        }
        let t = dot(n, sub(self.center, eye)) / along;
        if t <= 0.0 {
            return None;
        }
        let q = sub(std::array::from_fn(|i| eye[i] + t * ray[i]), self.center);
        let a = dot(q, self.u) / dot(self.u, self.u).max(1e-12);
        let b = dot(q, self.v) / dot(self.v, self.v).max(1e-12);
        ((-1.0..=1.0).contains(&a) && (-1.0..=1.0).contains(&b)).then_some((
            t,
            (a + 1.0) / 2.0,
            (1.0 - b) / 2.0,
        ))
    }
}

/// Where the camera of a `w`-wide frame shows the point `p` (pixels from
/// the frame's center, y up), when it is in front of it.
pub fn project(p: [f32; 3], w: f32) -> Option<[f32; 2]> {
    let d = CAMERA_DISTANCE * w;
    let depth = d + p[2];
    (depth > 1e-3).then(|| [p[0] * d / depth, p[1] * d / depth])
}

/// A keyframe of a 3D box, on the timeline.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Box3dKey {
    pub at: Tick,
    pub value: Box3d,
    #[serde(default)]
    pub curve: KeyCurve,
}

/// The box at `t` along `keys` (`still` when there are none).
pub fn box_at(keys: &[Box3dKey], still: &Box3d, t: Tick) -> Box3d {
    let Some(first) = keys.first() else {
        return *still;
    };
    if t <= first.at {
        return first.value;
    }
    for w in keys.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if t < b.at {
            let span = (b.at - a.at).0.max(1) as f64;
            let f = a.curve.eval(((t - a.at).0 as f64 / span) as f32);
            return a.value.lerp(&b.value, f);
        }
    }
    keys.last().map_or(*still, |k| k.value)
}

/// The box at `t` set to `value` in `keys` (and `still`): a keyframe there
/// when it is animated (or `t` is past the timeline's start).
pub fn set_box_at(
    keys: &mut Vec<Box3dKey>,
    still: &mut Box3d,
    t: Tick,
    value: Box3d,
    curve: KeyCurve,
) {
    if keys.is_empty() {
        if t == Tick::ZERO {
            *still = value;
            return;
        }
        keys.push(Box3dKey {
            at: Tick::ZERO,
            value: *still,
            curve,
        });
    }
    match keys.iter_mut().find(|k| k.at == t) {
        Some(k) => k.value = value,
        None => {
            let before = keys
                .iter()
                .rev()
                .find(|k| k.at < t)
                .map_or(curve, |k| k.curve);
            keys.push(Box3dKey {
                at: t,
                value,
                curve: before,
            });
            keys.sort_by_key(|k| k.at);
        }
    }
    *still = keys[0].value;
}

/// The keyframe at `t` removed from `keys` (the last one left becomes
/// `still`).
pub fn remove_box_key(keys: &mut Vec<Box3dKey>, still: &mut Box3d, t: Tick) {
    keys.retain(|k| k.at != t);
    if keys.len() == 1 {
        *still = keys[0].value;
        keys.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: f32 = 1920.0;
    const H: f32 = 1080.0;

    #[test]
    fn a_box_at_rest_fills_the_frame() {
        let p = Box3d::default().plane(W, H);
        let [tl, _, br, _] = p.corners();
        assert_eq!(project(tl, W), Some([-960.0, 540.0]));
        assert_eq!(project(br, W), Some([960.0, -540.0]));
        // The frame's center sees the picture's center.
        let (_, a, b) = p.hit(0.0, 0.0, W).unwrap();
        assert!((a - 0.5).abs() < 1e-6 && (b - 0.5).abs() < 1e-6);
        // Its top left corner, the picture's.
        let (_, a, b) = p.hit(-959.0, 539.0, W).unwrap();
        assert!(a < 0.01 && b < 0.01, "{a} {b}");
        assert!(p.hit(1000.0, 0.0, W).is_none());
    }

    #[test]
    fn farther_boxes_look_smaller() {
        let far = Box3d {
            z: CAMERA_DISTANCE,
            ..Box3d::default()
        };
        let [tl, ..] = far.plane(W, H).corners();
        // Twice as far from the camera: half the size.
        assert_eq!(project(tl, W), Some([-480.0, 270.0]));
    }

    #[test]
    fn a_box_turned_about_y_is_nearer_on_one_side() {
        let turned = Box3d {
            orientation: [0.0, 30.0, 0.0],
            ..Box3d::default()
        };
        let p = turned.plane(W, H);
        let (left, ..) = p.hit(-400.0, 0.0, W).unwrap();
        let (right, ..) = p.hit(400.0, 0.0, W).unwrap();
        assert!(left != right);
        // Seen edge on: nothing.
        let edge = Box3d {
            orientation: [0.0, 90.0, 0.0],
            ..Box3d::default()
        };
        assert!(edge.plane(W, H).hit(300.0, 0.0, W).is_none());
    }

    #[test]
    fn keyframes_interpolate_the_box() {
        let mut keys = Vec::new();
        let mut still = Box3d::default();
        let moved = Box3d {
            z: 1.0,
            ..Box3d::default()
        };
        set_box_at(
            &mut keys,
            &mut still,
            Tick::from_seconds(2),
            moved,
            KeyCurve::Linear,
        );
        assert_eq!(keys.len(), 2);
        let half = box_at(&keys, &still, Tick::from_seconds(1));
        assert!((half.z - 0.5).abs() < 1e-6);
        remove_box_key(&mut keys, &mut still, Tick::from_seconds(2));
        assert!(keys.is_empty());
        assert!(still.is_default());
    }
}
