//! Playback faster or slower with its pitch kept, as it plays (WSOLA): the
//! sound is heard in windows of two hundredths of a second, each half
//! over the one before (their weights add up to one), each taken from where
//! the mix should be by then — a little before or after, where it goes on
//! most like the window before it would have gone on. Between the mixed
//! buffer and the device, so that a new speed is heard within a hundredth
//! of a second; at normal speed what was mixed goes through as it is.

use std::collections::VecDeque;

/// A window's length, in seconds: half of it is heard before the next one.
const WINDOW: f64 = 0.015;

/// How far before or after where the mix should be a window may be taken
/// from, in seconds: more than a low voice's period.
const SEARCH: f64 = 0.025;

/// How alike, at least, to the best a window's natural continuation must
/// be to be kept.
const NATURAL: f32 = 0.98;

/// How the windows are going.
#[derive(Debug)]
enum Mode {
    /// As it was mixed, from frame `next` on.
    Through { next: usize },
    /// In windows: the last one would go on from frame `natural`; where
    /// the next one should begin is `pos` (what was heard stands for the
    /// mix until there); `pending` is the last one's second half, weighed.
    Windows {
        natural: usize,
        pos: f64,
        pending: Vec<f32>,
    },
}

/// What is being played at another speed.
#[derive(Debug)]
pub(crate) struct Stretch {
    channels: usize,
    /// A window and half of it, in frames, and how far a window may stray.
    window: usize,
    half: usize,
    search: usize,
    /// The window's weights (a Hann window: those of its two halves add up
    /// to one).
    weights: Vec<f32>,
    /// What was mixed and not let go of yet (samples of all channels side
    /// by side): frames from `first` on, counted since playback began.
    input: Vec<f32>,
    first: usize,
    mode: Mode,
    /// Frames ready to be heard, and how many frames of what was mixed each
    /// stretch of them stands for.
    ready: VecDeque<f32>,
    runs: VecDeque<(usize, f64)>,
}

/// What [`Stretch::pull`] gave.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Pulled {
    /// Frames heard.
    pub frames: usize,
    /// How many frames of what was mixed they stand for.
    pub passed: f64,
}

impl Stretch {
    pub fn new(rate: u32, channels: usize) -> Stretch {
        let rate = f64::from(rate);
        let half = ((WINDOW * rate / 2.0).round() as usize).max(8);
        let window = 2 * half;
        let weights = (0..window)
            .map(|i| {
                let x = std::f64::consts::PI * (i as f64 + 0.5) / window as f64;
                x.sin().powi(2) as f32
            })
            .collect();
        Stretch {
            channels: channels.max(1),
            window,
            half,
            search: ((SEARCH * rate).round() as usize).max(4),
            weights,
            input: Vec::new(),
            first: 0,
            mode: Mode::Through { next: 0 },
            ready: VecDeque::new(),
            runs: VecDeque::new(),
        }
    }

    /// How much more than `speed` × what is heard must be mixed ahead, in
    /// seconds.
    pub fn look_ahead(speed: f64) -> f64 {
        if speed == 1.0 {
            return 0.0;
        }
        WINDOW + 2.0 * SEARCH + WINDOW / 2.0 * speed + 0.02
    }

    /// The frames of the input there are, counted from the start.
    fn end(&self) -> usize {
        self.first + self.input.len() / self.channels
    }

    /// Takes from `ring` until the input reaches frame `until`; whether it
    /// does.
    fn gather(&mut self, ring: &mut VecDeque<f32>, until: usize) -> bool {
        let end = self.end();
        if end < until {
            let more = (until - end).min(ring.len() / self.channels) * self.channels;
            self.input.extend(ring.drain(..more));
        }
        self.end() >= until
    }

    /// Lets go of the input before frame `keep`, now and then.
    fn let_go(&mut self, keep: usize) {
        if keep > self.first + (1 << 14) {
            let n = (keep - self.first).min(self.input.len() / self.channels);
            self.input.drain(..n * self.channels);
            self.first += n;
        }
    }

    /// `n` frames of the input from frame `at`.
    fn frames(&self, at: usize, n: usize) -> &[f32] {
        let from = (at - self.first) * self.channels;
        &self.input[from..from + n * self.channels]
    }

