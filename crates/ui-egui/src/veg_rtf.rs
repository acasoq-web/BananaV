//! The rich text a `.veg` keeps Titles & Text's text in (RTF): read into
//! BananaV's plain text with its runs of other fonts, sizes, bold and
//! italic ([`read`]), and written back ([`write`]).

/// A stretch of the text in one style.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Span {
    pub text: String,
    pub font: String,
    /// Half points, as RTF counts them.
    pub half_points: u32,
    pub bold: bool,
    pub italic: bool,
}

/// What a text holds: its spans (a paragraph's end a line feed in them)
/// and its first paragraph's justification (0 left, 1 center, 2 right).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Rich {
    pub spans: Vec<Span>,
    pub justify: i32,
}

/// The state a group of RTF keeps.
#[derive(Clone, Debug)]
struct State {
    font: usize,
    half_points: u32,
    bold: bool,
    italic: bool,
    /// The group is a destination that holds no text (a font table, a
    /// color table, an ignored `\*` one).
    skip: bool,
    /// Characters to leave out after a `\u` (`\uc`).
    uc: usize,
}

/// Windows' code page 1252's characters 0x80–0x9F (the rest is Latin-1).
const WINDOWS_1252: [char; 32] = [
    '\u{20AC}', '\u{81}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{8D}', '\u{017D}', '\u{8F}',
    '\u{90}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{9D}', '\u{017E}', '\u{0178}',
];

fn cp1252(b: u8) -> char {
    match b {
        0x80..=0x9F => WINDOWS_1252[usize::from(b - 0x80)],
        _ => char::from(b),
    }
}

