//! Final Cut Pro 7 XML (`xmeml`): the timeline interchange of Final Cut
//! Pro 7, DaVinci Resolve, Premiere Pro and VEGAS (File › Import and File ›
//! Export's "Final Cut Pro 7/DaVinci Resolve (*.xml)", File › Open), read
//! into a timeline document ([`read`]; [`crate::otio::build`] makes the
//! project once its media are found) and written from a project
//! ([`export`]).
//!
//! The format as its writers keep it (Apple's description of it,
//! OpenTimelineIO's reading, those programs' files):
//! - a `file` is described where it first appears in the document — a
//!   bin's master clip (VEGAS's), or the first clip item that uses it — and
//!   named only by its id after;
//! - a clip item's `start` and `end` are the sequence's frames, its `in`
//!   and `out` its media's at the clip item's own rate. Next to a
//!   transition its `start` or `end` is −1 and the transition places it:
//!   the clip reaches over the whole transition, its `in` being its media's
//!   frame at the transition's start (a handle), so two clips around a
//!   transition overlap — BananaV's crossfade — and a clip with nothing on
//!   the transition's other side fades from or to black (`start-black`,
//!   `end-black`);
//! - sound is mono clip items: a stereo pair is two of them on two tracks,
//!   their `sourcetrack`'s `trackindex` the file's channels (counted across
//!   its audio streams), linked and panned left and right — one event
//!   playing its stream in BananaV;
//! - VEGAS lists its audio tracks from the bottom up, and names its first
//!   stream's channels whichever stream an event plays (the build gives
//!   each further track playing the same channels at once the next
//!   stream).

mod read;
#[cfg(test)]
mod tests;
mod write;

pub use read::read;
pub use write::export;

use bv_core::FrameRate;

use crate::xml::Element;

/// A `<rate>`'s frame rate: its timebase, made NTSC's (×1000⁄1001) when
/// it says so.
fn rate_of(rate: &Element) -> Option<FrameRate> {
    let timebase: f64 = rate.child_text("timebase")?.trim().parse().ok()?;
    let timebase = timebase.round() as u32;
    let ntsc = rate
        .child_text("ntsc")
        .is_some_and(|t| t.eq_ignore_ascii_case("true"));
    (timebase > 0).then(|| {
        if ntsc {
            FrameRate::new(timebase * 1000, 1001)
        } else {
            FrameRate::new(timebase, 1)
        }
    })
}

/// A rate as `<rate>` says it: whole frames a second, and whether it is
/// NTSC's (29.97 is 30 NTSC).
fn timebase(rate: FrameRate) -> (u32, bool) {
    (rate.as_f64().round() as u32, rate.den == 1001)
}

/// Final Cut Pro 7's audio crossfades: of constant power (+3 dB in the
/// middle) and of constant gain (0 dB).
const CROSSFADE_3DB: (&str, &str) = ("Cross Fade (+3dB)", "KGAudioTransCrossFade3dB");
const CROSSFADE_0DB: (&str, &str) = ("Cross Fade (0dB)", "KGAudioTransCrossFade0dB");
