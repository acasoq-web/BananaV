//! Keyboard map (Options › Customize Keyboard): every command that can have
//! shortcuts, in the contexts of VEGAS (TrackView, Global, …), with its
//! default keys, the user's saved maps and the dispatch of key presses.
//!
//! Punctuation keys are matched by their physical position, like VEGAS does,
//! so a shortcut stays on the same key in any layout; their label comes from
//! the user's layout (the US `[` key shows as `´` on a Brazilian keyboard).

use crate::i18n::{tr, trc};
use crate::tabs::details::DetailsOp;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::OnceLock;

use bv_timeline::{EnvelopeKind, RippleScope};
use serde::{Deserialize, Serialize};

use crate::app::actions::LayoutPreset;
use crate::app::more_actions::Script;
use crate::app::{BananaApp, Tool};
use crate::commands::Cmd;
use crate::dock::Tab;
use crate::egui::{self, Key, Modifiers};
use crate::tabs::console::{ChannelWidth, ConsoleOp, Region};
use crate::tabs::trimmer::{Streams, TrimmerOp};
use crate::timefmt::{GridSpacing, RulerFormat};

/// The tabs of the Customize Keyboard window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Context {
    TrackView,
    TrackList,
    Explorer,
    Trimmer,
    MixingDesk,
    EditDetails,
    TrackMotion,
    Global,
    MixingConsole,
}

impl Context {
    pub const ALL: [Context; 9] = [
        Context::TrackView,
        Context::TrackList,
        Context::Explorer,
        Context::Trimmer,
        Context::MixingDesk,
        Context::EditDetails,
        Context::TrackMotion,
        Context::Global,
        Context::MixingConsole,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Context::TrackView => "TrackView",
            Context::TrackList => "TrackList",
            Context::Explorer => "Explorer",
            Context::Trimmer => tr("Trimmer"),
            Context::MixingDesk => tr("Mixer"),
            Context::EditDetails => "EditDetails",
            Context::TrackMotion => "TrackMotion",
            Context::Global => "Global",
            Context::MixingConsole => tr("Mixing Console"),
        }
    }

    fn key(self) -> &'static str {
        match self {
            Context::TrackView => "TrackView",
            Context::TrackList => "TrackList",
            Context::Explorer => "Explorer",
            Context::Trimmer => "Trimmer",
            Context::MixingDesk => "MixingDesk",
            Context::EditDetails => "EditDetails",
            Context::TrackMotion => "TrackMotion",
            Context::Global => "Global",
            Context::MixingConsole => "MixingConsole",
        }
    }

    /// Contexts that have their command list. A key press is looked up in
    /// the focused one (TrackView or TrackList), then in Global.
    const LISTED: [Context; 9] = Context::ALL;
}

/// Keyboard-only actions that are not menu items.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Extra {
    /// F11 (`None`: every docking area) and its kin: the windows docked on
    /// one side of the timeline hidden, or shown again.
    DockArea(Option<crate::dock::Area>),
    /// Ctrl+F11: the track headers folded to their color bars, or back.
    FoldTrackHeaders,
    /// Ctrl+Alt+Shift+F11: the timeline maximized — the docking areas
    /// hidden and the track headers folded — or back.
    MaximizeTimeline,
    GoToMarker(u8),
    /// The cursor to the next or previous CD track (with `indices`: track
    /// or index marker).
    CdStep {
        indices: bool,
        dir: i8,
    },
    /// A command of Edit Details.
    Details(DetailsOp),
    SelectNone,
    OpenInTrimmer,
    SelectInMediaPool,
    /// Moves the selected events by one frame (or one pixel) left/right;
    /// trims the edge instead in edge-trimming and Expanded Edit Mode.
    Nudge {
        frames: bool,
        dir: i8,
    },
    /// F3/F9: one frame back or on (the edge, when trimming one).
    Jog(i8),
    Refresh,
    /// A command of the TrackMotion context (the Track Motion window's).
    TrackMotion(crate::track_motion::TrackMotionKey),
    TrackMute {
        exclusive: bool,
    },
    TrackSolo {
        exclusive: bool,
    },
    /// ±1 dB on the current track (±1 % of the level on video tracks).
    TrackVolume(i8),
    /// Pans the current audio track 2 % left (−1) or right (+1).
    TrackPan(i8),
    /// Numeric keypad pan: columns 1/4/7 left, 2/5/8 center, 3/6/9 right.
    TrackPanPosition(u8),
    /// Moves the track selection (the TrackList cursor).
    TrackCursor(TrackMove),
    /// Edits the current track's name in its header.
    RenameTrack,
    /// Saves the window layout to slot 0–9 (Alt+D, 0–9 loads it).
    SaveLayout(usize),
    /// A command of the Trimmer window.
    Trimmer(TrimmerOp),
    /// A view command of the Mixing Console.
    Console(ConsoleOp),
    /// Removes the selected track (Mixing Console: Delete).
    DeleteTrack,
    /// Opens Detect Scenes and Split for the selected event.
    DetectScenes,
    /// `: every track minimized, or back as they were (those minimized
    /// alone stay so); Ctrl+`: every track at the default height.
    ToggleMinimizeTracks,
    DefaultTrackHeights,
    Play,
    PlayToggle,
    Pause,
    Stop,
    PlayFromStart,
    StepForward,
    StepBack,
    ScrubRight,
    ScrubPause,
    /// J: backwards, faster at each press (the picture; the sound waits for
    /// reverse playback).
    ScrubLeft,
    PlayReverse,
    /// The cursor shuttled back or on fast.
    Rewind,
    FastForward,
    /// Ctrl+Shift+G and the selection's length: typed.
    TypeSelection(crate::menu_dialogs::GoToField),
    /// Tab: the keys between the track list and the timeline.
    SwitchFocus,
    /// Num 8/2: the selected events one track up (−1) or down (+1).
    MoveToTrack(i8),
    DuplicateTrack,
    /// Ctrl+Shift+Up/Down: every track taller (+1) or shorter (−1).
    AllTrackHeights(i8),
    /// Ctrl+Alt+Shift+Up/Down: every track one point taller or shorter.
    TrackHeightBy1(i8),
    /// The focused track as tall as the timeline.
    TrackHeightMax,
    /// The cursor to the other end of the time selection.
    SwapCursor,
    /// Num / * −: the cut at the cursor made a crossfade.
    CutToTransition,
    /// Ctrl+Num /: the crossfade at the cursor made a cut.
    TransitionToCut,
    /// The selected audio events' pitch, this many cents higher (`None`:
    /// back to none).
    Pitch(Option<i16>),
    /// The Video Preview's zoom: 5 % in (+1) or out (−1).
    PreviewZoom(i8),
    PreviewZoomDefault,
    PreviewBypassZoom,
}

/// Where [`Extra::TrackCursor`] moves the track selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackMove {
    Up,
    Down,
    PageUp,
    PageDown,
    First,
    Last,
    /// Track 1–9; 0 is track 10.
    Number(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Cmd(Cmd),
    Extra(Extra),
    /// A VEGAS command BananaV does not have yet: listed, but inactive.
    Soon,
}

