//! Smart rendering (VEGAS's "no-recompress rendering", Options ›
//! Preferences › File I/O): a frame the project leaves as its media had it
//! — one event and nothing done to it, its media of the very format,
//! size, rate and fields being rendered — goes into the file as the
//! media's own compressed frame, not decoded and encoded again. BananaV
//! does it for ProRes, whose frames each stand alone.

use std::collections::HashMap;

use bv_core::{FieldOrder, FrameRate, Tick};
use bv_timeline::scene::{TrackContent, frame_scene};
use bv_timeline::{CompositeMode, MediaSource, Project};

/// What a file being rendered takes as compressed frames, as they are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Passable {
    /// The compressed format (a ProRes profile's FourCC).
    pub fourcc: [u8; 4],
    pub width: u32,
    pub height: u32,
    pub rate: FrameRate,
    pub field_order: FieldOrder,
    pub alpha: bool,
}

/// The media files' compressed frames, read as the render goes.
pub(crate) struct SmartRender {
    pass: Passable,
    /// Each file's frames (`None`: of another format, or unreadable).
    files: HashMap<String, Option<Packets>>,
}

impl SmartRender {
    pub(crate) fn new(pass: Passable) -> SmartRender {
        SmartRender {
            pass,
            files: HashMap::new(),
        }
    }

    /// The compressed frame that frame `t` can be, when the project
    /// leaves it as its media had it.
    pub(crate) fn frame(&mut self, project: &Project, t: Tick) -> Option<Vec<u8>> {
        let (path, at) = unedited(project, t, &self.pass)?;
        let pass = self.pass;
        let file = self
            .files
            .entry(path)
            .or_insert_with_key(|path| Packets::open(path, &pass));
        let data = file.as_mut()?.at(at)?;
        let f = bv_prores::frame_info(&data)?;
        let interlace = match pass.field_order {
            FieldOrder::Progressive => 0,
            FieldOrder::UpperFirst => 1,
            FieldOrder::LowerFirst => 2,
        };
        let fits = (f.width, f.height) == (pass.width, pass.height)
            && f.alpha == pass.alpha
            && f.interlace == interlace;
        fits.then_some(data)
    }
}

/// The media file and the instant of it that frame `t` shows unchanged —
/// one event; no effects, motion, fades, opacity, Pan/Crop or masks on its
/// way; its media of the size, rate and fields being rendered, shown at
/// one of its frames — or `None`.
pub(crate) fn unedited(project: &Project, t: Tick, pass: &Passable) -> Option<(String, Tick)> {
    let (w, h) = project.settings.output_size();
    if (w, h) != (pass.width, pass.height)
        || project.settings.frame_rate != pass.rate
        || project.motion_blur_at(t).is_some()
    {
        return None;
    }
    let scene = frame_scene(project, t, w, h);
    if scene.muted
        || !scene.output_fx.is_empty()
        || scene.output_fade.is_some()
        || scene.color.is_some()
    {
        return None;
    }
    let [track] = scene.tracks.as_slice() else {
        return None;
    };
    let plain_track = track.mode == CompositeMode::default()
        && track.depth == 0
        && !track.adjustment
        && track.level >= 1.0
        && track.fade.is_none()
        && track.effects.is_empty()
        && track.motion.is_none()
        && track.parent_motion.is_none();
    if !plain_track {
        return None;
    }
    let TrackContent::Event(layer) = &track.content else {
        return None;
    };
    let m = layer.source.as_ref()?;
    let plain_event = layer.level >= 1.0
        && layer.fade.is_none()
        && m.effects.is_empty()
        && m.transform.is_identity()
        && m.blend.is_none()
        && m.deinterlace.is_none()
        && m.clip.mask.is_none()
        && (m.par - 1.0).abs() < 1e-6;
    let v = m.info.video?;
    let same_video = (v.width, v.height) == (w, h)
        && v.frame_rate == pass.rate
        && v.field_order == pass.field_order
        && m.info.color_space.is_none()
        && v.reading == Default::default()
        && m.info.layer.is_none()
        && m.info.sequence.is_none();
    if !plain_event || !same_video {
        return None;
    }
    let MediaSource::File { path } = &m.info.source else {
        return None;
    };
    // On one of the media's frames (a thousandth of a frame aside at most).
    let k = (m.time.as_seconds_f64() * pass.rate.as_f64()).round() as i64;
    let start = pass.rate.frame_start(k);
    let slack = pass.rate.frame_duration().0 / 1000;
    ((m.time - start).0.abs() <= slack && k >= 0).then(|| (path.clone(), start))
}

/// A media file's compressed frames, read in order (a seek when the render
/// jumps).
struct Packets {
    reader: bv_mf::PacketReader,
    rate: FrameRate,
    /// Where the reader stands: the next packet's start (100 ns).
    next: Option<i64>,
}

impl Packets {
    /// The file's frames, when they are of the format `pass` takes.
    fn open(path: &str, pass: &Passable) -> Option<Packets> {
        let reader = bv_mf::PacketReader::video(path).ok()?;
        let info = reader.info();
        let ours = bv_mf::codec::normalized(info.codec) == bv_mf::codec::fourcc(&pass.fourcc);
        ours.then_some(Packets {
            reader,
            rate: info.frame_rate,
            next: None,
        })
    }

    /// The frame starting at `at`.
    fn at(&mut self, at: Tick) -> Option<Vec<u8>> {
        let target = at.to_timescale(10_000_000);
        let half = (self.rate.frame_duration().to_timescale(10_000_000) / 2).max(1);
        let ahead = self
            .next
            .is_some_and(|n| target + half >= n && target < n + 2 * 10_000_000);
        if !ahead {
            self.reader.seek(at).ok()?;
            self.next = None;
        }
        while let Some(p) = self.reader.next_packet().ok()? {
            self.next = Some(p.time + p.duration.max(1));
            if (p.time - target).abs() < half {
                return Some(p.data);
            }
            if p.time > target {
                // Not there: start over next time.
                self.next = None;
                return None;
            }
        }
        self.next = None;
        None
    }
}
