//! Ruler time formats (Options › Ruler Format) and grid spacing
//! (Options › Grid Spacing).

use crate::i18n::tr;
use bv_core::{FrameRate, Tick, timecode};
use bv_timeline::Project;

use bv_timeline::ProjectSettings;
pub use bv_timeline::RulerFormat;

/// Names and text of the ruler formats (the formats themselves live in the
/// project: File › Properties › Ruler).
pub trait RulerFormatExt {
    /// As Options › Ruler Format lists it.
    fn label(self) -> &'static str;
    /// As File › Properties › Ruler lists it.
    fn dialog_label(self) -> &'static str;
    /// Text for timeline time `t` (the project's ruler offset included).
    fn format(self, t: Tick, project: &Project) -> String;
    /// Text for a ruler time (offset already added).
    fn format_ruler_time(self, t: Tick, settings: &ProjectSettings) -> String;
    /// The ruler time `text` names in this format ("1.3" in Measures &
    /// Beats, "00:01:02:40" in Audio CD Time…), or in timecode or seconds.
    fn parse(self, text: &str, settings: &ProjectSettings) -> Option<Tick>;
}

/// An audio CD's frames: 75 a second (Red Book).
pub const CD_FRAMES: FrameRate = FrameRate::new(75, 1);

/// The timeline time `text` names in the project's ruler format: its
/// ruler offset taken away, a leading minus for times before it.
pub fn parse_position(project: &Project, text: &str) -> Option<Tick> {
    let format = project.settings.effective_ruler_format();
    let text = text.trim();
    let (negative, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let t = format.parse(body, &project.settings)?;
    let t = if negative { Tick::ZERO - t } else { t };
    let offset = if format.uses_offset() {
        project.ruler_offset
    } else {
        Tick::ZERO
    };
    Some(t - offset)
}

/// A length (or a time within a media) `text` names in the ruler's
/// format.
pub fn parse_length(project: &Project, text: &str) -> Option<Tick> {
    project
        .settings
        .effective_ruler_format()
        .parse(text, &project.settings)
}

/// A length (or a time within a media) written in the ruler's format,
/// without its offset.
pub fn length_text(project: &Project, d: Tick) -> String {
    let format = project.settings.effective_ruler_format();
    if d < Tick::ZERO {
        format!(
            "-{}",
            format.format_ruler_time(Tick::ZERO - d, &project.settings)
        )
    } else {
        format.format_ruler_time(d, &project.settings)
    }
}

/// The time display in Audio CD Time, as VEGAS's Help describes it:
/// "tt+mm:ss:ff", the track and the time from its start (minus before it,
/// in the pause that leads to it); `None` without a CD layout.
pub fn cd_track_time(project: &Project, t: Tick) -> Option<String> {
    let tracks = &project.cd_tracks;
    let clock = |d: Tick| {
        let frames = CD_FRAMES.frame_at(d);
        format!(
            "{:02}:{:02}:{:02}",
            frames / (75 * 60),
            (frames / 75) % 60,
            frames % 75
        )
    };
    let current = tracks.iter().rev().find(|c| c.range.start <= t);
    let (track, sign, d) = match current {
        Some(c) if t < c.range.end() => (c, '+', t - c.range.start),
        _ => match tracks.iter().find(|c| c.range.start > t) {
            Some(next) => (next, '-', next.range.start - t),
            None => {
                let last = current?;
                (last, '+', t - last.range.start)
            }
        },
    };
    let n = project.cd_track_number(track.id)?;
    Some(format!("{n:02}{sign}{}", clock(d)))
}

impl RulerFormatExt for RulerFormat {
    fn label(self) -> &'static str {
        match self {
            RulerFormat::Samples => tr("Samples"),
            RulerFormat::Time => tr("Time"),
            RulerFormat::Seconds => tr("Seconds"),
            RulerFormat::TimeAndFrames => tr("Time & Frames"),
            RulerFormat::AbsoluteFrames => tr("Absolute Frames"),
            RulerFormat::MeasuresBeats => tr("Measures & Beats"),
            RulerFormat::Feet16 => tr("Feet and Frames 16mm (40 fpf)"),
            RulerFormat::Feet35 => tr("Feet and Frames 35mm (16 fpf)"),
            RulerFormat::SmpteFilmSyncIvtc => tr("SMPTE Film Sync IVTC (23.976 fps, Video)"),
            RulerFormat::SmpteFilm => tr("SMPTE Film Sync (24 fps, Film)"),
            RulerFormat::SmpteEbu => tr("SMPTE EBU (25 fps, Video)"),
            RulerFormat::SmpteNonDrop => tr("SMPTE Non-Drop Frame (29.97 fps, Video)"),
            RulerFormat::SmpteDrop => tr("SMPTE Drop Frame (29.97 fps, Video)"),
            RulerFormat::Smpte30 => tr("SMPTE 30 (30 fps, Audio)"),
            RulerFormat::AudioCd => tr("Audio CD Time"),
        }
    }

    fn dialog_label(self) -> &'static str {
        match self {
            RulerFormat::Feet16 => tr("Feet & Frames 16mm (40 fpf)"),
            RulerFormat::Feet35 => tr("Feet & Frames 35mm (16 fpf)"),
            RulerFormat::SmpteNonDrop => tr("SMPTE Non-Drop (29.97 fps, Video)"),
            RulerFormat::SmpteDrop => tr("SMPTE Drop (29.97 fps, Video)"),
            other => other.label(),
        }
    }

    fn format(self, t: Tick, project: &Project) -> String {
        let offset = if self.uses_offset() {
            project.ruler_offset
        } else {
            Tick::ZERO
        };
        let t = t + offset;
        if t < Tick::ZERO {
            return format!(
                "-{}",
                self.format_ruler_time(Tick::ZERO - t, &project.settings)
            );
        }
        self.format_ruler_time(t, &project.settings)
    }

    fn format_ruler_time(self, t: Tick, settings: &ProjectSettings) -> String {
        let rate = settings.frame_rate;
        let secs = t.as_seconds_f64();
        let hms = |whole: u64| (whole / 3600, (whole / 60) % 60, whole % 60);
        match self {
            RulerFormat::Samples => settings.sample_rate.sample_at(t).to_string(),
            RulerFormat::Time => {
                let ms = (secs * 1000.0).round() as u64;
                let (h, m, s) = hms(ms / 1000);
                format!("{h:02}:{m:02}:{s:02}.{:03}", ms % 1000)
            }
            RulerFormat::Seconds => crate::fmt::num(secs, 3),
            RulerFormat::TimeAndFrames => {
                let frame = rate.frame_at(t);
                let start = rate.frame_start(frame);
                let whole = start.as_seconds_f64().floor() as u64;
                let in_second = frame - rate.frame_at(Tick::from_seconds(whole as i64));
                let (h, m, s) = hms(whole);
                format!("{h:02}:{m:02}:{s:02}.{in_second:02}")
            }
            RulerFormat::AbsoluteFrames => rate.frame_at(t).to_string(),
            RulerFormat::MeasuresBeats => {
                // measures.beats.ticks, 64 ticks per beat, counted from 1.1.
                let tempo = &settings.tempo;
                let beats = secs * tempo.beats_per_minute / 60.0;
                let per = tempo.beats_per_measure.max(1) as f64;
                // A hair over, so exact beats do not show as x.63.
                let beats = beats + 1e-9;
                let measure = (beats / per).floor();
                let in_measure = beats - measure * per;
                let beat = in_measure.floor();
                let ticks = ((in_measure - beat) * 64.0).floor();
                format!(
                    "{}.{}.{:03}",
                    measure as u64 + 1,
                    beat as u64 + 1,
                    ticks as u64
                )
            }
            RulerFormat::Feet16 | RulerFormat::Feet35 => {
                let per_foot = if self == RulerFormat::Feet16 { 40 } else { 16 };
                let f = rate.frame_at(t);
                format!("{}+{:02}", f / per_foot, f % per_foot)
            }
            RulerFormat::SmpteFilmSyncIvtc => {
                timecode::format_frame(FrameRate::FPS_23_976.frame_at(t), FrameRate::FPS_24)
            }
            RulerFormat::SmpteFilm => timecode::format(t, FrameRate::FPS_24),
            RulerFormat::SmpteEbu => timecode::format(t, FrameRate::FPS_25),
            RulerFormat::SmpteNonDrop => {
                timecode::format_frame(FrameRate::FPS_29_97.frame_at(t), FrameRate::FPS_30)
            }
            RulerFormat::SmpteDrop => timecode::format(t, FrameRate::FPS_29_97),
            RulerFormat::Smpte30 => timecode::format(t, FrameRate::FPS_30),
            RulerFormat::AudioCd => {
                let f = CD_FRAMES.frame_at(t).max(0) as u64;
                let (h, m, s) = hms(f / 75);
                format!("{h:02}:{m:02}:{s:02}:{:02}", f % 75)
            }
        }
    }

    fn parse(self, text: &str, settings: &ProjectSettings) -> Option<Tick> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let rate = settings.frame_rate;
        let number = |s: &str| s.trim().replace(',', ".").parse::<f64>().ok();
        let whole = |s: &str| s.trim().parse::<i64>().ok();
        let own = match self {
            RulerFormat::Samples => whole(text)
                .filter(|n| *n >= 0)
                .map(|n| settings.sample_rate.sample_start(n)),
            RulerFormat::Seconds => number(text)
                .filter(|n| *n >= 0.0)
                .map(Tick::from_seconds_f64),
            RulerFormat::AbsoluteFrames => {
                whole(text).filter(|n| *n >= 0).map(|n| rate.frame_start(n))
            }
            RulerFormat::MeasuresBeats => parse_measures(text, &settings.tempo),
            RulerFormat::Feet16 | RulerFormat::Feet35 => {
                let per_foot = if self == RulerFormat::Feet16 { 40 } else { 16 };
                text.split_once('+').and_then(|(feet, frames)| {
                    let (feet, frames) = (whole(feet)?, whole(frames)?);
                    (feet >= 0 && (0..per_foot).contains(&frames))
                        .then(|| rate.frame_start(feet * per_foot + frames))
                })
            }
            RulerFormat::AudioCd => {
                let parts: Option<Vec<i64>> = text.split(':').map(whole).collect();
                match parts.as_deref() {
                    Some(&[h, m, s, f]) => cd_time(h, m, s, f),
                    Some(&[m, s, f]) => cd_time(0, m, s, f),
                    _ => None,
                }
            }
            RulerFormat::SmpteFilmSyncIvtc => timecode::parse(text, FrameRate::FPS_23_976),
            RulerFormat::SmpteFilm => timecode::parse(text, FrameRate::FPS_24),
            RulerFormat::SmpteEbu => timecode::parse(text, FrameRate::FPS_25),
            RulerFormat::SmpteDrop | RulerFormat::SmpteNonDrop => {
                timecode::parse(text, FrameRate::FPS_29_97)
            }
            RulerFormat::Smpte30 => timecode::parse(text, FrameRate::FPS_30),
            RulerFormat::Time | RulerFormat::TimeAndFrames => None,
        };
        // "1.5" in Measures & Beats names a beat that is not there, not
        // a second and a half: only a clock with colons falls through.
        if self == RulerFormat::MeasuresBeats && !text.contains([':', ';']) {
            return own;
        }
        own.or_else(|| parse_clock(text, rate))
            .or_else(|| timecode::parse(text, rate))
    }
}

