//! Motion tracking's work (VEGAS 2026's Motion Tracking window): the
//! event's own picture drawn frame after frame — the event alone, framed
//! by its Pan/Crop, without its effects, masks or its track's — and a
//! region followed through it by `bv_track`, in the background, the
//! regions found handed out as they come.

use std::sync::{Arc, Mutex};

use bv_core::Tick;
use bv_media::VideoPool;
use bv_timeline::{ClipId, Project};

use crate::accel::Painter;
use crate::plugins::Purpose;

/// Which way tracking goes from where it starts (VEGAS's track buttons).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackDirection {
    Backward,
    Forward,
    /// Forward to the event's end, then backward to its start.
    Both,
}

/// What to track.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackRequest {
    pub clip: ClipId,
    /// From the event's start: the frame tracking starts on.
    pub from: Tick,
    pub direction: TrackDirection,
    /// One frame only (Track one frame forward or backward).
    pub one_frame: bool,
    /// The region there: its corners, 0–1 of the frame from its top left.
    pub quad: bv_track::Quad,
    /// Track outside Region.
    pub outside: bool,
    pub mode: bv_track::Mode,
    pub precision: bv_track::Precision,
}

/// How tracking goes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrackingProgress {
    /// The regions found since last asked: when (from the event's start)
    /// and their corners (0–1 of the frame).
    pub keys: Vec<(Tick, bv_track::Quad)>,
    /// The last frame's motion vectors (0–1 of the frame).
    pub vectors: Vec<(bv_track::Point, bv_track::Point)>,
    /// How much of it is done (0–1).
    pub done: f32,
    pub finished: bool,
    /// Why it stopped before the event's edge.
    pub lost: Option<bv_track::Lost>,
}

pub(crate) struct Tracking {
    shared: Arc<Mutex<(TrackingProgress, bool)>>,
}

/// The event's own picture: `project` with only `clip`, without effects,
/// masks or its track's motion.
pub(crate) fn picture_of(project: &Project, clip: ClipId) -> Option<Project> {
    let mut p = crate::alone(project, clip);
    p.video_output_fx.clear();
    let track = p.tracks.iter_mut().find(|t| !t.clips.is_empty())?;
    track.effects.clear();
    track.motion = None;
    track.parent_motion = None;
    track.composite_depth = 0;
    for c in &mut track.clips {
        c.effects.clear();
        c.mask = None;
    }
    p.tracks.retain(|t| !t.clips.is_empty());
    Some(p)
}

impl crate::Engine {
    /// Starts tracking as `req` says (a tracking going on stops first).
    /// False when the event is not there.
    pub fn start_tracking(&mut self, req: TrackRequest) -> bool {
        self.cancel_tracking();
        let Some(start) = self.project.clip(req.clip).map(|c| c.start) else {
            return false;
        };
        let Some(duration) = self.project.clip(req.clip).map(|c| c.duration) else {
            return false;
        };
        let Some(project) = picture_of(&self.project, req.clip) else {
            return false;
        };
        let shared = Arc::new(Mutex::new((TrackingProgress::default(), false)));
        let accel = self.accel.clone();
        let out = shared.clone();
        let repaint = self.repaint.clone();
        std::thread::Builder::new()
            .name("motion-tracking".into())
            .spawn(move || {
                run(&project, start, duration, req, accel, &out);
                if let Ok(mut s) = out.lock() {
                    s.0.finished = true;
                    s.0.done = 1.0;
                }
                repaint();
            })
            .ok();
        self.tracking = Some(Tracking { shared });
        true
    }

    /// How tracking goes: the regions found since last asked (`None`
    /// while nothing tracks).
    pub fn tracking(&mut self) -> Option<TrackingProgress> {
        let t = self.tracking.as_ref()?;
        let mut s = t.shared.lock().ok()?;
        let out = TrackingProgress {
            keys: std::mem::take(&mut s.0.keys),
            ..s.0.clone()
        };
        drop(s);
        if out.finished {
            self.tracking = None;
        }
        Some(out)
    }

    /// Stops tracking (what was found stays found).
    pub fn cancel_tracking(&mut self) {
        if let Some(t) = self.tracking.take()
            && let Ok(mut s) = t.shared.lock()
        {
            s.1 = true;
        }
    }
}

