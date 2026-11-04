//! Color Grading (View › Window › Color Grading, Alt+G): VEGAS's panel,
//! editing the Color Grading effect (BananaV's own plug-in
//! `org.bananav.ColorGrading`: the first in the chain, added by the first
//! change) at an FX level — the event picked, its track, its media or the
//! video output —, laid out as VEGAS 23's screenshots show it. Its header
//! names the level (a menu of them) and what it grades and holds Bypass
//! Color Grading, Reset All and More (Bézier Mask, Broadcast Colors,
//! Export LUT); its navigation lists the groups, folded at first — LUTs,
//! Color Wheels and RL Color Wheels (each wheel or sliders), Utilities,
//! Curves (the Color and HSL Curves, pinned they stay in view at the
//! right, their left border dragged wider) — a changed item marked; a
//! group picked shows its items side by side in columns, an item alone
//! in half the width.
//! The Color Curves are `grading_curves.rs`'s. Each value is the effect's parameter at
//! the cursor (a key there when it is animated). The pickers (Lift's black
//! point, Gain's white point, Select Neutral Tone, the HSL Curves' Color
//! Selector) take a click or a rectangle on the Video Preview, read from
//! the effect's input there (`bv_engine::plugins::watch_input`); Show
//! Limits on Scopes has the Video Scopes draw the RL thresholds. Older
//! projects' Color Correction (`EffectKind::ColorGrade`) stays in their
//! chains.

use crate::i18n::tr;
use bv_ofx::{ParamInfo, Value};
use bv_timeline::{Effect, PluginFx, TrackKind};

use crate::app::BananaApp;
use crate::egui::{
    self, Align, Align2, Color32, FontId, Layout, Pos2, Rect, RichText, Sense, Stroke, UiBuilder,
    pos2, vec2,
};
use crate::fx_window::FxTarget;
use crate::icons::Icon;
use crate::theme;
use bv_core::curve::CurvePoint;
use bv_engine::ColorGradingPicker as Picker;
use bv_engine::color_grading as cg;

/// The Color Grading effect's identifier.
pub(crate) const ID: &str = "org.bananav.ColorGrading";

/// Where the panel grades.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Level {
    #[default]
    Event,
    Track,
    Media,
    VideoOutput,
}

impl Level {
    const ALL: [Level; 4] = [Level::Event, Level::Track, Level::Media, Level::VideoOutput];

    fn label(self) -> &'static str {
        match self {
            Level::Event => tr("Event FX:"),
            Level::Track => tr("Track FX:"),
            Level::Media => tr("Media FX:"),
            Level::VideoOutput => tr("Video Output FX:"),
        }
    }
}

/// The navigation's groups.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Section {
    #[default]
    Luts,
    Wheels,
    RlWheels,
    Utilities,
    Curves,
}

/// The navigation's groups: their names, sections and items.
const GROUPS: [(&str, Section, &[&str]); 5] = [
    ("LUTs", Section::Luts, &["Camera LUT", "Look LUT"]),
    (
        "Color Wheels",
        Section::Wheels,
        &["Lift", "Gamma", "Gain", "Offset"],
    ),
    (
        "RL Color Wheels",
        Section::RlWheels,
        &["Shadows", "Midtones", "Highlights", "Offset"],
    ),
    (
        "Utilities",
        Section::Utilities,
        &["Levels", "Tone", "White Balance", "HSL"],
    ),
    ("Curves", Section::Curves, &["Color Curves", "HSL Curves"]),
];

/// The parameters each item sets, by group (the navigation marks a
/// changed one).
const ITEM_PARAMS: [&[&[&str]]; 5] = [
    &[
        &[
            "lutEnable0",
            "lutFilename0",
            "lutInterpolation0",
            "lutGain0",
        ],
        &[
            "lutEnable1",
            "lutFilename1",
            "lutInterpolation1",
            "lutGain1",
        ],
    ],
    &[
        &["liftR", "liftG", "liftB", "liftY"],
        &["gammaR", "gammaG", "gammaB", "gammaY"],
        &["gainR", "gainG", "gainB", "gainY"],
        &["offsetR", "offsetG", "offsetB", "offsetY"],
    ],
    &[
        &[
            "logWheelShadowsR",
            "logWheelShadowsG",
            "logWheelShadowsB",
            "logWheelShadowsY",
        ],
        &[
            "logWheelMidtonesR",
            "logWheelMidtonesG",
            "logWheelMidtonesB",
            "logWheelMidtonesY",
        ],
        &[
            "logWheelHighlightsR",
            "logWheelHighlightsG",
            "logWheelHighlightsB",
            "logWheelHighlightsY",
        ],
        &[
            "logWheelOffsetR",
            "logWheelOffsetG",
            "logWheelOffsetB",
            "logWheelOffsetY",
        ],
    ],
    &[
        UTILITIES[0].1,
        UTILITIES[1].1,
        UTILITIES[2].1,
        UTILITIES[3].1,
    ],
    &[
        &COLOR_CURVES,
        &[
            "curveHvH", "curveHvS", "curveHvL", "curveSvS", "curveSvL", "curveLvS",
        ],
    ],
];

/// The Utilities' blocks: their titles and parameters.
const UTILITIES: [(&str, &[&str]); 4] = [
    (
        "Levels",
        &["inMinAdjust", "inMaxAdjust", "outMinAdjust", "outMaxAdjust"],
    ),
    (
        "Tone",
        &[
            "brightnessAdjust",
            "contrastAdjust",
            "contrastCtrAdjust",
            "saturationAdjust",
            "exposureLnAdjust",
        ],
    ),
    ("White Balance", &["temperatureAdjust", "tintAdjust"]),
    ("HSL", &["hueAdjust", "satAdjust", "valAdjust"]),
];

/// The Color Curves' texts, as the editor lists them: Brightness, Red,
/// Green, Blue.
const COLOR_CURVES: [&str; 4] = ["curveY", "curveR", "curveG", "curveB"];

/// The navigation's width and a row of it, as VEGAS's.
const NAV_WIDTH: f32 = 186.0;
const NAV_ROW: f32 = 26.0;
/// A column's title row.
const TITLE_ROW: f32 = 26.0;
/// A number's box.
const BOX: egui::Vec2 = vec2(48.0, 22.0);
/// The RL Color Wheels' Threshold column.
const THRESHOLD_WIDTH: f32 = 66.0;

/// The wheels' parameters' stems (each its R, G, B, Y, U and V) and
/// names: the Color Wheels, then the RL Color Wheels.
const WHEELS: [(&str, &str); 8] = [
    ("lift", "Lift"),
    ("gamma", "Gamma"),
    ("gain", "Gain"),
    ("offset", "Offset"),
    ("logWheelShadows", "Shadows"),
    ("logWheelMidtones", "Midtones"),
    ("logWheelHighlights", "Highlights"),
    ("logWheelOffset", "Offset"),
];

/// Wheel `i`'s range, as the panel's: its low and high ends, and where it
/// rests.
fn wheel_range(i: usize) -> (f64, f64, f64) {
    cg::WHEEL_RANGES.get(i).copied().unwrap_or(cg::RL_RANGE)
}

/// The HSL Curves, as the type menu lists them.
const HSL_KINDS: [&str; 6] = [
    "Hue vs Hue",
    "Hue vs Saturation",
    "Hue vs Luminance",
    "Saturation vs Saturation",
    "Saturation vs Luminance",
    "Luminance vs Saturation",
];

/// The pinned curves' width at first, and its least and most.
const PINNED_WIDTH: f32 = 470.0;
const PINNED_RANGE: std::ops::RangeInclusive<f32> = 320.0..=1400.0;

/// The panel's state.
#[derive(Debug)]
pub struct ColorPanel {
    pub level: Level,
    /// The navigation's group picked, and its item when one is: an item
    /// shows alone, a group every item of it side by side.
    pub section: Section,
    pub item: Option<usize>,
    /// The navigation's groups folded (all of them at first, as VEGAS's).
    pub folded: [bool; 5],
    /// Each wheel (the Color Wheels', then the RL ones') shown as sliders.
    pub sliders: [bool; 8],
    /// Color Curves and HSL Curves pinned beside the section, and how
    /// wide they are (their left border dragged).
    pub pinned: [bool; 2],
    pub pinned_width: f32,
    pub(crate) curves: crate::grading_curves::GradingCurves,
    hsl: HslView,
    /// The picker waiting for a click (or a rectangle) on the Video
    /// Preview.
    pub(crate) picking: Option<Picking>,
    /// The color under the pointer meanwhile (the effect's input there).
    hover: Option<[f32; 3]>,
    /// Show Limits on Scopes: the Video Scopes draw the RL thresholds.
    pub show_limits: bool,
    /// The panel was drawn this frame: a picker waits only while it is.
    drawn: bool,
    /// The picks made, each its own undo.
    picks: u64,
    /// The HSL Curves' grey swatch's color picker, open.
    hue_picker: Option<crate::color_picker::ColorPicker>,
}

/// A picker waiting on the Video Preview, and where a press on it began
/// (a rectangle is dragged from there).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Picking {
    picker: Picker,
    from: Option<Pos2>,
}

/// The HSL Curves' editor: the curve shown, its point picked or dragged.
#[derive(Clone, Debug, Default)]
struct HslView {
    kind: usize,
    picked: Option<usize>,
    drag: Option<usize>,
    /// A tangent's handle dragged: its point, and whether the one after it.
    handle: Option<(usize, bool)>,
    /// Every point's tangents shown (VEGAS's All tangents), else only the
    /// picked one's (Selected tangents).
    all_tangents: bool,
    /// Points move up and down only, their hue kept (VEGAS's Lock axis).
    lock_axis: bool,
    /// The hue at the left edge (the spectrum dragged sideways turns the
    /// hues round).
    shift: f64,
}

impl Default for ColorPanel {
    fn default() -> ColorPanel {
        ColorPanel {
            level: Level::Event,
            section: Section::Luts,
            item: None,
            folded: [true; 5],
            // VEGAS 23's: every wheel a wheel.
            sliders: [false; 8],
            pinned: [false; 2],
            pinned_width: PINNED_WIDTH,
            curves: Default::default(),
            hsl: HslView::default(),
            picking: None,
            hover: None,
            show_limits: false,
            drawn: false,
            picks: 0,
            hue_picker: None,
        }
    }
}

/// What the panel works on this frame: the level's target and its name,
/// the effect's chain, the Color Grading effect in it (or a new one) and
/// its parameters as the plug-in describes them.
struct Cx {
    target: FxTarget,
    name: String,
    place: crate::ofx::FxPlace,
    chain: Vec<Effect>,
    index: Option<usize>,
    fx: PluginFx,
    params: Vec<ParamInfo>,
}

fn new_fx() -> PluginFx {
    PluginFx {
        id: ID.to_string(),
        version: [2, 0],
        name: "Color Grading".to_string(),
        params: Default::default(),
    }
}

impl Cx {
    fn info(&self, name: &str) -> Option<&ParamInfo> {
        self.params.iter().find(|p| p.name == name)
    }

    fn value(&self, name: &str) -> Value {
        match self.info(name) {
            Some(i) => crate::ofx::value_now(i, &self.fx, &self.place),
            None => Value::None,
        }
    }

    fn num(&self, name: &str) -> f64 {
        self.value(name).num(0)
    }

    fn text(&self, name: &str) -> String {
        self.value(name).text().to_string()
    }

    fn default_of(&self, name: &str) -> Value {
        self.info(name).map_or(Value::None, |i| i.default.clone())
    }

    fn range(&self, name: &str) -> (f64, f64) {
        self.info(name).map_or((0.0, 1.0), |i| {
            (
                i.display_min.first().copied().unwrap_or(0.0),
                i.display_max.first().copied().unwrap_or(1.0),
            )
        })
    }

    /// Whether the effect applies (Bypass Color Grading off).
    fn on(&self) -> bool {
        self.index.is_none_or(|i| self.chain[i].enabled)
    }

    /// Every parameter's value at the cursor: what the effect works with.
    fn values(&self) -> Vec<(String, Value)> {
        self.params
            .iter()
            .map(|i| (i.name.clone(), self.value(&i.name)))
            .filter(|(_, v)| !matches!(v, Value::None))
            .collect()
    }

    /// The effect as the preview knows it (`None` until it is in the
    /// chain).
    fn effect_key(&self) -> Option<bv_engine::plugins::EffectKey> {
        use bv_timeline::scene::FxOwner;
        let owner = match self.target {
            FxTarget::Clip(c) => FxOwner::Event(c),
            FxTarget::Track(t) => FxOwner::Track(t),
            FxTarget::Media(m, _) => FxOwner::Media(m),
            FxTarget::VideoOutput => FxOwner::Output,
            _ => return None,
        };
        Some(bv_engine::plugins::EffectKey {
            owner,
            index: self.index?,
            id: ID.to_string(),
        })
    }
}

/// The context of the panel's level now (`None`: nothing to grade there,
/// or the plug-in missing).
fn context(app: &BananaApp) -> Option<Cx> {
    let p = app.project();
    let (target, name) = match app.color_panel.level {
        Level::Event => {
            let c = app
                .primary_clip()
                .filter(|&c| p.clip_track(c).is_some_and(|t| t.kind == TrackKind::Video))?;
            let name = p
                .clip(c)
                .and_then(|c| p.media(c.media))
                .map_or_else(String::new, crate::i18n::media_name);
            (FxTarget::Clip(c), name)
        }
        Level::Track => {
            let t = app
                .selected_track
                .filter(|&t| p.track(t).is_some_and(|t| t.kind == TrackKind::Video))?;
            (FxTarget::Track(t), p.track(t).map(crate::i18n::track_name)?)
        }
        Level::Media => {
            let m = app
                .selected_media
                .filter(|&m| p.media(m).is_some_and(|m| m.video.is_some()))?;
            (
                FxTarget::Media(m, false),
                p.media(m).map(crate::i18n::media_name)?,
            )
        }
        Level::VideoOutput => (FxTarget::VideoOutput, String::new()),
    };
    let chain = target.chain(p)?;
    let index = chain
        .iter()
        .position(|e| e.plugin.as_ref().is_some_and(|f| f.id == ID));
    let fx = index
        .and_then(|i| chain[i].plugin.as_deref().cloned())
        .unwrap_or_else(new_fx);
    let info = bv_ofx::registry().info(ID)?;
    let params = info
        .context(bv_ofx::Context::Filter)
        .or_else(|| info.described.first())?
        .params
        .clone();
    let place = crate::ofx::place_of(target, p, app.cursor);
    Some(Cx {
        target,
        name,
        place,
        chain,
        index,
        fx,
        params,
    })
}

