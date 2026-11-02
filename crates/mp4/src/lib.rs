//! MP4, M4A and QuickTime MOV writing (ISO/IEC 14496-12 and -14, the
//! codec bindings of 14496-15 and AV1-ISOBMFF, Apple's QuickTime File
//! Format): one video and one audio track, written as the encoders give
//! them out. The movie header can go before the media (progressive
//! download): space for it is reserved at the start and filled at the end.
//! [`read`] reads what such a file says about itself (its boxes, its
//! timecode and when it was made, whether its video is 360°); [`tape`]
//! the start timecode of MXF and DV files.

use std::io::{self, Seek, SeekFrom, Write};

mod av1;
pub mod facts;
mod nal;
pub mod ps;
pub mod read;
pub mod tape;

pub use nal::split_annex_b;

use nal::{Flavor, Unit};

/// The family of the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Container {
    /// MPEG-4 (`.mp4`); with audio only, an `.m4a`.
    Mp4,
    /// QuickTime (`.mov`).
    Mov,
}

/// Compressed video, as the encoders give it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoFormat {
    /// H.264 access units in Annex B (start codes); the parameter sets in
    /// them move to the `avcC` box.
    H264,
    /// HEVC access units in Annex B.
    Hevc,
    /// AV1 temporal units (low-overhead OBUs).
    Av1,
    /// Apple ProRes frames, by sample entry (`apco`, `apcs`, `apcn`,
    /// `apch`, `ap4h`, `ap4x`); MOV only.
    ProRes { fourcc: [u8; 4] },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoTrack {
    pub format: VideoFormat,
    pub width: u32,
    pub height: u32,
    /// Ticks per second of the track's times.
    pub timescale: u32,
    /// Bits per sample (8, 10, 12).
    pub bit_depth: u8,
    /// H.273 colour description: primaries, transfer, matrix, full range.
    pub color: (u16, u16, u16, bool),
    /// The frames carry alpha (ProRes 4444).
    pub alpha: bool,
    /// The shape of the pixels, width : height (1 : 1 square).
    pub pixel_aspect: (u32, u32),
    /// A 360° video: the frames are the whole sphere around the camera,
    /// equirectangular. Players learn it from Google's spherical video
    /// metadata, written both ways: V2's boxes in the sample description,
    /// V1's XML in the track.
    pub spherical: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioFormat {
    /// AAC with its AudioSpecificConfig.
    Aac { config: Vec<u8>, bitrate: u32 },
    /// Apple Lossless with its ALACSpecificConfig ("magic cookie").
    Alac { cookie: Vec<u8> },
    /// Uncompressed little-endian samples (MOV).
    Pcm { bits: u16, float: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioTrack {
    pub format: AudioFormat,
    pub sample_rate: u32,
    pub channels: u16,
    /// Samples the encoder put before the sound (its delay): the edit list
    /// skips them.
    pub priming: u32,
}

/// Text that describes the file (shown by players and file managers).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Metadata {
    pub title: String,
    pub artist: String,
    pub comment: String,
    pub copyright: String,
    /// The program that wrote the file.
    pub encoder: String,
    /// The project the file was rendered from (Render's "Save project as
    /// path reference in rendered file"): a `bvpr` box of BananaV's in the
    /// movie's user data, which other programs pass over.
    pub project: String,
}

/// A named point of the timeline (a marker), in milliseconds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chapter {
    pub start_ms: u64,
    pub title: String,
}

/// The sample tables of a track, built as samples are written.
#[derive(Debug, Default)]
struct Table {
    count: u64,
    /// Sample sizes, unless they are all `constant_size`.
    sizes: Vec<u32>,
    constant_size: Option<u32>,
    /// stts: (count, delta).
    durations: Vec<(u32, u32)>,
    /// ctts: (count, offset).
    offsets: Vec<(u32, i32)>,
    any_offset: bool,
    /// 1-based numbers of the sync samples.
    sync: Vec<u32>,
    all_sync: bool,
    /// (file offset, first sample index, sample count).
    chunks: Vec<(u64, u64, u32)>,
    duration: u64,
}

impl Table {
    fn new() -> Table {
        Table {
            all_sync: true,
            ..Table::default()
        }
    }

    fn push(&mut self, size: u32, duration: u32, offset: i32, sync: bool) {
        self.push_many(1, size, duration, offset, sync);
    }

    /// `n` samples of `size` bytes and `duration` ticks each.
    fn push_many(&mut self, n: u32, size: u32, duration: u32, offset: i32, sync: bool) {
        if n == 0 {
            return;
        }
        match self.constant_size {
            _ if self.count == 0 => self.constant_size = Some(size),
            Some(c) if c != size => {
                self.sizes = vec![c; self.count as usize];
                self.constant_size = None;
            }
            _ => {}
        }
        if self.constant_size.is_none() {
            self.sizes.extend(std::iter::repeat_n(size, n as usize));
        }
        match self.durations.last_mut() {
            Some((c, d)) if *d == duration => *c += n,
            _ => self.durations.push((n, duration)),
        }
        match self.offsets.last_mut() {
            Some((c, o)) if *o == offset => *c += n,
            _ => self.offsets.push((n, offset)),
        }
        self.any_offset |= offset != 0;
        if sync {
            if n == 1 {
                self.sync.push(self.count as u32 + 1);
            }
        } else {
            self.all_sync = false;
        }
        self.count += n as u64;
        self.duration += n as u64 * duration as u64;
    }
}

/// Which track the last chunk belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Video,
    Audio,
}

/// An MP4/MOV file being written.
#[derive(Debug)]
pub struct Writer<W: Write + Seek> {
    out: W,
    container: Container,
    video: Option<(VideoTrack, Table)>,
    audio: Option<(AudioTrack, Table)>,
    /// Parameter sets / sequence header seen in the stream.
    vps: Vec<Vec<u8>>,
    sps: Vec<Vec<u8>>,
    pps: Vec<Vec<u8>>,
    av1_header: Option<Vec<u8>>,
    /// Where the reserved space for the movie header starts, and its size.
    reserved: Option<(u64, u64)>,
    mdat_header: u64,
    pos: u64,
    last: Option<Kind>,
    metadata: Metadata,
    chapters: Vec<Chapter>,
    /// Exact length of the audio (in samples), for its edit list.
    audio_length: Option<u64>,
    created: u64,
}

fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(body.len() + 8);
    v.extend_from_slice(&((body.len() + 8) as u32).to_be_bytes());
    v.extend_from_slice(kind);
    v.extend_from_slice(body);
    v
}

fn full(kind: &[u8; 4], version: u8, flags: u32, body: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(body.len() + 4);
    b.push(version);
    b.extend_from_slice(&flags.to_be_bytes()[1..]);
    b.extend_from_slice(body);
    bx(kind, &b)
}

/// Spherical Video V1's metadata, which older players and sites read: RDF
/// XML in a `uuid` box of the video track.
fn spherical_v1() -> Vec<u8> {
    let xml = concat!(
        "<?xml version=\"1.0\"?>",
        "<rdf:SphericalVideo\n",
        "xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"\n",
        "xmlns:GSpherical=\"http://ns.google.com/videos/1.0/spherical/\">",
        "<GSpherical:Spherical>true</GSpherical:Spherical>",
        "<GSpherical:Stitched>true</GSpherical:Stitched>",
        "<GSpherical:StitchingSoftware>BananaV</GSpherical:StitchingSoftware>",
        "<GSpherical:ProjectionType>equirectangular</GSpherical:ProjectionType>",
        "</rdf:SphericalVideo>",
    );
    let mut b = read::SPHERICAL_V1.to_vec();
    b.extend_from_slice(xml.as_bytes());
    bx(b"uuid", &b)
}

fn u16b(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_be_bytes());
}

fn u32b(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_be_bytes());
}

fn u64b(v: &mut Vec<u8>, x: u64) {
    v.extend_from_slice(&x.to_be_bytes());
}

/// The unity matrix of `mvhd` and `tkhd`.
fn matrix(v: &mut Vec<u8>) {
    for x in [0x0001_0000u32, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000] {
        u32b(v, x);
    }
}

/// Seconds since 1904 (the files' epoch).
fn now_1904() -> u64 {
    let unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    unix + 2_082_844_800
}

/// `ticks` of `from` per second in `to` per second, rounded.
fn rescale(ticks: u64, from: u32, to: u32) -> u64 {
    ((ticks as u128 * to as u128 + from as u128 / 2) / from.max(1) as u128) as u64
}

impl<W: Write + Seek> Writer<W> {
    /// Starts a file. `reserve` bytes are kept before the media for the
    /// movie header (0: it goes at the end); see [`Writer::reserve_for`].
    pub fn new(
        mut out: W,
        container: Container,
        video: Option<VideoTrack>,
        audio: Option<AudioTrack>,
        reserve: u64,
    ) -> io::Result<Writer<W>> {
        let mut ftyp = Vec::new();
        let audio_only = video.is_none();
        let (major, compatible): (&[u8; 4], Vec<&[u8; 4]>) = match container {
            Container::Mov => (b"qt  ", vec![b"qt  "]),
            Container::Mp4 if audio_only => (b"M4A ", vec![b"M4A ", b"isom", b"iso2"]),
            Container::Mp4 => {
                let mut c = vec![b"isom", b"iso2", b"mp41"];
                match video.map(|v| v.format) {
                    Some(VideoFormat::H264) => c.push(b"avc1"),
                    Some(VideoFormat::Av1) => c.push(b"av01"),
                    _ => {}
                }
                (b"isom", c)
            }
        };
        ftyp.extend_from_slice(major);
        u32b(&mut ftyp, 0x200);
        for c in compatible {
            ftyp.extend_from_slice(c);
        }
        let ftyp = bx(b"ftyp", &ftyp);
        out.write_all(&ftyp)?;
        let mut pos = ftyp.len() as u64;
        let reserved = if reserve >= 8 {
            out.write_all(&(reserve as u32).to_be_bytes())?;
            out.write_all(b"free")?;
            write_zeros(&mut out, reserve - 8)?;
            let r = (pos, reserve);
            pos += reserve;
            Some(r)
        } else {
            None
        };
        // A 64-bit mdat header: its size is known at the end.
        let mdat_header = pos;
        out.write_all(&1u32.to_be_bytes())?;
        out.write_all(b"mdat")?;
        out.write_all(&0u64.to_be_bytes())?;
        pos += 16;
        Ok(Writer {
            out,
            container,
            video: video.map(|v| (v, Table::new())),
            audio: audio.map(|a| (a, Table::new())),
            vps: Vec::new(),
            sps: Vec::new(),
            pps: Vec::new(),
            av1_header: None,
            reserved,
            mdat_header,
            pos,
            last: None,
            metadata: Metadata::default(),
            chapters: Vec::new(),
            audio_length: None,
            created: now_1904(),
        })
    }

    /// Bytes enough for the movie header of a file with these many video
    /// frames and audio packets (PCM: audio samples / 1024).
    pub fn reserve_for(video_frames: u64, audio_packets: u64) -> u64 {
        // stsz 4 + stco/co64 8 + stsc 12 + stss 4 + stts/ctts 16 per sample,
        // at worst, plus the fixed boxes, metadata and chapters.
        ((video_frames + audio_packets) * 44 + 64 * 1024).min(u32::MAX as u64 - 1)
    }

    /// Bytes of the file so far.
    pub fn bytes_written(&self) -> u64 {
        self.pos
    }

    pub fn set_metadata(&mut self, m: Metadata) {
        self.metadata = m;
    }

    pub fn set_chapters(&mut self, c: Vec<Chapter>) {
        self.chapters = c;
    }

    /// The exact length of the audio, in samples: what the edit list plays
    /// (an encoder pads its last packet).
    pub fn set_audio_length(&mut self, samples: u64) {
        self.audio_length = Some(samples);
    }

