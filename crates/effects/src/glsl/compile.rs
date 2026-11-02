//! GLSL made into closures over a block of values: each variable has a
//! place of its own (GLSL has no recursion, so a function's parameters
//! and locals are never needed twice at once), each value 16 floats — a
//! scalar, a vector or a column-major matrix; ints and bools as floats —,
//! its type known here.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use super::lex;
use super::parse::{self, E, Ek, Item, Param, Qual, St};
use super::{Base, Error, Ty};

/// A value: a scalar, a vector or a matrix (column-major).
pub(crate) type Val = [f32; 16];

const ZERO: Val = [0.0; 16];

/// How many loop turns a pixel may take (a loop that never ends would
/// hold the render forever).
const STEPS: u32 = 1 << 20;

/// The most parameters a function may have.
const MAX_PARAMS: usize = 8;

/// The picture `which` (0 the first, 1 the second) at `uv` (GLSL's, from
/// the lower left).
pub(crate) type Sample<'a> = &'a dyn Fn(u8, [f32; 2]) -> [f32; 4];

/// What the code runs on: every variable's value, the value being
/// returned, and the pictures.
pub(crate) struct Env<'a> {
    slots: &'a mut [Val],
    ret: Val,
    discarded: bool,
    steps: u32,
    sample: Sample<'a>,
}

type Ev = Arc<dyn Fn(&mut Env<'_>) -> Val + Send + Sync>;
type Set = Arc<dyn Fn(&mut Env<'_>, Val) + Send + Sync>;
type Run = Arc<dyn Fn(&mut Env<'_>) -> Flow + Send + Sync>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Flow {
    Next,
    Break,
    Continue,
    Return,
}

/// An expression and its type.
struct Te {
    ty: Ty,
    ev: Ev,
}

/// What can be assigned to.
struct Place {
    ty: Ty,
    get: Ev,
    set: Set,
}

#[derive(Clone, Copy, Debug)]
struct Var {
    slot: usize,
    ty: Ty,
    fixed: bool,
    /// An array's length: its elements in the places from `slot` on.
    len: Option<usize>,
}

struct Func {
    name: String,
    ret: Ty,
    params: Vec<(Param, usize)>,
    body: Arc<OnceLock<Run>>,
    line: usize,
}

/// A struct's fields: each one's name, type and first number.
type Fields = Vec<(String, Ty, usize)>;

/// A uniform the code declares: its name, its type and its default (GL
/// Transitions' `// = …`, else zero).
#[derive(Clone, Debug)]
pub(crate) struct Uniform {
    pub(crate) name: String,
    pub(crate) ty: Ty,
    pub(crate) default: Val,
    slot: usize,
}

/// The harness's values for a frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Inputs<'a> {
    pub(crate) progress: f32,
    pub(crate) ratio: f32,
    /// The uniforms' values, in [`Program::uniforms`]' order (their
    /// defaults where missing).
    pub(crate) values: &'a [Val],
}

/// Code compiled: what it declares, and its `transition`.
pub(crate) struct Program {
    slots: usize,
    init: Run,
    main: Run,
    uv: usize,
    builtins: [usize; 4],
    pub(crate) uniforms: Vec<Uniform>,
}

impl std::fmt::Debug for Program {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Program")
            .field("slots", &self.slots)
            .field("uniforms", &self.uniforms)
            .finish()
    }
}

/// GL Transitions' harness, as VEGAS's: its uniforms, then the two
/// pictures.
const HARNESS: [(&str, Ty); 4] = [
    ("progress", Ty::FLOAT),
    ("ratio", Ty::FLOAT),
    ("fromImage", Ty::Sampler),
    ("toImage", Ty::Sampler),
];

impl Program {
    /// Compiles `src`: a GL Transition's code, its `vec4 transition(vec2
    /// uv)`.
    pub(crate) fn compile(src: &str) -> Result<Program, Error> {
        let lexed = lex::lex(src)?;
        let items = parse::parse(&lexed)?;
        let mut c = Compiler::new();
        for item in &items {
            if let Item::Struct { name, fields } = item {
                let mut at = 0;
                let fields = fields
                    .iter()
                    .map(|(n, t)| {
                        at += t.size();
                        (n.clone(), *t, at - t.size())
                    })
                    .collect();
                c.structs.push((name.clone(), fields));
            }
        }
        let mut builtins = [0; 4];
        for (b, (name, ty)) in builtins.iter_mut().zip(HARNESS) {
            *b = c.declare(name, ty, true, 0)?;
        }
        // Every function's signature first: calls may come before bodies.
        for item in &items {
            if let Item::Func {
                ret,
                name,
                params,
                line,
                ..
            } = item
            {
                c.declare_func(*ret, name, params, *line)?;
            }
        }
        let mut init = Vec::new();
        let mut uniforms = Vec::new();
        for item in &items {
            match item {
                Item::Global {
                    qual: Qual::Uniform,
                    size: Some(_),
                    line,
                    ..
                } => {
                    return Err(Error::new(*line, "uniform arrays are not supported yet"));
                }
                Item::Global {
                    qual: Qual::Uniform,
                    ty,
                    name,
                    default,
                    line,
                    ..
                } => {
                    // The harness's own, declared again.
                    if let Some(v) = c.scopes[0].get(name.as_str())
                        && v.fixed
                        && v.ty == *ty
                        && HARNESS.iter().any(|(n, _)| *n == name.as_str())
                    {
                        continue;
                    }
                    match ty {
                        Ty::Sampler => {
                            return Err(Error::new(
                                *line,
                                "only the two pictures can be read: use getFromColor and getToColor",
                            ));
                        }
                        Ty::Void => return Err(Error::new(*line, "a uniform cannot be void")),
                        _ => {}
                    }
                    let default = default
                        .as_deref()
                        .and_then(|d| c.constant(d, *ty))
                        .unwrap_or(ZERO);
                    let slot = c.declare(name, *ty, true, *line)?;
                    uniforms.push(Uniform {
                        name: name.clone(),
                        ty: *ty,
                        default,
                        slot,
                    });
                }
                Item::Global {
                    qual,
                    ty,
                    name,
                    size: Some(size),
                    init: value,
                    line,
                    ..
                } => {
                    let fixed = *qual == Qual::Const;
                    init.extend(c.array(name, *ty, size.as_ref(), value.as_ref(), fixed, *line)?);
                }
                Item::Global {
                    qual,
                    ty,
                    name,
                    init: value,
                    line,
                    ..
                } => {
                    if matches!(ty, Ty::Void | Ty::Sampler) {
                        return Err(Error::new(
                            *line,
                            format!("a variable cannot be a {}", ty.name()),
                        ));
                    }
                    if *qual == Qual::Const
                        && let Some(n) = value.as_ref().and_then(|e| c.fold(e))
                    {
                        c.consts.insert(name.clone(), n);
                    }
                    let value = match value {
                        Some(e) => Some(c.expr_as(e, *ty)?),
                        None if *qual == Qual::Const => {
                            return Err(Error::new(*line, format!("const '{name}' needs a value")));
                        }
                        None => None,
                    };
                    let slot = c.declare(name, *ty, *qual == Qual::Const, *line)?;
                    init.push((slot, value));
                }
                Item::Func {
                    name,
                    params,
                    body: Some(body),
                    line,
                    ..
                } => c.define_func(name, params, body, *line)?,
                Item::Func { body: None, .. } | Item::Struct { .. } => {}
            }
        }
        c.check_calls()?;
        let main = c
            .find_exact("transition", &[Ty::VEC2])
            .filter(|&i| c.funcs[i].ret == Ty::VEC4)
            .ok_or_else(|| Error::new(1, "the code needs a function vec4 transition(vec2 uv)"))?;
        let f = &c.funcs[main];
        if f.body.get().is_none() {
            return Err(Error::new(
                f.line,
                "'transition' is declared but never defined",
            ));
        }
        let uv = f.params[0].1;
        let body = f.body.clone();
        let main: Run = Arc::new(move |e| match body.get() {
            Some(run) => run(e),
            None => Flow::Next,
        });
        let init: Run = Arc::new(move |e| {
            for (slot, value) in &init {
                e.slots[*slot] = match value {
                    Some(v) => v(e),
                    None => ZERO,
                };
            }
            Flow::Next
        });
        Ok(Program {
            slots: c.slots,
            init,
            main,
            uv,
            builtins,
            uniforms,
        })
    }

    /// The pixel at `uv` (GLSL's, from the lower left): what `transition`
    /// gives, on the pictures `sample` reads. `slots` is room to work in,
    /// kept between pixels.
    pub(crate) fn run(
        &self,
        slots: &mut Vec<Val>,
        inputs: &Inputs<'_>,
        sample: Sample<'_>,
        uv: [f32; 2],
    ) -> [f32; 4] {
        if slots.len() != self.slots {
            slots.clear();
            slots.resize(self.slots, ZERO);
        }
        let [progress, ratio, from, to] = self.builtins;
        slots[progress] = scalar(inputs.progress);
        slots[ratio] = scalar(inputs.ratio);
        slots[from] = scalar(0.0);
        slots[to] = scalar(1.0);
        for (i, u) in self.uniforms.iter().enumerate() {
            slots[u.slot] = inputs.values.get(i).copied().unwrap_or(u.default);
        }
        let mut env = Env {
            slots: slots.as_mut_slice(),
            ret: ZERO,
            discarded: false,
            steps: 0,
            sample,
        };
        (self.init)(&mut env);
        let mut at = ZERO;
        at[0] = uv[0];
        at[1] = uv[1];
        env.slots[self.uv] = at;
        env.ret = ZERO;
        (self.main)(&mut env);
        if env.discarded {
            return [0.0; 4];
        }
        [env.ret[0], env.ret[1], env.ret[2], env.ret[3]]
    }
}