/// Keys egui does not report separately: the numeric keypad's (told apart
/// by the keyboard's state, see [`keypad`]) and the media keys (listed like
/// in VEGAS, inactive).
const SPECIALS: &[&str] = &[
    "Num 0",
    "Num 1",
    "Num 2",
    "Num 3",
    "Num 4",
    "Num 5",
    "Num 6",
    "Num 7",
    "Num 8",
    "Num 9",
    "Num /",
    "Num *",
    "Num -",
    "Num +",
    "Play",
    "Stop",
    "Volume Mute",
    "Volume Down",
    "Volume Up",
    "Previous",
    "Next",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BindKey {
    Key(Key),
    Special(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Binding {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: BindKey,
    /// Second key of a two-key shortcut ("Alt+D, A"), pressed after the
    /// first one.
    pub then: Option<BindKey>,
}

/// Punctuation keys matched by physical position, with their US scan code.
const PHYSICAL: &[(Key, u32)] = &[
    (Key::Minus, 0x0C),
    (Key::Equals, 0x0D),
    (Key::OpenBracket, 0x1A),
    (Key::CloseBracket, 0x1B),
    (Key::Semicolon, 0x27),
    (Key::Quote, 0x28),
    (Key::Backtick, 0x29),
    (Key::Backslash, 0x2B),
    (Key::Comma, 0x33),
    (Key::Period, 0x34),
    (Key::Slash, 0x35),
];

fn key_from_name(name: &str) -> Option<Key> {
    Some(match name {
        "Left" => Key::ArrowLeft,
        "Right" => Key::ArrowRight,
        "Up" => Key::ArrowUp,
        "Down" => Key::ArrowDown,
        "[" => Key::OpenBracket,
        "]" => Key::CloseBracket,
        "\\" => Key::Backslash,
        "'" => Key::Quote,
        "`" => Key::Backtick,
        ";" => Key::Semicolon,
        "," => Key::Comma,
        "." => Key::Period,
        "/" => Key::Slash,
        "-" => Key::Minus,
        "=" => Key::Equals,
        other => return Key::from_name(other),
    })
}

/// VEGAS-style name of a key in the interface language; punctuation
/// follows the layout.
fn key_label(key: Key) -> String {
    if let Some(&(_, scan)) = PHYSICAL.iter().find(|(k, _)| *k == key)
        && let Some(c) = bv_platform::key_char(scan)
    {
        return c.to_uppercase().to_string();
    }
    match key {
        Key::ArrowLeft => trc("key", "Left").into(),
        Key::ArrowRight => trc("key", "Right").into(),
        Key::ArrowUp => trc("key", "Up").into(),
        Key::ArrowDown => trc("key", "Down").into(),
        Key::End => trc("key", "End").into(),
        Key::PageUp => "Page Up".into(),
        Key::PageDown => "Page Down".into(),
        Key::Backspace => trc("key", "Backspace").into(),
        Key::Delete => tr("Delete").into(),
        Key::Insert => tr("Insert").into(),
        Key::Space => tr("Space").into(),
        Key::Escape => "Esc".into(),
        other => other.symbol_or_name().to_string(),
    }
}

impl Binding {
    /// `"Ctrl+Alt+Shift+Left"`, `"Num 5"`, `"Shift+Play"`, `"Alt+D, A"`…
    pub fn parse(s: &str) -> Option<Binding> {
        if let Some((first, second)) = s.split_once(", ") {
            let mut b = Binding::parse(first)?;
            b.then = Some(parse_key(second.trim())?);
            return Some(b);
        }
        let mut b = Binding {
            ctrl: false,
            alt: false,
            shift: false,
            key: BindKey::Special(""),
            then: None,
        };
        let mut rest = s.trim();
        loop {
            if let Some(r) = rest.strip_prefix("Ctrl+") {
                b.ctrl = true;
                rest = r;
            } else if let Some(r) = rest.strip_prefix("Alt+") {
                b.alt = true;
                rest = r;
            } else if let Some(r) = rest.strip_prefix("Shift+") {
                b.shift = true;
                rest = r;
            } else {
                break;
            }
        }
        b.key = parse_key(rest)?;
        Some(b)
    }

    /// Text stored in the keyboard map file.
    pub fn storage(&self) -> String {
        let name = |k: BindKey| match k {
            BindKey::Key(k) => k.name().to_string(),
            BindKey::Special(s) => s.to_string(),
        };
        match self.then {
            Some(t) => format!("{}{}, {}", self.mods(), name(self.key), name(t)),
            None => format!("{}{}", self.mods(), name(self.key)),
        }
    }

    /// The first key of a two-key shortcut (itself for a single key).
    pub fn first(&self) -> Binding {
        Binding {
            then: None,
            ..*self
        }
    }

    fn mods(&self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s.push_str("Ctrl+");
        }
        if self.alt {
            s.push_str("Alt+");
        }
        if self.shift {
            s.push_str("Shift+");
        }
        s
    }

    /// What the user sees: `"Ctrl+Alt+Para a esquerda"`, `"Ctrl+´"`…
    pub fn label(&self) -> String {
        let name = |k: BindKey| match k {
            BindKey::Key(k) => key_label(k),
            BindKey::Special(s) => tr(s).to_string(),
        };
        match self.then {
            Some(t) => format!("{}{}, {}", self.mods(), name(self.key), name(t)),
            None => format!("{}{}", self.mods(), name(self.key)),
        }
    }

    /// The binding of a key press (physical position for punctuation).
    pub fn from_event(key: Key, physical: Option<Key>, m: Modifiers) -> Binding {
        let key = match physical {
            Some(p) if PHYSICAL.iter().any(|(k, _)| *k == p) => p,
            _ => key,
        };
        Binding {
            ctrl: m.ctrl || m.command,
            alt: m.alt,
            shift: m.shift,
            key: BindKey::Key(key),
            then: None,
        }
    }
}

/// The numeric keypad's keys: egui reports them as the digits and signs
/// they type, so a key the keypad holds down is taken as its own ("Num 1").
/// With Shift (or Num Lock off) a keypad digit types a navigation key:
/// with Shift it is still the keypad's, as VEGAS reads it.
fn keypad(b: Binding, physical: Option<Key>) -> Binding {
    let digit = |k: Key| -> Option<char> {
        let n = match k {
            Key::Num0 => 0,
            Key::Num1 => 1,
            Key::Num2 => 2,
            Key::Num3 => 3,
            Key::Num4 => 4,
            Key::Num5 => 5,
            Key::Num6 => 6,
            Key::Num7 => 7,
            Key::Num8 => 8,
            Key::Num9 => 9,
            _ => return None,
        };
        char::from_digit(n, 10)
    };
    let navigation = matches!(
        b.key,
        BindKey::Key(
            Key::Home
                | Key::End
                | Key::PageUp
                | Key::PageDown
                | Key::ArrowLeft
                | Key::ArrowRight
                | Key::ArrowUp
                | Key::ArrowDown
                | Key::Insert
                | Key::Delete
        )
    );
    let c = match b.key {
        BindKey::Key(k) if digit(k).is_some() => digit(k),
        BindKey::Key(Key::Plus) => Some('+'),
        BindKey::Key(Key::Minus) => Some('-'),
        BindKey::Key(Key::Slash) => Some('/'),
        // Shift+keypad digit: a navigation key, the digit's key physically.
        _ if b.shift && navigation => physical.and_then(digit),
        _ => None,
    };
    let Some(c) = c else {
        return b;
    };
    // Shift+keypad: Windows lets the digit key up for the navigation key;
    // the physical key says which it was.
    if !(bv_platform::numpad_down(c) || b.shift && navigation) {
        return b;
    }
    let name = match c {
        '0' => "Num 0",
        '1' => "Num 1",
        '2' => "Num 2",
        '3' => "Num 3",
        '4' => "Num 4",
        '5' => "Num 5",
        '6' => "Num 6",
        '7' => "Num 7",
        '8' => "Num 8",
        '9' => "Num 9",
        '+' => "Num +",
        '-' => "Num -",
        '/' => "Num /",
        _ => return b,
    };
    Binding {
        key: BindKey::Special(name),
        ..b
    }
}

/// Whether Num * was down last frame (egui reports no event for it: it is
/// read from the keyboard).
static KEYPAD_STAR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The keys an input event presses, the likeliest first. egui's window
/// layer turns Ctrl+X, Ctrl+C and Ctrl+V (with any other modifiers, and on
/// Windows Shift+Delete, Ctrl+Insert and Shift+Insert) into clipboard events
/// without their key: the modifiers held say which keys they were. Cut and
/// Copy are a label's when it has selected text. Ctrl+V becomes nothing at
/// all when the clipboard holds no text (a picture, files): then only its
/// release tells that it was pressed, `v_down` saying whether the press
/// was seen.
fn pressed_keys(
    event: &egui::Event,
    held: Modifiers,
    label_text: bool,
    v_down: &mut bool,
) -> Vec<Binding> {
    let with = |key: Key, m: Modifiers| Binding::from_event(key, None, m);
    let mut keys = Vec::new();
    match event {
        egui::Event::Key {
            key,
            physical_key,
            pressed: true,
            modifiers,
            ..
        } => {
            if *key == Key::V {
                *v_down = true;
            }
            keys.push(keypad(
                Binding::from_event(*key, *physical_key, *modifiers),
                *physical_key,
            ));
        }
        egui::Event::Key {
            key: Key::V,
            pressed: false,
            modifiers,
            ..
        } => {
            // Released unseen: it was Ctrl+V with no text to paste.
            if !std::mem::take(v_down) {
                keys.push(with(Key::V, Modifiers::COMMAND | *modifiers));
            }
        }
        egui::Event::Cut | egui::Event::Copy if label_text => {}
        egui::Event::Cut => {
            if held.command {
                keys.push(with(Key::X, held));
            }
            if held.shift {
                keys.push(with(Key::Delete, held));
            }
        }
        egui::Event::Copy => {
            if held.command {
                keys.push(with(Key::C, held));
            }
            if held.ctrl {
                keys.push(with(Key::Insert, held));
            }
        }
        egui::Event::Paste(_) => {
            if held.command {
                *v_down = true;
                keys.push(with(Key::V, held));
            } else if held.shift {
                keys.push(with(Key::Insert, held));
            }
        }
        _ => {}
    }
    keys
}

/// A key name of the keyboard map file: a special key or an egui key.
fn parse_key(name: &str) -> Option<BindKey> {
    match SPECIALS.iter().find(|s| **s == name) {
        Some(s) => Some(BindKey::Special(s)),
        None => Some(BindKey::Key(key_from_name(name)?)),
    }
}

/// One command of the keyboard map.
#[derive(Clone, Debug)]
pub struct KeyCommand {
    pub id: String,
    pub context: Context,
    pub action: Action,
    pub defaults: Vec<Binding>,
}

macro_rules! cmds {
    ($($id:expr => [$($k:expr),*] $a:expr;)*) => {
        &[$(($id, &[$($k),*], $a)),*]
    };
}

use crate::menu_dialogs::GoToField;
use crate::track_motion::TrackMotionKey;
use Action::{Cmd as C, Extra as X, Soon};

/// The TrackView context, in the order of VEGAS's list.
#[rustfmt::skip]
const TRACK_VIEW: &[(&str, &[&str], Action)] = cmds! {
    "CursorTo.Center" => ["\\"] C(Cmd::CenterOnCursor);
    "CursorTo.CycleTimeSelection" => ["Backspace"] C(Cmd::RestoreSelection);
    "CursorTo.End" => [] C(Cmd::ProjectEnd);
    "CursorTo.Home" => ["W"] C(Cmd::ProjectStart);
    "CursorTo.LeftByCDIndex" => [","] X(Extra::CdStep { indices: true, dir: -1 });
    "CursorTo.LeftByCDTrack" => ["Ctrl+,"] X(Extra::CdStep { indices: false, dir: -1 });
    "CursorTo.LeftByEdit" => ["Ctrl+Alt+Left"] C(Cmd::EditPointLeft);
    "CursorTo.LeftByFrame" => ["Alt+Left"] C(Cmd::FrameLeft);
    "CursorTo.LeftByGrid" => ["PageUp"] C(Cmd::GridLeft);
    "CursorTo.LeftByMarker" => ["Ctrl+Left"] C(Cmd::MarkerLeft);
    "CursorTo.LeftByPixel" => ["Left"] C(Cmd::PixelLeft);
    "CursorTo.LeftBySelected" => ["Ctrl+["] C(Cmd::SelectedEventLeft);
    "CursorTo.LoopEnd" => ["End"] C(Cmd::SelectionEndGo);
    "CursorTo.LoopHome" => ["Home"] C(Cmd::SelectionStartGo);
    "CursorTo.Marker0" => ["0"] X(Extra::GoToMarker(0));
    "CursorTo.Marker1" => ["1"] X(Extra::GoToMarker(1));
    "CursorTo.Marker2" => ["2"] X(Extra::GoToMarker(2));
    "CursorTo.Marker3" => ["3"] X(Extra::GoToMarker(3));
    "CursorTo.Marker4" => ["4"] X(Extra::GoToMarker(4));
    "CursorTo.Marker5" => ["5"] X(Extra::GoToMarker(5));
    "CursorTo.Marker6" => ["6"] X(Extra::GoToMarker(6));
    "CursorTo.Marker7" => ["7"] X(Extra::GoToMarker(7));
    "CursorTo.Marker8" => ["8"] X(Extra::GoToMarker(8));
    "CursorTo.Marker9" => ["9"] X(Extra::GoToMarker(9));
    "CursorTo.RightByCDIndex" => ["."] X(Extra::CdStep { indices: true, dir: 1 });
    "CursorTo.RightByCDTrack" => ["Ctrl+."] X(Extra::CdStep { indices: false, dir: 1 });
    "CursorTo.RightByEdit" => ["Ctrl+Alt+Right"] C(Cmd::EditPointRight);
    "CursorTo.RightByFrame" => ["Alt+Right"] C(Cmd::FrameRight);
    "CursorTo.RightByGrid" => ["PageDown"] C(Cmd::GridRight);
    "CursorTo.RightByMarker" => ["Ctrl+Right"] C(Cmd::MarkerRight);
    "CursorTo.RightByPixel" => ["Right"] C(Cmd::PixelRight);
    "CursorTo.RightBySelected" => ["Ctrl+]"] C(Cmd::SelectedEventRight);
    "Edit.ConvertTranstionToCut" => ["Ctrl+Num /"] X(Extra::TransitionToCut);
    "Edit.Copy" => [] C(Cmd::Copy);
    "Edit.Cut" => [] C(Cmd::Cut);
    "Edit.Delete" => [] C(Cmd::Delete);
    "Edit.Group.Clear" => ["Ctrl+U"] C(Cmd::GroupClear);
    "Edit.Group.CopyAll" => [] C(Cmd::GroupCopy);
    "Edit.Group.CreateNew" => ["G"] C(Cmd::GroupNew);
    "Edit.Group.CutAll" => [] C(Cmd::GroupCut);
    "Edit.Group.DeleteAll" => [] C(Cmd::GroupDelete);
    "Edit.Group.RemoveFrom" => ["U"] C(Cmd::GroupRemove);
    "Edit.Group.SelectAll" => ["Shift+G"] C(Cmd::GroupSelectAll);
    "Edit.Heal" => ["H"] C(Cmd::Rejoin);
    "Edit.Paste" => [] C(Cmd::Paste);
    "Edit.PasteInsert" => ["Ctrl+Shift+V"] C(Cmd::PasteInsert);
    "Edit.PasteRepeat" => ["Ctrl+B"] C(Cmd::PasteRepeat);
    "Edit.Redo" => [] C(Cmd::Redo);
    "Edit.Ripple.AffectedTracks" => ["F"] C(Cmd::Ripple(RippleScope::AffectedTracks));
    "Edit.Ripple.All" => ["Ctrl+Shift+F"] C(Cmd::Ripple(RippleScope::All));
    "Edit.Ripple.AllAffected" => ["Ctrl+F"] C(Cmd::Ripple(RippleScope::AffectedTracksAndMarkers));
    "Edit.Select.All" => [] C(Cmd::SelectAll);
    "Edit.Select.EventEnd" => ["Num 9", "]"] C(Cmd::SelectEventEnd);
    "Edit.Select.EventStart" => ["Num 7", "["] C(Cmd::SelectEventStart);
    "Edit.Select.None" => [] X(Extra::SelectNone);
    "Edit.Selection.End" => ["Ctrl+Shift+G"] X(Extra::TypeSelection(GoToField::SelectionEnd));
    "Edit.Selection.LeftBySelected" => ["Ctrl+Shift+["] C(Cmd::SelectionPrevEvent);
    "Edit.Selection.Length" => [] X(Extra::TypeSelection(GoToField::SelectionLength));
    "Edit.Selection.RightByMarker" => ["Ctrl+Shift+Right"] C(Cmd::SelectToMarker);
    "Edit.Selection.RightBySelected" => ["Ctrl+Shift+]"] C(Cmd::SelectionNextEvent);
    "Edit.Selection.Start" => ["Ctrl+G"] C(Cmd::GoTo);
    "Edit.SelectivelyPasteAttribute" => [] C(Cmd::PasteAttributesSelective);
    "Edit.SetMarkerTime" => ["Ctrl+K"] X(Extra::Details(DetailsOp::SetTime));
    "Edit.SmartSplit" => ["Alt+S"] C(Cmd::SmartSplit);
    "Edit.Split" => ["S"] C(Cmd::Split);
    "Edit.Take.NextTake" => ["T"] C(Cmd::TakeNext);
    "Edit.Take.PreviousTake" => ["Shift+T"] C(Cmd::TakePrevious);
    "Edit.Take.RenameActive" => ["F2"] C(Cmd::TakeRenameActive);
    "Edit.Tool.Envelope" => [] C(Cmd::UseTool(Tool::Envelope));
    "Edit.Tool.Next" => ["D"] C(Cmd::NextTool);
    "Edit.Tool.Normal" => ["Ctrl+D"] C(Cmd::UseTool(Tool::Edit));
    "Edit.Tool.Prev" => ["Shift+D"] C(Cmd::PrevTool);
    "Edit.Tool.Selection" => [] C(Cmd::UseTool(Tool::Select));
    "Edit.Tool.Shuffle" => [] C(Cmd::UseTool(Tool::Shuffle));
    "Edit.Tool.Slide" => [] C(Cmd::UseTool(Tool::Slide));
    "Edit.Tool.Slip" => [] C(Cmd::UseTool(Tool::Slip));
    "Edit.Tool.SplitTrim" => [] C(Cmd::UseTool(Tool::SplitTrim));
    "Edit.Tool.TimeStretchCompress" => [] C(Cmd::UseTool(Tool::Stretch));
    "Edit.Tool.Trim" => [] Soon;
    "Edit.Tool.Zoom" => [] C(Cmd::UseTool(Tool::Zoom));
    "Edit.Trim" => ["Ctrl+T"] C(Cmd::Trim);
    "Edit.Trim.End" => ["Alt+]"] C(Cmd::TrimEnd);
    "Edit.Trim.Start" => ["Alt+["] C(Cmd::TrimStart);
    "Edit.Trimmer.CutFromCursor" => ["A"] X(Extra::Trimmer(TrimmerOp::AddFromCursor));
    "Edit.Trimmer.CutToCursor" => ["Shift+A"] X(Extra::Trimmer(TrimmerOp::AddToCursor));
    "Edit.Trimmer.FitToFill" => [] X(Extra::Trimmer(TrimmerOp::FitToFill));
    "Edit.Trimmer.Overwrite" => [] X(Extra::Trimmer(TrimmerOp::Overwrite));
    "Edit.Undo" => [] C(Cmd::Undo);
    "Edit.UndoAll" => [] C(Cmd::UndoAll);
    "Insert.AdjustmentEvent" => [] C(Cmd::InsertAdjustmentEvent);
    "Insert.AdjustmentTrack" => ["Ctrl+Alt+Shift+Q"] C(Cmd::InsertAdjustmentTrack);
    "Insert.AudioCDTrackIndex" => ["Shift+N"] C(Cmd::InsertCdIndex);
    "Insert.AudioCDTrackRegion" => ["N"] C(Cmd::InsertCdTrack);
    "Insert.AudioTrack" => ["Ctrl+Q"] C(Cmd::InsertAudioTrack);
    "Insert.Command" => ["C"] C(Cmd::InsertCommand);
    "Insert.Envelope.TrackPan" => ["Shift+P"] C(Cmd::AudioEnvelope(EnvelopeKind::Pan));
    "Insert.Envelope.TrackVolume" => ["Shift+V"] C(Cmd::AudioEnvelope(EnvelopeKind::Volume));
    "Insert.Marker" => ["M"] C(Cmd::InsertMarker);
    "Insert.Region" => ["R"] C(Cmd::InsertRegion);
    "Insert.Transition1" => ["Num /"] X(Extra::CutToTransition);
    "Insert.Transition2" => ["Num *"] X(Extra::CutToTransition);
    "Insert.Transition3" => ["Num -"] X(Extra::CutToTransition);
    "Insert.VideoTrack" => ["Ctrl+Shift+Q"] C(Cmd::InsertVideoTrack);
    "Options.AutoCrossfade" => ["Ctrl+Shift+X"] C(Cmd::AutoCrossfade);
    "Options.DoAutoRipple" => ["Ctrl+L"] C(Cmd::AutoRipple);
    "Options.EnableSnapping" => ["F8"] C(Cmd::Snapping);
    "Options.IgnoreEventGrouping" => ["Ctrl+Shift+U"] C(Cmd::IgnoreGrouping);
    "Options.QuantizeToFrames" => ["Alt+F8"] C(Cmd::Quantize);
    "Options.SlipAllTakes" => [] C(Cmd::SlipAllTakes);
    "Options.SnapToAllEvents" => ["Ctrl+Shift+F8"] C(Cmd::SnapEvents);
    "Options.SnapToGrid" => ["Ctrl+F8"] C(Cmd::SnapGrid);
    "Options.SnapToMarkers" => ["Shift+F8"] C(Cmd::SnapMarkers);
    "Script.Add Timecode to all Media" => [] Soon;
    "Script.Batch Render" => [] Soon;
    "Script.Export Chapters" => [] C(Cmd::RunScript(Script::ExportChapters));
    "Script.Export Closed Captioning as DVD Subtitles" => [] C(Cmd::RunScript(Script::ExportCaptionsDvd));
    "Script.Export Closed Captioning as QuickTime Text" => [] C(Cmd::RunScript(Script::ExportCaptionsQuickTime));
    "Script.Export Closed Captioning as SAMI" => [] C(Cmd::RunScript(Script::ExportCaptionsSami));
    "Script.Export Closed Captioning as SubRip" => [] C(Cmd::RunScript(Script::ExportCaptionsSubRip));
    "Script.Export EDL" => [] C(Cmd::ExportEdl);
    "Script.Export Regions as Subtitles" => [] C(Cmd::RunScript(Script::ExportRegionsAsSubtitles));
    "Script.Group Video and Audio Events" => [] C(Cmd::RunScript(Script::GroupVideoAndAudio));
    "Script.Promote Media Closed Captioning" => [] C(Cmd::RunScript(Script::PromoteMediaCaptions));
    "Script.Promote Media Markers" => [] Soon;
    "Script.Remove Letterboxing" => [] C(Cmd::RunScript(Script::RemoveLetterboxing));
    "Script.Remove Timecode from all Media" => [] Soon;
    "Script.Render Audio Tracks" => [] Soon;
    "Script.Repair and Smooth unwanted Section" => [] Soon;
    "Script.Resize Generated Media" => [] Soon;
    "Script.Stereo to Mono" => [] C(Cmd::RunScript(Script::StereoToMono));
    "Tools.Audio.OpenInAudioEditor" => ["Ctrl+E"] C(Cmd::OpenInAudioEditor);
    "Tools.BuildRAMPreview" => ["Shift+B"] C(Cmd::BuildRamPreview);
    "Tools.ClearRAMPreview" => ["Alt+Shift+B"] C(Cmd::ClearRamPreview);
    "Tools.Multicamera.EnableMulticameraEditing" => ["Ctrl+Shift+D"] C(Cmd::MulticamEditing);
    "Tools.PrerenderVideo" => ["Shift+M"] C(Cmd::PrerenderVideo);
    "Tools.Render" => [] C(Cmd::Render);
    "Tools.Prerender Video" => [] C(Cmd::PrerenderVideo);
    "Tools.Apply Non-Real-Time Event FX" => [] C(Cmd::NonRealTimeFx);
    "Tools.Render to New Track" => [] C(Cmd::RenderToNewTrack);
    "Tools.RenderToNew" => ["Ctrl+M"] C(Cmd::RenderToNewTrack);
    "Tools.SpeechToText" => [] Soon;
    "Tools.TextToSpeech" => [] Soon;
    "Tools.Welcome Screen" => [] C(Cmd::Welcome);
    "TrackView.EnterExpandedEditMode" => ["Num 5"] C(Cmd::EditModeKey);
    "TrackView.Event.EditSourceProject" => [] C(Cmd::EditSourceProject);
    "TrackView.Event.OpenInTrimmer" => [] X(Extra::OpenInTrimmer);
    "TrackView.Event.OpenParentInTrimmer" => [] Soon;
    "TrackView.Event.PitchCentDown" => ["Ctrl+-"] X(Extra::Pitch(Some(-1)));
    "TrackView.Event.PitchCentUp" => ["Ctrl+="] X(Extra::Pitch(Some(1)));
    "TrackView.Event.PitchOctaveDown" => ["Shift+-"] X(Extra::Pitch(Some(-1200)));
    "TrackView.Event.PitchOctaveUp" => ["Shift+="] X(Extra::Pitch(Some(1200)));
    "TrackView.Event.PitchReset" => ["Ctrl+Shift+=", "Ctrl+Shift+-"] X(Extra::Pitch(None));
    "TrackView.Event.PitchSemiDown" => ["-"] X(Extra::Pitch(Some(-100)));
    "TrackView.Event.PitchSemiUp" => ["="] X(Extra::Pitch(Some(100)));
    "TrackView.Event.PitchThirdDown" => [] Soon;
    "TrackView.Event.PitchThirdUp" => [] Soon;
    "TrackView.Event.SelectInMediaPool" => [] X(Extra::SelectInMediaPool);
    "TrackView.ExitEventTrimMode" => ["Num 5"] C(Cmd::EditModeKey);
    "TrackView.FocusToNext" => ["Tab"] X(Extra::SwitchFocus);
    "TrackView.FocusToPrev" => [] X(Extra::SwitchFocus);
    "TrackView.Nudge.Down" => ["Num 2"] X(Extra::MoveToTrack(1));
    "TrackView.Nudge.LeftByFrames" => ["Num 1"] X(Extra::Nudge { frames: true, dir: -1 });
    "TrackView.Nudge.LeftByPixels" => ["Num 4"] X(Extra::Nudge { frames: false, dir: -1 });
    "TrackView.Nudge.RightByFrames" => ["Num 3"] X(Extra::Nudge { frames: true, dir: 1 });
    "TrackView.Nudge.RightByPixels" => ["Num 6"] X(Extra::Nudge { frames: false, dir: 1 });
    "TrackView.Nudge.Up" => ["Num 8"] X(Extra::MoveToTrack(-1));
    "TrackView.Refresh" => ["F5"] X(Extra::Refresh);
    "TrackView.Select.Loop" => ["Shift+L", "Shift+Q"] C(Cmd::SelectLoopRegion);
    "TrackView.Select.MarkIn" => ["I"] C(Cmd::SetSelectionStart);
    "TrackView.Select.MarkOut" => ["O"] C(Cmd::SetSelectionEnd);
    "TrackView.Select.SwapCursor" => [] X(Extra::SwapCursor);
    "TrackView.Shuttle.JogForward" => ["F9"] X(Extra::Jog(1));
    "TrackView.Shuttle.JogReverse" => ["F3"] X(Extra::Jog(-1));
    "TrackView.Shuttle.StepForward0" => ["F17"] Soon;
    "TrackView.Shuttle.StepForward1" => ["F18"] Soon;
    "TrackView.Shuttle.StepForward2" => ["F19"] Soon;
    "TrackView.Shuttle.StepForward3" => ["F20"] Soon;
    "TrackView.Shuttle.StepForward4" => ["F21"] Soon;
    "TrackView.Shuttle.StepForward5" => ["F22"] Soon;
    "TrackView.Shuttle.StepForward6" => ["F23"] Soon;
    "TrackView.Shuttle.StepForward7" => [] Soon;
    "TrackView.Shuttle.StepForward8" => [] Soon;
    "TrackView.Shuttle.StepForward9" => [] Soon;
    "TrackView.Shuttle.StepReverse0" => ["Ctrl+F17"] Soon;
    "TrackView.Shuttle.StepReverse1" => ["Ctrl+F18"] Soon;
    "TrackView.Shuttle.StepReverse2" => ["Ctrl+F19"] Soon;
    "TrackView.Shuttle.StepReverse3" => ["Ctrl+F20"] Soon;
    "TrackView.Shuttle.StepReverse4" => ["Ctrl+F21"] Soon;
    "TrackView.Shuttle.StepReverse5" => ["Ctrl+F22"] Soon;
    "TrackView.Shuttle.StepReverse6" => ["Ctrl+F23"] Soon;
    "TrackView.Shuttle.StepReverse7" => [] Soon;
    "TrackView.Shuttle.StepReverse8" => [] Soon;
    "TrackView.Shuttle.StepReverse9" => [] Soon;
    "TrackView.Shuttle.Stop" => ["F16"] X(Extra::Stop);
    "TrackView.Track.AutomationLatch" => [] C(Cmd::AutomationMode(bv_timeline::AutomationMode::Latch));
    "TrackView.Track.AutomationOff" => [] C(Cmd::AutomationMode(bv_timeline::AutomationMode::Off));
    "TrackView.Track.AutomationRead" => [] C(Cmd::AutomationMode(bv_timeline::AutomationMode::Read));
    "TrackView.Track.AutomationTouch" => [] C(Cmd::AutomationMode(bv_timeline::AutomationMode::Touch));
    "TrackView.Track.CollapseTrackGroup" => [] Soon;
    "TrackView.Track.CreateTrackGroup" => [] Soon;
    "TrackView.Track.CursorDown" => ["Alt+Shift+Down", "Next"] X(Extra::TrackCursor(TrackMove::Down));
    "TrackView.Track.CursorEnd" => ["Ctrl+Next"] X(Extra::TrackCursor(TrackMove::Last));
    "TrackView.Track.CursorHome" => ["Ctrl+Previous"] X(Extra::TrackCursor(TrackMove::First));
    "TrackView.Track.CursorPageDown" => [] X(Extra::TrackCursor(TrackMove::PageDown));
    "TrackView.Track.CursorPageUp" => [] X(Extra::TrackCursor(TrackMove::PageUp));
    "TrackView.Track.CursorTo0" => [] X(Extra::TrackCursor(TrackMove::Number(0)));
    "TrackView.Track.CursorTo1" => [] X(Extra::TrackCursor(TrackMove::Number(1)));
    "TrackView.Track.CursorTo2" => [] X(Extra::TrackCursor(TrackMove::Number(2)));
    "TrackView.Track.CursorTo3" => [] X(Extra::TrackCursor(TrackMove::Number(3)));
    "TrackView.Track.CursorTo4" => [] X(Extra::TrackCursor(TrackMove::Number(4)));
    "TrackView.Track.CursorTo5" => [] X(Extra::TrackCursor(TrackMove::Number(5)));
    "TrackView.Track.CursorTo6" => [] X(Extra::TrackCursor(TrackMove::Number(6)));
    "TrackView.Track.CursorTo7" => [] X(Extra::TrackCursor(TrackMove::Number(7)));
    "TrackView.Track.CursorTo8" => [] X(Extra::TrackCursor(TrackMove::Number(8)));
    "TrackView.Track.CursorTo9" => [] X(Extra::TrackCursor(TrackMove::Number(9)));
    "TrackView.Track.CursorUp" => ["Alt+Shift+Up", "Previous"] X(Extra::TrackCursor(TrackMove::Up));
    "TrackView.Track.Duplicate" => [] X(Extra::DuplicateTrack);
    "TrackView.Track.EditToPlayCursor" => ["Alt+Down"] C(Cmd::CursorToPlayhead);
    "TrackView.Track.ExpandTrackGroup" => [] Soon;
    "TrackView.Track.InputAuto" => [] Soon;
    "TrackView.Track.InputOff" => [] Soon;
    "TrackView.Track.InputOn" => [] Soon;
    "TrackView.Track.PanLeft" => ["Shift+Volume Down"] X(Extra::TrackPan(-1));
    "TrackView.Track.PanNum1" => [] X(Extra::TrackPanPosition(1));
    "TrackView.Track.PanNum2" => [] X(Extra::TrackPanPosition(2));
    "TrackView.Track.PanNum3" => [] X(Extra::TrackPanPosition(3));
    "TrackView.Track.PanNum4" => [] X(Extra::TrackPanPosition(4));
    "TrackView.Track.PanNum5" => [] X(Extra::TrackPanPosition(5));
    "TrackView.Track.PanNum6" => [] X(Extra::TrackPanPosition(6));
    "TrackView.Track.PanNum7" => [] X(Extra::TrackPanPosition(7));
    "TrackView.Track.PanNum8" => [] X(Extra::TrackPanPosition(8));
    "TrackView.Track.PanNum9" => [] X(Extra::TrackPanPosition(9));
    "TrackView.Track.PanRight" => ["Shift+Volume Up"] X(Extra::TrackPan(1));
    "TrackView.Track.RemoveFromTrackGroup" => [] Soon;
    "TrackView.Track.Rename" => [] X(Extra::RenameTrack);
    "TrackView.Track.RenameTrackGroup" => [] Soon;
    "TrackView.Track.ShowAutomationControls" => [] C(Cmd::AutomationControls);
    "TrackView.Track.ShowNextFxEnvelope" => ["E"] Soon;
    "TrackView.Track.ShowPrevFxEnvelope" => ["Shift+E"] Soon;
    "TrackView.Track.ToggleGroupMute" => ["Ctrl+Alt+Z"] Soon;
    "TrackView.Track.ToggleGroupMuteExclusive" => ["Ctrl+Alt+Shift+Z"] Soon;
    "TrackView.Track.ToggleGroupSolo" => ["Ctrl+Alt+X"] Soon;
    "TrackView.Track.ToggleGroupSoloExclusive" => ["Ctrl+Alt+Shift+X"] Soon;
    "TrackView.Track.ToggleMute" => ["Z", "Volume Mute"] X(Extra::TrackMute { exclusive: false });
    "TrackView.Track.ToggleMuteExclusive" => ["Shift+Z", "Shift+Volume Mute"] X(Extra::TrackMute { exclusive: true });
    "TrackView.Track.ToggleNonAutomatedMute" => [] Soon;
    "TrackView.Track.ToggleSolo" => ["X", "Ctrl+Volume Mute"] X(Extra::TrackSolo { exclusive: false });
    "TrackView.Track.ToggleSoloExclusive" => ["Shift+X", "Ctrl+Shift+Volume Mute"] X(Extra::TrackSolo { exclusive: true });
    "TrackView.Track.VolumeDown" => ["Ctrl+Volume Down"] X(Extra::TrackVolume(-1));
    "TrackView.Track.VolumeUp" => ["Ctrl+Volume Up"] X(Extra::TrackVolume(1));
    "TrackView.Zoom.DefaultTrackHeight" => ["Ctrl+`"] X(Extra::DefaultTrackHeights);
    "TrackView.Zoom.LessTrackHeight" => ["Ctrl+Shift+Down"] X(Extra::AllTrackHeights(-1));
    "TrackView.Zoom.LessTrackHeightBy1" => ["Ctrl+Alt+Shift+Down"] X(Extra::TrackHeightBy1(-1));
    "TrackView.Zoom.MaxTrackHeight" => [] X(Extra::TrackHeightMax);
    "TrackView.Zoom.MinTrackHeight" => ["`"] X(Extra::ToggleMinimizeTracks);
    "TrackView.Zoom.MoreTrackHeight" => ["Ctrl+Shift+Up"] X(Extra::AllTrackHeights(1));
    "TrackView.Zoom.MoreTrackHeightBy1" => ["Ctrl+Alt+Shift+Up"] X(Extra::TrackHeightBy1(1));
    "Transport.CursorPreview" => ["Num 0"] C(Cmd::CursorPreview);
    "Transport.Fastforward" => [] X(Extra::FastForward);
    "Transport.LoopPlayback" => ["Ctrl+Shift+L", "Q"] C(Cmd::LoopPlayback);
    "Transport.Pause" => ["Play"] X(Extra::Pause);
    "Transport.Play" => [] X(Extra::Play);
    "Transport.PlayFromStart" => [] X(Extra::PlayFromStart);
    "Transport.PlayInReverse" => [] X(Extra::PlayReverse);
    "Transport.PlayToggle" => [] X(Extra::PlayToggle);
    "Transport.Record" => ["Ctrl+R"] C(Cmd::Record);
    "Transport.Rewind" => [] X(Extra::Rewind);
    "Transport.ScrubLeft" => ["J"] X(Extra::ScrubLeft);
    "Transport.ScrubPause" => ["K"] X(Extra::ScrubPause);
    "Transport.ScrubRight" => ["L"] X(Extra::ScrubRight);
    "Transport.SetPathAndRecord" => ["Ctrl+Alt+Shift+R"] C(Cmd::SetPathAndRecord);
    "Transport.StepBack" => [] X(Extra::StepBack);
    "Transport.StepForward" => [] X(Extra::StepForward);
    "Transport.Stop" => ["Stop"] X(Extra::Stop);
    "Transport.TogglePlayFromStart" => ["Shift+Play"] X(Extra::PlayFromStart);
    "View.AudioBusTracks" => ["B"] C(Cmd::AudioBusTracks);
    "View.Envelope.TrackPan" => ["P"] C(Cmd::ShowEnvelopeType(EnvelopeKind::Pan));
    "View.Envelope.TrackVolume" => ["V"] C(Cmd::ShowEnvelopeType(EnvelopeKind::Volume));
    "View.Hardware Detection" => [] Soon;
    "View.Motion Tracking" => [] C(Cmd::MotionTracking);
    "View.RebuildAudioPeaks" => ["Shift+F5"] C(Cmd::RebuildPeaks);
    "View.Explorer" => [] C(Cmd::Window(Tab::Explorer));
    "View.VideoBusTrack" => ["Ctrl+Shift+B"] C(Cmd::VideoBusTrack);
};

/// The Explorer context (the Explorer window focused): VEGAS lists the menu
/// commands here, none of them with a default key.
#[rustfmt::skip]
const EXPLORER: &[(&str, &[&str], Action)] = cmds! {
    "Edit.ClearEditHistory" => [] C(Cmd::ClearHistory);
    "Edit.Copy" => [] C(Cmd::Copy);
    "Edit.Cut" => [] C(Cmd::Cut);
    "Edit.Delete" => [] C(Cmd::Delete);
    "Edit.Event.DisableResample" => [] C(Cmd::Resample(Some(bv_timeline::ResampleMode::Disable)));
    "Edit.Event.ForceResample" => [] C(Cmd::Resample(Some(bv_timeline::ResampleMode::FrameBlend)));
    "Edit.Event.InvertPhase" => [] C(Cmd::InvertPhase);
    "Edit.Event.Lock" => [] C(Cmd::Lock);
    "Edit.Event.Loop" => [] C(Cmd::Loop);
    "Edit.Event.MaintainAspectRatio" => [] C(Cmd::MaintainAspect);
    "Edit.Event.Mute" => [] C(Cmd::Mute);
    "Edit.Event.Normalize" => [] C(Cmd::Normalize);
    "Edit.Event.ReduceInterlaceFlicker" => [] C(Cmd::ReduceFlicker);
    "Edit.Event.SmartResample" => [] C(Cmd::Resample(None));
    "Edit.Group.Clear" => [] C(Cmd::GroupClear);
    "Edit.Group.CreateNew" => [] C(Cmd::GroupNew);
    "Edit.Group.RemoveFrom" => [] C(Cmd::GroupRemove);
    "Edit.Group.SelectAll" => [] C(Cmd::GroupSelectAll);
    "Edit.Heal" => [] C(Cmd::Rejoin);
    "Edit.Paste" => [] C(Cmd::Paste);
    "Edit.PasteEventAttributes" => [] C(Cmd::PasteAttributes);
    "Edit.PasteInsert" => [] C(Cmd::PasteInsert);
    "Edit.PasteRepeat" => [] C(Cmd::PasteRepeat);
    "Edit.Redo" => [] C(Cmd::Redo);
    "Edit.Ripple.AffectedTracks" => [] C(Cmd::Ripple(RippleScope::AffectedTracks));
    "Edit.Ripple.All" => [] C(Cmd::Ripple(RippleScope::All));
    "Edit.Ripple.AllAffected" => [] C(Cmd::Ripple(RippleScope::AffectedTracksAndMarkers));
    "Edit.Select.All" => [] C(Cmd::SelectAll);
    "Edit.Select.EventEnd" => [] C(Cmd::SelectEventEnd);
    "Edit.Select.EventStart" => [] C(Cmd::SelectEventStart);
    "Edit.Select.None" => [] X(Extra::SelectNone);
    "Edit.SelectivelyPasteAttribute" => [] C(Cmd::PasteAttributesSelective);
    "Edit.SelectivelyPasteEventAttributes" => [] C(Cmd::PasteAttributesSelective);
    "Edit.SetMarkerTime" => [] Soon;
    "Edit.SmartSplit" => [] C(Cmd::SmartSplit);
    "Edit.Split" => [] C(Cmd::Split);
    "Edit.Take.ChooseActive" => [] C(Cmd::TakeChooseActive);
    "Edit.Take.Delete" => [] C(Cmd::TakeDelete);
    "Edit.Take.DeleteActive" => [] C(Cmd::TakeDeleteActive);
    "Edit.Take.NextTake" => [] C(Cmd::TakeNext);
    "Edit.Take.PreviousTake" => [] C(Cmd::TakePrevious);
    "Edit.Take.RenameActive" => [] C(Cmd::TakeRenameActive);
    "Edit.Tool.Envelope" => [] C(Cmd::UseTool(Tool::Envelope));
    "Edit.Tool.Next" => [] C(Cmd::NextTool);
    "Edit.Tool.Normal" => [] C(Cmd::UseTool(Tool::Edit));
    "Edit.Tool.Prev" => [] C(Cmd::PrevTool);
    "Edit.Tool.Selection" => [] C(Cmd::UseTool(Tool::Select));
    "Edit.Tool.Shuffle" => [] C(Cmd::UseTool(Tool::Shuffle));
    "Edit.Tool.Slide" => [] C(Cmd::UseTool(Tool::Slide));
    "Edit.Tool.Slip" => [] C(Cmd::UseTool(Tool::Slip));
    "Edit.Tool.SplitTrim" => [] C(Cmd::UseTool(Tool::SplitTrim));
    "Edit.Tool.TimeStretchCompress" => [] C(Cmd::UseTool(Tool::Stretch));
    "Edit.Tool.Trim" => [] Soon;
    "Edit.Tool.Zoom" => [] C(Cmd::UseTool(Tool::Zoom));
    "Edit.Trim" => [] C(Cmd::Trim);
    "Edit.Trim.End" => [] C(Cmd::TrimEnd);
    "Edit.Trim.Start" => [] C(Cmd::TrimStart);
    "Edit.Undo" => [] C(Cmd::Undo);
    "Edit.UndoAll" => [] C(Cmd::UndoAll);
    "File.Archive" => [] Soon;
    "File.Close" => [] C(Cmd::Close);
    "File.Exit" => [] C(Cmd::Exit);
    "File.ExportAAFAVID" => [] Soon;
    "File.ExportAAF" => [] Soon;
    "File.ExportEDL" => [] C(Cmd::ExportEdl);
    "File.ExportFCPXML" => [] C(Cmd::ExportFcpxml);
    "File.ExportPRPROJ" => [] Soon;
    "File.ExportProject" => [] C(Cmd::ExportProject);
    "File.ExportXML" => [] C(Cmd::ExportXml);
    "File.ExtractAudioFromCD" => [] Soon;
    "File.ImportAAF" => [] Soon;
    "File.ImportBWF" => [] C(Cmd::ImportBwf);
    "File.ImportClosedCaptioning" => [] C(Cmd::ImportClosedCaptioning);
    "File.ImportEDL" => [] C(Cmd::ImportEdl);
    "File.ImportFCPXML" => [] C(Cmd::ImportFcpxml);
    "File.ImportMedia" => [] C(Cmd::ImportMedia);
    "File.ImportMediaFromProject" => [] C(Cmd::ImportProjectMedia);
    "File.ImportPRPROJ" => [] Soon;
    "File.ImportXML" => [] C(Cmd::ImportXml);
    "File.ManageMobileClips" => [] Soon;
    "File.New" => [] C(Cmd::New);
    "File.New.Quick" => [] Soon;
    "File.Open" => [] C(Cmd::Open);
    "File.Properties" => [] C(Cmd::Properties);
    "File.RenderAs" => [] C(Cmd::Render);
    "File.Save" => [] C(Cmd::Save);
    "File.SaveAs" => [] C(Cmd::SaveAs);
    "File.SaveInc" => [] C(Cmd::SaveIncremental);
    "File.SyncToMobile" => [] Soon;
    "Help.Contents" => [] C(Cmd::HelpContents);
    "Help.DeactivateRegistration" => [] Soon;
    "Help.GettingStarted" => [] Soon;
    "Help.Keyboard" => [] C(Cmd::KeyboardShortcuts);
    "Help.WhatsThis" => [] Soon;
    "Insert.AdjustmentEvent" => [] C(Cmd::InsertAdjustmentEvent);
    "Insert.AdjustmentTrack" => [] C(Cmd::InsertAdjustmentTrack);
    "Insert.AudioBus" => [] C(Cmd::InsertAudioBus);
    "Insert.AudioCDTrackIndex" => [] C(Cmd::InsertCdIndex);
    "Insert.AudioCDTrackRegion" => [] C(Cmd::InsertCdTrack);
    "Insert.AudioTrack" => [] C(Cmd::InsertAudioTrack);
    "Insert.Command" => [] C(Cmd::InsertCommand);
    "Insert.EmptyEvent" => [] C(Cmd::EmptyEvent);
    "Insert.Envelope.EventVelocity" => [] C(Cmd::VideoEnvelope(EnvelopeKind::Velocity));
    "Insert.Envelope.TrackCompositeLevel" => [] C(Cmd::VideoEnvelope(EnvelopeKind::CompositeLevel));
    "Insert.Envelope.TrackFadeToColor" => [] C(Cmd::VideoEnvelope(EnvelopeKind::FadeToColor));
    "Insert.Envelope.TrackPan" => [] C(Cmd::AudioEnvelope(EnvelopeKind::Pan));
    "Insert.Envelope.TrackVolume" => [] C(Cmd::AudioEnvelope(EnvelopeKind::Volume));
    "Insert.Envelope.TransitionProgress" => [] C(Cmd::VideoEnvelope(EnvelopeKind::TransitionProgress));
    "Insert.GeneratedMedia" => [] C(Cmd::Window(Tab::Generators));
    "Insert.Marker" => [] C(Cmd::InsertMarker);
    "Insert.Region" => [] C(Cmd::InsertRegion);
    "Insert.Slideshow" => [] C(Cmd::Slideshow);
    "Insert.TextMedia" => [] C(Cmd::InsertTextMedia);
    "Insert.Time" => [] C(Cmd::InsertTime);
    "Insert.VideoTrack" => [] C(Cmd::InsertVideoTrack);
    "Mixer.Dim" => [] C(Cmd::Dim);
    "Mixer.Downmix" => [] C(Cmd::Downmix);
    "Options.AutoCrossfade" => [] C(Cmd::AutoCrossfade);
    "Options.BypassAllAudioFX" => [] C(Cmd::BypassAudioFx);
    "Options.CustomizeTimelineToolbar" => [] C(Cmd::CustomizeTimelineToolbar);
    "Options.CustomizeToolbar" => [] C(Cmd::CustomizeToolbar);
    "Options.DoAutoRipple" => [] C(Cmd::AutoRipple);
    "Options.EnableSnapping" => [] C(Cmd::Snapping);
    "Options.IgnoreEventGrouping" => [] C(Cmd::IgnoreGrouping);
    "Options.LockEnvelopesToEvents" => [] C(Cmd::LockEnvelopes);
    "Options.Metronome" => [] C(Cmd::Metronome);
    "Options.MuteAllAudio" => [] C(Cmd::MuteAllAudio);
    "Options.MuteAllVideo" => [] C(Cmd::MuteAllVideo);
    "Options.Preferences" => [] C(Cmd::Preferences);
    "Options.QuantizeToFrames" => [] C(Cmd::Quantize);
    "Options.QuickfadeAudioEdits" => [] C(Cmd::QuickFades);
    "Options.RulerFormatAbsoluteFrames" => [] C(Cmd::Ruler(RulerFormat::AbsoluteFrames));
    "Options.RulerFormatCDTime" => [] C(Cmd::Ruler(RulerFormat::AudioCd));
    "Options.RulerFormatFeetandFrames16mm" => [] C(Cmd::Ruler(RulerFormat::Feet16));
    "Options.RulerFormatFeetandFrames35mm" => [] C(Cmd::Ruler(RulerFormat::Feet35));
    "Options.RulerFormatMeasuresAndBeats" => [] C(Cmd::Ruler(RulerFormat::MeasuresBeats));
    "Options.RulerFormatSamples" => [] C(Cmd::Ruler(RulerFormat::Samples));
    "Options.RulerFormatSeconds" => [] C(Cmd::Ruler(RulerFormat::Seconds));
    "Options.RulerFormatSMPTE30" => [] C(Cmd::Ruler(RulerFormat::Smpte30));
    "Options.RulerFormatSMPTEDrop" => [] C(Cmd::Ruler(RulerFormat::SmpteDrop));
    "Options.RulerFormatSMPTEEBU" => [] C(Cmd::Ruler(RulerFormat::SmpteEbu));
    "Options.RulerFormatSMPTEFilmSync" => [] C(Cmd::Ruler(RulerFormat::SmpteFilm));
    "Options.RulerFormatSMPTEFilmSyncIVTC" => [] C(Cmd::Ruler(RulerFormat::SmpteFilmSyncIvtc));
    "Options.RulerFormatSMPTENon-Drop" => [] C(Cmd::Ruler(RulerFormat::SmpteNonDrop));
    "Options.RulerFormatTime" => [] C(Cmd::Ruler(RulerFormat::Time));
    "Options.RulerFormatTimeFrames" => [] C(Cmd::Ruler(RulerFormat::TimeAndFrames));
    "Options.SetTimeAtCursor" => [] C(Cmd::SetTimeAtCursor);
    "Options.SetProjectTempo" => [] C(Cmd::SetProjectTempo);
    "Options.SlipAllTakes" => [] C(Cmd::SlipAllTakes);
    "Options.SnapTo16thNotes" => [] C(Cmd::Grid(GridSpacing::SixteenthNotes));
    "Options.SnapTo16thNoteTriplets" => [] C(Cmd::Grid(GridSpacing::SixteenthNoteTriplets));
    "Options.SnapTo32ndNotes" => [] C(Cmd::Grid(GridSpacing::ThirtySecondNotes));
    "Options.SnapTo32ndNoteTriplets" => [] C(Cmd::Grid(GridSpacing::ThirtySecondNoteTriplets));
    "Options.SnapTo8thNotes" => [] C(Cmd::Grid(GridSpacing::EighthNotes));
    "Options.SnapTo8thNoteTriplets" => [] C(Cmd::Grid(GridSpacing::EighthNoteTriplets));
    "Options.SnapToAllEvents" => [] C(Cmd::SnapEvents);
    "Options.SnapToFrames" => [] C(Cmd::Grid(GridSpacing::Frames));
    "Options.SnapToGrid" => [] C(Cmd::SnapGrid);
    "Options.SnapToMarkers" => [] C(Cmd::SnapMarkers);
    "Options.SnapToMeasures" => [] C(Cmd::Grid(GridSpacing::Measures));
    "Options.SnapToMilliseconds" => [] C(Cmd::Grid(GridSpacing::Milliseconds));
    "Options.SnapToMinutes" => [] C(Cmd::Grid(GridSpacing::Minutes));
    "Options.SnapToQuarterNotes" => [] C(Cmd::Grid(GridSpacing::QuarterNotes));
    "Options.SnapToQuarterNoteTriplets" => [] C(Cmd::Grid(GridSpacing::QuarterNoteTriplets));
    "Options.SnapToRulerMarks" => [] C(Cmd::Grid(GridSpacing::RulerMarks));
    "Options.SnapToSeconds" => [] C(Cmd::Grid(GridSpacing::Seconds));
    "Preview.BypassZoom" => [] X(Extra::PreviewBypassZoom);
    "Preview.DefaultZoom" => [] X(Extra::PreviewZoomDefault);
    "Preview.MinusZoom" => [] X(Extra::PreviewZoom(-1));
    "Preview.PlusZoom" => [] X(Extra::PreviewZoom(1));
    "Script.Add Timecode to all Media" => [] Soon;
    "Script.Batch Render" => [] Soon;
    "Script.Export Chapters" => [] C(Cmd::RunScript(Script::ExportChapters));
    "Script.Export Closed Captioning as DVD Subtitles" => [] C(Cmd::RunScript(Script::ExportCaptionsDvd));
    "Script.Export Closed Captioning as QuickTime Text" => [] C(Cmd::RunScript(Script::ExportCaptionsQuickTime));
    "Script.Export Closed Captioning as SAMI" => [] C(Cmd::RunScript(Script::ExportCaptionsSami));
    "Script.Export Closed Captioning as SubRip" => [] C(Cmd::RunScript(Script::ExportCaptionsSubRip));
    "Script.Export EDL" => [] C(Cmd::ExportEdl);
    "Script.Export Regions as Subtitles" => [] C(Cmd::RunScript(Script::ExportRegionsAsSubtitles));
    "Script.Group Video and Audio Events" => [] C(Cmd::RunScript(Script::GroupVideoAndAudio));
    "Script.Promote Media Closed Captioning" => [] C(Cmd::RunScript(Script::PromoteMediaCaptions));
    "Script.Promote Media Markers" => [] Soon;
    "Script.Remove Letterboxing" => [] C(Cmd::RunScript(Script::RemoveLetterboxing));
    "Script.Remove Timecode from all Media" => [] Soon;
    "Script.Render Audio Tracks" => [] Soon;
    "Script.Repair and Smooth unwanted Section" => [] Soon;
    "Script.Resize Generated Media" => [] Soon;
    "Script.Stereo to Mono" => [] C(Cmd::RunScript(Script::StereoToMono));
    "Sync.GenerateMIDI" => [] C(Cmd::GenerateClock);
    "Sync.GenerateMTC" => [] C(Cmd::GenerateMtc);
    "Sync.ListenToMTC" => [] C(Cmd::TriggerMtc);
    "Tools.Audio.ApplyEventFX" => [] C(Cmd::AudioEventFx);
    "Tools.Audio.EditTrackFX" => [] C(Cmd::AudioTrackFx);
    "Tools.Audio.InsertFX" => [] Soon;
    "Tools.Audio.OpenCopyInAudioEditor" => [] C(Cmd::OpenCopyInAudioEditor);
    "Tools.Audio.OpenInAudioEditor" => [] C(Cmd::OpenInAudioEditor);
    "Tools.Audio.RemoveFX" => [] Soon;
    "Tools.BuildRAMPreview" => [] C(Cmd::BuildRamPreview);
    "Tools.Burn.BluRayDisc" => [] Soon;
    "Tools.Burn.Disc-at-OnceAudioCD" => [] C(Cmd::BurnDaoCd);
    "Tools.Burn.DVD" => [] C(Cmd::BurnDvd);
    "Tools.Burn.Track-at-OnceAudioCD" => [] C(Cmd::BurnTaoCd);
    "Tools.CleanMediaPool" => [] C(Cmd::CleanProjectMedia);
    "Tools.ClearRAMPreview" => [] C(Cmd::ClearRamPreview);
    "Tools.DeletePrerenderedVideo" => [] C(Cmd::CleanUpPrerender);
    "Tools.GenerateLoudnessLog" => [] C(Cmd::LoudnessLog);
    "Tools.LayOutAudioCD" => [] C(Cmd::LayOutCd);
    "Tools.Multicamera.AudioSync" => [] C(Cmd::MulticamSyncAudio);
    "Tools.PrerenderVideo" => [] C(Cmd::PrerenderVideo);
    "Tools.PrintToHDVTape" => [] Soon;
    "Tools.PrintToTape" => [] Soon;
    "Tools.Render" => [] C(Cmd::Render);
    "Tools.Prerender Video" => [] C(Cmd::PrerenderVideo);
    "Tools.Apply Non-Real-Time Event FX" => [] C(Cmd::NonRealTimeFx);
    "Tools.Render to New Track" => [] C(Cmd::RenderToNewTrack);
    "Tools.RenderToNew" => [] Soon;
    "Tools.RunScript" => [] Soon;
    "Tools.SpeechToText" => [] Soon;
    "Tools.TextToSpeech" => [] Soon;
    "Tools.Video.MediaFX" => [] C(Cmd::MediaFx);
    "Tools.Video.TrackMotion" => [] C(Cmd::TrackMotion);
    "Tools.Video.VideoEventFX" => [] C(Cmd::VideoEventFx);
    "Tools.Video.VideoEventPanCrop" => [] C(Cmd::PanCropEvent);
    "Tools.Video.VideoOutputFX" => [] C(Cmd::VideoOutputFx);
    "Tools.Video.VideoTrackFX" => [] C(Cmd::VideoTrackFx);
    "Tools.Welcome Screen" => [] C(Cmd::Welcome);
    "TrackView.Event.OpenAsNestedTimeline" => [] C(Cmd::OpenNestedTimeline);
    "TrackView.Timeline.CreateNestedTimeline" => [] C(Cmd::CreateNestedTimeline);
    "TrackView.Timeline.OpenParentTimeline" => [] C(Cmd::OpenParentTimeline);
    "Transport.Fastforward" => [] X(Extra::FastForward);
    "Transport.LoopPlayback" => [] C(Cmd::LoopPlayback);
    "Transport.Pause" => [] X(Extra::Pause);
    "Transport.PlayFromStart" => [] X(Extra::PlayFromStart);
    "Transport.PlayToggle" => [] X(Extra::PlayToggle);
    "Transport.Record" => [] C(Cmd::Record);
    "Transport.Rewind" => [] X(Extra::Rewind);
    "Transport.StepBack" => [] X(Extra::StepBack);
    "Transport.StepForward" => [] X(Extra::StepForward);
    "Transport.Stop" => [] X(Extra::Stop);
    "VideoPreview.ExternalMonitorToggle" => [] C(Cmd::ExternalMonitor);
    "View.AudioBusTracks" => [] C(Cmd::AudioBusTracks);
    "View.ColorGrading" => [] C(Cmd::Window(Tab::ColorGrading));
    "View.DeviceExplorer" => [] C(Cmd::DeviceExplorer);
    "View.EditDetails" => [] C(Cmd::Window(Tab::EditDetails));
    "View.Envelope.EventVelocity" => [] C(Cmd::ShowEnvelopeType(EnvelopeKind::Velocity));
    "View.Envelope.MotionBlurAmount" => [] C(Cmd::ShowEnvelopeType(EnvelopeKind::MotionBlur));
    "View.Envelope.TrackComposite Level" => [] C(Cmd::ShowEnvelopeType(EnvelopeKind::CompositeLevel));
    "View.Envelope.TrackFade to Color" => [] C(Cmd::ShowEnvelopeType(EnvelopeKind::FadeToColor));
    "View.Envelope.TrackKeyframes" => [] Soon;
    "View.Envelope.TrackPan" => [] C(Cmd::ShowEnvelopeType(EnvelopeKind::Pan));
    "View.Envelope.TrackVolume" => [] C(Cmd::ShowEnvelopeType(EnvelopeKind::Volume));
    "View.Envelope.TransitionProgress" => [] C(Cmd::ShowEnvelopeType(EnvelopeKind::TransitionProgress));
    "View.Envelope.VideoSupersampling" => [] C(Cmd::ShowEnvelopeType(EnvelopeKind::Supersampling));
    "Event Handles" => [] Soon;
    "View.EventMediaMarkers" => [] C(Cmd::EventMediaMarkers);
    "View.FocusToTrackView" => [] C(Cmd::FocusTimeline);
    "View.Hardware Detection" => [] Soon;
    "View.HubExplorer" => [] Soon;
    "View.LoudnessMeters" => [] C(Cmd::Window(Tab::LoudnessMeters));
    "View.MediaGenerators" => [] C(Cmd::Window(Tab::Generators));
    "View.MinimizeAll" => [] X(Extra::MaximizeTimeline);
    "View.MinimizeAllDocks" => [] X(Extra::DockArea(None));
    "View.MinimizeAllTracks" => [] C(Cmd::MinimizeTracks);
    "View.MinimizeDock" => [] X(Extra::DockArea(Some(crate::dock::Area::Top)));
    "View.MinimizeLabels" => [] X(Extra::FoldTrackHeaders);
    "View.MinimizeLowerDock" => [] X(Extra::DockArea(Some(crate::dock::Area::Bottom)));
    "View.MinimizeRightDock" => [] X(Extra::DockArea(Some(crate::dock::Area::Right)));
    "View.MixingConsole" => [] C(Cmd::Window(Tab::MixingConsole));
    "View.MasterBus" => [] C(Cmd::Window(Tab::Master));
    "View.MixerPreviewFader" => [] C(Cmd::PreviewFader);
    "View.MotionTrackGuiCommand" => [] C(Cmd::MotionTracking);
    "View.Motion Tracking" => [] C(Cmd::MotionTracking);
    "View.Next" => [] Soon;
    "View.Plug-InManager" => [] C(Cmd::Window(Tab::PluginManager));
    "View.Prev" => [] Soon;
    "View.ProjectMedia" => [] C(Cmd::Window(Tab::ProjectMedia));
    "View.ProjectNotes" => [] C(Cmd::Window(Tab::Notes));
    "View.RebuildAudioPeaks" => [] C(Cmd::RebuildPeaks);
    "View.ScriptEdit" => [] Soon;
    "View.ShowActiveTakeInformation" => [] C(Cmd::ActiveTakeInfo);
    "View.ShowEnvelopes" => [] C(Cmd::ShowEnvelopes);
    "View.ShowEventButtons" => [] C(Cmd::EventButtons);
    "Event Headers" => [] C(Cmd::EventHeaders);
    "Event Length" => [] C(Cmd::EventDuration);
    "View.ShowFadeLengths" => [] C(Cmd::FadeDurations);
    "View.ShowVideoEventEdgeFramesDuringEdits" => [] C(Cmd::EdgeFrames);
    "View.ShowWaveformsAndFramesInEvents" => [] C(Cmd::Waveforms);
    "View.StatusBar" => [] C(Cmd::StatusBar);
    "View.SurroundPanner" => [] C(Cmd::Window(Tab::SurroundPanner));
    "View.ToggleDock" => [] Soon;
    "View.ToggleLabels" => [] Soon;
    "View.ToggleMinimizeAll" => [] Soon;
    "View.Toolbar" => [] C(Cmd::Toolbar);
    "View.Transitions" => [] C(Cmd::Window(Tab::Transitions));
    "View.Trimmer" => [] C(Cmd::Window(Tab::Trimmer));
    "View.UnifiedExplorer" => [] C(Cmd::Window(Tab::Explorer));
    "View.Explorer" => [] C(Cmd::Window(Tab::Explorer));
    "View.Video Preview" => [] C(Cmd::Window(Tab::Preview));
    "View.VideoBusTrack" => [] C(Cmd::VideoBusTrack);
    "View.VideoFX" => [] C(Cmd::Window(Tab::VideoFx));
    "View.VideoScopes" => [] C(Cmd::Window(Tab::Scopes));
    "View.WindowLayouts.Load.AudioMixing" => [] C(Cmd::Layout(LayoutPreset::AudioMixing));
    "View.WindowLayouts.Load.ColorCorrection" => [] C(Cmd::Layout(LayoutPreset::ColorCorrection));
    "View.WindowLayouts.Load.Default" => [] C(Cmd::Layout(LayoutPreset::Default));
    "View.WindowLayouts.Load.SourceWindowLayout" => [] C(Cmd::Layout(LayoutPreset::SourceWindow));
    "View.WindowLayouts.Load.TimelineTopLayout" => [] C(Cmd::Layout(LayoutPreset::TimelineTop));
    "View.WindowLayouts.Load.UserLayout0" => [] C(Cmd::LayoutSlot(0));
    "View.WindowLayouts.Load.UserLayout1" => [] C(Cmd::LayoutSlot(1));
    "View.WindowLayouts.Load.UserLayout2" => [] C(Cmd::LayoutSlot(2));
    "View.WindowLayouts.Load.UserLayout3" => [] C(Cmd::LayoutSlot(3));
    "View.WindowLayouts.Load.UserLayout4" => [] C(Cmd::LayoutSlot(4));
    "View.WindowLayouts.Load.UserLayout5" => [] C(Cmd::LayoutSlot(5));
    "View.WindowLayouts.Load.UserLayout6" => [] C(Cmd::LayoutSlot(6));
    "View.WindowLayouts.Load.UserLayout7" => [] C(Cmd::LayoutSlot(7));
    "View.WindowLayouts.Load.UserLayout8" => [] C(Cmd::LayoutSlot(8));
    "View.WindowLayouts.Load.UserLayout9" => [] C(Cmd::LayoutSlot(9));
    "View.WindowLayouts.Save.UserLayout0" => [] X(Extra::SaveLayout(0));
    "View.WindowLayouts.Save.UserLayout1" => [] X(Extra::SaveLayout(1));
    "View.WindowLayouts.Save.UserLayout2" => [] X(Extra::SaveLayout(2));
    "View.WindowLayouts.Save.UserLayout3" => [] X(Extra::SaveLayout(3));
    "View.WindowLayouts.Save.UserLayout4" => [] X(Extra::SaveLayout(4));
    "View.WindowLayouts.Save.UserLayout5" => [] X(Extra::SaveLayout(5));
    "View.WindowLayouts.Save.UserLayout6" => [] X(Extra::SaveLayout(6));
    "View.WindowLayouts.Save.UserLayout7" => [] X(Extra::SaveLayout(7));
    "View.WindowLayouts.Save.UserLayout8" => [] X(Extra::SaveLayout(8));
    "View.WindowLayouts.Save.UserLayout9" => [] X(Extra::SaveLayout(9));
};

/// The Trimmer context (the Trimmer window focused), in the order of
/// VEGAS's list. Cursor, selection and add-to-timeline commands act on the
/// Trimmer; the others on the project like in TrackView.
#[rustfmt::skip]
const TRIMMER: &[(&str, &[&str], Action)] = cmds! {
    "CursorTo.Center" => [] Soon;
    "CursorTo.CycleTimeSelection" => [] Soon;
    "CursorTo.End" => ["Ctrl+End"] X(Extra::Trimmer(TrimmerOp::End));
    "CursorTo.Home" => ["Ctrl+Home"] X(Extra::Trimmer(TrimmerOp::Start));
    "CursorTo.LeftByFrame" => ["Alt+Left"] X(Extra::Trimmer(TrimmerOp::Frame(-1)));
    "CursorTo.LeftByGrid" => [] Soon;
    "CursorTo.LeftByMarker" => ["Ctrl+Left"] X(Extra::Trimmer(TrimmerOp::MarkerStep(-1)));
    "CursorTo.LeftByPixel" => ["Left"] X(Extra::Trimmer(TrimmerOp::Pixel(-1)));
    "CursorTo.LoopEnd" => [] X(Extra::Trimmer(TrimmerOp::GoToOut));
    "CursorTo.LoopHome" => [] X(Extra::Trimmer(TrimmerOp::GoToIn));
    "CursorTo.Marker0" => ["0"] X(Extra::Trimmer(TrimmerOp::GoToMarker(0)));
    "CursorTo.Marker1" => ["1"] X(Extra::Trimmer(TrimmerOp::GoToMarker(1)));
    "CursorTo.Marker2" => ["2"] X(Extra::Trimmer(TrimmerOp::GoToMarker(2)));
    "CursorTo.Marker3" => ["3"] X(Extra::Trimmer(TrimmerOp::GoToMarker(3)));
    "CursorTo.Marker4" => ["4"] X(Extra::Trimmer(TrimmerOp::GoToMarker(4)));
    "CursorTo.Marker5" => ["5"] X(Extra::Trimmer(TrimmerOp::GoToMarker(5)));
    "CursorTo.Marker6" => ["6"] X(Extra::Trimmer(TrimmerOp::GoToMarker(6)));
    "CursorTo.Marker7" => ["7"] X(Extra::Trimmer(TrimmerOp::GoToMarker(7)));
    "CursorTo.Marker8" => ["8"] X(Extra::Trimmer(TrimmerOp::GoToMarker(8)));
    "CursorTo.Marker9" => ["9"] X(Extra::Trimmer(TrimmerOp::GoToMarker(9)));
    "CursorTo.RightByFrame" => ["Alt+Right"] X(Extra::Trimmer(TrimmerOp::Frame(1)));
    "CursorTo.RightByGrid" => [] Soon;
    "CursorTo.RightByMarker" => ["Ctrl+Right"] X(Extra::Trimmer(TrimmerOp::MarkerStep(1)));
    "CursorTo.RightByPixel" => ["Right"] X(Extra::Trimmer(TrimmerOp::Pixel(1)));
    "Edit.Ripple.AffectedTracks" => ["F"] C(Cmd::Ripple(RippleScope::AffectedTracks));
    "Edit.Ripple.All" => ["Ctrl+Shift+F"] C(Cmd::Ripple(RippleScope::All));
    "Edit.Ripple.AllAffected" => ["Ctrl+F"] C(Cmd::Ripple(RippleScope::AffectedTracksAndMarkers));
    "TrackView.Select.Loop" => ["Shift+L", "Shift+Q"] X(Extra::Trimmer(TrimmerOp::SelectLoop));
    "TrackView.Select.MarkIn" => ["I", "["] X(Extra::Trimmer(TrimmerOp::MarkIn));
    "TrackView.Select.MarkOut" => ["O", "]"] X(Extra::Trimmer(TrimmerOp::MarkOut));
    "TrackView.Select.SwapCursor" => [] X(Extra::SwapCursor);
    "Edit.Selection.End" => ["Ctrl+Shift+G"] X(Extra::Trimmer(TrimmerOp::MarkOut));
    "Edit.Selection.Length" => ["Shift+G"] Soon;
    "Edit.Selection.Start" => ["Ctrl+G"] X(Extra::Trimmer(TrimmerOp::MarkIn));
    "Edit.SelectivelyPasteAttribute" => [] C(Cmd::PasteAttributesSelective);
    "Options.DoAutoRipple" => ["Ctrl+L"] C(Cmd::AutoRipple);
    "Options.EnableSnapping" => ["F8"] C(Cmd::Snapping);
    "Options.Preferences" => [] C(Cmd::Preferences);
    "Options.QuantizeToFrames" => ["Alt+F8"] C(Cmd::Quantize);
    "Options.SnapToAllEvents" => ["Ctrl+Shift+F8"] C(Cmd::SnapEvents);
    "Options.SnapToFrames" => [] C(Cmd::Grid(GridSpacing::Frames));
    "Options.SnapToGrid" => ["Ctrl+F8"] C(Cmd::SnapGrid);
    "Options.SnapToMarkers" => ["Shift+F8"] C(Cmd::SnapMarkers);
    "Script.Add Timecode to all Media" => [] Soon;
    "Script.Batch Render" => [] Soon;
    "Script.Export Chapters" => [] C(Cmd::RunScript(Script::ExportChapters));
    "Script.Export Closed Captioning as DVD Subtitles" => [] C(Cmd::RunScript(Script::ExportCaptionsDvd));
    "Script.Export Closed Captioning as QuickTime Text" => [] C(Cmd::RunScript(Script::ExportCaptionsQuickTime));
    "Script.Export Closed Captioning as SAMI" => [] C(Cmd::RunScript(Script::ExportCaptionsSami));
    "Script.Export Closed Captioning as SubRip" => [] C(Cmd::RunScript(Script::ExportCaptionsSubRip));
    "Script.Export EDL" => [] C(Cmd::ExportEdl);
    "Script.Export Regions as Subtitles" => [] C(Cmd::RunScript(Script::ExportRegionsAsSubtitles));
    "Script.Group Video and Audio Events" => [] C(Cmd::RunScript(Script::GroupVideoAndAudio));
    "Script.Promote Media Closed Captioning" => [] C(Cmd::RunScript(Script::PromoteMediaCaptions));
    "Script.Promote Media Markers" => [] Soon;
    "Script.Remove Letterboxing" => [] C(Cmd::RunScript(Script::RemoveLetterboxing));
    "Script.Remove Timecode from all Media" => [] Soon;
    "Script.Render Audio Tracks" => [] Soon;
    "Script.Repair and Smooth unwanted Section" => [] Soon;
    "Script.Resize Generated Media" => [] Soon;
    "Script.Stereo to Mono" => [] C(Cmd::RunScript(Script::StereoToMono));
    "TrackView.Shuttle.JogForward" => ["F9"] Soon;
    "TrackView.Shuttle.JogReverse" => ["F3"] Soon;
    "TrackView.Shuttle.StepForward0" => ["F17"] Soon;
    "TrackView.Shuttle.StepForward1" => ["F18"] Soon;
    "TrackView.Shuttle.StepForward2" => ["F19"] Soon;
    "TrackView.Shuttle.StepForward3" => ["F20"] Soon;
    "TrackView.Shuttle.StepForward4" => ["F21"] Soon;
    "TrackView.Shuttle.StepForward5" => ["F22"] Soon;
    "TrackView.Shuttle.StepForward6" => ["F23"] Soon;
    "TrackView.Shuttle.StepForward7" => [] Soon;
    "TrackView.Shuttle.StepForward8" => [] Soon;
    "TrackView.Shuttle.StepForward9" => [] Soon;
    "TrackView.Shuttle.StepReverse0" => ["Ctrl+F17"] Soon;
    "TrackView.Shuttle.StepReverse1" => ["Ctrl+F18"] Soon;
    "TrackView.Shuttle.StepReverse2" => ["Ctrl+F19"] Soon;
    "TrackView.Shuttle.StepReverse3" => ["Ctrl+F20"] Soon;
    "TrackView.Shuttle.StepReverse4" => ["Ctrl+F21"] Soon;
    "TrackView.Shuttle.StepReverse5" => ["Ctrl+F22"] Soon;
    "TrackView.Shuttle.StepReverse6" => ["Ctrl+F23"] Soon;
    "TrackView.Shuttle.StepReverse7" => [] Soon;
    "TrackView.Shuttle.StepReverse8" => [] Soon;
    "TrackView.Shuttle.StepReverse9" => [] Soon;
    "TrackView.Shuttle.Stop" => ["F16"] X(Extra::Stop);
    "Tools.Render" => [] C(Cmd::Render);
    "Tools.Prerender Video" => [] C(Cmd::PrerenderVideo);
    "Tools.Apply Non-Real-Time Event FX" => [] C(Cmd::NonRealTimeFx);
    "Tools.Render to New Track" => [] C(Cmd::RenderToNewTrack);
    "Tools.Welcome Screen" => [] C(Cmd::Welcome);
    "Transport.Fastforward" => [] X(Extra::FastForward);
    "Transport.LoopPlayback" => ["Ctrl+Shift+L", "Q"] C(Cmd::LoopPlayback);
    "Transport.Pause" => ["Play", "Enter"] X(Extra::Trimmer(TrimmerOp::Pause));
    "Transport.Play" => [] X(Extra::Trimmer(TrimmerOp::Play));
    "Transport.PlayFromStart" => ["Shift+Space"] X(Extra::Trimmer(TrimmerOp::PlayFromStart));
    "Transport.Rewind" => [] X(Extra::Rewind);
    "Transport.ScrubLeft" => ["J"] X(Extra::ScrubLeft);
    "Transport.ScrubPause" => ["K"] X(Extra::ScrubPause);
    "Transport.ScrubRight" => ["L"] X(Extra::ScrubRight);
    "Transport.StepBack" => [] X(Extra::StepBack);
    "Transport.StepForward" => [] X(Extra::StepForward);
    "Transport.Stop" => ["Stop"] X(Extra::Trimmer(TrimmerOp::Stop));
    "Transport.PlayToggle" => ["Space"] X(Extra::Trimmer(TrimmerOp::PlayToggle));
    "Transport.TogglePlayFromStart" => ["Shift+Play"] X(Extra::Trimmer(TrimmerOp::PlayFromStart));
    "Edit.Trimmer.CutFromCursor" => ["A"] X(Extra::Trimmer(TrimmerOp::AddFromCursor));
    "Edit.Trimmer.CutToCursor" => ["Shift+A"] X(Extra::Trimmer(TrimmerOp::AddToCursor));
    "Trimmer.AudioHeightLarger" => [] X(Extra::Trimmer(TrimmerOp::Heights(-1)));
    "Trimmer.AutoSaveMarkers" => ["Shift+S"] X(Extra::Trimmer(TrimmerOp::AutoSaveMarkers));
    "Trimmer.BeatDetection" => ["B"] X(Extra::Trimmer(TrimmerOp::BeatDetection));
    "Trimmer.CreateSubclip" => [] X(Extra::Trimmer(TrimmerOp::CreateSubclip));
    "Trimmer.EditCopyInAudioEditor" => [] X(Extra::Trimmer(TrimmerOp::OpenCopyInAudioEditor));
    "Trimmer.EqualHeight" => [] X(Extra::Trimmer(TrimmerOp::Heights(0)));
    "Edit.Trimmer.FitToFill" => [] X(Extra::Trimmer(TrimmerOp::FitToFill));
    "Trimmer.History.Delete" => ["Ctrl+H"] X(Extra::Trimmer(TrimmerOp::RemoveFromHistory));
    "Trimmer.History.Next" => [] X(Extra::Trimmer(TrimmerOp::NextMedia(1)));
    "Trimmer.History.Prev" => [] X(Extra::Trimmer(TrimmerOp::NextMedia(-1)));
    "Trimmer.History.PurgeUnused" => ["Ctrl+Shift+H"] X(Extra::Trimmer(TrimmerOp::PurgeHistory));
    "Trimmer.History.SortAscending" => ["H"] X(Extra::Trimmer(TrimmerOp::SortHistory(false)));
    "Trimmer.History.SortDecending" => ["Shift+H"] X(Extra::Trimmer(TrimmerOp::SortHistory(true)));
    "Trimmer.InsertMarker" => ["M"] X(Extra::Trimmer(TrimmerOp::InsertMarker));
    "Trimmer.InsertRegion" => ["R"] X(Extra::Trimmer(TrimmerOp::InsertRegion));
    "Trimmer.OpenInAudioEditor" => ["Ctrl+E"] X(Extra::Trimmer(TrimmerOp::OpenInAudioEditor));
    "Edit.Trimmer.Overwrite" => [] X(Extra::Trimmer(TrimmerOp::Overwrite));
    "TrackView.Refresh" => ["F5"] X(Extra::Refresh);
    "Trimmer.SaveMarkers" => ["S"] X(Extra::Trimmer(TrimmerOp::SaveMarkers));
    "Trimmer.Select.AudioOnly" => [] X(Extra::Trimmer(TrimmerOp::Select(Streams::AudioOnly)));
    "Trimmer.Select.VideoAndAudio" => [] X(Extra::Trimmer(TrimmerOp::Select(Streams::VideoAndAudio)));
    "Trimmer.Select.VideoOnly" => [] X(Extra::Trimmer(TrimmerOp::Select(Streams::VideoOnly)));
    "Trimmer.SelectNextTrack" => ["Tab"] X(Extra::Trimmer(TrimmerOp::CycleStreams(1)));
    "Trimmer.SelectPrevTrack" => ["Shift+Tab"] X(Extra::Trimmer(TrimmerOp::CycleStreams(-1)));
    "Trimmer.SyncSelectionFromCursor" => ["T"] X(Extra::Trimmer(TrimmerOp::SyncSelection { up_to: false }));
    "Trimmer.SyncSelectionUpToCursor" => ["Shift+T"] X(Extra::Trimmer(TrimmerOp::SyncSelection { up_to: true }));
    "Trimmer.ToggleVideoPreview" => ["Ctrl+P"] X(Extra::Trimmer(TrimmerOp::TogglePreviewWindow));
    "Trimmer.VideoHeightLarger" => [] X(Extra::Trimmer(TrimmerOp::Heights(1)));
    "Trimmer.ZoomSelection" => [] X(Extra::Trimmer(TrimmerOp::ZoomSelection));
    "View.Hardware Detection" => [] Soon;
    "View.Motion Tracking" => [] C(Cmd::MotionTracking);
    "View.RebuildAudioPeaks" => ["Shift+F5"] C(Cmd::RebuildPeaks);
    "View.Explorer" => [] C(Cmd::Window(Tab::Explorer));
    "TrackView.Zoom.LessTrackHeight" => ["Ctrl+Shift+Down"] X(Extra::Trimmer(TrimmerOp::Heights(-1)));
    "TrackView.Zoom.MoreTrackHeight" => ["Ctrl+Shift+Up"] X(Extra::Trimmer(TrimmerOp::Heights(1)));
};

/// The Mixer context (VEGAS's Mixer window: BananaV's Master Bus).
#[rustfmt::skip]
const MIXER: &[(&str, &[&str], Action)] = cmds! {
    "Insert.AudioBus" => [] C(Cmd::InsertAudioBus);
    "AddFX" => [] Soon;
    "Configure" => [] Soon;
    "Tools.Audio.RemoveFX" => [] Soon;
    "Mixer.Dim" => [] C(Cmd::Dim);
    "Mixer.Downmix" => [] C(Cmd::Downmix);
    "Edit.SelectivelyPasteAttribute" => [] C(Cmd::PasteAttributesSelective);
    "Script.Add Timecode to all Media" => [] Soon;
    "Script.Batch Render" => [] Soon;
    "Script.Export Chapters" => [] C(Cmd::RunScript(Script::ExportChapters));
    "Script.Export Closed Captioning as DVD Subtitles" => [] C(Cmd::RunScript(Script::ExportCaptionsDvd));
    "Script.Export Closed Captioning as QuickTime Text" => [] C(Cmd::RunScript(Script::ExportCaptionsQuickTime));
    "Script.Export Closed Captioning as SAMI" => [] C(Cmd::RunScript(Script::ExportCaptionsSami));
    "Script.Export Closed Captioning as SubRip" => [] C(Cmd::RunScript(Script::ExportCaptionsSubRip));
    "Script.Export EDL" => [] C(Cmd::ExportEdl);
    "Script.Export Regions as Subtitles" => [] C(Cmd::RunScript(Script::ExportRegionsAsSubtitles));
    "Script.Group Video and Audio Events" => [] C(Cmd::RunScript(Script::GroupVideoAndAudio));
    "Script.Promote Media Closed Captioning" => [] C(Cmd::RunScript(Script::PromoteMediaCaptions));
    "Script.Promote Media Markers" => [] Soon;
    "Script.Remove Letterboxing" => [] C(Cmd::RunScript(Script::RemoveLetterboxing));
    "Script.Remove Timecode from all Media" => [] Soon;
    "Script.Render Audio Tracks" => [] Soon;
    "Script.Repair and Smooth unwanted Section" => [] Soon;
    "Script.Resize Generated Media" => [] Soon;
    "Script.Stereo to Mono" => [] C(Cmd::RunScript(Script::StereoToMono));
    "ShowFX" => [] Soon;
    "View.MixerPreviewFader" => [] C(Cmd::PreviewFader);
    "TrackView.Track.ToggleMute" => ["Z"] X(Extra::TrackMute { exclusive: false });
    "TrackView.Track.ToggleMuteExclusive" => ["Shift+Z"] X(Extra::TrackMute { exclusive: true });
    "TrackView.Track.ToggleNonAutomatedMute" => [] Soon;
    "TrackView.Track.ToggleSolo" => ["X"] X(Extra::TrackSolo { exclusive: false });
    "TrackView.Track.ToggleSoloExclusive" => ["Shift+X"] X(Extra::TrackSolo { exclusive: true });
    "Tools.Render" => [] C(Cmd::Render);
    "Tools.Prerender Video" => [] C(Cmd::PrerenderVideo);
    "Tools.Apply Non-Real-Time Event FX" => [] C(Cmd::NonRealTimeFx);
    "Tools.Render to New Track" => [] C(Cmd::RenderToNewTrack);
    "Tools.Welcome Screen" => [] C(Cmd::Welcome);
    "View.Hardware Detection" => [] Soon;
    "View.Motion Tracking" => [] C(Cmd::MotionTracking);
    "View.Explorer" => [] C(Cmd::Window(Tab::Explorer));
};

/// The EditDetails context (the Edit Details window).
#[rustfmt::skip]
const EDIT_DETAILS: &[(&str, &[&str], Action)] = cmds! {
    "Edit.Take.ChooseActive" => ["Shift+T"] Soon;
    "Details.Copy" => ["Ctrl+C"] X(Extra::Details(DetailsOp::Copy));
    "Details.Cut" => ["Ctrl+X"] X(Extra::Details(DetailsOp::Cut));
    "Details.Delete" => ["Delete"] X(Extra::Details(DetailsOp::Delete));
    "Details.Edit" => ["F2"] X(Extra::Details(DetailsOp::Edit));
    "Details.Paste" => ["Ctrl+V"] X(Extra::Details(DetailsOp::Paste));
    "Edit.SelectivelyPasteAttribute" => [] C(Cmd::PasteAttributesSelective);
    "TrackView.Event.OpenInTrimmer" => [] X(Extra::OpenInTrimmer);
    "TrackView.Event.SelectInMediaPool" => [] X(Extra::SelectInMediaPool);
    "Script.Add Timecode to all Media" => [] Soon;
    "Script.Batch Render" => [] Soon;
    "Script.Export Chapters" => [] C(Cmd::RunScript(Script::ExportChapters));
    "Script.Export Closed Captioning as DVD Subtitles" => [] C(Cmd::RunScript(Script::ExportCaptionsDvd));
    "Script.Export Closed Captioning as QuickTime Text" => [] C(Cmd::RunScript(Script::ExportCaptionsQuickTime));
    "Script.Export Closed Captioning as SAMI" => [] C(Cmd::RunScript(Script::ExportCaptionsSami));
    "Script.Export Closed Captioning as SubRip" => [] C(Cmd::RunScript(Script::ExportCaptionsSubRip));
    "Script.Export EDL" => [] C(Cmd::ExportEdl);
    "Script.Export Regions as Subtitles" => [] C(Cmd::RunScript(Script::ExportRegionsAsSubtitles));
    "Script.Group Video and Audio Events" => [] C(Cmd::RunScript(Script::GroupVideoAndAudio));
    "Script.Promote Media Closed Captioning" => [] C(Cmd::RunScript(Script::PromoteMediaCaptions));
    "Script.Promote Media Markers" => [] Soon;
    "Script.Remove Letterboxing" => [] C(Cmd::RunScript(Script::RemoveLetterboxing));
    "Script.Remove Timecode from all Media" => [] Soon;
    "Script.Render Audio Tracks" => [] Soon;
    "Script.Repair and Smooth unwanted Section" => [] Soon;
    "Script.Resize Generated Media" => [] Soon;
    "Script.Stereo to Mono" => [] C(Cmd::RunScript(Script::StereoToMono));
    "Tools.Render" => [] C(Cmd::Render);
    "Tools.Prerender Video" => [] C(Cmd::PrerenderVideo);
    "Tools.Apply Non-Real-Time Event FX" => [] C(Cmd::NonRealTimeFx);
    "Tools.Render to New Track" => [] C(Cmd::RenderToNewTrack);
    "Tools.Welcome Screen" => [] C(Cmd::Welcome);
    "View.Hardware Detection" => [] Soon;
    "View.Motion Tracking" => [] C(Cmd::MotionTracking);
    "View.Explorer" => [] C(Cmd::Window(Tab::Explorer));
};

/// The TrackMotion context (the Track Motion window, while it has the
/// keys): the 3D layouts and the toolbar's toggles.
#[rustfmt::skip]
const TRACK_MOTION: &[(&str, &[&str], Action)] = cmds! {
    "3DLayout1" => ["1"] X(Extra::TrackMotion(TrackMotionKey::Layout(1)));
    "3DLayout2" => ["2"] X(Extra::TrackMotion(TrackMotionKey::Layout(2)));
    "3DLayout3" => ["3"] X(Extra::TrackMotion(TrackMotionKey::Layout(3)));
    "3DLayout4" => ["4"] X(Extra::TrackMotion(TrackMotionKey::Layout(4)));
    "3DLayout5" => ["5"] X(Extra::TrackMotion(TrackMotionKey::Layout(5)));
    "3DLayout6" => ["6"] X(Extra::TrackMotion(TrackMotionKey::Layout(6)));
    "Edit.SelectivelyPasteAttribute" => [] C(Cmd::PasteAttributesSelective);
    "Script.Add Timecode to all Media" => [] Soon;
    "Script.Batch Render" => [] Soon;
    "Script.Export Chapters" => [] C(Cmd::RunScript(Script::ExportChapters));
    "Script.Export Closed Captioning as DVD Subtitles" => [] C(Cmd::RunScript(Script::ExportCaptionsDvd));
    "Script.Export Closed Captioning as QuickTime Text" => [] C(Cmd::RunScript(Script::ExportCaptionsQuickTime));
    "Script.Export Closed Captioning as SAMI" => [] C(Cmd::RunScript(Script::ExportCaptionsSami));
    "Script.Export Closed Captioning as SubRip" => [] C(Cmd::RunScript(Script::ExportCaptionsSubRip));
    "Script.Export EDL" => [] C(Cmd::ExportEdl);
    "Script.Export Regions as Subtitles" => [] C(Cmd::RunScript(Script::ExportRegionsAsSubtitles));
    "Script.Group Video and Audio Events" => [] C(Cmd::RunScript(Script::GroupVideoAndAudio));
    "Script.Promote Media Closed Captioning" => [] C(Cmd::RunScript(Script::PromoteMediaCaptions));
    "Script.Promote Media Markers" => [] Soon;
    "Script.Remove Letterboxing" => [] C(Cmd::RunScript(Script::RemoveLetterboxing));
    "Script.Remove Timecode from all Media" => [] Soon;
    "Script.Render Audio Tracks" => [] Soon;
    "Script.Repair and Smooth unwanted Section" => [] Soon;
    "Script.Resize Generated Media" => [] Soon;
    "Script.Stereo to Mono" => [] C(Cmd::RunScript(Script::StereoToMono));
    "Toggle.Constrain.Aspect" => ["A"] X(Extra::TrackMotion(TrackMotionKey::Aspect));
    "Toggle.Constrain.Center" => ["C"] X(Extra::TrackMotion(TrackMotionKey::Center));
    "Toggle.Constrain.ScaleX" => ["Shift+X"] X(Extra::TrackMotion(TrackMotionKey::ScaleX));
    "Toggle.Constrain.ScaleY" => ["Shift+Y"] X(Extra::TrackMotion(TrackMotionKey::ScaleY));
    "Toggle.Constrain.ScaleZ" => ["Shift+Z"] X(Extra::TrackMotion(TrackMotionKey::ScaleZ));
    "Toggle.Constrain.X" => ["X"] X(Extra::TrackMotion(TrackMotionKey::MoveX));
    "Toggle.Constrain.Y" => ["Y"] X(Extra::TrackMotion(TrackMotionKey::MoveY));
    "Toggle.Constrain.Z" => ["Z"] X(Extra::TrackMotion(TrackMotionKey::MoveZ));
    "Toggle.EnableRotation" => ["Shift+F8"] X(Extra::TrackMotion(TrackMotionKey::Rotation));
    "Toggle.EnableSnap" => ["F8"] X(Extra::TrackMotion(TrackMotionKey::Snap));
    "Toggle.ObjectSpace" => ["O"] X(Extra::TrackMotion(TrackMotionKey::ObjectSpace));
    "Tools.Render" => [] C(Cmd::Render);
    "Tools.Prerender Video" => [] C(Cmd::PrerenderVideo);
    "Tools.Apply Non-Real-Time Event FX" => [] C(Cmd::NonRealTimeFx);
    "Tools.Render to New Track" => [] C(Cmd::RenderToNewTrack);
    "Tools.Welcome Screen" => [] C(Cmd::Welcome);
    "View.Hardware Detection" => [] Soon;
    "View.Motion Tracking" => [] C(Cmd::MotionTracking);
    "View.Explorer" => [] C(Cmd::Window(Tab::Explorer));
};

/// The MixingConsole context (the Mixing Console window): channel
/// width and which channels and regions show act on the console.
#[rustfmt::skip]
const MIXING_CONSOLE: &[(&str, &[&str], Action)] = cmds! {
    "Edit.Copy" => ["Ctrl+C"] Soon;
    "Edit.Cut" => ["Ctrl+X"] Soon;
    "Edit.Delete" => [] X(Extra::Console(ConsoleOp::Delete));
    "Edit.Paste" => ["Ctrl+V"] Soon;
    "Edit.Rename" => ["F2"] X(Extra::Console(ConsoleOp::Rename));
    "Edit.Select.All" => [] Soon;
    "Edit.SelectivelyPasteAttribute" => [] C(Cmd::PasteAttributesSelective);
    "Insert.AssignableFX" => [] C(Cmd::InsertAssignableFx);
    "Insert.AudioTrack" => ["Ctrl+Q"] C(Cmd::InsertAudioTrack);
    "Insert.Bus" => [] C(Cmd::InsertAudioBus);
    "Insert.InputBus" => ["Ctrl+Shift+I"] C(Cmd::InsertInputBus);
    "MixingConsole.ChannelWidth.Default" => ["D", "Ctrl+`"] X(Extra::Console(ConsoleOp::Width(ChannelWidth::Default)));
    "MixingConsole.ChannelWidth.Narrow" => ["N", "`"] X(Extra::Console(ConsoleOp::Width(ChannelWidth::Narrow)));
    "MixingConsole.ChannelWidth.Wide" => ["W", "Shift+`"] X(Extra::Console(ConsoleOp::Width(ChannelWidth::Wide)));
    "MixingConsole.Dim" => [] C(Cmd::Dim);
    "MixingConsole.Downmix" => [] C(Cmd::Downmix);
    "MixingConsole.HideChannel" => ["Shift+H"] X(Extra::Console(ConsoleOp::HideChannel));
    "MixingConsole.Show.ChannelList" => ["Shift+C"] X(Extra::Console(ConsoleOp::ToggleChannelList));
    "MixingConsole.Show.Channels.All" => ["Q"] X(Extra::Console(ConsoleOp::ShowAllChannels));
    "MixingConsole.Show.Channels.AudioBusses" => ["U"] X(Extra::Console(ConsoleOp::ToggleBusses));
    "MixingConsole.Show.Channels.AudioTracks" => ["A"] X(Extra::Console(ConsoleOp::ToggleTracks));
    "MixingConsole.Show.Channels.FXBusses" => ["E"] X(Extra::Console(ConsoleOp::ToggleFx));
    "MixingConsole.Show.Channels.InputBusses" => [] X(Extra::Console(ConsoleOp::ToggleInputs));
    "MixingConsole.Show.Channels.MasterBus" => ["T"] X(Extra::Console(ConsoleOp::ToggleMaster));
    "MixingConsole.Show.Channels.PreviewBus" => ["P"] Soon;
    "MixingConsole.Show.FaderTicks" => ["Shift+T"] X(Extra::Console(ConsoleOp::ToggleTicks));
    "MixingConsole.Show.Region.Labels" => ["Shift+L"] X(Extra::Console(ConsoleOp::ToggleLabels));
    "MixingConsole.Show.RegionControls" => ["Shift+R"] X(Extra::Console(ConsoleOp::ToggleViewControls));
    "MixingConsole.Show.Regions.All" => [] X(Extra::Console(ConsoleOp::ShowAllRegions));
    "MixingConsole.Show.Regions.Faders" => ["F"] X(Extra::Console(ConsoleOp::ToggleRegion(Region::Faders)));
    "MixingConsole.Show.Regions.Inserts" => ["I"] X(Extra::Console(ConsoleOp::ToggleRegion(Region::InsertFx)));
    "MixingConsole.Show.Regions.IO" => ["H"] X(Extra::Console(ConsoleOp::ToggleRegion(Region::Io)));
    "MixingConsole.Show.Regions.Meters" => ["M"] X(Extra::Console(ConsoleOp::ToggleRegion(Region::Meters)));
    "MixingConsole.Show.Regions.Sends" => ["S"] X(Extra::Console(ConsoleOp::ToggleRegion(Region::Sends)));
    "MixingConsole.Show.Regions.VUMeters" => [] X(Extra::Console(ConsoleOp::ToggleRegion(Region::VuMeters)));
    "Script.Add Timecode to all Media" => [] Soon;
    "Script.Batch Render" => [] Soon;
    "Script.Export Chapters" => [] C(Cmd::RunScript(Script::ExportChapters));
    "Script.Export Closed Captioning as DVD Subtitles" => [] C(Cmd::RunScript(Script::ExportCaptionsDvd));
    "Script.Export Closed Captioning as QuickTime Text" => [] C(Cmd::RunScript(Script::ExportCaptionsQuickTime));
    "Script.Export Closed Captioning as SAMI" => [] C(Cmd::RunScript(Script::ExportCaptionsSami));
    "Script.Export Closed Captioning as SubRip" => [] C(Cmd::RunScript(Script::ExportCaptionsSubRip));
    "Script.Export EDL" => [] C(Cmd::ExportEdl);
    "Script.Export Regions as Subtitles" => [] C(Cmd::RunScript(Script::ExportRegionsAsSubtitles));
    "Script.Group Video and Audio Events" => [] C(Cmd::RunScript(Script::GroupVideoAndAudio));
    "Script.Promote Media Closed Captioning" => [] C(Cmd::RunScript(Script::PromoteMediaCaptions));
    "Script.Promote Media Markers" => [] Soon;
    "Script.Remove Letterboxing" => [] C(Cmd::RunScript(Script::RemoveLetterboxing));
    "Script.Remove Timecode from all Media" => [] Soon;
    "Script.Render Audio Tracks" => [] Soon;
    "Script.Repair and Smooth unwanted Section" => [] Soon;
    "Script.Resize Generated Media" => [] Soon;
    "Script.Stereo to Mono" => [] C(Cmd::RunScript(Script::StereoToMono));
    "Tools.Render" => [] C(Cmd::Render);
    "Tools.Prerender Video" => [] C(Cmd::PrerenderVideo);
    "Tools.Apply Non-Real-Time Event FX" => [] C(Cmd::NonRealTimeFx);
    "Tools.Render to New Track" => [] C(Cmd::RenderToNewTrack);
    "Tools.Welcome Screen" => [] C(Cmd::Welcome);
    "View.Hardware Detection" => [] Soon;
    "View.Motion Tracking" => [] C(Cmd::MotionTracking);
    "View.Explorer" => [] C(Cmd::Window(Tab::Explorer));
};

/// Default keys of the Global context. VEGAS lists the same commands in
/// Global as in Explorer ([`EXPLORER`]); these are the ones with keys.
#[rustfmt::skip]
const GLOBAL_KEYS: &[(&str, &[&str])] = &[
    ("Edit.SmartSplit", &["Alt+S"]),
    ("Edit.Trim.End", &["Alt+]"]),
    ("Edit.Trim.Start", &["Alt+["]),
    ("File.Close", &["Ctrl+F4"]),
    ("File.Exit", &["Alt+F4"]),
    ("File.New", &["Ctrl+N"]),
    ("File.New.Quick", &["Ctrl+Shift+N"]),
    ("File.Open", &["Ctrl+O"]),
    ("File.Properties", &["Alt+Enter"]),
    ("File.Save", &["Ctrl+S"]),
    ("File.SaveAs", &["Ctrl+Shift+S"]),
    ("File.SaveInc", &["Ctrl+Alt+S"]),
    ("Help.Contents", &["F1"]),
    ("Help.WhatsThis", &["Shift+F1"]),
    ("Mixer.Dim", &["Ctrl+Shift+F12"]),
    ("Preview.BypassZoom", &["Ctrl+Alt+Shift+B"]),
    ("Preview.DefaultZoom", &["Ctrl+Num 1"]),
    ("Preview.MinusZoom", &["Ctrl+Num -"]),
    ("Preview.PlusZoom", &["Ctrl+Num +"]),
    ("Sync.GenerateMIDI", &["Shift+F7"]),
    ("Sync.GenerateMTC", &["F7"]),
    ("Sync.ListenToMTC", &["Ctrl+F7"]),
    ("Tools.TextToSpeech", &["Ctrl+Shift+P"]),
    ("TrackView.Event.OpenAsNestedTimeline", &["Alt+N"]),
    ("TrackView.Timeline.CreateNestedTimeline", &["Alt+C"]),
    ("TrackView.Timeline.OpenParentTimeline", &["Alt+P"]),
    ("Transport.Pause", &["Ctrl+F12"]),
    ("Transport.PlayFromStart", &["Shift+F12"]),
    ("Transport.PlayToggle", &["Ctrl+Space", "F12"]),
    ("VideoPreview.ExternalMonitorToggle", &["Alt+Shift+4"]),
    ("View.ColorGrading", &["Alt+G"]),
    ("View.DeviceExplorer", &["Ctrl+Alt+6"]),
    ("View.EditDetails", &["Ctrl+Alt+4"]),
    ("View.EventMediaMarkers", &["Ctrl+Shift+K"]),
    ("View.FocusToTrackView", &["Alt+0"]),
    ("View.HubExplorer", &["Ctrl+Alt+9"]),
    ("View.LoudnessMeters", &["Ctrl+Alt+7"]),
    ("View.MediaGenerators", &["Alt+9"]),
    ("View.MinimizeAll", &["Ctrl+Alt+Shift+F11"]),
    ("View.MinimizeAllDocks", &["F11"]),
    ("View.MinimizeDock", &["Alt+F11"]),
    ("View.MinimizeLabels", &["Ctrl+F11"]),
    ("View.MinimizeLowerDock", &["Ctrl+Alt+F11"]),
    ("View.MinimizeRightDock", &["Ctrl+Shift+F11"]),
    ("View.MixingConsole", &["Ctrl+Alt+5"]),
    ("View.MasterBus", &["Alt+3"]),
    ("View.MotionTrackGuiCommand", &["Alt+M"]),
    ("View.Next", &["Ctrl+Tab", "F6"]),
    ("View.Plug-InManager", &["Ctrl+Alt+1"]),
    ("View.Prev", &["Ctrl+Shift+Tab", "Shift+F6"]),
    ("View.ProjectMedia", &["Alt+5"]),
    ("View.ProjectNotes", &["Alt+6"]),
    ("View.ScriptEdit", &["Ctrl+Alt+0"]),
    ("View.ShowActiveTakeInformation", &["Ctrl+Shift+I"]),
    ("View.ShowEnvelopes", &["Ctrl+Shift+E"]),
    ("View.ShowEventButtons", &["Ctrl+Shift+C"]),
    ("Event Headers", &["Ctrl+Shift+M"]),
    ("View.ShowFadeLengths", &["Ctrl+Shift+T"]),
    ("View.ShowVideoEventEdgeFramesDuringEdits", &["Ctrl+Shift+O"]),
    ("View.ShowWaveformsAndFramesInEvents", &["Ctrl+Shift+W"]),
    ("View.SurroundPanner", &["Ctrl+Alt+3"]),
    ("View.ToggleDock", &["Alt+`"]),
    ("View.ToggleLabels", &["Alt+Shift+`"]),
    ("View.ToggleMinimizeAll", &["Ctrl+Alt+`"]),
    ("View.Transitions", &["Alt+7"]),
    ("View.Trimmer", &["Alt+2"]),
    ("View.UnifiedExplorer", &["Alt+1"]),
    ("View.Video Preview", &["Alt+4"]),
    ("View.VideoFX", &["Alt+8"]),
    ("View.VideoScopes", &["Ctrl+Alt+2"]),
    ("View.WindowLayouts.Load.AudioMixing", &["Alt+D, A"]),
    ("View.WindowLayouts.Load.ColorCorrection", &["Alt+D, C"]),
    ("View.WindowLayouts.Load.Default", &["Alt+D, D"]),
    ("View.WindowLayouts.Load.SourceWindowLayout", &["Alt+D, S"]),
    ("View.WindowLayouts.Load.TimelineTopLayout", &["Alt+D, T"]),
    ("View.WindowLayouts.Load.UserLayout0", &["Alt+D, 0"]),
    ("View.WindowLayouts.Load.UserLayout1", &["Alt+D, 1"]),
    ("View.WindowLayouts.Load.UserLayout2", &["Alt+D, 2"]),
    ("View.WindowLayouts.Load.UserLayout3", &["Alt+D, 3"]),
    ("View.WindowLayouts.Load.UserLayout4", &["Alt+D, 4"]),
    ("View.WindowLayouts.Load.UserLayout5", &["Alt+D, 5"]),
    ("View.WindowLayouts.Load.UserLayout6", &["Alt+D, 6"]),
    ("View.WindowLayouts.Load.UserLayout7", &["Alt+D, 7"]),
    ("View.WindowLayouts.Load.UserLayout8", &["Alt+D, 8"]),
    ("View.WindowLayouts.Load.UserLayout9", &["Alt+D, 9"]),
    ("View.WindowLayouts.Save.UserLayout0", &["Ctrl+Alt+D, 0"]),
    ("View.WindowLayouts.Save.UserLayout1", &["Ctrl+Alt+D, 1"]),
    ("View.WindowLayouts.Save.UserLayout2", &["Ctrl+Alt+D, 2"]),
    ("View.WindowLayouts.Save.UserLayout3", &["Ctrl+Alt+D, 3"]),
    ("View.WindowLayouts.Save.UserLayout4", &["Ctrl+Alt+D, 4"]),
    ("View.WindowLayouts.Save.UserLayout5", &["Ctrl+Alt+D, 5"]),
    ("View.WindowLayouts.Save.UserLayout6", &["Ctrl+Alt+D, 6"]),
    ("View.WindowLayouts.Save.UserLayout7", &["Ctrl+Alt+D, 7"]),
    ("View.WindowLayouts.Save.UserLayout8", &["Ctrl+Alt+D, 8"]),
    ("View.WindowLayouts.Save.UserLayout9", &["Ctrl+Alt+D, 9"]),
];

/// Default keys of the TrackList context (the track headers, focused by
/// clicking them). It lists the same commands as TrackView; like in VEGAS,
/// only the track commands have keys — the track envelopes' V and P (and
/// Shift's) among them: a track picked by its header takes them.
#[rustfmt::skip]
const TRACK_LIST: &[(&str, &[&str])] = &[
    ("Insert.Envelope.TrackPan", &["Shift+P"]),
    ("Insert.Envelope.TrackVolume", &["Shift+V"]),
    ("TrackView.Track.CursorDown", &["Down"]),
    ("TrackView.Track.CursorEnd", &["Ctrl+PageDown", "End"]),
    ("TrackView.Track.CursorHome", &["Ctrl+PageUp", "Home"]),
    ("TrackView.Track.CursorPageDown", &["PageDown"]),
    ("TrackView.Track.CursorPageUp", &["PageUp"]),
    ("TrackView.Track.CursorTo0", &["0"]),
    ("TrackView.Track.CursorTo1", &["1"]),
    ("TrackView.Track.CursorTo2", &["2"]),
    ("TrackView.Track.CursorTo3", &["3"]),
    ("TrackView.Track.CursorTo4", &["4"]),
    ("TrackView.Track.CursorTo5", &["5"]),
    ("TrackView.Track.CursorTo6", &["6"]),
    ("TrackView.Track.CursorTo7", &["7"]),
    ("TrackView.Track.CursorTo8", &["8"]),
    ("TrackView.Track.CursorTo9", &["9"]),
    ("TrackView.Track.CursorUp", &["Up"]),
    ("TrackView.Track.PanLeft", &["Ctrl+Shift+Left"]),
    ("TrackView.Track.PanNum1", &["Num 1"]),
    ("TrackView.Track.PanNum2", &["Num 2"]),
    ("TrackView.Track.PanNum3", &["Num 3"]),
    ("TrackView.Track.PanNum4", &["Num 4"]),
    ("TrackView.Track.PanNum5", &["Num 5"]),
    ("TrackView.Track.PanNum6", &["Num 6"]),
    ("TrackView.Track.PanNum7", &["Num 7"]),
    ("TrackView.Track.PanNum8", &["Num 8"]),
    ("TrackView.Track.PanNum9", &["Num 9"]),
    ("TrackView.Track.PanRight", &["Ctrl+Shift+Right"]),
    ("TrackView.Track.Rename", &["F2"]),
    ("TrackView.Track.VolumeDown", &["Ctrl+Left"]),
    ("TrackView.Track.VolumeUp", &["Ctrl+Right"]),
    ("TrackView.Zoom.LessTrackHeight", &["Ctrl+Shift+Down"]),
    ("TrackView.Zoom.MoreTrackHeight", &["Ctrl+Shift+Up"]),
    ("View.Envelope.TrackPan", &["P"]),
    ("View.Envelope.TrackVolume", &["V"]),
];

fn parse_keys(keys: &[&str], id: &str) -> Vec<Binding> {
    keys.iter()
        .map(|k| Binding::parse(k).unwrap_or_else(|| panic!("bad key {k:?} in {id}")))
        .collect()
}

/// Name of a menu command in the Global context ("File.Save", …) until the
/// VEGAS list of that tab is in.
fn global_id(cmd: Cmd) -> String {
    let leaf = format!("{cmd:?}").replace('(', ".").replace([')', ' '], "");
    let group = match cmd {
        Cmd::New
        | Cmd::Welcome
        | Cmd::Open
        | Cmd::Close
        | Cmd::Save
        | Cmd::SaveAs
        | Cmd::SaveIncremental
        | Cmd::Render
        | Cmd::ImportMedia
        | Cmd::ImportImageSequence
        | Cmd::ImportProjectMedia
        | Cmd::ImportEdl
        | Cmd::ImportOtio
        | Cmd::ImportXml
        | Cmd::ImportFcpxml
        | Cmd::ImportClosedCaptioning
        | Cmd::ImportBwf
        | Cmd::ExportProject
        | Cmd::ExportArchive
        | Cmd::CopyMediaWithProject
        | Cmd::ExportXml
        | Cmd::ExportFcpxml
        | Cmd::ExportEdl
        | Cmd::ExportOtio
        | Cmd::Properties
        | Cmd::Exit => "File",
        Cmd::Toolbar
        | Cmd::StatusBar
        | Cmd::OrganizeLayouts
        | Cmd::SaveLayout
        | Cmd::SaveLayoutAs
        | Cmd::ReloadLayout
        | Cmd::Layout(_)
        | Cmd::LayoutSlot(_)
        | Cmd::FocusTimeline
        | Cmd::Window(_)
        | Cmd::ActiveTakeInfo
        | Cmd::Waveforms
        | Cmd::EventButtons
        | Cmd::FadeDurations
        | Cmd::EdgeFrames
        | Cmd::EventHeaders
        | Cmd::EventDuration
        | Cmd::EventMediaMarkers
        | Cmd::EventMediaMarkerLabels
        | Cmd::ShowEnvelopes
        | Cmd::MinimizeTracks
        | Cmd::AudioBusTracks
        | Cmd::VideoBusTrack
        | Cmd::RebuildPeaks
        | Cmd::ShowEnvelopeType(_) => "View",
        Cmd::AudioEnvelope(_)
        | Cmd::VideoMuteEnvelope
        | Cmd::InsertAudioTrack
        | Cmd::InsertVideoTrack
        | Cmd::EmptyEvent
        | Cmd::Slideshow
        | Cmd::InsertTime
        | Cmd::InsertMarker
        | Cmd::InsertRegion
        | Cmd::AddMissingStream
        | Cmd::InsertTextMedia
        | Cmd::SubtitlesFromFile
        | Cmd::SubtitlesFromRegions
        | Cmd::InsertAdjustmentEvent
        | Cmd::InsertAdjustmentTrack => "Insert",
        Cmd::OpenInAudioEditor
        | Cmd::OpenCopyInAudioEditor
        | Cmd::EditSourceProject
        | Cmd::CreateNestedTimeline
        | Cmd::OpenNestedTimeline
        | Cmd::OpenParentTimeline
        | Cmd::AudioEventFx
        | Cmd::NonRealTimeFx
        | Cmd::AudioTrackFx
        | Cmd::MediaFx
        | Cmd::PanCropEvent
        | Cmd::VideoEventFx
        | Cmd::VideoTrackFx
        | Cmd::VideoOutputFx
        | Cmd::LoudnessLog
        | Cmd::AdvancedSave
        | Cmd::CleanProjectMedia
        | Cmd::RunScript(_) => "Tools",
        Cmd::Quantize
        | Cmd::ShowUnquantized
        | Cmd::Snapping
        | Cmd::SnapGrid
        | Cmd::SnapMarkers
        | Cmd::SnapEvents
        | Cmd::Grid(_)
        | Cmd::Ruler(_)
        | Cmd::SetTimeAtCursor
        | Cmd::SetProjectTime
        | Cmd::SetProjectTempo
        | Cmd::AutoCrossfade
        | Cmd::QuickFades
        | Cmd::AutoRipple
        | Cmd::RippleMode(_)
        | Cmd::IgnoreGrouping
        | Cmd::LockEnvelopes
        | Cmd::BypassAudioFx
        | Cmd::MuteAllAudio
        | Cmd::MuteAllVideo
        | Cmd::ExportPreferences
        | Cmd::ImportPreferences
        | Cmd::Preferences
        | Cmd::CustomizeKeyboard
        | Cmd::CustomizeToolbar
        | Cmd::CustomizeTimelineToolbar => "Options",
        Cmd::HelpContents | Cmd::KeyboardShortcuts | Cmd::About => "Help",
        _ => "Edit",
    };
    format!("{group}.{leaf}")
}

/// Every command of every context, built once.
pub fn registry() -> &'static [KeyCommand] {
    static REG: OnceLock<Vec<KeyCommand>> = OnceLock::new();
    REG.get_or_init(|| {
        let mut out: Vec<KeyCommand> = TRACK_VIEW
            .iter()
            .map(|(id, keys, action)| KeyCommand {
                id: (*id).to_string(),
                context: Context::TrackView,
                action: *action,
                defaults: parse_keys(keys, id),
            })
            .collect();
        for (id, _) in TRACK_LIST {
            assert!(
                TRACK_VIEW.iter().any(|(t, _, _)| t == id),
                "TrackList key for unknown command {id}"
            );
        }
        // TrackList: the same commands, with the keys of the track headers.
        out.extend(TRACK_VIEW.iter().map(|(id, _, action)| {
            KeyCommand {
                id: (*id).to_string(),
                context: Context::TrackList,
                action: *action,
                defaults: TRACK_LIST
                    .iter()
                    .find(|(t, _)| t == id)
                    .map_or_else(Vec::new, |(_, keys)| parse_keys(keys, id)),
            }
        }));
        out.extend(EXPLORER.iter().map(|(id, keys, action)| KeyCommand {
            id: (*id).to_string(),
            context: Context::Explorer,
            action: *action,
            defaults: parse_keys(keys, id),
        }));
        for (context, table) in [
            (Context::Trimmer, TRIMMER),
            (Context::MixingDesk, MIXER),
            (Context::EditDetails, EDIT_DETAILS),
            (Context::TrackMotion, TRACK_MOTION),
            (Context::MixingConsole, MIXING_CONSOLE),
        ] {
            out.extend(table.iter().map(|(id, keys, action)| KeyCommand {
                id: (*id).to_string(),
                context,
                action: *action,
                defaults: parse_keys(keys, id),
            }));
        }
        // Global: VEGAS's list (the Explorer's commands) with its keys.
        for (id, _) in GLOBAL_KEYS {
            assert!(
                EXPLORER.iter().any(|(e, _, _)| e == id),
                "Global key for unknown command {id}"
            );
        }
        let global = out.len();
        out.extend(EXPLORER.iter().map(|(id, _, action)| {
            KeyCommand {
                id: (*id).to_string(),
                context: Context::Global,
                action: *action,
                defaults: GLOBAL_KEYS
                    .iter()
                    .find(|(g, _)| g == id)
                    .map_or_else(Vec::new, |(_, keys)| parse_keys(keys, id)),
            }
        }));
        // BananaV's menu keys that VEGAS keeps outside its lists (Ctrl+Z,
        // Ctrl+C…) join the command's Global row; menu commands VEGAS lists
        // nowhere get a Global row of their own, at the end.
        for cmd in crate::commands::all() {
            let action = Action::Cmd(cmd);
            let taken: Vec<Binding> = out
                .iter()
                .filter(|c| {
                    matches!(c.context, Context::TrackView | Context::Global)
                        && c.action != Action::Soon
                })
                .flat_map(|c| c.defaults.iter().copied())
                .collect();
            let keys: Vec<Binding> = cmd
                .keys()
                .iter()
                .map(|&(m, k)| Binding::from_event(k, None, m))
                .filter(|b| !taken.contains(b))
                .collect();
            if let Some(row) = out[global..].iter_mut().find(|c| c.action == action) {
                row.defaults.extend(keys);
                continue;
            }
            let in_track_view = out
                .iter()
                .any(|c| c.context == Context::TrackView && c.action == action);
            if in_track_view && keys.is_empty() {
                continue;
            }
            out.push(KeyCommand {
                id: global_id(cmd),
                context: Context::Global,
                action,
                defaults: keys,
            });
        }
        out
    })
}

/// Saved keyboard maps, in `%APPDATA%\BananaV\teclado.json`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Store {
    #[serde(default)]
    active: Option<String>,
    /// Map name → "Context/id" → bindings.
    #[serde(default)]
    maps: BTreeMap<String, BTreeMap<String, Vec<String>>>,
}

