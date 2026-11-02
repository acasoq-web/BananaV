//! Bits written most significant first, and start codes on byte
//! boundaries.

#[derive(Debug, Default)]
pub(crate) struct Bits {
    pub out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    /// The `n` low bits of `v` (`n` ≤ 32).
    pub fn put(&mut self, v: u32, n: u32) {
        if n == 0 {
            return;
        }
        let mask = if n == 32 { u32::MAX } else { (1 << n) - 1 };
        self.acc = (self.acc << n) | (v & mask) as u64;
        self.n += n;
        while self.n >= 8 {
            self.n -= 8;
            self.out.push((self.acc >> self.n) as u8);
        }
        self.acc &= (1u64 << self.n) - 1;
    }

    /// A code written as its text of 0s and 1s ("0011"); spaces ignored.
    pub fn code(&mut self, text: &str) {
        for c in text.bytes() {
            match c {
                b'0' => self.put(0, 1),
                b'1' => self.put(1, 1),
                _ => {}
            }
        }
    }

    /// Zeros up to the next byte.
    pub fn align(&mut self) {
        if self.n > 0 {
            self.put(0, 8 - self.n);
        }
    }

    /// A start code: `00 00 01 xx`, aligned.
    pub fn start(&mut self, code: u8) {
        self.align();
        self.out.extend_from_slice(&[0, 0, 1, code]);
    }

    /// Bits written so far.
    pub fn len(&self) -> usize {
        self.out.len() * 8 + self.n as usize
    }

    pub fn finish(mut self) -> Vec<u8> {
        self.align();
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_pack_most_significant_first() {
        let mut b = Bits::default();
        b.put(0b101, 3);
        b.code("1 0000");
        b.put(0xABC, 12);
        b.start(0xB3);
        assert_eq!(b.finish(), [0b1011_0000, 0xAB, 0xC0, 0, 0, 1, 0xB3]);
    }
}
