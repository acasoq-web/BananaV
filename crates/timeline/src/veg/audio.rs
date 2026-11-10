//! The other editor's audio plug-ins that BananaV has as its own effects:
//! each one's class and BananaV's effect, and how its settings read into
//! the effect's parameters. Three families of settings:
//!
//! - DirectX: a record of 4-byte numbers after their format's GUID, the
//!   parameter being a number ÷ `div` × `mul` + `add` ([`PLUGINS`]), and the
//!   graphs' points in the records of [`graphs`];
//! - the studio effects: a program of normalized values, big-endian
//!   ([`STUDIO`]);
//! - the Modern Equalizer: its state, as text ([`modern_eq`]).
//!
//! Any other VST 3 plug-in comes as BananaV's VST 3 effect with its states
//! ([`vst3`]), any other VST 2 plug-in as BananaV's VST 2 effect with its
//! program ([`vst2`]): its class's family and its kind byte tell one; the
//! settings are the plug-in's own, as it wrote them.

use super::riff::Guid;
use crate::fx::{Effect, EffectKind};

/// How a record keeps a number.
#[derive(Clone, Copy, Debug)]
enum Number {
    /// 4 bytes, whole.
    Whole,
    /// 4 bytes, with a point.
    Point,
    /// 8 bytes, with a point.
    Double,
}

/// Where a record keeps one of BananaV's parameters.
#[derive(Clone, Copy, Debug)]
struct Field {
    param: u8,
    at: u16,
    number: Number,
    div: f32,
    mul: f32,
    add: f32,
}

/// A whole number's field.
const fn i(param: u8, at: u16, div: f32, mul: f32, add: f32) -> Field {
    Field {
        param,
        at,
        number: Number::Whole,
        div,
        mul,
        add,
    }
}

/// A field of a number with a point.
const fn f(param: u8, at: u16, div: f32, mul: f32, add: f32) -> Field {
    Field {
        param,
        at,
        number: Number::Point,
        div,
        mul,
        add,
    }
}

/// A field of a long number with a point.
const fn d(param: u8, at: u16, div: f32, mul: f32, add: f32) -> Field {
    Field {
        param,
        at,
        number: Number::Double,
        div,
        mul,
        add,
    }
}

/// A plug-in BananaV has: its class, BananaV's effect, its bypass
/// switch's byte (on when not 0) and its fields. A graph's plug-in has
/// none: [`graphs`] reads its record.
struct Plugin {
    class: Guid,
    kind: EffectKind,
    bypass: Option<u16>,
    fields: &'static [Field],
}

