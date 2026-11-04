//! An event's Properties (its shortcut menu's Properties…), as VEGAS's
//! (the user's prints; VEGAS's Help, "Editing event properties"): the
//! Video Event tab — the active take's name, the switches, the playback
//! and undersample rates — or the Audio Event tab — the take's name, the
//! switches, the time stretch and pitch —, then the Media tab (its media's
//! Properties, `media_properties.rs`) and the General tab (what its file
//! is, `media_general.rs`). OK makes every change one step of the undo
//! history; Cancel leaves the event as it was.

use bv_core::FrameRate;
use bv_timeline::{ClipId, ResampleMode, TrackKind};

use crate::app::BananaApp;
use crate::egui::{self, RichText, Stroke, Ui, vec2};
use crate::i18n::tr;
use crate::media_properties::MediaPropertiesDialog;
use crate::theme;

/// The labels' column.
const LABEL_W: f32 = 118.0;
/// The content's width (the Media tab's).
const WIDTH: f32 = 382.0;
/// The content's height: every tab as tall, as VEGAS's.
const HEIGHT: f32 = 500.0;

/// The window's tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Event,
    Media,
    General,
}

/// What a video event shows past its media's end, one of three as VEGAS's
/// radio buttons say (none: nothing shows there).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PastEnd {
    Hold,
    Loop,
    AllFrames,
}

/// The settings the event tab edits.
#[derive(Clone, Debug, PartialEq)]
struct Settings {
    take_name: String,
    muted: bool,
    locked: bool,
    /// Video: Maintain aspect ratio.
    keep_aspect: bool,
    reduce_flicker: bool,
    past_end: Option<PastEnd>,
    resample: Option<ResampleMode>,
    rate: f32,
    undersample: f32,
    /// Conform to Project Frame Rate was pressed (done on OK, unless the
    /// rate is changed again).
    conform: Option<f32>,
    /// Audio: Loop, Invert phase, Normalize.
    looped: bool,
    invert_phase: bool,
    normalize: bool,
    /// Audio: the time stretch's method — the pitch kept (Classic) or not
    /// (None) — and the pitch change, in semitones.
    preserve_pitch: bool,
    pitch: f32,
}

#[derive(Debug)]
pub struct EventPropertiesDialog {
    clip: ClipId,
    video: bool,
    page: Page,
    was: Settings,
    now: Settings,
    /// The Media tab: its media's Properties.
    media: MediaPropertiesDialog,
    /// The General tab's text, worked out when it first shows.
    general: Option<String>,
}

impl EventPropertiesDialog {
    /// The window for event `clip`, when the project has it.
    pub fn new(app: &BananaApp, clip: ClipId) -> Option<EventPropertiesDialog> {
        let p = app.project();
        let c = p.clip(clip)?;
        let video = p.clip_track(clip)?.kind == TrackKind::Video;
        let past_end = if c.all_frames {
            Some(PastEnd::AllFrames)
        } else if c.looped {
            Some(PastEnd::Loop)
        } else if c.hold_last {
            Some(PastEnd::Hold)
        } else {
            None
        };
        let settings = Settings {
            take_name: p.take_name(clip, c.active_take()).unwrap_or_default(),
            muted: c.muted,
            locked: c.locked,
            keep_aspect: c.pan_crop.keep_aspect,
            reduce_flicker: c.reduce_flicker,
            past_end,
            resample: c.resample,
            rate: c.rate as f32,
            undersample: c.undersample,
            conform: None,
            looped: c.looped,
            invert_phase: c.invert_phase,
            normalize: c.normalize.is_some(),
            preserve_pitch: c.preserve_pitch,
            pitch: c.pitch,
        };
        Some(EventPropertiesDialog {
            clip,
            video,
            page: Page::Event,
            was: settings.clone(),
            now: settings,
            media: MediaPropertiesDialog::new(app, c.media)?,
            general: None,
        })
    }

    /// Its tab `page` shown (screen tests).
    fn show_page(&mut self, page: Page) {
        self.page = page;
    }

