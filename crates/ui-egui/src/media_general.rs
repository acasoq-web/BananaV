//! A media's Properties' General tab, as VEGAS's (the user's prints): what
//! its file is — its name, folder, type, size, dates and attributes —, its
//! streams as the file describes them (formats, sizes, bits, rates, bit
//! rates and lengths, from an MPEG-4 family file's boxes), what else it
//! holds, its tags, and what reads it, in a list that scrolls. Worked out
//! once, when the tab first shows.

use bv_core::{FieldOrder, Tick};
use bv_engine::{FileDescription, FileReader, TrackFacts};
use bv_timeline::{MediaInfo, MediaSource, Project};

use crate::egui::{self, RichText, Stroke, Ui};
use crate::i18n::tr;
use crate::theme;

/// The tab's text (`about`: what the file is, read when the window opened):
/// sections, their lines stepped in under them.
pub(crate) fn general_text(
    project: &Project,
    m: &MediaInfo,
    about: Option<&FileDescription>,
) -> String {
    let mut out = Text::default();
    let yes_no = |b: bool| if b { tr("yes") } else { tr("no") };
    let path = match &m.source {
        MediaSource::File { path } => Some(std::path::Path::new(path)),
        MediaSource::Generated(_) => None,
    };
    let video_track = about.and_then(|d| d.tracks.iter().find(|t| &t.handler == b"vide"));
    let audio_tracks: Vec<&TrackFacts> = about
        .map(|d| d.tracks.iter().filter(|t| &t.handler == b"soun").collect())
        .unwrap_or_default();

    // The file.
    out.line(0, tr("General"));
    match path {
        Some(p) => {
            let name = p
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into());
            let folder = p
                .parent()
                .map_or_else(String::new, |f| f.display().to_string());
            out.line(1, &trf!("Name: {}", name));
            out.line(1, &trf!("Folder: {}", folder));
            if let Some(d) = about {
                // The MPEG-4 family's files are ISO base media files.
                let kind = if d.brand.is_some() {
                    "ISO Base".to_string()
                } else {
                    d.format.clone()
                };
                if !kind.is_empty() {
                    out.line(1, &trf!("Type: {}", kind));
                }
            }
            if let Ok(meta) = std::fs::metadata(p) {
                let size = meta.len();
                out.line(
                    1,
                    &trf!(
                        "Size: {0} ({1} bytes)",
                        size_text(size),
                        crate::fmt::grouped(size)
                    ),
                );
                let offset = bv_platform::utc_offset();
                let when = |t: std::io::Result<std::time::SystemTime>| {
                    let t = t.ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
                    Some(crate::fmt::file_date_time(t.as_secs() as i64, offset))
                };
                if let Some(t) = when(meta.created()) {
                    out.line(1, &trf!("Created: {}", t));
                }
                if let Some(t) = when(meta.modified()) {
                    out.line(1, &trf!("Modified: {}", t));
                }
                if let Some(t) = when(meta.accessed()) {
                    out.line(1, &trf!("Accessed: {}", t));
                }
                if let Some(a) = attributes(&meta) {
                    out.line(1, &trf!("Attributes: {}", a));
                }
            }
        }
        None => {
            out.line(1, &trf!("Name: {}", crate::i18n::media_name(m)));
            out.line(1, &trf!("Type: {}", tr("Generated media")));
        }
    }

    // The streams, as the file describes them (its numbers as they are).
    out.gap();
    out.line(0, tr("Media information"));
    if let Some(d) = about.filter(|d| !d.format.is_empty()) {
        out.line(1, &trf!("Stream format: {}", d.format));
    }
    let video_codec = about.and_then(|d| d.video_codec.as_deref()).map(codec_name);
    if let Some(v) = m.video {
        let (w, h) = v.coded_size();
        out.line(
            1,
            &if m.still {
                tr("Picture").to_string()
            } else {
                trf!("Video stream #{}", 1)
            },
        );
        if let Some(c) = &video_codec {
            out.line(2, &trf!("Video format: {}", c));
        } else if m.still
            && let Some(d) = about
        {
            out.line(2, &trf!("Video format: {}", d.format));
        }
        out.line(2, &trf!("Resolution: {0} x {1} px", w, h));
        out.line(
            2,
            &trf!(
                "Aspect ratio: {}",
                aspect(
                    u64::from(w) * u64::from(v.pixel_aspect.num),
                    u64::from(h) * u64::from(v.pixel_aspect.den)
                )
            ),
        );
        if let Some(bits) = video_track.and_then(|t| t.bit_depth) {
            out.line(2, &trf!("Color depth: {} bit", bits));
        }
        if !m.still {
            out.line(
                2,
                &trf!(
                    "Frame rate: {} fps",
                    format!("{:.3}", v.frame_rate.as_f64())
                ),
            );
            out.line(2, &trf!("Scan type: {}", scan(v.field_order)));
            if let Some(bps) = video_track.and_then(|t| t.bit_rate()) {
                out.line(2, &trf!("Bit rate: {} bps", bps));
            }
        }
    }
    let audio: Vec<_> = m.audio.iter().chain(&m.extra_audio).copied().collect();
    for (k, a) in audio.iter().enumerate() {
        let track = audio_tracks.get(k);
        out.line(1, &trf!("Audio stream #{}", k + 1));
        if let Some(c) = about.and_then(|d| d.audio_codecs.get(k)) {
            out.line(2, &trf!("Audio format: {}", c));
        }
        out.line(2, &trf!("Sampling rate: {} Hz", a.sample_rate.0));
        out.line(
            2,
            &trf!(
                "Channels: {}",
                trn!(a.channels, "{} channel", "{} channels", a.channels)
            ),
        );
        if let Some(constant) = track.and_then(|t| t.constant_rate()) {
            let mode = if constant {
                tr("Constant")
            } else {
                tr("Variable")
            };
            out.line(2, &trf!("Bit rate mode: {}", mode));
        }
        if let Some(bps) = track.and_then(|t| t.bit_rate()) {
            out.line(2, &trf!("Bit rate: {} bps", bps));
        }
    }

    // As the events read them (the numbers in the interface's language).
    out.gap();
    out.line(0, tr("Streams"));
    let length = |seconds: Option<f64>| clock(seconds.map_or(m.duration, Tick::from_seconds_f64));
    if let Some(v) = m.video.filter(|_| !m.still) {
        let (w, h) = v.coded_size();
        let fields = match v.field_order {
            FieldOrder::Progressive => tr("progressive"),
            _ => tr("interlaced"),
        };
        let mut parts = vec![
            length(video_track.map(|t| t.seconds).filter(|s| *s > 0.0)),
            format!("{} fps {fields}", crate::fmt::num(v.frame_rate.as_f64(), 3)),
            format!("{w}x{h}x32"),
        ];
        parts.extend(video_codec.clone());
        out.line(1, &trf!("Video: {}", parts.join(", ")));
    } else if let Some(v) = m.video {
        let (w, h) = v.coded_size();
        out.line(1, &trf!("Picture: {}", format!("{w}x{h}x32")));
    }
    for (k, a) in audio.iter().enumerate() {
        let track = audio_tracks.get(k);
        let mut parts = vec![
            length(track.map(|t| t.seconds).filter(|s| *s > 0.0)),
            format!("{} Hz", a.sample_rate.0),
        ];
        if let Some(bits) = track.map(|t| t.sample_size).filter(|&b| b > 0) {
            parts.push(trf!("{} Bits", bits));
        }
        parts.push(channels(a.channels));
        parts.extend(about.and_then(|d| d.audio_codecs.get(k).cloned()));
        out.line(1, &trf!("Audio: {}", parts.join(", ")));
    }

    // A Wave file's loop chunk.
    out.gap();
    out.line(0, tr("Loop information"));
    out.line(
        1,
        &trf!(
            "Loop chunk: {}",
            yes_no(about.is_some_and(|d| d.loop_chunk))
        ),
    );

    // What else it holds.
    out.gap();
    out.line(0, tr("Other metadata"));
    out.line(
        1,
        &trf!("Regions/markers: {}", yes_no(!m.markers.is_empty())),
    );
    out.line(1, &trf!("Command markers: {}", yes_no(false)));

    // Its tags (Project Media's).
    out.gap();
    out.line(0, tr("Media manager"));
    let tagged = project
        .media_tags
        .iter()
        .any(|t| t.media == m.id && !t.tags.is_empty());
    out.line(1, &trf!("Media tags: {}", yes_no(tagged)));

    // What reads it.
    if let Some(d) = about
        && let Some(&reader) = d.readers.first()
    {
        out.gap();
        out.line(0, tr("Plug-In"));
        let module = reader_module(reader, d);
        let version = module
            .path
            .as_deref()
            .and_then(bv_platform::version_texts)
            .unwrap_or_default();
        out.line(1, &trf!("Name: {}", module.name));
        if let Some(folder) = module.path.as_deref().and_then(|p| p.parent()) {
            out.line(1, &trf!("Folder: {}", folder.display()));
        }
        out.line(1, &trf!("Format: {}", module.format));
        if let Some(v) = module.version.or(version.version) {
            out.line(1, &trf!("Version: {}", v));
        }
        if let Some(c) = module.company.or(version.company) {
            out.line(1, &trf!("Company: {}", c));
        }
        // ProRes and AC-3: BananaV's own decoders on the system's packets.
        if d.readers.contains(&FileReader::Own) && reader != FileReader::Own {
            out.line(1, &trf!("Decoder: {}", "BananaV"));
        }
    }
    out.0
}

