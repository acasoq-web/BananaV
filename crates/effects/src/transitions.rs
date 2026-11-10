//! BananaV's transitions: VEGAS's transition plug-ins as OpenFX ones (the
//! Transition context: the picture going out, `SourceFrom`, the one coming
//! in, `SourceTo`, and how far the transition has gone, its `Transition`
//! parameter), with VEGAS's settings as its Help describes them — Linear
//! Wipe, Clock Wipe, Iris, Barn Door, Venetian Blinds, Star Wipe, Push,
//! Slide, Squeeze, Split, Swap, Zoom, Spiral, Flash, Dissolve, Cross
//! Effect and Portals, the Loop and Peels (Page Loop, Page Peel, Page
//! Roll) and the 3D ones (3D Blinds, 3D Cascade, 3D Fly In/Out, 3D
//! Shuffle) —, each with factory presets. VEGAS's own panels, presets and
//! math draw the ones worked out pixel by pixel in [`pixel`], the ones
//! that move the pictures (Barn Door, Push, Slide, Swap, Squeeze, Split,
//! Zoom, Spiral) in [`moves`], Venetian Blinds in [`blinds`], Portals in
//! [`portals`], the pages in [`pages`] and the 3D ones in [`cards`]. The
//! GL transitions ([`gl`]) run GLSL code: their own pixel by pixel.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_void};

use bv_ofx_sys::names::*;
use bv_ofx_sys::*;

use crate::Preset;
use crate::kit::Spec;
use crate::ofx::{Fx, Props, for_rows, is, suites};

mod blinds;
mod cards;
pub(crate) mod gl;
mod moves;
mod pages;
mod pixel;
mod portals;

pub(crate) use pixel::{Frame, framed};

/// One of the transitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    LinearWipe,
    ClockWipe,
    Iris,
    BarnDoor,
    VenetianBlinds,
    StarWipe,
    Push,
    Slide,
    Squeeze,
    Split,
    Swap,
    Zoom,
    Spiral,
    Flash,
    Dissolve,
    CrossEffect,
    Portals,
    Blinds3D,
    Cascade3D,
    FlyInOut3D,
    Shuffle3D,
    PageLoop,
    PagePeel,
    PageRoll,
}

impl Kind {
    pub(crate) const ALL: [Kind; 24] = [
        Kind::LinearWipe,
        Kind::ClockWipe,
        Kind::Iris,
        Kind::BarnDoor,
        Kind::VenetianBlinds,
        Kind::StarWipe,
        Kind::Push,
        Kind::Slide,
        Kind::Squeeze,
        Kind::Split,
        Kind::Swap,
        Kind::Zoom,
        Kind::Spiral,
        Kind::Flash,
        Kind::Dissolve,
        Kind::CrossEffect,
        Kind::Portals,
        Kind::Blinds3D,
        Kind::Cascade3D,
        Kind::FlyInOut3D,
        Kind::Shuffle3D,
        Kind::PageLoop,
        Kind::PagePeel,
        Kind::PageRoll,
    ];

