//! Options › Internal… (the Options menu opened with Shift held, as
//! VEGAS's): every preference of BananaV in one list — Name, Value and
//! Default, each value as text to be changed there. The names are VEGAS's
//! where it has the same setting, in its order (by kind of value: whole
//! numbers, decimals, texts, colors, lists, switches…); what BananaV lacks
//! shows greyed out. Before the search box is used every preference shows;
//! once it has been, only the internal ones (those no Preferences tab has)
//! whose name holds its text. OK reads each value changed back: whole
//! numbers as far as their digits go, decimals as C reads them, TRUE or
//! FALSE by the first letter; one that does not read, or falls outside its
//! range, is left as it was, without a word.

use std::path::PathBuf;

use bv_timeline::{FadeCurve, KeyCurve, RenderQuality};

use crate::app::BananaApp;
use crate::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use crate::i18n::{tr, trc};
use crate::settings::{
    AudioDeviceType, AutoProxies, CutAlign, FadeAlign, FrameNumbers, JklSpeed, LocateMessages,
    MtcRate, NewProjectDefaults, PreviewBackground, RecordWaveforms, Thumbnails, UiSettings,
};
use crate::theme;

/// A preference's value, as the list holds it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Value {
    Int(i64),
    UInt(u64),
    Float(f32),
    Double(f64),
    Text(String),
    Bool(bool),
    /// A list (folders, colors…): shown empty, never read back.
    List,
}

impl Value {
    fn whole(&self) -> Option<i64> {
        match *self {
            Value::Int(n) => Some(n),
            Value::UInt(n) => i64::try_from(n).ok(),
            _ => None,
        }
    }

    fn real(&self) -> Option<f64> {
        match *self {
            Value::Float(v) => Some(v as f64),
            Value::Double(v) => Some(v),
            Value::Int(n) => Some(n as f64),
            Value::UInt(n) => Some(n as f64),
            _ => None,
        }
    }

    fn flag(&self) -> Option<bool> {
        match *self {
            Value::Bool(b) => Some(b),
            _ => None,
        }
    }

    fn text(&self) -> Option<&str> {
        match self {
            Value::Text(t) => Some(t),
            _ => None,
        }
    }
}

/// How a preference's text reads back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A signed whole number of 32 or 64 bits.
    Int(u32),
    /// An unsigned whole number of 32 bits (a minus sign wraps it round).
    UInt,
    Float,
    Double,
    Text,
    Bool,
    List,
}

/// What [`Pref::get`] and [`Pref::set`] may need beyond the settings: the
/// video decoders found when the window opened.
#[derive(Clone, Debug, Default)]
pub(crate) struct Ctx {
    pub decoders: Vec<String>,
}

type Get = fn(&Ctx, &UiSettings) -> Value;
type Set = fn(&Ctx, &mut UiSettings, &Value) -> bool;

/// Where a preference lives.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Access {
    /// In the settings: read, and written back (`false`: the value means
    /// nothing there, and is left out).
    Live { get: Get, set: Set },
    /// A setting of VEGAS's that BananaV does not have yet: its value
    /// shown, greyed out.
    Soon(&'static str),
}

/// A line of the list.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Pref {
    pub name: &'static str,
    /// No Preferences tab has it: it stays in the list once it is searched.
    pub internal: bool,
    pub kind: Kind,
    /// Values outside are left out on OK.
    pub range: Option<(f64, f64)>,
    pub access: Access,
}

impl Pref {
    fn live(name: &'static str, kind: Kind, get: Get, set: Set) -> Pref {
        Pref {
            name,
            internal: false,
            kind,
            range: None,
            access: Access::Live { get, set },
        }
    }

    fn soon(name: &'static str, internal: bool, shown: &'static str) -> Pref {
        Pref {
            name,
            internal,
            kind: Kind::Int(32),
            range: None,
            access: Access::Soon(shown),
        }
    }

    fn internal(mut self) -> Pref {
        self.internal = true;
        self
    }

    fn range(mut self, lo: f64, hi: f64) -> Pref {
        self.range = Some((lo, hi));
        self
    }

    /// Its value in `s`, as the list shows it.
    pub(crate) fn shown(&self, ctx: &Ctx, s: &UiSettings) -> String {
        match self.access {
            Access::Live { get, .. } => show(&get(ctx, s)),
            Access::Soon(t) => t.to_string(),
        }
    }

    /// `text` read back into `s`, as OK does: whether it was taken.
    pub(crate) fn take(&self, ctx: &Ctx, s: &mut UiSettings, text: &str) -> bool {
        let Access::Live { set, .. } = self.access else {
            return false;
        };
        let Some(v) = read(self.kind, text) else {
            return false;
        };
        if let (Some((lo, hi)), Some(x)) = (self.range, v.real())
            && !(lo..=hi).contains(&x)
        {
            return false;
        }
        set(ctx, s, &v)
    }
}

/// The text a value shows: whole numbers in decimal, decimals as C's "%f"
/// (six places), switches TRUE or FALSE, lists nothing.
pub(crate) fn show(v: &Value) -> String {
    match v {
        Value::Int(n) => n.to_string(),
        Value::UInt(n) => n.to_string(),
        Value::Float(f) => fixed(*f as f64),
        Value::Double(d) => fixed(*d),
        Value::Text(t) => t.clone(),
        Value::Bool(true) => "TRUE".to_string(),
        Value::Bool(false) => "FALSE".to_string(),
        Value::List => String::new(),
    }
}

fn fixed(v: f64) -> String {
    if v.is_nan() {
        "nan".into()
    } else if v == f64::INFINITY {
        "inf".into()
    } else if v == f64::NEG_INFINITY {
        "-inf".into()
    } else {
        format!("{v:.6}")
    }
}

/// `text` read as a `kind` of value, as VEGAS's OK reads it: whole numbers
/// after any spaces and a sign, as far as their digits go ("2xyz" is 2);
/// none, or too many for the type, and it does not read. An unsigned one
/// with a minus wraps round ("-1" is 4294967295). Decimals as C reads
/// them (inf and nan too); a float out of its range does not read.
/// Switches TRUE or FALSE by their first letter.
pub(crate) fn read(kind: Kind, text: &str) -> Option<Value> {
    match kind {
        Kind::Bool => {
            let up = text.to_uppercase();
            if up.starts_with('T') {
                Some(Value::Bool(true))
            } else if up.starts_with('F') {
                Some(Value::Bool(false))
            } else {
                None
            }
        }
        Kind::Int(bits) => {
            let (neg, n) = digits(text)?;
            let n = if neg { -n } else { n };
            let lim = 1i128 << (bits - 1);
            (-lim..lim).contains(&n).then_some(Value::Int(n as i64))
        }
        Kind::UInt => {
            let (neg, n) = digits(text)?;
            if n > u32::MAX as i128 {
                return None;
            }
            let n = n as u32;
            let n = if neg { n.wrapping_neg() } else { n };
            Some(Value::UInt(n as u64))
        }
        Kind::Float => {
            let v = decimal(text)?;
            let f = v as f32;
            (!v.is_finite() || f.is_finite()).then_some(Value::Float(f))
        }
        Kind::Double => decimal(text).map(Value::Double),
        Kind::Text => Some(Value::Text(text.to_string())),
        Kind::List => None,
    }
}

/// The sign and digits a text starts with (after spaces); `None` without
/// a digit or past any whole number we hold.
fn digits(text: &str) -> Option<(bool, i128)> {
    let t = text.trim_start();
    let (neg, t) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let n = t.bytes().take_while(u8::is_ascii_digit).count();
    if n == 0 {
        return None;
    }
    t[..n].parse::<i128>().ok().map(|v| (neg, v))
}

