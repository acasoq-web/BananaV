//! What the color effects share: the pixels of each depth the plug-ins
//! take, read as straight colors (their alpha divided out), and a render
//! that passes every pixel's color through a function, its alpha kept
//! (told the pixel's place, for effects that need it: Add Noise) or
//! changed too (Color Corrector (Secondary)'s Alpha).
#![allow(unsafe_code)]

use std::ffi::c_int;

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::ofx::{Depth, Fx, Image, Props, for_rows};

/// A channel of a pixel, of one of the depths the plug-ins take.
pub(crate) trait Channel: Copy {
    const ZERO: Self;
    fn get(self) -> f32;
    fn put(v: f32) -> Self;
}

impl Channel for f32 {
    const ZERO: f32 = 0.0;
    fn get(self) -> f32 {
        self
    }
    fn put(v: f32) -> f32 {
        v
    }
}

impl Channel for u16 {
    const ZERO: u16 = 0;
    fn get(self) -> f32 {
        self as f32 / 65535.0
    }
    fn put(v: f32) -> u16 {
        (v.clamp(0.0, 1.0) * 65535.0).round() as u16
    }
}

impl Channel for u8 {
    const ZERO: u8 = 0;
    fn get(self) -> f32 {
        self as f32 / 255.0
    }
    fn put(v: f32) -> u8 {
        (v.clamp(0.0, 1.0) * 255.0).round() as u8
    }
}

/// Row `y` of a float RGBA image (most hosts' pictures): its pixels'
/// numbers as they are, to read whole.
pub(crate) fn rgba_row(img: &Image, y: c_int) -> Option<&[[f32; 4]]> {
    if img.depth != Depth::Float || img.channels != 4 {
        return None;
    }
    let r = img.row(y)?;
    let w = (img.bounds[2] - img.bounds[0]).max(0) as usize;
    // SAFETY: a row the image has, `w` RGBA float pixels long; the host
    // keeps it while the image lives.
    Some(unsafe { std::slice::from_raw_parts(r as *const [f32; 4], w) })
}

/// Writes `result` (premultiplied) over the output `out` within columns
/// `columns` and rows `rows` (the render window), on the host's threads:
/// float RGBA rows copied whole, other depths pixel by pixel.
pub(crate) fn write_out(
    out: &Image,
    result: &crate::kit::Buf,
    columns: std::ops::Range<c_int>,
    rows: std::ops::Range<c_int>,
) {
    let out_straight = out.channels == 4 && out.unpremultiplied;
    let plain = !out_straight && out.depth == Depth::Float && out.channels == 4;
    let x0 = out.bounds[0];
    let fill = |y: c_int| {
        let Some(row) = out.row(y) else { return };
        if plain {
            let w = (out.bounds[2] - x0).max(0) as usize;
            // SAFETY: a row the output has, `w` RGBA float pixels long,
            // which no other thread writes.
            let pixels = unsafe { std::slice::from_raw_parts_mut(row as *mut [f32; 4], w) };
            for x in columns.clone() {
                pixels[(x - x0) as usize] = result.inside(x, y);
            }
            return;
        }
        for x in columns.clone() {
            let mut c = result.inside(x, y);
            if out_straight && c[3] > 0.0 {
                c = [c[0] / c[3], c[1] / c[3], c[2] / c[3], c[3]];
            }
            // SAFETY: a row the output has, a column inside its bounds.
            unsafe { put_pixel(out, row, x, c) };
        }
    };
    for_rows(rows, &fill);
}

