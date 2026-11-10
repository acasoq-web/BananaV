//! Video Scopes (View › Window), as VEGAS 23's: the toolbar's list of
//! views (each scope alone, or several side by side), its settings and
//! Update Scopes While Playing, and each scope under a bar with its name
//! and its mode. The scales are drawn whether a frame is shown or not —
//! the vectorscope's rings, I and Q axes, skin tone line and the 75 %
//! color bars' targets; the waveform's IRE lines and the bars' levels; the
//! histogram's and the RGB parade's 0–255 lines — and the frame of the
//! Video Preview over them.
//!
//! Measured on VEGAS's screenshots: the vectorscope plots NTSC's
//! U = 0.492 (B − Y′) and V = 0.877 (R − Y′), Y′ being BT.601's luma; its
//! 100 ring is a chroma of 0.5, its outer ring 120 with a tick a degree,
//! its targets span ±10° and ±20 % around the 75 % bars' colors, drawn as
//! brackets (each side's first and last thirds). The waveform shows that
//! luma from −20 to 120 IRE, the bars' levels a seventh of its width each.
//! Their colors are the scales' own, on black whatever the interface's
//! colors, as VEGAS's.
//!
//! The traces are VEGAS's too, measured on its screenshots of the
//! calibration card: the waveform and the vectorscope read the last row of
//! each 8 of the frame, the RGB parade the last of each 9, the histogram
//! every pixel (the card's mean and deviation are the whole picture's);
//! the waveform and the parade one column of the frame for each of theirs,
//! the vectorscope the first of each 8.
//! The waveform lights a point a quarter of the way to grey 220 for each
//! pixel there, the parade sets its channel's color, the vectorscope draws
//! a small cross per pixel in a color of its chroma, and the histogram's
//! bars rise with the logarithm of their count, the whole picture's at the
//! top. They read the Video Preview's frame, as VEGAS's: at Best (Full),
//! the project's whole picture.

use std::sync::Arc;

use bv_engine::PreviewFrame;
use serde::{Deserialize, Serialize};

use crate::app::BananaApp;
use crate::egui::{
    self, Align2, Color32, ColorImage, FontId, Painter, Pos2, Rect, Sense, Shape, Stroke,
    TextureHandle, TextureOptions, pos2, vec2,
};
use crate::i18n::tr;
use crate::icons::Icon;
use crate::theme;
use crate::widgets::{icon_button, icon_toggle};

/// The views of the toolbar's list, in its order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum View {
    #[default]
    Vectorscope,
    Waveform,
    Histogram,
    RgbParade,
    VectorscopeWaveform,
    VectorscopeWaveformHistogram,
    VectorscopeRgbParade,
    All,
}

impl View {
    pub const ALL: [View; 8] = [
        View::Vectorscope,
        View::Waveform,
        View::Histogram,
        View::RgbParade,
        View::VectorscopeWaveform,
        View::VectorscopeWaveformHistogram,
        View::VectorscopeRgbParade,
        View::All,
    ];

    pub fn name(self) -> &'static str {
        match self {
            View::Vectorscope => tr("Vectorscope"),
            View::Waveform => tr("Waveform"),
            View::Histogram => tr("Histogram"),
            View::RgbParade => tr("RGB Parade"),
            View::VectorscopeWaveform => tr("Vectorscope/Waveform"),
            View::VectorscopeWaveformHistogram => tr("Vectorscope/Waveform/Histogram"),
            View::VectorscopeRgbParade => tr("Vectorscope/RGB Parade"),
            View::All => tr("All"),
        }
    }

    /// Its scopes, a row at a time: two side by side, the histogram under
    /// them, or two over two.
    fn rows(self) -> &'static [&'static [Scope]] {
        use Scope as S;
        match self {
            View::Vectorscope => &[&[S::Vectorscope]],
            View::Waveform => &[&[S::Waveform]],
            View::Histogram => &[&[S::Histogram]],
            View::RgbParade => &[&[S::Parade]],
            View::VectorscopeWaveform => &[&[S::Vectorscope, S::Waveform]],
            View::VectorscopeWaveformHistogram => {
                &[&[S::Vectorscope, S::Waveform], &[S::Histogram]]
            }
            View::VectorscopeRgbParade => &[&[S::Vectorscope, S::Parade]],
            View::All => &[&[S::Vectorscope, S::Waveform], &[S::Histogram, S::Parade]],
        }
    }
}

/// The waveform's modes: the luma alone, or with the chroma as a
/// composite signal carries it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WaveformMode {
    #[default]
    Luminance,
    Composite,
}

impl WaveformMode {
    pub const ALL: [WaveformMode; 2] = [WaveformMode::Luminance, WaveformMode::Composite];

    fn name(self) -> &'static str {
        match self {
            WaveformMode::Luminance => tr("Luminance"),
            WaveformMode::Composite => tr("Composite"),
        }
    }
}

/// What the histogram counts: one channel, or several stacked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HistogramChannel {
    #[default]
    Luminance,
    Red,
    Green,
    Blue,
    Alpha,
    /// Luminance, red, green and blue stacked.
    LuminanceRgb,
    /// L*a*b*'s lightness, a and b stacked.
    Lab,
}

impl HistogramChannel {
    pub const ALL: [HistogramChannel; 7] = [
        HistogramChannel::Luminance,
        HistogramChannel::Red,
        HistogramChannel::Green,
        HistogramChannel::Blue,
        HistogramChannel::Alpha,
        HistogramChannel::LuminanceRgb,
        HistogramChannel::Lab,
    ];

    fn name(self) -> &'static str {
        match self {
            HistogramChannel::Luminance => tr("Luminance"),
            HistogramChannel::Red => tr("Red"),
            HistogramChannel::Green => tr("Green"),
            HistogramChannel::Blue => tr("Blue"),
            HistogramChannel::Alpha => tr("Alpha"),
            HistogramChannel::LuminanceRgb => tr("Luminance/R/G/B"),
            HistogramChannel::Lab => tr("Lightness/a/b"),
        }
    }

    /// The charts it stacks, top to bottom (`Levels`' channels).
    fn charts(self) -> &'static [usize] {
        match self {
            HistogramChannel::Luminance => &[LUMA],
            HistogramChannel::Red => &[RED],
            HistogramChannel::Green => &[GREEN],
            HistogramChannel::Blue => &[BLUE],
            HistogramChannel::Alpha => &[ALPHA],
            HistogramChannel::LuminanceRgb => &[LUMA, RED, GREEN, BLUE],
            HistogramChannel::Lab => &[LAB_L, LAB_A, LAB_B],
        }
    }
}

/// The RGB parade's modes: the channels side by side, or over one another
/// (each shown or hidden by its R, G and B buttons).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParadeMode {
    #[default]
    Separate,
    Combined,
}

impl ParadeMode {
    pub const ALL: [ParadeMode; 2] = [ParadeMode::Separate, ParadeMode::Combined];

    fn name(self) -> &'static str {
        match self {
            ParadeMode::Separate => tr("Separate"),
            ParadeMode::Combined => tr("Combined"),
        }
    }
}

/// Video Scope Settings (the toolbar's gear).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScopeSettings {
    /// The vectorscope's skin tone line, and its angle (degrees,
    /// counter-clockwise from the blue side of the horizontal axis; 123 is
    /// NTSC vectorscopes').
    pub skin_tone_line: bool,
    pub skin_tone_angle: f32,
    /// 7.5 IRE Setup: black at 7.5 IRE on the waveform, as scopes on
    /// hardware that adds setup show it (NTSC in the US).
    pub setup: bool,
    /// Studio RGB (16 to 235): 16 shows as black (0 IRE) and 235 as white,
    /// chroma as far out; on, as VEGAS's scopes show the card. (Its key
    /// changed when it took that default: settings saved before take it.)
    #[serde(rename = "studio_rgb_levels")]
    pub studio_rgb: bool,
}

impl Default for ScopeSettings {
    fn default() -> ScopeSettings {
        ScopeSettings {
            skin_tone_line: true,
            skin_tone_angle: 123.0,
            setup: false,
            studio_rgb: true,
        }
    }
}

/// The Video Scopes' view, modes and settings, kept between sessions as
/// VEGAS keeps them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScopePrefs {
    pub view: View,
    pub waveform: WaveformMode,
    pub histogram: HistogramChannel,
    pub parade: ParadeMode,
    /// The combined parade's R, G and B buttons.
    pub parade_channels: [bool; 3],
    /// Update Scopes While Playing: off, the scopes keep the frame they had
    /// when playback started.
    pub while_playing: bool,
    pub settings: ScopeSettings,
}

impl Default for ScopePrefs {
    fn default() -> ScopePrefs {
        ScopePrefs {
            view: View::default(),
            waveform: WaveformMode::default(),
            histogram: HistogramChannel::default(),
            parade: ParadeMode::default(),
            parade_channels: [true; 3],
            while_playing: false,
            settings: ScopeSettings::default(),
        }
    }
}

