//! A media file's Properties (Project Media's Properties button), as
//! VEGAS's Media tab: the file, its timecode and each stream's properties
//! — frame rate, field order, pixel aspect ratio, alpha, the color space
//! its values are in under ACES, color range and rotation. BananaV changes
//! the tape name, the timecode (its own instead of the file's: Edit
//! Details' Timecode In and Out count from it), the field order (how it
//! is deinterlaced), the pixel aspect ratio and the color space; the rest
//! shows what the file has, greyed out until BananaV can change it. The
//! General tab tells what the file is (`media_general.rs`); an event's
//! Properties show both tabs after its own (`event_properties.rs`). With
//! several media picked, their field order, pixel aspect ratio and color
//! space are edited together (a setting they do not share shows
//! "(differing values)" and stays as each has it unless changed), and a
//! video's settings can be saved as the profile files of its kind get
//! when imported (`UiSettings::video_profiles`).

use bv_core::{FrameRate, timecode};
use bv_timeline::{
    AlphaChannel, ColorSpace, MediaId, MediaInfo, MediaRotation, MediaSource, PixelAspect,
};

use crate::app::BananaApp;
use crate::egui::{self, RichText, Stroke, Ui, vec2};
use crate::i18n::{tr, trc};
use crate::project_properties::{choice, field_orders, fixed_choice, pixel_aspects};
use crate::theme;
use crate::timefmt::{RulerFormat, RulerFormatExt};

/// Each list's width.
const LIST_W: f32 = 200.0;
/// The labels' column.
const LABEL_W: f32 = 142.0;

/// A stream of the file, as the Stream list names it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Stream {
    /// Its picture (a video's, or a still's).
    Video,
    /// Its audio streams, the first one 0.
    Audio(usize),
}

/// Color range, as its list names it: as the file says, or Limited or Full
/// whatever it says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Range {
    Undefined,
    Limited,
    Full,
}

/// How the media's pictures are read, as the Media tab's lists show it
/// (one media at a time).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Reading {
    alpha: AlphaChannel,
    background: [u8; 3],
    range: Range,
    rotation: MediaRotation,
}

pub struct MediaPropertiesDialog {
    media: MediaId,
    /// The others picked with it, edited together.
    others: Vec<MediaId>,
    stream: Stream,
    /// The settings (`None`: differing values, kept).
    pixel_aspect: Option<PixelAspect>,
    field_order: Option<bv_core::FieldOrder>,
    color_space: Option<Option<ColorSpace>>,
    /// Alpha channel, its Background color, Color range and Rotation; and
    /// as the window found them (OK changes what differs: the lists show
    /// what the file says where nothing is set).
    reading: Reading,
    was: Reading,
    /// What its file is (its format, what it says of its streams), read
    /// when the window opens.
    about: Option<bv_engine::FileDescription>,
    /// The Background color's picker, while it is open.
    picking: Option<crate::color_picker::ColorPicker>,
    tape_name: String,
    /// Use custom timecode, and the timecode typed.
    custom_timecode: bool,
    timecode: String,
    bad_timecode: bool,
    /// A profile saved with the button, kept in the settings once drawn.
    saved_profile: Option<VideoProfile>,
    /// The General tab shows (its text worked out when it first does).
    general: Option<Option<String>>,
}

impl std::fmt::Debug for MediaPropertiesDialog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaPropertiesDialog")
            .field("media", &self.media)
            .field("reading", &self.reading)
            .finish_non_exhaustive()
    }
}

impl MediaPropertiesDialog {
    /// The window for media `id`, when the project has it.
    pub fn new(app: &BananaApp, id: MediaId) -> Option<MediaPropertiesDialog> {
        let m = app.project().media(id)?;
        let about = match &m.source {
            MediaSource::File { path } => Some(bv_engine::describe_file(path)),
            MediaSource::Generated(_) => None,
        };
        let reading = shown_reading(m, about.as_ref());
        Some(MediaPropertiesDialog {
            media: id,
            stream: if m.video.is_some() {
                Stream::Video
            } else {
                Stream::Audio(0)
            },
            others: Vec::new(),
            pixel_aspect: Some(m.video.map_or(PixelAspect::SQUARE, |v| v.pixel_aspect)),
            field_order: Some(m.video.map_or(Default::default(), |v| v.field_order)),
            reading,
            was: reading,
            about,
            picking: None,
            color_space: Some(m.color_space),
            tape_name: m.reel.tape_name.clone(),
            custom_timecode: m.reel.timecode.is_some(),
            timecode: {
                let start = m.reel.timecode.unwrap_or_else(|| {
                    bv_engine::media_stamps(m)
                        .timecode
                        .map_or(bv_core::Tick::ZERO, bv_core::Tick::from_seconds_f64)
                });
                timecode_format(m).format_ruler_time(start, &app.project().settings)
            },
            bad_timecode: false,
            saved_profile: None,
            general: None,
        })
    }

