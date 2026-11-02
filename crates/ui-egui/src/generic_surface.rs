//! The Generic Control surface (Options › Preferences › External Control
//! & Automation, VEGAS's): any MIDI controller's knobs, faders and buttons
//! doing what its mapping says — learnt from the controller (Learn: the
//! next message it sends) or typed (Edit) —, up to five of them. Channel
//! strips (Number of channels) are banked over the audio tracks or, in
//! Video mode, the video ones: Fader, Pan, Send, Mute and Solo; the
//! transport; Set Audio Mode, Set Video Mode and Toggle Automation Mode:
//! trims, or (in automation mode) the tracks' automation controls,
//! recorded while playing as their headers' sliders do. Effect parameters
//! are not controlled, as VEGAS's are not.

use std::time::{Duration, Instant};

use bv_core::Tick;
use bv_timeline::{TrackId, TrackKind};
use serde::{Deserialize, Serialize};

use crate::app::BananaApp;
use crate::automation::TrackControl;
use crate::egui::{self, RichText, vec2};
use crate::i18n::tr;
use crate::settings::ControlDevice;
use crate::surface::fader_to_gain;

/// How many generic controllers BananaV takes, as VEGAS.
pub(crate) const MAX_GENERIC: usize = 5;
/// The most channel strips a generic controller has.
const MAX_CHANNELS: u8 = 32;
/// In automation mode, a knob untouched this long is let go (a Touch
/// recording ends): generic controls do not say when they are let go.
const LET_GO: Duration = Duration::from_millis(600);

/// A MIDI message's kind, as the mapping names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Message {
    ControlChange,
    Note,
    PitchBend,
    ProgramChange,
}

impl Message {
    pub const ALL: [Message; 4] = [
        Message::ControlChange,
        Message::Note,
        Message::PitchBend,
        Message::ProgramChange,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Message::ControlChange => tr("Control Change"),
            Message::Note => tr("Note"),
            Message::PitchBend => tr("Pitch Bend"),
            Message::ProgramChange => tr("Program Change"),
        }
    }
}

/// What a control of the controller does (VEGAS's host commands); channel
/// strips count from 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Command {
    Play,
    Stop,
    Pause,
    Record,
    GoToStart,
    GoToEnd,
    LoopPlayback,
    BankUp,
    BankDown,
    MasterFader,
    AudioMode,
    VideoMode,
    ToggleAutomation,
    AudioFader(u8),
    AudioPan(u8),
    AudioSend(u8),
    AudioMute(u8),
    AudioSolo(u8),
    VideoFader(u8),
    VideoMute(u8),
    VideoSolo(u8),
}

/// Which mode a command answers in: always, or only in Audio or Video
/// mode (a control may do one thing in each).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Always,
    Audio,
    Video,
}

impl Command {
    pub fn name(self) -> String {
        use Command::*;
        match self {
            Play => tr("Play").into(),
            Stop => tr("Stop").into(),
            Pause => tr("Pause").into(),
            Record => tr("Record").into(),
            GoToStart => tr("Go to Start").into(),
            GoToEnd => tr("Go to End").into(),
            LoopPlayback => tr("Loop Playback").into(),
            BankUp => tr("Channel Bank Up").into(),
            BankDown => tr("Channel Bank Down").into(),
            MasterFader => tr("Master Fader").into(),
            AudioMode => tr("Set Audio Mode").into(),
            VideoMode => tr("Set Video Mode").into(),
            ToggleAutomation => tr("Toggle Automation Mode").into(),
            AudioFader(n) | VideoFader(n) => trf!("Channel {} Fader", n + 1),
            AudioPan(n) => trf!("Channel {} Pan", n + 1),
            AudioSend(n) => trf!("Channel {} Send", n + 1),
            AudioMute(n) | VideoMute(n) => trf!("Channel {} Mute", n + 1),
            AudioSolo(n) | VideoSolo(n) => trf!("Channel {} Solo", n + 1),
        }
    }

    /// A fader or a knob (it gives a value), else a button (it is pressed).
    fn continuous(self) -> bool {
        use Command::*;
        matches!(
            self,
            MasterFader | AudioFader(_) | AudioPan(_) | AudioSend(_) | VideoFader(_)
        )
    }

