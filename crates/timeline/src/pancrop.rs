//! An event's Pan/Crop, as VEGAS's: a box over its media — its size, its
//! middle, its angle and where it turns, all in the media's pixels, keyframed
//! with a curve and a smoothness — that says which part of the media goes to
//! the frame. The smaller the box, the larger that part shows; a width or
//! height below zero mirrors it. Maintain aspect ratio and Stretch to fill
//! frame say where it lands ([`place`]).
//!
//! Without a turn or a mirror a rectangle of the media goes to a rectangle
//! of the frame, its edges hard; turned or mirrored, a parallelogram, its
//! edges as soft as the pixels they cross.

use bv_core::{TICKS_PER_SECOND, Tick};
use serde::{Deserialize, Serialize};

use crate::fx::{KeyCurve, Transform};

/// The box at a keyframe, times from the event's start.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PanCropKey {
    pub at: Tick,
    /// How it goes to the next keyframe.
    #[serde(default)]
    pub curve: KeyCurve,
    /// Its size, in the media's pixels; negative: mirrored.
    pub width: f64,
    pub height: f64,
    /// Its middle.
    pub center: [f64; 2],
    /// Its turn, in radians.
    #[serde(default)]
    pub angle: f64,
    /// Where it turns.
    pub pivot: [f64; 2],
    /// How much the way of the pivot curves through this keyframe, 0–1 (0:
    /// straight lines between keyframes).
    #[serde(default)]
    pub smoothness: f64,
    /// What the Properties panel's crop sliders take off the media: its
    /// left, right, top and bottom, as parts of its size.
    #[serde(default, skip_serializing_if = "no_crop")]
    pub crop: [f32; 4],
}

fn no_crop(c: &[f32; 4]) -> bool {
    *c == [0.0; 4]
}

fn yes() -> bool {
    true
}

fn is_yes(v: &bool) -> bool {
    *v
}

/// An event's Pan/Crop.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PanCrop {
    /// The media's size its keyframes count in: another size when it opens
    /// (or another take shows), they are scaled to it.
    #[serde(default)]
    pub size: [u32; 2],
    /// Maintain aspect ratio: what shows keeps its proportions in the frame
    /// (off: stretched to it).
    #[serde(default = "yes", skip_serializing_if = "is_yes")]
    pub keep_aspect: bool,
    /// Stretch to fill frame: the box becomes the frame (off: the picture
    /// stays where it is, the box only crops it).
    #[serde(default = "yes", skip_serializing_if = "is_yes")]
    pub fill: bool,
    /// In time order; none: the whole media all along.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<PanCropKey>,
}

impl Default for PanCrop {
    fn default() -> PanCrop {
        PanCrop {
            size: [0; 2],
            keep_aspect: true,
            fill: true,
            keys: Vec::new(),
        }
    }
}

/// 100 ns steps in a second: the unit the box's times are worked out in, a
/// frame's time cut down to it.
const UNITS: i128 = 10_000_000;

pub(crate) fn units_floor(t: Tick) -> i64 {
    (t.0 as i128 * UNITS).div_euclid(TICKS_PER_SECOND as i128) as i64
}

pub(crate) fn units_near(t: Tick) -> i64 {
    let d = TICKS_PER_SECOND as i128;
    ((t.0 as i128 * UNITS + d / 2).div_euclid(d)) as i64
}

/// A time from the event's start in the box's unit.
pub fn local_units(t: Tick) -> i64 {
    units_floor(t)
}

/// A keyframe curve's shape at `u` (0–1 of the way to the next keyframe).
pub fn ease(curve: KeyCurve, u: f64) -> f64 {
    match curve {
        KeyCurve::Hold => {
            if u < 1.0 {
                0.0
            } else {
                1.0
            }
        }
        KeyCurve::Slow => u * u,
        KeyCurve::Fast => 1.0 - (1.0 - u) * (1.0 - u),
        KeyCurve::Smooth => u * u * (3.0 - 2.0 * u),
        KeyCurve::Sharp => u * (u * (4.0 * u - 6.0) + 3.0),
        KeyCurve::Linear | KeyCurve::Bezier | KeyCurve::BezierSplit => u,
    }
}

fn lerp(a: f64, b: f64, u: f64) -> f64 {
    a + (b - a) * u
}

/// `a`'s cosine and sine, near 0 and ±1 exactly those.
fn cos_sin(a: f64) -> (f64, f64) {
    let (s, c) = a.sin_cos();
    if c.abs() < 1e-6 {
        (0.0, if s >= 0.0 { 1.0 } else { -1.0 })
    } else if s.abs() < 1e-6 {
        (if c >= 0.0 { 1.0 } else { -1.0 }, 0.0)
    } else {
        (c, s)
    }
}

/// The corners of a box: top left, top right, bottom right, bottom left.
pub type Corners = [[f64; 2]; 4];

impl PanCropKey {
    /// The box over all of a media of size `media`.
    pub fn whole(media: [f64; 2]) -> PanCropKey {
        PanCropKey {
            at: Tick::ZERO,
            curve: KeyCurve::Linear,
            width: media[0],
            height: media[1],
            center: [media[0] / 2.0, media[1] / 2.0],
            angle: 0.0,
            pivot: [media[0] / 2.0, media[1] / 2.0],
            smoothness: 0.0,
            crop: [0.0; 4],
        }
    }

    /// Whether it is the box [`Self::whole`] makes, wherever it is in time.
    pub fn is_whole(&self, media: [f64; 2]) -> bool {
        let w = PanCropKey::whole(media);
        self.width == w.width
            && self.height == w.height
            && self.center == w.center
            && self.angle == 0.0
            && self.pivot == w.pivot
            && no_crop(&self.crop)
    }

    /// Its corners.
    pub fn corners(&self) -> Corners {
        let (c, s) = cos_sin(self.angle);
        let (c, s) = (c / 2.0, s / 2.0);
        let [x, y] = self.center;
        let (w, h) = (self.width, self.height);
        [
            [x - (w * c - h * s), y - (w * s + h * c)],
            [x + (w * c + h * s), y + (w * s - h * c)],
            [x + (w * c - h * s), y + (w * s + h * c)],
            [x - (w * c + h * s), y - (w * s - h * c)],
        ]
    }

    /// Turned `by` radians more about its pivot.
    pub fn turned(&self, by: f64) -> PanCropKey {
        let (c, s) = cos_sin(by);
        let (dx, dy) = (
            self.center[0] - self.pivot[0],
            self.center[1] - self.pivot[1],
        );
        PanCropKey {
            angle: self.angle + by,
            center: [
                dx * c - dy * s + self.pivot[0],
                dy * c + dx * s + self.pivot[1],
            ],
            ..*self
        }
    }

    /// Moved, its pivot with it.
    pub fn moved(&self, by: [f64; 2]) -> PanCropKey {
        PanCropKey {
            center: [self.center[0] + by[0], self.center[1] + by[1]],
            pivot: [self.pivot[0] + by[0], self.pivot[1] + by[1]],
            ..*self
        }
    }

    /// Its size and its middle's distance from its pivot scaled.
    pub fn scaled(&self, by: [f64; 2]) -> PanCropKey {
        PanCropKey {
            width: self.width * by[0],
            height: self.height * by[1],
            center: [
                (self.center[0] - self.pivot[0]) * by[0] + self.pivot[0],
                (self.center[1] - self.pivot[1]) * by[1] + self.pivot[1],
            ],
            ..*self
        }
    }

    /// The menu's Center: moved so that its pivot is the media's middle.
    pub fn centered(&self, media: [f64; 2]) -> PanCropKey {
        self.moved([
            media[0] / 2.0 - self.pivot[0],
            media[1] / 2.0 - self.pivot[1],
        ])
    }

    /// The menu's Flip Horizontal: mirrored about its pivot, its turn
    /// mirrored too.
    pub fn flipped_h(&self) -> PanCropKey {
        PanCropKey {
            width: -self.width,
            center: [2.0 * self.pivot[0] - self.center[0], self.center[1]],
            angle: reflected(self.angle),
            ..*self
        }
    }

    /// The menu's Flip Vertical.
    pub fn flipped_v(&self) -> PanCropKey {
        PanCropKey {
            height: -self.height,
            center: [self.center[0], 2.0 * self.pivot[1] - self.center[1]],
            angle: reflected(self.angle),
            ..*self
        }
    }

