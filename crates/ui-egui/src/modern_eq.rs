//! The Modern Equalizer's editor (`EffectKind::ModernEq`), as VEGAS's
//! Help lays its window out: A/B comparison, the graph's zoom and the
//! settings over the filter graph — each band's area in the color of its
//! channel routing, the whole curve, Mono Below's green line and the
//! bands' circles: dragged for frequency and gain, the wheel for Q, a
//! double-click adding a band (its type from where) or deleting one, a
//! right-click's Cut, Copy and Replace —; under it the bands' toolbar
//! (each band's button, its dot turning it on or off, and +), the band
//! picked — Filter Type, slope, Channel Routing, Frequency, Gain, Q and
//! its delete button — and the global controls. The curves are the
//! mixer's (`bv_engine::modern_eq`): what is heard, at Settings'
//! Oversampling's rate. While it plays, the mixer measures the effect
//! (`eq::watch`, `take_analysis`): its output's spectrum behind the curves
//! (the FFT analyzer) and its input's and output's peaks by the graph (the
//! peak meters), each of which Settings turns off.

use bv_engine::modern_eq::{self as eq, Band, BandType, Route, Settings};
use bv_timeline::{Effect, EffectKind};

use crate::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};
use crate::i18n::{tr, trc};
use crate::icons::Icon;
use crate::widgets::icon_button_ex;
use crate::{menus, theme};

/// The graph's frequencies.
const F_LO: f32 = 20.0;
const F_HI: f32 = 20_000.0;
/// The graph's height, and the room for its dB scale on the left and its
/// frequencies under it.
const GRAPH_H: f32 = 250.0;
const SCALE_W: f32 = 30.0;
const SCALE_H: f32 = 16.0;
/// A band's circle, and how near the pointer takes it.
const NODE: f32 = 8.0;
const REACH: f32 = 11.0;
/// The zoom's choices: the graph's range, ± dB.
const RANGES: [f32; 5] = [3.0, 6.0, 12.0, 18.0, 30.0];
/// Gains the circles reach (the parameters' range).
const GAIN_MAX: f32 = 30.0;
/// Where a double-click on the graph adds a pass rather than a shelf, and
/// a shelf rather than a peak (Hz).
const PASS_BELOW: f32 = 40.0;
const SHELF_BELOW: f32 = 120.0;
const SHELF_ABOVE: f32 = 8_000.0;
const PASS_ABOVE: f32 = 16_000.0;
/// Mono Below's line.
const MONO_GREEN: Color32 = Color32::from_rgb(0x4C, 0xD9, 0x6A);
/// The knobs' size, with their name over them and their value under.
const KNOB: f32 = 34.0;
const KNOB_W: f32 = 70.0;
/// A band's button in the toolbar.
const CHIP: Vec2 = Vec2::new(46.0, 22.0);
/// The width of the band picked's controls.
const BAND_W: f32 = 470.0;

/// A routing's color: its bands' circles, areas and buttons.
pub(crate) fn route_color(r: Route) -> Color32 {
    match r {
        Route::Stereo => Color32::from_rgb(0xF2, 0xA1, 0x3B),
        Route::Left => Color32::from_rgb(0x4F, 0xA3, 0xF7),
        Route::Right => Color32::from_rgb(0xF2, 0x5C, 0x6E),
        Route::Mid => Color32::from_rgb(0xE3, 0xCF, 0x4A),
        Route::Side => Color32::from_rgb(0xA8, 0x7B, 0xF2),
    }
}

/// A routing's name in the interface's language (Mid and Side are the
/// middle and the sides of the stereo picture, not frequencies).
fn route_name(r: Route) -> &'static str {
    match r {
        Route::Stereo => tr("Stereo"),
        Route::Left => tr("Left"),
        Route::Right => tr("Right"),
        Route::Mid => trc("routing", "Mid"),
        Route::Side => trc("routing", "Side"),
    }
}

/// The editor's state, kept by its id.
#[derive(Clone, Debug)]
struct View {
    /// The band picked.
    picked: Option<usize>,
    /// A circle dragged: its band and where it was from the pointer when
    /// taken.
    drag: Option<(usize, Vec2)>,
    /// The graph's range (± dB).
    range: f32,
    /// A/B comparison: whether B shows, and the other one's values (none
    /// until it is first shown: then it starts as a copy).
    b: bool,
    other: Option<Vec<f32>>,
    /// What a right-click's menu is for.
    menu: Option<MenuFor>,
    /// Hover texts on the bands (Settings' Show Tooltips).
    tooltips: bool,
    /// The FFT analyzer and the peak meters shown (Settings' Disable FFT
    /// Analyzer, Disable Peak Meters).
    analyzer: bool,
    meters: bool,
    /// The output's spectrum at [`analysis_freqs`] (dB of a full-scale
    /// sine) and the input's and output's peaks (dB), falling back
    /// slowly.
    spectrum: Vec<f32>,
    peaks: [f32; 2],
}

impl Default for View {
    fn default() -> View {
        View {
            picked: None,
            drag: None,
            range: 12.0,
            b: false,
            other: None,
            menu: None,
            tooltips: true,
            analyzer: true,
            meters: true,
            spectrum: Vec::new(),
            peaks: [FLOOR_DB; 2],
        }
    }
}

/// Where the analyzer and the meters bottom out (dB).
const FLOOR_DB: f32 = -90.0;
/// The meters' range (dB).
const METER_DB: f32 = -60.0;
/// How fast the analyzer and the meters fall back (dB a frame).
const FALL_DB: f32 = 1.2;

/// The frequencies the analyzer shows: log-spaced over the graph.
fn analysis_freqs() -> Vec<f32> {
    const N: usize = 160;
    (0..N)
        .map(|i| F_LO * (F_HI / F_LO).powf(i as f32 / (N - 1) as f32))
        .collect()
}

/// What a right-click's menu is for: a band, or the graph away from them
/// (where).
#[derive(Clone, Copy, Debug)]
enum MenuFor {
    Band(usize),
    Graph { freq: f32, db: f32 },
}

