//! The transitions VEGAS draws by moving the pictures — Barn Door, Push,
//! Slide, Swap, Squeeze, Split, Zoom and Spiral —, as its own: each frame
//! is a list of steps on the frame's pixels, rows counted from its top and
//! places in 16,384ths of a pixel ([`ONE`]): a picture, or part of it,
//! drawn into a rectangle ([`Blit`]: its edges covering pixels partly, read
//! between pixels as large or larger — the first and last pixels at the
//! rectangle's ends, the others at their left and top edges —, averaged
//! over each pixel's footprint when smaller and asked to), and VEGAS's
//! colored border along some edges ([`Border`]), in order, over a frame
//! that starts transparent. The card runs the same steps from a table
//! ([`Drawing::table`]).

use std::ffi::{CStr, c_int};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use super::Kind;
use super::pixel::{Frame, STEREO, framed};
use crate::accel::{TransitionKernel, TransitionPlan, TransitionSteps};
use crate::kit::{Buf, Spec, Vals};
use crate::ofx::{Fx, Props, for_rows};

/// A pixel, in the places' units.
pub(crate) const ONE: i32 = 16384;
/// Two pixels: the least feather a border gets.
const TWO: i32 = 2 * ONE;

/// A rectangle: its left, top, right and bottom, in 16,384ths of a pixel
/// (rows from the top).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rect {
    pub l: i32,
    pub t: i32,
    pub r: i32,
    pub b: i32,
}

impl Rect {
    pub(crate) fn of_pixels(w: usize, h: usize) -> Rect {
        Rect {
            l: 0,
            t: 0,
            r: w as i32 * ONE,
            b: h as i32 * ONE,
        }
    }

    fn w(&self) -> i32 {
        self.r - self.l
    }

    fn h(&self) -> i32 {
        self.b - self.t
    }

    /// The part inside `o` (an empty rectangle at 0 when none).
    fn and(&self, o: &Rect) -> Rect {
        let r = Rect {
            l: self.l.max(o.l),
            t: self.t.max(o.t),
            r: self.r.min(o.r),
            b: self.b.min(o.b),
        };
        if r.l >= r.r || r.t >= r.b {
            Rect {
                l: 0,
                t: 0,
                r: 0,
                b: 0,
            }
        } else {
            r
        }
    }
}

/// `v` over a pixel, rounded down.
fn floor_px(v: i32) -> i32 {
    v.div_euclid(ONE)
}

/// `v` over a pixel, rounded up.
fn ceil_px(v: i32) -> i32 {
    (v + ONE - 1).div_euclid(ONE)
}

/// `a × b` 16,384ths, truncated in double precision.
fn times(a: i32, b: i32) -> i32 {
    (f64::from(a) * f64::from(b) * (1.0 / f64::from(ONE))) as i32
}

/// One axis of a blit as each pixel reads it: the destination's whole
/// pixels `first..end`, the first's and the last's coverage (16,384ths;
/// the others whole), where the first, the second and the last pixel read
/// the source and the step between pixels, and the source's last pixel
/// (held past it); for an averaged blit the source's widths of the first,
/// a middle and the last pixel instead of positions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Axis {
    pub first: i32,
    pub end: i32,
    pub c0: i32,
    pub c_last: i32,
    pub s0: i32,
    pub s1: i32,
    pub step: i32,
    pub s_last: i32,
    pub limit: i32,
}

impl Axis {
    /// The axis of a destination span `d0..d1` reading the source from
    /// `s0` to `s1` (both clipped already), `area`: averaged with `scale`
    /// (the source's 16,384ths a destination pixel covers).
    fn of(d0: i32, d1: i32, s0: i32, s1: i32, area: bool, scale: i32) -> Axis {
        let first = floor_px(d0);
        let end = ceil_px(d1);
        let n = end - first;
        let c0 = if n < 2 {
            d1 - d0
        } else {
            (first + 1) * ONE - d0
        };
        let c_last = d1 - (end - 1) * ONE;
        let limit = ceil_px(s1) - 1;
        if area {
            // Each pixel's footprint: the first's, a middle one's and the
            // last's widths, one after another.
            Axis {
                first,
                end,
                c0,
                c_last,
                s0,
                s1: times(c0, scale),
                step: times(ONE, scale),
                s_last: times(c_last, scale),
                limit,
            }
        } else {
            // Corners aligned: the first pixel at the source's start, the
            // last a pixel before its end.
            let span = d1 - d0;
            let step = if span > 0x7fff {
                let s =
                    ((f64::from(s1 - s0 - ONE) * f64::from(ONE)) / f64::from(span - ONE)) as i32;
                if s < 1 { 0 } else { s }
            } else {
                0
            };
            let second = s0 + times(c0, step);
            let s_last = if n > 2 {
                second + (n - 3) * step + times(c_last, step)
            } else {
                second
            };
            Axis {
                first,
                end,
                c0,
                c_last,
                s0,
                s1: second,
                step,
                s_last,
                limit,
            }
        }
    }

    /// Pixel `k`'s coverage, 16,384ths (0 outside).
    fn cover(&self, k: i32) -> i32 {
        if k < self.first || k >= self.end {
            0
        } else if k == self.first {
            self.c0
        } else if k == self.end - 1 {
            self.c_last
        } else {
            ONE
        }
    }

    /// Where pixel `k` reads the source (a bilinear blit's).
    fn at(&self, k: i32) -> i32 {
        let n = self.end - self.first;
        if k == self.first {
            self.s0
        } else if k == self.end - 1 && n > 1 {
            self.s_last
        } else {
            self.s1 + (k - self.first - 1) * self.step
        }
    }

    /// Pixel `k`'s footprint in the source: its start and width (an
    /// averaged blit's).
    fn footprint(&self, k: i32) -> (i32, i32) {
        let n = self.end - self.first;
        if k == self.first {
            (self.s0, self.s1)
        } else {
            let start = self.s0 + self.s1 + (k - self.first - 1) * self.step;
            if k == self.end - 1 && n > 1 {
                (start, self.s_last)
            } else {
                (start, self.step)
            }
        }
    }
}

/// A picture (`second`: the one coming in) drawn into a rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Blit {
    pub second: bool,
    pub src: Rect,
    pub dst: Rect,
    /// Averaged over each pixel's footprint when smaller (VEGAS's area
    /// filter), else read between pixels.
    pub area: bool,
    /// How much of it shows (its coverage times this).
    pub alpha: f32,
    /// Laid over as VEGAS composites (single columns drawn, the alpha in
    /// the edges' 16,384ths), else as it scales a picture.
    pub composite: bool,
}

/// A rectangle of one color, premultiplied, laid over what is there (its
/// edges covering pixels partly).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Fill {
    pub dst: Rect,
    pub color: [f32; 4],
}

impl Fill {
    /// Where it covers a frame `f` big (`None`: nowhere).
    fn drawn(&self, f: &Frame) -> Option<Drawn> {
        Blit {
            second: false,
            src: Rect {
                l: 0,
                t: 0,
                r: ONE,
                b: ONE,
            },
            dst: self.dst,
            area: false,
            alpha: 1.0,
            composite: true,
        }
        .drawn(f)
    }
}

/// A blit as the pixels read it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Drawn {
    pub second: bool,
    pub area: bool,
    pub cols: Axis,
    pub rows: Axis,
    /// One over the footprint's area, in double precision, for the
    /// whole pixels of an averaged blit.
    pub inv_area: f64,
    pub alpha: f32,
    pub composite: bool,
}

impl Blit {
    /// The blit on a frame `f` big from pictures as big, as VEGAS prepares
    /// it: the source and the destination clipped to their pictures and to
    /// each other (`None`: nothing to draw).
    pub(crate) fn drawn(&self, f: &Frame) -> Option<Drawn> {
        let frame = Rect::of_pixels(f.w, f.h);
        let s = self.src.and(&frame);
        if s.w() < ONE || s.h() < ONE {
            return None;
        }
        let d0 = self.dst;
        if d0.w() < 0x400 || d0.h() < 0x400 {
            return None;
        }
        let scale_x = ((f64::from(s.w()) * f64::from(ONE)) / f64::from(d0.w())) as i32;
        let scale_y = ((f64::from(s.h()) * f64::from(ONE)) / f64::from(d0.h())) as i32;
        let d = d0.and(&frame);
        if d.w() < 0x400 || d.h() < 0x400 || scale_x <= 0 || scale_y <= 0 {
            return None;
        }
        // The source's place in the destination, the part seen, back in
        // the source.
        let mapped = Rect {
            l: d0.l,
            t: d0.t,
            r: d0.l + ((f64::from(s.w()) * f64::from(ONE)) / f64::from(scale_x)) as i32,
            b: d0.t + ((f64::from(s.h()) * f64::from(ONE)) / f64::from(scale_y)) as i32,
        };
        let v = mapped.and(&d);
        let back = Rect {
            l: s.l + times(v.l - d0.l, scale_x),
            r: s.l + times(v.r - d0.l, scale_x),
            t: s.t + times(v.t - d0.t, scale_y),
            b: s.t + times(v.b - d0.t, scale_y),
        };
        let src = back.and(&s);
        let area = self.area && scale_x > ONE && scale_y > ONE;
        Some(Drawn {
            second: self.second,
            area,
            cols: Axis::of(d.l, d.r, src.l, src.r, area, scale_x),
            rows: Axis::of(d.t, d.b, src.t, src.b, area, scale_y),
            inv_area: 1.0
                / (f64::from(scale_x)
                    * (1.0 / f64::from(ONE))
                    * f64::from(scale_y)
                    * (1.0 / f64::from(ONE))),
            alpha: self.alpha,
            composite: self.composite,
        })
    }
}

