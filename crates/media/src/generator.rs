//! Mídias geradas: cor sólida, barras de cor, xadrez, degradê, ruído e tom de teste.

use std::f64::consts::TAU;
use std::sync::Arc;

use bv_color::transfer::bt709_to_linear;
use bv_core::{AudioBuffer, ChannelLayout, FrameRate, ImageRgbaF32, SampleRate, Tick};
use bv_timeline::Generator;

use crate::{AudioReader, MediaError, Result, VideoReader};

/// Vídeo gerado. Quase todos os geradores são estáticos (a imagem é feita
/// uma vez); o ruído muda a cada quadro.
pub(crate) struct GeneratedVideo {
    kind: VideoKind,
}

enum VideoKind {
    Static(Arc<ImageRgbaF32>),
    Noise {
        amount: f32,
        color: bool,
        width: u32,
        height: u32,
        rate: FrameRate,
        cache: Option<(i64, Arc<ImageRgbaF32>)>,
    },
}

impl GeneratedVideo {
    pub(crate) fn new(
        g: Generator,
        width: u32,
        height: u32,
        rate: FrameRate,
    ) -> Result<GeneratedVideo> {
        let image = match g {
            Generator::SolidColor { rgba } => {
                let [r, gr, b, a] = premultiplied_video(rgba);
                ImageRgbaF32::filled(width, height, [r, gr, b, a])
            }
            Generator::ColorBars => color_bars(width, height),
            Generator::Checkerboard { size } => checkerboard(width, height, size.max(1)),
            Generator::Gradient {
                from,
                to,
                angle_deg,
            } => gradient(width, height, from, to, angle_deg),
            Generator::Noise { amount, color } => {
                return Ok(GeneratedVideo {
                    kind: VideoKind::Noise {
                        amount,
                        color,
                        width,
                        height,
                        rate,
                        cache: None,
                    },
                });
            }
            // Plug-ins' pictures come from their own reader.
            Generator::TestTone { .. } | Generator::Plugin(_) => return Err(MediaError::NoStream),
            // The compositors give an adjustment event the picture below it.
            Generator::Adjustment => ImageRgbaF32::transparent(width, height),
        };
        Ok(GeneratedVideo {
            kind: VideoKind::Static(Arc::new(image)),
        })
    }
}

impl VideoReader for GeneratedVideo {
    fn frame_at(&mut self, t: Tick) -> Result<Arc<ImageRgbaF32>> {
        match &mut self.kind {
            VideoKind::Static(img) => Ok(img.clone()),
            VideoKind::Noise {
                amount,
                color,
                width,
                height,
                rate,
                cache,
            } => {
                let frame = rate.frame_at(t);
                if let Some((f, img)) = cache
                    && *f == frame
                {
                    return Ok(img.clone());
                }
                let img = Arc::new(noise(*width, *height, frame, *amount, *color));
                *cache = Some((frame, img.clone()));
                Ok(img)
            }
        }
    }
}

/// A color picked for a generator (video's values, as VEGAS takes them:
/// BT.709-encoded) with its alpha → premultiplied linear light.
fn premultiplied_video(rgba: [f32; 4]) -> [f32; 4] {
    let a = rgba[3].clamp(0.0, 1.0);
    let [r, g, b] = [rgba[0], rgba[1], rgba[2]].map(bt709_to_linear);
    [r * a, g * a, b * a, a]
}

fn gradient(width: u32, height: u32, from: [f32; 4], to: [f32; 4], angle_deg: f32) -> ImageRgbaF32 {
    let (s, c) = angle_deg.to_radians().sin_cos();
    // Projeção de cada canto na direção do degradê, para normalizar em [0, 1].
    let proj = |x: f32, y: f32| x * c + y * s;
    let corners = [
        proj(-0.5, -0.5),
        proj(0.5, -0.5),
        proj(-0.5, 0.5),
        proj(0.5, 0.5),
    ];
    let lo = corners.iter().copied().fold(f32::MAX, f32::min);
    let hi = corners.iter().copied().fold(f32::MIN, f32::max);
    let mut img = ImageRgbaF32::transparent(width, height);
    for y in 0..height {
        for x in 0..width {
            let nx = (x as f32 + 0.5) / width as f32 - 0.5;
            // y para cima, como em ângulos usuais.
            let ny = 0.5 - (y as f32 + 0.5) / height as f32;
            let t = ((proj(nx, ny) - lo) / (hi - lo).max(1e-6)).clamp(0.0, 1.0);
            // Mixed on the encoded values (perceptual), then made light.
            let mix = [0, 1, 2, 3].map(|i| from[i] + (to[i] - from[i]) * t);
            *img.pixel_mut(x, y) = premultiplied_video(mix);
        }
    }
    img
}