/// A scope of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Scope {
    Vectorscope,
    Waveform,
    Histogram,
    Parade,
}

impl Scope {
    fn name(self) -> &'static str {
        match self {
            Scope::Vectorscope => tr("Vectorscope"),
            Scope::Waveform => tr("Waveform"),
            Scope::Histogram => tr("Histogram"),
            Scope::Parade => tr("RGB Parade"),
        }
    }
}

/// The scales' colors, measured on VEGAS's: grey for the main lines and
/// their numbers, orange between them, a dimmer orange between those.
const GREY: Color32 = Color32::from_rgb(128, 128, 128);
const ORANGE: Color32 = Color32::from_rgb(165, 112, 0);
const DIM_ORANGE: Color32 = Color32::from_rgb(110, 75, 0);
/// The 75 % bars' levels on the waveform.
const BARS_GREEN: Color32 = Color32::from_rgb(91, 176, 15);
/// The Color Grading window's RL thresholds drawn as guides.
const LIMITS: Color32 = Color32::from_rgb(80, 200, 230);
/// The waveform's trace and the histogram's bars.
const TRACE_GREY: u8 = 220;
/// The parade's channels and the histogram's red, green and blue.
const CHANNELS: [Color32; 3] = [
    Color32::from_rgb(235, 0, 0),
    Color32::from_rgb(0, 235, 0),
    Color32::from_rgb(0, 0, 235),
];

/// The chroma at the vectorscope's 100 ring.
const FULL_CHROMA: f32 = 0.5;
/// The vectorscope's outer ring, in parts of its 100 ring.
const OUTER: f32 = 1.2;
/// The I and Q axes' angles (degrees).
const I_AXIS: f32 = 123.0;
const Q_AXIS: f32 = 33.0;

/// The vectorscope's targets: each 75 % bar's color, its label and the
/// color it is drawn in.
const TARGETS: [([f32; 3], &str, Color32); 6] = [
    ([1.0, 0.0, 0.0], "R", Color32::from_rgb(235, 0, 0)),
    ([1.0, 0.0, 1.0], "Mg", Color32::from_rgb(235, 0, 235)),
    ([0.0, 0.0, 1.0], "B", Color32::from_rgb(0, 0, 235)),
    ([0.0, 1.0, 1.0], "Cy", Color32::from_rgb(0, 235, 235)),
    ([0.0, 1.0, 0.0], "G", Color32::from_rgb(0, 235, 0)),
    ([1.0, 1.0, 0.0], "Yl", Color32::from_rgb(235, 235, 0)),
];

/// The color bars, left to right: white, yellow, cyan, green, magenta,
/// red and blue.
const BARS: [[f32; 3]; 7] = [
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 0.0],
    [0.0, 1.0, 1.0],
    [0.0, 1.0, 0.0],
    [1.0, 0.0, 1.0],
    [1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0],
];

/// BT.601's luma of video values (0–1), as VEGAS's scopes take it.
fn luma(r: f32, g: f32, b: f32) -> f32 {
    0.299 * r + 0.587 * g + 0.114 * b
}

/// NTSC's U and V of video values (0–1).
fn chroma(r: f32, g: f32, b: f32) -> (f32, f32) {
    let y = luma(r, g, b);
    (0.492 * (b - y), 0.877 * (r - y))
}

/// Where the 75 % bar of a color sits on the vectorscope: its angle
/// (degrees, counter-clockwise from +U) and its distance in parts of the
/// 100 ring.
fn target(rgb: [f32; 3]) -> (f32, f32) {
    let (u, v) = chroma(0.75 * rgb[0], 0.75 * rgb[1], 0.75 * rgb[2]);
    (v.atan2(u).to_degrees(), u.hypot(v) / FULL_CHROMA)
}

/// The IRE a luma (0–1 of the frame's values) shows at: Studio RGB takes
/// 16 as black and 235 as white, 7.5 IRE Setup puts black at 7.5.
fn ire(y: f32, s: &ScopeSettings) -> f32 {
    let y = if s.studio_rgb {
        (y * 255.0 - 16.0) / 219.0
    } else {
        y
    };
    with_setup(y * 100.0, s)
}

/// An IRE level as 7.5 IRE Setup shows it.
fn with_setup(ire: f32, s: &ScopeSettings) -> f32 {
    if s.setup { 7.5 + 0.925 * ire } else { ire }
}

/// The 75 % bars' levels on the waveform, left to right (IRE).
fn bar_levels(s: &ScopeSettings) -> [f32; 7] {
    BARS.map(|[r, g, b]| with_setup(75.0 * luma(r, g, b), s))
}

/// The color of the vectorscope's outer ring at an angle: the hue that
/// points there, between the six bars' colors.
fn hue_at(deg: f32) -> Color32 {
    const HUES: [(f32, [f32; 3]); 6] = [
        (60.7, [1.0, 0.0, 1.0]),
        (103.5, [1.0, 0.0, 0.0]),
        (167.1, [1.0, 1.0, 0.0]),
        (240.7, [0.0, 1.0, 0.0]),
        (283.5, [0.0, 1.0, 1.0]),
        (347.1, [0.0, 0.0, 1.0]),
    ];
    let deg = (deg - HUES[0].0).rem_euclid(360.0) + HUES[0].0;
    let i = HUES.iter().rposition(|h| h.0 <= deg).unwrap_or(0);
    let (a0, c0) = HUES[i];
    let (a1, c1) = match HUES.get(i + 1) {
        Some(&h) => h,
        None => (HUES[0].0 + 360.0, HUES[0].1),
    };
    let t = (deg - a0) / (a1 - a0);
    let c = |k: usize| ((c0[k] + (c1[k] - c0[k]) * t) * 235.0) as u8;
    Color32::from_rgb(c(0), c(1), c(2))
}

/// The vectorscope's dial in a plot: its middle and its 100 ring's
/// radius, the outer ring touching the plot's nearer sides.
#[derive(Clone, Copy, Debug)]
struct Dial {
    c: Pos2,
    r100: f32,
}

impl Dial {
    fn of(plot: Rect) -> Dial {
        let outer = (plot.width().min(plot.height()) / 2.0 - 0.5).max(8.0);
        Dial {
            c: plot.center(),
            r100: outer / OUTER,
        }
    }

    /// The point at `deg` (counter-clockwise from +U) and `parts` of the
    /// 100 ring.
    fn at(&self, deg: f32, parts: f32) -> Pos2 {
        let a = deg.to_radians();
        self.c + vec2(a.cos(), -a.sin()) * parts * self.r100
    }
}

/// A scale's place in a plot: the lines run from `left` to `right`, the
/// highest value at `top`, the lowest at `bottom`.
#[derive(Clone, Copy, Debug)]
struct Scale {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
    low: f32,
    high: f32,
}

impl Scale {
    /// The waveform's: −20 to 120 IRE, numbered on the left.
    fn waveform(plot: Rect) -> Scale {
        let margin = plot.height() * 0.0635;
        Scale {
            left: plot.left() + 19.0,
            right: plot.right() - 1.0,
            top: plot.top() + margin,
            bottom: plot.bottom() - margin,
            low: -20.0,
            high: 120.0,
        }
    }

    /// The parade's: 0 to 255, numbered on the left.
    fn parade(plot: Rect) -> Scale {
        let margin = plot.height() * 0.0588;
        Scale {
            left: plot.left() + 39.0,
            right: plot.right() - 1.0,
            top: plot.top() + margin,
            bottom: plot.bottom() - margin,
            low: 0.0,
            high: 255.0,
        }
    }

    fn y(&self, v: f32) -> f32 {
        self.bottom - (v - self.low) / (self.high - self.low) * (self.bottom - self.top)
    }

    fn value(&self, y: f32) -> f32 {
        self.low + (self.bottom - y) / (self.bottom - self.top) * (self.high - self.low)
    }
}

/// The histogram's place in a plot: 0 to 255 across, numbered on top.
#[derive(Clone, Copy, Debug)]
struct Across {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

impl Across {
    fn of(plot: Rect) -> Across {
        Across {
            left: plot.left() + plot.width() * 0.0557,
            right: plot.right() - plot.width() * 0.0594,
            top: plot.top() + 15.0,
            bottom: plot.bottom() - 16.0,
        }
    }