/// Sets parameters (a change merged with those of the same `key`): the
/// effect added when it was not in the chain.
fn set(app: &mut BananaApp, cx: &Cx, values: &[(&str, Value)], key: egui::Id) {
    let mut chain = cx.chain.clone();
    let i = match cx.index {
        Some(i) => i,
        None => {
            chain.push(Effect::plugin(cx.fx.clone()));
            chain.len() - 1
        }
    };
    let Some(fx) = chain[i].plugin.as_mut() else {
        return;
    };
    for (name, v) in values {
        if let Some(info) = cx.info(name) {
            crate::ofx::set_now(info, fx, &cx.place, v.clone());
        }
    }
    let t = cx.target;
    app.modify_merged(key, move |p| t.set_chain(p, chain));
}

/// [`set`] with the parameters' names made as it runs.
fn set_named(app: &mut BananaApp, cx: &Cx, values: &[(String, Value)], key: egui::Id) {
    let values: Vec<(&str, Value)> = values
        .iter()
        .map(|(n, v)| (n.as_str(), v.clone()))
        .collect();
    set(app, cx, &values, key);
}

/// Changes the effect in the chain (bypass, reset); nothing when it is not
/// there.
fn change_effect(app: &mut BananaApp, cx: &Cx, f: impl FnOnce(&mut Effect)) {
    let Some(i) = cx.index else {
        return;
    };
    let mut chain = cx.chain.clone();
    f(&mut chain[i]);
    let t = cx.target;
    app.modify(move |p| t.set_chain(p, chain));
}

pub fn ui(ui: &mut egui::Ui, app: &mut BananaApp) {
    app.color_panel.drawn = true;
    let cx = context(app);
    header(ui, app, cx.as_ref());
    let Some(cx) = cx else {
        stop_picking(app);
        super::hint(
            ui,
            match app.color_panel.level {
                Level::Event => tr("Select a video event on the timeline to grade it."),
                Level::Track => tr("Select a video track to grade it."),
                Level::Media => tr("Select a video media in Project Media to grade it."),
                Level::VideoOutput => tr("Color Grading is not available."),
            },
        );
        return;
    };
    // A picker waiting: the preview keeps the effect's input for it.
    if app.color_panel.picking.is_some() {
        bv_engine::plugins::watch_input(cx.effect_key());
    }
    // The navigation, the border after it (two dark lines, as VEGAS's),
    // what it picked.
    let area = ui.available_rect_before_wrap();
    let nav = Rect::from_min_size(area.min, vec2(NAV_WIDTH, area.height()));
    let body = Rect::from_min_max(pos2(nav.right() + 6.0, area.top()), area.max);
    let line = Stroke::new(1.0, theme::panel_dark());
    for x in [nav.right() + 1.5, nav.right() + 4.5] {
        ui.painter().vline(x, area.y_range(), line);
    }
    let mut left = ui.new_child(
        UiBuilder::new()
            .max_rect(nav)
            .layout(Layout::top_down(Align::Min)),
    );
    egui::ScrollArea::vertical()
        .id_salt("cg-nav")
        .auto_shrink([false, false])
        .show(&mut left, |ui| navigation(ui, app, &cx));
    let mut right = ui.new_child(
        UiBuilder::new()
            .max_rect(body)
            .layout(Layout::top_down(Align::Min)),
    );
    content(&mut right, app, &cx);
    ui.advance_cursor_after_rect(area);
}

/// The header, as VEGAS's: the FX level — a menu of the others — and what
/// it grades, then Bypass Color Grading, Reset All and More at the right;
/// a dark line under it.
fn header(ui: &mut egui::Ui, app: &mut BananaApp, cx: Option<&Cx>) {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
    ui.painter().hline(
        row.x_range(),
        row.bottom() - 0.5,
        Stroke::new(1.0, theme::panel_dark()),
    );
    let inner = row.shrink2(vec2(6.0, 0.0));
    let mut left = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center)),
    );
    let (r, _) = left.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
    crate::icons::paint(left.painter(), r.shrink(2.0), Icon::Fx, theme::text());
    left.add_space(8.0);
    let level = app.color_panel.level;
    let text = left.painter().layout_no_wrap(
        level.label().to_string(),
        FontId::proportional(13.0),
        theme::text_strong(),
    );
    let (r, menu) = left.allocate_exact_size(vec2(text.size().x + 24.0, 22.0), Sense::click());
    if menu.hovered() {
        left.painter().rect_filled(r, 3.0, theme::hover());
    }
    left.painter().galley(
        pos2(r.left() + 3.0, r.center().y - text.size().y / 2.0),
        text,
        theme::text_strong(),
    );
    let m = pos2(r.right() - 9.0, r.center().y);
    left.painter().add(egui::Shape::convex_polygon(
        vec![
            m + vec2(-3.5, -1.5),
            m + vec2(3.5, -1.5),
            m + vec2(0.0, 2.5),
        ],
        theme::text(),
        Stroke::NONE,
    ));
    egui::Popup::menu(&menu).show(|ui| {
        for l in Level::ALL {
            if ui.selectable_label(level == l, l.label()).clicked() {
                stop_picking(app);
                app.color_panel.level = l;
                ui.close();
            }
        }
    });
    left.add_space(16.0);
    let Some(cx) = cx else {
        return;
    };
    left.label(RichText::new(&cx.name).color(theme::text()));
    let mut right = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::right_to_left(Align::Center)),
    );
    right.spacing_mut().item_spacing.x = 2.0;
    let more = crate::widgets::icon_button(&mut right, Icon::More, tr("More"));
    egui::Popup::menu(&more).show(|ui| {
        // Bézier Mask: VEGAS's Bézier Masking after the grade.
        if ui.button(tr("Bézier Mask")).clicked() {
            let mut chain = cx.chain.clone();
            chain.push(Effect::plugin(PluginFx {
                id: "org.bananav.BezierMasking".into(),
                version: [1, 0],
                name: "Bézier Masking".into(),
                params: Default::default(),
            }));
            let t = cx.target;
            app.modify(move |p| t.set_chain(p, chain));
            ui.close();
        }
        if ui.button(tr("Broadcast Colors")).clicked() {
            let mut chain = cx.chain.clone();
            chain.push(Effect::plugin(PluginFx {
                id: "org.bananav.BroadcastColors".into(),
                version: [1, 0],
                name: "Broadcast Colors".into(),
                params: Default::default(),
            }));
            let t = cx.target;
            app.modify(move |p| t.set_chain(p, chain));
            ui.close();
        }
        if ui.button(tr("Export LUT…")).clicked() {
            ui.close();
            export_lut(app, cx);
        }
    });
    if crate::widgets::icon_button(&mut right, Icon::Refresh, tr("Reset All")).clicked() {
        change_effect(app, cx, |e| {
            if let Some(fx) = e.plugin.as_mut() {
                fx.params.clear();
            }
        });
    }
    let bypassed = !cx.on();
    if crate::widgets::icon_toggle(
        &mut right,
        Icon::BypassFx,
        bypassed,
        tr("Bypass Color Grading"),
    )
    .clicked()
    {
        change_effect(app, cx, |e| e.enabled = bypassed);
    }
}

/// Export LUT: the grade at this level as a `.cube` file.
fn export_lut(app: &mut BananaApp, cx: &Cx) {
    let Some(path) = rfd::FileDialog::new()
        .set_title(tr("Export LUT"))
        .add_filter(tr("Cube LUT"), &["cube"])
        .set_file_name("Color Grading.cube")
        .save_file()
    else {
        return;
    };
    let text = bv_engine::Engine::color_grading_cube(&cx.values(), 33);
    match std::fs::write(&path, text) {
        Ok(()) => app.notify(trf!("LUT exported to {}", path.display())),
        Err(e) => app.report_error(trf!("Could not export the LUT: {}", e)),
    }
}

/// The navigation, as VEGAS's: each group a row — its fold, its name —
/// and, open, its items under it, a changed one marked, a wheel's view and
/// a curve's pin at their right. A click picks a group (its items side by
/// side) or an item (alone).
fn navigation(ui: &mut egui::Ui, app: &mut BananaApp, cx: &Cx) {
    ui.spacing_mut().item_spacing.y = 0.0;
    let width = ui.available_width();
    for (g, (name, section, items)) in GROUPS.iter().enumerate() {
        let folded = app.color_panel.folded[g];
        let on = app.color_panel.section == *section && app.color_panel.item.is_none();
        let (row, resp) = nav_row(ui, width, on);
        let fold =
            Rect::from_center_size(pos2(row.left() + 14.0, row.center().y), vec2(16.0, 16.0));
        let m = fold.center();
        let points = if folded {
            vec![
                m + vec2(-2.0, -4.0),
                m + vec2(2.5, 0.0),
                m + vec2(-2.0, 4.0),
            ]
        } else {
            vec![
                m + vec2(-4.0, -2.0),
                m + vec2(4.0, -2.0),
                m + vec2(0.0, 2.5),
            ]
        };
        ui.painter().add(egui::Shape::convex_polygon(
            points,
            theme::text(),
            Stroke::NONE,
        ));
        nav_text(ui, row, 35.0, tr(name));
        if resp.clicked() {
            if resp
                .interact_pointer_pos()
                .is_some_and(|p| fold.expand(3.0).contains(p))
            {
                app.color_panel.folded[g] = !folded;
            } else {
                app.color_panel.section = *section;
                app.color_panel.item = None;
            }
        }
        if folded {
            continue;
        }
        for (k, item) in items.iter().enumerate() {
            let on = app.color_panel.section == *section && app.color_panel.item == Some(k);
            let (row, resp) = nav_row(ui, width, on);
            if changed(cx, g, k) {
                asterisk(
                    ui.painter(),
                    pos2(row.left() + 27.0, row.center().y),
                    theme::text(),
                );
            }
            nav_text(ui, row, 47.0, tr(item));
            let button =
                Rect::from_center_size(pos2(row.right() - 16.0, row.center().y), vec2(20.0, 18.0));
            let id = ui.id().with(("cg-nav-button", g, k));
            match section {
                Section::Wheels | Section::RlWheels => {
                    let w = if *section == Section::Wheels {
                        k
                    } else {
                        4 + k
                    };
                    let s = app.color_panel.sliders[w];
                    if view_toggle(ui, button, id, s).clicked() {
                        app.color_panel.sliders[w] = !s;
                    }
                }
                Section::Curves => {
                    let p = app.color_panel.pinned[k];
                    if pin_toggle(ui, button, id, p).clicked() {
                        app.color_panel.pinned[k] = !p;
                    }
                }
                _ => {}
            }
            if resp.clicked() {
                app.color_panel.section = *section;
                app.color_panel.item = Some(k);
            }
        }
    }
}

/// A navigation row, VEGAS's blue when picked, lit under the pointer.
fn nav_row(ui: &mut egui::Ui, width: f32, on: bool) -> (Rect, egui::Response) {
    let (row, resp) = ui.allocate_exact_size(vec2(width, NAV_ROW), Sense::click());
    let lit = row.shrink2(vec2(3.0, 1.5));
    if on {
        ui.painter().rect_filled(lit, 0.0, theme::selection());
    } else if resp.hovered() {
        ui.painter().rect_filled(lit, 0.0, theme::hover());
    }
    (row, resp)
}