    /// Fills `out` (whole frames) from what was mixed, `ring`, at `speed`.
    pub fn pull(&mut self, ring: &mut VecDeque<f32>, out: &mut [f32], speed: f64) -> Pulled {
        let ch = self.channels;
        let wanted = out.len() / ch;
        let mut got = Pulled::default();
        while got.frames < wanted {
            // What is ready.
            if let Some((left, each)) = self.runs.front_mut() {
                let n = (*left).min(wanted - got.frames);
                let to = &mut out[got.frames * ch..(got.frames + n) * ch];
                for (o, s) in to.iter_mut().zip(self.ready.drain(..n * ch)) {
                    *o = s;
                }
                got.frames += n;
                got.passed += n as f64 * *each;
                *left -= n;
                if *left == 0 {
                    self.runs.pop_front();
                }
                continue;
            }
            if let Mode::Through { next } = self.mode {
                if speed == 1.0 {
                    // As it was mixed: what is kept here, then the rest.
                    let n = (wanted - got.frames).min(self.end() - next);
                    let to = &mut out[got.frames * ch..(got.frames + n) * ch];
                    if n > 0 {
                        to.copy_from_slice(self.frames(next, n));
                        self.mode = Mode::Through { next: next + n };
                        if next + n == self.end() {
                            self.input.clear();
                            self.first = next + n;
                        }
                    } else {
                        let n = (wanted - got.frames).min(ring.len() / ch);
                        let to = &mut out[got.frames * ch..(got.frames + n) * ch];
                        for (o, s) in to.iter_mut().zip(ring.drain(..n * ch)) {
                            *o = s;
                        }
                        self.mode = Mode::Through { next: next + n };
                        self.input.clear();
                        self.first = next + n;
                        got.frames += n;
                        got.passed += n as f64;
                        break;
                    }
                    got.frames += n;
                    got.passed += n as f64;
                    continue;
                }
                // Into windows: as if the one before began half a window
                // ago, the way the sound went on.
                if !self.gather(ring, next + self.window) {
                    break;
                }
                let half = self.half;
                let pending = self
                    .frames(next, half)
                    .chunks_exact(ch)
                    .zip(&self.weights[half..])
                    .flat_map(|(f, w)| f.iter().map(move |s| s * w))
                    .collect();
                self.mode = Mode::Windows {
                    natural: next,
                    pos: next as f64,
                    pending,
                };
            }
            if !self.step(ring, speed) {
                break;
            }
        }
        got
    }

    /// Makes the next half window ready; false when not enough is mixed.
    fn step(&mut self, ring: &mut VecDeque<f32>, speed: f64) -> bool {
        let Mode::Windows { natural, pos, .. } = self.mode else {
            return false;
        };
        let (ch, half, window, search) = (self.channels, self.half, self.window, self.search);
        let (from, to) = if speed == 1.0 {
            // Back to the mix as it is: the way it went on, and what is
            // heard catching up with what it stands for.
            let reach = (natural + half) as f64;
            let to = reach.clamp(pos + half as f64 * 0.5, pos + half as f64 * 1.5);
            (natural, to)
        } else {
            let target = pos.round() as usize;
            let lo = target.saturating_sub(search).max(self.first);
            if !self.gather(ring, (target + search + window).max(natural + window)) {
                return false;
            }
            (
                self.best(natural, lo, target + search, target),
                pos + half as f64 * speed,
            )
        };
        if !self.gather(ring, (from + window).max(natural + window)) {
            return false;
        }
        let Mode::Windows { pending, .. } = &mut self.mode else {
            return false;
        };
        let mut pending = std::mem::take(pending);
        // The first half heard over the last one's second, the second kept.
        let start = (from - self.first) * ch;
        let window_of = &self.input[start..start + window * ch];
        for (i, frame) in window_of[..half * ch].chunks_exact(ch).enumerate() {
            let w = self.weights[i];
            for (c, s) in frame.iter().enumerate() {
                self.ready.push_back(pending[i * ch + c] + s * w);
            }
        }
        for (i, frame) in window_of[half * ch..].chunks_exact(ch).enumerate() {
            let w = self.weights[half + i];
            for (c, s) in frame.iter().enumerate() {
                pending[i * ch + c] = s * w;
            }
        }
        self.runs.push_back((half, (to - pos) / half as f64));
        let caught_up = speed == 1.0 && to == (from + half) as f64;
        self.mode = if caught_up {
            // What follows is the mix as it is.
            Mode::Through { next: from + half }
        } else {
            Mode::Windows {
                natural: from + half,
                pos: to,
                pending,
            }
        };
        let keep = match self.mode {
            Mode::Through { next } => next,
            Mode::Windows { natural, pos, .. } => {
                natural.min((pos.round() as usize).saturating_sub(search))
            }
        };
        self.let_go(keep);
        true
    }

