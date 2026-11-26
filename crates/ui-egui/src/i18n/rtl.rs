//! Right-to-left text — Arabic — as egui shows it. egui shapes each run of
//! a line that one font draws (an Arabic word's letters joined by the
//! font's own rules, the word read from the right), but it lays a line's
//! runs out left to right in the order they come. So a translation is made
//! ready once, when its catalog is read: each line put in visual order as
//! a right-to-left paragraph (Unicode's bidirectional algorithm, UAX #9,
//! simplified):
//!
//! - the runs of Arabic placed from the right, each kept in reading order
//!   for egui to turn around itself;
//! - Latin words, numbers and the templates' placeholders (`{}`, `{0}`) in
//!   their own order;
//! - the neutral characters between two runs going their way when both go
//!   one way and the paragraph's otherwise, brackets mirrored in
//!   right-to-left runs.
//!
//! The reordering is its own inverse: [`logical`] gives the texts the
//! operating system lays out itself (a window's title, a file dialog's)
//! their reading order back.
//!
//! A text a widget wraps itself is not reordered across the lines it is
//! wrapped into: the essential texts are short.

/// Whether `c` is written right to left (Arabic's letters, its marks and
/// punctuation; Hebrew's): the characters egui's runs of right-to-left
/// text are made of.
fn is_rtl(c: char) -> bool {
    matches!(c as u32,
        0x0590..=0x05FF | 0x0600..=0x065F | 0x066A..=0x06EF | 0x06FA..=0x08FF
        | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF)
}

/// Whether `text` has right-to-left letters.
pub(super) fn has_rtl(text: &str) -> bool {
    text.chars().any(is_rtl)
}

/// Which way a piece of a line goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Way {
    Rtl,
    Ltr,
    /// Spaces and punctuation: their neighbours' way.
    Neutral,
}

/// A piece a line is reordered by.
struct Piece {
    chars: Vec<char>,
    way: Way,
    /// A neutral character (its way taken from its neighbours).
    neutral: bool,
}

/// The pieces of a line: a run of right-to-left characters (an Arabic
/// word), a placeholder (`{}`, `{0}`, `{:.1}`, `{{`, `}}`), any other
/// character.
fn pieces(line: &[char]) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::new();
    let mut i = 0;
    while i < line.len() {
        let c = line[i];
        // A placeholder of a template: one piece, left to right.
        if c == '{' || c == '}' {
            let pair = line.get(i + 1) == Some(&c);
            let len = if pair {
                Some(2)
            } else if c == '{' {
                line[i + 1..]
                    .iter()
                    .position(|&d| d == '}')
                    .filter(|&len| {
                        line[i + 1..i + 1 + len]
                            .iter()
                            .all(|d| d.is_ascii_alphanumeric() || *d == ':' || *d == '.')
                    })
                    .map(|len| len + 2)
            } else {
                None
            };
            if let Some(len) = len {
                out.push(Piece {
                    chars: line[i..i + len].to_vec(),
                    way: Way::Ltr,
                    neutral: false,
                });
                i += len;
                continue;
            }
        }
        if is_rtl(c) {
            let start = i;
            while i < line.len() && is_rtl(line[i]) {
                i += 1;
            }
            out.push(Piece {
                chars: line[start..i].to_vec(),
                way: Way::Rtl,
                neutral: false,
            });
            continue;
        }
        let neutral = !c.is_alphanumeric();
        out.push(Piece {
            chars: vec![c],
            way: if neutral { Way::Neutral } else { Way::Ltr },
            neutral,
        });
        i += 1;
    }
    out
}

/// A bracket's mirror, for a right-to-left run.
fn mirrored(c: char) -> char {
    match c {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        '«' => '»',
        '»' => '«',
        '‹' => '›',
        '›' => '‹',
        _ => c,
    }
}

