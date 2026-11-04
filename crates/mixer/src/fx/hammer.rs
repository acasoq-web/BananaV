//! Wave Hammer Surround, as VEGAS's: a compressor and a volume maximizer
//! in a row, each steered by the channels chosen for its control signal
//! and working on the channels chosen to be affected.
//!
//! The compressor reads the level by its peak or its power (RMS), moving
//! by the attack and release (smart release lets it come back sooner
//! after a short burst), a little ahead of the sound (longer with the
//! longer look-ahead); above the threshold it brings the level down by
//! the ratio (50:1 a limiter), its corner rounded over 6 dB. Auto gain
//! compensation gives back 70 % of what it takes at full scale; smooth
//! saturation rounds off what still goes past full scale. The maximizer
//! holds the peaks (seen ahead) at its threshold and brings the result up
//! to its output level.

use bv_timeline::audio_fx::wave_hammer as wh;

use super::Own;

/// BananaV's 5.1 order (L R C LFE Ls Rs) to the switches' (L R C Ls Rs
/// LFE).
const SWITCH: [usize; 6] = [0, 1, 2, 5, 3, 4];

#[derive(Debug)]
pub(crate) struct WaveHammer {
    rate: f32,
    params: Vec<f32>,
    /// The sound held back for the look-ahead, a line a channel.
    lines: Vec<Vec<f32>>,
    at: usize,
    level: f32,
    /// How long the level has been rising (samples), for smart release.
    rising: usize,
    max_gain: f32,
}

impl WaveHammer {
    pub(crate) fn new(rate: u32) -> WaveHammer {
        WaveHammer {
            rate: rate.max(1) as f32,
            params: Vec::new(),
            lines: Vec::new(),
            at: 0,
            level: 0.0,
            rising: 0,
            max_gain: 1.0,
        }
    }

    fn p(&self, i: usize) -> f32 {
        let s = &wh::PARAMS[i];
        self.params
            .get(i)
            .copied()
            .unwrap_or(s.default)
            .clamp(s.min, s.max)
    }

    fn keep(&self, ms: f32) -> f32 {
        let n = self.rate * ms / 1000.0;
        if n < 1.0 { 0.0 } else { 0.1f32.powf(1.0 / n) }
    }

    /// How far ahead it reads, in samples: the compressor's and the
    /// maximizer's look-ahead, the longer of the two.
    fn ahead(&self) -> usize {
        let ms = |longer: bool| if longer { 5.0f32 } else { 1.5 };
        let comp = if self.p(wh::BYPASS) < 0.5 {
            ms(self.p(wh::LOOK_AHEAD) >= 0.5)
        } else {
            0.0
        };
        let max = if self.p(wh::MAX_BYPASS) < 0.5 {
            ms(self.p(wh::MAX_LOOK_AHEAD) >= 0.5)
        } else {
            0.0
        };
        (comp.max(max) * self.rate / 1000.0).round() as usize
    }
}

/// The compressor's gain at level `e` over threshold `t` (both linear),
/// `slope` = 1/ratio − 1, the corner rounded over ±3 dB.
fn squeeze(e: f32, t: f32, slope: f32) -> f32 {
    let over = 20.0 * (e.max(1e-9) / t).log10();
    let cut = if over <= -3.0 {
        0.0
    } else if over < 3.0 {
        slope * (over + 3.0).powi(2) / 12.0
    } else {
        slope * over
    };
    10f32.powf(cut / 20.0)
}

