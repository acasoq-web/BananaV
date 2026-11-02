//! File › Properties › Video's color management, as VEGAS's: ACES — the
//! space video is composited in, the view frames are shown and rendered
//! through, a look — on BananaV's own OpenColorIO configuration
//! (`bv_ocio::builtin`). It works with 32-bit floating point (full range)
//! and a view transform; then video composites in linear light.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};

/// ACES's versions ("ACES version"). Their SDR outputs and the Blue Light
/// Artifact Fix are the same (ACES 1.3 added the Reference Gamut
/// Compression and cameras): VEGAS 23 shows 1.2 by default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AcesVersion {
    #[default]
    V1_2,
    V1_3,
}

impl AcesVersion {
    pub const ALL: [AcesVersion; 2] = [AcesVersion::V1_2, AcesVersion::V1_3];

    pub fn name(self) -> &'static str {
        match self {
            AcesVersion::V1_2 => "1.2",
            AcesVersion::V1_3 => "1.3",
        }
    }
}

/// The ACES spaces video can be composited in ("ACES color space").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AcesSpace {
    /// ACES2065-1: AP0 primaries, VEGAS's default.
    #[default]
    Aces2065_1,
    /// ACEScg: AP1 primaries, made for rendering and compositing.
    AcesCg,
}

impl AcesSpace {
    pub const ALL: [AcesSpace; 2] = [AcesSpace::Aces2065_1, AcesSpace::AcesCg];

    /// Its name in the configuration.
    pub fn name(self) -> &'static str {
        match self {
            AcesSpace::Aces2065_1 => "ACES2065-1",
            AcesSpace::AcesCg => "ACEScg",
        }
    }
}

/// The displays of BananaV's configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AcesDisplay {
    Srgb,
    Rec709,
    Rec2020,
    DisplayP3,
    P3D65,
    P3Dci,
}

impl AcesDisplay {
    pub const ALL: [AcesDisplay; 6] = [
        AcesDisplay::Srgb,
        AcesDisplay::Rec709,
        AcesDisplay::Rec2020,
        AcesDisplay::DisplayP3,
        AcesDisplay::P3D65,
        AcesDisplay::P3Dci,
    ];

    /// Its name in the configuration (what the interface shows).
    pub fn name(self) -> &'static str {
        match self {
            AcesDisplay::Srgb => "sRGB",
            AcesDisplay::Rec709 => "Rec.709",
            AcesDisplay::Rec2020 => "Rec.2020",
            AcesDisplay::DisplayP3 => "Display P3",
            AcesDisplay::P3D65 => "P3-D65",
            AcesDisplay::P3Dci => "P3-DCI",
        }
    }
}

/// A display's views.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AcesView {
    /// ACES 1.0's output for the display: video's (dim surround) or
    /// cinema's (dark surround).
    Aces,
    /// The scene's light as it is, through the display's curve.
    UnToneMapped,
    /// The working space's values as they are.
    Raw,
}

impl AcesView {
    pub const ALL: [AcesView; 3] = [AcesView::Aces, AcesView::UnToneMapped, AcesView::Raw];

    /// Its name in the configuration.
    pub fn name(self) -> &'static str {
        match self {
            AcesView::Aces => "ACES",
            AcesView::UnToneMapped => "Un-tone-mapped",
            AcesView::Raw => "Raw",
        }
    }
}

/// The view transform: a display and one of its views.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ViewTransform {
    pub display: AcesDisplay,
    pub view: AcesView,
}

/// The look modification transforms (VEGAS's lists: with ACES 1.3 None
/// and the Reference Gamut Compression; with 1.2 the Blue Light Artifact
/// Fix).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AcesLook {
    #[default]
    None,
    /// ACES 1.1's Blue Light Artifact Fix: bright blues (sirens, LEDs)
    /// desaturated, so they neither clip nor shift hue.
    BlueLightFix,
    /// ACES 1.3's Reference Gamut Compression: colors outside AP1 (the
    /// strongest of cameras and lights) brought inside it.
    GamutCompression,
}

impl AcesLook {
    pub const ALL: [AcesLook; 3] = [
        AcesLook::None,
        AcesLook::BlueLightFix,
        AcesLook::GamutCompression,
    ];

