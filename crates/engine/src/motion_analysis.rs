//! Video Stabilization's Analyze Motion (BananaV's
//! `kBananaVParamPropAnalysis` "Motion"): the picture an event's or a
//! media's effects get — the event alone, framed by its Pan/Crop, or the
//! media alone, without effects — drawn frame after frame in the
//! background, and its motion from each frame to the next found by
//! `bv_track` over the whole frame (an affine fit, kept as a shift, a
//! turn and a scale), for the effect to keep as text
//! (`bv_effects::stabilization::motion_text`). The same picture's light
//! over its frames ("Histogram": Brightness and Contrast's Current Clip,
//! `bv_effects::brightness_contrast::Light`) or frame by frame
//! ("Histograms": its Set for each Frame independently), each frame's
//! color histograms ("ColorHistograms": Flicker Control's Calculate), and
//! an equirectangular picture's turns ("Rotation") too.

use std::sync::{Arc, Mutex};

use bv_core::Tick;
use bv_effects::stabilization::Step;
use bv_media::VideoPool;
use bv_timeline::scene::FxOwner;
use bv_timeline::{Project, TrackKind};

use crate::accel::Painter;
use crate::plugins::Purpose;

/// What an analysis looks for (BananaV's `kBananaVParamPropAnalysis`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalysisKind {
    /// The picture's shift, turn and scale from each frame to the next.
    Motion,
    /// An equirectangular picture's turns: `exclude` leaves poles out (1
    /// the north, 2 the south, 3 both), `layout` says where the first eye
    /// is (1 the top half, 2 the left half).
    Rotation { exclude: i32, layout: i32 },
    /// The picture's light (Rec. 709's weights of its colors, as the
    /// effects get them), counted over its frames.
    Histogram,
    /// The same, each frame's apart.
    Histograms,
    /// Each frame's red, green and blue levels counted apart, as Flicker
    /// Control counts them.
    ColorHistograms,
}

/// The features a motion analysis followed: each frame's, from where to
/// where in the next, in pixels of frames `size` big (Video
/// Stabilization's Show Points).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FeatureTracks {
    pub size: (u32, u32),
    pub frames: Vec<Vec<([f32; 2], [f32; 2])>>,
}

impl FeatureTracks {
    /// Each feature of frame `k` and its track over the next frames (at
    /// most `reach`): the points it went through, as far as a feature of
    /// each next frame starts where it ended.
    pub fn tracks_at(&self, k: usize, reach: usize) -> Vec<Vec<[f32; 2]>> {
        let Some(first) = self.frames.get(k) else {
            return Vec::new();
        };
        // Re-found within a pixel and a half.
        let near = 1.5f32;
        first
            .iter()
            .map(|&(from, to)| {
                let mut path = vec![from, to];
                for next in self.frames.iter().skip(k + 1).take(reach.saturating_sub(1)) {
                    let at = *path.last().unwrap_or(&to);
                    match next
                        .iter()
                        .find(|(a, _)| (a[0] - at[0]).hypot(a[1] - at[1]) <= near)
                    {
                        Some(&(_, b)) => path.push(b),
                        None => break,
                    }
                }
                path
            })
            .collect()
    }
}

/// Where a motion analysis looks and on how big frames (Video
/// Stabilization's Expert view): the area followed — its left, top, right
/// and bottom as parts of the frame, rows down — and how many times the
/// frames are halved (else the tuning's width).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MotionSetup {
    pub area: Option<[f64; 4]>,
    pub halvings: Option<u32>,
}

/// How motion analysis goes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnalysisProgress {
    pub owner: Option<FxOwner>,
    /// Frames done, of all.
    pub done: u64,
    pub total: u64,
    pub finished: bool,
    /// What was found — the motion, the turns, the light —, as the effect
    /// keeps it (when finished).
    pub motion: Option<String>,
    /// The features followed (a motion analysis's, when finished).
    pub points: Option<Arc<FeatureTracks>>,
}

