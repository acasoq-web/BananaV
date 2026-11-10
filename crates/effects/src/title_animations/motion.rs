//! A row's value at a moment, as VEGAS's titles play their rows: the curve
//! through its keys (held before the first and after the last), plus the
//! wave that started last (a damped spring, shrinking hops or a jitter);
//! a stagger's delay for each element of its list; and the size of the
//! blur or glow a row's number asks for.

use super::{Ease, Key, Shape, Stagger, Wave};

/// The curve through `keys` at `x`: the first key's value up to it, the
/// last one's from it; between two keys, the ease of the one starting the
/// piece. Auto and tangent pieces are cubic (Hermite) pieces whose slopes
/// come from the neighbouring keys — so a piece between equal values can
/// bulge — or from a key's own tangents.
pub(crate) fn curve(keys: &[Key], x: f64) -> f64 {
    let (Some(first), Some(last)) = (keys.first(), keys.last()) else {
        return 0.0;
    };
    if x.is_nan() || x < first.at {
        return first.value;
    }
    if x >= last.at {
        return last.value;
    }
    // The piece from keys[j] (at or before x) to keys[j + 1] (after it).
    let j = keys.partition_point(|k| k.at <= x).saturating_sub(1);
    let (a, b) = (keys[j], keys[j + 1]);
    let len = b.at - a.at;
    if len <= 0.0 {
        return b.value;
    }
    let p = (x - a.at) / len;
    let d = b.value - a.value;
    match a.ease {
        Ease::Linear => a.value + d * p,
        Ease::Slow => a.value + d * p * p,
        Ease::Fast => a.value + d * (1.0 - (1.0 - p) * (1.0 - p)),
        Ease::Smooth => a.value + d * p * p * (3.0 - 2.0 * p),
        Ease::Auto | Ease::Tangents(..) => {
            let out = match a.ease {
                Ease::Tangents(_, o) => o,
                _ => {
                    let (before, gap) = match j.checked_sub(1).map(|i| keys[i]) {
                        Some(k) => (k.value, a.at - k.at),
                        None => (a.value, len),
                    };
                    (0.5 * (a.value - before) + 0.5 * d) * 2.0 * len / (gap + len)
                }
            };
            let into = match b.ease {
                Ease::Tangents(i, _) => -i,
                _ => {
                    let (after, gap) = match keys.get(j + 2) {
                        Some(k) => (k.value, k.at - b.at),
                        None => (b.value, len),
                    };
                    (0.5 * d + 0.5 * (after - b.value)) * 2.0 * len / (gap + len)
                }
            };
            let (p2, p3) = (p * p, p * p * p);
            (2.0 * p3 - 3.0 * p2 + 1.0) * a.value
                + (3.0 * p2 - 2.0 * p3) * b.value
                + (p3 - 2.0 * p2 + p) * out
                + (p3 - p2) * into
        }
    }
}

/// A row's value at `x`: its curve, plus the last of its waves (sorted by
/// their start) that has started — held at its end once over.
pub(crate) fn value(keys: &[Key], waves: &[Wave], x: f64) -> f64 {
    let base = curve(keys, x);
    match waves.iter().rev().find(|w| w.start < x) {
        Some(w) => {
            let t = if x > w.start + w.length || w.length <= 0.0 {
                1.0
            } else {
                (x - w.start) / w.length
            };
            base + w.scale * shape(&w.shape, t)
        }
        None => base,
    }
}

/// A wave's shape at `t` (0–1 over its length).
pub(crate) fn shape(s: &Shape, t: f64) -> f64 {
    match *s {
        Shape::Spring {
            amplitude,
            frequency,
            damping,
        } => {
            amplitude
                * (-10.0 * damping * t).exp()
                * ((2.0 * std::f64::consts::PI * frequency * t).cos() - 1.0)
        }
        Shape::Hops {
            loops,
            decay,
            inner,
            settle,
        } => {
            let at = hops(t, loops, decay, inner);
            if settle {
                at - hops(0.0, loops, decay, inner)
            } else {
                at
            }
        }
        Shape::Jitter {
            frequency,
            octaves,
            gradients,
        } => (0..octaves.min(16))
            .map(|o| {
                let k = f64::from(1u32 << o);
                noise(t * frequency * k, gradients) / k
            })
            .sum(),
    }
}

/// Arches 0 → 1 → 0, the first `1 / loops` long and each next one `decay`
/// as long and as high, begun `inner` of the first one in.
fn hops(t: f64, loops: f64, decay: f64, inner: f64) -> f64 {
    let mut w = 1.0 / loops.max(1e-9);
    let (mut start, mut amp) = (0.0, 1.0);
    let at = t + inner * w;
    let mut s = at;
    for _ in 0..32 {
        if start <= at && at < start + w {
            s = (at - start) / w;
            break;
        }
        start += w;
        amp *= decay;
        w *= decay;
        if w < 1e-6 {
            s = 1.0;
            break;
        }
    }
    let s = s - s.floor();
    amp * 4.0 * s * (1.0 - s)
}

