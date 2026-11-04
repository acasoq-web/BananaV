//! Volume and Pan, as VEGAS's: a linear gain, and a balance that moves
//! one channel into the other (the channels' sum kept).

use bv_timeline::audio_fx::{pan, volume};

use super::{Own, Ramp, gain_ramp};

/// Volume: every sample of every channel times one gain, 0 to 2.
#[derive(Debug)]
pub(crate) struct Volume {
    gain: Ramp,
}

impl Default for Volume {
    fn default() -> Volume {
        Volume {
            gain: Ramp::new(1.0),
        }
    }
}

impl Own for Volume {
    fn set(&mut self, params: &[f32]) {
        self.gain.set(
            params
                .get(volume::GAIN)
                .copied()
                .unwrap_or(1.0)
                .clamp(0.0, 2.0),
        );
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let (start, step) = self.gain.block(n);
        for ch in channels.iter_mut() {
            gain_ramp(ch, start, step);
        }
    }
}

/// Pan: on a stereo pair, `p` from −1 to 1, positive to the left — the
/// left takes `p` of the right and the right keeps 1 − `p` of itself;
/// below 0 the other way round. Other layouts pass as they are.
#[derive(Debug)]
pub(crate) struct Pan {
    pan: Ramp,
}

impl Default for Pan {
    fn default() -> Pan {
        Pan {
            pan: Ramp::new(0.0),
        }
    }
}

impl Own for Pan {
    fn set(&mut self, params: &[f32]) {
        self.pan.set(
            params
                .get(pan::PAN)
                .copied()
                .unwrap_or(0.0)
                .clamp(-1.0, 1.0),
        );
    }

    fn process(&mut self, channels: &mut [Vec<f32>]) {
        let n = channels.first().map_or(0, Vec::len);
        let (start, step) = self.pan.block(n);
        let [l, r] = channels else {
            return;
        };
        let n = n.min(l.len()).min(r.len());
        // Each sample's own pan: the ramp's, unrolled into two branch-free
        // lines — max(p, 0) of the right to the left, max(−p, 0) of the
        // left to the right — so the loop vectorizes.
        for i in 0..n {
            let p = start + step * i as f32;
            let to_left = p.max(0.0);
            let to_right = (-p).max(0.0);
            let (a, b) = (l[i], r[i]);
            l[i] = a * (1.0 - to_right) + b * to_left;
            r[i] = b * (1.0 - to_left) + a * to_right;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_scales_by_its_gain() {
        let mut v = Volume::default();
        v.set(&[0.5]);
        let mut ch = vec![vec![1.0f32; 8], vec![-0.5; 8]];
        v.process(&mut ch);
        assert!(ch[0].iter().all(|&s| (s - 0.5).abs() < 1e-6));
        assert!(ch[1].iter().all(|&s| (s + 0.25).abs() < 1e-6));
    }

    #[test]
    fn volume_ramps_to_a_new_gain_without_a_jump() {
        let mut v = Volume::default();
        v.set(&[1.0]);
        let mut ch = vec![vec![1.0f32; 4]];
        v.process(&mut ch);
        v.set(&[0.0]);
        let mut ch = vec![vec![1.0f32; 4]];
        v.process(&mut ch);
        assert_eq!(ch[0], [1.0, 0.75, 0.5, 0.25]);
        let mut ch = vec![vec![1.0f32; 4]];
        v.process(&mut ch);
        assert_eq!(ch[0], [0.0; 4]);
    }

    #[test]
    fn pan_moves_one_channel_into_the_other_keeping_their_sum() {
        for p in [-1.0f32, -0.5, 0.0, 0.25, 1.0] {
            let mut pan = Pan::default();
            pan.set(&[p]);
            let mut ch = vec![vec![0.6f32; 16], vec![-0.2; 16]];
            pan.process(&mut ch);
            let (l, r) = (ch[0][5], ch[1][5]);
            assert!((l + r - 0.4).abs() < 1e-6, "{p}: {l} {r}");
            if p >= 0.0 {
                assert!((l - (0.6 + p * -0.2)).abs() < 1e-6);
                assert!((r - (1.0 - p) * -0.2).abs() < 1e-6);
            } else {
                assert!((l - (1.0 + p) * 0.6).abs() < 1e-6);
                assert!((r - (-0.2 - p * 0.6)).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn pan_leaves_other_layouts_alone() {
        let mut pan = Pan::default();
        pan.set(&[1.0]);
        let mut ch = vec![vec![0.3f32; 4]];
        pan.process(&mut ch);
        assert_eq!(ch[0], [0.3; 4]);
    }
}