    pub(crate) fn id(self) -> &'static CStr {
        match self {
            Kind::LinearWipe => c"org.bananav.LinearWipe",
            Kind::ClockWipe => c"org.bananav.ClockWipe",
            Kind::Iris => c"org.bananav.Iris",
            Kind::BarnDoor => c"org.bananav.BarnDoor",
            Kind::VenetianBlinds => c"org.bananav.VenetianBlinds",
            Kind::StarWipe => c"org.bananav.StarWipe",
            Kind::Push => c"org.bananav.Push",
            Kind::Slide => c"org.bananav.Slide",
            Kind::Squeeze => c"org.bananav.Squeeze",
            Kind::Split => c"org.bananav.Split",
            Kind::Swap => c"org.bananav.Swap",
            Kind::Zoom => c"org.bananav.Zoom",
            Kind::Spiral => c"org.bananav.Spiral",
            Kind::Flash => c"org.bananav.Flash",
            Kind::Dissolve => c"org.bananav.Dissolve",
            Kind::CrossEffect => c"org.bananav.CrossEffect",
            Kind::Portals => c"org.bananav.Portals",
            Kind::Blinds3D => c"org.bananav.Blinds3D",
            Kind::Cascade3D => c"org.bananav.Cascade3D",
            Kind::FlyInOut3D => c"org.bananav.FlyInOut3D",
            Kind::Shuffle3D => c"org.bananav.Shuffle3D",
            Kind::PageLoop => c"org.bananav.PageLoop",
            Kind::PagePeel => c"org.bananav.PagePeel",
            Kind::PageRoll => c"org.bananav.PageRoll",
        }
    }

    fn label(self) -> &'static CStr {
        match self {
            Kind::LinearWipe => c"Linear Wipe",
            Kind::ClockWipe => c"Clock Wipe",
            Kind::Iris => c"Iris",
            Kind::BarnDoor => c"Barn Door",
            Kind::VenetianBlinds => c"Venetian Blinds",
            Kind::StarWipe => c"Star Wipe",
            Kind::Push => c"Push",
            Kind::Slide => c"Slide",
            Kind::Squeeze => c"Squeeze",
            Kind::Split => c"Split",
            Kind::Swap => c"Swap",
            Kind::Zoom => c"Zoom",
            Kind::Spiral => c"Spiral",
            Kind::Flash => c"Flash",
            Kind::Dissolve => c"Dissolve",
            Kind::CrossEffect => c"Cross Effect",
            Kind::Portals => c"Portals",
            Kind::Blinds3D => c"3D Blinds",
            Kind::Cascade3D => c"3D Cascade",
            Kind::FlyInOut3D => c"3D Fly In/Out",
            Kind::Shuffle3D => c"3D Shuffle",
            Kind::PageLoop => c"Page Loop",
            Kind::PagePeel => c"Page Peel",
            Kind::PageRoll => c"Page Roll",
        }
    }

    /// The Transitions window's category (as VEGAS groups them).
    fn grouping(self) -> &'static CStr {
        match self {
            Kind::LinearWipe
            | Kind::ClockWipe
            | Kind::Iris
            | Kind::BarnDoor
            | Kind::VenetianBlinds
            | Kind::StarWipe => c"BananaV/Wipes",
            Kind::Push | Kind::Slide | Kind::Squeeze | Kind::Split | Kind::Portals => {
                c"BananaV/Reveals"
            }
            Kind::Swap
            | Kind::Zoom
            | Kind::Spiral
            | Kind::Blinds3D
            | Kind::Cascade3D
            | Kind::FlyInOut3D
            | Kind::Shuffle3D => c"BananaV/3D Effects",
            Kind::PageLoop | Kind::PagePeel | Kind::PageRoll => c"BananaV/Loop and Peels",
            Kind::Flash | Kind::Dissolve | Kind::CrossEffect => c"BananaV/Fades",
        }
    }

    fn description(self) -> &'static CStr {
        match self {
            Kind::LinearWipe => {
                c"An edge sweeps across the picture at any angle, revealing the next one; feather it for a softer change."
            }
            Kind::ClockWipe => c"A clock hand sweeps around, clockwise or not, revealing the next picture.",
            Kind::Iris => {
                c"A shape grows from a point (or closes on it) onto the next picture, repeated in rows and columns if you like, with a border."
            }
            Kind::BarnDoor => c"Two doors open from the middle, or close from the edges, onto the next picture.",
            Kind::VenetianBlinds => c"The next picture shows as window blinds open, at any angle.",
            Kind::StarWipe => c"A star grows (or shrinks) onto the next picture: its arms, their length, rings of stars.",
            Kind::Push => c"The next picture comes in and pushes the first one out of the frame, or slides over it.",
            Kind::Slide => c"The next picture slides in over the first one, or the first slides away from it.",
            Kind::Squeeze => {
                c"The next picture comes in squeezed and stretches out as the first one is squeezed away."
            }
            Kind::Split => c"One picture splits into four parts that open onto the other or close over it.",
            Kind::Swap => c"The pictures slide aside and change places.",
            Kind::Zoom => c"The next picture grows from a point to fill the frame.",
            Kind::Spiral => c"A picture moves in a circling slide, in or out, growing or shrinking.",
            Kind::Flash => c"A camera-like flash of light between the pictures.",
            Kind::Dissolve => c"One picture fades into the other.",
            Kind::CrossEffect => {
                c"A crossfade with a zoom, pixelation or blur that peaks halfway and resolves into the next picture."
            }
            Kind::Portals => c"Squares of the next picture fly in over the first one.",
            Kind::Blinds3D => c"The next picture shows as blinds spin open, each drawn in depth.",
            Kind::Cascade3D => c"The next picture drapes down over the first one, fold after fold.",
            Kind::FlyInOut3D => c"A picture flies in from a point in space, or out to it, turning on the way.",
            Kind::Shuffle3D => c"The pictures change places as playing cards shuffled.",
            Kind::PageLoop => c"The next picture comes in over the first one as a book page, looping.",
            Kind::PagePeel => c"The first picture peels back as a page, revealing the next one.",
            Kind::PageRoll => c"The first picture rolls up as a page, revealing the next one.",
        }
    }

    fn specs(self) -> &'static [Spec] {
        match self {
            Kind::LinearWipe => pixel::LINEAR_WIPE,
            Kind::ClockWipe => pixel::CLOCK_WIPE,
            Kind::Iris => pixel::IRIS,
            Kind::BarnDoor => moves::BARN_DOOR,
            Kind::VenetianBlinds => blinds::VENETIAN_BLINDS,
            Kind::StarWipe => pixel::STAR_WIPE,
            Kind::Push => moves::PUSH,
            Kind::Slide => moves::SLIDE,
            Kind::Squeeze => moves::SQUEEZE,
            Kind::Split => moves::SPLIT,
            Kind::Swap => moves::SWAP,
            Kind::Zoom => moves::ZOOM,
            Kind::Spiral => moves::SPIRAL,
            Kind::Flash => pixel::FLASH,
            Kind::Dissolve => pixel::DISSOLVE,
            Kind::CrossEffect => pixel::CROSS_EFFECT,
            Kind::Portals => portals::PORTALS,
            Kind::Blinds3D => cards::BLINDS_3D,
            Kind::Cascade3D => cards::CASCADE_3D,
            Kind::FlyInOut3D => cards::FLY_3D,
            Kind::Shuffle3D => cards::SHUFFLE_3D,
            Kind::PageLoop => pages::PAGE_LOOP,
            Kind::PagePeel => pages::PAGE_PEEL,
            Kind::PageRoll => pages::PAGE_ROLL,
        }
    }

    /// The kind whose identifier is `id`.
    pub(crate) fn of(id: &str) -> Option<Kind> {
        Kind::ALL
            .into_iter()
            .find(|k| k.id().to_str().ok() == Some(id))
    }
}

