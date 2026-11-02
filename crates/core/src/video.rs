//! Quadros de vídeo decodificados e a imagem de trabalho em luz linear.

use crate::time::Tick;

/// Layout de pixels de um quadro decodificado.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    Yuv420P8,
    Yuv422P8,
    Yuv444P8,
    Yuv420P10,
    Yuv422P10,
    Yuv444P10,
    Gray8,
    /// RGBA 8 bits intercalado, em um único plano.
    Rgba8,
}

impl PixelFormat {
    pub fn bit_depth(self) -> u8 {
        match self {
            PixelFormat::Yuv420P10 | PixelFormat::Yuv422P10 | PixelFormat::Yuv444P10 => 10,
            _ => 8,
        }
    }

    /// Log2 da subamostragem de croma (horizontal, vertical).
    pub fn chroma_shift(self) -> (u8, u8) {
        match self {
            PixelFormat::Yuv420P8 | PixelFormat::Yuv420P10 => (1, 1),
            PixelFormat::Yuv422P8 | PixelFormat::Yuv422P10 => (1, 0),
            _ => (0, 0),
        }
    }

    pub fn plane_count(self) -> usize {
        match self {
            PixelFormat::Gray8 | PixelFormat::Rgba8 => 1,
            _ => 3,
        }
    }

    pub fn is_yuv(self) -> bool {
        !matches!(self, PixelFormat::Rgba8)
    }
}

/// Primárias de cor (ITU-T H.273, tabela 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum ColorPrimaries {
    #[default]
    Bt709,
    /// BT.601 625 linhas (PAL), H.273 valor 5.
    Bt601_625,
    /// BT.601 525 linhas (NTSC) / SMPTE 170M, H.273 valor 6.
    Bt601_525,
    Bt2020,
    DisplayP3,
}

/// Função de transferência (ITU-T H.273, tabela 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum TransferCharacteristic {
    /// BT.709 / BT.601 / BT.2020 (mesma curva OETF).
    #[default]
    Bt709,
    Srgb,
    Linear,
    /// SMPTE ST 2084 (HDR10).
    Pq,
    /// ARIB STD-B67 (HLG).
    Hlg,
}

/// Matriz Y′CbCr ↔ R′G′B′ (ITU-T H.273, tabela 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum MatrixCoefficients {
    #[default]
    Bt709,
    Bt601,
    Bt2020Ncl,
    /// Os planos já são G, B, R.
    Identity,
}

/// How a video's lines are scanned: whole frames, or two fields a frame
/// — the upper one (the even lines, counting from 0) shown first, or the
/// lower one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FieldOrder {
    /// None (progressive scan).
    #[default]
    Progressive,
    UpperFirst,
    LowerFirst,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum ColorRange {
    /// 16–235 (luma) / 16–240 (croma) em 8 bits.
    #[default]
    Limited,
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ColorInfo {
    pub primaries: ColorPrimaries,
    pub transfer: TransferCharacteristic,
    pub matrix: MatrixCoefficients,
    pub range: ColorRange,
}

impl ColorInfo {
    /// Suposição usual quando o arquivo não informa a cor: SD usa BT.601,
    /// HD e acima usam BT.709.
    pub fn guess_for(width: u32, height: u32) -> ColorInfo {
        if width <= 1024 && height <= 576 {
            let primaries = if height == 576 {
                ColorPrimaries::Bt601_625
            } else {
                ColorPrimaries::Bt601_525
            };
            ColorInfo {
                primaries,
                transfer: TransferCharacteristic::Bt709,
                matrix: MatrixCoefficients::Bt601,
                range: ColorRange::Limited,
            }
        } else {
            ColorInfo::default()
        }
    }

    pub const SRGB: ColorInfo = ColorInfo {
        primaries: ColorPrimaries::Bt709,
        transfer: TransferCharacteristic::Srgb,
        matrix: MatrixCoefficients::Identity,
        range: ColorRange::Full,
    };
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaneData {
    U8(Vec<u8>),
    /// Amostras de 9 a 16 bits, alinhadas à direita.
    U16(Vec<u16>),
}

/// Um plano de imagem. `stride` é medido em amostras, não em bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plane {
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub data: PlaneData,
}

impl Plane {
    pub fn new_u8(width: usize, height: usize, fill: u8) -> Plane {
        Plane {
            width,
            height,
            stride: width,
            data: PlaneData::U8(vec![fill; width * height]),
        }
    }

    pub fn new_u16(width: usize, height: usize, fill: u16) -> Plane {
        Plane {
            width,
            height,
            stride: width,
            data: PlaneData::U16(vec![fill; width * height]),
        }
    }

    pub fn as_u8(&self) -> Option<&[u8]> {
        match &self.data {
            PlaneData::U8(d) => Some(d),
            PlaneData::U16(_) => None,
        }
    }

    pub fn as_u16(&self) -> Option<&[u16]> {
        match &self.data {
            PlaneData::U16(d) => Some(d),
            PlaneData::U8(_) => None,
        }
    }

    /// Amostra em `(x, y)` como inteiro, qualquer que seja a profundidade.
    pub fn sample(&self, x: usize, y: usize) -> u16 {
        let i = y * self.stride + x;
        match &self.data {
            PlaneData::U8(d) => d[i] as u16,
            PlaneData::U16(d) => d[i],
        }
    }
}

/// Um quadro decodificado, com o instante de apresentação na mídia de origem.
#[derive(Clone, Debug, PartialEq)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub color: ColorInfo,
    pub planes: Vec<Plane>,
    pub pts: Tick,
    pub duration: Tick,
}