fn store_path() -> Option<PathBuf> {
    Some(
        bv_platform::config_dir()?
            .join("BananaV")
            .join("teclado.json"),
    )
}

fn entry_key(c: &KeyCommand) -> String {
    format!("{}/{}", c.context.key(), c.id)
}

/// Bindings in use and the saved maps.
#[derive(Debug)]
pub struct Keymap {
    /// Bindings of each registry command (same order as [`registry`]).
    pub bindings: Vec<Vec<Binding>>,
    /// Name of the map in use (`None` = the default map).
    pub active: Option<String>,
    store: Store,
    path: Option<PathBuf>,
    /// (context, binding) → registry index of the command it runs.
    index: HashMap<(Context, Binding), usize>,
    /// First keys of the two-key shortcuts of each context.
    prefixes: std::collections::HashSet<(Context, Binding)>,
}

pub const DEFAULT_MAP: &str = "[Default]";

pub fn default_bindings() -> Vec<Vec<Binding>> {
    registry().iter().map(|c| c.defaults.clone()).collect()
}

impl Keymap {
    pub fn load() -> Keymap {
        let path = store_path();
        let store: Store = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Keymap::with_store(store, path)
    }

    pub fn in_memory() -> Keymap {
        Keymap::with_store(Store::default(), None)
    }

