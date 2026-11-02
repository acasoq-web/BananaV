//! Insert › Insert Subtitles from File / from Regions, as VEGAS's: title
//! events (Titles & Text) on a new track at the top, from a text file's
//! lines — a plain one gives each line five seconds from the project's
//! start; one with times gives "start, end, length, text" a line (tabs in
//! a `.txt`, commas in a `.csv`), the times in the ruler's format — or from
//! the regions' names, over the regions. The dialog's preset gives the
//! titles their look, and can be kept as the default.

use std::path::PathBuf;

use bv_core::Tick;
use bv_timeline::{
    Generator, MediaInfo, PluginParam, PluginValue, ProjectSettings, RulerFormat, TrackKind,
};

use crate::app::BananaApp;
use crate::egui::{self, RichText};
use crate::i18n::tr;
use crate::theme;
use crate::timefmt::RulerFormatExt;

/// A plain file's titles each last this long.
const PLAIN_LENGTH: Tick = Tick::from_seconds(5);

/// One subtitle: where it starts, how long it lasts and what it says.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Subtitle {
    pub start: Tick,
    pub length: Tick,
    pub text: String,
}

/// A line of a `.csv` file split at its commas, as RFC 4180 says: a field
/// that starts with a double quote may hold commas, two quotes standing
/// for one; elsewhere quotes are text.
fn csv_fields(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' if quoted => quoted = false,
            '"' if field.trim().is_empty() => {
                field.clear();
                quoted = true;
            }
            ',' if !quoted => out.push(std::mem::take(&mut field)),
            c => field.push(c),
        }
    }
    out.push(field);
    out
}

/// The subtitles in a file's `text`: "start, end, length, text" lines
/// (`csv`: commas, else tabs), the times in `format` (or timecode, or
/// seconds) — the lines that are not left out, as a heading — or, when
/// none has times, each line five seconds from the start.
pub(crate) fn parse(
    text: &str,
    csv: bool,
    format: RulerFormat,
    settings: &ProjectSettings,
) -> Vec<Subtitle> {
    let time = |s: &str| format.parse(s.trim(), settings);
    let mut timed = Vec::new();
    let mut plain = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<String> = if csv {
            csv_fields(line)
        } else {
            line.split('\t').map(str::to_string).collect()
        };
        let times = (fields.len() >= 4)
            .then(|| Some((time(&fields[0])?, time(&fields[1])?)))
            .flatten();
        match times {
            Some((start, end)) => {
                let length = if end > start {
                    end - start
                } else {
                    time(&fields[2]).unwrap_or(PLAIN_LENGTH)
                };
                let text = fields[3..].join(if csv { "," } else { "\t" });
                timed.push(Subtitle {
                    start,
                    length,
                    text: text.trim().to_string(),
                });
            }
            None => plain.push(line.trim().to_string()),
        }
    }
    if !timed.is_empty() {
        return timed;
    }
    plain
        .into_iter()
        .enumerate()
        .map(|(i, text)| Subtitle {
            start: Tick(PLAIN_LENGTH.0 * i as i64),
            length: PLAIN_LENGTH,
            text,
        })
        .collect()
}

/// The regions' names as subtitles, over the regions (unnamed ones left
/// out).
pub(crate) fn from_regions(p: &bv_timeline::Project) -> Vec<Subtitle> {
    let mut regions: Vec<&bv_timeline::Region> = p
        .regions
        .iter()
        .filter(|r| !r.name.trim().is_empty() && r.range.duration > Tick::ZERO)
        .collect();
    regions.sort_by_key(|r| r.range.start);
    regions
        .into_iter()
        .map(|r| Subtitle {
            start: r.range.start,
            length: r.range.duration,
            text: r.name.trim().to_string(),
        })
        .collect()
}

