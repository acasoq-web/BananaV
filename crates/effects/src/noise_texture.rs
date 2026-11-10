//! VEGAS's Noise Texture as its pictures have it: a gradient noise in
//! three dimensions on a lattice of 256 random unit vectors (Perlin's
//! smoother step), its layers each twice as fine, mapped by the noise
//! style onto the 16-bit steps between Color 1 and Color 2 — worked out in
//! double precision step by step as VEGAS's renders show, a row at a
//! time: along a row the noise goes on from pixel to pixel by its slope
//! across the lattice's cell, worked out again at each new cell, and
//! those sums round as VEGAS's do.
//!
//! The card draws it in single precision from tables made here (each
//! layer's place across for every column and down for every row): the
//! same noise, a level apart at most here and there.

use crate::noise_tables::{GRADIENTS, PERM};

/// The generator's settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    /// Noise style: its place in the list.
    pub style: usize,
    pub layers: f64,
    pub swap: bool,
    /// Color 1 and Color 2, straight, with their alphas.
    pub colors: [[f64; 4]; 2],
    /// Frequency X and Y: the lattice's cells across the frame's width.
    pub frequency: [f64; 2],
    pub offset: [f64; 2],
    /// Offset's Progress: the noise's third dimension.
    pub progress: f64,
    pub min: f64,
    pub max: f64,
    pub bias: f64,
    pub amplitude: f64,
    /// Grain (amplitude falloff): each layer's strength next to the one
    /// before.
    pub falloff: f64,
}

/// A noise style: how the noise becomes a color (0: straight, 1: veins
/// across the frame, 2: rings), whether each layer counts as its size,
/// and its numbers.
struct Style {
    kind: u8,
    abs: bool,
    p: [f32; 5],
}

/// The styles in the list's order: Fractal - smooth, Fractal -
/// turbulent, Marble - light grain, Marble - heavy grain, Jagged lines,
/// Contours, Rings.
const STYLES: [Style; 7] = [
    Style {
        kind: 0,
        abs: false,
        p: [0.0; 5],
    },
    Style {
        kind: 0,
        abs: true,
        p: [0.0; 5],
    },
    Style {
        kind: 1,
        abs: true,
        p: [0.9, 0.15, 0.4, 1.2, 0.1],
    },
    Style {
        kind: 1,
        abs: true,
        p: [0.05, 0.9, 1.3, 0.0, 0.0],
    },
    Style {
        kind: 1,
        abs: true,
        p: [0.6, 0.0, 0.2, 0.5, 0.1],
    },
    Style {
        kind: 1,
        abs: false,
        p: [1.0, 0.0, 1.5, 0.5, 0.05],
    },
    Style {
        kind: 2,
        abs: false,
        p: [4.0, 0.0, 0.0, 0.0, 0.0],
    },
];

/// The noise's gain: its values reach about ±1 with it.
const GAIN: f64 = 50.0 / 33.0;
/// Added to the places before their whole parts are taken (the third
/// dimension's a quarter more).
const SHIFT: f64 = 1_000_000.0;
const SHIFT_Z: f64 = 1_000_000.25;
/// Added before a 16-bit step is rounded.
const NUDGE: f64 = 1.5e-8;

/// The nearest whole number, halves to the even one, as a 32-bit integer
/// (a double's low bits after 1.5 · 2⁵² is added).
fn rnd(a: f64) -> i32 {
    (a + 6_755_399_441_055_744.0).to_bits() as u32 as i32
}

/// The whole number at or below `a` (but within 1.5 · 10⁻⁸ below the next
/// one: that one).
fn piso(a: f64) -> i32 {
    rnd(a - 0.499_999_985)
}

/// `a`'s part past [`piso`].
fn frac(a: f64) -> f64 {
    a - f64::from(piso(a))
}

/// Perlin's smoother step, 6t⁵ − 15t⁴ + 10t³, multiplied as VEGAS does.
fn fade(t: f64) -> f64 {
    ((t * 6.0 - 15.0) * t + 10.0) * ((t * t) * t)
}

fn lerp(a: f64, b: f64, s: f64) -> f64 {
    (b - a) * s + a
}

/// Gradient `i`.
fn gradient(i: usize) -> [f64; 3] {
    GRADIENTS[i & 255].map(|b| f64::from(f32::from_bits(b)))
}