/// `rtf`'s text and styles. Text that is not RTF is a span of its own.
pub(crate) fn read(rtf: &str) -> Rich {
    let src: Vec<char> = rtf.chars().collect();
    if !rtf.trim_start().starts_with("{\\rtf") {
        return Rich {
            spans: vec![Span {
                text: rtf.to_string(),
                font: String::new(),
                half_points: 24,
                bold: false,
                italic: false,
            }],
            justify: 0,
        };
    }
    let mut fonts: Vec<(usize, String)> = Vec::new();
    let mut stack: Vec<State> = Vec::new();
    let mut st = State {
        font: 0,
        half_points: 24,
        bold: false,
        italic: false,
        skip: false,
        uc: 1,
    };
    let mut in_fonts = false;
    let mut font_name = String::new();
    let mut font_number = 0usize;
    let mut rich = Rich::default();
    let mut justify: Option<i32> = None;
    let mut paragraph = 0;
    let mut skip_chars = 0usize;
    // Before the first paragraph's properties, blanks are the writer's
    // layout, not text.
    let mut begun = false;
    let push = |rich: &mut Rich, st: &State, fonts: &[(usize, String)], c: char| {
        let font = fonts
            .iter()
            .find(|(n, _)| *n == st.font)
            .map(|(_, f)| f.clone())
            .unwrap_or_default();
        match rich.spans.last_mut() {
            Some(s)
                if s.font == font
                    && s.half_points == st.half_points
                    && s.bold == st.bold
                    && s.italic == st.italic =>
            {
                s.text.push(c);
            }
            _ => rich.spans.push(Span {
                text: c.to_string(),
                font,
                half_points: st.half_points,
                bold: st.bold,
                italic: st.italic,
            }),
        }
    };
    let mut i = 0;
    while i < src.len() {
        let c = src[i];
        match c {
            '{' => {
                stack.push(st.clone());
                i += 1;
                // A destination to leave out.
                if src.get(i) == Some(&'\\') && src.get(i + 1) == Some(&'*') {
                    st.skip = true;
                }
            }
            '}' => {
                if in_fonts && !font_name.is_empty() {
                    let name = font_name.trim().trim_end_matches(';').trim().to_string();
                    fonts.push((font_number, name));
                    font_name.clear();
                }
                if let Some(prev) = stack.pop() {
                    let leaving_fonts = in_fonts && !prev.skip && prev_is_fonts(&stack);
                    st = prev;
                    if leaving_fonts || stack.is_empty() {
                        in_fonts = false;
                    }
                }
                i += 1;
            }
            '\\' => {
                i += 1;
                let Some(&n) = src.get(i) else { break };
                if n == '\'' {
                    // A byte of the code page, in hex.
                    let hex: String = src.iter().skip(i + 1).take(2).collect();
                    i += 3;
                    if let Ok(b) = u8::from_str_radix(&hex, 16) {
                        if skip_chars > 0 {
                            skip_chars -= 1;
                        } else if in_fonts {
                            font_name.push(cp1252(b));
                        } else if !st.skip {
                            push(&mut rich, &st, &fonts, cp1252(b));
                        }
                    }
                    continue;
                }
                if !n.is_ascii_alphabetic() {
                    // A control symbol: an escaped character, or ignored.
                    i += 1;
                    match n {
                        '\\' | '{' | '}' if !st.skip && !in_fonts => {
                            push(&mut rich, &st, &fonts, n)
                        }
                        '~' if !st.skip && !in_fonts => push(&mut rich, &st, &fonts, '\u{A0}'),
                        '*' => st.skip = true,
                        _ => {}
                    }
                    continue;
                }
                let start = i;
                while i < src.len() && src[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let word: String = src[start..i].iter().collect();
                let num_start = i;
                if i < src.len() && (src[i] == '-' || src[i].is_ascii_digit()) {
                    i += 1;
                    while i < src.len() && src[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                let arg: Option<i64> = src[num_start..i].iter().collect::<String>().parse().ok();
                if src.get(i) == Some(&' ') {
                    i += 1;
                }
                match word.as_str() {
                    "fonttbl" => in_fonts = true,
                    "colortbl" | "stylesheet" | "info" | "pict" | "header" | "footer" => {
                        st.skip = true
                    }
                    "f" if in_fonts => font_number = arg.unwrap_or(0).max(0) as usize,
                    "f" => st.font = arg.unwrap_or(0).max(0) as usize,
                    "fs" => st.half_points = arg.unwrap_or(24).clamp(1, 32_000) as u32,
                    "b" => st.bold = arg != Some(0),
                    "i" => st.italic = arg != Some(0),
                    "plain" => {
                        st.bold = false;
                        st.italic = false;
                        st.half_points = 24;
                    }
                    "uc" => st.uc = arg.unwrap_or(1).max(0) as usize,
                    "u" => {
                        let v = arg.unwrap_or(0);
                        let v = if v < 0 { v + 65_536 } else { v };
                        if let Some(ch) = char::from_u32(v as u32)
                            && !st.skip
                            && !in_fonts
                        {
                            push(&mut rich, &st, &fonts, ch);
                        }
                        skip_chars = st.uc;
                    }
                    "par" | "line" if !st.skip && !in_fonts => {
                        push(&mut rich, &st, &fonts, '\n');
                        paragraph += 1;
                    }
                    "tab" if !st.skip && !in_fonts => push(&mut rich, &st, &fonts, '\t'),
                    "pard" => begun = true,
                    "qc" | "qr" | "ql" | "qj" if justify.is_none() && paragraph == 0 => {
                        justify = Some(match word.as_str() {
                            "qc" => 1,
                            "qr" => 2,
                            _ => 0,
                        });
                    }
                    _ => {}
                }
            }
            // The source's own line breaks and tabs are its layout (a tab
            // of the text is a control word).
            '\r' | '\n' | '\t' => i += 1,
            _ => {
                i += 1;
                if skip_chars > 0 {
                    skip_chars -= 1;
                } else if in_fonts {
                    font_name.push(c);
                } else if !st.skip && (begun || !c.is_whitespace()) {
                    push(&mut rich, &st, &fonts, c);
                }
            }
        }
    }
    // The paragraph mark after the last line is no line of its own.
    if let Some(last) = rich.spans.last_mut()
        && last.text.ends_with('\n')
    {
        last.text.pop();
        if last.text.is_empty() {
            rich.spans.pop();
        }
    }
    rich.justify = justify.unwrap_or(0);
    rich
}

/// Whether the group being closed holds the font table (its parent is the
/// document's).
fn prev_is_fonts(stack: &[State]) -> bool {
    stack.len() <= 1
}

/// RTF for `spans` justified `justify` (0 left, 1 center, 2 right), as
/// Titles & Text keeps it: a font table, a paragraph a line.
pub(crate) fn write(spans: &[Span], justify: i32) -> String {
    let mut fonts: Vec<&str> = Vec::new();
    for s in spans {
        if !fonts.contains(&s.font.as_str()) {
            fonts.push(&s.font);
        }
    }
    if fonts.is_empty() {
        fonts.push("Arial");
    }
    let mut out = String::from("{\\rtf1\\ansi\\ansicpg1252\\deff0{\\fonttbl");
    for (n, f) in fonts.iter().enumerate() {
        out.push_str(&format!("{{\\f{n}\\fnil\\fcharset0 {f};}}"));
    }
    out.push_str("}\r\n\\viewkind4\\uc1\\pard");
    out.push_str(match justify {
        1 => "\\qc",
        2 => "\\qr",
        _ => "",
    });
    out.push_str("\\lang1033");
    let (mut font, mut size, mut bold, mut italic) = (usize::MAX, 0, false, false);
    for s in spans {
        let n = fonts.iter().position(|f| *f == s.font).unwrap_or(0);
        let mut words = String::new();
        if n != font {
            words.push_str(&format!("\\f{n}"));
            font = n;
        }
        if s.bold != bold {
            words.push_str(if s.bold { "\\b" } else { "\\b0" });
            bold = s.bold;
        }
        if s.italic != italic {
            words.push_str(if s.italic { "\\i" } else { "\\i0" });
            italic = s.italic;
        }
        if s.half_points != size {
            words.push_str(&format!("\\fs{}", s.half_points));
            size = s.half_points;
        }
        if !words.is_empty() {
            out.push_str(&words);
            out.push(' ');
        }
        for c in s.text.chars() {
            match c {
                '\n' => out.push_str("\\par\r\n"),
                '\t' => out.push_str("\\tab "),
                '\\' | '{' | '}' => {
                    out.push('\\');
                    out.push(c);
                }
                c if (c as u32) < 0x80 => out.push(c),
                c => {
                    let v = c as u32;
                    if v <= 0xFFFF {
                        out.push_str(&format!("\\u{}?", v as u16 as i16));
                    } else {
                        let mut units = [0u16; 2];
                        for u in c.encode_utf16(&mut units) {
                            out.push_str(&format!("\\u{}?", *u as i16));
                        }
                    }
                }
            }
        }
    }
    out.push_str("\\par\r\n}\r\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_text_reads_into_spans() {
        let r = read(
            "{\\rtf1\\ansi\\ansicpg1252\\deff0{\\fonttbl{\\f0\\fnil\\fcharset0 Arial;}} \\viewkind4\\uc1\\pard\\qc\\lang1033\\fs28 FREE CUSTOMER\\par \\fs52 SUPPORT\\fs28\\par }",
        );
        assert_eq!(r.justify, 1);
        assert_eq!(r.spans.len(), 2);
        assert_eq!(r.spans[0].text, "FREE CUSTOMER\n");
        assert_eq!(
            (r.spans[0].font.as_str(), r.spans[0].half_points),
            ("Arial", 28)
        );
        assert_eq!(
            (r.spans[1].text.as_str(), r.spans[1].half_points),
            ("SUPPORT", 52)
        );
    }

    #[test]
    fn what_is_written_reads_back() {
        let spans = vec![
            Span {
                text: "Caf\u{e9} {1}\n".into(),
                font: "Verdana".into(),
                half_points: 96,
                bold: true,
                italic: false,
            },
            Span {
                text: "\u{2014} two".into(),
                font: "Arial".into(),
                half_points: 48,
                bold: false,
                italic: true,
            },
        ];
        let r = read(&write(&spans, 2));
        assert_eq!(r.spans, spans);
        assert_eq!(r.justify, 2);
    }
}
