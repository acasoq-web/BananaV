//! Control surfaces (Options › Preferences › External Control &
//! Automation; Options › External Control turns them on): the Mackie
//! Control protocol, which many surfaces speak, laid out as VEGAS's Help
//! has it ("Using a Mackie Control"). A Mackie Control and up to four
//! Extenders, each over eight tracks of its own (Channel Mapping), banked
//! together: faders, V-Pots, REC, SOLO, MUTE and SELECT. The V-Pots do
//! what the Control buttons choose — Output (Compositing in Video mode),
//! Input, Pan (Fade to Color), Sends, Inserts (PL: each track's effects;
//! PE: one effect's parameters; PS: its chain, effects added to it) and
//! Settings (F1–F6 held: automation mode, pan mode, phase, input
//! monitoring, pre/post volume, the track defaults) —; Flip trades them
//! with the faders; Video sets the strips over the video tracks, and
//! Shift with Meters/Values over the tracks, the busses (their faders,
//! pans, mutes, solos and selection) or both. The
//! master fader; transport, markers, regions, the loop region, event
//! trimming and the jog wheel; the Add New, Windows, View, Audio/Video,
//! Project and Timeline buttons. Option with F1–F16, the footswitches and
//! any button do what Configure Mackie Control maps (`mackie_config.rs`).
//! BananaV sends back the faders' positions (for motorized ones), the
//! buttons' lights, the V-Pots' rings, the strips' names and values (or
//! meters), the assignment display and the time in the ruler's format.
//!
//! Faders move tracks the way their headers' sliders do (see
//! `automation.rs`): trims, or with Automation on their automation
//! controls, recorded while playing.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use bv_core::Tick;
use bv_timeline::{
    AutomationMode, BusId, InputMonitor, Output, PanMode, SendTo, Strip, TrackId, TrackKind,
};

use crate::app::BananaApp;
use crate::automation::{AutoOwner, TrackControl};
use crate::commands::Cmd;
use crate::egui;
use crate::keymap::Extra;
use crate::settings::{ControlDevice, SurfaceKind};

/// Channel strips of a Mackie Control (and of an Extender).
pub(crate) const STRIPS: usize = 8;

/// Extenders beside a Mackie Control, as VEGAS takes them.
pub(crate) const MAX_EXTENDERS: usize = 4;

/// The highest gain a fader reaches (+12 dB), as the track headers'.
const MAX_GAIN: f32 = bv_timeline::envelope::MAX_VOLUME;

/// In Meters mode a value just changed shows this long.
const SHOW_VALUE: Duration = Duration::from_millis(1500);

/// Buttons, by the note they send, named as VEGAS's overlay labels them.
pub(crate) mod note {
    pub const REC: u8 = 0;
    pub const SOLO: u8 = 8;
    pub const MUTE: u8 = 16;
    pub const SELECT: u8 = 24;
    pub const VPOT_PUSH: u8 = 32;
    pub const OUTPUT: u8 = 40;
    pub const INPUT: u8 = 41;
    pub const PAN: u8 = 42;
    pub const SENDS: u8 = 43;
    pub const INSERTS: u8 = 44;
    pub const SETTINGS: u8 = 45;
    pub const BANK_LEFT: u8 = 46;
    pub const BANK_RIGHT: u8 = 47;
    pub const CHANNEL_LEFT: u8 = 48;
    pub const CHANNEL_RIGHT: u8 = 49;
    pub const FLIP: u8 = 50;
    pub const VIDEO: u8 = 51;
    pub const METERS: u8 = 52;
    pub const TIME_FORMAT: u8 = 53;
    pub const F1: u8 = 54;
    pub const ADD_TRACK: u8 = 62;
    pub const ADD_BUS: u8 = 63;
    pub const MASTER_BUS: u8 = 64;
    pub const VIDEO_PREVIEW: u8 = 65;
    pub const PLUG_INS: u8 = 66;
    pub const BUS_TRACKS: u8 = 67;
    pub const DOCK_AREA: u8 = 68;
    pub const TRACK_LIST: u8 = 69;
    pub const SHIFT: u8 = 70;
    pub const OPTION: u8 = 71;
    pub const CTRL: u8 = 72;
    pub const ALT: u8 = 73;
    pub const AUTOMATION: u8 = 74;
    pub const BYPASS_FX: u8 = 75;
    pub const METRONOME: u8 = 76;
    pub const SURROUND: u8 = 77;
    pub const DOWNMIX: u8 = 78;
    pub const DIM: u8 = 79;
    pub const SAVE: u8 = 80;
    pub const UNDO: u8 = 81;
    pub const OK: u8 = 82;
    pub const CANCEL: u8 = 83;
    pub const MARKER: u8 = 84;
    pub const REGION: u8 = 85;
    pub const LOOP: u8 = 86;
    pub const MARK_IN: u8 = 87;
    pub const MARK_OUT: u8 = 88;
    pub const EVENT_TRIM: u8 = 89;
    pub const RTZ: u8 = 90;
    pub const REWIND: u8 = 91;
    pub const FAST_FORWARD: u8 = 92;
    pub const STOP: u8 = 93;
    pub const PLAY: u8 = 94;
    pub const RECORD: u8 = 95;
    pub const UP: u8 = 96;
    pub const DOWN: u8 = 97;
    pub const LEFT: u8 = 98;
    pub const RIGHT: u8 = 99;
    pub const ZOOM: u8 = 100;
    pub const SCRUB: u8 = 101;
    pub const FOOT_A: u8 = 102;
    pub const FOOT_B: u8 = 103;
    pub const FADER_TOUCH: u8 = 104;
    /// The time display's SMPTE and BEATS lights.
    pub const SMPTE: u8 = 113;
    pub const BEATS: u8 = 114;
}

/// What the V-Pots do: the Control buttons' choice, shown on the
/// assignment display.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Assign {
    Output,
    Input,
    #[default]
    Pan,
    Sends,
    /// Inserts: each track's effects (PL).
    Plugins,
    /// One effect's parameters (PE).
    PluginEdit,
    /// A track's chain, effects added to it (PS).
    PluginChain,
    Settings,
}

impl Assign {
    /// The assignment display's two characters (VEGAS's: CO when Output
    /// sets the compositing modes).
    fn code(self, video: bool) -> [u8; 2] {
        match self {
            Assign::Output if video => *b"CO",
            Assign::Output => *b"OU",
            Assign::Input if video => *b"MB",
            Assign::Input => *b"IN",
            Assign::Pan if video => *b"FC",
            Assign::Pan => *b"PN",
            Assign::Sends => *b"SE",
            Assign::Plugins => *b"PL",
            Assign::PluginEdit => *b"PE",
            Assign::PluginChain => *b"PS",
            Assign::Settings => *b"ST",
        }
    }
}

/// PE: the effect edited — its track's `effect`th — and the page of its
/// parameters on V-Pots 1–4 (the first: Bypass), and the preset V-Pot 6
/// points at until pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FxEdit {
    pub track: TrackId,
    pub effect: usize,
    pub page: usize,
    pub preset: Option<usize>,
}

/// PS: the track's chain, three effects a page, and what V-Pots 1, 3 and
/// 5 point at: the effect there (0) or one to add there (the nth audio
/// effect, shown with a *).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ChainView {
    pub track: TrackId,
    pub page: usize,
    pub picks: [usize; 3],
}

/// What the strips control in Audio mode (Shift with Meters/Values goes
/// through them): the audio tracks, the busses, or the tracks and then
/// the busses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Channels {
    #[default]
    Tracks,
    Busses,
    Both,
}

impl Channels {
    fn next(self) -> Channels {
        match self {
            Channels::Tracks => Channels::Busses,
            Channels::Busses => Channels::Both,
            Channels::Both => Channels::Tracks,
        }
    }
}

/// A strip's channel: a track or a bus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Chan {
    Track(TrackId),
    Bus(BusId),
}

/// What a Mackie Control's state is on BananaV's side: the bank and
/// modes, the buttons held, and what was sent last (only changes go out).
#[derive(Debug, Default)]
pub(crate) struct Mackie {
    /// Strips moved along from the device's Channel Mapping (Bank and
    /// Channel buttons; a Mackie Control's Extenders move with it).
    pub bank: i64,
    shift: bool,
    option: bool,
    ctrl: bool,
    alt: bool,
    zoom: bool,
    scrub: bool,
    /// Rewind (-1) or Fast Forward (+1) held: the cursor runs.
    running: i8,
    pub assign: Assign,
    /// Flip: the faders do what the V-Pots did (Pan and Sends), the
    /// V-Pots set the volume.
    pub flip: bool,
    /// Video mode: the strips control the video tracks.
    pub video: bool,
    /// Audio mode: the strips over the tracks, the busses or both.
    pub channels: Channels,
    /// Automation: faders and V-Pots move the tracks' automation controls
    /// (recorded in Touch and Latch), else their trims.
    pub automation: bool,
    /// Meters/Values: the display's lower line shows meters.
    pub meters: bool,
    /// F1–F8 held (with Settings: what the V-Pots set).
    held_f: [bool; 8],
    /// Time Fmt held: the display shows the time format, V-Pot 8
    /// changes it.
    time_held: bool,
    /// Sends: the bus or chain the V-Pots send to (its place among them).
    pub send: usize,
    /// Output and Input: what each strip's V-Pot points at until pressed.
    choice: [Option<usize>; STRIPS],
    /// PL: the effect each strip shows.
    showing: [usize; STRIPS],
    pub edit: Option<FxEdit>,
    pub chain: Option<ChainView>,
    /// The strip whose value changed last, and when (shown in Meters mode).
    changed: Option<(usize, Instant)>,
    /// 5.1 projects: what Pan's V-Pots move — left to right (0), front to
    /// rear (1) or the center's level (2); Pan again moves on.
    axis: u8,
    touched: [bool; STRIPS + 1],
    faders: [Option<u16>; STRIPS + 1],
    lights: HashMap<u8, bool>,
    rings: [Option<u8>; STRIPS],
    assignment: Option<[u8; 2]>,
    lcd: Option<Vec<u8>>,
    clock: Option<[u8; 10]>,
    /// A Generic Control's modes and the controls it holds (and a Mackie
    /// Control's V-Pots held in automation, let go a moment later).
    pub generic: crate::generic_surface::GenericState,
}

/// A fader's 14-bit position for a gain (the headers' taper: 0 dB near
/// three quarters).
pub(crate) fn gain_to_fader(gain: f32) -> u16 {
    ((gain / MAX_GAIN).max(0.0).sqrt().min(1.0) * 16383.0).round() as u16
}

/// The gain of a fader's 14-bit position.
pub(crate) fn fader_to_gain(value: u16) -> f32 {
    let pos = value.min(16383) as f32 / 16383.0;
    pos * pos * MAX_GAIN
}

/// A V-Pot's turn: ticks, negative counterclockwise.
pub(crate) fn vpot_ticks(value: u8) -> i32 {
    let n = (value & 0x3F) as i32;
    if value & 0x40 != 0 { -n } else { n }
}

/// A letter as the display has it: accents left off, else `?`.
fn plain(c: char) -> char {
    if c.is_ascii() {
        return if c.is_ascii_control() { '?' } else { c };
    }
    match c {
        'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
        'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => 'A',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'É' | 'È' | 'Ê' | 'Ë' => 'E',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'Í' | 'Ì' | 'Î' | 'Ï' => 'I',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
        'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'O',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'Ú' | 'Ù' | 'Û' | 'Ü' => 'U',
        'ç' => 'c',
        'Ç' => 'C',
        'ñ' => 'n',
        'Ñ' => 'N',
        _ => '?',
    }
}

/// The 7-character cell of a strip on the display.
fn cell(text: &str) -> String {
    let mut s: String = text.chars().map(plain).take(6).collect();
    while s.chars().count() < 7 {
        s.push(' ');
    }
    s
}

/// A line of the display, 56 characters.
fn line(text: &str) -> String {
    let mut s: String = text.chars().map(plain).take(56).collect();
    while s.chars().count() < 56 {
        s.push(' ');
    }
    s
}

