//! Coloca a imagem de um clipe no quadro: encaixe (letterbox), pan/crop
//! (posição, escala, rotação, corte) e amostragem bilinear.

use bv_core::ImageRgbaF32;
use bv_timeline::scene::fit_rect_par;
use bv_timeline::{RenderQuality, Transform};

use crate::par;

/// Reduz pela metade com média 2×2 (evita serrilhado em reduções grandes).
/// Reduce Interlace Flicker (an event's switch): each line half itself
/// and a quarter of each line next to it (the edge lines held), which
/// keeps fine horizontal detail from flickering between the fields of
/// interlaced video. The card's `flicker_filter` kernel does the same.
pub(crate) fn flicker_filter(img: &mut ImageRgbaF32) {
    let (w, h) = (img.width as usize, img.height as usize);
    if h < 2 {
        return;
    }
    let src = img.data.clone();
    for y in 0..h {
        let (up, down) = (y.saturating_sub(1), (y + 1).min(h - 1));
        for x in 0..w {
            let (a, b, c) = (src[up * w + x], src[y * w + x], src[down * w + x]);
            img.data[y * w + x] = std::array::from_fn(|k| 0.25 * a[k] + 0.5 * b[k] + 0.25 * c[k]);
        }
    }
}

/// Interpolate (the Deinterlace method): the lines of the field kept
/// (`keep`: 0 the upper one — the even lines —, 1 the lower) as they are,
/// the others the mean of the lines above and below them (at the edges,
/// the one there is).
pub(crate) fn interpolate_fields(img: &mut ImageRgbaF32, keep: u32) {
    let (w, h) = (img.width as usize, img.height as usize);
    if h < 2 {
        return;
    }
    let src = img.data.clone();
    for y in (0..h).filter(|y| (*y as u32) % 2 != keep) {
        let up = if y == 0 { 1 } else { y - 1 };
        let down = if y + 1 >= h { y - 1 } else { y + 1 };
        for x in 0..w {
            let (a, c) = (src[up * w + x], src[down * w + x]);
            img.data[y * w + x] = std::array::from_fn(|k| 0.5 * a[k] + 0.5 * c[k]);
        }
    }
}

/// Smart adaptive (the Deinterlace method): the field kept (`keep`) as it
/// is, each line of the other moved toward the mean of the lines above and
/// below it by how far its luma stands out from theirs
/// (`bv_timeline::SMART_THRESHOLDS`).
pub(crate) fn smart_fields(img: &mut ImageRgbaF32, keep: u32) {
    let (w, h) = (img.width as usize, img.height as usize);
    if h < 2 {
        return;
    }
    let [low, high] = bv_timeline::SMART_THRESHOLDS;
    let luma = |p: [f32; 4]| 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2];
    let src = img.data.clone();
    for y in (0..h).filter(|y| (*y as u32) % 2 != keep) {
        let up = if y == 0 { 1 } else { y - 1 };
        let down = if y + 1 >= h { y - 1 } else { y + 1 };
        for x in 0..w {
            let (a, b, c) = (src[up * w + x], src[y * w + x], src[down * w + x]);
            let mean: [f32; 4] = std::array::from_fn(|k| 0.5 * a[k] + 0.5 * c[k]);
            let d = (luma(b) - luma(mean)).abs();
            let m = ((d - low) / (high - low)).clamp(0.0, 1.0);
            img.data[y * w + x] = std::array::from_fn(|k| b[k] + (mean[k] - b[k]) * m);
        }
    }
}

/// A picture of a media deinterlaced as `how` says (see
/// `MediaLayer::deinterlaced`): Blend Fields as Reduce Interlace
/// Flicker's filter, Interpolate and Smart adaptive keeping the field
/// shown first.
pub(crate) fn deinterlace(
    img: &mut ImageRgbaF32,
    how: (bv_timeline::DeinterlaceMethod, bv_core::FieldOrder),
) {
    match how.0 {
        bv_timeline::DeinterlaceMethod::None => {}
        bv_timeline::DeinterlaceMethod::Interpolate => {
            let keep = u32::from(how.1 == bv_core::FieldOrder::LowerFirst);
            interpolate_fields(img, keep);
        }
        bv_timeline::DeinterlaceMethod::SmartAdaptive => {
            let keep = u32::from(how.1 == bv_core::FieldOrder::LowerFirst);
            smart_fields(img, keep);
        }
        _ => flicker_filter(img),
    }
}

