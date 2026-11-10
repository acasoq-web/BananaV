//! Portals as VEGAS draws it: squares of the second picture come in over
//! the first one at random places — BananaV's seeded random numbers
//! (`r250.rs`), so a seed always gives the same pattern —, each
//! showing a part of the picture
//! moved and scaled a little by Max offset and Max scale, as transparent
//! as Max transparency says. Over the first half they appear one after
//! another, the second picture fades in under them at the middle, and over
//! the second half they go in the reverse order; each has a border of
//! Border size around it. The frame is drawn by [`super::moves`]'s steps:
//! the pictures composited into their rectangles and the border's strips
//! laid over, in 16,384ths of a pixel, on the card too.

use bv_ofx_sys::OfxStatus;

use super::moves::{Blit, Drawing, Fill, Rect, Step, plan_steps, render_steps};
use super::pixel::Frame;
use crate::accel::TransitionPlan;
use crate::kit::{Spec, Vals};
use crate::ofx::{Fx, Props};
use crate::r250::R250;

pub(super) const PORTALS: &[Spec] = &[
    Spec::Int(
        c"seed",
        c"Random pattern seed",
        54321,
        0,
        16_777_216,
        c"Another number places the portals elsewhere.",
    ),
    Spec::Int(
        c"squares",
        c"Squares",
        12,
        1,
        256,
        c"How many portals open.",
    ),
    Spec::Number(
        c"maxTransparency",
        c"Max transparency",
        0.5,
        0.0,
        1.0,
        c"How opaque the portals are: 0 clear, 1 opaque.",
    ),
    Spec::Number(
        c"maxOffset",
        c"Max offset",
        0.5,
        0.0,
        1.0,
        c"How far the pictures in the portals are moved from their places, at most.",
    ),
    Spec::Number(
        c"maxScale",
        c"Max scale",
        0.5,
        0.0,
        1.0,
        c"How much the pictures in the portals are enlarged or reduced, at most.",
    ),
    Spec::Number(
        c"borderSize",
        c"Border size",
        0.5,
        0.0,
        1.0,
        c"How wide the border around each portal is.",
    ),
    Spec::Rgb(
        c"borderColor",
        c"Border color",
        [1.0, 1.0, 1.0],
        c"The borders' color.",
    ),
];

/// Portals' settings.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Portals {
    seed: u32,
    squares: i32,
    transparency: f64,
    offset: f64,
    scale: f64,
    border: f64,
    color: [f32; 3],
}

/// One of the portals: where it is in the frame and the part of the
/// picture it shows (0–1 from the frame's top left: left, top, right,
/// bottom), and how much it shows.
struct Square {
    at: [f32; 4],
    shows: [f32; 4],
    alpha: f32,
}

impl Portals {
    fn read(get: &dyn Fn(Spec) -> Vec<f64>) -> Portals {
        let v = Vals::of(PORTALS, get);
        Portals {
            seed: v.i(c"seed").clamp(0, 16_777_216) as u32,
            squares: v.i(c"squares").clamp(1, 256),
            transparency: v.f(c"maxTransparency"),
            offset: v.f(c"maxOffset"),
            scale: v.f(c"maxScale"),
            border: v.f(c"borderSize"),
            color: v.get_rgb(c"borderColor").map(|c| c as f32),
        }
    }

    /// The portals at progress `p` in a frame of pixels `par` times as wide
    /// as tall.
    fn squares(&self, p: f64, par: f64) -> Vec<Square> {
        let n = self.squares;
        let t = f64::from(2 * n + 1) * p;
        let now = t as i32;
        let frac = t - f64::from(now);
        let side = (1.0 / f64::from(n)).sqrt() * 1.4;
        let shift = self.offset * 0.4 * side;
        let mut rng = R250::new(self.seed);
        let clamp01 = |v: f32| v.clamp(0.0, 1.0);
        (0..n)
            .map(|k| {
                let u = rng.unit() as f32;
                let v = rng.unit() as f32;
                let mut w = (rng.signed() * side + side) as f32;
                let mut h = (rng.signed() * side + side) as f32;
                if par >= 1.0 {
                    h = (f64::from(h) * par) as f32;
                } else {
                    w = (f64::from(w) / par) as f32;
                }
                let at = [
                    clamp01(u - w * 0.5),
                    clamp01(v - h * 0.5),
                    clamp01(w * 0.5 + u),
                    clamp01(h * 0.5 + v),
                ];
                let mut shows = at;
                let dx = (rng.signed() * shift) as f32;
                let dy = (rng.signed() * shift) as f32;
                shows[0] += dx;
                shows[2] += dx;
                shows[1] += dy;
                shows[3] += dy;
                let grow = (rng.signed() * self.scale * 0.4 * side) as f32;
                shows[0] += grow;
                shows[1] += grow;
                shows[2] -= grow;
                shows[3] -= grow;
                // One after another, then going in the reverse order.
                let share = if now < n {
                    if k < now {
                        1.0
                    } else if k == now {
                        frac
                    } else {
                        0.0
                    }
                } else if now == n {
                    1.0
                } else {
                    let left = n - k;
                    if now - n < left {
                        1.0
                    } else if now - n == left {
                        1.0 - frac
                    } else {
                        0.0
                    }
                };
                Square {
                    at,
                    shows,
                    alpha: (share * self.transparency) as f32,
                }
            })
            .collect()
    }

