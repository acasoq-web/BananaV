//! A part of a media picked on its thumbnail, as VEGAS's Help describes
//! Project Media's hover scrubbing ("Project Media Hover Scrubbing") and
//! its storyboards' cells: the picture spans the media, a drag across it
//! selects a part (one starting up or down takes the media away instead),
//! the part's ends are dragged, I and O set them where the pointer is,
//! and the part shows lighter between two lines.

use bv_core::{FrameRate, Tick, TimeRange};

use crate::egui::{self, Color32, Rect, Stroke};

/// How near an end the pointer grabs it.
const GRAB: f32 = 4.0;

/// What a drag on a thumbnail moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PartDrag {
    /// A new part, from this media time.
    From(Tick),
    /// The part's start.
    In,
    /// The part's end.
    Out,
}

/// The media time at `x` across `pic`, for a media `length` long (on a
/// frame of `rate` when it has one).
pub(crate) fn time_across(pic: Rect, x: f32, length: Tick, rate: Option<FrameRate>) -> Tick {
    let k = ((x - pic.left()) / pic.width().max(1.0)).clamp(0.0, 1.0) as f64;
    let t = Tick((length.0 as f64 * k) as i64);
    rate.map_or(t, |r| r.snap_floor(t))
}

fn x_of(pic: Rect, t: Tick, length: Tick) -> f32 {
    pic.left() + pic.width() * (t.0 as f64 / length.0.max(1) as f64) as f32
}

/// Whether `resp`'s drag started more across than up or down: where it
/// was pressed, then.
pub(crate) fn across(resp: &egui::Response) -> Option<egui::Pos2> {
    let origin = resp.ctx.input(|i| i.pointer.press_origin())?;
    let now = resp.interact_pointer_pos()?;
    ((now.x - origin.x).abs() >= (now.y - origin.y).abs()).then_some(origin)
}

/// What a drag across `pic` pressed at `x` moves: an end of `part` near
/// it, else a new part.
pub(crate) fn drag_at(
    pic: Rect,
    x: f32,
    length: Tick,
    rate: Option<FrameRate>,
    part: Option<TimeRange>,
) -> PartDrag {
    if let Some(p) = part {
        let (a, b) = (x_of(pic, p.start, length), x_of(pic, p.end(), length));
        if (x - b).abs() <= GRAB {
            return PartDrag::Out;
        }
        if (x - a).abs() <= GRAB {
            return PartDrag::In;
        }
    }
    PartDrag::From(time_across(pic, x, length, rate))
}

/// `part` as `drag` takes it to media time `t` (`None`: nothing yet).
pub(crate) fn dragged(
    drag: PartDrag,
    part: Option<TimeRange>,
    t: Tick,
    length: Tick,
) -> Option<TimeRange> {
    let whole = TimeRange::new(Tick::ZERO, length);
    let p = part.unwrap_or(whole);
    let r = match drag {
        PartDrag::From(f) => TimeRange::from_bounds(f.min(t), f.max(t)),
        PartDrag::In => TimeRange::from_bounds(t.min(p.end()), p.end()),
        PartDrag::Out => TimeRange::from_bounds(p.start, t.max(p.start)),
    };
    (r.duration > Tick::ZERO).then_some(r)
}

/// I (`start`) or O at media time `t`: the part's end there, the other
/// kept when it stays on its side (else the media's start or end).
pub(crate) fn mark(part: Option<TimeRange>, t: Tick, length: Tick, start: bool) -> TimeRange {
    let t = t.clamp(Tick::ZERO, length);
    match (start, part) {
        (true, Some(p)) if p.end() > t => TimeRange::from_bounds(t, p.end()),
        (true, _) => TimeRange::from_bounds(t, length),
        (false, Some(p)) if p.start < t => TimeRange::from_bounds(p.start, t),
        (false, _) => TimeRange::from_bounds(Tick::ZERO, t),
    }
}

/// The part over the picture: lighter, between two white lines (the
/// Help's picture).
pub(crate) fn paint(painter: &egui::Painter, pic: Rect, length: Tick, part: TimeRange) {
    let (a, b) = (x_of(pic, part.start, length), x_of(pic, part.end(), length));
    let r = Rect::from_x_y_ranges(a..=b, pic.y_range());
    painter.rect_filled(r, 0.0, Color32::from_white_alpha(64));
    for x in [a, b] {
        painter.vline(x, pic.y_range(), Stroke::new(1.5, Color32::WHITE));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::egui::{pos2, vec2};

    fn s(n: i64) -> Tick {
        Tick::from_seconds(n)
    }

    /// A drag across picks from where it began to where it is, an end near
    /// the pointer is dragged instead, and I and O keep the other end on
    /// its side.
    #[test]
    fn parts_are_dragged_and_marked() {
        let pic = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 50.0));
        let len = s(10);
        // On frames of 25 a second: whole seconds where the math is.
        let r = Some(FrameRate::new(25, 1));
        assert_eq!(time_across(pic, 50.0, len, r), s(5));
        let d = drag_at(pic, 80.0, len, r, None);
        assert_eq!(d, PartDrag::From(s(8)));
        assert_eq!(
            dragged(d, None, s(3), len),
            Some(TimeRange::from_bounds(s(3), s(8)))
        );
        assert_eq!(dragged(d, None, s(8), len), None, "no length yet");
        let part = Some(TimeRange::from_bounds(s(3), s(8)));
        assert_eq!(drag_at(pic, 79.0, len, r, part), PartDrag::Out);
        assert_eq!(drag_at(pic, 32.0, len, r, part), PartDrag::In);
        assert_eq!(
            dragged(PartDrag::In, part, s(9), len),
            None,
            "the start never passes the end"
        );
        assert_eq!(
            mark(part, s(5), len, true),
            TimeRange::from_bounds(s(5), s(8))
        );
        assert_eq!(
            mark(part, s(9), len, true),
            TimeRange::from_bounds(s(9), len)
        );
        assert_eq!(
            mark(None, s(4), len, false),
            TimeRange::from_bounds(Tick::ZERO, s(4))
        );
    }
}