#[cfg(test)]
mod smart_tests {
    use super::*;

    /// Smart adaptive keeps a line of the other field that matches the
    /// lines around it (still detail) and replaces one that stands out
    /// (the comb of motion).
    #[test]
    fn smart_adaptive_keeps_detail_and_removes_combs() {
        let rows = |v: [f32; 4]| -> ImageRgbaF32 {
            let mut img = ImageRgbaF32::transparent(1, 4);
            for (y, &g) in v.iter().enumerate() {
                img.data[y] = [g, g, g, 1.0];
            }
            img
        };
        // Row 1 a little off its neighbours: kept as it is.
        let mut a = rows([0.5, 0.52, 0.5, 0.5]);
        smart_fields(&mut a, 0);
        assert!((a.data[1][0] - 0.52).abs() < 1e-6);
        // Row 1 far off (a comb): the mean of rows 0 and 2.
        let mut b = rows([0.2, 0.9, 0.2, 0.9]);
        smart_fields(&mut b, 0);
        assert!((b.data[1][0] - 0.2).abs() < 1e-6, "{:?}", b.data[1]);
        // The field kept stays.
        assert_eq!(b.data[0][0], 0.2);
    }
}

pub(crate) fn half_size(src: &ImageRgbaF32) -> ImageRgbaF32 {
    let w = (src.width / 2).max(1);
    let h = (src.height / 2).max(1);
    let mut out = ImageRgbaF32::transparent(w, h);
    par::for_each_pixel(&mut out.data, w as usize, |x, y, px| {
        let (x, y) = (x as u32, y as u32);
        let xs = [(2 * x).min(src.width - 1), (2 * x + 1).min(src.width - 1)];
        let ys = [(2 * y).min(src.height - 1), (2 * y + 1).min(src.height - 1)];
        let mut acc = [0.0f32; 4];
        for &sy in &ys {
            for &sx in &xs {
                let p = src.pixel(sx, sy);
                for c in 0..4 {
                    acc[c] += p[c] * 0.25;
                }
            }
        }
        *px = acc;
    });
    out
}

/// Source pixel `(x, y)`; outside the image (or the crop) it is
/// transparent, which softens the edges of rotated or reduced images.
#[inline]
fn texel(src: &ImageRgbaF32, x: i64, y: i64, crop: [f32; 4]) -> [f32; 4] {
    let [cx0, cy0, cx1, cy1] = crop;
    let (xf, yf) = (x as f32 + 0.5, y as f32 + 0.5);
    if x < 0
        || y < 0
        || x >= src.width as i64
        || y >= src.height as i64
        || xf < cx0
        || xf > cx1
        || yf < cy0
        || yf > cy1
    {
        [0.0; 4]
    } else {
        src.data[(y * src.width as i64 + x) as usize]
    }
}

/// Samples `src` at `(u, v)` (pixel centers at integers) with the filter of
/// the rendering quality.
#[inline]
fn sample_with(
    src: &ImageRgbaF32,
    u: f32,
    v: f32,
    crop: [f32; 4],
    quality: RenderQuality,
) -> [f32; 4] {
    match quality {
        RenderQuality::Draft => texel(src, u.round() as i64, v.round() as i64, crop),
        RenderQuality::Preview | RenderQuality::Good => sample(src, u, v, crop),
        RenderQuality::Best => sample_bicubic(src, u, v, crop),
    }
}

/// Catmull-Rom weights of the 4 taps around a sample at fraction `t`.
#[inline]
fn catmull_rom(t: f32) -> [f32; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    [
        0.5 * (-t3 + 2.0 * t2 - t),
        0.5 * (3.0 * t3 - 5.0 * t2 + 2.0),
        0.5 * (-3.0 * t3 + 4.0 * t2 + t),
        0.5 * (t3 - t2),
    ]
}