/// The display's system-exclusive message for a Mackie Control (or an
/// Extender): `text` from position `at`.
fn lcd_message(extender: bool, at: u8, text: &[u8]) -> Vec<u8> {
    let device = if extender { 0x15 } else { 0x14 };
    let mut m = vec![0xF0, 0x00, 0x00, 0x66, device, 0x12, at];
    m.extend(text.iter().map(|&b| b & 0x7F));
    m.push(0xF7);
    m
}

/// The time display's ten characters, right to left as they are sent: the
/// ruler's text, its fields from the right into the display's (three
/// digits, two, two and three; one field alone takes them all).
fn clock_digits(text: &str) -> [u8; 10] {
    let fields: Vec<&str> = text
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|f| !f.is_empty())
        .collect();
    let mut out = [b' '; 10];
    if let [one] = fields.as_slice() {
        for (i, b) in one.bytes().rev().take(10).enumerate() {
            out[i] = b;
        }
        return out;
    }
    let mut at = 0;
    for (field, width) in fields.iter().rev().zip([3, 2, 2, 3]) {
        let width: usize = width;
        let bytes: Vec<u8> = field.bytes().rev().take(width).collect();
        for k in 0..width {
            out[at + k] = match bytes.get(k) {
                Some(&b) => b,
                // Two-digit fields keep their zero.
                None if width == 2 => b'0',
                None => b' ',
            };
        }
        at += width;
    }
    out
}

/// A level as a meter of six characters.
fn meter_text(db: f32) -> String {
    let n = ((db + 60.0) / 60.0 * 6.0).clamp(0.0, 6.0).round() as usize;
    format!("{:<6}", "#".repeat(n))
}

/// A pan as the display shows it: "L 50", "C", "R 20".
fn pan_text(pan: f32) -> String {
    let n = (pan.abs() * 100.0).round() as i32;
    match n {
        0 => "C".into(),
        _ if pan < 0.0 => format!("L{n}"),
        _ => format!("R{n}"),
    }
}

/// A level in dB as the display shows it.
fn db_text(gain: f32) -> String {
    if gain <= 1e-4 {
        "-inf".into()
    } else {
        format!("{:.1}", 20.0 * gain.log10())
    }
}

/// Decibels of a gain.
fn bv_mixer_db(gain: f32) -> f32 {
    if gain <= 1e-5 {
        -96.0
    } else {
        20.0 * gain.log10()
    }
}

/// `value` stepped by `ticks` within `0..len` (nothing to step in an
/// empty list).
fn step(value: usize, ticks: i32, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (value as i64 + ticks.signum() as i64).clamp(0, len as i64 - 1) as usize
}

/// `list`'s item after (or before) `now`, round the list.
fn cycle<T: Copy + PartialEq>(list: &[T], now: T, ticks: i32) -> T {
    let n = list.len() as i64;
    let i = list.iter().position(|&x| x == now).unwrap_or(0) as i64;
    list[(i + ticks.signum() as i64).rem_euclid(n) as usize]
}

/// A parameter of an effect on the surface: the first is its Bypass, the
/// others its own (a plug-in's are edited in its window).
fn fx_params(e: &bv_timeline::Effect) -> usize {
    1 + if e.plugin.is_some() || e.vst.is_some() {
        0
    } else {
        e.kind.params().len()
    }
}

impl BananaApp {
    /// Whether device `index` is a Mackie Control Extender.
    fn is_extender(&self, index: usize) -> bool {
        self.settings
            .prefs
            .control_devices
            .get(index)
            .is_some_and(|d| d.kind == SurfaceKind::MackieControl && d.extender)
    }

    /// `f` on every Mackie Control and Extender: their bank and modes go
    /// together.
    fn each_mackie(&mut self, f: impl Fn(&mut Mackie)) {
        let devices = &self.settings.prefs.control_devices;
        for (d, m) in devices.iter().zip(self.surfaces.iter_mut()) {
            if d.kind == SurfaceKind::MackieControl {
                f(m);
            }
        }
    }

    /// The channels the strips of device `index` control now: from its
    /// Channel Mapping and the bank, the audio tracks, the busses or the
    /// tracks and then the busses (Video mode: the video tracks).
    pub(crate) fn surface_channels(&self, index: usize) -> Vec<Option<Chan>> {
        let first = self
            .settings
            .prefs
            .control_devices
            .get(index)
            .map_or(0, |d| d.first_track as i64);
        let m = self.surfaces.get(index);
        let bank = m.map_or(0, |m| m.bank);
        let video = m.is_some_and(|m| m.video);
        let (kind, channels) = if video {
            (TrackKind::Video, Channels::Tracks)
        } else {
            (TrackKind::Audio, m.map_or(Channels::Tracks, |m| m.channels))
        };
        let p = self.project();
        let mut list: Vec<Chan> = Vec::new();
        if channels != Channels::Busses {
            list.extend(
                p.tracks
                    .iter()
                    .filter(|t| t.kind == kind)
                    .map(|t| Chan::Track(t.id)),
            );
        }
        if channels != Channels::Tracks {
            list.extend(p.busses.iter().map(|b| Chan::Bus(b.id)));
        }
        (0..STRIPS as i64)
            .map(|i| {
                usize::try_from(first + bank + i)
                    .ok()
                    .and_then(|k| list.get(k))
                    .copied()
            })
            .collect()
    }

    /// The tracks under the strips of device `index` (none where a bus
    /// is).
    pub(crate) fn surface_tracks(&self, index: usize) -> Vec<Option<TrackId>> {
        self.surface_channels(index)
            .into_iter()
            .map(|c| match c {
                Some(Chan::Track(t)) => Some(t),
                _ => None,
            })
            .collect()
    }

    /// The busses under the strips of device `index`.
    pub(crate) fn surface_busses(&self, index: usize) -> Vec<Option<BusId>> {
        self.surface_channels(index)
            .into_iter()
            .map(|c| match c {
                Some(Chan::Bus(b)) => Some(b),
                _ => None,
            })
            .collect()
    }

    /// `control` of bus `b` as surface `index` shows it (see
    /// [`Self::surface_value`]).
    fn surface_bus_value(&self, index: usize, b: BusId, control: TrackControl) -> f32 {
        let owner = AutoOwner::Strip(Strip::Bus(b));
        if self.surfaces.get(index).is_some_and(|m| m.automation) {
            self.owner_value(owner, control)
        } else {
            crate::automation::trim_value(self.project(), owner, control)
        }
    }

    /// Strip `strip` of surface `index` sets `control` of bus `b` (see
    /// [`Self::surface_set`]; a V-Pot, `knob`, lets go at once).
    fn surface_bus_set(
        &mut self,
        index: usize,
        strip: usize,
        (b, control): (BusId, TrackControl),
        value: f32,
        knob: bool,
    ) {
        let owner = AutoOwner::Strip(Strip::Bus(b));
        let key = egui::Id::new(("superficie", index, strip, format!("{control:?}")));
        self.surfaces[index].changed = Some((strip, Instant::now()));
        if self.surfaces[index].automation {
            self.set_owner_control(owner, control, value, key);
            if knob {
                self.release_owner_control(owner, control);
            }
            return;
        }
        self.modify_merged(key, |p| {
            crate::automation::set_trim(p, owner, control, value)
        });
    }

    /// A bus's fader: its volume, or with Flip in Pan its pan.
    fn surface_bus_fader(&mut self, index: usize, strip: usize, b: BusId, value: u16) {
        let m = &self.surfaces[index];
        let unit = value.min(16383) as f32 / 16383.0;
        let what = if m.flip && m.assign == Assign::Pan {
            (TrackControl::Pan, unit * 2.0 - 1.0)
        } else {
            (TrackControl::Volume, fader_to_gain(value))
        };
        self.surface_bus_set(index, strip, (b, what.0), what.1, false);
    }

    /// A bus's V-Pot: in Pan its pan (with Flip its volume).
    fn surface_bus_vpot(&mut self, index: usize, strip: usize, b: BusId, ticks: i32) {
        let m = &self.surfaces[index];
        if m.assign != Assign::Pan {
            return;
        }
        if m.flip {
            let g = self
                .surface_bus_value(index, b, TrackControl::Volume)
                .max(1e-3);
            let g = (g * 10f32.powf(ticks as f32 * 0.5 / 20.0)).min(MAX_GAIN);
            return self.surface_bus_set(index, strip, (b, TrackControl::Volume), g, true);
        }
        let pan = self.surface_bus_value(index, b, TrackControl::Pan) + ticks as f32 * 0.02;
        self.surface_bus_set(
            index,
            strip,
            (b, TrackControl::Pan),
            pan.clamp(-1.0, 1.0),
            true,
        );
    }

    /// What a bus's strip shows under its name: its meter (Meters mode),
    /// its pan (Pan) or its volume.
    fn bus_value(&mut self, index: usize, strip: usize, b: BusId) -> String {
        let m = &self.surfaces[index];
        let fresh = m
            .changed
            .is_some_and(|(s, at)| s == strip && at.elapsed() < SHOW_VALUE);
        if m.meters && !fresh {
            let db = self
                .strip_meters
                .get(&Strip::Bus(b))
                .map_or(-96.0, |m| m.level(0).max(m.level(1)));
            return meter_text(db);
        }
        if m.assign == Assign::Pan && !m.flip {
            pan_text(self.surface_bus_value(index, b, TrackControl::Pan))
        } else {
            db_text(self.surface_bus_value(index, b, TrackControl::Volume))
        }
    }

    /// Each frame: what the control surfaces sent, done; what they show,
    /// sent back.
    pub(crate) fn follow_surfaces(&mut self, dt: f32) {
        let devices = self.settings.prefs.control_devices.clone();
        if !self.settings.prefs.external_control || devices.is_empty() {
            return;
        }
        self.surfaces.resize_with(devices.len(), Mackie::default);
        let mut incoming: Vec<(String, [u8; 3])> = self
            .midi
            .incoming
            .lock()
            .map(|mut q| std::mem::take(&mut *q))
            .unwrap_or_default();
        // Under the render's window (modal) the program takes no input.
        let modal = self.dialogs.rendering.is_some();
        if modal {
            incoming.clear();
        }
        for (port, bytes) in incoming {
            for (i, d) in devices.iter().enumerate() {
                if d.input.as_deref() == Some(port.as_str()) {
                    match d.kind {
                        SurfaceKind::MackieControl => self.surface_message(i, bytes),
                        SurfaceKind::Generic => self.generic_message(i, d, bytes),
                    }
                }
            }
        }
        // While Configure Mackie Control is open, each device shows its
        // channel mapping.
        let mapping = std::mem::take(&mut self.mackie_mapping_shown);
        for (i, device) in devices.iter().enumerate() {
            // Knobs turned in automation are let go a moment later.
            self.generic_let_go(i);
            if device.kind == SurfaceKind::Generic {
                continue;
            }
            // Rewind and Fast Forward run the cursor at 20× while held.
            let run = self.surfaces[i].running;
            if run != 0 && !modal {
                let step = Tick::from_seconds_f64(20.0 * dt as f64);
                let t = if run > 0 {
                    self.cursor + step
                } else {
                    (self.cursor - step).max(Tick::ZERO)
                };
                self.seek_playback(t);
            }
            self.surface_feedback(i, device, mapping);
        }
    }

    /// A short message from surface `index`.
    fn surface_message(&mut self, index: usize, [status, a, b]: [u8; 3]) {
        let tracks = self.surface_tracks(index);
        let busses = self.surface_busses(index);
        let key = egui::Id::new(("superficie", index));
        match status & 0xF0 {
            // A fader: pitch bend on its strip's channel (the 9th: master).
            0xE0 => {
                let ch = (status & 0x0F) as usize;
                let value = (a as u16) | (b as u16) << 7;
                if ch == STRIPS && !self.is_extender(index) {
                    let db = bv_mixer_db(fader_to_gain(value));
                    self.modify_merged(key.with("master"), |p| p.master.gain_db = db);
                } else if let Some(Some(t)) = tracks.get(ch) {
                    self.surface_fader(index, ch, *t, value);
                } else if let Some(Some(bus)) = busses.get(ch) {
                    self.surface_bus_fader(index, ch, *bus, value);
                }
            }
            0xB0 if (16..16 + STRIPS as u8).contains(&a) => {
                let strip = (a - 16) as usize;
                // PE, PS and Time Fmt held work as ever.
                let m = &self.surfaces[index];
                let plain =
                    !m.time_held && !matches!(m.assign, Assign::PluginEdit | Assign::PluginChain);
                match busses[strip] {
                    Some(bus) if tracks[strip].is_none() && plain => {
                        self.surface_bus_vpot(index, strip, bus, vpot_ticks(b));
                    }
                    _ => self.surface_vpot(index, strip, tracks[strip], vpot_ticks(b)),
                }
            }
            // The jog wheel.
            0xB0 if a == 60 && !self.is_extender(index) => self.surface_jog(index, vpot_ticks(b)),
            0x90 => self.surface_button(index, a, b >= 0x40, &tracks),
            0x80 => self.surface_button(index, a, false, &tracks),
            _ => {}
        }
    }

