//! MD5 (RFC 1321), for FLAC's signature of the decoded audio.

const S: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
    14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15,
    21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

/// The constants: the integer part of |sin(i + 1)| · 2³².
fn table() -> &'static [u32; 64] {
    static TABLE: std::sync::OnceLock<[u32; 64]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = [0u32; 64];
        for (i, v) in t.iter_mut().enumerate() {
            *v = ((i as f64 + 1.0).sin().abs() * 4_294_967_296.0) as u32;
        }
        t
    })
}

#[derive(Clone, Debug)]
pub struct Md5 {
    state: [u32; 4],
    buffer: Vec<u8>,
    length: u64,
}

impl Default for Md5 {
    fn default() -> Md5 {
        Md5 {
            state: [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476],
            buffer: Vec::with_capacity(64),
            length: 0,
        }
    }
}

impl Md5 {
    pub fn update(&mut self, mut data: &[u8]) {
        self.length += data.len() as u64;
        if !self.buffer.is_empty() {
            let take = (64 - self.buffer.len()).min(data.len());
            self.buffer.extend_from_slice(&data[..take]);
            data = &data[take..];
            if self.buffer.len() == 64 {
                let block: [u8; 64] = self.buffer[..].try_into().expect("64 bytes");
                self.block(&block);
                self.buffer.clear();
            }
        }
        let (blocks, rest) = data.as_chunks::<64>();
        for b in blocks {
            self.block(b);
        }
        self.buffer.extend_from_slice(rest);
    }

    fn block(&mut self, b: &[u8; 64]) {
        let mut m = [0u32; 16];
        for (i, w) in m.iter_mut().enumerate() {
            *w = u32::from_le_bytes([b[i * 4], b[i * 4 + 1], b[i * 4 + 2], b[i * 4 + 3]]);
        }
        let [mut a, mut bb, mut c, mut d] = self.state;
        for (i, (shift, k)) in S.iter().zip(table()).enumerate() {
            let (f, g) = match i / 16 {
                0 => ((bb & c) | (!bb & d), i),
                1 => ((d & bb) | (!d & c), (5 * i + 1) % 16),
                2 => (bb ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (bb | !d), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(*k).wrapping_add(m[g]);
            a = d;
            d = c;
            c = bb;
            bb = bb.wrapping_add(f.rotate_left(*shift));
        }
        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(bb);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
    }

    pub fn finish(mut self) -> [u8; 16] {
        let bits = self.length.wrapping_mul(8);
        let mut pad = vec![0x80u8];
        while (self.buffer.len() + pad.len()) % 64 != 56 {
            pad.push(0);
        }
        pad.extend_from_slice(&bits.to_le_bytes());
        let length = self.length;
        self.update(&pad);
        self.length = length;
        let mut out = [0u8; 16];
        for (i, s) in self.state.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&s.to_le_bytes());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(d: [u8; 16]) -> String {
        d.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn rfc_1321_examples() {
        let md5 = |s: &str| {
            let mut m = Md5::default();
            m.update(s.as_bytes());
            hex(m.finish())
        };
        assert_eq!(md5(""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5("abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            md5("12345678901234567890123456789012345678901234567890123456789012345678901234567890"),
            "57edf4a22be3c955ac49da2e2107b67a"
        );
        // Fed in pieces.
        let mut m = Md5::default();
        for c in "message digest".as_bytes().chunks(3) {
            m.update(c);
        }
        assert_eq!(hex(m.finish()), "f96b697d7cb7938d525a2f31aaf161d0");
    }
}