    fn with_store(store: Store, path: Option<PathBuf>) -> Keymap {
        let mut k = Keymap {
            bindings: default_bindings(),
            active: None,
            store,
            path,
            index: HashMap::new(),
            prefixes: std::collections::HashSet::new(),
        };
        if let Some(name) = k.store.active.clone()
            && let Some(b) = k.map_bindings(&name)
        {
            k.bindings = b;
            k.active = Some(name);
        }
        k.reindex();
        k
    }

    /// Names of the saved maps.
    pub fn map_names(&self) -> Vec<String> {
        self.store.maps.keys().cloned().collect()
    }

    /// The bindings of a saved map (commands it does not mention keep their
    /// defaults).
    pub fn map_bindings(&self, name: &str) -> Option<Vec<Vec<Binding>>> {
        let map = self.store.maps.get(name)?;
        Some(
            registry()
                .iter()
                .map(|c| match map.get(&entry_key(c)) {
                    Some(list) => list.iter().filter_map(|s| Binding::parse(s)).collect(),
                    None => c.defaults.clone(),
                })
                .collect(),
        )
    }

    fn save(&self) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(&self.store).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    /// Uses `bindings` from now on, as the map `name` (`None` = default).
    pub fn apply(
        &mut self,
        name: Option<String>,
        bindings: Vec<Vec<Binding>>,
    ) -> Result<(), String> {
        if let Some(n) = &name {
            let map = registry()
                .iter()
                .zip(&bindings)
                .map(|(c, b)| (entry_key(c), b.iter().map(Binding::storage).collect()))
                .collect();
            self.store.maps.insert(n.clone(), map);
        }
        self.store.active = name.clone();
        self.active = name;
        self.bindings = bindings;
        self.reindex();
        self.save()
    }