#[rustfmt::skip]
const PLUGINS: &[Plugin] = &[
    // Volume
    Plugin {
        class: Guid::parse("EE38CA88-D78E-4BFB-B05E-577892730C83"),
        kind: EffectKind::Volume,
        bypass: None,
        fields: &[f(0, 0, 1.0, 1.0, 0.0)],
    },
    // Pan
    Plugin {
        class: Guid::parse("EB6213DB-08FF-4510-9F8D-3058B0ECE4C6"),
        kind: EffectKind::Pan,
        bypass: None,
        fields: &[f(0, 0, 1.0, 1.0, 0.0)],
    },
    // ExpressFX Audio Restoration
    Plugin {
        class: Guid::parse("0000000C-0F56-11D2-9887-00A0C969725B"),
        kind: EffectKind::ExpressAudioRestoration,
        bypass: None,
        fields: &[
            i(0, 0, 1.0, 1.0, 0.0),
            i(1, 4, 10.0, 1.0, 0.0),
            i(2, 8, 1.0, 1.0, 0.0),
            i(3, 12, 1.0, 1.0, 0.0),
            i(4, 16, 1.0, 1.0, 0.0),
            i(5, 20, 10.0, 1.0, 0.0),
        ],
    },
    // Amplitude Modulation
    Plugin {
        class: Guid::parse("FDB0D300-6F82-11D0-AEBC-00A0C9053912"),
        kind: EffectKind::AmplitudeModulation,
        bypass: Some(292),
        fields: &[
        ],
    },
    // Chorus
    Plugin {
        class: Guid::parse("28D9F1E0-6ECC-11D0-AEBC-00A0C9053912"),
        kind: EffectKind::Chorus,
        bypass: Some(48),
        fields: &[
            i(0, 0, 10.0, 1.0, 0.0),
            i(1, 4, 10.0, 1.0, 0.0),
            i(2, 8, 10.0, 1.0, 0.0),
            i(3, 20, 1.0, 1.0, 0.0),
            i(4, 44, 1.0, 1.0, 0.0),
            i(5, 12, 1.0, 1.0, 0.0),
            i(6, 28, 1000.0, 1.0, 0.0),
            i(7, 32, 100.0, 1.0, 0.0),
            i(8, 16, 100.0, 1.0, 0.0),
            i(9, 24, 10.0, 1.0, 0.0),
            i(10, 36, 1.0, 1.0, 0.0),
            i(11, 40, 1.0, 1.0, 0.0),
            i(12, 60, 1.0, 1.0, 0.0),
            i(13, 56, 1000.0, 1.0, 0.0),
            i(14, 52, 1.0, 1.0, 0.0),
        ],
    },
    // Distortion
    Plugin {
        class: Guid::parse("39224540-6F92-11D0-AEBC-00A0C9053912"),
        kind: EffectKind::Distortion,
        bypass: Some(552),
        fields: &[
        ],
    },
    // Dither
    Plugin {
        class: Guid::parse("260DF3E1-AC77-11D2-9E93-00C04F68BE44"),
        kind: EffectKind::Dither,
        bypass: None,
        fields: &[
            i(0, 0, 1.0, 1.0, 0.0),
            i(1, 4, 1.0, 1.0, 0.0),
            i(2, 8, 1.0, 1.0, 0.0),
        ],
    },
    // ExpressFX Amplitude Modulation
    Plugin {
        class: Guid::parse("00000005-0F56-11D2-9887-00A0C969725B"),
        kind: EffectKind::ExpressAmplitudeModulation,
        bypass: None,
        fields: &[
            i(0, 20, 10.0, 1.0, 0.0),
            i(1, 16, 1000.0, 1.0, 0.0),
            i(2, 32, 1000.0, 1.0, 0.0),
        ],
    },
    // ExpressFX Chorus
    Plugin {
        class: Guid::parse("00000000-0F56-11D2-9887-00A0C969725B"),
        kind: EffectKind::ExpressChorus,
        bypass: None,
        fields: &[
            i(0, 0, 1.0, 1.0, 0.0),
            i(1, 4, 1000.0, 1.0, 0.0),
            i(2, 8, 100.0, 1.0, 0.0),
            i(3, 12, 1000.0, 1.0, 0.0),
        ],
    },
    // ExpressFX Delay
    Plugin {
        class: Guid::parse("00000001-0F56-11D2-9887-00A0C969725B"),
        kind: EffectKind::ExpressDelay,
        bypass: None,
        fields: &[
            i(0, 4, 10.0, 1.0, 0.0),
            i(1, 8, 1000.0, 1.0, 0.0),
            i(2, 0, 1000.0, 1.0, 0.0),
        ],
    },
    // ExpressFX Distortion
    Plugin {
        class: Guid::parse("00000002-0F56-11D2-9887-00A0C969725B"),
        kind: EffectKind::ExpressDistortion,
        bypass: None,
        fields: &[
            i(0, 0, 1000.0, 1.0, 0.0),
            i(1, 4, 1000.0, 1.0, 0.0),
            i(2, 8, 1.0, 1.0, 0.0),
            i(3, 12, 1000.0, 1.0, 0.0),
        ],
    },
    // ExpressFX Dynamics
    Plugin {
        class: Guid::parse("00000008-0F56-11D2-9887-00A0C969725B"),
        kind: EffectKind::ExpressDynamics,
        bypass: None,
        fields: &[
            i(0, 0, 10.0, 1.0, 0.0),
            i(1, 4, 10.0, 1.0, 0.0),
            i(2, 8, 10.0, 1.0, 0.0),
            i(3, 12, 10.0, 1.0, 0.0),
            i(4, 16, 1.0, 1.0, 0.0),
        ],
    },
    // ExpressFX Equalization
    Plugin {
        class: Guid::parse("00000003-0F56-11D2-9887-00A0C969725B"),
        kind: EffectKind::ExpressEqualization,
        bypass: None,
        fields: &[
            i(0, 8, 10.0, 1.0, 0.0),
            i(1, 4, 10.0, 1.0, 0.0),
            i(2, 12, 10.0, 1.0, 0.0),
            i(3, 0, 1000.0, 1.0, 0.0),
        ],
    },
    // ExpressFX Flange/Wah-Wah
    Plugin {
        class: Guid::parse("00000004-0F56-11D2-9887-00A0C969725B"),
        kind: EffectKind::ExpressFlangeWahWah,
        bypass: None,
        fields: &[
            i(0, 0, 2.0, 1.0, 0.0),
            i(1, 12, 1000.0, 1.0, 0.0),
            i(2, 16, 100.0, 1.0, 0.0),
            i(3, 28, 1000.0, 1.0, 0.0),
        ],
    },
    // ExpressFX Graphic EQ
    Plugin {
        class: Guid::parse("00000009-0F56-11D2-9887-00A0C969725B"),
        kind: EffectKind::ExpressGraphicEq,
        bypass: None,
        fields: &[
            i(0, 4, 10.0, 1.0, 0.0),
            i(1, 8, 10.0, 1.0, 0.0),
            i(2, 12, 10.0, 1.0, 0.0),
            i(3, 16, 10.0, 1.0, 0.0),
            i(4, 20, 10.0, 1.0, 0.0),
            i(5, 24, 10.0, 1.0, 0.0),
            i(6, 28, 10.0, 1.0, 0.0),
            i(7, 32, 10.0, 1.0, 0.0),
            i(8, 36, 10.0, 1.0, 0.0),
            i(9, 40, 10.0, 1.0, 0.0),
            i(10, 44, 10.0, 1.0, 0.0),
            i(11, 0, 1.0, 1.0, 0.0),
        ],
    },
    // ExpressFX Noise Gate
    Plugin {
        class: Guid::parse("0000000A-0F56-11D2-9887-00A0C969725B"),
        kind: EffectKind::ExpressNoiseGate,
        bypass: None,
        fields: &[
            i(0, 8, 10.0, 1.0, 0.0),
            i(1, 0, 1.0, 1.0, 0.0),
            i(2, 4, 1.0, 1.0, 0.0),
        ],
    },
    // ExpressFX Reverb
    Plugin {
        class: Guid::parse("00000006-0F56-11D2-9887-00A0C969725B"),
        kind: EffectKind::ExpressReverb,
        bypass: None,
        fields: &[
            i(0, 12, 1.0, 1.0, 0.0),
            i(1, 20, 90.0, 1.0, -0.1111111),
            i(2, 24, 200.0, 1.0, 0.0),
            i(3, 44, 1000.0, 1.0, 0.0),
        ],
    },
    // ExpressFX Stutter
    Plugin {
        class: Guid::parse("00000007-0F56-11D2-9887-00A0C969725B"),
        kind: EffectKind::ExpressStutter,
        bypass: None,
        fields: &[
            i(0, 0, 100.0, 1.0, 0.0),
            i(1, 4, 100.0, 1.0, 0.0),
            i(2, 8, 1.0, 1.0, 0.0),
            i(3, 12, 1000.0, 1.0, 0.0),
        ],
    },
    // ExpressFX Time Stretch
    Plugin {
        class: Guid::parse("0000000B-0F56-11D2-9887-00A0C969725B"),
        kind: EffectKind::ExpressTimeStretch,
        bypass: None,
        fields: &[
            i(0, 8, 1.0, 1.0, 0.0),
            d(1, 0, 100.0, 1.0, 0.0),
        ],
    },
    // Flange/Wah-wah
    Plugin {
        class: Guid::parse("F09F6980-7845-11D0-AEBC-00A0C9053912"),
        kind: EffectKind::FlangeWahWah,
        bypass: Some(28),
        fields: &[
            i(0, 0, 1.0, 1.0, 0.0),
            i(1, 4, 10.0, 1.0, 0.0),
            i(2, 8, 10.0, 1.0, 0.0),
            i(3, 12, 1000.0, 1.0, 0.0),
            i(4, 16, 100.0, 1.0, 0.0),
            i(5, 20, 1.0, 1.0, 0.0),
            i(6, 24, 100.0, 1.0, 0.0),
            i(7, 40, 1.0, 1.0, 0.0),
            i(8, 36, 1000.0, 1.0, 0.0),
            i(9, 32, 1.0, 1.0, 0.0),
        ],
    },
    // Gapper/Snipper
    Plugin {
        class: Guid::parse("3F901A20-79BE-11D0-AEBC-00A0C9053912"),
        kind: EffectKind::GapperSnipper,
        bypass: None,
        fields: &[
            i(0, 12, 1.0, 1.0, 0.0),
            i(1, 0, 10000.0, 1.0, 0.0),
            i(2, 4, 10000.0, 1.0, 0.0),
            i(3, 8, 1.0, 1.0, 0.0),
        ],
    },
    // Graphic Dynamics
    Plugin {
        class: Guid::parse("F3B8E880-B4E0-11D0-AEBC-00A0C9053912"),
        kind: EffectKind::GraphicDynamics,
        bypass: Some(160),
        fields: &[
        ],
    },
    // Graphic EQ
    Plugin {
        class: Guid::parse("8B7226EE-4584-11D1-B4CB-00A0C9270A10"),
        kind: EffectKind::GraphicEq,
        bypass: None,
        fields: &[
        ],
    },
    // Multi-Band Dynamics
    Plugin {
        class: Guid::parse("026D0AA0-9BB9-11D0-AEBC-00A0C9053912"),
        kind: EffectKind::MultiBandDynamics,
        bypass: Some(208),
        fields: &[
            i(0, 0, 10.0, 1.0, 0.0),
            i(1, 132, 1.0, 1.0, 0.0),
            i(2, 136, 1.0, 1.0, 0.0),
            i(3, 192, 1.0, 1.0, 0.0),
            i(4, 20, 1.0, 1.0, 0.0),
            i(5, 116, 1.0, 1.0, 0.0),
            i(6, 100, 10.0, 1.0, 0.0),
            i(7, 4, 10.0, 1.0, 0.0),
            i(8, 36, 10.0, 1.0, 0.0),
            i(9, 84, 10.0, 1.0, 0.0),
            i(10, 52, 10.0, 1.0, 0.0),
            i(11, 68, 10.0, 1.0, 0.0),
            i(12, 140, 1.0, 1.0, 0.0),
            i(13, 196, 1.0, 1.0, 0.0),
            i(14, 24, 1.0, 1.0, 0.0),
            i(15, 120, 1.0, 1.0, 0.0),
            i(16, 104, 10.0, 1.0, 0.0),
            i(17, 8, 10.0, 1.0, 0.0),
            i(18, 40, 10.0, 1.0, 0.0),
            i(19, 88, 10.0, 1.0, 0.0),
            i(20, 56, 10.0, 1.0, 0.0),
            i(21, 72, 10.0, 1.0, 0.0),
            i(22, 144, 1.0, 1.0, 0.0),
            i(23, 200, 1.0, 1.0, 0.0),
            i(24, 28, 1.0, 1.0, 0.0),
            i(25, 124, 1.0, 1.0, 0.0),
            i(26, 108, 10.0, 1.0, 0.0),
            i(27, 12, 10.0, 1.0, 0.0),
            i(28, 44, 10.0, 1.0, 0.0),
            i(29, 92, 10.0, 1.0, 0.0),
            i(30, 60, 10.0, 1.0, 0.0),
            i(31, 76, 10.0, 1.0, 0.0),
            i(32, 148, 1.0, 1.0, 0.0),
            i(33, 204, 1.0, 1.0, 0.0),
            i(34, 32, 1.0, 1.0, 0.0),
            i(35, 128, 1.0, 1.0, 0.0),
            i(36, 112, 10.0, 1.0, 0.0),
            i(37, 16, 10.0, 1.0, 0.0),
            i(38, 48, 10.0, 1.0, 0.0),
            i(39, 96, 10.0, 1.0, 0.0),
            i(40, 64, 10.0, 1.0, 0.0),
            i(41, 80, 10.0, 1.0, 0.0),
        ],
    },
    // Multi-Tap Delay
    Plugin {
        class: Guid::parse("7298A3E0-78EE-11D0-AEBC-00A0C9053912"),
        kind: EffectKind::MultiTapDelay,
        bypass: None,
        fields: &[
            i(0, 112, 10.0, 1.0, 0.0),
            i(1, 104, 10.0, 1.0, 0.0),
            i(2, 100, 10.0, 1.0, 0.0),
            i(3, 128, 10.0, 1.0, 0.0),
            i(4, 124, 100.0, 1.0, 0.0),
            i(5, 108, 100.0, 1.0, 0.0),
            i(6, 116, 1.0, 1.0, 0.0),
            i(7, 120, 1.0, 1.0, 0.0),
            i(8, 96, 1.0, 1.0, 0.0),
            i(9, 32, 100.0, 1.0, 0.0),
            i(10, 0, 1.0, 1.0, 0.0),
            i(11, 64, -100.0, 1.0, 0.0),
            i(12, 36, 100.0, 1.0, 0.0),
            i(13, 4, 1.0, 1.0, 0.0),
            i(14, 68, -100.0, 1.0, 0.0),
            i(15, 40, 100.0, 1.0, 0.0),
            i(16, 8, 1.0, 1.0, 0.0),
            i(17, 72, -100.0, 1.0, 0.0),
            i(18, 44, 100.0, 1.0, 0.0),
            i(19, 12, 1.0, 1.0, 0.0),
            i(20, 76, -100.0, 1.0, 0.0),
            i(21, 48, 100.0, 1.0, 0.0),
            i(22, 16, 1.0, 1.0, 0.0),
            i(23, 80, -100.0, 1.0, 0.0),
            i(24, 52, 100.0, 1.0, 0.0),
            i(25, 20, 1.0, 1.0, 0.0),
            i(26, 84, -100.0, 1.0, 0.0),
            i(27, 56, 100.0, 1.0, 0.0),
            i(28, 24, 1.0, 1.0, 0.0),
            i(29, 88, -100.0, 1.0, 0.0),
            i(30, 60, 100.0, 1.0, 0.0),
            i(31, 28, 1.0, 1.0, 0.0),
            i(32, 92, -100.0, 1.0, 0.0),
        ],
    },
    // Noise Gate
    Plugin {
        class: Guid::parse("B97C0F23-196D-11D1-B99B-00A0C9053912"),
        kind: EffectKind::NoiseGateFx,
        bypass: None,
        fields: &[
            i(0, 8, 10.0, 1.0, 0.0),
            i(1, 0, 1.0, 1.0, 0.0),
            i(2, 4, 1.0, 1.0, 0.0),
        ],
    },
    // Paragraphic EQ
    Plugin {
        class: Guid::parse("D616F3E0-D622-11CE-AAC5-0020AF0B99A3"),
        kind: EffectKind::ParagraphicEq,
        bypass: None,
        fields: &[
            i(0, 0, 10.0, 1.0, 0.0),
            i(1, 4, 10.0, 1.0, 0.0),
            i(2, 8, 10.0, 1.0, 0.0),
            i(3, 24, 10.0, 1.0, 0.0),
            i(4, 40, 1.0, 1.0, 0.0),
            i(5, 8, 10.0, 1.0, 0.0),
            i(6, 24, 10.0, 1.0, 0.0),
            i(7, 44, 1.0, 1.0, 0.0),
            i(8, 16, 10.0, 1.0, 0.0),
            i(9, 24, 10.0, 1.0, 0.0),
            i(10, 48, 1.0, 1.0, 0.0),
            i(11, 20, 10.0, 1.0, 0.0),
            i(12, 36, 10.0, 1.0, 0.0),
            i(13, 52, 1.0, 1.0, 0.0),
            i(14, 56, 1.0, 1.0, 0.0),
            i(15, 64, 1.0, 1.0, 0.0),
            i(16, 72, 10.0, 1.0, 0.0),
            i(17, 60, 1.0, 1.0, 0.0),
            i(18, 68, 1.0, 1.0, 0.0),
            i(19, 76, 10.0, 1.0, 0.0),
        ],
    },
    // Parametric EQ
    Plugin {
        class: Guid::parse("A8448720-96FD-11D0-AEBC-00A0C9053912"),
        kind: EffectKind::ParametricEq,
        bypass: None,
        fields: &[
            i(0, 16, 10.0, 1.0, 0.0),
            i(1, 12, 10.0, 1.0, 0.0),
            i(2, 0, 1.0, 1.0, 0.0),
            i(3, 4, 1.0, 1.0, 0.0),
            i(4, 8, 10.0, 1.0, 0.0),
            i(5, 20, 1.0, 1.0, 0.0),
        ],
    },
    // Pitch Shift
    Plugin {
        class: Guid::parse("ED1B4100-93BE-11D0-AEBC-00A0C9053912"),
        kind: EffectKind::PitchShift,
        bypass: None,
        fields: &[
            i(0, 0, 1.0, 1.0, 0.0),
            i(1, 4, 1.0, 1.0, 0.0),
            i(2, 8, 1.0, 1.0, 0.0),
            i(3, 32, 1.0, 1.0, 0.0),
            i(4, 12, 1.0, 1.0, 0.0),
            i(5, 16, 1.0, 1.0, 0.0),
        ],
    },
    // Resonant Filter
    Plugin {
        class: Guid::parse("A6A78627-D619-48BF-AD26-0C6B44B5C7D8"),
        kind: EffectKind::ResonantFilter,
        bypass: None,
        fields: &[
            f(0, 0, 1.0, 1.0, 0.0),
            f(1, 4, 100.0, 1.0, 0.0),
            f(2, 20, 100.0, 1.0, 0.0),
            f(3, 8, 1.0, 1.0, 0.0),
            f(4, 12, 1.0, 1.0, 0.0),
        ],
    },
    // Reverb
    Plugin {
        class: Guid::parse("607682E0-6E21-11D0-AEBC-00A0C9053912"),
        kind: EffectKind::ReverbFx,
        bypass: Some(44),
        fields: &[
            i(0, 12, 1.0, 1.0, 0.0),
            i(1, 0, 10.0, 1.0, 0.0),
            i(2, 4, 10.0, 1.0, 0.0),
            i(3, 8, 10.0, 1.0, 0.0),
            i(4, 16, 1.0, 1.0, 0.0),
            i(5, 20, 10.0, 1.0, 0.0),
            i(6, 24, 1.0, 1.0, 0.0),
            i(7, 28, 1.0, 1.0, 0.0),
            i(8, 32, 1.0, 1.0, 0.0),
            i(9, 36, 1.0, 1.0, 0.0),
            i(10, 40, 1.0, 1.0, 0.0),
        ],
    },
    // Simple Delay
    Plugin {
        class: Guid::parse("54F29260-79B1-11D0-AEBC-00A0C9053912"),
        kind: EffectKind::SimpleDelay,
        bypass: Some(20),
        fields: &[
            i(0, 16, 10.0, 1.0, 0.0),
            i(1, 12, 10.0, 1.0, 0.0),
            i(2, 4, 1000.0, 1.0, 0.0),
            i(3, 32, 1.0, 1.0, 0.0),
            i(4, 28, 1000.0, 1.0, 0.0),
            i(5, 24, 1.0, 1.0, 0.0),
            i(6, 0, 1.0, 1.0, 0.0),
            i(7, 8, 10.0, 1.0, 0.0),
            i(8, 44, 1.0, 1.0, 0.0),
            i(9, 40, 1000.0, 1.0, 0.0),
            i(10, 36, 1.0, 1.0, 0.0),
        ],
    },
    // Smooth/Enhance
    Plugin {
        class: Guid::parse("9E3E4540-8339-11D0-AEBC-00A0C9053912"),
        kind: EffectKind::SmoothEnhance,
        bypass: Some(4),
        fields: &[
            i(0, 0, 1.0, 1.0, 0.0),
        ],
    },
    // Time Stretch
    Plugin {
        class: Guid::parse("B97C0F22-196D-11D1-B99B-00A0C9053912"),
        kind: EffectKind::TimeStretch,
        bypass: None,
        fields: &[
            i(0, 8, 1.0, 1.0, 0.0),
            d(1, 0, 100.0, 1.0, 0.0),
        ],
    },
    // Track Compressor
    Plugin {
        class: Guid::parse("23C9F225-40EC-11D2-9D36-00C04F8EDC1E"),
        kind: EffectKind::TrackCompressor,
        bypass: None,
        fields: &[
            f(0, 0, 1.0, 1.0, 0.0),
            f(1, 4, 1.0, 1.0, 0.0),
            f(2, 8, 1.0, 1.0, 0.0),
            f(3, 12, 1.0, 1.0, 0.0),
            f(4, 16, 1.0, 1.0, 0.0),
            f(5, 20, 1.0, 1.0, 0.0),
            i(6, 24, 1.0, 1.0, 0.0),
            i(7, 28, 1.0, 1.0, 0.0),
        ],
    },
    // Track EQ
    Plugin {
        class: Guid::parse("8CB69A0A-10E8-11D2-9B89-00104B8D13C2"),
        kind: EffectKind::TrackEq,
        bypass: Some(100),
        fields: &[
            i(0, 80, 1.0, 1.0, 0.0),
            i(1, 48, 1.0, 1.0, -1.0),
            i(2, 32, 1.0, 1.0, 0.0),
            i(3, 0, 10.0, 1.0, 0.0),
            i(4, 16, 10.0, 1.0, 0.0),
            i(5, 64, 1.0, 1.0, -1.0),
            i(6, 84, 1.0, 1.0, 0.0),
            i(7, 52, 1.0, 1.0, -1.0),
            i(8, 36, 1.0, 1.0, 0.0),
            i(9, 4, 10.0, 1.0, 0.0),
            i(10, 20, 10.0, 1.0, 0.0),
            i(11, 68, 1.0, 1.0, -1.0),
            i(12, 88, 1.0, 1.0, 0.0),
            i(13, 56, 1.0, 1.0, -1.0),
            i(14, 40, 1.0, 1.0, 0.0),
            i(15, 8, 10.0, 1.0, 0.0),
            i(16, 24, 10.0, 1.0, 0.0),
            i(17, 72, 1.0, 1.0, -1.0),
            i(18, 92, 1.0, 1.0, 0.0),
            i(19, 60, 1.0, 1.0, -1.0),
            i(20, 44, 1.0, 1.0, 0.0),
            i(21, 12, 10.0, 1.0, 0.0),
            i(22, 28, 10.0, 1.0, 0.0),
            i(23, 76, 1.0, 1.0, -1.0),
            i(24, 104, 1.0, 1.0, 0.0),
            i(25, 108, 1.0, 1.0, 0.0),
            i(26, 112, 1.0, 1.0, 0.0),
            i(27, 116, 1.0, 1.0, 0.0),
            i(28, 120, 1.0, 1.0, 0.0),
            i(29, 124, 1.0, 1.0, 0.0),
        ],
    },
    // Track Noise Gate
    Plugin {
        class: Guid::parse("869419DD-501F-11D3-8CDC-00C04F6B8E4C"),
        kind: EffectKind::TrackNoiseGate,
        bypass: Some(12),
        fields: &[
            i(0, 8, 10.0, 1.0, 0.0),
            i(1, 0, 1.0, 1.0, 0.0),
            i(2, 4, 1.0, 1.0, 0.0),
        ],
    },
    // Wave Hammer Surround
    Plugin {
        class: Guid::parse("8010C341-6D4C-4390-B828-E4D246C3DDB2"),
        kind: EffectKind::WaveHammer,
        bypass: None,
        fields: &[
            i(0, 0, 1.0, 1.0, 0.0),
            i(1, 8, 10.0, 1.0, 0.0),
            i(2, 16, 10.0, 1.0, 0.0),
            i(3, 12, 10.0, 1.0, 0.0),
            i(4, 40, 1.0, 1.0, 0.0),
            i(5, 32, 1.0, 1.0, 0.0),
            i(6, 28, 1.0, 1.0, 0.0),
            i(7, 44, 1.0, 1.0, 0.0),
            i(8, 20, 10.0, 1.0, 0.0),
            i(9, 24, 10.0, 1.0, 0.0),
            i(10, 36, 1.0, 1.0, 0.0),
            i(11, 4, 1.0, 1.0, 0.0),
            i(12, 48, 10.0, 1.0, 0.0),
            i(13, 52, 10.0, 1.0, 0.0),
            i(14, 60, 1.0, 1.0, 0.0),
            i(15, 56, 10.0, 1.0, 0.0),
            i(16, 88, 1.0, 1.0, 0.0),
            i(17, 92, 1.0, 1.0, 0.0),
            i(18, 96, 1.0, 1.0, 0.0),
            i(19, 100, 1.0, 1.0, 0.0),
            i(20, 104, 1.0, 1.0, 0.0),
            i(21, 108, 1.0, 1.0, 0.0),
            i(22, 112, 1.0, 1.0, 0.0),
            i(23, 116, 1.0, 1.0, 0.0),
            i(24, 120, 1.0, 1.0, 0.0),
            i(25, 124, 1.0, 1.0, 0.0),
            i(26, 128, 1.0, 1.0, 0.0),
            i(27, 132, 1.0, 1.0, 0.0),
        ],
    },
    // Vibrato
    Plugin {
        class: Guid::parse("D6802BA0-A056-11D0-AEBC-00A0C9053912"),
        kind: EffectKind::Vibrato,
        bypass: Some(276),
        fields: &[
        ],
    },
];

