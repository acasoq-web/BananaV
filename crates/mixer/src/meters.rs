//! The Mixing Console's VU and PPM meters, as VEGAS offers them: the
//! ballistics of IEC 60268-17's VU meter (full-wave averaging, critically
//! damped, 99 % of a step in 300 ms; Vintage Mode an analog needle's 1.5 %
//! overshoot) and of IEC 60268-10's peak programme meters — Type I (DIN,
//! Nordic) integrating in 5 ms and falling 20 dB in 1.7 s, Type II (UK,
//! EBU) in 10 ms and 24 dB in 2.8 s (an integration time: a tone burst that
//! long reads 2 dB low) — on each channel of a track's output, a bus, a
//! chain or the Master. Readings are levels a sine of that amplitude would
//! give (AES17: a full-scale sine is 0 dBFS); [`MeterKind::position`]
//! places them on each meter's scale, aligned as EBU R68 (0 dBu at
//! −18 dBFS).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};

use bv_core::AudioBuffer;
use bv_timeline::{Strip, TrackId};

/// The VU/PPM meters' kinds (the meters' right-click menu).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MeterKind {
    /// −10 to +2 VU on a scale linear in voltage; 0 VU is +4 dBu.
    #[default]
    TraditionalVu,
    /// −30 to +8 VU, linear in voltage.
    ExtendedVu,
    /// −30 to +8 VU, linear in decibels.
    LogarithmicVu,
    /// The BBC's marks 1 to 7: −12 to +12 dBu, 4 dB a mark.
    UkPpm,
    /// −12 to +12 dB, 0 at 0 dBu.
    EbuPpm,
    /// −50 to +5 dB, 0 at +6 dBu.
    DinPpm,
    /// −42 to +12 dB, 0 at 0 dBu.
    NordicPpm,
}

/// The level of 0 dBu (dBFS): EBU R68's alignment.
pub const ZERO_DBU_DBFS: f32 = -18.0;

/// The VU's time constant (two equal poles: critically damped, 99 % of a
/// step in 300 ms).
const VU_TAU: f32 = 0.045_19;
/// Vintage Mode's needle: damping 0.8 (a 1.5 % overshoot) and the natural
/// frequency that first reaches 99 % in 300 ms (rad/s).
const VINTAGE_ZETA: f32 = 0.800_75;
const VINTAGE_OMEGA: f32 = 13.126;
/// The PPMs' charging time constants — a 5 kHz tone burst 5 ms (Type I)
/// or 10 ms (Type II) long reads 2 dB low through the rectifier, as IEC
/// 60268-10 defines their integration times — and falls (dB a second).
const TYPE_I_TAU: f32 = 0.001_276;
const TYPE_II_TAU: f32 = 0.002_552;
const TYPE_I_FALL: f32 = 20.0 / 1.7;
const TYPE_II_FALL: f32 = 24.0 / 2.8;

impl MeterKind {
    pub const ALL: [MeterKind; 7] = [
        MeterKind::TraditionalVu,
        MeterKind::ExtendedVu,
        MeterKind::LogarithmicVu,
        MeterKind::UkPpm,
        MeterKind::EbuPpm,
        MeterKind::DinPpm,
        MeterKind::NordicPpm,
    ];

