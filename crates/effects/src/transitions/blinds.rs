//! Venetian Blinds as VEGAS draws it: the blinds' openness along the
//! frame's direction is a table of 8-bit steps twice the frame's diagonal
//! long, one a pixel, built from its middle out — open within `o` of every
//! multiple of the blinds' period (the frame's width over their number,
//! whatever the angle: VEGAS's horizontal blinds in a wide frame are fewer
//! than their number), `o` half the period times the progress, closed
//! elsewhere, the pixels an edge crosses partly — then softened by VEGAS's
//! blur over Feather times its length. Each pixel reads it at the frame's
//! middle plus its own place along the angle, rows from the top and
//! places in 16,384ths of a pixel as VEGAS's, between steps, and mixes the
//! pictures by it. The card reads the same table ([`plan`]).

use std::ffi::c_int;

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use super::Kind;
use super::pixel::{Frame, framed};
use crate::accel::{TransitionKernel, TransitionPlan, TransitionSteps};
use crate::kit::{Buf, Spec, Vals};
use crate::ofx::{Fx, Props, for_rows};
use crate::recursive_gauss;

/// A pixel, in the places' units.
const ONE: i32 = 16384;

pub(super) const VENETIAN_BLINDS: &[Spec] = &[
    Spec::Number(
        c"angle",
        c"Angle",
        0.0,
        0.0,
        360.0,
        c"The blinds' angle: 0 vertical blinds, 90 horizontal ones.",
    ),
    Spec::Number(
        c"feather",
        c"Feather",
        0.0,
        0.0,
        1.0,
        c"How soft the blinds' edges are.",
    ),
    Spec::Number(
        c"blinds",
        c"Number of blinds",
        1.0,
        1.0,
        30.0,
        c"How many blinds open across the frame's width.",
    ),
];

/// Venetian Blinds' settings.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Blinds {
    angle: f64,
    feather: f64,
    count: f64,
}

impl Blinds {
    fn read(get: &dyn Fn(Spec) -> Vec<f64>) -> Blinds {
        let v = Vals::of(VENETIAN_BLINDS, get);
        Blinds {
            angle: v.f(c"angle"),
            feather: v.f(c"feather"),
            // VEGAS's blinds are 1 to 30.
            count: v.f(c"blinds").clamp(1.0, 30.0),
        }
    }

    /// The openness table of a frame `w` × `h` at progress `p`.
    fn table(&self, w: usize, h: usize, p: f32) -> Vec<u8> {
        let mut t = steps(w, h, p, self.count);
        let len = t.len();
        if (self.feather * 16384.0) as i32 > 0 && len > 0 {
            let radius = len.max(1) as f64 * self.feather.clamp(0.0, 1.0);
            let line = recursive_gauss::Line::of_sigma(recursive_gauss::sigma(radius));
            let mut row: Vec<[f32; 4]> = t.iter().map(|&v| [f32::from(v); 4]).collect();
            line.run(&mut row, &mut Vec::new());
            // Back to steps, rounded as VEGAS's 8-bit blur rounds them.
            for (v, r) in t.iter_mut().zip(&row) {
                *v = f64::from(r[0]).round_ties_even().clamp(0.0, 255.0) as u8;
            }
        }
        t
    }

    /// Where each column and each row of a frame `f` moves the reading of
    /// the table, beside its middle: (columns, rows), in 16,384ths.
    fn offsets(&self, f: &Frame) -> (Vec<i32>, Vec<i32>) {
        let a = self.angle * std::f64::consts::PI / 180.0;
        let s = f64::from((a.sin() * 16384.0) as i32);
        let c = f64::from((a.cos() * f.par * 16384.0) as i32);
        let (w, h) = (f.w as i32, f.h as i32);
        let along = |from: i32, k: i32, m: f64| {
            -((f64::from(k * ONE - from * (ONE / 2)) * m * -6.103_515_625e-5) as i32)
        };
        (
            (0..w).map(|x| along(w, x, c)).collect(),
            (0..h).map(|y| along(h, y, s)).collect(),
        )
    }
}

