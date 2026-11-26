//! An effect's description — its settings, presets and handles on the
//! preview — and what builds its steps from its settings' values.
//!
//! Lengths are percents of the frame's height and places parts of the
//! frame, so that a project of any size and a preview of any quality look
//! the same; times are seconds.

use crate::bolt::Guide;
use crate::graph::{Data, Graph, Kernel, Params, Pass, ROWS, Slot};

/// Where an effect is listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Group {
    Color,
    Blur,
    Distort,
    Light,
    Generate,
    Stylize,
    Time,
    Dissolves,
    Wipes,
    Motion,
}

impl Group {
    /// Its name: the part of OpenFX's grouping after the pack's.
    pub fn name(self) -> &'static str {
        match self {
            Group::Color => "Color",
            Group::Blur => "Blur & Sharpen",
            Group::Distort => "Distort",
            Group::Light => "Light",
            Group::Generate => "Generate",
            Group::Stylize => "Stylize",
            Group::Time => "Time",
            Group::Dissolves => "Dissolves",
            Group::Wipes => "Wipes",
            Group::Motion => "Motion",
        }
    }
}

/// What an effect works on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A picture.
    Filter,
    /// Nothing, or the picture under what it draws.
    Generator,
    /// Two pictures.
    Transition,
}

/// A setting's kind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// A number: the range it may have, the range its slider shows.
    Num {
        hard: [f64; 2],
        show: [f64; 2],
    },
    /// Degrees, counted anticlockwise.
    Angle,
    Int {
        lo: i32,
        hi: i32,
    },
    Choice(&'static [&'static str]),
    Toggle,
    /// A place: parts of the frame from its lower left.
    Point,
    /// A color.
    Color,
    /// A button the host answers: what it asks for (a mask's tracking).
    Button(&'static str),
    /// A section's start, open or closed at first; [`Shape::End`] ends it.
    Section(bool),
    End,
}

/// A setting.
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub shape: Shape,
    pub default: [f64; 3],
    /// Never keyframed.
    pub still: bool,
    /// Kept with the others, never shown.
    pub hidden: bool,
}

impl Param {
    /// How many numbers it has.
    pub fn len(&self) -> usize {
        match self.shape {
            Shape::Point => 2,
            Shape::Color => 3,
            Shape::Section(_) | Shape::End | Shape::Button(_) => 0,
            _ => 1,
        }
    }