/// The graph's scales: frequencies across (logarithmic), dB up.
#[derive(Clone, Copy)]
struct Axes {
    plot: Rect,
    range: f32,
}

impl Axes {
    fn x(&self, f: f32) -> f32 {
        let t = (f.max(1.0) / F_LO).ln() / (F_HI / F_LO).ln();
        self.plot.left() + self.plot.width() * t
    }

    fn freq(&self, x: f32) -> f32 {
        let t = ((x - self.plot.left()) / self.plot.width()).clamp(0.0, 1.0);
        F_LO * (F_HI / F_LO).powf(t)
    }

    /// Half the plot's height, less a margin, for the range.
    fn half(&self) -> f32 {
        self.plot.height() / 2.0 - 8.0
    }

    fn y(&self, db: f32) -> f32 {
        self.plot.center().y - db / self.range * self.half()
    }

    fn db(&self, y: f32) -> f32 {
        (self.plot.center().y - y) / self.half() * self.range
    }

    /// Band `b`'s circle: at its frequency and gain (at 0 dB for the
    /// types without one), inside the plot.
    fn node(&self, b: &Band) -> Pos2 {
        let db = if b.kind.has_gain() { b.gain_db } else { 0.0 };
        pos2(
            self.x(b.freq),
            self.y(db)
                .clamp(self.plot.top() + NODE, self.plot.bottom() - NODE),
        )
    }
}

/// The band whose circle is under `at` (the one picked first: it is
/// drawn on top).
fn hit(ax: &Axes, s: &Settings, picked: Option<usize>, at: Pos2) -> Option<usize> {
    let near = |b: usize| s.bands[b].used && ax.node(&s.bands[b]).distance(at) <= REACH;
    if let Some(p) = picked
        && near(p)
    {
        return Some(p);
    }
    (0..eq::BANDS).filter(|&b| near(b)).min_by(|&a, &b| {
        let d = |b: usize| ax.node(&s.bands[b]).distance(at);
        d(a).total_cmp(&d(b))
    })
}

/// Band slot `b` as a new equalizer has it: unused, where it rests.
fn at_rest(b: usize) -> Band {
    Settings::of(&Effect::new(EffectKind::ModernEq)).bands[b]
}

/// A band in use, on, of `kind` at `freq` and `gain_db`.
fn new_band(kind: BandType, freq: f32, gain_db: f32) -> Band {
    Band {
        used: true,
        on: true,
        kind,
        freq: freq.clamp(F_LO, F_HI),
        gain_db: if kind.has_gain() {
            gain_db.clamp(-GAIN_MAX, GAIN_MAX)
        } else {
            0.0
        },
        q: 1.0,
        slope: 12,
        route: Route::Stereo,
    }
}

/// The type a double-click on the graph adds at `freq`: a pass at the
/// ends, a shelf near them, a peak between.
fn type_at(freq: f32) -> BandType {
    if freq < PASS_BELOW {
        BandType::HighPass
    } else if freq > PASS_ABOVE {
        BandType::LowPass
    } else if freq < SHELF_BELOW {
        BandType::LowShelf
    } else if freq > SHELF_ABOVE {
        BandType::HighShelf
    } else {
        BandType::Peaking
    }
}

/// Adds a band where the graph was double-clicked, in the first free
/// slot; which (none when all eight are in use).
fn add_at(s: &mut Settings, freq: f32, db: f32) -> Option<usize> {
    let b = s.bands.iter().position(|b| !b.used)?;
    s.bands[b] = new_band(type_at(freq), freq, db);
    Some(b)
}

/// Adds a peak where the bands in use leave the widest room (the +
/// button); which.
fn add_free(s: &mut Settings) -> Option<usize> {
    let b = s.bands.iter().position(|b| !b.used)?;
    let mut at: Vec<f32> = s
        .bands
        .iter()
        .filter(|b| b.used)
        .map(|b| b.freq.ln())
        .chain([F_LO.ln(), F_HI.ln()])
        .collect();
    at.sort_by(f32::total_cmp);
    let (lo, hi) = at
        .windows(2)
        .map(|w| (w[0], w[1]))
        .max_by(|x, y| (x.1 - x.0).total_cmp(&(y.1 - y.0)))?;
    s.bands[b] = new_band(BandType::Peaking, ((lo + hi) / 2.0).exp(), 0.0);
    Some(b)
}

/// The band a copy keeps (Cut, Copy, Replace), for every equalizer.
fn clipboard_id() -> egui::Id {
    egui::Id::new("modern-eq-band")
}

/// The editor of `e`, a Modern Equalizer, its curves at the project's
/// sample `rate`; changes go into `e`.
/// The editor of effect `e` (`tap`: its chain's owner and its place,
/// which the mixer measures while it plays).
pub(crate) fn editor(
    ui: &mut Ui,
    e: &mut Effect,
    rate: f32,
    id: egui::Id,
    tap: Option<(eq::TapOwner, usize)>,
) {
    let mut view: View = ui.data_mut(|d| d.get_temp_mut_or_default::<View>(id).clone());
    top_bar(ui, &mut view, e, id);
    follow_analysis(ui, &mut view, tap);
    let mut s = Settings::of(e);
    // The curves at the rate the bands run at.
    let rate = rate * f32::from(s.oversampling.max(1));
    let before = s;
    if view.picked.is_none_or(|b| !s.bands[b].used) {
        view.picked = s.bands.iter().position(|b| b.used);
    }
    ui.add_space(4.0);
    graph(ui, &mut view, &mut s, rate, id);
    ui.add_space(4.0);
    toolbar(ui, &mut view, &mut s);
    ui.add_space(6.0);
    ui.horizontal_top(|ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        let column = egui::Layout::top_down(egui::Align::Min);
        let left = ui
            .allocate_ui_with_layout(vec2(BAND_W, 0.0), column, |ui| {
                ui.set_width(BAND_W);
                band_controls(ui, &mut view, &mut s, id);
            })
            .response
            .rect;
        ui.add_space(14.0);
        let right = ui.vertical(|ui| global_controls(ui, &mut s)).response.rect;
        // A line between them, as tall as they are.
        ui.painter().vline(
            (left.right() + right.left()) / 2.0,
            left.top()..=left.bottom().max(right.bottom()),
            Stroke::new(1.0, theme::separator()),
        );
    });
    if s != before {
        s.write(e);
    }
    ui.data_mut(|d| d.insert_temp(id, view));
}