impl Drawn {
    /// Pixel `x`, `y`'s share of the blit (0 where it does not draw) and
    /// what it draws there, from `pic` (pixel `i`, `j`, rows from the top,
    /// the picture's edges held).
    pub(crate) fn at(
        &self,
        x: i32,
        y: i32,
        pic: &dyn Fn(i32, i32) -> [f32; 4],
    ) -> Option<(f32, [f32; 4], bool)> {
        let (cx, cy) = (self.cols.cover(x), self.rows.cover(y));
        if cx == 0 || cy == 0 {
            return None;
        }
        // A single column is never scaled (VEGAS's rows stop a pixel
        // early); composites draw it.
        if !self.composite && self.cols.end - self.cols.first < 2 {
            return None;
        }
        let k = 1.0 / ONE as f32;
        if self.area {
            let (sx, wx) = self.cols.footprint(x);
            let (sy, wy) = self.rows.footprint(y);
            let sum = footprint_sum(pic, sx, wx, sy, wy);
            let whole = cx == ONE && cy == ONE;
            if whole {
                return Some((1.0, sum.map(|v| (v * self.inv_area) as f32), true));
            }
            let part = (i64::from(wx) * i64::from(wy)) >> 14;
            if part <= 0 {
                return None;
            }
            let share = times(cx, cy) as f32 * k;
            let inv = 1.0 / (part as f64 * (1.0 / f64::from(ONE)));
            return Some((share, sum.map(|v| (inv * v) as f32), false));
        }
        let px = self.cols.at(x);
        let py = self.rows.at(y);
        let (ix, iy) = (px.div_euclid(ONE), py.div_euclid(ONE));
        let fx = px.rem_euclid(ONE) as f32 * k;
        let fy = py.rem_euclid(ONE) as f32 * k;
        let nx = if ix != self.cols.limit { ix + 1 } else { ix };
        let ny = if iy != self.rows.limit { iy + 1 } else { iy };
        let (a, b, c, d) = (pic(ix, iy), pic(nx, iy), pic(ix, ny), pic(nx, ny));
        let (g, h) = (1.0 - fy, fy);
        let w00 = (1.0 - fx) * g;
        let w10 = g * fx;
        let w01 = (1.0 - fx) * h;
        let w11 = h * fx;
        let v: [f32; 4] =
            std::array::from_fn(|i| w00 * a[i] + w10 * b[i] + w01 * c[i] + w11 * d[i]);
        let share = if !self.composite {
            (cx as f32 * k) * (cy as f32 * k)
        } else if cx == ONE && cy == ONE {
            self.alpha
        } else {
            // The edges' coverage times the alpha in 16,384ths, the row's
            // first.
            times(times(cy, (self.alpha * 16384.0) as i32), cx) as f32 * k
        };
        Some((share, v, share == 1.0))
    }

    /// A fill's share of pixel `x`, `y` (`None`: none).
    fn fill_share(&self, x: i32, y: i32) -> Option<f32> {
        let (cx, cy) = (self.cols.cover(x), self.rows.cover(y));
        if cx == 0 || cy == 0 {
            return None;
        }
        Some(if cx == ONE && cy == ONE {
            1.0
        } else {
            times(cy, cx) as f32 * (1.0 / ONE as f32)
        })
    }
}

/// The weighted sum of a source footprint `sx`..`sx + wx` × `sy`..`sy + wy`
/// (16,384ths), its edge pixels weighed by how much of them it covers.
fn footprint_sum(
    pic: &dyn Fn(i32, i32) -> [f32; 4],
    sx: i32,
    wx: i32,
    sy: i32,
    wy: i32,
) -> [f64; 4] {
    let span = |s: i32, w: i32| {
        let i0 = s.div_euclid(ONE);
        let i1 = ceil_px(s + w);
        let n = i1 - i0;
        let k = 1.0 / ONE as f32;
        let first = if n > 1 { (i0 + 1) * ONE - s } else { w } as f32 * k;
        let last = (w + s - (i1 - 1) * ONE) as f32 * k;
        (i0, i1, first, last)
    };
    let (x0, x1, fx0, fx1) = span(sx, wx);
    let (y0, y1, fy0, fy1) = span(sy, wy);
    let mut acc = [0.0f64; 4];
    let mut add = |w: f32, c: [f32; 4]| {
        for (a, v) in acc.iter_mut().zip(c) {
            *a += f64::from(w * v);
        }
    };
    for j in y0..y1 {
        let wy = if j == y0 {
            fy0
        } else if j == y1 - 1 {
            fy1
        } else {
            1.0
        };
        for i in x0..x1 {
            let wx = if i == x0 {
                fx0
            } else if i == x1 - 1 {
                fx1
            } else {
                1.0
            };
            let w = if wx == 1.0 && wy == 1.0 { 1.0 } else { wx * wy };
            add(w, pic(i, j));
        }
    }
    acc
}

/// One edge of VEGAS's border: a segment from `a` to `e` along x (or along
/// y when `down`) at `at` across, the band on the side `after` says (after
/// it: right or below; else before it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Edge {
    pub a: i32,
    pub e: i32,
    pub at: i32,
    pub down: bool,
    pub after: bool,
}

impl Edge {
    /// VEGAS's flags: bit 0 down, bit 1 after.
    pub(crate) fn flags(f: u32, a: i32, e: i32, at: i32) -> Edge {
        Edge {
            a,
            e,
            at,
            down: f & 1 != 0,
            after: f & 2 != 0,
        }
    }
}

/// VEGAS's border along `edges`: a band `size` wide (16,384ths) in
/// `color`, softened over `feather` both outside it and inside the edge,
/// `over` farther away.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Border {
    pub edges: Vec<Edge>,
    pub size: i32,
    pub feather: i32,
    pub over: i32,
    pub color: [f32; 3],
}

impl Border {
    /// The least feather VEGAS gives a border, the size giving way.
    fn settled(&self) -> (i32, i32) {
        let (mut b, mut f) = (self.size, self.feather);
        if (b != 0 || f != 0) && TWO - f > 0 {
            b = (b - (TWO - f)).max(0);
            f = TWO;
        }
        (b, f)
    }

    /// The border's share of pixel `x`, `y` (`None`: none).
    pub(crate) fn at(&self, x: i32, y: i32) -> Option<f32> {
        let (b, f) = self.settled();
        let most = b + f;
        if most == 0 || self.edges.is_empty() {
            return None;
        }
        let (px, py) = (x * ONE + ONE / 2, y * ONE + ONE / 2);
        let mut near = most;
        let mut inside = f;
        let mut hit = false;
        for s in &self.edges {
            let (along, across) = if s.down { (py, px) } else { (px, py) };
            if s.a <= along && along <= s.e {
                let d = s.at - across;
                let d = if s.after { -d } else { d };
                if d < 0 {
                    if -d < inside {
                        inside = -d;
                        hit = true;
                    }
                } else if d < near {
                    near = d;
                    hit = true;
                }
            }
        }
        if !hit {
            // The ends of the edges along x, rounded.
            for s in self.edges.iter().filter(|s| !s.down) {
                let mut beyond = near;
                if s.a - px >= 0 {
                    beyond = s.a - px;
                }
                let mut past = near;
                if px - s.e >= 0 {
                    past = px - s.e;
                }
                let along = past.min(beyond);
                if along >= 0 && along < near {
                    let side = if s.after { py - s.at } else { s.at - py };
                    if side > 0 && side < near {
                        let d = ((f64::from(along) * f64::from(along)
                            + f64::from(side) * f64::from(side))
                        .sqrt()
                            + 0.5) as i32;
                        if d < near {
                            near = d;
                            hit = true;
                        }
                    }
                }
            }
        }
        if !hit {
            return None;
        }
        if self.over != 0 {
            near = (near + self.over).min(most);
            inside = (inside + self.over).min(f);
        }
        Some(if near < b {
            1.0
        } else if f != 0 {
            let t = (near - b).min(inside) as f32 / f as f32;
            1.0 - (t * 3.0 * t - (t + t) * t * t)
        } else {
            0.0
        })
    }
}

/// A step of a frame's drawing.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Step {
    Blit(Blit),
    Border(Border),
    Fill(Fill),
}

/// A frame's drawing: its steps in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Drawing {
    pub steps: Vec<Step>,
}

