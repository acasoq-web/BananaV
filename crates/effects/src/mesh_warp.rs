//! Mesh Warp, as VEGAS's: a grid over the picture whose points are dragged
//! on the preview, the picture following them. Each point says where the
//! grid at rest's point goes (Mesh point refers to Target, the hidden
//! default): every cell split in two triangles along its shorter diagonal
//! (the other one when those overlap), each mapping the picture's
//! triangle at rest onto the mesh's — worked out on a picture halved until
//! its shorter side is under 480 pixels, then enlarged —; or where each
//! pixel of the grid at rest reads (Source: each cell's four points mixed
//! bilinearly). The picture is read where that map says between four
//! pixels in 1024ths, as Lens Correction reads it. A dragged point stays
//! inside the ring of its eight neighbours; Elasticity pulls the other
//! points along by a Gaussian of their distance (Shift: only its
//! neighbours), Fixed Border Points keeps the border's points where they
//! are; a double click puts a point back, Reset Mesh all of them. The
//! grid's size changes only while the mesh is at rest and not animated.
//! The mesh is a custom parameter (each point's place as text) the plug-in
//! interpolates between keyframes. On the card `kit_mesh_warp`, the map's
//! table worked out here (`mesh_warp_table`).
#![allow(unsafe_code)]

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::{Plan, Values};
use crate::kit::{Buf, Spec, Vals, define};
use crate::ofx::{Clicks, Draw, Fx, Props, interact_props, is, redraw, suites};

pub(crate) const ID: &CStr = c"org.bananav.MeshWarp";

const MESH: &CStr = c"mesh";
const DIRECTION: &CStr = c"meshPointRefersTo";
const GRID_X: &CStr = c"gridX";
const GRID_Y: &CStr = c"gridY";
const LOCK: &CStr = c"lock";
const ENABLE: &CStr = c"enableWarp";
const GROUP: &CStr = c"meshBehavior";
const ELASTICITY: &CStr = c"elasticity";
const FIXED_BORDER: &CStr = c"fixedBorder";
const RESET: &CStr = c"resetMesh";

/// The most cells a side of the grid has.
const MOST: i32 = 60;

const SPECS: &[Spec] = &[
    Spec::Choice(
        DIRECTION,
        c"Mesh point refers to",
        &[c"Source", c"Target"],
        1,
        c"",
    ),
    Spec::Int(GRID_X, c"Grid size in X", 8, 1, MOST, c"Grid size in X"),
    Spec::Int(GRID_Y, c"Grid size in Y", 8, 1, MOST, c"Grid size in Y"),
    Spec::Toggle(
        LOCK,
        c"Lock Grid size in X and Y",
        true,
        c"Keeps grid size in X and Y equal.",
    ),
    Spec::Toggle(ENABLE, c"Enable Warp Effect", true, c"Enable Warp Effect"),
    Spec::Group(GROUP, c"Mesh Behavior"),
    Spec::Ranged(
        ELASTICITY,
        c"Elasticity",
        0.0,
        [0.0, 3.0],
        [0.0, 1.0],
        c"Determines how strongly an adjustment to one mesh point affects all other points.  Hold Shift to limit the effect to only directly neighboring points.",
    ),
    Spec::Toggle(
        FIXED_BORDER,
        c"Fixed Border Points",
        false,
        c"Keeps the mesh points at the border fixed.",
    ),
    Spec::GroupEnd,
];

// The mesh.

/// The grid's points: `cols` × `rows` of them (the cells and one), row by
/// row from the bottom, each its place in parts of the picture (across,
/// up).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Mesh {
    cols: usize,
    rows: usize,
    p: Vec<[f32; 2]>,
}

impl Mesh {
    /// The grid at rest of `cols` × `rows` points.
    fn rest(cols: usize, rows: usize) -> Mesh {
        let (cols, rows) = (cols.max(2), rows.max(2));
        let (sx, sy) = (1.0 / (cols - 1) as f32, 1.0 / (rows - 1) as f32);
        Mesh {
            cols,
            rows,
            p: (0..rows)
                .flat_map(|r| (0..cols).map(move |c| [c as f32 * sx, r as f32 * sy]))
                .collect(),
        }
    }

    /// The mesh `text` keeps (its cells across and up, then each point's
    /// place); `None` for an empty text (the grid at rest) or not a mesh.
    fn parse(text: &str) -> Option<Mesh> {
        let mut it = text.split_ascii_whitespace();
        let nx: usize = it.next()?.parse().ok()?;
        let ny: usize = it.next()?.parse().ok()?;
        let most = MOST as usize;
        if !(1..=most).contains(&nx) || !(1..=most).contains(&ny) {
            return None;
        }
        let v: Vec<f32> = it
            .map(|s| s.parse::<f32>().ok().filter(|v| v.is_finite()))
            .collect::<Option<_>>()?;
        (v.len() == 2 * (nx + 1) * (ny + 1)).then(|| Mesh {
            cols: nx + 1,
            rows: ny + 1,
            p: v.chunks(2).map(|c| [c[0], c[1]]).collect(),
        })
    }

    /// The mesh `text` keeps, the grid at rest of `cells` for an empty one.
    fn read(text: &str, cells: [usize; 2]) -> Mesh {
        Mesh::parse(text).unwrap_or_else(|| Mesh::rest(cells[0] + 1, cells[1] + 1))
    }

    /// As text: its cells, then each point's place.
    fn text(&self) -> String {
        let mut s = format!("{} {}", self.cols - 1, self.rows - 1);
        for [x, y] in &self.p {
            s.push_str(&format!(" {x} {y}"));
        }
        s
    }

    fn index(&self, c: usize, r: usize) -> usize {
        r * self.cols + c
    }

    /// Point `n`'s place at rest.
    fn rest_at(&self, n: usize) -> [f32; 2] {
        let (c, r) = (n % self.cols, n / self.cols);
        [
            c as f32 * (1.0 / (self.cols - 1) as f32),
            r as f32 * (1.0 / (self.rows - 1) as f32),
        ]
    }

    /// Whether every point is within `reach` of its place at rest.
    fn at_rest(&self, reach: f32) -> bool {
        let reach = reach * reach;
        self.p.iter().enumerate().all(|(n, p)| {
            let r = self.rest_at(n);
            let (dx, dy) = (r[0] - p[0], r[1] - p[1]);
            dy * dy + dx * dx < reach
        })
    }

