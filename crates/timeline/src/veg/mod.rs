//! `.veg` projects (VEGAS Pro's): read into a [`Document`] — the
//! project's settings, media, tracks and their events with their takes,
//! fades, envelopes and effects, markers, regions, command markers, the
//! audio CD layout, groups, busses, bins and summary — that [`build`]
//! makes a BananaV project of once the caller found its media; and
//! written from a project ([`write`]) as VEGAS Pro 23 writes its own, so
//! that it opens them.
//!
//! The file is Sony Wave64's RIFF of 64 bits ([`riff`]): a tree of chunks
//! named by GUIDs, each structure in them starting with its own size, so
//! a reader takes what it knows of a longer one and a shorter one leaves
//! the rest at its defaults. Times are counted in 100 ns, text is UTF-16.
//! What BananaV has no place for (generated media, say) is left out and
//! counted.
//!
//! Everything here is bytes in, bytes out; reading and writing files and
//! probing the media are the caller's.

mod audio;
mod build;
mod pancrop;
mod read;
mod riff;
#[cfg(test)]
mod tests;
mod write;

pub use build::{Imported, build};
pub use read::{DocMedia, Document, read};
pub use riff::VegError;
pub use write::{WriteOptions, write};

/// A plug-in parameter's value as the file holds it.
#[derive(Clone, Debug, PartialEq)]
pub enum ParamValue {
    Number(f64),
    /// A whole number: an integer, a choice, a switch.
    Integer(i64),
    /// Two numbers or more: a point, a color.
    Numbers(Vec<f64>),
    /// A text: a file's path, a curve's points…
    Text(String),
}

/// A keyframe of an effect's parameter: its frame from the effect's start,
/// its interpolation (the file's numbers), its value and, for each number
/// that has them, its handles as they are kept: where the one toward the
/// keyframe before is (frame, value), then the one toward the next.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocKey {
    pub frame: f64,
    pub interpolation: i32,
    pub value: Vec<f64>,
    pub handles: Vec<[f64; 4]>,
}

/// An effect as another program's plug-in has it: its id
/// (`com.vegascreativesoftware:sepia`), its preset's name and its
/// parameters; or, for a plug-in of the file's own kind, its class and its
/// preset's name alone.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocEffect {
    pub plugin: String,
    pub preset: String,
    pub params: Vec<(String, ParamValue)>,
    /// The parameters that change over time.
    pub keys: Vec<(String, Vec<DocKey>)>,
    pub bypass: bool,
    /// An event's: it works before its Pan/Crop.
    pub before_pan_crop: bool,
    /// It is a transition.
    pub transition: bool,
    /// The class of a plug-in of the file's own kind, and its settings'
    /// record.
    pub native: Option<String>,
    pub native_data: Vec<u8>,
}

use bv_core::{TICKS_PER_SECOND, Tick};
use riff::Guid;

/// 100 ns steps in a second, the file's unit of time.
const UNITS: i64 = 10_000_000;

/// A time of the file as BananaV's ticks.
fn ticks(units: i64) -> Tick {
    Tick((units as i128 * TICKS_PER_SECOND as i128 / UNITS as i128) as i64)
}

/// BananaV's ticks as the file's 100 ns, rounded.
fn units(t: Tick) -> i64 {
    let n = t.0 as i128 * UNITS as i128;
    let d = TICKS_PER_SECOND as i128;
    (if n >= 0 {
        (n + d / 2) / d
    } else {
        (n - d / 2) / d
    }) as i64
}

/// The file's type for a VEGAS Pro project, and its version: VEGAS Pro
/// 23's (2026's is 0x170000).
const PROJECT: Guid = Guid::parse("46C429EF-904A-11D2-8722-00C04F8EDB8A");
const VERSION: u32 = 0x16_0000;

