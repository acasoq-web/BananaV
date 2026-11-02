//! A mask's own tracking (Bézier Masking's Tracking › Start; BananaV's
//! `kBananaVParamPropMaskTracking` on the button), as VEGAS's Help
//! describes it ("Motion tracking a mask"): from the cursor the mask's
//! box is followed through the event's own picture — forward, backward or
//! both ways, at Normal or High Precision — by the engine's tracker, and
//! what it found becomes keyframes, every Keyframe Interval frames, of the
//! mask's Location, with Rotation & Location its Angle too, with Size,
//! Rotation & Location its Width and Height too (one undo step). The
//! status bar shows how far it is, with Cancel.

use bv_core::Tick;
use bv_engine::plugins::EffectKey;
use bv_engine::track::{Mode, Precision};
use bv_timeline::motion_tracks::Quad;
use bv_timeline::scene::FxOwner;
use bv_timeline::{ClipId, KeyCurve, MotionTrack, PluginKey, PluginParam, PluginValue};

use crate::app::BananaApp;
use crate::egui;
use crate::i18n::tr;

/// What a mask's Start button asked for: the effect, the button's spec
/// (the mask's number, then its location's, angle's, width's and
/// height's parameters, then its precision's, mode's, direction's and
/// keyframe interval's) and the effect's values then.
#[derive(Clone, Debug)]
pub(crate) struct MaskTrackAsk {
    pub key: EffectKey,
    pub spec: String,
    pub params: std::collections::BTreeMap<String, PluginParam>,
}

/// A mask being tracked.
#[derive(Debug)]
pub(crate) struct MaskTracking {
    key: EffectKey,
    clip: ClipId,
    /// The location's, angle's, width's and height's parameters.
    names: [String; 4],
    /// 0 the location, 1 and the angle, 2 and the size.
    mode: usize,
    interval: i64,
    /// Where it started (from the event's start), the box there, and the
    /// mask's angle, width and height there.
    from: Tick,
    start: Quad,
    values: (f64, f64, f64),
    found: MotionTrack,
    done: f32,
}

/// What a box tracked to `q` makes of a mask whose box was `start` with
/// `(angle, width, height)`, on a frame `aspect` wide for 1 high: its
/// location (0–1, y up), its angle (degrees) turned as the box turned, and
/// its width and height grown as the box grew.
pub(crate) fn mask_values(
    start: &Quad,
    q: &Quad,
    aspect: f64,
    (angle, width, height): (f64, f64, f64),
) -> ([f64; 2], f64, f64, f64) {
    // In the frame's proportions, y up.
    let up = |p: [f32; 2]| [f64::from(p[0]) * aspect, 1.0 - f64::from(p[1])];
    let center = q.iter().fold([0.0, 0.0], |c, p| {
        [c[0] + f64::from(p[0]) / 4.0, c[1] + f64::from(p[1]) / 4.0]
    });
    let edge = |q: &Quad, a: usize, b: usize| {
        let (p, r) = (up(q[a]), up(q[b]));
        [r[0] - p[0], r[1] - p[1]]
    };
    let len = |v: [f64; 2]| v[0].hypot(v[1]);
    let turn = |q: &Quad| {
        let e = edge(q, 0, 1);
        e[1].atan2(e[0])
    };
    let mut d = (turn(q) - turn(start)).to_degrees();
    if d > 180.0 {
        d -= 360.0;
    } else if d < -180.0 {
        d += 360.0;
    }
    let grow = |a: usize, b: usize| {
        let s = len(edge(start, a, b));
        if s > 1e-9 {
            len(edge(q, a, b)) / s
        } else {
            1.0
        }
    };
    (
        [center[0], 1.0 - center[1]],
        angle + d,
        width * grow(0, 1),
        height * grow(0, 3),
    )
}

impl BananaApp {
    /// Each frame: a mask's Start asked for, tracking started; what it
    /// found, once it is over, made keyframes.
    pub(crate) fn follow_mask_tracking(&mut self) {
        if let Some(ask) = self.ofx.take_mask_track() {
            self.start_mask_tracking(ask);
        }
        if self.mask_tracking.is_none() || self.motion.running.is_some() {
            return;
        }
        let Some(t) = self.engine.tracking() else {
            self.mask_tracking = None;
            return;
        };
        let frame = self.frame_rate().frame_duration();
        let Some(mt) = self.mask_tracking.as_mut() else {
            return;
        };
        for (at, quad) in &t.keys {
            mt.found.set_key(*at, *quad, frame);
        }
        mt.done = t.done;
        if t.finished {
            let Some(mt) = self.mask_tracking.take() else {
                return;
            };
            self.keep_mask_keys(&mt);
            self.notify(match t.lost {
                Some(l) => tr(l.text()).to_string(),
                None => tr("The mask was tracked.").to_string(),
            });
        }
    }

