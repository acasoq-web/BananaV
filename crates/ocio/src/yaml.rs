//! The part of YAML 1.2 OpenColorIO's configurations are written in: block
//! mappings and sequences by indentation, flow `[…]` and `{…}` collections
//! (over several lines too), `!<Type>` tags, plain, quoted and block (`|`,
//! `>`) scalars, comments. Scalars stay text: a configuration's reader
//! says what each is.

use std::fmt;

/// A value and its tag (`!<ColorSpace>` → `ColorSpace`).
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub tag: Option<String>,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Scalar(String),
    Seq(Vec<Node>),
    /// In the order written.
    Map(Vec<(String, Node)>),
}

/// Where a document could not be read: its line (from 1) and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for Error {}

type Result<T> = std::result::Result<T, Error>;

impl Node {
    fn new(value: Value) -> Node {
        Node { tag: None, value }
    }

    pub fn is_null(&self) -> bool {
        self.value == Value::Null
    }

    /// A scalar's text.
    pub fn str(&self) -> Option<&str> {
        match &self.value {
            Value::Scalar(s) => Some(s),
            _ => None,
        }
    }

    /// A scalar as a number.
    pub fn f64(&self) -> Option<f64> {
        let s = self.str()?.trim();
        match s {
            ".inf" | ".Inf" | "+.inf" => Some(f64::INFINITY),
            "-.inf" | "-.Inf" => Some(f64::NEG_INFINITY),
            ".nan" | ".NaN" => Some(f64::NAN),
            _ => s.parse().ok(),
        }
    }

    /// A scalar as a boolean (`true`/`false`, `yes`/`no`).
    pub fn bool(&self) -> Option<bool> {
        match self.str()?.trim() {
            "true" | "True" | "TRUE" | "yes" | "Yes" => Some(true),
            "false" | "False" | "FALSE" | "no" | "No" => Some(false),
            _ => None,
        }
    }

    pub fn seq(&self) -> Option<&[Node]> {
        match &self.value {
            Value::Seq(s) => Some(s),
            _ => None,
        }
    }

    pub fn map(&self) -> Option<&[(String, Node)]> {
        match &self.value {
            Value::Map(m) => Some(m),
            _ => None,
        }
    }

    /// A mapping's value for `key`.
    pub fn get(&self, key: &str) -> Option<&Node> {
        self.map()?.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// A sequence of numbers.
    pub fn f64s(&self) -> Option<Vec<f64>> {
        self.seq()?.iter().map(Node::f64).collect()
    }

    /// A sequence of texts (or one text: a list of one).
    pub fn strs(&self) -> Option<Vec<String>> {
        match &self.value {
            Value::Seq(s) => s.iter().map(|n| n.str().map(str::to_string)).collect(),
            Value::Scalar(s) => Some(vec![s.clone()]),
            Value::Null => Some(Vec::new()),
            Value::Map(_) => None,
        }
    }
}

/// Reads a document.
pub fn parse(text: &str) -> Result<Node> {
    let mut p = Parser {
        lines: lines(text)?,
        pos: 0,
    };
    let Some(first) = p.next_line() else {
        return Ok(Node::new(Value::Null));
    };
    let indent = first.indent;
    let node = p.block(indent)?;
    if let Some(l) = p.next_line() {
        return Err(Error {
            line: l.number,
            message: format!("unexpected text: {}", l.text),
        });
    }
    Ok(node)
}

fn is_dash(text: &str) -> bool {
    text == "-" || text.starts_with("- ")
}

/// A line's text without its comment and trailing space, and how far it
/// is indented (blank lines and comments have no text: only block
/// scalars read them).
#[derive(Clone, Debug)]
struct Line {
    number: usize,
    indent: usize,
    text: String,
    /// The line as written (block scalars keep their own `#`s).
    raw: String,
}

fn lines(text: &str) -> Result<Vec<Line>> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        let body = strip_comment(raw);
        let trimmed = body.trim_end();
        let indent = trimmed.len() - trimmed.trim_start().len();
        let text = trimmed.trim_start();
        if raw.contains('\t') && raw[..raw.len() - raw.trim_start().len()].contains('\t') {
            return Err(Error {
                line: i + 1,
                message: "tabs indent it".into(),
            });
        }
        if text == "---" || text == "..." || text.starts_with('%') {
            continue;
        }
        out.push(Line {
            number: i + 1,
            indent,
            text: text.to_string(),
            raw: raw.to_string(),
        });
    }
    Ok(out)
}