/// A navigation row's name, `x` from its left.
fn nav_text(ui: &egui::Ui, row: Rect, x: f32, text: &str) {
    ui.painter().text(
        pos2(row.left() + x, row.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(13.0),
        theme::text(),
    );
}

/// Whether item `k` of group `g` is set off its defaults: the
/// navigation's mark, as VEGAS's.
fn changed(cx: &Cx, g: usize, k: usize) -> bool {
    ITEM_PARAMS[g]
        .get(k)
        .is_some_and(|names| names.iter().any(|n| cx.value(n) != cx.default_of(n)))
}

/// The mark of a changed item: an asterisk.
fn asterisk(painter: &egui::Painter, at: Pos2, color: Color32) {
    for k in 0..3 {
        let a = std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::PI / 3.0;
        let d = vec2(a.cos(), a.sin()) * 4.5;
        painter.line_segment([at - d, at + d], Stroke::new(1.3, color));
    }
}

/// A wheel's view switch: a wheel's mark when it shows sliders (to show
/// the wheel), sliders' when it shows the wheel.
fn view_toggle(ui: &mut egui::Ui, rect: Rect, id: egui::Id, sliders: bool) -> egui::Response {
    let resp = ui.interact(rect, id, Sense::click());
    let painter = ui.painter();
    if resp.hovered() {
        painter.rect_stroke(
            rect,
            2.0,
            Stroke::new(1.0, theme::text_dim()),
            egui::StrokeKind::Inside,
        );
    }
    let c = if resp.hovered() {
        theme::text_strong()
    } else {
        theme::text()
    };
    if sliders {
        painter.circle_stroke(rect.center(), 5.5, Stroke::new(1.2, c));
        painter.circle_filled(rect.center() + vec2(1.5, -1.5), 1.6, c);
    } else {
        for (i, k) in [0.3, 0.7, 0.45].into_iter().enumerate() {
            let x = rect.center().x - 4.0 + i as f32 * 4.0;
            painter.line_segment(
                [pos2(x, rect.top() + 3.0), pos2(x, rect.bottom() - 3.0)],
                Stroke::new(1.0, c),
            );
            let y = rect.top() + 3.0 + k * (rect.height() - 6.0);
            painter.rect_filled(Rect::from_center_size(pos2(x, y), vec2(3.0, 2.0)), 0.0, c);
        }
    }
    resp.on_hover_text(if sliders {
        tr("Show as a wheel")
    } else {
        tr("Show as sliders")
    })
}

/// A curve's pin: pinned, it stays in view beside the other sections.
fn pin_toggle(ui: &mut egui::Ui, rect: Rect, id: egui::Id, on: bool) -> egui::Response {
    let resp = ui.interact(rect, id, Sense::click());
    let painter = ui.painter();
    let c = if on {
        theme::banana()
    } else if resp.hovered() {
        theme::text_strong()
    } else {
        theme::text()
    };
    let top = pos2(rect.center().x, rect.top() + 3.0);
    painter.rect_filled(
        Rect::from_center_size(top + vec2(0.0, 3.0), vec2(6.0, 6.0)),
        1.0,
        c,
    );
    painter.line_segment(
        [top + vec2(0.0, 6.0), pos2(top.x, rect.bottom() - 2.0)],
        Stroke::new(1.2, c),
    );
    painter.line_segment(
        [top + vec2(-5.0, 6.0), top + vec2(5.0, 6.0)],
        Stroke::new(1.2, c),
    );
    resp.on_hover_text(if on { tr("Unpin") } else { tr("Pin") })
}

/// What the navigation picked, as VEGAS lays it out: an item alone in
/// half the width, a group's items side by side in columns; the pinned
/// curves at the right, their left border dragged.
fn content(ui: &mut egui::Ui, app: &mut BananaApp, cx: &Cx) {
    let area = ui.max_rect();
    let (section, item) = (app.color_panel.section, app.color_panel.item);
    let shown = |k: usize| section == Section::Curves && item.is_none_or(|i| i == k);
    let side: Vec<usize> = (0..2)
        .filter(|&k| app.color_panel.pinned[k] && !shown(k))
        .collect();
    let mut main = area;
    if !side.is_empty() {
        let w = app
            .color_panel
            .pinned_width
            .min(area.width() - 240.0)
            .max(*PINNED_RANGE.start());
        let border = Rect::from_min_max(
            pos2(area.right() - w - 6.0, area.top()),
            pos2(area.right() - w, area.bottom()),
        );
        main.max.x = border.left();
        let r = ui
            .interact(border, egui::Id::new("cg-pinned-border"), Sense::drag())
            .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
        let color = if r.hovered() || r.dragged() {
            theme::text()
        } else {
            theme::panel_dark()
        };
        ui.painter()
            .vline(border.center().x, border.y_range(), Stroke::new(1.0, color));
        if r.dragged() {
            let w = app.color_panel.pinned_width - r.drag_delta().x;
            app.color_panel.pinned_width = w.clamp(*PINNED_RANGE.start(), *PINNED_RANGE.end());
        }
        let pinned = Rect::from_min_max(pos2(border.right(), area.top()), area.max);
        let h = pinned.height() / side.len() as f32;
        for (j, &k) in side.iter().enumerate() {
            let r = Rect::from_min_size(
                pos2(pinned.left(), pinned.top() + j as f32 * h),
                vec2(pinned.width(), h),
            );
            curves_column(ui, app, cx, k, r);
        }
    }
    match (section, item) {
        (Section::Luts, None) => {
            for (k, r) in split(main, 2).into_iter().enumerate() {
                lut_column(ui, app, cx, k, r);
            }
        }
        (Section::Luts, Some(k)) => lut_column(ui, app, cx, k.min(1), half(main)),
        (Section::Wheels, None) => {
            for (k, r) in split(main, 4).into_iter().enumerate() {
                wheel_column(ui, app, cx, k, r);
            }
        }
        (Section::Wheels, Some(k)) => wheel_column(ui, app, cx, k.min(3), half(main)),
        (Section::RlWheels, _) => {
            let th = Rect::from_min_size(main.min, vec2(THRESHOLD_WIDTH, main.height()));
            threshold_column(ui, app, cx, th);
            let rest = Rect::from_min_max(pos2(th.right(), main.top()), main.max);
            match item {
                None => {
                    for (k, r) in split(rest, 4).into_iter().enumerate() {
                        wheel_column(ui, app, cx, 4 + k, r);
                    }
                }
                Some(k) => wheel_column(ui, app, cx, 4 + k.min(3), half(rest)),
            }
        }
        (Section::Utilities, _) => utilities(ui, app, cx, main, item),
        (Section::Curves, None) => {
            for (k, r) in split(main, 2).into_iter().enumerate() {
                curves_column(ui, app, cx, k, r);
            }
        }
        (Section::Curves, Some(k)) => curves_column(ui, app, cx, k.min(1), half(main)),
    }
}

/// `r` cut into `n` columns side by side.
fn split(r: Rect, n: usize) -> Vec<Rect> {
    let w = r.width() / n as f32;
    (0..n)
        .map(|k| Rect::from_min_size(pos2(r.left() + k as f32 * w, r.top()), vec2(w, r.height())))
        .collect()
}

/// An item alone: the left half of `r`, as VEGAS's (all of it when it is
/// narrow).
fn half(r: Rect) -> Rect {
    let w = if r.width() < 640.0 {
        r.width()
    } else {
        r.width() / 2.0
    };
    Rect::from_min_size(r.min, vec2(w, r.height()))
}

/// A column: `title` on its title row, its background lit while the
/// pointer is over it when `lit` (as VEGAS's wheels). The title row's ui
/// (its buttons, from the right) and the body's (under it).
fn column(ui: &mut egui::Ui, rect: Rect, title: &str, lit: bool) -> (egui::Ui, egui::Ui) {
    if lit && ui.rect_contains_pointer(rect) {
        ui.painter().rect_filled(rect, 0.0, theme::hover());
    }
    let row = Rect::from_min_size(rect.min, vec2(rect.width(), TITLE_ROW));
    ui.painter().text(
        pos2(row.left() + 11.0, row.center().y),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(13.0),
        theme::text_strong(),
    );
    let mut bar = ui.new_child(
        UiBuilder::new()
            .max_rect(row.shrink2(vec2(6.0, 0.0)))
            .layout(Layout::right_to_left(Align::Center)),
    );
    bar.spacing_mut().item_spacing.x = 2.0;
    let body = ui.new_child(
        UiBuilder::new()
            .max_rect(Rect::from_min_max(
                pos2(rect.left(), row.bottom()),
                rect.max,
            ))
            .layout(Layout::top_down(Align::Min)),
    );
    (bar, body)
}

/// A title row's Reset.
fn reset_button(ui: &mut egui::Ui) -> bool {
    crate::widgets::icon_button(ui, Icon::Refresh, tr("Reset")).clicked()
}

/// A number's field, written as the interface's language writes numbers
/// (typing takes a comma or a point).
fn field(value: &mut f64) -> egui::DragValue<'_> {
    egui::DragValue::new(value)
        .speed(0.005)
        .max_decimals(3)
        .custom_formatter(|v, _| crate::fmt::num(v, 3))
        .custom_parser(crate::fmt::parse)
}

/// A slider's box: as few decimals as the number needs, as VEGAS's ("1",
/// "0.5").
fn short_field(value: &mut f64) -> egui::DragValue<'_> {
    field(value).custom_formatter(|v, _| {
        let s = crate::fmt::num(v, 3);
        let sep = crate::i18n::language().decimal_separator();
        if s.contains(sep) {
            s.trim_end_matches('0').trim_end_matches(sep).to_string()
        } else {
            s
        }
    })
}

/// How far from a row's left its slider starts: past the longest of
/// `labels` (its language's), at least `least`.
fn labels_end(ui: &egui::Ui, labels: &[&str], least: f32) -> f32 {
    let widest = labels
        .iter()
        .map(|l| {
            ui.painter()
                .layout_no_wrap(l.to_string(), FontId::proportional(12.5), theme::text())
                .size()
                .x
        })
        .fold(0.0, f32::max);
    (23.0 + widest + 14.0).max(least)
}

/// A label at `at`.
fn label_at(ui: &egui::Ui, at: Pos2, text: &str, align: Align2) {
    ui.painter()
        .text(at, align, text, FontId::proportional(12.5), theme::text());
}

/// A number's box as VEGAS's: typed (a comma or a point) and kept to three
/// decimals within `(lo, hi)`, the mouse wheel over it adding a hundredth
/// a notch (Ctrl: a thousandth). Whether it changed `value`.
fn number_box(
    ui: &mut egui::Ui,
    rect: Rect,
    value: &mut f64,
    (lo, hi): (f64, f64),
    short: bool,
) -> bool {
    let mut v = *value;
    let field = if short {
        short_field(&mut v)
    } else {
        field(&mut v)
    };
    let resp = ui.put(rect, field.range(lo..=hi));
    let mut changed = resp.changed();
    if resp.hovered() {
        let (notches, ctrl) = ui.input(|i| crate::widgets::wheel_notches(&i.events));
        if notches != 0.0 {
            v += f64::from(notches) * if ctrl { 0.001 } else { 0.01 };
            ui.input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
            changed = true;
        }
    }
    if changed {
        *value = (v.clamp(lo, hi) * 1000.0).round() / 1000.0;
    }
    changed
}

/// A slider's handling, as VEGAS's sliders do it: the knob dragged (with
/// Ctrl a thirty-second as fast), a click beside it a tenth that way, a
/// double-click the default, the mouse wheel a hundredth a notch (Ctrl: a
/// thousandth). `along(p)`: how far along the slider a point is (0–1 from
/// its low end); `on_knob(p)`: whether the knob is there. Whether `value`
/// changed.
fn slide(
    ui: &egui::Ui,
    resp: &egui::Response,
    value: &mut f64,
    (lo, hi): (f64, f64),
    default: f64,
    along: impl Fn(Pos2) -> f64,
    on_knob: impl Fn(Pos2) -> bool,
) -> bool {
    let span = (hi - lo).max(1e-12);
    let before = *value;
    // The drag's value, not held within the ends: the knob comes back
    // with the pointer.
    let id = resp.id.with("slide");
    if resp.drag_started() {
        let grabbed = ui.input(|i| i.pointer.press_origin()).is_some_and(&on_knob);
        ui.data_mut(|d| d.insert_temp(id, grabbed.then_some(*value)));
    }
    if resp.dragged()
        && let Some(Some(mut v)) = ui.data(|d| d.get_temp::<Option<f64>>(id))
        && let Some(p) = resp.interact_pointer_pos()
    {
        let moved = along(p) - along(p - resp.drag_delta());
        let k = if ui.input(|i| i.modifiers.ctrl) {
            1.0 / 32.0
        } else {
            1.0
        };
        v += moved * span * k;
        ui.data_mut(|d| d.insert_temp(id, Some(v)));
        *value = v;
    }
    if resp.clicked()
        && let Some(p) = resp.interact_pointer_pos()
        && !on_knob(p)
    {
        *value += if lo + along(p) * span > *value {
            0.1
        } else {
            -0.1
        };
    }
    if resp.double_clicked() {
        *value = default;
    }
    if resp.hovered() {
        let (notches, ctrl) = ui.input(|i| crate::widgets::wheel_notches(&i.events));
        if notches != 0.0 {
            *value += f64::from(notches) * if ctrl { 0.001 } else { 0.01 };
            ui.input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
        }
    }
    *value = value.clamp(lo, hi);
    *value != before
}

/// A slider across `rect` as VEGAS's: a groove — or a band through
/// `colors` — and a light knob, handled as [`slide`] says.
fn h_slider(
    ui: &mut egui::Ui,
    rect: Rect,
    id: egui::Id,
    value: &mut f64,
    (lo, hi): (f64, f64),
    default: f64,
    colors: &[Color32],
) -> egui::Response {
    let mut resp = ui.interact(rect, id, Sense::click_and_drag());
    let track = rect.shrink2(vec2(5.0, 0.0));
    let span = (hi - lo).max(1e-12);
    let y = rect.center().y;
    let knob = |v: f64| {
        let x = track.left() + ((v - lo) / span).clamp(0.0, 1.0) as f32 * track.width();
        Rect::from_center_size(pos2(x, y), vec2(9.0, 15.0))
    };
    let at = knob(*value);
    if slide(
        ui,
        &resp,
        value,
        (lo, hi),
        default,
        |p| f64::from((p.x - track.left()) / track.width()),
        |p| at.expand(2.0).contains(p),
    ) {
        resp.mark_changed();
    }
    band(ui.painter(), track.x_range(), y, colors);
    crate::widgets::paint_knob(ui.painter(), knob(*value), resp.hovered() || resp.dragged());
    resp
}

/// A slider's groove: a dark line, or a band through `colors`.
fn band(painter: &egui::Painter, xs: egui::Rangef, y: f32, colors: &[Color32]) {
    if colors.len() < 2 {
        painter.hline(xs, y, Stroke::new(2.0, theme::panel_dark()));
        return;
    }
    let mut mesh = egui::Mesh::default();
    let n = colors.len() - 1;
    for (k, c) in colors.iter().enumerate() {
        let x = xs.min + xs.span() * k as f32 / n as f32;
        mesh.colored_vertex(pos2(x, y - 1.5), *c);
        mesh.colored_vertex(pos2(x, y + 1.5), *c);
    }
    for k in 0..n as u32 {
        let a = 2 * k;
        mesh.add_triangle(a, a + 1, a + 2);
        mesh.add_triangle(a + 1, a + 3, a + 2);
    }
    painter.add(mesh);
}

/// A number's row, as VEGAS's: its name, its slider `label_w` from the
/// row's left — over `colors` — and its box at the right.
#[allow(clippy::too_many_arguments)]
fn slider_row(
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    cx: &Cx,
    row: Rect,
    name: &str,
    label: &str,
    label_w: f32,
    colors: &[Color32],
) {
    let y = row.center().y;
    label_at(ui, pos2(row.left() + 23.0, y), label, Align2::LEFT_CENTER);
    let bx = Rect::from_min_size(pos2(row.right() - 5.0 - BOX.x, y - BOX.y / 2.0), BOX);
    let track = Rect::from_min_max(
        pos2(row.left() + label_w, row.top()),
        pos2(bx.left() - 12.0, row.bottom()),
    );
    let (min, max) = cx.range(name);
    let id = egui::Id::new(("cg-slider", name));
    let mut v = cx.num(name);
    h_slider(
        ui,
        track,
        id,
        &mut v,
        (min, max),
        cx.default_of(name).num(0),
        colors,
    );
    number_box(ui, bx, &mut v, (min, max), true);
    if (v - cx.num(name)).abs() > 1e-12 {
        set(app, cx, &[(name, Value::Num(vec![v]))], id);
    }
}

/// Which of two knobs (`knobs`: their places) the pointer at `p` takes:
/// the one under it, else the nearer (`near`: how far each is).
fn which_knob(p: Pos2, knobs: [Rect; 2], near: impl Fn(Rect) -> f32) -> usize {
    let on = knobs.map(|k| k.expand(2.0).contains(p));
    match on {
        [true, _] => 0,
        [false, true] => 1,
        _ => usize::from(near(knobs[1]) < near(knobs[0])),
    }
}

