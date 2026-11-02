//! Optical Flow resampling (the project's or an event's Resample mode):
//! the frame a media lacks made along the dense flow from its frame before
//! to its next ([`bv_track::flow::between`]), each pair's flow kept for the
//! frames that follow between the same two.

use std::sync::{Arc, Mutex};

use bv_core::{ImageRgbaF32, Tick};
use bv_track::flow::{Flow, FlowQuality, between, pictures_flow};

/// A pair of frames: the reader's owner, the next frame's instant and the
/// pictures' size.
type Key = (u64, Tick, u32, u32);

/// A flow and its scale ([`pictures_flow`]'s).
type Scaled = Arc<(Flow, [f32; 2])>;

/// The last flows worked out.
static FLOWS: Mutex<Vec<(Key, Scaled)>> = Mutex::new(Vec::new());

/// How many flows are kept (a few events resampled at once).
const KEPT: usize = 8;

/// The picture `t` of the way from `a` to `b` (an owner's frames around
/// the instant shown, `b` the one at `next`) along their flow.
pub(crate) fn frame_between(
    owner: u64,
    next: Tick,
    a: &ImageRgbaF32,
    b: &ImageRgbaF32,
    t: f32,
) -> Vec<[f32; 4]> {
    let (w, h) = (a.width as usize, a.height as usize);
    let key = (owner, next, a.width, a.height);
    let kept = FLOWS.lock().ok().and_then(|f| {
        f.iter()
            .find(|(k, _)| *k == key)
            .map(|(_, f)| Arc::clone(f))
    });
    let flow = kept.unwrap_or_else(|| {
        let made = Arc::new(pictures_flow(w, h, &a.data, &b.data, FlowQuality::Medium));
        if let Ok(mut f) = FLOWS.lock() {
            f.retain(|(k, _)| *k != key);
            f.push((key, Arc::clone(&made)));
            if f.len() > KEPT {
                f.remove(0);
            }
        }
        made
    });
    between(w, h, &a.data, &b.data, &flow, t)
}