impl Drawing {
    /// Pixel `x`, `y` (rows from the top) of the frame `f` drawn from the
    /// pictures (`pic(second, i, j)`, rows from the top).
    pub(crate) fn pixel(
        &self,
        drawn: &[Option<Drawn>],
        x: i32,
        y: i32,
        pic: &dyn Fn(bool, i32, i32) -> [f32; 4],
    ) -> [f32; 4] {
        let mut out = [0.0f32; 4];
        for (step, d) in self.steps.iter().zip(drawn) {
            match step {
                Step::Blit(_) => {
                    let Some(d) = d else { continue };
                    let read = |i: i32, j: i32| pic(d.second, i, j);
                    if let Some((share, v, whole)) = d.at(x, y, &read) {
                        out = if whole {
                            v
                        } else {
                            std::array::from_fn(|i| (v[i] - out[i]) * share + out[i])
                        };
                    }
                }
                Step::Border(b) => {
                    if let Some(k) = b.at(x, y) {
                        let c = [b.color[0], b.color[1], b.color[2], 1.0];
                        out = std::array::from_fn(|i| (c[i] - out[i]) * k + out[i]);
                    }
                }
                Step::Fill(fl) => {
                    if let Some(k) = d.as_ref().and_then(|d| d.fill_share(x, y)) {
                        let c = fl.color;
                        out = std::array::from_fn(|i| c[i] * k + out[i] * (1.0 - c[3] * k));
                    }
                }
            }
        }
        out
    }

    /// Each blit prepared for the frame `f` (a border's place stays empty).
    pub(crate) fn prepare(&self, f: &Frame) -> Vec<Option<Drawn>> {
        self.steps
            .iter()
            .map(|s| match s {
                Step::Blit(b) => b.drawn(f),
                Step::Border(_) => None,
                Step::Fill(fl) => fl.drawn(f),
            })
            .collect()
    }
}

// The transitions' drawings, as VEGAS lays them out each frame.

/// A border's settings: its size, feather and color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Edging {
    pub size: f32,
    pub feather: f32,
    pub color: [f32; 3],
}

/// Which transition, with its own settings (centers as VEGAS keeps them:
/// x from the left, y from the top).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Move {
    BarnDoor {
        vertical: bool,
        out: bool,
    },
    Push {
        dir: i32,
        push_off: bool,
    },
    /// Slide (k 1) and Spiral (its angle turning, k its zoom).
    Slide {
        angle: f64,
        out: bool,
        k: f64,
    },
    Swap {
        dir: i32,
    },
    Squeeze {
        start: i32,
        squeeze: bool,
    },
    Split {
        center: [f32; 2],
        mode: i32,
        out: bool,
    },
    Zoom {
        center: [f32; 2],
        out: bool,
    },
}

/// `ext` 16,384ths times `v`, rounded as VEGAS rounds its sizes.
fn part(ext: i32, v: f32) -> i32 {
    (f64::from(ext as f32 * v) + 0.5) as i32
}

/// The least feather VEGAS's moves give a border, the size giving way.
fn forced(b: i32, f: i32) -> (i32, i32) {
    if (b != 0 || f != 0) && TWO - f > 0 {
        ((b - (TWO - f)).max(0), TWO)
    } else {
        (b, f)
    }
}

/// `r` grown out to whole pixels.
fn snapped(r: Rect) -> Rect {
    Rect {
        l: r.l.div_euclid(ONE) * ONE,
        t: r.t.div_euclid(ONE) * ONE,
        r: ceil_px(r.r) * ONE,
        b: ceil_px(r.b) * ONE,
    }
}

fn blit(second: bool, src: Rect, dst: Rect) -> Step {
    Step::Blit(Blit {
        second,
        src,
        dst,
        area: false,
        alpha: 1.0,
        composite: false,
    })
}

/// An edge as the moves give it: from, to, at, VEGAS's flags.
type Along = (i32, i32, i32, u32);

/// A border along `edges` (from, to, at, VEGAS's flags: bit 0 down, bit 1
/// after).
fn border(e: &Edging, edges: &[Along], size: i32, feather: i32, over: i32) -> Step {
    Step::Border(Border {
        edges: edges
            .iter()
            .map(|&(a, b, at, fl)| Edge::flags(fl, a, b, at))
            .collect(),
        size,
        feather,
        over,
        color: e.color,
    })
}

/// A rectangle moved by `dx`, `dy`.
fn moved(r: Rect, dx: i32, dy: i32) -> Rect {
    Rect {
        l: r.l + dx,
        t: r.t + dy,
        r: r.r + dx,
        b: r.b + dy,
    }
}