// ---- Values -----------------------------------------------------------------------

fn scalar(x: f32) -> Val {
    let mut v = ZERO;
    v[0] = x;
    v
}

fn truth(b: bool) -> f32 {
    if b { 1.0 } else { 0.0 }
}

fn constant(ty: Ty, v: Val) -> Te {
    Te {
        ty,
        ev: Arc::new(move |_| v),
    }
}

/// `te` as type `to`: itself, or ints taken as floats (GLSL 3's implicit
/// conversion).
fn coerce(te: Te, to: Ty, line: usize) -> Result<Ev, Error> {
    match (te.ty, to) {
        (a, b) if a == b => Ok(te.ev),
        (Ty::Vec(Base::Int, n), Ty::Vec(Base::Float, m)) if n == m => Ok(te.ev),
        (a, b) => Err(Error::new(
            line,
            format!("a {} cannot be used as a {}", a.name(), b.name()),
        )),
    }
}

/// The type two values may both be taken as.
fn unify(a: Ty, b: Ty) -> Option<Ty> {
    match (a, b) {
        _ if a == b => Some(a),
        (Ty::Vec(Base::Int, n), Ty::Vec(Base::Float, m))
        | (Ty::Vec(Base::Float, n), Ty::Vec(Base::Int, m))
            if n == m =>
        {
            Some(Ty::Vec(Base::Float, n))
        }
        _ => None,
    }
}

/// A scalar or vector of numbers: its base and size.
fn numeric(t: Ty) -> Option<(Base, usize)> {
    match t {
        Ty::Vec(b @ (Base::Float | Base::Int), n) => Some((b, usize::from(n))),
        _ => None,
    }
}

/// A swizzle of a value of type `ty`: its type and the components it
/// takes.
fn swizzle(ty: Ty, field: &str, line: usize) -> Result<(Ty, Vec<usize>), Error> {
    let none = || Error::new(line, format!("a {} has no '{field}'", ty.name()));
    let Ty::Vec(base, n) = ty else {
        return Err(none());
    };
    let set = ["xyzw", "rgba", "stpq"]
        .into_iter()
        .find(|s| field.chars().all(|c| s.contains(c)))
        .ok_or_else(none)?;
    let idx: Vec<usize> = field.chars().filter_map(|c| set.find(c)).collect();
    if idx.is_empty() || idx.len() > 4 || idx.iter().any(|&i| i >= usize::from(n)) {
        return Err(none());
    }
    Ok((Ty::Vec(base, idx.len() as u8), idx))
}

fn map1(a: Ev, n: usize, f: fn(f32) -> f32) -> Ev {
    Arc::new(move |e| {
        let x = a(e);
        let mut r = ZERO;
        for (o, v) in r.iter_mut().zip(x).take(n) {
            *o = f(v);
        }
        r
    })
}

/// `f` of each component, a scalar argument (true) taken for every one.
fn map2([(a, sa), (b, sb)]: [(Ev, bool); 2], n: usize, f: fn(f32, f32) -> f32) -> Ev {
    Arc::new(move |e| {
        let (x, y) = (a(e), b(e));
        let mut r = ZERO;
        for (i, o) in r.iter_mut().enumerate().take(n) {
            *o = f(x[if sa { 0 } else { i }], y[if sb { 0 } else { i }]);
        }
        r
    })
}

fn map3([(a, sa), (b, sb), (c, sc)]: [(Ev, bool); 3], n: usize, f: fn(f32, f32, f32) -> f32) -> Ev {
    Arc::new(move |e| {
        let (x, y, z) = (a(e), b(e), c(e));
        let mut r = ZERO;
        for (i, o) in r.iter_mut().enumerate().take(n) {
            let k = |s: bool| if s { 0 } else { i };
            *o = f(x[k(sa)], y[k(sb)], z[k(sc)]);
        }
        r
    })
}

/// The product of two n × n matrices.
fn matmul(a: Ev, b: Ev, n: usize) -> Ev {
    Arc::new(move |e| {
        let (x, y) = (a(e), b(e));
        let mut r = ZERO;
        for c in 0..n {
            for row in 0..n {
                r[c * n + row] = (0..n).map(|k| x[k * n + row] * y[c * n + k]).sum();
            }
        }
        r
    })
}

/// Matrix `m`'s determinant (n × n), by elimination.
fn determinant(m: &Val, n: usize) -> f32 {
    let mut a = [[0f64; 4]; 4];
    for (c, col) in m.chunks(n).take(n).enumerate() {
        for (row, v) in col.iter().enumerate() {
            a[row][c] = f64::from(*v);
        }
    }
    let mut det = 1.0;
    for k in 0..n {
        let p = (k..n)
            .max_by(|&i, &j| a[i][k].abs().total_cmp(&a[j][k].abs()))
            .unwrap_or(k);
        if a[p][k] == 0.0 {
            return 0.0;
        }
        if p != k {
            a.swap(p, k);
            det = -det;
        }
        det *= a[k][k];
        let top = a[k];
        for row in a.iter_mut().take(n).skip(k + 1) {
            let f = row[k] / top[k];
            for (x, y) in row.iter_mut().zip(top).take(n).skip(k) {
                *x -= f * y;
            }
        }
    }
    det as f32
}

/// Matrix `m`'s inverse (n × n), by Gauss–Jordan; zero when it has none.
fn inverse(m: &Val, n: usize) -> Val {
    let mut a = [[0f64; 8]; 4];
    for (c, col) in m.chunks(n).take(n).enumerate() {
        for (row, v) in col.iter().enumerate() {
            a[row][c] = f64::from(*v);
        }
    }
    for (i, row) in a.iter_mut().enumerate().take(n) {
        row[n + i] = 1.0;
    }
    for k in 0..n {
        let p = (k..n)
            .max_by(|&i, &j| a[i][k].abs().total_cmp(&a[j][k].abs()))
            .unwrap_or(k);
        if a[p][k] == 0.0 {
            return ZERO;
        }
        a.swap(p, k);
        let pivot = a[k][k];
        for v in a[k].iter_mut().take(2 * n) {
            *v /= pivot;
        }
        let top = a[k];
        for (i, row) in a.iter_mut().enumerate().take(n) {
            if i != k {
                let f = row[k];
                for (x, y) in row.iter_mut().zip(top).take(2 * n) {
                    *x -= f * y;
                }
            }
        }
    }
    let mut r = ZERO;
    for c in 0..n {
        for (row, line) in a.iter().enumerate().take(n) {
            r[c * n + row] = line[n + c] as f32;
        }
    }
    r
}

/// GLSL's functions of one number, component by component.
fn unary_fn(name: &str) -> Option<fn(f32) -> f32> {
    Some(match name {
        "radians" => f32::to_radians,
        "degrees" => f32::to_degrees,
        "sin" => sine,
        "cos" => cosine,
        "tan" => f32::tan,
        "asin" => f32::asin,
        "acos" => f32::acos,
        "atan" => f32::atan,
        "sinh" => f32::sinh,
        "cosh" => f32::cosh,
        "tanh" => f32::tanh,
        "asinh" => f32::asinh,
        "acosh" => f32::acosh,
        "atanh" => f32::atanh,
        "exp" => f32::exp,
        "log" => f32::ln,
        "exp2" => f32::exp2,
        "log2" => f32::log2,
        "sqrt" => f32::sqrt,
        "inversesqrt" => |x| 1.0 / x.sqrt(),
        "abs" => f32::abs,
        "sign" => |x| {
            if x > 0.0 {
                1.0
            } else if x < 0.0 {
                -1.0
            } else {
                0.0
            }
        },
        "floor" => f32::floor,
        "ceil" => f32::ceil,
        "fract" => |x| x - x.floor(),
        "round" => f32::round,
        "roundEven" => f32::round_ties_even,
        "trunc" => f32::trunc,
        _ => return None,
    })
}

/// GLSL's sine and cosine as both the processor and the graphics card work
/// them out (the card's kernels the same sums, `glsl::wgsl`'s): the
/// angle brought to an eighth of a turn around a multiple of a quarter, in
/// three parts of the quarter, and a polynomial there — the same digits
/// on both, which random numbers from a sine's far digits need.
fn trig(x: f32, cosine: bool) -> f32 {
    const FOPI: f32 = 1.273_239_5;
    const DP1: f32 = 0.785_156_25;
    const DP2: f32 = 2.418_756_5e-4;
    const DP3: f32 = 3.774_895e-8;
    let mut sign = 1.0f32;
    let mut a = x;
    if a < 0.0 {
        a = -a;
        if !cosine {
            sign = -1.0;
        }
    }
    let mut j = (FOPI * a) as i32;
    let mut y = j as f32;
    if j & 1 != 0 {
        j += 1;
        y += 1.0;
    }
    j &= 7;
    if j > 3 {
        sign = -sign;
        j -= 4;
    }
    if cosine && j > 1 {
        sign = -sign;
    }
    let r = ((a - y * DP1) - y * DP2) - y * DP3;
    let z = r * r;
    let sine_part = ((-1.951_529_6e-4 * z + 8.332_161e-3) * z - 0.166_666_55) * z * r + r;
    let cosine_part =
        ((2.443_315_7e-5 * z - 1.388_731_6e-3) * z + 4.166_664_6e-2) * z * z - 0.5 * z + 1.0;
    let v = if (j == 1 || j == 2) != cosine {
        cosine_part
    } else {
        sine_part
    };
    sign * v
}

