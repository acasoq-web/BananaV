//! Page Peel, Page Roll and Page Loop as VEGAS draws them, in floats: the
//! frame's pixels (rows from the bottom, the pixels' height taken in their
//! widths) turned to the transition's direction, `u` along it and `v`
//! across, and each pixel's `u` mapped onto a page bent around a cylinder
//! whose axis crosses the frame — layer after layer, each read between
//! pixels where it lands on the picture, lit and laid over what is drawn
//! by its share. The page is the first picture peeling or rolling off the
//! second (Page Peel, Page Roll) or the second looping in over the first
//! (Page Loop). A pixel whose layers land outside the picture's pixels, or
//! whose outcome differs from its right, upper and upper right
//! neighbours', is instead the mean of four samples around it, each read at
//! the nearest pixel, as VEGAS smooths the page's edges; its last column
//! compares with what VEGAS's row before last left there. The card draws
//! the same, a pixel at a time ([`plan`]).

use std::ffi::c_int;

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use super::Kind;
use super::pixel::{Frame, framed};
use crate::accel::{TransitionKernel, TransitionPlan, TransitionSteps};
use crate::kit::{Buf, Spec, Vals, for_chunks};
use crate::ofx::{Fx, Props, for_rows};

pub(super) const PAGE_PEEL: &[Spec] = &[
    Spec::Number(
        c"peelAngle",
        c"Peel angle",
        210.0,
        0.0,
        360.0,
        c"Where the page peels from: 180 the left, 270 the top, 210 the top left corner.",
    ),
    Spec::Number(
        c"foldRadius",
        c"Fold radius",
        0.3,
        0.0,
        1.0,
        c"How round the fold is, a part of the frame's shorter side.",
    ),
    Spec::Number(
        c"slideAmount",
        c"Slide amount",
        0.0,
        0.0,
        1.0,
        c"How far the page also slides away as it peels.",
    ),
    Spec::Number(
        c"peelOpacity",
        c"Peel opacity",
        0.75,
        0.0,
        1.0,
        c"How opaque the page's back is once it peels over.",
    ),
    Spec::Number(
        c"perspective",
        c"Perspective",
        0.4,
        0.0,
        1.0,
        c"How much nearer the curled page looks.",
    ),
    Spec::Rgb(
        c"lightColor",
        c"Light color",
        [1.0, 1.0, 1.0],
        c"The color of the light on the page.",
    ),
];
pub(super) const PAGE_ROLL: &[Spec] = &[
    Spec::Number(
        c"rollAngle",
        c"Roll angle",
        210.0,
        0.0,
        360.0,
        c"Where the page rolls from: 180 the left, 270 the top, 210 the top left corner.",
    ),
    Spec::Number(
        c"foldRadius",
        c"Fold radius",
        0.1,
        0.0,
        0.2,
        c"How thick the roll is, a part of the frame's shorter side.",
    ),
    Spec::Number(
        c"slideAmount",
        c"Slide amount",
        0.0,
        0.0,
        1.0,
        c"How far the page also slides away as it rolls.",
    ),
    Spec::Number(
        c"rollOpacity",
        c"Roll opacity",
        0.75,
        0.0,
        1.0,
        c"How opaque the rolled page is.",
    ),
    Spec::Number(
        c"perspective",
        c"Perspective",
        0.4,
        0.0,
        1.0,
        c"How much nearer the rolled page looks.",
    ),
    Spec::Rgb(
        c"lightColor",
        c"Light color",
        [1.0, 1.0, 1.0],
        c"The color of the light inside the roll.",
    ),
];
pub(super) const PAGE_LOOP: &[Spec] = &[
    Spec::Number(
        c"paperAngle",
        c"Paper angle",
        210.0,
        0.0,
        360.0,
        c"Where the paper comes in from: 180 the left, 270 the top, 210 the top left corner.",
    ),
    Spec::Number(
        c"paperOpacity",
        c"Paper opacity",
        0.5,
        0.0,
        1.0,
        c"How opaque the paper is as it comes in; it is whole once in.",
    ),
    Spec::Number(
        c"loopRadius",
        c"Loop radius",
        0.4,
        0.0,
        1.0,
        c"How big the loop is, a part of half the frame.",
    ),
    Spec::Number(
        c"loopPosition",
        c"Loop position",
        0.2,
        0.0,
        1.0,
        c"Where the loop starts: 0 at the frame's far side, 1 at the side the paper comes from.",
    ),
    Spec::Number(
        c"perspective",
        c"Perspective",
        0.5,
        0.0,
        1.0,
        c"How much nearer the looping paper looks.",
    ),
    Spec::Rgb(
        c"lightColor",
        c"Light color",
        [1.0, 1.0, 1.0],
        c"The color of the light on the loop.",
    ),
];

/// A page transition's settings: Page Loop's paper angle, opacity, loop
/// radius and position in the same slots as the others' angle, opacity,
/// fold radius and slide amount.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Page {
    kind: Kind,
    angle: f64,
    radius: f64,
    /// Slide amount (Page Loop: the loop's position).
    slide: f64,
    opacity: f64,
    perspective: f64,
    light: [f64; 3],
}