/// Bicubic (Catmull-Rom) sample; sharper than bilinear when enlarging.
#[inline]
fn sample_bicubic(src: &ImageRgbaF32, u: f32, v: f32, crop: [f32; 4]) -> [f32; 4] {
    let (x0, y0) = (u.floor(), v.floor());
    let wx = catmull_rom(u - x0);
    let wy = catmull_rom(v - y0);
    let (x0, y0) = (x0 as i64, y0 as i64);
    let mut out = [0.0f32; 4];
    for (j, ky) in wy.iter().enumerate() {
        for (i, kx) in wx.iter().enumerate() {
            let p = texel(src, x0 - 1 + i as i64, y0 - 1 + j as i64, crop);
            let k = kx * ky;
            for c in 0..4 {
                out[c] += p[c] * k;
            }
        }
    }
    // The negative lobes may overshoot: keep a valid premultiplied pixel.
    out[3] = out[3].clamp(0.0, 1.0);
    for c in &mut out[..3] {
        *c = c.max(0.0);
    }
    out
}

/// Amostra bilinear; fora da imagem (ou do corte) é transparente, o que
/// suaviza as bordas de imagens giradas ou reduzidas.
#[inline]
fn sample(src: &ImageRgbaF32, u: f32, v: f32, crop: [f32; 4]) -> [f32; 4] {
    let x0 = u.floor();
    let y0 = v.floor();
    let fx = u - x0;
    let fy = v - y0;
    let (x0, y0) = (x0 as i64, y0 as i64);
    let tex = |x: i64, y: i64| texel(src, x, y, crop);
    let a = tex(x0, y0);
    let b = tex(x0 + 1, y0);
    let c = tex(x0, y0 + 1);
    let d = tex(x0 + 1, y0 + 1);
    let mut out = [0.0f32; 4];
    for i in 0..4 {
        let top = a[i] + (b[i] - a[i]) * fx;
        let bottom = c[i] + (d[i] - c[i]) * fx;
        out[i] = top + (bottom - top) * fy;
    }
    out
}

/// Desenha `src` numa camada transparente `w` × `h`, encaixada e com o
/// enquadramento `tf` aplicado.
pub fn place(src: &ImageRgbaF32, w: u32, h: u32, tf: &Transform) -> ImageRgbaF32 {
    place_with(src, w, h, tf, RenderQuality::Good)
}

/// [`place`] with the resampling of a rendering quality.
pub fn place_with(
    src: &ImageRgbaF32,
    w: u32,
    h: u32,
    tf: &Transform,
    quality: RenderQuality,
) -> ImageRgbaF32 {
    place_par(src, w, h, tf, quality, 1.0)
}

