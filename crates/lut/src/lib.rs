//! Look-up tables of colors in the `.cube` format: Adobe's Cube LUT
//! Specification 1.0 (a 1D table, a curve per channel, or a 3D table, a
//! lattice of colors) and DaVinci Resolve's variant (a 1D shaper before a
//! 3D table, each with its input range). Colors between the table's
//! points are worked out linearly (trilinearly in 3D) or, in 3D,
//! tetrahedrally — the math BananaV's graphics card kernels repeat, in the
//! same order, on 32-bit floats.

use std::fmt;

/// A table read from a `.cube` file.
#[derive(Clone, Debug, PartialEq)]
pub struct Lut {
    /// Its `TITLE` (empty when it has none).
    pub title: String,
    /// A curve per channel, applied first.
    pub shaper: Option<Table1>,
    /// The lattice, applied next.
    pub cube: Option<Table3>,
}

/// A curve per channel: its entries spread evenly over `min`–`max`.
#[derive(Clone, Debug, PartialEq)]
pub struct Table1 {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub entries: Vec<[f32; 3]>,
}

/// A lattice of `size`³ colors over `min`–`max` each way, red changing
/// fastest, then green, then blue (the format's order).
#[derive(Clone, Debug, PartialEq)]
pub struct Table3 {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub size: usize,
    pub entries: Vec<[f32; 3]>,
}

/// How colors between a lattice's points are worked out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Interpolation {
    /// From the 8 points around (trilinear).
    #[default]
    Linear,
    /// From the 4 points of the tetrahedron around: closer to how the
    /// table was made, grays staying gray.
    Tetrahedral,
}

/// Why a file is not a table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// A line the format does not have (its number, from 1).
    Line(usize, String),
    /// Neither `LUT_1D_SIZE` nor `LUT_3D_SIZE`.
    NoSize,
    /// A size beyond the format's limits (2–65536 in 1D, 2–256 in 3D).
    Size(usize),
    /// Not as many colors as the sizes ask for.
    Count { expected: usize, found: usize },
    /// The file could not be read.
    Io(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Line(n, l) => write!(f, "line {n} is not .cube: {l}"),
            Error::NoSize => write!(f, "no LUT_1D_SIZE or LUT_3D_SIZE"),
            Error::Size(n) => write!(f, "size {n} out of range"),
            Error::Count { expected, found } => {
                write!(f, "{found} colors where the sizes ask for {expected}")
            }
            Error::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl Lut {
    /// The table in a `.cube` file's text.
    pub fn parse(text: &str) -> Result<Lut, Error> {
        let mut title = String::new();
        let (mut size1, mut size3) = (None, None);
        let (mut domain_min, mut domain_max) = ([0.0f32; 3], [1.0f32; 3]);
        let (mut range1, mut range3) = (None, None);
        let mut rows: Vec<[f32; 3]> = Vec::new();
        for (i, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let bad = || Error::Line(i + 1, raw.to_string());
            let mut words = line.split_whitespace();
            let first = words.next().unwrap_or_default();
            let floats = |words: std::str::SplitWhitespace<'_>| -> Result<Vec<f32>, Error> {
                words.map(|w| w.parse::<f32>().map_err(|_| bad())).collect()
            };
            let three = |v: Vec<f32>| -> Result<[f32; 3], Error> {
                <[f32; 3]>::try_from(v.as_slice()).map_err(|_| bad())
            };
            let pair = |v: Vec<f32>| -> Result<[f32; 2], Error> {
                <[f32; 2]>::try_from(v.as_slice()).map_err(|_| bad())
            };
            let size = |mut words: std::str::SplitWhitespace<'_>| -> Result<usize, Error> {
                words
                    .next()
                    .and_then(|w| w.parse::<usize>().ok())
                    .ok_or_else(bad)
            };
            match first {
                "TITLE" => {
                    title = line["TITLE".len()..].trim().trim_matches('"').to_string();
                }
                "LUT_1D_SIZE" => size1 = Some(size(words)?),
                "LUT_3D_SIZE" => size3 = Some(size(words)?),
                "DOMAIN_MIN" => domain_min = three(floats(words)?)?,
                "DOMAIN_MAX" => domain_max = three(floats(words)?)?,
                "LUT_1D_INPUT_RANGE" => range1 = Some(pair(floats(words)?)?),
                "LUT_3D_INPUT_RANGE" => range3 = Some(pair(floats(words)?)?),
                w if w
                    .starts_with(|c: char| c.is_ascii_digit() || matches!(c, '-' | '+' | '.')) =>
                {
                    rows.push(three(floats(line.split_whitespace())?)?);
                }
                // Other programs' keywords (upper case and underscores).
                w if w
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit()) => {}
                _ => return Err(bad()),
            }
        }
        if size1.is_none() && size3.is_none() {
            return Err(Error::NoSize);
        }
        if let Some(n) = size1
            && !(2..=65536).contains(&n)
        {
            return Err(Error::Size(n));
        }
        if let Some(n) = size3
            && !(2..=256).contains(&n)
        {
            return Err(Error::Size(n));
        }
        let n1 = size1.unwrap_or(0);
        let n3 = size3.map_or(0, |n| n * n * n);
        if rows.len() != n1 + n3 {
            return Err(Error::Count {
                expected: n1 + n3,
                found: rows.len(),
            });
        }
        // Each table over its own input range, or the file's domain.
        let span = |range: Option<[f32; 2]>| match range {
            Some([lo, hi]) => ([lo; 3], [hi; 3]),
            None => (domain_min, domain_max),
        };
        let cube = rows.split_off(n1);
        Ok(Lut {
            title,
            shaper: size1.map(|_| {
                let (min, max) = span(range1);
                Table1 {
                    min,
                    max,
                    entries: rows,
                }
            }),
            cube: size3.map(|size| {
                let (min, max) = span(range3);
                Table3 {
                    min,
                    max,
                    size,
                    entries: cube,
                }
            }),
        })
    }

    /// The table in the `.cube` file at `path`.
    pub fn read(path: &std::path::Path) -> Result<Lut, Error> {
        let text = std::fs::read_to_string(path).map_err(|e| Error::Io(e.to_string()))?;
        Lut::parse(&text)
    }

    /// The color `rgb` becomes.
    pub fn apply(&self, rgb: [f32; 3], how: Interpolation) -> [f32; 3] {
        let mut c = rgb;
        if let Some(s) = &self.shaper {
            c = s.apply(c);
        }
        if let Some(t) = &self.cube {
            c = t.apply(c, how);
        }
        c
    }
}