/// Puts `subs` on a new video track at the top: a Titles & Text event each,
/// `look`'s settings with the subtitle's text. Returns how many.
pub(crate) fn insert(app: &mut BananaApp, subs: &[Subtitle], look: &Generator) -> usize {
    let Generator::Plugin(base) = look else {
        return 0;
    };
    app.modify(|p| {
        let track = p.insert_track_at(TrackKind::Video, 0);
        let min = p.min_clip_duration();
        let mut n = 0;
        for s in subs {
            let mut fx = (**base).clone();
            fx.params.insert(
                "text".into(),
                PluginParam {
                    value: PluginValue::Text(s.text.clone()),
                    keys: Vec::new(),
                },
            );
            let info = MediaInfo::generated(Generator::Plugin(Box::new(fx)), &p.settings);
            let m = p.add_media(info);
            if let Ok(ids) = p.insert_media(m, s.start.max(Tick::ZERO), Some(track), None)
                && let Some(&id) = ids.first()
            {
                let _ = p.set_clip_duration(id, s.length.max(min));
                n += 1;
            }
        }
        n
    })
}

/// The Insert Subtitles dialog: from a file (its path) or from the
/// regions; the preset; keeping it as the default.
#[derive(Debug)]
pub struct SubtitlesDialog {
    from_file: bool,
    path: String,
    preset: String,
    remember: bool,
    error: Option<String>,
}

impl SubtitlesDialog {
    /// From a file (`true`) or from the regions; the preset last kept.
    pub fn new(app: &BananaApp, from_file: bool) -> SubtitlesDialog {
        SubtitlesDialog {
            from_file,
            path: String::new(),
            preset: app.settings.subtitle_preset.clone().unwrap_or_default(),
            remember: false,
            error: None,
        }
    }