    /// Where, from frame `lo` to `hi`, a window sounds most like the one
    /// from frame `like` (how the last went on): that one itself when it is
    /// as good as any (a voice's next period is about as alike, but its
    /// pitch has moved on); `target` when none is more alike (silence).
    fn best(&self, like: usize, lo: usize, hi: usize, target: usize) -> usize {
        let ch = self.channels;
        let window = self.window;
        let hi = hi.min(self.end().saturating_sub(window));
        let lo = lo.min(hi);
        let template = self.frames(like, window);
        // How alike, from every `by`-th frame: the product over the
        // candidate's loudness.
        let score = |k: usize, by: usize| -> f32 {
            let x = self.frames(k, window);
            let (mut like, mut power) = (0.0f32, 1e-12f32);
            for (a, b) in template
                .chunks_exact(ch)
                .step_by(by)
                .zip(x.chunks_exact(ch).step_by(by))
            {
                for (p, q) in a.iter().zip(b) {
                    like += p * q;
                    power += q * q;
                }
            }
            like / power.sqrt()
        };
        let target = target.clamp(lo, hi);
        // Every fourth candidate from every other frame first.
        let mut coarse = (target, score(target, 2));
        for k in (lo..=hi).step_by(4) {
            let s = score(k, 2);
            if s > coarse.1 + 1e-6 {
                coarse = (k, s);
            }
        }
        // Then frame by frame about it, and about the target.
        let mut fine = (target, score(target, 1));
        for about in [coarse.0, target] {
            for k in about.saturating_sub(3).max(lo)..=(about + 3).min(hi) {
                let s = score(k, 1);
                if s > fine.1 + 1e-6 {
                    fine = (k, s);
                }
            }
        }
        if (lo..=hi).contains(&like) && score(like, 1) >= fine.1 * NATURAL {
            return like;
        }
        fine.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn tone(hz: f64, frames: usize) -> VecDeque<f32> {
        (0..frames)
            .flat_map(|i| {
                let t = i as f64 / f64::from(RATE);
                let s = (std::f64::consts::TAU * hz * t).sin() as f32 * 0.5;
                [s, s * 0.5]
            })
            .collect()
    }

    /// The whole of `ring` at `speed`, a device's buffer at a time.
    fn play(s: &mut Stretch, ring: &mut VecDeque<f32>, speed: f64) -> (Vec<f32>, f64) {
        let (mut all, mut passed) = (Vec::new(), 0.0);
        let mut out = vec![0.0f32; 480 * 2];
        loop {
            let got = s.pull(ring, &mut out, speed);
            all.extend_from_slice(&out[..got.frames * 2]);
            passed += got.passed;
            if got.frames < 480 {
                return (all, passed);
            }
        }
    }

    /// How often the left channel goes up through 0 a second.
    fn pitch(all: &[f32]) -> f64 {
        let left: Vec<f32> = all.iter().step_by(2).copied().collect();
        let ups = left
            .windows(2)
            .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
            .count();
        ups as f64 * f64::from(RATE) / left.len() as f64
    }

    fn jump(all: &[f32]) -> f32 {
        all.iter()
            .step_by(2)
            .copied()
            .collect::<Vec<_>>()
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0, f32::max)
    }

    #[test]
    fn normal_speed_is_what_was_mixed() {
        let mut ring = tone(440.0, 48_000);
        let all: Vec<f32> = ring.iter().copied().collect();
        let mut s = Stretch::new(RATE, 2);
        let (out, passed) = play(&mut s, &mut ring, 1.0);
        assert_eq!(out, all);
        assert_eq!(passed, 48_000.0);
    }

    /// Any speed keeps the pitch, takes as long as it should and says so,
    /// and joins its pieces without a click.
    #[test]
    fn a_speed_keeps_the_pitch() {
        for speed in [0.25, 0.5, 1.5, 2.0, 3.0, 4.0] {
            for hz in [110.0, 440.0, 1_000.0] {
                let mut ring = tone(hz, 4 * 48_000);
                let mut s = Stretch::new(RATE, 2);
                let (out, passed) = play(&mut s, &mut ring, speed);
                let frames = out.len() as f64 / 2.0;
                let p = pitch(&out);
                assert!((p / hz - 1.0).abs() < 0.01, "{speed}: {hz} as {p}");
                assert!(
                    (passed / frames / speed - 1.0).abs() < 0.01,
                    "{speed}: {frames} of {passed}"
                );
                // Less than what is kept for the next piece was not played.
                assert!(passed > 4.0 * 48_000.0 - 48_000.0 * 0.5, "{passed}");
                let step = std::f64::consts::TAU * hz / f64::from(RATE) * 0.5;
                let j = f64::from(jump(&out));
                assert!(j < step * 1.6, "{speed}, {hz}: a jump of {j}");
            }
        }
    }