    fn put(&mut self, kind: Kind, data: &[u8], samples: u32) -> io::Result<()> {
        let table = match kind {
            Kind::Video => &mut self.video.as_mut().ok_or_else(no_track)?.1,
            Kind::Audio => &mut self.audio.as_mut().ok_or_else(no_track)?.1,
        };
        let first = table.count - samples as u64;
        if self.last == Some(kind)
            && let Some(c) = table.chunks.last_mut()
        {
            c.2 += samples;
        } else {
            table.chunks.push((self.pos, first, samples));
        }
        self.last = Some(kind);
        self.out.write_all(data)?;
        self.pos += data.len() as u64;
        Ok(())
    }

    /// A video frame: `pts` and `duration` in the track's timescale, `dts`
    /// when it differs from `pts` (B-frames).
    pub fn write_video(
        &mut self,
        data: &[u8],
        pts: i64,
        dts: Option<i64>,
        duration: u32,
        keyframe: bool,
    ) -> io::Result<()> {
        let format = self.video.as_ref().ok_or_else(no_track)?.0.format;
        let sample = match format {
            VideoFormat::H264 | VideoFormat::Hevc => {
                let flavor = if format == VideoFormat::H264 {
                    Flavor::Avc
                } else {
                    Flavor::Hevc
                };
                let mut out = Vec::with_capacity(data.len() + 16);
                for unit in nal::split_annex_b(data) {
                    match nal::classify(flavor, unit) {
                        Unit::Parameter(t) => {
                            let list = match (flavor, t) {
                                (Flavor::Hevc, 32) => &mut self.vps,
                                (Flavor::Avc, 7) | (Flavor::Hevc, 33) => &mut self.sps,
                                _ => &mut self.pps,
                            };
                            if !list.iter().any(|p| p == unit) {
                                list.push(unit.to_vec());
                            }
                        }
                        Unit::Delimiter => {}
                        Unit::Data => {
                            out.extend_from_slice(&(unit.len() as u32).to_be_bytes());
                            out.extend_from_slice(unit);
                        }
                    }
                }
                out
            }
            VideoFormat::Av1 => {
                if self.av1_header.is_none() {
                    self.av1_header = av1::sequence_header(data);
                }
                av1::sample(data)
            }
            VideoFormat::ProRes { .. } => data.to_vec(),
        };
        let offset = dts.map_or(0, |d| (pts - d) as i32);
        let table = &mut self.video.as_mut().ok_or_else(no_track)?.1;
        table.push(sample.len() as u32, duration, offset, keyframe);
        self.put(Kind::Video, &sample, 1)
    }

    /// Parameter sets announced outside the stream (an encoder's sequence
    /// header, Annex B).
    pub fn add_parameter_sets(&mut self, annex_b: &[u8]) {
        let Some(format) = self.video.as_ref().map(|v| v.0.format) else {
            return;
        };
        let flavor = match format {
            VideoFormat::H264 => Flavor::Avc,
            VideoFormat::Hevc => Flavor::Hevc,
            VideoFormat::Av1 => {
                if self.av1_header.is_none() {
                    self.av1_header = av1::sequence_header(annex_b);
                }
                return;
            }
            VideoFormat::ProRes { .. } => return,
        };
        for unit in nal::split_annex_b(annex_b) {
            if let Unit::Parameter(t) = nal::classify(flavor, unit) {
                let list = match (flavor, t) {
                    (Flavor::Hevc, 32) => &mut self.vps,
                    (Flavor::Avc, 7) | (Flavor::Hevc, 33) => &mut self.sps,
                    _ => &mut self.pps,
                };
                if !list.iter().any(|p| p == unit) {
                    list.push(unit.to_vec());
                }
            }
        }
    }

    /// A compressed audio packet of `samples` samples.
    pub fn write_audio(&mut self, data: &[u8], samples: u32) -> io::Result<()> {
        let (track, table) = self.audio.as_mut().ok_or_else(no_track)?;
        if let AudioFormat::Pcm { bits, .. } = track.format {
            // Uncompressed: every sample is one frame of all channels.
            let frame = (bits as u32 / 8) * track.channels as u32;
            let n = data.len() as u32 / frame.max(1);
            table.push_many(n, frame, 1, 0, true);
            return self.put(Kind::Audio, data, n);
        }
        table.push(data.len() as u32, samples, 0, true);
        self.put(Kind::Audio, data, 1)
    }

    /// Writes the movie header and returns the output.
    pub fn finish(mut self) -> io::Result<W> {
        let end = self.pos;
        self.out.seek(SeekFrom::Start(self.mdat_header + 8))?;
        self.out
            .write_all(&(end - self.mdat_header).to_be_bytes())?;
        let moov = self.moov();
        match self.reserved {
            Some((at, size)) if moov.len() as u64 + 8 <= size || moov.len() as u64 == size => {
                self.out.seek(SeekFrom::Start(at))?;
                self.out.write_all(&moov)?;
                let rest = size - moov.len() as u64;
                if rest >= 8 {
                    self.out.write_all(&(rest as u32).to_be_bytes())?;
                    self.out.write_all(b"free")?;
                }
            }
            _ => {
                self.out.seek(SeekFrom::Start(end))?;
                self.out.write_all(&moov)?;
            }
        }
        self.out.flush()?;
        Ok(self.out)
    }

    const MOVIE_SCALE: u32 = 1000;