impl Page {
    fn read(kind: Kind, get: &dyn Fn(Spec) -> Vec<f64>) -> Page {
        let v = Vals::of(specs(kind), get);
        let (angle, opacity, radius, slide) = match kind {
            Kind::PagePeel => (c"peelAngle", c"peelOpacity", c"foldRadius", c"slideAmount"),
            Kind::PageRoll => (c"rollAngle", c"rollOpacity", c"foldRadius", c"slideAmount"),
            _ => (
                c"paperAngle",
                c"paperOpacity",
                c"loopRadius",
                c"loopPosition",
            ),
        };
        Page {
            kind,
            angle: v.f(angle),
            radius: v.f(radius),
            slide: v.f(slide),
            opacity: v.f(opacity),
            perspective: v.f(c"perspective"),
            light: v.get_rgb(c"lightColor"),
        }
    }
}

fn specs(kind: Kind) -> &'static [Spec] {
    match kind {
        Kind::PagePeel => PAGE_PEEL,
        Kind::PageRoll => PAGE_ROLL,
        _ => PAGE_LOOP,
    }
}

/// π and the like in single precision.
const PI: f32 = std::f32::consts::PI;
const HALF_PI: f32 = std::f32::consts::FRAC_PI_2;
const QUARTER_PI: f32 = std::f32::consts::FRAC_PI_4;
/// Light's falloff from its peak, in 255ths a radian (200 × 4 ⁄ π).
const SLOPE: f32 = 200.0 * 1.273_239_5;
/// A byte's share.
const SHARE: f32 = 0.003_921_569;

/// Page Peel's numbers: the page slides `slide` along, the fold is at
/// `fold`, the cylinder's radius `r`.
#[derive(Clone, Copy, Debug)]
struct Peel {
    slide: f32,
    fold: f32,
    r: f32,
    inv_r: f32,
    /// Where the page's back starts (πr past the fold).
    back: f32,
    /// What the flat back mirrors about (twice the fold plus πr).
    mirror: f32,
    /// The cylinder's far side.
    end: f32,
}

/// Page Roll's: a roll of radius `r` with an inner turn `r9` (0.9 of it).
#[derive(Clone, Copy, Debug)]
struct Roll {
    slide: f32,
    fold: f32,
    r: f32,
    r9: f32,
    inv_r: f32,
    inv_r9: f32,
    back: f32,
    end: f32,
}

/// Page Loop's: the loop at `at` with radius `r`, the paper rising toward
/// it over `rise`, moved `shift` back; `alpha` the paper's opacity in
/// 255ths.
#[derive(Clone, Copy, Debug)]
struct Looped {
    at: f32,
    rise: f32,
    shift: f32,
    around: f32,
    alpha: f32,
    r: f32,
    lo: f32,
    hi: f32,
    /// The transition not over yet: the rise still shows.
    unfinished: bool,
}

#[derive(Clone, Copy, Debug)]
enum Shape {
    Peel(Peel),
    Roll(Roll),
    Loop(Looped),
}

/// A page transition's numbers for a frame.
#[derive(Clone, Copy, Debug)]
struct Curl {
    shape: Shape,
    layers: usize,
    /// The pixels' height in their widths (1 ⁄ the pixel aspect), and its
    /// inverse.
    k: f32,
    inv_k: f32,
    sin: f32,
    cos: f32,
    light: [f32; 3],
    /// The perspective: how much nearer the bent page comes.
    persp: f32,
    /// The page's opacity in 255ths, and what is left of it.
    opacity: u8,
    rest: u8,
}

/// Where a layer lands on the page, how lit and how opaque it is.
#[derive(Clone, Copy, Debug)]
struct Hit {
    /// The page's place along and across the direction.
    u: f32,
    v: f32,
    /// The light's share and the layer's, in 255ths.
    light: u8,
    alpha: u8,
    /// The light's color (Page Roll's shade inside the roll is black).
    color: [f32; 3],
}

/// What a layer is at a point.
enum Mapped {
    Hit(Hit),
    Miss,
    /// The layer before, again: VEGAS's Page Loop draws the paper's layer
    /// again where its loop's layers are not.
    Again,
}

/// The frame's extent along `theta` (`k` the pixels' height in widths),
/// with VEGAS's angle in tenths of a degree folded into the first quarter.
fn extent(theta: f32, w: f32, h: f32, k: f32, looped: bool) -> f32 {
    let kh = k * h;
    let d = (w * w + kh * kh).sqrt();
    let mut t = if looped {
        ((theta * 572.957_8) as i32 % 1800) as f32 * 0.001_745_329_2
    } else {
        ((((theta * 1800.0) / PI) as i32 % 1800) as f32 * PI) / 1800.0
    };
    if t > HALF_PI {
        t = PI - t;
    }
    d * (t - kh.atan2(w)).cos()
}

