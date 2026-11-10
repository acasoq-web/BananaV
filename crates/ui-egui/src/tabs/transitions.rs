//! Transitions: VEGAS's plug-in window for transitions. BananaV's own,
//! VEGAS's set as OFX plug-ins (`bv_effects`' transitions: Linear Wipe,
//! Iris, Push…), are listed in VEGAS's categories with their presets, next
//! to the two simplest built-in ones (Cross Dissolve, Fade Through Black,
//! drawn on the graphics card); other makers' OFX transitions are Third
//! Party. Each preset is drawn by its plug-in (or, for the built-in ones,
//! as A going and B coming). Drag a preset to the start of an event, or
//! double-click it to give it to the selected events. The older built-in
//! wipes, pushes and the like stay for the projects that use them.

use std::collections::HashMap;

use crate::i18n::tr;
use bv_core::Tick;
use bv_timeline::{Clip, ClipId, Direction, OutTransition, PluginFx, Transition};

use super::chooser::{self, Category, Plugin};
use crate::app::{BananaApp, MediaDrag};
use crate::egui::{
    self, Align2, Color32, ColorImage, FontId, Painter, Rect, Shape, TextureHandle, TextureOptions,
    pos2, vec2,
};
use crate::icons::Icon;

const A: Color32 = Color32::from_rgb(0xF7, 0xC9, 0x48);
const B: Color32 = Color32::from_rgb(0x3E, 0x5F, 0x95);

/// Desenha uma prévia esquemática da transição (A sai, B entra) em `r`.
pub fn paint_preview(painter: &Painter, r: Rect, t: Transition, p: f32) {
    let p = p.clamp(0.0, 1.0);
    let label = |painter: &Painter, rect: Rect, s: &str, c: Color32| {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            s,
            FontId::proportional(18.0),
            c,
        );
    };
    let full = |painter: &Painter, c: Color32, s: &str| {
        painter.rect_filled(r, 0.0, c);
        label(painter, r, s, Color32::from_black_alpha(140));
    };
    let clipped = |rect: Rect| painter.with_clip_rect(rect.intersect(r));
    match t {
        Transition::Dissolve => {
            full(painter, A, "A");
            painter.rect_filled(r, 0.0, B.gamma_multiply(p));
            label(
                painter,
                r,
                "B",
                Color32::from_white_alpha((p * 200.0) as u8),
            );
        }
        Transition::FadeThroughBlack => {
            if p < 0.5 {
                full(painter, A, "A");
                painter.rect_filled(r, 0.0, Color32::from_black_alpha((p * 2.0 * 255.0) as u8));
            } else {
                full(painter, B, "B");
                painter.rect_filled(
                    r,
                    0.0,
                    Color32::from_black_alpha(((1.0 - p) * 2.0 * 255.0) as u8),
                );
            }
        }
        Transition::Wipe(d) => {
            full(painter, A, "A");
            let (w, h) = (r.width(), r.height());
            let rb = match d {
                Direction::Left => Rect::from_min_max(pos2(r.right() - w * p, r.top()), r.max),
                Direction::Right => Rect::from_min_max(r.min, pos2(r.left() + w * p, r.bottom())),
                Direction::Up => Rect::from_min_max(pos2(r.left(), r.bottom() - h * p), r.max),
                Direction::Down => Rect::from_min_max(r.min, pos2(r.right(), r.top() + h * p)),
            };
            let cp = clipped(rb);
            cp.rect_filled(r, 0.0, B);
            label(&cp, r, "B", Color32::from_white_alpha(200));
        }
        Transition::Push(d) | Transition::Slide(d) => {
            let push = matches!(t, Transition::Push(_));
            let off = match d {
                Direction::Left => vec2(-r.width(), 0.0),
                Direction::Right => vec2(r.width(), 0.0),
                Direction::Up => vec2(0.0, -r.height()),
                Direction::Down => vec2(0.0, r.height()),
            };
            let ra = if push { r.translate(off * p) } else { r };
            let rb = r.translate(-off * (1.0 - p));
            let cp = clipped(r);
            cp.rect_filled(ra, 0.0, A);
            label(&cp, ra, "A", Color32::from_black_alpha(140));
            cp.rect_filled(rb, 0.0, B);
            label(&cp, rb, "B", Color32::from_white_alpha(200));
        }
        Transition::Iris => {
            full(painter, A, "A");
            let max = (r.width().powi(2) + r.height().powi(2)).sqrt() / 2.0;
            let cp = clipped(r);
            cp.circle_filled(r.center(), max * p, B);
            label(&cp, r, "B", Color32::from_white_alpha((p * 220.0) as u8));
        }
        Transition::Clock => {
            full(painter, A, "A");
            let cp = clipped(r);
            let radius = r.width().max(r.height());
            let n = 40;
            let mut pts = vec![r.center()];
            for i in 0..=n {
                let a =
                    -std::f32::consts::FRAC_PI_2 + p * std::f32::consts::TAU * i as f32 / n as f32;
                pts.push(r.center() + vec2(a.cos(), a.sin()) * radius);
            }
            // O leque pode ser côncavo: uma malha de triângulos, sem emendas.
            let mut mesh = egui::Mesh::default();
            for p in &pts {
                mesh.colored_vertex(*p, B);
            }
            for i in 1..pts.len() as u32 - 1 {
                mesh.add_triangle(0, i, i + 1);
            }
            cp.add(Shape::mesh(mesh));
        }
        Transition::Zoom => {
            full(painter, A, "A");
            let s = 0.05 + 0.95 * p;
            let rb = Rect::from_center_size(r.center(), r.size() * s);
            painter.rect_filled(rb, 0.0, B.gamma_multiply(p.max(0.2)));
            label(
                painter,
                rb,
                "B",
                Color32::from_white_alpha((p * 220.0) as u8),
            );
        }
    }
}