    fn on_border(&self, n: usize) -> bool {
        let (c, r) = (n % self.cols, n / self.cols);
        c == 0 || r == 0 || c == self.cols - 1 || r == self.rows - 1
    }
}

/// The mesh `t` of the way from text `a` to text `b` (two keyframes'):
/// each point between, as a float rounds it, when both have the grid's
/// size (an empty text the grid at rest of the other's); else the first
/// up to half way, then the second.
fn between(a: &str, b: &str, t: f64) -> String {
    if t < f64::EPSILON {
        return a.to_string();
    }
    if t > 1.0 - f64::EPSILON {
        return b.to_string();
    }
    let (ma, mb) = match (Mesh::parse(a), Mesh::parse(b)) {
        (Some(x), Some(y)) => (x, y),
        (Some(x), None) if b.trim().is_empty() => {
            let rest = Mesh::rest(x.cols, x.rows);
            (x, rest)
        }
        (None, Some(y)) if a.trim().is_empty() => (Mesh::rest(y.cols, y.rows), y),
        _ => return if t >= 0.5 { b } else { a }.to_string(),
    };
    if ma.cols != mb.cols || ma.rows != mb.rows {
        return if t >= 0.5 { b } else { a }.to_string();
    }
    let p =
        ma.p.iter()
            .zip(&mb.p)
            .map(|(p, q)| std::array::from_fn(|k| (f64::from(q[k] - p[k]) * t) as f32 + p[k]))
            .collect();
    Mesh { p, ..ma }.text()
}

// The map.

/// A triangle: its three points.
type Triangle = [[f32; 2]; 3];

/// The cells' triangles where the mesh puts them (targets) and where the
/// grid at rest has them (sources), cell by cell, row by row; and whether
/// the last cell's two don't overlap.
fn triangles(m: &Mesh) -> (Vec<Triangle>, Vec<Triangle>, bool) {
    // The two ways to split a cell (its corners 0–3 around it): along
    // 1–3, or along 0–2.
    const SPLITS: [[usize; 6]; 2] = [[0, 3, 1, 1, 3, 2], [0, 3, 2, 0, 2, 1]];
    let n = (m.cols - 1) * (m.rows - 1) * 2;
    let (mut targets, mut sources) = (Vec::with_capacity(n), Vec::with_capacity(n));
    let mut valid = true;
    let (sx, sy) = (1.0 / (m.cols - 1) as f32, 1.0 / (m.rows - 1) as f32);
    for r in 1..m.rows {
        for c in 1..m.cols {
            let at = |c: usize, r: usize| m.p[m.index(c, r)];
            let t = [at(c - 1, r - 1), at(c, r - 1), at(c, r), at(c - 1, r)];
            let s = [
                [(c - 1) as f32 * sx, (r - 1) as f32 * sy],
                [c as f32 * sx, (r - 1) as f32 * sy],
                [c as f32 * sx, r as f32 * sy],
                [(c - 1) as f32 * sx, r as f32 * sy],
            ];
            let d02 = (t[0][0] - t[2][0]).powi(2) + (t[0][1] - t[2][1]).powi(2);
            let d13 = (t[1][0] - t[3][0]).powi(2) + (t[1][1] - t[3][1]).powi(2);
            let order = if d02 < d13 { [1, 0] } else { [0, 1] };
            let mut pair = ([[[0.0; 2]; 3]; 2], [[[0.0; 2]; 3]; 2]);
            for (k, &split) in order.iter().enumerate() {
                let v = SPLITS[split];
                let tri = |q: &[[f32; 2]; 4], a: usize| {
                    counterclockwise([q[v[a]], q[v[a + 1]], q[v[a + 2]]])
                };
                pair = ([tri(&t, 0), tri(&t, 3)], [tri(&s, 0), tri(&s, 3)]);
                if !overlap(&pair.0[0], &pair.0[1]) || k == 1 {
                    break;
                }
            }
            valid = !overlap(&pair.0[0], &pair.0[1]);
            sources.extend(pair.1);
            targets.extend(pair.0);
        }
    }
    (targets, sources, valid)
}

/// `t` with its last two points swapped when it turns clockwise.
fn counterclockwise(t: Triangle) -> Triangle {
    let [a, b, c] = t;
    if (b[1] - c[1]) * a[0] + (c[1] - a[1]) * b[0] + (a[1] - b[1]) * c[0] < 0.0 {
        [a, c, b]
    } else {
        t
    }
}

/// Whether two triangles overlap: no side of either has the other's
/// three points on its outer side (within 0.0001).
fn overlap(a: &Triangle, b: &Triangle) -> bool {
    const ROOM: f64 = 1e-4;
    let outside = |t: &Triangle, o: &Triangle| {
        (0..3).any(|i| {
            let (p, q) = (t[i], t[(i + 1) % 3]);
            let dy = p[1] - q[1];
            o.iter()
                .all(|v| f64::from((q[1] - v[1]) * p[0] + (v[1] - p[1]) * q[0] + v[0] * dy) <= ROOM)
        })
    };
    !(outside(a, b) || outside(b, a))
}

/// Where point `p` lies in triangle `t`: its three weights, when the
/// triangle is not flat.
fn weights_in(t: &Triangle, p: [f32; 2]) -> Option<[f32; 3]> {
    let [a, b, c] = *t;
    let d = (a[1] - c[1]) * (c[0] - b[0]) + (a[0] - c[0]) * (b[1] - c[1]);
    if d == 0.0 {
        return None;
    }
    let w0 = ((p[1] - c[1]) * (c[0] - b[0]) + (p[0] - c[0]) * (b[1] - c[1])) / d;
    let w1 = ((c[1] - a[1]) * (p[0] - c[0]) + (p[1] - c[1]) * (a[0] - c[0])) / d;
    Some([w0, w1, (1.0 - w0) - w1])
}

/// Whether `p` lies in the box around `t` (its edges in).
fn in_box(t: &Triangle, p: [f32; 2]) -> bool {
    (0..2).all(|k| {
        let lo = t[0][k].min(t[1][k]).min(t[2][k]);
        let hi = t[0][k].max(t[1][k]).max(t[2][k]);
        lo <= p[k] && p[k] <= hi
    })
}

/// The size the triangles are drawn at for a `w` × `h` picture: halved
/// until its shorter side is under 480 pixels.
fn reduced(w: u32, h: u32) -> [u32; 2] {
    let (mut w, mut h) = (w, h);
    while w.min(h) > 479 {
        w >>= 1;
        h >>= 1;
    }
    [w, h]
}