impl Curl {
    /// The numbers of `page` at progress `p` in a frame `w` × `h` of
    /// pixels `par` times as wide as tall.
    fn new(page: &Page, w: usize, h: usize, par: f64, p: f64) -> Curl {
        let par = if par > 0.0 { par as f32 } else { 1.0 };
        let (sy, sx) = if par >= 1.0 {
            (1.0 / par, 1.0)
        } else {
            (1.0, par)
        };
        let a = if page.kind == Kind::PagePeel {
            (360.0 - page.angle as f32) * 0.017_453_292
        } else {
            ((360.0 - page.angle) * 0.017_453_292_384_743_69) as f32
        };
        let c = a.cos();
        let mut theta = a;
        if c > 1e-6 {
            theta = (a.sin() * sy).atan2(c * sx);
            if theta < 0.0 {
                theta += 6.283_185_5;
            }
        }
        let mut k = 1.0 / par;
        if k <= 1e-8 {
            k = 1.0;
        }
        let p = p as f32;
        let (wf, hf) = (w as f32, h as f32);
        let rho = page.radius as f32;
        let persp = page.perspective as f32 * 0.125;
        let opacity = if page.kind == Kind::PageRoll {
            (page.opacity * 255.0) as i32 as u8
        } else {
            (page.opacity as f32 * 255.0) as i32 as u8
        };
        let rest = 0xffu8.wrapping_sub(opacity);
        let looped = page.kind == Kind::PageLoop;
        let e = extent(theta, wf, hf, k, looped);
        let short = if k * hf <= wf { k * hf } else { wf };
        let (shape, layers) = match page.kind {
            Kind::PageLoop => {
                let pp = p * p;
                let at_ = page.slide as f32;
                let x = (((1.0 - at_) * pp + at_) - 0.5) * e;
                let half = e * 0.5;
                let r = half * rho;
                let rise = (half - x) / 1.396_263_2f32.sin();
                let o = page.opacity as f32;
                let shape = Looped {
                    at: x,
                    rise,
                    shift: (half + x + r * 6.283_185_5 + rise * 1.396_263_2) * (1.0 - p),
                    around: r * 6.283_185_5,
                    alpha: ((1.0 - o) * pp + o) * 255.0,
                    r,
                    lo: x - r,
                    hi: r + x,
                    unfinished: p < 1.0,
                };
                (Shape::Loop(shape), 3)
            }
            kind => {
                let r = short * rho;
                let mut slide = (p - rho * 1.570_796_4) * (page.slide as f32) * short;
                if slide < 0.0 {
                    slide = 0.0;
                }
                let fold = (e * 0.5 - (r + e) * p) - slide;
                let inv = |r: f32| if r <= 0.0 { 1e14 } else { 1.0 / r };
                let back = r * PI + fold;
                if kind == Kind::PageRoll {
                    let r9 = r * 0.9;
                    let shape = Roll {
                        slide,
                        fold,
                        r,
                        r9,
                        inv_r: inv(r),
                        inv_r9: inv(r9),
                        back,
                        end: fold + r,
                    };
                    (Shape::Roll(shape), 4)
                } else {
                    let shape = Peel {
                        slide,
                        fold,
                        r,
                        inv_r: inv(r),
                        back,
                        mirror: fold + fold + r * PI,
                        end: fold + r,
                    };
                    (Shape::Peel(shape), 2)
                }
            }
        };
        Curl {
            shape,
            layers,
            k,
            inv_k: 1.0 / k,
            sin: theta.sin(),
            cos: theta.cos(),
            light: page.light.map(|c| c as f32),
            persp,
            opacity,
            rest,
        }
    }

