//! Photoshop documents (PSD and PSB): the merged image stored with "Maximize
//! compatibility" (Photoshop's default) — bitmap, gray, indexed, RGB and
//! CMYK of 1, 8, 16 and 32 bits, raw, RLE or ZIP (with or without
//! prediction); the transparency of the merged result when the file has
//! it. Layers are not composited again; each picture layer can be read on
//! its own, on the document's canvas (`psd_layer_names`, `psd_layer`:
//! gray, RGB or CMYK of 8 or 16 bits, raw, RLE or ZIP, through its layer
//! mask), with its blend mode (`psd_layer_blends`), as Adobe's published
//! file format describes its layer records.

use super::{
    Picture, PictureData, ReadError, Result, checked_size, corrupt, pixel_count, unsupported,
};

fn be16(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

fn be32(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

fn be64(d: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(d.get(at..at + 8)?.try_into().ok()?))
}

fn bad() -> ReadError {
    ReadError::Corrupt("damaged Photoshop document".into())
}

pub(super) fn read(d: &[u8]) -> Result<Picture> {
    let version = be16(d, 4).ok_or_else(bad)?;
    let big = version == 2;
    if version != 1 && !big {
        return unsupported(format!("Photoshop format version {version}"));
    }
    let channels = be16(d, 12).ok_or_else(bad)? as usize;
    let height = be32(d, 14).ok_or_else(bad)? as usize;
    let width = be32(d, 18).ok_or_else(bad)? as usize;
    let depth = be16(d, 22).ok_or_else(bad)? as usize;
    let mode = be16(d, 24).ok_or_else(bad)?;
    if width == 0 || height == 0 || width > 1 << 17 || height > 1 << 17 || channels == 0 {
        return corrupt("Photoshop document size");
    }
    let pixels = pixel_count(width, height)?;
    if !matches!(depth, 1 | 8 | 16 | 32) {
        return unsupported(format!("Photoshop document of {depth} bits"));
    }
    // 0 bitmap, 1 gray, 2 indexed, 3 RGB, 4 CMYK, 8 duotone (shown gray).
    let colors = match mode {
        0 | 1 | 2 | 8 => 1,
        3 => 3,
        4 => 4,
        7 => return unsupported("multichannel Photoshop document"),
        9 => return unsupported("Lab Photoshop document"),
        m => return unsupported(format!("Photoshop color mode {m}")),
    };
    // Every color of the mode has its channel: with fewer, a color would
    // be read from a plane that is not there.
    if channels < colors {
        return corrupt("Photoshop document channels");
    }
    let mut pos = 26;
    let cmap_len = be32(d, pos).ok_or_else(bad)? as usize;
    let palette = d.get(pos + 4..pos + 4 + cmap_len).unwrap_or(&[]).to_vec();
    pos += 4 + cmap_len;
    let res_len = be32(d, pos).ok_or_else(bad)? as usize;
    pos += 4 + res_len;
    // Layer and mask information: a negative layer count says the first
    // extra channel is the merged image's transparency.
    let (layer_len, head) = if big {
        (be64(d, pos).ok_or_else(bad)? as usize, 8)
    } else {
        (be32(d, pos).ok_or_else(bad)? as usize, 4)
    };
    let mut merged_alpha = false;
    if layer_len > 0 {
        let info = pos + head;
        let layer_info_len = if big {
            be64(d, info).unwrap_or(0) as usize
        } else {
            be32(d, info).unwrap_or(0) as usize
        };
        if layer_info_len > 0 {
            let count = be16(d, info + head).unwrap_or(0) as i16;
            merged_alpha = count < 0;
        }
    }
    pos += head + layer_len;
    let compression = be16(d, pos).ok_or_else(bad)?;
    pos += 2;
    let row_bytes = (width * depth).div_ceil(8);
    let plane_bytes = checked_size(row_bytes, height)?;
    // Only the channels needed: the colors and maybe alpha.
    let used = (colors + usize::from(channels > colors)).min(channels);
    let mut planes: Vec<Vec<u8>> = Vec::with_capacity(used);
    match compression {
        0 => {
            for c in 0..used {
                let at = pos + c * plane_bytes;
                planes.push(d.get(at..at + plane_bytes).ok_or_else(bad)?.to_vec());
            }
        }
        1 => {
            // Every row's byte count first (all channels), then the rows.
            let count_size = if big { 4 } else { 2 };
            let counts_at = pos;
            let mut data_at = pos + channels * height * count_size;
            for c in 0..used {
                let mut plane = Vec::with_capacity(plane_bytes);
                for y in 0..height {
                    let i = c * height + y;
                    let n = if big {
                        be32(d, counts_at + i * 4).ok_or_else(bad)? as usize
                    } else {
                        be16(d, counts_at + i * 2).ok_or_else(bad)? as usize
                    };
                    let row = d.get(data_at..data_at + n).ok_or_else(bad)?;
                    let mut out = packbits(row, row_bytes);
                    out.resize(row_bytes, 0);
                    plane.extend_from_slice(&out);
                    data_at += n;
                }
                planes.push(plane);
            }
        }
        // One stream of every channel's plane.
        2 | 3 => {
            let all = unzip(
                d.get(pos..).ok_or_else(bad)?,
                compression == 3,
                row_bytes,
                checked_size(height, channels)?,
                depth,
            )?;
            for c in 0..used {
                planes.push(all[c * plane_bytes..(c + 1) * plane_bytes].to_vec());
            }
        }
        c => return unsupported(format!("Photoshop compression {c}")),
    }
    let sample = |p: &[u8], i: usize| -> f32 {
        match depth {
            1 => {
                let (y, x) = (i / width, i % width);
                let byte = p[y * row_bytes + x / 8];
                // Bitmap: 1 is black.
                if byte >> (7 - x % 8) & 1 != 0 {
                    0.0
                } else {
                    1.0
                }
            }
            8 => p[i] as f32 / 255.0,
            16 => u16::from_be_bytes([p[i * 2], p[i * 2 + 1]]) as f32 / 65535.0,
            _ => f32::from_be_bytes([p[i * 4], p[i * 4 + 1], p[i * 4 + 2], p[i * 4 + 3]]),
        }
    };
    let has_alpha = merged_alpha && planes.len() > colors;
    let n = pixels;
    let mut rgba = vec![0f32; checked_size(n, 4)?];
    for i in 0..n {
        let a = if has_alpha {
            sample(&planes[colors], i)
        } else {
            1.0
        };
        let rgb = match mode {
            2 => {
                let idx = (sample(&planes[0], i) * 255.0).round() as usize;
                let at = |k: usize| {
                    palette
                        .get(k * 256 + idx)
                        .map_or(0.0, |&v| v as f32 / 255.0)
                };
                [at(0), at(1), at(2)]
            }
            3 => [
                sample(&planes[0], i),
                sample(&planes[1], i),
                sample(&planes[2], i),
            ],
            4 => {
                // Photoshop keeps CMYK inverted (1 is no ink).
                let k = sample(&planes[3], i);
                [
                    sample(&planes[0], i) * k,
                    sample(&planes[1], i) * k,
                    sample(&planes[2], i) * k,
                ]
            }
            _ => [sample(&planes[0], i); 3],
        };
        // The merged image is matted with white where it is transparent.
        let rgb = if has_alpha && a > 0.0 && a < 1.0 {
            rgb.map(|c| ((c - (1.0 - a)) / a).clamp(0.0, 1.0))
        } else {
            rgb
        };
        rgba[i * 4..i * 4 + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], a]);
    }
    let data = match depth {
        16 => PictureData::Rgba16(
            rgba.iter()
                .map(|v| (v.clamp(0.0, 1.0) * 65535.0).round() as u16)
                .collect(),
        ),
        // Photoshop's 32-bit documents are linear.
        32 => PictureData::RgbaF32(rgba),
        _ => PictureData::Rgba8(
            rgba.iter()
                .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
                .collect(),
        ),
    };
    Ok(Picture {
        width: width as u32,
        height: height as u32,
        data,
        linear: depth == 32,
        premultiplied: false,
    })
}

