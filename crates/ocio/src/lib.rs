//! Color management as OpenColorIO's. BananaV reads OpenColorIO
//! configurations ([`Config`]: their YAML, profile versions 1 and 2 —
//! color spaces, roles, displays and views, view transforms, looks) and
//! works out their conversions itself ([`Processor`]): matrices, curves,
//! logs, CDLs, ranges, LUT files and the builtin transforms — ACES 1.3's
//! encodings, Looks and SDR output transforms ([`aces`], ported from the
//! Academy's reference), cameras' log encodings, displays' encodings.
//! Processors are worked out exactly; `bv-gpu` has a kernel for each step
//! BananaV's own configuration uses (the same math).
//!
//! BananaV's own configuration ([`builtin`]) is ACES 1.3's, written for
//! it: ACES2065-1 the scene reference, CIE XYZ (D65) the displays'.

pub mod aces;
mod builtins;
pub mod config;
pub mod curves;
mod luts;
pub mod math;
pub mod ops;
mod processor;
pub mod yaml;

pub use config::{
    ColorSpace, Config, Display, Error, Kind, Look, Reference, Result, Transform, View,
    ViewTransform,
};
pub use processor::Processor;

/// BananaV's ACES 1.3 configuration, as text.
pub const BUILTIN_CONFIG: &str = include_str!("../configs/bananav-aces-1.3.ocio");

/// BananaV's ACES 1.3 configuration.
pub fn builtin() -> &'static Config {
    static CONFIG: std::sync::OnceLock<Config> = std::sync::OnceLock::new();
    CONFIG.get_or_init(|| {
        Config::parse(BUILTIN_CONFIG, None).expect("BananaV's own configuration reads")
    })
}
