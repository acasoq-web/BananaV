//! Tipos fundamentais do BananaV.
//!
//! Esta crate não depende de nenhuma outra crate do workspace: tudo aqui pode
//! ser usado por codecs, containers, timeline, render e interface.

pub mod audio;
pub mod base64;
pub mod curve;
pub mod date;
pub mod locale;
pub mod time;
pub mod timecode;
pub mod video;

pub use audio::{AudioBuffer, ChannelLayout};
pub use time::{FrameRate, SampleRate, TICKS_PER_SECOND, Tick, TimeRange};
pub use video::{
    ColorInfo, ColorPrimaries, ColorRange, FieldOrder, ImageRgbaF32, MatrixCoefficients,
    PixelFormat, Plane, PlaneData, TransferCharacteristic, VideoFrame,
};
