//! The animations' numbers, kept exactly: each one's template (the
//! block's pivot, its fixed tilt and effects, what it gives lines, words
//! and letters by their place in the text, its staggers) and its rows
//! of keys over the media's length — the moves and timings of VEGAS's
//! titles' animations; numbers to keep, not to tune.

// Turns near a quarter or a whole turn are the animations' own numbers,
// not those constants.
#![allow(clippy::approx_constant)]

use super::Ease::{Auto, Fast, Linear, Slow, Smooth, Tangents};
use super::*;

/// The animations, in the Animation list's order ("None" aside).
pub(super) const ALL: [&Animation; 50] = [
    &TITLE01,
    &TITLE02,
    &TITLE03,
    &TITLE04,
    &TITLE05,
    &TITLE06,
    &TITLE07,
    &TITLE08,
    &TITLE09,
    &TITLE10,
    &TITLE11,
    &TITLE12,
    &TITLE13,
    &TITLE14,
    &TITLE15,
    &TITLE16,
    &TITLE17,
    &TITLE18,
    &TITLE19,
    &TITLE20,
    &TITLE21,
    &TITLE22,
    &TITLE23,
    &TITLE24,
    &TITLE25,
    &ACTION_FLIP,
    &BOUNCE,
    &COMING_AT_YOU,
    &DOUBLE_FLASH_GLOW,
    &DROP_SPLIT,
    &DROPPING_WORDS,
    &EARTHQUAKE,
    &FALL_DOWN,
    &FLOAT_AND_POP,
    &FLY_IN_FROM_RIGHT,
    &FLY_IN,
    &JUMP,
    &MENACE,
    &POPUP,
    &ROLLING_GLOW_AND_ENLARGE,
    &ROUGH_DAY,
    &SCROLL,
    &SCROLL_LEFT,
    &SLIDE_DOWN,
    &SLIDE_LEFT,
    &SLIDE_RIGHT,
    &SLIDE_UP,
    &SLIDE,
    &SPEEDY,
    &TWIST_IN,
];

const fn key(at: f64, value: f64, ease: Ease) -> Key {
    Key { at, value, ease }
}

/// A damped spring's wobble from `start` for `length` of the media.
const fn spring(
    start: f64,
    length: f64,
    scale: f64,
    amplitude: f64,
    frequency: f64,
    damping: f64,
) -> Wave {
    Wave {
        start,
        length,
        scale,
        shape: Shape::Spring {
            amplitude,
            frequency,
            damping,
        },
    }
}

/// Hops that shrink by `decay` each time.
#[allow(clippy::too_many_arguments)]
const fn hops(
    start: f64,
    length: f64,
    scale: f64,
    loops: f64,
    decay: f64,
    inner: f64,
    settle: bool,
) -> Wave {
    Wave {
        start,
        length,
        scale,
        shape: Shape::Hops {
            loops,
            decay,
            inner,
            settle,
        },
    }
}

/// A jitter of `octaves` layers over the gradients `gradients`.
const fn jitter(
    start: f64,
    length: f64,
    scale: f64,
    frequency: f64,
    octaves: u32,
    gradients: &'static [f64; 128],
) -> Wave {
    Wave {
        start,
        length,
        scale,
        shape: Shape::Jitter {
            frequency,
            octaves,
            gradients,
        },
    }
}

/// The jitter's gradients for its seed 42: the first 128 of its
/// 1024 (the animations read no further).
#[rustfmt::skip]
const JITTER_42: [f64; 128] = [
    -0.159267681, -0.648040781, -0.56867914, -0.940832834, 0.446942579, 0.305738127,
    0.268553466, -0.523852693, -0.191493412, 0.4909809, 0.857448702, 0.632363899,
    0.32910743, -0.011625126, 0.763361322, -0.632994395, -0.96124584, 0.536129284,
    0.344345412, -0.061509855, 0.251025598, 0.402461583, 0.819372912, -0.287535557,
    0.802486855, -0.692157079, 0.617893634, 0.651563498, -0.383600674, -0.178920075,
    0.40514237, -0.369631326, 0.470210304, 0.42230918, 0.915357357, 0.76906218,
    0.503126313, 0.972732055, 0.781591551, 0.654251273, -0.750624379, 0.583816409,
    0.303110793, 0.593857681, 0.209118568, 0.41030426, 0.254421482, -0.199941033,
    -0.318398004, 0.163479607, 0.30166466, 0.421834217, -0.072622333, 0.704430853,
    0.97089188, 0.419777569, 0.444934639, 0.496717854, 0.690576441, -0.202084692,
    0.512446614, 0.836863907, -0.198602015, -0.672708176, 0.393420793, 0.988899123,
    0.068634389, 0.894157066, -0.357382288, 0.231862963, 0.725703671, 0.83325752,
    0.4946315, -0.127029649, 0.279645809, 0.060516233, -0.181007366, -0.33433147,
    -0.740724615, 0.622430553, 0.354442518, 0.337712593, 0.766365692, -0.288547127,
    -0.779732499, 0.133949063, -0.591784186, 0.756177113, -0.092422546, -0.808668435,
    -0.836321799, -0.841400745, 0.90673047, -0.40329383, -0.930921934, -0.70466189,
    0.419563705, -0.031311808, 0.196355612, -0.999844191, 0.802516916, -0.416143805,
    0.024750932, -0.252224683, -0.577519961, 0.063250501, 0.628906712, -0.294369204,
    -0.537012367, -0.707147529, -0.933508524, -0.023095956, 0.595442883, -0.876753826,
    -0.254465541, -0.872920052, -0.258941584, -0.916980067, 0.935924478, -0.693682695,
    -0.197642525, -0.76874965, -0.45233548, -0.860973963, -0.138484846, -0.530646791,
    -0.630275795, -0.380334803,
];

/// The jitter's gradients for its seed 420: the first 128 of its
/// 1024 (the animations read no further).
#[rustfmt::skip]
const JITTER_420: [f64; 128] = [
    -0.673247, 0.541099316, 0.552659569, 0.357712573, -0.071833789, -0.153471496,
    -0.701824876, 0.030277703, 0.245067924, -0.928169318, 0.700188923, -0.275792418,
    -0.526027364, -0.803474828, 0.720188681, -0.160085588, 0.984281252, -0.892789927,
    0.552874814, -0.393324748, 0.890484686, 0.089540325, 0.21285176, -0.492192176,
    -0.926214426, 0.350018744, 0.399587709, 0.447990139, -0.308322034, -0.113564631,
    -0.963807816, -0.710249541, 0.435615991, 0.770284704, -0.446535301, 0.508808597,
    -0.18957613, 0.143944193, -0.462112788, -0.116571691, -0.427793451, -0.143430441,
    -0.528134078, -0.15014428, 0.835241747, 0.868589151, 0.066939357, -0.181168736,
    0.505967732, -0.687667914, -0.528681389, -0.21509807, -0.238190651, 0.761400306,
    0.742485009, 0.949547178, -0.017715623, -0.084830392, -0.925891823, -0.489369637,
    -0.173223729, 0.520898912, 0.434394257, -0.897430366, 0.720580704, -0.909527576,
    -0.96635745, -0.535072653, -0.44801036, 0.73744358, -0.341673404, -0.010090648,
    0.754474054, 0.326297075, 0.503291604, -0.15885769, -0.228291884, -0.298701429,
    -0.133944505, -0.045568982, 0.734663416, -0.470987996, 0.573175527, 0.467735141,
    -0.352551611, -0.893666604, 0.080807271, -0.34286574, 0.515789031, 0.633162697,
    0.394305265, -0.019992744, -0.841723555, 0.424866834, -0.29349718, -0.52889067,
    -0.815324852, 0.042939948, -0.56988839, 0.613639729, -0.356916443, 0.470948808,
    -0.493896305, 0.057975584, -0.934557211, 0.681094101, -0.784607335, -0.84067133,
    -0.525674505, 0.016179681, 0.845951209, 0.744229073, -0.962140211, -0.701669065,
    0.24390752, -0.316299212, 0.518867542, -0.219709653, 0.697003672, -0.539758419,
    0.94801305, -0.39834977, -0.555890354, -0.915382205, -0.860155245, 0.758251438,
    0.204589793, 0.662509823,
];

