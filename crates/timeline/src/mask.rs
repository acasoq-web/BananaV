//! An event's Pan/Crop masks, as VEGAS 2026's Help describes them
//! ("Creating masks using the Pan/Crop tool"): shapes on the event's media
//! — closed Bézier paths in the media's pixels, drawn anchor by anchor or
//! as rectangles and ovals in the Pan/Crop window — each positive (what is
//! inside shows), negative (what is inside goes) or disabled, with its
//! opacity, its feather (into the shape, out of it or both ways) and
//! anti-aliasing. They cut what shows of the event or, with Apply to FX,
//! keep its effects inside them (the rest of it as it is without them).
//! Their shapes animate on the Pan/Crop window's Mask row; the renderers
//! take what they keep of each pixel from [`PanCropMask::coverage`].

use bv_core::Tick;
use serde::{Deserialize, Serialize};

use crate::fx::KeyCurve;

/// An anchor of a mask's path: its point (media pixels, y down) and its
/// tangents' ends from it, toward the anchor before and the one after.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MaskAnchor {
    pub at: [f32; 2],
    #[serde(default)]
    pub before: [f32; 2],
    #[serde(default)]
    pub after: [f32; 2],
    /// Its tangents turn apart (Split Tangents); else they stay in line.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub split: bool,
}

impl MaskAnchor {
    /// A corner: no tangents.
    pub fn corner(at: [f32; 2]) -> MaskAnchor {
        MaskAnchor {
            at,
            ..MaskAnchor::default()
        }
    }
}

/// What a path does with what is inside it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaskMode {
    /// It shows (the effects work there, with Apply to FX).
    #[default]
    Positive,
    /// It goes (the effects work outside it).
    Negative,
    /// Nothing: the path stays, unused.
    Disabled,
}

impl MaskMode {
    pub const ALL: [MaskMode; 3] = [MaskMode::Positive, MaskMode::Negative, MaskMode::Disabled];

    pub fn name(self) -> &'static str {
        match self {
            MaskMode::Positive => "Positive",
            MaskMode::Negative => "Negative",
            MaskMode::Disabled => "Disabled",
        }
    }
}

/// Which way a path's edge softens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaskFeather {
    #[default]
    None,
    /// Into the shape.
    In,
    /// Out of it.
    Out,
    /// Both ways, half each.
    Both,
}

impl MaskFeather {
    pub const ALL: [MaskFeather; 4] = [
        MaskFeather::None,
        MaskFeather::In,
        MaskFeather::Out,
        MaskFeather::Both,
    ];

    pub fn name(self) -> &'static str {
        match self {
            MaskFeather::None => "None",
            MaskFeather::In => "In",
            MaskFeather::Out => "Out",
            MaskFeather::Both => "Both",
        }
    }
}

/// One mask: a closed path and how it masks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MaskPath {
    pub anchors: Vec<MaskAnchor>,
    #[serde(default)]
    pub mode: MaskMode,
    /// Its edge smoothed over a pixel.
    #[serde(default = "yes")]
    pub anti_alias: bool,
    /// How much of what it masks goes (0–1).
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default)]
    pub feather: MaskFeather,
    /// How far its edge softens: a percentage of the media's shorter side.
    #[serde(default)]
    pub feather_pct: f32,
}

fn yes() -> bool {
    true
}

fn one() -> f32 {
    1.0
}

/// The kappa that makes four cubic Béziers a circle.
const KAPPA: f32 = 0.552_284_8;

impl MaskPath {
    /// A path of `anchors`, positive, anti-aliased, opaque, not feathered.
    pub fn new(anchors: Vec<MaskAnchor>) -> MaskPath {
        MaskPath {
            anchors,
            mode: MaskMode::Positive,
            anti_alias: true,
            opacity: 1.0,
            feather: MaskFeather::None,
            feather_pct: 0.0,
        }
    }