/// A studio effect: its class, BananaV's effect, and for each parameter
/// the place of its value in the program and where its range's middle
/// falls (0.5 being a straight line).
struct Studio {
    class: Guid,
    kind: EffectKind,
    fields: &'static [(u8, u8, f32)],
}

#[rustfmt::skip]
const STUDIO: &[Studio] = &[
    // Chorus/Flanger
    Studio { class: Guid::parse("4D583231-8A7A-478E-024A-0007E95D36FA"), kind: EffectKind::ChorusFlanger, fields: &[
        (0, 1, 0.5), (1, 3, 0.5), (2, 4, 0.5), (3, 5, 0.125), (4, 6, 0.25), (5, 7, 0.25), (6, 8, 0.5), (7, 20, 0.125), (8, 9, 0.5),
    ] },
    // Compressor
    Studio { class: Guid::parse("4D583237-3C15-7005-024A-0007E95D36FA"), kind: EffectKind::StudioCompressor, fields: &[
        (0, 3, 0.5), (1, 4, 0.5), (2, 6, 0.5), (3, 7, 0.25), (4, 8, 0.03065), (5, 9, 0.066), (6, 20, 0.25), (7, 21, 0.5), (8, 11, 0.5), (9, 22, 0.5), (10, 15, 0.5), (11, 5, 0.1), (12, 10, 0.5),
    ] },
    // DeEsser
    Studio { class: Guid::parse("4D583238-1767-B5FF-024A-0007E95D36FA"), kind: EffectKind::DeEsser, fields: &[
        (0, 3, 0.5), (1, 4, 0.5), (2, 5, 0.5), (3, 6, 0.5), (4, 20, 0.1), (5, 10, 0.5),
    ] },
    // Gate
    Studio { class: Guid::parse("4D583239-72C1-E1B5-024A-0007E95D36FA"), kind: EffectKind::StudioGate, fields: &[
        (0, 3, 0.5), (1, 4, 0.5), (2, 6, 0.7333), (3, 7, 0.1333), (4, 8, 0.0995), (5, 9, 0.0965), (6, 17, 0.5), (7, 11, 0.5), (8, 20, 0.1), (9, 15, 0.5), (10, 5, 0.1), (11, 10, 0.5), (12, 21, 0.1),
    ] },
    // Limiter
    Studio { class: Guid::parse("4D583241-CEE4-D900-024A-0007E95D36FA"), kind: EffectKind::StudioLimiter, fields: &[
        (0, 3, 0.5), (1, 4, 0.875), (2, 5, 0.6), (3, 6, 0.049), (4, 7, 0.5), (5, 20, 0.1), (6, 21, 0.5), (7, 22, 0.5), (8, 23, 0.5), (9, 24, 0.5), (10, 25, 0.2), (11, 26, 0.5),
    ] },
    // Phaser
    Studio { class: Guid::parse("4D583232-8725-70EB-024A-0007E95D36FA"), kind: EffectKind::Phaser, fields: &[
        (0, 1, 0.5), (1, 3, 0.5), (2, 4, 0.5), (3, 5, 0.5), (4, 6, 0.5), (5, 7, 0.25), (6, 8, 0.5), (7, 20, 0.2), (8, 9, 0.5),
    ] },
    // Studio Reverb
    Studio { class: Guid::parse("4D583233-BDF1-E94D-024A-0007E95D36FA"), kind: EffectKind::StudioReverb, fields: &[
        (0, 1, 0.5), (1, 3, 0.5), (2, 4, 0.5), (3, 5, 0.5), (4, 6, 0.5), (5, 7, 0.25), (6, 8, 0.5), (7, 20, 0.15), (8, 21, 0.5), (9, 22, 0.25), (10, 9, 0.5),
    ] },
    // Stereo Delay
    Studio { class: Guid::parse("4D583234-D195-D4C0-024A-0007E95D36FA"), kind: EffectKind::StereoDelay, fields: &[
        (0, 1, 0.5), (1, 3, 0.5), (2, 4, 0.5), (3, 5, 0.04545), (4, 6, 0.04545), (5, 10, 0.5), (8, 8, 0.5), (9, 7, 0.5), (10, 20, 0.2), (11, 9, 0.5), (12, 22, 0.5), (13, 23, 0.5), (14, 24, 0.25), (15, 21, 0.3333),
    ] },
    // Tremolo Pan
    Studio { class: Guid::parse("4D583332-97F3-37C3-024A-0007E95D36FA"), kind: EffectKind::TremoloPan, fields: &[
        (0, 3, 0.5), (1, 4, 0.5), (2, 5, 0.00214), (3, 6, 0.5), (4, 7, 0.25), (5, 8, 0.25),
    ] },
    // Tube Stage
    Studio { class: Guid::parse("4D583242-9D12-E479-024A-0007E95D36FA"), kind: EffectKind::TubeStage, fields: &[
        (0, 3, 0.5), (1, 4, 0.5), (2, 5, 0.5), (3, 6, 0.5), (4, 7, 0.5), (5, 10, 0.5), (6, 15, 0.5), (7, 16, 0.5),
    ] },
    // Vocal Strip
    Studio { class: Guid::parse("4D583330-F3CB-88E9-024A-0007E95D36FA"), kind: EffectKind::VocalStrip, fields: &[
        (0, 3, 0.5), (1, 4, 0.5), (2, 5, 0.1667), (3, 6, 0.7333), (4, 7, 0.5), (5, 8, 0.5), (6, 9, 0.5), (7, 20, 0.5), (8, 22, 0.25), (9, 23, 0.25), (10, 21, 0.5),
    ] },
];

