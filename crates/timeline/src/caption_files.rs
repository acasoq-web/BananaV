//! Closed captioning files, as VEGAS's Help lists them: File › Import ›
//! Closed Captioning reads Scenarist files (`.scc`: CEA-608's own bytes,
//! turned into caption markup), MacCaption files (`.mcc`: CEA-708's
//! caption data packets, their CEA-608 bytes read as Scenarist's),
//! SubRip (`.srt`), SAMI (`.smi`), RealText (`.rt`), QuickTime text and
//! transcripts (`.txt`) and DVD subtitles (`.sub`); Tools › Scripting
//! writes a channel's captions as SubRip, SAMI (with an `.asx` playlist),
//! QuickTime text (with a `.smil` presentation) and DVD subtitles, and a
//! render writes CC1 and CC2 beside its file as Scenarist's (`write_scc`).

use std::collections::HashMap;
use std::sync::OnceLock;

use bv_core::{FrameRate, Tick, timecode};

use crate::captions::{Caption, pop_on};

/// The extensions read.
pub const EXTENSIONS: [&str; 7] = ["scc", "mcc", "srt", "smi", "rt", "txt", "sub"];

/// The captions of a closed captioning file whose extension is `ext`
/// (CC1's unless the file says otherwise); `rate` counts DVD subtitles'
/// and transcripts' frames. None when it holds none.
pub fn read(ext: &str, text: &str, rate: FrameRate) -> Vec<Caption> {
    let text = text.trim_start_matches('\u{feff}');
    match ext.to_ascii_lowercase().as_str() {
        "scc" => read_scc(text),
        "mcc" => read_mcc(text),
        "srt" => read_srt(text),
        "smi" | "sami" => read_smi(text),
        "rt" => read_rt(text),
        "txt" => read_txt(text, rate),
        "sub" => read_sub(text, rate),
        _ => Vec::new(),
    }
}

/// A file's bytes as text: UTF-8, else Windows' Latin-1 (older caption
/// files).
pub fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

// ---- timed text --------------------------------------------------------------

/// Subtitles (start, end if known, text) as captions: a pop-on caption at
/// each start, the screen cleared at its end unless the next one starts
/// by then.
fn timed(mut subs: Vec<(Tick, Option<Tick>, String)>, channel: usize) -> Vec<Caption> {
    subs.sort_by_key(|s| s.0);
    let mut out = Vec::new();
    for (k, (start, end, text)) in subs.iter().enumerate() {
        let text = text.trim();
        out.push(Caption {
            time: *start,
            channel,
            markup: pop_on(text),
        });
        if let Some(end) = *end
            && !text.is_empty()
            && subs.get(k + 1).is_none_or(|n| n.0 > end)
        {
            out.push(Caption {
                time: end,
                channel,
                markup: "{EDM}".to_string(),
            });
        }
    }
    out
}

