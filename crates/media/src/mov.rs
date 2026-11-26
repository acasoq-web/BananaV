//! MPEG-4 family files (`.mov`, `.mp4`, `.m4v`…): what BananaV reads of
//! them itself, beside the system's readers. The turn a track's matrix
//! shows its picture with (a phone's portrait video, stored lying), and
//! the formats the system reads but has no decoder for on Windows —
//! QuickTime's Animation and uncompressed video ([`bv_quicktime`]), PNG and
//! TIFF pictures as frames, and µ-law, A-law and IMA 4:1 sound —, decoded
//! from the system's packets as ProRes is.

use std::fs::File;
use std::sync::Arc;

use bv_core::{
    ColorInfo, ColorRange, FrameRate, ImageRgbaF32, PixelFormat, Plane, PlaneData, Tick, VideoFrame,
};
use bv_image::read::{Picture, PictureData};
use bv_mp4::facts::TrackFacts;
use bv_timeline::{AlphaChannel, MediaRotation};

use crate::files::{AlphaReading, alpha_over, mf_err, reduction, to_linear};
use crate::{MediaError, Result, VideoReader};

/// Whether a file starting with `head` is an MPEG-4 family one: its first
/// box one of theirs (QuickTime's older files start without `ftyp`).
pub(crate) fn is_mpeg4(head: &[u8]) -> bool {
    matches!(
        head.get(4..8),
        Some(b"ftyp" | b"moov" | b"mdat" | b"wide" | b"free" | b"skip" | b"pnot")
    )
}

/// The tracks of `path` when it is an MPEG-4 family file (none otherwise).
pub(crate) fn tracks(path: &str) -> Vec<TrackFacts> {
    File::open(path)
        .map(|mut f| bv_mp4::facts::tracks(&mut f))
        .unwrap_or_default()
}

pub(crate) fn video_track(tracks: &[TrackFacts]) -> Option<&TrackFacts> {
    tracks.iter().find(|t| &t.handler == b"vide")
}

/// The turn a file's video track shows its picture with.
pub(crate) fn rotation(tracks: &[TrackFacts]) -> MediaRotation {
    match video_track(tracks).map_or(0, |t| t.rotation) {
        1 => MediaRotation::Clockwise,
        2 => MediaRotation::Inverted,
        3 => MediaRotation::Counterclockwise,
        _ => MediaRotation::Original,
    }
}

/// A video track BananaV decodes itself.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum OwnVideo {
    QuickTime(TrackFacts),
    Png,
    Tiff,
}

/// How BananaV decodes `tracks`' video itself, when it does.
pub(crate) fn own_video(tracks: &[TrackFacts]) -> Option<OwnVideo> {
    let t = video_track(tracks)?;
    if bv_quicktime::supported(t.codec, t.depth) {
        return Some(OwnVideo::QuickTime(t.clone()));
    }
    match &t.codec {
        b"png " | b"PNG " => Some(OwnVideo::Png),
        b"tiff" | b"TIFF" => Some(OwnVideo::Tiff),
        _ => None,
    }
}

/// A frame decoded: QuickTime's samples, or a picture.
enum Decoded {
    Frame(bv_quicktime::Frame),
    Picture(Picture),
}

impl Decoded {
    fn size(&self) -> (u32, u32) {
        match self {
            Decoded::Frame(f) => (f.width, f.height),
            Decoded::Picture(p) => (p.width, p.height),
        }
    }
}

/// A video track BananaV decodes from the system's packets: every packet
/// from the keyframe before the time asked for (Animation's frames build
/// on the one before), going on from where it is when it can.
pub(crate) struct MovVideo {
    reader: bv_mf::PacketReader,
    kind: OwnVideo,
    decoder: Option<bv_quicktime::Decoder>,
    rate: FrameRate,
    /// The frame decoded last: its start and end (100 ns) and itself.
    current: Option<(i64, i64, Arc<Decoded>)>,
    /// A packet read past the time asked for: the next one decoded.
    pending: Option<bv_mf::Packet>,
    /// The start of the next packet the reader gives (none: after a seek).
    next: Option<i64>,
    /// The last frame converted (start, reduction, levels).
    last: Option<(i64, u32, bool, Arc<ImageRgbaF32>)>,
    video_levels: bool,
    alpha: AlphaReading,
}

impl MovVideo {
    pub(crate) fn new(reader: bv_mf::PacketReader, kind: OwnVideo) -> MovVideo {
        let decoder = match &kind {
            OwnVideo::QuickTime(t) => bv_quicktime::Decoder::new(
                t.codec,
                u32::from(t.width),
                u32::from(t.height),
                t.depth,
                &t.palette,
            ),
            _ => None,
        };
        let rate = reader.info().frame_rate;
        MovVideo {
            reader,
            kind,
            decoder,
            rate,
            current: None,
            pending: None,
            next: None,
            last: None,
            video_levels: false,
            alpha: AlphaReading::default(),
        }
    }