    /// The buses and chains Sends goes through, in their order.
    fn send_targets(&self) -> Vec<SendTo> {
        let p = self.project();
        p.busses
            .iter()
            .map(|b| SendTo::Bus(b.id))
            .chain(p.fx_chains.iter().map(|c| SendTo::Fx(c.id)))
            .collect()
    }

    /// The bus or chain Sends of surface `index` sends to now.
    fn surface_send(&self, index: usize) -> Option<SendTo> {
        let targets = self.send_targets();
        let n = self.surfaces.get(index).map_or(0, |m| m.send);
        (!targets.is_empty()).then(|| targets[n % targets.len()])
    }

    /// `control` of `track` as surface `index` shows it: the automation
    /// control with Automation on, else the trim.
    fn surface_value(&self, index: usize, track: TrackId, control: TrackControl) -> f32 {
        if self.surfaces.get(index).is_some_and(|m| m.automation) {
            self.control_value(track, control)
        } else {
            crate::automation::trim_value(self.project(), AutoOwner::Track(track), control)
        }
    }

    /// Strip `strip` of surface `index` sets `control` of `track`: its
    /// automation control with Automation on (recorded while playing, as
    /// the headers' sliders do; a V-Pot, `knob`, is let go a moment
    /// after it stops), else its trim.
    fn surface_set(
        &mut self,
        index: usize,
        strip: usize,
        (track, control): (TrackId, TrackControl),
        value: f32,
        knob: bool,
    ) {
        let key = egui::Id::new(("superficie", index, strip, format!("{control:?}")));
        self.surfaces[index].changed = Some((strip, Instant::now()));
        if self.surfaces[index].automation {
            self.set_control(track, control, value, key);
            if knob {
                self.surfaces[index].generic.hold(track, control);
            }
            return;
        }
        self.modify_merged(key, |p| {
            crate::automation::set_trim(p, AutoOwner::Track(track), control, value)
        });
    }

    /// A strip's fader: the track's volume (a video track's opacity), or
    /// with Flip what Pan or Sends set.
    fn surface_fader(&mut self, index: usize, strip: usize, track: TrackId, value: u16) {
        let Some(kind) = self.project().track(track).map(|t| t.kind) else {
            return;
        };
        let m = &self.surfaces[index];
        let unit = value.min(16383) as f32 / 16383.0;
        let what = match (kind, m.flip, m.assign) {
            (TrackKind::Audio, true, Assign::Pan) => Some((TrackControl::Pan, unit * 2.0 - 1.0)),
            (TrackKind::Audio, true, Assign::Sends) => self
                .surface_send(index)
                .map(|to| (TrackControl::Send(to), fader_to_gain(value))),
            (TrackKind::Audio, ..) => Some((TrackControl::Volume, fader_to_gain(value))),
            (TrackKind::Video, ..) => Some((TrackControl::Level, unit)),
        };
        if let Some((control, v)) = what {
            self.surface_set(index, strip, (track, control), v, false);
        }
    }

    /// A V-Pot turned `ticks`: what the Control buttons choose.
    fn surface_vpot(&mut self, index: usize, strip: usize, track: Option<TrackId>, ticks: i32) {
        let m = &self.surfaces[index];
        let (assign, flip, held, time_held) = (m.assign, m.flip, m.held_f, m.time_held);
        // Time Fmt held: V-Pot 8 changes the ruler's format.
        if time_held && strip == STRIPS - 1 {
            let now = self.effective_ruler_format();
            let next = cycle(&crate::timefmt::RulerFormat::ALL, now, ticks);
            self.modify(|p| p.settings.ruler_format = Some(next));
            return;
        }
        match assign {
            Assign::PluginEdit => return self.vpot_edit(index, strip, ticks),
            Assign::PluginChain => return self.vpot_chain(index, strip, ticks),
            _ => {}
        }
        let Some(t) = track else {
            return;
        };
        let Some(tr) = self.project().track(t).cloned() else {
            return;
        };
        let audio = tr.kind == TrackKind::Audio;
        if assign == Assign::Settings {
            // F1: the automation mode; F2: the pan mode; F4: input
            // monitoring.
            if held[0] {
                use AutomationMode::*;
                let mode = cycle(&[Off, Read, Touch, Latch], tr.automation.mode, ticks);
                self.set_automation_mode(t, mode);
            } else if held[1] && audio {
                let mode = cycle(&PanMode::ALL, tr.pan_mode, ticks);
                self.modify(|p| {
                    if let Some(t) = p.track_by_id_mut(t) {
                        t.pan_mode = mode;
                    }
                });
            } else if held[3] && audio {
                use InputMonitor::*;
                let mode = cycle(&[Off, On, Auto], tr.input_monitor, ticks);
                self.modify(|p| {
                    if let Some(t) = p.track_by_id_mut(t) {
                        t.input_monitor = mode;
                    }
                });
            }
            self.surfaces[index].changed = Some((strip, Instant::now()));
            return;
        }
        // Flip: the V-Pots set the volume.
        if flip && audio && matches!(assign, Assign::Pan | Assign::Sends) {
            let g = self.surface_value(index, t, TrackControl::Volume).max(1e-3);
            let g = (g * 10f32.powf(ticks as f32 * 0.5 / 20.0)).min(MAX_GAIN);
            return self.surface_set(index, strip, (t, TrackControl::Volume), g, true);
        }
        match assign {
            Assign::Pan if audio => {
                let room = self
                    .project()
                    .surround_pans(bv_timeline::SurroundOwner::Track(t));
                let axis = self.surfaces[index].axis;
                if room {
                    // 5.1: left to right or front to rear, as the Surround
                    // Panner moves the point (keyframes with automation
                    // controls); or the center's level.
                    let owner = bv_timeline::SurroundOwner::Track(t);
                    let key = egui::Id::new(("superficie-sala", index, strip, axis));
                    let d = ticks as f32 * 0.02;
                    if axis < 2 {
                        use crate::tabs::surround::{point_now, release_surround, set_point};
                        let mut q = point_now(self.project(), owner, self.display_time());
                        q[axis as usize] = (q[axis as usize] + d).clamp(-1.0, 1.0);
                        set_point(self, owner, q, key);
                        release_surround(self, owner);
                    } else {
                        self.modify_merged(key, |p| {
                            if let Some(s) = p.surround_mut(owner) {
                                s.center_db = (s.center_db + ticks as f32 * 0.5).clamp(-60.0, 12.0);
                            }
                        });
                    }
                    self.surfaces[index].changed = Some((strip, Instant::now()));
                } else {
                    let pan = self.surface_value(index, t, TrackControl::Pan) + ticks as f32 * 0.02;
                    self.surface_set(
                        index,
                        strip,
                        (t, TrackControl::Pan),
                        pan.clamp(-1.0, 1.0),
                        true,
                    );
                }
            }
            // Video: the fade to color (an automation control only).
            Assign::Pan => {
                let fade = self.control_value(t, TrackControl::Fade) + ticks as f32 * 0.02;
                self.surface_set(
                    index,
                    strip,
                    (t, TrackControl::Fade),
                    fade.clamp(-1.0, 1.0),
                    true,
                );
            }
            Assign::Sends if audio => {
                if let Some(to) = self.surface_send(index) {
                    let g = self
                        .surface_value(index, t, TrackControl::Send(to))
                        .max(1e-3);
                    let g = (g * 10f32.powf(ticks as f32 * 0.5 / 20.0)).min(MAX_GAIN);
                    self.surface_set(index, strip, (t, TrackControl::Send(to)), g, true);
                }
            }
            // Output: the bus to play into, chosen by pressing; in Video
            // mode the compositing mode, at once.
            Assign::Output if audio => {
                let outputs = self.output_choices();
                let now = self.surfaces[index].choice[strip]
                    .unwrap_or_else(|| outputs.iter().position(|&o| o == tr.output).unwrap_or(0));
                self.surfaces[index].choice[strip] = Some(step(now, ticks, outputs.len()));
            }
            Assign::Output => {
                let mode = cycle(&bv_timeline::CompositeMode::MENU, tr.composite_mode, ticks);
                self.modify(|p| {
                    if let Some(t) = p.track_by_id_mut(t) {
                        t.composite_mode = mode;
                    }
                });
            }
            // Input: the input to record from, chosen by pressing.
            Assign::Input if audio => {
                let inputs = self.input_choices();
                let current = tr
                    .record_input
                    .clone()
                    .unwrap_or_else(|| self.default_record_input());
                let now = self.surfaces[index].choice[strip]
                    .unwrap_or_else(|| inputs.iter().position(|i| *i == current).unwrap_or(0));
                self.surfaces[index].choice[strip] = Some(step(now, ticks, inputs.len()));
            }
            // PL: the effect the strip shows.
            Assign::Plugins => {
                let n = tr.effects.len();
                let m = &mut self.surfaces[index];
                m.showing[strip] = step(m.showing[strip], ticks, n);
            }
            _ => {}
        }
    }

    /// The outputs Output offers: the Master, then the busses.
    fn output_choices(&self) -> Vec<Output> {
        std::iter::once(Output::Master)
            .chain(self.project().busses.iter().map(|b| Output::Bus(b.id)))
            .collect()
    }

    /// The inputs Input offers: each device's pairs and channels, then
    /// the input busses.
    fn input_choices(&mut self) -> Vec<bv_timeline::RecordInput> {
        let mut out = Vec::new();
        for d in self.input_devices() {
            let device = if d.default {
                String::new()
            } else {
                d.name.clone()
            };
            let n = d.channels.max(1);
            let pairs = (0..n.saturating_sub(1)).step_by(2).map(|first| (first, 2));
            let singles = (0..n).map(|first| (first, 1));
            for (first, channels) in pairs.chain(singles) {
                out.push(bv_timeline::RecordInput {
                    device: device.clone(),
                    first,
                    channels,
                    bus: None,
                });
            }
        }
        out.extend(
            self.project()
                .input_busses
                .iter()
                .map(|b| bv_timeline::RecordInput {
                    bus: Some(b.id),
                    ..Default::default()
                }),
        );
        out
    }

    /// The name Output or Input shows for a strip's choice.
    fn choice_text(&mut self, index: usize, strip: usize, track: TrackId) -> String {
        let m = &self.surfaces[index];
        let (assign, choice) = (m.assign, m.choice[strip]);
        let p = self.project();
        let Some(tr) = p.track(track) else {
            return String::new();
        };
        match assign {
            Assign::Output if tr.kind == TrackKind::Video => tr.composite_mode.name().into(),
            Assign::Output => {
                let to = choice
                    .and_then(|c| self.output_choices().get(c).copied())
                    .unwrap_or(tr.output);
                crate::i18n::output_name(p, to)
            }
            Assign::Input if tr.kind == TrackKind::Video => if tr.bypass_motion_blur {
                "Bypass"
            } else {
                "MBlur"
            }
            .into(),
            Assign::Input => {
                let current = tr.record_input.clone();
                let input = match choice {
                    Some(c) => self.input_choices().get(c).cloned(),
                    None => current,
                }
                .unwrap_or_else(|| self.default_record_input());
                match input.bus {
                    Some(b) => crate::i18n::input_bus_name(self.project(), b),
                    None => crate::recording::input_label(&input),
                }
            }
            _ => String::new(),
        }
    }