/// What the mixer measured since the last frame: the spectrum and the
/// peaks, falling back slowly where it measured less (or nothing).
fn follow_analysis(ui: &Ui, view: &mut View, tap: Option<(eq::TapOwner, usize)>) {
    let on = view.analyzer || view.meters;
    eq::watch(tap.filter(|_| on));
    if !on {
        return;
    }
    let freqs = analysis_freqs();
    let got = eq::take_analysis(&freqs);
    if view.spectrum.len() != freqs.len() {
        view.spectrum = vec![FLOOR_DB; freqs.len()];
    }
    let new = got.as_ref().map(|a| &a.spectrum);
    for (i, v) in view.spectrum.iter_mut().enumerate() {
        let now = new.and_then(|n| n.get(i)).copied().unwrap_or(FLOOR_DB);
        *v = now.max(*v - FALL_DB).max(FLOOR_DB);
    }
    for (k, v) in view.peaks.iter_mut().enumerate() {
        let now = got
            .as_ref()
            .map_or(FLOOR_DB, |a| 20.0 * a.peaks[k].max(1e-6).log10());
        *v = now.max(*v - FALL_DB).max(FLOOR_DB);
    }
    let falling =
        view.spectrum.iter().any(|&v| v > FLOOR_DB) || view.peaks.iter().any(|&v| v > FLOOR_DB);
    if tap.is_some() && (got.is_some() || falling) {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
    }
}

/// A/B comparison, the graph's zoom and the settings.
fn top_bar(ui: &mut Ui, view: &mut View, e: &mut Effect, id: egui::Id) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (b, name) in [(false, "A"), (true, "B")] {
            let r = ui
                .selectable_label(view.b == b, name)
                .on_hover_text(tr("Switch between two settings to compare them"));
            if r.clicked() && view.b != b {
                let shown = e.params.clone();
                e.params = view.other.take().unwrap_or_else(|| shown.clone());
                view.other = Some(shown);
                view.b = b;
            }
        }
        let copy = if view.b {
            tr("Copy B to A")
        } else {
            tr("Copy A to B")
        };
        if icon_button_ex(ui, Icon::Copy, copy, false, true).clicked() {
            view.other = Some(e.params.clone());
        }
        ui.add_space(12.0);
        ui.label(tr("Zoom:"));
        egui::ComboBox::from_id_salt(id.with("zoom"))
            .width(76.0)
            .selected_text(format!(
                "± {} dB",
                crate::fmt::num(f64::from(view.range), 0)
            ))
            .show_ui(ui, |ui| {
                for r in RANGES {
                    let text = format!("± {} dB", crate::fmt::num(f64::from(r), 0));
                    if ui.selectable_label(view.range == r, text).clicked() {
                        view.range = r;
                    }
                }
            });
        let settings = icon_button_ex(ui, Icon::Settings, tr("Settings"), false, true);
        egui::Popup::menu(&settings).show(|ui| {
            if menus::toggle_item(ui, tr("Show Tooltips"), view.tooltips) {
                view.tooltips = !view.tooltips;
            }
            ui.separator();
            if menus::toggle_item(ui, tr("Disable FFT Analyzer"), !view.analyzer) {
                view.analyzer = !view.analyzer;
            }
            if menus::toggle_item(ui, tr("Disable Peak Meters"), !view.meters) {
                view.meters = !view.meters;
            }
            // How many times the rate the bands run at.
            menus::sub(ui, tr("Oversampling"), true, |ui| {
                let now = Settings::of(e).oversampling;
                for (k, name) in [(1u8, tr("Off")), (2, "2×"), (4, "4×"), (8, "8×")] {
                    if menus::check_item(ui, name, now == k, true, "") {
                        let mut s = Settings::of(e);
                        s.oversampling = k;
                        s.write(e);
                    }
                }
            });
        });
    });
}

