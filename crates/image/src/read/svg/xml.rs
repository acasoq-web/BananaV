//! A small XML reader, enough for SVG documents (W3C's XML 1.0): elements,
//! their attributes and text. Comments, processing instructions and the
//! document type are skipped — the entities its internal subset declares
//! (`<!ENTITY ns_svg "…">`, which some drawing programs use in attribute
//! values) kept —, CDATA sections give their text, and the predefined
//! entities and character references are decoded. Damaged markup ends the
//! elements still open instead of failing.

use std::collections::HashMap;

/// An element: its local name (a prefix such as `svg:` left out), its
/// attributes as written and what it holds.
#[derive(Clone, Debug, Default)]
pub(super) struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Child>,
}

#[derive(Clone, Debug)]
pub(super) enum Child {
    Element(Element),
    Text(String),
}

impl Element {
    /// The attribute `name`'s value.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// A link: `href`, or XLink's `xlink:href` (any prefix bound to it).
    pub fn href(&self) -> Option<&str> {
        self.attr("href").or_else(|| {
            self.attrs
                .iter()
                .find(|(n, _)| n.ends_with(":href"))
                .map(|(_, v)| v.as_str())
        })
    }

    /// The elements it holds.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|c| match c {
            Child::Element(e) => Some(e),
            Child::Text(_) => None,
        })
    }

    /// Its text (a `<style>` sheet's).
    pub fn text(&self) -> String {
        let mut out = String::new();
        for c in &self.children {
            if let Child::Text(t) = c {
                out.push_str(t);
            }
        }
        out
    }
}

/// How deep elements may nest: a document deeper than that is refused (it
/// is no drawing, and following it would take the stack).
const MAX_DEPTH: usize = 512;

/// The document's root element.
pub(super) fn parse(text: &str) -> Result<Element, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let b = text.as_bytes();
    let mut entities: HashMap<String, String> = HashMap::new();
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    let mut i = 0usize;
    let find = |from: usize, what: &str| text[from..].find(what).map(|at| from + at);
    while i < b.len() {
        if b[i] != b'<' {
            let end = find(i, "<").unwrap_or(b.len());
            if let Some(top) = stack.last_mut() {
                let t = decode(&text[i..end], &entities);
                if !t.is_empty() {
                    top.children.push(Child::Text(t));
                }
            }
            i = end;
            continue;
        }
        let rest = &text[i..];
        if rest.starts_with("<?") {
            i = find(i, "?>").map_or(b.len(), |at| at + 2);
        } else if rest.starts_with("<!--") {
            i = find(i + 4, "-->").map_or(b.len(), |at| at + 3);
        } else if rest.starts_with("<![CDATA[") {
            let end = find(i + 9, "]]>").unwrap_or(b.len());
            if let Some(top) = stack.last_mut() {
                top.children.push(Child::Text(text[i + 9..end].to_string()));
            }
            i = (end + 3).min(b.len());
        } else if rest.starts_with("<!") {
            i = doctype(text, i, &mut entities);
        } else if rest.starts_with("</") {
            i = find(i, ">").map_or(b.len(), |at| at + 1);
            if let Some(done) = stack.pop() {
                close(&mut stack, &mut root, done);
            }
        } else {
            let (element, end, empty) = start_tag(text, i, &entities);
            i = end;
            if empty {
                close(&mut stack, &mut root, element);
            } else {
                if stack.len() >= MAX_DEPTH {
                    return Err("elements nested too deep".into());
                }
                stack.push(element);
            }
        }
        if root.is_some() && stack.is_empty() {
            break;
        }
    }
    // Elements left open (a file cut short) end where it does.
    while let Some(done) = stack.pop() {
        close(&mut stack, &mut root, done);
    }
    root.ok_or_else(|| "no element".to_string())
}

/// An element ended: into the one holding it, or the root.
fn close(stack: &mut [Element], root: &mut Option<Element>, done: Element) {
    match stack.last_mut() {
        Some(parent) => parent.children.push(Child::Element(done)),
        None => {
            if root.is_none() {
                *root = Some(done);
            }
        }
    }
}

/// The start tag at `at`: its element, where it ends and whether it is
/// empty (`<… />`).
fn start_tag(text: &str, at: usize, entities: &HashMap<String, String>) -> (Element, usize, bool) {
    let b = text.as_bytes();
    let mut i = at + 1;
    let name_start = i;
    while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'/' && b[i] != b'>' {
        i += 1;
    }
    let qname = &text[name_start..i];
    let name = qname.rsplit(':').next().unwrap_or(qname).to_string();
    let mut attrs = Vec::new();
    loop {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() {
            return (
                Element {
                    name,
                    attrs,
                    children: Vec::new(),
                },
                b.len(),
                true,
            );
        }
        if b[i] == b'>' {
            return (
                Element {
                    name,
                    attrs,
                    children: Vec::new(),
                },
                i + 1,
                false,
            );
        }
        if b[i] == b'/' {
            let end = text[i..].find('>').map_or(b.len(), |e| i + e + 1);
            return (
                Element {
                    name,
                    attrs,
                    children: Vec::new(),
                },
                end,
                true,
            );
        }
        let an = i;
        while i < b.len()
            && !b[i].is_ascii_whitespace()
            && b[i] != b'='
            && b[i] != b'>'
            && b[i] != b'/'
        {
            i += 1;
        }
        let attr = text[an..i].to_string();
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < b.len() && b[i] == b'=' {
            i += 1;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
                let q = b[i] as char;
                let vs = i + 1;
                let ve = text[vs..].find(q).map_or(b.len(), |e| vs + e);
                let raw = &text[vs..ve];
                // Attribute values' white space becomes spaces (XML 1.0,
                // 3.3.3).
                let value = decode(raw, entities).replace(['\t', '\n', '\r'], " ");
                attrs.push((attr, value));
                i = (ve + 1).min(b.len());
            } else {
                // An unquoted value (not XML, but written by hand).
                let vs = i;
                while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'>' {
                    i += 1;
                }
                attrs.push((attr, decode(&text[vs..i], entities)));
            }
        } else if !attr.is_empty() {
            attrs.push((attr, String::new()));
        } else {
            i += 1;
        }
    }
}