/// The decimal a text starts with (after spaces), as C's `strtod` takes
/// it: a sign, digits with a point and an exponent, or inf, infinity, nan;
/// `None` without one, or past the doubles' range.
fn decimal(text: &str) -> Option<f64> {
    let t = text.trim_start();
    let b = t.as_bytes();
    let mut i = 0;
    if matches!(b.first(), Some(b'+' | b'-')) {
        i = 1;
    }
    let rest = t[i..].to_ascii_lowercase();
    for word in ["infinity", "inf", "nan"] {
        if rest.starts_with(word) {
            let v = if word == "nan" {
                f64::NAN
            } else {
                f64::INFINITY
            };
            return Some(if b.first() == Some(&b'-') { -v } else { v });
        }
    }
    let mut seen = false;
    while b.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
        seen = true;
    }
    if b.get(i) == Some(&b'.') {
        i += 1;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
            seen = true;
        }
    }
    if !seen {
        return None;
    }
    // An exponent only with its digits.
    if matches!(b.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(b.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        if b.get(j).is_some_and(u8::is_ascii_digit) {
            while b.get(j).is_some_and(u8::is_ascii_digit) {
                j += 1;
            }
            i = j;
        }
    }
    let v: f64 = t[..i].parse().ok()?;
    v.is_finite().then_some(v)
}

/// A fade type of new envelopes as VEGAS numbers it.
fn curve_code(c: KeyCurve) -> i64 {
    match c {
        KeyCurve::Hold => 0,
        KeyCurve::Fast => 2,
        KeyCurve::Slow => -2,
        KeyCurve::Smooth => 4,
        KeyCurve::Sharp => -4,
        KeyCurve::Linear | KeyCurve::Bezier | KeyCurve::BezierSplit => 1,
    }
}

fn curve_of(n: i64) -> Option<KeyCurve> {
    Some(match n {
        0 => KeyCurve::Hold,
        1 => KeyCurve::Linear,
        2 => KeyCurve::Fast,
        -2 => KeyCurve::Slow,
        4 => KeyCurve::Smooth,
        -4 => KeyCurve::Sharp,
        _ => return None,
    })
}

fn fade_code(c: FadeCurve) -> i64 {
    match c {
        FadeCurve::Linear => 1,
        FadeCurve::Fast => 2,
        FadeCurve::Slow => -2,
        FadeCurve::Smooth => 4,
        FadeCurve::Sharp => -4,
    }
}

fn fade_of(n: i64) -> Option<FadeCurve> {
    Some(match n {
        1 => FadeCurve::Linear,
        2 => FadeCurve::Fast,
        -2 => FadeCurve::Slow,
        4 => FadeCurve::Smooth,
        -4 => FadeCurve::Sharp,
        _ => return None,
    })
}

/// A new keyframe's type as VEGAS numbers it: the place in its list.
fn key_code(c: KeyCurve) -> i64 {
    KeyCurve::ALL.iter().position(|&k| k == c).unwrap_or(0) as i64
}

fn key_of(n: i64) -> Option<KeyCurve> {
    usize::try_from(n)
        .ok()
        .and_then(|i| KeyCurve::ALL.get(i).copied())
}

/// The place of `v` in `all`, and back.
fn index_of<T: PartialEq>(all: &[T], v: &T) -> i64 {
    all.iter().position(|x| x == v).unwrap_or(0) as i64
}

fn at_index<T: Copy>(all: &[T], n: i64) -> Option<T> {
    usize::try_from(n).ok().and_then(|i| all.get(i).copied())
}

/// What new projects start with (made from a new project when none was
/// chosen).
fn new_project(s: &mut UiSettings) -> &mut NewProjectDefaults {
    s.new_project.get_or_insert_with(|| {
        let p = bv_timeline::Project::default();
        NewProjectDefaults {
            settings: p.settings,
            prerender_folder: p.prerender_folder,
            recorded_folder: p.recorded_folder,
            ruler_offset: p.ruler_offset,
            summary: p.summary,
        }
    })
}

fn new_settings(s: &UiSettings) -> bv_timeline::ProjectSettings {
    s.new_project
        .as_ref()
        .map_or_else(|| bv_timeline::Project::default().settings, |d| d.settings)
}

/// A color as VEGAS keeps one: red, green and blue bytes from the lowest
/// (its alpha kept apart).
fn colorref([r, g, b, _]: [u8; 4]) -> u64 {
    r as u64 | (g as u64) << 8 | (b as u64) << 16
}

fn set_colorref(c: &mut [u8; 4], v: &Value) -> bool {
    match v.whole() {
        Some(n) if (0..=0xFF_FFFF).contains(&n) => {
            *c = [n as u8, (n >> 8) as u8, (n >> 16) as u8, c[3]];
            true
        }
        _ => false,
    }
}

/// A switch of the settings at `path`.
macro_rules! flag {
    ($name:literal, $($f:ident).+) => {
        Pref::live(
            $name,
            Kind::Bool,
            |_, s| Value::Bool(s.$($f).+),
            |_, s, v| v.flag().map(|b| s.$($f).+ = b).is_some(),
        )
    };
}

/// `v` as a whole number of at least `min` that a `u32` holds.
fn count_of(v: &Value, min: u32) -> Option<u32> {
    v.whole()
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n >= min)
}

/// A whole number (`u32`) of the settings at `path`, at least `min`.
macro_rules! count {
    ($name:literal, $kind:expr, $min:expr, $($f:ident).+) => {
        Pref::live(
            $name,
            $kind,
            |_, s| Value::Int(s.$($f).+ as i64),
            |_, s, v| count_of(v, $min).map(|n| s.$($f).+ = n).is_some(),
        )
    };
}

/// A list of choices of the settings at `path`: its place in `all`.
macro_rules! choice {
    ($name:literal, $all:expr, $($f:ident).+) => {
        Pref::live(
            $name,
            Kind::Int(32),
            |_, s| Value::Int(index_of(&$all, &s.$($f).+)),
            |_, s, v| v.whole().and_then(|n| at_index(&$all, n)).map(|c| s.$($f).+ = c).is_some(),
        )
    };
}

/// A file or folder (empty: none) of the settings at `path`.
macro_rules! path {
    ($name:literal, $($f:ident).+) => {
        Pref::live(
            $name,
            Kind::Text,
            |_, s| Value::Text(s.$($f).+.as_ref().map_or(String::new(), |p| p.display().to_string())),
            |_, s, v| v
                .text()
                .map(|t| s.$($f).+ = (!t.is_empty()).then(|| PathBuf::from(t)))
                .is_some(),
        )
    };
}

/// A name (empty: none) of the settings at `path`.
macro_rules! name {
    ($name:literal, $($f:ident).+) => {
        Pref::live(
            $name,
            Kind::Text,
            |_, s| Value::Text(s.$($f).+.clone().unwrap_or_default()),
            |_, s, v| v
                .text()
                .map(|t| s.$($f).+ = (!t.is_empty()).then(|| t.to_string()))
                .is_some(),
        )
    };
}

/// A time of the settings in seconds (`f64` or `f32`), shown as VEGAS's in
/// tenths of a microsecond.
macro_rules! seconds {
    ($name:literal, $t:ty, $($f:ident).+) => {
        Pref::live(
            $name,
            Kind::Int(64),
            |_, s| Value::Int((s.$($f).+ as f64 * 1e7).round() as i64),
            |_, s, v| v.whole().filter(|n| *n >= 0).map(|n| s.$($f).+ = (n as f64 / 1e7) as $t).is_some(),
        )
    };
}

/// A list of the settings: shown empty, left as it is.
fn list(name: &'static str) -> Pref {
    Pref::live(name, Kind::List, |_, _| Value::List, |_, _, _| false)
}