    fn moov(&self) -> Vec<u8> {
        let mut traks = Vec::new();
        let mut movie_duration = 0u64;
        let mut next_id = 1u32;
        if let Some((v, t)) = &self.video {
            let d = rescale(t.duration, v.timescale, Self::MOVIE_SCALE);
            movie_duration = movie_duration.max(d);
            traks.push(self.trak_video(next_id, v, t, d));
            next_id += 1;
        }
        if let Some((a, t)) = &self.audio {
            let played = self
                .audio_length
                .unwrap_or_else(|| t.duration.saturating_sub(a.priming as u64));
            let d = rescale(played, a.sample_rate, Self::MOVIE_SCALE);
            movie_duration = movie_duration.max(d);
            traks.push(self.trak_audio(next_id, a, t, d, played));
            next_id += 1;
        }
        let mut mvhd = Vec::new();
        let v1 = movie_duration > u32::MAX as u64;
        if v1 {
            u64b(&mut mvhd, self.created);
            u64b(&mut mvhd, self.created);
            u32b(&mut mvhd, Self::MOVIE_SCALE);
            u64b(&mut mvhd, movie_duration);
        } else {
            u32b(&mut mvhd, self.created as u32);
            u32b(&mut mvhd, self.created as u32);
            u32b(&mut mvhd, Self::MOVIE_SCALE);
            u32b(&mut mvhd, movie_duration as u32);
        }
        u32b(&mut mvhd, 0x0001_0000); // rate 1.0
        u16b(&mut mvhd, 0x0100); // volume 1.0
        mvhd.extend_from_slice(&[0; 10]);
        matrix(&mut mvhd);
        mvhd.extend_from_slice(&[0; 24]);
        u32b(&mut mvhd, next_id);
        let mut moov = full(b"mvhd", v1 as u8, 0, &mvhd);
        for t in traks {
            moov.extend_from_slice(&t);
        }
        moov.extend_from_slice(&self.udta());
        bx(b"moov", &moov)
    }

    fn tkhd(&self, id: u32, duration: u64, audio: bool, size: (u32, u32)) -> Vec<u8> {
        let mut b = Vec::new();
        let v1 = duration > u32::MAX as u64;
        if v1 {
            u64b(&mut b, self.created);
            u64b(&mut b, self.created);
            u32b(&mut b, id);
            u32b(&mut b, 0);
            u64b(&mut b, duration);
        } else {
            u32b(&mut b, self.created as u32);
            u32b(&mut b, self.created as u32);
            u32b(&mut b, id);
            u32b(&mut b, 0);
            u32b(&mut b, duration as u32);
        }
        b.extend_from_slice(&[0; 8]);
        u16b(&mut b, 0); // layer
        u16b(&mut b, if audio { 1 } else { 0 }); // alternate group
        u16b(&mut b, if audio { 0x0100 } else { 0 });
        u16b(&mut b, 0);
        matrix(&mut b);
        u32b(&mut b, size.0 << 16);
        u32b(&mut b, size.1 << 16);
        // Enabled, in the movie and in previews.
        full(b"tkhd", v1 as u8, 0x7, &b)
    }

    /// An edit list showing `duration` (movie timescale) of the media from
    /// `media_time` (track timescale).
    fn edts(duration: u64, media_time: i64) -> Vec<u8> {
        let mut e = Vec::new();
        u32b(&mut e, 1);
        let v1 = duration > u32::MAX as u64;
        if v1 {
            u64b(&mut e, duration);
            e.extend_from_slice(&media_time.to_be_bytes());
        } else {
            u32b(&mut e, duration as u32);
            e.extend_from_slice(&(media_time as i32).to_be_bytes());
        }
        u32b(&mut e, 0x0001_0000);
        bx(b"edts", &full(b"elst", v1 as u8, 0, &e))
    }

    fn mdhd(&self, timescale: u32, duration: u64) -> Vec<u8> {
        let mut b = Vec::new();
        let v1 = duration > u32::MAX as u64;
        if v1 {
            u64b(&mut b, self.created);
            u64b(&mut b, self.created);
            u32b(&mut b, timescale);
            u64b(&mut b, duration);
        } else {
            u32b(&mut b, self.created as u32);
            u32b(&mut b, self.created as u32);
            u32b(&mut b, timescale);
            u32b(&mut b, duration as u32);
        }
        u16b(&mut b, 0x55C4); // "und"
        u16b(&mut b, 0);
        full(b"mdhd", v1 as u8, 0, &b)
    }

    fn hdlr(&self, kind: &[u8; 4], name: &str, media: bool) -> Vec<u8> {
        let mut b = Vec::new();
        match self.container {
            Container::Mov => {
                b.extend_from_slice(if media { b"mhlr" } else { b"dhlr" });
                b.extend_from_slice(kind);
                b.extend_from_slice(&[0; 12]);
                // A counted string.
                b.push(name.len() as u8);
                b.extend_from_slice(name.as_bytes());
            }
            Container::Mp4 => {
                u32b(&mut b, 0);
                b.extend_from_slice(kind);
                b.extend_from_slice(&[0; 12]);
                b.extend_from_slice(name.as_bytes());
                b.push(0);
            }
        }
        full(b"hdlr", 0, 0, &b)
    }

    fn dinf() -> Vec<u8> {
        let mut dref = Vec::new();
        u32b(&mut dref, 1);
        dref.extend_from_slice(&full(b"url ", 0, 1, &[]));
        bx(b"dinf", &full(b"dref", 0, 0, &dref))
    }

    fn minf(&self, header: Vec<u8>, stbl: Vec<u8>) -> Vec<u8> {
        let mut m = header;
        if self.container == Container::Mov {
            m.extend_from_slice(&self.hdlr(b"alis", "BananaV Data Handler", false));
        }
        m.extend_from_slice(&Self::dinf());
        m.extend_from_slice(&stbl);
        bx(b"minf", &m)
    }