/// The line without a `#` comment (one outside quotes, after a space or
/// at its start).
fn strip_comment(s: &str) -> &str {
    let (mut single, mut double) = (false, false);
    let mut prev = ' ';
    for (i, c) in s.char_indices() {
        match c {
            '\'' if !double => single = !single,
            '"' if !single && prev != '\\' => double = !double,
            '#' if !single && !double && prev.is_whitespace() => return &s[..i],
            _ => {}
        }
        prev = c;
    }
    s
}

struct Parser {
    lines: Vec<Line>,
    pos: usize,
}

impl Parser {
    fn err<T>(&self, message: impl Into<String>) -> Result<T> {
        let line = self
            .lines
            .get(self.pos)
            .or_else(|| self.lines.last())
            .map_or(0, |l| l.number);
        Err(Error {
            line,
            message: message.into(),
        })
    }

    /// The next line with text, blank ones and comments passed over.
    fn next_line(&mut self) -> Option<&Line> {
        while self.lines.get(self.pos).is_some_and(|l| l.text.is_empty()) {
            self.pos += 1;
        }
        self.lines.get(self.pos)
    }

    /// The block (mapping, sequence or scalar) whose lines start at
    /// `indent`.
    fn block(&mut self, indent: usize) -> Result<Node> {
        let Some(line) = self.next_line() else {
            return Ok(Node::new(Value::Null));
        };
        if line.indent != indent {
            return self.err("unexpected indentation");
        }
        let text = line.text.clone();
        if is_dash(&text) {
            self.sequence(indent)
        } else if key_split(&text).is_some() {
            self.mapping(indent)
        } else {
            // A lone value (a tagged block on its own line, or a scalar).
            self.pos += 1;
            self.inline(&text, indent)
        }
    }

    fn sequence(&mut self, indent: usize) -> Result<Node> {
        let mut items = Vec::new();
        while let Some(line) = self.next_line() {
            if line.indent != indent || !is_dash(&line.text) {
                break;
            }
            let rest = line.text[1..].trim_start().to_string();
            // Where the item's own text starts (a mapping begun on its line
            // goes on at that column).
            let column = indent + (line.text.len() - rest.len());
            self.pos += 1;
            items.push(self.item(&rest, indent, column)?);
        }
        Ok(Node::new(Value::Seq(items)))
    }

    /// A sequence item written `rest` after its dash at `indent`, its text
    /// at `column`.
    fn item(&mut self, rest: &str, indent: usize, column: usize) -> Result<Node> {
        let (tag, rest) = split_tag(rest);
        let rest = rest.trim_start();
        let mut node = if rest.is_empty() {
            self.nested(indent)?
        } else if rest.starts_with('[') || rest.starts_with('{') {
            self.flow_from(rest)?
        } else if let Some((key, value)) = key_split(rest) {
            // A mapping whose first entry is on the dash's line.
            let first_value = self.value_after_key(value, column)?;
            let mut entries = vec![(key, first_value)];
            let more_here = self.next_line().is_some_and(|next| {
                next.indent == column && key_split(&next.text).is_some() && !is_dash(&next.text)
            });
            if more_here && let Value::Map(more) = self.mapping(column)?.value {
                entries.extend(more);
            }
            Node::new(Value::Map(entries))
        } else {
            Node::new(Value::Scalar(scalar(rest)))
        };
        if let Some(tag) = tag {
            node.tag = Some(tag);
        }
        Ok(node)
    }

    fn mapping(&mut self, indent: usize) -> Result<Node> {
        let mut entries: Vec<(String, Node)> = Vec::new();
        while let Some(line) = self.next_line() {
            if line.indent != indent || is_dash(&line.text) {
                break;
            }
            let text = line.text.clone();
            let Some((key, value)) = key_split(&text) else {
                return self.err(format!("expected a key: {text}"));
            };
            self.pos += 1;
            let node = self.value_after_key(value, indent)?;
            entries.push((key, node));
        }
        Ok(Node::new(Value::Map(entries)))
    }