pub(crate) struct Analysis {
    shared: Arc<Mutex<(AnalysisProgress, bool)>>,
}

/// The picture `owner`'s effects get, alone: the project to draw, where
/// its frames start and how long it is.
fn picture_of(project: &Project, owner: FxOwner) -> Option<(Project, Tick, Tick)> {
    match owner {
        FxOwner::Event(clip) => {
            let c = project.clip(clip)?;
            let (start, duration) = (c.start, c.duration);
            let p = crate::motion_tracking::picture_of(project, clip)?;
            Some((p, start, duration))
        }
        FxOwner::Media(id) => {
            let info = project.media(id)?;
            let duration = info.duration;
            let mut p = Project::new(project.settings);
            p.add_media(info.clone());
            let v = p.tracks.iter().find(|t| t.kind == TrackKind::Video)?.id;
            if let Some(m) = p.media.iter_mut().find(|m| m.id == id) {
                m.effects.clear();
            }
            let c = p
                .insert_media(id, Tick::ZERO, Some(v), None)
                .ok()?
                .first()
                .copied()?;
            if let Some(clip) = p.clip_mut(c) {
                clip.effects.clear();
            }
            p.tracks
                .retain(|t| t.kind == TrackKind::Video && !t.clips.is_empty());
            Some((p, Tick::ZERO, duration))
        }
        _ => None,
    }
}

impl crate::Engine {
    /// Starts analyzing the motion `owner`'s effects see (an event's or a
    /// media's), `accurate` or fast (`tuning`: work of its own; `setup`:
    /// the area and the frames' size); false when there is no such
    /// picture.
    pub fn start_motion_analysis(
        &mut self,
        owner: FxOwner,
        kind: AnalysisKind,
        accurate: bool,
        tuning: Option<bv_track::Tuning>,
        setup: MotionSetup,
    ) -> bool {
        self.cancel_motion_analysis();
        let Some((project, start, duration)) = picture_of(&self.project, owner) else {
            return false;
        };
        let shared = Arc::new(Mutex::new((
            AnalysisProgress {
                owner: Some(owner),
                ..Default::default()
            },
            false,
        )));
        let accel = self.accel.clone();
        let out = shared.clone();
        let repaint = self.repaint.clone();
        std::thread::Builder::new()
            .name("motion-analysis".into())
            .spawn(move || {
                let mut points = None;
                let text = match kind {
                    AnalysisKind::Motion => {
                        let span = (start, duration);
                        let found = run(&project, span, (accurate, tuning, setup), accel, &out);
                        found.map(|(s, p)| {
                            points = Some(Arc::new(p));
                            bv_effects::stabilization::motion_text(&s)
                        })
                    }
                    AnalysisKind::Rotation { exclude, layout } => {
                        let span = (start, duration);
                        let turns = turns(&project, span, (exclude, layout), accurate, accel, &out);
                        turns.map(|t| bv_effects::stabilization_360::rotation_text(&t))
                    }
                    AnalysisKind::Histogram => {
                        light(&project, (start, duration), false, accel, &out)
                            .and_then(|l| l.first().map(|(_, l)| l.text()))
                    }
                    AnalysisKind::Histograms => {
                        light(&project, (start, duration), true, accel, &out)
                            .map(|l| bv_effects::brightness_contrast::Light::frames_text(&l))
                    }
                    AnalysisKind::ColorHistograms => {
                        colors(&project, (start, duration), accel, &out)
                            .map(|f| bv_effects::flicker_control::histograms_text(&f))
                    }
                };
                if let Ok(mut s) = out.lock() {
                    s.0.motion = text;
                    s.0.points = points;
                    s.0.finished = true;
                }
                repaint();
            })
            .ok();
        self.analysis = Some(Analysis { shared });
        true
    }