/// The Modern Equalizer's class.
const MODERN_EQ: Guid = Guid::parse("F6A7FB77-7DD2-857C-034A-0007E95D36FA");

/// A studio effect's parameter from its normalized value: a straight line
/// bent so the value's middle lands at `middle` of the range.
fn studio_value(n: f32, middle: f32, min: f32, max: f32) -> f32 {
    let below = (1.0 - 1.0 / middle) * (n - 1.0) + n;
    let share = if below.abs() < 1e-9 { 0.0 } else { n / below };
    min + (max - min) * share
}

/// The note nearest a delay of `ms`, at the tempo the plug-in's default
/// program was made for.
fn nearest_note(ms: f32) -> f32 {
    let tempo = crate::Tempo::default();
    (0..crate::audio_fx::tempo::NOTES.len())
        .map(|n| n as f32)
        .min_by(|a, b| {
            let off = |note: f32| {
                (crate::audio_fx::tempo::seconds(note, &tempo) * 1000.0 - f64::from(ms)).abs()
            };
            off(*a).total_cmp(&off(*b))
        })
        .unwrap_or(crate::audio_fx::tempo::QUARTER)
}

/// A studio effect's program read into its parameters (`CcnK`, its size,
/// `FxCk`, two versions and the id, how many values at 24, the program's
/// name, the values at 56). Left as it is if the program keeps something
/// else (a whole bank, or the plug-in's own data).
fn studio(s: &Studio, program: &[u8]) -> Effect {
    let mut e = Effect::new(s.kind);
    if program.get(..4) != Some(b"CcnK") || program.get(8..12) != Some(b"FxCk") {
        return e;
    }
    let count = program
        .get(24..28)
        .and_then(|b| b.try_into().ok())
        .map_or(0, u32::from_be_bytes) as usize;
    let value = |k: usize| -> Option<f32> {
        if k >= count {
            return None;
        }
        let at = 56 + 4 * k;
        Some(f32::from_be_bytes(
            program.get(at..at + 4)?.try_into().ok()?,
        ))
    };
    let specs = s.kind.params();
    for &(param, place, middle) in s.fields {
        let (Some(n), Some(spec)) = (value(place as usize), specs.get(param as usize)) else {
            continue;
        };
        // The drive goes down as far as up in the plug-in, BananaV's from
        // nothing.
        let (min, max) = match (s.kind, param) {
            (EffectKind::TubeStage, 3) => (-spec.max, spec.max),
            _ => (spec.min, spec.max),
        };
        let v = studio_value(n, middle, min, max);
        if v.is_finite() {
            e.set_param(param as usize, v.clamp(spec.min, spec.max));
        }
    }
    if s.kind == EffectKind::StereoDelay {
        // The delays' notes are not kept: the nearest to their times.
        e.set_param(6, nearest_note(e.params[3]));
        e.set_param(7, nearest_note(e.params[4]));
    }
    if let Some(on) = value(0) {
        e.enabled = on >= 0.5;
    }
    e
}

