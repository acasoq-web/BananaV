//! Seeded random numbers (effects and transitions draw their patterns
//! from them, so a seed always gives the same pattern).

/// R250, Kirkpatrick and Stoll's generator: a shift register of 250
/// words, each new one the word 250 back with the one 147 back mixed in
/// (x[n] = x[n−250] ^ x[n−147]), seeded by a linear congruential one.
pub(crate) struct R250 {
    x: [u32; 250],
    i: usize,
    j: usize,
}

impl R250 {
    pub(crate) fn new(seed: u32) -> R250 {
        let mut state = seed;
        let mut rand = || {
            state = state.wrapping_mul(214_013).wrapping_add(2_531_011);
            (state >> 16) & 0x7fff
        };
        let mut x = [0u32; 250];
        for v in x.iter_mut().rev() {
            let (r1, r2, r3) = (rand(), rand(), rand());
            *v = r3
                .wrapping_add(r2.wrapping_mul(0x20000))
                .wrapping_add(r1.wrapping_mul(4));
        }
        // 32 words made independent, a bit each.
        let (mut bit, mut mask) = (0x8000_0000u32, u32::MAX);
        let mut k = 3;
        while bit != 0 {
            x[k] = (x[k] & mask) | bit;
            bit >>= 1;
            mask >>= 1;
            k += 7;
        }
        R250 { x, i: 0, j: 103 }
    }

    pub(crate) fn next(&mut self) -> u32 {
        self.x[self.i] ^= self.x[self.j];
        let v = self.x[self.i];
        self.i = (self.i + 1) % 250;
        self.j = (self.j + 1) % 250;
        v
    }

    /// A number from 0 to 1023 ⁄ 1024.
    pub(crate) fn unit(&mut self) -> f64 {
        f64::from(self.next() & 0x3ff) * 0.000_976_562_5
    }

    /// A number from −1023 ⁄ 1024 to 1023 ⁄ 1024.
    pub(crate) fn signed(&mut self) -> f64 {
        f64::from((self.next() as i32) % 1024) * 0.000_976_562_5
    }
}
