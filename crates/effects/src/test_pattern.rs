//! VEGAS's Test Pattern as its 8-bit pictures have it: each of its nine
//! patterns in whole bytes, laid out by whole-number parts of the frame
//! (a seventh, a sixth, a third, a half, a twenty-first of its width and a
//! quarter and a third of its height, cut) or by single precision ones (a
//! tenth and two, three, four, five and seven tenths), its rows counted
//! from the bottom.
//!
//! Every pattern is the same down each of up to three bands of rows (the
//! SMPTE bars' bottom row, their middle row and the bars), but for what a
//! row brings — a gradient's blue, the porches' heights, the border —:
//! [`Pattern::cols`] holds each column's colors in the three bands and the
//! channels the row brings, [`Pattern::rows`] each row's band and color.
//! The card draws from the same tables.

/// A column's colors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Col {
    /// In the bottom band, the middle one and the top one.
    pub bands: [[u8; 3]; 3],
    /// The channels (red, green, blue) the row's color brings.
    pub from_row: [bool; 3],
}

/// A row's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Row {
    /// Its band: 0 the bottom one, 1 the middle one, 2 the top one.
    pub band: u8,
    /// All of it the border's color (`color`).
    pub border: bool,
    pub color: [u8; 3],
}

/// A pattern worked out for a picture.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Pattern {
    pub cols: Vec<Col>,
    /// From the top.
    pub rows: Vec<Row>,
}

/// The patterns' grays and colors.
const BORDER: [u8; 3] = [7, 7, 7];
const fn gray(v: u8) -> [u8; 3] {
    [v, v, v]
}
/// The bars at 75 %: gray, yellow, cyan, green, magenta, red and blue.
const BARS: [[u8; 3]; 7] = [
    gray(180),
    [180, 180, 16],
    [16, 180, 180],
    [16, 180, 16],
    [180, 16, 180],
    [180, 16, 16],
    [16, 16, 180],
];
/// The SMPTE bars' middle row: blue, black, magenta, black, cyan, black
/// and gray.
const CASTLES: [[u8; 3]; 7] = [
    [7, 7, 235],
    gray(16),
    [180, 7, 180],
    gray(16),
    [7, 180, 180],
    gray(16),
    gray(180),
];

/// `k` of `n` in single precision, as VEGAS's fractions of the frame are.
fn part(k: f32, n: usize) -> f32 {
    k * n as f32
}

/// A byte from a float, cut.
fn byte(v: f32) -> u8 {
    v.clamp(0.0, 255.0) as u8
}