// The chunks, as the file names them.
const HEADER: Guid = Guid::parse("B28F2D5A-230F-11D2-86AF-00C04F8EDB8A");
const FOUR_LIST: Guid = Guid::parse("2CCCB9D8-DEDD-46AC-853D-1916A99A9A02");
const FOUR: Guid = Guid::parse("5E8A8C39-DA6B-4499-B0B2-3663709E1CE4");
const OUTPUT_FX: Guid = Guid::parse("6697A914-7B18-11D2-8713-00C04F8EDB8A");
const PROJECT_ENVELOPES: Guid = Guid::parse("F0CA0912-3506-496F-BDBF-7807976210D2");
const MEDIA_LIST: Guid = Guid::parse("B28F2D5B-230F-11D2-86AF-00C04F8EDB8A");
const MEDIA: Guid = Guid::parse("B28F2D5D-230F-11D2-86AF-00C04F8EDB8A");
const GROUPS: Guid = Guid::parse("B8E73E18-7F4C-456B-93CD-6866B95E5F2B");
const GROUP: Guid = Guid::parse("E901E132-C677-4604-AC9D-DB0B87B4EEBD");
const TRACKS: Guid = Guid::parse("4D6C0747-2316-11D2-86B0-00C04F8EDB8A");
const TRACK: Guid = Guid::parse("4D6C0748-2316-11D2-86B0-00C04F8EDB8A");
/// A track's header, and the type of the list of its events.
const TRACK_HEADER: Guid = Guid::parse("4D6C0749-2316-11D2-86B0-00C04F8EDB8A");
const EVENT: Guid = Guid::parse("4D6C074A-2316-11D2-86B0-00C04F8EDB8A");
const EVENT_HEADER: Guid = Guid::parse("168D206A-2321-11D2-86B0-00C04F8EDB8A");
const TAKE: Guid = Guid::parse("168D206D-2321-11D2-86B0-00C04F8EDB8A");
const EVENT_ENVELOPES: Guid = Guid::parse("84701B5A-6863-11D2-86FD-00C04F8EDB8A");
const ENVELOPES_HEADER: Guid = Guid::parse("84701B5B-6863-11D2-86FD-00C04F8EDB8A");
const ENVELOPE: Guid = Guid::parse("84701B5C-6863-11D2-86FD-00C04F8EDB8A");
const TRACK_ENVELOPES: Guid = Guid::parse("84701B5D-6863-11D2-86FD-00C04F8EDB8A");
/// An audio event's properties: a list of a chunk of the same GUID.
const AUDIO_EVENT: Guid = Guid::parse("D54AD82A-EFA7-4C24-8C0C-316427A5F209");
const VIDEO_EVENT_LIST: Guid = Guid::parse("FC2B3BCE-7985-11D2-8711-00C04F8EDB8A");
const VIDEO_EVENT: Guid = Guid::parse("FC2B3BCF-7985-11D2-8711-00C04F8EDB8A");
const VIDEO_EVENT_ENVELOPES_1: Guid = Guid::parse("3277F926-B5D3-40A1-8203-0EE5FA443F4A");
const EVENT_FX: Guid = Guid::parse("1AF0E4E4-6E7F-11D2-8705-00C04F8EDB8A");
const TRACK_FX: Guid = Guid::parse("1AF0E4E5-6E7F-11D2-8705-00C04F8EDB8A");
const EFFECT: Guid = Guid::parse("1AF0E4E6-6E7F-11D2-8705-00C04F8EDB8A");
const PAN_CROP: Guid = Guid::parse("884BACFF-74EB-456C-88F3-739E8390BD07");
const AUDIO_TRACK_LIST: Guid = Guid::parse("69D46C26-7F0B-11D2-8717-00C04F8EDB8A");
const AUDIO_TRACK: Guid = Guid::parse("69D46C27-7F0B-11D2-8717-00C04F8EDB8A");
const AUDIO_TRACK_FX: Guid = Guid::parse("F79298F4-5695-4B17-9DDB-19A0912D39F3");
const AUDIO_EVENT_FX: Guid = Guid::parse("B66A95CE-5946-48AB-A4F4-F88C95ACDC1D");
const VIDEO_TRACK_LIST: Guid = Guid::parse("69D46C28-7F0B-11D2-8717-00C04F8EDB8A");
const VIDEO_TRACK: Guid = Guid::parse("69D46C29-7F0B-11D2-8717-00C04F8EDB8A");
const TRACK_LIST_B5707E20: Guid = Guid::parse("B5707E20-7A06-4E83-B961-4DE58141220A");
const TRACK_FX_2: Guid = Guid::parse("20E4E6B9-4EEE-4D5A-9EDF-2E7AAC9672F4");
const SUMMARY_LIST: Guid = Guid::parse("925F94BC-525A-11D2-86DC-00C04F8EDB8A");
const SUMMARY: Guid = Guid::parse("71234F1D-5257-11D2-86DC-00C04F8EDB8A");
const MARKERS: Guid = Guid::parse("90020EF4-392D-11D2-86C7-00C04F8EDB8A");
const COMMANDS: Guid = Guid::parse("90020EF5-392D-11D2-86C7-00C04F8EDB8A");
const CD_LAYOUT: Guid = Guid::parse("90020EF7-392D-11D2-86C7-00C04F8EDB8A");
const MARKER: Guid = Guid::parse("ABF76256-392D-11D2-86C7-00C04F8EDB8A");
const MIXER: Guid = Guid::parse("220D2BE4-7FD3-11D2-8718-00C04F8EDB8A");
const MIXER_SETTINGS: Guid = Guid::parse("F1FF1C46-A61A-4BCC-BCF3-2477D138DB8A");
const BUS: Guid = Guid::parse("72B8749D-14AB-4588-94A9-39343AEEF7CC");
const BINS: Guid = Guid::parse("CA8CAF30-CB40-4C9C-992C-659CB76624BA");
const TREE_ITEM: Guid = Guid::parse("CA8CAF31-CB40-4C9C-992C-659CB76624BA");
const TREE_DATA: Guid = Guid::parse("CA8CAF32-CB40-4C9C-992C-659CB76624BA");
const LIST_1F0900B5: Guid = Guid::parse("1F0900B5-DC84-4743-81CB-087744404598");
const TIMELINES: Guid = Guid::parse("AA585C13-5A89-4B7C-A288-CDDD1066DACF");
/// The folders and file options: a list of a chunk of the same GUID.
const FOLDERS: Guid = Guid::parse("C5EA942E-91F9-4AD0-ACCC-72C090E71969");

