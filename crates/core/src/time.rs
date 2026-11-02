//! Tempo em inteiros.
//!
//! Todo instante do BananaV é um [`Tick`]: 1/254.016.000.000 de segundo. O número
//! é 2¹² · 3⁴ · 5⁶ · 7², então a duração de um frame ou de uma amostra é um
//! inteiro exato para todas as taxas usuais — 23,976 / 24 / 25 / 29,97 / 30 /
//! 50 / 59,94 / 60 / 120 fps e 8 / 11,025 / 16 / 22,05 / 32 / 44,1 / 48 / 88,2 /
//! 96 / 176,4 / 192 kHz — e timescales de container como 600, 1000, 12800,
//! 15360 e 90000. Assim, editar em 29,97 nunca acumula erro de arredondamento.
//!
//! Um `i64` de ticks cobre ±420 dias, muito além de qualquer projeto.

use std::fmt;
use std::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};

/// Ticks por segundo.
pub const TICKS_PER_SECOND: i64 = 254_016_000_000;

/// Um instante ou uma duração, em ticks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct Tick(pub i64);

impl Tick {
    pub const ZERO: Tick = Tick(0);
    pub const MAX: Tick = Tick(i64::MAX);
    pub const MIN: Tick = Tick(i64::MIN);
    pub const SECOND: Tick = Tick(TICKS_PER_SECOND);

    pub const fn from_seconds(seconds: i64) -> Tick {
        Tick(seconds * TICKS_PER_SECOND)
    }

    /// Converte segundos fracionários, arredondando para o tick mais próximo.
    /// Use apenas para entrada humana (arrastar com o mouse, campos de texto);
    /// cálculos internos devem ficar em inteiros.
    pub fn from_seconds_f64(seconds: f64) -> Tick {
        Tick((seconds * TICKS_PER_SECOND as f64).round() as i64)
    }

    pub fn as_seconds_f64(self) -> f64 {
        self.0 as f64 / TICKS_PER_SECOND as f64
    }

    /// Converte um valor numa timescale de container (ex.: `mdhd` do MP4) para
    /// ticks, arredondando para baixo. É exato quando `timescale` divide
    /// [`TICKS_PER_SECOND`].
    pub fn from_timescale(value: i64, timescale: u32) -> Tick {
        assert!(timescale > 0, "timescale zero");
        Tick(div_floor_i128(
            value as i128 * TICKS_PER_SECOND as i128,
            timescale as i128,
        ))
    }

    /// Converte ticks para uma timescale de container, arredondando para baixo.
    pub fn to_timescale(self, timescale: u32) -> i64 {
        assert!(timescale > 0, "timescale zero");
        div_floor_i128(self.0 as i128 * timescale as i128, TICKS_PER_SECOND as i128)
    }

    pub const fn abs(self) -> Tick {
        Tick(self.0.abs())
    }

    /// No time at all (serde's `skip_serializing_if`).
    pub fn is_zero(&self) -> bool {
        self.0 == 0
    }

    pub fn min(self, other: Tick) -> Tick {
        if self <= other { self } else { other }
    }

    pub fn max(self, other: Tick) -> Tick {
        if self >= other { self } else { other }
    }

    pub fn clamp(self, lo: Tick, hi: Tick) -> Tick {
        self.max(lo).min(hi)
    }

    pub fn checked_add(self, other: Tick) -> Option<Tick> {
        self.0.checked_add(other.0).map(Tick)
    }

    pub fn saturating_add(self, other: Tick) -> Tick {
        Tick(self.0.saturating_add(other.0))
    }

    pub fn saturating_sub(self, other: Tick) -> Tick {
        Tick(self.0.saturating_sub(other.0))
    }
}

impl Add for Tick {
    type Output = Tick;
    fn add(self, rhs: Tick) -> Tick {
        Tick(self.0 + rhs.0)
    }
}

impl AddAssign for Tick {
    fn add_assign(&mut self, rhs: Tick) {
        self.0 += rhs.0;
    }
}

impl Sub for Tick {
    type Output = Tick;
    fn sub(self, rhs: Tick) -> Tick {
        Tick(self.0 - rhs.0)
    }
}

impl SubAssign for Tick {
    fn sub_assign(&mut self, rhs: Tick) {
        self.0 -= rhs.0;
    }
}

impl Neg for Tick {
    type Output = Tick;
    fn neg(self) -> Tick {
        Tick(-self.0)
    }
}

impl fmt::Display for Tick {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.6}s", self.as_seconds_f64())
    }
}

impl Mul<i64> for Tick {
    type Output = Tick;
    fn mul(self, rhs: i64) -> Tick {
        Tick(self.0 * rhs)
    }
}

/// Divisão com arredondamento para −∞ (a de Rust trunca para zero).
/// O divisor é sempre positivo aqui, e para `b > 0` `div_euclid` é o piso.
pub(crate) fn div_floor_i128(a: i128, b: i128) -> i64 {
    debug_assert!(b > 0);
    a.div_euclid(b) as i64
}