impl Pattern {
    /// Pattern `kind` (the list's order: Color bars (NTSC), Color bars
    /// (PAL), Gradient, Pluge and porches, Ramp, SMPTE bars (NTSC), SMPTE
    /// bars (PAL), White porches, Yellow porches) on a picture `width` ×
    /// `height` pixels.
    pub(crate) fn new(kind: u32, width: usize, height: usize) -> Pattern {
        let (w, h) = (width.max(1), height.max(1));
        let (wf, hf) = (w as f32, h as f32);
        let seventh = (w / 7).max(1);
        let bar = |x: usize| BARS[(x / seventh).min(6)];
        // The border: a tenth of the frame all around.
        let bordered = matches!(kind, 3 | 4 | 7 | 8);
        let (tenth_w, tenth_h) = (wf / 10.0, hf / 10.0);
        let col_border = |x: usize| {
            let xf = x as f32;
            bordered && (xf < tenth_w || xf > wf - tenth_w)
        };
        let row_border = |r: usize| {
            let rf = r as f32;
            bordered && (rf < tenth_h || rf > hf - tenth_h)
        };
        // The gradients' steps.
        let (kx, ky) = (255.0 / wf, 255.0 / hf);
        let at = |x: usize, f: f32| (x as f32) < part(f, w);
        let porch = |x: usize| {
            if at(x, 0.3) {
                235
            } else if at(x, 0.5) {
                180
            } else if at(x, 0.7) {
                125
            } else {
                70
            }
        };
        let (sixth, third, half, q) = (w / 6, w / 3, w / 2, w / 21);
        let pluge = |x: usize| {
            if x < 15 * q {
                gray(16)
            } else if x < 16 * q {
                gray(7)
            } else if x < 17 * q {
                gray(16)
            } else if x < 18 * q {
                gray(25)
            } else {
                gray(16)
            }
        };
        let cols = (0..w)
            .map(|x| {
                let all = |c: [u8; 3]| Col {
                    bands: [c; 3],
                    from_row: [false; 3],
                };
                let col = match kind {
                    1 if x / seventh == 0 => all(gray(235)),
                    0 | 1 => all(bar(x)),
                    2 => {
                        let red = byte(255.0 - kx * x as f32);
                        Col {
                            bands: [[red, byte(kx * x as f32), 0]; 3],
                            from_row: [false, false, true],
                        }
                    }
                    3 if at(x, 0.2) => all(gray(7)),
                    3 if at(x, 0.3) => all(gray(16)),
                    3 if at(x, 0.4) => all(gray(25)),
                    3 if at(x, 0.5) => all(gray(16)),
                    3 => Col {
                        bands: [gray(0); 3],
                        from_row: [true; 3],
                    },
                    4 => all(gray(byte(kx * x as f32))),
                    5 | 6 => {
                        let bottom = if kind == 6 && x < half {
                            gray(16)
                        } else if x < sixth {
                            [7, 40, 113]
                        } else if x < third {
                            gray(235)
                        } else if x < half {
                            [70, 7, 165]
                        } else {
                            pluge(x)
                        };
                        let top = if kind == 6 && x / seventh == 0 {
                            gray(235)
                        } else {
                            bar(x)
                        };
                        Col {
                            bands: [bottom, CASTLES[(x / seventh).min(6)], top],
                            from_row: [false; 3],
                        }
                    }
                    7 => all(gray(porch(x))),
                    _ => {
                        let v = porch(x);
                        all([v, v, 7])
                    }
                };
                if col_border(x) {
                    Col {
                        bands: [BORDER; 3],
                        from_row: [false; 3],
                    }
                } else {
                    col
                }
            })
            .collect();
        let (quarter_h, third_h) = (h / 4, h / 3);
        let rows = (0..h)
            .map(|t| {
                let r = h - 1 - t;
                let rf = r as f32;
                let band = match kind {
                    5 | 6 if r < quarter_h => 0,
                    5 | 6 if r < third_h => 1,
                    _ => 2,
                };
                let color = match kind {
                    2 => [0, 0, byte(255.0 - ky * rf)],
                    3 if rf > part(0.7, h) => gray(235),
                    3 if rf > part(0.5, h) => gray(180),
                    3 if rf > part(0.3, h) => gray(125),
                    3 => gray(70),
                    _ => gray(0),
                };
                if row_border(r) {
                    Row {
                        band,
                        border: true,
                        color: BORDER,
                    }
                } else {
                    Row {
                        band,
                        border: false,
                        color,
                    }
                }
            })
            .collect();
        Pattern { cols, rows }
    }

    /// The pixel at column `x`, row `y` from the top: opaque, 0–1.
    pub(crate) fn at(&self, x: usize, y: usize) -> [f32; 4] {
        let (Some(c), Some(r)) = (self.cols.get(x), self.rows.get(y)) else {
            return [0.0; 4];
        };
        let rgb = if r.border {
            r.color
        } else {
            let band = c.bands[usize::from(r.band.min(2))];
            std::array::from_fn(|k| if c.from_row[k] { r.color[k] } else { band[k] })
        };
        [unit(rgb[0]), unit(rgb[1]), unit(rgb[2]), 1.0]
    }

    /// The tables as the card reads them: four rows of numbers for each
    /// column — its colors in the three bands and which channels the row
    /// brings (1) —, then two for each row from the top: its band and
    /// whether it is all border, then its color.
    pub(crate) fn table(&self) -> Vec<[f32; 4]> {
        let color = |c: [u8; 3]| [unit(c[0]), unit(c[1]), unit(c[2]), 1.0];
        let flag = |b: bool| if b { 1.0 } else { 0.0 };
        let mut t = Vec::with_capacity(4 * self.cols.len() + 2 * self.rows.len());
        for c in &self.cols {
            t.extend(c.bands.map(color));
            t.push([
                flag(c.from_row[0]),
                flag(c.from_row[1]),
                flag(c.from_row[2]),
                0.0,
            ]);
        }
        for r in &self.rows {
            t.push([f32::from(r.band), flag(r.border), 0.0, 0.0]);
            t.push(color(r.color));
        }
        t
    }
}