/// The pixel of `img` at `x`, `y` as it keeps it (alpha 1 when it has
/// none); `None` outside it or for an alpha-only image.
pub(crate) fn pixel(img: &Image, x: c_int, y: c_int) -> Option<[f32; 4]> {
    let [x1, _, x2, _] = img.bounds;
    if img.channels < 3 || !(x1..x2).contains(&x) {
        return None;
    }
    let r = img.row(y)?;
    let i = (x - x1) as usize * img.channels;
    let get = |c: usize| -> f32 {
        // SAFETY: a pixel of a row the image has, in its depth.
        unsafe {
            match img.depth {
                Depth::Float => *(r as *const f32).add(i + c),
                Depth::Short => (*(r as *const u16).add(i + c)).get(),
                Depth::Byte => (*r.add(i + c)).get(),
            }
        }
    };
    Some([
        get(0),
        get(1),
        get(2),
        if img.channels == 4 { get(3) } else { 1.0 },
    ])
}

/// The straight color of a pixel (its alpha divided out when
/// `premultiplied`); `None` when it is transparent.
pub(crate) fn straight(px: [f32; 4], premultiplied: bool) -> Option<[f32; 3]> {
    let a = px[3];
    if a <= 0.0 {
        return None;
    }
    let k = if premultiplied { 1.0 / a } else { 1.0 };
    Some([px[0] * k, px[1] * k, px[2] * k])
}

/// Writes one row of the output: each pixel the source's there, its
/// straight color and alpha through `f` (given the pixel's column).
///
/// # Safety
/// `out` starts the output's row at `out_x1`, `src` (when there is one)
/// the source's at its range's start; both hold `channels` channels of `T`
/// per pixel over the columns they are read and written at.
unsafe fn row<T: Channel>(
    out: *mut u8,
    out_x1: c_int,
    src: Option<(*const u8, std::ops::Range<c_int>)>,
    columns: std::ops::Range<c_int>,
    channels: usize,
    premultiplied: bool,
    f: &dyn Fn(c_int, [f32; 4]) -> [f32; 4],
) {
    let out = out as *mut T;
    for x in columns {
        // SAFETY: as the caller promises.
        unsafe {
            let o = out.add((x - out_x1) as usize * channels);
            match &src {
                Some((s, xs)) if xs.contains(&x) => {
                    let s = (*s as *const T).add((x - xs.start) as usize * channels);
                    if channels < 3 {
                        for c in 0..channels {
                            *o.add(c) = *s.add(c);
                        }
                        continue;
                    }
                    let a = if channels == 4 {
                        (*s.add(3)).get()
                    } else {
                        1.0
                    };
                    let v = [(*s).get(), (*s.add(1)).get(), (*s.add(2)).get()];
                    let mut alpha = a;
                    if premultiplied {
                        if a > 0.0 {
                            let q = f(x, [v[0] / a, v[1] / a, v[2] / a, a]);
                            alpha = q[3];
                            for (k, c) in q[..3].iter().enumerate() {
                                *o.add(k) = T::put(c * alpha);
                            }
                        } else {
                            for k in 0..3 {
                                *o.add(k) = T::ZERO;
                            }
                        }
                    } else {
                        let q = f(x, [v[0], v[1], v[2], a]);
                        alpha = q[3];
                        for (k, c) in q[..3].iter().enumerate() {
                            *o.add(k) = T::put(*c);
                        }
                    }
                    if channels == 4 {
                        // The source's own when it stays (exactly, in any depth).
                        *o.add(3) = if alpha == a { *s.add(3) } else { T::put(alpha) };
                    }
                }
                _ => {
                    for c in 0..channels {
                        *o.add(c) = T::ZERO;
                    }
                }
            }
        }
    }
}

/// The render action of a color effect: the source with each pixel's
/// straight color through `f`, its alpha kept.
pub(crate) fn render(fx: Fx, args: Props, f: &(dyn Fn([f32; 3]) -> [f32; 3] + Sync)) -> OfxStatus {
    render_at(fx, args, &|_, c| f(c))
}

/// [`render`] with `f` told each pixel's place: across and up from the
/// frame's first pixel, at render scale (wrapping below it).
pub(crate) fn render_at(
    fx: Fx,
    args: Props,
    f: &(dyn Fn([u32; 2], [f32; 3]) -> [f32; 3] + Sync),
) -> OfxStatus {
    render_rgba_at(fx, args, &|at, p| {
        let c = f(at, [p[0], p[1], p[2]]);
        [c[0], c[1], c[2], p[3]]
    })
}

