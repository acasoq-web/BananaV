//! VEGAS's Checkerboard generator, worked out as VEGAS's 8-bit pictures
//! have it: tiles Width and Height of the frame (square on screen with
//! Square, at least a pixel), each edge blended over its Edge blending part
//! of the tile on either side — half and half on the edge, half a pixel at
//! least —, the grid moved by its offset; single precision floats step by
//! step, the blends in 65536ths, the bytes cut.
//!
//! Every row crosses the same columns: where a tile starts and how far a
//! pixel is into its blend are the columns' ([`Board::cols`]), which tile
//! a row is in and its colors (blended toward the next row of tiles at
//! their top and bottom) the rows' ([`Board::rows`]). The card draws from
//! the same two tables, in whole numbers.

/// The generator's settings at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    /// Width and Height: parts of the frame's width and height.
    pub tile: [f64; 2],
    pub square: bool,
    /// Edge blending's Horizontal and Vertical: parts of a tile.
    pub blend: [f64; 2],
    /// The vertical blending is the horizontal one.
    pub proportional: bool,
    /// Color 1 and Color 2, straight, with their alphas.
    pub colors: [[f64; 4]; 2],
    /// Grid offset's X and Y: parts of the frame.
    pub offset: [f64; 2],
}

/// The tiles worked out for a picture.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Board {
    /// Each column's tile, counted from the one at the left edge, and how
    /// much of the tile before it shows there (65536ths; 0: none).
    pub cols: Vec<(u32, u32)>,
    /// Each row's (from the top): whether the tile at the left edge is an
    /// odd one, and the colors of the even and the odd tiles there.
    pub rows: Vec<Row>,
    /// Bytes (an 8-bit project's) or floats.
    pub bytes: bool,
}

/// A row of tiles' colors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Row {
    pub odd: bool,
    /// Premultiplied: bytes (0–255) in an 8-bit project, else 0–1.
    pub even: [f32; 4],
    pub other: [f32; 4],
}

/// The whole number at or below `a / b`, as VEGAS's squares fall: cut
/// toward zero, one less when that is past `a`.
fn below(a: f32, b: f32) -> i64 {
    let t = (a / b).trunc() as i64;
    if t as f32 * b <= a { t } else { t - 1 }
}

/// A color premultiplied: bytes cut as VEGAS's are (the products in
/// single precision), or floats.
fn premultiplied(c: [f64; 4], bytes: bool) -> [f32; 4] {
    let a = c[3];
    if bytes {
        let byte = |v: f64| ((v as f32) * 255.0).trunc().clamp(0.0, 255.0);
        [byte(a * c[0]), byte(a * c[1]), byte(a * c[2]), byte(a)]
    } else {
        [
            (a * c[0]) as f32,
            (a * c[1]) as f32,
            (a * c[2]) as f32,
            a as f32,
        ]
    }
}

/// `a` and `b` mixed, `w` 65536ths of `b`: bytes as whole numbers, the
/// result cut; floats as they are.
fn mixed(a: [f32; 4], b: [f32; 4], w: u32, bytes: bool) -> [f32; 4] {
    let w = w.min(65536);
    if bytes {
        std::array::from_fn(|k| {
            let s = a[k] as u64 * u64::from(65536 - w) + b[k] as u64 * u64::from(w);
            (s >> 16) as f32
        })
    } else {
        let t = w as f32 / 65536.0;
        std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t)
    }
}

impl Board {
    /// The tiles of a picture `width` × `height` pixels (its pixels `par`
    /// times as wide as tall), in bytes for an 8-bit project.
    pub(crate) fn new(s: &Settings, width: usize, height: usize, par: f64, bytes: bool) -> Board {
        let (wf, hf) = (width.max(1) as f32, height.max(1) as f32);
        let par = if par > 0.0 { par as f32 } else { 1.0 };
        // A tile's size in pixels: square on screen with Square, a pixel
        // at least each way (the other side grown alike).
        let mut cw = s.tile[0] as f32 * wf;
        let mut ch = s.tile[1] as f32 * hf;
        let inv = 1.0 / par;
        if s.square {
            ch = cw * par;
        }
        if inv > cw {
            ch = ch * inv / cw;
            cw = inv;
        }
        if 1.0 > ch {
            cw /= ch;
            ch = 1.0;
        }
        // The blends' half widths (half a pixel at least) and their slopes
        // in 65536ths.
        let vertical = if s.proportional {
            s.blend[0]
        } else {
            s.blend[1]
        };
        let ex = (s.blend[0] as f32 * cw * 0.5).max(0.5);
        let ey = (vertical as f32 * ch * 0.5).max(0.5);
        let (kx, ky) = (32768.0 / ex, 32768.0 / ey);
        let down = s.offset[1] as f32 * hf;
        let x0 = 0.5 - s.offset[0] as f32 * wf;

        // The columns: from the tile at the left edge, a pixel at a time,
        // its blend toward the tile before it, then its solid part; each
        // next tile's edges a tile's width further (in floats, added).
        let first = below(x0, cw);
        let mut cols = Vec::with_capacity(width);
        let (mut solid, mut end) = (first as f32 * cw + ex, (first + 1) as f32 * cw - ex);
        let (mut x, mut k) = (x0, 0u32);
        while cols.len() < width {
            while solid > x && cols.len() < width {
                let d = solid - x;
                x += 1.0;
                cols.push((k, (d * kx + 0.5) as u32));
            }
            while end > x && cols.len() < width {
                cols.push((k, 0));
                x += 1.0;
            }
            k += 1;
            solid += cw;
            end += cw;
        }

        // The rows: in which tile each is, its colors blended toward the
        // tiles above or below near their edges.
        let (c1, c2) = (
            premultiplied(s.colors[0], bytes),
            premultiplied(s.colors[1], bytes),
        );
        let rows = (0..height)
            .map(|t| {
                let y = (t as f32 + 0.5) - down;
                let j = below(y, ch);
                let top = j as f32 * ch + ey;
                let bottom = (j + 1) as f32 * ch - ey;
                let (even, other) = if y < top || bottom <= y {
                    let d = if top > y { top - y } else { y - bottom };
                    let w = (d * ky + 0.5) as u32;
                    (mixed(c1, c2, w, bytes), mixed(c2, c1, w, bytes))
                } else {
                    (c1, c2)
                };
                Row {
                    odd: (first + j).rem_euclid(2) == 1,
                    even,
                    other,
                }
            })
            .collect();
        Board { cols, rows, bytes }
    }