    /// The window for several media (the video ones among `ids`, at least
    /// two; else as `new` for the first).
    pub fn new_many(app: &BananaApp, ids: &[MediaId]) -> Option<MediaPropertiesDialog> {
        let p = app.project();
        let video: Vec<&MediaInfo> = ids
            .iter()
            .filter_map(|&id| p.media(id))
            .filter(|m| m.video.is_some())
            .collect();
        if video.len() < 2 {
            return MediaPropertiesDialog::new(app, *ids.first()?);
        }
        let mut d = MediaPropertiesDialog::new(app, video[0].id)?;
        d.others = video[1..].iter().map(|m| m.id).collect();
        let same = |f: &dyn Fn(&MediaInfo) -> String| video.iter().all(|m| f(m) == f(video[0]));
        if !same(&|m| format!("{:?}", m.video.map(|v| v.pixel_aspect))) {
            d.pixel_aspect = None;
        }
        if !same(&|m| format!("{:?}", m.video.map(|v| v.field_order))) {
            d.field_order = None;
        }
        if !same(&|m| format!("{:?}", m.color_space)) {
            d.color_space = None;
        }
        Some(d)
    }

    /// Draws the window; `false` once it is closed.
    pub fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        let Some(m) = app.project().media(self.media).cloned() else {
            return false;
        };
        let mut open = true;
        let mut close = false;
        crate::floating::Window::new(tr("Properties"))
            .id(egui::Id::new(("media-properties", self.media)))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let general = self.general.is_some();
                    if crate::widgets::tab_button(ui, tr("Media"), !general, true, 70.0).clicked() {
                        self.general = None;
                    }
                    // One media's: what its file is.
                    let one = self.others.is_empty();
                    if crate::widgets::tab_button(ui, tr("General"), general, one, 70.0).clicked()
                        && !general
                    {
                        self.general = Some(None);
                    }
                });
                egui::Frame::new()
                    .fill(theme::panel())
                    .stroke(Stroke::new(1.0, theme::separator()))
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.set_width(LABEL_W + LIST_W + 40.0);
                        // A profile saved: in place of the kind's old one.
                        if let Some(p) = self.saved_profile.take() {
                            let list = &mut app.settings.video_profiles;
                            list.retain(|q| {
                                (&q.extension, q.width, q.height, q.rate)
                                    != (&p.extension, p.width, p.height, p.rate)
                            });
                            list.push(p);
                            let _ = app.settings.save();
                            app.notify(tr("Saved: files of this kind get these settings."));
                        }
                        // Lists with a border, like VEGAS's.
                        ui.visuals_mut().widgets.inactive.bg_stroke =
                            Stroke::new(1.0, theme::separator());
                        if let Some(text) = &mut self.general {
                            let about = self.about.as_ref();
                            let text = text.get_or_insert_with(|| {
                                crate::media_general::general_text(app.project(), &m, about)
                            });
                            crate::media_general::general_tab(ui, text, 420.0);
                        } else if self.others.is_empty() {
                            self.media_tab(ui, app, &m);
                        } else {
                            self.many_tab(ui, app);
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
        self.picker(ctx);
        open && !close
    }

    /// The Background color's picker, after the window that opened it.
    pub(crate) fn picker(&mut self, ctx: &egui::Context) {
        let Some(p) = &mut self.picking else {
            return;
        };
        match p.show(ctx) {
            crate::color_picker::Outcome::Open => {}
            crate::color_picker::Outcome::Cancelled => self.picking = None,
            crate::color_picker::Outcome::Chosen(c) => {
                self.reading.background = [c[0], c[1], c[2]];
                self.picking = None;
            }
        }
    }

    /// What the file is (the General tab's), read when the window opened.
    pub(crate) fn about(&self) -> Option<&bv_engine::FileDescription> {
        self.about.as_ref()
    }

    /// Several media: the settings they can share, each with "(differing
    /// values)" when they do not.
    fn many_tab(&mut self, ui: &mut Ui, app: &BananaApp) {
        let n = self.others.len() + 1;
        row(ui, tr("File name:"), |ui| {
            ui.label(trn!(n, "{} file", "{} files", n));
        });
        let differing = tr("(differing values)").to_string();
        row(ui, tr("Field order:"), |ui| {
            let list = field_orders()
                .into_iter()
                .map(|(f, s, ok)| (Some(f), s, ok))
                .collect();
            let list = with_differing(list, &self.field_order);
            choice(ui, "mp-field-order", LIST_W, &mut self.field_order, &list);
        });
        row(ui, tr("Pixel aspect ratio:"), |ui| {
            let (ratios, _) = pixel_aspects(self.pixel_aspect.unwrap_or(PixelAspect::SQUARE));
            let list = ratios
                .into_iter()
                .map(|(a, s, ok)| (Some(a), s, ok))
                .collect();
            let list = with_differing(list, &self.pixel_aspect);
            choice(ui, "mp-par", LIST_W, &mut self.pixel_aspect, &list);
        });
        if app.project().settings.color_managed() {
            row(ui, tr("Color space:"), |ui| {
                let mut list = vec![(Some(None), tr("Default").to_string(), true)];
                list.extend(
                    ColorSpace::ALL
                        .iter()
                        .map(|&c| (Some(Some(c)), c.name().to_string(), c.drawn())),
                );
                if self.color_space.is_none() {
                    list.insert(0, (None, differing.clone(), true));
                }
                choice(ui, "mp-color-space", LIST_W, &mut self.color_space, &list);
            });
        }
    }

    /// The changes, as one undo; `false` while the timecode typed cannot
    /// be read.
    pub(crate) fn apply(&mut self, app: &mut BananaApp, m: &MediaInfo) -> bool {
        if !self.others.is_empty() {
            let ids: Vec<MediaId> = std::iter::once(self.media)
                .chain(self.others.iter().copied())
                .collect();
            let (par, fields, space) = (self.pixel_aspect, self.field_order, self.color_space);
            app.modify(|p| {
                for m in p.media.iter_mut().filter(|m| ids.contains(&m.id)) {
                    if let Some(v) = &mut m.video {
                        if let Some(a) = par {
                            v.pixel_aspect = a;
                        }
                        if let Some(f) = fields {
                            v.field_order = f;
                        }
                    }
                    if let Some(c) = space {
                        m.color_space = c;
                    }
                }
            });
            return true;
        }
        let timecode = if self.custom_timecode {
            let settings = app.project().settings;
            match timecode_format(m).parse(&self.timecode, &settings) {
                Some(t) => Some(t),
                None => {
                    self.bad_timecode = true;
                    return false;
                }
            }
        } else {
            None
        };
        let reel = bv_timeline::MediaReel {
            tape_name: self.tape_name.trim().to_string(),
            timecode,
        };
        let pixel_aspect = self.pixel_aspect.unwrap_or(PixelAspect::SQUARE);
        let field_order = self.field_order.unwrap_or_default();
        let color_space = self.color_space.flatten();
        let (r, was) = (self.reading, self.was);
        let mut video = m.video;
        if let Some(v) = &mut video {
            v.pixel_aspect = pixel_aspect;
            v.field_order = field_order;
            if r.range != was.range {
                (v.full_range, v.reading.limited) = match r.range {
                    Range::Undefined => (false, false),
                    Range::Limited => (false, true),
                    Range::Full => (true, false),
                };
            }
            if r.alpha != was.alpha {
                v.reading.alpha = r.alpha;
            }
            v.reading.background = r.background;
            // Turned: its width and height swap with a quarter turn.
            if r.rotation != v.reading.rotation {
                *v = v.turned_to(r.rotation);
            }
        }
        if video == m.video && m.color_space == color_space && m.reel == reel {
            return true;
        }
        let id = self.media;
        app.modify(|p| {
            if let Some(m) = p.media.iter_mut().find(|m| m.id == id) {
                m.video = video;
                m.color_space = color_space;
                m.reel = reel;
            }
        });
        true
    }

    /// The Media tab (an event's Properties show it too).
    pub(crate) fn media_tab(&mut self, ui: &mut Ui, app: &BananaApp, m: &MediaInfo) {
        let soon = tr("Coming soon to BananaV");
        let path = match &m.source {
            MediaSource::File { path } => path.clone(),
            MediaSource::Generated(_) => crate::i18n::media_name(m),
        };
        row(ui, tr("File name:"), |ui| {
            ui.add(egui::Label::new(path).truncate());
        });
        row(ui, tr("Tape name:"), |ui| {
            ui.add_sized(
                vec2(LIST_W, 20.0),
                egui::TextEdit::singleline(&mut self.tape_name),
            );
        });
        group(ui, tr("Timecode"));
        if ui
            .radio(!self.custom_timecode, tr("Use timecode in file"))
            .clicked()
        {
            self.custom_timecode = false;
        }
        ui.horizontal(|ui| {
            if ui
                .radio(self.custom_timecode, tr("Use custom timecode:"))
                .clicked()
            {
                self.custom_timecode = true;
            }
            ui.add_enabled(
                self.custom_timecode,
                egui::TextEdit::singleline(&mut self.timecode)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(110.0),
            );
        });
        if self.bad_timecode {
            ui.colored_label(theme::error(), tr("Type the timecode in its format."));
        }
        ui.horizontal(|ui| {
            ui.add_space(18.0);
            fixed_choice(
                ui,
                LIST_W + LABEL_W - 18.0,
                "mp-timecode-format",
                timecode_format(m).label(),
                soon,
            );
        });
        group(ui, tr("Stream properties"));
        let streams = streams(m);
        let file = matches!(m.source, MediaSource::File { .. });
        row(ui, tr("Stream:"), |ui| {
            choice(ui, "mp-stream", LIST_W, &mut self.stream, &streams);
            // Files of this kind get these settings when imported.
            let profile = file && !m.still && self.stream == Stream::Video;
            let clicked = ui
                .add_enabled_ui(profile, |ui| {
                    crate::widgets::icon_button(
                        ui,
                        crate::icons::Icon::Save,
                        tr("Save settings to video profiles for future auto-detection"),
                    )
                })
                .inner
                .clicked();
            if clicked
                && profile
                && let Some(p) = VideoProfile::of(
                    m,
                    self.field_order.unwrap_or_default(),
                    self.pixel_aspect.unwrap_or(PixelAspect::SQUARE),
                    self.color_space.flatten(),
                )
            {
                self.saved_profile = Some(p);
            }
        });
        row(ui, tr("Format:"), |ui| {
            ui.label(format_text(m, self.about.as_ref(), self.stream));
        });
        let rate = app.project().settings.frame_rate;
        row(ui, tr("Attributes:"), |ui| {
            ui.label(attributes(m, self.stream, rate));
        });
        if self.stream != Stream::Video {
            return;
        }
        let Some(v) = m.video else {
            return;
        };
        ui.add_space(10.0);
        row(ui, tr("Frame rate:"), |ui| {
            let text = if m.still {
                crate::fmt::num(0.0, 3)
            } else {
                rate_text(v.frame_rate)
            };
            fixed_choice(ui, LIST_W, "mp-rate", &text, soon);
        });
        row(ui, tr("Field order:"), |ui| {
            if m.still {
                fixed_choice(
                    ui,
                    LIST_W,
                    "mp-field-order",
                    tr("None (progressive scan)"),
                    soon,
                );
            } else {
                let mut order = self.field_order.unwrap_or_default();
                choice(ui, "mp-field-order", LIST_W, &mut order, &field_orders());
                self.field_order = Some(order);
            }
        });
        row(ui, tr("Pixel aspect ratio:"), |ui| {
            let (ratios, mut par) = pixel_aspects(self.pixel_aspect.unwrap_or(PixelAspect::SQUARE));
            choice(ui, "mp-par", LIST_W, &mut par, &ratios);
            self.pixel_aspect = Some(par);
        });
        // How its pictures' alpha channel is read (files' media).
        row(ui, tr("Alpha channel:"), |ui| {
            if file {
                let list: Vec<(AlphaChannel, String, bool)> = AlphaChannel::ALL
                    .iter()
                    .map(|&a| (a, alpha_name(a).to_string(), true))
                    .collect();
                choice(ui, "mp-alpha", LIST_W, &mut self.reading.alpha, &list);
            } else {
                fixed_choice(ui, LIST_W, "mp-alpha", alpha_name(AlphaChannel::None), soon);
            }
        });
        // Premultiplied (dirty): the color the picture was matted against.
        let dirty = self.reading.alpha == AlphaChannel::PremultipliedDirty;
        ui.horizontal(|ui| {
            ui.add_space(LABEL_W + 4.0);
            ui.add_enabled_ui(dirty, |ui| {
                ui.label(tr("Background color:"));
                let [r, g, b] = self.reading.background;
                if crate::preferences::swatch(ui, [r, g, b, 255], tr("Background color")).clicked()
                {
                    self.picking = Some(crate::color_picker::ColorPicker::new(
                        tr("Background color"),
                        [r, g, b, 255],
                    ));
                }
            });
        });
        ui.add_space(6.0);
        // The space its values are in under ACES (VEGAS's list).
        let aces = app.project().settings.color_managed();
        row(ui, tr("Color space:"), |ui| {
            let mut spaces = vec![(None, tr("Default").to_string(), true)];
            spaces.extend(
                ColorSpace::ALL
                    .iter()
                    .map(|&c| (Some(c), c.name().to_string(), c.drawn())),
            );
            let mut space = self.color_space.flatten();
            let r = ui
                .scope(|ui| choice(ui, "mp-color-space", LIST_W, &mut space, &spaces))
                .response;
            if !aces {
                r.on_hover_text(tr(
                    "The color space applies with ACES (File › Properties › Video).",
                ));
            }
            self.color_space = Some(space);
        });
        row(ui, tr("Input Device Transform:"), |ui| {
            fixed_choice(ui, LIST_W, "mp-idt", tr("Off"), soon);
        });
        // Color Range: Undefined (as the file says), Limited or Full for a
        // video's Y′CbCr, in full-range projects (stills are RGB).
        row(ui, tr("Color range:"), |ui| {
            let video_levels = app.project().settings.pixel_format.is_video_levels();
            if m.still {
                fixed_choice(ui, LIST_W, "mp-range", range_name(Range::Undefined), soon);
            } else if video_levels {
                fixed_choice(
                    ui,
                    LIST_W,
                    "mp-range",
                    range_name(self.reading.range),
                    tr(
                        "The color range applies in full range pixel formats (File › Properties › Video).",
                    ),
                );
            } else {
                let list = [Range::Undefined, Range::Limited, Range::Full]
                    .map(|r| (r, range_name(r).to_string(), true));
                choice(ui, "mp-range", LIST_W, &mut self.reading.range, &list);
            }
        });
        // Turned (files' media): a quarter turn swaps the width and height.
        row(ui, tr("Rotation:"), |ui| {
            if file {
                let list: Vec<(MediaRotation, String, bool)> = MediaRotation::ALL
                    .iter()
                    .map(|&r| (r, rotation_name(r).to_string(), true))
                    .collect();
                choice(ui, "mp-rotation", LIST_W, &mut self.reading.rotation, &list);
            } else {
                fixed_choice(
                    ui,
                    LIST_W,
                    "mp-rotation",
                    rotation_name(MediaRotation::Original),
                    soon,
                );
            }
        });
    }
}