    /// The frame `f`'s drawing at progress `p`.
    fn drawing(&self, f: &Frame, p: f64) -> Drawing {
        // VEGAS's progress is a float.
        let p = f64::from(p as f32);
        let frame = Rect::of_pixels(f.w, f.h);
        let (w, h) = (f.w as f32, f.h as f32);
        let n = self.squares;
        let t = f64::from(2 * n + 1) * p;
        let now = t as i32;
        let picture = |second: bool, src: Rect, dst: Rect, alpha: f32| {
            Step::Blit(Blit {
                second,
                src,
                dst,
                area: false,
                alpha,
                composite: true,
            })
        };
        // The picture under the portals: the first, then (at the middle) the
        // second fading in over it, then the second.
        let mut steps = Vec::new();
        if now > n {
            steps.push(picture(true, frame, frame, 1.0));
        } else {
            steps.push(picture(false, frame, frame, 1.0));
            if now == n {
                let frac = (t - f64::from(now)) as f32;
                if frac > 0.0 {
                    steps.push(picture(true, frame, frame, frac));
                }
            }
        }
        let place = |r: [f32; 4]| Rect {
            l: (f64::from(w * r[0]) * 16384.0) as i32,
            t: (f64::from(h * r[1]) * 16384.0) as i32,
            r: (f64::from(w * r[2]) * 16384.0) as i32,
            b: (f64::from(h * r[3]) * 16384.0) as i32,
        };
        let by = (f64::from(f.w.max(f.h) as i32) * self.border * 0.05) as f32;
        let bx = (f64::from(by) / f.par) as f32;
        for sq in self.squares(p, f.par) {
            if sq.alpha <= 0.0 {
                continue;
            }
            let dst = place(sq.at);
            if bx != 0.0 && by != 0.0 {
                let a = sq.alpha;
                let c = [self.color[0] * a, self.color[1] * a, self.color[2] * a, a];
                border(&mut steps, dst, bx, by, c);
            }
            steps.push(picture(true, place(sq.shows), dst, sq.alpha));
        }
        Drawing { steps }
    }
}

/// The border `bx` × `by` pixels wide around `square`, in `color`: the
/// whole pixels between the square rounded out and the outer edge rounded
/// in, then the outer edge's parts of pixels, then the inner edge's.
fn border(steps: &mut Vec<Step>, square: Rect, bx: f32, by: f32, color: [f32; 4]) {
    let floor = |v: i32| v & !0x3fff;
    let ceil = |v: i32| ((v - 1) | 0x3fff) + 1;
    let (gx, gy) = (
        (f64::from(bx) * 16384.0) as i32,
        (f64::from(by) * 16384.0) as i32,
    );
    let Rect { l, t, r, b } = square;
    let (ol, ot, or, ob) = (l - gx, t - gy, r + gx, b + gy);
    let (il, it, ir, ib) = (floor(l), floor(t), ceil(r), ceil(b));
    let (wl, wt, wr, wb) = (ceil(ol), ceil(ot), floor(or), floor(ob));
    let rects = [
        // Whole pixels.
        [wl, wt, wr, it],
        [wl, ib, wr, wb],
        [wl, it, il, ib],
        [ir, it, wr, ib],
        // The outer edge.
        [ol, ot, or, ceil(ot)],
        [ol, floor(ob), or, ob],
        [ol, ceil(ot), ceil(ol), floor(ob)],
        [floor(or), ceil(ot), or, floor(ob)],
        // The inner edge.
        [il, it, ir, t],
        [il, b, ir, ib],
        [il, t, l, b],
        [r, t, ir, b],
    ];
    for [l, t, r, b] in rects {
        steps.push(Step::Fill(Fill {
            dst: Rect { l, t, r, b },
            color,
        }));
    }
}