    fn start_mask_tracking(&mut self, ask: MaskTrackAsk) {
        let FxOwner::Event(clip) = ask.key.owner else {
            self.notify(tr("Masks are tracked in an event's effects."));
            return;
        };
        let words: Vec<&str> = ask.spec.split_whitespace().collect();
        let [n, loc, ang, w, h, precision, mode, direction, interval] = words[..] else {
            return;
        };
        let Ok(mask) = n.parse::<usize>() else {
            return;
        };
        let Some(c) = self.project().clip(clip).cloned() else {
            return;
        };
        let Some(fx) = c
            .effects
            .get(ask.key.index)
            .and_then(|e| e.plugin.as_deref())
            .filter(|f| f.id == ask.key.id)
            .cloned()
        else {
            return;
        };
        let s = &self.project().settings;
        let aspect = s.width as f64 * f64::from(s.pixel_aspect.ratio()) / s.height.max(1) as f64;
        let rate = s.frame_rate;
        let from = (rate.frame_start(rate.frame_at(self.cursor)) - c.start)
            .clamp(Tick::ZERO, (c.duration - Tick(1)).max(Tick::ZERO));
        let values = crate::motion_tracking::values_at(&fx, from, rate);
        let Some(start) = bv_engine::Engine::tracked_region(&fx.id, &values, aspect, mask)
            .map(|q| q.map(|c| c.map(|v| v as f32)))
        else {
            self.notify(tr("The mask is off: there is nothing to track."));
            return;
        };
        let num = |name: &str, default: f64| {
            values
                .iter()
                .find(|(n, _)| n == name)
                .and_then(|(_, v)| match v {
                    bv_ofx::Value::Num(v) => v.first().copied(),
                    _ => None,
                })
                .unwrap_or(default)
        };
        let choice = |name: &str| {
            ask.params
                .get(name)
                .and_then(|p| match &p.value {
                    PluginValue::Num(v) => v.first().copied(),
                    _ => None,
                })
                .unwrap_or(0.0)
                .round() as i64
        };
        let mode = choice(mode).clamp(0, 2) as usize;
        let interval = choice(interval).max(1);
        let started = self.engine.start_tracking(bv_engine::TrackRequest {
            clip,
            from,
            direction: match choice(direction) {
                1 => bv_engine::TrackDirection::Backward,
                2 => bv_engine::TrackDirection::Both,
                _ => bv_engine::TrackDirection::Forward,
            },
            one_frame: false,
            quad: start,
            outside: false,
            mode: [Mode::Location, Mode::RotationLocation, Mode::ShapeLocation][mode],
            precision: if choice(precision) >= 1 {
                Precision::Accurate
            } else {
                Precision::Balanced
            },
        });
        if !started {
            return;
        }
        let mut found = MotionTrack::new(String::new());
        found.set_key(from, start, rate.frame_duration());
        self.motion.status = None;
        self.mask_tracking = Some(MaskTracking {
            key: ask.key,
            clip,
            names: [loc, ang, w, h].map(str::to_string),
            mode,
            interval: interval.max(1),
            from,
            start,
            values: (num(ang, 0.0), num(w, 0.5), num(h, 0.5)),
            found,
            done: 0.0,
        });
    }