/// An audio CD time: hours, minutes, seconds and frames of 1/75 s.
fn cd_time(h: i64, m: i64, s: i64, f: i64) -> Option<Tick> {
    (h >= 0 && (0..60).contains(&m) && (0..60).contains(&s) && (0..75).contains(&f))
        .then(|| CD_FRAMES.frame_start(((h * 60 + m) * 60 + s) * 75 + f))
}

/// "measures.beats.ticks" counted from 1.1.000 (64 ticks a beat); a lone
/// measure is its first beat.
fn parse_measures(text: &str, tempo: &bv_timeline::Tempo) -> Option<Tick> {
    let parts: Vec<&str> = text.split('.').collect();
    let whole = |s: &str| s.trim().parse::<i64>().ok();
    let measure = whole(parts[0])?;
    let beat = parts.get(1).map_or(Some(1), |p| whole(p))?;
    let ticks = parts.get(2).map_or(Some(0), |p| whole(p))?;
    let per = tempo.beats_per_measure.max(1) as i64;
    if parts.len() > 3 || measure < 1 || !(1..=per).contains(&beat) || !(0..64).contains(&ticks) {
        return None;
    }
    let beats = ((measure - 1) * per + (beat - 1)) as f64 + ticks as f64 / 64.0;
    Some(Tick::from_seconds_f64(
        beats * 60.0 / tempo.beats_per_minute.max(1e-3),
    ))
}

