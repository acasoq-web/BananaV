//! PNG (ISO/IEC 15948), escrito a partir da especificação.
//!
//! Leitura: todos os tipos de cor (cinza, RGB, paleta, cinza+alfa, RGBA),
//! profundidades de 1 a 16 bits, transparência `tRNS` e entrelaçamento Adam7.
//! A saída é sempre RGBA (8 ou 16 bits). Escrita: RGB/RGBA de 8 ou 16 bits,
//! com filtro escolhido por linha.

use bv_deflate::{Crc32, zlib_compress, zlib_decompress};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    NotPng,
    Truncated,
    Corrupt(String),
    Unsupported(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotPng => f.write_str("não é um arquivo PNG"),
            Error::Truncated => f.write_str("PNG truncado"),
            Error::Corrupt(s) => write!(f, "PNG corrompido: {s}"),
            Error::Unsupported(s) => write!(f, "PNG não suportado: {s}"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// Pixels RGBA, linha a linha, sem preenchimento.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pixels {
    Rgba8(Vec<u8>),
    Rgba16(Vec<u16>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Pixels,
}

impl Image {
    /// RGBA de 8 bits (16 bits são arredondados).
    pub fn to_rgba8(&self) -> Vec<u8> {
        match &self.pixels {
            Pixels::Rgba8(v) => v.clone(),
            Pixels::Rgba16(v) => v
                .iter()
                .map(|&x| ((x as u32 * 255 + 32767) / 65535) as u8)
                .collect(),
        }
    }
}

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

#[derive(Clone, Copy, Debug)]
pub struct Header {
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub color_type: u8,
    pub interlaced: bool,
}

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

/// Percorre os chunks: (tipo, dados).
fn chunks(data: &[u8]) -> Result<Vec<([u8; 4], &[u8])>> {
    if data.len() < 8 || data[..8] != SIGNATURE {
        return Err(Error::NotPng);
    }
    let mut out = Vec::new();
    let mut pos = 8;
    while pos + 12 <= data.len() {
        let len = be32(&data[pos..]) as usize;
        let kind: [u8; 4] = data[pos + 4..pos + 8].try_into().expect("4 bytes");
        let end = pos + 8 + len;
        if end + 4 > data.len() {
            return Err(Error::Truncated);
        }
        let body = &data[pos + 8..end];
        let mut crc = Crc32::new();
        crc.update(&kind);
        crc.update(body);
        if crc.finish() != be32(&data[end..]) {
            return Err(Error::Corrupt(format!(
                "CRC do chunk {}",
                String::from_utf8_lossy(&kind)
            )));
        }
        out.push((kind, body));
        pos = end + 4;
        if &kind == b"IEND" {
            break;
        }
    }
    Ok(out)
}

/// Lê só o cabeçalho (dimensões e formato).
pub fn read_header(data: &[u8]) -> Result<Header> {
    if data.len() < 33 || data[..8] != SIGNATURE {
        return Err(Error::NotPng);
    }
    if &data[12..16] != b"IHDR" {
        return Err(Error::Corrupt("IHDR ausente".into()));
    }
    let h = &data[16..29];
    let header = Header {
        width: be32(&h[0..]),
        height: be32(&h[4..]),
        bit_depth: h[8],
        color_type: h[9],
        interlaced: h[12] == 1,
    };
    let ok_depth = match header.color_type {
        0 => [1, 2, 4, 8, 16].contains(&header.bit_depth),
        3 => [1, 2, 4, 8].contains(&header.bit_depth),
        2 | 4 | 6 => [8, 16].contains(&header.bit_depth),
        _ => false,
    };
    if !ok_depth {
        return Err(Error::Unsupported(format!(
            "tipo de cor {} com {} bits",
            header.color_type, header.bit_depth
        )));
    }
    if header.width == 0 || header.height == 0 {
        return Err(Error::Corrupt("dimensões nulas".into()));
    }
    Ok(header)
}

fn channels(color_type: u8) -> usize {
    match color_type {
        0 | 3 => 1,
        2 => 3,
        4 => 2,
        _ => 4,
    }
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i16 + b as i16 - c as i16;
    let pa = (p - a as i16).abs();
    let pb = (p - b as i16).abs();
    let pc = (p - c as i16).abs();
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// Desfaz os filtros de uma imagem (ou passo Adam7) de `rows` linhas de
/// `stride` bytes; `bpp` = bytes por pixel (mínimo 1).
fn unfilter(data: &[u8], rows: usize, stride: usize, bpp: usize) -> Result<Vec<u8>> {
    let mut out = vec![0u8; rows * stride];
    for y in 0..rows {
        let src = data
            .get(y * (stride + 1)..(y + 1) * (stride + 1))
            .ok_or(Error::Truncated)?;
        let (filter, line) = (src[0], &src[1..]);
        let (prev_rows, cur_rows) = out.split_at_mut(y * stride);
        let prev = if y > 0 {
            &prev_rows[(y - 1) * stride..]
        } else {
            &[][..]
        };
        let cur = &mut cur_rows[..stride];
        for x in 0..stride {
            let a = if x >= bpp { cur[x - bpp] } else { 0 };
            let b = if y > 0 { prev[x] } else { 0 };
            let c = if y > 0 && x >= bpp { prev[x - bpp] } else { 0 };
            let pred = match filter {
                0 => 0,
                1 => a,
                2 => b,
                3 => ((a as u16 + b as u16) / 2) as u8,
                4 => paeth(a, b, c),
                f => return Err(Error::Corrupt(format!("filtro {f}"))),
            };
            cur[x] = line[x].wrapping_add(pred);
        }
    }
    Ok(out)
}

struct Palette {
    rgba: Vec<[u8; 4]>,
}

/// Converte as amostras de um pixel para RGBA de 16 bits.
fn sample(raw: &[u8], x: usize, h: &Header, pal: &Palette, trns: Option<[u16; 3]>) -> [u16; 4] {
    let d = h.bit_depth as usize;
    let ch = channels(h.color_type);
    let get = |i: usize| -> u16 {
        if d == 16 {
            u16::from_be_bytes([raw[i * 2], raw[i * 2 + 1]])
        } else if d == 8 {
            raw[i] as u16
        } else {
            let bit = i * d;
            let byte = raw[bit / 8];
            let shift = 8 - d - (bit % 8);
            ((byte >> shift) & ((1 << d) - 1) as u8) as u16
        }
    };
    // Escala para 16 bits replicando os bits (0xF → 0xFFFF).
    let max = (1u32 << d) - 1;
    let up = |v: u16| -> u16 { (v as u32 * 65535 / max) as u16 };
    match h.color_type {
        0 => {
            let g = get(x);
            let a = if trns.is_some_and(|t| t[0] == g) {
                0
            } else {
                65535
            };
            let g = up(g);
            [g, g, g, a]
        }
        2 => {
            let (r, g, b) = (get(x * 3), get(x * 3 + 1), get(x * 3 + 2));
            let a = if trns == Some([r, g, b]) { 0 } else { 65535 };
            [up(r), up(g), up(b), a]
        }
        3 => {
            let idx = get(x) as usize;
            let c = pal.rgba.get(idx).copied().unwrap_or([0, 0, 0, 255]);
            c.map(|v| v as u16 * 257)
        }
        4 => {
            let g = up(get(x * 2));
            [g, g, g, up(get(x * 2 + 1))]
        }
        _ => {
            debug_assert_eq!(ch, 4);
            [
                up(get(x * 4)),
                up(get(x * 4 + 1)),
                up(get(x * 4 + 2)),
                up(get(x * 4 + 3)),
            ]
        }
    }
}

/// Decodifica um PNG.
pub fn decode(data: &[u8]) -> Result<Image> {
    let h = read_header(data)?;
    let mut idat = Vec::new();
    let mut pal = Palette { rgba: Vec::new() };
    let mut trns_key = None;
    let mut trns_alpha: Option<&[u8]> = None;
    for (kind, body) in chunks(data)? {
        match &kind {
            b"PLTE" => {
                pal.rgba = body
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|c| [c[0], c[1], c[2], 255])
                    .collect();
            }
            b"tRNS" => match h.color_type {
                0 if body.len() >= 2 => {
                    trns_key = Some([u16::from_be_bytes([body[0], body[1]]), 0, 0])
                }
                2 if body.len() >= 6 => {
                    trns_key = Some([0, 2, 4].map(|i| u16::from_be_bytes([body[i], body[i + 1]])))
                }
                3 => trns_alpha = Some(body),
                _ => {}
            },
            b"IDAT" => idat.extend_from_slice(body),
            _ => {}
        }
    }
    if let Some(alpha) = trns_alpha {
        for (c, &a) in pal.rgba.iter_mut().zip(alpha) {
            c[3] = a;
        }
    }
    if h.color_type == 3 && pal.rgba.is_empty() {
        return Err(Error::Corrupt("paleta ausente".into()));
    }
    let raw = zlib_decompress(&idat).map_err(|e| Error::Corrupt(e.to_string()))?;
    let (w, ht) = (h.width as usize, h.height as usize);
    let bits_pp = channels(h.color_type) * h.bit_depth as usize;
    let bpp = bits_pp.div_ceil(8).max(1);
    let mut rgba16 = vec![[0u16; 4]; w * ht];
    let passes: &[(usize, usize, usize, usize)] = if h.interlaced {
        &[
            (0, 0, 8, 8),
            (4, 0, 8, 8),
            (0, 4, 4, 8),
            (2, 0, 4, 4),
            (0, 2, 2, 4),
            (1, 0, 2, 2),
            (0, 1, 1, 2),
        ]
    } else {
        &[(0, 0, 1, 1)]
    };
    let mut offset = 0;
    for &(x0, y0, dx, dy) in passes {
        if x0 >= w || y0 >= ht {
            continue;
        }
        let pw = (w - x0).div_ceil(dx);
        let ph = (ht - y0).div_ceil(dy);
        let stride = (pw * bits_pp).div_ceil(8);
        let len = ph * (stride + 1);
        let pass = raw.get(offset..offset + len).ok_or(Error::Truncated)?;
        offset += len;
        let pixels = unfilter(pass, ph, stride, bpp)?;
        for py in 0..ph {
            let line = &pixels[py * stride..(py + 1) * stride];
            for px in 0..pw {
                let (x, y) = (x0 + px * dx, y0 + py * dy);
                rgba16[y * w + x] = sample(line, px, &h, &pal, trns_key);
            }
        }
    }
    let pixels = if h.bit_depth == 16 {
        Pixels::Rgba16(rgba16.into_iter().flatten().collect())
    } else {
        Pixels::Rgba8(
            rgba16
                .into_iter()
                .flatten()
                .map(|v| (v >> 8) as u8)
                .collect(),
        )
    };
    Ok(Image {
        width: h.width,
        height: h.height,
        pixels,
    })
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
    let mut crc = Crc32::new();
    crc.update(kind);
    crc.update(body);
    out.extend_from_slice(&crc.finish().to_be_bytes());
}

/// Aplica o filtro que der a menor soma de diferenças absolutas (heurística
/// recomendada pela especificação).
fn filter_rows(raw: &[u8], rows: usize, stride: usize, bpp: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(rows * (stride + 1));
    let mut cand = vec![0u8; stride];
    let mut best = vec![0u8; stride];
    for y in 0..rows {
        let cur = &raw[y * stride..(y + 1) * stride];
        let prev = (y > 0).then(|| &raw[(y - 1) * stride..y * stride]);
        let mut best_score = u64::MAX;
        let mut best_f = 0u8;
        for f in 0..5u8 {
            for x in 0..stride {
                let a = if x >= bpp { cur[x - bpp] } else { 0 };
                let b = prev.map_or(0, |p| p[x]);
                let c = if x >= bpp {
                    prev.map_or(0, |p| p[x - bpp])
                } else {
                    0
                };
                let pred = match f {
                    0 => 0,
                    1 => a,
                    2 => b,
                    3 => ((a as u16 + b as u16) / 2) as u8,
                    _ => paeth(a, b, c),
                };
                cand[x] = cur[x].wrapping_sub(pred);
            }
            let score: u64 = cand.iter().map(|&v| (v as i8).unsigned_abs() as u64).sum();
            if score < best_score {
                best_score = score;
                best_f = f;
                best.copy_from_slice(&cand);
            }
        }
        out.push(best_f);
        out.extend_from_slice(&best);
    }
    out
}

/// Codifica pixels intercalados. `channels` 3 (RGB) ou 4 (RGBA);
/// `samples` em 8 bits (`bit_depth` 8) ou big-endian de 16 bits (`bit_depth` 16).
fn encode(
    width: u32,
    height: u32,
    channels: usize,
    bit_depth: u8,
    raw: &[u8],
    level: u8,
) -> Vec<u8> {
    let bpp = channels * bit_depth as usize / 8;
    let stride = width as usize * bpp;
    assert_eq!(raw.len(), stride * height as usize, "tamanho dos pixels");
    let mut out = SIGNATURE.to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[bit_depth, if channels == 4 { 6 } else { 2 }, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    // Intenção de renderização perceptual: o arquivo é sRGB.
    chunk(&mut out, b"sRGB", &[0]);
    let filtered = filter_rows(raw, height as usize, stride, bpp);
    let z = zlib_compress(&filtered, level);
    for part in z.chunks(1 << 20) {
        chunk(&mut out, b"IDAT", part);
    }
    chunk(&mut out, b"IEND", &[]);
    out
}

/// PNG RGBA de 8 bits.
pub fn encode_rgba8(width: u32, height: u32, rgba: &[u8], level: u8) -> Vec<u8> {
    encode(width, height, 4, 8, rgba, level)
}

/// PNG RGB de 8 bits (descarta o alfa de uma entrada RGBA).
pub fn encode_rgb8_from_rgba(width: u32, height: u32, rgba: &[u8], level: u8) -> Vec<u8> {
    let rgb: Vec<u8> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    encode(width, height, 3, 8, &rgb, level)
}

/// PNG RGBA de 16 bits.
pub fn encode_rgba16(width: u32, height: u32, rgba: &[u16], level: u8) -> Vec<u8> {
    let bytes: Vec<u8> = rgba.iter().flat_map(|v| v.to_be_bytes()).collect();
    encode(width, height, 4, 16, &bytes, level)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_rgba(w: u32, h: u32) -> Vec<u8> {
        (0..w * h)
            .flat_map(|i| {
                let (x, y) = (i % w, i / w);
                [
                    (x * 7) as u8,
                    (y * 11) as u8,
                    ((x ^ y) * 3) as u8,
                    (255 - x) as u8,
                ]
            })
            .collect()
    }

    #[test]
    fn ida_e_volta_rgba8() {
        let (w, h) = (37, 23);
        let px = test_rgba(w, h);
        let png = encode_rgba8(w, h, &px, 6);
        let img = decode(&png).unwrap();
        assert_eq!((img.width, img.height), (w, h));
        assert_eq!(img.pixels, Pixels::Rgba8(px));
    }

    #[test]
    fn ida_e_volta_rgb8_e_16_bits() {
        let (w, h) = (16, 9);
        let px = test_rgba(w, h);
        let img = decode(&encode_rgb8_from_rgba(w, h, &px, 9)).unwrap();
        let back = img.to_rgba8();
        for (a, b) in back.chunks(4).zip(px.chunks(4)) {
            assert_eq!(&a[..3], &b[..3]);
            assert_eq!(a[3], 255);
        }
        let px16: Vec<u16> = (0..w * h * 4).map(|i| (i * 977) as u16).collect();
        let img = decode(&encode_rgba16(w, h, &px16, 6)).unwrap();
        assert_eq!(img.pixels, Pixels::Rgba16(px16));
    }

    #[test]
    fn cabecalho() {
        let png = encode_rgba8(3, 2, &test_rgba(3, 2), 1);
        let h = read_header(&png).unwrap();
        assert_eq!((h.width, h.height, h.bit_depth, h.color_type), (3, 2, 8, 6));
        assert_eq!(decode(b"GIF89a"), Err(Error::NotPng));
    }

    #[test]
    fn crc_errado_e_detectado() {
        let mut png = encode_rgba8(3, 2, &test_rgba(3, 2), 1);
        png[20] ^= 0xFF;
        assert!(decode(&png).is_err());
    }

    /// Arquivos gerados por outras implementações (ffmpeg, Pillow e um
    /// gerador Adam7), comparados com os pixels que elas mesmas decodificam.
    #[test]
    fn le_arquivos_de_terceiros() {
        let cases: [(&str, &[u8], &[u8]); 10] = [
            (
                "rgb24",
                include_bytes!("../testdata/src_rgb24.png"),
                include_bytes!("../testdata/src_rgb24.rgba"),
            ),
            (
                "rgba",
                include_bytes!("../testdata/src_rgba.png"),
                include_bytes!("../testdata/src_rgba.rgba"),
            ),
            (
                "pal8",
                include_bytes!("../testdata/src_pal8.png"),
                include_bytes!("../testdata/src_pal8.rgba"),
            ),
            (
                "gray",
                include_bytes!("../testdata/src_gray.png"),
                include_bytes!("../testdata/src_gray.rgba"),
            ),
            (
                "gray16",
                include_bytes!("../testdata/src_gray16be.png"),
                include_bytes!("../testdata/src_gray16be.rgba"),
            ),
            (
                "rgb48",
                include_bytes!("../testdata/src_rgb48be.png"),
                include_bytes!("../testdata/src_rgb48be.rgba"),
            ),
            (
                "ya8",
                include_bytes!("../testdata/src_ya8.png"),
                include_bytes!("../testdata/src_ya8.rgba"),
            ),
            (
                "monob",
                include_bytes!("../testdata/src_monob.png"),
                include_bytes!("../testdata/src_monob.rgba"),
            ),
            (
                "adam7",
                include_bytes!("../testdata/adam7_rgba.png"),
                include_bytes!("../testdata/adam7_rgba.rgba"),
            ),
            (
                "pal2+tRNS",
                include_bytes!("../testdata/pal2_trns.png"),
                include_bytes!("../testdata/pal2_trns.rgba"),
            ),
        ];
        for (name, png, expected) in cases {
            let img = decode(png).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!((img.width, img.height), (61, 37), "{name}");
            let got = img.to_rgba8();
            assert_eq!(got.len(), expected.len(), "{name}");
            let worst = got
                .iter()
                .zip(expected)
                .map(|(a, b)| (*a as i32 - *b as i32).abs())
                .max()
                .unwrap();
            assert!(worst <= 1, "{name}: diferença máxima {worst}");
        }
    }
}
