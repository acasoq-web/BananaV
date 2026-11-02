//! Prevenção de emulação de start code (H.264 7.4.1 / HEVC 7.4.2).
//!
//! Dentro de uma NAL unit, a sequência `00 00 0x` (x ≤ 3) nunca aparece:
//! o encoder insere `03` depois de cada `00 00` que precederia um byte ≤ 3.

/// Remove os bytes `emulation_prevention_three_byte`, convertendo o payload
/// de uma NAL unit em RBSP.
pub fn unescape_rbsp(nal: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(nal.len());
    let mut zeros = 0;
    for &b in nal {
        if zeros >= 2 && b == 3 {
            zeros = 0;
            continue;
        }
        zeros = if b == 0 { zeros + 1 } else { 0 };
        out.push(b);
    }
    out
}

/// Insere os bytes `03` necessários para que o RBSP possa ir numa NAL unit.
pub fn escape_rbsp(rbsp: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rbsp.len() + rbsp.len() / 64);
    let mut zeros = 0;
    for &b in rbsp {
        if zeros >= 2 && b <= 3 {
            out.push(3);
            zeros = 0;
        }
        zeros = if b == 0 { zeros + 1 } else { 0 };
        out.push(b);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exemplos() {
        assert_eq!(escape_rbsp(&[0, 0, 1]), vec![0, 0, 3, 1]);
        assert_eq!(escape_rbsp(&[0, 0, 0, 0]), vec![0, 0, 3, 0, 0]);
        assert_eq!(escape_rbsp(&[0, 0, 4]), vec![0, 0, 4]);
        assert_eq!(unescape_rbsp(&[0, 0, 3, 1]), vec![0, 0, 1]);
        assert_eq!(unescape_rbsp(&[0, 0, 3, 0, 0, 3, 3]), vec![0, 0, 0, 0, 3]);
    }

    #[test]
    fn ida_e_volta() {
        let mut seed = 12345u32;
        for len in 0..200 {
            let data: Vec<u8> = (0..len)
                .map(|_| {
                    seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
                    // Muitos zeros para exercitar as sequências críticas.
                    [0u8, 0, 0, 1, 2, 3, 0xFF][(seed >> 16) as usize % 7]
                })
                .collect();
            let escaped = escape_rbsp(&data);
            for w in escaped.windows(3) {
                assert!(!(w[0] == 0 && w[1] == 0 && w[2] <= 2), "{escaped:?}");
            }
            assert_eq!(unescape_rbsp(&escaped), data);
        }
    }
}
