//! Closed captions (CEA-608), as VEGAS's Help describes them ("Adding
//! closed captioning to video files"): command markers `608CC1` to
//! `608CC4` (CC1 the primary language, CC3 the secondary) whose Comment
//! holds the caption's markup — `{RCL}` loads a pop-on caption, `{EOC}`
//! shows it, `{EDM}` clears the screen, `{RU2}`…`{RU4}` roll up, `{RDC}`
//! paints on, `{R14In00Wh}` places a row (row, indent, color),
//! `{TabOff1}`… nudge it, `{GrTxt}`… change the color mid-row — and whose
//! Parameter shows its text (`[BR]` between rows). Plain text typed as a
//! caption is made a pop-on caption ([`pop_on`]). What each channel shows
//! at a time is worked out as a CEA-608 decoder does, with a displayed
//! memory and one loading off screen ([`CaptionDecoder`]).

use std::collections::BTreeMap;

use bv_core::Tick;

use crate::commands::CommandMarker;
use crate::model::Project;

/// The commands of the four channels, CC1 to CC4.
pub const CHANNELS: [&str; 4] = ["608CC1", "608CC2", "608CC3", "608CC4"];

/// A caption's screen: 15 rows of 32 columns.
pub const ROWS: u8 = 15;
pub const COLUMNS: u8 = 32;

/// A caption character's color.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CaptionColor {
    #[default]
    White,
    Green,
    Blue,
    Cyan,
    Red,
    Yellow,
    Magenta,
    Black,
}

impl CaptionColor {
    /// The color of its markup's letters ("Wh", "Gr"…).
    fn from_code(code: &str) -> Option<CaptionColor> {
        Some(match code {
            "Wh" => CaptionColor::White,
            "Gr" => CaptionColor::Green,
            "Bl" => CaptionColor::Blue,
            "Cy" => CaptionColor::Cyan,
            "Rd" => CaptionColor::Red,
            "Yl" => CaptionColor::Yellow,
            "Ma" => CaptionColor::Magenta,
            "Blk" => CaptionColor::Black,
            _ => return None,
        })
    }

    /// Its markup's letters.
    pub fn code(self) -> &'static str {
        match self {
            CaptionColor::White => "Wh",
            CaptionColor::Green => "Gr",
            CaptionColor::Blue => "Bl",
            CaptionColor::Cyan => "Cy",
            CaptionColor::Red => "Rd",
            CaptionColor::Yellow => "Yl",
            CaptionColor::Magenta => "Ma",
            CaptionColor::Black => "Blk",
        }
    }

    /// Its color, 8-bit RGB.
    pub fn rgb(self) -> [u8; 3] {
        match self {
            CaptionColor::White => [255, 255, 255],
            CaptionColor::Green => [0, 255, 0],
            CaptionColor::Blue => [0, 0, 255],
            CaptionColor::Cyan => [0, 255, 255],
            CaptionColor::Red => [255, 0, 0],
            CaptionColor::Yellow => [255, 255, 0],
            CaptionColor::Magenta => [255, 0, 255],
            CaptionColor::Black => [0, 0, 0],
        }
    }
}

/// How a run of characters looks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CaptionStyle {
    pub color: CaptionColor,
    pub underline: bool,
    pub italic: bool,
    /// Its background's color and whether it is half see-through (`None`:
    /// CEA-608's own, opaque black).
    pub background: Option<(CaptionColor, bool)>,
    /// `{BgTran}`, and the blanks before a placed row: no background.
    pub transparent: bool,
}

/// A run of characters of one style on a row, from column `column`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CaptionRun {
    pub column: u8,
    pub text: String,
    pub style: CaptionStyle,
}

/// A row of the screen (1–15) and its runs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CaptionRow {
    pub row: u8,
    pub runs: Vec<CaptionRun>,
}

impl CaptionRow {
    /// Its text: the runs one after another, the blanks between them kept.
    pub fn text(&self) -> String {
        let mut out = String::new();
        let mut at = self.runs.first().map_or(0, |r| r.column) as usize;
        for r in &self.runs {
            while at < r.column as usize {
                out.push(' ');
                at += 1;
            }
            out.push_str(&r.text);
            at += r.text.chars().count();
        }
        out.trim_end().to_string()
    }
}

/// What a channel shows: its rows, from the top.
pub type CaptionScreen = Vec<CaptionRow>;

