//! AV1 in MP4 (AV1 Codec ISO Media File Format Binding): samples are the
//! OBUs of a temporal unit without its temporal delimiter; the `av1C` box
//! repeats the sequence header.

use bv_bitstream::BitReader;

const OBU_SEQUENCE_HEADER: u8 = 1;
const OBU_TEMPORAL_DELIMITER: u8 = 2;
const OBU_PADDING: u8 = 15;

/// One OBU: its type and its bytes (header included).
pub(crate) struct Obu<'a> {
    pub kind: u8,
    pub bytes: &'a [u8],
    /// Where the payload starts in `bytes`.
    pub payload: usize,
}

fn leb128(data: &[u8], at: &mut usize) -> Option<u64> {
    let mut v = 0u64;
    for i in 0..8 {
        let b = *data.get(*at)?;
        *at += 1;
        v |= ((b & 0x7F) as u64) << (7 * i);
        if b & 0x80 == 0 {
            return Some(v);
        }
    }
    None
}

/// The OBUs of a low-overhead bitstream (every OBU with its size).
pub(crate) fn obus(data: &[u8]) -> Vec<Obu<'_>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < data.len() {
        let start = at;
        let header = data[at];
        let kind = (header >> 3) & 0xF;
        let extension = header & 0x4 != 0;
        let has_size = header & 0x2 != 0;
        at += 1 + extension as usize;
        let size = if has_size {
            match leb128(data, &mut at) {
                Some(s) => s as usize,
                None => break,
            }
        } else {
            data.len().saturating_sub(at)
        };
        let payload = at - start;
        let end = (at + size).min(data.len());
        out.push(Obu {
            kind,
            bytes: &data[start..end],
            payload,
        });
        at = end;
    }
    out
}

/// A temporal unit as an MP4 sample: no temporal delimiters or padding.
pub(crate) fn sample(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for o in obus(data) {
        if !matches!(o.kind, OBU_TEMPORAL_DELIMITER | OBU_PADDING) {
            out.extend_from_slice(o.bytes);
        }
    }
    out
}

/// The sequence header OBU of a temporal unit, if it has one.
pub(crate) fn sequence_header(data: &[u8]) -> Option<Vec<u8>> {
    obus(data)
        .into_iter()
        .find(|o| o.kind == OBU_SEQUENCE_HEADER)
        .map(|o| o.bytes.to_vec())
}

/// `av1C`: the profile, level and tier of the sequence header, the
/// sampling of `bit_depth` 4:2:0 video, and the header itself.
pub(crate) fn av1c(sequence_header: &[u8], bit_depth: u8) -> Vec<u8> {
    let (profile, level, tier) = obus(sequence_header)
        .first()
        .and_then(|o| profile_level_tier(&o.bytes[o.payload..]))
        .unwrap_or((0, 8, 0));
    let high_bitdepth = bit_depth > 8;
    let twelve_bit = bit_depth > 10;
    let mut v = vec![
        0x81, // marker, version 1
        (profile << 5) | level,
        (tier << 7) | ((high_bitdepth as u8) << 6) | ((twelve_bit as u8) << 5) | 0b0000_1100,
        0, // no initial presentation delay
    ];
    v.extend_from_slice(sequence_header);
    v
}

/// seq_profile, seq_level_idx[0] and seq_tier[0] of a sequence header.
fn profile_level_tier(payload: &[u8]) -> Option<(u8, u8, u8)> {
    let mut r = BitReader::new(payload);
    let profile = r.read_bits(3).ok()? as u8;
    r.read_bit().ok()?; // still_picture
    if r.read_bit().ok()? {
        // reduced_still_picture_header
        return Some((profile, r.read_bits(5).ok()? as u8, 0));
    }
    let timing_info = r.read_bit().ok()?;
    if timing_info {
        r.skip_bits(64).ok()?;
        if r.read_bit().ok()? {
            // num_ticks_per_picture_minus_1: uvlc
            let mut zeros = 0;
            while !r.read_bit().ok()? {
                zeros += 1;
                if zeros > 32 {
                    return None;
                }
            }
            r.skip_bits(zeros).ok()?;
        }
        // decoder_model_info_present_flag
        if r.read_bit().ok()? {
            r.skip_bits(5 + 32 + 5 + 5).ok()?;
        }
    }
    r.read_bit().ok()?; // initial_display_delay_present_flag
    r.read_bits(5).ok()?; // operating_points_cnt_minus_1
    r.read_bits(12).ok()?; // operating_point_idc[0]
    let level = r.read_bits(5).ok()? as u8;
    let tier = if level > 7 {
        r.read_bit().ok()? as u8
    } else {
        0
    };
    Some((profile, level, tier))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporal_delimiters_leave_the_sample() {
        // TD, sequence header (2 bytes), frame (3 bytes).
        let tu = [0x12, 0x00, 0x0A, 0x02, 0x00, 0x00, 0x32, 0x03, 1, 2, 3];
        assert_eq!(
            sample(&tu),
            vec![0x0A, 0x02, 0x00, 0x00, 0x32, 0x03, 1, 2, 3]
        );
        assert_eq!(sequence_header(&tu), Some(vec![0x0A, 0x02, 0x00, 0x00]));
    }

    #[test]
    fn level_and_tier_come_from_the_header() {
        // Profile 0, no timing info, one operating point at level 8 (4.0).
        // seq_profile 000, still 0, reduced 0, timing 0, initial delay 0,
        // count 00000, idc 000000000000, level 01000.
        let bits = "000".to_string()
            + "0"
            + "0"
            + "0"
            + "0"
            + "00000"
            + "000000000000"
            + "01000"
            + "0000000";
        let bytes: Vec<u8> = bits
            .as_bytes()
            .chunks(8)
            .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 2).unwrap())
            .collect();
        assert_eq!(profile_level_tier(&bytes), Some((0, 8, 0)));
    }
}