/// Hash inteiro rápido (estilo PCG) para ruído determinístico.
fn hash(mut v: u32) -> u32 {
    v = v.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    let w = ((v >> ((v >> 28) + 4)) ^ v).wrapping_mul(277_803_737);
    (w >> 22) ^ w
}

fn noise(width: u32, height: u32, frame: i64, amount: f32, color: bool) -> ImageRgbaF32 {
    let mut img = ImageRgbaF32::transparent(width, height);
    let seed = hash(frame as u32 ^ 0x9E37_79B9);
    for y in 0..height {
        for x in 0..width {
            let base = hash(seed ^ hash(x ^ hash(y)));
            let n = |k: u32| (hash(base ^ k) as f32 / u32::MAX as f32 - 0.5) * amount + 0.5;
            let px = if color {
                video_rgb(n(1), n(2), n(3))
            } else {
                let v = n(1);
                video_rgb(v, v, v)
            };
            *img.pixel_mut(x, y) = px;
        }
    }
    img
}

/// Valores R′G′B′ (codificados em BT.709) → pixel linear opaco.
fn video_rgb(r: f32, g: f32, b: f32) -> [f32; 4] {
    [
        bt709_to_linear(r),
        bt709_to_linear(g),
        bt709_to_linear(b),
        1.0,
    ]
}

/// Barras de cor no estilo SMPTE EG 1 (75%).
fn color_bars(width: u32, height: u32) -> ImageRgbaF32 {
    let top = [
        video_rgb(0.75, 0.75, 0.75),
        video_rgb(0.75, 0.75, 0.0),
        video_rgb(0.0, 0.75, 0.75),
        video_rgb(0.0, 0.75, 0.0),
        video_rgb(0.75, 0.0, 0.75),
        video_rgb(0.75, 0.0, 0.0),
        video_rgb(0.0, 0.0, 0.75),
    ];
    let black = video_rgb(0.0, 0.0, 0.0);
    let middle = [top[6], black, top[4], black, top[2], black, top[0]];
    // Faixa de baixo: −I, branco 100%, +Q, preto e PLUGE (preto, preto, +4%).
    let bottom: [(f32, [f32; 4]); 7] = [
        (1.25, video_rgb(0.0, 0.129, 0.290)),
        (1.25, video_rgb(1.0, 1.0, 1.0)),
        (1.25, video_rgb(0.196, 0.0, 0.416)),
        (1.25, black),
        (1.0 / 3.0, black),
        (1.0 / 3.0, video_rgb(0.04, 0.04, 0.04)),
        (1.0 + 1.0 / 3.0, black),
    ];
    let mut img = ImageRgbaF32::transparent(width, height);
    let bar_w = width as f32 / 7.0;
    for y in 0..height {
        let fy = y as f32 / height as f32;
        for x in 0..width {
            let bar = ((x as f32 / bar_w) as usize).min(6);
            let px = if fy < 0.67 {
                top[bar]
            } else if fy < 0.75 {
                middle[bar]
            } else {
                let mut edge = 0.0;
                let pos = x as f32 / bar_w;
                bottom
                    .iter()
                    .find(|(w, _)| {
                        edge += w;
                        pos < edge
                    })
                    .map_or(black, |(_, c)| *c)
            };
            *img.pixel_mut(x, y) = px;
        }
    }
    img
}

fn checkerboard(width: u32, height: u32, size: u32) -> ImageRgbaF32 {
    let light = video_rgb(0.8, 0.8, 0.8);
    let dark = video_rgb(0.35, 0.35, 0.35);
    let mut img = ImageRgbaF32::transparent(width, height);
    for y in 0..height {
        for x in 0..width {
            *img.pixel_mut(x, y) = if (x / size + y / size).is_multiple_of(2) {
                light
            } else {
                dark
            };
        }
    }
    img
}

pub(crate) struct GeneratedAudio {
    generator: Generator,
    rate: SampleRate,
    channels: usize,
}

