//! Writes `src/generated.rs` from the kernels in `kernels/*.wgsl`: each
//! kernel's Rust twin, so that the processor works the same math out as the
//! graphics card, from one text.
//!
//! The kernels are the functions named `k_…`, `fn(p: vec2<i32>) ->
//! vec4<f32>`. They and the functions they call are read with naga — wgpu's
//! own shader compiler — and written as Rust on `src/math.rs`' vectors.
//! What reads the pictures (`NATIVE`) is written by hand in `src/cpu.rs`.
//!
//! Run by `cargo test -p bv-banana --test generated` (which fails when the
//! file is out of date) and, with `BV_BANANA_WRITE=1`, to write it.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write as _;
use std::path::Path;

use naga::{
    BinaryOperator as Bin, Expression as Ex, Handle, Literal, MathFunction as Mf, Scalar,
    ScalarKind as Sk, Statement as St, TypeInner as Ti, UnaryOperator,
};

/// The functions that read the pictures, written by hand in `src/cpu.rs`.
const NATIVE: [&str; 6] = ["at0", "at1", "at2", "bil0", "bil1", "bil2"];

/// A family of kernels: a file of `kernels/`.
#[derive(Debug)]
pub struct Family {
    pub name: String,
    pub source: String,
    /// Its kernels' names, in the file's order, without their `k_`.
    pub kernels: Vec<String>,
}

/// The kernels' families, by name.
pub fn families(dir: &Path) -> Result<Vec<Family>, String> {
    let mut v = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for e in entries.flatten() {
        let path = e.path();
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if path.extension().is_none_or(|x| x != "wgsl") || name == "prelude" {
            continue;
        }
        let source = read(&path)?;
        let mut kernels = Vec::new();
        for line in source.lines() {
            if let Some(rest) = line.strip_prefix("fn k_")
                && let Some((k, _)) = rest.split_once('(')
            {
                kernels.push(k.to_string());
            }
        }
        v.push(Family {
            name,
            source,
            kernels,
        });
    }
    v.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(v)
}

/// A text file, its lines ending alike everywhere.
pub fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map(|s| s.replace("\r\n", "\n"))
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// A kernel's entry point on the card.
pub fn entry(kernel: &str) -> String {
    format!(
        "\n@compute @workgroup_size(16, 16)\nfn main_{kernel}(@builtin(global_invocation_id) id: vec3<u32>) {{\n    if id.x >= P.w || id.y >= P.h {{\n        return;\n    }}\n    dst[id.y * P.w + id.x] = k_{kernel}(vec2<i32>(i32(id.x), i32(id.y)));\n}}\n"
    )
}