/// The render action of an effect that changes alpha too: each pixel's
/// straight color and its alpha through `f`.
pub(crate) fn render_rgba(
    fx: Fx,
    args: Props,
    f: &(dyn Fn([f32; 4]) -> [f32; 4] + Sync),
) -> OfxStatus {
    render_rgba_at(fx, args, &|_, p| f(p))
}

/// [`render_rgba`] with `f` told each pixel's place, as [`render_at`].
fn render_rgba_at(
    fx: Fx,
    args: Props,
    f: &(dyn Fn([u32; 2], [f32; 4]) -> [f32; 4] + Sync),
) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let src = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time);
    if let Some(s) = &src
        && (s.depth != out.depth || s.channels != out.channels)
    {
        return kOfxStatErrImageFormat;
    }
    let premultiplied = out.channels == 4 && !src.as_ref().is_some_and(|s| s.unpremultiplied);
    let scale: [f64; 2] = args.dbls(kOfxImageEffectPropRenderScale);
    let scale = scale.map(|s| if s > 0.0 { s } else { 1.0 });
    let (origin, _) = fx.frame();
    let first = [
        (origin[0] * scale[0] / out.par).round() as c_int,
        (origin[1] * scale[1]).round() as c_int,
    ];
    let columns = x1.max(out.bounds[0])..x2.min(out.bounds[2]);
    let fill = |y: c_int| {
        let Some(o) = out.row(y) else { return };
        let source = src.as_ref().and_then(|s| {
            let r = s.row(y)?;
            Some((r as *const u8, s.bounds[0]..s.bounds[2]))
        });
        let (cols, n) = (columns.clone(), out.channels);
        let up = y.wrapping_sub(first[1]) as u32;
        let g = |x: c_int, c: [f32; 4]| f([x.wrapping_sub(first[0]) as u32, up], c);
        // SAFETY: rows the images have, columns inside the output's bounds
        // (and checked against the source's), in the images' own depth.
        unsafe {
            match out.depth {
                Depth::Float => row::<f32>(o, out.bounds[0], source, cols, n, premultiplied, &g),
                Depth::Short => row::<u16>(o, out.bounds[0], source, cols, n, premultiplied, &g),
                Depth::Byte => row::<u8>(o, out.bounds[0], source, cols, n, premultiplied, &g),
            }
        }
    };
    for_rows(y1.max(out.bounds[1])..y2.min(out.bounds[3]), &fill);
    drop(src);
    drop(out);
    kOfxStatOK
}

/// Writes `v` (as the image keeps its colors: premultiplied or not) as the
/// pixel at `x` of `row`, one of `img`'s rows (from `Image::row`).
///
/// # Safety
/// `row` is a row of `img` and `x` is inside its bounds.
pub(crate) unsafe fn put_pixel(img: &Image, row: *mut u8, x: c_int, v: [f32; 4]) {
    /// # Safety
    /// `row` holds pixel `i`, `channels` channels of `T` each.
    unsafe fn put<T: Channel>(row: *mut u8, i: usize, channels: usize, v: [f32; 4]) {
        // SAFETY: as the caller promises.
        unsafe {
            let o = (row as *mut T).add(i * channels);
            if channels == 1 {
                *o = T::put(v[3]);
            } else {
                for (c, x) in v.iter().take(channels).enumerate() {
                    *o.add(c) = T::put(*x);
                }
            }
        }
    }
    let i = (x - img.bounds[0]) as usize;
    // SAFETY: as the caller promises.
    unsafe {
        match img.depth {
            Depth::Float => put::<f32>(row, i, img.channels, v),
            Depth::Short => put::<u16>(row, i, img.channels, v),
            Depth::Byte => put::<u8>(row, i, img.channels, v),
        }
    }
}