    /// A rectangle from corner `a` to corner `b`.
    pub fn rectangle(a: [f32; 2], b: [f32; 2]) -> MaskPath {
        let (x0, x1) = (a[0].min(b[0]), a[0].max(b[0]));
        let (y0, y1) = (a[1].min(b[1]), a[1].max(b[1]));
        MaskPath::new(
            [[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
                .map(MaskAnchor::corner)
                .to_vec(),
        )
    }

    /// A rectangle from `a` to `b` with its corners rounded by `r` (a
    /// part, 0–½, of its shorter side).
    pub fn rounded(a: [f32; 2], b: [f32; 2], r: f32) -> MaskPath {
        let (x0, x1) = (a[0].min(b[0]), a[0].max(b[0]));
        let (y0, y1) = (a[1].min(b[1]), a[1].max(b[1]));
        let r = r.clamp(0.0, 0.5) * (x1 - x0).min(y1 - y0);
        let k = r * KAPPA;
        let side = |at: [f32; 2], before: [f32; 2], after: [f32; 2]| MaskAnchor {
            at,
            before,
            after,
            split: true,
        };
        MaskPath::new(vec![
            side([x0 + r, y0], [-k, 0.0], [0.0, 0.0]),
            side([x1 - r, y0], [0.0, 0.0], [k, 0.0]),
            side([x1, y0 + r], [0.0, -k], [0.0, 0.0]),
            side([x1, y1 - r], [0.0, 0.0], [0.0, k]),
            side([x1 - r, y1], [k, 0.0], [0.0, 0.0]),
            side([x0 + r, y1], [0.0, 0.0], [-k, 0.0]),
            side([x0, y1 - r], [0.0, k], [0.0, 0.0]),
            side([x0, y0 + r], [0.0, 0.0], [0.0, -k]),
        ])
    }

    /// An oval filling the box from `a` to `b`.
    pub fn oval(a: [f32; 2], b: [f32; 2]) -> MaskPath {
        let c = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
        let (rx, ry) = ((b[0] - a[0]).abs() / 2.0, (b[1] - a[1]).abs() / 2.0);
        let (kx, ky) = (rx * KAPPA, ry * KAPPA);
        let smooth = |at: [f32; 2], t: [f32; 2]| MaskAnchor {
            at,
            before: [-t[0], -t[1]],
            after: t,
            split: false,
        };
        MaskPath::new(vec![
            smooth([c[0], c[1] - ry], [kx, 0.0]),
            smooth([c[0] + rx, c[1]], [0.0, ky]),
            smooth([c[0], c[1] + ry], [-kx, 0.0]),
            smooth([c[0] - rx, c[1]], [0.0, -ky]),
        ])
    }

    /// The upper half of the oval filling the box from `a` to `b` (its
    /// flat side the box's middle line).
    pub fn half_oval(a: [f32; 2], b: [f32; 2]) -> MaskPath {
        let (x0, x1) = (a[0].min(b[0]), a[0].max(b[0]));
        let (y0, y1) = (a[1].min(b[1]), a[1].max(b[1]));
        let (rx, ry) = ((x1 - x0) / 2.0, y1 - y0);
        let (kx, ky) = (rx * KAPPA, ry * KAPPA);
        let cx = (x0 + x1) / 2.0;
        MaskPath::new(vec![
            MaskAnchor {
                at: [x0, y1],
                before: [0.0, 0.0],
                after: [0.0, -ky],
                split: true,
            },
            MaskAnchor {
                at: [cx, y0],
                before: [-kx, 0.0],
                after: [kx, 0.0],
                split: false,
            },
            MaskAnchor {
                at: [x1, y1],
                before: [0.0, -ky],
                after: [0.0, 0.0],
                split: true,
            },
        ])
    }

    /// Its anchors' bounding box (`None` without anchors).
    pub fn bounds(&self) -> Option<([f32; 2], [f32; 2])> {
        bounds(&self.anchors)
    }
}

/// The closed polygon `poly` moved `d` along its normals: out of the
/// shape for a positive `d`, into it for a negative one (where a feather's
/// softening ends).
pub fn offset_polygon(poly: &[[f64; 2]], d: f64) -> Vec<[f64; 2]> {
    let n = poly.len();
    if n < 3 {
        return poly.to_vec();
    }
    // Which way round it goes: its normals out of it on the right of its
    // way when its area counts positive.
    let area: f64 = (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };
    (0..n)
        .map(|i| {
            let (p, q) = (poly[(i + n - 1) % n], poly[(i + 1) % n]);
            let t = [q[0] - p[0], q[1] - p[1]];
            let l = t[0].hypot(t[1]);
            if l < 1e-12 {
                return poly[i];
            }
            let normal = [t[1] / l * sign, -t[0] / l * sign];
            [poly[i][0] + normal[0] * d, poly[i][1] + normal[1] * d]
        })
        .collect()
}

/// `anchors`' bounding box.
pub fn bounds(anchors: &[MaskAnchor]) -> Option<([f32; 2], [f32; 2])> {
    let first = anchors.first()?.at;
    Some(anchors.iter().fold((first, first), |(lo, hi), a| {
        (
            [lo[0].min(a.at[0]), lo[1].min(a.at[1])],
            [hi[0].max(a.at[0]), hi[1].max(a.at[1])],
        )
    }))
}

/// The paths' shapes at a keyframe of the Mask row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MaskKey {
    /// From the event's start.
    pub at: Tick,
    /// Each path's anchors, in the paths' order.
    pub shapes: Vec<Vec<MaskAnchor>>,
    #[serde(default)]
    pub curve: KeyCurve,
}

/// An event's masks: the Pan/Crop window's Mask row.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PanCropMask {
    /// The Mask row's checkbox: whether they mask at all.
    #[serde(default)]
    pub enabled: bool,
    /// They keep the event's effects inside them (the rest of the event as
    /// it is without them) instead of cutting what shows of it.
    #[serde(default)]
    pub apply_to_fx: bool,
    #[serde(default)]
    pub paths: Vec<MaskPath>,
    /// The shapes over time (none: the paths' own all along).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<MaskKey>,
}

impl PanCropMask {
    /// Whether anything is masked: on, with a path that masks.
    pub fn active(&self) -> bool {
        self.enabled
            && self
                .paths
                .iter()
                .any(|p| p.mode != MaskMode::Disabled && p.anchors.len() >= 2)
    }

    /// The paths' shapes at `local` (from the event's start): their own
    /// without keyframes, else from the keyframes — anchor by anchor as
    /// far as the curve goes where both keyframes have as many, else the
    /// one before's.
    pub fn shapes_at(&self, local: Tick) -> Vec<Vec<MaskAnchor>> {
        let own = || self.paths.iter().map(|p| p.anchors.clone()).collect();
        let (Some(first), Some(last)) = (self.keys.first(), self.keys.last()) else {
            return own();
        };
        let fit = |s: &[Vec<MaskAnchor>]| {
            let mut v = s.to_vec();
            v.resize_with(self.paths.len(), Vec::new);
            v.truncate(self.paths.len());
            v
        };
        if local <= first.at {
            return fit(&first.shapes);
        }
        if local >= last.at {
            return fit(&last.shapes);
        }
        let i = self.keys.partition_point(|k| k.at <= local);
        let (a, b) = (&self.keys[i - 1], &self.keys[i]);
        let span = (b.at - a.at).0.max(1) as f32;
        let f = a.curve.eval((local - a.at).0 as f32 / span);
        let mix = |x: f32, y: f32| x + (y - x) * f;
        let mix2 = |x: [f32; 2], y: [f32; 2]| [mix(x[0], y[0]), mix(x[1], y[1])];
        let shapes = a
            .shapes
            .iter()
            .enumerate()
            .map(|(p, sa)| match b.shapes.get(p) {
                Some(sb) if sb.len() == sa.len() && a.curve != KeyCurve::Hold => sa
                    .iter()
                    .zip(sb)
                    .map(|(x, y)| MaskAnchor {
                        at: mix2(x.at, y.at),
                        before: mix2(x.before, y.before),
                        after: mix2(x.after, y.after),
                        split: x.split,
                    })
                    .collect(),
                _ => sa.clone(),
            })
            .collect::<Vec<_>>();
        fit(&shapes)
    }