    /// A mapping value written `text` after its key, the key at `indent`.
    fn value_after_key(&mut self, text: &str, indent: usize) -> Result<Node> {
        let (tag, rest) = split_tag(text);
        let rest = rest.trim();
        let mut node = if rest.is_empty() {
            // On the following lines: deeper, or a sequence at the key's
            // own indentation.
            let sequence_here = self
                .next_line()
                .is_some_and(|next| next.indent == indent && is_dash(&next.text));
            if sequence_here {
                self.sequence(indent)?
            } else {
                self.nested(indent)?
            }
        } else if rest.starts_with('|') || rest.starts_with('>') {
            self.block_scalar(rest, indent)?
        } else {
            let mut node = self.inline(rest, indent)?;
            // A plain scalar may go on over deeper lines (blank ones its
            // line breaks).
            if let Value::Scalar(s) = &mut node.value
                && !rest.starts_with('"')
                && !rest.starts_with('\'')
            {
                loop {
                    let mut k = self.pos;
                    while self.lines.get(k).is_some_and(|l| l.text.is_empty()) {
                        k += 1;
                    }
                    let Some(next) = self.lines.get(k) else {
                        break;
                    };
                    if next.indent <= indent
                        || is_dash(&next.text)
                        || key_split(&next.text).is_some()
                    {
                        break;
                    }
                    if k == self.pos {
                        s.push(' ');
                    } else {
                        s.extend(std::iter::repeat_n('\n', k - self.pos));
                    }
                    s.push_str(&next.text);
                    self.pos = k + 1;
                }
            }
            node
        };
        if let Some(tag) = tag {
            node.tag = Some(tag);
        }
        Ok(node)
    }

    /// The block on the lines deeper than `indent` (null when there is
    /// none).
    fn nested(&mut self, indent: usize) -> Result<Node> {
        match self.next_line().map(|next| next.indent) {
            Some(i) if i > indent => self.block(i),
            _ => Ok(Node::new(Value::Null)),
        }
    }

    /// A value on one line (or a flow collection going on over the next).
    fn inline(&mut self, text: &str, indent: usize) -> Result<Node> {
        let (tag, rest) = split_tag(text);
        let rest = rest.trim();
        let mut node = if rest.is_empty() {
            self.nested(indent)?
        } else if rest.starts_with('[') || rest.starts_with('{') {
            self.flow_from(rest)?
        } else {
            Node::new(Value::Scalar(scalar(rest)))
        };
        if let Some(tag) = tag {
            node.tag = Some(tag);
        }
        Ok(node)
    }

    /// A flow collection starting with `first`, taking the following lines
    /// until its brackets close.
    fn flow_from(&mut self, first: &str) -> Result<Node> {
        let mut text = first.to_string();
        while !balanced(&text) {
            let Some(next) = self.lines.get(self.pos) else {
                return self.err("a bracket is never closed");
            };
            text.push(' ');
            text.push_str(&next.text);
            self.pos += 1;
        }
        let mut f = Flow {
            s: text.as_bytes(),
            i: 0,
        };
        let node = f.value().map_err(|message| Error {
            line: self
                .lines
                .get(self.pos.saturating_sub(1))
                .map_or(0, |l| l.number),
            message,
        })?;
        f.space();
        if f.i < f.s.len() {
            return self.err(format!("text after a collection: {}", &text[f.i..]));
        }
        Ok(node)
    }

    /// A `|` (lines kept) or `>` (folded) scalar on the lines deeper than
    /// `indent`.
    fn block_scalar(&mut self, header: &str, indent: usize) -> Result<Node> {
        let literal = header.starts_with('|');
        let keep = header.contains('+');
        let strip = header.contains('-');
        let mut body: Vec<String> = Vec::new();
        let mut base: Option<usize> = None;
        while let Some(line) = self.lines.get(self.pos) {
            // Its own lines as written: a `#` in them is text.
            let raw = line.raw.trim_end();
            let raw_indent = raw.len() - raw.trim_start().len();
            if raw.is_empty() {
                body.push(String::new());
                self.pos += 1;
                continue;
            }
            if raw_indent <= indent {
                break;
            }
            let b = *base.get_or_insert(raw_indent);
            body.push(raw.get(b.min(raw_indent)..).unwrap_or("").to_string());
            self.pos += 1;
        }
        // Blank lines at its end belong to it only when kept (`+`).
        if !keep {
            while body.last().is_some_and(String::is_empty) {
                body.pop();
            }
        }
        let mut text = if literal {
            body.join("\n")
        } else {
            let mut out = String::new();
            for (k, l) in body.iter().enumerate() {
                if k > 0 {
                    out.push(if l.is_empty() || body[k - 1].is_empty() {
                        '\n'
                    } else {
                        ' '
                    });
                }
                out.push_str(l);
            }
            out
        };
        if !strip && (keep || !text.is_empty()) {
            text.push('\n');
        }
        Ok(Node::new(Value::Scalar(text)))
    }
}

