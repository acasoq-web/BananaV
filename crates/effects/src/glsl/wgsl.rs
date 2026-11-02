//! GLSL made into WGSL, for GL Transitions' code on the graphics card
//! (VEGAS compiles it there too): the syntax [`super::parse`] reads, its
//! types worked out here, made into a compute kernel writing each pixel of
//! `dst` from `transition(uv)` on the pictures `ta` (the first) and `tb`
//! (the second), all three `w` × `h`, rows from the top. The harness as
//! VEGAS gives it: `progress` and `ratio` (`P.f[0]`, `P.f[1]`), the
//! panel's controls (`P.f[2]` on, [`SLOTS`]' numbers one after another),
//! `getFromColor`, `getToColor` and `texture2D` on `fromImage` and
//! `toImage` — the pictures read as VEGAS's textures, bilinearly and
//! repeated past their edges —, the pixel held within 0–1. The same sums
//! as the interpreter's ([`super::compile`]): `round` halves away from
//! zero, `mod` by floor, loops cut after 2^20 turns.

use std::collections::HashMap;
use std::fmt::Write as _;

use super::lex;
use super::parse::{self, E, Ek, Item, Param, Qual, Size, St};
use super::{Base, Error, Ty};

/// The panel's controls a code's uniforms may be, and their types: their
/// numbers in `P.f` after `progress` and `ratio`, one after another.
pub(crate) const SLOTS: [(&str, Ty); 11] = [
    ("kParamFloat0", Ty::FLOAT),
    ("kParamFloat1", Ty::FLOAT),
    ("kParamFloat2", Ty::FLOAT),
    ("kParamFloat3", Ty::FLOAT),
    ("kParamInt0", Ty::INT),
    ("kParamInt1", Ty::INT),
    ("kParamBool0", Ty::BOOL),
    ("kParamVec20", Ty::VEC2),
    ("kParamIVec20", Ty::Vec(Base::Int, 2)),
    ("kParamVec40", Ty::VEC4),
    ("kParamVec41", Ty::VEC4),
];

/// Where slot `k`'s first number is in `P.f`.
pub(crate) fn slot_offset(k: usize) -> usize {
    2 + SLOTS[..k].iter().map(|(_, t)| t.size()).sum::<usize>()
}

/// How many loop turns a pixel may take, as the interpreter's.
const STEPS: u32 = 1 << 20;

/// The kernel's entry point.
pub(crate) const ENTRY: &str = "glt_main";

/// The harness's part of the kernel, before the code's.
const HARNESS: &str = "
@group(0) @binding(1) var<storage, read> ta: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> tb: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> dst: array<vec4<f32>>;

var<private> glt_progress: f32;
var<private> glt_ratio: f32;
var<private> glt_steps: u32;

// A texel of picture `k` (0 the first), `i` across and `j` up from the
// lower left, repeated past the edges.
fn glt_texel(k: u32, i: i32, j: i32) -> vec4<f32> {
    let w = i32(P.w);
    let h = i32(P.h);
    let x = ((i % w) + w) % w;
    let y = ((j % h) + h) % h;
    let o = u32(h - 1 - y) * P.w + u32(x);
    if k == 0u {
        return ta[o];
    }
    return tb[o];
}

// Picture `k` at `uv` (from the lower left): bilinear, repeated;
// nothing at no number or past ten million pixels.
fn glt_sample(k: u32, uv: vec2<f32>) -> vec4<f32> {
    let p = uv * vec2<f32>(f32(P.w), f32(P.h)) - 0.5;
    if !(abs(p.x) < 1.0e7 && abs(p.y) < 1.0e7) {
        return vec4<f32>(0.0);
    }
    let f0 = floor(p);
    let t = p - f0;
    let i = i32(f0.x);
    let j = i32(f0.y);
    let a = mix(glt_texel(k, i, j), glt_texel(k, i + 1, j), t.x);
    let b = mix(glt_texel(k, i, j + 1), glt_texel(k, i + 1, j + 1), t.x);
    return mix(a, b, t.y);
}

fn glt_from(uv: vec2<f32>) -> vec4<f32> {
    return glt_sample(0u, uv);
}

fn glt_to(uv: vec2<f32>) -> vec4<f32> {
    return glt_sample(1u, uv);
}

// GLSL's sine and cosine as the interpreter works them out (its `trig`):
// the same sums, the same digits.
fn glt_trig(x: f32, cosine: bool) -> f32 {
    var sign = 1.0;
    var a = x;
    if a < 0.0 {
        a = -a;
        if !cosine {
            sign = -1.0;
        }
    }
    var j = i32(1.2732395 * a);
    var y = f32(j);
    if (j & 1) != 0 {
        j += 1;
        y += 1.0;
    }
    j = j & 7;
    if j > 3 {
        sign = -sign;
        j -= 4;
    }
    if cosine && j > 1 {
        sign = -sign;
    }
    let r = ((a - y * 0.78515625) - y * 2.4187565e-4) - y * 3.774895e-8;
    let z = r * r;
    let s = ((-1.9515296e-4 * z + 8.332161e-3) * z - 0.16666655) * z * r + r;
    let c = ((2.4433157e-5 * z - 1.3887316e-3) * z + 4.1666646e-2) * z * z - 0.5 * z + 1.0;
    var v = s;
    if (j == 1 || j == 2) != cosine {
        v = c;
    }
    return sign * v;
}

fn glt_sin1(x: f32) -> f32 {
    return glt_trig(x, false);
}

fn glt_cos1(x: f32) -> f32 {
    return glt_trig(x, true);
}

fn glt_sin2(v: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(glt_sin1(v.x), glt_sin1(v.y));
}

fn glt_sin3(v: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(glt_sin1(v.x), glt_sin1(v.y), glt_sin1(v.z));
}

fn glt_sin4(v: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(glt_sin1(v.x), glt_sin1(v.y), glt_sin1(v.z), glt_sin1(v.w));
}

fn glt_cos2(v: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(glt_cos1(v.x), glt_cos1(v.y));
}

fn glt_cos3(v: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(glt_cos1(v.x), glt_cos1(v.y), glt_cos1(v.z));
}

fn glt_cos4(v: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(glt_cos1(v.x), glt_cos1(v.y), glt_cos1(v.z), glt_cos1(v.w));
}

// GLSL's `round` as the interpreter's: halves away from zero.
fn glt_round2(v: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(rround(v.x), rround(v.y));
}

fn glt_round3(v: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(rround(v.x), rround(v.y), rround(v.z));
}

fn glt_round4(v: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(rround(v.x), rround(v.y), rround(v.z), rround(v.w));
}
";

/// A variable as the kernel names it.
#[derive(Clone, Debug)]
struct Var {
    w: String,
    ty: Ty,
    len: Option<usize>,
    /// An `out` or `inout` parameter: read and written through.
    ptr: bool,
    /// The module's (`var<private>`): no `out` argument.
    global: bool,
}

/// A function as the kernel names it (overloads apart).
struct Func {
    name: String,
    params: Vec<Param>,
    ret: Ty,
    w: String,
    defined: bool,
}