    /// PE's V-Pots: 1–4 the parameters, 5 their pages, 7 the same effect
    /// on another track, 8 the track's other effects (6, presets: not
    /// yet).
    fn vpot_edit(&mut self, index: usize, k: usize, ticks: i32) {
        let Some(mut e) = self.surfaces[index].edit else {
            return;
        };
        let p = self.project();
        let Some(tr) = p.track(e.track) else {
            return;
        };
        let Some(fx) = tr.effects.get(e.effect).cloned() else {
            return;
        };
        let pages = fx_params(&fx).div_ceil(4);
        match k {
            0..=3 => {
                let i = e.page * 4 + k;
                // The parameters past Bypass: the effect's own.
                let Some(spec) = i
                    .checked_sub(1)
                    .and_then(|i| fx.kind.params().get(i).copied())
                    .filter(|_| i < fx_params(&fx))
                else {
                    return;
                };
                if spec.kind != bv_timeline::ParamKind::Slider {
                    return;
                }
                let v = fx.param(i - 1) + ticks as f32 * (spec.max - spec.min) / 100.0;
                let (track, effect) = (e.track, e.effect);
                let key = egui::Id::new(("superficie-pe", index, track, effect, i));
                self.modify_merged(key, |p| {
                    if let Some(f) = p
                        .track_by_id_mut(track)
                        .and_then(|t| t.effects.get_mut(effect))
                    {
                        f.set_param(i - 1, v);
                    }
                });
                self.surfaces[index].changed = Some((k, Instant::now()));
                return;
            }
            4 => e.page = step(e.page, ticks, pages),
            // The effect's presets, chosen by pressing.
            5 => {
                if fx.plugin.is_some() || fx.vst.is_some() {
                    return;
                }
                let presets = crate::tabs::console::effect_presets(self, fx.kind);
                let now = e.preset.unwrap_or_else(|| {
                    presets
                        .iter()
                        .position(|(_, v)| *v == fx.params)
                        .unwrap_or(0)
                });
                e.preset = Some(step(now, ticks, presets.len()));
            }
            6 => {
                // Another track of the kind with effects.
                let kind = tr.kind;
                let with_fx: Vec<TrackId> = p
                    .tracks
                    .iter()
                    .filter(|t| t.kind == kind && !t.effects.is_empty())
                    .map(|t| t.id)
                    .collect();
                let at = with_fx.iter().position(|&t| t == e.track).unwrap_or(0);
                let t = with_fx[step(at, ticks, with_fx.len())];
                let n = p.track(t).map_or(0, |t| t.effects.len());
                e = FxEdit {
                    track: t,
                    effect: e.effect.min(n.saturating_sub(1)),
                    page: 0,
                    preset: None,
                };
            }
            7 => {
                e.effect = step(e.effect, ticks, tr.effects.len());
                e.page = 0;
                e.preset = None;
            }
            _ => return,
        }
        self.surfaces[index].edit = Some(e);
    }

    /// The preset PE's V-Pot 6 points at, or the one the edited effect is
    /// at ("--": none, or a plug-in's).
    fn pe_preset(&mut self, index: usize) -> String {
        let Some(e) = self.surfaces[index].edit else {
            return "--".into();
        };
        let Some(fx) = self
            .project()
            .track(e.track)
            .and_then(|t| t.effects.get(e.effect))
            .filter(|f| f.plugin.is_none() && f.vst.is_none())
            .cloned()
        else {
            return "--".into();
        };
        let presets = crate::tabs::console::effect_presets(self, fx.kind);
        e.preset
            .or_else(|| presets.iter().position(|(_, v)| *v == fx.params))
            .and_then(|at| presets.into_iter().nth(at))
            .map_or_else(|| "--".into(), |(n, _)| n)
    }

    /// PE's V-Pots pressed: 1–4 switch a switch parameter (Bypass), 6
    /// takes the preset it points at.
    fn push_edit(&mut self, index: usize, k: usize) {
        let Some(e) = self.surfaces[index].edit else {
            return;
        };
        if k == 5 {
            let Some(pick) = e.preset else {
                return;
            };
            let Some(kind) = self
                .project()
                .track(e.track)
                .and_then(|t| t.effects.get(e.effect))
                .filter(|f| f.plugin.is_none() && f.vst.is_none())
                .map(|f| f.kind)
            else {
                return;
            };
            let Some((_, params)) = crate::tabs::console::effect_presets(self, kind)
                .into_iter()
                .nth(pick)
            else {
                return;
            };
            self.modify(|p| {
                if let Some(f) = p
                    .track_by_id_mut(e.track)
                    .and_then(|t| t.effects.get_mut(e.effect))
                {
                    for (j, v) in params.iter().enumerate() {
                        f.set_param(j, *v);
                    }
                }
            });
            if let Some(edit) = &mut self.surfaces[index].edit {
                edit.preset = None;
            }
            self.surfaces[index].changed = Some((k, Instant::now()));
            return;
        }
        let i = e.page * 4 + k;
        if k > 3 {
            return;
        }
        let p = self.project();
        let Some(fx) = p.track(e.track).and_then(|t| t.effects.get(e.effect)) else {
            return;
        };
        let toggle = i == 0
            || fx
                .kind
                .params()
                .get(i - 1)
                .is_some_and(|s| s.kind == bv_timeline::ParamKind::Toggle && i < fx_params(fx));
        if !toggle {
            return;
        }
        self.modify(|p| {
            if let Some(f) = p
                .track_by_id_mut(e.track)
                .and_then(|t| t.effects.get_mut(e.effect))
            {
                if i == 0 {
                    f.enabled ^= true;
                } else {
                    let v = if f.param(i - 1) >= 0.5 { 0.0 } else { 1.0 };
                    f.set_param(i - 1, v);
                }
            }
        });
        self.surfaces[index].changed = Some((k, Instant::now()));
    }

    /// PS's V-Pots: 1, 3 and 5 choose the effect there or one to add, 7
    /// the chain's pages, 8 another track's chain.
    fn vpot_chain(&mut self, index: usize, k: usize, ticks: i32) {
        let Some(mut c) = self.surfaces[index].chain else {
            return;
        };
        let p = self.project();
        let Some(tr) = p.track(c.track) else {
            return;
        };
        let addable = if tr.kind == TrackKind::Audio {
            bv_timeline::EffectKind::AUDIO.len()
        } else {
            0
        };
        match k {
            0 | 2 | 4 => {
                let s = k / 2;
                c.picks[s] = step(c.picks[s], ticks, addable + 1);
            }
            6 => {
                let pages = tr.effects.len() / 3 + 1;
                c.page = step(c.page, ticks, pages);
                c.picks = [0; 3];
            }
            7 => {
                let kind = tr.kind;
                let tracks: Vec<TrackId> = p
                    .tracks
                    .iter()
                    .filter(|t| t.kind == kind)
                    .map(|t| t.id)
                    .collect();
                let at = tracks.iter().position(|&t| t == c.track).unwrap_or(0);
                c = ChainView {
                    track: tracks[step(at, ticks, tracks.len())],
                    page: 0,
                    picks: [0; 3],
                };
            }
            _ => return,
        }
        self.surfaces[index].chain = Some(c);
    }

    /// PS's V-Pots pressed: 1, 3 and 5 edit the effect there (PE); 2, 4
    /// and 6 add there the effect chosen by the V-Pot on their left.
    fn push_chain(&mut self, index: usize, k: usize) {
        let Some(mut c) = self.surfaces[index].chain else {
            return;
        };
        if k > 5 {
            return;
        }
        let s = k / 2;
        let at = c.page * 3 + s;
        let n = self.project().track(c.track).map_or(0, |t| t.effects.len());
        if k.is_multiple_of(2) {
            if c.picks[s] == 0 && at < n {
                self.surfaces[index].edit = Some(FxEdit {
                    track: c.track,
                    effect: at,
                    page: 0,
                    preset: None,
                });
                self.each_mackie(|m| m.assign = Assign::PluginEdit);
            }
            return;
        }
        let Some(&kind) = c.picks[s]
            .checked_sub(1)
            .and_then(|i| bv_timeline::EffectKind::AUDIO.get(i))
        else {
            return;
        };
        let track = c.track;
        self.modify(|p| {
            if let Some(t) = p.track_by_id_mut(track) {
                let at = at.min(t.effects.len());
                t.effects.insert(at, bv_timeline::Effect::new(kind));
            }
        });
        c.picks[s] = 0;
        self.surfaces[index].chain = Some(c);
    }

    /// A V-Pot pressed.
    fn surface_push(&mut self, index: usize, strip: usize, track: Option<TrackId>) {
        let m = &self.surfaces[index];
        let (assign, held) = (m.assign, m.held_f);
        match assign {
            Assign::PluginEdit => return self.push_edit(index, strip),
            Assign::PluginChain => return self.push_chain(index, strip),
            _ => {}
        }
        let Some(t) = track else {
            return;
        };
        let Some(tr) = self.project().track(t).cloned() else {
            return;
        };
        self.surfaces[index].changed = Some((strip, Instant::now()));
        match assign {
            // F3: the phase; F5: the send before or after the volume; F6:
            // the track's defaults.
            Assign::Settings => {
                if held[2] {
                    self.modify(|p| {
                        if let Some(t) = p.track_by_id_mut(t) {
                            t.invert_phase ^= true;
                        }
                    });
                } else if held[4] {
                    if let Some(to) = self.surface_send(index) {
                        let send = tr
                            .sends
                            .iter()
                            .find(|s| s.to == to)
                            .copied()
                            .unwrap_or_else(|| bv_timeline::Send::new(to));
                        self.modify(|p| {
                            p.set_send(
                                t,
                                bv_timeline::Send {
                                    pre_volume: !send.pre_volume,
                                    ..send
                                },
                            );
                        });
                    }
                } else if held[5] {
                    self.surface_track_defaults(t);
                }
            }
            // Pan and Sends: the track's effects chain.
            Assign::Pan | Assign::Sends => self.clip_windows.track_fx = Some(t),
            Assign::Output if tr.kind == TrackKind::Audio => {
                if let Some(c) = self.surfaces[index].choice[strip].take()
                    && let Some(&to) = self.output_choices().get(c)
                {
                    self.modify(|p| {
                        p.set_track_output(t, to);
                    });
                }
            }
            Assign::Input if tr.kind == TrackKind::Audio => {
                if let Some(c) = self.surfaces[index].choice[strip].take()
                    && let Some(input) = self.input_choices().get(c).cloned()
                {
                    self.set_record_input(t, input);
                }
            }
            // Video mode's Motion Blur: the track's Bypass Motion Blur.
            Assign::Input => {
                let on = !tr.bypass_motion_blur;
                self.modify(|p| {
                    if let Some(t) = p.track_by_id_mut(t) {
                        t.bypass_motion_blur = on;
                    }
                });
            }
            // PL: the effect shown, edited (PE).
            Assign::Plugins => {
                let effect = self.surfaces[index].showing[strip];
                if effect < tr.effects.len() {
                    self.surfaces[index].edit = Some(FxEdit {
                        track: t,
                        effect,
                        page: 0,
                        preset: None,
                    });
                    self.each_mackie(|m| m.assign = Assign::PluginEdit);
                }
            }
            _ => {}
        }
    }

    /// F6 with a V-Pot pressed: the track back to the defaults new tracks
    /// of its kind get (Set Default Track Properties).
    fn surface_track_defaults(&mut self, track: TrackId) {
        let Some(kind) = self.project().track(track).map(|t| t.kind) else {
            return;
        };
        let d = self.settings.track_defaults.of(kind).clone();
        self.modify(|p| {
            if let Some(t) = p.track_by_id_mut(track) {
                t.level = 1.0;
                t.pan = 0.0;
                t.pan_mode = PanMode::default();
                t.composite_mode = Default::default();
                t.input_monitor = InputMonitor::Off;
                t.automation = Default::default();
                d.apply(t);
            }
        });
    }