    /// Layer `layer` at `u` along the direction and `v` across it.
    fn map(&self, u: f32, v: f32, layer: usize) -> Mapped {
        let light = self.light;
        let hit = |u: f32, v: f32, lit: u8, alpha: u8, color: [f32; 3]| {
            Mapped::Hit(Hit {
                u,
                v,
                light: lit,
                alpha,
                color,
            })
        };
        let byte = |x: f32| x as i32 as u8;
        let share = |x: f32| byte(x * 0.318_309_87).wrapping_add(self.opacity);
        let rest = f32::from(self.rest);
        let persp = self.persp;
        match self.shape {
            Shape::Peel(c) => {
                let u = u - c.slide;
                if u < c.fold {
                    return if layer == 0 {
                        hit(u, v, 0, 0xff, light)
                    } else {
                        let f = 1.0 - (persp + persp);
                        hit((c.mirror - u) * f, f * v, 0, self.opacity, light)
                    };
                }
                if c.end <= u {
                    return Mapped::Miss;
                }
                let x = ((u - c.fold) * c.inv_r).asin();
                let cos = x.cos();
                if layer == 0 {
                    let f = (cos - 1.0) * persp + 1.0;
                    hit(
                        (c.r * x + c.fold) * f,
                        f * v,
                        0,
                        share((PI - x) * rest),
                        light,
                    )
                } else {
                    let f = 1.0 - (cos + 1.0) * persp;
                    let lit = 200u8.wrapping_sub(byte((x - QUARTER_PI).abs() * SLOPE));
                    hit((c.back - c.r * x) * f, f * v, lit, share(rest * x), light)
                }
            }
            Shape::Roll(c) => {
                let u = u - c.slide;
                let black = [0.0; 3];
                match layer {
                    0 => {
                        if u < c.fold {
                            return hit(u, v, 0, 0xff, light);
                        }
                        if c.end <= u {
                            return Mapped::Miss;
                        }
                        let x = ((u - c.fold) * c.inv_r).asin();
                        let alpha = share(rest * (PI - x));
                        let f = (x.cos() - 1.0) * persp + 1.0;
                        hit((x * c.r + c.fold) * f, f * v, 0, alpha, light)
                    }
                    1 => {
                        if u < c.fold - c.r || c.fold + c.r9 <= u {
                            return Mapped::Miss;
                        }
                        let (x, along) = if c.fold <= u {
                            let x = ((u - c.fold) * c.inv_r9).asin();
                            (x, c.r * x + c.fold + c.r * 6.283_185_5)
                        } else {
                            let x = ((c.fold - u) * c.inv_r).asin();
                            (x, (c.r * PI + c.back) - c.r * x)
                        };
                        let alpha = share((PI - x) * rest);
                        let f = (x.cos() - 1.0) * persp + 1.0;
                        hit(along * f, f * v, 0, alpha, light)
                    }
                    2 => {
                        if u < c.fold - c.r9 || c.r9 + c.fold <= u {
                            return Mapped::Miss;
                        }
                        let (x, along, dim, color) = if c.fold <= u {
                            let x = ((u - c.fold) * c.inv_r9).asin();
                            let dim = byte((x - QUARTER_PI).abs() * SLOPE);
                            (x, c.fold - c.r9 * x, dim, light)
                        } else {
                            let x = ((c.fold - u) * c.inv_r9).asin();
                            let dim = byte((HALF_PI - x) * SLOPE * 0.5);
                            (x, c.r9 * x + c.fold, dim, black)
                        };
                        let alpha = share(rest * x);
                        let f = 1.0 - (x.cos() + 1.0) * persp;
                        let lit = 200u8.wrapping_sub(dim);
                        hit((along + c.r9 * 9.424_778) * f, f * v, lit, alpha, color)
                    }
                    _ => {
                        if u < c.fold - c.r || c.end <= u {
                            return Mapped::Miss;
                        }
                        let (x, along, dim, color) = if c.fold <= u {
                            let x = ((u - c.fold) * c.inv_r).asin();
                            let dim = byte((x - QUARTER_PI).abs() * SLOPE);
                            (x, c.back - c.r * x, dim, light)
                        } else {
                            let x = ((c.fold - u) * c.inv_r).asin();
                            let dim = byte((HALF_PI - x) * SLOPE * 0.5);
                            (x, c.r * x + c.back, dim, black)
                        };
                        let alpha = share(rest * x);
                        let t = (x.cos() + 1.0) * persp;
                        let lit = 200u8.wrapping_sub(dim);
                        hit(along * (1.0 - t), (1.0 - t) * v, lit, alpha, color)
                    }
                }
            }
            Shape::Loop(c) => {
                let opaque = 255.0 - c.alpha;
                let glow = |x: f32| 1.0 - ((x - QUARTER_PI) * 1.273_239_5).abs();
                if layer == 0 {
                    let mut u = u;
                    if c.at <= u {
                        if c.unfinished {
                            let x = ((u - c.at) / c.rise).asin();
                            let f = 1.0 - (1.0 - x.cos()) * persp;
                            let alpha = opaque * x * 0.636_619_75 + c.alpha;
                            let lit = byte(glow(x) * 200.0 + 0.5);
                            let along = ((c.at + c.around + c.rise * x) - c.shift) * f;
                            return hit(along, f * v, lit, byte(alpha + 0.5), light);
                        }
                        u += c.around;
                    }
                    return hit(u - c.shift, v, 0, byte(c.alpha + 0.5), light);
                }
                if u <= c.lo || c.hi <= u {
                    return Mapped::Again;
                }
                let r = c.r.max(0.001);
                let alpha = |x: f32| byte(opaque * x * 0.636_619_75 + c.alpha + 0.5);
                if c.at <= u {
                    let mut x = ((u - c.at) / r).asin();
                    let alpha = alpha(x);
                    let cos = x.cos();
                    let (turn, lit) = if layer == 1 {
                        (1.0 - cos, byte(glow(x) * 200.0))
                    } else {
                        x = PI - x;
                        (cos + 1.0, 0)
                    };
                    let f = 1.0 - turn * persp;
                    hit(((r * x + c.at) - c.shift) * f, f * v, lit, alpha, light)
                } else {
                    let x = ((c.at - u) / r).asin();
                    let alpha = alpha(x);
                    let cos = x.cos();
                    if layer == 1 {
                        let f = 1.0 - (1.0 - cos) * persp;
                        let along = (((6.283_185_5 - x) * r + c.at) - c.shift) * f;
                        hit(along, f * v, 0, alpha, light)
                    } else {
                        let f = 1.0 - (cos + 1.0) * persp;
                        let along = (((x + PI) * r + c.at) - c.shift) * f;
                        hit(along, f * v, byte(glow(x) * 200.0 + 0.5), alpha, light)
                    }
                }
            }
        }
    }