/// `generated.rs`' text.
pub fn generate(dir: &Path) -> Result<String, String> {
    let prelude = read(&dir.join("prelude.wgsl"))?;
    let families = families(dir)?;
    let mut out = String::from(
        "//! The kernels on the processor: written by `gen/gen.rs` from\n//! `kernels/*.wgsl` (`BV_BANANA_WRITE=1 cargo test -p bv-banana --test\n//! generated`). Never edited by hand.\n#![allow(clippy::all, unused)]\n\nuse crate::cpu::*;\nuse crate::math::*;\nuse crate::graph::Kernel;\n\n",
    );
    let mut index = 0u16;
    let mut consts = String::new();
    let mut table = String::new();
    let mut modules = String::new();
    let mut shared: BTreeMap<String, String> = BTreeMap::new();
    for (f, family) in families.iter().enumerate() {
        let mut source = format!("{prelude}\n{}", family.source);
        for k in &family.kernels {
            source.push_str(&entry(k));
        }
        let module = naga::front::wgsl::parse_str(&source)
            .map_err(|e| format!("{}: {}", family.name, e.emit_to_string(&source)))?;
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::default(),
        )
        .validate(&module)
        .map_err(|e| format!("{}: {}", family.name, e.emit_to_string(&source)))?;
        let functions = Writer {
            module: &module,
            info: &info,
        }
        .family(&family.kernels)?;
        let mut body = String::new();
        for (name, text) in functions {
            if prelude.contains(&format!("\nfn {name}(")) {
                shared.insert(name, text);
            } else {
                body.push_str(&text);
            }
        }
        let _ = writeln!(
            modules,
            "mod {} {{\n    use super::shared::*;\n    use super::*;\n{body}}}\n",
            family.name
        );
        for k in &family.kernels {
            let _ = writeln!(
                consts,
                "pub const {}: Kernel = Kernel({index});",
                k.to_uppercase()
            );
            let _ = writeln!(
                table,
                "    Entry {{ name: \"{k}\", family: {f}, run: {}::k_{k} }},",
                family.name
            );
            index += 1;
        }
    }
    out.push_str("/// Each kernel's name, family and twin on the processor.\n#[derive(Clone, Copy)]\npub struct Entry {\n    pub name: &'static str,\n    pub family: usize,\n    pub run: fn(&C<'_>, I2) -> V4,\n}\n\n");
    let _ = writeln!(
        out,
        "/// The kernels' shared part.\npub const PRELUDE: &str = include_str!(\"../kernels/prelude.wgsl\");\n"
    );
    let _ = writeln!(
        out,
        "/// The families' names and sources.\npub const FAMILIES: [(&str, &str); {}] = [",
        families.len()
    );
    for f in &families {
        let _ = writeln!(
            out,
            "    (\"{0}\", include_str!(\"../kernels/{0}.wgsl\")),",
            f.name
        );
    }
    let _ = writeln!(
        out,
        "];\n\n/// Every kernel.\npub const ENTRIES: [Entry; {index}] = [\n{table}];\n"
    );
    let _ = writeln!(
        out,
        "/// The kernels, by name.\npub mod k {{\n    use super::Kernel;\n{}}}\n",
        consts
            .lines()
            .map(|l| format!("    {l}\n"))
            .collect::<String>()
    );
    let _ = writeln!(
        out,
        "/// What the kernels share.\nmod shared {{\n    use super::*;\n{}}}\n",
        shared.into_values().collect::<String>()
    );
    out.push_str(&modules);
    Ok(out)
}

struct Writer<'a> {
    module: &'a naga::Module,
    info: &'a naga::valid::ModuleInfo,
}

impl Writer<'_> {
    /// The Rust of a family's kernels and what they call.
    fn family(&self, kernels: &[String]) -> Result<BTreeMap<String, String>, String> {
        let names: BTreeMap<String, Handle<naga::Function>> = self
            .module
            .functions
            .iter()
            .filter_map(|(h, f)| Some((f.name.clone()?, h)))
            .collect();
        let mut wanted: Vec<String> = kernels.iter().map(|k| format!("k_{k}")).collect();
        let mut done = BTreeSet::new();
        let mut text = BTreeMap::new();
        while let Some(name) = wanted.pop() {
            if !done.insert(name.clone()) || NATIVE.contains(&name.as_str()) {
                continue;
            }
            let h = *names
                .get(&name)
                .ok_or_else(|| format!("no function {name}"))?;
            let f = &self.module.functions[h];
            let (code, calls) = self.function(f, h)?;
            text.insert(name, code);
            wanted.extend(calls);
        }
        Ok(text)
    }

    fn function(
        &self,
        f: &naga::Function,
        h: Handle<naga::Function>,
    ) -> Result<(String, BTreeSet<String>), String> {
        let name = f.name.clone().unwrap_or_default();
        let mut w = Fun {
            m: self.module,
            f,
            info: &self.info[h],
            emitted: HashSet::new(),
            bound: HashSet::new(),
            calls: BTreeSet::new(),
            loops: 0,
            out: String::new(),
        };
        w.find_bound();
        let args: String = f
            .arguments
            .iter()
            .enumerate()
            .map(|(i, a)| Ok(format!(", a{i}: {}", w.ty(a.ty)?)))
            .collect::<Result<String, String>>()?;
        let result = match &f.result {
            Some(r) => format!(" -> {}", w.ty(r.ty)?),
            None => String::new(),
        };
        let vis = if name.starts_with("k_") {
            "pub fn"
        } else {
            "#[inline]\n    pub fn"
        };
        let mut head = format!("    {vis} {name}(c: &C<'_>{args}){result} {{\n");
        for (lh, l) in f.local_variables.iter() {
            let init = match l.init {
                Some(e) => w.expr(e)?,
                None => w.zero(l.ty)?,
            };
            let _ = writeln!(
                head,
                "        let mut v{}: {} = {init};",
                lh.index(),
                w.ty(l.ty)?
            );
        }
        w.block(&f.body, 2, None)?;
        Ok((format!("{head}{}    }}\n\n", w.out), w.calls))
    }
}