    /// The jog wheel turned `ticks`: frames (Zoom: pixels; Shift: the time
    /// selection along); while playing, faster or slower; with Event Trim,
    /// the edge trimmed (Ctrl: by pixels).
    fn surface_jog(&mut self, index: usize, ticks: i32) {
        let m = &self.surfaces[index];
        let (zoom, shift, ctrl) = (m.zoom, m.shift, m.ctrl);
        let pixel = Tick((self.timeline.ticks_per_px.round() as i64).max(1));
        let frame = self.frame_rate().frame_duration();
        if self.timeline.edge_trim.is_some() {
            let unit = if ctrl { pixel } else { frame };
            return self.trim_edit_point(Tick(unit.0 * ticks as i64));
        }
        if self.is_playing() {
            let e = if ticks > 0 {
                Extra::ScrubRight
            } else {
                Extra::ScrubLeft
            };
            return self.run_extra(e);
        }
        let from = self.cursor;
        let t = if zoom {
            from + Tick(pixel.0 * ticks as i64)
        } else {
            let rate = self.frame_rate();
            rate.frame_start((rate.frame_at(from) + ticks as i64).max(0))
        };
        self.seek_playback(t.max(Tick::ZERO));
        if shift {
            self.extend_selection(from);
        }
    }

    /// Runs `c` when it can run.
    fn surface_cmd(&mut self, c: Cmd) {
        if c.enabled(self) {
            c.run(self);
        }
    }

    /// What Configure Mackie Control maps button `n` of device `index` to
    /// (with Shift and Option as held): `Some(None)` cleared (Option with
    /// F1–F16 does only what is mapped), `None` its default.
    fn surface_mapped(&self, index: usize, n: u8) -> Option<Option<String>> {
        let m = &self.surfaces[index];
        let d = self.settings.prefs.control_devices.get(index)?;
        if let Some(b) = d
            .mackie_map
            .iter()
            .find(|b| b.note == n && b.shift == m.shift && b.option == m.option)
        {
            return Some(b.command.clone());
        }
        (m.option && (note::F1..note::F1 + 8).contains(&n)).then_some(None)
    }

    /// The Control buttons: Output, Input, Pan (again: the 5.1 axes),
    /// Sends (again: the next bus or chain), Inserts (PL, again PS) and
    /// Settings.
    fn surface_assign(&mut self, index: usize, n: u8) {
        use note::*;
        let m = &self.surfaces[index];
        let now = m.assign;
        let focus = self
            .focused_track()
            .filter(|t| self.surface_tracks(index).contains(&Some(*t)))
            .or_else(|| self.surface_tracks(index).into_iter().flatten().next());
        let next = match n {
            OUTPUT => Assign::Output,
            INPUT => Assign::Input,
            PAN => {
                if now == Assign::Pan {
                    self.each_mackie(|m| m.axis = (m.axis + 1) % 3);
                }
                Assign::Pan
            }
            SENDS => {
                if now == Assign::Sends {
                    self.each_mackie(|m| m.send += 1);
                }
                Assign::Sends
            }
            INSERTS => match now {
                Assign::Plugins | Assign::PluginEdit => {
                    let track = self.surfaces[index].edit.map(|e| e.track).or(focus);
                    let Some(track) = track else {
                        return;
                    };
                    let chain = ChainView {
                        track,
                        page: 0,
                        picks: [0; 3],
                    };
                    self.each_mackie(|m| m.chain = Some(chain));
                    Assign::PluginChain
                }
                _ => Assign::Plugins,
            },
            _ => Assign::Settings,
        };
        self.each_mackie(|m| {
            m.assign = next;
            m.choice = [None; STRIPS];
        });
    }

    /// A button of surface `index` pressed (or let go).
    fn surface_button(&mut self, index: usize, n: u8, down: bool, tracks: &[Option<TrackId>]) {
        use note::*;
        let strip = |base: u8| {
            (base..base + STRIPS as u8)
                .contains(&n)
                .then(|| tracks.get((n - base) as usize).copied().flatten())
                .flatten()
        };
        let busses = self.surface_busses(index);
        // Faders touched and let go.
        if (FADER_TOUCH..=FADER_TOUCH + STRIPS as u8).contains(&n) {
            let i = (n - FADER_TOUCH) as usize;
            self.surfaces[index].touched[i] = down;
            if !down && let Some(Some(t)) = tracks.get(i) {
                for control in [TrackControl::Volume, TrackControl::Level, TrackControl::Pan] {
                    self.release_control(*t, control);
                }
            } else if !down && let Some(Some(b)) = busses.get(i) {
                for control in [TrackControl::Volume, TrackControl::Pan] {
                    self.release_owner_control(AutoOwner::Strip(Strip::Bus(*b)), control);
                }
            }
            return;
        }
        // A bus's strip: its solo, mute and selection.
        if down {
            let bus = |base: u8| {
                (base..base + STRIPS as u8)
                    .contains(&n)
                    .then(|| busses.get((n - base) as usize).copied().flatten())
                    .flatten()
            };
            if let Some(b) = bus(SOLO) {
                return self.modify(|p| {
                    if let Some(c) = p.strip_channel_mut(Strip::Bus(b)) {
                        c.solo ^= true;
                    }
                });
            } else if let Some(b) = bus(MUTE) {
                return self.modify(|p| {
                    if let Some(c) = p.strip_channel_mut(Strip::Bus(b)) {
                        c.muted ^= true;
                    }
                });
            } else if let Some(b) = bus(SELECT) {
                self.console.selected = Some(Strip::Bus(b));
                self.console.picks = vec![Strip::Bus(b)];
                return;
            }
        }
        let m = &mut self.surfaces[index];
        match n {
            SHIFT => m.shift = down,
            OPTION => m.option = down,
            CTRL => m.ctrl = down,
            ALT => m.alt = down,
            REWIND => m.running = if down { -1 } else { 0 },
            FAST_FORWARD => m.running = if down { 1 } else { 0 },
            TIME_FORMAT => m.time_held = down,
            _ if (F1..F1 + 8).contains(&n) => m.held_f[(n - F1) as usize] = down,
            _ => {}
        }
        if !down {
            return;
        }
        let (shift, ctrl, alt, video, zoom) = (m.shift, m.ctrl, m.alt, m.video, m.zoom);
        let settings = m.assign == Assign::Settings;
        // The channel strips' buttons.
        if let Some(t) = strip(REC) {
            return self.toggle_arm(t);
        } else if let Some(t) = strip(SOLO) {
            return self.modify(|p| {
                if let Some(tr) = p.track_by_id_mut(t) {
                    tr.solo ^= true;
                }
            });
        } else if let Some(t) = strip(MUTE) {
            return self.modify(|p| {
                if let Some(tr) = p.track_by_id_mut(t) {
                    tr.muted ^= true;
                }
            });
        } else if let Some(t) = strip(SELECT) {
            self.selected_track = Some(t);
            return;
        } else if (VPOT_PUSH..VPOT_PUSH + STRIPS as u8).contains(&n) {
            let i = (n - VPOT_PUSH) as usize;
            return self.surface_push(index, i, tracks.get(i).copied().flatten());
        }
        // An Extender has only its strips.
        if self.is_extender(index) {
            return;
        }
        // The user's mapping first.
        if let Some(mapped) = self.surface_mapped(index, n) {
            if let Some(id) = mapped {
                crate::mackie_config::run_host_function(self, &id);
            }
            return;
        }
        let option = self.surfaces[index].option;
        match n {
            OUTPUT | INPUT | PAN | SENDS | INSERTS | SETTINGS => self.surface_assign(index, n),
            BANK_LEFT | BANK_RIGHT | CHANNEL_LEFT | CHANNEL_RIGHT => {
                let by = match n {
                    BANK_LEFT => -(STRIPS as i64),
                    BANK_RIGHT => STRIPS as i64,
                    CHANNEL_LEFT => -1,
                    _ => 1,
                };
                if option && matches!(n, CHANNEL_LEFT | CHANNEL_RIGHT) {
                    // Option: the selected track moves up or down.
                    self.surface_track_order(by < 0);
                } else {
                    self.each_mackie(|m| m.bank = (m.bank + by).max(0));
                }
            }
            FLIP => self.each_mackie(|m| m.flip ^= true),
            VIDEO => self.each_mackie(|m| {
                m.video ^= true;
                m.bank = 0;
            }),
            // Shift: the strips over the tracks, the busses or both.
            METERS if shift && !video => self.each_mackie(|m| {
                m.channels = m.channels.next();
                m.bank = 0;
            }),
            METERS => self.each_mackie(|m| m.meters ^= true),
            // Shift: the cursor's time reads zero.
            TIME_FORMAT if shift => self.set_time_at_cursor(Tick::ZERO),
            // F1–F8: markers 1–8 (Shift: 9–16); Alt: the Scripting menu's
            // scripts; with Settings they are held, not pressed.
            _ if (F1..F1 + 8).contains(&n) && !settings => {
                let k = n - F1 + if shift { 8 } else { 0 };
                if alt {
                    if let Some(&c) = SCRIPTS.get(k as usize) {
                        self.surface_cmd(c);
                    }
                } else {
                    self.run_extra(Extra::GoToMarker(k + 1));
                }
            }
            ADD_TRACK if video => self.surface_cmd(Cmd::InsertVideoTrack),
            ADD_TRACK => self.surface_cmd(Cmd::InsertAudioTrack),
            ADD_BUS => self.surface_cmd(Cmd::InsertAudioBus),
            MASTER_BUS => self.surface_cmd(Cmd::Window(crate::dock::Tab::Master)),
            VIDEO_PREVIEW => self.surface_cmd(Cmd::Window(crate::dock::Tab::Preview)),
            PLUG_INS => self.surface_cmd(Cmd::Window(crate::dock::Tab::PluginManager)),
            BUS_TRACKS if video => self.surface_cmd(Cmd::VideoBusTrack),
            BUS_TRACKS => self.surface_cmd(Cmd::AudioBusTracks),
            DOCK_AREA => self.run_extra(Extra::DockArea(Some(crate::dock::Area::Bottom))),
            TRACK_LIST => self.run_extra(Extra::FoldTrackHeaders),
            AUTOMATION => self.each_mackie(|m| m.automation ^= true),
            BYPASS_FX => self.surface_cmd(Cmd::BypassAudioFx),
            METRONOME => self.surface_cmd(Cmd::Metronome),
            SURROUND if video => self.surface_cmd(Cmd::ExternalMonitor),
            SURROUND => {
                use bv_core::ChannelLayout;
                self.modify(|p| {
                    p.settings.channels = if p.settings.channels == ChannelLayout::Surround51 {
                        ChannelLayout::Stereo
                    } else {
                        ChannelLayout::Surround51
                    };
                });
            }
            DOWNMIX if video => self.preview.toggle_split_screen(),
            DOWNMIX => self.surface_cmd(Cmd::Downmix),
            DIM if video => self.preview.overlays ^= true,
            DIM => self.surface_cmd(Cmd::Dim),
            SAVE => self.surface_cmd(Cmd::Save),
            UNDO if shift => self.surface_cmd(Cmd::Redo),
            UNDO => self.surface_cmd(Cmd::Undo),
            MARKER | REGION => self.surface_marks(n == MARKER, shift, ctrl),
            LOOP if !shift => self.surface_cmd(Cmd::LoopPlayback),
            MARK_IN | MARK_OUT => self.surface_mark(n == MARK_IN, shift),
            EVENT_TRIM if shift => self.surface_cmd(Cmd::CenterOnCursor),
            EVENT_TRIM => self.surface_event_trim(),
            RTZ => {
                let end = self.project().duration();
                self.seek_playback(if shift { end } else { Tick::ZERO });
            }
            STOP if shift => self.surface_cmd(Cmd::BuildRamPreview),
            STOP => self.stop(),
            PLAY if shift => self.preview_cursor(),
            PLAY | FOOT_A => self.toggle_play(),
            RECORD if shift => self.surface_cmd(Cmd::RenderToNewTrack),
            RECORD | FOOT_B => self.toggle_recording(),
            UP | DOWN if zoom => self.zoom_on_cursor(if n == UP { 0.8 } else { 1.25 }),
            UP | DOWN => self.surface_focus(n == UP),
            LEFT | RIGHT => self.surface_arrow(index, n == RIGHT),
            ZOOM => self.surfaces[index].zoom ^= true,
            SCRUB => self.surfaces[index].scrub ^= true,
            _ => {}
        }
    }