    fn class(self) -> Class {
        use Command::*;
        match self {
            AudioFader(_) | AudioPan(_) | AudioSend(_) | AudioMute(_) | AudioSolo(_) => {
                Class::Audio
            }
            VideoFader(_) | VideoMute(_) | VideoSolo(_) => Class::Video,
            _ => Class::Always,
        }
    }
}

/// The Configure Generic Control dialog's View function group list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Group {
    #[default]
    Transport,
    Channels,
    Assign,
    AudioChannels,
    VideoChannels,
}

impl Group {
    const ALL: [Group; 5] = [
        Group::Transport,
        Group::Channels,
        Group::Assign,
        Group::AudioChannels,
        Group::VideoChannels,
    ];

    fn name(self) -> &'static str {
        match self {
            Group::Transport => tr("Transport"),
            Group::Channels => tr("Channels"),
            Group::Assign => tr("Assign"),
            Group::AudioChannels => tr("Audio Channels"),
            Group::VideoChannels => tr("Video Channels"),
        }
    }

    /// Its host commands, for `channels` channel strips.
    fn commands(self, channels: u8) -> Vec<Command> {
        use Command::*;
        let strips = 0..channels.clamp(1, MAX_CHANNELS);
        match self {
            Group::Transport => vec![Play, Stop, Pause, Record, GoToStart, GoToEnd, LoopPlayback],
            Group::Channels => vec![BankUp, BankDown, MasterFader],
            Group::Assign => vec![AudioMode, VideoMode, ToggleAutomation],
            Group::AudioChannels => strips
                .flat_map(|n| {
                    [
                        AudioFader(n),
                        AudioPan(n),
                        AudioSend(n),
                        AudioMute(n),
                        AudioSolo(n),
                    ]
                })
                .collect(),
            Group::VideoChannels => strips
                .flat_map(|n| [VideoFader(n), VideoMute(n), VideoSolo(n)])
                .collect(),
        }
    }
}

/// A control's MIDI message: its channel (0–15), kind and data (the
/// controller's or the note's number; 0 for pitch bend).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub command: Command,
    pub channel: u8,
    pub message: Message,
    pub data: u8,
}

impl Binding {
    fn answers(&self, m: &Incoming) -> bool {
        self.channel == m.channel
            && self.message == m.message
            && (self.message == Message::PitchBend || self.data == m.data)
    }
}

/// A Generic Control's mapping (Save As keeps it in a `.bvgeneric` file).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GenericMap {
    /// Number of channels: its channel strips.
    pub channels: u8,
    pub bindings: Vec<Binding>,
}

impl Default for GenericMap {
    fn default() -> GenericMap {
        GenericMap {
            channels: 8,
            bindings: Vec::new(),
        }
    }
}

impl GenericMap {
    pub fn binding(&self, c: Command) -> Option<&Binding> {
        self.bindings.iter().find(|b| b.command == c)
    }

    /// Binds `b`'s command to its message; what the message did in the
    /// same mode (or always) is let go.
    pub fn set(&mut self, b: Binding) {
        let class = b.command.class();
        self.bindings.retain(|o| {
            let same = o.channel == b.channel
                && o.message == b.message
                && (b.message == Message::PitchBend || o.data == b.data);
            let clash = class == Class::Always
                || o.command.class() == Class::Always
                || o.command.class() == class;
            o.command != b.command && !(same && clash)
        });
        self.bindings.push(b);
    }

    /// `c` bound to the message `m` is.
    pub(crate) fn learn(&mut self, c: Command, m: &Incoming) {
        self.set(Binding {
            command: c,
            channel: m.channel,
            message: m.message,
            data: if m.message == Message::PitchBend {
                0
            } else {
                m.data
            },
        });
    }

    pub fn reset(&mut self, c: Command) {
        self.bindings.retain(|b| b.command != c);
    }

    /// What message `m` does now (`video`: in Video mode).
    pub(crate) fn find(&self, m: &Incoming, video: bool) -> Option<Command> {
        self.bindings
            .iter()
            .filter(|b| b.answers(m))
            .map(|b| b.command)
            .find(|c| match c.class() {
                Class::Always => true,
                Class::Audio => !video,
                Class::Video => video,
            })
    }
}