    fn next_packet(&mut self) -> Result<Option<bv_mf::Packet>> {
        if let Some(p) = self.pending.take() {
            return Ok(Some(p));
        }
        self.reader.next_packet().map_err(mf_err)
    }

    fn decode(&mut self, data: &[u8]) -> Result<Decoded> {
        let corrupt = |e: &dyn std::fmt::Display| MediaError::Corrupt(e.to_string());
        Ok(match &self.kind {
            OwnVideo::QuickTime(_) => {
                let d = self
                    .decoder
                    .as_mut()
                    .ok_or_else(|| MediaError::Corrupt("no decoder".into()))?;
                Decoded::Frame(d.decode(data).map_err(|e| corrupt(&e))?)
            }
            OwnVideo::Png => {
                let png = bv_png::decode(data).map_err(|e| corrupt(&e))?;
                let pixels = match png.pixels {
                    bv_png::Pixels::Rgba8(px) => PictureData::Rgba8(px),
                    bv_png::Pixels::Rgba16(px) => PictureData::Rgba16(px),
                };
                Decoded::Picture(Picture {
                    width: png.width,
                    height: png.height,
                    data: pixels,
                    linear: false,
                    premultiplied: false,
                })
            }
            OwnVideo::Tiff => {
                Decoded::Picture(bv_image::read::read(data).map_err(|e| corrupt(&e))?)
            }
        })
    }

    /// The decoded frame showing at `t` (100 ns).
    fn decoded(&mut self, t: i64) -> Result<(i64, Arc<Decoded>)> {
        if let Some((s, e, d)) = &self.current
            && *s <= t
            && t < *e
        {
            return Ok((*s, d.clone()));
        }
        let nominal = self.rate.frame_duration().to_timescale(10_000_000).max(1);
        // Going on is cheaper than seeking (and Animation's frames build on
        // those before).
        let ahead = self.next.is_some_and(|n| t >= n && t < n + 2 * 10_000_000)
            && self.current.as_ref().is_none_or(|(s, _, _)| t >= *s);
        if !ahead {
            self.reader
                .seek(Tick::from_timescale(t.max(0), 10_000_000))
                .map_err(mf_err)?;
            self.pending = None;
            self.next = None;
            if let Some(d) = self.decoder.as_mut() {
                d.reset();
            }
        }
        let mut shown: Option<(i64, i64, Arc<Decoded>)> = None;
        while let Some(p) = self.next_packet()? {
            let end = p.time + if p.duration > 0 { p.duration } else { nominal };
            if p.time > t && shown.is_some() {
                // The frame before shows at `t`: this one waits its turn.
                self.next = Some(p.time);
                self.pending = Some(p);
                break;
            }
            let d = Arc::new(self.decode(&p.data)?);
            self.next = Some(end);
            let here = p.time <= t && t < end;
            shown = Some((p.time, end, d));
            if here {
                break;
            }
        }
        let (s, e, d) = shown
            .or_else(|| self.current.clone())
            .ok_or_else(|| MediaError::Corrupt("no frames".into()))?;
        self.current = Some((s, e, d.clone()));
        Ok((s, d))
    }
}

impl VideoReader for MovVideo {
    fn frame_at(&mut self, t: Tick) -> Result<Arc<ImageRgbaF32>> {
        self.frame_at_scaled(t, 0, 0)
    }

    fn set_video_levels(&mut self, on: bool) {
        self.video_levels = on;
    }

    fn set_alpha(&mut self, alpha: AlphaChannel, background: [u8; 3]) {
        let alpha = AlphaReading {
            mode: alpha,
            background,
        };
        if alpha != self.alpha {
            self.alpha = alpha;
            self.last = None;
        }
    }

