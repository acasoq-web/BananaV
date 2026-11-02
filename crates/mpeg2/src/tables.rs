//! ISO/IEC 13818-2's tables BananaV's encoder uses: the variable-length
//! codes (Annex B) — those it writes; any DCT coefficient missing from
//! `coefficient` goes out escaped — the default intra quantiser matrix
//! and the zigzag scan.

/// Table B-1: macroblock_address_increment 1–33.
pub(crate) const ADDRESS_INCREMENT: [&str; 33] = [
    "1",
    "011",
    "010",
    "0011",
    "0010",
    "00011",
    "00010",
    "0000111",
    "0000110",
    "00001011",
    "00001010",
    "00001001",
    "00001000",
    "00000111",
    "00000110",
    "0000010111",
    "0000010110",
    "0000010101",
    "0000010100",
    "0000010011",
    "0000010010",
    "00000100011",
    "00000100010",
    "00000100001",
    "00000100000",
    "00000011111",
    "00000011110",
    "00000011101",
    "00000011100",
    "00000011011",
    "00000011010",
    "00000011001",
    "00000011000",
];

/// Table B-1's escape: 33 more.
pub(crate) const ADDRESS_ESCAPE: &str = "00000001000";

/// Table B-9: coded_block_pattern (4:2:0), by the pattern's value.
pub(crate) fn coded_block_pattern(cbp: u8) -> &'static str {
    match cbp {
        60 => "111",
        4 => "1101",
        8 => "1100",
        16 => "1011",
        32 => "1010",
        12 => "10011",
        48 => "10010",
        20 => "10001",
        40 => "10000",
        28 => "01111",
        44 => "01110",
        52 => "01101",
        56 => "01100",
        1 => "01011",
        61 => "01010",
        2 => "01001",
        62 => "01000",
        24 => "001111",
        36 => "001110",
        3 => "001101",
        63 => "001100",
        5 => "0010111",
        9 => "0010110",
        17 => "0010101",
        33 => "0010100",
        6 => "0010011",
        10 => "0010010",
        18 => "0010001",
        34 => "0010000",
        7 => "00011111",
        11 => "00011110",
        19 => "00011101",
        35 => "00011100",
        13 => "00011011",
        49 => "00011010",
        21 => "00011001",
        41 => "00011000",
        14 => "00010111",
        50 => "00010110",
        22 => "00010101",
        42 => "00010100",
        15 => "00010011",
        51 => "00010010",
        23 => "00010001",
        43 => "00010000",
        25 => "00001111",
        37 => "00001110",
        26 => "00001101",
        38 => "00001100",
        29 => "00001011",
        45 => "00001010",
        53 => "00001001",
        57 => "00001000",
        30 => "00000111",
        46 => "00000110",
        54 => "00000101",
        58 => "00000100",
        31 => "000000111",
        47 => "000000110",
        55 => "000000101",
        59 => "000000100",
        27 => "000000011",
        39 => "000000010",
        _ => "000000001",
    }
}

/// Table B-10: motion_code 0–16 (a sign bit follows the others).
pub(crate) const MOTION_CODE: [&str; 17] = [
    "1",
    "01",
    "001",
    "0001",
    "000011",
    "0000101",
    "0000100",
    "0000011",
    "000001011",
    "000001010",
    "000001001",
    "0000010001",
    "0000010000",
    "0000001111",
    "0000001110",
    "0000001101",
    "0000001100",
];

/// Table B-12: dct_dc_size_luminance 0–11.
pub(crate) const DC_SIZE_LUMA: [&str; 12] = [
    "100",
    "00",
    "01",
    "101",
    "110",
    "1110",
    "11110",
    "111110",
    "1111110",
    "11111110",
    "111111110",
    "111111111",
];

/// Table B-13: dct_dc_size_chrominance 0–11.
pub(crate) const DC_SIZE_CHROMA: [&str; 12] = [
    "00",
    "01",
    "10",
    "110",
    "1110",
    "11110",
    "111110",
    "1111110",
    "11111110",
    "111111110",
    "1111111110",
    "1111111111",
];

/// Table B-14: the code of (run, level) — a sign bit follows — for the
/// pairs BananaV writes so; others are escaped. (0, 1) is "11" here;
/// a non-intra block's first coefficient writes it "1".
pub(crate) fn coefficient(run: usize, level: u32) -> Option<&'static str> {
    Some(match (run, level) {
        (0, 1) => "11",
        (1, 1) => "011",
        (0, 2) => "0100",
        (2, 1) => "0101",
        (0, 3) => "00101",
        (3, 1) => "00111",
        (4, 1) => "00110",
        (1, 2) => "000110",
        (5, 1) => "000111",
        (6, 1) => "000101",
        (7, 1) => "000100",
        (0, 4) => "0000110",
        (2, 2) => "0000100",
        (8, 1) => "0000111",
        (9, 1) => "0000101",
        (0, 5) => "00100110",
        (0, 6) => "00100001",
        (1, 3) => "00100101",
        (3, 2) => "00100100",
        (10, 1) => "00100111",
        (11, 1) => "00100011",
        (12, 1) => "00100010",
        (13, 1) => "00100000",
        _ => return None,
    })
}

/// End of block.
pub(crate) const EOB: &str = "10";
/// A coefficient escaped: then 6 bits of run, 12 of level.
pub(crate) const ESCAPE: &str = "000001";

/// The default intra quantiser matrix, in raster order.
pub(crate) const INTRA_MATRIX: [u8; 64] = [
    8, 16, 19, 22, 26, 27, 29, 34, //
    16, 16, 22, 24, 27, 29, 34, 37, //
    19, 22, 26, 27, 29, 34, 34, 38, //
    22, 22, 26, 27, 29, 34, 37, 40, //
    22, 26, 27, 29, 32, 35, 40, 48, //
    26, 27, 29, 32, 35, 40, 48, 58, //
    26, 27, 29, 34, 38, 46, 56, 69, //
    27, 29, 35, 38, 46, 56, 69, 83, //
];

/// The zigzag scan: the raster index of each scan position.
pub(crate) const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// A prefix-free code: no code begins another.
    fn prefix_free(codes: &[&str]) -> bool {
        codes.iter().enumerate().all(|(i, a)| {
            codes
                .iter()
                .enumerate()
                .all(|(j, b)| i == j || !b.starts_with(a))
        })
    }

    #[test]
    fn the_codes_are_prefix_free() {
        assert!(prefix_free(&ADDRESS_INCREMENT));
        assert!(prefix_free(&MOTION_CODE));
        assert!(prefix_free(&DC_SIZE_LUMA));
        assert!(prefix_free(&DC_SIZE_CHROMA));
        let cbp: Vec<&str> = (1..64).map(coded_block_pattern).collect();
        assert!(prefix_free(&cbp));
        let mut coefs: Vec<&str> = Vec::new();
        for run in 0..32 {
            for level in 1..41 {
                if let Some(c) = coefficient(run, level) {
                    coefs.push(c);
                }
            }
        }
        coefs.push(EOB);
        coefs.push(ESCAPE);
        assert!(prefix_free(&coefs));
    }

    #[test]
    fn the_zigzag_visits_every_coefficient_once() {
        let mut seen = [false; 64];
        for &i in &ZIGZAG {
            assert!(!seen[i]);
            seen[i] = true;
        }
    }
}
