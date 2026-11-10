//! Media Generator: VEGAS's plug-in window for generated media — solid
//! colors and tones, BananaV's own OFX generators (Checkerboard, Color
//! Gradient, Noise Texture, Test Pattern, Credit Roll, Titles & Text; the
//! older built-in checkerboards, gradients, noise and bars stay for the
//! projects that have them) and other makers' (Third Party) — with presets
//! ready to drag to the timeline or double-click (placed at the cursor).
//! Under the pointer a preset plays (noise changes every frame, a credit
//! roll scrolls).

use crate::i18n::tr;
use bv_core::Tick;
use bv_timeline::{Generator, MediaInfo, PluginFx};

use super::chooser::{self, Category, Plugin};
use crate::app::{BananaApp, MediaDrag};
use crate::egui::{self, Color32, Rect, pos2};
use crate::icons::{self, Icon};
use crate::theme;

/// A generator of the list, as VEGAS names them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Checkerboard,
    ColorGradient,
    CreditRoll,
    NoiseTexture,
    SolidColor,
    TestPattern,
    TestTone,
    TitlesAndText,
}

const KINDS: [Kind; 8] = [
    Kind::Checkerboard,
    Kind::ColorGradient,
    Kind::CreditRoll,
    Kind::NoiseTexture,
    Kind::SolidColor,
    Kind::TestPattern,
    Kind::TestTone,
    Kind::TitlesAndText,
];

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Checkerboard => tr("Checkerboard"),
            Kind::ColorGradient => tr("Color Gradient"),
            Kind::CreditRoll => tr("Credit Roll"),
            Kind::NoiseTexture => tr("Noise Texture"),
            Kind::SolidColor => tr("Solid Color"),
            Kind::TestPattern => tr("Test Pattern"),
            Kind::TestTone => tr("Test Tone"),
            Kind::TitlesAndText => tr("Titles & Text"),
        }
    }

    /// The kind a generator is of (`None`: another maker's plug-in's).
    fn of(g: &Generator) -> Option<Kind> {
        Some(match g {
            Generator::SolidColor { .. } => Kind::SolidColor,
            Generator::ColorBars => Kind::TestPattern,
            Generator::Checkerboard { .. } => Kind::Checkerboard,
            Generator::Gradient { .. } => Kind::ColorGradient,
            Generator::Noise { .. } => Kind::NoiseTexture,
            Generator::TestTone { .. } => Kind::TestTone,
            Generator::Plugin(fx) => {
                return KINDS
                    .into_iter()
                    .find(|k| k.plugin_id() == Some(fx.id.as_str()));
            }
            Generator::Adjustment => return None,
        })
    }

    /// The identifier of BananaV's OFX generator that makes it.
    fn plugin_id(self) -> Option<&'static str> {
        match self {
            Kind::Checkerboard => Some("org.bananav.Checkerboard"),
            Kind::ColorGradient => Some("org.bananav.ColorGradient"),
            Kind::CreditRoll => Some("org.bananav.CreditRoll"),
            Kind::NoiseTexture => Some("org.bananav.NoiseTexture"),
            Kind::SolidColor => Some("org.bananav.SolidColor"),
            Kind::TestPattern => Some("org.bananav.TestPattern"),
            Kind::TitlesAndText => Some(bv_engine::plugins::TITLES_ID),
            Kind::TestTone => None,
        }
    }

    /// That plug-in, when loaded.
    fn plugin(self) -> Option<std::sync::Arc<bv_ofx::PluginInfo>> {
        let id = self.plugin_id()?;
        crate::ofx::generators()
            .into_iter()
            .find(|p| p.identifier == id)
    }

    /// Its key (favorites, the presets saved).
    fn key(self) -> String {
        format!("gen:{self:?}")
    }

    /// BananaV has it: Titles & Text and Credit Roll when their plug-ins
    /// are loaded (the others have their older built-in presets).
    fn ready(self) -> bool {
        match self {
            Kind::CreditRoll | Kind::TitlesAndText => self.plugin().is_some(),
            _ => true,
        }
    }

    /// Its category among [`categories`].
    fn category(self) -> usize {
        match self {
            Kind::ColorGradient | Kind::NoiseTexture => 0,
            Kind::CreditRoll | Kind::TitlesAndText => 1,
            _ => 2,
        }
    }

    fn about(self) -> &'static str {
        match self {
            Kind::Checkerboard => tr("Checked and striped patterns, sharp or blended."),
            Kind::ColorGradient => {
                tr("Colors blending into each other across the frame or around a center.")
            }
            Kind::CreditRoll => tr("Names rolling up the screen, as at a film's end."),
            Kind::NoiseTexture => tr("Textures from noise: clouds, lava, metal, wood, marble."),
            Kind::SolidColor => tr("The whole frame in one color."),
            Kind::TestPattern => tr("Color bars and other patterns for checking the picture."),
            Kind::TestTone => tr("A steady tone for checking the sound levels."),
            Kind::TitlesAndText => tr("Text over the picture: titles and captions."),
        }
    }

    /// Its built-in presets (the older generators' where a plug-in makes
    /// it now); `None`: Solid Color's "Custom Color…", which asks.
    fn presets(self) -> Vec<(&'static str, Option<Generator>)> {
        let solid = |r, g, b| {
            Some(Generator::SolidColor {
                rgba: [r, g, b, 1.0],
            })
        };
        match self {
            Kind::SolidColor => vec![
                (tr("White"), solid(1.0, 1.0, 1.0)),
                (tr("Black"), solid(0.0, 0.0, 0.0)),
                (tr("Red"), solid(1.0, 0.0, 0.0)),
                (tr("Green"), solid(0.0, 1.0, 0.0)),
                (tr("Blue"), solid(0.0, 0.0, 1.0)),
                (tr("Yellow"), solid(1.0, 1.0, 0.0)),
                (tr("Gray"), solid(0.5, 0.5, 0.5)),
                (tr("Cyan"), solid(0.0, 1.0, 1.0)),
                (tr("Magenta"), solid(1.0, 0.0, 1.0)),
                (tr("Orange"), solid(1.0, 0.5, 0.0)),
                ("Banana", solid(0.97, 0.79, 0.28)),
                (tr("Custom Color…"), None),
            ],
            Kind::TestPattern => vec![("SMPTE 75%", Some(Generator::ColorBars))],
            Kind::Checkerboard => vec![
                (tr("Small"), Some(Generator::Checkerboard { size: 30 })),
                (tr("Medium"), Some(Generator::Checkerboard { size: 60 })),
                (tr("Large"), Some(Generator::Checkerboard { size: 120 })),
            ],
            Kind::ColorGradient => {
                let g = |a: [f32; 3], b: [f32; 3], angle_deg| {
                    Some(Generator::Gradient {
                        from: [a[0], a[1], a[2], 1.0],
                        to: [b[0], b[1], b[2], 1.0],
                        angle_deg,
                    })
                };
                vec![
                    (tr("Sunset"), g([0.98, 0.6, 0.2], [0.35, 0.1, 0.45], 90.0)),
                    (tr("Ocean"), g([0.1, 0.7, 0.75], [0.03, 0.1, 0.3], 90.0)),
                    ("Banana", g([1.0, 0.9, 0.4], [0.95, 0.55, 0.1], 45.0)),
                    (tr("Grays"), g([0.0, 0.0, 0.0], [1.0, 1.0, 1.0], 0.0)),
                    (tr("Night"), g([0.05, 0.05, 0.15], [0.2, 0.1, 0.35], 135.0)),
                ]
            }
            Kind::NoiseTexture => {
                let n = |amount, color| Some(Generator::Noise { amount, color });
                vec![
                    (tr("Static"), n(1.0, false)),
                    (tr("Colorful"), n(1.0, true)),
                    (tr("Soft Grain"), n(0.25, false)),
                ]
            }
            Kind::TestTone => {
                let t = |frequency_hz, level_db| {
                    Some(Generator::TestTone {
                        frequency_hz,
                        level_db,
                    })
                };
                vec![
                    ("1 kHz −18 dBFS", t(1000.0, -18.0)),
                    ("1 kHz −12 dBFS", t(1000.0, -12.0)),
                    (tr("440 Hz (A) −12 dBFS"), t(440.0, -12.0)),
                    ("100 Hz −18 dBFS", t(100.0, -18.0)),
                    ("10 kHz −24 dBFS", t(10_000.0, -24.0)),
                ]
            }
            Kind::CreditRoll | Kind::TitlesAndText => Vec::new(),
        }
    }
}

