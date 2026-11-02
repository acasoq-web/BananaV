//! What a plug-in tells about itself, in plain data: what scans keep in
//! their cache and what interfaces show.

use std::ffi::CStr;
use std::path::PathBuf;

use bv_ofx_sys::names::*;
use serde::{Deserialize, Serialize};

use crate::effect::{Clip, Effect};
use crate::param::Param;
use crate::value::{ParamType, Value};

/// Where a plug-in runs: its job.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Context {
    /// Changes a picture (an effect on an event, a track, the output).
    Filter,
    /// Like a filter, with other inputs it may use.
    General,
    /// Goes from one picture to another.
    Transition,
    /// Makes pictures from nothing (generated media).
    Generator,
}

impl Context {
    pub const ALL: [Context; 4] = [
        Context::Filter,
        Context::General,
        Context::Transition,
        Context::Generator,
    ];

    pub fn ofx_name(self) -> &'static CStr {
        match self {
            Context::Filter => kOfxImageEffectContextFilter,
            Context::General => kOfxImageEffectContextGeneral,
            Context::Transition => kOfxImageEffectContextTransition,
            Context::Generator => kOfxImageEffectContextGenerator,
        }
    }

    pub fn from_ofx_name(s: &str) -> Option<Context> {
        Context::ALL
            .into_iter()
            .find(|c| c.ofx_name().to_bytes() == s.as_bytes())
    }
}

/// An error of a plug-in or of loading it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Error {
    /// The system could not load the binary (its message).
    Load(String),
    /// The binary has no OpenFX entry points.
    NotOfx,
    /// The binary says it is not for this host (`OfxSetHost` failed).
    NotForThisHost,
    /// An action did not succeed.
    Action {
        action: String,
        status: i32,
    },
    /// The plug-in cannot run in that context.
    Context,
    /// The plug-in rendered without filling its output.
    NoOutput,
    /// The plug-in crashed (while being described or tried in its own
    /// process, or caught by the guard).
    Crashed(String),
    /// The plug-in did not answer in time (described or tried in its own
    /// process, which was stopped).
    Hung(String),
    /// CUDA's driver refused (a render on the graphics card): its message.
    Cuda(String),
    Other(String),
}

