//! Janela de FX no estilo do VEGAS: no topo, a cadeia de plug-ins (o
//! Pan/Crop e os efeitos, ligados da entrada à saída) e a linha de
//! predefinições; embaixo, o editor do nó escolhido. O do Pan/Crop tem as
//! ferramentas à esquerda, as propriedades em seções recolhíveis, a área de
//! trabalho com o quadro "F" sobre a mídia e a linha de quadros-chave.
//! Também o seletor de plug-ins (pastas, catálogo e a cadeia sendo montada).

use crate::i18n::{tr, trc};
use bv_core::{FrameRate, Tick};
use bv_timeline::{
    Clip, ClipId, Effect, EffectKind, KeyCurve, MediaId, MediaInfo, Project, TrackId, TrackKind,
    Transform,
};

use crate::app::BananaApp;
use crate::cursors::{self, Cursor};
use crate::egui::{
    self, Align, Align2, Color32, FontId, Layout, Mesh, Painter, PointerButton, Pos2, Rect,
    RichText, Sense, Shape, Stroke, StrokeKind, Vec2, WidgetInfo, WidgetType, pos2, vec2,
};
use crate::fx_packages::Package;
use crate::icons::{self, Icon};
use crate::pancrop::{self, Viewport};
use crate::pancrop_keys::KeyRow;
use crate::presets::{self, Presets};
use crate::tabs::properties;
use crate::theme;
use bv_timeline::pancrop::{self as framing, PanCropKey, PlaceIn};

/// A área de trabalho do Pan/Crop é clara e pontilhada.
pub(crate) const WS_BG: Color32 = Color32::from_gray(236);
/// VEGAS's grid over the workspace: the frame repeated in lines, a dot at
/// each 16th of it.
pub(crate) const WS_DOT: Color32 = Color32::from_gray(40);
pub(crate) const WS_LINE: Color32 = Color32::from_gray(176);

/// The letter F's outline in its own box (0–1 each way), as VEGAS's: it
/// shows the box's turn and flips.
const F_SHAPE: [(f32, f32); 10] = [
    (0.0, 0.0),
    (1.0, 0.0),
    (1.0, 0.123),
    (0.193, 0.123),
    (0.193, 0.432),
    (0.89, 0.432),
    (0.89, 0.543),
    (0.193, 0.543),
    (0.193, 1.0),
    (0.0, 1.0),
];

/// Alças do F: cantos e meios das bordas.
const HANDLES: [(f32, f32); 8] = [
    (-1.0, -1.0),
    (0.0, -1.0),
    (1.0, -1.0),
    (1.0, 0.0),
    (1.0, 1.0),
    (0.0, 1.0),
    (-1.0, 1.0),
    (-1.0, 0.0),
];

// ---- alvo da cadeia ---------------------------------------------------------

/// De quem é a cadeia de efeitos.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FxTarget {
    Clip(ClipId),
    /// Mídia; `true` = cadeia de áudio.
    Media(MediaId, bool),
    Track(TrackId),
    /// Effects on the finished video frame (Tools › Video › Video Output FX).
    VideoOutput,
    /// An audio bus's effects (Bus FX), an assignable effects chain's, the
    /// Master bus's (Master FX).
    Bus(bv_timeline::BusId),
    Chain(bv_timeline::FxChainId),
    /// An input bus's effects (Input Bus FX).
    InputBus(bv_timeline::InputBusId),
    Master,
    /// The chain Insert › Audio Assignable FX makes once its effects are
    /// picked.
    NewChain,
    /// The transition an event comes in with (`true`: the one it goes out
    /// with): its plug-in, alone in the chain, as VEGAS's Video Event FX
    /// shows a transition.
    Transition(ClipId, bool),
}

impl FxTarget {
    /// Whose envelopes its chain's FX automation is in: a track's, a
    /// bus's or a chain's.
    pub fn envelope_owner(self) -> Option<bv_timeline::EnvelopeOwner> {
        use bv_timeline::{EnvelopeOwner, Strip};
        match self {
            FxTarget::Track(id) => Some(EnvelopeOwner::Track(id)),
            FxTarget::Bus(id) => Some(EnvelopeOwner::Strip(Strip::Bus(id))),
            FxTarget::Chain(id) => Some(EnvelopeOwner::Strip(Strip::Chain(id))),
            _ => None,
        }
    }

    pub fn audio(self, p: &Project) -> bool {
        match self {
            FxTarget::Clip(id) => p.clip_track(id).is_some_and(|t| t.kind == TrackKind::Audio),
            FxTarget::Media(_, audio) => audio,
            FxTarget::Track(id) => p.track(id).is_some_and(|t| t.kind == TrackKind::Audio),
            FxTarget::VideoOutput | FxTarget::Transition(..) => false,
            FxTarget::Bus(_)
            | FxTarget::Chain(_)
            | FxTarget::InputBus(_)
            | FxTarget::Master
            | FxTarget::NewChain => true,
        }
    }

    /// A cadeia atual (`None` se o alvo não existe mais).
    pub fn chain(self, p: &Project) -> Option<Vec<Effect>> {
        match self {
            FxTarget::Clip(id) => p.clip(id).map(|c| c.effects.clone()),
            FxTarget::Media(id, audio) => p.media(id).map(|m| {
                m.effects
                    .iter()
                    .filter(|e| e.kind.is_audio() == audio)
                    .cloned()
                    .collect()
            }),
            FxTarget::Track(id) => p.track(id).map(|t| t.effects.clone()),
            FxTarget::VideoOutput => Some(p.video_output_fx.clone()),
            FxTarget::Bus(id) => p.bus(id).map(|b| b.channel.effects.clone()),
            FxTarget::Chain(id) => p.fx_chain(id).map(|c| c.channel.effects.clone()),
            FxTarget::InputBus(id) => p.input_bus(id).map(|b| b.effects.clone()),
            FxTarget::Master => Some(p.master_fx.clone()),
            FxTarget::NewChain => Some(Vec::new()),
            // Only while it has a plug-in (taken out, the window closes).
            FxTarget::Transition(id, out) => p.clip(id).and_then(|c| {
                let (fx, off) = c.transition_plugin(out);
                let mut e = Effect::plugin(fx?.clone());
                e.enabled = !off;
                Some(vec![e])
            }),
        }
    }

    pub fn set_chain(self, p: &mut Project, chain: Vec<Effect>) {
        self.put_chain(p, chain);
        // An effect taken out takes its automation with it.
        p.prune_fx_automation();
    }

    fn put_chain(self, p: &mut Project, chain: Vec<Effect>) {
        match self {
            FxTarget::Clip(id) => {
                if let Some(c) = p.clip_mut(id) {
                    c.effects = chain;
                }
            }
            FxTarget::Media(id, audio) => {
                if let Some(m) = p.media.iter_mut().find(|m| m.id == id) {
                    // A mídia guarda vídeo e áudio juntos: vídeo primeiro.
                    let other: Vec<_> = m
                        .effects
                        .iter()
                        .filter(|e| e.kind.is_audio() != audio)
                        .cloned()
                        .collect();
                    m.effects = if audio {
                        other.into_iter().chain(chain).collect()
                    } else {
                        chain.into_iter().chain(other).collect()
                    };
                }
            }
            FxTarget::Track(id) => {
                if let Some(t) = p.track_by_id_mut(id) {
                    t.effects = chain;
                }
            }
            FxTarget::VideoOutput => p.video_output_fx = chain,
            FxTarget::Bus(id) => {
                if let Some(b) = p.bus_mut(id) {
                    b.channel.effects = chain;
                }
            }
            FxTarget::Chain(id) => {
                if let Some(c) = p.fx_chain_mut(id) {
                    c.channel.effects = chain;
                }
            }
            FxTarget::InputBus(id) => {
                if let Some(b) = p.input_bus_mut(id) {
                    b.effects = chain;
                }
            }
            FxTarget::Master => p.master_fx = chain,
            FxTarget::NewChain => {
                p.add_fx_chain(chain);
            }
            FxTarget::Transition(id, out) => {
                if let Some(c) = p.clip_mut(id) {
                    let first = chain.into_iter().find(|e| e.plugin.is_some());
                    let off = first.as_ref().is_some_and(|e| !e.enabled);
                    // Taken out: a plain crossfade (or fade), as VEGAS's.
                    c.set_transition_plugin(out, first.and_then(|e| e.plugin.map(|b| *b)), off);
                }
            }
        }
    }

    /// O Pan/Crop entra na cadeia só nos clipes de vídeo.
    pub fn has_pan_crop(self, p: &Project) -> bool {
        matches!(self, FxTarget::Clip(_)) && !self.audio(p)
    }

    /// The button among the chain's `len` effects that is no effect, and
    /// its place: a video event's Pan/Crop (`Clip::pan_crop_at`), a video
    /// track's Composite (`Track::composite_at`; an adjustment track has
    /// none: its effects work on what is below it).
    fn fixed(self, p: &Project, len: usize) -> Option<(Fixed, usize)> {
        match self {
            FxTarget::Clip(id) if !self.audio(p) => {
                p.clip(id).map(|c| (Fixed::PanCrop, c.pan_crop_at.min(len)))
            }
            FxTarget::Track(id) => p
                .track(id)
                .filter(|t| t.kind == TrackKind::Video && !t.adjustment)
                .map(|t| (Fixed::Composite, t.composite_at.map_or(len, |k| k.min(len)))),
            _ => None,
        }
    }

    /// Puts that button at `at` among the chain's `len` effects.
    fn set_fixed_at(self, p: &mut Project, at: usize, len: usize) {
        match self {
            FxTarget::Clip(id) => {
                if let Some(c) = p.clip_mut(id) {
                    c.pan_crop_at = at;
                }
            }
            FxTarget::Track(id) => {
                if let Some(t) = p.track_by_id_mut(id) {
                    // Last, VEGAS's default: it stays last as effects are
                    // added.
                    t.composite_at = (at < len).then_some(at);
                }
            }
            _ => {}
        }
    }

    /// "Video Event FX", "Audio Track FX"…
    pub fn title(self, p: &Project) -> String {
        let audio = self.audio(p);
        tr(match self {
            FxTarget::Clip(_) if audio => "Audio Event FX",
            FxTarget::Clip(_) | FxTarget::Transition(..) => "Video Event FX",
            FxTarget::Media(..) if audio => "Audio Media FX",
            FxTarget::Media(..) => "Video Media FX",
            FxTarget::Track(_) if audio => "Audio Track FX",
            FxTarget::Track(_) => "Video Track FX",
            FxTarget::VideoOutput => "Video Output FX",
            FxTarget::Bus(_) => "Bus FX",
            FxTarget::InputBus(_) => "Input Bus FX",
            FxTarget::Chain(_) | FxTarget::NewChain => "Assignable FX",
            FxTarget::Master => "Master FX",
        })
        .into()
    }

    fn name(self, p: &Project) -> String {
        match self {
            FxTarget::Clip(id) => p
                .clip(id)
                .and_then(|c| p.media(c.media))
                .map_or_else(|| tr("clip").into(), crate::i18n::media_name),
            FxTarget::Media(id, _) => p
                .media(id)
                .map_or_else(|| tr("media").into(), crate::i18n::media_name),
            FxTarget::Track(id) => p
                .track(id)
                .map_or_else(|| tr("track").into(), crate::i18n::track_name),
            FxTarget::VideoOutput => tr("video output").into(),
            FxTarget::Bus(id) => crate::i18n::bus_name(p, id),
            FxTarget::Chain(id) => crate::i18n::chain_name(p, id),
            FxTarget::InputBus(id) => crate::i18n::input_bus_name(p, id),
            FxTarget::Master => tr("Master").into(),
            FxTarget::NewChain => trf!("FX {}", p.fx_chains.len() + 1),
            FxTarget::Transition(id, out) => p
                .clip(id)
                .and_then(|c| c.transition_plugin(out).0)
                .map_or_else(|| tr("Transition").into(), |fx| tr(&fx.name).to_string()),
        }
    }
}

// ---- estado da janela -------------------------------------------------------

/// Nó selecionado na cadeia.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Node {
    PanCrop,
    Effect(usize),
}