/// The filter graph and what the mouse does on it.
fn graph(ui: &mut Ui, view: &mut View, s: &mut Settings, rate: f32, id: egui::Id) {
    let width = ui.available_width().max(320.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, GRAPH_H), Sense::hover());
    let plot = Rect::from_min_max(
        pos2(rect.left() + SCALE_W, rect.top()),
        pos2(rect.right(), rect.bottom() - SCALE_H),
    );
    let ax = Axes {
        plot,
        range: view.range,
    };
    let resp = ui.interact(plot, id.with("graph"), Sense::click_and_drag());
    resp.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Other, true, tr("Filter Graph"))
    });
    let hovered = resp.hover_pos().and_then(|p| hit(&ax, s, view.picked, p));

    // The wheel over a band with a width sets its Q (the window does not
    // scroll meanwhile).
    let wheel = ui.input(|i| i.smooth_scroll_delta.y);
    if wheel != 0.0
        && let Some(b) = hovered
        && s.bands[b].kind.has_q()
    {
        let band = &mut s.bands[b];
        band.q = (band.q * (wheel * 0.004).exp()).clamp(0.1, 30.0);
        ui.input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
    }
    // A drag moves a circle: frequency across, gain up and down.
    if resp.drag_started()
        && let Some(at) = ui.input(|i| i.pointer.press_origin())
    {
        view.drag = hit(&ax, s, view.picked, at).map(|b| {
            view.picked = Some(b);
            (b, ax.node(&s.bands[b]) - at)
        });
    }
    if resp.dragged()
        && let (Some((b, off)), Some(p)) = (view.drag, resp.interact_pointer_pos())
    {
        let at = p + off;
        let band = &mut s.bands[b];
        band.freq = ax.freq(at.x).clamp(F_LO, F_HI);
        if band.kind.has_gain() {
            band.gain_db = ax.db(at.y).clamp(-GAIN_MAX, GAIN_MAX);
        }
    }
    if !resp.dragged() {
        view.drag = None;
    }
    if resp.clicked() {
        resp.request_focus();
        if let Some(b) = resp
            .interact_pointer_pos()
            .and_then(|p| hit(&ax, s, view.picked, p))
        {
            view.picked = Some(b);
        }
    }
    // A double-click deletes a band, or adds one where it was.
    if resp.double_clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        match hit(&ax, s, view.picked, p) {
            Some(b) => {
                s.bands[b] = at_rest(b);
                view.picked = None;
            }
            None => {
                if let Some(b) = add_at(s, ax.freq(p.x), ax.db(p.y)) {
                    view.picked = Some(b);
                }
            }
        }
    }
    if resp.has_focus()
        && let Some(b) = view.picked
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete))
    {
        s.bands[b] = at_rest(b);
        view.picked = None;
    }
    if resp.secondary_clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        view.menu = Some(match hit(&ax, s, view.picked, p) {
            Some(b) => {
                view.picked = Some(b);
                MenuFor::Band(b)
            }
            None => MenuFor::Graph {
                freq: ax.freq(p.x),
                db: ax.db(p.y),
            },
        });
    }
    let menu = view.menu;
    resp.context_menu(|ui| {
        let copied: Option<Band> = ui.data(|d| d.get_temp(clipboard_id()));
        match menu {
            Some(MenuFor::Band(b)) => {
                if menus::item(ui, tr("Cut")) {
                    ui.data_mut(|d| d.insert_temp(clipboard_id(), s.bands[b]));
                    s.bands[b] = at_rest(b);
                    view.picked = None;
                    ui.close();
                }
                if menus::item(ui, tr("Copy")) {
                    ui.data_mut(|d| d.insert_temp(clipboard_id(), s.bands[b]));
                    ui.close();
                }
                if ui
                    .add_enabled_ui(copied.is_some(), |ui| menus::item(ui, tr("Replace")))
                    .inner
                    && let Some(c) = copied
                {
                    s.bands[b] = Band { used: true, ..c };
                    ui.close();
                }
                ui.separator();
                if menus::toggle_item(ui, tr("Bypass"), !s.bands[b].on) {
                    s.bands[b].on = !s.bands[b].on;
                    ui.close();
                }
                if menus::item(ui, tr("Delete Band")) {
                    s.bands[b] = at_rest(b);
                    view.picked = None;
                    ui.close();
                }
            }
            Some(MenuFor::Graph { freq, db }) => {
                let free = s.bands.iter().any(|b| !b.used);
                if ui
                    .add_enabled_ui(free, |ui| menus::item(ui, tr("Add Band Here")))
                    .inner
                {
                    view.picked = add_at(s, freq, db).or(view.picked);
                    ui.close();
                }
                if ui
                    .add_enabled_ui(free && copied.is_some(), |ui| menus::item(ui, tr("Paste")))
                    .inner
                    && let (Some(c), Some(b)) = (copied, s.bands.iter().position(|b| !b.used))
                {
                    s.bands[b] = Band { used: true, ..c };
                    view.picked = Some(b);
                    ui.close();
                }
            }
            None => ui.close(),
        }
    });
    if let Some(b) = hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        if view.tooltips && view.drag.is_none() {
            resp.clone().on_hover_text(band_text(b, &s.bands[b]));
        }
    }
    paint_graph(ui, rect, &ax, s, rate, view, hovered);
}

/// What a band is, in a line: its number, type, frequency, gain and Q.
fn band_text(b: usize, band: &Band) -> String {
    let mut parts = vec![
        trf!("Band {}", b + 1),
        tr(band.kind.name()).to_string(),
        text(Unit::Hz, band.freq, false),
    ];
    if band.kind.has_gain() {
        parts.push(text(Unit::Db, band.gain_db, false));
    }
    if band.kind.has_q() {
        parts.push(format!("Q {}", text(Unit::Plain, band.q, false)));
    }
    if band.kind.has_slope() {
        parts.push(trf!("{} dB/oct", band.slope));
    }
    parts.push(route_name(band.route).to_string());
    parts.join(" · ")
}

