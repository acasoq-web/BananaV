//! Conversão de quadros inteiros: quadro decodificado ↔ imagem linear de trabalho.
//!
//! Posição do croma (H.273 `chroma_sample_loc_type` 0, o padrão de H.264,
//! HEVC e MPEG-2): em 4:2:0 e 4:2:2 a amostra de croma fica alinhada com a
//! coluna de luma par; em 4:2:0 fica verticalmente entre duas linhas de luma.

use bv_core::{
    ColorInfo, ColorPrimaries, ImageRgbaF32, PixelFormat, Plane, PlaneData, TransferCharacteristic,
    VideoFrame,
};

use crate::matrix::{self, Normalizer};
use crate::primaries;
use crate::transfer::{DecodeLut, EncodeLut};

/// Primárias do espaço de trabalho.
pub const WORKING_PRIMARIES: ColorPrimaries = ColorPrimaries::Bt709;

/// Interpola uma linha de croma até a largura do luma.
fn upsample_chroma_row(plane: &Plane, y: usize, shift: (u8, u8), width: usize, out: &mut [f32]) {
    let (sx, sy) = shift;
    let ch = plane.height;
    let cw = plane.width;
    let (j0, j1, wy) = if sy == 1 {
        // Entre as linhas: luma y ↔ croma (y − 0,5) / 2.
        let fy = (y as f32 * 0.5 - 0.25).max(0.0);
        let j0 = (fy as usize).min(ch - 1);
        (j0, (j0 + 1).min(ch - 1), fy - j0 as f32)
    } else {
        let j = y.min(ch - 1);
        (j, j, 0.0)
    };
    for (x, o) in out.iter_mut().enumerate().take(width) {
        let (i0, i1, wx) = if sx == 1 {
            let i0 = (x >> 1).min(cw - 1);
            (i0, (i0 + 1).min(cw - 1), if x & 1 == 1 { 0.5 } else { 0.0 })
        } else {
            let i = x.min(cw - 1);
            (i, i, 0.0)
        };
        let a = plane.sample(i0, j0) as f32;
        let b = plane.sample(i1, j0) as f32;
        let c = plane.sample(i0, j1) as f32;
        let d = plane.sample(i1, j1) as f32;
        let top = a + (b - a) * wx;
        let bottom = c + (d - c) * wx;
        *o = top + (bottom - top) * wy;
    }
}