/// A button of the chain that is no effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fixed {
    /// A video event's Pan/Crop: the effects before it work on the
    /// media's own picture.
    PanCrop,
    /// A video track's Composite: the effects after it work on its
    /// compositing group's picture.
    Composite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tool {
    Normal,
    Zoom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axis {
    Free,
    X,
    Y,
}

#[derive(Clone, Copy, Debug)]
enum Drag {
    Move {
        vp: Viewport,
        from: Pos2,
    },
    Resize {
        vp: Viewport,
        sx: f32,
        sy: f32,
    },
    Rotate {
        vp: Viewport,
        from: f32,
    },
    /// The small circle where the box turns: only that moves.
    Pivot {
        at: (f32, f32),
        from: Pos2,
    },
    View {
        offset: Vec2,
        from: Pos2,
    },
}

impl Drag {
    /// The pointer while it goes on: the one its press showed (the box
    /// mirrored `flips`), the view dragged the closed hand.
    fn cursor(&self, flips: (bool, bool)) -> Cursor {
        match *self {
            Drag::Move { .. } => Cursor::MoveFrame,
            Drag::Resize { vp, sx, sy } => box_handle_cursor(sx, sy, vp.angle_deg, flips),
            Drag::Rotate { .. } => Cursor::Rotate,
            Drag::Pivot { .. } => Cursor::MoveCenter,
            Drag::View { .. } => Cursor::HandClosed,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Hit {
    Handle(f32, f32),
    Rotate,
    Inside,
    Outside,
}

#[derive(Clone, Copy, Debug)]
enum MenuAction {
    Reset,
    Center,
    FlipH,
    FlipV,
    MatchOutput,
    MatchSource,
    FitAdjustment,
}

#[derive(Debug)]
struct PanEditor {
    tool: Tool,
    snap: bool,
    /// The grid's spacing: the frame's width and height parted in so many
    /// (VEGAS's dots: 16).
    grid: f32,
    lock_aspect: bool,
    from_center: bool,
    axis: Axis,
    /// Zoom da área de trabalho (1 = mídia encaixada com folga).
    zoom: f32,
    offset: Vec2,
    drag: Option<Drag>,
    /// The box (the F) picked by a click or a drag on it: the arrow keys
    /// move it, until a click off it.
    picked: bool,
    /// Seções abertas: Posição, Rotação, Interpolação, Origem, Espaço.
    sections: [bool; 5],
    /// Posição na linha de quadros-chave, a partir do início do clipe.
    pos: Tick,
    /// Acompanha o cursor da timeline.
    sync: bool,
    /// Geometria do último desenho da área de trabalho.
    view: Option<(Space, Viewport)>,
    /// The keyframe controller's row picked: Position (the framing) or
    /// Mask (VEGAS 2026's masks: the tools, properties and workspace are
    /// theirs).
    row: KeyRow,
    mask: crate::pancrop_mask::MaskEditor,
    /// The keyframe controller under the workspace.
    keys: crate::pancrop_keys::KeyStrip,
    /// The media's picture the workspace shows.
    source: Option<ShownMedia>,
}

/// How far the arrow keys pressed move the Pan/Crop's box, in its media's
/// pixels: one a press, ten with Shift, along the axes `axis` leaves free.
/// The keys are taken (the timeline's cursor stays).
fn arrow_nudge(i: &mut egui::InputState, axis: Axis) -> (f32, f32) {
    let (mut dx, mut dy) = (0.0f32, 0.0f32);
    for (key, x, y) in [
        (egui::Key::ArrowLeft, -1.0, 0.0),
        (egui::Key::ArrowRight, 1.0, 0.0),
        (egui::Key::ArrowUp, 0.0, -1.0),
        (egui::Key::ArrowDown, 0.0, 1.0),
    ] {
        // Shift's first: the plain arrow's would take them too.
        for (mods, step) in [(egui::Modifiers::SHIFT, 10.0), (egui::Modifiers::NONE, 1.0)] {
            let n = i.count_and_consume_key(mods, key) as f32;
            dx += x * step * n;
            dy += y * step * n;
        }
    }
    match axis {
        Axis::X => (dx, 0.0),
        Axis::Y => (0.0, dy),
        Axis::Free => (dx, dy),
    }
}

/// The media's picture the Pan/Crop workspace shows: the engine's picture
/// it came from, its event, and the texture.
struct ShownMedia {
    /// The picture's pixels, held so that no other picture is made where
    /// they are while they show.
    pixels: std::sync::Arc<Vec<u8>>,
    clip: ClipId,
    texture: egui::TextureHandle,
}

impl std::fmt::Debug for ShownMedia {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShownMedia")
            .field("clip", &self.clip)
            .finish_non_exhaustive()
    }
}

impl Default for PanEditor {
    fn default() -> PanEditor {
        PanEditor {
            tool: Tool::Normal,
            snap: false,
            grid: 16.0,
            lock_aspect: true,
            from_center: true,
            axis: Axis::Free,
            zoom: 1.0,
            offset: Vec2::ZERO,
            drag: None,
            picked: false,
            sections: [true; 5],
            pos: Tick::ZERO,
            sync: true,
            view: None,
            row: KeyRow::Position,
            mask: crate::pancrop_mask::MaskEditor::default(),
            keys: crate::pancrop_keys::KeyStrip::default(),
            source: None,
        }
    }
}

/// Mapeamento entre pixels da mídia e a tela da área de trabalho.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Space {
    pub center: Pos2,
    pub k: f32,
    pub src: (f32, f32),
}

impl Space {
    pub(crate) fn screen(&self, p: (f32, f32)) -> Pos2 {
        pos2(
            self.center.x + (p.0 - self.src.0 / 2.0) * self.k,
            self.center.y + (p.1 - self.src.1 / 2.0) * self.k,
        )
    }

    /// [`Self::screen`] of a point in the numbers masks keep.
    pub(crate) fn screen64(&self, p: (f64, f64)) -> Pos2 {
        self.screen((p.0 as f32, p.1 as f32))
    }

    /// [`Self::media`] in the numbers masks keep.
    pub(crate) fn media64(&self, s: Pos2) -> (f64, f64) {
        let (x, y) = self.media(s);
        (f64::from(x), f64::from(y))
    }

    pub(crate) fn media(&self, s: Pos2) -> (f32, f32) {
        (
            (s.x - self.center.x) / self.k + self.src.0 / 2.0,
            (s.y - self.center.y) / self.k + self.src.1 / 2.0,
        )
    }
}

impl PanEditor {
    fn space(&self, rect: Rect, k0: f32, src: (f32, f32)) -> Space {
        Space {
            center: rect.center() + self.offset,
            k: k0 * self.zoom,
            src,
        }
    }

    /// Zoom da vista mantendo parado o ponto da mídia sob `p`.
    fn zoom_at(&mut self, rect: Rect, k0: f32, src: (f32, f32), p: Pos2, factor: f32) {
        let m = self.space(rect, k0, src).media(p);
        self.zoom = (self.zoom * factor).clamp(0.05, 40.0);
        let k = k0 * self.zoom;
        let center = pos2(p.x - (m.0 - src.0 / 2.0) * k, p.y - (m.1 - src.1 / 2.0) * k);
        self.offset = center - rect.center();
    }
}

#[derive(Clone, Debug, PartialEq)]
enum PresetValue {
    Params(Vec<f32>),
    Pan(Transform),
    /// A Pan/Crop's box of its own shape (a framing keeps the frame's).
    PanBox(PanCropKey),
    /// An OFX plug-in's values.
    Plugin(crate::presets::PluginValues),
    /// A VST 3 effect's values and states.
    Vst(crate::presets::VstPreset),
    /// A VST 3 plug-in's program (a preset it holds), by its place.
    VstProgram(usize),
}

/// Whose presets the preset row shows.
#[derive(Clone, Debug, PartialEq)]
enum PresetOwner {
    PanCrop,
    Effect(EffectKind),
    /// An OFX plug-in, by identifier.
    Plugin(String),
    /// A VST 3 effect, by `presets::vst_key`.
    Vst(String),
}

/// Tudo que o editor de Pan/Crop precisa saber do clipe neste quadro.
struct PanCtx<'a> {
    clip: &'a Clip,
    info: Option<&'a MediaInfo>,
    /// Instante editado (na timeline).
    t: Tick,
    /// The box there, and what its place depends on.
    key: PanCropKey,
    place: PlaceIn,
    vp: Viewport,
    src: (f32, f32),
}

/// What the FX window takes its size from: the chain, the node shown and
/// the size a plug-in's page takes (measured).
type Fit = (Vec<EffectKind>, Node, Option<(i32, i32)>);

#[derive(Debug)]
pub struct FxWindow {
    pub target: FxTarget,
    pub node: Node,
    /// Predefinição aplicada por último e os valores dela.
    preset: Option<(String, PresetValue)>,
    /// Nome sendo digitado para salvar uma predefinição.
    saving: Option<String>,
    fx_open: bool,
    pan: PanEditor,
    /// Cada clique começa uma nova entrada de desfazer.
    gesture: u64,
    /// What the window was sized for last.
    fitted: Option<Fit>,
    /// The size a plug-in's page takes (measured last frame).
    natural: Option<Vec2>,
    /// How far the chain's row is scrolled (last frame), where its scroll
    /// buttons send it next frame, and the button it last brought into
    /// view with the chain's length then.
    chain_offset: f32,
    chain_to: Option<f32>,
    chain_seen: Option<(Node, usize)>,
    /// The page showed a VST plug-in's own editor inside the window (last
    /// frame): the window takes its size.
    vst_embedded: bool,
}

impl FxWindow {
    pub fn new(target: FxTarget, node: Node) -> FxWindow {
        FxWindow {
            fitted: None,
            natural: None,
            chain_offset: 0.0,
            chain_to: None,
            chain_seen: None,
            vst_embedded: false,
            target,
            node,
            preset: None,
            saving: None,
            fx_open: true,
            pan: PanEditor::default(),
            gesture: 0,
        }
    }

    /// The size its plug-in page takes (measured last frame).
    /// The Pan/Crop editor's Mask row picked, with mask `path` picked and
    /// its anchor `anchor`, its box showing when `boxed` (screen and
    /// interface tests).
    pub(crate) fn masks_for_review(
        &mut self,
        path: Option<usize>,
        anchor: Option<usize>,
        boxed: bool,
    ) {
        self.node = Node::PanCrop;
        self.pan.row = KeyRow::Mask;
        self.pan.mask.pick_for_review(path, anchor, boxed);
    }

    /// The Pan/Crop editor's mask tool (interface tests).
    #[cfg(test)]
    pub(crate) fn mask_tool_for_review(&mut self, tool: crate::pancrop_mask::MaskTool) {
        self.pan.mask.tool = tool;
    }

    pub fn natural(&self) -> Option<Vec2> {
        self.natural
    }

    /// Where the Pan/Crop workspace drew its box's point (`sx`, `sy` from
    /// −1 to 1; 0, 0 the middle).
    pub(crate) fn pan_spot(&self, sx: f32, sy: f32) -> Option<Pos2> {
        let (sp, vp) = self.pan.view?;
        Some(box_point(&sp, &vp, sx, sy))
    }

    /// Where the keyframe controller drew keyframe `i` of the Position row
    /// (the Mask row's, with `mask`), for screen tests.
    pub(crate) fn pan_key_spot(&self, mask: bool, i: usize) -> Option<Pos2> {
        let row = if mask { KeyRow::Mask } else { KeyRow::Position };
        self.pan
            .keys
            .spots
            .iter()
            .filter(|((r, _), _)| *r == row)
            .nth(i)
            .map(|(_, at)| *at)
    }

    /// A point `f` (0–1) of the way across the Position row (the Mask
    /// row's, with `mask`) of the keyframe controller, for screen tests.
    /// Where the Pan/Crop workspace drew the media's point (`fx`, `fy`
    /// parts of its width and height).
    pub(crate) fn pan_media_spot(&self, fx: f32, fy: f32) -> Option<Pos2> {
        let (sp, _) = self.pan.view?;
        Some(sp.screen((fx * sp.src.0, fy * sp.src.1)))
    }

    pub(crate) fn pan_lane_spot(&self, mask: bool, f: f32) -> Option<Pos2> {
        let row = if mask { KeyRow::Mask } else { KeyRow::Position };
        let (_, r) = self.pan.keys.rows_drawn.iter().find(|(x, _)| *x == row)?;
        Some(pos2(r.left() + f * r.width(), r.center().y))
    }

    pub fn select(&mut self, node: Node) {
        if self.node != node {
            self.node = node;
            self.preset = None;
            self.saving = None;
        }
    }

    /// Desenha a janela. Devolve `false` quando ela foi fechada.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        app: &mut BananaApp,
        presets: &mut Presets,
        chooser: &mut Option<Chooser>,
    ) -> bool {
        let project = app.project().clone();
        let Some(chain) = self.target.chain(&project) else {
            return false;
        };
        let pan = self.target.has_pan_crop(&project);
        self.node = match self.node {
            Node::PanCrop if pan => Node::PanCrop,
            Node::Effect(i) if i < chain.len() => Node::Effect(i),
            _ if pan => Node::PanCrop,
            _ => Node::Effect(chain.len().saturating_sub(1)),
        };
        if ctx.input(|i| i.pointer.any_pressed()) {
            self.gesture += 1;
        }
        // The window takes the size of its chain and of the effect shown,
        // as these change (Options › Preferences › General: "Enable
        // Automatic FX Window Resizing"); a plug-in's page, the size it
        // takes, as VEGAS's: it grows when its keyframe controller opens.
        let embedded = self.vst_embedded;
        let plugin_page = matches!(self.node, Node::Effect(i)
            if chain.get(i).is_some_and(|e| e.plugin.is_some() || embedded && e.vst.is_some()));
        let natural = self
            .natural
            .filter(|_| plugin_page)
            .map(|n| (n.x.ceil() as i32, n.y.ceil() as i32));
        // A few pixels of rounding never resize it: only what the page
        // shows does (and the user's own size stays until then).
        let natural = match (natural, self.fitted.as_ref().and_then(|f| f.2)) {
            (Some(n), Some(old)) if (n.0 - old.0).abs() <= 3 && (n.1 - old.1).abs() <= 3 => {
                Some(old)
            }
            (n, _) => n,
        };
        let fit = (
            chain.iter().map(|e| e.kind).collect::<Vec<_>>(),
            self.node,
            natural,
        );
        if app.settings.prefs.fx_window_resize && self.fitted.as_ref() != Some(&fit) {
            let nodes =
                chain.len() + usize::from(self.target.fixed(&project, chain.len()).is_some());
            let size = match (self.node, natural) {
                (Node::PanCrop, _) => egui::vec2(940.0, 640.0),
                (Node::Effect(_), Some((w, h))) => egui::vec2(w as f32 + 2.0, h as f32 + 2.0),
                // The Modern Equalizer's graph and knobs.
                (Node::Effect(i), None)
                    if chain.get(i).is_some_and(|e| e.kind == EffectKind::ModernEq) =>
                {
                    egui::vec2(940.0, 760.0)
                }
                // A VST 3 plug-in's rows.
                (Node::Effect(i), None) if chain.get(i).is_some_and(|e| e.vst.is_some()) => {
                    egui::vec2(760.0, 560.0)
                }
                (Node::Effect(i), None) => {
                    let params = chain.get(i).map_or(0, |e| e.kind.params().len());
                    egui::vec2(
                        (330.0 + 130.0 * nodes as f32).clamp(660.0, 1400.0),
                        (300.0 + 34.0 * params as f32).clamp(440.0, 900.0),
                    )
                }
            };
            crate::floating::resize(ctx, egui::Id::new("janela-fx"), size);
        }
        self.fitted = Some(fit);
        let title = self.target.title(&project);
        let mut open = true;
        let shown = crate::floating::Window::new(title.clone())
            .id(egui::Id::new("janela-fx"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(if plugin_page {
                [480.0, 380.0]
            } else {
                [940.0, 640.0]
            })
            .min_size([360.0, 220.0])
            .show(ctx, |ui| {
                let top = ui.cursor().top();
                let name = self.target.name(&project);
                let header = match (self.node, self.target) {
                    (Node::PanCrop, _) => trf!("Event Pan/Crop: {}", name),
                    (_, FxTarget::Transition(..)) => trf!("Transition: {}", name),
                    (Node::Effect(_), _) => format!("{title}: {name}"),
                };
                // The name, and the Columns buttons on the right (a
                // plug-in's settings in one, two or three columns).
                let mut width = ui
                    .horizontal(|ui| {
                        let name = ui.label(RichText::new(header).strong().size(14.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            for n in (1..=3u8).rev() {
                                let on = app.settings.fx_columns.clamp(1, 3) == n;
                                if columns_button(ui, n, on, plugin_page).clicked() {
                                    app.settings.fx_columns = n;
                                }
                            }
                        });
                        name.rect.width() + 3.0 * 26.0 + 24.0
                    })
                    .inner;
                width = width.max(self.chain_row(ui, app, &chain, chooser));
                width = width.max(self.preset_row(ui, app, &project, &chain, presets));
                ui.separator();
                let page_top = ui.cursor().top();
                let page = match self.node {
                    Node::PanCrop => {
                        self.pan_crop(ui, app, &project);
                        None
                    }
                    Node::Effect(i) => self.effect_editor(ui, app, &chain, i, chooser),
                };
                // The other keys, while this window (an OS window of its
                // own) has them, are the program's, as VEGAS's: Space plays,
                // Ctrl+Z undoes… The clipboard's, Delete and Ctrl+arrows
                // stay the page's (its keyframes').
                if !ui.ctx().embed_viewports()
                    && ui.input(|i| i.focused)
                    && !ui.ctx().egui_wants_keyboard_input()
                {
                    ui.input_mut(|i| {
                        i.events.retain(|e| match e {
                            egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_) => false,
                            egui::Event::Key { key, modifiers, .. } => !matches!(
                                (key, modifiers.command),
                                (egui::Key::V | egui::Key::Delete, _)
                                    | (egui::Key::ArrowLeft | egui::Key::ArrowRight, true)
                            ),
                            _ => true,
                        })
                    });
                    crate::keymap::dispatch(app, ui.ctx());
                }
                page.map(|p| vec2(width.max(p.x), page_top - top + p.y))
            });
        if let Some(Some(natural)) = shown {
            self.natural = Some(natural);
        }
        open
    }

    // ---- cadeia ---------------------------------------------------------------

    /// The chain's row; the width its nodes and buttons take. An event's
    /// Pan/Crop button stands where its chain has it (`Clip::pan_crop_at`:
    /// the effects before it work on the media's own picture), a video
    /// track's Composite button too (`Track::composite_at`: those after it
    /// work on its compositing group's picture); any button is dragged to
    /// another place, or moved by its menu. The chain stops before the
    /// buttons at the row's end: when it does not fit, it scrolls between
    /// two scroll buttons, as VEGAS 23's, and by the mouse wheel and a
    /// button dragged past its ends; the button picked comes into view.
    fn chain_row(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut BananaApp,
        chain: &[Effect],
        chooser: &mut Option<Chooser>,
    ) -> f32 {
        let mut edit: Option<(Vec<Effect>, usize, Node)> = None;
        let fixed = self.target.fixed(app.project(), chain.len());
        let composite = fixed.is_some_and(|(f, _)| f == Fixed::Composite);
        let at = fixed.map_or(0, |(_, at)| at);
        // The buttons in order: `None` the Pan/Crop or the Composite,
        // `Some(i)` effect `i`.
        let mut seq: Vec<Option<usize>> = (0..chain.len()).map(Some).collect();
        if fixed.is_some() {
            seq.insert(at, None);
        }
        let names: Vec<String> = seq
            .iter()
            .map(|node| match *node {
                None if composite => tr("Composite").to_string(),
                None => tr("Pan/Crop").to_string(),
                Some(i) => match chain[i].vst.as_deref() {
                    Some(v) => vst_node_name(&crate::ofx::fx_name(&chain[i]), v),
                    None => crate::ofx::fx_name(&chain[i]),
                },
            })
            .collect();
        // Each button's left and right along the chain, from its input's
        // dot to its output's: how long the chain is.
        let mut spans = Vec::with_capacity(seq.len());
        let mut x = CHAIN_END;
        for (node, name) in seq.iter().zip(&names) {
            x += CHAIN_LINK;
            let w = chain_node_width(ui, name, node.is_some());
            spans.push((x, x + w));
            x += w;
        }
        let full = x + CHAIN_LINK + CHAIN_END;
        let after_remove = |c: &[Effect], i: usize| {
            if i > 0 {
                Node::Effect(i - 1)
            } else if c.is_empty() {
                Node::PanCrop
            } else {
                Node::Effect(0)
            }
        };
        let mut moved: Option<(usize, usize)> = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            // The vertical wheel scrolls it too.
            ui.style_mut().always_scroll_the_only_direction = true;
            let room = (ui.available_width() - CHAIN_TOOLS).max(0.0);
            let over = full > room;
            let shown = if over {
                (room - 2.0 * CHAIN_ARROW).max(0.0)
            } else {
                full
            };
            let end = (full - shown).max(0.0);
            // A pass that only measures the window keeps the scroll as it
            // is: it lays the row out in another width.
            let sizing = ui.is_sizing_pass();
            // A button newly picked (or the chain changed): into view.
            let mut to = if sizing { None } else { self.chain_to.take() };
            if !sizing && self.chain_seen != Some((self.node, seq.len())) {
                self.chain_seen = Some((self.node, seq.len()));
                let picked = seq.iter().position(|n| match *n {
                    None => !composite && self.node == Node::PanCrop,
                    Some(i) => self.node == Node::Effect(i),
                });
                if let Some(k) = picked {
                    let from = to.unwrap_or(self.chain_offset);
                    to = Some(chain_reveal(spans[k], from, shown, full));
                }
            }
            let offset = to.unwrap_or(self.chain_offset).clamp(0.0, end);
            let back = over && chain_arrow(ui, false, offset > 0.5, tr("Scroll Left")).clicked();
            let area = egui::ScrollArea::horizontal()
                .id_salt("fx-chain")
                .max_width(shown)
                .min_scrolled_width(0.0)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .scroll_source(egui::scroll_area::ScrollSource::MOUSE_WHEEL)
                .horizontal_scroll_offset(offset);
            let out = area.show(ui, |ui| {
                chain_end(ui);
                let mut rects = Vec::with_capacity(seq.len());
                let (mut dragged, mut stopped) = (None, None);
                for (k, node) in seq.iter().enumerate() {
                    chain_link(ui);
                    let name = &names[k];
                    let r = match *node {
                        None if composite => chain_node(ui, name, None, false)
                            .on_hover_text(tr(
                                "Effects before this button work on the track's own picture; those after it on its compositing group's picture, once the track is composited into it.",
                            )),
                        None => {
                            let r = chain_node(ui, name, None, self.node == Node::PanCrop);
                            if r.clicked() {
                                self.select(Node::PanCrop);
                            }
                            r
                        }
                        Some(i) => {
                            let e = &chain[i];
                            let mut on = e.enabled;
                            let r =
                                chain_node(ui, name, Some(&mut on), self.node == Node::Effect(i));
                            if on != e.enabled {
                                let mut c = chain.to_vec();
                                c[i].enabled = on;
                                edit = Some((c, at, Node::Effect(i)));
                            }
                            if r.clicked() {
                                self.select(Node::Effect(i));
                            }
                            r
                        }
                    };
                    rects.push(r.rect);
                    if r.dragged() {
                        dragged = Some(k);
                    }
                    if r.drag_stopped() {
                        stopped = Some(k);
                    }
                    let first = k == 0;
                    let last = k + 1 == seq.len();
                    r.context_menu(|ui| {
                        if ui
                            .add_enabled(!first, egui::Button::new(tr("Move Left")))
                            .clicked()
                        {
                            moved = Some((k, k - 1));
                            ui.close();
                        } else if ui
                            .add_enabled(!last, egui::Button::new(tr("Move Right")))
                            .clicked()
                        {
                            moved = Some((k, k + 2));
                            ui.close();
                        }
                        let Some(i) = *node else {
                            return;
                        };
                        let e = &chain[i];
                        let mut c = chain.to_vec();
                        if ui
                            .button(if e.enabled {
                                tr("Disable")
                            } else {
                                tr("Enable")
                            })
                            .clicked()
                        {
                            c[i].enabled = !e.enabled;
                            edit = Some((c, at, Node::Effect(i)));
                            ui.close();
                        } else if ui.button(tr("Remove")).clicked() {
                            c.remove(i);
                            let node = after_remove(&c, i);
                            let at = if i < at { at - 1 } else { at };
                            edit = Some((c, at, node));
                            ui.close();
                        }
                    });
                }
                // A button let go: where it goes, among all of them.
                if let (Some(k), Some(p)) = (stopped, ui.input(|i| i.pointer.latest_pos())) {
                    moved = Some((k, drop_slot(&rects, p.x)));
                }
                // A button being dragged: where it would go.
                if let (Some(_), Some(p)) = (dragged, ui.input(|i| i.pointer.latest_pos())) {
                    let slot = drop_slot(&rects, p.x);
                    let x = match rects.get(slot) {
                        Some(r) => r.left() - 3.0,
                        None => rects.last().map_or(p.x, |r| r.right() + 3.0),
                    };
                    if let Some(r) = rects.first() {
                        ui.painter().vline(
                            x,
                            r.y_range().expand(3.0),
                            Stroke::new(2.0, theme::banana()),
                        );
                    }
                }
                chain_link(ui);
                chain_end(ui);
                dragged.is_some()
            });
            let offset = out.state.offset.x;
            if !sizing {
                self.chain_offset = offset;
            }
            let ahead =
                over && chain_arrow(ui, true, offset < end - 0.5, tr("Scroll Right")).clicked();
            if back || ahead {
                self.chain_to = Some(chain_step(&spans, offset, shown, full, ahead));
                ui.ctx().request_repaint();
            }
            // A button dragged past the chain's ends scrolls it there.
            if over
                && out.inner
                && let Some(p) = ui.input(|i| i.pointer.latest_pos())
            {
                let step = CHAIN_DRAG_SCROLL * ui.input(|i| i.stable_dt).min(0.1);
                let view = out.inner_rect;
                let to = if p.x < view.left() {
                    offset - step
                } else if p.x > view.right() {
                    offset + step
                } else {
                    offset
                };
                if to != offset {
                    self.chain_to = Some(to.clamp(0.0, end));
                    ui.ctx().request_repaint();
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let sel = match self.node {
                    Node::Effect(i) if i < chain.len() => Some(i),
                    _ => None,
                };
                if icon_button(
                    ui,
                    Icon::Delete,
                    sel.is_some(),
                    tr("Remove Selected Plug-In"),
                )
                .clicked()
                    && let Some(i) = sel
                {
                    let mut c = chain.to_vec();
                    c.remove(i);
                    let node = after_remove(&c, i);
                    let at = if i < at { at - 1 } else { at };
                    edit = Some((c, at, node));
                }
                // A transition's: another from the Transitions window.
                let transition = matches!(self.target, FxTarget::Transition(..));
                if icon_button(
                    ui,
                    Icon::Save,
                    !chain.is_empty() && !transition,
                    tr("Save Plug-In Chain As Filter Package"),
                )
                .clicked()
                {
                    let audio = self.target.audio(app.project());
                    app.fx_packages.ask_save(chain.to_vec(), audio);
                }
                if icon_button(ui, Icon::Fx, true, tr("Plug-In Chain…")).clicked() {
                    if transition {
                        app.show_tab(crate::dock::Tab::Transitions, false);
                    } else {
                        *chooser = Some(Chooser::new(self.target, chain.to_vec()));
                    }
                }
            });
        });
        // A button moved: the chain in its new order, the Pan/Crop's or
        // the Composite's place.
        if let Some((from, to)) = moved
            && let Some((c, at, node)) = reorder(chain, &seq, from, to)
        {
            edit = Some((c, at, node));
        }
        if let Some((c, at, node)) = edit {
            let t = self.target;
            app.modify(|p| {
                let len = c.len();
                t.set_chain(p, c);
                t.set_fixed_at(p, at, len);
            });
            // The Composite button shows nothing: what was shown stays.
            if !(composite && node == Node::PanCrop) {
                self.select(node);
            }
        }
        // All of it shown: the window fits it when it fits its page.
        full + CHAIN_TOOLS
    }

    // ---- predefinições --------------------------------------------------------

    /// The preset's row; the width it takes.
    fn preset_row(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut BananaApp,
        project: &Project,
        chain: &[Effect],
        presets: &mut Presets,
    ) -> f32 {
        let mut pan_box = None;
        let (current, owner, is_default) = match self.node {
            Node::PanCrop => {
                let FxTarget::Clip(id) = self.target else {
                    return 0.0;
                };
                let Some(c) = project.clip(id) else {
                    return 0.0;
                };
                let Some(place) = project.clip_place_in(id) else {
                    return 0.0;
                };
                let key = c.box_at(c.start + self.pan.pos, place.media, place.media_aspect);
                let tf = framing::transform_from_key(&key, &place);
                let whole = key.is_whole(place.media);
                pan_box = Some(key);
                (PresetValue::Pan(tf), PresetOwner::PanCrop, whole)
            }
            Node::Effect(i) => match chain.get(i) {
                // A plug-in at its defaults has no values of its own.
                Some(Effect {
                    plugin: Some(fx), ..
                }) => (
                    PresetValue::Plugin(fx.params.clone()),
                    PresetOwner::Plugin(fx.id.clone()),
                    fx.params.is_empty(),
                ),
                // A VST 3 effect: its values and states (never "at its
                // defaults": the plug-in alone knows them).
                Some(Effect { vst: Some(v), .. }) => (
                    PresetValue::Vst(crate::presets::VstPreset::of(v)),
                    PresetOwner::Vst(crate::presets::vst_key(v)),
                    false,
                ),
                Some(e) => (
                    PresetValue::Params(params_of(e)),
                    PresetOwner::Effect(e.kind),
                    e.is_default(),
                ),
                None => return 0.0,
            },
        };
        let same_box = |v: &PresetValue| match (v, &pan_box) {
            (PresetValue::PanBox(a), Some(b)) => {
                let close = |x: f64, y: f64| (x - y).abs() < 1e-6;
                close(a.width, b.width)
                    && close(a.height, b.height)
                    && close(a.center[0], b.center[0])
                    && close(a.center[1], b.center[1])
                    && close(a.angle, b.angle)
            }
            _ => false,
        };
        // A VST 3 plug-in's programs, as its page knows them.
        let (programs, program) = match (&owner, self.node) {
            (PresetOwner::Vst(_), Node::Effect(i)) => app.vst.programs(self.target, i),
            _ => (Vec::new(), None),
        };
        let shown = match &self.preset {
            Some((name, v)) if *v == current || same_box(v) => name.clone(),
            _ if is_default => tr("(Default)").to_string(),
            // A plug-in's preset, however it came (a Video FX tile): its
            // name.
            _ => match (&owner, &current) {
                (PresetOwner::Plugin(id), PresetValue::Plugin(v)) => presets::builtin_plugin(id)
                    .into_iter()
                    .chain(presets.user_plugin(id).iter().cloned())
                    .find(|(_, p)| p == v)
                    .map_or_else(|| tr("(Custom)").to_string(), |(n, _)| n),
                // A preset of the user's, else the plug-in's program.
                (PresetOwner::Vst(k), PresetValue::Vst(v)) => presets
                    .user_vst(k)
                    .iter()
                    .find(|(_, p)| p == v)
                    .map(|(n, _)| n.clone())
                    .or_else(|| program.and_then(|n| programs.get(n).cloned()))
                    .unwrap_or_else(|| tr("(Custom)").to_string()),
                _ => tr("(Custom)").to_string(),
            },
        };
        let user_names: Vec<String> = match &owner {
            PresetOwner::PanCrop => presets
                .user_pan_crop()
                .iter()
                .map(|(n, _)| n.clone())
                .collect(),
            PresetOwner::Effect(k) => presets.user(*k).iter().map(|(n, _)| n.clone()).collect(),
            PresetOwner::Plugin(id) => presets
                .user_plugin(id)
                .iter()
                .map(|(n, _)| n.clone())
                .collect(),
            PresetOwner::Vst(k) => presets.user_vst(k).iter().map(|(n, _)| n.clone()).collect(),
        };
        let is_user = user_names.contains(&shown);
        let mut pick: Option<(String, PresetValue)> = None;
        let mut vst_file: Option<bv_timeline::VstFx> = None;
        let row = ui.horizontal(|ui| {
            ui.label(tr("Preset:"));
            // Tall enough for a dozen presets, as VEGAS's list.
            egui::ComboBox::from_id_salt("fx-predefinicao")
                .width(300.0)
                .height(420.0)
                .selected_text(&shown)
                .show_ui(ui, |ui| {
                    let default = match &owner {
                        PresetOwner::PanCrop => PresetValue::Pan(Transform::default()),
                        PresetOwner::Effect(k) => PresetValue::Params(params_of(&Effect::new(*k))),
                        PresetOwner::Plugin(_) => PresetValue::Plugin(Default::default()),
                        PresetOwner::Vst(_) => PresetValue::Vst(Default::default()),
                    };
                    // A VST 3 effect's defaults are the plug-in's own.
                    if !matches!(owner, PresetOwner::Vst(_))
                        && ui
                            .selectable_label(shown == tr("(Default)"), tr("(Default)"))
                            .clicked()
                    {
                        pick = Some((tr("(Default)").into(), default));
                    }
                    let mut items: Vec<(String, PresetValue)> = match &owner {
                        PresetOwner::PanCrop => {
                            // The shapes first: as large as fits in the media.
                            let shapes = match self.target {
                                FxTarget::Clip(id) => project.clip_place_in(id),
                                _ => None,
                            };
                            let shapes = shapes.iter().flat_map(|place| {
                                presets::PAN_CROP_SHAPES.iter().map(move |&(name, shape)| {
                                    let r = shape / place.media_aspect.max(1e-6);
                                    let key = PanCropKey::whole(place.media).matched(r);
                                    (tr(name).to_string(), PresetValue::PanBox(key))
                                })
                            });
                            let rest = presets::builtin_pan_crop()
                                .into_iter()
                                .map(|(n, tf)| (n.to_string(), PresetValue::Pan(tf)));
                            shapes.chain(rest).collect()
                        }
                        PresetOwner::Effect(k) => presets::builtin(*k)
                            .into_iter()
                            .map(|(n, pairs)| {
                                let mut e = Effect::new(*k);
                                for (j, v) in pairs {
                                    e.set_param(j, v);
                                }
                                (n.to_string(), PresetValue::Params(params_of(&e)))
                            })
                            .collect(),
                        PresetOwner::Plugin(id) => presets::builtin_plugin(id)
                            .into_iter()
                            .map(|(n, v)| (n, PresetValue::Plugin(v)))
                            .collect(),
                        // The plug-in's programs.
                        PresetOwner::Vst(_) => programs
                            .iter()
                            .enumerate()
                            .map(|(n, name)| (name.clone(), PresetValue::VstProgram(n)))
                            .collect(),
                    };
                    let builtin_count = items.len();
                    match &owner {
                        PresetOwner::PanCrop => items.extend(
                            presets
                                .user_pan_crop()
                                .iter()
                                .map(|(n, tf)| (n.clone(), PresetValue::Pan(*tf))),
                        ),
                        PresetOwner::Effect(k) => items.extend(
                            presets
                                .user(*k)
                                .iter()
                                .map(|(n, v)| (n.clone(), PresetValue::Params(v.clone()))),
                        ),
                        PresetOwner::Plugin(id) => items.extend(
                            presets
                                .user_plugin(id)
                                .iter()
                                .map(|(n, v)| (n.clone(), PresetValue::Plugin(v.clone()))),
                        ),
                        PresetOwner::Vst(k) => items.extend(
                            presets
                                .user_vst(k)
                                .iter()
                                .map(|(n, v)| (n.clone(), PresetValue::Vst(v.clone()))),
                        ),
                    }
                    for (n, (name, value)) in items.into_iter().enumerate() {
                        if n == builtin_count && n > 0 {
                            ui.separator();
                        }
                        if ui.selectable_label(shown == name, &name).clicked() {
                            pick = Some((name, value));
                        }
                    }
                });
            // A VST plug-in's preset and bank files, as VEGAS's bar has
            // them for one (its programs in the list).
            if let (PresetOwner::Vst(_), Node::Effect(i)) = (&owner, self.node)
                && let Some(v) = chain.get(i).and_then(|e| e.vst.as_deref())
            {
                use crate::vst_ui::VstFile;
                let banks = app.vst.keeps_banks(self.target, i);
                let files = [
                    (Icon::Open, VstFile::OpenPreset, tr("Open VST Preset"), true),
                    (
                        Icon::Save,
                        VstFile::SavePreset,
                        tr("Save VST Preset As"),
                        true,
                    ),
                    (
                        Icon::OpenBank,
                        VstFile::OpenBank,
                        tr("Open VST Bank"),
                        banks,
                    ),
                    (
                        Icon::SaveBank,
                        VstFile::SaveBank,
                        tr("Save VST Bank As"),
                        banks,
                    ),
                ];
                for (icon, what, tip, on) in files {
                    let r = icon_button(ui, icon, on, tip);
                    let r = if on {
                        r
                    } else {
                        r.on_disabled_hover_text(tr("A VST 3 plug-in keeps no banks."))
                    };
                    if r.clicked() {
                        let (new, note) = app.vst.file(self.target, i, v, what);
                        if let Some(new) = new {
                            vst_file = Some(new);
                        }
                        if let Some(note) = note {
                            app.notify(note);
                        }
                    }
                }
                return;
            }
            let save = icon_button(
                ui,
                Icon::Save,
                true,
                tr("Save the current settings as a preset"),
            );
            if save.clicked() {
                self.saving = Some(if shown.starts_with('(') {
                    String::new()
                } else {
                    shown.clone()
                });
            }
            if icon_button(ui, Icon::Delete, is_user, tr("Delete this preset")).clicked() {
                let r = match &owner {
                    PresetOwner::PanCrop => presets.remove_pan_crop(&shown),
                    PresetOwner::Effect(k) => presets.remove(*k, &shown),
                    PresetOwner::Plugin(id) => presets.remove_plugin(id, &shown),
                    PresetOwner::Vst(k) => presets.remove_vst(k, &shown),
                };
                if let Err(e) = r {
                    app.report_error(trf!("Could not delete the preset: {}", e));
                }
                self.preset = None;
            }
        });
        if let Some(mut name) = self.saving.take() {
            let (mut done, mut cancel) = (false, false);
            ui.horizontal(|ui| {
                ui.label(tr("Name:"));
                let r = ui.add(
                    egui::TextEdit::singleline(&mut name)
                        .desired_width(260.0)
                        .hint_text(tr("My preset")),
                );
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    done = true;
                }
                done |= ui.button(tr("Save")).clicked();
                cancel = ui.button(tr("Cancel")).clicked();
            });
            let n = name.trim().to_string();
            if done && !n.is_empty() && !n.starts_with('(') {
                let r = match (&current, &owner) {
                    (PresetValue::Pan(tf), _) => presets.add_pan_crop(n.clone(), *tf),
                    (PresetValue::Params(v), PresetOwner::Effect(k)) => {
                        presets.add(*k, n.clone(), v.clone())
                    }
                    (PresetValue::Plugin(v), PresetOwner::Plugin(id)) => {
                        presets.add_plugin(id, n.clone(), v.clone())
                    }
                    (PresetValue::Vst(v), PresetOwner::Vst(k)) => {
                        presets.add_vst(k, n.clone(), v.clone())
                    }
                    _ => Ok(()),
                };
                match r {
                    Ok(()) => {
                        app.notify(trf!("Preset \"{}\" saved.", n));
                        self.preset = Some((n, current.clone()));
                    }
                    Err(e) => app.report_error(trf!("Could not save the preset: {}", e)),
                }
            } else if !cancel {
                self.saving = Some(name);
            }
        }
        if let Some((name, value)) = pick {
            self.apply_preset(app, project, chain, &value);
            self.preset = Some((name, value));
        }
        // A VST preset or bank file opened into the plug-in: the effect
        // takes its settings.
        if let Some(v) = vst_file {
            let value = PresetValue::Vst(crate::presets::VstPreset::of(&v));
            self.apply_preset(app, project, chain, &value);
            self.preset = None;
        }
        row.response.rect.width()
    }

    fn apply_preset(
        &mut self,
        app: &mut BananaApp,
        project: &Project,
        chain: &[Effect],
        value: &PresetValue,
    ) {
        match (self.node, value) {
            (Node::PanCrop, PresetValue::Pan(tf)) => {
                if let FxTarget::Clip(id) = self.target
                    && let Some(c) = project.clip(id)
                {
                    let (t, tf) = (c.start + self.pan.pos, *tf);
                    let new = app.settings.prefs.new_motion_keys();
                    if let Some(place) = project.clip_place_in(id) {
                        let key = framing::key_from_transform(&tf, &place);
                        app.edit(|p| p.set_clip_box_at(id, t, key, new));
                    }
                }
            }
            (Node::PanCrop, PresetValue::PanBox(key)) => {
                if let FxTarget::Clip(id) = self.target
                    && let Some(c) = project.clip(id)
                {
                    let (t, key) = (c.start + self.pan.pos, *key);
                    let new = app.settings.prefs.new_motion_keys();
                    app.edit(|p| p.set_clip_box_at(id, t, key, new));
                }
            }
            (Node::Effect(i), PresetValue::Params(v)) => {
                let mut c = chain.to_vec();
                if let Some(e) = c.get_mut(i) {
                    for (j, x) in v.iter().enumerate() {
                        e.set_param(j, *x);
                    }
                    let t = self.target;
                    app.modify(|p| t.set_chain(p, c));
                }
            }
            (Node::Effect(i), PresetValue::Plugin(v)) => {
                let mut c = chain.to_vec();
                if let Some(fx) = c.get_mut(i).and_then(|e| e.plugin.as_mut()) {
                    fx.params = v.clone();
                    let t = self.target;
                    app.modify(|p| t.set_chain(p, c));
                }
            }
            // A VST 3 plug-in's program: chosen on its page's instance,
            // whose state the effect takes (the mixer's follows it).
            (Node::Effect(i), PresetValue::VstProgram(n)) => {
                let mut c = chain.to_vec();
                if let Some(fx) = c.get_mut(i).and_then(|e| e.vst.as_deref_mut())
                    && app.vst.choose_program(self.target, i, *n, fx)
                {
                    let t = self.target;
                    app.modify(|p| t.set_chain(p, c));
                }
            }
            // A VST 3 effect takes the preset's states and values (the
            // mixer's instance and the page's follow them).
            (Node::Effect(i), PresetValue::Vst(v)) => {
                let mut c = chain.to_vec();
                if let Some(fx) = c.get_mut(i).and_then(|e| e.vst.as_mut()) {
                    fx.params = v.params.clone();
                    fx.state = v.state.clone();
                    fx.controller_state = v.controller_state.clone();
                    let t = self.target;
                    app.modify(|p| t.set_chain(p, c));
                }
            }
            _ => {}
        }
    }

    // ---- efeito ---------------------------------------------------------------

    /// The effect's page; a plug-in's, the size it takes (the window fits
    /// it).
    fn effect_editor(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut BananaApp,
        chain: &[Effect],
        i: usize,
        chooser: &mut Option<Chooser>,
    ) -> Option<Vec2> {
        let Some(e0) = chain.get(i) else {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(tr("There are no plug-ins in the chain."))
                        .color(theme::text_dim()),
                );
                if ui.button(tr("Plug-In Chain…")).clicked() {
                    *chooser = Some(Chooser::new(self.target, Vec::new()));
                }
            });
            return None;
        };
        let mut e = e0.clone();
        let target = self.target;
        let mut size = None;
        // FX automation: automated parameters show their envelopes' values
        // at the cursor, and a change sets the envelope's point there
        // (Bypass FX Automation leaves the envelopes out).
        let owner = target.envelope_owner();
        let automated = owner
            .filter(|_| bv_timeline::fx_automation::automatable(e0))
            .map_or_else(Vec::new, |o| app.project().fx_automation(o, i));
        let live = !e0.automation_bypassed && !automated.is_empty();
        if live
            && let Some((fx, envs)) = owner.and_then(|o| app.project().fx_chain_and_envelopes(o))
            && let Some(now) = bv_timeline::fx_automation::automated(fx, envs, app.cursor).get(i)
        {
            e.params = now.params.clone();
            e.enabled = now.enabled;
        }
        let shown = e.clone();
        match e.plugin.clone() {
            // An OFX plug-in, as VEGAS shows one: its bar and its rows,
            // and its keyframe controller at the window's foot.
            Some(fx) => {
                // A transition's plug-in works in OpenFX's Transition
                // context, over the transition's time.
                let owner = match target {
                    FxTarget::Transition(id, out) => crate::ofx::EditorOwner::Transition(id, out),
                    _ => crate::ofx::EditorOwner::Fx(target),
                };
                let key = crate::ofx::EditorKey {
                    owner,
                    index: i,
                    id: fx.id.clone(),
                };
                let cursor = app.ofx.cursor_of(&key).unwrap_or(app.cursor);
                let place = crate::ofx::place_of(target, app.project(), cursor);
                let columns = app.settings.fx_columns.clamp(1, 3) as usize;
                // The settings; under them, when it shows, the bar that sets
                // the keyframe controller's height (the settings give it
                // room, scrolling) and the controller at the window's foot.
                let tray = app.ofx.keyframes_height(&key, &fx);
                // VEGAS's Animate button at the window's foot, for a
                // plug-in that animates as a whole (Color Curves) while it
                // has no keyframes: the controller shows once it has.
                let footer = if tray == 0.0 && crate::ofx::animate_button(&app.ofx, &key, &fx) {
                    FOOTER
                } else {
                    0.0
                };
                let avail = ui.available_height();
                let most = (avail - SPLITTER - MIN_SETTINGS).max(crate::ofx_keys::MIN_HEIGHT);
                let tray_h = tray.min(most);
                let room = if tray > 0.0 {
                    avail - tray_h - SPLITTER
                } else {
                    avail - footer
                };
                let mut fx = *fx;
                let rows = egui::ScrollArea::vertical()
                    .id_salt("fx-parametros")
                    .max_height(room.max(MIN_SETTINGS))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        if let Some(changed) =
                            crate::ofx::params(ui, &mut app.ofx, key.clone(), &fx, &place, columns)
                        {
                            fx = changed;
                        }
                    });
                let mut keyed = None;
                let mut seek = None;
                if footer > 0.0 {
                    let spare = ui.available_height() - footer;
                    if spare > 0.0 {
                        ui.add_space(spare);
                    }
                    if animate_footer(ui) {
                        fx = crate::ofx::animate_whole(&app.ofx, &key, &fx, &place);
                    }
                }
                if tray > 0.0 {
                    let spare = ui.available_height() - tray_h - SPLITTER;
                    if spare > 0.0 {
                        ui.add_space(spare);
                    }
                    if let Some(h) = splitter(ui, tray_h, most) {
                        app.ofx.set_keyframes_height(&key, h);
                    }
                    (keyed, seek) =
                        crate::ofx::keyframes(ui, &mut app.ofx, &key, &fx, &place, tray_h);
                }
                let width = if tray > 0.0 {
                    rows.content_size.x.max(crate::ofx_keys::MIN_WIDTH)
                } else {
                    rows.content_size.x
                };
                // The rows' whole height (what shows of them is at most the
                // window's) and, under them, the controller as it opens:
                // nothing that depends on the window's own size or on the
                // bar between them, or fitting it would change what it fits.
                let gap = ui.spacing().item_spacing.y;
                let keys = if tray > 0.0 {
                    gap + SPLITTER + crate::ofx_keys::HEIGHT
                } else if footer > 0.0 {
                    gap + footer
                } else {
                    0.0
                };
                size = Some(vec2(width, rows.content_size.y + keys));
                if let Some(k) = keyed {
                    fx = k;
                }
                if e.plugin.as_deref() != Some(&fx) {
                    e.plugin = Some(Box::new(fx));
                }
                if let Some(t) = seek {
                    app.seek(t);
                }
            }
            None => {
                let rate = app.project().settings.sample_rate.0 as f32;
                // A VST plug-in with an editor of its own: the editor alone,
                // inside this window, as VEGAS shows it (the rows where it
                // has none, or Preferences ask for the generic editor).
                let generic = app.settings.prefs.vst_generic_editor;
                let title = self.target.title(app.project());
                let room = if e.vst.is_some() && !generic && bv_platform::EMBEDDED_EDITORS {
                    crate::vst_ui::embedded(
                        ui,
                        &mut app.vst,
                        f64::from(rate),
                        target,
                        i,
                        &mut e,
                        &title,
                    )
                } else {
                    None
                };
                self.vst_embedded = room.is_some();
                if let Some(room) = room {
                    size = Some(room);
                } else {
                    egui::ScrollArea::vertical()
                    .id_salt("fx-parametros")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            crate::widgets::checkbox(ui, &mut e.enabled, tr("Active"));
                            if ui.button(tr("Reset to Defaults")).clicked() {
                                e = e.reset();
                            }
                            if !automated.is_empty() {
                                crate::widgets::checkbox(
                                    ui,
                                    &mut e.automation_bypassed,
                                    tr("Bypass FX Automation"),
                                )
                                .on_hover_text(tr(
                                    "Its automation envelopes are left out: its own values hold.",
                                ));
                            }
                        });
                        ui.add_space(4.0);
                        let name = crate::ofx::fx_name(&e);
                        section(ui, &mut self.fx_open, &name, |ui| {
                            ui.add_space(2.0);
                            if e.vst.is_some() {
                                let generic = app.settings.prefs.vst_generic_editor;
                                crate::vst_ui::page(
                                    ui,
                                    &mut app.vst,
                                    generic,
                                    f64::from(rate),
                                    target,
                                    i,
                                    &mut e,
                                );
                            } else if e.kind == EffectKind::ModernEq {
                                let id = egui::Id::new(("modern-eq", target, i));
                                // The chain the mixer measures it in.
                                use bv_engine::modern_eq::TapOwner;
                                let tap = match target {
                                    FxTarget::Clip(c) => Some(TapOwner::Clip(c)),
                                    FxTarget::Track(t) => Some(TapOwner::Track(t)),
                                    FxTarget::Bus(b) => Some(TapOwner::Bus(b)),
                                    FxTarget::Chain(c) => Some(TapOwner::Chain(c)),
                                    FxTarget::Master => Some(TapOwner::Master),
                                    _ => None,
                                };
                                crate::modern_eq::editor(
                                    ui,
                                    &mut e,
                                    rate,
                                    id,
                                    tap.map(|o| (o, i)),
                                );
                            } else {
                                properties::params(ui, &mut e);
                            }
                        });
                        ui.add_space(6.0);
                        if e.kind.works_offline() {
                            // On an event (or its media) the event plays
                            // its sound made through the whole chain; a
                            // track's or a bus's sound cannot change length.
                            let on_event = matches!(target, FxTarget::Clip(_) | FxTarget::Media(..));
                            crate::tabs::hint(
                                ui,
                                if on_event {
                                    tr("It changes the sound's length: the event plays its sound made through the whole chain, ready a moment after each change, and keeps its own length.")
                                } else {
                                    tr("It changes the sound's length, so it works in an event's chain; here the sound goes through it unchanged.")
                                },
                            );
                        }
                        crate::tabs::hint(
                            ui,
                            &trf!(
                                "Category: {}. Right-click the chain node to reorder, disable or remove it.",
                                tr(e.kind.category())
                            ),
                        );
                    });
                }
            }
        }
        // Automated values changed: their envelopes' points at the cursor;
        // the effect's own values stay.
        if live && let Some(o) = owner {
            use bv_timeline::fx_automation::kind_of;
            let mut points = Vec::new();
            for what in &automated {
                match *what {
                    None => {
                        if e.enabled != shown.enabled {
                            points.push((kind_of(e0.id, None), if e.enabled { 1.0 } else { 0.0 }));
                        }
                        e.enabled = e0.enabled;
                    }
                    Some(p) => {
                        let k = p as usize;
                        if let Some(spec) = e.kind.params().get(k)
                            && e.param(k) != shown.param(k)
                        {
                            points.push((kind_of(e0.id, Some(p)), spec.to_unit(e.param(k))));
                        }
                        e.set_param(k, e0.param(k));
                    }
                }
            }
            if !points.is_empty() {
                let at = app.cursor;
                let key = egui::Id::new(("fx-automacao", self.target, i, self.gesture));
                app.modify_merged(key, |p| {
                    for (k, v) in &points {
                        let _ = p.set_envelope_point(o, *k, at, *v);
                    }
                });
            }
        }
        if e != *e0 {
            let params_only = e.kind == e0.kind && e.enabled == e0.enabled;
            let mut c = chain.to_vec();
            c[i] = e;
            let key = egui::Id::new(("fx-parametros", self.target, i, self.gesture));
            let t = self.target;
            if params_only {
                app.set_fx_params(key, c, |p| t.chain(p), |p, c| t.set_chain(p, c));
            } else {
                app.modify_merged(key, |p| t.set_chain(p, c));
            }
        }
        size
    }

    // ---- Pan/Crop -------------------------------------------------------------

    fn pan_crop(&mut self, ui: &mut egui::Ui, app: &mut BananaApp, project: &Project) {
        let FxTarget::Clip(id) = self.target else {
            return;
        };
        let Some(clip) = project.clip(id) else { return };
        let info = project.media(clip.media);
        let s = project.settings;
        let out = (s.width as f32, s.height as f32);
        let src = info
            .and_then(|m| m.video)
            .map_or(out, |v| (v.width as f32, v.height as f32));
        let rate = s.frame_rate;
        let last = (clip.duration - rate.frame_duration()).max(Tick::ZERO);
        if self.pan.sync && app.cursor >= clip.start && app.cursor < clip.end() {
            self.pan.pos = rate.snap_floor(app.cursor) - clip.start;
        }
        self.pan.pos = self.pan.pos.clamp(Tick::ZERO, last);
        let t = clip.start + self.pan.pos;
        let Some(place) = project.clip_place_in(id) else {
            return;
        };
        let key = clip.box_at(t, place.media, place.media_aspect);
        let pc = PanCtx {
            clip,
            info,
            t,
            key,
            place,
            vp: Viewport::of(&key),
            src,
        };

        let strip_h = crate::pancrop_keys::height();
        let body_h = (ui.available_height() - strip_h - 8.0).max(200.0);
        let mut new_box = None;
        let mut new_source = None;
        // The Mask row picked: the masks' tools, properties and workspace.
        let mut new_mask = None;
        let masks = self.pan.row == KeyRow::Mask;
        // The properties beside the workspace while Show Properties is on
        // (off at first, as VEGAS's).
        let mut properties = app.settings.pan_crop_properties;
        ui.horizontal(|ui| {
            ui.set_height(body_h);
            self.toolbar(ui, &mut properties);
            ui.add_space(4.0);
            ui.spacing_mut().item_spacing.x = 0.0;
            if properties {
                // As wide as the splitter at their right says (dragged).
                let area = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), body_h));
                let w = crate::widgets::pane_width(
                    ui,
                    egui::Id::new("pancrop-propriedades-largura"),
                    area,
                    250.0,
                    160.0,
                    240.0,
                );
                ui.allocate_ui_with_layout(vec2(w, body_h), Layout::top_down(Align::LEFT), |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("pancrop-propriedades")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if masks {
                                new_mask =
                                    self.pan
                                        .mask
                                        .properties(ui, clip.mask.as_ref(), self.pan.pos);
                                self.workspace_section(ui);
                            } else {
                                (new_box, new_source) = self.properties(ui, app, &pc);
                            }
                        });
                });
                // The splitter (drawn by `pane_width`) and a little room.
                ui.add_space(4.0 + 3.0);
            }
            let size = vec2(ui.available_width(), body_h);
            if let Some(k) = self.workspace(ui, app, &pc, size, &mut new_mask) {
                new_box = Some(k);
            }
        });
        app.settings.pan_crop_properties = properties;
        ui.add_space(4.0);
        self.key_strip(ui, app, clip, rate);
        if let Some(k) = new_box {
            let key = egui::Id::new(("pancrop", id, self.gesture));
            let new = app.settings.prefs.new_motion_keys();
            app.modify_merged(key, |p| {
                let _ = p.set_clip_box_at(id, t, k, new);
            });
        }
        if let Some((keep, fill)) = new_source {
            let key = egui::Id::new(("pancrop-origem", id, self.gesture));
            app.modify_merged(key, |p| {
                if let Some(c) = p.clip_mut(id) {
                    c.pan_crop.keep_aspect = keep;
                    c.pan_crop.fill = fill;
                }
            });
        }
        if let Some(m) = new_mask {
            let key = egui::Id::new(("pancrop-mascara", id, self.gesture));
            app.modify_merged(key, |p| {
                if let Some(c) = p.clip_mut(id) {
                    c.mask = m;
                }
            });
        }
    }

    /// The tools down the left side, as VEGAS's: Show Properties, then the
    /// Position row's (Normal and Zoom Edit Tool, Enable Snapping, Lock
    /// Aspect Ratio, Size About Center, the movement's mode) or the Mask
    /// row's.
    fn toolbar(&mut self, ui: &mut egui::Ui, properties: &mut bool) {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            toggle(
                ui,
                properties,
                Glyph::Icon(Icon::Settings),
                tr("Show Properties"),
            );
            let p = &mut self.pan;
            let zoom_tip = tr(
                "Zoom Edit Tool (click: zoom in · right-click or Shift+click: zoom out · drag: pan the view)",
            );
            if p.row == KeyRow::Mask {
                // The Mask row's: VEGAS's tools for masks, then the zoom
                // and snapping.
                if p.mask.toolbar(ui, p.tool == Tool::Zoom) {
                    p.tool = Tool::Normal;
                }
                let zoom = Glyph::Icon(Icon::ZoomTool);
                if glyph_button(ui, zoom, p.tool == Tool::Zoom, true, zoom_tip).clicked() {
                    p.tool = Tool::Zoom;
                }
                toggle(
                    ui,
                    &mut p.snap,
                    Glyph::Icon(Icon::Snap),
                    tr("Enable Snapping"),
                );
                return;
            }
            if glyph_button(
                ui,
                Glyph::Pointer,
                p.tool == Tool::Normal,
                true,
                tr("Normal Edit Tool"),
            )
            .clicked()
            {
                p.tool = Tool::Normal;
            }
            let zoom = Glyph::Icon(Icon::ZoomTool);
            if glyph_button(ui, zoom, p.tool == Tool::Zoom, true, zoom_tip).clicked() {
                p.tool = Tool::Zoom;
            }
            toggle(
                ui,
                &mut p.snap,
                Glyph::Icon(Icon::Snap),
                tr("Enable Snapping"),
            );
            toggle(
                ui,
                &mut p.lock_aspect,
                Glyph::Icon(Icon::Lock),
                tr("Lock Aspect Ratio"),
            );
            toggle(
                ui,
                &mut p.from_center,
                Glyph::FromCenter,
                tr("Size About Center"),
            );
            // One button, as VEGAS's: a click goes to the next mode.
            let (g, tip) = match p.axis {
                Axis::Free => (Glyph::MoveFree, tr("Move Freely (X or Y)")),
                Axis::X => (Glyph::MoveX, tr("Move in X Only")),
                Axis::Y => (Glyph::MoveY, tr("Move in Y Only")),
            };
            if glyph_button(ui, g, p.axis != Axis::Free, true, tip).clicked() {
                p.axis = match p.axis {
                    Axis::Free => Axis::X,
                    Axis::X => Axis::Y,
                    Axis::Y => Axis::Free,
                };
            }
        });
    }

    /// The properties beside the workspace, as VEGAS's grid lists them:
    /// the box's Position, its Rotation (the angle and where it turns),
    /// the keyframe's interpolation, the Source settings and the
    /// Workspace. The new box and the new Source settings, when they
    /// changed.
    fn properties(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut BananaApp,
        pc: &PanCtx<'_>,
    ) -> (Option<PanCropKey>, Option<(bool, bool)>) {
        let mut vp = pc.vp;
        let mut key = pc.key;
        let (mut keep, mut fill) = (pc.place.keep_aspect, pc.place.fill);
        let lock = self.pan.lock_aspect;
        let src = pc.src;
        section(ui, &mut self.pan.sections[0], tr("Position"), |ui| {
            prop_grid(ui, "pc-posicao", |ui| {
                let (w0, h0) = (vp.w, vp.h);
                prop(
                    ui,
                    tr("Width"),
                    crate::fmt::drag_grouped(&mut vp.w, 1)
                        .speed(1.0)
                        .range(1.0..=src.0 * 50.0),
                );
                prop(
                    ui,
                    tr("Height"),
                    crate::fmt::drag_grouped(&mut vp.h, 1)
                        .speed(1.0)
                        .range(1.0..=src.1 * 50.0),
                );
                if lock {
                    if vp.w != w0 {
                        vp.h = vp.w * h0 / w0;
                    } else if vp.h != h0 {
                        vp.w = vp.h * w0 / h0;
                    }
                }
                prop(
                    ui,
                    tr("X Center"),
                    crate::fmt::drag_grouped(&mut vp.cx, 1).speed(1.0),
                );
                prop(
                    ui,
                    tr("Y Center"),
                    crate::fmt::drag_grouped(&mut vp.cy, 1).speed(1.0),
                );
            });
        });
        section(ui, &mut self.pan.sections[1], tr("Rotation"), |ui| {
            prop_grid(ui, "pc-rotacao", |ui| {
                // The box turns about its X and Y Center.
                let mut angle = vp.angle_deg;
                let r = prop(
                    ui,
                    tr("Angle"),
                    crate::fmt::drag_grouped(&mut angle, 1)
                        .speed(0.5)
                        .range(-360.0..=360.0),
                );
                if r.changed() {
                    let by = f64::from(angle).to_radians() - key.angle;
                    key = key.turned(by);
                }
                let (mut x, mut y) = (key.pivot[0] as f32, key.pivot[1] as f32);
                let (rx, ry) = (
                    prop(
                        ui,
                        tr("X Center"),
                        crate::fmt::drag_grouped(&mut x, 1).speed(1.0),
                    ),
                    prop(
                        ui,
                        tr("Y Center"),
                        crate::fmt::drag_grouped(&mut y, 1).speed(1.0),
                    ),
                );
                if rx.changed() {
                    key.pivot[0] = f64::from(x);
                }
                if ry.changed() {
                    key.pivot[1] = f64::from(y);
                }
            });
        });
        // The keyframe at the cursor or the one before it (the first one,
        // made when changed, before there are any).
        let keys = &pc.clip.pan_crop.keys;
        let key_idx = keys.iter().rposition(|k| k.at <= self.pan.pos);
        let mut smooth_edit = None;
        section(
            ui,
            &mut self.pan.sections[2],
            tr("Keyframe interpolation"),
            |ui| {
                prop_grid(ui, "pc-interpolacao", |ui| {
                    let mut s = key_idx.map_or(0.0, |i| (keys[i].smoothness * 100.0) as f32);
                    let r = prop(
                        ui,
                        tr("Smoothness"),
                        crate::fmt::drag_grouped(&mut s, 1)
                            .speed(0.5)
                            .range(0.0..=100.0),
                    );
                    if r.changed() {
                        let at = key_idx.map_or(Tick::ZERO, |i| keys[i].at);
                        smooth_edit = Some((at, s));
                    }
                });
            },
        );
        if let Some((at, s)) = smooth_edit {
            self.set_smoothness(app, pc.clip, at, s);
        }
        section(ui, &mut self.pan.sections[3], tr("Source"), |ui| {
            prop_grid(ui, "pc-origem", |ui| {
                prop_yes_no(ui, tr("Maintain aspect ratio"), "pc-proporcao", &mut keep);
                prop_yes_no(ui, tr("Stretch to fill frame"), "pc-preencher", &mut fill);
            });
        });
        self.workspace_section(ui);
        let new_box = vp.onto(&pc.vp, &key);
        (
            (new_box != pc.key).then_some(new_box),
            ((keep, fill) != (pc.place.keep_aspect, pc.place.fill)).then_some((keep, fill)),
        )
    }

    /// The properties' Workspace section (both rows have it), as VEGAS's:
    /// the zoom (the media's pixels as a percentage of the screen's), how
    /// far the view is moved (the media's pixels) and the grid's spacing.
    fn workspace_section(&mut self, ui: &mut egui::Ui) {
        let ppp = ui.ctx().pixels_per_point();
        let pan = &mut self.pan;
        let k = pan.view.map(|(sp, _)| sp.k).filter(|k| *k > 0.0);
        section(ui, &mut pan.sections[4], tr("Workspace"), |ui| {
            prop_grid(ui, "pc-espaco", |ui| {
                if let Some(k) = k {
                    let shown = k * ppp * 100.0;
                    let mut z = shown;
                    let r = prop(
                        ui,
                        tr("Zoom (%)"),
                        crate::fmt::drag_grouped(&mut z, 1)
                            .speed(0.5)
                            .range(0.5..=4000.0),
                    );
                    if r.changed() {
                        // About the workspace's middle.
                        let zoom = (pan.zoom * z / shown).clamp(0.05, 40.0);
                        pan.offset *= zoom / pan.zoom;
                        pan.zoom = zoom;
                    }
                    let mut off = [pan.offset.x / k, pan.offset.y / k];
                    let rx = prop(
                        ui,
                        tr("X Offset"),
                        crate::fmt::drag_grouped(&mut off[0], 1).speed(1.0),
                    );
                    let ry = prop(
                        ui,
                        tr("Y Offset"),
                        crate::fmt::drag_grouped(&mut off[1], 1).speed(1.0),
                    );
                    if rx.changed() || ry.changed() {
                        pan.offset = vec2(off[0] * k, off[1] * k);
                    }
                }
                let mut grid = pan.grid;
                let r = prop(
                    ui,
                    tr("Grid spacing"),
                    crate::fmt::drag_grouped(&mut grid, 0)
                        .speed(0.25)
                        .range(2.0..=64.0),
                );
                if r.changed() {
                    pan.grid = grid.round();
                }
            });
        });
    }

    /// A área de trabalho: a mídia, o F e a interação. Devolve o novo
    /// enquadramento, se mudou.
    fn workspace(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut BananaApp,
        pc: &PanCtx<'_>,
        size: Vec2,
        new_mask: &mut Option<Option<bv_timeline::mask::PanCropMask>>,
    ) -> Option<PanCropKey> {
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
        // The Mask row picked: the masks' tools work here, not the F's.
        let masks = self.pan.row == KeyRow::Mask;
        let src = pc.src;
        // At first, as VEGAS's: the media's rotation circle within four
        // fifths of the workspace.
        let k0 = 0.8 * rect.width().min(rect.height()) / (1.1 * src.0.hypot(src.1));
        let sp = self.pan.space(rect, k0, src);
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let shift = ui.input(|i| i.modifiers.shift);
        let mut vp = pc.vp;
        let mut changed = false;
        // The box turns about its pivot (Rotation's X and Y Center): the
        // pointer's angle round it.
        let pivot = (pc.key.pivot[0] as f32, pc.key.pivot[1] as f32);
        let round_pivot = |p: Pos2| (p - sp.screen(pivot)).angle();
        let mut result = None;

        // Vista: roda do mouse e ferramenta de zoom.
        if resp.hovered()
            && let Some(p) = pointer
        {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.pan.zoom_at(rect, k0, src, p, (scroll * 0.0025).exp());
            }
        }
        if self.pan.tool == Tool::Zoom
            && resp.clicked()
            && let Some(p) = pointer
        {
            let f = if shift { 1.0 / 1.5 } else { 1.5 };
            self.pan.zoom_at(rect, k0, src, p, f);
        }
        // Its right button zooms out by half as it goes down, as VEGAS's
        // (the magnifier with − meanwhile).
        if self.pan.tool == Tool::Zoom
            && resp.hovered()
            && ui.input(|i| i.pointer.button_pressed(PointerButton::Secondary))
            && let Some(p) = pointer
        {
            self.pan.zoom_at(rect, k0, src, p, 0.5);
        }

        // Arrastos.
        if resp.drag_started()
            && let Some(o) = ui.input(|i| i.pointer.press_origin())
        {
            let view = Drag::View {
                offset: self.pan.offset,
                from: o,
            };
            self.pan.drag = if resp.drag_started_by(PointerButton::Middle)
                || (self.pan.tool == Tool::Zoom && resp.drag_started_by(PointerButton::Primary))
            {
                Some(view)
            } else if !masks && resp.drag_started_by(PointerButton::Primary) {
                match hit(&sp, &vp, o, true) {
                    Hit::Handle(sx, sy) => Some(Drag::Resize { vp, sx, sy }),
                    _ if (sp.screen(pivot) - o).length() <= 7.0 => {
                        Some(Drag::Pivot { at: pivot, from: o })
                    }
                    Hit::Rotate => Some(Drag::Rotate {
                        vp,
                        from: round_pivot(o),
                    }),
                    Hit::Inside => Some(Drag::Move { vp, from: o }),
                    // Far from the box the view is dragged, as VEGAS's open
                    // hand there says.
                    Hit::Outside => Some(view),
                }
            } else {
                None
            };
        }
        if resp.dragged()
            && let (Some(d), Some(p)) = (self.pan.drag, pointer)
        {
            // The grid's steps, each way: a part of the media's frame.
            let (gx, gy) = (
                src.0 / self.pan.grid.max(1.0),
                src.1 / self.pan.grid.max(1.0),
            );
            match d {
                Drag::View { offset, from } => self.pan.offset = offset + (p - from),
                Drag::Move { vp: v0, from } => {
                    let (mut dx, mut dy) = ((p.x - from.x) / sp.k, (p.y - from.y) / sp.k);
                    match self.pan.axis {
                        Axis::X => dy = 0.0,
                        Axis::Y => dx = 0.0,
                        Axis::Free if shift => {
                            if dx.abs() > dy.abs() {
                                dy = 0.0;
                            } else {
                                dx = 0.0;
                            }
                        }
                        Axis::Free => {}
                    }
                    vp.cx = v0.cx + dx;
                    vp.cy = v0.cy + dy;
                    if self.pan.snap {
                        vp.cx = (vp.cx / gx).round() * gx;
                        vp.cy = (vp.cy / gy).round() * gy;
                    }
                    // The pivot goes with it.
                    result = Some(pc.key.moved([
                        f64::from(vp.cx) - pc.key.center[0],
                        f64::from(vp.cy) - pc.key.center[1],
                    ]));
                }
                Drag::Resize { vp: v0, sx, sy } => {
                    let l = local(&sp, &v0, p);
                    let (lock, center) = (self.pan.lock_aspect, self.pan.from_center);
                    vp = resize(&v0, l, sx, sy, lock, center);
                    if self.pan.snap {
                        vp = snap_resize(&v0, &vp, sx, sy, lock, center, (gx, gy));
                    }
                    changed = true;
                }
                Drag::Rotate { vp: v0, from } => {
                    let mut a = v0.angle_deg + (round_pivot(p) - from).to_degrees();
                    // Turned on from where it is, however far round it went.
                    a += ((vp.angle_deg - a) / 360.0).round() * 360.0;
                    if shift {
                        a = (a / 15.0).round() * 15.0;
                    }
                    result = Some(pc.key.turned(f64::from(a).to_radians() - pc.key.angle));
                }
                Drag::Pivot { at, from } => {
                    let (mut x, mut y) =
                        (at.0 + (p.x - from.x) / sp.k, at.1 + (p.y - from.y) / sp.k);
                    if self.pan.snap {
                        x = (x / gx).round() * gx;
                        y = (y / gy).round() * gy;
                    }
                    let mut k = pc.key;
                    k.pivot = [f64::from(x), f64::from(y)];
                    result = Some(k);
                }
            }
        }
        if resp.drag_stopped() {
            self.pan.drag = None;
        }

        // A click or a drag on the box picks it, one off it lets it go; the
        // arrow keys then move it a pixel of the media (Shift: ten), as
        // VEGAS's, along the axis the movement's mode leaves free.
        if masks || self.pan.tool == Tool::Zoom {
            self.pan.picked = false;
        } else if (resp.clicked() || resp.drag_started_by(PointerButton::Primary))
            && let Some(o) = ui.input(|i| i.pointer.press_origin())
        {
            self.pan.picked = !matches!(hit(&sp, &pc.vp, o, true), Hit::Outside)
                || (sp.screen(pivot) - o).length() <= 7.0;
        }
        if self.pan.picked && self.pan.drag.is_none() && !ui.ctx().egui_wants_keyboard_input() {
            let (dx, dy) = ui.input_mut(|i| arrow_nudge(i, self.pan.axis));
            if dx != 0.0 || dy != 0.0 {
                result = Some(pc.key.moved([f64::from(dx), f64::from(dy)]));
            }
        }

        // Menu do botão direito (the zoom tool's right button zooms out).
        let mut action = None;
        let adjustment = pc.info.is_some_and(MediaInfo::is_adjustment);
        let menu = if masks || self.pan.tool == Tool::Zoom {
            None
        } else {
            Some(&resp)
        };
        menu.map(|resp| {
            resp.context_menu(|ui| {
                let mut item = |ui: &mut egui::Ui, label: &str, a: MenuAction| {
                    if ui.button(label).clicked() {
                        action = Some(a);
                        ui.close();
                    }
                };
                // VEGAS's, in its order.
                item(ui, tr("Restore"), MenuAction::Reset);
                item(ui, trc("menu", "Center"), MenuAction::Center);
                item(ui, tr("Flip Horizontal"), MenuAction::FlipH);
                item(ui, tr("Flip Vertical"), MenuAction::FlipV);
                item(ui, tr("Match Output Aspect"), MenuAction::MatchOutput);
                item(ui, tr("Match Source Aspect"), MenuAction::MatchSource);
                if adjustment {
                    item(
                        ui,
                        tr("Resize Adjustment Event to Project"),
                        MenuAction::FitAdjustment,
                    );
                }
            })
        });
        let place = &pc.place;
        match action {
            Some(MenuAction::Reset) => result = Some(PanCropKey::whole(place.media)),
            Some(MenuAction::Center) => result = Some(pc.key.centered(place.media)),
            Some(MenuAction::FlipH) => result = Some(pc.key.flipped_h()),
            Some(MenuAction::FlipV) => result = Some(pc.key.flipped_v()),
            // The shape of the frame or of the media, as large as fits.
            Some(MenuAction::MatchOutput) => {
                let r = (place.frame[0] * place.frame_aspect)
                    / (place.frame[1] * place.media_aspect.max(1e-6));
                result = Some(pc.key.matched(r));
            }
            Some(MenuAction::MatchSource) => {
                result = Some(pc.key.matched(place.media[0] / place.media[1]));
            }
            Some(MenuAction::FitAdjustment) => {
                let id = pc.clip.id;
                app.modify(|p| {
                    let _ = p.resize_adjustment_to_project(id);
                });
            }
            None => {}
        }
        if changed && result.is_none() {
            result = Some(vp.onto(&pc.vp, &pc.key));
        }
        if let Some(k) = &result {
            vp = Viewport::of(k);
        }

        // Desenho (com o estado já atualizado).
        let sp = self.pan.space(rect, k0, src);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, WS_BG);
        paint_grid(&painter, rect, &sp, self.pan.grid);
        // A mídia (parada; quem gira e se move é o F).
        let media_rect = Rect::from_min_max(sp.screen((0.0, 0.0)), sp.screen(src));
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        // The media at the needle, decoded for this window at the size it
        // shows (as VEGAS's: whatever the Video Preview shows, Sync Cursor
        // off too); the last picture stays while the next one is decoded,
        // the media's thumbnail before the first.
        let ppp = ui.ctx().pixels_per_point();
        let shown_size = (
            (media_rect.width() * ppp).round().max(1.0) as u32,
            (media_rect.height() * ppp).round().max(1.0) as u32,
        );
        let wanted = pc.clip.source_time(pc.t);
        let picture = pc.info.and_then(|m| {
            app.engine
                .source_picture(pc.clip.id, m, wanted, pc.clip.duration, shown_size)
        });
        app.source_watched = true;
        if let Some(s) = picture.filter(|s| !s.rgba.is_empty()) {
            // Its pixels' (a still's are the same at every time).
            if self
                .pan
                .source
                .as_ref()
                .is_none_or(|m| !std::sync::Arc::ptr_eq(&m.pixels, &s.rgba))
            {
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [s.width as usize, s.height as usize],
                    &s.rgba,
                );
                let options = egui::TextureOptions::LINEAR;
                match &mut self.pan.source {
                    Some(m) => {
                        m.texture.set(image, options);
                        (m.pixels, m.clip) = (s.rgba.clone(), pc.clip.id);
                    }
                    slot => {
                        *slot = Some(ShownMedia {
                            pixels: s.rgba.clone(),
                            clip: pc.clip.id,
                            texture: ui.ctx().load_texture("pancrop-media", image, options),
                        });
                    }
                }
            }
        }
        let shown = self
            .pan
            .source
            .as_ref()
            .filter(|m| m.clip == pc.clip.id)
            .map(|m| m.texture.id());
        let thumb = || pc.info.and_then(|m| app.engine.thumbnail(m, wanted, 360));
        match shown {
            Some(tex) => {
                painter.image(tex, media_rect, uv, Color32::WHITE);
            }
            None => match thumb() {
                Some(th) => {
                    let tex = app.textures.get(ui.ctx(), &th);
                    painter.image(tex, media_rect, uv, Color32::WHITE);
                }
                None => {
                    painter.rect_filled(media_rect, 0.0, Color32::from_gray(120));
                }
            },
        }
        let [crop_left, crop_right, crop_top, crop_bottom] = pc.key.crop;
        let shade = Color32::from_black_alpha(120);
        let crop = |u0: f32, v0: f32, u1: f32, v1: f32| {
            Rect::from_min_max(
                sp.screen((u0 * src.0, v0 * src.1)),
                sp.screen((u1 * src.0, v1 * src.1)),
            )
        };
        for (on, r) in [
            (crop_left > 0.0, crop(0.0, 0.0, crop_left, 1.0)),
            (crop_right > 0.0, crop(1.0 - crop_right, 0.0, 1.0, 1.0)),
            (crop_top > 0.0, crop(0.0, 0.0, 1.0, crop_top)),
            (crop_bottom > 0.0, crop(0.0, 1.0 - crop_bottom, 1.0, 1.0)),
        ] {
            if on {
                painter.rect_filled(r, 0.0, shade);
            }
        }
        let shown = result.as_ref().unwrap_or(&pc.key);
        let flips = (shown.width < 0.0, shown.height < 0.0);
        let turns = (shown.pivot[0] as f32, shown.pivot[1] as f32);
        if masks {
            let n = f64::from(self.pan.grid.max(1.0));
            let grid = self
                .pan
                .snap
                .then_some([f64::from(src.0) / n, f64::from(src.1) / n]);
            let normal = self.pan.tool == Tool::Normal;
            *new_mask = self.pan.mask.workspace(
                ui,
                &resp,
                &painter,
                &sp,
                pc.clip.mask.as_ref(),
                self.pan.pos,
                grid,
                normal,
            );
        } else {
            paint_f_box(&painter, &sp, &vp, flips, turns, true);
        }

        // The pointer, as VEGAS's (`pan_cursor`): the Mask row's tools pick
        // their own, but for the view dragged and the zoom tool.
        let viewing = matches!(self.pan.drag, Some(Drag::View { .. }));
        if !masks || viewing || self.pan.tool == Tool::Zoom {
            let hover = resp.hover_pos().map(|p| {
                let on_pivot = (sp.screen(turns) - p).length() <= 7.0;
                (hit(&sp, &vp, p, true), on_pivot)
            });
            let zoom_out = shift
                || (resp.is_pointer_button_down_on() && ui.input(|i| i.pointer.secondary_down()));
            let drag = self.pan.drag.as_ref();
            if let Some(c) = pan_cursor(self.pan.tool, drag, hover, zoom_out, vp.angle_deg, flips) {
                cursors::set(ui.ctx(), c);
            }
        }
        self.pan.view = Some((sp, vp));
        result
    }

    // ---- the keyframe controller -------------------------------------------

    /// The keyframe controller under the workspace (`pancrop_keys`), and
    /// what it asked for: the needle, the row, Sync Cursor, the masks on
    /// or off, the rows' new keyframes.
    fn key_strip(&mut self, ui: &mut egui::Ui, app: &mut BananaApp, clip: &Clip, rate: FrameRate) {
        let place = app.project().clip_place_in(clip.id);
        let cx = crate::pancrop_keys::StripCtx {
            clip,
            media: place.map_or([1.0; 2], |p| p.media),
            aspect: place.map_or(1.0, |p| p.media_aspect),
            rate,
            pos: self.pan.pos,
            row: self.pan.row,
            sync: self.pan.sync,
            curve: app.settings.prefs.new_motion_keys().0,
            keys_on: true,
        };
        let answer = self.pan.keys.show(ui, &cx);
        if let Some(r) = answer.row {
            self.pan.row = r;
        }
        if let Some(s) = answer.sync {
            self.pan.sync = s;
        }
        let id = clip.id;
        if answer.toggle_mask {
            app.modify(|p| {
                if let Some(c) = p.clip_mut(id) {
                    match &mut c.mask {
                        Some(m) => m.enabled = !m.enabled,
                        None => {
                            c.mask = Some(bv_timeline::mask::PanCropMask {
                                enabled: true,
                                ..Default::default()
                            })
                        }
                    }
                }
            });
        }
        if let Some((keys, drag)) = answer.keys {
            let set = move |p: &mut Project| {
                let media = p.clip_place_in(id).map(|place| place.media);
                let Some(c) = p.clip_mut(id) else { return };
                if keys.position != c.pan_crop.keys {
                    c.pan_crop.keys = keys.position.clone();
                    if let Some(m) = media {
                        c.pan_crop.size = [m[0] as u32, m[1] as u32];
                    }
                }
                if let Some(m) = c.mask.as_mut()
                    && (!m.keys.is_empty() || !keys.mask.is_empty())
                    && keys.mask != m.keys
                {
                    m.keys = keys.mask.clone();
                }
            };
            match drag {
                // A drag undoes as one step.
                Some(n) => app.modify_merged(egui::Id::new(("pancrop-keys", id, n)), set),
                None => app.modify(set),
            }
        }
        if let Some(np) = answer.pos {
            self.pan.pos = np;
            if self.pan.sync {
                app.seek(rate.snap_floor(clip.start + np));
            }
        }
    }

    /// The Keyframe interpolation's Smoothness of the keyframe at `at`
    /// (the first one made, at the start, when there are none).
    fn set_smoothness(&self, app: &mut BananaApp, clip: &Clip, at: Tick, s: f32) {
        let id = clip.id;
        let key = egui::Id::new(("qc-suavidade", id, at.0));
        let (curve, _) = app.settings.prefs.new_motion_keys();
        app.modify_merged(key, |p| {
            let media = p.clip_place_in(id).map(|place| place.media);
            let Some(c) = p.clip_mut(id) else { return };
            if c.pan_crop.keys.is_empty()
                && let Some(m) = media
            {
                c.pan_crop.ensure_size(m);
                c.pan_crop.keys.push(PanCropKey {
                    curve,
                    ..PanCropKey::whole(m)
                });
            }
            if let Some(k) = c.pan_crop.keys.iter_mut().find(|k| k.at == at) {
                k.smoothness = f64::from(s.clamp(0.0, 100.0) / 100.0);
            }
        });
    }
}