fn sine(x: f32) -> f32 {
    trig(x, false)
}

fn cosine(x: f32) -> f32 {
    trig(x, true)
}

/// GLSL's functions of two numbers, component by component.
fn binary_fn(name: &str) -> Option<fn(f32, f32) -> f32> {
    Some(match name {
        // As graphics cards work it out (2 to the power of y log2 x):
        // nothing for a negative x.
        "pow" => |x, y| if x < 0.0 { f32::NAN } else { x.powf(y) },
        "atan" => f32::atan2,
        "mod" => |x, y| x - y * (x / y).floor(),
        "min" => f32::min,
        "max" => f32::max,
        "step" => |edge, x| if x < edge { 0.0 } else { 1.0 },
        _ => return None,
    })
}

/// GLSL's functions of three numbers, component by component.
fn ternary_fn(name: &str) -> Option<fn(f32, f32, f32) -> f32> {
    Some(match name {
        "clamp" => |x, lo, hi| x.max(lo).min(hi),
        "mix" => |x, y, a| x * (1.0 - a) + y * a,
        "smoothstep" => |e0, e1, x| {
            let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        },
        "fma" => |a, b, c| a * b + c,
        _ => return None,
    })
}

/// The built-in functions taking other than numbers component by
/// component (to tell a wrong call from an unknown name).
const OTHER_BUILTINS: &[&str] = &[
    "length",
    "distance",
    "dot",
    "cross",
    "normalize",
    "faceforward",
    "reflect",
    "refract",
    "matrixCompMult",
    "transpose",
    "determinant",
    "inverse",
    "lessThan",
    "lessThanEqual",
    "greaterThan",
    "greaterThanEqual",
    "equal",
    "notEqual",
    "any",
    "all",
    "not",
    "getFromColor",
    "getToColor",
    "texture2D",
    "texture",
];

fn dot(a: &Val, b: &Val, n: usize) -> f32 {
    a.iter().zip(b).take(n).map(|(x, y)| x * y).sum()
}

/// A built-in function's call.
fn builtin(name: &str, mut a: Vec<Te>, line: usize) -> Result<Te, Error> {
    let types = a.iter().map(|t| t.ty.name()).collect::<Vec<_>>().join(", ");
    let bad = || Error::new(line, format!("no function '{name}' takes ({types})"));
    let count = a.len();
    if count == 1
        && let Some(f) = unary_fn(name)
    {
        let (base, n) = numeric(a[0].ty).ok_or_else(bad)?;
        let base = if base == Base::Int && matches!(name, "abs" | "sign") {
            Base::Int
        } else {
            Base::Float
        };
        let ev = a.remove(0).ev;
        return Ok(Te {
            ty: Ty::Vec(base, n as u8),
            ev: map1(ev, n, f),
        });
    }
    // The others take vectors and scalars, a scalar for every component.
    let sizes = |a: &[Te]| -> Option<(Vec<usize>, usize, bool)> {
        let mut out = Vec::new();
        let mut int = true;
        for t in a {
            let (b, n) = numeric(t.ty)?;
            int &= b == Base::Int;
            out.push(n);
        }
        let n = out.iter().copied().max()?;
        out.iter()
            .all(|&k| k == n || k == 1)
            .then_some((out, n, int))
    };
    if count == 2
        && let Some(f) = binary_fn(name)
    {
        let (s, n, int) = sizes(&a).ok_or_else(bad)?;
        let int = int && matches!(name, "min" | "max");
        let y = a.pop().ok_or_else(bad)?.ev;
        let x = a.pop().ok_or_else(bad)?.ev;
        return Ok(Te {
            ty: Ty::Vec(if int { Base::Int } else { Base::Float }, n as u8),
            ev: map2([(x, s[0] == 1), (y, s[1] == 1)], n, f),
        });
    }
    if count == 3
        && let Some(f) = ternary_fn(name)
    {
        let (s, n, int) = sizes(&a).ok_or_else(bad)?;
        let int = int && name == "clamp";
        let z = a.pop().ok_or_else(bad)?.ev;
        let y = a.pop().ok_or_else(bad)?.ev;
        let x = a.pop().ok_or_else(bad)?.ev;
        return Ok(Te {
            ty: Ty::Vec(if int { Base::Int } else { Base::Float }, n as u8),
            ev: map3([(x, s[0] == 1), (y, s[1] == 1), (z, s[2] == 1)], n, f),
        });
    }
    let float_vec = |t: &Te| numeric(t.ty).map(|(_, n)| n);
    let same = |a: &[Te]| -> Option<usize> {
        let n = float_vec(a.first()?)?;
        a.iter().all(|t| float_vec(t) == Some(n)).then_some(n)
    };
    let ev = |a: &mut Vec<Te>| a.remove(0).ev;
    match (name, count) {
        ("length", 1) => {
            let n = same(&a).ok_or_else(bad)?;
            let x = ev(&mut a);
            Ok(Te {
                ty: Ty::FLOAT,
                ev: Arc::new(move |e| {
                    let v = x(e);
                    scalar(dot(&v, &v, n).sqrt())
                }),
            })
        }
        ("distance" | "dot", 2) => {
            let n = same(&a).ok_or_else(bad)?;
            let (x, y) = (ev(&mut a), ev(&mut a));
            let distance = name == "distance";
            Ok(Te {
                ty: Ty::FLOAT,
                ev: Arc::new(move |e| {
                    let (p, q) = (x(e), y(e));
                    if distance {
                        let mut d = ZERO;
                        for (o, (u, v)) in d.iter_mut().zip(p.iter().zip(q)).take(n) {
                            *o = u - v;
                        }
                        scalar(dot(&d, &d, n).sqrt())
                    } else {
                        scalar(dot(&p, &q, n))
                    }
                }),
            })
        }
        ("cross", 2) if same(&a) == Some(3) => {
            let (x, y) = (ev(&mut a), ev(&mut a));
            Ok(Te {
                ty: Ty::Vec(Base::Float, 3),
                ev: Arc::new(move |e| {
                    let (p, q) = (x(e), y(e));
                    let mut r = ZERO;
                    r[0] = p[1] * q[2] - p[2] * q[1];
                    r[1] = p[2] * q[0] - p[0] * q[2];
                    r[2] = p[0] * q[1] - p[1] * q[0];
                    r
                }),
            })
        }
        ("normalize", 1) => {
            let n = same(&a).ok_or_else(bad)?;
            let x = ev(&mut a);
            Ok(Te {
                ty: Ty::Vec(Base::Float, n as u8),
                ev: Arc::new(move |e| {
                    let mut v = x(e);
                    let len = dot(&v, &v, n).sqrt();
                    for c in v.iter_mut().take(n) {
                        *c /= len;
                    }
                    v
                }),
            })
        }
        ("faceforward", 3) | ("reflect", 2) => {
            let n = same(&a).ok_or_else(bad)?;
            let ty = Ty::Vec(Base::Float, n as u8);
            if name == "reflect" {
                let (i, nn) = (ev(&mut a), ev(&mut a));
                return Ok(Te {
                    ty,
                    ev: Arc::new(move |e| {
                        let (i, nn) = (i(e), nn(e));
                        let d = 2.0 * dot(&nn, &i, n);
                        let mut r = ZERO;
                        for (k, o) in r.iter_mut().enumerate().take(n) {
                            *o = i[k] - d * nn[k];
                        }
                        r
                    }),
                });
            }
            let (nn, i, nref) = (ev(&mut a), ev(&mut a), ev(&mut a));
            Ok(Te {
                ty,
                ev: Arc::new(move |e| {
                    let (v, i, r) = (nn(e), i(e), nref(e));
                    if dot(&r, &i, n) < 0.0 {
                        v
                    } else {
                        let mut out = ZERO;
                        for (o, c) in out.iter_mut().zip(v).take(n) {
                            *o = -c;
                        }
                        out
                    }
                }),
            })
        }
        ("refract", 3) => {
            let n = same(&a[..2]).ok_or_else(bad)?;
            if a[2].ty.size() != 1 {
                return Err(bad());
            }
            let (i, nn, eta) = (ev(&mut a), ev(&mut a), ev(&mut a));
            Ok(Te {
                ty: Ty::Vec(Base::Float, n as u8),
                ev: Arc::new(move |e| {
                    let (i, nn, eta) = (i(e), nn(e), eta(e)[0]);
                    let d = dot(&nn, &i, n);
                    let k = 1.0 - eta * eta * (1.0 - d * d);
                    let mut r = ZERO;
                    if k >= 0.0 {
                        for (j, o) in r.iter_mut().enumerate().take(n) {
                            *o = eta * i[j] - (eta * d + k.sqrt()) * nn[j];
                        }
                    }
                    r
                }),
            })
        }
        ("matrixCompMult", 2) => match (a[0].ty, a[1].ty) {
            (Ty::Mat(m), Ty::Mat(k)) if m == k => {
                let n = usize::from(m) * usize::from(m);
                let (x, y) = (ev(&mut a), ev(&mut a));
                Ok(Te {
                    ty: Ty::Mat(m),
                    ev: map2([(x, false), (y, false)], n, |p, q| p * q),
                })
            }
            _ => Err(bad()),
        },
        ("transpose" | "determinant" | "inverse", 1) => {
            let Ty::Mat(m) = a[0].ty else {
                return Err(bad());
            };
            let n = usize::from(m);
            let x = ev(&mut a);
            Ok(match name {
                "transpose" => Te {
                    ty: Ty::Mat(m),
                    ev: Arc::new(move |e| {
                        let v = x(e);
                        let mut r = ZERO;
                        for c in 0..n {
                            for row in 0..n {
                                r[c * n + row] = v[row * n + c];
                            }
                        }
                        r
                    }),
                },
                "determinant" => Te {
                    ty: Ty::FLOAT,
                    ev: Arc::new(move |e| scalar(determinant(&x(e), n))),
                },
                _ => Te {
                    ty: Ty::Mat(m),
                    ev: Arc::new(move |e| inverse(&x(e), n)),
                },
            })
        }
        (
            "lessThan" | "lessThanEqual" | "greaterThan" | "greaterThanEqual" | "equal"
            | "notEqual",
            2,
        ) => {
            let ty = unify(a[0].ty, a[1].ty).ok_or_else(bad)?;
            let Ty::Vec(base, n) = ty else {
                return Err(bad());
            };
            if base == Base::Bool && !matches!(name, "equal" | "notEqual") {
                return Err(bad());
            }
            let f: fn(f32, f32) -> f32 = match name {
                "lessThan" => |p, q| truth(p < q),
                "lessThanEqual" => |p, q| truth(p <= q),
                "greaterThan" => |p, q| truth(p > q),
                "greaterThanEqual" => |p, q| truth(p >= q),
                "equal" => |p, q| truth(p == q),
                _ => |p, q| truth(p != q),
            };
            let (x, y) = (ev(&mut a), ev(&mut a));
            Ok(Te {
                ty: Ty::Vec(Base::Bool, n),
                ev: map2([(x, false), (y, false)], usize::from(n), f),
            })
        }
        ("any" | "all" | "not", 1) => {
            let Ty::Vec(Base::Bool, n) = a[0].ty else {
                return Err(bad());
            };
            let k = usize::from(n);
            let x = ev(&mut a);
            Ok(match name {
                "not" => Te {
                    ty: Ty::Vec(Base::Bool, n),
                    ev: map1(x, k, |v| truth(v == 0.0)),
                },
                "any" => Te {
                    ty: Ty::BOOL,
                    ev: Arc::new(move |e| scalar(truth(x(e).iter().take(k).any(|v| *v != 0.0)))),
                },
                _ => Te {
                    ty: Ty::BOOL,
                    ev: Arc::new(move |e| scalar(truth(x(e).iter().take(k).all(|v| *v != 0.0)))),
                },
            })
        }
        ("dFdx" | "dFdy" | "fwidth", _) => {
            Err(Error::new(line, format!("'{name}' is not supported yet")))
        }
        _ if unary_fn(name).is_some()
            || binary_fn(name).is_some()
            || ternary_fn(name).is_some()
            || OTHER_BUILTINS.contains(&name) =>
        {
            Err(bad())
        }
        _ => Err(Error::new(line, format!("unknown function '{name}'"))),
    }
}