impl Own for WaveHammer {
    fn set(&mut self, params: &[f32]) {
        self.params = params.to_vec();
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let count = channels.len();
        let ahead = self.ahead();
        let room = ahead + 1;
        if self.lines.len() != count || self.lines.first().map_or(0, Vec::len) != room {
            self.lines = vec![vec![0.0; room]; count];
            self.at = 0;
        }
        let switch = |c: usize, at: usize| {
            let k = if count == 6 { SWITCH[c] } else { c.min(5) };
            self.p(at + k) >= 0.5
        };
        let control: Vec<bool> = (0..count).map(|c| switch(c, wh::CONTROL)).collect();
        let affect: Vec<bool> = (0..count).map(|c| switch(c, wh::AFFECT)).collect();
        let compress = self.p(wh::BYPASS) < 0.5;
        let maximize = self.p(wh::MAX_BYPASS) < 0.5;
        let rms = self.p(wh::RMS) >= 0.5;
        let t = 10f32.powf(self.p(wh::THRESHOLD) / 20.0);
        let ratio = self.p(wh::RATIO);
        let slope = if ratio >= 50.0 {
            -1.0
        } else {
            1.0 / ratio - 1.0
        };
        let mut out = 10f32.powf(self.p(wh::OUTPUT) / 20.0);
        if self.p(wh::AUTO_GAIN) >= 0.5 {
            out /= t.powf(-slope).powf(0.7);
        }
        let smooth = self.p(wh::SMOOTH) >= 0.5;
        let attack = self.keep(self.p(wh::ATTACK));
        let release = self.keep(self.p(wh::RELEASE));
        let smart = self.keep(self.p(wh::RELEASE) * (1.0 - 0.75 * self.p(wh::SMART) / 100.0));
        let ceiling = 10f32.powf(self.p(wh::MAX_THRESHOLD) / 20.0);
        let lift = 10f32.powf(self.p(wh::MAX_OUTPUT) / 20.0) / ceiling;
        let max_release = self.keep(self.p(wh::MAX_RELEASE));
        let mut e = self.level;
        let mut rising = self.rising;
        let mut mg = self.max_gain;
        for i in 0..n {
            // What comes in now is what the detectors see ahead.
            let mut peak = 0f32;
            let mut power = 0f32;
            let mut seen = 0usize;
            for (c, ch) in channels.iter().enumerate() {
                if control[c] {
                    peak = peak.max(ch[i].abs());
                    power += ch[i] * ch[i];
                    seen += 1;
                }
            }
            let now = if rms {
                (power / seen.max(1) as f32).sqrt()
            } else {
                peak
            };
            let keep = if now > e {
                rising += 1;
                attack
            } else if rising > 0 && rising < (0.05 * self.rate) as usize {
                // A short burst: let go sooner.
                rising = 0;
                smart
            } else {
                rising = 0;
                release
            };
            e = now + (e - now) * keep;
            let comp = if compress {
                squeeze(e, t, slope) * out
            } else {
                1.0
            };
            let max_target = if maximize && peak * comp > ceiling {
                ceiling / (peak * comp)
            } else {
                1.0
            };
            mg = if max_target < mg {
                max_target
            } else {
                max_target + (mg - max_target) * max_release
            };
            let gain = comp * if maximize { mg * lift } else { 1.0 };
            let at = self.at;
            for (c, ch) in channels.iter_mut().enumerate() {
                let line = &mut self.lines[c];
                line[at] = ch[i];
                // Written `ahead` samples ago (now, with no look-ahead).
                let late = line[(at + 1) % room];
                let mut y = if affect[c] { late * gain } else { late };
                if smooth && compress && y.abs() > 0.9 {
                    // Past −0.9 dB, rounded off toward full scale.
                    let s = y.signum();
                    y = s * (0.9 + 0.1 * ((y.abs() - 0.9) / 0.1).tanh());
                }
                ch[i] = y;
            }
            self.at = (at + 1) % room;
        }
        self.level = e;
        self.rising = rising;
        self.max_gain = mg;
    }

    fn latency(&self) -> usize {
        self.ahead()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hammer(values: &[(usize, f32)]) -> WaveHammer {
        let mut h = WaveHammer::new(48_000);
        let mut p: Vec<f32> = wh::PARAMS.iter().map(|s| s.default).collect();
        for &(i, v) in values {
            p[i] = v;
        }
        h.set(&p);
        h
    }

    fn tone(level: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| level * (std::f32::consts::TAU * 440.0 * i as f32 / 48_000.0).sin())
            .collect()
    }

    fn settled(h: &mut WaveHammer, level: f32) -> f32 {
        let mut ch = vec![tone(level, 48_000), tone(level, 48_000)];
        h.process(&mut ch);
        ch[0][40_000..].iter().fold(0f32, |m, s| m.max(s.abs()))
    }

    #[test]
    fn the_compressor_brings_loud_sound_down() {
        // 4:1 from −12 dB, peak scan, no saturation: −2 dB in, −9.5 out.
        let mut h = hammer(&[(wh::RMS, 0.0), (wh::SMOOTH, 0.0), (wh::MAX_BYPASS, 1.0)]);
        let out = 20.0 * settled(&mut h, 10f32.powf(-2.0 / 20.0)).log10();
        assert!((out + 9.5).abs() < 0.5, "{out}");
    }

    #[test]
    fn the_maximizer_holds_peaks_and_lifts_to_its_level() {
        // Only the maximizer: peaks held at −6 dB, then lifted to −1 dB.
        let mut h = hammer(&[
            (wh::BYPASS, 1.0),
            (wh::MAX_THRESHOLD, -6.0),
            (wh::MAX_OUTPUT, -1.0),
        ]);
        let out = 20.0 * settled(&mut h, 1.0).log10();
        assert!((out + 1.0).abs() < 0.3, "{out}");
        assert_eq!(h.latency(), 72);
    }

    #[test]
    fn channels_left_out_are_untouched() {
        let mut h = hammer(&[
            (wh::RMS, 0.0),
            (wh::SMOOTH, 0.0),
            (wh::MAX_BYPASS, 1.0),
            (wh::AFFECT + 1, 0.0),
        ]);
        let mut ch = vec![tone(0.9, 4_800), tone(0.9, 4_800)];
        h.process(&mut ch);
        let lat = h.latency();
        let right = tone(0.9, 4_800);
        assert!(
            ch[1][lat..]
                .iter()
                .zip(&right)
                .all(|(a, b)| (a - b).abs() < 1e-6)
        );
    }
}