/// Where `v` is from `min` to `max`, 0–1 (the ends beyond them).
fn unit(v: f32, min: f32, max: f32) -> f32 {
    let t = (v - min) / (max - min).max(1e-12);
    if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

impl Table1 {
    pub fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        let n = self.entries.len();
        std::array::from_fn(|k| {
            let t = unit(c[k], self.min[k], self.max[k]) * (n - 1) as f32;
            let i = (t as usize).min(n - 2);
            lerp(self.entries[i][k], self.entries[i + 1][k], t - i as f32)
        })
    }
}

impl Table3 {
    fn at(&self, r: usize, g: usize, b: usize) -> [f32; 3] {
        self.entries[r + self.size * (g + self.size * b)]
    }

    pub fn apply(&self, c: [f32; 3], how: Interpolation) -> [f32; 3] {
        let top = (self.size - 1) as f32;
        let t: [f32; 3] = std::array::from_fn(|k| unit(c[k], self.min[k], self.max[k]) * top);
        let i: [usize; 3] = std::array::from_fn(|k| (t[k] as usize).min(self.size - 2));
        let [fr, fg, fb]: [f32; 3] = std::array::from_fn(|k| t[k] - i[k] as f32);
        let p = |r: usize, g: usize, b: usize| self.at(i[0] + r, i[1] + g, i[2] + b);
        match how {
            Interpolation::Linear => {
                // Along red, then green, then blue.
                let along = |a: [f32; 3], b: [f32; 3], t: f32| -> [f32; 3] {
                    std::array::from_fn(|k| lerp(a[k], b[k], t))
                };
                let c00 = along(p(0, 0, 0), p(1, 0, 0), fr);
                let c10 = along(p(0, 1, 0), p(1, 1, 0), fr);
                let c01 = along(p(0, 0, 1), p(1, 0, 1), fr);
                let c11 = along(p(0, 1, 1), p(1, 1, 1), fr);
                along(along(c00, c10, fg), along(c01, c11, fg), fb)
            }
            Interpolation::Tetrahedral => {
                // The tetrahedron around the point: its corners and their
                // weights.
                let (w, q) = if fr > fg {
                    if fg > fb {
                        ([1.0 - fr, fr - fg, fg - fb, fb], [p(1, 0, 0), p(1, 1, 0)])
                    } else if fr > fb {
                        ([1.0 - fr, fr - fb, fb - fg, fg], [p(1, 0, 0), p(1, 0, 1)])
                    } else {
                        ([1.0 - fb, fb - fr, fr - fg, fg], [p(0, 0, 1), p(1, 0, 1)])
                    }
                } else if fb > fg {
                    ([1.0 - fb, fb - fg, fg - fr, fr], [p(0, 0, 1), p(0, 1, 1)])
                } else if fb > fr {
                    ([1.0 - fg, fg - fb, fb - fr, fr], [p(0, 1, 0), p(0, 1, 1)])
                } else {
                    ([1.0 - fg, fg - fr, fr - fb, fb], [p(0, 1, 0), p(1, 1, 0)])
                };
                let (a, d) = (p(0, 0, 0), p(1, 1, 1));
                std::array::from_fn(|k| w[0] * a[k] + w[1] * q[0][k] + w[2] * q[1][k] + w[3] * d[k])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lattice of `n`³ colors, each `f` of its place (0–1).
    fn lattice(n: usize, f: impl Fn([f32; 3]) -> [f32; 3]) -> String {
        let mut s = format!("TITLE \"Test\"\nLUT_3D_SIZE {n}\n");
        let k = (n - 1) as f32;
        for b in 0..n {
            for g in 0..n {
                for r in 0..n {
                    let c = f([r as f32 / k, g as f32 / k, b as f32 / k]);
                    s += &format!("{} {} {}\n", c[0], c[1], c[2]);
                }
            }
        }
        s
    }

    fn near(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5)
    }

    #[test]
    fn an_identity_lattice_changes_nothing_either_way() {
        let lut = Lut::parse(&lattice(5, |c| c)).unwrap();
        assert_eq!(lut.title, "Test");
        for c in [[0.1, 0.5, 0.9], [0.33, 0.33, 0.33], [1.0, 0.0, 0.7]] {
            for how in [Interpolation::Linear, Interpolation::Tetrahedral] {
                assert!(near(lut.apply(c, how), c), "{c:?} {how:?}");
            }
        }
        // Outside the domain: its edge.
        assert!(near(
            lut.apply([-0.5, 2.0, 0.5], Interpolation::Linear),
            [0.0, 1.0, 0.5]
        ));
    }

    #[test]
    fn red_changes_fastest() {
        // Red and blue swapped.
        let lut = Lut::parse(&lattice(3, |[r, g, b]| [b, g, r])).unwrap();
        assert!(near(
            lut.apply([1.0, 0.5, 0.0], Interpolation::Linear),
            [0.0, 0.5, 1.0]
        ));
    }

    #[test]
    fn tetrahedral_follows_the_lattices_diagonal() {
        // Two points each way: red times green, green times blue, blue
        // times red.
        let lut = Lut::parse(&lattice(2, |[r, g, b]| [r * g, g * b, b * r])).unwrap();
        // A gray: tetrahedral goes along the diagonal, from black to white;
        // trilinear mixes all eight corners.
        let gray = [0.5, 0.5, 0.5];
        assert!(near(lut.apply(gray, Interpolation::Tetrahedral), [0.5; 3]));
        assert!(near(lut.apply(gray, Interpolation::Linear), [0.25; 3]));
        // Elsewhere: trilinear 0.8 × 0.2, tetrahedral the corner (1, 1, 1)
        // weighing 0.2.
        let c = [0.8, 0.2, 0.5];
        assert!((lut.apply(c, Interpolation::Linear)[0] - 0.16).abs() < 1e-6);
        assert!((lut.apply(c, Interpolation::Tetrahedral)[0] - 0.2).abs() < 1e-6);
    }

    #[test]
    fn a_1d_table_bends_each_channel() {
        let text = "# a comment\nLUT_1D_SIZE 3\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 1 1 2\n0 0 0\n0.25 1 0.5\n1 1 1\n";
        let lut = Lut::parse(text).unwrap();
        let c = lut.apply([0.5, 0.25, 1.0], Interpolation::Linear);
        // Half way: the middle entry; a quarter: half way to it; blue's
        // domain is 0–2.
        assert!(near(c, [0.25, 0.5, 0.5]), "{c:?}");
    }

    #[test]
    fn resolves_shaper_comes_before_its_lattice() {
        let mut text = String::from("LUT_1D_SIZE 2\nLUT_1D_INPUT_RANGE 0 2\nLUT_3D_SIZE 2\n");
        text += "0 0 0\n1 1 1\n";
        for b in 0..2 {
            for g in 0..2 {
                for r in 0..2 {
                    text += &format!("{r} {g} {b}\n");
                }
            }
        }
        let lut = Lut::parse(&text).unwrap();
        assert!(lut.shaper.is_some() && lut.cube.is_some());
        // 1 of 0–2 is half way.
        assert!(near(
            lut.apply([1.0, 2.0, 0.0], Interpolation::Linear),
            [0.5, 1.0, 0.0]
        ));
    }

    #[test]
    fn what_is_not_a_cube_says_why() {
        assert_eq!(Lut::parse("0 0 0\n"), Err(Error::NoSize));
        assert_eq!(Lut::parse("LUT_3D_SIZE 1\n"), Err(Error::Size(1)));
        assert_eq!(
            Lut::parse("LUT_1D_SIZE 2\n0 0 0\n"),
            Err(Error::Count {
                expected: 2,
                found: 1
            })
        );
        assert!(matches!(
            Lut::parse("LUT_1D_SIZE 2\n0 0\n1 1 1\n"),
            Err(Error::Line(2, _))
        ));
        assert!(matches!(
            Lut::parse("lut_3d_size 2\n"),
            Err(Error::Line(1, _))
        ));
        // Others' keywords pass.
        assert!(Lut::parse("LUT_1D_SIZE 2\nLUT_IN_VIDEO_RANGE\n0 0 0\n1 1 1\n").is_ok());
    }
}