    /// Whether it holds no value.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A factory preset: its name and the values it gives settings.
#[derive(Clone, Debug, PartialEq)]
pub struct Preset {
    pub name: &'static str,
    pub values: &'static [(&'static str, &'static [f64])],
}

/// What is drawn and dragged on the preview (besides the mask).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Handle {
    /// A place.
    Point(&'static str),
    /// Two places and the line between them.
    Line(&'static str, &'static str),
    /// A circle round a place: its radius, percents of the frame's height.
    Ring(&'static str, &'static str),
    /// An arrow from a place ("" the frame's middle) along an angle.
    Arrow(&'static str, &'static str),
    /// Four places round a picture.
    Quad([&'static str; 4]),
    /// A curve between two places, through others that are put in and
    /// taken out (see [`Fx::path`]).
    Path(&'static str, &'static str),
}

/// How a transition's frames run over its time.
const EASE: &[&str] = &["Linear", "Smooth", "Ease In", "Ease Out", "Snap", "Bounce"];

/// Past the frame's edges.
pub const EDGES: &[&str] = &["Transparent", "Hold", "Mirror", "Repeat"];

type Draw = fn(&mut B<'_>) -> Slot;
type Frames = fn(&V<'_>, f64) -> Vec<f64>;

/// An effect.
#[derive(Clone, Debug)]
pub struct Fx {
    /// Its identifier's last part.
    pub id: &'static str,
    /// Its name, in English.
    pub name: &'static str,
    pub group: Group,
    pub kind: Kind,
    pub about: &'static str,
    pub params: Vec<Param>,
    pub presets: Vec<Preset>,
    pub handles: Vec<Handle>,
    /// Whether what it draws changes as time goes (with nothing keyed).
    pub moves: bool,
    /// A generator's way of laying itself on a picture at first (an index
    /// of [`LAY`]).
    lay: i32,
    draw: Draw,
    frames: Option<Frames>,
}

/// How a generator lies on the picture under it.
pub const LAY: &[&str] = &["Over", "Add", "Screen", "Multiply", "Soft Light", "Replace"];

const MASKS: &[&str] = &["Off", "Ellipse", "Rectangle", "Diamond", "Rounded"];

/// The settings every filter ends with, by name.
pub mod common {
    pub const MIX: &str = "mix";
    pub const MASK: &str = "mask";
    pub const MASK_CENTER: &str = "maskCenter";
    pub const MASK_WIDTH: &str = "maskWidth";
    pub const MASK_HEIGHT: &str = "maskHeight";
    pub const MASK_ANGLE: &str = "maskAngle";
    pub const MASK_FEATHER: &str = "maskFeather";
    pub const MASK_INVERT: &str = "maskInvert";
    pub const MASK_SHOW: &str = "maskShow";
    pub const MASK_TRACK: &str = "maskTrack";
    /// What the tracker reads: the button's `kBananaVParamPropMaskTracking`.
    pub const MASK_TRACKING: &str = "1 maskCenter maskAngle maskWidth maskHeight maskPrecision maskMode maskDirection maskInterval";
    pub const LAY: &str = "lay";
    pub const EASE: &str = "ease";
    /// How many places a path goes through, how it goes through them, and
    /// how many have been given a place ([`super::Fx::path`]).
    pub const POINTS: &str = "points";
    pub const CURVE: &str = "curve";
    pub const PLACED: &str = "placed";
}

/// The places a path may go through ([`Fx::path`]).
pub const POINTS: [&str; 10] = [
    "point1", "point2", "point3", "point4", "point5", "point6", "point7", "point8", "point9",
    "point10",
];

const POINT_LABELS: [&str; 10] = [
    "Point 1", "Point 2", "Point 3", "Point 4", "Point 5", "Point 6", "Point 7", "Point 8",
    "Point 9", "Point 10",
];

fn unset(_: &mut B<'_>) -> Slot {
    Slot(0)
}

impl Fx {
    fn new(
        kind: Kind,
        id: &'static str,
        name: &'static str,
        group: Group,
        about: &'static str,
    ) -> Fx {
        Fx {
            id,
            name,
            group,
            kind,
            about,
            params: Vec::new(),
            presets: Vec::new(),
            handles: Vec::new(),
            moves: false,
            lay: 0,
            draw: unset,
            frames: None,
        }
    }

    pub fn filter(id: &'static str, name: &'static str, group: Group, about: &'static str) -> Fx {
        Fx::new(Kind::Filter, id, name, group, about)
    }

    pub fn generator(id: &'static str, name: &'static str, about: &'static str) -> Fx {
        Fx::new(Kind::Generator, id, name, Group::Generate, about)
    }

    pub fn transition(
        id: &'static str,
        name: &'static str,
        group: Group,
        about: &'static str,
    ) -> Fx {
        Fx::new(Kind::Transition, id, name, group, about)
    }

    fn push(
        mut self,
        name: &'static str,
        label: &'static str,
        shape: Shape,
        default: [f64; 3],
    ) -> Fx {
        self.params.push(Param {
            name,
            label,
            hint: "",
            shape,
            default,
            still: false,
            hidden: false,
        });
        self
    }

    /// A number whose slider runs from `lo` to `hi`; what may be typed goes
    /// four times as far up (and down, below 0).
    pub fn num(
        self,
        name: &'static str,
        label: &'static str,
        default: f64,
        lo: f64,
        hi: f64,
    ) -> Fx {
        let low = if lo < 0.0 { lo * 4.0 } else { lo };
        self.push(
            name,
            label,
            Shape::Num {
                hard: [low, hi * 4.0],
                show: [lo, hi],
            },
            [default, 0.0, 0.0],
        )
    }

    /// A number that may be nothing but what its slider shows.
    pub fn within(
        self,
        name: &'static str,
        label: &'static str,
        default: f64,
        lo: f64,
        hi: f64,
    ) -> Fx {
        self.push(
            name,
            label,
            Shape::Num {
                hard: [lo, hi],
                show: [lo, hi],
            },
            [default, 0.0, 0.0],
        )
    }

    /// Percents, 0 to 100 and nothing else.
    pub fn pct(self, name: &'static str, label: &'static str, default: f64) -> Fx {
        self.within(name, label, default, 0.0, 100.0)
    }

    pub fn angle(self, name: &'static str, label: &'static str, default: f64) -> Fx {
        self.push(name, label, Shape::Angle, [default, 0.0, 0.0])
    }

    pub fn int(
        self,
        name: &'static str,
        label: &'static str,
        default: i32,
        lo: i32,
        hi: i32,
    ) -> Fx {
        self.push(
            name,
            label,
            Shape::Int { lo, hi },
            [f64::from(default), 0.0, 0.0],
        )
    }

    pub fn choice(
        self,
        name: &'static str,
        label: &'static str,
        default: i32,
        options: &'static [&'static str],
    ) -> Fx {
        self.push(
            name,
            label,
            Shape::Choice(options),
            [f64::from(default), 0.0, 0.0],
        )
    }

    pub fn toggle(self, name: &'static str, label: &'static str, default: bool) -> Fx {
        self.push(
            name,
            label,
            Shape::Toggle,
            [f64::from(u8::from(default)), 0.0, 0.0],
        )
    }

    pub fn point(self, name: &'static str, label: &'static str, default: [f64; 2]) -> Fx {
        self.push(name, label, Shape::Point, [default[0], default[1], 0.0])
    }

    pub fn color(self, name: &'static str, label: &'static str, default: [f64; 3]) -> Fx {
        self.push(name, label, Shape::Color, default)
    }

    pub fn section(self, name: &'static str, label: &'static str, open: bool) -> Fx {
        self.push(name, label, Shape::Section(open), [0.0; 3])
    }

    pub fn end(self) -> Fx {
        self.push("", "", Shape::End, [0.0; 3])
    }

    /// What the last setting is for, shown over it.
    pub fn hint(mut self, text: &'static str) -> Fx {
        if let Some(p) = self.params.last_mut() {
            p.hint = text;
        }
        self
    }

    pub fn seed(self) -> Fx {
        self.int("seed", "Seed", 1, 0, 9999)
    }

    /// What past the frame's edges shows.
    pub fn edges(self, default: i32) -> Fx {
        self.choice("edges", "Edges", default, EDGES)
    }

    /// How light is laid.
    pub fn blend(self, default: i32) -> Fx {
        self.choice("blend", "Blend", default, &["Add", "Screen"])
    }

    /// How a transition's frames run over its time.
    pub fn ease(self, default: i32) -> Fx {
        self.choice(common::EASE, "Speed", default, EASE)
    }

    pub fn preset(
        mut self,
        name: &'static str,
        values: &'static [(&'static str, &'static [f64])],
    ) -> Fx {
        self.presets.push(Preset { name, values });
        self
    }

    pub fn handle(mut self, h: Handle) -> Fx {
        self.handles.push(h);
        self
    }

    /// A curve between places `from` and `to`, through up to ten others put
    /// in and taken out on the preview.
    pub fn path(
        mut self,
        (from, a): (&'static str, [f64; 2]),
        (to, b): (&'static str, [f64; 2]),
    ) -> Fx {
        self = self
            .point(from, "From", a)
            .point(to, "To", b)
            .int(common::POINTS, "Points Between", 0, 0, POINTS.len() as i32)
            .hint("The places it bends through. On the preview: drag a small plus between two places to add one there, or double-click the line; double-click a place, or click it holding Ctrl, to take it away.");
        if let Some(p) = self.params.last_mut() {
            p.still = true;
        }
        for (n, (name, label)) in POINTS.into_iter().zip(POINT_LABELS).enumerate() {
            // Along the way at first, to one side and the other.
            let t = (n as f64 + 1.0) / (POINTS.len() as f64 + 1.0);
            let side = if n % 2 == 0 { 0.1 } else { -0.1 };
            let at = [a[0] + (b[0] - a[0]) * t + side, a[1] + (b[1] - a[1]) * t];
            self = self.point(name, label, at);
        }
        self = self
            .choice(common::CURVE, "Curve", 0, &["Smooth", "Straight"])
            .int(common::PLACED, "Placed", 0, 0, POINTS.len() as i32);
        if let Some(p) = self.params.last_mut() {
            p.still = true;
            p.hidden = true;
        }
        self.handles.push(Handle::Path(from, to));
        self
    }

    /// Changes as time goes.
    pub fn moving(mut self) -> Fx {
        self.moves = true;
        self
    }

    /// How a generator lies on a picture at first.
    pub fn lay(mut self, mode: i32) -> Fx {
        self.lay = mode;
        self
    }

    /// The other frames it reads: their times from its settings and the
    /// time now (frames).
    pub fn frames(mut self, f: Frames) -> Fx {
        self.frames = Some(f);
        self
    }

    /// Whether it reads other frames.
    pub fn is_temporal(&self) -> bool {
        self.frames.is_some()
    }

    /// The other frames' times with settings `v`, at `frame`.
    pub fn times(&self, v: &V<'_>, frame: f64) -> Vec<f64> {
        self.frames.map_or_else(Vec::new, |f| {
            let mut t = f(v, frame);
            t.truncate(MAX_FRAMES);
            t
        })
    }

    /// The steps it is drawn with; finishes the description.
    pub fn draw(mut self, f: Draw) -> Fx {
        self.draw = f;
        match self.kind {
            Kind::Filter => self.with_mix(),
            Kind::Generator => {
                let lay = self.lay;
                let mut fx = self
                    .section("compose", "Over the Picture", true)
                    .choice(common::LAY, "Compose", lay, LAY)
                    .hint("How it lies on the picture when used as an effect; a generated media shows it alone.");
                fx = fx.end();
                fx.with_mix()
            }
            Kind::Transition => self,
        }
    }

    fn with_mix(self) -> Fx {
        self.section("mixing", "Mix & Mask", false)
            .pct(common::MIX, "Mix", 100.0)
            .hint("How much of the effect shows: 0 leaves the picture as it was.")
            .choice(common::MASK, "Mask", 0, MASKS)
            .hint("Keeps the effect inside a shape you place on the preview.")
            .point(common::MASK_CENTER, "Mask Center", [0.5, 0.5])
            .within(common::MASK_WIDTH, "Mask Width", 0.5, 0.01, 3.0)
            .within(common::MASK_HEIGHT, "Mask Height", 0.5, 0.01, 3.0)
            .angle(common::MASK_ANGLE, "Mask Angle", 0.0)
            .pct(common::MASK_FEATHER, "Mask Feather", 4.0)
            .toggle(common::MASK_INVERT, "Invert Mask", false)
            .toggle(common::MASK_SHOW, "Show Mask", false)
            .end()
            .section("tracking", "Mask Tracking", false)
            .push(
                common::MASK_TRACK,
                "Track Mask",
                Shape::Button(common::MASK_TRACKING),
                [0.0; 3],
            )
            .hint("Follows what the mask covers from the cursor on, keyframing the mask.")
            .choice(
                "maskPrecision",
                "Precision",
                0,
                &["Normal", "High Precision"],
            )
            .choice(
                "maskMode",
                "Follow",
                0,
                &[
                    "Location",
                    "Rotation & Location",
                    "Size, Rotation & Location",
                ],
            )
            .choice(
                "maskDirection",
                "Direction",
                0,
                &["Forward", "Backward", "Both"],
            )
            .int("maskInterval", "Keyframe Interval", 5, 1, 100)
            .end()
    }
}

/// How many other frames an effect reads at most.
pub const MAX_FRAMES: usize = 16;

/// An effect's settings' values.
#[derive(Debug)]
pub struct V<'a> {
    fx: &'a Fx,
    values: Vec<[f64; 3]>,
}

impl<'a> V<'a> {
    /// `fx`'s values: what `get` gives for each setting, its default where
    /// it gives nothing (or something that is no number).
    pub fn of(fx: &'a Fx, get: impl Fn(&Param) -> Option<[f64; 3]>) -> V<'a> {
        let values = fx
            .params
            .iter()
            .map(|p| match get(p) {
                Some(v) if v.iter().all(|x| x.is_finite()) => p.held(v),
                _ => p.default,
            })
            .collect();
        V { fx, values }
    }

    fn get(&self, name: &str) -> [f64; 3] {
        let found = self.fx.params.iter().position(|p| p.name == name);
        debug_assert!(found.is_some(), "{}: no setting {name}", self.fx.id);
        found.map_or([0.0; 3], |i| self.values[i])
    }

    /// Whether the effect has the setting.
    pub fn has(&self, name: &str) -> bool {
        self.fx.params.iter().any(|p| p.name == name)
    }

    pub fn f64(&self, name: &str) -> f64 {
        self.get(name)[0]
    }

    /// A number.
    pub fn n(&self, name: &str) -> f32 {
        self.f64(name) as f32
    }

    /// Percents, as parts of 1.
    pub fn k(&self, name: &str) -> f32 {
        (self.f64(name) / 100.0) as f32
    }

    /// An integer or a choice.
    pub fn i(&self, name: &str) -> i32 {
        self.f64(name).round() as i32
    }

    pub fn on(&self, name: &str) -> bool {
        self.f64(name) >= 0.5
    }

    pub fn color(&self, name: &str) -> [f32; 3] {
        self.get(name).map(|v| v as f32)
    }

    /// A place, as the host counts it.
    pub fn xy(&self, name: &str) -> [f64; 2] {
        let v = self.get(name);
        [v[0], v[1]]
    }

    /// The names of the places a path goes through, between its ends.
    pub fn between(&self) -> &'static [&'static str] {
        let n = self.i(common::POINTS).max(0) as usize;
        &POINTS[..n.min(POINTS.len())]
    }

    /// Degrees as radians, counted as the kernels do (y grows down, so
    /// clockwise): an angle that grows turns anticlockwise on the screen.
    pub fn a(&self, name: &str) -> f32 {
        (-self.f64(name).to_radians()) as f32
    }
}

impl Param {
    /// A value held in its range.
    fn held(&self, v: [f64; 3]) -> [f64; 3] {
        match self.shape {
            Shape::Num { hard, .. } => [v[0].clamp(hard[0], hard[1]), 0.0, 0.0],
            Shape::Int { lo, hi } => [v[0].clamp(f64::from(lo), f64::from(hi)), 0.0, 0.0],
            Shape::Choice(o) => [v[0].clamp(0.0, o.len().saturating_sub(1) as f64), 0.0, 0.0],
            Shape::Angle => [v[0].clamp(-1e6, 1e6), 0.0, 0.0],
            Shape::Point => [v[0].clamp(-100.0, 100.0), v[1].clamp(-100.0, 100.0), 0.0],
            Shape::Color => v.map(|c| c.clamp(-100.0, 100.0)),
            _ => v,
        }
    }
}

/// When and how big a frame is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// Pixels drawn.
    pub w: u32,
    pub h: u32,
    /// Pixels drawn for each of the full frame's.
    pub scale: f32,
    /// The frame's width over its height, as shown.
    pub shape: f32,
    /// The time, in frames and in frames a second.
    pub frame: f64,
    pub rate: f64,
    /// The effect's length in frames (0: not known).
    pub length: f64,
    /// A transition's progress.
    pub progress: f64,
    /// A generator drawn alone.
    pub alone: bool,
    /// How many other frames there are (see [`Fx::times`]).
    pub others: usize,
}

/// Numbers for a kernel, put one after another.
#[derive(Clone, Copy, Debug)]
pub struct N {
    f: [f32; ROWS * 4],
    n: usize,
}

impl Default for N {
    fn default() -> N {
        N {
            f: [0.0; ROWS * 4],
            n: 0,
        }
    }
}

/// What [`N`] takes.
pub trait Put {
    fn put(&self, n: &mut N);
}

impl Put for f32 {
    fn put(&self, n: &mut N) {
        debug_assert!(n.n < n.f.len());
        if let Some(f) = n.f.get_mut(n.n) {
            *f = if self.is_finite() { *self } else { 0.0 };
        }
        n.n += 1;
    }
}

impl<const M: usize> Put for [f32; M] {
    fn put(&self, n: &mut N) {
        for v in self {
            v.put(n);
        }
    }
}

impl Put for bool {
    fn put(&self, n: &mut N) {
        f32::from(u8::from(*self)).put(n);
    }
}

/// The numbers of a kernel, flat: `nums![1.0, b.color("tint")]`.
#[macro_export]
macro_rules! nums {
    ($($v:expr),* $(,)?) => {{
        #[allow(unused_mut)]
        let mut n = $crate::fx::N::default();
        $($crate::fx::Put::put(&$v, &mut n);)*
        n
    }};
}

/// Builds an effect's steps.
#[derive(Debug)]
pub struct B<'a> {
    pub v: &'a V<'a>,
    pub at: Frame,
    kind: Kind,
    sizes: Vec<[u32; 2]>,
    data: Vec<Data>,
    passes: Vec<Pass>,
    inputs: u16,
}

impl<'a> std::ops::Deref for B<'a> {
    type Target = V<'a>;

    fn deref(&self) -> &V<'a> {
        self.v
    }
}

impl B<'_> {
    /// The picture the effect gets (a transition's first).
    pub fn src(&self) -> Slot {
        Slot(0)
    }

    /// A transition's second picture.
    pub fn to(&self) -> Slot {
        Slot(1)
    }

    /// The `n`th of the other frames it asked for ([`Fx::frames`]); the
    /// frame itself where there is none.
    pub fn other(&self, n: usize) -> Slot {
        if n < self.at.others {
            Slot(1 + n as u16)
        } else {
            Slot(0)
        }
    }

    pub fn w(&self) -> u32 {
        self.at.w
    }

    pub fn h(&self) -> u32 {
        self.at.h
    }

    /// Seconds from the effect's start.
    pub fn time(&self) -> f32 {
        (self.at.frame / self.at.rate.max(1e-3)) as f32
    }

    /// The time `speed` (a setting) times as fast.
    pub fn run_time(&self, speed: &str) -> f32 {
        self.time() * self.n(speed)
    }

    pub fn seed_of(&self, name: &str) -> u32 {
        self.i(name).max(0) as u32
    }

    /// A place, in frame heights from the frame's middle (y down).
    pub fn p(&self, name: &str) -> [f32; 2] {
        let [x, y] = self.xy(name);
        [(x as f32 - 0.5) * self.at.shape, 0.5 - y as f32]
    }

    /// The path from place `from` to place `to` ([`Fx::path`]).
    pub fn guide(&self, from: &str, to: &str) -> Guide {
        let mut places = vec![self.p(from)];
        places.extend(self.between().iter().map(|n| self.p(n)));
        places.push(self.p(to));
        Guide::through(&places, self.i(common::CURVE) == 0)
    }

    /// Percents of the frame's height, as frame heights.
    pub fn len(&self, name: &str) -> f32 {
        self.k(name)
    }

    /// A length in frame heights as pixels across.
    pub fn px_x(&self, len: f32) -> f32 {
        len * self.at.w as f32 / self.at.shape
    }

    /// A length in frame heights as pixels down.
    pub fn px_y(&self, len: f32) -> f32 {
        len * self.at.h as f32
    }

    /// How far a transition is, 0 to 1, along its speed.
    pub fn t(&self) -> f32 {
        let t = self.at.progress.clamp(0.0, 1.0);
        let e = if self.has(common::EASE) {
            self.i(common::EASE)
        } else {
            0
        };
        let v = match e {
            1 => t * t * (3.0 - 2.0 * t),
            2 => t * t * t,
            3 => 1.0 - (1.0 - t).powi(3),
            4 => {
                let s = t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
                s * s * (3.0 - 2.0 * s)
            }
            5 => {
                // Past the end and back, a little.
                let s = 1.0 - (1.0 - t).powi(3);
                s + (t * std::f64::consts::PI).sin().powi(2) * (1.0 - t) * 0.35
            }
            _ => t,
        };
        v as f32
    }

    /// How strong a transition's middle is: 0 at its ends, 1 halfway.
    pub fn bell(&self) -> f32 {
        let t = self.at.progress.clamp(0.0, 1.0);
        let s = (t * std::f64::consts::PI).sin();
        (s * s * (3.0 - 2.0 * s)).max(0.0).powf(0.85) as f32
    }

    /// A picture of numbers worked out here, for kernels to read.
    pub fn data(&mut self, size: [u32; 2], pixels: Vec<[f32; 4]>) -> Slot {
        debug_assert_eq!(size[0] as usize * size[1] as usize, pixels.len());
        let slot = Slot(self.sizes.len() as u16);
        self.sizes.push(size);
        self.data.push(Data {
            slot,
            size,
            pixels: pixels.into(),
        });
        slot
    }

    /// Runs kernel `k` on `inputs` with `numbers` and `ints` into a new
    /// picture `size` big.
    pub fn run_sized(
        &mut self,
        k: Kernel,
        inputs: &[Slot],
        numbers: N,
        ints: [i32; 4],
        size: [u32; 2],
        seed: u32,
    ) -> Slot {
        let first = inputs.first().copied().unwrap_or(Slot(0));
        let input: [Slot; 3] = std::array::from_fn(|n| inputs.get(n).copied().unwrap_or(first));
        let of = |s: Slot| self.sizes.get(s.0 as usize).copied().unwrap_or([1, 1]);
        let [a, b, c] = input.map(of);
        let mut params = Params::ZERO;
        [params.w, params.h] = size.map(|n| n.max(1));
        [params.w0, params.h0] = a;
        [params.w1, params.h1] = b;
        [params.w2, params.h2] = c;
        params.scale = self.at.scale * size[1] as f32 / self.at.h.max(1) as f32;
        params.shape = self.at.shape;
        params.time = self.time();
        params.seed = seed;
        params.i = ints;
        for (n, f) in numbers.f.iter().enumerate() {
            params.f[n / 4][n % 4] = *f;
        }
        let out = Slot(self.sizes.len() as u16);
        self.sizes.push([params.w, params.h]);
        self.passes.push(Pass {
            kernel: k,
            inputs: input,
            out,
            params,
        });
        out
    }

    /// The seed the settings give.
    fn own_seed(&self) -> u32 {
        if self.has("seed") {
            self.seed_of("seed")
        } else {
            1
        }
    }

    /// [`Self::run_sized`] at the frame's size.
    pub fn run(&mut self, k: Kernel, inputs: &[Slot], numbers: N, ints: [i32; 4]) -> Slot {
        let seed = self.own_seed();
        self.run_sized(k, inputs, numbers, ints, [self.at.w, self.at.h], seed)
    }

    /// A kernel at a picture's own size.
    pub fn run_like(
        &mut self,
        like: Slot,
        k: Kernel,
        inputs: &[Slot],
        numbers: N,
        ints: [i32; 4],
    ) -> Slot {
        let seed = self.own_seed();
        let size = self.size(like);
        self.run_sized(k, inputs, numbers, ints, size, seed)
    }

    /// A picture's size.
    pub fn size(&self, s: Slot) -> [u32; 2] {
        self.sizes.get(s.0 as usize).copied().unwrap_or([1, 1])
    }

    /// A picture blurred: a Gaussian whose sigma is `len` (frame heights).
    /// `edges`: 0 nothing past them, 1 held, 2 mirrored.
    pub fn blur(&mut self, s: Slot, len: f32, edges: i32) -> Slot {
        self.blur_xy(s, len, len, edges)
    }

    /// [`Self::blur`] with its own sigma each way.
    pub fn blur_xy(&mut self, s: Slot, x: f32, y: f32, edges: i32) -> Slot {
        let [w, h] = self.size(s);
        let sx = x * w as f32 / self.at.shape;
        let sy = y * h as f32;
        self.blur_px(s, sx, sy, edges)
    }

    /// [`Self::blur`] with its sigmas in the picture's pixels.
    pub fn blur_px(&mut self, s: Slot, sx: f32, sy: f32, edges: i32) -> Slot {
        use crate::generated::k;
        let full = self.size(s);
        let (sx, sy) = (sx.max(0.0), sy.max(0.0));
        if sx < 0.05 && sy < 0.05 {
            return s;
        }
        let edge = edges.clamp(0, 2);
        // Halved while it stays wide enough to blur well.
        let halvings = |sigma: f32, n: u32| -> u32 {
            let mut l = 0;
            while l < 10 && sigma / f32::from(1u16 << (l + 1)) > 2.6 && (n >> (l + 1)) > 4 {
                l += 1;
            }
            l
        };
        let (lx, ly) = (halvings(sx, full[0]), halvings(sy, full[1]));
        // What is left to blur once the halvings and the enlarging (both
        // blur a little) are counted.
        let rest = |sigma: f32, l: u32| -> f32 {
            let f = f32::from(1u16 << l);
            let own = if l == 0 {
                0.0
            } else {
                0.25 * (1.0 - 1.0 / (f * f)) + 1.0 / 3.0
            };
            ((sigma / f).powi(2) - own).max(0.0).sqrt()
        };
        let (rx, ry) = (rest(sx, lx), rest(sy, ly));
        let mut cur = s;
        let mut size = full;
        for _ in 0..lx {
            size[0] = size[0].div_ceil(2);
            cur = self.run_sized(k::HALF_X, &[cur], N::default(), [edge, 0, 0, 0], size, 0);
        }
        for _ in 0..ly {
            size[1] = size[1].div_ceil(2);
            cur = self.run_sized(k::HALF_Y, &[cur], N::default(), [edge, 0, 0, 0], size, 0);
        }
        let reach = |r: f32| ((r * 3.0).ceil() as i32).clamp(1, 24);
        if rx >= 0.05 {
            let n = crate::nums![rx];
            cur = self.run_sized(k::GAUSS_X, &[cur], n, [edge, reach(rx), 0, 0], size, 0);
        }
        if ry >= 0.05 {
            let n = crate::nums![ry];
            cur = self.run_sized(k::GAUSS_Y, &[cur], n, [edge, reach(ry), 0, 0], size, 0);
        }
        if lx > 0 || ly > 0 {
            let n = crate::nums![f32::from(1u16 << lx), f32::from(1u16 << ly)];
            cur = self.run_sized(k::UP, &[cur], n, [edge, 0, 0, 0], full, 0);
        }
        cur
    }

    /// A picture halved `n` times each way.
    pub fn smaller(&mut self, s: Slot, n: u32, edges: i32) -> Slot {
        use crate::generated::k;
        let mut cur = s;
        let mut size = self.size(s);
        for _ in 0..n {
            size[0] = size[0].div_ceil(2);
            cur = self.run_sized(k::HALF_X, &[cur], N::default(), [edges, 0, 0, 0], size, 0);
            size[1] = size[1].div_ceil(2);
            cur = self.run_sized(k::HALF_Y, &[cur], N::default(), [edges, 0, 0, 0], size, 0);
        }
        cur
    }

    /// A picture made smaller ([`Self::smaller`] `n` times) back at the
    /// frame's size, smoothly.
    pub fn larger(&mut self, s: Slot, n: u32, edges: i32) -> Slot {
        use crate::generated::k;
        if n == 0 {
            return s;
        }
        let f = f32::from(1u16 << n.min(12));
        self.run(k::UP, &[s], crate::nums![f, f], [edges, 0, 0, 0])
    }

    /// How many times to halve a picture for work that needs no more than
    /// `rows` of them (a preview of fewer halves less often).
    pub fn halvings(&self, rows: u32) -> u32 {
        let mut n = 0;
        while (self.at.h >> (n + 1)) >= rows {
            n += 1;
        }
        n
    }

    /// A picture drawn out along a line `dx`, `dy` long (frame heights),
    /// `shift` of it off its middle: in two steps, so that a long one is
    /// smooth.
    pub fn smear(&mut self, s: Slot, [dx, dy]: [f32; 2], shift: f32, edges: i32) -> Slot {
        use crate::generated::k;
        let [w, h] = self.size(s);
        let d = [dx * w as f32 / self.at.shape, dy * h as f32];
        let long = d[0].hypot(d[1]);
        if long < 0.35 {
            return s;
        }
        // The first step fills the gaps the second leaves.
        let n = ((long / 2.0).sqrt().ceil() as i32 + 2).clamp(5, 28);
        let small = [d[0] / n as f32, d[1] / n as f32];
        let ints = [edges, n, 0, 0];
        let a = self.run_like(s, k::SMEAR, &[s], crate::nums![small, 0.0], ints);
        self.run_like(s, k::SMEAR, &[a], crate::nums![d, shift], ints)
    }

    /// A picture drawn out round a place: zoomed (the log of how many
    /// times) and turned, in two steps. `from` and `over`: from how far out
    /// (frame heights) and over how far it grows to whole.
    #[allow(clippy::too_many_arguments)]
    pub fn spin(
        &mut self,
        s: Slot,
        center: [f32; 2],
        zoom: f32,
        turn: f32,
        shift: f32,
        edges: i32,
        (from, over): (f32, f32),
    ) -> Slot {
        use crate::generated::k;
        let h = self.size(s)[1] as f32;
        let reach = 0.5 * self.at.shape.hypot(1.0) * h;
        let long = (zoom.abs().min(2.0) + turn.abs()) * reach;
        if long < 0.35 {
            return s;
        }
        let n = ((long / 2.0).sqrt().ceil() as i32 + 2).clamp(5, 30);
        let m = n as f32;
        let ints = [edges, n, 0, 0];
        let first = crate::nums![center, zoom / m, turn / m, 0.0, from, over];
        let a = self.run_like(s, k::SPIN, &[s], first, ints);
        let second = crate::nums![center, zoom, turn, shift, from, over];
        self.run_like(s, k::SPIN, &[a], second, ints)
    }

    /// Two pictures mixed.
    pub fn mix(&mut self, a: Slot, b: Slot, t: f32) -> Slot {
        if t <= 0.0 {
            return a;
        }
        if t >= 1.0 {
            return b;
        }
        self.run(crate::generated::k::MIX, &[a, b], crate::nums![t], [0; 4])
    }

    /// The bright parts as light: past `threshold`, `soft` softly.
    pub fn bright(&mut self, s: Slot, threshold: f32, soft: f32, sat: f32, gain: f32) -> Slot {
        let n = crate::nums![threshold, soft, sat, gain];
        self.run_like(s, crate::generated::k::BRIGHT, &[s], n, [0; 4])
    }
}

impl Fx {
    /// The steps of a frame with settings `v`.
    pub fn graph(&self, v: &V<'_>, at: Frame) -> Graph {
        let inputs: u16 = match self.kind {
            Kind::Transition => 2,
            _ => 1 + at.others.min(MAX_FRAMES) as u16,
        };
        let mut b = B {
            v,
            at,
            kind: self.kind,
            sizes: vec![[at.w.max(1), at.h.max(1)]; inputs as usize],
            data: Vec::new(),
            passes: Vec::new(),
            inputs,
        };
        let out = match self.kind {
            // A transition's ends are the pictures themselves.
            Kind::Transition if at.progress <= 0.0 => Slot(0),
            Kind::Transition if at.progress >= 1.0 => Slot(1),
            _ => (self.draw)(&mut b),
        };
        b.finish(out)
    }

