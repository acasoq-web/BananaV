//! Conversão entre o enquadramento do renderer ([`Transform`]: onde a mídia
//! fica no quadro) e a janela de visão do Pan/Crop (o "F": que parte da
//! mídia aparece), em pixels da mídia.
//!
//! O renderer leva um ponto `p` da mídia ao quadro por
//! `P = C + T + R(θ)·K·F·(p − S)`, com `S` o centro da mídia, `C` o centro do
//! quadro, `T` o deslocamento, `K` a escala (encaixe × zoom × esticar), `F`
//! os espelhamentos e `R(θ)` a rotação. A janela de visão é a imagem inversa
//! do quadro inteiro: o retângulo da mídia que cai dentro dele.

use bv_timeline::Transform;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    /// Centro, em pixels da mídia.
    pub cx: f32,
    pub cy: f32,
    /// Tamanho, em pixels da mídia.
    pub w: f32,
    pub h: f32,
    /// Ângulo do retângulo sobre a mídia, em graus (sentido horário na tela).
    pub angle_deg: f32,
}

fn fit(src: (f32, f32), out: (f32, f32)) -> f32 {
    (out.0 / src.0).min(out.1 / src.1)
}

pub(crate) fn rot(v: (f32, f32), deg: f32) -> (f32, f32) {
    let (s, c) = deg.to_radians().sin_cos();
    (v.0 * c - v.1 * s, v.0 * s + v.1 * c)
}

/// The box the framing shows: what the frame shows, as much of it as the
/// box's size says (`Transform::box_size`).
pub fn viewport_of(tf: &Transform, src: (f32, f32), out: (f32, f32)) -> Viewport {
    let f = fit(src, out);
    let kx = f * tf.scale.max(1e-4);
    let ky = kx * tf.stretch.max(1e-4);
    let t = (tf.x * out.0, -tf.y * out.1);
    let back = rot(t, -tf.rotation_deg);
    let (fx, fy) = (
        if tf.flip_h { -1.0 } else { 1.0 },
        if tf.flip_v { -1.0 } else { 1.0 },
    );
    Viewport {
        cx: src.0 / 2.0 - fx * back.0 / kx,
        cy: src.1 / 2.0 - fy * back.1 / ky,
        w: out.0 / kx * tf.box_size[0].max(1e-4),
        h: out.1 / ky * tf.box_size[1].max(1e-4),
        angle_deg: -tf.rotation_deg,
    }
}

/// The framing that shows box `vp` as `base`'s Source settings say, as
/// VEGAS's: its picture filling the frame — stretched to it, or keeping
/// its proportions with what is around the box filling the rest — or,
/// not filling it, a pixel of the media a pixel of the frame. The crops,
/// flips and the box's settings come from `base`.
pub fn transform_for(
    vp: &Viewport,
    base: &Transform,
    src: (f32, f32),
    out: (f32, f32),
) -> Transform {
    let f = fit(src, out);
    let (w, h) = (vp.w.max(1e-3), vp.h.max(1e-3));
    let (kx, ky) = if !base.fill_frame {
        (1.0, 1.0)
    } else if base.keeps_aspect() {
        let k = (out.0 / w).min(out.1 / h);
        (k, k)
    } else {
        (out.0 / w, out.1 / h)
    };
    let theta = -vp.angle_deg;
    let (fx, fy) = (
        if base.flip_h { -1.0 } else { 1.0 },
        if base.flip_v { -1.0 } else { 1.0 },
    );
    let d = (
        fx * kx * (vp.cx - src.0 / 2.0),
        fy * ky * (vp.cy - src.1 / 2.0),
    );
    let r = rot(d, theta);
    Transform {
        x: -r.0 / out.0,
        y: r.1 / out.1,
        scale: kx / f,
        stretch: ky / kx,
        rotation_deg: theta,
        box_size: [w * kx / out.0, h * ky / out.1],
        ..*base
    }
}

