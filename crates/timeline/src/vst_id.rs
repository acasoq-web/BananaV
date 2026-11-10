//! How another editor's project files name a VST 3 plug-in, so that
//! BananaV finds the plug-in installed here when it reads one: a number
//! made from the plug-in's class, and checks of the plug-in's name and of
//! its file's name. They are measured: the four fields agree with the
//! files' for the plug-ins tried.

use serde::{Deserialize, Serialize};

/// A VST 3 effect read from another editor's project whose plug-in BananaV
/// has not found yet: what that editor knows it by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VstWanted {
    /// The number made from its class ([`vst3_number`]).
    pub number: u32,
    /// The checks of its name and of its file's name ([`name_check`]).
    pub name: u16,
    pub file: u16,
}

impl VstWanted {
    /// How well a plug-in of class `class_id` named `name`, in file
    /// `file_name`, is the one wanted: `None` when not at all (another
    /// number), else 0–2, the checks that agree too.
    pub fn score(&self, class_id: &str, name: &str, file_name: &str) -> Option<u8> {
        self.score_number(vst3_number(class_id)?, name, file_name)
    }

    /// The same for a plug-in known by `number` itself (a VST 2 plug-in's
    /// own number).
    pub fn score_number(&self, number: u32, name: &str, file_name: &str) -> Option<u8> {
        (number == self.number).then(|| {
            u8::from(name_check(name) == self.name) + u8::from(name_check(file_name) == self.file)
        })
    }
}

/// The number another editor knows a VST 3 effect by: its class's four
/// 32-bit words (its 32 hexadecimal digits, eight by eight) folded as
/// `n · 31 + word`, wrapping. `None` when `class_id` is no class.
pub fn vst3_number(class_id: &str) -> Option<u32> {
    let id = class_id.trim();
    if id.len() != 32 || !id.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut n = 0u32;
    for k in 0..4 {
        let word = u32::from_str_radix(&id[k * 8..k * 8 + 8], 16).ok()?;
        n = n.wrapping_mul(31).wrapping_add(word);
    }
    Some(n)
}

/// `c` in the Windows Latin 1 code page; `?` for what it has not.
fn latin1(c: char) -> u8 {
    const HIGH: [(char, u8); 27] = [
        ('€', 0x80),
        ('‚', 0x82),
        ('ƒ', 0x83),
        ('„', 0x84),
        ('…', 0x85),
        ('†', 0x86),
        ('‡', 0x87),
        ('ˆ', 0x88),
        ('‰', 0x89),
        ('Š', 0x8A),
        ('‹', 0x8B),
        ('Œ', 0x8C),
        ('Ž', 0x8E),
        ('‘', 0x91),
        ('’', 0x92),
        ('“', 0x93),
        ('”', 0x94),
        ('•', 0x95),
        ('–', 0x96),
        ('—', 0x97),
        ('˜', 0x98),
        ('™', 0x99),
        ('š', 0x9A),
        ('›', 0x9B),
        ('œ', 0x9C),
        ('ž', 0x9E),
        ('Ÿ', 0x9F),
    ];
    match u32::from(c) {
        n @ (0..=0x7F | 0xA0..=0xFF) => n as u8,
        _ => HIGH.iter().find(|(h, _)| *h == c).map_or(b'?', |&(_, b)| b),
    }
}

/// The check of a name: CRC-16 (polynomial 0x1021, from 0xFFFF, the
/// message followed by two zero bytes) of its Windows Latin 1 bytes, the
/// letters a–z upper-cased.
pub fn name_check(text: &str) -> u16 {
    let mut crc = 0xFFFFu16;
    let bytes = text
        .chars()
        .map(|c| latin1(c).to_ascii_uppercase())
        .chain([0, 0]);
    for b in bytes {
        for k in (0..8).rev() {
            let top = crc & 0x8000 != 0;
            crc = (crc << 1) | u16::from((b >> k) & 1);
            if top {
                crc ^= 0x1021;
            }
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The equalizer measured: its class, name and file give the four
    /// fields of the class its file names it by
    /// (`F6A7FB77-7DD2-857C-…`).
    #[test]
    fn the_measured_plug_in_is_found() {
        assert_eq!(name_check("123456789"), 0xE5CC);
        let class = "ABCDEF019182FAEB4D4147584D584645";
        assert_eq!(vst3_number(class), Some(0xF6A7_FB77));
        assert_eq!(name_check("Modern Equalizer"), 0x857C);
        assert_eq!(name_check("APU_modernEQ_x64.vst3"), 0x7DD2);
        let wanted = VstWanted {
            number: 0xF6A7_FB77,
            name: 0x857C,
            file: 0x7DD2,
        };
        assert_eq!(
            wanted.score(class, "Modern Equalizer", "APU_modernEQ_x64.vst3"),
            Some(2)
        );
        assert_eq!(wanted.score(class, "Renamed", "Other.vst3"), Some(0));
        assert_eq!(
            wanted.score("42414E414E4156544553544741494E31", "Modern Equalizer", ""),
            None
        );
        assert_eq!(vst3_number("not a class"), None);
    }
}