/// A byte's value, 0–1.
fn unit(v: u8) -> f32 {
    f32::from(v) / 255.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(p: &Pattern, x: usize, y: usize) -> [u8; 3] {
        let c = p.at(x, y);
        [0, 1, 2].map(|k| (c[k] * 255.0).round() as u8)
    }

    #[test]
    fn the_bars_are_a_seventh_cut_the_last_to_the_edge() {
        let p = Pattern::new(0, 640, 360);
        // 640 / 7 = 91: the blue bar from 546 to the edge.
        assert_eq!(rgb(&p, 90, 0), [180, 180, 180]);
        assert_eq!(rgb(&p, 91, 0), [180, 180, 16]);
        assert_eq!(rgb(&p, 545, 0), [180, 16, 16]);
        assert_eq!(rgb(&p, 639, 359), [16, 16, 180]);
        assert_eq!(rgb(&Pattern::new(1, 640, 360), 0, 0), [235, 235, 235]);
    }

    #[test]
    fn smpte_bars_have_their_three_rows() {
        let p = Pattern::new(5, 640, 360);
        // From the bottom: a quarter (90 rows) of −I, white, +Q and the
        // PLUGE, up to a third (120) the middle row, then the bars.
        let (bottom, middle, top) = (359, 359 - 95, 0);
        assert_eq!(rgb(&p, 0, bottom), [7, 40, 113]);
        assert_eq!(rgb(&p, 110, bottom), [235, 235, 235]);
        assert_eq!(rgb(&p, 220, bottom), [70, 7, 165]);
        // 640 / 21 = 30: −4 % from 450 to 480, +4 % from 510 to 540.
        assert_eq!(rgb(&p, 455, bottom), [7, 7, 7]);
        assert_eq!(rgb(&p, 515, bottom), [25, 25, 25]);
        assert_eq!(rgb(&p, 0, middle), [7, 7, 235]);
        assert_eq!(rgb(&p, 0, top), [180, 180, 180]);
        let pal = Pattern::new(6, 640, 360);
        assert_eq!(rgb(&pal, 0, bottom), [16, 16, 16]);
        assert_eq!(rgb(&pal, 0, top), [235, 235, 235]);
    }

    #[test]
    fn the_border_is_a_tenth_and_the_gradient_runs_both_ways() {
        let p = Pattern::new(7, 640, 360);
        assert_eq!(rgb(&p, 63, 100), [7, 7, 7]);
        assert_eq!(rgb(&p, 64, 100), [235, 235, 235]);
        assert_eq!(rgb(&p, 64, 0), [7, 7, 7]);
        let g = Pattern::new(2, 640, 360);
        assert_eq!(rgb(&g, 0, 359), [255, 0, 255]);
        let far = rgb(&g, 639, 0);
        assert!(far[0] <= 1 && far[1] >= 253 && far[2] <= 1, "{far:?}");
    }

    #[test]
    fn the_cards_table_holds_every_pixel() {
        for kind in 0..9 {
            let p = Pattern::new(kind, 97, 61);
            let t = p.table();
            let (w, h) = (97, 61);
            for y in 0..h {
                for x in 0..w {
                    let info = t[4 * w + 2 * y];
                    let row = t[4 * w + 2 * y + 1];
                    let px = if info[1] > 0.5 {
                        row
                    } else {
                        let band = t[4 * x + info[0] as usize];
                        let from = t[4 * x + 3];
                        std::array::from_fn(|k| if from[k] > 0.5 { row[k] } else { band[k] })
                    };
                    assert_eq!(px, p.at(x, y), "{kind}: {x} {y}");
                }
            }
        }
    }
}