    /// Left and Right: by grid marks (Ctrl: by edits on the focus track;
    /// Zoom: by pixels, Ctrl: by markers), Shift selecting along; with
    /// Event Trim, the previous or next event edge.
    fn surface_arrow(&mut self, index: usize, right: bool) {
        let m = &self.surfaces[index];
        let (shift, ctrl, zoom) = (m.shift, m.ctrl, m.zoom);
        if self.timeline.edge_trim.is_some() && !zoom {
            return self.select_event_edge(!right);
        }
        let from = self.cursor;
        match (zoom, ctrl) {
            (false, false) => self.move_by_grid(if right { 1 } else { -1 }),
            (false, true) => self.surface_cmd(if right {
                Cmd::EditPointRight
            } else {
                Cmd::EditPointLeft
            }),
            (true, true) => self.surface_cmd(if right {
                Cmd::MarkerRight
            } else {
                Cmd::MarkerLeft
            }),
            (true, false) => {
                let px = Tick((self.timeline.ticks_per_px.round() as i64).max(1));
                let t = if right { from + px } else { from - px };
                self.seek_playback(t.max(Tick::ZERO));
            }
        }
        if shift && self.cursor != from {
            self.extend_selection(from);
        }
    }

    /// Event Trim: the edge nearest the cursor of the focus track's event
    /// under it (or nearest) gets the red bracket; again, none.
    fn surface_event_trim(&mut self) {
        if self.timeline.edge_trim.take().is_some() {
            return;
        }
        let project = self.project().clone();
        let at = self.cursor;
        let Some(track) = self.focused_track().and_then(|t| project.track(t)) else {
            return;
        };
        let nearest = track
            .clips
            .iter()
            .flat_map(|c| [(c.start, c.id, false), (c.end(), c.id, true)])
            .min_by_key(|(t, ..)| (*t - at).abs());
        if let Some((t, id, end)) = nearest {
            self.timeline.trim_edge(&project, id, end, false);
            self.seek(t);
        }
    }

    /// Marker (Region): one at the cursor (the time selection), Shift a CD
    /// index (CD track); Ctrl takes the one at the cursor away.
    fn surface_marks(&mut self, marker: bool, shift: bool, ctrl: bool) {
        let at = self.cursor;
        let near = self.frame_rate().frame_duration();
        match (marker, shift, ctrl) {
            (true, false, false) => self.insert_marker(),
            (true, true, false) => self.surface_cmd(Cmd::InsertCdIndex),
            (false, false, false) => self.insert_region(),
            (false, true, false) => self.surface_cmd(Cmd::InsertCdTrack),
            (true, false, true) => self.modify(|p| {
                if let Some(i) = p.markers.iter().position(|m| (m.time - at).abs() < near) {
                    p.markers.remove(i);
                }
            }),
            (true, true, true) => self.modify(|p| {
                if let Some(i) = p.cd_indices.iter().position(|m| (m.time - at).abs() < near) {
                    p.cd_indices.remove(i);
                }
            }),
            (false, false, true) => self.modify(|p| {
                if let Some(i) = p.regions.iter().position(|r| r.range.contains(at)) {
                    p.regions.remove(i);
                }
            }),
            (false, true, true) => self.modify(|p| {
                if let Some(i) = p.cd_tracks.iter().position(|r| r.range.contains(at)) {
                    p.cd_tracks.remove(i);
                }
            }),
        }
    }

    /// Option with Channel < or >: the selected track one place up or down.
    fn surface_track_order(&mut self, up: bool) {
        let Some(t) = self.focused_track() else {
            return;
        };
        let Some(i) = self.project().tracks.iter().position(|x| x.id == t) else {
            return;
        };
        let n = self.project().tracks.len();
        let to = if up {
            i.checked_sub(1)
        } else {
            (i + 1 < n).then_some(i + 1)
        };
        if let Some(to) = to {
            let _ = self.edit(|p| p.move_track_to(t, to));
        }
    }

    /// Mark In and Mark Out: the loop region's start or end at the cursor;
    /// with Shift, the cursor to it.
    fn surface_mark(&mut self, start: bool, go: bool) {
        let r = self.project().loop_region;
        if go {
            if let Some(r) = r {
                self.seek_playback(if start { r.start } else { r.end() });
            }
            return;
        }
        let at = self.cursor;
        let (a, b) = match (r, start) {
            (Some(r), true) => (at, r.end().max(at)),
            (Some(r), false) => (r.start.min(at), at),
            (None, true) => (at, at),
            (None, false) => (Tick::ZERO, at),
        };
        let r = bv_core::TimeRange::from_bounds(a, b);
        self.set_loop_region((r.duration > Tick::ZERO).then_some(r));
    }

    /// Up and Down: the track above or below gets the focus.
    fn surface_focus(&mut self, up: bool) {
        let tracks: Vec<TrackId> = self.project().tracks.iter().map(|t| t.id).collect();
        let at = self
            .focused_track()
            .and_then(|f| tracks.iter().position(|&t| t == f));
        let next = match at {
            Some(i) if up => i.checked_sub(1),
            Some(i) => (i + 1 < tracks.len()).then_some(i + 1),
            None => (!tracks.is_empty()).then_some(0),
        };
        if let Some(i) = next {
            self.selected_track = Some(tracks[i]);
        }
    }

    /// Zooms the timeline by `factor` (below 1: in) toward the needle.
    fn zoom_on_cursor(&mut self, factor: f64) {
        let t = self.display_time();
        self.timeline.zoom_around(factor, t);
    }

    /// What strip `strip` (track `t`) of surface `index` shows below its
    /// name: the value its V-Pot sets (in Meters mode, its meter but for a
    /// moment after a change).
    fn strip_value(&mut self, index: usize, strip: usize, t: TrackId) -> String {
        let Some(tr) = self.project().track(t).cloned() else {
            return String::new();
        };
        let m = &self.surfaces[index];
        let (assign, held, flip, axis) = (m.assign, m.held_f, m.flip, m.axis);
        let fresh = m
            .changed
            .is_some_and(|(s, at)| s == strip && at.elapsed() < SHOW_VALUE);
        if m.meters && !fresh && tr.kind == TrackKind::Audio {
            let db = self
                .track_meters
                .get(&t)
                .map_or(-96.0, |m| m.level(0).max(m.level(1)));
            return meter_text(db);
        }
        let audio = tr.kind == TrackKind::Audio;
        match assign {
            Assign::Settings if held[0] => match tr.automation.mode {
                AutomationMode::Off => "Off".into(),
                AutomationMode::Read => "Read".into(),
                AutomationMode::Touch => "Touch".into(),
                AutomationMode::Latch => "Latch".into(),
            },
            Assign::Settings if held[1] && audio => tr.pan_mode.name().into(),
            Assign::Settings if held[2] => if tr.invert_phase { "Inv" } else { "Norm" }.into(),
            Assign::Settings if held[3] && audio => match tr.input_monitor {
                InputMonitor::Off => "MonOff".into(),
                InputMonitor::On => "MonOn".into(),
                InputMonitor::Auto => "MonAut".into(),
            },
            Assign::Settings if held[4] && audio => {
                let pre = self
                    .surface_send(index)
                    .and_then(|to| tr.sends.iter().find(|s| s.to == to))
                    .is_some_and(|s| s.pre_volume);
                if pre { "PreVol" } else { "PstVol" }.into()
            }
            Assign::Settings if held[5] => "Deflt".into(),
            Assign::Output | Assign::Input => self.choice_text(index, strip, t),
            Assign::Plugins => {
                let i = self.surfaces[index].showing[strip];
                tr.effects
                    .get(i)
                    .map_or_else(|| "------".into(), crate::ofx::fx_name)
            }
            Assign::Pan if audio && !flip => {
                let owner = bv_timeline::SurroundOwner::Track(t);
                if self.project().surround_pans(owner) {
                    let p = self.project();
                    let q = crate::tabs::surround::point_now(p, owner, self.display_time());
                    let center = p.surround(owner).map_or(0.0, |s| s.center_db);
                    match axis {
                        0 => format!("X{:+.0}", q[0] * 100.0),
                        1 => format!("Y{:+.0}", q[1] * 100.0),
                        _ => format!("C{center:+.1}"),
                    }
                } else {
                    pan_text(self.surface_value(index, t, TrackControl::Pan))
                }
            }
            Assign::Sends if audio && !flip => {
                self.surface_send(index).map_or_else(String::new, |to| {
                    db_text(self.surface_value(index, t, TrackControl::Send(to)))
                })
            }
            _ if audio => db_text(self.surface_value(index, t, TrackControl::Volume)),
            _ => format!(
                "{:.0}%",
                self.surface_value(index, t, TrackControl::Level) * 100.0
            ),
        }
    }

    /// The display's two lines for surface `index`.
    fn surface_display(
        &mut self,
        index: usize,
        tracks: &[Option<TrackId>],
        mapping: bool,
    ) -> String {
        let m = &self.surfaces[index];
        let (assign, time_held) = (m.assign, m.time_held);
        let extender = self.is_extender(index);
        // Configure Mackie Control open: the channel mapping.
        if mapping {
            let first = self
                .settings
                .prefs
                .control_devices
                .get(index)
                .map_or(0, |d| d.first_track);
            let name = if extender {
                "Mackie Control Extender"
            } else {
                "Mackie Control"
            };
            return line(name)
                + &line(&format!("Channels {}-{}", first + 1, first + STRIPS as u32));
        }
        if time_held && !extender {
            use crate::timefmt::RulerFormatExt;
            let f = self.effective_ruler_format();
            return line(&format!("Time format: {}", crate::i18n::tr(f.label())))
                + &line(&format!("{:>56}", "V-Pot 8: change"));
        }
        // PE's preset, worked out before the project is borrowed.
        let preset = if assign == Assign::PluginEdit && !extender {
            self.pe_preset(index)
        } else {
            String::new()
        };
        let p = self.project();
        match assign {
            Assign::PluginEdit if !extender => {
                if let Some(e) = self.surfaces[index].edit
                    && let Some(tr) = p.track(e.track)
                    && let Some(fx) = tr.effects.get(e.effect)
                {
                    let mut names = String::new();
                    let mut values = String::new();
                    for k in 0..4 {
                        let i = e.page * 4 + k;
                        let (name, value) = if i == 0 {
                            (
                                "Bypass".to_string(),
                                if fx.enabled { "Off" } else { "On" }.to_string(),
                            )
                        } else if i < fx_params(fx) {
                            let spec = fx.kind.params()[i - 1];
                            let v = fx.param(i - 1);
                            let value = match spec.kind {
                                bv_timeline::ParamKind::Toggle => {
                                    if v >= 0.5 { "On" } else { "Off" }.to_string()
                                }
                                _ => format!("{v:.2}"),
                            };
                            (crate::i18n::tr(spec.name).to_string(), value)
                        } else {
                            (String::new(), String::new())
                        };
                        names.push_str(&cell(&name));
                        values.push_str(&cell(&value));
                    }
                    let pages = fx_params(fx).div_ceil(4);
                    names.push_str(&cell("Page"));
                    values.push_str(&cell(&format!("{}/{}", e.page + 1, pages)));
                    names.push_str(&cell("Preset"));
                    values.push_str(&cell(&preset));
                    names.push_str(&cell("Track"));
                    values.push_str(&cell(&crate::i18n::track_name(tr)));
                    names.push_str(&cell("Effect"));
                    values.push_str(&cell(&crate::ofx::fx_name(fx)));
                    return names + &values;
                }
            }
            Assign::PluginChain if !extender => {
                if let Some(c) = self.surfaces[index].chain
                    && let Some(tr) = p.track(c.track)
                {
                    let mut names = String::new();
                    let mut values = String::new();
                    for s in 0..3 {
                        let at = c.page * 3 + s;
                        let (name, add) = match c.picks[s]
                            .checked_sub(1)
                            .and_then(|i| bv_timeline::EffectKind::AUDIO.get(i))
                        {
                            Some(k) => (format!("*{}", crate::i18n::tr(k.name())), "Add"),
                            None => (
                                tr.effects
                                    .get(at)
                                    .map_or_else(|| "<None>".into(), crate::ofx::fx_name),
                                "",
                            ),
                        };
                        names.push_str(&cell(&name));
                        names.push_str(&cell(add));
                        values.push_str(&cell(&format!("Slot{}", at + 1)));
                        values.push_str(&cell(""));
                    }
                    let pages = tr.effects.len() / 3 + 1;
                    names.push_str(&cell(&crate::i18n::track_name(tr)));
                    values.push_str(&cell(&format!("{}/{}", c.page + 1, pages)));
                    names.push_str(&cell("Chain"));
                    values.push_str(&cell(""));
                    return names + &values;
                }
            }
            _ => {}
        }
        let busses = self.surface_busses(index);
        let mut text = String::new();
        let mut values = String::new();
        for (i, t) in tracks.iter().enumerate() {
            let bus = busses.get(i).copied().flatten();
            let name = match (t, bus) {
                (Some(t), _) => self
                    .project()
                    .track(*t)
                    .map_or(String::new(), crate::i18n::track_name),
                (None, Some(b)) => crate::i18n::bus_name(self.project(), b),
                _ => String::new(),
            };
            text.push_str(&cell(&name));
            let value = match (t, bus) {
                (Some(t), _) => self.strip_value(index, i, *t),
                (None, Some(b)) => self.bus_value(index, i, b),
                _ => String::new(),
            };
            values.push_str(&cell(&value));
        }
        text + &values
    }