/// What the Media tab's reading lists show of media `m` (`about`, what its
/// file is): its settings, else what the file says — its color range, an
/// alpha channel's presence (straight; OpenEXR's premultiplied).
fn shown_reading(m: &MediaInfo, about: Option<&bv_engine::FileDescription>) -> Reading {
    let stored = m.video.map(|v| v.reading).unwrap_or_default();
    let video_track = about.and_then(|d| d.tracks.iter().find(|t| &t.handler == b"vide"));
    let range = match m.video {
        Some(v) if v.full_range => Range::Full,
        Some(v) if v.reading.limited => Range::Limited,
        _ => match video_track.and_then(|t| t.full_range) {
            Some(true) => Range::Full,
            Some(false) => Range::Limited,
            None => Range::Undefined,
        },
    };
    let alpha = match stored.alpha {
        AlphaChannel::Undefined => match about.and_then(|d| d.alpha) {
            Some(true) if about.is_some_and(|d| d.format == "OpenEXR") => {
                AlphaChannel::Premultiplied
            }
            Some(true) => AlphaChannel::Straight,
            Some(false) => AlphaChannel::None,
            None => AlphaChannel::Undefined,
        },
        a => a,
    };
    Reading {
        alpha,
        background: stored.background,
        range,
        rotation: stored.rotation,
    }
}

