//! A small XML reader: a document read whole into elements, enough for
//! the interchange formats (Final Cut Pro 7's XML, FCPXML) — elements,
//! attributes in either quote, text and CDATA, the five named entities and
//! numbered ones; the declaration, processing instructions, comments and a
//! DOCTYPE (without an internal subset) passed over.

/// An element: its name, attributes and what it holds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

/// What an element holds.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Element(Element),
    Text(String),
}

impl Element {
    /// Attribute `name`'s value.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// The child elements, in order.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            Node::Text(_) => None,
        })
    }

    /// The first child element named `name`.
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.elements().find(|e| e.name == name)
    }

    /// The child elements named `name`.
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.elements().filter(move |e| e.name == name)
    }

    /// Its text (all of it, its children's left out), its spaces at the
    /// ends trimmed.
    pub fn text(&self) -> String {
        let mut s = String::new();
        for n in &self.children {
            if let Node::Text(t) = n {
                s.push_str(t);
            }
        }
        s.trim().to_string()
    }

    /// The text of child `name`.
    pub fn child_text(&self, name: &str) -> Option<String> {
        self.child(name).map(Element::text)
    }

    /// The element at `path` of child names ("rate/timebase").
    pub fn find(&self, path: &str) -> Option<&Element> {
        path.split('/')
            .filter(|p| !p.is_empty())
            .try_fold(self, |e, name| e.child(name))
    }
}

/// Text with its entities in their characters' places.
fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';').filter(|&e| e <= 12) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..end];
        let c = match name {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => name
                .strip_prefix("#x")
                .or_else(|| name.strip_prefix("#X"))
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| name.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match c {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Reads a document: its root element.
pub fn parse(text: &str) -> Result<Element, String> {
    let mut p = Parser {
        s: text.trim_start_matches('\u{feff}'),
        at: 0,
    };
    p.skip_misc()?;
    let root = p.element()?;
    p.skip_misc()?;
    Ok(root)
}

struct Parser<'a> {
    s: &'a str,
    at: usize,
}

impl<'a> Parser<'a> {
    /// What is left to read (the text's, not the parser's: it outlives
    /// the parser's moves).
    fn rest(&self) -> &'a str {
        &self.s[self.at..]
    }

    fn error(&self, what: &str) -> String {
        let line = self.s[..self.at].matches('\n').count() + 1;
        format!("{what} (line {line})")
    }

    fn skip_spaces(&mut self) {
        let trimmed = self.rest().trim_start();
        self.at = self.s.len() - trimmed.len();
    }

    /// Passes over `<!--…-->`, `<?…?>` and `<!…>`; whether it did.
    fn skip_one_misc(&mut self) -> Result<bool, String> {
        let r = self.rest();
        let end = if r.starts_with("<!--") {
            r.find("-->").map(|e| e + 3)
        } else if r.starts_with("<?") {
            r.find("?>").map(|e| e + 2)
        } else if r.starts_with("<!") && !r.starts_with("<![CDATA[") {
            r.find('>').map(|e| e + 1)
        } else {
            return Ok(false);
        };
        let end = end.ok_or_else(|| self.error("unclosed markup"))?;
        self.at += end;
        Ok(true)
    }

    fn skip_misc(&mut self) -> Result<(), String> {
        loop {
            self.skip_spaces();
            if !self.skip_one_misc()? {
                return Ok(());
            }
        }
    }

    fn name(&mut self) -> Result<String, String> {
        let r = self.rest();
        let len = r
            .find(|c: char| c.is_whitespace() || matches!(c, '>' | '/' | '='))
            .unwrap_or(r.len());
        if len == 0 {
            return Err(self.error("a name was expected"));
        }
        let name = r[..len].to_string();
        self.at += len;
        Ok(name)
    }

    fn element(&mut self) -> Result<Element, String> {
        if !self.rest().starts_with('<') {
            return Err(self.error("an element was expected"));
        }
        self.at += 1;
        let mut e = Element {
            name: self.name()?,
            ..Element::default()
        };
        // Attributes, then the tag's end.
        loop {
            self.skip_spaces();
            let r = self.rest();
            if r.starts_with("/>") {
                self.at += 2;
                return Ok(e);
            }
            if r.starts_with('>') {
                self.at += 1;
                break;
            }
            if r.is_empty() {
                return Err(self.error("unclosed tag"));
            }
            let key = self.name()?;
            self.skip_spaces();
            if !self.rest().starts_with('=') {
                return Err(self.error("= was expected"));
            }
            self.at += 1;
            self.skip_spaces();
            let quote = self
                .rest()
                .chars()
                .next()
                .filter(|c| *c == '"' || *c == '\'')
                .ok_or_else(|| self.error("a quoted value was expected"))?;
            self.at += 1;
            let end = self
                .rest()
                .find(quote)
                .ok_or_else(|| self.error("unclosed value"))?;
            let value = unescape(&self.rest()[..end]);
            self.at += end + 1;
            e.attrs.push((key, value));
        }
        // What it holds, to its end tag.
        loop {
            let r = self.rest();
            if r.is_empty() {
                return Err(self.error(&format!("<{}> is not closed", e.name)));
            }
            if let Some(after) = r.strip_prefix("</") {
                let len = after
                    .find('>')
                    .ok_or_else(|| self.error("unclosed end tag"))?;
                if after[..len].trim() != e.name {
                    return Err(self.error(&format!(
                        "</{}> closes <{}>",
                        after[..len].trim(),
                        e.name
                    )));
                }
                self.at += 2 + len + 1;
                return Ok(e);
            }
            if let Some(after) = r.strip_prefix("<![CDATA[") {
                let len = after
                    .find("]]>")
                    .ok_or_else(|| self.error("unclosed CDATA"))?;
                e.children.push(Node::Text(after[..len].to_string()));
                self.at += 9 + len + 3;
                continue;
            }
            if self.skip_one_misc()? {
                continue;
            }
            if r.starts_with('<') {
                let child = self.element()?;
                e.children.push(Node::Element(child));
                continue;
            }
            let len = r.find('<').unwrap_or(r.len());
            let text = unescape(&r[..len]);
            if !text.trim().is_empty() {
                e.children.push(Node::Text(text));
            }
            self.at += len;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_reads_into_elements() {
        let doc = parse(
            "\u{feff}<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE xmeml>\n<!-- a comment -->\n\
             <xmeml version=\"5\"><sequence id='s1'><name>Cut &amp; paste &#233;t&#xE9;</name>\
             <rate><timebase>30</timebase><ntsc>TRUE</ntsc></rate><empty/>\
             <note><![CDATA[a < b]]></note></sequence></xmeml>",
        )
        .unwrap();
        assert_eq!(doc.name, "xmeml");
        assert_eq!(doc.attr("version"), Some("5"));
        let seq = doc.child("sequence").unwrap();
        assert_eq!(seq.attr("id"), Some("s1"));
        assert_eq!(seq.child_text("name").as_deref(), Some("Cut & paste été"));
        assert_eq!(
            doc.find("sequence/rate/timebase")
                .map(Element::text)
                .as_deref(),
            Some("30")
        );
        assert!(seq.child("empty").unwrap().children.is_empty());
        assert_eq!(seq.child_text("note").as_deref(), Some("a < b"));
    }

    #[test]
    fn broken_documents_say_where() {
        assert!(parse("<a><b></a>").unwrap_err().contains("closes"));
        assert!(parse("<a x=1/>").is_err());
        assert!(parse("<a>").unwrap_err().contains("not closed"));
    }
}
