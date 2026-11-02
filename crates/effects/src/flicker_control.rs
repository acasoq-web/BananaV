//! Flicker Control, as VEGAS's: Reduction takes a flicker away — Temporal
//! blend mixes each frame with the frames of its analysis window (read
//! along the motion data's corners with motion compensation), Histogram
//! smoothing matches each color channel of a frame to the mean of the
//! window's histograms (the histograms Calculate counts) — and Addition
//! adds a flicker of a given frequency, or a random one. Type keeps it to
//! the highlights, midtones or shadows; Show mask only shows how much it
//! changed.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};
use std::sync::{Arc, Mutex};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::kit::{Buf, Spec, Vals, define};
use crate::ofx::{Fx, Props, is, suites};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.FlickerControl";

const MODE: &CStr = c"mode";
const FREQUENCY: &CStr = c"frequency";
const REDUCTION: &CStr = c"reductionMode";
const WINDOW: &CStr = c"window";
const SENSITIVITY: &CStr = c"sensitivity";
const THRESHOLD: &CStr = c"threshold";
const TYPE: &CStr = c"type";
const MOTION: &CStr = c"motion";
/// The motion data's corners, as parts of the frame: top left, top right,
/// bottom right, bottom left.
pub(crate) const CORNERS: [&CStr; 4] = [c"cornerTL", c"cornerTR", c"cornerBR", c"cornerBL"];
const CALCULATE: &CStr = c"calculate";
const RESET: &CStr = c"reset";
/// The histograms Calculate counted ([`histograms_text`]), hidden.
const HISTOGRAMS: &CStr = c"histograms";
const MASK: &CStr = c"mask";

const MODES: &[&CStr] = &[c"Reduction", c"Addition"];
const REDUCTIONS: &[&CStr] = &[c"Temporal blend", c"Histogram smoothing"];
const TYPES: &[&CStr] = &[c"Highlights", c"Midtones", c"Shadow", c"All"];

/// What may be typed where VEGAS takes any number.
const ANY: [f64; 2] = [-1.0e9, 1.0e9];

/// The settings, in VEGAS's order (Calculate and Reset after the corners).
const SPECS: &[Spec] = &[
    Spec::Choice(MODE, c"Mode", MODES, 0, c"Controls the modes."),
    Spec::Ranged(
        FREQUENCY,
        c"Flicker Frequency",
        55.0,
        ANY,
        [0.0, 120.0],
        c"How many times a second the light pulses, in hertz; 1 and below: a random flicker.",
    ),
    Spec::Choice(
        REDUCTION,
        c"Reduction Mode",
        REDUCTIONS,
        0,
        c"Controls the reduction modes.",
    ),
    Spec::Int(
        WINDOW,
        c"Analysis Window",
        2,
        0,
        60,
        c"How many frames around each frame it works with.",
    ),
    Spec::Ranged(
        SENSITIVITY,
        c"Sensitivity",
        1.0,
        ANY,
        [0.0, 1.0],
        c"How much the frames around count: at 1 as much as the frame itself; with Addition, how strong the flicker is.",
    ),
    Spec::Ranged(THRESHOLD, c"Threshold", 0.0, ANY, [0.0, 1.0], c""),
    Spec::Choice(
        TYPE,
        c"Type",
        TYPES,
        3,
        c"The tones it works on: the highlights, the midtones, the shadows or all.",
    ),
    Spec::Toggle(
        MOTION,
        c"Use motion compensation",
        false,
        c"To enable motion compensation, use Motion Tracking panel for generating motion data.",
    ),
    Spec::Pair(CORNERS[0], c"CornerTL", [-1.0; 2], ANY, [0.0, 1.0], c""),
    Spec::Pair(CORNERS[1], c"CornerTR", [-1.0; 2], ANY, [0.0, 1.0], c""),
    Spec::Pair(CORNERS[2], c"CornerBR", [-1.0; 2], ANY, [0.0, 1.0], c""),
    Spec::Pair(CORNERS[3], c"CornerBL", [-1.0; 2], ANY, [0.0, 1.0], c""),
    Spec::Toggle(MASK, c"Show mask only", false, c"Show mask only."),
];

/// The settings at one time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    pub(crate) addition: bool,
    pub(crate) histogram: bool,
    /// Analysis Window: the frames around a frame, 0–60.
    pub(crate) window: i32,
    pub(crate) sensitivity: f32,
    pub(crate) frequency: f64,
    /// Type: highlights, midtones, shadows or all.
    pub(crate) tones: i32,
    pub(crate) motion: bool,
    pub(crate) mask: bool,
}

impl Settings {
    fn of(v: &Vals) -> Settings {
        Settings {
            addition: v.i(MODE) != 0,
            histogram: v.i(REDUCTION) == 1,
            window: v.i(WINDOW).clamp(0, 60),
            sensitivity: v.f(SENSITIVITY) as f32,
            frequency: v.f(FREQUENCY),
            tones: v.i(TYPE).clamp(0, 3),
            motion: v.b(MOTION),
            mask: v.b(MASK),
        }
    }

    fn at(fx: Fx, time: f64) -> Settings {
        Settings::of(&Vals::of(SPECS, &|s| s.read(fx, time)))
    }

    /// The settings the card's plan gets.
    pub(crate) fn of_values(values: crate::accel::Values<'_>) -> Settings {
        Settings::of(&Vals::of(SPECS, &|spec| {
            values
                .nums(spec.name())
                .map_or_else(|| spec.default(), <[f64]>::to_vec)
        }))
    }

    /// The frames before a frame in its window: VEGAS's ⌊n/2⌋ (the rest
    /// after it).
    pub(crate) fn before(&self) -> i32 {
        self.window / 2
    }
}

/// How much of the change a color whose largest channel is `m` gets for
/// Type `tones`: the highlights', midtones' or shadows' share, or all of it.
pub(crate) fn tone(tones: i32, m: f32) -> f32 {
    match tones {
        0 => 1.0 - (1.0 - m) * (1.0 - m),
        1 => 1.0 - (m - 0.5) * (m - 0.5) * 4.0,
        2 => 1.0 - m * m,
        _ => 1.0,
    }
}