    /// Its name in the configuration (`None`: no look).
    pub fn name(self) -> Option<&'static str> {
        match self {
            AcesLook::None => None,
            AcesLook::BlueLightFix => Some("Blue Light Artifact Fix"),
            AcesLook::GamutCompression => Some("ACES 1.3 Reference Gamut Compression"),
        }
    }
}

/// The color spaces a media's Properties and a render template's Project
/// tab offer under ACES, in VEGAS's order (OpenColorIO's ACES studio
/// configuration's ACES and camera spaces); `None` beside one of them is
/// VEGAS's Default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColorSpace {
    Aces2065_1,
    AcesCc,
    AcesCct,
    AcesCg,
    Adx10,
    Adx16,
    LinearArriWideGamut3,
    ArriLogC3Ei800,
    LinearArriWideGamut4,
    ArriLogC4,
    BmdFilmWideGamutGen5,
    DavinciIntermediateWideGamut,
    LinearBmdWideGamutGen5,
    LinearDavinciWideGamut,
    CanonLog2CinemaGamutD55,
    CanonLog3CinemaGamutD55,
    LinearCinemaGamutD55,
    LinearVGamut,
    VLogVGamut,
    LinearRedWideGamutRgb,
    Log3G10RedWideGamutRgb,
    LinearSGamut3,
    LinearSGamut3Cine,
    LinearVeniceSGamut3,
    LinearVeniceSGamut3Cine,
    SLog3SGamut3,
    SLog3SGamut3Cine,
    SLog3VeniceSGamut3,
    SLog3VeniceSGamut3Cine,
}

impl ColorSpace {
    pub const ALL: [ColorSpace; 29] = {
        use ColorSpace::*;
        [
            Aces2065_1,
            AcesCc,
            AcesCct,
            AcesCg,
            Adx10,
            Adx16,
            LinearArriWideGamut3,
            ArriLogC3Ei800,
            LinearArriWideGamut4,
            ArriLogC4,
            BmdFilmWideGamutGen5,
            DavinciIntermediateWideGamut,
            LinearBmdWideGamutGen5,
            LinearDavinciWideGamut,
            CanonLog2CinemaGamutD55,
            CanonLog3CinemaGamutD55,
            LinearCinemaGamutD55,
            LinearVGamut,
            VLogVGamut,
            LinearRedWideGamutRgb,
            Log3G10RedWideGamutRgb,
            LinearSGamut3,
            LinearSGamut3Cine,
            LinearVeniceSGamut3,
            LinearVeniceSGamut3Cine,
            SLog3SGamut3,
            SLog3SGamut3Cine,
            SLog3VeniceSGamut3,
            SLog3VeniceSGamut3Cine,
        ]
    };

    /// Its name in the configuration (what the lists show).
    pub fn name(self) -> &'static str {
        use ColorSpace::*;
        match self {
            Aces2065_1 => "ACES2065-1",
            AcesCc => "ACEScc",
            AcesCct => "ACEScct",
            AcesCg => "ACEScg",
            Adx10 => "ADX10",
            Adx16 => "ADX16",
            LinearArriWideGamut3 => "Linear ARRI Wide Gamut 3",
            ArriLogC3Ei800 => "ARRI LogC3 (EI800)",
            LinearArriWideGamut4 => "Linear ARRI Wide Gamut 4",
            ArriLogC4 => "ARRI LogC4",
            BmdFilmWideGamutGen5 => "BMDFilm WideGamut Gen5",
            DavinciIntermediateWideGamut => "DaVinci Intermediate WideGamut",
            LinearBmdWideGamutGen5 => "Linear BMD WideGamut Gen5",
            LinearDavinciWideGamut => "Linear DaVinci WideGamut",
            CanonLog2CinemaGamutD55 => "CanonLog2 CinemaGamut D55",
            CanonLog3CinemaGamutD55 => "CanonLog3 CinemaGamut D55",
            LinearCinemaGamutD55 => "Linear CinemaGamut D55",
            LinearVGamut => "Linear V-Gamut",
            VLogVGamut => "V-Log V-Gamut",
            LinearRedWideGamutRgb => "Linear REDWideGamutRGB",
            Log3G10RedWideGamutRgb => "Log3G10 REDWideGamutRGB",
            LinearSGamut3 => "Linear S-Gamut3",
            LinearSGamut3Cine => "Linear S-Gamut3.Cine",
            LinearVeniceSGamut3 => "Linear Venice S-Gamut3",
            LinearVeniceSGamut3Cine => "Linear Venice S-Gamut3.Cine",
            SLog3SGamut3 => "S-Log3 S-Gamut3",
            SLog3SGamut3Cine => "S-Log3 S-Gamut3.Cine",
            SLog3VeniceSGamut3 => "S-Log3 Venice S-Gamut3",
            SLog3VeniceSGamut3Cine => "S-Log3 Venice S-Gamut3.Cine",
        }
    }

    /// Whether BananaV converts it (the Academy Density Exchange encodings
    /// of film scans are still to come).
    pub fn drawn(self) -> bool {
        !matches!(self, ColorSpace::Adx10 | ColorSpace::Adx16)
    }
}