/// Levels' Input or Output row: the low end's box, a black-to-white band
/// with both ends' knobs, the high end's box. Each end is handled as
/// VEGAS's sliders handle theirs ([`slide`]: the knob under the pointer,
/// or beside them the nearer, takes the drag, the click and the mouse
/// wheel); a double-click puts both at their defaults.
#[allow(clippy::too_many_arguments)]
fn range_row(
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    cx: &Cx,
    row: Rect,
    label: &str,
    label_w: f32,
    (lo_p, hi_p): (&str, &str),
) {
    let y = row.center().y;
    label_at(ui, pos2(row.left() + 23.0, y), label, Align2::LEFT_CENTER);
    let lo_box = Rect::from_min_size(pos2(row.left() + label_w, y - BOX.y / 2.0), BOX);
    let hi_box = Rect::from_min_size(pos2(row.right() - 5.0 - BOX.x, y - BOX.y / 2.0), BOX);
    let rect = Rect::from_min_max(
        pos2(lo_box.right() + 4.0, row.top()),
        pos2(hi_box.left() - 12.0, row.bottom()),
    );
    let (min, max) = cx.range(lo_p);
    let span = (max - min).max(1e-12);
    let (lo0, hi0) = (cx.num(lo_p), cx.num(hi_p));
    let mut pair = [lo0, hi0];
    let defaults = [cx.default_of(lo_p).num(0), cx.default_of(hi_p).num(0)];
    let id = egui::Id::new(("cg-range", lo_p));
    let resp = ui.interact(rect, id, Sense::click_and_drag());
    let track = rect.shrink2(vec2(5.0, 0.0));
    let x_of = |v: f64| track.left() + ((v - min) / span).clamp(0.0, 1.0) as f32 * track.width();
    let knob = |v: f64| Rect::from_center_size(pos2(x_of(v), y), vec2(9.0, 15.0));
    // The end the pointer took, kept while it drags.
    let end = id.with("end");
    let k = if resp.dragged() && !resp.drag_started() {
        ui.data(|d| d.get_temp::<usize>(end)).unwrap_or(0)
    } else {
        let at = ui.input(|i| i.pointer.press_origin().or(i.pointer.hover_pos()));
        let k = at.map_or(0, |p| {
            which_knob(p, [knob(pair[0]), knob(pair[1])], |r| {
                (p.x - r.center().x).abs()
            })
        });
        ui.data_mut(|d| d.insert_temp(end, k));
        k
    };
    let grabbed = knob(pair[k]);
    slide(
        ui,
        &resp,
        &mut pair[k],
        (min, max),
        defaults[k],
        |p| f64::from((p.x - track.left()) / track.width()),
        |p| grabbed.expand(2.0).contains(p),
    );
    if resp.double_clicked() {
        pair = defaults;
    } else if k == 0 {
        pair[0] = pair[0].min(pair[1]);
    } else {
        pair[1] = pair[1].max(pair[0]);
    }
    band(
        ui.painter(),
        track.x_range(),
        y,
        &[Color32::BLACK, Color32::WHITE],
    );
    for v in pair {
        crate::widgets::paint_knob(ui.painter(), knob(v), resp.hovered() || resp.dragged());
    }
    number_box(ui, lo_box, &mut pair[0], (min, max), true);
    number_box(ui, hi_box, &mut pair[1], (min, max), true);
    let [lo, hi] = pair;
    if (lo - lo0).abs() > 1e-12 || (hi - hi0).abs() > 1e-12 {
        set(
            app,
            cx,
            &[(lo_p, Value::Num(vec![lo])), (hi_p, Value::Num(vec![hi]))],
            id,
        );
    }
}

/// The LUTs' entries: none, BananaV's (Apple Log's curves; the looks)
/// — each its name, its file and whether it is there yet.
fn lut_entries(camera: bool) -> Vec<(String, String, bool)> {
    let mut entries = vec![(tr("None").to_string(), String::new(), true)];
    if camera {
        entries.push((
            tr("Apple Log to Linear").into(),
            bv_engine::APPLE_LOG_TO_LINEAR.into(),
            true,
        ));
        entries.push((
            tr("Apple Log to REC.709").into(),
            bv_engine::APPLE_LOG_TO_REC709.into(),
            true,
        ));
        entries.push((tr("Samsung Log to Linear").into(), String::new(), false));
        entries.push((tr("Samsung Log to REC.709").into(), String::new(), false));
    } else {
        for (name, values) in crate::presets::builtin_plugin("org.bananav.LutFilter") {
            if let Some(bv_timeline::PluginParam {
                value: bv_timeline::PluginValue::Text(f),
                ..
            }) = values.get("file")
            {
                entries.push((name, f.clone(), true));
            }
        }
    }
    entries
}

/// A LUT's column (`k`: 0 the Camera LUT, 1 the Look LUT), as VEGAS's:
/// Tetrahedral Interpolation, LUT Selection, Reset and Browse on its title
/// row; its table in a box — its picture, its name, the arrows to the one
/// before or after; a click lists them —; its Strength.
fn lut_column(ui: &mut egui::Ui, app: &mut BananaApp, cx: &Cx, k: usize, rect: Rect) {
    let (title, camera) = [(tr("Camera LUT"), true), (tr("Look LUT"), false)][k];
    let (file_p, strength_p, tetra_p, on_p) = (
        cg::LUT_FILE[k],
        cg::LUT_GAIN[k],
        cg::LUT_INTERPOLATION[k],
        cg::LUT_ENABLE[k],
    );
    // A table picked: its file, and the LUT on (as VEGAS's list does).
    let pick = |file: String| {
        let on = if file.is_empty() { 0.0 } else { 1.0 };
        [(file_p, Value::Text(file)), (on_p, Value::Num(vec![on]))]
    };
    let (mut bar, mut body) = column(ui, rect, title, false);
    if crate::widgets::icon_button(&mut bar, Icon::Folder, tr("Browse")).clicked()
        && let Some(path) = rfd::FileDialog::new()
            .add_filter(tr("Cube LUT"), &["cube"])
            .pick_file()
    {
        let key = egui::Id::new(("cg-lut-file", k));
        set(app, cx, &pick(path.to_string_lossy().into_owned()), key);
    }
    if reset_button(&mut bar) {
        let key = egui::Id::new(("cg-lut-reset", k));
        let values: Vec<(&str, Value)> = [file_p, on_p, tetra_p, strength_p]
            .into_iter()
            .map(|n| (n, cx.default_of(n)))
            .collect();
        set(app, cx, &values, key);
    }
    // Lit while a table is picked and left off.
    let on = cx.num(on_p) != 0.0;
    let bypassed = !on && !cx.text(file_p).is_empty();
    if crate::widgets::icon_toggle(
        &mut bar,
        Icon::BypassFx,
        bypassed,
        tr("LUT Selection: apply or remove the LUT"),
    )
    .clicked()
    {
        let key = egui::Id::new(("cg-lut-on", k));
        set(
            app,
            cx,
            &[(on_p, Value::Num(vec![if on { 0.0 } else { 1.0 }]))],
            key,
        );
    }
    let tetra = cx.num(tetra_p) != 0.0;
    if crate::widgets::icon_toggle(&mut bar, Icon::Grid, tetra, tr("Tetrahedral Interpolation"))
        .clicked()
    {
        let key = egui::Id::new(("cg-lut-tetra", k));
        set(
            app,
            cx,
            &[(tetra_p, Value::Num(vec![if tetra { 0.0 } else { 1.0 }]))],
            key,
        );
    }
    let file = cx.text(file_p);
    let entries = lut_entries(camera);
    let at = entries
        .iter()
        .position(|(_, f, _)| *f == file && (!f.is_empty() || file.is_empty()));
    let shown = at.map(|i| entries[i].0.clone()).unwrap_or_else(|| {
        std::path::Path::new(&file)
            .file_stem()
            .map_or_else(|| file.clone(), |s| s.to_string_lossy().into_owned())
    });
    let top = body.max_rect().top();
    label_at(
        &body,
        pos2(rect.left() + 23.0, top + 33.0),
        tr("LUT:"),
        Align2::LEFT_CENTER,
    );
    let boxr = Rect::from_min_max(
        pos2(
            rect.left() + labels_end(&body, &[tr("LUT:")], 99.0),
            top + 3.0,
        ),
        pos2(rect.right() - 4.0, top + 63.0),
    );
    let painter = body.painter().clone();
    painter.rect_filled(boxr, 0.0, theme::panel_dark());
    // Its picture: a cross for none, else a little lattice of colors.
    let thumb = Rect::from_min_size(
        pos2(boxr.left() + 8.0, boxr.center().y - 22.0),
        vec2(44.0, 44.0),
    );
    if file.is_empty() {
        painter.rect_filled(thumb, 2.0, Color32::from_gray(208));
        let s = Stroke::new(1.6, Color32::from_gray(34));
        let c = thumb.shrink(14.0);
        painter.line_segment([c.left_top(), c.right_bottom()], s);
        painter.line_segment([c.right_top(), c.left_bottom()], s);
    } else {
        for j in 0..4 {
            for i in 0..4 {
                let h = (i + 4 * j) as f32 / 16.0;
                let [r, g, b] = crate::widgets::hue_rgb(h * std::f32::consts::TAU)
                    .map(|v| (60.0 + 150.0 * v) as u8);
                let cell = Rect::from_min_size(
                    thumb.min + vec2(i as f32 * 11.0, j as f32 * 11.0),
                    vec2(11.0, 11.0),
                );
                painter.rect_filled(cell, 0.0, Color32::from_rgb(r, g, b));
            }
        }
    }
    crate::widgets::fit_text(
        &painter,
        pos2(thumb.right() + 12.0, boxr.center().y),
        Align2::LEFT_CENTER,
        &shown,
        FontId::proportional(13.0),
        theme::text(),
        (boxr.right() - 30.0 - thumb.right() - 12.0).max(10.0),
    );
    // The list, and the arrows to the entry before or after.
    let list_rect = Rect::from_min_max(boxr.min, pos2(boxr.right() - 26.0, boxr.bottom()));
    let list = body.interact(list_rect, egui::Id::new(("cg-lut-box", k)), Sense::click());
    let mut chosen: Option<String> = None;
    egui::Popup::menu(&list).show(|ui| {
        for (name, f, enabled) in &entries {
            let r = ui.add_enabled(
                *enabled,
                egui::Button::selectable(*f == file && *enabled, name.as_str()),
            );
            let r = if *enabled {
                r
            } else {
                r.on_disabled_hover_text(tr("Coming soon to BananaV"))
            };
            if r.clicked() {
                chosen = Some(f.clone());
            }
        }
    });
    let usable: Vec<usize> = (0..entries.len()).filter(|&i| entries[i].2).collect();
    for (up, center) in [
        (true, pos2(boxr.right() - 14.0, boxr.top() + 15.0)),
        (false, pos2(boxr.right() - 14.0, boxr.bottom() - 15.0)),
    ] {
        let r = Rect::from_center_size(center, vec2(20.0, 22.0));
        let b = body.interact(r, egui::Id::new(("cg-lut-step", k, up)), Sense::click());
        let c = if b.hovered() {
            theme::text_strong()
        } else {
            theme::text()
        };
        let m = r.center();
        let points = if up {
            vec![m + vec2(-4.0, 2.0), m + vec2(4.0, 2.0), m + vec2(0.0, -2.5)]
        } else {
            vec![
                m + vec2(-4.0, -2.0),
                m + vec2(4.0, -2.0),
                m + vec2(0.0, 2.5),
            ]
        };
        painter.add(egui::Shape::convex_polygon(points, c, Stroke::NONE));
        if b.clicked() && !usable.is_empty() {
            let here = at.and_then(|i| usable.iter().position(|&u| u == i));
            let next = match (here, up) {
                (Some(p), true) => p.saturating_sub(1),
                (Some(p), false) => (p + 1).min(usable.len() - 1),
                (None, _) => 0,
            };
            chosen = Some(entries[usable[next]].1.clone());
        }
    }
    if let Some(f) = chosen
        && f != file
    {
        let key = egui::Id::new(("cg-lut-pick", k));
        set(app, cx, &pick(f), key);
    }
    let row = Rect::from_min_max(
        pos2(rect.left(), boxr.bottom() + 3.0),
        pos2(boxr.right() + 5.0, boxr.bottom() + 29.0),
    );
    let label_w = labels_end(&body, &[tr("Strength:")], 110.0);
    slider_row(
        &mut body,
        app,
        cx,
        row,
        strength_p,
        tr("Strength:"),
        label_w,
        &[],
    );
}

/// The color a wheel's offsets from where it rests show: their hue, as
/// bright as it goes (none: a light grey).
fn offset_color(o: [f64; 3]) -> Color32 {
    let m = (o[0] + o[1] + o[2]) / 3.0;
    let c = o.map(|v| v - m);
    let s = c.iter().fold(0.0f64, |a, v| a.max(v.abs()));
    if s < 1e-9 {
        return Color32::from_gray(200);
    }
    let [r, g, b] = c.map(|v| ((0.5 + 0.5 * v / s).clamp(0.0, 1.0) * 255.0).round() as u8);
    Color32::from_rgb(r, g, b)
}

/// Wheel `stem`'s color `rgb` and point `dot`, as its parameters hold
/// them.
fn wheel_settings(stem: &str, rgb: [f64; 3], dot: [f64; 2]) -> Vec<(String, Value)> {
    let mut v: Vec<(String, Value)> = (0..3)
        .map(|k| {
            (
                cg::wheel_param(stem, cg::PARTS[k]),
                Value::Num(vec![rgb[k]]),
            )
        })
        .collect();
    v.push((cg::wheel_param(stem, "U"), Value::Num(vec![dot[0]])));
    v.push((cg::wheel_param(stem, "V"), Value::Num(vec![dot[1]])));
    v
}

