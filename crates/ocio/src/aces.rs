//! ACES 1.3's reference rendering (RRT) and standard-dynamic-range output
//! transforms (ODTs), its Looks (Blue Light Artifact Fix, Reference Gamut
//! Compression), ported from the Academy's CTL reference (aces-dev v1.3:
//! ACESlib.RRT_Common, ACESlib.Tonescales, ACESlib.ODT_Common, the
//! Rec.709, sRGB and P3 ODTs and the LMTs).
//!
//! Copyright © 2015 Academy of Motion Picture Arts and Sciences
//! (A.M.P.A.S.). Portions contributed by others as indicated. All rights
//! reserved. Used under the ACES license (`licenses/ACES.md`), whose
//! conditions and Disclaimer of Warranty go with BananaV.
//!
//! OpenColorIO splits an output transform at the display connection
//! space: ACES2065-1 → CIE XYZ (D65) display-linear (1 = the display's
//! white), then the display's encoding. [`output`] is the first half.

use std::sync::OnceLock;

use crate::math::{self, AP0, AP1, M3};

const HALF_MIN: f64 = 5.960_464_477_539_063e-8;
const HALF_MAX: f64 = 65504.0;

/// ACES's fixed matrices, worked out once.
struct Matrices {
    ap0_to_ap1: M3,
    ap1_to_ap0: M3,
    ap1_to_xyz: M3,
    xyz_to_ap1: M3,
    /// The RRT's global desaturation and the video ODTs'.
    rrt_sat: M3,
    odt_sat: M3,
}

fn matrices() -> &'static Matrices {
    static M: OnceLock<Matrices> = OnceLock::new();
    M.get_or_init(|| {
        let ap1_to_xyz = math::rgb_to_xyz(&AP1);
        // AP1's luminance weights: the Y row.
        let y = ap1_to_xyz[1];
        Matrices {
            ap0_to_ap1: math::rgb_to_rgb(&AP0, &AP1),
            ap1_to_ap0: math::rgb_to_rgb(&AP1, &AP0),
            ap1_to_xyz,
            xyz_to_ap1: math::inverse(&ap1_to_xyz),
            rrt_sat: sat_matrix(0.96, y),
            odt_sat: sat_matrix(0.93, y),
        }
    })
}

/// A saturation matrix about `rgb2y`'s luminance (`calc_sat_adjust_matrix`).
fn sat_matrix(sat: f64, rgb2y: [f64; 3]) -> M3 {
    let mut m = [[0.0; 3]; 3];
    for (i, row) in m.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = (1.0 - sat) * rgb2y[j] + if i == j { sat } else { 0.0 };
        }
    }
    m
}

fn rgb_2_saturation(rgb: [f64; 3]) -> f64 {
    const TINY: f64 = 1e-10;
    let max = rgb[0].max(rgb[1]).max(rgb[2]);
    let min = rgb[0].min(rgb[1]).min(rgb[2]);
    (max.max(TINY) - min.max(TINY)) / max.max(1e-2)
}

fn rgb_2_yc(rgb: [f64; 3]) -> f64 {
    let [r, g, b] = rgb;
    let chroma = (b * (b - g) + g * (g - r) + r * (r - b)).max(0.0).sqrt();
    (b + g + r + 1.75 * chroma) / 3.0
}

/// The hue angle (degrees, 0–360; NaN for neutrals).
fn rgb_2_hue(rgb: [f64; 3]) -> f64 {
    if rgb[0] == rgb[1] && rgb[1] == rgb[2] {
        return f64::NAN;
    }
    let h = (3f64.sqrt() * (rgb[1] - rgb[2]))
        .atan2(2.0 * rgb[0] - rgb[1] - rgb[2])
        .to_degrees();
    if h < 0.0 { h + 360.0 } else { h }
}

fn sigmoid_shaper(x: f64) -> f64 {
    let t = (1.0 - (x / 2.0).abs()).max(0.0);
    let y = 1.0 + x.signum() * (1.0 - t * t);
    y / 2.0
}

fn glow_fwd(yc: f64, gain: f64, mid: f64) -> f64 {
    if yc <= 2.0 / 3.0 * mid {
        gain
    } else if yc >= 2.0 * mid {
        0.0
    } else {
        gain * (mid / yc - 0.5)
    }
}