/// A ZIP-compressed (zlib) block of `rows` rows of `row_bytes`, its
/// prediction undone when `predicted`: 8 and 16 bits each sample from the
/// one before it in its row, 32 bits the row's bytes so, its floats' bytes
/// laid out as four planes (the highest first).
fn unzip(
    data: &[u8],
    predicted: bool,
    row_bytes: usize,
    rows: usize,
    depth: usize,
) -> Result<Vec<u8>> {
    // No more unpacked than its rows: a few bytes can be made to unpack
    // into gigabytes.
    let want = checked_size(row_bytes, rows)?;
    let mut out = bv_deflate::zlib_decompress_limited(data, want)
        .map_err(|_| ReadError::Corrupt("Photoshop ZIP data".into()))?;
    if out.len() < want {
        return corrupt("Photoshop ZIP data");
    }
    out.truncate(want);
    if !predicted || row_bytes == 0 {
        return Ok(out);
    }
    for row in out.chunks_exact_mut(row_bytes) {
        match depth {
            16 => {
                let mut last = 0u16;
                for s in row.as_chunks_mut::<2>().0 {
                    last = last.wrapping_add(u16::from_be_bytes(*s));
                    *s = last.to_be_bytes();
                }
            }
            32 => {
                for x in 1..row.len() {
                    row[x] = row[x].wrapping_add(row[x - 1]);
                }
                let w = row.len() / 4;
                let planes = row.to_vec();
                for x in 0..w {
                    for b in 0..4 {
                        row[x * 4 + b] = planes[b * w + x];
                    }
                }
            }
            _ => {
                for x in 1..row.len() {
                    row[x] = row[x].wrapping_add(row[x - 1]);
                }
            }
        }
    }
    Ok(out)
}

// ---- Layers ----------------------------------------------------------------------------

/// A picture layer's record (Adobe's "Layer records").
#[derive(Clone, Debug)]
struct LayerRecord {
    top: i32,
    left: i32,
    bottom: i32,
    right: i32,
    /// Its channels: id (0 red or gray, 1 green, 2 blue, −1 transparency,
    /// −2 and −3 masks) and the length of its data.
    channels: Vec<(i16, usize)>,
    opacity: u8,
    /// Its blend mode's key (`norm`, `mul `, `scrn`…).
    blend: [u8; 4],
    name: String,
    /// A group's folder (1, 2) or its end (3): no picture.
    section: u32,
    /// Its layer mask, when it has one turned on.
    mask: Option<LayerMask>,
    /// Where its channels' data begins.
    data_at: usize,
}

impl LayerRecord {
    fn is_picture(&self) -> bool {
        self.section == 0
    }
}

/// A layer's mask (its pixels the channel `channel`, −2 or −3 when a vector
/// mask comes with it): where it lies, and its value outside.
#[derive(Clone, Copy, Debug)]
struct LayerMask {
    channel: i16,
    top: i32,
    left: i32,
    bottom: i32,
    right: i32,
    outside: u8,
}