/// Every preference, in the list's order.
pub(crate) fn all() -> Vec<Pref> {
    vec![
        // Whole numbers.
        Pref::live(
            "Def Quality Preview",
            Kind::Int(32),
            |_, s| Value::Int(index_of(&RenderQuality::ALL, &s.preview.quality.level) + 1),
            |_, s, v| {
                v.whole()
                    .and_then(|n| at_index(&RenderQuality::ALL, n - 1))
                    .map(|q| s.preview.quality.level = q)
                    .is_some()
            },
        )
        .range(1.0, 4.0),
        Pref::live(
            "Def Quality Render",
            Kind::Int(32),
            |_, s| Value::Int(index_of(&RenderQuality::ALL, &new_settings(s).render_quality) + 1),
            |_, s, v| {
                v.whole()
                    .and_then(|n| at_index(&RenderQuality::ALL, n - 1))
                    .map(|q| new_project(s).settings.render_quality = q)
                    .is_some()
            },
        )
        .range(1.0, 4.0),
        Pref::soon("S3D Mode Projects", false, "0"),
        Pref::live(
            "360 mode",
            Kind::Int(32),
            |_, s| Value::Int(new_settings(s).output_360 as i64),
            |_, s, v| match v.whole() {
                Some(n @ (0 | 1)) => {
                    new_project(s).settings.output_360 = n == 1;
                    true
                }
                _ => false,
            },
        ),
        Pref::soon("HDR mode", false, "0"),
        count!(
            "Maximum Video Render Threads",
            Kind::Int(32),
            1,
            prefs.render_threads
        ),
        count!(
            "Minimum Video Render Threads",
            Kind::Int(32),
            1,
            prefs.min_render_threads
        )
        .internal(),
        count!(
            "Max For Maximum Video Render Threads",
            Kind::Int(32),
            1,
            prefs.render_threads_limit
        ),
        choice!(
            "Level of brightness for the interface",
            crate::theme::Interface::ALL,
            prefs.interface
        ),
        Pref::soon("S3D Mode External Monitor", false, "0"),
        Pref::soon("S3D Mode Video Preview", false, "0"),
        choice!("JKL / shuttle speed", JklSpeed::ALL, prefs.jkl_speed),
        Pref::live(
            "Default Transition Alignment",
            Kind::Int(32),
            |_, s| Value::Int(index_of(&CutAlign::ALL, &s.prefs.cut_overlap_align) - 1),
            |_, s, v| {
                v.whole()
                    .and_then(|n| at_index(&CutAlign::ALL, n + 1))
                    .map(|a| s.prefs.cut_overlap_align = a)
                    .is_some()
            },
        )
        .range(-1.0, 1.0),
        choice!(
            "Time Sel Envelope Editing Alignment",
            FadeAlign::ALL,
            prefs.envelope_fade_align
        )
        .range(0.0, 2.0),
        count!("Dim Attenuation", Kind::Int(32), 0, prefs.dim_db).internal(),
        Pref::soon("VU Meter Type", true, "0"),
        Pref::live(
            "Hardware Decoder selected if not AUTO",
            Kind::Int(32),
            |c, s| {
                Value::Int(match &s.decoding {
                    bv_engine::DecodeChoice::Card(n) => c
                        .decoders
                        .iter()
                        .position(|d| d == n)
                        .map_or(-1, |i| i as i64),
                    _ => -1,
                })
            },
            |c, s, v| match v.whole() {
                Some(n) if n < 0 => {
                    if matches!(s.decoding, bv_engine::DecodeChoice::Card(_)) {
                        s.decoding = bv_engine::DecodeChoice::Auto;
                    }
                    true
                }
                Some(n) => match usize::try_from(n).ok().and_then(|i| c.decoders.get(i)) {
                    Some(d) => {
                        s.decoding = bv_engine::DecodeChoice::Card(d.clone());
                        true
                    }
                    None => false,
                },
                None => false,
            },
        ),
        choice!(
            "FileIOLogging Load Level",
            crate::logs::LogLevel::ALL,
            prefs.log_plugins
        ),
        choice!(
            "FileIOLogging Read Level",
            crate::logs::LogLevel::ALL,
            prefs.log_reads
        ),
        choice!(
            "FileIOLogging Render Level",
            crate::logs::LogLevel::ALL,
            prefs.log_renders
        ),
        Pref::live(
            "Default Audio Envelope Fade Type",
            Kind::Int(32),
            |_, s| Value::Int(curve_code(s.prefs.new_audio_envelopes)),
            |_, s, v| {
                v.whole()
                    .and_then(curve_of)
                    .map(|c| s.prefs.new_audio_envelopes = c)
                    .is_some()
            },
        ),
        Pref::live(
            "eDefEnvAudioASRType",
            Kind::Int(32),
            |_, s| Value::Int(fade_code(s.prefs.new_audio_event_gain)),
            |_, s, v| {
                v.whole()
                    .and_then(fade_of)
                    .map(|c| s.prefs.new_audio_event_gain = c)
                    .is_some()
            },
        ),
        Pref::live(
            "Default FX Parameter Automation Fade Type",
            Kind::Int(32),
            |_, s| Value::Int(curve_code(s.prefs.new_audio_fx_envelopes)),
            |_, s, v| {
                v.whole()
                    .and_then(curve_of)
                    .map(|c| s.prefs.new_audio_fx_envelopes = c)
                    .is_some()
            },
        ),
        Pref::live(
            "Default Video Envelope Fade Type",
            Kind::Int(32),
            |_, s| Value::Int(curve_code(s.prefs.new_video_envelopes)),
            |_, s, v| {
                v.whole()
                    .and_then(curve_of)
                    .map(|c| s.prefs.new_video_envelopes = c)
                    .is_some()
            },
        ),
        Pref::live(
            "eDefEnvVideoASRType",
            Kind::Int(32),
            |_, s| Value::Int(fade_code(s.prefs.new_video_event_opacity)),
            |_, s, v| {
                v.whole()
                    .and_then(fade_of)
                    .map(|c| s.prefs.new_video_event_opacity = c)
                    .is_some()
            },
        ),
        Pref::live(
            "eDefEnvVideoKeyframeType",
            Kind::Int(32),
            |_, s| Value::Int(key_code(s.prefs.new_video_fx_keyframes)),
            |_, s, v| {
                v.whole()
                    .and_then(key_of)
                    .map(|c| s.prefs.new_video_fx_keyframes = c)
                    .is_some()
            },
        )
        .range(0.0, 5.0),
        Pref::live(
            "eDefEnvVideoMotionType",
            Kind::Int(32),
            |_, s| Value::Int(key_code(s.prefs.new_motion_keyframes)),
            |_, s, v| {
                v.whole()
                    .and_then(key_of)
                    .map(|c| s.prefs.new_motion_keyframes = c)
                    .is_some()
            },
        )
        .range(0.0, 5.0),
        count!(
            "Number of Control Plug-ins allowed",
            Kind::Int(32),
            0,
            prefs.max_control_devices
        )
        .internal(),
        Pref::soon("MIDI Driver Type", false, "1"),
        Pref::live(
            "Mixer Driver Model",
            Kind::Int(32),
            |_, s| {
                Value::Int(match s.prefs.audio_device_type {
                    AudioDeviceType::SystemDefault => 0,
                    AudioDeviceType::Windows => 1,
                })
            },
            |_, s, v| match v.whole() {
                Some(0) => {
                    s.prefs.audio_device_type = AudioDeviceType::SystemDefault;
                    true
                }
                Some(1) => {
                    s.prefs.audio_device_type = AudioDeviceType::Windows;
                    true
                }
                _ => false,
            },
        ),
        choice!(
            "Audio draw on record",
            RecordWaveforms::ALL,
            prefs.record_waveforms
        )
        .range(0.0, 2.0),
        count!(
            "Video Preview Grid Subdivisions (x)",
            Kind::Int(32),
            1,
            prefs.grid_horizontal
        ),
        count!(
            "Video Preview Grid Subdivisions (y)",
            Kind::Int(32),
            1,
            prefs.grid_vertical
        ),
        choice!(
            "Video Preview Background",
            PreviewBackground::ALL,
            prefs.preview_background
        ),
        count!(
            "Thinning gap threshold (ms)",
            Kind::Int(32),
            0,
            prefs.thinning_gap_ms
        )
        .internal(),
        Pref::live(
            "type of hardware decoder choice",
            Kind::Int(32),
            |_, s| {
                Value::Int(match s.decoding {
                    bv_engine::DecodeChoice::Off => 0,
                    bv_engine::DecodeChoice::Auto => 1,
                    bv_engine::DecodeChoice::Card(_) => 2,
                })
            },
            |c, s, v| match v.whole() {
                Some(0) => {
                    s.decoding = bv_engine::DecodeChoice::Off;
                    true
                }
                Some(1) => {
                    s.decoding = bv_engine::DecodeChoice::Auto;
                    true
                }
                Some(2) => match (&s.decoding, c.decoders.first()) {
                    (bv_engine::DecodeChoice::Card(_), _) => true,
                    (_, Some(d)) => {
                        s.decoding = bv_engine::DecodeChoice::Card(d.clone());
                        true
                    }
                    (_, None) => false,
                },
                _ => false,
            },
        ),
        choice!(
            "Create Video Proxy Files",
            AutoProxies::ALL,
            prefs.auto_proxies
        ),
        choice!("Language", crate::i18n::Language::ALL, language),
        Pref::live(
            "Sync Offset (frames)",
            Kind::Int(32),
            |_, s| Value::Int(s.prefs.preview_sync_offset as i64),
            |_, s, v| {
                v.whole()
                    .and_then(|n| i32::try_from(n).ok())
                    .map(|n| s.prefs.preview_sync_offset = n)
                    .is_some()
            },
        ),
        choice!("MTC Input Frame Rate", MtcRate::ALL, prefs.mtc_input_rate),
        choice!("MTC Output Frame Rate", MtcRate::ALL, prefs.mtc_output_rate),
        choice!(
            "MTC Output Full-Frame Messages",
            LocateMessages::ALL,
            prefs.mtc_full_frames
        ),
        choice!(
            "MIDI Clock Song Position Pointers",
            LocateMessages::ALL,
            prefs.clock_song_position
        ),
        count!(
            "MTC Input Free-Wheel Slack (frames)",
            Kind::Int(32),
            0,
            prefs.mtc_slack_frames
        ),
        // Decimals.
        Pref::live(
            "Track Motion Default Smoothness",
            Kind::Float,
            |_, s| Value::Float(s.prefs.track_motion_smoothness),
            |_, s, v| {
                v.real()
                    .map(|x| s.prefs.track_motion_smoothness = x as f32)
                    .is_some()
            },
        )
        .range(0.0, 100.0),
        Pref::live(
            "Pan/Crop Default Smoothness",
            Kind::Float,
            |_, s| Value::Float(s.prefs.pan_crop_smoothness),
            |_, s, v| {
                v.real()
                    .map(|x| s.prefs.pan_crop_smoothness = x as f32)
                    .is_some()
            },
        )
        .range(0.0, 100.0),
        Pref::live(
            "Normalize To (0.25 to 1.0)",
            Kind::Float,
            |_, s| Value::Float(10f32.powf(s.prefs.normalize_peak_db / 20.0)),
            |_, s, v| {
                v.real()
                    .map(|x| s.prefs.normalize_peak_db = 20.0 * (x as f32).log10())
                    .is_some()
            },
        )
        .range(0.25, 1.0),
        fade_channel("DefVidFadeTopRed", true, 0),
        fade_channel("DefVidFadeTopGreen", true, 1),
        fade_channel("DefVidFadeTopBlue", true, 2),
        fade_channel("DefVidFadeTopAlpha", true, 3),
        fade_channel("DefVidFadeBottomRed", false, 0),
        fade_channel("DefVidFadeBottomGreen", false, 1),
        fade_channel("DefVidFadeBottomBlue", false, 2),
        fade_channel("DefVidFadeBottomAlpha", false, 3),
        Pref::live(
            "MTC Input Sync Delay (seconds)",
            Kind::Float,
            |_, s| Value::Float(s.prefs.mtc_sync_delay),
            |_, s, v| {
                v.real()
                    .filter(|x| *x >= 0.0)
                    .map(|x| s.prefs.mtc_sync_delay = x as f32)
                    .is_some()
            },
        ),
        // Texts.
        path!("Temporary Folder Path", prefs.temp_folder),
        name!("MTC Input Device Name", prefs.mtc_input),
        name!("MTC Output Device Name", prefs.mtc_output),
        name!("MCLK Output Device Name", prefs.clock_output),
        name!("Default Master Port Label", prefs.playback_device),
        path!("Sound Editor", prefs.audio_editor),
        path!("Tick sound file", prefs.metronome_normal),
        path!("Accent tick sound file", prefs.metronome_accent),
        name!("Preview Device Display", prefs.preview_display),
        Pref::live(
            "GPU acceleration of video processing",
            Kind::Text,
            |_, s| {
                Value::Text(match &s.video_gpu {
                    bv_engine::GpuChoice::Off => "Off".into(),
                    bv_engine::GpuChoice::Auto => "Auto".into(),
                    bv_engine::GpuChoice::Named(n) => n.clone(),
                })
            },
            |_, s, v| {
                v.text()
                    .filter(|t| !t.is_empty())
                    .map(|t| {
                        s.video_gpu = match t {
                            "Off" => bv_engine::GpuChoice::Off,
                            "Auto" => bv_engine::GpuChoice::Auto,
                            n => bv_engine::GpuChoice::Named(n.to_string()),
                        }
                    })
                    .is_some()
            },
        ),
        // Colors.
        Pref::live(
            "SelectedButtonBackgroundColor",
            Kind::UInt,
            |_, s| Value::UInt(colorref(s.prefs.button_highlight)),
            |_, s, v| set_colorref(&mut s.prefs.button_highlight, v),
        ),
        Pref::live(
            "Multicam video color",
            Kind::UInt,
            |_, s| Value::UInt(colorref(s.prefs.take_indicator)),
            |_, s, v| set_colorref(&mut s.prefs.take_indicator, v),
        ),
        // Lists.
        list("Default snap colors"),
        list("Default envelope colors"),
        list("UI Track colors"),
        list("MIDI Output Ports"),
        list("MIDI Input Ports"),
        list("MIDI Thru Routes"),
        list("VST Search Folders"),
        list("VST Plug-ins Left Out"),
        list("Control Devices"),
        // Switches.
        flag!("Show Dock Tabs Above Panes", prefs.tabs_at_top),
        flag!("Allow floating windows to dock", prefs.floating_dock),
        flag!("Auto Hide Docking Area", prefs.auto_hide_dock),
        flag!("Recompress Edited Frames", prefs.preview_recompress),
        flag!("fMultiDevicePreviewPlayback", prefs.preview_in_window),
        flag!("fUseExtMonRotation", prefs.preview_rotation),
        flag!("Loop events by default", prefs.loop_events),
        flag!("Evt Stretch defaults Preserve Pitch", prefs.preserve_pitch),
        flag!("Cut, copy, and delete grouped events", prefs.group_edits),
        flag!("Media Dependent Snap to Frames", prefs.no_quantize_audio),
        flag!("Time Sel Envelope Editing Enable", prefs.envelope_fades),
        flag!(
            "Automatically overlap media on insertion",
            prefs.overlap_added_media
        ),
        flag!("DefVideoStillCropToProject", prefs.crop_stills),
        flag!("Auto Open Last Session", prefs.open_last_project),
        flag!("fConfirmDeleteWhenMediaUsed", prefs.confirm_media_removal),
        flag!(
            "Save Active Previews On Project Close",
            prefs.keep_prerenders
        ),
        flag!("Close Files On Loss of Focus", prefs.release_media_inactive),
        flag!("Close Ports on Focus Loss", prefs.release_audio_inactive),
        flag!("Enable AutoSave", prefs.autosave),
        flag!(
            "Prompt to match settings to first video on timeline",
            prefs.match_first_media
        ),
        flag!(
            "Automatically re-associate project types with App",
            prefs.check_associations
        ),
        flag!("Enable joystick support", prefs.joystick),
        flag!("Enable auto-detection of 360 media", prefs.detect_360),
        flag!(
            "Allow inverse telecine of AVI/DV",
            prefs.dv_pulldown_removal
        ),
        flag!(
            "AAF Export - Use frame unit for audio",
            prefs.aaf_frame_units
        ),
        flag!(
            "AAF Export - Use clip-based audio envelope",
            prefs.aaf_clip_envelopes
        ),
        flag!("Import Stereo As Dual Mono", prefs.stereo_as_dual_mono),
        flag!(
            "Enable no - recompress long - GOP rendering",
            prefs.no_recompress
        ),
        flag!(
            "Use GPU processing for ACES color management",
            prefs.aces_on_gpu
        ),
        flag!("Show Post Record Dialog", prefs.ask_keep_recordings),
        flag!("Create undos for fx parameter changes", prefs.fx_undo),
        flag!(
            "Keep bypassed FX running (to avoid pause on bypass/enable)",
            prefs.keep_bypassed_fx
        ),
        flag!(
            "Automatically name regions and markers if not playing",
            prefs.name_markers
        ),
        flag!("Linear Scrub", prefs.linear_scrub),
        flag!(
            "Allow Ctrl+drag scrub over events",
            prefs.ctrl_scrub_over_events
        ),
        flag!(
            "Make spacebar and F12 Play/Stop instead of Play/Pause",
            prefs.space_play_stop
        ),
        flag!("Always Draw Marker Lines", prefs.marker_lines),
        flag!("Allow Drags on Cursor", prefs.cursor_drag),
        flag!("fDoubleClickLoadsTrimmer", prefs.media_to_trimmer),
        flag!(
            "Show trimmer history with filename first, then folder",
            prefs.trimmer_name_first
        ),
        flag!("fTrimmerAutosaveMarkers", prefs.save_trimmer_markers),
        flag!(
            "Show warning when deleting a track",
            prefs.warn_track_delete
        ),
        flag!(
            "Show warning when creating a nested timeline",
            prefs.warn_nesting
        ),
        flag!(
            "Show warning when opening a parent timeline",
            prefs.warn_nested_save
        ),
        flag!(
            "Automatically thin recorded automation",
            prefs.thin_automation
        ),
        flag!("Show Welcome Screen when program starts", show_welcome),
        flag!(
            "Pre-Fader sends listen to mute",
            prefs.prefader_sends_listen_to_mute
        ),
        flag!("Enable Recent Files Menu", prefs.recent_list),
        flag!(
            "Reset Automation levels when turned off",
            prefs.automation_off_defaults
        ),
        flag!("MTC Input FreeWheel", prefs.mtc_freewheel),
        flag!("Enable Record Latency Offset", prefs.record_latency_auto),
        flag!("Record files as broadcast wav", prefs.record_bwf),
        flag!(
            "Prompt user when no track has been armed for record",
            prefs.ask_no_armed_track
        ),
        Pref::live(
            "Use default ticks",
            Kind::Bool,
            |_, s| Value::Bool(!s.prefs.metronome_custom),
            |_, s, v| v.flag().map(|b| s.prefs.metronome_custom = !b).is_some(),
        ),
        flag!("Loudness Log Launch Editor", prefs.open_loudness_log),
        Pref::live(
            "Draw Video Frame Nums",
            Kind::Bool,
            |_, s| Value::Bool(s.prefs.frame_numbers != FrameNumbers::None),
            |_, s, v| {
                v.flag()
                    .map(|on| match (on, s.prefs.frame_numbers) {
                        (false, _) => s.prefs.frame_numbers = FrameNumbers::None,
                        (true, FrameNumbers::None) => {
                            s.prefs.frame_numbers = FrameNumbers::Timecode
                        }
                        (true, _) => {}
                    })
                    .is_some()
            },
        ),
        flag!("Multicam show take names", prefs.take_names),
        flag!("Multicam show take numbers", prefs.take_numbers),
        flag!("fVideoPreviewScaleToFit", prefs.preview_scale_to_fit),
        flag!("Enable Envelope Painting", prefs.envelope_painting).internal(),
        flag!(
            "Double-click event edge to toggle Expanded Edit Mode",
            prefs.edge_expanded_edit
        ),
        flag!(
            "Automatically resize the plug-in window when switching to a new plug-in",
            prefs.fx_window_resize
        ),
        flag!("DefVideoFileCropToProject", prefs.crop_video),
        flag!(
            "Display parts of the timeline in different colors if AutoRipple is on",
            prefs.ripple_colors
        ),
        flag!(
            "Show memory, disc and CPU statistics in the status bar",
            prefs.system_stats
        ),
        flag!(
            "Enable right mouse selection mode",
            prefs.right_button_selection
        ),
        flag!(
            "Reposition timeline cursor when clicking on an event",
            prefs.click_moves_cursor
        ),
        flag!(
            "Display timeline at bottom of main window",
            prefs.timeline_at_bottom
        ),
        flag!("Reverse Event Background color", prefs.reverse_event_colors),
        flag!("Use High DPI scaling (where available)", prefs.high_dpi),
        flag!("Display at project size", prefs.display_at_project_size),
        flag!("Simulate device aspect ratio", prefs.simulate_device_aspect),
        flag!("Apply deinterlace filter", prefs.preview_deinterlace),
        flag!(
            "Adjust levels from studio RGB to computer RGB",
            prefs.preview_studio_rgb
        ),
        flag!(
            "Optimize GPU display performance",
            prefs.preview_optimize_gpu
        ),
        flag!("Enable track buffering", prefs.track_buffering),
        flag!("Generic VST editor", prefs.vst_generic_editor),
        flag!(
            "Do not create .bvpk files to store peak data",
            prefs.no_peak_files
        ),
        flag!("Hide new .bvpk files", prefs.hide_peak_files),
        flag!("MCLK Output Send Start", prefs.clock_send_start),
        flag!("External Control", prefs.external_control),
        // Unsigned whole numbers.
        count!("cmbMaxVideoRamBuffer", Kind::UInt, 0, prefs.ram_preview_mb),
        count!(
            "Maximum Recent Files in Menu",
            Kind::UInt,
            1,
            prefs.recent_count
        )
        .range(1.0, 9.0),
        // Doubles.
        Pref::live(
            "Video Preview Title Safe",
            Kind::Double,
            |_, s| Value::Double(s.prefs.title_safe as f64 / 100.0),
            |_, s, v| {
                v.real()
                    .filter(|x| (0.0..=1.0).contains(x))
                    .map(|x| s.prefs.title_safe = (x * 100.0).round() as u32)
                    .is_some()
            },
        ),
        Pref::live(
            "Video Preview Action Safe",
            Kind::Double,
            |_, s| Value::Double(s.prefs.action_safe as f64 / 100.0),
            |_, s, v| {
                v.real()
                    .filter(|x| (0.0..=1.0).contains(x))
                    .map(|x| s.prefs.action_safe = (x * 100.0).round() as u32)
                    .is_some()
            },
        ),
        // Long whole numbers.
        Pref::live(
            "CD Insert Pause Time",
            Kind::Int(32),
            |_, s| Value::Int((s.prefs.cd_track_gap * 75.0).round() as i64),
            |_, s, v| {
                v.whole()
                    .filter(|n| *n >= 0)
                    .map(|n| s.prefs.cd_track_gap = n as f64 / 75.0)
                    .is_some()
            },
        ),
        Pref::live(
            "MTC Input QuarterFrame adjust",
            Kind::Int(32),
            |_, s| Value::Int(s.prefs.mtc_offset_quarters as i64),
            |_, s, v| {
                v.whole()
                    .and_then(|n| i32::try_from(n).ok())
                    .map(|n| s.prefs.mtc_offset_quarters = n)
                    .is_some()
            },
        ),
        Pref::live(
            "Video Frame Num Format",
            Kind::Int(32),
            |_, s| {
                Value::Int(match s.prefs.frame_numbers {
                    FrameNumbers::Frames => 0,
                    FrameNumbers::Time => 1,
                    FrameNumbers::Timecode | FrameNumbers::None => 2,
                })
            },
            |_, s, v| match (v.whole(), s.prefs.frame_numbers) {
                // Off, the format waits for the numbers to be drawn.
                (Some(0..=2), FrameNumbers::None) => true,
                (Some(n @ 0..=2), _) => {
                    s.prefs.frame_numbers = [
                        FrameNumbers::Frames,
                        FrameNumbers::Time,
                        FrameNumbers::Timecode,
                    ][n as usize];
                    true
                }
                _ => false,
            },
        )
        .range(0.0, 2.0),
        choice!("Video Thumbnails Shown", Thumbnails::ALL, prefs.thumbnails).range(0.0, 4.0),
        // Times, in tenths of a microsecond.
        Pref::live(
            "Quickfade size",
            Kind::Int(64),
            |_, s| Value::Int(s.prefs.quick_fade_ms as i64 * 10_000),
            |_, s, v| {
                v.whole()
                    .map(|n| s.prefs.quick_fade_ms = (n as f64 / 10_000.0).round() as u32)
                    .is_some()
            },
        )
        .range(0.0, 20_000_000.0),
        seconds!("ntDefVideoStillLength", f64, prefs.still_length).range(0.0, 600_000_000.0),
        seconds!("Default Transition Size", f64, prefs.cut_overlap).range(0.0, 600_000_000.0),
        Pref::live(
            "Time Sel Envelope Editing Size",
            Kind::Int(64),
            |_, s| Value::Int(s.prefs.envelope_fade_ms as i64 * 10_000),
            |_, s, v| {
                v.whole()
                    .map(|n| s.prefs.envelope_fade_ms = (n as f64 / 10_000.0).round() as u32)
                    .is_some()
            },
        )
        .range(0.0, 600_000_000.0),
        seconds!(
            "Pre and postroll for preview selection",
            f64,
            prefs.cursor_preview
        )
        .range(1_000_000.0, 200_000_000.0),
        Pref::live(
            "ntDefSmartSplitLength",
            Kind::Int(64),
            |_, s| Value::Int(s.prefs.smart_split_frames as i64),
            |_, s, v| {
                v.whole()
                    .map(|n| s.prefs.smart_split_frames = n as u32)
                    .is_some()
            },
        )
        .range(0.0, 60_000.0),
        seconds!("Track Render Buffering/Latency", f32, prefs.track_buffer),
        seconds!("MTC Input FreeWheel Time", f32, prefs.mtc_freewheel_seconds),
        seconds!("Mixer Circbuf Preroll/Latency", f32, prefs.playback_buffer)
            .range(100_000.0, 10_000_000.0),
        Pref::live(
            "User Record Latency Offset (nanos)",
            Kind::Int(64),
            |_, s| Value::Int((s.prefs.record_latency_ms as f64 * 10_000.0).round() as i64),
            |_, s, v| {
                v.whole()
                    .map(|n| s.prefs.record_latency_ms = (n as f64 / 10_000.0) as f32)
                    .is_some()
            },
        ),
        // Bytes.
        Pref::live(
            "Icon color strength",
            Kind::Int(32),
            |_, s| Value::Int(s.prefs.icon_strength as i64),
            |_, s, v| {
                v.whole()
                    .and_then(|n| u8::try_from(n).ok())
                    .map(|n| s.prefs.icon_strength = n)
                    .is_some()
            },
        )
        .range(0.0, 100.0),
    ]
}