/// The side, in the reduced map's pixels, of the squares its triangles are
/// looked up by (each square lists those whose boxes may reach it, in
/// their order, so the first that holds a point is the one the whole list
/// would give).
const SQUARE: usize = 16;

/// The map Target's triangles draw on a `w` × `h` reduced picture: for
/// each of its pixels (rows up) where the picture at rest is read, in
/// parts of it — (−1, −1) where no triangle reaches.
fn small_map(m: &Mesh, [w, h]: [u32; 2]) -> Vec<[f32; 2]> {
    let (targets, sources, _) = triangles(m);
    let (wf, hf) = ((w.max(2) - 1) as f32, (h.max(2) - 1) as f32);
    let (cols, rows) = ((w as usize).div_ceil(SQUARE), (h as usize).div_ceil(SQUARE));
    let mut squares: Vec<Vec<u32>> = vec![Vec::new(); cols * rows];
    for (k, t) in targets.iter().enumerate() {
        let lo = [0, 1].map(|i| t[0][i].min(t[1][i]).min(t[2][i]));
        let hi = [0, 1].map(|i| t[0][i].max(t[1][i]).max(t[2][i]));
        // The pixels the box may reach, a pixel of room either way.
        let span = |lo: f32, hi: f32, scale: f32, n: usize| {
            let a = ((lo * scale).floor() - 1.0).max(0.0);
            let b = ((hi * scale).ceil() + 1.0).min(n as f32 - 1.0);
            (a <= b && hi >= lo).then_some(((a as usize) / SQUARE, (b as usize) / SQUARE))
        };
        let (Some((x0, x1)), Some((y0, y1))) = (
            span(lo[0], hi[0], wf, w as usize),
            span(lo[1], hi[1], hf, h as usize),
        ) else {
            continue;
        };
        for sy in y0..=y1 {
            for sx in x0..=x1 {
                squares[sy * cols + sx].push(k as u32);
            }
        }
    }
    let mut map = vec![[-1.0f32; 2]; (w * h) as usize];
    crate::kit::for_chunks(&mut map, w as usize, |y, row| {
        for (x, out) in row.iter_mut().enumerate() {
            let p = [x as f32 / wf, y as f32 / hf];
            let list = &squares[(y / SQUARE) * cols + x / SQUARE];
            for &k in list {
                let t = &targets[k as usize];
                if !in_box(t, p) {
                    continue;
                }
                let Some(b) = weights_in(t, p) else { continue };
                let outside = b.iter().any(|&v| !(0.0..=1.0).contains(&v));
                if outside && b.iter().fold(f32::INFINITY, |m, v| m.min(v.abs())) >= 1e-6 {
                    continue;
                }
                let s = &sources[k as usize];
                *out = std::array::from_fn(|i| b[0] * s[0][i] + b[1] * s[1][i] + b[2] * s[2][i]);
                break;
            }
        }
    });
    map
}

/// The map's place for pixel (x, y) of a `w` × `h` picture (rows up),
/// enlarged from the reduced `map` (`mw` × `mh`) between its four values
/// — past its last value the one before held —; `ratio`: `(mw − 1) /
/// (w − 1)` and `(mh − 1) / (h − 1)`.
fn enlarged(map: &[[f32; 2]], mw: u32, ratio: [f32; 2], x: u32, y: u32) -> [f32; 2] {
    let ty = y as f32 * ratio[1];
    let iy = ty as u32;
    let fy = ty - iy as f32;
    let tx = x as f32 * ratio[0];
    let ix = tx as u32;
    let fx = tx - ix as f32;
    let n = map.len();
    let i = (mw * iy + ix) as usize;
    let up = i + mw as usize;
    let m00 = map[i];
    let m10 = if i + 1 < n { map[i + 1] } else { m00 };
    let m01 = if up < n { map[up] } else { m00 };
    let m11 = if up + 1 < n { map[up + 1] } else { m01 };
    let (gx, gy) = (1.0 - fx, 1.0 - fy);
    std::array::from_fn(|k| {
        m10[k] * fx * gy + gx * m00[k] * gy + m01[k] * fy * gx + fy * fx * m11[k]
    })
}

/// Each column's (or row's) cell and share of the way across it.
type Steps = Vec<(u32, f32)>;

/// Source's map on a `w` × `h` picture: for each column its cell (the
/// point column on its right) and its share of the way across it, then
/// each row's — the cells covering the pixels from the rounded multiple of
/// their size, the shares added up pixel by pixel.
fn source_steps(m: &Mesh, w: u32, h: u32) -> (Steps, Steps) {
    let steps = |n: u32, points: usize| {
        let cell = n as f32 / (points - 1) as f32;
        let mut out = vec![(1u32, 0.0f32); n as usize];
        for k in 1..points {
            let a = (cell * (k - 1) as f32).round() as u32;
            let b = (k as f32 * cell).round() as u32;
            let mut f = 0.0f32;
            for i in a..b.min(n) {
                out[i as usize] = (k as u32, f);
                f += 1.0 / cell;
            }
        }
        out
    };
    (steps(w, m.cols), steps(h, m.rows))
}

/// Source's place for pixel (x, y): its cell's four points mixed.
fn source_at(m: &Mesh, cols: &[(u32, f32)], rows: &[(u32, f32)], x: u32, y: u32) -> [f32; 2] {
    let ((c, fx), (r, fy)) = (cols[x as usize], rows[y as usize]);
    let (c, r) = (c as usize, r as usize);
    let at = |c: usize, r: usize| m.p[m.index(c, r)];
    let (p10, p00, p01, p11) = (at(c, r - 1), at(c - 1, r - 1), at(c - 1, r), at(c, r));
    let (w1, w0, w2) = ((1.0 - fy) * fx, (1.0 - fy) * (1.0 - fx), (1.0 - fx) * fy);
    std::array::from_fn(|k| p10[k] * w1 + p00[k] * w0 + p01[k] * w2 + p11[k] * fx * fy)
}

/// A reduced map kept: its mesh's text, the reduced size and the map.
type Kept = (String, [u32; 2], Arc<Vec<[f32; 2]>>);

/// The reduced map of the last mesh drawn on the processor, kept while
/// the mesh and the picture's size stay.
static MAPS: Mutex<Option<Kept>> = Mutex::new(None);