    fn x(&self, v: f32) -> f32 {
        self.left + v / 255.0 * (self.right - self.left)
    }
}

/// The lines of a 0–255 scale: every 16 and 255, grey and numbered at 0,
/// 64, 128, 192 and 255.
fn levels_lines() -> impl Iterator<Item = (f32, Color32, bool)> {
    (0..=16).map(|k| {
        let v = (k * 16).min(255);
        let main = v % 64 == 0 || v == 255;
        (v as f32, if main { GREY } else { DIM_ORANGE }, main)
    })
}

fn scale_font() -> FontId {
    FontId::proportional(11.0)
}

/// The vectorscope's dial: the rings numbered 20 to 100, the axes, I and
/// Q dashed, the skin tone line, the outer ring of hues with its ticks and
/// the six targets.
fn paint_vectorscope(painter: &Painter, plot: Rect, s: &ScopeSettings) {
    let d = Dial::of(plot);
    let grey = Stroke::new(1.0, GREY);
    for k in 1..=5 {
        let r = d.r100 * k as f32 / 5.0;
        painter.circle_stroke(d.c, r, grey);
        painter.text(
            pos2(d.c.x + 4.0, d.c.y - r - 2.0),
            Align2::LEFT_BOTTOM,
            (20 * k).to_string(),
            scale_font(),
            GREY,
        );
    }
    // The axes cross the whole scope, apart near the middle.
    let gap = 0.3 * d.r100;
    painter.hline(plot.left() + 2.0..=d.c.x - gap, d.c.y, grey);
    painter.hline(d.c.x + gap..=plot.right() - 2.0, d.c.y, grey);
    painter.vline(d.c.x, plot.top() + 2.0..=d.c.y - gap, grey);
    painter.vline(d.c.x, d.c.y + gap..=plot.bottom() - 2.0, grey);
    for deg in [I_AXIS, I_AXIS + 180.0, Q_AXIS, Q_AXIS + 180.0] {
        painter.extend(Shape::dashed_line(&[d.c, d.at(deg, OUTER)], grey, 6.0, 4.0));
    }
    if s.skin_tone_line {
        painter.line_segment(
            [d.c, d.at(s.skin_tone_angle, OUTER)],
            Stroke::new(1.0, ORANGE),
        );
    }
    // The outer ring: a tick a degree, longer every five, and the hues.
    let outer = d.r100 * OUTER;
    let k = (outer / 216.0).clamp(0.5, 2.0);
    painter.circle_stroke(d.c, outer, grey);
    for deg in 0..360 {
        let len = if deg % 5 == 0 { 11.0 } else { 5.0 } * k;
        let a = (deg as f32).to_radians();
        let dir = vec2(a.cos(), -a.sin());
        painter.line_segment([d.c + dir * (outer - len), d.c + dir * outer], grey);
    }
    for i in 0..120 {
        let (a0, a1) = (i as f32 * 3.0, i as f32 * 3.0 + 3.0);
        let pts: Vec<Pos2> = (0..=3)
            .map(|j| d.at(a0 + (a1 - a0) * j as f32 / 3.0, OUTER + 1.0 / d.r100))
            .collect();
        painter.add(Shape::line(pts, Stroke::new(1.5, hue_at(a0 + 1.5))));
    }
    // The targets: ±10° and ±20 % around each 75 % bar, each side's first
    // and last thirds drawn.
    for (rgb, label, color) in TARGETS {
        let (deg, parts) = target(rgb);
        let st = Stroke::new(1.5, color);
        let (inner, out) = (parts * 0.8, parts * 1.2);
        let (a0, a1) = (deg - 10.0, deg + 10.0);
        let arc = |r: f32, from: f32, to: f32| -> Vec<Pos2> {
            (0..=6)
                .map(|i| d.at(from + (to - from) * i as f32 / 6.0, r))
                .collect()
        };
        for r in [inner, out] {
            painter.add(Shape::line(arc(r, a0, a0 + 20.0 / 3.0), st));
            painter.add(Shape::line(arc(r, a1 - 20.0 / 3.0, a1), st));
        }
        let third = (out - inner) / 3.0;
        for a in [a0, a1] {
            painter.line_segment([d.at(a, inner), d.at(a, inner + third)], st);
            painter.line_segment([d.at(a, out - third), d.at(a, out)], st);
        }
        painter.text(
            d.at(deg, parts),
            Align2::CENTER_CENTER,
            label,
            scale_font(),
            color,
        );
    }
}

/// The waveform's scale: a line every 10 IRE (grey at 0 and 100, orange
/// every 20, dimmer between), numbered every 20, and the 75 % bars'
/// levels.
fn paint_waveform(painter: &Painter, plot: Rect, s: &ScopeSettings) {
    let sc = Scale::waveform(plot);
    for k in -2..=12 {
        let v = k as f32 * 10.0;
        let color = match k {
            0 | 10 => GREY,
            _ if k % 2 == 0 => ORANGE,
            _ => DIM_ORANGE,
        };
        let y = sc.y(v);
        painter.hline(sc.left..=sc.right, y, Stroke::new(1.0, color));
        if k % 2 == 0 {
            painter.text(
                pos2(sc.left - 1.0, y),
                Align2::RIGHT_CENTER,
                format!("{}", k * 10),
                scale_font(),
                GREY,
            );
        }
    }
    let w = (sc.right - sc.left) / 7.0;
    for (i, level) in bar_levels(s).into_iter().enumerate() {
        let x = sc.left + w * i as f32;
        painter.hline(x..=x + w, sc.y(level), Stroke::new(1.0, BARS_GREEN));
    }
}

/// A 0–255 scale's lines across the parade, numbered on the left.
fn paint_parade(painter: &Painter, plot: Rect) {
    let sc = Scale::parade(plot);
    for (v, color, main) in levels_lines() {
        let y = sc.y(v);
        painter.hline(sc.left..=sc.right, y, Stroke::new(1.0, color));
        if main {
            painter.text(
                pos2(sc.left - 4.0, y),
                Align2::RIGHT_CENTER,
                format!("{v}"),
                scale_font(),
                GREY,
            );
        }
    }
}

/// A 0–255 scale's lines down the histogram, numbered on top, and the
/// charts over them.
fn paint_histogram(
    painter: &Painter,
    plot: Rect,
    channel: HistogramChannel,
    levels: Option<&Levels>,
) {
    let a = Across::of(plot);
    for (v, color, main) in levels_lines() {
        let x = a.x(v);
        painter.vline(x, a.top..=a.bottom, Stroke::new(1.0, color));
        if main {
            painter.text(
                pos2(x, plot.top() + 7.5),
                Align2::CENTER_CENTER,
                format!("{v}"),
                scale_font(),
                GREY,
            );
        }
    }
    let Some(levels) = levels else {
        return;
    };
    // Each chart its share of the lines' height; a bar's height the
    // logarithm of its count, the whole picture's at the top (VEGAS's: the
    // card's black, half the frame, reaches 95 %).
    let charts = channel.charts();
    let band = (a.bottom - a.top) / charts.len() as f32;
    let step = (a.right - a.left) / 255.0;
    for (i, &chart) in charts.iter().enumerate() {
        let bottom = a.top + band * (i + 1) as f32;
        let counts = &levels.counts[chart];
        let full = (1.0 + levels.totals[chart] as f32)
            .ln()
            .max(f32::MIN_POSITIVE);
        let mut mesh = egui::Mesh::default();
        for (v, &n) in counts.iter().enumerate() {
            if n == 0 {
                continue;
            }
            let h = band * (1.0 + n as f32).ln() / full;
            let x = a.x(v as f32);
            mesh.add_colored_rect(
                Rect::from_min_max(
                    pos2(x - step / 2.0, bottom - h),
                    pos2(x + step / 2.0, bottom),
                ),
                chart_color(chart),
            );
        }
        painter.add(mesh);
        // The mean and the standard deviation under the chart, from 0 and
        // from 128, as VEGAS writes them (stacked charts: over each).
        if let Some((mean, deviation)) = statistics(counts) {
            let (mean, deviation) = match chart {
                LAB_L => (mean / 2.55, deviation / 2.55),
                LAB_A | LAB_B => (mean - 128.0, deviation),
                _ => (mean, deviation),
            };
            let y = if charts.len() == 1 {
                plot.bottom() - 6.5
            } else {
                bottom - band + 7.0
            };
            for (x, text) in [
                (a.x(0.0), trf!("Mean: {}", crate::fmt::num(mean as f64, 2))),
                (
                    a.x(128.0),
                    trf!(
                        "Standard Deviation: {}",
                        crate::fmt::num(deviation as f64, 2)
                    ),
                ),
            ] {
                let galley = painter.layout_no_wrap(text, scale_font(), GREY);
                let at = pos2(x, y - galley.size().y / 2.0);
                if charts.len() > 1 {
                    let r = Rect::from_min_size(at, galley.size()).expand(1.0);
                    painter.rect_filled(r, 1.0, Color32::from_black_alpha(170));
                }
                painter.galley(at, galley, GREY);
            }
        }
    }
}

/// The color of a histogram's chart: VEGAS's grey for the luminance.
fn chart_color(chart: usize) -> Color32 {
    match chart {
        RED => CHANNELS[0],
        GREEN => CHANNELS[1],
        BLUE => CHANNELS[2],
        LAB_A => Color32::from_rgb(220, 0, 220),
        LAB_B => Color32::from_rgb(220, 220, 0),
        _ => Color32::from_gray(TRACE_GREY),
    }
}

/// The mean and the standard deviation of a channel's levels (`None`
/// without pixels).
fn statistics(counts: &[u32; 256]) -> Option<(f32, f32)> {
    let n: f64 = counts.iter().map(|&c| c as f64).sum();
    if n == 0.0 {
        return None;
    }
    let mean = counts
        .iter()
        .enumerate()
        .map(|(v, &c)| v as f64 * c as f64)
        .sum::<f64>()
        / n;
    let variance = counts
        .iter()
        .enumerate()
        .map(|(v, &c)| (v as f64 - mean).powi(2) * c as f64)
        .sum::<f64>()
        / n;
    Some((mean as f32, variance.sqrt() as f32))
}

// The histogram's channels (`Levels::counts`).
const LUMA: usize = 0;
const RED: usize = 1;
const GREEN: usize = 2;
const BLUE: usize = 3;
const ALPHA: usize = 4;
const LAB_L: usize = 5;
const LAB_A: usize = 6;
const LAB_B: usize = 7;

/// A frame's pixels at each level (0–255) of each channel: luminance,
/// red, green, blue, alpha, and L*a*b*'s lightness, a and b as 8-bit Lab
/// keeps them (L × 2.55, a and b + 128); and how many each counted.
#[derive(Clone, Debug)]
struct Levels {
    counts: [[u32; 256]; 8],
    totals: [u64; 8],
}

/// A frame's pixels, when it holds as many as it says.
struct Pixels<'a> {
    rgba: &'a [u8],
    width: usize,
    height: usize,
}

