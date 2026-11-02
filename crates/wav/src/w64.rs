//! Sony Wave64 (`.w64`): WAVE with 64-bit chunk sizes and GUIDs for chunk
//! ids, for audio files of any length.

use std::io::{Seek, SeekFrom, Write};

use crate::{Error, Result, SampleFormat, WAVE_FORMAT_IEEE_FLOAT, WAVE_FORMAT_PCM, encode_le};

/// A GUID as Wave64 stores it (the fields of the structure little-endian).
const fn guid(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> [u8; 16] {
    let a = d1.to_le_bytes();
    let b = d2.to_le_bytes();
    let c = d3.to_le_bytes();
    [
        a[0], a[1], a[2], a[3], b[0], b[1], c[0], c[1], d4[0], d4[1], d4[2], d4[3], d4[4], d4[5],
        d4[6], d4[7],
    ]
}

pub(crate) const RIFF: [u8; 16] = guid(
    0x6666_6972,
    0x912E,
    0x11CF,
    [0xA5, 0xD6, 0x28, 0xDB, 0x04, 0xC1, 0x00, 0x00],
);
pub(crate) const WAVE: [u8; 16] = guid(
    0x6576_6177,
    0xACF3,
    0x11D3,
    [0x8C, 0xD1, 0x00, 0xC0, 0x4F, 0x8E, 0xDB, 0x8A],
);
pub(crate) const FMT: [u8; 16] = guid(
    0x2074_6D66,
    0xACF3,
    0x11D3,
    [0x8C, 0xD1, 0x00, 0xC0, 0x4F, 0x8E, 0xDB, 0x8A],
);
pub(crate) const DATA: [u8; 16] = guid(
    0x6174_6164,
    0xACF3,
    0x11D3,
    [0x8C, 0xD1, 0x00, 0xC0, 0x4F, 0x8E, 0xDB, 0x8A],
);

/// Streaming Wave64 writer; the sizes are filled in by [`W64Writer::finish`].
#[derive(Debug)]
pub struct W64Writer<W: Write + Seek> {
    inner: W,
    format: SampleFormat,
    channels: u16,
    data_bytes: u64,
    data_start: u64,
}

impl<W: Write + Seek> W64Writer<W> {
    pub fn new(
        mut inner: W,
        sample_rate: u32,
        channels: u16,
        format: SampleFormat,
    ) -> Result<W64Writer<W>> {
        let (tag, bits) = match format {
            SampleFormat::Int(b @ (8 | 16 | 24 | 32)) => (WAVE_FORMAT_PCM, b),
            SampleFormat::Float(32) => (WAVE_FORMAT_IEEE_FLOAT, 32),
            other => return Err(Error::Unsupported(format!("writing {other:?}"))),
        };
        let block = channels as u32 * bits as u32 / 8;
        let mut h = Vec::with_capacity(128);
        h.extend_from_slice(&RIFF);
        h.extend_from_slice(&0u64.to_le_bytes());
        h.extend_from_slice(&WAVE);
        h.extend_from_slice(&FMT);
        h.extend_from_slice(&(24u64 + 16).to_le_bytes());
        h.extend_from_slice(&tag.to_le_bytes());
        h.extend_from_slice(&channels.to_le_bytes());
        h.extend_from_slice(&sample_rate.to_le_bytes());
        h.extend_from_slice(&(sample_rate * block).to_le_bytes());
        h.extend_from_slice(&(block as u16).to_le_bytes());
        h.extend_from_slice(&bits.to_le_bytes());
        // Chunks start on 8-byte boundaries.
        h.extend_from_slice(&DATA);
        h.extend_from_slice(&0u64.to_le_bytes());
        inner.write_all(&h)?;
        Ok(W64Writer {
            inner,
            format,
            channels,
            data_bytes: 0,
            data_start: h.len() as u64,
        })
    }

    /// Writes planar frames (one vector per channel, all the same length).
    pub fn write_planar(&mut self, channels: &[Vec<f32>]) -> Result<()> {
        assert_eq!(channels.len(), self.channels as usize, "channel count");
        let buf = encode_le(self.format, channels);
        self.inner.write_all(&buf)?;
        self.data_bytes += buf.len() as u64;
        Ok(())
    }

    /// Bytes of the file so far.
    pub fn bytes_written(&self) -> u64 {
        self.data_start + self.data_bytes
    }

    /// Fills in the sizes and returns the output.
    pub fn finish(mut self) -> Result<W> {
        let pad = (8 - self.data_bytes % 8) % 8;
        crate::write_zeros(&mut self.inner, pad)?;
        let total = self.data_start + self.data_bytes + pad;
        self.inner.seek(SeekFrom::Start(16))?;
        self.inner.write_all(&total.to_le_bytes())?;
        self.inner.seek(SeekFrom::Start(self.data_start - 8))?;
        self.inner
            .write_all(&(24 + self.data_bytes).to_le_bytes())?;
        self.inner.seek(SeekFrom::End(0))?;
        self.inner.flush()?;
        Ok(self.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_carry_their_guids_and_sizes() {
        let mut w = W64Writer::new(
            std::io::Cursor::new(Vec::new()),
            48_000,
            2,
            SampleFormat::Int(16),
        )
        .unwrap();
        w.write_planar(&[vec![0.5; 3], vec![-0.5; 3]]).unwrap();
        let d = w.finish().unwrap().into_inner();
        assert_eq!(&d[..4], b"riff");
        assert_eq!(
            u64::from_le_bytes(d[16..24].try_into().unwrap()),
            d.len() as u64
        );
        assert_eq!(&d[24..28], b"wave");
        assert_eq!(&d[40..44], b"fmt ");
        let data = 40 + 40;
        assert_eq!(&d[data..data + 4], b"data");
        assert_eq!(
            u64::from_le_bytes(d[data + 16..data + 24].try_into().unwrap()),
            24 + 12
        );
        assert_eq!(d.len() % 8, 0);
    }
}