/// One of the default fade to color envelope colors' channels (`top`: the
/// top color's), 0 to 1.
fn fade_channel(name: &'static str, top: bool, channel: usize) -> Pref {
    let get: Get = if top {
        match channel {
            0 => |_, s| Value::Float(s.prefs.fade_top[0] as f32 / 255.0),
            1 => |_, s| Value::Float(s.prefs.fade_top[1] as f32 / 255.0),
            2 => |_, s| Value::Float(s.prefs.fade_top[2] as f32 / 255.0),
            _ => |_, s| Value::Float(s.prefs.fade_top[3] as f32 / 255.0),
        }
    } else {
        match channel {
            0 => |_, s| Value::Float(s.prefs.fade_bottom[0] as f32 / 255.0),
            1 => |_, s| Value::Float(s.prefs.fade_bottom[1] as f32 / 255.0),
            2 => |_, s| Value::Float(s.prefs.fade_bottom[2] as f32 / 255.0),
            _ => |_, s| Value::Float(s.prefs.fade_bottom[3] as f32 / 255.0),
        }
    };
    let set: Set = if top {
        match channel {
            0 => |_, s, v| channel_to(&mut s.prefs.fade_top[0], v),
            1 => |_, s, v| channel_to(&mut s.prefs.fade_top[1], v),
            2 => |_, s, v| channel_to(&mut s.prefs.fade_top[2], v),
            _ => |_, s, v| channel_to(&mut s.prefs.fade_top[3], v),
        }
    } else {
        match channel {
            0 => |_, s, v| channel_to(&mut s.prefs.fade_bottom[0], v),
            1 => |_, s, v| channel_to(&mut s.prefs.fade_bottom[1], v),
            2 => |_, s, v| channel_to(&mut s.prefs.fade_bottom[2], v),
            _ => |_, s, v| channel_to(&mut s.prefs.fade_bottom[3], v),
        }
    };
    Pref::live(name, Kind::Float, get, set)
}