    pub fn delete_map(&mut self, name: &str) -> Result<(), String> {
        self.store.maps.remove(name);
        if self.store.active.as_deref() == Some(name) {
            self.store.active = None;
            self.active = None;
            self.bindings = default_bindings();
            self.reindex();
        }
        self.save()
    }

    fn reindex(&mut self) {
        self.index.clear();
        self.prefixes.clear();
        let reg = registry();
        for (i, c) in reg.iter().enumerate() {
            if !Context::LISTED.contains(&c.context) || c.action == Action::Soon {
                continue;
            }
            for b in &self.bindings[i] {
                self.index.entry((c.context, *b)).or_insert(i);
                if b.then.is_some() {
                    self.prefixes.insert((c.context, b.first()));
                }
            }
        }
    }

    /// Whether `b` starts a two-key shortcut in `focus` or Global.
    pub fn is_prefix(&self, b: &Binding, focus: Context) -> bool {
        self.prefixes.contains(&(focus, *b)) || self.prefixes.contains(&(Context::Global, *b))
    }

    /// The command a key press runs, if any: the focused context's
    /// (TrackView or TrackList) first, then Global's.
    pub fn lookup(&self, b: &Binding, focus: Context) -> Option<usize> {
        self.index
            .get(&(focus, *b))
            .or_else(|| self.index.get(&(Context::Global, *b)))
            .copied()
    }

