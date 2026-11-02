//! Video Stabilization, as VEGAS's: its panel — User View's Basic,
//! Professional and Expert controls, which groups show and which grey out
//! before and after an analysis, as VEGAS's rules say —, its analysis
//! through the host (BananaV's `kBananaVParamPropAnalysis` "Motion": each
//! frame's move to the next, kept as text in the hidden Motion Data, the
//! analysis's area, grid, tracker and resolution the Expert view's, named
//! by `kBananaVParamPropAnalysisRegion`, `…Grid`, `…Follow` and
//! `…Reduction`), and its math: each move's six numbers (shift, angle,
//! scales, shear — as many as Motion Compensation keeps) smoothed by a
//! Blackman window Smoothing seconds long, its ends mirrored, or held at
//! nothing to freeze the Reference Frame Number's view; the moves summed
//! from the reference into each frame's correction, the Smoothing
//! Parameters' unticked components left as filmed; Auto Zoom the least
//! zoom whose corners stay inside the corrected frames where a correction
//! is at its most (a hundredth up, and one more), written into Additional
//! Zoom as VEGAS writes it; the picture read through the correction and
//! the additional zoom, turn and shift between its four pixels weighed in
//! truncated 1024ths, edges held — transparent outside, or the picture
//! mirrored, repeated or held as Edge Filling says —, and Add Motion
//! Blur's box along the rows, as long as the smoothed pan is past
//! Threshold, its ends mirrored. Expert's region of interest is drawn and
//! dragged on the preview over the analysis grid; Show Points shows the
//! frame unstabilized under the features the host followed.
#![allow(unsafe_code)]

use std::collections::hash_map::DefaultHasher;
use std::ffi::{CStr, c_char, c_int, c_void};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::{Plan, Values};
use crate::kit::Buf;
use crate::ofx::{Clicks, Draw, Fx, Props, interact_props, is, redraw, suites};

pub(crate) const ID: &CStr = c"org.bananav.VideoStabilization";

const HAS_MOTION: &CStr = c"hasMotionData";
pub(crate) const VIEWS: &CStr = c"views";
const GROUP_PARAMETERS: &CStr = c"groupParameters";
const PRESETS: &CStr = c"presets";
const COMPENSATION: &CStr = c"motionCompensation";
const GROUP_EXPERT: &CStr = c"groupExpertMode";
const X_SCALE: &CStr = c"xScale";
const Y_SCALE: &CStr = c"yScale";
const LOCATION: &CStr = c"location";
const GRIDS_X: &CStr = c"numGridsX";
const GRIDS_Y: &CStr = c"numGridsY";
const POINTS_PER_CELL: &CStr = c"pointsPerGridCell";
const TRACKER_TYPE: &CStr = c"choiceTrackerType";
const RESOLUTION: &CStr = c"resolution";
const ANALYZE_BASIC: &CStr = c"analyzeMotionBasic";
const CLEAR_BASIC: &CStr = c"clearMotionBasic";
const ANALYZE: &CStr = c"analyzeMotion";
const CLEAR: &CStr = c"clearMotion";
const GROUP_SMOOTHING: &CStr = c"groupSmoothing";
const MODE: &CStr = c"stabilizeMode";
const MAGNITUDE: &CStr = c"smoothingMagnitude";
const REFERENCE: &CStr = c"staticReference";
const GROUP_LOCKS: &CStr = c"groupLockState";
/// Each component smoothed (ticked) or left as filmed: the shift across
/// and up, the angle, the scales across and up, the shear.
const LOCKS: [&CStr; 6] = [
    c"lockTranslateX",
    c"lockTranslateY",
    c"lockAngle",
    c"lockScaleX",
    c"lockScaleY",
    c"lockShear",
];
const GROUP_BORDERS: &CStr = c"groupBorders";
const AUTO_ZOOM: &CStr = c"avoidBlackBorders";
const ADD_SCALE: &CStr = c"additionalScale";
const ADD_ROTATION: &CStr = c"additionalRotation";
const ADD_TRANSLATION: &CStr = c"additionalTranslation";
const FILLING: &CStr = c"zoomFilling";
const FILL_COLOR: &CStr = c"zoomColorPicker";
const OFFSET: &CStr = c"correction";
const GROUP_BLUR: &CStr = c"groupMotionBlur";
const BLUR: &CStr = c"motionBlur";
const BLUR_THRESHOLD: &CStr = c"motionBlurThreshold";
const BLUR_MOST: &CStr = c"motionBlurMaxMagnitude";
const GROUP_VIEW: &CStr = c"groupVisualization";
pub(crate) const SHOW_POINTS: &CStr = c"showFeatures";
pub(crate) const MIN_TRACK: &CStr = c"showFeaturesMinLen";
/// A move from elsewhere (a tracked region's four corners, in pixels of
/// a frame External Project Size big), used once Apply External Motion
/// is set.
const EXTERNAL: [&CStr; 4] = [
    c"externalMotionData1",
    c"externalMotionData2",
    c"externalMotionData3",
    c"externalMotionData4",
];
const EXTERNAL_SIZE: &CStr = c"externalProjectSize";
const APPLY_EXTERNAL: &CStr = c"applyExternalMotion";
const LAST_STATE: &CStr = c"lastState";
const GEOMETRIC_MODEL: &CStr = c"geometricModel";
const TRACKER: &CStr = c"tracker";
const WINDOW_SIZE: &CStr = c"trackerWinSize";
const LEVELS: &CStr = c"trackerLevel";
const DETECTOR: &CStr = c"detector";
const POINT_HANDLING: &CStr = c"pointHandling";
const RANSAC: &CStr = c"ransac";
const SMOOTHING: &CStr = c"smoothing";
const TEMPO: &CStr = c"tempoMode";
const FRAME_LENGTH: &CStr = c"smoothingFrameLength";
const STATIC_CAMERA: &CStr = c"isStaticCamera";
const CLEAR_STATE: &CStr = c"clearState";
/// The motion the host found (hidden).
pub(crate) const MOTION: &CStr = c"motionData";
const HAS_CORRECTION: &CStr = c"hasCorrectionData";
const CORRECTION: &CStr = c"correctionData";
const FRAME_NO: &CStr = c"frameNo";
const TRACK_DATA: &CStr = c"trackData";

const VIEW_NAMES: [&CStr; 3] = [c"Basic", c"Professional", c"Expert"];
const METHODS: [&CStr; 2] = [c"Fast", c"Accurate"];
const COMPENSATIONS: [&CStr; 4] = [
    c"Translation",
    c"Translation + Rotation",
    c"Translation + Rotation + Scale",
    c"Warp",
];
const TRACKERS: [&CStr; 3] = [c"Lucas-Kanade", c"Sum of Absolute Differences", c"Hybrid"];
const RESOLUTIONS: [&CStr; 3] = [c"Full", c"Half", c"Quarter"];
const MODES: [&CStr; 2] = [c"Smooth motion", c"Freeze motion"];
const FILLS: [&CStr; 4] = [
    c"Transparent",
    c"Mirror Padding",
    c"Repeat",
    c"Clamp to Edge",
];

/// The message while there is nothing analyzed.
const MUST_CLICK: &CStr = c"To stabilize the video, click the Stabilize / Analyze Motion button. Do not attempt to preview the media or play your project while stabilization is in process.";

/// The motion text's first word.
const MAGIC: &str = "bvmotion";

/// One frame's move to the next, about the frame's middle: its shift
/// (parts of the frame's width, y down), its turn (radians, clockwise on
/// the screen) and scale — the closest shift, turn and scale —, and the
/// whole linear part of the move (`[a, b, c, d]`: `x′ = a x + b y`,
/// `y′ = c x + d y` about the middle, y down), which Warp keeps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub dx: f64,
    pub dy: f64,
    pub turn: f64,
    pub scale: f64,
    pub linear: [f64; 4],
}

impl Step {
    /// A shift, turn and scale alone.
    pub fn similar(dx: f64, dy: f64, turn: f64, scale: f64) -> Step {
        let (s, c) = turn.sin_cos();
        Step {
            dx,
            dy,
            turn,
            scale,
            linear: [scale * c, -scale * s, scale * s, scale * c],
        }
    }
}

impl Default for Step {
    fn default() -> Step {
        Step::similar(0.0, 0.0, 0.0, 1.0)
    }
}

/// The motion as the hidden parameter keeps it: a line a frame.
pub fn motion_text(steps: &[Step]) -> String {
    let mut s = format!("{MAGIC} 2 {}\n", steps.len());
    for m in steps {
        let [a, b, c, d] = m.linear;
        s.push_str(&format!(
            "{:.7} {:.7} {:.8} {:.8} {a:.8} {b:.8} {c:.8} {d:.8}\n",
            m.dx, m.dy, m.turn, m.scale
        ));
    }
    s
}

/// The motion of the text (earlier lines without the linear part keep
/// their shift, turn and scale).
pub(crate) fn parse(text: &str) -> Option<Vec<Step>> {
    let mut lines = text.lines();
    let head: Vec<&str> = lines.next()?.split_whitespace().collect();
    if head.first() != Some(&MAGIC) {
        return None;
    }
    Some(
        lines
            .filter_map(|l| {
                let v: Vec<f64> = l
                    .split_whitespace()
                    .filter_map(|w| w.parse().ok())
                    .collect();
                match v.len() {
                    4 => Some(Step::similar(v[0], v[1], v[2], v[3].max(1e-3))),
                    8 => Some(Step {
                        dx: v[0],
                        dy: v[1],
                        turn: v[2],
                        scale: v[3].max(1e-3),
                        linear: [v[4], v[5], v[6], v[7]],
                    }),
                    _ => None,
                }
            })
            .collect(),
    )
}

/// What Motion Compensation keeps of each move (VEGAS's geometric
/// models 1, 3, 4 and 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Model {
    Translation,
    Rotation,
    Similar,
    Warp,
}

impl Model {
    fn of(compensation: c_int) -> Model {
        match compensation {
            0 => Model::Translation,
            1 => Model::Rotation,
            3 => Model::Warp,
            _ => Model::Similar,
        }
    }

    /// VEGAS's number for it (the hidden Geometric Model).
    fn number(self) -> c_int {
        match self {
            Model::Translation => 1,
            Model::Rotation => 3,
            Model::Similar => 4,
            Model::Warp => 6,
        }
    }

    /// Which Smoothing Parameters it has.
    fn locks(self) -> [bool; 6] {
        match self {
            Model::Translation => [true, true, false, false, false, false],
            Model::Rotation => [true, true, true, false, false, false],
            Model::Similar => [true, true, true, true, false, false],
            Model::Warp => [true; 6],
        }
    }
}

/// A plane's affine map in a frame's pixels, rows up: `x′ = m[0] x +
/// m[1] y + m[2]`, `y′ = m[3] x + m[4] y + m[5]`.
pub(crate) type Affine = [f64; 6];

pub(crate) const IDENTITY: Affine = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];

/// `a` after `b`.
pub(crate) fn compose(a: &Affine, b: &Affine) -> Affine {
    [
        a[0] * b[0] + a[1] * b[3],
        a[0] * b[1] + a[1] * b[4],
        a[0] * b[2] + a[1] * b[5] + a[2],
        a[3] * b[0] + a[4] * b[3],
        a[3] * b[1] + a[4] * b[4],
        a[3] * b[2] + a[4] * b[5] + a[5],
    ]
}

pub(crate) fn invert(m: &Affine) -> Affine {
    let det = m[0] * m[4] - m[1] * m[3];
    let det = if det.abs() < 1e-15 { 1e-15 } else { det };
    let (a, b, d, e) = (m[4] / det, -m[1] / det, -m[3] / det, m[0] / det);
    [a, b, -(a * m[2] + b * m[5]), d, e, -(d * m[2] + e * m[5])]
}

pub(crate) fn apply(m: &Affine, [x, y]: [f64; 2]) -> [f64; 2] {
    [m[0] * x + m[1] * y + m[2], m[3] * x + m[4] * y + m[5]]
}