/// [`place_with`] for a picture whose pixels are `par` times as wide as
/// the frame's.
pub fn place_par(
    src: &ImageRgbaF32,
    w: u32,
    h: u32,
    tf: &Transform,
    quality: RenderQuality,
    par: f32,
) -> ImageRgbaF32 {
    let mut out = ImageRgbaF32::transparent(w, h);
    let fit = fit_rect_par(src.width, src.height, par, w, h);
    let scale = tf.scale.max(1e-4);

    // Caminho rápido: sem enquadramento e no mesmo tamanho.
    if tf.is_identity() && fit.w as u32 == src.width && fit.h as u32 == src.height {
        let (x0, y0) = (fit.x as usize, fit.y as usize);
        let sw = src.width as usize;
        par::for_each_row(&mut out.data, w as usize, |y, row| {
            if y >= y0 && y < y0 + src.height as usize {
                let s = &src.data[(y - y0) * sw..][..sw];
                row[x0..x0 + sw].copy_from_slice(s);
            }
        });
        return out;
    }

    // Reduções grandes passam antes por médias 2×2 (Good and Best).
    let mut reduced;
    let mut src = src;
    let stretch = tf.stretch.max(1e-3);
    let prefilter = matches!(quality, RenderQuality::Good | RenderQuality::Best);
    while prefilter
        && (src.width as f32) >= fit.w * scale * 2.0
        && (src.height as f32) >= fit.h * scale * stretch * 2.0
        && src.width > 1
    {
        reduced = half_size(src);
        src = &reduced;
    }

    let (sw, sh) = (src.width as f32, src.height as f32);
    let crop = [
        tf.crop_left.clamp(0.0, 1.0) * sw,
        tf.crop_top.clamp(0.0, 1.0) * sh,
        (1.0 - tf.crop_right.clamp(0.0, 1.0)) * sw,
        (1.0 - tf.crop_bottom.clamp(0.0, 1.0)) * sh,
    ];
    let center_x = w as f32 / 2.0 + tf.x * w as f32;
    let center_y = h as f32 / 2.0 - tf.y * h as f32;
    let (sin, cos) = (-tf.rotation_deg.to_radians()).sin_cos();
    let to_src_x = sw / (fit.w * scale);
    let to_src_y = sh / (fit.h * scale * stretch);
    let (fx, fy) = (
        if tf.flip_h { -1.0 } else { 1.0 },
        if tf.flip_v { -1.0 } else { 1.0 },
    );

    par::for_each_pixel(&mut out.data, w as usize, |x, y, px| {
        let dx = x as f32 + 0.5 - center_x;
        let dy = y as f32 + 0.5 - center_y;
        // Desfaz a rotação.
        let rx = dx * cos - dy * sin;
        let ry = dx * sin + dy * cos;
        let u = fx * rx * to_src_x + sw / 2.0 - 0.5;
        let v = fy * ry * to_src_y + sh / 2.0 - 0.5;
        if u < -1.0 || v < -1.0 || u > sw || v > sh {
            return;
        }
        *px = sample_with(src, u, v, crop, quality);
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolating_keeps_one_field_and_fills_the_other() {
        // Even lines 1, odd lines 0: the upper field kept fills the odd
        // lines with 1; the lower kept fills the even ones with 0 (the top
        // line with the one below it).
        let striped = || {
            let mut img = ImageRgbaF32::transparent(1, 5);
            for y in (0..5).step_by(2) {
                *img.pixel_mut(0, y) = [1.0; 4];
            }
            img
        };
        let mut upper = striped();
        interpolate_fields(&mut upper, 0);
        assert!((0..5).all(|y| upper.pixel(0, y)[0] == 1.0));
        let mut lower = striped();
        interpolate_fields(&mut lower, 1);
        assert!((0..5).all(|y| lower.pixel(0, y)[0] == 0.0));
    }

    #[test]
    fn flicker_filter_spreads_a_line_over_its_neighbours() {
        let mut img = ImageRgbaF32::transparent(2, 5);
        for x in 0..2 {
            *img.pixel_mut(x, 2) = [1.0; 4];
        }
        flicker_filter(&mut img);
        let column: Vec<f32> = (0..5).map(|y| img.pixel(0, y)[0]).collect();
        assert_eq!(column, vec![0.0, 0.25, 0.5, 0.25, 0.0]);
        // The edges held: a flat picture stays as it was.
        let mut flat = ImageRgbaF32::filled(3, 3, [0.5; 4]);
        flicker_filter(&mut flat);
        assert!(flat.data.iter().all(|p| *p == [0.5; 4]));
    }
    use bv_timeline::scene::fit_rect;

    #[test]
    fn imagem_uniforme_continua_uniforme_em_qualquer_escala() {
        let src = ImageRgbaF32::filled(37, 23, [0.5, 0.25, 0.125, 1.0]);
        for (w, h) in [(37, 23), (100, 60), (9, 5), (3, 2)] {
            let out = place(&src, w, h, &Transform::default());
            let r = fit_rect(37, 23, w, h);
            let p = out.pixel((r.x + r.w / 2.0) as u32, (r.y + r.h / 2.0) as u32);
            for c in 0..4 {
                assert!((p[c] - src.data[0][c]).abs() < 1e-5, "{w}x{h}: {p:?}");
            }
        }
    }

    #[test]
    fn reducao_media_preserva_energia() {
        let mut src = ImageRgbaF32::transparent(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                let v = (y % 2) as f32;
                *src.pixel_mut(x, y) = [v, v, v, 1.0];
            }
        }
        let out = place(&src, 16, 16, &Transform::default());
        for px in &out.data {
            assert!((px[0] - 0.5).abs() < 1e-4, "{px:?}");
        }
    }

    #[test]
    fn posicao_escala_e_corte() {
        let src = ImageRgbaF32::filled(100, 100, [1.0, 1.0, 1.0, 1.0]);
        // Metade do tamanho, deslocada para a direita: o centro fica vazio
        // à esquerda e cheio em x = 75.
        let tf = Transform {
            scale: 0.5,
            x: 0.25,
            ..Transform::default()
        };
        let out = place(&src, 100, 100, &tf);
        assert_eq!(out.pixel(20, 50)[3], 0.0);
        assert!((out.pixel(75, 50)[3] - 1.0).abs() < 1e-5);
        // Corte de 50% à direita.
        let tf = Transform {
            crop_right: 0.5,
            ..Transform::default()
        };
        let out = place(&src, 100, 100, &tf);
        assert!((out.pixel(25, 50)[3] - 1.0).abs() < 1e-5);
        assert_eq!(out.pixel(75, 50)[3], 0.0);
    }

    #[test]
    fn espelhar_e_esticar() {
        let mut src = ImageRgbaF32::filled(10, 10, [0.0, 0.0, 0.0, 1.0]);
        *src.pixel_mut(0, 5) = [1.0, 0.0, 0.0, 1.0];
        let tf = Transform {
            flip_h: true,
            ..Transform::default()
        };
        let out = place(&src, 10, 10, &tf);
        assert!(out.pixel(9, 5)[0] > 0.99 && out.pixel(0, 5)[0] < 0.01);
        // Esticar 0,5 na vertical: a imagem ocupa metade da altura.
        let white = ImageRgbaF32::filled(10, 10, [1.0, 1.0, 1.0, 1.0]);
        let tf = Transform {
            stretch: 0.5,
            ..Transform::default()
        };
        let out = place(&white, 10, 10, &tf);
        assert!(out.pixel(5, 1)[3] < 0.01 && out.pixel(5, 5)[3] > 0.99);
    }

    #[test]
    fn rendering_qualities() {
        // A hard edge enlarged 4×: Draft keeps it hard, Good blends it, Best
        // blends it with a sharper (overshooting) curve.
        let mut src = ImageRgbaF32::filled(4, 4, [0.0, 0.0, 0.0, 1.0]);
        for y in 0..4 {
            for x in 2..4 {
                *src.pixel_mut(x, y) = [1.0, 1.0, 1.0, 1.0];
            }
        }
        let tf = Transform::default();
        let at = |q| place_with(&src, 16, 16, &tf, q).pixel(7, 8)[0];
        let (draft, good, best) = (
            at(RenderQuality::Draft),
            at(RenderQuality::Good),
            at(RenderQuality::Best),
        );
        assert!(draft == 0.0 || draft == 1.0, "{draft}");
        assert!(good > 0.05 && good < 0.95, "{good}");
        assert!(best > 0.0 && best < good, "{best} vs {good}");
        // A uniform image stays uniform with every filter.
        let flat = ImageRgbaF32::filled(8, 8, [0.5, 0.5, 0.5, 1.0]);
        for q in RenderQuality::ALL {
            let p = place_with(&flat, 20, 20, &tf, q).pixel(10, 10);
            assert!((p[0] - 0.5).abs() < 1e-4, "{q:?}: {p:?}");
        }
    }

    #[test]
    fn rotacao_de_180_inverte() {
        let mut src = ImageRgbaF32::filled(10, 10, [0.0, 0.0, 0.0, 1.0]);
        *src.pixel_mut(0, 0) = [1.0, 0.0, 0.0, 1.0];
        let tf = Transform {
            rotation_deg: 180.0,
            ..Transform::default()
        };
        let out = place(&src, 10, 10, &tf);
        assert!(out.pixel(9, 9)[0] > 0.99, "{:?}", out.pixel(9, 9));
        assert!(out.pixel(0, 0)[0] < 0.01);
    }
}