    /// The point `x`, `y` (the frame's pixels from its middle, y up in
    /// pixel widths) along and across the direction.
    fn turned(&self, x: f32, y: f32) -> (f32, f32) {
        let (ys, yc) = (y * self.sin, y * self.cos);
        (x * self.cos + ys, yc - x * self.sin)
    }

    /// Where a hit lands in the picture: its row (from the bottom) and
    /// column, between pixels.
    fn landing(&self, hit: &Hit, half: (f32, f32)) -> (f32, f32) {
        let row = (hit.v * self.cos + hit.u * self.sin) * self.inv_k + half.1;
        let col = hit.v * -self.sin + hit.u * self.cos + half.0;
        (row, col)
    }

    /// Each layer's hit at `u`, `v` in turn.
    fn layers(&self, u: f32, v: f32, mut f: impl FnMut(usize, &Hit)) {
        let mut last: Option<Hit> = None;
        for layer in 0..self.layers {
            let hit = match self.map(u, v, layer) {
                Mapped::Hit(h) => {
                    last = Some(h);
                    h
                }
                Mapped::Again => match last {
                    Some(h) => h,
                    None => continue,
                },
                Mapped::Miss => continue,
            };
            f(layer, &hit);
        }
    }
}

/// `c` lit by `hit`'s light: toward its color by its share, its alpha
/// toward opaque.
fn lit(c: [f32; 4], hit: &Hit) -> [f32; 4] {
    if hit.light == 0 {
        return c;
    }
    let s = f32::from(hit.light) * SHARE;
    [
        (hit.color[0] - c[0]) * s + c[0],
        (hit.color[1] - c[1]) * s + c[1],
        (hit.color[2] - c[2]) * s + c[2],
        (1.0 - c[3]) * s + c[3],
    ]
}

/// How a pixel's layers ended: none landed on the picture (`NONE`), or
/// the last that did, with `EDGE` when it read past the picture's pixels.
const NONE: u8 = 0x7f;
const EDGE: u8 = 0x80;

/// The frame drawn from the first picture `a` and the second `b` (rows
/// from the bottom), `w` × `h`.
fn draw(curl: &Curl, w: usize, h: usize, a: &[[f32; 4]], b: &[[f32; 4]]) -> Vec<[f32; 4]> {
    // Page Loop's page is the second picture, over the first.
    let (page, under) = match curl.shape {
        Shape::Loop(_) => (b, a),
        _ => (a, b),
    };
    let half = (w as f32 * 0.5, h as f32 * 0.5);
    let (wi, hi) = (w as i32, h as i32);
    let mut cells: Vec<([f32; 4], u8)> = under.iter().map(|&c| (c, NONE)).collect();
    for_chunks(&mut cells, w, |j, row| {
        let y = (j as f32 - half.1) * curl.k;
        for (i, cell) in row.iter_mut().enumerate() {
            let (u, v) = curl.turned(i as f32 - half.0, y);
            let (out, flag) = cell;
            curl.layers(u, v, |layer, hit| {
                let (row, col) = curl.landing(hit, half);
                if !(row >= -1.0 && row < h as f32 && col >= -1.0 && col < w as f32) {
                    return;
                }
                let (r0, c0) = (row.floor() as i32, col.floor() as i32);
                let left = c0 < 0;
                let right = c0 + 1 >= wi;
                let low = r0 < 0;
                let high = r0 + 1 >= hi;
                let at = |r: i32, c: i32| page[r as usize * w + c as usize];
                let p00 = if left || low { *out } else { at(r0, c0) };
                let p01 = if right || low { *out } else { at(r0, c0 + 1) };
                let p10 = if left || high { *out } else { at(r0 + 1, c0) };
                let p11 = if right || high {
                    *out
                } else {
                    at(r0 + 1, c0 + 1)
                };
                *flag = layer as u8
                    | if left || right || low || high {
                        EDGE
                    } else {
                        0
                    };
                let wr = 1.0 - (row - r0 as f32);
                let wc = 1.0 - (col - c0 as f32);
                let (fc, fr) = (1.0 - wc, 1.0 - wr);
                let c: [f32; 4] = std::array::from_fn(|k| {
                    (wc * p00[k] + fc * p01[k]) * wr + (fc * p11[k] + wc * p10[k]) * fr
                });
                let c = lit(c, hit);
                let s = f32::from(hit.alpha) * SHARE;
                *out = std::array::from_fn(|k| (c[k] - out[k]) * s + out[k]);
            });
        }
    });
    let flags: Vec<u8> = cells.iter().map(|c| c.1).collect();
    let mut out: Vec<[f32; 4]> = cells.into_iter().map(|c| c.0).collect();
    let flag = |r: usize, i: usize| flags[r * w + i];
    for_chunks(&mut out, w, |r, row| {
        for (i, px) in row.iter_mut().enumerate() {
            let a0 = flag(r, i);
            let a1 = flag(r, (i + 1).min(w - 1));
            let (b0, b1) = if r + 1 < h {
                let b1 = if i + 1 < w {
                    flag(r + 1, i + 1)
                } else if r >= 2 {
                    flag(r - 2, w - 1)
                } else {
                    0
                };
                (flag(r + 1, i), b1)
            } else {
                (a0, a1)
            };
            if a0 > NONE || (a0 & a1 & b0 & b1) != (a0 | a1 | b0 | b1) {
                *px = smoothed(curl, w, h, page, under, r, i, half);
            }
        }
    });
    out
}