/// Define a transição de entrada dos clipes; se não houver fade nem
/// sobreposição, cria um fade de entrada de 1 s para ela aparecer.
pub fn apply(app: &mut BananaApp, clips: &[ClipId], t: Transition) {
    apply_with(app, clips, move |c| {
        c.transition = t;
        c.transition_fx = None;
    });
}

/// An OFX plug-in's transition for the events (BananaV's own stays, and
/// stands in where the plug-in is missing).
pub fn apply_plugin(app: &mut BananaApp, clips: &[ClipId], fx: &PluginFx) {
    let fx = fx.clone();
    apply_with(app, clips, move |c| {
        c.transition_fx = Some(Box::new(fx.clone()));
    });
}

/// A transition let go on the end of event `clip` (a video one): the
/// entrance transition of the event after it, where one starts there (a
/// crossfade), else the transition `clip` goes out with, over its fade-out
/// — a second long when it has none —, as VEGAS's.
pub fn apply_at_end(app: &mut BananaApp, clip: ClipId, drag: &MediaDrag) {
    let p = app.project();
    let Some(c) = p.clip(clip) else {
        return;
    };
    let next = p.clip_track(clip).and_then(|t| {
        t.clips
            .iter()
            .filter(|n| n.id != clip && !n.muted && n.start > c.start && n.start <= c.end())
            .min_by_key(|n| n.start)
            .map(|n| n.id)
    });
    match (next, drag) {
        (Some(n), MediaDrag::Transition(t)) => apply(app, &[n], *t),
        (Some(n), MediaDrag::PluginTransition(fx)) => apply_plugin(app, &[n], fx),
        (None, MediaDrag::Transition(t)) => {
            let t = *t;
            apply_out(app, clip, move |o| {
                o.kind = t;
                o.fx = None;
            });
        }
        (None, MediaDrag::PluginTransition(fx)) => {
            let fx = fx.clone();
            apply_out(app, clip, move |o| o.fx = Some(fx));
        }
        _ => {}
    }
}

/// Gives event `clip` the transition it goes out with (`set`), on, over
/// its fade-out: a second long when it has none.
fn apply_out(app: &mut BananaApp, clip: ClipId, set: impl FnOnce(&mut OutTransition)) {
    app.modify(|p| {
        if let Some(c) = p.clip_mut(clip) {
            let o = c.out_transition.get_or_insert_with(Default::default);
            set(o);
            o.bypass = false;
            if c.fade_out == Tick::ZERO {
                c.fade_out = Tick::SECOND.min(c.duration - c.fade_in).max(Tick::ZERO);
            }
        }
    });
}

