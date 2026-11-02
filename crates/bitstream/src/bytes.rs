use crate::{Error, Result};

/// Leitor de bytes big-endian sobre um slice.
#[derive(Clone, Debug)]
pub struct ByteReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> ByteReader<'a> {
    pub fn new(data: &'a [u8]) -> ByteReader<'a> {
        ByteReader { data, pos: 0 }
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    pub fn rest(&self) -> &'a [u8] {
        &self.data[self.pos..]
    }

    pub fn seek(&mut self, pos: usize) -> Result<()> {
        if pos > self.data.len() {
            return Err(Error::Eof);
        }
        self.pos = pos;
        Ok(())
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.take(n).map(|_| ())
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.remaining() {
            return Err(Error::Eof);
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.take(N)?.try_into().expect("tamanho conferido"))
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }

    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    pub fn u24(&mut self) -> Result<u32> {
        let [a, b, c] = self.array()?;
        Ok(u32::from_be_bytes([0, a, b, c]))
    }

    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    pub fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    pub fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_be_bytes(self.array()?))
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(self.array()?))
    }

    pub fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_be_bytes(self.array()?))
    }

    pub fn u16_le(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub fn u32_le(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub fn fourcc(&mut self) -> Result<[u8; 4]> {
        self.array()
    }
}

/// Escritor de bytes big-endian.
#[derive(Clone, Debug, Default)]
pub struct ByteWriter {
    data: Vec<u8>,
}

impl ByteWriter {
    pub fn new() -> ByteWriter {
        ByteWriter::default()
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn u8(&mut self, v: u8) {
        self.data.push(v);
    }

    pub fn u16(&mut self, v: u16) {
        self.data.extend_from_slice(&v.to_be_bytes());
    }

    pub fn u24(&mut self, v: u32) {
        debug_assert!(v < 1 << 24);
        self.data.extend_from_slice(&v.to_be_bytes()[1..]);
    }

    pub fn u32(&mut self, v: u32) {
        self.data.extend_from_slice(&v.to_be_bytes());
    }

    pub fn u64(&mut self, v: u64) {
        self.data.extend_from_slice(&v.to_be_bytes());
    }

    pub fn i16(&mut self, v: i16) {
        self.data.extend_from_slice(&v.to_be_bytes());
    }

    pub fn i32(&mut self, v: i32) {
        self.data.extend_from_slice(&v.to_be_bytes());
    }

    pub fn u16_le(&mut self, v: u16) {
        self.data.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u32_le(&mut self, v: u32) {
        self.data.extend_from_slice(&v.to_le_bytes());
    }

    pub fn bytes(&mut self, b: &[u8]) {
        self.data.extend_from_slice(b);
    }

    pub fn zeros(&mut self, n: usize) {
        self.data.resize(self.data.len() + n, 0);
    }

    /// Sobrescreve um u32 já escrito (ex.: o tamanho de uma box ISOBMFF).
    pub fn patch_u32(&mut self, at: usize, v: u32) {
        self.data[at..at + 4].copy_from_slice(&v.to_be_bytes());
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ida_e_volta() {
        let mut w = ByteWriter::new();
        w.u8(1);
        w.u16(0x0203);
        w.u24(0x040506);
        w.u32(0x0708090A);
        w.u64(0x0B0C0D0E0F101112);
        w.i16(-2);
        w.bytes(b"moov");
        w.u32_le(0xAABBCCDD);
        let bytes = w.into_bytes();
        let mut r = ByteReader::new(&bytes);
        assert_eq!(r.u8().unwrap(), 1);
        assert_eq!(r.u16().unwrap(), 0x0203);
        assert_eq!(r.u24().unwrap(), 0x040506);
        assert_eq!(r.u32().unwrap(), 0x0708090A);
        assert_eq!(r.u64().unwrap(), 0x0B0C0D0E0F101112);
        assert_eq!(r.i16().unwrap(), -2);
        assert_eq!(&r.fourcc().unwrap(), b"moov");
        assert_eq!(r.u32_le().unwrap(), 0xAABBCCDD);
        assert!(r.is_empty());
        assert_eq!(r.u8(), Err(Error::Eof));
    }
}
