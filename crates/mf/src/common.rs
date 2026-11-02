//! What the decoders of Windows and FFmpeg share.

use bv_core::FrameRate;

/// The common rate nearest `fps` within 0.2 % of it (30 for 30, 29.97 for
/// 29.98), or `fps` itself.
pub(crate) fn rate_near(fps: f64) -> Option<FrameRate> {
    const COMMON: [FrameRate; 16] = [
        FrameRate::FPS_23_976,
        FrameRate::FPS_24,
        FrameRate::FPS_25,
        FrameRate::FPS_29_97,
        FrameRate::FPS_30,
        FrameRate::new(48, 1),
        FrameRate::FPS_50,
        FrameRate::FPS_59_94,
        FrameRate::FPS_60,
        FrameRate::new(100, 1),
        FrameRate::new(120_000, 1001),
        FrameRate::new(120, 1),
        FrameRate::new(15, 1),
        FrameRate::new(12, 1),
        FrameRate::new(10, 1),
        FrameRate::new(20, 1),
    ];
    if !(fps.is_finite() && fps > 0.5 && fps < 1000.0) {
        return None;
    }
    let off = |r: &FrameRate| (r.as_f64() - fps).abs() / fps;
    Some(
        COMMON
            .into_iter()
            .filter(|r| off(r) < 0.002)
            .min_by(|a, b| off(a).total_cmp(&off(b)))
            .unwrap_or_else(|| FrameRate::new((fps * 1000.0).round() as u32, 1000)),
    )
}

/// Interleaved samples of `from` channels as `to` channels (the first ones
/// kept, missing ones silent).
pub(crate) fn remap(samples: &[f32], from: usize, to: usize) -> Vec<f32> {
    if from == 0 {
        return Vec::new();
    }
    let frames = samples.len() / from;
    let mut out = vec![0.0; frames * to];
    for f in 0..frames {
        for c in 0..from.min(to) {
            out[f * to + c] = samples[f * from + c];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_go_to_the_nearest_common_one() {
        assert_eq!(rate_near(30.0), Some(FrameRate::FPS_30));
        assert_eq!(rate_near(30000.0 / 1001.0), Some(FrameRate::FPS_29_97));
        assert_eq!(rate_near(29.98), Some(FrameRate::FPS_29_97));
        assert_eq!(rate_near(59.95), Some(FrameRate::FPS_59_94));
        assert_eq!(rate_near(12.5), Some(FrameRate::new(12_500, 1000)));
        assert_eq!(rate_near(f64::NAN), None);
    }
}
