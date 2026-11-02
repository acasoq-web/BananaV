//! What the plug-ins written as tables of settings share ([`crate::filters`],
//! [`crate::transitions`]): their parameters' descriptions, the frame's
//! pixels as the frame's 0–1 coordinates, pictures sampled anywhere, and
//! a few small sums.

use std::ffi::{CStr, c_int};

use bv_ofx_sys::names::*;

use crate::ofx::{Fx, Image, Props};

/// A parameter's description: its name, label, default, range and hint.
#[derive(Clone, Copy)]
pub(crate) enum Spec {
    Number(&'static CStr, &'static CStr, f64, f64, f64, &'static CStr),
    /// A number whose slider spans less than what may be typed (VEGAS's
    /// display range): its name, label, default, range, slider's range and
    /// hint.
    Ranged(
        &'static CStr,
        &'static CStr,
        f64,
        [f64; 2],
        [f64; 2],
        &'static CStr,
    ),
    Angle(&'static CStr, &'static CStr, f64, &'static CStr),
    Int(&'static CStr, &'static CStr, i32, i32, i32, &'static CStr),
    Choice(
        &'static CStr,
        &'static CStr,
        &'static [&'static CStr],
        i32,
        &'static CStr,
    ),
    Toggle(&'static CStr, &'static CStr, bool, &'static CStr),
    Point(&'static CStr, &'static CStr, [f64; 2], &'static CStr),
    /// Two numbers that are not a place in the frame (VEGAS's offsets): its
    /// name, label, default, range, slider's range and hint.
    Pair(
        &'static CStr,
        &'static CStr,
        [f64; 2],
        [f64; 2],
        [f64; 2],
        &'static CStr,
    ),
    Color(&'static CStr, &'static CStr, [f64; 4], &'static CStr),
    /// Three plain numbers (VEGAS's 3D positions): its name, label,
    /// default, smallest and largest values and hint.
    Triple(
        &'static CStr,
        &'static CStr,
        [f64; 3],
        [f64; 3],
        [f64; 3],
        &'static CStr,
    ),
    /// A color without alpha (VEGAS's RGB colors): its name, label,
    /// default and hint.
    Rgb(&'static CStr, &'static CStr, [f64; 3], &'static CStr),
    /// A section of the panel (VEGAS's groups): the parameters after it,
    /// up to [`Spec::GroupEnd`], are in it. Its name and label.
    Group(&'static CStr, &'static CStr),
    GroupEnd,
}

impl Spec {
    pub(crate) fn name(self) -> &'static CStr {
        match self {
            Spec::Number(n, ..)
            | Spec::Ranged(n, ..)
            | Spec::Angle(n, ..)
            | Spec::Int(n, ..)
            | Spec::Choice(n, ..)
            | Spec::Toggle(n, ..)
            | Spec::Point(n, ..)
            | Spec::Pair(n, ..)
            | Spec::Triple(n, ..)
            | Spec::Color(n, ..)
            | Spec::Rgb(n, ..)
            | Spec::Group(n, _) => n,
            Spec::GroupEnd => c"",
        }
    }

    /// Whether it holds a value (not a group's start or end).
    pub(crate) fn has_value(self) -> bool {
        !matches!(self, Spec::Group(..) | Spec::GroupEnd)
    }

    pub(crate) fn default(self) -> Vec<f64> {
        match self {
            Spec::Number(_, _, d, ..) | Spec::Ranged(_, _, d, ..) | Spec::Angle(_, _, d, _) => {
                vec![d]
            }
            Spec::Int(_, _, d, ..) | Spec::Choice(_, _, _, d, _) => vec![f64::from(d)],
            Spec::Toggle(_, _, d, _) => vec![f64::from(u8::from(d))],
            Spec::Point(_, _, d, _) | Spec::Pair(_, _, d, ..) => d.to_vec(),
            Spec::Triple(_, _, d, ..) => d.to_vec(),
            Spec::Color(_, _, d, _) => d.to_vec(),
            Spec::Rgb(_, _, d, _) => d.to_vec(),
            Spec::Group(..) | Spec::GroupEnd => Vec::new(),
        }
    }

    /// Its value at `time` from the host.
    pub(crate) fn read(self, fx: Fx, time: f64) -> Vec<f64> {
        if !self.has_value() {
            return Vec::new();
        }
        let p = fx.param(self.name());
        match self {
            Spec::Point(..) | Spec::Pair(..) => p.xy_at(time).to_vec(),
            Spec::Triple(..) => p.xyz_at(time).to_vec(),
            Spec::Color(..) => p.rgba_at(time).to_vec(),
            Spec::Rgb(..) => p.rgb_at(time).to_vec(),
            Spec::Int(..) | Spec::Choice(..) | Spec::Toggle(..) => vec![f64::from(p.int_at(time))],
            _ => vec![p.f64_at(time)],
        }
    }
}

/// Defines the parameters `specs` describe, those after a
/// [`Spec::Group`] in its section.
pub(crate) fn define_all(fx: Fx, specs: &[Spec]) {
    let mut group: Option<&'static CStr> = None;
    for &spec in specs {
        match spec {
            Spec::Group(name, label) => {
                let g = fx.define_param(kOfxParamTypeGroup, name, label);
                g.set_int(kOfxParamPropGroupOpen, 1);
                group = Some(name);
            }
            Spec::GroupEnd => group = None,
            _ => {
                if let (Some(p), Some(parent)) = (define(fx, spec), group) {
                    p.set_str(kOfxParamPropParent, parent);
                }
            }
        }
    }
}

/// Defines the parameter `spec` describes.
pub(crate) fn define(fx: Fx, spec: Spec) -> Option<Props> {
    let p = match spec {
        Spec::Group(..) | Spec::GroupEnd => return None,
        Spec::Number(name, label, default, min, max, hint) => {
            let p = fx.define_param(kOfxParamTypeDouble, name, label);
            p.set_dbl(kOfxParamPropDefault, default);
            p.set_dbl(kOfxParamPropMin, min);
            p.set_dbl(kOfxParamPropMax, max);
            p.set_dbl(kOfxParamPropDisplayMin, min);
            p.set_dbl(kOfxParamPropDisplayMax, max);
            p.set_dbl(kOfxParamPropIncrement, (max - min) / 100.0);
            p.set_int(kOfxParamPropDigits, 3);
            p.set_str(kOfxParamPropHint, hint);
            p
        }
        Spec::Ranged(name, label, default, [min, max], [shown_min, shown_max], hint) => {
            let p = fx.define_param(kOfxParamTypeDouble, name, label);
            p.set_dbl(kOfxParamPropDefault, default);
            p.set_dbl(kOfxParamPropMin, min);
            p.set_dbl(kOfxParamPropMax, max);
            p.set_dbl(kOfxParamPropDisplayMin, shown_min);
            p.set_dbl(kOfxParamPropDisplayMax, shown_max);
            p.set_dbl(kOfxParamPropIncrement, (shown_max - shown_min) / 200.0);
            p.set_int(kOfxParamPropDigits, 4);
            p.set_str(kOfxParamPropHint, hint);
            p
        }
        Spec::Angle(name, label, default, hint) => {
            let p = fx.define_param(kOfxParamTypeDouble, name, label);
            p.set_str(kOfxParamPropDoubleType, kOfxParamDoubleTypeAngle);
            p.set_dbl(kOfxParamPropDefault, default);
            p.set_dbl(kOfxParamPropMin, -360.0);
            p.set_dbl(kOfxParamPropMax, 360.0);
            p.set_dbl(kOfxParamPropDisplayMin, -180.0);
            p.set_dbl(kOfxParamPropDisplayMax, 180.0);
            p.set_dbl(kOfxParamPropIncrement, 1.0);
            p.set_int(kOfxParamPropDigits, 1);
            p.set_str(kOfxParamPropHint, hint);
            p
        }
        Spec::Int(name, label, default, min, max, hint) => {
            let p = fx.define_param(kOfxParamTypeInteger, name, label);
            p.set_int(kOfxParamPropDefault, default);
            p.set_int(kOfxParamPropMin, min);
            p.set_int(kOfxParamPropMax, max);
            p.set_int(kOfxParamPropDisplayMin, min);
            p.set_int(kOfxParamPropDisplayMax, max);
            p.set_str(kOfxParamPropHint, hint);
            p
        }
        Spec::Choice(name, label, options, default, hint) => {
            let p = fx.define_param(kOfxParamTypeChoice, name, label);
            p.set_strs(kOfxParamPropChoiceOption, options);
            p.set_int(kOfxParamPropDefault, default);
            p.set_str(kOfxParamPropHint, hint);
            p
        }
        Spec::Toggle(name, label, default, hint) => {
            let p = fx.define_param(kOfxParamTypeBoolean, name, label);
            p.set_int(kOfxParamPropDefault, c_int::from(default));
            p.set_str(kOfxParamPropHint, hint);
            p
        }
        Spec::Point(name, label, default, hint) => {
            let p = fx.define_param(kOfxParamTypeDouble2D, name, label);
            p.set_str(
                kOfxParamPropDoubleType,
                kOfxParamDoubleTypeNormalisedXYAbsolute,
            );
            p.set_dbls(kOfxParamPropDefault, &default);
            p.set_str(kOfxParamPropHint, hint);
            p
        }
        Spec::Pair(name, label, default, range, slider, hint) => {
            let p = fx.define_param(kOfxParamTypeDouble2D, name, label);
            p.set_dbls(kOfxParamPropDefault, &default);
            p.set_dbls(kOfxParamPropMin, &[range[0]; 2]);
            p.set_dbls(kOfxParamPropMax, &[range[1]; 2]);
            p.set_dbls(kOfxParamPropDisplayMin, &[slider[0]; 2]);
            p.set_dbls(kOfxParamPropDisplayMax, &[slider[1]; 2]);
            p.set_dbl(kOfxParamPropIncrement, (slider[1] - slider[0]) / 200.0);
            p.set_int(kOfxParamPropDigits, 4);
            p.set_str(kOfxParamPropHint, hint);
            p
        }
        Spec::Triple(name, label, default, min, max, hint) => {
            let p = fx.define_param(kOfxParamTypeDouble3D, name, label);
            p.set_dbls(kOfxParamPropDefault, &default);
            p.set_dbls(kOfxParamPropMin, &min);
            p.set_dbls(kOfxParamPropMax, &max);
            p.set_dbls(kOfxParamPropDisplayMin, &min);
            p.set_dbls(kOfxParamPropDisplayMax, &max);
            p.set_dbl(kOfxParamPropIncrement, 0.1);
            p.set_int(kOfxParamPropDigits, 4);
            p.set_str(kOfxParamPropHint, hint);
            p
        }
        Spec::Rgb(name, label, default, hint) => {
            let p = fx.define_param(kOfxParamTypeRGB, name, label);
            p.set_dbls(kOfxParamPropDefault, &default);
            p.set_str(kOfxParamPropHint, hint);
            p.set_str(bv_ofx_sys::extensions::kBananaVParamPropColorModel, c"RGB");
            p
        }
        Spec::Color(name, label, default, hint) => {
            let p = fx.define_param(kOfxParamTypeRGBA, name, label);
            p.set_dbls(kOfxParamPropDefault, &default);
            p.set_str(kOfxParamPropHint, hint);
            p.set_str(bv_ofx_sys::extensions::kBananaVParamPropColorModel, c"RGB");
            p
        }
    };
    Some(p)
}

/// The values of a plug-in's parameters at a time, by name.
pub(crate) struct Vals {
    values: Vec<(&'static CStr, Vec<f64>)>,
}

impl Vals {
    /// Each of `specs`' values from `get`.
    pub(crate) fn of(specs: &[Spec], get: &dyn Fn(Spec) -> Vec<f64>) -> Vals {
        Vals {
            values: specs
                .iter()
                .filter(|s| s.has_value())
                .map(|s| (s.name(), get(*s)))
                .collect(),
        }
    }

    fn get(&self, name: &CStr) -> &[f64] {
        self.values
            .iter()
            .find(|(n, _)| *n == name)
            .map_or(&[], |(_, v)| v.as_slice())
    }

    pub(crate) fn f(&self, name: &CStr) -> f64 {
        self.get(name).first().copied().unwrap_or(0.0)
    }

    pub(crate) fn i(&self, name: &CStr) -> i32 {
        self.f(name).round() as i32
    }

    pub(crate) fn b(&self, name: &CStr) -> bool {
        self.f(name) != 0.0
    }

    /// A point as the frame counts it from its top left (VEGAS's count up
    /// from the bottom).
    pub(crate) fn point(&self, name: &CStr) -> [f64; 2] {
        let v = self.get(name);
        [
            v.first().copied().unwrap_or(0.5),
            1.0 - v.get(1).copied().unwrap_or(0.5),
        ]
    }

    /// Two plain numbers (a [`Spec::Pair`]'s).
    pub(crate) fn pair(&self, name: &CStr) -> [f64; 2] {
        let v = self.get(name);
        [
            v.first().copied().unwrap_or(0.0),
            v.get(1).copied().unwrap_or(0.0),
        ]
    }

    /// Three plain numbers (a [`Spec::Triple`]'s).
    pub(crate) fn triple(&self, name: &CStr) -> [f64; 3] {
        let v = self.get(name);
        std::array::from_fn(|k| v.get(k).copied().unwrap_or(0.0))
    }

    /// A color without alpha (a [`Spec::Rgb`]'s).
    pub(crate) fn get_rgb(&self, name: &CStr) -> [f64; 3] {
        let v = self.get(name);
        std::array::from_fn(|k| v.get(k).copied().unwrap_or(1.0))
    }

    pub(crate) fn rgba(&self, name: &CStr) -> [f32; 4] {
        let v = self.get(name);
        std::array::from_fn(|k| v.get(k).copied().unwrap_or(1.0) as f32)
    }
}

/// Where the frame's pixels are: canonical coordinates and pixels.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Grid {
    pub origin: [f64; 2],
    pub frame: [f64; 2],
    pub scale: [f64; 2],
    pub par: f64,
}

impl Grid {
    /// Pixel (x, y)'s middle as the frame's u (0 left) and v (0 top).
    pub(crate) fn uv(&self, x: c_int, y: c_int) -> (f64, f64) {
        let cx = (x as f64 + 0.5) * self.par / self.scale[0] - self.origin[0];
        let cy = (y as f64 + 0.5) / self.scale[1] - self.origin[1];
        (cx / self.frame[0], 1.0 - cy / self.frame[1])
    }

    /// The pixel position (its middle at whole numbers) of u, v.
    pub(crate) fn xy(&self, u: f64, v: f64) -> (f64, f64) {
        let cx = u * self.frame[0] + self.origin[0];
        let cy = (1.0 - v) * self.frame[1] + self.origin[1];
        (
            cx * self.scale[0] / self.par - 0.5,
            cy * self.scale[1] - 0.5,
        )
    }

    /// The frame's width over its height, as it shows.
    pub(crate) fn aspect(&self) -> f64 {
        self.frame[0] / self.frame[1].max(1.0)
    }

    /// The frame's height in pixels at this scale.
    pub(crate) fn height_px(&self) -> f64 {
        self.frame[1] * self.scale[1]
    }

    /// The frame's width in pixels at this scale.
    pub(crate) fn width_px(&self) -> f64 {
        self.frame[0] * self.scale[0] / self.par
    }

    /// How many times as wide as tall a pixel shows: the pixel aspect
    /// times the render scale's height over its width.
    pub(crate) fn pixel_shape(&self) -> f64 {
        self.par * self.scale[1] / self.scale[0]
    }
}

/// A picture's pixels, premultiplied, read once: the window of the frame
/// it covers, rows going up as OpenFX counts them.
#[derive(Clone, Debug)]
pub(crate) struct Buf {
    pub bounds: [c_int; 4],
    pub data: Vec<[f32; 4]>,
}

impl Buf {
    /// `img`'s pixels (none: an empty picture).
    pub(crate) fn read(img: Option<&Image>) -> Buf {
        let Some(img) = img else {
            return Buf {
                bounds: [0, 0, 0, 0],
                data: Vec::new(),
            };
        };
        let [x1, y1, x2, y2] = img.bounds;
        let straight = img.channels == 4 && img.unpremultiplied;
        let (w, h) = ((x2 - x1).max(0) as usize, (y2 - y1).max(0));
        let mut buf = Buf {
            bounds: img.bounds,
            data: vec![[0.0; 4]; w * h as usize],
        };
        // Rows on the host's threads: float RGBA ones copied whole.
        buf.rows(|y, row| {
            match crate::colors::rgba_row(img, y) {
                Some(src) => row.copy_from_slice(&src[..row.len()]),
                None => {
                    for (x, px) in (x1..x2).zip(row.iter_mut()) {
                        *px = crate::colors::pixel(img, x, y).unwrap_or([0.0; 4]);
                    }
                }
            }
            if straight {
                for c in row.iter_mut() {
                    *c = [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]];
                }
            }
        });
        buf
    }

    /// Works out each row (`y`, its pixels) on the host's threads.
    pub(crate) fn rows(&mut self, f: impl Fn(c_int, &mut [[f32; 4]]) + Sync) {
        let (w, y1) = (self.width().max(0) as usize, self.bounds[1]);
        for_chunks(&mut self.data, w, |j, row| f(y1 + j as c_int, row));
    }

    /// Works out the rows in bands of `rows` rows (the band's first `y`, its
    /// pixels row after row) on the host's threads: for work that runs down
    /// the picture.
    pub(crate) fn bands(&mut self, rows: usize, f: impl Fn(c_int, &mut [[f32; 4]]) + Sync) {
        let (w, y1, rows) = (self.width().max(0) as usize, self.bounds[1], rows.max(1));
        for_chunks(&mut self.data, w * rows, |j, band| {
            f(y1 + (j * rows) as c_int, band)
        });
    }

    /// A picture of the host's (rows from the top) as a buffer from
    /// (0, 0), rows up.
    pub(crate) fn of_image(img: &bv_core::video::ImageRgbaF32) -> Buf {
        let (w, h) = (img.width as usize, img.height as usize);
        let mut data = Vec::with_capacity(w * h);
        for y in (0..h).rev() {
            data.extend_from_slice(&img.data[y * w..(y + 1) * w]);
        }
        Buf {
            bounds: [0, 0, w as c_int, h as c_int],
            data,
        }
    }

    /// One color all over (the tests').
    #[cfg(test)]
    pub(crate) fn solid(bounds: [c_int; 4], c: [f32; 4]) -> Buf {
        let n = ((bounds[2] - bounds[0]) * (bounds[3] - bounds[1])).max(0) as usize;
        Buf {
            bounds,
            data: vec![c; n],
        }
    }

    pub(crate) fn width(&self) -> c_int {
        self.bounds[2] - self.bounds[0]
    }

    pub(crate) fn height(&self) -> c_int {
        self.bounds[3] - self.bounds[1]
    }

    /// Pixel (x, y), its edges held (nothing when it is empty).
    pub(crate) fn at(&self, x: c_int, y: c_int) -> [f32; 4] {
        if self.data.is_empty() {
            return [0.0; 4];
        }
        let xi = (x.clamp(self.bounds[0], self.bounds[2] - 1) - self.bounds[0]) as usize;
        let yi = (y.clamp(self.bounds[1], self.bounds[3] - 1) - self.bounds[1]) as usize;
        self.data[yi * self.width() as usize + xi]
    }

    /// Pixel (x, y), nothing outside it.
    pub(crate) fn inside(&self, x: c_int, y: c_int) -> [f32; 4] {
        if x < self.bounds[0] || x >= self.bounds[2] || y < self.bounds[1] || y >= self.bounds[3] {
            return [0.0; 4];
        }
        self.at(x, y)
    }

    /// The picture between pixels, bilinearly, its edges held.
    pub(crate) fn bilinear(&self, x: f64, y: f64) -> [f32; 4] {
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = ((x - x0) as f32, (y - y0) as f32);
        let (xi, yi) = (x0 as c_int, y0 as c_int);
        let (a, b, c, d) = (
            self.at(xi, yi),
            self.at(xi + 1, yi),
            self.at(xi, yi + 1),
            self.at(xi + 1, yi + 1),
        );
        std::array::from_fn(|i| {
            let low = a[i] + (b[i] - a[i]) * fx;
            let high = c[i] + (d[i] - c[i]) * fx;
            low + (high - low) * fy
        })
    }

    /// The picture at the frame's u, v (nothing outside the frame).
    pub(crate) fn sample(&self, grid: &Grid, u: f64, v: f64) -> [f32; 4] {
        if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            return [0.0; 4];
        }
        let (x, y) = grid.xy(u, v);
        self.bilinear(x, y)
    }

    /// The picture at u, v with its edges held beyond the frame.
    pub(crate) fn sample_held(&self, grid: &Grid, u: f64, v: f64) -> [f32; 4] {
        let (x, y) = grid.xy(u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
        self.bilinear(x, y)
    }

    /// A new picture as big, from `f` of each pixel.
    pub(crate) fn map(&self, f: impl Fn(c_int, c_int) -> [f32; 4] + Sync) -> Buf {
        let mut out = Buf {
            bounds: self.bounds,
            data: vec![[0.0; 4]; self.data.len()],
        };
        let x1 = self.bounds[0];
        out.rows(|y, row| {
            for (i, px) in row.iter_mut().enumerate() {
                *px = f(x1 + i as c_int, y);
            }
        });
        out
    }
}

/// Works out `data` in chunks of `len` items (each chunk's number and its
/// items) on the host's threads.
pub(crate) fn for_chunks<T: Send>(data: &mut [T], len: usize, f: impl Fn(usize, &mut [T]) + Sync) {
    if len == 0 {
        return;
    }
    let chunks: Vec<std::sync::Mutex<&mut [T]>> =
        data.chunks_mut(len).map(std::sync::Mutex::new).collect();
    crate::ofx::for_rows(0..chunks.len() as c_int, &|j| {
        if let Ok(mut chunk) = chunks[j as usize].lock() {
            f(j as usize, &mut chunk);
        }
    });
}

/// A smooth step from 0 at `e0` to 1 at `e1`.
pub(crate) fn smooth(e0: f64, e1: f64, x: f64) -> f64 {
    if e1 <= e0 {
        return if x >= e0 { 1.0 } else { 0.0 };
    }
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub(crate) fn mix(a: [f32; 4], b: [f32; 4], k: f64) -> [f32; 4] {
    let k = k as f32;
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * k)
}

/// A number 0–1 from two coordinates and a seed, always the same for them.
/// A value's 8-bit step, as VEGAS's 8-bit pictures hold it: rounded, a
/// value on a half step (0.5's 127.5) going up — nudged a ten-thousandth
/// of a step, so that the processor's floats and the card's, a last bit
/// or two apart, round alike (the shaders' `step8`).
pub(crate) fn step8(v: f32) -> u32 {
    (v * 255.0 + 0.5001).floor().clamp(0.0, 255.0) as u32
}

pub(crate) fn hash01(x: u32, y: u32, seed: u32) -> f64 {
    let mut h =
        x.wrapping_mul(0x8da6_b343) ^ y.wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^= h >> 16;
    f64::from(h) / f64::from(u32::MAX)
}

/// BT.709's luma of a color.
pub(crate) fn luma(c: [f32; 4]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// The three box widths that stand for a Gaussian of `sigma` (each odd:
/// some one size, the rest two more).
pub(crate) fn gauss_boxes(sigma: f32) -> [usize; 3] {
    let ideal = (12.0 * sigma * sigma / 3.0 + 1.0).sqrt();
    let mut low = ideal.floor() as i32;
    if low % 2 == 0 {
        low -= 1;
    }
    let low = low.max(1);
    let high = low + 2;
    let (l, n) = (low as f32, 3.0);
    let m = ((12.0 * sigma * sigma - n * l * l - 4.0 * n * l - 3.0 * n) / (-4.0 * l - 4.0)).round();
    std::array::from_fn(|i| {
        if (i as f32) < m {
            low as usize
        } else {
            high as usize
        }
    })
}

/// A box of `radius` across each row of `src` (`width` wide) into `dst`:
/// each value the mean of those `radius` either side, the row's ends
/// repeated past it.
pub(crate) fn box_rows(src: &[f32], dst: &mut [f32], width: usize, radius: usize) {
    if width == 0 {
        return;
    }
    let r = radius as isize;
    let scale = 1.0 / (2 * radius + 1) as f32;
    for (row, out) in src.chunks(width).zip(dst.chunks_mut(width)) {
        let at = |i: isize| row[i.clamp(0, width as isize - 1) as usize];
        let mut sum: f32 = (-r..=r).map(at).sum();
        for (x, o) in out.iter_mut().enumerate() {
            *o = sum * scale;
            let x = x as isize;
            sum += at(x + r + 1) - at(x - r);
        }
    }
}

/// [`box_rows`] down the columns of `src` (`width` wide).
pub(crate) fn box_columns(src: &[f32], dst: &mut [f32], width: usize, radius: usize) {
    if width == 0 || src.is_empty() {
        return;
    }
    let height = src.len() / width;
    let r = radius as isize;
    let scale = 1.0 / (2 * radius + 1) as f32;
    for x in 0..width {
        let at = |i: isize| src[i.clamp(0, height as isize - 1) as usize * width + x];
        let mut sum: f32 = (-r..=r).map(at).sum();
        for y in 0..height {
            dst[y * width + x] = sum * scale;
            let y = y as isize;
            sum += at(y + r + 1) - at(y - r);
        }
    }
}

/// A plane (`width` wide) blurred by three boxes across standing for a
/// Gaussian of `sigma_x`, then three down for `sigma_y`.
pub(crate) fn gauss_plane(plane: &mut Vec<f32>, width: usize, sigma_x: f32, sigma_y: f32) {
    let mut other = vec![0.0; plane.len()];
    for size in gauss_boxes(sigma_x) {
        box_rows(plane, &mut other, width, (size - 1) / 2);
        std::mem::swap(plane, &mut other);
    }
    for size in gauss_boxes(sigma_y) {
        box_columns(plane, &mut other, width, (size - 1) / 2);
        std::mem::swap(plane, &mut other);
    }
}