    /// The menu's Match Output Aspect and Match Source Aspect: the shape
    /// `ratio` (width over height), as large as fits what it was; signs kept.
    pub fn matched(&self, ratio: f64) -> PanCropKey {
        let (w, h) = (self.width.abs(), self.height.abs());
        let (nw, nh) = if w < h * ratio {
            (w, w / ratio)
        } else {
            (h * ratio, h)
        };
        PanCropKey {
            width: nw.copysign(self.width),
            height: nh.copysign(self.height),
            ..*self
        }
    }

    /// The same box in a media of `to` pixels instead of `from`.
    fn rescaled(&self, from: [f64; 2], to: [f64; 2]) -> PanCropKey {
        let (x, y) = (to[0] / from[0], to[1] / from[1]);
        PanCropKey {
            width: self.width * x,
            height: self.height * y,
            center: [self.center[0] * x, self.center[1] * y],
            pivot: [self.pivot[0] * x, self.pivot[1] * y],
            ..*self
        }
    }
}

/// A turn as a mirror leaves it.
fn reflected(a: f64) -> f64 {
    use std::f64::consts::{FRAC_PI_2, PI, TAU};
    let m = a.abs();
    let mut k = (m / TAU).round() * TAU;
    let rest = m - k;
    if rest > FRAC_PI_2 {
        k += if rest > 3.0 * FRAC_PI_2 { TAU } else { PI };
    }
    (k - (m - k)) * if a < 0.0 { -1.0 } else { 1.0 }
}

/// A point of the pivot's way: when, where, how smooth.
#[derive(Clone, Copy)]
struct WayPoint {
    t: f64,
    at: [f64; 2],
    tension: f64,
}

/// A handle of the way through `p1`, from what comes before and after it.
fn handle(p0: &WayPoint, p1: &WayPoint, p2: &WayPoint) -> [f64; 2] {
    let d1 = (p1.at[0] - p0.at[0]).hypot(p1.at[1] - p0.at[1]);
    let d2 = (p2.at[0] - p1.at[0]).hypot(p2.at[1] - p1.at[1]);
    if d1 == 0.0 && d2 == 0.0 {
        return p1.at;
    }
    const K: f64 = 0.5;
    let (fa, fb) = if d2 <= d1 {
        ((d2 / d1) * K + 1.0, K)
    } else {
        (K + 1.0, (d1 / d2) * K)
    };
    [0, 1].map(|i| {
        let a = p0.at[i] + (p1.at[i] - p0.at[i]) * fa;
        let b = p1.at[i] + (p2.at[i] - p1.at[i]) * fb;
        (a + b) * 0.5
    })
}

/// A handle pulled toward a third of the straight way, as little as its
/// tension is high.
fn tensed(handle: [f64; 2], from: &WayPoint, to: &WayPoint) -> [f64; 2] {
    if from.tension >= 1.0 {
        return handle;
    }
    [0, 1].map(|i| {
        let line = from.at[i] + (to.at[i] - from.at[i]) / 3.0;
        line + (handle[i] - line) * from.tension
    })
}

/// The pivot's way at `u` between keyframes `i` and `i + 1` of `points`.
fn way_at(points: &[WayPoint], i: usize, u: f64) -> [f64; 2] {
    let last = points.len() - 1;
    let p0 = &points[i.saturating_sub(1)];
    let p1 = &points[i];
    let p2 = &points[(i + 1).min(last)];
    let p3 = &points[(i + 2).min(last)];
    let c1 = tensed(handle(p0, p1, p2), p1, p2);
    let c2 = tensed(handle(p3, p2, p1), p2, p1);
    [0, 1].map(|k| {
        let ab = lerp(p1.at[k], c1[k], u);
        let bc = lerp(c1[k], c2[k], u);
        let cd = lerp(c2[k], p2.at[k], u);
        lerp(lerp(ab, bc, u), lerp(bc, cd, u), u)
    })
}

impl PanCrop {
    /// Whether it has nothing to save: as a new event's.
    pub fn is_unset(&self) -> bool {
        self.keys.is_empty() && self.keep_aspect && self.fill
    }

    /// A Pan/Crop that holds `key` all along.
    pub fn holding(key: PanCropKey, media: [f64; 2], keep_aspect: bool, fill: bool) -> PanCrop {
        PanCrop {
            size: [media[0] as u32, media[1] as u32],
            keep_aspect,
            fill,
            keys: vec![PanCropKey {
                at: Tick::ZERO,
                ..key
            }],
        }
    }

    /// Whether it changes nothing: the whole media all along, as usual.
    pub fn is_default(&self) -> bool {
        self.fill
            && self.keep_aspect
            && match self.keys.as_slice() {
                [] => true,
                [k] => k.at == Tick::ZERO && k.is_whole([self.size[0] as f64, self.size[1] as f64]),
                _ => false,
            }
    }

    /// The media's size its keyframes count in.
    pub fn own_size(&self) -> [f64; 2] {
        [
            f64::from(self.size[0].max(1)),
            f64::from(self.size[1].max(1)),
        ]
    }