impl Move {
    /// The drawing of a frame `size` big, `p` of the way.
    pub(crate) fn drawing(&self, e: &Edging, size: &Frame, p: f64) -> Drawing {
        let (w, h) = (size.w as i32 * ONE, size.h as i32 * ONE);
        let frame = Rect::of_pixels(size.w, size.h);
        let p = p.clamp(0.0, 1.0);
        let mut steps = Vec::new();
        match *self {
            Move::BarnDoor { vertical, out } => {
                let (p, door) = if out { (1.0 - p, false) } else { (p, true) };
                let half = (if vertical { h } else { w }) >> 1;
                let (mut b, f) = forced(part(half, e.size), part(half, e.feather));
                let d = (f64::from(b + f * 2 + half) * p + 0.5) as i32 - f;
                let open = (d - b).max(0);
                let over = if open > half { open - half } else { 0 };
                let o = open.min(half);
                let mut reach = d - ONE;
                if half < reach {
                    b += half - reach;
                    reach = half;
                }
                steps.push(blit(!door, frame, frame));
                let edges: Vec<Along> = if vertical {
                    let (s1, d1) = (
                        Rect {
                            t: half - o,
                            b: half,
                            ..frame
                        },
                        Rect {
                            t: 0,
                            b: o,
                            ..frame
                        },
                    );
                    let (s2, d2) = (
                        Rect {
                            t: half,
                            b: half + o,
                            ..frame
                        },
                        Rect {
                            t: h - o,
                            b: h,
                            ..frame
                        },
                    );
                    steps.push(blit(door, s1, d1));
                    steps.push(blit(door, s2, d2));
                    vec![(0, w, reach, 0), (0, w, h - reach, 2)]
                } else {
                    let (s1, d1) = (
                        Rect {
                            l: half - o,
                            r: half,
                            ..frame
                        },
                        Rect {
                            l: 0,
                            r: o,
                            ..frame
                        },
                    );
                    let (s2, d2) = (
                        Rect {
                            l: half,
                            r: half + o,
                            ..frame
                        },
                        Rect {
                            l: w - o,
                            r: w,
                            ..frame
                        },
                    );
                    steps.push(blit(door, s1, d1));
                    steps.push(blit(door, s2, d2));
                    vec![(0, h, reach, 1), (0, h, w - reach, 3)]
                };
                steps.push(border(e, &edges, if over == 0 { b } else { 0 }, f, over));
            }
            Move::Push { dir, push_off } => {
                let ext = if dir > 1 { w } else { h };
                let (b, f) = forced(part(ext, e.size), part(ext, e.feather));
                let fp = if f > 0x3fff { f } else { 0 };
                let d = (f64::from(b + fp * 2 + ext) * p + 0.5) as i32 - fp;
                let s = d.clamp(0, ext);
                let x = (d - b).clamp(0, ext) - ext;
                let (a_rect, b_rect, edge) = match dir {
                    0 => (
                        moved(frame, 0, -s),
                        moved(frame, 0, -x),
                        (0, w, ext - (d - b), 0),
                    ),
                    1 => (moved(frame, 0, s), moved(frame, 0, x), (0, w, d - b, 2)),
                    2 => (
                        moved(frame, -s, 0),
                        moved(frame, -x, 0),
                        (0, h, ext - (d - b), 1),
                    ),
                    _ => (moved(frame, s, 0), moved(frame, x, 0), (0, h, d - b, 3)),
                };
                if e.size == 0.0 && push_off {
                    steps.push(blit(false, frame, snapped(a_rect)));
                }
                steps.push(blit(false, frame, if push_off { a_rect } else { frame }));
                steps.push(blit(true, frame, b_rect));
                steps.push(border(e, &[edge], b, fp, 0));
            }
            Move::Swap { dir } => {
                let ext = if dir > 1 { w } else { h };
                let (b, f) = forced(part(ext, e.size), part(ext, e.feather));
                let fp = if f > 0x3fff { f } else { 0 };
                let first_half = p < 0.5;
                let bb = if p == 0.0 { 0 } else { b };
                let q = if first_half { p } else { 1.0 - p };
                let d = (q * f64::from(b + fp * 2 + ext) + 0.5) as i32 - fp;
                let s = d.clamp(0, ext);
                let mask = !(ONE - 1);
                let near = if first_half { ext - d } else { d };
                let far = if first_half { d } else { ext - d };
                // The first picture's place and the second's, whether their
                // edges share a pixel or leave a gap, the border's side and
                // where it runs, and whether the edges run down.
                let (a, bx, same, gap, hi, along, down) = match dir {
                    0 => {
                        let (a, bx) = (moved(frame, 0, -s), moved(frame, 0, s));
                        let same = (bx.t ^ a.b) & mask == 0;
                        let gap = (a.b & mask) + bb < ceil_px(bx.t) * ONE;
                        (a, bx, same, gap, first_half, near, false)
                    }
                    1 => {
                        let (a, bx) = (moved(frame, 0, s), moved(frame, 0, -s));
                        let same = (bx.b ^ a.t) & mask == 0;
                        let gap = (bx.b & mask) + bb < ceil_px(a.t) * ONE;
                        (a, bx, same, gap, !first_half, far, false)
                    }
                    2 => {
                        let (a, bx) = (moved(frame, -s, 0), moved(frame, s, 0));
                        let same = (bx.l ^ a.r) & mask == 0;
                        let gap = (a.r & mask) + bb < ceil_px(bx.l) * ONE;
                        (a, bx, same, gap, first_half, near, true)
                    }
                    _ => {
                        let (a, bx) = (moved(frame, s, 0), moved(frame, -s, 0));
                        let same = (bx.r ^ a.l) & mask == 0;
                        let gap = (bx.r & mask) + bb < ceil_px(a.l) * ONE;
                        (a, bx, same, gap, !first_half, far, true)
                    }
                };
                let span = if down { h } else { w };
                let flags = (u32::from(hi) * 2) | u32::from(down);
                if bb == 0 {
                    if same {
                        steps.push(blit(false, frame, snapped(a)));
                    }
                } else if gap {
                    let at = i32::from(hi) * TWO - ONE + along;
                    steps.push(border(e, &[(0, span, at, flags)], bb + TWO, 0, 0));
                }
                if !first_half {
                    steps.push(blit(false, frame, a));
                }
                steps.push(blit(true, frame, bx));
                if first_half {
                    steps.push(blit(false, frame, a));
                }
                steps.push(border(e, &[(0, span, along, flags)], bb, fp, 0));
            }
            Move::Slide { angle, out, k } => {
                let (p, moving) = if out { (1.0 - p, false) } else { (p, true) };
                steps.push(blit(!moving, frame, frame));
                let m = w.min(h);
                let (b, f) = forced(part(m, e.size), part(m, e.feather));
                let (hx, hy) = (f64::from(-(w / 2)), f64::from(-(h / 2)));
                let g = f64::from(f) / f64::from(w.max(h));
                let q = (g + 1.0) * (1.0 - p) - g;
                let s = (k - 1.0) * q + 1.0;
                let a = angle * 0.017_453_292_519_943;
                let fac = q / a.sin().abs().max(a.cos().abs());
                let (wf, hf, bf, ff) = (f64::from(w), f64::from(h), f64::from(b), f64::from(f));
                let rect = if fac < 0.0 {
                    let grow = ((-q / g) * ff) as i32;
                    Rect {
                        l: -grow,
                        t: -grow,
                        r: w + grow,
                        b: h + grow,
                    }
                } else {
                    let ox = ((1.0 - s) * wf * 0.5 + 0.5) as i32
                        + (a.cos() * (hx - (wf * s * 0.5 + bf + ff)) * fac + 0.5) as i32;
                    let oy = ((1.0 - s) * hf * 0.5 + 0.5) as i32
                        + (a.sin() * (hy - (hf * s * 0.5 + bf + ff)) * fac + 0.5) as i32;
                    moved(
                        Rect {
                            r: (wf * s) as i32,
                            b: (hf * s) as i32,
                            ..frame
                        },
                        ox,
                        oy,
                    )
                };
                steps.push(blit(moving, frame, rect));
                if f != 0 || s < 1.0 {
                    let edges = [
                        (rect.t, rect.b, rect.r, 3),
                        (rect.t, rect.b, rect.l, 1),
                        (rect.l, rect.r, rect.t, 0),
                        (rect.l, rect.r, rect.b, 2),
                    ];
                    steps.push(border(e, &edges, b, f, 0));
                }
            }
            Move::Zoom { center, out } => {
                let (p, grows) = if out { (1.0 - p, false) } else { (p, true) };
                let l = w.min(h);
                let mut f = part(l, e.feather);
                if (part(l, e.size) != 0 || f != 0) && TWO - f > 0 {
                    f = TWO;
                }
                let d = (f64::from(l + f * 2) * p + 0.5) as i32;
                let k = if l != 0 && d < l {
                    d as f32 / l as f32
                } else {
                    1.0
                };
                let (cx, cy) = (part(w, center[0]), part(h, center[1]));
                let at = |side: i32, c: i32| (f64::from((side - c) as f32 * k) + 0.5) as i32 + c;
                let rect = Rect {
                    l: at(0, cx),
                    t: at(0, cy),
                    r: at(w, cx),
                    b: at(h, cy),
                };
                steps.push(blit(!grows, frame, frame));
                steps.push(Step::Blit(Blit {
                    second: grows,
                    src: frame,
                    dst: rect,
                    area: true,
                    alpha: 1.0,
                    composite: false,
                }));
                let o = if l <= d {
                    let g = (d - l) >> 1;
                    Rect {
                        l: rect.l - g,
                        t: rect.t - g,
                        r: rect.r + g,
                        b: rect.b + g,
                    }
                } else {
                    rect
                };
                let edges = [
                    (o.l, o.r, o.t, 0),
                    (o.l, o.r, o.b, 2),
                    (o.t, o.b, o.l, 1),
                    (o.t, o.b, o.r, 3),
                ];
                let bs = (f64::from(l as f32 * e.size * k) + 0.5) as i32;
                let fs = (f64::from(l as f32 * e.feather * k) + 0.5) as i32;
                steps.push(border(e, &edges, bs, fs, 0));
            }
            Move::Squeeze { start, squeeze } => {
                let ext = match start {
                    0 | 1 => h,
                    2 | 3 => w,
                    4 => h >> 1,
                    _ => w >> 1,
                };
                let (mut b, f) = forced(part(ext, e.size), part(ext, e.feather));
                let fp = if f > 0x3fff { f } else { 0 };
                let d = (f64::from(b + fp * 2 + ext) * p + 0.5) as i32;
                let ea = (d - fp).clamp(0, ext);
                let eb = (d - fp - b).clamp(0, ext);
                let m = (d - fp - b).max(0);
                let reach = d - fp - b;
                // The first picture's parts (where each squeezes from and
                // into), the second's strip and the edges.
                let (pieces, strip, edges): (Vec<(Rect, Rect)>, Rect, Vec<Along>) = match start {
                    0 => (
                        vec![(frame, Rect { t: ea, ..frame })],
                        Rect { b: eb, ..frame },
                        vec![(0, w, reach, 2)],
                    ),
                    1 => (
                        vec![(
                            frame,
                            Rect {
                                b: ext - ea,
                                ..frame
                            },
                        )],
                        Rect {
                            t: ext - eb,
                            ..frame
                        },
                        vec![(0, w, ext - reach, 0)],
                    ),
                    2 => (
                        vec![(frame, Rect { l: ea, ..frame })],
                        Rect { r: eb, ..frame },
                        vec![(0, h, reach, 3)],
                    ),
                    3 => (
                        vec![(
                            frame,
                            Rect {
                                r: ext - ea,
                                ..frame
                            },
                        )],
                        Rect {
                            l: ext - eb,
                            ..frame
                        },
                        vec![(0, h, ext - reach, 1)],
                    ),
                    4 => (
                        vec![
                            (
                                Rect { b: ext, ..frame },
                                Rect {
                                    b: ext - ea,
                                    ..frame
                                },
                            ),
                            (
                                Rect { t: ext, ..frame },
                                Rect {
                                    t: ea + ext,
                                    ..frame
                                },
                            ),
                        ],
                        Rect {
                            t: ext - eb,
                            b: eb + ext,
                            ..frame
                        },
                        vec![(0, w, ext - m, 0), (0, w, m + ext, 2)],
                    ),
                    _ => (
                        vec![
                            (
                                Rect { r: ext, ..frame },
                                Rect {
                                    r: ext - ea,
                                    ..frame
                                },
                            ),
                            (
                                Rect { l: ext, ..frame },
                                Rect {
                                    l: ea + ext,
                                    ..frame
                                },
                            ),
                        ],
                        Rect {
                            l: ext - eb,
                            r: eb + ext,
                            ..frame
                        },
                        vec![(0, h, ext - m, 1), (0, h, m + ext, 3)],
                    ),
                };
                if squeeze {
                    for (src, dst) in &pieces {
                        steps.push(blit(false, *src, snapped(*dst)));
                    }
                    for (src, dst) in &pieces {
                        steps.push(blit(false, *src, *dst));
                    }
                } else {
                    steps.push(blit(false, frame, frame));
                }
                steps.push(blit(true, frame, strip));
                if pieces.len() > 1 {
                    let x = (fp - d) + b;
                    if x > 0 {
                        b -= x;
                    }
                }
                let (bs, over) = if fp - d >= 0 { (0, fp - d) } else { (b, 0) };
                steps.push(border(e, &edges, bs, fp, over));
            }
            Move::Split { center, mode, out } => {
                // In: the second picture's parts close over the first.
                let (p, parts) = if out { (p, false) } else { (1.0 - p, true) };
                let m = w.min(h);
                let b = (f64::from(m as f32 * e.size * 0.5) + 0.5) as i32;
                let mut f = (f64::from(m as f32 * e.feather * 0.5) + 0.5) as i32;
                let g = f64::from(f) / f64::from(w.max(h));
                let q = (g + 1.0) * p - g;
                let (ox, oy, bw) = if q < 0.0 {
                    f = ((1.0 - -q / g) * f64::from(f)) as i32;
                    (0, 0, 0)
                } else {
                    (
                        (f64::from(w) * q) as i32,
                        (f64::from(h) * q) as i32,
                        (f64::from(f + b) * q) as i32,
                    )
                };
                let (ox, oy, bw) = (ox.max(0), oy.max(0), bw.max(0));
                // The border no wider than the opening.
                let tx = ((ox / 2 + bw) * 2 - 1).max(0);
                let ty = (oy / 2 + bw) * 2 - 1;
                let least = if ty.max(0) <= tx { ty.max(0) } else { tx };
                let bs = if least < b { least } else { b };
                if -g < q {
                    steps.push(blit(!parts, frame, frame));
                    let sx = (w as f32 * center[0]) as i32;
                    let sy = (h as f32 * center[1]) as i32;
                    let left = (-(ox as f32) * center[0] - bw as f32) as i32;
                    let up = (-(oy as f32) * center[1] - bw as f32) as i32;
                    let right = ((1.0 - center[0]) * ox as f32 + bw as f32) as i32;
                    let down = ((1.0 - center[1]) * oy as f32 + bw as f32) as i32;
                    // Each part, its move, and the sides facing the cross.
                    let quads = [
                        (
                            Rect {
                                r: sx,
                                b: sy,
                                ..frame
                            },
                            left,
                            up,
                            3u32,
                            2u32,
                        ),
                        (
                            Rect {
                                l: sx,
                                b: sy,
                                ..frame
                            },
                            right,
                            up,
                            1,
                            2,
                        ),
                        (
                            Rect {
                                r: sx,
                                t: sy,
                                ..frame
                            },
                            left,
                            down,
                            3,
                            0,
                        ),
                        (
                            Rect {
                                l: sx,
                                t: sy,
                                ..frame
                            },
                            right,
                            down,
                            1,
                            0,
                        ),
                    ];
                    for (src, dx, dy, side_x, side_y) in quads {
                        // The part's side toward the cross moved, the others kept.
                        let cut = Rect {
                            l: if side_x == 1 { src.l + dx } else { src.l },
                            r: if side_x == 3 { src.r + dx } else { src.r },
                            t: if side_y == 0 { src.t + dy } else { src.t },
                            b: if side_y == 2 { src.b + dy } else { src.b },
                        };
                        let (s, d) = match mode {
                            0 => (src, moved(src, dx, dy)),
                            1 => (cut, cut),
                            _ => (src, cut),
                        };
                        steps.push(blit(parts, s, d));
                        let x_edge = if side_x == 3 { d.r } else { d.l };
                        let y_edge = if side_y == 2 { d.b } else { d.t };
                        let edges = [(d.t, d.b, x_edge, side_x), (d.l, d.r, y_edge, side_y)];
                        steps.push(border(e, &edges, bs, f, 0));
                    }
                } else {
                    steps.push(blit(parts, frame, frame));
                }
            }
        }
        Drawing { steps }
    }
}