/// ACES as File › Properties › Video sets it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ColorManagement {
    #[serde(default)]
    pub version: AcesVersion,
    #[serde(default)]
    pub space: AcesSpace,
    /// `None`: Off.
    #[serde(default)]
    pub view: Option<ViewTransform>,
    /// Applied only with a view transform.
    #[serde(default)]
    pub look: AcesLook,
    /// A render's: frames leave in this color space instead of through the
    /// look and the view (a render template's Color space; never saved
    /// with the project).
    #[serde(skip)]
    pub output: Option<ColorSpace>,
}

/// Media's color space under ACES (VEGAS's default: Media Properties'
/// Color space lists it first).
pub const MEDIA_SPACE: &str = "ACES2065-1";

/// What the compositors do with colors under ACES.
#[derive(Debug)]
pub struct ColorPipeline {
    /// The working space.
    pub working: AcesSpace,
    /// Media and the colors picked for generators and fades, linearized
    /// as BananaV reads them (BT.709's curve) and taken as ACES2065-1 —
    /// VEGAS's default media color space under ACES (its help; the
    /// calibration card's colors through VEGAS's sRGB view match to 1–2
    /// of 255 that way, and miss by up to 70 read as Rec. 709) — into the
    /// working space. With the 2.222 compositing gamma their encoded
    /// values are taken so, not linearized, as VEGAS takes them (the
    /// card's grays through its view to half a level).
    pub input: [[f32; 3]; 3],
    /// The working space to the display's code values, through the look
    /// and the view.
    pub view: bv_ocio::Processor,
}

impl ColorManagement {
    /// The pipeline these settings make (`None` without a view transform),
    /// worked out once each.
    pub fn pipeline(&self) -> Option<Arc<ColorPipeline>> {
        let view = self.view?;
        static PIPELINES: OnceLock<Mutex<HashMap<ColorManagement, Arc<ColorPipeline>>>> =
            OnceLock::new();
        let mut known = PIPELINES
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(p) = known.get(self) {
            return Some(Arc::clone(p));
        }
        let config = bv_ocio::builtin();
        let space = self.space.name();
        let (m, _) = config.processor(MEDIA_SPACE, space).ok()?.matrix()?;
        let view = match self.output {
            // A render template's color space: the working space's colors
            // in it, as they are (scene light or a camera's encoding).
            Some(out) => config.processor(space, out.name()).ok()?,
            None => config
                .display_processor(
                    space,
                    view.display.name(),
                    view.view.name(),
                    self.look.name(),
                )
                .ok()?,
        };
        let pipeline = Arc::new(ColorPipeline {
            working: self.space,
            input: m.map(|row| row.map(|v| v as f32)),
            view,
        });
        known.insert(*self, Arc::clone(&pipeline));
        Some(pipeline)
    }
}

impl ColorPipeline {
    /// A color taken as ACES2065-1 (see [`ColorPipeline::input`]) in the
    /// working space.
    pub fn enter(&self, c: [f32; 3]) -> [f32; 3] {
        let m = &self.input;
        [0, 1, 2].map(|i| m[i][0] * c[0] + m[i][1] * c[1] + m[i][2] * c[2])
    }

    /// What takes a media's values in `space` to the working space (see
    /// [`media_input`]).
    pub fn media_input(&self, space: ColorSpace) -> Option<Arc<bv_ocio::Processor>> {
        media_input(space, self.working)
    }
}

