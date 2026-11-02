//! A complex fast Fourier transform (radix 2, in place) and the
//! cross-correlation of two signals through it, for finding where two
//! recordings of one sound line up (`align.rs`).

use std::ops::{Add, Mul, Sub};

/// A complex number.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Complex {
    pub re: f64,
    pub im: f64,
}

impl Complex {
    pub const ZERO: Complex = Complex { re: 0.0, im: 0.0 };

    pub const fn new(re: f64, im: f64) -> Complex {
        Complex { re, im }
    }

    pub fn conj(self) -> Complex {
        Complex::new(self.re, -self.im)
    }
}

impl Add for Complex {
    type Output = Complex;
    fn add(self, o: Complex) -> Complex {
        Complex::new(self.re + o.re, self.im + o.im)
    }
}

impl Sub for Complex {
    type Output = Complex;
    fn sub(self, o: Complex) -> Complex {
        Complex::new(self.re - o.re, self.im - o.im)
    }
}

impl Mul for Complex {
    type Output = Complex;
    fn mul(self, o: Complex) -> Complex {
        Complex::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }
}

/// Transforms `data` in place, its length a power of two: forward, or with
/// `inverse` back (scaled by 1 ⁄ n, so a round trip gives the signal back).
pub fn fft(data: &mut [Complex], inverse: bool) {
    let n = data.len();
    assert!(n.is_power_of_two(), "the FFT's length is a power of two");
    if n < 2 {
        return;
    }
    let bits = n.trailing_zeros();
    for i in 0..n {
        let j = i.reverse_bits() >> (usize::BITS - bits);
        if j > i {
            data.swap(i, j);
        }
    }
    // Each twiddle worked out on its own: no error builds up along a stage.
    let sign = if inverse { 1.0 } else { -1.0 };
    let twiddles: Vec<Complex> = (0..n / 2)
        .map(|k| {
            let a = sign * std::f64::consts::TAU * k as f64 / n as f64;
            Complex::new(a.cos(), a.sin())
        })
        .collect();
    let mut len = 2;
    while len <= n {
        let half = len / 2;
        let step = n / len;
        for start in (0..n).step_by(len) {
            for k in 0..half {
                let u = data[start + k];
                let v = data[start + k + half] * twiddles[k * step];
                data[start + k] = u + v;
                data[start + k + half] = u - v;
            }
        }
        len <<= 1;
    }
    if inverse {
        let s = 1.0 / n as f64;
        for d in data {
            d.re *= s;
            d.im *= s;
        }
    }
}

/// The spectrum of real signal `x`, padded with zeros to `n` (a power of
/// two).
pub fn spectrum(x: &[f64], n: usize) -> Vec<Complex> {
    let mut v = vec![Complex::ZERO; n];
    for (d, &s) in v.iter_mut().zip(x) {
        d.re = s;
    }
    fft(&mut v, false);
    v
}

/// The cross-correlation of the signals whose spectra (of one length, each
/// padded past both signals' lengths together) are `a` and `b`: element `k`
/// is Σᵢ a[i + k] · b[i], negative `k`s from the end (`n + k`). `b`'s room
/// is reused.
pub fn correlate(a: &[Complex], mut b: Vec<Complex>) -> Vec<f64> {
    for (y, &x) in b.iter_mut().zip(a) {
        *y = x * y.conj();
    }
    fft(&mut b, true);
    b.into_iter().map(|z| z.re).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_round_trip_gives_the_signal_back() {
        let x: Vec<f64> = (0..64).map(|i| ((i * 37) % 11) as f64 - 5.0).collect();
        let mut s = spectrum(&x, 64);
        fft(&mut s, true);
        for (a, b) in s.iter().zip(&x) {
            assert!((a.re - b).abs() < 1e-9 && a.im.abs() < 1e-9);
        }
    }

    #[test]
    fn a_sine_lands_in_its_bin() {
        let n = 32;
        let x: Vec<f64> = (0..n)
            .map(|i| (std::f64::consts::TAU * 3.0 * i as f64 / n as f64).cos())
            .collect();
        let s = spectrum(&x, n);
        for (k, z) in s.iter().enumerate() {
            let m = (z.re * z.re + z.im * z.im).sqrt();
            let expected = if k == 3 || k == n - 3 {
                n as f64 / 2.0
            } else {
                0.0
            };
            assert!((m - expected).abs() < 1e-9, "bin {k}: {m}");
        }
    }

    #[test]
    fn the_correlation_matches_the_direct_sum() {
        let a: Vec<f64> = (0..20).map(|i| ((i * 7) % 5) as f64 - 2.0).collect();
        let b: Vec<f64> = (0..9).map(|i| ((i * 3) % 4) as f64 - 1.5).collect();
        let n = (a.len() + b.len()).next_power_of_two();
        let r = correlate(&spectrum(&a, n), spectrum(&b, n));
        for k in -(b.len() as i64 - 1)..a.len() as i64 {
            let direct: f64 = (0..b.len() as i64)
                .filter(|&i| (0..a.len() as i64).contains(&(i + k)))
                .map(|i| a[(i + k) as usize] * b[i as usize])
                .sum();
            let got = r[k.rem_euclid(n as i64) as usize];
            assert!((got - direct).abs() < 1e-9, "lag {k}: {got} vs {direct}");
        }
    }
}