/// A graph's points into its parameters: how many at `at`, the pairs
/// after it, those it does not use left as they are.
fn graph(e: &mut Effect, at: usize, room: usize, points: impl Iterator<Item = (f32, f32)>) {
    let mut count = 0;
    for (n, (x, y)) in points.take(room).enumerate() {
        e.set_param(at + 1 + 2 * n, x);
        e.set_param(at + 2 + 2 * n, y);
        count = n + 1;
    }
    e.set_param(at, count as f32);
}

/// The graphic EQ's graph: positions 0 to 10000 on a log axis of about
/// 17.84 Hz to 22.3 kHz.
fn eq_position_hz(p: f32) -> f32 {
    (17.84 * 1250f32.powf(p / 10_000.0)).clamp(20.0, 20_000.0)
}

/// The graphic EQ's graph: levels where 0 is 0 dB, 202 is +6 dB and −500
/// the bottom (silence), as (a^0.49 − 1)·500 of the amplitude a.
fn eq_level_db(v: f32) -> f32 {
    let a = v / 500.0 + 1.0;
    if a <= 0.0 {
        return -60.0;
    }
    (20.0 * a.powf(1.0 / 0.49).log10()).max(-60.0)
}

/// The records of the plug-ins with graphs, in their fields.
fn graphs(e: &mut Effect, record: &[u8]) {
    let r = |k: usize| -> f32 {
        record
            .get(4 * k..4 * k + 4)
            .and_then(|b| b.try_into().ok())
            .map_or(0.0, |b| i32::from_le_bytes(b) as f32)
    };
    let put = |e: &mut Effect, param: usize, v: f32| {
        let spec = e.kind.params().get(param).copied();
        if let (Some(spec), true) = (spec, v.is_finite()) {
            e.set_param(param, v.clamp(spec.min, spec.max));
        }
    };
    let count = |k: usize| r(k).max(0.0) as usize;
    match e.kind {
        EffectKind::Vibrato => {
            graph(
                e,
                3,
                32,
                (0..count(4)).map(|k| (r(5 + 2 * k), r(6 + 2 * k))),
            );
            put(e, 0, r(1) / 100.0);
            put(e, 1, r(2) / 1000.0);
            put(e, 2, r(3) / 10.0);
            put(e, 68, r(0));
        }
        EffectKind::AmplitudeModulation => {
            graph(
                e,
                3,
                32,
                (0..count(8)).map(|k| (r(9 + 2 * k), r(10 + 2 * k))),
            );
            put(e, 0, r(7) / 10.0);
            put(e, 1, r(6) / 10.0);
            put(e, 2, r(4) / 10.0);
            put(e, 68, r(3));
            put(e, 69, r(5) / 1000.0);
            put(e, 70, r(2) / 100.0);
            put(e, 71, r(1));
            put(e, 72, r(0));
            put(e, 73, r(76));
            put(e, 74, (r(75) / 1000.0).max(0.001));
            put(e, 75, r(74));
        }
        EffectKind::GraphicDynamics => {
            let points = (0..count(37)).map(|k| (r(5 + 2 * k) / 10.0, r(6 + 2 * k) / 10.0));
            graph(e, 0, 16, points);
            put(e, 33, r(1) / 10.0);
            put(e, 34, r(2) / 10.0);
            put(e, 35, r(0) / 10.0);
            put(e, 36, r(3));
            put(e, 37, r(4));
        }
        EffectKind::Distortion => {
            let half = |from: usize, n: usize| {
                let r = &r;
                (0..n).map(move |k| (r(from + 2 * k) / 10.0, r(from + 1 + 2 * k) / 10.0))
            };
            graph(e, 2, 32, half(9, count(8)));
            graph(e, 67, 32, half(74, count(73)));
            put(e, 0, r(2) / 10.0);
            put(e, 1, r(3) / 10.0);
            put(e, 132, r(4));
            put(e, 133, r(5));
            put(e, 134, r(0));
            put(e, 135, r(1));
        }
        EffectKind::GraphicEq => {
            let points = (0..count(149))
                .map(|k| (eq_position_hz(r(21 + 2 * k)), eq_level_db(r(22 + 2 * k))));
            graph(e, 0, 32, points);
            put(e, 65, r(150) / 10.0);
            put(e, 66, r(0));
        }
        _ => {}
    }
}