    fn stbl(entry: Vec<u8>, t: &Table) -> Vec<u8> {
        let mut stsd = Vec::new();
        u32b(&mut stsd, 1);
        stsd.extend_from_slice(&entry);
        let mut s = full(b"stsd", 0, 0, &stsd);

        let mut stts = Vec::new();
        u32b(&mut stts, t.durations.len() as u32);
        for (c, d) in &t.durations {
            u32b(&mut stts, *c);
            u32b(&mut stts, *d);
        }
        s.extend_from_slice(&full(b"stts", 0, 0, &stts));

        if t.any_offset {
            let mut ctts = Vec::new();
            u32b(&mut ctts, t.offsets.len() as u32);
            let negative = t.offsets.iter().any(|(_, o)| *o < 0);
            for (c, o) in &t.offsets {
                u32b(&mut ctts, *c);
                ctts.extend_from_slice(&o.to_be_bytes());
            }
            s.extend_from_slice(&full(b"ctts", negative as u8, 0, &ctts));
        }

        if !t.all_sync {
            let mut stss = Vec::new();
            u32b(&mut stss, t.sync.len() as u32);
            for n in &t.sync {
                u32b(&mut stss, *n);
            }
            s.extend_from_slice(&full(b"stss", 0, 0, &stss));
        }

        // stsc: runs of chunks with the same number of samples.
        let mut runs: Vec<(u32, u32)> = Vec::new();
        for (i, c) in t.chunks.iter().enumerate() {
            if runs.last().is_none_or(|r| r.1 != c.2) {
                runs.push((i as u32 + 1, c.2));
            }
        }
        let mut stsc = Vec::new();
        u32b(&mut stsc, runs.len() as u32);
        for (first, n) in runs {
            u32b(&mut stsc, first);
            u32b(&mut stsc, n);
            u32b(&mut stsc, 1);
        }
        s.extend_from_slice(&full(b"stsc", 0, 0, &stsc));

        let mut stsz = Vec::new();
        match t.constant_size {
            Some(c) => {
                u32b(&mut stsz, c);
                u32b(&mut stsz, t.count as u32);
            }
            None => {
                u32b(&mut stsz, 0);
                u32b(&mut stsz, t.count as u32);
                for z in &t.sizes {
                    u32b(&mut stsz, *z);
                }
            }
        }
        s.extend_from_slice(&full(b"stsz", 0, 0, &stsz));

        let big = t.chunks.last().is_some_and(|c| c.0 > u32::MAX as u64);
        let mut co = Vec::new();
        u32b(&mut co, t.chunks.len() as u32);
        for c in &t.chunks {
            if big {
                u64b(&mut co, c.0);
            } else {
                u32b(&mut co, c.0 as u32);
            }
        }
        s.extend_from_slice(&full(if big { b"co64" } else { b"stco" }, 0, 0, &co));
        bx(b"stbl", &s)
    }

    fn visual_entry(&self, v: &VideoTrack) -> Vec<u8> {
        let (fourcc, compressor): (&[u8; 4], String) = match v.format {
            VideoFormat::H264 => (b"avc1", "AVC Coding".into()),
            VideoFormat::Hevc => (b"hvc1", "HEVC Coding".into()),
            VideoFormat::Av1 => (b"av01", "AOM Coding".into()),
            VideoFormat::ProRes { fourcc } => (
                match &fourcc {
                    b"apco" => b"apco",
                    b"apcs" => b"apcs",
                    b"apcn" => b"apcn",
                    b"ap4h" => b"ap4h",
                    b"ap4x" => b"ap4x",
                    _ => b"apch",
                },
                prores_name(fourcc).into(),
            ),
        };
        let mut e = vec![0u8; 6];
        u16b(&mut e, 1); // data reference index
        if self.container == Container::Mov {
            u16b(&mut e, 0); // version
            u16b(&mut e, 0); // revision
            e.extend_from_slice(b"BNNV");
            u32b(&mut e, 0); // temporal quality
            u32b(&mut e, 0x200); // spatial quality: normal
        } else {
            e.extend_from_slice(&[0; 16]);
        }
        u16b(&mut e, v.width as u16);
        u16b(&mut e, v.height as u16);
        u32b(&mut e, 0x0048_0000);
        u32b(&mut e, 0x0048_0000);
        u32b(&mut e, 0);
        u16b(&mut e, 1); // frame count
        let mut name = [0u8; 32];
        let n = compressor.len().min(31);
        name[0] = n as u8;
        name[1..=n].copy_from_slice(&compressor.as_bytes()[..n]);
        e.extend_from_slice(&name);
        u16b(&mut e, if v.alpha { 32 } else { 24 });
        e.extend_from_slice(&(-1i16).to_be_bytes());
        match v.format {
            VideoFormat::H264 => {
                e.extend_from_slice(&bx(b"avcC", &nal::avcc(&self.sps, &self.pps)))
            }
            VideoFormat::Hevc => {
                e.extend_from_slice(&bx(b"hvcC", &nal::hvcc(&self.vps, &self.sps, &self.pps)))
            }
            VideoFormat::Av1 => e.extend_from_slice(&bx(
                b"av1C",
                &av1::av1c(self.av1_header.as_deref().unwrap_or(&[]), v.bit_depth),
            )),
            VideoFormat::ProRes { .. } => {
                // One progressive field.
                e.extend_from_slice(&bx(b"fiel", &[1, 0]));
            }
        }
        let (p, t, m, full_range) = v.color;
        let mut colr = Vec::new();
        if self.container == Container::Mov {
            colr.extend_from_slice(b"nclc");
            u16b(&mut colr, p);
            u16b(&mut colr, t);
            u16b(&mut colr, m);
        } else {
            colr.extend_from_slice(b"nclx");
            u16b(&mut colr, p);
            u16b(&mut colr, t);
            u16b(&mut colr, m);
            colr.push(if full_range { 0x80 } else { 0 });
        }
        e.extend_from_slice(&bx(b"colr", &colr));
        let mut pasp = Vec::new();
        u32b(&mut pasp, v.pixel_aspect.0.max(1));
        u32b(&mut pasp, v.pixel_aspect.1.max(1));
        e.extend_from_slice(&bx(b"pasp", &pasp));
        if v.spherical {
            // Spherical Video V2: one picture for both eyes, the sphere's
            // projection looking ahead (no yaw, pitch or roll), nothing
            // cropped from the equirectangular frame.
            e.extend_from_slice(&full(b"st3d", 0, 0, &[0]));
            let proj = [full(b"prhd", 0, 0, &[0; 12]), full(b"equi", 0, 0, &[0; 16])].concat();
            let sv3d = [full(b"svhd", 0, 0, b"BananaV\0"), bx(b"proj", &proj)].concat();
            e.extend_from_slice(&bx(b"sv3d", &sv3d));
        }
        bx(fourcc, &e)
    }