/// The reduced map of `m` for a `w` × `h` picture (kept: see [`MAPS`]).
fn map_of(m: &Mesh, w: u32, h: u32) -> Arc<Vec<[f32; 2]>> {
    let key = m.text();
    let size = reduced(w, h);
    if let Ok(kept) = MAPS.lock()
        && let Some((k, s, map)) = kept.as_ref()
        && *k == key
        && *s == size
    {
        return Arc::clone(map);
    }
    let map = Arc::new(small_map(m, size));
    if let Ok(mut kept) = MAPS.lock() {
        *kept = Some((key, size, Arc::clone(&map)));
    }
    map
}

/// `src` drawn on the mesh as `target` says (rows up, its box the
/// picture).
fn warped(m: &Mesh, target: bool, src: &Buf) -> Buf {
    let (w, h) = (src.width().max(0) as u32, src.height().max(0) as u32);
    if w < 2 || h < 2 {
        return src.clone();
    }
    let [x1, y1, ..] = src.bounds;
    let read = |at: [f32; 2]| crate::lens_correction::read(&src.data, [w, h], at);
    if target {
        let [mw, mh] = reduced(w, h);
        let map = map_of(m, w, h);
        let ratio = [
            (mw - 1) as f32 / (w - 1) as f32,
            (mh - 1) as f32 / (h - 1) as f32,
        ];
        src.map(|x, y| read(enlarged(&map, mw, ratio, (x - x1) as u32, (y - y1) as u32)))
    } else {
        let (cols, rows) = source_steps(m, w, h);
        src.map(|x, y| read(source_at(m, &cols, &rows, (x - x1) as u32, (y - y1) as u32)))
    }
}

/// The card's table for the mesh on a `w` × `h` picture and its size
/// (`sw`, `sh`) and numbers: Target's reduced map (a value a row, rows
/// up) with the enlargement's ratios; Source's columns' and rows' cells
/// and shares, then the points.
pub(crate) fn card_table(
    points: &[[f32; 2]],
    cells: [u32; 2],
    target: bool,
    w: u32,
    h: u32,
) -> (Vec<[f32; 4]>, [u32; 2], [f32; 2]) {
    let m = Mesh {
        cols: cells[0] as usize + 1,
        rows: cells[1] as usize + 1,
        p: points.to_vec(),
    };
    if target {
        let [mw, mh] = reduced(w, h);
        let map = map_of(&m, w, h);
        let ratio = [
            (mw - 1) as f32 / (w.max(2) - 1) as f32,
            (mh - 1) as f32 / (h.max(2) - 1) as f32,
        ];
        (
            map.iter().map(|p| [p[0], p[1], 0.0, 0.0]).collect(),
            [mw, mh],
            ratio,
        )
    } else {
        let (cols, rows) = source_steps(&m, w, h);
        let mut table: Vec<[f32; 4]> = cols
            .iter()
            .chain(&rows)
            .map(|&(k, f)| [k as f32, f, 0.0, 0.0])
            .collect();
        table.extend(m.p.iter().map(|p| [p[0], p[1], 0.0, 0.0]));
        (table, [m.cols as u32, m.rows as u32], [0.0; 2])
    }
}

// The plug-in.

/// The settings at one time.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Settings {
    target: bool,
    cells: [usize; 2],
    lock: bool,
    enable: bool,
    elasticity: f32,
    fixed: bool,
    mesh: Mesh,
}

impl Settings {
    fn of(v: &Vals, text: &str) -> Settings {
        let lock = v.b(LOCK);
        let nx = v.i(GRID_X).clamp(1, MOST) as usize;
        let ny = if lock {
            nx
        } else {
            v.i(GRID_Y).clamp(1, MOST) as usize
        };
        Settings {
            target: v.i(DIRECTION) != 0,
            cells: [nx, ny],
            lock,
            enable: v.b(ENABLE),
            elasticity: v.f(ELASTICITY) as f32,
            fixed: v.b(FIXED_BORDER),
            mesh: Mesh::read(text, [nx, ny]),
        }
    }

    fn at(fx: Fx, time: f64) -> Settings {
        let v = Vals::of(SPECS, &|s| s.read(fx, time));
        Settings::of(&v, &fx.param(MESH).text_at(time))
    }

    /// The settings in an instance's values (the card's).
    pub(crate) fn of_values(values: Values<'_>) -> Settings {
        let v = Vals::of(SPECS, &|spec| {
            values
                .nums(spec.name())
                .map_or_else(|| spec.default(), <[f64]>::to_vec)
        });
        Settings::of(&v, values.text(MESH).unwrap_or(""))
    }

    /// Whether the picture stays as it is: the warp off, or every point
    /// within a thousandth of its place at rest.
    pub(crate) fn changes_nothing(&self) -> bool {
        !self.enable || self.mesh.at_rest(0.001)
    }

    /// What the card draws: nothing, or the mesh for `card_table`.
    pub(crate) fn plan(&self) -> Plan {
        if self.changes_nothing() {
            return Plan::Same;
        }
        Plan::MeshWarp {
            points: Arc::new(self.mesh.p.clone()),
            cells: [self.mesh.cols as u32 - 1, self.mesh.rows as u32 - 1],
            target: self.target,
        }
    }
}

/// Whether the mesh has keyframes.
fn animated(fx: Fx) -> bool {
    !fx.param(MESH).key_times().is_empty()
}

/// VEGAS's grid of `cells` (Lock: as many up as across), the mesh made
/// the grid at rest when its size changes — or always, with `reset` —;
/// an animated mesh keeps its size, the grid's sizes going back to it.
fn rebuild(fx: Fx, time: f64, reset: bool) {
    let s = Settings::at(fx, time);
    let cells = [s.mesh.cols - 1, s.mesh.rows - 1];
    let keyed = animated(fx);
    if keyed {
        fx.param(GRID_X).set_int(cells[0] as c_int);
        fx.param(GRID_Y).set_int(cells[1] as c_int);
    }
    let nx = if keyed { cells[0] } else { s.cells[0] };
    let ny = if s.lock {
        nx
    } else if keyed {
        cells[1]
    } else {
        s.cells[1]
    };
    if reset || [nx, ny] != cells {
        if keyed && [nx, ny] == cells {
            let p = fx.param(MESH);
            p.set_text_at(time, &Mesh::rest(nx + 1, ny + 1).text());
        } else {
            fx.param(MESH).delete_keys();
            fx.param(MESH).set_text("");
        }
    }
    if s.lock {
        fx.param(GRID_Y).set_int(nx as c_int);
    }
}