/// The kinds this module draws.
pub(super) fn draws(kind: super::Kind) -> bool {
    kind == super::Kind::Portals
}

/// Renders Portals on the processor.
pub(super) fn render(fx: Fx, args: Props) -> OfxStatus {
    render_steps(fx, args, &|f, time, p| {
        Portals::read(&|spec| spec.read(fx, time)).drawing(f, p)
    })
}

/// What the card draws for Portals at `progress` in a frame of `size`
/// pixels (`canonical` its size in canonical units: the pixels' shape).
pub(super) fn plan(
    values: crate::accel::Values<'_>,
    canonical: [f64; 2],
    size: [f64; 2],
    progress: f64,
) -> TransitionPlan {
    let portals = Portals::read(&|spec| {
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
    plan_steps(&portals.drawing(&f, progress), &f)
}

/// VEGAS's presets.
pub(super) fn presets() -> Vec<crate::Preset> {
    use crate::{Preset, PresetValue};
    let num = |v: &[f64]| PresetValue::Num(v.to_vec());
    #[allow(clippy::too_many_arguments)]
    let w = |name: &'static str,
             seed: f64,
             squares: f64,
             transparency: f64,
             offset: f64,
             scale: f64,
             border: f64,
             color: [f64; 3]| Preset {
        name,
        values: vec![
            ("seed", num(&[seed])),
            ("squares", num(&[squares])),
            ("maxTransparency", num(&[transparency])),
            ("maxOffset", num(&[offset])),
            ("maxScale", num(&[scale])),
            ("borderSize", num(&[border])),
            ("borderColor", num(&color)),
        ],
    };
    vec![
        w("White Wash", 83.0, 12.0, 0.5, 0.5, 0.5, 0.5, [1.0; 3]),
        w("Mondrian", 64.0, 6.0, 0.85, 0.1, 0.05, 0.1, [0.0; 3]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_random_numbers_are_a_seeded_shift_register() {
        // The seeding generator from seed 1 (the classic C library's
        // linear congruential one): 41, 18467, 6334, ...
        let mut state = 1u32;
        let mut rand = || {
            state = state.wrapping_mul(214_013).wrapping_add(2_531_011);
            (state >> 16) & 0x7fff
        };
        assert_eq!([rand(), rand(), rand()], [41, 18467, 6334]);
        // The same seed, the same numbers; another seed, others.
        let (mut a, mut b, mut c) = (R250::new(83), R250::new(83), R250::new(64));
        let (x, y, z) = (a.next(), b.next(), c.next());
        assert_eq!(x, y);
        assert_ne!(x, z);
    }

    #[test]
    fn portals_appear_one_by_one_and_go_in_reverse() {
        let p = Portals {
            seed: 83,
            squares: 4,
            transparency: 1.0,
            offset: 0.5,
            scale: 0.5,
            border: 0.5,
            color: [1.0; 3],
        };
        let shown =
            |t: f64| -> Vec<f32> { p.squares(t / 9.0, 1.0).iter().map(|s| s.alpha).collect() };
        assert_eq!(shown(0.0), vec![0.0; 4]);
        assert_eq!(shown(2.5), vec![1.0, 1.0, 0.5, 0.0]);
        assert_eq!(shown(4.5), vec![1.0; 4]);
        assert_eq!(shown(6.5), vec![1.0, 1.0, 0.5, 0.0]);
        assert_eq!(shown(9.0), vec![0.0; 4]);
    }

    #[test]
    fn the_frame_starts_on_the_first_picture_and_ends_on_the_second() {
        let p = Portals::read(&|spec| spec.default());
        let f = Frame {
            w: 64,
            h: 36,
            par: 1.0,
        };
        let (red, blue) = ([1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]);
        let pic = |second: bool, _: i32, _: i32| if second { blue } else { red };
        for (t, want) in [(0.0, red), (1.0, blue)] {
            let d = p.drawing(&f, t);
            let prepared = d.prepare(&f);
            for y in 0..36 {
                for x in 0..64 {
                    let c = d.pixel(&prepared, x, y, &pic);
                    assert!(
                        c.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-5),
                        "{t}: {x}, {y} {c:?}"
                    );
                }
            }
        }
        // Halfway every portal and its border show over the first picture.
        let d = p.drawing(&f, 0.4);
        let prepared = d.prepare(&f);
        let shown = (0..36)
            .flat_map(|y| (0..64).map(move |x| (x, y)))
            .filter(|&(x, y)| d.pixel(&prepared, x, y, &pic) != red)
            .count();
        assert!(shown > 100, "{shown}");
    }
}