    /// Draws the window; `false` once it is closed.
    pub fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        let Some(m) = app
            .project()
            .clip(self.clip)
            .and_then(|c| app.project().media(c.media))
            .cloned()
        else {
            return false;
        };
        let mut open = true;
        let mut close = false;
        crate::floating::Window::new(tr("Properties"))
            .id(egui::Id::new(("event-properties", self.clip)))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let event = if self.video {
                        tr("Video Event")
                    } else {
                        tr("Audio Event")
                    };
                    for (page, label) in [
                        (Page::Event, event),
                        (Page::Media, tr("Media")),
                        (Page::General, tr("General")),
                    ] {
                        let on = self.page == page;
                        if crate::widgets::tab_button(ui, label, on, true, 70.0).clicked() {
                            self.page = page;
                        }
                    }
                });
                egui::Frame::new()
                    .fill(theme::panel())
                    .stroke(Stroke::new(1.0, theme::separator()))
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.set_width(WIDTH);
                        ui.set_min_height(HEIGHT);
                        // Lists and fields with a border, like VEGAS's.
                        ui.visuals_mut().widgets.inactive.bg_stroke =
                            Stroke::new(1.0, theme::separator());
                        match self.page {
                            Page::Event if self.video => self.video_tab(ui, &m, app),
                            Page::Event => self.audio_tab(ui),
                            Page::Media => self.media.media_tab(ui, app, &m),
                            Page::General => {
                                let about = self.media.about();
                                let text = self.general.get_or_insert_with(|| {
                                    crate::media_general::general_text(app.project(), &m, about)
                                });
                                crate::media_general::general_tab(ui, text, HEIGHT - 30.0);
                            }
                        }
                    });
                ui.add_space(6.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new(tr("Cancel")).min_size(vec2(72.0, 0.0)))
                        .clicked()
                    {
                        close = true;
                    }
                    if ui
                        .add(
                            egui::Button::new(RichText::new("OK").strong())
                                .min_size(vec2(72.0, 0.0)),
                        )
                        .clicked()
                    {
                        close = self.apply(app, &m);
                    }
                });
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    close = true;
                }
            });
        self.media.picker(ctx);
        open && !close
    }

    /// The Video Event tab, as VEGAS's: the take's name, the switches, then
    /// the playback rate (Conform to Project Frame Rate beside it) and the
    /// undersample rate (the frame rate it gives beside it).
    fn video_tab(&mut self, ui: &mut Ui, m: &bv_timeline::MediaInfo, app: &BananaApp) {
        let s = &mut self.now;
        take_row(ui, &mut s.take_name);
        group(ui, tr("Switches"));
        ui.indent("ep-switches", |ui| {
            crate::widgets::checkbox(ui, &mut s.muted, tr("Mute"));
            crate::widgets::checkbox(ui, &mut s.locked, tr("Lock"));
            crate::widgets::checkbox(ui, &mut s.keep_aspect, tr("Maintain aspect ratio"));
            crate::widgets::checkbox(ui, &mut s.reduce_flicker, tr("Reduce interlace flicker"));
            for (end, label) in [
                (PastEnd::Hold, tr("Hold last frame")),
                (PastEnd::Loop, tr("Loop event")),
                (PastEnd::AllFrames, tr("Trim event to include all frames")),
            ] {
                if crate::widgets::radio(ui, s.past_end == Some(end), label).clicked() {
                    s.past_end = Some(end);
                }
            }
            ui.add_space(8.0);
            for (mode, label) in [
                (None, tr("Use Project Resample Mode")),
                (Some(ResampleMode::FrameBlend), tr("Frame Blend")),
                (Some(ResampleMode::OpticalFlow), tr("Optical Flow")),
                (Some(ResampleMode::Disable), tr("Disable resample")),
            ] {
                crate::widgets::radio_value(ui, &mut s.resample, mode, label);
            }
        });
        ui.add_space(6.0);
        line(ui);
        ui.add_space(6.0);
        let media_rate = m.video.filter(|_| !m.still).map(|v| v.frame_rate);
        let project_rate = app.project().settings.frame_rate;
        row(ui, tr("Playback rate:"), |ui| {
            if spin(ui, "ep-rate", &mut s.rate, 0.05..=20.0) {
                s.conform = None;
            }
            let conform = conformed(media_rate, project_rate);
            if ui
                .add_enabled(
                    conform.is_some_and(|c| (c - s.rate).abs() > 1e-4),
                    egui::Button::new(tr("Conform to Project Frame Rate")),
                )
                .clicked()
                && let Some(c) = conform
            {
                s.rate = c;
                s.conform = Some(c);
            }
        });
        row(ui, tr("Undersample rate:"), |ui| {
            spin(ui, "ep-undersample", &mut s.undersample, 0.01..=1.0);
            if let Some(r) = media_rate {
                let fps = r.as_f64() * f64::from(s.undersample);
                ui.label(trf!("{} fps", crate::fmt::num(fps, 3)));
            }
        });
    }

    /// The Audio Event tab: the take's name, the switches, the time
    /// stretch's method (None: the pitch follows the playback rate;
    /// Classic: it is kept) and the pitch change.
    fn audio_tab(&mut self, ui: &mut Ui) {
        let s = &mut self.now;
        take_row(ui, &mut s.take_name);
        group(ui, tr("Switches"));
        ui.indent("ep-audio-switches", |ui| {
            crate::widgets::checkbox(ui, &mut s.muted, tr("Mute"));
            crate::widgets::checkbox(ui, &mut s.locked, tr("Lock"));
            crate::widgets::checkbox(ui, &mut s.looped, tr("Loop"));
            crate::widgets::checkbox(ui, &mut s.invert_phase, tr("Invert phase"));
            crate::widgets::checkbox(ui, &mut s.normalize, tr("Normalize"));
        });
        group(ui, tr("Time stretch/pitch shift"));
        row(ui, tr("Method:"), |ui| {
            let list = [
                (false, tr("None").to_string(), true),
                (true, tr("Classic").to_string(), true),
            ];
            crate::project_properties::choice(ui, "ep-method", 160.0, &mut s.preserve_pitch, &list);
        });
        row(ui, tr("Pitch change (semitones):"), |ui| {
            ui.add(
                crate::fmt::drag_grouped(&mut s.pitch, 3)
                    .speed(0.05)
                    .range(-48.0..=48.0),
            );
        });
    }

    /// The changes, as one step of the undo history; `false` while the
    /// Media tab's timecode cannot be read.
    fn apply(&mut self, app: &mut BananaApp, m: &bv_timeline::MediaInfo) -> bool {
        let id = self.clip;
        let (was, now) = (self.was.clone(), self.now.clone());
        let video = self.video;
        let media = &mut self.media;
        app.as_one_undo(|app| {
            if !media.apply(app, m) {
                return false;
            }
            if now == was {
                return true;
            }
            // Normalize's gain, worked out from the waveform.
            let gain = (now.normalize && !was.normalize)
                .then(|| app.project().clip(id).cloned())
                .flatten()
                .and_then(|c| app.normalize_gain(&c));
            if now.normalize && !was.normalize && gain.is_none() {
                app.notify(tr("Wait for the waveforms to finish computing."));
            }
            let fill = (video && now.keep_aspect != was.keep_aspect).then_some(!now.keep_aspect);
            app.modify(|p| {
                if now.take_name != was.take_name
                    && let Some(k) = p.clip(id).map(|c| c.active_take())
                {
                    let _ = p.rename_take(id, k, &now.take_name);
                }
                if video {
                    match now.conform.filter(|&c| c == now.rate) {
                        Some(_) => {
                            let _ = p.conform_to_project_rate(id, false);
                        }
                        None if now.rate != was.rate => {
                            if let Some(c) = p.clip_mut(id) {
                                c.rate = f64::from(now.rate);
                            }
                        }
                        None => {}
                    }
                    if now.past_end != was.past_end
                        && let Some(end) = now.past_end
                    {
                        if let Some(c) = p.clip_mut(id) {
                            c.looped = end == PastEnd::Loop;
                            c.hold_last = end == PastEnd::Hold;
                        }
                        let _ = p.set_all_frames(id, end == PastEnd::AllFrames);
                    }
                }
                let Some(c) = p.clip_mut(id) else {
                    return;
                };
                c.muted = now.muted;
                c.locked = now.locked;
                if video {
                    c.reduce_flicker = now.reduce_flicker;
                    c.resample = now.resample;
                    c.undersample = now.undersample;
                } else {
                    c.looped = now.looped;
                    c.invert_phase = now.invert_phase;
                    c.preserve_pitch = now.preserve_pitch;
                    c.pitch = now.pitch;
                    if !now.normalize {
                        c.normalize = None;
                    } else if let Some(Some(g)) = gain {
                        c.normalize = Some(g);
                    }
                }
            });
            if let Some(fill) = fill {
                app.set_maintain_aspect(&[id], !fill);
            }
            true
        })
    }
}