// ---- seletor de plug-ins ------------------------------------------------------

/// Resultado do seletor neste quadro.
pub enum Choice {
    Open,
    Cancel,
    /// Aplicar a cadeia; o índice é o plug-in a mostrar na janela de FX.
    Ok(Option<usize>),
}

/// What the chooser's catalog offers: one of BananaV's effects, an OFX
/// plug-in (by identifier), a VST 3 plug-in or a filter package (by name).
#[derive(Clone, Debug, PartialEq, Eq)]
enum Pick {
    Kind(EffectKind),
    Ofx(String),
    Vst(crate::vst::VstPlugin),
    Package(String),
}

impl Pick {
    /// What it adds to a chain: an effect, or a package's chain.
    fn effects(&self, packages: &[Package]) -> Vec<Effect> {
        match self {
            Pick::Kind(k) => vec![Effect::new(*k)],
            Pick::Ofx(id) => bv_ofx::registry()
                .info(id)
                .map(|i| crate::ofx::new_effect(&i))
                .into_iter()
                .collect(),
            Pick::Vst(p) => vec![crate::vst_ui::new_effect(p)],
            Pick::Package(name) => packages
                .iter()
                .find(|p| p.name == *name)
                .map_or_else(Vec::new, |p| p.chain.clone()),
        }
    }
}