/// A smooth noise through 0 at every whole `x`, its slopes there from
/// `gradients`.
fn noise(x: f64, gradients: &[f64; 128]) -> f64 {
    let i = x.trunc();
    let a = (i as i64).rem_euclid(1024) as usize;
    let b = (a + 1) % 1024;
    let f = x - i;
    let ga = gradients.get(a).copied().unwrap_or(0.0);
    let gb = gradients.get(b).copied().unwrap_or(0.0);
    let (g0, g1) = (f * ga, (f - 1.0) * gb);
    g0 + f * f * (3.0 - 2.0 * f) * (g1 - g0)
}

/// An element's rank in a stagger's list: element `i` of `n`, the list
/// starting at `k` (0 for the first, 1 for the last, 0 when alone).
pub(crate) fn rank(i: usize, k: usize, n: usize) -> f64 {
    let span = n.saturating_sub(1 + k);
    if span == 0 {
        0.0
    } else {
        (i.saturating_sub(k)) as f64 / span as f64
    }
}

/// How much later an element of rank `rank` plays the stagger's rows (a
/// part of the media; negative: earlier).
pub(crate) fn delay(s: &Stagger, rank: f64) -> f64 {
    let shaped = match s.shape {
        Some(keys) => curve(keys, rank),
        None => rank,
    };
    shaped * s.duration
}

/// The blur's radius, in pixels, for each of its sizes (the size the blur
/// is made with, from 0 to 54).
const RADII: [Key; 12] = {
    const fn k(at: f64, value: f64) -> Key {
        Key {
            at,
            value,
            ease: Ease::Auto,
        }
    }
    [
        k(0.0, 0.5),
        k(0.75, 3.0),
        k(6.0, 25.0),
        k(12.0, 55.0),
        k(18.0, 88.0),
        k(24.0, 127.0),
        k(30.0, 200.0),
        k(36.0, 300.0),
        k(42.0, 445.0),
        k(48.0, 730.0),
        k(51.0, 1200.0),
        k(54.0, 3000.0),
    ]
};

/// The radius (pixels) a blur of size `s` reaches.
pub(crate) fn radius(s: f64) -> f64 {
    curve(&RADII, s)
}