    /// What a mask's tracking found as its keyframes, every Keyframe
    /// Interval frames from where it started (and its ends), those of the
    /// span it covered taking the place of the ones there (one undo).
    fn keep_mask_keys(&mut self, mt: &MaskTracking) {
        let rate = self.frame_rate();
        let s = &self.project().settings;
        let aspect = s.width as f64 * f64::from(s.pixel_aspect.ratio()) / s.height.max(1) as f64;
        let keys = &mt.found.keys;
        let (Some(first), Some(last)) = (keys.first(), keys.last()) else {
            return;
        };
        let (lo, hi) = (first.at, last.at);
        let from = rate.frame_at(mt.from);
        let kept: Vec<_> = keys
            .iter()
            .filter(|k| {
                let n = rate.frame_at(k.at) - from;
                n % mt.interval == 0 || k.at == lo || k.at == hi
            })
            .collect();
        let mut made: [Vec<PluginKey>; 4] = Default::default();
        for k in kept {
            let (loc, angle, w, h) = mask_values(&mt.start, &k.quad, aspect, mt.values);
            made[0].push(PluginKey::new(
                k.at,
                PluginValue::Num(loc.to_vec()),
                KeyCurve::Linear,
            ));
            made[1].push(PluginKey::new(
                k.at,
                PluginValue::Num(vec![angle]),
                KeyCurve::Linear,
            ));
            made[2].push(PluginKey::new(
                k.at,
                PluginValue::Num(vec![w]),
                KeyCurve::Linear,
            ));
            made[3].push(PluginKey::new(
                k.at,
                PluginValue::Num(vec![h]),
                KeyCurve::Linear,
            ));
        }
        // The location always; the angle with rotation; the size with it.
        let wanted = [true, mt.mode >= 1, mt.mode >= 2, mt.mode >= 2];
        let key = mt.key.clone();
        let names = mt.names.clone();
        self.modify(|p| {
            let Some(fx) = p
                .clip_mut(mt.clip)
                .and_then(|c| c.effects.get_mut(key.index))
                .and_then(|e| e.plugin.as_deref_mut())
                .filter(|f| f.id == key.id)
            else {
                return;
            };
            for (k, name) in names.iter().enumerate() {
                if !wanted[k] {
                    continue;
                }
                let param = fx
                    .params
                    .entry(name.clone())
                    .or_insert_with(|| PluginParam {
                        value: made[k]
                            .first()
                            .map_or(PluginValue::Num(Vec::new()), |k| k.value.clone()),
                        keys: Vec::new(),
                    });
                param.keys.retain(|x| x.at < lo || x.at > hi);
                param.keys.extend(made[k].iter().cloned());
                param.keys.sort_by_key(|x| x.at);
            }
        });
    }
}

/// A mask's tracking in the status bar: how far, with Cancel.
pub(crate) fn status(app: &mut BananaApp, ui: &mut egui::Ui) {
    let Some(done) = app.mask_tracking.as_ref().map(|m| m.done) else {
        return;
    };
    if ui.button(tr("Cancel")).clicked() {
        app.engine.cancel_tracking();
        app.mask_tracking = None;
        return;
    }
    ui.add(
        egui::ProgressBar::new(done)
            .desired_width(200.0)
            .show_percentage(),
    );
    ui.label(tr("Tracking the mask…"));
    ui.separator();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A box moved, turned 30° and grown half as big again: the mask's
    /// location, angle and size follow.
    #[test]
    fn a_tracked_box_moves_turns_and_grows_the_mask() {
        let aspect = 16.0 / 9.0;
        // A box 0.2 × 0.2 of the frame's height around (0.5, 0.5), y down,
        // in 0–1 of each side.
        let boxed = |cx: f64, cy: f64, half: f64, turn: f64| -> Quad {
            let (s, c) = turn.to_radians().sin_cos();
            [[-1.0, 1.0], [1.0, 1.0], [1.0, -1.0], [-1.0, -1.0]].map(|[x, y]: [f64; 2]| {
                // y up in the frame's proportions, then back to 0–1 down.
                let (x, y) = (x * half, y * half);
                let (rx, ry) = (x * c - y * s, x * s + y * c);
                [
                    ((cx * aspect + rx) / aspect) as f32,
                    (1.0 - (cy + ry)) as f32,
                ]
            })
        };
        let start = boxed(0.5, 0.5, 0.1, 0.0);
        let now = boxed(0.6, 0.4, 0.15, 30.0);
        let (loc, angle, w, h) = mask_values(&start, &now, aspect, (10.0, 0.2, 0.3));
        assert!(
            (loc[0] - 0.6).abs() < 1e-4 && (loc[1] - 0.4).abs() < 1e-4,
            "{loc:?}"
        );
        assert!((angle - 40.0).abs() < 1e-3, "{angle}");
        assert!((w - 0.3).abs() < 1e-4 && (h - 0.45).abs() < 1e-4, "{w} {h}");
    }
}