/// A wheel's column, as VEGAS's: its title, its picker (Lift's black
/// point, Gain's white point) and Reset; the wheel, beside it R, G and B
/// over the Y slider and Y — the boxes left out when the column is small,
/// as VEGAS's are. Or, shown as sliders, R, G, B and Y side by side. The
/// wheel's point gives its color as VEGAS's does
/// (`color_grading::wheel_values`); a color typed puts the point about
/// where it gives it.
fn wheel_column(ui: &mut egui::Ui, app: &mut BananaApp, cx: &Cx, i: usize, rect: Rect) {
    let (stem, name) = WHEELS[i];
    let range = wheel_range(i);
    let (min, max, rest) = range;
    let param = |part: &str| cg::wheel_param(stem, part);
    let (mut bar, mut body) = column(ui, rect, tr(name), true);
    let picker = match i {
        0 => Some((Picker::BlackPoint, tr("Specify a Black Point"))),
        2 => Some((Picker::WhitePoint, tr("Specify a White Point"))),
        _ => None,
    };
    let key = egui::Id::new(("cg-wheel", i));
    if reset_button(&mut bar) {
        // The color and Y at their defaults, the point in the middle.
        let all: Vec<(String, Value)> = cg::PARTS
            .iter()
            .map(|p| (param(p), cx.default_of(&param(p))))
            .collect();
        set_named(app, cx, &all, key.with("reset"));
    }
    if let Some((picker, tip)) = picker {
        picker_button(&mut bar, app, cx, picker, tip);
    }
    let rgb: [f64; 3] = std::array::from_fn(|k| cx.num(&param(cg::PARTS[k])));
    let master = cx.num(&param("Y"));
    let dot = [cx.num(&param("U")), cx.num(&param("V"))];
    // A color typed or slid: the point about where it gives it.
    let typed = |rgb: [f64; 3]| wheel_settings(stem, rgb, cg::wheel_point(rgb, range, i == 1));
    let inner = body.max_rect();
    if app.color_panel.sliders[i] {
        // R, G, B and Y side by side, each its slider over its box.
        let w = inner.width() / 4.0;
        let mut v = [rgb[0], rgb[1], rgb[2], master];
        let mut changed = [false; 4];
        for (k, label) in [tr("Red"), tr("Green"), tr("Blue"), "Y"]
            .into_iter()
            .enumerate()
        {
            let x = inner.left() + w * (k as f32 + 0.5);
            let label_y = inner.bottom() - 10.0;
            let bx = Rect::from_center_size(pos2(x, label_y - 16.0 - BOX.y / 2.0), BOX);
            let slider = Rect::from_min_max(
                pos2(x - 10.0, inner.top() + 8.0),
                pos2(x + 10.0, bx.top() - 8.0),
            );
            if slider.height() > 24.0 {
                let id = key.with(("slider", k));
                changed[k] |=
                    y_slider(&mut body, slider, id, &mut v[k], (min, max), rest).changed();
            }
            changed[k] |= number_box(&mut body, bx, &mut v[k], (min, max), false);
            label_at(&body, pos2(x, label_y), label, Align2::CENTER_CENTER);
        }
        if changed[..3].contains(&true) {
            set_named(app, cx, &typed([v[0], v[1], v[2]]), key);
        }
        if changed[3] {
            set(
                app,
                cx,
                &[(&param("Y"), Value::Num(vec![v[3]]))],
                key.with("y"),
            );
        }
        return;
    }
    // The boxes go when the column is too small for them and a wheel, as
    // VEGAS's do.
    let boxes = rect.width() >= 190.0 && rect.height() >= 150.0;
    let mut m = master;
    let mut y_changed = false;
    let room_right = if boxes {
        // R, G and B down the right, Y at the bottom, its slider between.
        let fx = inner.right() - 4.0 - BOX.x;
        let label_x = fx - 22.0;
        let mut v = rgb;
        let mut typed_now = false;
        for (k, label) in ["R:", "G:", "B:"].into_iter().enumerate() {
            let r = Rect::from_min_size(pos2(fx, inner.top() + 3.0 + k as f32 * 28.0), BOX);
            label_at(
                &body,
                pos2(label_x, r.center().y),
                label,
                Align2::LEFT_CENTER,
            );
            typed_now |= number_box(&mut body, r, &mut v[k], (min, max), false);
        }
        if typed_now {
            set_named(app, cx, &typed(v), key);
        }
        let y_box = Rect::from_min_size(pos2(fx, inner.bottom() - 8.0 - BOX.y), BOX);
        label_at(
            &body,
            pos2(label_x, y_box.center().y),
            "Y:",
            Align2::LEFT_CENTER,
        );
        y_changed |= number_box(&mut body, y_box, &mut m, (min, max), false);
        let slider = Rect::from_min_max(
            pos2(
                fx + BOX.x / 2.0 - 10.0,
                inner.top() + 3.0 + 2.0 * 28.0 + BOX.y + 14.0,
            ),
            pos2(fx + BOX.x / 2.0 + 10.0, y_box.top() - 6.0),
        );
        if slider.height() > 24.0 {
            let id = key.with("y");
            y_changed |= y_slider(&mut body, slider, id, &mut m, (min, max), rest).changed();
        }
        label_x - 18.0
    } else {
        // Only the Y slider, at the right.
        let slider = Rect::from_min_max(
            pos2(inner.right() - 24.0, inner.top() + 8.0),
            pos2(inner.right() - 4.0, inner.bottom() - 8.0),
        );
        if slider.height() > 24.0 {
            let id = key.with("y");
            y_changed |= y_slider(&mut body, slider, id, &mut m, (min, max), rest).changed();
        }
        slider.left() - 6.0
    };
    if y_changed && (m - master).abs() > 1e-12 {
        set(
            app,
            cx,
            &[(&param("Y"), Value::Num(vec![m]))],
            key.with("y"),
        );
    }
    // The wheel, as big as the room left of the numbers lets it be.
    let room = Rect::from_min_max(inner.min, pos2(room_right, inner.max.y));
    let d = (room.width() - 18.0).min(room.height() - 22.0).max(60.0);
    let wheel = Rect::from_center_size(room.center(), vec2(d, d));
    let color_at =
        |p: [f64; 2]| offset_color(cg::wheel_values(p[0], p[1], range).map(|v| v - rest));
    let mut change = None;
    body.put(wheel, |ui: &mut egui::Ui| {
        let (resp, c) = crate::widgets::grading_wheel(ui, d, dot, &color_at);
        change = c;
        resp
    });
    match change {
        Some(crate::widgets::WheelChange::Moved(p)) => {
            let v = cg::wheel_values(p[0], p[1], range);
            set_named(app, cx, &wheel_settings(stem, v, p), key);
        }
        Some(crate::widgets::WheelChange::Reset) => {
            set_named(app, cx, &wheel_settings(stem, [rest; 3], [0.0; 2]), key);
        }
        None => {}
    }
    if let Some((picker, _)) = picker {
        // Over the wheel's corner, kept off the numbers.
        let corner = Rect::from_min_size(inner.min + vec2(8.0, 2.0), vec2(room.width(), 20.0));
        let mut note = body.new_child(
            UiBuilder::new()
                .max_rect(corner)
                .layout(Layout::left_to_right(Align::Center)),
        );
        note.set_clip_rect(corner);
        picking_readout(&mut note, app, picker);
    }
}

/// A vertical slider as VEGAS's wheels' Y: lit from the bottom up to its
/// knob, a mark where it rests; handled as [`slide`] says.
fn y_slider(
    ui: &mut egui::Ui,
    rect: Rect,
    id: egui::Id,
    value: &mut f64,
    (lo, hi): (f64, f64),
    neutral: f64,
) -> egui::Response {
    let mut resp = ui.interact(rect, id, Sense::click_and_drag());
    let track = rect.shrink2(vec2(0.0, 5.0));
    let span = (hi - lo).max(1e-12);
    let y_of = |v: f64| track.bottom() - ((v - lo) / span).clamp(0.0, 1.0) as f32 * track.height();
    let x = rect.center().x;
    let knob = |v: f64| Rect::from_center_size(pos2(x, y_of(v)), vec2(14.0, 9.0));
    let grabbed = knob(*value);
    if slide(
        ui,
        &resp,
        value,
        (lo, hi),
        neutral,
        |p| f64::from((track.bottom() - p.y) / track.height()),
        |p| grabbed.expand(2.0).contains(p),
    ) {
        resp.mark_changed();
    }
    let at = y_of(*value);
    let painter = ui.painter();
    painter.line_segment(
        [pos2(x, track.top()), pos2(x, at)],
        Stroke::new(2.0, theme::panel_dark()),
    );
    painter.line_segment(
        [pos2(x, at), pos2(x, track.bottom())],
        Stroke::new(2.0, theme::knob()),
    );
    let n = y_of(neutral);
    painter.line_segment(
        [pos2(x - 7.0, n), pos2(x + 7.0, n)],
        Stroke::new(1.0, theme::text_dim()),
    );
    let knob = knob(*value);
    painter.rect_filled(
        knob,
        2.0,
        if resp.hovered() || resp.dragged() {
            theme::text_strong()
        } else {
            theme::knob()
        },
    );
    painter.hline(
        knob.x_range().shrink(3.0),
        knob.center().y,
        Stroke::new(1.0, theme::panel_dark()),
    );
    resp
}

/// The RL Color Wheels' Threshold column, as VEGAS's: the upper
/// threshold's box, both on one slider, the lower one's box, and Show
/// Limits on Scopes.
fn threshold_column(ui: &mut egui::Ui, app: &mut BananaApp, cx: &Cx, rect: Rect) {
    let (_, mut body) = column(ui, rect, tr("Threshold"), false);
    let inner = body.max_rect();
    let small = vec2(40.0, BOX.y);
    let hi_box = Rect::from_min_size(pos2(rect.left() + 12.0, inner.top() + 3.0), small);
    let lo_box = Rect::from_min_size(pos2(rect.left() + 12.0, inner.bottom() - 57.0), small);
    let names = [cg::LOW_THRESHOLD, cg::HIGH_THRESHOLD];
    let before = names.map(|n| cx.num(n));
    let mut pair = before;
    number_box(&mut body, hi_box, &mut pair[1], (0.0, 1.0), true);
    number_box(&mut body, lo_box, &mut pair[0], (0.0, 1.0), true);
    let slider = Rect::from_min_max(
        pos2(rect.left() + 12.0, hi_box.bottom() + 8.0),
        pos2(rect.left() + 52.0, lo_box.top() - 8.0),
    );
    if slider.height() > 30.0 {
        let defaults = names.map(|n| cx.default_of(n).num(0));
        thresholds(&mut body, slider, &mut pair, defaults);
    }
    if pair != before {
        set(
            app,
            cx,
            &[
                (names[0], Value::Num(vec![pair[0]])),
                (names[1], Value::Num(vec![pair[1]])),
            ],
            egui::Id::new("cg-thresholds"),
        );
    }
    let icon = Rect::from_center_size(
        pos2(rect.left() + 32.0, inner.bottom() - 19.0),
        vec2(24.0, 24.0),
    );
    let on = app.color_panel.show_limits;
    if body
        .put(icon, |ui: &mut egui::Ui| {
            crate::widgets::icon_toggle(ui, Icon::Limits, on, tr("Show Limits on Scopes"))
        })
        .clicked()
    {
        app.color_panel.show_limits = !on;
    }
}

/// The two thresholds (`pair`: the lower, the upper) on one vertical
/// slider, as VEGAS's: the upper one's knob on the left of the line, the
/// lower one's on its right; each handled as [`slide`] says (the knob
/// under the pointer, or the nearer, takes it), a double-click putting
/// both at `defaults`.
fn thresholds(ui: &mut egui::Ui, rect: Rect, pair: &mut [f64; 2], defaults: [f64; 2]) -> bool {
    let id = egui::Id::new("cg-threshold-slider");
    let resp = ui.interact(rect, id, Sense::click_and_drag());
    let track = rect.shrink2(vec2(0.0, 6.0));
    let y_of = |v: f64| track.bottom() - v.clamp(0.0, 1.0) as f32 * track.height();
    let x = rect.center().x;
    let knob = |k: usize, v: f64| {
        let dx = if k == 1 { -7.0 } else { 7.0 };
        Rect::from_center_size(pos2(x + dx, y_of(v)), vec2(14.0, 9.0))
    };
    let before = *pair;
    let end = id.with("end");
    let k = if resp.dragged() && !resp.drag_started() {
        ui.data(|d| d.get_temp::<usize>(end)).unwrap_or(0)
    } else {
        let at = ui.input(|i| i.pointer.press_origin().or(i.pointer.hover_pos()));
        let k = at.map_or(0, |p| {
            which_knob(p, [knob(0, pair[0]), knob(1, pair[1])], |r| {
                (p.y - r.center().y).abs()
            })
        });
        ui.data_mut(|d| d.insert_temp(end, k));
        k
    };
    let grabbed = knob(k, pair[k]);
    slide(
        ui,
        &resp,
        &mut pair[k],
        (0.0, 1.0),
        defaults[k],
        |p| f64::from((track.bottom() - p.y) / track.height()),
        |p| grabbed.expand(2.0).contains(p),
    );
    if resp.double_clicked() {
        *pair = defaults;
    }
    let painter = ui.painter();
    painter.line_segment(
        [pos2(x, track.top()), pos2(x, track.bottom())],
        Stroke::new(1.5, theme::text_dim()),
    );
    painter.line_segment(
        [pos2(x, y_of(pair[1])), pos2(x, y_of(pair[0]))],
        Stroke::new(2.0, theme::knob()),
    );
    for (k, v) in pair.iter().enumerate() {
        let knob = knob(k, *v);
        painter.rect_filled(knob, 2.0, theme::knob());
        painter.hline(
            knob.x_range().shrink(3.0),
            knob.center().y,
            Stroke::new(1.0, theme::panel_dark()),
        );
    }
    resp.on_hover_text(tr(
        "The lower and upper thresholds of the shadows, midtones and highlights",
    ));
    *pair != before
}

/// A Utilities block's height: its title and its rows.
fn utility_height(k: usize) -> f32 {
    TITLE_ROW
        + 8.0
        + match k {
            0 => 72.0,
            1 => 140.0,
            2 => 56.0 + 22.0,
            _ => 84.0,
        }
}

/// The Utilities, as VEGAS's: Levels and Tone in one column, White
/// Balance and HSL in the other — or the one picked alone —, scrolled when
/// the panel is too low for them.
fn utilities(ui: &mut egui::Ui, app: &mut BananaApp, cx: &Cx, rect: Rect, item: Option<usize>) {
    let columns: Vec<(Rect, Vec<usize>)> = match item {
        None => split(rect, 2)
            .into_iter()
            .zip([vec![0, 1], vec![2, 3]])
            .collect(),
        Some(k) => vec![(half(rect), vec![k.min(3)])],
    };
    let total = columns
        .iter()
        .map(|(_, ks)| ks.iter().map(|&k| utility_height(k)).sum::<f32>())
        .fold(0.0, f32::max);
    let mut area = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
    );
    egui::ScrollArea::vertical()
        .id_salt("cg-utilities")
        .auto_shrink([false, false])
        .show(&mut area, |ui| {
            let (all, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), total), Sense::hover());
            let right = all.right();
            for (r, ks) in &columns {
                let mut y = all.top();
                for &k in ks {
                    let h = utility_height(k);
                    let block =
                        Rect::from_min_max(pos2(r.left(), y), pos2(r.right().min(right), y + h));
                    utility(ui, app, cx, k, block);
                    y += h;
                }
            }
        });
}