    /// How motion analysis goes (`None` while none is going on); once
    /// finished, it is over.
    pub fn motion_analysis(&mut self) -> Option<AnalysisProgress> {
        let a = self.analysis.as_ref()?;
        let p = a.shared.lock().ok()?.0.clone();
        if p.finished {
            self.analysis = None;
        }
        Some(p)
    }

    pub fn cancel_motion_analysis(&mut self) {
        if let Some(a) = self.analysis.take()
            && let Ok(mut s) = a.shared.lock()
        {
            s.1 = true;
        }
    }
}

/// Analyzes on this thread: each frame's move to the next and the
/// features followed (`None` when it was canceled).
fn run(
    project: &Project,
    (start, duration): (Tick, Tick),
    (accurate, tuning, setup): (bool, Option<bv_track::Tuning>, MotionSetup),
    accel: Arc<crate::accel::Accel>,
    out: &Mutex<(AnalysisProgress, bool)>,
) -> Option<(Vec<Step>, FeatureTracks)> {
    let rate = project.settings.frame_rate;
    let precision = if accurate {
        bv_track::Precision::Accurate
    } else {
        bv_track::Precision::Fast
    };
    let tuning = tuning.unwrap_or(precision.tuning());
    let (pw, ph) = (
        project.settings.width.max(1),
        project.settings.height.max(1),
    );
    let w = match setup.halvings {
        Some(k) => (pw >> k.min(4)).clamp(64.min(pw), pw),
        None => tuning.width.clamp(64, pw.max(64)).min(pw),
    };
    let h = ((w as u64 * ph as u64) / pw as u64).max(2) as u32;
    let mut painter = Painter::new(accel, Purpose::File);
    let mut pool = VideoPool::new();
    let mut draw = |n: i64| {
        let img = painter.linear(
            project,
            rate.frame_start(n),
            (w, h),
            &mut pool,
            [0.0, 0.0, 0.0, 1.0],
        );
        bv_track::Gray::from_rgba(img.width as usize, img.height as usize, &img.data)
    };
    let first = rate.frame_at(start);
    let last = rate.frame_at(start + duration - Tick(1)).max(first);
    let total = (last - first).max(0) as u64;
    if let Ok(mut s) = out.lock() {
        s.0.total = total;
    }
    // The area asked for, or the whole frame but a margin: what the
    // camera's move shifts.
    let (fw, fh) = (w as f32, h as f32);
    let [l, t, r, b] = setup
        .area
        .map_or([0.03, 0.03, 0.97, 0.97], |a| a.map(|v| v.clamp(0.0, 1.0)));
    let (l, t, r, b) = (l as f32 * fw, t as f32 * fh, r as f32 * fw, b as f32 * fh);
    let quad = [[l, t], [r, t], [r, b], [l, b]];
    let center = [fw as f64 / 2.0, fh as f64 / 2.0];
    let mut steps = Vec::with_capacity(total as usize);
    let mut points = FeatureTracks {
        size: (w, h),
        frames: Vec::with_capacity(total as usize),
    };
    let mut prev = draw(first);
    for n in first + 1..=last {
        if out.lock().map_or(true, |s| s.1) {
            return None;
        }
        let next = draw(n);
        let mode = bv_track::Mode::ShapeLocation;
        let step = match bv_track::track_tuned(&prev, &next, quad, false, mode, tuning) {
            Ok(s) => {
                points.frames.push(s.vectors.clone());
                // The affine move about the middle, as a shift, a turn and
                // a scale, and its whole linear part.
                let m = s.transform.0;
                let c = s.transform.apply([center[0] as f32, center[1] as f32]);
                Step {
                    dx: (c[0] as f64 - center[0]) / w as f64,
                    dy: (c[1] as f64 - center[1]) / w as f64,
                    turn: (m[3] - m[1]).atan2(m[0] + m[4]),
                    scale: (m[0] * m[4] - m[1] * m[3]).abs().sqrt(),
                    linear: [m[0], m[1], m[3], m[4]],
                }
            }
            // Lost (a cut, a flat picture): no move.
            Err(_) => {
                points.frames.push(Vec::new());
                Step::default()
            }
        };
        steps.push(step);
        prev = next;
        if let Ok(mut s) = out.lock() {
            s.0.done = (n - first) as u64;
        }
    }
    Some((steps, points))
}