    /// Its name in the meters' menu (English: the interface translates).
    pub fn name(self) -> &'static str {
        match self {
            MeterKind::TraditionalVu => "Traditional VU",
            MeterKind::ExtendedVu => "Extended VU",
            MeterKind::LogarithmicVu => "Logarithmic VU",
            MeterKind::UkPpm => "UK PPM",
            MeterKind::EbuPpm => "EBU PPM",
            MeterKind::DinPpm => "DIN PPM",
            MeterKind::NordicPpm => "Nordic PPM",
        }
    }

    pub fn is_vu(self) -> bool {
        matches!(
            self,
            MeterKind::TraditionalVu | MeterKind::ExtendedVu | MeterKind::LogarithmicVu
        )
    }

    /// A PPM's integration time constant (s) and fall (dB a second).
    fn ppm(self) -> (f32, f32) {
        match self {
            MeterKind::DinPpm | MeterKind::NordicPpm => (TYPE_I_TAU, TYPE_I_FALL),
            _ => (TYPE_II_TAU, TYPE_II_FALL),
        }
    }

    /// A reading (dBFS) in the meter's own unit: VU, dB (DIN, EBU,
    /// Nordic) or the UK's mark (1 to 7).
    pub fn value(self, dbfs: f32) -> f32 {
        let dbu = dbfs - ZERO_DBU_DBFS;
        match self {
            MeterKind::TraditionalVu | MeterKind::ExtendedVu | MeterKind::LogarithmicVu => {
                dbu - 4.0
            }
            MeterKind::UkPpm => 4.0 + dbu / 4.0,
            MeterKind::EbuPpm | MeterKind::NordicPpm => dbu,
            MeterKind::DinPpm => dbu - 6.0,
        }
    }

    /// The scale's ends in the meter's unit.
    pub fn range(self) -> (f32, f32) {
        match self {
            MeterKind::TraditionalVu => (-10.0, 2.0),
            MeterKind::ExtendedVu | MeterKind::LogarithmicVu => (-30.0, 8.0),
            MeterKind::UkPpm => (1.0, 7.0),
            MeterKind::EbuPpm => (-12.0, 12.0),
            MeterKind::DinPpm => (-50.0, 5.0),
            MeterKind::NordicPpm => (-42.0, 12.0),
        }
    }

    /// Where a value in the meter's unit sits on its scale, 0 (bottom) to
    /// 1 (top): the VUs linear in voltage (but the logarithmic one), the
    /// DIN's lower part (−50 to −20 dB) drawn in its bottom third, the
    /// others linear in decibels.
    pub fn place(self, value: f32) -> f32 {
        let (lo, hi) = self.range();
        let v = value.clamp(lo, hi);
        let volts = |db: f32| 10f32.powf(db / 20.0);
        let p = match self {
            MeterKind::TraditionalVu | MeterKind::ExtendedVu => {
                (volts(v) - volts(lo)) / (volts(hi) - volts(lo))
            }
            MeterKind::DinPpm if v < -20.0 => (v - lo) / 30.0 * (1.0 / 3.0),
            MeterKind::DinPpm => 1.0 / 3.0 + (v + 20.0) / 25.0 * (2.0 / 3.0),
            _ => (v - lo) / (hi - lo),
        };
        p.clamp(0.0, 1.0)
    }

    /// Where a reading (dBFS) sits on the scale (0 to 1).
    pub fn position(self, dbfs: f32) -> f32 {
        if dbfs <= -150.0 {
            return 0.0;
        }
        self.place(self.value(dbfs))
    }

    /// The scale's labelled marks, in the meter's unit.
    pub fn marks(self) -> &'static [f32] {
        match self {
            MeterKind::TraditionalVu => &[-10.0, -7.0, -5.0, -3.0, -2.0, -1.0, 0.0, 1.0, 2.0],
            MeterKind::ExtendedVu => &[-30.0, -20.0, -10.0, -5.0, 0.0, 3.0, 5.0, 8.0],
            MeterKind::LogarithmicVu => &[-30.0, -24.0, -18.0, -12.0, -6.0, 0.0, 6.0],
            MeterKind::UkPpm => &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0],
            MeterKind::EbuPpm => &[-12.0, -8.0, -4.0, 0.0, 4.0, 8.0, 12.0],
            MeterKind::DinPpm => &[-50.0, -40.0, -30.0, -20.0, -10.0, -5.0, 0.0, 5.0],
            MeterKind::NordicPpm => &[
                -42.0, -36.0, -30.0, -24.0, -18.0, -12.0, -6.0, 0.0, 6.0, 12.0,
            ],
        }
    }

    /// The scale's reference (0 VU, the PPMs' 0 or mark 6): above it the
    /// meter turns red.
    pub fn reference(self) -> f32 {
        match self {
            MeterKind::UkPpm => 6.0,
            _ => 0.0,
        }
    }
}