/// A Utilities block (`k`: Levels, Tone, White Balance, HSL) in `rect`:
/// its title with Reset — and White Balance's Select Neutral Tone —, its
/// rows.
fn utility(ui: &mut egui::Ui, app: &mut BananaApp, cx: &Cx, k: usize, rect: Rect) {
    let (title, names) = UTILITIES[k];
    let (mut bar, mut body) = column(ui, rect, tr(title), false);
    if reset_button(&mut bar) {
        let values: Vec<(&str, Value)> = names.iter().map(|n| (*n, cx.default_of(n))).collect();
        set(app, cx, &values, egui::Id::new(("cg-utility-reset", k)));
    }
    if k == 2 {
        picker_button(
            &mut bar,
            app,
            cx,
            Picker::NeutralTone,
            tr("Select Neutral Tone"),
        );
    }
    let top = rect.top() + TITLE_ROW;
    let row = |i: usize, h: f32| {
        Rect::from_min_size(pos2(rect.left(), top + i as f32 * h), vec2(rect.width(), h))
    };
    let rgb = |r: u8, g: u8, b: u8| Color32::from_rgb(r, g, b);
    let white = rgb(235, 235, 235);
    match k {
        0 => {
            let (input, output) = (tr("Input:"), tr("Output:"));
            let w = labels_end(&body, &[input, output], 99.0);
            range_row(
                &mut body,
                app,
                cx,
                row(0, 36.0),
                input,
                w,
                ("inMinAdjust", "inMaxAdjust"),
            );
            range_row(
                &mut body,
                app,
                cx,
                row(1, 36.0),
                output,
                w,
                ("outMinAdjust", "outMaxAdjust"),
            );
        }
        1 => {
            let rows = [
                ("brightnessAdjust", tr("Brightness:")),
                ("contrastAdjust", tr("Contrast:")),
                ("contrastCtrAdjust", tr("Contrast Center:")),
                ("saturationAdjust", tr("Saturation:")),
                ("exposureLnAdjust", tr("Log. Exposure:")),
            ];
            let labels: Vec<&str> = rows.iter().map(|r| r.1).collect();
            let w = labels_end(&body, &labels, 128.0);
            for (i, (name, label)) in rows.into_iter().enumerate() {
                slider_row(&mut body, app, cx, row(i, 28.0), name, label, w, &[]);
            }
        }
        2 => {
            let temperature = [rgb(220, 40, 40), white, rgb(40, 60, 220)];
            let tint = [rgb(220, 40, 220), white, rgb(40, 200, 60)];
            let (t, n) = (tr("Temperature:"), tr("Tint:"));
            let w = labels_end(&body, &[t, n], 112.0);
            slider_row(
                &mut body,
                app,
                cx,
                row(0, 28.0),
                "temperatureAdjust",
                t,
                w,
                &temperature,
            );
            slider_row(&mut body, app, cx, row(1, 28.0), "tintAdjust", n, w, &tint);
            let mut note = body.new_child(
                UiBuilder::new()
                    .max_rect(row(2, 28.0).shrink2(vec2(20.0, 2.0)))
                    .layout(Layout::left_to_right(Align::Center)),
            );
            picking_readout(&mut note, app, Picker::NeutralTone);
        }
        _ => {
            let hues = [
                rgb(230, 40, 40),
                rgb(230, 230, 40),
                rgb(40, 230, 40),
                rgb(40, 230, 230),
                rgb(40, 40, 230),
                rgb(230, 40, 230),
                rgb(230, 40, 40),
            ];
            let (h, sat, lum) = (tr("Hue:"), tr("Sat:"), tr("Luminance:"));
            let w = labels_end(&body, &[h, sat, lum], 112.0);
            slider_row(&mut body, app, cx, row(0, 28.0), "hueAdjust", h, w, &hues);
            let grey_red = [Color32::from_gray(128), rgb(220, 40, 40)];
            slider_row(
                &mut body,
                app,
                cx,
                row(1, 28.0),
                "satAdjust",
                sat,
                w,
                &grey_red,
            );
            let black_white = [Color32::BLACK, Color32::WHITE];
            slider_row(
                &mut body,
                app,
                cx,
                row(2, 28.0),
                "valAdjust",
                lum,
                w,
                &black_white,
            );
        }
    }
}

/// A Curves column: the Color Curves (`k` 0) or the HSL Curves (1).
fn curves_column(ui: &mut egui::Ui, app: &mut BananaApp, cx: &Cx, k: usize, rect: Rect) {
    if k == 0 {
        color_curves(ui, app, cx, rect);
    } else {
        hsl_curves(ui, app, cx, rect);
    }
}

/// The Color Curves column: Reset and Select Curves on its title row;
/// Brightness, Red, Green and Blue in one graph, or side by side when the
/// column is wide (`grading_curves.rs`). Each curve is kept as its text,
/// as VEGAS's is.
fn color_curves(ui: &mut egui::Ui, app: &mut BananaApp, cx: &Cx, rect: Rect) {
    let (mut bar, mut body) = column(ui, rect, tr("Color Curves"), false);
    let id = egui::Id::new("cg-curves");
    if reset_button(&mut bar) {
        let values: Vec<(&str, Value)> = COLOR_CURVES
            .iter()
            .map(|n| (*n, cx.default_of(n)))
            .collect();
        set(app, cx, &values, id);
    }
    let curves: Vec<Vec<CurvePoint>> = COLOR_CURVES
        .iter()
        .map(|n| cg::curve_points(&cx.text(n), false))
        .collect();
    let labels: Vec<String> = [tr("Brightness"), tr("Red"), tr("Green"), tr("Blue")]
        .into_iter()
        .map(str::to_string)
        .collect();
    let mut graph = body.new_child(
        UiBuilder::new()
            .max_rect(body.max_rect().shrink2(vec2(4.0, 2.0)))
            .layout(Layout::top_down(Align::Min)),
    );
    let mut view = std::mem::take(&mut app.color_panel.curves);
    let out = crate::grading_curves::editor(&mut graph, &mut view, id, &curves, &labels, &mut bar);
    app.color_panel.curves = view;
    if let Some(c) = out {
        let values: Vec<(&str, Value)> = COLOR_CURVES
            .iter()
            .zip(c.iter().zip(&curves))
            .filter(|(_, (new, old))| new != old)
            .map(|(n, (new, _))| (*n, curve_value(cx, n, new, false)))
            .collect();
        if !values.is_empty() {
            set(app, cx, &values, id);
        }
    }
}

/// A curve's value from its points: its text as VEGAS's panel writes it
/// (`hue`: a hue curve's, round the turn), every point given handles; no
/// points, the untouched text.
fn curve_value(cx: &Cx, name: &str, points: &[CurvePoint], hue: bool) -> Value {
    if points.is_empty() {
        return cx.default_of(name);
    }
    let points = cg::flat_handles(points);
    Value::Text(if hue {
        cg::hue_curve_text(&points)
    } else {
        cg::color_curve_text(&points)
    })
}

/// An HSL curve's point as the editor has it — 0 where the curve changes
/// nothing, −1 to 1 — from the effect's (0.5 there, 0 to 1).
fn to_editor(mut p: CurvePoint) -> CurvePoint {
    p.y = 2.0 * p.y - 1.0;
    if let Some(h) = &mut p.handles {
        h.before[1] *= 2.0;
        h.after[1] *= 2.0;
    }
    p
}

/// The effect's point of the editor's.
fn from_editor(mut p: CurvePoint) -> CurvePoint {
    p.y = (p.y + 1.0) / 2.0;
    if let Some(h) = &mut p.handles {
        h.before[1] /= 2.0;
        h.after[1] /= 2.0;
    }
    p
}