/// Taxa de quadros racional: `num / den` quadros por segundo
/// (29,97 = 30000/1001).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FrameRate {
    pub num: u32,
    pub den: u32,
}

impl FrameRate {
    pub const FPS_23_976: FrameRate = FrameRate::new(24000, 1001);
    pub const FPS_24: FrameRate = FrameRate::new(24, 1);
    pub const FPS_25: FrameRate = FrameRate::new(25, 1);
    pub const FPS_29_97: FrameRate = FrameRate::new(30000, 1001);
    pub const FPS_30: FrameRate = FrameRate::new(30, 1);
    pub const FPS_50: FrameRate = FrameRate::new(50, 1);
    pub const FPS_59_94: FrameRate = FrameRate::new(60000, 1001);
    pub const FPS_60: FrameRate = FrameRate::new(60, 1);

    pub const fn new(num: u32, den: u32) -> FrameRate {
        assert!(num > 0 && den > 0, "taxa de quadros inválida");
        FrameRate { num, den }
    }

    pub fn as_f64(self) -> f64 {
        self.num as f64 / self.den as f64
    }

    /// `true` quando a duração de cada quadro é um número inteiro de ticks.
    pub fn is_exact(self) -> bool {
        (TICKS_PER_SECOND as i128 * self.den as i128) % self.num as i128 == 0
    }

    /// Início do quadro `index` (arredondado para baixo se a taxa não for exata).
    pub fn frame_start(self, index: i64) -> Tick {
        Tick(div_floor_i128(
            index as i128 * self.den as i128 * TICKS_PER_SECOND as i128,
            self.num as i128,
        ))
    }

    /// Duração de um quadro.
    pub fn frame_duration(self) -> Tick {
        self.frame_start(1)
    }

    /// Índice do quadro que contém o instante `t`.
    pub fn frame_at(self, t: Tick) -> i64 {
        div_floor_i128(
            t.0 as i128 * self.num as i128,
            self.den as i128 * TICKS_PER_SECOND as i128,
        )
    }

    /// Arredonda `t` para baixo, até o início do quadro que o contém.
    pub fn snap_floor(self, t: Tick) -> Tick {
        self.frame_start(self.frame_at(t))
    }

    /// Arredonda `t` para o limite de quadro mais próximo.
    pub fn snap_nearest(self, t: Tick) -> Tick {
        let lo = self.snap_floor(t);
        let hi = self.frame_start(self.frame_at(t) + 1);
        if (t - lo) <= (hi - t) { lo } else { hi }
    }

    /// Base inteira de quadros por segundo usada em timecode (30 para 29,97).
    pub fn timecode_base(self) -> u32 {
        (self.num + self.den / 2) / self.den
    }

    /// Taxas NTSC (29,97 e 59,94) usam timecode drop-frame.
    pub fn is_drop_frame(self) -> bool {
        self.den == 1001 && (self.num == 30000 || self.num == 60000)
    }
}

impl fmt::Display for FrameRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 {
            write!(f, "{} fps", self.num)
        } else {
            write!(f, "{:.3} fps", self.as_f64())
        }
    }
}

/// Taxa de amostragem de áudio, em Hz.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct SampleRate(pub u32);

impl SampleRate {
    pub const HZ_44100: SampleRate = SampleRate(44_100);
    pub const HZ_48000: SampleRate = SampleRate(48_000);

    pub fn is_exact(self) -> bool {
        TICKS_PER_SECOND % self.0 as i64 == 0
    }

    pub fn sample_start(self, index: i64) -> Tick {
        Tick(div_floor_i128(
            index as i128 * TICKS_PER_SECOND as i128,
            self.0 as i128,
        ))
    }

    pub fn sample_duration(self) -> Tick {
        self.sample_start(1)
    }

    /// Índice da amostra que contém o instante `t`.
    pub fn sample_at(self, t: Tick) -> i64 {
        div_floor_i128(t.0 as i128 * self.0 as i128, TICKS_PER_SECOND as i128)
    }
}

/// Intervalo semiaberto `[start, start + duration)`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TimeRange {
    pub start: Tick,
    pub duration: Tick,
}

impl TimeRange {
    pub fn new(start: Tick, duration: Tick) -> TimeRange {
        debug_assert!(duration >= Tick::ZERO, "duração negativa");
        TimeRange { start, duration }
    }

    pub fn from_bounds(start: Tick, end: Tick) -> TimeRange {
        TimeRange::new(start, end - start)
    }

    pub fn end(self) -> Tick {
        self.start + self.duration
    }

    pub fn is_empty(self) -> bool {
        self.duration <= Tick::ZERO
    }

    pub fn contains(self, t: Tick) -> bool {
        t >= self.start && t < self.end()
    }

