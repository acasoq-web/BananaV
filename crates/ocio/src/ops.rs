//! What a processor does to a color, step by step: OpenColorIO's
//! transforms once a configuration's references are resolved and their
//! directions applied.

use std::sync::Arc;

use crate::aces::{self, SdrOutput};
use crate::curves::Curve;
use crate::math::{self, M3};

/// A step on an RGB color (linear or encoded, as the step before left
/// it).
#[derive(Clone, Debug)]
pub enum Op {
    /// m · c + offset.
    Matrix(M3, [f64; 3]),
    /// Each channel through a curve: to linear (`true`) or from it.
    Curve(Curve, bool),
    /// x^e per channel; `negatives`: what happens below 0.
    Power([f64; 3], Negatives),
    /// log_base(x) (`inverse`: base^x), negatives and 0 to the smallest
    /// float's log.
    Log { base: f64, inverse: bool },
    /// OpenColorIO's LogAffine/LogCamera:
    /// log_slope · log_base(lin_slope · x + lin_offset) + log_offset above
    /// `lin_break` (when there is one: a straight line below, joining it).
    LogAffine {
        base: f64,
        log_slope: [f64; 3],
        log_offset: [f64; 3],
        lin_slope: [f64; 3],
        lin_offset: [f64; 3],
        camera: Option<Camera>,
        inverse: bool,
    },
    /// ASC CDL: (c · slope + offset)^power, then saturation about
    /// Rec. 709's luma; `clamp` keeps 0–1 (style "asc").
    Cdl {
        slope: [f64; 3],
        offset: [f64; 3],
        power: [f64; 3],
        sat: f64,
        clamp: bool,
        inverse: bool,
    },
    /// Each channel scaled from min_in–max_in onto min_out–max_out, clamped
    /// there (`clamp`).
    Range {
        scale: f64,
        offset: f64,
        lo: Option<f64>,
        hi: Option<f64>,
    },
    /// A curve per channel from a table, over `min`–`max`.
    Lut1(Arc<bv_lut::Table1>, bool),
    /// A lattice of colors.
    Lut3(Arc<bv_lut::Table3>, bv_lut::Interpolation),
    /// ACES's RRT and an SDR ODT, to CIE XYZ display-linear.
    AcesSdr(Arc<SdrOutput>),
    /// ACES 1.3's Reference Gamut Compression (`true`: its inverse).
    GamutCompress(bool),
    /// RGB ↔ HSV (OpenColorIO's RGB_TO_HSV fixed function).
    RgbToHsv(bool),
    /// XYZ ↔ xyY.
    XyzToXyy(bool),
}

/// LogCamera's linear segment: below `lin_break` on the linear side, a
/// line of `slope` through the curve's value there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub lin_break: [f64; 3],
    pub linear_slope: Option<[f64; 3]>,
}

/// What a power does below 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Negatives {
    Clamp,
    Mirror,
    PassThrough,
}