/// The light of the picture the effects get (`None` when canceled): each
/// frame drawn whole, at the project's size, every pixel counted (fewer,
/// or a smaller frame, move a picture's ends), its colors as the effects
/// get them — encoded with BT.709's curve where the project composites in
/// video gamma —, what is see-through left out. Over all the frames
/// together (one light, at 0), or (`each`) each frame's at its time (the
/// effects', in frames from the picture's start).
fn light(
    project: &Project,
    (start, duration): (Tick, Tick),
    each: bool,
    accel: Arc<crate::accel::Accel>,
    out: &Mutex<(AnalysisProgress, bool)>,
) -> Option<Vec<(f64, bv_effects::brightness_contrast::Light)>> {
    use bv_effects::brightness_contrast::Light;
    let rate = project.settings.frame_rate;
    let size = (
        project.settings.width.max(1),
        project.settings.height.max(1),
    );
    let encoded = project.settings.composites_in_video_gamma();
    let mut painter = Painter::new(accel, Purpose::File);
    let mut pool = VideoPool::new();
    let first = rate.frame_at(start);
    let last = rate.frame_at(start + duration - Tick(1)).max(first);
    if let Ok(mut s) = out.lock() {
        s.0.total = (last - first + 1) as u64;
    }
    let mut lights = vec![(0.0, Light::new())];
    for n in first..=last {
        if out.lock().map_or(true, |s| s.1) {
            return None;
        }
        let at = rate.frame_start(n);
        if each && n > first {
            lights.push((0.0, Light::new()));
        }
        let (time, light) = lights.last_mut()?;
        if each {
            *time = (at - start).max(Tick::ZERO).as_seconds_f64() * rate.as_f64();
        }
        let img = painter.linear(project, at, size, &mut pool, [0.0; 4]);
        for &[r, g, b, a] in &img.data {
            if a <= 1e-6 {
                continue;
            }
            let c = [r / a, g / a, b / a];
            light.add(if encoded {
                c.map(|v| bv_color::transfer::linear_to_bt709(v.max(0.0)))
            } else {
                c
            });
        }
        if let Ok(mut s) = out.lock() {
            s.0.done = (n - first + 1) as u64;
        }
    }
    Some(lights)
}

/// Each frame's color histograms (`None` when canceled): drawn whole at
/// the project's size, every pixel's channels counted as the effects get
/// them — premultiplied, encoded with BT.709's curve where the project
/// composites in video gamma — in Flicker Control's levels, at the frame's
/// time (the effects', in frames from the picture's start).
fn colors(
    project: &Project,
    (start, duration): (Tick, Tick),
    accel: Arc<crate::accel::Accel>,
    out: &Mutex<(AnalysisProgress, bool)>,
) -> Option<Vec<(f64, [Vec<u64>; 3])>> {
    use bv_effects::flicker_control::{LEVELS, level};
    let rate = project.settings.frame_rate;
    let size = (
        project.settings.width.max(1),
        project.settings.height.max(1),
    );
    let encoded = project.settings.composites_in_video_gamma();
    let mut painter = Painter::new(accel, Purpose::File);
    let mut pool = VideoPool::new();
    let first = rate.frame_at(start);
    let last = rate.frame_at(start + duration - Tick(1)).max(first);
    if let Ok(mut s) = out.lock() {
        s.0.total = (last - first + 1) as u64;
    }
    // A channel as the effects get it.
    let got = |v: f32, a: f32| {
        if encoded && a > 0.0 {
            let s = v / a;
            s.signum() * bv_color::transfer::linear_to_bt709(s.abs()) * a
        } else {
            v
        }
    };
    let mut frames = Vec::new();
    for n in first..=last {
        if out.lock().map_or(true, |s| s.1) {
            return None;
        }
        let at = rate.frame_start(n);
        let time = (at - start).max(Tick::ZERO).as_seconds_f64() * rate.as_f64();
        let img = painter.linear(project, at, size, &mut pool, [0.0; 4]);
        let mut counts = [vec![0u64; LEVELS], vec![0u64; LEVELS], vec![0u64; LEVELS]];
        for &[r, g, b, a] in &img.data {
            for (c, v) in [r, g, b].into_iter().enumerate() {
                counts[c][level(got(v, a))] += 1;
            }
        }
        frames.push((time, counts));
        if let Ok(mut s) = out.lock() {
            s.0.done = (n - first + 1) as u64;
        }
    }
    Some(frames)
}