fn cubic_basis_shaper(x: f64, w: f64) -> f64 {
    const M: [[f64; 4]; 4] = [
        [-1.0 / 6.0, 3.0 / 6.0, -3.0 / 6.0, 1.0 / 6.0],
        [3.0 / 6.0, -6.0 / 6.0, 3.0 / 6.0, 0.0],
        [-3.0 / 6.0, 0.0, 3.0 / 6.0, 0.0],
        [1.0 / 6.0, 4.0 / 6.0, 1.0 / 6.0, 0.0],
    ];
    let knots = [-w / 2.0, -w / 4.0, 0.0, w / 4.0, w / 2.0];
    if !(x > knots[0] && x < knots[4]) {
        return 0.0;
    }
    let coord = (x - knots[0]) * 4.0 / w;
    let j = coord as usize;
    let t = coord - j as f64;
    let mono = [t * t * t, t * t, t, 1.0];
    let col = match j {
        3 => 0,
        2 => 1,
        1 => 2,
        0 => 3,
        _ => return 0.0,
    };
    let y: f64 = (0..4).map(|k| mono[k] * M[k][col]).sum();
    y * 1.5
}

fn center_hue(hue: f64, center: f64) -> f64 {
    let h = hue - center;
    if h < -180.0 {
        h + 360.0
    } else if h > 180.0 {
        h - 360.0
    } else {
        h
    }
}

/// The RRT's "sweeteners": glow, red modifier, the move to AP1 and its
/// global desaturation (`rrt_sweeteners`): ACES2065-1 → AP1.
pub fn rrt_sweeteners(aces: [f64; 3]) -> [f64; 3] {
    const GLOW_GAIN: f64 = 0.05;
    const GLOW_MID: f64 = 0.08;
    const RED_SCALE: f64 = 0.82;
    const RED_PIVOT: f64 = 0.03;
    const RED_HUE: f64 = 0.0;
    const RED_WIDTH: f64 = 135.0;
    let mut a = aces;
    let saturation = rgb_2_saturation(a);
    let yc = rgb_2_yc(a);
    let s = sigmoid_shaper((saturation - 0.4) / 0.2);
    let glow = 1.0 + glow_fwd(yc, GLOW_GAIN * s, GLOW_MID);
    a = a.map(|v| v * glow);
    let hue = rgb_2_hue(a);
    let weight = if hue.is_nan() {
        0.0
    } else {
        cubic_basis_shaper(center_hue(hue, RED_HUE), RED_WIDTH)
    };
    a[0] += weight * saturation * (RED_PIVOT - a[0]) * (1.0 - RED_SCALE);
    let a = a.map(|v| v.clamp(0.0, f64::INFINITY));
    let m = matrices();
    let pre = math::apply(&m.ap0_to_ap1, a).map(|v| v.clamp(0.0, HALF_MAX));
    math::apply(&m.rrt_sat, pre)
}

/// A segmented spline's parameters (`SegmentedSplineParams_c5`/`_c9`).
#[derive(Clone, Debug)]
pub struct Spline {
    pub low: Vec<f64>,
    pub high: Vec<f64>,
    pub min: [f64; 2],
    pub mid: [f64; 2],
    pub max: [f64; 2],
    pub slope_low: f64,
    pub slope_high: f64,
    /// log10 of min.x, mid.x, max.x, min.y and max.y.
    logs: [f64; 5],
}

const BASIS: [[f64; 3]; 3] = [[0.5, -1.0, 0.5], [-1.0, 1.0, 0.5], [0.5, 0.0, 0.0]];

impl Spline {
    fn new(
        low: Vec<f64>,
        high: Vec<f64>,
        [min, mid, max]: [[f64; 2]; 3],
        slope_low: f64,
        slope_high: f64,
    ) -> Spline {
        Spline {
            logs: [min[0], mid[0], max[0], min[1], max[1]].map(f64::log10),
            low,
            high,
            min,
            mid,
            max,
            slope_low,
            slope_high,
        }
    }

    /// The RRT's tone scale.
    pub fn rrt() -> Spline {
        Spline::new(
            vec![
                -4.0,
                -4.0,
                -3.1573765773,
                -0.4852499958,
                1.8477324706,
                1.8477324706,
            ],
            vec![-0.7185482425, 2.0810307172, 3.6681241237, 4.0, 4.0, 4.0],
            [
                [0.18 * 2f64.powi(-15), 0.0001],
                [0.18, 4.8],
                [0.18 * 2f64.powi(18), 10000.0],
            ],
            0.0,
            0.0,
        )
    }

