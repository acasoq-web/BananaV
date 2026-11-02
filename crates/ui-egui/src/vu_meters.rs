//! The Mixing Console's VU/PPM meters on screen: the mixer's readings as
//! they are heard (`Engine::take_vu_levels`), each meter's highest and
//! lowest readings held and its clip light, drawn on the scale of the kind
//! picked (VEGAS's meter menu: Traditional, Extended and Logarithmic VU,
//! UK, EBU, DIN and Nordic PPM; labels, peaks, held peaks and valleys).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use bv_engine::meters::{MeterKind, MeterOf};

use crate::egui::{self, Align2, Color32, FontId, Rect, Stroke, pos2, vec2};
use crate::theme;

/// A meter as shown: each channel's reading (dBFS), its highest and lowest
/// since the clip lights were cleared, its clip light, and when it last
/// heard.
#[derive(Clone, Debug)]
pub(crate) struct Shown {
    levels: Vec<f32>,
    highs: Vec<f32>,
    lows: Vec<f32>,
    clipped: Vec<bool>,
    at: Instant,
}

/// Readings below this are silence (dBFS).
const SILENCE: f32 = -150.0;

fn to_db(x: f32) -> f32 {
    if x <= 1e-8 {
        SILENCE - 50.0
    } else {
        20.0 * x.log10()
    }
}

/// The meters as shown, by what they measure.
#[derive(Debug, Default)]
pub(crate) struct VuDisplay {
    meters: HashMap<MeterOf, Shown>,
    /// Readings held still (screen tests).
    held: bool,
}

impl VuDisplay {
    /// The readings heard since the last frame; meters that heard nothing
    /// for a while fall to rest.
    pub(crate) fn update(&mut self, readings: Vec<(MeterOf, Vec<f32>)>, now: Instant) {
        for (of, levels) in readings {
            let db: Vec<f32> = levels.iter().map(|x| to_db(*x)).collect();
            let n = db.len();
            let s = self.meters.entry(of).or_insert_with(|| Shown {
                levels: Vec::new(),
                highs: vec![SILENCE - 50.0; n],
                lows: vec![f32::INFINITY; n],
                clipped: vec![false; n],
                at: now,
            });
            if s.highs.len() != n {
                s.highs = vec![SILENCE - 50.0; n];
                s.lows = vec![f32::INFINITY; n];
                s.clipped = vec![false; n];
            }
            for (i, d) in db.iter().enumerate() {
                s.highs[i] = s.highs[i].max(*d);
                // Valleys: the lowest of what sounded (not the silences).
                if *d > -90.0 {
                    s.lows[i] = s.lows[i].min(*d);
                }
                s.clipped[i] |= *d >= 0.0;
            }
            s.levels = db;
            s.at = now;
        }
        for s in self.meters.values_mut() {
            if !self.held && now.duration_since(s.at) > Duration::from_millis(300) {
                s.levels.iter_mut().for_each(|l| *l = SILENCE - 50.0);
            }
        }
    }

    /// The readings stay as they are (for screen tests).
    pub(crate) fn hold(&mut self) {
        self.held = true;
    }

    /// Reset Clip: the clip lights out, the held peaks and valleys gone.
    pub(crate) fn reset_clips(&mut self) {
        for s in self.meters.values_mut() {
            s.clipped.iter_mut().for_each(|c| *c = false);
            s.highs.iter_mut().for_each(|h| *h = SILENCE - 50.0);
            s.lows.iter_mut().for_each(|l| *l = f32::INFINITY);
        }
    }

    /// Whether a meter moves (the window keeps drawing).
    pub(crate) fn active(&self) -> bool {
        self.meters
            .values()
            .any(|s| s.levels.iter().any(|l| *l > SILENCE))
    }

    pub(crate) fn get(&self, of: MeterOf) -> Option<&Shown> {
        self.meters.get(&of)
    }

    #[cfg(test)]
    pub(crate) fn level(&self, of: MeterOf, channel: usize) -> Option<f32> {
        self.meters.get(&of)?.levels.get(channel).copied()
    }
}

/// How the meters are drawn (the meters' menu).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct VuOptions {
    pub kind: MeterKind,
    pub labels: bool,
    pub peaks: bool,
    pub hold_peaks: bool,
    pub hold_valleys: bool,
}

/// A value in the meter's unit as its labels write it ("−7", "+3", "4").
fn mark_text(kind: MeterKind, v: f32) -> String {
    match kind {
        MeterKind::UkPpm => format!("{}", v.round()),
        _ if v > 0.0 => format!("+{}", v.round()),
        _ if v < 0.0 => format!("−{}", (-v).round()),
        _ => "0".to_string(),
    }
}