/// VEGAS's say: the grid's sizes and Lock change only while the mesh is at
/// rest and not animated, Grid size in Y not while locked; locking sizes
/// that differ once the mesh moved is undone.
fn show(fx: Fx, time: f64) -> bool {
    let s = Settings::at(fx, time);
    let free = s.mesh.at_rest(0.01) && !animated(fx);
    fx.param(GRID_X).set_enabled(free);
    fx.param(GRID_Y).set_enabled(free && !s.lock);
    fx.param(LOCK).set_enabled(free);
    free
}

fn changed(fx: Fx, time: f64, name: &str) {
    let is = |n: &CStr| n.to_str() == Ok(name);
    if is(GRID_X) || is(GRID_Y) {
        rebuild(fx, time, false);
    } else if is(MESH) || is(LOCK) {
        let s = Settings::at(fx, time);
        if is(MESH) {
            // A mesh of another size (a preset's, an undo) sets the grid's.
            let cells = [s.mesh.cols - 1, s.mesh.rows - 1];
            if cells != s.cells {
                fx.param(GRID_X).set_int(cells[0] as c_int);
                fx.param(GRID_Y).set_int(cells[1] as c_int);
            }
        }
        let free = show(fx, time);
        if is(LOCK)
            && !free
            && s.lock
            && fx.param(GRID_X).int_at(time) != fx.param(GRID_Y).int_at(time)
        {
            fx.param(LOCK).set_int(0);
            return;
        }
        if s.lock {
            rebuild(fx, time, false);
        }
    } else if is(RESET) {
        rebuild(fx, time, true);
    }
    show(fx, time);
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
        if Settings::at(fx, args.dbl(kOfxPropTime, 0)).changes_nothing() {
            out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        // Any pixel may come from anywhere in the picture.
        let time = args.dbl(kOfxPropTime, 0);
        let roi: [f64; 4] = args.dbls(kOfxImageEffectPropRegionOfInterest);
        let roi = fx
            .clip(kOfxImageEffectSimpleSourceClipName)
            .rod(time)
            .unwrap_or(roi);
        out.set_dbls(c"OfxImageClipPropRoI_Source", &roi);
        kOfxStatOK
    } else if is(action, kOfxActionInstanceChanged) {
        let time = args.dbl(kOfxPropTime, 0);
        if args.is(kOfxPropType, kOfxTypeParameter) {
            changed(fx, time, &args.text(kOfxPropName, 0));
        }
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

fn describe(fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, c"Mesh Warp");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Creative");
    p.set_str(kOfxPropPluginDescription, c"Distorts images");
    p.set_strs(
        kOfxImageEffectPropSupportedContexts,
        &[kOfxImageEffectContextFilter, kOfxImageEffectContextGeneral],
    );
    p.set_strs(
        kOfxImageEffectPropSupportedPixelDepths,
        &[kOfxBitDepthFloat, kOfxBitDepthShort, kOfxBitDepthByte],
    );
    // The map spans the whole picture.
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

fn describe_in_context(fx: Fx) {
    let comps = [kOfxImageComponentRGBA, kOfxImageComponentRGB];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    fx.define_clip(kOfxImageEffectOutputClipName, &[kOfxImageComponentRGBA]);
    let p = fx.define_param(kOfxParamTypeCustom, MESH, c"Mesh");
    p.set_str(kOfxParamPropDefault, c"");
    p.set_int(kOfxParamPropAnimates, 1);
    p.set_str(
        kOfxParamPropHint,
        c"The grid's points, dragged on the preview.",
    );
    p.set_ptr(
        kOfxParamPropCustomInterpCallbackV1,
        interpolate as OfxCustomParamInterpFuncV1 as *mut c_void,
    );
    let mut group: Option<&CStr> = None;
    for &spec in SPECS {
        match spec {
            Spec::Group(name, label) => {
                let g = fx.define_param(kOfxParamTypeGroup, name, label);
                g.set_int(kOfxParamPropGroupOpen, 1);
                group = Some(name);
                continue;
            }
            Spec::GroupEnd => {
                group = None;
                continue;
            }
            _ => {}
        }
        let Some(p) = define(fx, spec) else { continue };
        if let Some(g) = group {
            p.set_str(kOfxParamPropParent, g);
        }
        let name = spec.name();
        if name == DIRECTION || name == ENABLE {
            p.set_int(kOfxParamPropSecret, 1);
        }
        if name != ENABLE {
            p.set_int(kOfxParamPropAnimates, 0);
        }
        if name == GRID_X || name == GRID_Y {
            p.set_int(kOfxParamPropDisplayMin, 1);
            p.set_int(kOfxParamPropDisplayMax, 30);
        }
        if name == ELASTICITY {
            p.set_dbl(kOfxParamPropIncrement, 0.1);
            p.set_int(kOfxParamPropDigits, 2);
        }
    }
    let p = fx.define_param(kOfxParamTypePushButton, RESET, c"Reset Mesh");
    p.set_str(kOfxParamPropHint, c"Resets the mesh.");
}

/// The mesh between two keyframes (OpenFX's custom parameters'
/// interpolation): see [`between`].
unsafe extern "C" fn interpolate(
    _set: OfxParamSetHandle,
    in_args: OfxPropertySetHandle,
    out_args: OfxPropertySetHandle,
) -> OfxStatus {
    let (args, out) = (Props(in_args), Props(out_args));
    let a = args.text(kOfxParamPropCustomValue, 0);
    let b = args.text(kOfxParamPropCustomValue, 1);
    let t = args.dbl(kOfxParamPropInterpolationAmount, 0);
    match CString::new(between(&a, &b, t)) {
        Ok(text) => {
            out.set_str(kOfxParamPropCustomValue, &text);
            kOfxStatOK
        }
        Err(_) => kOfxStatFailed,
    }
}

fn render(fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let Some(src) = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time) else {
        return kOfxStatFailed;
    };
    if src.depth != out.depth {
        return kOfxStatErrImageFormat;
    }
    let s = Settings::at(fx, time);
    let source = Buf::read(Some(&src));
    drop(src);
    let result = if s.changes_nothing() {
        source
    } else {
        warped(&s.mesh, s.target, &source)
    };
    crate::compositors::write(&out, &result, (x1, y1, x2, y2));
    drop(out);
    kOfxStatOK
}

// The overlay.

/// A point of a ring around a dragged point: a neighbour of the mesh, or
/// one past the picture's edge where it has none.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Around {
    Point(usize),
    Past([f32; 2]),
}