/// The table's steps before any softening (`count` blinds, `p` of the
/// way): 255 where the second picture shows.
fn steps(w: usize, h: usize, p: f32, count: f64) -> Vec<u8> {
    let d = ((h as f64).powi(2) + (w as f64).powi(2)).sqrt();
    let len = (((d + d) * 16384.0) as u32 >> 14) as usize;
    let mut t = vec![0u8; len];
    let progress = (p * 16384.0) as i32;
    if progress == 0 || len == 0 {
        return t;
    }
    let period = ((f64::from((w as f64 * 16384.0) as i32) * 16384.0)
        / f64::from((count * 16384.0) as i32)) as i32;
    // The open part's edge and the closed part's, from the middle.
    let mut open_to = (f64::from(period / 2) * f64::from(progress) * 6.103_515_625e-5) as i32;
    let mut open_from = period - open_to;
    let part = |v: i32| (((v & 0x3fff) * 255 + 0x2000) >> 14) as u8;
    let (mut to_px, mut from_px) = (open_to >> 14, open_from >> 14);
    let mut open = true;
    let (mut right, mut left) = (len / 2, (len as isize - 1) / 2);
    let mut k = 0;
    while right < len && left >= 0 {
        let v = if k == to_px {
            let v = if from_px == to_px {
                let v = if open_from < open_to {
                    part(open_to - open_from)
                } else {
                    255 - part(open_from - open_to)
                };
                open_from += period;
                from_px = open_from >> 14;
                v
            } else {
                open = false;
                part(open_to)
            };
            open_to += period;
            to_px = open_to >> 14;
            v
        } else if k == from_px {
            open = true;
            let v = 255 - part(open_from);
            open_from += period;
            from_px = open_from >> 14;
            v
        } else if open {
            255
        } else {
            0
        };
        t[right] = v;
        t[left as usize] = v;
        right += 1;
        left -= 1;
        k += 1;
    }
    t
}

/// The table's step `u` (16,384ths from its start) reads: between its
/// steps, as VEGAS mixes them in 8 bits.
fn openness(t: &[u8], u: i32) -> f32 {
    let i = (u >> 14).clamp(0, t.len() as i32 - 1) as usize;
    let a = i32::from(t[i]);
    let b = i32::from(t[(i + 1).min(t.len() - 1)]);
    let mut v = (b - a) * (((u & 0x3fff) * 255) >> 14) + 0x80;
    v = (((v >> 8) + v) >> 8) + a;
    v.clamp(0, 255) as f32 * 0.003_921_569
}

/// The kinds this module draws.
pub(super) fn draws(kind: Kind) -> bool {
    kind == Kind::VenetianBlinds
}