impl<'a> Pixels<'a> {
    fn of(frame: &'a PreviewFrame) -> Option<Pixels<'a>> {
        let (width, height) = (frame.width as usize, frame.height as usize);
        (width > 0 && height > 0 && frame.rgba.len() >= width * height * 4).then_some(Pixels {
            rgba: &frame.rgba,
            width,
            height,
        })
    }

    /// The pixel's values (0–255).
    fn at(&self, x: usize, y: usize) -> [u8; 4] {
        let i = (y * self.width + x) * 4;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }

    /// The frame's column for column `c` of `cols` across a scope.
    fn column(&self, c: usize, cols: usize) -> usize {
        ((c as f32 * self.width as f32 / cols as f32).round() as usize).min(self.width - 1)
    }
}

/// The rows VEGAS's scopes read: the last of each `every`.
fn sampled_rows(height: usize, every: usize) -> impl Iterator<Item = usize> {
    (every.min(height).max(1) - 1..height).step_by(every)
}

/// Values 0–255 as 0–1.
fn unit([r, g, b, _]: [u8; 4]) -> (f32, f32, f32) {
    (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}

/// A plot's size in pixels.
fn plot_pixels(plot: Rect, ppp: f32) -> [usize; 2] {
    [
        (plot.width() * ppp).round().max(1.0) as usize,
        (plot.height() * ppp).round().max(1.0) as usize,
    ]
}

/// One more pixel at a point of the waveform: a quarter of the way to its
/// grey, in whole steps (one point 55, two 96, four 150, at most 217).
fn brighten(level: u8) -> u8 {
    level + (TRACE_GREY - level) / 4
}

/// The waveform's trace over its plot, pixel for pixel: a column of the
/// frame for each of the scale's, its pixels lit where their luma shows.
fn waveform_image(
    frame: &PreviewFrame,
    mode: WaveformMode,
    s: &ScopeSettings,
    plot: Rect,
    ppp: f32,
) -> ColorImage {
    let [w, h] = plot_pixels(plot, ppp);
    let mut level = vec![0u8; w * h];
    let sc = Scale::waveform(plot);
    let x0 = ((sc.left - plot.left()) * ppp).round() as usize;
    let x1 = (((sc.right - plot.left()) * ppp).round() as usize).min(w);
    let cols = x1.saturating_sub(x0);
    if let Some(px) = Pixels::of(frame)
        && cols > 0
    {
        let row_at = |ire: f32| ((sc.y(ire) - plot.top()) * ppp + 0.25).ceil();
        for y in sampled_rows(px.height, 8) {
            for c in 0..cols {
                let (r, g, b) = unit(px.at(px.column(c, cols), y));
                let mut hit = |ire: f32| {
                    let row = row_at(ire);
                    if row >= 0.0 && (row as usize) < h {
                        let i = row as usize * w + x0 + c;
                        level[i] = brighten(level[i]);
                    }
                };
                let luma = luma(r, g, b);
                match mode {
                    WaveformMode::Luminance => hit(ire(luma, s)),
                    // The chroma rides on the luma: the signal swings that
                    // far either way.
                    WaveformMode::Composite => {
                        let (u, v) = chroma(r, g, b);
                        let c = u.hypot(v);
                        for k in [-1.0, -0.7, 0.0, 0.7, 1.0] {
                            hit(ire(luma + k * c, s));
                        }
                    }
                }
            }
        }
    }
    let pixels = level
        .into_iter()
        .map(|l| {
            let alpha = (l as u32 * 255 / TRACE_GREY as u32) as u8;
            Color32::from_rgba_unmultiplied(TRACE_GREY, TRACE_GREY, TRACE_GREY, alpha)
        })
        .collect();
    ColorImage::new([w, h], pixels)
}

/// A vectorscope dot's color, as VEGAS's (to an 8-bit step on the card's
/// ramps): the pixel's U and V taken as Cb and Cr at nine tenths, around
/// grey.
fn dot_color(u: f32, v: f32) -> [u8; 3] {
    let (u, v) = (u * 255.0 * 0.91, v * 255.0 * 0.91);
    let c = |x: f32| (127.0 + x).round().clamp(0.0, 255.0) as u8;
    [
        c(1.402 * v),
        c(-0.344_136 * u - 0.714_136 * v),
        c(1.772 * u),
    ]
}

/// The vectorscope's trace over its plot, pixel for pixel: a cross for
/// each pixel read — the first of each 8 across, of the last row of each
/// 8 — its middle the dot's color, its arms half of it, at the pixel's
/// chroma, beyond the outer ring too.
fn vectorscope_image(frame: &PreviewFrame, s: &ScopeSettings, plot: Rect, ppp: f32) -> ColorImage {
    let [w, h] = plot_pixels(plot, ppp);
    let mut rgb = vec![[0u8; 3]; w * h];
    // 1: an arm, 2: a dot's middle (arms never cover one).
    let mut kind = vec![0u8; w * h];
    if let Some(px) = Pixels::of(frame) {
        let d = Dial::of(plot);
        // The middle pixel's corner, as VEGAS places its dots.
        let (cx, cy) = (
            (d.c.x - plot.left()) * ppp - 0.5,
            (d.c.y - plot.top()) * ppp - 0.5,
        );
        let gain = if s.studio_rgb { 255.0 / 219.0 } else { 1.0 };
        let scale = d.r100 * ppp / FULL_CHROMA * gain;
        for y in sampled_rows(px.height, 8) {
            for x in (0..px.width).step_by(8) {
                let (r, g, b) = unit(px.at(x, y));
                let (u, v) = chroma(r, g, b);
                let (dx, dy) = ((cx + u * scale).round(), (cy - v * scale).round());
                if dx < -1.0 || dy < -1.0 || dx > w as f32 || dy > h as f32 {
                    continue;
                }
                let color = dot_color(u, v);
                let half = color.map(|c| c / 2);
                for (ox, oy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                    let (ax, ay) = (dx + ox, dy + oy);
                    if ax >= 0.0 && ay >= 0.0 && (ax as usize) < w && (ay as usize) < h {
                        let i = ay as usize * w + ax as usize;
                        if kind[i] < 2 {
                            rgb[i] = [0, 1, 2].map(|k| rgb[i][k].max(half[k]));
                            kind[i] = 1;
                        }
                    }
                }
                if dx >= 0.0 && dy >= 0.0 && (dx as usize) < w && (dy as usize) < h {
                    let i = dy as usize * w + dx as usize;
                    rgb[i] = color;
                    kind[i] = 2;
                }
            }
        }
    }
    let pixels = rgb
        .into_iter()
        .zip(kind)
        .map(|([r, g, b], k)| {
            if k == 0 {
                Color32::TRANSPARENT
            } else {
                Color32::from_rgb(r, g, b)
            }
        })
        .collect();
    ColorImage::new([w, h], pixels)
}

/// The parade's trace over its plot, pixel for pixel: separate, three
/// parts two points apart, a column of the frame for each of a part's,
/// each pixel setting its channel's color where its value shows;
/// combined, the channels shown over one another.
fn parade_image(
    frame: &PreviewFrame,
    mode: ParadeMode,
    shown: [bool; 3],
    plot: Rect,
    ppp: f32,
) -> ColorImage {
    let [w, h] = plot_pixels(plot, ppp);
    // The channels lit at each pixel (bits 1, 2 and 4).
    let mut lit = vec![0u8; w * h];
    let sc = Scale::parade(plot);
    let x0 = ((sc.left + 1.0 - plot.left()) * ppp).round() as usize;
    let x1 = (((sc.right - 2.0 - plot.left()) * ppp).round() as usize).min(w);
    let width = x1.saturating_sub(x0);
    let gap = (2.0 * ppp).round() as usize;
    let (part, step) = match mode {
        ParadeMode::Separate => {
            let part = width.saturating_sub(2 * gap) / 3;
            (part, part + gap)
        }
        ParadeMode::Combined => (width, 0),
    };
    if let Some(px) = Pixels::of(frame)
        && part > 0
    {
        let row_at = |v: u8| ((sc.y(v as f32) - plot.top()) * ppp + 0.75).round();
        for y in sampled_rows(px.height, 9) {
            for c in 0..part {
                let values = px.at(px.column(c, part), y);
                for k in 0..3 {
                    if mode == ParadeMode::Combined && !shown[k] {
                        continue;
                    }
                    let row = row_at(values[k]);
                    if row >= 0.0 && (row as usize) < h {
                        lit[row as usize * w + x0 + k * step + c] |= 1 << k;
                    }
                }
            }
        }
    }
    let pixels = lit
        .into_iter()
        .map(|bits| {
            if bits == 0 {
                Color32::TRANSPARENT
            } else {
                let on = |k: u8| if bits & (1 << k) != 0 { 235 } else { 0 };
                Color32::from_rgb(on(0), on(1), on(2))
            }
        })
        .collect();
    ColorImage::new([w, h], pixels)
}

/// CIE L*a*b* (D65) of video values taken as sRGB's.
fn lab(rgb: [f32; 3], linear: &[f32; 256]) -> [f32; 3] {
    let [r, g, b] = rgb.map(|v| linear[(v * 255.0).round().clamp(0.0, 255.0) as usize]);
    let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.950_47;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.088_83;
    let f = |t: f32| {
        if t > 0.008_856 {
            t.cbrt()
        } else {
            7.787 * t + 16.0 / 116.0
        }
    };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// A frame's levels: every pixel, as VEGAS's histogram counts them (its
/// mean and deviation of the card's luma are the whole picture's); L*a*b*,
/// slower, every other pixel each way.
fn levels(frame: &PreviewFrame, with_lab: bool) -> Levels {
    let mut counts = [[0u32; 256]; 8];
    let mut totals = [0u64; 8];
    let Some(px) = Pixels::of(frame) else {
        return Levels { counts, totals };
    };
    for y in 0..px.height {
        for x in 0..px.width {
            let [r, g, b, a] = px.at(x, y);
            let luma = luma(r as f32, g as f32, b as f32).round().min(255.0) as usize;
            counts[LUMA][luma] += 1;
            counts[RED][r as usize] += 1;
            counts[GREEN][g as usize] += 1;
            counts[BLUE][b as usize] += 1;
            counts[ALPHA][a as usize] += 1;
        }
    }
    let all = (px.width * px.height) as u64;
    totals[..=ALPHA].fill(all);
    if with_lab {
        let linear: [f32; 256] = std::array::from_fn(|i| {
            let c = i as f32 / 255.0;
            if c <= 0.040_45 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        });
        let byte = |v: f32| v.round().clamp(0.0, 255.0) as usize;
        for y in (0..px.height).step_by(2) {
            for x in (0..px.width).step_by(2) {
                let (r, g, b) = unit(px.at(x, y));
                let [l, lab_a, lab_b] = lab([r, g, b], &linear);
                counts[LAB_L][byte(l * 2.55)] += 1;
                counts[LAB_A][byte(lab_a + 128.0)] += 1;
                counts[LAB_B][byte(lab_b + 128.0)] += 1;
                for t in &mut totals[LAB_L..] {
                    *t += 1;
                }
            }
        }
    }
    Levels { counts, totals }
}

/// A scope's picture of a frame, worked out again only when the frame or
/// what the scope shows changes.
#[derive(Clone)]
struct Trace {
    /// The frame, what the scope shows and the plot's size in pixels.
    key: (u64, bv_core::Tick, u32, u32, u64, [usize; 2]),
    texture: Option<TextureHandle>,
    levels: Option<Arc<Levels>>,
}

/// What a scope's trace depends on besides the frame.
fn look(scope: Scope, p: &ScopePrefs) -> u64 {
    let s = &p.settings;
    match scope {
        Scope::Vectorscope => s.studio_rgb as u64,
        Scope::Waveform => p.waveform as u64 | (s.studio_rgb as u64) << 1 | (s.setup as u64) << 2,
        Scope::Parade => {
            p.parade as u64
                | (p.parade_channels[0] as u64) << 1
                | (p.parade_channels[1] as u64) << 2
                | (p.parade_channels[2] as u64) << 3
        }
        Scope::Histogram => (p.histogram == HistogramChannel::Lab) as u64,
    }
}

fn trace(
    ui: &egui::Ui,
    scope: Scope,
    prefs: &ScopePrefs,
    frame: &PreviewFrame,
    plot: Rect,
) -> Trace {
    let id = ui.id().with(("scope-trace", scope));
    let ppp = ui.ctx().pixels_per_point();
    let key = (
        frame.generation,
        frame.time,
        frame.width,
        frame.height,
        look(scope, prefs),
        plot_pixels(plot, ppp),
    );
    let mut cached: Option<Trace> = ui.data(|d| d.get_temp(id));
    if let Some(c) = &cached
        && c.key == key
    {
        return c.clone();
    }
    let s = &prefs.settings;
    let (image, levels) = match scope {
        Scope::Vectorscope => (Some(vectorscope_image(frame, s, plot, ppp)), None),
        Scope::Waveform => (
            Some(waveform_image(frame, prefs.waveform, s, plot, ppp)),
            None,
        ),
        Scope::Parade => (
            Some(parade_image(
                frame,
                prefs.parade,
                prefs.parade_channels,
                plot,
                ppp,
            )),
            None,
        ),
        Scope::Histogram => {
            let lab = prefs.histogram == HistogramChannel::Lab;
            (None, Some(Arc::new(levels(frame, lab))))
        }
    };
    let texture = image.map(
        |image| match cached.as_mut().and_then(|c| c.texture.take()) {
            Some(mut t) => {
                t.set(image, TextureOptions::NEAREST);
                t
            }
            None => ui
                .ctx()
                .load_texture("video-scope", image, TextureOptions::NEAREST),
        },
    );
    let t = Trace {
        key,
        texture,
        levels,
    };
    ui.data_mut(|d| d.insert_temp(id, t.clone()));
    t
}

/// Whether the project shows video at `t`: an event on a video track that
/// is not muted.
fn video_at(project: &bv_timeline::Project, t: bv_core::Tick) -> bool {
    project.tracks.iter().any(|track| {
        track.kind == bv_timeline::TrackKind::Video
            && !track.muted
            && track.clips_at(t).next().is_some()
    })
}

/// The frame the scopes show: the Video Preview's (a render's while it
/// shows them), except while playing without Update Scopes While Playing,
/// when they keep the one they had. Over an empty timeline the preview's
/// black is no picture: the scales alone, as VEGAS's without video.
fn shown_frame(ui: &egui::Ui, app: &BananaApp) -> Option<Arc<PreviewFrame>> {
    let id = ui.id().with("scopes-frame");
    let frame = if app.is_playing() && !app.settings.scopes.while_playing {
        ui.data(|d| d.get_temp::<Option<Arc<PreviewFrame>>>(id))
            .flatten()
    } else {
        app.engine
            .render_frame()
            .or_else(|| app.engine.preview_frame())
            .filter(|f| video_at(app.project(), f.time))
    };
    ui.data_mut(|d| d.insert_temp(id, frame.clone()));
    frame
}

/// Without the Video Preview drawing (closed, or behind another window in
/// its pane), the scopes ask for the cursor's frame themselves: VEGAS's
/// show it whichever windows are open.
fn ask_for_frame(ui: &egui::Ui, app: &BananaApp) {
    let pass = ui.ctx().cumulative_pass_nr_for(egui::ViewportId::ROOT);
    let preview_asked = pass <= app.preview.requested_at + 1;
    if !preview_asked && app.external.is_none() && app.engine.render_frame().is_none() {
        app.engine.request_preview(app.display_time(), 960, 540);
    }
}

/// A small ▾.
fn triangle(painter: &Painter, c: Pos2) {
    painter.add(Shape::convex_polygon(
        vec![
            c + vec2(-3.5, -1.5),
            c + vec2(3.5, -1.5),
            c + vec2(0.0, 2.5),
        ],
        theme::text(),
        Stroke::NONE,
    ));
}

/// A mode's menu, flat as VEGAS's: the mode and a ▾; `Some` of the item
/// picked.
fn mode_menu(ui: &mut egui::Ui, items: &[&str], current: usize) -> Option<usize> {
    let galley = ui.painter().layout_no_wrap(
        items[current].to_string(),
        FontId::proportional(13.0),
        theme::text(),
    );
    let size = vec2(galley.size().x + 26.0, 22.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 2.0, theme::hover());
    }
    let y = rect.center().y - galley.size().y / 2.0;
    ui.painter()
        .galley(pos2(rect.left() + 4.0, y), galley, theme::text());
    triangle(ui.painter(), pos2(rect.right() - 10.0, rect.center().y));
    resp.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, items[current])
    });
    let mut picked = None;
    egui::Popup::menu(&resp).show(|ui| {
        for (i, item) in items.iter().enumerate() {
            if crate::menus::check_item(ui, item, i == current, true, "") {
                picked = Some(i);
            }
        }
    });
    picked
}