/// A DirectX plug-in's record read into its parameters.
fn directx(p: &Plugin, record: &[u8]) -> Effect {
    let word = |at: u16, n: usize| record.get(at as usize..at as usize + n);
    let specs = p.kind.params();
    let mut e = Effect::new(p.kind);
    for fl in p.fields {
        let Some(spec) = specs.get(fl.param as usize) else {
            continue;
        };
        let raw = match fl.number {
            Number::Whole => word(fl.at, 4)
                .and_then(|b| b.try_into().ok())
                .map(|b| i32::from_le_bytes(b) as f32),
            Number::Point => word(fl.at, 4)
                .and_then(|b| b.try_into().ok())
                .map(f32::from_le_bytes),
            Number::Double => word(fl.at, 8)
                .and_then(|b| b.try_into().ok())
                .map(|b| f64::from_le_bytes(b) as f32),
        };
        let Some(raw) = raw else {
            continue;
        };
        let v = raw / fl.div * fl.mul + fl.add;
        if v.is_finite() {
            e.set_param(fl.param as usize, v.clamp(spec.min, spec.max));
        }
    }
    if p.fields.is_empty() {
        graphs(&mut e, record);
    }
    if let Some(on) = p
        .bypass
        .and_then(|at| word(at, 4))
        .and_then(|b| b.try_into().ok())
    {
        e.enabled = i32::from_le_bytes(on) == 0;
    }
    e
}