struct Fun<'a> {
    m: &'a naga::Module,
    f: &'a naga::Function,
    info: &'a naga::valid::FunctionInfo,
    emitted: HashSet<Handle<Ex>>,
    /// Those written as variables though used once: their parts are taken
    /// one by one.
    bound: HashSet<Handle<Ex>>,
    calls: BTreeSet<String>,
    loops: usize,
    out: String,
}

/// A loop's labels: its own and its body's (a `continue` leaves the body,
/// the continuing part still runs).
#[derive(Clone, Copy)]
struct Loop(usize);

impl Fun<'_> {
    fn find_bound(&mut self) {
        for (_, e) in self.f.expressions.iter() {
            match e {
                Ex::Swizzle { vector, .. } => {
                    self.bound.insert(*vector);
                }
                Ex::Compose { components, .. } => {
                    for c in components {
                        if matches!(self.inner(*c), Ti::Vector { .. }) {
                            self.bound.insert(*c);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Whether an expression gets a variable: what reads a variable or the
    /// parameters, or is used more than once.
    fn kept(&self, e: Handle<Ex>) -> bool {
        matches!(self.f.expressions[e], Ex::Load { .. })
            || self.info[e].ref_count > 1
            || self.bound.contains(&e)
    }

    fn scalar(s: Scalar) -> Result<&'static str, String> {
        Ok(match s.kind {
            Sk::Float => "f32",
            Sk::Sint => "i32",
            Sk::Uint => "u32",
            Sk::Bool => "bool",
            _ => return Err(format!("scalar {s:?}")),
        })
    }

    fn ty(&self, t: Handle<naga::Type>) -> Result<String, String> {
        match &self.m.types[t].inner {
            Ti::Scalar(s) => Ok(Self::scalar(*s)?.into()),
            Ti::Vector { size, scalar } => {
                Ok(format!("Vn<{}, {}>", Self::scalar(*scalar)?, *size as u8))
            }
            Ti::Array { base, size, .. } => {
                let naga::ArraySize::Constant(n) = size else {
                    return Err("an array of no known size".into());
                };
                Ok(format!("[{}; {n}]", self.ty(*base)?))
            }
            other => Err(format!("type {other:?}")),
        }
    }

    fn zero(&self, t: Handle<naga::Type>) -> Result<String, String> {
        Ok(match &self.m.types[t].inner {
            Ti::Scalar(s) => match s.kind {
                Sk::Float => "0.0f32".into(),
                Sk::Bool => "false".into(),
                _ => format!("0{}", Self::scalar(*s)?),
            },
            Ti::Vector { size, scalar } => {
                let z = match scalar.kind {
                    Sk::Float => "0.0f32",
                    Sk::Bool => "false",
                    Sk::Sint => "0i32",
                    _ => "0u32",
                };
                format!("Vn([{z}; {}])", *size as u8)
            }
            Ti::Array { base, size, .. } => {
                let naga::ArraySize::Constant(n) = size else {
                    return Err("an array of no known size".into());
                };
                format!("[{}; {n}]", self.zero(*base)?)
            }
            other => return Err(format!("zero of {other:?}")),
        })
    }

    fn inner(&self, e: Handle<Ex>) -> &Ti {
        self.info[e].ty.inner_with(&self.m.types)
    }

    fn is_pointer(&self, e: Handle<Ex>) -> bool {
        matches!(self.inner(e), Ti::Pointer { .. } | Ti::ValuePointer { .. })
    }

    /// (how many components, the scalar's kind): one component for scalars.
    fn shape(&self, e: Handle<Ex>) -> Result<(u8, Sk), String> {
        match self.inner(e) {
            Ti::Scalar(s) => Ok((1, s.kind)),
            Ti::Vector { size, scalar } => Ok((*size as u8, scalar.kind)),
            other => Err(format!("shape of {other:?}")),
        }
    }

    fn literal(l: Literal) -> Result<String, String> {
        Ok(match l {
            Literal::F32(v) => format!("{v:?}f32"),
            Literal::I32(v) if v < 0 => format!("({v}i32)"),
            Literal::I32(v) => format!("{v}i32"),
            Literal::U32(v) => format!("{v}u32"),
            Literal::Bool(v) => format!("{v}"),
            other => return Err(format!("literal {other:?}")),
        })
    }

    /// A constant expression of the module.
    fn global(&self, e: Handle<Ex>) -> Result<String, String> {
        match &self.m.global_expressions[e] {
            Ex::Literal(l) => Self::literal(*l),
            Ex::Constant(c) => self.global(self.m.constants[*c].init),
            Ex::ZeroValue(t) => self.zero(*t),
            Ex::Splat { size, value } => {
                Ok(format!("Vn([{}; {}])", self.global(*value)?, *size as u8))
            }
            Ex::Compose { ty, components } => {
                let parts = components
                    .iter()
                    .map(|c| self.global(*c))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(", ");
                Ok(match &self.m.types[*ty].inner {
                    Ti::Vector { .. } => format!("Vn([{parts}])"),
                    _ => format!("[{parts}]"),
                })
            }
            other => Err(format!("constant {other:?}")),
        }
    }

    /// An expression where it is used.
    fn expr(&mut self, e: Handle<Ex>) -> Result<String, String> {
        if self.emitted.contains(&e) {
            return Ok(format!("e{}", e.index()));
        }
        match &self.f.expressions[e] {
            Ex::Literal(l) => Self::literal(*l),
            Ex::Constant(c) => {
                let init = self.m.constants[*c].init;
                self.global(init)
            }
            Ex::ZeroValue(t) => self.zero(*t),
            Ex::FunctionArgument(i) => Ok(format!("a{i}")),
            Ex::CallResult(_) => Ok(format!("e{}", e.index())),
            _ => self.value(e),
        }
    }

    /// A pointer's place.
    fn place(&mut self, e: Handle<Ex>) -> Result<String, String> {
        match &self.f.expressions[e] {
            Ex::LocalVariable(l) => Ok(format!("v{}", l.index())),
            Ex::GlobalVariable(g) => {
                let name = self.m.global_variables[*g].name.clone().unwrap_or_default();
                if name == "P" {
                    Ok("c.p".into())
                } else {
                    Err(format!("{name} read outside the functions written by hand"))
                }
            }
            Ex::AccessIndex { base, index } => {
                let (base, index) = (*base, *index);
                let place = self.place(base)?;
                let inner = self.pointee(base)?;
                Ok(match inner {
                    Pointee::Struct(t) => {
                        let Ti::Struct { members, .. } = &self.m.types[t].inner else {
                            return Err("a struct".into());
                        };
                        let field = members[index as usize].name.clone().unwrap_or_default();
                        format!("{place}.{field}")
                    }
                    // The parameters' rows are plain arrays.
                    Pointee::Vector if place.starts_with("c.p") => format!("{place}[{index}]"),
                    Pointee::Vector => format!("{place}.0[{index}]"),
                    Pointee::Array => format!("{place}[{index}]"),
                })
            }
            Ex::Access { base, index } => {
                let (base, index) = (*base, *index);
                let place = self.place(base)?;
                let i = self.expr(index)?;
                Ok(match self.pointee(base)? {
                    Pointee::Vector if place.starts_with("c.p") => {
                        format!("{place}[{i} as usize & 3]")
                    }
                    Pointee::Vector => format!("{place}.0[{i} as usize]"),
                    Pointee::Array => format!("{place}[{i} as usize]"),
                    Pointee::Struct(_) => return Err("a struct's member by number".into()),
                })
            }
            other => Err(format!("place {other:?}")),
        }
    }

    fn pointee(&self, e: Handle<Ex>) -> Result<Pointee, String> {
        match self.inner(e) {
            Ti::Pointer { base, .. } => Ok(match &self.m.types[*base].inner {
                Ti::Struct { .. } => Pointee::Struct(*base),
                Ti::Vector { .. } => Pointee::Vector,
                Ti::Array { .. } => Pointee::Array,
                other => return Err(format!("pointer to {other:?}")),
            }),
            Ti::ValuePointer { size: Some(_), .. } => Ok(Pointee::Vector),
            other => Err(format!("pointee of {other:?}")),
        }
    }

    /// An expression worked out.
    fn value(&mut self, e: Handle<Ex>) -> Result<String, String> {
        let expr = self.f.expressions[e].clone();
        Ok(match expr {
            Ex::Compose { ty, components } => match &self.m.types[ty].inner {
                Ti::Vector { .. } => {
                    let mut parts = Vec::new();
                    for c in components {
                        let (n, _) = self.shape(c)?;
                        let text = self.expr(c)?;
                        if n == 1 {
                            parts.push(text);
                        } else {
                            parts.extend((0..n).map(|i| format!("{text}.0[{i}]")));
                        }
                    }
                    format!("Vn([{}])", parts.join(", "))
                }
                _ => {
                    let parts = components
                        .into_iter()
                        .map(|c| self.expr(c))
                        .collect::<Result<Vec<_>, _>>()?;
                    format!("[{}]", parts.join(", "))
                }
            },
            Ex::Access { base, index } => {
                let b = self.expr(base)?;
                let i = self.expr(index)?;
                match self.inner(base) {
                    Ti::Vector { .. } => format!("{b}.0[{i} as usize]"),
                    _ => format!("{b}[{i} as usize]"),
                }
            }
            Ex::AccessIndex { base, index } => {
                let b = self.expr(base)?;
                match self.inner(base) {
                    Ti::Vector { .. } => format!("{b}.0[{index}]"),
                    _ => format!("{b}[{index}]"),
                }
            }
            Ex::Splat { size, value } => {
                format!("Vn([{}; {}])", self.expr(value)?, size as u8)
            }
            Ex::Swizzle {
                size,
                vector,
                pattern,
            } => {
                let v = self.expr(vector)?;
                let parts: Vec<String> = pattern[..size as usize]
                    .iter()
                    .map(|p| format!("{v}.0[{}]", *p as u8))
                    .collect();
                format!("Vn([{}])", parts.join(", "))
            }
            Ex::Load { pointer } => {
                let place = self.place(pointer)?;
                let vector = matches!(self.inner(e), Ti::Vector { .. });
                if vector && place.starts_with("c.p") {
                    format!("Vn({place})")
                } else {
                    place
                }
            }
            Ex::Unary { op, expr } => {
                let v = self.expr(expr)?;
                let (n, kind) = self.shape(expr)?;
                match (op, kind, n) {
                    (UnaryOperator::Negate, Sk::Float, _) => format!("(-{v})"),
                    (UnaryOperator::Negate, _, 1) => format!("{v}.wrapping_neg()"),
                    (UnaryOperator::Negate, _, _) => format!("{v}.map(|x| x.wrapping_neg())"),
                    (_, _, 1) => format!("(!{v})"),
                    _ => format!("{v}.map(|x| !x)"),
                }
            }
            Ex::Binary { op, left, right } => self.binary(op, left, right)?,
            Ex::Select {
                condition,
                accept,
                reject,
            } => {
                let (n, _) = self.shape(condition)?;
                let (c, a, r) = (
                    self.expr(condition)?,
                    self.expr(accept)?,
                    self.expr(reject)?,
                );
                if n == 1 {
                    format!("(if {c} {{ {a} }} else {{ {r} }})")
                } else {
                    format!("pick({c}, {a}, {r})")
                }
            }
            Ex::Relational { fun, argument } => {
                let a = self.expr(argument)?;
                match fun {
                    naga::RelationalFunction::All => format!("{a}.all()"),
                    naga::RelationalFunction::Any => format!("{a}.any()"),
                    other => return Err(format!("{other:?}")),
                }
            }
            Ex::Math {
                fun,
                arg,
                arg1,
                arg2,
                ..
            } => self.math(fun, arg, arg1, arg2)?,
            Ex::As {
                expr,
                kind,
                convert,
            } => {
                let v = self.expr(expr)?;
                let (n, from) = self.shape(expr)?;
                let one = |x: &str| -> Result<String, String> {
                    Ok(match (convert.is_some(), from, kind) {
                        (_, a, b) if a == b => x.to_string(),
                        (true, Sk::Bool, Sk::Float) => format!("({x} as u8 as f32)"),
                        (true, Sk::Bool, Sk::Sint) => format!("({x} as i32)"),
                        (true, Sk::Bool, Sk::Uint) => format!("({x} as u32)"),
                        (true, _, Sk::Float) => format!("({x} as f32)"),
                        (true, _, Sk::Sint) => format!("({x} as i32)"),
                        (true, _, Sk::Uint) => format!("({x} as u32)"),
                        (false, Sk::Float, Sk::Uint) => format!("{x}.to_bits()"),
                        (false, Sk::Uint, Sk::Float) => format!("f32::from_bits({x})"),
                        (false, Sk::Sint, Sk::Uint) => format!("({x} as u32)"),
                        (false, Sk::Uint, Sk::Sint) => format!("({x} as i32)"),
                        other => return Err(format!("cast {other:?}")),
                    })
                };
                if n == 1 {
                    one(&v)?
                } else {
                    format!("{v}.map(|x| {})", one("x")?)
                }
            }
            other => return Err(format!("expression {other:?}")),
        })
    }

    fn binary(&mut self, op: Bin, left: Handle<Ex>, right: Handle<Ex>) -> Result<String, String> {
        let (l, r) = (self.expr(left)?, self.expr(right)?);
        let ((nl, kind), (nr, _)) = (self.shape(left)?, self.shape(right)?);
        let sign = match op {
            Bin::Add => "+",
            Bin::Subtract => "-",
            Bin::Multiply => "*",
            Bin::Divide => "/",
            Bin::Modulo => "%",
            Bin::Equal => "==",
            Bin::NotEqual => "!=",
            Bin::Less => "<",
            Bin::LessEqual => "<=",
            Bin::Greater => ">",
            Bin::GreaterEqual => ">=",
            Bin::And => "&",
            Bin::ExclusiveOr => "^",
            Bin::InclusiveOr => "|",
            Bin::LogicalAnd => "&",
            Bin::LogicalOr => "|",
            Bin::ShiftLeft => "<<",
            Bin::ShiftRight => ">>",
        };
        let int = matches!(kind, Sk::Sint | Sk::Uint);
        if nl == 1 && nr == 1 {
            if int {
                let f = match op {
                    Bin::Add => Some("wrapping_add"),
                    Bin::Subtract => Some("wrapping_sub"),
                    Bin::Multiply => Some("wrapping_mul"),
                    Bin::ShiftLeft => Some("wrapping_shl"),
                    Bin::ShiftRight => Some("wrapping_shr"),
                    _ => None,
                };
                if let Some(f) = f {
                    return Ok(format!("{l}.{f}({r})"));
                }
                // WGSL: dividing by 0 leaves the number.
                match op {
                    Bin::Divide => return Ok(format!("idiv({l}, {r})")),
                    Bin::Modulo => return Ok(format!("imod({l}, {r})")),
                    _ => {}
                }
            }
            return Ok(format!("({l} {sign} {r})"));
        }
        if int {
            let f = match op {
                Bin::Add => "|a, b| a.wrapping_add(b)",
                Bin::Subtract => "|a, b| a.wrapping_sub(b)",
                Bin::Multiply => "|a, b| a.wrapping_mul(b)",
                Bin::ShiftLeft => "|a, b| a.wrapping_shl(b)",
                Bin::ShiftRight => "|a, b| a.wrapping_shr(b)",
                Bin::And => "|a, b| a & b",
                Bin::ExclusiveOr => "|a, b| a ^ b",
                Bin::InclusiveOr => "|a, b| a | b",
                Bin::Equal => "|a, b| a == b",
                Bin::NotEqual => "|a, b| a != b",
                Bin::Less => "|a, b| a < b",
                Bin::LessEqual => "|a, b| a <= b",
                Bin::Greater => "|a, b| a > b",
                Bin::GreaterEqual => "|a, b| a >= b",
                other => return Err(format!("whole numbers' {other:?}")),
            };
            let n = nl.max(nr);
            let l = if nl == 1 {
                format!("Vn([{l}; {n}])")
            } else {
                l
            };
            let r = if nr == 1 {
                format!("Vn([{r}; {n}])")
            } else {
                r
            };
            return Ok(format!("{l}.zip({r}, {f})"));
        }
        Ok(match op {
            Bin::Equal
            | Bin::NotEqual
            | Bin::Less
            | Bin::LessEqual
            | Bin::Greater
            | Bin::GreaterEqual => format!("{l}.zip({r}, |a, b| a {sign} b)"),
            Bin::And | Bin::LogicalAnd => format!("{l}.zip({r}, |a, b| a & b)"),
            Bin::InclusiveOr | Bin::LogicalOr => format!("{l}.zip({r}, |a, b| a | b)"),
            // A vector and a number either way round.
            _ if nl == 1 => format!("(Vn([{l}; {nr}]) {sign} {r})"),
            _ if nr == 1 => format!("({l} {sign} Vn([{r}; {nl}]))"),
            _ => format!("({l} {sign} {r})"),
        })
    }

    fn math(
        &mut self,
        fun: Mf,
        arg: Handle<Ex>,
        arg1: Option<Handle<Ex>>,
        arg2: Option<Handle<Ex>>,
    ) -> Result<String, String> {
        let (n, kind) = self.shape(arg)?;
        let a = self.expr(arg)?;
        let b = arg1.map(|x| self.expr(x)).transpose()?.unwrap_or_default();
        let c = arg2.map(|x| self.expr(x)).transpose()?.unwrap_or_default();
        let one = |f: &str| {
            if n == 1 {
                format!("{f}({a})")
            } else {
                format!("{a}.map({f})")
            }
        };
        let two = |f: &str| {
            if n == 1 {
                format!("{f}({a}, {b})")
            } else {
                format!("{a}.zip({b}, {f})")
            }
        };
        let three = |f: &str| {
            if n == 1 {
                format!("{f}({a}, {b}, {c})")
            } else {
                format!("{a}.zip3({b}, {c}, {f})")
            }
        };
        Ok(match (fun, kind) {
            (Mf::Abs, Sk::Float) => one("f32::abs"),
            (Mf::Abs, Sk::Sint) => one("i32::wrapping_abs"),
            (Mf::Min, Sk::Float) => two("f32::min"),
            (Mf::Max, Sk::Float) => two("f32::max"),
            (Mf::Min, Sk::Sint) => two("i32_min"),
            (Mf::Max, Sk::Sint) => two("i32_max"),
            (Mf::Min, Sk::Uint) => two("u32_min"),
            (Mf::Max, Sk::Uint) => two("u32_max"),
            (Mf::Clamp, Sk::Float) => three("clampf"),
            (Mf::Clamp, Sk::Sint) => three("clampi"),
            (Mf::Clamp, Sk::Uint) => three("clampu"),
            (Mf::Saturate, _) => one("saturate"),
            (Mf::Cos, _) => one("f32::cos"),
            (Mf::Sin, _) => one("f32::sin"),
            (Mf::Tan, _) => one("f32::tan"),
            (Mf::Tanh, _) => one("f32::tanh"),
            (Mf::Acos, _) => one("f32::acos"),
            (Mf::Asin, _) => one("f32::asin"),
            (Mf::Atan, _) => one("f32::atan"),
            (Mf::Atan2, _) => two("f32::atan2"),
            (Mf::Ceil, _) => one("f32::ceil"),
            (Mf::Floor, _) => one("f32::floor"),
            (Mf::Trunc, _) => one("f32::trunc"),
            (Mf::Fract, _) => one("fract"),
            (Mf::Exp, _) => one("f32::exp"),
            (Mf::Exp2, _) => one("f32::exp2"),
            (Mf::Log, _) => one("f32::ln"),
            (Mf::Log2, _) => one("f32::log2"),
            (Mf::Pow, _) => two("f32::powf"),
            (Mf::Sqrt, _) => one("f32::sqrt"),
            (Mf::InverseSqrt, _) => one("inverse_sqrt"),
            (Mf::Sign, Sk::Float) => one("signf"),
            (Mf::Sign, Sk::Sint) => one("i32::signum"),
            (Mf::Step, _) => two("step"),
            (Mf::SmoothStep, _) => three("smoothstep"),
            (Mf::Mix, _) => {
                let scalar = arg2.is_some_and(|x| matches!(self.shape(x), Ok((1, _))));
                if n > 1 && scalar {
                    format!("{a}.zip({b}, |x, y| mix(x, y, {c}))")
                } else {
                    three("mix")
                }
            }
            (Mf::Dot, _) => format!("{a}.dot({b})"),
            (Mf::Cross, _) => format!("cross({a}, {b})"),
            (Mf::Distance, _) if n > 1 => format!("({a} - {b}).length()"),
            (Mf::Length, _) if n > 1 => format!("{a}.length()"),
            (Mf::Normalize, _) if n > 1 => format!("{a}.normalize()"),
            (Mf::Reflect, _) if n > 1 => format!("reflect({a}, {b})"),
            other => return Err(format!("function {other:?}")),
        })
    }

    fn line(&mut self, depth: usize, text: &str) {
        for _ in 0..depth {
            self.out.push_str("    ");
        }
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn block(
        &mut self,
        block: &naga::Block,
        depth: usize,
        inside: Option<Loop>,
    ) -> Result<(), String> {
        for st in block.iter() {
            match st {
                St::Emit(range) => {
                    for e in range.clone() {
                        if self.is_pointer(e) || self.info[e].ref_count == 0 || !self.kept(e) {
                            continue;
                        }
                        let v = self.value(e)?;
                        self.emitted.insert(e);
                        self.line(depth, &format!("let e{} = {v};", e.index()));
                    }
                }
                St::Block(b) => self.block(b, depth, inside)?,
                St::If {
                    condition,
                    accept,
                    reject,
                } => {
                    let c = self.expr(*condition)?;
                    self.line(depth, &format!("if {c} {{"));
                    self.block(accept, depth + 1, inside)?;
                    if !reject.is_empty() {
                        self.line(depth, "} else {");
                        self.block(reject, depth + 1, inside)?;
                    }
                    self.line(depth, "}");
                }
                St::Loop {
                    body,
                    continuing,
                    break_if,
                } => {
                    let l = Loop(self.loops);
                    self.loops += 1;
                    self.line(depth, &format!("'l{0}: loop {{", l.0));
                    self.line(depth + 1, &format!("'b{0}: {{", l.0));
                    self.block(body, depth + 2, Some(l))?;
                    self.line(depth + 1, "}");
                    self.block(continuing, depth + 1, Some(l))?;
                    if let Some(b) = break_if {
                        let b = self.expr(*b)?;
                        self.line(depth + 1, &format!("if {b} {{ break 'l{}; }}", l.0));
                    }
                    self.line(depth, "}");
                }
                St::Break => {
                    let l = inside.ok_or("a break outside a loop")?;
                    self.line(depth, &format!("break 'l{};", l.0));
                }
                St::Continue => {
                    let l = inside.ok_or("a continue outside a loop")?;
                    self.line(depth, &format!("break 'b{};", l.0));
                }
                St::Return { value } => match value {
                    Some(v) => {
                        let v = self.expr(*v)?;
                        self.line(depth, &format!("return {v};"));
                    }
                    None => self.line(depth, "return;"),
                },
                St::Store { pointer, value } => {
                    let place = self.place(*pointer)?;
                    let v = self.expr(*value)?;
                    self.line(depth, &format!("{place} = {v};"));
                }
                St::Call {
                    function,
                    arguments,
                    result,
                } => {
                    let name = self.m.functions[*function].name.clone().unwrap_or_default();
                    let args = arguments
                        .iter()
                        .map(|a| Ok(format!(", {}", self.expr(*a)?)))
                        .collect::<Result<String, String>>()?;
                    self.calls.insert(name.clone());
                    match result {
                        Some(r) => {
                            self.emitted.insert(*r);
                            self.line(depth, &format!("let e{} = {name}(c{args});", r.index()));
                        }
                        None => self.line(depth, &format!("{name}(c{args});")),
                    }
                }
                other => return Err(format!("statement {other:?}")),
            }
        }
        Ok(())
    }
}

enum Pointee {
    Struct(Handle<naga::Type>),
    Vector,
    Array,
}