    /// Its keyframes in a media of size `media`.
    fn keys_in(&self, media: [f64; 2]) -> std::borrow::Cow<'_, [PanCropKey]> {
        let from = [f64::from(self.size[0]), f64::from(self.size[1])];
        if from[0] <= 0.0 || from[1] <= 0.0 || from == media {
            std::borrow::Cow::Borrowed(&self.keys)
        } else {
            std::borrow::Cow::Owned(self.keys.iter().map(|k| k.rescaled(from, media)).collect())
        }
    }

    /// The box `local` after the event's start: as far between its
    /// keyframes as their curve says, turned back to no angle while it
    /// goes and on again to the angle there, its pivot on the spline
    /// through theirs (`aspect`: how wide the media's pixels are, as the
    /// box turns in real proportions).
    pub fn at(&self, local: Tick, media: [f64; 2], aspect: f64) -> PanCropKey {
        self.at_units(units_floor(local), media, aspect)
    }

    /// [`Self::at`] at timeline time `t` of an event starting at `start`:
    /// the frame's time cut down to the box's unit, as the box is worked
    /// out.
    pub fn at_time(&self, t: Tick, start: Tick, media: [f64; 2], aspect: f64) -> PanCropKey {
        self.at_units(units_floor(t) - units_near(start), media, aspect)
    }

    fn at_units(&self, t: i64, media: [f64; 2], aspect: f64) -> PanCropKey {
        let keys = self.keys_in(media);
        let keys: &[PanCropKey] = &keys;
        let Some(first) = keys.first() else {
            return PanCropKey::whole(media);
        };
        let when = |k: &PanCropKey| units_near(k.at);
        let i = keys.partition_point(|k| when(k) <= t);
        if i == 0 {
            return PanCropKey {
                at: Tick::ZERO,
                ..*first
            };
        }
        let a = &keys[i - 1];
        if when(a) == t || i == keys.len() {
            return *a;
        }
        let b = &keys[i];
        let wide = aspect != 0.0 && aspect != 1.0;
        let in_wide = |k: &PanCropKey| {
            if wide {
                PanCropKey {
                    width: k.width * aspect,
                    center: [k.center[0] * aspect, k.center[1]],
                    pivot: [k.pivot[0] * aspect, k.pivot[1]],
                    ..*k
                }
            } else {
                *k
            }
        };
        let (ka, kb) = (in_wide(a), in_wide(b));
        let span = (when(b) - when(a)) as f64;
        let u = if span > 0.0 {
            ease(a.curve, (t - when(a)) as f64 / span)
        } else {
            0.0
        };
        let (flat_a, flat_b) = (ka.turned(-ka.angle), kb.turned(-kb.angle));
        let mut now = PanCropKey {
            at: a.at,
            curve: a.curve,
            width: lerp(ka.width, kb.width, u),
            height: lerp(ka.height, kb.height, u),
            center: [
                lerp(flat_a.center[0], flat_b.center[0], u),
                lerp(flat_a.center[1], flat_b.center[1], u),
            ],
            angle: 0.0,
            pivot: [
                lerp(ka.pivot[0], kb.pivot[0], u),
                lerp(ka.pivot[1], kb.pivot[1], u),
            ],
            smoothness: lerp(ka.smoothness, kb.smoothness, u),
            crop: std::array::from_fn(|c| {
                lerp(f64::from(a.crop[c]), f64::from(b.crop[c]), u) as f32
            }),
        };
        now = now.turned(lerp(ka.angle, kb.angle, u));
        // The pivot on its spline, the box staying where it is from it.
        let points: Vec<WayPoint> = keys
            .iter()
            .map(|k| {
                let k = in_wide(k);
                WayPoint {
                    t: when(&k) as f64,
                    at: k.pivot,
                    tension: k.smoothness,
                }
            })
            .collect();
        let way = way_at(&points, i - 1, u);
        debug_assert!(points[i - 1].t <= points[i].t);
        now.center = [
            now.center[0] + way[0] - now.pivot[0],
            now.center[1] + way[1] - now.pivot[1],
        ];
        now.pivot = way;
        if wide {
            now.width /= aspect;
            now.center[0] /= aspect;
            now.pivot[0] /= aspect;
        }
        now
    }

    /// The box at `at` (from the event's start) becomes `value`: without
    /// keyframes, at the start, what it holds all along; elsewhere the
    /// animation starts (a keyframe at the start with the box there, and
    /// this one); with keyframes, the one at `at`, made when there is none,
    /// with the curve of the one before. New keyframes get `new`, their
    /// curve when they are the first and their smoothness.
    pub fn set_at(&mut self, at: Tick, value: PanCropKey, media: [f64; 2], new: (KeyCurve, f64)) {
        self.ensure_size(media);
        let (curve, smoothness) = new;
        if self.keys.is_empty() {
            if at == Tick::ZERO {
                self.keys.push(PanCropKey {
                    at,
                    curve,
                    smoothness,
                    ..value
                });
                return;
            }
            self.keys.push(PanCropKey {
                curve,
                smoothness,
                ..PanCropKey::whole(media)
            });
        }
        match self.keys.iter_mut().find(|k| k.at == at) {
            Some(k) => {
                *k = PanCropKey {
                    at,
                    curve: k.curve,
                    smoothness: k.smoothness,
                    ..value
                };
            }
            None => {
                let before = self
                    .keys
                    .iter()
                    .rev()
                    .find(|k| k.at < at)
                    .map_or(curve, |k| k.curve);
                let i = self.keys.partition_point(|k| k.at < at);
                self.keys.insert(
                    i,
                    PanCropKey {
                        at,
                        curve: before,
                        smoothness,
                        ..value
                    },
                );
            }
        }
    }

    /// The keyframe at `at` gone; the last one left holds all along.
    pub fn remove_key(&mut self, at: Tick) {
        self.keys.retain(|k| k.at != at);
        if let [only] = self.keys.as_mut_slice() {
            only.at = Tick::ZERO;
        }
    }

    /// Records the media's size its keyframes count in, scaling them from
    /// another one they counted in.
    pub fn ensure_size(&mut self, media: [f64; 2]) {
        let keys = self.keys_in(media).into_owned();
        self.keys = keys;
        self.size = [media[0] as u32, media[1] as u32];
    }

    /// The event's start moved `delta` later in it: the box there is the
    /// first keyframe's, the later ones `delta` earlier.
    pub fn rebase(&mut self, delta: Tick, media: [f64; 2], aspect: f64) {
        if self.keys.is_empty() {
            return;
        }
        let before = self.keys.iter().rev().find(|k| k.at <= delta).copied();
        let first = PanCropKey {
            at: Tick::ZERO,
            curve: before.map_or(KeyCurve::Linear, |k| k.curve),
            ..self.at(delta, media, aspect)
        };
        let mut keys: Vec<PanCropKey> = self
            .keys
            .iter()
            .filter(|k| k.at > delta)
            .map(|k| PanCropKey {
                at: k.at - delta,
                ..*k
            })
            .collect();
        keys.insert(0, first);
        self.keys = keys;
    }

    /// The event cut at `cut` (from its start): this one keeps what is
    /// before (a keyframe at the cut ending it), the other half starts with
    /// the box at the cut.
    pub fn split_at(&mut self, cut: Tick, media: [f64; 2], aspect: f64) -> PanCrop {
        let mut right = self.clone();
        right.rebase(cut, media, aspect);
        if !self.keys.is_empty() {
            let there = self.at(cut, media, aspect);
            self.keys.retain(|k| k.at < cut);
            self.keys.push(PanCropKey {
                at: cut,
                curve: KeyCurve::Linear,
                ..there
            });
        }
        right
    }

    /// `next` follows this one, `offset` after its start (joining an event
    /// back after a cut).
    pub fn joined(&mut self, next: &PanCrop, offset: Tick, media: [f64; 2]) {
        if self.keys.is_empty() && next.keys.is_empty() {
            return;
        }
        if self.keys.is_empty() {
            self.keys.push(PanCropKey::whole(media));
        }
        let tail = if next.keys.is_empty() {
            vec![PanCropKey::whole(media)]
        } else {
            next.keys_in(media).into_owned()
        };
        // The keyframe a cut left at its end gives way to the next one's
        // first.
        self.keys.retain(|k| k.at < offset);
        self.keys.extend(tail.into_iter().map(|k| PanCropKey {
            at: k.at + offset,
            ..k
        }));
    }
}

/// What a box's place depends on besides itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaceIn {
    /// The media's size, and how wide its pixels are.
    pub media: [f64; 2],
    pub media_aspect: f64,
    /// The project's frame, and how wide its pixels are.
    pub frame: [f64; 2],
    pub frame_aspect: f64,
    pub keep_aspect: bool,
    pub fill: bool,
    /// How much a project's Adjust source media enlarges the picture.
    pub adjust: f64,
}

/// What an event of media `info` lands on in a project of `settings`.
pub fn place_in(
    settings: &crate::model::ProjectSettings,
    info: &crate::model::MediaInfo,
    keep_aspect: bool,
    fill: bool,
) -> PlaceIn {
    let frame = [f64::from(settings.width), f64::from(settings.height)];
    let (media, media_aspect) = info.video.map_or((frame, 1.0), |v| {
        (
            [f64::from(v.width), f64::from(v.height)],
            f64::from(v.pixel_aspect.ratio()),
        )
    });
    let frame_aspect = f64::from(settings.pixel_aspect.ratio());
    let adjust = match info.video {
        Some(v) if settings.adjust_source => f64::from(crate::scene::adjust_factor(
            v.width,
            v.height,
            v.pixel_aspect.ratio() / settings.pixel_aspect.ratio(),
            settings.width,
            settings.height,
        )),
        _ => 1.0,
    };
    PlaceIn {
        media,
        media_aspect,
        frame,
        frame_aspect,
        keep_aspect,
        fill,
        adjust,
    }
}

impl PlaceIn {
    /// Where event `clip`'s picture lands at `t` on the timeline, on a frame
    /// of `size` pixels.
    pub fn at(&self, clip: &crate::model::Clip, t: Tick, size: [f64; 2]) -> Placement {
        let key = clip.box_at(t, self.media, self.media_aspect);
        place(&key, self).on_frame(self.frame, size)
    }
}

/// Where a picture of the media lands on a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// From a point of the frame (pixels from its top left corner) to the
    /// media's: `[ux, uy, u0, vx, vy, v0]`.
    pub to_media: [f64; 6],
    /// What it may reach of the frame: left, top, right, bottom. A pixel
    /// outside goes, whole.
    pub clip: [f64; 4],
    /// Turned or mirrored: from a point of the frame to the box's own place
    /// (0–1 each way), as `to_media`. Its edges are as soft as the pixels
    /// they cross.
    pub to_box: Option<[f64; 6]>,
    /// The part of the media it shows: left, top, right, bottom.
    pub limits: [f64; 4],
    /// The size `to_media` and `limits` count in.
    pub media: [f64; 2],
    /// Without a turn or a mirror: the rectangle of the media and the one of
    /// the frame it goes to, each left, top, right, bottom.
    pub rect: Option<([f64; 4], [f64; 4])>,
    /// Nothing of it shows.
    pub empty: bool,
}

/// `v` in fixed point, the box's way through a rectangle's numbers.
const FIXED: f64 = 16384.0;

