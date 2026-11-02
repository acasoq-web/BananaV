//! H.264 and HEVC streams as encoders give them (Annex B: NAL units after
//! start codes) turned into MP4 samples (NAL units after their length) and
//! decoder configuration records (ISO/IEC 14496-15: `avcC`, `hvcC`).

use bv_bitstream::{BitReader, unescape_rbsp};

/// The NAL units of an Annex B byte stream (without their start codes).
pub fn split_annex_b(data: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            starts.push(i + 3);
            i += 3;
        } else {
            i += 1;
        }
    }
    let mut out = Vec::with_capacity(starts.len());
    for (k, &s) in starts.iter().enumerate() {
        let mut end = starts.get(k + 1).map_or(data.len(), |&n| n - 3);
        // The zero before a 4-byte start code, and trailing zeros, belong to
        // no unit.
        while end > s && data[end - 1] == 0 {
            end -= 1;
        }
        if end > s {
            out.push(&data[s..end]);
        }
    }
    if starts.is_empty() && !data.is_empty() {
        out.push(data);
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Flavor {
    Avc,
    Hevc,
}

/// What a unit is to the writer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unit {
    /// A parameter set (`kind` is the NAL unit type).
    Parameter(u8),
    /// An access unit delimiter: dropped.
    Delimiter,
    /// Picture data, SEI…: kept in the sample.
    Data,
}

pub(crate) fn classify(flavor: Flavor, nal: &[u8]) -> Unit {
    let Some(&b) = nal.first() else {
        return Unit::Delimiter;
    };
    match flavor {
        Flavor::Avc => match b & 0x1F {
            7 | 8 => Unit::Parameter(b & 0x1F),
            9 => Unit::Delimiter,
            _ => Unit::Data,
        },
        Flavor::Hevc => match (b >> 1) & 0x3F {
            32..=34 => Unit::Parameter((b >> 1) & 0x3F),
            35 => Unit::Delimiter,
            _ => Unit::Data,
        },
    }
}

/// `avcC`: AVCDecoderConfigurationRecord.
pub(crate) fn avcc(sps: &[Vec<u8>], pps: &[Vec<u8>]) -> Vec<u8> {
    let first = sps.first().map(Vec::as_slice).unwrap_or(&[]);
    let at = |i: usize| first.get(i).copied().unwrap_or(0);
    let mut v = vec![1, at(1), at(2), at(3), 0xFF, 0xE0 | sps.len().min(31) as u8];
    for s in sps.iter().take(31) {
        v.extend_from_slice(&(s.len() as u16).to_be_bytes());
        v.extend_from_slice(s);
    }
    v.push(pps.len().min(255) as u8);
    for p in pps.iter().take(255) {
        v.extend_from_slice(&(p.len() as u16).to_be_bytes());
        v.extend_from_slice(p);
    }
    // High profiles carry the chroma format and bit depths too.
    if matches!(at(1), 100 | 110 | 122 | 144)
        && let Some(f) = avc_sps_format(first)
    {
        v.push(0xFC | f.chroma_format as u8);
        v.push(0xF8 | f.luma_bits_minus8 as u8);
        v.push(0xF8 | f.chroma_bits_minus8 as u8);
        v.push(0); // no SPS extensions
    }
    v
}

struct Format {
    chroma_format: u32,
    luma_bits_minus8: u32,
    chroma_bits_minus8: u32,
}

/// Chroma format and bit depths of an H.264 SPS of a high profile.
fn avc_sps_format(sps: &[u8]) -> Option<Format> {
    let rbsp = unescape_rbsp(sps.get(1..)?);
    let mut r = BitReader::new(&rbsp);
    let profile = r.read_bits(8).ok()?;
    r.skip_bits(16).ok()?; // constraint flags, level
    r.read_ue().ok()?; // seq_parameter_set_id
    if !matches!(
        profile,
        100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134 | 135 | 144
    ) {
        return None;
    }
    let chroma_format = r.read_ue().ok()?;
    if chroma_format == 3 {
        r.read_bit().ok()?;
    }
    Some(Format {
        chroma_format,
        luma_bits_minus8: r.read_ue().ok()?,
        chroma_bits_minus8: r.read_ue().ok()?,
    })
}

/// What `hvcC` repeats of an HEVC SPS.
#[derive(Debug, Default, PartialEq)]
struct HevcSps {
    sub_layers_minus1: u32,
    temporal_id_nesting: bool,
    /// general_profile_space, tier and profile_idc, as one byte.
    profile_byte: u8,
    compatibility: u32,
    constraints: [u8; 6],
    level: u8,
    chroma_format: u32,
    luma_bits_minus8: u32,
    chroma_bits_minus8: u32,
}