/// The most threads a frame's work is split into (0: the processor's).
static THREADS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Sets the most threads frames are processed with (Options › Preferences ›
/// Video, "Maximum number of rendering threads"; 0: all the processor's).
pub fn set_threads(n: usize) {
    THREADS.store(n, std::sync::atomic::Ordering::Relaxed);
}

/// The threads a frame's work is split into.
pub fn threads() -> usize {
    let all = std::thread::available_parallelism().map_or(1, |n| n.get());
    match THREADS.load(std::sync::atomic::Ordering::Relaxed) {
        0 => all,
        n => n.min(all),
    }
    .max(1)
}

/// Works out `data`'s rows, `width` items each, on the threads frames are
/// processed with (`f` gets each row's number and the row).
pub fn par_rows<T: Send>(data: &mut [T], width: usize, f: impl Fn(usize, &mut [T]) + Sync) {
    let height = data.len() / width.max(1);
    // A thread costs tens of microseconds to start: small images get few.
    let n = threads().min(data.len() / (32 * 1024) + 1);
    if n == 1 || height < 32 {
        for (y, row) in data.chunks_mut(width).enumerate() {
            f(y, row);
        }
        return;
    }
    let per = height.div_ceil(n);
    std::thread::scope(|s| {
        for (ci, chunk) in data.chunks_mut(width * per).enumerate() {
            let f = &f;
            s.spawn(move || {
                for (i, row) in chunk.chunks_mut(width).enumerate() {
                    f(ci * per + i, row);
                }
            });
        }
    });
}

/// Converte um quadro decodificado para RGB linear BT.709 pré-multiplicado.
pub fn frame_to_linear(f: &VideoFrame) -> ImageRgbaF32 {
    frame_to_linear_scaled(f, 1)
}

/// Como [`frame_to_linear`], reduzindo a imagem `factor` vezes em cada eixo
/// (média de blocos `factor`×`factor`) já na conversão. É o caminho rápido
/// da prévia: converter 1080p para um monitor de 640 px faz ¼ do trabalho.
pub fn frame_to_linear_scaled(f: &VideoFrame, factor: u32) -> ImageRgbaF32 {
    frame_to_linear_levels(f, factor, false)
}

/// As [`frame_to_linear_scaled`]; with `video_levels` (VEGAS's video
/// levels pixel formats) Y′CbCr comes in as studio RGB — its 16–235 kept,
/// not stretched to 0–255, whatever range the frame says —, as
/// [`linear_to_420_levels`] and [`linear_to_frame_levels`] give it back.
pub fn frame_to_linear_levels(f: &VideoFrame, factor: u32, video_levels: bool) -> ImageRgbaF32 {
    let range = if video_levels {
        bv_core::ColorRange::Full
    } else {
        f.color.range
    };
    let k = factor.max(1) as usize;
    let w = f.width as usize;
    let h = f.height as usize;
    let ow = w.div_ceil(k);
    let oh = h.div_ceil(k);
    let mut out = ImageRgbaF32::transparent(ow as u32, oh as u32);
    let lut = DecodeLut::shared(f.color.transfer);
    let gamut = (f.color.primaries != WORKING_PRIMARIES)
        .then(|| primaries::conversion(f.color.primaries, WORKING_PRIMARIES));
    let finish = |rgb: [f32; 3]| -> [f32; 3] {
        let lin = [lut.eval(rgb[0]), lut.eval(rgb[1]), lut.eval(rgb[2])];
        match &gamut {
            Some(m) => primaries::apply(m, lin),
            None => lin,
        }
    };
    // Média de um bloco de um plano (coordenadas do plano).
    let avg = |p: &Plane, x0: usize, y0: usize, bw: usize, bh: usize| -> f32 {
        let x1 = (x0 + bw).min(p.width).max(x0 + 1);
        let y1 = (y0 + bh).min(p.height).max(y0 + 1);
        let mut acc = 0u32;
        for y in y0.min(p.height - 1)..y1.min(p.height) {
            for x in x0.min(p.width - 1)..x1.min(p.width) {
                acc += p.sample(x, y) as u32;
            }
        }
        let n = ((x1.min(p.width) - x0.min(p.width - 1))
            * (y1.min(p.height) - y0.min(p.height - 1)))
        .max(1);
        acc as f32 / n as f32
    };

    match f.format {
        PixelFormat::Rgba8 => {
            let p = &f.planes[0];
            let d = p.as_u8().expect("RGBA8 tem plano de 8 bits");
            par_rows(&mut out.data, ow, |oy, row| {
                for (ox, px) in row.iter_mut().enumerate() {
                    let (x, y) = ((ox * k).min(w - 1), (oy * k).min(h - 1));
                    let i = y * p.stride + x * 4;
                    let a = d[i + 3] as f32 / 255.0;
                    let [r, g, b] = finish([
                        d[i] as f32 / 255.0,
                        d[i + 1] as f32 / 255.0,
                        d[i + 2] as f32 / 255.0,
                    ]);
                    *px = [r * a, g * a, b * a, a];
                }
            });
        }
        PixelFormat::Gray8 => {
            let n = Normalizer::new(range, 8);
            let p = &f.planes[0];
            par_rows(&mut out.data, ow, |oy, row| {
                for (ox, px) in row.iter_mut().enumerate() {
                    let v = n.luma(avg(p, ox * k, oy * k, k, k));
                    let [r, g, b] = finish([v, v, v]);
                    *px = [r, g, b, 1.0];
                }
            });
        }
        _ => {
            let n = Normalizer::new(range, f.format.bit_depth());
            let shift = f.format.chroma_shift();
            let coeffs = matrix::kr_kb(f.color.matrix);
            let convert = |y: f32, cb: f32, cr: f32| -> [f32; 4] {
                let rgb = match coeffs {
                    Some((kr, kb)) => {
                        matrix::ycbcr_to_rgb(kr, kb, n.luma(y), n.chroma(cb), n.chroma(cr))
                    }
                    // GBR: Y = G, Cb = B, Cr = R.
                    None => [n.luma(cr), n.luma(y), n.luma(cb)],
                };
                let [r, g, b] = finish(rgb);
                [r, g, b, 1.0]
            };
            if k == 1 {
                par_rows(&mut out.data, w, |y, row| {
                    let mut cb_row = vec![0.0f32; w];
                    let mut cr_row = vec![0.0f32; w];
                    upsample_chroma_row(&f.planes[1], y, shift, w, &mut cb_row);
                    upsample_chroma_row(&f.planes[2], y, shift, w, &mut cr_row);
                    for (x, px) in row.iter_mut().enumerate() {
                        *px = convert(f.planes[0].sample(x, y) as f32, cb_row[x], cr_row[x]);
                    }
                });
            } else {
                let (sx, sy) = (shift.0 as usize, shift.1 as usize);
                par_rows(&mut out.data, ow, |oy, row| {
                    for (ox, px) in row.iter_mut().enumerate() {
                        let (x0, y0) = (ox * k, oy * k);
                        let yv = avg(&f.planes[0], x0, y0, k, k);
                        let (cx, cy) = (x0 >> sx, y0 >> sy);
                        let (cw, ch) = ((k >> sx).max(1), (k >> sy).max(1));
                        let cb = avg(&f.planes[1], cx, cy, cw, ch);
                        let cr = avg(&f.planes[2], cx, cy, cw, ch);
                        *px = convert(yv, cb, cr);
                    }
                });
            }
        }
    }
    out
}

/// Filtro [1 2 1]/4 na horizontal, centrado na coluna `2i` (croma co-situado).
#[inline]
fn h121(row: &[f32], i: usize) -> f32 {
    let c = 2 * i;
    let l = row[c.saturating_sub(1)];
    let m = row[c.min(row.len() - 1)];
    let r = row[(c + 1).min(row.len() - 1)];
    0.25 * l + 0.5 * m + 0.25 * r
}

fn quantize(v: f32, max: f32) -> u16 {
    v.round().clamp(0.0, max) as u16
}

/// Converte a imagem linear de trabalho em um quadro no formato pedido.
/// A imagem deve estar opaca (composta sobre preto); o alfa é descartado.
pub fn linear_to_frame(img: &ImageRgbaF32, format: PixelFormat, color: ColorInfo) -> VideoFrame {
    linear_to_frame_levels(img, format, color, false)
}

/// As [`linear_to_frame`]; with `video_levels` the picture is studio RGB,
/// its levels kept in the Y′CbCr frame (see [`linear_to_420_levels`]).
pub fn linear_to_frame_levels(
    img: &ImageRgbaF32,
    format: PixelFormat,
    color: ColorInfo,
    video_levels: bool,
) -> VideoFrame {
    let range = if video_levels {
        bv_core::ColorRange::Full
    } else {
        color.range
    };
    let w = img.width as usize;
    let h = img.height as usize;
    let lut = EncodeLut::shared(color.transfer);
    let gamut = (color.primaries != WORKING_PRIMARIES)
        .then(|| primaries::conversion(WORKING_PRIMARIES, color.primaries));
    let encode = |px: [f32; 4]| -> [f32; 3] {
        let lin = [px[0], px[1], px[2]];
        let lin = match &gamut {
            Some(m) => primaries::apply(m, lin),
            None => lin,
        };
        lin.map(|c| lut.eval(c).clamp(0.0, 1.0))
    };

    let mut frame = VideoFrame::black(img.width, img.height, format, color);
    if format == PixelFormat::Rgba8 {
        let PlaneData::U8(d) = &mut frame.planes[0].data else {
            unreachable!()
        };
        for (i, px) in img.data.iter().enumerate() {
            let rgb = encode(*px);
            for c in 0..3 {
                d[i * 4 + c] = (rgb[c] * 255.0).round() as u8;
            }
            d[i * 4 + 3] = 255;
        }
        return frame;
    }

    let depth = format.bit_depth();
    let max = ((1u32 << depth) - 1) as f32;
    let n = Normalizer::new(range, depth);
    let coeffs = matrix::kr_kb(color.matrix);
    // Y′, Cb e Cr em resolução cheia, como valores inteiros ainda em float.
    let mut ys = vec![0.0f32; w * h];
    let mut cbs = vec![0.0f32; w * h];
    let mut crs = vec![0.0f32; w * h];
    for (i, px) in img.data.iter().enumerate() {
        let rgb = encode(*px);
        let (y, cb, cr) = match coeffs {
            Some((kr, kb)) => {
                let [y, cb, cr] = matrix::rgb_to_ycbcr(kr, kb, rgb);
                (n.luma_inv(y), n.chroma_inv(cb), n.chroma_inv(cr))
            }
            None => (n.luma_inv(rgb[1]), n.luma_inv(rgb[2]), n.luma_inv(rgb[0])),
        };
        ys[i] = y;
        cbs[i] = cb;
        crs[i] = cr;
    }

    let (sx, sy) = format.chroma_shift();
    let cw = frame.planes[1].width;
    let chh = frame.planes[1].height;
    let sub = |full: &[f32]| -> Vec<u16> {
        let mut out = Vec::with_capacity(cw * chh);
        for j in 0..chh {
            let rows: Vec<&[f32]> = if sy == 1 {
                let r0 = (2 * j).min(h - 1);
                let r1 = (2 * j + 1).min(h - 1);
                vec![&full[r0 * w..r0 * w + w], &full[r1 * w..r1 * w + w]]
            } else {
                vec![&full[j * w..j * w + w]]
            };
            for i in 0..cw {
                let v: f32 = rows
                    .iter()
                    .map(|row| if sx == 1 { h121(row, i) } else { row[i] })
                    .sum::<f32>()
                    / rows.len() as f32;
                out.push(quantize(v, max));
            }
        }
        out
    };
    let cb = sub(&cbs);
    let cr = sub(&crs);
    let y: Vec<u16> = ys.iter().map(|&v| quantize(v, max)).collect();

    let store = |plane: &mut Plane, v: Vec<u16>| {
        plane.data = if depth == 8 {
            PlaneData::U8(v.into_iter().map(|x| x as u8).collect())
        } else {
            PlaneData::U16(v)
        };
    };
    store(&mut frame.planes[0], y);
    if format.plane_count() == 3 {
        store(&mut frame.planes[1], cb);
        store(&mut frame.planes[2], cr);
    }
    frame
}

/// The linear image as 4:2:0 for the video encoders, in BT.709 colors and
/// limited range: NV12, or with `ten_bit` P010 (each sample in the high 10
/// bits of a little-endian 16-bit word) — the luma rows, then the rows of
/// Cb and Cr side by side. The same values as [`linear_to_frame`] (chroma
/// filtered [1 2 1] across the even columns, averaged over its two rows),
/// made on all the processor's threads, laid out as
/// [`VideoFrame::packed_420`]. `img` must be opaque, its width and height
/// even.
pub fn linear_to_420(img: &ImageRgbaF32, ten_bit: bool) -> Vec<u8> {
    linear_to_420_levels(img, ten_bit, false)
}

/// As [`linear_to_420`]; with `video_levels` the picture is studio RGB and
/// goes out as it is (its 16–235 as Y′ 16–235, not squeezed again), as
/// VEGAS's video levels pixel formats write it.
pub fn linear_to_420_levels(img: &ImageRgbaF32, ten_bit: bool, video_levels: bool) -> Vec<u8> {
    let (w, h) = (img.width as usize, img.height as usize);
    let bytes = if ten_bit { 2 } else { 1 };
    let depth = if ten_bit { 10 } else { 8 };
    let max = ((1u32 << depth) - 1) as f32;
    let range = if video_levels {
        bv_core::ColorRange::Full
    } else {
        bv_core::ColorRange::Limited
    };
    let n = Normalizer::new(range, depth);
    let (kr, kb) = matrix::kr_kb(bv_core::MatrixCoefficients::Bt709).unwrap_or((0.2126, 0.0722));
    let lut = EncodeLut::shared(TransferCharacteristic::Bt709);
    let ycc = |px: [f32; 4]| -> [f32; 3] {
        let rgb = [px[0], px[1], px[2]].map(|c| lut.eval(c).clamp(0.0, 1.0));
        let [y, cb, cr] = matrix::rgb_to_ycbcr(kr, kb, rgb);
        [n.luma_inv(y), n.chroma_inv(cb), n.chroma_inv(cr)]
    };
    let put = |buf: &mut [u8], i: usize, v: f32| {
        let q = v.round().clamp(0.0, max) as u16;
        if ten_bit {
            buf[2 * i..2 * i + 2].copy_from_slice(&(q << 6).to_le_bytes());
        } else {
            buf[i] = q as u8;
        }
    };
    let (cw, ch) = (w / 2, h / 2);
    let mut out = vec![0u8; (w * h + 2 * cw * ch) * bytes];
    if cw == 0 || ch == 0 {
        return out;
    }
    let (luma, chroma) = out.split_at_mut(w * h * bytes);
    // Two luma rows and their chroma row at a time, the pairs shared out
    // among the threads.
    let workers = threads().min(ch).max(1);
    let per = ch.div_ceil(workers);
    std::thread::scope(|s| {
        let luma_chunks = luma.chunks_mut(per * 2 * w * bytes);
        let chroma_chunks = chroma.chunks_mut(per * 2 * cw * bytes);
        for (k, (lc, cc)) in luma_chunks.zip(chroma_chunks).enumerate() {
            let (ycc, put) = (&ycc, &put);
            s.spawn(move || {
                let mut rows = [vec![[0.0f32; 3]; w], vec![[0.0f32; 3]; w]];
                let pairs = lc
                    .chunks_mut(2 * w * bytes)
                    .zip(cc.chunks_mut(2 * cw * bytes));
                for (p, (l2, c1)) in pairs.enumerate() {
                    let j = k * per + p;
                    for (r, row) in rows.iter_mut().enumerate() {
                        let y = 2 * j + r;
                        for (x, v) in row.iter_mut().enumerate() {
                            *v = ycc(img.data[y * w + x]);
                            put(l2, r * w + x, v[0]);
                        }
                    }
                    for i in 0..cw {
                        let c = 2 * i;
                        let h121 = |row: &[[f32; 3]], k: usize| {
                            0.25 * row[c.saturating_sub(1)][k]
                                + 0.5 * row[c][k]
                                + 0.25 * row[(c + 1).min(w - 1)][k]
                        };
                        let cb = (h121(&rows[0], 1) + h121(&rows[1], 1)) / 2.0;
                        let cr = (h121(&rows[0], 2) + h121(&rows[1], 2)) / 2.0;
                        put(c1, 2 * i, cb);
                        put(c1, 2 * i + 1, cr);
                    }
                }
            });
        }
    });
    out
}

/// Converte a imagem linear para RGBA 8 bits para exibição no monitor,
/// codificando com a curva `transfer` (a mesma da exportação, para que o
/// monitor mostre o que vai sair no arquivo).
pub fn linear_to_display_rgba8(img: &ImageRgbaF32, transfer: TransferCharacteristic) -> Vec<u8> {
    let lut = EncodeLut::shared(transfer);
    let w = img.width as usize;
    let mut out = vec![0u8; img.data.len() * 4];
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    par_rows(&mut out, w * 4, |y, row| {
        let src = &img.data[y * w..(y + 1) * w];
        for (px, o) in src.iter().zip(row.as_chunks_mut::<4>().0) {
            *o = [
                q(lut.eval(px[0])),
                q(lut.eval(px[1])),
                q(lut.eval(px[2])),
                q(px[3]),
            ];
        }
    });
    out
}

/// [`linear_to_display_rgba8`] with straight alpha, as picture files keep
/// it: each color divided by its alpha before it is encoded (none where
/// there is no alpha); opaque pixels come out the same.
pub fn linear_to_display_rgba8_straight(
    img: &ImageRgbaF32,
    transfer: TransferCharacteristic,
) -> Vec<u8> {
    let lut = EncodeLut::shared(transfer);
    let w = img.width as usize;
    let mut out = vec![0u8; img.data.len() * 4];
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    par_rows(&mut out, w * 4, |y, row| {
        let src = &img.data[y * w..(y + 1) * w];
        for (px, o) in src.iter().zip(row.as_chunks_mut::<4>().0) {
            let a = px[3].clamp(0.0, 1.0);
            let k = match a {
                a if a >= 1.0 => 1.0,
                a if a > 0.0 => 1.0 / a,
                _ => 0.0,
            };
            *o = [
                q(lut.eval(px[0] * k)),
                q(lut.eval(px[1] * k)),
                q(lut.eval(px[2] * k)),
                q(a),
            ];
        }
    });
    out
}

#[cfg(test)]
mod levels_tests {
    use super::*;

    #[test]
    fn video_levels_keep_studio_rgb_both_ways() {
        // A gray of Y′ 16 and one of 235, limited range.
        let color = ColorInfo::default();
        let mut f = VideoFrame::black(4, 2, PixelFormat::Yuv420P8, color);
        if let PlaneData::U8(y) = &mut f.planes[0].data {
            for (i, v) in y.iter_mut().enumerate() {
                *v = if i % 4 < 2 { 16 } else { 235 };
            }
        }
        let encoded = |img: &ImageRgbaF32, x: u32| {
            let lut = EncodeLut::shared(TransferCharacteristic::Bt709);
            lut.eval(img.pixel(x, 0)[0])
        };
        // Full range: black and white.
        let full = frame_to_linear_levels(&f, 1, false);
        assert!(encoded(&full, 0) < 1e-3 && encoded(&full, 3) > 0.999);
        // Video levels: studio RGB.
        let studio = frame_to_linear_levels(&f, 1, true);
        assert!((encoded(&studio, 0) - 16.0 / 255.0).abs() < 2e-3);
        assert!((encoded(&studio, 3) - 235.0 / 255.0).abs() < 2e-3);
        // Back as they were, not squeezed again.
        let packed = linear_to_420_levels(&studio, false, true);
        assert_eq!((packed[0], packed[3]), (16, 235));
        let frame = linear_to_frame_levels(&studio, PixelFormat::Yuv420P8, color, true);
        let PlaneData::U8(y) = &frame.planes[0].data else {
            unreachable!()
        };
        assert_eq!((y[0], y[3]), (16, 235));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_core::{ColorRange, MatrixCoefficients};

    #[test]
    fn packed_420_matches_the_planes() {
        // A gradient with a bright corner: values all over both ranges.
        let (w, h) = (38, 22);
        let mut img = ImageRgbaF32::transparent(w, h);
        for y in 0..h {
            for x in 0..w {
                let (u, v) = (x as f32 / w as f32, y as f32 / h as f32);
                *img.pixel_mut(x, y) = [u * 1.2, v, (u * v).sqrt(), 1.0];
            }
        }
        for (ten_bit, format) in [
            (false, PixelFormat::Yuv420P8),
            (true, PixelFormat::Yuv420P10),
        ] {
            let planes = linear_to_frame(&img, format, ColorInfo::default());
            assert_eq!(
                linear_to_420(&img, ten_bit),
                planes.packed_420(ten_bit),
                "{format:?}"
            );
        }
    }

    fn gradient(w: u32, h: u32) -> ImageRgbaF32 {
        let mut img = ImageRgbaF32::transparent(w, h);
        for y in 0..h {
            for x in 0..w {
                // Longe do preto: lá a OETF é íngreme e a subamostragem de
                // croma erra muito por natureza (luma não constante).
                let r = x as f32 / (w - 1) as f32;
                let g = y as f32 / (h - 1) as f32;
                *img.pixel_mut(x, y) = [0.1 + 0.8 * r * r, 0.2 + 0.6 * g, 0.25, 1.0];
            }
        }
        img
    }

    fn max_error(a: &ImageRgbaF32, b: &ImageRgbaF32) -> f32 {
        a.data
            .iter()
            .zip(&b.data)
            .flat_map(|(p, q)| (0..3).map(move |c| (p[c] - q[c]).abs()))
            .fold(0.0, f32::max)
    }

    #[test]
    fn ida_e_volta_444_10_bits_e_quase_exata() {
        let img = gradient(64, 32);
        let color = ColorInfo::default();
        let f = linear_to_frame(&img, PixelFormat::Yuv444P10, color);
        let back = frame_to_linear(&f);
        assert!(max_error(&img, &back) < 0.01, "{}", max_error(&img, &back));
    }

    #[test]
    fn ida_e_volta_420_8_bits_em_gradiente_suave() {
        let img = gradient(64, 32);
        for color in [
            ColorInfo::default(),
            ColorInfo::guess_for(720, 576),
            ColorInfo {
                range: ColorRange::Full,
                ..ColorInfo::default()
            },
        ] {
            let f = linear_to_frame(&img, PixelFormat::Yuv420P8, color);
            let back = frame_to_linear(&f);
            assert!(
                max_error(&img, &back) < 0.04,
                "{color:?}: {}",
                max_error(&img, &back)
            );
        }
    }

    #[test]
    fn branco_e_preto_em_faixa_limitada() {
        let white = ImageRgbaF32::filled(4, 4, [1.0, 1.0, 1.0, 1.0]);
        let f = linear_to_frame(&white, PixelFormat::Yuv420P8, ColorInfo::default());
        assert_eq!(f.planes[0].sample(0, 0), 235);
        assert_eq!(f.planes[1].sample(0, 0), 128);
        let black = ImageRgbaF32::filled(4, 4, [0.0, 0.0, 0.0, 1.0]);
        let f = linear_to_frame(&black, PixelFormat::Yuv420P8, ColorInfo::default());
        assert_eq!(f.planes[0].sample(0, 0), 16);
    }

    #[test]
    fn gbr_identidade() {
        let img = gradient(8, 8);
        let color = ColorInfo {
            matrix: MatrixCoefficients::Identity,
            range: ColorRange::Full,
            ..ColorInfo::default()
        };
        let f = linear_to_frame(&img, PixelFormat::Yuv444P10, color);
        assert!(max_error(&img, &frame_to_linear(&f)) < 0.01);
    }

    #[test]
    fn rgba8_ida_e_volta() {
        let img = gradient(16, 16);
        let f = linear_to_frame(&img, PixelFormat::Rgba8, ColorInfo::SRGB);
        assert!(max_error(&img, &frame_to_linear(&f)) < 0.01);
    }

    #[test]
    fn conversao_reduzida_preserva_cor_media() {
        let img = ImageRgbaF32::filled(64, 32, [0.3, 0.5, 0.1, 1.0]);
        let f = linear_to_frame(&img, PixelFormat::Yuv420P8, ColorInfo::default());
        let full = frame_to_linear(&f);
        let half = frame_to_linear_scaled(&f, 2);
        let quarter = frame_to_linear_scaled(&f, 4);
        assert_eq!((half.width, half.height), (32, 16));
        assert_eq!((quarter.width, quarter.height), (16, 8));
        for c in 0..3 {
            assert!((half.pixel(5, 5)[c] - full.pixel(10, 10)[c]).abs() < 1e-3);
            assert!((quarter.pixel(3, 3)[c] - full.pixel(12, 12)[c]).abs() < 1e-3);
        }
    }

    #[test]
    fn exibicao() {
        let half = crate::transfer::srgb_to_linear(128.0 / 255.0);
        let img = ImageRgbaF32::filled(1, 1, [1.0, half, 0.0, 1.0]);
        let px = linear_to_display_rgba8(&img, TransferCharacteristic::Srgb);
        assert_eq!(px, vec![255, 128, 0, 255]);
    }
}
