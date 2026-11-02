//! Options › Preferences › File I/O's logs (VEGAS 2026's "Log Plugin
//! loading", "Log Reads" and "Log Renders"): the plug-ins found, the media
//! read and the renders written down in text files, to find out which file
//! or plug-in makes trouble — Basic a line each, Detailed with what each
//! one found. They are `Plugins.log`, `Reads.log` and `Renders.log` in the
//! `Logs` folder of BananaV's local data (`%LOCALAPPDATA%\BananaV` on
//! Windows), in English, as technical support reads them.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

use crate::i18n::tr;

/// How much a log says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    #[default]
    Off,
    Basic,
    Detailed,
}

impl LogLevel {
    pub const ALL: [LogLevel; 3] = [LogLevel::Off, LogLevel::Basic, LogLevel::Detailed];

    pub fn name(self) -> &'static str {
        match self {
            LogLevel::Off => tr("Off"),
            LogLevel::Basic => tr("Basic"),
            LogLevel::Detailed => tr("Detailed"),
        }
    }
}

/// Which log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LogKind {
    Plugins,
    Reads,
    Renders,
}

/// Each log's level (the preferences', set by [`set_levels`]): the
/// searches for plug-ins run on threads of their own.
static LEVELS: [AtomicU8; 3] = [AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0)];

/// The preferences' levels, from now on.
pub(crate) fn set_levels(plugins: LogLevel, reads: LogLevel, renders: LogLevel) {
    for (kind, level) in [
        (LogKind::Plugins, plugins),
        (LogKind::Reads, reads),
        (LogKind::Renders, renders),
    ] {
        LEVELS[kind as usize].store(level as u8, Ordering::Relaxed);
    }
}

fn level(kind: LogKind) -> LogLevel {
    match LEVELS[kind as usize].load(Ordering::Relaxed) {
        1 => LogLevel::Basic,
        2 => LogLevel::Detailed,
        _ => LogLevel::Off,
    }
}

/// Whether `kind`'s log is on (what it would write is worth working out).
pub(crate) fn wants(kind: LogKind) -> bool {
    level(kind) != LogLevel::Off
}

impl LogKind {
    fn file(self) -> &'static str {
        match self {
            LogKind::Plugins => "Plugins.log",
            LogKind::Reads => "Reads.log",
            LogKind::Renders => "Renders.log",
        }
    }
}

/// The logs' folder.
pub(crate) fn folder() -> Option<PathBuf> {
    Some(bv_platform::cache_dir()?.join("BananaV").join("Logs"))
}

/// Notes down a failure of the audio output in `Audio.log`, always: they
/// are rare, and without it nothing tells later what went wrong.
pub(crate) fn audio(line: &str) {
    let Some(dir) = folder() else {
        return;
    };
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(&dir)?;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("Audio.log"))?
            .write_all(format!("{}  {line}\r\n", now()).as_bytes())
    };
    let _ = write();
}

/// Writes `lines` down in `kind`'s log at its level: the first one in
/// Basic, all of them in Detailed (the others indented under it).
pub(crate) fn log(kind: LogKind, lines: &[String]) {
    let level = level(kind);
    if level == LogLevel::Off || lines.is_empty() {
        return;
    }
    if let Some(dir) = folder() {
        let _ = write_to(&dir, kind, level, lines, &now());
    }
}

fn write_to(
    dir: &Path,
    kind: LogKind,
    level: LogLevel,
    lines: &[String],
    stamp: &str,
) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(kind.file()))?;
    let n = if level == LogLevel::Detailed {
        lines.len()
    } else {
        1
    };
    let mut text = String::new();
    for (i, line) in lines.iter().take(n).enumerate() {
        let indent = if i == 0 { "" } else { "    " };
        text.push_str(&format!("{stamp}  {indent}{line}\r\n"));
    }
    f.write_all(text.as_bytes())
}

/// The time now, as `2026-10-21 14:03:22 UTC`.
fn now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs()) as i64;
    let (y, m, d) = bv_core::date::civil_from_days(secs.div_euclid(86_400));
    let s = secs.rem_euclid(86_400);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC",
        s / 3600,
        s / 60 % 60,
        s % 60
    )
}

/// A media's streams as Reads' Detailed lines say them.
pub(crate) fn media_lines(info: &bv_timeline::MediaInfo) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(v) = info.video {
        out.push(format!(
            "video: {}x{}, {:.3} fps, pixel aspect {}:{}, field order {:?}{}{}",
            v.width,
            v.height,
            v.frame_rate.as_f64(),
            v.pixel_aspect.num,
            v.pixel_aspect.den,
            v.field_order,
            if info.still { ", still picture" } else { "" },
            if v.spherical { ", 360°" } else { "" },
        ));
    }
    for (i, a) in info.audio.iter().chain(&info.extra_audio).enumerate() {
        out.push(format!(
            "audio stream {}: {} Hz, {} channels",
            i + 1,
            a.sample_rate.0,
            a.channels
        ));
    }
    out.push(format!("length: {:.3} s", info.duration.as_seconds_f64()));
    out
}

/// A render's settings as Renders' Detailed lines say them.
pub(crate) fn render_lines(s: &bv_engine::RenderSettings) -> Vec<String> {
    let mut out = vec![
        format!("Render started: {}", s.path.display()),
        format!("format: {:?}", s.format),
        format!(
            "range: {:.3} s to {:.3} s",
            s.range.start.as_seconds_f64(),
            s.range.end().as_seconds_f64()
        ),
    ];
    if let Some(v) = &s.video {
        out.push(format!(
            "video: {}x{}, {:.3} fps, {} bits, {:?}, {} b/s",
            v.width,
            v.height,
            v.frame_rate.as_f64(),
            v.bit_depth,
            v.encoder,
            v.bitrate
        ));
    }
    if let Some(a) = &s.audio {
        out.push(format!(
            "audio: {} Hz, {} channels, {} bits{}",
            a.sample_rate,
            a.channels,
            a.bit_depth,
            if a.float { " float" } else { "" }
        ));
    }
    out.push(format!("options: {:?}", s.options));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_writes_a_line_and_detailed_all_of_them() {
        let dir = std::env::temp_dir().join(format!("bv-logs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let lines = ["Read a.wav".to_string(), "audio: 48000 Hz".to_string()];
        let stamp = "2026-10-21 12:00:00 UTC";
        write_to(&dir, LogKind::Reads, LogLevel::Basic, &lines, stamp).unwrap();
        write_to(&dir, LogKind::Reads, LogLevel::Detailed, &lines, stamp).unwrap();
        let text = std::fs::read_to_string(dir.join("Reads.log")).unwrap();
        assert_eq!(
            text,
            "2026-10-21 12:00:00 UTC  Read a.wav\r\n\
             2026-10-21 12:00:00 UTC  Read a.wav\r\n\
             2026-10-21 12:00:00 UTC      audio: 48000 Hz\r\n"
        );
        assert!(now().ends_with(" UTC") && now().len() == 23, "{}", now());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
