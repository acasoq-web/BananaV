//! OpenTimelineIO (`.otio`): the Academy Software Foundation's open format
//! for editorial timelines, read and written as its JSON serialization
//! specifies, with the schemas OpenTimelineIO 0.15 to 0.18 write
//! (`Timeline.1`, `Stack.1`, `Track.1`, `Clip.2`, `Gap.1`, `Transition.1`,
//! `Marker.2`, `ExternalReference.1`, `ImageSequenceReference.1`,
//! `GeneratorReference.1`, `MissingReference.1`, `LinearTimeWarp.1`,
//! `FreezeFrame.1`, `Effect.1`; older `Clip.1` and `Marker.1` are read too).
//!
//! [`export`] writes a project: its video tracks from the bottom up, then
//! its audio tracks, each event a clip whose source range is what it shows
//! of its media; a crossfade is a transition centred on a cut, a fade a
//! transition to or from a gap, a speed a `LinearTimeWarp`, a frozen frame
//! a `FreezeFrame`. Overlaps a transition cannot say (an event inside
//! another, three at once, overlaps without crossfades) go to tracks of
//! their own over their track ("lanes"). BananaV keeps everything else in
//! the objects' `metadata["bananav"]`, so a project comes back as it went.
//!
//! [`read`] parses a document into a [`Document`]: its tracks' events
//! placed in time and the media they use; the caller finds each media file
//! ([`MediaRef`]) and [`build`] makes the project. Other programs' files
//! get what the schema says; where they kept BananaV's metadata, the
//! timing in the file still wins (another program may have edited it).
//!
//! Everything here is text in, text out; the files, bundles (`.otioz`,
//! `.otiod`) and media are the caller's.

mod build;
mod export;
mod read;
#[cfg(test)]
mod tests;

pub use build::{Imported, build};
pub(crate) use export::lanes;
pub use export::{export, export_with, relative_url};
pub(crate) use read::{DocChannels, DocEvent, DocMarker, DocTrack};
pub use read::{Document, MediaRef, MediaRefSource, read, url_path};

use bv_core::{FrameRate, TICKS_PER_SECOND, Tick};
use serde_json::{Value, json};

/// The key of BananaV's metadata in each object's `metadata`.
const KEY: &str = "bananav";
/// The version of what BananaV keeps there.
const METADATA_VERSION: u32 = 1;
/// A clip's media reference's key, OpenTimelineIO's default.
const DEFAULT_MEDIA: &str = "DEFAULT_MEDIA";

/// Frames of `t` at `rate`: whole when `t` falls on a frame.
fn frames(t: Tick, rate: FrameRate) -> f64 {
    let n = t.0 as i128 * rate.num as i128;
    let d = rate.den as i128 * TICKS_PER_SECOND as i128;
    if n % d == 0 {
        (n / d) as f64
    } else {
        n as f64 / d as f64
    }
}

/// A `RationalTime.1`: `t` counted in frames of `rate`.
fn rational(t: Tick, rate: FrameRate) -> Value {
    json!({
        "OTIO_SCHEMA": "RationalTime.1",
        "rate": rate.as_f64(),
        "value": frames(t, rate),
    })
}

/// A `TimeRange.1`.
fn time_range(start: Tick, duration: Tick, rate: FrameRate) -> Value {
    json!({
        "OTIO_SCHEMA": "TimeRange.1",
        "duration": rational(duration, rate),
        "start_time": rational(start, rate),
    })
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// A rate as OpenTimelineIO writes it (a float) as a fraction: whole
/// rates, NTSC's (×1000⁄1001, also when written rounded, 29.97 or 23.98),
/// anything else to a thousandth.
fn fraction(rate: f64) -> Option<(u32, u32)> {
    if !rate.is_finite() || rate <= 0.0 || rate > 1e7 {
        return None;
    }
    let whole = rate.round();
    if whole >= 1.0 && (rate - whole).abs() < 1e-6 {
        return Some((whole as u32, 1));
    }
    let ntsc = (rate * 1.001).round();
    if ntsc >= 1.0 && (rate - ntsc / 1.001).abs() < 0.006 {
        return Some((ntsc as u32 * 1000, 1001));
    }
    let n = ((rate * 1000.0).round() as u64).max(1);
    let g = gcd(n, 1000);
    Some(((n / g) as u32, (1000 / g) as u32))
}

/// The frame rate OpenTimelineIO's `rate` stands for.
fn frame_rate(rate: f64) -> Option<FrameRate> {
    fraction(rate).map(|(num, den)| FrameRate::new(num, den))
}

/// `value` frames at `rate` in ticks; whole frames exactly.
fn ticks(value: f64, rate: f64) -> Tick {
    let Some((num, den)) = fraction(rate) else {
        return Tick::ZERO;
    };
    if !value.is_finite() {
        return Tick::ZERO;
    }
    let value = value.clamp(-1e15, 1e15);
    let whole = value.round();
    let t = if (value - whole).abs() < 1e-6 {
        let n = whole as i128 * den as i128 * TICKS_PER_SECOND as i128;
        let d = num as i128;
        (n + n.signum() * d / 2) / d
    } else {
        (value * den as f64 * TICKS_PER_SECOND as f64 / num as f64).round() as i128
    };
    Tick(t.clamp(i64::MIN as i128 / 4, i64::MAX as i128 / 4) as i64)
}

/// A `RationalTime`'s ticks, and its rate.
fn read_time(v: &Value) -> Option<(Tick, f64)> {
    let rate = v.get("rate")?.as_f64()?;
    let value = v.get("value")?.as_f64()?;
    fraction(rate)?;
    Some((ticks(value, rate), rate))
}

/// A `TimeRange`'s start and duration, in ticks.
fn read_range(v: &Value) -> Option<(Tick, Tick)> {
    let (start, _) = read_time(v.get("start_time")?)?;
    let (duration, _) = read_time(v.get("duration")?)?;
    Some((start, duration))
}

/// An object's schema: its name and version (`"Clip.2"`: `("Clip", 2)`).
fn schema(v: &Value) -> Option<(&str, u32)> {
    let s = v.get("OTIO_SCHEMA")?.as_str()?;
    let (name, version) = s.rsplit_once('.')?;
    Some((name, version.parse().ok()?))
}

/// What BananaV kept in an object's metadata.
fn ours(v: &Value) -> Option<&Value> {
    v.get("metadata")?.get(KEY)
}