fn perm(i: usize) -> usize {
    usize::from(PERM[i & 255])
}

/// The lattice's hash of a row of cells, `z` then `y` (each 0–255).
fn hash(z: usize, y: usize) -> usize {
    perm(perm(z) + y)
}

/// A cell's corners along the x axis, worked out where the row is in it.
struct Cell {
    /// The noise at the cell's left corners and right ones, the row's place
    /// in y and z blended in.
    left: f64,
    right: f64,
    /// How those change a pixel to the right.
    step_left: f64,
    step_right: f64,
}

/// One layer's row of the noise: what the lattice's corners give, row by
/// row, along `x` at `y`, `z`.
struct Row {
    y: [usize; 2],
    z: [usize; 2],
    fy: f64,
    fz: f64,
    sy: f64,
    sz: f64,
}

impl Row {
    fn cell(&self, x: usize, fx: f64, dx: f64) -> Cell {
        let xs = [x & 255, ((x & 255) + 1) & 255];
        let dot = |g: [f64; 3], x: f64, y: f64, z: f64| (g[1] * y + g[0] * x) + g[2] * z;
        let corner =
            |i: usize, j: usize, k: usize| gradient(perm(hash(self.z[k], self.y[j]) + xs[i]));
        let side = |i: usize| {
            let fxi = if i == 0 { fx } else { fx - 1.0 };
            let g = [
                corner(i, 0, 0),
                corner(i, 0, 1),
                corner(i, 1, 0),
                corner(i, 1, 1),
            ];
            let d = [
                dot(g[0], fxi, self.fy, self.fz),
                dot(g[1], fxi, self.fy, self.fz - 1.0),
                dot(g[2], fxi, self.fy - 1.0, self.fz),
                dot(g[3], fxi, self.fy - 1.0, self.fz - 1.0),
            ];
            let value = lerp(
                lerp(d[0], d[1], self.sz),
                lerp(d[2], d[3], self.sz),
                self.sy,
            );
            let slope = lerp(
                lerp(g[0][0], g[1][0], self.sz),
                lerp(g[2][0], g[3][0], self.sz),
                self.sy,
            );
            (value, slope * dx)
        };
        let ((left, step_left), (right, step_right)) = (side(0), side(1));
        Cell {
            left,
            right,
            step_left,
            step_right,
        }
    }
}

/// A layer of noise along a row, `amp` of it added to `buf`: from `x`
/// on, a pixel `dx` further each, at `y` and `z`.
fn layer(x: f64, y: f64, z: f64, dx: f64, abs: bool, amp: f64, buf: &mut [f64]) {
    let g = amp * GAIN;
    let (xs, ys, zs) = (x + SHIFT, y + SHIFT, z + SHIFT_Z);
    let (mut ix, iy, iz) = (piso(xs), piso(ys), piso(zs));
    let mut fx = xs - f64::from(ix);
    let (fy, fz) = (ys - f64::from(iy), zs - f64::from(iz));
    let wrap = |i: i32| (i & 255) as usize;
    let row = Row {
        y: [wrap(iy), (wrap(iy) + 1) & 255],
        z: [wrap(iz), (wrap(iz) + 1) & 255],
        fy,
        fz,
        sy: fade(fy),
        sz: fade(fz),
    };
    let mut c = row.cell(wrap(ix), fx, dx);
    for v in buf.iter_mut() {
        let mut val = (fade(fx) * (c.right - c.left) + c.left) * g;
        if abs {
            val = val.abs();
        }
        *v += val;
        fx += dx;
        if fx < 1.0 {
            c.left += c.step_left;
            c.right += c.step_right;
        } else {
            let k = piso(fx);
            ix = ix.wrapping_add(k);
            fx -= f64::from(k);
            c = row.cell(wrap(ix), fx, dx);
        }
    }
}

/// The noise worked out for a picture.
#[derive(Clone, Debug)]
pub(crate) struct Noise {
    s: Settings,
    width: usize,
    height: usize,
    bytes: bool,
    /// The colors at the noise's low end and its high end, premultiplied
    /// (single precision).
    from: [f32; 4],
    to: [f32; 4],
    /// The 16-bit steps: so many a unit of noise, and where 0 lands (a
    /// half added).
    k: f64,
    b: f64,
    /// The layers' strengths and their sum.
    amps: Vec<f64>,
    sum: f64,
    /// A pixel's step across the lattice (the first layer's), and down
    /// it, negated.
    dx: f64,
    dy_neg: f64,
}