/// VEGAS's six numbers of a move: its shift (pixels, rows up), its angle
/// (radians, counterclockwise), its scales less one across and up, and
/// its shear — the linear part `diag(sx, sy) · [[1, 0], [shear, 1]] ·
/// turn(angle)`.
pub(crate) type Numbers = [f64; 6];

/// The linear part of `n`.
fn linear(n: &Numbers) -> [f64; 4] {
    let (s, c) = n[2].sin_cos();
    let (sx, sy, h) = (1.0 + n[3], 1.0 + n[4], n[5]);
    [sx * c, -sx * s, sy * (h * c + s), sy * (c - h * s)]
}

/// The angle, scales and shear of a linear part.
fn parts_of([a, b, c, d]: [f64; 4]) -> [f64; 4] {
    let sx = a.hypot(b);
    let angle = (-b).atan2(a);
    let (s, co) = angle.sin_cos();
    let sy = c * s + d * co;
    let shear = if sy.abs() > 1e-12 {
        (c * co - d * s) / sy
    } else {
        0.0
    };
    [angle, sx - 1.0, sy - 1.0, shear]
}

/// A move's numbers as `model` keeps them, about the middle of a frame
/// `size` pixels big (rows up).
fn numbers_of(step: &Step, model: Model, size: [f64; 2]) -> Numbers {
    let t = [step.dx * size[0], -step.dy * size[0]];
    match model {
        Model::Translation => [t[0], t[1], 0.0, 0.0, 0.0, 0.0],
        Model::Rotation => [t[0], t[1], -step.turn, 0.0, 0.0, 0.0],
        Model::Similar => {
            let s = step.scale - 1.0;
            [t[0], t[1], -step.turn, s, s, 0.0]
        }
        Model::Warp => {
            let [a, b, c, d] = step.linear;
            let [angle, sx, sy, shear] = parts_of([a, -b, -c, d]);
            [t[0], t[1], angle, sx, sy, shear]
        }
    }
}

/// The move of numbers `n` about the middle `mid`: `L (p − mid) + mid +
/// shift`.
fn move_of(n: &Numbers, mid: [f64; 2]) -> Affine {
    let l = linear(n);
    [
        l[0],
        l[1],
        mid[0] - l[0] * mid[0] - l[1] * mid[1] + n[0],
        l[2],
        l[3],
        mid[1] - l[2] * mid[0] - l[3] * mid[1] + n[1],
    ]
}

/// VEGAS's smoothing window: Blackman's, `n` long, in single precision
/// as it builds it, and its sum.
fn blackman(n: usize) -> (Vec<f32>, f32) {
    let inv = 1.0f32 / (n as f32 - 1.0);
    let mut sum = 0.0f32;
    let w = (0..n)
        .map(|i| {
            let a = ((i as f64 * std::f64::consts::TAU) as f32 * inv).cos();
            let b = (((2 * i) as f64 * std::f64::consts::TAU) as f32 * inv).cos();
            let v = (0.42f32 - a * 0.5) + b * 0.08;
            sum += v;
            v
        })
        .collect();
    (w, sum)
}

/// `x` smoothed by a Blackman window `window` long (odd; as long as `x`
/// at most, an odd length), its ends mirrored without repeating the end,
/// as VEGAS's; fewer than three values stay.
pub(crate) fn smoothed(x: &[f32], window: usize) -> Vec<f32> {
    let n = x.len();
    if n < 3 {
        return x.to_vec();
    }
    let mut len = window.max(3);
    if n < len {
        len = if n.is_multiple_of(2) { n - 1 } else { n };
    }
    let (w, sum) = blackman(len);
    let inv = 1.0f32 / sum;
    let half = (len / 2) as isize;
    let last = n as isize - 1;
    let at = |k: isize| {
        let k = if k < 0 {
            -k
        } else if k > last {
            2 * last - k
        } else {
            k
        };
        x[k.clamp(0, last) as usize]
    };
    (0..n as isize)
        .map(|i| {
            let mut s = 0.0f32;
            for (j, wj) in w.iter().enumerate() {
                s += at(i - half + j as isize) * wj;
            }
            s * inv
        })
        .collect()
}

/// What the corrections come from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Smoothing {
    pub model: Model,
    pub freeze: bool,
    /// The window's length in frames (odd).
    pub window: usize,
    pub reference: usize,
    /// Each component smoothed (else left as filmed).
    pub locks: [bool; 6],
}

/// Each frame's correction (where its output reads the frame: VEGAS's
/// six numbers about the frame's corner) and each move's smoothed shift
/// across (pixels a frame, for the motion blur).
#[derive(Debug)]
pub(crate) struct Corrections {
    pub numbers: Vec<Numbers>,
    pub pans: Vec<f32>,
}

/// The corrections of `steps` on a frame `size` pixels big.
pub(crate) fn corrections(steps: &[Step], s: Smoothing, size: [f64; 2]) -> Corrections {
    let mid = [size[0] / 2.0, size[1] / 2.0];
    let moves: Vec<Numbers> = steps.iter().map(|m| numbers_of(m, s.model, size)).collect();
    // Each component smoothed in single precision, as VEGAS keeps them, or
    // held at nothing to freeze the view; unticked ones as filmed.
    let mut smooth: Vec<Numbers> = moves.clone();
    for j in 0..6 {
        if !s.locks[j] || !s.model.locks()[j] {
            continue;
        }
        let column: Vec<f32> = moves.iter().map(|m| m[j] as f32).collect();
        let done = if s.freeze {
            vec![0.0; column.len()]
        } else {
            smoothed(&column, s.window)
        };
        for (m, v) in smooth.iter_mut().zip(done) {
            m[j] = f64::from(v);
        }
    }
    if !s.model.locks()[4] {
        for m in smooth.iter_mut() {
            m[4] = m[3];
        }
    }
    let n = steps.len() + 1;
    let r = if s.freeze { s.reference.min(n - 1) } else { 0 };
    // The camera's path and the smoothed one from the reference out, and
    // each frame read where its view meets the smoothed one.
    let (mut path, mut want) = (vec![IDENTITY; n], vec![IDENTITY; n]);
    for k in r + 1..n {
        path[k] = compose(&move_of(&moves[k - 1], mid), &path[k - 1]);
        want[k] = compose(&move_of(&smooth[k - 1], mid), &want[k - 1]);
    }
    for k in (0..r).rev() {
        path[k] = compose(&invert(&move_of(&moves[k], mid)), &path[k + 1]);
        want[k] = compose(&invert(&move_of(&smooth[k], mid)), &want[k + 1]);
    }
    let numbers = path
        .iter()
        .zip(&want)
        .map(|(p, q)| {
            let c = compose(p, &invert(q));
            let [angle, sx, sy, shear] = parts_of([c[0], c[1], c[3], c[4]]);
            [c[2], c[5], angle, sx, sy, shear]
        })
        .collect();
    Corrections {
        numbers,
        pans: smooth.iter().map(|m| m[0] as f32).collect(),
    }
}

/// What the additional controls add: Additional Zoom, its turn
/// (radians) and its shift (pixels from the middle, rows up).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Additional {
    pub zoom: f64,
    pub turn: f64,
    pub shift: [f64; 2],
}

/// Where each output pixel of a frame `size` pixels big reads the frame:
/// the correction `c` with the additional zoom, turn and shift, put
/// together as VEGAS puts them (the turn added to the correction's angle,
/// the zoom taken from its scales, the turn and zoom about the middle).
pub(crate) fn matrix(c: &Numbers, model: Model, add: Additional, [w, h]: [f64; 2]) -> Affine {
    let (sx, sy, shear) = match model {
        Model::Warp => (1.0 + c[3], 1.0 + c[4], c[5]),
        _ => (1.0 + c[3], 1.0 + c[3], 0.0),
    };
    let k = 1.0 - add.zoom;
    let (sa, ca) = (c[2] + add.turn).sin_cos();
    let (sr, cr) = add.turn.sin_cos();
    let d4 = (1.0 - cr * k) * 0.5;
    let d3 = sr * k * 0.5;
    let (zx, zy) = (sx - add.zoom, sy - add.zoom);
    [
        ca * zx,
        -sa * zx,
        d3 * h + d4 * w + (c[0] - add.shift[0]),
        (shear * ca + sa) * zy,
        (ca - shear * sa) * zy,
        d4 * h - d3 * w + (c[1] - add.shift[1]),
    ]
}

/// How far in (each way, pixels) a frame `size` big must be cut so its
/// view stays inside the picture a correction `m` reads: where the
/// corrected picture's edges cross the frame's diagonals, as VEGAS
/// measures it.
pub(crate) fn margins(m: &Affine, [w, h]: [f64; 2]) -> [f64; 2] {
    let back = invert(m);
    let q = [[0.0, 0.0], [0.0, h], [w, 0.0], [w, h]].map(|p| apply(&back, p));
    let mut out = [0.0f64; 2];
    let diagonals = [([0.0, 0.0], [w, h]), ([0.0, h], [w, 0.0])];
    let cross = |a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]| {
        let (r, s) = ([b[0] - a[0], b[1] - a[1]], [d[0] - c[0], d[1] - c[1]]);
        let den = r[0] * s[1] - r[1] * s[0];
        if den.abs() < 1e-12 {
            return None;
        }
        let t = ((c[0] - a[0]) * s[1] - (c[1] - a[1]) * s[0]) / den;
        Some([a[0] + t * r[0], a[1] + t * r[1]])
    };
    for (i, j) in [(0, 1), (0, 2), (1, 3), (2, 3)] {
        let (a, b) = (q[i], q[j]);
        let points: Vec<[f64; 2]> = if (a[0] - b[0]).abs() <= f64::from(f32::EPSILON) {
            vec![a, b]
        } else {
            diagonals
                .iter()
                .filter_map(|(c, d)| cross(a, b, *c, *d))
                .collect()
        };
        for p in points {
            out[0] = out[0].max(p[0].min(w - p[0]));
            out[1] = out[1].max(p[1].min(h - p[1]));
        }
    }
    out
}

/// Auto Zoom: the zoom whose view stays inside every corrected frame
/// where a correction's number is at its most or its least (as VEGAS
/// picks them, the frames before a frozen reference a frame late), a
/// hundredth up and one more, at most a half.
fn auto_zoom(c: &Corrections, model: Model, r: usize, size: [f64; 2]) -> f64 {
    let n = c.numbers.len();
    let mut lows = [0.0f64; 6];
    let mut highs = [0.0f64; 6];
    let mut frames = [0usize; 12];
    // The frames as VEGAS walks them: back from the reference, then on.
    let order = (0..r)
        .rev()
        .map(|k| (k, k))
        .chain((r + 1..n).map(|k| (k, k - 1)));
    for (k, noted) in order {
        for j in 0..6 {
            let v = c.numbers[k][j] as f32 as f64;
            if v < lows[j] {
                lows[j] = v;
                frames[2 * j] = noted;
            } else if v > highs[j] {
                highs[j] = v;
                frames[2 * j + 1] = noted;
            }
        }
    }
    let mut most = 0.0f32;
    for &f in &frames {
        let k = (f + 1).min(n - 1);
        let m = matrix(&c.numbers[k], model, Additional::default(), size);
        let [mx, my] = margins(&m, size);
        let part = ((mx as f32 + mx as f32) / size[0] as f32)
            .max((my as f32 + my as f32) / size[1] as f32);
        most = most.max(part);
    }
    let zoom = (most * 100.0).ceil() / 100.0 + 0.01;
    f64::from(zoom.min(0.5))
}

/// The plans made lately (rendering asks for one at every frame).
type Kept = (u64, Arc<Corrections>);
static PLANS: Mutex<Vec<Kept>> = Mutex::new(Vec::new());

/// The corrections of `text` with these settings, worked out once.
fn corrections_of(text: &str, s: Smoothing, size: [f64; 2]) -> Option<Arc<Corrections>> {
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    (s.model, s.freeze, s.window, s.reference, s.locks).hash(&mut h);
    size.map(f64::to_bits).hash(&mut h);
    let key = h.finish();
    if let Ok(kept) = PLANS.lock()
        && let Some((_, c)) = kept.iter().find(|(k, _)| *k == key)
    {
        return Some(c.clone());
    }
    let steps = parse(text)?;
    if steps.is_empty() {
        return None;
    }
    let made = Arc::new(corrections(&steps, s, size));
    if let Ok(mut kept) = PLANS.lock() {
        kept.retain(|(k, _)| *k != key);
        kept.push((key, made.clone()));
        if kept.len() > 4 {
            kept.remove(0);
        }
    }
    Some(made)
}