    /// The paths' shapes at `local` become `shapes`: their own without
    /// keyframes, else the keyframe there (made, with the curve of the one
    /// before, when there is none).
    pub fn set_shapes_at(&mut self, local: Tick, shapes: Vec<Vec<MaskAnchor>>) {
        if self.keys.is_empty() {
            for (p, s) in self.paths.iter_mut().zip(shapes) {
                p.anchors = s;
            }
            return;
        }
        match self.keys.iter_mut().find(|k| k.at == local) {
            Some(k) => k.shapes = shapes,
            None => {
                let curve = self
                    .keys
                    .iter()
                    .rev()
                    .find(|k| k.at < local)
                    .map_or(KeyCurve::Smooth, |k| k.curve);
                let i = self.keys.partition_point(|k| k.at < local);
                self.keys.insert(
                    i,
                    MaskKey {
                        at: local,
                        shapes,
                        curve,
                    },
                );
            }
        }
    }

    /// A keyframe at `local` holding the shapes there (the first one at the
    /// event's start too, when there were none).
    pub fn add_key(&mut self, local: Tick) {
        let shapes = self.shapes_at(local);
        if self.keys.is_empty() {
            self.keys.push(MaskKey {
                at: Tick::ZERO,
                shapes: shapes.clone(),
                curve: KeyCurve::Smooth,
            });
        }
        self.set_shapes_at(local, shapes);
    }

    /// The keyframe at `local` moved to `to` (onto another one there, it
    /// takes its place).
    pub fn move_key(&mut self, local: Tick, to: Tick) {
        let Some(i) = self.keys.iter().position(|k| k.at == local) else {
            return;
        };
        let mut key = self.keys.remove(i);
        key.at = to.max(Tick::ZERO);
        self.keys.retain(|k| k.at != key.at);
        let i = self.keys.partition_point(|k| k.at < key.at);
        self.keys.insert(i, key);
    }

    /// The curve from the keyframe at `local` to the next.
    pub fn set_key_curve(&mut self, local: Tick, curve: KeyCurve) {
        if let Some(k) = self.keys.iter_mut().find(|k| k.at == local) {
            k.curve = curve;
        }
    }

    /// Path `i`'s shape at `local` through `map` (a move, a scale, a flip:
    /// any affine map of the media's pixels), its tangents with it.
    pub fn map_path_at(&mut self, local: Tick, i: usize, map: impl Fn([f32; 2]) -> [f32; 2]) {
        let mut shapes = self.shapes_at(local);
        let Some(shape) = shapes.get_mut(i) else {
            return;
        };
        for a in shape.iter_mut() {
            let at = map(a.at);
            let end = |t: [f32; 2]| {
                let p = map([a.at[0] + t[0], a.at[1] + t[1]]);
                [p[0] - at[0], p[1] - at[1]]
            };
            *a = MaskAnchor {
                at,
                before: end(a.before),
                after: end(a.after),
                split: a.split,
            };
        }
        self.set_shapes_at(local, shapes);
    }

    /// An anchor put in path `path` before its anchor `index` (at the end
    /// past the last): `anchor` in its shape at `local`, and at the other
    /// keyframes halfway between its neighbors there, so every shape keeps
    /// as many anchors.
    pub fn insert_anchor(&mut self, path: usize, index: usize, local: Tick, anchor: MaskAnchor) {
        if path >= self.paths.len() {
            return;
        }
        let between = |s: &[MaskAnchor]| {
            let n = s.len();
            if n == 0 {
                return anchor;
            }
            let (a, b) = (s[(index + n - 1) % n].at, s[index % n].at);
            MaskAnchor::corner([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0])
        };
        let p = &mut self.paths[path];
        let m = between(&p.anchors);
        p.anchors.insert(index.min(p.anchors.len()), m);
        for k in &mut self.keys {
            if let Some(s) = k.shapes.get_mut(path) {
                let m = between(s);
                s.insert(index.min(s.len()), m);
            }
        }
        let mut shapes = self.shapes_at(local);
        if let Some(a) = shapes.get_mut(path).and_then(|s| s.get_mut(index)) {
            *a = anchor;
        }
        self.set_shapes_at(local, shapes);
    }

    /// An anchor put on path `path`'s segment from its anchor `index` to the
    /// next, `t` (0–1) of the way along it, the curve kept as it was — in
    /// its shape at every keyframe too, as far along.
    pub fn split_segment(&mut self, path: usize, index: usize, t: f32) {
        let Some(p) = self.paths.get_mut(path) else {
            return;
        };
        split_in(&mut p.anchors, index, t);
        for k in &mut self.keys {
            if let Some(s) = k.shapes.get_mut(path) {
                split_in(s, index, t);
            }
        }
    }

    /// Anchor `index` of path `path` gone, from every keyframe too; a path
    /// left with fewer than three goes.
    pub fn remove_anchor(&mut self, path: usize, index: usize) {
        let Some(p) = self.paths.get_mut(path) else {
            return;
        };
        if index < p.anchors.len() {
            p.anchors.remove(index);
        }
        for k in &mut self.keys {
            if let Some(s) = k.shapes.get_mut(path)
                && index < s.len()
            {
                s.remove(index);
            }
        }
        if self.paths[path].anchors.len() < 3 {
            self.remove_path(path);
        }
    }

    /// A copy of path `i` (its shape at every keyframe too), moved by
    /// `offset`, added last; its index.
    pub fn duplicate_path(&mut self, i: usize, offset: [f32; 2]) -> Option<usize> {
        let mut path = self.paths.get(i)?.clone();
        let shift = |s: &[MaskAnchor]| -> Vec<MaskAnchor> {
            s.iter()
                .map(|a| MaskAnchor {
                    at: [a.at[0] + offset[0], a.at[1] + offset[1]],
                    ..*a
                })
                .collect()
        };
        path.anchors = shift(&path.anchors);
        let n = self.paths.len();
        for k in &mut self.keys {
            k.shapes.resize_with(n, Vec::new);
            let s = shift(&k.shapes[i]);
            k.shapes.push(s);
        }
        self.paths.push(path);
        Some(n)
    }