impl GeneratedAudio {
    pub(crate) fn new(g: Generator, rate: SampleRate, channels: usize) -> Result<GeneratedAudio> {
        match g {
            Generator::TestTone { .. } => Ok(GeneratedAudio {
                generator: g,
                rate,
                channels: channels.max(1),
            }),
            _ => Err(MediaError::NoStream),
        }
    }
}

impl AudioReader for GeneratedAudio {
    fn sample_rate(&self) -> SampleRate {
        self.rate
    }

    fn channels(&self) -> usize {
        self.channels
    }

    fn read(&mut self, start: i64, frames: usize) -> Result<AudioBuffer> {
        let layout = ChannelLayout::from_count(self.channels);
        let mut buf = AudioBuffer::silent(self.rate, layout, frames);
        if let Generator::TestTone {
            frequency_hz,
            level_db,
        } = self.generator
        {
            let amp = 10f64.powf(level_db as f64 / 20.0);
            let w = TAU * frequency_hz as f64 / self.rate.0 as f64;
            // A fase vem do índice absoluto da amostra: ler em blocos de
            // qualquer tamanho dá exatamente a mesma onda.
            let tone: Vec<f32> = (0..frames as i64)
                .map(|i| (amp * ((start + i) as f64 * w).sin()) as f32)
                .collect();
            for ch in &mut buf.channels {
                ch.copy_from_slice(&tone);
            }
        }
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_timeline::{MediaInfo, ProjectSettings};

    #[test]
    fn cor_solida_e_linear() {
        let g = Generator::SolidColor {
            rgba: [1.0, 0.5, 0.0, 1.0],
        };
        let mut v = GeneratedVideo::new(g, 4, 2, FrameRate::FPS_30).unwrap();
        let img = v.frame_at(Tick::ZERO).unwrap();
        let px = img.pixel(3, 1);
        // Video's 0.5 (BT.709's curve) in light.
        assert!((px[1] - 0.2596).abs() < 1e-3, "{px:?}");
        assert_eq!(px[0], 1.0);
    }

    #[test]
    fn ruido_muda_por_quadro_e_e_deterministico() {
        let g = Generator::Noise {
            amount: 1.0,
            color: false,
        };
        let mut v = GeneratedVideo::new(g, 16, 16, FrameRate::FPS_25).unwrap();
        let f0 = v.frame_at(Tick::ZERO).unwrap();
        let f0b = v.frame_at(Tick(1)).unwrap();
        let f1 = v.frame_at(FrameRate::FPS_25.frame_start(1)).unwrap();
        assert_eq!(f0, f0b);
        assert_ne!(f0, f1);
    }

    #[test]
    fn degrade_horizontal() {
        let img = gradient(10, 2, [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0], 0.0);
        assert!(img.pixel(0, 0)[0] < 0.02 && img.pixel(9, 1)[0] > 0.8);
        assert!(img.pixel(3, 0)[0] < img.pixel(6, 0)[0]);
    }

    #[test]
    fn barras_tem_sete_cores_no_topo() {
        let img = color_bars(700, 100);
        let first = img.pixel(50, 10);
        let last = img.pixel(650, 10);
        assert!(first[0] > 0.5 && first[2] > 0.5, "cinza 75%");
        assert!(last[0] < 0.01 && last[2] > 0.5, "azul");
        assert!(img.pixel(150, 90)[0] > 0.9, "branco 100% embaixo");
    }

    #[test]
    fn tom_e_continuo_entre_blocos() {
        let info = MediaInfo::generated(
            Generator::TestTone {
                frequency_hz: 1000.0,
                level_db: -6.0,
            },
            &ProjectSettings::default(),
        );
        let mut r = crate::open_audio(&info).unwrap();
        let whole = r.read(100, 480).unwrap();
        let a = r.read(100, 200).unwrap();
        let b = r.read(300, 280).unwrap();
        let joined: Vec<f32> = a.channels[0]
            .iter()
            .chain(&b.channels[0])
            .copied()
            .collect();
        assert_eq!(joined, whole.channels[0]);
        let peak = whole.channels[1].iter().fold(0f32, |m, &s| m.max(s.abs()));
        assert!((peak - 0.501).abs() < 0.01, "−6 dBFS ≈ 0,5: {peak}");
        assert!(crate::open_video(&info).is_err());
    }
}