/// A media's values in `space` (its file's values, not linearized: a
/// camera's log as it was recorded) to the working space `working`, worked
/// out once each; `None` for a space BananaV does not convert.
pub fn media_input(space: ColorSpace, working: AcesSpace) -> Option<Arc<bv_ocio::Processor>> {
    type Inputs = HashMap<(ColorSpace, AcesSpace), Option<Arc<bv_ocio::Processor>>>;
    static INPUTS: OnceLock<Mutex<Inputs>> = OnceLock::new();
    let mut known = INPUTS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    known
        .entry((space, working))
        .or_insert_with(|| {
            if !space.drawn() {
                return None;
            }
            bv_ocio::builtin()
                .processor(space.name(), working.name())
                .ok()
                .map(Arc::new)
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_setting_makes_a_pipeline() {
        assert!(ColorManagement::default().pipeline().is_none());
        for space in AcesSpace::ALL {
            for display in AcesDisplay::ALL {
                for view in AcesView::ALL {
                    for look in AcesLook::ALL {
                        let cm = ColorManagement {
                            space,
                            view: Some(ViewTransform { display, view }),
                            look,
                            ..ColorManagement::default()
                        };
                        let p = cm.pipeline().unwrap_or_else(|| panic!("{cm:?}"));
                        // White enters as white; grey reaches the display
                        // grey.
                        let w = p.enter([1.0; 3]);
                        assert!(w.iter().all(|v| (v - 1.0).abs() < 1e-5), "{w:?}");
                        let g = p.view.apply(p.enter([0.18; 3]).map(f64::from));
                        assert!(
                            (g[0] - g[1]).abs() < 1e-5 && (g[1] - g[2]).abs() < 1e-5,
                            "{cm:?}: {g:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn every_listed_space_converts_but_adx() {
        for working in AcesSpace::ALL {
            for s in ColorSpace::ALL {
                assert_eq!(
                    media_input(s, working).is_some(),
                    s.drawn(),
                    "{s:?} to {working:?}"
                );
            }
        }
        // A render's frames leave in each.
        let srgb = Some(ViewTransform {
            display: AcesDisplay::Srgb,
            view: AcesView::Aces,
        });
        for s in ColorSpace::ALL.into_iter().filter(|s| s.drawn()) {
            let cm = ColorManagement {
                view: srgb,
                output: Some(s),
                ..ColorManagement::default()
            };
            let p = cm.pipeline().unwrap_or_else(|| panic!("{s:?}"));
            // Grey stays grey, whatever the space's encoding.
            let g = p.view.apply([0.18; 3]);
            assert!(
                (g[0] - g[1]).abs() < 1e-6 && (g[1] - g[2]).abs() < 1e-6,
                "{s:?}: {g:?}"
            );
        }
        // A camera's grey: S-Log3's 420 is 18%.
        let q = media_input(ColorSpace::SLog3SGamut3Cine, AcesSpace::Aces2065_1).unwrap();
        let g = q.apply([420.0 / 1023.0; 3]);
        assert!(g.iter().all(|v| (v - 0.18).abs() < 1e-6), "{g:?}");
    }

    #[test]
    fn the_look_needs_linear_light() {
        use crate::{CompositingGamma, PixelFormat, ProjectSettings};
        let look = ColorManagement {
            view: Some(ViewTransform {
                display: AcesDisplay::Srgb,
                view: AcesView::Aces,
            }),
            look: AcesLook::BlueLightFix,
            ..ColorManagement::default()
        };
        let mut s = ProjectSettings {
            pixel_format: PixelFormat::Float32Full,
            color: look,
            ..ProjectSettings::default()
        };
        assert!(s.look_applies() && !s.composites_in_video_gamma());
        let with_look = s.color_pipeline().unwrap();
        // With the 2.222 gamma ACES composites the encoded values, as
        // VEGAS, which greys the look out there.
        s.compositing_gamma = CompositingGamma::Video;
        assert!(!s.look_applies() && s.composites_in_video_gamma());
        let without = ColorManagement {
            look: AcesLook::None,
            ..look
        };
        let p = s.color_pipeline().unwrap();
        assert!(Arc::ptr_eq(&p, &without.pipeline().unwrap()));
        assert!(!Arc::ptr_eq(&p, &with_look));
    }
}