// The card's table.

/// How many rows of the card's table a step takes.
const RECORD: usize = 6;

impl Drawing {
    /// The steps on a frame `f` big as the card's `tr_moves` reads them: a
    /// count, then six rows a step. A blit's: 0, its flags (1 the second
    /// picture, 2 averaged, 4 composited), one over its footprint's area
    /// and its alpha (floats' bits), its columns' and its rows' axes, their
    /// sources' last pixels. A border's: 1, its edges' count, its size and
    /// feather as it draws them, how much farther, its color (floats'
    /// bits), its edges (their flags 1 down, 2 after). A fill's: 2, its
    /// columns' and rows' coverage, its color (floats' bits). Blits and
    /// fills that draw nothing are left out.
    pub(crate) fn table(&self, f: &Frame) -> Vec<[i32; 4]> {
        let mut t = vec![[0; 4]];
        for (step, d) in self.steps.iter().zip(self.prepare(f)) {
            let mut rec = [[0i32; 4]; RECORD];
            match step {
                Step::Blit(_) => {
                    let Some(d) = d else { continue };
                    let flags = i32::from(d.second)
                        | (i32::from(d.area) << 1)
                        | (i32::from(d.composite) << 2);
                    rec[0] = [
                        0,
                        flags,
                        (d.inv_area as f32).to_bits() as i32,
                        d.alpha.to_bits() as i32,
                    ];
                    let (c, r) = (d.cols, d.rows);
                    rec[1] = [c.first, c.end, c.c0, c.c_last];
                    rec[2] = [c.s0, c.s1, c.step, c.s_last];
                    rec[3] = [r.first, r.end, r.c0, r.c_last];
                    rec[4] = [r.s0, r.s1, r.step, r.s_last];
                    rec[5] = [c.limit, r.limit, 0, 0];
                }
                Step::Border(b) => {
                    let (size, feather) = b.settled();
                    let edges = &b.edges[..b.edges.len().min(RECORD - 2)];
                    rec[0] = [1, edges.len() as i32, size, feather];
                    let [red, green, blue] = b.color.map(|c| c.to_bits() as i32);
                    rec[1] = [b.over, red, green, blue];
                    for (row, e) in rec[2..].iter_mut().zip(edges) {
                        *row = [
                            e.a,
                            e.e,
                            e.at,
                            i32::from(e.down) | (i32::from(e.after) << 1),
                        ];
                    }
                }
                Step::Fill(fl) => {
                    let Some(d) = d else { continue };
                    let (c, r) = (d.cols, d.rows);
                    rec[0] = [2, 0, 0, 0];
                    rec[1] = [c.first, c.end, c.c0, c.c_last];
                    rec[2] = [r.first, r.end, r.c0, r.c_last];
                    rec[3] = fl.color.map(|v| v.to_bits() as i32);
                }
            }
            t.extend_from_slice(&rec);
            t[0][0] += 1;
        }
        t
    }
}

// Panels, as VEGAS's.

const IN_OUT: &[&CStr] = &[c"In", c"Out"];
const DIRECTIONS: &[&CStr] = &[c"Up", c"Down", c"Left", c"Right"];
const BORDER: Spec = Spec::Group(c"border", c"Border");
const SIZE: Spec = Spec::Number(
    c"borderSize",
    c"Size",
    0.0,
    0.0,
    1.0,
    c"How wide the border along the moving edges is, a part of the frame.",
);
const FEATHER: Spec = Spec::Number(
    c"borderFeather",
    c"Feather",
    0.0,
    0.0,
    1.0,
    c"How far the border fades out, a part of the frame (two pixels at least).",
);
const COLOR: Spec = Spec::Rgb(
    c"borderColor",
    c"Color",
    [0.0, 0.0, 0.0],
    c"The border's color.",
);

pub(super) const BARN_DOOR: &[Spec] = &[
    Spec::Choice(
        c"orientation",
        c"Orientation",
        &[c"Horizontal", c"Vertical"],
        0,
        c"Horizontal: the doors move across the frame. Vertical: up and down.",
    ),
    Spec::Choice(
        c"direction",
        c"Direction",
        IN_OUT,
        0,
        c"In: the doors close from the edges with the next picture. Out: the first one opens from the middle.",
    ),
    BORDER,
    SIZE,
    FEATHER,
    COLOR,
    Spec::GroupEnd,
];

pub(super) const PUSH: &[Spec] = &[
    Spec::Choice(
        c"direction",
        c"Direction",
        DIRECTIONS,
        0,
        c"Which way the pictures move: the next one comes in from the other side.",
    ),
    BORDER,
    SIZE,
    FEATHER,
    COLOR,
    Spec::GroupEnd,
    Spec::Toggle(
        c"pushOff",
        c"Push off previous image",
        false,
        c"The first picture is pushed out of the frame; else the next one slides in over it.",
    ),
];

pub(super) const SLIDE: &[Spec] = &[
    Spec::Number(
        c"angle",
        c"Angle",
        45.0,
        0.0,
        360.0,
        c"The side the picture comes in from (In) or goes out to (Out): 0 the left, 90 the top, 180 the right, 270 the bottom.",
    ),
    Spec::Choice(
        c"direction",
        c"Direction",
        IN_OUT,
        0,
        c"In: the next picture slides in over the first. Out: the first slides away.",
    ),
    BORDER,
    SIZE,
    FEATHER,
    COLOR,
    Spec::GroupEnd,
];

pub(super) const SWAP: &[Spec] = &[
    Spec::Choice(
        c"direction",
        c"Direction",
        DIRECTIONS,
        2,
        c"Which way the first picture moves aside, the next one the other way, until they change places.",
    ),
    BORDER,
    SIZE,
    FEATHER,
    COLOR,
    Spec::GroupEnd,
];

pub(super) const SQUEEZE: &[Spec] = &[
    Spec::Choice(
        c"start",
        c"Start",
        &[c"Top", c"Bottom", c"Left", c"Right", c"Lateral", c"Vertical"],
        0,
        c"Where the next picture comes in from: an edge, or a line through the middle (Lateral: across, growing up and down; Vertical: down, growing sideways).",
    ),
    Spec::Toggle(
        c"squeezePrevious",
        c"Squeeze previous image",
        false,
        c"The first picture is squeezed into what is left of the frame; else the next one comes in over it.",
    ),
    BORDER,
    SIZE,
    FEATHER,
    COLOR,
    Spec::GroupEnd,
];

