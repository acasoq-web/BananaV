//! The look-up table files a FileTransform reads: `.cube` (Adobe's and
//! Resolve's, through `bv-lut`), Sony Pictures Imageworks' `.spi1d`,
//! `.spi3d` and `.spimtx`.

use std::sync::Arc;

use crate::config::{Error, Result};
use crate::ops::Op;

/// A file's steps; `interpolation`: the FileTransform's (`linear`,
/// `tetrahedral`, `best`…).
pub fn load(path: &std::path::Path, interpolation: &str) -> Result<Vec<Op>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let text = std::fs::read_to_string(path)
        .map_err(|e| Error::File(format!("{}: {e}", path.display())))?;
    let how = if interpolation == "linear" || interpolation == "trilinear" {
        bv_lut::Interpolation::Linear
    } else {
        bv_lut::Interpolation::Tetrahedral
    };
    let bad = |why: String| Error::File(format!("{}: {why}", path.display()));
    match ext.as_str() {
        "cube" => {
            let lut = bv_lut::Lut::parse(&text).map_err(|e| bad(e.to_string()))?;
            let mut ops = Vec::new();
            if let Some(s) = lut.shaper {
                ops.push(Op::Lut1(Arc::new(s), false));
            }
            if let Some(c) = lut.cube {
                ops.push(Op::Lut3(Arc::new(c), how));
            }
            Ok(ops)
        }
        "spi1d" => spi1d(&text)
            .map(|t| vec![Op::Lut1(Arc::new(t), false)])
            .map_err(bad),
        "spi3d" => spi3d(&text)
            .map(|t| vec![Op::Lut3(Arc::new(t), how)])
            .map_err(bad),
        "spimtx" => spimtx(&text).map(|op| vec![op]).map_err(bad),
        other => Err(Error::Unsupported(format!("LUT files .{other}"))),
    }
}

fn numbers(s: &str) -> std::result::Result<Vec<f64>, String> {
    s.split_whitespace()
        .map(|w| w.parse::<f64>().map_err(|_| format!("not a number: {w}")))
        .collect()
}

/// `.spi1d`: a curve (1 or 3 components) over From–To.
pub fn spi1d(text: &str) -> std::result::Result<bv_lut::Table1, String> {
    let (mut from, mut to, mut components) = (0.0f32, 1.0f32, 1usize);
    let mut values: Vec<f64> = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        if inside {
            if l.starts_with('}') {
                inside = false;
                continue;
            }
            values.extend(numbers(l)?);
            continue;
        }
        let mut words = l.split_whitespace();
        match words.next() {
            Some("From") => {
                let v = numbers(&l[4..])?;
                if v.len() != 2 {
                    return Err("From takes two numbers".into());
                }
                (from, to) = (v[0] as f32, v[1] as f32);
            }
            Some("Components") => {
                components = words
                    .next()
                    .and_then(|w| w.parse().ok())
                    .ok_or("Components")?;
            }
            Some("{") => inside = true,
            _ => {}
        }
    }
    if components != 1 && components != 3 {
        return Err(format!("{components} components"));
    }
    let entries: Vec<[f32; 3]> = values
        .chunks(components)
        .map(|c| {
            if components == 1 {
                [c[0] as f32; 3]
            } else {
                [c[0] as f32, c[1] as f32, c[2] as f32]
            }
        })
        .collect();
    if entries.len() < 2 {
        return Err("too few entries".into());
    }
    Ok(bv_lut::Table1 {
        min: [from; 3],
        max: [to; 3],
        entries,
    })
}

/// `.spi3d`: a lattice, each entry with its indices.
pub fn spi3d(text: &str) -> std::result::Result<bv_lut::Table3, String> {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let header = lines.next().ok_or("empty")?;
    if !header.starts_with("SPILUT") {
        return Err("not an SPILUT".into());
    }
    lines.next();
    let sizes = numbers(lines.next().ok_or("no sizes")?)?;
    if sizes.len() != 3 || sizes[0] != sizes[1] || sizes[1] != sizes[2] {
        return Err("a lattice as long every way".into());
    }
    let n = sizes[0] as usize;
    let mut entries = vec![[0f32; 3]; n * n * n];
    for l in lines {
        let v = numbers(l)?;
        if v.len() != 6 {
            return Err(format!("an entry has 6 numbers: {l}"));
        }
        let (i, j, k) = (v[0] as usize, v[1] as usize, v[2] as usize);
        if i >= n || j >= n || k >= n {
            return Err(format!("an entry out of the lattice: {l}"));
        }
        // bv-lut: red changes fastest.
        entries[i + j * n + k * n * n] = [v[3] as f32, v[4] as f32, v[5] as f32];
    }
    Ok(bv_lut::Table3 {
        min: [0.0; 3],
        max: [1.0; 3],
        size: n,
        entries,
    })
}

/// `.spimtx`: three rows of a matrix and its offsets (in 16-bit steps).
pub fn spimtx(text: &str) -> std::result::Result<Op, String> {
    let v = numbers(text)?;
    if v.len() != 12 {
        return Err(format!("12 numbers, found {}", v.len()));
    }
    let m = [[v[0], v[1], v[2]], [v[4], v[5], v[6]], [v[8], v[9], v[10]]];
    Ok(Op::Matrix(
        m,
        [v[3] / 65535.0, v[7] / 65535.0, v[11] / 65535.0],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imageworks_formats() {
        let t = spi1d("Version 1\nFrom 0.0 1.0\nLength 3\nComponents 1\n{\n 0.0\n 0.25\n 1.0\n}\n")
            .unwrap();
        assert_eq!(t.entries.len(), 3);
        assert_eq!(t.apply([0.5; 3]), [0.25; 3]);
        let mut text = String::from("SPILUT 1.0\n3 3\n2 2 2\n");
        for k in 0..2 {
            for j in 0..2 {
                for i in 0..2 {
                    text += &format!("{i} {j} {k} {i} {j} {k}\n");
                }
            }
        }
        let t = spi3d(&text).unwrap();
        let c = t.apply([0.25, 0.5, 0.75], bv_lut::Interpolation::Linear);
        assert!(
            c.iter()
                .zip([0.25, 0.5, 0.75])
                .all(|(a, b)| (a - b).abs() < 1e-6),
            "{c:?}"
        );
        let Op::Matrix(m, o) = spimtx("2 0 0 0\n0 1 0 65535\n0 0 1 0\n").unwrap() else {
            panic!()
        };
        assert_eq!((m[0][0], o[1]), (2.0, 1.0));
    }
}