    /// What surface `index` shows, sent where it changed.
    fn surface_feedback(&mut self, index: usize, device: &ControlDevice, mapping: bool) {
        let Some(out) = device
            .output
            .as_ref()
            .and_then(|o| self.midi.outputs.get(o))
            .cloned()
        else {
            return;
        };
        let extender = device.extender;
        let tracks = self.surface_tracks(index);
        let busses = self.surface_busses(index);
        let project = self.project().clone();
        let m = &self.surfaces[index];
        let (flip, assign) = (m.flip, m.assign);
        // Faders, unless touched: what they set.
        let mut faders = [None; STRIPS + 1];
        for (i, t) in tracks.iter().enumerate() {
            faders[i] = t.and_then(|t| {
                let tr = project.track(t)?;
                Some(match (tr.kind, flip, assign) {
                    (TrackKind::Audio, true, Assign::Pan) => {
                        ((self.surface_value(index, t, TrackControl::Pan) + 1.0) / 2.0 * 16383.0)
                            .round() as u16
                    }
                    (TrackKind::Audio, true, Assign::Sends) => {
                        self.surface_send(index).map_or(0, |to| {
                            gain_to_fader(self.surface_value(index, t, TrackControl::Send(to)))
                        })
                    }
                    (TrackKind::Audio, ..) => {
                        gain_to_fader(self.surface_value(index, t, TrackControl::Volume))
                    }
                    (TrackKind::Video, ..) => (self
                        .surface_value(index, t, TrackControl::Level)
                        .clamp(0.0, 1.0)
                        * 16383.0)
                        .round() as u16,
                })
            });
        }
        // A bus's: its volume (with Flip in Pan its pan).
        for (i, b) in busses.iter().enumerate() {
            if let Some(b) = *b {
                faders[i] = Some(if flip && assign == Assign::Pan {
                    ((self.surface_bus_value(index, b, TrackControl::Pan) + 1.0) / 2.0 * 16383.0)
                        .round() as u16
                } else {
                    gain_to_fader(self.surface_bus_value(index, b, TrackControl::Volume))
                });
            }
        }
        if !extender {
            faders[STRIPS] = Some(gain_to_fader(10f32.powf(project.master.gain_db / 20.0)));
        }
        let m = &mut self.surfaces[index];
        for (ch, value) in faders.into_iter().enumerate() {
            if ch == STRIPS && extender {
                continue;
            }
            let value = value.unwrap_or(0);
            if !m.touched[ch] && m.faders[ch] != Some(value) {
                out.send(&[0xE0 | ch as u8, (value & 0x7F) as u8, (value >> 7) as u8]);
                m.faders[ch] = Some(value);
            }
        }
        // The V-Pots' rings: a dot for pan, a bar for a send's level.
        let mut rings = [0u8; STRIPS];
        for (i, t) in tracks.iter().enumerate() {
            let Some(t) = *t else {
                continue;
            };
            if project.track(t).is_none_or(|t| t.kind != TrackKind::Audio) || flip {
                continue;
            }
            rings[i] = match assign {
                Assign::Pan => {
                    let pan = self.surface_value(index, t, TrackControl::Pan);
                    0x40 | (1 + ((pan + 1.0) / 2.0 * 10.0).round() as u8)
                }
                Assign::Sends => self.surface_send(index).map_or(0, |to| {
                    let g = self.surface_value(index, t, TrackControl::Send(to));
                    let pos = (gain_to_fader(g) as f32 / 16383.0 * 11.0).round() as u8;
                    if pos == 0 { 0 } else { 0x20 | pos }
                }),
                _ => 0,
            };
        }
        for (i, b) in busses.iter().enumerate() {
            if let Some(b) = *b
                && assign == Assign::Pan
                && !flip
            {
                let pan = self.surface_bus_value(index, b, TrackControl::Pan);
                rings[i] = 0x40 | (1 + ((pan + 1.0) / 2.0 * 10.0).round() as u8);
            }
        }
        let m = &mut self.surfaces[index];
        for (i, &ring) in rings.iter().enumerate() {
            if m.rings[i] != Some(ring) {
                out.send(&[0xB0, 0x30 + i as u8, ring]);
                m.rings[i] = Some(ring);
            }
        }
        // Lights.
        let mut lights: Vec<(u8, bool)> = Vec::new();
        for (i, t) in tracks.iter().enumerate() {
            let tr = t.and_then(|t| project.track(t));
            let bus = busses
                .get(i)
                .copied()
                .flatten()
                .and_then(|b| project.strip_channel(Strip::Bus(b)).map(|c| (b, c)));
            let i = i as u8;
            lights.push((note::REC + i, tr.is_some_and(|t| t.armed)));
            lights.push((
                note::SOLO + i,
                tr.is_some_and(|t| t.solo) || bus.is_some_and(|(_, c)| c.solo),
            ));
            lights.push((
                note::MUTE + i,
                tr.is_some_and(|t| t.muted) || bus.is_some_and(|(_, c)| c.muted),
            ));
            lights.push((
                note::SELECT + i,
                tr.is_some_and(|t| Some(t.id) == self.focused_track())
                    || bus.is_some_and(|(b, _)| self.console.selected == Some(Strip::Bus(b))),
            ));
        }
        if !extender {
            let playing = self.is_playing();
            let m = &self.surfaces[index];
            let beats = self.effective_ruler_format() == crate::timefmt::RulerFormat::MeasuresBeats;
            lights.extend([
                (note::OUTPUT, m.assign == Assign::Output),
                (note::INPUT, m.assign == Assign::Input),
                (note::PAN, m.assign == Assign::Pan),
                (note::SENDS, m.assign == Assign::Sends),
                (
                    note::INSERTS,
                    matches!(
                        m.assign,
                        Assign::Plugins | Assign::PluginEdit | Assign::PluginChain
                    ),
                ),
                (note::SETTINGS, m.assign == Assign::Settings),
                (note::FLIP, m.flip),
                (note::VIDEO, m.video),
                (note::METERS, m.meters),
                (note::AUTOMATION, m.automation),
                (note::BYPASS_FX, project.bypass_audio_fx),
                (note::METRONOME, self.metronome),
                (
                    note::SURROUND,
                    project.settings.channels == bv_core::ChannelLayout::Surround51,
                ),
                (note::DOWNMIX, Cmd::Downmix.checked(self) == Some(true)),
                (note::DIM, Cmd::Dim.checked(self) == Some(true)),
                (note::PLAY, playing),
                (note::STOP, !playing),
                (note::RECORD, self.engine.is_recording()),
                (note::LOOP, self.loop_enabled),
                (note::EVENT_TRIM, self.timeline.edge_trim.is_some()),
                (note::ZOOM, m.zoom),
                (note::SCRUB, m.scrub),
                (note::SMPTE, !beats),
                (note::BEATS, beats),
            ]);
        }
        let m = &mut self.surfaces[index];
        for (n, on) in lights {
            if m.lights.get(&n) != Some(&on) {
                out.send(&[0x90, n, if on { 0x7F } else { 0x00 }]);
                m.lights.insert(n, on);
            }
        }
        // The display.
        let lcd: Vec<u8> = self.surface_display(index, &tracks, mapping).into_bytes();
        let m = &mut self.surfaces[index];
        if m.lcd.as_ref() != Some(&lcd) {
            let _ = out.send_sysex(&lcd_message(extender, 0, &lcd));
            m.lcd = Some(lcd);
        }
        if extender {
            // Meters only.
            return self.surface_meters(&tracks, &busses, &out);
        }
        // The assignment display.
        let m = &mut self.surfaces[index];
        let code = m.assign.code(m.video);
        if m.assignment != Some(code) {
            out.send(&[0xB0, 0x4B, code[0] & 0x3F]);
            out.send(&[0xB0, 0x4A, code[1] & 0x3F]);
            m.assignment = Some(code);
        }
        // The time, in the ruler's format.
        let shown = self.display_time();
        let digits = clock_digits(&self.ruler_text(shown));
        let m = &mut self.surfaces[index];
        if m.clock != Some(digits) {
            for (i, &d) in digits.iter().enumerate() {
                if m.clock.is_none_or(|c| c[i] != d) {
                    out.send(&[0xB0, 0x40 + i as u8, d & 0x3F]);
                }
            }
            m.clock = Some(digits);
        }
        self.surface_meters(&tracks, &busses, &out);
    }

    /// Meters: channel pressure, the strip in the high nibble.
    fn surface_meters(
        &self,
        tracks: &[Option<TrackId>],
        busses: &[Option<BusId>],
        out: &bv_platform::MidiOut,
    ) {
        for (i, t) in tracks.iter().enumerate() {
            let bus = busses.get(i).copied().flatten();
            let meter = match (t, bus) {
                (Some(t), _) => self.track_meters.get(t),
                (None, Some(b)) => self.strip_meters.get(&Strip::Bus(b)),
                _ => None,
            };
            let db = meter.map_or(-96.0, |m| m.level(0).max(m.level(1)));
            let level = ((db + 60.0) / 60.0 * 12.0).clamp(0.0, 12.0) as u8;
            if level > 0 {
                out.send(&[0xD0, (i as u8) << 4 | level]);
            }
        }
    }
}

/// Alt with F1–F16: the Scripting menu's first scripts, in its order.
const SCRIPTS: [Cmd; 10] = {
    use crate::app::more_actions::Script::*;
    [
        Cmd::RunScript(ExportChapters),
        Cmd::RunScript(ExportCaptionsDvd),
        Cmd::RunScript(ExportCaptionsQuickTime),
        Cmd::RunScript(ExportCaptionsSami),
        Cmd::RunScript(ExportCaptionsSubRip),
        Cmd::ExportEdl,
        Cmd::RunScript(ExportRegionsAsSubtitles),
        Cmd::RunScript(GroupVideoAndAudio),
        Cmd::RunScript(PromoteMediaCaptions),
        Cmd::RunScript(RemoveLetterboxing),
    ]
};

#[cfg(test)]
mod tests {
    use super::*;

    fn surface(app: &mut BananaApp, extenders: u32) {
        app.settings.prefs.external_control = true;
        app.settings.prefs.control_devices = (0..=extenders)
            .map(|i| ControlDevice {
                kind: SurfaceKind::MackieControl,
                input: Some(format!("Surface {i}")),
                output: None,
                first_track: i * STRIPS as u32,
                generic: Default::default(),
                extender: i > 0,
                mackie_map: Vec::new(),
            })
            .collect();
        app.surfaces
            .resize_with(extenders as usize + 1, Mackie::default);
    }