    /// The pixel at column `x`, row `y` from the top: premultiplied, 0–1.
    pub(crate) fn at(&self, x: usize, y: usize) -> [f32; 4] {
        let (Some(&(k, w)), Some(r)) = (self.cols.get(x), self.rows.get(y)) else {
            return [0.0; 4];
        };
        let odd = r.odd ^ (k % 2 == 1);
        let (here, before) = if odd {
            (r.other, r.even)
        } else {
            (r.even, r.other)
        };
        let c = if w > 0 {
            mixed(here, before, w, self.bytes)
        } else {
            here
        };
        if self.bytes { c.map(|v| v / 255.0) } else { c }
    }

    /// The tables as the card reads them: a row of four numbers for each
    /// column — its tile's parity and its blend, as whole numbers in the
    /// floats' bits —, then three for each row: its even tiles' color, its
    /// odd tiles' (bytes as whole numbers' bits, or floats) and whether the
    /// first tile is odd.
    pub(crate) fn table(&self) -> Vec<[f32; 4]> {
        let bits = |v: u32| f32::from_bits(v);
        let color = |c: [f32; 4]| {
            if self.bytes {
                c.map(|v| bits(v as u32))
            } else {
                c
            }
        };
        let mut t = Vec::with_capacity(self.cols.len() + 3 * self.rows.len());
        t.extend(
            self.cols
                .iter()
                .map(|&(k, w)| [bits(k & 1), bits(w), 0.0, 0.0]),
        );
        for r in &self.rows {
            t.push(color(r.even));
            t.push(color(r.other));
            t.push([bits(u32::from(r.odd)), 0.0, 0.0, 0.0]);
        }
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            tile: [0.1, 0.1],
            square: true,
            blend: [0.0, 0.0],
            proportional: true,
            colors: [[0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]],
            offset: [0.0, 0.0],
        }
    }

    #[test]
    fn tiles_alternate_and_blend_half_and_half_on_their_edges() {
        // 640 wide: tiles 64 pixels, black first.
        let b = Board::new(&settings(), 640, 360, 1.0, true);
        assert_eq!(b.at(0, 0), [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(b.at(63, 0), [0.0, 0.0, 0.0, 1.0]);
        // Half a pixel's blend at least: the edge's pixel is the next tile's.
        assert_eq!(b.at(64, 0), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(b.at(64, 64), [0.0, 0.0, 0.0, 1.0]);
        // A blend across the whole tile: half and half on the edge.
        let s = Settings {
            blend: [1.0, 1.0],
            ..settings()
        };
        let b = Board::new(&s, 640, 360, 1.0, true);
        let edge = b.at(64, 32);
        assert!((edge[0] - 0.5).abs() < 0.01, "{edge:?}");
        // The grid moved right by a tenth of a tile (6.4 pixels): the edge
        // at 70.4, the pixel across it blended.
        let s = Settings {
            offset: [0.01, 0.0],
            ..settings()
        };
        let b = Board::new(&s, 640, 360, 1.0, true);
        assert_eq!(b.at(0, 0), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(b.at(69, 0), [0.0, 0.0, 0.0, 1.0]);
        assert!((b.at(70, 0)[0] - 0.6).abs() < 0.01, "{:?}", b.at(70, 0));
        assert_eq!(b.at(71, 0), [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn colors_are_premultiplied_bytes_cut() {
        let s = Settings {
            colors: [[0.9, 0.3, 0.1, 0.6], [1.0, 1.0, 1.0, 1.0]],
            ..settings()
        };
        let b = Board::new(&s, 640, 360, 1.0, true);
        let c = b.at(0, 0).map(|v| (v * 255.0).round() as u8);
        assert_eq!(c, [137, 45, 15, 153]);
    }

    #[test]
    fn the_cards_table_holds_every_pixel() {
        let s = Settings {
            blend: [0.4, 0.4],
            colors: [[1.0, 0.0, 0.0, 1.0], [0.45098, 0.25098, 0.0, 1.0]],
            ..settings()
        };
        let b = Board::new(&s, 160, 90, 1.0, true);
        let t = b.table();
        let (w, h) = (160, 90);
        for y in 0..h {
            for x in 0..w {
                let col = t[x];
                let row = &t[w + 3 * y..w + 3 * y + 3];
                let k = col[0].to_bits();
                let wgt = col[1].to_bits();
                let odd = (row[2][0].to_bits() ^ k) & 1 == 1;
                let (here, before) = if odd {
                    (row[1], row[0])
                } else {
                    (row[0], row[1])
                };
                let px: [f32; 4] = std::array::from_fn(|c| {
                    let (a, z) = (here[c].to_bits() as u64, before[c].to_bits() as u64);
                    ((a * (65536 - u64::from(wgt)) + z * u64::from(wgt)) >> 16) as f32 / 255.0
                });
                assert_eq!(px, b.at(x, y), "{x} {y}");
            }
        }
    }
}