/// An Alpha channel, as its list names it.
fn alpha_name(a: AlphaChannel) -> &'static str {
    match a {
        AlphaChannel::Undefined => tr("Undefined"),
        AlphaChannel::None => trc_none(),
        AlphaChannel::Straight => tr("Straight (unmatted)"),
        AlphaChannel::Premultiplied => tr("Premultiplied"),
        AlphaChannel::PremultipliedDirty => tr("Premultiplied (dirty)"),
    }
}

/// A Rotation, as its list names it.
fn rotation_name(r: MediaRotation) -> &'static str {
    match r {
        MediaRotation::Original => tr("0° (original)"),
        MediaRotation::Clockwise => tr("90° clockwise"),
        MediaRotation::Inverted => tr("180° (inverted)"),
        MediaRotation::Counterclockwise => tr("90° counterclockwise"),
    }
}

/// A Color range, as its list names it.
fn range_name(r: Range) -> &'static str {
    match r {
        Range::Undefined => tr("Undefined"),
        Range::Limited => tr("Limited"),
        Range::Full => tr("Full"),
    }
}

/// Settings files of a kind get when imported (Media Properties' "Save
/// settings to video profiles for future auto-detection"): a file whose
/// extension, frame size and frame rate are these.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VideoProfile {
    pub extension: String,
    pub width: u32,
    pub height: u32,
    pub rate: (u32, u32),
    /// 0 progressive, 1 upper field first, 2 lower.
    pub field_order: u8,
    pub pixel_aspect: PixelAspect,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_space: Option<ColorSpace>,
}

