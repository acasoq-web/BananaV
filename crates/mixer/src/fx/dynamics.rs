//! Noise Gate, Track Noise Gate, Track Compressor and Multi-Band
//! Dynamics, as VEGAS's.
//!
//! The gates: the sound let through
//! while it is above a threshold. The gate opens once the level has stayed
//! above the threshold for the attack time and closes once it has stayed
//! under 0.85 of it (about 1.4 dB lower, so it does not chatter) for the
//! release time; every channel opens and closes together, by the loudest,
//! a millisecond at a time. Its gain glides toward open while opening (by
//! the attack) and open (in 15 ms), and toward shut while closing and
//! closed (by the release): nine tenths of the way in that time, sample by
//! sample.

use bv_timeline::audio_fx::{gate, multi_band_dynamics as mb, track_compressor as tc};

use super::eq::Section;
use super::{Own, db_gain};

/// The threshold at −80 dB, the gates' lowest (−Inf.).
const FLOOR: f32 = 3.0517e-5;
/// Under this the gain is 0.
const SHUT: f32 = 0.0002;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Closed,
    /// Above the threshold, waiting out the attack.
    Opening,
    Open,
    /// Under the closing threshold, waiting out the release.
    Closing,
}

#[derive(Debug)]
pub(crate) struct Gate {
    rate: f32,
    open_at: f32,
    attack: usize,
    release: usize,
    /// What is left of the way to its target after a sample, while
    /// opening, open, and closing.
    keep: [f32; 3],
    phase: Phase,
    count: usize,
    gain: f32,
    /// One stretch's gains (kept, not made again each time).
    gains: Vec<f32>,
}

impl Gate {
    pub(crate) fn new(rate: u32) -> Gate {
        Gate {
            rate: rate.max(1) as f32,
            open_at: FLOOR,
            attack: 0,
            release: 0,
            keep: [0.0; 3],
            phase: Phase::Closed,
            count: 0,
            gain: 0.0,
            gains: Vec::new(),
        }
    }

    /// What is left of the way after a sample when a tenth is left after
    /// `ms` (none at 0).
    fn keep(&self, ms: f32) -> f32 {
        if ms <= 0.0 {
            0.0
        } else {
            0.1f32.powf(1000.0 / (ms * self.rate))
        }
    }

    /// One stretch's level: the phase it leaves the gate in.
    fn follow(&mut self, level: f32, len: usize) {
        let close_at = 0.85 * self.open_at;
        self.phase = match self.phase {
            Phase::Closed if level >= self.open_at => {
                self.count = len;
                Phase::Opening
            }
            Phase::Opening if level <= close_at => Phase::Closed,
            Phase::Opening => {
                self.count += len;
                if self.count >= self.attack {
                    Phase::Open
                } else {
                    Phase::Opening
                }
            }
            Phase::Open if level <= close_at => {
                self.count = len;
                Phase::Closing
            }
            Phase::Closing if level >= self.open_at => Phase::Open,
            Phase::Closing => {
                self.count += len;
                if self.count >= self.release {
                    Phase::Closed
                } else {
                    Phase::Closing
                }
            }
            phase => phase,
        };
    }
}

impl Own for Gate {
    fn set(&mut self, params: &[f32]) {
        let p = |i: usize, d: f32| params.get(i).copied().unwrap_or(d);
        let db = p(gate::THRESHOLD, -80.0);
        self.open_at = if db <= -80.0 {
            FLOOR
        } else {
            10f32.powf(db / 20.0)
        };
        let attack = p(gate::ATTACK, 3.0).max(0.0);
        let release = p(gate::RELEASE, 100.0).max(1.0);
        self.attack = (attack / 1000.0 * self.rate) as usize;
        self.release = (release / 1000.0 * self.rate) as usize;
        self.keep = [self.keep(attack), self.keep(15.0), self.keep(release)];
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let ms = ((self.rate / 1000.0).round() as usize).max(1);
        let mut at = 0;
        while at < n {
            let end = (at + ms).min(n);
            let level = channels
                .iter()
                .map(|ch| ch[at..end].iter().fold(0f32, |m, s| m.max(s.abs())))
                .fold(0f32, f32::max);
            self.follow(level, end - at);
            let (keep, target) = match self.phase {
                Phase::Closed => (self.keep[2], 0.0),
                Phase::Opening => (self.keep[0], 1.0),
                Phase::Open => (self.keep[1], 1.0),
                // Held open until it closes.
                Phase::Closing => (self.keep[2], 1.0),
            };
            // The gain at each sample of the stretch, then on every channel.
            let gains = &mut self.gains;
            gains.clear();
            let mut g = self.gain;
            for _ in at..end {
                g = target + (g - target) * keep;
                if g < SHUT && target == 0.0 {
                    g = 0.0;
                }
                gains.push(g);
            }
            for ch in channels.iter_mut() {
                for (s, g) in ch[at..end].iter_mut().zip(gains.iter()) {
                    *s *= g;
                }
            }
            self.gain = g;
            at = end;
        }
    }
}