/// A message the controller sent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Incoming {
    pub channel: u8,
    pub message: Message,
    pub data: u8,
    /// Its value, 0–1 (a pitch bend's 14 bits).
    pub value: f32,
    /// −1 to 1 around its middle (a pan).
    pub centered: f32,
    /// A button pressed (not let go).
    pub press: bool,
}

/// The message in `bytes`, if a controller's.
pub(crate) fn incoming([status, a, b]: [u8; 3]) -> Option<Incoming> {
    let channel = status & 0x0F;
    let seven = |v: u8, press: bool, message, data| Incoming {
        channel,
        message,
        data,
        value: v as f32 / 127.0,
        centered: ((v as f32 - 64.0) / 63.0).clamp(-1.0, 1.0),
        press,
    };
    match status & 0xF0 {
        0xB0 => Some(seven(b, b >= 64, Message::ControlChange, a)),
        0x90 if b > 0 => Some(seven(b, true, Message::Note, a)),
        0x80 | 0x90 => Some(seven(0, false, Message::Note, a)),
        0xE0 => {
            let v = (a as u16) | (b as u16) << 7;
            Some(Incoming {
                channel,
                message: Message::PitchBend,
                data: 0,
                value: v as f32 / 16383.0,
                centered: ((v as f32 - 8192.0) / 8191.0).clamp(-1.0, 1.0),
                press: v >= 8192,
            })
        }
        0xC0 => Some(seven(127, true, Message::ProgramChange, a)),
        _ => None,
    }
}

/// A generic controller's state on BananaV's side.
#[derive(Debug, Default)]
pub(crate) struct GenericState {
    /// Video mode (Set Video Mode): the strips control video tracks.
    pub video: bool,
    /// Automation mode (Toggle Automation Mode): the strips move the
    /// tracks' automation controls, else their trims.
    pub automation: bool,
    /// Controls moved in automation mode, and when last.
    held: Vec<(TrackId, TrackControl, Instant)>,
}

impl GenericState {
    /// `control` of `track` moved now (in automation): let go a moment
    /// after it stops (a Mackie Control's V-Pots too).
    pub(crate) fn hold(&mut self, track: TrackId, control: TrackControl) {
        self.held.retain(|(t, c, _)| (*t, *c) != (track, control));
        self.held.push((track, control, Instant::now()));
    }
}

impl BananaApp {
    /// The track channel strip `n` of surface `index` controls now: the
    /// `n`th track of `kind` from its bank.
    pub(crate) fn generic_track(&self, index: usize, n: u8, kind: TrackKind) -> Option<TrackId> {
        let bank = self.surfaces.get(index).map_or(0, |s| s.bank).max(0) as usize;
        self.project()
            .tracks
            .iter()
            .filter(|t| t.kind == kind)
            .nth(bank + n as usize)
            .map(|t| t.id)
    }

