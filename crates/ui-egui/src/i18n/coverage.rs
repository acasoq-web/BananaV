//! Which texts each language lacks and which of its entries no text uses
//! any more. `cargo xtask i18n` prints the report (and writes it to
//! `target/i18n/<tag>.txt` for translators); the tests fail when a language
//! marked `complete` lacks a text or keeps an unused entry.
//!
//! Texts are found in the source: the string literals passed to `tr`,
//! `trf!`, `trc` and `trn!` in this crate. Names the interface translates
//! by value (`tr(effect.name())`) are not seen there, so an entry counts
//! as used when its English text is a string literal anywhere in the
//! workspace.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use super::{Language, Locale};

/// A text the interface translates.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Use {
    /// `tr("…")`, `trf!("…", …)`.
    Text(String),
    /// `trc("context", "…")`.
    Context(String, String),
    /// `trn!(n, "one", "other", …)`.
    Plural(String, String),
}

#[derive(Debug, PartialEq)]
enum Tok {
    Ident(String),
    Str(String),
    Punct(char),
}

/// The identifiers, string literals and punctuation of Rust source;
/// comments, numbers, characters and lifetimes are left out.
fn lex(src: &str) -> Vec<Tok> {
    let c: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch == '/' && c.get(i + 1) == Some(&'/') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
        } else if ch == '/' && c.get(i + 1) == Some(&'*') {
            let mut depth = 0;
            while i < c.len() {
                if c[i] == '/' && c.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if c[i] == '*' && c.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
        } else if ch == '"' {
            let (s, next) = string(&c, i + 1);
            out.push(Tok::Str(s));
            i = next;
        } else if ch.is_alphabetic() || ch == '_' {
            let start = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            let word: String = c[start..i].iter().collect();
            // Raw and byte strings: r"…", r#"…"#, b"…", br"…".
            if matches!(word.as_str(), "r" | "br") && matches!(c.get(i), Some('"' | '#')) {
                let mut hashes = 0;
                while c.get(i) == Some(&'#') {
                    hashes += 1;
                    i += 1;
                }
                i += 1; // the opening quote
                let start = i;
                loop {
                    if i >= c.len() {
                        break;
                    }
                    if c[i] == '"' && (0..hashes).all(|h| c.get(i + 1 + h) == Some(&'#')) {
                        break;
                    }
                    i += 1;
                }
                out.push(Tok::Str(c[start..i.min(c.len())].iter().collect()));
                i += 1 + hashes;
            } else if word == "b" && c.get(i) == Some(&'"') {
                let (s, next) = string(&c, i + 1);
                out.push(Tok::Str(s));
                i = next;
            } else {
                out.push(Tok::Ident(word));
            }
        } else if ch.is_ascii_digit() {
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_' || c[i] == '.') {
                // `0..n`: a range, not a decimal point.
                if c[i] == '.' && c.get(i + 1) == Some(&'.') {
                    break;
                }
                i += 1;
            }
        } else if ch == '\'' {
            // A character ('a', '\n', '\u{2212}') or a lifetime ('a).
            if c.get(i + 1) == Some(&'\\') {
                i += 2;
                while i < c.len() && c[i] != '\'' {
                    i += 1;
                }
                i += 1;
            } else if c.get(i + 2) == Some(&'\'') {
                i += 3;
            } else {
                i += 1;
                while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                    i += 1;
                }
            }
        } else {
            out.push(Tok::Punct(ch));
            i += 1;
        }
    }
    out
}

/// A string literal's text from just after its opening quote; returns it
/// and the position after the closing quote.
fn string(c: &[char], mut i: usize) -> (String, usize) {
    let mut s = String::new();
    while i < c.len() && c[i] != '"' {
        if c[i] == '\\' {
            i += 1;
            match c.get(i) {
                Some('n') => s.push('\n'),
                Some('t') => s.push('\t'),
                Some('r') => s.push('\r'),
                Some('0') => s.push('\0'),
                Some('x') => {
                    let hex: String = c[i + 1..(i + 3).min(c.len())].iter().collect();
                    if let Ok(v) = u8::from_str_radix(&hex, 16) {
                        s.push(v as char);
                    }
                    i += 2;
                }
                Some('u') => {
                    let close = (i..c.len()).find(|&j| c[j] == '}').unwrap_or(i);
                    let hex: String = c[(i + 2).min(close)..close].iter().collect();
                    if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                        s.push(ch);
                    }
                    i = close;
                }
                // A line continuation skips the newline and the indentation.
                Some('\n') | Some('\r') => {
                    while i + 1 < c.len() && c[i + 1].is_whitespace() {
                        i += 1;
                    }
                }
                Some(other) => s.push(*other),
                None => {}
            }
            i += 1;
        } else {
            s.push(c[i]);
            i += 1;
        }
    }
    (s, i + 1)
}