/// Gives the events their transition with `set`, making room for it.
fn apply_with(app: &mut BananaApp, clips: &[ClipId], set: impl Fn(&mut Clip)) {
    let ids = clips.to_vec();
    let (before, after) = app.settings.prefs.cut_overlap_split();
    app.modify(|p| {
        for id in ids {
            let has_overlap = p
                .clip_track(id)
                .zip(p.clip(id))
                .is_some_and(|(tr, c)| tr.effective_fades(c, p.auto_crossfade).0 > c.fade_in);
            // A cut with the event before: the two grow into each other for
            // the transition (Options › Preferences › Editing: "Cut-to-overlap
            // conversion").
            let made = if has_overlap {
                Tick::ZERO
            } else {
                p.overlap_cut(id, before, after).unwrap_or(Tick::ZERO)
            };
            let auto = p.auto_crossfade;
            if let Some(c) = p.clip_mut(id) {
                set(c);
                if made > Tick::ZERO && !auto {
                    // Without Automatic Crossfades the event fades in over
                    // the overlap by itself.
                    c.fade_in = c.fade_in.max(made);
                }
                if c.fade_in == Tick::ZERO && !has_overlap && made == Tick::ZERO {
                    c.fade_in = Tick::SECOND.min(c.duration - c.fade_out);
                }
            }
        }
    });
}

/// A transition of the list; its presets are its directions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Dissolve,
    FadeThroughBlack,
    Wipe,
    Push,
    Slide,
    Iris,
    Clock,
    Zoom,
}

const KINDS: [Kind; 8] = [
    Kind::Dissolve,
    Kind::FadeThroughBlack,
    Kind::Wipe,
    Kind::Push,
    Kind::Slide,
    Kind::Iris,
    Kind::Clock,
    Kind::Zoom,
];

fn kind_of(t: Transition) -> Kind {
    match t {
        Transition::Dissolve => Kind::Dissolve,
        Transition::FadeThroughBlack => Kind::FadeThroughBlack,
        Transition::Wipe(_) => Kind::Wipe,
        Transition::Push(_) => Kind::Push,
        Transition::Slide(_) => Kind::Slide,
        Transition::Iris => Kind::Iris,
        Transition::Clock => Kind::Clock,
        Transition::Zoom => Kind::Zoom,
    }
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Dissolve => tr("Cross Dissolve"),
            Kind::FadeThroughBlack => tr("Fade Through Black"),
            Kind::Wipe => tr("Wipe"),
            Kind::Push => tr("Push"),
            Kind::Slide => tr("Slide"),
            Kind::Iris => tr("Iris"),
            Kind::Clock => tr("Clock Wipe"),
            Kind::Zoom => tr("Zoom"),
        }
    }

    /// Its category among [`categories`].
    fn category(self) -> usize {
        match self {
            Kind::Zoom => 0,
            Kind::Push | Kind::Slide => 1,
            Kind::Wipe | Kind::Iris | Kind::Clock => 2,
            Kind::Dissolve | Kind::FadeThroughBlack => 3,
        }
    }

    fn about(self) -> &'static str {
        match self {
            Kind::Dissolve => tr("The next picture appears through the first one."),
            Kind::FadeThroughBlack => {
                tr("The picture fades to black, then the next one comes up from it.")
            }
            Kind::Wipe => tr("An edge sweeps across the picture, revealing the next one."),
            Kind::Push => tr("The next picture pushes the first one out of the frame."),
            Kind::Slide => tr("The next picture slides in over the first one."),
            Kind::Iris => tr("A circle opens from the middle onto the next picture."),
            Kind::Clock => tr("A clock hand sweeps around, revealing the next picture."),
            Kind::Zoom => tr("The next picture grows from the middle of the frame."),
        }
    }

    /// Whether the window lists it: the built-in transitions BananaV's
    /// plug-ins of the same names supersede are not.
    fn listed(self) -> bool {
        matches!(self, Kind::Dissolve | Kind::FadeThroughBlack)
    }

    /// Its presets: the transitions of its kind (its directions).
    fn presets(self) -> Vec<Transition> {
        Transition::ALL
            .into_iter()
            .filter(|&t| kind_of(t) == self)
            .collect()
    }
}