/// A binary operator on two values.
fn binary(op: &'static str, a: Te, b: Te, line: usize) -> Result<Te, Error> {
    let msg = format!("'{op}' cannot take a {} and a {}", a.ty.name(), b.ty.name());
    let bad = || Error::new(line, msg.clone());
    if matches!(op, "==" | "!=") {
        let ty = unify(a.ty, b.ty)
            .filter(|t| !matches!(t, Ty::Void | Ty::Sampler))
            .ok_or_else(bad)?;
        let n = ty.size();
        let equal = op == "==";
        let (x, y) = (a.ev, b.ev);
        return Ok(Te {
            ty: Ty::BOOL,
            ev: Arc::new(move |e| {
                let (p, q) = (x(e), y(e));
                scalar(truth((p[..n] == q[..n]) == equal))
            }),
        });
    }
    if matches!(op, "<" | ">" | "<=" | ">=") {
        if numeric(a.ty).map(|t| t.1) != Some(1) || numeric(b.ty).map(|t| t.1) != Some(1) {
            return Err(bad());
        }
        let f: fn(f32, f32) -> f32 = match op {
            "<" => |p, q| truth(p < q),
            ">" => |p, q| truth(p > q),
            "<=" => |p, q| truth(p <= q),
            _ => |p, q| truth(p >= q),
        };
        return Ok(Te {
            ty: Ty::BOOL,
            ev: map2([(a.ev, true), (b.ev, true)], 1, f),
        });
    }
    if op == "^^" {
        if a.ty != Ty::BOOL || b.ty != Ty::BOOL {
            return Err(bad());
        }
        return Ok(Te {
            ty: Ty::BOOL,
            ev: map2([(a.ev, true), (b.ev, true)], 1, |p, q| {
                truth((p != 0.0) != (q != 0.0))
            }),
        });
    }
    let elementwise = |op: &str| -> Option<fn(f32, f32) -> f32> {
        Some(match op {
            "+" => |x, y| x + y,
            "-" => |x, y| x - y,
            "*" => |x, y| x * y,
            "/" => |x, y| x / y,
            _ => return None,
        })
    };
    match (a.ty, b.ty) {
        (Ty::Vec(ba, na), Ty::Vec(bb, nb)) if ba != Base::Bool && bb != Base::Bool => {
            if na != nb && na != 1 && nb != 1 {
                return Err(bad());
            }
            let n = usize::from(na.max(nb));
            let int = ba == Base::Int && bb == Base::Int;
            let f: fn(f32, f32) -> f32 = match (op, int) {
                ("/", true) => |x, y| if y == 0.0 { 0.0 } else { (x / y).trunc() },
                ("%", true) => |x, y| if y == 0.0 { 0.0 } else { x % y },
                ("%", false) => |x, y| x - y * (x / y).floor(),
                ("&", true) => |x, y| ((x as i64) & (y as i64)) as f32,
                ("|", true) => |x, y| ((x as i64) | (y as i64)) as f32,
                ("^", true) => |x, y| ((x as i64) ^ (y as i64)) as f32,
                ("<<", true) => |x, y| ((x as i64) << (y as i64).clamp(0, 31)) as f32,
                (">>", true) => |x, y| ((x as i64) >> (y as i64).clamp(0, 31)) as f32,
                _ => elementwise(op).ok_or_else(bad)?,
            };
            Ok(Te {
                ty: Ty::Vec(if int { Base::Int } else { Base::Float }, n as u8),
                ev: map2([(a.ev, na == 1), (b.ev, nb == 1)], n, f),
            })
        }
        (Ty::Mat(m), Ty::Mat(k)) if m == k => {
            let n = usize::from(m);
            if op == "*" {
                return Ok(Te {
                    ty: a.ty,
                    ev: matmul(a.ev, b.ev, n),
                });
            }
            let f = elementwise(op).ok_or_else(bad)?;
            Ok(Te {
                ty: a.ty,
                ev: map2([(a.ev, false), (b.ev, false)], n * n, f),
            })
        }
        (Ty::Mat(m), Ty::Vec(Base::Float | Base::Int, k)) if op == "*" && k == m => {
            let n = usize::from(m);
            let (x, v) = (a.ev, b.ev);
            Ok(Te {
                ty: Ty::Vec(Base::Float, m),
                ev: Arc::new(move |e| {
                    let (x, v) = (x(e), v(e));
                    let mut r = ZERO;
                    for (row, o) in r.iter_mut().enumerate().take(n) {
                        *o = (0..n).map(|k| x[k * n + row] * v[k]).sum();
                    }
                    r
                }),
            })
        }
        (Ty::Vec(Base::Float | Base::Int, k), Ty::Mat(m)) if op == "*" && k == m => {
            let n = usize::from(m);
            let (v, x) = (a.ev, b.ev);
            Ok(Te {
                ty: Ty::Vec(Base::Float, m),
                ev: Arc::new(move |e| {
                    let (v, x) = (v(e), x(e));
                    let mut r = ZERO;
                    for (c, o) in r.iter_mut().enumerate().take(n) {
                        *o = (0..n).map(|k| v[k] * x[c * n + k]).sum();
                    }
                    r
                }),
            })
        }
        (Ty::Mat(m), Ty::Vec(Base::Float | Base::Int, 1)) => {
            let f = elementwise(op).ok_or_else(bad)?;
            let n = usize::from(m) * usize::from(m);
            Ok(Te {
                ty: a.ty,
                ev: map2([(a.ev, false), (b.ev, true)], n, f),
            })
        }
        (Ty::Vec(Base::Float | Base::Int, 1), Ty::Mat(m)) => {
            let f = elementwise(op).ok_or_else(bad)?;
            let n = usize::from(m) * usize::from(m);
            Ok(Te {
                ty: b.ty,
                ev: map2([(a.ev, true), (b.ev, false)], n, f),
            })
        }
        _ => Err(bad()),
    }
}

