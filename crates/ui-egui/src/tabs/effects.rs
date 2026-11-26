//! Video FX: VEGAS's plug-in window for video effects. Each preset's
//! picture is BananaV's sample picture with the effect; under the pointer
//! the effect sweeps across it, moving when it moves by itself. Drag a preset onto an event, or
//! double-click it to apply it to the selected events. Filter Packages lists the chains saved as packages
//! (`fx_packages.rs`), each one preset: the sample through its chain.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, mpsc};

use crate::i18n::tr;
use bv_timeline::{ClipId, Effect, EffectKind, TrackId, TrackKind};

use super::chooser::{self, Category, Plugin};
use crate::app::{BananaApp, MediaDrag};
use crate::egui::{
    self, Color32, ColorImage, Rect, Stroke, TextureHandle, TextureId, TextureOptions, pos2,
};
use crate::fx_packages::Package;
use crate::icons::Icon;

pub fn description(k: EffectKind) -> &'static str {
    match k {
        EffectKind::BrightnessContrast => tr("Brightens/darkens and raises/lowers the contrast."),
        EffectKind::ColorGrade => {
            tr("Exposure, contrast, saturation, white balance and lift/gamma/gain wheels.")
        }
        EffectKind::Levels => tr("Remaps input and output blacks, whites and gamma."),
        EffectKind::BlackAndWhite => tr("Converts to grayscale by luminance."),
        EffectKind::Sepia => tr("Aged tones of an old photograph."),
        EffectKind::Invert => tr("Image negative."),
        EffectKind::GaussianBlur => tr("Soft, even blur."),
        EffectKind::Sharpen => tr("Enhances detail (unsharp mask)."),
        EffectKind::Vignette => tr("Darkens the edges of the frame."),
        EffectKind::Mirror => tr("Mirrors horizontally and/or vertically."),
        EffectKind::Pixelate => tr("Large blocks (censoring, retro style)."),
        EffectKind::ChromaKey => tr("Removes a colored background (green or blue screen)."),
        EffectKind::Glow => tr("Makes bright areas glow."),
        EffectKind::Gain => tr("Raises or lowers the volume."),
        EffectKind::Eq3 => tr("Lows, mids and highs, with adjustable frequencies."),
        EffectKind::HighPass => tr("Removes lows below the frequency (rumble, wind)."),
        EffectKind::LowPass => tr("Removes highs above the frequency (hiss)."),
        EffectKind::Compressor => tr("Reduces the difference between loud and quiet parts."),
        EffectKind::Limiter => tr("Keeps the sound from going over the ceiling."),
        EffectKind::NoiseGate => tr("Silences background noise in the pauses."),
        EffectKind::Delay => tr("Repeats the sound like an echo."),
        EffectKind::Reverb => tr("Room ambience."),
        EffectKind::ModernEq => tr(
            "Up to eight bands drawn on a graph, on both channels, one of them, the middle or the sides.",
        ),
        EffectKind::Volume => tr("Raises or lowers the volume, up to 6 dB."),
        EffectKind::Pan => {
            tr("Moves a stereo sound left or right, blending one channel into the other.")
        }
        EffectKind::SmoothEnhance => {
            tr("Softens the highs (smooth) or brings them forward (enhance).")
        }
        EffectKind::NoiseGateFx => tr("Silences the sound while it stays below the threshold."),
        EffectKind::TrackNoiseGate => tr("Silences the track while it stays below the threshold."),
        EffectKind::SimpleDelay => {
            tr("An echo, or many fading away, its time in seconds or in notes of the tempo.")
        }
        EffectKind::Dither => {
            tr("Rounds the sound to a bit depth with a little noise, so it stays clean.")
        }
        EffectKind::ResonantFilter => {
            tr("A low-, band- or high-pass filter that can ring at its frequency.")
        }
        EffectKind::ParametricEq => tr("One shelf, band-pass or notch, accurate down to the lows."),
        EffectKind::Chorus => {
            tr("Thickens the sound with slightly detuned, delayed copies of itself.")
        }
        EffectKind::FlangeWahWah => tr("A sweeping flange, phaser or wah-wah."),
        EffectKind::ReverbFx => {
            tr("A room's reverberation: 19 spaces, early reflections and pre-delay.")
        }
        EffectKind::TrackCompressor => {
            tr("Evens out the level, from gentle compression to a limiter.")
        }
        EffectKind::TrackEq => tr("Four bands, each a low shelf, a peak or a high shelf."),
        EffectKind::ParagraphicEq => tr("Four peak or notch bands, with low and high shelves."),
        EffectKind::MultiTapDelay => {
            tr("Up to eight echoes of one delay line, each with its own time, level and side.")
        }
        EffectKind::MultiBandDynamics => tr(
            "Four bands, each compressed on its own: tames plosives, sibilants or a boomy low end.",
        ),
        EffectKind::ExpressDelay => tr("A quick echo, fed back by a share of itself."),
        EffectKind::ExpressDistortion => tr("A quick distortion, from warm to mangled."),
        EffectKind::ExpressEqualization => tr("Quick bass, mid and treble."),
        EffectKind::ExpressNoiseGate => {
            tr("Silences the sound while it stays below the threshold.")
        }
        EffectKind::ExpressChorus => {
            tr("A quick chorus: one to three modulated copies of the sound.")
        }
        EffectKind::ExpressFlangeWahWah => tr("A quick flange or wah-wah."),
        EffectKind::ExpressDynamics => tr("A quick compressor, up to a limiter."),
        EffectKind::ExpressStutter => tr("Switches the sound on and off in a fast rhythm."),
        EffectKind::ExpressAmplitudeModulation => {
            tr("The volume rises and falls in a smooth wave (tremolo).")
        }
        EffectKind::ExpressGraphicEq => tr("Ten bands an octave apart, from 28 Hz to 15 kHz."),
        EffectKind::ExpressReverb => tr("A quick reverb: a room, its size and how lively it is."),
        EffectKind::ExpressAudioRestoration => {
            tr("Takes clicks and crackle out and turns the steady hiss of old recordings down.")
        }
        EffectKind::AmplitudeModulation => {
            tr("The volume follows a wave you draw, slow (tremolo) or fast (ring tones).")
        }
        EffectKind::GraphicDynamics => {
            tr("Compression, limiting or gating drawn as an input/output curve.")
        }
        EffectKind::Distortion => {
            tr("Reshapes the wave along curves you draw, each half on its own.")
        }
        EffectKind::GraphicEq => tr("A frequency response you draw, accurate down to the lows."),
        EffectKind::WaveHammer => {
            tr("A compressor and a volume maximizer for mastering, up to 5.1.")
        }
        EffectKind::StudioGate => tr("Lowers the sound below the threshold, smoothly and quickly."),
        EffectKind::DeEsser => tr("Tames the hiss of \"s\" sounds in a voice."),
        EffectKind::StudioLimiter => {
            tr("Makes the sound louder without going over the top, then clips it gently.")
        }
        EffectKind::TremoloPan => tr("A tremolo, or an auto-pan when the sides move apart."),
        EffectKind::TubeStage => tr("The warmth and grit of a tube preamp."),
        EffectKind::StudioCompressor => {
            tr("A musical compressor with parallel mix, soft clip and side-chain filter.")
        }
        EffectKind::Phaser => tr("Sweeping notches of 4, 8 or 12 stages, with feedback."),
        EffectKind::ChorusFlanger => {
            tr("A chorus, flanger or wide ensemble of three voices a side.")
        }
        EffectKind::StudioReverb => tr("A plate, room or hall, with size, time and damping."),
        EffectKind::StereoDelay => {
            tr("Two delays, digital, analog or bucket brigade, in ms or notes of the tempo.")
        }
        EffectKind::TimeStretch | EffectKind::ExpressTimeStretch => {
            tr("Makes the sound longer or shorter without changing its pitch.")
        }
        EffectKind::PitchShift => {
            tr("Raises or lowers the pitch, like a tape or keeping the length.")
        }
        EffectKind::GapperSnipper => tr("Inserts silences into the sound or cuts bits out of it."),
        EffectKind::Vibrato => {
            tr("The pitch wavers along a wave you draw, like a tape speeding up and slowing down.")
        }
        EffectKind::VocalStrip => {
            tr("A voice's chain in one: high-pass, gate, de-esser, compressor and tone.")
        }
        EffectKind::Plugin => tr("An effect from an OFX plug-in."),
        EffectKind::Vst => tr("An audio effect from a VST 3 plug-in."),
    }
}