/// The graph: its grid and scales, Mono Below's line, each band's area
/// and curve, the whole curve and the circles.
fn paint_graph(
    ui: &Ui,
    rect: Rect,
    ax: &Axes,
    s: &Settings,
    rate: f32,
    view: &View,
    hovered: Option<usize>,
) {
    let plot = ax.plot;
    let painter = ui.painter_at(rect);
    painter.rect_filled(plot, 2.0, theme::panel_dark());
    let small = FontId::proportional(10.0);
    // The frequencies: a line every step of each decade, the main ones
    // named under the graph.
    let mut decade = 10.0f32;
    while decade <= 10_000.0 {
        for k in 1..10 {
            let f = decade * k as f32;
            if !(F_LO..=F_HI).contains(&f) {
                continue;
            }
            let x = ax.x(f);
            let main = matches!(k, 1 | 2 | 5);
            let stroke = if main {
                Stroke::new(1.0, theme::faint_line())
            } else {
                Stroke::new(1.0, theme::faint_line().gamma_multiply(0.45))
            };
            painter.vline(x, plot.y_range(), stroke);
            if main {
                let name = if f >= 1000.0 {
                    format!("{}k", f / 1000.0)
                } else {
                    format!("{f}")
                };
                // The ends' names inside the graph's width.
                let align = if f <= F_LO {
                    Align2::LEFT_TOP
                } else if f >= F_HI {
                    Align2::RIGHT_TOP
                } else {
                    Align2::CENTER_TOP
                };
                painter.text(
                    pos2(x, plot.bottom() + 2.0),
                    align,
                    name,
                    small.clone(),
                    theme::text_dim(),
                );
            }
        }
        decade *= 10.0;
    }
    // The dB: a line every step, 0 stronger, named on the left.
    let step = match view.range {
        r if r <= 3.0 => 1.0,
        r if r <= 6.0 => 3.0,
        r if r <= 18.0 => 6.0,
        _ => 10.0,
    };
    let mut db = -view.range;
    while db <= view.range + 1e-3 {
        let y = ax.y(db);
        let stroke = if db.abs() < 1e-3 {
            Stroke::new(1.0, theme::text_dim().gamma_multiply(0.6))
        } else {
            Stroke::new(1.0, theme::faint_line())
        };
        painter.hline(plot.x_range(), y, stroke);
        let name = if db.abs() < 1e-3 {
            "0".to_string()
        } else {
            crate::fmt::signed(f64::from(db), 0)
        };
        painter.text(
            pos2(plot.left() - 4.0, y),
            Align2::RIGHT_CENTER,
            name,
            small.clone(),
            theme::text_dim(),
        );
        db += step;
    }

    let clip = ui.painter_at(plot);
    // The analyzer: the output's spectrum behind the curves, its floor at
    // the graph's bottom and 0 dB at its top.
    if view.analyzer && view.spectrum.iter().any(|&v| v > FLOOR_DB) {
        let y_of =
            |db: f32| plot.bottom() - ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0) * plot.height();
        let pts: Vec<Pos2> = analysis_freqs()
            .iter()
            .zip(&view.spectrum)
            .map(|(&f, &db)| pos2(ax.x(f), y_of(db)))
            .collect();
        let fill = theme::text_dim().gamma_multiply(0.18);
        for w in pts.windows(2) {
            clip.add(egui::Shape::convex_polygon(
                vec![
                    w[0],
                    w[1],
                    pos2(w[1].x, plot.bottom()),
                    pos2(w[0].x, plot.bottom()),
                ],
                fill,
                Stroke::NONE,
            ));
        }
        clip.add(egui::Shape::line(
            pts,
            Stroke::new(1.0, theme::text_dim().gamma_multiply(0.6)),
        ));
    }
    // The peak meters: the input's and the output's, at the right.
    if view.meters {
        for (k, &db) in view.peaks.iter().enumerate() {
            let x1 = plot.right() - 3.0 - 8.0 * (1 - k) as f32;
            let bar = Rect::from_min_max(
                pos2(x1 - 6.0, plot.top() + 4.0),
                pos2(x1, plot.bottom() - 4.0),
            );
            clip.rect_filled(bar, 1.0, Color32::from_black_alpha(90));
            let t = ((db - METER_DB) / -METER_DB).clamp(0.0, 1.0);
            if t > 0.0 {
                let color = if db > -0.1 {
                    Color32::from_rgb(0xE8, 0x4A, 0x3C)
                } else if db > -12.0 {
                    Color32::from_rgb(0xE8, 0xC4, 0x3C)
                } else {
                    Color32::from_rgb(0x4C, 0xC8, 0x6A)
                };
                let lit =
                    Rect::from_min_max(pos2(bar.left(), bar.bottom() - t * bar.height()), bar.max);
                clip.rect_filled(lit, 1.0, color);
            }
        }
    }
    // A column every 2 points.
    let n = (plot.width() / 2.0).max(2.0) as usize;
    let xs: Vec<f32> = (0..=n)
        .map(|i| plot.left() + plot.width() * i as f32 / n as f32)
        .collect();
    let freqs: Vec<f32> = xs.iter().map(|&x| ax.freq(x)).collect();
    let line = |dbs: &[f32]| -> Vec<Pos2> {
        xs.iter()
            .zip(dbs)
            .map(|(&x, &d)| pos2(x, ax.y(d.clamp(-200.0, 200.0))))
            .collect()
    };
    if let Some(mono) = eq::mono_below_curve(s, rate, &freqs) {
        clip.add(egui::Shape::line(line(&mono), Stroke::new(1.5, MONO_GREEN)));
    }
    // Each band's area (the one picked last, on top) and its curve; a
    // bypassed band's curve dashed.
    let zero = ax.y(0.0);
    let mut order: Vec<usize> = (0..eq::BANDS).filter(|&b| s.bands[b].used).collect();
    order.sort_by_key(|&b| Some(b) == view.picked);
    for &b in &order {
        let band = &s.bands[b];
        let color = route_color(band.route);
        let dbs = eq::band_curve(&Band { on: true, ..*band }, s.scale, rate, &freqs);
        let points = line(&dbs);
        if band.on {
            let alpha = if Some(b) == view.picked { 0.32 } else { 0.18 };
            let fill = color.gamma_multiply(alpha);
            let mut mesh = egui::Mesh::default();
            for (i, p) in points.iter().enumerate() {
                mesh.colored_vertex(*p, fill);
                mesh.colored_vertex(pos2(p.x, zero), fill);
                if i > 0 {
                    let k = (2 * i) as u32;
                    mesh.add_triangle(k - 2, k - 1, k);
                    mesh.add_triangle(k - 1, k, k + 1);
                }
            }
            clip.add(egui::Shape::mesh(mesh));
            clip.add(egui::Shape::line(
                points,
                Stroke::new(1.0, color.gamma_multiply(0.8)),
            ));
        } else {
            clip.extend(egui::Shape::dashed_line(
                &points,
                Stroke::new(1.0, theme::text_disabled()),
                4.0,
                3.0,
            ));
        }
    }
    // The whole curve.
    let total = eq::total_curve(s, rate, &freqs);
    clip.add(egui::Shape::line(
        line(&total),
        Stroke::new(2.0, theme::text_strong()),
    ));
    // The circles, the one picked on top.
    for &b in &order {
        let band = &s.bands[b];
        let at = ax.node(band);
        let picked = Some(b) == view.picked;
        let color = if band.on {
            route_color(band.route)
        } else {
            theme::text_disabled()
        };
        if hovered == Some(b) {
            painter.circle_stroke(at, NODE + 3.0, Stroke::new(1.0, color.gamma_multiply(0.7)));
        }
        let (fill, ink) = if picked {
            (color, Color32::BLACK)
        } else {
            (theme::panel_dark(), color)
        };
        painter.circle(at, NODE, fill, Stroke::new(1.5, color));
        painter.text(
            at,
            Align2::CENTER_CENTER,
            (b + 1).to_string(),
            FontId::proportional(10.0),
            ink,
        );
    }
    if order.is_empty() {
        painter.text(
            plot.center() - vec2(0.0, 24.0),
            Align2::CENTER_CENTER,
            tr("Double-click the graph or press + to add a band."),
            FontId::proportional(12.0),
            theme::text_dim(),
        );
    }
}