/// Where the box turns (Rotation › X Center and Y Center), in the media's
/// pixels.
pub fn pivot_point(vp: &Viewport, pivot: [f32; 2]) -> (f32, f32) {
    let d = rot((pivot[0] * vp.w, pivot[1] * vp.h), vp.angle_deg);
    (vp.cx + d.0, vp.cy + d.1)
}

/// The pivot at `at` (the media's pixels), as `Transform::pivot` keeps it.
pub fn pivot_of(vp: &Viewport, at: (f32, f32)) -> [f32; 2] {
    let l = rot((at.0 - vp.cx, at.1 - vp.cy), -vp.angle_deg);
    [l.0 / vp.w.max(1e-3), l.1 / vp.h.max(1e-3)]
}

/// The box turned to `angle_deg` about its pivot: its middle goes round
/// it, the pivot stays where it is.
pub fn turned(vp: &Viewport, pivot: [f32; 2], angle_deg: f32) -> Viewport {
    let p = pivot_point(vp, pivot);
    let d = rot((vp.cx - p.0, vp.cy - p.1), angle_deg - vp.angle_deg);
    Viewport {
        cx: p.0 + d.0,
        cy: p.1 + d.1,
        angle_deg,
        ..*vp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: &Transform, b: &Transform) -> bool {
        let e = |x: f32, y: f32| (x - y).abs() < 1e-3;
        e(a.x, b.x)
            && e(a.y, b.y)
            && e(a.scale, b.scale)
            && e(a.stretch, b.stretch)
            && e(a.rotation_deg, b.rotation_deg)
            && a.flip_h == b.flip_h
            && a.flip_v == b.flip_v
    }

    #[test]
    fn padrao_mostra_a_midia_inteira() {
        // Mídia 1280×720 num projeto 1920×1080: o F é a mídia toda.
        let vp = viewport_of(&Transform::default(), (1280.0, 720.0), (1920.0, 1080.0));
        assert!((vp.w - 1280.0).abs() < 1e-3 && (vp.h - 720.0).abs() < 1e-3);
        assert!((vp.cx - 640.0).abs() < 1e-3 && (vp.cy - 360.0).abs() < 1e-3);
    }

    #[test]
    fn zoom_de_2x_encolhe_o_f() {
        let tf = Transform {
            scale: 2.0,
            ..Transform::default()
        };
        let vp = viewport_of(&tf, (1280.0, 720.0), (1280.0, 720.0));
        assert!((vp.w - 640.0).abs() < 1e-3);
    }

    #[test]
    fn ida_e_volta() {
        let cases = [
            Transform::default(),
            Transform {
                x: 0.2,
                y: -0.1,
                scale: 1.7,
                rotation_deg: 30.0,
                ..Transform::default()
            },
            Transform {
                x: -0.3,
                y: 0.25,
                scale: 0.6,
                rotation_deg: -75.0,
                flip_h: true,
                stretch: 1.4,
                ..Transform::default()
            },
            Transform {
                x: 0.05,
                scale: 3.0,
                rotation_deg: 180.0,
                flip_v: true,
                ..Transform::default()
            },
        ];
        for tf in cases {
            for (src, out) in [
                ((1280.0, 720.0), (1920.0, 1080.0)),
                ((640.0, 480.0), (1920.0, 1080.0)),
            ] {
                let vp = viewport_of(&tf, src, out);
                let back = transform_for(&vp, &tf, src, out);
                assert!(close(&tf, &back), "{tf:?}\n{back:?}");
            }
        }
    }

    fn same_box(a: &Viewport, b: &Viewport) -> bool {
        let e = |x: f32, y: f32| (x - y).abs() < 1e-2;
        e(a.cx, b.cx) && e(a.cy, b.cy) && e(a.w, b.w) && e(a.h, b.h) && e(a.angle_deg, b.angle_deg)
    }

    /// Source › Maintain aspect ratio, as VEGAS's: a box narrower than the
    /// frame keeps its picture's proportions (the frame shows more than the
    /// box); off, the picture stretches to fill it. Either way the box comes
    /// back as it was.
    #[test]
    fn a_box_of_another_shape_keeps_its_picture_s_proportions() {
        let (src, out) = ((1920.0, 1080.0), (1920.0, 1080.0));
        let mut vp = viewport_of(&Transform::default(), src, out);
        vp.w *= 0.5;
        let kept = transform_for(&vp, &Transform::default(), src, out);
        assert!((kept.stretch - 1.0).abs() < 1e-4, "{kept:?}");
        assert!((kept.box_size[0] - 0.5).abs() < 1e-4 && (kept.box_size[1] - 1.0).abs() < 1e-4);
        assert!(same_box(&viewport_of(&kept, src, out), &vp));
        let stretched = Transform {
            keep_aspect: Some(false),
            ..Transform::default()
        };
        let tf = transform_for(&vp, &stretched, src, out);
        assert!((tf.stretch - 0.5).abs() < 1e-4, "{tf:?}");
        assert_eq!(tf.box_size, [1.0; 2]);
        assert!(same_box(&viewport_of(&tf, src, out), &vp));
        // Projects from before the setting: stretched ones stay stretched.
        let old = Transform {
            stretch: 1.4,
            ..Transform::default()
        };
        assert!(!old.keeps_aspect() && Transform::default().keeps_aspect());
    }

    /// Source › Stretch to fill frame off: a pixel of the media a pixel of
    /// the frame, whatever the box's size.
    #[test]
    fn not_filling_the_frame_shows_the_media_at_its_own_size() {
        let (src, out) = ((640.0, 480.0), (1920.0, 1080.0));
        let base = Transform {
            fill_frame: false,
            ..Transform::default()
        };
        let vp = Viewport {
            cx: 320.0,
            cy: 240.0,
            w: 640.0,
            h: 480.0,
            angle_deg: 0.0,
        };
        let tf = transform_for(&vp, &base, src, out);
        // The fit is 2.25: a scale of 1 ⁄ 2.25 is the media's own size.
        assert!((tf.scale - 1.0 / 2.25).abs() < 1e-4, "{tf:?}");
        assert!(same_box(&viewport_of(&tf, src, out), &vp));
    }

    /// Rotation's X and Y Center: the box turns about them, and they stay
    /// where they are.
    #[test]
    fn the_box_turns_about_its_pivot() {
        let vp = Viewport {
            cx: 960.0,
            cy: 540.0,
            w: 1920.0,
            h: 1080.0,
            angle_deg: 0.0,
        };
        let pivot = pivot_of(&vp, (0.0, 0.0));
        assert!((pivot[0] + 0.5).abs() < 1e-4 && (pivot[1] + 0.5).abs() < 1e-4);
        let t = turned(&vp, pivot, 90.0);
        let p = pivot_point(&t, pivot);
        assert!(p.0.abs() < 1e-2 && p.1.abs() < 1e-2, "{p:?}");
        assert!((t.angle_deg - 90.0).abs() < 1e-4);
        // Its middle went round the corner.
        assert!(
            (t.cx - (-540.0)).abs() < 1e-2 && (t.cy - 960.0).abs() < 1e-2,
            "{t:?}"
        );
        // About its middle, it stays put.
        let m = turned(&vp, [0.0; 2], 30.0);
        assert!((m.cx - 960.0).abs() < 1e-3 && (m.cy - 540.0).abs() < 1e-3);
    }

    #[test]
    fn mover_o_f_para_a_direita_desloca_a_imagem_para_a_esquerda() {
        let src = (1000.0, 1000.0);
        let out = (1000.0, 1000.0);
        let mut vp = viewport_of(&Transform::default(), src, out);
        vp.cx += 100.0;
        let tf = transform_for(&vp, &Transform::default(), src, out);
        assert!((tf.x + 0.1).abs() < 1e-4, "{tf:?}");
    }
}