/// The letters of the Modern Equalizer's state, a number each.
const STATE_LETTERS: &[u8; 64] =
    b".ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+";

/// The state's bytes, kept as letters of 6 bits each, the lowest first.
fn state_bytes(size: usize, letters: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; size.min(1 << 24)];
    let mut bit = 0usize;
    for c in letters {
        let Some(v) = STATE_LETTERS.iter().position(|x| x == c) else {
            continue;
        };
        for k in 0..6 {
            if (v >> k) & 1 != 0
                && let Some(b) = out.get_mut(bit >> 3)
            {
                *b |= 1 << (bit & 7);
            }
            bit += 1;
        }
    }
    out
}

/// A value of the state's text: `<param_{name} val="…"/>`.
fn state_value<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let at = text.find(&format!("<param_{name} val=\""))? + name.len() + 13;
    let rest = text.get(at..)?;
    rest.get(..rest.find('"')?)
}

/// The file's band type as BananaV's type and slope.
fn eq_band_type(t: i32) -> Option<(f32, f32)> {
    Some(match t {
        0 => (1.0, 6.0),
        1 => (1.0, 12.0),
        2 => (1.0, 24.0),
        3 => (1.0, 36.0),
        4 => (2.0, 6.0),
        5 => (2.0, 12.0),
        6 => (5.0, 12.0),
        7 => (4.0, 6.0),
        8 => (4.0, 12.0),
        9 => (4.0, 18.0),
        10 => (6.0, 6.0),
        11 => (3.0, 6.0),
        12 => (3.0, 12.0),
        13 => (0.0, 6.0),
        14 => (0.0, 12.0),
        15 => (0.0, 24.0),
        16 => (0.0, 36.0),
        _ => return None,
    })
}

