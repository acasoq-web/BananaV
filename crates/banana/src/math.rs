//! The vectors and small functions the kernels' twins on the processor use
//! (`generated.rs`), as the shading language has them.
// Not every function is one a kernel calls today.
#![allow(dead_code)]

use std::ops::{Add, Div, Mul, Neg, Rem, Sub};

/// `N` numbers worked on together.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vn<T, const N: usize>(pub [T; N]);

pub type V2 = Vn<f32, 2>;
pub type V4 = Vn<f32, 4>;
pub type I2 = Vn<i32, 2>;

impl<T: Copy, const N: usize> Vn<T, N> {
    #[inline(always)]
    pub fn map<U: Copy>(self, f: impl Fn(T) -> U) -> Vn<U, N> {
        Vn(self.0.map(f))
    }

    #[inline(always)]
    pub fn zip<O: Copy, U: Copy>(self, o: Vn<O, N>, f: impl Fn(T, O) -> U) -> Vn<U, N> {
        Vn(std::array::from_fn(|i| f(self.0[i], o.0[i])))
    }

    #[inline(always)]
    pub fn zip3<U: Copy>(self, a: Vn<T, N>, b: Vn<T, N>, f: impl Fn(T, T, T) -> U) -> Vn<U, N> {
        Vn(std::array::from_fn(|i| f(self.0[i], a.0[i], b.0[i])))
    }
}

macro_rules! ops {
    ($($op:ident $f:ident $sign:tt),*) => {
        $(impl<const N: usize> $op for Vn<f32, N> {
            type Output = Self;
            #[inline(always)]
            fn $f(self, o: Self) -> Self {
                self.zip(o, |a, b| a $sign b)
            }
        })*
    };
}

ops!(Add add +, Sub sub -, Mul mul *, Div div /, Rem rem %);

impl<const N: usize> Neg for Vn<f32, N> {
    type Output = Self;
    #[inline(always)]
    fn neg(self) -> Self {
        self.map(|a| -a)
    }
}

impl<const N: usize> Vn<f32, N> {
    #[inline(always)]
    pub fn dot(self, o: Self) -> f32 {
        let mut s = 0.0;
        for i in 0..N {
            s += self.0[i] * o.0[i];
        }
        s
    }

    #[inline(always)]
    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    #[inline(always)]
    pub fn normalize(self) -> Self {
        let l = self.length();
        self.map(|a| a / l)
    }
}

impl<const N: usize> Vn<bool, N> {
    #[inline(always)]
    pub fn all(self) -> bool {
        self.0.iter().all(|&b| b)
    }

    #[inline(always)]
    pub fn any(self) -> bool {
        self.0.iter().any(|&b| b)
    }
}

/// Each of `a`'s where `c` holds, `r`'s elsewhere.
#[inline(always)]
pub fn pick<T: Copy, const N: usize>(c: Vn<bool, N>, a: Vn<T, N>, r: Vn<T, N>) -> Vn<T, N> {
    Vn(std::array::from_fn(
        |i| if c.0[i] { a.0[i] } else { r.0[i] },
    ))
}

#[inline(always)]
pub fn cross(a: Vn<f32, 3>, b: Vn<f32, 3>) -> Vn<f32, 3> {
    let ([ax, ay, az], [bx, by, bz]) = (a.0, b.0);
    Vn([ay * bz - az * by, az * bx - ax * bz, ax * by - ay * bx])
}

#[inline(always)]
pub fn reflect<const N: usize>(i: Vn<f32, N>, n: Vn<f32, N>) -> Vn<f32, N> {
    let d = 2.0 * n.dot(i);
    i.zip(n, |a, b| a - b * d)
}

#[inline(always)]
pub fn clampf(x: f32, lo: f32, hi: f32) -> f32 {
    x.max(lo).min(hi)
}

#[inline(always)]
pub fn clampi(x: i32, lo: i32, hi: i32) -> i32 {
    x.max(lo).min(hi)
}

#[inline(always)]
pub fn clampu(x: u32, lo: u32, hi: u32) -> u32 {
    x.max(lo).min(hi)
}

#[inline(always)]
pub fn i32_min(a: i32, b: i32) -> i32 {
    a.min(b)
}

#[inline(always)]
pub fn i32_max(a: i32, b: i32) -> i32 {
    a.max(b)
}

#[inline(always)]
pub fn u32_min(a: u32, b: u32) -> u32 {
    a.min(b)
}

#[inline(always)]
pub fn u32_max(a: u32, b: u32) -> u32 {
    a.max(b)
}

// What is no number becomes 0, as the card's `clamp` mostly has it.
#[allow(clippy::manual_clamp)]
#[inline(always)]
pub fn saturate(x: f32) -> f32 {
    x.max(0.0).min(1.0)
}

#[inline(always)]
pub fn fract(x: f32) -> f32 {
    x - x.floor()
}

#[inline(always)]
pub fn inverse_sqrt(x: f32) -> f32 {
    1.0 / x.sqrt()
}

/// The sign: 0 for 0.
#[inline(always)]
pub fn signf(x: f32) -> f32 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

#[inline(always)]
pub fn step(edge: f32, x: f32) -> f32 {
    if x >= edge { 1.0 } else { 0.0 }
}

#[inline(always)]
pub fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = saturate((x - a) / (b - a));
    t * t * (3.0 - 2.0 * t)
}

#[inline(always)]
pub fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Whole numbers divided as the shading language does: by 0, the number.
pub trait Whole: Copy {
    fn idiv(self, o: Self) -> Self;
    fn imod(self, o: Self) -> Self;
}

impl Whole for i32 {
    #[inline(always)]
    fn idiv(self, o: i32) -> i32 {
        self.checked_div(o).unwrap_or(self)
    }

    #[inline(always)]
    fn imod(self, o: i32) -> i32 {
        self.checked_rem(o).unwrap_or(0)
    }
}

impl Whole for u32 {
    #[inline(always)]
    fn idiv(self, o: u32) -> u32 {
        self.checked_div(o).unwrap_or(self)
    }

    #[inline(always)]
    fn imod(self, o: u32) -> u32 {
        self.checked_rem(o).unwrap_or(0)
    }
}

#[inline(always)]
pub fn idiv<T: Whole>(a: T, b: T) -> T {
    a.idiv(b)
}

#[inline(always)]
pub fn imod<T: Whole>(a: T, b: T) -> T {
    a.imod(b)
}