/// The chooser's folders of VST 3 effects and of VST 2 ones.
const VST_FOLDER: &str = "VST3";
const VST2_FOLDER: &str = "VST";

/// How many points a side the chain's LUT has (Save LUT), as Color
/// Grading's Export LUT.
const LUT_POINTS: usize = 33;

#[derive(Debug)]
pub struct Chooser {
    pub target: FxTarget,
    pub chain: Vec<Effect>,
    folder: Option<String>,
    picked: Option<Pick>,
    chain_sel: Option<usize>,
    /// The search field's text: the plug-ins of every folder whose names
    /// have it.
    search: String,
    /// The letters typed over the list (see [`type_ahead`]) and when the
    /// last one was.
    typed: String,
    typed_at: f64,
    /// The plug-in picked by typing: the list scrolls to it.
    scroll_pick: bool,
}

/// How long after a letter typed over the list the next one adds to it
/// (as in a file manager); later, a new search begins.
const TYPE_AHEAD_PAUSE: f64 = 1.0;

/// A plug-in of the chooser's list as shown: what it adds, its name, its
/// badge's color and the text under the pointer.
struct ListItem {
    pick: Pick,
    name: String,
    badge: Color32,
    hover: Option<String>,
}

/// `s` for searching: lowercase, its accented letters as plain ones
/// ("Correção" finds "correcao").
fn search_fold(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' | 'å' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            c => c,
        })
        .collect()
}

/// The plug-in typing `typed` over the list picks, as a file manager's
/// list does: the same letter again and again goes to the next name
/// beginning with it after `current`, round to the first; more letters,
/// to the first name from `current` on that begins with them all.
fn type_ahead(names: &[String], current: Option<usize>, typed: &str) -> Option<usize> {
    let typed = search_fold(typed);
    let first = typed.chars().next()?;
    let n = names.len();
    let starts = |i: usize, prefix: &str| search_fold(&names[i]).starts_with(prefix);
    if typed.chars().all(|c| c == first) {
        let one = first.to_string();
        let from = current.map_or(0, |c| c + 1);
        (0..n).map(|k| (from + k) % n).find(|&i| starts(i, &one))
    } else {
        let from = current.unwrap_or(0);
        (0..n).map(|k| (from + k) % n).find(|&i| starts(i, &typed))
    }
}

impl Chooser {
    pub fn new(target: FxTarget, chain: Vec<Effect>) -> Chooser {
        Chooser {
            target,
            chain,
            folder: None,
            picked: None,
            chain_sel: None,
            search: String::new(),
            typed: String::new(),
            typed_at: f64::NEG_INFINITY,
            scroll_pick: false,
        }
    }

    /// Its search set to `text` (screen tests).
    pub(crate) fn search_for_review(&mut self, text: &str) {
        self.search = text.to_string();
    }

    /// Adds what `pick` is to the chain's end (a package's whole chain).
    fn add(&mut self, pick: &Pick, packages: &[Package]) {
        let effects = pick.effects(packages);
        if !effects.is_empty() {
            self.chain.extend(effects);
            self.chain_sel = Some(self.chain.len() - 1);
        }
    }

