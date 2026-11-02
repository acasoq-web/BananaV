//! AIFF (Apple's Audio Interchange File Format): big-endian PCM in `FORM`
//! chunks, the sample rate as an 80-bit extended float.

use std::io::{Seek, SeekFrom, Write};

use crate::{Error, Result};

/// `rate` as an IEEE 754 80-bit extended float (big-endian).
fn extended(rate: u32) -> [u8; 10] {
    let mut out = [0u8; 10];
    if rate == 0 {
        return out;
    }
    let shift = rate.leading_zeros();
    let exponent = 16383 + 31 - shift as u16;
    let mantissa = (rate as u64) << (32 + shift);
    out[..2].copy_from_slice(&exponent.to_be_bytes());
    out[2..].copy_from_slice(&mantissa.to_be_bytes());
    out
}

/// Streaming AIFF writer of 8, 16, 24 or 32-bit integer PCM.
#[derive(Debug)]
pub struct AiffWriter<W: Write + Seek> {
    inner: W,
    bits: u16,
    channels: u16,
    frames: u64,
    data_start: u64,
}

impl<W: Write + Seek> AiffWriter<W> {
    pub fn new(mut inner: W, sample_rate: u32, channels: u16, bits: u16) -> Result<AiffWriter<W>> {
        if !matches!(bits, 8 | 16 | 24 | 32) {
            return Err(Error::Unsupported(format!("AIFF of {bits} bits")));
        }
        let mut h = Vec::with_capacity(64);
        h.extend_from_slice(b"FORM");
        h.extend_from_slice(&0u32.to_be_bytes());
        h.extend_from_slice(b"AIFF");
        h.extend_from_slice(b"COMM");
        h.extend_from_slice(&18u32.to_be_bytes());
        h.extend_from_slice(&channels.to_be_bytes());
        h.extend_from_slice(&0u32.to_be_bytes()); // frames, at the end
        h.extend_from_slice(&bits.to_be_bytes());
        h.extend_from_slice(&extended(sample_rate));
        h.extend_from_slice(b"SSND");
        h.extend_from_slice(&0u32.to_be_bytes());
        h.extend_from_slice(&0u32.to_be_bytes()); // offset
        h.extend_from_slice(&0u32.to_be_bytes()); // block size
        inner.write_all(&h)?;
        Ok(AiffWriter {
            inner,
            bits,
            channels,
            frames: 0,
            data_start: h.len() as u64,
        })
    }

    /// Writes planar frames (one vector per channel, all the same length).
    pub fn write_planar(&mut self, channels: &[Vec<f32>]) -> Result<()> {
        assert_eq!(channels.len(), self.channels as usize, "channel count");
        let n = channels.first().map_or(0, Vec::len);
        let bytes = self.bits as usize / 8;
        let mut buf = Vec::with_capacity(n * bytes * channels.len());
        for i in 0..n {
            for ch in channels {
                let s = ch[i].clamp(-1.0, 1.0) as f64;
                let max = ((1u64 << (self.bits - 1)) - 1) as f64;
                let v = (s * max).round() as i64;
                buf.extend_from_slice(&v.to_be_bytes()[8 - bytes..]);
            }
        }
        self.inner.write_all(&buf)?;
        self.frames += n as u64;
        Ok(())
    }

    pub fn bytes_written(&self) -> u64 {
        self.data_start + self.frames * (self.bits as u64 / 8) * self.channels as u64
    }

    /// Fills in the sizes and returns the output.
    pub fn finish(mut self) -> Result<W> {
        let data = self.frames * (self.bits as u64 / 8) * self.channels as u64;
        if data & 1 == 1 {
            self.inner.write_all(&[0])?;
        }
        let form = self.data_start - 8 + data + (data & 1);
        if form > u32::MAX as u64 {
            return Err(Error::Unsupported("AIFF files end at 4 GB".into()));
        }
        self.inner.seek(SeekFrom::Start(4))?;
        self.inner.write_all(&(form as u32).to_be_bytes())?;
        self.inner.seek(SeekFrom::Start(22))?;
        self.inner.write_all(&(self.frames as u32).to_be_bytes())?;
        self.inner.seek(SeekFrom::Start(self.data_start - 12))?;
        self.inner.write_all(&(data as u32 + 8).to_be_bytes())?;
        self.inner.seek(SeekFrom::End(0))?;
        self.inner.flush()?;
        Ok(self.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_as_extended_floats() {
        // 44100 = 0x400E AC44000000000000.
        assert_eq!(extended(44_100), [0x40, 0x0E, 0xAC, 0x44, 0, 0, 0, 0, 0, 0]);
        assert_eq!(extended(48_000), [0x40, 0x0E, 0xBB, 0x80, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn samples_are_big_endian() {
        let mut w = AiffWriter::new(std::io::Cursor::new(Vec::new()), 48_000, 1, 16).unwrap();
        w.write_planar(&[vec![0.5]]).unwrap();
        let d = w.finish().unwrap().into_inner();
        assert_eq!(&d[..4], b"FORM");
        assert_eq!(u32::from_be_bytes(d[22..26].try_into().unwrap()), 1);
        assert_eq!(&d[d.len() - 2..], &16384i16.to_be_bytes());
    }
}