fn fixed(v: f64) -> f64 {
    (v * FIXED).trunc()
}

/// Where its box lands: the rectangle path, or the parallelogram's when it is
/// turned or mirrored.
pub fn place(key: &PanCropKey, s: &PlaceIn) -> Placement {
    let par = if s.media_aspect > 0.0 {
        s.media_aspect
    } else {
        1.0
    };
    let adjusted;
    let key = if s.adjust != 1.0 && s.adjust > 0.0 {
        adjusted = PanCropKey {
            width: key.width / s.adjust,
            height: key.height / s.adjust,
            ..*key
        };
        &adjusted
    } else {
        key
    };
    let corners = if par == 1.0 || key.angle == 0.0 {
        key.corners()
    } else {
        let mut c = PanCropKey {
            width: key.width * par,
            center: [key.center[0] * par, key.center[1]],
            pivot: [key.pivot[0] * par, key.pivot[1]],
            ..*key
        }
        .corners();
        for p in &mut c {
            p[0] /= par;
        }
        c
    };
    let [tl, tr, _, bl] = corners;
    if key.angle.abs() <= 1e-8 && tl[0] < tr[0] && tl[1] < bl[1] {
        rectangle(&corners, key, s, par)
    } else {
        parallelogram(&corners, key, s, par)
    }
}

fn limits_of(key: &PanCropKey, media: [f64; 2]) -> [f64; 4] {
    let [l, r, t, b] = key.crop.map(|c| f64::from(c.clamp(0.0, 1.0)));
    [
        l * media[0],
        t * media[1],
        (1.0 - r) * media[0],
        (1.0 - b) * media[1],
    ]
}

fn nothing(s: &PlaceIn) -> Placement {
    Placement {
        to_media: [0.0; 6],
        clip: [0.0; 4],
        to_box: None,
        limits: [0.0; 4],
        media: s.media,
        rect: None,
        empty: true,
    }
}

/// How much of a frame `w` × `h` a `width` × `height` picture of pixels
/// `ratio` times as wide takes, kept in shape: its width and height there.
fn fitted(width: f64, height: f64, ratio: f64, w: f64, h: f64) -> (f64, f64) {
    let wide = ratio * width;
    let shape = wide / height;
    if h / height <= w / wide {
        (shape * h, h)
    } else {
        (w, w / shape)
    }
}

fn rectangle(corners: &Corners, key: &PanCropKey, s: &PlaceIn, par: f64) -> Placement {
    let [w, h] = s.media;
    let [ow, oh] = s.frame;
    let ratio = par / s.frame_aspect;
    let xs = corners.map(|c| c[0]);
    let ys = corners.map(|c| c[1]);
    let min = |v: [f64; 4]| v.into_iter().fold(f64::INFINITY, f64::min);
    let max = |v: [f64; 4]| v.into_iter().fold(f64::NEG_INFINITY, f64::max);
    // The media's rectangle, the frame's it goes to and what it may reach,
    // all in fixed point.
    let from = [
        fixed(min(xs)),
        fixed(min(ys)),
        fixed(max(xs)),
        fixed(max(ys)),
    ];
    let frame = [0.0, 0.0, ow * FIXED, oh * FIXED];
    let (mut to, clip);
    if !s.keep_aspect {
        if s.fill {
            to = frame;
            clip = frame;
        } else {
            to = [
                (from[0] * (ow / w)).trunc(),
                (from[1] * (oh / h)).trunc(),
                (from[2] * (ow / w)).trunc(),
                (from[3] * (oh / h)).trunc(),
            ];
            clip = frame;
        }
    } else if s.fill {
        // The box fitted in the frame, in its middle.
        let (bw, bh) = ((from[2] - from[0]) / FIXED, (from[3] - from[1]) / FIXED);
        let (fw, fh) = fitted(bw, bh, ratio, ow, oh);
        to = [
            fixed((ow - fw) * 0.5),
            fixed((oh - fh) * 0.5),
            fixed((ow + fw) * 0.5),
            fixed((oh + fh) * 0.5),
        ];
        clip = to;
    } else {
        // The whole media fitted in the frame, the box where it is in it.
        let (fw, fh) = fitted(w, h, ratio, ow, oh);
        let (ox, oy) = ((ow - fw) * 0.5, (oh - fh) * 0.5);
        to = [
            fixed(from[0] * (fw / w) / FIXED + ox),
            fixed(from[1] * (fh / h) / FIXED + oy),
            fixed(from[2] * (fw / w) / FIXED + ox),
            fixed(from[3] * (fh / h) / FIXED + oy),
        ];
        clip = [fixed(ox), fixed(oy), fixed(ox + fw), fixed(oy + fh)];
    }
    // What of the box is off the media leaves it, and as much of the frame's.
    let limits = limits_of(key, s.media);
    let cut = [
        from[0].max((limits[0] * FIXED).ceil()),
        from[1].max((limits[1] * FIXED).ceil()),
        from[2].min((limits[2] * FIXED).floor()),
        from[3].min((limits[3] * FIXED).floor()),
    ];
    if cut != from {
        let ex = (to[2] - to[0]) / (from[2] - from[0]);
        let ey = (to[3] - to[1]) / (from[3] - from[1]);
        to = [
            (to[0] - (from[0] - cut[0]) * ex).trunc(),
            (to[1] - (from[1] - cut[1]) * ey).trunc(),
            (to[2] - (from[2] - cut[2]) * ex).trunc(),
            (to[3] - (from[3] - cut[3]) * ey).trunc(),
        ];
    }
    if to[2] <= to[0] || to[3] <= to[1] || cut[2] <= cut[0] || cut[3] <= cut[1] {
        return nothing(s);
    }
    let ex = (cut[2] - cut[0]) / (to[2] - to[0]);
    let ey = (cut[3] - cut[1]) / (to[3] - to[1]);
    Placement {
        to_media: [
            ex,
            0.0,
            (cut[0] - to[0] * ex) / FIXED,
            0.0,
            ey,
            (cut[1] - to[1] * ey) / FIXED,
        ],
        clip: [
            clip[0].max(to[0]) / FIXED,
            clip[1].max(to[1]) / FIXED,
            clip[2].min(to[2]) / FIXED,
            clip[3].min(to[3]) / FIXED,
        ],
        to_box: None,
        limits,
        media: s.media,
        rect: Some((cut.map(|v| v / FIXED), to.map(|v| v / FIXED))),
        empty: false,
    }
}

fn parallelogram(corners: &Corners, key: &PanCropKey, s: &PlaceIn, par: f64) -> Placement {
    let [w, h] = s.media;
    let [ow, oh] = s.frame;
    let ratio = par / s.frame_aspect;
    let map =
        |sx: f64, sy: f64, ox: f64, oy: f64| corners.map(|c| [c[0] * sx + ox, c[1] * sy + oy]);
    let (to, clip): (Corners, [f64; 4]) = match (s.keep_aspect, s.fill) {
        (false, true) => (
            [[0.0, 0.0], [ow, 0.0], [ow, oh], [0.0, oh]],
            [0.0, 0.0, ow, oh],
        ),
        // The picture stays still; only what crops it turns.
        (false, false) => (map(ow / w, oh / h, 0.0, 0.0), [0.0, 0.0, ow, oh]),
        (true, false) => {
            let (fw, fh) = fitted(w, h, ratio, ow, oh);
            let (ox, oy) = ((ow - fw) * 0.5, (oh - fh) * 0.5);
            (map(fw / w, fh / h, ox, oy), [ox, oy, ox + fw, oy + fh])
        }
        (true, true) => {
            let [tl, tr, _, bl] = *corners;
            let side = (((tl[0] - tr[0]) * par).hypot(tl[1] - tr[1])) / s.frame_aspect;
            let down = ((tl[0] - bl[0]) * par).hypot(tl[1] - bl[1]);
            if side < 1e-6 || down < 1e-6 {
                return nothing(s);
            }
            let (fx, fy) = (ow / side, oh / down);
            let (mut x0, mut x1, mut y0, mut y1) = (0.0, ow, 0.0, oh);
            if fy <= fx {
                let m = (fy / fx) * (ow * 0.5);
                (x0, x1) = (ow * 0.5 - m, ow * 0.5 + m);
            } else {
                let m = (fx / fy) * (oh * 0.5);
                (y0, y1) = (oh * 0.5 - m, oh * 0.5 + m);
            }
            ([[x0, y0], [x1, y0], [x1, y1], [x0, y1]], [x0, y0, x1, y1])
        }
    };
    let [q, qr, _, ql] = to;
    let (ux, uy) = (qr[0] - q[0], qr[1] - q[1]);
    let (vx, vy) = (ql[0] - q[0], ql[1] - q[1]);
    let det = ux * vy - uy * vx;
    if det.abs() < 1e-9 {
        return nothing(s);
    }
    // From the frame to the box's own place...
    let (ax, ay) = (vy / det, -vx / det);
    let (bx, by) = (-uy / det, ux / det);
    let a0 = -(ax * q[0] + ay * q[1]);
    let b0 = -(bx * q[0] + by * q[1]);
    // ...and on to the media.
    let [tl, tr, _, bl] = *corners;
    let (sx, sy) = (tr[0] - tl[0], tr[1] - tl[1]);
    let (dx, dy) = (bl[0] - tl[0], bl[1] - tl[1]);
    Placement {
        to_media: [
            sx * ax + dx * bx,
            sx * ay + dx * by,
            tl[0] + sx * a0 + dx * b0,
            sy * ax + dy * bx,
            sy * ay + dy * by,
            tl[1] + sy * a0 + dy * b0,
        ],
        clip,
        to_box: Some([ax, ay, a0, bx, by, b0]),
        limits: limits_of(key, s.media),
        media: s.media,
        rect: None,
        empty: false,
    }
}

