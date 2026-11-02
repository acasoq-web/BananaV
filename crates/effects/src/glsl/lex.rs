//! GLSL's tokens, after a small preprocessor: comments left out (the `//`
//! ones kept apart with their lines: GL Transitions write uniforms'
//! defaults there), `#define`d names replaced by what they stand for
//! (with their arguments, for macros with parameters),
//! `#ifdef`, `#ifndef`, `#if`, `#elif`, `#else` and `#endif` followed
//! (`GL_ES` is defined; `#if` reads a number, a defined name and
//! `defined(NAME)`), and `#version`, `#extension`, `#pragma` and the like
//! left out.

use std::collections::HashMap;

use super::Error;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Tok {
    Ident(String),
    Float(f32),
    Int(i64),
    Punct(&'static str),
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Token {
    pub(super) tok: Tok,
    pub(super) line: usize,
}

/// The code's tokens, and its `//` comments (their line, the text after
/// the slashes).
#[derive(Debug, Default)]
pub(super) struct Lexed {
    pub(super) tokens: Vec<Token>,
    pub(super) comments: Vec<(usize, String)>,
}

/// The operators and signs, the longest first.
const PUNCTS: &[&str] = &[
    "<<=", ">>=", "++", "--", "<=", ">=", "==", "!=", "&&", "||", "^^", "+=", "-=", "*=", "/=",
    "%=", "&=", "|=", "^=", "<<", ">>", "(", ")", "[", "]", "{", "}", ".", ",", ";", ":", "?", "+",
    "-", "*", "/", "%", "<", ">", "=", "!", "~", "&", "|", "^",
];

/// How deep a `#define` may stand for others.
const MACRO_DEPTH: usize = 16;

/// The code without its comments (their lines kept), and the `//` ones.
fn strip_comments(src: &str) -> (String, Vec<(usize, String)>) {
    let mut out = String::with_capacity(src.len());
    let mut comments = Vec::new();
    let mut line = 1;
    let mut chars = src.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'/') {
            chars.next();
            let mut text = String::new();
            while let Some(&d) = chars.peek() {
                if d == '\n' {
                    break;
                }
                text.push(d);
                chars.next();
            }
            comments.push((line, text));
        } else if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            let mut last = ' ';
            for d in chars.by_ref() {
                if d == '\n' {
                    line += 1;
                    out.push('\n');
                }
                if last == '*' && d == '/' {
                    break;
                }
                last = d;
            }
            out.push(' ');
        } else {
            if c == '\n' {
                line += 1;
            }
            out.push(c);
        }
    }
    (out, comments)
}

/// A number at the start of `s`: its token and its length.
fn number(s: &str) -> Option<(Tok, usize)> {
    let b = s.as_bytes();
    let digits = |mut i: usize| {
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        i
    };
    if b.len() > 1 && b[0] == b'0' && (b[1] == b'x' || b[1] == b'X') {
        let mut i = 2;
        while i < b.len() && b[i].is_ascii_hexdigit() {
            i += 1;
        }
        let v = i64::from_str_radix(&s[2..i], 16).ok()?;
        if i < b.len() && (b[i] == b'u' || b[i] == b'U') {
            i += 1;
        }
        return Some((Tok::Int(v), i));
    }
    let mut i = digits(0);
    let mut float = false;
    if i < b.len() && b[i] == b'.' {
        float = true;
        i = digits(i + 1);
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        if j < b.len() && b[j].is_ascii_digit() {
            i = digits(j);
            float = true;
        }
    }
    let text = &s[..i];
    let mut len = i;
    if i < b.len() && (b[i] == b'f' || b[i] == b'F') {
        float = true;
        len += 1;
    } else if !float && i < b.len() && (b[i] == b'u' || b[i] == b'U') {
        len += 1;
    }
    if float {
        Some((Tok::Float(text.parse().ok()?), len))
    } else if text.len() > 1 && text.starts_with('0') {
        // Octal, as GLSL reads a leading zero.
        Some((Tok::Int(i64::from_str_radix(&text[1..], 8).ok()?), len))
    } else {
        Some((Tok::Int(text.parse().ok()?), len))
    }
}

/// The tokens of line `line`'s `text`.
fn tokens_of(text: &str, line: usize, out: &mut Vec<Token>) -> Result<(), Error> {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(Token {
                tok: Tok::Ident(text[start..i].to_string()),
                line,
            });
        } else if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            let (tok, len) =
                number(&text[i..]).ok_or_else(|| Error::new(line, "a number is malformed"))?;
            out.push(Token { tok, line });
            i += len;
        } else {
            match PUNCTS.iter().find(|p| text[i..].starts_with(**p)) {
                Some(p) => {
                    out.push(Token {
                        tok: Tok::Punct(p),
                        line,
                    });
                    i += p.len();
                }
                None => {
                    let ch = text[i..].chars().next().unwrap_or('?');
                    return Err(Error::new(line, format!("unexpected character '{ch}'")));
                }
            }
        }
    }
    Ok(())
}

/// A `#define`: its parameters (none: it stands for its body alone) and
/// its body.
struct Macro {
    params: Option<Vec<String>>,
    body: Vec<Tok>,
}

/// Whether `#if`'s condition `text` holds.
fn condition(text: &str, macros: &HashMap<String, Macro>) -> bool {
    let t = text.trim();
    if let Some(rest) = t.strip_prefix('!') {
        return !condition(rest, macros);
    }
    if let Some(rest) = t.strip_prefix("defined") {
        let name = rest
            .trim()
            .trim_start_matches('(')
            .trim_end_matches(')')
            .trim();
        return name == "GL_ES" || macros.contains_key(name);
    }
    match t.parse::<i64>() {
        Ok(v) => v != 0,
        Err(_) => {
            matches!(macros.get(t).and_then(|m| m.body.first()), Some(Tok::Int(v)) if *v != 0)
        }
    }
}