/// The size of a blur reaching `r` pixels: none up to half a pixel, and
/// none beyond the largest (as VEGAS's titles do).
pub(crate) fn size(r: f64) -> f64 {
    if r.is_nan() || r <= 0.5 || r > 3000.0 {
        return 0.0;
    }
    let (mut lo, mut hi) = (0.0f64, 54.0f64);
    let mut mid = 0.5 * (lo + hi);
    for _ in 0..64 {
        mid = 0.5 * (lo + hi);
        let at = radius(mid);
        if (at - r).abs() <= 0.001 {
            break;
        }
        if at < r {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    mid
}

/// The blur's sigma in pixels for each unit of its size, as VEGAS's own
/// renders on the processor blur (fitted on Double Flash Glow's halo
/// halfway: a size of 2.235 blurs about 2.45 pixels).
pub(crate) const SIGMA_PER_SIZE: f64 = 1.1;

/// The sigma (pixels) of a blur or glow whose number is `v` (a part of the
/// frame's width: its diameter), in a frame `width` pixels wide.
pub(crate) fn sigma(v: f64, width: f64) -> f64 {
    if v.is_nan() || width.is_nan() || v <= 0.0 || width <= 0.0 {
        return 0.0;
    }
    size(v * width / 2.0) * SIGMA_PER_SIZE
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::title_animations::find;

    const fn key(at: f64, value: f64, ease: Ease) -> Key {
        Key { at, value, ease }
    }

    fn near(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    /// A row of an animation by its place in the list of rows.
    fn row(name: &str, i: usize) -> &'static super::super::Row {
        &find(name).unwrap().rows[i]
    }

    #[test]
    fn curves_ease_as_their_start_key_says() {
        let two = [key(0.0, 0.0, Ease::Auto), key(1.0, 1.0, Ease::Auto)];
        for (p, v) in [(0.25, 0.203125), (0.5, 0.5), (0.75, 0.796875)] {
            assert!(near(curve(&two, p), v, 1e-12), "{p}");
        }
        let lin = [key(0.0, 2.0, Ease::Linear), key(1.0, 4.0, Ease::Auto)];
        assert!(near(curve(&lin, 0.25), 2.5, 1e-12));
        let slow = [key(0.0, 0.0, Ease::Slow), key(1.0, 1.0, Ease::Auto)];
        assert!(near(curve(&slow, 0.5), 0.25, 1e-12));
        let fast = [key(0.0, 0.0, Ease::Fast), key(1.0, 1.0, Ease::Auto)];
        assert!(near(curve(&fast, 0.5), 0.75, 1e-12));
        let smooth = [key(0.0, 0.0, Ease::Smooth), key(1.0, 1.0, Ease::Auto)];
        assert!(near(curve(&smooth, 0.25), 0.15625, 1e-12));
        // Held before the first key and from the last; a key's own value
        // exactly on it.
        assert_eq!(curve(&lin, -3.0), 2.0);
        assert_eq!(curve(&lin, 1.0), 4.0);
        assert_eq!(curve(&lin, 7.0), 4.0);
        assert_eq!(curve(&lin, 0.0), 2.0);
    }

    #[test]
    fn slide_up_moves_and_fades_as_measured() {
        // Its block's Offset.Y (before the location is added) and opacity.
        let y = row("_Slide_Up", 0);
        for (u, v) in [
            (0.0, -3.0),
            (0.02, -2.326796),
            (0.0582, -1.000042),
            (0.3, -0.398887),
            (0.5, 0.098330),
            (0.9, 1.404428),
            (0.95, 2.334197),
            (0.99, 2.938776),
            (1.0, 3.0),
        ] {
            let got = value(y.keys, y.waves, u);
            assert!(near(got, v, 2e-6), "{u}: {got}");
        }
        let o = row("_Slide_Up", 1);
        for (u, v) in [
            (0.02, 0.258389),
            (0.3, 1.181672),
            (0.5, 1.227711),
            (0.9, 1.009545),
            (0.95, 0.616005),
            (0.99, 0.076005),
        ] {
            let got = value(o.keys, o.waves, u);
            assert!(near(got, v, 2e-6), "{u}: {got}");
        }
    }

    #[test]
    fn a_spring_overshoots_and_settles() {
        // Title01's first line across: −25 → 3.05 and a spring.
        let r = row("_Title01", 0);
        for (u, v) in [
            (0.01, -18.618554),
            (0.03, -0.732426),
            (0.05, 4.155765),
            (0.08, 3.184586),
            (0.2, 3.05),
        ] {
            let got = value(r.keys, r.waves, u);
            assert!(near(got, v, 2e-6), "{u}: {got}");
        }
        assert!(near(value(r.keys, r.waves, 0.0965), 3.05, 1e-9));
    }

    #[test]
    fn bounce_hops_lower_and_shorter_each_time() {
        let a = find("_Bounce").unwrap();
        let s = &a.staggers[0];
        for (r, d) in [(0.0, 0.0), (0.5, -0.5), (1.0, -1.0)] {
            assert!(near(delay(s, r) / s.duration, d, 1e-6), "{r}");
        }
        let r = &a.rows[0];
        for (u, v) in [
            (0.2, 5.0),
            (0.3, 4.954739),
            (0.35, 0.049324),
            (0.4, 4.225337),
            (0.5, 2.939450),
            (0.7, 0.626074),
            (0.9, 0.376921),
        ] {
            let got = value(r.keys, r.waves, u);
            assert!(near(got, v, 2e-6), "{u}: {got}");
        }
    }

    #[test]
    fn the_jitter_crosses_zero_at_whole_steps() {
        let a = find("_Earthquake").unwrap();
        let (x, y) = (&a.rows[0].waves[0].shape, &a.rows[1].waves[0].shape);
        for (t, vx, vy) in [
            (0.25, -0.083090, -0.053698),
            (0.77, 0.062993, -0.087943),
            (0.1, 0.0, 0.0),
            (0.5, 0.0, 0.0),
        ] {
            assert!(near(shape(x, t), vx, 2e-6), "{t}: {}", shape(x, t));
            assert!(near(shape(y, t), vy, 2e-6), "{t}: {}", shape(y, t));
        }
    }

    #[test]
    fn ranks_run_from_the_stagger_s_element_to_the_list_s_end() {
        assert_eq!(rank(0, 0, 13), 0.0);
        assert_eq!(rank(12, 0, 13), 1.0);
        assert!(near(rank(6, 0, 13), 0.5, 1e-12));
        // A list starting later: its first element 0, its last 1.
        assert_eq!(rank(11, 11, 17), 0.0);
        assert_eq!(rank(16, 11, 17), 1.0);
        // Alone: no delay.
        assert_eq!(rank(0, 0, 1), 0.0);
    }

    #[test]
    fn blur_sizes_follow_their_radius() {
        for (s, r) in [
            (0.5, 1.9907),
            (1.0, 4.0184),
            (2.0, 8.0624),
            (3.0, 12.1120),
            (6.0, 25.0),
        ] {
            assert!(near(radius(s), r, 1e-4), "{s}: {}", radius(s));
        }
        // A quarter of a 640-pixel frame: 160 across, a radius of 80.
        assert!(near(size(80.0), 16.631, 1e-3), "{}", size(80.0));
        assert_eq!(size(0.5), 0.0);
        assert_eq!(size(4000.0), 0.0);
        assert!(near(radius(size(9.01)), 9.01, 1e-3));
        assert_eq!(sigma(0.0, 640.0), 0.0);
        assert!(near(sigma(0.25, 640.0), 16.631 * SIGMA_PER_SIZE, 2e-3));
    }
}