fn channel_to(c: &mut u8, v: &Value) -> bool {
    match v.real() {
        Some(x) if (0.0..=1.0).contains(&x) => {
            *c = (x * 255.0).round() as u8;
            true
        }
        _ => false,
    }
}

/// The settings BananaV starts with, for the Default column.
fn defaults() -> UiSettings {
    let mut s = UiSettings::default();
    // The welcome screen shows the first time.
    s.show_welcome = true;
    s
}

/// A column of the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Column {
    Name,
    Value,
    Default,
}

/// A line as the window holds it.
#[derive(Clone, Debug)]
struct Row {
    pref: usize,
    value: String,
    /// The value it opened with: only lines changed are read back.
    opened: String,
    default: String,
}

/// The window.
#[derive(Debug)]
pub(crate) struct InternalDialog {
    prefs: Vec<Pref>,
    ctx: Ctx,
    rows: Vec<Row>,
    search: String,
    /// The search box was changed: only internal preferences show.
    searched: bool,
    sort: Option<(Column, bool)>,
    selected: Option<usize>,
    /// The line whose value is being typed, and the text.
    editing: Option<(usize, String)>,
    applicable: bool,
}

/// The list's columns: Name, Value and Default, as VEGAS's.
const NAME_W: f32 = 254.0;
const VALUE_W: f32 = 100.0;
const DEFAULT_W: f32 = 100.0;
const ROW_H: f32 = 17.0;
const LIST_H: f32 = 528.0;