/// "hh:mm:ss.mmm" (Time) or "hh:mm:ss.ff" (Time & Frames: two digits are
/// frames when the rate has fewer than 100).
fn parse_clock(text: &str, rate: FrameRate) -> Option<Tick> {
    let (clock, fraction) = text.split_once(['.', ','])?;
    let parts: Vec<i64> = clock
        .split(':')
        .map(|p| p.trim().parse().ok())
        .collect::<Option<_>>()?;
    let seconds = match parts[..] {
        [h, m, s] => h * 3600 + m * 60 + s,
        [m, s] => m * 60 + s,
        [s] => s,
        _ => return None,
    };
    let digits = fraction.trim();
    let value: i64 = digits.parse().ok()?;
    let base = Tick::from_seconds(seconds);
    if digits.len() == 2 && (value as u32) < rate.timecode_base() {
        // Frames within the second.
        let first = rate.frame_at(base);
        let first = if rate.frame_start(first) < base {
            first + 1
        } else {
            first
        };
        return Some(rate.frame_start(first + value));
    }
    let scale = 10f64.powi(digits.len() as i32);
    Some(base + Tick::from_seconds_f64(value as f64 / scale))
}

/// Spacing of the grid used by "Snap to Grid" and the grid lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridSpacing {
    RulerMarks,
    Minutes,
    Seconds,
    Milliseconds,
    Frames,
    /// Musical spacings at the project's tempo (File › Properties › Ruler).
    Measures,
    QuarterNotes,
    QuarterNoteTriplets,
    EighthNotes,
    EighthNoteTriplets,
    SixteenthNotes,
    SixteenthNoteTriplets,
    ThirtySecondNotes,
    ThirtySecondNoteTriplets,
}