    fn trak_video(&self, id: u32, v: &VideoTrack, t: &Table, movie_duration: u64) -> Vec<u8> {
        // The track shows at the width its pixels' shape gives.
        let (num, den) = (v.pixel_aspect.0.max(1), v.pixel_aspect.1.max(1));
        let shown = (v.width as u64 * num as u64 / den as u64) as u32;
        let mut trak = self.tkhd(id, movie_duration, false, (shown, v.height));
        if t.any_offset {
            // The first frame shows at its composition time.
            let first = t.offsets.first().map_or(0, |o| o.1) as i64;
            trak.extend_from_slice(&Self::edts(movie_duration, first));
        }
        let mut mdia = self.mdhd(v.timescale, t.duration);
        mdia.extend_from_slice(&self.hdlr(b"vide", "BananaV Video", true));
        let mut vmhd = Vec::new();
        u16b(&mut vmhd, 0);
        vmhd.extend_from_slice(&[0; 6]);
        let header = full(b"vmhd", 0, 1, &vmhd);
        mdia.extend_from_slice(&self.minf(header, Self::stbl(self.visual_entry(v), t)));
        trak.extend_from_slice(&bx(b"mdia", &mdia));
        if v.spherical {
            trak.extend_from_slice(&spherical_v1());
        }
        bx(b"trak", &trak)
    }

    fn audio_entry(&self, a: &AudioTrack) -> Vec<u8> {
        let mut e = vec![0u8; 6];
        u16b(&mut e, 1);
        let rate16 = if a.sample_rate <= 0xFFFF {
            a.sample_rate << 16
        } else {
            0
        };
        match &a.format {
            AudioFormat::Pcm { bits, float } => {
                // QuickTime SoundDescription version 2.
                u16b(&mut e, 2);
                u16b(&mut e, 0);
                u32b(&mut e, 0);
                u16b(&mut e, 3);
                u16b(&mut e, 16);
                e.extend_from_slice(&(-2i16).to_be_bytes());
                u16b(&mut e, 0);
                u32b(&mut e, 0x0001_0000);
                u32b(&mut e, 72);
                e.extend_from_slice(&(a.sample_rate as f64).to_bits().to_be_bytes());
                u32b(&mut e, a.channels as u32);
                u32b(&mut e, 0x7F00_0000);
                u32b(&mut e, *bits as u32);
                // Packed, little-endian; float or signed integer.
                u32b(&mut e, if *float { 0x1 | 0x8 } else { 0x4 | 0x8 });
                u32b(&mut e, *bits as u32 / 8 * a.channels as u32);
                u32b(&mut e, 1);
                bx(b"lpcm", &e)
            }
            AudioFormat::Aac { config, bitrate } => {
                e.extend_from_slice(&[0; 8]);
                u16b(&mut e, a.channels);
                u16b(&mut e, 16);
                u32b(&mut e, 0);
                u32b(&mut e, rate16);
                e.extend_from_slice(&esds(config, *bitrate));
                bx(b"mp4a", &e)
            }
            AudioFormat::Alac { cookie } => {
                e.extend_from_slice(&[0; 8]);
                u16b(&mut e, a.channels);
                // ALACSpecificConfig: bitDepth is its sixth byte.
                u16b(&mut e, cookie.get(5).copied().unwrap_or(16) as u16);
                u32b(&mut e, 0);
                u32b(&mut e, rate16);
                e.extend_from_slice(&full(b"alac", 0, 0, cookie));
                bx(b"alac", &e)
            }
        }
    }

    fn trak_audio(
        &self,
        id: u32,
        a: &AudioTrack,
        t: &Table,
        movie_duration: u64,
        played: u64,
    ) -> Vec<u8> {
        let mut trak = self.tkhd(id, movie_duration, true, (0, 0));
        if a.priming > 0 || played < t.duration {
            trak.extend_from_slice(&Self::edts(movie_duration, a.priming as i64));
        }
        let mut mdia = self.mdhd(a.sample_rate, t.duration);
        mdia.extend_from_slice(&self.hdlr(b"soun", "BananaV Audio", true));
        let header = full(b"smhd", 0, 0, &[0; 4]);
        mdia.extend_from_slice(&self.minf(header, Self::stbl(self.audio_entry(a), t)));
        trak.extend_from_slice(&bx(b"mdia", &mdia));
        bx(b"trak", &trak)
    }

    /// Metadata and chapters.
    fn udta(&self) -> Vec<u8> {
        let m = &self.metadata;
        let mut u = Vec::new();
        let items: [(&[u8; 4], &str); 5] = [
            (b"\xA9nam", &m.title),
            (b"\xA9ART", &m.artist),
            (b"\xA9cmt", &m.comment),
            (b"cprt", &m.copyright),
            (b"\xA9too", &m.encoder),
        ];
        match self.container {
            Container::Mp4 => {
                // iTunes-style: meta / hdlr mdir / ilst.
                let mut ilst = Vec::new();
                for (k, text) in items {
                    if text.is_empty() {
                        continue;
                    }
                    let mut data = Vec::new();
                    u32b(&mut data, 1); // UTF-8
                    u32b(&mut data, 0);
                    data.extend_from_slice(text.as_bytes());
                    ilst.extend_from_slice(&bx(k, &bx(b"data", &data)));
                }
                if !ilst.is_empty() {
                    let mut meta = Vec::new();
                    let mut hdlr = Vec::new();
                    u32b(&mut hdlr, 0);
                    hdlr.extend_from_slice(b"mdir");
                    hdlr.extend_from_slice(b"appl");
                    hdlr.extend_from_slice(&[0; 9]);
                    meta.extend_from_slice(&full(b"hdlr", 0, 0, &hdlr));
                    meta.extend_from_slice(&bx(b"ilst", &ilst));
                    u.extend_from_slice(&full(b"meta", 0, 0, &meta));
                }
            }
            Container::Mov => {
                for (k, text) in items {
                    if text.is_empty() {
                        continue;
                    }
                    // QuickTime text: length, language, text.
                    let mut b = Vec::new();
                    u16b(&mut b, text.len() as u16);
                    u16b(&mut b, 0x55C4);
                    b.extend_from_slice(text.as_bytes());
                    u.extend_from_slice(&bx(k, &b));
                }
            }
        }
        if !m.project.is_empty() {
            u.extend_from_slice(&bx(b"bvpr", m.project.as_bytes()));
        }
        if !self.chapters.is_empty() {
            // Nero chapters: start in 100 ns units and a counted title.
            let mut c = vec![0u8; 4];
            c.push(self.chapters.len().min(255) as u8);
            for ch in self.chapters.iter().take(255) {
                u64b(&mut c, ch.start_ms * 10_000);
                let title = &ch.title.as_bytes()[..ch.title.len().min(255)];
                c.push(title.len() as u8);
                c.extend_from_slice(title);
            }
            u.extend_from_slice(&full(b"chpl", 1, 0, &c));
        }
        if u.is_empty() {
            Vec::new()
        } else {
            bx(b"udta", &u)
        }
    }
}