    /// The 48 cd/m² ODTs' tone scale.
    pub fn odt_48nits() -> Spline {
        let rrt = Spline::rrt();
        Spline::new(
            vec![
                -1.6989700043,
                -1.6989700043,
                -1.4779,
                -1.2291,
                -0.8648,
                -0.448,
                0.00518,
                0.4511080334,
                0.9113744414,
                0.9113744414,
            ],
            vec![
                0.5154386965,
                0.8470437783,
                1.1358,
                1.3802,
                1.5197,
                1.5985,
                1.6467,
                1.6746091357,
                1.687873339,
                1.687873339,
            ],
            [
                [rrt.fwd(0.18 * 2f64.powf(-6.5)), 0.02],
                [rrt.fwd(0.18), 4.8],
                [rrt.fwd(0.18 * 2f64.powf(6.5)), 48.0],
            ],
            0.0,
            0.04,
        )
    }

    /// `segmented_spline_c5_fwd` / `_c9_fwd`.
    pub fn fwd(&self, x: f64) -> f64 {
        let knots = self.low.len() - 2;
        let logx = x.max(HALF_MIN).log10();
        let [lmin, lmid, lmax, ymin, ymax] = self.logs;
        let logy = if logx <= lmin {
            logx * self.slope_low + (ymin - self.slope_low * lmin)
        } else if logx < lmid {
            let coord = (knots - 1) as f64 * (logx - lmin) / (lmid - lmin);
            Self::segment(&self.low, coord)
        } else if logx < lmax {
            let coord = (knots - 1) as f64 * (logx - lmid) / (lmax - lmid);
            Self::segment(&self.high, coord)
        } else {
            logx * self.slope_high + (ymax - self.slope_high * lmax)
        };
        10f64.powf(logy)
    }

    /// For the graphics card (`aces_spline` in `bv-gpu`'s shaders): its
    /// points' logs, slopes and knots, then each coefficient in an x.
    fn gpu_constants(&self, out: &mut Vec<[f32; 4]>) {
        let [lmin, lmid, lmax, ymin, ymax] = self.logs.map(|v| v as f32);
        let knots = (self.low.len() - 2) as f32;
        out.push([lmin, lmid, lmax, ymin]);
        out.push([ymax, self.slope_low as f32, self.slope_high as f32, knots]);
        for &c in self.low.iter().chain(&self.high) {
            out.push([c as f32, 0.0, 0.0, 0.0]);
        }
    }

    fn segment(coefs: &[f64], coord: f64) -> f64 {
        let j = coord as usize;
        let t = coord - j as f64;
        let cf = [coefs[j], coefs[j + 1], coefs[j + 2]];
        let mono = [t * t, t, 1.0];
        // monomials · (cf · M)
        let basis: [f64; 3] = [0, 1, 2].map(|c| (0..3).map(|k| cf[k] * BASIS[k][c]).sum());
        (0..3).map(|k| mono[k] * basis[k]).sum()
    }

    /// `segmented_spline_c5_rev` / `_c9_rev`.
    pub fn rev(&self, y: f64) -> f64 {
        let knots = self.low.len() - 2;
        let [lmin, lmid, lmax, ..] = self.logs;
        let inc_low = (lmid - lmin) / (knots - 1) as f64;
        let inc_high = (lmax - lmid) / (knots - 1) as f64;
        let knot_y =
            |c: &[f64]| -> Vec<f64> { (0..knots).map(|i| (c[i] + c[i + 1]) / 2.0).collect() };
        let (ky_low, ky_high) = (knot_y(&self.low), knot_y(&self.high));
        let logy = y.max(1e-10).log10();
        let solve = |coefs: &[f64], ky: &[f64], start: f64, inc: f64| -> f64 {
            let mut j = 0;
            for k in 0..knots - 1 {
                if logy > ky[k] && logy <= ky[k + 1] {
                    j = k;
                    break;
                }
            }
            let cf = [coefs[j], coefs[j + 1], coefs[j + 2]];
            let tmp: [f64; 3] = [0, 1, 2].map(|c| (0..3).map(|k| cf[k] * BASIS[k][c]).sum());
            let (a, b, c) = (tmp[0], tmp[1], tmp[2] - logy);
            let d = (b * b - 4.0 * a * c).max(0.0).sqrt();
            let t = (2.0 * c) / (-d - b);
            start + (t + j as f64) * inc
        };
        let logx = if logy <= self.min[1].log10() {
            lmin
        } else if logy <= self.mid[1].log10() {
            solve(&self.low, &ky_low, lmin, inc_low)
        } else if logy < self.max[1].log10() {
            solve(&self.high, &ky_high, lmid, inc_high)
        } else {
            lmax
        };
        10f64.powf(logx)
    }
}

const CINEMA_WHITE: f64 = 48.0;
/// `pow10(log10(0.02))`, exactly as the 48 cd/m² tone scale returns it.
fn cinema_black() -> f64 {
    10f64.powf(0.02f64.log10())
}