/// The HSL Curves: the type picked, its spectrum, its points (a
/// double-click adds one or deletes it, dragging moves it, the middle
/// button deletes it, the arrow keys move the one picked — with Shift its
/// tangents), their tangents' handles (the picked point's, or all), the
/// point's menu (Lock Tangents, Smooth curves, All or Selected tangents,
/// Lock axis, Reset), the hue swatches adding points, the point's numbers,
/// Reset and Delete All.
fn hsl_curves(ui: &mut egui::Ui, app: &mut BananaApp, cx: &Cx, column_rect: Rect) {
    let (mut bar, mut body) = column(ui, column_rect, "", false);
    let mut head = ui.new_child(
        UiBuilder::new()
            .max_rect(Rect::from_min_size(
                column_rect.min + vec2(4.0, 0.0),
                vec2((column_rect.width() - 96.0).max(60.0), TITLE_ROW),
            ))
            .layout(Layout::left_to_right(Align::Center)),
    );
    let ui = &mut body;
    let kind = app.color_panel.hsl.kind.min(HSL_KINDS.len() - 1);
    let name = cg::CURVES[4 + kind];
    let hue_axis = kind < 3;
    let mut curve: Vec<CurvePoint> = cg::curve_points(&cx.text(name), hue_axis)
        .into_iter()
        .map(to_editor)
        .collect();
    let mut changed = false;
    let selecting = app
        .color_panel
        .picking
        .is_some_and(|p| matches!(p.picker, Picker::Selector(_)));
    let mut open_hue_picker = false;
    let view = &mut app.color_panel.hsl;
    // At the right of the title row: More, Reset and the Color Selector.
    let more = crate::widgets::icon_button(&mut bar, Icon::More, tr("More"));
    egui::Popup::menu(&more).show(|ui| {
        if ui.button(tr("Delete All")).clicked() {
            curve.clear();
            view.picked = None;
            changed = true;
            ui.close();
        }
    });
    if reset_button(&mut bar) {
        curve.clear();
        view.picked = None;
        changed = true;
    }
    let toggle_selector = crate::widgets::icon_toggle(
        &mut bar,
        Icon::Crosshair,
        selecting,
        tr("Color Selector: click a color on the Video Preview, or drag a rectangle over some"),
    )
    .clicked();
    // At its left: the curve's kind, the hue swatches (a point at each),
    // the picked point's numbers.
    egui::ComboBox::from_id_salt("cg-hsl-kind")
        .width(160.0)
        .selected_text(tr(HSL_KINDS[kind]))
        .show_ui(&mut head, |ui| {
            for (k, name) in HSL_KINDS.iter().enumerate() {
                if ui.selectable_label(k == kind, tr(name)).clicked() {
                    view.kind = k;
                    view.picked = None;
                }
            }
        });
    if hue_axis {
        head.add_space(6.0);
        for (k, name) in [
            tr("Red"),
            tr("Yellow"),
            tr("Green"),
            tr("Cyan"),
            tr("Blue"),
            tr("Magenta"),
        ]
        .into_iter()
        .enumerate()
        {
            let h = k as f32 / 6.0;
            let [r, g, b] =
                crate::widgets::hue_rgb(h * std::f32::consts::TAU).map(|v| (v * 255.0) as u8);
            let (cell, resp) = head.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
            head.painter().rect_filled(
                Rect::from_center_size(cell.center(), vec2(10.0, 10.0)),
                1.0,
                Color32::from_rgb(r, g, b),
            );
            if resp.on_hover_text(name).clicked() {
                // As VEGAS's: the color and its two neighbours, where the
                // curve changes nothing, their handles level.
                let x = f64::from(h);
                for d in [0.0, 1.0 / 6.0, -1.0 / 6.0] {
                    let at = (x + d).rem_euclid(1.0);
                    if !curve.iter().any(|p| distance(p.x, at, true) < 1e-3) {
                        curve.push(CurvePoint::new(at, 0.0));
                    }
                }
                curve.sort_by(|a, b| a.x.total_cmp(&b.x));
                level_new(&mut curve, true);
                view.picked = curve.iter().position(|p| distance(p.x, x, true) < 1e-3);
                changed = true;
            }
        }
        // The grey one: any hue, from the color picker.
        let (cell, resp) = head.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
        head.painter().rect_filled(
            Rect::from_center_size(cell.center(), vec2(10.0, 10.0)),
            1.0,
            Color32::from_gray(128),
        );
        open_hue_picker = resp.on_hover_text(tr("Any hue: pick its color")).clicked();
    }
    if let Some(i) = view.picked.filter(|&i| i < curve.len()) {
        head.add_space(6.0);
        let mut x = curve[i].x;
        let mut y = (curve[i].y + 1.0) / 2.0;
        changed |= head
            .add_sized(BOX, field(&mut x).speed(0.002).range(0.0..=1.0))
            .changed();
        changed |= head
            .add_sized(BOX, field(&mut y).speed(0.002).range(0.0..=1.0))
            .changed();
        curve[i].x = x;
        curve[i].y = 2.0 * y - 1.0;
    }
    let width = (ui.available_width() - 8.0).max(160.0);
    let height = (ui.available_height() - if hue_axis { 20.0 } else { 6.0 }).max(80.0);
    ui.add_space(2.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    // A curve's x where it shows (0–1 across), and back: the hues turned
    // round by the spectrum's shift.
    let shift = if hue_axis { view.shift } else { 0.0 };
    let shown = move |x: f64| {
        if shift == 0.0 {
            x
        } else {
            (x - shift).rem_euclid(1.0)
        }
    };
    let stored = move |d: f64| {
        if shift == 0.0 {
            d
        } else {
            (d + shift).rem_euclid(1.0)
        }
    };
    // The background: the hues — darker toward the top and the bottom, as
    // VEGAS's —, or a ramp of saturation or luminance.
    let steps: u32 = 64;
    let mut mesh = egui::Mesh::default();
    let color = |c: [f32; 3]| {
        let [r, g, b] = c.map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8);
        Color32::from_rgb(r, g, b)
    };
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let (mid, edge) = if hue_axis {
            let h = stored(f64::from(t)) as f32;
            let c = crate::widgets::hue_rgb(h * std::f32::consts::TAU);
            (c.map(|v| 0.10 + 0.42 * v), c.map(|v| 0.05 + 0.12 * v))
        } else {
            let c = if kind < 5 {
                [0.35 + 0.25 * t, 0.35 - 0.15 * t, 0.35 - 0.15 * t]
            } else {
                [t * 0.6; 3]
            };
            (c, c.map(|v| v * 0.5))
        };
        let x = rect.left() + rect.width() * t;
        mesh.colored_vertex(pos2(x, rect.top()), color(edge));
        mesh.colored_vertex(pos2(x, rect.center().y), color(mid));
        mesh.colored_vertex(pos2(x, rect.bottom()), color(edge));
    }
    for i in 0..steps {
        let a = 3 * i;
        for (p, q) in [(0, 1), (1, 2)] {
            mesh.add_triangle(a + p, a + 3 + p, a + q);
            mesh.add_triangle(a + q, a + 3 + p, a + 3 + q);
        }
    }
    painter.add(mesh);
    let place = |d: f64, y: f64| {
        pos2(
            rect.left() + d as f32 * rect.width(),
            rect.center().y - y as f32 * rect.height() / 2.0,
        )
    };
    let to_screen = |x: f64, y: f64| place(shown(x), y);
    let from_screen = |p: Pos2| {
        (
            stored(((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64),
            (-(p.y - rect.center().y) / (rect.height() / 2.0)).clamp(-1.0, 1.0) as f64,
        )
    };
    painter.hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(1.0, Color32::from_white_alpha(70)),
    );
    // The curve.
    let line: Vec<Pos2> = (0..=128)
        .map(|i| {
            let d = i as f64 / 128.0;
            place(d, hsl_at(&curve, stored(d), hue_axis))
        })
        .collect();
    painter.add(egui::Shape::line(line, Stroke::new(1.5, Color32::WHITE)));
    // Interaction.
    let near = |c: &[bv_core::curve::CurvePoint], p: Pos2| {
        c.iter()
            .position(|q| (to_screen(q.x, q.y) - p).length() < 7.0)
    };
    // The tangents shown: the picked point's, or every point's.
    let tangents: Vec<usize> = if view.all_tangents {
        (0..curve.len()).collect()
    } else {
        view.picked
            .filter(|&i| i < curve.len())
            .into_iter()
            .collect()
    };
    let handle_end = |c: &[bv_core::curve::CurvePoint], i: usize, after: bool| {
        let h = bv_core::curve::handles(c, i);
        let d = if after { h.after } else { h.before };
        to_screen(c[i].x, c[i].y)
            + vec2(
                d[0] as f32 * rect.width(),
                -(d[1] as f32) * rect.height() / 2.0,
            )
    };
    let near_handle = |c: &[bv_core::curve::CurvePoint], p: Pos2| {
        tangents
            .iter()
            .flat_map(|&i| [(i, false), (i, true)])
            .find(|&(i, after)| i < c.len() && (handle_end(c, i, after) - p).length() < 6.0)
    };
    if resp.drag_started()
        && let Some(p) = resp.interact_pointer_pos()
    {
        view.handle = near_handle(&curve, p);
        view.drag = if view.handle.is_some() {
            None
        } else {
            near(&curve, p)
        };
        view.picked = view.drag.or(view.picked);
    }
    if let (true, Some(i), Some(p)) = (resp.dragged(), view.drag, resp.interact_pointer_pos())
        && i < curve.len()
    {
        let (x, y) = from_screen(p);
        if !view.lock_axis {
            curve[i].x = x;
        }
        curve[i].y = y;
        changed = true;
    }
    // A tangent's handle: the hue's way round kept short.
    if let (true, Some((i, after)), Some(p)) =
        (resp.dragged(), view.handle, resp.interact_pointer_pos())
        && i < curve.len()
    {
        let (x, y) = from_screen(p);
        let mut d = x - curve[i].x;
        if hue_axis {
            d = (d + 0.5).rem_euclid(1.0) - 0.5;
        }
        let to = [curve[i].x + d, y];
        crate::ofx_curves::move_handle(&mut curve, i, after, to);
        changed = true;
    }
    if resp.drag_stopped() {
        view.drag = None;
        view.handle = None;
        let picked = view.picked.and_then(|i| curve.get(i).map(|q| (q.x, q.y)));
        curve.sort_by(|a, b| a.x.total_cmp(&b.x));
        view.picked = picked.and_then(|(x, y)| curve.iter().position(|q| q.x == x && q.y == y));
    }
    // The arrow keys move the point picked (a degree of hue, or a
    // hundredth), with Shift its tangents (up and down tilt them, left and
    // right lengthen or shorten them); with Ctrl a fifth as far.
    if resp.clicked() || resp.drag_started() {
        resp.request_focus();
    }
    if resp.has_focus() {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                resp.id,
                egui::EventFilter {
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    ..Default::default()
                },
            );
        });
        let (dx, dy, m) = ui.input(|inp| {
            let k = |key| f64::from(u8::from(inp.key_pressed(key)));
            (
                k(egui::Key::ArrowRight) - k(egui::Key::ArrowLeft),
                k(egui::Key::ArrowUp) - k(egui::Key::ArrowDown),
                inp.modifiers,
            )
        });
        if let Some(i) = view.picked.filter(|&i| i < curve.len())
            && (dx != 0.0 || dy != 0.0)
        {
            let fine = if m.command { 0.2 } else { 1.0 };
            if m.shift {
                let h = bv_core::curve::handles(&curve, i);
                let a = h.after;
                let grow = 1.0 + 0.1 * fine * dx;
                let to = [
                    curve[i].x + a[0] * grow,
                    curve[i].y + a[1] * grow + 0.01 * fine * dy,
                ];
                crate::ofx_curves::move_handle(&mut curve, i, true, to);
            } else {
                let step = if hue_axis { 1.0 / 360.0 } else { 0.01 };
                if !view.lock_axis {
                    let x = curve[i].x + dx * step * fine;
                    curve[i].x = if hue_axis {
                        x.rem_euclid(1.0)
                    } else {
                        x.clamp(0.0, 1.0)
                    };
                }
                curve[i].y = (curve[i].y + dy * 0.01 * fine).clamp(-1.0, 1.0);
                let (x, y) = (curve[i].x, curve[i].y);
                curve.sort_by(|a, b| a.x.total_cmp(&b.x));
                view.picked = curve.iter().position(|q| q.x == x && q.y == y);
            }
            changed = true;
        }
    }
    if resp.clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        view.picked = near(&curve, p);
    }
    if resp.double_clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        match near(&curve, p) {
            Some(i) => {
                crate::grading_curves::flatten(&mut curve, i, hue_axis);
                view.picked = Some(i);
            }
            None => {
                let (x, y) = from_screen(p);
                let (added, i) = crate::grading_curves::add_dot(&curve, [x, y], hue_axis);
                curve = added;
                view.picked = Some(i);
            }
        }
        changed = true;
    }
    if resp.middle_clicked()
        && let Some(p) = resp.interact_pointer_pos()
        && let Some(i) = near(&curve, p)
    {
        curve.remove(i);
        view.picked = None;
        changed = true;
    }
    for &i in tangents.iter().filter(|&&i| i < curve.len()) {
        let at = to_screen(curve[i].x, curve[i].y);
        for after in [false, true] {
            let end = handle_end(&curve, i, after);
            painter.line_segment([at, end], Stroke::new(1.0, Color32::from_gray(170)));
            painter.rect_filled(
                Rect::from_center_size(end, vec2(6.0, 6.0)),
                1.0,
                Color32::from_gray(200),
            );
        }
    }
    for (i, q) in curve.iter().enumerate() {
        let at = to_screen(q.x, q.y);
        let fill = if view.picked == Some(i) {
            theme::banana()
        } else {
            Color32::WHITE
        };
        painter.vline(
            at.x,
            rect.y_range(),
            Stroke::new(1.0, Color32::from_white_alpha(40)),
        );
        painter.circle_filled(at, 4.5, fill);
        painter.circle_stroke(at, 4.5, Stroke::new(1.0, Color32::BLACK));
    }
    // The spectrum under it: dragged sideways, it turns the hues round (red
    // to the middle); a double-click puts them back.
    if hue_axis {
        let (strip, turn) = ui.allocate_exact_size(vec2(width, 10.0), Sense::click_and_drag());
        let painter = ui.painter_at(strip);
        for i in 0..steps {
            let x0 = strip.left() + strip.width() * i as f32 / steps as f32;
            let x1 = strip.left() + strip.width() * (i + 1) as f32 / steps as f32;
            let h = stored((i as f64 + 0.5) / steps as f64) as f32;
            let [r, g, b] =
                crate::widgets::hue_rgb(h * std::f32::consts::TAU).map(|v| (v * 255.0) as u8);
            painter.rect_filled(
                Rect::from_min_max(pos2(x0, strip.top()), pos2(x1, strip.bottom())),
                0.0,
                Color32::from_rgb(r, g, b),
            );
        }
        if turn.dragged() {
            view.shift = (view.shift - (turn.drag_delta().x / width) as f64).rem_euclid(1.0);
        }
        if turn.double_clicked() {
            view.shift = 0.0;
        }
        if turn.hovered() || turn.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        turn.on_hover_text(tr(
            "Drag sideways to turn the hues round · double-click: back",
        ));
    }
    // A right-click picks the point under it, for its menu.
    if resp.secondary_clicked()
        && let Some(p) = resp.interact_pointer_pos()
        && let Some(i) = near(&curve, p)
    {
        view.picked = Some(i);
    }
    let resp = resp.on_hover_text(tr(
        "Double-click: add or delete a point · drag: move it · middle click: delete · arrows: move the point picked (Shift: its tangents)",
    ));
    resp.context_menu(|ui| {
        if let Some(i) = view.picked.filter(|&i| i < curve.len()) {
            let locked = !bv_core::curve::handles(&curve, i).split;
            if crate::menus::toggle_item(ui, tr("Lock Tangents"), locked) {
                crate::ofx_curves::lock_handles(&mut curve, i, !locked);
                changed = true;
                ui.close();
            }
            // The curve's tangents worked out again: smooth.
            if crate::menus::item(ui, tr("Smooth curves")) {
                crate::grading_curves::smooth(&mut curve, hue_axis);
                changed = true;
                ui.close();
            }
        }
        if crate::menus::check_item(ui, tr("All tangents"), view.all_tangents, true, "") {
            view.all_tangents = true;
            ui.close();
        }
        if crate::menus::check_item(ui, tr("Selected tangents"), !view.all_tangents, true, "") {
            view.all_tangents = false;
            ui.close();
        }
        if crate::menus::toggle_item(ui, tr("Lock axis"), view.lock_axis) {
            view.lock_axis = !view.lock_axis;
            ui.close();
        }
        if let Some(i) = view.picked.filter(|&i| i < curve.len()) {
            ui.separator();
            if ui.button(tr("Reset")).clicked() {
                curve[i].y = 0.0;
                crate::grading_curves::flatten(&mut curve, i, hue_axis);
                changed = true;
                ui.close();
            }
            if ui.button(tr("Delete")).clicked() {
                curve.remove(i);
                view.picked = None;
                changed = true;
                ui.close();
            }
        }
        if ui.button(tr("Delete All")).clicked() {
            curve.clear();
            view.picked = None;
            changed = true;
            ui.close();
        }
    });
    // The grey swatch's color picker: a point at the hue chosen.
    if open_hue_picker {
        app.color_panel.hue_picker = Some(crate::color_picker::ColorPicker::new(
            tr("Hue"),
            [128, 128, 128, 255],
        ));
    }
    if let Some(picker) = app.color_panel.hue_picker.as_mut() {
        use crate::color_picker::Outcome;
        match picker.show(ui.ctx()) {
            Outcome::Open => {}
            Outcome::Cancelled => app.color_panel.hue_picker = None,
            Outcome::Chosen(c) => {
                app.color_panel.hue_picker = None;
                if let Some(x) = hue_of(c).filter(|_| hue_axis) {
                    let y = hsl_at(&curve, x, true);
                    curve.push(CurvePoint::new(x, y));
                    curve.sort_by(|a, b| a.x.total_cmp(&b.x));
                    level_new(&mut curve, true);
                    app.color_panel.hsl.picked = curve.iter().position(|p| (p.x - x).abs() < 1e-9);
                    changed = true;
                }
            }
        }
    }
    if changed {
        let points: Vec<CurvePoint> = curve.into_iter().map(from_editor).collect();
        let value = curve_value(cx, name, &points, hue_axis);
        set(app, cx, &[(name, value)], egui::Id::new("cg-hsl"));
    }
    if toggle_selector {
        if selecting {
            stop_picking(app);
        } else {
            start_picking(app, cx, Picker::Selector(kind));
        }
    }
    picking_readout(ui, app, Picker::Selector(kind));
}

// The pickers.

/// A picker's button (a crosshair): lit while it waits for the Video
/// Preview; a click starts or stops it.
fn picker_button(ui: &mut egui::Ui, app: &mut BananaApp, cx: &Cx, picker: Picker, tip: &str) {
    let on = app.color_panel.picking.is_some_and(|p| p.picker == picker);
    if crate::widgets::icon_toggle(ui, Icon::Crosshair, on, tip).clicked() {
        if on {
            stop_picking(app);
        } else {
            start_picking(app, cx, picker);
        }
    }
}

/// Starts a picker. The effect is added first when it is not in the chain:
/// its input is what is picked.
fn start_picking(app: &mut BananaApp, cx: &Cx, picker: Picker) {
    if cx.index.is_none() {
        set(app, cx, &[], egui::Id::new("cg-picker-adds"));
    }
    app.color_panel.picking = Some(Picking { picker, from: None });
    app.color_panel.hover = None;
    app.ofx.watch_held = true;
}

/// Stops the picker waiting.
pub(crate) fn stop_picking(app: &mut BananaApp) {
    if app.color_panel.picking.take().is_some() {
        app.ofx.watch_held = false;
    }
    app.color_panel.hover = None;
}

/// Under a picker's button while it waits: the color under the pointer
/// on the preview (a black or white point's), or what to do.
fn picking_readout(ui: &mut egui::Ui, app: &BananaApp, picker: Picker) {
    let waiting = app.color_panel.picking.is_some_and(|p| {
        p.picker == picker
            || matches!(
                (p.picker, picker),
                (Picker::Selector(_), Picker::Selector(_))
            )
    });
    if !waiting {
        return;
    }
    ui.horizontal(|ui| match app.color_panel.hover {
        Some(c) if matches!(picker, Picker::BlackPoint | Picker::WhitePoint) => {
            let (r, _) = ui.allocate_exact_size(vec2(18.0, 12.0), Sense::hover());
            let [red, green, blue] = c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
            ui.painter()
                .rect_filled(r, 1.0, Color32::from_rgb(red, green, blue));
            ui.painter().rect_stroke(
                r,
                1.0,
                Stroke::new(1.0, theme::separator()),
                egui::StrokeKind::Inside,
            );
            let n = |v: f32| crate::fmt::num(v as f64, 3);
            ui.label(
                RichText::new(format!("R: {}  G: {}  B: {}", n(c[0]), n(c[1]), n(c[2]))).size(11.0),
            );
        }
        _ => {
            ui.label(
                RichText::new(tr("Click the Video Preview, or drag a rectangle over it"))
                    .size(11.0)
                    .color(theme::text_dim()),
            );
        }
    });
}

/// A picker waiting on the Video Preview (`frame`: where the frame is on
/// screen; `nav`: the preview's own response to the mouse): the color
/// under the pointer shown in the panel, a click's few pixels or a
/// rectangle dragged picked from the effect's input there. Escape stops it.
pub(crate) fn pick_on_preview(
    ui: &egui::Ui,
    app: &mut BananaApp,
    nav: &egui::Response,
    frame: Rect,
) {
    let Some(mut picking) = app.color_panel.picking else {
        return;
    };
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        stop_picking(app);
        return;
    }
    let Some(cx) = context(app) else {
        stop_picking(app);
        return;
    };
    let (Some(key), Some(inv), true) = (
        cx.effect_key(),
        cx.place.map.inverse(),
        frame.width() > 1.0 && frame.height() > 1.0,
    ) else {
        return;
    };
    let input = bv_engine::plugins::watched_input(&key);
    let (pressed, released, pos) = ui.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_released(),
            i.pointer.interact_pos().or(i.pointer.hover_pos()),
        )
    });
    let over = pos.is_some_and(|q| frame.contains(q))
        && (nav.hovered() || nav.is_pointer_button_down_on());
    if over {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    // A point on screen as a pixel of the effect's picture (its canonical
    // coordinates count up from the bottom, in full-size pixels).
    let colors = |a: Pos2, b: Pos2| -> Vec<[f32; 3]> {
        let Some((_, picture, full)) = &input else {
            return Vec::new();
        };
        let pixel = |q: Pos2| {
            let [x, y] = inv.apply([
                ((q.x - frame.left()) / frame.width()) as f64,
                ((q.y - frame.top()) / frame.height()) as f64,
            ]);
            [
                x * picture.width as f64 / full.0.max(1.0),
                (full.1 - y) * picture.height as f64 / full.1.max(1.0),
            ]
        };
        picture_colors(picture, pixel(a), pixel(b))
    };
    let mut done = None;
    if let Some(q) = pos {
        if pressed && over {
            picking.from = Some(q);
        }
        if let Some(from) = picking.from {
            if released {
                let region = (q - from).length() > 3.0;
                done = Some((colors(from, q), region));
                picking.from = None;
            } else {
                // The rectangle, as it is dragged.
                let r = Rect::from_two_pos(from, q).intersect(frame);
                let painter = ui.painter().with_clip_rect(frame);
                for (width, color) in [(3.0, Color32::from_black_alpha(200)), (1.2, Color32::WHITE)]
                {
                    painter.rect_stroke(
                        r,
                        0.0,
                        Stroke::new(width, color),
                        egui::StrokeKind::Middle,
                    );
                }
            }
        } else if over {
            app.color_panel.hover = mean(&colors(q, q));
        }
    }
    app.color_panel.picking = Some(picking);
    ui.ctx().request_repaint();
    if let Some((colors, region)) = done {
        apply_pick(app, &cx, picking.picker, &colors, region);
        stop_picking(app);
    }
}