/// The compressor name of a ProRes flavor.
fn prores_name(fourcc: [u8; 4]) -> &'static str {
    match &fourcc {
        b"apco" => "Apple ProRes 422 Proxy",
        b"apcs" => "Apple ProRes 422 LT",
        b"apcn" => "Apple ProRes 422",
        b"ap4h" => "Apple ProRes 4444",
        b"ap4x" => "Apple ProRes 4444 XQ",
        _ => "Apple ProRes 422 HQ",
    }
}

/// An MPEG-4 descriptor: tag, length (4-byte form) and body.
fn descriptor(tag: u8, body: &[u8]) -> Vec<u8> {
    let n = body.len() as u32;
    let mut v = vec![
        tag,
        0x80 | ((n >> 21) & 0x7F) as u8,
        0x80 | ((n >> 14) & 0x7F) as u8,
        0x80 | ((n >> 7) & 0x7F) as u8,
        (n & 0x7F) as u8,
    ];
    v.extend_from_slice(body);
    v
}

/// `esds`: the elementary stream descriptor of AAC audio.
fn esds(config: &[u8], bitrate: u32) -> Vec<u8> {
    let mut dcd = vec![0x40, 0x15]; // MPEG-4 audio, audio stream
    dcd.extend_from_slice(&[0, 0x18, 0]); // buffer size
    dcd.extend_from_slice(&bitrate.to_be_bytes());
    dcd.extend_from_slice(&bitrate.to_be_bytes());
    dcd.extend_from_slice(&descriptor(0x05, config));
    let mut es = vec![0, 0, 0]; // ES_ID, flags
    es.extend_from_slice(&descriptor(0x04, &dcd));
    es.extend_from_slice(&descriptor(0x06, &[0x02]));
    full(b"esds", 0, 0, &descriptor(0x03, &es))
}

fn no_track() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "the file has no such track")
}

fn write_zeros(out: &mut impl Write, mut n: u64) -> io::Result<()> {
    let block = [0u8; 8192];
    while n > 0 {
        let k = n.min(block.len() as u64) as usize;
        out.write_all(&block[..k])?;
        n -= k as u64;
    }
    Ok(())
}