/// `key: value` split at its colon (outside quotes and brackets): the key
/// (unquoted) and what follows.
fn key_split(text: &str) -> Option<(String, &str)> {
    let bytes = text.as_bytes();
    let (mut single, mut double, mut depth) = (false, false, 0i32);
    for i in 0..bytes.len() {
        match bytes[i] {
            b'\'' if !double => single = !single,
            b'"' if !single => double = !double,
            b'[' | b'{' if !single && !double => depth += 1,
            b']' | b'}' if !single && !double => depth -= 1,
            b':' if !single && !double && depth == 0 => {
                let next = bytes.get(i + 1);
                if next.is_none() || next == Some(&b' ') {
                    let key = text[..i].trim();
                    if key.is_empty() || key.starts_with('!') {
                        return None;
                    }
                    return Some((scalar(key), text[i + 1..].trim_start()));
                }
            }
            _ => {}
        }
    }
    None
}

/// A leading `!<Tag>` (or `!Tag`) and the rest.
fn split_tag(text: &str) -> (Option<String>, &str) {
    let t = text.trim_start();
    if let Some(rest) = t.strip_prefix("!<")
        && let Some(end) = rest.find('>')
    {
        return (Some(rest[..end].to_string()), &rest[end + 1..]);
    }
    if let Some(rest) = t.strip_prefix('!') {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        return (Some(rest[..end].to_string()), &rest[end..]);
    }
    (None, t)
}

/// A scalar's text: quotes taken away and their escapes read.
fn scalar(text: &str) -> String {
    let t = text.trim();
    if t.len() >= 2 && t.starts_with('\'') && t.ends_with('\'') {
        return t[1..t.len() - 1].replace("''", "'");
    }
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        return unescape(&t[1..t.len() - 1]);
    }
    t.to_string()
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('/') => out.push('/'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                if let Some(c) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    out.push(c);
                }
            }
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Whether a flow collection's brackets (outside quotes) are all closed.
fn balanced(text: &str) -> bool {
    let (mut single, mut double, mut depth) = (false, false, 0i32);
    let mut prev = ' ';
    for c in text.chars() {
        match c {
            '\'' if !double => single = !single,
            '"' if !single && prev != '\\' => double = !double,
            '[' | '{' if !single && !double => depth += 1,
            ']' | '}' if !single && !double => depth -= 1,
            _ => {}
        }
        prev = c;
    }
    depth <= 0
}

/// A flow collection's reader.
struct Flow<'a> {
    s: &'a [u8],
    i: usize,
}