macro_rules! entry {
    ($($name:ident => $kind:expr;)*) => {
        $(
            /// The plug-in's entry point.
            pub(crate) unsafe extern "C" fn $name(
                action: *const c_char,
                handle: *const c_void,
                in_args: OfxPropertySetHandle,
                out_args: OfxPropertySetHandle,
            ) -> OfxStatus {
                main($kind, action, handle, in_args, out_args)
            }
        )*
    };
}

entry! {
    linear_wipe => Kind::LinearWipe;
    clock_wipe => Kind::ClockWipe;
    iris => Kind::Iris;
    barn_door => Kind::BarnDoor;
    venetian_blinds => Kind::VenetianBlinds;
    star_wipe => Kind::StarWipe;
    push => Kind::Push;
    slide => Kind::Slide;
    squeeze => Kind::Squeeze;
    split => Kind::Split;
    swap => Kind::Swap;
    zoom => Kind::Zoom;
    spiral => Kind::Spiral;
    flash => Kind::Flash;
    dissolve => Kind::Dissolve;
    cross_effect => Kind::CrossEffect;
    portals => Kind::Portals;
    blinds_3d => Kind::Blinds3D;
    cascade_3d => Kind::Cascade3D;
    fly_in_out_3d => Kind::FlyInOut3D;
    shuffle_3d => Kind::Shuffle3D;
    page_loop => Kind::PageLoop;
    page_peel => Kind::PagePeel;
    page_roll => Kind::PageRoll;
}