impl Noise {
    /// The noise of a picture `width` × `height` pixels, in bytes for an
    /// 8-bit project.
    pub(crate) fn new(s: &Settings, width: usize, height: usize, bytes: bool) -> Noise {
        let width = width.max(1);
        let pre = |c: [f64; 4]| {
            let a = c[3];
            [
                (a * c[0]) as f32,
                (a * c[1]) as f32,
                (a * c[2]) as f32,
                a as f32,
            ]
        };
        let (c1, c2) = (pre(s.colors[0]), pre(s.colors[1]));
        let (from, to) = if s.swap { (c2, c1) } else { (c1, c2) };
        // Min below Max (or either not a number): the noise spread between
        // them; else none, at their middle.
        let spread = matches!(
            s.min.partial_cmp(&s.max),
            Some(std::cmp::Ordering::Less) | None
        );
        let (k, b) = if spread {
            let k = (s.amplitude / (s.max - s.min)) * 65536.0;
            (k, s.bias * 65536.0 - s.min * k)
        } else {
            (
                0.0,
                ((s.min + s.max) * 0.5 * s.amplitude + s.bias) * 65536.0,
            )
        };
        // The layers: whole ones, each `falloff` of the one before, then
        // the part of one; those past a hundredth's strength left out.
        let n = piso(s.layers).clamp(0, 5);
        let part = s.layers - f64::from(n as u32);
        let mut amps = vec![1.0];
        for i in 1..=n as usize {
            let next = s.falloff * amps[i - 1];
            amps.push(next);
        }
        amps[n as usize] *= part;
        let kept = amps.iter().take_while(|&&a| a > 0.01).count();
        amps.truncate(kept);
        let sum = amps.iter().sum();
        let inv_w = 1.0 / f64::from(width as f32);
        Noise {
            s: *s,
            width,
            height: height.max(1),
            bytes,
            from,
            to,
            k,
            b: b + 0.5,
            amps,
            sum,
            dx: inv_w * s.frequency[0],
            dy_neg: (-inv_w) * s.frequency[1],
        }
    }