/// What a meter measures: a track's output, a bus's or chain's, the
/// Master's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MeterOf {
    Track(TrackId),
    Strip(Strip),
    Master,
}

/// Off, or the kind of the meters computed (and Vintage Mode): what the
/// Mixing Console's VU Meters region shows.
static VU_METERS: AtomicU8 = AtomicU8::new(0);

/// Sets the VU/PPM meters computed (`None`: none, the default — the
/// Mixing Console's VU Meters region hidden).
pub fn set_vu_meters(kind: Option<MeterKind>, vintage: bool) {
    let v = match kind {
        None => 0,
        Some(k) => {
            let i = MeterKind::ALL.iter().position(|x| *x == k).unwrap_or(0) as u8 + 1;
            i | if vintage { 0x80 } else { 0 }
        }
    };
    VU_METERS.store(v, Ordering::Relaxed);
}

/// The meters computed, if any: their kind and Vintage Mode.
pub fn vu_meters() -> Option<(MeterKind, bool)> {
    let v = VU_METERS.load(Ordering::Relaxed);
    let i = (v & 0x7F) as usize;
    (i > 0).then(|| (MeterKind::ALL[(i - 1).min(6)], v & 0x80 != 0))
}

/// One channel's needle.
#[derive(Clone, Copy, Debug, Default)]
struct Needle {
    /// VU: the two poles (Vintage: the needle and its speed); PPM: the
    /// level held.
    a: f32,
    b: f32,
}

impl Needle {
    /// The needle through `samples` at `rate`; the reading (a sine's
    /// amplitude).
    fn run(&mut self, kind: MeterKind, vintage: bool, rate: f32, samples: &[f32]) -> f32 {
        if kind.is_vu() {
            // A sine's rectified mean is 2/π of its amplitude.
            const SINE: f32 = std::f32::consts::FRAC_PI_2;
            if vintage {
                let dt = 1.0 / rate;
                let w2 = VINTAGE_OMEGA * VINTAGE_OMEGA;
                let damp = 2.0 * VINTAGE_ZETA * VINTAGE_OMEGA;
                for s in samples {
                    let speed = self.b + (w2 * (s.abs() - self.a) - damp * self.b) * dt;
                    self.b = speed;
                    self.a += speed * dt;
                }
                return self.a.max(0.0) * SINE;
            }
            let k = 1.0 - (-1.0 / (VU_TAU * rate)).exp();
            for s in samples {
                self.a += k * (s.abs() - self.a);
                self.b += k * (self.a - self.b);
            }
            return self.b * SINE;
        }
        let (tau, fall) = kind.ppm();
        let k = 1.0 - (-1.0 / (tau * rate)).exp();
        let down = 10f32.powf(-fall / (20.0 * rate));
        for s in samples {
            let x = s.abs();
            if x > self.a {
                self.a += k * (x - self.a);
            } else {
                self.a *= down;
            }
        }
        self.a
    }
}

/// One meter's needles, the next sample they expect and the setting they
/// follow.
#[derive(Debug)]
struct Needles {
    each: Vec<Needle>,
    next: i64,
    setting: (MeterKind, bool),
}

/// The meters' needles, by what they measure, and the readings of the
/// last block.
#[derive(Debug, Default)]
pub(crate) struct Meters {
    needles: HashMap<MeterOf, Needles>,
    pub(crate) readings: Vec<(MeterOf, Vec<f32>)>,
}