/// The specification's name of a status (`kOfxStatFailed`…).
pub fn status_name(status: i32) -> &'static str {
    match status {
        0 => "kOfxStatOK",
        1 => "kOfxStatFailed",
        2 => "kOfxStatErrFatal",
        3 => "kOfxStatErrUnknown",
        4 => "kOfxStatErrMissingHostFeature",
        5 => "kOfxStatErrUnsupported",
        6 => "kOfxStatErrExists",
        7 => "kOfxStatErrFormat",
        8 => "kOfxStatErrMemory",
        9 => "kOfxStatErrBadHandle",
        10 => "kOfxStatErrBadIndex",
        11 => "kOfxStatErrValue",
        12 => "kOfxStatReplyYes",
        13 => "kOfxStatReplyNo",
        14 => "kOfxStatReplyDefault",
        15 => "kOfxStatUnlicensed",
        1000 => "kOfxStatErrImageFormat",
        1001 => "kOfxStatGPUOutOfMemory",
        1002 => "kOfxStatGPURenderFailed",
        _ => "an unknown status",
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Load(m) => write!(f, "the plug-in could not be loaded: {m}"),
            Error::NotOfx => write!(f, "not an OpenFX plug-in"),
            Error::NotForThisHost => write!(f, "the plug-in does not run in this program"),
            Error::Action { action, status } => {
                write!(f, "{action} answered {}", status_name(*status))
            }
            Error::Context => write!(f, "the plug-in cannot do that"),
            Error::NoOutput => write!(f, "the plug-in rendered nothing"),
            Error::Crashed(m) => write!(f, "the plug-in crashed: {m}"),
            Error::Hung(m) => write!(f, "the plug-in did not answer: {m}"),
            Error::Cuda(m) => write!(f, "the graphics card could not render it: {m}"),
            Error::Other(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for Error {}

/// One plug-in: who it is, what it does, its clips and parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PluginInfo {
    /// Unique ("com.example.Blur").
    pub identifier: String,
    pub version: [u32; 2],
    /// The binary it lives in, and its index there.
    pub file: PathBuf,
    pub index: u32,
    pub label: String,
    /// Where interfaces list it ("Blur/Fast").
    pub grouping: String,
    pub description: String,
    pub contexts: Vec<Context>,
    /// Channel types it takes ("OfxBitDepthFloat"…).
    pub depths: Vec<String>,
    pub multi_resolution: bool,
    pub tiles: bool,
    /// It reads its inputs at other times.
    pub temporal: bool,
    /// It draws on the video preview.
    pub overlay: bool,
    pub thread_safety: String,
    pub single_instance: bool,
    /// It reads or writes files (the reader and writer contexts of
    /// Natron's and TuttleOFX's extensions): BananaV imports and renders
    /// files itself and lists it nowhere.
    #[serde(default)]
    pub files: bool,
    /// Its clips and parameters in each context.
    pub described: Vec<ContextInfo>,
}

impl PluginInfo {
    /// The name to show: its label, or its identifier.
    pub fn name(&self) -> &str {
        if self.label.is_empty() {
            &self.identifier
        } else {
            &self.label
        }
    }

    pub fn context(&self, c: Context) -> Option<&ContextInfo> {
        self.described.iter().find(|d| d.context == c)
    }
}

/// A plug-in's clips and parameters in one context.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContextInfo {
    pub context: Context,
    pub clips: Vec<ClipInfo>,
    pub params: Vec<ParamInfo>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClipInfo {
    pub name: String,
    pub label: String,
    pub optional: bool,
    pub mask: bool,
    pub temporal: bool,
    pub components: Vec<String>,
}

impl ClipInfo {
    pub fn of(c: &Clip) -> ClipInfo {
        use bv_ofx_sys::names::*;
        ClipInfo {
            name: c.name.clone(),
            label: c.props.str(kOfxPropLabel, 0).unwrap_or_default(),
            optional: c.props.flag(kOfxImageClipPropOptional).unwrap_or(false),
            mask: c.props.flag(kOfxImageClipPropIsMask).unwrap_or(false),
            temporal: c
                .props
                .flag(kOfxImageEffectPropTemporalClipAccess)
                .unwrap_or(false),
            components: c.props.strs(kOfxImageEffectPropSupportedComponents),
        }
    }
}

/// A parameter as its plug-in describes it (an instance's may change
/// while it runs: labels, options, enabled, hidden).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParamInfo {
    pub name: String,
    pub ty: ParamType,
    pub label: String,
    pub hint: String,
    /// The group it sits in ("" at the top).
    pub parent: String,
    pub default: Value,
    pub min: Vec<f64>,
    pub max: Vec<f64>,
    pub display_min: Vec<f64>,
    pub display_max: Vec<f64>,
    /// What a double means (`kOfxParamDoubleTypeAngle`…).
    pub double_type: String,
    /// Canonical or normalised coordinates.
    pub coordinates: String,
    /// A choice's options.
    pub options: Vec<String>,
    /// A string choice's stored values (one per option).
    pub option_values: Vec<String>,
    pub dimension_labels: Vec<String>,
    pub animates: bool,
    pub enabled: bool,
    pub secret: bool,
    pub persistent: bool,
    pub string_mode: String,
    pub digits: i32,
    pub increment: f64,
    /// A group shows open.
    pub open: bool,
    /// A page's parameters.
    pub children: Vec<String>,
    /// The parametric curves' count and range.
    pub curves: u32,
    pub curve_range: [f64; 2],
    /// The curves' colours, red, green and blue from 0 to 1 (none: the
    /// host's).
    #[serde(default)]
    pub curve_colours: Vec<[f64; 3]>,
    /// The size (width, height, in screen pixels) of its own interface
    /// drawn with the draw suite, when it has one (BananaV's extension,
    /// `kBananaVParamPropInteract`).
    #[serde(default)]
    pub interact: Option<[f64; 2]>,
    /// That interface waits closed under its triangle at first.
    #[serde(default)]
    pub interact_closed: bool,
    /// The model a color's picker shows at first ("HSV", "HSL", "RGB" or
    /// "Lab"; empty: the host's), BananaV's extension.
    #[serde(default)]
    pub color_model: String,
    /// Its slider runs logarithmically over its display range, BananaV's
    /// extension.
    #[serde(default)]
    pub logarithmic: bool,
    /// How it sits among the others (OpenFX's layout hint): 2, the next
    /// one on its line.
    #[serde(default)]
    pub layout_hint: i32,
    /// The picture a push button asks the host for and the clip it
    /// arrives on (BananaV's extension, `kBananaVParamPropPicture`; empty:
    /// none).
    #[serde(default)]
    pub picture: String,
    #[serde(default)]
    pub picture_clip: String,
    /// What a push button asks the host to analyze, the parameter the
    /// result goes to and the one saying how hard to look (BananaV's
    /// extension, `kBananaVParamPropAnalysis`; empty: none).
    #[serde(default)]
    pub analysis: String,
    #[serde(default)]
    pub analysis_param: String,
    #[serde(default)]
    pub analysis_method: String,
    #[serde(default)]
    pub analysis_exclude: String,
    #[serde(default)]
    pub analysis_layout: String,
    /// The parameters of the analysis's own work (BananaV's
    /// `kBananaVParamPropAnalysisTuning`, apart by spaces; empty: none).
    #[serde(default)]
    pub analysis_tuning: String,
    /// The area analyzed, the grid its features spread over, how they are
    /// followed and how many times the frames are halved (BananaV's
    /// `kBananaVParamPropAnalysisRegion`, `…Grid`, `…Follow` and
    /// `…Reduction`; empty: none).
    #[serde(default)]
    pub analysis_region: String,
    #[serde(default)]
    pub analysis_grid: String,
    #[serde(default)]
    pub analysis_follow: String,
    #[serde(default)]
    pub analysis_reduction: String,
    /// A text whose parts the host styles: the runs' parameter, then the
    /// parameters a part may set (BananaV's `kBananaVParamPropRichText`;
    /// empty: none).
    #[serde(default)]
    pub rich_text: String,
    /// A mask a push button has the host track, and the parameters it
    /// keyframes and reads (BananaV's `kBananaVParamPropMaskTracking`;
    /// empty: none).
    #[serde(default)]
    pub mask_tracking: String,
    /// The rate parameter a push button lengthens its event by (BananaV's
    /// `kBananaVParamPropEventLength`; empty: none).
    #[serde(default)]
    pub event_length: String,
    /// A file path's file is one to write, which need not exist yet
    /// (OpenFX's `kOfxParamPropStringFilePathExists` off): a dialog to
    /// save.
    #[serde(default)]
    pub file_to_write: bool,
}

