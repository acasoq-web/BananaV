//! What a frame of the project shows at an instant: for each video track,
//! bottom first, the event (or the two events of a crossfade) visible, where
//! its picture comes from, how it is framed, faded and mixed, and the
//! effects it goes through.
//!
//! The compositors draw frames from this description — the processor's
//! (`bv-render`, the reference) and the graphics card's (`bv-gpu`) — so both
//! show the same thing.

use bv_core::{FrameRate, ImageRgbaF32, Tick};

use crate::fx::{Effect, FadeCurve, PluginFx, Transition};
use crate::model::{Clip, ClipId, MediaId, MediaInfo, Project, RenderQuality, Track, TrackId};

/// What an effect belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FxOwner {
    Media(MediaId),
    Event(ClipId),
    Track(TrackId),
    /// Tools › Video › Video Output FX.
    Output,
    /// The transition an event comes in with.
    Transition(ClipId),
    /// A track's compositor: its Compositing Mode's (`false`) or, a
    /// compositing parent's, its Parent Composite Mode's (`true`).
    Compositor(TrackId, bool),
}

/// An effect a frame goes through: what it belongs to, and its place in
/// its owner's chain.
#[derive(Clone, Copy, Debug)]
pub struct SceneFx<'a> {
    pub fx: &'a Effect,
    pub owner: FxOwner,
    pub index: usize,
}

/// One plug-in effect (OpenFX) to draw.
#[derive(Clone, Copy, Debug)]
pub struct PluginCall<'a> {
    pub fx: &'a PluginFx,
    pub owner: FxOwner,
    /// Its place in its owner's chain.
    pub index: usize,
    /// Its owner's time: from the event's start, the media's or the
    /// project's (track and output effects).
    pub time: Tick,
    /// Its owner's length.
    pub duration: Tick,
    /// The frame's time as the project's ruler shows it (its offset added):
    /// what a timecode shows.
    pub timeline: Tick,
    pub rate: FrameRate,
    /// The picture's size at full resolution (what the picture the effect
    /// gets is a reduction of).
    pub full_size: (u32, u32),
    /// Where its input comes from, for plug-ins that read it at other
    /// times (a media's or event's effects; `None` for the others).
    pub source: Option<FxSource<'a>>,
}

/// How a media's or event's effect's input looks at other times, for
/// plug-ins that ask for it there (OFX temporal clip access): the event
/// and its media, the effects before it in the chain, and how pictures
/// enter the chain.
#[derive(Clone, Copy, Debug)]
pub struct FxSource<'a> {
    pub info: &'a MediaInfo,
    pub clip: &'a Clip,
    /// The effect is the media's: its time is the media's (else the
    /// event's, from its start).
    pub media_time: bool,
    /// The chain's effects before it.
    pub before: &'a [SceneFx<'a>],
    /// What the chain's effect sizes are scaled by.
    pub fx_scale: f32,
    /// Pictures enter the chain in video gamma (compositing on the
    /// encoded values).
    pub video_gamma: bool,
    /// Under ACES, the matrix pictures enter its working space through
    /// ([`crate::ColorPipeline::input`]), and that space (for a media in a
    /// color space of its own: [`crate::color::media_input`]).
    pub enter: Option<[[f32; 3]; 3]>,
    pub working: Option<crate::AcesSpace>,
    /// What each effect leaves is clipped to 0–1 ([`FrameScene::clips`]).
    pub clips: bool,
    /// For an event's effect, how the media is framed on the way: the
    /// event's effects work on the framed picture.
    pub framing: Option<Framing>,
}

/// How an event's effects get their picture (VEGAS's default: Pan/Crop
/// first in the event's chain): the media's own effects (the chain's
/// first `framed_at`) work on its picture, `media_size` big, their sizes
/// scaled by `media_fx_scale`; then the event's Pan/Crop places it in the
/// frame as `place` says, at `quality`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Framing {
    pub framed_at: usize,
    pub media_size: (u32, u32),
    pub media_fx_scale: f32,
    pub place: crate::pancrop::PlaceIn,
    pub quality: RenderQuality,
}

/// Draws the effects of plug-ins, which the compositors cannot draw
/// themselves (the engine runs them).
pub trait PluginEffects: Sync {
    /// The effect applied to `input` (premultiplied, in the compositing
    /// space); `None` leaves the picture as it was.
    fn apply(
        &self,
        call: &PluginCall<'_>,
        input: std::sync::Arc<ImageRgbaF32>,
    ) -> Option<ImageRgbaF32>;

    /// Whether `call` (a track's or the video output's effect) takes the
    /// input of the chain it is in: its plug-in has an "Original" clip
    /// (Bézier Masking's Mask FX). An event's or a media's effect works it
    /// out from its `source`.
    fn takes_chain_input(&self, _call: &PluginCall<'_>) -> bool {
        false
    }

    /// [`Self::apply`], given the picture its chain started from too.
    fn apply_in_chain(
        &self,
        call: &PluginCall<'_>,
        input: std::sync::Arc<ImageRgbaF32>,
        _chain_input: std::sync::Arc<ImageRgbaF32>,
    ) -> Option<ImageRgbaF32> {
        self.apply(call, input)
    }

    /// A compositor's picture (a Custom compositing mode): `source`, what
    /// is below (the same size), drawn as `map`, the track's picture,
    /// guides it; `None` when the plug-in cannot draw it (Source Alpha
    /// stands in).
    fn composite(
        &self,
        _call: &PluginCall<'_>,
        _source: std::sync::Arc<ImageRgbaF32>,
        _map: std::sync::Arc<ImageRgbaF32>,
    ) -> Option<ImageRgbaF32> {
        None
    }

    /// A transition from `from` to `to` (the same size), `progress` (0–1)
    /// of the way; `None` when the plug-in cannot draw it (BananaV's own
    /// transition stands in).
    fn transition(
        &self,
        call: &PluginCall<'_>,
        from: std::sync::Arc<ImageRgbaF32>,
        to: std::sync::Arc<ImageRgbaF32>,
        progress: f32,
    ) -> Option<ImageRgbaF32>;

    /// `call`'s parameters' values at its time, as its plug-in sees them:
    /// the graphics card draws BananaV's own effects itself with them
    /// (`None`: they are not known, the plug-in draws).
    fn values_at(&self, _call: &PluginCall<'_>) -> Option<Vec<(String, crate::PluginValue)>> {
        None
    }

    /// Whether the interface wants the picture `call` gets (an eyedropper
    /// or a Calculate reads it) when the card draws the effect.
    fn watched(&self, _call: &PluginCall<'_>) -> bool {
        false
    }

    /// The picture `call` got, for the interface (see [`Self::watched`]).
    fn note_input(&self, _call: &PluginCall<'_>, _input: std::sync::Arc<ImageRgbaF32>) {}

    /// `call`'s parameter `name`'s numbers at `time` (frames, as its
    /// plug-in counts them): BananaV's own effects that read a parameter at
    /// other frames (Slow Motion's playback rate, Flicker Control's motion
    /// data) read it so when the graphics card draws them (`None`: not
    /// known).
    fn numbers_at(&self, _call: &PluginCall<'_>, _name: &str, _time: f64) -> Option<Vec<f64>> {
        None
    }

    /// `call`'s input at `time` (frames, as its plug-in counts them), `size`
    /// big, as its plug-in would get it there: BananaV's own effects that
    /// read other frames take them so when the graphics card draws them
    /// (`None`: not known).
    fn input_at(
        &self,
        _call: &PluginCall<'_>,
        _time: f64,
        _size: (u32, u32),
    ) -> Option<std::sync::Arc<ImageRgbaF32>> {
        None
    }

    /// Whether `call`'s plug-in draws on pictures in the graphics card's
    /// memory as CUDA sees it ([`CardPicture`]s): a card CUDA shares then
    /// hands it its pictures where they are, instead of bringing them to
    /// the processor and back.
    fn on_card(&self, _call: &PluginCall<'_>) -> bool {
        false
    }

    /// [`Self::apply`] on the card: `call` drawn from `input` into
    /// `output`, the same size; `false` when it could not (the processor
    /// then draws it).
    fn apply_on_card(
        &self,
        _call: &PluginCall<'_>,
        _input: CardPicture,
        _output: CardPicture,
    ) -> bool {
        false
    }

    /// [`Self::transition`] on the card, into `output`; `false` when it
    /// could not.
    fn transition_on_card(
        &self,
        _call: &PluginCall<'_>,
        _from: CardPicture,
        _to: CardPicture,
        _output: CardPicture,
        _progress: f32,
    ) -> bool {
        false
    }
}

/// A picture in the graphics card's memory as CUDA sees it, for plug-ins
/// that render with CUDA (OpenFX 1.5's CUDA render): `width` × `height`
/// pixels of four floats, premultiplied, in rows of `row_bytes` from the
/// bottom one up (OpenFX's order), at `address` in CUDA device `device`'s
/// memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CardPicture {
    pub device: i32,
    pub address: u64,
    pub width: u32,
    pub height: u32,
    pub row_bytes: u32,
}