    /// A message from generic controller `index`.
    pub(crate) fn generic_message(&mut self, index: usize, device: &ControlDevice, bytes: [u8; 3]) {
        use Command::*;
        let Some(m) = incoming(bytes) else {
            return;
        };
        let video = self.surfaces[index].generic.video;
        let Some(c) = device.generic.find(&m, video) else {
            return;
        };
        if !c.continuous() && !m.press {
            return;
        }
        let key = egui::Id::new(("generico", index, c));
        let channels = device.generic.channels.clamp(1, MAX_CHANNELS) as i64;
        let audio = |app: &BananaApp, n| app.generic_track(index, n, TrackKind::Audio);
        let video_track = |app: &BananaApp, n| app.generic_track(index, n, TrackKind::Video);
        match c {
            Play => {
                if !self.is_playing() {
                    self.toggle_play();
                }
            }
            Stop => self.stop(),
            Pause => self.toggle_pause(),
            Record => self.toggle_recording(),
            GoToStart => self.seek_playback(Tick::ZERO),
            GoToEnd => self.go_end(),
            LoopPlayback => crate::commands::Cmd::LoopPlayback.run(self),
            BankUp => {
                let s = &mut self.surfaces[index];
                s.bank = (s.bank - channels).max(0);
            }
            BankDown => self.surfaces[index].bank += channels,
            MasterFader => {
                let db = gain_db(fader_gain(m.value));
                self.modify_merged(key, |p| p.master.gain_db = db);
            }
            AudioMode => self.surfaces[index].generic.video = false,
            VideoMode => self.surfaces[index].generic.video = true,
            ToggleAutomation => self.surfaces[index].generic.automation ^= true,
            AudioFader(n) => {
                if let Some(t) = audio(self, n) {
                    self.generic_set(index, t, TrackControl::Volume, fader_gain(m.value), key);
                }
            }
            AudioPan(n) => {
                if let Some(t) = audio(self, n) {
                    self.generic_set(index, t, TrackControl::Pan, m.centered, key);
                }
            }
            AudioSend(n) => {
                // The track's first send.
                if let Some(t) = audio(self, n) {
                    let db = gain_db(fader_gain(m.value));
                    self.modify_merged(key, |p| {
                        if let Some(s) = p.track_by_id_mut(t).and_then(|t| t.sends.first_mut()) {
                            s.gain_db = db;
                        }
                    });
                }
            }
            VideoFader(n) => {
                if let Some(t) = video_track(self, n) {
                    self.generic_set(index, t, TrackControl::Level, m.value, key);
                }
            }
            AudioMute(n) | VideoMute(n) | AudioSolo(n) | VideoSolo(n) => {
                let track = if matches!(c, AudioMute(_) | AudioSolo(_)) {
                    audio(self, n)
                } else {
                    video_track(self, n)
                };
                let mute = matches!(c, AudioMute(_) | VideoMute(_));
                if let Some(t) = track {
                    self.modify(|p| {
                        if let Some(tr) = p.track_by_id_mut(t) {
                            if mute {
                                tr.muted ^= true;
                            } else {
                                tr.solo ^= true;
                            }
                        }
                    });
                }
            }
        }
    }

    /// A strip's fader or knob: the track's trim, or in automation mode its
    /// automation control (recorded while playing, as the headers' sliders
    /// do).
    fn generic_set(
        &mut self,
        index: usize,
        track: TrackId,
        control: TrackControl,
        value: f32,
        key: egui::Id,
    ) {
        if self.surfaces[index].generic.automation {
            self.set_control(track, control, value, key);
            self.surfaces[index].generic.hold(track, control);
            return;
        }
        self.modify_merged(key, |p| {
            crate::automation::set_trim(
                p,
                crate::automation::AutoOwner::Track(track),
                control,
                value,
            )
        });
    }

    /// Each frame: controls of generic controller `index` left alone a
    /// moment are let go (a Touch recording ends).
    pub(crate) fn generic_let_go(&mut self, index: usize) {
        let now = Instant::now();
        let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.surfaces[index].generic.held)
            .into_iter()
            .partition(|(_, _, at)| now.duration_since(*at) >= LET_GO);
        self.surfaces[index].generic.held = kept;
        for (t, c, _) in gone {
            self.release_control(t, c);
        }
    }
}

/// A fader's gain for a value 0–1 (the headers' taper).
fn fader_gain(value: f32) -> f32 {
    fader_to_gain((value.clamp(0.0, 1.0) * 16383.0).round() as u16)
}

fn gain_db(gain: f32) -> f32 {
    if gain <= 1e-5 {
        -96.0
    } else {
        20.0 * gain.log10()
    }
}

/// What the Configure Generic Control dialog shows between frames.
#[derive(Debug, Default)]
pub(crate) struct GenericUi {
    group: Group,
    picked: Option<Command>,
    learn: bool,
    /// The Edit MIDI Message window: the binding edited.
    edit: Option<Binding>,
}