/// Track Compressor, as VEGAS's: the sound at the input gain, its level
/// followed by a detector that moves nine tenths of the way in the attack
/// time going up and in the release time going down (every channel by
/// the loudest), and above the threshold brought down to
/// `t·(level/t)^(1/ratio)` (from 500:1 a limiter). A threshold above
/// −3 dB is worked 3 dB lower inside, the gains moved to match. Auto gain
/// compensation gives back 70 % of what the curve takes at full scale;
/// smooth saturation rounds the curve's corner over 6 dB.
#[derive(Debug)]
pub(crate) struct TrackCompressor {
    rate: f32,
    input: f32,
    output: f32,
    threshold: f32,
    /// `1 / ratio − 1`: the gain's slope above the threshold, in log terms.
    slope: f32,
    limit: bool,
    soft: bool,
    attack: f32,
    release: f32,
    level: f32,
}

impl TrackCompressor {
    pub(crate) fn new(rate: u32) -> TrackCompressor {
        TrackCompressor {
            rate: rate.max(1) as f32,
            input: 1.0,
            output: 1.0,
            threshold: 1.0,
            slope: 0.0,
            limit: false,
            soft: false,
            attack: 0.0,
            release: 0.0,
            level: 0.0,
        }
    }

    /// What is left of the way after a sample when a tenth is left after
    /// `ms` (0: at once).
    fn keep(&self, ms: f32) -> f32 {
        let n = self.rate * ms / 1000.0;
        if n < 1.0 { 0.0 } else { 0.1f32.powf(1.0 / n) }
    }

    /// The gain at detected level `e`.
    fn gain(&self, e: f32) -> f32 {
        let t = self.threshold;
        if self.soft {
            // The corner rounded over 6 dB (3 either side of the threshold).
            let over = 20.0 * (e.max(1e-9) / t).log10();
            let cut = if over <= -3.0 {
                0.0
            } else if over < 3.0 {
                self.slope * (over + 3.0).powi(2) / 12.0
            } else {
                self.slope * over
            };
            return 10f32.powf(cut / 20.0);
        }
        if e <= t {
            1.0
        } else if self.limit {
            t / e
        } else {
            (e / t).powf(self.slope)
        }
    }
}

impl Own for TrackCompressor {
    fn set(&mut self, params: &[f32]) {
        let p = |i: usize| {
            let s = &tc::PARAMS[i];
            params
                .get(i)
                .copied()
                .unwrap_or(s.default)
                .clamp(s.min, s.max)
        };
        let mut input = db_gain(p(tc::INPUT), -60.0);
        let mut output = db_gain(p(tc::OUTPUT), -60.0);
        let db = p(tc::THRESHOLD);
        let mut t = 10f32.powf(db / 20.0).max(1e-3);
        if db > -3.0 {
            // Worked 3 dB under full scale, the gains moved to match.
            let d = 10f32.powf((-3.0 - db) / 20.0);
            input *= d;
            output /= d;
            t *= d;
        }
        let ratio = p(tc::RATIO);
        self.limit = ratio >= 500.0;
        self.slope = if self.limit { -1.0 } else { 1.0 / ratio - 1.0 };
        if p(tc::AUTO_GAIN) >= 0.5 {
            // 70 % of the cut at full scale given back.
            let full = t.powf(-self.slope);
            output /= full.powf(0.7);
        }
        self.input = input;
        self.output = output;
        self.threshold = t;
        self.soft = p(tc::SMOOTH) >= 0.5;
        self.attack = self.keep(p(tc::ATTACK));
        self.release = self.keep(p(tc::RELEASE));
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let (gi, go) = (self.input, self.output);
        let mut e = self.level;
        for i in 0..n {
            let peak = channels.iter().fold(0f32, |m, c| m.max((c[i] * gi).abs()));
            let keep = if peak > e { self.attack } else { self.release };
            e = peak + (e - peak) * keep;
            let g = gi * self.gain(e) * go;
            for c in channels.iter_mut() {
                c[i] *= g;
            }
        }
        self.level = e;
    }
}