    fn style(&self) -> &'static Style {
        &STYLES[self.s.style.min(6)]
    }

    /// The layers' noise along row `yr` (from the top).
    fn noise_row(&self, yr: usize) -> Vec<f64> {
        let s = &self.s;
        let mut buf = vec![0.0; self.width];
        let mut dx = self.dx;
        let mut x = (s.offset[0] + 0.0) * s.frequency[0];
        let mut y = (f64::from(yr as f32 / self.width as f32) + s.offset[1]) * s.frequency[1];
        let mut z = s.progress;
        let abs = self.style().abs;
        for &amp in &self.amps {
            layer(x, y, z, dx, abs, amp, &mut buf);
            dx += dx;
            x += x;
            y += y;
            z += z;
        }
        buf
    }

    /// The type-1 styles' row term: where the veins are along the row's
    /// start.
    fn veins_row(&self, yr: usize) -> f64 {
        let s = &self.s;
        let p = self.style().p.map(f64::from);
        let w = self.width as f64;
        let mut a = -(w * s.offset[1] + yr as f64);
        a *= self.dy_neg;
        a *= p[1];
        if self.style().p[3] > 0.0 {
            let up = (self.height - 1 - yr) as f64;
            let mut wave = -((((s.offset[1] - 1000.0) * w + self.height as f64) - 1.0) - up);
            wave = wave * self.dy_neg * p[3];
            let fr = frac(wave);
            let onda = if 0.5 > fr {
                let t = 4.0 * fr - 1.0;
                1.0 - t * t
            } else {
                let t = 4.0 * fr - 3.0;
                t * t - 1.0
            };
            a += p[4] * onda;
        }
        (w * s.offset[0]) * (p[0] * self.dx) + a
    }

    /// Row `yr` (from the top): premultiplied, 0–1 (bytes' values in an
    /// 8-bit project).
    pub(crate) fn row(&self, yr: usize) -> Vec<[f32; 4]> {
        let f = self.noise_row(yr.min(self.height - 1));
        let style = self.style();
        let p = style.p.map(f64::from);
        let (k, b) = (self.k, self.b);
        let step: Box<dyn Fn(usize, f64) -> i32> = match style.kind {
            0 => {
                let (s0, b0) = if style.abs {
                    (self.sum, b)
                } else {
                    (self.sum + self.sum, b + k * 0.5)
                };
                let k0 = k / s0;
                Box::new(move |_, f| rnd((k0 * f + b0) + NUDGE))
            }
            1 => {
                let (ku, kx) = (p[2] * self.sum, p[0] * self.dx);
                let c = self.veins_row(yr);
                Box::new(move |i, f| {
                    let u = frac((ku * f + i as f64 * kx) + c);
                    let s = (u + u) - 1.0;
                    rnd(((1.0 - s * s) * k + b) + NUDGE)
                })
            }
            _ => {
                let (k2, kr) = (k * 0.5, p[0] * self.sum);
                Box::new(move |_, f| {
                    let u = frac((f + 2.0) * kr);
                    let ring = if 0.5 > u {
                        let t = 4.0 * u - 1.0;
                        2.0 - t * t
                    } else {
                        let t = 4.0 * u - 3.0;
                        t * t
                    };
                    rnd((ring * k2 + b) + NUDGE)
                })
            }
        };
        let rounded = style.kind == 0;
        f.iter()
            .enumerate()
            .map(|(i, &f)| {
                let mut v = step(i, f);
                if v | 0xFFFF != 0xFFFF {
                    v = if v < 0 { 0 } else { 0x10000 };
                }
                let t = f64::from(v) * (1.0 / 65536.0);
                let u = 1.0 - t;
                std::array::from_fn(|c| {
                    let out = f64::from(self.to[c]) * t + f64::from(self.from[c]) * u;
                    if self.bytes {
                        let byte = if rounded {
                            (out * 255.0 + 0.5) as i32
                        } else {
                            (out * 255.0) as i32
                        };
                        f32::from(byte as u8) / 255.0
                    } else {
                        out as f32
                    }
                })
            })
            .collect()
    }

    /// What the card draws from: its numbers (`f`: the two colors, the
    /// 16-bit steps' scale and start, the style's; `j`: the style's kind,
    /// whether each layer counts as its size, the layers, bytes) and its
    /// table — the lattice's 256 gradients with the permutation, each
    /// layer's place in the third dimension and strength, then each
    /// layer's place across every column, down every row, and each row's
    /// veins' term.
    pub(crate) fn plan(&self) -> (Vec<f32>, [u32; 4], Vec<[f32; 4]>) {
        let s = &self.s;
        let style = self.style();
        let p = style.p.map(f64::from);
        let (w, h, layers) = (self.width, self.height, self.amps.len());
        let mut t: Vec<[f32; 4]> = (0..256)
            .map(|i| {
                let g = gradient(i);
                [g[0] as f32, g[1] as f32, g[2] as f32, f32::from(PERM[i])]
            })
            .collect();
        // Each layer's place in z and strength.
        let mut z = s.progress;
        for &amp in &self.amps {
            let zs = z + SHIFT_Z;
            let iz = piso(zs);
            let fz = zs - f64::from(iz);
            t.push([
                (iz & 255) as f32,
                fz as f32,
                fade(fz) as f32,
                (amp * GAIN) as f32,
            ]);
            z += z;
        }
        // Each layer's place across every column.
        let mut x0 = (s.offset[0] + 0.0) * s.frequency[0];
        let mut dx = self.dx;
        for _ in 0..layers {
            for i in 0..w {
                let xs = (x0 + i as f64 * dx) + SHIFT;
                let ix = piso(xs);
                t.push([(ix & 255) as f32, (xs - f64::from(ix)) as f32, 0.0, 0.0]);
            }
            x0 += x0;
            dx += dx;
        }
        // Down every row, and the row's veins.
        let mut ys: Vec<f64> = (0..h)
            .map(|yr| (f64::from(yr as f32 / w as f32) + s.offset[1]) * s.frequency[1])
            .collect();
        for _ in 0..layers {
            for y in &mut ys {
                let at = *y + SHIFT;
                let iy = piso(at);
                let fy = at - f64::from(iy);
                t.push([(iy & 255) as f32, fy as f32, fade(fy) as f32, 0.0]);
                *y += *y;
            }
        }
        for yr in 0..h {
            let c = if style.kind == 1 {
                self.veins_row(yr)
            } else {
                0.0
            };
            t.push([c as f32, 0.0, 0.0, 0.0]);
        }
        let (k0, b0) = if style.abs {
            (self.k / self.sum, self.b)
        } else {
            (self.k / (self.sum + self.sum), self.b + self.k * 0.5)
        };
        let mut f: Vec<f32> = self.from.to_vec();
        f.extend(self.to);
        f.extend(
            [
                self.k,
                self.b,
                k0,
                b0,
                p[2] * self.sum,
                p[0] * self.dx,
                p[0] * self.sum,
                self.k * 0.5,
            ]
            .map(|v| v as f32),
        );
        let j = [
            u32::from(style.kind),
            u32::from(style.abs),
            layers as u32,
            u32::from(self.bytes),
        ];
        (f, j, t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            style: 0,
            layers: 1.0,
            swap: false,
            colors: [[0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]],
            frequency: [10.0, 10.0],
            offset: [0.0, 0.0],
            progress: 0.0,
            min: 0.0,
            max: 1.0,
            bias: 0.0,
            amplitude: 1.0,
            falloff: 0.5,
        }
    }

    #[test]
    fn rounding_is_halves_to_even_and_floors_are_nudged() {
        assert_eq!((rnd(2.5), rnd(3.5), rnd(-2.5), rnd(-0.4)), (2, 4, -2, 0));
        assert_eq!((piso(2.0), piso(2.7), piso(-0.3)), (2, 2, -1));
        // Within 1.5 · 10⁻⁸ of the next whole number: that one.
        assert_eq!(piso(2.999_999_999), 3);
    }

    #[test]
    fn layers_fall_off_and_a_part_of_one_counts() {
        let n = |layers: f64| {
            let s = Settings {
                layers,
                ..settings()
            };
            let noise = Noise::new(&s, 64, 36, true);
            (noise.amps.len(), noise.sum)
        };
        assert_eq!(n(1.0), (1, 1.0));
        assert_eq!(n(4.0), (4, 1.875));
        assert_eq!(n(3.5), (4, 1.8125));
    }

    #[test]
    fn the_noise_stays_between_its_colors_and_moves_with_progress() {
        let noise = Noise::new(&settings(), 160, 90, true);
        let row = noise.row(45);
        assert!(
            row.iter()
                .all(|c| c[0] >= 0.0 && c[0] <= 1.0 && c[3] == 1.0)
        );
        let spread = row.iter().map(|c| c[0]).fold(0f32, f32::max)
            - row.iter().map(|c| c[0]).fold(1f32, f32::min);
        assert!(spread > 0.2, "{spread}");
        let later = Noise::new(
            &Settings {
                progress: 0.5,
                ..settings()
            },
            160,
            90,
            true,
        );
        assert_ne!(later.row(45), row);
    }

    #[test]
    fn going_on_from_pixel_to_pixel_stays_with_the_noise() {
        // The incremental row against the noise worked out at each pixel.
        let s = Settings {
            layers: 3.0,
            ..settings()
        };
        let noise = Noise::new(&s, 200, 10, false);
        let f = noise.noise_row(3);
        let (_, _, t) = noise.plan();
        let at = |i: usize| -> f64 {
            let mut sum = 0.0;
            let base = 256;
            for (k, amp) in noise.amps.iter().enumerate() {
                let zk = t[base + k];
                let col = t[base + noise.amps.len() + k * 200 + i];
                let row = t[base + noise.amps.len() + 3 * 200 + k * 10 + 3];
                let (x0, y0, z0) = (col[0] as usize, row[0] as usize, zk[0] as usize);
                let r = Row {
                    y: [y0, (y0 + 1) & 255],
                    z: [z0, (z0 + 1) & 255],
                    fy: f64::from(row[1]),
                    fz: f64::from(zk[1]),
                    sy: f64::from(row[2]),
                    sz: f64::from(zk[2]),
                };
                let fx = f64::from(col[1]);
                let c = r.cell(x0, fx, 0.0);
                sum += (fade(fx) * (c.right - c.left) + c.left) * amp * GAIN;
            }
            sum
        };
        for i in [0, 1, 17, 99, 199] {
            assert!((f[i] - at(i)).abs() < 1e-5, "{i}: {} {}", f[i], at(i));
        }
    }
}