/// A piece of markup: a `{…}` code or text.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Code(String),
    Text(String),
}

fn tokens(markup: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut rest = markup;
    while !rest.is_empty() {
        match rest.find('{') {
            Some(0) => match rest.find('}') {
                Some(end) => {
                    out.push(Token::Code(rest[1..end].to_string()));
                    rest = &rest[end + 1..];
                }
                None => {
                    out.push(Token::Text(rest.to_string()));
                    rest = "";
                }
            },
            Some(i) => {
                out.push(Token::Text(rest[..i].to_string()));
                rest = &rest[i..];
            }
            None => {
                out.push(Token::Text(rest.to_string()));
                rest = "";
            }
        }
    }
    out
}

/// Whether `text` has caption markup (else it is plain text).
pub fn has_markup(text: &str) -> bool {
    tokens(text).iter().any(|t| match t {
        Token::Code(c) => is_code(c.trim()),
        Token::Text(_) => false,
    })
}

/// A code the decoder knows.
fn is_code(code: &str) -> bool {
    matches!(
        code,
        "RCL"
            | "RDC"
            | "RU2"
            | "RU3"
            | "RU4"
            | "EDM"
            | "ENM"
            | "EOC"
            | "CR"
            | "Backspace"
            | "DelEndRow"
            | "FlashOn"
            | "TabOff1"
            | "TabOff2"
            | "TabOff3"
            | "BgTran"
            | "TH"
    ) || code.ends_with("CharSet")
        || code.ends_with("Charset")
        || placement(code).is_some()
        || mid_row(code).is_some()
        || background(code).is_some()
}

/// A row placement `{R14In04WhU}` (VEGAS's "R yy In xx Wh"; spaces
/// allowed): its row, indent, color, underline and italics.
fn placement(code: &str) -> Option<(u8, u8, CaptionColor, bool, bool)> {
    let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    let rest = code.strip_prefix('R')?;
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let row: u8 = rest[..digits].parse().ok()?;
    let mut rest = &rest[digits..];
    let mut indent = 0;
    if let Some(r) = rest.strip_prefix("In") {
        let d = r.chars().take_while(char::is_ascii_digit).count();
        indent = r[..d].parse().ok()?;
        rest = &r[d..];
    }
    let underline = rest.ends_with('U');
    let rest = rest.strip_suffix('U').unwrap_or(rest);
    let (italic, rest) = match rest.strip_suffix('I') {
        Some("Wh") => (true, "Wh"),
        _ => (false, rest),
    };
    let color = CaptionColor::from_code(rest)?;
    Some((
        row.clamp(1, ROWS),
        indent.min(COLUMNS - 1),
        color,
        underline,
        italic,
    ))
}

/// A mid-row code `{GrUTxt}`, `{ITxt}`: its color (`None`: italics, the
/// color kept) and underline.
fn mid_row(code: &str) -> Option<(Option<CaptionColor>, bool)> {
    let c = code.strip_suffix("Txt")?;
    let underline = c.ends_with('U');
    let c = c.strip_suffix('U').unwrap_or(c);
    if c == "I" {
        return Some((None, underline));
    }
    Some((Some(CaptionColor::from_code(c)?), underline))
}

/// A background code `{BgWhSemi}`: its color and whether half see-through.
fn background(code: &str) -> Option<(CaptionColor, bool)> {
    let b = code.strip_prefix("Bg")?;
    let semi = b.ends_with("Semi");
    let b = b.strip_suffix("Semi").unwrap_or(b);
    Some((CaptionColor::from_code(b)?, semi))
}

/// How a channel writes: pop-on loads off screen, paint-on writes on it,
/// roll-up writes its base row and rolls the others up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    PopOn,
    PaintOn,
    RollUp(u8),
}

/// A memory of the screen: its rows' characters and their styles.
#[derive(Clone, Debug, Default, PartialEq)]
struct Memory {
    rows: BTreeMap<u8, Vec<(char, CaptionStyle)>>,
}

impl Memory {
    fn put(&mut self, row: u8, column: u8, c: char, style: CaptionStyle) {
        let line = self.rows.entry(row).or_default();
        let col = column as usize;
        let blank = CaptionStyle {
            transparent: true,
            ..style
        };
        while line.len() < col {
            line.push((' ', blank));
        }
        if col < line.len() {
            line[col] = (c, style);
        } else {
            line.push((c, style));
        }
    }