/// The layer mask data of a layer record (`at` its length): the pixel mask
/// it describes, unless it is turned off.
fn layer_mask(d: &[u8], at: usize, channels: &[(i16, usize)]) -> Option<LayerMask> {
    let size = be32(d, at)? as usize;
    if size < 20 {
        return None;
    }
    let rect = |p: usize| -> Option<(i32, i32, i32, i32)> {
        Some((
            be32(d, p)? as i32,
            be32(d, p + 4)? as i32,
            be32(d, p + 8)? as i32,
            be32(d, p + 12)? as i32,
        ))
    };
    let p = at + 4;
    let (top, left, bottom, right) = rect(p)?;
    let outside = *d.get(p + 16)?;
    let flags = *d.get(p + 17)?;
    let mut mask = LayerMask {
        channel: -2,
        top,
        left,
        bottom,
        right,
        outside,
    };
    let mut disabled = flags & 2 != 0;
    // With a vector mask too, the pixels are channel −3's, after the
    // mask's parameters (when its flags say it has them).
    if size > 20 && channels.iter().any(|&(id, _)| id == -3) {
        let mut q = p + 18;
        if flags & 16 != 0 {
            let params = *d.get(q)?;
            q += 1;
            for (bit, len) in [(1, 1), (2, 8), (4, 1), (8, 8)] {
                if params & bit != 0 {
                    q += len;
                }
            }
        }
        let real_flags = *d.get(q)?;
        let real_outside = *d.get(q + 1)?;
        let (top, left, bottom, right) = rect(q + 2)?;
        mask = LayerMask {
            channel: -3,
            top,
            left,
            bottom,
            right,
            outside: real_outside,
        };
        disabled = real_flags & 2 != 0;
    }
    (!disabled).then_some(mask)
}

/// The document's header fields the layers need.
struct Header {
    big: bool,
    width: usize,
    height: usize,
    depth: usize,
    mode: u16,
    /// Where the layer and mask section begins.
    layers_at: usize,
}

fn header(d: &[u8]) -> Result<Header> {
    let version = be16(d, 4).ok_or_else(bad)?;
    let big = version == 2;
    if version != 1 && !big {
        return unsupported(format!("Photoshop format version {version}"));
    }
    let height = be32(d, 14).ok_or_else(bad)? as usize;
    let width = be32(d, 18).ok_or_else(bad)? as usize;
    let depth = be16(d, 22).ok_or_else(bad)? as usize;
    let mode = be16(d, 24).ok_or_else(bad)?;
    let mut pos = 26;
    pos += 4 + be32(d, pos).ok_or_else(bad)? as usize;
    pos += 4 + be32(d, pos).ok_or_else(bad)? as usize;
    Ok(Header {
        big,
        width,
        height,
        depth,
        mode,
        layers_at: pos,
    })
}

/// The document's layer records, bottom first.
fn layer_records(d: &[u8]) -> Result<(Header, Vec<LayerRecord>)> {
    let h = header(d)?;
    let len_size = if h.big { 8 } else { 4 };
    let read_len = |at: usize| -> Option<usize> {
        if h.big {
            be64(d, at).map(|v| v as usize)
        } else {
            be32(d, at).map(|v| v as usize)
        }
    };
    let section_len = read_len(h.layers_at).ok_or_else(bad)?;
    if section_len == 0 {
        return Ok((h, Vec::new()));
    }
    let info_at = h.layers_at + len_size;
    let info_len = read_len(info_at).unwrap_or(0);
    if info_len == 0 {
        return Ok((h, Vec::new()));
    }
    let mut pos = info_at + len_size;
    let count = (be16(d, pos).ok_or_else(bad)? as i16).unsigned_abs() as usize;
    pos += 2;
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        let top = be32(d, pos).ok_or_else(bad)? as i32;
        let left = be32(d, pos + 4).ok_or_else(bad)? as i32;
        let bottom = be32(d, pos + 8).ok_or_else(bad)? as i32;
        let right = be32(d, pos + 12).ok_or_else(bad)? as i32;
        let n = be16(d, pos + 16).ok_or_else(bad)? as usize;
        pos += 18;
        let mut channels = Vec::with_capacity(n);
        for _ in 0..n {
            let id = be16(d, pos).ok_or_else(bad)? as i16;
            let len = read_len(pos + 2).ok_or_else(bad)?;
            channels.push((id, len));
            pos += 2 + len_size;
        }
        // Blend mode's signature and key, opacity, clipping, flags, filler.
        let blend: [u8; 4] = d
            .get(pos + 4..pos + 8)
            .ok_or_else(bad)?
            .try_into()
            .map_err(|_| bad())?;
        let opacity = *d.get(pos + 8).ok_or_else(bad)?;
        pos += 12;
        let extra = be32(d, pos).ok_or_else(bad)? as usize;
        pos += 4;
        // Its extra data ends where the file does, whatever its length
        // says: its blocks are not looked for past it.
        let extra_end = pos.saturating_add(extra).min(d.len());
        // The layer mask and the blending ranges.
        let mut at = pos;
        let mask = layer_mask(d, at, &channels);
        at += 4 + be32(d, at).ok_or_else(bad)? as usize;
        at += 4 + be32(d, at).ok_or_else(bad)? as usize;
        // Its name: a Pascal string padded to 4 bytes.
        let name_len = *d.get(at).ok_or_else(bad)? as usize;
        let mut name =
            String::from_utf8_lossy(d.get(at + 1..at + 1 + name_len).ok_or_else(bad)?).to_string();
        at += (1 + name_len).div_ceil(4) * 4;
        // Additional information: the Unicode name, the group sections.
        let mut section = 0;
        while at + 12 <= extra_end {
            let (Some(sig), Some(key)) = (d.get(at..at + 4), d.get(at + 4..at + 8)) else {
                break;
            };
            if sig != b"8BIM" && sig != b"8B64" {
                break;
            }
            let len = be32(d, at + 8).ok_or_else(bad)? as usize;
            let data = at + 12;
            match key {
                b"luni" => {
                    // As many letters as it says, and no more than its
                    // block holds (a count of billions was walked through
                    // one by one).
                    let chars = be32(d, data).unwrap_or(0) as usize;
                    let end = data.saturating_add(len).min(extra_end);
                    let letters = d.get(data + 4..end).unwrap_or(&[]);
                    let units: Vec<u16> = letters
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .take(chars)
                        .map(|b| u16::from_be_bytes(*b))
                        .collect();
                    let text = String::from_utf16_lossy(&units);
                    let text = text.trim_end_matches('\0');
                    if !text.is_empty() {
                        name = text.to_string();
                    }
                }
                b"lsct" | b"lsdk" => section = be32(d, data).unwrap_or(0),
                _ => {}
            }
            at = data + len;
        }
        pos = extra_end;
        records.push(LayerRecord {
            top,
            left,
            bottom,
            right,
            channels,
            opacity,
            blend,
            name,
            section,
            mask,
            data_at: 0,
        });
    }
    // The channels' data follows the records, layer after layer.
    for r in &mut records {
        r.data_at = pos;
        pos += r.channels.iter().map(|&(_, len)| len).sum::<usize>();
    }
    Ok((h, records))
}