    fn frame_at_scaled(&mut self, t: Tick, min_w: u32, min_h: u32) -> Result<Arc<ImageRgbaF32>> {
        let (start, d) = self.decoded(t.to_timescale(10_000_000))?;
        let (w, h) = d.size();
        let factor = if min_w == 0 && min_h == 0 {
            1
        } else {
            reduction(w, h, min_w, min_h).min(8)
        };
        let levels = self.video_levels;
        if let Some((s, f, l, img)) = &self.last
            && *s == start
            && *f == factor
            && *l == levels
        {
            return Ok(img.clone());
        }
        let img = match &*d {
            Decoded::Picture(p) => to_linear(p, factor as usize, self.alpha),
            Decoded::Frame(f) => match &f.pixels {
                bv_quicktime::Pixels::Rgba8(px) => to_linear(
                    &Picture {
                        width: f.width,
                        height: f.height,
                        data: PictureData::Rgba8(px.clone()),
                        linear: false,
                        premultiplied: false,
                    },
                    factor as usize,
                    self.alpha,
                ),
                bv_quicktime::Pixels::Rgba16(px) => to_linear(
                    &Picture {
                        width: f.width,
                        height: f.height,
                        data: PictureData::Rgba16(px.clone()),
                        linear: false,
                        premultiplied: false,
                    },
                    factor as usize,
                    self.alpha,
                ),
                bv_quicktime::Pixels::Yuv {
                    chroma444,
                    y,
                    cb,
                    cr,
                    alpha,
                } => {
                    let (w, h) = (f.width as usize, f.height as usize);
                    let cw = if *chroma444 { w } else { w.div_ceil(2) };
                    let plane = |data: &[u16], pw: usize| Plane {
                        width: pw,
                        height: h,
                        stride: pw,
                        data: PlaneData::U16(data.to_vec()),
                    };
                    let mut color = ColorInfo::guess_for(f.width, f.height);
                    color.range = ColorRange::Limited;
                    let frame = VideoFrame {
                        width: f.width,
                        height: f.height,
                        format: if *chroma444 {
                            PixelFormat::Yuv444P10
                        } else {
                            PixelFormat::Yuv422P10
                        },
                        color,
                        planes: vec![plane(y, w), plane(cb, cw), plane(cr, cw)],
                        pts: Tick::ZERO,
                        duration: Tick::ZERO,
                    };
                    let mut img = bv_color::frame_to_linear_levels(&frame, factor, levels);
                    if let Some(a) = alpha {
                        alpha_over(&mut img, a, w, h, factor, self.alpha);
                    }
                    img
                }
            },
        };
        let img = Arc::new(img);
        self.last = Some((start, factor, levels, img.clone()));
        Ok(img)
    }
}

/// Sound BananaV decodes itself from the system's packets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OwnAudio {
    MuLaw,
    ALaw,
    Ima4,
}

/// How BananaV decodes a sound track itself, when it does.
pub(crate) fn own_audio(t: &TrackFacts) -> Option<OwnAudio> {
    match &t.codec {
        b"ulaw" | b"ULAW" => Some(OwnAudio::MuLaw),
        b"alaw" | b"ALAW" => Some(OwnAudio::ALaw),
        b"ima4" => Some(OwnAudio::Ima4),
        _ => None,
    }
}

/// The `i`-th sound track's (in the file's order, as the system numbers
/// them) own decoding, when it has one.
pub(crate) fn own_audio_track(tracks: &[TrackFacts], i: usize) -> Option<(OwnAudio, &TrackFacts)> {
    let t = tracks.iter().filter(|t| &t.handler == b"soun").nth(i)?;
    own_audio(t).map(|a| (a, t))
}

/// A decoder of µ-law, A-law or IMA 4:1 packets.
pub(crate) struct OwnAudioDecoder {
    codec: OwnAudio,
    rate: u32,
    channels: usize,
    /// IMA 4:1's bytes of a packet cut in two.
    carry: Vec<u8>,
}

impl OwnAudioDecoder {
    pub(crate) fn new(codec: OwnAudio, t: &TrackFacts) -> OwnAudioDecoder {
        OwnAudioDecoder {
            codec,
            rate: t.sample_rate.max(1),
            channels: usize::from(t.channels.max(1)),
            carry: Vec::new(),
        }
    }
}

impl bv_mf::PacketDecoder for OwnAudioDecoder {
    fn decode(&mut self, packet: &[u8], out: &mut Vec<f32>) -> std::result::Result<(), String> {
        match self.codec {
            OwnAudio::MuLaw => out.extend(
                packet
                    .iter()
                    .map(|&b| f32::from(bv_wav::mulaw(b)) / 32768.0),
            ),
            OwnAudio::ALaw => {
                out.extend(packet.iter().map(|&b| f32::from(bv_wav::alaw(b)) / 32768.0))
            }
            OwnAudio::Ima4 => {
                // Each channel's 34-byte packet in turn, 64 samples each.
                self.carry.extend_from_slice(packet);
                let group = 34 * self.channels;
                let whole = self.carry.len() / group * group;
                for frame in self.carry[..whole].chunks_exact(group) {
                    let decoded: Vec<[i16; 64]> = frame
                        .as_chunks::<34>()
                        .0
                        .iter()
                        .map(bv_wav::ima4_packet)
                        .collect();
                    for k in 0..64 {
                        for c in &decoded {
                            out.push(f32::from(c[k]) / 32768.0);
                        }
                    }
                }
                self.carry.drain(..whole);
            }
        }
        Ok(())
    }

    fn reset(&mut self) {
        self.carry.clear();
    }

    fn sample_rate(&self) -> u32 {
        self.rate
    }

    fn channels(&self) -> usize {
        self.channels
    }
}