/// The texts `tr`, `trf!`, `trc` and `trn!` are called with in `toks`.
fn uses(toks: &[Tok]) -> Vec<Use> {
    let punct = |i: usize, p: char| matches!(toks.get(i), Some(Tok::Punct(q)) if *q == p);
    let text = |i: usize| match toks.get(i) {
        Some(Tok::Str(s)) => Some(s.clone()),
        _ => None,
    };
    let mut out = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        let Tok::Ident(name) = t else {
            continue;
        };
        match name.as_str() {
            "tr" if punct(i + 1, '(') => {
                // `tr("…")`, or `tr("…",)` as rustfmt writes a long one.
                let closed = punct(i + 3, ')') || (punct(i + 3, ',') && punct(i + 4, ')'));
                if let Some(s) = text(i + 2).filter(|_| closed) {
                    out.push(Use::Text(s));
                }
            }
            "trf" if punct(i + 1, '!') && punct(i + 2, '(') => {
                if let Some(s) = text(i + 3) {
                    out.push(Use::Text(s));
                }
            }
            "trc" if punct(i + 1, '(') => {
                if let (Some(ctx), Some(s)) = (text(i + 2), text(i + 4))
                    && punct(i + 3, ',')
                {
                    out.push(Use::Context(ctx, s));
                }
            }
            "trn" if punct(i + 1, '!') && punct(i + 2, '(') => {
                // Past the count (an expression) to the first top-level comma.
                let mut depth = 0;
                let mut j = i + 3;
                while let Some(t) = toks.get(j) {
                    match t {
                        Tok::Punct('(' | '[' | '{') => depth += 1,
                        Tok::Punct(')' | ']' | '}') if depth == 0 => break,
                        Tok::Punct(')' | ']' | '}') => depth -= 1,
                        Tok::Punct(',') if depth == 0 => break,
                        _ => {}
                    }
                    j += 1;
                }
                if punct(j, ',')
                    && let (Some(one), Some(other)) = (text(j + 1), text(j + 3))
                    && punct(j + 2, ',')
                {
                    out.push(Use::Plural(one, other));
                }
            }
            _ => {}
        }
    }
    out
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// What the source says about texts.
struct Sources {
    /// The texts the interface translates (this crate).
    uses: BTreeSet<Use>,
    /// Every string literal of the workspace.
    literals: HashSet<String>,
}

impl Sources {
    fn read() -> Sources {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace = crate_dir.join("..").join("..");
        let mut files = Vec::new();
        for group in ["crates", "apps"] {
            if let Ok(entries) = std::fs::read_dir(workspace.join(group)) {
                for e in entries.flatten() {
                    rust_files(&e.path().join("src"), &mut files);
                    rust_files(&e.path().join("tests"), &mut files);
                }
            }
        }
        let own = crate_dir.join("src").canonicalize().unwrap_or_default();
        // The catalogs are not sources of texts.
        let i18n = own.join("i18n");
        let is_catalog = |p: &Path| {
            p.starts_with(&i18n)
                && !matches!(
                    p.file_name().and_then(|n| n.to_str()),
                    Some("mod.rs" | "coverage.rs")
                )
        };
        let mut uses = BTreeSet::new();
        let mut literals = HashSet::new();
        for f in files {
            let f = f.canonicalize().unwrap_or(f);
            if is_catalog(&f) {
                continue;
            }
            let Ok(src) = std::fs::read_to_string(&f) else {
                continue;
            };
            let toks = lex(&src);
            if f.starts_with(&own) {
                uses.extend(self::uses(&toks));
            }
            literals.extend(toks.into_iter().filter_map(|t| match t {
                Tok::Str(s) => Some(s),
                _ => None,
            }));
        }
        Sources { uses, literals }
    }
}

/// One language's gaps.
#[derive(Debug, Default)]
struct Coverage {
    /// Texts the interface shows that the catalog lacks.
    missing: Vec<String>,
    /// Catalog entries whose English text the source no longer has.
    unused: Vec<String>,
}