/// A direction of the sphere for an equirectangular picture's u (0 left)
/// and v (0 top): x across, y up, z ahead.
fn direction(u: f64, v: f64) -> [f64; 3] {
    let lon = (u - 0.5) * std::f64::consts::TAU;
    let lat = (0.5 - v) * std::f64::consts::PI;
    [lat.cos() * lon.sin(), lat.sin(), lat.cos() * lon.cos()]
}

/// The small turn (a rotation vector) that takes directions `p` to `q`
/// best: least squares of `q − p = ω × p`.
fn turn_fit(pairs: &[([f64; 3], [f64; 3])]) -> Option<[f64; 3]> {
    let mut m = [[0.0f64; 3]; 3];
    let mut b = [0.0f64; 3];
    for (p, q) in pairs {
        // ω × p = A ω, with A = −[p]×.
        let a = [[0.0, p[2], -p[1]], [-p[2], 0.0, p[0]], [p[1], -p[0], 0.0]];
        let d = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
        for i in 0..3 {
            for j in 0..3 {
                m[i][j] += (0..3).map(|k| a[k][i] * a[k][j]).sum::<f64>();
            }
            b[i] += (0..3).map(|k| a[k][i] * d[k]).sum::<f64>();
        }
    }
    let det = |m: &[[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let d = det(&m);
    if d.abs() < 1e-12 {
        return None;
    }
    // Cramer's rule.
    Some(std::array::from_fn(|c| {
        let mut mc = m;
        for (row, v) in mc.iter_mut().zip(b) {
            row[c] = v;
        }
        det(&mc) / d
    }))
}

/// The turn most pairs agree with (RANSAC on pairs of pairs), refitted on
/// them (none: no turn).
fn robust_turn(pairs: &[([f64; 3], [f64; 3])], agree: f64) -> [f64; 3] {
    use bv_effects::stabilization_360::rotate;
    if pairs.len() < 3 {
        return [0.0; 3];
    }
    let fits = |w: [f64; 3]| -> Vec<([f64; 3], [f64; 3])> {
        pairs
            .iter()
            .filter(|(p, q)| {
                let r = rotate(w, *p);
                ((r[0] - q[0]).powi(2) + (r[1] - q[1]).powi(2) + (r[2] - q[2]).powi(2)).sqrt()
                    <= agree
            })
            .copied()
            .collect()
    };
    let mut best: Vec<([f64; 3], [f64; 3])> = Vec::new();
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    for _ in 0..200 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let i = (seed % pairs.len() as u64) as usize;
        let j = ((seed >> 20) % pairs.len() as u64) as usize;
        if i == j {
            continue;
        }
        let Some(w) = turn_fit(&[pairs[i], pairs[j]]) else {
            continue;
        };
        let kept = fits(w);
        if kept.len() > best.len() {
            best = kept;
        }
    }
    // Refitted on them, the turn found taken off and fitted again (the
    // straight fit's error is of the turn's square).
    let mut w = turn_fit(&best).unwrap_or([0.0; 3]);
    for _ in 0..2 {
        let moved: Vec<([f64; 3], [f64; 3])> =
            best.iter().map(|(p, q)| (rotate(w, *p), *q)).collect();
        if let Some(d) = turn_fit(&moved) {
            w = [w[0] + d[0], w[1] + d[1], w[2] + d[2]];
        }
    }
    w
}

/// Each frame's turn to the next of an equirectangular picture (`None`
/// when canceled): the features of the first eye's picture, poles left
/// out as `(exclude, layout)` say, followed and fitted as a turn.
fn turns(
    project: &Project,
    (start, duration): (Tick, Tick),
    (exclude, layout): (i32, i32),
    accurate: bool,
    accel: Arc<crate::accel::Accel>,
    out: &Mutex<(AnalysisProgress, bool)>,
) -> Option<Vec<[f64; 3]>> {
    let rate = project.settings.frame_rate;
    let precision = if accurate {
        bv_track::Precision::Accurate
    } else {
        bv_track::Precision::Fast
    };
    let (pw, ph) = (
        project.settings.width.max(1),
        project.settings.height.max(1),
    );
    let w = precision.width().max(960).min(pw);
    let h = ((w as u64 * ph as u64) / pw as u64).max(2) as u32;
    let mut painter = Painter::new(accel, Purpose::File);
    let mut pool = VideoPool::new();
    let mut draw = |n: i64| {
        let img = painter.linear(
            project,
            rate.frame_start(n),
            (w, h),
            &mut pool,
            [0.0, 0.0, 0.0, 1.0],
        );
        bv_track::Gray::from_rgba(img.width as usize, img.height as usize, &img.data)
    };
    let first = rate.frame_at(start);
    let last = rate.frame_at(start + duration - Tick(1)).max(first);
    let total = (last - first).max(0) as u64;
    if let Ok(mut s) = out.lock() {
        s.0.total = total;
    }
    // The first eye's picture, and the latitudes kept (the poles a sixth of
    // its height each).
    let (fw, fh) = (w as f64, h as f64);
    let (ew, eh) = match layout {
        1 => (fw, fh / 2.0),
        2 => (fw / 2.0, fh),
        _ => (fw, fh),
    };
    let top = if exclude == 1 || exclude == 3 {
        eh / 6.0
    } else {
        2.0
    };
    let bottom = if exclude == 2 || exclude == 3 {
        eh * 5.0 / 6.0
    } else {
        eh - 2.0
    };
    let area = [
        [2.0, top as f32],
        [(ew - 2.0) as f32, top as f32],
        [(ew - 2.0) as f32, bottom as f32],
        [2.0, bottom as f32],
    ];
    let agree = 1.5 * std::f64::consts::TAU / ew;
    let to_dir = |p: [f32; 2]| direction((p[0] as f64 + 0.5) / ew, (p[1] as f64 + 0.5) / eh);
    let mut turns = Vec::with_capacity(total as usize);
    let mut prev = draw(first);
    for n in first + 1..=last {
        if out.lock().map_or(true, |s| s.1) {
            return None;
        }
        let next = draw(n);
        let turn = match bv_track::follow_features(&prev, &next, &area, false, precision) {
            Ok(pairs) => {
                // Pairs across the picture's seam (a jump of most of its
                // width) are left out.
                let dirs: Vec<([f64; 3], [f64; 3])> = pairs
                    .iter()
                    .filter(|(p, q)| ((p[0] - q[0]).abs() as f64) < ew / 4.0)
                    .map(|(p, q)| (to_dir(*p), to_dir(*q)))
                    .collect();
                robust_turn(&dirs, agree)
            }
            Err(_) => [0.0; 3],
        };
        turns.push(turn);
        prev = next;
        if let Ok(mut s) = out.lock() {
            s.0.done = (n - first) as u64;
        }
    }
    Some(turns)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A turn about up is found from directions it turned.
    /// A feature's track goes on while the next frame has a feature where
    /// it ended.
    #[test]
    fn feature_tracks_follow_on_through_the_frames() {
        let t = FeatureTracks {
            size: (100, 100),
            frames: vec![
                vec![([10.0, 10.0], [12.0, 10.0]), ([50.0, 50.0], [50.0, 52.0])],
                vec![([12.5, 10.0], [14.0, 10.0])],
                vec![([14.0, 11.0], [16.0, 11.0])],
            ],
        };
        let at = t.tracks_at(0, 10);
        assert_eq!(at.len(), 2);
        assert_eq!(at[0].len(), 4, "{:?}", at[0]);
        assert_eq!(at[1].len(), 2);
        assert_eq!(t.tracks_at(0, 2)[0].len(), 3);
        assert!(t.tracks_at(5, 10).is_empty());
    }

    /// Current Clip's light: every frame's pixels counted at the values
    /// the effects get (an 8-bit project's: the encoded ones); set for
    /// each frame, each frame's at its time.
    #[test]
    fn a_clips_light_is_counted_as_its_effects_get_it() {
        use bv_timeline::MediaInfo;
        use bv_timeline::{Generator, ProjectSettings};
        let mut p = Project::new(ProjectSettings {
            width: 64,
            height: 36,
            ..ProjectSettings::default()
        });
        let info = MediaInfo::generated(
            Generator::SolidColor {
                rgba: [0.5, 0.5, 0.5, 1.0],
            },
            &p.settings.clone(),
        );
        let m = p.add_media(info);
        let v = p.tracks[0].id;
        let c = p.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
        let rate = p.settings.frame_rate;
        if let Some(clip) = p.clip_mut(c) {
            clip.duration = rate.frame_start(3);
        }
        let (picture, start, duration) = picture_of(&p, FxOwner::Event(c)).expect("a picture");
        let out = Mutex::new((AnalysisProgress::default(), false));
        let accel = crate::accel::Accel::new(Arc::new(|| {}));
        let lights = light(&picture, (start, duration), false, accel.clone(), &out);
        let [(_, clip)] = &lights.expect("measured")[..] else {
            panic!("one light");
        };
        let total: u64 = clip.counts.iter().sum();
        assert_eq!(total, 3 * 64 * 36);
        let bins = bv_effects::brightness_contrast::BINS;
        let middle = (0.5 * (bins - 1) as f32).round() as usize;
        assert_eq!(
            clip.counts[middle],
            total,
            "{:?}",
            &clip.counts[middle - 2..middle + 3]
        );
        assert_eq!(out.lock().unwrap().0.done, 3);
        // Frame by frame: a light a frame, at 0, 1 and 2.
        let lights = light(&picture, (start, duration), true, accel, &out).expect("measured");
        let times: Vec<f64> = lights.iter().map(|(t, _)| *t).collect();
        assert_eq!(times, [0.0, 1.0, 2.0]);
        for (_, l) in &lights {
            assert_eq!(l.counts[middle], 64 * 36);
        }
    }

    #[test]
    fn a_turn_is_found_from_directions() {
        use bv_effects::stabilization_360::rotate;
        let w = [0.002, 0.03, -0.01];
        let mut pairs: Vec<([f64; 3], [f64; 3])> = (0..40)
            .map(|i| {
                let p = direction(i as f64 * 0.023 % 1.0, 0.3 + (i % 7) as f64 * 0.05);
                (p, rotate(w, p))
            })
            .collect();
        // Two that went astray.
        pairs[3].1 = direction(0.9, 0.9);
        pairs[17].1 = direction(0.1, 0.2);
        let got = robust_turn(&pairs, 0.002);
        for k in 0..3 {
            assert!((got[k] - w[k]).abs() < 2e-4, "{got:?}");
        }
    }
}