/// The plug-in effects of a frame.
#[derive(Clone, Copy)]
pub struct Plugins<'a>(pub &'a dyn PluginEffects);

impl std::fmt::Debug for Plugins<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Plugins")
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// The largest rectangle of `src`'s shape that fits in `dst`, centered,
/// with edges on whole pixels.
pub fn fit_rect(src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Rect {
    fit_rect_par(src_w, src_h, 1.0, dst_w, dst_h)
}

/// [`fit_rect`] for a picture whose pixels are `par` times as wide as the
/// frame's (the media's pixel aspect over the project's).
pub fn fit_rect_par(src_w: u32, src_h: u32, par: f32, dst_w: u32, dst_h: u32) -> Rect {
    let src_w = src_w as f32 * par.max(1e-3);
    let scale = (dst_w as f32 / src_w).min(dst_h as f32 / src_h as f32);
    let w = (src_w * scale).round().clamp(1.0, dst_w as f32);
    let h = (src_h as f32 * scale).round().clamp(1.0, dst_h as f32);
    Rect {
        x: ((dst_w as f32 - w) / 2.0).floor(),
        y: ((dst_h as f32 - h) / 2.0).floor(),
        w,
        h,
    }
}

/// "Adjust source media to better match project or render settings": how
/// much a `src_w` × `src_h` picture (pixels `par` times as wide as the
/// frame's) is enlarged so that it fills a `dst_w` × `dst_h` frame — when
/// one of them is a standard definition frame (720 or 704 pixels by 480,
/// 486 or 576 lines) and their shapes are within 3 % of each other: DV's
/// blanking beside HD, 486 lines in 480. Otherwise 1: other shapes keep
/// their bars, and 480 lines in 486 are padded as they are.
pub fn adjust_factor(src_w: u32, src_h: u32, par: f32, dst_w: u32, dst_h: u32) -> f32 {
    let sd = |w: u32, h: u32| matches!(w, 704 | 720) && matches!(h, 480 | 486 | 576);
    if src_w == 0
        || src_h == 0
        || dst_w == 0
        || dst_h == 0
        || !(sd(src_w, src_h) || sd(dst_w, dst_h))
    {
        return 1.0;
    }
    let ratio = (src_w as f32 * par.max(1e-3) / src_h as f32) / (dst_w as f32 / dst_h as f32);
    let off = (ratio - 1.0).abs();
    if off < 1e-4 || off > 0.03 {
        return 1.0;
    }
    ratio.max(1.0 / ratio)
}

/// A frame to draw: `width` × `height` (the project's frame, or smaller
/// for previews), before the output rotation.
#[derive(Clone, Debug)]
pub struct FrameScene<'a> {
    pub width: u32,
    pub height: u32,
    /// The project's frame size (effect sizes in pixels refer to it).
    pub project_size: (u32, u32),
    /// Rendered height ÷ the project's: scales effect sizes in pixels.
    pub scale: f32,
    /// Video is muted (View › Mute All Video): only the background shows.
    pub muted: bool,
    /// Composite on the encoded values (8-bit formats, 2.222 gamma)
    /// instead of on light.
    pub video_gamma: bool,
    /// The pixel format holds only 0–1 (8-bit formats): what each effect
    /// leaves and the finished frame are clipped, as VEGAS's 8-bit
    /// pictures hold them (its effects in 32-bit projects keep what passes
    /// white or black).
    pub clips: bool,
    /// Bottom track first.
    pub tracks: Vec<TrackScene<'a>>,
    /// Tools › Video › Video Output FX: applied to the finished frame
    /// (none while the video bus bypasses them).
    pub output_fx: &'a [Effect],
    /// The video bus's Fade to Color: the finished frame toward this color
    /// (in the compositing space) this far (0–1), after the output's
    /// effects.
    pub output_fade: Option<([f32; 3], f32)>,
    /// The frame's time in the project, its length and frame rate (for
    /// plug-in effects).
    pub time: Tick,
    pub duration: Tick,
    pub rate: FrameRate,
    /// The ruler's offset: the frame's time on the ruler is `time` plus it.
    pub ruler_offset: Tick,
    /// Who draws plug-in effects: without it they leave pictures as they
    /// were.
    pub plugins: Option<Plugins<'a>>,
    /// ACES (File › Properties › Video): media and colors enter its
    /// working space, the finished frame leaves through its view.
    pub color: Option<std::sync::Arc<crate::ColorPipeline>>,
}

/// A track with something to show (or, in a compositing group, its place:
/// [`TrackContent::Nothing`]).
#[derive(Clone, Debug)]
pub struct TrackScene<'a> {
    pub track: TrackId,
    /// How its picture combines with what is below it (a parent's, with
    /// its children), and a parent's group with what is below the group.
    pub mode: crate::model::CompositeMode,
    pub parent_mode: crate::model::CompositeMode,
    /// The compositor plug-ins of its Custom modes (`None`: they composite
    /// as Source Alpha).
    pub compositor: Option<&'a PluginFx>,
    pub parent_compositor: Option<&'a PluginFx>,
    /// How far in it is among compositing children (0: not a child).
    pub depth: u8,
    /// An adjustment track: its events (`content`) go over what is below
    /// it (its children's group, when it has one), then its effects work on
    /// all of that, `level` their strength.
    pub adjustment: bool,
    /// The track's opacity (0–1), its Composite Level envelope included.
    pub level: f32,
    /// Its Fade to Color envelope: the color (in the compositing space:
    /// light — ACES's working space under ACES — or video gamma) and how
    /// far toward it (0–1).
    pub fade: Option<([f32; 3], f32)>,
    pub effects: &'a [Effect],
    /// The first of `effects` that works after compositing, on its
    /// group's picture once the track composited into it (VEGAS's
    /// post-compositing track effects); `effects.len()` when none does —
    /// always outside compositing groups, whose signal flow has no such
    /// step.
    pub post_from: usize,
    pub content: TrackContent<'a>,
    /// Its Track Motion at this frame: where its picture goes, its 2D
    /// shadow and glow (`None`: the picture as it is).
    pub motion: Option<crate::motion::MotionAt>,
    /// A compositing parent's Parent Motion: its group moved.
    pub parent_motion: Option<crate::motion::MotionAt>,
    /// 3D Source Alpha (its Compositing Mode): its box in space at this
    /// frame (`crate::space`), where its picture goes instead of `motion`.
    pub space: Option<crate::space::Box3d>,
    /// A compositing parent's 3D Parent Motion: its group's box in space.
    pub parent_space: Option<crate::space::Box3d>,
}

// A transition holds two events' layers: built once a frame for a few
// tracks, boxing them would buy nothing.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum TrackContent<'a> {
    /// A compositing parent or child with nothing to show now (muted,
    /// hidden or between events): its group still composites through it.
    Nothing,
    Event(EventLayer<'a>),
    /// The top event coming in with its transition: over the event before
    /// it (crossfade) or over nothing (`from` is `None`: a fade-in), at
    /// `progress` (0–1). `plugin` draws it when the event's transition is
    /// a plug-in's (`kind` stands in when it cannot).
    Transition {
        kind: Transition,
        plugin: Option<Box<PluginCall<'a>>>,
        from: Option<EventLayer<'a>>,
        to: EventLayer<'a>,
        progress: f32,
    },
}

/// An event's picture placed in the frame.
#[derive(Clone, Debug)]
pub struct EventLayer<'a> {
    pub clip: ClipId,
    /// `None`: nothing shows (the media is missing from the project, or the
    /// event runs past its end without Loop or Hold Last Frame).
    pub source: Option<MediaLayer<'a>>,
    /// The event's opacity (0–1).
    pub level: f32,
    /// The fade-out's factor, while the event fades out.
    pub fade: Option<f32>,
}

