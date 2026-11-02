//! GLSL's syntax: the declarations, functions, statements and expressions
//! of the tokens [`super::lex`] gives.

use super::lex::{Lexed, Tok, Token};
use super::{Error, Ty};

/// An expression, and its line.
#[derive(Clone, Debug)]
pub(super) struct E {
    pub(super) kind: Ek,
    pub(super) line: usize,
}

#[derive(Clone, Debug)]
pub(super) enum Ek {
    Float(f32),
    Int(i64),
    Bool(bool),
    Name(String),
    /// A function's call or a type's constructor.
    Call(String, Vec<E>),
    /// A swizzle (`.xy`, `.rgb`).
    Field(Box<E>, String),
    Index(Box<E>, Box<E>),
    Unary(&'static str, Box<E>),
    Binary(&'static str, Box<E>, Box<E>),
    /// `=` or a compound assignment (`+=`…).
    Assign(&'static str, Box<E>, Box<E>),
    /// `++` or `--`, before (true) or after.
    Step(&'static str, bool, Box<E>),
    Cond(Box<E>, Box<E>, Box<E>),
    Comma(Box<E>, Box<E>),
    /// An array's constructor (`float[3](…)`, its size or none: as many
    /// as its values).
    Array(Ty, Option<Box<E>>, Vec<E>),
    /// An array's `.length()`.
    Length(Box<E>),
}

/// Whether a variable is an array: `None` not one, `Some(None)` one whose
/// size its value says (`float a[] = …`), `Some(Some(n))` of size `n`.
pub(super) type Size = Option<Option<E>>;

#[derive(Clone, Debug)]
pub(super) enum St {
    Decl {
        fixed: bool,
        ty: Ty,
        /// Each variable's name, size (an array's) and value.
        vars: Vec<(String, Size, Option<E>)>,
        line: usize,
    },
    Expr(E),
    If(E, Box<St>, Option<Box<St>>),
    For {
        init: Option<Box<St>>,
        cond: Option<E>,
        step: Option<E>,
        body: Box<St>,
    },
    While(E, Box<St>),
    Do(Box<St>, E),
    Return(Option<E>, usize),
    Break(usize),
    Continue(usize),
    Discard,
    Block(Vec<St>),
    Empty,
}

/// A function's parameter: read in (`in`, `inout`), written back (`out`,
/// `inout`).
#[derive(Clone, Debug)]
pub(super) struct Param {
    pub(super) ty: Ty,
    pub(super) name: String,
    pub(super) input: bool,
    pub(super) output: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Qual {
    None,
    Const,
    Uniform,
}

#[derive(Clone, Debug)]
pub(super) enum Item {
    /// A struct: its name and its fields' names and types, in order.
    Struct {
        name: String,
        fields: Vec<(String, Ty)>,
    },
    Global {
        qual: Qual,
        ty: Ty,
        name: String,
        size: Size,
        init: Option<E>,
        /// A uniform's default, as GL Transitions write it after it
        /// (`// = 1.0`).
        default: Option<String>,
        line: usize,
    },
    Func {
        ret: Ty,
        name: String,
        params: Vec<Param>,
        body: Option<Vec<St>>,
        line: usize,
    },
}

/// Precision and interpolation qualifiers: left out.
const IGNORED: &[&str] = &[
    "lowp",
    "mediump",
    "highp",
    "flat",
    "smooth",
    "noperspective",
    "invariant",
];

/// The binary operators by precedence, the loosest first.
const LEVELS: &[&[&str]] = &[
    &["||"],
    &["^^"],
    &["&&"],
    &["|"],
    &["^"],
    &["&"],
    &["==", "!="],
    &["<", ">", "<=", ">="],
    &["<<", ">>"],
    &["+", "-"],
    &["*", "/", "%"],
];

const ASSIGNMENTS: &[&str] = &[
    "=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=",
];

struct Parser<'a> {
    t: &'a [Token],
    i: usize,
    comments: &'a [(usize, String)],
    /// The structs declared so far, as types.
    structs: Vec<(String, Ty)>,
}

/// The code's declarations and functions.
pub(super) fn parse(lexed: &Lexed) -> Result<Vec<Item>, Error> {
    let mut p = Parser {
        t: &lexed.tokens,
        i: 0,
        comments: &lexed.comments,
        structs: Vec::new(),
    };
    let mut items = Vec::new();
    while !p.done() {
        p.item(&mut items)?;
    }
    Ok(items)
}

/// A lone expression (a uniform's default).
pub(super) fn expression(lexed: &Lexed) -> Result<E, Error> {
    let mut p = Parser {
        t: &lexed.tokens,
        i: 0,
        comments: &[],
        structs: Vec::new(),
    };
    let e = p.assignment()?;
    if !p.done() {
        return Err(p.unexpected("the end"));
    }
    Ok(e)
}

impl Parser<'_> {
    fn done(&self) -> bool {
        self.i >= self.t.len()
    }

    fn line(&self) -> usize {
        self.t.get(self.i).or(self.t.last()).map_or(1, |t| t.line)
    }

    fn peek(&self) -> Option<&Tok> {
        self.peek_at(0)
    }

    fn peek_at(&self, k: usize) -> Option<&Tok> {
        self.t.get(self.i + k).map(|t| &t.tok)
    }

    fn is(&self, p: &str) -> bool {
        matches!(self.peek(), Some(Tok::Punct(q)) if *q == p)
    }

    fn is_word(&self, w: &str) -> bool {
        matches!(self.peek(), Some(Tok::Ident(s)) if s == w)
    }

    fn eat(&mut self, p: &str) -> bool {
        let yes = self.is(p);
        if yes {
            self.i += 1;
        }
        yes
    }

    fn expect(&mut self, p: &str) -> Result<(), Error> {
        if self.eat(p) {
            Ok(())
        } else {
            Err(self.unexpected(&format!("'{p}'")))
        }
    }

    fn unexpected(&self, wanted: &str) -> Error {
        let found = match self.peek() {
            None => "the end".to_string(),
            Some(Tok::Ident(s)) => format!("'{s}'"),
            Some(Tok::Punct(p)) => format!("'{p}'"),
            Some(Tok::Float(v)) => v.to_string(),
            Some(Tok::Int(v)) => v.to_string(),
        };
        Error::new(self.line(), format!("expected {wanted}, found {found}"))
    }

    fn ident(&mut self) -> Result<String, Error> {
        match self.peek() {
            Some(Tok::Ident(s)) => {
                let s = s.clone();
                self.i += 1;
                Ok(s)
            }
            _ => Err(self.unexpected("a name")),
        }
    }

    fn arrays_unsupported(&self) -> Result<(), Error> {
        if self.is("[") {
            Err(Error::new(
                self.line(),
                "arrays as parameters are not supported yet",
            ))
        } else {
            Ok(())
        }
    }

    /// An array's size here (`[4]`, `[]`), if any.
    fn size(&mut self) -> Result<Size, Error> {
        if !self.eat("[") {
            return Ok(None);
        }
        if self.eat("]") {
            return Ok(Some(None));
        }
        let n = self.expression()?;
        self.expect("]")?;
        Ok(Some(Some(n)))
    }

    /// How many ignored qualifiers come first.
    fn ignored(&self) -> usize {
        let mut k = 0;
        while matches!(self.peek_at(k), Some(Tok::Ident(s)) if IGNORED.contains(&s.as_str())) {
            k += 1;
        }
        k
    }

    /// The type named `name`: GLSL's, or a struct of the code's.
    fn named(&self, name: &str) -> Option<Ty> {
        self.structs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, t)| *t)
            .or_else(|| Ty::named(name))
    }

    /// Whether a declaration starts here: a type, then a name (a type then
    /// '(' is a constructor).
    fn declaration_here(&self) -> bool {
        let mut k = self.ignored();
        if !matches!(self.peek_at(k), Some(Tok::Ident(s)) if self.named(s).is_some()) {
            return false;
        }
        k += 1;
        // An array's type (`float[4] a`), not its constructor.
        if matches!(self.peek_at(k), Some(Tok::Punct("["))) {
            while !matches!(self.peek_at(k), Some(Tok::Punct("]")) | None) {
                k += 1;
            }
            k += 1;
        }
        matches!(self.peek_at(k), Some(Tok::Ident(_)))
    }

    fn ty(&mut self) -> Result<Ty, Error> {
        self.i += self.ignored();
        let line = self.line();
        let name = self.ident()?;
        let ty = self
            .named(&name)
            .ok_or_else(|| Error::new(line, format!("unknown type '{name}'")))?;
        Ok(ty)
    }

    fn item(&mut self, items: &mut Vec<Item>) -> Result<(), Error> {
        let line = self.line();
        if self.eat(";") {
            return Ok(());
        }
        if self.is_word("precision") {
            while !self.eat(";") {
                if self.done() {
                    return Err(self.unexpected("';'"));
                }
                self.i += 1;
            }
            return Ok(());
        }
        if self.is_word("struct") {
            self.i += 1;
            return self.structure(items, line);
        }
        let mut qual = Qual::None;
        while let Some(Tok::Ident(s)) = self.peek() {
            match s.as_str() {
                "const" => qual = Qual::Const,
                "uniform" => qual = Qual::Uniform,
                "in" | "out" | "varying" | "attribute" | "layout" => {
                    return Err(Error::new(
                        line,
                        format!("'{s}' variables are not supported: write a transition function"),
                    ));
                }
                _ => break,
            }
            self.i += 1;
        }
        let ty = self.ty()?;
        let typed = self.size()?;
        let mut line = self.line();
        let mut name = self.ident()?;
        if self.is("(") {
            if typed.is_some() {
                return Err(Error::new(line, "a function cannot return an array"));
            }
            if qual != Qual::None {
                return Err(Error::new(line, "a function cannot be const or uniform"));
            }
            let params = self.params()?;
            let body = if self.eat(";") {
                None
            } else {
                Some(self.block()?)
            };
            items.push(Item::Func {
                ret: ty,
                name,
                params,
                body,
                line,
            });
            return Ok(());
        }
        let first = items.len();
        loop {
            let size = self.size()?.or_else(|| typed.clone());
            let init = if self.eat("=") {
                Some(self.assignment()?)
            } else {
                None
            };
            items.push(Item::Global {
                qual,
                ty,
                name,
                size,
                init,
                default: None,
                line,
            });
            if !self.eat(",") {
                break;
            }
            line = self.line();
            name = self.ident()?;
        }
        let end = self.line();
        self.expect(";")?;
        // GL Transitions' defaults: `uniform float x; // = 1.0`.
        if qual == Qual::Uniform
            && let Some((_, text)) = self.comments.iter().find(|(l, _)| *l == end)
            && let Some(d) = text.trim().strip_prefix('=')
        {
            for item in &mut items[first..] {
                if let Item::Global { default, .. } = item {
                    *default = Some(d.trim().to_string());
                }
            }
        }
        Ok(())
    }

    /// A struct's declaration, after `struct`: its fields, numbers only,
    /// 16 of them at most.
    fn structure(&mut self, items: &mut Vec<Item>, line: usize) -> Result<(), Error> {
        let name = self.ident()?;
        self.expect("{")?;
        let mut fields: Vec<(String, Ty)> = Vec::new();
        while !self.eat("}") {
            let ty = self.ty()?;
            if !matches!(ty, Ty::Vec(..) | Ty::Mat(_)) {
                return Err(Error::new(self.line(), "a struct's fields must be numbers"));
            }
            loop {
                let f = self.ident()?;
                if self.is("[") {
                    return Err(Error::new(
                        self.line(),
                        "arrays in structs are not supported yet",
                    ));
                }
                if fields.iter().any(|(n, _)| *n == f) {
                    return Err(Error::new(self.line(), format!("'{f}' twice")));
                }
                fields.push((f, ty));
                if !self.eat(",") {
                    break;
                }
            }
            self.expect(";")?;
            if self.done() {
                return Err(self.unexpected("'}'"));
            }
        }
        if !matches!(self.peek(), Some(Tok::Punct(";"))) {
            return Err(Error::new(
                self.line(),
                "a struct's variables are not supported yet",
            ));
        }
        self.expect(";")?;
        let size: usize = fields.iter().map(|(_, t)| t.size()).sum();
        if fields.is_empty() || size > 16 || self.structs.len() >= 255 {
            return Err(Error::new(line, "a struct must hold 1 to 16 numbers"));
        }
        if self.named(&name).is_some() {
            return Err(Error::new(line, format!("'{name}' is already a type")));
        }
        let ty = Ty::Struct(self.structs.len() as u8, size as u8);
        self.structs.push((name.clone(), ty));
        items.push(Item::Struct { name, fields });
        Ok(())
    }

    fn params(&mut self) -> Result<Vec<Param>, Error> {
        self.expect("(")?;
        let mut out = Vec::new();
        if self.eat(")") {
            return Ok(out);
        }
        if self.is_word("void") && matches!(self.peek_at(1), Some(Tok::Punct(")"))) {
            self.i += 2;
            return Ok(out);
        }
        loop {
            let (mut input, mut output) = (true, false);
            while let Some(Tok::Ident(s)) = self.peek() {
                match s.as_str() {
                    "in" | "const" => {}
                    "out" => (input, output) = (false, true),
                    "inout" => output = true,
                    _ => break,
                }
                self.i += 1;
            }
            let ty = self.ty()?;
            self.arrays_unsupported()?;
            let name = if matches!(self.peek(), Some(Tok::Ident(_))) {
                self.ident()?
            } else {
                String::new()
            };
            self.arrays_unsupported()?;
            out.push(Param {
                ty,
                name,
                input,
                output,
            });
            if self.eat(",") {
                continue;
            }
            self.expect(")")?;
            return Ok(out);
        }
    }

    fn block(&mut self) -> Result<Vec<St>, Error> {
        self.expect("{")?;
        let mut out = Vec::new();
        while !self.eat("}") {
            if self.done() {
                return Err(self.unexpected("'}'"));
            }
            out.push(self.statement()?);
        }
        Ok(out)
    }

    fn statement(&mut self) -> Result<St, Error> {
        let line = self.line();
        if self.is("{") {
            return Ok(St::Block(self.block()?));
        }
        if self.eat(";") {
            return Ok(St::Empty);
        }
        let word = match self.peek() {
            Some(Tok::Ident(w)) => w.clone(),
            _ => String::new(),
        };
        match word.as_str() {
            "if" => {
                self.i += 1;
                self.expect("(")?;
                let c = self.expression()?;
                self.expect(")")?;
                let then = Box::new(self.statement()?);
                let other = if self.is_word("else") {
                    self.i += 1;
                    Some(Box::new(self.statement()?))
                } else {
                    None
                };
                Ok(St::If(c, then, other))
            }
            "for" => {
                self.i += 1;
                self.expect("(")?;
                let init = if self.eat(";") {
                    None
                } else {
                    Some(Box::new(self.simple()?))
                };
                let cond = if self.is(";") {
                    None
                } else {
                    Some(self.expression()?)
                };
                self.expect(";")?;
                let step = if self.is(")") {
                    None
                } else {
                    Some(self.expression()?)
                };
                self.expect(")")?;
                let body = Box::new(self.statement()?);
                Ok(St::For {
                    init,
                    cond,
                    step,
                    body,
                })
            }
            "while" => {
                self.i += 1;
                self.expect("(")?;
                let c = self.expression()?;
                self.expect(")")?;
                Ok(St::While(c, Box::new(self.statement()?)))
            }
            "do" => {
                self.i += 1;
                let body = Box::new(self.statement()?);
                if !self.is_word("while") {
                    return Err(self.unexpected("'while'"));
                }
                self.i += 1;
                self.expect("(")?;
                let c = self.expression()?;
                self.expect(")")?;
                self.expect(";")?;
                Ok(St::Do(body, c))
            }
            "return" => {
                self.i += 1;
                let e = if self.is(";") {
                    None
                } else {
                    Some(self.expression()?)
                };
                self.expect(";")?;
                Ok(St::Return(e, line))
            }
            "break" | "continue" | "discard" => {
                self.i += 1;
                self.expect(";")?;
                Ok(match word.as_str() {
                    "break" => St::Break(line),
                    "continue" => St::Continue(line),
                    _ => St::Discard,
                })
            }
            _ => self.simple(),
        }
    }

    /// A declaration or an expression, and its ';'.
    fn simple(&mut self) -> Result<St, Error> {
        let line = self.line();
        let fixed = self.is_word("const");
        if fixed {
            self.i += 1;
        }
        if fixed || self.declaration_here() {
            let ty = self.ty()?;
            let typed = self.size()?;
            let mut vars = Vec::new();
            loop {
                let name = self.ident()?;
                let size = self.size()?.or_else(|| typed.clone());
                let init = if self.eat("=") {
                    Some(self.assignment()?)
                } else {
                    None
                };
                vars.push((name, size, init));
                if !self.eat(",") {
                    break;
                }
            }
            self.expect(";")?;
            return Ok(St::Decl {
                fixed,
                ty,
                vars,
                line,
            });
        }
        let e = self.expression()?;
        self.expect(";")?;
        Ok(St::Expr(e))
    }

    fn expression(&mut self) -> Result<E, Error> {
        let mut e = self.assignment()?;
        while self.is(",") {
            let line = self.line();
            self.i += 1;
            let r = self.assignment()?;
            e = E {
                kind: Ek::Comma(Box::new(e), Box::new(r)),
                line,
            };
        }
        Ok(e)
    }

    fn assignment(&mut self) -> Result<E, Error> {
        let target = self.conditional()?;
        if let Some(Tok::Punct(p)) = self.peek()
            && let Some(op) = ASSIGNMENTS.iter().find(|a| *a == p)
        {
            let line = self.line();
            self.i += 1;
            let value = self.assignment()?;
            return Ok(E {
                kind: Ek::Assign(op, Box::new(target), Box::new(value)),
                line,
            });
        }
        Ok(target)
    }

    fn conditional(&mut self) -> Result<E, Error> {
        let c = self.binary(0)?;
        if !self.is("?") {
            return Ok(c);
        }
        let line = self.line();
        self.i += 1;
        let a = self.expression()?;
        self.expect(":")?;
        let b = self.assignment()?;
        Ok(E {
            kind: Ek::Cond(Box::new(c), Box::new(a), Box::new(b)),
            line,
        })
    }

    fn binary(&mut self, level: usize) -> Result<E, Error> {
        if level == LEVELS.len() {
            return self.unary();
        }
        let mut e = self.binary(level + 1)?;
        while let Some(Tok::Punct(p)) = self.peek() {
            let Some(op) = LEVELS[level].iter().find(|o| *o == p) else {
                break;
            };
            let line = self.line();
            self.i += 1;
            let r = self.binary(level + 1)?;
            e = E {
                kind: Ek::Binary(op, Box::new(e), Box::new(r)),
                line,
            };
        }
        Ok(e)
    }

    fn unary(&mut self) -> Result<E, Error> {
        let line = self.line();
        if let Some(Tok::Punct(p)) = self.peek() {
            let p: &'static str = p;
            if matches!(p, "-" | "+" | "!" | "~") {
                self.i += 1;
                let e = self.unary()?;
                return Ok(E {
                    kind: Ek::Unary(p, Box::new(e)),
                    line,
                });
            }
            if matches!(p, "++" | "--") {
                self.i += 1;
                let e = self.unary()?;
                return Ok(E {
                    kind: Ek::Step(p, true, Box::new(e)),
                    line,
                });
            }
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<E, Error> {
        let mut e = self.primary()?;
        loop {
            let line = self.line();
            if self.eat(".") {
                let f = self.ident()?;
                // An array's `.length()`.
                if f == "length" && self.is("(") {
                    self.expect("(")?;
                    self.expect(")")?;
                    e = E {
                        kind: Ek::Length(Box::new(e)),
                        line,
                    };
                    continue;
                }
                e = E {
                    kind: Ek::Field(Box::new(e), f),
                    line,
                };
            } else if self.eat("[") {
                let i = self.expression()?;
                self.expect("]")?;
                e = E {
                    kind: Ek::Index(Box::new(e), Box::new(i)),
                    line,
                };
            } else if self.is("++") || self.is("--") {
                let op = if self.is("++") { "++" } else { "--" };
                self.i += 1;
                e = E {
                    kind: Ek::Step(op, false, Box::new(e)),
                    line,
                };
            } else {
                return Ok(e);
            }
        }
    }

    fn primary(&mut self) -> Result<E, Error> {
        let line = self.line();
        let kind = match self.peek().cloned() {
            Some(Tok::Float(v)) => {
                self.i += 1;
                Ek::Float(v)
            }
            Some(Tok::Int(v)) => {
                self.i += 1;
                Ek::Int(v)
            }
            Some(Tok::Ident(s)) => {
                self.i += 1;
                match s.as_str() {
                    "true" => Ek::Bool(true),
                    "false" => Ek::Bool(false),
                    _ if self.eat("(") => {
                        let mut args = Vec::new();
                        if self.is_word("void") && matches!(self.peek_at(1), Some(Tok::Punct(")")))
                        {
                            self.i += 1;
                        }
                        if !self.eat(")") {
                            loop {
                                args.push(self.assignment()?);
                                if self.eat(",") {
                                    continue;
                                }
                                self.expect(")")?;
                                break;
                            }
                        }
                        Ek::Call(s, args)
                    }
                    // An array's constructor.
                    _ if self.is("[") && Ty::named(&s).is_some() => {
                        let ty = Ty::named(&s).unwrap_or(Ty::FLOAT);
                        let size = self.size()?.flatten().map(Box::new);
                        self.expect("(")?;
                        let mut args = Vec::new();
                        if !self.eat(")") {
                            loop {
                                args.push(self.assignment()?);
                                if self.eat(",") {
                                    continue;
                                }
                                self.expect(")")?;
                                break;
                            }
                        }
                        Ek::Array(ty, size, args)
                    }
                    _ => Ek::Name(s),
                }
            }
            Some(Tok::Punct("(")) => {
                self.i += 1;
                let e = self.expression()?;
                self.expect(")")?;
                return Ok(e);
            }
            _ => return Err(self.unexpected("an expression")),
        };
        Ok(E { kind, line })
    }
}
