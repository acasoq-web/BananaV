//! Transições entre duas camadas (alfa pré-multiplicado).
//!
//! `a` é o que sai, `b` o que entra e `p` ∈ [0, 1] o progresso. Num fade-in
//! sem clipe anterior, `a` é a camada transparente.

use bv_core::ImageRgbaF32;
use bv_timeline::{Direction, Transition};

use crate::par;

#[inline]
fn lerp(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

#[inline]
fn over(src: [f32; 4], dst: [f32; 4]) -> [f32; 4] {
    let k = 1.0 - src[3];
    [
        src[0] + dst[0] * k,
        src[1] + dst[1] * k,
        src[2] + dst[2] * k,
        src[3] + dst[3] * k,
    ]
}

/// Largura da borda suave das cortinas, em fração do percurso.
const SOFT: f32 = 0.04;

#[inline]
fn reveal(edge: f32, u: f32) -> f32 {
    ((edge * (1.0 + SOFT) - u) / SOFT).clamp(0.0, 1.0)
}

fn get(img: &ImageRgbaF32, x: i64, y: i64) -> [f32; 4] {
    if x < 0 || y < 0 || x >= img.width as i64 || y >= img.height as i64 {
        [0.0; 4]
    } else {
        img.data[y as usize * img.width as usize + x as usize]
    }
}

/// Compõe a transição de `a` para `b` no progresso `p`.
pub fn apply(kind: Transition, a: &ImageRgbaF32, b: &ImageRgbaF32, p: f32) -> ImageRgbaF32 {
    debug_assert_eq!((a.width, a.height), (b.width, b.height));
    let p = p.clamp(0.0, 1.0);
    let (w, h) = (a.width as usize, a.height as usize);
    let (wf, hf) = (w as f32, h as f32);
    let mut out = ImageRgbaF32::transparent(a.width, a.height);
    match kind {
        Transition::Dissolve => {
            par::for_each_pixel(&mut out.data, w, |x, y, px| {
                let i = y * w + x;
                *px = lerp(a.data[i], b.data[i], p);
            });
        }
        Transition::FadeThroughBlack => {
            par::for_each_pixel(&mut out.data, w, |x, y, px| {
                let i = y * w + x;
                let (src, k) = if p < 0.5 {
                    (a.data[i], 1.0 - 2.0 * p)
                } else {
                    (b.data[i], 2.0 * p - 1.0)
                };
                // A cor escurece; a cobertura (alfa) se mantém.
                *px = [src[0] * k, src[1] * k, src[2] * k, src[3]];
            });
        }
        Transition::Wipe(dir) => {
            par::for_each_pixel(&mut out.data, w, |x, y, px| {
                let i = y * w + x;
                let (xn, yn) = ((x as f32 + 0.5) / wf, (y as f32 + 0.5) / hf);
                // `u` é a posição ao longo do sentido da cortina (0 = onde ela começa).
                let u = match dir {
                    Direction::Left => 1.0 - xn,
                    Direction::Right => xn,
                    Direction::Up => 1.0 - yn,
                    Direction::Down => yn,
                };
                *px = lerp(a.data[i], b.data[i], reveal(p, u));
            });
        }
        Transition::Push(dir) | Transition::Slide(dir) => {
            let push = matches!(kind, Transition::Push(_));
            let (dx, dy) = match dir {
                Direction::Left => (-1, 0),
                Direction::Right => (1, 0),
                Direction::Up => (0, -1),
                Direction::Down => (0, 1),
            };
            let shift_x = (dx as f32 * p * wf).round() as i64;
            let shift_y = (dy as f32 * p * hf).round() as i64;
            let (wi, hi) = (w as i64, h as i64);
            par::for_each_pixel(&mut out.data, w, |x, y, px| {
                let (x, y) = (x as i64, y as i64);
                // `b` entra pelo lado oposto ao movimento.
                let bx = x - shift_x + dx * wi;
                let by = y - shift_y + dy * hi;
                let pb = get(b, bx, by);
                let pa = if push {
                    get(a, x - shift_x, y - shift_y)
                } else {
                    get(a, x, y)
                };
                *px = over(pb, pa);
            });
        }
        Transition::Iris => {
            let max_r = (wf * wf + hf * hf).sqrt() / 2.0;
            par::for_each_pixel(&mut out.data, w, |x, y, px| {
                let i = y * w + x;
                let dx = x as f32 + 0.5 - wf / 2.0;
                let dy = y as f32 + 0.5 - hf / 2.0;
                let u = (dx * dx + dy * dy).sqrt() / max_r;
                *px = lerp(a.data[i], b.data[i], reveal(p, u));
            });
        }
        Transition::Clock => {
            par::for_each_pixel(&mut out.data, w, |x, y, px| {
                let i = y * w + x;
                let dx = x as f32 + 0.5 - wf / 2.0;
                let dy = y as f32 + 0.5 - hf / 2.0;
                // Ângulo a partir do meio-dia, no sentido horário, em [0, 1).
                let ang = dx.atan2(-dy);
                let u = (ang / std::f32::consts::TAU).rem_euclid(1.0);
                *px = lerp(a.data[i], b.data[i], reveal(p, u));
            });
        }
        Transition::Zoom => {
            // `b` cresce a partir do centro enquanto aparece.
            let s = 0.05 + 0.95 * p;
            par::for_each_pixel(&mut out.data, w, |x, y, px| {
                let i = y * w + x;
                let u = (x as f32 + 0.5 - wf / 2.0) / s + wf / 2.0 - 0.5;
                let v = (y as f32 + 0.5 - hf / 2.0) / s + hf / 2.0 - 0.5;
                let pb = get(b, u.round() as i64, v.round() as i64);
                let pb = pb.map(|c| c * p);
                *px = over(pb, a.data[i].map(|c| c * (1.0 - p * 0.5)));
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(v: f32) -> ImageRgbaF32 {
        ImageRgbaF32::filled(32, 18, [v, v, v, 1.0])
    }

    #[test]
    fn extremos_dao_a_e_b() {
        let a = solid(0.2);
        let b = solid(0.8);
        for t in Transition::ALL {
            let start = apply(t, &a, &b, 0.0);
            let end = apply(t, &a, &b, 1.0);
            let mid = start.pixel(16, 9);
            assert!((mid[0] - 0.2).abs() < 1e-4, "{t:?} início: {mid:?}");
            let mid = end.pixel(16, 9);
            assert!((mid[0] - 0.8).abs() < 1e-4, "{t:?} fim: {mid:?}");
        }
    }

    #[test]
    fn cortina_para_a_direita_revela_pela_esquerda() {
        let out = apply(
            Transition::Wipe(Direction::Right),
            &solid(0.0),
            &solid(1.0),
            0.5,
        );
        assert!(out.pixel(2, 9)[0] > 0.99);
        assert!(out.pixel(29, 9)[0] < 0.01);
    }

    #[test]
    fn passar_pelo_preto_fica_preto_no_meio() {
        let out = apply(Transition::FadeThroughBlack, &solid(0.5), &solid(0.9), 0.5);
        assert_eq!(out.pixel(5, 5), [0.0, 0.0, 0.0, 1.0]);
    }
}