/// The rate at which a media of rate `media` plays one frame per project
/// frame (Conform to Project Frame Rate), within 0.25 to 4; `None` for no
/// video or one of the project's rate.
fn conformed(media: Option<FrameRate>, project: FrameRate) -> Option<f32> {
    let m = media.filter(|m| m.as_f64() > 0.0 && *m != project)?;
    Some((project.as_f64() / m.as_f64()).clamp(0.25, 4.0) as f32)
}

/// A number with VEGAS's spin buttons beside it, a thousandth a step up or
/// down (dragged or typed as the others); whether it changed.
fn spin(ui: &mut Ui, id: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>) -> bool {
    let mut changed = ui
        .add(
            crate::fmt::drag_grouped(value, 3)
                .speed(0.005)
                .range(range.clone()),
        )
        .changed();
    let (rect, _) = ui.allocate_exact_size(vec2(11.0, 20.0), egui::Sense::hover());
    for up in [true, false] {
        let half = if up {
            egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rect.center().y))
        } else {
            egui::Rect::from_min_max(egui::pos2(rect.min.x, rect.center().y), rect.max)
        };
        let resp = ui.interact(half, ui.id().with((id, up)), egui::Sense::click());
        let color = if resp.hovered() {
            theme::text()
        } else {
            theme::text_dim()
        };
        let c = half.center();
        let (tip, base) = if up { (-2.5, 2.0) } else { (2.5, -2.0) };
        ui.painter().add(egui::Shape::convex_polygon(
            vec![
                egui::pos2(c.x, c.y + tip),
                egui::pos2(c.x - 3.0, c.y + base),
                egui::pos2(c.x + 3.0, c.y + base),
            ],
            color,
            Stroke::NONE,
        ));
        if resp.clicked() {
            let step = if up { 0.001 } else { -0.001 };
            *value = ((*value + step) * 1000.0).round() / 1000.0;
            *value = value.clamp(*range.start(), *range.end());
            changed = true;
        }
    }
    changed
}