/// The window's length for Smoothing seconds at `rate` frames a second:
/// whole frames, odd, at least three.
pub(crate) fn window_of(seconds: f64, rate: f64) -> usize {
    let rate = if rate < 1.0 { 30.0 } else { rate };
    let n = (seconds * rate).max(0.0) as usize;
    (if n.is_multiple_of(2) { n + 1 } else { n }).max(3)
}

/// The settings a frame is drawn with.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Settings {
    motion: String,
    clear: bool,
    smoothing: Smoothing,
    add: [f64; 4],
    fill: c_int,
    blur: bool,
    threshold: f64,
    most: f64,
    show_points: bool,
}

impl Settings {
    fn at(fx: Fx, time: f64) -> Settings {
        let rate = fx.props().dbl_or(kOfxImageEffectPropFrameRate, 30.0);
        let p = |n: &CStr| fx.param(n);
        let t = p(ADD_TRANSLATION).xy_at(time);
        Settings {
            motion: p(MOTION).text_at(0.0),
            clear: p(CLEAR_STATE).int_at(0.0) != 0,
            smoothing: Smoothing {
                model: Model::of(p(COMPENSATION).int_at(0.0)),
                freeze: p(MODE).int_at(0.0) == 1,
                window: window_of(p(MAGNITUDE).f64_at(0.0), rate),
                reference: p(REFERENCE).int_at(0.0).max(0) as usize,
                locks: LOCKS.map(|l| p(l).int_at(time) != 0),
            },
            add: [
                p(ADD_SCALE).f64_at(time),
                p(ADD_ROTATION).f64_at(time),
                t[0],
                t[1],
            ],
            fill: p(FILLING).int_at(time),
            blur: p(BLUR).int_at(time) != 0,
            threshold: p(BLUR_THRESHOLD).f64_at(time),
            most: p(BLUR_MOST).f64_at(time),
            show_points: p(SHOW_POINTS).int_at(0.0) != 0,
        }
    }

    /// The settings in an instance's values (the card's).
    pub(crate) fn of_values(v: Values<'_>, rate: f64) -> Settings {
        let t = v.array(ADD_TRANSLATION, [0.5, 0.5]);
        Settings {
            motion: v.text(MOTION).unwrap_or("").to_string(),
            clear: v.int(CLEAR_STATE, 0) != 0,
            smoothing: Smoothing {
                model: Model::of(v.int(COMPENSATION, 2)),
                freeze: v.int(MODE, 0) == 1,
                window: window_of(v.f64(MAGNITUDE, 2.5), rate),
                reference: v.int(REFERENCE, 0).max(0) as usize,
                locks: LOCKS.map(|l| v.int(l, 1) != 0),
            },
            add: [v.f64(ADD_SCALE, 0.0), v.f64(ADD_ROTATION, 0.0), t[0], t[1]],
            fill: v.int(FILLING, 0),
            blur: v.int(BLUR, 0) != 0,
            threshold: v.f64(BLUR_THRESHOLD, 10.0),
            most: v.f64(BLUR_MOST, 19.0),
            show_points: v.int(SHOW_POINTS, 0) != 0,
        }
    }

    /// What frame `frame` of a picture `size` pixels big draws: where its
    /// pixels read the picture and the blur's reach (pixels), or nothing
    /// to change.
    fn drawing(&self, frame: f64, size: [f64; 2]) -> Option<(Affine, u32)> {
        if self.clear {
            return None;
        }
        let c = corrections_of(&self.motion, self.smoothing, size)?;
        let k = frame.round().max(0.0) as usize;
        // Past the frames analyzed, and while the points show: as filmed.
        if self.show_points || k >= c.numbers.len() {
            return Some((IDENTITY, 0));
        }
        let add = Additional {
            zoom: self.add[0],
            turn: self.add[1].to_radians(),
            shift: [(self.add[2] - 0.5) * size[0], (self.add[3] - 0.5) * size[1]],
        };
        let m = matrix(&c.numbers[k], self.smoothing.model, add, size);
        let reach = if self.blur && k > 0 {
            let pan = c.pans.get(k - 1).map_or(0.0, |p| p.abs());
            let threshold = self.threshold as f32;
            let ratio = if threshold >= f32::EPSILON {
                pan / threshold
            } else {
                pan
            };
            let limit = (((size[0] as u32).saturating_sub(3)) >> 1).max(self.most.max(0.0) as u32);
            if threshold < pan {
                (ratio as u32).min(limit)
            } else {
                0
            }
        } else {
            0
        };
        Some((m, reach))
    }

    /// What the card draws: the frame's map as parts of the frame and the
    /// blur's reach as a part of its width, or nothing.
    pub(crate) fn plan(&self, frame: f64, size: [f64; 2]) -> Plan {
        match self.drawing(frame, size) {
            None => Plan::Same,
            Some((m, reach)) if m == IDENTITY && reach == 0 => Plan::Same,
            Some((m, reach)) => Plan::Stabilize {
                map: normalized(&m, size),
                fill: self.fill.clamp(0, 3) as u32,
                reach: reach as f32 / size[0].max(1.0) as f32,
            },
        }
    }
}

/// A map in a frame's pixels as one in its parts (`x / w`, `y / h`).
pub(crate) fn normalized(m: &Affine, [w, h]: [f64; 2]) -> [f64; 6] {
    [m[0], m[1] * h / w, m[2] / w, m[3] * w / h, m[4], m[5] / h]
}

/// The card's numbers for a map in parts (`map`) on a picture `w` × `h`
/// pixels big: `x′ = x + (a − 1) x + b y + c`, `y′ = y + d x + (e − 1)
/// y + f` with `c` and `f` apart as their whole pixels and the rest — so
/// that the card's single precision keeps the fraction the processor's
/// doubles do.
pub(crate) fn card_numbers(map: &[f64; 6], w: u32, h: u32) -> [f32; 8] {
    let (w, h) = (f64::from(w), f64::from(h));
    let m = [
        map[0],
        map[1] * w / h,
        map[2] * w,
        map[3] * h / w,
        map[4],
        map[5] * h,
    ];
    let (cx, cy) = (m[2].floor(), m[5].floor());
    [
        (m[0] - 1.0) as f32,
        m[1] as f32,
        cx as f32,
        (m[2] - cx) as f32,
        m[3] as f32,
        (m[4] - 1.0) as f32,
        cy as f32,
        (m[5] - cy) as f32,
    ]
}

/// Reads the picture (`w` × `h`, rows up) where output pixel `x`, `y`
/// maps to by `m`: between the four pixels around, weighed in truncated
/// 1024ths as VEGAS weighs them; with Transparent nothing from 0.4 of a
/// pixel before the first column or row or 0.6 before the end, the next
/// pixels the edge's past it; else the places mirrored, repeated or held.
pub(crate) fn read(
    src: &[[f32; 4]],
    [w, h]: [usize; 2],
    m: &Affine,
    x: f64,
    y: f64,
    fill: c_int,
) -> [f32; 4] {
    let sx = x * m[0] + y * m[1] + m[2];
    let sy = x * m[3] + y * m[4] + m[5];
    let (wf, hf) = (w as f64, h as f64);
    let (ix, iy, fx, fy, i1, j1) = if fill == 0 {
        let cut = f64::from(-0.4f32);
        if sx <= cut || wf - 0.6 <= sx || sy <= cut || hf - 0.6 <= sy {
            return [0.0; 4];
        }
        let (ix, iy) = (sx as i64, sy as i64);
        let (fx, fy) = ((sx - ix as f64).max(0.0), (sy - iy as f64).max(0.0));
        let (ix, iy) = (ix as usize, iy as usize);
        let i1 = if ix == w - 1 { ix } else { ix + 1 };
        let j1 = if iy == h - 1 { iy } else { iy + 1 };
        (ix, iy, fx, fy, i1, j1)
    } else {
        let (x0, y0) = (sx.floor(), sy.floor());
        let fold = |i: i64, n: usize| -> usize {
            let n = n as i64;
            let k = match fill {
                1 => {
                    let k = i.rem_euclid(2 * n);
                    if k >= n { 2 * n - 1 - k } else { k }
                }
                2 => i.rem_euclid(n),
                _ => i.clamp(0, n - 1),
            };
            k as usize
        };
        let (x0i, y0i) = (x0 as i64, y0 as i64);
        (
            fold(x0i, w),
            fold(y0i, h),
            sx - x0,
            sy - y0,
            fold(x0i + 1, w),
            fold(y0i + 1, h),
        )
    };
    let at = |i: usize, j: usize| src[j * w + i];
    let p00 = at(ix, iy);
    let p10 = at(i1, iy);
    let p01 = at(ix, j1);
    // The diagonal: the row below's when either neighbour is the edge's.
    let p11 = if fill == 0 && (i1 == ix || j1 == iy) {
        p01
    } else {
        at(i1, j1)
    };
    let w00 = ((1.0 - fx) * (1.0 - fy) * 1024.0) as i32 as f32;
    let w10 = (fx * (1.0 - fy) * 1024.0) as i32 as f32;
    let w01 = (fy * (1.0 - fx) * 1024.0) as i32 as f32;
    let w11 = (fy * fx * 1024.0) as i32 as f32;
    std::array::from_fn(|c| {
        (p00[c] * w00 + p10[c] * w10 + p01[c] * w01 + p11[c] * w11) * 0.000_976_562_5
    })
}