/// The combined parade's R, G or B button: lit in its color while its
/// channel shows.
fn channel_button(ui: &mut egui::Ui, letter: &str, on: bool, k: usize, tip: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
    let fill = if on {
        theme::button_highlight()
    } else if resp.hovered() {
        theme::hover()
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 2.0, fill);
    let color = if on { CHANNELS[k] } else { theme::text_dim() };
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        letter,
        FontId::proportional(12.0),
        color,
    );
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, tip));
    resp.on_hover_text(tip).clicked()
}

/// The bar over a scope: its name, and from the middle its mode's menu
/// (the combined parade's channel buttons after it, the menu moved left
/// when they would not fit).
fn header(ui: &mut egui::Ui, app: &mut BananaApp, scope: Scope, bar: Rect) {
    let width = |text: &str| {
        ui.painter()
            .layout_no_wrap(text.to_string(), FontId::proportional(13.0), theme::text())
            .size()
            .x
    };
    let name = width(scope.name());
    ui.painter().text(
        pos2(bar.left() + 4.0, bar.center().y),
        Align2::LEFT_CENTER,
        scope.name(),
        FontId::proportional(13.0),
        theme::text(),
    );
    let p = &app.settings.scopes;
    let buttons = if scope == Scope::Parade && p.parade == ParadeMode::Combined {
        width(p.parade.name()) + 26.0 + 6.0 + 3.0 * 22.0
    } else {
        0.0
    };
    let left = if bar.center().x + buttons > bar.right() - 4.0 {
        (bar.right() - 4.0 - buttons).max(bar.left() + name + 12.0)
    } else {
        bar.center().x
    };
    let menu = Rect::from_min_max(pos2(left, bar.top()), bar.max);
    let builder = egui::UiBuilder::new()
        .max_rect(menu)
        .id_salt(("scope-mode", scope))
        .layout(egui::Layout::left_to_right(egui::Align::Center));
    let p = &mut app.settings.scopes;
    let mut changed = false;
    ui.scope_builder(builder, |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        match scope {
            // Its only mode in VEGAS's screenshots.
            Scope::Vectorscope => {
                mode_menu(ui, &[tr("Normal")], 0);
            }
            Scope::Waveform => {
                let names = WaveformMode::ALL.map(WaveformMode::name);
                let i = WaveformMode::ALL.iter().position(|&m| m == p.waveform);
                if let Some(i) = mode_menu(ui, &names, i.unwrap_or(0)) {
                    changed |= p.waveform != WaveformMode::ALL[i];
                    p.waveform = WaveformMode::ALL[i];
                }
            }
            Scope::Histogram => {
                let names = HistogramChannel::ALL.map(HistogramChannel::name);
                let i = HistogramChannel::ALL.iter().position(|&m| m == p.histogram);
                if let Some(i) = mode_menu(ui, &names, i.unwrap_or(0)) {
                    changed |= p.histogram != HistogramChannel::ALL[i];
                    p.histogram = HistogramChannel::ALL[i];
                }
            }
            Scope::Parade => {
                let names = ParadeMode::ALL.map(ParadeMode::name);
                let i = ParadeMode::ALL.iter().position(|&m| m == p.parade);
                if let Some(i) = mode_menu(ui, &names, i.unwrap_or(0)) {
                    changed |= p.parade != ParadeMode::ALL[i];
                    p.parade = ParadeMode::ALL[i];
                }
                if p.parade == ParadeMode::Combined {
                    ui.add_space(6.0);
                    let tips = [tr("Red"), tr("Green"), tr("Blue")];
                    for (k, letter) in ["R", "G", "B"].into_iter().enumerate() {
                        if channel_button(ui, letter, p.parade_channels[k], k, tips[k]) {
                            p.parade_channels[k] = !p.parade_channels[k];
                            changed = true;
                        }
                    }
                }
            }
        }
    });
    if changed {
        let _ = app.settings.save();
    }
}