impl Op {
    pub fn apply(&self, c: [f64; 3]) -> [f64; 3] {
        match self {
            Op::Matrix(m, o) => {
                let v = math::apply(m, c);
                [v[0] + o[0], v[1] + o[1], v[2] + o[2]]
            }
            Op::Curve(curve, to_linear) => c.map(|v| {
                if *to_linear {
                    curve.to_linear(v)
                } else {
                    curve.from_linear(v)
                }
            }),
            Op::Power(e, neg) => [0, 1, 2].map(|k| {
                let (x, e) = (c[k], e[k]);
                if x >= 0.0 {
                    x.powf(e)
                } else {
                    match neg {
                        Negatives::Clamp => 0.0,
                        Negatives::Mirror => -(-x).powf(e),
                        Negatives::PassThrough => x,
                    }
                }
            }),
            Op::Log { base, inverse } => c.map(|v| {
                if *inverse {
                    base.powf(v)
                } else {
                    v.max(f64::MIN_POSITIVE).log(*base)
                }
            }),
            Op::LogAffine {
                base,
                log_slope,
                log_offset,
                lin_slope,
                lin_offset,
                camera,
                inverse,
            } => [0, 1, 2].map(|k| {
                let (b, ls, lo, ms, mo) = (
                    *base,
                    log_slope[k],
                    log_offset[k],
                    lin_slope[k],
                    lin_offset[k],
                );
                let log = |x: f64| ls * (ms * x + mo).max(f64::MIN_POSITIVE).log(b) + lo;
                let lin = |y: f64| (b.powf((y - lo) / ls) - mo) / ms;
                match camera {
                    None => {
                        if *inverse {
                            lin(c[k])
                        } else {
                            log(c[k])
                        }
                    }
                    Some(cam) => {
                        let brk = cam.lin_break[k];
                        let slope = cam
                            .linear_slope
                            .map_or_else(|| ls * ms / ((ms * brk + mo) * b.ln()), |s| s[k]);
                        let at_break = log(brk);
                        let offset = at_break - slope * brk;
                        if *inverse {
                            if c[k] <= at_break {
                                (c[k] - offset) / slope
                            } else {
                                lin(c[k])
                            }
                        } else if c[k] <= brk {
                            slope * c[k] + offset
                        } else {
                            log(c[k])
                        }
                    }
                }
            }),
            Op::Cdl {
                slope,
                offset,
                power,
                sat,
                clamp,
                inverse,
            } => cdl(c, *slope, *offset, *power, *sat, *clamp, *inverse),
            Op::Range {
                scale,
                offset,
                lo,
                hi,
            } => c.map(|v| {
                let mut x = v * scale + offset;
                if let Some(lo) = lo {
                    x = x.max(*lo);
                }
                if let Some(hi) = hi {
                    x = x.min(*hi);
                }
                x
            }),
            Op::Lut1(t, inverse) => {
                let f = c.map(|v| v as f32);
                let out = if *inverse { invert1(t, f) } else { t.apply(f) };
                out.map(f64::from)
            }
            Op::Lut3(t, how) => t.apply(c.map(|v| v as f32), *how).map(f64::from),
            Op::AcesSdr(o) => o.apply(c),
            Op::GamutCompress(inverse) => aces::gamut_compress(c, *inverse),
            Op::RgbToHsv(inverse) => {
                if *inverse {
                    hsv_to_rgb(c)
                } else {
                    rgb_to_hsv(c)
                }
            }
            Op::XyzToXyy(inverse) => {
                if *inverse {
                    let [x, y, yy] = c;
                    let d = y.max(1e-10);
                    [x * yy / d, yy, (1.0 - x - y) * yy / d]
                } else {
                    let s = c[0] + c[1] + c[2];
                    let s = if s == 0.0 { 1e-10 } else { s };
                    [c[0] / s, c[1] / s, c[1]]
                }
            }
        }
    }

    /// The step undone, when it can be.
    pub fn inverse(&self) -> Option<Op> {
        Some(match self {
            Op::Matrix(m, o) => {
                let inv = math::inverse(m);
                let back = math::apply(&inv, *o);
                Op::Matrix(inv, back.map(|v| -v))
            }
            Op::Curve(c, to) => Op::Curve(*c, !to),
            Op::Power(e, neg) => Op::Power(e.map(|v| 1.0 / v), *neg),
            Op::Log { base, inverse } => Op::Log {
                base: *base,
                inverse: !inverse,
            },
            Op::LogAffine {
                base,
                log_slope,
                log_offset,
                lin_slope,
                lin_offset,
                camera,
                inverse,
            } => Op::LogAffine {
                base: *base,
                log_slope: *log_slope,
                log_offset: *log_offset,
                lin_slope: *lin_slope,
                lin_offset: *lin_offset,
                camera: *camera,
                inverse: !inverse,
            },
            Op::Cdl {
                slope,
                offset,
                power,
                sat,
                clamp,
                inverse,
            } => Op::Cdl {
                slope: *slope,
                offset: *offset,
                power: *power,
                sat: *sat,
                clamp: *clamp,
                inverse: !inverse,
            },
            Op::Range {
                scale,
                offset,
                lo,
                hi,
            } => Op::Range {
                scale: 1.0 / scale,
                offset: -offset / scale,
                lo: lo.map(|v| (v - offset) / scale),
                hi: hi.map(|v| (v - offset) / scale),
            },
            Op::Lut1(t, inv) => Op::Lut1(Arc::clone(t), !inv),
            Op::GamutCompress(inv) => Op::GamutCompress(!inv),
            Op::RgbToHsv(inv) => Op::RgbToHsv(!inv),
            Op::XyzToXyy(inv) => Op::XyzToXyy(!inv),
            Op::Lut3(..) | Op::AcesSdr(_) => return None,
        })
    }
}