/// Titles & Text at its defaults (Insert › Text Media's).
pub(crate) fn titles_generator() -> Option<Generator> {
    Kind::TitlesAndText
        .plugin()
        .map(|p| crate::ofx::new_generator(&p))
}

/// The factory presets of BananaV's generator plug-in `info`, as
/// generators.
fn own_presets(info: &bv_ofx::PluginInfo) -> Vec<(String, Option<Generator>)> {
    crate::presets::builtin_plugin(&info.identifier)
        .into_iter()
        .map(|(name, values)| {
            let mut fx = crate::ofx::plugin_fx(info);
            fx.params = values;
            (name, Some(Generator::Plugin(Box::new(fx))))
        })
        .collect()
}

/// Titles & Text's presets, BananaV's and the user's, as generators (the
/// Insert Subtitles dialog's list).
pub(crate) fn titles_presets_all(app: &BananaApp) -> Vec<(String, Generator)> {
    if Kind::TitlesAndText.plugin().is_none() {
        return Vec::new();
    }
    all_presets(app, Kind::TitlesAndText)
        .into_iter()
        .filter_map(|(n, g)| g.map(|g| (n, g)))
        .collect()
}

/// The OFX generators of other makers (BananaV's own show in their own
/// places: Titles & Text, Credit Roll, …).
fn third_party() -> Vec<std::sync::Arc<bv_ofx::PluginInfo>> {
    crate::ofx::generators()
        .into_iter()
        .filter(|p| !bv_engine::plugins::is_own(p))
        .collect()
}