/// A color's largest channel.
fn largest(c: [f32; 4]) -> f32 {
    c[1].max(c[2]).max(c[0])
}

/// Show mask only: how much the pixel `c` became `out` — its channels'
/// largest change, a fifth of the range or more white — with `c`'s alpha.
pub(crate) fn masked(c: [f32; 4], out: [f32; 4]) -> [f32; 4] {
    let d = (c[0] - out[0])
        .abs()
        .max((c[1] - out[1]).abs())
        .max((c[2] - out[2]).abs());
    let v = (d * (1.0 / 0.2)).clamp(0.0, 1.0);
    [v, v, v, c[3]]
}

/// `out` (made from `cur`) as Show mask only shows it.
fn mask_of(cur: &Buf, out: Buf) -> Buf {
    cur.map(|x, y| masked(cur.at(x, y), out.at(x, y)))
}

// Addition.

/// A frame's own random numbers, so that it always looks alike (VEGAS's
/// come from one generator, in the order frames are drawn).
struct Random(u64);

impl Random {
    fn of(frame: f64) -> Random {
        Random((frame as i64 as u64) ^ 0x5eed_f11c_4e52_0000)
    }

    /// A number in 0–1 (1 left out).
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^= z >> 31;
        (z >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// How many of `n` tries come out at odds `p`.
    fn binomial(&mut self, n: i32, p: f64) -> i32 {
        (0..n.max(0)).filter(|_| self.next() < p).count() as i32
    }
}

/// The light Addition adds at `frame` of a video `rate` frames a second
/// (VEGAS's: 30 when it has none): a sine of Flicker Frequency hertz a
/// tenth of Sensitivity deep; at 1 hertz and below, random — a light near a
/// tenth of Sensitivity in the seconds a random count (smaller, the more
/// sensitive) divides, nothing in the others.
pub(crate) fn added(s: &Settings, frame: f64, rate: f64) -> f64 {
    let t = frame.floor();
    let fps = if rate >= 1.0 { rate } else { 30.0 };
    if s.frequency > 1.0 {
        return (s.frequency * std::f64::consts::TAU * t / fps).sin()
            * f64::from(s.sensitivity)
            * 0.1;
    }
    let p = f64::from(s.sensitivity).clamp(0.01, 1.0);
    let mut random = Random::of(t);
    let light = random.binomial(1000, p);
    let period = random.binomial((fps / p) as i32, 1.0 - p * 0.9);
    if (t / fps) as i64 % (i64::from(period) + 1) == 0 {
        f64::from(light) * 0.0001
    } else {
        0.0
    }
}

/// Pixel `c` with Addition's light `amount`: as much of it as Type gives
/// its largest channel, times its alpha (at most 1), each channel held
/// within 0–1; next to nothing see-through, nothing.
pub(crate) fn add_pixel(tones: i32, amount: f64, c: [f32; 4]) -> [f32; 4] {
    let a = if c[3] <= 1.0 { c[3] } else { 1.0 };
    if a < f32::EPSILON {
        return [0.0; 4];
    }
    let d = f64::from(tone(tones, largest(c))) * amount * f64::from(a);
    let ch = |v: f32| {
        let v = f64::from(v) + d;
        if v >= 0.0 { v.min(1.0) as f32 } else { 0.0 }
    };
    [ch(c[0]), ch(c[1]), ch(c[2]), a]
}

// Temporal blend.

/// VEGAS's weights of an analysis window of `n` frames, from ⌊n/2⌋ before
/// a frame to n − ⌊n/2⌋ after it: binomial (each factor of a coefficient a
/// whole-number division, as VEGAS works them out; for two frames 0.666
/// and 0.333), drawn toward the frame itself below a Sensitivity `s` of 0.5
/// and toward an even mean above, summing to 1. With the index of the
/// frame itself.
pub(crate) fn weights(n: i32, s: f32) -> (Vec<f32>, usize) {
    let n = n.max(0);
    let center = (n / 2) as usize;
    let binomial: Vec<f32> = if n == 1 {
        vec![0.666, 0.333]
    } else {
        (0..=n)
            .map(|k| {
                let j = if 2 * k <= n { k } else { n - k };
                (1..=j).fold(1.0f32, |f, i| f * ((n - j + i) / i) as f32)
            })
            .collect()
    };
    let even = 1.0 / (n as f32 + 1.0);
    let mut w: Vec<f32> = binomial
        .iter()
        .enumerate()
        .map(|(k, &b)| {
            let (d, base) = if s >= 0.5 {
                let d = even - (b + b);
                (d, even - d)
            } else if k == center {
                let d = b - 2.0;
                (d, b - d * 0.5)
            } else {
                (b, b - b * 0.5)
            };
            d * s + base
        })
        .collect();
    let sum: f32 = w.iter().sum();
    for v in &mut w {
        *v /= sum;
    }
    (w, center)
}

/// A frame the blend takes: its picture, its weight and, with motion
/// compensation, the homography taking the frame's pixels to it.
pub(crate) struct Taken<'a> {
    pub(crate) picture: &'a Buf,
    pub(crate) weight: f32,
    pub(crate) map: Option<[f32; 9]>,
}

/// Where homography `h` takes pixel (`x`, `y`), as VEGAS works it out.
pub(crate) fn mapped(h: &[f32; 9], x: f32, y: f32) -> [f32; 2] {
    let d = 1.0 / (x * h[6] + y * h[7] + h[8]);
    [
        (y * h[1] + x * h[0] + h[2]) * d,
        (x * h[3] + y * h[4] + h[5]) * d,
    ]
}

