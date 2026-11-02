use crate::{Error, Result};

/// Leitor de bits MSB primeiro.
#[derive(Clone, Debug)]
pub struct BitReader<'a> {
    data: &'a [u8],
    /// Posição atual, em bits.
    pos: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> BitReader<'a> {
        BitReader { data, pos: 0 }
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn bits_left(&self) -> usize {
        self.data.len() * 8 - self.pos
    }

    pub fn is_byte_aligned(&self) -> bool {
        self.pos.is_multiple_of(8)
    }

    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    pub fn read_bit(&mut self) -> Result<bool> {
        if self.pos >= self.data.len() * 8 {
            return Err(Error::Eof);
        }
        let byte = self.data[self.pos >> 3];
        let bit = (byte >> (7 - (self.pos & 7))) & 1;
        self.pos += 1;
        Ok(bit == 1)
    }

    /// Lê `n` bits (0 a 32) como inteiro sem sinal.
    pub fn read_bits(&mut self, n: u32) -> Result<u32> {
        debug_assert!(n <= 32);
        if n == 0 {
            return Ok(0);
        }
        let v = self.peek_bits(n)?;
        self.pos += n as usize;
        Ok(v)
    }

    /// Lê `n` bits (0 a 64).
    pub fn read_bits_u64(&mut self, n: u32) -> Result<u64> {
        debug_assert!(n <= 64);
        if n <= 32 {
            return self.read_bits(n).map(u64::from);
        }
        let hi = self.read_bits(n - 32)? as u64;
        let lo = self.read_bits(32)? as u64;
        Ok((hi << 32) | lo)
    }

    /// Lê `n` bits com sinal em complemento de dois.
    pub fn read_signed(&mut self, n: u32) -> Result<i32> {
        let v = self.read_bits(n)?;
        if n == 0 {
            return Ok(0);
        }
        let shift = 32 - n;
        Ok(((v << shift) as i32) >> shift)
    }

    /// Olha os próximos `n` bits (0 a 32) sem avançar. Bits além do fim do
    /// buffer contam como erro.
    pub fn peek_bits(&self, n: u32) -> Result<u32> {
        debug_assert!(n <= 32);
        if n == 0 {
            return Ok(0);
        }
        if self.pos + n as usize > self.data.len() * 8 {
            return Err(Error::Eof);
        }
        Ok(self.peek_bits_padded(n))
    }

    /// Como [`peek_bits`](Self::peek_bits), mas completa com zeros após o fim.
    /// Útil para tabelas VLC que olham mais bits do que o código precisa.
    pub fn peek_bits_padded(&self, n: u32) -> u32 {
        debug_assert!((1..=32).contains(&n));
        let byte = self.pos >> 3;
        let mut acc: u64 = 0;
        for i in 0..5 {
            acc = (acc << 8) | *self.data.get(byte + i).unwrap_or(&0) as u64;
        }
        let shift = 40 - (self.pos & 7) as u32 - n;
        ((acc >> shift) & ((1u64 << n) - 1)) as u32
    }

    pub fn skip_bits(&mut self, n: usize) -> Result<()> {
        if self.pos + n > self.data.len() * 8 {
            return Err(Error::Eof);
        }
        self.pos += n;
        Ok(())
    }

    pub fn byte_align(&mut self) {
        self.pos = (self.pos + 7) & !7;
    }

    /// Exp-Golomb sem sinal, `ue(v)` em H.264/HEVC.
    pub fn read_ue(&mut self) -> Result<u32> {
        let mut zeros = 0u32;
        while !self.read_bit()? {
            zeros += 1;
            if zeros > 31 {
                return Err(Error::InvalidExpGolomb);
            }
        }
        if zeros == 0 {
            return Ok(0);
        }
        let rest = self.read_bits(zeros)? as u64;
        Ok(((1u64 << zeros) - 1 + rest) as u32)
    }

    /// Exp-Golomb com sinal, `se(v)`: 0, 1, −1, 2, −2, …
    pub fn read_se(&mut self) -> Result<i32> {
        let k = self.read_ue()? as i64;
        Ok(if k & 1 == 1 { (k + 1) / 2 } else { -(k / 2) } as i32)
    }

    /// Exp-Golomb truncado, `te(v)`, com valor máximo `max`.
    pub fn read_te(&mut self, max: u32) -> Result<u32> {
        if max > 1 {
            self.read_ue()
        } else {
            Ok(!self.read_bit()? as u32)
        }
    }

    /// `more_rbsp_data()` de H.264 7.2: há dados antes do bit de parada final?
    pub fn more_rbsp_data(&self) -> bool {
        let Some(last) = self.data.iter().rposition(|&b| b != 0) else {
            return false;
        };
        let stop_bit = last * 8 + 7 - self.data[last].trailing_zeros() as usize;
        self.pos < stop_bit
    }
}

/// Escritor de bits MSB primeiro.
#[derive(Clone, Debug, Default)]
pub struct BitWriter {
    data: Vec<u8>,
    /// Bits ocupados no último byte (0 = byte cheio ou buffer vazio).
    used: u32,
}

impl BitWriter {
    pub fn new() -> BitWriter {
        BitWriter::default()
    }