/// Where an event's picture comes from and how it is framed.
#[derive(Clone, Debug)]
pub struct MediaLayer<'a> {
    pub info: &'a MediaInfo,
    /// The event that shows it.
    pub clip: &'a Clip,
    /// The media's instant shown.
    pub time: Tick,
    /// The size the picture shows at: a reader may give a reduced picture
    /// as long as it covers this.
    pub min_size: (u32, u32),
    /// Where its Pan/Crop puts it on the frame drawn, and what that depends
    /// on besides the box.
    pub placement: crate::pancrop::Placement,
    pub place: crate::pancrop::PlaceIn,
    pub quality: RenderQuality,
    /// The media's effects, then the event's (enabled video ones).
    pub effects: Vec<SceneFx<'a>>,
    /// The time since the event's start, and its length (plug-in effects
    /// of the event count from there).
    pub local: Tick,
    pub length: Tick,
    /// How much wider its pixels are than the frame's (its pixel aspect
    /// over the project's): 1 for square pixels in a square project.
    pub par: f32,
    /// Frame Blend (the Resample mode): the media's next frame's instant
    /// and how much of it is mixed into the one shown.
    pub blend: Option<(Tick, f32)>,
    /// Optical Flow (the Resample mode): that much of the way to the next
    /// frame along what moves, instead of mixed (on the processor).
    pub flow: bool,
    /// How its fields become whole frames (the project's Deinterlace
    /// method) and their order, where they are not the project's; done at
    /// Good and Best `quality` only ([`MediaLayer::deinterlaced`]).
    pub deinterlace: Option<(crate::DeinterlaceMethod, bv_core::FieldOrder)>,
    /// The project's video levels pixel format: Y′CbCr comes in as studio
    /// RGB.
    pub video_levels: bool,
}

impl MediaLayer<'_> {
    /// How a picture of the media `height` lines tall is deinterlaced: its
    /// method and its field order, where the media's fields are not the
    /// project's — at Good and Best quality, as VEGAS's (at Draft and
    /// Preview nothing is), and on pictures as tall as the media (a
    /// reduced one has its fields averaged already).
    pub fn deinterlaced(
        &self,
        height: u32,
    ) -> Option<(crate::DeinterlaceMethod, bv_core::FieldOrder)> {
        let full = self.info.video.is_some_and(|v| v.height == height);
        let quality = matches!(self.quality, RenderQuality::Good | RenderQuality::Best);
        self.deinterlace.filter(|_| full && quality)
    }

    /// What the event's Pan/Crop masks keep of its media's picture, `w` ×
    /// `h` (the picture's size, whatever the reader gave), at the frame
    /// shown: white where they keep it, see-through where they cut it, for
    /// the renderers to frame as they frame the picture. `None` without
    /// masks that mask.
    pub fn mask_picture(&self, w: u32, h: u32) -> Option<std::sync::Arc<bv_core::ImageRgbaF32>> {
        use std::sync::{Arc, Mutex};
        /// The last pictures worked out: a mask that does not change (while
        /// playing, or while something else is edited) is worked out once.
        type Kept = Vec<(u64, Arc<bv_core::ImageRgbaF32>)>;
        static KEPT: Mutex<Kept> = Mutex::new(Vec::new());
        let mask = self.clip.mask.as_ref().filter(|k| k.active())?;
        let (mw, mh) = self.info.video.map_or((w, h), |v| (v.width, v.height));
        // The frame's time cut down to the box's unit, as the box's.
        let t = crate::pancrop::units_floor(self.clip.start + self.local)
            - crate::pancrop::units_near(self.clip.start);
        let key = mask.fingerprint(t, [w, h, mw, mh]);
        let found = KEPT.lock().ok().and_then(|kept| {
            kept.iter()
                .find(|(k, _)| *k == key)
                .map(|(_, p)| Arc::clone(p))
        });
        if found.is_some() {
            return found;
        }
        let scale = [w as f64 / mw.max(1) as f64, h as f64 / mh.max(1) as f64];
        let keep = mask.coverage(t, w as usize, h as usize, scale, self.place.media_aspect);
        let picture = Arc::new(bv_core::ImageRgbaF32 {
            width: w,
            height: h,
            data: keep.into_iter().map(|k| [k; 4]).collect(),
        });
        if let Ok(mut kept) = KEPT.lock() {
            kept.retain(|(k, _)| *k != key);
            kept.insert(0, (key, Arc::clone(&picture)));
            kept.truncate(2);
        }
        Some(picture)
    }

    /// How many of `effects` work on the media's own picture, before the
    /// event's Pan/Crop frames it: the media's, and the event's before
    /// its Pan/Crop in its chain (`Clip::pan_crop_at`). The others work on
    /// the framed picture, the frame's size — all of the event's by
    /// VEGAS's default, its Pan/Crop first.
    pub fn framed_from(&self) -> usize {
        let at = self.clip.pan_crop_at;
        self.effects
            .iter()
            .take_while(|e| match e.owner {
                FxOwner::Media(_) => true,
                FxOwner::Event(_) => e.index < at,
                _ => false,
            })
            .count()
    }
}

impl<'a> FrameScene<'a> {
    /// Scales every picture at `quality` instead of the project's (the
    /// Video Preview's own quality).
    pub fn set_quality(&mut self, quality: RenderQuality) {
        for t in &mut self.tracks {
            let layers = match &mut t.content {
                TrackContent::Nothing => [None, None],
                TrackContent::Event(e) => [Some(e), None],
                TrackContent::Transition { from, to, .. } => [from.as_mut(), Some(to)],
            };
            for e in layers.into_iter().flatten() {
                if let Some(m) = &mut e.source {
                    m.quality = quality;
                }
            }
            for m in [&mut t.motion, &mut t.parent_motion].into_iter().flatten() {
                m.quality = quality;
            }
        }
    }

    /// Every media picture the frame needs, in the order they are drawn.
    pub fn media_layers(&self) -> impl Iterator<Item = (ClipId, &MediaLayer<'a>)> {
        self.tracks.iter().flat_map(|t| {
            let (a, b) = match &t.content {
                TrackContent::Nothing => (None, None),
                TrackContent::Event(e) => (None, Some(e)),
                TrackContent::Transition { from, to, .. } => (from.as_ref(), Some(to)),
            };
            a.into_iter()
                .chain(b)
                .filter_map(|e| e.source.as_ref().map(|m| (e.clip, m)))
        })
    }
}

/// The video fade curve: "smooth" becomes a smoothstep (the sine, good for
/// constant power in audio, would make video show up too fast).
pub fn video_curve(c: FadeCurve, p: f32) -> f32 {
    match c {
        FadeCurve::Smooth => {
            let p = p.clamp(0.0, 1.0);
            p * p * (3.0 - 2.0 * p)
        }
        other => other.eval(p),
    }
}

fn ratio(num: Tick, den: Tick) -> f32 {
    if den <= Tick::ZERO {
        1.0
    } else {
        (num.0 as f64 / den.0 as f64) as f32
    }
}

/// The media frame an event shows at timeline time `t`: with Loop the
/// media repeats; without it, past the end the last frame stays (Hold Last
/// Frame) or nothing shows (`None`).
pub fn media_time(clip: &Clip, info: &MediaInfo, t: Tick) -> Option<Tick> {
    let len = info.duration;
    let s = clip.media_time(t, len);
    // Undersampled: the media's frames at that share of its rate, each
    // held until the next one.
    let s = match info.video {
        Some(v) if clip.undersample < 1.0 && !info.still => {
            let step =
                v.frame_rate.frame_duration().0 as f64 / f64::from(clip.undersample.max(0.01));
            Tick(((s.0 as f64 / step).floor() * step) as i64)
        }
        _ => s,
    };
    if info.still || clip.freeze.is_some() || len <= Tick::ZERO || s < len {
        return Some(s.max(Tick::ZERO));
    }
    let frame = info
        .video
        .map_or(Tick::ZERO, |v| v.frame_rate.frame_duration());
    clip.hold_last.then(|| (len - frame).max(Tick::ZERO))
}

/// A track's fade colors when it has none of its own: white toward the
/// top, black toward the bottom.
pub const DEFAULT_FADE_COLORS: [[u8; 4]; 2] = [[255, 255, 255, 255], [0, 0, 0, 255]];

