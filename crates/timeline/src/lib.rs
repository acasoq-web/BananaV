//! Modelo de projeto do BananaV.
//!
//! Um [`Project`] tem uma lista de mídias importadas e uma lista de trilhas.
//! Cada trilha guarda clipes ordenados pelo início. Clipes de vídeo e áudio
//! importados juntos compartilham um [`GroupId`] e são editados juntos.
//!
//! Composição: trilhas de vídeo são listadas de cima para baixo e a de cima
//! cobre as de baixo. Dentro de uma trilha, se dois clipes se sobrepõem, o que
//! começa depois cobre o anterior. No áudio, tudo que se sobrepõe é somado.

mod arrange;
pub mod audio_fx;
pub mod bins;
pub mod busses;
pub mod caption_files;
pub mod captions;
pub mod cd;
mod clipboard;
pub mod color;
pub mod commands;
pub mod ducking;
mod edit;
pub mod envelope;
pub mod envelope_paint;
pub mod fcp_import;
pub mod fx;
pub mod fx_automation;
mod history;
pub mod input_busses;
pub mod interchange;
mod markers;
pub mod mask;
mod mask_raster;
mod model;
pub mod motion;
pub mod motion_tracks;
mod multicam;
pub mod nesting;
pub mod notes;
pub mod otio;
pub mod pancrop;
pub mod prerender;
mod ripple;
pub mod scene;
pub mod space;
pub mod storyboards;
pub mod subclips;
pub mod surround;
pub mod tags;
pub mod takes;
pub mod track_groups;
pub mod veg;
pub mod video_bus;
pub mod vst_id;
pub mod xmeml;
mod xml;

pub use bins::{BinId, MediaBin};
pub use busses::{
    Bus, BusId, Channel, FxChain, FxChainId, MappedStrip, Output, Send, SendTo, Strip,
};
pub use cd::{CdIndex, CdRow, CdTrack};
pub use clipboard::ClipboardData;
pub use color::{
    AcesDisplay, AcesLook, AcesSpace, AcesVersion, AcesView, ColorManagement, ColorPipeline,
    ColorSpace, ViewTransform,
};
pub use commands::CommandMarker;
pub use ducking::{DuckLevels, Ducking};
pub use edit::EditError;
pub use envelope::{Envelope, EnvelopeKind, EnvelopeOwner, EnvelopePoint};
pub use fx::{
    Direction, Effect, EffectKind, FadeCurve, Graph, KeyCurve, KeyHandles, ParamKind, ParamSpec,
    PluginFx, PluginKey, PluginParam, PluginValue, Show, Transform, TransformKey, Transition,
    VstFormat, VstFx,
};
pub use history::{Gesture, History};
pub use input_busses::{InputBus, InputBusId, InputMonitor, MAX_INPUT_BUSSES};
pub use model::{
    AlphaChannel, AudioCd, AudioQuality, AudioStreamInfo, AutomationMode, ChannelMap, Clip, ClipId,
    CompositeMode, CompositingGamma, DeinterlaceMethod, Generator, GroupId, ImageSequence, Marker,
    MarkerId, MasterBus, MediaId, MediaInfo, MediaMarker, MediaReel, MediaRotation, MediaSource,
    NewEvents, OutTransition, OutputRotation, PanMode, PixelAspect, PixelFormat, Project,
    ProjectSettings, RecordInput, Region, RenderQuality, ResampleMode, RulerFormat, SAME_PAIRS,
    SMART_THRESHOLDS, Summary, SyncLink, TRACK_PALETTE, Tempo, Track, TrackAutomation, TrackId,
    TrackKind, VideoLayer, VideoReading, VideoStreamInfo, markers_from_json, markers_to_json,
};
pub use motion::{Halo, HaloKey, HaloLook, MotionAt, TrackMotion};
pub use motion_tracks::{MotionTrack, TrackKey};
pub use prerender::Prerender;
pub use ripple::RippleScope;
pub use storyboards::{StoryCell, Storyboard, TimelineCell};
pub use subclips::Subclip;
pub use surround::{SurroundKey, SurroundOwner, SurroundPan};
pub use tags::{
    MediaKind, MediaSearch, MediaTags, SearchCondition, SearchField, SearchIn, SearchOp, SmartBin,
};
pub use takes::Take;
pub use track_groups::{TrackGroup, TrackGroupId};
pub use video_bus::{MotionBlurType, VideoBus};
pub use vst_id::VstWanted;