/// The straight colors of `picture` in the rectangle between pixels `a`
/// and `b` (one pixel: the 5 × 5 around it, as an eyedropper's), at most
/// some 40,000 of them spread evenly.
fn picture_colors(picture: &bv_core::ImageRgbaF32, a: [f64; 2], b: [f64; 2]) -> Vec<[f32; 3]> {
    let (w, h) = (picture.width as i64, picture.height as i64);
    let (mut x0, mut y0) = (a[0].min(b[0]).floor() as i64, a[1].min(b[1]).floor() as i64);
    let (mut x1, mut y1) = (a[0].max(b[0]).floor() as i64, a[1].max(b[1]).floor() as i64);
    if x0 == x1 && y0 == y1 {
        (x0, y0, x1, y1) = (x0 - 2, y0 - 2, x1 + 2, y1 + 2);
    }
    let (x0, y0, x1, y1) = (x0.max(0), y0.max(0), x1.min(w - 1), y1.min(h - 1));
    if x0 > x1 || y0 > y1 {
        return Vec::new();
    }
    let area = ((x1 - x0 + 1) * (y1 - y0 + 1)) as f64;
    let step = (area / 40_000.0).sqrt().ceil().max(1.0) as usize;
    let mut out = Vec::new();
    for y in (y0..=y1).step_by(step) {
        for x in (x0..=x1).step_by(step) {
            let [r, g, b, alpha] = picture.data[(y * w + x) as usize];
            if alpha > 1e-4 {
                out.push([r / alpha, g / alpha, b / alpha]);
            }
        }
    }
    out
}

/// The mean of colors (none: `None`).
fn mean(colors: &[[f32; 3]]) -> Option<[f32; 3]> {
    let n = colors.len() as f32;
    (n > 0.0).then(|| std::array::from_fn(|i| colors.iter().map(|c| c[i]).sum::<f32>() / n))
}

/// Sets what a picker took from `colors` (`region`: a rectangle's), each
/// pick its own undo; the Color Selector's new point in the middle is
/// picked on the curve shown.
fn apply_pick(app: &mut BananaApp, cx: &Cx, picker: Picker, colors: &[[f32; 3]], region: bool) {
    if colors.is_empty() {
        app.notify(tr(
            "The Video Preview has not shown this effect's picture yet.",
        ));
        return;
    }
    let picker = match picker {
        Picker::Selector(_) => Picker::Selector(app.color_panel.hsl.kind),
        p => p,
    };
    let new = bv_engine::Engine::color_grading_pick(&cx.values(), picker, colors, region);
    if new.is_empty() {
        app.notify(tr("No color to pick there."));
        return;
    }
    if let Picker::Selector(n) = picker {
        let name = cg::CURVES[4 + n.min(5)];
        let before: Vec<f64> = cg::curve_points(&cx.text(name), n < 3)
            .iter()
            .map(|p| p.x)
            .collect();
        if let Some((_, Value::Text(t))) = new.iter().find(|(k, _)| k == name) {
            let curve = cg::curve_points(t, n < 3);
            let added: Vec<usize> = (0..curve.len())
                .filter(|&i| !before.iter().any(|x| (x - curve[i].x).abs() < 1e-9))
                .collect();
            let xs: Vec<f64> = added.iter().map(|&i| curve[i].x).collect();
            let middle = middle_of(&xs, n < 3);
            app.color_panel.hsl.picked = added.iter().copied().min_by(|&a, &b| {
                distance(curve[a].x, middle, n < 3).total_cmp(&distance(curve[b].x, middle, n < 3))
            });
        }
    }
    app.color_panel.picks += 1;
    let key = egui::Id::new(("cg-pick", app.color_panel.picks));
    let values: Vec<(&str, Value)> = new
        .iter()
        .map(|(name, v)| (name.as_str(), v.clone()))
        .collect();
    set(app, cx, &values, key);
}

/// The middle of points on an axis (a hue's turn, `wrap`: round red).
fn middle_of(xs: &[f64], wrap: bool) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    if !wrap {
        return xs.iter().sum::<f64>() / xs.len() as f64;
    }
    use std::f64::consts::TAU;
    let (x, y) = xs.iter().fold((0.0, 0.0), |(x, y), h| {
        let (sin, cos) = (h * TAU).sin_cos();
        (x + cos, y + sin)
    });
    (y.atan2(x) / TAU).rem_euclid(1.0)
}

/// How far apart two points of an axis are (`wrap`: round red).
fn distance(a: f64, b: f64, wrap: bool) -> f64 {
    let d = (a - b).abs();
    if wrap { d.min(1.0 - d) } else { d }
}

/// Every frame's end: a picker waits only while the panel shows.
pub(crate) fn end_frame(app: &mut BananaApp) {
    if !std::mem::take(&mut app.color_panel.drawn) {
        stop_picking(app);
    }
}

/// The RL thresholds the Video Scopes draw (Show Limits on Scopes).
pub(crate) fn scope_limits(app: &BananaApp) -> Option<[f64; 2]> {
    if !app.color_panel.show_limits {
        return None;
    }
    let cx = context(app)?;
    Some([cx.num(cg::LOW_THRESHOLD), cx.num(cg::HIGH_THRESHOLD)])
}

/// Opens the panel at `level` (the event's, the track's, the media's or
/// the video output's More menu).
pub(crate) fn open_at(app: &mut BananaApp, level: Level) {
    stop_picking(app);
    app.color_panel.level = level;
    app.show_tab(crate::dock::Tab::ColorGrading, false);
}

/// A picker waiting with a color under the pointer (screen tests): 0 the
/// black point, 1 the white point, 2 Select Neutral Tone, 3 the Color
/// Selector (the hues turned to put red in the middle).
pub(crate) fn picker_for_review(app: &mut BananaApp, which: usize) {
    let Some(cx) = context(app) else {
        return;
    };
    let picker = match which {
        0 => Picker::BlackPoint,
        1 => Picker::WhitePoint,
        2 => Picker::NeutralTone,
        _ => {
            app.color_panel.hsl.shift = 0.5;
            Picker::Selector(app.color_panel.hsl.kind)
        }
    };
    stop_picking(app);
    start_picking(app, &cx, picker);
    app.color_panel.hover = Some([0.082, 0.071, 0.094]);
}

impl BananaApp {
    /// The HSL Curves' tangents shown on every point, the first picked
    /// (screen tests).
    pub fn hsl_tangents_for_review(&mut self) {
        self.color_panel.hsl.all_tangents = true;
        self.color_panel.hsl.picked = Some(0);
    }
}

/// A little grade on what the panel shows now (screen tests).
pub(crate) fn grade_for_review(app: &mut BananaApp) {
    let Some(cx) = context(app) else {
        return;
    };
    let mut values: Vec<(String, Value)> = Vec::new();
    for (i, rgb) in [
        (0, [0.06, 0.02, -0.03]),
        (2, [1.08, 1.0, 0.94]),
        (4, [-0.02, 0.0, 0.05]),
    ] {
        let (stem, _) = WHEELS[i];
        let dot = cg::wheel_point(rgb, wheel_range(i), false);
        values.extend(wheel_settings(stem, rgb, dot));
    }
    let red = cg::color_curve_text(&cg::flat_handles(&[
        CurvePoint::new(0.0, 0.0),
        CurvePoint::new(0.5, 0.58),
        CurvePoint::new(1.0, 1.0),
    ]));
    let hue_sat = cg::hue_curve_text(&cg::flat_handles(&[
        CurvePoint::new(0.0, 0.3),
        CurvePoint::new(0.33, 0.65),
        CurvePoint::new(0.66, 0.5),
    ]));
    values.extend([
        ("gammaY".to_string(), Value::Num(vec![1.1])),
        ("contrastAdjust".to_string(), Value::Num(vec![0.15])),
        ("curveR".to_string(), Value::Text(red)),
        ("curveHvS".to_string(), Value::Text(hue_sat)),
    ]);
    app.color_panel.hsl.kind = 1;
    set_named(app, &cx, &values, egui::Id::new("cg-review"));
}

/// A color's hue (a turn's 0–1); greys have none.
fn hue_of([r, g, b, _]: [u8; 4]) -> Option<f64> {
    let [r, g, b] = [r, g, b].map(|v| v as f64 / 255.0);
    let max = r.max(g).max(b);
    let d = max - r.min(g).min(b);
    if d < 1e-6 {
        return None;
    }
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    Some(h / 6.0)
}

/// The points of `c` without handles given level ones, as VEGAS's new
/// points have (`wrap`: a hue curve's).
fn level_new(c: &mut [CurvePoint], wrap: bool) {
    for i in 0..c.len() {
        if c[i].handles.is_none() {
            crate::grading_curves::flatten(c, i, wrap);
        }
    }
}

/// An HSL curve at `x`: a hue curve wraps round red (its points a turn
/// before and after too), as the effect reads it.
fn hsl_at(curve: &[bv_core::curve::CurvePoint], x: f64, wrap: bool) -> f64 {
    if !wrap || curve.is_empty() {
        return bv_core::curve::curve_at(curve, x);
    }
    let mut all: Vec<_> = [-1.0, 0.0, 1.0]
        .into_iter()
        .flat_map(|shift| {
            curve.iter().map(move |p| bv_core::curve::CurvePoint {
                x: p.x + shift,
                ..*p
            })
        })
        .collect();
    all.sort_by(|a, b| a.x.total_cmp(&b.x));
    all.dedup_by(|a, b| (a.x - b.x).abs() < 1e-9);
    bv_core::curve::curve_at(&all, x)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The panel's first change adds the Color Grading effect to the event
    /// picked; Bypass and Reset All act on it; Export LUT's text holds
    /// the grade.
    #[test]
    fn the_first_change_adds_the_effect() {
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        let video = app
            .project()
            .clips()
            .find(|(t, _)| t.kind == TrackKind::Video)
            .map(|(_, c)| c.id)
            .unwrap();
        app.selection = [video].into_iter().collect();
        let Some(cx) = context(&app) else {
            // The plug-in is not registered in this test run.
            return;
        };
        assert!(cx.index.is_none());
        assert_eq!(cx.num("gainR"), 1.0);
        set(
            &mut app,
            &cx,
            &[("gainY", Value::Num(vec![0.5]))],
            egui::Id::new("t"),
        );
        let cx = context(&app).unwrap();
        assert!(cx.index.is_some());
        assert_eq!(cx.num("gainY"), 0.5);
        let text = bv_engine::Engine::color_grading_cube(&cx.values(), 2);
        assert!(text.contains("LUT_3D_SIZE 2"));
        change_effect(&mut app, &cx, |e| e.enabled = false);
        assert!(!context(&app).unwrap().on());
        let cx = context(&app).unwrap();
        change_effect(&mut app, &cx, |e| {
            if let Some(fx) = e.plugin.as_mut() {
                fx.params.clear();
            }
        });
        assert_eq!(context(&app).unwrap().num("gainY"), 1.0);
    }

    /// A click reads the 5 × 5 pixels around it, a rectangle what is in
    /// it (straight colors; see-through pixels left out), within the
    /// picture.
    #[test]
    fn pickers_read_a_click_or_a_rectangle() {
        let mut picture = bv_core::ImageRgbaF32::filled(20, 10, [0.2, 0.4, 0.6, 1.0]);
        picture.data[5 * 20 + 5] = [0.25, 0.25, 0.25, 0.5];
        picture.data[0] = [0.0; 4];
        let click = picture_colors(&picture, [5.5, 5.5], [5.5, 5.5]);
        assert_eq!(click.len(), 25);
        assert!(click.contains(&[0.5, 0.5, 0.5]));
        let corner = picture_colors(&picture, [0.2, 0.2], [0.2, 0.2]);
        assert_eq!(
            corner.len(),
            8,
            "the corner's 3 × 3 within, less the see-through one"
        );
        let all = picture_colors(&picture, [-4.0, -4.0], [40.0, 40.0]);
        assert_eq!(all.len(), 199);
        assert!(picture_colors(&picture, [30.0, 3.0], [35.0, 4.0]).is_empty());
        // A hue's middle goes round red.
        assert!(middle_of(&[0.95, 0.05], true) < 0.01 || middle_of(&[0.95, 0.05], true) > 0.99);
        assert!((middle_of(&[0.2, 0.4], false) - 0.3).abs() < 1e-9);
        assert!((distance(0.98, 0.02, true) - 0.04).abs() < 1e-9);
        // The grey swatch's colors: a hue, greys none.
        assert!((hue_of([0, 0, 255, 255]).unwrap() - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(hue_of([255, 0, 0, 255]), Some(0.0));
        assert_eq!(hue_of([90, 90, 90, 255]), None);
    }

    /// The black point picker on the event: the effect added when it
    /// starts, Lift set from the colors picked, the picker stopped by
    /// Escape or when the panel goes; the Video Output level grades the
    /// output's chain.
    #[test]
    fn a_black_point_sets_lift() {
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        let video = app
            .project()
            .clips()
            .find(|(t, _)| t.kind == TrackKind::Video)
            .map(|(_, c)| c.id)
            .unwrap();
        app.selection = [video].into_iter().collect();
        let Some(cx) = context(&app) else {
            // The plug-in is not registered in this test run.
            return;
        };
        start_picking(&mut app, &cx, Picker::BlackPoint);
        let cx = context(&app).unwrap();
        assert!(cx.index.is_some() && cx.effect_key().is_some());
        assert!(app.ofx.watch_held);
        apply_pick(&mut app, &cx, Picker::BlackPoint, &[[0.1, 0.1, 0.1]], false);
        // VEGAS's: each channel between Lift's ends, the color turned over.
        let cx = context(&app).unwrap();
        let lift = ["liftR", "liftG", "liftB"].map(|n| cx.num(n));
        assert!(lift.iter().all(|l| (l - 0.8).abs() < 1e-9), "{lift:?}");
        assert!(cx.num("liftU") != 0.0, "the point follows the color");
        end_frame(&mut app);
        assert!(app.color_panel.picking.is_none() && !app.ofx.watch_held);
        app.color_panel.level = Level::VideoOutput;
        let cx = context(&app).unwrap();
        assert_eq!(cx.target, FxTarget::VideoOutput);
        set(
            &mut app,
            &cx,
            &[("gainY", Value::Num(vec![0.8]))],
            egui::Id::new("t"),
        );
        assert!(
            app.project()
                .video_output_fx
                .iter()
                .any(|e| e.plugin.as_ref().is_some_and(|f| f.id == ID))
        );
    }
}
