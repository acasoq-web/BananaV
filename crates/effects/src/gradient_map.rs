//! Gradient Map: VEGAS's. Each pixel's BT.601 luma picks a color along a
//! gradient of color stops and the color is mixed in by the gradient's
//! alpha times Amount (Blend), or added (Add); the alpha rises by as much.
//!
//! The math, in single precision: the luma of the premultiplied pixel;
//! the gradient a spline through the stops in order of place (curvature 0.5, each
//! stop's weight 0.8) — their alpha through one, their colors
//! premultiplied by it through another —, the alpha held within 0–1 and
//! the colors within 0 and the alpha; that color times its alpha times
//! Amount mixed in. Transparent pixels take the gradient's start, as
//! VEGAS's do.
//!
//! Its panel is VEGAS's: the gradient's bar with a marker under it for
//! each stop — a click on a marker picks it (Color shows its color, and
//! editing Color recolors it), a drag moves it, a click elsewhere on the
//! bar adds a stop of the gradient's color there, a marker dragged well
//! below the bar goes (two always stay) —, Amount and Mode. The stops are
//! kept as text in a hidden parameter, "place red green blue alpha" each,
//! parted by semicolons. VEGAS's gradient when the effect is added has
//! three stops whose colors await a picture of its panel: BananaV's start
//! black, gray and white.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};

use bv_ofx_sys::extensions::kBananaVParamPropInteract;
use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::accel::Values;
use crate::ofx::{Draw, Fx, Props, interact_props, is, redraw, suites};
use crate::spline::{Piece, Point, Spline};
use crate::{Preset, PresetValue};

pub(crate) const ID: &CStr = c"org.bananav.GradientMap";

const GRADIENT: &CStr = c"gradient";
const STOPS: &CStr = c"stops";
const PICKED: &CStr = c"picked";
const COLOR: &CStr = c"color";
const AMOUNT: &CStr = c"amount";
const MODE: &CStr = c"mode";

/// The stops a new Gradient Map starts with.
pub(crate) const DEFAULT_STOPS: &CStr = c"0 0 0 0 1;0.5 0.5 0.5 0.5 1;1 1 1 1 1";

/// A stop of the gradient.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Stop {
    /// Where along the gradient: 0 is black's, 1 white's.
    pub at: f64,
    /// Its color, straight: red, green, blue and alpha.
    pub color: [f64; 4],
}

/// The stops in `text` ("place red green blue alpha;…").
pub(crate) fn parse(text: &str) -> Vec<Stop> {
    text.split(';')
        .filter_map(|s| {
            let v: Vec<f64> = s
                .split_whitespace()
                .filter_map(|x| x.parse().ok())
                .filter(|x: &f64| x.is_finite())
                .collect();
            (v.len() == 5).then(|| Stop {
                at: v[0].clamp(0.0, 1.0),
                color: [v[1], v[2], v[3], v[4]].map(|c| c.clamp(0.0, 1.0)),
            })
        })
        .collect()
}