    /// Shortcut text of `action` for the menus: Global keys first, then the
    /// other contexts ("Ctrl+Y; Ctrl+Shift+Z").
    pub fn text_for(&self, action: Action) -> String {
        let reg = registry();
        let mut keys: Vec<Binding> = Vec::new();
        for ctx in [Context::Global, Context::TrackView] {
            for (i, c) in reg.iter().enumerate() {
                if c.context == ctx && c.action == action {
                    for b in &self.bindings[i] {
                        if matches!(b.key, BindKey::Key(_)) && !keys.contains(b) {
                            keys.push(*b);
                        }
                    }
                }
            }
        }
        keys.iter()
            .map(Binding::label)
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// Shortcut text of `action` in context `ctx` alone, the keys of the
    /// first of its commands there (the Trimmer's menus show its own keys,
    /// as VEGAS's: Set In Point "I; [", not Edit.Selection.Start's too).
    pub fn text_in(&self, action: Action, ctx: Context) -> String {
        let reg = registry();
        let Some(i) = reg
            .iter()
            .position(|c| c.context == ctx && c.action == action)
        else {
            return String::new();
        };
        self.bindings[i]
            .iter()
            .filter(|b| matches!(b.key, BindKey::Key(_)))
            .map(Binding::label)
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// The visible part of each name, like VEGAS's list: a name shows only what
/// it does not share with the row above, indented by the hidden part. Returns
/// the length in bytes of that hidden prefix (dot included) and the rest.
pub fn compact_names<'a>(ids: impl IntoIterator<Item = &'a str>) -> Vec<(usize, String)> {
    let mut prev: Vec<&str> = Vec::new();
    let mut out = Vec::new();
    for id in ids {
        let parts: Vec<&str> = id.split('.').collect();
        let parent = &parts[..parts.len() - 1];
        let k = parent.iter().zip(&prev).take_while(|(a, b)| a == b).count();
        let hidden: usize = parts[..k].iter().map(|p| p.len() + 1).sum();
        out.push((hidden, parts[k..].join(".")));
        prev = parts;
    }
    out
}

/// How long a two-key shortcut waits for its second key.
const CHORD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// Runs the commands of the keys pressed this frame. The first key of a
/// two-key shortcut ("Alt+D, A") waits for the second one.
/// Commands that only move the edit cursor: with Shift, the time selection
/// follows it.
fn moves_cursor(action: Action) -> bool {
    matches!(
        action,
        Action::Cmd(
            Cmd::PixelLeft
                | Cmd::PixelRight
                | Cmd::FrameLeft
                | Cmd::FrameRight
                | Cmd::GridLeft
                | Cmd::GridRight
                | Cmd::MarkerLeft
                | Cmd::MarkerRight
                | Cmd::EditPointLeft
                | Cmd::EditPointRight
                | Cmd::ProjectStart
                | Cmd::ProjectEnd
                | Cmd::SelectedEventLeft
                | Cmd::SelectedEventRight
        )
    )
}

/// Shift with a key that moves the cursor (and nothing of its own): that
/// command, the time selection following the cursor, like VEGAS's.
fn extending(keymap: &Keymap, b: &Binding, focus: Context) -> Option<usize> {
    if !b.shift {
        return None;
    }
    let idx = keymap.lookup(&Binding { shift: false, ..*b }, focus)?;
    moves_cursor(registry()[idx].action).then_some(idx)
}

pub fn dispatch(app: &mut BananaApp, ctx: &egui::Context) {
    // Each command found, and whether the time selection follows it.
    let mut hits: Vec<(usize, bool)> = Vec::new();
    let keymap = &app.keymap;
    let focus = app.key_focus;
    let mut chord = app
        .key_chord
        .filter(|(_, at)| at.elapsed() < CHORD_TIMEOUT)
        .map(|(b, _)| b);
    let mut started = None;
    // A label's selected text is what Ctrl+C copies then (egui does it).
    let label_text = ctx
        .with_plugin(|s: &mut egui::text_selection::LabelSelectionState| s.has_selection())
        .unwrap_or(false);
    let mut v_down = app.v_down;
    let mut pasted = None;
    ctx.input_mut(|i| {
        let mut k = 0;
        while k < i.events.len() {
            let keys = pressed_keys(&i.events[k], i.modifiers, label_text, &mut v_down);
            let Some(&b) = keys.first() else {
                k += 1;
                continue;
            };
            if let Some(first) = chord.take() {
                // The second key: its modifiers do not count.
                let full = Binding {
                    then: Some(b.key),
                    ..first
                };
                hits.extend(keymap.lookup(&full, focus).map(|idx| (idx, false)));
                i.events.remove(k);
                continue;
            }
            if keymap.is_prefix(&b, focus) {
                chord = Some(b);
                started = Some(b);
                i.events.remove(k);
                continue;
            }
            if let Some(idx) = keys.iter().find_map(|b| keymap.lookup(b, focus)) {
                if let egui::Event::Paste(text) = &i.events[k] {
                    pasted = Some(text.clone());
                }
                i.events.remove(k);
                hits.push((idx, false));
                continue;
            }
            if let Some(idx) = extending(keymap, &b, focus) {
                i.events.remove(k);
                hits.push((idx, true));
                continue;
            }
            k += 1;
        }
    });
    // Num *, read from the keyboard while BananaV has it.
    let star = bv_platform::numpad_down('*') && ctx.input(|i| i.focused);
    let was = KEYPAD_STAR.swap(star, std::sync::atomic::Ordering::Relaxed);
    if star && !was {
        let m = ctx.input(|i| i.modifiers);
        let b = Binding {
            ctrl: m.ctrl || m.command,
            alt: m.alt,
            shift: m.shift,
            key: BindKey::Special("Num *"),
            then: None,
        };
        hits.extend(keymap.lookup(&b, focus).map(|idx| (idx, false)));
    }
    app.v_down = v_down;
    if pasted.is_some() {
        app.pasted_text = pasted;
    }
    app.key_chord = match (chord, started) {
        (Some(b), Some(_)) => Some((b, std::time::Instant::now())),
        (Some(_), None) => app.key_chord,
        (None, _) => None,
    };
    if let Some(b) = started.filter(|_| app.key_chord.is_some()) {
        app.notify(trf!("{}, … (press the second key)", b.label()));
    }
    for (idx, extend) in hits {
        let from = app.cursor;
        match registry()[idx].action {
            Action::Cmd(c) => {
                if c.enabled(app) {
                    c.run(app);
                }
            }
            Action::Extra(e) => app.run_extra(e),
            Action::Soon => {}
        }
        if extend {
            app.extend_selection(from);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_names_like_vegas() {
        let ids = [
            "CursorTo.Center",
            "CursorTo.End",
            "Edit.Copy",
            "Edit.Group.Clear",
            "Edit.Group.CopyAll",
            "Edit.Heal",
            "Edit.Select.EventEnd",
            "Edit.Select.EventEnd",
        ];
        let names: Vec<(usize, String)> = compact_names(ids);
        assert_eq!(names[0], (0, "CursorTo.Center".into()));
        assert_eq!(names[1], ("CursorTo.".len(), "End".into()));
        assert_eq!(names[2], (0, "Edit.Copy".into()));
        assert_eq!(names[3], ("Edit.".len(), "Group.Clear".into()));
        assert_eq!(names[4], ("Edit.Group.".len(), "CopyAll".into()));
        assert_eq!(names[5], ("Edit.".len(), "Heal".into()));
        assert_eq!(
            names[7],
            ("Edit.Select.".len(), "EventEnd".into()),
            "second key of the same command"
        );
    }

    #[test]
    fn bindings_round_trip_and_parse() {
        for s in [
            "Ctrl+Alt+Shift+Q",
            "Alt+Left",
            "Num 5",
            "Shift+Play",
            "Ctrl+[",
            "F23",
        ] {
            let b = Binding::parse(s).expect(s);
            assert_eq!(Binding::parse(&b.storage()), Some(b), "{s}");
        }
        assert_eq!(
            Binding::parse("Alt+Left").map(|b| b.label()).as_deref(),
            Some("Alt+Left")
        );
    }

    #[test]
    fn registry_has_no_conflicting_active_keys() {
        let reg = registry();
        let defaults = default_bindings();
        let mut seen: HashMap<(Binding, Context), Action> = HashMap::new();
        for (c, keys) in reg.iter().zip(&defaults) {
            if c.action == Action::Soon {
                continue;
            }
            for b in keys {
                if let Some(other) = seen.insert((*b, c.context), c.action) {
                    assert_eq!(other, c.action, "{} uses {} twice", c.id, b.label());
                }
            }
        }
        // Across TrackView and Global a key may repeat only for the same action.
        for (c, keys) in reg.iter().zip(&defaults) {
            if c.context != Context::Global || c.action == Action::Soon {
                continue;
            }
            for b in keys {
                if let Some(a) = seen.get(&(*b, Context::TrackView)) {
                    assert_eq!(*a, c.action, "{} vs TrackView on {}", c.id, b.label());
                }
            }
        }
    }

    #[test]
    fn saved_maps_change_and_restore_shortcuts() {
        let mut km = Keymap::in_memory();
        let split = registry()
            .iter()
            .position(|c| c.id == "Edit.Split")
            .expect("split");
        let s = Binding::parse("S").expect("S");
        let new = Binding::parse("Ctrl+Alt+Shift+K").expect("new key");
        assert_eq!(km.lookup(&s, Context::TrackView), Some(split));
        let mut b = km.bindings.clone();
        b[split] = vec![new];
        km.apply(Some("Test".into()), b).expect("apply");
        assert_eq!(
            km.lookup(&s, Context::TrackView),
            None,
            "the old key no longer splits"
        );
        assert_eq!(km.lookup(&new, Context::TrackView), Some(split));
        assert_eq!(km.map_names(), vec!["Test".to_string()]);
        assert_eq!(km.text_for(Action::Cmd(Cmd::Split)), "Ctrl+Alt+Shift+K");
        assert_eq!(km.map_bindings("Test").expect("saved")[split], vec![new]);
        km.delete_map("Test").expect("delete");
        assert_eq!(km.active, None);
        assert_eq!(
            km.lookup(&s, Context::TrackView),
            Some(split),
            "back to the default map"
        );
    }

    #[test]
    fn track_list_lists_the_track_view_commands_with_its_own_keys() {
        let reg = registry();
        let ids = |ctx| {
            reg.iter()
                .filter(|c| c.context == ctx)
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(Context::TrackList), ids(Context::TrackView));
        let keys = |id: &str| {
            let c = reg
                .iter()
                .find(|c| c.context == Context::TrackList && c.id == id)
                .expect(id);
            c.defaults.iter().map(Binding::storage).collect::<Vec<_>>()
        };
        assert_eq!(keys("TrackView.Track.CursorEnd"), ["Ctrl+PageDown", "End"]);
        assert_eq!(keys("TrackView.Track.Rename"), ["F2"]);
        assert_eq!(keys("TrackView.Track.PanNum5"), ["Num 5"]);
        assert!(
            keys("Edit.Split").is_empty(),
            "only track commands have keys"
        );
    }

    #[test]
    fn explorer_lists_vegas_commands_without_keys() {
        let reg = registry();
        let explorer: Vec<&KeyCommand> = reg
            .iter()
            .filter(|c| c.context == Context::Explorer)
            .collect();
        assert_eq!(explorer.len(), 326);
        assert_eq!(explorer[0].id, "Edit.ClearEditHistory");
        assert_eq!(
            explorer.last().map(|c| c.id.as_str()),
            Some("View.WindowLayouts.Save.UserLayout9")
        );
        assert!(explorer.iter().all(|c| c.defaults.is_empty()));
        let action = |id: &str| explorer.iter().find(|c| c.id == id).map(|c| c.action);
        assert_eq!(
            action("File.SaveInc"),
            Some(Action::Cmd(Cmd::SaveIncremental))
        );
        assert_eq!(
            action("Edit.Take.ChooseActive"),
            Some(Action::Cmd(Cmd::TakeChooseActive))
        );
        // Menu commands stay in Global even though Explorer lists them.
        assert!(
            reg.iter()
                .any(|c| c.context == Context::Global && c.action == Action::Cmd(Cmd::Preferences))
        );
    }

    #[test]
    fn trimmer_has_its_own_list_and_keys() {
        let reg = registry();
        let trimmer: Vec<&KeyCommand> = reg
            .iter()
            .filter(|c| c.context == Context::Trimmer)
            .collect();
        assert_eq!(trimmer.len(), 140);
        assert_eq!(trimmer[0].id, "CursorTo.Center");
        assert_eq!(
            trimmer.last().map(|c| c.id.as_str()),
            Some("TrackView.Zoom.MoreTrackHeight")
        );
        let keys = |id: &str| {
            let c = trimmer.iter().find(|c| c.id == id).expect(id);
            c.defaults.iter().map(Binding::storage).collect::<Vec<_>>()
        };
        assert_eq!(keys("TrackView.Select.MarkIn"), ["I", "OpenBracket"]);
        assert_eq!(keys("Edit.Trimmer.CutToCursor"), ["Shift+A"]);
        assert_eq!(keys("Trimmer.SelectPrevTrack"), ["Shift+Tab"]);
        // VEGAS's More menu's: Ctrl+End, Shift+Space, Shift+L, Ctrl+P.
        let km = Keymap::in_memory();
        for (key, op) in [
            ("Ctrl+End", TrimmerOp::End),
            ("Ctrl+Home", TrimmerOp::Start),
            ("Shift+Space", TrimmerOp::PlayFromStart),
            ("Space", TrimmerOp::PlayToggle),
            ("Shift+L", TrimmerOp::SelectLoop),
            ("Shift+Q", TrimmerOp::SelectLoop),
            ("Ctrl+P", TrimmerOp::TogglePreviewWindow),
            ("Ctrl+Shift+Down", TrimmerOp::Heights(-1)),
        ] {
            let b = Binding::parse(key).expect(key);
            let hit = km.lookup(&b, Context::Trimmer).map(|i| &reg[i]);
            assert_eq!(
                hit.map(|c| c.action),
                Some(Action::Extra(Extra::Trimmer(op))),
                "{key}"
            );
        }
    }

    #[test]
    fn every_vegas_tab_has_its_list() {
        let reg = registry();
        let count = |ctx| reg.iter().filter(|c| c.context == ctx).count();
        assert_eq!(count(Context::MixingDesk), 40);
        assert_eq!(count(Context::EditDetails), 35);
        assert_eq!(count(Context::TrackMotion), 44);
        assert_eq!(count(Context::MixingConsole), 61);
        let keys = |ctx, id: &str| {
            let c = reg
                .iter()
                .find(|c| c.context == ctx && c.id == id)
                .expect(id);
            c.defaults.iter().map(Binding::storage).collect::<Vec<_>>()
        };
        assert_eq!(
            keys(Context::MixingDesk, "TrackView.Track.ToggleSolo"),
            ["X"]
        );
        assert_eq!(
            keys(Context::EditDetails, "Edit.Take.ChooseActive"),
            ["Shift+T"]
        );
        assert_eq!(keys(Context::TrackMotion, "3DLayout6"), ["6"]);
        assert_eq!(
            keys(Context::MixingConsole, "MixingConsole.ChannelWidth.Default"),
            ["D", "Ctrl+Backtick"]
        );
        let km = Keymap::in_memory();
        let n = Binding::parse("N").expect("N");
        assert_eq!(
            km.lookup(&n, Context::MixingConsole).map(|i| reg[i].action),
            Some(Action::Extra(Extra::Console(ConsoleOp::Width(
                ChannelWidth::Narrow
            ))))
        );
    }

    #[test]
    fn shift_with_a_cursor_key_extends_the_selection() {
        let km = Keymap::in_memory();
        let reg = registry();
        let right = Binding::parse("Shift+Right").expect("Shift+Right");
        assert_eq!(km.lookup(&right, Context::TrackView), None);
        let i = extending(&km, &right, Context::TrackView).expect("extends");
        assert_eq!(reg[i].action, Action::Cmd(Cmd::PixelRight));
        let page = Binding::parse("Shift+PageDown").expect("Shift+PageDown");
        let i = extending(&km, &page, Context::TrackView).expect("extends");
        assert_eq!(reg[i].action, Action::Cmd(Cmd::GridRight));
        // Not a key that moves the cursor; nor without Shift.
        let del = Binding::parse("Shift+Delete").expect("Shift+Delete");
        assert_eq!(extending(&km, &del, Context::TrackView), None);
        let plain = Binding::parse("Right").expect("Right");
        assert_eq!(extending(&km, &plain, Context::TrackView), None);
    }

    #[test]
    fn global_is_vegas_list_with_its_keys() {
        let reg = registry();
        let global: Vec<&KeyCommand> = reg
            .iter()
            .filter(|c| c.context == Context::Global)
            .collect();
        assert_eq!(global[0].id, "Edit.ClearEditHistory");
        assert_eq!(global[325].id, "View.WindowLayouts.Save.UserLayout9");
        // Menu commands VEGAS lists nowhere come after its list.
        assert!(global[326..].iter().any(|c| c.id == "Edit.CloseGaps"));
        let keys = |id: &str| {
            let c = global.iter().find(|c| c.id == id).expect(id);
            c.defaults.iter().map(Binding::label).collect::<Vec<_>>()
        };
        assert_eq!(keys("File.Close"), ["Ctrl+F4"]);
        assert_eq!(keys("Transport.PlayToggle"), ["Ctrl+Space", "F12"]);
        assert_eq!(
            keys("View.WindowLayouts.Load.TimelineTopLayout"),
            ["Alt+D, T"]
        );
        assert_eq!(
            keys("View.WindowLayouts.Save.UserLayout3"),
            ["Ctrl+Alt+D, 3"]
        );
        // BananaV's fixed menu keys are listed (and can be changed) too.
        assert_eq!(keys("Edit.Undo"), ["Ctrl+Z"]);
    }

    #[test]
    fn two_key_shortcuts_round_trip() {
        let b = Binding::parse("Ctrl+Alt+D, 3").expect("chord");
        assert!(b.ctrl && b.alt && b.then.is_some());
        assert_eq!(Binding::parse(&b.storage()), Some(b));
        assert_eq!(b.label(), "Ctrl+Alt+D, 3");
        let km = Keymap::in_memory();
        assert!(km.is_prefix(&b.first(), Context::TrackView));
        assert!(
            km.lookup(&b, Context::TrackList).is_some(),
            "Global works everywhere"
        );
    }

    #[test]
    fn keys_follow_the_focused_context() {
        let km = Keymap::in_memory();
        let id = |i: Option<usize>| i.map(|i| registry()[i].id.as_str());
        let down = Binding::parse("Down").expect("Down");
        assert_eq!(
            id(km.lookup(&down, Context::TrackList)),
            Some("TrackView.Track.CursorDown")
        );
        assert_eq!(id(km.lookup(&down, Context::TrackView)), None);
        let s = Binding::parse("S").expect("S");
        assert_eq!(id(km.lookup(&s, Context::TrackView)), Some("Edit.Split"));
        assert_eq!(
            id(km.lookup(&s, Context::TrackList)),
            None,
            "TrackView keys stay there"
        );
        assert_eq!(id(km.lookup(&s, Context::Explorer)), None);
        // The track envelopes' keys work on a track picked by its header.
        let v = Binding::parse("V").expect("V");
        assert_eq!(
            id(km.lookup(&v, Context::TrackList)),
            Some("View.Envelope.TrackVolume")
        );
        // Global keys work everywhere.
        let ctrl_s = Binding::parse("Ctrl+S").expect("Ctrl+S");
        assert!(km.lookup(&ctrl_s, Context::TrackList).is_some());
        assert!(km.lookup(&ctrl_s, Context::Explorer).is_some());
    }

    #[test]
    fn punctuation_matches_the_physical_key() {
        // On a Brazilian layout the `´` dead key arrives as logical Key::OpenBracket
        // (fallback) and `[` too; the physical key tells them apart.
        let b = Binding::from_event(Key::OpenBracket, Some(Key::CloseBracket), Modifiers::ALT);
        assert_eq!(b.key, BindKey::Key(Key::CloseBracket));
        let km = Keymap::in_memory();
        let idx = km.lookup(&b, Context::TrackView).expect("Alt+] is bound");
        assert_eq!(registry()[idx].id, "Edit.Trim.End");
    }

    #[test]
    fn clipboard_events_are_the_keys_that_made_them() {
        let km = Keymap::in_memory();
        let reg = registry();
        let mut v_down = false;
        let mut run = |e: egui::Event, m: Modifiers| {
            pressed_keys(&e, m, false, &mut v_down)
                .iter()
                .find_map(|b| km.lookup(b, Context::TrackView))
                .map(|i| reg[i].action)
        };
        let ctrl = Modifiers::COMMAND;
        assert_eq!(run(egui::Event::Copy, ctrl), Some(Action::Cmd(Cmd::Copy)));
        assert_eq!(run(egui::Event::Cut, ctrl), Some(Action::Cmd(Cmd::Cut)));
        let text = || egui::Event::Paste("text".into());
        assert_eq!(run(text(), ctrl), Some(Action::Cmd(Cmd::Paste)));
        assert_eq!(
            run(text(), ctrl | Modifiers::SHIFT),
            Some(Action::Cmd(Cmd::PasteInsert))
        );
        let v = |pressed, modifiers| egui::Event::Key {
            key: Key::V,
            physical_key: Some(Key::V),
            pressed,
            repeat: false,
            modifiers,
        };
        // The text was pasted: its key coming up pastes nothing more.
        assert_eq!(run(v(false, ctrl), ctrl), None);
        // Ctrl+V with no text on the clipboard: only its release arrives.
        assert_eq!(run(v(false, ctrl), ctrl), Some(Action::Cmd(Cmd::Paste)));
        // A V seen going down pastes nothing when it comes up.
        let _ = run(v(true, Modifiers::NONE), Modifiers::NONE);
        assert_eq!(run(v(false, Modifiers::NONE), Modifiers::NONE), None);
        // A label's selected text copies itself.
        assert!(pressed_keys(&egui::Event::Copy, ctrl, true, &mut false).is_empty());
    }

    #[test]
    fn ctrl_c_and_ctrl_v_copy_and_paste_the_timelines_events() {
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1280.0, 800.0))
            .build_ui_state(
                |ui, app: &mut BananaApp| {
                    crate::apply_theme(ui.ctx());
                    app.frame(ui);
                },
                app,
            );
        h.run_steps(3);
        let events = |h: &egui_kittest::Harness<'_, BananaApp>| h.state().project().clips().count();
        let before = events(&h);
        let press = |h: &mut egui_kittest::Harness<'_, BananaApp>, e: egui::Event| {
            let events = &mut h.input_mut().events;
            events.push(egui::Event::ModifiersChanged(Modifiers::COMMAND));
            events.push(e);
            h.step();
            h.input_mut()
                .events
                .push(egui::Event::ModifiersChanged(Modifiers::NONE));
            h.step();
        };
        press(&mut h, egui::Event::Copy);
        assert!(h.state().clipboard.is_some(), "copied");
        press(&mut h, egui::Event::Paste("text".into()));
        assert_eq!(events(&h), before + 1, "pasted");
        // A picture on the clipboard: Ctrl+V comes only as its release.
        press(
            &mut h,
            egui::Event::Key {
                key: Key::V,
                physical_key: Some(Key::V),
                pressed: false,
                repeat: false,
                modifiers: Modifiers::COMMAND,
            },
        );
        assert_eq!(events(&h), before + 1, "the pasted text's key, up");
        press(
            &mut h,
            egui::Event::Key {
                key: Key::V,
                physical_key: Some(Key::V),
                pressed: false,
                repeat: false,
                modifiers: Modifiers::COMMAND,
            },
        );
        assert_eq!(events(&h), before + 2, "pasted again");
    }
}