/// VEGAS's Temporal blend: each pixel the frames' mean, each weighted by
/// its weight and by how much Type gives its largest channel — all of it
/// for Type All (without dividing by what the frames there weigh) and for
/// frame `center`, whose alpha it keeps (none past the frames). A frame
/// read along a map takes its nearest pixel there, or frame `center`'s
/// own past the picture.
pub(crate) fn blend(frames: &[Taken<'_>], center: usize, tones: i32, bounds: [c_int; 4]) -> Buf {
    let Some(first) = frames.first() else {
        return Buf {
            bounds,
            data: vec![
                [0.0; 4];
                (bounds[2] - bounds[0]).max(0) as usize
                    * (bounds[3] - bounds[1]).max(0) as usize
            ],
        };
    };
    let [x1, y1, x2, y2] = bounds.map(|v| v as f32);
    let own = frames.get(center);
    first.picture.map(|x, y| {
        let read = |k: usize, t: &Taken<'_>| match (t.map, own) {
            (Some(h), Some(own)) if k != center => {
                let [mx, my] = mapped(&h, x as f32, y as f32);
                if mx <= x1 - 0.4 || x2 - 0.6 <= mx || my <= y1 - 0.4 || y2 - 0.6 <= my {
                    own.picture.at(x, y)
                } else {
                    t.picture.at(mx.round() as c_int, my.round() as c_int)
                }
            }
            _ => t.picture.at(x, y),
        };
        let mut alpha = 0.0;
        let mut sum = [0.0f32; 3];
        if tones == 3 {
            for (k, t) in frames.iter().enumerate() {
                let c = read(k, t);
                if k == center {
                    alpha = c[3];
                }
                for i in 0..3 {
                    sum[i] += t.weight * c[i];
                }
            }
            return [sum[0], sum[1], sum[2], alpha];
        }
        let mut taken = [[0.0f32; 4]; 64];
        let mut total = 0.0f32;
        for (k, t) in frames.iter().enumerate().take(64) {
            let c = read(k, t);
            if k == center {
                alpha = c[3];
            }
            let share = if k == center {
                1.0
            } else {
                tone(tones, largest(c))
            };
            let w = share * t.weight;
            total += w;
            taken[k] = [c[0], c[1], c[2], w];
        }
        for c in &taken[..frames.len().min(64)] {
            let f = c[3] / total;
            for i in 0..3 {
                sum[i] += f * c[i];
            }
        }
        [sum[0], sum[1], sum[2], alpha]
    })
}

/// The homography taking corners `from` to corners `to` (pixels), its last
/// entry 1; `None` when they span no quadrilateral.
pub(crate) fn homography(from: [[f64; 2]; 4], to: [[f64; 2]; 4]) -> Option<[f32; 9]> {
    let mut m = [[0.0f64; 9]; 8];
    for i in 0..4 {
        let ([x, y], [u, v]) = (from[i], to[i]);
        m[2 * i] = [x, y, 1.0, 0.0, 0.0, 0.0, -x * u, -y * u, u];
        m[2 * i + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -x * v, -y * v, v];
    }
    for col in 0..8 {
        let pivot = (col..8).max_by(|&a, &b| m[a][col].abs().total_cmp(&m[b][col].abs()))?;
        if m[pivot][col].abs() < 1e-9 {
            return None;
        }
        m.swap(col, pivot);
        let row = m[col];
        for (r, other) in m.iter_mut().enumerate() {
            if r != col {
                let f = other[col] / row[col];
                for (v, p) in other.iter_mut().zip(&row).skip(col) {
                    *v -= f * p;
                }
            }
        }
    }
    let h: [f64; 8] = std::array::from_fn(|i| m[i][8] / m[i][i]);
    h.iter()
        .all(|v| v.is_finite())
        .then(|| [h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7], 1.0].map(|v| v as f32))
}

/// The motion data's corners at a time in pixels of a picture whose render
/// window ends at (`w`, `h`), from their parts of the frame.
pub(crate) fn corners_in(parts: [[f64; 2]; 4], (w, h): (c_int, c_int)) -> [[f64; 2]; 4] {
    parts.map(|[x, y]| {
        [
            f64::from(x as f32 * w as f32),
            f64::from(y as f32 * h as f32),
        ]
    })
}

/// The maps of the frames the blend takes, `corners` theirs: from frame
/// `center`'s corners to each frame's (frame `center` itself: none).
pub(crate) fn maps(corners: &[[[f64; 2]; 4]], center: usize) -> Vec<Option<[f32; 9]>> {
    let identity = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    corners
        .iter()
        .enumerate()
        .map(|(k, c)| match corners.get(center) {
            Some(own) if k != center => Some(homography(*own, *c).unwrap_or(identity)),
            _ => None,
        })
        .collect()
}

/// Whether the clip has frame `t`: inside its frame range, half a frame
/// either way (VEGAS's: the others are left out).
pub(crate) fn has_frame([start, end]: [f64; 2], t: f64) -> bool {
    start - 0.5 < t && t < end + 0.5
}

// Histogram smoothing.

/// The levels a channel's histogram counts (VEGAS's for float pictures).
pub const LEVELS: usize = 1024;

/// The level a channel's value `v` is counted in: VEGAS's — the value
/// times the levels, rounded, within them.
pub fn level(v: f32) -> usize {
    ((v * LEVELS as f32 + 0.5) as i32).clamp(0, LEVELS as i32 - 1) as usize
}

/// The frames' color histograms as the host writes them for Calculate
/// (BananaV's `kBananaVParamPropAnalysis` "ColorHistograms"): a first line
/// `bvcolors 1 <levels>`, then a line a frame — its time (the effect's, in
/// frames), then the counts of its red, green and blue levels.
pub fn histograms_text(frames: &[(f64, [Vec<u64>; 3])]) -> String {
    let mut text = format!("bvcolors 1 {LEVELS}\n");
    for (time, counts) in frames {
        text.push_str(&time.to_string());
        for n in counts.iter().flatten() {
            text.push(' ');
            text.push_str(&n.to_string());
        }
        text.push('\n');
    }
    text
}

/// A frame's cumulative share of its pixels at each level, a channel
/// after another.
type Cdfs = [Vec<f32>; 3];