impl Placement {
    /// The same place on a frame of `size` pixels instead of `from`.
    pub fn on_frame(&self, from: [f64; 2], size: [f64; 2]) -> Placement {
        let (x, y) = (from[0] / size[0], from[1] / size[1]);
        let scale = |m: [f64; 6]| [m[0] * x, m[1] * y, m[2], m[3] * x, m[4] * y, m[5]];
        Placement {
            to_media: scale(self.to_media),
            clip: [
                self.clip[0] / x,
                self.clip[1] / y,
                self.clip[2] / x,
                self.clip[3] / y,
            ],
            to_box: self.to_box.map(scale),
            rect: self
                .rect
                .map(|(from, to)| (from, [to[0] / x, to[1] / y, to[2] / x, to[3] / y])),
            ..*self
        }
    }

    /// `to_media` and `limits` for a picture of the media `size` big, as a
    /// picture is read: without a turn or a mirror, the pixels at a
    /// rectangle's ends land on the pixels at the other's ends.
    pub fn in_picture(&self, size: [f64; 2]) -> ([f64; 6], [f64; 4]) {
        let (x, y) = (size[0] / self.media[0], size[1] / self.media[1]);
        let m = self.to_media;
        let limits = [
            self.limits[0] * x,
            self.limits[1] * y,
            self.limits[2] * x,
            self.limits[3] * y,
        ];
        let Some((from, to)) = self.rect else {
            return (
                [m[0] * x, m[1] * x, m[2] * x, m[3] * y, m[4] * y, m[5] * y],
                limits,
            );
        };
        // How far a pixel of the frame goes in the picture, and where the
        // first one is there.
        let across = |lo: f64, hi: f64, at: f64, end: f64, k: f64| {
            let (s, d) = ((hi - lo) * k, end - at);
            if s > 1.0 && d > 1.0 {
                let step = (s - 1.0) / (d - 1.0);
                (step, lo * k + 0.5 - (at + 0.5) * step)
            } else {
                let step = s / d.max(1e-9);
                (step, lo * k - at * step)
            }
        };
        let (ux, u0) = across(from[0], from[2], to[0], to[2], x);
        let (vy, v0) = across(from[1], from[3], to[1], to[3], y);
        ([ux, 0.0, u0, 0.0, vy, v0], limits)
    }

    /// How many of the media's pixels go across a pixel of the frame, each
    /// way.
    pub fn media_per_pixel(&self) -> (f64, f64) {
        let m = self.to_media;
        (m[0].hypot(m[1]), m[3].hypot(m[4]))
    }

    /// How many of a `w` × `h` picture's pixels go across a pixel of the
    /// frame, each way.
    pub fn picture_per_pixel(&self, w: u32, h: u32) -> (f64, f64) {
        let (x, y) = self.media_per_pixel();
        (
            x * f64::from(w) / self.media[0],
            y * f64::from(h) / self.media[1],
        )
    }

    /// Whether half of a `w` × `h` picture would still cover the frame's
    /// pixels.
    pub fn reduces(&self, w: u32, h: u32) -> bool {
        if self.empty {
            return false;
        }
        let (x, y) = self.picture_per_pixel(w, h);
        x >= 2.0 && y >= 2.0
    }

    /// How large a picture of the media has to be for a pixel of it to
    /// show as at most a pixel of the frame.
    pub fn size_needed(&self) -> (u32, u32) {
        if self.empty {
            return (1, 1);
        }
        let (x, y) = self.media_per_pixel();
        (
            (self.media[0] / x.max(1e-6)).ceil().max(1.0) as u32,
            (self.media[1] / y.max(1e-6)).ceil().max(1.0) as u32,
        )
    }

    /// Where a point of the media lands on the frame.
    pub fn on_frame_point(&self, p: [f64; 2]) -> [f64; 2] {
        let m = self.to_media;
        let det = m[0] * m[4] - m[1] * m[3];
        if det == 0.0 {
            return [0.0; 2];
        }
        let (du, dv) = (p[0] - m[2], p[1] - m[5]);
        [(du * m[4] - dv * m[1]) / det, (dv * m[0] - du * m[3]) / det]
    }

    /// The pixels of the frame it may reach, from its size: left, top,
    /// right, bottom (a pixel counts by its middle).
    pub fn pixels(&self, size: (u32, u32)) -> [i32; 4] {
        if self.empty {
            return [0; 4];
        }
        let first = |v: f64, n: u32| ((v - 0.5).ceil().max(0.0) as i64).min(i64::from(n)) as i32;
        [
            first(self.clip[0], size.0),
            first(self.clip[1], size.1),
            first(self.clip[2], size.0),
            first(self.clip[3], size.1),
        ]
    }

    /// The same place with hard edges: for what goes with a picture, whose
    /// own edges already soften (its mask).
    pub fn hard(&self) -> Placement {
        Placement {
            to_box: None,
            ..*self
        }
    }

    /// Its soft edges, when it has them.
    pub fn soft_edges(&self) -> Option<SoftEdges> {
        let b = self.to_box?;
        let m = self.to_media;
        let axis = |g: [f64; 3], lo: f64, hi: f64| {
            let len = g[0].hypot(g[1]);
            SoftAxis {
                g,
                lo,
                hi,
                len,
                nx: g[0].abs() / len.max(1e-300),
                ny: g[1].abs() / len.max(1e-300),
            }
        };
        // The media's own edges are in how its picture is read; a crop's are
        // lines like the box's.
        let [l, t, r, d] = self.crop_lines();
        Some(SoftEdges([
            axis([b[0], b[1], b[2]], 0.0, 1.0),
            axis([b[3], b[4], b[5]], 0.0, 1.0),
            axis([m[0], m[1], m[2]], l, r),
            axis([m[3], m[4], m[5]], t, d),
        ]))
    }

    /// What a crop leaves of the media (left, top, right, bottom): all there
    /// is, far beyond its edges, without one.
    pub fn crop_lines(&self) -> [f64; 4] {
        const FAR: f64 = 1e7;
        let [l, t, r, b] = self.limits;
        [
            if l > 0.0 { l } else { -FAR },
            if t > 0.0 { t } else { -FAR },
            if r < self.media[0] { r } else { FAR },
            if b < self.media[1] { b } else { FAR },
        ]
    }

    /// How much of a frame pixel `(x, y)` the picture covers.
    pub fn coverage(&self, x: f64, y: f64) -> f64 {
        let (cx, cy) = (x + 0.5, y + 0.5);
        if cx < self.clip[0] || cx >= self.clip[2] || cy < self.clip[1] || cy >= self.clip[3] {
            return 0.0;
        }
        self.soft_edges().map_or(1.0, |e| e.share(cx, cy))
    }
}