pub(super) const SPLIT: &[Spec] = &[
    Spec::Point(
        c"center",
        c"Center",
        [0.5, 0.5],
        c"Where the picture splits into four parts.",
    ),
    Spec::Choice(
        c"splitMode",
        c"Split mode",
        &[c"Push", c"Wipe", c"Squeeze"],
        0,
        c"How the four parts move: pushed away, cut where they stand, or squeezed.",
    ),
    Spec::Choice(
        c"direction",
        c"Direction",
        IN_OUT,
        0,
        c"In: the next picture's four parts close over the first. Out: the first one's parts open onto the next.",
    ),
    BORDER,
    SIZE,
    FEATHER,
    COLOR,
    Spec::GroupEnd,
];

pub(super) const ZOOM: &[Spec] = &[
    Spec::Choice(
        c"direction",
        c"Direction",
        &[c"Zoom in", c"Zoom out"],
        0,
        c"Zoom in: the next picture grows from Center. Zoom out: the first one shrinks into it.",
    ),
    Spec::Point(
        c"center",
        c"Center",
        [0.5, 0.5],
        c"Where the picture grows from or shrinks into.",
    ),
    STEREO,
    BORDER,
    SIZE,
    FEATHER,
    COLOR,
    Spec::GroupEnd,
];

pub(super) const SPIRAL: &[Spec] = &[
    Spec::Number(
        c"turns",
        c"Turns",
        1.0,
        0.0,
        10.0,
        c"How many times the picture circles on its way.",
    ),
    Spec::Number(
        c"zoom",
        c"Zoom (%)",
        50.0,
        0.0,
        100.0,
        c"How big the picture is where it starts (in) or ends (out), in percent of the frame.",
    ),
    Spec::Choice(
        c"orientation",
        c"Orientation",
        &[c"Left", c"Top", c"Right", c"Bottom"],
        0,
        c"The side the picture comes in from or goes out to.",
    ),
    Spec::Choice(
        c"motion",
        c"Motion",
        &[c"Clockwise", c"Counterclockwise"],
        0,
        c"Which way the picture circles.",
    ),
    Spec::Choice(
        c"direction",
        c"Direction",
        &[c"Spiral in", c"Spiral out"],
        0,
        c"Spiral in: the next picture circles in over the first. Spiral out: the first circles away.",
    ),
    BORDER,
    SIZE,
    FEATHER,
    COLOR,
    Spec::GroupEnd,
];

/// The kinds this module draws.
pub(super) fn draws(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::BarnDoor
            | Kind::Push
            | Kind::Slide
            | Kind::Swap
            | Kind::Squeeze
            | Kind::Split
            | Kind::Zoom
            | Kind::Spiral
    )
}

/// Transition `kind`'s move and border from its values (`get`), `p` of
/// the way (Spiral's angle turns with it).
fn settings(kind: Kind, get: &dyn Fn(Spec) -> Vec<f64>, p: f64) -> (Move, Edging) {
    let v = Vals::of(kind.specs(), get);
    let edging = Edging {
        size: v.f(c"borderSize") as f32,
        feather: v.f(c"borderFeather") as f32,
        color: v.get_rgb(c"borderColor").map(|c| c as f32),
    };
    // Centers as VEGAS keeps them: from the top.
    let center = || {
        let c = v.point(c"center");
        [c[0] as f32, (1.0 - c[1]) as f32]
    };
    let out = v.i(c"direction") != 0;
    let m = match kind {
        Kind::BarnDoor => Move::BarnDoor {
            vertical: v.i(c"orientation") != 0,
            out,
        },
        Kind::Push => Move::Push {
            dir: v.i(c"direction").clamp(0, 3),
            push_off: v.b(c"pushOff"),
        },
        Kind::Slide => Move::Slide {
            angle: v.f(c"angle"),
            out,
            k: 1.0,
        },
        Kind::Swap => Move::Swap {
            dir: v.i(c"direction").clamp(0, 3),
        },
        Kind::Squeeze => Move::Squeeze {
            start: v.i(c"start").clamp(0, 5),
            squeeze: v.b(c"squeezePrevious"),
        },
        Kind::Split => Move::Split {
            center: center(),
            mode: v.i(c"splitMode").clamp(0, 2),
            out,
        },
        Kind::Zoom => Move::Zoom {
            center: center(),
            out,
        },
        _ => {
            // Spiral: Slide's move, its angle turning from the side it
            // starts on.
            let side = f64::from(v.i(c"orientation").clamp(0, 3)) * 90.0;
            let turn = if v.i(c"motion") == 0 { 360.0 } else { -360.0 };
            Move::Slide {
                angle: side + turn * p * v.f(c"turns"),
                out,
                k: v.f(c"zoom") * 0.01,
            }
        }
    };
    (m, edging)
}

/// Renders transition `kind` on the processor.
pub(super) fn render(kind: Kind, fx: Fx, args: Props) -> OfxStatus {
    render_steps(fx, args, &|f, time, progress| {
        let (m, edging) = settings(kind, &|spec| spec.read(fx, time), progress);
        m.drawing(&edging, f, progress)
    })
}

/// Renders a transition drawn by steps on the processor: `drawing` gives
/// a frame's (its size, the time and the progress).
pub(super) fn render_steps(
    fx: Fx,
    args: Props,
    drawing: &dyn Fn(&Frame, f64, f64) -> Drawing,
) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let scale: [f64; 2] = args.dbls(kOfxImageEffectPropRenderScale);
    let scale = scale.map(|v| if v > 0.0 { v } else { 1.0 });
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let from = fx
        .clip(kOfxImageEffectTransitionSourceFromClipName)
        .image(time);
    let to = fx
        .clip(kOfxImageEffectTransitionSourceToClipName)
        .image(time);
    for img in [&from, &to].into_iter().flatten() {
        if img.depth != out.depth || img.channels != out.channels {
            return kOfxStatErrImageFormat;
        }
    }
    let progress = fx.param(kOfxImageEffectTransitionParamName).f64_at(time);
    let (origin, frame) = fx.frame();
    let par = if out.par > 0.0 { out.par } else { 1.0 };
    let f = Frame {
        w: (frame[0] * scale[0] / par).round().max(1.0) as usize,
        h: (frame[1] * scale[1]).round().max(1.0) as usize,
        par,
    };
    let at = (
        (origin[0] * scale[0] / par).round() as c_int,
        (origin[1] * scale[1]).round() as c_int,
    );
    let (a, b) = (Buf::read(from.as_ref()), Buf::read(to.as_ref()));
    let (a, b) = (framed(&a, at, &f), framed(&b, at, &f));
    drop(from);
    drop(to);
    let drawing = drawing(&f, time, progress);
    let drawn = drawing.prepare(&f);
    let (w, h) = (f.w as i32, f.h as i32);
    // The pictures with rows from the top, their edges held.
    let pic = |second: bool, i: i32, j: i32| {
        let p = if second { &b } else { &a };
        let (i, j) = (i.clamp(0, w - 1), j.clamp(0, h - 1));
        p[((h - 1 - j) * w + i) as usize]
    };
    let out_straight = out.channels == 4 && out.unpremultiplied;
    let columns = x1.max(out.bounds[0])..x2.min(out.bounds[2]);
    let fill = |y: c_int| {
        let Some(row) = out.row(y) else { return };
        let fy = y - at.1;
        for x in columns.clone() {
            let fx = x - at.0;
            let mut c = if fx < 0 || fy < 0 || fx >= w || fy >= h {
                [0.0; 4]
            } else {
                drawing.pixel(&drawn, fx, h - 1 - fy, &pic)
            };
            if out_straight && c[3] > 0.0 {
                c = [c[0] / c[3], c[1] / c[3], c[2] / c[3], c[3]];
            }
            // SAFETY: a row the output has, a column inside its bounds.
            unsafe { crate::colors::put_pixel(&out, row, x, c) };
        }
    };
    for_rows(y1.max(out.bounds[1])..y2.min(out.bounds[3]), &fill);
    drop(out);
    kOfxStatOK
}

/// What the card draws for transition `kind` at `progress` in a frame of
/// `size` pixels: the same steps, from a table.
pub(super) fn plan(
    kind: Kind,
    values: crate::accel::Values<'_>,
    size: [f64; 2],
    progress: f64,
) -> TransitionPlan {
    let (m, edging) = settings(
        kind,
        &|spec| {
            values
                .nums(spec.name())
                .map(<[f64]>::to_vec)
                .unwrap_or_else(|| spec.default())
        },
        progress,
    );
    let f = Frame {
        w: size[0].max(1.0) as usize,
        h: size[1].max(1.0) as usize,
        par: 1.0,
    };
    plan_steps(&m.drawing(&edging, &f, progress), &f)
}

/// What the card draws for a frame `f` drawn by `drawing`: its steps as
/// a table.
pub(super) fn plan_steps(drawing: &Drawing, f: &Frame) -> TransitionPlan {
    TransitionPlan {
        kernel: TransitionKernel::Moves,
        f: [0.0; 24],
        i: [0; 4],
        j: [0; 4],
        steps: TransitionSteps::Table {
            table: drawing.table(f),
        },
    }
}

// Presets, VEGAS's.