    pub fn intersects(self, other: TimeRange) -> bool {
        self.start < other.end() && other.start < self.end()
    }

    pub fn intersection(self, other: TimeRange) -> Option<TimeRange> {
        let start = self.start.max(other.start);
        let end = self.end().min(other.end());
        (start < end).then(|| TimeRange::from_bounds(start, end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIDEO_RATES: [FrameRate; 8] = [
        FrameRate::FPS_23_976,
        FrameRate::FPS_24,
        FrameRate::FPS_25,
        FrameRate::FPS_29_97,
        FrameRate::FPS_30,
        FrameRate::FPS_50,
        FrameRate::FPS_59_94,
        FrameRate::FPS_60,
    ];

    #[test]
    fn todas_as_taxas_usuais_sao_exatas() {
        for rate in VIDEO_RATES {
            assert!(rate.is_exact(), "{rate}");
        }
        assert!(FrameRate::new(120, 1).is_exact());
        assert!(FrameRate::new(120000, 1001).is_exact());
        for hz in [
            8000, 11025, 16000, 22050, 32000, 44100, 48000, 88200, 96000, 176400, 192000,
        ] {
            assert!(SampleRate(hz).is_exact(), "{hz} Hz");
        }
        for ts in [
            600u32, 1000, 12800, 15360, 24000, 30000, 44100, 48000, 90000,
        ] {
            assert_eq!(TICKS_PER_SECOND % ts as i64, 0, "timescale {ts}");
        }
    }

    #[test]
    fn uma_hora_em_29_97_nao_acumula_erro() {
        let rate = FrameRate::FPS_29_97;
        // 1 hora de vídeo NTSC tem 107.892 quadros (drop-frame) = 3600,0 s * 29,97.
        let frames = 107_892;
        let t = rate.frame_start(frames);
        assert_eq!(t, Tick(frames * (1001 * TICKS_PER_SECOND / 30000)));
        assert_eq!(rate.frame_at(t), frames);
        assert_eq!(rate.frame_at(t - Tick(1)), frames - 1);
        // Somar a duração de um quadro N vezes dá exatamente o mesmo instante.
        let mut acc = Tick::ZERO;
        for _ in 0..frames {
            acc += rate.frame_duration();
        }
        assert_eq!(acc, t);
    }

    #[test]
    fn frame_at_e_frame_start_sao_inversos() {
        for rate in VIDEO_RATES {
            for i in [-5i64, -1, 0, 1, 2, 1000, 1_000_000] {
                let t = rate.frame_start(i);
                assert_eq!(rate.frame_at(t), i, "{rate} quadro {i}");
                assert_eq!(rate.snap_floor(t + rate.frame_duration() - Tick(1)), t);
            }
        }
    }

    #[test]
    fn instantes_negativos_arredondam_para_menos_infinito() {
        let rate = FrameRate::FPS_25;
        assert_eq!(rate.frame_at(Tick(-1)), -1);
        assert_eq!(SampleRate::HZ_48000.sample_at(Tick(-1)), -1);
        assert_eq!(Tick(-1).to_timescale(1000), -1);
    }

    #[test]
    fn conversao_de_timescale() {
        assert_eq!(Tick::from_timescale(90000, 90000), Tick::SECOND);
        assert_eq!(
            Tick::from_timescale(3003, 90000),
            FrameRate::FPS_29_97.frame_duration()
        );
        assert_eq!(
            FrameRate::FPS_29_97.frame_duration().to_timescale(90000),
            3003
        );
        assert_eq!(Tick::from_timescale(1024, 48000).to_timescale(48000), 1024);
    }

    #[test]
    fn amostras() {
        let sr = SampleRate::HZ_48000;
        assert_eq!(sr.sample_start(48000), Tick::SECOND);
        assert_eq!(sr.sample_at(Tick::SECOND - Tick(1)), 47999);
        assert_eq!(SampleRate::HZ_44100.sample_duration() * 1, Tick(5_760_000));
    }

    #[test]
    fn snap_nearest() {
        let rate = FrameRate::FPS_25;
        let d = rate.frame_duration();
        assert_eq!(
            rate.snap_nearest(Tick(d.0 * 3 + d.0 / 2 - 1)),
            Tick(d.0 * 3)
        );
        assert_eq!(
            rate.snap_nearest(Tick(d.0 * 3 + d.0 / 2 + 1)),
            Tick(d.0 * 4)
        );
    }

    #[test]
    fn intervalos() {
        let a = TimeRange::new(Tick(0), Tick(10));
        let b = TimeRange::new(Tick(5), Tick(10));
        let c = TimeRange::new(Tick(10), Tick(1));
        assert!(a.intersects(b));
        assert!(!a.intersects(c));
        assert!(a.contains(Tick(9)) && !a.contains(Tick(10)));
        assert_eq!(a.intersection(b), Some(TimeRange::new(Tick(5), Tick(5))));
        assert_eq!(a.intersection(c), None);
    }
}