fn main(
    kind: Kind,
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
        describe(kind, fx);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionDescribeInContext) {
        describe_in_context(kind, fx);
        kOfxStatOK
    } else if is(action, kOfxImageEffectActionRender) {
        render(kind, fx, args)
    } else if is(action, kOfxImageEffectActionGetRegionsOfInterest) {
        // Pictures move and are sampled anywhere: all of both.
        let (origin, frame) = fx.frame();
        let all = [
            origin[0],
            origin[1],
            origin[0] + frame[0],
            origin[1] + frame[1],
        ];
        out.set_dbls(c"OfxImageClipPropRoI_SourceFrom", &all);
        out.set_dbls(c"OfxImageClipPropRoI_SourceTo", &all);
        kOfxStatOK
    } else if is(action, kOfxActionInstanceChanged) {
        pixel::show(kind, fx, args.dbl(kOfxPropTime, 0));
        kOfxStatOK
    } else if is(action, kOfxActionCreateInstance) || is(action, kOfxActionBeginInstanceEdit) {
        pixel::show(kind, fx, 0.0);
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

fn describe(kind: Kind, fx: Fx) {
    let p = fx.props();
    p.set_str(kOfxPropLabel, kind.label());
    p.set_str(kOfxImageEffectPluginPropGrouping, kind.grouping());
    p.set_str(kOfxPropPluginDescription, kind.description());
    p.set_strs(
        kOfxImageEffectPropSupportedContexts,
        &[kOfxImageEffectContextTransition],
    );
    p.set_strs(
        kOfxImageEffectPropSupportedPixelDepths,
        &[kOfxBitDepthFloat, kOfxBitDepthShort, kOfxBitDepthByte],
    );
    p.set_int(kOfxImageEffectPropSupportsTiles, 0);
    p.set_int(kOfxImageEffectPropSupportsMultiResolution, 1);
    p.set_int(kOfxImageEffectPluginPropHostFrameThreading, 0);
    p.set_str(
        kOfxImageEffectPluginRenderThreadSafety,
        kOfxImageEffectRenderFullySafe,
    );
}

fn describe_in_context(kind: Kind, fx: Fx) {
    let rgba = [kOfxImageComponentRGBA];
    fx.define_clip(kOfxImageEffectTransitionSourceFromClipName, &rgba);
    fx.define_clip(kOfxImageEffectTransitionSourceToClipName, &rgba);
    fx.define_clip(kOfxImageEffectOutputClipName, &rgba);
    let p = fx.define_param(
        kOfxParamTypeDouble,
        kOfxImageEffectTransitionParamName,
        c"Progress",
    );
    p.set_dbl(kOfxParamPropDefault, 0.0);
    p.set_dbl(kOfxParamPropMin, 0.0);
    p.set_dbl(kOfxParamPropMax, 1.0);
    crate::kit::define_all(fx, kind.specs());
}

/// What the card draws for transition `id` (`accel::transition_plan`) in
/// a frame `frame` canonical units and `size` pixels big.
pub(crate) fn plan(
    id: &str,
    values: crate::accel::Values<'_>,
    frame: [f64; 2],
    size: [f64; 2],
    progress: f64,
) -> Option<crate::accel::TransitionPlan> {
    if Some(id) == crate::gradient_wipe::ID.to_str().ok() {
        return Some(crate::gradient_wipe::plan(values, frame, size, progress));
    }
    let kind = Kind::of(id)?;
    if pixel::draws(kind) {
        return Some(pixel::plan(kind, values, frame, size, progress));
    }
    if moves::draws(kind) {
        return Some(moves::plan(kind, values, size, progress));
    }
    if blinds::draws(kind) {
        return Some(blinds::plan(values, frame, size, progress));
    }
    if portals::draws(kind) {
        return Some(portals::plan(values, frame, size, progress));
    }
    if pages::draws(kind) {
        return Some(pages::plan(kind, values, frame, size, progress));
    }
    cards::draws(kind).then(|| cards::plan(kind, values, frame, size, progress))
}

fn render(kind: Kind, fx: Fx, args: Props) -> OfxStatus {
    if pixel::draws(kind) {
        return pixel::render(kind, fx, args);
    }
    if moves::draws(kind) {
        return moves::render(kind, fx, args);
    }
    if blinds::draws(kind) {
        return blinds::render(fx, args);
    }
    if portals::draws(kind) {
        return portals::render(fx, args);
    }
    if pages::draws(kind) {
        return pages::render(kind, fx, args);
    }
    if cards::draws(kind) {
        return cards::render(kind, fx, args);
    }
    kOfxStatErrUnsupported
}

// Presets.

/// The factory presets of transition `kind`.
pub(crate) fn presets(kind: Kind) -> Vec<Preset> {
    let own = match kind {
        Kind::LinearWipe
        | Kind::ClockWipe
        | Kind::Iris
        | Kind::StarWipe
        | Kind::Flash
        | Kind::Dissolve
        | Kind::CrossEffect => pixel::presets(kind),
        Kind::BarnDoor
        | Kind::Push
        | Kind::Slide
        | Kind::Squeeze
        | Kind::Split
        | Kind::Swap
        | Kind::Zoom
        | Kind::Spiral => moves::presets(kind),
        Kind::VenetianBlinds => blinds::presets(),
        Kind::Portals => portals::presets(),
        Kind::PageLoop | Kind::PagePeel | Kind::PageRoll => pages::presets(kind),
        Kind::Blinds3D | Kind::Cascade3D | Kind::FlyInOut3D | Kind::Shuffle3D => {
            cards::presets(kind)
        }
    };
    // The transition at its defaults first, as VEGAS lists a transition
    // none of whose presets is its default: "Default" for the ones it
    // names so (the 3D ones, Venetian Blinds, Portals), "(Default)" for
    // the others.
    let name = match kind {
        Kind::Blinds3D
        | Kind::Cascade3D
        | Kind::FlyInOut3D
        | Kind::Shuffle3D
        | Kind::VenetianBlinds
        | Kind::Portals => "Default",
        _ => "(Default)",
    };
    std::iter::once(Preset {
        name,
        values: Vec::new(),
    })
    .chain(own)
    .collect()
}
