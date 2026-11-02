//! Closed captions in the interface (VEGAS's Help: "Adding closed
//! captioning to video files"): File › Import › Closed Captioning reads a
//! captioning file into caption command markers
//! (`bv_timeline::caption_files`), Tools › Scripting's exports write each
//! channel's captions as SubRip, SAMI, QuickTime text or DVD subtitles,
//! Promote Media Closed Captioning places the captions of the selected
//! events' media — a Scenarist or MacCaption file beside the media file,
//! as VEGAS reads them ("wildflowers.avi.scc") — where the events show
//! them, and the Video Preview's Overlays draw the channels turned on as a
//! CEA-608 decoder shows them: 15 rows of 32 columns in the middle 80 %
//! of a 4:3 picture as tall as the frame, each character in its cell on
//! its background (CEA-608's own: opaque black).

use std::path::{Path, PathBuf};

use bv_core::Tick;
use bv_timeline::caption_files::{self, EXTENSIONS};
use bv_timeline::captions::{COLUMNS, Caption, CaptionColor, CaptionScreen, ROWS, pop_on};

use crate::app::BananaApp;
use crate::egui::{self, Color32, FontId, Rect, Stroke, pos2, vec2};
use crate::i18n::tr;

/// The formats Tools › Scripting writes captions as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptionFormat {
    /// `.srt` (VEGAS's "for YouTube").
    SubRip,
    /// `.smi` with an `.asx` playlist (VEGAS's "for Windows Media
    /// Player").
    Sami,
    /// `.txt` with a `.smil` presentation (VEGAS's "for QuickTime").
    QuickTime,
    /// `.sub`, a number, two timecodes and the text a line.
    DvdSubtitles,
}

impl CaptionFormat {
    fn extension(self) -> &'static str {
        match self {
            CaptionFormat::SubRip => "srt",
            CaptionFormat::Sami => "smi",
            CaptionFormat::QuickTime => "txt",
            CaptionFormat::DvdSubtitles => "sub",
        }
    }

    fn filter(self) -> &'static str {
        match self {
            CaptionFormat::SubRip => "SubRip",
            CaptionFormat::Sami => "SAMI",
            CaptionFormat::QuickTime => "QuickTime Text",
            CaptionFormat::DvdSubtitles => tr("DVD Subtitles"),
        }
    }
}

