//! Dense optical flow: where every pixel of one picture went in the next —
//! Lucas–Kanade at every pixel over a box window (its sums by integral
//! images), coarse to fine down a pyramid, smoothed at each level — and
//! pictures drawn in between along it (Slow Motion's frames, Warp Flow's
//! morph, Motion Blur's streaks).

use crate::Gray;

/// Where each pixel of a picture went in another: its move across and
/// down, in pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Flow {
    pub width: usize,
    pub height: usize,
    pub dx: Vec<f32>,
    pub dy: Vec<f32>,
}

impl Flow {
    fn zero(width: usize, height: usize) -> Flow {
        Flow {
            width,
            height,
            dx: vec![0.0; width * height],
            dy: vec![0.0; width * height],
        }
    }

    /// The move at (`x`, `y`) between pixels, the edges held.
    pub fn at(&self, x: f32, y: f32) -> [f32; 2] {
        if self.width == 0 || self.height == 0 {
            return [0.0; 2];
        }
        let at = |v: &[f32], x: isize, y: isize| {
            let x = x.clamp(0, self.width as isize - 1) as usize;
            let y = y.clamp(0, self.height as isize - 1) as usize;
            v[y * self.width + x]
        };
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let (x0, y0) = (x0 as isize, y0 as isize);
        let lerp = |v: &[f32]| {
            let top = at(v, x0, y0) * (1.0 - fx) + at(v, x0 + 1, y0) * fx;
            let bottom = at(v, x0, y0 + 1) * (1.0 - fx) + at(v, x0 + 1, y0 + 1) * fx;
            top * (1.0 - fy) + bottom * fy
        };
        [lerp(&self.dx), lerp(&self.dy)]
    }

    /// The flow for a picture `k` times as big (each move `k` times as
    /// long).
    fn scaled_to(&self, width: usize, height: usize) -> Flow {
        let kx = width as f32 / self.width.max(1) as f32;
        let ky = height as f32 / self.height.max(1) as f32;
        let mut out = Flow::zero(width, height);
        for y in 0..height {
            for x in 0..width {
                let m = self.at((x as f32 + 0.5) / kx - 0.5, (y as f32 + 0.5) / ky - 0.5);
                out.dx[y * width + x] = m[0] * kx;
                out.dy[y * width + x] = m[1] * ky;
            }
        }
        out
    }

    /// The biggest move, in pixels.
    pub fn largest(&self) -> f32 {
        self.dx
            .iter()
            .zip(&self.dy)
            .map(|(x, y)| x.hypot(*y))
            .fold(0.0, f32::max)
    }
}

/// How hard the flow is worked out (VEGAS's Quality): the window's half
/// width and the steps at each level.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlowQuality {
    Low,
    #[default]
    Medium,
    High,
}

impl FlowQuality {
    fn window(self) -> usize {
        match self {
            FlowQuality::Low => 4,
            FlowQuality::Medium => 5,
            FlowQuality::High => 7,
        }
    }

    fn steps(self) -> usize {
        match self {
            FlowQuality::Low => 2,
            FlowQuality::Medium => 3,
            FlowQuality::High => 5,
        }
    }

    /// The widest picture the flow is worked out on (the frames are made
    /// that small first).
    pub fn width(self) -> usize {
        match self {
            FlowQuality::Low => 320,
            FlowQuality::Medium => 640,
            FlowQuality::High => 1280,
        }
    }
}

/// Sums of `v` over the `(2r+1)²` box around each pixel (the edges held).
fn box_sums(v: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    // Rows, then columns, each a running sum.
    let mut rows = vec![0.0f32; w * h];
    for y in 0..h {
        let line = &v[y * w..(y + 1) * w];
        let at = |x: isize| line[x.clamp(0, w as isize - 1) as usize];
        let mut sum: f32 = (-(r as isize)..=r as isize).map(at).sum();
        for x in 0..w {
            rows[y * w + x] = sum;
            sum += at(x as isize + r as isize + 1) - at(x as isize - r as isize);
        }
    }
    let mut out = vec![0.0f32; w * h];
    for x in 0..w {
        let at = |y: isize| rows[y.clamp(0, h as isize - 1) as usize * w + x];
        let mut sum: f32 = (-(r as isize)..=r as isize).map(at).sum();
        for y in 0..h {
            out[y * w + x] = sum;
            sum += at(y as isize + r as isize + 1) - at(y as isize - r as isize);
        }
    }
    out
}