const DIM_SURROUND_GAMMA: f64 = 0.9811;

fn dark_to_dim(lin: [f64; 3]) -> [f64; 3] {
    let m = matrices();
    let xyz = math::apply(&m.ap1_to_xyz, lin);
    let sum = xyz[0] + xyz[1] + xyz[2];
    let div = if sum == 0.0 { 1e-10 } else { sum };
    let (x, y) = (xyz[0] / div, xyz[1] / div);
    let yy = xyz[1].clamp(0.0, f64::INFINITY).powf(DIM_SURROUND_GAMMA);
    let back = [x * yy / y.max(1e-10), yy, (1.0 - x - y) * yy / y.max(1e-10)];
    math::apply(&m.xyz_to_ap1, back)
}

/// How an SDR output transform ends: for video (dim surround) or cinema
/// (dark surround).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surround {
    Video,
    Cinema,
}

/// ACES2065-1 → CIE XYZ (adapted to `white`), display-linear 0–1: the
/// RRT and an SDR ODT up to its display primaries (ACES 1.0's
/// SDR-VIDEO_1.0 / SDR-CINEMA_1.0).
#[derive(Clone, Debug)]
pub struct SdrOutput {
    rrt: Spline,
    odt: Spline,
    surround: Surround,
    /// AP1 → XYZ, then to the observer's white.
    to_xyz: M3,
    black: f64,
}

impl SdrOutput {
    pub fn new(surround: Surround, white: [f64; 2]) -> SdrOutput {
        SdrOutput {
            rrt: Spline::rrt(),
            odt: Spline::odt_48nits(),
            surround,
            to_xyz: math::mul(
                &math::bradford(math::ACES_WHITE, white),
                &matrices().ap1_to_xyz,
            ),
            black: cinema_black(),
        }
    }

    pub fn apply(&self, aces: [f64; 3]) -> [f64; 3] {
        let m = matrices();
        // RRT: sweeteners, its tone scale, back to ACES (OCES)…
        let pre = rrt_sweeteners(aces);
        let post = pre.map(|v| self.rrt.fwd(v));
        let oces = math::apply(&m.ap1_to_ap0, post);
        // …then the ODT.
        let pre = math::apply(&m.ap0_to_ap1, oces);
        let post = pre.map(|v| self.odt.fwd(v));
        let black = self.black;
        let mut lin = post.map(|y| (y - black) / (CINEMA_WHITE - black));
        if self.surround == Surround::Video {
            lin = dark_to_dim(lin);
            lin = math::apply(&m.odt_sat, lin);
        }
        math::apply(&self.to_xyz, lin)
    }

    /// What `bv-gpu`'s `aces_output` kernel reads, in its order: the
    /// matrices' rows (AP0 → AP1, AP1 → AP0, the RRT's and the ODT's
    /// desaturations, AP1 → XYZ and back, the output's AP1 → XYZ), then
    /// black, white, the dim surround's gamma and whether it applies, then
    /// the RRT's tone scale and the ODT's.
    pub fn gpu_constants(&self) -> Vec<[f32; 4]> {
        let m = matrices();
        let mut out = Vec::with_capacity(58);
        for mat in [
            &m.ap0_to_ap1,
            &m.ap1_to_ap0,
            &m.rrt_sat,
            &m.odt_sat,
            &m.ap1_to_xyz,
            &m.xyz_to_ap1,
            &self.to_xyz,
        ] {
            for row in mat {
                out.push([row[0] as f32, row[1] as f32, row[2] as f32, 0.0]);
            }
        }
        out.push([
            self.black as f32,
            CINEMA_WHITE as f32,
            DIM_SURROUND_GAMMA as f32,
            if self.surround == Surround::Video {
                1.0
            } else {
                0.0
            },
        ]);
        self.rrt.gpu_constants(&mut out);
        self.odt.gpu_constants(&mut out);
        out
    }
}

/// The matrices the Reference Gamut Compression moves through (AP0 → AP1
/// and back), for the graphics card.
pub fn gamut_compress_matrices() -> [M3; 2] {
    let m = matrices();
    [m.ap0_to_ap1, m.ap1_to_ap0]
}

/// The Blue Light Artifact Fix Look (ACES 1.1), on ACES2065-1.
pub fn blue_light_fix() -> M3 {
    // CTL multiplies row vectors: the matrix as written, transposed.
    math::transpose(&[
        [0.9404372683, 0.0083786969, 0.0005471261],
        [-0.0183068787, 0.8286599939, -0.0008833746],
        [0.0778696104, 0.1629613092, 1.0003362486],
    ])
}