/// The names of a Photoshop document's picture layers, bottom first
/// (groups' folders and ends left out); none for a flat document.
pub fn psd_layer_names(d: &[u8]) -> Result<Vec<String>> {
    let (_, records) = layer_records(d)?;
    Ok(records
        .iter()
        .filter(|r| r.is_picture())
        .map(|r| r.name.clone())
        .collect())
}

/// The blend modes' keys of a Photoshop document's picture layers
/// (`norm`, `mul `, `scrn`…), in `psd_layer_names`' order.
pub fn psd_layer_blends(d: &[u8]) -> Result<Vec<[u8; 4]>> {
    let (_, records) = layer_records(d)?;
    Ok(records
        .iter()
        .filter(|r| r.is_picture())
        .map(|r| r.blend)
        .collect())
}

/// Picture layer `index` (in `psd_layer_names`' order) on the document's
/// canvas: its pixels where it lies, its transparency, layer mask and
/// opacity in the alpha, nothing elsewhere. 8 and 16 bits, gray, RGB or
/// CMYK, raw, RLE or ZIP.
pub fn psd_layer(d: &[u8], index: usize) -> Result<Picture> {
    let (h, records) = layer_records(d)?;
    let r = records
        .iter()
        .filter(|r| r.is_picture())
        .nth(index)
        .ok_or_else(|| ReadError::Corrupt("no such Photoshop layer".into()))?;
    if !matches!(h.depth, 8 | 16) {
        return unsupported(format!("Photoshop layers of {} bits", h.depth));
    }
    let colors = match h.mode {
        1 => 1,
        3 => 3,
        4 => 4,
        m => return unsupported(format!("Photoshop layers in color mode {m}")),
    };
    // Sizes in 64 bits: a rectangle's sides may be further apart than 32
    // bits count.
    let extent = |lo: i32, hi: i32| (i64::from(hi) - i64::from(lo)).max(0) as usize;
    let lw = extent(r.left, r.right);
    let lh = extent(r.top, r.bottom);
    let bytes = h.depth / 8;
    let mask = r.mask;
    let (w, ht) = (h.width, h.height);
    let canvas = pixel_count(w, ht)?;
    // Each channel's plane over the layer's rectangle (the mask's over
    // its own).
    let mut planes: Vec<(i16, Vec<u8>)> = Vec::new();
    let mut at = r.data_at;
    for &(id, len) in &r.channels {
        let block = d.get(at..at + len).ok_or_else(bad)?;
        at += len;
        let (pw, ph) = match mask.filter(|m| m.channel == id) {
            Some(m) => (extent(m.left, m.right), extent(m.top, m.bottom)),
            None if id >= -1 => (lw, lh),
            None => continue,
        };
        if pw == 0 || ph == 0 || block.len() < 2 {
            continue;
        }
        let (lw, lh) = (pw, ph);
        // A layer may be larger than the canvas, within the budget.
        let plane_bytes = checked_size(pixel_count(lw, lh)?, bytes)?;
        let row_bytes = lw * bytes;
        let compression = be16(block, 0).ok_or_else(bad)?;
        let data = &block[2..];
        let plane = match compression {
            0 => data.get(..plane_bytes).ok_or_else(bad)?.to_vec(),
            1 => {
                let count_size = if h.big { 4 } else { 2 };
                let mut out = Vec::with_capacity(plane_bytes);
                let mut data_at = lh * count_size;
                for y in 0..lh {
                    let n = if h.big {
                        be32(data, y * 4).ok_or_else(bad)? as usize
                    } else {
                        be16(data, y * 2).ok_or_else(bad)? as usize
                    };
                    let row = data.get(data_at..data_at + n).ok_or_else(bad)?;
                    let mut unpacked = packbits(row, row_bytes);
                    unpacked.resize(row_bytes, 0);
                    out.extend_from_slice(&unpacked);
                    data_at += n;
                }
                out
            }
            2 | 3 => unzip(data, compression == 3, row_bytes, lh, h.depth)?,
            c => return unsupported(format!("Photoshop compression {c}")),
        };
        planes.push((id, plane));
    }
    let plane = |id: i16| planes.iter().find(|(i, _)| *i == id).map(|(_, p)| p);
    let sample = |p: &[u8], i: usize| -> u16 {
        match bytes {
            1 => u16::from(p[i]) * 257,
            _ => u16::from_be_bytes([p[i * 2], p[i * 2 + 1]]),
        }
    };
    let mut rgba = vec![0u16; checked_size(canvas, 4)?];
    let opacity = u32::from(r.opacity);
    // Only the part of the layer on the canvas is walked: its rectangle
    // may be billions of pixels each way. The columns (or rows) of a
    // layer `len` long from `from` that are on a canvas `size` long.
    let on_canvas = |from: i32, len: usize, size: usize| {
        let first = (-i64::from(from)).clamp(0, len as i64) as usize;
        let last = (size as i64 - i64::from(from)).clamp(0, len as i64) as usize;
        first..last.max(first)
    };
    for y in on_canvas(r.top, lh, ht) {
        let cy = (i64::from(r.top) + y as i64) as usize;
        for x in on_canvas(r.left, lw, w) {
            let cx = (i64::from(r.left) + x as i64) as usize;
            let i = y * lw + x;
            let c = |id: i16| plane(id).map_or(0, |p| sample(p, i));
            let (red, green, blue) = match colors {
                1 => {
                    let g = c(0);
                    (g, g, g)
                }
                // CMYK kept inverted (65535 is no ink), black over each.
                4 => {
                    let k = u32::from(c(3));
                    let ink = |v: u16| (u32::from(v) * k / 65535) as u16;
                    (ink(c(0)), ink(c(1)), ink(c(2)))
                }
                _ => (c(0), c(1), c(2)),
            };
            let alpha = plane(-1).map_or(65535, |p| sample(p, i));
            // The layer mask: its pixel there, else its value outside.
            let masked = match mask {
                Some(m) => {
                    let (mx, my) = (cx as i64 - i64::from(m.left), cy as i64 - i64::from(m.top));
                    let mw = (i64::from(m.right) - i64::from(m.left)).max(0);
                    let inside = mx >= 0 && my >= 0 && mx < mw && (cy as i64) < i64::from(m.bottom);
                    match plane(m.channel) {
                        Some(p) if inside => u32::from(sample(p, (my * mw + mx) as usize)),
                        _ => u32::from(m.outside) * 257,
                    }
                }
                None => 65535,
            };
            let alpha =
                (u64::from(alpha) * u64::from(masked) / 65535 * u64::from(opacity) / 255) as u16;
            let o = (cy * w + cx) * 4;
            rgba[o..o + 4].copy_from_slice(&[red, green, blue, alpha]);
        }
    }
    let data = if bytes == 1 {
        PictureData::Rgba8(rgba.iter().map(|v| (v >> 8) as u8).collect())
    } else {
        PictureData::Rgba16(rgba)
    };
    Ok(Picture {
        width: w as u32,
        height: ht as u32,
        data,
        linear: false,
        premultiplied: false,
    })
}