/// The frames [`histograms_text`] wrote, each with its channels'
/// cumulative shares (VEGAS's: summed, then over the last).
fn parse_histograms(text: &str) -> Option<Vec<(f64, Cdfs)>> {
    let mut lines = text.lines();
    let head: Vec<&str> = lines.next()?.split_whitespace().collect();
    if head.first() != Some(&"bvcolors") || head.get(1) != Some(&"1") {
        return None;
    }
    let levels: usize = head.get(2)?.parse().ok()?;
    if levels != LEVELS {
        return None;
    }
    lines
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let mut words = l.split_whitespace();
            let time: f64 = words.next()?.parse().ok()?;
            let counts: Vec<f32> = words
                .map(|n| n.parse::<u64>().ok().map(|n| n as f32))
                .collect::<Option<_>>()?;
            if counts.len() != 3 * LEVELS {
                return None;
            }
            let cdfs = std::array::from_fn(|c| {
                let mut sum = 0.0f32;
                let mut cdf: Vec<f32> = counts[c * LEVELS..(c + 1) * LEVELS]
                    .iter()
                    .map(|&n| {
                        sum += n;
                        sum
                    })
                    .collect();
                let k = 1.0 / sum;
                for v in &mut cdf {
                    *v *= k;
                }
                cdf
            });
            Some((time, cdfs))
        })
        .collect()
}

/// A quick fingerprint of a long text.
fn fingerprint(text: &str) -> u64 {
    let mut h = text.len() as u64 ^ 0xcbf2_9ce4_8422_2325;
    for chunk in text.as_bytes().chunks(8) {
        let mut b = [0u8; 8];
        b[..chunk.len()].copy_from_slice(chunk);
        h = (h ^ u64::from_le_bytes(b)).wrapping_mul(0x0100_0000_01b3);
        h ^= h >> 29;
    }
    h
}

/// Each frame's time and cumulative shares.
type Counted = Arc<Vec<(f64, Cdfs)>>;

/// The last histograms parsed: a project's are long, and every frame reads
/// them.
static PARSED: Mutex<Option<(u64, Counted)>> = Mutex::new(None);

fn histograms(text: &str) -> Option<Counted> {
    if text.trim().is_empty() {
        return None;
    }
    let key = fingerprint(text);
    if let Ok(p) = PARSED.lock()
        && let Some((k, frames)) = p.as_ref()
        && *k == key
    {
        return Some(frames.clone());
    }
    let frames = Arc::new(parse_histograms(text)?);
    if let Ok(mut p) = PARSED.lock() {
        *p = Some((key, frames.clone()));
    }
    Some(frames)
}

/// VEGAS's smoothed share of frame `i`, one channel (`cdfs` each frame's),
/// over the `half` frames on either side (reflected at the clip's ends):
/// the shares' harmonic mean — below a Sensitivity `s` of 0.99, weighted by
/// a bell `half` frames wide at 1 (its first level 0, its last 1). `None`
/// with fewer frames than the window (VEGAS keeps none then).
fn smoothed(cdfs: &[&[f32]], i: usize, half: usize, s: f32) -> Option<Vec<f32>> {
    let n = cdfs.len();
    let size = 2 * half + 1;
    if n < size || i >= n {
        return None;
    }
    let frame = |k: usize| {
        let j = (i + k) as isize - half as isize;
        let j = if j < 0 {
            -j
        } else if j >= n as isize {
            2 * (n as isize - 1) - j
        } else {
            j
        };
        cdfs[j.clamp(0, n as isize - 1) as usize]
    };
    if s > 0.99 {
        return Some(
            (0..LEVELS)
                .map(|b| {
                    let sum = (0..size).fold(0.0f32, |sum, k| 1.0 / frame(k)[b] + sum);
                    size as f32 / sum
                })
                .collect(),
        );
    }
    let s = if s >= f32::EPSILON {
        s.min(1.0)
    } else {
        f32::EPSILON
    };
    let wide = (half * half) as f32 * s;
    let bell = |k: usize| {
        let d = k as f32 - half as f32;
        ((-d * d) / wide).exp()
    };
    let whole = (0..size).fold(0.0f32, |sum, k| sum + bell(k) / frame(k)[LEVELS - 1]);
    let mut out = vec![0.0f32; LEVELS];
    out[LEVELS - 1] = 1.0;
    for b in (1..LEVELS - 1).rev() {
        let sum = (0..size).fold(0.0f32, |sum, k| sum + bell(k) / frame(k)[b]);
        out[b] = whole / sum;
    }
    Some(out)
}

/// VEGAS's histogram match: each level to the first level whose smoothed
/// share reaches the frame's own there (past them all: the levels' count).
fn matched(own: &[f32], smooth: &[f32]) -> Vec<f32> {
    own.iter()
        .map(|&v| smooth.iter().position(|&t| v <= t).unwrap_or(LEVELS) as f32)
        .collect()
}

/// The levels Type maps (the others are left): the highlights the top
/// third of them, the midtones the middle one, the shadows the bottom one,
/// or all.
pub(crate) fn mapped_levels(tones: i32) -> [i32; 2] {
    match tones {
        0 => [688, 1024],
        1 => [342, 688],
        2 => [0, 342],
        _ => [0, LEVELS as i32],
    }
}

/// Histogram smoothing's table at `time`, from the histograms Calculate
/// counted (`text`): each level of each channel, the level it is matched
/// to (a channel after another); `None` without them, or with fewer frames
/// than the window.
pub(crate) fn levels_table(s: &Settings, text: &str, time: f64) -> Option<Vec<f32>> {
    let frames = histograms(text)?;
    // The frame's histograms: the last counted at or before it (VEGAS keys
    // them a frame apart).
    let i = frames
        .iter()
        .rposition(|(t, _)| *t <= time + 1e-6)
        .unwrap_or(0);
    let half = s.before() as usize;
    let mut table = Vec::with_capacity(3 * LEVELS);
    for c in 0..3 {
        let cdfs: Vec<&[f32]> = frames.iter().map(|(_, f)| f[c].as_slice()).collect();
        let smooth = smoothed(&cdfs, i, half, s.sensitivity)?;
        table.extend(matched(cdfs[i], &smooth));
    }
    Some(table)
}