/// An expression in WGSL and its type (an array's length too). `lit`: an
/// integer literal, which WGSL takes as a float where one is wanted.
#[derive(Clone, Debug)]
struct X {
    w: String,
    ty: Ty,
    len: Option<usize>,
    lit: bool,
}

impl X {
    fn new(w: String, ty: Ty) -> X {
        X {
            w,
            ty,
            len: None,
            lit: false,
        }
    }
}

fn err(line: usize, m: impl Into<String>) -> Error {
    Error::new(line, m)
}

/// A type in WGSL.
fn wty(t: Ty) -> Result<String, Error> {
    Ok(match t {
        Ty::Vec(Base::Float, 1) => "f32".into(),
        Ty::Vec(Base::Int, 1) => "i32".into(),
        Ty::Vec(Base::Bool, 1) => "bool".into(),
        Ty::Vec(Base::Float, n) => format!("vec{n}<f32>"),
        Ty::Vec(Base::Int, n) => format!("vec{n}<i32>"),
        Ty::Vec(Base::Bool, n) => format!("vec{n}<bool>"),
        Ty::Mat(n) => format!("mat{n}x{n}<f32>"),
        Ty::Struct(id, _) => format!("s_{id}"),
        Ty::Void | Ty::Sampler => return Err(err(0, "no such value on the card")),
    })
}

/// A variable's type in WGSL, an array's too.
fn wvar(t: Ty, len: Option<usize>) -> Result<String, Error> {
    Ok(match len {
        Some(n) => format!("array<{}, {n}>", wty(t)?),
        None => wty(t)?,
    })
}

/// A value of type `t` that is zero.
fn zero(t: Ty) -> Result<String, Error> {
    Ok(format!("{}()", wty(t)?))
}

fn base(t: Ty) -> Option<(Base, u8)> {
    match t {
        Ty::Vec(b, n) => Some((b, n)),
        _ => None,
    }
}

/// A float literal WGSL reads as the same float.
fn float(v: f32) -> String {
    let s = format!("{v:?}");
    if s.contains(['.', 'e', 'E']) {
        s
    } else {
        format!("{s}.0")
    }
}

/// A swizzle's letters as WGSL's (`stpq` as `xyzw`), and their indices.
fn swizzle_letters(field: &str) -> Option<(String, Vec<usize>)> {
    let sets = ["xyzw", "rgba", "stpq"];
    let set = sets.iter().find(|s| field.chars().all(|c| s.contains(c)))?;
    let idx: Vec<usize> = field.chars().map(|c| set.find(c).unwrap_or(0)).collect();
    if idx.is_empty() || idx.len() > 4 {
        return None;
    }
    Some((idx.iter().map(|&i| ['x', 'y', 'z', 'w'][i]).collect(), idx))
}

struct T {
    /// The code's structs: their names and fields.
    structs: Vec<(String, Vec<(String, Ty)>)>,
    globals: HashMap<String, Var>,
    scopes: Vec<HashMap<String, Var>>,
    funcs: Vec<Func>,
    /// Whole-number constants (arrays' sizes).
    consts: HashMap<String, i64>,
    ret: Ty,
    tmp: usize,
}

/// `src` (a GL Transition's code) as a WGSL kernel's source: the harness,
/// the code, and [`ENTRY`] (WGSL's prelude — `Params`, `P`, `rround` — goes
/// before it).
pub(crate) fn translate(src: &str) -> Result<String, Error> {
    let lexed = lex::lex(src)?;
    let items = parse::parse(&lexed)?;
    let mut t = T {
        structs: Vec::new(),
        globals: HashMap::new(),
        scopes: Vec::new(),
        funcs: Vec::new(),
        consts: HashMap::new(),
        ret: Ty::Void,
        tmp: 0,
    };
    let mut out = String::from(HARNESS);
    for item in &items {
        if let Item::Struct { name, fields } = item {
            let id = t.structs.len();
            writeln!(out, "\nstruct s_{id} {{").ok();
            for (k, (_, ty)) in fields.iter().enumerate() {
                writeln!(out, "    f{k}: {},", wty(*ty)?).ok();
            }
            out.push_str("}\n");
            t.structs.push((name.clone(), fields.clone()));
        }
    }
    // The functions first: calls may come before bodies.
    for item in &items {
        if let Item::Func {
            ret,
            name,
            params,
            body,
            ..
        } = item
        {
            match t
                .funcs
                .iter_mut()
                .find(|f| f.name == *name && same_params(&f.params, params))
            {
                Some(f) => f.defined |= body.is_some(),
                None => {
                    let w = format!("u_{}_{}", name, t.funcs.len());
                    t.funcs.push(Func {
                        name: name.clone(),
                        params: params.clone(),
                        ret: *ret,
                        w,
                        defined: body.is_some(),
                    });
                }
            }
        }
    }
    // The module's variables, and what the entry point sets first.
    let mut init = String::new();
    for item in &items {
        let Item::Global {
            qual,
            ty,
            name,
            size,
            init: value,
            line,
            ..
        } = item
        else {
            continue;
        };
        if matches!(ty, Ty::Void) {
            return Err(err(*line, "a variable cannot be void"));
        }
        if *ty == Ty::Sampler {
            if matches!(name.as_str(), "fromImage" | "toImage") {
                continue;
            }
            return Err(err(*line, "only the two pictures can be read"));
        }
        if matches!(name.as_str(), "progress" | "ratio") && *qual == Qual::Uniform {
            continue;
        }
        let len = t.size(size, value.as_ref(), *line)?;
        let w = format!("g_{name}");
        if *qual == Qual::Const
            && len.is_none()
            && let Some(n) = value.as_ref().and_then(|e| t.fold(e))
        {
            t.consts.insert(name.clone(), n);
        }
        writeln!(out, "var<private> {w}: {};", wvar(*ty, len)?).ok();
        match (qual, value) {
            (Qual::Uniform, _) => {
                if let Some(k) = SLOTS.iter().position(|(n, st)| n == name && st == ty) {
                    writeln!(init, "    {w} = {};", slot_value(k)?).ok();
                }
            }
            (_, Some(e)) => {
                let x = t.expr(e)?;
                let v = t.conv(&x, *ty, *line)?;
                writeln!(init, "    {w} = {v};").ok();
            }
            _ => {}
        }
        t.globals.insert(
            name.clone(),
            Var {
                w,
                ty: *ty,
                len,
                ptr: false,
                global: true,
            },
        );
    }
    for item in &items {
        if let Item::Func {
            ret,
            name,
            params,
            body: Some(body),
            line,
        } = item
        {
            let f = t
                .funcs
                .iter()
                .position(|f| f.name == *name && same_params(&f.params, params))
                .ok_or_else(|| err(*line, "no such function"))?;
            out.push_str(&t.function(f, *ret, params, body, *line)?);
        }
    }
    let main = t
        .funcs
        .iter()
        .find(|f| f.name == "transition" && f.params.len() == 1 && f.params[0].ty == Ty::VEC2)
        .filter(|f| f.ret == Ty::VEC4 && f.defined)
        .ok_or_else(|| err(1, "the code needs a function vec4 transition(vec2 uv)"))?;
    write!(
        out,
        "
@compute @workgroup_size(16, 16)
fn {ENTRY}(@builtin(global_invocation_id) id: vec3<u32>) {{
    if id.x >= P.w || id.y >= P.h {{
        return;
    }}
    glt_progress = P.f[0].x;
    glt_ratio = P.f[0].y;
{init}    let uv = vec2<f32>((f32(id.x) + 0.5) / f32(P.w), (f32(P.h - 1u - id.y) + 0.5) / f32(P.h));
    let c = {}(uv);
    // Within 0–1, no number as 0.
    dst[id.y * P.w + id.x] = select(vec4<f32>(0.0), min(c, vec4<f32>(1.0)), c >= vec4<f32>(0.0));
}}
",
        main.w
    )
    .ok();
    Ok(out)
}