/// The Modern Equalizer's settings: `VC2!`, the size, and its state as a
/// text that holds, between `<IComponent>`s, the state's bytes as
/// `size.letters`; the bytes being `VC2!`, the size, and the values.
fn modern_eq(settings: &[u8]) -> Effect {
    use crate::fx::modern_eq as m;
    let mut e = Effect::new(EffectKind::ModernEq);
    let Some(body) = settings.get(8..) else {
        return e;
    };
    let text = String::from_utf8_lossy(body);
    let inner = (|| {
        let from = text.find("<IComponent>")? + "<IComponent>".len();
        let rest = &text[from..];
        let rest = &rest[..rest.find("</IComponent>")?];
        let dot = rest.find('.')?;
        let size: usize = rest[..dot].trim().parse().ok()?;
        let bytes = state_bytes(size, &rest.as_bytes()[dot + 1..]);
        Some(String::from_utf8_lossy(&bytes).into_owned())
    })();
    let Some(state) = inner else {
        return e;
    };
    let number = |name: &str| -> Option<f32> { state_value(&state, name)?.parse().ok() };
    let switch = |name: &str| -> Option<bool> { Some(state_value(&state, name)? == "true") };
    let specs = EffectKind::ModernEq.params();
    let put = |e: &mut Effect, param: usize, v: Option<f32>| {
        if let (Some(v), Some(spec)) = (v.filter(|v| v.is_finite()), specs.get(param)) {
            e.set_param(param, v.clamp(spec.min, spec.max));
        }
    };
    put(
        &mut e,
        m::GLOBAL_SCALE,
        number("globalScale").map(|v| v * 100.0),
    );
    put(&mut e, m::MONO_BELOW, number("globalMonoBelowFreq"));
    put(
        &mut e,
        m::STEREO_WIDTH,
        number("globalStereoWidth").map(|v| v * 100.0),
    );
    put(&mut e, m::OUTPUT_GAIN, number("globalGain"));
    put(&mut e, m::OVERSAMPLING, number("oversampling"));
    for b in 0..m::BANDS {
        let band = |p: &str| format!("b{b:02}_{p}");
        let on = |v: bool| f32::from(u8::from(v));
        put(&mut e, m::at(b, m::USED), switch(&band("enabled")).map(on));
        put(
            &mut e,
            m::at(b, m::ON),
            switch(&band("bypassed")).map(|v| on(!v)),
        );
        if let Some((kind, slope)) = number(&band("type")).and_then(|t| eq_band_type(t as i32)) {
            put(&mut e, m::at(b, m::TYPE), Some(kind));
            put(&mut e, m::at(b, m::SLOPE), Some(slope));
        }
        put(&mut e, m::at(b, m::FREQUENCY), number(&band("freq")));
        put(&mut e, m::at(b, m::GAIN), number(&band("gain")));
        put(&mut e, m::at(b, m::Q), number(&band("q")));
        put(&mut e, m::at(b, m::ROUTING), number(&band("mode")));
        put(&mut e, m::model(b), number(&band("model")));
    }
    if switch("bypass") == Some(true) {
        e.enabled = false;
    }
    e
}

/// The last six bytes of the classes the other editor makes for VST
/// plug-ins.
const VST_FAMILY: [u8; 6] = [0x00, 0x07, 0xE9, 0x5D, 0x36, 0xFA];
/// Their kind byte: 2 for a VST 2 plug-in, 3 for a VST 3 one.
const VST3_BIT: u8 = 0x01;

/// The bytes of the `N.letters` between `<tag>` and `</tag>` in `text`.
fn tagged_state(text: &str, tag: &str) -> Option<Vec<u8>> {
    let open = format!("<{tag}>");
    let from = text.find(&open)? + open.len();
    let rest = &text[from..];
    let rest = &rest[..rest.find(&format!("</{tag}>"))?];
    let dot = rest.find('.')?;
    let size: usize = rest[..dot].trim().parse().ok()?;
    Some(state_bytes(size, &rest.as_bytes()[dot + 1..]))
}

/// A VST 3 plug-in of a chain: BananaV's VST 3 effect with its states —
/// the component's and the controller's, kept as text of 6-bit letters
/// in the settings (`VC2!`, the text's size, the text) — its
/// plug-in to be found among those installed by what its class says
/// ([`crate::VstWanted`]). `None` for a class of no VST 3 plug-in.
pub(super) fn vst3(class: Guid, name: &str, settings: &[u8]) -> Option<Effect> {
    let g = class.0;
    if g[10..16] != VST_FAMILY || g[8] & VST3_BIT == 0 {
        return None;
    }
    let wanted = wanted(class);
    // `VC2!`, the text's size, the text: first (the chain's record keeps
    // the program before them), or after the program.
    let mark = [0, 4]
        .into_iter()
        .find(|&at| settings.get(at..at + 4) == Some(b"VC2!".as_slice()));
    let text = mark
        .and_then(|at| {
            let size = u32::from_le_bytes(settings.get(at + 4..at + 8)?.try_into().ok()?) as usize;
            let from = at + 8;
            let body = settings.get(from..from.checked_add(size)?.min(settings.len()))?;
            Some(String::from_utf8_lossy(body).into_owned())
        })
        .unwrap_or_default();
    Some(Effect::vst(crate::fx::VstFx {
        name: shown_name(name),
        state: tagged_state(&text, "IComponent").unwrap_or_default(),
        controller_state: tagged_state(&text, "IEditController").unwrap_or_default(),
        wanted: Some(wanted),
        ..crate::fx::VstFx::default()
    }))
}

/// A plug-in's name as listed there: before the kind's note
/// ("Name\t(VST3, 64 Bit)").
fn shown_name(name: &str) -> String {
    let shown = name.split('\t').next().unwrap_or(name).trim();
    shown
        .strip_suffix(")")
        .and_then(|s| s.rsplit_once(" (VST"))
        .map_or(shown, |(n, _)| n.trim())
        .to_string()
}

/// What the class says of a VST plug-in: the number it makes, the checks
/// of the plug-in's file's name and of its name.
fn wanted(class: Guid) -> crate::VstWanted {
    let g = class.0;
    crate::VstWanted {
        number: u32::from_le_bytes([g[0], g[1], g[2], g[3]]),
        file: u16::from_le_bytes([g[4], g[5]]),
        name: u16::from_le_bytes([g[6], g[7]]),
    }
}

/// A VST 2 plug-in of a chain that BananaV has no effect of its own for:
/// BananaV's VST 2 effect with its program — the preset file the settings
/// keep ("CcnK"…) —, its plug-in to be found among those installed by its
/// own number ([`crate::VstWanted`]). `None` for a class of no VST 2
/// plug-in.
pub(super) fn vst2(class: Guid, name: &str, settings: &[u8]) -> Option<Effect> {
    let g = class.0;
    if g[10..16] != VST_FAMILY || g[8] & VST3_BIT != 0 {
        return None;
    }
    // The preset file: first, or after the program.
    let state = [0, 4]
        .into_iter()
        .find(|&at| settings.get(at..at + 4) == Some(b"CcnK".as_slice()))
        .and_then(|at| {
            let size = i32::from_be_bytes(settings.get(at + 4..at + 8)?.try_into().ok()?);
            let end = at
                .checked_add(8)?
                .checked_add(usize::try_from(size).ok()?)?;
            Some(settings.get(at..end.min(settings.len()))?.to_vec())
        })
        .unwrap_or_default();
    Some(Effect::vst(crate::fx::VstFx {
        format: crate::fx::VstFormat::Vst2,
        name: shown_name(name),
        state,
        wanted: Some(wanted(class)),
        ..crate::fx::VstFx::default()
    }))
}

/// BananaV's effect for an audio plug-in of a chain, its settings (from
/// their format's GUID or their program's mark) read into its parameters;
/// `None` for a plug-in BananaV does not have.
pub(super) fn effect(class: Guid, settings: &[u8]) -> Option<Effect> {
    if let Some(s) = STUDIO.iter().find(|s| s.class == class) {
        return Some(studio(s, settings));
    }
    if class == MODERN_EQ {
        return Some(modern_eq(settings));
    }
    let p = PLUGINS.iter().find(|p| p.class == class)?;
    Some(directx(p, settings.get(16..).unwrap_or(&[])))
}