/// The ring of point (c, r)'s eight neighbours, round from its lower left
/// and closed — where one is missing, a point 0.2 past the picture's edge
/// on its neighbour's line —; and the neighbours with the point, in order.
fn ring(m: &Mesh, c: usize, r: usize) -> (Vec<Around>, Vec<usize>) {
    const LOW: f32 = -0.2;
    const HIGH: f32 = 1.2;
    let (cols, rows) = (m.cols as isize, m.rows as isize);
    let (c, r) = (c as isize, r as isize);
    let real = |c: isize, r: isize| (0..cols).contains(&c) && (0..rows).contains(&r);
    let idx = |c: isize, r: isize| (r * cols + c) as usize;
    let at = |c: isize, r: isize| m.p[idx(c, r)];
    let me = at(c, r);
    // Each neighbour in turn: itself, or past the edge — a coordinate of
    // the point beside it (`along`) and the edge's.
    let around = [
        (-1, -1),
        (0, -1),
        (1, -1),
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
    ];
    let mut ring = Vec::with_capacity(9);
    let mut near = Vec::with_capacity(9);
    for (dc, dr) in around {
        let (nc, nr) = (c + dc, r + dr);
        if real(nc, nr) {
            ring.push(Around::Point(idx(nc, nr)));
            near.push(idx(nc, nr));
            continue;
        }
        let x = match dc {
            -1 if nc < 0 => LOW,
            1 if nc >= cols => HIGH,
            _ => {
                // Across inside: the column's point on this row, or the
                // point's own.
                if real(nc, r) { at(nc, r)[0] } else { me[0] }
            }
        };
        let y = match dr {
            -1 if nr < 0 => LOW,
            1 if nr >= rows => HIGH,
            _ => {
                if real(c, nr) {
                    at(c, nr)[1]
                } else {
                    me[1]
                }
            }
        };
        ring.push(Around::Past([x, y]));
    }
    ring.push(ring[0]);
    near.push(idx(c, r));
    near.sort_unstable();
    (ring, near)
}

/// Whether `p` lies inside the closed outline `poly` (crossings counted).
fn inside(poly: &[[f32; 2]], p: [f32; 2]) -> bool {
    let mut odd = false;
    for e in poly.windows(2) {
        let (a, b) = (e[0], e[1]);
        let crosses = (a[1] <= p[1] && p[1] < b[1]) || (p[1] < a[1] && b[1] <= p[1]);
        if crosses && p[0] < (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) + a[0] {
            odd = !odd;
        }
    }
    odd
}

/// The ring's points as places.
fn outline(ring: &[Around], m: &Mesh) -> Vec<[f32; 2]> {
    ring.iter()
        .map(|a| match *a {
            Around::Point(n) => m.p[n],
            Around::Past(p) => p,
        })
        .collect()
}

/// How much each point follows a drag of point `a`: a Gaussian of its
/// distance (a fifth of Elasticity its spread), the border's none with
/// Fixed Border Points; nothing below an Elasticity of 0.001 (only the
/// point moves).
fn pull(m: &Mesh, a: usize, elasticity: f32, fixed: bool) -> Vec<f32> {
    if elasticity < 0.001 {
        return Vec::new();
    }
    let spread = elasticity * 0.2;
    let [ax, ay] = m.p[a];
    (0..m.p.len())
        .map(|k| {
            if fixed && m.on_border(k) {
                return 0.0;
            }
            let (dx, dy) = (m.p[k][0] - ax, m.p[k][1] - ay);
            (-((dy * dy + dx * dx) / spread)).exp()
        })
        .collect()
}

/// The point nearest the pen at `pen` (parts of the frame), within 0.06
/// of the frame either way.
fn hit(m: &Mesh, pen: [f32; 2]) -> Option<usize> {
    let mut best = (0.0036f32, None);
    for (k, p) in m.p.iter().enumerate() {
        let (dx, dy) = (pen[0] - p[0], pen[1] - p[1]);
        let d = dy * dy + dx * dx;
        if d < best.0 {
            best = (d, Some(k));
        }
    }
    best.1
}

/// A drag: the point, its ring, its neighbours, how much each point
/// follows it and the spread that pull was worked out with.
#[derive(Clone, Debug)]
struct Drag {
    point: usize,
    ring: Vec<Around>,
    near: Vec<usize>,
    pull: Vec<f32>,
    spread: f32,
}

/// What the overlay keeps between the pen's actions.
#[derive(Debug, Default)]
struct Overlay {
    drag: Option<Drag>,
    clicks: Clicks,
}

/// `m` with the drag's point taken toward `to` (parts of the frame): there
/// when it stays inside its ring, else a tenth of the way, else nowhere;
/// the others following by their pull — with Shift only its neighbours,
/// each inside its own ring, or none —, the ring's points past the edge
/// too.
fn dragged(m: &Mesh, drag: &mut Drag, to: [f32; 2], shift: bool) -> Option<Mesh> {
    let a = drag.point;
    let old = m.p[a];
    let poly = outline(&drag.ring, m);
    let to = if inside(&poly, to) {
        to
    } else {
        let tenth = [
            old[0] - (old[0] - to[0]) * 0.1,
            old[1] - (old[1] - to[1]) * 0.1,
        ];
        if !inside(&poly, tenth) {
            return None;
        }
        tenth
    };
    let delta = [old[0] - to[0], old[1] - to[1]];
    let mut out = m.clone();
    if drag.pull.is_empty() {
        out.p[a] = to;
        return Some(out);
    }
    if shift {
        for &k in &drag.near {
            let (r, _) = ring(m, k % m.cols, k / m.cols);
            let w = drag.pull[k];
            let p = [m.p[k][0] - w * delta[0], m.p[k][1] - w * delta[1]];
            if !inside(&outline(&r, m), p) {
                return None;
            }
        }
        for &k in &drag.near {
            let w = drag.pull[k];
            out.p[k] = [m.p[k][0] - delta[0] * w, m.p[k][1] - delta[1] * w];
        }
    } else {
        for (k, p) in out.p.iter_mut().enumerate() {
            let w = drag.pull[k];
            *p = [p[0] - delta[0] * w, p[1] - delta[1] * w];
        }
    }
    // The ring's points past the edge follow as the mesh does.
    let now = out.p[a];
    for q in drag.ring.iter_mut() {
        if let Around::Past(p) = q {
            let (dx, dy) = (p[0] - now[0], p[1] - now[1]);
            let w = (-((dy * dy + dx * dx) / drag.spread)).exp();
            *p = [p[0] - delta[0] * w, p[1] - delta[1] * w];
        }
    }
    Some(out)
}