/// One band of Multi-Band Dynamics: its filter, fixed gain, compression
/// and each channel's filter memory and level.
#[derive(Debug)]
struct Band {
    filter: Section,
    gain: f32,
    threshold: f32,
    slope: f32,
    attack: f32,
    release: f32,
    solo: bool,
    memory: Vec<[f32; 4]>,
    level: Vec<f32>,
}

/// Multi-Band Dynamics, as VEGAS's: each band picked out of the sound (a
/// low-pass for a low shelf, a band-pass for a band, a high-pass for a
/// high shelf), its level followed (nine tenths of the way in the attack
/// going up, the release going down; every channel by the loudest with
/// stereo sync), compressed above its threshold by its amount, raised or
/// lowered by its gain, and put back in place of what it was; the rest of
/// the sound goes through as it is. A soloed band plays alone (with the
/// other soloed ones); a bypassed one is left out.
#[derive(Debug)]
pub(crate) struct MultiBand {
    rate: f32,
    output: f32,
    sync: bool,
    bands: Vec<Band>,
    /// What the bands change, a channel (kept, not made again each time),
    /// and one sample's band on each channel.
    add: Vec<Vec<f32>>,
    picked: Vec<f32>,
}

impl MultiBand {
    pub(crate) fn new(rate: u32) -> MultiBand {
        MultiBand {
            rate: rate.max(1) as f32,
            output: 1.0,
            sync: true,
            bands: Vec::new(),
            add: Vec::new(),
            picked: Vec::new(),
        }
    }

    fn keep(&self, ms: f32) -> f32 {
        let n = self.rate * ms / 1000.0;
        if n < 1.0 { 0.0 } else { 0.1f32.powf(1.0 / n) }
    }
}