/// The property sets closing a track's header, an event's header and an
/// audio file's entry.
const TRACK_PROPERTIES: Guid = Guid::parse("ADE0C7D1-1991-12EF-4573-2BEF2100B10A");
const EVENT_PROPERTIES: Guid = Guid::parse("F44FC1AB-6C91-4B8E-A3FC-0F4DB7B18C08");
const AUDIO_MEDIA_PROPERTIES: Guid = Guid::parse("612CCA25-92BB-4C55-8408-7BDA2F15BD27");
/// An OpenFX plug-in's effect.
const OFX_HOST: Guid = Guid::parse("A2FC1A41-01F6-460E-AE5A-2BE4AF57B92E");
const TRACK_MOTION_LIST: Guid = Guid::parse("D24DF987-1842-4A39-B079-9E4379485A38");
const PARENT_MOTION_LIST: Guid = Guid::parse("B12F5229-C44C-413E-84BD-A6EFF5746218");
const TRACK_MOTION: Guid = Guid::parse("CD6915D8-117A-45D6-8CCD-BE37A6C3BC9D");
/// A transition's progress envelope's kind.
const TRANSITION_PROGRESS: i32 = 200;
/// An event's velocity envelope's kind.
const VELOCITY: i32 = 202;

/// The kinds of tracks and events, as the file counts them.
const AUDIO: u32 = 2;
const VIDEO: u32 = 3;

/// Command markers' commands as the file spells them (BananaV's list
/// has `Text` for `TEXT`, `Scott EOM` for `SCOTT EOM`…).
const COMMAND_NAMES: [&str; 14] = [
    "TEXT",
    "URL",
    "Title",
    "Author",
    "Copyright",
    "WMClosedCaption",
    "WMTextBodytext",
    "WMTextHeadline",
    "SCOTT EOM",
    "SCOTT Cue In",
    "608CC1",
    "608CC2",
    "608CC3",
    "608CC4",
];

/// A command as BananaV's list spells it (one not in it as it is).
fn command_in(name: &str) -> String {
    crate::commands::COMMANDS
        .iter()
        .find(|c| c.eq_ignore_ascii_case(name))
        .map_or_else(|| name.to_string(), |c| (*c).to_string())
}

/// A command as the file spells it.
fn command_out(name: &str) -> &str {
    COMMAND_NAMES
        .iter()
        .find(|c| c.eq_ignore_ascii_case(name))
        .copied()
        .unwrap_or(name)
}

/// The fades' and envelopes' curves, as the file numbers them.
fn fade_curve(n: i32) -> crate::fx::FadeCurve {
    use crate::fx::FadeCurve;
    match n {
        2 => FadeCurve::Fast,
        -2 => FadeCurve::Slow,
        4 => FadeCurve::Smooth,
        -4 => FadeCurve::Sharp,
        _ => FadeCurve::Linear,
    }
}

fn fade_curve_number(c: crate::fx::FadeCurve) -> i32 {
    use crate::fx::FadeCurve;
    match c {
        FadeCurve::Linear => 1,
        FadeCurve::Fast => 2,
        FadeCurve::Slow => -2,
        FadeCurve::Smooth => 4,
        FadeCurve::Sharp => -4,
    }
}

fn key_curve(n: i32) -> crate::fx::KeyCurve {
    use crate::fx::KeyCurve;
    match n {
        0 => KeyCurve::Hold,
        2 => KeyCurve::Fast,
        -2 => KeyCurve::Slow,
        4 => KeyCurve::Smooth,
        -4 => KeyCurve::Sharp,
        _ => KeyCurve::Linear,
    }
}

/// A keyframe's curve from an effect parameter's interpolation number (the
/// choices, switches and whole numbers have none: they hold).
pub fn interpolation_curve(n: i32) -> crate::fx::KeyCurve {
    use crate::fx::KeyCurve;
    match n {
        1 => KeyCurve::Linear,
        2 => KeyCurve::Fast,
        3 => KeyCurve::Slow,
        4 => KeyCurve::Smooth,
        5 => KeyCurve::Sharp,
        7 | 9 => KeyCurve::Bezier,
        8 | 10 => KeyCurve::BezierSplit,
        _ => KeyCurve::Hold,
    }
}

fn key_curve_number(c: crate::fx::KeyCurve) -> i32 {
    use crate::fx::KeyCurve;
    match c {
        KeyCurve::Hold => 0,
        KeyCurve::Fast => 2,
        KeyCurve::Slow => -2,
        KeyCurve::Smooth => 4,
        KeyCurve::Sharp => -4,
        KeyCurve::Linear | KeyCurve::Bezier | KeyCurve::BezierSplit => 1,
    }
}