/// Tracks on this thread, handing out what it finds.
fn run(
    project: &Project,
    start: Tick,
    duration: Tick,
    req: TrackRequest,
    accel: Arc<crate::accel::Accel>,
    out: &Mutex<(TrackingProgress, bool)>,
) {
    let rate = project.settings.frame_rate;
    let (pw, ph) = (
        project.settings.width.max(1),
        project.settings.height.max(1),
    );
    let w = req.precision.width().min(pw);
    let h = ((w as u64 * ph as u64) / pw as u64).max(2) as u32;
    let mut painter = Painter::new(accel, Purpose::File);
    let mut pool = VideoPool::new();
    let mut draw = |local: Tick| {
        let img = painter.linear(
            project,
            start + local,
            (w, h),
            &mut pool,
            [0.0, 0.0, 0.0, 1.0],
        );
        bv_track::Gray::from_rgba(img.width as usize, img.height as usize, &img.data)
    };
    // The event's frames, by number from its start.
    let first = rate.frame_at(start);
    let last = rate.frame_at(start + duration - Tick(1)).max(first);
    let at = |n: i64| rate.frame_start(n) - start;
    let from = rate.frame_at(start + req.from).clamp(first, last);
    let legs: Vec<i64> = match req.direction {
        TrackDirection::Forward => vec![1],
        TrackDirection::Backward => vec![-1],
        TrackDirection::Both => vec![1, -1],
    };
    let total: i64 = legs
        .iter()
        .map(|&d| {
            if req.one_frame {
                1
            } else if d > 0 {
                last - from
            } else {
                from - first
            }
        })
        .sum::<i64>()
        .max(1);
    let px = |q: bv_track::Quad| q.map(|p| [p[0] * w as f32, p[1] * h as f32]);
    let unit = |q: bv_track::Quad| q.map(|p| [p[0] / w as f32, p[1] / h as f32]);
    let mut done = 0i64;
    for dir in legs {
        let mut quad = px(req.quad);
        let mut frame = from;
        let mut prev = draw(at(frame));
        loop {
            let next_frame = frame + dir;
            if next_frame < first || next_frame > last {
                break;
            }
            if out.lock().map_or(true, |s| s.1) {
                return;
            }
            let next = draw(at(next_frame));
            match bv_track::track(&prev, &next, quad, req.outside, req.mode, req.precision) {
                Ok(step) => {
                    quad = step.quad;
                    done += 1;
                    if let Ok(mut s) = out.lock() {
                        s.0.keys.push((at(next_frame), unit(quad)));
                        s.0.vectors = step
                            .vectors
                            .iter()
                            .map(|(a, b)| {
                                (
                                    [a[0] / w as f32, a[1] / h as f32],
                                    [b[0] / w as f32, b[1] / h as f32],
                                )
                            })
                            .collect();
                        s.0.done = (done as f32 / total as f32).min(1.0);
                    }
                }
                Err(lost) => {
                    if let Ok(mut s) = out.lock() {
                        s.0.lost = Some(lost);
                    }
                    return;
                }
            }
            prev = next;
            frame = next_frame;
            if req.one_frame {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_timeline::{Generator, MediaInfo, ProjectSettings, TrackKind};

    /// A moving checkerboard... a generated picture panned across: the
    /// region follows it frame by frame.
    #[test]
    fn a_region_follows_the_picture() {
        let mut p = Project::new(ProjectSettings::default());
        let v = p
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video)
            .unwrap()
            .id;
        let info = MediaInfo::generated(Generator::ColorBars, &p.settings);
        let m = p.add_media(info);
        let c = p.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
        let mut engine = crate::Engine::new(Arc::new(|| {}));
        engine.set_project(&p);
        let req = TrackRequest {
            clip: c,
            from: Tick::ZERO,
            direction: TrackDirection::Forward,
            one_frame: true,
            quad: [[0.3, 0.3], [0.6, 0.3], [0.6, 0.6], [0.3, 0.6]],
            outside: false,
            mode: bv_track::Mode::Location,
            precision: bv_track::Precision::Fast,
        };
        assert!(engine.start_tracking(req));
        let mut got = None;
        for _ in 0..600 {
            if let Some(t) = engine.tracking()
                && t.finished
            {
                got = Some(t);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        // Color bars stand still: the region stays (or is lost for want of
        // detail inside it, never moved).
        let t = got.expect("tracking ends");
        assert!(t.finished);
    }
}