/// The tab's lines.
#[derive(Default)]
struct Text(String);

impl Text {
    fn line(&mut self, indent: usize, text: &str) {
        self.0.push_str(&" ".repeat(indent * 2));
        self.0.push_str(text);
        self.0.push('\n');
    }

    fn gap(&mut self) {
        self.0.push('\n');
    }
}

/// The tab: the file's properties in a list that scrolls, under its label.
pub(crate) fn general_tab(ui: &mut Ui, text: &str, height: f32) {
    ui.label(tr("File properties:"));
    egui::Frame::new()
        .fill(theme::panel_dark())
        .stroke(Stroke::new(1.0, theme::separator()))
        .inner_margin(6.0)
        .show(ui, |ui| {
            egui::ScrollArea::both()
                .max_height(height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(text).size(12.0)).extend());
                });
        });
}

/// What reads a file, as the tab's Plug-In section names it.
struct Module {
    name: String,
    /// Its library or program, whose version resource tells the rest.
    path: Option<std::path::PathBuf>,
    format: String,
    version: Option<String>,
    company: Option<String>,
}

/// The module `reader` is for file `d`: BananaV itself, or the system's
/// library that reads files of its kind.
fn reader_module(reader: FileReader, d: &FileDescription) -> Module {
    let format = match (&d.video_codec, d.audio_codecs.first()) {
        (Some(v), _) => format!("{} - {}", d.format, codec_name(v)),
        (None, Some(a)) => format!("{} - {a}", d.format),
        _ => d.format.clone(),
    };
    let system = |dll: &str| {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        let path = std::path::Path::new(&root).join("System32").join(dll);
        Module {
            name: dll.to_string(),
            path: path.is_file().then_some(path),
            format: format.clone(),
            version: None,
            company: None,
        }
    };
    match reader {
        FileReader::Own => {
            let exe = std::env::current_exe().ok();
            Module {
                name: exe
                    .as_deref()
                    .and_then(|p| p.file_name())
                    .map_or_else(|| "BananaV".into(), |n| n.to_string_lossy().into()),
                path: exe,
                format,
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
                company: Some("BananaV".into()),
            }
        }
        FileReader::SystemPictures if cfg!(windows) => system("WindowsCodecs.dll"),
        FileReader::SystemMedia if cfg!(windows) => system(match d.format.as_str() {
            "MPEG-4" | "QuickTime" => "mfmp4srcsnk.dll",
            "Matroska" | "WebM" => "mfmkvsrcsnk.dll",
            "Windows Media" => "mfasfsrcsnk.dll",
            "MPEG program stream" | "MPEG transport stream" => "mfmpeg2srcsnk.dll",
            _ => "mfsrcsnk.dll",
        }),
        _ => Module {
            name: "FFmpeg".into(),
            path: None,
            format,
            version: None,
            company: None,
        },
    }
}