/// Pixel `c` through Histogram smoothing's `table`: each channel whose
/// level (its value times the levels, cut down) is one of `range`, the one
/// it is matched to.
pub(crate) fn match_pixel(table: &[f32], [lo, hi]: [i32; 2], c: [f32; 4]) -> [f32; 4] {
    let ch = |k: usize| {
        let v = c[k];
        let x = v * LEVELS as f32;
        if x.is_nan() || x <= -1.0 || x >= LEVELS as f32 {
            return v;
        }
        let i = x as i32;
        if i >= lo && i < hi {
            table[k * LEVELS + i as usize] * (1.0 / LEVELS as f32)
        } else {
            v
        }
    };
    [ch(0), ch(1), ch(2), c[3]]
}

// The plug-in.

/// The plug-in's entry point.
pub(crate) unsafe extern "C" fn main(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    out_args: OfxPropertySetHandle,
) -> OfxStatus {
    if suites().is_none() {
        return kOfxStatErrMissingHostFeature;
    }
    let fx = Fx(handle as OfxImageEffectHandle);
    let (args, out) = (Props(in_args), Props(out_args));
    if is(action, kOfxActionDescribe) {
        describe(fx);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionDescribeInContext) {
        describe_in_context(fx);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionRender) {
        render(fx, args)
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        // Motion compensation reads anywhere in the frames.
        let time = args.dbl(kOfxPropTime, 0);
        let roi: [f64; 4] = args.dbls(kOfxImageEffectPropRegionOfInterest);
        let roi = fx
            .clip(kOfxImageEffectSimpleSourceClipName)
            .rod(time)
            .unwrap_or(roi);
        out.set_dbls(c"OfxImageClipPropRoI_Source", &roi);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionGetFramesNeeded) {
        let time = args.dbl(kOfxPropTime, 0);
        let s = Settings::at(fx, time);
        let (before, after) = if s.addition || s.histogram {
            (0, 0)
        } else {
            (s.before(), s.window - s.before())
        };
        out.set_dbls(
            c"OfxImageClipPropFrameRange_Source",
            &[time - f64::from(before), time + f64::from(after)],
        );
        kOfxStatOK
    } else if is(action, kOfxActionInstanceChanged) {
        let time = args.dbl(kOfxPropTime, 0);
        if args.is(kOfxPropType, kOfxTypeParameter) {
            changed(fx, time, &args.text(kOfxPropName, 0));
        }
        show(fx, time);
        kOfxStatOK
    } else if is(action, kOfxActionCreateInstance) || is(action, kOfxActionBeginInstanceEdit) {
        show(fx, 0.0);
        kOfxStatOK
    } else if is(action, kOfxActionLoad)
        || is(action, kOfxActionUnload)
        || is(action, kOfxActionDestroyInstance)
    {
        kOfxStatOK
    } else {
        kOfxStatReplyDefault
    }
}

/// Whether Calculate counted the histograms.
fn analyzed(fx: Fx, time: f64) -> bool {
    !fx.param(HISTOGRAMS).text_at(time).trim().is_empty()
}

/// Shows what the mode uses, as VEGAS does: Flicker Frequency for
/// Addition; Reduction Mode and Analysis Window for Reduction, with motion
/// compensation (on once motion data is keyed) for Temporal blend and
/// Calculate and Reset (one of them on, as the histograms are counted or
/// not) for Histogram smoothing.
fn show(fx: Fx, time: f64) {
    let s = Settings::at(fx, time);
    let histograms = !s.addition && s.histogram;
    fx.param(FREQUENCY).set_secret(!s.addition);
    for name in [REDUCTION, WINDOW] {
        fx.param(name).set_secret(s.addition);
    }
    fx.param(MOTION).set_secret(s.addition || s.histogram);
    for name in [CALCULATE, RESET] {
        fx.param(name).set_secret(!histograms);
    }
    let done = analyzed(fx, time);
    fx.param(CALCULATE).set_enabled(!done);
    fx.param(RESET).set_enabled(done);
    fx.param(MOTION)
        .set_enabled(fx.param(CORNERS[0]).key_times().len() > 1);
    // Analysis Window's slider: up to 60 frames for histograms, 5 for
    // Temporal blend.
    let p = fx.param(WINDOW).props();
    p.set_int(kOfxParamPropDisplayMin, 0);
    p.set_int(kOfxParamPropDisplayMax, if s.histogram { 60 } else { 5 });
}

/// VEGAS's say when `name` changes: Temporal blend's window lowered to 5
/// frames when the mode changes to it, Histogram smoothing's ask for
/// Calculate, the warning that a flicker frequency in step with the frame
/// rate does not show, Reset clearing the histograms.
fn changed(fx: Fx, time: f64, name: &str) {
    let s = Settings::at(fx, time);
    let is = |n: &CStr| n.to_str() == Ok(name);
    if is(MODE) || is(REDUCTION) {
        fx.persistent_message(None);
        if !s.addition && !s.histogram && s.window > 5 {
            fx.param(WINDOW).set_int(5);
        }
        if !s.addition && s.histogram && !analyzed(fx, time) {
            fx.persistent_message_as(
                kOfxMessageMessage,
                Some(c"To apply Histogram Smoothing, click the Calculate."),
            );
        }
    } else if is(FREQUENCY) {
        let p = fx.param(FREQUENCY);
        if p.key_times().is_empty() {
            let rate = fx.props().dbl_or(kOfxImageEffectPropFrameRate, 30.0);
            let rate = if rate >= 1.0 { rate } else { 30.0 };
            let beats = 2.0 * p.f64_at(time) / rate;
            if beats.fract() < 0.1 && beats.trunc() != 0.0 {
                fx.persistent_message_as(
                    kOfxMessageMessage,
                    Some(c"Your flicker frequency is synchronized with your project frame rate and will not be visible. For better results, set your flicker frequency to something that is not a multiple of either your project frame rate."),
                );
            } else {
                fx.persistent_message(None);
            }
        }
    } else if is(RESET) {
        fx.param(HISTOGRAMS).set_text("");
    } else if is(HISTOGRAMS) && analyzed(fx, time) {
        fx.persistent_message(None);
    }
}