/// Adds `chain` (an effect, or a filter package's) to the video events of
/// the list; how many took it.
pub fn apply_to_clips(app: &mut BananaApp, clips: &[ClipId], chain: &[Effect]) -> usize {
    let ids = clips.to_vec();
    let chain = chain.to_vec();
    app.modify(|p| {
        let mut n = 0;
        for id in ids {
            let is_video = p.clip_track(id).is_some_and(|t| t.kind == TrackKind::Video);
            if is_video && let Some(c) = p.clip_mut(id) {
                c.effects.extend(chain.iter().cloned());
                n += 1;
            }
        }
        n
    })
}

pub fn apply_to_track(app: &mut BananaApp, track: TrackId, chain: &[Effect]) {
    let chain = chain.to_vec();
    app.modify(|p| {
        if let Some(t) = p.track_by_id_mut(track) {
            t.effects.extend(chain);
        }
    });
}

/// The window's categories, as VEGAS's: BananaV's effects are in
/// Creative, Color, Utility and Blur, and its pack's in Banana Split (by
/// group in its Filter); Filter Packages has the chains saved.
fn categories() -> [Category; 9] {
    let c = |name, ready| Category {
        name,
        ready,
        packages: false,
    };
    [
        // First, where it shows in any width.
        c("Banana Split", true),
        c(tr("AI/ML"), true),
        c(tr("Creative"), true),
        c(tr("Color"), true),
        c(tr("Utility"), true),
        c(tr("Blur"), true),
        c("360°", false),
        // OFX plug-ins, as VEGAS lists other makers'.
        c(
            tr("Third Party"),
            crate::ofx::filters()
                .iter()
                .any(|p| !bv_engine::plugins::is_own(p)),
        ),
        Category {
            name: tr("Filter Packages"),
            ready: true,
            packages: true,
        },
    ]
}

