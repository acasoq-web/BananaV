//! Motion tracking (VEGAS 2026's Motion Tracking window, as its Help
//! describes it): a region of a frame — four corners — followed to the
//! next frame. Corner-like features inside the region (or outside it) are
//! found by their structure tensor's smaller eigenvalue (Shi and Tomasi),
//! followed by pyramidal Lucas–Kanade and kept only when tracking them
//! back returns them home; the region's movement is the transform the mode
//! allows — a shift, a turn and a shift, a scale and a shift, an affine or
//! a perspective transform — fitted to them by RANSAC, then by least
//! squares on the features that agree with it.

pub mod flow;

/// A grey picture: luma, rows from the top.
#[derive(Clone, Debug, PartialEq)]
pub struct Gray {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f32>,
}

impl Gray {
    /// From premultiplied RGBA pixels: BT.709's luma of their colors.
    pub fn from_rgba(width: usize, height: usize, px: &[[f32; 4]]) -> Gray {
        Gray {
            width,
            height,
            data: px
                .iter()
                .map(|p| 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2])
                .collect(),
        }
    }

    /// The pixel at (`x`, `y`), the edges held past the picture.
    fn at(&self, x: isize, y: isize) -> f32 {
        let x = x.clamp(0, self.width as isize - 1) as usize;
        let y = y.clamp(0, self.height as isize - 1) as usize;
        self.data[y * self.width + x]
    }

    /// The picture at (`x`, `y`) between pixels (their centers at whole
    /// numbers).
    pub fn sample(&self, x: f32, y: f32) -> f32 {
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let (x0, y0) = (x0 as isize, y0 as isize);
        let top = self.at(x0, y0) * (1.0 - fx) + self.at(x0 + 1, y0) * fx;
        let bottom = self.at(x0, y0 + 1) * (1.0 - fx) + self.at(x0 + 1, y0 + 1) * fx;
        top * (1.0 - fy) + bottom * fy
    }

    /// Half as big: each pixel a weighted mean of the 4×4 around its four
    /// (1, 3, 3, 1 each way), so nothing aliases.
    fn half(&self) -> Gray {
        let (w, h) = (
            self.width.div_ceil(2).max(1),
            self.height.div_ceil(2).max(1),
        );
        let k = [1.0, 3.0, 3.0, 1.0];
        let mut data = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let mut sum = 0.0;
                for (j, ky) in k.iter().enumerate() {
                    for (i, kx) in k.iter().enumerate() {
                        let sx = 2 * x as isize + i as isize - 1;
                        let sy = 2 * y as isize + j as isize - 1;
                        sum += self.at(sx, sy) * kx * ky;
                    }
                }
                data.push(sum / 64.0);
            }
        }
        Gray {
            width: w,
            height: h,
            data,
        }
    }

    /// Its slopes across and down (central differences).
    fn gradients(&self) -> (Vec<f32>, Vec<f32>) {
        let (w, h) = (self.width, self.height);
        let mut gx = vec![0.0; w * h];
        let mut gy = vec![0.0; w * h];
        for y in 0..h {
            for x in 0..w {
                let (xi, yi) = (x as isize, y as isize);
                gx[y * w + x] = (self.at(xi + 1, yi) - self.at(xi - 1, yi)) / 2.0;
                gy[y * w + x] = (self.at(xi, yi + 1) - self.at(xi, yi - 1)) / 2.0;
            }
        }
        (gx, gy)
    }
}

/// A point in pixels of the frames given.
pub type Point = [f32; 2];
/// A region: its four corners, in order round it.
pub type Quad = [Point; 4];

/// How hard tracking works (VEGAS's Precision): Fast, Balanced or
/// Accurate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Precision {
    Fast,
    #[default]
    Balanced,
    Accurate,
}

impl Precision {
    pub const ALL: [Precision; 3] = [Precision::Fast, Precision::Balanced, Precision::Accurate];

    pub fn name(self) -> &'static str {
        match self {
            Precision::Fast => "Fast",
            Precision::Balanced => "Balanced",
            Precision::Accurate => "Accurate",
        }
    }

    /// How wide the frames tracking works on are.
    pub fn width(self) -> u32 {
        match self {
            Precision::Fast => 480,
            Precision::Balanced => 720,
            Precision::Accurate => 1280,
        }
    }

    /// How many features are followed.
    fn features(self) -> usize {
        match self {
            Precision::Fast => 40,
            Precision::Balanced => 90,
            Precision::Accurate => 180,
        }
    }

    /// Lucas–Kanade's steps at each level.
    fn iterations(self) -> usize {
        match self {
            Precision::Fast => 8,
            Precision::Balanced => 15,
            Precision::Accurate => 30,
        }
    }
}

/// How a feature is followed into the next frame (Video Stabilization's
/// Tracker Type): by Lucas–Kanade's slopes, by the place where its patch
/// differs least (the sum of absolute differences, coarse to fine), or
/// that place refined by Lucas–Kanade.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Follow {
    #[default]
    LucasKanade,
    AbsoluteDifferences,
    Hybrid,
}