#[derive(Clone, Copy, Debug)]
struct SoftAxis {
    g: [f64; 3],
    lo: f64,
    hi: f64,
    len: f64,
    nx: f64,
    ny: f64,
}

/// A turned or mirrored picture's edges: the box's and the media's, each two
/// lines on the frame.
#[derive(Clone, Copy, Debug)]
pub struct SoftEdges([SoftAxis; 4]);

impl SoftEdges {
    /// How much of the pixel whose middle is `(x, y)` is inside them, its
    /// edge's coverage over sixteen bands a line, each at its top.
    pub fn share(&self, x: f64, y: f64) -> f64 {
        let y = y - 1.0 / 32.0;
        let mut c = 1.0f64;
        for a in &self.0 {
            if a.len <= 0.0 {
                continue;
            }
            let v = a.g[0] * x + a.g[1] * y + a.g[2];
            let (lo, hi) = ((v - a.lo) / a.len, (a.hi - v) / a.len);
            if lo < 1.0 {
                c = c.min(half_plane(lo, a.nx, a.ny));
            }
            if hi < 1.0 {
                c = c.min(half_plane(hi, a.nx, a.ny));
            }
        }
        c
    }
}

/// How much of a unit pixel lies on the inner side of a line `d` from its
/// middle, the line's normal `(nx, ny)` (a unit vector, both not negative).
pub fn half_plane(d: f64, nx: f64, ny: f64) -> f64 {
    let (a, b) = if nx >= ny { (nx, ny) } else { (ny, nx) };
    let reach = (a + b) * 0.5;
    if d >= reach {
        return 1.0;
    }
    if d <= -reach {
        return 0.0;
    }
    let flat = (a - b) * 0.5;
    if d.abs() <= flat {
        return 0.5 + d / a;
    }
    let corner = (reach - d.abs()) * (reach - d.abs()) / (2.0 * a * b);
    if d > 0.0 { 1.0 - corner } else { corner }
}

/// The box a framing of the older kind shows: a picture fitted in the frame,
/// moved, zoomed, turned and mirrored. Projects and presets saved with it
/// come in through here, and the Properties panel's sliders show the box so.
pub fn key_from_transform(tf: &Transform, s: &PlaceIn) -> PanCropKey {
    let [mw, mh] = s.media;
    let [ow, oh] = s.frame;
    let ratio = s.media_aspect / s.frame_aspect;
    // The frame's pixels per media pixel: fitted, zoomed, stretched.
    let (fw, _) = fitted(mw, mh, ratio, ow, oh);
    let kx = fw / mw * f64::from(tf.scale.max(1e-4));
    let ky = kx / ratio.max(1e-6) * f64::from(tf.stretch.max(1e-4));
    let (fx, fy) = (
        if tf.flip_h { -1.0 } else { 1.0 },
        if tf.flip_v { -1.0 } else { 1.0 },
    );
    let turn = -f64::from(tf.rotation_deg).to_radians();
    let (sin, cos) = turn.sin_cos();
    let (tx, ty) = (f64::from(tf.x) * ow, -f64::from(tf.y) * oh);
    let center = [
        mw / 2.0 - fx * (tx * cos - ty * sin) / kx,
        mh / 2.0 - fy * (tx * sin + ty * cos) / ky,
    ];
    let angle = turn * fx * fy;
    let (width, height) = (fx * ow / kx, fy * oh / ky);
    // Where it turned: from its middle, in parts of its size, on its axes.
    let (px, py) = (
        f64::from(tf.pivot[0]) * width.abs() * f64::from(tf.box_size[0]),
        f64::from(tf.pivot[1]) * height.abs() * f64::from(tf.box_size[1]),
    );
    let (sa, ca) = angle.sin_cos();
    PanCropKey {
        at: Tick::ZERO,
        curve: KeyCurve::Linear,
        width,
        height,
        center,
        angle,
        pivot: [center[0] + px * ca - py * sa, center[1] + px * sa + py * ca],
        smoothness: 0.0,
        crop: [tf.crop_left, tf.crop_right, tf.crop_top, tf.crop_bottom],
    }
}