    /// Two events' masks, the second `offset` after the first's start,
    /// as one event's (joining them back after a cut): `None` when they
    /// differ in more than their shapes over time.
    pub fn joined(
        a: &Option<PanCropMask>,
        b: &Option<PanCropMask>,
        offset: Tick,
    ) -> Option<Option<PanCropMask>> {
        let (a, b) = match (a, b) {
            (None, None) => return Some(None),
            (Some(a), Some(b)) => (a, b),
            _ => return None,
        };
        let settings = |m: &PanCropMask| {
            let mut s = m.clone();
            s.keys.clear();
            for p in &mut s.paths {
                p.anchors.clear();
            }
            s
        };
        if settings(a) != settings(b) {
            return None;
        }
        if a.keys.is_empty() && b.keys.is_empty() {
            return (a == b).then(|| Some(a.clone()));
        }
        let own = |m: &PanCropMask| MaskKey {
            at: Tick::ZERO,
            shapes: m.paths.iter().map(|p| p.anchors.clone()).collect(),
            curve: KeyCurve::Smooth,
        };
        let mut out = a.clone();
        if out.keys.is_empty() {
            out.keys.push(own(a));
        }
        let tail = if b.keys.is_empty() {
            vec![own(b)]
        } else {
            b.keys.clone()
        };
        // The keyframe a cut left at its end gives way to the second's first.
        out.keys.retain(|k| k.at < offset);
        out.keys.extend(tail.into_iter().map(|k| MaskKey {
            at: k.at + offset,
            ..k
        }));
        Some(Some(out))
    }

    /// The keyframe at `local` deleted (the last one gone, its shapes
    /// stay the paths' own).
    pub fn delete_key(&mut self, local: Tick) {
        let Some(i) = self.keys.iter().position(|k| k.at == local) else {
            return;
        };
        let gone = self.keys.remove(i);
        if self.keys.is_empty() {
            for (p, s) in self.paths.iter_mut().zip(gone.shapes) {
                p.anchors = s;
            }
        }
    }

    /// A new path, its shape the same at every keyframe.
    pub fn add_path(&mut self, path: MaskPath) {
        for k in &mut self.keys {
            k.shapes.resize_with(self.paths.len(), Vec::new);
            k.shapes.push(path.anchors.clone());
        }
        self.paths.push(path);
    }

    /// Path `i` gone, from every keyframe too.
    pub fn remove_path(&mut self, i: usize) {
        if i >= self.paths.len() {
            return;
        }
        self.paths.remove(i);
        for k in &mut self.keys {
            if i < k.shapes.len() {
                k.shapes.remove(i);
            }
        }
    }

    /// The event's start moved `delta` later in it: the shapes there are
    /// the first keyframe's, the later ones `delta` earlier.
    pub fn rebase(&mut self, delta: Tick) {
        if self.keys.is_empty() {
            return;
        }
        let first = MaskKey {
            at: Tick::ZERO,
            shapes: self.shapes_at(delta),
            curve: self
                .keys
                .iter()
                .rev()
                .find(|k| k.at <= delta)
                .map_or(KeyCurve::Smooth, |k| k.curve),
        };
        let mut keys: Vec<MaskKey> = self
            .keys
            .iter()
            .filter(|k| k.at > delta)
            .map(|k| MaskKey {
                at: k.at - delta,
                ..k.clone()
            })
            .collect();
        keys.insert(0, first);
        self.keys = keys;
    }

    /// The event cut at `cut` (from its start): this one keeps what is
    /// before (a keyframe at the cut ending it), the other half starts with
    /// the shapes at the cut.
    pub fn split_at(&mut self, cut: Tick) -> PanCropMask {
        let mut right = self.clone();
        right.rebase(cut);
        if !self.keys.is_empty() {
            let shapes = self.shapes_at(cut);
            self.keys.retain(|k| k.at < cut);
            self.keys.push(MaskKey {
                at: cut,
                shapes,
                curve: KeyCurve::Linear,
            });
        }
        right
    }

    /// What [`PanCropMask::coverage`] at `local` depends on, with `sizes`
    /// (the picture's and the media's): equal for equal coverages.
    pub fn fingerprint(&self, local: Tick, sizes: [u32; 4]) -> u64 {
        use std::hash::{Hash, Hasher};
        use std::mem::discriminant;
        let mut h = std::collections::hash_map::DefaultHasher::new();
        sizes.hash(&mut h);
        for (path, anchors) in self.paths.iter().zip(self.shapes_at(local)) {
            discriminant(&path.mode).hash(&mut h);
            discriminant(&path.feather).hash(&mut h);
            path.anti_alias.hash(&mut h);
            path.opacity.to_bits().hash(&mut h);
            path.feather_pct.to_bits().hash(&mut h);
            anchors.len().hash(&mut h);
            for a in anchors {
                for v in a.at.iter().chain(&a.before).chain(&a.after) {
                    v.to_bits().hash(&mut h);
                }
            }
        }
        h.finish()
    }

    /// How much of each pixel of a `w` × `h` picture of the media the
    /// masks keep at `local` (0–1, rows from the top): the media's pixels
    /// times `scale` are the picture's, `short` the media's shorter side
    /// (what the feather counts from). Positive paths add what is inside
    /// them (starting from nothing, or from everything when there are
    /// none), negative ones take it away, each as far as its opacity says.
    pub fn coverage(
        &self,
        local: Tick,
        w: usize,
        h: usize,
        scale: [f64; 2],
        short: f64,
    ) -> Vec<f32> {
        let shapes = self.shapes_at(local);
        let used: Vec<(&MaskPath, &Vec<MaskAnchor>)> = self
            .paths
            .iter()
            .zip(&shapes)
            .filter(|(p, s)| p.mode != MaskMode::Disabled && s.len() >= 2)
            .collect();
        let base = if used.iter().any(|(p, _)| p.mode == MaskMode::Positive) {
            0.0
        } else {
            1.0
        };
        let mut keep = vec![base; w * h];
        let px = (scale[0] + scale[1]) / 2.0;
        for (path, anchors) in used {
            let outline: Vec<[f64; 2]> = polygon(anchors)
                .into_iter()
                .map(|p| [p[0] * scale[0] - 0.5, p[1] * scale[1] - 0.5])
                .collect();
            let feather = match path.feather {
                MaskFeather::None => 0.0,
                _ => path.feather_pct.max(0.0) as f64 / 100.0 * short * px,
            };
            let opacity = path.opacity.clamp(0.0, 1.0);
            shape_coverage(
                &outline,
                w,
                h,
                feather,
                path.feather,
                path.anti_alias,
                |i, c| {
                    let c = c * opacity;
                    keep[i] = match path.mode {
                        MaskMode::Positive => keep[i] + c - keep[i] * c,
                        _ => keep[i] * (1.0 - c),
                    };
                },
            );
        }
        keep
    }
}