    /// Save LUT: the chain's enabled effects as a `.cube` file.
    fn save_lut(&self, app: &mut BananaApp) {
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr("Save LUT"))
            .add_filter(tr("Cube LUT"), &["cube"])
            .set_file_name("Plug-In Chain.cube")
            .save_file()
        else {
            return;
        };
        let title = self
            .chain
            .iter()
            .filter(|e| e.enabled)
            .map(crate::ofx::fx_name)
            .collect::<Vec<_>>()
            .join(", ");
        let video_gamma = app.project().settings.composites_in_video_gamma();
        let Some(text) = bv_engine::effect_chain_cube(&self.chain, LUT_POINTS, video_gamma, &title)
        else {
            app.report_error(tr(
                "This chain changes the picture's size: a LUT cannot hold it.",
            ));
            return;
        };
        match std::fs::write(&path, text) {
            Ok(()) => app.notify(trf!("LUT exported to {}", path.display())),
            Err(e) => app.report_error(trf!("Could not export the LUT: {}", e)),
        }
    }

    /// Shows the chooser; `vst` are the VST 3 effects found (offered on
    /// audio chains).
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        app: &mut BananaApp,
        vst: &[crate::vst::VstPlugin],
    ) -> Choice {
        let project = app.project().clone();
        let project = &project;
        if self.target.chain(project).is_none() {
            return Choice::Cancel;
        }
        let audio = self.target.audio(project);
        // The filter packages of chains of its kind.
        let packages = app.fx_packages.list(audio);
        let all: &[EffectKind] = if audio {
            &EffectKind::AUDIO
        } else {
            &EffectKind::VIDEO
        };
        // OFX plug-ins work on pictures.
        let ofx = if audio {
            Vec::new()
        } else {
            crate::ofx::filters()
        };
        let mut folders: Vec<String> = categories(all)
            .into_iter()
            .map(|f| tr(f).to_string())
            .collect();
        for p in &ofx {
            let g = crate::ofx::group_of(p);
            if !folders.contains(&g) {
                folders.push(g);
            }
        }
        // VST plug-ins work on sound.
        let vst = if audio { vst } else { &[] };
        if vst.iter().any(|p| p.vst3) {
            folders.push(VST_FOLDER.to_string());
        }
        if vst.iter().any(|p| !p.vst3) {
            folders.push(VST2_FOLDER.to_string());
        }
        let packages_folder = tr("Filter Packages").to_string();
        folders.push(packages_folder.clone());
        let mut open = true;
        let mut choice = Choice::Open;
        crate::floating::Window::new(trf!("Plug-In Chooser — {}", self.target.title(project)))
            .id(egui::Id::new("seletor-plugin"))
            .pivot(Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([820.0, 470.0])
            .min_size([600.0, 340.0])
            .show(ctx, |ui| {
                self.chain_bar(ui);
                ui.add_space(6.0);
                let body_h = (ui.available_height() - 30.0).max(160.0);
                ui.horizontal(|ui| {
                    ui.set_height(body_h);
                    panel(ui, vec2(200.0, body_h), |ui| self.folders(ui, &folders));
                    let list_w = (ui.available_width() - 128.0).max(220.0);
                    panel(ui, vec2(list_w, body_h), |ui| {
                        self.list(ui, all, &ofx, vst, &packages, &packages_folder);
                    });
                    ui.allocate_ui_with_layout(
                        vec2(118.0, body_h),
                        Layout::top_down_justified(Align::Center),
                        |ui| {
                            ui.spacing_mut().button_padding = vec2(8.0, 4.0);
                            if ui.button(RichText::new("OK").strong()).clicked() {
                                choice = Choice::Ok(self.chain_sel);
                            }
                            if ui.button(tr("Cancel")).clicked() {
                                choice = Choice::Cancel;
                            }
                            ui.add_space(16.0);
                            if ui
                                .add_enabled(self.picked.is_some(), egui::Button::new(tr("Add")))
                                .clicked()
                                && let Some(k) = self.picked.clone()
                            {
                                self.add(&k, &packages);
                            }
                            // Replace: the whole chain becomes what is
                            // picked (a plug-in, or a package's chain).
                            let sel = self.chain_sel.filter(|&i| i < self.chain.len());
                            if ui
                                .add_enabled(self.picked.is_some(), egui::Button::new(tr("Replace")))
                                .on_hover_text(tr(
                                    "Replaces the whole chain with the plug-in or filter package picked",
                                ))
                                .clicked()
                                && let Some(k) = self.picked.clone()
                            {
                                let effects = k.effects(&packages);
                                if !effects.is_empty() {
                                    self.chain = effects;
                                    self.chain_sel = Some(0);
                                }
                            }
                            if ui
                                .add_enabled(sel.is_some(), egui::Button::new(tr("Remove")))
                                .clicked()
                                && let Some(i) = sel
                            {
                                self.chain.remove(i);
                                self.chain_sel = None;
                            }
                            ui.add_space(16.0);
                            // Save As: the chain as a filter package.
                            if ui
                                .add_enabled(
                                    !self.chain.is_empty(),
                                    egui::Button::new(tr("Save As…")),
                                )
                                .on_hover_text(tr("Saves the chain as a filter package"))
                                .clicked()
                            {
                                app.fx_packages.ask_save(self.chain.clone(), audio);
                            }
                            // Save LUT: a video chain's colors as a table.
                            if !audio
                                && ui
                                    .add_enabled(
                                        self.chain.iter().any(|e| e.enabled),
                                        egui::Button::new(tr("Save LUT…")),
                                    )
                                    .on_hover_text(tr(
                                        "Saves what the chain's enabled effects do to colors as a .cube LUT: leave out those that are not about color (blurs, masks, vignettes)",
                                    ))
                                    .clicked()
                            {
                                self.save_lut(app);
                            }
                        },
                    );
                });
                let desc = match &self.picked {
                    Some(Pick::Kind(k)) => describe(*k),
                    Some(Pick::Ofx(id)) => {
                        ofx.iter()
                            .find(|p| &p.identifier == id)
                            .map_or_else(String::new, |p| {
                                let name = crate::ofx::plugin_name(p);
                                if bv_engine::plugins::is_own(p) {
                                    trf!(
                                        "{} ({}): {}",
                                        name,
                                        crate::ofx::group_of(p),
                                        tr(&p.description)
                                    )
                                } else if p.description.is_empty() {
                                    trf!("{} (OFX plug-in, {}).", name, p.grouping)
                                } else {
                                    trf!("{} (OFX plug-in): {}", name, p.description)
                                }
                            })
                    }
                    Some(Pick::Vst(p)) if p.vendor.is_empty() => {
                        trf!("{} (VST 3 plug-in).", p.name)
                    }
                    Some(Pick::Package(name)) => {
                        let names: Vec<String> = packages
                            .iter()
                            .find(|p| p.name == *name)
                            .map(|p| p.chain.iter().map(crate::ofx::fx_name).collect())
                            .unwrap_or_default();
                        trf!("{} (filter package): {}", name, names.join(" → "))
                    }
                    Some(Pick::Vst(p)) => trf!("{} (VST 3 plug-in by {}).", p.name, p.vendor),
                    None => tr("Pick a plug-in and click Add (or double-click it).").into(),
                };
                ui.label(RichText::new(trf!("Description: {}", desc)).color(theme::text_dim()));
            });
        if open { choice } else { Choice::Cancel }
    }

    fn chain_bar(&mut self, ui: &mut egui::Ui) {
        let mut sel = None;
        egui::Frame::new()
            .fill(theme::panel_dark())
            .stroke(Stroke::new(1.0, theme::separator()))
            .inner_margin(egui::Margin::symmetric(8, 5))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.set_min_height(26.0);
                    ui.spacing_mut().item_spacing.x = 0.0;
                    if self.chain.is_empty() {
                        ui.label(
                            RichText::new(tr("There are no plug-ins in the chain"))
                                .color(theme::text_dim()),
                        );
                        return;
                    }
                    chain_end(ui);
                    for (i, e) in self.chain.iter().enumerate() {
                        chain_link(ui);
                        if chain_node(ui, &crate::ofx::fx_name(e), None, self.chain_sel == Some(i))
                            .clicked()
                        {
                            sel = Some(i);
                        }
                    }
                    chain_link(ui);
                    chain_end(ui);
                });
            });
        if sel.is_some() {
            self.chain_sel = sel;
        }
    }

    fn folders(&mut self, ui: &mut egui::Ui, folders: &[String]) {
        egui::ScrollArea::vertical()
            .id_salt("seletor-pastas")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                // A search looks in every folder.
                let searching = !self.search.trim().is_empty();
                let all = self.folder.is_none() || searching;
                if folder_row(ui, tr("All"), 0.0, all).clicked() {
                    self.folder = None;
                    self.search.clear();
                }
                for f in folders {
                    let on = !searching && self.folder.as_ref() == Some(f);
                    if folder_row(ui, f, 18.0, on).clicked() {
                        self.folder = Some(f.clone());
                        self.search.clear();
                    }
                }
            });
    }

    #[allow(clippy::too_many_arguments)]
    fn list(
        &mut self,
        ui: &mut egui::Ui,
        all: &[EffectKind],
        ofx: &[std::sync::Arc<bv_ofx::PluginInfo>],
        vst: &[crate::vst::VstPlugin],
        packages: &[Package],
        packages_folder: &str,
    ) {
        // The search field: names in every folder.
        let field = ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .hint_text(tr("Search plug-ins"))
                .desired_width(f32::INFINITY),
        );
        if field.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.search.clear();
        }
        ui.add_space(4.0);
        let items = self.items(all, ofx, vst, packages, packages_folder);
        // Letters typed over the list pick the plug-in they begin.
        if !ui.ctx().egui_wants_keyboard_input() {
            let now = ui.input(|i| i.time);
            let typed: String = ui.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Text(t) => Some(t.as_str()),
                        _ => None,
                    })
                    .collect()
            });
            if !typed.trim().is_empty() {
                if now - self.typed_at > TYPE_AHEAD_PAUSE {
                    self.typed.clear();
                }
                self.typed.push_str(&typed);
                self.typed_at = now;
                let names: Vec<String> = items.iter().map(|it| it.name.clone()).collect();
                let current = items
                    .iter()
                    .position(|it| self.picked.as_ref() == Some(&it.pick));
                if let Some(i) = type_ahead(&names, current, &self.typed) {
                    self.picked = Some(items[i].pick.clone());
                    self.scroll_pick = true;
                }
            }
        }
        let mut add = None;
        let scroll = std::mem::take(&mut self.scroll_pick);
        egui::ScrollArea::vertical()
            .id_salt("seletor-lista")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(4.0, 2.0);
                    if items.is_empty() {
                        ui.label(
                            RichText::new(tr("No plug-in has that name")).color(theme::text_dim()),
                        );
                    }
                    for it in &items {
                        let selected = self.picked.as_ref() == Some(&it.pick);
                        let mut r = plugin_item(ui, &it.name, it.badge, selected);
                        if let Some(h) = &it.hover {
                            r = r.on_hover_text(h);
                        }
                        if selected && scroll {
                            r.scroll_to_me(Some(Align::Center));
                        }
                        if r.clicked() {
                            self.picked = Some(it.pick.clone());
                        }
                        if r.double_clicked() {
                            add = Some(it.pick.clone());
                        }
                    }
                });
            });
        if let Some(k) = add {
            self.add(&k, packages);
        }
    }

    /// The list's plug-ins, in its order: those of the chosen folder (all
    /// of them but the filter packages, with none), or those of every
    /// folder whose names have what is searched for.
    fn items(
        &self,
        all: &[EffectKind],
        ofx: &[std::sync::Arc<bv_ofx::PluginInfo>],
        vst: &[crate::vst::VstPlugin],
        packages: &[Package],
        packages_folder: &str,
    ) -> Vec<ListItem> {
        let query = search_fold(self.search.trim());
        let searching = !query.is_empty();
        let folder = self.folder.as_deref().filter(|_| !searching);
        let mut items = Vec::new();
        for &k in all
            .iter()
            .filter(|k| folder.is_none_or(|f| tr(k.category()) == f))
        {
            items.push(ListItem {
                pick: Pick::Kind(k),
                name: tr(k.name()).to_string(),
                badge: category_color(k.category()),
                hover: None,
            });
        }
        for p in ofx
            .iter()
            .filter(|p| folder.is_none_or(|f| crate::ofx::group_of(p) == f))
        {
            items.push(ListItem {
                pick: Pick::Ofx(p.identifier.clone()),
                name: crate::ofx::plugin_name(p),
                badge: category_color("OFX"),
                hover: Some(p.identifier.clone()),
            });
        }
        for p in vst {
            let own = if p.vst3 { VST_FOLDER } else { VST2_FOLDER };
            if folder.is_some_and(|f| f != own) {
                continue;
            }
            let about: Vec<&str> = [p.vendor.as_str(), p.category.as_str(), p.version.as_str()]
                .into_iter()
                .filter(|t| !t.is_empty())
                .collect();
            let hover = if about.is_empty() {
                p.path.display().to_string()
            } else {
                format!("{}\n{}", about.join(" · "), p.path.display())
            };
            items.push(ListItem {
                pick: Pick::Vst(p.clone()),
                name: if p.bits32 {
                    trf!("{} (32-bit)", p.name)
                } else {
                    p.name.clone()
                },
                badge: category_color(own),
                hover: Some(hover),
            });
        }
        // The filter packages: in their folder, or found by a search.
        if folder == Some(packages_folder) || searching {
            for p in packages {
                let names: Vec<String> = p.chain.iter().map(crate::ofx::fx_name).collect();
                items.push(ListItem {
                    pick: Pick::Package(p.name.clone()),
                    name: p.name.clone(),
                    badge: category_color("Package"),
                    hover: Some(names.join(" → ")),
                });
            }
        }
        if searching {
            items.retain(|it| search_fold(&it.name).contains(&query));
        }
        items
    }
}

/// Categorias do catálogo, na ordem em que aparecem.
pub fn categories(all: &[EffectKind]) -> Vec<&'static str> {
    let mut cats: Vec<&'static str> = Vec::new();
    for k in all {
        if !cats.contains(&k.category()) {
            cats.push(k.category());
        }
    }
    cats
}

fn describe(k: EffectKind) -> String {
    // The Modern Equalizer's: its global ones, then its bands.
    let specs = match k {
        EffectKind::ModernEq => &k.params()[..bv_timeline::fx::modern_eq::GLOBALS],
        _ => k.params(),
    };
    let mut names: Vec<&str> = specs
        .iter()
        .map(|p| tr(p.name))
        .filter(|n| !n.is_empty())
        .collect();
    if k == EffectKind::ModernEq {
        names.push(tr("up to eight bands"));
    }
    trf!(
        "{} ({}). Parameters: {}.",
        tr(k.name()),
        tr(k.category()),
        if names.is_empty() {
            tr("none").to_string()
        } else {
            names.join(", ")
        }
    )
}

// ---- peças de interface ---------------------------------------------------------

fn params_of(e: &Effect) -> Vec<f32> {
    (0..e.kind.params().len()).map(|j| e.param(j)).collect()
}

pub(crate) fn wrap180(a: f32) -> f32 {
    (a + 180.0).rem_euclid(360.0) - 180.0
}

/// A section of a property grid, as VEGAS's: a band across with its ⊟/⊞
/// box and its name, a click folding it.
pub(crate) fn section(
    ui: &mut egui::Ui,
    open: &mut bool,
    title: &str,
    body: impl FnOnce(&mut egui::Ui),
) {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::click());
    let painter = ui.painter();
    // VEGAS's band: #505050 on its #282828 panels.
    let band = crate::ofx_keys::mix(theme::panel(), theme::text(), 0.21);
    painter.rect_filled(
        rect,
        0.0,
        if resp.hovered() {
            crate::ofx_keys::mix(band, theme::text(), 0.08)
        } else {
            band
        },
    );
    let b = Rect::from_center_size(pos2(rect.left() + 9.5, rect.center().y), vec2(9.0, 9.0));
    painter.rect_filled(b, 0.0, theme::text());
    let st = Stroke::new(1.0, theme::panel());
    painter.hline(b.left() + 2.0..=b.right() - 2.0, b.center().y, st);
    if !*open {
        painter.vline(b.center().x, b.top() + 2.0..=b.bottom() - 2.0, st);
    }
    crate::widgets::fit_text(
        painter,
        pos2(rect.left() + 18.0, rect.center().y),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(13.0),
        theme::text_strong(),
        rect.width() - 22.0,
    );
    resp.widget_info(|| WidgetInfo::selected(WidgetType::CollapsingHeader, true, *open, title));
    if resp.clicked() {
        *open = !*open;
    }
    if *open {
        ui.add_space(1.0);
        body(ui);
        ui.add_space(1.0);
    }
}

/// The names' column of a property grid: the same in every section, so
/// the values line up down the grid, as VEGAS's.
const PROP_NAMES: f32 = 124.0;

/// The rows of a property grid, as VEGAS's: each a dash, its name and a
/// colon, then its value.
pub(crate) fn prop_grid(ui: &mut egui::Ui, id: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id)
        .num_columns(2)
        .min_col_width(PROP_NAMES)
        .max_col_width(PROP_NAMES.max(ui.available_width() - PROP_NAMES - 4.0))
        .spacing([2.0, 0.0])
        .min_row_height(17.0)
        .show(ui, body);
}

/// A row's name in a property grid: stepped in after a dash, cut short
/// with an ellipsis where it does not fit (VEGAS's "Maintain aspect…").
pub(crate) fn prop_name(ui: &mut egui::Ui, label: &str, enabled: bool) {
    let color = if enabled {
        theme::text()
    } else {
        theme::text_disabled()
    };
    // As wide as the names' column: a long name cut short.
    let (rect, _) = ui.allocate_exact_size(vec2(PROP_NAMES, 17.0), Sense::hover());
    let mut cell = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    cell.add_space(12.0);
    cell.label(egui::RichText::new("-").color(theme::text_dim()));
    cell.add_space(2.0);
    cell.add(
        egui::Label::new(egui::RichText::new(format!("{label}:")).color(color))
            .truncate()
            .selectable(false),
    );
}

/// `add`'s widget as a property grid's value, as VEGAS's: plain text at
/// rest, a box only under the pointer and to type in.
pub(crate) fn prop_value<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.x = 0.0;
        ui.spacing_mut().button_padding = vec2(2.0, 0.0);
        let w = &mut ui.visuals_mut().widgets;
        w.inactive.bg_fill = Color32::TRANSPARENT;
        w.inactive.weak_bg_fill = Color32::TRANSPARENT;
        w.inactive.bg_stroke = Stroke::NONE;
        w.inactive.fg_stroke.color = theme::text();
        w.hovered.weak_bg_fill = Color32::TRANSPARENT;
        w.hovered.bg_fill = Color32::TRANSPARENT;
        add(ui)
    })
    .inner
}

/// A row of a property grid: its name and its value.
pub(crate) fn prop(ui: &mut egui::Ui, label: &str, w: impl egui::Widget) -> egui::Response {
    prop_name(ui, label, true);
    let r = prop_value(ui, |ui| ui.add(w));
    ui.end_row();
    r
}

/// A row of a property grid whose value is picked from `options`: whether
/// it changed.
pub(crate) fn prop_choice<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    label: &str,
    id: &str,
    value: &mut T,
    options: &[(T, &str)],
) -> bool {
    prop_name(ui, label, true);
    let before = *value;
    let shown = options
        .iter()
        .find(|(v, _)| v == value)
        .map_or("", |(_, name)| *name);
    prop_value(ui, |ui| {
        // Plain, as VEGAS's: no arrow until it opens.
        egui::ComboBox::from_id_salt(id)
            .selected_text(shown)
            .width(0.0)
            .icon(|_, _, _, _| {})
            .show_ui(ui, |ui| {
                for (v, name) in options {
                    ui.selectable_value(value, *v, *name);
                }
            });
    });
    ui.end_row();
    *value != before
}

/// A Yes/No row of a property grid, as VEGAS's: whether it changed.
pub(crate) fn prop_yes_no(ui: &mut egui::Ui, label: &str, id: &str, value: &mut bool) -> bool {
    prop_choice(
        ui,
        label,
        id,
        value,
        &[(true, tr("Yes")), (false, tr("No"))],
    )
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Glyph {
    Icon(Icon),
    Pointer,
    Rotate,
    FromCenter,
    MoveFree,
    MoveX,
    MoveY,
    /// Track Motion's Prevent Movement (X) and (Y): the axis barred.
    NoMoveX,
    NoMoveY,
    /// Track Motion's Prevent Scaling (X) and (Y): the size held between
    /// two bars.
    NoScaleX,
    NoScaleY,
    /// 3D Track Motion's Prevent Movement (Z) and Prevent Scaling (Z): the
    /// same along a diagonal, the axis toward the viewer.
    NoMoveZ,
    NoScaleZ,
    /// Track Motion's Edit in Object Space: the object's own axes.
    Axes,
    /// The Pan/Crop masks' tools: Anchor Creation (a pen), Anchor Deletion
    /// (a pen with a minus), Split Tangent (an anchor with its tangents
    /// apart), Rectangle and Oval.
    Pen,
    PenMinus,
    SplitTangent,
    MaskRect,
    MaskOval,
    /// A picked mask's Positive and Negative (layers with a plus or a
    /// minus) and its feather Inside, Both ways or Outside.
    MaskPositive,
    MaskNegative,
    FeatherIn,
    FeatherBoth,
    FeatherOut,
}

fn paint_glyph(painter: &Painter, rect: Rect, g: Glyph, color: Color32) {
    let s = rect.width().min(rect.height());
    let o = rect.center() - vec2(s / 2.0, s / 2.0);
    let q = |x: f32, y: f32| pos2(o.x + x * s, o.y + y * s);
    let st = Stroke::new(1.3, color);
    let head = |b: Pos2, d: Vec2| {
        let n = d.rot90();
        let h = s * 0.22;
        painter.line_segment([b, b - d * h + n * h * 0.7], st);
        painter.line_segment([b, b - d * h - n * h * 0.7], st);
    };
    let arrow = |a: Pos2, b: Pos2| {
        painter.line_segment([a, b], st);
        head(b, (b - a).normalized());
    };
    match g {
        Glyph::Icon(i) => icons::paint(painter, rect, i, color),
        Glyph::Pointer => {
            painter.add(Shape::closed_line(
                vec![
                    q(0.24, 0.04),
                    q(0.24, 0.84),
                    q(0.43, 0.67),
                    q(0.56, 0.96),
                    q(0.68, 0.91),
                    q(0.55, 0.62),
                    q(0.8, 0.62),
                ],
                st,
            ));
        }
        Glyph::Rotate => {
            let c = q(0.5, 0.5);
            let r = s * 0.38;
            let (a0, a1) = (-60.0_f32, 210.0_f32);
            let pts: Vec<Pos2> = (0..=24)
                .map(|i| c + Vec2::angled((a0 + (a1 - a0) * i as f32 / 24.0).to_radians()) * r)
                .collect();
            let end = pts[pts.len() - 1];
            painter.add(Shape::line(pts, st));
            head(
                end,
                Vec2::angled(a1.to_radians() + std::f32::consts::FRAC_PI_2),
            );
        }
        Glyph::FromCenter => {
            painter.rect_stroke(
                Rect::from_center_size(q(0.5, 0.5), vec2(s * 0.26, s * 0.26)),
                0.0,
                st,
                StrokeKind::Middle,
            );
            for (x, y) in [(0.06, 0.06), (0.94, 0.06), (0.94, 0.94), (0.06, 0.94)] {
                arrow(q(0.5 + (x - 0.5) * 0.4, 0.5 + (y - 0.5) * 0.4), q(x, y));
            }
        }
        Glyph::MoveFree => {
            for (x, y) in [(0.5, 0.04), (0.96, 0.5), (0.5, 0.96), (0.04, 0.5)] {
                arrow(q(0.5, 0.5), q(x, y));
            }
        }
        Glyph::MoveX => {
            arrow(q(0.5, 0.5), q(0.96, 0.5));
            arrow(q(0.5, 0.5), q(0.04, 0.5));
        }
        Glyph::MoveY => {
            arrow(q(0.5, 0.5), q(0.5, 0.04));
            arrow(q(0.5, 0.5), q(0.5, 0.96));
        }
        Glyph::NoMoveX => {
            arrow(q(0.5, 0.5), q(0.96, 0.5));
            arrow(q(0.5, 0.5), q(0.04, 0.5));
            painter.line_segment([q(0.5, 0.18), q(0.5, 0.82)], Stroke::new(2.2, color));
        }
        Glyph::NoMoveY => {
            arrow(q(0.5, 0.5), q(0.5, 0.04));
            arrow(q(0.5, 0.5), q(0.5, 0.96));
            painter.line_segment([q(0.18, 0.5), q(0.82, 0.5)], Stroke::new(2.2, color));
        }
        Glyph::NoScaleX => {
            painter.line_segment([q(0.08, 0.2), q(0.08, 0.8)], st);
            painter.line_segment([q(0.92, 0.2), q(0.92, 0.8)], st);
            arrow(q(0.5, 0.5), q(0.84, 0.5));
            arrow(q(0.5, 0.5), q(0.16, 0.5));
        }
        Glyph::NoScaleY => {
            painter.line_segment([q(0.2, 0.08), q(0.8, 0.08)], st);
            painter.line_segment([q(0.2, 0.92), q(0.8, 0.92)], st);
            arrow(q(0.5, 0.5), q(0.5, 0.84));
            arrow(q(0.5, 0.5), q(0.5, 0.16));
        }
        Glyph::NoMoveZ => {
            arrow(q(0.5, 0.5), q(0.88, 0.12));
            arrow(q(0.5, 0.5), q(0.12, 0.88));
            painter.line_segment([q(0.24, 0.24), q(0.76, 0.76)], Stroke::new(2.2, color));
        }
        Glyph::NoScaleZ => {
            painter.line_segment([q(0.6, 0.04), q(0.96, 0.4)], st);
            painter.line_segment([q(0.04, 0.6), q(0.4, 0.96)], st);
            arrow(q(0.5, 0.5), q(0.72, 0.28));
            arrow(q(0.5, 0.5), q(0.28, 0.72));
        }
        Glyph::Axes => {
            arrow(q(0.2, 0.8), q(0.92, 0.56));
            arrow(q(0.2, 0.8), q(0.44, 0.08));
        }
        Glyph::Pen | Glyph::PenMinus => {
            // A nib: its point down-left, its slit and its hole.
            painter.add(Shape::closed_line(
                vec![
                    q(0.1, 0.9),
                    q(0.2, 0.52),
                    q(0.52, 0.2),
                    q(0.8, 0.48),
                    q(0.48, 0.8),
                ],
                st,
            ));
            painter.line_segment([q(0.1, 0.9), q(0.36, 0.64)], st);
            painter.circle_stroke(q(0.4, 0.6), s * 0.05, st);
            painter.line_segment([q(0.6, 0.12), q(0.88, 0.4)], st);
            if g == Glyph::PenMinus {
                painter.line_segment([q(0.62, 0.86), q(0.96, 0.86)], Stroke::new(1.8, color));
            }
        }
        Glyph::SplitTangent => {
            let c = q(0.5, 0.62);
            painter.line_segment([c, q(0.1, 0.22)], st);
            painter.line_segment([c, q(0.92, 0.36)], st);
            painter.circle_filled(q(0.1, 0.22), s * 0.08, color);
            painter.circle_filled(q(0.92, 0.36), s * 0.08, color);
            painter.rect_filled(
                Rect::from_center_size(c, vec2(s * 0.2, s * 0.2)),
                0.0,
                color,
            );
        }
        Glyph::MaskRect => {
            painter.rect_stroke(
                Rect::from_min_max(q(0.1, 0.22), q(0.9, 0.78)),
                0.0,
                st,
                StrokeKind::Middle,
            );
        }
        Glyph::MaskOval => {
            let pts: Vec<Pos2> = (0..32)
                .map(|i| {
                    let a = i as f32 / 32.0 * std::f32::consts::TAU;
                    q(0.5 + 0.42 * a.cos(), 0.5 + 0.3 * a.sin())
                })
                .collect();
            painter.add(Shape::closed_line(pts, st));
        }
        Glyph::MaskPositive | Glyph::MaskNegative => {
            painter.rect_stroke(
                Rect::from_min_max(q(0.04, 0.3), q(0.62, 0.88)),
                1.0,
                st,
                StrokeKind::Middle,
            );
            painter.rect_stroke(
                Rect::from_min_max(q(0.2, 0.12), q(0.78, 0.7)),
                1.0,
                st,
                StrokeKind::Middle,
            );
            let b = Stroke::new(1.6, color);
            painter.line_segment([q(0.64, 0.88), q(0.98, 0.88)], b);
            if g == Glyph::MaskPositive {
                painter.line_segment([q(0.81, 0.71), q(0.81, 1.0)], b);
            }
        }
        Glyph::FeatherIn | Glyph::FeatherBoth | Glyph::FeatherOut => {
            // A square, solid or dashed inside and outside it as the
            // feather goes.
            let inner = Rect::from_center_size(q(0.5, 0.5), vec2(s * 0.36, s * 0.36));
            let mid = Rect::from_center_size(q(0.5, 0.5), vec2(s * 0.62, s * 0.62));
            let outer = Rect::from_center_size(q(0.5, 0.5), vec2(s * 0.9, s * 0.9));
            let dashed = |r: Rect| {
                let pts = vec![
                    r.left_top(),
                    r.right_top(),
                    r.right_bottom(),
                    r.left_bottom(),
                    r.left_top(),
                ];
                painter.extend(Shape::dashed_line(&pts, Stroke::new(1.0, color), 2.0, 2.0));
            };
            let solid = |r: Rect| painter.rect_stroke(r, 0.0, st, StrokeKind::Middle);
            match g {
                Glyph::FeatherIn => {
                    solid(mid);
                    dashed(inner);
                }
                Glyph::FeatherOut => {
                    solid(mid);
                    dashed(outer);
                }
                _ => {
                    dashed(inner);
                    solid(mid);
                    dashed(outer);
                }
            }
        }
    }
}

pub(crate) fn glyph_button(
    ui: &mut egui::Ui,
    g: Glyph,
    on: bool,
    enabled: bool,
    tip: &str,
) -> egui::Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), sense);
    let painter = ui.painter();
    let fill = if on {
        theme::banana_dim()
    } else if enabled && resp.hovered() {
        theme::hover()
    } else {
        Color32::TRANSPARENT
    };
    painter.rect_filled(rect, 3.0, fill);
    let color = if !enabled {
        theme::text_disabled()
    } else if on {
        theme::banana()
    } else {
        theme::text()
    };
    paint_glyph(painter, rect.shrink(5.0), g, color);
    resp.widget_info(|| WidgetInfo::selected(WidgetType::Button, enabled, on, tip));
    resp.on_hover_text(tip)
}

fn icon_button(ui: &mut egui::Ui, icon: Icon, enabled: bool, tip: &str) -> egui::Response {
    glyph_button(ui, Glyph::Icon(icon), false, enabled, tip)
}

pub(crate) fn toggle(ui: &mut egui::Ui, v: &mut bool, g: Glyph, tip: &str) {
    if glyph_button(ui, g, *v, true, tip).clicked() {
        *v = !*v;
    }
}

/// The width of the dots at the chain's ends and of its links.
const CHAIN_END: f32 = 12.0;
const CHAIN_LINK: f32 = 16.0;
/// The room the buttons at the end of the FX window's chain row take
/// (Plug-In Chain, Save and Remove: 24 wide, 4 apart), with a gap before
/// them.
const CHAIN_TOOLS: f32 = 3.0 * 24.0 + 2.0 * 4.0 + 8.0;
/// The width of the chain row's scroll buttons.
const CHAIN_ARROW: f32 = 14.0;
/// How fast a button dragged past the chain's ends scrolls it (points a
/// second).
const CHAIN_DRAG_SCROLL: f32 = 360.0;

