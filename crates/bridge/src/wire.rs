//! What BananaV and its 32-bit bridge say to each other through the
//! bridge's standard input and output: frames of a length (u32) and bytes,
//! numbers little-endian, texts and lists of a length first. A request is
//! its kind (a byte) and its arguments; an answer its first byte
//! ([`crate::answer`]) and its values, or the failure's text.

use std::io::{Read, Write};

/// The longest frame taken (a garbled length ends the conversation
/// instead of a huge allocation).
const MAX_FRAME: usize = 256 << 20;

pub fn write_frame(w: &mut impl Write, bytes: &[u8]) -> std::io::Result<()> {
    w.write_all(&(bytes.len() as u32).to_le_bytes())?;
    w.write_all(bytes)?;
    w.flush()
}

pub fn read_frame(r: &mut impl Read) -> std::io::Result<Vec<u8>> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len)?;
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_FRAME {
        return Err(std::io::Error::other("a frame too long"));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

/// Builds a frame's bytes.
#[derive(Debug, Default)]
pub struct Out(pub Vec<u8>);

impl Out {
    pub fn new(kind: u8) -> Out {
        Out(vec![kind])
    }

    pub fn u8(&mut self, v: u8) -> &mut Out {
        self.0.push(v);
        self
    }

    pub fn u32(&mut self, v: u32) -> &mut Out {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn i32(&mut self, v: i32) -> &mut Out {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn i64(&mut self, v: i64) -> &mut Out {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn u64(&mut self, v: u64) -> &mut Out {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn f32(&mut self, v: f32) -> &mut Out {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn f64(&mut self, v: f64) -> &mut Out {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn bytes(&mut self, v: &[u8]) -> &mut Out {
        self.u32(v.len() as u32);
        self.0.extend_from_slice(v);
        self
    }

    pub fn str(&mut self, v: &str) -> &mut Out {
        self.bytes(v.as_bytes())
    }

    pub fn f32s(&mut self, v: &[f32]) -> &mut Out {
        self.u32(v.len() as u32);
        for x in v {
            self.0.extend_from_slice(&x.to_le_bytes());
        }
        self
    }
}

/// Reads a frame's bytes.
#[derive(Debug)]
pub struct In<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> In<'a> {
    pub fn new(b: &'a [u8]) -> In<'a> {
        In { b, at: 0 }
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(s)
    }

    pub fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    pub fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    pub fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    pub fn i64(&mut self) -> Option<i64> {
        Some(i64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    pub fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    pub fn f32(&mut self) -> Option<f32> {
        Some(f32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    pub fn f64(&mut self) -> Option<f64> {
        Some(f64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    pub fn bytes(&mut self) -> Option<Vec<u8>> {
        let n = self.u32()? as usize;
        Some(self.take(n)?.to_vec())
    }

    pub fn str(&mut self) -> Option<String> {
        Some(String::from_utf8_lossy(&self.bytes()?).into_owned())
    }

    pub fn f32s(&mut self) -> Option<Vec<f32>> {
        let n = self.u32()? as usize;
        let raw = self.take(n.checked_mul(4)?)?;
        Some(
            raw.as_chunks::<4>()
                .0
                .iter()
                .map(|c| f32::from_le_bytes(*c))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_go_and_come_back() {
        let mut out = Out::new(7);
        out.u8(1)
            .u32(u32::MAX)
            .i32(-3)
            .i64(-1 << 40)
            .u64(u64::MAX)
            .f32(0.5)
            .f64(-2.25)
            .f32s(&[0.5, -1.0])
            .bytes(&[1, 2, 3])
            .str("ação");
        let mut pipe = Vec::new();
        write_frame(&mut pipe, &out.0).unwrap();
        let frame = read_frame(&mut pipe.as_slice()).unwrap();
        let mut r = In::new(&frame);
        assert_eq!(r.u8(), Some(7));
        assert_eq!(r.u8(), Some(1));
        assert_eq!(r.u32(), Some(u32::MAX));
        assert_eq!(r.i32(), Some(-3));
        assert_eq!(r.i64(), Some(-1 << 40));
        assert_eq!(r.u64(), Some(u64::MAX));
        assert_eq!(r.f32(), Some(0.5));
        assert_eq!(r.f64(), Some(-2.25));
        assert_eq!(r.f32s(), Some(vec![0.5, -1.0]));
        assert_eq!(r.bytes(), Some(vec![1, 2, 3]));
        assert_eq!(r.str().as_deref(), Some("ação"));
        assert_eq!(r.u8(), None, "nothing left");
        // A garbled length ends the conversation.
        let huge = (u32::MAX).to_le_bytes();
        assert!(read_frame(&mut huge.as_slice()).is_err());
    }
}