/// ACES 1.3's Reference Gamut Compression, on ACES2065-1 (`invert`: its
/// inverse).
pub fn gamut_compress(aces: [f64; 3], invert: bool) -> [f64; 3] {
    const LIM: [f64; 3] = [1.147, 1.264, 1.312];
    const THR: [f64; 3] = [0.815, 0.803, 0.880];
    const PWR: f64 = 1.2;
    let compress = |dist: f64, lim: f64, thr: f64| -> f64 {
        if dist < thr {
            return dist;
        }
        let scl = (lim - thr) / (((1.0 - thr) / (lim - thr)).powf(-PWR) - 1.0).powf(1.0 / PWR);
        let nd = (dist - thr) / scl;
        let p = nd.powf(PWR);
        if !invert {
            thr + scl * nd / (1.0 + p).powf(1.0 / PWR)
        } else if dist > thr + scl {
            dist
        } else {
            thr + scl * (-(p / (p - 1.0))).powf(1.0 / PWR)
        }
    };
    let m = matrices();
    let lin = math::apply(&m.ap0_to_ap1, aces);
    let ach = lin[0].max(lin[1]).max(lin[2]);
    let dist = if ach == 0.0 {
        [0.0; 3]
    } else {
        lin.map(|v| (ach - v) / ach.abs())
    };
    let c = [0, 1, 2].map(|k| compress(dist[k], LIM[k], THR[k]));
    let out = c.map(|d| ach - d * ach.abs());
    math::apply(&m.ap1_to_ap0, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64, by: f64) -> bool {
        (a - b).abs() <= by
    }

    #[test]
    fn the_tone_scales_go_through_their_points() {
        let rrt = Spline::rrt();
        // 18% grey to 4.8 cd/m² through the RRT.
        assert!(near(rrt.fwd(0.18), 4.8, 1e-6), "{}", rrt.fwd(0.18));
        let odt = Spline::odt_48nits();
        assert!(near(odt.fwd(odt.mid[0]), 4.8, 1e-6));
        assert!(near(odt.fwd(odt.max[0]), 48.0, 1e-6));
        // Their reverses undo them.
        for x in [0.001, 0.01, 0.18, 1.0, 10.0, 100.0] {
            let y = rrt.fwd(x);
            assert!(near(rrt.rev(y) / x, 1.0, 1e-6), "rrt {x}: {}", rrt.rev(y));
        }
        for x in [0.1, 1.0, 4.8, 20.0, 200.0] {
            let y = odt.fwd(x);
            assert!(near(odt.rev(y) / x, 1.0, 1e-5), "odt {x}");
        }
    }

    #[test]
    fn the_sdr_video_output_maps_greys_as_acess() {
        let out = SdrOutput::new(Surround::Video, math::D65);
        // Black to black, middle grey to about 10% of the display's white
        // (4.8 of 48 cd/m², through the dim surround's gamma), a very
        // bright grey to white.
        let y = |v: f64| out.apply([v; 3])[1];
        assert!(y(0.0).abs() < 1e-4, "{}", y(0.0));
        let mid = y(0.18);
        assert!(mid > 0.09 && mid < 0.11, "{mid}");
        assert!(y(16.0) > 0.98, "{}", y(16.0));
        // Greys stay grey at D65 (XYZ's x, y).
        let xyz = out.apply([0.18; 3]);
        let s = xyz[0] + xyz[1] + xyz[2];
        assert!(
            near(xyz[0] / s, 0.3127, 1e-3) && near(xyz[1] / s, 0.329, 1e-3),
            "{xyz:?}"
        );
    }

    #[test]
    fn the_looks() {
        // Blue Light Artifact Fix keeps greys.
        let g = math::apply(&blue_light_fix(), [0.5; 3]);
        assert!(g.iter().all(|v| near(*v, 0.5, 1e-6)), "{g:?}");
        // Gamut compression leaves colors inside AP1's core alone, brings
        // one far outside in, and its inverse undoes it.
        let inside = [0.2, 0.18, 0.15];
        let out = gamut_compress(inside, false);
        assert!(
            inside.iter().zip(out).all(|(a, b)| near(*a, b, 1e-9)),
            "{out:?}"
        );
        let outside = math::apply(&math::rgb_to_rgb(&AP1, &AP0), [0.5, -0.1, 0.05]);
        let c = gamut_compress(outside, false);
        let c1 = math::apply(&matrices().ap0_to_ap1, c);
        assert!(c1[1] > -0.1, "{c1:?}");
        let back = gamut_compress(c, true);
        assert!(
            back.iter().zip(outside).all(|(a, b)| near(*a, b, 1e-6)),
            "{back:?}"
        );
    }
}