/// The Configure Generic Control dialog for `d` (its MIDI input among
/// `inputs`); false once it is closed. With Learn on, the next message its
/// input sends binds the host command picked.
pub(crate) fn window(
    ctx: &egui::Context,
    app: &mut BananaApp,
    d: &mut ControlDevice,
    inputs: &[String],
    s: &mut GenericUi,
) -> bool {
    // Learn: the input's messages come to the dialog.
    if s.learn
        && let Some(port) = d.input.clone()
    {
        app.midi_learn = Some(port.clone());
        let learnt: Vec<[u8; 3]> = app
            .midi
            .learned
            .lock()
            .map(|mut q| std::mem::take(&mut *q))
            .unwrap_or_default()
            .into_iter()
            .filter(|(p, _)| *p == port)
            .map(|(_, b)| b)
            .collect();
        if let Some(c) = s.picked {
            // A note let go teaches nothing.
            if let Some(m) = learnt
                .iter()
                .filter_map(|b| incoming(*b))
                .rfind(|m| m.message != Message::Note || m.press)
            {
                d.generic.learn(c, &m);
            }
        }
    }
    let mut open = true;
    let mut close = false;
    let none = tr("(None)");
    crate::floating::Window::new(tr("Configure Generic Control"))
        .id(egui::Id::new("preferencias-generico"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_min_width(520.0);
            ui.spacing_mut().item_spacing.y = 6.0;
            egui::Grid::new("generico-topo")
                .num_columns(2)
                .spacing(vec2(10.0, 6.0))
                .show(ui, |ui| {
                    ui.label(tr("MIDI input:"));
                    egui::ComboBox::from_id_salt("generico-entrada")
                        .width(240.0)
                        .truncate()
                        .selected_text(d.input.clone().unwrap_or_else(|| none.to_string()))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut d.input, None, none);
                            for p in inputs {
                                ui.selectable_value(&mut d.input, Some(p.clone()), p);
                            }
                        });
                    ui.end_row();
                    ui.label(tr("Number of channels:"));
                    let mut n = d.generic.channels as f64;
                    crate::widgets::spin_box(ui, &mut n, 1.0..=MAX_CHANNELS as f64, 1.0, 0, 70.0);
                    d.generic.channels = n.round() as u8;
                    ui.end_row();
                    ui.label(tr("View function group:"));
                    egui::ComboBox::from_id_salt("generico-grupo")
                        .width(240.0)
                        .selected_text(s.group.name())
                        .show_ui(ui, |ui| {
                            for g in Group::ALL {
                                ui.selectable_value(&mut s.group, g, g.name());
                            }
                        });
                    ui.end_row();
                });
            // The host commands of the group, with their messages.
            let commands = s.group.commands(d.generic.channels);
            egui::Frame::new()
                .stroke(egui::Stroke::new(1.0, crate::theme::separator()))
                .inner_margin(4.0)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(260.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            egui::Grid::new("generico-comandos")
                                .num_columns(4)
                                .striped(true)
                                .min_col_width(60.0)
                                .show(ui, |ui| {
                                    for h in [
                                        tr("Host Command"),
                                        tr("Channel"),
                                        tr("MIDI Message"),
                                        tr("MIDI Data"),
                                    ] {
                                        ui.label(RichText::new(h).strong());
                                    }
                                    ui.end_row();
                                    for c in &commands {
                                        let b = d.generic.binding(*c);
                                        let picked = s.picked == Some(*c);
                                        if ui.add(
                                                egui::Button::selectable(picked, c.name())
                                                    .min_size(vec2(200.0, 18.0))
                                                    .right_text(""),
                                            )
                                            .clicked()
                                        {
                                            s.picked = Some(*c);
                                        }
                                        ui.label(b.map_or(String::new(), |b| {
                                            (b.channel + 1).to_string()
                                        }));
                                        ui.label(b.map_or("", |b| b.message.name()));
                                        ui.label(b.map_or(String::new(), |b| {
                                            if b.message == Message::PitchBend {
                                                String::new()
                                            } else {
                                                b.data.to_string()
                                            }
                                        }));
                                        ui.end_row();
                                    }
                                });
                        });
                });
            ui.horizontal(|ui| {
                crate::widgets::checkbox(ui, &mut s.learn, tr("Learn"))
                    .on_hover_text(tr(
                        "Pick a host command, then move or press the control that should do it.",
                    ));
                let picked = s.picked;
                if ui
                    .add_enabled(picked.is_some(), egui::Button::new(tr("Edit…")))
                    .clicked()
                    && let Some(c) = picked
                {
                    s.edit = Some(d.generic.binding(c).copied().unwrap_or(Binding {
                        command: c,
                        channel: 0,
                        message: Message::ControlChange,
                        data: 0,
                    }));
                }
                if ui
                    .add_enabled(picked.is_some(), egui::Button::new(tr("Reset")))
                    .clicked()
                    && let Some(c) = picked
                {
                    d.generic.reset(c);
                }
                if ui.button(tr("Reset All")).clicked() {
                    d.generic.bindings.clear();
                }
                if ui.button(tr("Save As…")).clicked() {
                    save_map(&d.generic);
                }
                if ui.button(tr("Open…")).clicked()
                    && let Some(m) = open_map()
                {
                    d.generic = m;
                }
            });
            if s.learn && d.input.is_none() {
                crate::tabs::hint(ui, tr("Choose its MIDI input to learn from it."));
            }
            crate::tabs::hint(
                ui,
                tr(
                    "Options › External Control turns it on. Set Audio Mode and Set Video Mode pick which tracks the channels control; Toggle Automation Mode moves their automation controls instead of their trims. Effect parameters are not controlled.",
                ),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("Close")).clicked() {
                    close = true;
                }
            });
        });
    if let Some(b) = &mut s.edit {
        let mut keep = true;
        let (mut ok, mut cancel) = (false, false);
        crate::floating::Window::new(tr("Edit MIDI Message"))
            .id(egui::Id::new("preferencias-generico-editar"))
            .collapsible(false)
            .resizable(false)
            .open(&mut keep)
            .show(ctx, |ui| {
                ui.label(RichText::new(b.command.name()).strong());
                egui::Grid::new("generico-editar")
                    .num_columns(2)
                    .spacing(vec2(10.0, 6.0))
                    .show(ui, |ui| {
                        ui.label(tr("Channel:"));
                        let mut ch = (b.channel + 1) as f64;
                        crate::widgets::spin_box(ui, &mut ch, 1.0..=16.0, 1.0, 0, 70.0);
                        b.channel = (ch.round() as u8).saturating_sub(1);
                        ui.end_row();
                        ui.label(tr("MIDI message:"));
                        egui::ComboBox::from_id_salt("generico-mensagem")
                            .width(160.0)
                            .selected_text(b.message.name())
                            .show_ui(ui, |ui| {
                                for m in Message::ALL {
                                    ui.selectable_value(&mut b.message, m, m.name());
                                }
                            });
                        ui.end_row();
                        ui.label(tr("MIDI data:"));
                        let mut data = b.data as f64;
                        ui.add_enabled_ui(b.message != Message::PitchBend, |ui| {
                            crate::widgets::spin_box(ui, &mut data, 0.0..=127.0, 1.0, 0, 70.0);
                        });
                        b.data = data.round() as u8;
                        ui.end_row();
                    });
                ui.horizontal(|ui| {
                    ok = ui.button(tr("OK")).clicked();
                    cancel = ui.button(tr("Cancel")).clicked();
                });
            });
        if ok {
            d.generic.set(*b);
        }
        if ok || cancel || !keep {
            s.edit = None;
        }
    }
    let open = open && !close;
    if !open {
        s.learn = false;
        s.edit = None;
    }
    open
}