/// The point `t` (0–1) of the way along the segment of `anchors` from its
/// anchor `i` to the next.
pub fn segment_point(anchors: &[MaskAnchor], i: usize, t: f32) -> [f32; 2] {
    let n = anchors.len();
    let (a, b) = (anchors[i % n], anchors[(i + 1) % n]);
    let p = [
        [a.at[0] as f64, a.at[1] as f64],
        [(a.at[0] + a.after[0]) as f64, (a.at[1] + a.after[1]) as f64],
        [
            (b.at[0] + b.before[0]) as f64,
            (b.at[1] + b.before[1]) as f64,
        ],
        [b.at[0] as f64, b.at[1] as f64],
    ];
    let q = bezier(&p, t as f64);
    [q[0] as f32, q[1] as f32]
}

/// Splits the segment of `anchors` from anchor `i` to the next at `t`
/// (de Casteljau's): the new anchor's tangents and its neighbors' make the
/// same curve; a straight segment stays straight, the anchor a corner.
fn split_in(anchors: &mut Vec<MaskAnchor>, i: usize, t: f32) {
    let n = anchors.len();
    if n < 2 || i >= n {
        return;
    }
    let j = (i + 1) % n;
    let (a, b) = (anchors[i], anchors[j]);
    let lerp = |p: [f32; 2], q: [f32; 2]| [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t];
    let add = |p: [f32; 2], d: [f32; 2]| [p[0] + d[0], p[1] + d[1]];
    let sub = |p: [f32; 2], q: [f32; 2]| [p[0] - q[0], p[1] - q[1]];
    let (p0, p1, p2, p3) = (a.at, add(a.at, a.after), add(b.at, b.before), b.at);
    let (q0, q1, q2) = (lerp(p0, p1), lerp(p1, p2), lerp(p2, p3));
    let (r0, r1) = (lerp(q0, q1), lerp(q1, q2));
    let s = lerp(r0, r1);
    let new = if a.after == [0.0; 2] && b.before == [0.0; 2] {
        MaskAnchor::corner(s)
    } else {
        MaskAnchor {
            at: s,
            before: sub(r0, s),
            after: sub(r1, s),
            split: false,
        }
    };
    anchors[i].after = sub(q0, p0);
    anchors[j].before = sub(q2, p3);
    anchors.insert(i + 1, new);
}

/// The closed path through `anchors` as a polygon: each segment a cubic
/// Bézier from an anchor along its tangent after it to the next along its
/// tangent before it, in 24 steps.
pub fn polygon(anchors: &[MaskAnchor]) -> Vec<[f64; 2]> {
    let n = anchors.len();
    let mut out = Vec::with_capacity(n * 24);
    let f = |p: [f32; 2]| [p[0] as f64, p[1] as f64];
    for i in 0..n {
        let (a, b) = (anchors[i], anchors[(i + 1) % n]);
        let p = [
            f(a.at),
            [(a.at[0] + a.after[0]) as f64, (a.at[1] + a.after[1]) as f64],
            [
                (b.at[0] + b.before[0]) as f64,
                (b.at[1] + b.before[1]) as f64,
            ],
            f(b.at),
        ];
        let straight = a.after == [0.0; 2] && b.before == [0.0; 2];
        let steps = if straight { 1 } else { 24 };
        for s in 0..steps {
            out.push(bezier(&p, s as f64 / steps as f64));
        }
    }
    out
}

/// A cubic Bézier's point at `t`.
fn bezier(p: &[[f64; 2]; 4], t: f64) -> [f64; 2] {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    [
        a * p[0][0] + b * p[1][0] + c * p[2][0] + d * p[3][0],
        a * p[0][1] + b * p[1][1] + c * p[2][1] + d * p[3][1],
    ]
}

