//! Ciência de cor do BananaV.
//!
//! O espaço de trabalho é **RGB linear com primárias BT.709, float 32 bits,
//! alfa pré-multiplicado** ([`ImageRgbaF32`](bv_core::ImageRgbaF32)). Todo
//! quadro decodificado é convertido para ele antes da composição, e convertido
//! de volta só na saída (exportação ou monitor).

pub mod convert;
pub mod matrix;
pub mod primaries;
pub mod transfer;

pub use convert::{
    frame_to_linear, frame_to_linear_levels, frame_to_linear_scaled, linear_to_420,
    linear_to_420_levels, linear_to_display_rgba8, linear_to_display_rgba8_straight,
    linear_to_frame, linear_to_frame_levels, par_rows,
};