/// A compressed format as VEGAS names it ("AVC" for H.264).
pub(crate) fn codec_name(c: &str) -> String {
    match c {
        "H.264" => "AVC".into(),
        c => c.to_string(),
    }
}

/// An MPEG-4 family file's major brand, as the file type registry
/// describes it ("MP4 Base Media v1" for `isom`).
pub(crate) fn brand_name(brand: [u8; 4]) -> String {
    let code = String::from_utf8_lossy(&brand).trim().to_string();
    match &brand {
        b"isom" => "MP4 Base Media v1".into(),
        b"iso2" | b"iso3" | b"iso4" | b"iso5" | b"iso6" | b"iso7" | b"iso8" | b"iso9" => {
            format!("MP4 Base Media v{}", brand[3] as char)
        }
        b"mp41" => "MP4 v1".into(),
        b"mp42" => "MP4 v2".into(),
        b"avc1" => "MP4 Base w/ AVC ext".into(),
        b"qt  " => "QuickTime".into(),
        b"M4V " | b"M4VH" | b"M4VP" => "Apple iTunes Video".into(),
        b"M4A " => "Apple iTunes AAC-LC Audio".into(),
        b"3gp4" | b"3gp5" | b"3gp6" | b"3gp7" => format!("3GPP Release {}", brand[3] as char),
        b"3g2a" | b"3g2b" | b"3g2c" => "3GPP2".into(),
        b"mmp4" => "3GPP/3GPP2 Mobile MP4".into(),
        b"f4v " => "F4V".into(),
        b"dash" => "MPEG-DASH".into(),
        _ => format!("MPEG-4 ({code})"),
    }
}