fn category_of(k: EffectKind) -> usize {
    match k {
        EffectKind::Glow | EffectKind::Vignette | EffectKind::Mirror | EffectKind::Pixelate => 2,
        EffectKind::BrightnessContrast
        | EffectKind::ColorGrade
        | EffectKind::Levels
        | EffectKind::BlackAndWhite
        | EffectKind::Sepia
        | EffectKind::Invert => 3,
        EffectKind::GaussianBlur => 5,
        _ => 4,
    }
}

/// The window's index of the Banana Split category (BananaV's own pack).
const BANANA_SPLIT: usize = 0;

/// The window's index of the Third Party category (OFX plug-ins).
const THIRD_PARTY: usize = 7;

/// The window's index of the Filter Packages category.
pub(crate) const FILTER_PACKAGES: usize = 8;

/// The category of one of BananaV's own plug-ins: its group's.
fn own_category(p: &bv_ofx::PluginInfo) -> usize {
    if bv_engine::plugins::is_banana_split(p) {
        return BANANA_SPLIT;
    }
    match p.grouping.rsplit(['/', '\\']).next().unwrap_or("") {
        "AI/ML" | "AI" => 1,
        "Creative" => 2,
        "Color" => 3,
        "Blur" => 5,
        "360°" => 6,
        _ => 4,
    }
}

/// An OFX plug-in's presets: its defaults, BananaV's (for its own) and
/// the user's.
fn plugin_presets(app: &mut BananaApp, p: &bv_ofx::PluginInfo) -> Vec<(String, Effect)> {
    let mut v = vec![(tr("(Default)").to_string(), crate::ofx::new_effect(p))];
    for (name, values) in crate::presets::builtin_plugin(&p.identifier) {
        // One of the pack's that is the defaults: (Default) already.
        if values.is_empty() && bv_engine::plugins::is_banana_split(p) {
            continue;
        }
        let mut e = crate::ofx::new_effect(p);
        if let Some(fx) = e.plugin.as_mut() {
            fx.params = values;
        }
        v.push((name, e));
    }
    for (name, values) in app.clip_windows.presets().user_plugin(&p.identifier) {
        let mut e = crate::ofx::new_effect(p);
        if let Some(fx) = e.plugin.as_mut() {
            fx.params = values.clone();
        }
        v.push((name.clone(), e));
    }
    v
}