/// A macro call's arguments, from just after its '(' at `from`: each one's
/// tokens (parentheses within kept), and where the call ends.
fn arguments(t: &[Token], from: usize) -> Option<(Vec<Vec<Token>>, usize)> {
    let mut args = vec![Vec::new()];
    let mut depth = 0;
    for (k, token) in t.iter().enumerate().skip(from) {
        match token.tok {
            Tok::Punct("(") => depth += 1,
            Tok::Punct(")") if depth == 0 => {
                // `F()`: no argument at all.
                if args.len() == 1 && args[0].is_empty() {
                    args.clear();
                }
                return Some((args, k + 1));
            }
            Tok::Punct(")") => depth -= 1,
            Tok::Punct(",") if depth == 0 => {
                args.push(Vec::new());
                continue;
            }
            _ => {}
        }
        args.last_mut()?.push(token.clone());
    }
    None
}

/// `t` with the macros it names replaced (their own bodies too, up to
/// `MACRO_DEPTH` deep).
fn expand(t: &[Token], macros: &HashMap<String, Macro>, depth: usize) -> Vec<Token> {
    let mut out = Vec::with_capacity(t.len());
    let mut i = 0;
    while i < t.len() {
        let token = &t[i];
        if let Tok::Ident(name) = &token.tok
            && depth < MACRO_DEPTH
            && let Some(m) = macros.get(name)
        {
            let at = |tok: &Tok| Token {
                tok: tok.clone(),
                line: token.line,
            };
            match &m.params {
                None => {
                    let body: Vec<Token> = m.body.iter().map(at).collect();
                    out.extend(expand(&body, macros, depth + 1));
                    i += 1;
                    continue;
                }
                Some(params) => {
                    if matches!(t.get(i + 1).map(|n| &n.tok), Some(Tok::Punct("(")))
                        && let Some((args, next)) = arguments(t, i + 2)
                        && args.len() == params.len()
                    {
                        let mut body = Vec::new();
                        for tok in &m.body {
                            match tok {
                                Tok::Ident(n) if params.contains(n) => {
                                    let k = params.iter().position(|p| p == n).unwrap_or(0);
                                    body.extend(args[k].iter().cloned());
                                }
                                _ => body.push(at(tok)),
                            }
                        }
                        out.extend(expand(&body, macros, depth + 1));
                        i = next;
                        continue;
                    }
                }
            }
        }
        out.push(token.clone());
        i += 1;
    }
    out
}

pub(super) fn lex(src: &str) -> Result<Lexed, Error> {
    let (code, comments) = strip_comments(src);
    let mut macros: HashMap<String, Macro> = HashMap::new();
    // Each open conditional: whether its lines are kept, and whether one
    // of its branches was.
    let mut open: Vec<(bool, bool)> = Vec::new();
    let mut raw = Vec::new();
    let mut last = 1;
    for (i, text) in code.lines().enumerate() {
        let line = i + 1;
        last = line;
        let live = open.iter().all(|o| o.0);
        let t = text.trim_start();
        let Some(d) = t.strip_prefix('#') else {
            if live {
                tokens_of(text, line, &mut raw)?;
            }
            continue;
        };
        let d = d.trim_start();
        let (word, rest) = d.split_once(char::is_whitespace).unwrap_or((d, ""));
        let rest = rest.trim();
        let unmatched = |w: &str| Error::new(line, format!("#{w} without #if"));
        match word {
            "ifdef" | "ifndef" => {
                let defined = rest == "GL_ES" || macros.contains_key(rest);
                let on = live && defined == (word == "ifdef");
                open.push((on, on));
            }
            "if" => {
                let on = live && condition(rest, &macros);
                open.push((on, on));
            }
            "elif" => {
                let outer = open.len() < 2 || open[..open.len() - 1].iter().all(|o| o.0);
                let top = open.last_mut().ok_or_else(|| unmatched(word))?;
                let on = outer && !top.1 && condition(rest, &macros);
                top.0 = on;
                top.1 |= on;
            }
            "else" => {
                let outer = open.len() < 2 || open[..open.len() - 1].iter().all(|o| o.0);
                let top = open.last_mut().ok_or_else(|| unmatched(word))?;
                top.0 = outer && !top.1;
                top.1 = true;
            }
            "endif" => {
                open.pop().ok_or_else(|| unmatched(word))?;
            }
            "define" if live => {
                // Its name, its parameters right after it in parentheses
                // (a macro with parameters), its body.
                let end = rest
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .unwrap_or(rest.len());
                let (name, after) = rest.split_at(end);
                let (params, body) = match after.strip_prefix('(') {
                    Some(list) => {
                        let (list, body) = list.split_once(')').ok_or_else(|| {
                            Error::new(line, format!("#define {name}( without its ')'"))
                        })?;
                        let params = list
                            .split(',')
                            .map(|p| p.trim().to_string())
                            .filter(|p| !p.is_empty())
                            .collect();
                        (Some(params), body)
                    }
                    None => (None, after),
                };
                let mut toks = Vec::new();
                tokens_of(body, line, &mut toks)?;
                let body = toks.into_iter().map(|t| t.tok).collect();
                macros.insert(name.to_string(), Macro { params, body });
            }
            "undef" if live => {
                macros.remove(rest);
            }
            // #version, #extension, #pragma, #line, and what a branch left
            // out says.
            _ => {}
        }
    }
    if !open.is_empty() {
        return Err(Error::new(last, "#if without #endif"));
    }
    let tokens = expand(&raw, &macros, 0);
    Ok(Lexed { tokens, comments })
}