impl Follow {
    /// Video Stabilization's Tracker Type, by its place in the list.
    pub fn of_index(i: i32) -> Follow {
        match i {
            1 => Follow::AbsoluteDifferences,
            2 => Follow::Hybrid,
            _ => Follow::LucasKanade,
        }
    }
}

/// What tracking's work is: the frames' width, how many features are
/// followed and Lucas–Kanade's steps at each level ([`Precision`] presets
/// them; Video Stabilization's Expert view sets its own), how features are
/// followed, and whether they are spread over a grid of the frame's cells
/// (columns, rows, each cell's most: only the cells whose middle is in the
/// area tracked).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Tuning {
    pub width: u32,
    pub features: usize,
    pub iterations: usize,
    pub follow: Follow,
    pub grid: Option<[usize; 3]>,
}

impl Precision {
    /// Its preset work.
    pub fn tuning(self) -> Tuning {
        Tuning {
            width: self.width(),
            features: self.features(),
            iterations: self.iterations(),
            follow: Follow::LucasKanade,
            grid: None,
        }
    }
}

/// What movement is found (VEGAS's Mode).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Mode {
    /// A shift.
    Location,
    /// A turn and a shift.
    RotationLocation,
    /// A scale and a shift.
    ScaleLocation,
    /// An affine transform: all of the above and a shear.
    ShapeLocation,
    /// A perspective transform.
    #[default]
    Perspective,
}

impl Mode {
    pub const ALL: [Mode; 5] = [
        Mode::Location,
        Mode::RotationLocation,
        Mode::ScaleLocation,
        Mode::ShapeLocation,
        Mode::Perspective,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Mode::Location => "Location",
            Mode::RotationLocation => "Rotation & Location",
            Mode::ScaleLocation => "Scale & Location",
            Mode::ShapeLocation => "Shape & Location",
            Mode::Perspective => "Perspective",
        }
    }

    /// The correspondences a transform of the mode needs.
    fn needs(self) -> usize {
        match self {
            Mode::Location => 1,
            Mode::RotationLocation | Mode::ScaleLocation => 2,
            Mode::ShapeLocation => 3,
            Mode::Perspective => 4,
        }
    }
}

/// A plane's transform (a 3 × 3 matrix by rows): `x′ = (a x + b y + c) ⁄
/// (g x + h y + i)`, `y′ = (d x + e y + f) ⁄ (g x + h y + i)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform(pub [f64; 9]);

impl Transform {
    pub const IDENTITY: Transform = Transform([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);

    pub fn apply(&self, [x, y]: Point) -> Point {
        let m = &self.0;
        let (x, y) = (x as f64, y as f64);
        let w = m[6] * x + m[7] * y + m[8];
        let w = if w.abs() < 1e-12 { 1e-12 } else { w };
        [
            ((m[0] * x + m[1] * y + m[2]) / w) as f32,
            ((m[3] * x + m[4] * y + m[5]) / w) as f32,
        ]
    }
}

/// One frame's step: where the region went, and the features that agreed
/// (from where to where: the motion vectors).
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub quad: Quad,
    pub transform: Transform,
    pub vectors: Vec<(Point, Point)>,
}

/// Why the region was lost.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lost {
    /// Too little texture in it to follow.
    TooFewFeatures,
    /// Its features did not move together.
    NoMatch,
    /// The region it would have become folds or vanishes.
    Degenerate,
}

impl Lost {
    pub fn text(self) -> &'static str {
        match self {
            Lost::TooFewFeatures => "The region has too little detail to track.",
            Lost::NoMatch => "The region's motion could not be found.",
            Lost::Degenerate => "The region was lost.",
        }
    }
}

/// Whether `p` is inside `quad` (any polygon: its crossings counted).
pub fn inside(quad: &[Point], [x, y]: Point) -> bool {
    let mut odd = false;
    let n = quad.len();
    for i in 0..n {
        let (a, b) = (quad[i], quad[(i + n - 1) % n]);
        if (a[1] > y) != (b[1] > y) && x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0] {
            odd = !odd;
        }
    }
    odd
}

/// The half width of Lucas–Kanade's window and of the features' tensor.
const HALF: isize = 7;
/// A feature tracked back must come home within this (pixels).
const ROUND_TRIP: f32 = 1.0;
/// A feature agrees with a transform within this (pixels).
const AGREE: f32 = 1.5;