/// What the window lists: BananaV's effects (its own plug-ins among
/// them), the other OFX plug-ins, then the filter packages.
fn plugins(ofx: &[std::sync::Arc<bv_ofx::PluginInfo>], packages: &[Package]) -> Vec<Plugin> {
    let cats = categories();
    let own = EffectKind::VIDEO.iter().map(|&k| {
        let name = tr(k.name());
        Plugin {
            key: format!("fx:{k:?}"),
            name: name.to_string(),
            category: category_of(k),
            ready: true,
            about: [
                trf!(
                    "{}: BananaV, 32-bit floating point, GPU accelerated, group {}",
                    name,
                    cats[category_of(k)].name
                ),
                trf!("Description: {}", description(k)),
            ],
            provider: None,
        }
    });
    let theirs = ofx.iter().map(|p| {
        let name = crate::ofx::plugin_name(p);
        if bv_engine::plugins::is_own(p) {
            let category = own_category(p);
            return Plugin {
                key: format!("ofx:{}", p.identifier),
                about: [
                    crate::ofx::own_about(p, &name, cats[category].name),
                    trf!("Description: {}", tr(&p.description)),
                ],
                name,
                category,
                ready: true,
                // The pack's Filter picks by group.
                provider: (category == BANANA_SPLIT).then(|| crate::ofx::group_of(p)),
            };
        }
        Plugin {
            key: format!("ofx:{}", p.identifier),
            about: [
                trf!(
                    "{}: OFX plug-in {}, group {}",
                    name,
                    p.identifier,
                    p.grouping
                ),
                trf!("Description: {}", p.description),
            ],
            name,
            category: THIRD_PARTY,
            ready: true,
            provider: Some(crate::ofx::provider(p)),
        }
    });
    let saved = packages.iter().map(|p| {
        let n = p.chain.len();
        let names: Vec<String> = p.chain.iter().map(crate::ofx::fx_name).collect();
        Plugin {
            key: format!("pkg:{}", p.name),
            name: p.name.clone(),
            category: FILTER_PACKAGES,
            ready: true,
            about: [
                trn!(
                    n,
                    "{1}: filter package of {0} plug-in",
                    "{1}: filter package of {0} plug-ins",
                    n,
                    p.name
                ),
                trf!("Chain: {}", names.join(" → ")),
            ],
            provider: None,
        }
    });
    own.chain(theirs).chain(saved).collect()
}

/// An effect's presets: its defaults, BananaV's and the user's.
fn presets(app: &mut BananaApp, k: EffectKind) -> Vec<(String, Effect)> {
    let mut v = vec![(tr("(Default)").to_string(), Effect::new(k))];
    for (name, pairs) in crate::presets::builtin(k) {
        let mut e = Effect::new(k);
        for (i, value) in pairs {
            if let Some(p) = e.params.get_mut(i) {
                *p = value;
            }
        }
        v.push((name.to_string(), e));
    }
    for (name, params) in app.clip_windows.presets().user(k) {
        let mut e = Effect::new(k);
        for (p, value) in e.params.iter_mut().zip(params) {
            *p = *value;
        }
        v.push((name.clone(), e));
    }
    v
}

/// How many pictures a second an effect that moves by itself plays under
/// the pointer, and how many before it starts over (four seconds).
const MOVING_FPS: f64 = 15.0;
const MOVING_LOOP: u32 = 60;

/// The picture compared with the first to tell whether an effect moves:
/// half a second in.
const PROBE: u32 = 8;

/// The presets' pictures: the sample picture with each effect, made once;
/// and the one under the pointer as its effect moves (noise, a flicker, a
/// bolt), drawn on a thread of its own.
#[derive(Default)]
pub struct Previews {
    map: HashMap<String, TextureHandle>,
    moving: Option<Moving>,
    drawer: Option<Drawer>,
}

/// The pictures of the preset under the pointer, as its effect moves.
struct Moving {
    key: String,
    /// Its pictures drawn, by number (a fifteenth of a second each).
    frames: HashMap<u32, TextureHandle>,
    /// Those asked for and not back yet.
    asked: HashSet<u32>,
    /// The first and the probe's bytes, until it is known whether it moves.
    bytes: HashMap<u32, Vec<u8>>,
    /// Whether it moves: `None` until the probe was compared with the
    /// first.
    moves: Option<bool>,
}