    fn screen(&self) -> CaptionScreen {
        self.rows
            .iter()
            .filter(|(_, l)| l.iter().any(|(c, _)| *c != ' '))
            .map(|(&row, line)| {
                let mut runs: Vec<CaptionRun> = Vec::new();
                // The blanks before the first character are the row's
                // indent, not text.
                let first = line.iter().position(|(c, _)| *c != ' ').unwrap_or(0);
                let end = line
                    .iter()
                    .rposition(|(c, _)| *c != ' ')
                    .map_or(0, |i| i + 1);
                for (k, &(c, style)) in line.iter().enumerate().take(end).skip(first) {
                    match runs.last_mut() {
                        Some(r) if r.style == style => r.text.push(c),
                        _ => runs.push(CaptionRun {
                            column: k as u8,
                            text: c.to_string(),
                            style,
                        }),
                    }
                }
                CaptionRow { row, runs }
            })
            .collect()
    }
}

/// A CEA-608 decoder of VEGAS's markup: what a channel shows.
#[derive(Clone, Debug, Default)]
pub struct CaptionDecoder {
    mode: Mode,
    displayed: Memory,
    loading: Memory,
    /// Where the next character goes (row 0: none placed yet, the bottom
    /// row).
    row: u8,
    column: u8,
    style: CaptionStyle,
}

impl CaptionDecoder {
    /// The memory being written.
    fn target(&mut self) -> &mut Memory {
        match self.mode {
            Mode::PopOn => &mut self.loading,
            _ => &mut self.displayed,
        }
    }

    /// One command marker's markup.
    pub fn apply(&mut self, markup: &str) {
        for t in tokens(markup) {
            match t {
                Token::Text(text) => {
                    for c in text.chars().filter(|c| !c.is_control()) {
                        if self.row == 0 {
                            self.row = ROWS;
                        }
                        let (row, column, style) = (self.row, self.column, self.style);
                        if column < COLUMNS {
                            self.target().put(row, column, c, style);
                            self.column += 1;
                        }
                    }
                }
                Token::Code(code) => self.code(code.trim()),
            }
        }
    }

    fn code(&mut self, code: &str) {
        match code {
            "RCL" => self.mode = Mode::PopOn,
            "RDC" => self.mode = Mode::PaintOn,
            "RU2" | "RU3" | "RU4" => {
                let n = code.as_bytes()[2] - b'0';
                if !matches!(self.mode, Mode::RollUp(_)) {
                    // Into roll-up: the screen is cleared.
                    self.displayed = Memory::default();
                    self.loading = Memory::default();
                }
                self.mode = Mode::RollUp(n);
                self.row = ROWS;
                self.column = 0;
            }
            "EDM" => self.displayed = Memory::default(),
            "ENM" => self.loading = Memory::default(),
            "EOC" => {
                std::mem::swap(&mut self.displayed, &mut self.loading);
                self.mode = Mode::PopOn;
            }
            "CR" => {
                if let Mode::RollUp(n) = self.mode {
                    // The window's rows roll up one; the top one goes.
                    let base = self.row.max(n);
                    let old = std::mem::take(&mut self.displayed.rows);
                    for (r, line) in old {
                        if r + n > base + 1 && r <= base {
                            self.displayed.rows.insert(r - 1, line);
                        }
                    }
                    self.row = base;
                    self.column = 0;
                }
            }
            "Backspace" => {
                if self.column > 0 {
                    self.column -= 1;
                    let (row, column) = (self.row, self.column);
                    if let Some(line) = self.target().rows.get_mut(&row) {
                        line.truncate(column as usize);
                    }
                }
            }
            "DelEndRow" => {
                let (row, column) = (self.row, self.column);
                if let Some(line) = self.target().rows.get_mut(&row) {
                    line.truncate(column as usize);
                }
            }
            "TabOff1" | "TabOff2" | "TabOff3" => {
                let n = code.as_bytes()[6] - b'0';
                self.column = (self.column + n).min(COLUMNS - 1);
            }
            "BgTran" => {
                self.style.transparent = true;
                self.style.background = None;
            }
            _ => {
                if let Some((row, indent, color, underline, italic)) = placement(code) {
                    if let Mode::RollUp(n) = self.mode
                        && self.row != 0
                        && row != self.row
                    {
                        // A roll-up window moved to another base row.
                        let shift = row as i16 - self.row as i16;
                        let old = std::mem::take(&mut self.displayed.rows);
                        for (r, line) in old {
                            let to = r as i16 + shift;
                            if (1..=ROWS as i16).contains(&to) && to > row as i16 - n as i16 {
                                self.displayed.rows.insert(to as u8, line);
                            }
                        }
                    }
                    self.row = row;
                    self.column = indent;
                    self.style.color = color;
                    self.style.underline = underline;
                    self.style.italic = italic;
                    return;
                }
                if let Some((color, underline)) = mid_row(code) {
                    match color {
                        Some(c) => {
                            self.style.color = c;
                            self.style.italic = false;
                        }
                        None => self.style.italic = true,
                    }
                    self.style.underline = underline;
                    return;
                }
                if let Some(bg) = background(code) {
                    self.style.background = Some(bg);
                    self.style.transparent = false;
                }
                // {FlashOn}, {TH} and the character sets: not drawn.
            }
        }
    }