/// Pixel `i` of row `r` as the mean of four samples a third of a pixel
/// apart, each layer read at its nearest pixel.
#[allow(clippy::too_many_arguments)]
fn smoothed(
    curl: &Curl,
    w: usize,
    h: usize,
    page: &[[f32; 4]],
    under: &[[f32; 4]],
    r: usize,
    i: usize,
    half: (f32, f32),
) -> [f32; 4] {
    let base = under[r * w + i];
    let mut sum = [0.0f32; 4];
    let (top, right) = ((r + 1) as f32 - 0.1, i as f32 + 0.7);
    let mut y = (r + 1) as f32 - 0.666;
    while y < top {
        let yy = (y - half.1) * curl.k;
        let mut x = i as f32 + 0.333;
        while x < right {
            let (u, v) = curl.turned(x - half.0, yy);
            let mut c = base;
            curl.layers(u, v, |_, hit| {
                let (row, col) = curl.landing(hit, half);
                if !(row >= 0.0 && row < h as f32 && col >= 0.0 && col < w as f32) {
                    return;
                }
                let s = lit(page[row as usize * w + col as usize], hit);
                let k = f32::from(hit.alpha) * SHARE;
                c = std::array::from_fn(|n| c[n] + (s[n] - c[n]) * k);
            });
            for n in 0..4 {
                sum[n] += c[n];
            }
            x += 0.333;
        }
        y += 0.333;
    }
    sum.map(|s| s * 0.25)
}

/// The kinds this module draws.
pub(super) fn draws(kind: Kind) -> bool {
    matches!(kind, Kind::PagePeel | Kind::PageRoll | Kind::PageLoop)
}