/// A size as VEGAS writes it ("11,10 MB": kilobytes of 1024 bytes, then
/// thousands of them).
fn size_text(bytes: u64) -> String {
    let kb = bytes as f64 / 1024.0;
    if kb < 1000.0 {
        format!("{} KB", crate::fmt::num(kb, 2))
    } else if kb < 1_000_000.0 {
        format!("{} MB", crate::fmt::num(kb / 1000.0, 2))
    } else {
        format!("{} GB", crate::fmt::num(kb / 1_000_000.0, 2))
    }
}

/// A file's attributes, as Windows keeps them ("Archive", "Read-only"…).
#[cfg(windows)]
fn attributes(meta: &std::fs::Metadata) -> Option<String> {
    use std::os::windows::fs::MetadataExt;
    let a = meta.file_attributes();
    let names: Vec<&str> = [
        (0x1, tr("Read-only")),
        (0x2, tr("Hidden")),
        (0x4, tr("System")),
        (0x20, tr("Archive")),
        (0x800, tr("Compressed")),
        (0x4000, tr("Encrypted")),
    ]
    .into_iter()
    .filter(|(bit, _)| a & bit != 0)
    .map(|(_, n)| n)
    .collect();
    Some(if names.is_empty() {
        tr("Normal").to_string()
    } else {
        names.join(", ")
    })
}