/// The features of `img` to follow: the strongest corners inside `quad`
/// (outside it with `outside`), at least `apart` pixels from each other,
/// at most `n` — or, with a `grid` of the frame's cells, at most its
/// third number in each of the cells whose middle is in the area.
fn features(
    img: &Gray,
    quad: &[Point],
    outside: bool,
    n: usize,
    apart: f32,
    grid: Option<[usize; 3]>,
) -> Vec<Point> {
    let (w, h) = (img.width, img.height);
    if w < 2 * HALF as usize + 2 || h < 2 * HALF as usize + 2 {
        return Vec::new();
    }
    let (gx, gy) = img.gradients();
    // Sums of the tensor's terms over a 5 × 5 window, by integral images.
    let integral = |f: &dyn Fn(usize) -> f32| {
        let mut s = vec![0.0f64; (w + 1) * (h + 1)];
        for y in 0..h {
            let mut row = 0.0f64;
            for x in 0..w {
                row += f(y * w + x) as f64;
                s[(y + 1) * (w + 1) + x + 1] = s[y * (w + 1) + x + 1] + row;
            }
        }
        s
    };
    let sxx = integral(&|i| gx[i] * gx[i]);
    let sxy = integral(&|i| gx[i] * gy[i]);
    let syy = integral(&|i| gy[i] * gy[i]);
    let boxed = |s: &[f64], x: usize, y: usize| {
        let (x0, y0, x1, y1) = (x - 2, y - 2, x + 3, y + 3);
        s[y1 * (w + 1) + x1] - s[y0 * (w + 1) + x1] - s[y1 * (w + 1) + x0] + s[y0 * (w + 1) + x0]
    };
    let margin = HALF as usize + 1;
    let mut strength = vec![0.0f32; w * h];
    let mut best = 0.0f32;
    for y in margin..h - margin {
        for x in margin..w - margin {
            let p = [x as f32, y as f32];
            if inside(quad, p) == outside {
                continue;
            }
            let (a, b, c) = (boxed(&sxx, x, y), boxed(&sxy, x, y), boxed(&syy, x, y));
            let l = ((a + c) / 2.0 - (((a - c) / 2.0).powi(2) + b * b).sqrt()) as f32;
            strength[y * w + x] = l;
            best = best.max(l);
        }
    }
    if best <= 1e-9 {
        return Vec::new();
    }
    // Local maxima well above the noise, strongest first.
    let mut found: Vec<(f32, usize, usize)> = Vec::new();
    for y in margin..h - margin {
        for x in margin..w - margin {
            let l = strength[y * w + x];
            if l < best * 0.01 {
                continue;
            }
            let peak = (y - 1..=y + 1).all(|j| (x - 1..=x + 1).all(|i| strength[j * w + i] <= l));
            if peak {
                found.push((l, x, y));
            }
        }
    }
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut out: Vec<Point> = Vec::new();
    let far = |out: &[Point], p: Point| {
        out.iter()
            .all(|q| (q[0] - p[0]).hypot(q[1] - p[1]) >= apart)
    };
    match grid {
        Some([cols, rows, each]) if cols > 0 && rows > 0 => {
            let (cw, ch) = (w as f32 / cols as f32, h as f32 / rows as f32);
            let mut taken = vec![0usize; cols * rows];
            for (_, x, y) in found {
                let (c, r) = (
                    ((x as f32 / cw) as usize).min(cols - 1),
                    ((y as f32 / ch) as usize).min(rows - 1),
                );
                let middle = [(c as f32 + 0.5) * cw, (r as f32 + 0.5) * ch];
                let p = [x as f32, y as f32];
                if taken[r * cols + c] < each && inside(quad, middle) != outside && far(&out, p) {
                    taken[r * cols + c] += 1;
                    out.push(p);
                }
            }
        }
        _ => {
            for (_, x, y) in found {
                let p = [x as f32, y as f32];
                if far(&out, p) {
                    out.push(p);
                    if out.len() >= n {
                        break;
                    }
                }
            }
        }
    }
    out
}

/// A picture and its smaller copies, with each one's slopes.
struct Pyramid {
    levels: Vec<(Gray, Vec<f32>, Vec<f32>)>,
}

impl Pyramid {
    fn new(img: &Gray, levels: usize) -> Pyramid {
        let mut out = Vec::with_capacity(levels);
        let mut g = img.clone();
        for l in 0..levels {
            let (gx, gy) = g.gradients();
            let next = (l + 1 < levels).then(|| g.half());
            out.push((g, gx, gy));
            match next {
                Some(n) if n.width > 2 * HALF as usize && n.height > 2 * HALF as usize => g = n,
                _ => break,
            }
        }
        Pyramid { levels: out }
    }
}

/// A slope image at (`x`, `y`) between pixels.
fn sample(v: &[f32], w: usize, h: usize, x: f32, y: f32) -> f32 {
    let at = |x: isize, y: isize| {
        let x = x.clamp(0, w as isize - 1) as usize;
        let y = y.clamp(0, h as isize - 1) as usize;
        v[y * w + x]
    };
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (x0, y0) = (x0 as isize, y0 as isize);
    let top = at(x0, y0) * (1.0 - fx) + at(x0 + 1, y0) * fx;
    let bottom = at(x0, y0 + 1) * (1.0 - fx) + at(x0 + 1, y0 + 1) * fx;
    top * (1.0 - fy) + bottom * fy
}