impl VideoProfile {
    /// The profile of `m`'s kind with these settings.
    fn of(
        m: &MediaInfo,
        field_order: bv_core::FieldOrder,
        pixel_aspect: PixelAspect,
        color_space: Option<ColorSpace>,
    ) -> Option<VideoProfile> {
        let MediaSource::File { path } = &m.source else {
            return None;
        };
        let v = m.video?;
        Some(VideoProfile {
            extension: extension(path),
            width: v.width,
            height: v.height,
            rate: (v.frame_rate.num, v.frame_rate.den),
            field_order: match field_order {
                bv_core::FieldOrder::Progressive => 0,
                bv_core::FieldOrder::UpperFirst => 1,
                bv_core::FieldOrder::LowerFirst => 2,
            },
            pixel_aspect,
            color_space,
        })
    }

    /// Whether `m` is of its kind.
    fn fits(&self, m: &MediaInfo) -> bool {
        let (MediaSource::File { path }, Some(v)) = (&m.source, m.video) else {
            return false;
        };
        !m.still
            && extension(path) == self.extension
            && (v.width, v.height) == (self.width, self.height)
            && (v.frame_rate.num, v.frame_rate.den) == self.rate
    }

    /// Its settings on `m`.
    pub(crate) fn apply(&self, m: &mut MediaInfo) {
        if let Some(v) = &mut m.video {
            v.field_order = match self.field_order {
                1 => bv_core::FieldOrder::UpperFirst,
                2 => bv_core::FieldOrder::LowerFirst,
                _ => bv_core::FieldOrder::Progressive,
            };
            v.pixel_aspect = self.pixel_aspect;
        }
        m.color_space = self.color_space;
    }
}