/// A kind's presets: BananaV's (its plug-in's, when loaded), then the
/// user's (saved in the Video Media Generators window); Solid Color's
/// Custom Color… last.
fn all_presets(app: &BananaApp, kind: Kind) -> Vec<(String, Option<Generator>)> {
    let mut v: Vec<(String, Option<Generator>)> = match kind.plugin() {
        Some(info) => own_presets(&info),
        None => kind
            .presets()
            .into_iter()
            .map(|(n, g)| (n.to_string(), g))
            .collect(),
    };
    let custom = v.iter().position(|(_, g)| g.is_none()).map(|i| v.remove(i));
    v.extend(user_presets(app, &kind.key()));
    v.extend(custom);
    v
}

/// The presets the user saved for the plug-in `key`.
fn user_presets(app: &BananaApp, key: &str) -> Vec<(String, Option<Generator>)> {
    app.settings
        .generator_presets
        .iter()
        .filter(|s| s.plugin == key)
        .map(|s| (s.name.clone(), Some(s.generator.clone())))
        .collect()
}

/// An OFX generator's key (favorites, the presets saved).
fn plugin_key(id: &str) -> String {
    format!("ofx:{id}")
}

/// An OFX generator's presets: its defaults, then the user's.
fn plugin_presets(app: &BananaApp, fx: &PluginFx) -> Vec<(String, Option<Generator>)> {
    let defaults = PluginFx {
        params: Default::default(),
        ..fx.clone()
    };
    let mut v = vec![(
        tr("(Default)").to_string(),
        Some(Generator::Plugin(Box::new(defaults))),
    )];
    v.extend(user_presets(app, &plugin_key(&fx.id)));
    v
}

/// A generator's plug-in for the Video Media Generators window: its key,
/// its name and its presets.
pub(crate) fn plugin_of(
    app: &BananaApp,
    g: &Generator,
) -> (String, String, Vec<(String, Generator)>) {
    let (key, name, presets) = match (g, Kind::of(g)) {
        (_, Some(kind)) => (kind.key(), kind.name().to_string(), all_presets(app, kind)),
        (Generator::Plugin(fx), None) => (
            plugin_key(&fx.id),
            g.display_name(),
            plugin_presets(app, fx),
        ),
        _ => (String::new(), String::new(), Vec::new()),
    };
    let presets = presets
        .into_iter()
        .filter_map(|(n, g)| g.map(|g| (n, g)))
        .collect();
    (key, name, presets)
}

/// What the Video Media Generators window's Replace Plug-In offers: each
/// generator making that kind of media (`video` or sound) with its first
/// preset, by name; other makers' OFX generators at their defaults.
pub(crate) fn replacements(app: &BananaApp, video: bool) -> Vec<(String, Generator)> {
    let mut v: Vec<(String, Generator)> = KINDS
        .into_iter()
        .filter(|k| k.ready() && (*k == Kind::TestTone) != video)
        .filter_map(|k| {
            let g = all_presets(app, k).into_iter().find_map(|(_, g)| g)?;
            Some((k.name().to_string(), g))
        })
        .collect();
    if video {
        v.extend(
            third_party()
                .iter()
                .map(|p| (p.label.clone(), crate::ofx::new_generator(p))),
        );
    }
    v
}