/// The bands' toolbar: a button for each band in use (its dot turns it on
/// or off, the rest picks it) and + adding one.
fn toolbar(ui: &mut Ui, view: &mut View, s: &mut Settings) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for b in 0..eq::BANDS {
            if !s.bands[b].used {
                continue;
            }
            let band = s.bands[b];
            let (rect, resp) = ui.allocate_exact_size(CHIP, Sense::click());
            resp.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, trf!("Band {}", b + 1))
            });
            let dot = pos2(rect.left() + 10.0, rect.center().y);
            if resp.clicked() {
                let on_dot = resp
                    .interact_pointer_pos()
                    .is_some_and(|p| p.x < rect.left() + 18.0);
                if on_dot {
                    s.bands[b].on = !band.on;
                } else {
                    view.picked = Some(b);
                }
            }
            let color = if band.on {
                route_color(band.route)
            } else {
                theme::text_disabled()
            };
            let picked = view.picked == Some(b);
            let painter = ui.painter();
            painter.rect_filled(
                rect,
                3.0,
                if picked {
                    color.gamma_multiply(0.35)
                } else {
                    theme::panel_dark()
                },
            );
            painter.rect_stroke(
                rect,
                3.0,
                Stroke::new(if picked { 1.5 } else { 1.0 }, color),
                egui::StrokeKind::Inside,
            );
            if band.on {
                painter.circle_filled(dot, 4.0, color);
            } else {
                painter.circle_stroke(dot, 4.0, Stroke::new(1.0, color));
            }
            painter.text(
                pos2(rect.left() + 28.0, rect.center().y),
                Align2::CENTER_CENTER,
                (b + 1).to_string(),
                FontId::proportional(12.0),
                if band.on {
                    theme::text_strong()
                } else {
                    theme::text_disabled()
                },
            );
            resp.on_hover_text(tr("The dot turns the band on or off; the rest picks it."));
        }
        let free = s.bands.iter().any(|b| !b.used);
        if icon_button_ex(ui, Icon::Plus, tr("Add a band"), false, free).clicked()
            && let Some(b) = add_free(s)
        {
            view.picked = Some(b);
        }
    });
}

/// The band picked: its type, slope and routing, its knobs and its delete
/// button.
fn band_controls(ui: &mut Ui, view: &mut View, s: &mut Settings, id: egui::Id) {
    ui.vertical(|ui| {
        let Some(b) = view.picked.filter(|&b| s.bands[b].used) else {
            ui.label(egui::RichText::new(tr("No band picked.")).color(theme::text_dim()));
            return;
        };
        let band = &mut s.bands[b];
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(trf!("Band {}", b + 1)).strong());
            crate::widgets::checkbox(ui, &mut band.on, tr("On"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icon_button_ex(ui, Icon::Delete, tr("Delete the band"), false, true).clicked() {
                    *band = at_rest(b);
                    view.picked = None;
                }
            });
        });
        if !band.used {
            return;
        }
        ui.horizontal(|ui| {
            ui.label(tr("Filter Type:"));
            ui.spacing_mut().item_spacing.x = 2.0;
            for kind in BandType::ALL {
                if type_button(ui, kind, band.kind == kind).clicked() {
                    band.kind = kind;
                }
            }
            ui.add_space(8.0);
            ui.add_enabled_ui(band.kind.has_slope(), |ui| {
                egui::ComboBox::from_id_salt(id.with(("slope", b)))
                    .width(96.0)
                    .selected_text(trf!(
                        "{} dB/oct",
                        if band.kind.has_slope() {
                            band.slope
                        } else {
                            12
                        }
                    ))
                    .show_ui(ui, |ui| {
                        for slope in [6u8, 12, 18, 24, 30, 36] {
                            if ui
                                .selectable_label(band.slope == slope, trf!("{} dB/oct", slope))
                                .clicked()
                            {
                                band.slope = slope;
                            }
                        }
                    })
                    .response
                    .on_hover_text(tr("Slope"))
                    .on_disabled_hover_text(tr(
                        "Only the passes have a slope to pick; the others' is 12 dB/oct.",
                    ));
            });
        });
        ui.horizontal(|ui| {
            ui.label(tr("Channel Routing:"));
            ui.spacing_mut().item_spacing.x = 2.0;
            // Each routing underlined in its bands' color.
            for r in Route::ALL {
                let resp = ui.selectable_label(band.route == r, route_name(r));
                let under = resp.rect.shrink2(vec2(3.0, 0.0));
                ui.painter().hline(
                    under.x_range(),
                    under.bottom() - 1.0,
                    Stroke::new(2.0, route_color(r)),
                );
                if resp.clicked() {
                    band.route = r;
                }
            }
        });
        ui.horizontal(|ui| {
            Knob::FREQUENCY.show(ui, &mut band.freq, true);
            Knob::GAIN.show(ui, &mut band.gain_db, band.kind.has_gain());
            Knob::Q.show(ui, &mut band.q, band.kind.has_q());
        });
    });
}

/// Global Scale, Mono Below, Stereo Width and Output Gain.
fn global_controls(ui: &mut Ui, s: &mut Settings) {
    ui.vertical(|ui| {
        ui.label(egui::RichText::new(tr("Global")).strong());
        ui.horizontal(|ui| {
            let mut scale = s.scale * 100.0;
            if Knob::SCALE.show(ui, &mut scale, true) {
                s.scale = scale / 100.0;
            }
            Knob::MONO_BELOW.show(ui, &mut s.mono_below, true);
            let mut width = s.width * 100.0;
            if Knob::WIDTH.show(ui, &mut width, true) {
                s.width = width / 100.0;
            }
            Knob::OUTPUT.show(ui, &mut s.output_db, true);
        });
    });
}