/// `list` with "(differing values)" first when `value` is one.
fn with_differing<T>(
    mut list: Vec<(Option<T>, String, bool)>,
    value: &Option<T>,
) -> Vec<(Option<T>, String, bool)> {
    if value.is_none() {
        list.insert(0, (None, tr("(differing values)").to_string(), true));
    }
    list
}

/// A path's extension, lowercase.
fn extension(path: &str) -> String {
    std::path::Path::new(path)
        .extension()
        .map_or_else(String::new, |e| e.to_string_lossy().to_lowercase())
}

/// The profile `profiles` has for `m`'s kind, if any.
pub(crate) fn profile_for<'a>(
    profiles: &'a [VideoProfile],
    m: &MediaInfo,
) -> Option<&'a VideoProfile> {
    profiles.iter().find(|p| p.fits(m))
}

/// "None", as an alpha channel's.
fn trc_none() -> &'static str {
    trc("alpha channel", "None")
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
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(title);
        let r = ui.available_rect_before_wrap();
        let y = r.center().y;
        ui.painter().hline(
            r.left()..=r.right(),
            y,
            Stroke::new(1.0, theme::separator()),
        );
    });
}

/// The Stream list: the picture ("Video 1", or "Still 1" for a picture
/// file), then each audio stream.
fn streams(m: &MediaInfo) -> Vec<(Stream, String, bool)> {
    let mut out = Vec::new();
    if m.video.is_some() {
        let name = if m.still {
            trf!("Still {}", 1)
        } else {
            trf!("Video {}", 1)
        };
        out.push((Stream::Video, name, true));
    }
    let audio = m.audio.iter().count() + m.extra_audio.len();
    for k in 0..audio {
        out.push((Stream::Audio(k), trf!("Audio {}", k + 1), true));
    }
    out
}