/// VEGAS's motion blur: each row's box of `2 r + 1` pixels, the row's ends
/// mirrored without repeating the end, summed as it sums them.
fn blur_rows(data: &mut [[f32; 4]], w: usize, r: usize) {
    if r == 0 || w == 0 {
        return;
    }
    let n = (2 * r + 1) as f32;
    let last = w as isize - 1;
    let at = |row: &[[f32; 4]], k: isize| {
        let k = if k < 0 {
            -k
        } else if k > last {
            2 * last - k
        } else {
            k
        };
        row[k.clamp(0, last) as usize]
    };
    crate::kit::for_chunks(data, w, |_, row| {
        let mut sums = vec![[0.0f32; 4]; w];
        let mut s = [0.0f32; 4];
        for k in 1..=r as isize {
            let p = at(row, k);
            for c in 0..4 {
                s[c] += p[c];
            }
        }
        for c in 0..4 {
            s[c] = s[c] + s[c] + row[0][c];
        }
        sums[0] = s;
        for i in 1..w as isize {
            let (add, sub) = (at(row, i + r as isize), at(row, i - r as isize - 1));
            for c in 0..4 {
                s[c] = ((s[c] + add[c]) - sub[c]) + 0.0;
            }
            sums[i as usize] = s;
        }
        for (px, s) in row.iter_mut().zip(&sums) {
            *px = s.map(|v| v / n);
        }
    });
}

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
    } else if is(action, kOfxImageEffectActionIsIdentity) {
        let time = args.dbl(kOfxPropTime, 0);
        let size = pixels(fx);
        let s = Settings::at(fx, time);
        if s.drawing(time, size)
            .is_none_or(|(m, r)| m == IDENTITY && r == 0)
        {
            out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        // Any part of the picture may show anywhere.
        let time = args.dbl(kOfxPropTime, 0);
        let roi: [f64; 4] = args.dbls(kOfxImageEffectPropRegionOfInterest);
        let all = fx
            .clip(kOfxImageEffectSimpleSourceClipName)
            .rod(time)
            .unwrap_or(roi);
        out.set_dbls(c"OfxImageClipPropRoI_Source", &all);
        kOfxStatOK
    } else if is(action, kOfxActionInstanceChanged) {
        if args.is(kOfxPropType, kOfxTypeParameter) {
            changed(fx, &args.text(kOfxPropName, 0));
        }
        kOfxStatOK
    } else if is(action, kOfxActionCreateInstance) || is(action, kOfxActionBeginInstanceEdit) {
        show(fx);
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

/// Whether there is motion analyzed.
fn analyzed(fx: Fx) -> bool {
    fx.param(HAS_MOTION).int_at(0.0) != 0
        && parse(&fx.param(MOTION).text_at(0.0)).is_some_and(|m| !m.is_empty())
}

/// The frames the effect's clip runs: its first and last as VEGAS rounds
/// them.
fn frame_range(fx: Fx) -> (c_int, c_int) {
    let range: [f64; 2] = fx
        .clip(kOfxImageEffectSimpleSourceClipName)
        .props()
        .map_or([0.0, 0.0], |p| p.dbls(kOfxImageEffectPropFrameRange));
    (
        range[0].round() as c_int,
        (range[1] - 0.25).round() as c_int,
    )
}

/// Shows, hides and greys the controls as the view and the analysis say
/// (VEGAS's Basic, Professional and Expert rules, before and after an
/// analysis), the buttons of the view, and the message while nothing is
/// analyzed.
fn show(fx: Fx) {
    let p = |n: &CStr| fx.param(n);
    let view = p(VIEWS).int_at(0.0);
    let done = analyzed(fx);
    let (first, last) = frame_range(fx);
    let r = p(REFERENCE).props();
    r.set_int(kOfxParamPropMin, first);
    r.set_int(kOfxParamPropMax, last.max(first));
    r.set_int(kOfxParamPropDisplayMin, first);
    r.set_int(kOfxParamPropDisplayMax, last.max(first));
    // Analyzed or not; VEGAS's 2 and 4.
    let state = if done { 2 } else { 4 };
    set_int(p(LAST_STATE), state);
    let hide = |n: &CStr, h: bool| p(n).set_secret(h);
    let enable = |n: &CStr, e: bool| p(n).set_enabled(e);
    let tracking = [
        X_SCALE,
        Y_SCALE,
        LOCATION,
        GRIDS_X,
        GRIDS_Y,
        POINTS_PER_CELL,
        TRACKER_TYPE,
    ];
    match view {
        0 => {
            hide(GROUP_VIEW, true);
            for n in [GROUP_PARAMETERS, PRESETS, COMPENSATION, GROUP_EXPERT] {
                hide(n, true);
            }
            hide(GROUP_SMOOTHING, !done);
        }
        1 => {
            hide(GROUP_VIEW, true);
            for n in [GROUP_PARAMETERS, PRESETS, COMPENSATION] {
                hide(n, false);
            }
            enable(PRESETS, !done);
            enable(COMPENSATION, true);
            hide(GROUP_EXPERT, true);
            hide(GROUP_SMOOTHING, !done);
            hide(GROUP_BORDERS, !done);
            hide(GROUP_BLUR, !done);
        }
        _ => {
            hide(GROUP_VIEW, !done);
            for n in [GROUP_PARAMETERS, PRESETS, COMPENSATION, GROUP_EXPERT] {
                hide(n, false);
            }
            enable(PRESETS, !done);
            enable(COMPENSATION, true);
            enable(GROUP_EXPERT, !done);
            for n in tracking {
                enable(n, !done);
            }
            hide(GROUP_SMOOTHING, !done);
            hide(GROUP_BORDERS, !done);
            hide(GROUP_BLUR, !done);
        }
    }
    if view == 0 {
        hide(GROUP_BORDERS, true);
        hide(GROUP_BLUR, true);
    }
    // Smoothing works once there is something analyzed.
    enable(MAGNITUDE, done);
    hide(GROUP_LOCKS, view != 2 || !done);
    // Show Points goes off with its group.
    if view != 2 || !done {
        set_int(p(SHOW_POINTS), 0);
    }
    // The view's buttons: Stabilize and Reset, or Analyze Motion and
    // Clear Motion Data; the one to analyze until there is something.
    let (go, back) = if view == 0 {
        (ANALYZE_BASIC, CLEAR_BASIC)
    } else {
        (ANALYZE, CLEAR)
    };
    for n in [ANALYZE_BASIC, CLEAR_BASIC, ANALYZE, CLEAR] {
        hide(n, true);
    }
    hide(go, done);
    hide(back, !done);
    // Freeze motion's reference, Smooth motion's smoothing.
    let freeze = p(MODE).int_at(0.0) == 1;
    hide(REFERENCE, !freeze);
    hide(MAGNITUDE, freeze);
    // The motion blur's limits only while it is on.
    let blur = p(BLUR).int_at(0.0) != 0;
    enable(BLUR_THRESHOLD, blur);
    enable(BLUR_MOST, blur);
    lock_rows(fx);
    fx.persistent_message_as(kOfxMessageMessage, (!done).then_some(MUST_CLICK));
}

/// Sets an integer when it changes.
fn set_int(p: crate::ofx::Param, v: c_int) {
    if p.int_at(0.0) != v {
        p.set_int(v);
    }
}

/// The frame's size in pixels (its canonical width over the pixels'
/// aspect).
fn pixels(fx: Fx) -> [f64; 2] {
    let (_, size) = fx.frame();
    let par = fx
        .props()
        .dbl_or(kOfxImageEffectPropProjectPixelAspectRatio, 1.0);
    let par = if par > 0.0 { par } else { 1.0 };
    [size[0] / par, size[1]]
}

/// The Smoothing Parameters Motion Compensation has.
fn lock_rows(fx: Fx) {
    let model = Model::of(fx.param(COMPENSATION).int_at(0.0));
    for (name, has) in LOCKS.iter().zip(model.locks()) {
        fx.param(name).set_secret(!has);
    }
}

/// What a change does.
fn changed(fx: Fx, name: &str) {
    let p = |n: &CStr| fx.param(n);
    let named = |n: &CStr| n.to_str().is_ok_and(|n| n == name);
    let rate = fx.props().dbl_or(kOfxImageEffectPropFrameRate, 30.0);
    if named(CLEAR) || named(CLEAR_BASIC) {
        clear(fx);
    } else if named(ANALYZE) || named(ANALYZE_BASIC) {
        // The analysis's pyramid, as VEGAS sets it for the frame's width.
        let size = pixels(fx);
        let w = size[0];
        let levels = if w >= 240.0 {
            ((w / 240.0).trunc().log2() + 0.5) as c_int + 1
        } else {
            1
        };
        p(LEVELS).set_int(levels);
    } else if named(MOTION) {
        if parse(&p(MOTION).text_at(0.0)).is_some_and(|m| !m.is_empty()) {
            finished(fx, rate, true);
        }
    } else if named(COMPENSATION) {
        p(GEOMETRIC_MODEL).set_int(Model::of(p(COMPENSATION).int_at(0.0)).number());
        lock_rows(fx);
        if analyzed(fx) {
            finished(fx, rate, false);
        }
    } else if named(PRESETS) {
        if p(PRESETS).props().int(kOfxParamPropEnabled, 0) != 0 {
            // Fast: the sums of differences on a 12 × 8 grid; Accurate:
            // both trackers on 14 × 9.
            let accurate = p(PRESETS).int_at(0.0) == 1;
            let kind = if accurate { 2 } else { 1 };
            p(TRACKER_TYPE).set_int(kind);
            p(TRACKER).set_int(kind);
            p(WINDOW_SIZE).set_int(16);
            p(DETECTOR).set_int(c_int::from(accurate));
            p(RANSAC).set_int(1);
            p(GRIDS_X).set_int(if accurate { 14 } else { 12 });
            p(GRIDS_Y).set_int(if accurate { 9 } else { 8 });
        }
    } else if named(TRACKER_TYPE) {
        p(TRACKER).set_int(p(TRACKER_TYPE).int_at(0.0));
    } else if named(MAGNITUDE) {
        p(FRAME_LENGTH).set_int(window_of(p(MAGNITUDE).f64_at(0.0), rate) as c_int);
        smoothing_changed(fx, rate);
    } else if named(MODE) || named(AUTO_ZOOM) || named(REFERENCE) {
        smoothing_changed(fx, rate);
    } else if LOCKS.iter().any(|l| named(l)) {
        if analyzed(fx) {
            keep_corrections(fx, rate);
        }
    } else if named(ADD_SCALE) {
        // A zoom of one's own turns Auto Zoom off.
        if p(AUTO_ZOOM).int_at(0.0) != 0
            && let Some(auto) = zoom_now(fx, rate)
            && (p(ADD_SCALE).f64_at(0.0) - auto).abs() > 0.001
        {
            p(AUTO_ZOOM).set_int(0);
        }
    } else if named(APPLY_EXTERNAL) {
        external(fx, rate);
    }
    show(fx);
}

/// Clear Motion Data and Reset: as before any analysis.
fn clear(fx: Fx) {
    let p = |n: &CStr| fx.param(n);
    p(CLEAR_STATE).set_int(1);
    p(HAS_MOTION).set_int(0);
    p(MOTION).set_text("");
    p(HAS_CORRECTION).set_int(-1);
    p(CORRECTION).set_text("");
    p(TRACK_DATA).set_text("");
    p(AUTO_ZOOM).set_int(1);
    for n in [ADD_SCALE, ADD_ROTATION, ADD_TRANSLATION] {
        if !p(n).key_times().is_empty() {
            p(n).delete_keys();
        }
    }
    p(PRESETS).set_enabled(true);
    p(COMPENSATION).set_enabled(true);
}

/// The smoothing's settings as they are now.
fn smoothing_now(fx: Fx, rate: f64) -> Smoothing {
    let p = |n: &CStr| fx.param(n);
    Smoothing {
        model: Model::of(p(COMPENSATION).int_at(0.0)),
        freeze: p(MODE).int_at(0.0) == 1,
        window: window_of(p(MAGNITUDE).f64_at(0.0), rate),
        reference: p(REFERENCE).int_at(0.0).max(0) as usize,
        locks: LOCKS.map(|l| p(l).int_at(0.0) != 0),
    }
}

/// The zoom Auto Zoom gives now, when there is motion analyzed.
fn zoom_now(fx: Fx, rate: f64) -> Option<f64> {
    let size = pixels(fx);
    let s = smoothing_now(fx, rate);
    let c = corrections_of(&fx.param(MOTION).text_at(0.0), s, size)?;
    let r = if s.freeze { s.reference } else { 0 };
    Some(auto_zoom(&c, s.model, r.min(c.numbers.len() - 1), size))
}

/// The corrections kept in the hidden Correction Data, as VEGAS keeps
/// them.
fn keep_corrections(fx: Fx, rate: f64) {
    let size = pixels(fx);
    let s = smoothing_now(fx, rate);
    if let Some(c) = corrections_of(&fx.param(MOTION).text_at(0.0), s, size) {
        let mut text = format!("bvcorrection 1 {}\n", c.numbers.len());
        for n in &c.numbers {
            text.push_str(&format!(
                "{:.6} {:.6} {:.8} {:.8} {:.8} {:.8}\n",
                n[0], n[1], n[2], n[3], n[4], n[5]
            ));
        }
        fx.param(CORRECTION).set_text(&text);
        fx.param(HAS_CORRECTION).set_int(0);
    }
}

/// Smoothing, Mode, Auto Zoom or the reference changed: the corrections
/// again, and Auto Zoom's zoom written into Additional Zoom.
fn smoothing_changed(fx: Fx, rate: f64) {
    if !analyzed(fx) {
        return;
    }
    keep_corrections(fx, rate);
    let p = |n: &CStr| fx.param(n);
    if p(AUTO_ZOOM).int_at(0.0) != 0
        && let Some(zoom) = zoom_now(fx, rate)
    {
        if !p(ADD_SCALE).key_times().is_empty() {
            p(ADD_SCALE).delete_keys();
        }
        p(ADD_SCALE).set_f64(zoom);
    }
}

/// An analysis came in (or the compensation changed): VEGAS's reference
/// frame (the one needing the least zoom frozen), Smoothing from the
/// window's length, the corrections and Auto Zoom's zoom.
fn finished(fx: Fx, rate: f64, fresh: bool) {
    let p = |n: &CStr| fx.param(n);
    p(HAS_MOTION).set_int(1);
    p(CLEAR_STATE).set_int(0);
    for n in [ADD_SCALE, ADD_ROTATION, ADD_TRANSLATION] {
        if !p(n).key_times().is_empty() {
            p(n).delete_keys();
        }
    }
    let size = pixels(fx);
    let text = p(MOTION).text_at(0.0);
    if fresh && let Some(steps) = parse(&text) {
        let base = smoothing_now(fx, rate);
        let mut best = (0usize, 10.0f64);
        for r in 0..=steps.len() {
            let s = Smoothing {
                freeze: true,
                reference: r,
                ..base
            };
            let c = corrections(&steps, s, size);
            let z = auto_zoom(&c, s.model, r, size);
            if z < best.1 {
                best = (r, z);
            }
        }
        p(REFERENCE).set_int(best.0 as c_int);
    }
    // Smoothing from the window's length in frames, to a tenth of a
    // second.
    let rate_or = if rate < 1.0 { 30.0 } else { rate };
    let frames = p(FRAME_LENGTH).int_at(0.0).max(3);
    let seconds = (f64::from(frames) * 10.0 / rate_or).round() * 0.1;
    p(MAGNITUDE).set_f64(seconds);
    keep_corrections(fx, rate);
    if p(AUTO_ZOOM).int_at(0.0) != 0
        && let Some(zoom) = zoom_now(fx, rate)
    {
        p(ADD_SCALE).set_f64(zoom);
    }
    p(PRESETS).set_enabled(false);
}

/// Apply External Motion: the four corners' keyframes (pixels of a frame
/// External Project Size big, rows from the top) as the motion, each
/// frame's move the affine map taking its corners to the next frame's —
/// only their shift with Apply External Motion 1.
fn external(fx: Fx, rate: f64) {
    let p = |n: &CStr| fx.param(n);
    let apply = p(APPLY_EXTERNAL).int_at(0.0);
    if apply == -1 {
        return;
    }
    let times = p(EXTERNAL[0]).key_times();
    if times.len() < 2 {
        return;
    }
    let size = p(EXTERNAL_SIZE).ij_at(0.0).map(|v| f64::from(v.max(1)));
    let corners = |t: f64| EXTERNAL.map(|e| p(e).xy_at(t));
    let mid = [size[0] / 2.0, size[1] / 2.0];
    let steps: Vec<Step> = times
        .windows(2)
        .map(|t| {
            let (a, b) = (corners(t[0]), corners(t[1]));
            let centre = |q: &[[f64; 2]; 4]| {
                [
                    q.iter().map(|c| c[0]).sum::<f64>() / 4.0,
                    q.iter().map(|c| c[1]).sum::<f64>() / 4.0,
                ]
            };
            let (ca, cb) = (centre(&a), centre(&b));
            let shift = [(cb[0] - ca[0]) / size[0], (cb[1] - ca[1]) / size[0]];
            if apply == 1 {
                return Step::similar(shift[0], shift[1], 0.0, 1.0);
            }
            // The linear part by least squares about the corners' middles.
            let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
            let (mut ux, mut uy, mut vx, mut vy) = (0.0, 0.0, 0.0, 0.0);
            for (pa, pb) in a.iter().zip(&b) {
                let (x, y) = (pa[0] - ca[0], pa[1] - ca[1]);
                let (u, v) = (pb[0] - cb[0], pb[1] - cb[1]);
                sxx += x * x;
                sxy += x * y;
                syy += y * y;
                ux += u * x;
                uy += u * y;
                vx += v * x;
                vy += v * y;
            }
            let det = sxx * syy - sxy * sxy;
            if det.abs() < 1e-12 {
                return Step::similar(shift[0], shift[1], 0.0, 1.0);
            }
            let l = [
                (ux * syy - uy * sxy) / det,
                (uy * sxx - ux * sxy) / det,
                (vx * syy - vy * sxy) / det,
                (vy * sxx - vx * sxy) / det,
            ];
            // The move about the frame's middle.
            let off = [
                cb[0] - mid[0] - (l[0] * (ca[0] - mid[0]) + l[1] * (ca[1] - mid[1])),
                cb[1] - mid[1] - (l[2] * (ca[0] - mid[0]) + l[3] * (ca[1] - mid[1])),
            ];
            let turn = (l[2] - l[1]).atan2(l[0] + l[3]);
            let scale = (l[0] * l[3] - l[1] * l[2]).abs().sqrt();
            Step {
                dx: off[0] / size[0],
                dy: off[1] / size[0],
                turn,
                scale,
                linear: l,
            }
        })
        .collect();
    p(MOTION).set_text(&motion_text(&steps));
    finished(fx, rate, true);
    p(APPLY_EXTERNAL).set_int(-1);
}

fn describe(fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, c"Video Stabilization");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Utility");
    p.set_str(kOfxPropPluginDescription, c"Stabilizes video motion");
    p.set_strs(
        kOfxImageEffectPropSupportedContexts,
        &[kOfxImageEffectContextFilter],
    );
    p.set_strs(
        kOfxImageEffectPropSupportedPixelDepths,
        &[kOfxBitDepthFloat, kOfxBitDepthShort, kOfxBitDepthByte],
    );
    p.set_int(kOfxImageEffectPropSupportsTiles, 0);
    p.set_int(kOfxImageEffectPropSupportsMultiResolution, 1);
    p.set_int(kOfxImageEffectPluginPropHostFrameThreading, 0);
    p.set_str(
        kOfxImageEffectPluginRenderThreadSafety,
        kOfxImageEffectRenderFullySafe,
    );
    p.set_ptr(
        kOfxImageEffectPluginPropOverlayInteractV2,
        overlay as *mut c_void,
    );
}

/// A parameter's definition, its group and its flags.
struct Def<'a> {
    fx: Fx,
    group: Option<&'a CStr>,
}