/// A line (no line breaks) in visual order, as a right-to-left paragraph.
fn reorder(line: &[char]) -> Vec<char> {
    let mut ps = pieces(line);
    // Neutrals: their neighbours' way when both go one way, else the
    // paragraph's (right to left).
    let mut i = 0;
    while i < ps.len() {
        if ps[i].way != Way::Neutral {
            i += 1;
            continue;
        }
        let start = i;
        while i < ps.len() && ps[i].way == Way::Neutral {
            i += 1;
        }
        let before = if start == 0 {
            Way::Rtl
        } else {
            ps[start - 1].way
        };
        let after = if i == ps.len() { Way::Rtl } else { ps[i].way };
        let way = if before == after { before } else { Way::Rtl };
        for p in &mut ps[start..i] {
            p.way = way;
        }
    }
    for p in &mut ps {
        if p.neutral && p.way == Way::Rtl {
            p.chars[0] = mirrored(p.chars[0]);
        }
    }
    // The whole line turned around, its left-to-right runs back in their
    // own order (egui turns each Arabic run around itself).
    ps.reverse();
    let mut i = 0;
    while i < ps.len() {
        if ps[i].way != Way::Ltr {
            i += 1;
            continue;
        }
        let start = i;
        while i < ps.len() && ps[i].way == Way::Ltr {
            i += 1;
        }
        ps[start..i].reverse();
    }
    ps.into_iter().flat_map(|p| p.chars).collect()
}

fn each_line(text: &str) -> String {
    if !has_rtl(text) {
        return text.to_string();
    }
    text.split('\n')
        .map(|line| {
            let chars: Vec<char> = line.chars().collect();
            reorder(&chars).into_iter().collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `text` ready for egui: each line in visual order. Text with no
/// right-to-left letters comes back as it is.
pub(super) fn visual(text: &str) -> String {
    each_line(text)
}

/// A text [`visual`] made ready, back in reading order (the reordering
/// undoes itself).
pub(super) fn logical(text: &str) -> String {
    each_line(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arabic_words_keep_their_letters_in_reading_order() {
        // egui joins and turns each word around itself: only the words'
        // order changes.
        assert_eq!(visual("ملف"), "ملف");
        assert_eq!(visual("شاشة الترحيب"), "الترحيب شاشة");
        // Marks and Arabic punctuation stay with their word.
        assert_eq!(visual("بَب؟ نعم"), "نعم بَب؟");
    }

    #[test]
    fn a_line_reads_from_the_right_latin_and_numbers_kept() {
        // "Open file.mp4" — Arabic, then a Latin name: the name keeps its
        // order and sits on the left.
        let v = visual("فتح file.mp4");
        assert_eq!(v, "file.mp4 فتح");
        // A number keeps its digits' order.
        let v = visual("عدد 120 ملف");
        assert_eq!(v, "ملف 120 عدد");
        // Placeholders stay whole, and so do their braces.
        let v = visual("تم الحفظ في {}");
        assert_eq!(v, "{} في الحفظ تم");
        let v = visual("{0} من {1}");
        assert_eq!(v, "{1} من {0}");
        // An ellipsis ends the line on the left.
        assert_eq!(visual("جديد…"), "…جديد");
        // Brackets between Arabic words are mirrored.
        assert_eq!(visual("ملف (جديد)"), "(جديد) ملف");
        // Each line on its own; a text with no Arabic as it is.
        assert_eq!(visual("A\nB"), "A\nB");
        assert_eq!(visual("Ctrl+K"), "Ctrl+K");
        assert_eq!(visual("سطر\nثاني"), "سطر\nثاني");
    }

    #[test]
    fn keys_and_menu_paths_keep_their_order() {
        // A shortcut inside Arabic text stays as typed.
        let v = visual("اضغط Ctrl+Shift+M للتشغيل");
        assert_eq!(v, "للتشغيل Ctrl+Shift+M اضغط");
        // A menu path: its arrows point the paragraph's way.
        assert_eq!(visual("خيارات › تفضيلات"), "تفضيلات ‹ خيارات");
    }

    #[test]
    fn the_reading_order_comes_back() {
        for text in [
            "شاشة الترحيب",
            "فتح file.mp4",
            "{0} من {1}",
            "ملف (جديد)",
            "اضغط Ctrl+Shift+M للتشغيل",
            "خيارات › تفضيلات…",
            "تكبير (↑)",
        ] {
            assert_eq!(logical(&visual(text)), text);
        }
        // A template filled after it was made ready (a dialog's title with
        // a clip's name).
        let title = visual("خصائص {}").replace("{}", "Clip 1");
        assert_eq!(logical(&title), "خصائص Clip 1");
    }
}
