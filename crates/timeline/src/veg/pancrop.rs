//! An event's Pan/Crop as a `.veg` keeps it: a chunk holding a RIFF of 32 bits
//! (four letters, a size of 4 bytes, its data, each chunk starting on a
//! multiple of 4): its settings, then the box's keyframes and the masks'.

use super::read::{f64_at, i32_at, i64_at, u32_at};
use super::{ticks, units};
use crate::fx::KeyCurve;
use crate::mask::{MaskAnchor, MaskFeather, MaskKey, MaskLook, MaskMode, MaskPath, PanCropMask};
use crate::pancrop::{PanCrop, PanCropKey};

/// A chunk of the inner RIFF.
struct Inner<'a> {
    name: [u8; 4],
    /// A list's type, for lists.
    kind: Option<[u8; 4]>,
    data: &'a [u8],
}

/// The chunks in `b`, one after the other.
fn chunks(b: &[u8]) -> Vec<Inner<'_>> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at + 8 <= b.len() {
        let name: [u8; 4] = b[at..at + 4].try_into().expect("4 bytes");
        if name == [0; 4] {
            break;
        }
        let size = u32_at(b, at + 4) as usize;
        let Some(end) = at.checked_add(8).and_then(|v| v.checked_add(size)) else {
            break;
        };
        let Some(data) = b.get(at + 8..end) else {
            break;
        };
        if &name == b"LIST" && data.len() >= 4 {
            out.push(Inner {
                name,
                kind: Some(data[..4].try_into().expect("4 bytes")),
                data: &data[4..],
            });
        } else {
            out.push(Inner {
                name,
                kind: None,
                data,
            });
        }
        at = end.next_multiple_of(4);
    }
    out
}