fn describe(fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, c"Flicker Control");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Utility");
    p.set_str(
        kOfxPropPluginDescription,
        c"Reduces or adds flicker artifacts",
    );
    p.set_strs(
        kOfxImageEffectPropSupportedContexts,
        &[kOfxImageEffectContextFilter, kOfxImageEffectContextGeneral],
    );
    p.set_strs(
        kOfxImageEffectPropSupportedPixelDepths,
        &[kOfxBitDepthFloat, kOfxBitDepthShort, kOfxBitDepthByte],
    );
    p.set_int(kOfxImageEffectPropSupportsTiles, 0);
    p.set_int(kOfxImageEffectPropSupportsMultiResolution, 1);
    p.set_int(kOfxImageEffectPropTemporalClipAccess, 1);
    p.set_int(kOfxImageEffectFrameVarying, 1);
    p.set_int(kOfxImageEffectPluginPropHostFrameThreading, 0);
    p.set_str(
        kOfxImageEffectPluginRenderThreadSafety,
        kOfxImageEffectRenderFullySafe,
    );
}

fn describe_in_context(fx: Fx) {
    let comps = [kOfxImageComponentRGBA, kOfxImageComponentRGB];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps)
        .set_int(kOfxImageEffectPropTemporalClipAccess, 1);
    fx.define_clip(kOfxImageEffectOutputClipName, &[kOfxImageComponentRGBA]);
    for spec in &SPECS[..SPECS.len() - 1] {
        let Some(p) = define(fx, *spec) else { continue };
        let name = spec.name();
        if [MODE, REDUCTION, WINDOW, THRESHOLD].contains(&name) {
            p.set_int(kOfxParamPropAnimates, 0);
        }
        if name == FREQUENCY {
            p.set_int(kOfxParamPropSecret, 1);
            p.set_dbl(kOfxParamPropIncrement, 0.1);
            p.set_int(kOfxParamPropDigits, 1);
        }
        if name == SENSITIVITY || name == THRESHOLD {
            p.set_dbl(kOfxParamPropIncrement, 0.1);
            p.set_int(kOfxParamPropDigits, 2);
        }
        if name == THRESHOLD {
            p.set_int(kOfxParamPropSecret, 1);
            p.set_int(kOfxParamPropEnabled, 0);
        }
        if name == MOTION {
            p.set_int(kOfxParamPropEnabled, 0);
        }
        if CORNERS.contains(&name) {
            p.set_int(kOfxParamPropSecret, 1);
            p.set_dbl(kOfxParamPropIncrement, 0.1);
            p.set_int(kOfxParamPropDigits, 2);
        }
    }
    let p = fx.define_param(kOfxParamTypePushButton, CALCULATE, c"Calculate");
    p.set_str(kOfxParamPropHint, c"Starts the analysis.");
    p.set_int(kOfxParamPropSecret, 1);
    p.set_str(
        bv_ofx_sys::extensions::kBananaVParamPropAnalysis,
        c"ColorHistograms",
    );
    p.set_str(
        bv_ofx_sys::extensions::kBananaVParamPropAnalysisParam,
        HISTOGRAMS,
    );
    let p = fx.define_param(kOfxParamTypePushButton, RESET, c"Reset");
    p.set_str(kOfxParamPropHint, c"Clears the results of analysis.");
    p.set_int(kOfxParamPropSecret, 1);
    p.set_int(kOfxParamPropEnabled, 0);
    let p = fx.define_param(kOfxParamTypeString, HISTOGRAMS, c"Histograms");
    p.set_str(kOfxParamPropDefault, c"");
    p.set_int(kOfxParamPropSecret, 1);
    p.set_int(kOfxParamPropAnimates, 0);
    if let Some(p) = define(fx, SPECS[SPECS.len() - 1]) {
        p.set_int(kOfxParamPropAnimates, 0);
        p.set_int(kOfxParamPropPersistant, 0);
    }
}

fn render(fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let window: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let clip = fx.clip(kOfxImageEffectSimpleSourceClipName);
    let Some(src) = clip.image(time) else {
        return kOfxStatFailed;
    };
    if src.depth != out.depth {
        return kOfxStatErrImageFormat;
    }
    let s = Settings::at(fx, time);
    let cur = Buf::read(Some(&src));
    drop(src);
    let rate = || {
        let r = fx.props().dbl_or(kOfxImageEffectPropFrameRate, 0.0);
        if r >= 1.0 {
            r
        } else {
            clip.props()
                .map_or(0.0, |p| p.dbl_or(kOfxImageEffectPropFrameRate, 0.0))
        }
    };
    let result = if s.addition {
        let amount = added(&s, time, rate());
        cur.map(|x, y| add_pixel(s.tones, amount, cur.at(x, y)))
    } else if s.histogram {
        let text = fx.param(HISTOGRAMS).text_at(time);
        match levels_table(&s, &text, time) {
            Some(table) => {
                let range = mapped_levels(s.tones);
                cur.map(|x, y| match_pixel(&table, range, cur.at(x, y)))
            }
            None => cur.clone(),
        }
    } else {
        let range = clip
            .props()
            .map_or([f64::NEG_INFINITY, f64::INFINITY], |p| {
                p.dbls(kOfxImageEffectPropFrameRange)
            });
        let (w, center) = weights(s.window, s.sensitivity);
        let start = time - f64::from(s.before());
        // The window's frames the clip has, with their weights (and
        // corners).
        let mut pictures: Vec<(Buf, f32, f64)> = Vec::new();
        for (k, &weight) in w.iter().enumerate() {
            let t = start + k as f64;
            if !has_frame(range, t) {
                continue;
            }
            let picture = if t == time {
                Some(cur.clone())
            } else {
                clip.image(t).map(|i| Buf::read(Some(&i)))
            };
            if let Some(p) = picture.filter(|p| p.bounds == cur.bounds) {
                pictures.push((p, weight, t));
            }
        }
        let maps = if s.motion {
            let size = (cur.bounds[2], cur.bounds[3]);
            let corners: Vec<_> = pictures
                .iter()
                .map(|&(_, _, t)| corners_in(CORNERS.map(|c| fx.param(c).xy_at(t)), size))
                .collect();
            maps(&corners, center)
        } else {
            vec![None; pictures.len()]
        };
        let taken: Vec<Taken<'_>> = pictures
            .iter()
            .zip(maps)
            .map(|((picture, weight, _), map)| Taken {
                picture,
                weight: *weight,
                map,
            })
            .collect();
        blend(&taken, center, s.tones, cur.bounds)
    };
    let result = if s.mask {
        mask_of(&cur, result)
    } else {
        result
    };
    let [x1, y1, x2, y2] = window;
    crate::compositors::write(&out, &result, (x1, y1, x2, y2));
    drop(out);
    kOfxStatOK
}