impl Def<'_> {
    fn param(&self, ty: &CStr, name: &CStr, label: &CStr, hint: Option<&CStr>) -> Props {
        let p = self.fx.define_param(ty, name, label);
        if let Some(g) = self.group {
            p.set_str(kOfxParamPropParent, g);
        }
        if let Some(h) = hint {
            p.set_str(kOfxParamPropHint, h);
        }
        p
    }

    fn choice(
        &self,
        name: &CStr,
        label: &CStr,
        options: &[&CStr],
        default: c_int,
        hint: Option<&CStr>,
    ) -> Props {
        let p = self.param(kOfxParamTypeChoice, name, label, hint);
        p.set_strs(kOfxParamPropChoiceOption, options);
        p.set_int(kOfxParamPropDefault, default);
        p.set_int(kOfxParamPropAnimates, 0);
        p
    }

    fn boolean(&self, name: &CStr, label: &CStr, default: bool, hint: Option<&CStr>) -> Props {
        let p = self.param(kOfxParamTypeBoolean, name, label, hint);
        p.set_int(kOfxParamPropDefault, c_int::from(default));
        p.set_int(kOfxParamPropAnimates, 0);
        p
    }

    /// A double: its default, its range and the slider's, its step and
    /// decimals.
    fn double(
        &self,
        name: &CStr,
        label: &CStr,
        (default, min, max): (f64, f64, f64),
        (low, high, step, digits): (f64, f64, f64, c_int),
        hint: Option<&CStr>,
    ) -> Props {
        let p = self.param(kOfxParamTypeDouble, name, label, hint);
        p.set_dbl(kOfxParamPropDefault, default);
        p.set_dbl(kOfxParamPropMin, min);
        p.set_dbl(kOfxParamPropMax, max);
        p.set_dbl(kOfxParamPropDisplayMin, low);
        p.set_dbl(kOfxParamPropDisplayMax, high);
        p.set_dbl(kOfxParamPropIncrement, step);
        p.set_int(kOfxParamPropDigits, digits);
        p.set_int(kOfxParamPropAnimates, 0);
        p
    }

    fn int(
        &self,
        name: &CStr,
        label: &CStr,
        (default, min, max): (c_int, c_int, c_int),
        (low, high): (c_int, c_int),
        hint: Option<&CStr>,
    ) -> Props {
        let p = self.param(kOfxParamTypeInteger, name, label, hint);
        p.set_int(kOfxParamPropDefault, default);
        p.set_int(kOfxParamPropMin, min);
        p.set_int(kOfxParamPropMax, max);
        p.set_int(kOfxParamPropDisplayMin, low);
        p.set_int(kOfxParamPropDisplayMax, high);
        p.set_int(kOfxParamPropAnimates, 0);
        p
    }

    /// A hidden integer of the analysis's own.
    fn hidden_int(&self, name: &CStr, default: c_int) -> Props {
        let p = self.int(name, c"", (default, c_int::MIN, c_int::MAX), (0, 100), None);
        p.set_int(kOfxParamPropSecret, 1);
        p
    }

    fn group(&self, name: &CStr, label: &CStr, hint: Option<&CStr>) -> Props {
        let p = self.param(kOfxParamTypeGroup, name, label, hint);
        p.set_int(kOfxParamPropGroupOpen, 1);
        p
    }

    fn text(&self, name: &CStr) -> Props {
        let p = self.param(kOfxParamTypeString, name, c"", None);
        p.set_str(kOfxParamPropDefault, c"");
        p.set_int(kOfxParamPropAnimates, 0);
        p.set_int(kOfxParamPropSecret, 1);
        p
    }
}

/// An analyzing button: what the host analyzes and how.
fn analyzing(p: Props) {
    use bv_ofx_sys::extensions as x;
    p.set_str(x::kBananaVParamPropAnalysis, c"Motion");
    p.set_str(x::kBananaVParamPropAnalysisParam, MOTION);
    p.set_str(x::kBananaVParamPropAnalysisMethod, PRESETS);
    p.set_str(
        x::kBananaVParamPropAnalysisRegion,
        c"location xScale yScale",
    );
    p.set_str(
        x::kBananaVParamPropAnalysisGrid,
        c"numGridsX numGridsY pointsPerGridCell",
    );
    p.set_str(x::kBananaVParamPropAnalysisFollow, TRACKER_TYPE);
    p.set_str(x::kBananaVParamPropAnalysisReduction, RESOLUTION);
}