/// Renders Venetian Blinds on the processor.
pub(super) fn render(fx: Fx, args: Props) -> OfxStatus {
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
    let blinds = Blinds::read(&|spec| spec.read(fx, time));
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
    let table = blinds.table(f.w, f.h, progress as f32);
    let (columns_at, rows_at) = blinds.offsets(&f);
    let middle = (table.len() as i32 * ONE) / 2;
    let (w, h) = (f.w as i32, f.h as i32);
    let out_straight = out.channels == 4 && out.unpremultiplied;
    let columns = x1.max(out.bounds[0])..x2.min(out.bounds[2]);
    let fill = |y: c_int| {
        let Some(row) = out.row(y) else { return };
        let fy = y - at.1;
        for x in columns.clone() {
            let fx = x - at.0;
            let mut c = if fx < 0 || fy < 0 || fx >= w || fy >= h || table.is_empty() {
                [0.0; 4]
            } else {
                // Rows from the top, as VEGAS counts them.
                let top = (h - 1 - fy) as usize;
                let k = openness(&table, middle + rows_at[top] + columns_at[fx as usize]);
                let i = fy as usize * f.w + fx as usize;
                let (pa, pb) = (a[i], b[i]);
                std::array::from_fn(|n| (pb[n] - pa[n]) * k + pa[n])
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

/// What the card draws for Venetian Blinds at `progress` in a frame of
/// `size` pixels (`canonical` its size in canonical units: the pixels'
/// shape): a table, its first row the table's length and middle, then a
/// row a step — the step, the column's offset and the row's (rows from
/// the top) where there are as many.
pub(super) fn plan(
    values: crate::accel::Values<'_>,
    canonical: [f64; 2],
    size: [f64; 2],
    progress: f64,
) -> TransitionPlan {
    let blinds = Blinds::read(&|spec| {
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
    let f = Frame {
        w: size[0].max(1.0) as usize,
        h: size[1].max(1.0) as usize,
        par,
    };
    let steps = blinds.table(f.w, f.h, progress as f32);
    let (columns, rows) = blinds.offsets(&f);
    let len = steps.len();
    let mut table = vec![[len as i32, (len as i32 * ONE) / 2, 0, 0]];
    table.extend((0..len.max(f.w).max(f.h)).map(|i| {
        [
            steps.get(i).map_or(0, |&v| i32::from(v)),
            columns.get(i).copied().unwrap_or(0),
            rows.get(i).copied().unwrap_or(0),
            0,
        ]
    }));
    TransitionPlan {
        kernel: TransitionKernel::VenetianBlinds,
        f: [0.0; 24],
        i: [0; 4],
        j: [0; 4],
        steps: TransitionSteps::Table { table },
    }
}

/// VEGAS's presets.
pub(super) fn presets() -> Vec<crate::Preset> {
    use crate::{Preset, PresetValue};
    let num = |v: f64| PresetValue::Num(vec![v]);
    let w = |name: &'static str, angle: f64, feather: f64, blinds: f64| Preset {
        name,
        values: vec![
            ("angle", num(angle)),
            ("feather", num(feather)),
            ("blinds", num(blinds)),
        ],
    };
    vec![
        w("One Vertical Blind", 0.0, 0.0, 0.0),
        w("Five Vertical Blinds", 0.0, 0.0, 5.0),
        w("Seven Horizontal Blinds", 90.0, 0.0, 7.0),
        w("Sloped Blinds", 45.0, 0.07, 3.0),
        w("Fifteen Tilted Blinds", 105.0, 0.01, 15.0),
        w("Many Crooked Blinds", 190.0, 0.0, 30.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_opens_from_the_blinds_middles() {
        // 1920 wide, five blinds: a period of 384 pixels; halfway each
        // blind is open 96 pixels each side of its middle line.
        let t = steps(1920, 1080, 0.5, 5.0);
        let mid = t.len() / 2;
        assert_eq!(t.len(), 4405);
        assert!(t[mid..mid + 95].iter().all(|&v| v == 255));
        assert!(t[mid + 97..mid + 287].iter().all(|&v| v == 0));
        assert!(t[mid + 289..mid + 480].iter().all(|&v| v == 255));
        // Mirrored about the middle.
        assert_eq!(t[mid - 100], t[mid + 100]);
        // Closed at the start, open at the end.
        assert!(steps(1920, 1080, 0.0, 5.0).iter().all(|&v| v == 0));
        assert!(steps(1920, 1080, 1.0, 5.0).iter().all(|&v| v == 255));
    }

    #[test]
    fn the_feather_softens_the_edges() {
        let b = Blinds {
            angle: 0.0,
            feather: 0.01,
            count: 5.0,
        };
        let t = b.table(320, 180, 0.5);
        let soft = t.iter().filter(|&&v| v > 0 && v < 255).count();
        assert!(soft > 10, "{soft}");
    }

    #[test]
    fn the_pixels_read_the_table_along_the_angle() {
        let b = Blinds {
            angle: 0.0,
            feather: 0.0,
            count: 5.0,
        };
        let f = Frame {
            w: 320,
            h: 180,
            par: 1.0,
        };
        let t = b.table(f.w, f.h, 0.5);
        let (columns, rows) = b.offsets(&f);
        let middle = (t.len() as i32 * ONE) / 2;
        let at = |x: usize, y: usize| openness(&t, middle + rows[y] + columns[x]);
        // Vertical blinds: the middle column open, a quarter period away
        // closed, whatever the row.
        assert!(at(160, 10) > 0.99 && at(160, 170) > 0.99);
        assert!(at(160 + 32, 90) < 0.01);
    }
}