/// Bolinha de entrada/saída da cadeia.
fn chain_end(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(CHAIN_END, 26.0), Sense::hover());
    ui.painter().circle(
        rect.center(),
        4.5,
        theme::panel_dark(),
        Stroke::new(1.2, theme::text_dim()),
    );
}

fn chain_link(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(CHAIN_LINK, 26.0), Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(1.2, theme::text_dim()),
    );
}

/// A scroll button of the chain's row: a triangle toward the end it
/// scrolls to (`ahead`: the chain's output).
fn chain_arrow(ui: &mut egui::Ui, ahead: bool, enabled: bool, tip: &str) -> egui::Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(CHAIN_ARROW, 24.0), sense);
    let painter = ui.painter();
    if enabled && resp.hovered() {
        painter.rect_filled(rect, 3.0, theme::hover());
    }
    let c = rect.center();
    let s = if ahead { 1.0 } else { -1.0 };
    painter.add(Shape::convex_polygon(
        vec![
            c + vec2(-3.0 * s, -5.0),
            c + vec2(3.0 * s, 0.0),
            c + vec2(-3.0 * s, 5.0),
        ],
        if enabled {
            theme::text()
        } else {
            theme::text_disabled()
        },
        Stroke::NONE,
    ));
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, tip));
    resp.on_hover_text(tip)
}

/// Where the chain's row scrolls to from `offset` to show the button at
/// `span` (its left and right along the chain) and its links: as little
/// as it must, all the way to an end that little more would show. `shown`
/// is how much of the chain shows, `full` its length.
fn chain_reveal(span: (f32, f32), offset: f32, shown: f32, full: f32) -> f32 {
    let (a, b) = (span.0 - CHAIN_LINK, span.1 + CHAIN_LINK);
    let to = if a < offset || b - a > shown {
        a
    } else if b > offset + shown {
        b - shown
    } else {
        offset
    };
    let end = (full - shown).max(0.0);
    if to <= CHAIN_END + 0.5 {
        0.0
    } else if to >= end - CHAIN_END - 0.5 {
        end
    } else {
        to
    }
}

/// Where the chain's row scrolls to from `offset` by its scroll buttons:
/// the next button and its link come into view, toward the chain's output
/// (`ahead`) or its input, the rest of the way when only its dot is left.
/// `spans` are the buttons' left and right along the chain.
fn chain_step(spans: &[(f32, f32)], offset: f32, shown: f32, full: f32, ahead: bool) -> f32 {
    let end = (full - shown).max(0.0);
    if ahead {
        let to = spans
            .iter()
            .map(|s| s.1 + CHAIN_LINK - shown)
            .find(|&to| to > offset + 0.5)
            .unwrap_or(end);
        if to >= end - CHAIN_END - 0.5 { end } else { to }
    } else {
        let to = spans
            .iter()
            .rev()
            .map(|s| s.0 - CHAIN_LINK)
            .find(|&to| to < offset - 0.5)
            .unwrap_or(0.0);
        if to <= CHAIN_END + 0.5 { 0.0 } else { to }
    }
}

/// A chain button's width: its name, and the box that enables it.
fn chain_node_width(ui: &egui::Ui, label: &str, check: bool) -> f32 {
    let name =
        ui.painter()
            .layout_no_wrap(label.to_string(), FontId::proportional(13.0), theme::text());
    name.size().x + 20.0 + if check { 18.0 } else { 0.0 }
}

/// Um plug-in na cadeia; com `enabled`, mostra a caixa de ativar.
fn chain_node(
    ui: &mut egui::Ui,
    label: &str,
    enabled: Option<&mut bool>,
    selected: bool,
) -> egui::Response {
    let on = enabled.as_deref().is_none_or(|e| *e);
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        FontId::proportional(13.0),
        if on { theme::text() } else { theme::text_dim() },
    );
    let check_w = if enabled.is_some() { 18.0 } else { 0.0 };
    let size = vec2(chain_node_width(ui, label, enabled.is_some()), 24.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let painter = ui.painter();
    let fill = if selected {
        theme::banana_dim()
    } else if resp.hovered() {
        theme::hover()
    } else {
        theme::panel_dark()
    };
    let stroke = Stroke::new(
        1.0,
        if selected {
            theme::banana()
        } else {
            theme::separator()
        },
    );
    painter.rect(rect, 3.0, fill, stroke, StrokeKind::Inside);
    if let Some(en) = enabled {
        let b = Rect::from_center_size(pos2(rect.left() + 13.0, rect.center().y), vec2(11.0, 11.0));
        painter.rect(
            b,
            2.0,
            theme::panel_dark(),
            Stroke::new(1.0, theme::text_dim()),
            StrokeKind::Inside,
        );
        if *en {
            painter.add(Shape::line(
                vec![
                    b.left_center() + vec2(2.0, 0.0),
                    b.center_bottom() + vec2(-1.0, -2.5),
                    b.right_top() + vec2(-2.0, 2.0),
                ],
                Stroke::new(1.5, theme::banana()),
            ));
        }
        if resp.clicked()
            && resp
                .interact_pointer_pos()
                .is_some_and(|p| b.expand(3.0).contains(p))
        {
            *en = !*en;
        }
    }
    let text_pos = pos2(
        rect.left() + 10.0 + check_w,
        rect.center().y - galley.size().y / 2.0,
    );
    painter.galley(text_pos, galley, theme::text());
    resp.widget_info(|| WidgetInfo::selected(WidgetType::SelectableLabel, true, selected, label));
    resp
}

/// The place a button let go at `x` goes to among buttons at `rects`
/// (before the first whose middle is past it; after the last: their
/// count).
fn drop_slot(rects: &[Rect], x: f32) -> usize {
    rects.iter().take_while(|r| r.center().x < x).count()
}

/// The chain made by moving button `from` of `seq` (`None` the Pan/Crop,
/// `Some(i)` effect `i` of `chain`) to place `to` (before the button
/// there; past the last: the end): the effects in their new order, how
/// many come before the Pan/Crop, and the button moved, to show. `None`:
/// nothing moves.
fn reorder(
    chain: &[Effect],
    seq: &[Option<usize>],
    from: usize,
    to: usize,
) -> Option<(Vec<Effect>, usize, Node)> {
    if from >= seq.len() || to == from || to == from + 1 {
        return None;
    }
    let mut order = seq.to_vec();
    let node = order.remove(from);
    let to = if to > from { to - 1 } else { to };
    order.insert(to.min(order.len()), node);
    let effects: Vec<Effect> = order.iter().flatten().map(|&i| chain[i].clone()).collect();
    let at = order.iter().position(Option::is_none).unwrap_or(0);
    let shown = match node {
        None => Node::PanCrop,
        Some(i) => Node::Effect(order.iter().flatten().position(|&k| k == i).unwrap_or(0)),
    };
    Some((effects, at, shown))
}

/// Painel escuro de tamanho fixo (colunas do seletor).
fn panel(ui: &mut egui::Ui, size: Vec2, body: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(size, Layout::top_down(Align::LEFT), |ui| {
        egui::Frame::new()
            .fill(theme::panel_dark())
            .stroke(Stroke::new(1.0, theme::separator()))
            .inner_margin(4)
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                body(ui);
            });
    });
}

fn folder_row(ui: &mut egui::Ui, label: &str, indent: f32, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 2.0, theme::banana_dim());
    } else if resp.hovered() {
        painter.rect_filled(rect, 2.0, theme::hover());
    }
    let x = rect.left() + 4.0 + indent;
    if indent == 0.0 {
        // Raiz aberta: triângulo para baixo.
        let c = pos2(x + 4.0, rect.center().y);
        painter.add(Shape::convex_polygon(
            vec![
                c + vec2(-4.0, -2.0),
                c + vec2(4.0, -2.0),
                c + vec2(0.0, 3.0),
            ],
            theme::text_dim(),
            Stroke::NONE,
        ));
    }
    let icon = Rect::from_min_size(pos2(x + 10.0, rect.center().y - 7.0), vec2(14.0, 14.0));
    icons::paint(painter, icon, Icon::Folder, theme::banana());
    crate::widgets::fit_text(
        painter,
        pos2(icon.right() + 6.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.0),
        theme::text(),
        rect.right() - icon.right() - 10.0,
    );
    resp.widget_info(|| WidgetInfo::selected(WidgetType::SelectableLabel, true, selected, label));
    resp
}

fn category_color(cat: &str) -> Color32 {
    match cat {
        "Color" => Color32::from_rgb(0xD8, 0x7A, 0x2E),
        "Focus and Light" => Color32::from_rgb(0x3E, 0x8E, 0xD8),
        "Style" => Color32::from_rgb(0x9A, 0x5C, 0xC8),
        "Compositing" => Color32::from_rgb(0x3E, 0xA8, 0x5C),
        "Equalization" => Color32::from_rgb(0x2E, 0xA8, 0xA0),
        "Dynamics" => Color32::from_rgb(0xC8, 0x4E, 0x4E),
        "VST3" | "VST" => Color32::from_rgb(0xB8, 0x9A, 0x2E),
        "Package" => Color32::from_rgb(0x5E, 0x8C, 0x48),
        _ => Color32::from_rgb(0x7A, 0x6C, 0xD0),
    }
}

/// A catalog item: an "fx" badge in its category's color, and its name.
fn plugin_item(
    ui: &mut egui::Ui,
    name: &str,
    badge_color: Color32,
    selected: bool,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(200.0, 22.0), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 2.0, theme::banana_dim());
    } else if resp.hovered() {
        painter.rect_filled(rect, 2.0, theme::hover());
    }
    let badge = Rect::from_min_size(
        pos2(rect.left() + 4.0, rect.center().y - 7.0),
        vec2(18.0, 14.0),
    );
    painter.rect_filled(badge, 3.0, badge_color);
    painter.text(
        badge.center(),
        Align2::CENTER_CENTER,
        "fx",
        FontId::proportional(10.0),
        Color32::WHITE,
    );
    painter.text(
        pos2(badge.right() + 7.0, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(13.0),
        theme::text(),
    );
    resp.widget_info(|| WidgetInfo::selected(WidgetType::SelectableLabel, true, selected, name));
    resp
}

pub(crate) fn paint_key(p: &Painter, c: Pos2, curve: KeyCurve, fill: Color32) {
    let st = Stroke::new(1.0, theme::panel_dark());
    match curve {
        KeyCurve::Hold => {
            p.rect(
                Rect::from_center_size(c, vec2(9.0, 9.0)),
                0.0,
                fill,
                st,
                StrokeKind::Middle,
            );
        }
        _ => {
            p.add(Shape::convex_polygon(
                vec![
                    c + vec2(0.0, -6.0),
                    c + vec2(6.0, 0.0),
                    c + vec2(0.0, 6.0),
                    c + vec2(-6.0, 0.0),
                ],
                fill,
                st,
            ));
        }
    }
    if curve == KeyCurve::Linear {
        p.hline(
            c.x - 3.0..=c.x + 3.0,
            c.y,
            Stroke::new(1.0, theme::panel_dark()),
        );
    }
}

/// A VST plug-in's chain button, as VEGAS names it: its name, its kind
/// and its processor's width ("Modern Equalizer(VST3, 64 Bit)").
fn vst_node_name(name: &str, v: &bv_timeline::VstFx) -> String {
    use std::collections::HashMap;
    use std::sync::Mutex;
    // Each file's width, read once (its header).
    static BITS: Mutex<Option<HashMap<std::path::PathBuf, u32>>> = Mutex::new(None);
    let bits = {
        let mut known = BITS.lock().unwrap_or_else(|e| e.into_inner());
        *known
            .get_or_insert_with(HashMap::new)
            .entry(v.path.clone())
            .or_insert_with(|| {
                let narrow = match v.format {
                    bv_timeline::VstFormat::Vst3 => bv_vst3::is_32bit(&v.path),
                    bv_timeline::VstFormat::Vst2 => {
                        bv_vst2::machine(&v.path) == Some(bv_vst2::Machine::X86)
                    }
                };
                if narrow || cfg!(target_pointer_width = "32") {
                    32
                } else {
                    64
                }
            })
    };
    let kind = match v.format {
        bv_timeline::VstFormat::Vst3 => "VST3",
        bv_timeline::VstFormat::Vst2 => "VST",
    };
    format!("{name}({kind}, {bits} Bit)")
}

/// A Columns button: `n` bars, lit when on.
pub(crate) fn columns_button(ui: &mut egui::Ui, n: u8, on: bool, enabled: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(
        vec2(24.0, 20.0),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    let painter = ui.painter();
    if on {
        painter.rect_filled(rect, 2.0, theme::selection());
    } else if resp.hovered() && enabled {
        painter.rect_filled(rect, 2.0, theme::hover());
    }
    let color = if !enabled {
        theme::text_disabled()
    } else if on {
        egui::Color32::WHITE
    } else {
        theme::text()
    };
    // Its bars, side by side in the middle.
    let (bar, gap) = (3.0, 2.0);
    let w = n as f32 * bar + (n as f32 - 1.0) * gap;
    for i in 0..n {
        let x = rect.center().x - w / 2.0 + i as f32 * (bar + gap);
        painter.rect_filled(
            egui::Rect::from_min_size(egui::pos2(x, rect.center().y - 6.0), vec2(bar, 12.0)),
            0.0,
            color,
        );
    }
    let tip = trn!(n as usize, "{} Column", "{} Columns", n);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &tip));
    if enabled {
        resp.on_hover_text(tip)
    } else {
        resp.on_hover_text(tr("Only a plug-in's settings take columns"))
    }
}

/// The bar between a plug-in's settings and its keyframe controller, and
/// the least room the settings keep over it.
const SPLITTER: f32 = 6.0;
const MIN_SETTINGS: f32 = 60.0;
/// The window's foot with VEGAS's Animate button.
const FOOTER: f32 = 32.0;

/// The window's foot under a plug-in that animates as a whole: VEGAS's
/// Animate button, on the left. Whether it was clicked.
fn animate_footer(ui: &mut egui::Ui) -> bool {
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), FOOTER), egui::Sense::hover());
    let mut clicked = false;
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(rect.shrink2(vec2(6.0, 0.0))),
        |ui| {
            ui.horizontal_centered(|ui| {
                clicked = ui
                    .add(egui::Button::new(tr("Animate")).min_size(vec2(72.0, 22.0)))
                    .on_hover_text(tr("Animate the effect: its keyframe controller opens"))
                    .clicked();
            });
        },
    );
    clicked
}

/// The bar between a plug-in's settings and its keyframe controller
/// (`tray` high, at most `most`): dragged, it gives the controller the
/// height returned.
fn splitter(ui: &mut egui::Ui, tray: f32, most: f32) -> Option<f32> {
    let (bar, resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), SPLITTER), egui::Sense::drag());
    let hot = resp.hovered() || resp.dragged();
    if hot {
        // A window's divider, as VEGAS's split bars.
        cursors::set(ui.ctx(), Cursor::SplitterNs);
    }
    ui.painter().hline(
        bar.x_range(),
        bar.center().y,
        Stroke::new(
            if hot { 2.0 } else { 1.0 },
            if hot {
                theme::banana_dim()
            } else {
                theme::separator()
            },
        ),
    );
    let resp = resp.on_hover_text(tr("Drag to resize the keyframe controller"));
    (resp.dragged() && resp.drag_delta().y != 0.0)
        .then(|| (tray - resp.drag_delta().y).clamp(crate::ofx_keys::MIN_HEIGHT, most))
}

// ---- geometria do F --------------------------------------------------------------

pub(crate) fn box_point(sp: &Space, vp: &Viewport, sx: f32, sy: f32) -> Pos2 {
    let d = pancrop::rot((sx * vp.w / 2.0, sy * vp.h / 2.0), vp.angle_deg);
    sp.screen((vp.cx + d.0, vp.cy + d.1))
}

/// Ponto da tela no sistema do F (sem rotação, origem no centro), em
/// pixels da mídia.
pub(crate) fn local(sp: &Space, vp: &Viewport, p: Pos2) -> (f32, f32) {
    let m = sp.media(p);
    pancrop::rot((m.0 - vp.cx, m.1 - vp.cy), -vp.angle_deg)
}

pub(crate) fn angle_of(sp: &Space, vp: &Viewport, p: Pos2) -> f32 {
    (p - sp.screen((vp.cx, vp.cy))).angle()
}

/// The rotation circle's radius on the screen: VEGAS's, 0.55 of the box's
/// diagonal (round the box, its corners inside).
fn rotation_radius(sp: &Space, vp: &Viewport) -> f32 {
    0.55 * vp.w.hypot(vp.h) * sp.k
}

pub(crate) fn hit(sp: &Space, vp: &Viewport, p: Pos2, rotation: bool) -> Hit {
    for (sx, sy) in HANDLES {
        if (box_point(sp, vp, sx, sy) - p).length() <= 7.0 {
            return Hit::Handle(sx, sy);
        }
    }
    let l = local(sp, vp, p);
    if l.0.abs() <= vp.w / 2.0 && l.1.abs() <= vp.h / 2.0 {
        return Hit::Inside;
    }
    // Round the box, up to its circle (and a little past its line), the
    // box turns, as VEGAS's.
    let c = sp.screen((vp.cx, vp.cy));
    if rotation && (p - c).length() <= rotation_radius(sp, vp) + 6.0 {
        Hit::Rotate
    } else {
        Hit::Outside
    }
}

/// The pointer over a handle of a Pan/Crop or Track Motion box, as VEGAS
/// chooses it: the handle's own — ↔ on the left and right sides, ↘ on the
/// top-left and bottom-right corners, ↕ on the top and bottom sides, ↗ on
/// the other two corners —, a step further round that list (↔ ↘ ↕ ↗) for
/// each 45° the box is turned clockwise on the screen (rounded, the angle
/// taken within a turn first), and a corner's two steps more when the box
/// is mirrored (its width and height of different signs). `handle` is in
/// the box's own coordinates, its mirrors included (−1 left or top, 1
/// right or bottom, 0 a side's middle); `angle` in radians, clockwise on
/// the screen.
pub(crate) fn handle_cursor(handle: (f32, f32), angle: f64, mirrored: bool) -> Cursor {
    const ROUND: [Cursor; 4] = [
        Cursor::ResizeEw,
        Cursor::ResizeNwse,
        Cursor::ResizeNs,
        Cursor::ResizeNesw,
    ];
    let (sx, sy) = handle;
    let corner = sx != 0.0 && sy != 0.0;
    let own: i64 = match (sx != 0.0, sy != 0.0) {
        (true, false) => 0,
        (false, _) => 2,
        (true, true) if (sx > 0.0) == (sy > 0.0) => 1,
        (true, true) => 3,
    };
    // From 0 to 8 steps (an angle that is not a number: none).
    let steps =
        (angle.rem_euclid(std::f64::consts::TAU) * 4.0 / std::f64::consts::PI).round() as i64;
    let mirror = if corner && mirrored { 2 } else { 0 };
    ROUND[(own + steps + mirror).rem_euclid(4) as usize]
}

/// [`handle_cursor`] for the handle `sx`, `sy` of a box as the workspaces
/// find it — on its outline as drawn, its mirrors aside —, the box turned
/// `angle_deg` and mirrored `flips` (across, up and down). A mirror puts
/// the box's own corners on the other side: VEGAS's two steps more for a
/// mirrored box's corners bring the pointer back to the drawn corner's.
pub(crate) fn box_handle_cursor(sx: f32, sy: f32, angle_deg: f32, flips: (bool, bool)) -> Cursor {
    let own = (
        if flips.0 { -sx } else { sx },
        if flips.1 { -sy } else { sy },
    );
    handle_cursor(own, f64::from(angle_deg).to_radians(), flips.0 != flips.1)
}

/// The pointer over what [`hit`] found on a Pan/Crop or Track Motion box
/// (`pivot`: on its turning center's small circle, where the handles come
/// first), as VEGAS's: a handle's arrows ([`box_handle_cursor`]), the
/// center's, the box moved inside it, turned round it; none far from it.
pub(crate) fn box_cursor(
    hit: Hit,
    pivot: bool,
    angle_deg: f32,
    flips: (bool, bool),
) -> Option<Cursor> {
    match hit {
        Hit::Handle(sx, sy) => Some(box_handle_cursor(sx, sy, angle_deg, flips)),
        _ if pivot => Some(Cursor::MoveCenter),
        Hit::Rotate => Some(Cursor::Rotate),
        Hit::Inside => Some(Cursor::MoveFrame),
        Hit::Outside => None,
    }
}

/// The Pan/Crop workspace's pointer, as VEGAS's: a drag's own while it
/// goes on (the one its press showed; the view dragged, the closed hand);
/// the zoom tool's magnifier, with − while its right button is down (or
/// Shift, which zooms out too); else what is under it on the box
/// ([`box_cursor`]; `hover`: what [`hit`] found and whether it is on the
/// turning center), the open hand far from it, where the view drags from.
/// None while the pointer is elsewhere.
fn pan_cursor(
    tool: Tool,
    drag: Option<&Drag>,
    hover: Option<(Hit, bool)>,
    zoom_out: bool,
    angle_deg: f32,
    flips: (bool, bool),
) -> Option<Cursor> {
    if let Some(d) = drag {
        return Some(d.cursor(flips));
    }
    let (hit, pivot) = hover?;
    Some(match tool {
        Tool::Zoom if zoom_out => Cursor::ZoomOut,
        Tool::Zoom => Cursor::ZoomIn,
        Tool::Normal => box_cursor(hit, pivot, angle_deg, flips).unwrap_or(Cursor::HandOpen),
    })
}

/// Novo F ao arrastar a alça (`sx`, `sy` ∈ {−1, 0, 1}) até `local` (o
/// ponteiro no sistema do F original, em pixels da mídia).
pub(crate) fn resize(
    v0: &Viewport,
    local: (f32, f32),
    sx: f32,
    sy: f32,
    lock: bool,
    from_center: bool,
) -> Viewport {
    let (hw0, hh0) = (v0.w / 2.0, v0.h / 2.0);
    let (mut hw, mut hh) = (hw0, hh0);
    if from_center {
        if sx != 0.0 {
            hw = (local.0 * sx).max(1.0);
        }
        if sy != 0.0 {
            hh = (local.1 * sy).max(1.0);
        }
    } else {
        // A borda oposta fica parada.
        if sx != 0.0 {
            hw = ((local.0 * sx + hw0) / 2.0).max(1.0);
        }
        if sy != 0.0 {
            hh = ((local.1 * sy + hh0) / 2.0).max(1.0);
        }
    }
    if lock {
        if sx != 0.0 && sy != 0.0 {
            let s = (hw / hw0).max(hh / hh0);
            hw = hw0 * s;
            hh = hh0 * s;
        } else if sx != 0.0 {
            hh = hw * hh0 / hw0;
        } else {
            hw = hh * hw0 / hh0;
        }
    }
    let shift = if from_center {
        (0.0, 0.0)
    } else {
        (sx * (hw - hw0), sy * (hh - hh0))
    };
    let d = pancrop::rot(shift, v0.angle_deg);
    Viewport {
        cx: v0.cx + d.0,
        cy: v0.cy + d.1,
        w: hw * 2.0,
        h: hh * 2.0,
        angle_deg: v0.angle_deg,
    }
}

/// A resize (`v`, what [`resize`] made of `v0` dragged by the handle `sx`,
/// `sy`) as Enable Snapping has it: the edges dragged on the grid's lines
/// (`step` apart each way, in the media's pixels, from its corner) — with
/// the aspect locked the one nearer its line, the other side following —;
/// a turned box, whose edges cross the lines, sized in whole steps.
pub(crate) fn snap_resize(
    v0: &Viewport,
    v: &Viewport,
    sx: f32,
    sy: f32,
    lock: bool,
    from_center: bool,
    step: (f32, f32),
) -> Viewport {
    let (hw0, hh0) = (v0.w / 2.0, v0.h / 2.0);
    let near = |e: f32, g: f32| (e / g).round() * g;
    let turned = v0
        .angle_deg
        .rem_euclid(360.0)
        .min(360.0 - v0.angle_deg.rem_euclid(360.0))
        > 0.01;
    // A half size with its dragged edge (`s` its side) on a line.
    let half = |c0: f32, h0: f32, h: f32, s: f32, g: f32| -> f32 {
        let g = g.max(1e-3);
        let snapped = if turned {
            near(2.0 * h, g) / 2.0
        } else if from_center {
            (near(c0 + s * h, g) - c0) * s
        } else {
            // The other edge stays where it was.
            let fixed = c0 - s * h0;
            (near(fixed + s * 2.0 * h, g) - fixed) * s / 2.0
        };
        snapped.max(g / 2.0)
    };
    let (hw, hh) = (v.w / 2.0, v.h / 2.0);
    let x = |hw: f32| half(v0.cx, hw0, hw, sx, step.0);
    let y = |hh: f32| half(v0.cy, hh0, hh, sy, step.1);
    let (hw, hh) = match (sx != 0.0, sy != 0.0, lock) {
        (true, true, true) => {
            // The side nearer its line snaps; the other keeps the aspect.
            let (by_x, by_y) = (x(hw), y(hh));
            if (by_x / hw - 1.0).abs() <= (by_y / hh - 1.0).abs() {
                (by_x, by_x * hh0 / hw0)
            } else {
                (by_y * hw0 / hh0, by_y)
            }
        }
        (true, false, true) => {
            let w = x(hw);
            (w, w * hh0 / hw0)
        }
        (false, true, true) => {
            let h = y(hh);
            (h * hw0 / hh0, h)
        }
        (dx, dy, false) => (if dx { x(hw) } else { hw }, if dy { y(hh) } else { hh }),
        (false, false, true) => (hw, hh),
    };
    let shift = if from_center {
        (0.0, 0.0)
    } else {
        (sx * (hw - hw0), sy * (hh - hh0))
    };
    let d = pancrop::rot(shift, v0.angle_deg);
    Viewport {
        cx: v0.cx + d.0,
        cy: v0.cy + d.1,
        w: hw * 2.0,
        h: hh * 2.0,
        angle_deg: v0.angle_deg,
    }
}