/// ASC CDL (v1.2): slope, offset, power, then saturation.
fn cdl(
    c: [f64; 3],
    slope: [f64; 3],
    offset: [f64; 3],
    power: [f64; 3],
    sat: f64,
    clamp: bool,
    inverse: bool,
) -> [f64; 3] {
    const LUMA: [f64; 3] = [0.2126, 0.7152, 0.0722];
    let saturate = |c: [f64; 3], s: f64| {
        let y = c[0] * LUMA[0] + c[1] * LUMA[1] + c[2] * LUMA[2];
        c.map(|v| y + s * (v - y))
    };
    let lim = |v: f64| if clamp { v.clamp(0.0, 1.0) } else { v };
    if !inverse {
        let v = [0, 1, 2].map(|k| {
            let x = lim(c[k] * slope[k] + offset[k]);
            if x >= 0.0 { x.powf(power[k]) } else { x }
        });
        saturate(v, sat).map(lim)
    } else {
        let v = saturate(c.map(lim), 1.0 / sat).map(lim);
        [0, 1, 2].map(|k| {
            let x = if v[k] >= 0.0 {
                v[k].powf(1.0 / power[k])
            } else {
                v[k]
            };
            (x - offset[k]) / slope[k]
        })
    }
}

/// A 1D table undone: each channel's value found in its (rising) curve.
fn invert1(t: &bv_lut::Table1, c: [f32; 3]) -> [f32; 3] {
    let n = t.entries.len();
    [0, 1, 2].map(|k| {
        let y = c[k];
        let at = |i: usize| t.entries[i][k];
        let x_of = |i: f32| t.min[k] + (t.max[k] - t.min[k]) * i / (n - 1) as f32;
        if y <= at(0) {
            return x_of(0.0);
        }
        if y >= at(n - 1) {
            return x_of((n - 1) as f32);
        }
        let (mut lo, mut hi) = (0, n - 1);
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if at(mid) <= y {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let span = at(hi) - at(lo);
        let f = if span > 0.0 { (y - at(lo)) / span } else { 0.0 };
        x_of(lo as f32 + f)
    })
}

fn rgb_to_hsv(c: [f64; 3]) -> [f64; 3] {
    let [r, g, b] = c;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    let s = if max == 0.0 { 0.0 } else { d / max };
    [h, s, max]
}

fn hsv_to_rgb(c: [f64; 3]) -> [f64; 3] {
    let [h, s, v] = c;
    let h6 = h.rem_euclid(1.0) * 6.0;
    let i = h6.floor();
    let f = h6 - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match i as i32 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near3(a: [f64; 3], b: [f64; 3], by: f64) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() <= by)
    }

    #[test]
    fn every_invertible_step_undoes_itself() {
        let c = [0.31, 0.18, 0.07];
        let ops = [
            Op::Matrix(
                math::rgb_to_rgb(&math::REC709, &math::AP0),
                [0.01, 0.0, -0.02],
            ),
            Op::Curve(Curve::AcesCct, false),
            Op::Power([2.2, 2.4, 2.6], Negatives::Clamp),
            Op::Log {
                base: 2.0,
                inverse: false,
            },
            Op::LogAffine {
                base: 10.0,
                log_slope: [0.24719; 3],
                log_offset: [0.385537; 3],
                lin_slope: [5.555556; 3],
                lin_offset: [0.052272; 3],
                camera: Some(Camera {
                    lin_break: [0.010591; 3],
                    linear_slope: None,
                }),
                inverse: false,
            },
            Op::Cdl {
                slope: [1.1, 0.9, 1.0],
                offset: [0.01, 0.0, -0.01],
                power: [1.2, 1.0, 0.9],
                sat: 0.8,
                clamp: true,
                inverse: false,
            },
            Op::Range {
                scale: 2.0,
                offset: 0.1,
                lo: None,
                hi: None,
            },
            Op::GamutCompress(false),
            Op::RgbToHsv(false),
            Op::XyzToXyy(false),
        ];
        for op in ops {
            let back = op.inverse().unwrap().apply(op.apply(c));
            assert!(near3(back, c, 1e-9), "{op:?}: {back:?}");
        }
    }

    #[test]
    fn log_camera_matches_logc3() {
        // LogC3 (EI 800) written as a LogCamera: the curve's own values.
        let op = Op::LogAffine {
            base: 10.0,
            log_slope: [0.24719; 3],
            log_offset: [0.385537; 3],
            lin_slope: [5.555556; 3],
            lin_offset: [0.052272; 3],
            camera: Some(Camera {
                lin_break: [0.010591; 3],
                linear_slope: Some([5.367655; 3]),
            }),
            inverse: false,
        };
        for x in [0.0, 0.005, 0.18, 1.0] {
            let want = Curve::ArriLogC3Ei800.from_linear(x);
            let got = op.apply([x; 3])[0];
            assert!((got - want).abs() < 2e-5, "{x}: {got} vs {want}");
        }
    }
}