/// One shape's coverage of a `w` × `h` picture (its outline in the
/// picture's pixels, their middles at whole numbers), handed pixel by
/// pixel to `put` (index, 0–1) where it is not 0: inside 1, outside 0, its
/// edge softened over `feather` pixels the way `style` says, or with
/// `anti_alias` each edge pixel as much as the shape covers it (four rows
/// of samples, the spans' overlaps across). Only the shape's bounding box
/// (and its feather) is worked out.
fn shape_coverage(
    outline: &[[f64; 2]],
    w: usize,
    h: usize,
    feather: f64,
    style: MaskFeather,
    anti_alias: bool,
    mut put: impl FnMut(usize, f32),
) {
    if outline.len() < 3 || w == 0 || h == 0 {
        return;
    }
    let soft = if feather >= 1.0 { feather } else { 0.0 };
    let reach = soft + 3.0;
    let (lo, hi) = outline.iter().fold(
        ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]),
        |(lo, hi), p| {
            (
                [lo[0].min(p[0]), lo[1].min(p[1])],
                [hi[0].max(p[0]), hi[1].max(p[1])],
            )
        },
    );
    let x0 = (lo[0] - reach).floor().clamp(0.0, w as f64) as usize;
    let y0 = (lo[1] - reach).floor().clamp(0.0, h as f64) as usize;
    let x1 = (hi[0] + reach).ceil().clamp(0.0, w as f64) as usize;
    let y1 = (hi[1] + reach).ceil().clamp(0.0, h as f64) as usize;
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let (bw, bh) = (x1 - x0, y1 - y0);
    let at = |x: usize, y: usize| (y + y0) * w + x + x0;
    // Where the outline's edges cross the line `fy` (even-odd pairs, the
    // box's x).
    let crossings = |fy: f64, xs: &mut Vec<f64>| {
        xs.clear();
        for i in 0..outline.len() {
            let (a, b) = (outline[i], outline[(i + 1) % outline.len()]);
            if (a[1] <= fy) != (b[1] <= fy) {
                let t = (fy - a[1]) / (b[1] - a[1]);
                xs.push(a[0] + t * (b[0] - a[0]) - x0 as f64);
            }
        }
        xs.sort_by(|a, b| a.total_cmp(b));
    };
    let mut xs = Vec::new();
    if soft <= 0.0 && anti_alias {
        let mut row = vec![0.0f64; bw];
        for y in 0..bh {
            row.fill(0.0);
            for sub in [-0.375, -0.125, 0.125, 0.375] {
                crossings((y + y0) as f64 + sub, &mut xs);
                for pair in xs.chunks(2) {
                    if let [xa, xb] = pair {
                        let from = (xa + 0.5).floor().max(0.0) as usize;
                        let to = ((xb + 0.5).ceil().max(0.0) as usize).min(bw);
                        for (x, v) in row.iter_mut().enumerate().take(to).skip(from) {
                            let px = x as f64;
                            let o = (xb.min(px + 0.5) - xa.max(px - 0.5)).max(0.0);
                            *v += o / 4.0;
                        }
                    }
                }
            }
            for (x, v) in row.iter().enumerate() {
                if *v > 0.0 {
                    put(at(x, y), v.min(1.0) as f32);
                }
            }
        }
        return;
    }
    // Each row's middle: inside or not (rows on the processor's threads).
    let mut inside = vec![false; bw * bh];
    bv_color::par_rows(&mut inside, bw, |y, row| {
        let mut xs = Vec::new();
        crossings((y + y0) as f64, &mut xs);
        for pair in xs.chunks(2) {
            if let [xa, xb] = pair {
                let from = xa.ceil().max(0.0) as usize;
                let to = (xb.floor() + 1.0).clamp(0.0, bw as f64) as usize;
                for v in &mut row[from.min(bw)..to.max(from.min(bw))] {
                    *v = true;
                }
            }
        }
    });
    if soft <= 0.0 {
        for y in 0..bh {
            for x in 0..bw {
                if inside[y * bw + x] {
                    put(at(x, y), 1.0);
                }
            }
        }
        return;
    }
    // Feathered: by the signed distance to the edge (inside positive), in
    // pixels.
    let outside_d = distance(&inside, bw, bh, true, reach);
    let inside_d = distance(&inside, bw, bh, false, reach);
    let mut soft_cover = vec![0.0f32; bw * bh];
    bv_color::par_rows(&mut soft_cover, bw, |y, row| {
        for (x, out) in row.iter_mut().enumerate() {
            let i = y * bw + x;
            let d = if inside[i] {
                inside_d[i] - 0.5
            } else {
                0.5 - outside_d[i]
            };
            *out = match style {
                MaskFeather::In => smooth(0.0, soft, d),
                MaskFeather::Out => smooth(-soft, 0.0, d),
                MaskFeather::Both | MaskFeather::None => smooth(-soft / 2.0, soft / 2.0, d),
            } as f32;
        }
    });
    for (i, &c) in soft_cover.iter().enumerate() {
        if c > 0.0 {
            put(at(i % bw, i / bw), c);
        }
    }
}

fn smooth(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0).max(1e-9)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Each pixel's distance (pixels, up to `reach`) to the nearest pixel of
/// the other side: to an inside one from outside (`from_outside`), else to
/// an outside one (the box's edges count as outside). Felzenszwalb and
/// Huttenlocher's exact transform, a column then a row at a time.
fn distance(inside: &[bool], w: usize, h: usize, from_outside: bool, reach: f64) -> Vec<f64> {
    if w == 0 || h == 0 {
        return Vec::new();
    }
    let far = reach * reach + 1.0;
    // Down each column first, the columns kept as rows (the picture
    // turned), then along each row: rows on the processor's threads.
    let mut columns = vec![0.0f64; w * h];
    bv_color::par_rows(&mut columns, h, |x, out| {
        let line: Vec<f64> = (0..h)
            .map(|y| {
                if inside[y * w + x] == from_outside {
                    0.0
                } else {
                    far
                }
            })
            .collect();
        transform_1d(&line, out);
    });
    let mut f = vec![0.0f64; w * h];
    bv_color::par_rows(&mut f, w, |y, out| {
        let line: Vec<f64> = (0..w).map(|x| columns[x * h + y]).collect();
        transform_1d(&line, out);
        for v in out.iter_mut() {
            *v = v.sqrt().min(reach);
        }
    });
    f
}