/// The window's categories, as VEGAS's (GL Transitions: its GL
/// Transition, the open collection's transitions as presets).
fn categories() -> [Category; 7] {
    let c = |name, ready| Category {
        name,
        ready,
        packages: false,
    };
    [
        c(tr("3D Effects"), true),
        c(tr("Reveals"), true),
        c(tr("Wipes"), true),
        c(tr("Fades"), true),
        c(tr("Loop and Peels"), true),
        c(tr("GL Transitions"), true),
        // OFX plug-ins, as VEGAS lists other makers'.
        c(
            tr("Third Party"),
            crate::ofx::transitions()
                .iter()
                .any(|p| !bv_engine::plugins::is_own(p)),
        ),
    ]
}

/// The window's index of the Third Party category (OFX plug-ins).
const THIRD_PARTY: usize = 6;

/// The category of one of BananaV's own transitions, by its grouping
/// ("BananaV/Wipes").
fn own_category(info: &bv_ofx::PluginInfo) -> usize {
    match info.grouping.rsplit('/').next().unwrap_or("") {
        "3D Effects" => 0,
        "Reveals" => 1,
        "Wipes" => 2,
        "Loop and Peels" => 4,
        "GL Transitions" => 5,
        _ => 3,
    }
}

/// A preset of the window: one of BananaV's transitions or a plug-in's.
#[derive(Clone, Debug)]
enum Pick {
    Own(Transition),
    Plugin(PluginFx),
}

/// The transition for the selected events.
fn apply_selected(app: &mut BananaApp, t: &Pick) {
    let sel = app.selected();
    if sel.is_empty() {
        app.notify(tr("Select a clip on the timeline to apply the transition."));
        return;
    }
    match t {
        Pick::Own(t) => apply(app, &sel, *t),
        Pick::Plugin(fx) => apply_plugin(app, &sel, fx),
    }
}

/// The pictures of plug-ins' transitions, made as they are first shown
/// (by plug-in and preset — its values —, size and step of the way;
/// `None`: the plug-in drew none).
#[derive(Default)]
pub struct Previews {
    map: HashMap<(String, u32, u32, u32), Option<TextureHandle>>,
}

impl std::fmt::Debug for Previews {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Previews")
            .field("n", &self.map.len())
            .finish()
    }
}

/// The steps of the way a plug-in's preview is drawn at.
const STEPS: u32 = 24;

/// A plug-in's transition in `rect`, `p` (0–1) of the way from the sample
/// picture to its night; BananaV's dissolve where the plug-in draws none.
fn paint_plugin(painter: &Painter, rect: Rect, previews: &mut Previews, fx: &PluginFx, p: f32) {
    let ctx = painter.ctx().clone();
    let ppp = ctx.pixels_per_point();
    let (w, h) = (
        (rect.width() * ppp).round().max(1.0) as u32,
        (rect.height() * ppp).round().max(1.0) as u32,
    );
    let step = (p.clamp(0.0, 1.0) * STEPS as f32).round() as u32;
    let texture = previews
        .map
        .entry((format!("{}{:?}", fx.id, fx.params), w, h, step))
        .or_insert_with(|| {
            let progress = step as f32 / STEPS as f32;
            let rgba = bv_engine::transition_sample(fx, progress, w, h)?;
            Some(ctx.load_texture(
                "transicao",
                ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba),
                TextureOptions::LINEAR,
            ))
        });
    match texture {
        Some(t) => {
            let all = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            painter.image(t.id(), rect, all, Color32::WHITE);
        }
        None => paint_preview(painter, rect, Transition::Dissolve, p),
    }
}

