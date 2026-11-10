//! VST 2 preset files, as other hosts write them (every number big-endian):
//! a program (`.fxp`) — its parameters' values or the plug-in's chunk —
//! or a bank (`.fxb`) of programs, or the plug-in's bank chunk.
//!
//! ```text
//! "CcnK"  size of the rest (i32)  kind: "FxCk" | "FPCh" | "FxBk" | "FBCh"
//! format version (i32)  the plug-in's number (i32)  its version (i32)
//! how many values (or programs) (i32)
//! a program: its name (28 bytes), then its values (f32 each)
//!            or the chunk's size (i32) and the chunk
//! a bank:    the program chosen (i32, format 2) and 124 bytes (128 in
//!            format 1), then its programs (each a program as above)
//!            or the chunk's size (i32) and the chunk
//! ```

use std::path::{Path, PathBuf};

/// The files' extensions: a program, a bank.
pub const PROGRAM_EXTENSION: &str = "fxp";
pub const BANK_EXTENSION: &str = "fxb";

/// What a preset file holds.
#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    Program {
        name: String,
        values: Vec<f32>,
    },
    ProgramChunk {
        name: String,
        chunk: Vec<u8>,
    },
    Bank {
        programs: Vec<Vec<f32>>,
        current: i32,
    },
    BankChunk {
        chunk: Vec<u8>,
        current: i32,
    },
}

/// A preset file: whose (the plug-in's number and version), how many
/// parameters, and what it holds.
#[derive(Clone, Debug, PartialEq)]
pub struct Preset {
    pub unique_id: i32,
    pub version: i32,
    pub params: u32,
    pub body: Body,
}

impl Preset {
    /// Whether it is for the plug-in known by `unique_id`.
    pub fn is_for(&self, unique_id: i32) -> bool {
        self.unique_id == unique_id
    }

    /// A bank (`.fxb`), not a program.
    pub fn is_bank(&self) -> bool {
        matches!(self.body, Body::Bank { .. } | Body::BankChunk { .. })
    }
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.b.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(s)
    }

    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }

    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }

    fn name(&mut self) -> Option<String> {
        let raw = self.take(28)?;
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        Some(
            raw[..end]
                .iter()
                .map(|&b| char::from(b))
                .collect::<String>()
                .trim()
                .to_string(),
        )
    }
}

/// The program at `r`'s place ("CcnK"…"FxCk"/"FPCh"): its header, then its
/// body.
fn program(r: &mut Reader<'_>) -> Option<(i32, i32, u32, Body)> {
    if r.take(4)? != b"CcnK" {
        return None;
    }
    let _size = r.i32()?;
    let kind = r.take(4)?.to_vec();
    let _format = r.i32()?;
    let id = r.i32()?;
    let version = r.i32()?;
    let count = usize::try_from(r.i32()?).ok()?;
    match kind.as_slice() {
        b"FxCk" => {
            let name = r.name()?;
            let values = (0..count.min(1 << 16))
                .map(|_| r.f32())
                .collect::<Option<Vec<f32>>>()?;
            Some((id, version, count as u32, Body::Program { name, values }))
        }
        b"FPCh" => {
            let name = r.name()?;
            let len = usize::try_from(r.i32()?).ok()?;
            let chunk = r.take(len)?.to_vec();
            Some((
                id,
                version,
                count as u32,
                Body::ProgramChunk { name, chunk },
            ))
        }
        _ => None,
    }
}

/// The preset in `bytes`; `None` when they are no preset file.
pub fn read(bytes: &[u8]) -> Option<Preset> {
    let mut r = Reader { b: bytes, at: 0 };
    if bytes.get(8..12) == Some(b"FxCk".as_slice()) || bytes.get(8..12) == Some(b"FPCh".as_slice())
    {
        let (unique_id, version, params, body) = program(&mut r)?;
        return Some(Preset {
            unique_id,
            version,
            params,
            body,
        });
    }
    if r.take(4)? != b"CcnK" {
        return None;
    }
    let _size = r.i32()?;
    let kind = r.take(4)?.to_vec();
    let format = r.i32()?;
    let unique_id = r.i32()?;
    let version = r.i32()?;
    let count = usize::try_from(r.i32()?).ok()?;
    let current = if format >= 2 {
        let c = r.i32()?;
        r.take(124)?;
        c
    } else {
        r.take(128)?;
        0
    };
    match kind.as_slice() {
        b"FxBk" => {
            let mut programs = Vec::new();
            let mut params = 0;
            for _ in 0..count.min(4096) {
                let (_, _, n, body) = program(&mut r)?;
                params = n;
                let Body::Program { values, .. } = body else {
                    return None;
                };
                programs.push(values);
            }
            Some(Preset {
                unique_id,
                version,
                params,
                body: Body::Bank { programs, current },
            })
        }
        b"FBCh" => {
            let len = usize::try_from(r.i32()?).ok()?;
            let chunk = r.take(len)?.to_vec();
            Some(Preset {
                unique_id,
                version,
                params: 0,
                body: Body::BankChunk { chunk, current },
            })
        }
        _ => None,
    }
}