/// The factory presets of transition `kind`.
pub(super) fn presets(kind: Kind) -> Vec<crate::Preset> {
    use crate::{Preset, PresetValue};
    const BLACK: [f64; 3] = [0.0, 0.0, 0.0];
    const WHITE: [f64; 3] = [1.0, 1.0, 1.0];
    const RED: [f64; 3] = [1.0, 0.0, 0.0];
    const GREEN: [f64; 3] = [0.0, 1.0, 0.0];
    const BLUE: [f64; 3] = [0.0, 0.0, 1.0];
    const YELLOW: [f64; 3] = [1.0, 1.0, 0.0];
    let num = |v: &[f64]| PresetValue::Num(v.to_vec());
    // A preset: its own values, then its border's.
    let p = |name: &'static str,
             mut values: Vec<(&'static str, PresetValue)>,
             (size, feather, color): (f64, f64, [f64; 3])| {
        values.extend([
            ("borderSize", num(&[size])),
            ("borderFeather", num(&[feather])),
            ("borderColor", num(&color)),
        ]);
        Preset { name, values }
    };
    const NONE: (f64, f64, [f64; 3]) = (0.0, 0.0, BLACK);
    match kind {
        Kind::BarnDoor => {
            let w = |name, orientation: f64, dir: f64, border| {
                p(
                    name,
                    vec![
                        ("orientation", num(&[orientation])),
                        ("direction", num(&[dir])),
                    ],
                    border,
                )
            };
            vec![
                w("Vertical, In, No Border", 0.0, 0.0, NONE),
                w("Vertical, Out, No Border", 0.0, 1.0, NONE),
                w("Horizontal, In, No Border", 1.0, 0.0, NONE),
                w("Horizontal, Out, No Border", 1.0, 1.0, NONE),
                w("Vertical, In, White Border", 0.0, 0.0, (0.1, 0.0, WHITE)),
                w("Horizontal, Out, White Border", 1.0, 1.0, (0.1, 0.0, WHITE)),
                w("Vertical, Out, Black Border", 0.0, 1.0, (0.0, 0.07, BLACK)),
                w("Horizontal, In, Black Border", 1.0, 0.0, (0.0, 0.07, BLACK)),
                w(
                    "Horizontal, Out, Yellow Border",
                    1.0,
                    1.0,
                    (0.0, 0.1, YELLOW),
                ),
                w("Vertical, Out, Red Border", 0.0, 1.0, (0.0, 0.3, RED)),
            ]
        }
        Kind::Push => {
            let w = |name, dir: f64, off: bool, border| {
                p(
                    name,
                    vec![
                        ("direction", num(&[dir])),
                        ("pushOff", num(&[f64::from(u8::from(off))])),
                    ],
                    border,
                )
            };
            vec![
                w("Push Up", 0.0, true, NONE),
                w("Push Down", 1.0, true, NONE),
                w("Push Left", 2.0, true, NONE),
                w("Push Right", 3.0, true, NONE),
                w("Push In, Down", 1.0, false, NONE),
                w("Push In, Left", 2.0, false, NONE),
                w("Push In, Right", 3.0, false, NONE),
                w("Push In, Up", 0.0, false, NONE),
                w(
                    "Push In, Left, White Border",
                    2.0,
                    false,
                    (0.05, 0.0, WHITE),
                ),
                w(
                    "Push In, Right, Yellow Border",
                    3.0,
                    false,
                    (0.08, 0.08, YELLOW),
                ),
                w("Push Up, Red Border", 0.0, false, (0.0, 0.08, RED)),
                w("Push Down, Blue Border", 1.0, false, (0.05, 0.02, BLUE)),
            ]
        }
        Kind::Slide => {
            let w = |name, angle: f64, dir: f64| {
                p(
                    name,
                    vec![("angle", num(&[angle])), ("direction", num(&[dir]))],
                    NONE,
                )
            };
            vec![
                w("Slide In, Top-Left Corner", 45.0, 0.0),
                w("Slide Out, Bottom-Right Corner", 225.0, 1.0),
                w("Slide In, Left-Right", 0.0, 0.0),
                w("Slide In, Bottom-Up", 270.0, 0.0),
                w("Slide In, Right-Left", 180.0, 0.0),
                w("Slide In, Top-Down", 90.0, 0.0),
                w("Slide Out, Left-Right", 180.0, 1.0),
                w("Slide Out, Bottom-Up", 90.0, 1.0),
                w("Slide Out, Right-Left", 0.0, 1.0),
                w("Slide Out, Top-Down", 270.0, 1.0),
            ]
        }
        Kind::Swap => {
            let w = |name, dir: f64, border| p(name, vec![("direction", num(&[dir]))], border);
            vec![
                w("Swap Left", 2.0, NONE),
                w("Swap Right", 3.0, NONE),
                w("Swap Up", 0.0, NONE),
                w("Swap Down", 1.0, NONE),
                w("Swap Left, White Border", 2.0, (0.1, 0.0, WHITE)),
                w("Swap Right, Blue Border", 3.0, (0.0, 0.1, BLUE)),
                w("Swap Up, Yellow Border", 0.0, (0.05, 0.05, YELLOW)),
                w("Swap Down, Black Border", 1.0, (0.01, 0.06, BLACK)),
            ]
        }
        Kind::Squeeze => {
            let w = |name, start: f64, squeeze: bool, border| {
                p(
                    name,
                    vec![
                        ("start", num(&[start])),
                        ("squeezePrevious", num(&[f64::from(u8::from(squeeze))])),
                    ],
                    border,
                )
            };
            vec![
                w("Squeeze Down", 0.0, true, NONE),
                w("Squeeze Up", 1.0, true, NONE),
                w("Squeeze Right-Left", 3.0, true, NONE),
                w("Squeeze Left-Right", 2.0, true, NONE),
                w("Squeeze In, Down", 0.0, false, NONE),
                w("Squeeze In, Up", 1.0, false, NONE),
                w("Squeeze In, Right-Left", 3.0, false, NONE),
                w("Squeeze In, Left-Right", 2.0, false, NONE),
                w("Lateral", 5.0, true, NONE),
                w("Vertical", 4.0, true, NONE),
                w("Lateral In, White Border", 5.0, false, (0.01, 0.0, WHITE)),
                w("Vertical In, Red Border", 4.0, false, (0.01, 0.0, RED)),
                w(
                    "Squeeze In, Left-Right, Green Border",
                    2.0,
                    false,
                    (0.02, 0.01, GREEN),
                ),
                w(
                    "Squeeze In, Down, Blue Border",
                    0.0,
                    false,
                    (0.0, 0.03, BLUE),
                ),
            ]
        }
        Kind::Split => {
            let w = |name, center: [f64; 2], mode: f64, dir: f64, border| {
                p(
                    name,
                    vec![
                        ("center", num(&center)),
                        ("splitMode", num(&[mode])),
                        ("direction", num(&[dir])),
                    ],
                    border,
                )
            };
            let middle = [0.5, 0.5];
            vec![
                w("Push, Out, Center", middle, 0.0, 1.0, NONE),
                w("Wipe, Out, Center", middle, 1.0, 1.0, NONE),
                w("Squeeze, Out, Center", middle, 2.0, 1.0, NONE),
                w("Push, In, Center", middle, 0.0, 0.0, NONE),
                w("Wipe, In, Center", middle, 1.0, 0.0, NONE),
                w("Squeeze, In, Center", middle, 2.0, 0.0, NONE),
                w(
                    "Push, In, Top-Left Corner, White Border",
                    [0.06, 0.94],
                    0.0,
                    0.0,
                    (0.1, 0.3, WHITE),
                ),
                w(
                    "Push, Out, Top-Right Corner, Blue Border",
                    [0.94, 0.94],
                    0.0,
                    1.0,
                    (0.1, 0.2, BLUE),
                ),
            ]
        }
        Kind::Zoom => {
            let w = |name, dir: f64, center: [f64; 2], border| {
                p(
                    name,
                    vec![("direction", num(&[dir])), ("center", num(&center))],
                    border,
                )
            };
            vec![
                w("Zoom In, Center", 0.0, [0.5, 0.5], NONE),
                w("Zoom In, Top-Left", 0.0, [0.0, 1.0], NONE),
                w("Zoom In, Top-Right", 0.0, [1.0, 1.0], NONE),
                w("Zoom In, Bottom-Left", 0.0, [0.0, 0.0], NONE),
                w("Zoom In, Bottom-Right", 0.0, [1.0, 0.0], NONE),
                w("Zoom Out, Center", 1.0, [0.5, 0.5], NONE),
                w("Zoom Out, Top-Left", 1.0, [0.0, 1.0], NONE),
                w("Zoom Out, Top-Right", 1.0, [1.0, 1.0], NONE),
                w("Zoom Out, Bottom-Left", 1.0, [0.0, 0.0], NONE),
                w("Zoom Out, Bottom-Right", 1.0, [1.0, 0.0], NONE),
                w(
                    "Zoom Out, Center, Red Border",
                    1.0,
                    [0.5, 0.5],
                    (0.05, 0.0, RED),
                ),
                w(
                    "Zoom In, Center, White Border",
                    0.0,
                    [0.5, 0.5],
                    (0.15, 0.0, WHITE),
                ),
                w(
                    "Zoom Out, Top-Left, Blue Border",
                    1.0,
                    [0.0, 1.0],
                    (0.03, 0.05, BLUE),
                ),
                w(
                    "Zoom In, Bottom-Right, Yellow Border",
                    0.0,
                    [1.0, 0.0],
                    (0.0, 0.1, YELLOW),
                ),
            ]
        }
        _ => {
            // Spiral.
            let w = |name, orientation: f64, motion: f64, dir: f64, border| {
                p(
                    name,
                    vec![
                        ("turns", num(&[1.0])),
                        ("zoom", num(&[50.0])),
                        ("orientation", num(&[orientation])),
                        ("motion", num(&[motion])),
                        ("direction", num(&[dir])),
                    ],
                    border,
                )
            };
            vec![
                w("Spiral In, Left, Clockwise", 0.0, 0.0, 0.0, NONE),
                w("Spiral In, Up, Counter Clockwise", 1.0, 1.0, 0.0, NONE),
                w("Spiral In, Right, Clockwise", 2.0, 0.0, 0.0, NONE),
                w("Spiral In, Down, Counter Clockwise", 3.0, 1.0, 0.0, NONE),
                w("Spiral Out, Left, Clockwise", 0.0, 0.0, 1.0, NONE),
                w("Spiral Out, Up, Counter Clockwise", 1.0, 1.0, 1.0, NONE),
                w("Spiral Out, Right, Clockwise", 2.0, 0.0, 1.0, NONE),
                w("Spiral Out Down, Counter Clockwise", 3.0, 1.0, 1.0, NONE),
                w(
                    "Spiral In, Yellow Border",
                    3.0,
                    1.0,
                    0.0,
                    (0.0, 0.2, YELLOW),
                ),
                w("Spiral Out, Red Border", 3.0, 0.0, 1.0, (0.0, 0.1, RED)),
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: usize, h: usize) -> Frame {
        Frame { w, h, par: 1.0 }
    }

    const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
    const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

    /// Transition `kind`'s preset, `p` of the way, on a red first picture
    /// and a blue second one: the frame's pixels, rows from the top.
    fn drawn(kind: Kind, preset: &crate::Preset, f: &Frame, p: f64) -> Vec<[f32; 4]> {
        let (m, e) = settings(
            kind,
            &|spec| {
                let key = spec.name().to_str().unwrap_or("");
                preset
                    .values
                    .iter()
                    .find(|(n, _)| *n == key)
                    .and_then(|(_, v)| match v {
                        crate::PresetValue::Num(v) => Some(v.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| spec.default())
            },
            p,
        );
        let d = m.drawing(&e, f, p);
        let prepared = d.prepare(f);
        let pic = |second: bool, _: i32, _: i32| if second { BLUE } else { RED };
        let mut out = Vec::with_capacity(f.w * f.h);
        for y in 0..f.h as i32 {
            for x in 0..f.w as i32 {
                out.push(d.pixel(&prepared, x, y, &pic));
            }
        }
        out
    }

    fn named(kind: Kind, name: &str) -> crate::Preset {
        presets(kind)
            .into_iter()
            .find(|p| p.name == name)
            .expect("preset")
    }

    fn close(a: [f32; 4], b: [f32; 4]) -> bool {
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-3)
    }

    #[test]
    fn every_move_starts_on_the_first_picture_and_ends_on_the_second() {
        for kind in Kind::ALL.into_iter().filter(|k| draws(*k)) {
            let list = presets(kind);
            assert!(!list.is_empty(), "{kind:?}");
            for preset in &list {
                // Even sizes: in odd ones Barn Door's doors meet in the
                // middle column, each covering half of it, as VEGAS's do.
                for f in [frame(32, 18), frame(34, 20)] {
                    for (p, want) in [(0.0, RED), (1.0, BLUE)] {
                        for (i, c) in drawn(kind, preset, &f, p).into_iter().enumerate() {
                            assert!(
                                close(c, want),
                                "{kind:?} {} at {p}: pixel {i} {c:?}",
                                preset.name
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn halfway_the_pictures_are_where_vegas_puts_them() {
        let f = frame(32, 18);
        let at = |px: &[[f32; 4]], x: usize, y: usize| px[y * f.w + x];
        // Push Right: the second picture on the left, the first pushed right.
        let px = drawn(Kind::Push, &named(Kind::Push, "Push Right"), &f, 0.5);
        assert!(close(at(&px, 4, 9), BLUE) && close(at(&px, 27, 9), RED));
        // Barn Door's doors (Orientation 0 moves them across) close from
        // the edges.
        let px = drawn(
            Kind::BarnDoor,
            &named(Kind::BarnDoor, "Vertical, In, No Border"),
            &f,
            0.5,
        );
        assert!(close(at(&px, 2, 9), BLUE) && close(at(&px, 29, 9), BLUE));
        assert!(close(at(&px, 16, 9), RED));
        // Zoom grows from its center.
        let px = drawn(Kind::Zoom, &named(Kind::Zoom, "Zoom In, Center"), &f, 0.5);
        assert!(close(at(&px, 16, 9), BLUE) && close(at(&px, 1, 1), RED));
        // Slide In from the left.
        let px = drawn(
            Kind::Slide,
            &named(Kind::Slide, "Slide In, Left-Right"),
            &f,
            0.5,
        );
        assert!(close(at(&px, 4, 9), BLUE) && close(at(&px, 27, 9), RED));
        // Split Out: the first picture's parts open from the middle.
        let px = drawn(
            Kind::Split,
            &named(Kind::Split, "Wipe, Out, Center"),
            &f,
            0.5,
        );
        assert!(close(at(&px, 16, 9), BLUE) && close(at(&px, 1, 1), RED));
        // Squeeze Down: the second picture comes in from the top.
        let px = drawn(
            Kind::Squeeze,
            &named(Kind::Squeeze, "Squeeze Down"),
            &f,
            0.5,
        );
        assert!(close(at(&px, 16, 2), BLUE) && close(at(&px, 16, 15), RED));
        // A border shows between the pictures.
        let px = drawn(
            Kind::Swap,
            &named(Kind::Swap, "Swap Left, White Border"),
            &f,
            0.3,
        );
        assert!(px.iter().any(|c| close(*c, [1.0, 1.0, 1.0, 1.0])));
    }

    #[test]
    fn the_cards_table_has_a_record_for_each_step_drawn() {
        let f = frame(32, 18);
        let (m, e) = (
            Move::Split {
                center: [0.5, 0.5],
                mode: 0,
                out: true,
            },
            Edging {
                size: 0.1,
                feather: 0.1,
                color: [1.0, 1.0, 1.0],
            },
        );
        let d = m.drawing(&e, &f, 0.4);
        let table = d.table(&f);
        let drawn = d.prepare(&f).iter().filter(|s| s.is_some()).count();
        let borders = d
            .steps
            .iter()
            .filter(|s| matches!(s, Step::Border(_)))
            .count();
        assert_eq!(table[0][0] as usize, drawn + borders);
        assert_eq!(table.len(), 1 + (drawn + borders) * RECORD);
    }

    #[test]
    fn a_whole_picture_copies_itself() {
        let f = frame(16, 9);
        let all = Rect::of_pixels(16, 9);
        let d = Blit {
            second: false,
            src: all,
            dst: all,
            area: false,
            alpha: 1.0,
            composite: false,
        }
        .drawn(&f)
        .expect("drawn");
        let pic = |i: i32, j: i32| [i as f32, j as f32, 0.0, 1.0];
        for (x, y) in [(0, 0), (7, 4), (15, 8)] {
            let (share, v, whole) = d.at(x, y, &pic).expect("inside");
            assert!(whole && share == 1.0);
            assert_eq!(v, pic(x, y), "{x} {y}");
        }
    }

    #[test]
    fn a_picture_moved_half_a_pixel_covers_its_edge_half() {
        let f = frame(16, 9);
        let all = Rect::of_pixels(16, 9);
        let dst = Rect {
            l: ONE / 2,
            r: 16 * ONE + ONE / 2,
            ..all
        };
        let d = Blit {
            second: false,
            src: all,
            dst,
            area: false,
            alpha: 1.0,
            composite: false,
        }
        .drawn(&f)
        .expect("drawn");
        let pic = |i: i32, _j: i32| [i as f32, 0.0, 0.0, 1.0];
        // The first column half covered, reading the source's start.
        let (share, v, _) = d.at(0, 3, &pic).expect("edge");
        assert!((share - 0.5).abs() < 1e-6 && v[0] == 0.0, "{share} {v:?}");
        // Inside, pixel 5 reads between source pixels 4 and 5.
        let (share, v, _) = d.at(5, 3, &pic).expect("inside");
        assert!(share == 1.0 && (v[0] - 4.5).abs() < 1e-4, "{v:?}");
    }

    #[test]
    fn the_border_band_and_its_feather() {
        let b = Border {
            edges: vec![Edge::flags(1, 0, 9 * ONE, 8 * ONE)],
            size: 3 * ONE,
            feather: 2 * ONE,
            over: 0,
            color: [1.0, 1.0, 1.0],
        };
        // A vertical edge at x = 8, the band to its left: pixels 5–7 full,
        // 3–4 fading, 9 inside the feather.
        assert_eq!(b.at(6, 4), Some(1.0));
        let fading = b.at(4, 4).expect("feather");
        assert!(fading > 0.0 && fading < 1.0, "{fading}");
        assert!(b.at(0, 4).is_none());
        let inner = b.at(8, 4).expect("inside feather");
        assert!(inner > 0.0 && inner < 1.0, "{inner}");
    }
}
