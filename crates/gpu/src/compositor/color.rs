//! ACES on the card: pictures into the working space, the finished frame
//! out through the look and the view — each step of the `bv-ocio`
//! processor a kernel of `shaders/color_*.wgsl`, the same math as
//! `bv_render::enter_chain` and `leave_through_view`.

use bv_ocio::curves::Curve;
use bv_ocio::ops::{Negatives, Op};
use bv_timeline::ColorPipeline;
use wgpu::util::DeviceExt;

use super::{Compositor, Img};
use crate::kernels::{Grid, K, Params};

/// Whether the card draws every step of `view` (the processor draws the
/// frames it cannot).
pub fn draws(view: &bv_ocio::Processor) -> bool {
    view.ops().iter().all(|op| {
        matches!(
            op,
            Op::Matrix(..)
                | Op::Range { .. }
                | Op::Power(..)
                | Op::GamutCompress(_)
                | Op::AcesSdr(_)
                | Op::Curve(
                    Curve::Srgb | Curve::MonCurve { .. } | Curve::Gamma { .. },
                    _
                )
        )
    })
}

fn rows(m: &[[f32; 3]; 3]) -> [f32; 9] {
    [
        m[0][0], m[0][1], m[0][2], m[1][0], m[1][1], m[1][2], m[2][0], m[2][1], m[2][2],
    ]
}

fn rows64(m: &bv_ocio::math::M3) -> [f32; 9] {
    rows(&m.map(|r| r.map(|v| v as f32)))
}

impl Compositor {
    /// `img` (premultiplied) taken as ACES2065-1 into the working space, in
    /// place: its light, or with the 2.222 gamma its encoded values.
    pub(super) fn enter_working_space(
        &self,
        enc: &mut wgpu::CommandEncoder,
        img: &Img,
        p: &ColorPipeline,
    ) {
        self.kernel(
            enc,
            K::ColorEnter,
            Params::sized(img.w, img.h).with_f(&rows(&p.input)),
            &[&img.buf],
            Grid::Pixels(img.w, img.h),
        );
    }

    /// The finished frame through the view, in place: the display's code
    /// values as video's light.
    pub(super) fn leave_through_view(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        img: &Img,
        p: &ColorPipeline,
    ) {
        self.through(enc, img, &p.view, true);
    }

    /// A media's file values (encoded) through the conversion from its
    /// color space to the working space, in place.
    pub(super) fn enter_from_space(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        img: &Img,
        q: &bv_ocio::Processor,
    ) {
        self.through(enc, img, q, false);
    }

    /// `img`'s straight colors through `q`'s steps, one kernel each; the
    /// result as video's light (`decode`: the view's code values) or as it
    /// is.
    fn through(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        img: &Img,
        q: &bv_ocio::Processor,
        decode: bool,
    ) {
        let (w, h) = (img.w, img.h);
        let grid = Grid::Pixels(w, h);
        self.kernel(enc, K::ViewBegin, Params::sized(w, h), &[&img.buf], grid);
        for op in q.ops() {
            let mut params = Params::sized(w, h);
            let k = match op {
                Op::Matrix(m, o) => {
                    params.f[..9].copy_from_slice(&rows64(m));
                    params.f[9..12].copy_from_slice(&o.map(|v| v as f32));
                    K::ViewMatrix
                }
                Op::Range {
                    scale,
                    offset,
                    lo,
                    hi,
                } => {
                    params.f[..4].copy_from_slice(&[
                        *scale as f32,
                        *offset as f32,
                        lo.unwrap_or(0.0) as f32,
                        hi.unwrap_or(0.0) as f32,
                    ]);
                    params.j[0] = u32::from(lo.is_some()) | (u32::from(hi.is_some()) << 1);
                    K::ViewRange
                }
                Op::Power(e, negatives) => {
                    params.f[..3].copy_from_slice(&e.map(|v| v as f32));
                    params.j[0] = match negatives {
                        Negatives::Clamp => 0,
                        Negatives::Mirror => 1,
                        Negatives::PassThrough => 2,
                    };
                    K::ViewPower
                }
                Op::Curve(curve, to_linear) => {
                    params.j[1] = u32::from(*to_linear);
                    match *curve {
                        Curve::Srgb => params.j[0] = 0,
                        Curve::MonCurve { gamma, offset } => {
                            // Where its straight part ends and its slope,
                            // as `curves::moncurve_f` / `_r` work them out.
                            let k = offset * gamma / ((gamma - 1.0) * (1.0 + offset));
                            let (end, slope) = if *to_linear {
                                (
                                    offset / (gamma - 1.0),
                                    ((gamma - 1.0) / offset) * k.powf(gamma),
                                )
                            } else {
                                (
                                    k.powf(gamma),
                                    ((gamma - 1.0) / offset).powf(gamma - 1.0)
                                        * ((1.0 + offset) / gamma).powf(gamma),
                                )
                            };
                            params.j[0] = 1;
                            params.f[..4].copy_from_slice(&[
                                gamma as f32,
                                offset as f32,
                                end as f32,
                                slope as f32,
                            ]);
                        }
                        Curve::Gamma { gamma, mirror } => {
                            params.j[0] = 2;
                            params.j[2] = u32::from(mirror);
                            params.f[0] = (if *to_linear { gamma } else { 1.0 / gamma }) as f32;
                        }
                        _ => unreachable!("`draws` lets only these through"),
                    }
                    K::ViewCurve
                }
                Op::GamutCompress(inverse) => {
                    let [to, back] = bv_ocio::aces::gamut_compress_matrices();
                    params.f[..9].copy_from_slice(&rows64(&to));
                    params.f[9..18].copy_from_slice(&rows64(&back));
                    params.j[0] = u32::from(*inverse);
                    K::AcesGamutCompress
                }
                Op::AcesSdr(o) => {
                    let constants =
                        self.gpu
                            .device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("aces constants"),
                                contents: bytemuck::cast_slice(&o.gpu_constants()),
                                usage: wgpu::BufferUsages::STORAGE,
                            });
                    self.kernel(enc, K::AcesRrt, params, &[&constants, &img.buf], grid);
                    self.kernel(enc, K::AcesOdt, params, &[&constants, &img.buf], grid);
                    continue;
                }
                _ => unreachable!("`draws` lets only these through"),
            };
            self.kernel(enc, k, params, &[&img.buf], grid);
        }
        let mut end = Params::sized(w, h);
        end.j[0] = u32::from(!decode);
        self.kernel(enc, K::ViewEnd, end, &[&img.buf], grid);
    }
}