fn coverage(locale: &Locale, src: &Sources) -> Coverage {
    let texts: HashSet<&str> = locale.texts.iter().map(|(en, _)| *en).collect();
    let plurals: HashSet<(&str, &str)> = locale.plurals.iter().map(|(o, t, _)| (*o, *t)).collect();
    let mut c = Coverage::default();
    for u in &src.uses {
        match u {
            Use::Text(s) if !texts.contains(s.as_str()) => c.missing.push(s.clone()),
            Use::Context(ctx, s)
                if !texts.contains(s.as_str())
                    && !locale.context.iter().any(|(x, en, _)| x == ctx && en == s) =>
            {
                c.missing.push(format!("[{ctx}] {s}"));
            }
            Use::Plural(one, other) if !plurals.contains(&(one.as_str(), other.as_str())) => {
                c.missing.push(format!("{one} | {other}"));
            }
            _ => {}
        }
    }
    let used = |s: &str| src.literals.contains(s);
    c.unused.extend(
        locale
            .texts
            .iter()
            .filter(|(en, _)| !used(en))
            .map(|(en, _)| en.to_string()),
    );
    c.unused.extend(
        locale
            .plurals
            .iter()
            .filter(|(one, other, _)| {
                !src.uses
                    .contains(&Use::Plural(one.to_string(), other.to_string()))
            })
            .map(|(one, other, _)| format!("{one} | {other}")),
    );
    c.unused.extend(
        locale
            .context
            .iter()
            .filter(|(_, en, _)| !used(en))
            .map(|(ctx, en, _)| format!("[{ctx}] {en}")),
    );
    c
}

/// The languages with a catalog (all but English).
fn translated() -> impl Iterator<Item = Language> {
    Language::ALL
        .iter()
        .copied()
        .filter(|l| *l != Language::English)
}

#[test]
fn complete_languages_translate_every_text() {
    let src = Sources::read();
    assert!(
        src.uses.len() > 500,
        "the source scan found only {} texts",
        src.uses.len()
    );
    let mut problems = Vec::new();
    for lang in translated().filter(|l| l.locale().complete) {
        let c = coverage(lang.locale(), &src);
        for m in c.missing {
            problems.push(format!("{}: missing {m:?}", lang.code()));
        }
        for u in c.unused {
            problems.push(format!("{}: unused entry {u:?}", lang.code()));
        }
    }
    assert!(
        problems.is_empty(),
        "{}\n(`cargo xtask i18n` lists every language)",
        problems.join("\n")
    );
}

/// `cargo xtask i18n`: every language's gaps, printed and written to
/// `target/i18n/<tag>.txt`.
#[test]
#[ignore = "run by `cargo xtask i18n`"]
fn report() {
    let src = Sources::read();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/i18n");
    let _ = std::fs::create_dir_all(&dir);
    for lang in translated() {
        let c = coverage(lang.locale(), &src);
        let mut text = format!(
            "{} ({}): {} texts in the source, {} missing, {} unused entries\n",
            lang.native_name(),
            lang.code(),
            src.uses.len(),
            c.missing.len(),
            c.unused.len()
        );
        if !c.missing.is_empty() {
            text.push_str("\nMissing:\n");
            for m in &c.missing {
                text.push_str(&format!("  {m}\n"));
            }
        }
        if !c.unused.is_empty() {
            text.push_str("\nUnused:\n");
            for u in &c.unused {
                text.push_str(&format!("  {u}\n"));
            }
        }
        eprintln!("{text}");
        let _ = std::fs::write(dir.join(format!("{}.txt", lang.code())), text);
    }
}

#[test]
fn the_scanner_reads_calls() {
    let src = r##"
        // tr("in a comment")
        let a = tr("Save");
        let b = trf!("Saved to {}", path);
        let c = trc("key", "End");
        let d = trn!(items.len().max(1), "{} file", "{} files", n);
        let e = tr(name); // by value: not seen
        let f = r#"raw "quoted""#;
        let g = 'x'; fn h<'a>(s: &'a str) {}
        let i = tr("line \
                    continued");
        let j = tr(
            "long",
        );
    "##;
    let toks = lex(src);
    assert_eq!(
        uses(&toks),
        vec![
            Use::Text("Save".into()),
            Use::Text("Saved to {}".into()),
            Use::Context("key".into(), "End".into()),
            Use::Plural("{} file".into(), "{} files".into()),
            Use::Text("line continued".into()),
            Use::Text("long".into()),
        ]
    );
    assert!(toks.contains(&Tok::Str("raw \"quoted\"".into())));
}