/// A Filter Type button: the shape's curve, drawn.
fn type_button(ui: &mut Ui, kind: BandType, on: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 22.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tr(kind.name())));
    let visuals = ui.style().interact_selectable(&resp, on);
    let painter = ui.painter();
    painter.rect(
        rect,
        3.0,
        if on {
            ui.visuals().selection.bg_fill
        } else {
            visuals.bg_fill
        },
        visuals.bg_stroke,
        egui::StrokeKind::Inside,
    );
    // The shape over the whole graph's frequencies, ± 18 dB.
    let inner = rect.shrink2(vec2(4.0, 5.0));
    let n = 24;
    let freqs: Vec<f32> = (0..=n)
        .map(|i| F_LO * (F_HI / F_LO).powf(i as f32 / n as f32))
        .collect();
    let gain = if kind == BandType::LowShelf || kind == BandType::HighShelf {
        12.0
    } else {
        15.0
    };
    let sample = Band {
        q: 1.4,
        ..new_band(kind, (F_LO * F_HI).sqrt(), gain)
    };
    let dbs = eq::band_curve(&sample, 1.0, 48_000.0, &freqs);
    let points: Vec<Pos2> = dbs
        .iter()
        .enumerate()
        .map(|(i, &d)| {
            pos2(
                inner.left() + inner.width() * i as f32 / n as f32,
                inner.center().y - (d.clamp(-18.0, 18.0) / 18.0) * inner.height() / 2.0,
            )
        })
        .collect();
    painter.add(egui::Shape::line(
        points,
        Stroke::new(1.5, visuals.fg_stroke.color),
    ));
    resp.on_hover_text(tr(kind.name()))
}

/// How a knob's value is written.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Unit {
    Hz,
    Db,
    Percent,
    Plain,
}

/// A value as a knob writes it (`off`: under Mono Below's start, "Off").
fn text(unit: Unit, v: f32, off: bool) -> String {
    if off && v < eq::MONO_BELOW_OFF {
        return tr("Off").to_string();
    }
    let v = f64::from(v);
    match unit {
        Unit::Hz if v >= 1000.0 => format!("{} kHz", crate::fmt::num(v / 1000.0, 2)),
        Unit::Hz => format!("{} Hz", crate::fmt::num(v, 0)),
        Unit::Db => format!("{} dB", crate::fmt::signed(v, 1)),
        Unit::Percent => format!("{} %", crate::fmt::num(v, 0)),
        Unit::Plain => crate::fmt::num(v, 2),
    }
}

/// A value typed: kilohertz with a "k", "Off" for Mono Below's.
fn read(unit: Unit, typed: &str, off: bool) -> Option<f64> {
    let t = typed.trim().to_lowercase();
    if off && (t == "off" || t == tr("Off").to_lowercase()) {
        return Some(0.0);
    }
    let v = crate::fmt::parse(&t)?;
    Some(if unit == Unit::Hz && t.contains('k') {
        v * 1000.0
    } else {
        v
    })
}

/// A knob: its name over it, an arc from its start (from its middle when
/// `bipolar`) to the value, dragged up and down (with Shift, finer) and
/// back to its default with a double-click; under it the value, typed
/// into when clicked (Tab goes on to the next one).
struct Knob {
    name: &'static str,
    min: f32,
    max: f32,
    log: bool,
    bipolar: bool,
    default: f32,
    unit: Unit,
    /// Its start is "Off": the value 0 (Mono Below).
    off: bool,
}

/// How far up a knob "Off" takes.
const OFF_T: f32 = 0.06;

impl Knob {
    const FREQUENCY: Knob = Knob {
        name: "Frequency",
        min: F_LO,
        max: F_HI,
        log: true,
        bipolar: false,
        default: 1000.0,
        unit: Unit::Hz,
        off: false,
    };
    const GAIN: Knob = Knob {
        name: "Gain",
        min: -GAIN_MAX,
        max: GAIN_MAX,
        log: false,
        bipolar: true,
        default: 0.0,
        unit: Unit::Db,
        off: false,
    };
    const Q: Knob = Knob {
        name: "Q",
        min: 0.1,
        max: 30.0,
        log: true,
        bipolar: false,
        default: 1.0,
        unit: Unit::Plain,
        off: false,
    };
    const SCALE: Knob = Knob {
        name: "Global Scale",
        min: -200.0,
        max: 200.0,
        log: false,
        bipolar: true,
        default: 100.0,
        unit: Unit::Percent,
        off: false,
    };
    const MONO_BELOW: Knob = Knob {
        name: "Mono Below",
        min: eq::MONO_BELOW_OFF,
        max: 1000.0,
        log: true,
        bipolar: false,
        default: 0.0,
        unit: Unit::Hz,
        off: true,
    };
    const WIDTH: Knob = Knob {
        name: "Stereo Width",
        min: 0.0,
        max: 200.0,
        log: false,
        bipolar: true,
        default: 100.0,
        unit: Unit::Percent,
        off: false,
    };
    const OUTPUT: Knob = Knob {
        name: "Output Gain",
        min: -24.0,
        max: 24.0,
        log: false,
        bipolar: true,
        default: 0.0,
        unit: Unit::Db,
        off: false,
    };

    /// How far round (0 to 1) value `v` turns it.
    fn turn(&self, v: f32) -> f32 {
        if self.off && v < eq::MONO_BELOW_OFF {
            return 0.0;
        }
        let t = if self.log {
            (v.max(self.min) / self.min).ln() / (self.max / self.min).ln()
        } else {
            (v - self.min) / (self.max - self.min)
        };
        let t = t.clamp(0.0, 1.0);
        if self.off {
            OFF_T + (1.0 - OFF_T) * t
        } else {
            t
        }
    }

    /// The value it gives turned `t` of the way round.
    fn value(&self, t: f32) -> f32 {
        let mut t = t.clamp(0.0, 1.0);
        if self.off {
            if t < OFF_T {
                return 0.0;
            }
            t = (t - OFF_T) / (1.0 - OFF_T);
        }
        if self.log {
            self.min * (self.max / self.min).powf(t)
        } else {
            self.min + (self.max - self.min) * t
        }
    }