// ---- The compiler ------------------------------------------------------------------

struct Compiler {
    /// The code's structs: their names and fields.
    structs: Vec<(String, Fields)>,
    slots: usize,
    scopes: Vec<HashMap<String, Var>>,
    funcs: Vec<Func>,
    by_name: HashMap<String, Vec<usize>>,
    /// Calls: from which function (none: a global's value), to which, on
    /// which line.
    calls: Vec<(Option<usize>, usize, usize)>,
    current: Option<usize>,
    ret: Ty,
    loops: usize,
    /// The whole-number constants known so far (arrays' sizes).
    consts: HashMap<String, i64>,
}

impl Compiler {
    fn new() -> Compiler {
        Compiler {
            structs: Vec::new(),
            slots: 0,
            scopes: vec![HashMap::new()],
            funcs: Vec::new(),
            by_name: HashMap::new(),
            calls: Vec::new(),
            current: None,
            ret: Ty::Void,
            loops: 0,
            consts: HashMap::new(),
        }
    }

    fn lookup(&self, name: &str) -> Option<Var> {
        self.scopes.iter().rev().find_map(|s| s.get(name).copied())
    }

    fn declare(&mut self, name: &str, ty: Ty, fixed: bool, line: usize) -> Result<usize, Error> {
        let slot = self.slots;
        let scope = self.scopes.last_mut().expect("a scope");
        if scope.contains_key(name) {
            return Err(Error::new(line, format!("'{name}' is declared twice")));
        }
        scope.insert(
            name.to_string(),
            Var {
                slot,
                ty,
                fixed,
                len: None,
            },
        );
        self.slots += 1;
        Ok(slot)
    }

    /// A whole number `e` stands for, worked out now (an array's size).
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

    /// The array `e` names, if it names one.
    fn array_var(&self, e: &E) -> Option<Var> {
        match &e.kind {
            Ek::Name(n) => self.lookup(n).filter(|v| v.len.is_some()),
            _ => None,
        }
    }