fn hevc_sps(sps: &[u8]) -> Option<HevcSps> {
    let rbsp = unescape_rbsp(sps.get(2..)?);
    let mut r = BitReader::new(&rbsp);
    r.read_bits(4).ok()?; // sps_video_parameter_set_id
    let sub_layers_minus1 = r.read_bits(3).ok()?;
    let temporal_id_nesting = r.read_bit().ok()?;
    let profile_byte = r.read_bits(8).ok()? as u8;
    let compatibility = r.read_bits(32).ok()?;
    let mut constraints = [0u8; 6];
    for c in &mut constraints {
        *c = r.read_bits(8).ok()? as u8;
    }
    let level = r.read_bits(8).ok()? as u8;
    let mut profile_present = [false; 8];
    let mut level_present = [false; 8];
    for i in 0..sub_layers_minus1 as usize {
        profile_present[i] = r.read_bit().ok()?;
        level_present[i] = r.read_bit().ok()?;
    }
    if sub_layers_minus1 > 0 {
        for _ in sub_layers_minus1..8 {
            r.read_bits(2).ok()?;
        }
    }
    for i in 0..sub_layers_minus1 as usize {
        if profile_present[i] {
            r.skip_bits(88).ok()?;
        }
        if level_present[i] {
            r.skip_bits(8).ok()?;
        }
    }
    r.read_ue().ok()?; // sps_seq_parameter_set_id
    let chroma_format = r.read_ue().ok()?;
    if chroma_format == 3 {
        r.read_bit().ok()?;
    }
    r.read_ue().ok()?; // width
    r.read_ue().ok()?; // height
    if r.read_bit().ok()? {
        for _ in 0..4 {
            r.read_ue().ok()?;
        }
    }
    Some(HevcSps {
        sub_layers_minus1,
        temporal_id_nesting,
        profile_byte,
        compatibility,
        constraints,
        level,
        chroma_format,
        luma_bits_minus8: r.read_ue().ok()?,
        chroma_bits_minus8: r.read_ue().ok()?,
    })
}

/// `hvcC`: HEVCDecoderConfigurationRecord, with the parameter sets in
/// complete arrays (as `hvc1` requires).
pub(crate) fn hvcc(vps: &[Vec<u8>], sps: &[Vec<u8>], pps: &[Vec<u8>]) -> Vec<u8> {
    let info = sps.first().and_then(|s| hevc_sps(s)).unwrap_or_default();
    let mut v = vec![1, info.profile_byte];
    v.extend_from_slice(&info.compatibility.to_be_bytes());
    v.extend_from_slice(&info.constraints);
    v.push(info.level);
    v.extend_from_slice(&0xF000u16.to_be_bytes()); // min_spatial_segmentation_idc 0
    v.push(0xFC); // parallelismType 0
    v.push(0xFC | info.chroma_format as u8);
    v.push(0xF8 | info.luma_bits_minus8 as u8);
    v.push(0xF8 | info.chroma_bits_minus8 as u8);
    v.extend_from_slice(&0u16.to_be_bytes()); // avgFrameRate
    v.push(
        (((info.sub_layers_minus1 + 1) as u8) << 3) | ((info.temporal_id_nesting as u8) << 2) | 3,
    );
    let arrays: Vec<(u8, &[Vec<u8>])> = [(32u8, vps), (33, sps), (34, pps)]
        .into_iter()
        .filter(|(_, list)| !list.is_empty())
        .collect();
    v.push(arrays.len() as u8);
    for (kind, list) in arrays {
        v.push(0x80 | kind);
        v.extend_from_slice(&(list.len() as u16).to_be_bytes());
        for n in list {
            v.extend_from_slice(&(n.len() as u16).to_be_bytes());
            v.extend_from_slice(n);
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_split_on_three_and_four_byte_start_codes() {
        let s = [
            0, 0, 0, 1, 0x67, 1, 2, 0, 0, 1, 0x68, 3, 0, 0, 0, 1, 0x65, 9, 9,
        ];
        assert_eq!(
            split_annex_b(&s),
            vec![&[0x67, 1, 2][..], &[0x68, 3][..], &[0x65, 9, 9][..]]
        );
        assert_eq!(classify(Flavor::Avc, &[0x67]), Unit::Parameter(7));
        assert_eq!(classify(Flavor::Avc, &[0x09, 0xF0]), Unit::Delimiter);
        assert_eq!(classify(Flavor::Hevc, &[0x40, 0x01]), Unit::Parameter(32));
        assert_eq!(classify(Flavor::Hevc, &[0x26, 0x01]), Unit::Data);
    }

    #[test]
    fn avcc_copies_profile_and_level() {
        let sps = vec![vec![0x67, 77, 0x40, 31, 0xAB]];
        let pps = vec![vec![0x68, 0xCE]];
        let c = avcc(&sps, &pps);
        assert_eq!(&c[..6], &[1, 77, 0x40, 31, 0xFF, 0xE1]);
        assert_eq!(&c[6..8], &[0, 5]);
        assert_eq!(c[13], 1);
        assert_eq!(&c[14..], &[0, 2, 0x68, 0xCE]);
    }
}