/// Where `p` of `from` went in `to` (pyramidal Lucas–Kanade), if found.
fn follow(from: &Pyramid, to: &Pyramid, p: Point, iterations: usize) -> Option<Point> {
    let levels = from.levels.len().min(to.levels.len());
    let mut g = [0.0f32; 2];
    for l in (0..levels).rev() {
        let (prev, gx, gy) = &from.levels[l];
        let next = &to.levels[l].0;
        let k = (1 << l) as f32;
        let c = [p[0] / k, p[1] / k];
        // The window's slopes, and their matrix.
        let mut win = Vec::with_capacity(((2 * HALF + 1) * (2 * HALF + 1)) as usize);
        let (mut a, mut b, mut d) = (0.0f32, 0.0f32, 0.0f32);
        for j in -HALF..=HALF {
            for i in -HALF..=HALF {
                let (x, y) = (c[0] + i as f32, c[1] + j as f32);
                let ix = sample(gx, prev.width, prev.height, x, y);
                let iy = sample(gy, prev.width, prev.height, x, y);
                win.push((x, y, prev.sample(x, y), ix, iy));
                a += ix * ix;
                b += ix * iy;
                d += iy * iy;
            }
        }
        let det = a * d - b * b;
        if det.abs() < 1e-9 {
            return None;
        }
        let mut v = [0.0f32; 2];
        for _ in 0..iterations {
            let (mut ex, mut ey) = (0.0f32, 0.0f32);
            for &(x, y, i0, ix, iy) in &win {
                let diff = i0 - next.sample(x + g[0] + v[0], y + g[1] + v[1]);
                ex += diff * ix;
                ey += diff * iy;
            }
            let step = [(d * ex - b * ey) / det, (a * ey - b * ex) / det];
            v[0] += step[0];
            v[1] += step[1];
            if step[0].hypot(step[1]) < 0.01 {
                break;
            }
        }
        g = if l == 0 {
            [g[0] + v[0], g[1] + v[1]]
        } else {
            [2.0 * (g[0] + v[0]), 2.0 * (g[1] + v[1])]
        };
    }
    within(to, [p[0] + g[0], p[1] + g[1]])
}

/// `q` when it is a place inside the pyramid's picture.
fn within(to: &Pyramid, q: Point) -> Option<Point> {
    let (w, h) = (to.levels[0].0.width as f32, to.levels[0].0.height as f32);
    (q[0].is_finite() && q[1].is_finite() && q[0] >= 0.0 && q[1] >= 0.0 && q[0] < w && q[1] < h)
        .then_some(q)
}

/// How far the shifts searched reach at the coarsest level and at the
/// others (pixels of each level).
const SEARCH_TOP: i32 = 4;
const SEARCH: i32 = 2;

/// Where `p` of `from` went in `to` by the least sum of absolute
/// differences of its window, coarse to fine: at each level the whole
/// shifts around the coarser level's, then at the finest a parabola
/// through the sums either side places it between pixels.
fn follow_sad(from: &Pyramid, to: &Pyramid, p: Point) -> Option<Point> {
    let levels = from.levels.len().min(to.levels.len());
    let mut g = [0.0f32; 2];
    for l in (0..levels).rev() {
        let prev = &from.levels[l].0;
        let next = &to.levels[l].0;
        let k = (1 << l) as f32;
        let c = [p[0] / k, p[1] / k];
        let window: Vec<(f32, f32, f32)> = (-HALF..=HALF)
            .flat_map(|j| (-HALF..=HALF).map(move |i| (i as f32, j as f32)))
            .map(|(i, j)| (i, j, prev.sample(c[0] + i, c[1] + j)))
            .collect();
        let sad = |d: [f32; 2]| -> f32 {
            window
                .iter()
                .map(|&(i, j, v)| {
                    (v - next.sample(c[0] + g[0] + d[0] + i, c[1] + g[1] + d[1] + j)).abs()
                })
                .sum()
        };
        let reach = if l + 1 == levels { SEARCH_TOP } else { SEARCH };
        let mut best = ([0i32; 2], f32::INFINITY);
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let s = sad([dx as f32, dy as f32]);
                if s < best.1 {
                    best = ([dx, dy], s);
                }
            }
        }
        let [bx, by] = best.0.map(|v| v as f32);
        let mut d = [bx, by];
        if l == 0 {
            // Between pixels: the parabola through the sums either side.
            let between = |lo: f32, mid: f32, hi: f32| {
                let curve = lo - 2.0 * mid + hi;
                if curve > 1e-9 {
                    ((lo - hi) / (2.0 * curve)).clamp(-0.5, 0.5)
                } else {
                    0.0
                }
            };
            d[0] += between(sad([bx - 1.0, by]), best.1, sad([bx + 1.0, by]));
            d[1] += between(sad([bx, by - 1.0]), best.1, sad([bx, by + 1.0]));
        }
        g = [g[0] + d[0], g[1] + d[1]];
        if l > 0 {
            g = [2.0 * g[0], 2.0 * g[1]];
        }
    }
    within(to, [p[0] + g[0], p[1] + g[1]])
}