impl Flow<'_> {
    fn space(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn value(&mut self) -> std::result::Result<Node, String> {
        self.space();
        let mut tag = None;
        if self.peek() == Some(b'!') {
            let start = self.i;
            if self.s.get(self.i + 1) == Some(&b'<') {
                let end = self.s[self.i..]
                    .iter()
                    .position(|&b| b == b'>')
                    .ok_or("a tag is never closed")?;
                tag = Some(String::from_utf8_lossy(&self.s[start + 2..start + end]).into_owned());
                self.i += end + 1;
            } else {
                while self.i < self.s.len() && !self.s[self.i].is_ascii_whitespace() {
                    self.i += 1;
                }
                tag = Some(String::from_utf8_lossy(&self.s[start + 1..self.i]).into_owned());
            }
            self.space();
        }
        let mut node = match self.peek() {
            Some(b'[') => self.seq()?,
            Some(b'{') => self.map()?,
            Some(b'"') | Some(b'\'') => Node::new(Value::Scalar(self.quoted()?)),
            _ => {
                let s = self.plain(false);
                if s.is_empty() {
                    Node::new(Value::Null)
                } else {
                    Node::new(Value::Scalar(s))
                }
            }
        };
        if tag.is_some() {
            node.tag = tag;
        }
        Ok(node)
    }

    fn seq(&mut self) -> std::result::Result<Node, String> {
        self.i += 1;
        let mut items = Vec::new();
        loop {
            self.space();
            match self.peek() {
                Some(b']') => {
                    self.i += 1;
                    break;
                }
                None => return Err("a sequence is never closed".into()),
                _ => {}
            }
            items.push(self.value()?);
            self.space();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b']') => {}
                other => return Err(format!("expected , or ] in a sequence, found {other:?}")),
            }
        }
        Ok(Node::new(Value::Seq(items)))
    }

    fn map(&mut self) -> std::result::Result<Node, String> {
        self.i += 1;
        let mut entries = Vec::new();
        loop {
            self.space();
            match self.peek() {
                Some(b'}') => {
                    self.i += 1;
                    break;
                }
                None => return Err("a mapping is never closed".into()),
                _ => {}
            }
            let key = match self.peek() {
                Some(b'"') | Some(b'\'') => self.quoted()?,
                _ => self.plain(true),
            };
            self.space();
            let value = if self.peek() == Some(b':') {
                self.i += 1;
                self.value()?
            } else {
                Node::new(Value::Null)
            };
            entries.push((key, value));
            self.space();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b'}') => {}
                other => return Err(format!("expected , or }} in a mapping, found {other:?}")),
            }
        }
        Ok(Node::new(Value::Map(entries)))
    }

    fn quoted(&mut self) -> std::result::Result<String, String> {
        let q = self.s[self.i];
        let start = self.i;
        self.i += 1;
        while self.i < self.s.len() {
            let c = self.s[self.i];
            if q == b'"' && c == b'\\' {
                self.i += 2;
                continue;
            }
            if c == q {
                if q == b'\'' && self.s.get(self.i + 1) == Some(&b'\'') {
                    self.i += 2;
                    continue;
                }
                self.i += 1;
                let text = String::from_utf8_lossy(&self.s[start..self.i]).into_owned();
                return Ok(scalar(&text));
            }
            self.i += 1;
        }
        Err("a quote is never closed".into())
    }

    /// A plain scalar: up to `,`, `]`, `}` (and, a key, `: `).
    fn plain(&mut self, key: bool) -> String {
        let start = self.i;
        while self.i < self.s.len() {
            let c = self.s[self.i];
            if c == b',' || c == b']' || c == b'}' {
                break;
            }
            if key
                && c == b':'
                && self
                    .s
                    .get(self.i + 1)
                    .is_none_or(|n| n.is_ascii_whitespace() || *n == b',' || *n == b'}')
            {
                break;
            }
            if !key && c == b':' && self.s.get(self.i + 1).is_some_and(u8::is_ascii_whitespace) {
                // `a: b` inside a sequence: a one-entry mapping is not
                // something OpenColorIO writes; keep it text.
            }
            self.i += 1;
        }
        String::from_utf8_lossy(&self.s[start..self.i])
            .trim()
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_opencolorio_configurations_write() {
        let doc = r#"
ocio_profile_version: 2.1

environment:
  {}
search_path: "luts:other"
luma: [0.2126, 0.7152, 0.0722]
name: test # a comment
description: |
  Line one: with a colon.
  Line two # not a comment

roles:
  scene_linear: ACEScg
  color_timing: ACEScct

displays:
  sRGB - Display:
    - !<View> {name: Raw, colorspace: Raw}
    - !<Views> [ACES 1.0 - SDR Video, Un-tone-mapped]

colorspaces:
  - !<ColorSpace>
    name: ACEScct
    aliases: [ACES - ACEScct, acescct_ap1]
    isdata: false
    to_scene_reference: !<BuiltinTransform> {style: ACEScct_to_ACES2065-1}

  - !<ColorSpace>
    name: sRGB - Texture
    to_scene_reference: !<GroupTransform>
      children:
        - !<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055, direction: inverse}
        - !<MatrixTransform> {matrix: [0.439632981919491, 0.382988698151554, 0.177378319928955, 0,
            0.0897764429588424, 0.813439428748981, 0.0967841282921771, 0,
            0.0175411703831727, 0.111546553302387, 0.870912276314441, 0, 0, 0, 0, 1]}
"#;
        let n = parse(doc).unwrap();
        assert_eq!(n.get("ocio_profile_version").unwrap().f64(), Some(2.1));
        assert_eq!(n.get("environment").unwrap().map(), Some(&[][..]));
        assert_eq!(n.get("search_path").unwrap().str(), Some("luts:other"));
        assert_eq!(
            n.get("luma").unwrap().f64s(),
            Some(vec![0.2126, 0.7152, 0.0722])
        );
        assert_eq!(n.get("name").unwrap().str(), Some("test"));
        assert_eq!(
            n.get("description").unwrap().str(),
            Some("Line one: with a colon.\nLine two # not a comment\n")
        );
        assert_eq!(
            n.get("roles").unwrap().get("color_timing").unwrap().str(),
            Some("ACEScct")
        );
        let display = n.get("displays").unwrap().get("sRGB - Display").unwrap();
        let views = display.seq().unwrap();
        assert_eq!(views[0].tag.as_deref(), Some("View"));
        assert_eq!(views[0].get("colorspace").unwrap().str(), Some("Raw"));
        assert_eq!(views[1].tag.as_deref(), Some("Views"));
        assert_eq!(
            views[1].strs().unwrap(),
            ["ACES 1.0 - SDR Video", "Un-tone-mapped"]
        );
        let spaces = n.get("colorspaces").unwrap().seq().unwrap();
        assert_eq!(spaces.len(), 2);
        assert_eq!(spaces[0].tag.as_deref(), Some("ColorSpace"));
        assert_eq!(spaces[0].get("name").unwrap().str(), Some("ACEScct"));
        assert_eq!(
            spaces[0].get("aliases").unwrap().strs().unwrap(),
            ["ACES - ACEScct", "acescct_ap1"]
        );
        assert_eq!(spaces[0].get("isdata").unwrap().bool(), Some(false));
        let t = spaces[0].get("to_scene_reference").unwrap();
        assert_eq!(t.tag.as_deref(), Some("BuiltinTransform"));
        assert_eq!(t.get("style").unwrap().str(), Some("ACEScct_to_ACES2065-1"));
        let g = spaces[1].get("to_scene_reference").unwrap();
        assert_eq!(g.tag.as_deref(), Some("GroupTransform"));
        let children = g.get("children").unwrap().seq().unwrap();
        assert_eq!(children[0].get("direction").unwrap().str(), Some("inverse"));
        let m = children[1].get("matrix").unwrap().f64s().unwrap();
        assert_eq!(m.len(), 16);
        assert_eq!(m[15], 1.0);
    }

    #[test]
    fn version_one_configurations_too() {
        let doc = "
ocio_profile_version: 1
roles:
  default: raw
displays:
  sRGB:
    - !<View> {name: Film, colorspace: srgb8, looks: +grade}
colorspaces:
  - !<ColorSpace>
    name: lnf
    family: ln
    bitdepth: 32f
    allocation: lg2
    allocationvars: [-15, 6]
  - !<ColorSpace>
    name: srgb8
    from_reference: !<GroupTransform>
      children:
        - !<ColorSpaceTransform> {src: lnf, dst: lg10}
        - !<FileTransform> {src: 'spi_ocio_srgb_test.spi3d', interpolation: linear}
";
        let n = parse(doc).unwrap();
        let spaces = n.get("colorspaces").unwrap().seq().unwrap();
        assert_eq!(
            spaces[0].get("allocationvars").unwrap().f64s(),
            Some(vec![-15.0, 6.0])
        );
        let file = &spaces[1]
            .get("from_reference")
            .unwrap()
            .get("children")
            .unwrap()
            .seq()
            .unwrap()[1];
        assert_eq!(
            file.get("src").unwrap().str(),
            Some("spi_ocio_srgb_test.spi3d")
        );
        let view = &n
            .get("displays")
            .unwrap()
            .get("sRGB")
            .unwrap()
            .seq()
            .unwrap()[0];
        assert_eq!(view.get("looks").unwrap().str(), Some("+grade"));
    }

    #[test]
    fn mappings_begun_on_a_dash_and_plain_scalars_over_lines() {
        let doc = "
file_rules:
  - name: Default
    colorspace: default
  - !<Rule> {name: ColorSpaceNamePathSearch}
note: a long text
  that goes on
quoted: 'it''s'
escaped: \"a\\tb\"
";
        let n = parse(doc).unwrap();
        let rules = n.get("file_rules").unwrap().seq().unwrap();
        assert_eq!(rules[0].get("colorspace").unwrap().str(), Some("default"));
        assert_eq!(rules[1].tag.as_deref(), Some("Rule"));
        assert_eq!(
            n.get("note").unwrap().str(),
            Some("a long text that goes on")
        );
        assert_eq!(n.get("quoted").unwrap().str(), Some("it's"));
        assert_eq!(n.get("escaped").unwrap().str(), Some("a\tb"));
    }

    #[test]
    fn says_where_it_could_not_read() {
        let e = parse("a: [1, 2\n").unwrap_err();
        assert!(e.message.contains("never closed"), "{e}");
        let e = parse("a: 1\n  b: 2\nc\n").unwrap_err();
        assert!(e.line > 0);
    }
}