fn same_params(a: &[Param], b: &[Param]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.ty == y.ty)
}

/// Slot `k`'s value from the parameters.
fn slot_value(k: usize) -> Result<String, Error> {
    let o = slot_offset(k);
    let f = |i: usize| format!("pf({}u)", o + i);
    let (_, ty) = SLOTS[k];
    Ok(match ty {
        Ty::Vec(Base::Float, 1) => f(0),
        Ty::Vec(Base::Int, 1) => format!("i32({})", f(0)),
        Ty::Vec(Base::Bool, 1) => format!("({} != 0.0)", f(0)),
        Ty::Vec(Base::Float, n) => format!(
            "vec{n}<f32>({})",
            (0..usize::from(n)).map(f).collect::<Vec<_>>().join(", ")
        ),
        Ty::Vec(Base::Int, n) => format!(
            "vec{n}<i32>({})",
            (0..usize::from(n))
                .map(|i| format!("i32({})", f(i)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => return Err(err(0, "no such control")),
    })
}

impl T {
    fn fold(&self, e: &E) -> Option<i64> {
        match &e.kind {
            Ek::Int(v) => Some(*v),
            Ek::Name(n) => self.consts.get(n).copied(),
            Ek::Unary("-", a) => Some(-self.fold(a)?),
            Ek::Binary(op, a, b) => {
                let (a, b) = (self.fold(a)?, self.fold(b)?);
                match *op {
                    "+" => Some(a + b),
                    "-" => Some(a - b),
                    "*" => Some(a * b),
                    "/" if b != 0 => Some(a / b),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// An array's length: its size, or its values'.
    fn size(&self, size: &Size, value: Option<&E>, line: usize) -> Result<Option<usize>, Error> {
        let Some(size) = size else { return Ok(None) };
        let n = match size {
            Some(e) => self
                .fold(e)
                .ok_or_else(|| err(line, "an array's size must be a constant"))?,
            None => match value.map(|e| &e.kind) {
                Some(Ek::Array(_, _, values)) => values.len() as i64,
                _ => return Err(err(line, "an array without a size needs values")),
            },
        };
        usize::try_from(n)
            .ok()
            .filter(|&n| n > 0)
            .map(Some)
            .ok_or_else(|| err(line, "an array's size must be positive"))
    }

    fn lookup(&self, name: &str) -> Option<Var> {
        self.scopes
            .iter()
            .rev()
            .find_map(|s| s.get(name))
            .or_else(|| self.globals.get(name))
            .cloned()
    }

    fn declare(&mut self, name: &str, ty: Ty, len: Option<usize>, ptr: bool) -> String {
        self.tmp += 1;
        let w = format!("l_{name}_{}", self.tmp);
        if let Some(s) = self.scopes.last_mut() {
            s.insert(
                name.to_string(),
                Var {
                    w: w.clone(),
                    ty,
                    len,
                    ptr,
                    global: false,
                },
            );
        }
        w
    }

    fn function(
        &mut self,
        f: usize,
        ret: Ty,
        params: &[Param],
        body: &[St],
        line: usize,
    ) -> Result<String, Error> {
        self.ret = ret;
        self.scopes = vec![HashMap::new()];
        let mut head = Vec::new();
        let mut copies = String::new();
        for (k, p) in params.iter().enumerate() {
            if p.ty == Ty::Sampler || p.ty == Ty::Void {
                return Err(err(line, "such parameters are not supported on the card"));
            }
            let name = if p.name.is_empty() {
                format!("unnamed{k}")
            } else {
                p.name.clone()
            };
            if p.output {
                let w = self.declare(&name, p.ty, None, true);
                head.push(format!("{w}: ptr<function, {}>", wty(p.ty)?));
            } else {
                let w = self.declare(&name, p.ty, None, false);
                head.push(format!("{w}_in: {}", wty(p.ty)?));
                writeln!(copies, "    var {w} = {w}_in;").ok();
            }
        }
        let ret_w = if ret == Ty::Void {
            String::new()
        } else {
            format!(" -> {}", wty(ret)?)
        };
        let mut s = format!(
            "\nfn {}({}){ret_w} {{\n{copies}",
            self.funcs[f].w,
            head.join(", ")
        );
        for st in body {
            s.push_str(&self.stmt(st, 1)?);
        }
        if ret != Ty::Void {
            writeln!(s, "    return {};", zero(ret)?).ok();
        }
        s.push_str("}\n");
        self.scopes.clear();
        Ok(s)
    }

    // Statements.

    fn stmt(&mut self, s: &St, depth: usize) -> Result<String, Error> {
        let pad = "    ".repeat(depth);
        Ok(match s {
            St::Decl {
                fixed,
                ty,
                vars,
                line,
            } => {
                let mut out = String::new();
                for (name, size, value) in vars {
                    let len = self.size(size, value.as_ref(), *line)?;
                    let v = match value {
                        Some(e) => {
                            let x = self.expr(e)?;
                            Some(if len.is_some() {
                                x.w
                            } else {
                                self.conv(&x, *ty, *line)?
                            })
                        }
                        None => None,
                    };
                    if *fixed
                        && len.is_none()
                        && let Some(n) = value.as_ref().and_then(|e| self.fold(e))
                    {
                        self.consts.insert(name.clone(), n);
                    }
                    let w = self.declare(name, *ty, len, false);
                    let t = wvar(*ty, len)?;
                    match v {
                        Some(v) => writeln!(out, "{pad}var {w}: {t} = {v};").ok(),
                        None => writeln!(out, "{pad}var {w}: {t};").ok(),
                    };
                }
                out
            }
            St::Expr(e) => format!("{pad}{};\n", self.effect(e)?),
            St::If(c, then, other) => {
                let c = self.cond(c)?;
                let mut out = format!("{pad}if {c} {{\n");
                out.push_str(&self.scoped(then, depth + 1)?);
                match other {
                    Some(o) => {
                        out.push_str(&format!("{pad}}} else {{\n"));
                        out.push_str(&self.scoped(o, depth + 1)?);
                        out.push_str(&format!("{pad}}}\n"));
                    }
                    None => out.push_str(&format!("{pad}}}\n")),
                }
                out
            }
            St::For {
                init,
                cond,
                step,
                body,
            } => {
                self.scopes.push(HashMap::new());
                let mut out = format!("{pad}{{\n");
                if let Some(i) = init {
                    out.push_str(&self.stmt(i, depth + 1)?);
                }
                let c = match cond {
                    Some(c) => self.cond(c)?,
                    None => "true".into(),
                };
                let inner = "    ".repeat(depth + 1);
                out.push_str(&format!("{inner}loop {{\n"));
                out.push_str(&format!(
                    "{inner}    if !{c} {{\n{inner}        break;\n{inner}    }}\n"
                ));
                out.push_str(&self.guard(depth + 2));
                out.push_str(&self.scoped(body, depth + 2)?);
                if let Some(st) = step {
                    out.push_str(&format!(
                        "{inner}    continuing {{\n{inner}        {};\n{inner}    }}\n",
                        self.effect(st)?
                    ));
                }
                out.push_str(&format!("{inner}}}\n{pad}}}\n"));
                self.scopes.pop();
                out
            }
            St::While(c, body) => {
                let c = self.cond(c)?;
                let mut out = format!(
                    "{pad}loop {{\n{pad}    if !{c} {{\n{pad}        break;\n{pad}    }}\n"
                );
                out.push_str(&self.guard(depth + 1));
                out.push_str(&self.scoped(body, depth + 1)?);
                out.push_str(&format!("{pad}}}\n"));
                out
            }
            St::Do(body, c) => {
                let mut out = format!("{pad}loop {{\n");
                out.push_str(&self.guard(depth + 1));
                out.push_str(&self.scoped(body, depth + 1)?);
                let c = self.cond(c)?;
                out.push_str(&format!(
                    "{pad}    continuing {{\n{pad}        break if !{c};\n{pad}    }}\n{pad}}}\n"
                ));
                out
            }
            St::Return(e, line) => match e {
                Some(e) => {
                    let x = self.expr(e)?;
                    let v = self.conv(&x, self.ret, *line)?;
                    format!("{pad}return {v};\n")
                }
                None => format!("{pad}return;\n"),
            },
            St::Break(_) => format!("{pad}break;\n"),
            St::Continue(_) => format!("{pad}continue;\n"),
            St::Discard => return Err(err(0, "discard is not supported on the card")),
            St::Block(b) => {
                self.scopes.push(HashMap::new());
                let mut out = format!("{pad}{{\n");
                for s in b {
                    out.push_str(&self.stmt(s, depth + 1)?);
                }
                out.push_str(&format!("{pad}}}\n"));
                self.scopes.pop();
                out
            }
            St::Empty => String::new(),
        })
    }

    /// A loop turn counted: past the interpreter's most, the loop ends.
    fn guard(&self, depth: usize) -> String {
        let pad = "    ".repeat(depth);
        format!(
            "{pad}glt_steps += 1u;\n{pad}if glt_steps > {STEPS}u {{\n{pad}    break;\n{pad}}}\n"
        )
    }

    /// A statement in a scope of its own (a block's statements in it).
    fn scoped(&mut self, s: &St, depth: usize) -> Result<String, Error> {
        self.scopes.push(HashMap::new());
        let out = match s {
            St::Block(b) => {
                let mut out = String::new();
                for s in b {
                    out.push_str(&self.stmt(s, depth)?);
                }
                out
            }
            _ => self.stmt(s, depth)?,
        };
        self.scopes.pop();
        Ok(out)
    }

    fn cond(&mut self, c: &E) -> Result<String, Error> {
        let x = self.expr(c)?;
        if x.ty != Ty::BOOL {
            return Err(err(c.line, "a condition must be a bool"));
        }
        Ok(format!("({})", x.w))
    }

    /// An expression used for what it does: an assignment, a step, a
    /// call.
    fn effect(&mut self, e: &E) -> Result<String, Error> {
        match &e.kind {
            Ek::Assign(op, target, value) => self.assign(op, target, value, e.line),
            Ek::Step(op, _, target) => {
                let place = self.place(target)?;
                let one = if base(place.ty).is_some_and(|(b, _)| b == Base::Float) {
                    "1.0"
                } else {
                    "1"
                };
                let op = if *op == "++" { "+=" } else { "-=" };
                Ok(format!("{} {op} {one}", place.w))
            }
            Ek::Comma(a, b) => Ok(format!("{};\n{}", self.effect(a)?, self.effect(b)?)),
            _ => {
                let x = self.expr(e)?;
                if x.ty == Ty::Void {
                    Ok(x.w)
                } else {
                    Ok(format!("_ = {}", x.w))
                }
            }
        }
    }

    fn assign(&mut self, op: &str, target: &E, value: &E, line: usize) -> Result<String, Error> {
        let v = self.expr(value)?;
        // A swizzle of more than one component: each apart.
        if let Ek::Field(inner, field) = &target.kind {
            let base_x = self.place(inner)?;
            if let (Ty::Vec(b, _), Some((letters, _))) = (base_x.ty, swizzle_letters(field))
                && letters.len() > 1
            {
                let n = letters.len() as u8;
                let ty = Ty::Vec(b, n);
                let v = self.conv(&v, ty, line)?;
                self.tmp += 1;
                let tmp = format!("t_{}", self.tmp);
                let mut out = format!("{{\n        let {tmp} = {v};\n");
                for (k, l) in letters.chars().enumerate() {
                    let c = ['x', 'y', 'z', 'w'][k];
                    let rhs = if op == "=" {
                        format!("{tmp}.{c}")
                    } else {
                        let o = &op[..op.len() - 1];
                        format!("{}.{l} {o} {tmp}.{c}", base_x.w)
                    };
                    writeln!(out, "        {}.{l} = {rhs};", base_x.w).ok();
                }
                out.push_str("    }");
                return Ok(out);
            }
        }
        let place = self.place(target)?;
        if op == "=" {
            let v = if place.len.is_some() {
                v.w
            } else {
                self.conv(&v, place.ty, line)?
            };
            return Ok(format!("{} = {v}", place.w));
        }
        let o = &op[..op.len() - 1];
        let bin = self.binary(
            o,
            X {
                w: place.w.clone(),
                ty: place.ty,
                len: None,
                lit: false,
            },
            v,
            line,
        )?;
        let v = self.conv(&bin, place.ty, line)?;
        Ok(format!("{} = {v}", place.w))
    }

    /// What may be assigned to.
    fn place(&mut self, e: &E) -> Result<X, Error> {
        match &e.kind {
            Ek::Name(n) => {
                let v = self
                    .lookup(n)
                    .ok_or_else(|| err(e.line, format!("'{n}' is not a variable")))?;
                let w = if v.ptr {
                    format!("(*{})", v.w)
                } else {
                    v.w.clone()
                };
                Ok(X {
                    w,
                    ty: v.ty,
                    len: v.len,
                    lit: false,
                })
            }
            Ek::Index(..) | Ek::Field(..) => self.expr(e),
            _ => Err(err(e.line, "this cannot be assigned to")),
        }
    }

    // Expressions.

    /// `x` as a value of type `to`: the same, or GLSL's implicit
    /// conversions (ints to floats).
    fn conv(&self, x: &X, to: Ty, line: usize) -> Result<String, Error> {
        if x.ty == to {
            return Ok(x.w.clone());
        }
        match (base(x.ty), base(to)) {
            (Some((Base::Int, n)), Some((Base::Float, m))) if n == m => {
                if x.lit {
                    Ok(x.w.clone())
                } else {
                    Ok(format!("{}({})", wty(to)?, x.w))
                }
            }
            _ => Err(err(
                line,
                format!("a {} cannot be a {}", x.ty.name(), to.name()),
            )),
        }
    }

    /// `x` as `to`'s kind of number (a constructor's argument).
    fn as_base(&self, x: &X, b: Base) -> Result<String, Error> {
        match x.ty {
            Ty::Vec(xb, _) if xb == b => Ok(x.w.clone()),
            Ty::Vec(_, n) => Ok(format!("{}({})", wty(Ty::Vec(b, n))?, x.w)),
            Ty::Mat(_) if b == Base::Float => Ok(x.w.clone()),
            _ => Err(err(0, "no such conversion")),
        }
    }

    fn expr(&mut self, e: &E) -> Result<X, Error> {
        let line = e.line;
        Ok(match &e.kind {
            Ek::Float(v) => X::new(float(*v), Ty::FLOAT),
            Ek::Int(v) => X {
                w: if *v < 0 {
                    format!("({v})")
                } else {
                    v.to_string()
                },
                ty: Ty::INT,
                len: None,
                lit: true,
            },
            Ek::Bool(b) => X::new(b.to_string(), Ty::BOOL),
            Ek::Name(n) => match n.as_str() {
                "progress" if self.lookup(n).is_none() => X::new("glt_progress".into(), Ty::FLOAT),
                "ratio" if self.lookup(n).is_none() => X::new("glt_ratio".into(), Ty::FLOAT),
                _ => self.place(e)?,
            },
            Ek::Call(name, args) => self.call(name, args, line)?,
            Ek::Field(inner, field) => {
                let x = self.expr(inner)?;
                if let Ty::Struct(id, _) = x.ty {
                    let fields = &self.structs[usize::from(id)].1;
                    let k = fields
                        .iter()
                        .position(|(n, _)| n == field)
                        .ok_or_else(|| err(line, format!("the struct has no '{field}'")))?;
                    return Ok(X::new(format!("({}).f{k}", x.w), fields[k].1));
                }
                match x.ty {
                    Ty::Vec(b, n) => {
                        let (letters, idx) = swizzle_letters(field)
                            .filter(|(_, idx)| idx.iter().all(|&i| i < usize::from(n)))
                            .ok_or_else(|| err(line, format!("no field '{field}'")))?;
                        let ty = Ty::Vec(b, idx.len() as u8);
                        if n == 1 {
                            // A scalar's swizzle: as many copies.
                            if idx.len() == 1 {
                                X::new(x.w, ty)
                            } else {
                                X::new(format!("{}({})", wty(ty)?, x.w), ty)
                            }
                        } else {
                            X::new(format!("({}).{letters}", x.w), ty)
                        }
                    }
                    _ => return Err(err(line, format!("no field '{field}'"))),
                }
            }
            Ek::Index(a, i) => {
                let x = self.expr(a)?;
                let i = self.expr(i)?;
                if base(i.ty) != Some((Base::Int, 1)) {
                    return Err(err(line, "an index must be an int"));
                }
                let ty = match (x.len, x.ty) {
                    (Some(_), t) => t,
                    (None, Ty::Vec(b, n)) if n > 1 => Ty::Vec(b, 1),
                    (None, Ty::Mat(n)) => Ty::Vec(Base::Float, n),
                    _ => return Err(err(line, "this cannot be indexed")),
                };
                X::new(format!("{}[{}]", x.w, i.w), ty)
            }
            Ek::Unary(op, a) => {
                let x = self.expr(a)?;
                match *op {
                    "-" => X {
                        w: format!("(-{})", x.w),
                        ..x
                    },
                    "+" => x,
                    "!" => X::new(format!("(!{})", x.w), x.ty),
                    "~" => X::new(format!("(~{})", x.w), x.ty),
                    _ => return Err(err(line, format!("no operator {op}"))),
                }
            }
            Ek::Binary(op, a, b) => {
                let (a, b) = (self.expr(a)?, self.expr(b)?);
                self.binary(op, a, b, line)?
            }
            Ek::Cond(c, a, b) => {
                let c = self.cond(c)?;
                let (a, b) = (self.expr(a)?, self.expr(b)?);
                let ty = if a.ty == b.ty {
                    a.ty
                } else if let Ok(w) = self.conv(&a, b.ty, line) {
                    return Ok(X::new(format!("select({}, {w}, {c})", b.w), b.ty));
                } else {
                    let w = self.conv(&b, a.ty, line)?;
                    return Ok(X::new(format!("select({w}, {}, {c})", a.w), a.ty));
                };
                X::new(format!("select({}, {}, {c})", b.w, a.w), ty)
            }
            Ek::Array(ty, _, values) => {
                let ws = values
                    .iter()
                    .map(|v| {
                        let x = self.expr(v)?;
                        self.conv(&x, *ty, line)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                X {
                    w: format!("array<{}, {}>({})", wty(*ty)?, ws.len(), ws.join(", ")),
                    ty: *ty,
                    len: Some(ws.len()),
                    lit: false,
                }
            }
            Ek::Length(a) => {
                let x = self.expr(a)?;
                let n = x.len.ok_or_else(|| err(line, "length() of no array"))?;
                X::new(n.to_string(), Ty::INT)
            }
            Ek::Assign(..) | Ek::Step(..) | Ek::Comma(..) => {
                return Err(err(line, "such an expression is not supported on the card"));
            }
        })
    }

    fn binary(&mut self, op: &str, a: X, b: X, line: usize) -> Result<X, Error> {
        // Ints with floats: floats.
        let (a, b) = match (base(a.ty), base(b.ty)) {
            (Some((Base::Int, n)), Some((Base::Float, _))) => {
                let to = Ty::Vec(Base::Float, n);
                (X::new(self.conv(&a, to, line)?, to), b)
            }
            (Some((Base::Float, _)), Some((Base::Int, n))) => {
                let to = Ty::Vec(Base::Float, n);
                let w = self.conv(&b, to, line)?;
                (a, X::new(w, to))
            }
            _ => (a, b),
        };
        let (ta, tb) = (a.ty, b.ty);
        let bad = || {
            err(
                line,
                format!("no operator {op} for {} and {}", ta.name(), tb.name()),
            )
        };
        Ok(match op {
            "+" | "-" | "*" | "/" | "%" => {
                let ty = match (ta, tb) {
                    (Ty::Mat(n), Ty::Vec(Base::Float, m)) if op == "*" && n == m => tb,
                    (Ty::Vec(Base::Float, m), Ty::Mat(n)) if op == "*" && n == m => ta,
                    (Ty::Mat(n), Ty::Mat(m)) if n == m && op != "/" && op != "%" => ta,
                    (Ty::Mat(_), Ty::Vec(Base::Float, 1)) if op == "*" => ta,
                    (Ty::Vec(Base::Float, 1), Ty::Mat(_)) if op == "*" => tb,
                    (Ty::Mat(_), Ty::Vec(Base::Float, 1)) if op == "/" => {
                        return Ok(X::new(format!("({} * (1.0 / {}))", a.w, b.w), ta));
                    }
                    (Ty::Vec(x, n), Ty::Vec(y, m))
                        if x == y && x != Base::Bool && (n == m || n == 1 || m == 1) =>
                    {
                        Ty::Vec(x, n.max(m))
                    }
                    _ => return Err(bad()),
                };
                X::new(format!("({} {op} {})", a.w, b.w), ty)
            }
            "<" | ">" | "<=" | ">=" => {
                if ta != tb || ta.size() != 1 || ta == Ty::BOOL {
                    return Err(bad());
                }
                X::new(format!("({} {op} {})", a.w, b.w), Ty::BOOL)
            }
            "==" | "!=" => {
                if ta != tb || matches!(ta, Ty::Mat(_)) || a.len.is_some() {
                    return Err(bad());
                }
                if ta.size() > 1 {
                    let f = if op == "==" { "all" } else { "any" };
                    X::new(format!("{f}({} {op} {})", a.w, b.w), Ty::BOOL)
                } else {
                    X::new(format!("({} {op} {})", a.w, b.w), Ty::BOOL)
                }
            }
            "&&" | "||" => {
                if ta != Ty::BOOL || tb != Ty::BOOL {
                    return Err(bad());
                }
                X::new(format!("({} {op} {})", a.w, b.w), Ty::BOOL)
            }
            "^^" => {
                if ta != Ty::BOOL || tb != Ty::BOOL {
                    return Err(bad());
                }
                X::new(format!("({} != {})", a.w, b.w), Ty::BOOL)
            }
            "&" | "|" | "^" => {
                if ta != tb || base(ta).map(|(b, _)| b) != Some(Base::Int) {
                    return Err(bad());
                }
                X::new(format!("({} {op} {})", a.w, b.w), ta)
            }
            "<<" | ">>" => {
                if base(ta).map(|(b, _)| b) != Some(Base::Int) {
                    return Err(bad());
                }
                let n = ta.size() as u8;
                let shift = if n == 1 {
                    "u32".to_string()
                } else {
                    format!("vec{n}<u32>")
                };
                X::new(format!("({} {op} {shift}({}))", a.w, b.w), ta)
            }
            _ => return Err(bad()),
        })
    }

    fn call(&mut self, name: &str, args: &[E], line: usize) -> Result<X, Error> {
        // The pictures.
        if matches!(name, "texture2D" | "texture") {
            let Some(Ek::Name(s)) = args.first().map(|a| &a.kind) else {
                return Err(err(line, "only fromImage and toImage can be read"));
            };
            let which = match s.as_str() {
                "fromImage" => "glt_from",
                "toImage" => "glt_to",
                _ => return Err(err(line, "only fromImage and toImage can be read")),
            };
            let uv = self.expr(args.get(1).ok_or_else(|| err(line, "no coordinates"))?)?;
            if uv.ty != Ty::VEC2 {
                return Err(err(line, "texture2D needs a vec2"));
            }
            return Ok(X::new(format!("{which}({})", uv.w), Ty::VEC4));
        }
        // A struct's.
        if let Some(id) = self.structs.iter().position(|(n, _)| n == name) {
            let fields = self.structs[id].1.clone();
            if fields.len() != args.len() {
                return Err(err(line, format!("'{name}' has {} fields", fields.len())));
            }
            let mut ws = Vec::new();
            for ((_, ty), a) in fields.iter().zip(args) {
                let x = self.expr(a)?;
                ws.push(self.conv(&x, *ty, line)?);
            }
            let size: usize = fields.iter().map(|(_, t)| t.size()).sum();
            return Ok(X::new(
                format!("s_{id}({})", ws.join(", ")),
                Ty::Struct(id as u8, size as u8),
            ));
        }
        // A type's constructor.
        if let Some(ty) = Ty::named(name)
            && ty != Ty::Void
            && ty != Ty::Sampler
        {
            let xs = args
                .iter()
                .map(|a| self.expr(a))
                .collect::<Result<Vec<_>, _>>()?;
            return self.construct(ty, &xs, line);
        }
        let xs = args
            .iter()
            .map(|a| self.expr(a))
            .collect::<Result<Vec<_>, _>>()?;
        // The code's own functions, by their parameters' types.
        let candidates: Vec<usize> = (0..self.funcs.len())
            .filter(|&k| self.funcs[k].name == name)
            .collect();
        if !candidates.is_empty() {
            let exact = candidates.iter().copied().find(|&k| {
                self.funcs[k].params.len() == xs.len()
                    && self.funcs[k]
                        .params
                        .iter()
                        .zip(&xs)
                        .all(|(p, x)| p.ty == x.ty)
            });
            let k = exact
                .or_else(|| {
                    candidates.iter().copied().find(|&k| {
                        self.funcs[k].params.len() == xs.len()
                            && self.funcs[k].params.iter().zip(&xs).all(|(p, x)| {
                                p.ty == x.ty || (!p.output && self.conv(x, p.ty, line).is_ok())
                            })
                    })
                })
                .ok_or_else(|| err(line, format!("no function '{name}' takes these")))?;
            let (params, w, ret) = (
                self.funcs[k].params.clone(),
                self.funcs[k].w.clone(),
                self.funcs[k].ret,
            );
            let mut ws = Vec::new();
            for ((p, x), a) in params.iter().zip(&xs).zip(args) {
                if p.output {
                    let Ek::Name(n) = &a.kind else {
                        return Err(err(line, "an out argument must be a variable on the card"));
                    };
                    let v = self
                        .lookup(n)
                        .ok_or_else(|| err(line, "no such variable"))?;
                    if v.global {
                        return Err(err(
                            line,
                            "an out argument must be a local variable on the card",
                        ));
                    }
                    ws.push(if v.ptr {
                        v.w.clone()
                    } else {
                        format!("&{}", v.w)
                    });
                } else {
                    ws.push(self.conv(x, p.ty, line)?);
                }
            }
            return Ok(X::new(format!("{w}({})", ws.join(", ")), ret));
        }
        self.builtin(name, xs, line)
    }

    fn construct(&mut self, ty: Ty, xs: &[X], line: usize) -> Result<X, Error> {
        let bad = || err(line, format!("no {} of these", ty.name()));
        Ok(match ty {
            Ty::Vec(_, 1) => {
                let x = xs.first().filter(|_| xs.len() == 1).ok_or_else(bad)?;
                let first = match x.ty {
                    Ty::Vec(_, 1) => x.w.clone(),
                    Ty::Vec(..) => format!("({}).x", x.w),
                    Ty::Mat(_) => format!("({})[0].x", x.w),
                    _ => return Err(bad()),
                };
                X::new(format!("{}({first})", wty(ty)?), ty)
            }
            Ty::Vec(b, n) => {
                if xs.len() == 1 {
                    let x = &xs[0];
                    match x.ty {
                        Ty::Vec(_, 1) => {
                            return Ok(X::new(
                                format!("{}({})", wty(ty)?, self.as_base(x, b)?),
                                ty,
                            ));
                        }
                        Ty::Vec(_, m) if m >= n => {
                            let letters: String =
                                ['x', 'y', 'z', 'w'][..usize::from(n)].iter().collect();
                            let part =
                                X::new(format!("({}).{letters}", x.w), Ty::Vec(x.ty.base(), n));
                            return Ok(X::new(
                                format!("{}({})", wty(ty)?, self.as_base(&part, b)?),
                                ty,
                            ));
                        }
                        _ => return Err(bad()),
                    }
                }
                let total: usize = xs.iter().map(|x| x.ty.size()).sum();
                if total < usize::from(n) || xs.iter().any(|x| matches!(x.ty, Ty::Mat(_))) {
                    return Err(bad());
                }
                let ws = xs
                    .iter()
                    .map(|x| self.as_base(x, b))
                    .collect::<Result<Vec<_>, _>>()?;
                X::new(format!("{}({})", wty(ty)?, ws.join(", ")), ty)
            }
            Ty::Mat(n) => {
                let n_us = usize::from(n);
                if xs.len() == 1 && xs[0].ty.size() == 1 {
                    let d = self.as_base(&xs[0], Base::Float)?;
                    let ws: Vec<String> = (0..n_us * n_us)
                        .map(|k| {
                            if k % (n_us + 1) == 0 {
                                d.clone()
                            } else {
                                "0.0".into()
                            }
                        })
                        .collect();
                    return Ok(X::new(format!("{}({})", wty(ty)?, ws.join(", ")), ty));
                }
                if xs.len() == 1 && matches!(xs[0].ty, Ty::Mat(m) if m == n) {
                    return Ok(xs[0].clone());
                }
                let total: usize = xs.iter().map(|x| x.ty.size()).sum();
                if total != n_us * n_us {
                    return Err(bad());
                }
                let ws = xs
                    .iter()
                    .map(|x| self.as_base(x, Base::Float))
                    .collect::<Result<Vec<_>, _>>()?;
                X::new(format!("{}({})", wty(ty)?, ws.join(", ")), ty)
            }
            _ => return Err(bad()),
        })
    }

    fn builtin(&mut self, name: &str, xs: Vec<X>, line: usize) -> Result<X, Error> {
        let types = xs
            .iter()
            .map(|x| x.ty.name())
            .collect::<Vec<_>>()
            .join(", ");
        let bad = || {
            err(
                line,
                format!("no function '{name}' takes ({types}) on the card"),
            )
        };
        let arg = |k: usize| xs.get(k).cloned().ok_or_else(bad);
        // Floats from ints, as GLSL's functions of floats take them.
        let floaty = |x: X| -> X {
            match x.ty {
                Ty::Vec(Base::Int, n) => {
                    let to = Ty::Vec(Base::Float, n);
                    X::new(format!("{}({})", wty(to).unwrap_or_default(), x.w), to)
                }
                _ => x,
            }
        };
        // `x` as wide as `like` (a scalar spread).
        let splat = |x: &X, like: Ty| -> Result<String, Error> {
            match (x.ty, like) {
                (Ty::Vec(_, 1), Ty::Vec(b, n)) if n > 1 => Ok(format!(
                    "vec{n}<{}>({})",
                    match b {
                        Base::Float => "f32",
                        Base::Int => "i32",
                        Base::Bool => "bool",
                    },
                    x.w
                )),
                _ => Ok(x.w.clone()),
            }
        };
        let one = |f: &str| -> Result<X, Error> {
            let x = floaty(arg(0)?);
            if xs.len() != 1 || base(x.ty).map(|(b, _)| b) != Some(Base::Float) {
                return Err(bad());
            }
            Ok(X::new(format!("{f}({})", x.w), x.ty))
        };
        Ok(match name {
            "getFromColor" | "getToColor" => {
                let uv = arg(0)?;
                if xs.len() != 1 || uv.ty != Ty::VEC2 {
                    return Err(bad());
                }
                let f = if name == "getFromColor" {
                    "glt_from"
                } else {
                    "glt_to"
                };
                X::new(format!("{f}({})", uv.w), Ty::VEC4)
            }
            "sin" | "cos" => {
                let x = floaty(arg(0)?);
                if xs.len() != 1 || base(x.ty).map(|(b, _)| b) != Some(Base::Float) {
                    return Err(bad());
                }
                X::new(format!("glt_{name}{}({})", x.ty.size(), x.w), x.ty)
            }
            "radians" | "degrees" | "tan" | "asin" | "acos" | "sinh" | "cosh" | "tanh"
            | "asinh" | "acosh" | "atanh" | "exp" | "log" | "exp2" | "log2" | "sqrt" | "floor"
            | "ceil" | "fract" | "trunc" | "normalize" => one(name)?,
            "inversesqrt" => one("inverseSqrt")?,
            "roundEven" => one("round")?,
            "round" => {
                let x = floaty(arg(0)?);
                match x.ty.size() {
                    1 => one("rround")?,
                    n => X::new(format!("glt_round{n}({})", x.w), x.ty),
                }
            }
            "atan" if xs.len() == 1 => one("atan")?,
            "atan" => {
                let (y, x) = (floaty(arg(0)?), floaty(arg(1)?));
                if y.ty != x.ty {
                    return Err(bad());
                }
                X::new(format!("atan2({}, {})", y.w, x.w), y.ty)
            }
            "abs" | "sign" => {
                let x = arg(0)?;
                if xs.len() != 1 || x.ty == Ty::BOOL || matches!(x.ty, Ty::Mat(_)) {
                    return Err(bad());
                }
                X::new(format!("{name}({})", x.w), x.ty)
            }
            "pow" => {
                let (a, b) = (floaty(arg(0)?), floaty(arg(1)?));
                if a.ty != b.ty {
                    return Err(bad());
                }
                X::new(format!("pow({}, {})", a.w, b.w), a.ty)
            }
            "mod" => {
                let (a, b) = (floaty(arg(0)?), floaty(arg(1)?));
                let b_w = splat(&b, a.ty)?;
                X::new(
                    format!("(({0}) - ({1}) * floor(({0}) / ({1})))", a.w, b_w),
                    a.ty,
                )
            }
            "min" | "max" => {
                let (a, b) = (arg(0)?, arg(1)?);
                let (a, b) = if base(a.ty).map(|(x, _)| x) != base(b.ty).map(|(x, _)| x) {
                    (floaty(a), floaty(b))
                } else {
                    (a, b)
                };
                X::new(format!("{name}({}, {})", a.w, splat(&b, a.ty)?), a.ty)
            }
            "clamp" => {
                let (x, lo, hi) = (arg(0)?, arg(1)?, arg(2)?);
                let float = [&x, &lo, &hi]
                    .iter()
                    .any(|v| base(v.ty).map(|(b, _)| b) == Some(Base::Float));
                let (x, lo, hi) = if float {
                    (floaty(x), floaty(lo), floaty(hi))
                } else {
                    (x, lo, hi)
                };
                X::new(
                    format!(
                        "min(max({}, {}), {})",
                        x.w,
                        splat(&lo, x.ty)?,
                        splat(&hi, x.ty)?
                    ),
                    x.ty,
                )
            }
            "mix" => {
                let (x, y, a) = (floaty(arg(0)?), floaty(arg(1)?), arg(2)?);
                if base(a.ty).map(|(b, _)| b) == Some(Base::Bool) {
                    X::new(format!("select({}, {}, {})", x.w, y.w, a.w), x.ty)
                } else {
                    let a = floaty(a);
                    X::new(format!("mix({}, {}, {})", x.w, y.w, a.w), x.ty)
                }
            }
            "step" => {
                let (edge, x) = (floaty(arg(0)?), floaty(arg(1)?));
                let ty = if x.ty.size() >= edge.ty.size() {
                    x.ty
                } else {
                    edge.ty
                };
                X::new(
                    format!("step({}, {})", splat(&edge, ty)?, splat(&x, ty)?),
                    ty,
                )
            }
            "smoothstep" => {
                let (e0, e1, x) = (floaty(arg(0)?), floaty(arg(1)?), floaty(arg(2)?));
                X::new(
                    format!(
                        "smoothstep({}, {}, {})",
                        splat(&e0, x.ty)?,
                        splat(&e1, x.ty)?,
                        x.w
                    ),
                    x.ty,
                )
            }
            "fma" => {
                let (a, b, c) = (floaty(arg(0)?), floaty(arg(1)?), floaty(arg(2)?));
                X::new(format!("(({}) * ({}) + ({}))", a.w, b.w, c.w), a.ty)
            }
            "length" => X::new(format!("length({})", floaty(arg(0)?).w), Ty::FLOAT),
            "distance" => X::new(
                format!("distance({}, {})", floaty(arg(0)?).w, floaty(arg(1)?).w),
                Ty::FLOAT,
            ),
            "dot" => {
                let (a, b) = (floaty(arg(0)?), floaty(arg(1)?));
                if a.ty.size() == 1 {
                    X::new(format!("({} * {})", a.w, b.w), Ty::FLOAT)
                } else {
                    X::new(format!("dot({}, {})", a.w, b.w), Ty::FLOAT)
                }
            }
            "cross" => X::new(
                format!("cross({}, {})", floaty(arg(0)?).w, floaty(arg(1)?).w),
                Ty::Vec(Base::Float, 3),
            ),
            "faceforward" => {
                let (n, i, r) = (floaty(arg(0)?), floaty(arg(1)?), floaty(arg(2)?));
                X::new(format!("faceForward({}, {}, {})", n.w, i.w, r.w), n.ty)
            }
            "reflect" => {
                let (i, n) = (floaty(arg(0)?), floaty(arg(1)?));
                X::new(format!("reflect({}, {})", i.w, n.w), i.ty)
            }
            "refract" => {
                let (i, n, e) = (floaty(arg(0)?), floaty(arg(1)?), floaty(arg(2)?));
                X::new(format!("refract({}, {}, {})", i.w, n.w, e.w), i.ty)
            }
            "transpose" | "determinant" => {
                let m = arg(0)?;
                let Ty::Mat(_) = m.ty else { return Err(bad()) };
                let ty = if name == "determinant" {
                    Ty::FLOAT
                } else {
                    m.ty
                };
                X::new(format!("{name}({})", m.w), ty)
            }
            "matrixCompMult" => {
                let (a, b) = (arg(0)?, arg(1)?);
                let Ty::Mat(n) = a.ty else { return Err(bad()) };
                let cols: Vec<String> = (0..n)
                    .map(|k| format!("({0})[{k}] * ({1})[{k}]", a.w, b.w))
                    .collect();
                X::new(format!("{}({})", wty(a.ty)?, cols.join(", ")), a.ty)
            }
            "lessThan" | "lessThanEqual" | "greaterThan" | "greaterThanEqual" | "equal"
            | "notEqual" => {
                let (a, b) = (arg(0)?, arg(1)?);
                let op = match name {
                    "lessThan" => "<",
                    "lessThanEqual" => "<=",
                    "greaterThan" => ">",
                    "greaterThanEqual" => ">=",
                    "equal" => "==",
                    _ => "!=",
                };
                let n = a.ty.size() as u8;
                X::new(format!("({} {op} {})", a.w, b.w), Ty::Vec(Base::Bool, n))
            }
            "any" | "all" => X::new(format!("{name}({})", arg(0)?.w), Ty::BOOL),
            "not" => {
                let a = arg(0)?;
                X::new(format!("(!{})", a.w), a.ty)
            }
            _ => return Err(err(line, format!("'{name}' is not known on the card"))),
        })
    }
}

impl Ty {
    /// A scalar's or a vector's kind of number.
    fn base(self) -> Base {
        match self {
            Ty::Vec(b, _) => b,
            _ => Base::Float,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The card's prelude, then the kernel: valid WGSL.
    pub(crate) fn check(code: &str) -> String {
        let w = translate(code).unwrap_or_else(|e| panic!("{e}"));
        let src = format!("{PRELUDE}\n{w}");
        let module = naga::front::wgsl::parse_str(&src)
            .unwrap_or_else(|e| panic!("{}\n{src}", e.emit_to_string(&src)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{e:?}\n{src}"));
        w
    }

    /// The card's own prelude (`Params`, `P`, `pf`, `rround`…).
    const PRELUDE: &str = include_str!("../../../gpu/src/shaders/prelude.wgsl");

    #[test]
    fn a_crossfade_is_a_kernel() {
        let w = check(
            "vec4 transition (vec2 uv) { return mix(getFromColor(uv), getToColor(uv), progress); }",
        );
        assert!(w.contains("fn glt_main"));
    }

    #[test]
    fn glsl_s_conveniences_become_wgsl_s() {
        check(
            "uniform float kParamFloat0; // = 0.5
             uniform vec4 kParamVec40;
             uniform ivec2 kParamIVec20;
             const float PI = 3.14159;
             float f(float x, out float y) { y = x * 2.0; return mod(x, 2.0); }
             vec4 transition(vec2 uv) {
               float y;
               float a = f(uv.x, y);
               vec3 c = vec3(1);
               c.xy = uv * 2;
               c.z += 1.0;
               int n = 0;
               for (int i = 0; i < 4; i++) { n += i; }
               vec2 q = vec2(kParamIVec20);
               bool b = uv == vec2(0.5) ? true : false;
               float s = clamp(uv.x, 0.0, 1.0) + step(0.5, uv).x + smoothstep(0.0, 1.0, uv).y;
               mat2 m = mat2(1.0);
               vec2 r = m * uv;
               float arr[3];
               arr[0] = 1.0;
               return vec4(c * a + y + float(n) + s + r.x + arr[0] + q.x, kParamFloat0) + kParamVec40 * (b ? 1.0 : 0.0) + texture2D(fromImage, uv);
             }",
        );
    }
}