/// VEGAS's grid over a workspace (`rect`): the media's frame repeated, in
/// lines, and a dot at each `parts`th of it (fewer where they would crowd).
pub(crate) fn paint_grid(painter: &Painter, rect: Rect, sp: &Space, parts: f32) {
    let o = sp.screen((0.0, 0.0));
    let tile = vec2(sp.src.0 * sp.k, sp.src.1 * sp.k);
    if tile.x < 4.0 || tile.y < 4.0 {
        return;
    }
    // The first line or dot from the left or top, `step` apart.
    let first = |o: f32, from: f32, step: f32| o - ((o - from) / step).floor() * step;
    let line = Stroke::new(1.0, WS_LINE);
    let mut x = first(o.x, rect.left(), tile.x);
    while x <= rect.right() {
        painter.vline(painter.round_to_pixel_center(x), rect.y_range(), line);
        x += tile.x;
    }
    let mut y = first(o.y, rect.top(), tile.y);
    while y <= rect.bottom() {
        painter.hline(rect.x_range(), painter.round_to_pixel_center(y), line);
        y += tile.y;
    }
    let mut step = tile / parts.max(1.0);
    while step.x < 6.0 {
        step.x *= 2.0;
    }
    while step.y < 6.0 {
        step.y *= 2.0;
    }
    let mut dots = Mesh::default();
    let mut x = first(o.x, rect.left(), step.x);
    while x <= rect.right() {
        let px = painter.round_to_pixel_center(x) - 0.5;
        let mut y = first(o.y, rect.top(), step.y);
        while y <= rect.bottom() {
            let py = painter.round_to_pixel_center(y) - 0.5;
            dots.add_colored_rect(Rect::from_min_size(pos2(px, py), vec2(1.0, 1.0)), WS_DOT);
            y += step.y;
        }
        x += step.x;
    }
    painter.add(Shape::mesh(dots));
}

/// The box, as VEGAS draws it over the workspace: its outline, the
/// rotation circle around it, the letter F (its turn and flips) and its
/// middle in VEGAS's lines (see [`ants`]), and its eight handles — all
/// showing over any picture, light or dark.
pub(crate) fn paint_f_box(
    painter: &Painter,
    sp: &Space,
    vp: &Viewport,
    flips: (bool, bool),
    turns: (f32, f32),
    rotation: bool,
) {
    // Unturned, on the pixels' middles: a pixel wide and sharp.
    let snap = |p: Pos2| {
        if vp.angle_deg == 0.0 {
            pos2(
                painter.round_to_pixel_center(p.x),
                painter.round_to_pixel_center(p.y),
            )
        } else {
            p
        }
    };
    let mut ring: Vec<Pos2> = [
        (-1.0, -1.0),
        (1.0, -1.0),
        (1.0, 1.0),
        (-1.0, 1.0),
        (-1.0, -1.0),
    ]
    .into_iter()
    .map(|(sx, sy)| snap(box_point(sp, vp, sx, sy)))
    .collect();
    ants(painter, &ring);
    let c = sp.screen((vp.cx, vp.cy));
    if rotation {
        let r = rotation_radius(sp, vp);
        ring = (0..=128)
            .map(|i| c + Vec2::angled(i as f32 / 128.0 * std::f32::consts::TAU) * r)
            .collect();
        ants(painter, &ring);
    }
    // The F in the box's middle, 0.81 of its shorter side tall.
    let s = vp.w.min(vp.h);
    let (fw, fh) = (0.54 * s, 0.81 * s);
    let (flip_x, flip_y) = (
        if flips.0 { -1.0 } else { 1.0 },
        if flips.1 { -1.0 } else { 1.0 },
    );
    let f: Vec<Pos2> = F_SHAPE
        .iter()
        .chain(std::iter::once(&F_SHAPE[0]))
        .map(|&(x, y)| {
            let d = pancrop::rot(
                ((x - 0.5) * fw * flip_x, (y - 0.5) * fh * flip_y),
                vp.angle_deg,
            );
            snap(sp.screen((vp.cx + d.0, vp.cy + d.1)))
        })
        .collect();
    ants(painter, &f);
    // The small circle where the box turns: its middle, or its pivot
    // (Rotation's X and Y Center).
    let turns = sp.screen(turns);
    let middle: Vec<Pos2> = (0..=24)
        .map(|i| turns + Vec2::angled(i as f32 / 24.0 * std::f32::consts::TAU) * 4.0)
        .collect();
    ants(painter, &middle);
    for (sx, sy) in HANDLES {
        let at = snap(box_point(sp, vp, sx, sy));
        // Light, dark and light rings, 9 pixels across; the corners' middle
        // filled.
        for (size, color) in [(9.0, ANT_LIGHT), (7.0, ANT_DARK), (5.0, ANT_LIGHT)] {
            painter.rect_stroke(
                Rect::from_center_size(at, vec2(size, size)),
                0.0,
                Stroke::new(1.0, color),
                StrokeKind::Inside,
            );
        }
        if sx != 0.0 && sy != 0.0 {
            painter.rect_filled(Rect::from_center_size(at, vec2(3.0, 3.0)), 0.0, ANT_LIGHT);
        }
    }
}

/// VEGAS's lines over the Pan/Crop and Track Motion workspaces: a pixel
/// wide, 3 pixels dark and 3 light in turn, so they show over any
/// picture.
pub(crate) const ANT_DARK: Color32 = Color32::from_gray(25);
pub(crate) const ANT_LIGHT: Color32 = Color32::from_gray(230);