/// The point at rest for a double click: back where it was when that stays
/// inside its ring (or on the right or top edge), unless the cells then
/// overlap.
fn put_back(m: &Mesh, n: usize) -> Option<Mesh> {
    let (c, r) = (n % m.cols, n / m.cols);
    let rest = [
        c as f32 / (m.cols - 1) as f32,
        r as f32 / (m.rows - 1) as f32,
    ];
    let (ring, _) = ring(m, c, r);
    if !(inside(&outline(&ring, m), rest) || rest[0] == 1.0 || rest[1] == 1.0) {
        return None;
    }
    let mut out = m.clone();
    out.p[n] = rest;
    triangles(&out).2.then_some(out)
}

/// The frame on the preview: where it starts and its size (canonical
/// units).
#[derive(Clone, Copy, Debug)]
struct Frame {
    origin: [f64; 2],
    size: [f64; 2],
}

impl Frame {
    fn canonical(&self, p: [f32; 2]) -> [f64; 2] {
        [
            self.origin[0] + f64::from(p[0]) * self.size[0],
            self.origin[1] + f64::from(p[1]) * self.size[1],
        ]
    }

    fn parts(&self, c: [f64; 2]) -> [f32; 2] {
        [
            ((c[0] - self.origin[0]) / self.size[0]) as f32,
            ((c[1] - self.origin[1]) / self.size[1]) as f32,
        ]
    }
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
    let time = args.dbl(kOfxPropTime, 0);
    let ps: [f64; 2] = args.dbls(kOfxInteractPropPixelScale);
    let ps = ps.map(|v| if v.abs() > 1e-12 { v.abs() } else { 1.0 });
    let (origin, size) = fx.frame();
    let f = Frame { origin, size };
    let s = Settings::at(fx, time);
    if is(action, kOfxInteractActionDraw) {
        draw(args, state, &s.mesh, &f, ps);
        return kOfxStatOK;
    }
    let pen = f.parts(args.dbls(kOfxInteractPropPenPosition));
    if is(action, kOfxInteractActionPenDown) {
        let Some(n) = hit(&s.mesh, pen) else {
            return kOfxStatReplyDefault;
        };
        let screen = [
            f64::from(pen[0]) * size[0] / ps[0],
            f64::from(pen[1]) * size[1] / ps[1],
        ];
        if state.clicks.press(screen, Instant::now()) {
            // A double click puts the point back where it was.
            if let Some(m) = put_back(&s.mesh, n) {
                write(fx, time, &m);
            }
            state.drag = None;
        } else if s.fixed && s.mesh.on_border(n) {
            state.drag = None;
        } else {
            let (r, near) = ring(&s.mesh, n % s.mesh.cols, n / s.mesh.cols);
            state.drag = Some(Drag {
                point: n,
                ring: r,
                near,
                pull: pull(&s.mesh, n, s.elasticity, s.fixed),
                spread: s.elasticity * 0.2,
            });
        }
        redraw(handle);
        kOfxStatOK
    } else if is(action, kOfxInteractActionPenMotion) {
        let Some(drag) = state.drag.as_mut() else {
            return kOfxStatReplyDefault;
        };
        let shift = args.int(bv_ofx_sys::extensions::kBananaVInteractPropModifiers, 0) & 1 != 0;
        if let Some(m) = dragged(&s.mesh, drag, pen, shift) {
            write(fx, time, &m);
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

/// Gives the mesh `m` (at `time` when it is animated).
fn write(fx: Fx, time: f64, m: &Mesh) {
    let p = fx.param(MESH);
    if p.key_times().is_empty() {
        p.set_text(&m.text());
    } else {
        p.set_text_at(time, &m.text());
    }
}

/// The grid's lines and points.
const LINE: [f32; 4] = [64.0 / 255.0, 64.0 / 255.0, 64.0 / 255.0, 1.0];
const POINT: [f32; 4] = [1.0, 1.0, 0.0, 1.0];
const DRAGGED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

/// VEGAS's grid: its rows and columns as dark gray lines, its points as
/// yellow dots six pixels across — the dragged one red.
fn draw(args: Props, state: &Overlay, m: &Mesh, f: &Frame, ps: [f64; 2]) {
    let Some(d) = Draw::of(args) else { return };
    let at = |c: usize, r: usize| f.canonical(m.p[m.index(c, r)]);
    let mut lines = Vec::new();
    for r in 0..m.rows {
        for c in 1..m.cols {
            lines.push(at(c - 1, r));
            lines.push(at(c, r));
        }
    }
    for c in 0..m.cols {
        for r in 1..m.rows {
            lines.push(at(c, r - 1));
            lines.push(at(c, r));
        }
    }
    d.colour(LINE);
    d.width(1.0);
    d.lines(&lines);
    let dragged = state.drag.as_ref().map(|g| g.point);
    for (n, p) in m.p.iter().enumerate() {
        let c = f.canonical(*p);
        let dot: Vec<[f64; 2]> = (0..12)
            .map(|i| {
                let t = std::f64::consts::TAU * f64::from(i) / 12.0;
                [c[0] + 3.0 * ps[0] * t.cos(), c[1] + 3.0 * ps[1] * t.sin()]
            })
            .collect();
        d.colour(if dragged == Some(n) { DRAGGED } else { POINT });
        d.fill(&dot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mesh(text: &str) -> Mesh {
        Mesh::parse(text).unwrap()
    }

    #[test]
    fn a_mesh_keeps_as_text() {
        let m = Mesh::rest(3, 2);
        assert_eq!(m.text(), "2 1 0 0 0.5 0 1 0 0 1 0.5 1 1 1");
        assert_eq!(Mesh::parse(&m.text()), Some(m.clone()));
        assert_eq!(Mesh::read("", [2, 1]), m);
        assert!(Mesh::parse("2 1 0 0").is_none());
        assert!(m.at_rest(0.001));
        let mut moved = m.clone();
        moved.p[4][0] += 0.002;
        assert!(!moved.at_rest(0.001) && moved.at_rest(0.01));
    }

    #[test]
    fn keyframes_move_the_points_between_or_change_half_way() {
        let a = "1 1 0 0 1 0 0 1 1 1";
        let b = "1 1 0 0 0.5 0 0 1 0.5 1";
        assert_eq!(between(a, b, 0.5), "1 1 0 0 0.75 0 0 1 0.75 1");
        assert_eq!(between("", b, 0.5), "1 1 0 0 0.75 0 0 1 0.75 1");
        let c = "2 1 0 0 0.5 0 1 0 0 1 0.5 1 1 1";
        assert_eq!(between(a, c, 0.4), a);
        assert_eq!(between(a, c, 0.5), c);
    }

    #[test]
    fn a_cell_splits_along_its_shorter_diagonal() {
        // The lower right corner pulled in: 1–3 is the shorter one.
        let m = mesh("1 1 0 0 0.6 0.4 0 1 1 1");
        let (t, s, valid) = triangles(&m);
        assert!(valid);
        assert_eq!(t.len(), 2);
        assert!(t[0].contains(&[0.6, 0.4]) && t[0].contains(&[0.0, 1.0]));
        assert!(s[0].contains(&[1.0, 0.0]));
        // Pulled out instead: along 0–2.
        let m = mesh("1 1 0 0 1.4 -0.4 0 1 1 1");
        let (t, ..) = triangles(&m);
        assert!(t[0].contains(&[0.0, 0.0]) && t[0].contains(&[1.0, 1.0]));
    }

    #[test]
    fn triangles_overlap_unless_a_side_parts_them() {
        let a = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        let b = [[1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        assert!(!overlap(&a, &b));
        let c = [[0.2, 0.2], [1.0, 0.2], [0.2, 1.0]];
        assert!(overlap(&a, &c));
    }

    #[test]
    fn the_map_at_rest_reads_each_place_and_moves_with_the_mesh() {
        let m = Mesh::rest(3, 3);
        let map = small_map(&m, [5, 5]);
        for (k, p) in map.iter().enumerate() {
            let want = [(k % 5) as f32 / 4.0, (k / 5) as f32 / 4.0];
            assert!(
                (p[0] - want[0]).abs() < 1e-6 && (p[1] - want[1]).abs() < 1e-6,
                "{k}: {p:?}"
            );
        }
        // The middle point moved right: the frame's middle, two thirds of
        // the way to it along the middle row, reads two thirds of the way
        // to the picture's middle.
        let mut m = Mesh::rest(3, 3);
        m.p[4][0] = 0.75;
        let map = small_map(&m, [5, 5]);
        assert!((map[12][0] - 1.0 / 3.0).abs() < 1e-6, "{:?}", map[12]);
        // Nothing past where the mesh reaches.
        let shrunk = mesh("1 1 0.25 0.25 0.75 0.25 0.25 0.75 0.75 0.75");
        let map = small_map(&shrunk, [5, 5]);
        assert_eq!(map[0], [-1.0, -1.0]);
        assert!((map[12][0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn the_reduced_map_halves_to_under_480_pixels() {
        assert_eq!(reduced(1920, 1080), [480, 270]);
        assert_eq!(reduced(640, 480), [320, 240]);
        assert_eq!(reduced(320, 180), [320, 180]);
    }

    #[test]
    fn source_cells_cover_the_picture_and_add_their_shares() {
        let m = Mesh::rest(3, 2);
        let (cols, rows) = source_steps(&m, 10, 4);
        assert_eq!(cols[0], (1, 0.0));
        assert_eq!(cols[4], (1, 0.8));
        assert_eq!(cols[5], (2, 0.0));
        assert_eq!(rows[3].0, 1);
        let p = source_at(&m, &cols, &rows, 5, 2);
        assert!(
            (p[0] - 0.5).abs() < 1e-6 && (p[1] - 0.5).abs() < 1e-6,
            "{p:?}"
        );
    }

    #[test]
    fn a_dragged_point_stays_inside_its_ring() {
        let m = Mesh::rest(3, 3);
        let (r, near) = ring(&m, 1, 1);
        assert_eq!(near, (0..9).collect::<Vec<_>>());
        let poly = outline(&r, &m);
        assert!(inside(&poly, [0.6, 0.4]));
        assert!(!inside(&poly, [1.1, 0.5]));
        let mut drag = Drag {
            point: 4,
            ring: r,
            near,
            pull: Vec::new(),
            spread: 0.0,
        };
        let moved = dragged(&m, &mut drag, [0.6, 0.4], false).unwrap();
        assert_eq!(moved.p[4], [0.6, 0.4]);
        assert_eq!(moved.p[0], m.p[0]);
        // Past the ring: a tenth of the way.
        let moved = dragged(&m, &mut drag, [1.5, 0.5], false).unwrap();
        assert!((moved.p[4][0] - 0.6).abs() < 1e-6);
        // A corner's ring reaches past the picture's edge.
        let (r, _) = ring(&m, 0, 0);
        assert!(inside(&outline(&r, &m), [-0.1, -0.1]));
    }

    #[test]
    fn elasticity_pulls_the_others_by_a_gaussian() {
        let m = Mesh::rest(3, 3);
        let w = pull(&m, 4, 1.0, false);
        assert_eq!(w[4], 1.0);
        assert!((w[1] - (-0.25f32 / 0.2).exp()).abs() < 1e-6);
        assert!(pull(&m, 4, 0.0, false).is_empty());
        let fixed = pull(&m, 4, 1.0, true);
        assert_eq!(fixed[0], 0.0);
        assert_eq!(fixed[4], 1.0);
    }

    #[test]
    fn the_pen_takes_the_nearest_point_within_reach() {
        let m = Mesh::rest(3, 3);
        assert_eq!(hit(&m, [0.52, 0.49]), Some(4));
        assert_eq!(hit(&m, [0.25, 0.25]), None);
    }

    #[test]
    fn a_double_click_puts_a_point_back() {
        let mut m = Mesh::rest(3, 3);
        m.p[4] = [0.6, 0.6];
        let back = put_back(&m, 4).unwrap();
        assert_eq!(back.p[4], [0.5, 0.5]);
    }

    #[test]
    fn nothing_changes_at_rest_or_with_the_warp_off() {
        let v = Vals::of(SPECS, &|s| s.default());
        let s = Settings::of(&v, "");
        assert!(s.changes_nothing());
        assert_eq!(s.cells, [8, 8]);
        let s = Settings::of(&v, "1 1 0 0 0.5 0 0 1 0.5 1");
        assert!(!s.changes_nothing());
        let src = Buf::solid([0, 0, 8, 4], [0.5, 0.5, 0.5, 1.0]);
        let out = warped(&s.mesh, true, &src);
        // The right half reads past the picture: nothing there.
        assert_eq!(out.at(7, 2), [0.0; 4]);
        assert!(out.at(1, 2)[3] > 0.99);
    }
}