/// `name` as a program's 28 bytes.
fn name_bytes(name: &str) -> [u8; 28] {
    let mut out = [0u8; 28];
    for (d, c) in out.iter_mut().take(27).zip(name.chars()) {
        *d = u8::try_from(u32::from(c)).unwrap_or(b'?');
    }
    out
}

/// A program's bytes: kind, its header and `body`.
fn write_program(id: i32, version: i32, count: u32, kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut rest = Vec::with_capacity(20 + body.len());
    rest.extend_from_slice(kind);
    rest.extend_from_slice(&1i32.to_be_bytes());
    rest.extend_from_slice(&id.to_be_bytes());
    rest.extend_from_slice(&version.to_be_bytes());
    rest.extend_from_slice(&(count as i32).to_be_bytes());
    rest.extend_from_slice(body);
    wrap(rest)
}

/// `rest` as a file: "CcnK" and its size first.
fn wrap(rest: Vec<u8>) -> Vec<u8> {
    let mut out = b"CcnK".to_vec();
    out.extend_from_slice(&(rest.len() as i32).to_be_bytes());
    out.extend_from_slice(&rest);
    out
}

/// A bank's header (after "CcnK" and the size): its kind, format 2,
/// whose, how many and the program chosen.
fn bank_header(kind: &[u8; 4], p: &Preset, count: usize, current: i32) -> Vec<u8> {
    let mut rest = kind.to_vec();
    rest.extend_from_slice(&2i32.to_be_bytes());
    rest.extend_from_slice(&p.unique_id.to_be_bytes());
    rest.extend_from_slice(&p.version.to_be_bytes());
    rest.extend_from_slice(&(count as i32).to_be_bytes());
    rest.extend_from_slice(&current.to_be_bytes());
    rest.extend_from_slice(&[0u8; 124]);
    rest
}

/// The preset file of `p`.
pub fn write(p: &Preset) -> Vec<u8> {
    match &p.body {
        Body::Program { name, values } => {
            let mut body = name_bytes(name).to_vec();
            for v in values {
                body.extend_from_slice(&v.to_be_bytes());
            }
            write_program(p.unique_id, p.version, values.len() as u32, b"FxCk", &body)
        }
        Body::ProgramChunk { name, chunk } => {
            let mut body = name_bytes(name).to_vec();
            body.extend_from_slice(&(chunk.len() as i32).to_be_bytes());
            body.extend_from_slice(chunk);
            write_program(p.unique_id, p.version, p.params, b"FPCh", &body)
        }
        Body::Bank { programs, current } => {
            let mut rest = bank_header(b"FxBk", p, programs.len(), *current);
            for values in programs {
                let mut body = name_bytes("").to_vec();
                for v in values {
                    body.extend_from_slice(&v.to_be_bytes());
                }
                rest.extend(write_program(
                    p.unique_id,
                    p.version,
                    values.len() as u32,
                    b"FxCk",
                    &body,
                ));
            }
            wrap(rest)
        }
        Body::BankChunk { chunk, current } => {
            let mut rest = bank_header(b"FBCh", p, 1, *current);
            rest.extend_from_slice(&(chunk.len() as i32).to_be_bytes());
            rest.extend_from_slice(chunk);
            wrap(rest)
        }
    }
}

/// `name` made a folder's or a file's name.
pub fn file_name(name: &str) -> String {
    let clean: String = name
        .trim()
        .chars()
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim_end_matches(['.', ' ']).to_string();
    if clean.is_empty() {
        "_".to_string()
    } else {
        clean
    }
}

/// The folder of `vendor`'s `plugin`'s presets under `base`
/// (`Documents/VST2 Presets` on Windows).
pub fn folder(base: &Path, vendor: &str, plugin: &str) -> PathBuf {
    let vendor = if vendor.trim().is_empty() {
        "Unknown"
    } else {
        vendor
    };
    base.join(file_name(vendor)).join(file_name(plugin))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs_and_banks_go_and_come_back() {
        let cases = [
            Body::Program {
                name: "Warm".into(),
                values: vec![0.25, 0.5, 1.0],
            },
            Body::ProgramChunk {
                name: "Chunky".into(),
                chunk: vec![1, 2, 3, 4, 5],
            },
            Body::Bank {
                programs: vec![vec![0.1, 0.2], vec![0.3, 0.4]],
                current: 1,
            },
            Body::BankChunk {
                chunk: b"all of it".to_vec(),
                current: 0,
            },
        ];
        for body in cases {
            let params = match &body {
                Body::Program { values, .. } => values.len() as u32,
                Body::Bank { programs, .. } => programs[0].len() as u32,
                _ => 0,
            };
            let p = Preset {
                unique_id: i32::from_be_bytes(*b"BvG1"),
                version: 3,
                params,
                body,
            };
            let bytes = write(&p);
            assert_eq!(&bytes[..4], b"CcnK");
            // The size counts what follows it.
            let size = i32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize;
            assert_eq!(size, bytes.len() - 8);
            let back = read(&bytes).expect("it reads");
            assert_eq!(back, p);
            assert!(back.is_for(i32::from_be_bytes(*b"BvG1")));
        }
        assert_eq!(read(b"CcnK"), None);
        assert_eq!(read(b"something else entirely"), None);
    }
}