/// A preset's +: the generator added at the cursor (on the selected track
/// when it takes it), its window open to edit it.
pub(crate) fn add_and_edit(app: &mut BananaApp, g: &Generator) {
    let before: std::collections::HashSet<bv_timeline::ClipId> =
        app.project().clips().map(|(_, c)| c.id).collect();
    let at = app.cursor;
    app.add_generator(g.clone(), at, app.selected_track);
    let added = app
        .project()
        .clips()
        .map(|(_, c)| c.id)
        .find(|id| !before.contains(id));
    if let Some(clip) = added {
        app.open_generated_media(clip);
    }
}

/// The window's categories, as VEGAS's.
fn categories() -> [Category; 4] {
    let c = |name| Category {
        name,
        ready: true,
        packages: false,
    };
    [
        c(tr("Creative")),
        c(tr("Titles and Text")),
        c(tr("Utility")),
        // OFX plug-ins, as VEGAS lists other makers'.
        Category {
            name: tr("Third Party"),
            ready: !third_party().is_empty(),
            packages: false,
        },
    ]
}

/// The window's index of the Third Party category (OFX plug-ins).
const THIRD_PARTY: usize = 3;

/// How long a title's animation plays where it has no event: the length
/// an event of generated media takes.
const TITLE_SECONDS: f64 = 5.0;

/// Whether a Titles & Text preset moves (its Animation is one).
fn animated(fx: &PluginFx) -> bool {
    fx.params.get("animationName").is_some_and(
        |p| matches!(&p.value, bv_timeline::PluginValue::Text(t) if !t.is_empty() && t != "_None"),
    )
}