    #[test]
    fn faders_knobs_and_displays_as_the_protocol_has_them() {
        // 0 dB sits where the headers' sliders have it, and comes back.
        let unity = gain_to_fader(1.0);
        assert!((fader_to_gain(unity) - 1.0).abs() < 1e-3);
        assert_eq!(gain_to_fader(0.0), 0);
        assert_eq!(gain_to_fader(MAX_GAIN), 16383);
        assert_eq!((vpot_ticks(0x03), vpot_ticks(0x43)), (3, -3));
        assert_eq!(cell("Audio 12"), "Audio  ");
        assert_eq!(cell("Áudio"), "Audio  ");
        let m = lcd_message(false, 0, b"Hi");
        assert_eq!(&m[..7], &[0xF0, 0, 0, 0x66, 0x14, 0x12, 0]);
        assert_eq!(m.last(), Some(&0xF7));
        // An Extender's display answers to its own id.
        assert_eq!(lcd_message(true, 0, b"Hi")[4], 0x15);
        // 01:02:03;04 right to left: frames, seconds, minutes, hours.
        let d = clock_digits("01:02:03;04");
        assert_eq!(&d, b"40 302010 ");
        // Measures and beats, and one number alone.
        assert_eq!(&clock_digits("12.3.064")[..7], b"4603021");
        assert_eq!(&clock_digits("441000")[..6], b"000144");
        assert_eq!(pan_text(-0.5), "L50");
        assert_eq!(pan_text(0.0), "C");
    }

    #[test]
    fn a_surface_moves_tracks_and_plays() {
        let mut app = BananaApp::new(crate::egui::Context::default());
        app.load_demo();
        surface(&mut app, 0);
        let tracks = app.surface_tracks(0);
        // The strips are over the audio tracks.
        let t = tracks[0].unwrap();
        assert_eq!(app.project().track(t).unwrap().kind, TrackKind::Audio);
        // The fader to -6 dB: the track's trim.
        let v = gain_to_fader(0.5);
        app.surface_message(0, [0xE0, (v & 0x7F) as u8, (v >> 7) as u8]);
        assert!((app.project().track(t).unwrap().level - 0.5).abs() < 1e-3);
        // Its MUTE button.
        app.surface_message(0, [0x90, note::MUTE, 0x7F]);
        assert!(app.project().track(t).unwrap().muted);
        // Pan: the V-Pot.
        app.surface_message(0, [0xB0, 16, 0x05]);
        assert!((app.project().track(t).unwrap().pan - 0.1).abs() < 1e-4);
        // Flip: the fader pans, the V-Pot sets the volume.
        app.surface_message(0, [0x90, note::FLIP, 0x7F]);
        app.surface_message(0, [0xE0, 0, 0]);
        assert!((app.project().track(t).unwrap().pan + 1.0).abs() < 1e-3);
        // Bank right: the strips move on by eight.
        app.surface_message(0, [0x90, note::BANK_RIGHT, 0x7F]);
        assert_eq!(app.surfaces[0].bank, 8);
        assert!(app.surface_tracks(0).iter().all(Option::is_none));
        // Video: the strips over the video tracks, from the first.
        app.surface_message(0, [0x90, note::VIDEO, 0x7F]);
        let v = app.surface_tracks(0)[0].unwrap();
        assert_eq!(app.project().track(v).unwrap().kind, TrackKind::Video);
        // The jog wheel: three frames on.
        app.cursor = Tick::ZERO;
        app.surface_message(0, [0xB0, 60, 0x03]);
        assert_eq!(app.cursor, app.frame_rate().frame_start(3));
        // Metronome is its own button now.
        let on = app.metronome;
        app.surface_message(0, [0x90, note::METRONOME, 0x7F]);
        assert_ne!(app.metronome, on);
    }

    /// Shift with Meters/Values: the strips over the busses, then the
    /// tracks and the busses, then the tracks again.
    #[test]
    fn shift_meters_puts_the_busses_on_the_strips() {
        let mut app = BananaApp::new(crate::egui::Context::default());
        app.load_demo();
        surface(&mut app, 0);
        let busses: Vec<BusId> = (0..2).filter_map(|_| app.modify(|p| p.add_bus())).collect();
        let shift_meters = |app: &mut BananaApp| {
            app.surface_message(0, [0x90, note::SHIFT, 0x7F]);
            app.surface_message(0, [0x90, note::METERS, 0x7F]);
            app.surface_message(0, [0x90, note::SHIFT, 0x00]);
        };
        let meters = app.surfaces[0].meters;
        shift_meters(&mut app);
        assert_eq!(app.surfaces[0].channels, Channels::Busses);
        assert_eq!(app.surfaces[0].meters, meters, "Meters/Values itself kept");
        assert_eq!(app.surface_busses(0)[0], Some(busses[0]));
        assert!(app.surface_tracks(0).iter().all(Option::is_none));
        // The fader to -6 dB: the bus's level; its MUTE and SOLO; its pan.
        let v = gain_to_fader(0.5);
        app.surface_message(0, [0xE0, (v & 0x7F) as u8, (v >> 7) as u8]);
        let bus = |app: &BananaApp| app.project().bus(busses[0]).unwrap().channel.clone();
        assert!((10f32.powf(bus(&app).gain_db / 20.0) - 0.5).abs() < 1e-3);
        app.surface_message(0, [0x90, note::MUTE, 0x7F]);
        app.surface_message(0, [0x90, note::SOLO, 0x7F]);
        assert!(bus(&app).muted && bus(&app).solo);
        app.surface_message(0, [0xB0, 16, 0x05]);
        assert!((bus(&app).pan - 0.1).abs() < 1e-4);
        app.surface_message(0, [0x90, note::SELECT + 1, 0x7F]);
        assert_eq!(app.console.selected, Some(Strip::Bus(busses[1])));
        // The display names it.
        let lcd = app.surface_display(0, &app.surface_tracks(0), false);
        assert!(lcd.starts_with("Bus A"), "{lcd}");
        // Tracks and then busses.
        shift_meters(&mut app);
        let tracks = app
            .project()
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Audio)
            .count();
        assert!(app.surface_tracks(0)[0].is_some());
        assert_eq!(app.surface_busses(0)[tracks], Some(busses[0]));
        shift_meters(&mut app);
        assert_eq!(app.surfaces[0].channels, Channels::Tracks);
        assert!(app.surface_busses(0).iter().all(Option::is_none));
    }

    #[test]
    fn extenders_take_the_next_tracks_and_bank_together() {
        let mut app = BananaApp::new(crate::egui::Context::default());
        for _ in 0..20 {
            app.modify(|p| p.add_track(TrackKind::Audio));
        }
        surface(&mut app, 1);
        let audio: Vec<TrackId> = app
            .project()
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Audio)
            .map(|t| t.id)
            .collect();
        assert_eq!(app.surface_tracks(1)[0], Some(audio[8]));
        // The Mackie Control's Bank moves its Extender too.
        app.surface_message(0, [0x90, note::BANK_RIGHT, 0x7F]);
        assert_eq!(app.surface_tracks(1)[0], Some(audio[16]));
        // An Extender's fader moves its own track; its other buttons do
        // nothing.
        let v = gain_to_fader(0.5);
        app.surface_message(1, [0xE1, (v & 0x7F) as u8, (v >> 7) as u8]);
        assert!((app.project().track(audio[17]).unwrap().level - 0.5).abs() < 1e-3);
        app.surface_message(1, [0x90, note::BANK_RIGHT, 0x7F]);
        assert_eq!(app.surfaces[1].bank, 8);
    }

    #[test]
    fn sends_inserts_and_settings_work_the_strips() {
        let mut app = BananaApp::new(crate::egui::Context::default());
        let t = app.modify(|p| p.add_track(TrackKind::Audio));
        app.modify(|p| p.add_bus());
        surface(&mut app, 0);
        app.selected_track = Some(t);
        let s = app
            .surface_tracks(0)
            .iter()
            .position(|x| *x == Some(t))
            .unwrap() as u8;
        // Sends: the V-Pot raises the send to Bus A.
        app.surface_message(0, [0x90, note::SENDS, 0x7F]);
        app.surface_message(0, [0xB0, 16 + s, 0x04]);
        let send = app
            .project()
            .track(t)
            .unwrap()
            .sends
            .first()
            .copied()
            .unwrap();
        assert!(send.gain_db > -96.0);
        // Settings, F3 held, the V-Pot pressed: the phase inverted.
        app.surface_message(0, [0x90, note::SETTINGS, 0x7F]);
        app.surface_message(0, [0x90, note::F1 + 2, 0x7F]);
        app.surface_message(0, [0x90, note::VPOT_PUSH + s, 0x7F]);
        assert!(app.project().track(t).unwrap().invert_phase);
        app.surface_message(0, [0x90, note::F1 + 2, 0x00]);
        // F1 held, the V-Pot: the automation mode, Read to Touch.
        app.surface_message(0, [0x90, note::F1, 0x7F]);
        app.surface_message(0, [0xB0, 16 + s, 0x01]);
        assert_eq!(
            app.project().track(t).unwrap().automation.mode,
            AutomationMode::Touch
        );
        app.surface_message(0, [0x90, note::F1, 0x00]);
        // Inserts twice: PS; V-Pot 1 to the first effect, V-Pot 2 adds it.
        app.surface_message(0, [0x90, note::INSERTS, 0x7F]);
        assert_eq!(app.surfaces[0].assign, Assign::Plugins);
        app.surface_message(0, [0x90, note::INSERTS, 0x7F]);
        assert_eq!(app.surfaces[0].assign, Assign::PluginChain);
        app.surface_message(0, [0xB0, 16, 0x01]);
        app.surface_message(0, [0x90, note::VPOT_PUSH + 1, 0x7F]);
        let fx = app.project().track(t).unwrap().effects.clone();
        assert_eq!(fx.len(), 1);
        assert_eq!(fx[0].kind, bv_timeline::EffectKind::AUDIO[0]);
        // V-Pot 1 pressed: PE on it; V-Pot 1 there is Bypass.
        app.surface_message(0, [0x90, note::VPOT_PUSH, 0x7F]);
        assert_eq!(app.surfaces[0].assign, Assign::PluginEdit);
        app.surface_message(0, [0x90, note::VPOT_PUSH, 0x7F]);
        assert!(!app.project().track(t).unwrap().effects[0].enabled);
    }

    #[test]
    fn pe_v_pot_6_takes_a_preset() {
        let mut app = BananaApp::new(crate::egui::Context::default());
        app.load_demo();
        surface(&mut app, 0);
        let t = app.surface_tracks(0)[0].unwrap();
        let kind = bv_timeline::EffectKind::Reverb;
        app.modify(|p| {
            if let Some(tr) = p.track_by_id_mut(t) {
                tr.effects.push(bv_timeline::Effect::new(kind));
            }
        });
        app.surfaces[0].assign = Assign::PluginEdit;
        app.surfaces[0].edit = Some(FxEdit {
            track: t,
            effect: 0,
            page: 0,
            preset: None,
        });
        // At its defaults it shows (Default); V-Pot 6 turned points at the
        // first of BananaV's presets, pressed it takes it.
        assert!(app.pe_preset(0).contains("Default"));
        app.surface_message(0, [0xB0, 16 + 5, 0x01]);
        let first = crate::presets::builtin(kind)[0].0;
        assert_eq!(app.pe_preset(0), first);
        app.surface_message(0, [0x90, note::VPOT_PUSH + 5, 0x7F]);
        let presets = crate::tabs::console::effect_presets(&mut app, kind);
        assert_eq!(
            app.project().track(t).unwrap().effects[0].params,
            presets[1].1
        );
        assert_eq!(app.surfaces[0].edit.and_then(|e| e.preset), None);
        assert_eq!(app.pe_preset(0), first);
    }

    #[test]
    fn the_users_mapping_comes_before_the_defaults() {
        let mut app = BananaApp::new(crate::egui::Context::default());
        surface(&mut app, 0);
        app.settings.prefs.control_devices[0].mackie_map = vec![
            crate::mackie_config::MackieMapping {
                note: note::F1,
                shift: false,
                option: true,
                command: Some("Transport.LoopPlayback".into()),
            },
            crate::mackie_config::MackieMapping {
                note: note::FOOT_A,
                shift: false,
                option: false,
                command: None,
            },
        ];
        // Option with F1: the command mapped.
        let on = app.loop_enabled;
        app.surface_message(0, [0x90, note::OPTION, 0x7F]);
        app.surface_message(0, [0x90, note::F1, 0x7F]);
        assert_ne!(app.loop_enabled, on);
        app.surface_message(0, [0x90, note::OPTION, 0x00]);
        // Footswitch A cleared: no playback.
        app.surface_message(0, [0x90, note::FOOT_A, 0x7F]);
        assert!(!app.is_playing());
    }
}
