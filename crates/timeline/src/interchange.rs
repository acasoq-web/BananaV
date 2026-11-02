//! Project interchange formats (File › Import / Export): CMX3600 EDL
//! (export and import) and Final Cut Pro X XML (`fcpxml`) export. Final Cut
//! Pro 7's XML is [`crate::xmeml`]'s.
//!
//! Everything here is pure text in, text out; reading and writing files and
//! probing the referenced media is up to the caller.

use std::collections::HashMap;
use std::fmt::Write as _;

use bv_core::{FrameRate, TICKS_PER_SECOND, Tick, timecode};

use crate::model::{Clip, MediaId, MediaInfo, MediaSource, Project, TrackKind};

// ---- shared helpers -----------------------------------------------------------

fn media_path(m: &MediaInfo) -> Option<&str> {
    match &m.source {
        MediaSource::File { path } => Some(path),
        _ => None,
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// `file://` URL for a local path, percent-encoding everything outside the
/// unreserved set (Windows drive letters become `file:///C:/…`).
pub fn file_url(path: &str) -> String {
    let p = path.replace('\\', "/");
    let mut out = String::from(if p.starts_with('/') {
        "file://"
    } else {
        "file:///"
    });
    for b in p.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => {
                out.push(b as char)
            }
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Video tracks bottom-up then audio tracks, each with its 1-based number
/// within its kind (interchange formats count video tracks from the bottom).
fn numbered_tracks(p: &Project) -> Vec<(usize, TrackKind, usize)> {
    let mut out = Vec::new();
    let video: Vec<usize> = (0..p.tracks.len())
        .filter(|&i| p.tracks[i].kind == TrackKind::Video)
        .collect();
    for (n, &i) in video.iter().rev().enumerate() {
        out.push((i, TrackKind::Video, n + 1));
    }
    let audio = (0..p.tracks.len()).filter(|&i| p.tracks[i].kind == TrackKind::Audio);
    for (n, i) in audio.enumerate() {
        out.push((i, TrackKind::Audio, n + 1));
    }
    out
}

// ---- EDL ----------------------------------------------------------------------

/// One event of an EDL.
#[derive(Clone, Debug, PartialEq)]
pub struct EdlEvent {
    pub reel: String,
    pub kind: TrackKind,
    /// 1-based track number within its kind (`V`, `A`, `A2`, …).
    pub track: usize,
    pub source_in: Tick,
    pub source_out: Tick,
    pub record_in: Tick,
    pub record_out: Tick,
    /// From the `* FROM CLIP NAME:` comment.
    pub clip_name: Option<String>,
    /// From the `* SOURCE FILE:` comment.
    pub source_file: Option<String>,
}

/// CMX3600 EDL of every clip that uses a media file. Returns the text and
/// how many clips could not be represented (generated media, speed changes).
pub fn export_edl(p: &Project, title: &str) -> (String, usize) {
    let rate = p.settings.frame_rate;
    let mut out = String::new();
    let _ = writeln!(out, "TITLE: {title}");
    let fcm = if rate.is_drop_frame() {
        "DROP FRAME"
    } else {
        "NON-DROP FRAME"
    };
    let _ = writeln!(out, "FCM: {fcm}\n");
    let mut n = 0;
    let mut skipped = 0;
    let mut events: Vec<(&Clip, TrackKind, usize)> = Vec::new();
    for (i, kind, number) in numbered_tracks(p) {
        events.extend(p.tracks[i].clips.iter().map(|c| (c, kind, number)));
    }
    events.sort_by_key(|(c, kind, number)| (c.start, *kind == TrackKind::Audio, *number));
    for (c, kind, number) in events {
        let Some(m) = p.media(c.media) else { continue };
        let Some(path) = media_path(m) else {
            skipped += 1;
            continue;
        };
        if c.rate != 1.0 || c.freeze.is_some() {
            skipped += 1;
        }
        n += 1;
        let channel = match (kind, number) {
            (TrackKind::Video, 1) => "V".to_string(),
            (TrackKind::Video, k) => format!("V{k}"),
            (TrackKind::Audio, 1) => "A".to_string(),
            (TrackKind::Audio, k) => format!("A{k}"),
        };
        let src_out = c.source_in + c.to_source(c.duration);
        let tc = |t: Tick| timecode::format(rate.snap_nearest(t), rate);
        let _ = writeln!(
            out,
            "{n:03}  AX       {channel:<5} C        {} {} {} {}",
            tc(c.source_in),
            tc(src_out),
            tc(c.start),
            tc(c.end())
        );
        let _ = writeln!(out, "* FROM CLIP NAME: {}", file_name(path));
        let _ = writeln!(out, "* SOURCE FILE: {path}\n");
    }
    (out, skipped)
}

/// Parses a CMX3600 EDL. Timecodes are read at `rate`; when every record
/// time is past one hour (the usual 01:00:00:00 start), that hour is
/// removed so the sequence starts near zero.
pub fn parse_edl(text: &str, rate: FrameRate) -> Vec<EdlEvent> {
    let mut events: Vec<EdlEvent> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix('*') {
            let rest = rest.trim();
            let Some(e) = events.last_mut() else { continue };
            if let Some(v) = rest.strip_prefix("FROM CLIP NAME:") {
                e.clip_name = Some(v.trim().to_string());
            } else if let Some(v) = rest.strip_prefix("SOURCE FILE:") {
                e.source_file = Some(v.trim().to_string());
            }
            continue;
        }
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 8 || !f[0].chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        // Dissolves and wipes carry a duration field before the timecodes.
        let tcs: Vec<Tick> = f[f.len() - 4..]
            .iter()
            .filter_map(|s| timecode::parse(s, rate))
            .collect();
        if tcs.len() != 4 {
            continue;
        }
        let channel = f[2];
        let (kind, track) = if let Some(n) = channel.strip_prefix('V') {
            (TrackKind::Video, n.parse().unwrap_or(1))
        } else if let Some(n) = channel.strip_prefix('A') {
            (TrackKind::Audio, n.parse().unwrap_or(1))
        } else if channel == "B" {
            (TrackKind::Video, 1)
        } else {
            continue;
        };
        events.push(EdlEvent {
            reel: f[1].to_string(),
            kind,
            track,
            source_in: tcs[0],
            source_out: tcs[1],
            record_in: tcs[2],
            record_out: tcs[3],
            clip_name: None,
            source_file: None,
        });
    }
    let hour = Tick::from_seconds(3600);
    if !events.is_empty() && events.iter().all(|e| e.record_in >= hour) {
        for e in &mut events {
            e.record_in -= hour;
            e.record_out -= hour;
        }
    }
    events
}

// ---- Final Cut Pro X XML ----------------------------------------------------------

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a.abs() } else { gcd(b, a % b) }
}