/// [`follow_sad`]'s place refined by Lucas–Kanade on the finest level.
fn follow_hybrid(from: &Pyramid, to: &Pyramid, p: Point, iterations: usize) -> Option<Point> {
    let q = follow_sad(from, to, p)?;
    let (prev, gx, gy) = &from.levels[0];
    let next = &to.levels[0].0;
    let (mut a, mut b, mut d) = (0.0f32, 0.0f32, 0.0f32);
    let mut win = Vec::with_capacity(((2 * HALF + 1) * (2 * HALF + 1)) as usize);
    for j in -HALF..=HALF {
        for i in -HALF..=HALF {
            let (x, y) = (p[0] + i as f32, p[1] + j as f32);
            let ix = sample(gx, prev.width, prev.height, x, y);
            let iy = sample(gy, prev.width, prev.height, x, y);
            win.push((x, y, prev.sample(x, y), ix, iy));
            a += ix * ix;
            b += ix * iy;
            d += iy * iy;
        }
    }
    let det = a * d - b * b;
    if det.abs() < 1e-9 {
        return Some(q);
    }
    let mut v = [q[0] - p[0], q[1] - p[1]];
    for _ in 0..iterations {
        let (mut ex, mut ey) = (0.0f32, 0.0f32);
        for &(x, y, i0, ix, iy) in &win {
            let diff = i0 - next.sample(x + v[0], y + v[1]);
            ex += diff * ix;
            ey += diff * iy;
        }
        let step = [(d * ex - b * ey) / det, (a * ey - b * ex) / det];
        v[0] += step[0];
        v[1] += step[1];
        if step[0].hypot(step[1]) < 0.01 {
            break;
        }
    }
    within(to, [p[0] + v[0], p[1] + v[1]])
}

/// A small random sequence (the same every time: tracking repeats).
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n.max(1) as u64) as usize
    }
}

/// Solves `a x = b` (`n` unknowns, `a` by rows) by Gaussian elimination
/// with partial pivoting.
fn solve(mut a: Vec<f64>, mut b: Vec<f64>, n: usize) -> Option<Vec<f64>> {
    for col in 0..n {
        let pivot =
            (col..n).max_by(|&i, &j| a[i * n + col].abs().total_cmp(&a[j * n + col].abs()))?;
        if a[pivot * n + col].abs() < 1e-12 {
            return None;
        }
        if pivot != col {
            for k in 0..n {
                a.swap(pivot * n + k, col * n + k);
            }
            b.swap(pivot, col);
        }
        for row in col + 1..n {
            let f = a[row * n + col] / a[col * n + col];
            for k in col..n {
                a[row * n + k] -= f * a[col * n + k];
            }
            b[row] -= f * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let mut s = b[row];
        for k in row + 1..n {
            s -= a[row * n + k] * x[k];
        }
        x[row] = s / a[row * n + row];
    }
    Some(x)
}

/// The transform of `mode` that takes `from` to `to` best (least
/// squares), if there is one.
fn fit(mode: Mode, pairs: &[(Point, Point)]) -> Option<Transform> {
    let n = pairs.len();
    if n < mode.needs() {
        return None;
    }
    let f = |p: Point| [p[0] as f64, p[1] as f64];
    let mean = |pick: &dyn Fn(&(Point, Point)) -> Point| {
        let s = pairs.iter().fold([0.0, 0.0], |s, q| {
            [s[0] + f(pick(q))[0], s[1] + f(pick(q))[1]]
        });
        [s[0] / n as f64, s[1] / n as f64]
    };
    let (cp, cq) = (mean(&|q| q.0), mean(&|q| q.1));
    match mode {
        Mode::Location => Some(Transform([
            1.0,
            0.0,
            cq[0] - cp[0],
            0.0,
            1.0,
            cq[1] - cp[1],
            0.0,
            0.0,
            1.0,
        ])),
        Mode::RotationLocation | Mode::ScaleLocation => {
            // About the centroids: the turn (or the scale) that fits.
            let (mut dot, mut cross, mut pp) = (0.0, 0.0, 0.0);
            for (p, q) in pairs {
                let (p, q) = (f(*p), f(*q));
                let (px, py) = (p[0] - cp[0], p[1] - cp[1]);
                let (qx, qy) = (q[0] - cq[0], q[1] - cq[1]);
                dot += px * qx + py * qy;
                cross += px * qy - py * qx;
                pp += px * px + py * py;
            }
            let (c, s) = if mode == Mode::RotationLocation {
                let a = cross.atan2(dot);
                (a.cos(), a.sin())
            } else {
                if pp < 1e-12 {
                    return None;
                }
                (dot / pp, 0.0)
            };
            Some(Transform([
                c,
                -s,
                cq[0] - (c * cp[0] - s * cp[1]),
                s,
                c,
                cq[1] - (s * cp[0] + c * cp[1]),
                0.0,
                0.0,
                1.0,
            ]))
        }
        Mode::ShapeLocation => {
            // x′ = a x + b y + c and y′ = d x + e y + f, apart.
            let mut m = vec![0.0; 9];
            let mut bx = vec![0.0; 3];
            let mut by = vec![0.0; 3];
            for (p, q) in pairs {
                let (p, q) = (f(*p), f(*q));
                let r = [p[0] - cp[0], p[1] - cp[1], 1.0];
                for i in 0..3 {
                    for j in 0..3 {
                        m[i * 3 + j] += r[i] * r[j];
                    }
                    bx[i] += r[i] * (q[0] - cq[0]);
                    by[i] += r[i] * (q[1] - cq[1]);
                }
            }
            let x = solve(m.clone(), bx, 3)?;
            let y = solve(m, by, 3)?;
            // Back from the centroids.
            Some(Transform([
                x[0],
                x[1],
                cq[0] + x[2] - x[0] * cp[0] - x[1] * cp[1],
                y[0],
                y[1],
                cq[1] + y[2] - y[0] * cp[0] - y[1] * cp[1],
                0.0,
                0.0,
                1.0,
            ]))
        }
        Mode::Perspective => {
            // Normalized (centroids, mean distance √2), then the eight
            // unknowns by least squares, the last one 1.
            let spread = |c: [f64; 2], pick: &dyn Fn(&(Point, Point)) -> Point| {
                let d: f64 = pairs
                    .iter()
                    .map(|q| {
                        let p = f(pick(q));
                        (p[0] - c[0]).hypot(p[1] - c[1])
                    })
                    .sum::<f64>()
                    / n as f64;
                if d < 1e-12 { 1.0 } else { 2f64.sqrt() / d }
            };
            let (sp, sq) = (spread(cp, &|q| q.0), spread(cq, &|q| q.1));
            let mut ata = vec![0.0; 64];
            let mut atb = vec![0.0; 8];
            for (p, q) in pairs {
                let (p, q) = (f(*p), f(*q));
                let (x, y) = ((p[0] - cp[0]) * sp, (p[1] - cp[1]) * sp);
                let (u, v) = ((q[0] - cq[0]) * sq, (q[1] - cq[1]) * sq);
                for (row, rhs) in [
                    ([x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y], u),
                    ([0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y], v),
                ] {
                    for i in 0..8 {
                        for j in 0..8 {
                            ata[i * 8 + j] += row[i] * row[j];
                        }
                        atb[i] += row[i] * rhs;
                    }
                }
            }
            let h = solve(ata, atb, 8)?;
            let hn = [h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7], 1.0];
            // Undo the normalizations: T_q⁻¹ · H · T_p.
            let tp = [sp, 0.0, -sp * cp[0], 0.0, sp, -sp * cp[1], 0.0, 0.0, 1.0];
            let tq_inv = [1.0 / sq, 0.0, cq[0], 0.0, 1.0 / sq, cq[1], 0.0, 0.0, 1.0];
            let mul = |a: &[f64; 9], b: &[f64; 9]| {
                let mut o = [0.0; 9];
                for i in 0..3 {
                    for j in 0..3 {
                        o[i * 3 + j] = (0..3).map(|k| a[i * 3 + k] * b[k * 3 + j]).sum();
                    }
                }
                o
            };
            let m = mul(&tq_inv, &mul(&hn, &tp));
            let s = if m[8].abs() > 1e-12 { m[8] } else { 1.0 };
            Some(Transform(m.map(|v| v / s)))
        }
    }
}