impl Meters {
    /// Measures `buf` (the samples from `start`) for `of`; a jump or
    /// another kind starts its needles at rest.
    pub(crate) fn measure(&mut self, of: MeterOf, start: i64, buf: &AudioBuffer) {
        let Some(setting) = vu_meters() else {
            self.needles.clear();
            return;
        };
        let frames = buf.channels.first().map_or(0, Vec::len);
        let (kind, vintage) = setting;
        let at_rest = || Needles {
            each: vec![Needle::default(); buf.channels.len()],
            next: start,
            setting,
        };
        let n = self.needles.entry(of).or_insert_with(at_rest);
        if n.next != start || n.setting != setting || n.each.len() != buf.channels.len() {
            *n = at_rest();
        }
        let rate = buf.sample_rate.0 as f32;
        let reading = n
            .each
            .iter_mut()
            .zip(&buf.channels)
            .map(|(needle, ch)| needle.run(kind, vintage, rate, ch))
            .collect();
        n.next = start + frames as i64;
        self.readings.push((of, reading));
    }

    pub(crate) fn clear(&mut self) {
        self.needles.clear();
        self.readings.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(amp: f32, hz: f32, rate: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (std::f32::consts::TAU * hz * i as f32 / rate).sin())
            .collect()
    }

    /// The VU reads a sine's amplitude once settled, 99 % of it after
    /// 300 ms; Vintage Mode overshoots by about 1.5 %.
    #[test]
    fn the_vu_rises_in_300_ms() {
        let rate = 48_000.0;
        let tone = sine(0.5, 1000.0, rate, 48_000);
        let mut n = Needle::default();
        let at_300 = n.run(MeterKind::TraditionalVu, false, rate, &tone[..14_400]);
        assert!((at_300 / 0.5 - 0.99).abs() < 0.01, "{at_300}");
        let settled = n.run(MeterKind::TraditionalVu, false, rate, &tone[14_400..]);
        assert!((settled / 0.5 - 1.0).abs() < 0.01, "{settled}");
        let mut v = Needle::default();
        let mut most = 0f32;
        for block in tone.chunks(480) {
            most = most.max(v.run(MeterKind::TraditionalVu, true, rate, block));
        }
        assert!((most / 0.5 - 1.015).abs() < 0.006, "{most}");
    }

    /// A PPM catches a burst as its type says (a 5 ms or 10 ms one reads
    /// about 2 dB low) and falls 20 dB in 1.7 s (Type I) or 24 dB in 2.8 s
    /// (Type II).
    #[test]
    fn ppms_integrate_and_fall_by_their_type() {
        let rate = 48_000.0;
        let db = |x: f32| 20.0 * x.log10();
        for (kind, burst, fall_s, fall_db) in [
            (MeterKind::DinPpm, 240, 1.7, 20.0),
            (MeterKind::UkPpm, 480, 2.8, 24.0),
        ] {
            let tone = sine(1.0, 5000.0, rate, burst);
            let mut n = Needle::default();
            let caught = n.run(kind, false, rate, &tone);
            assert!((db(caught) + 2.0).abs() < 0.1, "{kind:?}: {}", db(caught));
            let silence = vec![0.0; (fall_s * rate) as usize];
            let after = n.run(kind, false, rate, &silence);
            assert!((db(caught) - db(after) - fall_db).abs() < 0.1, "{kind:?}");
        }
    }

    /// The scales: 0 VU at +4 dBu (−14 dBFS), the UK's mark 4 at 0 dBu,
    /// the DIN's 0 at +6 dBu.
    #[test]
    fn scales_place_the_alignment_levels() {
        assert_eq!(MeterKind::TraditionalVu.value(-14.0), 0.0);
        assert_eq!(MeterKind::UkPpm.value(-18.0), 4.0);
        assert_eq!(MeterKind::DinPpm.value(-12.0), 0.0);
        assert_eq!(MeterKind::EbuPpm.position(-18.0), 0.5);
        // 0 VU about three quarters up, as a VU's arc has it.
        let zero = MeterKind::TraditionalVu.position(-14.0);
        assert!((0.70..0.75).contains(&zero), "{zero}");
        assert_eq!(MeterKind::LogarithmicVu.position(-200.0), 0.0);
        assert!((MeterKind::DinPpm.place(-20.0) - 1.0 / 3.0).abs() < 1e-6);
    }
}