/// The markup declaration at `at` (`<!DOCTYPE …>`, its internal subset's
/// entities noted); where it ends.
fn doctype(text: &str, at: usize, entities: &mut HashMap<String, String>) -> usize {
    let b = text.as_bytes();
    let mut i = at + 2;
    let mut depth = 0i32;
    while i < b.len() {
        match b[i] {
            b'[' => depth += 1,
            b']' => depth -= 1,
            b'"' | b'\'' => {
                let q = b[i] as char;
                i = text[i + 1..].find(q).map_or(b.len(), |e| i + 1 + e);
            }
            b'<' if text[i..].starts_with("<!ENTITY") => {
                let end = entity(text, i, entities);
                i = end;
                continue;
            }
            b'<' if text[i..].starts_with("<!--") => {
                i = text[i..].find("-->").map_or(b.len(), |e| i + e + 3);
                continue;
            }
            b'>' if depth <= 0 => return i + 1,
            _ => {}
        }
        i += 1;
    }
    b.len()
}

/// `<!ENTITY name "value">` at `at`: noted (parameter entities, `%`, are
/// not); where it ends.
fn entity(text: &str, at: usize, entities: &mut HashMap<String, String>) -> usize {
    let end = text[at..].find('>').map_or(text.len(), |e| at + e + 1);
    let decl = &text[at + "<!ENTITY".len()..end.saturating_sub(1).max(at + 8)];
    let mut parts = decl.trim_start().splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or("").trim();
    let rest = parts.next().unwrap_or("").trim();
    if name != "%"
        && !name.is_empty()
        && let Some(q) = rest.chars().next().filter(|c| *c == '"' || *c == '\'')
        && let Some(close) = rest[1..].find(q)
    {
        // Quotes may be inside the other kind: `end` found the right `>`
        // only when none is in the value; look again from the value's end.
        let value = rest[1..1 + close].to_string();
        entities.insert(name.to_string(), value);
    }
    end
}

/// `s` with its entity and character references replaced.
fn decode(s: &str, entities: &HashMap<String, String>) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let Some(semi) = tail.find(';').filter(|&e| e <= 32) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let name = &tail[1..semi];
        let replaced = match name {
            "lt" => Some("<".to_string()),
            "gt" => Some(">".to_string()),
            "amp" => Some("&".to_string()),
            "quot" => Some("\"".to_string()),
            "apos" => Some("'".to_string()),
            _ if name.starts_with("#x") || name.starts_with("#X") => {
                u32::from_str_radix(&name[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
                    .map(String::from)
            }
            _ if name.starts_with('#') => name[1..]
                .parse::<u32>()
                .ok()
                .and_then(char::from_u32)
                .map(String::from),
            _ => entities.get(name).cloned(),
        };
        match replaced {
            Some(r) => {
                out.push_str(&r);
                rest = &tail[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The root element's start tag alone (from a file's first bytes): its
/// attributes; `None` when it is not an `<svg>`.
pub(super) fn root_tag(text: &str) -> Option<Element> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut entities = HashMap::new();
    let b = text.as_bytes();
    let mut i = 0usize;
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &text[i..];
        if rest.starts_with("<?") {
            i = rest.find("?>").map(|e| i + e + 2)?;
        } else if rest.starts_with("<!--") {
            i = rest.find("-->").map(|e| i + e + 3)?;
        } else if rest.starts_with("<!") {
            i = doctype(text, i, &mut entities);
        } else {
            // The whole tag must be there.
            rest.find('>')?;
            let (element, _, _) = start_tag(text, i, &entities);
            return (element.name == "svg").then_some(element);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_elements_attributes_text_and_entities() {
        let doc = r##"<?xml version="1.0"?>
<!DOCTYPE svg [ <!ENTITY ns "http://www.w3.org/2000/svg"> ]>
<!-- a comment -->
<svg xmlns="&ns;" width="10">
  <svg:g id='a' fill="#f00"><rect x="1"/></svg:g>
  <style><![CDATA[ .a { fill: red } ]]></style>
  <text>A &amp; B &#65;&#x42;</text>
</svg>"##;
        let root = parse(doc).expect("a document");
        assert_eq!(root.name, "svg");
        assert_eq!(root.attr("xmlns"), Some("http://www.w3.org/2000/svg"));
        let kids: Vec<&Element> = root.elements().collect();
        assert_eq!(kids[0].name, "g", "the prefix left out");
        assert_eq!(kids[0].attr("id"), Some("a"));
        assert_eq!(
            kids[0].elements().next().map(|e| e.name.as_str()),
            Some("rect")
        );
        assert!(kids[1].text().contains(".a { fill: red }"));
        assert_eq!(kids[2].text(), "A & B AB");
        let tag = root_tag(doc).expect("the root's tag");
        assert_eq!(tag.attr("width"), Some("10"));
    }

    #[test]
    fn a_file_cut_short_ends_its_open_elements() {
        let root = parse("<svg><g><rect width='1'/><g").expect("what is there");
        let g = root.elements().next().expect("the group");
        assert_eq!(g.elements().count(), 2);
    }
}