    /// Shows the dialog; `false` once it is closed.
    pub fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        let presets = crate::tabs::generators::titles_presets_all(app);
        if presets.is_empty() {
            return false;
        }
        if !presets.iter().any(|(n, _)| *n == self.preset) {
            self.preset = presets[0].0.clone();
        }
        let title = if self.from_file {
            tr("Insert Subtitles")
        } else {
            tr("Insert Subtitles from Regions")
        };
        let (mut open, mut ok, mut cancel) = (true, false, false);
        crate::floating::Window::new(title)
            .id(egui::Id::new(("subtitles-dialog", self.from_file)))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                egui::Grid::new("subtitles-grid")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        if self.from_file {
                            ui.label(tr("File:"));
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.path).desired_width(300.0),
                                );
                                if ui.button(tr("Browse…")).clicked()
                                    && let Some(p) = rfd::FileDialog::new()
                                        .set_title(tr("Insert Subtitles"))
                                        .add_filter(tr("Text files"), &["txt", "csv"])
                                        .pick_file()
                                {
                                    self.path = p.display().to_string();
                                }
                            });
                            ui.end_row();
                        }
                        ui.label(tr("Preset:"));
                        egui::ComboBox::from_id_salt("subtitles-preset")
                            .width(300.0)
                            .selected_text(self.preset.clone())
                            .show_ui(ui, |ui| {
                                for (name, _) in &presets {
                                    ui.selectable_value(&mut self.preset, name.clone(), name);
                                }
                            });
                        ui.end_row();
                    });
                crate::widgets::checkbox(
                    ui,
                    &mut self.remember,
                    tr("Use current preset as default"),
                );
                if self.from_file {
                    crate::tabs::hint(
                        ui,
                        tr("A plain text file gives each line a title five seconds long, from the project's start. With times, each line holds the start, the end, the length and the text (tabs in a .txt file, commas in a .csv one), in the ruler's format."),
                    );
                } else {
                    crate::tabs::hint(
                        ui,
                        tr("Each named region becomes a title over it, its name the text."),
                    );
                }
                if let Some(e) = &self.error {
                    ui.label(RichText::new(e).color(theme::error()));
                }
                ui.horizontal(|ui| {
                    ok = ui.button("OK").clicked();
                    cancel = ui.button(tr("Cancel")).clicked();
                });
            });
        if ok {
            let subs = if self.from_file {
                match std::fs::read_to_string(PathBuf::from(self.path.trim())) {
                    Ok(text) => {
                        let csv = self.path.trim().to_lowercase().ends_with(".csv");
                        let p = app.project();
                        parse(&text, csv, app.effective_ruler_format(), &p.settings)
                    }
                    Err(e) => {
                        self.error = Some(trf!("The file could not be read: {}", e));
                        return true;
                    }
                }
            } else {
                from_regions(app.project())
            };
            if subs.is_empty() {
                self.error = Some(
                    if self.from_file {
                        tr("The file has no subtitles.")
                    } else {
                        tr("No region has a name.")
                    }
                    .to_string(),
                );
                return true;
            }
            let Some((_, look)) = presets.iter().find(|(n, _)| *n == self.preset) else {
                return true;
            };
            let n = insert(app, &subs, look);
            if self.remember {
                app.settings.subtitle_preset = Some(self.preset.clone());
                let _ = app.settings.save();
            }
            app.notify(trn!(
                n,
                "{} subtitle inserted.",
                "{} subtitles inserted.",
                n
            ));
            return false;
        }
        open && !cancel
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_core::FrameRate;

    fn settings() -> ProjectSettings {
        ProjectSettings {
            frame_rate: FrameRate::FPS_29_97,
            ..ProjectSettings::default()
        }
    }

    #[test]
    fn a_plain_file_gives_each_line_five_seconds() {
        let subs = parse(
            "Hello\r\n\r\nWorld\n",
            false,
            RulerFormat::SmpteDrop,
            &settings(),
        );
        assert_eq!(subs.len(), 2);
        assert_eq!((subs[0].start, subs[0].length), (Tick::ZERO, PLAIN_LENGTH));
        assert_eq!(subs[1].start, PLAIN_LENGTH);
        assert_eq!(subs[1].text, "World");
    }

    #[test]
    fn timed_lines_take_their_times_in_the_ruler_format() {
        let s = settings();
        let tabs = "00:00:01;00\t00:00:03;15\t00:00:02;15\tFirst line\n\
                    00:00:05;00\t00:00:05;00\t00:00:01;00\tSecond";
        let subs = parse(tabs, false, RulerFormat::SmpteDrop, &s);
        assert_eq!(subs.len(), 2);
        let rate = s.frame_rate;
        assert_eq!(rate.frame_at(subs[0].start), 30);
        assert_eq!(rate.frame_at(subs[0].length), 75);
        // The end at the start: the length's field.
        assert_eq!(rate.frame_at(subs[1].length), 30);
        // CSV, a heading line, quotes and commas in the text.
        let csv = "Start,End,Length,Text\n\
                   1.000,2.500,1.500,\"Hello, world\"\n\
                   3,4,1,\"She said \"\"hi\"\"\"
                   5,6,1,A \"quote\" inside";
        let subs = parse(csv, true, RulerFormat::Seconds, &s);
        assert_eq!(subs.len(), 3);
        assert_eq!(subs[0].text, "Hello, world");
        assert_eq!(subs[0].length, Tick::from_seconds_f64(1.5));
        assert_eq!(subs[1].text, "She said \"hi\"");
        assert_eq!(subs[2].text, "A \"quote\" inside");
    }

    #[test]
    fn regions_become_subtitles_in_order() {
        let mut p = bv_timeline::Project::default();
        p.add_region(bv_core::TimeRange::new(
            Tick::from_seconds(8),
            Tick::from_seconds(2),
        ));
        p.add_region(bv_core::TimeRange::new(
            Tick::from_seconds(1),
            Tick::from_seconds(3),
        ));
        for r in &mut p.regions {
            r.name = if r.range.start == Tick::from_seconds(8) {
                "Later".into()
            } else {
                "First".into()
            };
        }
        let blank = p.add_region(bv_core::TimeRange::new(
            Tick::from_seconds(12),
            Tick::from_seconds(1),
        ));
        p.rename_marker(blank, String::new());
        let subs = from_regions(&p);
        // In time order; the unnamed one left out.
        assert_eq!(
            subs.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(),
            ["First", "Later"]
        );
        assert_eq!(subs[0].length, Tick::from_seconds(3));
    }
}