/// The file's format as VEGAS writes it: its kind ("MP4 Base Media v1",
/// its major brand's; else its container's or picture's) and the stream's
/// compressed format ("AVC").
fn format_text(
    m: &MediaInfo,
    about: Option<&bv_engine::FileDescription>,
    stream: Stream,
) -> String {
    let Some(d) = about else {
        return tr("Generated media").to_string();
    };
    let kind = match d.brand {
        Some(b) => crate::media_general::brand_name(b),
        None if !d.format.is_empty() => d.format.clone(),
        None => match &m.source {
            MediaSource::File { path } => std::path::Path::new(path)
                .extension()
                .map(|x| x.to_string_lossy().to_uppercase())
                .unwrap_or_default(),
            MediaSource::Generated(_) => String::new(),
        },
    };
    let codec = match stream {
        Stream::Video => d
            .video_codec
            .as_deref()
            .map(crate::media_general::codec_name),
        Stream::Audio(k) => d.audio_codecs.get(k).cloned(),
    };
    match codec {
        Some(c) if !c.is_empty() && c != kind => format!("{kind} - {c}"),
        _ => kind,
    }
}

/// A stream's attributes, as VEGAS writes them: a picture's size and depth
/// and the file's length, or a sound's rate and channels.
fn attributes(m: &MediaInfo, stream: Stream, rate: FrameRate) -> String {
    let length = timecode::format(m.duration, rate);
    match stream {
        // The file's own frames' size, before Rotation.
        Stream::Video => m.video.map_or(String::new(), |v| {
            let (w, h) = v.coded_size();
            format!("{w}x{h}x32; {length}")
        }),
        Stream::Audio(k) => {
            let a = if k == 0 {
                m.audio
            } else {
                m.extra_audio.get(k - 1).copied()
            };
            a.map_or(String::new(), |a| {
                format!(
                    "{} Hz; {}; {}",
                    crate::fmt::grouped(u64::from(a.sample_rate.0)),
                    channels(a.channels),
                    length
                )
            })
        }
    }
}

fn channels(n: u16) -> String {
    match n {
        1 => tr("Mono").to_string(),
        2 => tr("Stereo").to_string(),
        n => trn!(n, "{} channel", "{} channels", n),
    }
}

/// A frame rate as the lists write it ("24,000 (Film)").
fn rate_text(rate: FrameRate) -> String {
    let fps = crate::fmt::num(rate.as_f64(), 3);
    match crate::project_properties::RATES
        .iter()
        .find(|(r, _)| *r == rate)
    {
        Some((_, name)) => format!("{fps} ({})", tr(name)),
        None => fps,
    }
}