/// A fade color (8 bits) in the frame's compositing space: video's values,
/// as VEGAS takes a picked color's — as they are in video gamma, else their
/// light —, under ACES in its working space.
fn fade_color(scene: &FrameScene<'_>, c: [u8; 4]) -> [f32; 3] {
    let to_space = |x: u8| {
        let v = x as f32 / 255.0;
        if scene.video_gamma {
            v
        } else {
            bv_color::transfer::bt709_to_linear(v)
        }
    };
    let c = [to_space(c[0]), to_space(c[1]), to_space(c[2])];
    scene.color.as_ref().map_or(c, |p| p.enter(c))
}

/// `true` while the track's mute envelope is off at `t`.
fn muted_by_envelope(track: &Track, t: Tick) -> bool {
    track
        .automation_at(crate::envelope::EnvelopeKind::Mute, t)
        .is_some_and(|v| v < 0.5)
}

/// What the project shows at `t`, drawn `width` × `height`.
pub fn frame_scene(project: &Project, t: Tick, width: u32, height: u32) -> FrameScene<'_> {
    scene_at(project, t, None, width, height)
}

/// A sub-frame of the video bus's motion blur at `t` for the frame at
/// `held`: the tracks that bypass motion blur show what they show at
/// `held`.
pub fn frame_scene_held(
    project: &Project,
    t: Tick,
    held: Tick,
    width: u32,
    height: u32,
) -> FrameScene<'_> {
    scene_at(project, t, Some(held), width, height)
}

fn scene_at(
    project: &Project,
    t: Tick,
    held: Option<Tick>,
    width: u32,
    height: u32,
) -> FrameScene<'_> {
    let s = &project.settings;
    let mut scene = FrameScene {
        width,
        height,
        project_size: (s.width, s.height),
        scale: height as f32 / s.height.max(1) as f32,
        // View › Mute All Video, or the video bus's mute.
        muted: project.mute_all_video || project.video_bus.muted_at(t),
        video_gamma: s.composites_in_video_gamma(),
        clips: !s.pixel_format.is_float(),
        tracks: Vec::new(),
        output_fx: if project.video_bus.bypass {
            &[]
        } else {
            &project.video_output_fx
        },
        output_fade: None,
        time: t,
        duration: project.duration(),
        rate: s.frame_rate,
        ruler_offset: project.ruler_offset,
        plugins: None,
        color: s.color_pipeline(),
    };
    if scene.muted {
        return scene;
    }
    // The video bus's Fade to Color, toward its top or bottom color.
    scene.output_fade = project
        .video_bus
        .envelope_at(crate::envelope::EnvelopeKind::FadeToColor, t)
        .filter(|v| v.abs() > 1e-4)
        .map(|v| {
            let [top, bottom] = project.video_bus.fade_colors.unwrap_or(DEFAULT_FADE_COLORS);
            let c = if v > 0.0 { top } else { bottom };
            (fade_color(&scene, c), v.abs().min(1.0))
        });
    // Soloed video tracks hide the others; the tracks of compositing
    // groups keep their places (with nothing) when they show nothing.
    let visible: Vec<TrackId> = project.visible_video_tracks().map(|t| t.id).collect();
    let video: Vec<&Track> = project
        .tracks
        .iter()
        .filter(|t| t.kind == crate::model::TrackKind::Video)
        .collect();
    // With 3D tracks, every 2D track keeps its place too: one between 3D
    // tracks is a barrier, even with nothing to show.
    let any_3d = video
        .iter()
        .any(|t| t.composite_mode.is_three_d() || t.parent_mode.is_three_d());
    for (i, track) in video.iter().enumerate().rev() {
        // Bypass Motion Blur: the frame's own time in every sub-frame.
        let t = match held {
            Some(h) if track.bypass_motion_blur => h,
            _ => t,
        };
        let grouped = track.composite_depth > 0
            || video
                .get(i + 1)
                .is_some_and(|n| n.composite_depth > track.composite_depth);
        // Its motions, their shadows' and glows' colors in the
        // compositing space (as the fade colors below).
        let motion_of = |m: &Option<crate::motion::TrackMotion>| {
            let mut at = m.as_ref()?.at(t, project.settings.render_quality)?;
            for look in [&mut at.shadow, &mut at.glow].into_iter().flatten() {
                let c = look.color;
                let c = [c[0], c[1], c[2]].map(|v| {
                    if scene.video_gamma {
                        v
                    } else {
                        bv_color::transfer::bt709_to_linear(v)
                    }
                });
                let c = scene.color.as_ref().map_or(c, |p| p.enter(c));
                look.color = [c[0], c[1], c[2], look.color[3]];
            }
            Some(at)
        };
        let keeps = grouped || (any_3d && !track.composite_mode.is_three_d());
        let motion = motion_of(&track.motion);
        let parent_motion = motion_of(&track.parent_motion);
        // 3D Source Alpha: the box in space instead of the 2D motion.
        let space_of = |m: &Option<crate::motion::TrackMotion>| {
            m.as_ref()
                .map_or_else(crate::space::Box3d::default, |m| m.space_at(t))
        };
        let space = track
            .composite_mode
            .is_three_d()
            .then(|| space_of(&track.motion));
        let parent_space = track
            .parent_mode
            .is_three_d()
            .then(|| space_of(&track.parent_motion));
        let motion = if space.is_some() { None } else { motion };
        let parent_motion = if parent_space.is_some() {
            None
        } else {
            parent_motion
        };
        let compositor = track.compositor.as_deref();
        let parent_compositor = track.parent_compositor.as_deref();
        let place = |level| TrackScene {
            track: track.id,
            mode: track.composite_mode,
            parent_mode: track.parent_mode,
            compositor,
            parent_compositor,
            depth: track.composite_depth,
            adjustment: false,
            level,
            fade: None,
            effects: &[],
            post_from: 0,
            content: TrackContent::Nothing,
            motion: None,
            parent_motion,
            space: None,
            parent_space,
        };
        // An adjustment track's level is its effects' strength: its events
        // show at any.
        let faded_out = |level: f32| level <= 0.0 && !track.adjustment;
        if !visible.contains(&track.id) || faded_out(track.level) || muted_by_envelope(track, t) {
            if keeps {
                scene.tracks.push(place(0.0));
            }
            continue;
        }
        // The envelopes as automation plays them (bypassed when it is off).
        let envelope = |kind| track.automation_at(kind, t);
        let composite = envelope(crate::envelope::EnvelopeKind::CompositeLevel).unwrap_or(1.0);
        let level = track.level.clamp(0.0, 1.0) * composite.clamp(0.0, 1.0);
        if faded_out(level) {
            if keeps {
                scene.tracks.push(place(0.0));
            }
            continue;
        }
        let fade = envelope(crate::envelope::EnvelopeKind::FadeToColor)
            .filter(|v| v.abs() > 1e-4)
            .map(|v| {
                let [top, bottom] = track.fade_colors.unwrap_or(DEFAULT_FADE_COLORS);
                let c = if v > 0.0 { top } else { bottom };
                (fade_color(&scene, c), v.abs().min(1.0))
            });
        // An adjustment track: its events over what is below it, then its
        // effects on all of that, always.
        if track.adjustment {
            let content = track_content(project, track, t, width, height);
            let works = level > 0.0 && track.effects.iter().any(|e| e.enabled);
            if works || grouped || content.is_some() {
                scene.tracks.push(TrackScene {
                    adjustment: true,
                    level,
                    fade,
                    effects: &track.effects,
                    post_from: track.effects.len(),
                    content: content.unwrap_or(TrackContent::Nothing),
                    ..place(level)
                });
            }
            continue;
        }
        // Its effects after the Composite button work on its group's
        // picture: only a compositing parent or child has one.
        let post_from = match track.composite_at {
            Some(k) if grouped => k.min(track.effects.len()),
            _ => track.effects.len(),
        };
        match track_content(project, track, t, width, height) {
            Some(content) => scene.tracks.push(TrackScene {
                track: track.id,
                mode: track.composite_mode,
                parent_mode: track.parent_mode,
                compositor,
                parent_compositor,
                depth: track.composite_depth,
                adjustment: false,
                level,
                fade,
                effects: &track.effects,
                post_from,
                content,
                motion,
                parent_motion,
                space,
                parent_space,
            }),
            None if keeps => scene.tracks.push(place(0.0)),
            None => {}
        }
    }
    scene
}