/// The factory presets: VEGAS's, in its list's order.
pub(crate) fn presets() -> Vec<Preset> {
    let num = |v: f64| PresetValue::Num(vec![v]);
    let p = |name: &'static str, values: Vec<(&'static str, f64)>| Preset {
        name,
        values: values.into_iter().map(|(n, v)| (n, num(v))).collect(),
    };
    vec![
        p(
            "Flicker Reduction",
            vec![
                ("mode", 0.0),
                ("reductionMode", 0.0),
                ("window", 1.0),
                ("sensitivity", 1.0),
                ("threshold", 0.0),
                ("type", 0.0),
            ],
        ),
        p(
            "TimeLapse-Flicker Reduction",
            vec![
                ("mode", 0.0),
                ("reductionMode", 1.0),
                ("window", 50.0),
                ("sensitivity", 1.0),
                ("threshold", 0.0),
                ("type", 3.0),
            ],
        ),
        p(
            "Add 50Hz Flicker",
            vec![
                ("mode", 1.0),
                ("frequency", 50.0),
                ("sensitivity", 0.2),
                ("type", 3.0),
            ],
        ),
        p(
            "Add 60Hz Flicker",
            vec![
                ("mode", 1.0),
                ("frequency", 60.0),
                ("sensitivity", 0.2),
                ("type", 3.0),
            ],
        ),
        p(
            "Add Celluloid Flicker",
            vec![
                ("mode", 1.0),
                ("frequency", 0.0),
                ("sensitivity", 0.3),
                ("type", 3.0),
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: c_int = 16;
    const H: c_int = 8;

    fn settings(set: &[(&str, f64)]) -> Settings {
        Settings::of(&Vals::of(SPECS, &|s: Spec| {
            set.iter()
                .find(|(n, _)| s.name().to_str() == Ok(*n))
                .map(|(_, v)| vec![*v])
                .unwrap_or_else(|| s.default())
        }))
    }

    fn solid(v: f32) -> Buf {
        Buf::solid([0, 0, W, H], [v, v, v, 1.0])
    }

    #[test]
    fn the_weights_are_vegass() {
        // Two frames: the table's, drawn to an even mean at Sensitivity 1.
        let (w, center) = weights(1, 1.0);
        assert_eq!(center, 0);
        assert!(
            (w[0] - 0.5).abs() < 1e-6 && (w[1] - 0.5).abs() < 1e-6,
            "{w:?}"
        );
        let (w, _) = weights(1, 0.5);
        assert!((w[0] - (0.666 + 0.25) / 1.499).abs() < 1e-5, "{w:?}");
        // Binomial below 0.5, the frame itself counting more.
        let (w, center) = weights(2, 0.0);
        assert_eq!(center, 1);
        let sum = 0.5 + 2.0 + 0.5;
        assert!((w[1] - 2.0 / sum).abs() < 1e-6 && (w[0] - 0.5 / sum).abs() < 1e-6);
        // Five frames' middle coefficients cut down as VEGAS's are: 8, not
        // 10.
        let (w, _) = weights(5, 0.5);
        let b = [1.0, 5.0, 8.0, 8.0, 5.0, 1.0f32];
        let sum: f32 = b.iter().map(|b| b + 0.5 / 6.0).sum();
        assert!((w[2] - (8.0 + 0.5 / 6.0) / sum).abs() < 1e-6, "{w:?}");
        // No window: the frame alone.
        assert_eq!(weights(0, 0.3), (vec![1.0], 0));
    }

    #[test]
    fn temporal_blend_mixes_the_frames_by_tone() {
        let (a, b) = (solid(0.2), solid(0.6));
        let take = |p, weight| Taken {
            picture: p,
            weight,
            map: None,
        };
        let frames = [take(&a, 0.5), take(&b, 0.5)];
        // All: the weighted sum.
        let out = blend(&frames, 0, 3, [0, 0, W, H]);
        assert!((out.at(3, 3)[0] - 0.4).abs() < 1e-6);
        // Shadows: the brighter frame counts 1 − 0.36 of its weight, the
        // frame itself all of it.
        let out = blend(&frames, 0, 2, [0, 0, W, H]);
        let k = 1.0 - 0.36;
        let want = (0.5 * 0.2 + 0.5 * k * 0.6) / (0.5 + 0.5 * k);
        assert!((out.at(3, 3)[0] - want).abs() < 1e-6, "{:?}", out.at(3, 3));
        assert_eq!(out.at(3, 3)[3], 1.0);
        // A frame missing at the clip's start: All's sum is not made up for.
        let out = blend(&frames[1..], 1, 3, [0, 0, W, H]);
        assert!((out.at(3, 3)[0] - 0.3).abs() < 1e-6);
        assert_eq!(out.at(3, 3)[3], 0.0);
    }

    #[test]
    fn motion_compensation_reads_along_the_corners() {
        // The next frame's corners 2 pixels to the right: its pixels are
        // read 2 to the right; past the picture, the frame's own.
        let mut next = solid(0.0);
        for y in 0..H {
            for x in 0..W {
                next.data[(y * W + x) as usize] = [x as f32 / 100.0, 0.0, 0.0, 1.0];
            }
        }
        let own = solid(0.5);
        let parts = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
        let here = corners_in(parts, (W, H));
        let there = here.map(|[x, y]| [x + 2.0, y]);
        let maps = maps(&[here, there], 0);
        assert!(maps[0].is_none());
        let frames = [
            Taken {
                picture: &own,
                weight: 0.0,
                map: maps[0],
            },
            Taken {
                picture: &next,
                weight: 1.0,
                map: maps[1],
            },
        ];
        let out = blend(&frames, 0, 3, [0, 0, W, H]);
        assert!((out.at(5, 3)[0] - 0.07).abs() < 1e-5, "{:?}", out.at(5, 3));
        assert!(
            (out.at(W - 1, 3)[0] - 0.5).abs() < 1e-5,
            "{:?}",
            out.at(W - 1, 3)
        );
    }

    #[test]
    fn addition_is_vegass() {
        let s = settings(&[("mode", 1.0), ("frequency", 7.5), ("sensitivity", 0.5)]);
        // A quarter of a pulse in at 30 frames a second: frame 1.
        let a = added(&s, 1.0, 30.0);
        assert!((a - 0.05).abs() < 1e-9, "{a}");
        let c = add_pixel(3, a, [0.5, 0.25, 0.1, 1.0]);
        assert!((c[0] - 0.55).abs() < 1e-6 && (c[2] - 0.15).abs() < 1e-6);
        // Held within 0–1, by the largest channel's tone, times the alpha.
        assert_eq!(add_pixel(3, 0.5, [0.9, 0.0, 0.0, 1.0])[0], 1.0);
        let c = add_pixel(2, 0.1, [0.5, 0.2, 0.2, 0.5]);
        assert!((c[1] - (0.2 + 0.75 * 0.1 * 0.5)).abs() < 1e-6, "{c:?}");
        assert_eq!(add_pixel(3, 0.1, [0.5, 0.5, 0.5, 0.0]), [0.0; 4]);
        // Random below 1 hertz: the first second always, about a tenth of
        // Sensitivity; the same frame alike.
        let s = settings(&[("mode", 1.0), ("frequency", 0.0), ("sensitivity", 0.3)]);
        let a = added(&s, 4.0, 30.0);
        assert!((a - 0.03).abs() < 0.006, "{a}");
        assert_eq!(a, added(&s, 4.0, 30.0));
        // Later seconds only now and then.
        let lit = (300..3000)
            .filter(|&f| added(&s, f64::from(f), 30.0) != 0.0)
            .count();
        assert!(lit < 600, "{lit}");
    }

    /// A text of histograms: frame `k`'s pixels half at level 300, half at
    /// `levels[k]`.
    fn text(levels: &[usize]) -> String {
        let frames: Vec<(f64, [Vec<u64>; 3])> = levels
            .iter()
            .enumerate()
            .map(|(t, &l)| {
                let mut counts = vec![0u64; LEVELS];
                counts[300] += 50;
                counts[l] += 50;
                (t as f64, [counts.clone(), counts.clone(), counts])
            })
            .collect();
        histograms_text(&frames)
    }

    #[test]
    fn histogram_smoothing_matches_the_windows_histograms() {
        assert_eq!(level(0.5), 512);
        assert_eq!(level(-1.0), 0);
        assert_eq!(level(2.0), LEVELS - 1);
        let s = settings(&[("reductionMode", 1.0), ("window", 2.0)]);
        // A darker frame among brighter ones: its dark half's level matched
        // to where the shares' harmonic mean reaches all.
        let text = text(&[500, 500, 400, 500, 500]);
        let table = levels_table(&s, &text, 2.0).unwrap();
        assert_eq!(table[400], 500.0);
        assert_eq!(table[300], 300.0);
        let c = match_pixel(&table, mapped_levels(3), [400.0 / 1024.0, 0.0, 2.0, 1.0]);
        assert_eq!(c[0], 500.0 / 1024.0);
        // Above 1: left as it is.
        assert_eq!(c[2], 2.0);
        // Only the highlights' levels.
        let c = match_pixel(&table, mapped_levels(0), [0.4, 0.4, 0.4, 1.0]);
        assert_eq!(c[0], 0.4);
        // Without histograms, or a window wider than the clip: nothing.
        assert!(levels_table(&s, "", 2.0).is_none());
        let s = settings(&[("reductionMode", 1.0), ("window", 10.0)]);
        assert!(levels_table(&s, &text, 2.0).is_none());
    }

    #[test]
    fn smoothing_below_full_sensitivity_weighs_by_a_bell() {
        let cdf = |l: usize| -> Vec<f32> {
            (0..LEVELS)
                .map(|b| if b >= l { 1.0 } else { 0.5 })
                .collect()
        };
        let (a, b) = (cdf(300), cdf(600));
        let frames: Vec<&[f32]> = vec![&a, &a, &b, &a, &a];
        let out = smoothed(&frames, 2, 1, 0.5).unwrap();
        assert_eq!(out[0], 0.0);
        assert_eq!(out[LEVELS - 1], 1.0);
        // Between the levels: the near frames' 1, the frame's own 0.5.
        let bell = (-1.0f32 / 0.5).exp();
        let whole = 1.0 + 2.0 * bell;
        let want = whole / (1.0 / 0.5 + 2.0 * bell / 1.0);
        assert!((out[400] - want).abs() < 1e-6, "{}", out[400]);
    }

    #[test]
    fn the_mask_shows_the_change() {
        assert!((masked([0.5, 0.5, 0.5, 0.8], [0.6, 0.5, 0.5, 1.0])[0] - 0.5).abs() < 1e-6);
        assert_eq!(masked([0.5, 0.5, 0.5, 0.8], [0.6, 0.5, 0.5, 1.0])[3], 0.8);
        assert_eq!(masked([0.0; 4], [0.0, 0.9, 0.0, 1.0])[1], 1.0);
    }

    #[test]
    fn the_histograms_text_reads_back() {
        let text = text(&[10, 20]);
        let frames = parse_histograms(&text).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[1].0, 1.0);
        assert_eq!(frames[1].1[0][19], 0.0);
        assert_eq!(frames[1].1[0][20], 0.5);
        assert_eq!(frames[1].1[2][300], 1.0);
        assert!(parse_histograms("bvlights 1 1024\n").is_none());
    }
}