/// Transitions: VEGAS's plug-in window. A preset's picture shows the
/// transition halfway; under the pointer it plays.
pub fn ui(ui: &mut egui::Ui, app: &mut BananaApp) {
    let mut state = std::mem::take(&mut app.choosers.transitions);
    let categories = categories();
    let (mine, ofx): (Vec<_>, Vec<_>) = crate::ofx::transitions()
        .into_iter()
        .partition(|p| bv_engine::plugins::is_own(p));
    let kinds: Vec<Kind> = KINDS.into_iter().filter(|k| k.listed()).collect();
    let own = kinds.iter().map(|&k| Plugin {
        key: format!("tr:{k:?}"),
        name: k.name().to_string(),
        category: k.category(),
        ready: true,
        about: [
            trf!(
                "{}: BananaV, 32-bit floating point, GPU accelerated",
                k.name()
            ),
            trf!("Description: {}", k.about()),
        ],
        provider: None,
    });
    let plugins_own = mine.iter().map(|p| Plugin {
        key: format!("ofx:{}", p.identifier),
        name: crate::ofx::plugin_name(p),
        category: own_category(p),
        ready: true,
        about: [
            trf!(
                "{}: BananaV, 32-bit floating point",
                crate::ofx::plugin_name(p)
            ),
            trf!("Description: {}", tr(&p.description)),
        ],
        provider: None,
    });
    let theirs = ofx.iter().map(|p| Plugin {
        key: format!("ofx:{}", p.identifier),
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
    let plugins: Vec<Plugin> = own.chain(plugins_own).chain(theirs).collect();
    let frame = chooser::frame(
        ui,
        "transitions",
        &mut state,
        &mut app.settings.favorites,
        &categories,
        &plugins,
        Icon::Transition,
    );
    if frame.favorites_changed {
        let _ = app.settings.save();
    }
    if let Some(i) = frame.plugin {
        let (presets, names): (Vec<Pick>, Vec<String>) = match kinds.get(i) {
            Some(k) => {
                let presets = k.presets();
                let names = if presets.len() == 1 {
                    vec![tr("Default").to_string()]
                } else {
                    presets.iter().map(|t| tr(t.name()).to_string()).collect()
                };
                (presets.into_iter().map(Pick::Own).collect(), names)
            }
            None if i - kinds.len() < mine.len() => {
                // BananaV's own: its factory presets.
                let p = &mine[i - kinds.len()];
                let presets = crate::presets::builtin_plugin(&p.identifier);
                if presets.is_empty() {
                    (
                        vec![Pick::Plugin(crate::ofx::plugin_fx(p))],
                        vec![tr("(Default)").to_string()],
                    )
                } else {
                    let names = presets.iter().map(|(n, _)| n.clone()).collect();
                    let picks = presets
                        .into_iter()
                        .map(|(_, params)| {
                            Pick::Plugin(PluginFx {
                                params,
                                ..crate::ofx::plugin_fx(p)
                            })
                        })
                        .collect();
                    (picks, names)
                }
            }
            None => ofx
                .get(i - kinds.len() - mine.len())
                .map(|p| {
                    let fx = crate::ofx::plugin_fx(p);
                    (vec![Pick::Plugin(fx)], vec![tr("(Default)").to_string()])
                })
                .unwrap_or_default(),
        };
        // Dragged from the list: its first preset.
        if frame.dragged
            && let Some(t) = presets.first()
        {
            app.media_drag = Some(match t {
                Pick::Own(t) => MediaDrag::Transition(*t),
                Pick::Plugin(fx) => MediaDrag::PluginTransition(Box::new(fx.clone())),
            });
        }
        let previews = &mut app.transition_previews;
        let cells = chooser::preset_grid(
            ui,
            frame.grid,
            "transitions",
            &mut state,
            &names,
            tr("Add to the selected events"),
            |painter, rect, i, play| {
                let p = play.map_or(0.45, chooser::progress);
                match &presets[i] {
                    Pick::Own(t) => paint_preview(painter, rect, *t, p),
                    Pick::Plugin(fx) => paint_plugin(painter, rect, previews, fx, p),
                }
            },
        );
        for cell in cells {
            let t = &presets[cell.i];
            if cell.resp.drag_started() {
                app.media_drag = Some(match t {
                    Pick::Own(t) => MediaDrag::Transition(*t),
                    Pick::Plugin(fx) => MediaDrag::PluginTransition(Box::new(fx.clone())),
                });
            }
            if cell.double || cell.add {
                apply_selected(app, t);
            }
        }
    }
    app.choosers.transitions = state;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_transition_is_a_preset_of_one_kind() {
        let all: Vec<Transition> = KINDS.iter().flat_map(|k| k.presets()).collect();
        assert_eq!(all.len(), Transition::ALL.len());
        for t in Transition::ALL {
            assert!(all.contains(&t), "{t:?}");
        }
        assert!(KINDS.iter().all(|k| k.category() < categories().len()));
    }
}