/// The Active take name row.
fn take_row(ui: &mut Ui, name: &mut String) {
    row(ui, tr("Active take name:"), |ui| {
        ui.add_sized(
            vec2(WIDTH - LABEL_W - 8.0, 20.0),
            egui::TextEdit::singleline(name),
        );
    });
}

/// A labelled row, the label on the left as VEGAS's.
fn row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            vec2(LABEL_W, 22.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_width(LABEL_W);
                ui.label(label);
            },
        );
        add(ui);
    });
}

/// A group's title and its line, as VEGAS's.
fn group(ui: &mut Ui, title: &str) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(title);
        let r = ui.available_rect_before_wrap();
        ui.painter().hline(
            r.left()..=r.right(),
            r.center().y,
            Stroke::new(1.0, theme::separator()),
        );
    });
}

/// A line across, under the switches.
fn line(ui: &mut Ui) {
    let r = ui.available_rect_before_wrap();
    ui.painter().hline(
        r.left()..=r.right(),
        r.top(),
        Stroke::new(1.0, theme::separator()),
    );
    ui.add_space(1.0);
}

impl BananaApp {
    /// The picked event's Properties open on its tab `page` ("event",
    /// "media" or "general"): whether there is one (screen tests).
    pub fn event_properties_for_review(&mut self, page: &str) -> bool {
        let page = match page {
            "media" => Page::Media,
            "general" => Page::General,
            _ => Page::Event,
        };
        let Some(clip) = self.primary_clip() else {
            return false;
        };
        self.dialogs.event_properties = EventPropertiesDialog::new(self, clip).map(|mut d| {
            d.show_page(page);
            d
        });
        self.dialogs.event_properties.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// OK applies the switches and rates as one step of the undo history;
    /// a take named; Maintain aspect ratio off stretches the media.
    #[test]
    fn ok_applies_the_video_event_tab_in_one_step() {
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        let id = app.primary_clip().expect("an event");
        let mut d = EventPropertiesDialog::new(&app, id).expect("a dialog");
        assert!(d.video);
        let before = app.project().clone();
        d.now.muted = true;
        d.now.reduce_flicker = true;
        d.now.past_end = Some(PastEnd::Loop);
        d.now.resample = Some(ResampleMode::Disable);
        d.now.rate = 0.5;
        d.now.undersample = 0.5;
        d.now.take_name = "Take A".into();
        let m = app
            .project()
            .clip(id)
            .and_then(|c| app.project().media(c.media))
            .cloned()
            .expect("its media");
        assert!(d.apply(&mut app, &m));
        let p = app.project();
        let c = p.clip(id).expect("the event");
        assert!(c.muted && c.reduce_flicker && c.looped && !c.hold_last);
        assert_eq!(c.resample, Some(ResampleMode::Disable));
        assert_eq!((c.rate, c.undersample), (0.5, 0.5));
        assert_eq!(
            c.duration,
            before.clip(id).unwrap().duration,
            "its length kept"
        );
        assert_eq!(p.take_name(id, c.active_take()).as_deref(), Some("Take A"));
        app.undo();
        assert_eq!(*app.project(), before, "one undo step");
    }

    /// An audio event's tab: its switches, the method and the pitch.
    #[test]
    fn ok_applies_the_audio_event_tab() {
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        let id = app
            .project()
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Audio)
            .and_then(|t| t.clips.first())
            .map(|c| c.id)
            .expect("an audio event");
        let mut d = EventPropertiesDialog::new(&app, id).expect("a dialog");
        assert!(!d.video);
        d.now.invert_phase = true;
        d.now.locked = true;
        d.now.preserve_pitch = !d.was.preserve_pitch;
        d.now.pitch = 2.0;
        let m = app
            .project()
            .clip(id)
            .and_then(|c| app.project().media(c.media))
            .cloned()
            .expect("its media");
        assert!(d.apply(&mut app, &m));
        let c = app.project().clip(id).expect("the event");
        assert!(c.invert_phase && c.locked);
        assert_eq!((c.preserve_pitch, c.pitch), (d.now.preserve_pitch, 2.0));
    }

    /// The rate Conform to Project Frame Rate gives: a frame of the media
    /// a frame of the project, within 0.25 to 4.
    #[test]
    fn conforming() {
        let p = FrameRate::FPS_29_97;
        assert_eq!(conformed(Some(p), p), None);
        assert!((conformed(Some(FrameRate::FPS_24), FrameRate::FPS_24).is_none()));
        let r = conformed(Some(FrameRate::new(120, 1)), FrameRate::new(30, 1)).unwrap();
        assert!((r - 0.25).abs() < 1e-6);
        let r = conformed(Some(FrameRate::new(24, 1)), FrameRate::new(30, 1)).unwrap();
        assert!((r - 1.25).abs() < 1e-6);
    }
}