    /// What it shows now.
    pub fn screen(&self) -> CaptionScreen {
        self.displayed.screen()
    }

    /// What it would show once its loading caption is shown: a caption
    /// marker's own text, whatever came before it.
    fn written(&self) -> CaptionScreen {
        match self.mode {
            Mode::PopOn if !self.loading.rows.is_empty() => self.loading.screen(),
            _ => self.displayed.screen(),
        }
    }
}

/// The lines of `text` (parted by `[BR]` or line breaks) as rows of at
/// most `width` characters: long lines wrap between words.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.replace("[BR]", "\n").replace("[br]", "\n").lines() {
        let mut row = String::new();
        for word in line.split_whitespace() {
            let mut word: Vec<char> = word.chars().collect();
            while !word.is_empty() {
                let used = row.chars().count();
                let gap = usize::from(used > 0);
                if used + gap + word.len() <= width {
                    if gap == 1 {
                        row.push(' ');
                    }
                    row.extend(word.drain(..));
                } else if used > 0 {
                    out.push(std::mem::take(&mut row));
                } else {
                    // Longer than a row: cut.
                    out.push(word.drain(..width).collect());
                }
            }
        }
        if !row.is_empty() {
            out.push(row);
        }
    }
    out
}

/// `text` (rows parted by `[BR]` or line breaks) as a pop-on caption, as
/// VEGAS's Command Properties make plain text typed as a caption: its rows
/// at the bottom of the screen, each after a tab offset, so 31 characters
/// at most (longer lines wrap); nothing, a caption that clears the screen.
pub fn pop_on(text: &str) -> String {
    let rows = wrap(text, COLUMNS as usize - 1);
    if rows.is_empty() {
        return "{EDM}".to_string();
    }
    let rows = &rows[rows.len().saturating_sub(ROWS as usize)..];
    let first = ROWS + 1 - rows.len() as u8;
    let mut out = String::from("{RCL}{ENM}");
    for (k, line) in rows.iter().enumerate() {
        out.push_str(&format!(
            "{{R{:02}In00Wh}}{{TabOff1}}{line}",
            first + k as u8
        ));
    }
    out.push_str("{EDM}{EOC}");
    out
}

/// The text a caption's markup writes, rows parted by `[BR]`: what its
/// marker's Parameter shows.
pub fn shown_text(markup: &str) -> String {
    rows_text(markup).join("[BR]")
}

/// The rows of text a caption's markup writes.
pub fn rows_text(markup: &str) -> Vec<String> {
    let mut d = CaptionDecoder::default();
    d.apply(markup);
    d.written().iter().map(CaptionRow::text).collect()
}

/// The channel (0–3) of a command, when it is a caption's.
pub fn channel_of(command: &str) -> Option<usize> {
    CHANNELS
        .iter()
        .position(|c| c.eq_ignore_ascii_case(command.trim()))
}

/// A caption marker's markup: its Comment's, or its Parameter's when only
/// that one has markup; plain text in either (the Comment first) is a
/// pop-on caption of it.
pub fn marker_markup(c: &CommandMarker) -> String {
    if has_markup(&c.comment) {
        c.comment.clone()
    } else if has_markup(&c.parameter) {
        c.parameter.clone()
    } else if !c.comment.trim().is_empty() {
        pop_on(&c.comment)
    } else {
        pop_on(&c.parameter)
    }
}