/// FCPXML rational time: `"num/dens"` (or `"ns"` for whole seconds).
pub fn fcpxml_time(t: Tick) -> String {
    let d = TICKS_PER_SECOND;
    let g = gcd(t.0, d).max(1);
    let (n, d) = (t.0 / g, d / g);
    if d == 1 {
        format!("{n}s")
    } else {
        format!("{n}/{d}s")
    }
}

/// Final Cut Pro X XML (version 1.9). Each track becomes a lane of clips
/// connected to a gap that spans the whole sequence (video lanes above,
/// audio lanes below). Returns the text and how many clips were left out.
pub fn export_fcpxml(p: &Project, name: &str) -> (String, usize) {
    let s = p.settings;
    let rate = s.frame_rate;
    let frame = fcpxml_time(rate.frame_duration());
    let mut resources = format!(
        "<format id=\"r1\" frameDuration=\"{frame}\" width=\"{}\" height=\"{}\"/>",
        s.width, s.height
    );
    let mut assets: HashMap<MediaId, String> = HashMap::new();
    let mut skipped = 0;
    let mut clips = String::new();
    for (i, kind, number) in numbered_tracks(p) {
        let lane = match kind {
            TrackKind::Video => number as i64,
            TrackKind::Audio => -(number as i64),
        };
        for c in &p.tracks[i].clips {
            let Some(m) = p.media(c.media) else { continue };
            let Some(path) = media_path(m) else {
                skipped += 1;
                continue;
            };
            let next = format!("r{}", assets.len() + 2);
            let id = assets.entry(m.id).or_insert_with(|| {
                let _ = write!(
                    resources,
                    "<asset id=\"{next}\" name=\"{}\" start=\"0s\" duration=\"{}\" hasVideo=\"{}\" hasAudio=\"{}\" format=\"r1\"><media-rep kind=\"original-media\" src=\"{}\"/></asset>",
                    xml_escape(&m.name),
                    fcpxml_time(m.duration),
                    m.video.is_some() as u8,
                    m.audio.is_some() as u8,
                    xml_escape(&file_url(path)),
                );
                next.clone()
            });
            let enable = match kind {
                TrackKind::Video => "video",
                TrackKind::Audio => "audio",
            };
            let _ = write!(
                clips,
                "<asset-clip ref=\"{id}\" lane=\"{lane}\" name=\"{}\" offset=\"{}\" start=\"{}\" duration=\"{}\" srcEnable=\"{enable}\"{}/>",
                xml_escape(&m.name),
                fcpxml_time(c.start),
                fcpxml_time(c.source_in),
                fcpxml_time(c.duration),
                if c.muted { " enabled=\"0\"" } else { "" },
            );
        }
    }
    let total = fcpxml_time(p.duration().max(rate.frame_duration()));
    let tc_format = if rate.is_drop_frame() { "DF" } else { "NDF" };
    let out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE fcpxml>\n<fcpxml version=\"1.9\"><resources>{resources}</resources><library><event name=\"BananaV\"><project name=\"{}\"><sequence format=\"r1\" duration=\"{total}\" tcStart=\"0s\" tcFormat=\"{tc_format}\"><spine><gap name=\"Gap\" offset=\"0s\" start=\"0s\" duration=\"{total}\">{clips}</gap></spine></sequence></project></event></library></fcpxml>\n",
        xml_escape(name)
    );
    (out, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AudioStreamInfo, ProjectSettings, VideoStreamInfo};
    use bv_core::SampleRate;

    fn s(x: i64) -> Tick {
        Tick::from_seconds(x)
    }

    fn project() -> Project {
        let mut p = Project::new(ProjectSettings::default());
        let (v, a) = (p.tracks[0].id, p.tracks[1].id);
        let m = p.add_media(MediaInfo {
            id: MediaId(0),
            source: MediaSource::File {
                path: r"C:\Vídeos\meu clipe.mp4".into(),
            },
            name: "meu clipe.mp4".into(),
            duration: s(20),
            video: Some(VideoStreamInfo {
                width: 1920,
                height: 1080,
                frame_rate: p.settings.frame_rate,
                pixel_aspect: Default::default(),
                field_order: Default::default(),
                spherical: false,
                full_range: false,
                reading: Default::default(),
            }),
            audio: Some(AudioStreamInfo {
                sample_rate: SampleRate::HZ_48000,
                channels: 2,
            }),
            extra_audio: Vec::new(),
            sequence: None,
            still: false,
            effects: Vec::new(),
            markers: Vec::new(),
            color_space: None,
            reel: Default::default(),
            layer: None,
        });
        let ids = p.insert_media(m, s(2), Some(v), Some(a)).unwrap();
        p.trim_start(ids[0], s(3)).unwrap();
        p.trim_end(ids[0], s(8)).unwrap();
        p
    }

    #[test]
    fn edl_round_trip() {
        let p = project();
        let (text, skipped) = export_edl(&p, "Test");
        assert_eq!(skipped, 0);
        let events = parse_edl(&text, p.settings.frame_rate);
        assert_eq!(events.len(), 2, "{text}");
        let v = &events[0];
        assert_eq!((v.kind, v.track), (TrackKind::Video, 1));
        // 29.97 fps: times come back on the nearest frame.
        let f = |x: i64| p.settings.frame_rate.snap_nearest(s(x));
        assert_eq!((v.record_in, v.record_out), (f(3), f(8)));
        assert_eq!((v.source_in, v.source_out), (f(1), f(6)));
        assert_eq!(v.source_file.as_deref(), Some(r"C:\Vídeos\meu clipe.mp4"));
        assert_eq!(events[1].kind, TrackKind::Audio);
    }

    #[test]
    fn edl_one_hour_start_is_removed() {
        let text = "TITLE: x\nFCM: NON-DROP FRAME\n\n001  AX       V     C        00:00:01:00 00:00:02:00 01:00:00:00 01:00:01:00\n";
        let e = parse_edl(text, FrameRate::FPS_30);
        assert_eq!(e[0].record_in, Tick::ZERO);
        assert_eq!(e[0].source_in, s(1));
    }

    #[test]
    fn fcpxml_uses_rational_times_and_lanes() {
        let p = project();
        let (xml, _) = export_fcpxml(&p, "Seq");
        assert!(xml.contains("offset=\"3s\""), "{xml}");
        assert!(xml.contains("lane=\"1\"") && xml.contains("lane=\"-1\""));
        assert_eq!(fcpxml_time(Tick(TICKS_PER_SECOND / 2)), "1/2s");
    }
}