impl GridSpacing {
    pub const ALL: [GridSpacing; 14] = [
        GridSpacing::RulerMarks,
        GridSpacing::Minutes,
        GridSpacing::Seconds,
        GridSpacing::Milliseconds,
        GridSpacing::Frames,
        GridSpacing::Measures,
        GridSpacing::QuarterNotes,
        GridSpacing::QuarterNoteTriplets,
        GridSpacing::EighthNotes,
        GridSpacing::EighthNoteTriplets,
        GridSpacing::SixteenthNotes,
        GridSpacing::SixteenthNoteTriplets,
        GridSpacing::ThirtySecondNotes,
        GridSpacing::ThirtySecondNoteTriplets,
    ];

    /// The musical spacings, in the menu's order.
    pub const MUSICAL: [GridSpacing; 9] = [
        GridSpacing::Measures,
        GridSpacing::QuarterNotes,
        GridSpacing::QuarterNoteTriplets,
        GridSpacing::EighthNotes,
        GridSpacing::EighthNoteTriplets,
        GridSpacing::SixteenthNotes,
        GridSpacing::SixteenthNoteTriplets,
        GridSpacing::ThirtySecondNotes,
        GridSpacing::ThirtySecondNoteTriplets,
    ];

    /// Whether the grid counts the project's tempo (its lines start at
    /// the ruler's 1.1, the ruler's zero).
    pub fn is_musical(self) -> bool {
        GridSpacing::MUSICAL.contains(&self)
    }