impl VideoFrame {
    /// A 4:2:0 frame packed for the video encoders: NV12, or with
    /// `ten_bit` P010 (each sample in the high 10 bits of a little-endian
    /// 16-bit word) — the luma rows, then the rows of Cb and Cr side by
    /// side. Samples of another depth are shifted to the one asked for.
    pub fn packed_420(&self, ten_bit: bool) -> Vec<u8> {
        let depth = self.format.bit_depth();
        let shift = |v: u16| -> u16 {
            match (depth, ten_bit) {
                (8, true) => v << 2,
                (d, false) if d > 8 => v >> (d - 8),
                _ => v,
            }
        };
        let bytes = if ten_bit { 2 } else { 1 };
        let (w, h) = (self.width as usize, self.height as usize);
        let (cw, ch) = (w / 2, h / 2);
        let mut out = Vec::with_capacity((w * h + 2 * cw * ch) * bytes);
        let mut push = |v: u16| {
            let v = shift(v);
            if ten_bit {
                out.extend_from_slice(&(v << 6).to_le_bytes());
            } else {
                out.push(v as u8);
            }
        };
        let row = |p: &Plane, y: usize, n: usize| -> Vec<u16> {
            let at = y * p.stride;
            match &p.data {
                PlaneData::U8(d) => d[at..at + n].iter().map(|&v| v as u16).collect(),
                PlaneData::U16(d) => d[at..at + n].to_vec(),
            }
        };
        for y in 0..h {
            for v in row(&self.planes[0], y, w) {
                push(v);
            }
        }
        for y in 0..ch {
            let (cb, cr) = (row(&self.planes[1], y, cw), row(&self.planes[2], y, cw));
            for (b, r) in cb.into_iter().zip(cr) {
                push(b);
                push(r);
            }
        }
        out
    }

    /// Aloca um quadro preto (em YUV limitado ou RGBA opaco).
    pub fn black(width: u32, height: u32, format: PixelFormat, color: ColorInfo) -> VideoFrame {
        let (w, h) = (width as usize, height as usize);
        let (sx, sy) = format.chroma_shift();
        let cw = w.div_ceil(1 << sx);
        let ch = h.div_ceil(1 << sy);
        let depth = format.bit_depth();
        let (y0, c0) = match color.range {
            ColorRange::Limited => (16u16 << (depth - 8), 128u16 << (depth - 8)),
            ColorRange::Full => (0, 128u16 << (depth - 8)),
        };
        let planes = match format {
            PixelFormat::Rgba8 => {
                let mut p = Plane::new_u8(w * 4, h, 0);
                if let PlaneData::U8(d) = &mut p.data {
                    for px in d.as_chunks_mut::<4>().0 {
                        px[3] = 255;
                    }
                }
                vec![p]
            }
            PixelFormat::Gray8 => vec![Plane::new_u8(w, h, y0 as u8)],
            _ if depth == 8 => vec![
                Plane::new_u8(w, h, y0 as u8),
                Plane::new_u8(cw, ch, c0 as u8),
                Plane::new_u8(cw, ch, c0 as u8),
            ],
            _ => vec![
                Plane::new_u16(w, h, y0),
                Plane::new_u16(cw, ch, c0),
                Plane::new_u16(cw, ch, c0),
            ],
        };
        VideoFrame {
            width,
            height,
            format,
            color,
            planes,
            pts: Tick::ZERO,
            duration: Tick::ZERO,
        }
    }
}

/// Imagem RGBA em ponto flutuante de 32 bits, luz linear, alfa pré-multiplicado.
/// É o formato de trabalho de toda a composição.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageRgbaF32 {
    pub width: u32,
    pub height: u32,
    pub data: Vec<[f32; 4]>,
}

impl ImageRgbaF32 {
    /// Imagem totalmente transparente.
    pub fn transparent(width: u32, height: u32) -> ImageRgbaF32 {
        ImageRgbaF32::filled(width, height, [0.0; 4])
    }

    pub fn filled(width: u32, height: u32, px: [f32; 4]) -> ImageRgbaF32 {
        ImageRgbaF32 {
            width,
            height,
            data: vec![px; width as usize * height as usize],
        }
    }

    pub fn pixel(&self, x: u32, y: u32) -> [f32; 4] {
        self.data[y as usize * self.width as usize + x as usize]
    }

    pub fn pixel_mut(&mut self, x: u32, y: u32) -> &mut [f32; 4] {
        &mut self.data[y as usize * self.width as usize + x as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quadro_preto_420_tem_planos_de_croma_arredondados_para_cima() {
        let f = VideoFrame::black(5, 3, PixelFormat::Yuv420P8, ColorInfo::default());
        assert_eq!(f.planes[0].width, 5);
        assert_eq!((f.planes[1].width, f.planes[1].height), (3, 2));
        assert_eq!(f.planes[0].sample(4, 2), 16);
        assert_eq!(f.planes[2].sample(2, 1), 128);
    }

    #[test]
    fn quadro_preto_10_bits() {
        let f = VideoFrame::black(4, 4, PixelFormat::Yuv422P10, ColorInfo::default());
        assert_eq!(f.planes[0].sample(0, 0), 64);
        assert_eq!(f.planes[1].sample(0, 0), 512);
        assert_eq!((f.planes[1].width, f.planes[1].height), (2, 4));
    }
}