/// `text` without its markup tags (`<i>`, `<font …>`, `{\an8}`), its
/// character entities read.
fn strip_tags(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find(['<', '{']) {
        out.push_str(&rest[..i]);
        let close = if rest[i..].starts_with('<') { '>' } else { '}' };
        // A `{` that opens no override is text.
        if close == '}' && !rest[i..].starts_with("{\\") {
            out.push('{');
            rest = &rest[i + 1..];
            continue;
        }
        match rest[i..].find(close) {
            Some(j) => rest = &rest[i + j + 1..],
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    entities(&out)
}

/// HTML's character entities read (`&amp;`, `&nbsp;`, `&#233;`).
fn entities(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let end = after.find(';').filter(|&e| e <= 8);
        let named = end.map(|e| &after[..e]);
        let c = match named {
            Some("amp") => Some('&'),
            Some("lt") => Some('<'),
            Some("gt") => Some('>'),
            Some("quot") => Some('"'),
            Some("apos") => Some('\''),
            Some("nbsp") => Some(' '),
            Some(n) if n.starts_with("#x") || n.starts_with("#X") => {
                u32::from_str_radix(&n[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
            }
            Some(n) if n.starts_with('#') => n[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match (c, end) {
            (Some(c), Some(e)) => {
                out.push(c);
                rest = &after[e + 1..];
            }
            _ => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// "HH:MM:SS,mmm" (or with a dot), and RealText's and QuickTime's shorter
/// forms ("SS.x", "MM:SS.xx"): the fraction in units of `scale` a second
/// when given, else a decimal fraction.
fn clock(s: &str, scale: Option<u32>) -> Option<Tick> {
    let s = s.trim().trim_start_matches('[').trim_end_matches(']');
    let s = s.split_whitespace().next()?;
    let (whole, frac) = match s.rfind([',', '.']) {
        Some(i) => (&s[..i], &s[i + 1..]),
        None => (s, ""),
    };
    let mut secs: f64 = 0.0;
    for part in whole.split(':') {
        let v: f64 = part.trim().parse().ok()?;
        secs = secs * 60.0 + v;
    }
    if !frac.is_empty() {
        let v: f64 = frac.parse().ok()?;
        secs += match scale {
            Some(k) if k > 0 => v / k as f64,
            _ => v / 10f64.powi(frac.len() as i32),
        };
    }
    Some(Tick::from_seconds_f64(secs))
}

fn read_srt(text: &str) -> Vec<Caption> {
    let lines: Vec<&str> = text.lines().collect();
    let mut subs = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        i += 1;
        let Some((a, b)) = line.split_once("-->") else {
            continue;
        };
        let (Some(start), Some(end)) = (clock(a, None), clock(b, None)) else {
            continue;
        };
        let mut body = Vec::new();
        while i < lines.len() && !lines[i].trim().is_empty() {
            body.push(strip_tags(lines[i].trim()));
            i += 1;
        }
        subs.push((start, Some(end), body.join("\n")));
    }
    timed(subs, 0)
}

/// A tag's attribute's value (`name=value`, quoted or not), its name in
/// any case.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name) {
        let at = from + i;
        from = at + name.len();
        let before = lower[..at].chars().last();
        if before.is_some_and(|c| c.is_alphanumeric()) {
            continue;
        }
        let rest = tag[from..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim_start();
        let value = match rest.chars().next() {
            Some(q @ ('"' | '\'')) => rest[1..].split(q).next().unwrap_or(""),
            _ => rest
                .split(|c: char| c.is_whitespace() || c == '>' || c == '/')
                .next()
                .unwrap_or(""),
        };
        return Some(value.to_string());
    }
    None
}

/// HTML-like text: `<br>` a line break, the other tags left out.
fn html_text(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i].replace(['\r', '\n'], " "));
        let Some(j) = rest[i..].find('>') else {
            rest = "";
            break;
        };
        let tag = rest[i + 1..i + j].trim().to_ascii_lowercase();
        if tag == "br" || tag.starts_with("br ") || tag.starts_with("br/") {
            out.push('\n');
        }
        rest = &rest[i + j + 1..];
    }
    out.push_str(&rest.replace(['\r', '\n'], " "));
    entities(&out)
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n")
}

/// SAMI: each `<SYNC Start=ms>`'s paragraphs, a class (a language) a
/// channel — the first CC1, the second CC3 (VEGAS's secondary language),
/// then CC2 and CC4.
fn read_smi(text: &str) -> Vec<Caption> {
    const ORDER: [usize; 4] = [0, 2, 1, 3];
    let lower = text.to_ascii_lowercase();
    let mut classes: Vec<String> = Vec::new();
    let mut syncs: Vec<(usize, Tick, String)> = Vec::new();
    let starts: Vec<usize> = lower.match_indices("<sync").map(|(i, _)| i).collect();
    for (k, &at) in starts.iter().enumerate() {
        let end = starts
            .get(k + 1)
            .copied()
            .or_else(|| lower[at..].find("</body").map(|e| at + e))
            .unwrap_or(text.len());
        let Some(close) = text[at..end].find('>') else {
            continue;
        };
        let Some(ms) = attribute(&text[at..at + close], "start")
            .and_then(|v| v.trim_end_matches("ms").trim().parse::<f64>().ok())
        else {
            continue;
        };
        let time = Tick::from_seconds_f64(ms / 1000.0);
        let body = &text[at + close + 1..end];
        let body_lower = &lower[at + close + 1..end];
        let paragraphs: Vec<usize> = body_lower.match_indices("<p").map(|(i, _)| i).collect();
        if paragraphs.is_empty() {
            syncs.push((0, time, html_text(body)));
            if classes.is_empty() {
                classes.push(String::new());
            }
        }
        for (n, &p) in paragraphs.iter().enumerate() {
            let pend = paragraphs.get(n + 1).copied().unwrap_or(body.len());
            let Some(pclose) = body[p..pend].find('>') else {
                continue;
            };
            let class = attribute(&body[p..p + pclose], "class")
                .unwrap_or_default()
                .to_ascii_lowercase();
            let index = match classes.iter().position(|c| *c == class) {
                Some(i) => i,
                None => {
                    classes.push(class);
                    classes.len() - 1
                }
            };
            let content = &body[p + pclose + 1..pend];
            let content = match content.to_ascii_lowercase().find("</p") {
                Some(e) => &content[..e],
                None => content,
            };
            syncs.push((index, time, html_text(content)));
        }
    }
    let mut out = Vec::new();
    for (index, _) in classes.iter().enumerate().take(4) {
        let channel = ORDER[index];
        let mut last = String::new();
        for (_, time, text) in syncs.iter().filter(|(i, _, _)| *i == index) {
            let text = text.trim().to_string();
            if text == last {
                continue;
            }
            out.push(Caption {
                time: *time,
                channel,
                markup: pop_on(&text),
            });
            last = text;
        }
    }
    out.sort_by_key(|c| c.time);
    out
}

/// RealText: each `<time begin=…/>` starts a caption, the text after it
/// (added to what shows, unless a `<clear/>` came first).
fn read_rt(text: &str) -> Vec<Caption> {
    let mut out: Vec<(Tick, String)> = Vec::new();
    let mut time: Option<Tick> = None;
    let mut shown = String::new();
    let mut rest = text;
    let flush = |out: &mut Vec<(Tick, String)>, time: Option<Tick>, shown: &str| {
        if let Some(t) = time {
            out.push((t, shown.trim().to_string()));
        }
    };
    while !rest.is_empty() {
        let Some(i) = rest.find('<') else {
            shown.push_str(&entities(&rest.replace(['\r', '\n'], " ")));
            break;
        };
        shown.push_str(&entities(&rest[..i].replace(['\r', '\n'], " ")));
        let Some(j) = rest[i..].find('>') else {
            break;
        };
        let tag = &rest[i + 1..i + j];
        let name = tag
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        match name.as_str() {
            "time" => {
                if let Some(t) = attribute(tag, "begin").and_then(|v| clock(&v, None)) {
                    flush(&mut out, time, &shown);
                    time = Some(t);
                }
            }
            "clear" => shown.clear(),
            "br" | "p" => shown.push('\n'),
            _ => {}
        }
        rest = &rest[i + j + 1..];
    }
    flush(&mut out, time, &shown);
    out.into_iter()
        .map(|(time, text)| Caption {
            time,
            channel: 0,
            markup: pop_on(
                &text
                    .lines()
                    .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
                    .filter(|l| !l.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
        })
        .collect()
}

/// QuickTime text (`{QTtext}`: `[HH:MM:SS.ff]` stamps, the fraction in
/// its `{timeScale:…}`'s units, the text after each), or a transcript:
/// lines starting with a timecode (in `rate`) or a bracketed time, each
/// caption shown until the next.
fn read_txt(text: &str, rate: FrameRate) -> Vec<Caption> {
    let quicktime = text
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("{qttext}");
    let mut scale = None;
    let mut subs: Vec<(Tick, Option<Tick>, String)> = Vec::new();
    let mut current: Option<(Tick, Vec<String>)> = None;
    let push = |subs: &mut Vec<(Tick, Option<Tick>, String)>, c: Option<(Tick, Vec<String>)>| {
        if let Some((t, lines)) = c {
            subs.push((t, None, lines.join("\n")));
        }
    };
    for line in text.lines() {
        let line = line.trim();
        if quicktime {
            // Descriptors: `{timeScale:30}`, `{plain}`…
            let lower = line.to_ascii_lowercase();
            if let Some(i) = lower.find("{timescale:") {
                let v = &lower[i + 11..];
                scale = v
                    .split('}')
                    .next()
                    .and_then(|n| n.trim().parse::<u32>().ok());
            }
            if line.starts_with('[')
                && let Some(t) = clock(line, scale)
            {
                push(&mut subs, current.take());
                current = Some((t, Vec::new()));
                continue;
            }
            if let Some((_, lines)) = &mut current {
                let text = strip_braces(line);
                if !text.trim().is_empty() {
                    lines.push(text.trim().to_string());
                }
            }
            continue;
        }
        // A transcript: "00:00:01:15 Text", "[00:00:01.5] Text".
        let (stamp, rest) = match line.split_once(|c: char| c.is_whitespace()) {
            Some((a, b)) => (a, b.trim()),
            None => (line, ""),
        };
        let t = if stamp.starts_with('[') {
            clock(stamp, None)
        } else if stamp.matches([':', ';']).count() == 3 {
            timecode::parse(stamp, rate)
        } else {
            None
        };
        match t {
            Some(t) => {
                push(&mut subs, current.take());
                let lines = if rest.is_empty() {
                    Vec::new()
                } else {
                    vec![rest.to_string()]
                };
                current = Some((t, lines));
            }
            None => {
                if let Some((_, lines)) = &mut current
                    && !line.is_empty()
                {
                    lines.push(line.to_string());
                }
            }
        }
    }
    push(&mut subs, current.take());
    timed(subs, 0)
}

/// `text` without QuickTime's `{…}` descriptors.
fn strip_braces(text: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in text.chars() {
        match c {
            '{' => depth += 1,
            '}' if depth > 0 => depth -= 1,
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// DVD subtitles: "0001  00:00:01:00  00:00:04:00  Text" lines (tabs or
/// spaces, timecodes in `rate`; the lines after one, until the next, more
/// of its text); MicroDVD's "{start}{end}Text|More" frames; SubViewer's
/// "00:00:01.00,00:00:04.00" over its text (`[br]` a line break).
fn read_sub(text: &str, rate: FrameRate) -> Vec<Caption> {
    let mut subs: Vec<(Tick, Option<Tick>, String)> = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        i += 1;
        if line.is_empty() {
            continue;
        }
        // MicroDVD.
        if let Some(r) = line.strip_prefix('{')
            && let Some((a, r)) = r.split_once('}')
            && let Some(r) = r.strip_prefix('{')
            && let Some((b, body)) = r.split_once('}')
            && let (Ok(a), Ok(b)) = (a.trim().parse::<i64>(), b.trim().parse::<i64>())
        {
            subs.push((
                rate.frame_start(a),
                Some(rate.frame_start(b)),
                strip_braces(body).replace('|', "\n"),
            ));
            continue;
        }
        // SubViewer.
        if let Some((a, b)) = line.split_once(',')
            && a.contains(':')
            && b.contains(':')
            && let (Some(start), Some(end)) = (clock(a, None), clock(b, None))
        {
            let body = lines.get(i).map_or("", |l| l.trim());
            i += 1;
            subs.push((
                start,
                Some(end),
                body.replace("[br]", "\n").replace("[BR]", "\n"),
            ));
            continue;
        }
        // DVD subtitles: a number, two timecodes, the text.
        if let Some((start, end, first)) = dvd_line(line, rate) {
            let mut body = vec![first.to_string()];
            while i < lines.len() {
                let next = lines[i].trim();
                if next.is_empty() || dvd_line(next, rate).is_some() {
                    break;
                }
                body.push(next.to_string());
                i += 1;
            }
            let text = body
                .into_iter()
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            subs.push((start, Some(end), text));
        }
    }
    timed(subs, 0)
}

/// A DVD subtitle's first line: its number, start and end timecodes, and
/// the text after them.
fn dvd_line(line: &str, rate: FrameRate) -> Option<(Tick, Tick, &str)> {
    let mut rest = line.trim_start();
    let mut fields = [""; 3];
    for f in &mut fields {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        *f = &rest[..end];
        rest = rest[end..].trim_start();
    }
    let [n, a, b] = fields;
    if n.is_empty() || !n.chars().all(|c| c.is_ascii_digit()) || !a.contains([':', ';']) {
        return None;
    }
    let (start, end) = (timecode::parse(a, rate)?, timecode::parse(b, rate)?);
    Some((start, end, rest.trim()))
}

// ---- Scenarist (CEA-608's bytes) -----------------------------------------------

/// An SCC timecode: always 29.97 frames a second, drop-frame when the
/// frames follow a `;` (or a `.`).
fn scc_time(tc: &str) -> Option<Tick> {
    let drop = tc.contains([';', '.']);
    let parts: Vec<u64> = tc
        .split([':', ';', '.'])
        .map(|p| p.trim().parse().ok())
        .collect::<Option<_>>()?;
    let [h, m, s, f] = parts[..] else {
        return None;
    };
    if m >= 60 || s >= 60 || f >= 30 {
        return None;
    }
    let mut frame = ((h * 60 + m) * 60 + s) * 30 + f;
    if drop {
        let minutes = h * 60 + m;
        frame -= 2 * (minutes - minutes / 10);
    }
    Some(FrameRate::FPS_29_97.frame_start(frame as i64))
}

/// CEA-608's standard characters where they are not ASCII's.
fn standard_char(b: u8) -> char {
    match b {
        0x2A => 'á',
        0x5C => 'é',
        0x5E => 'í',
        0x5F => 'ó',
        0x60 => 'ú',
        0x7B => 'ç',
        0x7C => '÷',
        0x7D => 'Ñ',
        0x7E => 'ñ',
        0x7F => '█',
        b => b as char,
    }
}

const SPECIAL: [char; 16] = [
    '®', '°', '½', '¿', '™', '¢', '£', '♪', 'à', ' ', 'è', 'â', 'ê', 'î', 'ô', 'û',
];
const EXTENDED_12: [char; 32] = [
    'Á', 'É', 'Ó', 'Ú', 'Ü', 'ü', '‘', '¡', '*', '\'', '—', '©', '℠', '•', '“', '”', 'À', 'Â', 'Ç',
    'È', 'Ê', 'Ë', 'ë', 'Î', 'Ï', 'ï', 'Ô', 'Ù', 'ù', 'Û', '«', '»',
];
const EXTENDED_13: [char; 32] = [
    'Ã', 'ã', 'Í', 'Ì', 'ì', 'Ò', 'ò', 'Õ', 'õ', '{', '}', '\\', '^', '_', '|', '~', 'Ä', 'ä', 'Ö',
    'ö', 'ß', '¥', '¤', '¦', 'Å', 'å', 'Ø', 'ø', '┌', '┐', '└', '┘',
];
const COLORS: [&str; 8] = ["Wh", "Gr", "Bl", "Cy", "Rd", "Yl", "Ma", "Blk"];

/// What a control pair of channel 1's bytes (`a` 0x10–0x17) is.
enum Control {
    /// Markup: a code's name.
    Code(String),
    /// A character.
    Char(char),
    /// An extended character, in place of the one before it.
    Replace(char),
}

fn control(a: u8, b: u8) -> Option<Control> {
    let code = |s: &str| Some(Control::Code(s.to_string()));
    match (a, b) {
        (0x14 | 0x15, 0x20..=0x2F) => match b {
            0x20 => code("RCL"),
            0x21 => code("Backspace"),
            0x24 => code("DelEndRow"),
            0x25 => code("RU2"),
            0x26 => code("RU3"),
            0x27 => code("RU4"),
            0x28 => code("FlashOn"),
            0x29 => code("RDC"),
            0x2C => code("EDM"),
            0x2D => code("CR"),
            0x2E => code("ENM"),
            0x2F => code("EOC"),
            // Alarms and text mode: not captions.
            _ => None,
        },
        (0x17, 0x21..=0x23) => code(&format!("TabOff{}", b - 0x20)),
        (0x17, 0x24..=0x2A) => code(
            [
                "StdCharSet",
                "DSzCharSet",
                "1PCharSet",
                "2PCharSet",
                "CHNCharset",
                "KORCharSet",
                "1RgCharSet",
            ][(b - 0x24) as usize],
        ),
        (0x17, 0x2D) => code("BgTran"),
        (0x17, 0x2E) => code("BlkTxt"),
        (0x17, 0x2F) => code("BlkUTxt"),
        (0x11, 0x20..=0x2F) => {
            let k = (b - 0x20) as usize;
            let u = if k % 2 == 1 { "U" } else { "" };
            if k / 2 == 7 {
                code(&format!("I{u}Txt"))
            } else {
                code(&format!("{}{u}Txt", COLORS[k / 2]))
            }
        }
        (0x11, 0x30..=0x3F) => Some(Control::Char(SPECIAL[(b - 0x30) as usize])),
        (0x12, 0x20..=0x3F) => Some(Control::Replace(EXTENDED_12[(b - 0x20) as usize])),
        (0x13, 0x20..=0x3F) => Some(Control::Replace(EXTENDED_13[(b - 0x20) as usize])),
        (0x10, 0x20..=0x2F) => {
            let k = (b - 0x20) as usize;
            let semi = if k % 2 == 1 { "Semi" } else { "" };
            code(&format!("Bg{}{semi}", COLORS[k / 2]))
        }
        (0x10..=0x17, 0x40..=0x7F) => {
            let low = b >= 0x60;
            let row = match (a, low) {
                (0x11, false) => 1,
                (0x11, true) => 2,
                (0x12, false) => 3,
                (0x12, true) => 4,
                (0x15, false) => 5,
                (0x15, true) => 6,
                (0x16, false) => 7,
                (0x16, true) => 8,
                (0x17, false) => 9,
                (0x17, true) => 10,
                (0x10, false) => 11,
                (0x13, false) => 12,
                (0x13, true) => 13,
                (0x14, false) => 14,
                (0x14, true) => 15,
                _ => return None,
            };
            let attr = (b & 0x1F) as usize;
            let u = if attr % 2 == 1 { "U" } else { "" };
            if attr >= 0x10 {
                code(&format!("R{row:02}In{:02}Wh{u}", (attr - 0x10) / 2 * 4))
            } else if attr / 2 == 7 {
                code(&format!("R{row:02}WhI{u}"))
            } else {
                code(&format!("R{row:02}{}{u}", COLORS[attr / 2]))
            }
        }
        _ => None,
    }
}

/// Scenarist: each line's timecode and byte pairs (in hex, odd parity).
fn read_scc(text: &str) -> Vec<Caption> {
    let lines = text
        .lines()
        .filter_map(|line| {
            let (tc, data) = line.trim().split_once(|c: char| c.is_whitespace())?;
            let words = data
                .split_whitespace()
                .filter_map(|w| u16::from_str_radix(w, 16).ok())
                .collect();
            Some((scc_time(tc)?, words))
        })
        .collect();
    read_pairs(lines, FrameRate::FPS_29_97.frame_duration(), 0)
}

/// CEA-608 byte pairs of one field (from `channel` on: CC1 and CC2, or
/// CC3 and CC4), sent a pair a `word`, in lines (when each starts),
/// turned into VEGAS's markup — a marker per line and channel, at the
/// caption's End of Caption for pop-on captions (when it shows), else at
/// the line's time. Control pairs sent twice count once.
fn read_pairs(lines: Vec<(Tick, Vec<u16>)>, word: Tick, channel: usize) -> Vec<Caption> {
    let mut out = Vec::new();
    let first = channel;
    let mut channel = 0;
    let mut last_control: Option<(u8, u8)> = None;
    for (start, words) in lines {
        let mut markup = [String::new(), String::new()];
        let mut shows: [Option<usize>; 2] = [None, None];
        let mut char_last = [false, false];
        for (k, &v) in words.iter().enumerate() {
            let (a, b) = (((v >> 8) as u8) & 0x7F, (v as u8) & 0x7F);
            if a == 0 && b == 0 {
                continue;
            }
            if (0x10..=0x1F).contains(&a) {
                if last_control == Some((a, b)) {
                    last_control = None;
                    continue;
                }
                last_control = Some((a, b));
                let ch = usize::from(a >= 0x18);
                let Some(c) = control(a & !0x08, b) else {
                    continue;
                };
                channel = ch;
                match c {
                    Control::Code(s) => {
                        if s == "EOC" && shows[ch].is_none() {
                            shows[ch] = Some(k);
                        }
                        markup[ch].push('{');
                        markup[ch].push_str(&s);
                        markup[ch].push('}');
                        char_last[ch] = false;
                    }
                    Control::Char(c) => {
                        markup[ch].push(c);
                        char_last[ch] = true;
                    }
                    Control::Replace(c) => {
                        if char_last[ch] {
                            markup[ch].pop();
                        }
                        markup[ch].push(c);
                        char_last[ch] = true;
                    }
                }
                continue;
            }
            last_control = None;
            for byte in [a, b] {
                if byte >= 0x20 {
                    markup[channel].push(standard_char(byte));
                    char_last[channel] = true;
                }
            }
        }
        for ch in 0..2 {
            if !markup[ch].is_empty() {
                let at = shows[ch].unwrap_or(0) as i64;
                out.push(Caption {
                    time: start + Tick(word.0 * at),
                    channel: first + ch,
                    markup: std::mem::take(&mut markup[ch]),
                });
            }
        }
    }
    out
}

// ---- MacCaption (CEA-708's caption data packets) ------------------------------

/// MacCaption's letters for runs of bytes.
fn mcc_alias(c: char) -> Option<&'static [u8]> {
    const FA: [u8; 27] = [
        0xFA, 0, 0, 0xFA, 0, 0, 0xFA, 0, 0, 0xFA, 0, 0, 0xFA, 0, 0, 0xFA, 0, 0, 0xFA, 0, 0, 0xFA,
        0, 0, 0xFA, 0, 0,
    ];
    Some(match c {
        'G'..='O' => &FA[..3 * (c as usize - 'G' as usize + 1)],
        'P' => &[0xFB, 0x80, 0x80],
        'Q' => &[0xFC, 0x80, 0x80],
        'R' => &[0xFD, 0x80, 0x80],
        'S' => &[0x96, 0x69],
        'T' => &[0x61, 0x01],
        'U' => &[0xE1, 0x00, 0x00, 0x00],
        'Z' => &[0x00],
        _ => return None,
    })
}

/// A MacCaption line's bytes: hex pairs and the letters for runs.
fn mcc_bytes(data: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let chars: Vec<char> = data.trim().chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if let Some(run) = mcc_alias(chars[i]) {
            out.extend_from_slice(run);
            i += 1;
        } else if i + 1 < chars.len()
            && let (Some(a), Some(b)) = (chars[i].to_digit(16), chars[i + 1].to_digit(16))
        {
            out.push((a * 16 + b) as u8);
            i += 2;
        } else {
            i += 1;
        }
    }
    out
}

/// The CEA-608 pairs in a caption data packet (`96 69`…): field 1's and
/// field 2's.
fn cdp_pairs(bytes: &[u8]) -> [Vec<u16>; 2] {
    let mut out = [Vec::new(), Vec::new()];
    let Some(at) = bytes.windows(2).position(|w| w == [0x96, 0x69]) else {
        return out;
    };
    let cdp = &bytes[at..];
    let Some(&flags) = cdp.get(4) else {
        return out;
    };
    // Header: identifier, length, frame rate, flags, counter; then a time
    // code section when the flags say so.
    let mut i = 7;
    if flags & 0x80 != 0 && cdp.get(i) == Some(&0x71) {
        i += 5;
    }
    if cdp.get(i) != Some(&0x72) {
        return out;
    }
    let count = usize::from(cdp.get(i + 1).map_or(0, |c| c & 0x1F));
    for k in 0..count {
        let Some(t) = cdp.get(i + 2 + k * 3..i + 5 + k * 3) else {
            break;
        };
        let (valid, kind) = (t[0] & 0x04 != 0, t[0] & 0x03);
        if valid && kind < 2 {
            out[kind as usize].push(u16::from(t[1]) << 8 | u16::from(t[2]));
        }
    }
    out
}

/// MacCaption: a line a frame of its time code rate, each a caption data
/// packet; their CEA-608 pairs (field 1: CC1 and CC2, field 2: CC3 and
/// CC4) read as Scenarist's, runs of frames carrying some as a line.
fn read_mcc(text: &str) -> Vec<Caption> {
    let mut rate = FrameRate::FPS_29_97;
    let mut frames: Vec<(Tick, [Vec<u16>; 2])> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(r) = line.strip_prefix("Time Code Rate=") {
            rate = match r.trim() {
                "24" => FrameRate::new(24, 1),
                "25" => FrameRate::new(25, 1),
                "30" => FrameRate::new(30, 1),
                "50" => FrameRate::new(50, 1),
                "60" => FrameRate::new(60, 1),
                "60DF" => FrameRate::new(60_000, 1001),
                _ => FrameRate::FPS_29_97,
            };
            continue;
        }
        let Some((tc, data)) = line.split_once('\t') else {
            continue;
        };
        // "HH:MM:SS:FF" (".1": the second field of a frame).
        let tc = tc.split('.').next().unwrap_or(tc);
        if !tc.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        let Some(at) = timecode::parse(tc, rate) else {
            continue;
        };
        frames.push((at, cdp_pairs(&mcc_bytes(data))));
    }
    let word = rate.frame_duration();
    let mut out = Vec::new();
    for field in 0..2 {
        // Runs of frames with pairs that say something.
        let mut lines: Vec<(Tick, Vec<u16>)> = Vec::new();
        let mut open = false;
        for (at, pairs) in &frames {
            let words: Vec<u16> = pairs[field]
                .iter()
                .copied()
                .filter(|w| w & 0x7F7F != 0)
                .collect();
            if words.is_empty() {
                open = false;
                continue;
            }
            match lines.last_mut() {
                Some(l) if open => l.1.extend(words),
                _ => lines.push((*at, words)),
            }
            open = true;
        }
        out.extend(read_pairs(lines, word, field * 2));
    }
    out.sort_by_key(|c| (c.time, c.channel));
    out
}

// ---- writing Scenarist ----------------------------------------------------------

/// The byte pairs (channel 1's) of the markup's codes, special characters
/// and extended characters, from the table the reading uses.
struct Codes {
    codes: HashMap<String, (u8, u8)>,
    special: HashMap<char, (u8, u8)>,
    extended: HashMap<char, (u8, u8)>,
    standard: HashMap<char, u8>,
}

fn codes() -> &'static Codes {
    static CODES: OnceLock<Codes> = OnceLock::new();
    CODES.get_or_init(|| {
        let mut c = Codes {
            codes: HashMap::new(),
            special: HashMap::new(),
            extended: HashMap::new(),
            standard: HashMap::new(),
        };
        for a in 0x10..=0x17u8 {
            for b in 0x20..=0x7Fu8 {
                match control(a, b) {
                    Some(Control::Code(n)) => {
                        c.codes.entry(n).or_insert((a, b));
                    }
                    Some(Control::Char(ch)) => {
                        c.special.entry(ch).or_insert((a, b));
                    }
                    Some(Control::Replace(ch)) => {
                        c.extended.entry(ch).or_insert((a, b));
                    }
                    None => {}
                }
            }
        }
        for b in 0x20..=0x7Fu8 {
            c.standard.entry(standard_char(b)).or_insert(b);
        }
        c
    })
}

/// The plain letter an extended character stands on (what decoders
/// without it show).
fn plain_letter(ch: char) -> u8 {
    match ch {
        'Á' | 'À' | 'Â' | 'Ã' | 'Ä' | 'Å' => b'A',
        'á' | 'à' | 'â' | 'ã' | 'ä' | 'å' => b'a',
        'É' | 'È' | 'Ê' | 'Ë' => b'E',
        'é' | 'è' | 'ê' | 'ë' => b'e',
        'Í' | 'Ì' | 'Î' | 'Ï' => b'I',
        'í' | 'ì' | 'î' | 'ï' => b'i',
        'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' | 'Ø' => b'O',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'ø' => b'o',
        'Ú' | 'Ù' | 'Û' | 'Ü' => b'U',
        'ú' | 'ù' | 'û' | 'ü' => b'u',
        'Ç' => b'C',
        'ß' => b's',
        '«' | '»' | '“' | '”' => 0x22,
        '‘' => 0x27,
        _ => b'-',
    }
}

/// A byte with odd parity.
fn odd(b: u8) -> u8 {
    let b = b & 0x7F;
    if b.count_ones().is_multiple_of(2) {
        b | 0x80
    } else {
        b
    }
}

/// A caption's markup as CEA-608 words of channel `channel` (0 or 1 of
/// field 1): codes and special characters sent twice, standard characters
/// two a word, an extended character after a standard one it replaces.
/// The word its End of Caption is in, if it has one.
fn encode(markup: &str, channel: usize) -> (Vec<u16>, Option<usize>) {
    let c = codes();
    let mut words: Vec<u16> = Vec::new();
    let mut pending: Option<u8> = None;
    let mut eoc = None;
    let word = |a: u8, b: u8| u16::from(odd(a)) << 8 | u16::from(odd(b));
    let flush = |words: &mut Vec<u16>, pending: &mut Option<u8>| {
        if let Some(p) = pending.take() {
            words.push(word(p, 0));
        }
    };
    let control = |words: &mut Vec<u16>, (a, b): (u8, u8)| {
        let a = if channel == 1 { a | 0x08 } else { a };
        words.push(word(a, b));
        words.push(word(a, b));
    };
    let standard = |words: &mut Vec<u16>, pending: &mut Option<u8>, b: u8| match pending.take() {
        Some(p) => words.push(word(p, b)),
        None => *pending = Some(b),
    };
    let mut chars = markup.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '{' {
            let name: String = chars.by_ref().take_while(|&x| x != '}').collect();
            if let Some(&pair) = c.codes.get(&name) {
                flush(&mut words, &mut pending);
                if name == "EOC" && eoc.is_none() {
                    eoc = Some(words.len());
                }
                control(&mut words, pair);
            }
            continue;
        }
        if let Some(&b) = c.standard.get(&ch) {
            standard(&mut words, &mut pending, b);
        } else if let Some(&pair) = c.special.get(&ch) {
            flush(&mut words, &mut pending);
            control(&mut words, pair);
        } else if let Some(&pair) = c.extended.get(&ch) {
            // The plain letter first, for decoders without it.
            let plain = plain_letter(ch);
            standard(&mut words, &mut pending, plain);
            flush(&mut words, &mut pending);
            control(&mut words, pair);
        }
    }
    flush(&mut words, &mut pending);
    (words, eoc)
}

/// CC1's and CC2's captions (time, channel 0 or 1, markup; `offset`
/// taken away: the file's start) as a Scenarist file: each caption a line
/// starting as many frames before its time as its End of Caption comes
/// after the line's start, never before the line before it ended.
pub fn write_scc(captions: &[Caption], offset: Tick) -> String {
    let rate = FrameRate::FPS_29_97;
    let mut out = String::from("Scenarist_SCC V1.0\n");
    let mut sorted: Vec<&Caption> = captions.iter().filter(|c| c.channel < 2).collect();
    sorted.sort_by_key(|c| c.time);
    let mut free = 0i64;
    for c in sorted {
        let (words, eoc) = encode(&c.markup, c.channel);
        if words.is_empty() {
            continue;
        }
        let at = rate.frame_at((c.time - offset).max(Tick::ZERO));
        let start = (at - eoc.unwrap_or(0) as i64).max(free).max(0);
        free = start + words.len() as i64;
        let hex: Vec<String> = words.iter().map(|w| format!("{w:04x}")).collect();
        out.push_str(&format!(
            "\n{}\t{}\n",
            timecode::format_frame(start, rate),
            hex.join(" ")
        ));
    }
    out
}

// ---- writing -----------------------------------------------------------------

/// "00:01:02,345" (SubRip's; `sep` '.' for QuickTime's).
fn clock_text(t: Tick, sep: char) -> String {
    let ms = (t.as_seconds_f64() * 1000.0).round().max(0.0) as u64;
    format!(
        "{:02}:{:02}:{:02}{sep}{:03}",
        ms / 3_600_000,
        (ms / 60_000) % 60,
        (ms / 1000) % 60,
        ms % 1000
    )
}

/// A channel's captions (start, end, text; `offset` added: the ruler's)
/// as SubRip.
pub fn write_srt(spans: &[(Tick, Tick, String)], offset: Tick) -> String {
    spans
        .iter()
        .enumerate()
        .map(|(k, (a, b, text))| {
            format!(
                "{}\n{} --> {}\n{}\n",
                k + 1,
                clock_text(*a + offset, ','),
                clock_text(*b + offset, ','),
                text
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// A channel's captions as SAMI, its class `class` (CC1…), the page
/// titled `title`.
pub fn write_smi(spans: &[(Tick, Tick, String)], offset: Tick, title: &str, class: &str) -> String {
    let ms = |t: Tick| ((t + offset).as_seconds_f64() * 1000.0).round().max(0.0) as u64;
    let mut out = format!(
        "<SAMI>\n<HEAD>\n<TITLE>{}</TITLE>\n<STYLE TYPE=\"text/css\">\n<!--\n\
         P {{ margin-left: 8pt; margin-right: 8pt; margin-bottom: 2pt; margin-top: 2pt;\n    \
         text-align: center; font-size: 14pt; font-family: Arial, sans-serif;\n    \
         font-weight: normal; color: #ffffff; background-color: #000000; }}\n\
         .{class} {{ Name: \"{class}\"; lang: en-US; SAMIType: CC; }}\n-->\n</STYLE>\n</HEAD>\n<BODY>\n",
        html(title)
    );
    for (k, (a, b, text)) in spans.iter().enumerate() {
        let body = text.lines().map(html).collect::<Vec<_>>().join("<br>");
        out.push_str(&format!(
            "<SYNC Start={}><P Class={class}>{body}</P></SYNC>\n",
            ms(*a)
        ));
        if spans.get(k + 1).is_none_or(|n| n.0 > *b) {
            out.push_str(&format!(
                "<SYNC Start={}><P Class={class}>&nbsp;</P></SYNC>\n",
                ms(*b)
            ));
        }
    }
    out.push_str("</BODY>\n</SAMI>\n");
    out
}

/// The playlist that plays `movie` with SAMI file `smi`'s captions.
pub fn write_asx(movie: &str, smi: &str) -> String {
    format!(
        "<ASX version=\"3.0\">\n  <ENTRY>\n    <REF HREF=\"{}?sami={}\" />\n  </ENTRY>\n</ASX>\n",
        html(movie),
        html(smi)
    )
}

/// A channel's captions as QuickTime text (milliseconds as its time
/// scale).
pub fn write_qt_text(spans: &[(Tick, Tick, String)], offset: Tick) -> String {
    let mut out = String::from(
        "{QTtext}{font:Arial}{plain}{size:14}{textColor: 65535, 65535, 65535}\
         {backColor: 0, 0, 0}{justify:center}{timeScale:1000}{width:320}{height:60}\
         {timeStamps:absolute}{language:0}\n",
    );
    for (k, (a, b, text)) in spans.iter().enumerate() {
        out.push_str(&format!("[{}]\n{text}\n", clock_text(*a + offset, '.')));
        if spans.get(k + 1).is_none_or(|n| n.0 > *b) {
            out.push_str(&format!("[{}]\n\n", clock_text(*b + offset, '.')));
        }
    }
    out
}

/// The presentation that plays `movie` with QuickTime text file `txt`
/// under it.
pub fn write_smil(movie: &str, txt: &str) -> String {
    format!(
        "<smil xmlns:qt=\"http://www.apple.com/quicktime/resources/smilextensions\" \
         qt:autoplay=\"true\" qt:time-slider=\"true\">\n  <head>\n    <layout>\n      \
         <root-layout width=\"320\" height=\"300\" background-color=\"black\"/>\n      \
         <region id=\"videoregion\" top=\"0\" left=\"0\" width=\"320\" height=\"240\"/>\n      \
         <region id=\"textregion\" top=\"240\" left=\"0\" width=\"320\" height=\"60\"/>\n    \
         </layout>\n  </head>\n  <body>\n    <par>\n      \
         <video src=\"{}\" region=\"videoregion\"/>\n      \
         <textstream src=\"{}\" region=\"textregion\"/>\n    </par>\n  </body>\n</smil>\n",
        html(movie),
        html(txt)
    )
}

/// A channel's captions as DVD subtitles: a number, the start and end
/// timecodes (in `rate`) and the text, its other rows on the lines after.
pub fn write_sub(spans: &[(Tick, Tick, String)], offset: Tick, rate: FrameRate) -> String {
    spans
        .iter()
        .enumerate()
        .map(|(k, (a, b, text))| {
            format!(
                "{:04}\t{}\t{}\t{}\n",
                k + 1,
                timecode::format(*a + offset, rate),
                timecode::format(*b + offset, rate),
                text
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::captions::rows_text;

    fn s(x: f64) -> Tick {
        Tick::from_seconds_f64(x)
    }

    fn texts(c: &[Caption]) -> Vec<(Tick, Vec<String>)> {
        c.iter().map(|c| (c.time, rows_text(&c.markup))).collect()
    }

    #[test]
    fn subrip_files_round_trip() {
        let spans = vec![
            (s(1.0), s(3.5), "Hello <there>".to_string()),
            (s(3.5), s(6.0), "Two\nrows".to_string()),
        ];
        let file = write_srt(&spans, Tick::ZERO);
        assert!(file.starts_with("1\n00:00:01,000 --> 00:00:03,500\nHello <there>\n"));
        let back = read("srt", &file, FrameRate::FPS_25);
        // `<there>` reads as a tag: SubRip's own markup.
        assert_eq!(
            texts(&back),
            [
                (s(1.0), vec!["Hello".to_string()]),
                (s(3.5), vec!["Two".to_string(), "rows".to_string()]),
                (s(6.0), vec![])
            ]
        );
    }

    #[test]
    fn sami_files_give_a_channel_a_language() {
        let spans = vec![(s(1.0), s(2.0), "Hi & bye".to_string())];
        let smi = write_smi(&spans, Tick::ZERO, "Captions", "CC1");
        assert!(smi.contains("<SYNC Start=1000><P Class=CC1>Hi &amp; bye</P></SYNC>"));
        assert!(smi.contains("<SYNC Start=2000><P Class=CC1>&nbsp;</P></SYNC>"));
        let back = read("smi", &smi, FrameRate::FPS_25);
        assert_eq!(
            texts(&back),
            [(s(1.0), vec!["Hi & bye".to_string()]), (s(2.0), vec![])]
        );
        // Two languages: the second is CC3's.
        let two = "<SAMI><BODY><SYNC Start=500><P Class=ENCC>Hello<br>world\
                   <P Class=ESCC>Hola</SYNC></BODY></SAMI>";
        let c = read("smi", two, FrameRate::FPS_25);
        assert_eq!(c.len(), 2);
        assert_eq!((c[0].channel, rows_text(&c[0].markup).len()), (0, 2));
        assert_eq!(
            (c[1].channel, rows_text(&c[1].markup)),
            (2, vec!["Hola".to_string()])
        );
        assert!(write_asx("Movie.wmv", "Movie.smi").contains("Movie.wmv?sami=Movie.smi"));
    }

    #[test]
    fn quicktime_text_and_dvd_subtitles_round_trip() {
        let spans = vec![(s(2.0), s(4.0), "One".to_string())];
        let qt = write_qt_text(&spans, Tick::ZERO);
        assert_eq!(
            texts(&read("txt", &qt, FrameRate::FPS_25)),
            [(s(2.0), vec!["One".to_string()]), (s(4.0), vec![])]
        );
        let r = FrameRate::FPS_25;
        let sub = write_sub(&spans, Tick::ZERO, r);
        assert_eq!(sub, "0001\t00:00:02:00\t00:00:04:00\tOne\n");
        assert_eq!(
            texts(&read("sub", &sub, r)),
            [(s(2.0), vec!["One".to_string()]), (s(4.0), vec![])]
        );
        // MicroDVD's frames, SubViewer's times.
        let micro = read("sub", "{50}{100}Top|Bottom", r);
        assert_eq!(
            texts(&micro)[0],
            (s(2.0), vec!["Top".into(), "Bottom".into()])
        );
        let viewer = read("sub", "00:00:01.00,00:00:02.00\nA[br]B\n", r);
        assert_eq!(texts(&viewer)[0], (s(1.0), vec!["A".into(), "B".into()]));
        assert!(write_smil("Movie.mov", "Movie.txt").contains("textstream src=\"Movie.txt\""));
    }

    #[test]
    fn realtext_and_transcripts() {
        let rt = "<window><time begin=\"1.5\"/><clear/>First<br/>line\
                  <time begin=\"0:00:04\"/><clear/>Second</window>";
        assert_eq!(
            texts(&read("rt", rt, FrameRate::FPS_25)),
            [
                (s(1.5), vec!["First".to_string(), "line".to_string()]),
                (s(4.0), vec!["Second".to_string()])
            ]
        );
        let r = FrameRate::FPS_25;
        let tr = "00:00:01:00 Hello\n00:00:03:00 Bye\n";
        assert_eq!(
            texts(&read("txt", tr, r)),
            [
                (s(1.0), vec!["Hello".to_string()]),
                (s(3.0), vec!["Bye".to_string()])
            ]
        );
    }

    /// Captions written as Scenarist's come back as they went (CC1 and
    /// CC2, special and extended characters), each at its time.
    #[test]
    fn scenarist_files_are_written() {
        let rate = FrameRate::FPS_29_97;
        let at = |f: i64| rate.frame_start(f);
        let captions = vec![
            Caption {
                time: at(90),
                channel: 0,
                markup: pop_on("Olá ♪ «Ação»"),
            },
            Caption {
                time: at(300),
                channel: 1,
                markup: "{RCL}{ENM}{R15In00Wh}Hey{EDM}{EOC}".into(),
            },
            Caption {
                time: at(400),
                channel: 0,
                markup: "{EDM}".into(),
            },
        ];
        let scc = write_scc(&captions, Tick::ZERO);
        assert!(scc.starts_with("Scenarist_SCC V1.0"));
        let back = read("scc", &scc, rate);
        assert_eq!(back.len(), 3, "{scc}");
        for (a, b) in captions.iter().zip(&back) {
            assert_eq!(
                (a.time, a.channel, &a.markup),
                (b.time, b.channel, &b.markup)
            );
        }
        // From the file's start: a second later, a second earlier.
        let later = read("scc", &write_scc(&captions[2..], at(30)), rate);
        assert_eq!(later[0].time, at(370));
    }

    /// MacCaption: each frame's caption data packet (its letters for runs
    /// expanded), field 1's CEA-608 pairs read as Scenarist's, a caption
    /// at its End of Caption.
    #[test]
    fn maccaption_packets_become_markup() {
        let rate = FrameRate::FPS_29_97;
        let markup = "{RCL}{ENM}{R15In00Wh}Hi{EDM}{EOC}";
        let (words, eoc) = encode(markup, 0);
        let mut mcc = String::from("File Format=MacCaption_MCC V1.0\n\nTime Code Rate=30DF\n\n");
        let line =
            |frame: i64, data: String| format!("{}\t{data}\n", timecode::format_frame(frame, rate));
        // Each line: T (61 01) and the data's count, S (96 69) and the
        // packet's length, rate (4F), flags (43) and counter, then its
        // caption data (72, two triplets: field 1's pair, field 2's).
        // A frame of nothing, the caption a pair a frame, nothing again.
        let empty = "T16S164F430000".to_string() + "72E2FA0000FA0000";
        mcc.push_str(&line(29, empty.clone()));
        for (k, w) in words.iter().enumerate() {
            let packet = format!("T16S164F43{:04X}72E2FC{:04X}FD8080", k + 1, w);
            mcc.push_str(&line(30 + k as i64, packet));
        }
        mcc.push_str(&line(30 + words.len() as i64, empty));
        let c = read("mcc", &mcc, rate);
        assert_eq!(c.len(), 1, "{c:?}");
        assert_eq!(c[0].markup, markup);
        assert_eq!(c[0].channel, 0);
        assert_eq!(c[0].time, rate.frame_start(30 + eoc.unwrap() as i64));
        assert_eq!(mcc_bytes("TZG"), [0x61, 0x01, 0x00, 0xFA, 0x00, 0x00]);
    }

    #[test]
    fn scenarist_bytes_become_markup() {
        // RCL twice, ENM twice, row 15 indent 0, "Hi", EDM, EOC (the
        // eighth word: 7 frames after the line's time).
        let scc = "Scenarist_SCC V1.0\n\n\
                   00:00:01:00\t9420 9420 94ae 94ae 9470 c8e9 942c 942f\n\n\
                   00:00:03:00\t942c 942c\n";
        let c = read("scc", scc, FrameRate::FPS_29_97);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].markup, "{RCL}{ENM}{R15In00Wh}Hi{EDM}{EOC}");
        let start = scc_time("00:00:01:00").unwrap();
        let frame = FrameRate::FPS_29_97.frame_duration();
        assert_eq!(c[0].time, start + Tick(frame.0 * 7));
        assert_eq!(rows_text(&c[0].markup), ["Hi"]);
        assert_eq!(c[1].markup, "{EDM}");
        // A special character, an extended one in place of its stand-in,
        // a mid-row color, a roll-up row.
        let scc = "00:00:05:00\t9425 9425 94ad 94ad 9470 c180 1137 4180 1220 9120 c1\n";
        let c = read("scc", scc, FrameRate::FPS_29_97);
        assert_eq!(c[0].markup, "{RU2}{CR}{R15In00Wh}A♪Á{WhTxt}A");
        // Drop-frame timecode: a minute in, two frame numbers skipped.
        assert_eq!(
            scc_time("00:01:00;02").unwrap(),
            FrameRate::FPS_29_97.frame_start(1800)
        );
    }
}