/// A picture to draw, `frame` fifteenths of a second in.
struct Job {
    key: String,
    effects: Vec<Effect>,
    size: (u32, u32),
    frame: u32,
}

/// The thread drawing them: what to draw, what it drew, and the key of the
/// preset that plays (the others' jobs are let go).
struct Drawer {
    jobs: mpsc::Sender<Job>,
    done: mpsc::Receiver<(String, u32, Vec<u8>)>,
    current: Arc<Mutex<String>>,
}

impl Drawer {
    fn new(ctx: &egui::Context) -> Option<Drawer> {
        let (jobs, todo) = mpsc::channel::<Job>();
        let (send, done) = mpsc::channel();
        let current = Arc::new(Mutex::new(String::new()));
        let playing = current.clone();
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("bv-fx-previews".into())
            .spawn(move || {
                while let Ok(job) = todo.recv() {
                    let wanted = playing.lock().is_ok_and(|k| *k == job.key);
                    if !wanted {
                        continue;
                    }
                    let (w, h) = job.size;
                    let seconds = f64::from(job.frame) / MOVING_FPS;
                    let rgba = bv_engine::effect_sample_at(&job.effects, w, h, seconds);
                    if send.send((job.key, job.frame, rgba)).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            })
            .ok()?;
        Some(Drawer {
            jobs,
            done,
            current,
        })
    }
}

impl std::fmt::Debug for Previews {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Previews")
            .field("n", &self.map.len())
            .finish()
    }
}

fn preview_key(effects: &[Effect], w: u32, h: u32) -> String {
    format!("{w}x{h} {effects:?}")
}

impl Previews {
    /// The sample picture, `w`x`h`, with `effects` (none: as it is).
    fn get(&mut self, ctx: &egui::Context, effects: &[Effect], w: u32, h: u32) -> TextureId {
        self.map
            .entry(preview_key(effects, w, h))
            .or_insert_with(|| {
                let rgba = bv_engine::effect_sample(effects, w, h);
                ctx.load_texture(
                    "efeito",
                    ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba),
                    TextureOptions::LINEAR,
                )
            })
            .id()
    }

    /// The picture of `effects` `seconds` after the pointer came onto it,
    /// when they move by themselves and it is drawn yet (else the still
    /// one shows); asks for it and the next ones.
    fn moving(
        &mut self,
        ctx: &egui::Context,
        effects: &[Effect],
        (w, h): (u32, u32),
        seconds: f32,
    ) -> Option<TextureId> {
        if effects.is_empty() {
            return None;
        }
        let key = preview_key(effects, w, h);
        if self.drawer.is_none() {
            self.drawer = Drawer::new(ctx);
        }
        let drawer = self.drawer.as_ref()?;
        if self.moving.as_ref().is_none_or(|m| m.key != key) {
            if let Ok(mut k) = drawer.current.lock() {
                k.clone_from(&key);
            }
            self.moving = Some(Moving {
                key: key.clone(),
                frames: HashMap::new(),
                asked: HashSet::new(),
                bytes: HashMap::new(),
                moves: None,
            });
        }
        let m = self.moving.as_mut()?;
        // What came back.
        while let Ok((k, frame, rgba)) = drawer.done.try_recv() {
            if k != m.key {
                continue;
            }
            m.asked.remove(&frame);
            if m.moves.is_none() && (frame == 0 || frame == PROBE) {
                m.bytes.insert(frame, rgba.clone());
                if let (Some(a), Some(b)) = (m.bytes.get(&0), m.bytes.get(&PROBE)) {
                    m.moves = Some(a != b);
                    m.bytes.clear();
                }
            }
            if rgba.len() == (w * h * 4) as usize {
                let image = ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                let texture = ctx.load_texture("efeito-movendo", image, TextureOptions::LINEAR);
                m.frames.insert(frame, texture);
            }
        }
        let ask = |m: &mut Moving, frame: u32| {
            if !m.frames.contains_key(&frame) && m.asked.insert(frame) {
                let job = Job {
                    key: key.clone(),
                    effects: effects.to_vec(),
                    size: (w, h),
                    frame,
                };
                let _ = drawer.jobs.send(job);
            }
        };
        match m.moves {
            // Still: its one picture.
            Some(false) => None,
            None => {
                ask(m, 0);
                ask(m, PROBE);
                None
            }
            Some(true) => {
                let now = ((f64::from(seconds) * MOVING_FPS) as u32) % MOVING_LOOP;
                for next in now..now + 3 {
                    ask(m, next % MOVING_LOOP);
                }
                // The latest drawn up to now.
                (0..=now)
                    .rev()
                    .find_map(|f| m.frames.get(&f))
                    .map(TextureHandle::id)
            }
        }
    }
}