impl BananaApp {
    /// Options › Preferences › External Control & Automation with a Generic
    /// Control whose first channel's fader and pan and Channel Bank Down
    /// are mapped, its Configure Generic Control open on Audio Channels
    /// (screen tests; the settings in memory).
    pub fn generic_control_for_review(&mut self) {
        self.settings = crate::settings::UiSettings::in_memory();
        let mut map = GenericMap::default();
        for (c, bytes) in [
            (Command::AudioFader(0), [0xB0, 21, 0]),
            (Command::AudioPan(0), [0xB0, 22, 0]),
            (Command::BankDown, [0x91, 40, 127]),
        ] {
            if let Some(m) = incoming(bytes) {
                map.learn(c, &m);
            }
        }
        self.settings.prefs.control_devices.push(ControlDevice {
            kind: crate::settings::SurfaceKind::Generic,
            input: None,
            output: None,
            first_track: 0,
            generic: map,
            extender: false,
            mackie_map: Vec::new(),
        });
        let mut d = crate::preferences::PreferencesDialog::new(self);
        d.generic_for_review(
            0,
            GenericUi {
                group: Group::AudioChannels,
                picked: Some(Command::AudioFader(0)),
                learn: false,
                edit: None,
            },
        );
        self.menu_dialogs.more.preferences = Some(d);
    }
}

/// Save As: the mapping into a `.bvgeneric` file.
fn save_map(map: &GenericMap) {
    let Some(path) = rfd::FileDialog::new()
        .set_title(tr("Save As"))
        .add_filter(tr("Generic control mapping"), &["bvgeneric"])
        .set_file_name("Generic Control.bvgeneric")
        .save_file()
    else {
        return;
    };
    if let Ok(text) = serde_json::to_string_pretty(map) {
        let _ = std::fs::write(path, text);
    }
}