    /// The length of a musical spacing at `tempo`: a quarter note is a
    /// beat when the quarter gets one beat, two when the eighth does.
    pub fn musical_length(self, tempo: &bv_timeline::Tempo) -> Option<Tick> {
        let beat = tempo.beat().as_seconds_f64();
        let quarter = beat * tempo.beat_note.max(1) as f64 / 4.0;
        let seconds = match self {
            GridSpacing::Measures => beat * tempo.beats_per_measure.max(1) as f64,
            GridSpacing::QuarterNotes => quarter,
            GridSpacing::QuarterNoteTriplets => quarter * 2.0 / 3.0,
            GridSpacing::EighthNotes => quarter / 2.0,
            GridSpacing::EighthNoteTriplets => quarter / 3.0,
            GridSpacing::SixteenthNotes => quarter / 4.0,
            GridSpacing::SixteenthNoteTriplets => quarter / 6.0,
            GridSpacing::ThirtySecondNotes => quarter / 8.0,
            GridSpacing::ThirtySecondNoteTriplets => quarter / 12.0,
            _ => return None,
        };
        Some(Tick::from_seconds_f64(seconds))
    }

    pub fn label(self) -> &'static str {
        match self {
            GridSpacing::RulerMarks => tr("Ruler Marks"),
            GridSpacing::Minutes => tr("Minutes"),
            GridSpacing::Seconds => tr("Seconds"),
            GridSpacing::Milliseconds => tr("Milliseconds"),
            GridSpacing::Frames => tr("Frames"),
            GridSpacing::Measures => tr("Measures"),
            GridSpacing::QuarterNotes => tr("Quarter Notes"),
            GridSpacing::QuarterNoteTriplets => tr("Quarter Note Triplets"),
            GridSpacing::EighthNotes => tr("Eighth Notes"),
            GridSpacing::EighthNoteTriplets => tr("Eighth Note Triplets"),
            GridSpacing::SixteenthNotes => tr("Sixteenth Notes"),
            GridSpacing::SixteenthNoteTriplets => tr("Sixteenth Note Triplets"),
            GridSpacing::ThirtySecondNotes => tr("Thirty-Second Notes"),
            GridSpacing::ThirtySecondNoteTriplets => tr("Thirty-Second Note Triplets"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_one_and_a_half_seconds() {
        let mut p = Project::default();
        p.settings.frame_rate = FrameRate::FPS_30;
        let t = Tick::from_seconds_f64(61.5);
        let f = |r: RulerFormat| r.format(t, &p);
        assert_eq!(f(RulerFormat::Time), "00:01:01.500");
        assert_eq!(f(RulerFormat::Seconds), "61.500");
        assert_eq!(f(RulerFormat::TimeAndFrames), "00:01:01.15");
        assert_eq!(f(RulerFormat::AbsoluteFrames), "1845");
        assert_eq!(f(RulerFormat::Feet35), "115+05");
        assert_eq!(f(RulerFormat::SmpteEbu), "00:01:01:12");
        assert_eq!(f(RulerFormat::Samples), "2952000");
        // The ruler offset shifts what is shown.
        let mut q = p.clone();
        q.ruler_offset = Tick::from_seconds(3600);
        assert_eq!(RulerFormat::Time.format(t, &q), "01:01:01.500");
    }

    #[test]
    fn audio_cd_time_counts_75_frames_and_ignores_the_offset() {
        let mut p = Project::default();
        p.ruler_offset = Tick::from_seconds(3600);
        let t = Tick::from_seconds_f64(61.5);
        assert_eq!(RulerFormat::AudioCd.format(t, &p), "00:01:01:37");
        let back = RulerFormat::AudioCd
            .parse("00:01:01:37", &p.settings)
            .unwrap();
        assert_eq!(CD_FRAMES.frame_at(back), 61 * 75 + 37);
        assert!(!RulerFormat::AudioCd.uses_offset());
    }

    #[test]
    fn times_typed_in_the_ruler_format() {
        let mut p = Project::default();
        p.settings.frame_rate = FrameRate::FPS_30;
        let s = &p.settings;
        // 120 bpm, 4/4: measure 2, beat 3 is 3 s.
        let t = RulerFormat::MeasuresBeats.parse("2.3", s).unwrap();
        assert!((t.as_seconds_f64() - 3.0).abs() < 1e-6);
        let t = RulerFormat::MeasuresBeats.parse("1.1.032", s).unwrap();
        assert!((t.as_seconds_f64() - 0.25).abs() < 1e-6);
        assert_eq!(RulerFormat::MeasuresBeats.parse("1.5", s), None);
        let t = RulerFormat::Feet35.parse("115+05", s).unwrap();
        assert_eq!(s.frame_rate.frame_at(t), 1845);
        let t = RulerFormat::Time.parse("00:01:01.500", s).unwrap();
        assert!((t.as_seconds_f64() - 61.5).abs() < 1e-6);
        let t = RulerFormat::TimeAndFrames.parse("00:01:01.15", s).unwrap();
        assert_eq!(s.frame_rate.frame_at(t), 61 * 30 + 15);
        let t = RulerFormat::AbsoluteFrames.parse("1845", s).unwrap();
        assert_eq!(s.frame_rate.frame_at(t), 1845);
        // Timecode is always understood.
        let t = RulerFormat::Samples.parse("00:00:02;00", s).unwrap();
        assert!((t.as_seconds_f64() - 2.0).abs() < 1e-6);
    }

    #[test]
    fn musical_grid_lengths_follow_the_tempo() {
        let mut tempo = bv_timeline::Tempo::default();
        let len =
            |g: GridSpacing, t: &bv_timeline::Tempo| g.musical_length(t).unwrap().as_seconds_f64();
        // 120 bpm, a quarter note a beat: half a second.
        assert!((len(GridSpacing::QuarterNotes, &tempo) - 0.5).abs() < 1e-9);
        assert!((len(GridSpacing::Measures, &tempo) - 2.0).abs() < 1e-9);
        assert!((len(GridSpacing::EighthNoteTriplets, &tempo) - 0.5 / 3.0).abs() < 1e-9);
        // 6/8: the eighth gets the beat, so a quarter is two beats.
        tempo.beat_note = 8;
        tempo.beats_per_measure = 6;
        assert!((len(GridSpacing::QuarterNotes, &tempo) - 1.0).abs() < 1e-9);
        assert!((len(GridSpacing::Measures, &tempo) - 3.0).abs() < 1e-9);
        assert!(GridSpacing::Seconds.musical_length(&tempo).is_none());
    }

    #[test]
    fn measures_and_beats() {
        let mut p = Project::default();
        let f = |p: &Project, secs: f64| {
            RulerFormat::MeasuresBeats.format(Tick::from_seconds_f64(secs), p)
        };
        // 120 bpm, 4/4: a beat is half a second, a measure two seconds.
        assert_eq!(f(&p, 0.0), "1.1.000");
        assert_eq!(f(&p, 0.5), "1.2.000");
        assert_eq!(f(&p, 2.0), "2.1.000");
        assert_eq!(f(&p, 2.25), "2.1.032");
        // 3/4 at 90 bpm: a measure is two seconds.
        p.settings.tempo.beats_per_measure = 3;
        p.settings.tempo.beats_per_minute = 90.0;
        assert_eq!(f(&p, 2.0), "2.1.000");
        assert_eq!(f(&p, 4.0 + 2.0 / 3.0), "3.2.000");
    }
}