/// A preset's picture in `rect`: the sample picture through `effects`;
/// while it plays, they sweep across it, moving when they move by
/// themselves.
fn paint_preset(
    painter: &egui::Painter,
    rect: Rect,
    previews: &mut Previews,
    effects: &[Effect],
    play: Option<f32>,
) {
    let ctx = painter.ctx().clone();
    let ppp = ctx.pixels_per_point();
    let (w, h) = (
        (rect.width() * ppp).round().max(1.0) as u32,
        (rect.height() * ppp).round().max(1.0) as u32,
    );
    let with = previews.get(&ctx, effects, w, h);
    let all = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    match play {
        None => {
            painter.image(with, rect, all, Color32::WHITE);
        }
        Some(t) => {
            let with = previews.moving(&ctx, effects, (w, h), t).unwrap_or(with);
            // The effect sweeps in from the left over the squares (what it
            // makes see-through shows them), the picture as it was on the
            // right.
            let k = chooser::progress(t);
            let plain = previews.get(&ctx, &[], w, h);
            let split = rect.left() + rect.width() * k;
            painter.image(
                plain,
                Rect::from_min_max(pos2(split, rect.top()), rect.max),
                Rect::from_min_max(pos2(k, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            painter.image(
                with,
                Rect::from_min_max(rect.min, pos2(split, rect.bottom())),
                Rect::from_min_max(pos2(0.0, 0.0), pos2(k, 1.0)),
                Color32::WHITE,
            );
            if k < 1.0 {
                painter.vline(split, rect.y_range(), Stroke::new(1.0, Color32::WHITE));
            }
        }
    }
}

/// Applies `chain` (a preset's effect, or a filter package's) to the
/// selected video events; from the preset's + (`edit`) the event's FX
/// window then opens, where it can be changed and saved as a preset of its
/// own. Properties opens by itself never, only from View › Window.
fn apply_selected(app: &mut BananaApp, chain: &[Effect], edit: bool) {
    let sel = app.selected();
    if apply_to_clips(app, &sel, chain) == 0 {
        app.notify(tr("Select a video clip to apply the effect."));
    } else if edit {
        let video = sel.into_iter().find(|&c| {
            app.project()
                .clip_track(c)
                .is_some_and(|t| t.kind == TrackKind::Video)
        });
        app.clip_windows.clip_fx_last = video;
    }
}

/// The track of the event picked first, when it is a video one (a
/// preset's Apply to Track).
fn picked_video_track(app: &BananaApp) -> Option<TrackId> {
    app.primary_clip()
        .and_then(|c| app.project().clip_track(c))
        .filter(|t| t.kind == TrackKind::Video)
        .map(|t| t.id)
}

/// A filter package's one preset: the sample picture through its chain.
/// Dragged onto an event, double-clicked or its + clicked, the chain goes
/// to the events; its menu renames or deletes it.
fn package_grid(
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    state: &mut chooser::ChooserState,
    grid: Rect,
    package: &Package,
) {
    let names = vec![package.name.clone()];
    let previews = &mut app.fx_previews;
    let cells = chooser::preset_grid(
        ui,
        grid,
        "video-fx",
        state,
        &names,
        tr("Add to the selected events and edit"),
        |painter, rect, _, play| paint_preset(painter, rect, previews, &package.chain, play),
    );
    let chain = &package.chain;
    for cell in cells {
        let resp = cell.resp;
        if resp.drag_started() {
            app.media_drag = Some(MediaDrag::Package(package.name.clone(), chain.clone()));
        }
        if cell.double {
            apply_selected(app, chain, false);
        }
        if cell.add {
            apply_selected(app, chain, true);
        }
        resp.context_menu(|ui| {
            if ui.button(tr("Apply to Selected Events")).clicked() {
                apply_selected(app, chain, false);
                ui.close();
            }
            let track = picked_video_track(app);
            if ui
                .add_enabled(track.is_some(), egui::Button::new(tr("Apply to Track")))
                .clicked()
                && let Some(t) = track
            {
                apply_to_track(app, t, chain);
                ui.close();
            }
            ui.separator();
            if ui.button(tr("Rename…")).clicked() {
                app.fx_packages.ask_rename(&package.name, false);
                ui.close();
            }
            if ui.button(tr("Delete")).clicked() {
                crate::fx_packages::ask_delete(app, &package.name, false);
                ui.close();
            }
        });
    }
}

pub fn ui(ui: &mut egui::Ui, app: &mut BananaApp) {
    let mut state = std::mem::take(&mut app.choosers.effects);
    state.keys = app.browse.take(crate::dock::Tab::VideoFx);
    let categories = categories();
    let ofx = crate::ofx::filters();
    // The packages are read when their category or the favorites show.
    let packages = if state.category == 1 || state.category == FILTER_PACKAGES + 2 {
        app.fx_packages.list(false)
    } else {
        Vec::new()
    };
    let plugins = plugins(&ofx, &packages);
    let frame = chooser::frame(
        ui,
        "video-fx",
        &mut state,
        &mut app.settings.favorites,
        &categories,
        &plugins,
        Icon::Fx,
    );
    if frame.favorites_changed {
        let _ = app.settings.save();
    }
    if frame.refresh {
        app.fx_packages.refresh();
    }
    if frame.explore {
        crate::fx_packages::explore(app);
    }
    let first_package = EffectKind::VIDEO.len() + ofx.len();
    if let Some(p) = frame
        .plugin
        .and_then(|i| i.checked_sub(first_package))
        .and_then(|i| packages.get(i))
    {
        if frame.dragged {
            app.media_drag = Some(MediaDrag::Package(p.name.clone(), p.chain.clone()));
        }
        package_grid(ui, app, &mut state, frame.grid, p);
    } else if let Some(i) = frame.plugin {
        let presets = match EffectKind::VIDEO.get(i) {
            Some(&k) => presets(app, k),
            None => ofx
                .get(i - EffectKind::VIDEO.len())
                .map(|p| plugin_presets(app, p))
                .unwrap_or_default(),
        };
        // Dragged from the list: its (Default) preset.
        if frame.dragged
            && let Some((_, effect)) = presets.first()
        {
            app.media_drag = Some(MediaDrag::Effect(effect.clone()));
        }
        let names: Vec<String> = presets.iter().map(|(n, _)| n.clone()).collect();
        let previews = &mut app.fx_previews;
        let cells = chooser::preset_grid(
            ui,
            frame.grid,
            "video-fx",
            &mut state,
            &names,
            tr("Add to the selected events and edit"),
            |painter, rect, i, play| {
                paint_preset(
                    painter,
                    rect,
                    previews,
                    std::slice::from_ref(&presets[i].1),
                    play,
                );
            },
        );
        for cell in cells {
            let effect = &presets[cell.i].1;
            let resp = cell.resp;
            if resp.drag_started() {
                app.media_drag = Some(MediaDrag::Effect(effect.clone()));
            }
            let one = std::slice::from_ref(effect);
            if cell.double {
                apply_selected(app, one, false);
            }
            if cell.add {
                apply_selected(app, one, true);
            }
            resp.context_menu(|ui| {
                if ui.button(tr("Apply to Selected Events")).clicked() {
                    apply_selected(app, one, false);
                    ui.close();
                }
                let track = picked_video_track(app);
                if ui
                    .add_enabled(track.is_some(), egui::Button::new(tr("Apply to Track")))
                    .clicked()
                    && let Some(t) = track
                {
                    apply_to_track(app, t, one);
                    ui.close();
                }
            });
        }
    } else if state.category == FILTER_PACKAGES + 2 {
        // None saved yet: how one is made.
        let room = frame.grid.shrink2(egui::vec2(16.0, 14.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(room), |ui| {
            crate::tabs::hint(
                ui,
                tr("No filter packages yet. An FX window's Save Plug-In Chain As Filter Package keeps its chain here, to apply to other events, tracks, media or the video output; Refresh finds packages copied into the folder."),
            );
        });
    }
    app.choosers.effects = state;
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    /// While Video FX has the keys, a name's letters pick a plug-in, Up and
    /// Down the next ones, Left and Right its presets.
    #[test]
    fn letters_and_arrows_pick_effects_and_presets() {
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut app = BananaApp::new(crate::egui::Context::default());
        app.welcome_for_review("closed");
        app.fx_packages_for_review();
        app.show_tab(crate::dock::Tab::VideoFx, false);
        app.keys_tab = Some(crate::dock::Tab::VideoFx);
        let mut h = Harness::builder()
            .with_size(crate::egui::vec2(1600.0, 960.0))
            .build_ui_state(
                |ui, app: &mut BananaApp| {
                    crate::apply_theme(ui.ctx());
                    app.frame(ui);
                },
                app,
            );
        h.run_steps(4);
        let chosen = |h: &Harness<'_, BananaApp>| h.state().choosers.effects.plugin.clone();
        for c in "sepia".chars() {
            h.event(crate::egui::Event::Text(c.to_string()));
        }
        h.step();
        let sepia = chosen(&h).expect("chosen");
        assert!(sepia.to_lowercase().contains("sepia"), "{sepia}");
        h.key_press(crate::egui::Key::ArrowDown);
        h.step();
        let next = chosen(&h).expect("chosen");
        assert_ne!(next, sepia);
        h.key_press(crate::egui::Key::ArrowUp);
        h.step();
        assert_eq!(chosen(&h), Some(sepia));
        h.key_press(crate::egui::Key::ArrowRight);
        h.step();
        assert_eq!(h.state().choosers.effects.preset, Some(0));
        h.key_press(crate::egui::Key::ArrowRight);
        h.step();
        assert_eq!(h.state().choosers.effects.preset, Some(1));
    }

    /// An effect dragged out of the plug-in list onto an event lands on it
    /// with its (Default) preset, as one dragged from the presets does.
    #[test]
    fn an_effect_dragged_from_the_list_lands_on_the_event_with_its_default() {
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut app = BananaApp::new(crate::egui::Context::default());
        app.welcome_for_review("closed");
        app.fx_packages_for_review();
        app.load_demo();
        app.choosers.effects.search = Some("Sepia".into());
        app.show_tab(crate::dock::Tab::VideoFx, false);
        let mut h = Harness::builder()
            .with_size(crate::egui::vec2(1600.0, 960.0))
            .build_ui_state(
                |ui, app: &mut BananaApp| {
                    crate::apply_theme(ui.ctx());
                    app.frame(ui);
                },
                app,
            );
        h.run_steps(4);
        let gradient = h.state().project().tracks[0].clips[1].id;
        let before = h
            .state()
            .project()
            .clip(gradient)
            .expect("event")
            .effects
            .len();
        let from = h.get_by_label("Sepia").rect().center();
        let to = h.state().timeline_spot_for_review(0, 7.0).expect("event");
        h.event(crate::egui::Event::PointerMoved(from));
        h.step();
        let button = |h: &mut Harness<'_, BananaApp>, pos, pressed| {
            h.event(crate::egui::Event::PointerButton {
                pos,
                button: crate::egui::PointerButton::Primary,
                pressed,
                modifiers: crate::egui::Modifiers::NONE,
            });
            h.step();
        };
        button(&mut h, from, true);
        for k in 1..=10 {
            h.event(crate::egui::Event::PointerMoved(
                from + (to - from) * (k as f32 / 10.0),
            ));
            h.step();
        }
        assert!(
            matches!(h.state().media_drag, Some(MediaDrag::Effect(_))),
            "dragged"
        );
        button(&mut h, to, false);
        h.run_steps(2);
        let clip = h.state().project().clip(gradient).expect("event").clone();
        assert_eq!(clip.effects.len(), before + 1, "landed on it");
        let info = crate::ofx::filters()
            .into_iter()
            .find(|p| p.label == "Sepia")
            .expect("Sepia");
        let default = crate::ofx::new_effect(&info);
        let landed = clip.effects.last().and_then(|e| e.plugin.as_deref());
        assert_eq!(
            landed.map(|f| &f.id),
            default.plugin.as_deref().map(|f| &f.id)
        );
        assert_eq!(
            landed.map(|f| &f.params),
            default.plugin.as_deref().map(|f| &f.params),
            "its (Default) preset"
        );
    }
}