#[cfg(not(windows))]
fn attributes(meta: &std::fs::Metadata) -> Option<String> {
    meta.permissions()
        .readonly()
        .then(|| tr("Read-only").to_string())
}

/// A picture's shape, `w` : `h` reduced ("16:9"; past small numbers, its
/// width to 1).
fn aspect(w: u64, h: u64) -> String {
    let gcd = |mut a: u64, mut b: u64| {
        while b != 0 {
            (a, b) = (b, a % b);
        }
        a.max(1)
    };
    let g = gcd(w, h);
    let (a, b) = (w / g, h / g);
    if a <= 64 && b <= 64 {
        format!("{a}:{b}")
    } else {
        format!("{:.3}:1", w as f64 / h.max(1) as f64)
    }
}

/// A video's scan type.
fn scan(order: FieldOrder) -> &'static str {
    match order {
        FieldOrder::Progressive => tr("Progressive"),
        FieldOrder::UpperFirst => tr("Interlaced (upper field first)"),
        FieldOrder::LowerFirst => tr("Interlaced (lower field first)"),
    }
}

fn channels(n: u16) -> String {
    match n {
        1 => tr("Mono").to_string(),
        2 => tr("Stereo").to_string(),
        n => trn!(n, "{} channel", "{} channels", n),
    }
}

/// A length as hours, minutes, seconds and milliseconds ("00:00:08,000").
fn clock(t: Tick) -> String {
    let ms = (t.as_seconds_f64() * 1000.0).round().max(0.0) as u64;
    let sep = crate::i18n::language().decimal_separator();
    format!(
        "{:02}:{:02}:{:02}{sep}{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_lengths_sizes_and_names() {
        assert_eq!(aspect(1920, 1080), "16:9");
        assert_eq!(aspect(1440 * 4, 1080 * 3), "16:9");
        assert_eq!(aspect(720, 480), "3:2");
        assert!(clock(Tick::from_seconds_f64(8.0435)).starts_with("00:00:08"));
        assert!(clock(Tick::from_seconds_f64(3725.5)).ends_with("500"));
        // VEGAS's 11,10 MB for 11.368.635 bytes.
        assert!(
            size_text(11_368_635).starts_with("11"),
            "{}",
            size_text(11_368_635)
        );
        assert_eq!(brand_name(*b"isom"), "MP4 Base Media v1");
        assert_eq!(brand_name(*b"iso5"), "MP4 Base Media v5");
        assert_eq!(codec_name("H.264"), "AVC");
    }

    /// A video file's tab (the streams test's file, when made): its
    /// streams as its boxes describe them, what reads it.
    #[test]
    #[ignore = "needs test-media/generated/dois_streams_de_audio.mp4"]
    fn a_video_file_s_tab() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test-media/generated/dois_streams_de_audio.mp4"
        );
        let mut info = bv_engine::Engine::probe_file(path).expect("the test file");
        info.id = bv_timeline::MediaId(1);
        let about = bv_engine::describe_file(path);
        let text = general_text(&Project::default(), &info, Some(&about));
        eprintln!("{text}");
        for line in [
            "Type: ISO Base",
            "Stream format: MPEG-4",
            "Video format: AVC",
            "Color depth: 8 bit",
            "Bit rate: ",
            "Bit rate mode: ",
            "16 Bits",
            "Plug-In",
            "Company: ",
        ] {
            assert!(text.contains(line), "{line}: {text}");
        }
    }

    /// A generated media's tab: its name, its picture's streams.
    #[test]
    fn a_generated_media_s_tab() {
        let p = Project::default();
        let m = MediaInfo::generated(bv_timeline::Generator::ColorBars, &p.settings);
        let text = general_text(&p, &m, None);
        assert!(
            text.contains("Media information") && text.contains("Resolution: "),
            "{text}"
        );
        assert!(text.contains("Regions/markers: no"), "{text}");
        assert!(text.contains("Media manager"), "{text}");
    }
}