/// A compositing group of a frame's tracks (`TrackScene`s, bottom first):
/// a track's index, and the units its children make (none: a track of
/// its own).
#[derive(Clone, Debug, PartialEq)]
pub struct CompositeUnit {
    pub track: usize,
    pub children: Vec<CompositeUnit>,
}

/// The frame's tracks as compositing groups, top first: siblings in the
/// order they are drawn over each other reversed (the renderers draw
/// them from the last, the bottom one).
pub fn composite_units(tracks: &[TrackScene<'_>]) -> Vec<CompositeUnit> {
    // Top first.
    let order: Vec<usize> = (0..tracks.len()).rev().collect();
    let mut at = 0;
    units_at(tracks, &order, &mut at, 0)
}

fn units_at(
    tracks: &[TrackScene<'_>],
    order: &[usize],
    at: &mut usize,
    depth: u8,
) -> Vec<CompositeUnit> {
    let mut out = Vec::new();
    while *at < order.len() && (tracks[order[*at]].depth >= depth || out.is_empty() && depth == 0) {
        let me = order[*at];
        *at += 1;
        let mine = tracks[me].depth.max(depth);
        let children = if *at < order.len() && tracks[order[*at]].depth > mine {
            units_at(tracks, order, at, mine + 1)
        } else {
            Vec::new()
        };
        out.push(CompositeUnit {
            track: me,
            children,
        });
        if depth > 0 && *at < order.len() && tracks[order[*at]].depth < depth {
            break;
        }
    }
    out
}

fn track_content<'a>(
    project: &'a Project,
    track: &'a Track,
    t: Tick,
    width: u32,
    height: u32,
) -> Option<TrackContent<'a>> {
    let visible: Vec<&Clip> = track
        .clips
        .iter()
        .filter(|c| !c.muted && c.level > 0.0 && c.range().contains(t))
        .collect();
    let b = *visible.last()?;
    let mut layer_b = event_layer(project, b, t, width, height);
    // Where an event overlaps it, the crossfade is its fade there.
    let (fade_in, fade_out) = track.effective_fades(b, project.auto_crossfade);
    if fade_out > Tick::ZERO && t >= b.end() - fade_out {
        let p = ratio(b.end() - t, fade_out);
        layer_b.fade = Some(video_curve(b.fade_out_curve.out_shape(), p));
    }
    let below = visible
        .len()
        .checked_sub(2)
        .map(|i| visible[i])
        .filter(|_| project.auto_crossfade);
    // A plug-in's transition counts its time from the transition's start
    // (as far as the transition progress envelope says it has gone).
    let progressed = b
        .envelope(crate::envelope::EnvelopeKind::TransitionProgress)
        .is_some();
    let plugin = |length: Tick| {
        let time = if progressed {
            Tick(
                (b.transition_progress(t - b.start, length) as f64 * length.0 as f64).round()
                    as i64,
            )
        } else {
            t - b.start
        };
        b.transition_fx
            .as_deref()
            .filter(|_| !b.transition_bypass)
            .map(|fx| {
                Box::new(PluginCall {
                    fx,
                    owner: FxOwner::Transition(b.id),
                    index: 0,
                    time,
                    duration: length,
                    timeline: t + project.ruler_offset,
                    rate: project.settings.frame_rate,
                    full_size: (project.settings.width, project.settings.height),
                    source: None,
                })
            })
    };
    // Its transition switched off: a plain crossfade.
    let kind = if b.transition_bypass {
        Transition::default()
    } else {
        b.transition
    };
    if let Some(a) = below {
        // The automatic crossfade where the event overlaps the one before.
        return Some(TrackContent::Transition {
            kind,
            plugin: plugin(a.end() - b.start),
            from: Some(event_layer(project, a, t, width, height)),
            to: layer_b,
            progress: b.transition_progress(t - b.start, a.end() - b.start),
        });
    }
    if fade_in > Tick::ZERO && t < b.start + fade_in {
        return Some(TrackContent::Transition {
            kind,
            plugin: plugin(fade_in),
            from: None,
            to: layer_b,
            progress: video_curve(b.fade_in_curve, b.transition_progress(t - b.start, fade_in)),
        });
    }
    Some(TrackContent::Event(layer_b))
}

fn event_layer<'a>(
    project: &'a Project,
    clip: &'a Clip,
    t: Tick,
    width: u32,
    height: u32,
) -> EventLayer<'a> {
    let source = project.media(clip.media).and_then(|info| {
        let time = media_time(clip, info, t)?;
        let par = info.video.map_or(1.0, |v| v.pixel_aspect.ratio())
            / project.settings.pixel_aspect.ratio();
        let place = crate::pancrop::place_in(
            &project.settings,
            info,
            clip.pan_crop.keep_aspect,
            clip.pan_crop.fill,
        );
        let placement = place.at(clip, t, [f64::from(width), f64::from(height)]);
        // The size the media shows at, in its own pixels: the reader may
        // give less.
        let min_size = info
            .video
            .map_or((u32::MAX, u32::MAX), |_| placement.size_needed());
        Some(MediaLayer {
            info,
            clip,
            time,
            min_size,
            placement,
            place,
            par,
            quality: project.settings.render_quality,
            effects: chain(info, clip),
            local: t - clip.start,
            length: clip.duration,
            blend: frame_blend(project, clip, info, time),
            flow: clip.resample.unwrap_or(project.settings.resample)
                == crate::ResampleMode::OpticalFlow,
            deinterlace: deinterlace_of(project, info),
            // Video levels, or a media whose Color Range is Full: Y′CbCr
            // as it is.
            video_levels: project.settings.pixel_format.is_video_levels()
                || info.video.is_some_and(|v| v.full_range),
        })
    });
    EventLayer {
        clip: clip.id,
        source,
        level: clip.level.clamp(0.0, 1.0),
        fade: None,
    }
}

/// Frame Blend (the event's Resample mode, else the project's): when an
/// event shows its media at another rate than the media's own — its frame
/// rate not the project's, the event slowed or sped up —, the frame after
/// the one of the media's instant `time` and how far `time` is on the way
/// to it, the part of it mixed in. Optical flow blends until BananaV has
/// it; generated media and stills have no frames to blend.
fn frame_blend(
    project: &Project,
    clip: &Clip,
    info: &MediaInfo,
    time: Tick,
) -> Option<(Tick, f32)> {
    let mode = clip.resample.unwrap_or(project.settings.resample);
    if mode == crate::ResampleMode::Disable
        || info.still
        || !matches!(info.source, crate::MediaSource::File { .. })
    {
        return None;
    }
    let rate = info.video?.frame_rate;
    let other_rate = rate != project.settings.frame_rate;
    let stretched = (clip.rate - 1.0).abs() > 1e-9 || clip.velocity().is_some();
    if !other_rate && !stretched {
        return None;
    }
    let i = rate.frame_at(time);
    let (a, b) = (rate.frame_start(i), rate.frame_start(i + 1));
    if b <= a || b >= info.duration {
        return None;
    }
    let k = ((time - a).0 as f64 / (b - a).0 as f64) as f32;
    (k > 1e-3).then_some((b, k))
}

/// How a media's fields become whole frames (the project's Deinterlace
/// method) and their order, where they are not the project's (`None`
/// method: never).
fn deinterlace_of(
    project: &Project,
    info: &MediaInfo,
) -> Option<(crate::DeinterlaceMethod, bv_core::FieldOrder)> {
    let s = &project.settings;
    let order = info.video?.field_order;
    (order != bv_core::FieldOrder::Progressive
        && order != s.field_order
        && s.deinterlace != crate::DeinterlaceMethod::None)
        .then_some((s.deinterlace, order))
}

/// The media's effects, then the event's: the enabled video ones, each
/// with its owner and place.
fn chain<'a>(info: &'a MediaInfo, clip: &'a Clip) -> Vec<SceneFx<'a>> {
    let media = info.effects.iter().enumerate().map(|(index, fx)| SceneFx {
        fx,
        owner: FxOwner::Media(info.id),
        index,
    });
    let event = clip.effects.iter().enumerate().map(|(index, fx)| SceneFx {
        fx,
        owner: FxOwner::Event(clip.id),
        index,
    });
    media
        .chain(event)
        .filter(|e| e.fx.enabled && !e.fx.kind.is_audio())
        .collect()
}