fn list<'a>(all: &'a [Inner<'a>], kind: &[u8; 4]) -> Option<&'a [u8]> {
    all.iter()
        .find(|c| c.kind.as_ref() == Some(kind))
        .map(|c| c.data)
}

fn block<'a>(all: &'a [Inner<'a>], name: &[u8; 4]) -> Option<&'a [u8]> {
    all.iter()
        .find(|c| c.kind.is_none() && &c.name == name)
        .map(|c| c.data)
}

/// The keyframes' curves, as the file numbers them.
pub(super) fn curve(n: u32) -> KeyCurve {
    match n {
        1 => KeyCurve::Hold,
        2 => KeyCurve::Slow,
        3 => KeyCurve::Fast,
        4 => KeyCurve::Smooth,
        5 => KeyCurve::Sharp,
        _ => KeyCurve::Linear,
    }
}

pub(super) fn curve_number(c: KeyCurve) -> u32 {
    match c {
        KeyCurve::Hold => 1,
        KeyCurve::Slow => 2,
        KeyCurve::Fast => 3,
        KeyCurve::Smooth => 4,
        KeyCurve::Sharp => 5,
        KeyCurve::Linear | KeyCurve::Bezier | KeyCurve::BezierSplit => 0,
    }
}

/// An anchor's tangents' kind, as the file numbers it: no tangents, split, in
/// line, in line and as long.
const NO_TANGENTS: i32 = -1;
const SPLIT: i32 = 0;
const IN_LINE: i32 = 1;
const EVEN: i32 = 2;

fn anchor(b: &[u8]) -> Option<(i64, MaskAnchor)> {
    if b.len() < 0x40 {
        return None;
    }
    let at = [f64_at(b, 0x08), f64_at(b, 0x10)];
    let before = [f64_at(b, 0x18) - at[0], f64_at(b, 0x20) - at[1]];
    let after = [f64_at(b, 0x28) - at[0], f64_at(b, 0x30) - at[1]];
    let kind = i32_at(b, 0x38);
    Some((
        i64_at(b, 0),
        MaskAnchor {
            at,
            before,
            after,
            split: kind == SPLIT,
            even: kind == EVEN,
        },
    ))
}

/// A path of a keyframe of the file.
struct FilePath {
    id: i64,
    closed: bool,
    look: MaskLook,
    anchors: Vec<(i64, MaskAnchor)>,
}

fn path(b: &[u8]) -> Option<FilePath> {
    let all = chunks(b);
    let h = block(&all, b"PTHB")?;
    if h.len() < 0x2C {
        return None;
    }
    let flags = u32_at(h, 0x18);
    let feather_amount = f64_at(h, 0x10);
    // Files of an older kind leave these below zero.
    let mode = match i32_at(h, 0x28) {
        m if m < 0 => (flags >> 2 & 1) as i32 + 1,
        m => m,
    };
    let opacity = match f64_at(h, 0x20) {
        o if o < 0.0 => 1.0,
        o => o.clamp(0.0, 1.0),
    };
    let feather = match i32_at(h, 0x1C) {
        k if k < 0 => {
            if feather_amount >= 0.0 {
                2
            } else {
                1
            }
        }
        k => k,
    };
    let anchors = list(&all, b"ANCL")
        .map(|l| {
            chunks(l)
                .iter()
                .filter(|c| &c.name == b"ANCP")
                .filter_map(|c| anchor(c.data))
                .collect()
        })
        .unwrap_or_default();
    Some(FilePath {
        id: i64_at(h, 0),
        closed: flags & 1 != 0,
        look: MaskLook {
            mode: match mode {
                0 => MaskMode::Disabled,
                2 => MaskMode::Negative,
                _ => MaskMode::Positive,
            },
            anti_alias: flags & 2 != 0,
            opacity,
            feather: match feather {
                1 => MaskFeather::In,
                2 => MaskFeather::Out,
                3 => MaskFeather::Both,
                _ => MaskFeather::None,
            },
            feather_pct: feather_amount.abs().min(0.5) * 100.0,
        },
        anchors,
    })
}

/// The masks of the file's keyframes: its paths by their ids, in the order
/// they first come, their anchors in each keyframe by theirs.
fn masks(all: &[Inner<'_>], on: bool, apply_to_fx: bool) -> Option<PanCropMask> {
    struct FileKey {
        at: i64,
        curve: u32,
        paths: Vec<FilePath>,
    }
    let file: Vec<FileKey> = chunks(list(all, b"MSKL")?)
        .iter()
        .filter(|c| c.kind.as_ref() == Some(b"MSKK"))
        .filter_map(|c| {
            let inner = chunks(c.data);
            let h = block(&inner, b"MSKB")?;
            let paths = list(&inner, b"PTHL").map_or_else(Vec::new, |l| {
                chunks(l)
                    .iter()
                    .filter(|p| p.kind.as_ref() == Some(b"PATH"))
                    .filter_map(|p| path(p.data))
                    .collect()
            });
            Some(FileKey {
                at: i64_at(h, 0),
                curve: u32_at(h, 0x10),
                paths,
            })
        })
        .collect();
    let mut ids: Vec<i64> = Vec::new();
    for p in file.iter().flat_map(|k| &k.paths) {
        if !ids.contains(&p.id) {
            ids.push(p.id);
        }
    }
    if ids.is_empty() && !on {
        return None;
    }
    // Each path's own settings, its anchors' order: as it first comes.
    let first = |id: i64| file.iter().flat_map(|k| &k.paths).find(|p| p.id == id);
    let paths: Vec<MaskPath> = ids
        .iter()
        .filter_map(|&id| {
            let p = first(id)?;
            Some(MaskPath {
                anchors: p.anchors.iter().map(|a| a.1).collect(),
                mode: p.look.mode,
                anti_alias: p.look.anti_alias,
                opacity: p.look.opacity,
                feather: p.look.feather,
                feather_pct: p.look.feather_pct,
                closed: p.closed,
            })
        })
        .collect();
    let order = |id: i64, own: &FilePath| -> Vec<MaskAnchor> {
        // By the first keyframe's anchors, when this one has them all.
        let Some(lead) = first(id) else {
            return Vec::new();
        };
        let same = lead.anchors.len() == own.anchors.len()
            && lead
                .anchors
                .iter()
                .all(|a| own.anchors.iter().any(|b| b.0 == a.0));
        if same {
            lead.anchors
                .iter()
                .filter_map(|a| own.anchors.iter().find(|b| b.0 == a.0))
                .map(|a| a.1)
                .collect()
        } else {
            own.anchors.iter().map(|a| a.1).collect()
        }
    };
    let keys: Vec<MaskKey> = file
        .iter()
        .map(|k| {
            let found = |id: i64| k.paths.iter().find(|p| p.id == id);
            MaskKey {
                at: ticks(k.at),
                curve: curve(k.curve),
                shapes: ids
                    .iter()
                    .map(|&id| found(id).map_or_else(Vec::new, |p| order(id, p)))
                    .collect(),
                looks: ids
                    .iter()
                    .zip(&paths)
                    .map(|(&id, own)| found(id).map_or_else(|| look_of(own), |p| p.look))
                    .collect(),
            }
        })
        .collect();
    Some(PanCropMask {
        enabled: on,
        apply_to_fx,
        paths,
        keys,
    })
}

fn look_of(p: &MaskPath) -> MaskLook {
    MaskLook {
        mode: p.mode,
        anti_alias: p.anti_alias,
        opacity: p.opacity,
        feather: p.feather,
        feather_pct: p.feather_pct,
    }
}

/// A chunk of the inner RIFF, read: the box, and the masks when it has any.
pub(super) fn read(b: &[u8]) -> Option<(PanCrop, Option<PanCropMask>)> {
    let top = chunks(b);
    let root = top.iter().find(|c| c.kind.as_ref() == Some(b"EPCP"))?;
    let all = chunks(root.data);
    let s = block(&all, b"EPCS")?;
    let flags = u32_at(s, 0);
    let size = [u32_at(s, 0x2C), u32_at(s, 0x30)];
    let keys: Vec<PanCropKey> = list(&all, b"POSL")
        .map(|l| {
            chunks(l)
                .iter()
                .filter(|c| &c.name == b"POSK" && c.data.len() >= 0x54)
                .map(|c| {
                    let k = c.data;
                    PanCropKey {
                        at: ticks(i64_at(k, 0)),
                        curve: curve(u32_at(k, 0x10)),
                        width: f64_at(k, 0x14),
                        height: f64_at(k, 0x1C),
                        center: [f64_at(k, 0x24), f64_at(k, 0x2C)],
                        angle: f64_at(k, 0x34),
                        pivot: [f64_at(k, 0x3C), f64_at(k, 0x44)],
                        smoothness: f64_at(k, 0x4C),
                        crop: [0.0; 4],
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let mask_on = block(&all, b"KFRS").is_some_and(|k| u32_at(k, 0) != 0);
    Some((
        PanCrop {
            size,
            keep_aspect: flags & 1 != 0,
            fill: flags & 2 != 0,
            keys,
        },
        masks(&all, mask_on, flags & 4 != 0),
    ))
}

/// The same chunks written.
struct Out(Vec<u8>);

impl Out {
    fn block(&mut self, name: &[u8; 4], data: &[u8]) {
        self.0.extend_from_slice(name);
        self.0.extend_from_slice(&(data.len() as u32).to_le_bytes());
        self.0.extend_from_slice(data);
        self.0.resize(self.0.len().next_multiple_of(4), 0);
    }

    fn list(&mut self, kind: &[u8; 4], body: impl FnOnce(&mut Out)) {
        self.0.extend_from_slice(b"LIST");
        let at = self.0.len();
        self.0.extend_from_slice(&[0; 4]);
        self.0.extend_from_slice(kind);
        body(self);
        let size = (self.0.len() - at - 4) as u32;
        self.0[at..at + 4].copy_from_slice(&size.to_le_bytes());
        self.0.resize(self.0.len().next_multiple_of(4), 0);
    }
}

fn number(b: &mut [u8], at: usize, v: f64) {
    b[at..at + 8].copy_from_slice(&v.to_le_bytes());
}

fn whole(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

fn long(b: &mut [u8], at: usize, v: i64) {
    b[at..at + 8].copy_from_slice(&v.to_le_bytes());
}

/// A path's anchors at a keyframe, written.
fn path_out(o: &mut Out, id: i64, closed: bool, look: &MaskLook, shape: &[MaskAnchor]) {
    o.list(b"PATH", |o| {
        let mut h = [0u8; 0x2C];
        long(&mut h, 0, id);
        long(&mut h, 8, shape.len() as i64 + 1);
        number(&mut h, 0x10, look.feather_pct / 100.0);
        whole(
            &mut h,
            0x18,
            u32::from(closed)
                | u32::from(look.anti_alias) << 1
                | u32::from(look.mode == MaskMode::Negative) << 2,
        );
        whole(
            &mut h,
            0x1C,
            match look.feather {
                MaskFeather::None => 0,
                MaskFeather::In => 1,
                MaskFeather::Out => 2,
                MaskFeather::Both => 3,
            },
        );
        number(&mut h, 0x20, look.opacity);
        whole(
            &mut h,
            0x28,
            match look.mode {
                MaskMode::Disabled => 0,
                MaskMode::Positive => 1,
                MaskMode::Negative => 2,
            },
        );
        o.block(b"PTHB", &h);
        o.list(b"ANCL", |o| {
            for (k, a) in shape.iter().enumerate() {
                let mut p = [0u8; 0x40];
                long(&mut p, 0, k as i64 + 1);
                number(&mut p, 0x08, a.at[0]);
                number(&mut p, 0x10, a.at[1]);
                number(&mut p, 0x18, a.at[0] + a.before[0]);
                number(&mut p, 0x20, a.at[1] + a.before[1]);
                number(&mut p, 0x28, a.at[0] + a.after[0]);
                number(&mut p, 0x30, a.at[1] + a.after[1]);
                let bare = a.before == [0.0; 2] && a.after == [0.0; 2];
                let kind = if bare {
                    NO_TANGENTS
                } else if a.split {
                    SPLIT
                } else if a.even {
                    EVEN
                } else {
                    IN_LINE
                };
                whole(&mut p, 0x38, kind as u32);
                o.block(b"ANCP", &p);
            }
        });
    });
}

/// What the Pan/Crop chunk holds for an event's Pan/Crop and masks: `None`
/// when it is as a new event's, which has none.
pub(super) fn write(
    p: &PanCrop,
    mask: Option<&PanCropMask>,
    media: [u32; 2],
    fallback: &[PanCropKey],
) -> Option<Vec<u8>> {
    let masked = mask.is_some_and(|m| !m.paths.is_empty() || m.enabled);
    if p.is_default() && !masked {
        return None;
    }
    let mut o = Out(Vec::new());
    o.list(b"EPCP", |o| {
        let mut s = vec![0u8; 0x23C];
        whole(
            &mut s,
            0,
            u32::from(p.keep_aspect)
                | u32::from(p.fill) << 1
                | u32::from(mask.is_some_and(|m| m.apply_to_fx)) << 2,
        );
        number(&mut s, 0x04, 1.0);
        whole(&mut s, 0x1C, 16);
        long(&mut s, 0x20, mask.map_or(1, |m| m.paths.len() as i64 + 1));
        whole(&mut s, 0x28, 1);
        whole(&mut s, 0x2C, media[0]);
        whole(&mut s, 0x30, media[1]);
        o.block(b"EPCS", &s);
        let mut k = [0u8; 8];
        whole(&mut k, 0, u32::from(mask.is_some_and(|m| m.enabled)));
        o.block(b"KFRS", &k);
        o.list(b"POSL", |o| {
            let keys = if p.keys.is_empty() { fallback } else { &p.keys };
            for key in keys {
                let mut b = [0u8; 0x54];
                long(&mut b, 0, units(key.at));
                whole(&mut b, 0x10, curve_number(key.curve));
                number(&mut b, 0x14, key.width);
                number(&mut b, 0x1C, key.height);
                number(&mut b, 0x24, key.center[0]);
                number(&mut b, 0x2C, key.center[1]);
                number(&mut b, 0x34, key.angle);
                number(&mut b, 0x3C, key.pivot[0]);
                number(&mut b, 0x44, key.pivot[1]);
                number(&mut b, 0x4C, key.smoothness);
                o.block(b"POSK", &b);
            }
        });
        o.list(b"MSKL", |o| {
            let empty;
            let m = match mask {
                Some(m) => m,
                None => {
                    empty = PanCropMask::default();
                    &empty
                }
            };
            let own: Vec<MaskKey>;
            let keys: &[MaskKey] = if m.keys.is_empty() {
                own = vec![MaskKey {
                    at: bv_core::Tick::ZERO,
                    shapes: m.paths.iter().map(|p| p.anchors.clone()).collect(),
                    curve: KeyCurve::Linear,
                    looks: Vec::new(),
                }];
                &own
            } else {
                &m.keys
            };
            for key in keys {
                o.list(b"MSKK", |o| {
                    let mut b = [0u8; 0x14];
                    long(&mut b, 0, units(key.at));
                    whole(&mut b, 0x10, curve_number(key.curve));
                    o.block(b"MSKB", &b);
                    o.list(b"PTHL", |o| {
                        for (i, path) in m.paths.iter().enumerate() {
                            let look = key.looks.get(i).copied().unwrap_or_else(|| look_of(path));
                            let shape = key.shapes.get(i).map_or(&[][..], |s| &s[..]);
                            path_out(o, i as i64 + 1, path.closed, &look, shape);
                        }
                    });
                });
            }
        });
    });
    Some(o.0)
}