impl BananaApp {
    /// File › Import › Closed Captioning: a captioning file's captions as
    /// command markers, placed by its timecodes less the ruler's offset.
    pub(crate) fn import_closed_captioning(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr("Import Closed Captioning"))
            .add_filter(tr("Closed Captioning Files"), &EXTENSIONS)
            .pick_file()
        else {
            return;
        };
        self.import_captions_file(&path);
    }

    /// The captions of the file at `path` added to the project: how many.
    pub(crate) fn import_captions_file(&mut self, path: &Path) -> usize {
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                self.notify(trf!("Could not read {}: {}", name, e));
                return 0;
            }
        };
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let text = caption_files::decode(&bytes);
        let captions = caption_files::read(ext, &text, self.frame_rate());
        if captions.is_empty() {
            self.notify(trf!("No captions found in {}.", name));
            return 0;
        }
        let n = self.modify(|p| p.add_captions(&captions));
        self.notify(trn!(n, "{} caption imported.", "{} captions imported.", n));
        n
    }

    /// Tools › Scripting › Promote Media Closed Captioning: the captions of
    /// the selected events' media files (`media_captions`) as caption
    /// markers where each event shows them (one undo); those there already
    /// stay once. How many were added.
    pub(crate) fn promote_media_captions(&mut self) -> usize {
        if self.selection.is_empty() {
            self.notify(tr("Select events with captions."));
            return 0;
        }
        let p = self.project();
        let have = p.captions();
        let offset = p.ruler_offset;
        let mut add: Vec<Caption> = Vec::new();
        for &id in &self.selection {
            let Some(c) = p.clip(id) else {
                continue;
            };
            let Some(bv_timeline::MediaSource::File { path }) = p.media(c.media).map(|m| &m.source)
            else {
                continue;
            };
            // Events that play their media straight through.
            if c.reverse.is_some() || c.freeze.is_some() || c.velocity().is_some() {
                continue;
            }
            for cap in media_captions(Path::new(path)) {
                if cap.time < c.source_in || cap.time > c.source_end() {
                    continue;
                }
                let at = c.start + c.to_timeline(cap.time - c.source_in);
                let new = Caption {
                    time: at,
                    channel: cap.channel,
                    markup: cap.markup,
                };
                let there = have.iter().chain(&add).any(|h| {
                    (h.time, h.channel, &h.markup) == (new.time, new.channel, &new.markup)
                });
                if !there {
                    add.push(new);
                }
            }
        }
        if add.is_empty() {
            self.notify(tr("The selected events' media have no captions."));
            return 0;
        }
        // `add_captions` takes the ruler's times.
        for c in &mut add {
            c.time += offset;
        }
        let n = self.modify(|p| p.add_captions(&add));
        self.notify(trn!(n, "{} caption promoted.", "{} captions promoted.", n));
        n
    }

    /// Tools › Scripting's caption exports: each channel with captions as
    /// a `format` file, the first at the name picked, the others after
    /// their channel ("Captions_CC3.srt").
    pub(crate) fn export_closed_captioning(&mut self, format: CaptionFormat) {
        if self.project().caption_channels().is_empty() {
            self.notify(tr("The project has no closed captions."));
            return;
        }
        let ext = format.extension();
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr("Export Closed Captioning"))
            .add_filter(format.filter(), &[ext])
            .set_file_name(format!("{}.{ext}", tr("Captions")))
            .save_file()
        else {
            return;
        };
        match self.write_captions(format, &path) {
            Ok(files) => {
                let names: Vec<String> = files.iter().map(|f| f.display().to_string()).collect();
                self.notify(trf!("Saved to {}", names.join(", ")));
            }
            Err(e) => self.report_save_error(&path, &e),
        }
    }

    /// Writes the project's captions as `format` at `path` (a channel a
    /// file, the second on named after its channel): the files written.
    pub(crate) fn write_captions(
        &self,
        format: CaptionFormat,
        path: &Path,
    ) -> std::io::Result<Vec<PathBuf>> {
        let p = self.project();
        // The last caption shows for five seconds when the project ends
        // before it.
        let last = p
            .commands
            .iter()
            .filter(|c| bv_timeline::captions::channel_of(&c.command).is_some())
            .map(|c| c.time)
            .max()
            .unwrap_or(Tick::ZERO);
        let end = p.duration().max(last + Tick::from_seconds(5));
        let (offset, rate) = (p.ruler_offset, p.settings.frame_rate);
        let stem = path.file_stem().map_or_else(
            || "Captions".to_string(),
            |s| s.to_string_lossy().into_owned(),
        );
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut files = Vec::new();
        for (k, ch) in p.caption_channels().into_iter().enumerate() {
            let spans = p.caption_spans(ch, end);
            let class = format!("CC{}", ch + 1);
            let base = if k == 0 {
                stem.clone()
            } else {
                format!("{stem}_{class}")
            };
            let mut write = |ext: &str, text: String| -> std::io::Result<String> {
                let name = format!("{base}.{ext}");
                let file = dir.join(&name);
                std::fs::write(&file, text)?;
                files.push(file);
                Ok(name)
            };
            match format {
                CaptionFormat::SubRip => {
                    write("srt", caption_files::write_srt(&spans, offset))?;
                }
                CaptionFormat::Sami => {
                    let smi = write(
                        "smi",
                        caption_files::write_smi(&spans, offset, &base, &class),
                    )?;
                    write(
                        "asx",
                        caption_files::write_asx(&format!("{base}.wmv"), &smi),
                    )?;
                }
                CaptionFormat::QuickTime => {
                    let txt = write("txt", caption_files::write_qt_text(&spans, offset))?;
                    write(
                        "smil",
                        caption_files::write_smil(&format!("{base}.mov"), &txt),
                    )?;
                }
                CaptionFormat::DvdSubtitles => {
                    write("sub", caption_files::write_sub(&spans, offset, rate))?;
                }
            }
        }
        Ok(files)
    }

    /// Captions to look at (screen tests): a pop-on caption of two rows on
    /// CC1 at 1 s, one painted on at the top with a color change on CC3 at
    /// 2 s, the overlays of both on, the cursor at 3 s and Edit Details
    /// showing the commands.
    pub fn captions_for_review(&mut self) {
        let s = Tick::from_seconds;
        let captions = [
            Caption {
                time: s(1),
                channel: 0,
                markup: pop_on("Closed captions over the video,[BR]as a decoder shows them"),
            },
            Caption {
                time: s(2),
                channel: 2,
                markup: "{RDC}{R02In04Wh}{TabOff1}Second language: {YlTxt}yellow".into(),
            },
        ];
        self.modify(|p| {
            let offset = p.ruler_offset;
            let shifted: Vec<Caption> = captions
                .iter()
                .map(|c| Caption {
                    time: c.time + offset,
                    ..c.clone()
                })
                .collect();
            p.add_captions(&shifted)
        });
        self.preview.captions = [true, false, true, false];
        self.preview.overlays = true;
        self.details.show = crate::tabs::details::Show::Commands;
        self.seek(s(3));
    }
}