/// The framing of the older kind closest to a box: [`key_from_transform`]
/// the other way round.
pub fn transform_from_key(key: &PanCropKey, s: &PlaceIn) -> Transform {
    let [mw, mh] = s.media;
    let [ow, oh] = s.frame;
    let ratio = s.media_aspect / s.frame_aspect;
    let (fw, _) = fitted(mw, mh, ratio, ow, oh);
    let fit = fw / mw;
    let (w, h) = (key.width.abs().max(1e-6), key.height.abs().max(1e-6));
    let (kx, ky) = if !s.fill {
        (fit, fit / ratio.max(1e-6))
    } else if s.keep_aspect {
        let k = fitted(w, h, ratio, ow, oh).0 / w;
        (k, k / ratio.max(1e-6))
    } else {
        (ow / w, oh / h)
    };
    let (fx, fy) = (
        if key.width < 0.0 { -1.0 } else { 1.0 },
        if key.height < 0.0 { -1.0 } else { 1.0 },
    );
    let turn = key.angle * fx * fy;
    let (sin, cos) = turn.sin_cos();
    let rx = (mw / 2.0 - key.center[0]) * kx * fx;
    let ry = (mh / 2.0 - key.center[1]) * ky * fy;
    let (tx, ty) = (rx * cos + ry * sin, -rx * sin + ry * cos);
    Transform {
        x: (tx / ow) as f32,
        y: (-ty / oh) as f32,
        scale: (kx / fit) as f32,
        stretch: (ky / (kx / ratio.max(1e-6))) as f32,
        rotation_deg: (-turn).to_degrees() as f32,
        crop_left: key.crop[0],
        crop_right: key.crop[1],
        crop_top: key.crop[2],
        crop_bottom: key.crop[3],
        flip_h: key.width < 0.0,
        flip_v: key.height < 0.0,
        ..Transform::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEDIA: [f64; 2] = [64.0, 36.0];

    /// A box of another shape than the frame's keeps the picture's shape:
    /// the frame shows it whole, with nothing beside it stretched.
    #[test]
    fn a_box_of_its_own_shape_shows_whole() {
        let s = sizes(true, true);
        let square = PanCropKey::whole(s.media).matched(1.0);
        assert_eq!((square.width, square.height), (300.0, 300.0));
        assert_eq!(square.center, [200.0, 150.0]);
        let wide = PanCropKey::whole(s.media).matched(16.0 / 9.0);
        assert_eq!(wide.width, 400.0);
        assert!((wide.height - 225.0).abs() < 1e-9);
        let on = place(&square, &s);
        // 1:1 in a 640×360 frame: as tall as the frame, as wide as tall.
        let (_, dest) = on.rect.expect("a straight box");
        assert!((dest[2] - dest[0] - 360.0).abs() < 1e-6, "{dest:?}");
        assert!((dest[3] - dest[1] - 360.0).abs() < 1e-6, "{dest:?}");
    }

    fn sizes(keep_aspect: bool, fill: bool) -> PlaceIn {
        PlaceIn {
            media: [400.0, 300.0],
            media_aspect: 1.0,
            frame: [640.0, 360.0],
            frame_aspect: 1.0,
            keep_aspect,
            fill,
            adjust: 1.0,
        }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn the_box_moves_scales_and_turns_as_its_commands_say() {
        let k = PanCropKey::whole(MEDIA);
        assert_eq!(k.corners()[0], [0.0, 0.0]);
        assert_eq!(k.corners()[2], [64.0, 36.0]);
        // Both middles move.
        let m = k.moved([10.0, 5.0]);
        assert_eq!((m.center, m.pivot), ([42.0, 23.0], [42.0, 23.0]));
        // Its size and how far its middle is from its pivot.
        let s = k.scaled([0.5, 0.25]);
        assert_eq!((s.width, s.height), (32.0, 9.0));
        let t = k.turned(0.5);
        assert_eq!(t.angle, 0.5);
        assert_eq!(t.center, k.center);
        // Turned about a pivot off its middle, the middle goes round it.
        let off = PanCropKey {
            pivot: [10.0, 20.0],
            ..k
        };
        let t = off.turned(std::f64::consts::FRAC_PI_2);
        assert!(close(t.center[0], 10.0 - (18.0 - 20.0)));
        assert!(close(t.center[1], 20.0 + (32.0 - 10.0)));
    }

    #[test]
    fn the_curves_have_their_shapes() {
        assert_eq!(ease(KeyCurve::Linear, 0.25), 0.25);
        assert_eq!(ease(KeyCurve::Hold, 0.99), 0.0);
        assert_eq!(ease(KeyCurve::Hold, 1.0), 1.0);
        assert_eq!(ease(KeyCurve::Slow, 0.5), 0.25);
        assert_eq!(ease(KeyCurve::Fast, 0.5), 0.75);
        assert_eq!(ease(KeyCurve::Smooth, 0.5), 0.5);
        assert_eq!(ease(KeyCurve::Sharp, 0.5), 0.5);
        assert!(close(
            ease(KeyCurve::Sharp, 0.25),
            0.25 * (0.25 - 1.5 + 3.0)
        ));
    }

    fn two_keys(smoothness: f64) -> PanCrop {
        let a = PanCropKey {
            smoothness,
            ..PanCropKey::whole(MEDIA)
        };
        let second = PanCropKey {
            at: Tick::from_seconds(1),
            smoothness,
            ..a.moved([20.0, 0.0])
        };
        let third = PanCropKey {
            at: Tick::from_seconds(2),
            smoothness,
            ..second.moved([0.0, 20.0])
        };
        PanCrop {
            size: [64, 36],
            keys: vec![a, second, third],
            ..PanCrop::default()
        }
    }

    #[test]
    fn between_keyframes_the_box_goes_straight_without_smoothness() {
        let p = two_keys(0.0);
        let half = Tick(TICKS_PER_SECOND / 2);
        let k = p.at(half, MEDIA, 1.0);
        assert!(close(k.center[0], 42.0), "{:?}", k.center);
        assert!(close(k.center[1], 18.0), "{:?}", k.center);
        assert!(close(k.pivot[0], 42.0));
    }

    #[test]
    fn smoothness_curves_the_way_and_keeps_the_keyframes() {
        let straight = two_keys(0.0);
        let smooth = two_keys(1.0);
        let half = Tick::from_seconds(1) + Tick(TICKS_PER_SECOND / 2);
        let (a, b) = (straight.at(half, MEDIA, 1.0), smooth.at(half, MEDIA, 1.0));
        assert!(close(a.center[0], 52.0));
        // Past the corner, the curve leans out of the straight way.
        assert!(b.center[0] > a.center[0] + 0.5, "{:?}", b.center);
        // On a keyframe there is no difference.
        let on = Tick::from_seconds(1);
        assert_eq!(
            straight.at(on, MEDIA, 1.0).center,
            smooth.at(on, MEDIA, 1.0).center
        );
    }

    #[test]
    fn a_turn_goes_round_the_pivot_between_keyframes() {
        let a = PanCropKey::whole(MEDIA);
        let b = PanCropKey {
            at: Tick::from_seconds(1),
            ..a.turned(1.0)
        };
        let p = PanCrop {
            size: [64, 36],
            keys: vec![a, b],
            ..PanCrop::default()
        };
        let k = p.at(Tick(TICKS_PER_SECOND / 2), MEDIA, 1.0);
        assert!(close(k.angle, 0.5));
        assert!(close(k.center[0], 32.0) && close(k.center[1], 18.0));
    }

    #[test]
    fn a_time_before_the_first_keyframe_or_past_the_last_holds_it() {
        let p = two_keys(0.0);
        assert_eq!(p.at(Tick::ZERO, MEDIA, 1.0).center, [32.0, 18.0]);
        assert_eq!(p.at(Tick::from_seconds(9), MEDIA, 1.0).center, [52.0, 38.0]);
    }

    #[test]
    fn another_media_size_scales_the_keyframes() {
        let p = two_keys(0.0);
        let k = p.at(Tick::from_seconds(1), [128.0, 72.0], 1.0);
        assert_eq!((k.width, k.height), (128.0, 72.0));
        assert_eq!(k.center, [104.0, 36.0]);
    }

    #[test]
    fn the_whole_media_lands_fitted_in_the_frame() {
        let s = sizes(true, true);
        let p = place(&PanCropKey::whole(s.media), &s);
        assert!(p.to_box.is_none());
        assert_eq!(p.clip, [80.0, 0.0, 560.0, 360.0]);
        assert!(close(p.to_media[0], 400.0 / 480.0));
        assert!(close(p.to_media[2], -80.0 * 400.0 / 480.0));
        assert!(close(p.to_media[4], 300.0 / 360.0));
        // Not keeping its shape, it fills the frame.
        let s = sizes(false, true);
        let p = place(&PanCropKey::whole(s.media), &s);
        assert_eq!(p.clip, [0.0, 0.0, 640.0, 360.0]);
        assert!(close(p.to_media[0], 400.0 / 640.0));
    }

    #[test]
    fn without_filling_the_box_only_crops_where_the_picture_is() {
        let s = sizes(true, false);
        let k = PanCropKey::whole(s.media).scaled([0.5, 0.5]);
        let p = place(&k, &s);
        // The media fitted at x 80, 1.2 frame pixels each; the box in its middle.
        assert_eq!(p.clip, [200.0, 90.0, 440.0, 270.0]);
        assert!(close(p.to_media[0], 1.0 / 1.2));
        assert!(close(p.to_media[0] * 200.0 + p.to_media[2], 100.0));
    }

    #[test]
    fn what_is_off_the_media_leaves_the_frame_too() {
        let s = sizes(true, true);
        let k = PanCropKey::whole(s.media)
            .scaled([1.5, 1.5])
            .moved([150.0, 100.0]);
        // A box of 600 x 450 from (50, 25), fitted: 480 x 360, so 0.8 frame
        // pixels each; the media ends 350 x 275 into it.
        let p = place(&k, &s);
        assert_eq!(p.clip, [80.0, 0.0, 360.0, 220.0]);
        // That frame pixel shows the box's corner.
        assert!(close(p.to_media[0] * 80.0 + p.to_media[2], 50.0));
        assert!(close(p.to_media[4] * 0.0 + p.to_media[5], 25.0));
    }

    #[test]
    fn a_box_off_the_media_shows_nothing() {
        let s = sizes(true, true);
        let k = PanCropKey::whole(s.media).moved([1000.0, 0.0]);
        assert!(place(&k, &s).empty);
    }

    #[test]
    fn a_turned_box_goes_to_the_frames_rectangle() {
        let s = sizes(true, true);
        let k = PanCropKey::whole(s.media).turned(0.5);
        let p = place(&k, &s);
        let b = p.to_box.expect("a parallelogram");
        assert!(
            close(p.clip[0], 80.0) && close(p.clip[2], 560.0),
            "{:?}",
            p.clip
        );
        // The frame's rectangle's corner is the box's.
        let tl = k.corners()[0];
        let m = p.to_media;
        assert!(close(m[0] * 80.0 + m[2], tl[0]), "{m:?} {tl:?}");
        assert!(close(m[3] * 80.0 + m[5], tl[1]), "{m:?} {tl:?}");
        assert!(close(b[0] * 80.0 + b[2], 0.0));
        assert!(close(b[3] * 80.0 + b[5], 0.0));
        // Well inside it covers the pixel; past the clip, nothing.
        assert_eq!(p.coverage(320.0, 180.0), 1.0);
        assert_eq!(p.coverage(60.0, 2.0), 0.0);
    }

    #[test]
    fn without_filling_a_turned_box_turns_only_its_outline() {
        let s = sizes(true, false);
        let k = PanCropKey::whole(s.media).scaled([0.7, 0.7]).turned(0.5);
        let p = place(&k, &s);
        let m = p.to_media;
        // The picture stays still: no cross terms.
        assert!(m[1].abs() < 1e-9 && m[3].abs() < 1e-9, "{m:?}");
        assert!(close(m[0], 1.0 / 1.2) && close(m[4], 1.0 / 1.2));
    }

    #[test]
    fn a_mirrored_box_reads_the_media_backward() {
        let s = sizes(true, true);
        let k = PanCropKey::whole(s.media).flipped_h();
        assert_eq!(k.width, -400.0);
        let p = place(&k, &s);
        assert!(p.to_box.is_some());
        assert!(close(p.to_media[0], -400.0 / 480.0), "{:?}", p.to_media);
        assert!(close(p.to_media[0] * 80.0 + p.to_media[2], 400.0));
    }

    #[test]
    fn flipping_twice_gives_the_box_back() {
        let k = PanCropKey {
            pivot: [10.0, 20.0],
            ..PanCropKey::whole(MEDIA).turned(0.3)
        };
        let back = k.flipped_h().flipped_h();
        assert!(close(back.width, k.width));
        assert!(close(back.center[0], k.center[0]));
        assert!(close(back.angle, k.angle), "{} {}", back.angle, k.angle);
    }

    #[test]
    fn matching_an_aspect_keeps_what_fits() {
        let k = PanCropKey::whole([300.0, 300.0]).matched(16.0 / 9.0);
        assert!(close(k.width, 300.0));
        assert!(close(k.height, 300.0 * 9.0 / 16.0));
        let k = PanCropKey {
            width: -100.0,
            ..PanCropKey::whole([300.0, 300.0])
        }
        .matched(1.0);
        assert_eq!((k.width, k.height), (-100.0, 100.0));
    }

    #[test]
    fn a_line_cuts_a_pixel_by_its_area() {
        // Square on: as far as it goes.
        assert_eq!(half_plane(0.0, 1.0, 0.0), 0.5);
        assert_eq!(half_plane(0.25, 1.0, 0.0), 0.75);
        assert_eq!(half_plane(1.0, 1.0, 0.0), 1.0);
        assert_eq!(half_plane(-1.0, 1.0, 0.0), 0.0);
        // Along the diagonal, a triangle's area.
        let n = std::f64::consts::FRAC_1_SQRT_2;
        assert!(close(half_plane(0.0, n, n), 0.5));
        assert!(close(half_plane(-n / 2.0, n, n), 0.125));
        assert!(close(half_plane(n / 2.0, n, n), 0.875));
    }

    #[test]
    fn an_older_framing_lands_where_it_did() {
        // Moved, zoomed: the point of the frame's middle.
        let s = sizes(true, true);
        let tf = Transform {
            x: 0.25,
            y: 0.1,
            scale: 2.0,
            ..Transform::default()
        };
        let k = key_from_transform(&tf, &s);
        // A frame-shaped box: 640 / 2.4 pixels of the media wide.
        assert!(close(k.width, 640.0 / 2.4));
        assert!(close(k.height, 360.0 / 2.4));
        // The picture's middle is 0.25 of the frame right and 0.1 up: the
        // frame's middle shows what is left of and below the media's.
        assert!(close(k.center[0], 200.0 - 160.0 / 2.4));
        assert!(close(k.center[1], 150.0 + 36.0 / 2.4));
        let p = place(&k, &s);
        let m = p.to_media;
        let near = |a: f64, b: f64| (a - b).abs() < 1e-3;
        assert!(near(m[0] * 320.0 + m[2], k.center[0]));
        // The picture's old place on the frame.
        assert!(near(m[0] * (320.0 + 160.0) + m[2], 200.0));
        // Back again.
        let back = transform_from_key(&k, &s);
        assert!((back.x - 0.25).abs() < 1e-5 && (back.y - 0.1).abs() < 1e-5);
        assert!((back.scale - 2.0).abs() < 1e-5);
        assert!((back.stretch - 1.0).abs() < 1e-5);
    }

    #[test]
    fn an_older_turn_and_mirror_come_back() {
        let s = sizes(true, true);
        for (flip_h, flip_v) in [(false, false), (true, false), (false, true), (true, true)] {
            let tf = Transform {
                x: 0.1,
                y: -0.2,
                scale: 1.5,
                rotation_deg: 30.0,
                flip_h,
                flip_v,
                ..Transform::default()
            };
            let k = key_from_transform(&tf, &s);
            let back = transform_from_key(&k, &s);
            assert!((back.rotation_deg - 30.0).abs() < 1e-3, "{back:?}");
            assert!((back.x - 0.1).abs() < 1e-5, "{back:?}");
            assert!((back.y + 0.2).abs() < 1e-5, "{back:?}");
            assert_eq!((back.flip_h, back.flip_v), (flip_h, flip_v));
        }
    }

    #[test]
    fn splitting_and_joining_keep_the_box_in_time() {
        let mut left = two_keys(0.0);
        let whole = left.clone();
        let cut = Tick(TICKS_PER_SECOND / 2);
        let right = left.split_at(cut, MEDIA, 1.0);
        assert_eq!(left.keys.last().unwrap().at, cut);
        assert_eq!(right.keys[0].at, Tick::ZERO);
        let at = |p: &PanCrop, t: Tick| p.at(t, MEDIA, 1.0).center;
        assert_eq!(at(&right, Tick::ZERO), at(&whole, cut));
        assert_eq!(
            at(&right, Tick::from_seconds(1) - cut),
            at(&whole, Tick::from_seconds(1))
        );
        let mut joined = left.clone();
        joined.joined(&right, cut, MEDIA);
        assert_eq!(
            at(&joined, Tick::from_seconds(2)),
            at(&whole, Tick::from_seconds(2))
        );
    }

    #[test]
    fn setting_the_box_makes_keyframes_as_needed() {
        let mut p = PanCrop::default();
        let k = PanCropKey::whole(MEDIA).moved([3.0, 0.0]);
        let new = (KeyCurve::Smooth, 0.25);
        // At the start, what it holds all along.
        p.set_at(Tick::ZERO, k, MEDIA, new);
        assert_eq!(p.keys.len(), 1);
        assert_eq!(p.keys[0].curve, KeyCurve::Smooth);
        assert_eq!(p.keys[0].smoothness, 0.25);
        assert_eq!(p.size, [64, 36]);
        // Elsewhere, the animation.
        let second = Tick::from_seconds(1);
        p.set_at(second, k.moved([4.0, 0.0]), MEDIA, new);
        assert_eq!(p.keys.len(), 2);
        assert_eq!(p.keys[1].at, second);
        // On a keyframe, that one; its curve stays.
        p.keys[1].curve = KeyCurve::Hold;
        p.set_at(second, k, MEDIA, new);
        assert_eq!(p.keys.len(), 2);
        assert_eq!(p.keys[1].curve, KeyCurve::Hold);
        p.remove_key(second);
        assert_eq!(p.keys.len(), 1);
    }

    #[test]
    fn an_empty_pan_crop_changes_nothing() {
        let p = PanCrop::default();
        assert!(p.is_default());
        assert_eq!(p.at(Tick::ZERO, MEDIA, 1.0), PanCropKey::whole(MEDIA));
        let holding = PanCrop::holding(PanCropKey::whole(MEDIA), MEDIA, true, true);
        assert!(holding.is_default());
        let moved = PanCrop::holding(
            PanCropKey::whole(MEDIA).moved([1.0, 0.0]),
            MEDIA,
            true,
            true,
        );
        assert!(!moved.is_default());
    }

    #[test]
    fn a_frame_on_another_scale_keeps_the_place() {
        let s = sizes(true, true);
        let p = place(&PanCropKey::whole(s.media), &s);
        let half = p.on_frame(s.frame, [320.0, 180.0]);
        assert_eq!(half.clip, [40.0, 0.0, 280.0, 180.0]);
        // The same media point.
        let (a, b) = (p.to_media, half.to_media);
        assert!(close(a[0] * 320.0 + a[2], b[0] * 160.0 + b[2]));
        // A picture of half the size: its pixels at the ends land on the
        // frame's.
        let (m, l) = p.in_picture([200.0, 150.0]);
        assert!(close(m[0], 199.0 / 479.0));
        assert!(close(m[0] * 80.5 + m[2], 0.5));
        assert!(close(m[0] * 559.5 + m[2], 199.5));
        assert_eq!(l, [0.0, 0.0, 200.0, 150.0]);
        assert_eq!(p.size_needed(), (480, 360));
    }

    #[test]
    fn a_frames_time_is_cut_down_to_the_boxs_unit() {
        // 5 / 30 s from an event starting at 3 / 30 s.
        let frame = |f: i64| Tick(f * TICKS_PER_SECOND / 30);
        assert_eq!(units_floor(frame(5)), 1_666_666);
        assert_eq!(units_near(frame(3)), 1_000_000);
    }
}
