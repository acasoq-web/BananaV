//! Efeitos de vídeo em CPU, sobre RGB linear pré-multiplicado.
//!
//! Ajustes "de gosto" (brilho, contraste, níveis, lift/gamma/gain) operam num
//! espaço perceptual (potência 1/2,2), onde 0,5 é o cinza médio que o olho
//! espera; exposição e balanço de branco operam em luz linear, onde são
//! fisicamente corretos. Parâmetros em pixels referem-se à resolução do
//! projeto e são escalados quando a prévia renderiza em resolução menor.

use bv_core::ImageRgbaF32;
use bv_timeline::fx::grade;
use bv_timeline::scene::{FrameScene, PluginCall};
use bv_timeline::{Effect, EffectKind, TrackId};

use crate::par;

const GAMMA: f32 = 2.2;

#[inline]
fn to_perceptual(l: f32) -> f32 {
    if l <= 0.0 { 0.0 } else { l.powf(1.0 / GAMMA) }
}

#[inline]
fn to_linear(v: f32) -> f32 {
    if v <= 0.0 { 0.0 } else { v.powf(GAMMA) }
}

#[inline]
fn luma(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

/// Aplica `f` à cor não pré-multiplicada de cada pixel (o alfa é preservado).
fn map_color(img: &mut ImageRgbaF32, f: impl Fn([f32; 3]) -> [f32; 3] + Sync) {
    let w = img.width as usize;
    par::for_each_pixel(&mut img.data, w, |_, _, px| {
        let a = px[3];
        if a <= 0.0 {
            return;
        }
        let c = f([px[0] / a, px[1] / a, px[2] / a]);
        *px = [c[0] * a, c[1] * a, c[2] * a, a];
    });
}

/// Como [`map_color`], mas no espaço perceptual.
fn map_perceptual(img: &mut ImageRgbaF32, f: impl Fn([f32; 3]) -> [f32; 3] + Sync) {
    map_color(img, |c| {
        let v = f(c.map(to_perceptual));
        v.map(to_linear)
    });
}

/// Applies a track's effects (`track`) or the output's (`None`), in order.
/// `scale`: the rendered height over the project's. Plug-in effects go to
/// the scene's plug-ins.
pub fn apply_chain(
    scene: &FrameScene<'_>,
    img: &mut ImageRgbaF32,
    track: Option<TrackId>,
    effects: &[Effect],
    scale: f32,
) {
    apply_chain_from(scene, img, track, effects, 0, scale);
}

/// As [`apply_chain`], the effects from the `from`th on (their indices
/// the chain's, which plug-ins' instances go by).
pub fn apply_chain_from(
    scene: &FrameScene<'_>,
    img: &mut ImageRgbaF32,
    track: Option<TrackId>,
    effects: &[Effect],
    from: usize,
    scale: f32,
) {
    // The picture the chain starts from, for the plug-ins that take it
    // (Bézier Masking's Mask FX).
    let takes = |i: usize, e: &Effect| {
        scene.plugins.is_some_and(|p| {
            scene
                .frame_call(track, i, e)
                .is_some_and(|c| p.0.takes_chain_input(&c))
        })
    };
    let chain_input = effects
        .iter()
        .enumerate()
        .skip(from)
        .any(|(i, e)| e.enabled && takes(i, e))
        .then(|| std::sync::Arc::new(img.clone()));
    for (i, e) in effects
        .iter()
        .enumerate()
        .skip(from)
        .filter(|(_, e)| e.enabled)
    {
        match scene.frame_call(track, i, e) {
            Some(call) => match &chain_input {
                Some(start) if takes(i, e) => plugin_in_chain(scene, &call, img, start),
                _ => plugin(scene, &call, img),
            },
            None => apply(img, e, scale),
        }
        stored(scene.clips, img);
    }
}

/// What an effect left, as the project's pixel format keeps it: with
/// `clips` (8-bit formats) the alpha and the straight colors within 0–1,
/// as VEGAS's 8-bit pictures hold them — its effects in 32-bit projects
/// keep what passes white or black (the calibration card through ACES's
/// view shows it).
pub fn stored(clips: bool, img: &mut ImageRgbaF32) {
    if !clips {
        return;
    }
    let w = img.width as usize;
    par::for_each_pixel(&mut img.data, w, |_, _, px| {
        let a = px[3].clamp(0.0, 1.0);
        *px = [
            px[0].clamp(0.0, a),
            px[1].clamp(0.0, a),
            px[2].clamp(0.0, a),
            a,
        ];
    });
}

/// A plug-in effect on `img`, when the scene has someone to draw it.
pub fn plugin(scene: &FrameScene<'_>, call: &PluginCall<'_>, img: &mut ImageRgbaF32) {
    let Some(plugins) = scene.plugins else {
        return;
    };
    let input = std::sync::Arc::new(std::mem::replace(img, ImageRgbaF32::transparent(0, 0)));
    *img = match plugins.0.apply(call, std::sync::Arc::clone(&input)) {
        Some(out) => out,
        None => std::sync::Arc::try_unwrap(input).unwrap_or_else(|kept| (*kept).clone()),
    };
}

/// [`plugin`] for an effect that takes the picture its chain started
/// from, `start`.
fn plugin_in_chain(
    scene: &FrameScene<'_>,
    call: &PluginCall<'_>,
    img: &mut ImageRgbaF32,
    start: &std::sync::Arc<ImageRgbaF32>,
) {
    let Some(plugins) = scene.plugins else {
        return;
    };
    let input = std::sync::Arc::new(std::mem::replace(img, ImageRgbaF32::transparent(0, 0)));
    *img = match plugins.0.apply_in_chain(
        call,
        std::sync::Arc::clone(&input),
        std::sync::Arc::clone(start),
    ) {
        Some(out) => out,
        None => std::sync::Arc::try_unwrap(input).unwrap_or_else(|kept| (*kept).clone()),
    };
}

pub fn apply(img: &mut ImageRgbaF32, e: &Effect, scale: f32) {
    let p = |i: usize| e.param(i);
    match e.kind {
        EffectKind::BrightnessContrast => {
            let (b, c) = (p(0), p(1) + 1.0);
            map_perceptual(img, |v| v.map(|x| (x - 0.5) * c + 0.5 + b));
        }
        EffectKind::ColorGrade => color_grade(img, e),
        EffectKind::Levels => {
            let (ib, iw, g, ob, ow) = (p(0), p(1), p(2), p(3), p(4));
            let range = (iw - ib).max(1e-4);
            map_perceptual(img, |v| {
                v.map(|x| ((x - ib) / range).clamp(0.0, 1.0).powf(1.0 / g) * (ow - ob) + ob)
            });
        }
        EffectKind::BlackAndWhite => {
            let k = p(0);
            map_color(img, |c| {
                let y = luma(c);
                c.map(|x| x + (y - x) * k)
            });
        }
        EffectKind::Sepia => {
            let k = p(0);
            map_perceptual(img, |c| {
                let s = [
                    0.393 * c[0] + 0.769 * c[1] + 0.189 * c[2],
                    0.349 * c[0] + 0.686 * c[1] + 0.168 * c[2],
                    0.272 * c[0] + 0.534 * c[1] + 0.131 * c[2],
                ];
                [0, 1, 2].map(|i| c[i] + (s[i] - c[i]) * k)
            });
        }
        EffectKind::Invert => {
            let k = p(0);
            map_perceptual(img, |c| c.map(|x| x + ((1.0 - x.min(1.0)) - x) * k));
        }
        EffectKind::GaussianBlur => {
            let r = p(0) * scale;
            if r >= 0.5 {
                *img = blur(img, r);
            }
        }
        EffectKind::Sharpen => {
            let (amount, r) = (p(0), (p(1) * scale).max(0.5));
            let soft = blur(img, r);
            let w = img.width as usize;
            par::for_each_pixel(&mut img.data, w, |x, y, px| {
                let s = soft.data[y * w + x];
                for c in 0..3 {
                    px[c] = (px[c] + (px[c] - s[c]) * amount).max(0.0);
                }
            });
        }
        EffectKind::Vignette => {
            let (amount, size, soft) = (p(0), p(1), p(2));
            let (w, h) = (img.width as f32, img.height as f32);
            let wi = img.width as usize;
            par::for_each_pixel(&mut img.data, wi, |x, y, px| {
                // Distância normalizada ao centro (elipse do quadro), 1 = canto.
                let dx = (x as f32 + 0.5 - w / 2.0) / (w / 2.0);
                let dy = (y as f32 + 0.5 - h / 2.0) / (h / 2.0);
                let d = (dx * dx + dy * dy).sqrt() / std::f32::consts::SQRT_2;
                let t = ((d - size) / soft).clamp(0.0, 1.0);
                let k = 1.0 - amount * t * t * (3.0 - 2.0 * t);
                for c in &mut px[..3] {
                    *c *= k;
                }
            });
        }
        EffectKind::Mirror => {
            let (hz, vt) = (p(0) >= 0.5, p(1) >= 0.5);
            let (w, h) = (img.width as usize, img.height as usize);
            let src = img.data.clone();
            par::for_each_pixel(&mut img.data, w, |x, y, px| {
                let sx = if hz { w - 1 - x } else { x };
                let sy = if vt { h - 1 - y } else { y };
                *px = src[sy * w + sx];
            });
        }
        EffectKind::Pixelate => {
            let block = ((p(0) * scale).round() as usize).max(1);
            if block > 1 {
                pixelate(img, block);
            }
        }
        EffectKind::ChromaKey => chroma_key(img, e),
        EffectKind::Glow => {
            let (threshold, intensity, r) = (p(0), p(1), (p(2) * scale).max(0.5));
            let mut bright = img.clone();
            let w = img.width as usize;
            par::for_each_pixel(&mut bright.data, w, |_, _, px| {
                let y = luma([px[0], px[1], px[2]]);
                let keep =
                    ((to_perceptual(y) - threshold) / (1.0 - threshold).max(1e-3)).clamp(0.0, 1.0);
                for c in &mut px[..3] {
                    *c *= keep;
                }
            });
            let glow = blur(&bright, r);
            par::for_each_pixel(&mut img.data, w, |x, y, px| {
                let g = glow.data[y * w + x];
                for c in 0..3 {
                    px[c] += g[c] * intensity;
                }
            });
        }
        // Audio effects do not touch the picture; plug-ins' go through the
        // scene's plug-ins (`plugin`).
        // (the audio effects, all of them).
        _ => {}
    }
}

fn color_grade(img: &mut ImageRgbaF32, e: &Effect) {
    let p = |i: usize| e.param(i);
    let exposure = 2f32.powf(p(grade::EXPOSURE));
    let contrast = 1.0 + p(grade::CONTRAST);
    let saturation = p(grade::SATURATION);
    let temp = p(grade::TEMPERATURE);
    let tint = p(grade::TINT);
    let lift = [0, 1, 2].map(|i| p(grade::LIFT + i));
    let gamma = [0, 1, 2].map(|i| p(grade::GAMMA + i));
    let gain = [0, 1, 2].map(|i| p(grade::GAIN + i));
    // Balanço de branco simples em luz linear.
    let wb = [1.0 + 0.3 * temp, 1.0 - 0.3 * tint, 1.0 - 0.3 * temp];
    // Pivô de contraste: cinza 18% no espaço perceptual.
    let pivot = to_perceptual(0.18);
    map_color(img, |c| {
        let lin = [0, 1, 2].map(|i| c[i] * exposure * wb[i]);
        let mut v = lin.map(to_perceptual);
        for i in 0..3 {
            let mut x = v[i] * (1.0 + gain[i]);
            x += lift[i] * (1.0 - x);
            x = x.max(0.0).powf(1.0 / (1.0 + gamma[i]).max(0.05));
            x = (x - pivot) * contrast + pivot;
            v[i] = x;
        }
        let y = luma(v);
        v = v.map(|x| y + (x - y) * saturation);
        v.map(to_linear)
    });
}

fn chroma_key(img: &mut ImageRgbaF32, e: &Effect) {
    let key = [e.param(0), e.param(1), e.param(2)];
    let (tol, soft, spill) = (e.param(3), e.param(4).max(1e-3), e.param(5));
    // Cb/Cr perceptuais (BT.709) da cor-chave.
    let cbcr = |c: [f32; 3]| {
        let y = luma(c);
        [(c[2] - y) / 1.8556, (c[0] - y) / 1.5748]
    };
    let k = cbcr(key);
    let k_len = (k[0] * k[0] + k[1] * k[1]).sqrt().max(1e-4);
    let w = img.width as usize;
    par::for_each_pixel(&mut img.data, w, |_, _, px| {
        let a = px[3];
        if a <= 0.0 {
            return;
        }
        let lin = [px[0] / a, px[1] / a, px[2] / a];
        let v = lin.map(to_perceptual);
        let c = cbcr(v);
        // Distância no plano de croma, normalizada pela saturação da chave.
        let d = ((c[0] - k[0]).powi(2) + (c[1] - k[1]).powi(2)).sqrt() / (2.0 * k_len);
        let alpha = ((d - tol) / soft).clamp(0.0, 1.0);
        let mut out = lin;
        if spill > 0.0 {
            // Tira o excesso do canal dominante da chave (normalmente o verde).
            let dom = if key[1] >= key[0] && key[1] >= key[2] {
                1
            } else if key[2] >= key[0] {
                2
            } else {
                0
            };
            let others = [0, 1, 2]
                .iter()
                .filter(|&&i| i != dom)
                .map(|&i| out[i])
                .fold(0.0f32, f32::max);
            if out[dom] > others {
                out[dom] -= (out[dom] - others) * spill;
            }
        }
        let na = a * alpha;
        *px = [out[0] * na, out[1] * na, out[2] * na, na];
    });
}

fn pixelate(img: &mut ImageRgbaF32, block: usize) {
    let (w, h) = (img.width as usize, img.height as usize);
    let src = img.data.clone();
    par::for_each_row(&mut img.data, w, |y, row| {
        let by = y / block * block;
        let y1 = (by + block).min(h);
        for bx in (0..w).step_by(block) {
            let x1 = (bx + block).min(w);
            let mut acc = [0.0f32; 4];
            for yy in by..y1 {
                for xx in bx..x1 {
                    let s = src[yy * w + xx];
                    for c in 0..4 {
                        acc[c] += s[c];
                    }
                }
            }
            let n = ((y1 - by) * (x1 - bx)) as f32;
            let avg = acc.map(|v| v / n);
            row[bx..x1].fill(avg);
        }
    });
}

/// Desfoque gaussiano aproximado por três passagens de caixa em cada eixo
/// (custo constante por pixel, qualquer que seja o raio). `radius` ≈ 2σ.
pub fn blur(img: &ImageRgbaF32, radius: f32) -> ImageRgbaF32 {
    let sigma = (radius / 2.0).max(0.3);
    let boxes = box_sizes(sigma, 3);
    let (w, h) = (img.width as usize, img.height as usize);
    let mut a = img.data.clone();
    let mut b = vec![[0.0f32; 4]; a.len()];
    for &r in &boxes {
        box_h(&a, &mut b, w, h, r);
        box_v(&b, &mut a, w, h, r);
    }
    ImageRgbaF32 {
        width: img.width,
        height: img.height,
        data: a,
    }
}

/// Raios de caixa cujas passagens somadas aproximam um gaussiano de `sigma`
/// (método de Kovesi).
fn box_sizes(sigma: f32, n: usize) -> Vec<usize> {
    let ideal = ((12.0 * sigma * sigma / n as f32) + 1.0).sqrt();
    let mut wl = ideal.floor() as i32;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    let m = ((12.0 * sigma * sigma
        - (n as i32 * wl * wl) as f32
        - (4 * n as i32 * wl) as f32
        - 3.0 * n as f32)
        / (-4.0 * wl as f32 - 4.0))
        .round() as i32;
    (0..n as i32)
        .map(|i| (((if i < m { wl } else { wu }) - 1) / 2).max(0) as usize)
        .collect()
}

fn box_h(src: &[[f32; 4]], dst: &mut [[f32; 4]], w: usize, _h: usize, r: usize) {
    if r == 0 {
        dst.copy_from_slice(src);
        return;
    }
    let inv = 1.0 / (2 * r + 1) as f32;
    par::for_each_row(dst, w, |y, row| {
        let s = &src[y * w..y * w + w];
        let at = |i: isize| s[i.clamp(0, w as isize - 1) as usize];
        let mut acc = [0.0f32; 4];
        for i in -(r as isize)..=(r as isize) {
            let v = at(i);
            for c in 0..4 {
                acc[c] += v[c];
            }
        }
        for (x, out) in row.iter_mut().enumerate().take(w) {
            *out = acc.map(|v| v * inv);
            let add = at(x as isize + r as isize + 1);
            let sub = at(x as isize - r as isize);
            for c in 0..4 {
                acc[c] += add[c] - sub[c];
            }
        }
    });
}

/// Passagem vertical: transpõe, aplica a horizontal e transpõe de volta
/// (mantém o acesso à memória em linhas, e o código sem `unsafe`).
fn box_v(src: &[[f32; 4]], dst: &mut [[f32; 4]], w: usize, h: usize, r: usize) {
    if r == 0 {
        dst.copy_from_slice(src);
        return;
    }
    let t = transpose(src, w, h);
    let mut tb = vec![[0.0f32; 4]; t.len()];
    box_h(&t, &mut tb, h, w, r);
    let back = transpose(&tb, h, w);
    dst.copy_from_slice(&back);
}

fn transpose(src: &[[f32; 4]], w: usize, h: usize) -> Vec<[f32; 4]> {
    let mut out = vec![[0.0f32; 4]; src.len()];
    par::for_each_row(&mut out, h, |x, row| {
        for (y, px) in row.iter_mut().enumerate() {
            *px = src[y * w + x];
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gray(v: f32) -> ImageRgbaF32 {
        ImageRgbaF32::filled(40, 30, [v, v, v, 1.0])
    }

    #[test]
    fn padroes_nao_alteram_a_imagem() {
        for k in [
            EffectKind::BrightnessContrast,
            EffectKind::ColorGrade,
            EffectKind::Levels,
        ] {
            let mut img = gray(0.3);
            apply(&mut img, &Effect::new(k), 1.0);
            let px = img.pixel(5, 5);
            assert!((px[0] - 0.3).abs() < 1e-4, "{k:?}: {px:?}");
        }
    }

    #[test]
    fn desfoque_preserva_imagem_uniforme_e_espalha_ponto() {
        let mut img = gray(0.5);
        img = blur(&img, 10.0);
        assert!((img.pixel(20, 15)[0] - 0.5).abs() < 1e-4);
        let mut dot = ImageRgbaF32::filled(41, 41, [0.0, 0.0, 0.0, 1.0]);
        *dot.pixel_mut(20, 20) = [1.0, 1.0, 1.0, 1.0];
        let b = blur(&dot, 6.0);
        let total: f32 = b.data.iter().map(|p| p[0]).sum();
        assert!((total - 1.0).abs() < 1e-3, "energia {total}");
        assert!(b.pixel(20, 20)[0] < 0.2 && b.pixel(23, 20)[0] > 0.0);
        // Simétrico.
        assert!((b.pixel(17, 20)[0] - b.pixel(23, 20)[0]).abs() < 1e-5);
        assert!((b.pixel(20, 17)[0] - b.pixel(20, 23)[0]).abs() < 1e-5);
    }

    #[test]
    fn preto_e_branco_remove_cor() {
        let mut img = ImageRgbaF32::filled(4, 4, [1.0, 0.0, 0.0, 1.0]);
        apply(&mut img, &Effect::new(EffectKind::BlackAndWhite), 1.0);
        let px = img.pixel(0, 0);
        assert!((px[0] - px[1]).abs() < 1e-6 && (px[1] - px[2]).abs() < 1e-6);
    }

    #[test]
    fn chroma_key_remove_verde_e_mantem_vermelho() {
        let mut img = ImageRgbaF32::filled(2, 1, [0.0, 1.0, 0.0, 1.0]);
        *img.pixel_mut(1, 0) = [0.8, 0.1, 0.1, 1.0];
        apply(&mut img, &Effect::new(EffectKind::ChromaKey), 1.0);
        assert!(img.pixel(0, 0)[3] < 0.01, "{:?}", img.pixel(0, 0));
        assert!(img.pixel(1, 0)[3] > 0.99, "{:?}", img.pixel(1, 0));
    }

    #[test]
    fn exposicao_dobra_a_luz() {
        let mut e = Effect::new(EffectKind::ColorGrade);
        e.set_param(grade::EXPOSURE, 1.0);
        let mut img = gray(0.2);
        apply(&mut img, &e, 1.0);
        assert!((img.pixel(1, 1)[0] - 0.4).abs() < 1e-3);
    }

    #[test]
    fn espelho_horizontal() {
        let mut img = ImageRgbaF32::filled(3, 1, [0.0, 0.0, 0.0, 1.0]);
        *img.pixel_mut(0, 0) = [1.0, 0.0, 0.0, 1.0];
        apply(&mut img, &Effect::new(EffectKind::Mirror), 1.0);
        assert_eq!(img.pixel(2, 0)[0], 1.0);
    }
}