    /// An array's elements' values from `value`: an array's constructor or
    /// another array, of `len` elements (any, for none) of `ty`.
    fn elements(&mut self, value: &E, ty: Ty, len: Option<usize>) -> Result<Vec<Ev>, Error> {
        let line = value.line;
        let wrong = || Error::new(line, format!("this is not an array of {}", ty.name()));
        match &value.kind {
            Ek::Array(of, size, args) => {
                if unify(*of, ty) != Some(ty) {
                    return Err(wrong());
                }
                if let Some(size) = size {
                    let n = self.fold(size).ok_or_else(|| {
                        Error::new(line, "an array's size must be a whole-number constant")
                    })?;
                    if n != args.len() as i64 {
                        return Err(Error::new(
                            line,
                            format!("an array of {n} given {} values", args.len()),
                        ));
                    }
                }
                if len.is_some_and(|n| n != args.len()) {
                    return Err(wrong());
                }
                args.iter().map(|a| self.expr_as(a, ty)).collect()
            }
            _ => match self.array_var(value) {
                Some(v) if v.ty == ty && (len.is_none() || v.len == len) => {
                    let n = v.len.unwrap_or(0);
                    Ok((0..n)
                        .map(|k| {
                            let slot = v.slot + k;
                            Arc::new(move |e: &mut Env<'_>| e.slots[slot]) as Ev
                        })
                        .collect())
                }
                _ => Err(wrong()),
            },
        }
    }

    /// Declares array `name` of `ty` (`size` long, or as long as its
    /// value): the stores of its elements' values (none: zero).
    fn array(
        &mut self,
        name: &str,
        ty: Ty,
        size: Option<&E>,
        value: Option<&E>,
        fixed: bool,
        line: usize,
    ) -> Result<Vec<(usize, Option<Ev>)>, Error> {
        if matches!(ty, Ty::Void | Ty::Sampler) {
            return Err(Error::new(
                line,
                format!("an array cannot hold {}", ty.name()),
            ));
        }
        let len =
            match size {
                Some(e) => Some(self.fold(e).filter(|n| (1..=4096).contains(n)).ok_or_else(
                    || Error::new(line, "an array's size must be a whole-number constant"),
                )? as usize),
                None => None,
            };
        let values = match value {
            Some(v) => Some(self.elements(v, ty, len)?),
            None if fixed => {
                return Err(Error::new(line, format!("const '{name}' needs a value")));
            }
            None => None,
        };
        let len = len
            .or(values.as_ref().map(Vec::len))
            .filter(|n| *n > 0)
            .ok_or_else(|| Error::new(line, format!("'{name}' needs a size")))?;
        let slot = self.slots;
        let scope = self.scopes.last_mut().expect("a scope");
        if scope.contains_key(name) {
            return Err(Error::new(line, format!("'{name}' is declared twice")));
        }
        scope.insert(
            name.to_string(),
            Var {
                slot,
                ty,
                fixed,
                len: Some(len),
            },
        );
        self.slots += len;
        Ok(match values {
            Some(v) => v
                .into_iter()
                .enumerate()
                .map(|(k, e)| (slot + k, Some(e)))
                .collect(),
            None => (0..len).map(|k| (slot + k, None)).collect(),
        })
    }

    /// The function `name` taking exactly `types`.
    fn find_exact(&self, name: &str, types: &[Ty]) -> Option<usize> {
        self.by_name.get(name)?.iter().copied().find(|&i| {
            let f = &self.funcs[i];
            f.params.len() == types.len()
                && f.params.iter().zip(types).all(|((p, _), t)| p.ty == *t)
        })
    }

    fn declare_func(
        &mut self,
        ret: Ty,
        name: &str,
        params: &[Param],
        line: usize,
    ) -> Result<(), Error> {
        if params.len() > MAX_PARAMS {
            return Err(Error::new(
                line,
                format!("a function takes at most {MAX_PARAMS} parameters"),
            ));
        }
        if params.iter().any(|p| p.ty == Ty::Void) {
            return Err(Error::new(line, "a parameter cannot be void"));
        }
        let types: Vec<Ty> = params.iter().map(|p| p.ty).collect();
        // A prototype and its definition are one function.
        if let Some(i) = self.find_exact(name, &types) {
            if self.funcs[i].ret != ret {
                return Err(Error::new(
                    line,
                    format!("'{name}' is declared with another return type"),
                ));
            }
            return Ok(());
        }
        let params = params
            .iter()
            .map(|p| {
                let slot = self.slots;
                self.slots += 1;
                (p.clone(), slot)
            })
            .collect();
        self.funcs.push(Func {
            name: name.to_string(),
            ret,
            params,
            body: Arc::new(OnceLock::new()),
            line,
        });
        self.by_name
            .entry(name.to_string())
            .or_default()
            .push(self.funcs.len() - 1);
        Ok(())
    }

    fn define_func(
        &mut self,
        name: &str,
        params: &[Param],
        body: &[St],
        line: usize,
    ) -> Result<(), Error> {
        let types: Vec<Ty> = params.iter().map(|p| p.ty).collect();
        let i = self
            .find_exact(name, &types)
            .ok_or_else(|| Error::new(line, format!("'{name}' was not declared")))?;
        if self.funcs[i].body.get().is_some() {
            return Err(Error::new(line, format!("'{name}' is defined twice")));
        }
        let mut scope = HashMap::new();
        for (p, (_, slot)) in params.iter().zip(&self.funcs[i].params) {
            if !p.name.is_empty() {
                scope.insert(
                    p.name.clone(),
                    Var {
                        slot: *slot,
                        ty: p.ty,
                        fixed: false,
                        len: None,
                    },
                );
            }
        }
        self.current = Some(i);
        self.ret = self.funcs[i].ret;
        self.scopes.push(scope);
        let run = self.block(body);
        self.scopes.pop();
        self.current = None;
        self.ret = Ty::Void;
        let _ = self.funcs[i].body.set(run?);
        Ok(())
    }

    /// Every function called has a body, and none comes back to itself.
    fn check_calls(&self) -> Result<(), Error> {
        for &(_, to, line) in &self.calls {
            if self.funcs[to].body.get().is_none() {
                return Err(Error::new(
                    line,
                    format!("'{}' is declared but never defined", self.funcs[to].name),
                ));
            }
        }
        let mut next = vec![Vec::new(); self.funcs.len()];
        for &(from, to, _) in &self.calls {
            if let Some(f) = from {
                next[f].push(to);
            }
        }
        // 0 not seen, 1 being followed, 2 done.
        fn circles(i: usize, next: &[Vec<usize>], state: &mut [u8]) -> bool {
            state[i] = 1;
            for &j in &next[i] {
                if state[j] == 1 || (state[j] == 0 && circles(j, next, state)) {
                    return true;
                }
            }
            state[i] = 2;
            false
        }
        let mut state = vec![0u8; self.funcs.len()];
        for i in 0..self.funcs.len() {
            if state[i] == 0 && circles(i, &next, &mut state) {
                let f = &self.funcs[i];
                return Err(Error::new(
                    f.line,
                    format!("'{}' calls itself: GLSL has no recursion", f.name),
                ));
            }
        }
        Ok(())
    }

    /// The value of default `text` (a uniform's), as type `ty`.
    fn constant(&mut self, text: &str, ty: Ty) -> Option<Val> {
        let lexed = lex::lex(text).ok()?;
        let e = parse::expression(&lexed).ok()?;
        let ev = self.expr_as(&e, ty).ok()?;
        let mut slots = vec![ZERO; self.slots];
        let none = |_: u8, _: [f32; 2]| [0.0f32; 4];
        let mut env = Env {
            slots: &mut slots,
            ret: ZERO,
            discarded: false,
            steps: 0,
            sample: &none,
        };
        Some(ev(&mut env))
    }

    fn expr_as(&mut self, e: &E, ty: Ty) -> Result<Ev, Error> {
        let te = self.expr(e)?;
        coerce(te, ty, e.line)
    }

    /// A condition: a bool.
    fn cond(&mut self, e: &E) -> Result<Ev, Error> {
        let te = self.expr(e)?;
        if te.ty != Ty::BOOL {
            return Err(Error::new(
                e.line,
                format!("a condition must be a bool, not a {}", te.ty.name()),
            ));
        }
        Ok(te.ev)
    }

    /// An index: an int (a float taken whole).
    fn index(&mut self, e: &E) -> Result<Ev, Error> {
        let te = self.expr(e)?;
        match numeric(te.ty) {
            Some((_, 1)) => Ok(te.ev),
            _ => Err(Error::new(
                e.line,
                format!("an index must be an int, not a {}", te.ty.name()),
            )),
        }
    }

    fn expr(&mut self, e: &E) -> Result<Te, Error> {
        let line = e.line;
        match &e.kind {
            Ek::Float(v) => Ok(constant(Ty::FLOAT, scalar(*v))),
            Ek::Int(v) => Ok(constant(Ty::INT, scalar(*v as f32))),
            Ek::Bool(b) => Ok(constant(Ty::BOOL, scalar(truth(*b)))),
            Ek::Name(n) => {
                let v = self
                    .lookup(n)
                    .ok_or_else(|| Error::new(line, format!("unknown name '{n}'")))?;
                if v.len.is_some() {
                    return Err(Error::new(line, format!("'{n}' is an array: index it")));
                }
                let slot = v.slot;
                Ok(Te {
                    ty: v.ty,
                    ev: Arc::new(move |e| e.slots[slot]),
                })
            }
            Ek::Array(..) => Err(Error::new(
                line,
                "an array's value goes into an array's declaration or assignment",
            )),
            Ek::Length(base) => {
                let v = self
                    .array_var(base)
                    .ok_or_else(|| Error::new(line, "length() belongs to arrays"))?;
                let n = v.len.unwrap_or(0) as f32;
                Ok(constant(Ty::INT, scalar(n)))
            }
            Ek::Index(base, i) if self.array_var(base).is_some() => {
                let v = self.array_var(base).expect("an array");
                let i = self.index(i)?;
                let (first, n) = (v.slot, v.len.unwrap_or(1) as i64);
                Ok(Te {
                    ty: v.ty,
                    ev: Arc::new(move |e| {
                        let k = (i(e)[0] as i64).clamp(0, n - 1) as usize;
                        e.slots[first + k]
                    }),
                })
            }
            Ek::Call(name, args) => self.call(name, args, line),
            Ek::Field(base, field) => {
                let b = self.expr(base)?;
                let (ty, idx) = self.field(b.ty, field, line)?;
                let ev = b.ev;
                Ok(Te {
                    ty,
                    ev: Arc::new(move |e| {
                        let v = ev(e);
                        let mut r = ZERO;
                        for (o, &i) in r.iter_mut().zip(&idx) {
                            *o = v[i];
                        }
                        r
                    }),
                })
            }
            Ek::Index(base, i) => {
                let b = self.expr(base)?;
                let i = self.index(i)?;
                let (ty, n, part) = indexed(b.ty, line)?;
                let ev = b.ev;
                Ok(Te {
                    ty,
                    ev: Arc::new(move |e| {
                        let k = (i(e)[0] as i64).clamp(0, n as i64 - 1) as usize;
                        let v = ev(e);
                        let mut r = ZERO;
                        r[..part].copy_from_slice(&v[k * part..(k + 1) * part]);
                        r
                    }),
                })
            }
            Ek::Unary(op, a) => {
                let a = self.expr(a)?;
                let bad = Error::new(line, format!("'{op}' cannot take a {}", a.ty.name()));
                match (*op, a.ty) {
                    ("+", Ty::Vec(Base::Float | Base::Int, _) | Ty::Mat(_)) => Ok(a),
                    ("-", Ty::Vec(Base::Float | Base::Int, _) | Ty::Mat(_)) => {
                        let n = a.ty.size();
                        Ok(Te {
                            ty: a.ty,
                            ev: map1(a.ev, n, |x| -x),
                        })
                    }
                    ("!", Ty::Vec(Base::Bool, 1)) => Ok(Te {
                        ty: Ty::BOOL,
                        ev: map1(a.ev, 1, |x| truth(x == 0.0)),
                    }),
                    ("~", Ty::Vec(Base::Int, n)) => Ok(Te {
                        ty: a.ty,
                        ev: map1(a.ev, usize::from(n), |x| !(x as i64) as f32),
                    }),
                    _ => Err(bad),
                }
            }
            Ek::Binary(op @ ("&&" | "||"), a, b) => {
                let (x, y) = (self.cond(a)?, self.cond(b)?);
                let and = *op == "&&";
                Ok(Te {
                    ty: Ty::BOOL,
                    ev: Arc::new(move |e| {
                        let first = x(e)[0] != 0.0;
                        scalar(truth(if and {
                            first && y(e)[0] != 0.0
                        } else {
                            first || y(e)[0] != 0.0
                        }))
                    }),
                })
            }
            Ek::Binary(op, a, b) => {
                let (a, b) = (self.expr(a)?, self.expr(b)?);
                binary(op, a, b, line)
            }
            Ek::Assign(op, target, value) => self.assign(op, target, value, line),
            Ek::Step(op, prefix, target) => {
                let p = self.place(target)?;
                let n = match numeric(p.ty) {
                    Some((_, n)) => n,
                    None if matches!(p.ty, Ty::Mat(_)) => p.ty.size(),
                    None => {
                        return Err(Error::new(
                            line,
                            format!("'{op}' cannot take a {}", p.ty.name()),
                        ));
                    }
                };
                let d = if *op == "++" { 1.0 } else { -1.0 };
                let (get, set, prefix) = (p.get, p.set, *prefix);
                Ok(Te {
                    ty: p.ty,
                    ev: Arc::new(move |e| {
                        let old = get(e);
                        let mut new = old;
                        for c in new.iter_mut().take(n) {
                            *c += d;
                        }
                        set(e, new);
                        if prefix { new } else { old }
                    }),
                })
            }
            Ek::Cond(c, a, b) => {
                let c = self.cond(c)?;
                let (a, b) = (self.expr(a)?, self.expr(b)?);
                let ty = unify(a.ty, b.ty).ok_or_else(|| {
                    Error::new(
                        line,
                        format!(
                            "'?:' cannot choose between a {} and a {}",
                            a.ty.name(),
                            b.ty.name()
                        ),
                    )
                })?;
                let (x, y) = (a.ev, b.ev);
                Ok(Te {
                    ty,
                    ev: Arc::new(move |e| if c(e)[0] != 0.0 { x(e) } else { y(e) }),
                })
            }
            Ek::Comma(a, b) => {
                let a = self.expr(a)?.ev;
                let b = self.expr(b)?;
                let bev = b.ev;
                Ok(Te {
                    ty: b.ty,
                    ev: Arc::new(move |e| {
                        a(e);
                        bev(e)
                    }),
                })
            }
        }
    }

    fn assign(
        &mut self,
        op: &'static str,
        target: &E,
        value: &E,
        line: usize,
    ) -> Result<Te, Error> {
        // A whole array: each element from the value's, all read first.
        if let Some(v) = self.array_var(target) {
            if op != "=" || v.fixed {
                return Err(Error::new(line, "this cannot be assigned to an array"));
            }
            let values = self.elements(value, v.ty, v.len)?;
            let first = v.slot;
            return Ok(Te {
                ty: v.ty,
                ev: Arc::new(move |e| {
                    let all: Vec<Val> = values.iter().map(|x| x(e)).collect();
                    for (k, x) in all.into_iter().enumerate() {
                        e.slots[first + k] = x;
                    }
                    ZERO
                }),
            });
        }
        let p = self.place(target)?;
        let v = self.expr(value)?;
        let value = if op == "=" {
            coerce(v, p.ty, line)?
        } else {
            let now = Te {
                ty: p.ty,
                ev: p.get.clone(),
            };
            let r = binary(&op[..op.len() - 1], now, v, line)?;
            coerce(r, p.ty, line)?
        };
        let set = p.set;
        Ok(Te {
            ty: p.ty,
            ev: Arc::new(move |e| {
                let x = value(e);
                set(e, x);
                x
            }),
        })
    }

    /// What `e` names to be assigned to.
    fn place(&mut self, e: &E) -> Result<Place, Error> {
        let line = e.line;
        match &e.kind {
            Ek::Index(base, i) if self.array_var(base).is_some() => {
                let v = self.array_var(base).expect("an array");
                if v.fixed {
                    return Err(Error::new(line, "a const array cannot be changed"));
                }
                let i = self.index(i)?;
                let (first, n) = (v.slot, v.len.unwrap_or(1) as i64);
                let at = move |e: &mut Env<'_>, i: &Ev| {
                    first + (i(e)[0] as i64).clamp(0, n - 1) as usize
                };
                let i2 = i.clone();
                Ok(Place {
                    ty: v.ty,
                    get: Arc::new(move |e| {
                        let k = at(e, &i);
                        e.slots[k]
                    }),
                    set: Arc::new(move |e, x| {
                        let k = at(e, &i2);
                        e.slots[k] = x;
                    }),
                })
            }
            Ek::Name(n) => {
                let v = self
                    .lookup(n)
                    .ok_or_else(|| Error::new(line, format!("unknown name '{n}'")))?;
                if v.len.is_some() {
                    return Err(Error::new(line, format!("'{n}' is an array: index it")));
                }
                if v.fixed {
                    return Err(Error::new(line, format!("'{n}' cannot be changed")));
                }
                let slot = v.slot;
                Ok(Place {
                    ty: v.ty,
                    get: Arc::new(move |e| e.slots[slot]),
                    set: Arc::new(move |e, x| e.slots[slot] = x),
                })
            }
            Ek::Field(base, field) => {
                let b = self.place(base)?;
                let (ty, idx) = self.field(b.ty, field, line)?;
                if (1..idx.len()).any(|k| idx[..k].contains(&idx[k])) {
                    return Err(Error::new(
                        line,
                        format!("'{field}' names a component twice"),
                    ));
                }
                let (bget, bset) = (b.get, b.set);
                let (read, idx2, bget2) = (idx.clone(), idx, bget.clone());
                Ok(Place {
                    ty,
                    get: Arc::new(move |e| {
                        let v = bget(e);
                        let mut r = ZERO;
                        for (o, &i) in r.iter_mut().zip(&read) {
                            *o = v[i];
                        }
                        r
                    }),
                    set: Arc::new(move |e, x| {
                        let mut v = bget2(e);
                        for (k, &i) in idx2.iter().enumerate() {
                            v[i] = x[k];
                        }
                        bset(e, v);
                    }),
                })
            }
            Ek::Index(base, i) => {
                let b = self.place(base)?;
                let i = self.index(i)?;
                let (ty, n, part) = indexed(b.ty, line)?;
                let at =
                    move |e: &mut Env<'_>, i: &Ev| (i(e)[0] as i64).clamp(0, n as i64 - 1) as usize;
                let (bget, bset, i2) = (b.get, b.set, i.clone());
                let bget_set = bget.clone();
                Ok(Place {
                    ty,
                    get: Arc::new(move |e| {
                        let k = at(e, &i);
                        let v = bget(e);
                        let mut r = ZERO;
                        r[..part].copy_from_slice(&v[k * part..(k + 1) * part]);
                        r
                    }),
                    set: Arc::new(move |e, x| {
                        let k = at(e, &i2);
                        let mut v = bget_set(e);
                        v[k * part..(k + 1) * part].copy_from_slice(&x[..part]);
                        bset(e, v);
                    }),
                })
            }
            _ => Err(Error::new(line, "this cannot be assigned to")),
        }
    }