impl Own for MultiBand {
    fn set(&mut self, params: &[f32]) {
        let p = |i: usize| {
            let s = &mb::PARAMS[i];
            params
                .get(i)
                .copied()
                .unwrap_or(s.default)
                .clamp(s.min, s.max)
        };
        self.output = 10f32.powf(p(mb::OUTPUT) / 20.0);
        self.sync = p(mb::SYNC) >= 0.5;
        let fs = self.rate as f64;
        let mut bands = Vec::new();
        for b in 0..4 {
            let at = mb::BANDS + b * mb::PER_BAND;
            if p(at + mb::BYPASS) >= 0.5 {
                continue;
            }
            let hz = p(at + mb::CENTER) as f64;
            let filter = match p(at + mb::TYPE).round() as i32 {
                0 => Section::pass(hz, false, fs),
                2 => Section::pass(hz, true, fs),
                _ => Section::band(hz, p(at + mb::WIDTH) as f64, fs),
            };
            let ratio = p(at + mb::AMOUNT);
            bands.push(Band {
                filter,
                gain: 10f32.powf(p(at + mb::GAIN) / 20.0),
                threshold: 10f32.powf(p(at + mb::THRESHOLD) / 20.0).max(1.0 / 32768.0),
                slope: 1.0 / ratio - 1.0,
                attack: self.keep(p(at + mb::ATTACK)),
                release: self.keep(p(at + mb::RELEASE)),
                solo: p(at + mb::SOLO) >= 0.5,
                memory: Vec::new(),
                level: Vec::new(),
            });
        }
        // Keep each band's memory while its filter stays.
        for (new, old) in bands.iter_mut().zip(self.bands.iter_mut()) {
            if new.filter == old.filter {
                new.memory = std::mem::take(&mut old.memory);
                new.level = std::mem::take(&mut old.level);
            }
        }
        self.bands = bands;
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let count = channels.len();
        let soloing = self.bands.iter().any(|b| b.solo);
        let (output, sync) = (self.output, self.sync);
        // What each band changes, added up: on the sound, or alone.
        let (add, picked) = (&mut self.add, &mut self.picked);
        add.resize_with(count, Vec::new);
        for a in add.iter_mut() {
            a.clear();
            a.resize(n, 0.0);
        }
        picked.resize(count, 0.0);
        for band in &mut self.bands {
            band.memory.resize(count, [0.0; 4]);
            band.level.resize(count, 0.0);
            if soloing && !band.solo {
                continue;
            }
            for i in 0..n {
                for (c, ch) in channels.iter().enumerate() {
                    picked[c] = band.filter.run(ch[i], &mut band.memory[c]);
                }
                let loudest = picked.iter().fold(0f32, |m, v| m.max(v.abs()));
                for c in 0..count {
                    let peak = if sync { loudest } else { picked[c].abs() };
                    let e = &mut band.level[c];
                    let keep = if peak > *e { band.attack } else { band.release };
                    *e = peak + (*e - peak) * keep;
                    let squeeze = if *e > band.threshold {
                        (*e / band.threshold).powf(band.slope)
                    } else {
                        1.0
                    };
                    let g = band.gain * squeeze;
                    add[c][i] += if soloing { g } else { g - 1.0 } * picked[c];
                }
            }
        }
        for (ch, add) in channels.iter_mut().zip(add.iter()) {
            for (x, a) in ch.iter_mut().zip(add.iter()) {
                *x = output * if soloing { *a } else { *x + a };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(level: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| level * (i as f32 * 0.3).sin().signum())
            .collect()
    }

    fn gate(threshold: f32, attack: f32, release: f32) -> Gate {
        let mut g = Gate::new(48_000);
        g.set(&[threshold, attack, release]);
        g
    }

    #[test]
    fn quiet_sound_stays_shut() {
        let mut g = gate(-20.0, 3.0, 100.0);
        let mut ch = vec![tone(0.05, 4800)];
        g.process(&mut ch);
        assert!(ch[0].iter().all(|s| s.abs() < 1e-6));
    }

    #[test]
    fn loud_sound_opens_after_the_attack() {
        let mut g = gate(-20.0, 10.0, 100.0);
        let mut ch = vec![tone(0.5, 4800)];
        g.process(&mut ch);
        // Nine tenths of the way open after the attack (10 ms), all the way
        // by 60 ms.
        let at = |i: usize| ch[0][i].abs() / 0.5;
        assert!(at(48) < 0.5, "{}", at(48));
        assert!((at(480) - 0.9).abs() < 0.05, "{}", at(480));
        assert!(ch[0][2880..].iter().all(|s| (s.abs() - 0.5).abs() < 1e-3));
    }

    #[test]
    fn it_closes_after_the_release() {
        let mut g = gate(-20.0, 1.0, 10.0);
        let mut ch = vec![tone(0.5, 4800)];
        g.process(&mut ch);
        // Then quiet: held open for the release (10 ms), then gliding shut —
        // a tenth left after another 10 ms — and silent once under −74 dB.
        let mut ch = vec![tone(0.01, 9600)];
        g.process(&mut ch);
        assert!((ch[0][100].abs() - 0.01).abs() < 1e-4, "{}", ch[0][100]);
        // Shut from the stretch where the hold ran out (432), a tenth left
        // 480 samples later.
        let tenth = ch[0][432 + 480].abs() / 0.01;
        assert!((tenth - 0.1).abs() < 0.03, "{tenth}");
        assert!(ch[0][4800..].iter().all(|s| s.abs() < 1e-9));
    }

    fn compressor(values: &[(usize, f32)]) -> TrackCompressor {
        let mut c = TrackCompressor::new(48_000);
        let mut p: Vec<f32> = tc::PARAMS.iter().map(|s| s.default).collect();
        for &(i, v) in values {
            p[i] = v;
        }
        c.set(&p);
        c
    }

    fn settled(c: &mut TrackCompressor, level: f32) -> f32 {
        let mut ch = vec![tone(level, 48_000)];
        c.process(&mut ch);
        ch[0][47_000..].iter().fold(0f32, |m, s| m.max(s.abs()))
    }

    #[test]
    fn the_compressor_follows_its_curve_above_the_threshold() {
        // 2:1 from −18 dB, no makeup: −6 dB in comes out at −12 dB.
        let mut c = compressor(&[
            (tc::THRESHOLD, -18.0),
            (tc::RATIO, 2.0),
            (tc::AUTO_GAIN, 0.0),
        ]);
        let out = 20.0 * settled(&mut c, 0.501).log10();
        assert!((out + 12.0).abs() < 0.3, "{out}");
        // Below the threshold it is left alone.
        let mut c = compressor(&[
            (tc::THRESHOLD, -18.0),
            (tc::RATIO, 2.0),
            (tc::AUTO_GAIN, 0.0),
        ]);
        assert!((settled(&mut c, 0.05) - 0.05).abs() < 1e-4);
    }

    #[test]
    fn auto_gain_gives_back_most_of_the_cut_and_500_limits() {
        // 2:1 from −18 dB: 9 dB off at full scale, 70 % of it back.
        let mut c = compressor(&[(tc::THRESHOLD, -18.0), (tc::RATIO, 2.0)]);
        let out = 20.0 * settled(&mut c, 0.501).log10();
        assert!((out - (-12.0 + 0.7 * 9.0)).abs() < 0.3, "{out}");
        // A limiter holds the threshold.
        let mut c = compressor(&[
            (tc::THRESHOLD, -12.0),
            (tc::RATIO, 500.0),
            (tc::AUTO_GAIN, 0.0),
        ]);
        let out = 20.0 * settled(&mut c, 0.9).log10();
        assert!((out + 12.0).abs() < 0.3, "{out}");
    }

    #[test]
    fn neutral_settings_change_nothing() {
        let mut c = compressor(&[]);
        let x = tone(0.7, 4800);
        let mut ch = vec![x.clone()];
        c.process(&mut ch);
        assert!(ch[0].iter().zip(&x).all(|(a, b)| (a - b).abs() < 1e-6));
    }

    fn multi_band(values: &[(usize, f32)]) -> MultiBand {
        let mut m = MultiBand::new(48_000);
        let mut p: Vec<f32> = mb::PARAMS.iter().map(|s| s.default).collect();
        for &(i, v) in values {
            p[i] = v;
        }
        m.set(&p);
        m
    }

    fn sine(hz: f32, level: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| level * (std::f32::consts::TAU * hz * i as f32 / 48_000.0).sin())
            .collect()
    }

    fn db_of(m: &mut MultiBand, hz: f32, level: f32) -> f32 {
        let mut ch = vec![sine(hz, level, 48_000)];
        m.process(&mut ch);
        let tail = &ch[0][24_000..];
        let power = tail.iter().map(|s| s * s).sum::<f32>() / tail.len() as f32;
        10.0 * (2.0 * power).log10()
    }

    #[test]
    fn multi_band_defaults_change_nothing() {
        let mut m = multi_band(&[]);
        for hz in [60.0, 200.0, 3000.0] {
            assert!((db_of(&mut m, hz, 0.5) - 20.0 * 0.5f32.log10()).abs() < 0.05);
        }
    }

    #[test]
    fn a_band_s_gain_and_compression_touch_only_it() {
        // Band 1 a band at 1 kHz, −12 dB.
        let g = mb::BANDS + mb::GAIN;
        let mut m = multi_band(&[(mb::BANDS + mb::CENTER, 1000.0), (g, -12.0)]);
        assert!(
            (db_of(&mut m, 1000.0, 1.0) + 12.0).abs() < 0.5,
            "{}",
            db_of(&mut m, 1000.0, 1.0)
        );
        assert!(db_of(&mut m, 100.0, 1.0).abs() < 0.5);
        // Compressed 4:1 above −30 dB: a loud tone there comes down, a
        // quiet one stays.
        let mut m = multi_band(&[
            (mb::BANDS + mb::CENTER, 5000.0),
            (mb::BANDS + mb::THRESHOLD, -30.0),
            (mb::BANDS + mb::AMOUNT, 4.0),
        ]);
        let loud = db_of(&mut m, 5000.0, 0.5);
        assert!(loud < -12.0, "{loud}");
        let mut m = multi_band(&[
            (mb::BANDS + mb::CENTER, 5000.0),
            (mb::BANDS + mb::THRESHOLD, -30.0),
            (mb::BANDS + mb::AMOUNT, 4.0),
        ]);
        let quiet = db_of(&mut m, 5000.0, 0.01);
        assert!((quiet + 40.0).abs() < 0.5, "{quiet}");
    }

    #[test]
    fn channels_open_together_by_the_loudest() {
        let mut g = gate(-20.0, 1.0, 100.0);
        let mut ch = vec![tone(0.5, 4800), tone(0.01, 4800)];
        g.process(&mut ch);
        assert!((ch[1][4000].abs() - 0.01).abs() < 1e-4);
    }
}