fn describe_in_context(fx: Fx) {
    let comps = [kOfxImageComponentRGBA, kOfxImageComponentRGB];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    fx.define_clip(kOfxImageEffectOutputClipName, &[kOfxImageComponentRGBA]);
    let top = Def { fx, group: None };
    let p = top.boolean(HAS_MOTION, c"", false, None);
    p.set_int(kOfxParamPropSecret, 1);
    top.choice(
        VIEWS,
        c"User View",
        &VIEW_NAMES,
        0,
        Some(c"Switches the different user views."),
    );
    top.group(
        GROUP_PARAMETERS,
        c"Parameters",
        Some(c"Shows additional information about the parameters before stabilization."),
    )
    .set_int(kOfxParamPropSecret, 1);
    let g = Def {
        fx,
        group: Some(GROUP_PARAMETERS),
    };
    g.choice(
        PRESETS,
        c"Method",
        &METHODS,
        1,
        Some(c"This choice box affects the motion tracking algorithm, the way trackable points are determined and which motion tracks are concerned for motion modeling."),
    )
    .set_int(kOfxParamPropSecret, 1);
    g.choice(
        COMPENSATION,
        c"Motion Compensation",
        &COMPENSATIONS,
        2,
        Some(c"This choice box affects the motion modeling algorithm."),
    )
    .set_int(kOfxParamPropSecret, 1);
    top.group(
        GROUP_EXPERT,
        c"Expert Mode",
        Some(c"Shows additional information to adjust the tracking algorithm."),
    )
    .set_int(kOfxParamPropSecret, 1);
    let g = Def {
        fx,
        group: Some(GROUP_EXPERT),
    };
    g.double(
        X_SCALE,
        c"Width (Region of Interest)",
        (0.85, 0.01, 1.0),
        (0.01, 1.0, 0.1, 3),
        Some(c"This parameter shrinks the width of the region of interest."),
    );
    g.double(
        Y_SCALE,
        c"Height (Region of Interest)",
        (0.8, 0.01, 1.0),
        (0.01, 1.0, 0.1, 3),
        Some(c"This parameter shrinks the height of the region of interest."),
    );
    let p = g.param(kOfxParamTypeDouble2D, LOCATION, c"Location", None);
    p.set_dbls(kOfxParamPropDefault, &[0.5, 0.5]);
    p.set_dbls(kOfxParamPropMin, &[-1.0, -1.0]);
    p.set_dbls(kOfxParamPropMax, &[2.0, 2.0]);
    p.set_dbls(kOfxParamPropDisplayMin, &[0.0, 0.0]);
    p.set_dbls(kOfxParamPropDisplayMax, &[1.0, 1.0]);
    p.set_dbl(kOfxParamPropIncrement, 0.1);
    p.set_int(kOfxParamPropDigits, 3);
    p.set_int(kOfxParamPropAnimates, 0);
    g.int(
        GRIDS_X,
        c"Number of Grids in X",
        (14, 4, 50),
        (4, 20),
        Some(c"This parameter changes the number of grids in X."),
    );
    g.int(
        GRIDS_Y,
        c"Number of Grids in Y",
        (9, 4, 50),
        (4, 20),
        Some(c"This parameter changes the number of grids in Y."),
    );
    g.int(
        POINTS_PER_CELL,
        c"Number of Points per Grid Cell",
        (2, 2, 20),
        (2, 20),
        Some(c"This parameter changes the number of points per cell."),
    );
    g.choice(
        TRACKER_TYPE,
        c"Tracker Type",
        &TRACKERS,
        0,
        Some(c"This choice box affects the type of the tracker."),
    );
    g.choice(RESOLUTION, c"Resolution", &RESOLUTIONS, 1, None)
        .set_int(kOfxParamPropSecret, 1);
    let p = top.param(
        kOfxParamTypePushButton,
        ANALYZE_BASIC,
        c"Stabilize",
        Some(c"Starts the stabilization."),
    );
    analyzing(p);
    top.param(
        kOfxParamTypePushButton,
        CLEAR_BASIC,
        c"Reset",
        Some(c"Clears the results of stabilization."),
    )
    .set_int(kOfxParamPropSecret, 1);
    let p = top.param(
        kOfxParamTypePushButton,
        ANALYZE,
        c"Analyze Motion",
        Some(c"Starts the motion analysis."),
    );
    analyzing(p);
    p.set_int(kOfxParamPropSecret, 1);
    top.param(
        kOfxParamTypePushButton,
        CLEAR,
        c"Clear Motion Data",
        Some(c"Clears the results of motion analysis."),
    )
    .set_int(kOfxParamPropSecret, 1);
    top.group(
        GROUP_SMOOTHING,
        c"Stabilization options",
        Some(c"Shows additional information about the stabilization/smoothing process."),
    )
    .set_int(kOfxParamPropSecret, 1);
    let g = Def {
        fx,
        group: Some(GROUP_SMOOTHING),
    };
    g.choice(MODE, c"Mode", &MODES, 0, None);
    g.double(
        MAGNITUDE,
        c"Smoothing",
        (2.5, 0.1, 1000.0),
        (0.1, 10.0, 0.1, 1),
        Some(c"The Smoothing Magnitude (in seconds) is the parameter that helps to define how much \"unsteadiness\" should be kept in a scene, the higher that factor is, the more the movements are smoothed."),
    );
    g.int(
        REFERENCE,
        c"Reference Frame Number",
        (0, 0, 0),
        (0, 100),
        Some(c"This parameter changes the reference frame around the video motion is stabilized."),
    );
    g.group(
        GROUP_LOCKS,
        c"Smoothing Parameters",
        Some(c"Shows additional information to adjust the smoothing parameters."),
    )
    .set_int(kOfxParamPropSecret, 1);
    let g = Def {
        fx,
        group: Some(GROUP_LOCKS),
    };
    for (name, label, hint) in [
        (LOCKS[0], c"Translate X", c"Smoothing the translation in X."),
        (LOCKS[1], c"Translate Y", c"Smoothing the translation in Y."),
        (
            LOCKS[2],
            c"Rotation Angle",
            c"Smoothing the rotation angle.",
        ),
        (LOCKS[3], c"Scale X", c"Smoothing the scale in X."),
        (LOCKS[4], c"Scale Y", c"Smoothing the scale in Y."),
        (LOCKS[5], c"Shear", c"Smoothing the shear."),
    ] {
        g.boolean(name, label, true, Some(hint))
            .set_int(kOfxParamPropAnimates, 1);
    }
    top.group(
        GROUP_BORDERS,
        c"Borders",
        Some(c"Shows additional information about the borders."),
    )
    .set_int(kOfxParamPropSecret, 1);
    let g = Def {
        fx,
        group: Some(GROUP_BORDERS),
    };
    g.boolean(
        AUTO_ZOOM,
        c"Auto Zoom",
        true,
        Some(c"Click to automatically zoom in to avoid black borders."),
    );
    g.double(
        ADD_SCALE,
        c"Additional Zoom",
        (0.0, 0.0, 1.0),
        (0.0, 0.5, 0.01, 3),
        Some(c"Adds an offset to the zoom. You can zoom in to avoid black border or focus on the center of the image."),
    )
    .set_int(kOfxParamPropAnimates, 1);
    let p = g.double(
        ADD_ROTATION,
        c"Additional Rotation",
        (0.0, -180.0, 180.0),
        (-180.0, 180.0, 0.1, 1),
        Some(c"Adds an offset to the rotation. The Rotation (angle) parameter can be used to straighten the horizon."),
    );
    p.set_str(kOfxParamPropDoubleType, kOfxParamDoubleTypeAngle);
    p.set_int(kOfxParamPropAnimates, 1);
    let p = g.param(
        kOfxParamTypeDouble2D,
        ADD_TRANSLATION,
        c"Additional Translation",
        Some(c"Adds an offset to the position."),
    );
    p.set_dbls(kOfxParamPropDefault, &[0.5, 0.5]);
    p.set_dbls(kOfxParamPropMin, &[0.0, 0.0]);
    p.set_dbls(kOfxParamPropMax, &[1.0, 1.0]);
    p.set_dbls(kOfxParamPropDisplayMin, &[0.0, 0.0]);
    p.set_dbls(kOfxParamPropDisplayMax, &[1.0, 1.0]);
    p.set_dbl(kOfxParamPropIncrement, 0.001);
    p.set_int(kOfxParamPropDigits, 3);
    g.choice(
        FILLING,
        c"Edge Filling",
        &FILLS,
        0,
        Some(c"This choice box affects the filling of the background."),
    )
    .set_int(kOfxParamPropAnimates, 1);
    let p = g.param(
        kOfxParamTypeRGBA,
        FILL_COLOR,
        c"Color",
        Some(c"Selects the color for filling."),
    );
    p.set_dbls(kOfxParamPropDefault, &[0.0, 0.0, 1.0, 1.0]);
    p.set_int(kOfxParamPropSecret, 1);
    top.group(OFFSET, c"Offset", None)
        .set_int(kOfxParamPropSecret, 1);
    top.group(
        GROUP_BLUR,
        c"Motion Blur",
        Some(c"Shows additional information about the motion blur."),
    )
    .set_int(kOfxParamPropSecret, 1);
    let g = Def {
        fx,
        group: Some(GROUP_BLUR),
    };
    g.boolean(
        BLUR,
        c"Add Motion Blur",
        false,
        Some(c"Click to add Motion Blur to horizontal camera pans."),
    )
    .set_int(kOfxParamPropAnimates, 1);
    let p = g.double(
        BLUR_THRESHOLD,
        c"Threshold",
        (10.0, 1.0, 1000.0),
        (1.0, 100.0, 1.0, 0),
        Some(c"Motion Blur is only applied if the detected horizontal camera pans exceeds that threshold."),
    );
    p.set_int(kOfxParamPropAnimates, 1);
    p.set_int(kOfxParamPropEnabled, 0);
    let p = g.double(
        BLUR_MOST,
        c"Maximal Magnitude",
        (19.0, 0.0, 1000.0),
        (0.0, 39.0, 1.0, 0),
        None,
    );
    p.set_int(kOfxParamPropAnimates, 1);
    p.set_int(kOfxParamPropEnabled, 0);
    p.set_int(kOfxParamPropSecret, 1);
    top.group(
        GROUP_VIEW,
        c"Visualization",
        Some(c"Shows additional information about the visualization."),
    )
    .set_int(kOfxParamPropSecret, 1);
    let g = Def {
        fx,
        group: Some(GROUP_VIEW),
    };
    g.boolean(
        SHOW_POINTS,
        c"Show Points",
        false,
        Some(c"Shows the tracking points."),
    );
    g.int(
        MIN_TRACK,
        c"Minimum track length",
        (1, 1, 1000),
        (1, 100),
        Some(c"This parameter determines which tracking points are shown."),
    );
    for name in EXTERNAL {
        let p = top.param(kOfxParamTypeDouble2D, name, c"", None);
        p.set_dbls(kOfxParamPropDefault, &[0.0, 0.0]);
        p.set_dbls(kOfxParamPropDisplayMin, &[0.0, 0.0]);
        p.set_dbls(kOfxParamPropDisplayMax, &[1.0, 1.0]);
        p.set_int(kOfxParamPropDigits, 2);
        p.set_int(kOfxParamPropSecret, 1);
    }
    let p = top.param(kOfxParamTypeInteger2D, EXTERNAL_SIZE, c"", None);
    p.set_ints(kOfxParamPropDefault, &[1, 1]);
    p.set_int(kOfxParamPropSecret, 1);
    top.hidden_int(APPLY_EXTERNAL, -1)
        .set_int(kOfxParamPropAnimates, 1);
    for (name, default) in [
        (LAST_STATE, 0),
        (GEOMETRIC_MODEL, 4),
        (TRACKER, 2),
        (WINDOW_SIZE, 16),
        (LEVELS, 4),
        (DETECTOR, 0),
        (POINT_HANDLING, 1),
        (RANSAC, 0),
        (SMOOTHING, 0),
        (TEMPO, 1),
        (FRAME_LENGTH, 75),
    ] {
        top.hidden_int(name, default);
    }
    let p = top.boolean(
        STATIC_CAMERA,
        c"Static Camera",
        false,
        Some(c"Switches the mode between smoothing motion ('Static Camera' is disabled) and fixing motion ('Static Camera' is enabled)."),
    );
    p.set_int(kOfxParamPropSecret, 1);
    top.boolean(CLEAR_STATE, c"", false, None)
        .set_int(kOfxParamPropSecret, 1);
    top.text(MOTION);
    top.hidden_int(HAS_CORRECTION, -1);
    top.text(CORRECTION);
    let p = top.double(
        FRAME_NO,
        c"",
        (0.0, f64::MIN, f64::MAX),
        (0.0, 1.0, 0.1, 2),
        None,
    );
    p.set_int(kOfxParamPropSecret, 1);
    p.set_int(kOfxParamPropAnimates, 1);
    top.text(TRACK_DATA);
}

fn render(fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let scale: [f64; 2] = args.dbls(kOfxImageEffectPropRenderScale);
    let scale = scale.map(|v| if v > 0.0 { v } else { 1.0 });
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let src = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time);
    if let Some(s) = &src
        && (s.depth != out.depth || s.channels != out.channels)
    {
        return kOfxStatErrImageFormat;
    }
    let (origin, _) = fx.frame();
    let par = out.par;
    let full = pixels(fx);
    let source = Buf::read(src.as_ref());
    drop(src);
    let s = Settings::at(fx, time);
    let result = match s.drawing(time, full) {
        Some((m, reach)) if !(m == IDENTITY && reach == 0) && !source.data.is_empty() => {
            // The map in this render's pixels from the frame's corner.
            let render = [full[0] * scale[0], full[1] * scale[1]];
            let n = normalized(&m, full);
            let m = [
                n[0],
                n[1] * render[0] / render[1],
                n[2] * render[0],
                n[3] * render[1] / render[0],
                n[4],
                n[5] * render[1],
            ];
            let o = [
                (origin[0] * scale[0] / par).round() as c_int,
                (origin[1] * scale[1]).round() as c_int,
            ];
            let size = [
                source.width().max(0) as usize,
                source.height().max(0) as usize,
            ];
            let [bx, by] = [source.bounds[0] - o[0], source.bounds[1] - o[1]];
            // The source's pixels from the frame's corner, as the map reads
            // them.
            let m = [
                m[0],
                m[1],
                m[2] - f64::from(bx),
                m[3],
                m[4],
                m[5] - f64::from(by),
            ];
            let mut warped = source.map(|x, y| {
                let (fx, fy) = (f64::from(x - o[0]), f64::from(y - o[1]));
                read(&source.data, size, &m, fx, fy, s.fill)
            });
            let r = (f64::from(reach) * scale[0]).round() as usize;
            let w = warped.width().max(0) as usize;
            blur_rows(&mut warped.data, w, r);
            warped
        }
        _ => source,
    };
    crate::compositors::write(&out, &result, (x1, y1, x2, y2));
    drop(out);
    kOfxStatOK
}