    pub fn bit_len(&self) -> usize {
        if self.used == 0 {
            self.data.len() * 8
        } else {
            (self.data.len() - 1) * 8 + self.used as usize
        }
    }

    pub fn is_byte_aligned(&self) -> bool {
        self.used == 0
    }

    pub fn write_bit(&mut self, bit: bool) {
        if self.used == 0 {
            self.data.push(0);
        }
        if bit {
            *self.data.last_mut().unwrap() |= 1 << (7 - self.used);
        }
        self.used = (self.used + 1) & 7;
    }

    /// Escreve os `n` bits menos significativos de `value` (n ≤ 32).
    pub fn write_bits(&mut self, value: u32, n: u32) {
        debug_assert!(n <= 32);
        debug_assert!(
            n == 32 || value >> n == 0,
            "valor {value} não cabe em {n} bits"
        );
        for i in (0..n).rev() {
            self.write_bit((value >> i) & 1 == 1);
        }
    }

    pub fn write_bits_u64(&mut self, value: u64, n: u32) {
        if n > 32 {
            self.write_bits((value >> 32) as u32, n - 32);
            self.write_bits(value as u32, 32);
        } else {
            self.write_bits(value as u32, n);
        }
    }

    pub fn write_ue(&mut self, value: u32) {
        let v = value as u64 + 1;
        let len = 64 - v.leading_zeros();
        self.write_bits(0, len - 1);
        self.write_bits_u64(v, len);
    }

    pub fn write_se(&mut self, value: i32) {
        let k = if value > 0 {
            2 * value as i64 - 1
        } else {
            -2 * value as i64
        };
        self.write_ue(k as u32);
    }

    /// `rbsp_trailing_bits()`: um bit 1 seguido de zeros até alinhar.
    pub fn write_trailing_bits(&mut self) {
        self.write_bit(true);
        self.align_zero();
    }

    pub fn align_zero(&mut self) {
        while self.used != 0 {
            self.write_bit(false);
        }
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        if self.used == 0 {
            self.data.extend_from_slice(bytes);
        } else {
            for &b in bytes {
                self.write_bits(b as u32, 8);
            }
        }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.data
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_basicos() {
        let data = [0b1010_1100, 0b0101_0011, 0xFF];
        let mut r = BitReader::new(&data);
        assert_eq!(r.read_bits(3).unwrap(), 0b101);
        assert_eq!(r.read_bits(7).unwrap(), 0b011_0001);
        assert_eq!(r.peek_bits(6).unwrap(), 0b010011);
        assert_eq!(r.read_signed(3).unwrap(), 0b010);
        assert_eq!(r.bits_left(), 11);
        assert_eq!(r.read_bits(11).unwrap(), 0b011_1111_1111);
        assert_eq!(r.read_bit(), Err(Error::Eof));
    }

    #[test]
    fn exp_golomb_ida_e_volta() {
        let values: Vec<u32> = (0..300).chain([65_535, 1 << 20, u32::MAX - 1]).collect();
        let signed: Vec<i32> = (-150..150).chain([i32::MAX, -i32::MAX]).collect();
        let mut w = BitWriter::new();
        for &v in &values {
            w.write_ue(v);
        }
        for &v in &signed {
            w.write_se(v);
        }
        w.write_trailing_bits();
        let bytes = w.into_bytes();
        let mut r = BitReader::new(&bytes);
        for &v in &values {
            assert_eq!(r.read_ue().unwrap(), v);
        }
        for &v in &signed {
            assert_eq!(r.read_se().unwrap(), v);
        }
        assert!(!r.more_rbsp_data());
    }

    #[test]
    fn exp_golomb_codigos_conhecidos() {
        // ue: 0 → 1, 1 → 010, 2 → 011, 3 → 00100
        let mut r = BitReader::new(&[0b1010_0110, 0b0100_0000]);
        assert_eq!(r.read_ue().unwrap(), 0);
        assert_eq!(r.read_ue().unwrap(), 1);
        assert_eq!(r.read_ue().unwrap(), 2);
        assert_eq!(r.read_ue().unwrap(), 3);
        // se: 00100 = ue 3 → +2
        let mut r = BitReader::new(&[0b0010_0000]);
        assert_eq!(r.read_se().unwrap(), 2);
    }

    #[test]
    fn more_rbsp_data() {
        // Dados: 1 bit "1" de payload, depois o bit de parada e zeros.
        let data = [0b1100_0000];
        let mut r = BitReader::new(&data);
        assert!(r.more_rbsp_data());
        r.read_bit().unwrap();
        assert!(!r.more_rbsp_data());
        // Zeros de enchimento depois do RBSP não contam.
        let data = [0b1000_0000, 0, 0];
        assert!(!BitReader::new(&data).more_rbsp_data());
    }

    #[test]
    fn peek_preenchido() {
        let r = BitReader::new(&[0xF0]);
        assert_eq!(r.peek_bits_padded(16), 0xF000);
        assert!(r.peek_bits(16).is_err());
    }

    #[test]
    fn escritor_desalinhado() {
        let mut w = BitWriter::new();
        w.write_bits(0b101, 3);
        w.write_bytes(&[0xFF]);
        assert_eq!(w.bit_len(), 11);
        w.align_zero();
        assert_eq!(w.into_bytes(), vec![0b1011_1111, 0b1110_0000]);
    }
}