    /// A struct's field (its type and its numbers' places), or a
    /// vector's swizzle.
    fn field(&self, ty: Ty, field: &str, line: usize) -> Result<(Ty, Vec<usize>), Error> {
        if let Ty::Struct(id, _) = ty {
            let (_, fields) = &self.structs[usize::from(id)];
            let &(_, t, at) = fields
                .iter()
                .find(|(n, ..)| n == field)
                .ok_or_else(|| Error::new(line, format!("the struct has no '{field}'")))?;
            return Ok((t, (at..at + t.size()).collect()));
        }
        swizzle(ty, field, line)
    }

    /// A struct of the code's made of `args`, its fields in order.
    fn structure(&mut self, id: usize, args: &[E], line: usize) -> Result<Te, Error> {
        let fields = self.structs[id].1.clone();
        if args.len() != fields.len() {
            return Err(Error::new(
                line,
                format!("'{}' has {} fields", self.structs[id].0, fields.len()),
            ));
        }
        let mut parts = Vec::new();
        for ((_, t, at), a) in fields.iter().zip(args) {
            parts.push((self.expr_as(a, *t)?, *at, t.size()));
        }
        let size: usize = fields.iter().map(|(_, t, _)| t.size()).sum();
        Ok(Te {
            ty: Ty::Struct(id as u8, size as u8),
            ev: Arc::new(move |e| {
                let mut r = ZERO;
                for (ev, at, n) in &parts {
                    let v = ev(e);
                    r[*at..*at + *n].copy_from_slice(&v[..*n]);
                }
                r
            }),
        })
    }

    fn call(&mut self, name: &str, args: &[E], line: usize) -> Result<Te, Error> {
        if let Some(id) = self.structs.iter().position(|(n, _)| n == name) {
            return self.structure(id, args, line);
        }
        if let Some(ty) = Ty::named(name) {
            return self.construct(ty, args, line);
        }
        let mut tes = Vec::with_capacity(args.len());
        for a in args {
            tes.push(self.expr(a)?);
        }
        if self.by_name.contains_key(name) {
            return self.user_call(name, args, tes, line);
        }
        match name {
            "getFromColor" | "getToColor" if tes.len() == 1 => {
                let which = u8::from(name == "getToColor");
                let uv = coerce(tes.remove(0), Ty::VEC2, line)?;
                Ok(Te {
                    ty: Ty::VEC4,
                    ev: Arc::new(move |e| {
                        let p = uv(e);
                        sampled((e.sample)(which, [p[0], p[1]]))
                    }),
                })
            }
            "texture2D" | "texture" if matches!(tes.len(), 2 | 3) && tes[0].ty == Ty::Sampler => {
                let s = tes.remove(0).ev;
                let uv = coerce(tes.remove(0), Ty::VEC2, line)?;
                Ok(Te {
                    ty: Ty::VEC4,
                    ev: Arc::new(move |e| {
                        let which = s(e)[0] as u8;
                        let p = uv(e);
                        sampled((e.sample)(which, [p[0], p[1]]))
                    }),
                })
            }
            _ => builtin(name, tes, line),
        }
    }