impl<'a> FrameScene<'a> {
    /// The call that draws effect `k` of `m`'s chain (a plug-in effect of
    /// the media or the event), or `None` when it is no plug-in's;
    /// `fx_scale` is what the chain's effect sizes are scaled by there,
    /// `framing` how the media was framed for the event's effects (their
    /// pictures are the frame's: see [`MediaLayer::framed_from`]).
    pub fn media_call<'b>(
        &self,
        m: &'b MediaLayer<'a>,
        k: usize,
        fx_scale: f32,
        framing: Option<Framing>,
    ) -> Option<PluginCall<'b>> {
        let e = m.effects.get(k)?;
        let fx = e.fx.plugin.as_deref()?;
        let (time, duration) = match e.owner {
            FxOwner::Media(_) => (m.time, m.info.duration),
            _ => (m.local, m.length),
        };
        let full_size = match framing {
            Some(_) => self.project_size,
            None => m
                .info
                .video
                .map_or(self.project_size, |v| (v.width, v.height)),
        };
        Some(PluginCall {
            fx,
            owner: e.owner,
            index: e.index,
            time,
            duration,
            timeline: self.time + self.ruler_offset,
            rate: self.rate,
            full_size,
            source: Some(FxSource {
                info: m.info,
                clip: m.clip,
                media_time: matches!(e.owner, FxOwner::Media(_)),
                before: &m.effects[..k],
                fx_scale,
                video_gamma: self.video_gamma,
                enter: self.color.as_ref().map(|p| p.input),
                working: self.color.as_ref().map(|p| p.working),
                clips: self.clips,
                framing,
            }),
        })
    }

    /// The call that composites `track`'s picture with what is below it
    /// through its compositor (`parent`: its group's, onto what is below
    /// the group), when its mode is Custom and it has one.
    pub fn compositor_call(&self, track: &TrackScene<'a>, parent: bool) -> Option<PluginCall<'a>> {
        let (mode, fx) = if parent {
            (track.parent_mode, track.parent_compositor)
        } else {
            (track.mode, track.compositor)
        };
        if mode != crate::model::CompositeMode::Custom {
            return None;
        }
        Some(PluginCall {
            fx: fx?,
            owner: FxOwner::Compositor(track.track, parent),
            index: 0,
            time: self.time,
            duration: self.duration,
            timeline: self.time + self.ruler_offset,
            rate: self.rate,
            full_size: self.project_size,
            source: None,
        })
    }

    /// The call that draws effect `index` of a track's (or, with `None`,
    /// the output's), or `None` when it is no plug-in's.
    pub fn frame_call(
        &self,
        track: Option<TrackId>,
        index: usize,
        fx: &'a Effect,
    ) -> Option<PluginCall<'a>> {
        Some(PluginCall {
            fx: fx.plugin.as_deref()?,
            owner: track.map_or(FxOwner::Output, FxOwner::Track),
            index,
            time: self.time,
            duration: self.duration,
            timeline: self.time + self.ruler_offset,
            rate: self.rate,
            full_size: self.project_size,
            source: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CompositeMode, Generator, ProjectSettings, TrackKind};

    fn project() -> (Project, crate::MediaId) {
        let mut p = Project::new(ProjectSettings::default());
        let info = MediaInfo::generated(Generator::ColorBars, &p.settings);
        let m = p.add_media(info);
        (p, m)
    }

    #[test]
    fn modes_blend_as_their_formulas() {
        let near = |a: [f32; 4], b: [f32; 4]| a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-6);
        let gray = [0.5, 0.5, 0.5, 1.0];
        let red = [0.8, 0.2, 0.0, 1.0];
        // Source Alpha: the picture over what is below.
        let half = [0.4, 0.1, 0.0, 0.5];
        assert!(near(
            CompositeMode::SourceAlpha.blend(half, gray),
            [0.65, 0.35, 0.25, 1.0]
        ));
        assert!(near(
            CompositeMode::Screen.blend(red, gray),
            [0.9, 0.6, 0.5, 1.0]
        ));
        assert!(near(
            CompositeMode::Darken.blend(red, gray),
            [0.5, 0.2, 0.0, 1.0]
        ));
        assert!(near(
            CompositeMode::Difference.blend(red, gray),
            [0.3, 0.3, 0.5, 1.0]
        ));
        assert!(near(
            CompositeMode::Add.blend(red, gray),
            [1.3, 0.7, 0.5, 1.0]
        ));
        assert!(near(
            CompositeMode::Subtract.blend(red, gray),
            [0.0, 0.3, 0.5, 1.0]
        ));
        // Cut: the picture's shape out of what is below.
        assert!(near(
            CompositeMode::Cut.blend(half, gray),
            [0.25, 0.25, 0.25, 0.5]
        ));
        // Multiply (Mask): white keeps, black hides, transparent leaves.
        let white = [1.0, 1.0, 1.0, 1.0];
        let black = [0.0, 0.0, 0.0, 1.0];
        assert!(near(CompositeMode::MultiplyMask.blend(white, red), red));
        assert!(near(
            CompositeMode::MultiplyMask.blend(black, red),
            [0.0; 4]
        ));
        assert!(near(CompositeMode::MultiplyMask.blend([0.0; 4], red), red));
        // Overlay on mid gray is Hard Light's other way round.
        let o = CompositeMode::Overlay.blend(red, gray);
        let h = CompositeMode::HardLight.blend(gray, red);
        assert!(near(o, h));
        assert_eq!(CompositeMode::ALL.len(), 14);
        assert_eq!(CompositeMode::SourceAlpha.index(), 1);
    }

    #[test]
    fn effects_before_the_pan_crop_work_on_the_media_s_picture() {
        use crate::{Effect, EffectKind};
        let (mut p, m) = project();
        let v = p.tracks[0].id;
        let clip = p.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
        p.media.iter_mut().find(|x| x.id == m).unwrap().effects =
            vec![Effect::new(EffectKind::Sepia)];
        let mut off = Effect::new(EffectKind::Invert);
        off.enabled = false;
        p.clip_mut(clip).unwrap().effects = vec![
            Effect::new(EffectKind::BlackAndWhite),
            off,
            Effect::new(EffectKind::Invert),
        ];
        let split = |p: &Project| {
            let sc = frame_scene(p, Tick::ZERO, 64, 36);
            let (_, layer) = sc.media_layers().next().expect("the event");
            (layer.framed_from(), layer.effects.len())
        };
        // VEGAS's default: the Pan/Crop first, only the media's before it.
        assert_eq!(split(&p), (1, 3));
        // After the first two (one of them bypassed): one more before.
        p.clip_mut(clip).unwrap().pan_crop_at = 2;
        assert_eq!(split(&p), (2, 3));
        // Past the last: all of them.
        p.clip_mut(clip).unwrap().pan_crop_at = 9;
        assert_eq!(split(&p), (3, 3));
    }

    #[test]
    fn effects_after_the_composite_button_only_in_groups() {
        use crate::{Effect, EffectKind};
        let (mut p, m) = project();
        let top = p.tracks[0].id;
        let below = p.add_track(TrackKind::Video);
        p.insert_media(m, Tick::ZERO, Some(top), None).unwrap();
        p.insert_media(m, Tick::ZERO, Some(below), None).unwrap();
        let t = p.track_by_id_mut(top).unwrap();
        t.effects = vec![
            Effect::new(EffectKind::Sepia),
            Effect::new(EffectKind::Invert),
        ];
        t.composite_at = Some(1);
        let post = |p: &Project| {
            let sc = frame_scene(p, Tick::ZERO, 64, 36);
            let ts = sc
                .tracks
                .iter()
                .find(|s| s.track == top)
                .expect("the top track");
            (ts.post_from, ts.effects.len())
        };
        // Outside a group: all of them before compositing.
        assert_eq!(post(&p), (2, 2));
        // A compositing parent: the Invert after it.
        p.make_compositing_child(below, false).unwrap();
        assert_eq!(post(&p), (1, 2));
        // The button past the chain's end: none after.
        p.track_by_id_mut(top).unwrap().composite_at = Some(5);
        assert_eq!(post(&p), (2, 2));
    }

    #[test]
    fn frame_blend_mixes_the_next_frame_where_media_runs_at_another_rate() {
        use crate::{MediaSource, ResampleMode};
        let mut p = Project::new(ProjectSettings {
            frame_rate: bv_core::FrameRate::new(60, 1),
            ..ProjectSettings::default()
        });
        let mut info = MediaInfo::generated(Generator::ColorBars, &p.settings);
        info.source = MediaSource::File {
            path: "clip.mp4".into(),
        };
        info.video.as_mut().unwrap().frame_rate = bv_core::FrameRate::new(24, 1);
        info.duration = Tick::from_seconds(10);
        let m = p.add_media(info);
        let v = p.tracks[0].id;
        let clip = p.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
        // Frame 61 of the project, at 24 fps: 0.4 of the way from frame 24
        // to 25.
        let t = bv_core::FrameRate::new(60, 1).frame_start(61);
        let blend = |p: &Project| {
            let sc = frame_scene(p, t, 64, 36);
            sc.media_layers().next().unwrap().1.blend
        };
        assert_eq!(blend(&p), None, "VEGAS's default: no resampling");
        p.settings.resample = ResampleMode::FrameBlend;
        let (next, k) = blend(&p).expect("blended");
        assert_eq!(next, bv_core::FrameRate::new(24, 1).frame_start(25));
        assert!((k - 0.4).abs() < 1e-3, "{k}");
        // Optical Flow: the same frames, made along what moves.
        let flow = |p: &Project| {
            frame_scene(p, t, 64, 36)
                .media_layers()
                .next()
                .unwrap()
                .1
                .flow
        };
        assert!(!flow(&p));
        p.clip_mut(clip).unwrap().resample = Some(ResampleMode::OpticalFlow);
        assert!(flow(&p) && blend(&p).is_some());
        // The event's switch wins.
        p.clip_mut(clip).unwrap().resample = Some(ResampleMode::Disable);
        assert_eq!(blend(&p), None);
        // At the media's own rate, nothing to make.
        p.clip_mut(clip).unwrap().resample = None;
        p.settings.frame_rate = bv_core::FrameRate::new(24, 1);
        assert_eq!(blend(&p), None);
    }

    #[test]
    fn tracks_bypassing_motion_blur_hold_the_frame() {
        use crate::MediaSource;
        let mut p = Project::new(ProjectSettings::default());
        let mut info = MediaInfo::generated(Generator::ColorBars, &p.settings);
        info.source = MediaSource::File {
            path: "clip.mp4".into(),
        };
        info.duration = Tick::from_seconds(10);
        let m = p.add_media(info);
        let v = p.tracks[0].id;
        p.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
        let (t, held) = (Tick::from_seconds(2), Tick::from_seconds(1));
        let local = |p: &Project| {
            frame_scene_held(p, t, held, 64, 36)
                .media_layers()
                .next()
                .unwrap()
                .1
                .local
        };
        // A sub-frame shows its own time, but where the track bypasses
        // motion blur: there the frame's.
        assert_eq!(local(&p), t);
        p.track_by_id_mut(v).unwrap().bypass_motion_blur = true;
        assert_eq!(local(&p), held);
        assert_eq!(
            frame_scene(&p, t, 64, 36)
                .media_layers()
                .next()
                .unwrap()
                .1
                .local,
            t
        );
    }

    #[test]
    fn a_full_range_media_keeps_its_levels() {
        use crate::MediaSource;
        let mut p = Project::new(ProjectSettings::default());
        let mut info = MediaInfo::generated(Generator::ColorBars, &p.settings);
        info.source = MediaSource::File {
            path: "clip.mp4".into(),
        };
        info.duration = Tick::from_seconds(10);
        let m = p.add_media(info);
        let v = p.tracks[0].id;
        p.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
        let levels = |p: &Project| {
            frame_scene(p, Tick::from_seconds(1), 64, 36)
                .media_layers()
                .next()
                .unwrap()
                .1
                .video_levels
        };
        // Limited: brought out to full range; Full: as it is.
        assert!(!levels(&p));
        if let Some(v) = p
            .media
            .iter_mut()
            .find(|x| x.id == m)
            .and_then(|x| x.video.as_mut())
        {
            v.full_range = true;
        }
        assert!(levels(&p));
    }

    #[test]
    fn undersampled_events_hold_frames() {
        let p = Project::new(ProjectSettings::default());
        let mut info = MediaInfo::generated(crate::Generator::ColorBars, &p.settings);
        info.still = false;
        info.duration = Tick::from_seconds(10);
        let rate = info.video.unwrap().frame_rate;
        let mut clip = Clip::new(
            crate::ClipId(1),
            info.id,
            Tick::ZERO,
            Tick::from_seconds(10),
        );
        clip.undersample = 0.5;
        // Every other frame, each shown twice.
        let at = |f: i64| media_time(&clip, &info, rate.frame_start(f)).unwrap();
        assert_eq!(at(0), at(1));
        assert_eq!(at(2), at(3));
        assert_eq!(rate.frame_at(at(3)), 2);
        let first = at(0);
        clip.undersample = 1.0;
        assert_ne!(media_time(&clip, &info, rate.frame_start(1)), Some(first));
    }

    #[test]
    fn interlaced_media_is_deinterlaced_at_good_and_best_quality() {
        use bv_core::FieldOrder;
        let (mut p, m) = project();
        let height = p.media(m).unwrap().video.unwrap().height;
        p.media
            .iter_mut()
            .find(|x| x.id == m)
            .unwrap()
            .video
            .as_mut()
            .unwrap()
            .field_order = FieldOrder::UpperFirst;
        let v = p.tracks[0].id;
        p.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
        let how = |p: &Project, quality: RenderQuality, h: u32| {
            let mut sc = frame_scene(p, Tick::ZERO, 64, 36);
            sc.set_quality(quality);
            sc.media_layers().next().unwrap().1.deinterlaced(h)
        };
        let blend = Some((
            crate::DeinterlaceMethod::BlendFields,
            FieldOrder::UpperFirst,
        ));
        assert_eq!(
            how(&p, RenderQuality::Good, height),
            blend,
            "VEGAS's default"
        );
        assert_eq!(how(&p, RenderQuality::Preview, height), None);
        assert_eq!(
            how(&p, RenderQuality::Best, height / 2),
            None,
            "a reduced picture"
        );
        // An interlaced project of the same order keeps the fields.
        p.settings.field_order = FieldOrder::UpperFirst;
        assert_eq!(how(&p, RenderQuality::Best, height), None);
        p.settings.field_order = FieldOrder::LowerFirst;
        p.settings.deinterlace = crate::DeinterlaceMethod::Interpolate;
        assert_eq!(
            how(&p, RenderQuality::Best, height),
            Some((
                crate::DeinterlaceMethod::Interpolate,
                FieldOrder::UpperFirst
            ))
        );
    }

    #[test]
    fn parents_children_and_their_groups() {
        let (mut p, m) = project();
        // Video tracks 1 to 4 (the first is the project's), audio below.
        let mut v: Vec<TrackId> = vec![p.tracks[0].id];
        for _ in 0..3 {
            let id = p.add_track(TrackKind::Video);
            v.push(id);
        }
        assert!(!p.can_make_child(v[0]), "the top track is nobody's child");
        p.make_compositing_child(v[1], false).unwrap();
        p.make_compositing_child(v[2], false).unwrap();
        p.make_compositing_child(v[2], false).unwrap();
        assert_eq!(p.track(v[2]).unwrap().composite_depth, 2);
        // At most one in from the track above.
        p.make_compositing_child(v[2], false).unwrap();
        assert_eq!(p.track(v[2]).unwrap().composite_depth, 2);
        assert_eq!(p.compositing_family(v[0]), vec![v[0], v[1], v[2]]);
        assert!(p.is_compositing_parent(v[1]));
        // Ctrl: the track and its children one level out.
        p.make_compositing_parent(v[1], true).unwrap();
        assert_eq!(
            [1, 2].map(|i| p.track(v[i]).unwrap().composite_depth),
            [0, 1]
        );
        // A frame's groups: an event on each track; track 2 is 3's parent.
        for &t in &v {
            p.insert_media(m, Tick::ZERO, Some(t), None).unwrap();
        }
        let s = frame_scene(&p, Tick::ZERO, 64, 36);
        let units = composite_units(&s.tracks);
        let id = |u: &CompositeUnit| s.tracks[u.track].track;
        assert_eq!(units.len(), 3);
        assert_eq!(
            units.iter().map(id).collect::<Vec<_>>(),
            vec![v[0], v[1], v[3]]
        );
        assert_eq!(
            units[1].children.iter().map(id).collect::<Vec<_>>(),
            vec![v[2]]
        );
        // A muted parent keeps its group's place, with nothing.
        p.set_track_muted(v[1], true).unwrap();
        let s = frame_scene(&p, Tick::ZERO, 64, 36);
        let muted = s.tracks.iter().find(|t| t.track == v[1]).unwrap();
        assert!(matches!(muted.content, TrackContent::Nothing));
        assert_eq!(composite_units(&s.tracks)[1].children.len(), 1);
    }

    /// A transition switched off (its check box in the FX window) is a
    /// plain crossfade, its plug-in left out; on again, it is back.
    #[test]
    fn a_transition_switched_off_is_a_plain_crossfade() {
        let (mut p, m) = project();
        let v = p.tracks[0].id;
        let s = Tick::from_seconds;
        p.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
        let b = p.insert_media(m, s(3), Some(v), None).unwrap()[0];
        let c = p.clip_mut(b).unwrap();
        c.transition = Transition::FadeThroughBlack;
        c.transition_fx = Some(Box::new(PluginFx {
            id: "test.Wipe".into(),
            version: [1, 0],
            name: "Wipe".into(),
            params: Default::default(),
        }));
        let shown = |p: &Project| {
            let sc = frame_scene(p, s(4), 64, 36);
            let TrackContent::Transition { kind, plugin, .. } = &sc.tracks[0].content else {
                panic!("crossfade expected");
            };
            (*kind, plugin.is_some())
        };
        assert_eq!(shown(&p), (Transition::FadeThroughBlack, true));
        p.clip_mut(b).unwrap().transition_bypass = true;
        assert_eq!(shown(&p), (Transition::default(), false));
    }

    #[test]
    fn transition_progress_envelopes_hold_go_back_and_repeat() {
        use crate::envelope::EnvelopeKind;
        let (mut p, m) = project();
        let v = p.tracks[0].id;
        let s = Tick::from_seconds;
        p.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
        let b = p.insert_media(m, s(3), Some(v), None).unwrap()[0];
        let progress = |p: &Project, t: Tick| {
            let sc = frame_scene(p, t, 64, 36);
            let TrackContent::Transition { progress, .. } = &sc.tracks[0].content else {
                panic!("crossfade expected");
            };
            *progress
        };
        // The crossfade runs 3 to 5 s: a straight line without the envelope,
        // the envelope's line with it (0 to 100 % over the transition).
        assert!(
            p.toggle_event_envelope(b, EnvelopeKind::TransitionProgress)
                .unwrap()
        );
        let env = p
            .clip(b)
            .unwrap()
            .envelope(EnvelopeKind::TransitionProgress)
            .unwrap();
        assert_eq!(env.points.len(), 2);
        assert!((progress(&p, Tick::from_seconds_f64(3.5)) - 0.25).abs() < 1e-6);
        // Up to 100 % halfway, then back to 0.
        p.set_envelope_point(b, EnvelopeKind::TransitionProgress, s(1), 1.0)
            .unwrap();
        p.set_envelope_point(b, EnvelopeKind::TransitionProgress, s(2), 0.0)
            .unwrap();
        assert!((progress(&p, s(4)) - 1.0).abs() < 1e-6);
        assert!((progress(&p, Tick::from_seconds_f64(4.5)) - 0.5).abs() < 1e-6);
        // Events with no transition have none.
        let first = p.tracks[0].clips[0].id;
        assert!(
            p.toggle_event_envelope(first, EnvelopeKind::TransitionProgress)
                .is_err()
        );
    }

    #[test]
    fn standard_definition_media_fill_frames_a_few_percent_off() {
        // 486 lines in 480: enlarged by 486 ⁄ 480, the lines cropped.
        assert!((adjust_factor(720, 486, 1.0, 720, 480) - 486.0 / 480.0).abs() < 1e-4);
        // DV widescreen (pixels 1.2121 wide) in HD, and HD in it.
        let wide = 1.2121;
        let f = adjust_factor(720, 480, wide, 1920, 1080);
        assert!((f - 1.0227).abs() < 1e-3, "{f}");
        let f = adjust_factor(1920, 1080, 1.0 / wide, 720, 480);
        assert!((f - 1.0227).abs() < 1e-3, "{f}");
        // The same shape, or one far off (4:3 in 16:9): untouched.
        assert_eq!(adjust_factor(1280, 720, 1.0, 1920, 1080), 1.0);
        assert_eq!(adjust_factor(640, 480, 1.0, 1920, 1080), 1.0);
        // A project's event: only with the option on.
        let mut p = Project::default();
        p.settings.width = 720;
        p.settings.height = 480;
        let v = p.tracks[0].id;
        let mut info = MediaInfo::generated(Generator::ColorBars, &p.settings);
        info.video.as_mut().unwrap().height = 486;
        let m = p.add_media(info);
        p.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
        // How many of the frame's pixels a line of the media takes.
        let scale = |p: &Project| {
            event_layer(p, &p.tracks[0].clips[0], Tick::ZERO, 720, 480)
                .source
                .map(|m| 1.0 / m.placement.media_per_pixel().1)
        };
        assert!(p.settings.adjust_source, "on for new projects");
        let fitted = 480.0 / 486.0;
        assert!((scale(&p).unwrap() - fitted * 486.0 / 480.0).abs() < 2e-3);
        p.settings.adjust_source = false;
        assert!((scale(&p).unwrap() - fitted).abs() < 1e-4);
    }

    #[test]
    fn letterbox() {
        let r = fit_rect(640, 480, 1920, 1080);
        assert_eq!(
            r,
            Rect {
                x: 240.0,
                y: 0.0,
                w: 1440.0,
                h: 1080.0
            }
        );
    }

    #[test]
    fn crossfade_fade_in_and_empty_tracks() {
        let (mut p, m) = project();
        let v = p.tracks[0].id;
        let s = Tick::from_seconds;
        p.insert_media(m, Tick::ZERO, Some(v), None).unwrap();
        p.insert_media(m, s(3), Some(v), None).unwrap();
        p.add_track(TrackKind::Video);
        // Before the overlap: one event.
        let sc = frame_scene(&p, s(1), 64, 36);
        assert_eq!(sc.tracks.len(), 1, "the empty track has no layer");
        assert!(matches!(sc.tracks[0].content, TrackContent::Event(_)));
        // In the overlap: a crossfade halfway (3 to 5 s, at 4 s).
        let sc = frame_scene(&p, s(4), 64, 36);
        let TrackContent::Transition { from, progress, .. } = &sc.tracks[0].content else {
            panic!("crossfade expected");
        };
        assert!(from.is_some());
        assert!((progress - 0.5).abs() < 1e-6);
        // Both pictures come from the same media, at different instants.
        let times: Vec<Tick> = sc.media_layers().map(|(_, l)| l.time).collect();
        assert_eq!(times, vec![s(4), s(1)]);
        // Mute All Video: nothing.
        p.mute_all_video = true;
        assert!(frame_scene(&p, s(4), 64, 36).muted);
    }

    #[test]
    fn a_crossfade_takes_the_place_of_the_events_own_fades() {
        let (mut p, m) = project();
        let v = p.tracks[0].id;
        let s = Tick::from_seconds;
        let a = p.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
        let b = p.insert_media(m, s(3), Some(v), None).unwrap()[0];
        // Fades of their own longer than the overlap (3 to 5 s).
        p.clip_mut(a).unwrap().fade_out = s(4);
        p.clip_mut(b).unwrap().fade_in = s(4);
        // Before the overlap and after it, nothing fades.
        for t in [s(2), s(6)] {
            let sc = frame_scene(&p, t, 64, 36);
            let TrackContent::Event(layer) = &sc.tracks[0].content else {
                panic!("one event at {t:?}");
            };
            assert_eq!(layer.fade, None, "at {t:?}");
        }
        // In it, the crossfade.
        assert!(matches!(
            frame_scene(&p, s(4), 64, 36).tracks[0].content,
            TrackContent::Transition { from: Some(_), .. }
        ));
        // Without automatic crossfades, their own fades.
        p.auto_crossfade = false;
        let sc = frame_scene(&p, s(2), 64, 36);
        let TrackContent::Event(layer) = &sc.tracks[0].content else {
            panic!("one event");
        };
        assert!(layer.fade.is_some());
    }
}