/// Draws a VU/PPM meter in `rect`: a bar per channel lit to its reading
/// (green, red above the scale's reference), the clip lights on top, the
/// held peaks and valleys as lines, the labels on the right and the
/// highest reading over it.
pub(crate) fn paint(ui: &egui::Ui, rect: Rect, shown: Option<&Shown>, o: &VuOptions) {
    let painter = ui.painter();
    painter.rect_filled(rect, 1.0, Color32::from_gray(0x14));
    let mut area = rect.shrink(2.0);
    let font = FontId::proportional(9.0);
    if o.peaks {
        let top = Rect::from_min_size(area.min, vec2(area.width(), 11.0));
        let high = shown
            .map(|s| s.highs.iter().copied().fold(SILENCE - 50.0, f32::max))
            .unwrap_or(SILENCE - 50.0);
        if high > SILENCE {
            let v = o.kind.value(high);
            let text = match o.kind {
                MeterKind::UkPpm => crate::fmt::num(v as f64, 1),
                _ => crate::fmt::signed(v as f64, 1),
            };
            painter.text(
                top.center(),
                Align2::CENTER_CENTER,
                text,
                font.clone(),
                theme::text(),
            );
        }
        area.min.y = top.bottom() + 1.0;
    }
    let scale_w = if o.labels { 18.0 } else { 0.0 };
    let bars = Rect::from_min_max(area.min, pos2(area.right() - scale_w, area.bottom()));
    let n = shown
        .map_or(2, |s| s.levels.len().max(s.highs.len()))
        .max(1);
    let w = ((bars.width() - (n as f32 - 1.0)) / n as f32).max(1.0);
    let y_of = |pos: f32| bars.bottom() - pos * bars.height();
    let reference = o.kind.place(o.kind.reference());
    let green = Color32::from_rgb(0x3C, 0xB4, 0x4B);
    let red = Color32::from_rgb(0xE5, 0x39, 0x35);
    for i in 0..n {
        let x0 = bars.left() + i as f32 * (w + 1.0);
        let bar = Rect::from_min_size(pos2(x0, bars.top()), vec2(w, bars.height()));
        painter.rect_filled(bar, 0.0, Color32::from_gray(0x24));
        let Some(s) = shown else {
            continue;
        };
        let level = s.levels.get(i).copied().unwrap_or(SILENCE - 50.0);
        let pos = o.kind.position(level);
        if pos > 0.0 {
            let lit = pos.min(reference);
            painter.rect_filled(
                Rect::from_min_max(pos2(bar.left(), y_of(lit)), bar.max),
                0.0,
                green,
            );
            if pos > reference {
                painter.rect_filled(
                    Rect::from_min_max(
                        pos2(bar.left(), y_of(pos)),
                        pos2(bar.right(), y_of(reference)),
                    ),
                    0.0,
                    red,
                );
            }
        }
        if o.hold_peaks
            && let Some(h) = s.highs.get(i).filter(|h| **h > SILENCE)
        {
            painter.hline(
                bar.x_range(),
                y_of(o.kind.position(*h)),
                Stroke::new(1.0, Color32::from_gray(0xE6)),
            );
        }
        if o.hold_valleys
            && let Some(l) = s.lows.get(i).filter(|l| l.is_finite())
        {
            painter.hline(
                bar.x_range(),
                y_of(o.kind.position(*l)),
                Stroke::new(1.0, theme::banana()),
            );
        }
        if s.clipped.get(i).copied().unwrap_or(false) {
            painter.rect_filled(
                Rect::from_min_size(bar.min, vec2(bar.width(), 3.0)),
                0.0,
                red,
            );
        }
    }
    if o.labels {
        let grey = Color32::from_gray(0x8C);
        let mut last: Option<f32> = None;
        for &m in o.kind.marks().iter().rev() {
            let y = y_of(o.kind.place(m));
            painter.hline(
                (bars.right() + 1.0)..=(bars.right() + 3.0),
                y.round(),
                Stroke::new(1.0, grey),
            );
            // Marks too close together: every other label.
            if last.is_some_and(|l| (y - l).abs() < 9.0) {
                continue;
            }
            last = Some(y);
            painter.text(
                pos2(rect.right() - 1.0, y),
                Align2::RIGHT_CENTER,
                mark_text(o.kind, m),
                font.clone(),
                grey,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Readings come in as amplitudes and show as dBFS; the clip light
    /// stays lit and the peak held until Reset Clip; meters that hear
    /// nothing fall to rest.
    #[test]
    fn readings_hold_their_peaks_and_fall_to_rest() {
        let mut d = VuDisplay::default();
        let t0 = Instant::now();
        d.update(vec![(MeterOf::Master, vec![1.0, 0.5])], t0);
        assert!((d.level(MeterOf::Master, 1).unwrap() + 6.0206).abs() < 1e-3);
        d.update(vec![(MeterOf::Master, vec![0.1, 0.1])], t0);
        let s = d.get(MeterOf::Master).unwrap();
        assert!(s.clipped[0] && !s.clipped[1]);
        assert!(s.highs[0].abs() < 1e-4);
        assert!(d.active());
        d.update(Vec::new(), t0 + Duration::from_secs(1));
        assert!(!d.active());
        d.reset_clips();
        assert!(!d.get(MeterOf::Master).unwrap().clipped[0]);
    }
}