    /// A call of one of the code's own functions.
    fn user_call(
        &mut self,
        name: &str,
        args: &[E],
        tes: Vec<Te>,
        line: usize,
    ) -> Result<Te, Error> {
        let types: Vec<Ty> = tes.iter().map(|t| t.ty).collect();
        let candidates = self.by_name.get(name).cloned().unwrap_or_default();
        let fits = |i: usize, exact: bool| {
            let f = &self.funcs[i];
            f.params.len() == types.len()
                && f.params
                    .iter()
                    .zip(&types)
                    .all(|((p, _), t)| p.ty == *t || (!exact && unify(p.ty, *t) == Some(p.ty)))
        };
        let pick = candidates
            .iter()
            .copied()
            .find(|&i| fits(i, true))
            .or_else(|| candidates.iter().copied().find(|&i| fits(i, false)))
            .ok_or_else(|| {
                let list = types
                    .iter()
                    .map(|t| t.name())
                    .collect::<Vec<_>>()
                    .join(", ");
                Error::new(line, format!("no function '{name}' takes ({list})"))
            })?;
        self.calls.push((self.current, pick, line));
        let params = self.funcs[pick].params.clone();
        let mut ins = Vec::new();
        let mut outs = Vec::new();
        for (k, (te, (param, slot))) in tes.into_iter().zip(&params).enumerate() {
            ins.push(if param.input {
                coerce(te, param.ty, line)?
            } else {
                Arc::new(|_: &mut Env<'_>| ZERO) as Ev
            });
            if param.output {
                let p = self.place(&args[k])?;
                if p.ty != param.ty {
                    return Err(Error::new(
                        line,
                        format!("an out parameter needs a {}", param.ty.name()),
                    ));
                }
                outs.push((*slot, p.set));
            }
        }
        let slots: Vec<usize> = params.iter().map(|(_, s)| *s).collect();
        let body = self.funcs[pick].body.clone();
        Ok(Te {
            ty: self.funcs[pick].ret,
            ev: Arc::new(move |e| {
                let mut values = [ZERO; MAX_PARAMS];
                for (v, a) in values.iter_mut().zip(&ins) {
                    *v = a(e);
                }
                for (&s, v) in slots.iter().zip(values) {
                    e.slots[s] = v;
                }
                e.ret = ZERO;
                if let Some(run) = body.get() {
                    run(e);
                }
                let r = e.ret;
                for (s, set) in &outs {
                    let v = e.slots[*s];
                    set(e, v);
                }
                r
            }),
        })
    }

    /// A type's constructor.
    fn construct(&mut self, ty: Ty, args: &[E], line: usize) -> Result<Te, Error> {
        let mut tes = Vec::with_capacity(args.len());
        for a in args {
            let te = self.expr(a)?;
            if matches!(te.ty, Ty::Void | Ty::Sampler) {
                return Err(Error::new(
                    line,
                    format!("a {} cannot make a {}", te.ty.name(), ty.name()),
                ));
            }
            tes.push(te);
        }
        let wrong = || {
            Error::new(
                line,
                format!("wrong number of components for {}", ty.name()),
            )
        };
        if tes.is_empty() {
            return Err(wrong());
        }
        let (n, conv): (usize, fn(f32) -> f32) = match ty {
            Ty::Vec(Base::Float, n) => (usize::from(n), |x| x),
            Ty::Vec(Base::Int, n) => (usize::from(n), f32::trunc),
            Ty::Vec(Base::Bool, n) => (usize::from(n), |x| truth(x != 0.0)),
            Ty::Mat(n) => (usize::from(n) * usize::from(n), |x| x),
            _ => return Err(Error::new(line, format!("a {} cannot be made", ty.name()))),
        };
        let single = tes.len() == 1;
        // One scalar: every component (a matrix's diagonal).
        if single && tes[0].ty.size() == 1 {
            let a = tes.remove(0).ev;
            let diagonal = match ty {
                Ty::Mat(m) => Some(usize::from(m)),
                _ => None,
            };
            return Ok(Te {
                ty,
                ev: Arc::new(move |e| {
                    let x = conv(a(e)[0]);
                    let mut r = ZERO;
                    match diagonal {
                        Some(m) => (0..m).for_each(|c| r[c * m + c] = x),
                        None => r.iter_mut().take(n).for_each(|o| *o = x),
                    }
                    r
                }),
            });
        }
        // A matrix from a matrix: what they share, the identity elsewhere.
        if single && let (Ty::Mat(m), Ty::Mat(k)) = (ty, tes[0].ty) {
            let (m, k) = (usize::from(m), usize::from(k));
            let a = tes.remove(0).ev;
            return Ok(Te {
                ty,
                ev: Arc::new(move |e| {
                    let v = a(e);
                    let mut r = ZERO;
                    for c in 0..m {
                        for row in 0..m {
                            r[c * m + row] = if c < k && row < k {
                                v[c * k + row]
                            } else {
                                truth(c == row)
                            };
                        }
                    }
                    r
                }),
            });
        }
        // Otherwise the arguments' components in turn (the last may have
        // more than needed).
        let sizes: Vec<usize> = tes.iter().map(|t| t.ty.size()).collect();
        let total: usize = sizes.iter().sum();
        let before_last: usize = sizes[..sizes.len() - 1].iter().sum();
        if total < n || before_last >= n {
            return Err(wrong());
        }
        let parts: Vec<(Ev, usize)> = tes.into_iter().map(|t| t.ev).zip(sizes).collect();
        Ok(Te {
            ty,
            ev: Arc::new(move |e| {
                let mut r = ZERO;
                let mut k = 0;
                for (ev, s) in &parts {
                    let v = ev(e);
                    for x in v.iter().take(*s) {
                        if k < n {
                            r[k] = conv(*x);
                            k += 1;
                        }
                    }
                }
                r
            }),
        })
    }

    fn block(&mut self, body: &[St]) -> Result<Run, Error> {
        let mut runs = Vec::with_capacity(body.len());
        for s in body {
            runs.push(self.stmt(s)?);
        }
        Ok(Arc::new(move |e| {
            for r in &runs {
                let f = r(e);
                if f != Flow::Next {
                    return f;
                }
            }
            Flow::Next
        }))
    }

    /// A statement in a scope of its own.
    fn scoped(&mut self, s: &St) -> Result<Run, Error> {
        self.scopes.push(HashMap::new());
        let r = self.stmt(s);
        self.scopes.pop();
        r
    }

    /// A loop's body (break and continue allowed).
    fn body(&mut self, s: &St) -> Result<Run, Error> {
        self.loops += 1;
        let r = self.scoped(s);
        self.loops -= 1;
        r
    }

    fn stmt(&mut self, s: &St) -> Result<Run, Error> {
        match s {
            St::Empty => Ok(Arc::new(|_| Flow::Next)),
            St::Expr(e) => {
                let ev = self.expr(e)?.ev;
                Ok(Arc::new(move |e| {
                    ev(e);
                    Flow::Next
                }))
            }
            St::Decl {
                fixed,
                ty,
                vars,
                line,
            } => {
                if matches!(ty, Ty::Void | Ty::Sampler) {
                    return Err(Error::new(
                        *line,
                        format!("a variable cannot be a {}", ty.name()),
                    ));
                }
                let mut stores = Vec::new();
                for (name, size, init) in vars {
                    if let Some(size) = size {
                        let made =
                            self.array(name, *ty, size.as_ref(), init.as_ref(), *fixed, *line)?;
                        stores.extend(made);
                        continue;
                    }
                    if *fixed && let Some(n) = init.as_ref().and_then(|e| self.fold(e)) {
                        self.consts.insert(name.clone(), n);
                    }
                    let value = match init {
                        Some(e) => Some(self.expr_as(e, *ty)?),
                        None if *fixed => {
                            return Err(Error::new(*line, format!("const '{name}' needs a value")));
                        }
                        None => None,
                    };
                    let slot = self.declare(name, *ty, *fixed, *line)?;
                    stores.push((slot, value));
                }
                Ok(Arc::new(move |e| {
                    for (slot, value) in &stores {
                        e.slots[*slot] = match value {
                            Some(v) => v(e),
                            None => ZERO,
                        };
                    }
                    Flow::Next
                }))
            }
            St::Block(body) => {
                self.scopes.push(HashMap::new());
                let r = self.block(body);
                self.scopes.pop();
                r
            }
            St::If(c, then, other) => {
                let c = self.cond(c)?;
                let then = self.scoped(then)?;
                let other = match other {
                    Some(o) => Some(self.scoped(o)?),
                    None => None,
                };
                Ok(Arc::new(move |e| {
                    if c(e)[0] != 0.0 {
                        then(e)
                    } else if let Some(o) = &other {
                        o(e)
                    } else {
                        Flow::Next
                    }
                }))
            }
            St::For {
                init,
                cond,
                step,
                body,
            } => {
                self.scopes.push(HashMap::new());
                let made = self.for_loop(init.as_deref(), cond.as_ref(), step.as_ref(), body);
                self.scopes.pop();
                made
            }
            St::While(c, body) => {
                let c = self.cond(c)?;
                let body = self.body(body)?;
                Ok(looped(Some(c), body, None, true))
            }
            St::Do(body, c) => {
                let body = self.body(body)?;
                let c = self.cond(c)?;
                Ok(looped(Some(c), body, None, false))
            }
            St::Return(value, line) => {
                let ret = self.ret;
                match value {
                    Some(v) => {
                        if ret == Ty::Void {
                            return Err(Error::new(*line, "a void function returns no value"));
                        }
                        let v = self.expr_as(v, ret)?;
                        Ok(Arc::new(move |e| {
                            e.ret = v(e);
                            Flow::Return
                        }))
                    }
                    None if ret != Ty::Void => Err(Error::new(
                        *line,
                        format!("this function must return a {}", ret.name()),
                    )),
                    None => Ok(Arc::new(|_| Flow::Return)),
                }
            }
            St::Break(line) | St::Continue(line) => {
                if self.loops == 0 {
                    return Err(Error::new(*line, "'break' and 'continue' belong in loops"));
                }
                let flow = if matches!(s, St::Break(_)) {
                    Flow::Break
                } else {
                    Flow::Continue
                };
                Ok(Arc::new(move |_| flow))
            }
            St::Discard => Ok(Arc::new(|e| {
                e.discarded = true;
                Flow::Return
            })),
        }
    }

    fn for_loop(
        &mut self,
        init: Option<&St>,
        cond: Option<&E>,
        step: Option<&E>,
        body: &St,
    ) -> Result<Run, Error> {
        let init = match init {
            Some(s) => Some(self.stmt(s)?),
            None => None,
        };
        let cond = match cond {
            Some(c) => Some(self.cond(c)?),
            None => None,
        };
        let step = match step {
            Some(s) => Some(self.expr(s)?.ev),
            None => None,
        };
        let body = self.body(body)?;
        let l = looped(cond, body, step, true);
        Ok(Arc::new(move |e| {
            if let Some(i) = &init {
                i(e);
            }
            l(e)
        }))
    }
}

/// A picture's color as a value.
fn sampled(c: [f32; 4]) -> Val {
    let mut r = ZERO;
    r[..4].copy_from_slice(&c);
    r
}

/// What indexing a `ty` gives: its type, how many there are, and the size
/// of each.
fn indexed(ty: Ty, line: usize) -> Result<(Ty, usize, usize), Error> {
    match ty {
        Ty::Vec(b, n) if n > 1 => Ok((Ty::Vec(b, 1), usize::from(n), 1)),
        Ty::Mat(n) => Ok((Ty::Vec(Base::Float, n), usize::from(n), usize::from(n))),
        _ => Err(Error::new(
            line,
            format!("a {} cannot be indexed", ty.name()),
        )),
    }
}

/// A loop: `cond` before each turn (but the first, unless `first`), then
/// `body`, then `step`.
fn looped(cond: Option<Ev>, body: Run, step: Option<Ev>, first: bool) -> Run {
    Arc::new(move |e| {
        let mut check = first;
        loop {
            if check
                && let Some(c) = &cond
                && c(e)[0] == 0.0
            {
                break;
            }
            check = true;
            match body(e) {
                Flow::Break => break,
                Flow::Return => return Flow::Return,
                Flow::Next | Flow::Continue => {}
            }
            if let Some(s) = &step {
                s(e);
            }
            e.steps += 1;
            if e.steps > STEPS {
                break;
            }
        }
        Flow::Next
    })
}