/// Draws the captions `screen` shows over the frame at `frame`, as a
/// CEA-608 decoder shows them.
pub(crate) fn paint_captions(painter: &egui::Painter, frame: Rect, screen: &CaptionScreen) {
    let h = frame.height() * 0.8;
    let w = (frame.height() * 4.0 / 3.0).min(frame.width()) * 0.8;
    let grid = Rect::from_center_size(frame.center(), vec2(w, h));
    let (cw, ch) = (w / COLUMNS as f32, h / ROWS as f32);
    let font = FontId::monospace(ch * 0.8);
    for row in screen {
        let y = grid.top() + (row.row.max(1) - 1) as f32 * ch;
        for run in &row.runs {
            let x = grid.left() + run.column as f32 * cw;
            let n = run.text.chars().count() as f32;
            let cells = Rect::from_min_size(pos2(x, y), vec2(n * cw, ch));
            let s = run.style;
            if !s.transparent {
                let (c, semi) = s.background.unwrap_or((CaptionColor::Black, false));
                let [r, g, b] = c.rgb();
                let alpha = if semi { 128 } else { 255 };
                painter.rect_filled(cells, 0.0, Color32::from_rgba_unmultiplied(r, g, b, alpha));
            }
            let [r, g, b] = s.color.rgb();
            let color = Color32::from_rgb(r, g, b);
            // Each character in its cell, the decoder's fixed grid.
            for (k, c) in run.text.chars().enumerate() {
                if c == ' ' {
                    continue;
                }
                let mut job = egui::text::LayoutJob::default();
                job.append(
                    &c.to_string(),
                    0.0,
                    egui::TextFormat {
                        font_id: font.clone(),
                        color,
                        italics: s.italic,
                        ..Default::default()
                    },
                );
                let galley = painter.layout_job(job);
                let at = pos2(
                    x + (k as f32 + 0.5) * cw - galley.size().x / 2.0,
                    y + (ch - galley.size().y) / 2.0,
                );
                painter.galley(at, galley, color);
            }
            if s.underline {
                let line_y = cells.bottom() - ch * 0.12;
                painter.hline(
                    cells.x_range(),
                    line_y,
                    Stroke::new((ch * 0.06).max(1.0), color),
                );
            }
        }
    }
}