/// Open: a mapping from a `.bvgeneric` file.
fn open_map() -> Option<GenericMap> {
    let path = rfd::FileDialog::new()
        .set_title(tr("Open"))
        .add_filter(tr("Generic control mapping"), &["bvgeneric"])
        .pick_file()?;
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::SurfaceKind;

    fn device(map: GenericMap) -> ControlDevice {
        ControlDevice {
            kind: SurfaceKind::Generic,
            input: Some("Keys".into()),
            output: None,
            first_track: 0,
            generic: map,
            extender: false,
            mackie_map: Vec::new(),
        }
    }

    #[test]
    fn messages_are_read_as_the_mapping_names_them() {
        let cc = incoming([0xB2, 7, 127]).unwrap();
        assert_eq!(
            (cc.channel, cc.message, cc.data),
            (2, Message::ControlChange, 7)
        );
        assert!(cc.press && (cc.value - 1.0).abs() < 1e-6);
        let off = incoming([0x90, 60, 0]).unwrap();
        assert!(!off.press && off.message == Message::Note);
        let bend = incoming([0xE0, 0, 64]).unwrap();
        assert_eq!(bend.message, Message::PitchBend);
        assert!(bend.centered.abs() < 1e-6);
        assert!(incoming([0xF8, 0, 0]).is_none());
    }

    #[test]
    fn a_control_does_one_thing_in_each_mode() {
        let mut map = GenericMap::default();
        let knob = incoming([0xB0, 21, 10]).unwrap();
        map.learn(Command::AudioFader(0), &knob);
        map.learn(Command::VideoFader(0), &knob);
        assert_eq!(map.find(&knob, false), Some(Command::AudioFader(0)));
        assert_eq!(map.find(&knob, true), Some(Command::VideoFader(0)));
        // Learnt again for an audio command: the audio fader lets it go.
        map.learn(Command::AudioPan(0), &knob);
        assert!(map.binding(Command::AudioFader(0)).is_none());
        assert!(map.binding(Command::VideoFader(0)).is_some());
        // A command always on takes it from both.
        map.learn(Command::MasterFader, &knob);
        assert_eq!(map.bindings.len(), 1);
        map.reset(Command::MasterFader);
        assert!(map.bindings.is_empty());
    }

    #[test]
    fn a_generic_controller_moves_tracks_and_banks() {
        let mut app = BananaApp::new(crate::egui::Context::default());
        app.load_demo();
        let mut map = GenericMap {
            channels: 1,
            ..GenericMap::default()
        };
        let knob = [0xB0, 21, 0];
        let down = [0x90, 40, 127];
        map.learn(Command::AudioFader(0), &incoming(knob).unwrap());
        map.learn(Command::BankDown, &incoming(down).unwrap());
        map.learn(Command::VideoMode, &incoming([0x90, 41, 127]).unwrap());
        map.learn(Command::VideoFader(0), &incoming(knob).unwrap());
        let d = device(map);
        app.surfaces.resize_with(1, Default::default);
        let first = app.generic_track(0, 0, TrackKind::Audio).unwrap();
        // The knob at its top: the first audio track's trim at the top.
        app.generic_message(0, &d, [0xB0, 21, 127]);
        let level = app.project().track(first).unwrap().level;
        assert!((level - fader_to_gain(16383)).abs() < 1e-3, "{level}");
        // Video mode: the same knob is the first video track's level.
        app.generic_message(0, &d, [0x90, 41, 127]);
        let v = app.generic_track(0, 0, TrackKind::Video).unwrap();
        app.generic_message(0, &d, [0xB0, 21, 0]);
        assert!(app.project().track(v).unwrap().level.abs() < 1e-6);
        assert!((app.project().track(first).unwrap().level - level).abs() < 1e-6);
        // Bank down: the strip moves on by its one channel.
        app.generic_message(0, &d, down);
        assert_eq!(app.surfaces[0].bank, 1);
        // A note let go does nothing.
        app.generic_message(0, &d, [0x80, 40, 0]);
        assert_eq!(app.surfaces[0].bank, 1);
    }
}