/// What the pointer reads on a scope: the vectorscope's chroma, the
/// waveform's luma, the parade's value.
fn readout(scope: Scope, plot: Rect, p: &ScopePrefs, at: Pos2) -> Option<String> {
    match scope {
        Scope::Vectorscope => {
            let d = Dial::of(plot);
            let off = at - d.c;
            let parts = off.length() / d.r100;
            if parts > OUTER {
                return None;
            }
            let deg = (-off.y).atan2(off.x).to_degrees().rem_euclid(360.0);
            Some(trf!(
                "Chroma: {} %, {}°",
                crate::fmt::num((parts * 100.0) as f64, 0),
                crate::fmt::num(deg as f64, 0)
            ))
        }
        Scope::Waveform => {
            let sc = Scale::waveform(plot);
            let v = sc.value(at.y);
            (sc.low..=sc.high)
                .contains(&v)
                .then(|| trf!("Luma: {} IRE", crate::fmt::num(v as f64, 1)))
        }
        Scope::Parade => {
            let sc = Scale::parade(plot);
            let v = sc.value(at.y);
            if !(sc.low - 0.5..=sc.high + 0.5).contains(&v) || at.x < sc.left {
                return None;
            }
            let v = crate::fmt::num(v.round().clamp(0.0, 255.0) as f64, 0);
            Some(match p.parade {
                ParadeMode::Combined => trf!("RGB: {}", v),
                ParadeMode::Separate => {
                    let third = (sc.right - sc.left) / 3.0;
                    match ((at.x - sc.left) / third) as usize {
                        0 => trf!("Red: {}", v),
                        1 => trf!("Green: {}", v),
                        _ => trf!("Blue: {}", v),
                    }
                }
            })
        }
        Scope::Histogram => None,
    }
}

/// A scope: its bar, its scale on black, the frame's trace over it and
/// the pointer's readout.
fn scope_ui(
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    scope: Scope,
    rect: Rect,
    frame: Option<&PreviewFrame>,
) {
    const BAR: f32 = 30.0;
    let bar = Rect::from_min_size(rect.min, vec2(rect.width(), BAR));
    header(ui, app, scope, bar);
    let plot = Rect::from_min_max(pos2(rect.left(), bar.bottom()), rect.max);
    if plot.height() < 24.0 || plot.width() < 24.0 {
        return;
    }
    let painter = ui.painter_at(plot);
    painter.rect_filled(plot, 0.0, Color32::BLACK);
    let prefs = &app.settings.scopes;
    let trace = frame.map(|f| trace(ui, scope, prefs, f, plot));
    let texture = trace.as_ref().and_then(|t| t.texture.as_ref());
    let whole = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
    let draw_trace = || {
        if let Some(t) = texture {
            painter.image(t.id(), plot, whole, Color32::WHITE);
        }
    };
    match scope {
        // Its scale over the dots, as VEGAS's.
        Scope::Vectorscope => {
            draw_trace();
            paint_vectorscope(&painter, plot, &prefs.settings);
        }
        Scope::Waveform => {
            paint_waveform(&painter, plot, &prefs.settings);
            draw_trace();
        }
        Scope::Parade => {
            paint_parade(&painter, plot);
            draw_trace();
        }
        Scope::Histogram => {
            let levels = trace.as_ref().and_then(|t| t.levels.as_deref());
            paint_histogram(&painter, plot, prefs.histogram, levels);
        }
    }
    // The Color Grading window's RL thresholds (Show Limits on Scopes).
    if let Some(limits) = super::color::scope_limits(app) {
        paint_limits(&painter, plot, scope, &prefs.settings, limits);
    }
    let hover = ui.interact(plot, ui.id().with(("scope-plot", scope)), Sense::hover());
    if let Some(at) = hover.hover_pos()
        && let Some(text) = readout(scope, plot, &app.settings.scopes, at)
    {
        hover.on_hover_text_at_pointer(text);
    }
}

/// The Color Grading window's RL thresholds as dashed guides (Show Limits
/// on Scopes): across the waveform at their IRE and the parade at their
/// level, down the histogram.
fn paint_limits(painter: &Painter, plot: Rect, scope: Scope, s: &ScopeSettings, limits: [f64; 2]) {
    let stroke = Stroke::new(1.5, LIMITS);
    for v in limits.map(|v| v as f32) {
        let line = match scope {
            Scope::Waveform => {
                let sc = Scale::waveform(plot);
                let y = sc.y(ire(v, s));
                [pos2(sc.left, y), pos2(sc.right, y)]
            }
            Scope::Parade => {
                let sc = Scale::parade(plot);
                let y = sc.y(v * 255.0);
                [pos2(sc.left, y), pos2(sc.right, y)]
            }
            Scope::Histogram => {
                let a = Across::of(plot);
                let x = a.x(v * 255.0);
                [pos2(x, a.top), pos2(x, a.bottom)]
            }
            Scope::Vectorscope => continue,
        };
        painter.extend(egui::Shape::dashed_line(&line, stroke, 6.0, 4.0));
    }
}