/// Renders a page transition on the processor.
pub(super) fn render(kind: Kind, fx: Fx, args: Props) -> OfxStatus {
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
    let page = Page::read(kind, &|spec| spec.read(fx, time));
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
    let curl = Curl::new(&page, f.w, f.h, f.par, progress);
    let drawn = draw(&curl, f.w, f.h, &a, &b);
    let out_straight = out.channels == 4 && out.unpremultiplied;
    let columns = x1.max(out.bounds[0])..x2.min(out.bounds[2]);
    let fill = |y: c_int| {
        let Some(row) = out.row(y) else { return };
        let fy = y - at.1;
        for x in columns.clone() {
            let fxx = x - at.0;
            let mut c = if fxx < 0 || fy < 0 || fxx >= f.w as c_int || fy >= f.h as c_int {
                [0.0; 4]
            } else {
                drawn[fy as usize * f.w + fxx as usize]
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

/// What the card draws for page transition `kind` at `progress` in a
/// frame of `size` pixels (`canonical` its size in canonical units: the
/// pixels' shape): the same numbers, worked out once.
pub(super) fn plan(
    kind: Kind,
    values: crate::accel::Values<'_>,
    canonical: [f64; 2],
    size: [f64; 2],
    progress: f64,
) -> TransitionPlan {
    let page = Page::read(kind, &|spec| {
        values
            .nums(spec.name())
            .map(<[f64]>::to_vec)
            .unwrap_or_else(|| spec.default())
    });
    let par = if size[0] > 0.0 && size[1] > 0.0 && canonical[1] > 0.0 {
        (canonical[0] / size[0]) / (canonical[1] / size[1])
    } else {
        1.0
    };
    let (w, h) = (size[0].max(1.0) as usize, size[1].max(1.0) as usize);
    let c = Curl::new(&page, w, h, par, progress);
    let mut f = [0.0f32; 24];
    f[..8].copy_from_slice(&[
        c.k, c.inv_k, c.sin, c.cos, c.light[0], c.light[1], c.light[2], c.persp,
    ]);
    let (shape, numbers, unfinished) = match c.shape {
        Shape::Peel(s) => (
            0,
            vec![s.slide, s.fold, s.r, s.inv_r, s.back, s.mirror, s.end],
            false,
        ),
        Shape::Roll(s) => (
            1,
            vec![s.slide, s.fold, s.r, s.inv_r, s.back, s.r9, s.inv_r9, s.end],
            false,
        ),
        Shape::Loop(s) => (
            2,
            vec![s.at, s.rise, s.shift, s.around, s.alpha, s.r, s.lo, s.hi],
            s.unfinished,
        ),
    };
    f[8..8 + numbers.len()].copy_from_slice(&numbers);
    TransitionPlan {
        kernel: TransitionKernel::Pages,
        f,
        i: [
            shape,
            c.layers as i32,
            i32::from(c.opacity),
            i32::from(c.rest),
        ],
        j: [u32::from(unfinished), 0, 0, 0],
        steps: TransitionSteps::None,
    }
}

/// VEGAS's presets.
pub(super) fn presets(kind: Kind) -> Vec<crate::Preset> {
    use crate::{Preset, PresetValue};
    let num = |v: &[f64]| PresetValue::Num(v.to_vec());
    let (angle, opacity, radius, slide) = match kind {
        Kind::PagePeel => ("peelAngle", "peelOpacity", "foldRadius", "slideAmount"),
        Kind::PageRoll => ("rollAngle", "rollOpacity", "foldRadius", "slideAmount"),
        _ => ("paperAngle", "paperOpacity", "loopRadius", "loopPosition"),
    };
    let w = |name: &'static str, a: f64, rad: f64, s: f64, o: f64, persp: f64, light: [f64; 3]| {
        Preset {
            name,
            values: vec![
                (angle, num(&[a])),
                (radius, num(&[rad])),
                (slide, num(&[s])),
                (opacity, num(&[o])),
                ("perspective", num(&[persp])),
                ("lightColor", num(&light)),
            ],
        }
    };
    let white = [1.0; 3];
    match kind {
        Kind::PagePeel => vec![
            w("Top-Left, Medium Fold", 210.0, 0.3, 0.0, 0.75, 0.4, white),
            w("Top-Right, Medium Fold", 330.0, 0.3, 0.0, 0.75, 0.4, white),
            w(
                "Bottom-Right, Medium Fold",
                30.0,
                0.3,
                0.0,
                0.75,
                0.4,
                white,
            ),
            w(
                "Bottom-Left, Medium Fold",
                150.0,
                0.3,
                0.0,
                0.75,
                0.4,
                white,
            ),
            w(
                "Left, Tight Fold, Translucent, Yellow Light",
                180.0,
                0.15,
                0.0,
                0.5,
                0.4,
                [1.0, 1.0, 0.0],
            ),
            w(
                "Top, Tight Fold, Opaque, Red Light",
                270.0,
                0.15,
                0.0,
                1.0,
                0.4,
                [1.0, 0.0, 0.0],
            ),
            w(
                "Top-Left, Slide, Tight Fold, Opaque",
                210.0,
                0.2,
                1.0,
                1.0,
                0.4,
                white,
            ),
            w(
                "Bottom-Right, Loose Fold, Opaque",
                30.0,
                0.45,
                0.0,
                1.0,
                0.4,
                white,
            ),
        ],
        Kind::PageRoll => vec![
            w("Top-Left, Medium Curl", 210.0, 0.1, 0.0, 0.75, 0.4, white),
            w("Top-Right, Medium Curl", 330.0, 0.1, 0.0, 0.75, 0.4, white),
            w(
                "Bottom-Right, Medium Curl",
                30.0,
                0.1,
                0.0,
                0.75,
                0.4,
                white,
            ),
            w(
                "Bottom-Left, Medium Curl",
                150.0,
                0.1,
                0.0,
                0.75,
                0.4,
                white,
            ),
            w(
                "Left, Tight Curl, Translucent, Yellow Light",
                180.0,
                0.05,
                0.0,
                0.5,
                0.4,
                [1.0, 1.0, 0.0],
            ),
            w(
                "Top, Tight Curl, Opaque, Red Light",
                270.0,
                0.05,
                0.0,
                1.0,
                0.4,
                [1.0, 0.0, 0.0],
            ),
            w(
                "Top-Left, Slide, Tight Curl, Opaque",
                210.0,
                0.08,
                0.4,
                1.0,
                0.4,
                white,
            ),
            w(
                "Bottom-Right, Slide, Loose Curl, Opaque",
                30.0,
                0.2,
                1.0,
                1.0,
                0.4,
                white,
            ),
        ],
        _ => vec![
            w("Top-Left, Medium Loop", 210.0, 0.4, 0.2, 0.5, 0.5, white),
            w("Top-Right, Medium Loop", 330.0, 0.4, 0.2, 0.5, 0.5, white),
            w(
                "Bottom-Right, Small Translucent Loop",
                30.0,
                0.3,
                0.0,
                0.0,
                1.0,
                white,
            ),
            w(
                "Bottom-Left, Small Translucent Loop",
                150.0,
                0.3,
                0.0,
                0.0,
                1.0,
                white,
            ),
            w(
                "Left, Small Loop, Yellow Light",
                180.0,
                0.15,
                0.75,
                0.5,
                0.5,
                [0.8, 0.8, 0.501_961],
            ),
            w(
                "Top, Large Loop, Red Light",
                270.0,
                0.8,
                0.5,
                1.0,
                0.5,
                [1.0, 0.301_961, 0.301_961],
            ),
            w(
                "Top-Left, Opaque, No Loop",
                210.0,
                0.0,
                0.1,
                1.0,
                1.0,
                white,
            ),
            w(
                "Bottom-Right, Large Opaque Loop",
                30.0,
                0.8,
                0.1,
                1.0,
                1.0,
                white,
            ),
        ],
    }
}

/// The settings of `kind`'s preset `name`.
#[cfg(test)]
fn named(kind: Kind, name: &str) -> Page {
    let preset = presets(kind)
        .into_iter()
        .find(|p| p.name == name)
        .expect("a preset");
    Page::read(kind, &|spec| {
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
    const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

    fn frame(kind: Kind, preset: &str, p: f64) -> Vec<[f32; 4]> {
        let (w, h) = (64, 36);
        let a = vec![RED; w * h];
        let b = vec![BLUE; w * h];
        let curl = Curl::new(&named(kind, preset), w, h, 1.0, p);
        draw(&curl, w, h, &a, &b)
    }

    fn close(a: [f32; 4], b: [f32; 4]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
    }

    #[test]
    fn peel_and_roll_start_on_the_first_picture_and_end_on_the_second() {
        for (kind, preset) in [
            (Kind::PagePeel, "Top-Left, Medium Fold"),
            (Kind::PagePeel, "Bottom-Right, Loose Fold, Opaque"),
            (Kind::PageRoll, "Top-Left, Medium Curl"),
            (Kind::PageRoll, "Bottom-Right, Slide, Loose Curl, Opaque"),
        ] {
            let start = frame(kind, preset, 0.0);
            assert!(start.iter().all(|&c| close(c, RED)), "{preset} at 0");
            let end = frame(kind, preset, 1.0);
            assert!(end.iter().all(|&c| close(c, BLUE)), "{preset} at 1");
        }
    }

    #[test]
    fn a_loop_brings_the_second_picture_in_whole() {
        for preset in ["Top-Left, Medium Loop", "Top, Large Loop, Red Light"] {
            // At the start the rising paper's edge, nearer, reaches into the
            // frame's corner, as VEGAS's does.
            let start = frame(Kind::PageLoop, preset, 0.0);
            let off = start.iter().filter(|&&c| !close(c, RED)).count();
            assert!(off * 100 < start.len(), "{preset} at 0: {off}");
            let end = frame(Kind::PageLoop, preset, 1.0);
            assert!(end.iter().all(|&c| close(c, BLUE)), "{preset} at 1");
        }
    }

    #[test]
    fn the_top_left_corner_peels_first() {
        // Rows from the bottom: the top left is the last row's start.
        let (w, h) = (64usize, 36usize);
        let f = frame(Kind::PagePeel, "Top-Left, Medium Fold", 0.25);
        let at = |x: usize, y: usize| f[y * w + x];
        assert!(!close(at(0, h - 1), RED), "{:?}", at(0, h - 1));
        assert!(close(at(w - 1, 0), RED), "{:?}", at(w - 1, 0));
        let f = frame(Kind::PagePeel, "Bottom-Right, Medium Fold", 0.25);
        let at = |x: usize, y: usize| f[y * w + x];
        assert!(!close(at(w - 1, 0), RED));
        assert!(close(at(0, h - 1), RED));
    }

    #[test]
    fn the_curled_back_takes_the_light() {
        // A yellow light: the red page's curled back turns toward yellow
        // (its front, curling, lets the blue under it through a little).
        let f = frame(
            Kind::PagePeel,
            "Left, Tight Fold, Translucent, Yellow Light",
            0.4,
        );
        assert!(f.iter().any(|c| c[0] > 0.5 && c[1] > 0.05 && c[1] > c[2]));
        // Page Roll's inside is shaded where it turns away.
        let f = frame(Kind::PageRoll, "Top, Tight Curl, Opaque, Red Light", 0.4);
        assert!(f.iter().any(|c| c[0] < 0.9 && c[2] < 0.01 && c[3] > 0.99));
    }

    #[test]
    fn the_loops_paper_is_drawn_again_where_its_loop_is_not() {
        // Paper opacity 0 at progress p: the paper's share is p² there, laid
        // three times over.
        let mut page = named(Kind::PageLoop, "Bottom-Right, Small Translucent Loop");
        page.radius = 0.0;
        let curl = Curl::new(&page, 64, 36, 1.0, 0.7);
        let Shape::Loop(c) = curl.shape else {
            panic!("a loop")
        };
        let a = (c.alpha + 0.5) as i32 as f32 * SHARE;
        let thrice = 1.0 - (1.0 - a).powi(3);
        let (w, h) = (64, 36);
        let f = draw(&curl, w, h, &vec![RED; w * h], &vec![BLUE; w * h]);
        assert!(
            f.iter().any(|c| (c[2] - thrice).abs() < 1e-3),
            "{thrice} {:?}",
            f[w * h / 2]
        );
    }
}