/// The overlay's state: what the pen holds and double clicks.
#[derive(Debug, Default)]
struct Overlay {
    drag: Option<Grab>,
    hover: bool,
    clicks: Clicks,
}

/// What the pen holds: the region (and where in it), or one of its edges'
/// handles (0 left, 1 bottom, 2 right, 3 top).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Grab {
    Region([f64; 2]),
    Edge(usize),
}

/// The region of interest's box in parts of the frame (rows up): its
/// left, bottom, right and top, as VEGAS lays it out from Location and
/// the widths.
fn region(fx: Fx) -> [f64; 4] {
    let l = fx.param(LOCATION).xy_at(0.0);
    let (w, h) = (fx.param(X_SCALE).f64_at(0.0), fx.param(Y_SCALE).f64_at(0.0));
    [
        l[0] - w / 2.0,
        l[1] - h / 2.0,
        l[0] + w / 2.0,
        l[1] + h / 2.0,
    ]
}

/// Each edge's handle (parts of the frame): left, bottom, right, top.
fn handles(r: &[f64; 4]) -> [[f64; 2]; 4] {
    let (mx, my) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
    [[r[0], my], [mx, r[1]], [r[2], my], [mx, r[3]]]
}

/// The overlay's entry point.
unsafe extern "C" fn overlay(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    if is(action, kOfxActionDescribe) {
        return kOfxStatOK;
    }
    let Some(props) = interact_props(handle) else {
        return kOfxStatReplyDefault;
    };
    if is(action, kOfxActionCreateInstance) {
        let state = Box::into_raw(Box::new(Overlay::default()));
        props.set_ptr(kOfxPropInstanceData, state as *mut c_void);
        return kOfxStatOK;
    }
    let state = props.ptr(kOfxPropInstanceData) as *mut Overlay;
    if is(action, kOfxActionDestroyInstance) {
        if !state.is_null() {
            // SAFETY: made above, freed once.
            drop(unsafe { Box::from_raw(state) });
            props.set_ptr(kOfxPropInstanceData, std::ptr::null_mut());
        }
        return kOfxStatOK;
    }
    if state.is_null() {
        return kOfxStatReplyDefault;
    }
    // SAFETY: this overlay's state; hosts run one of its actions at a time.
    let state = unsafe { &mut *state };
    let args = Props(in_args);
    let fx = Fx(args.ptr(kOfxPropEffectInstance) as OfxImageEffectHandle);
    if fx.0.is_null() {
        return kOfxStatReplyDefault;
    }
    // Only Expert's, before the analysis (while the region can change).
    let editing = fx.param(VIEWS).int_at(0.0) >= 2
        && fx.param(X_SCALE).props().int(kOfxParamPropEnabled, 0) != 0
        && fx.param(GROUP_EXPERT).props().int(kOfxParamPropSecret, 0) == 0;
    if !editing {
        state.drag = None;
        return kOfxStatReplyDefault;
    }
    let ps: [f64; 2] = args.dbls(kOfxInteractPropPixelScale);
    let ps = ps.map(|v| if v.abs() > 1e-12 { v.abs() } else { 1.0 });
    let (origin, size) = fx.frame();
    let parts = |c: [f64; 2]| [(c[0] - origin[0]) / size[0], (c[1] - origin[1]) / size[1]];
    let canonical = |p: [f64; 2]| [origin[0] + p[0] * size[0], origin[1] + p[1] * size[1]];
    let r = region(fx);
    if is(action, kOfxInteractActionDraw) {
        draw(args, fx, state, &r, &canonical, ps);
        return kOfxStatOK;
    }
    let pen_c: [f64; 2] = args.dbls(kOfxInteractPropPenPosition);
    let pen = parts(pen_c);
    // Within six pixels of a handle (VEGAS's twelve-pixel boxes).
    let reach = [6.0 * ps[0] / size[0], 6.0 * ps[1] / size[1]];
    let on = |h: [f64; 2]| (pen[0] - h[0]).abs() <= reach[0] && (pen[1] - h[1]).abs() <= reach[1];
    let edge = handles(&r).iter().position(|h| on(*h));
    let inside = pen[0] >= r[0] && pen[0] <= r[2] && pen[1] >= r[1] && pen[1] <= r[3];
    let p = |n: &CStr| fx.param(n);
    if is(action, kOfxInteractActionPenDown) {
        if edge.is_none() && !inside {
            return kOfxStatReplyDefault;
        }
        let screen = [pen_c[0] / ps[0], pen_c[1] / ps[1]];
        let double = state.clicks.press(screen, Instant::now());
        match edge {
            // A double click on a handle: the whole width or height.
            Some(e) if double => {
                p(if e % 2 == 0 { X_SCALE } else { Y_SCALE }).set_f64(1.0);
                state.drag = None;
            }
            Some(e) => state.drag = Some(Grab::Edge(e)),
            // A double click inside: the region back in the middle.
            None if double => {
                p(LOCATION).set_xy([0.5, 0.5]);
                state.drag = None;
            }
            None => {
                let l = p(LOCATION).xy_at(0.0);
                state.drag = Some(Grab::Region([pen[0] - l[0], pen[1] - l[1]]));
            }
        }
        redraw(handle);
        kOfxStatOK
    } else if is(action, kOfxInteractActionPenMotion) {
        match state.drag {
            Some(Grab::Region(off)) => {
                p(LOCATION).set_xy([pen[0] - off[0], pen[1] - off[1]]);
            }
            Some(Grab::Edge(e)) => {
                // The edge follows the pen, the region about its middle.
                let l = p(LOCATION).xy_at(0.0);
                let (name, d) = if e % 2 == 0 {
                    (X_SCALE, (pen[0] - l[0]).abs())
                } else {
                    (Y_SCALE, (pen[1] - l[1]).abs())
                };
                p(name).set_f64((2.0 * d).clamp(0.01, 1.0));
            }
            None => {
                let hover = edge.is_some() || inside;
                if hover != state.hover {
                    state.hover = hover;
                    redraw(handle);
                }
                return kOfxStatReplyDefault;
            }
        }
        redraw(handle);
        kOfxStatOK
    } else if is(action, kOfxInteractActionPenUp) {
        if state.drag.take().is_some() {
            redraw(handle);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else {
        kOfxStatReplyDefault
    }
}

/// VEGAS's colors: the analysis grid's orange lines, the region's cells'
/// light green, the region's outline (dark gray, gray under the pen, light
/// green moved, light red sized) and its handles' yellow.
const GRID: [f32; 4] = [1.0, 128.0 / 255.0, 0.0, 100.0 / 255.0];
const CELLS: [f32; 4] = [128.0 / 255.0, 1.0, 0.0, 200.0 / 255.0];
const IDLE: [f32; 4] = [64.0 / 255.0, 64.0 / 255.0, 64.0 / 255.0, 1.0];
const HOVER: [f32; 4] = [128.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0, 1.0];
const MOVED: [f32; 4] = [128.0 / 255.0, 1.0, 128.0 / 255.0, 1.0];
const SIZED: [f32; 4] = [1.0, 128.0 / 255.0, 128.0 / 255.0, 1.0];
const HANDLE: [f32; 4] = [1.0, 1.0, 0.0, 1.0];

fn draw(
    args: Props,
    fx: Fx,
    state: &Overlay,
    r: &[f64; 4],
    canonical: &dyn Fn([f64; 2]) -> [f64; 2],
    ps: [f64; 2],
) {
    let Some(d) = Draw::of(args) else { return };
    let cols = fx.param(GRIDS_X).int_at(0.0).max(1) as usize;
    let rows = fx.param(GRIDS_Y).int_at(0.0).max(1) as usize;
    d.width(1.0);
    // The grid over the whole frame.
    let mut lines = Vec::new();
    for i in 0..=cols {
        let x = i as f64 / cols as f64;
        lines.push(canonical([x, 0.0]));
        lines.push(canonical([x, 1.0]));
    }
    for j in 0..=rows {
        let y = j as f64 / rows as f64;
        lines.push(canonical([0.0, y]));
        lines.push(canonical([1.0, y]));
    }
    d.colour(GRID);
    d.lines(&lines);
    // The cells the region takes, as the analysis counts them.
    let span = |lo: f64, hi: f64, n: usize| {
        let a = (n as f64 * lo.max(0.0)).round() as usize;
        let b = (n as f64 * hi.min(1.0)).round() as usize;
        (a.min(n), b.min(n))
    };
    let (c0, c1) = span(r[0], r[2], cols);
    let (r0, r1) = span(r[1], r[3], rows);
    if c0 < c1 && r0 < r1 {
        let mut cells = Vec::new();
        let (y0, y1) = (r0 as f64 / rows as f64, r1 as f64 / rows as f64);
        let (x0, x1) = (c0 as f64 / cols as f64, c1 as f64 / cols as f64);
        for i in c0..=c1 {
            let x = i as f64 / cols as f64;
            cells.push(canonical([x, y0]));
            cells.push(canonical([x, y1]));
        }
        for j in r0..=r1 {
            let y = j as f64 / rows as f64;
            cells.push(canonical([x0, y]));
            cells.push(canonical([x1, y]));
        }
        d.colour(CELLS);
        d.lines(&cells);
    }
    let outline = match state.drag {
        Some(Grab::Region(_)) => MOVED,
        Some(Grab::Edge(_)) => SIZED,
        None if state.hover => HOVER,
        None => IDLE,
    };
    d.colour(outline);
    d.outline(&[
        canonical([r[0], r[1]]),
        canonical([r[2], r[1]]),
        canonical([r[2], r[3]]),
        canonical([r[0], r[3]]),
    ]);
    for (k, h) in handles(r).iter().enumerate() {
        let c = canonical(*h);
        let ring = |radius: f64| -> Vec<[f64; 2]> {
            (0..16)
                .map(|i| {
                    let t = std::f64::consts::TAU * f64::from(i) / 16.0;
                    [
                        c[0] + radius * ps[0] * t.cos(),
                        c[1] + radius * ps[1] * t.sin(),
                    ]
                })
                .collect()
        };
        d.colour(HANDLE);
        d.fill(&ring(6.0));
        d.colour(outline);
        d.outline(&ring(6.0));
        if state.drag == Some(Grab::Edge(k)) {
            d.colour([0.0, 0.0, 0.0, 1.0]);
            d.fill(&ring(3.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shake(n: usize, amp: f64) -> Vec<Step> {
        (0..n)
            .map(|i| Step::similar(if i % 2 == 0 { amp } else { -amp }, 0.0, 0.0, 1.0))
            .collect()
    }

    const SIZE: [f64; 2] = [320.0, 180.0];

    fn smooth(model: Model) -> Smoothing {
        Smoothing {
            model,
            freeze: false,
            window: 75,
            reference: 0,
            locks: [true; 6],
        }
    }

    #[test]
    fn the_motion_keeps_as_text() {
        let steps = vec![
            Step::similar(0.01, -0.002, 0.001, 1.002),
            Step {
                linear: [1.01, 0.02, -0.01, 0.99],
                ..Step::similar(-0.004, 0.0, 0.0, 1.0)
            },
        ];
        let back = parse(&motion_text(&steps)).unwrap();
        assert_eq!(back.len(), 2);
        assert!((back[0].dx - 0.01).abs() < 1e-9 && (back[1].dx + 0.004).abs() < 1e-9);
        assert!((back[1].linear[1] - 0.02).abs() < 1e-9);
        // The first version's lines: a shift, turn and scale.
        let old = parse("bvmotion 1 1\n0.01 0 0.1 1\n").unwrap();
        assert!((old[0].linear[2] - 0.1f64.sin()).abs() < 1e-9);
        assert!(parse("something else").is_none());
    }

    #[test]
    fn the_window_is_blackmans_and_mirrors_the_ends() {
        let (w, sum) = blackman(5);
        // 0, 0.34, 1, 0.34, 0: Blackman's five.
        assert!(w[0].abs() < 1e-6 && (w[2] - 1.0).abs() < 1e-6, "{w:?}");
        assert!((w[1] - 0.34).abs() < 1e-6 && (sum - 1.68).abs() < 1e-5);
        // A ramp stays a ramp: its ends mirrored, not held.
        let ramp: Vec<f32> = (0..20).map(|i| i as f32).collect();
        let s = smoothed(&ramp, 5);
        assert!((s[10] - 10.0).abs() < 1e-4);
        // The first value: its neighbours mirrored about it.
        let x0 = (0.34 * 1.0 + 1.0 * 0.0 + 0.34 * 1.0) / 1.68;
        assert!((s[0] - x0).abs() < 1e-5, "{}", s[0]);
        // The window no longer than the values, and odd.
        let short = smoothed(&[1.0, 5.0, 1.0, 5.0], 75);
        assert_eq!(short.len(), 4);
        assert_eq!(smoothed(&[1.0, 2.0], 75), vec![1.0, 2.0]);
        assert_eq!(window_of(2.5, 29.97), 75);
        assert_eq!(window_of(2.5, 30.0), 75);
        assert_eq!(window_of(0.1, 25.0), 3);
    }

    #[test]
    fn a_shake_is_taken_out() {
        // A camera shaking left and right two pixels: smoothed, every frame
        // comes back near the middle.
        let steps = shake(60, 2.0 / SIZE[0]);
        let c = corrections(&steps, smooth(Model::Similar), SIZE);
        // Frame 1 (moved right by 2) reads about 2 pixels to its right:
        // the smoothed moves hardly go anywhere.
        let m = matrix(&c.numbers[1], Model::Similar, Additional::default(), SIZE);
        assert!((m[2] - 2.0).abs() < 0.3, "{m:?}");
        // Freeze motion: every frame back to the reference's view.
        let f = corrections(
            &steps,
            Smoothing {
                freeze: true,
                reference: 0,
                ..smooth(Model::Similar)
            },
            SIZE,
        );
        for k in 0..f.numbers.len() {
            let expected = if k % 2 == 1 { 2.0 } else { 0.0 };
            assert!(
                (f.numbers[k][0] - expected).abs() < 1e-6,
                "{k}: {:?}",
                f.numbers[k]
            );
        }
        // An unticked component stays as filmed.
        let mut s = smooth(Model::Similar);
        s.locks[0] = false;
        let u = corrections(&steps, s, SIZE);
        assert!(u.numbers.iter().all(|n| n[0].abs() < 1e-9));
    }

    #[test]
    fn a_turn_is_corrected_about_the_middle() {
        // Turning 1° a frame, frozen at the first: the frame after reads its
        // middle where it was.
        let steps = vec![Step::similar(0.0, 0.0, 1f64.to_radians(), 1.0); 5];
        let s = Smoothing {
            freeze: true,
            ..smooth(Model::Rotation)
        };
        let c = corrections(&steps, s, SIZE);
        let m = matrix(&c.numbers[3], Model::Rotation, Additional::default(), SIZE);
        let mid = apply(&m, [160.0, 90.0]);
        assert!(
            (mid[0] - 160.0).abs() < 1e-6 && (mid[1] - 90.0).abs() < 1e-6,
            "{mid:?}"
        );
        // On the screen the frame turned 3° clockwise: its point to the
        // right of the middle (rows up) went down; read back from there.
        let right = apply(&m, [260.0, 90.0]);
        assert!(right[1] < 90.0 - 4.0, "{right:?}");
    }

    #[test]
    fn the_additional_zoom_turn_and_shift_are_about_the_middle() {
        let add = Additional {
            zoom: 0.5,
            turn: 0.0,
            shift: [0.0, 0.0],
        };
        let m = matrix(&[0.0; 6], Model::Similar, add, SIZE);
        // Twice as big: the middle stays, the corner reads a quarter in.
        assert_eq!(apply(&m, [160.0, 90.0]), [160.0, 90.0]);
        assert_eq!(apply(&m, [0.0, 0.0]), [80.0, 45.0]);
        let turned = Additional {
            turn: 90f64.to_radians(),
            ..Additional::default()
        };
        let t = matrix(&[0.0; 6], Model::Similar, turned, SIZE);
        let mid = apply(&t, [160.0, 90.0]);
        assert!((mid[0] - 160.0).abs() < 1e-9 && (mid[1] - 90.0).abs() < 1e-9);
        // Shifted right by 10 pixels: each pixel reads 10 to its left.
        let shifted = Additional {
            shift: [10.0, 0.0],
            ..Additional::default()
        };
        let s = matrix(&[0.0; 6], Model::Similar, shifted, SIZE);
        assert_eq!(apply(&s, [50.0, 20.0]), [40.0, 20.0]);
    }

    #[test]
    fn auto_zoom_hides_the_borders() {
        // Every frame shifted 10 pixels right of the reference: the view is
        // cut 10 pixels each way, a hundredth up and one more.
        let c = Corrections {
            numbers: vec![[0.0; 6], [10.0, 0.0, 0.0, 0.0, 0.0, 0.0]],
            pans: vec![0.0],
        };
        let z = auto_zoom(&c, Model::Similar, 0, SIZE);
        let want = ((20.0f32 / 320.0) * 100.0).ceil() / 100.0 + 0.01;
        assert!((z - f64::from(want)).abs() < 1e-6, "{z}");
        // Nothing to hide: still a hundredth.
        let still = Corrections {
            numbers: vec![[0.0; 6]; 3],
            pans: vec![0.0; 2],
        };
        assert!((auto_zoom(&still, Model::Similar, 0, SIZE) - 0.01).abs() < 1e-6);
        // The margins where the corrected picture's edges cross the
        // diagonals.
        let m = [1.0, 0.0, 10.0, 0.0, 1.0, 0.0];
        let [mx, my] = margins(&m, SIZE);
        assert!((mx - 10.0).abs() < 1e-9 && my.abs() < 1e-9, "{mx} {my}");
    }

    #[test]
    fn pixels_are_read_as_vegas_reads_them() {
        // A 4 × 2 picture: its columns 0, 0.25, 0.5, 0.75.
        let src: Vec<[f32; 4]> = (0..8).map(|i| [(i % 4) as f32 * 0.25; 4]).collect();
        let shift = |dx: f64| [1.0, 0.0, dx, 0.0, 1.0, 0.0];
        // Half a pixel to the right: 512ths of each.
        let v = read(&src, [4, 2], &shift(0.5), 1.0, 0.0, 0)[0];
        assert!((v - 0.375).abs() < 1e-6, "{v}");
        // A third: the weights truncated to 1024ths.
        let w = read(&src, [4, 2], &shift(1.0 / 3.0), 1.0, 0.0, 0)[0];
        let (w0, w1) = (
            (2.0f64 / 3.0 * 1024.0) as i32,
            (1.0f64 / 3.0 * 1024.0) as i32,
        );
        let want = (0.25 * w0 as f32 + 0.5 * w1 as f32) / 1024.0;
        assert!((w - want).abs() < 1e-6, "{w} {want}");
        // Transparent from 0.4 of a pixel before the first column and 0.6
        // before the end; the last column's neighbour itself.
        assert_eq!(read(&src, [4, 2], &shift(-0.5), 0.0, 0.0, 0), [0.0; 4]);
        assert_eq!(read(&src, [4, 2], &shift(0.5), 3.0, 0.0, 0), [0.0; 4]);
        let held = read(&src, [4, 2], &shift(0.3), 3.0, 1.0, 0)[0];
        let (a, b) = ((0.7f64 * 1024.0) as i32, (0.3f64 * 1024.0) as i32);
        assert!(
            (held - 0.75 * (a + b) as f32 / 1024.0).abs() < 1e-6,
            "{held}"
        );
        let last = read(&src, [4, 2], &shift(-0.2), 3.0, 0.0, 0)[0];
        assert!(last > 0.5 && last <= 0.75, "{last}");
        // Repeat and Mirror Padding past the right edge.
        let repeat = read(&src, [4, 2], &shift(1.0), 3.0, 0.0, 2)[0];
        assert_eq!(repeat, 0.0);
        let mirror = read(&src, [4, 2], &shift(1.0), 3.0, 0.0, 1)[0];
        assert_eq!(mirror, 0.75);
        let clamp = read(&src, [4, 2], &shift(5.0), 3.0, 0.0, 3)[0];
        assert_eq!(clamp, 0.75);
    }

    #[test]
    fn the_motion_blur_is_a_mirrored_box() {
        let mut row: Vec<[f32; 4]> = (0..5).map(|i| [i as f32; 4]).collect();
        blur_rows(&mut row, 5, 1);
        // 0: (1 + 0 + 1) / 3; 2: (1 + 2 + 3) / 3; 4: (3 + 4 + 3) / 3.
        assert!((row[0][0] - 2.0 / 3.0).abs() < 1e-6);
        assert!((row[2][0] - 2.0).abs() < 1e-6);
        assert!((row[4][0] - 10.0 / 3.0).abs() < 1e-6, "{:?}", row[4]);
    }

    #[test]
    fn the_blur_follows_the_smoothed_pan() {
        // A steady pan of 30 pixels a frame, Threshold 10: a box of 3 each
        // way (the frame's width allowing).
        let steps = vec![Step::similar(30.0 / SIZE[0], 0.0, 0.0, 1.0); 40];
        let s = Settings {
            motion: motion_text(&steps),
            clear: false,
            smoothing: smooth(Model::Translation),
            add: [0.0, 0.0, 0.5, 0.5],
            fill: 0,
            blur: true,
            threshold: 10.0,
            most: 19.0,
            show_points: false,
        };
        let (_, reach) = s.drawing(10.0, SIZE).unwrap();
        assert_eq!(reach, 3);
        // Below the threshold: none.
        let calm = Settings {
            threshold: 40.0,
            ..s.clone()
        };
        assert_eq!(calm.drawing(10.0, SIZE).unwrap().1, 0);
        // While the points show: the frame as filmed.
        let points = Settings {
            show_points: true,
            ..s
        };
        assert_eq!(points.drawing(10.0, SIZE), Some((IDENTITY, 0)));
    }

    #[test]
    fn the_warp_keeps_shear_and_both_scales() {
        let l = [1.02, 0.03, -0.01, 0.97];
        let [angle, sx, sy, shear] = parts_of(l);
        let back = linear(&[0.0, 0.0, angle, sx, sy, shear]);
        for (a, b) in l.iter().zip(back) {
            assert!((a - b).abs() < 1e-12, "{l:?} {back:?}");
        }
    }

    #[test]
    fn the_cards_numbers_keep_the_map() {
        let m = [0.97, 0.01, 0.03, -0.02, 0.98, 0.05];
        let n = card_numbers(&m, 320, 180);
        let (x, y) = (100.0f64, 50.0f64);
        let direct = [
            m[0] * x + m[1] * 320.0 / 180.0 * y + m[2] * 320.0,
            m[3] * 180.0 / 320.0 * x + m[4] * y + m[5] * 180.0,
        ];
        let card = [
            x + f64::from(n[0]) * x + f64::from(n[1]) * y + f64::from(n[2]) + f64::from(n[3]),
            y + f64::from(n[4]) * x + f64::from(n[5]) * y + f64::from(n[6]) + f64::from(n[7]),
        ];
        assert!((direct[0] - card[0]).abs() < 1e-4 && (direct[1] - card[1]).abs() < 1e-4);
    }
}