/// The captions of media file `path` in its own time: a Scenarist or
/// MacCaption file beside it — `name.ext.scc`, `name.scc`, the same with
/// `.mcc` — as VEGAS looks for them.
pub(crate) fn media_captions(path: &Path) -> Vec<Caption> {
    for ext in ["scc", "mcc"] {
        let mut beside = path.as_os_str().to_owned();
        beside.push(format!(".{ext}"));
        for file in [PathBuf::from(beside), path.with_extension(ext)] {
            if let Ok(bytes) = std::fs::read(&file) {
                let text = caption_files::decode(&bytes);
                return caption_files::read(ext, &text, bv_core::FrameRate::FPS_29_97);
            }
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Promote Media Closed Captioning: the captions of a Scenarist file
    /// beside an event's media go where the event shows them, those
    /// outside its part left out, once however often it is run.
    #[test]
    fn media_captions_are_promoted_where_events_show_them() {
        let dir = temp_dir("promote");
        let media = dir.join("clip.mov");
        std::fs::write(&media, b"not read").unwrap();
        let rate = bv_core::FrameRate::FPS_29_97;
        let captions = [
            Caption {
                time: rate.frame_start(60),
                channel: 0,
                markup: pop_on("Early"),
            },
            Caption {
                time: rate.frame_start(150),
                channel: 0,
                markup: pop_on("Inside"),
            },
        ];
        let scc = caption_files::write_scc(&captions, Tick::ZERO);
        std::fs::write(dir.join("clip.mov.scc"), scc).unwrap();
        assert_eq!(media_captions(&media).len(), 2);
        let mut app = BananaApp::new(egui::Context::default());
        let mut info = bv_timeline::MediaInfo::generated(
            bv_timeline::Generator::ColorBars,
            &app.project().settings,
        );
        info.source = bv_timeline::MediaSource::File {
            path: media.display().to_string(),
        };
        info.duration = Tick::from_seconds(20);
        let m = app.modify(|p| p.add_media(info));
        let track = app.modify(|p| p.add_track(bv_timeline::TrackKind::Video));
        // The event shows the media from 3 s on, at 10 s.
        let clip = app.modify(|p| {
            let id = p
                .insert_media(m, Tick::from_seconds(10), Some(track), None)
                .unwrap()[0];
            let c = p.clip_mut(id).unwrap();
            c.source_in = Tick::from_seconds(3);
            c.duration = Tick::from_seconds(5);
            id
        });
        app.selection = [clip].into();
        assert_eq!(app.promote_media_captions(), 1, "the early one is outside");
        let placed = app.project().captions();
        let want = Tick::from_seconds(10) + (rate.frame_start(150) - Tick::from_seconds(3));
        assert_eq!(placed.len(), 1);
        assert_eq!(
            (placed[0].time, &placed[0].markup),
            (want, &pop_on("Inside"))
        );
        assert_eq!(app.promote_media_captions(), 0, "once");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("bananav-captions-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn caption_files_come_in_as_markers_and_go_out_by_channel() {
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut app = BananaApp::new(egui::Context::default());
        let dir = temp_dir("io");
        let srt = dir.join("In.srt");
        std::fs::write(&srt, "1\n00:00:01,000 --> 00:00:02,500\nHello\nthere\n").unwrap();
        assert_eq!(app.import_captions_file(&srt), 2, "the caption and its end");
        let c = &app.project().commands;
        assert_eq!(c[0].command, "608CC1");
        assert_eq!(c[0].parameter, "Hello[BR]there");
        // A second channel: each written to a file of its own.
        app.modify(|p| p.add_command(Tick::from_seconds(3), "608CC3", "", "Hola"));
        let files = app
            .write_captions(CaptionFormat::Sami, &dir.join("Out.smi"))
            .unwrap();
        let names: Vec<String> = files
            .iter()
            .map(|f| f.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["Out.smi", "Out.asx", "Out_CC3.smi", "Out_CC3.asx"]);
        let smi = std::fs::read_to_string(dir.join("Out.smi")).unwrap();
        assert!(smi.contains("<P Class=CC1>Hello<br>there</P>"), "{smi}");
        let asx = std::fs::read_to_string(dir.join("Out.asx")).unwrap();
        assert!(asx.contains("Out.wmv?sami=Out.smi"));
        let cc3 = std::fs::read_to_string(dir.join("Out_CC3.smi")).unwrap();
        assert!(cc3.contains("Hola"));
        // An unreadable file adds nothing.
        std::fs::write(dir.join("Empty.srt"), "nothing here").unwrap();
        assert_eq!(app.import_captions_file(&dir.join("Empty.srt")), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