impl InternalDialog {
    pub(crate) fn new(app: &BananaApp) -> InternalDialog {
        let ctx = Ctx {
            decoders: bv_engine::Engine::decode_adapters(),
        };
        let prefs = all();
        let fresh = defaults();
        let rows = prefs
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let value = p.shown(&ctx, &app.settings);
                Row {
                    pref: i,
                    opened: value.clone(),
                    value,
                    default: p.shown(&ctx, &fresh),
                }
            })
            .collect();
        InternalDialog {
            prefs,
            ctx,
            rows,
            search: String::new(),
            searched: false,
            sort: None,
            selected: None,
            editing: None,
            applicable: false,
        }
    }

    /// Whether line `r` shows: every one before the search box was used;
    /// then the internal ones whose name holds its text.
    fn visible(&self, r: &Row) -> bool {
        if !self.searched {
            return true;
        }
        let p = &self.prefs[r.pref];
        p.internal
            && (self.search.is_empty()
                || p.name.to_uppercase().contains(&self.search.to_uppercase()))
    }

    /// The values changed read back into the app's settings, as OK and
    /// Apply do.
    fn apply(&self, app: &mut BananaApp) {
        let mut s = app.settings.clone();
        for r in self.rows.iter().filter(|r| r.value != r.opened) {
            self.prefs[r.pref].take(&self.ctx, &mut s, &r.value);
        }
        app.settings.preview.quality = s.preview.quality;
        app.settings.new_project = s.new_project.clone();
        crate::preferences::Draft {
            show_welcome: s.show_welcome,
            language: s.language,
            gpu: s.video_gpu.clone(),
            decoding: s.decoding.clone(),
            prefs: s.prefs.clone(),
        }
        .apply(app);
    }

    /// Ends the typing of a value: a switch's text becomes TRUE or FALSE by
    /// its first letter (or stays as it was), as VEGAS's.
    fn end_edit(&mut self, keep: bool) {
        let Some((i, text)) = self.editing.take() else {
            return;
        };
        if !keep {
            return;
        }
        let row = &mut self.rows[i];
        let text = if row.value == "TRUE" || row.value == "FALSE" {
            match text.chars().next() {
                Some('T' | 't') => "TRUE".to_string(),
                Some('F' | 'f') => "FALSE".to_string(),
                _ => row.value.clone(),
            }
        } else {
            text
        };
        if text != row.value {
            row.value = text;
            self.applicable = true;
        }
    }

    fn sort_by(&mut self, column: Column) {
        let ascending = !matches!(self.sort, Some((c, true)) if c == column);
        self.sort = Some((column, ascending));
        let prefs = &self.prefs;
        let key = |r: &Row| -> String {
            match column {
                Column::Name => prefs[r.pref].name.to_string(),
                Column::Value => r.value.clone(),
                Column::Default => r.default.clone(),
            }
        };
        self.rows.sort_by(|a, b| {
            let (ka, kb) = (key(a), key(b));
            let o = ka
                .to_lowercase()
                .cmp(&kb.to_lowercase())
                .then_with(|| ka.cmp(&kb));
            if ascending { o } else { o.reverse() }
        });
        self.selected = None;
    }

    /// Draws the window; `false` once it is closed.
    pub(crate) fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        let mut open = true;
        let mut close = false;
        crate::floating::Window::new(tr("Preferences"))
            .id(egui::Id::new("internal-preferences"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(tr("Preferences:"));
                self.list(ui);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(tr("Show only prefs containing:"));
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .desired_width(ui.available_width()),
                    );
                    if resp.changed() {
                        self.searched = true;
                        self.selected = None;
                    }
                });
                ui.add_space(6.0);
                let width = NAME_W + VALUE_W + DEFAULT_W + 14.0;
                let layout = egui::Layout::right_to_left(egui::Align::Center);
                ui.allocate_ui_with_layout(
                    vec2(width, ui.spacing().interact_size.y),
                    layout,
                    |ui| {
                        if ui
                            .add_enabled(self.applicable, egui::Button::new(tr("Apply")))
                            .clicked()
                        {
                            self.end_edit(true);
                            self.apply(app);
                            self.applicable = false;
                        }
                        if ui.button(tr("Cancel")).clicked() {
                            close = true;
                        }
                        if ui.button(egui::RichText::new("OK").strong()).clicked() {
                            self.end_edit(true);
                            self.apply(app);
                            close = true;
                        }
                    },
                );
            });
        open && !close
    }

    /// The list: its column heads (a click sorts by that column, again the
    /// other way) and its lines; a double-click on a value (or F2 on the
    /// line picked) types it.
    fn list(&mut self, ui: &mut Ui) {
        let font = FontId::proportional(12.0);
        let widths = [NAME_W, VALUE_W, DEFAULT_W];
        let heads = [
            (Column::Name, trc("internal-prefs", "Name")),
            (Column::Value, trc("internal-prefs", "Value")),
            (Column::Default, trc("internal-prefs", "Default")),
        ];
        let (head, _) = ui.allocate_exact_size(
            vec2(widths.iter().sum::<f32>() + 14.0, 20.0),
            Sense::hover(),
        );
        let mut x = head.left();
        let mut sort = None;
        for ((column, text), w) in heads.into_iter().zip(widths) {
            let r = Rect::from_min_size(pos2(x, head.top()), vec2(w, head.height()));
            let resp = ui.interact(r, ui.id().with(("internal-head", text)), Sense::click());
            let painter = ui.painter();
            painter.rect_filled(
                r,
                0.0,
                if resp.hovered() {
                    theme::hover()
                } else {
                    theme::panel()
                },
            );
            painter.text(
                pos2(r.left() + 4.0, r.center().y),
                Align2::LEFT_CENTER,
                text,
                font.clone(),
                theme::text(),
            );
            if let Some((c, up)) = self.sort
                && c == column
            {
                let cx = r.right() - 10.0;
                let cy = r.center().y;
                let tri = if up {
                    vec![
                        pos2(cx - 4.0, cy + 2.0),
                        pos2(cx + 4.0, cy + 2.0),
                        pos2(cx, cy - 3.0),
                    ]
                } else {
                    vec![
                        pos2(cx - 4.0, cy - 2.0),
                        pos2(cx + 4.0, cy - 2.0),
                        pos2(cx, cy + 3.0),
                    ]
                };
                painter.add(egui::Shape::convex_polygon(
                    tri,
                    theme::text_dim(),
                    Stroke::NONE,
                ));
            }
            if resp.clicked() {
                sort = Some(column);
            }
            x += w;
        }
        if let Some(c) = sort {
            self.end_edit(true);
            self.sort_by(c);
        }
        let shown: Vec<usize> = (0..self.rows.len())
            .filter(|&i| self.visible(&self.rows[i]))
            .collect();
        let f2 = ui.input(|i| i.key_pressed(egui::Key::F2));
        if f2
            && self.editing.is_none()
            && let Some(i) = self.selected
            && self.live(i)
        {
            self.editing = Some((i, self.rows[i].value.clone()));
        }
        // The lines touch: the rows shown are worked out without spacing.
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::ScrollArea::vertical()
                .id_salt("internal-list")
                .max_height(LIST_H)
                .min_scrolled_height(LIST_H)
                .auto_shrink([false, false])
                .show_rows(ui, ROW_H, shown.len(), |ui, range| {
                    for &i in &shown[range] {
                        self.line(ui, i, &font);
                    }
                });
        });
    }

    /// Whether line `i`'s value can be typed (BananaV has the setting).
    fn live(&self, i: usize) -> bool {
        matches!(self.prefs[self.rows[i].pref].access, Access::Live { .. })
    }

    fn line(&mut self, ui: &mut Ui, i: usize, font: &FontId) {
        let (rect, resp) =
            ui.allocate_exact_size(vec2(NAME_W + VALUE_W + DEFAULT_W, ROW_H), Sense::click());
        if resp.clicked() || resp.double_clicked() {
            if self.editing.as_ref().is_some_and(|(e, _)| *e != i) {
                self.end_edit(true);
            }
            self.selected = Some(i);
        }
        let name_r = Rect::from_min_size(rect.min, vec2(NAME_W, ROW_H));
        let value_r =
            Rect::from_min_size(pos2(rect.left() + NAME_W, rect.top()), vec2(VALUE_W, ROW_H));
        let default_r =
            Rect::from_min_size(pos2(value_r.right(), rect.top()), vec2(DEFAULT_W, ROW_H));
        let live = self.live(i);
        let picked = self.selected == Some(i);
        let painter = ui.painter().clone();
        // Name and Default can be read only: dimmer, as VEGAS's.
        let fixed = theme::panel();
        let field = theme::panel_dark();
        let fixed_text = theme::text_dim();
        for (r, fill) in [(name_r, fixed), (value_r, field), (default_r, fixed)] {
            painter.rect_filled(r, 0.0, fill);
            painter.hline(
                r.x_range(),
                r.bottom() - 0.5,
                Stroke::new(1.0, theme::separator()),
            );
        }
        if picked {
            painter.rect_filled(rect, 0.0, theme::selection().gamma_multiply(0.45));
        }
        let row = &self.rows[i];
        let p = &self.prefs[row.pref];
        let cell = |r: Rect, text: &str, color: Color32| {
            crate::widgets::fit_text(
                &painter,
                pos2(r.left() + 3.0, r.center().y),
                Align2::LEFT_CENTER,
                text,
                font.clone(),
                color,
                r.width() - 5.0,
            );
        };
        cell(name_r, p.name, fixed_text);
        cell(default_r, &row.default, fixed_text);
        let editing = self.editing.as_ref().is_some_and(|(e, _)| *e == i);
        if editing {
            let id = ui.id().with(("internal-value", i));
            let mut keep = None;
            if let Some((_, text)) = &mut self.editing {
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(value_r));
                let edit = child.add(
                    egui::TextEdit::singleline(text)
                        .id(id)
                        .font(font.clone())
                        .frame(egui::Frame::NONE)
                        .desired_width(value_r.width() - 4.0)
                        .margin(vec2(3.0, 1.0)),
                );
                if !edit.has_focus() && !edit.lost_focus() {
                    edit.request_focus();
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    keep = Some(false);
                } else if edit.lost_focus() {
                    keep = Some(true);
                }
            }
            if let Some(k) = keep {
                self.end_edit(k);
            }
        } else {
            let color = if live {
                theme::text()
            } else {
                theme::text_disabled()
            };
            cell(value_r, &self.rows[i].value.clone(), color);
            let on_value = resp
                .interact_pointer_pos()
                .is_some_and(|p: Pos2| value_r.contains(p));
            if resp.double_clicked() && on_value && live {
                self.editing = Some((i, self.rows[i].value.clone()));
            }
            if !live {
                ui.interact(value_r, ui.id().with(("internal-soon", i)), Sense::hover())
                    .on_hover_text(tr("Coming soon to BananaV"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_show_as_vegas_shows_them() {
        assert_eq!(show(&Value::Bool(true)), "TRUE");
        assert_eq!(show(&Value::Float(0.707_106_77)), "0.707107");
        assert_eq!(show(&Value::Double(29.97003)), "29.970030");
        assert_eq!(show(&Value::Int(-1)), "-1");
        assert_eq!(show(&Value::UInt(4_294_967_294)), "4294967294");
        assert_eq!(show(&Value::List), "");
        assert_eq!(show(&Value::Float(f32::INFINITY)), "inf");
    }

    #[test]
    fn ok_reads_values_as_vegas_reads_them() {
        let int = Kind::Int(32);
        assert_eq!(read(int, " 2xyz"), Some(Value::Int(2)));
        assert_eq!(read(int, "+7"), Some(Value::Int(7)));
        assert_eq!(read(int, "-12"), Some(Value::Int(-12)));
        assert_eq!(read(int, "abc"), None, "no digit");
        assert_eq!(read(int, "2147483648"), None, "past 32 bits");
        assert_eq!(
            read(Kind::Int(64), "2147483648"),
            Some(Value::Int(2_147_483_648))
        );
        assert_eq!(read(Kind::UInt, "-1"), Some(Value::UInt(4_294_967_295)));
        assert_eq!(read(Kind::UInt, "4294967296"), None);
        assert_eq!(read(Kind::Float, "0.5x"), Some(Value::Float(0.5)));
        assert_eq!(read(Kind::Float, ".25"), Some(Value::Float(0.25)));
        assert_eq!(read(Kind::Float, "1e3"), Some(Value::Float(1000.0)));
        assert_eq!(read(Kind::Float, "1e"), Some(Value::Float(1.0)));
        assert_eq!(read(Kind::Float, "1e39"), None, "past a float");
        assert_eq!(read(Kind::Double, "1e39"), Some(Value::Double(1e39)));
        assert_eq!(
            read(Kind::Float, "-inf"),
            Some(Value::Float(f32::NEG_INFINITY))
        );
        assert_eq!(read(Kind::Bool, "true"), Some(Value::Bool(true)));
        assert_eq!(read(Kind::Bool, "fish"), Some(Value::Bool(false)));
        assert_eq!(read(Kind::Bool, "yes"), None);
        assert_eq!(read(Kind::Text, " a "), Some(Value::Text(" a ".into())));
        assert_eq!(read(Kind::List, "x"), None);
    }

    #[test]
    fn every_name_is_once_and_every_value_reads_back() {
        let prefs = all();
        let mut names: Vec<&str> = prefs.iter().map(|p| p.name).collect();
        names.sort_unstable();
        let n = names.len();
        names.dedup();
        assert_eq!(names.len(), n, "a name twice");
        // What each shows reads back to the same settings.
        let ctx = Ctx::default();
        let s = defaults();
        for p in &prefs {
            let mut t = s.clone();
            let shown = p.shown(&ctx, &s);
            if matches!(p.access, Access::Live { .. }) && p.kind != Kind::List {
                assert!(
                    p.take(&ctx, &mut t, &shown),
                    "{} did not read {shown:?}",
                    p.name
                );
                assert_eq!(p.shown(&ctx, &t), shown, "{}", p.name);
            }
        }
    }

    #[test]
    fn the_print_lines_are_there_in_order() {
        // The first thirty-one lines of the window, as the user's print.
        let print = [
            "Def Quality Preview",
            "Def Quality Render",
            "S3D Mode Projects",
            "360 mode",
            "HDR mode",
            "Maximum Video Render Threads",
            "Minimum Video Render Threads",
            "Max For Maximum Video Render Threads",
            "Level of brightness for the interface",
            "S3D Mode External Monitor",
            "S3D Mode Video Preview",
            "JKL / shuttle speed",
            "Default Transition Alignment",
            "Time Sel Envelope Editing Alignment",
            "Dim Attenuation",
            "VU Meter Type",
            "Hardware Decoder selected if not AUTO",
            "FileIOLogging Load Level",
            "FileIOLogging Read Level",
            "FileIOLogging Render Level",
            "Default Audio Envelope Fade Type",
            "eDefEnvAudioASRType",
            "Default FX Parameter Automation Fade Type",
            "Default Video Envelope Fade Type",
            "eDefEnvVideoASRType",
            "eDefEnvVideoKeyframeType",
            "eDefEnvVideoMotionType",
            "Number of Control Plug-ins allowed",
            "MIDI Driver Type",
            "Mixer Driver Model",
            "Audio draw on record",
        ];
        let prefs = all();
        let names: Vec<&str> = prefs.iter().take(print.len()).map(|p| p.name).collect();
        assert_eq!(names, print);
        // Their defaults as the print's Default column, where BananaV keeps
        // VEGAS's.
        let ctx = Ctx::default();
        let s = defaults();
        let default_of = |name: &str| {
            prefs
                .iter()
                .find(|p| p.name == name)
                .map(|p| p.shown(&ctx, &s))
                .unwrap()
        };
        for (name, want) in [
            ("Def Quality Preview", "2"),
            ("Def Quality Render", "3"),
            ("Minimum Video Render Threads", "1"),
            ("Max For Maximum Video Render Threads", "48"),
            ("Level of brightness for the interface", "0"),
            ("JKL / shuttle speed", "1"),
            ("Default Transition Alignment", "0"),
            ("Time Sel Envelope Editing Alignment", "1"),
            ("Dim Attenuation", "20"),
            ("Hardware Decoder selected if not AUTO", "-1"),
            ("FileIOLogging Read Level", "0"),
            ("FileIOLogging Render Level", "0"),
            ("Default Audio Envelope Fade Type", "4"),
            ("eDefEnvAudioASRType", "2"),
            ("Default FX Parameter Automation Fade Type", "1"),
            ("Default Video Envelope Fade Type", "4"),
            ("eDefEnvVideoASRType", "4"),
            ("eDefEnvVideoKeyframeType", "0"),
            ("eDefEnvVideoMotionType", "0"),
            ("Number of Control Plug-ins allowed", "6"),
            ("Mixer Driver Model", "0"),
            ("Audio draw on record", "2"),
            ("Thinning gap threshold (ms)", "250"),
            ("Enable Envelope Painting", "TRUE"),
            ("Automatically thin recorded automation", "TRUE"),
        ] {
            assert_eq!(default_of(name), want, "{name}");
        }
    }

    #[test]
    fn out_of_range_and_unknown_values_are_left_alone() {
        let prefs = all();
        let ctx = Ctx::default();
        let find = |name: &str| prefs.iter().find(|p| p.name == name).unwrap();
        let mut s = defaults();
        // Def Quality Preview goes from 1 to 4.
        assert!(!find("Def Quality Preview").take(&ctx, &mut s, "5"));
        assert!(find("Def Quality Preview").take(&ctx, &mut s, "4"));
        assert_eq!(s.preview.quality.level, RenderQuality::Best);
        // A fade type VEGAS does not number.
        assert!(!find("Default Video Envelope Fade Type").take(&ctx, &mut s, "3"));
        assert!(find("Default Video Envelope Fade Type").take(&ctx, &mut s, "-4"));
        assert_eq!(s.prefs.new_video_envelopes, KeyCurve::Sharp);
        // Times in tenths of a microsecond.
        assert!(find("Quickfade size").take(&ctx, &mut s, "250000"));
        assert_eq!(s.prefs.quick_fade_ms, 25);
        // Greyed: nothing to take.
        assert!(!find("HDR mode").take(&ctx, &mut s, "1"));
        // Painting off.
        assert!(find("Enable Envelope Painting").take(&ctx, &mut s, "f"));
        assert!(!s.prefs.envelope_painting);
    }

    #[test]
    fn the_search_shows_every_line_until_used_then_internal_ones() {
        let app_settings = defaults();
        let ctx = Ctx::default();
        let prefs = all();
        let rows: Vec<Row> = prefs
            .iter()
            .enumerate()
            .map(|(i, p)| Row {
                pref: i,
                value: p.shown(&ctx, &app_settings),
                opened: String::new(),
                default: String::new(),
            })
            .collect();
        let mut d = InternalDialog {
            prefs,
            ctx,
            rows,
            search: String::new(),
            searched: false,
            sort: None,
            selected: None,
            editing: None,
            applicable: false,
        };
        let shown = |d: &InternalDialog| d.rows.iter().filter(|r| d.visible(r)).count();
        assert_eq!(shown(&d), d.rows.len(), "all before the search");
        d.searched = true;
        let internal = d.prefs.iter().filter(|p| p.internal).count();
        assert_eq!(shown(&d), internal, "the internal ones once it was used");
        d.search = "envelope".into();
        let names: Vec<&str> = d
            .rows
            .iter()
            .filter(|r| d.visible(r))
            .map(|r| d.prefs[r.pref].name)
            .collect();
        assert_eq!(names, ["Enable Envelope Painting"]);
        d.search = "THINNING".into();
        assert_eq!(shown(&d), 1, "without minding case");
    }
}