/// The squared distance transform of a sampled function: the lower
/// envelope of the parabolas rooted at each sample.
fn transform_1d(f: &[f64], d: &mut [f64]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut v = vec![0usize; n];
    let mut z = vec![0.0f64; n + 1];
    let mut k = 0usize;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        let s = loop {
            let p = v[k];
            let s = ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2 * (q - p)) as f64;
            // z[0] is minus infinity: k never goes below 0.
            if s <= z[k] {
                k -= 1;
            } else {
                break s;
            }
        };
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    k = 0;
    for (q, out) in d.iter_mut().enumerate() {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let dq = q as f64 - v[k] as f64;
        *out = dq * dq + f[v[k]];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A feather's reach: a square grown or shrunk on every side, which
    /// ever way round it is drawn.
    #[test]
    fn polygons_grow_out_and_shrink_in() {
        let square = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let span = |p: &[[f64; 2]]| {
            let xs = p.iter().map(|q| q[0]);
            (
                xs.clone().fold(f64::MAX, f64::min),
                xs.fold(f64::MIN, f64::max),
            )
        };
        let out = offset_polygon(&square, 1.0);
        let (lo, hi) = span(&out);
        assert!(lo < 0.0 && hi > 10.0, "{out:?}");
        let mut back = square;
        back.reverse();
        let inside = offset_polygon(&back, -1.0);
        let (lo, hi) = span(&inside);
        assert!(lo > 0.0 && hi < 10.0, "{inside:?}");
    }

    fn s(x: i64) -> Tick {
        Tick::from_seconds(x)
    }

    /// A rectangle keeps what is inside it; negative, what is outside; an
    /// oval passes its middle and not its box's corners; feathered, the
    /// edge goes from nothing to all the way it says.
    #[test]
    fn masks_cover_what_they_say() {
        let mut m = PanCropMask {
            enabled: true,
            ..PanCropMask::default()
        };
        m.add_path(MaskPath::rectangle([10.25, 10.0], [30.0, 20.0]));
        assert!(m.active());
        let c = m.coverage(Tick::ZERO, 40, 30, [1.0, 1.0], 30.0);
        let at = |c: &[f32], x: usize, y: usize| c[y * 40 + x];
        assert!(at(&c, 20, 15) > 0.99 && at(&c, 2, 2) < 0.01);
        // Anti-aliased: the edge pixel as much as it is covered.
        assert!((at(&c, 10, 15) - 0.75).abs() < 1e-3, "{}", at(&c, 10, 15));
        assert!(at(&c, 9, 15) < 1e-6);
        m.paths[0].mode = MaskMode::Negative;
        let c = m.coverage(Tick::ZERO, 40, 30, [1.0, 1.0], 30.0);
        assert!(at(&c, 20, 15) < 0.01 && at(&c, 2, 2) > 0.99);
        // Half opacity: half of it goes.
        m.paths[0].opacity = 0.5;
        let c = m.coverage(Tick::ZERO, 40, 30, [1.0, 1.0], 30.0);
        assert!((at(&c, 20, 15) - 0.5).abs() < 1e-3);
        let mut o = PanCropMask {
            enabled: true,
            ..PanCropMask::default()
        };
        o.add_path(MaskPath::oval([0.0, 0.0], [40.0, 30.0]));
        let c = o.coverage(Tick::ZERO, 40, 30, [1.0, 1.0], 30.0);
        assert!(at(&c, 20, 15) > 0.99 && at(&c, 1, 1) < 0.01);
        // At half the picture's size, the same shape.
        let c = o.coverage(Tick::ZERO, 20, 15, [0.5, 0.5], 30.0);
        assert!(c[7 * 20 + 10] > 0.99 && c[0] < 0.01);
        // Feathered out by a tenth of 100 pixels: 10 pixels outside.
        let mut f = PanCropMask {
            enabled: true,
            ..PanCropMask::default()
        };
        let mut r = MaskPath::rectangle([40.0, 40.0], [60.0, 60.0]);
        r.feather = MaskFeather::Out;
        r.feather_pct = 10.0;
        f.add_path(r);
        let c = f.coverage(Tick::ZERO, 100, 100, [1.0, 1.0], 100.0);
        let row = |x: usize| c[50 * 100 + x];
        assert!(row(50) > 0.99 && row(45) > 0.99, "inside whole");
        assert!(row(35) > 0.05 && row(35) < 0.95, "softening outside");
        assert!(row(25) < 0.01, "nothing past the feather");
        // Disabled or off: nothing masked.
        f.paths[0].mode = MaskMode::Disabled;
        assert!(!f.active());
        let mut g = f.clone();
        g.paths[0].mode = MaskMode::Positive;
        g.enabled = false;
        assert!(!g.active());
    }

    /// An event's masks through edits: a cut gives each half the shapes
    /// where they were, trimming its start keeps them on the media, and the
    /// halves join back into one.
    #[test]
    fn masks_follow_cuts_trims_and_joins() {
        use crate::{Generator, MediaInfo, Project};
        let mut p = Project::default();
        let v = p.tracks[0].id;
        let m = p.add_media(MediaInfo::generated(Generator::ColorBars, &p.settings));
        let id = p.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
        let mut mask = PanCropMask {
            enabled: true,
            ..PanCropMask::default()
        };
        mask.add_path(MaskPath::rectangle([0.0, 0.0], [10.0, 10.0]));
        mask.add_key(Tick::ZERO);
        // Linear, so that a cut's halves keep the same motion exactly.
        mask.set_key_curve(Tick::ZERO, KeyCurve::Linear);
        mask.map_path_at(s(4), 0, |q| [q[0] + 40.0, q[1]]);
        p.clip_mut(id).unwrap().mask = Some(mask.clone());
        let right = p.split(id, s(2)).unwrap()[0];
        let x0 =
            |p: &Project, c, t| p.clip(c).unwrap().mask.as_ref().unwrap().shapes_at(t)[0][0].at[0];
        assert!(
            (x0(&p, id, s(2)) - 20.0).abs() < 1e-3,
            "the left half ends there"
        );
        assert!(
            (x0(&p, right, Tick::ZERO) - 20.0).abs() < 1e-3,
            "the right one starts there"
        );
        assert!((x0(&p, right, s(2)) - 40.0).abs() < 1e-3);
        assert!(p.rejoin(right).unwrap() || p.rejoin(id).unwrap());
        let joined = p.clip(id).unwrap().mask.as_ref().unwrap();
        assert!(
            (joined.shapes_at(s(1))[0][0].at[0] - mask.shapes_at(s(1))[0][0].at[0]).abs() < 1e-3
        );
        assert!((joined.shapes_at(s(4))[0][0].at[0] - 40.0).abs() < 1e-3);
        // Its start trimmed a second later: what was at 1 s is at its start.
        p.trim_start(id, s(1)).unwrap();
        assert!((x0(&p, id, Tick::ZERO) - mask.shapes_at(s(1))[0][0].at[0]).abs() < 1e-3);
        // Masks that differ in more than their shapes keep the halves apart.
        let right = p.split(id, s(3)).unwrap()[0];
        p.clip_mut(right)
            .unwrap()
            .mask
            .as_mut()
            .unwrap()
            .apply_to_fx = true;
        assert!(!p.rejoin(id).unwrap_or(false) && !p.rejoin(right).unwrap_or(false));
    }

    /// Paths moved, flipped and copied with their tangents.
    #[test]
    fn paths_move_flip_and_duplicate() {
        let mut m = PanCropMask {
            enabled: true,
            ..PanCropMask::default()
        };
        m.add_path(MaskPath::oval([0.0, 0.0], [20.0, 10.0]));
        m.map_path_at(Tick::ZERO, 0, |q| [40.0 - q[0], q[1]]);
        let a = m.paths[0].anchors[1];
        assert_eq!(a.at, [20.0, 5.0]);
        // The tangent flipped too.
        assert!(a.after[1] > 0.0 && a.after[0] == 0.0);
        let copy = m.duplicate_path(0, [5.0, 5.0]).unwrap();
        assert_eq!(m.paths[copy].anchors[1].at, [25.0, 10.0]);
        let (lo, hi) = m.paths[0].bounds().unwrap();
        assert_eq!((lo, hi), ([20.0, 0.0], [40.0, 10.0]));
        // An anchor put in at a keyframe: in the others too, between its
        // neighbors there; taken out of all of them.
        let mut r = PanCropMask::default();
        r.add_path(MaskPath::rectangle([0.0, 0.0], [10.0, 10.0]));
        r.add_key(Tick::ZERO);
        r.map_path_at(s(2), 0, |q| [q[0] + 10.0, q[1]]);
        r.insert_anchor(0, 1, s(2), MaskAnchor::corner([15.0, -5.0]));
        assert!(r.keys.iter().all(|k| k.shapes[0].len() == 5));
        assert_eq!(r.keys[0].shapes[0][1].at, [5.0, 0.0]);
        assert_eq!(r.keys[1].shapes[0][1].at, [15.0, -5.0]);
        r.remove_anchor(0, 1);
        assert!(r.keys.iter().all(|k| k.shapes[0].len() == 4));
        // Fewer than three left: the path goes.
        r.remove_anchor(0, 0);
        r.remove_anchor(0, 0);
        assert!(r.paths.is_empty() && r.keys.iter().all(|k| k.shapes.is_empty()));
    }

    /// Keyframes: shapes in between move anchor by anchor; a new path is
    /// at every keyframe; the event's start trimmed and a cut keep them.
    #[test]
    fn shapes_animate_and_follow_the_event() {
        let mut m = PanCropMask {
            enabled: true,
            ..PanCropMask::default()
        };
        m.add_path(MaskPath::rectangle([0.0, 0.0], [10.0, 10.0]));
        m.add_key(Tick::ZERO);
        let moved: Vec<Vec<MaskAnchor>> = m
            .shapes_at(Tick::ZERO)
            .into_iter()
            .map(|s| {
                s.into_iter()
                    .map(|mut a| {
                        a.at[0] += 20.0;
                        a
                    })
                    .collect()
            })
            .collect();
        m.set_shapes_at(s(2), moved);
        assert_eq!(m.keys.len(), 2);
        assert_eq!(m.shapes_at(s(1))[0][0].at, [10.0, 0.0]);
        m.add_path(MaskPath::oval([0.0, 0.0], [4.0, 4.0]));
        assert!(m.keys.iter().all(|k| k.shapes.len() == 2));
        m.remove_path(1);
        assert!(m.keys.iter().all(|k| k.shapes.len() == 1));
        // Trimmed by a second: what was at 1 s is at the start.
        let mut t = m.clone();
        t.rebase(s(1));
        assert_eq!(t.shapes_at(Tick::ZERO)[0][0].at, [10.0, 0.0]);
        assert_eq!(t.shapes_at(s(1))[0][0].at, [20.0, 0.0]);
        // Cut at 1 s.
        let mut left = m.clone();
        let right = left.split_at(s(1));
        assert_eq!(left.shapes_at(s(1))[0][0].at, [10.0, 0.0]);
        assert_eq!(right.shapes_at(Tick::ZERO)[0][0].at, [10.0, 0.0]);
        // Keyframes gone: the last one's shapes stay.
        m.delete_key(s(2));
        m.delete_key(Tick::ZERO);
        assert!(m.keys.is_empty());
        assert_eq!(m.paths[0].anchors[0].at, [0.0, 0.0]);
    }

    /// An anchor put on a curved segment keeps the curve (the two parts go
    /// through the same points); on a straight one it is a corner.
    #[test]
    fn splitting_a_segment_keeps_its_curve() {
        let a = MaskAnchor {
            at: [0.0, 0.0],
            before: [0.0, 0.0],
            after: [0.0, 40.0],
            split: false,
        };
        let b = MaskAnchor {
            at: [100.0, 0.0],
            before: [0.0, 40.0],
            after: [0.0, -10.0],
            split: false,
        };
        let c = MaskAnchor::corner([50.0, -60.0]);
        let mut m = PanCropMask {
            enabled: true,
            paths: vec![MaskPath::new(vec![a, b, c])],
            ..PanCropMask::default()
        };
        let curve: Vec<[f32; 2]> = (0..=10)
            .map(|k| segment_point(&m.paths[0].anchors, 0, k as f32 / 10.0))
            .collect();
        m.split_segment(0, 0, 0.3);
        let s = &m.paths[0].anchors;
        assert_eq!(s.len(), 4);
        for (k, got) in curve.iter().enumerate() {
            let t = k as f32 / 10.0;
            let want = if t <= 0.3 {
                segment_point(s, 0, t / 0.3)
            } else {
                segment_point(s, 1, (t - 0.3) / 0.7)
            };
            assert!(
                (want[0] - got[0]).abs() < 1e-3 && (want[1] - got[1]).abs() < 1e-3,
                "{t}: {want:?} {got:?}"
            );
        }
        // From the corner back to the first anchor, a straight line.
        m.split_segment(0, 3, 0.5);
        assert_eq!(m.paths[0].anchors[4], MaskAnchor::corner([25.0, -30.0]));
    }
}