impl ParamInfo {
    pub fn of(p: &Param) -> ParamInfo {
        use bv_ofx_sys::names::*;
        let props = &p.props;
        let s = |n| props.str(n, 0).unwrap_or_default();
        let range = props.dbls(kOfxParamPropParametricRange);
        ParamInfo {
            name: p.name.clone(),
            ty: p.ty,
            label: p.label(),
            hint: s(kOfxParamPropHint),
            parent: s(kOfxParamPropParent),
            default: p.default_value(),
            min: props.dbls(kOfxParamPropMin),
            max: props.dbls(kOfxParamPropMax),
            display_min: props.dbls(kOfxParamPropDisplayMin),
            display_max: props.dbls(kOfxParamPropDisplayMax),
            double_type: s(kOfxParamPropDoubleType),
            coordinates: s(kOfxParamPropDefaultCoordinateSystem),
            options: props.strs(kOfxParamPropChoiceOption),
            option_values: props.strs(kOfxParamPropChoiceEnum),
            dimension_labels: props.strs(kOfxParamPropDimensionLabel),
            animates: props.flag(kOfxParamPropAnimates).unwrap_or(false),
            enabled: props.flag(kOfxParamPropEnabled).unwrap_or(true),
            secret: props.flag(kOfxParamPropSecret).unwrap_or(false),
            persistent: props.flag(kOfxParamPropPersistant).unwrap_or(true),
            string_mode: s(kOfxParamPropStringMode),
            digits: props.int(kOfxParamPropDigits, 0).unwrap_or(2),
            increment: props.dbl(kOfxParamPropIncrement, 0).unwrap_or(1.0),
            open: props.flag(kOfxParamPropGroupOpen).unwrap_or(true),
            children: props.strs(kOfxParamPropPageChild),
            curves: props
                .int(kOfxParamPropParametricDimension, 0)
                .unwrap_or(1)
                .max(0) as u32,
            curve_range: [
                range.first().copied().unwrap_or(0.0),
                range.get(1).copied().unwrap_or(1.0),
            ],
            curve_colours: props
                .dbls(kOfxParamPropParametricUIColour)
                .as_chunks::<3>()
                .0
                .to_vec(),
            interact: props
                .ptr(bv_ofx_sys::extensions::kBananaVParamPropInteract, 0)
                .filter(|p| !p.is_null())
                .map(|_| {
                    let size = props.ints(kOfxParamPropInteractPreferedSize);
                    let at = |i: usize| size.get(i).copied().unwrap_or(10).max(1) as f64;
                    [at(0), at(1)]
                }),
            interact_closed: props.int(bv_ofx_sys::extensions::kBananaVParamPropInteractOpen, 0)
                == Some(0),
            color_model: s(bv_ofx_sys::extensions::kBananaVParamPropColorModel),
            logarithmic: props.int(bv_ofx_sys::extensions::kBananaVParamPropLogarithmic, 0)
                == Some(1),
            layout_hint: props.int(kOfxParamPropLayoutHint, 0).unwrap_or(0),
            picture: s(bv_ofx_sys::extensions::kBananaVParamPropPicture),
            picture_clip: s(bv_ofx_sys::extensions::kBananaVParamPropPictureClip),
            analysis: s(bv_ofx_sys::extensions::kBananaVParamPropAnalysis),
            analysis_param: s(bv_ofx_sys::extensions::kBananaVParamPropAnalysisParam),
            analysis_method: s(bv_ofx_sys::extensions::kBananaVParamPropAnalysisMethod),
            analysis_exclude: s(bv_ofx_sys::extensions::kBananaVParamPropAnalysisExclude),
            analysis_layout: s(bv_ofx_sys::extensions::kBananaVParamPropAnalysisLayout),
            analysis_tuning: s(bv_ofx_sys::extensions::kBananaVParamPropAnalysisTuning),
            analysis_region: s(bv_ofx_sys::extensions::kBananaVParamPropAnalysisRegion),
            analysis_grid: s(bv_ofx_sys::extensions::kBananaVParamPropAnalysisGrid),
            analysis_follow: s(bv_ofx_sys::extensions::kBananaVParamPropAnalysisFollow),
            analysis_reduction: s(bv_ofx_sys::extensions::kBananaVParamPropAnalysisReduction),
            rich_text: s(bv_ofx_sys::extensions::kBananaVParamPropRichText),
            mask_tracking: s(bv_ofx_sys::extensions::kBananaVParamPropMaskTracking),
            event_length: s(bv_ofx_sys::extensions::kBananaVParamPropEventLength),
            file_to_write: props.int(kOfxParamPropStringFilePathExists, 0) == Some(0),
        }
    }
}

/// A context's clips and parameters as its descriptor describes them.
pub(crate) fn context_info(context: Context, d: &Effect) -> ContextInfo {
    ContextInfo {
        context,
        clips: d.clips().into_iter().map(ClipInfo::of).collect(),
        params: d.params.all().into_iter().map(ParamInfo::of).collect(),
    }
}