/// The transform of `mode` most features agree with (RANSAC), refitted
/// on those features; and which they are.
fn robust_fit(
    mode: Mode,
    pairs: &[(Point, Point)],
    tries: usize,
) -> Option<(Transform, Vec<bool>)> {
    let need = mode.needs();
    if pairs.len() < need {
        return None;
    }
    let agrees = |t: &Transform| -> Vec<bool> {
        pairs
            .iter()
            .map(|(p, q)| {
                let r = t.apply(*p);
                (r[0] - q[0]).hypot(r[1] - q[1]) <= AGREE
            })
            .collect()
    };
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut best: Option<(usize, Vec<bool>)> = None;
    for _ in 0..tries {
        let mut pick: Vec<usize> = Vec::with_capacity(need);
        while pick.len() < need {
            let i = rng.below(pairs.len());
            if !pick.contains(&i) {
                pick.push(i);
            }
        }
        let sample: Vec<(Point, Point)> = pick.iter().map(|&i| pairs[i]).collect();
        let Some(t) = fit(mode, &sample) else {
            continue;
        };
        let ok = agrees(&t);
        let count = ok.iter().filter(|&&o| o).count();
        if best.as_ref().is_none_or(|(c, _)| count > *c) {
            best = Some((count, ok));
        }
        if count == pairs.len() {
            break;
        }
    }
    let (_, ok) = best?;
    let kept: Vec<(Point, Point)> = pairs
        .iter()
        .zip(&ok)
        .filter(|(_, o)| **o)
        .map(|(p, _)| *p)
        .collect();
    let t = fit(mode, &kept)?;
    let ok = agrees(&t);
    Some((t, ok))
}