/// A caption of a file: when it comes (the file's time), its channel
/// (0–3) and its markup.
#[derive(Clone, Debug, PartialEq)]
pub struct Caption {
    pub time: Tick,
    pub channel: usize,
    pub markup: String,
}

impl Project {
    /// The caption markers of channel `channel` (0: CC1 … 3: CC4), in
    /// time order, with their markup.
    fn caption_markers(&self, channel: usize) -> Vec<(Tick, String)> {
        let mut v: Vec<(Tick, String)> = self
            .commands
            .iter()
            .filter(|c| channel_of(&c.command) == Some(channel))
            .map(|c| (c.time, marker_markup(c)))
            .collect();
        v.sort_by_key(|(t, _)| *t);
        v
    }

    /// Every caption marker (time on the timeline, channel 0–3, markup),
    /// in time order.
    pub fn captions(&self) -> Vec<Caption> {
        let mut v: Vec<Caption> = self
            .commands
            .iter()
            .filter_map(|c| {
                Some(Caption {
                    time: c.time,
                    channel: channel_of(&c.command)?,
                    markup: marker_markup(c),
                })
            })
            .collect();
        v.sort_by_key(|c| (c.time, c.channel));
        v
    }

    /// The channels (0–3) that have captions.
    pub fn caption_channels(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self
            .commands
            .iter()
            .filter_map(|c| channel_of(&c.command))
            .collect();
        v.sort();
        v.dedup();
        v
    }

    /// What caption channel `channel` (0: CC1 … 3: CC4) shows at `t`: its
    /// markers up to `t`, decoded in order.
    pub fn captions_at(&self, channel: usize, t: Tick) -> CaptionScreen {
        let mut d = CaptionDecoder::default();
        for (_, markup) in self
            .caption_markers(channel)
            .iter()
            .filter(|(time, _)| *time <= t)
        {
            d.apply(markup);
        }
        d.screen()
    }

    /// The captions channel `channel` shows, as subtitles: each one's text
    /// (rows parted by line breaks), from when it shows to when it changes
    /// or goes (the last one until `end`).
    pub fn caption_spans(&self, channel: usize, end: Tick) -> Vec<(Tick, Tick, String)> {
        let markers = self.caption_markers(channel);
        let mut d = CaptionDecoder::default();
        let mut shown: Vec<(Tick, String)> = Vec::new();
        for (k, (t, markup)) in markers.iter().enumerate() {
            d.apply(markup);
            // The screen once every marker of that time is decoded.
            if markers.get(k + 1).is_some_and(|(next, _)| next == t) {
                continue;
            }
            let text = d
                .screen()
                .iter()
                .map(CaptionRow::text)
                .collect::<Vec<_>>()
                .join("\n");
            if shown.last().is_none_or(|(_, last)| *last != text) {
                shown.push((*t, text));
            }
        }
        let mut out = Vec::new();
        for (k, (t, text)) in shown.iter().enumerate() {
            let until = shown.get(k + 1).map_or(end.max(*t), |(next, _)| *next);
            if !text.is_empty() && until > *t {
                out.push((*t, until, text.clone()));
            }
        }
        out
    }