/// One level's refinement of `flow` from `a` to `b` (the same size).
fn refine(a: &Gray, b: &Gray, flow: &mut Flow, q: FlowQuality) {
    let (w, h) = (a.width, a.height);
    let r = q.window();
    let (gx, gy) = a.gradients();
    let xx: Vec<f32> = gx.iter().map(|g| g * g).collect();
    let xy: Vec<f32> = gx.iter().zip(&gy).map(|(a, b)| a * b).collect();
    let yy: Vec<f32> = gy.iter().map(|g| g * g).collect();
    let (sxx, sxy, syy) = (
        box_sums(&xx, w, h, r),
        box_sums(&xy, w, h, r),
        box_sums(&yy, w, h, r),
    );
    for _ in 0..q.steps() {
        // How far each pixel's color is from where the flow takes it.
        let mut ex = vec![0.0f32; w * h];
        let mut ey = vec![0.0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let t = b.sample(x as f32 + flow.dx[i], y as f32 + flow.dy[i]) - a.data[i];
                ex[i] = gx[i] * t;
                ey[i] = gy[i] * t;
            }
        }
        let (bx, by) = (box_sums(&ex, w, h, r), box_sums(&ey, w, h, r));
        for i in 0..w * h {
            let (p, s, u) = (sxx[i], sxy[i], syy[i]);
            let det = p * u - s * s;
            if det.abs() < 1e-6 {
                continue;
            }
            flow.dx[i] -= (u * bx[i] - s * by[i]) / det;
            flow.dy[i] -= (p * by[i] - s * bx[i]) / det;
        }
        // Smoothed: a 3 × 3 box, so the field holds together.
        let n = 9.0;
        flow.dx = box_sums(&flow.dx, w, h, 1).iter().map(|v| v / n).collect();
        flow.dy = box_sums(&flow.dy, w, h, 1).iter().map(|v| v / n).collect();
    }
}

/// Where each pixel of `a` went in `b` (the same size).
pub fn dense_flow(a: &Gray, b: &Gray, quality: FlowQuality) -> Flow {
    let (w, h) = (a.width, a.height);
    if w < 2 || h < 2 || b.width != w || b.height != h {
        return Flow::zero(w, h);
    }
    // Both pyramids, down to some thirty pixels.
    let mut levels = vec![(a.clone(), b.clone())];
    while let Some((la, lb)) = levels.last() {
        if la.width < 64 || la.height < 64 || levels.len() >= 6 {
            break;
        }
        let next = (la.half(), lb.half());
        levels.push(next);
    }
    let (top_a, _) = levels.last().expect("a level");
    let mut flow = Flow::zero(top_a.width, top_a.height);
    for (la, lb) in levels.iter().rev() {
        if flow.width != la.width || flow.height != la.height {
            flow = flow.scaled_to(la.width, la.height);
        }
        refine(la, lb, &mut flow, quality);
    }
    flow
}

/// The flow from picture `a` to picture `b` (`width` × `height`
/// premultiplied RGBA pixels each), worked out on their luma brought down
/// by halves to `quality`'s width at most; with how many of the pictures'
/// pixels a pixel of the flow's is, across and down.
pub fn pictures_flow(
    width: usize,
    height: usize,
    a: &[[f32; 4]],
    b: &[[f32; 4]],
    quality: FlowQuality,
) -> (Flow, [f32; 2]) {
    let mut ga = Gray::from_rgba(width, height, a);
    let mut gb = Gray::from_rgba(width, height, b);
    while ga.width > quality.width() && ga.width >= 4 && ga.height >= 4 {
        ga = ga.half();
        gb = gb.half();
    }
    let k = [
        width as f32 / ga.width.max(1) as f32,
        height as f32 / ga.height.max(1) as f32,
    ];
    (dense_flow(&ga, &gb, quality), k)
}

/// A premultiplied RGBA picture at (`x`, `y`) between pixels (their
/// centers at whole numbers), the edges held past it.
fn rgba_at(px: &[[f32; 4]], width: usize, height: usize, x: f32, y: f32) -> [f32; 4] {
    let at = |x: isize, y: isize| {
        let x = x.clamp(0, width as isize - 1) as usize;
        let y = y.clamp(0, height as isize - 1) as usize;
        px[y * width + x]
    };
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (x0, y0) = (x0 as isize, y0 as isize);
    let (a, b, c, d) = (
        at(x0, y0),
        at(x0 + 1, y0),
        at(x0, y0 + 1),
        at(x0 + 1, y0 + 1),
    );
    std::array::from_fn(|i| {
        let top = a[i] * (1.0 - fx) + b[i] * fx;
        let bottom = c[i] * (1.0 - fx) + d[i] * fx;
        top * (1.0 - fy) + bottom * fy
    })
}