/// Title01.
#[rustfmt::skip]
const TITLE01: Animation = Animation {
    name: "_Title01",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Baseline),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Line, index: 0, attrs: Attrs {
            offset: [-25.0, 0.0], opacity: 0.0,
            effects: Some(Effects { blur_on: true, blur: [0.25, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 7, attrs: Attrs {
            offset: [25.0, 0.0], opacity: 0.0,
            effects: Some(Effects { blur_on: true, blur: [0.25, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[],
    rows: &[
        Row { at: Element::Line(0), property: Property::OffsetX, stagger: None,
            keys: &[key(0.0, -25.0, Auto), key(0.036684584699664326, 3.05, Auto)],
            waves: &[spring(0.029798508750297235, 0.06663342492510448, -4.0, 0.7, 1.0, 0.4)],
        },
        Row { at: Element::Line(0), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.006663576178679195, 0.0, Slow),
                key(0.0266387314581823, 1.0, Fast),
            ],
            waves: &[],
        },
        Row { at: Element::Line(0), property: Property::BlurAcross, stagger: None,
            keys: &[
                key(0.0, 0.25, Linear), key(0.006663576178679195, 0.25, Fast),
                key(0.036684584699664326, 0.0, Slow),
            ],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::OffsetX, stagger: None,
            keys: &[key(0.0, 25.0, Auto), key(0.036684584699664326, 0.0, Auto)],
            waves: &[spring(0.029798508750297235, 0.06663342492510448, 4.0, 0.7, 1.0, 0.4)],
        },
        Row { at: Element::Line(1), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.006663576178679195, 0.0, Slow),
                key(0.0266387314581823, 1.0, Fast),
            ],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::BlurAcross, stagger: None,
            keys: &[
                key(0.0, 0.25, Linear), key(0.006663576178679195, 0.25, Fast),
                key(0.036684584699664326, 0.0, Slow),
            ],
            waves: &[],
        },
    ],
};

/// Title02.
#[rustfmt::skip]
const TITLE02: Animation = Animation {
    name: "_Title02",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Baseline),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Line, index: 0, attrs: Attrs {
            offset: [-24.0, 0.0], opacity: 0.0,
            effects: Some(Effects { blur_on: true, blur: [0.15, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 10, attrs: Attrs {
            offset: [-24.0, 0.0], opacity: 0.0,
            effects: Some(Effects { blur_on: true, blur: [0.15, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 19, attrs: Attrs {
            offset: [-24.0, 0.0], opacity: 0.0,
            effects: Some(Effects { blur_on: true, blur: [0.15, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[],
    rows: &[
        Row { at: Element::Line(0), property: Property::OffsetX, stagger: None,
            keys: &[key(0.0, -24.0, Fast), key(0.05332170826742793, 0.0, Slow)],
            waves: &[spring(0.039996088063342226, 0.06667123810413689, -2.0, 0.65, 1.0, 0.25)],
        },
        Row { at: Element::Line(0), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.010023639876785236, 0.0, Fast),
                key(0.039996088063342226, 1.0, Slow),
            ],
            waves: &[],
        },
        Row { at: Element::Line(0), property: Property::BlurAcross, stagger: None,
            keys: &[
                key(0.0, 0.15, Auto), key(0.010023639876785236, 0.15, Fast),
                key(0.05329914155167103, 0.0, Slow),
            ],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::OffsetX, stagger: None,
            keys: &[
                key(0.0, -24.0, Linear), key(0.009987577639751553, -24.0, Fast),
                key(0.063316712068299, 0.0, Slow),
            ],
            waves: &[spring(0.04998454888347723, 0.06668963438869756, -2.0, 0.65, 1.0, 0.25)],
        },
        Row { at: Element::Line(1), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.02002704599652071, 0.0, Fast),
                key(0.04998454888347723, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::BlurAcross, stagger: None,
            keys: &[
                key(0.0, 0.15, Linear), key(0.02002704599652071, 0.15, Fast),
                key(0.06330601389276243, 0.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(2), property: Property::OffsetX, stagger: None,
            keys: &[
                key(0.0, -24.0, Linear), key(0.02002263639199591, -24.0, Fast),
                key(0.07666859912491515, 0.0, Slow),
            ],
            waves: &[spring(0.059985970662479626, 0.06665805824821441, -2.0, 0.65, 1.0, 0.25)],
        },
        Row { at: Element::Line(2), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.029998772019756655, 0.0, Fast),
                key(0.059985970662479626, 1.0, Slow),
            ],
            waves: &[],
        },
        Row { at: Element::Line(2), property: Property::BlurAcross, stagger: None,
            keys: &[
                key(0.0, 0.15, Linear), key(0.029998772019756655, 0.15, Fast),
                key(0.07664237582288355, 0.0, Slow),
            ],
            waves: &[],
        },
    ],
};

/// Title03.
#[rustfmt::skip]
const TITLE03: Animation = Animation {
    name: "_Title03",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Baseline),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Line, index: 0, attrs: Attrs {
            rotation: -1.5707963267949, pivot: Pivot::Part(4.09980264270979e-5, 0.500368889185919),
            opacity: 0.0,
            effects: Some(Effects { blur_on: true, blur: [0.0, 0.2], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 14, attrs: Attrs {
            rotation: -1.5707963267949, pivot: Pivot::Part(1.00013132667447, 0.499921503497069),
            opacity: 0.0,
            effects: Some(Effects { blur_on: true, blur: [0.0, 0.2], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[],
    rows: &[
        Row { at: Element::Line(1), property: Property::Rotation, stagger: None,
            keys: &[key(0.0, -1.5707963267948966, Auto), key(0.049990162938594374, 0.0, Auto)],
            waves: &[spring(0.049990162938594374, 0.05000678426866097, 0.2, 0.7, 1.0, 0.4)],
        },
        Row { at: Element::Line(1), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.01998785902929929, 0.0, Auto),
                key(0.049990162938594374, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::BlurDown, stagger: None,
            keys: &[
                key(0.0, 0.2, Linear), key(0.01998785902929929, 0.2, Slow),
                key(0.049990162938594374, 0.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(0), property: Property::Rotation, stagger: None,
            keys: &[key(0.0, -1.5707963267948966, Auto), key(0.049990162938594374, 0.0, Auto)],
            waves: &[spring(0.049990162938594374, 0.05000678426866097, 0.2, 0.7, 1.0, 0.4)],
        },
        Row { at: Element::Line(0), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.01998785902929929, 0.0, Auto),
                key(0.049990162938594374, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(0), property: Property::BlurDown, stagger: None,
            keys: &[
                key(0.0, 0.2, Linear), key(0.01998785902929929, 0.2, Slow),
                key(0.049990162938594374, 0.0, Auto),
            ],
            waves: &[],
        },
    ],
};

/// Title04.
#[rustfmt::skip]
const TITLE04: Animation = Animation {
    name: "_Title04",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: false,
        selection: [0.0, 0.001],
        path: None,
    },
    nodes: &[],
    staggers: &[],
    rows: &[
        Row { at: Element::Block, property: Property::SelectionLeft, stagger: None,
            keys: &[key(0.0, 0.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::SelectionRight, stagger: None,
            keys: &[key(0.0, 0.001, Linear), key(0.866671465905763, 1.0, Linear)],
            waves: &[],
        },
    ],
};

/// Title05.
#[rustfmt::skip]
const TITLE05: Animation = Animation {
    name: "_Title05",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Line, index: 0, attrs: Attrs {
            offset: [0.0, -2.0], opacity: 0.0,
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 4, attrs: Attrs {
            offset: [0.0, -2.0], opacity: 0.0,
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 10, attrs: Attrs {
            offset: [0.0, -2.0], opacity: 0.0,
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 18, attrs: Attrs {
            offset: [0.0, -2.0], opacity: 0.0,
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[],
    rows: &[
        Row { at: Element::Line(0), property: Property::OffsetY, stagger: None,
            keys: &[key(0.0, -2.0, Fast), key(0.019999999999999997, 0.0, Slow)],
            waves: &[spring(0.013315901992502172, 0.06665511066369738, -1.0, 1.0, 1.0, 0.45)],
        },
        Row { at: Element::Line(0), property: Property::Opacity, stagger: None,
            keys: &[key(0.0, 0.0, Smooth), key(0.019999999999999997, 1.0, Fast)],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, -2.0, Linear), key(0.009976133651551313, -2.0, Fast),
                key(0.029976133651551315, 0.0, Slow),
            ],
            waves: &[spring(0.023343742906050034, 0.06334170220677797, -1.0, 1.0, 1.0, 0.45)],
        },
        Row { at: Element::Line(1), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.009976133651551313, 0.0, Smooth),
                key(0.029976133651551315, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(2), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.019999999999999997, 0.0, Smooth),
                key(0.039999999999999994, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(2), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, -2.0, Linear), key(0.019999999999999997, -2.0, Auto),
                key(0.039999999999999994, 0.0, Auto),
            ],
            waves: &[spring(0.03334580896107598, 0.05996614027484278, -1.0, 1.0, 1.0, 0.45)],
        },
        Row { at: Element::Line(3), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, -2.0, Linear), key(0.029976133651551315, -2.0, Auto),
                key(0.04998689384010484, 0.0, Auto),
            ],
            waves: &[spring(0.04330308921285548, 0.05669266465031262, -1.0, 1.0, 1.0, 0.45)],
        },
        Row { at: Element::Line(3), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.029976133651551315, 0.0, Smooth),
                key(0.04998689384010484, 1.0, Auto),
            ],
            waves: &[],
        },
    ],
};

/// Title06.
#[rustfmt::skip]
const TITLE06: Animation = Animation {
    name: "_Title06",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Line, index: 0, attrs: Attrs {
            offset: [0.0, 15.0],
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 5, attrs: Attrs { offset: [0.0, 15.0], ..Attrs::PLAIN } },
        Node { level: Level::Line, index: 10, attrs: Attrs {
            offset: [0.0, 15.0],
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[],
    rows: &[
        Row { at: Element::Line(2), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, 15.0, Linear), key(0.03998646593483694, 0.0, Slow),
                key(0.07335728768640856, 0.0, Fast), key(0.09335450716997107, -2.0, Fast),
                key(0.14000985941697053, 0.0, Slow), key(0.17333839687477995, 0.0, Fast),
                key(0.1933374987564793, -2.0, Fast), key(0.23998663620359273, 0.0, Slow),
            ],
            waves: &[
                spring(0.033296652138233185, 0.04006063554817538, 1.65, 0.7, 1.0, 0.4),
                spring(0.09335450716997107, 0.09998299158650822, -1.35, 1.0, 1.0, 0.4272108843537416),
                spring(0.23998663620359273, 0.09997702005540071, 1.0, 0.75, 0.65, 0.5),
            ],
        },
        Row { at: Element::Line(1), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, 15.0, Linear), key(0.03998646593483694, 15.0, Linear),
                key(0.07335728768640856, 0.0, Fast), key(0.09335450716997107, -2.0, Fast),
                key(0.14042118068243575, 0.0, Slow), key(0.17333839687477995, 0.0, Fast),
                key(0.1933374987564793, -2.0, Fast), key(0.23998663620359273, 0.0, Slow),
            ],
            waves: &[
                spring(0.09335450716997107, 0.09998299158650822, -1.35, 1.0, 1.0, 0.4272108843537416),
                spring(0.23998663620359273, 0.09997702005540071, 1.0, 0.75, 0.65, 0.5),
            ],
        },
        Row { at: Element::Line(0), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, 15.0, Linear), key(0.14000985941697053, 15.0, Linear),
                key(0.17333839687477995, 0.0, Fast), key(0.1933374987564793, -2.0, Fast),
                key(0.23998663620359273, 0.0, Slow),
            ],
            waves: &[spring(0.23998663620359273, 0.09997702005540071, 1.0, 0.75, 0.65, 0.5)],
        },
    ],
};

/// Title07.
#[rustfmt::skip]
const TITLE07: Animation = Animation {
    name: "_Title07",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Baseline),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Char, index: 0, attrs: Attrs {
            effects: Some(Effects { blur_on: true, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 13, attrs: Attrs {
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            tracking: 0.05,
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[
        Stagger { at: Element::Char(0, 0, 0), list: List::Chars,
            duration: 0.06, shape: Some(&[
                key(0.0, 0.0, Auto), key(1.0, 1.0, Fast),
            ]),
        },
    ],
    rows: &[
        Row { at: Element::Char(0, 0, 0), property: Property::OffsetX, stagger: Some(0),
            keys: &[key(0.0, 3.0, Auto), key(0.01, 0.0, Auto)],
            waves: &[spring(0.01, 0.06665927325480374, 2.0, 0.85, 1.0, 0.35)],
        },
        Row { at: Element::Char(0, 0, 0), property: Property::BlurAcross, stagger: Some(0),
            keys: &[key(0.0, 0.35, Fast), key(0.019998566290866045, 0.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Char(0, 0, 0), property: Property::Opacity, stagger: Some(0),
            keys: &[key(0.0, 0.0, Smooth), key(0.019998566290866045, 1.0, Auto)],
            waves: &[],
        },
    ],
};

/// Title08.
#[rustfmt::skip]
const TITLE08: Animation = Animation {
    name: "_Title08",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Char, index: 0, attrs: Attrs {
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            kerning: -0.2,
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 11, attrs: Attrs {
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Word, index: 11, attrs: Attrs { ..Attrs::PLAIN } },
        Node { level: Level::Char, index: 11, attrs: Attrs {
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[
        Stagger { at: Element::Char(2, 0, 0), list: List::Chars,
            duration: 0.02, shape: Some(&[
                key(0.0, 0.0, Auto), key(1.0, 1.0, Auto),
            ]),
        },
    ],
    rows: &[
        Row { at: Element::Line(1), property: Property::OffsetX, stagger: None,
            keys: &[key(0.0, -24.0, Fast), key(0.04002140351085158, 0.0, Fast)],
            waves: &[
                spring(0.03313137964837559, 0.06668418676723255, -1.5, 0.7006802721088434, 1.0, 0.5),
            ],
        },
        Row { at: Element::Line(1), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.009728436389773549, 0.0, Smooth),
                key(0.04002140351085158, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Char(2, 0, 0), property: Property::OffsetY, stagger: Some(0),
            keys: &[key(0.0, -4.0, Fast), key(0.04002140351085158, 0.0, Auto)],
            waves: &[
                spring(0.03313137964837559, 0.06668418676723255, -1.5, 0.7006802721088434, 1.0, 0.5),
            ],
        },
        Row { at: Element::Char(2, 0, 0), property: Property::Opacity, stagger: Some(0),
            keys: &[
                key(0.0, 0.0, Linear), key(0.009728436389773549, 0.0, Smooth),
                key(0.04002140351085158, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(0), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, 0.0, Auto), key(0.10000692391360792, 3.0, Auto),
                key(0.12000810883115406, 0.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(0), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.10000692391360792, 0.0, Smooth),
                key(0.12000810883115406, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.12000810883115406, 0.0, Tangents(3.2, -3.2)),
                key(0.15277772145132293, 0.0, Auto),
            ],
            waves: &[],
        },
    ],
};

/// Title09.
#[rustfmt::skip]
const TITLE09: Animation = Animation {
    name: "_Title09",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Line, index: 0, attrs: Attrs {
            offset: [-24.0, -1.0], opacity: 0.0,
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 5, attrs: Attrs { scale: [0.0, 0.0], ..Attrs::PLAIN } },
        Node { level: Level::Line, index: 15, attrs: Attrs {
            offset: [24.0, 1.0], opacity: 0.0,
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[],
    rows: &[
        Row { at: Element::Line(0), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, -1.0, Linear), key(0.06667316839553576, -1.05, Auto),
                key(0.08670073193002663, 0.05, Auto),
            ],
            waves: &[spring(0.08260986670251207, 0.10000524643237658, -1.0, 0.85, 3.0, 0.65)],
        },
        Row { at: Element::Line(0), property: Property::OffsetX, stagger: None,
            keys: &[key(0.0, -24.0, Fast), key(0.06667316839553576, 0.0, Slow)],
            waves: &[],
        },
        Row { at: Element::Line(0), property: Property::Opacity, stagger: None,
            keys: &[key(0.0, 0.0, Fast), key(0.0333342616276676, 1.0, Slow)],
            waves: &[],
        },
        Row { at: Element::Line(2), property: Property::OffsetX, stagger: None,
            keys: &[key(0.0, 24.0, Fast), key(0.06667316839553576, 0.0, Slow)],
            waves: &[],
        },
        Row { at: Element::Line(2), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, 1.0, Linear), key(0.06667316839553576, 1.05, Auto),
                key(0.08670073193002663, 0.05, Auto),
            ],
            waves: &[spring(0.08260986670251207, 0.10000524643237658, 1.0, 0.85, 3.0, 0.65)],
        },
        Row { at: Element::Line(2), property: Property::Opacity, stagger: None,
            keys: &[key(0.0, 0.0, Fast), key(0.0333342616276676, 1.0, Slow)],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::ScaleX, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.06667316839553576, 0.0, Auto),
                key(0.08670037927988951, 1.0, Auto),
            ],
            waves: &[spring(0.08260986670251207, 0.10000524643237658, -1.0, 0.85, 3.0, 0.65)],
        },
        Row { at: Element::Line(1), property: Property::ScaleY, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.06667316839553576, 0.0, Auto),
                key(0.08670037927988951, 1.0, Auto),
            ],
            waves: &[spring(0.08260986670251207, 0.10000524643237658, -1.0, 0.85, 3.0, 0.65)],
        },
    ],
};

/// Title10.
#[rustfmt::skip]
const TITLE10: Animation = Animation {
    name: "_Title10",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Line, index: 0, attrs: Attrs {
            offset: [0.0, -1.25], opacity: 0.0,
            effects: Some(Effects { blur_on: true, blur: [0.0, 0.15], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 19, attrs: Attrs {
            offset: [0.0, 1.45], opacity: 0.0,
            effects: Some(Effects { blur_on: true, blur: [0.0, 0.05], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[],
    rows: &[
        Row { at: Element::Line(1), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, 1.45, Linear), key(0.006678344007308053, 1.45, Fast),
                key(0.05334833033708219, 0.0, Slow),
            ],
            waves: &[
                spring(0.040630445343550974, 0.06604719315649418, 1.0, 0.7, 1.0, 0.6993197278911565),
            ],
        },
        Row { at: Element::Line(1), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.019986638102546146, 0.0, Smooth),
                key(0.0533483303370822, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::BlurDown, stagger: None,
            keys: &[
                key(0.0, 0.05, Linear), key(0.019986638102546146, 0.05, Fast),
                key(0.05334833033708219, 0.0, Slow),
            ],
            waves: &[],
        },
        Row { at: Element::Line(0), property: Property::OffsetY, stagger: None,
            keys: &[key(0.0, -1.25, Fast), key(0.0533483303370822, 0.0, Slow)],
            waves: &[],
        },
        Row { at: Element::Line(0), property: Property::Opacity, stagger: None,
            keys: &[key(0.0, 0.0, Smooth), key(0.039982999618332335, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Line(0), property: Property::BlurDown, stagger: None,
            keys: &[key(0.0, 0.15, Fast), key(0.039982999618332335, 0.0, Slow)],
            waves: &[],
        },
    ],
};

/// Title11.
#[rustfmt::skip]
const TITLE11: Animation = Animation {
    name: "_Title11",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Line, index: 0, attrs: Attrs {
            offset: [-12.0, 0.0], opacity: 0.0,
            effects: Some(Effects { blur_on: true, blur: [0.05, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 8, attrs: Attrs {
            offset: [12.0, 0.0], opacity: 0.0,
            effects: Some(Effects { blur_on: true, blur: [0.05, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 19, attrs: Attrs {
            offset: [0.0, 1.85], opacity: 0.0,
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[],
    rows: &[
        Row { at: Element::Line(0), property: Property::OffsetX, stagger: None,
            keys: &[key(0.0, -12.0, Fast), key(0.03999060223451469, 0.0, Auto)],
            waves: &[spring(0.03306004437181878, 0.0666754893874339, -1.0, 0.65, 2.0, 0.5)],
        },
        Row { at: Element::Line(0), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.006686205208535595, 0.0, Fast),
                key(0.03999060223451469, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(0), property: Property::BlurAcross, stagger: None,
            keys: &[
                key(0.0, 0.05, Linear), key(0.006686205208535595, 0.05, Fast),
                key(0.03999060223451469, 0.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::OffsetX, stagger: None,
            keys: &[key(0.0, 12.0, Fast), key(0.03999060223451469, 0.0, Auto)],
            waves: &[spring(0.03306004437181878, 0.0666754893874339, 1.0, 0.65, 2.0, 0.5)],
        },
        Row { at: Element::Line(1), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.006686205208535595, 0.0, Fast),
                key(0.03999060223451469, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::BlurAcross, stagger: None,
            keys: &[
                key(0.0, 0.05, Linear), key(0.006686205208535595, 0.05, Fast),
                key(0.03999060223451469, 0.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(2), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, 1.85, Linear), key(0.05336365417365709, 1.85, Fast),
                key(0.08667012315416268, 0.0, Auto),
            ],
            waves: &[spring(0.07772485179194034, 0.06669014465255471, 1.0, 0.65, 1.0, 0.5)],
        },
        Row { at: Element::Line(2), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.06331796567045418, 0.0, Smooth),
                key(0.08667012315416268, 1.0, Auto),
            ],
            waves: &[],
        },
    ],
};

/// Title12.
#[rustfmt::skip]
const TITLE12: Animation = Animation {
    name: "_Title12",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Line, index: 0, attrs: Attrs {
            offset: [-1.37579617834395, -0.2],
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 7, attrs: Attrs {
            effects: Some(Effects { blur_on: true, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 14, attrs: Attrs {
            effects: Some(Effects { blur_on: true, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 24, attrs: Attrs {
            offset: [0.917197452229303, 0.1],
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[],
    rows: &[
        Row { at: Element::Line(0), property: Property::OffsetY, stagger: None,
            keys: &[key(0.0, 5.0, Fast), key(0.040002008015238094, -0.2, Slow)],
            waves: &[],
        },
        Row { at: Element::Line(0), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.0032886944217687076, 0.0, Fast),
                key(0.040002008015238094, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(3), property: Property::OffsetY, stagger: None,
            keys: &[key(0.0, -5.0, Fast), key(0.040002008015238094, 0.1, Slow)],
            waves: &[],
        },
        Row { at: Element::Line(3), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.0032886944217687076, 0.0, Fast),
                key(0.040002008015238094, 1.0, Slow),
            ],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::OffsetX, stagger: None,
            keys: &[key(0.0, 14.0, Fast), key(0.040002008015238094, 0.0, Slow)],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.0032886944217687076, 0.0, Fast),
                key(0.040002008015238094, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::BlurAcross, stagger: None,
            keys: &[
                key(0.0, 0.05, Linear), key(0.0032886944217687076, 0.05, Fast),
                key(0.040002008015238094, 0.0, Slow),
            ],
            waves: &[],
        },
        Row { at: Element::Line(2), property: Property::OffsetX, stagger: None,
            keys: &[key(0.0, -14.0, Fast), key(0.040002008015238094, 0.0, Slow)],
            waves: &[],
        },
        Row { at: Element::Line(2), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.0032886944217687076, 0.0, Fast),
                key(0.040002008015238094, 1.0, Slow),
            ],
            waves: &[],
        },
        Row { at: Element::Line(2), property: Property::BlurAcross, stagger: None,
            keys: &[
                key(0.0, 0.05, Linear), key(0.0032886944217687076, 0.05, Fast),
                key(0.040002008015238094, 0.0, Slow),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::ScaleX, stagger: None,
            keys: &[
                key(0.0, 0.75, Fast), key(0.040002008015238094, 1.0, Auto),
                key(0.1400180412326612, 1.1, Slow),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::ScaleY, stagger: None,
            keys: &[
                key(0.0, 0.75, Fast), key(0.040002008015238094, 1.0, Auto),
                key(0.1400180412326612, 1.1, Slow),
            ],
            waves: &[],
        },
    ],
};

/// Title13.
#[rustfmt::skip]
const TITLE13: Animation = Animation {
    name: "_Title13",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: -0.0270534161586453,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Line, index: 6, attrs: Attrs {
            offset: [0.000596427717697611, 0.0],
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 15, attrs: Attrs {
            offset: [0.0112364681672383, 0.0],
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[],
    rows: &[
        Row { at: Element::Line(0), property: Property::OffsetX, stagger: None,
            keys: &[key(0.0, -16.0, Fast), key(0.03334496615528709, 0.0, Slow)],
            waves: &[
                spring(0.026705253478422068, 0.06665076900894225, -1.0, 0.7, 1.0, 0.4952380952380953),
            ],
        },
        Row { at: Element::Line(0), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.006687047423651698, 0.0, Smooth),
                key(0.03334496615528709, 1.0, Slow),
            ],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::OffsetX, stagger: None,
            keys: &[
                key(0.0, -16.0, Linear), key(0.010013605442176872, -16.0, Fast),
                key(0.043327678789432916, 0.0, Slow),
            ],
            waves: &[
                spring(0.03668822300711999, 0.06671958239680019, -1.0, 0.7, 1.0, 0.4952380952380953),
            ],
        },
        Row { at: Element::Line(1), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.016653061224489795, 0.0, Smooth),
                key(0.043327678789432916, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(2), property: Property::OffsetX, stagger: None,
            keys: &[
                key(0.0, -16.0, Linear), key(0.020027210884353736, -16.0, Fast),
                key(0.05334128423160979, 0.0, Slow),
            ],
            waves: &[
                spring(0.04664740668058938, 0.06664578119727932, -1.0, 0.7, 1.0, 0.4952380952380953),
            ],
        },
        Row { at: Element::Line(2), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.026666666666666655, 0.0, Smooth),
                key(0.05334128423160979, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(3), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, -10.0, Linear), key(0.05334128423160979, -10.0, Linear),
                key(0.073362538216732, 0.0, Slow),
            ],
            waves: &[],
        },
        Row { at: Element::Line(3), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.05334128423160979, 0.0, Smooth),
                key(0.073362538216732, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::Rotation, stagger: None,
            keys: &[
                key(0.0, 0.349065850398866, Linear),
                key(0.073362538216732, 0.3490658503988659, Fast),
                key(0.09002566255254496, 0.0, Linear),
            ],
            waves: &[spring(0.08334175698082158, 0.06666908038675595, 1.0, 0.1, 2.0, 0.85)],
        },
    ],
};

/// Title14.
#[rustfmt::skip]
const TITLE14: Animation = Animation {
    name: "_Title14",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Line, index: 0, attrs: Attrs {
            offset: [0.0, -0.00240044810409676],
            effects: Some(Effects { blur_on: true, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 13, attrs: Attrs {
            offset: [0.0, -3.29920154032203e-5],
            effects: Some(Effects { blur_on: true, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 23, attrs: Attrs {
            effects: Some(Effects { blur_on: true, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[],
    rows: &[
        Row { at: Element::Line(2), property: Property::OffsetY, stagger: None,
            keys: &[key(0.0, 12.0, Fast), key(0.03332788781902193, 0.0, Auto)],
            waves: &[spring(0.026702147201009566, 0.06644027606700802, 1.0, 0.35, 1.0, 0.4)],
        },
        Row { at: Element::Line(2), property: Property::Rotation, stagger: None,
            keys: &[key(0.0, 0.17453292519943295, Smooth), key(0.03332788781902193, 0.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Line(2), property: Property::BlurDown, stagger: None,
            keys: &[key(0.0, 0.05, Fast), key(0.03332788781902193, 0.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Line(2), property: Property::Opacity, stagger: None,
            keys: &[key(0.0, 0.0, Smooth), key(0.009989680082559342, 1.0, Linear)],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, 10.75, Linear), key(0.006652719665271968, 10.75, Fast),
                key(0.04000000000000001, 0.0, Slow),
            ],
            waves: &[spring(0.03332788781902193, 0.06667092355110028, 1.0, 0.35, 1.0, 0.4)],
        },
        Row { at: Element::Line(1), property: Property::Rotation, stagger: None,
            keys: &[
                key(0.0, -0.17453292519943295, Linear),
                key(0.006652719665271968, -0.17453292519943295, Smooth),
                key(0.04000000000000001, 0.0, Slow),
            ],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::BlurDown, stagger: None,
            keys: &[
                key(0.0, 0.05, Linear), key(0.006652719665271968, 0.05, Fast),
                key(0.04000000000000001, 0.0, Slow),
            ],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.006652719665271968, 0.0, Smooth),
                key(0.016683679417593952, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(0), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, 9.15, Linear), key(0.013319386331938632, 9.15, Fast),
                key(0.046652290303390846, 0.0, Slow),
            ],
            waves: &[spring(0.04000000000000001, 0.06641587415309202, 1.0, 0.35, 1.0, 0.4)],
        },
        Row { at: Element::Line(0), property: Property::Rotation, stagger: None,
            keys: &[
                key(0.0, 0.17453292519943295, Linear),
                key(0.013319386331938632, 0.17453292519943295, Smooth),
                key(0.046652290303390846, 0.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(0), property: Property::BlurDown, stagger: None,
            keys: &[
                key(0.0, 0.05, Linear), key(0.013319386331938632, 0.05, Fast),
                key(0.046652290303390846, 0.0, Slow),
            ],
            waves: &[],
        },
        Row { at: Element::Line(0), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.013319386331938632, 0.0, Smooth),
                key(0.023350346084260613, 1.0, Auto),
            ],
            waves: &[],
        },
    ],
};

/// Title15.
#[rustfmt::skip]
const TITLE15: Animation = Animation {
    name: "_Title15",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Line, index: 9, attrs: Attrs {
            offset: [0.0, -0.0998689902938902],
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[],
    rows: &[
        Row { at: Element::Word(0, 0), property: Property::OffsetY, stagger: None,
            keys: &[key(0.0, -2.6511627906976827, Fast), key(0.03331775700934579, 0.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Word(0, 0), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.006658739595719381, 0.0, Fast),
                key(0.03331775700934579, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Word(0, 1), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, -2.65, Linear), key(0.01663551401869159, -2.65, Fast),
                key(0.053271028037383185, 0.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Word(0, 1), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.02329425361441097, 0.0, Fast),
                key(0.053271028037383185, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, 2.65, Linear), key(0.03998366228408992, 2.65, Fast),
                key(0.07322668548898839, 0.0, Auto),
            ],
            waves: &[spring(0.07322668548898839, 0.06664279667063025, 1.0, 0.7, 1.0, 0.65)],
        },
        Row { at: Element::Line(1), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.046612288441663804, 0.0, Smooth),
                key(0.07322668548898839, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::ScaleX, stagger: None,
            keys: &[
                key(0.0, 1.0, Linear), key(0.046612288441663804, 0.85, Fast),
                key(0.07322668548898839, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::ScaleY, stagger: None,
            keys: &[
                key(0.0, 1.0, Auto), key(0.046612288441663804, 0.85, Fast),
                key(0.07322668548898839, 1.0, Auto),
            ],
            waves: &[],
        },
    ],
};

/// Title16.
#[rustfmt::skip]
const TITLE16: Animation = Animation {
    name: "_Title16",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Baseline),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Word, index: 23, attrs: Attrs {
            offset: [0.0, -0.0517356034954721], opacity: 0.9527937101199677,
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[],
    rows: &[
        Row { at: Element::Word(0, 0), property: Property::OffsetY, stagger: None,
            keys: &[key(0.0, -3.0, Fast), key(0.03331809059796665, 0.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Word(0, 0), property: Property::Opacity, stagger: None,
            keys: &[key(0.0, 0.0, Smooth), key(0.03331809059796665, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Word(0, 1), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, -3.0, Linear), key(0.02, -3.0, Fast), key(0.05332282018691589, 0.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Word(0, 1), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.02, 0.0, Smooth), key(0.05332282018691589, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Word(1, 0), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, -3.0, Linear), key(0.03666016260275458, -3.0, Fast),
                key(0.0700004398569774, 0.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Word(1, 0), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.03666016260275458, 0.0, Smooth),
                key(0.0700004398569774, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Word(1, 1), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, -3.0, Linear), key(0.05332282018691589, -3.0, Auto),
                key(0.08666007209676314, 0.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Word(1, 1), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.05332282018691589, 0.0, Smooth),
                key(0.08666007209676314, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Word(1, 2), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, -3.0, Linear), key(0.0699891640767158, -3.0, Fast),
                key(0.10333389757174509, 0.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Word(1, 2), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.0699891640767158, 0.0, Smooth),
                key(0.10333389757174509, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::OffsetX, stagger: None,
            keys: &[
                key(0.0, 3.0, Linear), key(0.06334302305063212, 3.0, Fast),
                key(0.09330735123136932, 0.0, Fast),
            ],
            waves: &[spring(0.08577719989822893, 0.10003120034602117, 1.0, 0.7, 1.0, 0.4)],
        },
    ],
};

/// Title17.
#[rustfmt::skip]
const TITLE17: Animation = Animation {
    name: "_Title17",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Baseline),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[],
    staggers: &[],
    rows: &[
        Row { at: Element::Word(0, 0), property: Property::OffsetY, stagger: None,
            keys: &[key(0.0, 4.0, Fast), key(0.03331700808392094, 0.0, Slow)],
            waves: &[],
        },
        Row { at: Element::Word(0, 0), property: Property::Opacity, stagger: None,
            keys: &[key(0.0, 0.0, Smooth), key(0.03331700808392094, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Word(0, 1), property: Property::OffsetY, stagger: None,
            keys: &[key(0.0, 4.0, Fast), key(0.06665872726130843, 0.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Word(0, 1), property: Property::Opacity, stagger: None,
            keys: &[key(0.0, 0.0, Smooth), key(0.06665872726130843, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, -2.0, Linear), key(0.04667321742454289, -2.0, Linear),
                key(0.07331709399761205, 0.0, Linear),
            ],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.04667321742454289, 0.0, Smooth),
                key(0.07331709399761205, 1.0, Smooth),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, -1.0, Linear), key(0.0533605332499485, -1.0, Linear),
                key(0.07331709399761205, 0.0, Auto),
            ],
            waves: &[spring(0.0695252453131044, 0.09839311803601547, -1.05, 0.55, 2.0, 0.6)],
        },
    ],
};

/// Title18.
#[rustfmt::skip]
const TITLE18: Animation = Animation {
    name: "_Title18",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Baseline),
        rotation: -0.10471975511966,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Line, index: 14, attrs: Attrs {
            offset: [0.0, -0.0696383557336257],
            effects: Some(Effects { blur_on: true, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            tracking: 0.15,
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[],
    rows: &[
        Row { at: Element::Word(0, 0), property: Property::OffsetY, stagger: None,
            keys: &[key(0.0, -2.0, Fast), key(0.03331158845464646, 0.0, Slow)],
            waves: &[],
        },
        Row { at: Element::Word(0, 0), property: Property::Opacity, stagger: None,
            keys: &[key(0.0, 0.0, Smooth), key(0.03331158845464646, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Word(0, 1), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, -2.0, Linear), key(0.0167133211172441, -2.0, Fast),
                key(0.06667411140560778, 0.0, Slow),
            ],
            waves: &[],
        },
        Row { at: Element::Word(0, 1), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.0167133211172441, 0.0, Smooth),
                key(0.06667411140560778, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, 2.0, Linear), key(0.03996181174205638, 2.0, Fast),
                key(0.07330962542429936, 0.0, Slow),
            ],
            waves: &[
                spring(0.06545070172403153, 0.0666857840720706, 1.0, 0.7, 1.0, 0.5972789115646259),
            ],
        },
        Row { at: Element::Line(1), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.046656443522412994, 0.0, Smooth),
                key(0.07330962542429936, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::BlurDown, stagger: None,
            keys: &[
                key(0.0, 0.05, Linear), key(0.046656443522412994, 0.05, Auto),
                key(0.07330962542429936, 0.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, -1.0, Linear), key(0.0167133211172441, -1.0, Smooth),
                key(0.06667411140560778, 0.0, Auto),
            ],
            waves: &[],
        },
    ],
};

/// Title19.
#[rustfmt::skip]
const TITLE19: Animation = Animation {
    name: "_Title19",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Line, index: 19, attrs: Attrs {
            opacity: 0.45267275922205297,
            effects: Some(Effects {
                blur_on: true, blur: [0.0, 0.014129584626689624], glow_on: false, glow: 0.01,
            }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[],
    rows: &[
        Row { at: Element::Line(0), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, -7.25, Fast), key(0.03331775700934579, -3.25, Linear),
                key(0.043314990654205604, -3.25, Fast), key(0.08336171962616822, 0.0, Fast),
            ],
            waves: &[],
        },
        Row { at: Element::Line(0), property: Property::Opacity, stagger: None,
            keys: &[key(0.0, 0.0, Smooth), key(0.03331775700934579, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, 1.0, Linear), key(0.043314990654205604, 1.0, Fast),
                key(0.08336171962616822, 0.0, Fast),
            ],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.05333803512557368, 0.0, Smooth),
                key(0.08336171962616822, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(2), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.08336171962616822, 0.0, Smooth),
                key(0.11665185553563623, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(2), property: Property::BlurDown, stagger: None,
            keys: &[
                key(0.0, 0.05, Linear), key(0.08336171962616822, 0.05, Fast),
                key(0.11665185553563623, 0.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::ScaleX, stagger: None,
            keys: &[
                key(0.0, 0.85, Linear), key(0.08336171962616822, 0.85, Fast),
                key(0.12336835451999316, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::ScaleY, stagger: None,
            keys: &[
                key(0.0, 0.85, Linear), key(0.08336171962616822, 0.85, Fast),
                key(0.12336835451999316, 1.0, Auto),
            ],
            waves: &[],
        },
    ],
};

/// Title20.
#[rustfmt::skip]
const TITLE20: Animation = Animation {
    name: "_Title20",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Line, index: 0, attrs: Attrs {
            offset: [0.0, -0.0100581102664861],
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 14, attrs: Attrs {
            scale: [0.999875005775982, 0.999875005775982],
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 23, attrs: Attrs {
            offset: [0.0, 0.0100684516615145],
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[],
    rows: &[
        Row { at: Element::Line(0), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, -2.0, Tangents(-13.088980865478515, 13.088980865478515)),
                key(0.1000738683279241, 0.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(0), property: Property::Opacity, stagger: None,
            keys: &[key(0.0, 0.0, Smooth), key(0.026680942184154177, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Line(2), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, 2.0, Tangents(13.005310440063477, -13.005310440063477)),
                key(0.1000738683279241, 0.0, Slow),
            ],
            waves: &[],
        },
        Row { at: Element::Line(2), property: Property::Opacity, stagger: None,
            keys: &[key(0.0, 0.0, Smooth), key(0.026680942184154177, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::ScaleX, stagger: None,
            keys: &[key(0.0, 0.0, Fast), key(0.1000738683279241, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::ScaleY, stagger: None,
            keys: &[key(0.0, 0.0, Fast), key(0.1000738683279241, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::Opacity, stagger: None,
            keys: &[key(0.0, 0.0, Smooth), key(0.033230514903266575, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::ScaleX, stagger: None,
            keys: &[
                key(0.0, 0.8, Linear), key(0.1000738683279241, 0.8, Fast),
                key(0.14002802819641724, 0.65, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::ScaleY, stagger: None,
            keys: &[
                key(0.0, 0.8, Linear), key(0.1000738683279241, 0.8, Fast),
                key(0.14002802819641724, 0.65, Auto),
            ],
            waves: &[],
        },
    ],
};

/// Title21.
#[rustfmt::skip]
const TITLE21: Animation = Animation {
    name: "_Title21",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Line, index: 0, attrs: Attrs {
            offset: [0.0, -1.10650059321677e-5],
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 9, attrs: Attrs {
            offset: [0.0, -0.00737564500158591],
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 16, attrs: Attrs {
            offset: [0.0, -0.0516631009559328],
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 23, attrs: Attrs {
            offset: [0.0, -0.147345022525534],
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[],
    rows: &[
        Row { at: Element::Line(0), property: Property::OffsetY, stagger: None,
            keys: &[key(0.0, 1.0, Smooth), key(0.04000019760957119, 0.0, Auto)],
            waves: &[spring(0.03317349145056477, 0.06668122970052776, 1.0, 0.7, 1.0, 0.55)],
        },
        Row { at: Element::Line(0), property: Property::Opacity, stagger: None,
            keys: &[key(0.0, 0.0, Slow), key(0.04000019760957119, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, 1.0, Linear), key(0.013319283456269758, 1.0, Smooth),
                key(0.053295219063733454, 0.0, Auto),
            ],
            waves: &[spring(0.046657103432041803, 0.06668971543954871, 1.0, 0.7, 1.0, 0.55)],
        },
        Row { at: Element::Line(1), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.013319283456269758, 0.0, Slow),
                key(0.053295219063733454, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(2), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, 1.0, Linear), key(0.02669410774570654, 1.0, Smooth),
                key(0.06664848615583041, 0.0, Auto),
            ],
            waves: &[spring(0.05985690439139491, 0.06667979409194646, 1.0, 0.7, 1.0, 0.55)],
        },
        Row { at: Element::Line(2), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.02669410774570654, 0.0, Slow),
                key(0.06664848615583041, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(3), property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.04000019760957119, 0.0, Slow),
                key(0.08000411448989958, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(3), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, 1.0, Linear), key(0.04000019760957119, 1.0, Smooth),
                key(0.08000411448989958, 0.0, Auto),
            ],
            waves: &[spring(0.07328034575328288, 0.06666078185908686, 1.0, 0.7, 1.0, 0.55)],
        },
    ],
};

/// Title22.
#[rustfmt::skip]
const TITLE22: Animation = Animation {
    name: "_Title22",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Baseline),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Char, index: 0, attrs: Attrs {
            pivot: Pivot::Part(0.5, 0.999630928842873),
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 1, attrs: Attrs {
            pivot: Pivot::Part(0.5, 0.999630928842873),
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 2, attrs: Attrs {
            pivot: Pivot::Part(0.5, 0.999630928842873),
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 3, attrs: Attrs {
            pivot: Pivot::Part(0.5, 0.999630928842873),
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 4, attrs: Attrs {
            offset: [0.0, 0.00148451023098595], pivot: Pivot::Part(0.5, 0.999630928842873),
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 6, attrs: Attrs {
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 6, attrs: Attrs {
            offset: [0.0, 0.0227650589986837], pivot: Pivot::Part(0.5, 0.999630928842873),
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 7, attrs: Attrs {
            offset: [0.0, 0.0962534167036954], pivot: Pivot::Part(0.5, 0.999630928842873),
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 8, attrs: Attrs {
            offset: [0.0, 0.194245381574587], pivot: Pivot::Part(0.5, 0.999630928842873),
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 9, attrs: Attrs {
            offset: [0.0, 0.151964042513341], pivot: Pivot::Part(0.5, 0.999630928842873),
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 10, attrs: Attrs {
            offset: [0.0, -1.05388027270358], pivot: Pivot::Part(0.5, 0.999630928842873),
            opacity: 1.9988803945144363e-5,
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 11, attrs: Attrs {
            offset: [0.0, -1.5], pivot: Pivot::Part(0.5, 0.999630928842873), opacity: 0.0,
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 12, attrs: Attrs {
            offset: [0.0, -1.5], pivot: Pivot::Part(0.5, 0.999630928842873), opacity: 0.0,
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 13, attrs: Attrs {
            offset: [0.0, -1.5], pivot: Pivot::Part(0.5, 0.999630928842873), opacity: 0.0,
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[
        Stagger { at: Element::Char(0, 0, 0), list: List::Chars,
            duration: 0.12, shape: Some(&[
                key(0.0, 0.0, Auto), key(1.0, 1.0, Auto),
            ]),
        },
    ],
    rows: &[
        Row { at: Element::Char(0, 0, 0), property: Property::OffsetY, stagger: Some(0),
            keys: &[key(0.0, -1.5, Auto), key(0.009961506071468913, 0.0, Auto)],
            waves: &[spring(0.0034130198521005782, 0.06665032911821285, -1.0, 0.7, 1.0, 0.5)],
        },
        Row { at: Element::Char(0, 0, 0), property: Property::Opacity, stagger: Some(0),
            keys: &[
                key(0.0, 0.0, Linear), key(0.003300252624911103, 0.0, Slow),
                key(0.009961506071468913, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Line(0), property: Property::OffsetY, stagger: None,
            keys: &[key(0.0, -1.35, Fast), key(0.039993232492959746, 0.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Line(1), property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, -1.35, Linear), key(0.05329515965134689, -1.35, Fast),
                key(0.0933247308200051, 0.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::ScaleX, stagger: None,
            keys: &[key(0.0, 0.85, Linear), key(0.20026692264758844, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::ScaleY, stagger: None,
            keys: &[key(0.0, 0.85, Linear), key(0.20026692264758844, 1.0, Auto)],
            waves: &[],
        },
    ],
};

/// Title23.
#[rustfmt::skip]
const TITLE23: Animation = Animation {
    name: "_Title23",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Line, index: 0, attrs: Attrs {
            pivot: Pivot::Part(0.5, 0.998490863812817),
            effects: Some(Effects { blur_on: true, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 7, attrs: Attrs {
            offset: [0.0, 0.0158668059736466], pivot: Pivot::Part(0.5, 0.998490863812817),
            effects: Some(Effects { blur_on: true, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Line, index: 14, attrs: Attrs {
            offset: [0.0, 0.120920979634248], pivot: Pivot::Part(0.5, 0.998490863812817),
            effects: Some(Effects { blur_on: true, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[
        Stagger { at: Element::Line(0), list: List::Lines,
            duration: 0.03, shape: Some(&[
                key(0.0, 0.0, Auto), key(1.0, 1.0, Auto),
            ]),
        },
    ],
    rows: &[
        Row { at: Element::Line(0), property: Property::OffsetY, stagger: Some(0),
            keys: &[key(0.0, -3.0, Linear), key(0.03335553841885513, 0.0, Linear)],
            waves: &[spring(0.029555207490616568, 0.06662356487977147, -1.0, 0.7, 1.0, 0.4)],
        },
        Row { at: Element::Line(0), property: Property::ScaleX, stagger: Some(0),
            keys: &[key(0.0, 0.0, Fast), key(0.03335553841885513, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Line(0), property: Property::ScaleY, stagger: Some(0),
            keys: &[key(0.0, 0.0, Fast), key(0.03335553841885513, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Line(0), property: Property::Opacity, stagger: Some(0),
            keys: &[key(0.0, 0.0, Smooth), key(0.03335553841885513, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Line(0), property: Property::BlurDown, stagger: Some(0),
            keys: &[key(0.0, 0.08, Fast), key(0.03335553841885513, 0.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::ScaleX, stagger: None,
            keys: &[
                key(0.0, 1.0, Linear), key(0.09668456794467678, 1.0, Fast),
                key(0.11668254675858618, 0.8, Fast),
            ],
            waves: &[spring(0.1099943295218215, 0.06661866855594233, 0.15, 0.7, 1.0, 0.4)],
        },
        Row { at: Element::Block, property: Property::ScaleY, stagger: None,
            keys: &[
                key(0.0, 1.0, Linear), key(0.09668456794467678, 1.0, Fast),
                key(0.11668254675858618, 0.8, Fast),
            ],
            waves: &[spring(0.1099943295218215, 0.06661866855594231, 0.15, 0.7, 1.0, 0.4)],
        },
    ],
};

/// Title24.
#[rustfmt::skip]
const TITLE24: Animation = Animation {
    name: "_Title24",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: -0.10471975511966,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[],
    staggers: &[
        Stagger { at: Element::Word(0, 0), list: List::Words,
            duration: 0.04, shape: Some(&[
                key(0.0, 0.0, Auto), key(1.0, 1.0, Auto),
            ]),
        },
    ],
    rows: &[
        Row { at: Element::Word(0, 0), property: Property::OffsetY, stagger: Some(0),
            keys: &[key(0.0, -3.0, Fast), key(0.033315019936775554, 0.0, Auto)],
            waves: &[
                spring(0.02447906348348321, 0.06666113791475592, -1.0, 0.6326530612244898, 1.0, 0.45),
            ],
        },
        Row { at: Element::Word(0, 0), property: Property::ScaleX, stagger: Some(0),
            keys: &[key(0.0, 0.85, Fast), key(0.033315019936775554, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Word(0, 0), property: Property::ScaleY, stagger: Some(0),
            keys: &[key(0.0, 0.85, Fast), key(0.033315019936775554, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Word(0, 0), property: Property::Opacity, stagger: Some(0),
            keys: &[key(0.0, 0.0, Smooth), key(0.033315019936775554, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::OffsetY, stagger: None,
            keys: &[key(0.0, -6.0, Fast), key(0.040005113842292966, 0.0, Auto)],
            waves: &[spring(0.032381201750656036, 0.06669172970573732, -1.0, 0.65, 1.0, 0.35)],
        },
    ],
};

/// Title25.
#[rustfmt::skip]
const TITLE25: Animation = Animation {
    name: "_Title25",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: true, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Char, index: 0, attrs: Attrs {
            scale: [0.85, 0.85],
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[
        Stagger { at: Element::Char(0, 0, 0), list: List::Chars,
            duration: 0.06, shape: Some(&[
                key(0.0, 0.0, Auto), key(1.0, 1.0, Auto),
            ]),
        },
    ],
    rows: &[
        Row { at: Element::Char(0, 0, 0), property: Property::Rotation, stagger: Some(0),
            keys: &[key(0.0, 0.24434609527920614, Fast), key(0.03998278170236263, 0.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Char(0, 0, 0), property: Property::ScaleX, stagger: Some(0),
            keys: &[
                key(0.0, 1.0, Linear), key(0.03998278170236263, 1.0, Fast),
                key(0.04997936491270663, 0.85, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Char(0, 0, 0), property: Property::ScaleY, stagger: Some(0),
            keys: &[
                key(0.0, 1.0, Linear), key(0.03998278170236263, 1.0, Fast),
                key(0.04997936491270663, 0.85, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::ScaleX, stagger: None,
            keys: &[
                key(0.0, 2.0, Fast), key(0.04997936491270663, 1.0, Auto),
                key(0.11668988821614473, 1.0, Smooth), key(0.13666881340054937, 1.25, Fast),
            ],
            waves: &[spring(0.13124130361896727, 0.06668521998771283, -1.0, 0.45, 1.0, 0.35)],
        },
        Row { at: Element::Block, property: Property::ScaleY, stagger: None,
            keys: &[
                key(0.0, 2.0, Fast), key(0.04997936491270663, 1.0, Auto),
                key(0.11668988821614473, 1.0, Smooth), key(0.13666881340054937, 1.25, Fast),
            ],
            waves: &[spring(0.13124130361896727, 0.06668521998771285, -1.0, 0.45, 1.0, 0.35)],
        },
        Row { at: Element::Block, property: Property::Opacity, stagger: None,
            keys: &[key(0.0, 0.0, Smooth), key(0.02003208670593561, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::BlurAcross, stagger: None,
            keys: &[key(0.0, 0.06, Fast), key(0.02003208670593561, 0.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::BlurDown, stagger: None,
            keys: &[key(0.0, 0.06, Fast), key(0.02003208670593561, 0.0, Auto)],
            waves: &[],
        },
    ],
};

/// Action Flip.
#[rustfmt::skip]
const ACTION_FLIP: Animation = Animation {
    name: "_Action_Flip",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: -1.03933068695108,
        shear: [0.0, 0.0],
        opacity: 0.01884982889618084,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[],
    staggers: &[],
    rows: &[
        Row { at: Element::Block, property: Property::OffsetX, stagger: None,
            keys: &[
                key(0.0, -10.0, Auto), key(0.13880597014925372, -4.0, Linear),
                key(0.8611940298507462, 4.0, Auto), key(0.9970149253731343, 10.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::Rotation, stagger: None,
            keys: &[
                key(0.0, -1.0471975511965976, Auto),
                key(0.14477611940298507, -0.3490658503988659, Linear),
                key(0.826865671641791, 0.3490658503988659, Auto),
                key(0.9970149253731343, 1.0471975511965976, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Auto), key(0.09104477611940298, 1.0, Linear),
                key(0.8835820895522388, 1.0, Auto), key(0.9970149253731344, 0.0, Auto),
            ],
            waves: &[],
        },
    ],
};

/// Bounce.
#[rustfmt::skip]
const BOUNCE: Animation = Animation {
    name: "_Bounce",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Baseline),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Char, index: 0, attrs: Attrs {
            offset: [0.0, 2.71164021164021],
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[
        Stagger { at: Element::Char(0, 0, 0), list: List::Chars,
            duration: 0.4820348837209303, shape: Some(&[
                key(0.0, 0.0, Auto), key(0.999999999, -1.0, Auto),
            ]),
        },
    ],
    rows: &[
        Row { at: Element::Char(0, 0, 0), property: Property::OffsetY, stagger: Some(0),
            keys: &[key(0.0, 5.0, Auto)],
            waves: &[hops(0.29477124183006537, 0.8793103448275862, 5.0, 8.0, 0.85, 0.5, true)],
        },
        Row { at: Element::Block, property: Property::OffsetX, stagger: None,
            keys: &[key(0.0, -14.1398897171021, Auto), key(0.9220644579670948, 0.0, Auto)],
            waves: &[],
        },
    ],
};

/// Coming at You.
#[rustfmt::skip]
const COMING_AT_YOU: Animation = Animation {
    name: "_Coming_at_You",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0000420342538798,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Char, index: 0, attrs: Attrs {
            opacity: 0.03133859994662216,
            effects: Some(Effects {
                blur_on: true, blur: [0.22090768548897283, 0.196022071005813], glow_on: false, glow: 0.01,
            }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 1, attrs: Attrs {
            opacity: 0.0,
            effects: Some(Effects { blur_on: true, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 2, attrs: Attrs {
            opacity: 0.0,
            effects: Some(Effects { blur_on: true, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 3, attrs: Attrs {
            opacity: 0.0,
            effects: Some(Effects { blur_on: true, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 4, attrs: Attrs {
            opacity: 0.0,
            effects: Some(Effects { blur_on: true, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 5, attrs: Attrs {
            opacity: 0.0,
            effects: Some(Effects { blur_on: true, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[
        Stagger { at: Element::Line(0), list: List::Lines,
            duration: 0.2, shape: Some(&[
                key(0.0, 0.0, Auto), key(1.0, 1.0, Auto),
            ]),
        },
        Stagger { at: Element::Word(0, 0), list: List::LineWords,
            duration: 0.2, shape: Some(&[
                key(0.0, 0.0, Auto), key(1.0, 1.0, Auto),
            ]),
        },
        Stagger { at: Element::Char(0, 0, 0), list: List::Chars,
            duration: 0.2, shape: Some(&[
                key(0.0, 0.0, Auto), key(0.2090909090909091, 0.827564143501127, Auto),
                key(0.3409090909090909, 0.3072788010991609, Auto),
                key(0.4636363636363636, 0.5744100359880769, Auto),
                key(0.5454545454545455, 0.2445660645532814, Auto),
                key(0.7045454545454546, 0.718878622006502, Auto),
                key(0.8454545454545455, 0.49186852394350744, Auto), key(1.0, 1.0, Auto),
            ]),
        },
    ],
    rows: &[
        Row { at: Element::Char(0, 0, 0), property: Property::Opacity, stagger: Some(2),
            keys: &[key(0.0, 0.0, Auto), key(0.1120162932790224, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Char(0, 0, 0), property: Property::BlurAcross, stagger: Some(2),
            keys: &[
                key(0.0, 0.0, Auto), key(1.0000000003410059e-9, 0.22260802469135801, Auto),
                key(0.10794297352342158, 0.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Char(0, 0, 0), property: Property::BlurDown, stagger: Some(2),
            keys: &[
                key(0.0, 0.0, Auto), key(1.0000000003410059e-9, 0.19753086419753085, Auto),
                key(0.10794297352342158, 0.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::ScaleX, stagger: None,
            keys: &[key(0.0, 1.0, Auto), key(0.999999999, 4.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::ScaleY, stagger: None,
            keys: &[key(0.0, 1.0, Auto), key(0.999999999, 4.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 1.0, Auto), key(0.8757637474541752, 1.0, Auto),
                key(0.9938900203665988, 0.0, Auto),
            ],
            waves: &[],
        },
    ],
};

/// Double Flash Glow.
#[rustfmt::skip]
const DOUBLE_FLASH_GLOW: Animation = Animation {
    name: "_Double_Flash_Glow",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: true, glow: 0.0 },
        spacing_floor: true,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[],
    staggers: &[],
    rows: &[
        Row { at: Element::Block, property: Property::Glow, stagger: None,
            keys: &[
                key(0.0, 0.0, Auto), key(0.019163763066202086, 0.17591671273263312, Auto),
                key(0.04529616724738676, 0.09214371008105951, Auto),
                key(0.08710801393728224, 0.0878570112400958, Auto),
                key(0.09407665505226484, 0.19084208586696147, Auto),
                key(0.17944250871080142, 0.028155518702782358, Auto),
            ],
            waves: &[],
        },
    ],
};

/// Drop split.
#[rustfmt::skip]
const DROP_SPLIT: Animation = Animation {
    name: "_Drop_split",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.028332160850509,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Char, index: 0, attrs: Attrs {
            effects: Some(Effects { blur_on: true, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[
        Stagger { at: Element::Char(0, 0, 0), list: List::Chars,
            duration: 0.2, shape: Some(&[
                key(0.0, 0.0, Auto), key(1.0, 1.0, Auto),
            ]),
        },
    ],
    rows: &[
        Row { at: Element::Char(0, 0, 0), property: Property::OffsetY, stagger: Some(0),
            keys: &[key(0.0, 8.135593220338983, Auto), key(0.13793103448275862, 0.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Char(0, 0, 0), property: Property::BlurDown, stagger: Some(0),
            keys: &[key(0.0, 0.65625, Auto), key(0.13609467455621302, 0.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Char(0, 0, 0), property: Property::Opacity, stagger: Some(0),
            keys: &[key(0.0, 0.0, Auto), key(0.13609467455621302, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::Tracking, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.6417624521072797, 0.0, Auto),
                key(1.0, 7.231638418079097, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 1.0, Auto), key(0.6455938697318008, 1.0, Auto),
                key(0.999999999, 0.0, Auto),
            ],
            waves: &[],
        },
    ],
};

/// Dropping words.
#[rustfmt::skip]
const DROPPING_WORDS: Animation = Animation {
    name: "_Dropping_words",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Word, index: 0, attrs: Attrs {
            offset: [0.0, 0.0637475728474367],
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[
        Stagger { at: Element::Word(0, 0), list: List::Words,
            duration: 0.5, shape: Some(&[
                key(0.0, 0.0, Auto), key(1.0, 1.0, Auto),
            ]),
        },
    ],
    rows: &[
        Row { at: Element::Word(0, 0), property: Property::OffsetY, stagger: Some(0),
            keys: &[
                key(0.0, 20.0, Auto),
                key(0.05363984674329503, 0.0, Tangents(-23.2, 1.898123550415039)),
                key(0.12452107279693485, 0.06372793771014085, Tangents(0.0, 0.0)),
            ],
            waves: &[spring(0.04917948601561106, 0.28735632183908044, 3.0, -1.0, 8.0, 0.4)],
        },
    ],
};

/// Earthquake.
#[rustfmt::skip]
const EARTHQUAKE: Animation = Animation {
    name: "_Earthquake",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Char, index: 0, attrs: Attrs {
            offset: [0.0482159109277089, 0.0],
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[
        Stagger { at: Element::Line(0), list: List::Lines, duration: 0.2, shape: None },
        Stagger { at: Element::Word(0, 0), list: List::LineWords, duration: 0.2, shape: None },
        Stagger { at: Element::Char(0, 0, 0), list: List::Chars,
            duration: 0.2, shape: Some(&[
                key(0.0, 0.0, Auto), key(1.0, 1.0, Auto),
            ]),
        },
    ],
    rows: &[
        Row { at: Element::Char(0, 0, 0), property: Property::OffsetX, stagger: Some(2),
            keys: &[key(0.0, 0.0, Auto)],
            waves: &[jitter(0.0, 0.4629080118694362, 0.5, 30.0, 3, &JITTER_42)],
        },
        Row { at: Element::Char(0, 0, 0), property: Property::OffsetY, stagger: Some(2),
            keys: &[key(0.0, 0.0, Auto)],
            waves: &[jitter(0.002967359050445104, 0.45845697329376855, 0.5, 30.0, 3, &JITTER_420)],
        },
    ],
};

/// Fall Down.
#[rustfmt::skip]
const FALL_DOWN: Animation = Animation {
    name: "_Fall_Down",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Baseline),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Line, index: 0, attrs: Attrs { ..Attrs::PLAIN } },
        Node { level: Level::Word, index: 0, attrs: Attrs {
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 0, attrs: Attrs {
            scale: [1.0, 0.989517537563403], shear: -0.00873538536383067,
            pivot: Pivot::At(Across::Middle, Down::Baseline),
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 1, attrs: Attrs {
            pivot: Pivot::At(Across::Middle, Down::Baseline),
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 2, attrs: Attrs {
            pivot: Pivot::At(Across::Middle, Down::Baseline),
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 3, attrs: Attrs {
            pivot: Pivot::At(Across::Middle, Down::Baseline),
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 4, attrs: Attrs {
            pivot: Pivot::At(Across::Middle, Down::Baseline),
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 5, attrs: Attrs {
            pivot: Pivot::At(Across::Middle, Down::Baseline),
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 6, attrs: Attrs {
            pivot: Pivot::At(Across::Middle, Down::Baseline),
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 7, attrs: Attrs {
            pivot: Pivot::At(Across::Middle, Down::Baseline),
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 8, attrs: Attrs {
            pivot: Pivot::At(Across::Middle, Down::Baseline),
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[
        Stagger { at: Element::Char(0, 0, 0), list: List::Chars,
            duration: 0.2, shape: Some(&[
                key(0.0, 0.0, Auto), key(1.0, 1.0, Auto),
            ]),
        },
    ],
    rows: &[
        Row { at: Element::Char(0, 0, 0), property: Property::ShearX, stagger: Some(0),
            keys: &[key(0.0, 0.0, Auto), key(0.09102402022756005, -0.5813953488372094, Auto)],
            waves: &[],
        },
        Row { at: Element::Char(0, 0, 0), property: Property::ScaleY, stagger: Some(0),
            keys: &[key(0.0, 1.0, Auto), key(0.09102402022756005, 0.30232558139535115, Auto)],
            waves: &[],
        },
    ],
};

/// Float and pop.
#[rustfmt::skip]
const FLOAT_AND_POP: Animation = Animation {
    name: "_Float_and_pop",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[],
    staggers: &[
        Stagger { at: Element::Char(0, 0, 0), list: List::Chars,
            duration: 0.2, shape: Some(&[
                key(0.0, 0.0, Auto), key(0.17021276595744683, 0.47200000000000003, Auto),
                key(0.29787234042553196, 0.14242312397060386, Auto),
                key(0.48936170212765956, 0.5236251971582878, Auto),
                key(0.6851063829787234, 0.1076817959293005, Auto),
                key(0.8553191489361702, 0.28086928532589794, Auto),
                key(0.9787234042553191, 0.6599999999999999, Auto),
            ]),
        },
    ],
    rows: &[
        Row { at: Element::Char(0, 0, 0), property: Property::OffsetY, stagger: Some(0),
            keys: &[key(0.0, 0.0, Slow), key(0.4464329877355926, 7.7664974619289335, Auto)],
            waves: &[],
        },
        Row { at: Element::Char(0, 0, 0), property: Property::ScaleX, stagger: Some(0),
            keys: &[
                key(0.0, 1.0, Linear), key(0.4294478527607362, 1.0, Auto),
                key(0.44782509002397186, 4.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Char(0, 0, 0), property: Property::ScaleY, stagger: Some(0),
            keys: &[
                key(0.0, 1.0, Linear), key(0.4294478527607362, 1.0, Auto),
                key(0.44782509002397186, 4.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Char(0, 0, 0), property: Property::Opacity, stagger: Some(0),
            keys: &[
                key(0.0, 1.0, Auto), key(0.4294478527607362, 1.0, Auto),
                key(0.44782509002397186, 0.0, Auto),
            ],
            waves: &[],
        },
    ],
};

/// Fly in from Right.
#[rustfmt::skip]
const FLY_IN_FROM_RIGHT: Animation = Animation {
    name: "_Fly_in_from_Right",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Word, index: 0, attrs: Attrs {
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 0, attrs: Attrs {
            offset: [22.4817953759577, 0.0],
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[
        Stagger { at: Element::Line(0), list: List::Lines,
            duration: 0.2, shape: Some(&[
                key(0.0, 0.0, Auto), key(1.0, 1.0, Auto),
            ]),
        },
        Stagger { at: Element::Word(0, 0), list: List::Words,
            duration: 0.2, shape: Some(&[
                key(0.0, 0.0, Auto), key(1.0, 1.0, Auto),
            ]),
        },
        Stagger { at: Element::Char(0, 0, 0), list: List::Chars,
            duration: 0.2, shape: Some(&[
                key(0.0, 0.0, Auto), key(1.0, 1.0, Auto),
            ]),
        },
    ],
    rows: &[
        Row { at: Element::Char(0, 0, 0), property: Property::OffsetX, stagger: Some(2),
            keys: &[key(0.0, 23.255813953488378, Auto), key(0.05192878338278931, 0.0, Auto)],
            waves: &[],
        },
    ],
};

/// Fly in.
#[rustfmt::skip]
const FLY_IN: Animation = Animation {
    name: "_Fly_in",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Baseline),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 0.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[],
    staggers: &[
        Stagger { at: Element::Char(0, 0, 0), list: List::Chars,
            duration: 0.2, shape: Some(&[
                key(0.0, 0.0, Auto), key(1.0, 1.0, Auto),
            ]),
        },
    ],
    rows: &[
        Row { at: Element::Char(0, 0, 0), property: Property::ScaleX, stagger: Some(0),
            keys: &[key(0.0, 6.341509247634594, Fast), key(0.11834319526627218, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Char(0, 0, 0), property: Property::ScaleY, stagger: Some(0),
            keys: &[key(0.0, 6.341509247634594, Fast), key(0.11685823754789272, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Char(0, 0, 0), property: Property::Opacity, stagger: Some(0),
            keys: &[key(0.0, 0.0, Slow), key(0.11877394636015326, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::ScaleX, stagger: None,
            keys: &[
                key(0.0, 1.5039094297241897, Auto),
                key(0.7761681289532749, 0.6641781108734834, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::ScaleY, stagger: None,
            keys: &[
                key(0.0, 1.5039094297241897, Auto),
                key(0.7761681289532749, 0.6641781108734834, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 1.0, Auto), key(0.7357001972386588, 1.0, Auto),
                key(0.999999999, 0.0, Auto),
            ],
            waves: &[],
        },
    ],
};

/// Jump.
#[rustfmt::skip]
const JUMP: Animation = Animation {
    name: "_Jump",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Char, index: 0, attrs: Attrs {
            scale: [1.0, 0.01], pivot: Pivot::At(Across::Middle, Down::Baseline), opacity: 0.0,
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 1, attrs: Attrs {
            scale: [1.0, 0.01], pivot: Pivot::At(Across::Middle, Down::Baseline), opacity: 0.0,
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 2, attrs: Attrs {
            scale: [1.0, 0.01], pivot: Pivot::At(Across::Middle, Down::Baseline), opacity: 0.0,
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 3, attrs: Attrs {
            scale: [1.0, 0.01], pivot: Pivot::At(Across::Middle, Down::Baseline), opacity: 0.0,
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[
        Stagger { at: Element::Char(0, 0, 0), list: List::Chars,
            duration: 0.1, shape: Some(&[
                key(0.0, 0.0, Auto), key(1.0, 1.0, Auto),
            ]),
        },
    ],
    rows: &[
        Row { at: Element::Char(0, 0, 0), property: Property::ScaleY, stagger: Some(0),
            keys: &[
                key(0.0, 0.01, Auto), key(0.026819923892617253, 1.0, Auto),
                key(0.07088122605363985, 1.0664257572490623, Auto),
            ],
            waves: &[
                spring(0.024294530982645487, 0.13793103448275862, -0.6, 0.3296296296296295, 8.0, 0.4),
            ],
        },
        Row { at: Element::Char(0, 0, 0), property: Property::OffsetY, stagger: Some(0),
            keys: &[
                key(0.0, 0.0, Linear), key(0.0196078431372549, 0.0, Auto),
                key(0.04419206626545007, 4.429223744292238, Auto),
                key(0.07124183006535947, 0.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Char(0, 0, 0), property: Property::Opacity, stagger: Some(0),
            keys: &[key(0.0, 0.0, Auto), key(0.02681992337164751, 1.0, Auto)],
            waves: &[],
        },
    ],
};

/// Menace.
#[rustfmt::skip]
const MENACE: Animation = Animation {
    name: "_Menace",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Baseline),
        rotation: 0.0,
        shear: [0.0211458207162468, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[],
    staggers: &[],
    rows: &[
        Row { at: Element::Block, property: Property::ScaleY, stagger: None,
            keys: &[
                key(0.0, 1.0, Auto), key(0.1011378002528445, 2.8604651162790695, Linear),
                key(0.22882427307206069, 2.8604651162790695, Auto),
                key(0.3388116308470291, 1.0, Linear), key(0.5006321112515802, 1.0, Auto),
                key(0.6333754740834387, 3.3183487259374664, Linear),
                key(0.800252844500632, 2.935573606320242, Auto),
                key(0.8710493046776233, 0.9043062200956937, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::ShearX, stagger: None,
            keys: &[
                key(0.0, 0.0, Auto), key(0.1011378002528445, 1.0465116279069768, Linear),
                key(0.10240202275600507, 1.0, Linear), key(0.22882427307206069, 1.0, Auto),
                key(0.3388116308470291, 0.0, Linear), key(0.5006321112515802, 0.0, Auto),
                key(0.6321112515802781, -1.0, Linear), key(0.8002528445006319, -1.0, Auto),
                key(0.8710493046776233, 0.0, Auto),
            ],
            waves: &[
                jitter(0.10240202275600506, 0.1238938053097345, 1.0, 10.0, 3, &JITTER_42),
                jitter(0.6321112515802781, 0.1681415929203539, 1.0, 10.0, 3, &JITTER_42),
            ],
        },
    ],
};

/// Popup.
#[rustfmt::skip]
const POPUP: Animation = Animation {
    name: "_Popup",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Char, index: 0, attrs: Attrs {
            scale: [1.0, 0.01], pivot: Pivot::At(Across::Middle, Down::Baseline), opacity: 0.0,
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 1, attrs: Attrs {
            scale: [1.0, 0.01], pivot: Pivot::At(Across::Middle, Down::Baseline), opacity: 0.0,
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 2, attrs: Attrs {
            scale: [1.0, 0.01], pivot: Pivot::At(Across::Middle, Down::Baseline), opacity: 0.0,
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 3, attrs: Attrs {
            scale: [1.0, 0.01], pivot: Pivot::At(Across::Middle, Down::Baseline), opacity: 0.0,
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 4, attrs: Attrs {
            scale: [1.0, 0.01], pivot: Pivot::At(Across::Middle, Down::Baseline), opacity: 0.0,
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[
        Stagger { at: Element::Char(0, 0, 0), list: List::Chars,
            duration: 0.1, shape: Some(&[
                key(0.0, 0.0, Auto), key(1.0, 1.0, Auto),
            ]),
        },
    ],
    rows: &[
        Row { at: Element::Char(0, 0, 0), property: Property::ScaleY, stagger: Some(0),
            keys: &[
                key(0.0, 0.01, Auto), key(0.026819923892617253, 1.0, Auto),
                key(0.12068965517241378, 1.022244996460875, Auto),
            ],
            waves: &[
                spring(0.024294530982645487, 0.13793103448275862, -0.6, 0.3296296296296295, 8.0, 0.4),
            ],
        },
        Row { at: Element::Char(0, 0, 0), property: Property::Opacity, stagger: Some(0),
            keys: &[key(0.0, 0.0, Auto), key(0.02681992337164751, 1.0, Auto)],
            waves: &[],
        },
    ],
};

/// Rolling Glow and Enlarge.
#[rustfmt::skip]
const ROLLING_GLOW_AND_ENLARGE: Animation = Animation {
    name: "_Rolling_Glow_and_Enlarge",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Baseline),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[
        Node { level: Level::Char, index: 0, attrs: Attrs {
            scale: [1.02210400546199, 1.02118579484835],
            effects: Some(Effects {
                blur_on: false, blur: [0.0, 0.0], glow_on: true, glow: 0.002992493522328914,
            }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 1, attrs: Attrs {
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: true, glow: 0.0 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 2, attrs: Attrs {
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: true, glow: 0.0 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 3, attrs: Attrs {
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: true, glow: 0.0 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 4, attrs: Attrs {
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: true, glow: 0.0 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 5, attrs: Attrs {
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: true, glow: 0.0 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 6, attrs: Attrs {
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: true, glow: 0.0 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 8, attrs: Attrs {
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: true, glow: 0.0 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 9, attrs: Attrs {
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: true, glow: 0.0 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 10, attrs: Attrs {
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: true, glow: 0.0 }),
            ..Attrs::PLAIN
        } },
        Node { level: Level::Char, index: 11, attrs: Attrs {
            effects: Some(Effects { blur_on: false, blur: [0.0, 0.0], glow_on: true, glow: 0.0 }),
            ..Attrs::PLAIN
        } },
    ],
    staggers: &[
        Stagger { at: Element::Line(0), list: List::Lines, duration: 0.2, shape: None },
        Stagger { at: Element::Word(0, 0), list: List::LineWords, duration: 0.2, shape: None },
        Stagger { at: Element::Char(0, 0, 0), list: List::Chars,
            duration: 0.2, shape: Some(&[
                key(0.0, 0.0, Auto), key(1.0, 1.0, Auto),
            ]),
        },
    ],
    rows: &[
        Row { at: Element::Char(0, 0, 0), property: Property::Glow, stagger: Some(2),
            keys: &[
                key(0.0, 0.0, Auto), key(0.0400593471810089, 0.14125000000000001, Auto),
                key(0.07418397626112759, 0.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Char(0, 0, 0), property: Property::ScaleX, stagger: Some(2),
            keys: &[
                key(0.0, 1.0, Auto), key(0.03857566765578635, 2.0, Auto),
                key(0.0712166172106825, 1.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Char(0, 0, 0), property: Property::ScaleY, stagger: Some(2),
            keys: &[
                key(0.0, 1.0, Auto), key(0.040059347181008904, 2.0, Auto),
                key(0.07270029673590504, 1.0, Auto),
            ],
            waves: &[],
        },
    ],
};

/// Rough Day.
#[rustfmt::skip]
const ROUGH_DAY: Animation = Animation {
    name: "_Rough_Day",
    block: Template {
        pivot: Pivot::At(Across::Far, Down::Baseline),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[],
    staggers: &[],
    rows: &[
        Row { at: Element::Block, property: Property::OffsetX, stagger: None,
            keys: &[
                key(0.0, -12.0, Auto), key(0.09734513274336283, 8.0, Auto),
                key(0.1820480404551201, 8.0, Auto), key(0.27054361567635904, 0.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::Rotation, stagger: None,
            keys: &[
                key(0.0, 0.0, Linear), key(0.09734513274336283, 0.0, Auto),
                key(0.13780025284450062, 0.5844823541562398, Auto),
                key(0.1820480404551201, 0.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::Tracking, stagger: None,
            keys: &[
                key(0.0, 0.0, Auto), key(0.09734513274336283, 0.0, Auto),
                key(0.13780025284450062, -0.6976744186046506, Auto),
                key(0.1820480404551201, 0.0, Auto),
            ],
            waves: &[],
        },
    ],
};

/// Scroll.
#[rustfmt::skip]
const SCROLL: Animation = Animation {
    name: "_Scroll",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Baseline),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: Some(&Path {
            points: &[
                PathPoint {
                    x: [-7.636548755226188e-7, 0.0, 0.13583453893661446],
                    y: [-6.456246828386704e-7, 0.0, 0.11484000682830864],
                },
                PathPoint {
                    x: [51.61661628484726, 51.40140809118748, 42.41808171008782],
                    y: [-0.05755719393491798, -0.19314581155776978, -5.8529526476941145],
                },
            ],
            wrap: true,
            position: -2.0,
        }),
    },
    nodes: &[],
    staggers: &[],
    rows: &[
        Row { at: Element::Block, property: Property::PathPosition, stagger: None,
            keys: &[key(0.0, -2.0, Linear), key(0.999999999, 2.0, Auto)],
            waves: &[],
        },
    ],
};

/// Scroll Left.
#[rustfmt::skip]
const SCROLL_LEFT: Animation = Animation {
    name: "_Scroll_Left",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Baseline),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: Some(&Path {
            points: &[
                PathPoint {
                    x: [-5.0, 0.0, 5.0],
                    y: [0.0, 0.0, 0.0],
                },
                PathPoint {
                    x: [45.0, 50.0, 95.0],
                    y: [0.0, 0.0, 0.0],
                },
            ],
            wrap: true,
            position: -2.0,
        }),
    },
    nodes: &[],
    staggers: &[],
    rows: &[
        Row { at: Element::Block, property: Property::PathPosition, stagger: None,
            keys: &[key(0.0, 2.0, Linear), key(0.999999999, -2.0, Auto)],
            waves: &[],
        },
    ],
};

/// Slide Down.
#[rustfmt::skip]
const SLIDE_DOWN: Animation = Animation {
    name: "_Slide_Down",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 0.0713231602282316,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[],
    staggers: &[],
    rows: &[
        Row { at: Element::Block, property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, 3.0, Auto), key(0.058208955223880594, 1.0, Linear),
                key(0.8626865671641791, -1.0, Auto), key(0.9970149253731343, -3.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Auto), key(0.07164179104477612, 1.0, Auto),
                key(0.908955223880597, 1.0, Auto), key(0.999999999, 0.0, Auto),
            ],
            waves: &[],
        },
    ],
};

/// Slide Left.
#[rustfmt::skip]
const SLIDE_LEFT: Animation = Animation {
    name: "_Slide_Left",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Baseline),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 0.019787464968978312,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[],
    staggers: &[],
    rows: &[
        Row { at: Element::Block, property: Property::OffsetX, stagger: None,
            keys: &[
                key(0.0, 2.2, Auto), key(0.11813643926788686, 0.3, Linear),
                key(0.7653910149750416, -0.9, Auto), key(0.9650582362728786, -2.2, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Auto), key(0.09484193011647254, 1.0, Auto),
                key(0.7321131447587355, 1.0, Auto), key(0.940099833610649, 0.0, Auto),
            ],
            waves: &[],
        },
    ],
};

/// Slide Right.
#[rustfmt::skip]
const SLIDE_RIGHT: Animation = Animation {
    name: "_Slide_Right",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Baseline),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 0.021024413423348307,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[],
    staggers: &[],
    rows: &[
        Row { at: Element::Block, property: Property::OffsetX, stagger: None,
            keys: &[
                key(0.0, -1.2, Auto), key(0.12479201331114811, -0.3, Linear),
                key(0.7554076539101497, 0.9, Auto), key(0.9600665557404326, 2.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Auto), key(0.08985024958402663, 1.0, Auto),
                key(0.7354409317803661, 1.0, Auto), key(0.9367720465890182, 0.0, Auto),
            ],
            waves: &[],
        },
    ],
};

/// Slide Up.
#[rustfmt::skip]
const SLIDE_UP: Animation = Animation {
    name: "_Slide_Up",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.2174363486056208,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[],
    staggers: &[],
    rows: &[
        Row { at: Element::Block, property: Property::OffsetY, stagger: None,
            keys: &[
                key(0.0, -3.0, Auto), key(0.058208955223880594, -1.0, Linear),
                key(0.8626865671641791, 1.0, Auto), key(0.9970149253731343, 3.0, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::Opacity, stagger: None,
            keys: &[
                key(0.0, 0.0, Auto), key(0.07164179104477612, 1.0, Auto),
                key(0.908955223880597, 1.0, Auto), key(0.999999999, 0.0, Auto),
            ],
            waves: &[],
        },
    ],
};

/// Slide.
#[rustfmt::skip]
const SLIDE: Animation = Animation {
    name: "_Slide",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Baseline),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: Some(&Path {
            points: &[
                PathPoint {
                    x: [-7.123327681399521, 0.0, 14.52570998072624],
                    y: [1.806652920534702, 0.0, -3.684081026911734],
                },
                PathPoint {
                    x: [24.50367866307497, 35.44846173375845, 54.56872739211552],
                    y: [23.175600522756575, 14.649823002517223, -0.24449971527833725],
                },
            ],
            wrap: false,
            position: -0.2372881355932206,
        }),
    },
    nodes: &[],
    staggers: &[],
    rows: &[
        Row { at: Element::Block, property: Property::PathPosition, stagger: None,
            keys: &[
                key(0.0, -0.2372881355932206, Auto),
                key(0.25862068965517243, 1.353002560019038, Auto),
                key(0.37739463601532564, 2.8043459594718434, Auto),
            ],
            waves: &[],
        },
    ],
};

/// Speedy.
#[rustfmt::skip]
const SPEEDY: Animation = Animation {
    name: "_Speedy",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Baseline),
        rotation: 0.0,
        shear: [0.0, 0.0],
        opacity: 1.0,
        effects: Effects { blur_on: false, blur: [0.0, 0.0], glow_on: false, glow: 0.01 },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[],
    staggers: &[],
    rows: &[
        Row { at: Element::Block, property: Property::OffsetX, stagger: None,
            keys: &[key(0.0, -20.0, Slow), key(0.1416208449419986, -0.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::ShearX, stagger: None,
            keys: &[
                key(0.0, 1.6860465116279064, Auto),
                key(0.11506730432024885, 1.9759015840916745, Auto),
                key(0.158959675737218, 0.0, Auto),
            ],
            waves: &[spring(0.1515143738645271, 0.15027829313543595, 1.0, 0.7, 8.0, 0.4)],
        },
    ],
};

/// Twist In.
#[rustfmt::skip]
const TWIST_IN: Animation = Animation {
    name: "_Twist_In",
    block: Template {
        pivot: Pivot::At(Across::Middle, Down::Middle),
        rotation: 6.28318530717959,
        shear: [0.0, 0.0],
        opacity: 0.0,
        effects: Effects {
            blur_on: true, blur: [0.10447761194029863, 0.09328358208955222], glow_on: false, glow: 0.01,
        },
        spacing_floor: false,
        fade_selection: true,
        selection: [0.0, 1.0],
        path: None,
    },
    nodes: &[],
    staggers: &[],
    rows: &[
        Row { at: Element::Block, property: Property::Rotation, stagger: None,
            keys: &[
                key(0.0, 6.283185307179586, Auto),
                key(0.11149825783972128, -6.283185307179586, Auto),
            ],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::Opacity, stagger: None,
            keys: &[key(0.0, 0.0, Auto), key(0.16202090592334495, 1.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::BlurAcross, stagger: None,
            keys: &[key(0.0, 0.10447761194029863, Auto), key(0.259581881533101, 0.0, Auto)],
            waves: &[],
        },
        Row { at: Element::Block, property: Property::BlurDown, stagger: None,
            keys: &[key(0.0, 0.09328358208955222, Auto), key(0.2682926829268293, 0.0, Auto)],
            waves: &[],
        },
    ],
};