    /// Whether settings `v` leave the picture as it is.
    pub fn is_identity(&self, v: &V<'_>) -> bool {
        self.kind == Kind::Filter && v.k(common::MIX) <= 0.0 && !v.on(common::MASK_SHOW)
    }
}

impl B<'_> {
    fn finish(mut self, mut out: Slot) -> Graph {
        use crate::generated::k;
        if self.kind == Kind::Generator && !self.at.alone {
            let mode = self.i(common::LAY);
            out = self.run(k::LAY, &[Slot(0), out], crate::nums![1.0], [mode, 0, 0, 0]);
        }
        if self.kind != Kind::Transition && !self.at.alone {
            let mix = self.k(common::MIX);
            let shape = self.i(common::MASK);
            if mix < 1.0 || shape > 0 {
                let half = [
                    0.5 * self.n(common::MASK_WIDTH) * self.at.shape,
                    0.5 * self.n(common::MASK_HEIGHT),
                ];
                let n = crate::nums![
                    mix,
                    self.p(common::MASK_CENTER),
                    half,
                    self.a(common::MASK_ANGLE),
                    self.len(common::MASK_FEATHER) * 0.5,
                ];
                let ints = [
                    shape,
                    i32::from(self.on(common::MASK_INVERT)),
                    i32::from(self.on(common::MASK_SHOW)),
                    0,
                ];
                out = self.run(k::FINISH, &[Slot(0), out], n, ints);
            }
        }
        Graph {
            inputs: self.inputs,
            slots: self.sizes.len() as u16,
            data: self.data,
            passes: self.passes,
            out,
        }
    }
}