/// The timecode format a file's rate reads in.
fn timecode_format(m: &MediaInfo) -> RulerFormat {
    let Some(v) = m.video.filter(|_| !m.still) else {
        return RulerFormat::TimeAndFrames;
    };
    match v.frame_rate {
        r if r == FrameRate::FPS_23_976 => RulerFormat::SmpteFilmSyncIvtc,
        r if r == FrameRate::FPS_24 => RulerFormat::SmpteFilm,
        r if r == FrameRate::FPS_25 || r == FrameRate::FPS_50 => RulerFormat::SmpteEbu,
        r if r == FrameRate::FPS_29_97 || r == FrameRate::FPS_59_94 => RulerFormat::SmpteDrop,
        _ => RulerFormat::Smpte30,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A video file of the project (its kind: an `.mp4` 1920×1080 at the
    /// project's rate).
    fn video(app: &mut BananaApp, name: &str, order: bv_core::FieldOrder) -> MediaId {
        let mut info =
            MediaInfo::generated(bv_timeline::Generator::ColorBars, &app.project().settings);
        info.source = MediaSource::File {
            path: format!("{name}.mp4"),
        };
        info.still = false;
        info.video.as_mut().unwrap().field_order = order;
        app.modify(|p| p.add_media(info))
    }

    /// The Media tab's reading lists applied, one step of the undo
    /// history: a quarter turn swaps the stored width and height (the
    /// turned picture's), the file's own size still told; Alpha channel,
    /// its Background color and Limited kept in its reading.
    #[test]
    fn rotation_alpha_and_range_are_kept() {
        let mut app = BananaApp::new(egui::Context::default());
        let id = video(&mut app, "phone", bv_core::FieldOrder::Progressive);
        let before = app.project().media(id).unwrap().video.unwrap();
        let mut d = MediaPropertiesDialog::new(&app, id).expect("a dialog");
        d.reading.rotation = MediaRotation::Clockwise;
        d.reading.alpha = AlphaChannel::PremultipliedDirty;
        d.reading.background = [255, 255, 255];
        d.reading.range = Range::Limited;
        let m = app.project().media(id).unwrap().clone();
        assert!(d.apply(&mut app, &m));
        let v = app.project().media(id).unwrap().video.unwrap();
        assert_eq!((v.width, v.height), (before.height, before.width), "turned");
        assert_eq!(v.coded_size(), (before.width, before.height));
        assert_eq!(v.reading.rotation, MediaRotation::Clockwise);
        assert_eq!(v.reading.alpha, AlphaChannel::PremultipliedDirty);
        assert_eq!(v.reading.background, [255; 3]);
        assert!(v.reading.limited && !v.full_range);
        // Back to the file's way: as it was.
        let mut d = MediaPropertiesDialog::new(&app, id).expect("a dialog");
        assert_eq!(d.reading.rotation, MediaRotation::Clockwise);
        d.reading.rotation = MediaRotation::Original;
        d.reading.range = Range::Full;
        let m = app.project().media(id).unwrap().clone();
        assert!(d.apply(&mut app, &m));
        let v = app.project().media(id).unwrap().video.unwrap();
        assert_eq!((v.width, v.height), (before.width, before.height));
        assert!(v.full_range && !v.reading.limited);
        app.undo();
        app.undo();
        assert_eq!(app.project().media(id).unwrap().video.unwrap(), before);
    }

    /// Several media: a setting they do not share is "(differing values)"
    /// and stays as each has it; one changed changes them all. A profile
    /// gives files of its kind its settings.
    #[test]
    fn several_media_and_profiles() {
        use bv_core::FieldOrder;
        let mut app = BananaApp::new(egui::Context::default());
        let a = video(&mut app, "a", FieldOrder::UpperFirst);
        let b = video(&mut app, "b", FieldOrder::Progressive);
        let mut d = MediaPropertiesDialog::new_many(&app, &[a, b]).expect("a dialog");
        assert_eq!(d.field_order, None, "differing");
        assert_eq!(d.pixel_aspect, Some(PixelAspect::SQUARE), "shared");
        d.pixel_aspect = Some(PixelAspect::new(4, 3));
        let m = app.project().media(a).unwrap().clone();
        assert!(d.apply(&mut app, &m));
        let p = app.project();
        let order = |id| p.media(id).unwrap().video.unwrap().field_order;
        assert_eq!(
            (order(a), order(b)),
            (FieldOrder::UpperFirst, FieldOrder::Progressive)
        );
        assert!(
            [a, b]
                .iter()
                .all(|&id| p.media(id).unwrap().video.unwrap().pixel_aspect
                    == PixelAspect::new(4, 3))
        );
        // A profile of the kind: lower field first.
        let profile =
            VideoProfile::of(&m, FieldOrder::LowerFirst, PixelAspect::SQUARE, None).unwrap();
        let mut other = p.media(b).unwrap().clone();
        assert!(profile_for(std::slice::from_ref(&profile), &other).is_some());
        profile.apply(&mut other);
        assert_eq!(other.video.unwrap().field_order, FieldOrder::LowerFirst);
        other.source = MediaSource::File {
            path: "c.mov".into(),
        };
        assert!(profile_for(&[profile], &other).is_none(), "another kind");
    }
}