    /// Draws it for `v`; whether `v` changed.
    fn show(&self, ui: &mut Ui, v: &mut f32, enabled: bool) -> bool {
        let mut changed = false;
        let name = tr(self.name);
        // As wide as its name (other languages' are longer).
        let named = ui
            .painter()
            .layout_no_wrap(name.to_string(), FontId::proportional(11.0), Color32::WHITE)
            .size()
            .x;
        let width = KNOB_W.max(named + 8.0);
        ui.allocate_ui_with_layout(
            vec2(width, KNOB + 44.0),
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                ui.set_width(width);
                ui.add_enabled_ui(enabled, |ui| {
                    ui.label(egui::RichText::new(name).size(11.0));
                    let sense = if enabled {
                        Sense::click_and_drag()
                    } else {
                        Sense::hover()
                    };
                    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(KNOB), sense);
                    resp.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Slider, enabled, name)
                    });
                    if resp.dragged() {
                        let fine = ui.input(|i| i.modifiers.shift);
                        let per = if fine { 600.0 } else { 150.0 };
                        let t = self.turn(*v) - resp.drag_delta().y / per;
                        *v = self.value(t);
                        changed = true;
                    }
                    if resp.double_clicked() {
                        *v = self.default;
                        changed = true;
                    }
                    self.paint(ui, rect, *v, enabled);
                    let (unit, off) = (self.unit, self.off);
                    let lo = if off { 0.0 } else { self.min };
                    let speed = match unit {
                        Unit::Hz | Unit::Plain => (v.abs() * 0.01).max(0.01),
                        Unit::Db => 0.1,
                        Unit::Percent => 1.0,
                    };
                    let mut typed = *v;
                    ui.add(
                        egui::DragValue::new(&mut typed)
                            .range(lo..=self.max)
                            .speed(speed)
                            .custom_formatter(move |x, _| text(unit, x as f32, off))
                            .custom_parser(move |t| read(unit, t, off)),
                    )
                    .on_hover_text(name);
                    if typed != *v {
                        *v = typed;
                        changed = true;
                    }
                });
            },
        );
        changed
    }

    /// The knob's face: its track, the arc to the value and the pointer.
    fn paint(&self, ui: &Ui, rect: Rect, v: f32, enabled: bool) {
        let painter = ui.painter();
        let c = rect.center();
        let r = rect.width() / 2.0 - 3.0;
        // From the lower left round to the lower right.
        let angle = |t: f32| (135.0 + 270.0 * t).to_radians();
        let arc = |a: f32, b: f32| -> Vec<Pos2> {
            let steps = ((b - a).abs() * 48.0).ceil().max(1.0) as usize;
            (0..=steps)
                .map(|i| {
                    let t = a + (b - a) * i as f32 / steps as f32;
                    c + r * Vec2::angled(angle(t))
                })
                .collect()
        };
        painter.circle_filled(c, r - 3.0, theme::panel_dark());
        painter.add(egui::Shape::line(
            arc(0.0, 1.0),
            Stroke::new(3.0, theme::separator()),
        ));
        let t = self.turn(v);
        let from = if self.bipolar {
            self.turn(if self.off {
                0.0
            } else {
                (self.min + self.max) / 2.0
            })
        } else {
            0.0
        };
        let accent = if enabled {
            theme::banana()
        } else {
            theme::text_disabled()
        };
        if (t - from).abs() > 1e-3 {
            painter.add(egui::Shape::line(arc(from, t), Stroke::new(3.0, accent)));
        }
        painter.line_segment(
            [c, c + (r - 5.0) * Vec2::angled(angle(t))],
            Stroke::new(
                2.0,
                if enabled {
                    theme::text_strong()
                } else {
                    theme::text_disabled()
                },
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_clicks_add_the_type_the_place_asks_for() {
        let mut s = Settings::of(&Effect::new(EffectKind::ModernEq));
        let b = add_at(&mut s, 30.0, 4.0).unwrap();
        assert_eq!((b, s.bands[b].kind), (0, BandType::HighPass));
        assert_eq!(s.bands[b].gain_db, 0.0);
        let b = add_at(&mut s, 90.0, 4.0).unwrap();
        assert_eq!(s.bands[b].kind, BandType::LowShelf);
        assert_eq!(s.bands[b].gain_db, 4.0);
        assert_eq!(
            s.bands[add_at(&mut s, 1000.0, -3.0).unwrap()].kind,
            BandType::Peaking
        );
        assert_eq!(
            s.bands[add_at(&mut s, 10_000.0, 3.0).unwrap()].kind,
            BandType::HighShelf
        );
        assert_eq!(
            s.bands[add_at(&mut s, 18_000.0, 3.0).unwrap()].kind,
            BandType::LowPass
        );
        // Eight at most.
        for _ in 0..3 {
            assert!(add_at(&mut s, 500.0, 0.0).is_some());
        }
        assert!(add_at(&mut s, 500.0, 0.0).is_none());
        assert!(add_free(&mut s).is_none());
    }

    #[test]
    fn plus_adds_a_peak_where_there_is_most_room() {
        let mut s = Settings::of(&Effect::new(EffectKind::ModernEq));
        let b = add_free(&mut s).unwrap();
        // The middle of 20 Hz to 20 kHz, on the logarithmic scale.
        assert!((s.bands[b].freq - 632.5).abs() < 1.0, "{}", s.bands[b].freq);
        assert_eq!(s.bands[b].kind, BandType::Peaking);
        let c = add_free(&mut s).unwrap();
        assert!(s.bands[c].freq < 200.0 || s.bands[c].freq > 2000.0);
    }

    #[test]
    fn knobs_turn_through_their_ranges() {
        let k = Knob::FREQUENCY;
        assert!((k.turn(632.46) - 0.5).abs() < 1e-3);
        assert!((k.value(0.5) - 632.46).abs() < 0.1);
        let m = Knob::MONO_BELOW;
        assert_eq!(m.turn(0.0), 0.0);
        assert_eq!(m.value(0.02), 0.0);
        assert!((m.value(1.0) - 1000.0).abs() < 1e-2);
        assert!((m.value(m.turn(120.0)) - 120.0).abs() < 1e-2);
        assert_eq!(read(Unit::Hz, "2,5 kHz", false), Some(2500.0));
        assert_eq!(read(Unit::Hz, "off", true), Some(0.0));
        assert_eq!(read(Unit::Db, "−6 dB", false), Some(-6.0));
    }
}