    /// Faster, slower and back as it plays: no click, no sound lost count
    /// of, and normal speed is what was mixed again.
    #[test]
    fn speeds_change_as_it_plays() {
        let whole = tone(330.0, 10 * 48_000);
        let mixed: Vec<f32> = whole.iter().copied().collect();
        let mut ring = whole;
        let mut s = Stretch::new(RATE, 2);
        let mut out = vec![0.0f32; 480 * 2];
        let (mut all, mut passed) = (Vec::new(), 0.0);
        for speed in [1.0, 2.0, 0.5, 4.0, 1.5, 1.0, 1.0] {
            for _ in 0..50 {
                let got = s.pull(&mut ring, &mut out, speed);
                assert_eq!(got.frames, 480);
                all.extend_from_slice(&out);
                passed += got.passed;
            }
        }
        let step = std::f64::consts::TAU * 330.0 / f64::from(RATE) * 0.5;
        let j = f64::from(jump(&all));
        assert!(j < step * 1.6, "a jump of {j}");
        // The last half second is the mix where its count says it is.
        let end = passed.round() as usize;
        assert!((passed - end as f64).abs() < 1e-6, "{passed}");
        assert_eq!(all[all.len() - 48_000..], mixed[end * 2 - 48_000..end * 2]);
    }

    /// A voice made up: a pitch gliding with a vibrato, its harmonics shaped
    /// by three formants, in syllables four times a second; stereo.
    fn voice(seconds: f64, low: f64) -> Vec<f32> {
        let n = (seconds * f64::from(RATE)) as usize;
        let mut phase = 0.0f64;
        let mut all = Vec::with_capacity(n * 2);
        for i in 0..n {
            let t = i as f64 / f64::from(RATE);
            let f0 = (110.0
                + 70.0 * (t * 0.7).sin().abs()
                + 4.0 * (t * 6.0 * std::f64::consts::TAU).sin())
                * low;
            phase += f0 / f64::from(RATE);
            let mut s = 0.0;
            let mut h = 1.0;
            while h * f0 < 5_000.0 {
                let f = h * f0;
                let formant = |c: f64, w: f64| (-((f - c) / w).powi(2)).exp();
                let a = formant(700.0, 150.0)
                    + 0.6 * formant(1_200.0, 200.0)
                    + 0.3 * formant(2_500.0, 300.0)
                    + 0.02;
                s += a * (std::f64::consts::TAU * h * phase).sin();
                h += 1.0;
            }
            let syllable = (t * 4.0 * std::f64::consts::PI).sin().abs().powf(0.5);
            let s = (s * syllable * 0.15) as f32;
            all.extend([s, s * 0.8]);
        }
        all
    }

    /// The short spectrum (dB, up to 5 kHz) of the left channel around
    /// frame `at`.
    fn spectrum(all: &[f32], at: usize) -> Vec<f64> {
        const N: usize = 2048;
        static TURNS: std::sync::OnceLock<Vec<(f64, f64, f64)>> = std::sync::OnceLock::new();
        let turns = TURNS.get_or_init(|| {
            (0..N)
                .map(|i| {
                    let a = std::f64::consts::TAU * i as f64 / N as f64;
                    (a.cos(), a.sin(), 0.5 - 0.5 * a.cos())
                })
                .collect()
        });
        let left: Vec<f64> = (0..N)
            .map(|i| {
                let f = (at + i).saturating_sub(N / 2);
                all.get(f * 2).copied().unwrap_or(0.0) as f64 * turns[i].2
            })
            .collect();
        let bins = 5_000 * N / RATE as usize;
        (1..bins)
            .map(|b| {
                let (mut re, mut im) = (0.0, 0.0);
                for (i, x) in left.iter().enumerate() {
                    let (c, s, _) = turns[(b * i) % N];
                    re += x * c;
                    im -= x * s;
                }
                10.0 * (re * re + im * im + 1e-9).log10()
            })
            .collect()
    }