/// An AudioSpecificConfig for AAC-LC (object type 2).
pub fn aac_config(sample_rate: u32, channels: u16) -> Vec<u8> {
    const RATES: [u32; 13] = [
        96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350,
    ];
    let index = RATES.iter().position(|&r| r == sample_rate).unwrap_or(4) as u16;
    let v: u16 = (2 << 11) | (index << 7) | ((channels.min(7)) << 3);
    v.to_be_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// The boxes at the top of `data`: (type, offset, size).
    fn top(data: &[u8]) -> Vec<([u8; 4], usize, usize)> {
        let mut out = Vec::new();
        let mut at = 0;
        while at + 8 <= data.len() {
            let mut size = u32::from_be_bytes(data[at..at + 4].try_into().unwrap()) as usize;
            let kind: [u8; 4] = data[at + 4..at + 8].try_into().unwrap();
            if size == 1 {
                size = u64::from_be_bytes(data[at + 8..at + 16].try_into().unwrap()) as usize;
            }
            out.push((kind, at, size));
            at += size;
        }
        out
    }

    fn track() -> VideoTrack {
        VideoTrack {
            format: VideoFormat::H264,
            width: 64,
            height: 48,
            timescale: 30,
            bit_depth: 8,
            color: (1, 1, 1, false),
            alpha: false,
            pixel_aspect: (1, 1),
            spherical: false,
        }
    }

    #[test]
    fn a_360_video_says_so_both_ways() {
        let au = [
            0, 0, 0, 1, 0x67, 100, 0, 30, 0, 0, 0, 1, 0x68, 0xCE, 0, 0, 0, 1, 0x65, 1,
        ];
        for container in [Container::Mp4, Container::Mov] {
            for spherical in [false, true] {
                let mut w = Writer::new(
                    Cursor::new(Vec::new()),
                    container,
                    Some(VideoTrack {
                        spherical,
                        ..track()
                    }),
                    None,
                    4096,
                )
                .unwrap();
                w.write_video(&au, 0, None, 1, true).unwrap();
                let data = w.finish().unwrap().into_inner();
                let has = |kind: &[u8]| data.windows(kind.len()).any(|b| b == kind);
                // V2's boxes and V1's XML, only for 360° video.
                for kind in [b"sv3d", b"svhd", b"prhd", b"equi", b"st3d"] {
                    assert_eq!(has(kind), spherical, "{container:?} {kind:?}");
                }
                assert_eq!(has(&crate::read::SPHERICAL_V1), spherical);
                assert_eq!(has(b"<GSpherical:Spherical>true"), spherical);
                assert_eq!(
                    crate::read::spherical(&mut Cursor::new(&data)),
                    spherical,
                    "{container:?}"
                );
                // The sample description's one entry still ends with it.
                let stsd = data.windows(4).position(|b| b == b"stsd").unwrap();
                let size = u32::from_be_bytes(data[stsd - 4..stsd].try_into().unwrap()) as usize;
                let entry = stsd + 12;
                let entry_size =
                    u32::from_be_bytes(data[entry..entry + 4].try_into().unwrap()) as usize;
                assert_eq!(entry + entry_size, stsd - 4 + size);
            }
        }
    }

    #[test]
    fn the_project_s_path_rides_in_the_user_data() {
        for container in [Container::Mp4, Container::Mov] {
            let mut w = Writer::new(
                Cursor::new(Vec::new()),
                container,
                Some(track()),
                None,
                4096,
            )
            .unwrap();
            w.set_metadata(Metadata {
                project: "C:/Projects/Show.bvproj".into(),
                ..Metadata::default()
            });
            let au = [
                0, 0, 0, 1, 0x67, 100, 0, 30, 0, 0, 0, 1, 0x68, 0xCE, 0, 0, 0, 1, 0x65, 1,
            ];
            w.write_video(&au, 0, None, 1, true).unwrap();
            let data = w.finish().unwrap().into_inner();
            assert_eq!(
                crate::read::project_reference(&mut Cursor::new(&data)).as_deref(),
                Some("C:/Projects/Show.bvproj"),
                "{container:?}"
            );
        }
        // None kept: none read.
        let mut w = Writer::new(
            Cursor::new(Vec::new()),
            Container::Mp4,
            Some(track()),
            None,
            4096,
        )
        .unwrap();
        let au = [
            0, 0, 0, 1, 0x67, 100, 0, 30, 0, 0, 0, 1, 0x68, 0xCE, 0, 0, 0, 1, 0x65, 1,
        ];
        w.write_video(&au, 0, None, 1, true).unwrap();
        let data = w.finish().unwrap().into_inner();
        assert_eq!(
            crate::read::project_reference(&mut Cursor::new(&data)),
            None
        );
    }

    #[test]
    fn wide_pixels_are_told() {
        let track = VideoTrack {
            pixel_aspect: (4, 3),
            ..track()
        };
        let mut w = Writer::new(
            Cursor::new(Vec::new()),
            Container::Mp4,
            Some(track),
            None,
            4096,
        )
        .unwrap();
        let au = [
            0, 0, 0, 1, 0x67, 100, 0, 30, 0, 0, 0, 1, 0x68, 0xCE, 0, 0, 0, 1, 0x65, 1,
        ];
        w.write_video(&au, 0, None, 1, true).unwrap();
        let data = w.finish().unwrap().into_inner();
        let at = data.windows(4).position(|b| b == b"pasp").expect("pasp");
        assert_eq!(&data[at + 4..at + 12], &[0, 0, 0, 4, 0, 0, 0, 3]);
        // The track shows 64 × 4/3 wide (16.16 fixed point in tkhd).
        let tkhd = data.windows(4).position(|b| b == b"tkhd").expect("tkhd");
        let size = u32::from_be_bytes(data[tkhd - 4..tkhd].try_into().unwrap()) as usize;
        let end = tkhd - 4 + size;
        let width = u32::from_be_bytes(data[end - 8..end - 4].try_into().unwrap()) >> 16;
        assert_eq!(width, 85);
    }

    #[test]
    fn the_movie_header_fills_the_reserved_space() {
        let mut w = Writer::new(
            Cursor::new(Vec::new()),
            Container::Mp4,
            Some(track()),
            None,
            4096,
        )
        .unwrap();
        let au = [
            0, 0, 0, 1, 0x67, 100, 0, 30, 0, 0, 0, 1, 0x68, 0xCE, 0, 0, 0, 1, 0x65, 1, 2, 3,
        ];
        w.write_video(&au, 0, None, 1, true).unwrap();
        w.write_video(&[0, 0, 0, 1, 0x41, 9], 1, None, 1, false)
            .unwrap();
        let data = w.finish().unwrap().into_inner();
        let boxes = top(&data);
        let kinds: Vec<&[u8; 4]> = boxes.iter().map(|b| &b.0).collect();
        assert_eq!(kinds, vec![b"ftyp", b"moov", b"free", b"mdat"]);
        // The samples are length-prefixed, without parameter sets.
        let mdat = boxes[3];
        assert_eq!(
            &data[mdat.1 + 16..mdat.1 + mdat.2],
            &[0, 0, 0, 4, 0x65, 1, 2, 3, 0, 0, 0, 2, 0x41, 9]
        );
    }

    #[test]
    fn without_space_the_movie_header_goes_last() {
        let mut w = Writer::new(
            Cursor::new(Vec::new()),
            Container::Mp4,
            Some(track()),
            None,
            0,
        )
        .unwrap();
        w.write_video(&[0, 0, 1, 0x65, 7], 0, None, 1, true)
            .unwrap();
        let data = w.finish().unwrap().into_inner();
        let kinds: Vec<[u8; 4]> = top(&data).iter().map(|b| b.0).collect();
        assert_eq!(kinds, vec![*b"ftyp", *b"mdat", *b"moov"]);
    }

    #[test]
    fn pcm_samples_are_counted_by_frame() {
        let audio = AudioTrack {
            format: AudioFormat::Pcm {
                bits: 16,
                float: false,
            },
            sample_rate: 48_000,
            channels: 2,
            priming: 0,
        };
        let mut w = Writer::new(
            Cursor::new(Vec::new()),
            Container::Mov,
            None,
            Some(audio),
            0,
        )
        .unwrap();
        w.write_audio(&[0u8; 4 * 100], 100).unwrap();
        w.write_audio(&[0u8; 4 * 50], 50).unwrap();
        let t = &w.audio.as_ref().unwrap().1;
        assert_eq!(t.count, 150);
        assert_eq!(t.constant_size, Some(4));
        assert_eq!(t.chunks.len(), 1);
        assert_eq!(t.durations, vec![(150, 1)]);
    }

    #[test]
    fn aac_config_for_48k_stereo() {
        assert_eq!(aac_config(48_000, 2), vec![0x11, 0x90]);
        assert_eq!(aac_config(44_100, 2), vec![0x12, 0x10]);
    }
}