/// The toolbar: the views' list, Video Scope Settings and Update Scopes
/// While Playing.
fn toolbar(ui: &mut egui::Ui, app: &mut BananaApp) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let mut view = app.settings.scopes.view;
        egui::ComboBox::from_id_salt("video-scopes-view")
            .width(200.0)
            .selected_text(view.name())
            .show_ui(ui, |ui| {
                for v in View::ALL {
                    ui.selectable_value(&mut view, v, v.name());
                }
            });
        if view != app.settings.scopes.view {
            app.settings.scopes.view = view;
            let _ = app.settings.save();
        }
        if icon_button(ui, Icon::Settings, tr("Video Scope Settings")).clicked() {
            app.menu_dialogs.more.scope_settings =
                Some(SettingsDialog::new(&app.settings.scopes.settings));
        }
        let on = app.settings.scopes.while_playing;
        if icon_toggle(
            ui,
            Icon::ScopesWhilePlaying,
            on,
            tr("Update Scopes While Playing"),
        )
        .clicked()
        {
            app.settings.scopes.while_playing = !on;
            let _ = app.settings.save();
        }
    });
}

pub fn ui(ui: &mut egui::Ui, app: &mut BananaApp) {
    toolbar(ui, app);
    let area = ui.available_rect_before_wrap();
    ui.allocate_rect(area, Sense::hover());
    if area.width() < 40.0 || area.height() < 40.0 {
        return;
    }
    ask_for_frame(ui, app);
    let frame = shown_frame(ui, app);
    // Scopes side by side and rows apart by a line of the window.
    const GAP: f32 = 2.0;
    let rows = app.settings.scopes.view.rows();
    let row_h = (area.height() - GAP * (rows.len() - 1) as f32) / rows.len() as f32;
    for (i, row) in rows.iter().enumerate() {
        let top = area.top() + i as f32 * (row_h + GAP);
        let w = (area.width() - GAP * (row.len() - 1) as f32) / row.len() as f32;
        for (j, &scope) in row.iter().enumerate() {
            let left = area.left() + j as f32 * (w + GAP);
            let rect = Rect::from_min_size(pos2(left, top), vec2(w, row_h));
            scope_ui(ui, app, scope, rect, frame.as_deref());
        }
    }
}

/// Video Scope Settings, as VEGAS's: applied on OK. The HDR scope waits
/// for BananaV's HDR.
#[derive(Debug)]
pub struct SettingsDialog {
    draft: ScopeSettings,
}

impl SettingsDialog {
    pub fn new(settings: &ScopeSettings) -> SettingsDialog {
        SettingsDialog {
            draft: settings.clone(),
        }
    }