/// The quad's signed area (positive or negative by its way round).
fn area(q: &Quad) -> f32 {
    (0..4)
        .map(|i| {
            let (a, b) = (q[i], q[(i + 1) % 4]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f32>()
        / 2.0
}

/// Whether a quad goes round without crossing itself (its corners' turns
/// all one way).
fn convex(q: &Quad) -> bool {
    let turn = |i: usize| {
        let (a, b, c) = (q[i], q[(i + 1) % 4], q[(i + 2) % 4]);
        (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0])
    };
    let t: Vec<f32> = (0..4).map(turn).collect();
    t.iter().all(|&v| v > 0.0) || t.iter().all(|&v| v < 0.0)
}

/// The features of `prev` inside the polygon `area` (outside it with
/// `outside`) and where they went in `next` (both the same size), kept
/// only when followed back they come home; too few features to follow:
/// lost.
pub fn follow_features(
    prev: &Gray,
    next: &Gray,
    area: &[Point],
    outside: bool,
    precision: Precision,
) -> Result<Vec<(Point, Point)>, Lost> {
    follow_features_tuned(prev, next, area, outside, precision.tuning())
}

/// [`follow_features`] with `tuning`'s work.
pub fn follow_features_tuned(
    prev: &Gray,
    next: &Gray,
    area: &[Point],
    outside: bool,
    tuning: Tuning,
) -> Result<Vec<(Point, Point)>, Lost> {
    let size = prev.width.min(prev.height) as f32;
    let apart = (size / 60.0).max(4.0);
    let points = features(
        prev,
        area,
        outside,
        tuning.features.max(5),
        apart,
        tuning.grid,
    );
    if points.len() < 5 {
        return Err(Lost::TooFewFeatures);
    }
    let levels = 4;
    let (a, b) = (Pyramid::new(prev, levels), Pyramid::new(next, levels));
    let iterations = tuning.iterations.max(1);
    let go = |from: &Pyramid, to: &Pyramid, p: Point| match tuning.follow {
        Follow::LucasKanade => follow(from, to, p, iterations),
        Follow::AbsoluteDifferences => follow_sad(from, to, p),
        Follow::Hybrid => follow_hybrid(from, to, p, iterations),
    };
    Ok(points
        .iter()
        .filter_map(|&p| {
            let q = go(&a, &b, p)?;
            let back = go(&b, &a, q)?;
            ((back[0] - p[0]).hypot(back[1] - p[1]) <= ROUND_TRIP).then_some((p, q))
        })
        .collect())
}

/// Follows the region `quad` of `prev` into `next` (both the same size):
/// the features inside it (outside with `outside`) followed and fitted as
/// `mode` allows, at `precision`.
pub fn track(
    prev: &Gray,
    next: &Gray,
    quad: Quad,
    outside: bool,
    mode: Mode,
    precision: Precision,
) -> Result<Step, Lost> {
    track_tuned(prev, next, quad, outside, mode, precision.tuning())
}

/// [`track`] with `tuning`'s work.
pub fn track_tuned(
    prev: &Gray,
    next: &Gray,
    quad: Quad,
    outside: bool,
    mode: Mode,
    tuning: Tuning,
) -> Result<Step, Lost> {
    let least = mode.needs().max(3) + 2;
    let pairs = follow_features_tuned(prev, next, &quad, outside, tuning)?;
    if pairs.len() < least {
        return Err(Lost::NoMatch);
    }
    let (t, ok) = robust_fit(mode, &pairs, 300).ok_or(Lost::NoMatch)?;
    let agreeing = ok.iter().filter(|&&o| o).count();
    if agreeing < least || (agreeing as f32) < pairs.len() as f32 * 0.3 {
        return Err(Lost::NoMatch);
    }
    let moved: Quad = quad.map(|p| t.apply(p));
    let before = area(&quad).abs();
    let after = area(&moved).abs();
    if !convex(&moved) || after < before * 0.05 || after > before * 20.0 {
        return Err(Lost::Degenerate);
    }
    Ok(Step {
        quad: moved,
        transform: t,
        vectors: pairs
            .into_iter()
            .zip(ok)
            .filter(|(_, o)| *o)
            .map(|(p, _)| p)
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A textured picture: smooth blobs and stripes, the same function at
    /// any place, drawn through `map` (from the new picture's pixels to
    /// the original's).
    fn texture(w: usize, h: usize, map: impl Fn(f32, f32) -> (f32, f32)) -> Gray {
        let mut data = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let (u, v) = map(x as f32, y as f32);
                let f = 0.5
                    + 0.18 * (u * 0.21).sin() * (v * 0.17).cos()
                    + 0.12 * (u * 0.07 + v * 0.11).sin()
                    + 0.1 * ((u - v) * 0.31).sin()
                    + 0.08 * (u * 0.53).cos() * (v * 0.41).sin();
                data.push(f);
            }
        }
        Gray {
            width: w,
            height: h,
            data,
        }
    }

    fn square() -> Quad {
        [[60.0, 40.0], [140.0, 40.0], [140.0, 120.0], [60.0, 120.0]]
    }

    fn near(a: Quad, b: Quad, tol: f32) -> bool {
        a.iter()
            .zip(&b)
            .all(|(p, q)| (p[0] - q[0]).abs() <= tol && (p[1] - q[1]).abs() <= tol)
    }

    #[test]
    fn a_shift_is_followed() {
        let a = texture(200, 160, |x, y| (x, y));
        let b = texture(200, 160, |x, y| (x - 3.5, y + 2.25));
        let s = track(&a, &b, square(), false, Mode::Location, Precision::Balanced).unwrap();
        let want = square().map(|p| [p[0] + 3.5, p[1] - 2.25]);
        assert!(near(s.quad, want, 0.15), "{:?}", s.quad);
        assert!(!s.vectors.is_empty());
    }

    #[test]
    fn a_shift_is_followed_by_each_way_and_on_a_grid() {
        let a = texture(200, 160, |x, y| (x, y));
        let b = texture(200, 160, |x, y| (x - 3.5, y + 2.25));
        let want = square().map(|p| [p[0] + 3.5, p[1] - 2.25]);
        for (follow, tol) in [
            (Follow::LucasKanade, 0.15),
            (Follow::AbsoluteDifferences, 0.35),
            (Follow::Hybrid, 0.15),
        ] {
            let tuning = Tuning {
                follow,
                grid: Some([4, 4, 3]),
                ..Precision::Balanced.tuning()
            };
            let s = track_tuned(&a, &b, square(), false, Mode::Location, tuning).unwrap();
            assert!(near(s.quad, want, tol), "{follow:?}: {:?}", s.quad);
        }
        // The grid keeps each cell's few, only where the area is.
        let points = features(&a, &square(), false, 1000, 4.0, Some([4, 4, 3]));
        assert!(
            !points.is_empty() && points.len() <= 4 * 3,
            "{}",
            points.len()
        );
        assert!(points.iter().all(|p| inside(&square(), *p)));
    }

    #[test]
    fn a_turn_and_a_scale_are_followed() {
        let a = texture(200, 160, |x, y| (x, y));
        let (cx, cy, ang): (f32, f32, f32) = (100.0, 80.0, 4f32.to_radians());
        // The new picture is the old turned by `ang` about the middle.
        let b = texture(200, 160, |x, y| {
            let (dx, dy) = (x - cx, y - cy);
            (
                cx + dx * ang.cos() + dy * ang.sin(),
                cy - dx * ang.sin() + dy * ang.cos(),
            )
        });
        let s = track(
            &a,
            &b,
            square(),
            false,
            Mode::RotationLocation,
            Precision::Accurate,
        )
        .unwrap();
        let turned = square().map(|[x, y]| {
            let (dx, dy) = (x - cx, y - cy);
            [
                cx + dx * ang.cos() - dy * ang.sin(),
                cy + dx * ang.sin() + dy * ang.cos(),
            ]
        });
        assert!(near(s.quad, turned, 0.3), "{:?} {turned:?}", s.quad);
        // Grown by 4 %: Scale & Location.
        let k = 1.04;
        let c = texture(200, 160, |x, y| (cx + (x - cx) / k, cy + (y - cy) / k));
        let s = track(
            &a,
            &c,
            square(),
            false,
            Mode::ScaleLocation,
            Precision::Accurate,
        )
        .unwrap();
        let grown = square().map(|[x, y]| [cx + (x - cx) * k, cy + (y - cy) * k]);
        assert!(near(s.quad, grown, 0.3), "{:?} {grown:?}", s.quad);
    }

    #[test]
    fn a_perspective_is_fitted_exactly() {
        let t = Transform([1.02, 0.03, 4.0, -0.02, 0.98, -3.0, 0.0002, -0.0001, 1.0]);
        let pts: Vec<Point> = (0..30)
            .map(|i| [(i * 37 % 200) as f32, (i * 53 % 160) as f32])
            .collect();
        let pairs: Vec<(Point, Point)> = pts.iter().map(|&p| (p, t.apply(p))).collect();
        let f = fit(Mode::Perspective, &pairs).unwrap();
        for (p, q) in &pairs {
            let r = f.apply(*p);
            assert!((r[0] - q[0]).abs() < 1e-3 && (r[1] - q[1]).abs() < 1e-3);
        }
        // And an affine one, with a few points gone astray.
        let a = Transform([1.1, 0.2, 5.0, -0.1, 0.9, 2.0, 0.0, 0.0, 1.0]);
        let mut pairs: Vec<(Point, Point)> = pts.iter().map(|&p| (p, a.apply(p))).collect();
        pairs[3].1[0] += 30.0;
        pairs[11].1[1] -= 25.0;
        let (f, ok) = robust_fit(Mode::ShapeLocation, &pairs, 200).unwrap();
        assert!(!ok[3] && !ok[11]);
        let r = f.apply([50.0, 50.0]);
        let q = a.apply([50.0, 50.0]);
        assert!((r[0] - q[0]).abs() < 1e-2 && (r[1] - q[1]).abs() < 1e-2);
    }

    #[test]
    fn a_flat_region_is_lost() {
        let flat = Gray {
            width: 200,
            height: 160,
            data: vec![0.5; 200 * 160],
        };
        assert_eq!(
            track(
                &flat,
                &flat,
                square(),
                false,
                Mode::Location,
                Precision::Fast
            ),
            Err(Lost::TooFewFeatures)
        );
        assert!(inside(&square(), [100.0, 80.0]));
        assert!(!inside(&square(), [10.0, 10.0]));
    }
}