/// The line through `points` in VEGAS's dark and light dashes.
pub(crate) fn ants(painter: &Painter, points: &[Pos2]) {
    const DASH: f32 = 3.0;
    let mut run = 0.0f32;
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let len = (b - a).length();
        if len <= 0.0 {
            continue;
        }
        let dir = (b - a) / len;
        let mut t = 0.0;
        while t < len {
            let phase = run % (2.0 * DASH);
            let (color, left) = if phase < DASH {
                (ANT_DARK, DASH - phase)
            } else {
                (ANT_LIGHT, 2.0 * DASH - phase)
            };
            let step = left.min(len - t);
            painter.line_segment([a + dir * t, a + dir * (t + step)], Stroke::new(1.0, color));
            t += step;
            run += step;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrow_keys_move_the_pan_crop_box_a_pixel_or_ten() {
        let press = |keys: &[(egui::Key, egui::Modifiers)]| {
            let mut i = egui::InputState::default();
            for &(key, modifiers) in keys {
                i.events.push(egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                });
            }
            i
        };
        let none = egui::Modifiers::NONE;
        let mut i = press(&[
            (egui::Key::ArrowRight, none),
            (egui::Key::ArrowRight, none),
            (egui::Key::ArrowUp, egui::Modifiers::SHIFT),
        ]);
        assert_eq!(arrow_nudge(&mut i, Axis::Free), (2.0, -10.0));
        assert!(i.events.is_empty(), "the keys are taken");
        let mut i = press(&[(egui::Key::ArrowLeft, none), (egui::Key::ArrowDown, none)]);
        assert_eq!(arrow_nudge(&mut i, Axis::X), (-1.0, 0.0), "X only");
        let mut i = press(&[(egui::Key::ArrowLeft, none), (egui::Key::ArrowDown, none)]);
        assert_eq!(arrow_nudge(&mut i, Axis::Y), (0.0, 1.0), "Y only");
        // Ctrl's arrows are the keyframes': left alone.
        let mut i = press(&[(egui::Key::ArrowLeft, egui::Modifiers::COMMAND)]);
        assert_eq!(arrow_nudge(&mut i, Axis::Free), (0.0, 0.0));
        assert_eq!(i.events.len(), 1);
    }

    #[test]
    fn letters_typed_over_the_chooser_pick_as_in_a_file_manager() {
        let names: Vec<String> = ["Blur", "Border", "Brightness", "Color Curves", "Correção"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        // One letter: the first name with it, then the next ones, round.
        assert_eq!(type_ahead(&names, None, "b"), Some(0));
        assert_eq!(type_ahead(&names, Some(0), "b"), Some(1));
        assert_eq!(type_ahead(&names, Some(1), "bb"), Some(2));
        assert_eq!(type_ahead(&names, Some(2), "bbb"), Some(0));
        // Letters typed together: the name they begin, accents or not.
        assert_eq!(type_ahead(&names, Some(0), "bri"), Some(2));
        assert_eq!(type_ahead(&names, None, "CORRE"), Some(4));
        assert_eq!(type_ahead(&names, None, "z"), None);
        assert_eq!(type_ahead(&[], None, "a"), None);
        assert_eq!(search_fold("Correção de Cor"), "correcao de cor");
    }

    fn vp() -> Viewport {
        Viewport {
            cx: 500.0,
            cy: 500.0,
            w: 400.0,
            h: 200.0,
            angle_deg: 0.0,
        }
    }

    /// A transition in the FX window, as VEGAS's: its plug-in alone in
    /// the chain; its check box switches it off (a plain crossfade, its
    /// settings kept), Remove takes it out (and the window closes).
    #[test]
    fn a_transition_is_switched_off_and_removed_in_the_fx_window() {
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        assert!(app.transition_for_review(false));
        let p = app.project().clone();
        let clip = p
            .clips()
            .find(|(_, c)| c.transition_fx.is_some())
            .map(|(_, c)| c.id)
            .expect("the transition");
        let target = FxTarget::Transition(clip, false);
        let chain = target.chain(&p).expect("its chain");
        assert_eq!(chain.len(), 1);
        assert!(chain[0].enabled);
        assert_eq!(target.title(&p), "Video Event FX");
        // Switched off: kept, bypassed.
        let mut off = chain.clone();
        off[0].enabled = false;
        app.modify(|p| target.set_chain(p, off));
        let c = app.project().clip(clip).unwrap();
        assert!(c.transition_bypass && c.transition_fx.is_some());
        assert!(!target.chain(app.project()).unwrap()[0].enabled);
        // Removed: a plain crossfade, nothing for the window.
        app.modify(|p| target.set_chain(p, Vec::new()));
        let c = app.project().clip(clip).unwrap();
        assert!(c.transition_fx.is_none() && !c.transition_bypass);
        assert!(target.chain(app.project()).is_none());
    }

    /// A preset of a shape: the box of that shape, as large as fits, where
    /// the box was; not the frame's shape a framing would keep.
    #[test]
    fn a_shape_preset_gives_its_box() {
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        let p = app.project().clone();
        let id = p
            .clips()
            .find(|(t, _)| t.kind == TrackKind::Video)
            .map(|(_, c)| c.id)
            .expect("a video event");
        let place = p.clip_place_in(id).expect("its place");
        let key = PanCropKey::whole(place.media).matched(1.0 / place.media_aspect);
        assert!(key.width < place.media[0]);
        let mut w = FxWindow::new(FxTarget::Clip(id), Node::PanCrop);
        w.apply_preset(&mut app, &p, &[], &PresetValue::PanBox(key));
        let c = app.project().clip(id).expect("the event");
        let now = c.box_at(c.start, place.media, place.media_aspect);
        assert_eq!((now.width, now.height), (key.width, key.height));
        assert_eq!(now.center, key.center);
    }

    #[test]
    fn buttons_move_through_the_chain_the_pan_crop_among_them() {
        use bv_timeline::EffectKind;
        let chain = vec![
            Effect::new(EffectKind::Sepia),
            Effect::new(EffectKind::Invert),
        ];
        let kinds = |c: &[Effect]| c.iter().map(|e| e.kind).collect::<Vec<_>>();
        // Pan/Crop first: [P, Sepia, Invert]. Pan/Crop after Invert.
        let seq = vec![None, Some(0), Some(1)];
        let (c, at, node) = reorder(&chain, &seq, 0, 3).unwrap();
        assert_eq!(
            (kinds(&c), at),
            (vec![EffectKind::Sepia, EffectKind::Invert], 2)
        );
        assert_eq!(node, Node::PanCrop);
        // Invert before everything: [Invert, P, Sepia].
        let (c, at, node) = reorder(&chain, &seq, 2, 0).unwrap();
        assert_eq!(
            (kinds(&c), at),
            (vec![EffectKind::Invert, EffectKind::Sepia], 1)
        );
        assert_eq!(node, Node::Effect(0));
        // Where it is: nothing moves.
        assert!(reorder(&chain, &seq, 1, 1).is_none());
        assert!(reorder(&chain, &seq, 1, 2).is_none());
        // The place a button let go goes to.
        let r = |x: f32| Rect::from_min_size(pos2(x, 0.0), vec2(40.0, 20.0));
        let rects = [r(0.0), r(50.0), r(100.0)];
        assert_eq!(drop_slot(&rects, 10.0), 0);
        assert_eq!(drop_slot(&rects, 80.0), 2);
        assert_eq!(drop_slot(&rects, 300.0), 3);
    }

    /// The Pan/Crop button moves by its menu, and dragged.
    #[test]
    fn the_pan_crop_button_moves_by_its_menu() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut app = BananaApp::new(egui::Context::default());
        app.welcome_for_review("closed");
        app.load_demo();
        let clip = app.primary_clip().expect("the demo's selected event");
        let mut h = Harness::builder()
            .with_size(vec2(1600.0, 960.0))
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(
                |ui, app: &mut BananaApp| {
                    crate::apply_theme(ui.ctx());
                    app.frame(ui);
                },
                app,
            );
        h.run_steps(4);
        h.state_mut().open_clip_window("fx");
        h.run_steps(6);
        h.get_by_label("Pan/Crop").click_secondary();
        h.run_steps(3);
        h.get_by_label("Move Right").click();
        h.run_steps(3);
        let at = h.state().project().clip(clip).unwrap().pan_crop_at;
        assert_eq!(at, 1, "after the event's first effect");
        // Undo puts it back first.
        h.state_mut().undo();
        assert_eq!(h.state().project().clip(clip).unwrap().pan_crop_at, 0);
        h.run_steps(3);
        // Dragged past the last effect: all of them before it.
        let from = h.get_by_label("Pan/Crop").rect().center();
        let to = pos2(1590.0, from.y);
        h.event(egui::Event::PointerMoved(from));
        h.step();
        h.event(button(from, true));
        h.step();
        for i in 1..=8 {
            h.event(egui::Event::PointerMoved(from.lerp(to, i as f32 / 8.0)));
            h.step();
        }
        h.event(button(to, false));
        h.run_steps(3);
        let c = h.state().project().clip(clip).unwrap().clone();
        assert!(!c.effects.is_empty());
        assert_eq!(c.pan_crop_at, c.effects.len(), "the Pan/Crop last");
    }

    /// A video track's chain has VEGAS's Composite button, last at first:
    /// moved left, the last effect works after compositing; moved back,
    /// VEGAS's default again.
    #[test]
    fn the_composite_button_moves_in_a_video_track_chain() {
        use bv_timeline::EffectKind;
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut app = BananaApp::new(egui::Context::default());
        app.welcome_for_review("closed");
        app.load_demo();
        let track = app
            .project()
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video && !t.adjustment)
            .map(|t| t.id)
            .expect("a video track");
        app.modify(|p| {
            p.track_by_id_mut(track).unwrap().effects = vec![
                Effect::new(EffectKind::Sepia),
                Effect::new(EffectKind::Invert),
            ];
        });
        app.clip_windows.track_fx = Some(track);
        let mut h = Harness::builder()
            .with_size(vec2(1600.0, 960.0))
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(
                |ui, app: &mut BananaApp| {
                    crate::apply_theme(ui.ctx());
                    app.frame(ui);
                },
                app,
            );
        h.run_steps(6);
        let at =
            |h: &Harness<'_, BananaApp>| h.state().project().track(track).unwrap().composite_at;
        assert_eq!(at(&h), None, "last, VEGAS's default");
        for (item, want) in [("Move Left", Some(1)), ("Move Right", None)] {
            h.get_by_label("Composite").click_secondary();
            h.run_steps(3);
            h.get_by_label(item).click();
            h.run_steps(3);
            assert_eq!(at(&h), want, "{item}");
        }
        assert_eq!(
            h.state().project().track(track).unwrap().effects.len(),
            2,
            "the effects stay"
        );
    }

    /// The chain's row scrolled by its scroll buttons: a button at a time,
    /// all the way to an end once only its dot is left; a button picked
    /// comes into view, the row moving as little as it must.
    #[test]
    fn the_chain_row_scrolls_a_button_at_a_time() {
        // Four buttons along a chain 400 long, 150 of it shown.
        let spans = [
            (28.0, 108.0),
            (124.0, 204.0),
            (220.0, 300.0),
            (316.0, 372.0),
        ];
        let full = 372.0 + CHAIN_LINK + CHAIN_END;
        let (shown, end) = (150.0, 250.0);
        let ahead = |offset| chain_step(&spans, offset, shown, full, true);
        let back = |offset| chain_step(&spans, offset, shown, full, false);
        // The second button and its link, then the third; the fourth
        // leaves only the output's dot: the end.
        assert_eq!(ahead(0.0), 70.0);
        assert_eq!(ahead(70.0), 166.0);
        assert_eq!(ahead(166.0), end);
        // Back: the third button with the link before it, the second; the
        // first leaves only the input's dot: the start.
        assert_eq!(back(end), 204.0);
        assert_eq!(back(204.0), 108.0);
        assert_eq!(back(108.0), 0.0);
        // Picked: already in view, past the right, near the end, past the
        // left, near the start.
        assert_eq!(chain_reveal(spans[0], 0.0, shown, full), 0.0);
        assert_eq!(chain_reveal(spans[2], 0.0, shown, full), 166.0);
        assert_eq!(chain_reveal(spans[3], 0.0, shown, full), end);
        assert_eq!(chain_reveal(spans[1], end, shown, full), 108.0);
        assert_eq!(chain_reveal(spans[0], end, shown, full), 0.0);
        // A button wider than what shows: from its start.
        assert_eq!(chain_reveal((124.0, 330.0), 0.0, shown, full), 108.0);
    }

    /// A chain longer than its row: its buttons stop before the buttons at
    /// the row's end and scroll between two scroll buttons, by them and by
    /// the mouse wheel.
    #[test]
    fn a_long_chain_scrolls_before_the_buttons_at_the_end() {
        use bv_timeline::EffectKind;
        use egui_kittest::Harness;
        use egui_kittest::kittest::{NodeT, Queryable};
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut app = BananaApp::new(egui::Context::default());
        app.welcome_for_review("closed");
        // The window keeps the size it is given, whatever button is picked.
        app.settings.prefs.fx_window_resize = false;
        app.load_demo();
        app.clip_chain_for_review(
            [
                EffectKind::BrightnessContrast,
                EffectKind::BlackAndWhite,
                EffectKind::GaussianBlur,
                EffectKind::Sharpen,
                EffectKind::Pixelate,
                EffectKind::Vignette,
                EffectKind::Mirror,
                EffectKind::Glow,
            ]
            .map(Effect::new)
            .to_vec(),
        );
        let mut h = Harness::builder()
            .with_size(vec2(1600.0, 960.0))
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(
                |ui, app: &mut BananaApp| {
                    crate::apply_theme(ui.ctx());
                    app.frame(ui);
                },
                app,
            );
        h.run_steps(6);
        let ctx = h.ctx.clone();
        h.state_mut()
            .fx_window_size_for_review(&ctx, vec2(600.0, 440.0));
        h.run_steps(6);
        let back = h.get_by_label("Scroll Left").rect();
        let ahead = h.get_by_label("Scroll Right").rect();
        // A button on the chain's row (the page may show the same name).
        let rect = |h: &Harness<'_, BananaApp>, label: &str| {
            h.get_all_by_label(label)
                .map(|n| n.rect())
                .find(|r| (r.center().y - ahead.center().y).abs() < 4.0)
                .unwrap_or_else(|| panic!("no {label} on the chain's row"))
        };
        let tools = rect(&h, "Plug-In Chain…");
        assert!(
            ahead.right() <= tools.left(),
            "the scroll buttons before the chain's tools: {ahead:?} {tools:?}"
        );
        // At the start: the Pan/Crop shows, the last effect is past the
        // right scroll button.
        assert!(rect(&h, "Pan/Crop").left() >= back.right());
        assert!(rect(&h, "Glow").right() > ahead.left());
        // To the end, a button at a time: the last effect shows, the
        // Pan/Crop is past the left scroll button.
        let mut clicks = 0;
        while !h
            .get_by_label("Scroll Right")
            .accesskit_node()
            .is_disabled()
        {
            h.get_by_label("Scroll Right").click();
            h.run_steps(3);
            clicks += 1;
            assert!(clicks < 20, "the end never came");
        }
        assert!(clicks > 1, "a button at a time: {clicks}");
        assert!(rect(&h, "Glow").right() <= ahead.left());
        assert!(rect(&h, "Pan/Crop").right() < back.right());
        // The wheel scrolls it back to the start.
        let over = rect(&h, "Glow").center();
        for _ in 0..4 {
            h.event(egui::Event::PointerMoved(over));
            h.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, 400.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::default(),
            });
            h.run_steps(10);
        }
        assert!(rect(&h, "Pan/Crop").left() >= back.right());
        assert!(
            h.get_by_label("Scroll Left").accesskit_node().is_disabled(),
            "at the start"
        );
        // A button picked out of view (by the accessibility tree: the
        // pointer cannot reach it) comes into view.
        h.get_by_label("Glow").click_accesskit();
        h.run_steps(4);
        assert!(rect(&h, "Glow").right() <= ahead.left());
        assert!(rect(&h, "Glow").left() >= back.right());
    }

    #[test]
    fn borda_direita_sem_centro_mantem_a_esquerda() {
        let v = resize(&vp(), (300.0, 0.0), 1.0, 0.0, false, false);
        assert!((v.w - 500.0).abs() < 1e-3 && (v.h - 200.0).abs() < 1e-3);
        assert!((v.cx - v.w / 2.0 - 300.0).abs() < 1e-3, "{v:?}");
    }

    #[test]
    fn canto_com_proporcao_travada() {
        let v = resize(&vp(), (300.0, 100.0), 1.0, 1.0, true, true);
        assert!((v.w / v.h - 2.0).abs() < 1e-4, "{v:?}");
        assert!((v.w - 600.0).abs() < 1e-3);
        assert_eq!((v.cx, v.cy), (500.0, 500.0));
    }

    /// With Enable Snapping a dragged edge lands on the grid's line, the
    /// other edge staying; with the aspect locked the side nearer its line
    /// snaps and the other follows; a turned box is sized in whole steps.
    #[test]
    fn snapping_puts_the_dragged_edge_on_the_grid() {
        let step = (120.0, 120.0);
        // The box: 300 to 700 across, 400 to 600 down.
        let v0 = vp();
        let edges = |v: &Viewport| {
            [
                v.cx - v.w / 2.0,
                v.cx + v.w / 2.0,
                v.cy - v.h / 2.0,
                v.cy + v.h / 2.0,
            ]
        };
        let close = |a: [f32; 4], b: [f32; 4]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3);
        // Its right edge dragged to 787: on the line at 840.
        let v = resize(&v0, (287.0, 0.0), 1.0, 0.0, false, false);
        let s = snap_resize(&v0, &v, 1.0, 0.0, false, false, step);
        assert!(close(edges(&s), [300.0, 840.0, 400.0, 600.0]), "{s:?}");
        // Its corner dragged with the aspect locked to 800 × 650: across
        // is nearer a line (840), down follows at 2 : 1.
        let v = resize(&v0, (300.0, 120.0), 1.0, 1.0, true, false);
        assert!(close(edges(&v), [300.0, 800.0, 400.0, 650.0]), "{v:?}");
        let s = snap_resize(&v0, &v, 1.0, 1.0, true, false, step);
        assert!(close(edges(&s), [300.0, 840.0, 400.0, 670.0]), "{s:?}");
        // About its center: the edge dragged on its line, the other mirrored.
        let v = resize(&v0, (230.0, 0.0), 1.0, 0.0, false, true);
        let s = snap_resize(&v0, &v, 1.0, 0.0, false, true, step);
        assert!(close(edges(&s), [280.0, 720.0, 400.0, 600.0]), "{s:?}");
        // Turned: its size in whole steps.
        let mut t0 = v0;
        t0.angle_deg = 30.0;
        let v = resize(&t0, (287.0, 0.0), 1.0, 0.0, false, false);
        let s = snap_resize(&t0, &v, 1.0, 0.0, false, false, step);
        assert!(
            (s.w - 480.0).abs() < 1e-3 && (s.h - 200.0).abs() < 1e-3,
            "{s:?}"
        );
    }

    #[test]
    fn borda_girada_anda_na_direcao_certa() {
        let mut v0 = vp();
        v0.angle_deg = 90.0;
        // Alça da direita do F girado 90° fica embaixo na tela.
        let v = resize(&v0, (300.0, 0.0), 1.0, 0.0, false, false);
        assert!(
            (v.cx - 500.0).abs() < 1e-3 && (v.cy - 550.0).abs() < 1e-3,
            "{v:?}"
        );
    }

    fn frame(ctx: &egui::Context, app: &mut BananaApp, events: Vec<egui::Event>) {
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 960.0))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| app.frame(ui));
        // Sem GPU: as texturas não são enviadas a lugar nenhum.
        out.textures_delta.clear();
    }

    fn button(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        }
    }

    #[test]
    fn arrastar_a_alca_do_f_anima_o_enquadramento_no_cursor() {
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        app.load_demo();
        let id = app.primary_clip().expect("clipe selecionado");
        app.open_clip_window("pancrop");
        for _ in 0..4 {
            frame(&ctx, &mut app, Vec::new());
        }
        let (sp, vp) = app
            .clip_windows
            .fx_window()
            .and_then(|w| w.pan.view)
            .expect("área de trabalho desenhada");
        // Puxa a alça da direita 60 px para fora: o F cresce (zoom para trás).
        let from = box_point(&sp, &vp, 1.0, 0.0);
        frame(&ctx, &mut app, vec![egui::Event::PointerMoved(from)]);
        frame(&ctx, &mut app, vec![button(from, true)]);
        for i in 1..=6 {
            let p = from + vec2(10.0 * i as f32, 0.0);
            frame(&ctx, &mut app, vec![egui::Event::PointerMoved(p)]);
        }
        frame(&ctx, &mut app, vec![button(from + vec2(60.0, 0.0), false)]);
        frame(&ctx, &mut app, Vec::new());
        let c = app.project().clip(id).expect("clipe").clone();
        // O cursor está no meio do clipe: virou animação com dois quadros-chave.
        assert_eq!(c.pan_crop.keys.len(), 2, "{:?}", c.pan_crop.keys);
        let at = |t: Tick| app.project().clip_transform_at(id, t).expect("clipe");
        assert_eq!(at(c.start).scale, 1.0);
        let tf = at(app.cursor);
        let expected = vp.w / (vp.w + 2.0 * 60.0 / sp.k);
        assert!(
            (tf.scale - expected).abs() < 0.02,
            "{tf:?} esperado {expected}"
        );
        // Um único passo de desfazer volta tudo.
        app.undo();
        assert!(
            app.project()
                .clip(id)
                .expect("clipe")
                .pan_crop
                .keys
                .is_empty()
        );
    }

    /// The Mask row picked and the Rectangle tool: a drag on the workspace
    /// draws a mask on the event's media, in one step of the undo history;
    /// the Mask checkbox turns the masks off.
    #[test]
    fn a_rectangle_dragged_in_the_mask_row_masks_the_event() {
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        app.load_demo();
        let id = app.primary_clip().expect("an event picked");
        app.open_clip_window("pancrop");
        app.clip_windows.mask_row = Some((None, None, false));
        frame(&ctx, &mut app, Vec::new());
        app.clip_windows
            .fx_window_mut()
            .expect("the window")
            .mask_tool_for_review(crate::pancrop_mask::MaskTool::Rectangle);
        for _ in 0..4 {
            frame(&ctx, &mut app, Vec::new());
        }
        let (sp, _) = app
            .clip_windows
            .fx_window()
            .and_then(|w| w.pan.view)
            .expect("the workspace drawn");
        let from = sp.screen((sp.src.0 * 0.25, sp.src.1 * 0.25));
        let to = sp.screen((sp.src.0 * 0.75, sp.src.1 * 0.5));
        frame(&ctx, &mut app, vec![egui::Event::PointerMoved(from)]);
        frame(&ctx, &mut app, vec![button(from, true)]);
        for i in 1..=6 {
            let p = from + (to - from) * (i as f32 / 6.0);
            frame(&ctx, &mut app, vec![egui::Event::PointerMoved(p)]);
        }
        frame(&ctx, &mut app, vec![button(to, false)]);
        frame(&ctx, &mut app, Vec::new());
        let c = app.project().clip(id).expect("the event").clone();
        let m = c.mask.expect("masks made");
        assert!(m.enabled && m.active(), "{m:?}");
        let (lo, hi) = m.paths[0].bounds().expect("a path");
        let near = |a: f64, b: f64| (a - b).abs() < f64::from(sp.src.0) * 0.01;
        assert!(
            near(lo[0], f64::from(sp.src.0) * 0.25) && near(hi[0], f64::from(sp.src.0) * 0.75),
            "{lo:?} {hi:?}"
        );
        assert!(
            near(lo[1], f64::from(sp.src.1) * 0.25) && near(hi[1], f64::from(sp.src.1) * 0.5),
            "{lo:?} {hi:?}"
        );
        app.undo();
        assert!(app.project().clip(id).expect("the event").mask.is_none());
    }

    /// The demo's event with its review masks (an oval, path 0, and a
    /// rectangle inside it, path 1), its Pan/Crop window on the Mask row
    /// with nothing picked and `tool` picked: the event and the workspace.
    fn masked(
        ctx: &egui::Context,
        app: &mut BananaApp,
        tool: crate::pancrop_mask::MaskTool,
    ) -> (ClipId, Space) {
        app.load_demo();
        let id = app.primary_clip().expect("an event picked");
        assert!(app.pan_crop_masks_for_review());
        app.clip_windows.mask_row = Some((None, None, false));
        frame(ctx, app, Vec::new());
        app.clip_windows
            .fx_window_mut()
            .expect("the window")
            .mask_tool_for_review(tool);
        for _ in 0..4 {
            frame(ctx, app, Vec::new());
        }
        let (sp, _) = app
            .clip_windows
            .fx_window()
            .and_then(|w| w.pan.view)
            .expect("the workspace drawn");
        (id, sp)
    }

    fn shapes(app: &BananaApp, id: ClipId) -> Vec<Vec<bv_timeline::mask::MaskAnchor>> {
        let c = app.project().clip(id).expect("the event");
        c.mask.as_ref().expect("masks").shapes_at(Tick::ZERO)
    }

    fn mask_editor(app: &BananaApp) -> &crate::pancrop_mask::MaskEditor {
        &app.clip_windows.fx_window().expect("the window").pan.mask
    }

    fn click_at(ctx: &egui::Context, app: &mut BananaApp, p: Pos2) {
        frame(ctx, app, vec![egui::Event::PointerMoved(p)]);
        frame(ctx, app, vec![button(p, true)]);
        frame(ctx, app, vec![button(p, false)]);
        frame(ctx, app, Vec::new());
    }

    fn drag_from(ctx: &egui::Context, app: &mut BananaApp, from: Pos2, to: Pos2) {
        frame(ctx, app, vec![egui::Event::PointerMoved(from)]);
        frame(ctx, app, vec![button(from, true)]);
        for i in 1..=6 {
            let p = from + (to - from) * (i as f32 / 6.0);
            frame(ctx, app, vec![egui::Event::PointerMoved(p)]);
        }
        frame(ctx, app, vec![button(to, false)]);
        frame(ctx, app, Vec::new());
    }

    /// The Split Tangent tool's click on a line — its path not picked —
    /// puts an anchor exactly at the line's point nearest the click, the
    /// curve as it was.
    #[test]
    fn split_tangent_puts_an_anchor_where_the_line_is_clicked() {
        use crate::pancrop_mask::MaskTool;
        use bv_timeline::mask::segment_point;
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        let (id, sp) = masked(&ctx, &mut app, MaskTool::SplitTangent);
        let was = shapes(&app, id);
        let on = segment_point(&was[0], 1, 0.37);
        // Clicked 3 pixels off the line, across it.
        let ahead = segment_point(&was[0], 1, 0.38);
        let along = (sp.screen64((ahead[0], ahead[1])) - sp.screen64((on[0], on[1]))).normalized();
        let p = sp.screen64((on[0], on[1])) + vec2(-along.y, along.x) * 3.0;
        click_at(&ctx, &mut app, p);
        let now = shapes(&app, id);
        assert_eq!(now[0].len(), was[0].len() + 1, "an anchor added");
        let a = now[0][2];
        let target = sp.media64(p);
        // The point of the curve nearest the click.
        let dist = |q: [f64; 2]| (q[0] - target.0).hypot(q[1] - target.1);
        let nearest = (0..=4000)
            .map(|k| segment_point(&was[0], 1, f64::from(k) / 4000.0))
            .min_by(|x, y| dist(*x).total_cmp(&dist(*y)))
            .expect("points");
        let px = 1.0 / f64::from(sp.k);
        assert!(
            (a.at[0] - nearest[0]).abs() < px && (a.at[1] - nearest[1]).abs() < px,
            "{:?} {nearest:?}",
            a.at
        );
        // The curve kept: its two halves run along the old segment.
        for seg in [1, 2] {
            for k in 1..10 {
                let q = segment_point(&now[0], seg, f64::from(k) / 10.0);
                let off = (0..=400)
                    .map(|j| segment_point(&was[0], 1, f64::from(j) / 400.0))
                    .map(|w| (w[0] - q[0]).hypot(w[1] - q[1]))
                    .fold(f64::MAX, f64::min);
                assert!(off < 1.0, "{seg} {k}: {off}");
            }
        }
        assert_eq!(
            mask_editor(&app)
                .anchors
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![(0, 2)]
        );
    }

    /// A click of the Split Tangent or Anchor Creation tool on a curved
    /// anchor makes it a corner; another brings its tangents back as they
    /// were.
    #[test]
    fn a_click_on_a_curved_anchor_makes_it_a_corner_and_back() {
        use crate::pancrop_mask::MaskTool;
        for tool in [MaskTool::SplitTangent, MaskTool::AddAnchor] {
            let ctx = egui::Context::default();
            let mut app = BananaApp::new(ctx.clone());
            let (id, sp) = masked(&ctx, &mut app, tool);
            let was = shapes(&app, id)[0][1];
            assert!(was.after != [0.0; 2], "curved");
            let p = sp.screen64((was.at[0], was.at[1]));
            click_at(&ctx, &mut app, p);
            let corner = shapes(&app, id)[0][1];
            assert_eq!(
                corner,
                bv_timeline::mask::MaskAnchor::corner(was.at),
                "{tool:?}"
            );
            // Past the double-click's time.
            for _ in 0..30 {
                frame(&ctx, &mut app, Vec::new());
            }
            click_at(&ctx, &mut app, p);
            assert_eq!(shapes(&app, id)[0][1], was, "{tool:?}");
        }
    }

    /// The Anchor Creation tool's clicks draw corners (straight lines
    /// between them) and a click on the first anchor closes the path.
    #[test]
    fn the_pen_s_clicks_draw_straight_lines() {
        use crate::pancrop_mask::MaskTool;
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        let (id, sp) = masked(&ctx, &mut app, MaskTool::AddAnchor);
        let (w, h) = (f64::from(sp.src.0), f64::from(sp.src.1));
        let spots = [(0.85, 0.1), (0.95, 0.1), (0.9, 0.3)];
        for (x, y) in spots {
            click_at(&ctx, &mut app, sp.screen64((x * w, y * h)));
            for _ in 0..30 {
                frame(&ctx, &mut app, Vec::new());
            }
        }
        click_at(
            &ctx,
            &mut app,
            sp.screen64((spots[0].0 * w, spots[0].1 * h)),
        );
        let now = shapes(&app, id);
        assert_eq!(now.len(), 3, "a new path");
        assert_eq!(now[2].len(), 3, "{:?}", now[2]);
        for (a, (x, y)) in now[2].iter().zip(spots) {
            assert_eq!((a.before, a.after), ([0.0; 2], [0.0; 2]), "a corner");
            assert!(
                (a.at[0] - x * w).abs() < 1.0 / f64::from(sp.k)
                    && (a.at[1] - y * h).abs() < 1.0 / f64::from(sp.k)
            );
        }
        // Closed: the next click starts another path.
        click_at(&ctx, &mut app, sp.screen64((0.86 * w, 0.92 * h)));
        assert_eq!(shapes(&app, id).len(), 4);
    }

    /// The Normal Edit tool: a rectangle dragged off the paths picks the
    /// anchors inside it, and a drag of one moves them all, the others
    /// staying.
    #[test]
    fn picked_anchors_move_together() {
        use crate::pancrop_mask::MaskTool;
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        let (id, sp) = masked(&ctx, &mut app, MaskTool::Normal);
        let was = shapes(&app, id);
        let (w, h) = (f64::from(sp.src.0), f64::from(sp.src.1));
        let (from, to) = (
            sp.screen64((0.05 * w, 0.05 * h)),
            sp.screen64((0.47 * w, 0.55 * h)),
        );
        drag_from(&ctx, &mut app, from, to);
        let r = Rect::from_two_pos(from, to);
        let inside: Vec<(usize, usize)> = was
            .iter()
            .enumerate()
            .flat_map(|(i, s)| {
                s.iter()
                    .enumerate()
                    .filter(|(_, a)| r.contains(sp.screen64((a.at[0], a.at[1]))))
                    .map(move |(k, _)| (i, k))
            })
            .collect();
        assert!(inside.len() >= 3, "{inside:?}");
        assert_eq!(
            mask_editor(&app)
                .anchors
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            inside
        );
        assert!(!mask_editor(&app).box_shows());
        // One of them dragged: all move alike.
        let (gp, ga) = inside[0];
        let g = was[gp][ga].at;
        let from = sp.screen64((g[0], g[1]));
        drag_from(&ctx, &mut app, from, from + vec2(30.0, 12.0));
        let now = shapes(&app, id);
        let d = [now[gp][ga].at[0] - g[0], now[gp][ga].at[1] - g[1]];
        assert!(d[0] > 1.0 && d[1] > 0.5, "{d:?}");
        for (i, s) in was.iter().enumerate() {
            for (k, a) in s.iter().enumerate() {
                let moved = [now[i][k].at[0] - a.at[0], now[i][k].at[1] - a.at[1]];
                if inside.contains(&(i, k)) {
                    assert!((moved[0] - d[0]).abs() < 1e-3 && (moved[1] - d[1]).abs() < 1e-3);
                } else {
                    assert_eq!(moved, [0.0; 2], "{i} {k}");
                }
            }
        }
    }

    /// The Normal Edit tool: the box shows only after a double-click on a
    /// path's line, and a click off the paths hides it; a line dragged
    /// shows its anchors' tangents as they follow.
    #[test]
    fn a_double_click_shows_the_box_and_a_bend_its_tangents() {
        use crate::pancrop_mask::MaskTool;
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        let (id, sp) = masked(&ctx, &mut app, MaskTool::Normal);
        let was = shapes(&app, id);
        let q = bv_timeline::mask::segment_point(&was[0], 0, 0.5);
        let p = sp.screen64((q[0], q[1]));
        click_at(&ctx, &mut app, p);
        assert_eq!(mask_editor(&app).selected, Some(0));
        assert!(!mask_editor(&app).box_shows(), "one click: no box");
        click_at(&ctx, &mut app, p);
        assert!(mask_editor(&app).box_shows(), "a double-click: the box");
        for _ in 0..30 {
            frame(&ctx, &mut app, Vec::new());
        }
        click_at(
            &ctx,
            &mut app,
            sp.screen((0.04 * sp.src.0, 0.04 * sp.src.1)),
        );
        assert!(!mask_editor(&app).box_shows(), "a click off it");
        assert_eq!(mask_editor(&app).selected, None);
        // The line bent: its anchors picked (their tangents drawn) while it
        // is dragged, and it goes where it was pulled.
        frame(&ctx, &mut app, vec![egui::Event::PointerMoved(p)]);
        frame(&ctx, &mut app, vec![button(p, true)]);
        for i in 1..=4 {
            let to = p + vec2(0.0, -6.0 * i as f32);
            frame(&ctx, &mut app, vec![egui::Event::PointerMoved(to)]);
        }
        assert_eq!(
            mask_editor(&app)
                .anchors
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![(0, 0), (0, 1)]
        );
        frame(&ctx, &mut app, vec![button(p + vec2(0.0, -24.0), false)]);
        frame(&ctx, &mut app, Vec::new());
        let now = bv_timeline::mask::segment_point(&shapes(&app, id)[0], 0, 0.5);
        assert!(
            (now[1] - (q[1] - 24.0 / f64::from(sp.k))).abs() < 0.5 / f64::from(sp.k),
            "{now:?} {q:?}"
        );
    }

    #[test]
    fn acerta_alcas_e_interior() {
        let sp = Space {
            center: pos2(400.0, 300.0),
            k: 0.5,
            src: (1000.0, 1000.0),
        };
        let v = vp();
        let corner = box_point(&sp, &v, 1.0, 1.0);
        assert_eq!(hit(&sp, &v, corner, true), Hit::Handle(1.0, 1.0));
        assert_eq!(hit(&sp, &v, sp.screen64((450.0, 520.0)), true), Hit::Inside);
        assert_eq!(
            hit(&sp, &v, sp.screen64((900.0, 900.0)), true),
            Hit::Outside
        );
        // Round the box, up to its circle (0.55 of its diagonal, as VEGAS's),
        // it turns.
        let round = sp.screen64((500.0, 630.0));
        assert_eq!(hit(&sp, &v, round, true), Hit::Rotate);
        assert_eq!(hit(&sp, &v, round, false), Hit::Outside);
        let ring = sp.screen((500.0 + 0.55 * 400f32.hypot(200.0), 500.0));
        assert_eq!(hit(&sp, &v, ring, true), Hit::Rotate);
    }

    /// The handles' arrows as VEGAS turns them with the box: unturned, as
    /// measured (top left ↘, top right ↗, top ↕, right ↔); a step round
    /// ↔ ↘ ↕ ↗ for each 45° clockwise on the screen — the box turned 45°
    /// the other way, as measured, its top right shows ↕, its top ↘ and
    /// its right ↗ —; half a turn as unturned.
    #[test]
    fn handle_pointers_turn_with_the_box_as_vegas_s() {
        use Cursor::{ResizeEw as Ew, ResizeNesw as Nesw, ResizeNs as Ns, ResizeNwse as Nwse};
        let (tl, t, tr, r) = ((-1.0, -1.0), (0.0, -1.0), (1.0, -1.0), (1.0, 0.0));
        let (br, b, bl, l) = ((1.0, 1.0), (0.0, 1.0), (-1.0, 1.0), (-1.0, 0.0));
        let at = |deg: f64, h: (f32, f32)| handle_cursor(h, deg.to_radians(), false);
        let all = [tl, t, tr, r, br, b, bl, l];
        let row = |deg: f64| all.map(|h| at(deg, h));
        assert_eq!(row(0.0), [Nwse, Ns, Nesw, Ew, Nwse, Ns, Nesw, Ew]);
        // 45° clockwise: the top-left corner shows ↕.
        assert_eq!(row(45.0), [Ns, Nesw, Ew, Nwse, Ns, Nesw, Ew, Nwse]);
        // 45° the other way: the measured three, and the top left ↔.
        assert_eq!(
            (at(-45.0, tr), at(-45.0, t), at(-45.0, r)),
            (Ns, Nwse, Nesw)
        );
        assert_eq!(at(-45.0, tl), Ew);
        assert_eq!(row(90.0), [Nesw, Ew, Nwse, Ns, Nesw, Ew, Nwse, Ns]);
        assert_eq!(row(180.0), row(0.0));
        assert_eq!(row(-90.0), row(270.0));
        // Rounded to the nearest step, the angle taken within a turn.
        assert_eq!(row(22.0), row(0.0));
        assert_eq!(row(23.0), row(45.0));
        assert_eq!(row(405.0), row(45.0));
        assert_eq!(row(-315.0), row(45.0));
        assert_eq!(at(f64::NAN, tl), Nwse);
    }

    /// A mirrored box (width and height of different signs): VEGAS turns
    /// its corners two steps more, its sides not. Its corners, its own
    /// (mirrored) way, are drawn on the other side: the pointer over each
    /// drawn corner is the one that corner shows unmirrored.
    #[test]
    fn a_mirrored_box_s_corners_turn_two_steps_more() {
        use Cursor::{ResizeEw as Ew, ResizeNesw as Nesw, ResizeNs as Ns, ResizeNwse as Nwse};
        let mirrored = |h: (f32, f32), deg: f64| handle_cursor(h, deg.to_radians(), true);
        assert_eq!(mirrored((-1.0, -1.0), 0.0), Nesw);
        assert_eq!(mirrored((1.0, -1.0), 0.0), Nwse);
        assert_eq!(mirrored((0.0, -1.0), 0.0), Ns);
        assert_eq!(mirrored((1.0, 0.0), 0.0), Ew);
        assert_eq!(mirrored((-1.0, -1.0), 45.0), Ew);
        for flips in [(false, false), (true, false), (false, true), (true, true)] {
            for deg in [0.0, 30.0, 45.0, 90.0, -45.0, 180.0] {
                for (sx, sy) in HANDLES {
                    assert_eq!(
                        box_handle_cursor(sx, sy, deg, flips),
                        box_handle_cursor(sx, sy, deg, (false, false)),
                        "{flips:?} {deg} ({sx}, {sy})"
                    );
                }
            }
        }
        assert_eq!(box_handle_cursor(-1.0, -1.0, 0.0, (true, false)), Nwse);
    }

    /// The workspace's pointer: over the box, its zones' (the turning
    /// center over the inside, the handles over the center); far from it
    /// the open hand; a drag keeps its own (the view dragged, the closed
    /// hand); the zoom tool's magnifier, − with its right button or Shift.
    #[test]
    fn the_pan_crop_workspace_s_pointer_follows_the_spot_the_drag_and_the_tool() {
        let plain = (false, false);
        let over = |hit: Hit, pivot: bool| {
            pan_cursor(Tool::Normal, None, Some((hit, pivot)), false, 0.0, plain)
        };
        assert_eq!(over(Hit::Handle(1.0, 1.0), true), Some(Cursor::ResizeNwse));
        assert_eq!(over(Hit::Inside, true), Some(Cursor::MoveCenter));
        assert_eq!(over(Hit::Inside, false), Some(Cursor::MoveFrame));
        assert_eq!(over(Hit::Rotate, false), Some(Cursor::Rotate));
        assert_eq!(over(Hit::Outside, false), Some(Cursor::HandOpen));
        assert_eq!(
            pan_cursor(Tool::Normal, None, None, false, 0.0, plain),
            None
        );
        let turned = Viewport {
            angle_deg: 45.0,
            ..vp()
        };
        let resize = Drag::Resize {
            vp: turned,
            sx: -1.0,
            sy: -1.0,
        };
        let view = Drag::View {
            offset: Vec2::ZERO,
            from: Pos2::ZERO,
        };
        for (d, c) in [
            (resize, Cursor::ResizeNs),
            (
                Drag::Move {
                    vp: vp(),
                    from: Pos2::ZERO,
                },
                Cursor::MoveFrame,
            ),
            (
                Drag::Rotate {
                    vp: vp(),
                    from: 0.0,
                },
                Cursor::Rotate,
            ),
            (
                Drag::Pivot {
                    at: (0.0, 0.0),
                    from: Pos2::ZERO,
                },
                Cursor::MoveCenter,
            ),
            (view, Cursor::HandClosed),
        ] {
            // Wherever the pointer went meanwhile.
            let far = Some((Hit::Outside, false));
            assert_eq!(
                pan_cursor(Tool::Normal, Some(&d), far, false, 0.0, plain),
                Some(c)
            );
            assert_eq!(
                pan_cursor(Tool::Normal, Some(&d), None, false, 0.0, plain),
                Some(c)
            );
        }
        let inside = Some((Hit::Inside, false));
        assert_eq!(
            pan_cursor(Tool::Zoom, None, inside, false, 0.0, plain),
            Some(Cursor::ZoomIn)
        );
        assert_eq!(
            pan_cursor(Tool::Zoom, None, inside, true, 0.0, plain),
            Some(Cursor::ZoomOut)
        );
        assert_eq!(
            pan_cursor(Tool::Zoom, Some(&view), inside, true, 0.0, plain),
            Some(Cursor::HandClosed)
        );
    }

    /// The pointer a frame of `events` leaves: BananaV's own, if one.
    fn pointer_after(
        ctx: &egui::Context,
        app: &mut BananaApp,
        events: Vec<egui::Event>,
    ) -> Option<Cursor> {
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 960.0))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| app.frame(ui));
        out.textures_delta.clear();
        let img = out.platform_output.cursor_image?;
        Cursor::ALL
            .into_iter()
            .find(|&c| std::sync::Arc::ptr_eq(&cursors::image(c, img.size[0]).rgba, &img.rgba))
    }

    /// In the Pan/Crop window itself: a corner's arrow, the center's, the
    /// inside's, round the box and far from it.
    #[test]
    fn the_pointer_over_the_pan_crop_box_is_its_zone_s() {
        let ctx = egui::Context::default();
        let mut app = BananaApp::new(ctx.clone());
        app.load_demo();
        app.open_clip_window("pancrop");
        for _ in 0..4 {
            frame(&ctx, &mut app, Vec::new());
        }
        let (sp, vp) = app
            .clip_windows
            .fx_window()
            .and_then(|w| w.pan.view)
            .expect("the workspace drawn");
        let mut at = |p: Pos2| {
            pointer_after(&ctx, &mut app, vec![egui::Event::PointerMoved(p)]);
            pointer_after(&ctx, &mut app, Vec::new())
        };
        let c = sp.screen((vp.cx, vp.cy));
        let r = rotation_radius(&sp, &vp);
        assert_eq!(
            at(box_point(&sp, &vp, -1.0, -1.0)),
            Some(Cursor::ResizeNwse)
        );
        assert_eq!(at(box_point(&sp, &vp, 1.0, -1.0)), Some(Cursor::ResizeNesw));
        assert_eq!(at(box_point(&sp, &vp, 0.0, -1.0)), Some(Cursor::ResizeNs));
        assert_eq!(at(box_point(&sp, &vp, 1.0, 0.0)), Some(Cursor::ResizeEw));
        assert_eq!(at(c), Some(Cursor::MoveCenter));
        assert_eq!(at(box_point(&sp, &vp, 0.5, 0.5)), Some(Cursor::MoveFrame));
        let between = (box_point(&sp, &vp, 0.0, 1.0).y - c.y + r) / 2.0;
        assert_eq!(at(c + vec2(0.0, between)), Some(Cursor::Rotate));
        assert_eq!(at(c + vec2(r + 12.0, 0.0)), Some(Cursor::HandOpen));
    }

    /// In the window, on the Mask row: the tools' pens over an anchor and
    /// a line of the review masks (none of BananaV's where a tool does
    /// nothing), the rectangle tool's crosshair everywhere.
    #[test]
    fn the_mask_tools_show_their_pens_in_the_window() {
        use crate::pancrop_mask::MaskTool;
        use bv_timeline::mask::segment_point;
        for (tool, on_anchor, on_line) in [
            (MaskTool::DeleteAnchor, Some(Cursor::PenDelete), None),
            (
                MaskTool::SplitTangent,
                Some(Cursor::PenSplitTangent),
                Some(Cursor::PenAdd),
            ),
            (
                MaskTool::AddAnchor,
                Some(Cursor::PenTangent),
                Some(Cursor::PenAdd),
            ),
            (MaskTool::Normal, Some(Cursor::Arrow), Some(Cursor::Arrow)),
            (
                MaskTool::Rectangle,
                Some(Cursor::Crosshair),
                Some(Cursor::Crosshair),
            ),
        ] {
            let ctx = egui::Context::default();
            let mut app = BananaApp::new(ctx.clone());
            let (id, sp) = masked(&ctx, &mut app, tool);
            let s = shapes(&app, id);
            let anchor = sp.screen64((s[0][1].at[0], s[0][1].at[1]));
            let mid = segment_point(&s[0], 1, 0.5);
            let line = sp.screen64((mid[0], mid[1]));
            let mut at = |p: Pos2| {
                pointer_after(&ctx, &mut app, vec![egui::Event::PointerMoved(p)]);
                pointer_after(&ctx, &mut app, Vec::new())
            };
            assert_eq!(at(anchor), on_anchor, "{tool:?} on an anchor");
            assert_eq!(at(line), on_line, "{tool:?} on a line");
        }
    }
}