/// The stops as their parameter keeps them.
pub(crate) fn write(stops: &[Stop]) -> String {
    stops
        .iter()
        .map(|s| {
            let [r, g, b, a] = s.color;
            format!("{} {r} {g} {b} {a}", s.at)
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// The gradient in single precision: its stops in order of place, their
/// alpha and their colors premultiplied by it through the spline (each
/// stop's weight 0.8), a piece before each stop and one past
/// the last.
pub(crate) struct Gradient {
    places: Vec<f64>,
    alpha: Vec<Piece>,
    colors: Vec<Piece>,
}

impl Gradient {
    pub(crate) fn new(stops: &[Stop]) -> Gradient {
        // In order of place, as VEGAS's selection sort puts them.
        let mut sorted = stops.to_vec();
        for i in 0..sorted.len() {
            let mut least = i;
            for j in i + 1..sorted.len() {
                if sorted[j].at < sorted[least].at {
                    least = j;
                }
            }
            sorted.swap(i, least);
        }
        let point = |s: &Stop, v: [f64; 3]| Point { t: s.at, v, w: 0.8 };
        let alpha: Vec<Point> = sorted
            .iter()
            .map(|s| point(s, [s.color[3], 0.0, 0.0]))
            .collect();
        let colors: Vec<Point> = sorted
            .iter()
            .map(|s| {
                let a = s.color[3];
                point(s, [a * s.color[0], a * s.color[1], a * s.color[2]])
            })
            .collect();
        let n = if sorted.is_empty() {
            0
        } else {
            sorted.len() + 1
        };
        Gradient {
            places: sorted.iter().map(|s| s.at).collect(),
            alpha: (0..n).map(|i| Spline(&alpha).piece(i)).collect(),
            colors: (0..n).map(|i| Spline(&colors).piece(i)).collect(),
        }
    }

    /// The gradient at `t`: its premultiplied red, green and blue (within
    /// 0 and its alpha) and its alpha (within 0–1), in single precision.
    pub(crate) fn at(&self, t: f64) -> [f32; 4] {
        if self.places.is_empty() {
            return [0.0; 4];
        }
        let i = self
            .places
            .iter()
            .position(|&p| p > t)
            .unwrap_or(self.places.len());
        let a = self.alpha[i].at(t)[0].clamp(0.0, 1.0) as f32;
        let c = self.colors[i].at(t);
        let held = |v: f64| v.clamp(0.0, f64::from(a)) as f32;
        [held(c[0]), held(c[1]), held(c[2]), a]
    }

    /// The card's table (`kit_gradient_map`): the stops' count, their
    /// places, then each piece's times and its Béziers — alpha, red,
    /// green, blue: the start, its two handles and the end.
    pub(crate) fn table(&self) -> Vec<[f32; 4]> {
        let mut t = vec![[self.places.len() as f32, 0.0, 0.0, 0.0]];
        t.extend(self.places.iter().map(|&p| [p as f32, 0.0, 0.0, 0.0]));
        for (a, c) in self.alpha.iter().zip(&self.colors) {
            t.push([a.t[0] as f32, a.t[1] as f32, 0.0, 0.0]);
            t.push(a.v[0].map(|v| v as f32));
            t.extend(c.v.iter().map(|v| v.map(|x| x as f32)));
        }
        t
    }
}

/// A premultiplied pixel through the gradient, as VEGAS's renders show it.
pub(crate) fn mapped(g: &Gradient, amount: f32, add: bool, p: [f32; 4]) -> [f32; 4] {
    let y = f64::from(p[1]) * 0.587 + f64::from(p[2]) * 0.114 + f64::from(p[0]) * 0.299;
    let c = g.at(y);
    let k = c[3] * amount;
    let rgb: [f32; 3] = if add {
        std::array::from_fn(|i| c[i] * k + p[i])
    } else {
        std::array::from_fn(|i| (1.0 - k) * p[i] + c[i] * k)
    };
    [rgb[0], rgb[1], rgb[2], (k + p[3]).min(1.0)]
}

/// The settings at a time.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Settings {
    pub stops: Vec<Stop>,
    pub amount: f64,
    pub add: bool,
}

impl Settings {
    /// The settings in `v`, a time's values as the host gives them (what
    /// the graphics card draws with).
    pub(crate) fn of(v: Values<'_>) -> Settings {
        Settings {
            stops: parse(
                v.text(STOPS)
                    .unwrap_or_else(|| DEFAULT_STOPS.to_str().unwrap_or_default()),
            ),
            amount: v.f64(AMOUNT, 1.0),
            add: v.int(MODE, 0) == 1,
        }
    }

    fn at(fx: Fx, time: f64) -> Settings {
        Settings {
            stops: parse(&fx.param(STOPS).text_at(time)),
            amount: fx.param(AMOUNT).f64_at(time),
            add: fx.param(MODE).int_at(time) == 1,
        }
    }

    /// Whether it changes nothing.
    pub(crate) fn changes_nothing(&self) -> bool {
        self.amount == 0.0 || self.stops.is_empty()
    }
}

/// The plug-in's entry point.
pub(crate) unsafe extern "C" fn main(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    out_args: OfxPropertySetHandle,
) -> OfxStatus {
    if suites().is_none() {
        return kOfxStatErrMissingHostFeature;
    }
    let fx = Fx(handle as OfxImageEffectHandle);
    let (args, out) = (Props(in_args), Props(out_args));
    if is(action, kOfxActionDescribe) {
        describe(fx);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionDescribeInContext) {
        describe_in_context(fx);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionRender) {
        render(fx, args)
    } else if is(action, kOfxImageEffectActionIsIdentity) {
        if Settings::at(fx, args.dbl(kOfxPropTime, 0)).changes_nothing() {
            out.set_str(kOfxPropName, kOfxImageEffectSimpleSourceClipName);
            kOfxStatOK
        } else {
            kOfxStatReplyDefault
        }
    } else if is(action, kOfxActionInstanceChanged) {
        let time = args.dbl(kOfxPropTime, 0);
        let name = args.text(kOfxPropName, 0);
        if name == "color" && args.is(kOfxPropChangeReason, kOfxChangeUserEdited) {
            // Color recolors the picked stop.
            let mut stops = parse(&fx.param(STOPS).text_at(time));
            let k = fx.param(PICKED).int_at(time).max(0) as usize;
            if let Some(s) = stops.get_mut(k) {
                s.color = fx.param(COLOR).rgba_at(time).map(|v| v.clamp(0.0, 1.0));
                fx.param(STOPS).set_text(&write(&stops));
            }
        } else if name == "stops" && !args.is(kOfxPropChangeReason, kOfxChangePluginEdited) {
            show_picked(fx, time);
        }
        kOfxStatOK
    } else if is(action, kOfxActionCreateInstance) {
        show_picked(fx, 0.0);
        kOfxStatOK
    } else if is(action, kOfxActionLoad)
        || is(action, kOfxActionUnload)
        || is(action, kOfxActionDestroyInstance)
    {
        kOfxStatOK
    } else {
        kOfxStatReplyDefault
    }
}

/// The render action: every pixel through the gradient, the transparent
/// ones too (they take the gradient's start, as VEGAS's do).
fn render(fx: Fx, args: Props) -> OfxStatus {
    let time = args.dbl(kOfxPropTime, 0);
    let [x1, y1, x2, y2]: [c_int; 4] = args.ints(kOfxImageEffectPropRenderWindow);
    let Some(out) = fx.clip(kOfxImageEffectOutputClipName).image(time) else {
        return kOfxStatFailed;
    };
    let src = fx.clip(kOfxImageEffectSimpleSourceClipName).image(time);
    if let Some(s) = &src
        && (s.depth != out.depth || s.channels != out.channels)
    {
        return kOfxStatErrImageFormat;
    }
    let source = crate::kit::Buf::read(src.as_ref());
    drop(src);
    let s = Settings::at(fx, time);
    let g = Gradient::new(&s.stops);
    let amount = s.amount.clamp(0.0, 1.0) as f32;
    let result = source.map(|x, y| mapped(&g, amount, s.add, source.at(x, y)));
    let columns = x1.max(out.bounds[0])..x2.min(out.bounds[2]);
    let rows = y1.max(out.bounds[1])..y2.min(out.bounds[3]);
    crate::colors::write_out(&out, &result, columns, rows);
    drop(out);
    kOfxStatOK
}

/// Color shows the picked stop's color (a preset or an undo may have
/// changed it).
fn show_picked(fx: Fx, time: f64) {
    let stops = parse(&fx.param(STOPS).text_at(time));
    let k = fx.param(PICKED).int_at(time).max(0) as usize;
    if let Some(s) = stops.get(k.min(stops.len().saturating_sub(1))) {
        let color = fx.param(COLOR);
        if color.rgba_at(time) != s.color {
            color.set_rgba(s.color);
        }
    }
}

fn describe(fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, c"Gradient Map");
    p.set_str(kOfxImageEffectPluginPropGrouping, c"BananaV/Color");
    p.set_str(
        kOfxPropPluginDescription,
        c"The picture's tones mapped onto a gradient of colors: its darks to the gradient's start, its lights to its end.",
    );
    p.set_strs(
        kOfxImageEffectPropSupportedContexts,
        &[kOfxImageEffectContextFilter, kOfxImageEffectContextGeneral],
    );
    p.set_strs(
        kOfxImageEffectPropSupportedPixelDepths,
        &[kOfxBitDepthFloat, kOfxBitDepthShort, kOfxBitDepthByte],
    );
    p.set_int(kOfxImageEffectPropSupportsTiles, 1);
    p.set_int(kOfxImageEffectPropSupportsMultiResolution, 1);
    p.set_int(kOfxImageEffectPluginPropHostFrameThreading, 0);
    p.set_str(
        kOfxImageEffectPluginRenderThreadSafety,
        kOfxImageEffectRenderFullySafe,
    );
}

fn describe_in_context(fx: Fx) {
    let comps = [kOfxImageComponentRGBA];
    fx.define_clip(kOfxImageEffectSimpleSourceClipName, &comps);
    fx.define_clip(kOfxImageEffectOutputClipName, &comps);
    let g = fx.define_param(kOfxParamTypeGroup, GRADIENT, c"Gradient");
    g.set_ptr(kBananaVParamPropInteract, editor as *mut c_void);
    g.set_ints(kOfxParamPropInteractPreferedSize, &EDITOR_SIZE);
    g.set_str(
        kOfxParamPropHint,
        c"Click a marker to pick its stop, drag it to move it; click the bar to add a stop; drag a marker well below the bar to take it away.",
    );
    let c = fx.define_param(kOfxParamTypeRGBA, COLOR, c"Color");
    c.set_dbls(kOfxParamPropDefault, &[0.0, 0.0, 0.0, 1.0]);
    c.set_int(kOfxParamPropAnimates, 0);
    c.set_str(kOfxParamPropParent, GRADIENT);
    c.set_str(kOfxParamPropHint, c"The picked stop's color.");
    let s = fx.define_param(kOfxParamTypeString, STOPS, c"Stops");
    s.set_str(kOfxParamPropDefault, DEFAULT_STOPS);
    s.set_int(kOfxParamPropAnimates, 0);
    s.set_int(kOfxParamPropSecret, 1);
    let k = fx.define_param(kOfxParamTypeInteger, PICKED, c"Picked");
    k.set_int(kOfxParamPropDefault, 0);
    k.set_int(kOfxParamPropAnimates, 0);
    k.set_int(kOfxParamPropSecret, 1);
    let a = fx.define_param(kOfxParamTypeDouble, AMOUNT, c"Amount");
    a.set_dbl(kOfxParamPropDefault, 1.0);
    a.set_dbl(kOfxParamPropMin, 0.0);
    a.set_dbl(kOfxParamPropMax, 1.0);
    a.set_dbl(kOfxParamPropDisplayMin, 0.0);
    a.set_dbl(kOfxParamPropDisplayMax, 1.0);
    a.set_dbl(kOfxParamPropIncrement, 0.001);
    a.set_int(kOfxParamPropDigits, 3);
    a.set_str(
        kOfxParamPropHint,
        c"How much of the gradient's colors: 0 the picture as it is.",
    );
    let m = fx.define_param(kOfxParamTypeChoice, MODE, c"Mode");
    m.set_strs(kOfxParamPropChoiceOption, &[c"Blend", c"Add"]);
    m.set_int(kOfxParamPropDefault, 0);
    m.set_str(
        kOfxParamPropHint,
        c"The gradient's colors blended with the picture, or added to it.",
    );
}

// The gradient's editor.

/// The editor's preferred size, in pixels.
const EDITOR_SIZE: [c_int; 2] = [300, 44];
/// The bar's margins at the sides and its height; the markers' height;
/// how far below them a marker dragged goes away.
const MARGIN: f64 = 8.0;
const BAR_HEIGHT: f64 = 18.0;
const MARKER: f64 = 13.0;
const AWAY: f64 = 24.0;

/// The editor's places, in its pixels (y up): the bar along its top, the
/// markers under it.
struct Bar {
    left: f64,
    right: f64,
    bottom: f64,
    top: f64,
}

impl Bar {
    fn of([w, h]: [f64; 2]) -> Bar {
        let top = h - 4.0;
        Bar {
            left: MARGIN,
            right: (w - MARGIN).max(MARGIN + 1.0),
            bottom: top - BAR_HEIGHT,
            top,
        }
    }

    /// The place along the gradient of column `x`.
    fn place(&self, x: f64) -> f64 {
        ((x - self.left) / (self.right - self.left)).clamp(0.0, 1.0)
    }

    fn column(&self, t: f64) -> f64 {
        self.left + t * (self.right - self.left)
    }

    /// The markers' tip (just under the bar) and base.
    fn tip(&self) -> f64 {
        self.bottom - 1.0
    }

    fn base(&self) -> f64 {
        self.tip() - MARKER
    }

    /// The marker of a stop at `t`: a pentagon pointing up at the bar.
    fn marker(&self, t: f64) -> [[f64; 2]; 5] {
        let (x, tip, base) = (self.column(t), self.tip(), self.base());
        [
            [x, tip],
            [x + 5.0, tip - 5.0],
            [x + 5.0, base],
            [x - 5.0, base],
            [x - 5.0, tip - 5.0],
        ]
    }

    /// The marker under the pen, the nearest within reach.
    fn marker_at(&self, stops: &[Stop], [px, py]: [f64; 2]) -> Option<usize> {
        if py > self.tip() + 2.0 || py < self.base() - 3.0 {
            return None;
        }
        stops
            .iter()
            .enumerate()
            .map(|(k, s)| (k, (self.column(s.at) - px).abs()))
            .filter(|&(_, d)| d <= 6.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(k, _)| k)
    }

    fn on_bar(&self, [px, py]: [f64; 2]) -> bool {
        py >= self.bottom - 1.0
            && py <= self.top + 1.0
            && px >= self.left - 3.0
            && px <= self.right + 3.0
    }
}

/// The editor's drag: the stop it moves and whether it went far enough
/// below to go away.
#[derive(Default)]
struct EditorState {
    drag: Option<usize>,
    away: bool,
}

/// Picks stop `k`: Color shows its color.
fn pick(fx: Fx, stops: &[Stop], k: usize) {
    fx.param(PICKED).set_int(k as c_int);
    if let Some(s) = stops.get(k) {
        fx.param(COLOR).set_rgba(s.color);
    }
}

unsafe extern "C" fn editor(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxStatus {
    if is(action, kOfxActionDescribe) {
        return kOfxStatOK;
    }
    let Some(props) = interact_props(handle) else {
        return kOfxStatReplyDefault;
    };
    if is(action, kOfxActionCreateInstance) {
        let state = Box::into_raw(Box::new(EditorState::default()));
        props.set_ptr(kOfxPropInstanceData, state as *mut c_void);
        return kOfxStatOK;
    }
    let state = props.ptr(kOfxPropInstanceData) as *mut EditorState;
    if is(action, kOfxActionDestroyInstance) {
        if !state.is_null() {
            // SAFETY: made above, freed once.
            drop(unsafe { Box::from_raw(state) });
            props.set_ptr(kOfxPropInstanceData, std::ptr::null_mut());
        }
        return kOfxStatOK;
    }
    if state.is_null() {
        return kOfxStatReplyDefault;
    }
    // SAFETY: this interface's state; hosts run one of its actions at a
    // time.
    let state = unsafe { &mut *state };
    let args = Props(in_args);
    let fx = Fx(args.ptr(kOfxPropEffectInstance) as OfxImageEffectHandle);
    if fx.0.is_null() {
        return kOfxStatReplyDefault;
    }
    let time = args.dbl(kOfxPropTime, 0);
    let size: [c_int; 2] = props.ints(kOfxInteractPropViewportSize);
    let bar = Bar::of([size[0] as f64, size[1] as f64]);
    let stops_param = fx.param(STOPS);
    let mut stops = parse(&stops_param.text_at(time));
    let picked = fx.param(PICKED).int_at(time).max(0) as usize;
    if is(action, kOfxInteractActionDraw) {
        if let Some(d) = Draw::of(args) {
            draw_editor(&d, &bar, &stops, picked, state);
        }
        return kOfxStatOK;
    }
    let pen: [f64; 2] = args.dbls(kOfxInteractPropPenPosition);
    if is(action, kOfxInteractActionPenDown) {
        let k = if let Some(k) = bar.marker_at(&stops, pen) {
            k
        } else if bar.on_bar(pen) {
            // A new stop of the gradient's color there.
            let t = bar.place(pen[0]);
            let c = Gradient::new(&stops).at(t);
            let a = f64::from(c[3]);
            let straight = |v: f32| {
                if a > 0.0 {
                    (f64::from(v) / a).clamp(0.0, 1.0)
                } else {
                    0.0
                }
            };
            stops.push(Stop {
                at: t,
                color: [straight(c[0]), straight(c[1]), straight(c[2]), a],
            });
            stops_param.set_text(&write(&stops));
            stops.len() - 1
        } else {
            return kOfxStatReplyDefault;
        };
        pick(fx, &stops, k);
        *state = EditorState {
            drag: Some(k),
            away: false,
        };
        redraw(handle);
        kOfxStatOK
    } else if is(action, kOfxInteractActionPenMotion) {
        let Some(k) = state.drag else {
            return kOfxStatReplyDefault;
        };
        if let Some(s) = stops.get_mut(k) {
            s.at = bar.place(pen[0]);
            stops_param.set_text(&write(&stops));
        }
        state.away = stops.len() > 2 && pen[1] < bar.base() - AWAY;
        redraw(handle);
        kOfxStatOK
    } else if is(action, kOfxInteractActionPenUp) {
        let Some(k) = state.drag.take() else {
            return kOfxStatReplyDefault;
        };
        if std::mem::take(&mut state.away) && k < stops.len() && stops.len() > 2 {
            stops.remove(k);
            stops_param.set_text(&write(&stops));
            pick(fx, &stops, k.min(stops.len() - 1));
        }
        redraw(handle);
        kOfxStatOK
    } else {
        kOfxStatReplyDefault
    }
}

/// The checkerboard's squares under the bar, light and dark.
const LIGHT: [f32; 4] = [0.78, 0.78, 0.78, 1.0];
const DARK: [f32; 4] = [0.55, 0.55, 0.55, 1.0];

/// Draws the bar (the gradient over a checkerboard) and the stops'
/// markers, the picked one on top and outlined white.
fn draw_editor(d: &Draw, bar: &Bar, stops: &[Stop], picked: usize, state: &EditorState) {
    let rect = |x0: f64, y0: f64, x1: f64, y1: f64| [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    let (x0, x1, y0, y1) = (bar.left, bar.right, bar.bottom, bar.top);
    let mid = (y0 + y1) / 2.0;
    let mut x = x0;
    let mut i = 0;
    while x < x1 {
        let xe = (x + 6.0).min(x1);
        for (j, (ya, yb)) in [(y0, mid), (mid, y1)].into_iter().enumerate() {
            d.colour(if (i + j) % 2 == 0 { LIGHT } else { DARK });
            d.fill(&rect(x, ya, xe, yb));
        }
        x = xe;
        i += 1;
    }
    let g = Gradient::new(stops);
    let mut x = x0;
    while x < x1 {
        let xe = (x + 2.0).min(x1);
        let c = g.at(bar.place((x + xe) / 2.0));
        let a = c[3];
        let s = |v: f32| if a > 0.0 { (v / a).min(1.0) } else { 0.0 };
        d.colour([s(c[0]), s(c[1]), s(c[2]), a]);
        d.fill(&rect(x, y0, xe, y1));
        x = xe;
    }
    d.width(1.0);
    d.colour([0.08, 0.08, 0.08, 1.0]);
    d.outline(&rect(x0, y0, x1, y1));
    let mut order: Vec<usize> = (0..stops.len()).filter(|&k| k != picked).collect();
    if picked < stops.len() {
        order.push(picked);
    }
    for k in order {
        let s = stops[k];
        let shape = bar.marker(s.at);
        let fade = if state.drag == Some(k) && state.away {
            0.35
        } else {
            1.0
        };
        let [r, g, b, _] = s.color.map(|v| v as f32);
        d.colour([r, g, b, fade]);
        d.fill(&shape);
        if k == picked {
            d.width(2.0);
            d.colour([1.0, 1.0, 1.0, fade]);
        } else {
            d.width(1.0);
            d.colour([0.05, 0.05, 0.05, fade]);
        }
        d.outline(&shape);
    }
    d.width(1.0);
}

/// The factory presets: BananaV's (VEGAS's has none).
pub(crate) fn presets() -> Vec<Preset> {
    let stops = |three: [[f64; 3]; 3]| {
        let stops: Vec<Stop> = three
            .iter()
            .zip([0.0, 0.5, 1.0])
            .map(|(&[r, g, b], at)| Stop {
                at,
                color: [r, g, b, 1.0],
            })
            .collect();
        PresetValue::Text(write(&stops))
    };
    [
        (
            "Night Vision",
            [[0.0, 0.05, 0.0], [0.1, 0.55, 0.15], [0.75, 1.0, 0.7]],
        ),
        (
            "Sepia",
            [[0.08, 0.05, 0.02], [0.55, 0.4, 0.25], [1.0, 0.95, 0.85]],
        ),
        (
            "Infrared",
            [[0.05, 0.0, 0.3], [0.9, 0.1, 0.2], [1.0, 1.0, 0.4]],
        ),
        (
            "Blue Duotone",
            [[0.02, 0.05, 0.2], [0.25, 0.45, 0.75], [0.9, 0.95, 1.0]],
        ),
        (
            "Black and White",
            [[0.0, 0.0, 0.0], [0.5, 0.5, 0.5], [1.0, 1.0, 1.0]],
        ),
    ]
    .into_iter()
    .map(|(name, three)| Preset {
        name,
        values: vec![("stops", stops(three))],
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stops_are_kept_as_text_and_read_back() {
        let stops = parse(DEFAULT_STOPS.to_str().unwrap());
        assert_eq!(stops.len(), 3);
        assert_eq!(stops[1].at, 0.5);
        assert_eq!(parse(&write(&stops)), stops);
        // What cannot be read is left out.
        assert_eq!(parse("0.2 1 0 0;x;0.4 1 1 1 1").len(), 1);
    }

    fn near(a: [f32; 4], b: [f32; 4]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5)
    }

    #[test]
    fn tones_follow_the_stops_through_the_spline() {
        // The default: black stays black and white white; between them
        // the spline eases out of each stop (darker than a straight
        // line just past black), always rising.
        let g = Gradient::new(&parse(DEFAULT_STOPS.to_str().unwrap()));
        let gray = |v: f32| mapped(&g, 1.0, false, [v, v, v, 1.0]);
        assert!(near(gray(0.0), [0.0, 0.0, 0.0, 1.0]), "{:?}", gray(0.0));
        assert!(near(gray(1.0), [1.0; 4]), "{:?}", gray(1.0));
        assert!(gray(0.25)[0] < 0.25, "{:?}", gray(0.25));
        let mut last = -1.0;
        for k in 0..=40 {
            let out = gray(k as f32 / 40.0);
            assert!(
                out[0] >= last && out[0] == out[1] && out[1] == out[2],
                "{k} {out:?}"
            );
            last = out[0];
        }
        // Black to red, white to blue: black turns red, white blue.
        let two = Gradient::new(&[
            Stop {
                at: 0.0,
                color: [1.0, 0.0, 0.0, 1.0],
            },
            Stop {
                at: 1.0,
                color: [0.0, 0.0, 1.0, 1.0],
            },
        ]);
        assert!(near(
            mapped(&two, 1.0, false, [0.0, 0.0, 0.0, 1.0]),
            [1.0, 0.0, 0.0, 1.0]
        ));
        assert!(near(
            mapped(&two, 1.0, false, [1.0; 4]),
            [0.0, 0.0, 1.0, 1.0]
        ));
        // Add: the color on top of the picture's.
        let added = mapped(&two, 0.5, true, [0.2, 0.2, 0.2, 1.0]);
        assert!(added[0] > 0.2 && added[2] > 0.2, "{added:?}");
    }

    #[test]
    fn a_stops_alpha_weighs_its_color_twice() {
        // A half-transparent white stop alone: the color is premultiplied
        // and then mixed in by its alpha, as VEGAS's.
        let g = Gradient::new(&[Stop {
            at: 0.5,
            color: [1.0, 1.0, 1.0, 0.5],
        }]);
        let out = mapped(&g, 1.0, false, [0.0, 0.0, 0.0, 1.0]);
        assert!((out[0] - 0.25).abs() < 1e-6, "{out:?}");
        // A transparent pixel takes the gradient's start.
        let none = mapped(&g, 1.0, false, [0.0; 4]);
        assert!((none[3] - 0.5).abs() < 1e-6, "{none:?}");
    }
}
