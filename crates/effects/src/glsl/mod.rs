//! GLSL run on the processor, for GL Transitions' code typed in (VEGAS
//! 2026 compiles it for the graphics card): a fragment shader's GLSL ES
//! 1.0, and the parts of GLSL 3 such code uses — scalars, vectors and
//! square matrices of floats, ints and bools, their operators,
//! constructors and swizzles, functions (never recursive, as GLSL wants),
//! `in`, `out` and `inout` parameters, `if`, `for`, `while`, `do`,
//! `discard` and GLSL's built-in functions —, read by [`lex`] (with a small
//! preprocessor) and [`parse`], checked and made into closures by
//! [`compile`]. GL Transitions' harness is given as VEGAS gives it:
//! `progress`, `ratio`, `getFromColor` and `getToColor` (and the pictures
//! as the samplers `fromImage` and `toImage`, for `texture2D`). Arrays are read (their
//! sizes constants; not as parameters), macros with parameters, and
//! structs of numbers (16 at most); derivatives (`dFdx`) are not yet.

mod compile;
mod lex;
mod parse;
pub(crate) mod wgsl;

pub(crate) use compile::{Inputs, Program, Val};

/// What is wrong with the code, and on which line.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Error {
    pub(crate) line: usize,
    pub(crate) message: String,
}

impl Error {
    fn new(line: usize, message: impl Into<String>) -> Error {
        Error {
            line,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

/// What a value's numbers are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Base {
    Float,
    Int,
    Bool,
}

/// A GLSL type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ty {
    Void,
    /// A scalar (1) or a vector (2 to 4).
    Vec(Base, u8),
    /// A square matrix of floats, 2 to 4 columns.
    Mat(u8),
    Sampler,
    /// A struct: its number among the code's structs, and how many
    /// numbers its fields hold (16 at most, one after another).
    Struct(u8, u8),
}

impl Ty {
    pub(crate) const FLOAT: Ty = Ty::Vec(Base::Float, 1);
    pub(crate) const INT: Ty = Ty::Vec(Base::Int, 1);
    pub(crate) const BOOL: Ty = Ty::Vec(Base::Bool, 1);
    pub(crate) const VEC2: Ty = Ty::Vec(Base::Float, 2);
    pub(crate) const VEC4: Ty = Ty::Vec(Base::Float, 4);

    /// How many numbers it holds.
    pub(crate) fn size(self) -> usize {
        match self {
            Ty::Vec(_, n) => usize::from(n),
            Ty::Mat(n) => usize::from(n) * usize::from(n),
            Ty::Struct(_, n) => usize::from(n),
            Ty::Void | Ty::Sampler => 0,
        }
    }

    /// The type named `name` (`vec3`, `mat2`…).
    pub(crate) fn named(name: &str) -> Option<Ty> {
        let vec = |b: Base, n: &str| match n {
            "2" => Some(Ty::Vec(b, 2)),
            "3" => Some(Ty::Vec(b, 3)),
            "4" => Some(Ty::Vec(b, 4)),
            _ => None,
        };
        match name {
            "void" => Some(Ty::Void),
            "float" => Some(Ty::FLOAT),
            "int" | "uint" => Some(Ty::INT),
            "bool" => Some(Ty::BOOL),
            "mat2" | "mat2x2" => Some(Ty::Mat(2)),
            "mat3" | "mat3x3" => Some(Ty::Mat(3)),
            "mat4" | "mat4x4" => Some(Ty::Mat(4)),
            "sampler2D" => Some(Ty::Sampler),
            _ => {
                if let Some(n) = name.strip_prefix("vec") {
                    vec(Base::Float, n)
                } else if let Some(n) = name.strip_prefix("ivec").or(name.strip_prefix("uvec")) {
                    vec(Base::Int, n)
                } else if let Some(n) = name.strip_prefix("bvec") {
                    vec(Base::Bool, n)
                } else {
                    None
                }
            }
        }
    }

    /// Its name in GLSL, for messages.
    pub(crate) fn name(self) -> String {
        match self {
            Ty::Void => "void".into(),
            Ty::Vec(Base::Float, 1) => "float".into(),
            Ty::Vec(Base::Int, 1) => "int".into(),
            Ty::Vec(Base::Bool, 1) => "bool".into(),
            Ty::Vec(Base::Float, n) => format!("vec{n}"),
            Ty::Vec(Base::Int, n) => format!("ivec{n}"),
            Ty::Vec(Base::Bool, n) => format!("bvec{n}"),
            Ty::Mat(n) => format!("mat{n}"),
            Ty::Sampler => "sampler2D".into(),
            Ty::Struct(..) => "struct".into(),
        }
    }
}

#[cfg(test)]
mod tests;