    /// Adds captions read from a file as caption command markers: placed
    /// by the file's time less the ruler's offset (what VEGAS's Help says
    /// to set before importing to move them), those before the timeline's
    /// start at it; their Parameter shows their text. How many.
    pub fn add_captions(&mut self, captions: &[Caption]) -> usize {
        for c in captions {
            let time = (c.time - self.ruler_offset).max(Tick::ZERO);
            let command = CHANNELS[c.channel.min(3)];
            self.add_command(time, command, &shown_text(&c.markup), &c.markup);
        }
        captions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(s: &CaptionScreen) -> Vec<(u8, String)> {
        s.iter().map(|r| (r.row, r.text())).collect()
    }

    #[test]
    fn a_pop_on_caption_shows_at_its_end() {
        let mut d = CaptionDecoder::default();
        d.apply("{RCL}{ENM}{R14In00Wh}{TabOff1}First Row{R15In00Wh}{TabOff1}Second Row");
        assert!(d.screen().is_empty(), "loading, not shown yet");
        d.apply("{EDM}{EOC}");
        assert_eq!(
            texts(&d.screen()),
            [(14, "First Row".into()), (15, "Second Row".into())]
        );
        // Its runs start after the tab offset.
        assert_eq!(d.screen()[0].runs[0].column, 1);
        d.apply("{EDM}");
        assert!(d.screen().is_empty(), "cleared");
    }

    #[test]
    fn roll_up_captions_roll() {
        let mut d = CaptionDecoder::default();
        for line in ["One", "Two", "Three"] {
            d.apply(&format!("{{RU2}}{{CR}}{{R15In00Wh}}{{TabOff1}}{line}"));
        }
        assert_eq!(
            texts(&d.screen()),
            [(14, "Two".into()), (15, "Three".into())]
        );
        // Three rows: the window keeps three.
        d.apply("{RU3}{CR}{R15In00Wh}Four");
        assert_eq!(
            texts(&d.screen()),
            [
                (13, "Two".into()),
                (14, "Three".into()),
                (15, "Four".into())
            ]
        );
    }

    #[test]
    fn paint_on_writes_on_screen_with_colors() {
        let mut d = CaptionDecoder::default();
        d.apply("{RDC}{R14In04Wh}{TabOff3}Hi {GrTxt}there");
        let s = d.screen();
        assert_eq!(texts(&s), [(14, "Hi there".into())]);
        assert_eq!(s[0].runs[0].column, 7, "indent 4 and a tab offset of 3");
        assert_eq!(s[0].runs.len(), 2);
        assert_eq!(s[0].runs[1].style.color, CaptionColor::Green);
        d.apply("{Backspace}{Backspace}{DelEndRow}");
        assert_eq!(texts(&d.screen()), [(14, "Hi the".into())]);
        // Italics and underline by the row placement.
        d.apply("{EDM}{R10WhIU}Slanted");
        let s = d.screen();
        assert!(s[0].runs[0].style.italic && s[0].runs[0].style.underline);
    }

    #[test]
    fn plain_text_becomes_a_pop_on_caption() {
        let m = pop_on("Hello there[BR]Second line");
        assert_eq!(
            m,
            "{RCL}{ENM}{R14In00Wh}{TabOff1}Hello there{R15In00Wh}{TabOff1}Second line{EDM}{EOC}"
        );
        assert_eq!(shown_text(&m), "Hello there[BR]Second line");
        // Long lines wrap at 31 characters.
        let long = "The quick brown fox jumps over the lazy dog again and again";
        let rows = rows_text(&pop_on(long));
        assert!(rows.iter().all(|r| r.chars().count() <= 31), "{rows:?}");
        assert_eq!(rows.join(" "), long);
        assert!(has_markup(&m) && !has_markup("Just {braces}"));
    }

    #[test]
    fn markers_give_each_channel_its_captions() {
        let mut p = Project::default();
        let s = Tick::from_seconds;
        p.add_command(s(1), "608CC1", "", &pop_on("Hello"));
        p.add_command(s(3), "608CC1", "", "{EDM}");
        p.add_command(s(2), "608CC3", "", &pop_on("Hola"));
        // Plain text in the Comment: a pop-on caption of it.
        p.add_command(s(5), "608CC1", "", "Bye[BR]now");
        assert_eq!(p.caption_channels(), [0, 2]);
        assert!(p.captions_at(0, s(0)).is_empty());
        assert_eq!(texts(&p.captions_at(0, s(2))), [(15, "Hello".into())]);
        assert!(p.captions_at(0, s(4)).is_empty());
        assert_eq!(texts(&p.captions_at(2, s(4))), [(15, "Hola".into())]);
        assert_eq!(
            p.caption_spans(0, s(9)),
            [
                (s(1), s(3), "Hello".to_string()),
                (s(5), s(9), "Bye\nnow".to_string())
            ]
        );
    }

    #[test]
    fn captions_from_a_file_follow_the_ruler_offset() {
        let mut p = Project {
            ruler_offset: Tick::from_seconds(3600),
            ..Project::default()
        };
        let n = p.add_captions(&[
            Caption {
                time: Tick::from_seconds(3602),
                channel: 0,
                markup: pop_on("Hi"),
            },
            Caption {
                time: Tick::from_seconds(10),
                channel: 2,
                markup: pop_on("Early"),
            },
        ]);
        assert_eq!(n, 2);
        let c = &p.commands;
        assert_eq!((c[0].time, c[0].command.as_str()), (Tick::ZERO, "608CC3"));
        assert_eq!(c[1].time, Tick::from_seconds(2));
        assert_eq!(c[1].parameter, "Hi");
        assert!(has_markup(&c[1].comment));
    }
}