    /// Shows it; `false` once it is closed.
    pub fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        let (mut open, mut ok, mut cancel) = (true, false, false);
        crate::floating::Window::new(tr("Video Scope Settings"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                // Room for its title, longer than its options.
                ui.set_min_width(320.0);
                let d = &mut self.draft;
                let soon = |ui: &mut egui::Ui, label: &str| {
                    let mut off = false;
                    ui.add_enabled_ui(false, |ui| crate::widgets::checkbox(ui, &mut off, label))
                        .inner
                        .on_disabled_hover_text(tr("Coming soon to BananaV"));
                };
                soon(ui, tr("Enable ST.2084 HDR Scope"));
                ui.indent("hdr-scope", |ui| soon(ui, tr("10 bit scale")));
                crate::widgets::checkbox(ui, &mut d.skin_tone_line, tr("Skin tone line"));
                ui.indent("skin-tone", |ui| {
                    ui.add_enabled_ui(d.skin_tone_line, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(tr("Angle"));
                            let mut a = d.skin_tone_angle as f64;
                            if crate::widgets::spin_box(ui, &mut a, 0.0..=360.0, 1.0, 0, 70.0)
                                .changed()
                            {
                                d.skin_tone_angle = a as f32;
                            }
                        });
                    });
                });
                crate::widgets::checkbox(ui, &mut d.setup, tr("7.5 IRE Setup"));
                crate::widgets::checkbox(ui, &mut d.studio_rgb, tr("Studio RGB (16 to 235)"));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ok = ui.button("OK").clicked();
                    cancel = ui.button(tr("Cancel")).clicked();
                });
            });
        if ok {
            app.settings.scopes.settings = self.draft.clone();
            let _ = app.settings.save();
            return false;
        }
        open && !cancel
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_of(pixels: &[[u8; 4]], width: u32) -> PreviewFrame {
        let height = (pixels.len() as u32 / width).max(1);
        PreviewFrame {
            time: bv_core::Tick::ZERO,
            generation: 0,
            width,
            height,
            drawn: (width, height),
            room: (0, 0),
            rgba: pixels.concat(),
            exact: true,
        }
    }

    fn frame(rgba: [u8; 4]) -> PreviewFrame {
        frame_of(&[rgba; 32], 8)
    }

    /// The targets where VEGAS's screenshot has them: R at 103.5° and
    /// 95 %, Mg 61° and 89 %, B −13° and 67 %, and their complements
    /// opposite.
    #[test]
    fn the_targets_sit_where_vegas_draws_them() {
        let near = |a: f32, b: f32, tol: f32| (a - b).abs() < tol;
        let (deg, parts) = target([1.0, 0.0, 0.0]);
        assert!(
            near(deg, 103.5, 0.3) && near(parts, 0.948, 0.005),
            "{deg} {parts}"
        );
        let (deg, parts) = target([1.0, 0.0, 1.0]);
        assert!(
            near(deg, 60.7, 0.3) && near(parts, 0.886, 0.005),
            "{deg} {parts}"
        );
        let (deg, parts) = target([0.0, 0.0, 1.0]);
        assert!(
            near(deg, -12.9, 0.3) && near(parts, 0.671, 0.005),
            "{deg} {parts}"
        );
        for (a, b) in [
            ([1.0, 0.0, 0.0], [0.0, 1.0, 1.0]),
            ([0.0, 0.0, 1.0], [1.0, 1.0, 0.0]),
        ] {
            let ((d1, p1), (d2, p2)) = (target(a), target(b));
            assert!(near((d1 - d2).rem_euclid(360.0), 180.0, 0.01) && near(p1, p2, 1e-4));
        }
    }

    /// The waveform's green steps, measured on VEGAS's screenshot: the
    /// 75 % bars' lumas, 75 IRE (white) down to 8.5 (blue).
    #[test]
    fn the_bars_levels_are_the_75_percent_bars_lumas() {
        let s = ScopeSettings::default();
        let measured = [75.2, 66.7, 52.6, 44.1, 31.1, 22.2, 8.5];
        for (got, want) in bar_levels(&s).into_iter().zip(measured) {
            assert!((got - want).abs() < 0.5, "{got} vs {want}");
        }
        let setup = ScopeSettings { setup: true, ..s };
        assert!((bar_levels(&setup)[0] - (7.5 + 0.925 * 75.0)).abs() < 1e-3);
    }

    /// No Studio RGB: 0 is black and 255 white.
    fn full_range() -> ScopeSettings {
        ScopeSettings {
            studio_rgb: false,
            ..ScopeSettings::default()
        }
    }

    /// A plot the size of VEGAS's in the user's screenshots.
    fn plot() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(539.0, 434.0))
    }

    #[test]
    fn studio_rgb_and_setup_move_black_and_white() {
        let full = full_range();
        assert_eq!(ire(0.0, &full), 0.0);
        assert!((ire(1.0, &full) - 100.0).abs() < 1e-4);
        // VEGAS's scopes show the card in studio levels.
        let studio = ScopeSettings::default();
        assert!(studio.studio_rgb);
        assert!(ire(16.0 / 255.0, &studio).abs() < 1e-3);
        assert!((ire(235.0 / 255.0, &studio) - 100.0).abs() < 1e-3);
        let setup = ScopeSettings {
            setup: true,
            ..full
        };
        assert!((ire(0.0, &setup) - 7.5).abs() < 1e-4);
        assert!((ire(1.0, &setup) - 100.0).abs() < 1e-4);
    }

    /// The rows VEGAS's scopes read of a 1080-line frame: the waveform's and
    /// the vectorscope's 7, 15 … (135), the parade's 8, 17 … (120).
    #[test]
    fn the_scopes_read_vegas_s_rows() {
        let every8: Vec<usize> = sampled_rows(1080, 8).collect();
        assert_eq!((every8[0], every8[1], every8.len()), (7, 15, 135));
        let every9: Vec<usize> = sampled_rows(1080, 9).collect();
        assert_eq!((every9[0], every9[1], every9.len()), (8, 17, 120));
        assert_eq!(sampled_rows(4, 8).collect::<Vec<_>>(), [3]);
        assert_eq!(sampled_rows(0, 8).count(), 0);
    }

    /// A point of the waveform brightens as VEGAS's do with the pixels
    /// there: 55, 96, 127, 150, 167, 180, 190, 197 … up to 217.
    #[test]
    fn waveform_points_brighten_as_vegas_s() {
        let mut level = 0;
        let mut steps = Vec::new();
        for _ in 0..40 {
            level = brighten(level);
            steps.push(level);
        }
        assert_eq!(steps[..8], [55, 96, 127, 150, 167, 180, 190, 197]);
        assert_eq!(*steps.last().unwrap(), 217);
    }

    /// The vectorscope's dots in VEGAS's colors, read on its screenshot of
    /// the card's ramps and lattice.
    #[test]
    fn vectorscope_dots_take_vegas_s_colors() {
        let dot = |r: f32, g: f32, b: f32| {
            let (u, v) = chroma(r / 255.0, g / 255.0, b / 255.0);
            dot_color(u, v)
        };
        let near = |a: [u8; 3], b: [u8; 3]| a.iter().zip(b).all(|(&x, y)| x.abs_diff(y) <= 2);
        assert!(near(dot(32.0, 0.0, 0.0), [152, 116, 119]));
        assert!(near(dot(160.0, 0.0, 0.0), [252, 71, 90]));
        assert!(near(dot(128.0, 0.0, 128.0), [211, 72, 187]));
        assert!(near(dot(0.0, 0.0, 128.0), [111, 118, 216]));
        assert!(near(dot(162.0, 227.0, 57.0), [98, 162, 23]));
        assert_eq!(dot(90.0, 90.0, 90.0), [127, 127, 127], "grey at the middle");
    }

    #[test]
    fn the_histogram_counts_every_pixel() {
        let l = levels(&frame([255, 0, 0, 255]), true);
        assert_eq!(l.counts[RED][255], 32);
        assert_eq!(l.counts[GREEN][0], 32);
        assert_eq!(l.counts[ALPHA][255], 32);
        assert_eq!(l.counts[LUMA][76], 32, "BT.601's red luma: 29.9 %");
        assert_eq!(l.totals[LUMA], 32);
        // sRGB red: L* 53.2, a* 80.1, b* 67.2 (every other pixel each way).
        assert_eq!(l.counts[LAB_L][136], 8);
        assert_eq!(l.counts[LAB_A][208], 8);
        assert_eq!(l.counts[LAB_B][195], 8);
        assert_eq!(l.totals[LAB_A], 8);
        let (mean, deviation) = statistics(&l.counts[RED]).unwrap();
        assert_eq!((mean, deviation), (255.0, 0.0));
        assert!(statistics(&[0; 256]).is_none());
    }

    /// White at 100 IRE, black at 0 — at −7.3 in studio levels, as the
    /// card's black on VEGAS's screenshot — and a composite signal's swing.
    #[test]
    fn white_sits_at_100_ire_and_black_at_0() {
        let (plot, full) = (plot(), full_range());
        let sc = Scale::waveform(plot);
        let at = |img: &ColorImage, ire: f32| {
            let w = img.size[0];
            let row = (sc.y(ire) + 0.25).ceil() as usize;
            (row - 1..=row + 1).any(|r| (0..w).any(|c| img.pixels[r * w + c].a() > 0))
        };
        let wave = |rgba, mode, s: &ScopeSettings| waveform_image(&frame(rgba), mode, s, plot, 1.0);
        let white = wave([255, 255, 255, 255], WaveformMode::Luminance, &full);
        assert!(at(&white, 100.0) && !at(&white, 0.0));
        let black = wave([0, 0, 0, 255], WaveformMode::Luminance, &full);
        assert!(at(&black, 0.0) && !at(&black, 100.0));
        let studio = wave(
            [0, 0, 0, 255],
            WaveformMode::Luminance,
            &ScopeSettings::default(),
        );
        assert!(at(&studio, -7.3) && !at(&studio, 0.0));
        // Composite: 75 % yellow swings up to 100 IRE.
        let yellow = wave([191, 191, 0, 255], WaveformMode::Composite, &full);
        assert!(at(&yellow, 66.5) && at(&yellow, 100.0) && at(&yellow, 33.0));
    }

    /// 75 % bars land in their targets on the vectorscope (and in studio
    /// levels, the bars of studio levels).
    #[test]
    fn the_bars_land_on_their_targets() {
        let plot = plot();
        let d = Dial::of(plot);
        for (s, full, black) in [
            (full_range(), 255.0, 0.0),
            (ScopeSettings::default(), 235.0, 16.0),
        ] {
            // Bars 8 pixels wide, 8 rows: the pixels the scope reads.
            let row: Vec<[u8; 4]> = TARGETS
                .iter()
                .flat_map(|(rgb, _, _)| {
                    let [r, g, b] = rgb.map(|v| (black + v * 0.75 * (full - black)).round() as u8);
                    [[r, g, b, 255]; 8]
                })
                .collect();
            let img = vectorscope_image(&frame_of(&row.repeat(8), 48), &s, plot, 1.0);
            let w = img.size[0];
            for (rgb, label, _) in TARGETS {
                let (deg, parts) = target(rgb);
                let p = d.at(deg, parts);
                let (x, y) = (p.x.round() as usize, p.y.round() as usize);
                let lit = (y - 1..=y + 1)
                    .flat_map(|r| (x - 1..=x + 1).map(move |c| (r, c)))
                    .any(|(r, c)| img.pixels[r * w + c].a() > 0);
                assert!(lit, "{label}");
            }
        }
    }

    #[test]
    fn the_parade_shows_each_channel_in_its_third() {
        let plot = plot();
        let sc = Scale::parade(plot);
        let row = |v: f32| (sc.y(v) + 0.75).round() as usize;
        let x0 = (sc.left + 1.0).round() as usize;
        let width = (sc.right - 2.0).round() as usize - x0;
        let part = (width - 4) / 3;
        let img = parade_image(
            &frame([255, 0, 0, 255]),
            ParadeMode::Separate,
            [true; 3],
            plot,
            1.0,
        );
        let w = img.size[0];
        let lit = |img: &ColorImage, row: usize, cols: std::ops::Range<usize>| {
            cols.into_iter().any(|c| img.pixels[row * w + c].a() > 0)
        };
        let parts = [0, 1, 2].map(|k| x0 + k * (part + 2)..x0 + k * (part + 2) + part);
        assert!(lit(&img, row(255.0), parts[0].clone()), "red at 255");
        assert!(!lit(&img, row(0.0), parts[0].clone()));
        assert!(lit(&img, row(0.0), parts[1].clone()), "green at 0");
        assert!(!lit(&img, row(255.0), parts[1].clone()));
        assert!(lit(&img, row(0.0), parts[2].clone()), "blue at 0");
        assert_eq!(img.pixels[row(255.0) * w + x0], CHANNELS[0]);
        // Combined, with blue's button off.
        let img = parade_image(
            &frame([0, 0, 255, 255]),
            ParadeMode::Combined,
            [true, true, false],
            plot,
            1.0,
        );
        assert!(!lit(&img, row(255.0), x0..x0 + width), "blue hidden");
        assert_eq!(
            img.pixels[row(0.0) * w + x0],
            Color32::from_rgb(235, 235, 0),
            "red and green at 0"
        );
    }

    /// The readouts read the scales back: the waveform's 100 line is 100
    /// IRE, the vectorscope's red target red's chroma.
    #[test]
    fn the_pointer_reads_the_scales() {
        crate::i18n::set_language(crate::i18n::Language::English);
        let plot = Rect::from_min_size(pos2(0.0, 0.0), vec2(544.0, 433.0));
        let p = ScopePrefs::default();
        let sc = Scale::waveform(plot);
        let text = readout(Scope::Waveform, plot, &p, pos2(200.0, sc.y(100.0))).unwrap();
        assert_eq!(text, "Luma: 100.0 IRE");
        let d = Dial::of(plot);
        let (deg, parts) = target([1.0, 0.0, 0.0]);
        let text = readout(Scope::Vectorscope, plot, &p, d.at(deg, parts)).unwrap();
        assert_eq!(text, "Chroma: 95 %, 103°");
        let sc = Scale::parade(plot);
        let text = readout(Scope::Parade, plot, &p, pos2(sc.left + 10.0, sc.y(128.0))).unwrap();
        assert_eq!(text, "Red: 128");
        assert!(readout(Scope::Histogram, plot, &p, plot.center()).is_none());
    }

    /// Every view draws its scales with no frame at all.
    #[test]
    fn every_view_draws_without_a_frame() {
        let mut app = BananaApp::new(egui::Context::default());
        app.welcome_for_review("closed");
        let mut h = egui_kittest::Harness::builder()
            .with_size(vec2(800.0, 600.0))
            .build_ui_state(
                |ui, app: &mut BananaApp| {
                    crate::apply_theme(ui.ctx());
                    super::ui(ui, app);
                },
                app,
            );
        for view in View::ALL {
            h.state_mut().settings.scopes.view = view;
            h.run_steps(2);
        }
    }
}