fn packbits(data: &[u8], max: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(max);
    let mut i = 0;
    while i < data.len() && out.len() < max {
        let n = data[i] as i8;
        i += 1;
        if n >= 0 {
            let end = (i + n as usize + 1).min(data.len());
            out.extend_from_slice(&data[i..end]);
            i = end;
        } else if n != -128 {
            let Some(&b) = data.get(i) else { break };
            out.extend(std::iter::repeat_n(b, (1 - n as isize) as usize));
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::read::PictureData;

    /// A test layer: its name, its rectangle (top, left, bottom, right) and its
    /// red, green, blue and alpha planes.
    type TestLayer<'a> = (&'a str, [i32; 4], [&'a [u8]; 4]);

    /// A 2×2 RGB document with two layers, raw: "Bg", all red, and "Dot",
    /// one green pixel at (1, 0) half transparent.
    fn layered() -> Vec<u8> {
        let mut d = b"8BPS".to_vec();
        d.extend_from_slice(&1u16.to_be_bytes());
        d.extend_from_slice(&[0; 6]);
        d.extend_from_slice(&3u16.to_be_bytes()); // channels
        d.extend_from_slice(&2u32.to_be_bytes()); // height
        d.extend_from_slice(&2u32.to_be_bytes()); // width
        d.extend_from_slice(&8u16.to_be_bytes());
        d.extend_from_slice(&3u16.to_be_bytes()); // RGB
        d.extend_from_slice(&0u32.to_be_bytes()); // color mode data
        d.extend_from_slice(&0u32.to_be_bytes()); // resources
        // The layers: (name, rect, R, G, B, A planes).
        let layers: [TestLayer<'_>; 2] = [
            ("Bg", [0, 0, 2, 2], [&[255; 4], &[0; 4], &[0; 4], &[255; 4]]),
            ("Dot", [0, 1, 1, 2], [&[0], &[255], &[0], &[128]]),
        ];
        let mut records = Vec::new();
        let mut data = Vec::new();
        for (name, rect, planes) in layers {
            for v in rect {
                records.extend_from_slice(&v.to_be_bytes());
            }
            records.extend_from_slice(&4u16.to_be_bytes());
            for (id, p) in [0i16, 1, 2, -1].into_iter().zip(planes) {
                records.extend_from_slice(&id.to_be_bytes());
                records.extend_from_slice(&(2 + p.len() as u32).to_be_bytes());
                data.extend_from_slice(&0u16.to_be_bytes());
                data.extend_from_slice(p);
            }
            records.extend_from_slice(b"8BIMnorm");
            records.extend_from_slice(&[255, 0, 0, 0]);
            let padded = (1 + name.len()).div_ceil(4) * 4;
            records.extend_from_slice(&(8 + padded as u32).to_be_bytes());
            records.extend_from_slice(&0u32.to_be_bytes()); // mask
            records.extend_from_slice(&0u32.to_be_bytes()); // ranges
            let mut pascal = vec![name.len() as u8];
            pascal.extend_from_slice(name.as_bytes());
            pascal.resize(padded, 0);
            records.extend_from_slice(&pascal);
        }
        let mut info = 2i16.to_be_bytes().to_vec();
        info.extend_from_slice(&records);
        info.extend_from_slice(&data);
        let section = 4 + info.len() + 4;
        d.extend_from_slice(&(section as u32).to_be_bytes());
        d.extend_from_slice(&(info.len() as u32).to_be_bytes());
        d.extend_from_slice(&info);
        d.extend_from_slice(&0u32.to_be_bytes()); // global mask
        // The merged image, raw.
        d.extend_from_slice(&0u16.to_be_bytes());
        d.extend_from_slice(&[255, 0, 255, 255, 0, 255, 0, 0, 0, 0, 0, 0]);
        d
    }

    #[test]
    fn layers_come_out_on_the_canvas() {
        let d = layered();
        assert_eq!(super::psd_layer_names(&d).unwrap(), ["Bg", "Dot"]);
        let bg = super::psd_layer(&d, 0).unwrap();
        let PictureData::Rgba8(px) = bg.data else {
            panic!()
        };
        assert_eq!(&px[..4], &[255, 0, 0, 255]);
        let dot = super::psd_layer(&d, 1).unwrap();
        assert_eq!((dot.width, dot.height), (2, 2), "the canvas");
        let PictureData::Rgba8(px) = dot.data else {
            panic!()
        };
        assert_eq!(&px[4..8], &[0, 255, 0, 128], "its pixel, half seen");
        assert_eq!(px[3], 0, "nothing elsewhere");
        // The merged image still reads.
        assert_eq!(super::read(&d).unwrap().width, 2);
    }

    /// A layer of `document`: its name, rectangle (top, left, bottom,
    /// right), blend mode, channels (id, compression, data) and mask
    /// (rectangle, value outside).
    struct Layer<'a> {
        name: &'a str,
        rect: [i32; 4],
        blend: &'a [u8; 4],
        channels: Vec<(i16, u16, Vec<u8>)>,
        mask: Option<([i32; 4], u8)>,
    }

    /// A 2×2 document of 8 bits in color `mode` (`colors` channels) with
    /// `layers`, its merged image raw and black.
    fn document(mode: u16, colors: u16, layers: &[Layer<'_>]) -> Vec<u8> {
        let mut d = b"8BPS".to_vec();
        d.extend_from_slice(&1u16.to_be_bytes());
        d.extend_from_slice(&[0; 6]);
        d.extend_from_slice(&colors.to_be_bytes());
        d.extend_from_slice(&2u32.to_be_bytes());
        d.extend_from_slice(&2u32.to_be_bytes());
        d.extend_from_slice(&8u16.to_be_bytes());
        d.extend_from_slice(&mode.to_be_bytes());
        d.extend_from_slice(&0u32.to_be_bytes());
        d.extend_from_slice(&0u32.to_be_bytes());
        let mut records = Vec::new();
        let mut data = Vec::new();
        for l in layers {
            for v in l.rect {
                records.extend_from_slice(&v.to_be_bytes());
            }
            records.extend_from_slice(&(l.channels.len() as u16).to_be_bytes());
            for (id, compression, bytes) in &l.channels {
                records.extend_from_slice(&id.to_be_bytes());
                records.extend_from_slice(&(2 + bytes.len() as u32).to_be_bytes());
                data.extend_from_slice(&compression.to_be_bytes());
                data.extend_from_slice(bytes);
            }
            records.extend_from_slice(b"8BIM");
            records.extend_from_slice(l.blend);
            records.extend_from_slice(&[255, 0, 0, 0]);
            let mut mask = Vec::new();
            if let Some((rect, outside)) = l.mask {
                for v in rect {
                    mask.extend_from_slice(&v.to_be_bytes());
                }
                mask.extend_from_slice(&[outside, 0, 0, 0]);
            }
            let padded = (1 + l.name.len()).div_ceil(4) * 4;
            let extra = 4 + mask.len() + 4 + padded;
            records.extend_from_slice(&(extra as u32).to_be_bytes());
            records.extend_from_slice(&(mask.len() as u32).to_be_bytes());
            records.extend_from_slice(&mask);
            records.extend_from_slice(&0u32.to_be_bytes()); // ranges
            let mut pascal = vec![l.name.len() as u8];
            pascal.extend_from_slice(l.name.as_bytes());
            pascal.resize(padded, 0);
            records.extend_from_slice(&pascal);
        }
        let mut info = (layers.len() as i16).to_be_bytes().to_vec();
        info.extend_from_slice(&records);
        info.extend_from_slice(&data);
        d.extend_from_slice(&((4 + info.len() + 4) as u32).to_be_bytes());
        d.extend_from_slice(&(info.len() as u32).to_be_bytes());
        d.extend_from_slice(&info);
        d.extend_from_slice(&0u32.to_be_bytes()); // global mask
        d.extend_from_slice(&0u16.to_be_bytes());
        d.extend(std::iter::repeat_n(0u8, colors as usize * 4));
        d
    }

    /// Rows of 8 bits compressed as Photoshop's ZIP with prediction: each
    /// byte the difference from the one before it in its row.
    fn zip_predicted(rows: &[&[u8]]) -> Vec<u8> {
        let mut deltas = Vec::new();
        for row in rows {
            let mut last = 0u8;
            for &v in *row {
                deltas.push(v.wrapping_sub(last));
                last = v;
            }
        }
        bv_deflate::zlib_compress(&deltas, 6)
    }

    /// A layer compressed with ZIP and prediction, through a mask over its
    /// top row (its right pixel hidden, the bottom row outside it), keeps
    /// its blend mode.
    #[test]
    fn zip_layers_masks_and_blend_modes() {
        let plane = |a: u8, b: u8, c: u8, e: u8| zip_predicted(&[&[a, b], &[c, e]]);
        let d = document(
            3,
            3,
            &[Layer {
                name: "Shade",
                rect: [0, 0, 2, 2],
                blend: b"mul ",
                channels: vec![
                    (0, 3, plane(10, 200, 30, 40)),
                    (1, 3, plane(50, 60, 70, 80)),
                    (2, 2, bv_deflate::zlib_compress(&[1, 2, 3, 4], 6)),
                    (-1, 0, vec![255; 4]),
                    (-2, 0, vec![255, 0]),
                ],
                mask: Some(([0, 0, 1, 2], 0)),
            }],
        );
        assert_eq!(super::psd_layer_blends(&d).unwrap(), [*b"mul "]);
        let p = super::psd_layer(&d, 0).unwrap();
        let PictureData::Rgba8(px) = p.data else {
            panic!()
        };
        assert_eq!(&px[..4], &[10, 50, 1, 255]);
        assert_eq!(&px[4..8], &[200, 60, 2, 0], "hidden by the mask");
        assert_eq!(&px[8..12], &[30, 70, 3, 0], "outside the mask");
        assert_eq!(&px[12..], &[40, 80, 4, 0]);
    }

    /// A CMYK layer (inverted: 255 no ink): magenta ink alone is magenta.
    #[test]
    fn cmyk_layers() {
        let d = document(
            4,
            4,
            &[Layer {
                name: "Ink",
                rect: [0, 0, 1, 1],
                blend: b"norm",
                channels: vec![
                    (0, 0, vec![255]),
                    (1, 0, vec![0]),
                    (2, 0, vec![255]),
                    (3, 0, vec![255]),
                ],
                mask: None,
            }],
        );
        let p = super::psd_layer(&d, 0).unwrap();
        let PictureData::Rgba8(px) = p.data else {
            panic!()
        };
        assert_eq!(&px[..4], &[255, 0, 255, 255]);
    }

    /// Prediction of 16 and 32 bits: each sample from the one before it;
    /// the floats' bytes in planes.
    #[test]
    fn zip_prediction_of_16_and_32_bits() {
        let deltas: Vec<u8> = [1000u16, 24u16]
            .iter()
            .flat_map(|v| v.to_be_bytes())
            .collect();
        let z = bv_deflate::zlib_compress(&deltas, 6);
        let out = super::unzip(&z, true, 4, 1, 16).unwrap();
        // 1000, then 1000 + 24.
        assert_eq!(out, [3, 232, 4, 0]);
        // Two floats, 1.0 and −2.0: their bytes in planes, then deltas.
        let (a, b) = (1.0f32.to_be_bytes(), (-2.0f32).to_be_bytes());
        let planes = [a[0], b[0], a[1], b[1], a[2], b[2], a[3], b[3]];
        let mut last = 0u8;
        let deltas: Vec<u8> = planes
            .iter()
            .map(|&v| {
                let d = v.wrapping_sub(last);
                last = v;
                d
            })
            .collect();
        let z = bv_deflate::zlib_compress(&deltas, 6);
        let out = super::unzip(&z, true, 8, 1, 32).unwrap();
        assert_eq!(f32::from_be_bytes(out[..4].try_into().unwrap()), 1.0);
        assert_eq!(f32::from_be_bytes(out[4..].try_into().unwrap()), -2.0);
    }

    /// A 2×2 RGB document of 8 bits whose file ends in its one layer's
    /// record: the layer's rectangle (top, left, bottom, right), its
    /// channels (id, length of its data), the length its extra data
    /// declares and that data; then `after` (the channels' data).
    fn one_record(
        rect: [i32; 4],
        channels: &[(i16, u32)],
        extra_len: u32,
        extra: &[u8],
        after: &[u8],
    ) -> Vec<u8> {
        let mut d = document(3, 3, &[]);
        // Up to the layer and mask section, which goes on to the end.
        d.truncate(34);
        d.extend_from_slice(&u32::MAX.to_be_bytes());
        d.extend_from_slice(&u32::MAX.to_be_bytes());
        d.extend_from_slice(&1i16.to_be_bytes());
        for v in rect {
            d.extend_from_slice(&v.to_be_bytes());
        }
        d.extend_from_slice(&(channels.len() as u16).to_be_bytes());
        for (id, len) in channels {
            d.extend_from_slice(&id.to_be_bytes());
            d.extend_from_slice(&len.to_be_bytes());
        }
        d.extend_from_slice(b"8BIMnorm");
        d.extend_from_slice(&[255, 0, 0, 0]);
        d.extend_from_slice(&extra_len.to_be_bytes());
        d.extend_from_slice(extra);
        d.extend_from_slice(after);
        d
    }

    /// A record's extra data with no mask, no blending ranges and no name,
    /// then `blocks`.
    fn extra(blocks: &[u8]) -> Vec<u8> {
        let mut e = vec![0u8; 12];
        e.extend_from_slice(blocks);
        e
    }

    #[test]
    fn a_document_with_fewer_channels_than_its_colors_is_refused() {
        // RGB with one channel, CMYK with three.
        for (mode, colors, channels) in [(3, 3, 1u16), (4, 4, 3)] {
            let mut d = document(mode, colors, &[]);
            d[12..14].copy_from_slice(&channels.to_be_bytes());
            assert!(matches!(
                super::read(&d),
                Err(crate::read::ReadError::Corrupt(_))
            ));
        }
    }

    #[test]
    fn a_layer_record_running_past_the_file_is_read_to_its_end() {
        // Its extra data says 65535 bytes; the file ends in the first four
        // of a block.
        let d = one_record([0, 0, 2, 2], &[], 0xffff, &extra(b"8BIM"), &[]);
        assert_eq!(super::psd_layer_names(&d).unwrap(), [""]);
    }

    #[test]
    fn a_layer_name_has_the_letters_its_block_holds() {
        // A Unicode name of 4,294,967,295 letters, two of them there.
        let mut block = b"8BIMluni".to_vec();
        block.extend_from_slice(&8u32.to_be_bytes());
        block.extend_from_slice(&u32::MAX.to_be_bytes());
        block.extend_from_slice(&[0, b'H', 0, b'i']);
        let e = extra(&block);
        let d = one_record([0, 0, 2, 2], &[], e.len() as u32, &e, &[]);
        assert_eq!(super::psd_layer_names(&d).unwrap(), ["Hi"]);
    }

    #[test]
    fn a_layer_far_larger_than_the_canvas_is_walked_inside_it() {
        // A rectangle of 2^31 − 1 each way, without channels: black over
        // the 2×2 canvas, and no walk over what lies outside.
        let e = extra(&[]);
        let d = one_record([0, 0, i32::MAX, i32::MAX], &[], 12, &e, &[]);
        let p = super::psd_layer(&d, 0).unwrap();
        assert_eq!(p.data, PictureData::Rgba8([0, 0, 0, 255].repeat(4)));
        // From the least number to the greatest: the same.
        let d = one_record([i32::MIN, i32::MIN, i32::MAX, i32::MAX], &[], 12, &e, &[]);
        assert!(super::psd_layer(&d, 0).is_ok());
    }

    #[test]
    fn a_picture_over_the_pixel_budget_is_refused() {
        // A layer of 2^31 − 1 each way with a run-length encoded channel.
        let e = extra(&[]);
        let d = one_record(
            [0, 0, i32::MAX, i32::MAX],
            &[(0, 2)],
            12,
            &e,
            &1u16.to_be_bytes(),
        );
        assert!(matches!(
            super::psd_layer(&d, 0),
            Err(crate::read::ReadError::Unsupported(_))
        ));
        // A document of 131072 × 131072, run-length encoded; and a layer
        // of 2×2 on such a canvas.
        let mut d = document(3, 3, &[]);
        d[14..18].copy_from_slice(&(1u32 << 17).to_be_bytes());
        d[18..22].copy_from_slice(&(1u32 << 17).to_be_bytes());
        let at = d.len() - 14;
        d[at..at + 2].copy_from_slice(&1u16.to_be_bytes());
        assert!(matches!(
            super::read(&d),
            Err(crate::read::ReadError::Unsupported(_))
        ));
        let mut d = one_record([0, 0, 2, 2], &[], 12, &e, &[]);
        d[14..18].copy_from_slice(&u32::MAX.to_be_bytes());
        d[18..22].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(matches!(
            super::psd_layer(&d, 0),
            Err(crate::read::ReadError::Unsupported(_))
        ));
    }

    #[test]
    fn zip_data_that_unpacks_past_its_rows_is_refused() {
        // A row of one byte, and 16 MB.
        let bomb = bv_deflate::zlib_compress(&vec![255; 16 << 20], 6);
        assert!(super::unzip(&bomb, false, 1, 1, 8).is_err());
        let one = bv_deflate::zlib_compress(&[255], 6);
        assert_eq!(super::unzip(&one, false, 1, 1, 8).unwrap(), [255]);
    }

    /// A 2×1 RGB document, raw, with a negative layer count and an alpha
    /// channel.
    #[test]
    fn merged_image_with_transparency() {
        let mut d = b"8BPS".to_vec();
        d.extend_from_slice(&1u16.to_be_bytes());
        d.extend_from_slice(&[0; 6]);
        d.extend_from_slice(&4u16.to_be_bytes()); // channels
        d.extend_from_slice(&1u32.to_be_bytes()); // height
        d.extend_from_slice(&2u32.to_be_bytes()); // width
        d.extend_from_slice(&8u16.to_be_bytes());
        d.extend_from_slice(&3u16.to_be_bytes()); // RGB
        d.extend_from_slice(&0u32.to_be_bytes()); // color mode data
        d.extend_from_slice(&0u32.to_be_bytes()); // resources
        // Layer and mask section: layer info of 2 bytes, count −1.
        d.extend_from_slice(&6u32.to_be_bytes());
        d.extend_from_slice(&2u32.to_be_bytes());
        d.extend_from_slice(&(-1i16).to_be_bytes());
        d.extend_from_slice(&0u16.to_be_bytes()); // raw
        // R, G, B, A planes: an opaque red, then black at 50 % stored
        // matted with white (127).
        d.extend_from_slice(&[255, 127, 0, 127, 0, 127, 255, 128]);
        let p = super::read(&d).unwrap();
        let PictureData::Rgba8(px) = p.data else {
            panic!()
        };
        assert_eq!(&px[..4], &[255, 0, 0, 255]);
        assert_eq!(&px[4..], &[0, 0, 0, 128]);
    }
}