    /// How far, in dB on average, `out` sounds from `input` played at
    /// `speed`: each tenth of a second of it against the part of the input
    /// it stands for, or the nearest one alike within 30 ms (where a piece
    /// was taken from may stray that far).
    fn distance(input: &[f32], out: &[f32], speed: f64) -> f64 {
        let frames = out.len() / 2;
        let (mut sum, mut n) = (0.0, 0);
        let mut at = RATE as usize / 4;
        let stray = RATE as usize * 30 / 1000;
        while at + RATE as usize / 4 < frames {
            let was = (at as f64 * speed) as usize;
            if was + RATE as usize / 4 > input.len() / 2 {
                break;
            }
            let a = spectrum(out, at);
            let mut nearest = f64::MAX;
            for d in (0..=2 * stray).step_by(RATE as usize / 400) {
                let b = spectrum(input, (was + d).saturating_sub(stray));
                let top = b.iter().copied().fold(f64::MIN, f64::max);
                // The bins that matter: within 50 dB of the loudest.
                let d: Vec<f64> = a
                    .iter()
                    .zip(&b)
                    .filter(|(_, y)| **y > top - 50.0)
                    .map(|(x, y)| (x - y).abs())
                    .collect();
                nearest = nearest.min(d.iter().sum::<f64>() / d.len().max(1) as f64);
            }
            sum += nearest;
            n += 1;
            at += RATE as usize / 10;
        }
        sum / f64::from(n.max(1))
    }

    fn write_wav(path: &std::path::Path, all: &[f32]) {
        let mut b = Vec::new();
        let data = (all.len() * 2) as u32;
        b.extend(b"RIFF");
        b.extend((36 + data).to_le_bytes());
        b.extend(b"WAVEfmt ");
        b.extend(16u32.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(2u16.to_le_bytes());
        b.extend(RATE.to_le_bytes());
        b.extend((RATE * 4).to_le_bytes());
        b.extend(4u16.to_le_bytes());
        b.extend(16u16.to_le_bytes());
        b.extend(b"data");
        b.extend(data.to_le_bytes());
        for s in all {
            b.extend(((s.clamp(-1.0, 1.0) * 32_767.0) as i16).to_le_bytes());
        }
        std::fs::write(path, b).expect("wav");
    }

    fn read_wav(path: &std::path::Path) -> Vec<f32> {
        let b = std::fs::read(path).expect("wav");
        let at = b.windows(4).position(|w| w == b"data").expect("data") + 8;
        b[at..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| f32::from(i16::from_le_bytes(*c)) / 32_767.0)
            .collect()
    }

    /// How a voice sounds at each speed: its distance from what it should
    /// be, beside FFmpeg's atempo's when FFmpeg is on the path. Writes the
    /// voices to target/tempo to be heard.
    /// `cargo test -p bv-engine --lib tempo::tests::how_a_voice_sounds -- --ignored --nocapture`
    #[test]
    #[ignore = "measures; writes files"]
    #[allow(clippy::print_stdout)]
    fn how_a_voice_sounds() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tempo");
        std::fs::create_dir_all(&dir).expect("dir");
        for (name, low) in [("voice", 1.0), ("low-voice", 0.65)] {
            let input = voice(6.0, low);
            let source = dir.join(format!("{name}.wav"));
            write_wav(&source, &input);
            for speed in [0.5, 0.75, 1.01, 1.5, 2.0, 3.0, 4.0] {
                let mut ring: VecDeque<f32> = input.iter().copied().collect();
                let mut s = Stretch::new(RATE, 2);
                let (out, _) = play(&mut s, &mut ring, speed);
                write_wav(&dir.join(format!("{name}-bananav-{speed}.wav")), &out);
                let ours = distance(&input, &out, speed);
                let theirs = dir.join(format!("{name}-ffmpeg-{speed}.wav"));
                let ffmpeg = std::process::Command::new("ffmpeg")
                    .args(["-y", "-v", "error", "-i"])
                    .arg(&source)
                    .args(["-af", &format!("atempo={speed}")])
                    .arg(&theirs)
                    .status()
                    .is_ok_and(|s| s.success());
                let other = if ffmpeg {
                    format!("{:.2}", distance(&input, &read_wav(&theirs), speed))
                } else {
                    "-".into()
                };
                println!("{name}, {speed}x: BananaV {ours:.2} dB, FFmpeg {other} dB");
            }
        }
    }

    /// A buffer that runs dry gives what there is and goes on later.
    #[test]
    fn a_dry_buffer_waits() {
        let mut s = Stretch::new(RATE, 2);
        let mut out = vec![0.0f32; 480 * 2];
        let mut ring = VecDeque::new();
        assert_eq!(s.pull(&mut ring, &mut out, 2.0), Pulled::default());
        ring.extend(tone(440.0, 1_000));
        assert_eq!(s.pull(&mut ring, &mut out, 2.0).frames, 0);
        ring.extend(tone(440.0, 20_000));
        assert_eq!(s.pull(&mut ring, &mut out, 2.0).frames, 480);
    }
}