/// The picture `t` (0–1) of the way from `a` to `b` along their flow
/// ([`pictures_flow`]'s): each pixel's color from `a` back along its move
/// and from `b` forward, mixed by `t` — the frames between that following
/// what moves makes (VEGAS's Optical Flow resampling).
pub fn between(
    width: usize,
    height: usize,
    a: &[[f32; 4]],
    b: &[[f32; 4]],
    (flow, [kx, ky]): &(Flow, [f32; 2]),
    t: f32,
) -> Vec<[f32; 4]> {
    let mut out = Vec::with_capacity(width * height);
    if width == 0 || height == 0 || a.len() < width * height || b.len() < width * height {
        return a.to_vec();
    }
    for y in 0..height {
        for x in 0..width {
            let (px, py) = (x as f32, y as f32);
            let m = flow.at((px + 0.5) / kx - 0.5, (py + 0.5) / ky - 0.5);
            let (mx, my) = (m[0] * kx, m[1] * ky);
            let ca = rgba_at(a, width, height, px - t * mx, py - t * my);
            let cb = rgba_at(b, width, height, px + (1.0 - t) * mx, py + (1.0 - t) * my);
            out.push(std::array::from_fn(|i| ca[i] * (1.0 - t) + cb[i] * t));
        }
    }
    out
}

/// The flow of a smaller picture made for one `width` × `height` big.
pub fn flow_for(flow: &Flow, width: usize, height: usize) -> Flow {
    if flow.width == width && flow.height == height {
        return flow.clone();
    }
    flow.scaled_to(width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texture(w: usize, h: usize, shift: (f32, f32)) -> Gray {
        let mut data = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let (u, v) = (x as f32 - shift.0, y as f32 - shift.1);
                data.push(
                    0.5 + 0.2 * (u * 0.19).sin() * (v * 0.23).cos()
                        + 0.15 * ((u + v) * 0.11).sin()
                        + 0.1 * (u * 0.41).cos() * (v * 0.37).sin(),
                );
            }
        }
        Gray {
            width: w,
            height: h,
            data,
        }
    }

    /// A texture as gray RGBA pixels.
    fn picture(w: usize, h: usize, shift: (f32, f32)) -> Vec<[f32; 4]> {
        texture(w, h, shift)
            .data
            .iter()
            .map(|&v| [v, v, v, 1.0])
            .collect()
    }

    #[test]
    fn the_frame_between_follows_what_moves() {
        // A texture moved 6 pixels across: halfway, the frame made along
        // the flow is the texture moved 3, far nearer it than a mix.
        let (w, h) = (160, 120);
        let (a, b) = (picture(w, h, (0.0, 0.0)), picture(w, h, (6.0, 0.0)));
        let truth = picture(w, h, (3.0, 0.0));
        let flow = pictures_flow(w, h, &a, &b, FlowQuality::Medium);
        let made = between(w, h, &a, &b, &flow, 0.5);
        let error = |p: &[[f32; 4]]| {
            let mut sum = 0.0;
            for y in 20..h - 20 {
                for x in 20..w - 20 {
                    sum += (p[y * w + x][0] - truth[y * w + x][0]).abs();
                }
            }
            sum
        };
        let mixed: Vec<[f32; 4]> = a
            .iter()
            .zip(&b)
            .map(|(p, q)| std::array::from_fn(|i| (p[i] + q[i]) / 2.0))
            .collect();
        assert!(
            error(&made) < error(&mixed) * 0.3,
            "{} {}",
            error(&made),
            error(&mixed)
        );
        // The ends are the pictures themselves.
        let start = between(w, h, &a, &b, &flow, 0.0);
        assert!(
            start
                .iter()
                .zip(&a)
                .all(|(p, q)| (p[0] - q[0]).abs() < 1e-5)
        );
    }

    #[test]
    fn a_shift_shows_in_the_flow() {
        let a = texture(160, 120, (0.0, 0.0));
        let b = texture(160, 120, (3.0, -2.0));
        let f = dense_flow(&a, &b, FlowQuality::Medium);
        // Away from the edges every pixel moved about (3, −2).
        let mut worst = 0.0f32;
        for y in 20..100 {
            for x in 20..140 {
                let m = f.at(x as f32, y as f32);
                worst = worst.max((m[0] - 3.0).abs()).max((m[1] + 2.0).abs());
            }
        }
        assert!(worst < 0.35, "{worst}");
        assert!(f.largest() >= 3.0);
        // Twice as big: twice the move.
        let g = flow_for(&f, 320, 240);
        let m = g.at(160.0, 120.0);
        assert!((m[0] - 6.0).abs() < 0.8, "{m:?}");
    }

    #[test]
    fn box_sums_add_up() {
        let v = vec![1.0f32; 5 * 4];
        let s = box_sums(&v, 5, 4, 1);
        assert!(s.iter().all(|&x| (x - 9.0).abs() < 1e-5));
    }
}