/// A preset's picture: the generated frame (noise moving while it plays),
/// a speaker for a tone, the colors for Custom Color….
fn paint_preset(
    painter: &egui::Painter,
    rect: Rect,
    app: &mut BananaApp,
    g: Option<&Generator>,
    play: Option<f32>,
) {
    let Some(g) = g else {
        // Custom Color…: the hues.
        let n = 24;
        for i in 0..n {
            let x0 = rect.left() + rect.width() * i as f32 / n as f32;
            let x1 = rect.left() + rect.width() * (i + 1) as f32 / n as f32;
            let hue = egui::ecolor::Hsva::new(i as f32 / n as f32, 0.8, 0.95, 1.0);
            painter.rect_filled(
                Rect::from_min_max(pos2(x0, rect.top()), pos2(x1 + 0.5, rect.bottom())),
                0.0,
                Color32::from(hue),
            );
        }
        return;
    };
    let info = MediaInfo::generated(g.clone(), &app.project().settings);
    if info.video.is_none() {
        // Sound has no picture: dark under its speaker, not the squares
        // of what is see-through.
        painter.rect_filled(rect, 0.0, Color32::BLACK);
        icons::paint(
            painter,
            rect.shrink(20.0),
            Icon::Speaker,
            theme::clip_audio().gamma_multiply(1.6),
        );
        return;
    }
    // Noise moves: frames along the time it has played. A credit roll is
    // shown on its way (its ten seconds when it has no event), scrolling
    // under the pointer.
    let at = match (g, play) {
        (Generator::Noise { .. }, Some(t)) => Tick::from_seconds_f64((t * 4.0).floor() as f64),
        (Generator::Plugin(_), play) if Kind::of(g) == Some(Kind::CreditRoll) => {
            Tick::from_seconds_f64(3.0 + f64::from(play.unwrap_or(0.0)) * 1.5)
        }
        // An animated title: halfway through its five seconds, playing
        // them under the pointer.
        (Generator::Plugin(fx), play)
            if Kind::of(g) == Some(Kind::TitlesAndText) && animated(fx) =>
        {
            Tick::from_seconds_f64(play.map_or(2.5, |t| f64::from(t) % TITLE_SECONDS))
        }
        _ => Tick::ZERO,
    };
    let h = (rect.height() * painter.ctx().pixels_per_point()).round() as u32;
    if let Some(t) = app.engine.thumbnail(&info, at, h) {
        let id = app.textures.get(painter.ctx(), &t);
        painter.image(
            id,
            rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
}

pub fn ui(ui: &mut egui::Ui, app: &mut BananaApp) {
    let mut state = std::mem::take(&mut app.choosers.generators);
    let categories = categories();
    let ofx = third_party();
    let own = KINDS.iter().map(|&k| Plugin {
        key: k.key(),
        name: k.name().to_string(),
        category: k.category(),
        ready: k.ready(),
        about: [
            if k == Kind::TestTone || k.plugin().is_some() {
                trf!("{}: BananaV, 32-bit floating point", k.name())
            } else {
                trf!(
                    "{}: BananaV, 32-bit floating point, GPU accelerated",
                    k.name()
                )
            },
            trf!("Description: {}", k.about()),
        ],
        provider: None,
    });
    let theirs = ofx.iter().map(|p| Plugin {
        key: plugin_key(&p.identifier),
        name: crate::ofx::plugin_name(p),
        category: THIRD_PARTY,
        ready: true,
        about: [
            trf!(
                "{}: OFX plug-in {}, group {}",
                p.name(),
                p.identifier,
                p.grouping
            ),
            trf!("Description: {}", p.description),
        ],
        provider: Some(crate::ofx::provider(p)),
    });
    let plugins: Vec<Plugin> = own.chain(theirs).collect();
    let frame = chooser::frame(
        ui,
        "generators",
        &mut state,
        &mut app.settings.favorites,
        &categories,
        &plugins,
        Icon::Generator,
    );
    if frame.favorites_changed {
        let _ = app.settings.save();
    }
    if let Some(i) = frame.plugin {
        let presets = match KINDS.get(i) {
            Some(&k) => all_presets(app, k),
            None => ofx
                .get(i - KINDS.len())
                .map(|p| plugin_presets(app, &crate::ofx::plugin_fx(p)))
                .unwrap_or_default(),
        };
        // Dragged from the list: its first preset.
        if frame.dragged
            && let Some(g) = presets.iter().find_map(|(_, g)| g.clone())
        {
            app.media_drag = Some(MediaDrag::Generator(g));
        }
        let names: Vec<String> = presets.iter().map(|(n, _)| n.clone()).collect();
        let cells = chooser::preset_grid(
            ui,
            frame.grid,
            "generators",
            &mut state,
            &names,
            tr("Add at the cursor and edit"),
            |painter, rect, i, play| paint_preset(painter, rect, app, presets[i].1.as_ref(), play),
        );
        for cell in cells {
            match &presets[cell.i].1 {
                Some(g) => {
                    if cell.resp.drag_started() {
                        app.media_drag = Some(MediaDrag::Generator(g.clone()));
                    }
                    // On the selected track, as VEGAS's.
                    if cell.double {
                        let at = app.cursor;
                        app.add_generator(g.clone(), at, app.selected_track);
                    }
                    if cell.add {
                        add_and_edit(app, g);
                    }
                }
                None => {
                    if cell.resp.clicked() {
                        app.generator_color = Some(crate::color_picker::ColorPicker::new(
                            tr("Custom Color"),
                            [0xF7, 0xC9, 0x48, 0xFF],
                        ));
                    }
                    cell.resp
                        .on_hover_text(tr("Choose a color: it goes to the cursor."));
                }
            }
        }
    }
    app.choosers.generators = state;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_generators_are_listed_as_vegas_names_them() {
        for k in KINDS {
            // Those of BananaV's plug-ins only have theirs; the others have
            // built-in presets (the older generators' where a plug-in
            // makes it now).
            if !matches!(k, Kind::TitlesAndText | Kind::CreditRoll) {
                assert!(k.ready() && !k.presets().is_empty(), "{k:?}");
            }
            assert!(k.category() < categories().len());
        }
        // A plug-in's generator is its kind's.
        let fx = PluginFx {
            id: "org.bananav.CreditRoll".into(),
            version: [1, 0],
            name: "Credit Roll".into(),
            params: Default::default(),
        };
        assert_eq!(
            Kind::of(&Generator::Plugin(Box::new(fx))),
            Some(Kind::CreditRoll)
        );
        // Solid Color: VEGAS's colors, then BananaV's and the custom one.
        let solid = Kind::SolidColor.presets();
        assert!(matches!(
            solid[0].1,
            Some(Generator::SolidColor {
                rgba: [1.0, 1.0, 1.0, 1.0]
            })
        ));
        assert!(solid.last().is_some_and(|(_, g)| g.is_none()));
    }
}
