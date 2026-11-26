//! The kernels on the processor: written by `gen/gen.rs` from
//! `kernels/*.wgsl` (`BV_BANANA_WRITE=1 cargo test -p bv-banana --test
//! generated`). Never edited by hand.
#![allow(clippy::all, unused)]

use crate::cpu::*;
use crate::math::*;
use crate::graph::Kernel;

/// Each kernel's name, family and twin on the processor.
#[derive(Clone, Copy)]
pub struct Entry {
    pub name: &'static str,
    pub family: usize,
    pub run: fn(&C<'_>, I2) -> V4,
}

/// The kernels' shared part.
pub const PRELUDE: &str = include_str!("../kernels/prelude.wgsl");

/// The families' names and sources.
pub const FAMILIES: [(&str, &str); 9] = [
    ("basic", include_str!("../kernels/basic.wgsl")),
    ("blur", include_str!("../kernels/blur.wgsl")),
    ("color", include_str!("../kernels/color.wgsl")),
    ("distort", include_str!("../kernels/distort.wgsl")),
    ("light", include_str!("../kernels/light.wgsl")),
    ("make", include_str!("../kernels/make.wgsl")),
    ("style", include_str!("../kernels/style.wgsl")),
    ("time", include_str!("../kernels/time.wgsl")),
    ("wipe", include_str!("../kernels/wipe.wgsl")),
];

/// Every kernel.
pub const ENTRIES: [Entry; 189] = [
    Entry { name: "copy", family: 0, run: basic::k_copy },
    Entry { name: "fill", family: 0, run: basic::k_fill },
    Entry { name: "half_x", family: 0, run: basic::k_half_x },
    Entry { name: "half_y", family: 0, run: basic::k_half_y },
    Entry { name: "gauss_x", family: 0, run: basic::k_gauss_x },
    Entry { name: "gauss_y", family: 0, run: basic::k_gauss_y },
    Entry { name: "up", family: 0, run: basic::k_up },
    Entry { name: "mix", family: 0, run: basic::k_mix },
    Entry { name: "add", family: 0, run: basic::k_add },
    Entry { name: "sum3", family: 0, run: basic::k_sum3 },
    Entry { name: "bright", family: 0, run: basic::k_bright },
    Entry { name: "finish", family: 0, run: basic::k_finish },
    Entry { name: "lay", family: 0, run: basic::k_lay },
    Entry { name: "smear", family: 1, run: blur::k_smear },
    Entry { name: "spin", family: 1, run: blur::k_spin },
    Entry { name: "lens", family: 1, run: blur::k_lens },
    Entry { name: "lens_join", family: 1, run: blur::k_lens_join },
    Entry { name: "surface", family: 1, run: blur::k_surface },
    Entry { name: "sharpen", family: 1, run: blur::k_sharpen },
    Entry { name: "chroma_join", family: 1, run: blur::k_chroma_join },
    Entry { name: "channel_join", family: 1, run: blur::k_channel_join },
    Entry { name: "median", family: 1, run: blur::k_median },
    Entry { name: "deband", family: 1, run: blur::k_deband },
    Entry { name: "convolve", family: 1, run: blur::k_convolve },
    Entry { name: "skin", family: 1, run: blur::k_skin },
    Entry { name: "edge_join", family: 1, run: blur::k_edge_join },
    Entry { name: "edges", family: 1, run: blur::k_edges },
    Entry { name: "light_join", family: 1, run: blur::k_light_join },
    Entry { name: "dream", family: 1, run: blur::k_dream },
    Entry { name: "matrix", family: 2, run: color::k_matrix },
    Entry { name: "gamma", family: 2, run: color::k_gamma },
    Entry { name: "hue", family: 2, run: color::k_hue },
    Entry { name: "mono", family: 2, run: color::k_mono },
    Entry { name: "negative", family: 2, run: color::k_negative },
    Entry { name: "tones", family: 2, run: color::k_tones },
    Entry { name: "only_bright", family: 2, run: color::k_only_bright },
    Entry { name: "limit", family: 2, run: color::k_limit },
    Entry { name: "zebra", family: 2, run: color::k_zebra },
    Entry { name: "threshold", family: 2, run: color::k_threshold },
    Entry { name: "split_tint", family: 2, run: color::k_split_tint },
    Entry { name: "posterize", family: 2, run: color::k_posterize },
    Entry { name: "solarize", family: 2, run: color::k_solarize },
    Entry { name: "bleach", family: 2, run: color::k_bleach },
    Entry { name: "false_color", family: 2, run: color::k_false_color },
    Entry { name: "print", family: 2, run: color::k_print },
    Entry { name: "mist", family: 2, run: color::k_mist },
    Entry { name: "fuse", family: 2, run: color::k_fuse },
    Entry { name: "displace", family: 3, run: distort::k_displace },
    Entry { name: "rgb_shift", family: 3, run: distort::k_rgb_shift },
    Entry { name: "turbulence", family: 3, run: distort::k_turbulence },
    Entry { name: "twirl", family: 3, run: distort::k_twirl },
    Entry { name: "fisheye", family: 3, run: distort::k_fisheye },
    Entry { name: "magnify", family: 3, run: distort::k_magnify },
    Entry { name: "ripple", family: 3, run: distort::k_ripple },
    Entry { name: "waves", family: 3, run: distort::k_waves },
    Entry { name: "polar", family: 3, run: distort::k_polar },
    Entry { name: "plane", family: 3, run: distort::k_plane },
    Entry { name: "kaleido", family: 3, run: distort::k_kaleido },
    Entry { name: "facets", family: 3, run: distort::k_facets },
    Entry { name: "tiles", family: 3, run: distort::k_tiles },
    Entry { name: "strips", family: 3, run: distort::k_strips },
    Entry { name: "endless", family: 3, run: distort::k_endless },
    Entry { name: "echoes", family: 3, run: distort::k_echoes },
    Entry { name: "drops", family: 3, run: distort::k_drops },
    Entry { name: "glass", family: 3, run: distort::k_glass },
    Entry { name: "rainbow", family: 3, run: distort::k_rainbow },
    Entry { name: "stretch_edges", family: 3, run: distort::k_stretch_edges },
    Entry { name: "prism", family: 3, run: distort::k_prism },
    Entry { name: "glow", family: 4, run: light::k_glow },
    Entry { name: "glow_two", family: 4, run: light::k_glow_two },
    Entry { name: "glow_rainbow", family: 4, run: light::k_glow_rainbow },
    Entry { name: "glow_rings", family: 4, run: light::k_glow_rings },
    Entry { name: "glow_dark", family: 4, run: light::k_glow_dark },
    Entry { name: "glow_smoke", family: 4, run: light::k_glow_smoke },
    Entry { name: "halation", family: 4, run: light::k_halation },
    Entry { name: "glow_tube", family: 4, run: light::k_glow_tube },
    Entry { name: "light_fade", family: 4, run: light::k_light_fade },
    Entry { name: "neon", family: 4, run: light::k_neon },
    Entry { name: "points", family: 4, run: light::k_points },
    Entry { name: "star", family: 4, run: light::k_star },
    Entry { name: "rays", family: 4, run: light::k_rays },
    Entry { name: "find_light", family: 4, run: light::k_find_light },
    Entry { name: "flare", family: 4, run: light::k_flare },
    Entry { name: "spot", family: 4, run: light::k_spot },
    Entry { name: "relight", family: 4, run: light::k_relight },
    Entry { name: "shadow", family: 4, run: light::k_shadow },
    Entry { name: "leak", family: 4, run: light::k_leak },
    Entry { name: "bokeh", family: 4, run: light::k_bokeh },
    Entry { name: "flashes", family: 4, run: light::k_flashes },
    Entry { name: "rim", family: 4, run: light::k_rim },
    Entry { name: "clouds", family: 5, run: make::k_clouds },
    Entry { name: "clouds_color", family: 5, run: make::k_clouds_color },
    Entry { name: "nebula", family: 5, run: make::k_nebula },
    Entry { name: "cloud_sky", family: 5, run: make::k_cloud_sky },
    Entry { name: "clouds_hue", family: 5, run: make::k_clouds_hue },
    Entry { name: "cloud_storm", family: 5, run: make::k_cloud_storm },
    Entry { name: "aurora", family: 5, run: make::k_aurora },
    Entry { name: "caustics", family: 5, run: make::k_caustics },
    Entry { name: "grid", family: 5, run: make::k_grid },
    Entry { name: "grunge", family: 5, run: make::k_grunge },
    Entry { name: "dust", family: 5, run: make::k_dust },
    Entry { name: "gradient", family: 5, run: make::k_gradient },
    Entry { name: "moon", family: 5, run: make::k_moon },
    Entry { name: "stars", family: 5, run: make::k_stars },
    Entry { name: "shape", family: 5, run: make::k_shape },
    Entry { name: "sparkles", family: 5, run: make::k_sparkles },
    Entry { name: "cells", family: 5, run: make::k_cells },
    Entry { name: "spiral", family: 5, run: make::k_spiral },
    Entry { name: "flux", family: 5, run: make::k_flux },
    Entry { name: "silk", family: 5, run: make::k_silk },
    Entry { name: "contours", family: 5, run: make::k_contours },
    Entry { name: "fibers", family: 5, run: make::k_fibers },
    Entry { name: "moire", family: 5, run: make::k_moire },
    Entry { name: "nerves", family: 5, run: make::k_nerves },
    Entry { name: "relief", family: 5, run: make::k_relief },
    Entry { name: "paint", family: 5, run: make::k_paint },
    Entry { name: "plasma", family: 5, run: make::k_plasma },
    Entry { name: "veins", family: 5, run: make::k_veins },
    Entry { name: "spots", family: 5, run: make::k_spots },
    Entry { name: "tiling", family: 5, run: make::k_tiling },
    Entry { name: "weave", family: 5, run: make::k_weave },
    Entry { name: "beam", family: 5, run: make::k_beam },
    Entry { name: "muzzle", family: 5, run: make::k_muzzle },
    Entry { name: "bolt", family: 5, run: make::k_bolt },
    Entry { name: "bolt_light", family: 5, run: make::k_bolt_light },
    Entry { name: "arc_edges", family: 5, run: make::k_arc_edges },
    Entry { name: "fall", family: 5, run: make::k_fall },
    Entry { name: "oil", family: 6, run: style::k_oil },
    Entry { name: "impasto", family: 6, run: style::k_impasto },
    Entry { name: "cartoon", family: 6, run: style::k_cartoon },
    Entry { name: "outline", family: 6, run: style::k_outline },
    Entry { name: "ink", family: 6, run: style::k_ink },
    Entry { name: "hatch", family: 6, run: style::k_hatch },
    Entry { name: "halftone", family: 6, run: style::k_halftone },
    Entry { name: "mosaic", family: 6, run: style::k_mosaic },
    Entry { name: "scatter", family: 6, run: style::k_scatter },
    Entry { name: "glitch", family: 6, run: style::k_glitch },
    Entry { name: "dct_rows", family: 6, run: style::k_dct_rows },
    Entry { name: "dct_columns", family: 6, run: style::k_dct_columns },
    Entry { name: "idct_columns", family: 6, run: style::k_idct_columns },
    Entry { name: "idct_rows", family: 6, run: style::k_idct_rows },
    Entry { name: "mosh", family: 6, run: style::k_mosh },
    Entry { name: "sort", family: 6, run: style::k_sort },
    Entry { name: "tape", family: 6, run: style::k_tape },
    Entry { name: "aerial", family: 6, run: style::k_aerial },
    Entry { name: "tube", family: 6, run: style::k_tube },
    Entry { name: "film", family: 6, run: style::k_film },
    Entry { name: "grain", family: 6, run: style::k_grain },
    Entry { name: "vignette", family: 6, run: style::k_vignette },
    Entry { name: "emboss", family: 6, run: style::k_emboss },
    Entry { name: "band", family: 6, run: style::k_band },
    Entry { name: "psyche", family: 6, run: style::k_psyche },
    Entry { name: "zebra_light", family: 6, run: style::k_zebra_light },
    Entry { name: "pencil", family: 6, run: style::k_pencil },
    Entry { name: "wash", family: 6, run: style::k_wash },
    Entry { name: "lamps", family: 6, run: style::k_lamps },
    Entry { name: "misprint", family: 6, run: style::k_misprint },
    Entry { name: "frames", family: 7, run: time::k_frames },
    Entry { name: "trail", family: 7, run: time::k_trail },
    Entry { name: "drift", family: 7, run: time::k_drift },
    Entry { name: "rgb_time", family: 7, run: time::k_rgb_time },
    Entry { name: "slit", family: 7, run: time::k_slit },
    Entry { name: "motion", family: 7, run: time::k_motion },
    Entry { name: "comb", family: 7, run: time::k_comb },
    Entry { name: "flicker", family: 7, run: time::k_flicker },
    Entry { name: "mean", family: 7, run: time::k_mean },
    Entry { name: "mean3", family: 7, run: time::k_mean3 },
    Entry { name: "steady", family: 7, run: time::k_steady },
    Entry { name: "calm", family: 7, run: time::k_calm },
    Entry { name: "repair", family: 7, run: time::k_repair },
    Entry { name: "wipe_line", family: 8, run: wipe::k_wipe_line },
    Entry { name: "wipe_shape", family: 8, run: wipe::k_wipe_shape },
    Entry { name: "wipe_clock", family: 8, run: wipe::k_wipe_clock },
    Entry { name: "wipe_bars", family: 8, run: wipe::k_wipe_bars },
    Entry { name: "wipe_tiles", family: 8, run: wipe::k_wipe_tiles },
    Entry { name: "wipe_dots", family: 8, run: wipe::k_wipe_dots },
    Entry { name: "wipe_rings", family: 8, run: wipe::k_wipe_rings },
    Entry { name: "wipe_noise", family: 8, run: wipe::k_wipe_noise },
    Entry { name: "wipe_cells", family: 8, run: wipe::k_wipe_cells },
    Entry { name: "wipe_waves", family: 8, run: wipe::k_wipe_waves },
    Entry { name: "wipe_luma", family: 8, run: wipe::k_wipe_luma },
    Entry { name: "swap", family: 8, run: wipe::k_swap },
    Entry { name: "slide", family: 8, run: wipe::k_slide },
    Entry { name: "card", family: 8, run: wipe::k_card },
    Entry { name: "strip_slide", family: 8, run: wipe::k_strip_slide },
    Entry { name: "flip_tiles", family: 8, run: wipe::k_flip_tiles },
    Entry { name: "burn", family: 8, run: wipe::k_burn },
    Entry { name: "water", family: 8, run: wipe::k_water },
    Entry { name: "doors", family: 8, run: wipe::k_doors },
];

/// The kernels, by name.
pub mod k {
    use super::Kernel;
    pub const COPY: Kernel = Kernel(0);
    pub const FILL: Kernel = Kernel(1);
    pub const HALF_X: Kernel = Kernel(2);
    pub const HALF_Y: Kernel = Kernel(3);
    pub const GAUSS_X: Kernel = Kernel(4);
    pub const GAUSS_Y: Kernel = Kernel(5);
    pub const UP: Kernel = Kernel(6);
    pub const MIX: Kernel = Kernel(7);
    pub const ADD: Kernel = Kernel(8);
    pub const SUM3: Kernel = Kernel(9);
    pub const BRIGHT: Kernel = Kernel(10);
    pub const FINISH: Kernel = Kernel(11);
    pub const LAY: Kernel = Kernel(12);
    pub const SMEAR: Kernel = Kernel(13);
    pub const SPIN: Kernel = Kernel(14);
    pub const LENS: Kernel = Kernel(15);
    pub const LENS_JOIN: Kernel = Kernel(16);
    pub const SURFACE: Kernel = Kernel(17);
    pub const SHARPEN: Kernel = Kernel(18);
    pub const CHROMA_JOIN: Kernel = Kernel(19);
    pub const CHANNEL_JOIN: Kernel = Kernel(20);
    pub const MEDIAN: Kernel = Kernel(21);
    pub const DEBAND: Kernel = Kernel(22);
    pub const CONVOLVE: Kernel = Kernel(23);
    pub const SKIN: Kernel = Kernel(24);
    pub const EDGE_JOIN: Kernel = Kernel(25);
    pub const EDGES: Kernel = Kernel(26);
    pub const LIGHT_JOIN: Kernel = Kernel(27);
    pub const DREAM: Kernel = Kernel(28);
    pub const MATRIX: Kernel = Kernel(29);
    pub const GAMMA: Kernel = Kernel(30);
    pub const HUE: Kernel = Kernel(31);
    pub const MONO: Kernel = Kernel(32);
    pub const NEGATIVE: Kernel = Kernel(33);
    pub const TONES: Kernel = Kernel(34);
    pub const ONLY_BRIGHT: Kernel = Kernel(35);
    pub const LIMIT: Kernel = Kernel(36);
    pub const ZEBRA: Kernel = Kernel(37);
    pub const THRESHOLD: Kernel = Kernel(38);
    pub const SPLIT_TINT: Kernel = Kernel(39);
    pub const POSTERIZE: Kernel = Kernel(40);
    pub const SOLARIZE: Kernel = Kernel(41);
    pub const BLEACH: Kernel = Kernel(42);
    pub const FALSE_COLOR: Kernel = Kernel(43);
    pub const PRINT: Kernel = Kernel(44);
    pub const MIST: Kernel = Kernel(45);
    pub const FUSE: Kernel = Kernel(46);
    pub const DISPLACE: Kernel = Kernel(47);
    pub const RGB_SHIFT: Kernel = Kernel(48);
    pub const TURBULENCE: Kernel = Kernel(49);
    pub const TWIRL: Kernel = Kernel(50);
    pub const FISHEYE: Kernel = Kernel(51);
    pub const MAGNIFY: Kernel = Kernel(52);
    pub const RIPPLE: Kernel = Kernel(53);
    pub const WAVES: Kernel = Kernel(54);
    pub const POLAR: Kernel = Kernel(55);
    pub const PLANE: Kernel = Kernel(56);
    pub const KALEIDO: Kernel = Kernel(57);
    pub const FACETS: Kernel = Kernel(58);
    pub const TILES: Kernel = Kernel(59);
    pub const STRIPS: Kernel = Kernel(60);
    pub const ENDLESS: Kernel = Kernel(61);
    pub const ECHOES: Kernel = Kernel(62);
    pub const DROPS: Kernel = Kernel(63);
    pub const GLASS: Kernel = Kernel(64);
    pub const RAINBOW: Kernel = Kernel(65);
    pub const STRETCH_EDGES: Kernel = Kernel(66);
    pub const PRISM: Kernel = Kernel(67);
    pub const GLOW: Kernel = Kernel(68);
    pub const GLOW_TWO: Kernel = Kernel(69);
    pub const GLOW_RAINBOW: Kernel = Kernel(70);
    pub const GLOW_RINGS: Kernel = Kernel(71);
    pub const GLOW_DARK: Kernel = Kernel(72);
    pub const GLOW_SMOKE: Kernel = Kernel(73);
    pub const HALATION: Kernel = Kernel(74);
    pub const GLOW_TUBE: Kernel = Kernel(75);
    pub const LIGHT_FADE: Kernel = Kernel(76);
    pub const NEON: Kernel = Kernel(77);
    pub const POINTS: Kernel = Kernel(78);
    pub const STAR: Kernel = Kernel(79);
    pub const RAYS: Kernel = Kernel(80);
    pub const FIND_LIGHT: Kernel = Kernel(81);
    pub const FLARE: Kernel = Kernel(82);
    pub const SPOT: Kernel = Kernel(83);
    pub const RELIGHT: Kernel = Kernel(84);
    pub const SHADOW: Kernel = Kernel(85);
    pub const LEAK: Kernel = Kernel(86);
    pub const BOKEH: Kernel = Kernel(87);
    pub const FLASHES: Kernel = Kernel(88);
    pub const RIM: Kernel = Kernel(89);
    pub const CLOUDS: Kernel = Kernel(90);
    pub const CLOUDS_COLOR: Kernel = Kernel(91);
    pub const NEBULA: Kernel = Kernel(92);
    pub const CLOUD_SKY: Kernel = Kernel(93);
    pub const CLOUDS_HUE: Kernel = Kernel(94);
    pub const CLOUD_STORM: Kernel = Kernel(95);
    pub const AURORA: Kernel = Kernel(96);
    pub const CAUSTICS: Kernel = Kernel(97);
    pub const GRID: Kernel = Kernel(98);
    pub const GRUNGE: Kernel = Kernel(99);
    pub const DUST: Kernel = Kernel(100);
    pub const GRADIENT: Kernel = Kernel(101);
    pub const MOON: Kernel = Kernel(102);
    pub const STARS: Kernel = Kernel(103);
    pub const SHAPE: Kernel = Kernel(104);
    pub const SPARKLES: Kernel = Kernel(105);
    pub const CELLS: Kernel = Kernel(106);
    pub const SPIRAL: Kernel = Kernel(107);
    pub const FLUX: Kernel = Kernel(108);
    pub const SILK: Kernel = Kernel(109);
    pub const CONTOURS: Kernel = Kernel(110);
    pub const FIBERS: Kernel = Kernel(111);
    pub const MOIRE: Kernel = Kernel(112);
    pub const NERVES: Kernel = Kernel(113);
    pub const RELIEF: Kernel = Kernel(114);
    pub const PAINT: Kernel = Kernel(115);
    pub const PLASMA: Kernel = Kernel(116);
    pub const VEINS: Kernel = Kernel(117);
    pub const SPOTS: Kernel = Kernel(118);
    pub const TILING: Kernel = Kernel(119);
    pub const WEAVE: Kernel = Kernel(120);
    pub const BEAM: Kernel = Kernel(121);
    pub const MUZZLE: Kernel = Kernel(122);
    pub const BOLT: Kernel = Kernel(123);
    pub const BOLT_LIGHT: Kernel = Kernel(124);
    pub const ARC_EDGES: Kernel = Kernel(125);
    pub const FALL: Kernel = Kernel(126);
    pub const OIL: Kernel = Kernel(127);
    pub const IMPASTO: Kernel = Kernel(128);
    pub const CARTOON: Kernel = Kernel(129);
    pub const OUTLINE: Kernel = Kernel(130);
    pub const INK: Kernel = Kernel(131);
    pub const HATCH: Kernel = Kernel(132);
    pub const HALFTONE: Kernel = Kernel(133);
    pub const MOSAIC: Kernel = Kernel(134);
    pub const SCATTER: Kernel = Kernel(135);
    pub const GLITCH: Kernel = Kernel(136);
    pub const DCT_ROWS: Kernel = Kernel(137);
    pub const DCT_COLUMNS: Kernel = Kernel(138);
    pub const IDCT_COLUMNS: Kernel = Kernel(139);
    pub const IDCT_ROWS: Kernel = Kernel(140);
    pub const MOSH: Kernel = Kernel(141);
    pub const SORT: Kernel = Kernel(142);
    pub const TAPE: Kernel = Kernel(143);
    pub const AERIAL: Kernel = Kernel(144);
    pub const TUBE: Kernel = Kernel(145);
    pub const FILM: Kernel = Kernel(146);
    pub const GRAIN: Kernel = Kernel(147);
    pub const VIGNETTE: Kernel = Kernel(148);
    pub const EMBOSS: Kernel = Kernel(149);
    pub const BAND: Kernel = Kernel(150);
    pub const PSYCHE: Kernel = Kernel(151);
    pub const ZEBRA_LIGHT: Kernel = Kernel(152);
    pub const PENCIL: Kernel = Kernel(153);
    pub const WASH: Kernel = Kernel(154);
    pub const LAMPS: Kernel = Kernel(155);
    pub const MISPRINT: Kernel = Kernel(156);
    pub const FRAMES: Kernel = Kernel(157);
    pub const TRAIL: Kernel = Kernel(158);
    pub const DRIFT: Kernel = Kernel(159);
    pub const RGB_TIME: Kernel = Kernel(160);
    pub const SLIT: Kernel = Kernel(161);
    pub const MOTION: Kernel = Kernel(162);
    pub const COMB: Kernel = Kernel(163);
    pub const FLICKER: Kernel = Kernel(164);
    pub const MEAN: Kernel = Kernel(165);
    pub const MEAN3: Kernel = Kernel(166);
    pub const STEADY: Kernel = Kernel(167);
    pub const CALM: Kernel = Kernel(168);
    pub const REPAIR: Kernel = Kernel(169);
    pub const WIPE_LINE: Kernel = Kernel(170);
    pub const WIPE_SHAPE: Kernel = Kernel(171);
    pub const WIPE_CLOCK: Kernel = Kernel(172);
    pub const WIPE_BARS: Kernel = Kernel(173);
    pub const WIPE_TILES: Kernel = Kernel(174);
    pub const WIPE_DOTS: Kernel = Kernel(175);
    pub const WIPE_RINGS: Kernel = Kernel(176);
    pub const WIPE_NOISE: Kernel = Kernel(177);
    pub const WIPE_CELLS: Kernel = Kernel(178);
    pub const WIPE_WAVES: Kernel = Kernel(179);
    pub const WIPE_LUMA: Kernel = Kernel(180);
    pub const SWAP: Kernel = Kernel(181);
    pub const SLIDE: Kernel = Kernel(182);
    pub const CARD: Kernel = Kernel(183);
    pub const STRIP_SLIDE: Kernel = Kernel(184);
    pub const FLIP_TILES: Kernel = Kernel(185);
    pub const BURN: Kernel = Kernel(186);
    pub const WATER: Kernel = Kernel(187);
    pub const DOORS: Kernel = Kernel(188);
}

/// What the kernels share.
mod shared {
    use super::*;
    #[inline]
    pub fn angle(c: &C<'_>, a0: Vn<f32, 2>) -> f32 {
        if ((f32::abs(a0.0[0]) + f32::abs(a0.0[1])) < 1e-7f32) {
            return 0.0f32;
        }
        return f32::atan2(a0.0[1], a0.0[0]);
    }

    #[inline]
    pub fn back(c: &C<'_>, a0: Vn<f32, 3>, a1: f32) -> Vn<f32, 4> {
        let e2 = (a0 * Vn([a1; 3]));
        return Vn([e2.0[0], e2.0[1], e2.0[2], a1]);
    }

    #[inline]
    pub fn cells(c: &C<'_>, a0: Vn<f32, 2>, a1: f32, a2: f32, a3: u32) -> Vn<f32, 3> {
        let mut v0: f32 = 64.0f32;
        let mut v1: f32 = 64.0f32;
        let mut v2: f32 = 0.0f32;
        let mut v3: i32 = (-1i32);
        let mut v4: i32 = 0i32;
        let e7 = (a0 - Vn([0.5f32; 2])).map(f32::floor);
        'l0: loop {
            'b0: {
                let e16 = v3;
                if (e16 <= 2i32) {
                } else {
                    break 'l0;
                }
                v4 = (-1i32);
                'l1: loop {
                    'b1: {
                        let e21 = v4;
                        if (e21 <= 2i32) {
                        } else {
                            break 'l1;
                        }
                        let e26 = v4;
                        let e27 = (e7.0[0] as i32).wrapping_add(e26);
                        let e30 = v3;
                        let e31 = (e7.0[1] as i32).wrapping_add(e30);
                        let e32 = hash2(c, e27, e31, a3);
                        let e33 = unit(c, e32);
                        let e35 = (e33 * 6.2831855f32);
                        let e38 = hash(c, e32.wrapping_add(2654435769u32));
                        let e39 = unit(c, e38);
                        let e43 = (0.6f32 + (e39 * 0.8f32));
                        let e46 = hash(c, e32.wrapping_add(625341585u32));
                        let e47 = unit(c, e46);
                        let e50 = hash(c, e32.wrapping_add(1821285621u32));
                        let e51 = unit(c, e50);
                        let e82 = ((Vn([((e27 as f32) + 0.5f32), ((e31 as f32) + 0.5f32)]) + ((((Vn([e47, e51]) - Vn([0.5f32; 2])) * Vn([0.72f32; 2])) + (Vn([f32::cos((e35 + (a2 * e43))), f32::sin(((e35 * 1.7f32) + ((a2 * e43) * 1.3f32)))]) * Vn([0.18f32; 2]))) * Vn([a1; 2]))) - a0).length();
                        let e83 = v0;
                        if (e82 < e83) {
                            let e85 = v0;
                            v1 = e85;
                            v0 = e82;
                            let e88 = hash(c, e32.wrapping_add(1374496523u32));
                            let e89 = unit(c, e88);
                            v2 = e89;
                        } else {
                            let e90 = v1;
                            if (e82 < e90) {
                                v1 = e82;
                            }
                        }
                    }
                    let e93 = v4;
                    v4 = e93.wrapping_add(1i32);
                }
            }
            let e96 = v3;
            v3 = e96.wrapping_add(1i32);
        }
        let e98 = v0;
        let e99 = v1;
        let e100 = v2;
        return Vn([e98, e99, e100]);
    }

    #[inline]
    pub fn contrast3(c: &C<'_>, a0: Vn<f32, 3>, a1: f32) -> Vn<f32, 3> {
        let e5 = ((a0 - Vn([0.5f32; 3])) * Vn([a1; 3]));
        let e12 = (e5.map(f32::abs) - Vn([0.4f32; 3])).zip(Vn([0.0f32; 3]), f32::max);
        return (Vn([0.5f32; 3]) + ((e5 - (e5.map(signf) * e12)) + (e5.map(signf) * (e12 / (Vn([1.0f32; 3]) + ((e12 * Vn([f32::max((a1 - 1.0f32), 0.0f32); 3])) * Vn([2.0f32; 3])))))));
    }

    #[inline]
    pub fn edge0(c: &C<'_>, a0: i32, a1: i32, a2: i32) -> Vn<f32, 4> {
        let mut v0: bool = false;
        let mut v1: bool = false;
        let mut v2: bool = false;
        let mut v3: bool = false;
        let mut v4: i32 = 0i32;
        let mut v5: i32 = 0i32;
        let e5 = c.p.w0;
        let e6 = (e5 as i32);
        let e9 = c.p.h0;
        let e10 = (e9 as i32);
        if (a2 == 0i32) {
            if (!(a0 < 0i32)) {
                v1 = (a1 < 0i32);
            } else {
                v1 = true;
            }
            let e23 = v1;
            if (!e23) {
                v2 = (a0 >= e6);
            } else {
                v2 = true;
            }
            let e29 = v2;
            if (!e29) {
                v3 = (a1 >= e10);
            } else {
                v3 = true;
            }
            let e35 = v3;
            v0 = e35;
        } else {
            v0 = false;
        }
        let e37 = v0;
        if e37 {
            return Vn([0.0f32; 4]);
        }
        if (a2 == 2i32) {
            v4 = a0;
            v5 = a1;
            let e44 = v4;
            if (e44 < 0i32) {
                let e47 = v4;
                v4 = (-1i32).wrapping_sub(e47);
            } else {
                let e50 = v4;
                if (e50 >= e6) {
                    let e56 = v4;
                    v4 = 2i32.wrapping_mul(e6).wrapping_sub(1i32).wrapping_sub(e56);
                }
            }
            let e58 = v5;
            if (e58 < 0i32) {
                let e61 = v5;
                v5 = (-1i32).wrapping_sub(e61);
            } else {
                let e64 = v5;
                if (e64 >= e10) {
                    let e70 = v5;
                    v5 = 2i32.wrapping_mul(e10).wrapping_sub(1i32).wrapping_sub(e70);
                }
            }
            let e72 = v4;
            let e73 = v5;
            let e74 = at0(c, e72, e73);
            return e74;
        }
        let e75 = at0(c, a0, a1);
        return e75;
    }

    #[inline]
    pub fn edge_gap(c: &C<'_>, a0: f32) -> f32 {
        return f32::max(a0, 0.7f32);
    }

    #[inline]
    pub fn edge_thin(c: &C<'_>, a0: f32) -> f32 {
        return f32::min(1.0f32, (a0 / 0.7f32));
    }

    #[inline]
    pub fn fade(c: &C<'_>, a0: f32) -> f32 {
        return (((a0 * a0) * a0) * ((a0 * ((a0 * 6.0f32) - 15.0f32)) + 10.0f32));
    }

    #[inline]
    pub fn fbm(c: &C<'_>, a0: Vn<f32, 3>, a1: i32, a2: u32) -> f32 {
        let mut v0: f32 = 0.0f32;
        let mut v1: f32 = 0.5f32;
        let mut v2: f32 = 0.0f32;
        let mut v3: Vn<f32, 3> = Vn([0.0f32; 3]);
        let mut v4: i32 = 0i32;
        v3 = a0;
        'l0: loop {
            'b0: {
                let e12 = v4;
                if (e12 < a1) {
                } else {
                    break 'l0;
                }
                let e14 = v0;
                let e15 = v3;
                let e16 = v4;
                let e21 = noise3(c, e15, a2.wrapping_add((e16 as u32).wrapping_mul(131u32)));
                let e22 = v1;
                v0 = (e14 + (e21 * e22));
                let e25 = v2;
                let e26 = v1;
                v2 = (e25 + e26);
                let e28 = v1;
                v1 = (e28 * 0.5f32);
                let e32 = v3.0[0];
                let e38 = v3.0[1];
                let e44 = v3.0[2];
                v3 = Vn([((e32 * 2.0f32) + 7.3f32), ((e38 * 2.0f32) + 3.1f32), ((e44 * 1.6f32) + 1.7f32)]);
            }
            let e51 = v4;
            v4 = e51.wrapping_add(1i32);
        }
        let e53 = v0;
        let e54 = v2;
        return (e53 / f32::max(e54, 1e-6f32));
    }

    #[inline]
    pub fn fr(c: &C<'_>, a0: f32) -> f32 {
        return (a0 - f32::floor(a0));
    }

    #[inline]
    pub fn full_pixel(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<i32, 2> {
        let e3 = c.p.scale;
        let e5 = (1.0f32 / e3);
        return Vn([(f32::floor((((a0.0[0] as f32) + 0.5f32) * e5)) as i32), (f32::floor((((a0.0[1] as f32) + 0.5f32) * e5)) as i32)]);
    }

    #[inline]
    pub fn grad3(c: &C<'_>, a0: u32, a1: f32, a2: f32, a3: f32) -> f32 {
        let mut v0: f32 = 0.0f32;
        let mut v1: f32 = 0.0f32;
        let mut v2: bool = false;
        let e5 = a0.wrapping_shr(28u32);
        v0 = a2;
        if (e5 < 8u32) {
            v0 = a1;
        }
        v1 = a3;
        if (e5 < 4u32) {
            v1 = a2;
        } else {
            if (!(e5 == 12u32)) {
                v2 = (e5 == 14u32);
            } else {
                v2 = true;
            }
            let e20 = v2;
            if e20 {
                v1 = a1;
            }
        }
        if ((e5 & 1u32) == 1u32) {
            let e25 = v0;
            v0 = (-e25);
        }
        if ((e5 & 2u32) == 2u32) {
            let e31 = v1;
            v1 = (-e31);
        }
        let e33 = v0;
        let e34 = v1;
        return (e33 + e34);
    }

    #[inline]
    pub fn grad_row(c: &C<'_>, a0: i32, a1: i32, a2: i32, a3: Vn<f32, 3>, a4: u32, a5: f32) -> f32 {
        let e6 = hash3(c, a0, a1, a2, a4);
        let e10 = grad3(c, e6, a3.0[0], a3.0[1], a3.0[2]);
        let e13 = hash3(c, a0.wrapping_add(1i32), a1, a2, a4);
        let e19 = grad3(c, e13, (a3.0[0] - 1.0f32), a3.0[1], a3.0[2]);
        return (e10 + ((e19 - e10) * a5));
    }

    #[inline]
    pub fn hash(c: &C<'_>, a0: u32) -> u32 {
        let mut v0: u32 = 0u32;
        v0 = a0;
        let e2 = v0;
        let e3 = v0;
        v0 = (e2 ^ e3.wrapping_shr(16u32));
        let e7 = v0;
        v0 = e7.wrapping_mul(2146121005u32);
        let e10 = v0;
        let e11 = v0;
        v0 = (e10 ^ e11.wrapping_shr(15u32));
        let e15 = v0;
        v0 = e15.wrapping_mul(2221713035u32);
        let e18 = v0;
        let e19 = v0;
        v0 = (e18 ^ e19.wrapping_shr(16u32));
        let e23 = v0;
        return e23;
    }

    #[inline]
    pub fn hash2(c: &C<'_>, a0: i32, a1: i32, a2: u32) -> u32 {
        let e13 = hash(c, (((a0 as u32).wrapping_mul(2376512323u32) ^ (a1 as u32).wrapping_mul(3625334849u32)) ^ a2.wrapping_mul(3407524639u32)));
        return e13;
    }

    #[inline]
    pub fn hash3(c: &C<'_>, a0: i32, a1: i32, a2: i32, a3: u32) -> u32 {
        let e18 = hash(c, ((((a0 as u32).wrapping_mul(2376512323u32) ^ (a1 as u32).wrapping_mul(3625334849u32)) ^ (a2 as u32).wrapping_mul(3407524639u32)) ^ a3.wrapping_mul(374761393u32)));
        return e18;
    }

    #[inline]
    pub fn hsv(c: &C<'_>, a0: f32, a1: f32, a2: f32) -> Vn<f32, 3> {
        let e9 = Vn([(a0 + 1.0f32), (a0 + 0.6666667f32), (a0 + 0.33333334f32)]);
        return ((Vn([1.0f32; 3]) + ((((((e9 - e9.map(f32::floor)) * Vn([6.0f32; 3])) - Vn([3.0f32; 3])).map(f32::abs) - Vn([1.0f32; 3])).zip3(Vn([0.0f32; 3]), Vn([1.0f32; 3]), clampf) - Vn([1.0f32; 3])) * Vn([a1; 3]))) * Vn([a2; 3]));
    }

    #[inline]
    pub fn hue_turn(c: &C<'_>, a0: Vn<f32, 3>, a1: f32) -> Vn<f32, 3> {
        let e2 = luma(c, a0);
        let e4 = (a0.0[2] - e2);
        let e6 = (a0.0[0] - e2);
        let e7 = f32::sin(a1);
        let e8 = f32::cos(a1);
        let e12 = ((e2 + (e4 * e7)) + (e6 * e8));
        let e16 = ((e2 + (e4 * e8)) - (e6 * e7));
        return Vn([e12, (((e2 - (0.2126f32 * e12)) - (0.0722f32 * e16)) / 0.7152f32), e16]);
    }

    #[inline]
    pub fn inside(c: &C<'_>, a0: Vn<f32, 2>, a1: f32, a2: f32) -> f32 {
        return clampf((f32::min((f32::min(a0.0[0], (1.0f32 - a0.0[0])) * a1), (f32::min(a0.0[1], (1.0f32 - a0.0[1])) * a2)) + 0.5f32), 0.0f32, 1.0f32);
    }

    #[inline]
    pub fn lit(c: &C<'_>, a0: Vn<f32, 4>, a1: Vn<f32, 3>, a2: i32) -> Vn<f32, 4> {
        let e5 = a1.zip(Vn([0.0f32; 3]), f32::max);
        let e19 = (a0.0[3] + ((1.0f32 - a0.0[3]) * clampf(f32::max(f32::max(e5.0[0], e5.0[1]), e5.0[2]), 0.0f32, 1.0f32)));
        if (a2 == 1i32) {
            let e36 = (Vn([a0.0[0], a0.0[1], a0.0[2]]) + (e5.zip(Vn([1.0f32; 3]), f32::min) * (Vn([1.0f32; 3]) - Vn([a0.0[0], a0.0[1], a0.0[2]])).zip3(Vn([0.0f32; 3]), Vn([1.0f32; 3]), clampf)));
            return Vn([e36.0[0], e36.0[1], e36.0[2], e19]);
        }
        let e39 = (Vn([a0.0[0], a0.0[1], a0.0[2]]) + e5);
        return Vn([e39.0[0], e39.0[1], e39.0[2], e19]);
    }

    #[inline]
    pub fn luma(c: &C<'_>, a0: Vn<f32, 3>) -> f32 {
        return (((0.2126f32 * a0.0[0]) + (0.7152f32 * a0.0[1])) + (0.0722f32 * a0.0[2]));
    }

    #[inline]
    pub fn md(c: &C<'_>, a0: f32, a1: f32) -> f32 {
        return (a0 - (a1 * f32::floor((a0 / a1))));
    }

    #[inline]
    pub fn mix1(c: &C<'_>, a0: f32, a1: f32, a2: f32) -> f32 {
        return (a0 + ((a1 - a0) * a2));
    }

    #[inline]
    pub fn mix3(c: &C<'_>, a0: Vn<f32, 3>, a1: Vn<f32, 3>, a2: f32) -> Vn<f32, 3> {
        return (a0 + ((a1 - a0) * Vn([a2; 3])));
    }

    #[inline]
    pub fn mix4(c: &C<'_>, a0: Vn<f32, 4>, a1: Vn<f32, 4>, a2: f32) -> Vn<f32, 4> {
        return (a0 + ((a1 - a0) * Vn([a2; 4])));
    }

    #[inline]
    pub fn noise3(c: &C<'_>, a0: Vn<f32, 3>, a1: u32) -> f32 {
        let e2 = a0.map(f32::floor);
        let e3 = (a0 - e2);
        let e5 = (e2.0[0] as i32);
        let e7 = (e2.0[1] as i32);
        let e9 = (e2.0[2] as i32);
        let e11 = fade(c, e3.0[0]);
        let e13 = fade(c, e3.0[1]);
        let e15 = fade(c, e3.0[2]);
        let e16 = grad_row(c, e5, e7, e9, e3, a1, e11);
        let e24 = grad_row(c, e5, e7.wrapping_add(1i32), e9, (e3 - Vn([0.0f32, 1.0f32, 0.0f32])), a1, e11);
        let e32 = grad_row(c, e5, e7, e9.wrapping_add(1i32), (e3 - Vn([0.0f32, 0.0f32, 1.0f32])), a1, e11);
        let e42 = grad_row(c, e5, e7.wrapping_add(1i32), e9.wrapping_add(1i32), (e3 - Vn([0.0f32, 1.0f32, 1.0f32])), a1, e11);
        let e45 = (e16 + ((e24 - e16) * e13));
        return (e45 + (((e32 + ((e42 - e32) * e13)) - e45) * e15));
    }

    #[inline]
    pub fn pf(c: &C<'_>, a0: u32) -> f32 {
        let e9 = c.p.f[idiv(a0, 4u32) as usize][imod(a0, 4u32) as usize & 3];
        return e9;
    }

    #[inline]
    pub fn pf2(c: &C<'_>, a0: u32) -> Vn<f32, 2> {
        let e1 = pf(c, a0);
        let e4 = pf(c, a0.wrapping_add(1u32));
        return Vn([e1, e4]);
    }

    #[inline]
    pub fn pf3(c: &C<'_>, a0: u32) -> Vn<f32, 3> {
        let e1 = pf(c, a0);
        let e4 = pf(c, a0.wrapping_add(1u32));
        let e7 = pf(c, a0.wrapping_add(2u32));
        return Vn([e1, e4, e7]);
    }

    #[inline]
    pub fn pic0(c: &C<'_>, a0: Vn<f32, 2>, a1: i32) -> Vn<f32, 4> {
        let e2 = uv_at(c, a0);
        let e3 = pic0_uv(c, e2, a1);
        return e3;
    }

    #[inline]
    pub fn pic0_uv(c: &C<'_>, a0: Vn<f32, 2>, a1: i32) -> Vn<f32, 4> {
        let e4 = c.p.w0;
        let e8 = c.p.h0;
        let e10 = Vn([(e4 as f32), (e8 as f32)]);
        let e12 = wrap1(c, a0.0[0], a1);
        let e14 = wrap1(c, a0.0[1], a1);
        let e20 = bil0(c, ((Vn([e12, e14]) * e10) - Vn([0.5f32; 2])));
        if (a1 == 0i32) {
            let e25 = inside(c, a0, e10.0[0], e10.0[1]);
            return (e20 * Vn([e25; 4]));
        }
        return e20;
    }

    #[inline]
    pub fn pic1(c: &C<'_>, a0: Vn<f32, 2>, a1: i32) -> Vn<f32, 4> {
        let e2 = uv_at(c, a0);
        let e3 = pic1_uv(c, e2, a1);
        return e3;
    }

    #[inline]
    pub fn pic1_uv(c: &C<'_>, a0: Vn<f32, 2>, a1: i32) -> Vn<f32, 4> {
        let e4 = c.p.w1;
        let e8 = c.p.h1;
        let e10 = Vn([(e4 as f32), (e8 as f32)]);
        let e12 = wrap1(c, a0.0[0], a1);
        let e14 = wrap1(c, a0.0[1], a1);
        let e20 = bil1(c, ((Vn([e12, e14]) * e10) - Vn([0.5f32; 2])));
        if (a1 == 0i32) {
            let e25 = inside(c, a0, e10.0[0], e10.0[1]);
            return (e20 * Vn([e25; 4]));
        }
        return e20;
    }

    #[inline]
    pub fn pos(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 2> {
        let e1 = uv_of(c, a0);
        let e7 = c.p.shape;
        return Vn([((e1.0[0] - 0.5f32) * e7), (e1.0[1] - 0.5f32)]);
    }

    #[inline]
    pub fn pw(c: &C<'_>, a0: f32, a1: f32) -> f32 {
        if (a0 <= 0.0f32) {
            return 0.0f32;
        }
        return f32::powf(a0, a1);
    }

    #[inline]
    pub fn pw3(c: &C<'_>, a0: Vn<f32, 3>, a1: f32) -> Vn<f32, 3> {
        let e3 = pw(c, a0.0[0], a1);
        let e5 = pw(c, a0.0[1], a1);
        let e7 = pw(c, a0.0[2], a1);
        return Vn([e3, e5, e7]);
    }

    #[inline]
    pub fn px(c: &C<'_>) -> f32 {
        let e2 = c.p.h;
        return (1.0f32 / (e2 as f32));
    }

    #[inline]
    pub fn ridges(c: &C<'_>, a0: Vn<f32, 3>, a1: i32, a2: u32) -> f32 {
        let mut v0: f32 = 0.0f32;
        let mut v1: f32 = 0.5f32;
        let mut v2: f32 = 0.0f32;
        let mut v3: Vn<f32, 3> = Vn([0.0f32; 3]);
        let mut v4: i32 = 0i32;
        v3 = a0;
        'l0: loop {
            'b0: {
                let e12 = v4;
                if (e12 < a1) {
                } else {
                    break 'l0;
                }
                let e14 = v3;
                let e15 = v4;
                let e20 = noise3(c, e14, a2.wrapping_add((e15 as u32).wrapping_mul(131u32)));
                let e23 = (1.0f32 - f32::abs(e20));
                let e24 = v0;
                let e26 = v1;
                v0 = (e24 + ((e23 * e23) * e26));
                let e29 = v2;
                let e30 = v1;
                v2 = (e29 + e30);
                let e32 = v1;
                v1 = (e32 * 0.5f32);
                let e36 = v3.0[0];
                let e42 = v3.0[1];
                let e48 = v3.0[2];
                v3 = Vn([((e36 * 2.0f32) + 7.3f32), ((e42 * 2.0f32) + 3.1f32), ((e48 * 1.6f32) + 1.7f32)]);
            }
            let e55 = v4;
            v4 = e55.wrapping_add(1i32);
        }
        let e57 = v0;
        let e58 = v2;
        return (e57 / f32::max(e58, 1e-6f32));
    }

    #[inline]
    pub fn rnd(c: &C<'_>, a0: i32, a1: i32, a2: u32) -> f32 {
        let e3 = hash2(c, a0, a1, a2);
        let e4 = unit(c, e3);
        return e4;
    }

    #[inline]
    pub fn rnd1(c: &C<'_>, a0: i32, a1: u32) -> f32 {
        let e3 = hash2(c, a0, 17i32, a1);
        let e4 = unit(c, e3);
        return e4;
    }

    #[inline]
    pub fn rnd2(c: &C<'_>, a0: i32, a1: i32, a2: u32) -> Vn<f32, 2> {
        let e3 = hash2(c, a0, a1, a2);
        let e4 = unit(c, e3);
        let e7 = hash(c, e3.wrapping_add(2654435769u32));
        let e8 = unit(c, e7);
        return Vn([e4, e8]);
    }

    #[inline]
    pub fn saturate3(c: &C<'_>, a0: Vn<f32, 3>, a1: f32) -> Vn<f32, 3> {
        let e2 = luma(c, a0);
        let e4 = mix3(c, Vn([e2; 3]), a0, a1);
        return e4;
    }

    #[inline]
    pub fn specks(c: &C<'_>, a0: f32) -> f32 {
        let e3 = c.p.scale;
        return f32::min(1.0f32, (a0 * e3));
    }

    #[inline]
    pub fn sq(c: &C<'_>, a0: f32) -> f32 {
        return (a0 * a0);
    }

    #[inline]
    pub fn ss(c: &C<'_>, a0: f32, a1: f32, a2: f32) -> f32 {
        let e8 = clampf(((a2 - a0) / (a1 - a0)), 0.0f32, 1.0f32);
        return ((e8 * e8) * (3.0f32 - (2.0f32 * e8)));
    }

    #[inline]
    pub fn straight(c: &C<'_>, a0: Vn<f32, 4>) -> Vn<f32, 3> {
        if (a0.0[3] <= 1e-6f32) {
            return Vn([0.0f32; 3]);
        }
        return (Vn([a0.0[0], a0.0[1], a0.0[2]]) / Vn([a0.0[3]; 3]));
    }

    #[inline]
    pub fn stroke(c: &C<'_>, a0: f32, a1: f32, a2: f32) -> f32 {
        let e5 = f32::max(a1, f32::max(a2, 1e-5f32));
        let e10 = ss(c, (e5 - a2), (e5 + f32::max(a2, 1e-5f32)), a0);
        return (((1.0f32 - e10) * f32::max(a1, 0.0f32)) / e5);
    }

    #[inline]
    pub fn to_hsv(c: &C<'_>, a0: Vn<f32, 3>) -> Vn<f32, 3> {
        let mut v0: f32 = 0.0f32;
        let e5 = f32::max(f32::max(a0.0[0], a0.0[1]), a0.0[2]);
        let e11 = (e5 - f32::min(f32::min(a0.0[0], a0.0[1]), a0.0[2]));
        if (e11 <= 1e-6f32) {
            return Vn([0.0f32, 0.0f32, e5]);
        }
        if (e5 == a0.0[0]) {
            v0 = ((a0.0[1] - a0.0[2]) / e11);
        } else {
            if (e5 == a0.0[1]) {
                v0 = (2.0f32 + ((a0.0[2] - a0.0[0]) / e11));
            } else {
                v0 = (4.0f32 + ((a0.0[0] - a0.0[1]) / e11));
            }
        }
        let e39 = v0;
        v0 = (e39 / 6.0f32);
        let e42 = v0;
        let e43 = v0;
        return Vn([(e42 - f32::floor(e43)), (e11 / f32::max(e5, 1e-6f32)), e5]);
    }

    #[inline]
    pub fn turn(c: &C<'_>, a0: Vn<f32, 2>, a1: f32) -> Vn<f32, 2> {
        let e2 = f32::cos(a1);
        let e3 = f32::sin(a1);
        return Vn([((a0.0[0] * e2) - (a0.0[1] * e3)), ((a0.0[0] * e3) + (a0.0[1] * e2))]);
    }

    #[inline]
    pub fn unit(c: &C<'_>, a0: u32) -> f32 {
        return ((a0.wrapping_shr(8u32) as f32) / 16777216.0f32);
    }

    #[inline]
    pub fn uv_at(c: &C<'_>, a0: Vn<f32, 2>) -> Vn<f32, 2> {
        let e4 = c.p.shape;
        return Vn([((a0.0[0] / e4) + 0.5f32), (a0.0[1] + 0.5f32)]);
    }

    #[inline]
    pub fn uv_of(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 2> {
        let e7 = c.p.w;
        let e16 = c.p.h;
        return Vn([(((a0.0[0] as f32) + 0.5f32) / (e7 as f32)), (((a0.0[1] as f32) + 0.5f32) / (e16 as f32))]);
    }

    #[inline]
    pub fn wander(c: &C<'_>, a0: f32, a1: u32) -> f32 {
        let e2 = f32::floor(a0);
        let e4 = fade(c, (a0 - e2));
        let e6 = rnd1(c, (e2 as i32), a1);
        let e10 = rnd1(c, (e2 as i32).wrapping_add(1i32), a1);
        return (((e6 + ((e10 - e6) * e4)) * 2.0f32) - 1.0f32);
    }

    #[inline]
    pub fn wrap1(c: &C<'_>, a0: f32, a1: i32) -> f32 {
        if (a1 == 2i32) {
            return (1.0f32 - f32::abs(((a0 - (2.0f32 * f32::floor((a0 * 0.5f32)))) - 1.0f32)));
        }
        if (a1 == 3i32) {
            return (a0 - f32::floor(a0));
        }
        return a0;
    }

}

mod basic {
    use super::shared::*;
    use super::*;
    pub fn k_add(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = at1(c, a0.0[0], a0.0[1]);
        let e9 = pf(c, 0u32);
        let e12 = pf3(c, 1u32);
        let e17 = c.p.i[0];
        let e18 = lit(c, e3, ((Vn([e6.0[0], e6.0[1], e6.0[2]]) * Vn([e9; 3])) * e12), e17);
        return e18;
    }

    pub fn k_bright(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let e3 = at0(c, a0.0[0], a0.0[1]);
        v0 = Vn([e3.0[0], e3.0[1], e3.0[2]]);
        let e9 = c.p.i[0];
        if (e9 == 1i32) {
            let e14 = v0;
            v0 = (Vn([e3.0[3]; 3]) - e14).zip(Vn([0.0f32; 3]), f32::max);
        }
        let e20 = v0.0[0];
        let e22 = v0.0[1];
        let e25 = v0.0[2];
        let e28 = pf(c, 0u32);
        let e30 = pf(c, 1u32);
        let e33 = pf(c, 0u32);
        let e35 = pf(c, 1u32);
        let e39 = ss(c, (e28 - e30), ((e33 + e35) + 0.0001f32), f32::max(f32::max(e20, e22), e25));
        let e40 = v0;
        let e42 = pf(c, 2u32);
        let e43 = saturate3(c, e40, e42);
        let e48 = pf(c, 3u32);
        let e50 = (e43.zip(Vn([0.0f32; 3]), f32::max) * Vn([(e39 * e48); 3]));
        return Vn([e50.0[0], e50.0[1], e50.0[2], (e3.0[3] * e39)]);
    }

    pub fn k_copy(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        return e3;
    }

    pub fn k_fill(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e2 = pf3(c, 0u32);
        let e4 = pf(c, 3u32);
        return Vn([e2.0[0], e2.0[1], e2.0[2], e4]);
    }

    pub fn k_finish(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 1.0f32;
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = at1(c, a0.0[0], a0.0[1]);
        let e12 = c.p.i[0];
        if (e12 > 0i32) {
            let e15 = pos(c, a0);
            let e17 = pf2(c, 1u32);
            let e20 = pf(c, 5u32);
            let e22 = turn(c, (e15 - e17), (-e20));
            let e24 = pf2(c, 3u32);
            let e28 = c.p.i[0];
            let e29 = shape_depth(c, e22, e24, e28);
            let e31 = pf(c, 6u32);
            let e32 = px(c);
            let e35 = f32::max(e31, (e32 * 0.5f32));
            let e37 = ss(c, (-e35), e35, e29);
            v0 = e37;
            let e41 = c.p.i[1];
            if (e41 == 1i32) {
                let e44 = v0;
                v0 = (1.0f32 - e44);
            }
            let e50 = c.p.i[2];
            if (e50 == 1i32) {
                let e53 = v0;
                let e54 = v0;
                let e55 = v0;
                return Vn([e53, e54, e55, 1.0f32]);
            }
        }
        let e58 = v0;
        let e60 = pf(c, 0u32);
        let e62 = mix4(c, e3, e6, (e58 * e60));
        return e62;
    }

    pub fn k_gauss_x(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 1.0f32;
        let mut v1: f32 = 0.0f32;
        let mut v2: Vn<f32, 4> = Vn([0.0f32; 4]);
        let mut v3: f32 = 1.0f32;
        let mut v4: i32 = 1i32;
        let e2 = pf(c, 0u32);
        v1 = f32::exp((-0.5f32 / (e2 * e2)));
        let e10 = v1;
        let e11 = v1;
        let e18 = c.p.i[0];
        let e19 = edge0(c, a0.0[0], a0.0[1], e18);
        v2 = e19;
        'l0: loop {
            'b0: {
                let e25 = v4;
                let e29 = c.p.i[1];
                if (e25 <= e29) {
                } else {
                    break 'l0;
                }
                let e31 = v0;
                let e32 = v1;
                v0 = (e31 * e32);
                let e34 = v1;
                v1 = (e34 * (e10 * e11));
                let e36 = v2;
                let e38 = v4;
                let e44 = c.p.i[0];
                let e45 = edge0(c, a0.0[0].wrapping_sub(e38), a0.0[1], e44);
                let e47 = v4;
                let e53 = c.p.i[0];
                let e54 = edge0(c, a0.0[0].wrapping_add(e47), a0.0[1], e53);
                let e56 = v0;
                v2 = (e36 + ((e45 + e54) * Vn([e56; 4])));
                let e59 = v3;
                let e60 = v0;
                v3 = (e59 + (2.0f32 * e60));
            }
            let e65 = v4;
            v4 = e65.wrapping_add(1i32);
        }
        let e67 = v2;
        let e68 = v3;
        return (e67 * Vn([(1.0f32 / e68); 4]));
    }

    pub fn k_gauss_y(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 1.0f32;
        let mut v1: f32 = 0.0f32;
        let mut v2: Vn<f32, 4> = Vn([0.0f32; 4]);
        let mut v3: f32 = 1.0f32;
        let mut v4: i32 = 1i32;
        let e2 = pf(c, 0u32);
        v1 = f32::exp((-0.5f32 / (e2 * e2)));
        let e10 = v1;
        let e11 = v1;
        let e18 = c.p.i[0];
        let e19 = edge0(c, a0.0[0], a0.0[1], e18);
        v2 = e19;
        'l0: loop {
            'b0: {
                let e25 = v4;
                let e29 = c.p.i[1];
                if (e25 <= e29) {
                } else {
                    break 'l0;
                }
                let e31 = v0;
                let e32 = v1;
                v0 = (e31 * e32);
                let e34 = v1;
                v1 = (e34 * (e10 * e11));
                let e36 = v2;
                let e39 = v4;
                let e44 = c.p.i[0];
                let e45 = edge0(c, a0.0[0], a0.0[1].wrapping_sub(e39), e44);
                let e48 = v4;
                let e53 = c.p.i[0];
                let e54 = edge0(c, a0.0[0], a0.0[1].wrapping_add(e48), e53);
                let e56 = v0;
                v2 = (e36 + ((e45 + e54) * Vn([e56; 4])));
                let e59 = v3;
                let e60 = v0;
                v3 = (e59 + (2.0f32 * e60));
            }
            let e65 = v4;
            v4 = e65.wrapping_add(1i32);
        }
        let e67 = v2;
        let e68 = v3;
        return (e67 * Vn([(1.0f32 / e68); 4]));
    }

    pub fn k_half_x(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = a0.0[0].wrapping_mul(2i32);
        let e7 = c.p.i[0];
        let e11 = edge0(c, e3.wrapping_sub(1i32), a0.0[1], e7);
        let e15 = edge0(c, e3.wrapping_add(2i32), a0.0[1], e7);
        let e20 = edge0(c, e3, a0.0[1], e7);
        let e24 = edge0(c, e3.wrapping_add(1i32), a0.0[1], e7);
        return (((e11 + e15) * Vn([0.125f32; 4])) + ((e20 + e24) * Vn([0.375f32; 4])));
    }

    pub fn k_half_y(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = a0.0[1].wrapping_mul(2i32);
        let e7 = c.p.i[0];
        let e11 = edge0(c, a0.0[0], e3.wrapping_sub(1i32), e7);
        let e15 = edge0(c, a0.0[0], e3.wrapping_add(2i32), e7);
        let e20 = edge0(c, a0.0[0], e3, e7);
        let e24 = edge0(c, a0.0[0], e3.wrapping_add(1i32), e7);
        return (((e11 + e15) * Vn([0.125f32; 4])) + ((e20 + e24) * Vn([0.375f32; 4])));
    }

    pub fn k_lay(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = at1(c, a0.0[0], a0.0[1]);
        let e8 = pf(c, 0u32);
        let e9 = (e6 * Vn([e8; 4]));
        let e13 = c.p.i[0];
        if (e13 == 1i32) {
            let e18 = (Vn([e3.0[0], e3.0[1], e3.0[2]]) + Vn([e9.0[0], e9.0[1], e9.0[2]]));
            return Vn([e18.0[0], e18.0[1], e18.0[2], (e3.0[3] + (e9.0[3] * (1.0f32 - e3.0[3])))]);
        }
        if (e13 == 2i32) {
            let e41 = (Vn([e3.0[0], e3.0[1], e3.0[2]]) + (Vn([e9.0[0], e9.0[1], e9.0[2]]) * (Vn([1.0f32; 3]) - Vn([e3.0[0], e3.0[1], e3.0[2]])).zip3(Vn([0.0f32; 3]), Vn([1.0f32; 3]), clampf)));
            return Vn([e41.0[0], e41.0[1], e41.0[2], (e3.0[3] + (e9.0[3] * (1.0f32 - e3.0[3])))]);
        }
        if (e13 == 3i32) {
            let e52 = straight(c, e9);
            let e57 = mix3(c, Vn([1.0f32; 3]), e52, e9.0[3]);
            let e58 = (Vn([e3.0[0], e3.0[1], e3.0[2]]) * e57);
            return Vn([e58.0[0], e58.0[1], e58.0[2], e3.0[3]]);
        }
        if (e13 == 4i32) {
            let e63 = straight(c, e3);
            let e64 = straight(c, e9);
            let e67 = soft_light1(c, e63.0[0], e64.0[0]);
            let e70 = soft_light1(c, e63.0[1], e64.0[1]);
            let e73 = soft_light1(c, e63.0[2], e64.0[2]);
            let e76 = mix3(c, e63, Vn([e67, e70, e73]), e9.0[3]);
            let e78 = (e76 * Vn([e3.0[3]; 3]));
            return Vn([e78.0[0], e78.0[1], e78.0[2], e3.0[3]]);
        }
        if (e13 == 5i32) {
            let e85 = at1(c, a0.0[0], a0.0[1]);
            let e87 = pf(c, 0u32);
            let e88 = mix4(c, e3, e85, e87);
            return e88;
        }
        let e95 = (Vn([e9.0[0], e9.0[1], e9.0[2]]) + (Vn([e3.0[0], e3.0[1], e3.0[2]]) * Vn([(1.0f32 - e9.0[3]); 3])));
        return Vn([e95.0[0], e95.0[1], e95.0[2], (e9.0[3] + (e3.0[3] * (1.0f32 - e9.0[3])))]);
    }

    pub fn k_mix(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = at1(c, a0.0[0], a0.0[1]);
        let e8 = pf(c, 0u32);
        let e9 = mix4(c, e3, e6, e8);
        return e9;
    }

    pub fn k_sum3(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e5 = pf(c, 0u32);
        let e9 = at1(c, a0.0[0], a0.0[1]);
        let e11 = pf(c, 1u32);
        let e16 = at2(c, a0.0[0], a0.0[1]);
        let e18 = pf(c, 2u32);
        return (((e3 * Vn([e5; 4])) + (e9 * Vn([e11; 4]))) + (e16 * Vn([e18; 4])));
    }

    pub fn k_up(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 4> = Vn([0.0f32, 1.0f32, 0.0f32, 0.0f32]);
        let mut v1: Vn<f32, 4> = Vn([0.0f32, 1.0f32, 0.0f32, 0.0f32]);
        let e6 = pf(c, 0u32);
        let e9 = ((((a0.0[0] as f32) + 0.5f32) / e6) - 0.5f32);
        let e15 = pf(c, 1u32);
        let e18 = ((((a0.0[1] as f32) + 0.5f32) / e15) - 0.5f32);
        let e19 = f32::floor(e9);
        let e20 = f32::floor(e18);
        let e28 = pf(c, 0u32);
        if (e28 > 1.0f32) {
            let e32 = spline(c, (e9 - e19));
            v0 = e32;
        }
        let e40 = pf(c, 1u32);
        if (e40 > 1.0f32) {
            let e44 = spline(c, (e18 - e20));
            v1 = e44;
        }
        let e45 = (e19 as i32);
        let e46 = (e20 as i32);
        let e50 = c.p.i[0];
        let e55 = edge0(c, e45.wrapping_sub(1i32), e46.wrapping_sub(1i32), e50);
        let e57 = v0.0[0];
        let e61 = edge0(c, e45, e46.wrapping_sub(1i32), e50);
        let e63 = v0.0[1];
        let e70 = edge0(c, e45.wrapping_add(1i32), e46.wrapping_sub(1i32), e50);
        let e72 = v0.0[2];
        let e79 = edge0(c, e45.wrapping_add(2i32), e46.wrapping_sub(1i32), e50);
        let e81 = v0.0[3];
        let e86 = edge0(c, e45.wrapping_sub(1i32), e46, e50);
        let e88 = v0.0[0];
        let e90 = edge0(c, e45, e46, e50);
        let e92 = v0.0[1];
        let e97 = edge0(c, e45.wrapping_add(1i32), e46, e50);
        let e99 = v0.0[2];
        let e104 = edge0(c, e45.wrapping_add(2i32), e46, e50);
        let e106 = v0.0[3];
        let e113 = edge0(c, e45.wrapping_sub(1i32), e46.wrapping_add(1i32), e50);
        let e115 = v0.0[0];
        let e119 = edge0(c, e45, e46.wrapping_add(1i32), e50);
        let e121 = v0.0[1];
        let e128 = edge0(c, e45.wrapping_add(1i32), e46.wrapping_add(1i32), e50);
        let e130 = v0.0[2];
        let e137 = edge0(c, e45.wrapping_add(2i32), e46.wrapping_add(1i32), e50);
        let e139 = v0.0[3];
        let e146 = edge0(c, e45.wrapping_sub(1i32), e46.wrapping_add(2i32), e50);
        let e148 = v0.0[0];
        let e152 = edge0(c, e45, e46.wrapping_add(2i32), e50);
        let e154 = v0.0[1];
        let e161 = edge0(c, e45.wrapping_add(1i32), e46.wrapping_add(2i32), e50);
        let e163 = v0.0[2];
        let e170 = edge0(c, e45.wrapping_add(2i32), e46.wrapping_add(2i32), e50);
        let e172 = v0.0[3];
        let e176 = v1.0[0];
        let e179 = v1.0[1];
        let e183 = v1.0[2];
        let e187 = v1.0[3];
        return ((((((((e55 * Vn([e57; 4])) + (e61 * Vn([e63; 4]))) + (e70 * Vn([e72; 4]))) + (e79 * Vn([e81; 4]))) * Vn([e176; 4])) + (((((e86 * Vn([e88; 4])) + (e90 * Vn([e92; 4]))) + (e97 * Vn([e99; 4]))) + (e104 * Vn([e106; 4]))) * Vn([e179; 4]))) + (((((e113 * Vn([e115; 4])) + (e119 * Vn([e121; 4]))) + (e128 * Vn([e130; 4]))) + (e137 * Vn([e139; 4]))) * Vn([e183; 4]))) + (((((e146 * Vn([e148; 4])) + (e152 * Vn([e154; 4]))) + (e161 * Vn([e163; 4]))) + (e170 * Vn([e172; 4]))) * Vn([e187; 4])));
    }

    #[inline]
    pub fn shape_depth(c: &C<'_>, a0: Vn<f32, 2>, a1: Vn<f32, 2>, a2: i32) -> f32 {
        let mut v0: f32 = 0.0f32;
        if (a2 == 1i32) {
            let e5 = (a0 / a1);
            let e6 = e5.length();
            if (e6 < 1e-5f32) {
                return f32::min(a1.0[0], a1.0[1]);
            }
            return ((1.0f32 - e6) / f32::max(((e5 / a1).length() / e6), 1e-6f32));
        }
        if (a2 == 3i32) {
            return (((1.0f32 - (f32::abs(a0.0[0]) / a1.0[0])) - (f32::abs(a0.0[1]) / a1.0[1])) / Vn([(1.0f32 / a1.0[0]), (1.0f32 / a1.0[1])]).length());
        }
        if (a2 == 4i32) {
            v0 = (f32::min(a1.0[0], a1.0[1]) * 0.35f32);
        }
        let e53 = v0;
        let e55 = ((a0.map(f32::abs) - a1) + Vn([e53; 2]));
        let e56 = v0;
        return ((e56 - e55.zip(Vn([0.0f32; 2]), f32::max).length()) - f32::min(f32::max(e55.0[0], e55.0[1]), 0.0f32));
    }

    #[inline]
    pub fn soft_light1(c: &C<'_>, a0: f32, a1: f32) -> f32 {
        return (a0 + (((2.0f32 * a1) - 1.0f32) * (a0 - (a0 * a0))));
    }

    #[inline]
    pub fn spline(c: &C<'_>, a0: f32) -> Vn<f32, 4> {
        let e2 = (1.0f32 - a0);
        let e6 = (((e2 * e2) * e2) / 6.0f32);
        let e18 = ((((((3.0f32 * a0) * a0) * a0) - ((6.0f32 * a0) * a0)) + 4.0f32) / 6.0f32);
        let e22 = (((a0 * a0) * a0) / 6.0f32);
        return Vn([e6, e18, (((1.0f32 - e6) - e18) - e22), e22]);
    }

}

mod blur {
    use super::shared::*;
    use super::*;
    #[inline]
    pub fn edge_here(c: &C<'_>, a0: Vn<i32, 2>, a1: f32) -> f32 {
        let e6 = Vn([(a0.0[0] as f32), (a0.0[1] as f32)]);
        let e11 = bil0(c, Vn([(e6.0[0] - a1), e6.0[1]]));
        let e13 = luma(c, Vn([e11.0[0], e11.0[1], e11.0[2]]));
        let e18 = bil0(c, Vn([(e6.0[0] + a1), e6.0[1]]));
        let e20 = luma(c, Vn([e18.0[0], e18.0[1], e18.0[2]]));
        let e25 = bil0(c, Vn([e6.0[0], (e6.0[1] - a1)]));
        let e27 = luma(c, Vn([e25.0[0], e25.0[1], e25.0[2]]));
        let e32 = bil0(c, Vn([e6.0[0], (e6.0[1] + a1)]));
        let e34 = luma(c, Vn([e32.0[0], e32.0[1], e32.0[2]]));
        return Vn([(e20 - e13), (e34 - e27)]).length();
    }

    #[inline]
    pub fn focus(c: &C<'_>, a0: Vn<f32, 2>) -> f32 {
        let mut v0: f32 = 0.0f32;
        let e4 = c.p.i[1];
        if (e4 == 0i32) {
            return 1.0f32;
        }
        let e9 = pf2(c, 8u32);
        let e12 = pf(c, 10u32);
        let e14 = turn(c, (a0 - e9), (-e12));
        v0 = f32::abs(e14.0[1]);
        let e21 = c.p.i[1];
        if (e21 == 2i32) {
            v0 = e14.length();
        } else {
            let e28 = c.p.i[1];
            if (e28 == 3i32) {
                v0 = e14.0[1];
            }
        }
        let e33 = pf(c, 11u32);
        let e35 = pf(c, 11u32);
        let e37 = pf(c, 12u32);
        let e41 = v0;
        let e42 = ss(c, e33, (e35 + f32::max(e37, 0.001f32)), e41);
        return e42;
    }

    #[inline]
    pub fn hi4(c: &C<'_>, a0: Vn<f32, 4>, a1: Vn<f32, 4>) -> Vn<f32, 4> {
        return a0.zip(a1, f32::max);
    }

    #[inline]
    pub fn iris(c: &C<'_>, a0: f32, a1: f32, a2: f32) -> f32 {
        if (a1 < 2.5f32) {
            return 1.0f32;
        }
        let e7 = (3.1415927f32 / a1);
        let e21 = mix1(c, (f32::cos(e7) / f32::cos(((a0 - ((2.0f32 * e7) * f32::floor((a0 / (2.0f32 * e7))))) - e7))), 1.0f32, a2);
        return e21;
    }

    pub fn k_channel_join(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = at1(c, a0.0[0], a0.0[1]);
        let e9 = at2(c, a0.0[0], a0.0[1]);
        return Vn([e3.0[0], e6.0[1], e9.0[2], f32::max(f32::max(e3.0[3], e6.0[3]), e9.0[3])]);
    }

    pub fn k_chroma_join(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = at1(c, a0.0[0], a0.0[1]);
        let e8 = luma(c, Vn([e3.0[0], e3.0[1], e3.0[2]]));
        let e11 = luma(c, Vn([e6.0[0], e6.0[1], e6.0[2]]));
        let e15 = pf(c, 1u32);
        let e19 = pf(c, 0u32);
        let e21 = (Vn([(e8 * e15); 3]) + ((Vn([e6.0[0], e6.0[1], e6.0[2]]) - Vn([e11; 3])) * Vn([e19; 3])));
        return Vn([e21.0[0], e21.0[1], e21.0[2], e3.0[3]]);
    }

    pub fn k_convolve(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let mut v1: i32 = 0i32;
        let mut v2: i32 = 0i32;
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e7 = c.p.i[0];
        'l0: loop {
            'b0: {
                let e13 = v1;
                if (e13 < 3i32) {
                } else {
                    break 'l0;
                }
                v2 = 0i32;
                'l1: loop {
                    'b1: {
                        let e18 = v2;
                        if (e18 < 3i32) {
                        } else {
                            break 'l1;
                        }
                        let e21 = v0;
                        let e23 = v2;
                        let e29 = v1;
                        let e34 = at0(c, a0.0[0].wrapping_add(e23.wrapping_sub(1i32).wrapping_mul(e7)), a0.0[1].wrapping_add(e29.wrapping_sub(1i32).wrapping_mul(e7)));
                        let e36 = v1;
                        let e39 = v2;
                        let e42 = pf(c, (e36.wrapping_mul(3i32).wrapping_add(e39) as u32));
                        v0 = (e21 + (Vn([e34.0[0], e34.0[1], e34.0[2]]) * Vn([e42; 3])));
                    }
                    let e46 = v2;
                    v2 = e46.wrapping_add(1i32);
                }
            }
            let e49 = v1;
            v1 = e49.wrapping_add(1i32);
        }
        let e52 = v0;
        let e54 = pf(c, 9u32);
        let e60 = pf(c, 10u32);
        let e61 = mix3(c, Vn([e3.0[0], e3.0[1], e3.0[2]]), (e52 + Vn([(e54 * e3.0[3]); 3])), e60);
        return Vn([e61.0[0], e61.0[1], e61.0[2], e3.0[3]]);
    }

    pub fn k_deband(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = full_pixel(c, a0);
        let e9 = c.p.seed;
        let e10 = rnd2(c, e4.0[0], e4.0[1], e9);
        let e13 = (e10.0[0] * 6.2831855f32);
        let e20 = pf(c, 0u32);
        let e25 = (Vn([f32::cos(e13), f32::sin(e13)]) * Vn([((0.35f32 + (0.65f32 * e10.0[1])) * e20); 2]));
        let e30 = Vn([(a0.0[0] as f32), (a0.0[1] as f32)]);
        let e32 = bil0(c, (e30 + e25));
        let e34 = bil0(c, (e30 - e25));
        let e41 = bil0(c, (e30 + Vn([(-e25.0[1]), e25.0[0]])));
        let e48 = bil0(c, (e30 - Vn([(-e25.0[1]), e25.0[0]])));
        let e51 = ((((e32 + e34) + e41) + e48) * Vn([0.25f32; 4]));
        let e55 = (Vn([e51.0[0], e51.0[1], e51.0[2]]) - Vn([e3.0[0], e3.0[1], e3.0[2]])).map(f32::abs);
        let e57 = pf(c, 1u32);
        let e61 = pf(c, 1u32);
        let e67 = ss(c, (e57 * 0.5f32), e61, f32::max(f32::max(e55.0[0], e55.0[1]), e55.0[2]));
        let e74 = c.p.seed;
        let e77 = rnd(c, e4.0[0], e4.0[1], e74.wrapping_add(77u32));
        let e81 = pf(c, 2u32);
        let e87 = mix3(c, Vn([e3.0[0], e3.0[1], e3.0[2]]), Vn([e51.0[0], e51.0[1], e51.0[2]]), (1.0f32 - e67));
        let e89 = (e87 + Vn([(((e77 - 0.5f32) * e81) * e3.0[3]); 3]));
        return Vn([e89.0[0], e89.0[1], e89.0[2], e3.0[3]]);
    }

    pub fn k_dream(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = at1(c, a0.0[0], a0.0[1]);
        let e10 = pf(c, 1u32);
        let e11 = mix3(c, Vn([e3.0[0], e3.0[1], e3.0[2]]), Vn([e6.0[0], e6.0[1], e6.0[2]]), e10);
        v0 = e11;
        let e26 = pf(c, 0u32);
        let e30 = v0;
        let e37 = v0;
        v0 = (e37 + ((((Vn([e6.0[0], e6.0[1], e6.0[2]]) * Vn([e6.0[0], e6.0[1], e6.0[2]])) * Vn([(1.0f32 / f32::max(e6.0[3], 0.001f32)); 3])).zip(Vn([0.0f32; 3]), f32::max) * Vn([e26; 3])).zip(Vn([1.0f32; 3]), f32::min) * (Vn([e3.0[3]; 3]) - e30).zip3(Vn([0.0f32; 3]), Vn([1.0f32; 3]), clampf)));
        let e46 = Vn([(0.5f32 * e3.0[3]); 3]);
        let e47 = v0;
        let e50 = pf(c, 2u32);
        let e52 = (e46 + ((e47 - e46) * Vn([e50; 3])));
        return Vn([e52.0[0], e52.0[1], e52.0[2], e3.0[3]]);
    }

    pub fn k_edge_join(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let e3 = at2(c, a0.0[0], a0.0[1]);
        let e6 = pf(c, 0u32);
        v0 = clampf((e3.0[0] * e6), 0.0f32, 1.0f32);
        let e15 = c.p.i[0];
        if (e15 == 1i32) {
            let e18 = v0;
            v0 = (1.0f32 - e18);
        }
        let e23 = at0(c, a0.0[0], a0.0[1]);
        let e26 = at1(c, a0.0[0], a0.0[1]);
        let e27 = v0;
        let e28 = mix4(c, e23, e26, e27);
        return e28;
    }

    pub fn k_edges(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e2 = pf(c, 1u32);
        let e3 = edge_gap(c, e2);
        let e4 = edge_here(c, a0, e3);
        let e6 = pf(c, 0u32);
        let e9 = pf(c, 1u32);
        let e10 = edge_thin(c, e9);
        let e11 = ((e4 * e6) * e10);
        return Vn([e11, e11, e11, 1.0f32]);
    }

    pub fn k_lens(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 4> = Vn([0.0f32; 4]);
        let mut v1: Vn<f32, 3> = Vn([0.0f32; 3]);
        let mut v2: f32 = 0.0f32;
        let mut v3: f32 = 0.0f32;
        let mut v4: i32 = 0i32;
        let e1 = pos(c, a0);
        let e2 = focus(c, e1);
        let e4 = pf(c, 0u32);
        let e5 = (e4 * e2);
        let e14 = Vn([((a0.0[0] as f32) + 0.5f32), ((a0.0[1] as f32) + 0.5f32)]);
        let e17 = c.p.w0;
        let e21 = c.p.h0;
        let e23 = Vn([(e17 as f32), (e21 as f32)]);
        if (e5 < 0.35f32) {
            let e28 = at0(c, a0.0[0], a0.0[1]);
            return e28;
        }
        let e32 = c.p.i[2];
        'l0: loop {
            'b0: {
                let e45 = v4;
                if (e45 < e32) {
                } else {
                    break 'l0;
                }
                let e47 = v4;
                let e53 = v4;
                let e56 = ((e53 as f32) * 2.3999631f32);
                let e64 = pf(c, 4u32);
                let e66 = pf(c, 5u32);
                let e67 = iris(c, e56, e64, e66);
                let e70 = pf2(c, 1u32);
                let e73 = pf(c, 3u32);
                let e74 = turn(c, (((Vn([f32::cos(e56), f32::sin(e56)]) * Vn([(f32::sqrt((((e47 as f32) + 0.5f32) / (e32 as f32))) * e5); 2])) * Vn([e67; 2])) * e70), e73);
                let e76 = pf(c, 13u32);
                if (e76 == 0.0f32) {
                    let e84 = c.p.i[0];
                    let e85 = pic0_uv(c, ((e14 + e74) / e23), e84);
                    let e87 = pf(c, 6u32);
                    let e89 = pf(c, 7u32);
                    let e93 = pf(c, 7u32);
                    let e97 = luma(c, Vn([e85.0[0], e85.0[1], e85.0[2]]));
                    let e98 = ss(c, (e89 - 0.15f32), (e93 + 0.15f32), e97);
                    let e101 = (1.0f32 + (e87 * e98));
                    let e102 = v0;
                    v0 = (e102 + (e85 * Vn([e101; 4])));
                    let e105 = v3;
                    v3 = (e105 + e101);
                } else {
                    let e108 = pf(c, 13u32);
                    let e117 = c.p.i[0];
                    let e118 = pic0_uv(c, ((e14 + (e74 * Vn([(1.0f32 + e108); 2]))) / e23), e117);
                    let e124 = c.p.i[0];
                    let e125 = pic0_uv(c, ((e14 + e74) / e23), e124);
                    let e127 = pf(c, 13u32);
                    let e136 = c.p.i[0];
                    let e137 = pic0_uv(c, ((e14 + (e74 * Vn([(1.0f32 - e127); 2]))) / e23), e136);
                    let e139 = pf(c, 6u32);
                    let e141 = pf(c, 7u32);
                    let e145 = pf(c, 7u32);
                    let e149 = luma(c, Vn([e118.0[0], e118.0[1], e118.0[2]]));
                    let e150 = ss(c, (e141 - 0.15f32), (e145 + 0.15f32), e149);
                    let e153 = (1.0f32 + (e139 * e150));
                    let e155 = pf(c, 6u32);
                    let e157 = pf(c, 7u32);
                    let e161 = pf(c, 7u32);
                    let e165 = luma(c, Vn([e125.0[0], e125.0[1], e125.0[2]]));
                    let e166 = ss(c, (e157 - 0.15f32), (e161 + 0.15f32), e165);
                    let e169 = (1.0f32 + (e155 * e166));
                    let e171 = pf(c, 6u32);
                    let e173 = pf(c, 7u32);
                    let e177 = pf(c, 7u32);
                    let e181 = luma(c, Vn([e137.0[0], e137.0[1], e137.0[2]]));
                    let e182 = ss(c, (e173 - 0.15f32), (e177 + 0.15f32), e181);
                    let e185 = (1.0f32 + (e171 * e182));
                    let e186 = v0;
                    v0 = (e186 + Vn([(e118.0[0] * e153), (e125.0[1] * e169), (e137.0[2] * e185), 0.0f32]));
                    let e196 = v1;
                    v1 = (e196 + Vn([e153, e169, e185]));
                    let e199 = v2;
                    v2 = (e199 + (((e118.0[3] + e125.0[3]) + e137.0[3]) / 3.0f32));
                }
            }
            let e209 = v4;
            v4 = e209.wrapping_add(1i32);
        }
        let e212 = pf(c, 13u32);
        if (e212 == 0.0f32) {
            let e215 = v0;
            let e216 = v3;
            return (e215 * Vn([(1.0f32 / e216); 4]));
        }
        let e220 = v0;
        let e222 = v1;
        let e223 = (Vn([e220.0[0], e220.0[1], e220.0[2]]) / e222);
        let e224 = v2;
        return Vn([e223.0[0], e223.0[1], e223.0[2], (e224 / (e32 as f32))]);
    }

    pub fn k_lens_join(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = pos(c, a0);
        let e4 = focus(c, e3);
        let e6 = pf(c, 0u32);
        let e8 = ss(c, 0.6f32, 2.2f32, (e4 * e6));
        let e11 = at0(c, a0.0[0], a0.0[1]);
        let e14 = at1(c, a0.0[0], a0.0[1]);
        let e15 = mix4(c, e11, e14, e8);
        return e15;
    }

    pub fn k_light_join(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = straight(c, e3);
        let e5 = luma(c, e4);
        let e8 = at1(c, a0.0[0], a0.0[1]);
        let e10 = luma(c, Vn([e8.0[0], e8.0[1], e8.0[2]]));
        let e15 = pf(c, 0u32);
        let e17 = pf(c, 1u32);
        let e20 = pf(c, 0u32);
        let e22 = pf(c, 1u32);
        let e26 = ss(c, (e15 - e17), ((e20 + e22) + 0.0001f32), f32::max(e5, (e10 * 0.8f32)));
        v0 = e26;
        let e31 = c.p.i[0];
        if (e31 == 1i32) {
            let e34 = v0;
            v0 = (1.0f32 - e34);
        }
        let e39 = at1(c, a0.0[0], a0.0[1]);
        let e40 = v0;
        let e41 = mix4(c, e3, e39, e40);
        return e41;
    }

    pub fn k_median(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e4 = c.p.i[0];
        let e9 = at0(c, a0.0[0].wrapping_sub(e4), a0.0[1].wrapping_sub(e4));
        let e13 = at0(c, a0.0[0], a0.0[1].wrapping_sub(e4));
        let e18 = at0(c, a0.0[0].wrapping_add(e4), a0.0[1].wrapping_sub(e4));
        let e22 = at0(c, a0.0[0].wrapping_sub(e4), a0.0[1]);
        let e25 = at0(c, a0.0[0], a0.0[1]);
        let e29 = at0(c, a0.0[0].wrapping_add(e4), a0.0[1]);
        let e34 = at0(c, a0.0[0].wrapping_sub(e4), a0.0[1].wrapping_add(e4));
        let e38 = at0(c, a0.0[0], a0.0[1].wrapping_add(e4));
        let e43 = at0(c, a0.0[0].wrapping_add(e4), a0.0[1].wrapping_add(e4));
        let e44 = lo4(c, e9, e13);
        let e45 = lo4(c, e44, e18);
        let e46 = hi4(c, e9, e13);
        let e47 = hi4(c, e46, e18);
        let e48 = mid3(c, e9, e13, e18);
        let e49 = lo4(c, e22, e25);
        let e50 = lo4(c, e49, e29);
        let e51 = hi4(c, e22, e25);
        let e52 = hi4(c, e51, e29);
        let e53 = mid3(c, e22, e25, e29);
        let e54 = lo4(c, e34, e38);
        let e55 = lo4(c, e54, e43);
        let e56 = hi4(c, e34, e38);
        let e57 = hi4(c, e56, e43);
        let e58 = mid3(c, e34, e38, e43);
        let e59 = hi4(c, e45, e50);
        let e60 = hi4(c, e59, e55);
        let e61 = mid3(c, e48, e53, e58);
        let e62 = lo4(c, e47, e52);
        let e63 = lo4(c, e62, e57);
        let e64 = mid3(c, e60, e61, e63);
        return e64;
    }

    pub fn k_sharpen(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let mut v1: f32 = 0.0f32;
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = at1(c, a0.0[0], a0.0[1]);
        let e7 = straight(c, e3);
        let e9 = luma(c, Vn([e3.0[0], e3.0[1], e3.0[2]]));
        let e11 = luma(c, Vn([e6.0[0], e6.0[1], e6.0[2]]));
        v0 = (e9 - e11);
        let e14 = v0;
        let e17 = pf(c, 1u32);
        let e21 = v0;
        v0 = (signf(e21) * f32::max((f32::abs(e14) - e17), 0.0f32));
        let e25 = pf(c, 0u32);
        v1 = e25;
        let e30 = c.p.i[0];
        if (e30 == 1i32) {
            let e33 = luma(c, e7);
            let e37 = v1;
            let e42 = sq(c, ((2.0f32 * clampf(e33, 0.0f32, 1.0f32)) - 1.0f32));
            v1 = (e37 * (1.0f32 - e42));
        }
        let e46 = v0;
        let e47 = v1;
        let e50 = pf(c, 2u32);
        let e53 = pf(c, 2u32);
        let e60 = (Vn([e3.0[0], e3.0[1], e3.0[2]]) + Vn([clampf((e46 * e47), (-e50), e53); 3])).zip(Vn([0.0f32; 3]), f32::max);
        return Vn([e60.0[0], e60.0[1], e60.0[2], e3.0[3]]);
    }

    pub fn k_skin(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let mut v1: Vn<f32, 3> = Vn([0.0f32; 3]);
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = at1(c, a0.0[0], a0.0[1]);
        let e9 = at2(c, a0.0[0], a0.0[1]);
        let e10 = straight(c, e3);
        let e14 = to_hsv(c, e10.zip(Vn([0.0f32; 3]), f32::max));
        let e16 = pf3(c, 0u32);
        let e17 = to_hsv(c, e16);
        v0 = f32::abs((e14.0[0] - e17.0[0]));
        let e23 = v0;
        let e24 = v0;
        v0 = f32::min(e23, (1.0f32 - e24));
        let e28 = v0;
        let e31 = sq(c, (e28 * 5.0f32));
        let e37 = sq(c, ((e14.0[1] - e17.0[1]) * 0.7f32));
        let e41 = pf(c, 3u32);
        let e45 = pf(c, 3u32);
        let e46 = ss(c, (e41 * 0.5f32), e45, f32::sqrt((e31 + e37)));
        let e48 = (1.0f32 - e46);
        let e50 = pf(c, 4u32);
        let e55 = ss(c, 0.05f32, 0.2f32, e14.0[2]);
        let e56 = ((e48 * e50) * e55);
        let e59 = mix3(c, Vn([e3.0[0], e3.0[1], e3.0[2]]), Vn([e6.0[0], e6.0[1], e6.0[2]]), e56);
        v1 = e59;
        let e61 = v1;
        let e66 = pf(c, 5u32);
        v1 = (e61 + ((Vn([e3.0[0], e3.0[1], e3.0[2]]) - Vn([e6.0[0], e6.0[1], e6.0[2]])) * Vn([(e56 * e66); 3])));
        let e72 = pf(c, 6u32);
        let e75 = v1;
        let e78 = v1;
        v1 = (e75 + ((Vn([e9.0[0], e9.0[1], e9.0[2]]) * Vn([(e72 * e48); 3])) * (Vn([e3.0[3]; 3]) - e78).zip3(Vn([0.0f32; 3]), Vn([1.0f32; 3]), clampf)));
        let e87 = v1;
        return Vn([e87.0[0], e87.0[1], e87.0[2], e3.0[3]]);
    }

    pub fn k_smear(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 4> = Vn([0.0f32; 4]);
        let mut v1: i32 = 0i32;
        let e4 = c.p.i[1];
        let e6 = i32_max(e4, 1i32);
        let e8 = pf2(c, 0u32);
        'l0: loop {
            'b0: {
                let e14 = v1;
                if (e14 < e6) {
                } else {
                    break 'l0;
                }
                let e16 = v1;
                let e25 = pf(c, 2u32);
                let e34 = v0;
                let e40 = c.p.w0;
                let e44 = c.p.h0;
                let e51 = c.p.i[0];
                let e52 = pic0_uv(c, (((Vn([(a0.0[0] as f32), (a0.0[1] as f32)]) + (e8 * Vn([(((((e16 as f32) + 0.5f32) / (e6 as f32)) - 0.5f32) + e25); 2]))) + Vn([0.5f32; 2])) / Vn([(e40 as f32), (e44 as f32)])), e51);
                v0 = (e34 + e52);
            }
            let e55 = v1;
            v1 = e55.wrapping_add(1i32);
        }
        let e57 = v0;
        return (e57 * Vn([(1.0f32 / (e6 as f32)); 4]));
    }

    pub fn k_spin(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 1.0f32;
        let mut v1: Vn<f32, 4> = Vn([0.0f32; 4]);
        let mut v2: i32 = 0i32;
        let e4 = c.p.i[1];
        let e6 = i32_max(e4, 1i32);
        let e7 = pos(c, a0);
        let e9 = pf2(c, 0u32);
        let e10 = (e7 - e9);
        let e14 = pf(c, 6u32);
        if (e14 > 0.0f32) {
            let e18 = pf(c, 5u32);
            let e20 = pf(c, 5u32);
            let e22 = pf(c, 6u32);
            let e25 = ss(c, e18, (e20 + e22), e10.length());
            v0 = e25;
        }
        'l0: loop {
            'b0: {
                let e31 = v2;
                if (e31 < e6) {
                } else {
                    break 'l0;
                }
                let e33 = v2;
                let e42 = pf(c, 4u32);
                let e44 = v0;
                let e45 = ((((((e33 as f32) + 0.5f32) / (e6 as f32)) - 0.5f32) + e42) * e44);
                let e46 = v1;
                let e48 = pf(c, 3u32);
                let e50 = turn(c, e10, (e48 * e45));
                let e52 = pf(c, 2u32);
                let e60 = c.p.i[0];
                let e61 = pic0(c, (e9 + (e50 * Vn([f32::exp((e52 * e45)); 2]))), e60);
                v1 = (e46 + e61);
            }
            let e64 = v2;
            v2 = e64.wrapping_add(1i32);
        }
        let e66 = v1;
        return (e66 * Vn([(1.0f32 / (e6 as f32)); 4]));
    }

    pub fn k_surface(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 4> = Vn([0.0f32; 4]);
        let mut v1: f32 = 0.0f32;
        let mut v2: i32 = 0i32;
        let mut v3: i32 = 0i32;
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e7 = c.p.i[0];
        let e11 = c.p.i[1];
        let e13 = pf(c, 0u32);
        let e14 = sq(c, e13);
        let e20 = pf(c, 1u32);
        let e21 = sq(c, e20);
        v2 = e7.wrapping_neg();
        'l0: loop {
            'b0: {
                let e33 = v2;
                if (e33 <= e7) {
                } else {
                    break 'l0;
                }
                v3 = e7.wrapping_neg();
                'l1: loop {
                    'b1: {
                        let e37 = v3;
                        if (e37 <= e7) {
                        } else {
                            break 'l1;
                        }
                        let e40 = v3;
                        let e44 = v2;
                        let e47 = at0(c, a0.0[0].wrapping_add(e40.wrapping_mul(e11)), a0.0[1].wrapping_add(e44.wrapping_mul(e11)));
                        let e50 = (Vn([e47.0[0], e47.0[1], e47.0[2]]) - Vn([e3.0[0], e3.0[1], e3.0[2]]));
                        let e53 = v3;
                        let e54 = v3;
                        let e56 = v2;
                        let e57 = v2;
                        let e63 = f32::exp(((e50.dot(e50) * (-0.5f32 / f32::max(e14, 1e-6f32))) + ((e53.wrapping_mul(e54).wrapping_add(e56.wrapping_mul(e57)) as f32) * (-0.5f32 / f32::max(e21, 1e-6f32)))));
                        let e64 = v0;
                        v0 = (e64 + (e47 * Vn([e63; 4])));
                        let e67 = v1;
                        v1 = (e67 + e63);
                    }
                    let e70 = v3;
                    v3 = e70.wrapping_add(1i32);
                }
            }
            let e73 = v2;
            v2 = e73.wrapping_add(1i32);
        }
        let e75 = v0;
        let e76 = v1;
        return (e75 * Vn([(1.0f32 / e76); 4]));
    }

    #[inline]
    pub fn lo4(c: &C<'_>, a0: Vn<f32, 4>, a1: Vn<f32, 4>) -> Vn<f32, 4> {
        return a0.zip(a1, f32::min);
    }

    #[inline]
    pub fn mid3(c: &C<'_>, a0: Vn<f32, 4>, a1: Vn<f32, 4>, a2: Vn<f32, 4>) -> Vn<f32, 4> {
        let e3 = lo4(c, a0, a1);
        let e4 = hi4(c, a0, a1);
        let e5 = lo4(c, e4, a2);
        let e6 = hi4(c, e3, e5);
        return e6;
    }

}

mod color {
    use super::shared::*;
    use super::*;
    #[inline]
    pub fn fwd(c: &C<'_>, a0: f32) -> f32 {
        return f32::max(f32::min(a0, (0.5f32 - a0)), 0.0f32);
    }

    pub fn k_bleach(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = straight(c, e3);
        let e5 = luma(c, e4);
        let e6 = Vn([e5; 3]);
        let e24 = ss(c, 0.35f32, 0.65f32, e5);
        let e25 = mix3(c, ((Vn([2.0f32; 3]) * e4) * e6), (Vn([1.0f32; 3]) - ((Vn([2.0f32; 3]) * (Vn([1.0f32; 3]) - e4)) * (Vn([1.0f32; 3]) - e6))), e24);
        let e27 = pf(c, 0u32);
        let e28 = mix3(c, e4, e25, e27);
        let e31 = pf(c, 1u32);
        let e33 = pf(c, 0u32);
        let e34 = mix1(c, 1.0f32, e31, e33);
        let e35 = saturate3(c, e28, e34);
        let e37 = pf(c, 2u32);
        let e38 = contrast3(c, e35, e37);
        let e40 = back(c, e38, e3.0[3]);
        return e40;
    }

    pub fn k_false_color(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let mut v1: Vn<f32, 3> = Vn([0.0f32; 3]);
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = straight(c, e3);
        let e5 = luma(c, e4);
        let e7 = pf(c, 0u32);
        let e10 = pf(c, 1u32);
        let e12 = pf(c, 0u32);
        let e18 = pf(c, 2u32);
        let e19 = (((e5 - e7) / f32::max((e10 - e12), 0.001f32)) * e18);
        v0 = clampf(e19, 0.0f32, 1.0f32);
        let e25 = pf(c, 2u32);
        if (e25 > 1.0f32) {
            let e29 = md(c, e19, 2.0f32);
            v0 = (1.0f32 - f32::abs((e29 - 1.0f32)));
        }
        let e41 = c.p.i[0];
        if (e41 == 0i32) {
            let e46 = v0;
            let e47 = ss(c, 0.15f32, 0.55f32, e46);
            let e50 = v0;
            let e51 = ss(c, 0.45f32, 0.85f32, e50);
            let e54 = v0;
            let e55 = ss(c, 0.0f32, 0.25f32, e54);
            let e58 = v0;
            let e59 = ss(c, 0.3f32, 0.6f32, e58);
            let e63 = v0;
            let e64 = ss(c, 0.85f32, 1.0f32, e63);
            v1 = Vn([e47, e51, ((e55 - e59) + e64)]);
        } else {
            let e70 = c.p.i[0];
            if (e70 == 1i32) {
                let e73 = v0;
                let e81 = v0;
                let e82 = ss(c, 0.0f32, 0.2f32, e81);
                let e87 = hsv(c, (0.75f32 - (0.75f32 * e73)), 0.95f32, (0.25f32 + (0.75f32 * e82)));
                v1 = e87;
            } else {
                let e91 = c.p.i[0];
                if (e91 == 2i32) {
                    let e98 = v0;
                    let e100 = pw(c, e98, 0.8f32);
                    let e108 = v0;
                    let e109 = ss(c, 0.75f32, 1.0f32, e108);
                    v1 = ((Vn([0.12f32, 1.0f32, 0.25f32]) * Vn([e100; 3])) + (Vn([0.6f32, 0.25f32, 0.5f32]) * Vn([e109; 3])));
                } else {
                    let e115 = c.p.i[0];
                    if (e115 == 3i32) {
                        let e128 = v0;
                        let e129 = ss(c, 0.0f32, 0.6f32, e128);
                        let e130 = mix3(c, Vn([0.01f32, 0.03f32, 0.12f32]), Vn([0.1f32, 0.45f32, 0.85f32]), e129);
                        let e137 = v0;
                        let e138 = ss(c, 0.5f32, 1.0f32, e137);
                        let e139 = mix3(c, e130, Vn([0.95f32, 1.0f32, 1.0f32]), e138);
                        v1 = e139;
                    } else {
                        let e150 = v0;
                        let e151 = ss(c, 0.0f32, 0.55f32, e150);
                        let e152 = mix3(c, Vn([0.12f32, 0.02f32, 0.25f32]), Vn([0.9f32, 0.25f32, 0.3f32]), e151);
                        let e159 = v0;
                        let e160 = ss(c, 0.45f32, 1.0f32, e159);
                        let e161 = mix3(c, e152, Vn([1.0f32, 0.88f32, 0.45f32]), e160);
                        v1 = e161;
                    }
                }
            }
        }
        let e162 = v1;
        let e164 = pf(c, 3u32);
        let e165 = mix3(c, e4, e162, e164);
        let e167 = back(c, e165, e3.0[3]);
        return e167;
    }

    pub fn k_fuse(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = straight(c, e3);
        let e5 = luma(c, e4);
        let e8 = clampf(e5, 0.0f32, 1.0f32);
        let e12 = to_hsv(c, e4.zip(Vn([0.0f32; 3]), f32::max));
        let e14 = pf3(c, 0u32);
        let e16 = pf3(c, 3u32);
        let e19 = ss(c, 0.15f32, 0.85f32, e8);
        let e20 = mix3(c, e14, e16, e19);
        let e27 = mix3(c, Vn([e8; 3]), e4, (0.55f32 + (0.45f32 * e12.0[1])));
        let e33 = mix3(c, Vn([1.0f32; 3]), (e20 * Vn([1.8f32; 3])), 0.75f32);
        let e34 = (e27 * e33);
        let e35 = luma(c, e34);
        let e41 = pf(c, 6u32);
        let e42 = mix3(c, e4, (e34 * Vn([(e8 / f32::max(e35, 0.02f32)); 3])), e41);
        let e44 = back(c, e42, e3.0[3]);
        let e47 = at1(c, a0.0[0], a0.0[1]);
        let e50 = pf(c, 7u32);
        let e53 = lit(c, e44, (Vn([e47.0[0], e47.0[1], e47.0[2]]) * Vn([e50; 3])), 1i32);
        return e53;
    }

    pub fn k_gamma(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = straight(c, e3);
        let e6 = pf3(c, 3u32);
        let e9 = pf3(c, 6u32);
        let e14 = ((e4 * e6) + (e9 * (Vn([1.0f32; 3]) - e4)));
        let e17 = pf(c, 0u32);
        let e18 = pw(c, e14.0[0], e17);
        let e21 = pf(c, 1u32);
        let e22 = pw(c, e14.0[1], e21);
        let e25 = pf(c, 2u32);
        let e26 = pw(c, e14.0[2], e25);
        let e29 = back(c, Vn([e18, e22, e26]), e3.0[3]);
        return e29;
    }

    pub fn k_hue(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = straight(c, e3);
        let e6 = pf(c, 0u32);
        let e7 = hue_turn(c, e4, e6);
        v0 = e7;
        let e10 = v0.0[0];
        let e12 = v0.0[1];
        let e15 = v0.0[2];
        let e18 = v0.0[0];
        let e20 = v0.0[1];
        let e23 = v0.0[2];
        let e31 = v0;
        let e33 = pf(c, 1u32);
        let e35 = pf(c, 2u32);
        let e42 = saturate3(c, e31, f32::max((e33 * (1.0f32 + (e35 * (1.0f32 - clampf((f32::max(f32::max(e10, e12), e15) - f32::min(f32::min(e18, e20), e23)), 0.0f32, 1.0f32))))), 0.0f32));
        v0 = e42;
        let e43 = v0;
        let e45 = pf3(c, 5u32);
        let e46 = v0;
        let e47 = luma(c, e46);
        let e50 = pf(c, 8u32);
        let e51 = mix3(c, e43, (e45 * Vn([e47; 3])), e50);
        v0 = e51;
        let e52 = v0;
        let e54 = pf(c, 3u32);
        let e57 = pf(c, 4u32);
        let e61 = back(c, ((e52 * Vn([e54; 3])) + Vn([e57; 3])), e3.0[3]);
        return e61;
    }

    pub fn k_limit(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let mut v1: f32 = 1.0f32;
        let mut v2: bool = false;
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = straight(c, e3);
        let e5 = luma(c, e4);
        let e7 = pf(c, 3u32);
        let e9 = f32::max(e7, 0.0001f32);
        v0 = e5;
        let e11 = v0;
        let e13 = pf(c, 0u32);
        let e15 = (e11 - (e13 - e9));
        if (e15 > 0.0f32) {
            let e19 = pf(c, 0u32);
            v0 = ((e19 - e9) + (e9 * (1.0f32 - f32::exp(((-e15) / e9)))));
        }
        let e29 = pf(c, 1u32);
        let e31 = v0;
        let e32 = ((e29 + e9) - e31);
        if (e32 > 0.0f32) {
            let e36 = pf(c, 1u32);
            v0 = ((e36 + e9) - (e9 * (1.0f32 - f32::exp(((-e32) / e9)))));
        }
        let e46 = (e4 - Vn([e5; 3]));
        let e54 = f32::max(f32::max(f32::abs(e46.0[0]), f32::abs(e46.0[1])), f32::abs(e46.0[2]));
        let e58 = pf(c, 2u32);
        if (e54 > (e58 - e9)) {
            v2 = (e54 > 1e-6f32);
        } else {
            v2 = false;
        }
        let e66 = v2;
        if e66 {
            let e68 = pf(c, 2u32);
            let e71 = pf(c, 2u32);
            v1 = (((e68 - e9) + (e9 * (1.0f32 - f32::exp(((-((e54 - e71) + e9)) / e9))))) / e54);
        }
        let e82 = v0;
        let e84 = v1;
        let e90 = back(c, (Vn([e82; 3]) + (e46 * Vn([f32::max(e84, 0.0f32); 3]))), e3.0[3]);
        return e90;
    }

    pub fn k_matrix(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = straight(c, e3);
        let e6 = pf3(c, 0u32);
        let e9 = pf(c, 3u32);
        let e12 = pf3(c, 4u32);
        let e15 = pf(c, 7u32);
        let e18 = pf3(c, 8u32);
        let e21 = pf(c, 11u32);
        let e25 = pf(c, 15u32);
        v0 = (Vn([(e4.dot(e6) + e9), (e4.dot(e12) + e15), (e4.dot(e18) + e21)]) * Vn([e25; 3]));
        let e28 = v0;
        let e30 = pf(c, 12u32);
        let e31 = contrast3(c, e28, e30);
        v0 = e31;
        let e32 = v0;
        let e34 = pf(c, 13u32);
        let e35 = saturate3(c, e32, e34);
        v0 = e35;
        let e37 = pf(c, 14u32);
        if (e37 != 1.0f32) {
            let e40 = v0;
            let e42 = pf(c, 14u32);
            let e43 = pw3(c, e40, e42);
            v0 = e43;
        }
        let e44 = v0;
        let e46 = back(c, e44, e3.0[3]);
        return e46;
    }

    pub fn k_mist(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = pos(c, a0);
        let e6 = pf2(c, 2u32);
        let e8 = pf2(c, 0u32);
        let e9 = (e6 - e8);
        let e11 = pf2(c, 0u32);
        v0 = clampf(((e4 - e11).dot(e9) / f32::max(e9.dot(e9), 1e-6f32)), 0.0f32, 1.0f32);
        let e23 = pf(c, 9u32);
        if (e23 > 0.0f32) {
            let e27 = pf(c, 10u32);
            let e34 = c.p.time;
            let e36 = pf(c, 11u32);
            let e40 = ((e4 / Vn([f32::max(e27, 0.001f32); 2])) + Vn([(e34 * e36), 0.0f32]));
            let e43 = c.p.time;
            let e50 = c.p.seed;
            let e51 = fbm(c, Vn([e40.0[0], e40.0[1], (e43 * 0.15f32)]), 4i32, e50);
            let e52 = v0;
            let e54 = pf(c, 9u32);
            v0 = clampf((e52 * (1.0f32 + ((e51 * e54) * 1.6f32))), 0.0f32, 1.0f32);
        }
        let e64 = v0;
        let e65 = v0;
        let e68 = v0;
        let e74 = pf(c, 8u32);
        let e75 = mix1(c, e64, (1.0f32 - f32::exp(((-3.0f32 * e65) * e68))), e74);
        let e77 = pf(c, 7u32);
        let e81 = pf3(c, 4u32);
        let e87 = (Vn([e3.0[0], e3.0[1], e3.0[2]]) + (((e81 * Vn([e3.0[3]; 3])) - Vn([e3.0[0], e3.0[1], e3.0[2]])) * Vn([(e75 * e77); 3])));
        return Vn([e87.0[0], e87.0[1], e87.0[2], e3.0[3]]);
    }

    pub fn k_mono(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = straight(c, e3);
        let e6 = pf3(c, 0u32);
        let e9 = pf(c, 3u32);
        let e13 = pf(c, 4u32);
        let e14 = contrast3(c, Vn([(e4.dot(e6) * e9); 3]), e13);
        let e16 = pf3(c, 5u32);
        let e19 = pf(c, 8u32);
        let e20 = mix3(c, e14, (e14 * e16), e19);
        let e22 = back(c, e20, e3.0[3]);
        return e22;
    }

    pub fn k_negative(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = straight(c, e3);
        let e5 = luma(c, e4);
        let e9 = c.p.i[0];
        if (e9 == 1i32) {
            let e19 = back(c, (e4 + Vn([(1.0f32 - (2.0f32 * e5)); 3])), e3.0[3]);
            return e19;
        }
        let e23 = c.p.i[0];
        if (e23 == 2i32) {
            let e31 = back(c, (Vn([(2.0f32 * e5); 3]) - e4), e3.0[3]);
            return e31;
        }
        let e36 = mix3(c, e4, (Vn([1.0f32; 3]) - e4), 1.0f32);
        let e38 = pf3(c, 0u32);
        let e43 = pf3(c, 0u32);
        let e48 = back(c, ((e36 * e38) + (e4 * (Vn([1.0f32; 3]) - e43))), e3.0[3]);
        return e48;
    }

    pub fn k_only_bright(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = straight(c, e3);
        let e6 = pf(c, 0u32);
        let e8 = pf(c, 1u32);
        let e11 = pf(c, 0u32);
        let e13 = pf(c, 1u32);
        let e17 = luma(c, e4);
        let e18 = ss(c, (e6 - e8), ((e11 + e13) + 0.0001f32), e17);
        let e20 = pf(c, 2u32);
        let e21 = saturate3(c, e4, e20);
        let e23 = pf(c, 3u32);
        let e24 = (e21 * Vn([e23; 3]));
        let e28 = c.p.i[0];
        if (e28 == 1i32) {
            let e33 = back(c, e24, (e3.0[3] * e18));
            return e33;
        }
        let e36 = back(c, (e24 * Vn([e18; 3])), e3.0[3]);
        return e36;
    }

    pub fn k_posterize(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = straight(c, e3);
        let e6 = pf(c, 0u32);
        let e10 = f32::max((e6 - 1.0f32), 1.0f32);
        let e14 = c.p.i[0];
        if (e14 == 1i32) {
            let e17 = luma(c, e4);
            let e19 = pf(c, 1u32);
            let e20 = posterize1(c, e17, e10, e19);
            let e26 = back(c, (e4 * Vn([(e20 / f32::max(e17, 0.001f32)); 3])), e3.0[3]);
            return e26;
        }
        let e29 = pf(c, 1u32);
        let e30 = posterize1(c, e4.0[0], e10, e29);
        let e33 = pf(c, 1u32);
        let e34 = posterize1(c, e4.0[1], e10, e33);
        let e37 = pf(c, 1u32);
        let e38 = posterize1(c, e4.0[2], e10, e37);
        let e41 = back(c, Vn([e30, e34, e38]), e3.0[3]);
        return e41;
    }

    pub fn k_print(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = straight(c, e3);
        let e6 = pf(c, 0u32);
        let e7 = contrast3(c, e4, e6);
        v0 = e7;
        let e10 = pf(c, 1u32);
        let e11 = v0;
        let e18 = (e11 - Vn([(1.0f32 - e10); 3])).zip(Vn([0.0f32; 3]), f32::max);
        let e19 = v0;
        v0 = ((e19 - e18) + (e18 / (Vn([1.0f32; 3]) + ((e18 / Vn([f32::max(e10, 0.001f32); 3])) * Vn([0.9f32; 3])))));
        let e32 = v0;
        let e36 = mix3(c, e32, Vn([1.0f32; 3]), 0.0f32);
        let e38 = pf(c, 2u32);
        let e43 = pf(c, 2u32);
        v0 = ((e36 * Vn([(1.0f32 - e38); 3])) + Vn([e43; 3]));
        let e46 = v0;
        let e48 = pf(c, 3u32);
        let e49 = saturate3(c, e46, e48);
        v0 = e49;
        let e50 = v0;
        let e51 = luma(c, e50);
        let e54 = clampf(e51, 0.0f32, 1.0f32);
        let e55 = v0;
        let e57 = pf3(c, 4u32);
        v0 = (e55 + ((((e57 - Vn([0.5f32; 3])) * Vn([(1.0f32 - e54); 3])) * Vn([(1.0f32 - e54); 3])) * Vn([0.5f32; 3])));
        let e70 = v0;
        let e72 = pf3(c, 7u32);
        v0 = (e70 + ((((e72 - Vn([0.5f32; 3])) * Vn([e54; 3])) * Vn([e54; 3])) * Vn([0.5f32; 3])));
        let e81 = v0;
        let e83 = pf(c, 10u32);
        let e84 = mix3(c, e4, e81, e83);
        let e86 = back(c, e84, e3.0[3]);
        return e86;
    }

    pub fn k_solarize(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = straight(c, e3);
        let e6 = solar1(c, e4.0[0]);
        let e8 = solar1(c, e4.0[1]);
        let e10 = solar1(c, e4.0[2]);
        let e13 = pf(c, 2u32);
        let e14 = mix3(c, e4, Vn([e6, e8, e10]), e13);
        let e16 = back(c, e14, e3.0[3]);
        return e16;
    }

    pub fn k_split_tint(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = straight(c, e3);
        let e6 = pf(c, 9u32);
        let e7 = saturate3(c, e4, e6);
        let e8 = luma(c, e7);
        let e10 = pf(c, 8u32);
        let e14 = pf(c, 8u32);
        let e17 = ss(c, (e10 - 0.35f32), (e14 + 0.35f32), e8);
        let e21 = pf3(c, 0u32);
        let e29 = pf(c, 6u32);
        let e30 = mix3(c, e7, ((e7 * Vn([0.45f32; 3])) + (e21 * Vn([(0.18f32 + (e8 * 0.75f32)); 3]))), e29);
        let e34 = pf3(c, 3u32);
        let e40 = pf(c, 7u32);
        let e41 = mix3(c, e7, ((e7 * Vn([0.45f32; 3])) + ((e34 * Vn([e8; 3])) * Vn([0.62f32; 3]))), e40);
        let e42 = mix3(c, e30, e41, e17);
        let e44 = back(c, e42, e3.0[3]);
        return e44;
    }

    pub fn k_threshold(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = straight(c, e3);
        let e6 = pf(c, 1u32);
        let e8 = f32::max(e6, 0.002f32);
        let e9 = luma(c, e4);
        let e11 = pf(c, 0u32);
        let e14 = pf(c, 0u32);
        let e16 = ss(c, (e11 - e8), (e14 + e8), e9);
        v0 = Vn([e16; 3]);
        let e22 = c.p.i[0];
        if (e22 == 1i32) {
            let e26 = pf(c, 0u32);
            let e29 = pf(c, 0u32);
            let e32 = ss(c, (e26 - e8), (e29 + e8), e4.0[0]);
            let e34 = pf(c, 0u32);
            let e37 = pf(c, 0u32);
            let e40 = ss(c, (e34 - e8), (e37 + e8), e4.0[1]);
            let e42 = pf(c, 0u32);
            let e45 = pf(c, 0u32);
            let e48 = ss(c, (e42 - e8), (e45 + e8), e4.0[2]);
            v0 = Vn([e32, e40, e48]);
        }
        let e50 = v0;
        let e53 = v0;
        let e56 = pf(c, 2u32);
        let e57 = mix3(c, e50, (Vn([1.0f32; 3]) - e53), e56);
        v0 = e57;
        let e58 = v0;
        let e59 = v0;
        let e62 = pf(c, 3u32);
        let e63 = mix3(c, e58, (e59 * e4), e62);
        let e65 = back(c, e63, e3.0[3]);
        return e65;
    }

    pub fn k_tones(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let mut v1: i32 = 1i32;
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = straight(c, e3);
        let e8 = c.p.i[0];
        let e9 = luma(c, e4);
        let e14 = pf(c, 16u32);
        let e15 = pw(c, clampf(e9, 0.0f32, 1.0f32), e14);
        let e21 = pf3(c, 0u32);
        v0 = e21;
        let e24 = pf(c, 15u32);
        let e29 = (clampf(e24, 0.001f32, 1.0f32) * 0.5f32);
        'l0: loop {
            'b0: {
                let e32 = v1;
                if (e32 < e8) {
                } else {
                    break 'l0;
                }
                let e38 = v1;
                let e43 = ss(c, (0.5f32 - e29), (0.5f32 + e29), ((e15 * (e8.wrapping_sub(1i32) as f32)) - (e38.wrapping_sub(1i32) as f32)));
                let e44 = v0;
                let e45 = v1;
                let e49 = pf3(c, (e45 as u32).wrapping_mul(3u32));
                let e50 = mix3(c, e44, e49, e43);
                v0 = e50;
            }
            let e52 = v1;
            v1 = e52.wrapping_add(1i32);
        }
        let e54 = luma(c, e4);
        let e55 = v0;
        let e56 = luma(c, e55);
        let e60 = v0;
        let e61 = v0;
        let e66 = pf(c, 18u32);
        let e67 = mix3(c, e60, (e61 * Vn([f32::min((e54 / f32::max(e56, 0.02f32)), 4.0f32); 3])), e66);
        v0 = e67;
        let e68 = v0;
        let e70 = pf(c, 17u32);
        let e71 = mix3(c, e68, e4, e70);
        let e73 = back(c, e71, e3.0[3]);
        return e73;
    }

    pub fn k_zebra(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let mut v1: bool = false;
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = straight(c, e3);
        let e5 = pos(c, a0);
        let e10 = pf(c, 2u32);
        let e16 = c.p.time;
        let e18 = pf(c, 3u32);
        let e21 = fr(c, (((e5.0[0] + e5.0[1]) / f32::max(e10, 0.0001f32)) - (e16 * e18)));
        let e22 = fwd(c, e21);
        let e23 = px(c);
        let e25 = pf(c, 2u32);
        let e33 = f32::min((e22 / f32::max((e23 / f32::max(e25, 0.0001f32)), 0.0001f32)), 1.0f32);
        let e34 = luma(c, e4);
        let e36 = (e4 - Vn([e34; 3]));
        v0 = e4;
        let e47 = pf(c, 0u32);
        if (e34 > e47) {
            let e53 = mix3(c, e4, Vn([1.0f32, 0.15f32, 0.1f32]), e33);
            v0 = e53;
        } else {
            let e55 = pf(c, 1u32);
            if (e34 < e55) {
                let e61 = mix3(c, e4, Vn([0.1f32, 0.35f32, 1.0f32]), e33);
                v0 = e61;
            } else {
                let e65 = c.p.i[0];
                if (e65 == 1i32) {
                    let e71 = pf(c, 4u32);
                    v1 = (f32::max(f32::max(f32::abs(e36.0[0]), f32::abs(e36.0[1])), f32::abs(e36.0[2])) > e71);
                } else {
                    v1 = false;
                }
                let e74 = v1;
                if e74 {
                    let e79 = mix3(c, e4, Vn([0.1f32, 1.0f32, 0.3f32]), e33);
                    v0 = e79;
                }
            }
        }
        let e80 = v0;
        let e82 = back(c, e80, e3.0[3]);
        return e82;
    }

    #[inline]
    pub fn posterize1(c: &C<'_>, a0: f32, a1: f32, a2: f32) -> f32 {
        let e3 = (a0 * a1);
        let e4 = f32::floor(e3);
        let e9 = (clampf(a2, 0.01f32, 1.0f32) * 0.5f32);
        let e15 = ss(c, (0.5f32 - e9), (0.5f32 + e9), (e3 - e4));
        return ((e4 + e15) / a1);
    }

    #[inline]
    pub fn solar1(c: &C<'_>, a0: f32) -> f32 {
        let e2 = pf(c, 0u32);
        let e7 = pf(c, 1u32);
        let e8 = ((a0 / f32::max(e2, 0.02f32)) * e7);
        return f32::min((1.0f32 - f32::abs(((e8 - (2.0f32 * f32::floor((e8 * 0.5f32)))) - 1.0f32))), 1.0f32);
    }

}

mod distort {
    use super::shared::*;
    use super::*;
    #[inline]
    pub fn bent(c: &C<'_>, a0: Vn<f32, 2>, a1: Vn<f32, 2>, a2: f32, a3: i32) -> Vn<f32, 4> {
        if (a2 == 0.0f32) {
            let e6 = pic0(c, a1, a3);
            return e6;
        }
        let e8 = ((a1 - a0) * Vn([a2; 2]));
        let e10 = pic0(c, (a1 + e8), a3);
        let e11 = pic0(c, a1, a3);
        let e13 = pic0(c, (a1 - e8), a3);
        return Vn([e10.0[0], e11.0[1], e13.0[2], f32::max(f32::max(e10.0[3], e11.0[3]), e13.0[3])]);
    }

    pub fn k_displace(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 2> = Vn([0.0f32; 2]);
        let mut v1: Vn<f32, 4> = Vn([0.0f32; 4]);
        let mut v2: i32 = 0i32;
        let e1 = pos(c, a0);
        let e2 = slope1(c, a0);
        let e4 = (e2 * Vn([0.05f32; 2]));
        v0 = (e4 / Vn([(1.0f32 + (e4.length() * 2.5f32)); 2]));
        let e14 = pf(c, 3u32);
        if (e14 > 0.5f32) {
            let e19 = at1(c, a0.0[0], a0.0[1]);
            let e21 = luma(c, Vn([e19.0[0], e19.0[1], e19.0[2]]));
            v0 = Vn([(e21 - 0.5f32), 0.0f32]);
        }
        let e26 = v0;
        let e28 = pf(c, 1u32);
        let e29 = turn(c, e26, e28);
        let e31 = pf(c, 0u32);
        v0 = (e29 * Vn([e31; 2]));
        let e36 = c.p.i[1];
        let e38 = i32_max(e36, 1i32);
        'l0: loop {
            'b0: {
                let e44 = v2;
                if (e44 < e38) {
                } else {
                    break 'l0;
                }
                let e47 = pf(c, 4u32);
                let e48 = v2;
                let e59 = v1;
                let e60 = v0;
                let e64 = pf(c, 2u32);
                let e68 = c.p.i[0];
                let e69 = bent(c, e1, (e1 + (e60 * Vn([(1.0f32 + (e47 * ((((e48 as f32) + 0.5f32) / (e38 as f32)) - 0.5f32))); 2]))), e64, e68);
                v1 = (e59 + e69);
            }
            let e72 = v2;
            v2 = e72.wrapping_add(1i32);
        }
        let e74 = v1;
        return (e74 * Vn([(1.0f32 / (e38 as f32)); 4]));
    }

    pub fn k_drops(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 4> = Vn([0.0f32; 4]);
        let mut v1: i32 = 0i32;
        let mut v2: Vn<f32, 4> = Vn([0.0f32; 4]);
        let e1 = pos(c, a0);
        let e4 = at0(c, a0.0[0], a0.0[1]);
        let e7 = at1(c, a0.0[0], a0.0[1]);
        let e9 = pf(c, 5u32);
        let e10 = mix4(c, e4, e7, e9);
        v0 = e10;
        'l0: loop {
            'b0: {
                let e14 = v1;
                if (e14 < 3i32) {
                } else {
                    break 'l0;
                }
                let e18 = pf(c, 0u32);
                let e19 = v1;
                let e25 = (e18 * (1.0f32 - (0.28f32 * (e19 as f32))));
                let e30 = v1;
                let e34 = v1;
                let e39 = ((e1 / Vn([f32::max(e25, 0.001f32); 2])) + Vn([((e30 as f32) * 3.7f32), ((e34 as f32) * 1.9f32)]));
                let e41 = f32::floor(e39.0[0]);
                let e44 = c.p.time;
                let e46 = pf(c, 3u32);
                let e54 = c.p.seed;
                let e55 = v1;
                let e60 = rnd1(c, (e41 as i32), e54.wrapping_add((e55 as u32).wrapping_mul(31u32)));
                let e65 = (e39.0[1] - (((e44 * e46) / f32::max(e25, 0.001f32)) * (0.5f32 + e60)));
                let e66 = f32::floor(e65);
                let e71 = c.p.seed;
                let e72 = v1;
                let e77 = rnd(c, (e41 as i32), (e66 as i32), e71.wrapping_add((e72 as u32).wrapping_mul(101u32)));
                let e79 = pf(c, 1u32);
                if (e77 < e79) {
                    let e85 = c.p.seed;
                    let e86 = v1;
                    let e93 = rnd2(c, (e41 as i32), (e66 as i32), e85.wrapping_add((e86 as u32).wrapping_mul(211u32)).wrapping_add(5u32));
                    let e98 = c.p.seed;
                    let e99 = v1;
                    let e106 = rnd(c, (e41 as i32), (e66 as i32), e98.wrapping_add((e99 as u32).wrapping_mul(17u32)).wrapping_add(3u32));
                    let e110 = (0.18f32 + (0.24f32 * e106));
                    let e132 = ((Vn([e39.0[0], e65]) - Vn([((e41 + e110) + ((1.0f32 - (2.0f32 * e110)) * e93.0[0])), ((e66 + e110) + ((1.0f32 - (2.0f32 * e110)) * e93.0[1]))])) / Vn([e110; 2]));
                    let e138 = Vn([e132.0[0], (e132.0[1] * 0.92f32)]).length();
                    let e139 = px(c);
                    let e149 = ss(c, (1.0f32 - ((e139 / f32::max((e25 * e110), 0.0001f32)) * 1.5f32)), 1.0f32, e138);
                    let e151 = (1.0f32 - e149);
                    if (e151 > 0.0f32) {
                        let e159 = f32::sqrt(f32::max((1.0f32 - (e138 * e138)), 0.0f32));
                        let e162 = pf(c, 2u32);
                        let e170 = pic0(c, (e1 + (e132 * Vn([((e25 * e110) * (e162 * (e159 - 1.6f32))); 2]))), 2i32);
                        v2 = e170;
                        let e173 = pf(c, 4u32);
                        let e174 = (e132 * Vn([e138; 2]));
                        let e184 = pw(c, f32::max(Vn([e174.0[0], e174.0[1], e159]).dot(Vn([-0.45341346f32, -0.65493053f32, 0.6045513f32])), 0.0f32), 18.0f32);
                        let e186 = v2;
                        let e194 = ((Vn([e186.0[0], e186.0[1], e186.0[2]]) * Vn([(0.72f32 + (0.38f32 * e159)); 3])) + Vn([(e173 * e184); 3]));
                        let e196 = v2.0[3];
                        v2 = Vn([e194.0[0], e194.0[1], e194.0[2], e196]);
                        let e198 = v0;
                        let e199 = v2;
                        let e200 = mix4(c, e198, e199, e151);
                        v0 = e200;
                    }
                }
            }
            let e202 = v1;
            v1 = e202.wrapping_add(1i32);
        }
        let e204 = v0;
        return e204;
    }

    pub fn k_echoes(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 4> = Vn([0.0f32; 4]);
        let mut v1: i32 = 0i32;
        let mut v2: i32 = 0i32;
        let mut v3: bool = false;
        let mut v4: i32 = 0i32;
        let mut v5: bool = false;
        let mut v6: Vn<f32, 4> = Vn([0.0f32; 4]);
        let e1 = pos(c, a0);
        let e8 = c.p.i[1];
        let e10 = i32_max(e8, 1i32);
        'l0: loop {
            'b0: {
                let e13 = v1;
                if (e13 < e10) {
                } else {
                    break 'l0;
                }
                let e15 = v1;
                v2 = e15;
                let e18 = pf(c, 7u32);
                if (e18 > 0.5f32) {
                    let e24 = pf(c, 7u32);
                    v3 = (e24 < 1.5f32);
                } else {
                    v3 = false;
                }
                let e28 = v3;
                if e28 {
                    let e31 = v1;
                    v2 = e10.wrapping_sub(1i32).wrapping_sub(e31);
                }
                let e33 = v2;
                let e34 = (e33 as f32);
                let e36 = pf(c, 2u32);
                let e43 = pf2(c, 0u32);
                let e46 = pf2(c, 4u32);
                let e50 = pf(c, 3u32);
                let e53 = turn(c, ((e1 - e43) - (e46 * Vn([e34; 2]))), ((-e50) * e34));
                let e59 = c.p.i[0];
                v4 = e59;
                let e61 = v2;
                if (e61 > 0i32) {
                    let e66 = v4;
                    v5 = (e66 == 1i32);
                } else {
                    v5 = false;
                }
                let e70 = v5;
                if e70 {
                    v4 = 0i32;
                }
                let e73 = pf2(c, 0u32);
                let e75 = v4;
                let e76 = pic0(c, (e73 + (e53 / Vn([f32::exp((e34 * f32::ln(f32::max(e36, 0.001f32)))); 2]))), e75);
                v6 = e76;
                let e79 = pf(c, 8u32);
                if (e79 != 0.0f32) {
                    let e82 = v6;
                    let e85 = pf(c, 8u32);
                    let e87 = hue_turn(c, Vn([e82.0[0], e82.0[1], e82.0[2]]), (e85 * e34));
                    let e89 = v6.0[3];
                    v6 = Vn([e87.0[0], e87.0[1], e87.0[2], e89]);
                }
                let e92 = pf(c, 6u32);
                let e93 = pw(c, e92, e34);
                let e94 = v2;
                if (e94 == 0i32) {
                    let e97 = v6;
                    v6 = (e97 * Vn([1.0f32; 4]));
                } else {
                    let e100 = v6;
                    v6 = (e100 * Vn([e93; 4]));
                }
                let e103 = pf(c, 7u32);
                if (e103 > 2.5f32) {
                    let e106 = v0;
                    let e107 = v6;
                    v0 = e106.zip(e107, f32::max);
                } else {
                    let e110 = pf(c, 7u32);
                    if (e110 > 1.5f32) {
                        let e113 = v0;
                        let e115 = v6;
                        let e117 = (Vn([e113.0[0], e113.0[1], e113.0[2]]) + Vn([e115.0[0], e115.0[1], e115.0[2]]));
                        let e119 = v0.0[3];
                        let e121 = v6.0[3];
                        let e123 = v0.0[3];
                        v0 = Vn([e117.0[0], e117.0[1], e117.0[2], (e119 + (e121 * (1.0f32 - e123)))]);
                    } else {
                        let e129 = v6;
                        let e130 = v0;
                        let e132 = v6.0[3];
                        v0 = (e129 + (e130 * Vn([(1.0f32 - e132); 4])));
                    }
                }
            }
            let e138 = v1;
            v1 = e138.wrapping_add(1i32);
        }
        let e140 = v0;
        return e140;
    }

    pub fn k_endless(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 4> = Vn([0.0f32; 4]);
        let mut v1: i32 = (-1i32);
        let e1 = pos(c, a0);
        let e3 = pf2(c, 0u32);
        let e6 = pf(c, 2u32);
        let e10 = pf(c, 3u32);
        let e11 = fr(c, e10);
        'l0: loop {
            'b0: {
                let e17 = v1;
                let e21 = c.p.i[1];
                if (e17 < e21) {
                } else {
                    break 'l0;
                }
                let e23 = v1;
                let e29 = f32::exp(((-((e23 as f32) - e11)) * f32::ln(f32::max(e6, 1.05f32))));
                let e31 = pf(c, 4u32);
                let e33 = v1;
                let e37 = turn(c, (e1 - e3), ((-e31) * ((e33 as f32) - e11)));
                let e41 = pf2(c, 0u32);
                let e43 = uv_at(c, ((e37 / Vn([e29; 2])) + e41));
                let e51 = c.p.shape;
                let e60 = pf(c, 5u32);
                let e61 = px(c);
                let e67 = ss(c, 0.0f32, f32::max(e60, (e61 / f32::max(e29, 0.001f32))), f32::min((f32::min(e43.0[0], (1.0f32 - e43.0[0])) * e51), f32::min(e43.0[1], (1.0f32 - e43.0[1]))));
                let e68 = v0;
                let e70 = pic0_uv(c, e43, 1i32);
                let e71 = mix4(c, e68, e70, e67);
                v0 = e71;
            }
            let e73 = v1;
            v1 = e73.wrapping_add(1i32);
        }
        let e75 = v0;
        return e75;
    }

    pub fn k_facets(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 2> = Vn([0.0f32; 2]);
        let mut v1: Vn<f32, 2> = Vn([0.0f32; 2]);
        let mut v2: f32 = 0.0f32;
        let mut v3: Vn<f32, 4> = Vn([0.0f32; 4]);
        let e1 = pos(c, a0);
        let e3 = pf(c, 1u32);
        let e5 = turn(c, e1, (-e3));
        let e7 = pf(c, 0u32);
        let e11 = (e5 / Vn([f32::max(e7, 0.001f32); 2]));
        v0 = (e11.map(f32::floor) + Vn([0.5f32; 2]));
        let e17 = v0;
        v1 = (e11 - e17);
        let e21 = v1.0[0];
        let e24 = v1.0[1];
        v2 = (f32::max(f32::abs(e21), f32::abs(e24)) * 2.0f32);
        let e33 = c.p.i[1];
        if (e33 != 1i32) {
            let e38 = Vn([1.0f32, 1.7320508f32]);
            let e43 = ((e11 / e38).map(f32::floor) + Vn([0.5f32; 2]));
            let e51 = (((e11 / e38) - Vn([0.5f32; 2])).map(f32::floor) + Vn([1.0f32; 2]));
            let e53 = (e11 - (e43 * e38));
            let e55 = (e11 - (e51 * e38));
            if (e53.dot(e53) < e55.dot(e55)) {
                v0 = (e43 * e38);
                v1 = e53;
            } else {
                v0 = (e51 * e38);
                v1 = e55;
            }
            let e61 = v1;
            let e62 = e61.map(f32::abs);
            v2 = (f32::max(e62.0[0], ((e62.0[0] * 0.5f32) + (e62.0[1] * 0.8660254f32))) * 2.0f32);
            let e77 = c.p.i[1];
            if (e77 == 2i32) {
                let e80 = v1;
                v2 = (e80.length() * 2.0f32);
            }
        }
        let e85 = pf(c, 3u32);
        let e86 = v2;
        let e87 = v2;
        let e94 = v0;
        let e95 = v1;
        let e98 = pf(c, 2u32);
        let e102 = pf(c, 0u32);
        let e105 = pf(c, 1u32);
        let e106 = turn(c, ((e94 + ((e95 * Vn([(1.0f32 + (e85 * ((e86 * e87) - 1.0f32))); 2])) * Vn([e98; 2]))) * Vn([e102; 2])), e105);
        let e110 = c.p.i[0];
        let e111 = pic0(c, e106, e110);
        v3 = e111;
        let e113 = v3;
        let e116 = pf(c, 5u32);
        let e117 = v2;
        let e119 = v2;
        let e123 = (Vn([e113.0[0], e113.0[1], e113.0[2]]) * Vn([(1.0f32 - ((e116 * e117) * e119)); 3]));
        let e125 = v3.0[3];
        v3 = Vn([e123.0[0], e123.0[1], e123.0[2], e125]);
        let e127 = px(c);
        let e129 = pf(c, 0u32);
        let e134 = ((e127 / f32::max(e129, 0.001f32)) * 2.0f32);
        let e136 = pf(c, 4u32);
        let e141 = pf(c, 4u32);
        let e145 = v2;
        let e146 = ss(c, ((1.0f32 - e136) - e134), ((1.0f32 - e141) + e134), e145);
        let e148 = pf(c, 4u32);
        let e152 = v3;
        let e154 = pf3(c, 6u32);
        let e157 = mix4(c, e152, Vn([e154.0[0], e154.0[1], e154.0[2], 1.0f32]), (e146 * step(0.0001f32, e148)));
        return e157;
    }

    pub fn k_fisheye(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 1.0f32;
        let e1 = pos(c, a0);
        let e4 = pf(c, 0u32);
        let e7 = pf(c, 5u32);
        let e11 = pf(c, 1u32);
        let e13 = Vn([((e1.0[0] - e4) / e7), (e1.0[1] - e11)]);
        let e16 = pf(c, 2u32);
        let e23 = c.p.shape;
        let e26 = ((e13.length() * 2.0f32) / f32::max(e23, 1.0f32));
        if (e16 > 0.0f32) {
            let e30 = (e16 * 1.35f32);
            v0 = (f32::tan(f32::min((e26 * e30), 1.5f32)) / f32::max((e26 * f32::tan(f32::min(e30, 1.5f32))), 1e-5f32));
            if (e26 < 0.0001f32) {
                v0 = (e30 / f32::tan(f32::min(e30, 1.5f32)));
            }
        } else {
            if (e16 < 0.0f32) {
                let e52 = ((-e16) * 1.35f32);
                v0 = (f32::atan((e26 * f32::tan(f32::min(e52, 1.5f32)))) / f32::max((e26 * e52), 1e-5f32));
                if (e26 < 0.0001f32) {
                    v0 = (f32::tan(f32::min(e52, 1.5f32)) / e52);
                }
            }
        }
        let e68 = v0;
        let e71 = pf(c, 3u32);
        let e73 = ((e13 * Vn([e68; 2])) / Vn([e71; 2]));
        let e75 = pf2(c, 0u32);
        let e78 = pf(c, 5u32);
        let e84 = pf(c, 4u32);
        let e90 = c.p.i[0];
        let e91 = bent(c, e1, (e75 + Vn([(e73.0[0] * e78), e73.0[1]])), (e84 * 0.3f32), e90);
        return e91;
    }

    pub fn k_glass(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 2> = Vn([0.0f32; 2]);
        let mut v1: f32 = 0.0f32;
        let mut v2: Vn<f32, 2> = Vn([0.0f32; 2]);
        let e1 = pos(c, a0);
        let e3 = pf(c, 3u32);
        let e5 = turn(c, e1, (-e3));
        let e7 = pf(c, 0u32);
        let e10 = (e5 * Vn([f32::max(e7, 1.0f32); 2]));
        let e19 = c.p.i[1];
        if (e19 == 2i32) {
            let e26 = c.p.seed;
            let e27 = cells(c, e10, 0.9f32, 0.0f32, e26);
            let e36 = c.p.seed;
            let e37 = cells(c, (e10 + Vn([0.04f32, 0.0f32])), 0.9f32, 0.0f32, e36);
            let e46 = c.p.seed;
            let e47 = cells(c, (e10 + Vn([0.0f32, 0.04f32])), 0.9f32, 0.0f32, e46);
            v0 = (Vn([(e37.0[0] - e27.0[0]), (e47.0[0] - e27.0[0])]) * Vn([(0.22f32 / 0.04f32); 2]));
            v1 = (e27.0[0] * 0.6f32);
        } else {
            let e64 = ((e10 - e10.map(f32::floor)) - Vn([0.5f32; 2]));
            v0 = (e64 * (((e64 * e64) * Vn([4.0f32; 2])) - Vn([1.0f32; 2])));
            v1 = f32::max(f32::abs(e64.0[0]), f32::abs(e64.0[1]));
            let e80 = c.p.i[1];
            if (e80 == 1i32) {
                let e84 = v0.0[0];
                v0 = Vn([e84, 0.0f32]);
                v1 = f32::abs(e64.0[0]);
            }
        }
        let e89 = v0;
        let e91 = pf(c, 1u32);
        v2 = (e89 * Vn([e91; 2]));
        let e95 = pf(c, 4u32);
        if (e95 > 0.0f32) {
            let e98 = v2;
            let e100 = (e10 * Vn([6.0f32; 2]));
            let e105 = c.p.seed;
            let e106 = noise3(c, Vn([e100.0[0], e100.0[1], 0.0f32]), e105);
            let e108 = (e10 * Vn([6.0f32; 2]));
            let e113 = c.p.seed;
            let e114 = noise3(c, Vn([e108.0[0], e108.0[1], 9.0f32]), e113);
            let e117 = pf(c, 4u32);
            v2 = (e98 + ((Vn([e106, e114]) * Vn([e117; 2])) * Vn([0.15f32; 2])));
        }
        let e122 = v2;
        let e124 = pf(c, 3u32);
        let e125 = turn(c, e122, e124);
        let e127 = pf(c, 0u32);
        let e134 = pf(c, 5u32);
        let e138 = c.p.i[0];
        let e139 = bent(c, e1, (e1 + (e125 / Vn([f32::max(e127, 1.0f32); 2]))), e134, e138);
        let e141 = pf(c, 2u32);
        let e144 = v1;
        let e145 = ss(c, 0.32f32, 0.5f32, e144);
        let e150 = (Vn([e139.0[0], e139.0[1], e139.0[2]]) * Vn([(1.0f32 - (e141 * e145)); 3]));
        return Vn([e150.0[0], e150.0[1], e150.0[2], e139.0[3]]);
    }

    pub fn k_kaleido(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let mut v1: f32 = 0.0f32;
        let e1 = pos(c, a0);
        let e3 = pf2(c, 0u32);
        let e4 = (e1 - e3);
        let e7 = pf(c, 3u32);
        v0 = (e4.length() / e7);
        let e12 = pf(c, 4u32);
        let e15 = (6.2831855f32 / f32::max(e12, 2.0f32));
        let e16 = angle(c, e4);
        let e18 = pf(c, 2u32);
        v1 = (e16 - e18);
        let e24 = c.p.i[1];
        if (e24 == 2i32) {
            let e27 = v0;
            let e28 = v1;
            let e30 = pf(c, 4u32);
            v0 = (e27 * (0.75f32 + (0.25f32 * f32::cos((e28 * f32::max(e30, 2.0f32))))));
        }
        let e40 = v1;
        let e41 = v1;
        v1 = (e40 - (e15 * f32::floor((e41 / e15))));
        let e46 = v1;
        let e52 = pf(c, 5u32);
        v1 = (f32::abs((e46 - (e15 * 0.5f32))) + e52);
        let e57 = c.p.i[1];
        if (e57 == 1i32) {
            let e60 = v0;
            let e62 = md(c, e60, 0.6f32);
            v0 = f32::abs((e62 - 0.3f32));
        }
        let e67 = pf2(c, 6u32);
        let e68 = v1;
        let e70 = v1;
        let e73 = v0;
        let e79 = c.p.i[0];
        let e80 = pic0(c, (e67 + (Vn([f32::cos(e68), f32::sin(e70)]) * Vn([e73; 2]))), e79);
        return e80;
    }

    pub fn k_magnify(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let mut v1: Vn<f32, 4> = Vn([0.0f32; 4]);
        let e1 = pos(c, a0);
        let e4 = pf(c, 0u32);
        let e8 = pf(c, 1u32);
        let e11 = pf(c, 4u32);
        let e13 = Vn([(e1.0[0] - e4), ((e1.0[1] - e8) / e11)]);
        v0 = e13.length();
        let e19 = c.p.i[1];
        if (e19 == 1i32) {
            v0 = f32::max(f32::abs(e13.0[0]), f32::abs(e13.0[1]));
        }
        let e28 = pf(c, 2u32);
        let e30 = f32::max(e28, 0.0001f32);
        let e31 = v0;
        let e32 = (e31 / e30);
        let e34 = pf(c, 6u32);
        let e35 = px(c);
        let e41 = ss(c, (1.0f32 - f32::max(e34, (e35 / e30))), 1.0f32, e32);
        let e45 = pf(c, 3u32);
        let e50 = pf(c, 5u32);
        let e54 = mix1(c, (1.0f32 / e45), 1.0f32, (((e50 * e32) * e32) * e32));
        let e55 = (e13 * Vn([e54; 2]));
        let e57 = pf2(c, 0u32);
        let e61 = pf(c, 4u32);
        let e66 = pic0(c, e1, 1i32);
        let e68 = mix_v2(c, e1, (e57 + Vn([e55.0[0], (e55.0[1] * e61)])), 1.0f32);
        let e70 = pf(c, 11u32);
        let e77 = c.p.i[0];
        let e78 = bent(c, e1, e68, ((e70 * e32) * 0.15f32), e77);
        let e79 = mix4(c, e66, e78, (1.0f32 - e41));
        v1 = e79;
        let e82 = pf(c, 10u32);
        if (e82 > 0.0f32) {
            let e86 = pf(c, 10u32);
            let e87 = (e86 / e30);
            let e92 = px(c);
            let e100 = ss(c, (e87 * 0.5f32), ((e87 * 0.5f32) + ((e92 / e30) * 1.5f32)), f32::abs((e32 - 1.0f32)));
            let e103 = v1;
            let e105 = pf3(c, 7u32);
            let e108 = mix4(c, e103, Vn([e105.0[0], e105.0[1], e105.0[2], 1.0f32]), (1.0f32 - e100));
            v1 = e108;
        }
        let e109 = v1;
        return e109;
    }

    pub fn k_plane(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 4> = Vn([0.0f32; 4]);
        let mut v1: i32 = 0i32;
        let mut v2: i32 = 0i32;
        let mut v3: bool = false;
        let mut v4: bool = false;
        let e2 = pf(c, 10u32);
        let e5 = i32_max((e2 as i32), 1i32);
        'l0: loop {
            'b0: {
                let e11 = v1;
                if (e11 < e5) {
                } else {
                    break 'l0;
                }
                v2 = 0i32;
                'l1: loop {
                    'b1: {
                        let e15 = v2;
                        if (e15 < e5) {
                        } else {
                            break 'l1;
                        }
                        let e17 = v2;
                        let e19 = v1;
                        let e31 = pos(c, a0);
                        let e32 = px(c);
                        let e34 = (e31 + ((((Vn([(e17 as f32), (e19 as f32)]) + Vn([0.5f32; 2])) / Vn([(e5 as f32); 2])) - Vn([0.5f32; 2])) * Vn([e32; 2])));
                        let e36 = pf(c, 6u32);
                        let e40 = pf(c, 7u32);
                        let e45 = pf(c, 8u32);
                        let e46 = (((e36 * e34.0[0]) + (e40 * e34.0[1])) + e45);
                        if (f32::abs(e46) > 1e-6f32) {
                            if (!(e46 > 0.0f32)) {
                                let e60 = c.p.i[1];
                                v4 = (e60 == 1i32);
                            } else {
                                v4 = true;
                            }
                            let e64 = v4;
                            v3 = e64;
                        } else {
                            v3 = false;
                        }
                        let e66 = v3;
                        if e66 {
                            let e68 = pf(c, 0u32);
                            let e72 = pf(c, 1u32);
                            let e77 = pf(c, 2u32);
                            let e80 = pf(c, 3u32);
                            let e84 = pf(c, 4u32);
                            let e89 = pf(c, 5u32);
                            let e94 = v0;
                            let e96 = pf(c, 9u32);
                            let e100 = c.p.i[0];
                            let e101 = bent(c, e34, (Vn([(((e68 * e34.0[0]) + (e72 * e34.0[1])) + e77), (((e80 * e34.0[0]) + (e84 * e34.0[1])) + e89)]) / Vn([e46; 2])), e96, e100);
                            v0 = (e94 + e101);
                        }
                    }
                    let e104 = v2;
                    v2 = e104.wrapping_add(1i32);
                }
            }
            let e107 = v1;
            v1 = e107.wrapping_add(1i32);
        }
        let e109 = v0;
        return (e109 * Vn([(1.0f32 / (e5.wrapping_mul(e5) as f32)); 4]));
    }

    pub fn k_polar(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: i32 = 0i32;
        let mut v1: f32 = 0.0f32;
        let mut v2: bool = false;
        let e1 = pos(c, a0);
        let e5 = c.p.i[1];
        if (e5 == 1i32) {
            let e8 = uv_of(c, a0);
            let e15 = pf(c, 5u32);
            let e18 = pf(c, 2u32);
            let e19 = ((((e8.0[0] - 0.5f32) * 6.2831855f32) * e15) + e18);
            let e21 = pf(c, 4u32);
            let e23 = pf(c, 3u32);
            let e25 = mix1(c, e21, e23, e8.0[1]);
            let e27 = pf2(c, 0u32);
            let e37 = c.p.i[0];
            let e38 = pic0(c, (e27 + (Vn([f32::sin(e19), (-f32::cos(e19))]) * Vn([e25; 2]))), e37);
            return e38;
        }
        let e40 = pf2(c, 0u32);
        let e41 = (e1 - e40);
        let e42 = e41.length();
        let e47 = angle(c, Vn([(-e41.0[1]), e41.0[0]]));
        let e49 = pf(c, 2u32);
        let e55 = fr(c, (((e47 + e49) / 6.2831855f32) + 0.5f32));
        let e57 = pf(c, 5u32);
        let e58 = (e55 * e57);
        let e60 = pf(c, 3u32);
        let e63 = pf(c, 4u32);
        let e65 = pf(c, 3u32);
        let e71 = (1.0f32 - ((e42 - e60) / f32::max((e63 - e65), 0.0001f32)));
        let e75 = c.p.i[0];
        v0 = e75;
        v1 = e58;
        let e79 = pf(c, 5u32);
        if (!(e79 > 1.0f32)) {
            let e85 = v0;
            v2 = (e85 != 0i32);
        } else {
            v2 = true;
        }
        let e89 = v2;
        if e89 {
            let e91 = md(c, e58, 2.0f32);
            v1 = (1.0f32 - f32::abs((e91 - 1.0f32)));
            let e98 = pf(c, 5u32);
            if (e98 <= 1.0f32) {
                v1 = (1.0f32 - f32::abs(((e58 * 2.0f32) - 1.0f32)));
            }
        }
        let e109 = pf(c, 3u32);
        let e111 = px(c);
        let e119 = pf(c, 4u32);
        let e121 = px(c);
        let e129 = v0;
        if (e129 == 0i32) {
            let e132 = v1;
            let e138 = pic0_uv(c, Vn([e132, clampf(e71, 0.0f32, 1.0f32)]), 1i32);
            return (e138 * Vn([(clampf((((e42 - e109) / e111) + 0.5f32), 0.0f32, 1.0f32) * clampf((((e119 - e42) / e121) + 0.5f32), 0.0f32, 1.0f32)); 4]));
        }
        let e140 = v1;
        let e142 = v0;
        let e143 = pic0_uv(c, Vn([e140, e71]), e142);
        return e143;
    }

    pub fn k_prism(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e1 = pos(c, a0);
        let e3 = pf2(c, 5u32);
        let e4 = (e1 - e3);
        let e7 = pf(c, 0u32);
        let e9 = f32::max(e7, 2.0f32);
        let e10 = angle(c, e4);
        let e12 = pf(c, 2u32);
        let e13 = (e10 - e12);
        let e19 = f32::floor((((e13 / 6.2831855f32) * e9) + 0.5f32));
        let e24 = pf(c, 2u32);
        let e25 = (((e19 / e9) * 6.2831855f32) + e24);
        let e34 = pf(c, 7u32);
        let e36 = pf(c, 7u32);
        let e39 = ss(c, e34, (e36 + 0.06f32), e4.length());
        let e44 = pf(c, 1u32);
        let e49 = pf(c, 3u32);
        let e52 = bent(c, e1, (e1 - ((Vn([f32::cos(e25), f32::sin(e25)]) * Vn([e44; 2])) * Vn([e39; 2]))), (e49 * e39), 2i32);
        let e54 = pf(c, 4u32);
        let e65 = ss(c, 0.9f32, 1.0f32, (f32::abs((((e13 / 6.2831855f32) * e9) - e19)) * 2.0f32));
        let e75 = (Vn([e52.0[0], e52.0[1], e52.0[2]]) * Vn([f32::max((1.0f32 + ((e54 * e39) * ((f32::cos(((e25 * 1.7f32) + 0.6f32)) * 0.6f32) - (e65 * 0.5f32)))), 0.0f32); 3]));
        return Vn([e75.0[0], e75.0[1], e75.0[2], e52.0[3]]);
    }

    pub fn k_rainbow(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let mut v1: Vn<f32, 3> = Vn([0.0f32; 3]);
        let mut v2: f32 = 0.0f32;
        let mut v3: i32 = 0i32;
        let mut v4: Vn<f32, 4> = Vn([0.0f32; 4]);
        let e1 = pos(c, a0);
        let e3 = pf2(c, 0u32);
        let e4 = (e1 - e3);
        let e6 = pf(c, 4u32);
        let e8 = pf(c, 4u32);
        let e12 = ss(c, e6, (e8 + 0.35f32), e4.length());
        let e16 = c.p.i[1];
        let e18 = i32_max(e16, 3i32);
        'l0: loop {
            'b0: {
                let e29 = v3;
                if (e29 < e18) {
                } else {
                    break 'l0;
                }
                let e31 = v3;
                let e36 = (((e31 as f32) + 0.5f32) / (e18 as f32));
                let e64 = Vn([f32::max((1.0f32 - (f32::abs((e36 - 0.14f32)) * 3.4f32)), 0.0f32), f32::max((1.0f32 - (f32::abs((e36 - 0.5f32)) * 3.4f32)), 0.0f32), f32::max((1.0f32 - (f32::abs((e36 - 0.86f32)) * 3.4f32)), 0.0f32)]);
                let e66 = (e36 - 0.5f32);
                v4 = Vn([0.0f32; 4]);
                let e73 = c.p.i[2];
                if (e73 == 1i32) {
                    let e77 = pf2(c, 5u32);
                    let e83 = c.p.i[0];
                    let e84 = pic0(c, (e1 + (e77 * Vn([e66; 2]))), e83);
                    v4 = e84;
                } else {
                    let e86 = pf2(c, 0u32);
                    let e88 = pf(c, 3u32);
                    let e91 = turn(c, e4, ((e88 * e66) * e12));
                    let e93 = pf(c, 2u32);
                    let e103 = c.p.i[0];
                    let e104 = pic0(c, (e86 + (e91 * Vn([(1.0f32 - ((e93 * e66) * e12)); 2]))), e103);
                    v4 = e104;
                }
                let e105 = v0;
                let e106 = v4;
                v0 = (e105 + (Vn([e106.0[0], e106.0[1], e106.0[2]]) * e64));
                let e110 = v1;
                v1 = (e110 + e64);
                let e112 = v2;
                let e114 = v4.0[3];
                v2 = (e112 + e114);
            }
            let e117 = v3;
            v3 = e117.wrapping_add(1i32);
        }
        let e119 = v0;
        let e120 = v1;
        let e121 = (e119 / e120);
        let e122 = v2;
        return Vn([e121.0[0], e121.0[1], e121.0[2], (e122 / (e18 as f32))]);
    }

    pub fn k_rgb_shift(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e1 = pos(c, a0);
        let e3 = pf2(c, 12u32);
        let e4 = (e1 - e3);
        let e6 = pf2(c, 12u32);
        let e8 = pf(c, 9u32);
        let e10 = turn(c, e4, (-e8));
        let e12 = pf(c, 6u32);
        let e17 = pf2(c, 0u32);
        let e22 = c.p.i[0];
        let e23 = pic0(c, ((e6 + (e10 / Vn([e12; 2]))) - e17), e22);
        let e25 = pf(c, 10u32);
        let e27 = turn(c, e4, (-e25));
        let e29 = pf(c, 7u32);
        let e34 = pf2(c, 2u32);
        let e39 = c.p.i[0];
        let e40 = pic0(c, ((e6 + (e27 / Vn([e29; 2]))) - e34), e39);
        let e42 = pf(c, 11u32);
        let e44 = turn(c, e4, (-e42));
        let e46 = pf(c, 8u32);
        let e51 = pf2(c, 4u32);
        let e56 = c.p.i[0];
        let e57 = pic0(c, ((e6 + (e44 / Vn([e46; 2]))) - e51), e56);
        return Vn([e23.0[0], e40.0[1], e57.0[2], f32::max(f32::max(e23.0[3], e40.0[3]), e57.0[3])]);
    }

    pub fn k_ripple(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let e1 = pos(c, a0);
        let e3 = pf2(c, 0u32);
        let e4 = (e1 - e3);
        let e5 = e4.length();
        let e7 = pf(c, 3u32);
        let e12 = pf(c, 4u32);
        let e15 = (((e5 / f32::max(e7, 0.001f32)) - e12) * 6.2831855f32);
        let e18 = pf(c, 5u32);
        v0 = f32::exp(((-e5) * e18));
        let e23 = pf(c, 8u32);
        if (e23 > 0.0f32) {
            let e26 = v0;
            let e28 = pf(c, 8u32);
            let e32 = pf(c, 8u32);
            let e33 = ss(c, (e28 * 0.7f32), e32, e5);
            v0 = (e26 * (1.0f32 - e33));
        }
        let e37 = v0;
        let e40 = pf(c, 3u32);
        let e43 = ss(c, 0.0f32, (e40 * 0.5f32), e5);
        v0 = (e37 * e43);
        let e48 = (e4 / Vn([f32::max(e5, 1e-5f32); 2]));
        let e51 = pf(c, 2u32);
        let e53 = v0;
        let e58 = pf(c, 6u32);
        let e62 = c.p.i[0];
        let e63 = bent(c, e1, (e1 + (e48 * Vn([((f32::cos(e15) * e51) * e53); 2]))), e58, e62);
        let e65 = v0;
        let e68 = pf(c, 7u32);
        let e83 = (Vn([e63.0[0], e63.0[1], e63.0[2]]) * Vn([f32::max((1.0f32 + (((f32::sin(e15) * e65) * e68) * ((e48.0[0] * 0.5f32) - (e48.0[1] * 0.86f32)))), 0.0f32); 3]));
        return Vn([e83.0[0], e83.0[1], e83.0[2], e63.0[3]]);
    }

    pub fn k_stretch_edges(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 2> = Vn([0.0f32; 2]);
        let mut v1: Vn<f32, 4> = Vn([0.0f32; 4]);
        let mut v2: i32 = 0i32;
        let e1 = uv_of(c, a0);
        let e3 = pf2(c, 0u32);
        let e5 = pf2(c, 2u32);
        v0 = e1.zip3(e3, e5, clampf);
        let e11 = c.p.i[1];
        if (e11 == 1i32) {
            let e25 = wrap1(c, ((e1.0[0] - e3.0[0]) / f32::max((e5.0[0] - e3.0[0]), 0.0001f32)), 2i32);
            let e36 = wrap1(c, ((e1.0[1] - e3.0[1]) / f32::max((e5.0[1] - e3.0[1]), 0.0001f32)), 2i32);
            v0 = (e3 + ((e5 - e3) * Vn([e25, e36])));
        }
        let e40 = v0;
        let e42 = pic0_uv(c, e40, 1i32);
        let e46 = c.p.i[1];
        if (e46 == 2i32) {
            let e50 = (e1 - e1.zip3(e3, e5, clampf));
            let e54 = c.p.shape;
            let e58 = Vn([(e50.0[0] * e54), e50.0[1]]).length();
            let e69 = pf(c, 4u32);
            let e70 = (((Vn([e50.0[1], e50.0[0]]) + Vn([1e-6f32, 0.0f32])).normalize() * Vn([e58; 2])) * Vn([e69; 2]));
            'l0: loop {
                'b0: {
                    let e76 = v2;
                    if (e76 < 9i32) {
                    } else {
                        break 'l0;
                    }
                    let e79 = v2;
                    let e85 = v0;
                    let e89 = c.p.shape;
                    let e96 = v1;
                    let e98 = pic0_uv(c, (e85 + (Vn([(e70.0[0] / e89), e70.0[1]]) * Vn([(((e79 as f32) / 4.0f32) - 1.0f32); 2]))).zip3(e3, e5, clampf), 1i32);
                    v1 = (e96 + e98);
                }
                let e101 = v2;
                v2 = e101.wrapping_add(1i32);
            }
            let e103 = v1;
            let e108 = pf(c, 5u32);
            let e111 = ss(c, 0.0f32, f32::max(e108, 0.0001f32), e58);
            let e112 = mix4(c, e42, (e103 * Vn([0.11111111f32; 4])), e111);
            return e112;
        }
        return e42;
    }

    pub fn k_strips(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 1.0f32;
        let e1 = pos(c, a0);
        let e3 = pf(c, 1u32);
        let e5 = turn(c, e1, (-e3));
        let e7 = pf(c, 0u32);
        let e11 = (1.0f32 / f32::max(e7, 1.0f32));
        let e16 = f32::floor(((e5.0[1] / e11) + 0.0001f32));
        let e21 = (((e5.0[1] / e11) - e16) - 0.5f32);
        let e25 = c.p.seed;
        let e26 = rnd1(c, (e16 as i32), e25);
        let e30 = ((e26 * 2.0f32) - 1.0f32);
        let e36 = c.p.i[1];
        if (e36 == 0i32) {
            v0 = e30;
        } else {
            let e42 = c.p.i[1];
            if (e42 == 1i32) {
                let e46 = md(c, e16, 2.0f32);
                v0 = (1.0f32 - (2.0f32 * e46));
                let e51 = v0;
                let e53 = pf(c, 3u32);
                v0 = (e51 * (1.0f32 + (e30 * e53)));
            } else {
                let e62 = pf(c, 3u32);
                v0 = (((e16 * e11) * 2.0f32) + (e30 * e62));
            }
        }
        let e68 = c.p.i[1];
        if (e68 == 0i32) {
            let e73 = pf(c, 3u32);
            let e74 = mix1(c, signf(e30), e30, e73);
            v0 = e74;
        }
        let e75 = v0;
        let e77 = pf(c, 2u32);
        let e79 = pf(c, 6u32);
        let e82 = c.p.time;
        let e87 = pf(c, 4u32);
        let e97 = pf(c, 1u32);
        let e98 = turn(c, Vn([((e5.0[0] * (1.0f32 + (e21 * e87))) - (e75 * (e77 + (e79 * e82)))), e5.0[1]]), e97);
        let e102 = c.p.i[0];
        let e103 = pic0(c, e98, e102);
        let e105 = pf(c, 5u32);
        let e108 = sq(c, (e21 * 2.0f32));
        let e115 = (Vn([e103.0[0], e103.0[1], e103.0[2]]) * Vn([(1.0f32 - ((e105 * e108) * 0.7f32)); 3]));
        return Vn([e115.0[0], e115.0[1], e115.0[2], e103.0[3]]);
    }

    pub fn k_tiles(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 2> = Vn([0.0f32; 2]);
        let mut v1: Vn<f32, 2> = Vn([0.0f32; 2]);
        let e1 = pos(c, a0);
        let e3 = pf(c, 0u32);
        let e7 = (1.0f32 / f32::max(e3, 1.0f32));
        let e9 = pf(c, 1u32);
        let e11 = Vn([(e7 * e9), e7]);
        let e13 = (e1 / e11).map(f32::floor);
        let e16 = c.p.time;
        let e18 = pf(c, 6u32);
        let e26 = c.p.seed;
        let e32 = e26.wrapping_add((i32_max((f32::floor(((e16 * e18) + 0.0001f32)) as i32), 0i32) as u32).wrapping_mul(7919u32));
        let e37 = rnd2(c, (e13.0[0] as i32), (e13.0[1] as i32), e32);
        let e41 = ((e13 + Vn([0.5f32; 2])) * e11);
        let e42 = (e1 - e41);
        let e49 = rnd(c, (e13.0[0] as i32), (e13.0[1] as i32), e32.wrapping_add(11u32));
        let e51 = pf(c, 4u32);
        let e54 = step(e49, (e51 - 0.0001f32));
        let e61 = pf(c, 3u32);
        let e64 = turn(c, e42, ((((e37.0[0] - 0.5f32) * 2.0f32) * e61) * e54));
        v0 = (e41 + e64);
        let e73 = pf(c, 2u32);
        v1 = ((((e37 - Vn([0.5f32; 2])) * Vn([2.0f32; 2])) * Vn([e73; 2])) * Vn([e54; 2]));
        let e80 = c.p.i[1];
        if (e80 == 1i32) {
            let e83 = v1;
            v1 = (((e83 / e11) + Vn([0.5f32; 2])).map(f32::floor) * e11);
        }
        let e90 = v0;
        let e91 = v1;
        v0 = (e90 + e91);
        let e93 = v0;
        let e97 = c.p.i[0];
        let e98 = pic0(c, e93, e97);
        let e113 = pf(c, 5u32);
        let e116 = ((e113 * e7) * 0.5f32);
        let e117 = px(c);
        let e119 = px(c);
        let e121 = ss(c, (e116 - e117), (e116 + e119), f32::min(((e11.0[0] * 0.5f32) - f32::abs(e42.0[0])), ((e11.0[1] * 0.5f32) - f32::abs(e42.0[1]))));
        let e125 = pf(c, 5u32);
        let e130 = pf3(c, 7u32);
        let e133 = mix4(c, e98, Vn([e130.0[0], e130.0[1], e130.0[2], 1.0f32]), ((1.0f32 - e121) * step(0.0001f32, e125)));
        return e133;
    }

    pub fn k_turbulence(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 2> = Vn([0.0f32; 2]);
        let e1 = pos(c, a0);
        let e3 = pf(c, 1u32);
        let e11 = pf(c, 6u32);
        let e16 = pf2(c, 3u32);
        let e19 = c.p.time;
        let e21 = ((Vn([e1.0[0], (e1.0[1] * e11)]) * Vn([(1.0f32 / f32::max(e3, 0.001f32)); 2])) + (e16 * Vn([e19; 2])));
        let e24 = c.p.time;
        let e26 = pf(c, 5u32);
        let e27 = (e24 * e26);
        let e34 = c.p.i[2];
        if (e34 == 1i32) {
            let e42 = c.p.i[1];
            let e45 = c.p.seed;
            let e46 = fbm(c, Vn([e21.0[0], e21.0[1], e27]), e42, e45);
            let e49 = (e21 + Vn([0.05f32, 0.0f32]));
            let e54 = c.p.i[1];
            let e57 = c.p.seed;
            let e58 = fbm(c, Vn([e49.0[0], e49.0[1], e27]), e54, e57);
            let e61 = (e21 + Vn([0.0f32, 0.05f32]));
            let e66 = c.p.i[1];
            let e69 = c.p.seed;
            let e70 = fbm(c, Vn([e61.0[0], e61.0[1], e27]), e66, e69);
            v0 = (Vn([(e70 - e46), (e46 - e58)]) * Vn([(0.35f32 / 0.05f32); 2]));
        } else {
            let e80 = c.p.i[2];
            if (e80 == 2i32) {
                let e87 = c.p.i[1];
                let e90 = c.p.seed;
                let e91 = ridges(c, Vn([e21.0[0], e21.0[1], e27]), e87, e90);
                let e95 = (e21 + Vn([19.1f32, 7.7f32]));
                let e100 = c.p.i[1];
                let e103 = c.p.seed;
                let e106 = ridges(c, Vn([e95.0[0], e95.0[1], e27]), e100, e103.wrapping_add(9u32));
                v0 = (Vn([e91, e106]) - Vn([0.62f32; 2]));
                let e111 = v0;
                v0 = (e111 * Vn([1.6f32; 2]));
            } else {
                let e118 = c.p.i[1];
                let e121 = c.p.seed;
                let e122 = fbm(c, Vn([e21.0[0], e21.0[1], e27]), e118, e121);
                let e126 = (e21 + Vn([31.7f32, 11.3f32]));
                let e131 = c.p.i[1];
                let e134 = c.p.seed;
                let e137 = fbm(c, Vn([e126.0[0], e126.0[1], e27]), e131, e134.wrapping_add(5u32));
                v0 = Vn([e122, e137]);
                let e142 = c.p.i[2];
                if (e142 == 3i32) {
                    let e147 = v0;
                    let e152 = pf2(c, 3u32);
                    let e155 = c.p.time;
                    let e159 = (((e21 * Vn([2.3f32; 2])) + (e147 * Vn([2.0f32; 2]))) - ((e152 * Vn([e155; 2])) * Vn([2.0f32; 2])));
                    let e160 = v0;
                    let e167 = c.p.i[1];
                    let e170 = c.p.seed;
                    let e173 = fbm(c, Vn([e159.0[0], e159.0[1], (e27 * 1.7f32)]), e167, e170.wrapping_add(17u32));
                    let e177 = (e159 + Vn([5.2f32, 9.4f32]));
                    let e184 = c.p.i[1];
                    let e187 = c.p.seed;
                    let e190 = fbm(c, Vn([e177.0[0], e177.0[1], (e27 * 1.7f32)]), e184, e187.wrapping_add(23u32));
                    v0 = (e160 + (Vn([e173, e190]) * Vn([0.6f32; 2])));
                }
            }
        }
        let e195 = v0;
        let e197 = pf(c, 0u32);
        let e201 = pf(c, 2u32);
        let e205 = c.p.i[0];
        let e206 = bent(c, e1, (e1 + (e195 * Vn([e197; 2]))), e201, e205);
        return e206;
    }

    pub fn k_twirl(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e1 = pos(c, a0);
        let e3 = pf2(c, 0u32);
        let e4 = (e1 - e3);
        let e7 = pf(c, 3u32);
        let e17 = pf(c, 4u32);
        let e18 = pw(c, clampf((1.0f32 - (e4.length() / f32::max(e7, 0.0001f32))), 0.0f32, 1.0f32), e17);
        let e20 = pf2(c, 0u32);
        let e22 = pf(c, 2u32);
        let e25 = turn(c, e4, ((-e22) * e18));
        let e27 = pf(c, 6u32);
        let e34 = pf(c, 5u32);
        let e40 = c.p.i[0];
        let e41 = bent(c, e1, (e20 + (e25 * Vn([(1.0f32 + (e27 * e18)); 2]))), (e34 * 0.2f32), e40);
        return e41;
    }

    pub fn k_waves(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 2> = Vn([0.0f32; 2]);
        let e1 = pos(c, a0);
        let e3 = pf(c, 2u32);
        let e5 = turn(c, e1, (-e3));
        let e8 = pf(c, 1u32);
        let e13 = pf(c, 3u32);
        let e15 = wave1(c, ((e5.0[0] / f32::max(e8, 0.001f32)) - e13));
        let e17 = pf(c, 0u32);
        let e18 = (e15 * e17);
        let e21 = pf(c, 5u32);
        let e26 = pf(c, 3u32);
        let e30 = wave1(c, ((e5.0[1] / f32::max(e21, 0.001f32)) - (e26 * 1.31f32)));
        let e32 = pf(c, 4u32);
        let e33 = (e30 * e32);
        v0 = Vn([e33, e18]);
        let e39 = c.p.i[2];
        if (e39 == 1i32) {
            v0 = Vn([e18, e33]);
        }
        let e43 = v0;
        let e45 = pf(c, 2u32);
        let e46 = turn(c, e43, e45);
        let e49 = pf(c, 6u32);
        let e53 = c.p.i[0];
        let e54 = bent(c, e1, (e1 + e46), e49, e53);
        return e54;
    }

    #[inline]
    pub fn mix_v2(c: &C<'_>, a0: Vn<f32, 2>, a1: Vn<f32, 2>, a2: f32) -> Vn<f32, 2> {
        return (a0 + ((a1 - a0) * Vn([a2; 2])));
    }

    #[inline]
    pub fn slope1(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 2> {
        let e5 = at1(c, a0.0[0].wrapping_sub(1i32), a0.0[1]);
        let e7 = luma(c, Vn([e5.0[0], e5.0[1], e5.0[2]]));
        let e12 = at1(c, a0.0[0].wrapping_add(1i32), a0.0[1]);
        let e14 = luma(c, Vn([e12.0[0], e12.0[1], e12.0[2]]));
        let e19 = at1(c, a0.0[0], a0.0[1].wrapping_sub(1i32));
        let e21 = luma(c, Vn([e19.0[0], e19.0[1], e19.0[2]]));
        let e26 = at1(c, a0.0[0], a0.0[1].wrapping_add(1i32));
        let e28 = luma(c, Vn([e26.0[0], e26.0[1], e26.0[2]]));
        let e34 = c.p.h1;
        return (Vn([(e14 - e7), (e28 - e21)]) * Vn([(0.5f32 * (e34 as f32)); 2]));
    }

    #[inline]
    pub fn wave1(c: &C<'_>, a0: f32) -> f32 {
        let e4 = c.p.i[1];
        if (e4 == 1i32) {
            let e7 = fr(c, a0);
            return (f32::abs(((e7 * 4.0f32) - 2.0f32)) - 1.0f32);
        }
        let e18 = c.p.i[1];
        if (e18 == 2i32) {
            let e23 = fr(c, a0);
            let e24 = ss(c, 0.0f32, 0.04f32, e23);
            let e31 = fr(c, a0);
            let e32 = ss(c, 0.5f32, 0.54f32, e31);
            return (((e24 * 2.0f32) - 1.0f32) - (2.0f32 * e32));
        }
        let e39 = c.p.i[1];
        if (e39 == 3i32) {
            let e46 = c.p.seed;
            let e47 = wander(c, (a0 * 2.0f32), e46);
            return e47;
        }
        return f32::sin((a0 * 6.2831855f32));
    }

}

mod light {
    use super::shared::*;
    use super::*;
    pub fn k_bokeh(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let mut v1: i32 = 0i32;
        let mut v2: i32 = 0i32;
        let mut v3: i32 = 0i32;
        let mut v4: f32 = 0.0f32;
        let e1 = pos(c, a0);
        let e6 = pf(c, 3u32);
        let e9 = pf(c, 3u32);
        'l0: loop {
            'b0: {
                let e14 = v1;
                let e18 = c.p.i[1];
                if (e14 < e18) {
                } else {
                    break 'l0;
                }
                let e20 = v1;
                let e21 = (e20 as f32);
                let e23 = pf(c, 0u32);
                let e30 = ((e23 * (1.0f32 + (0.55f32 * e21))) * 2.4f32);
                let e36 = pf(c, 2u32);
                let e49 = (((e1 / Vn([f32::max(e30, 0.001f32); 2])) + (Vn([f32::cos(e6), f32::sin(e9)]) * Vn([(e36 / (1.0f32 + (0.6f32 * e21))); 2]))) + Vn([(e21 * 5.3f32), (e21 * 2.1f32)]));
                let e50 = e49.map(f32::floor);
                v2 = (-1i32);
                'l1: loop {
                    'b1: {
                        let e53 = v2;
                        if (e53 <= 1i32) {
                        } else {
                            break 'l1;
                        }
                        v3 = (-1i32);
                        'l2: loop {
                            'b2: {
                                let e58 = v3;
                                if (e58 <= 1i32) {
                                } else {
                                    break 'l2;
                                }
                                let e63 = v3;
                                let e64 = (e50.0[0] as i32).wrapping_add(e63);
                                let e67 = v2;
                                let e68 = (e50.0[1] as i32).wrapping_add(e67);
                                let e71 = c.p.seed;
                                let e72 = v1;
                                let e77 = rnd(c, e64, e68, e71.wrapping_add((e72 as u32).wrapping_mul(37u32)));
                                let e79 = pf(c, 1u32);
                                if (e77 < e79) {
                                    let e83 = c.p.seed;
                                    let e84 = v1;
                                    let e91 = rnd2(c, e64, e68, e83.wrapping_add((e84 as u32).wrapping_mul(91u32)).wrapping_add(1u32));
                                    let e94 = c.p.seed;
                                    let e95 = v1;
                                    let e102 = rnd2(c, e64, e68, e94.wrapping_add((e95 as u32).wrapping_mul(53u32)).wrapping_add(2u32));
                                    let e116 = (0.2f32 + (0.26f32 * e102.0[0]));
                                    let e119 = ((e49 - ((Vn([(e64 as f32), (e68 as f32)]) + Vn([0.2f32; 2])) + (e91 * Vn([0.6f32; 2])))) / Vn([e116; 2]));
                                    v4 = e119.length();
                                    let e123 = pf(c, 4u32);
                                    if (e123 > 2.5f32) {
                                        let e127 = pf(c, 4u32);
                                        let e129 = pf(c, 5u32);
                                        let e130 = poly(c, e119, e127, e129);
                                        v4 = e130;
                                    }
                                    let e132 = pf(c, 14u32);
                                    let e136 = px(c);
                                    let e139 = f32::max((e132 + (0.12f32 * e21)), (e136 / (e30 * e116)));
                                    let e146 = v4;
                                    let e147 = ss(c, (1.0f32 - e139), (1.0f32 + (e139 * 0.3f32)), e146);
                                    let e151 = v4;
                                    let e154 = sq(c, f32::min(e151, 1.0f32));
                                    let e160 = pf(c, 12u32);
                                    let e161 = mix1(c, 1.0f32, (0.35f32 + (0.65f32 * e154)), e160);
                                    let e164 = pf(c, 13u32);
                                    let e167 = c.p.time;
                                    let e185 = v0;
                                    let e187 = pf3(c, 6u32);
                                    let e189 = pf3(c, 9u32);
                                    let e191 = mix3(c, e187, e189, e102.0[1]);
                                    v0 = (e185 + ((((e191 * Vn([((1.0f32 - e147) * e161); 3])) * Vn([(1.0f32 - (e164 * (0.5f32 + (0.5f32 * f32::sin(((e167 * (1.5f32 + (3.0f32 * e102.0[1]))) + (e77 * 40.0f32))))))); 3])) * Vn([(0.35f32 + (0.65f32 * e102.0[0])); 3])) / Vn([(1.0f32 + (0.4f32 * e21)); 3])));
                                }
                            }
                            let e208 = v3;
                            v3 = e208.wrapping_add(1i32);
                        }
                    }
                    let e211 = v2;
                    v2 = e211.wrapping_add(1i32);
                }
            }
            let e214 = v1;
            v1 = e214.wrapping_add(1i32);
        }
        let e216 = v0;
        return Vn([e216.0[0], e216.0[1], e216.0[2], 1.0f32]);
    }

    pub fn k_find_light(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let mut v1: f32 = 0.0f32;
        let mut v2: i32 = 0i32;
        let mut v3: i32 = 0i32;
        'l0: loop {
            'b0: {
                let e8 = v2;
                let e12 = c.p.i[1];
                if (e8 < e12) {
                } else {
                    break 'l0;
                }
                v3 = 0i32;
                'l1: loop {
                    'b1: {
                        let e16 = v3;
                        let e20 = c.p.i[0];
                        if (e16 < e20) {
                        } else {
                            break 'l1;
                        }
                        let e22 = v3;
                        let e29 = c.p.i[0];
                        let e32 = v2;
                        let e39 = c.p.i[1];
                        let e42 = Vn([(((e22 as f32) + 0.5f32) / (e29 as f32)), (((e32 as f32) + 0.5f32) / (e39 as f32))]);
                        let e44 = pic0_uv(c, e42, 1i32);
                        let e46 = luma(c, Vn([e44.0[0], e44.0[1], e44.0[2]]));
                        let e48 = f32::max(e46, 0.0f32);
                        let e50 = pf(c, 0u32);
                        let e51 = pw(c, e48, e50);
                        let e52 = v0;
                        let e53 = (e42 * Vn([e51; 2]));
                        v0 = (e52 + Vn([e53.0[0], e53.0[1], e51]));
                        let e56 = v1;
                        v1 = f32::max(e56, e48);
                    }
                    let e59 = v3;
                    v3 = e59.wrapping_add(1i32);
                }
            }
            let e62 = v2;
            v2 = e62.wrapping_add(1i32);
        }
        let e64 = v0;
        let e67 = v0.0[2];
        let e71 = (Vn([e64.0[0], e64.0[1]]) / Vn([f32::max(e67, 1e-6f32); 2]));
        let e77 = c.p.shape;
        let e82 = v1;
        return Vn([((e71.0[0] - 0.5f32) * e77), (e71.0[1] - 0.5f32), e82, 1.0f32]);
    }

    pub fn k_flare(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 2> = Vn([0.0f32; 2]);
        let mut v1: f32 = 1.0f32;
        let mut v2: Vn<f32, 3> = Vn([0.0f32; 3]);
        let mut v3: i32 = 0i32;
        let e1 = pos(c, a0);
        let e3 = pf2(c, 0u32);
        v0 = e3;
        let e10 = c.p.i[2];
        if (e10 == 1i32) {
            let e15 = at1(c, 0i32, 0i32);
            v0 = Vn([e15.0[0], e15.0[1]]);
            let e18 = pf(c, 16u32);
            let e22 = pf(c, 16u32);
            let e26 = ss(c, (e18 - 0.15f32), (e22 + 0.15f32), e15.0[2]);
            v1 = e26;
        }
        let e28 = pf(c, 4u32);
        let e30 = f32::max(e28, 0.001f32);
        let e31 = v0;
        let e34 = ((e1 - e31) / Vn([e30; 2]));
        let e35 = e34.length();
        let e36 = angle(c, e34);
        let e38 = pf(c, 14u32);
        let e39 = (e36 + e38);
        let e43 = v2;
        let e61 = pf(c, 8u32);
        v2 = (e43 + (Vn([((f32::exp((((-e35) * e35) * 260.0f32)) * 2.4f32) + ((0.012f32 / ((e35 * e35) + 0.012f32)) * 0.25f32)); 3]) * Vn([e61; 3])));
        let e64 = v2;
        let e73 = pf(c, 15u32);
        v2 = (e64 + (Vn([(f32::exp(((-e35) * 3.6f32)) * 0.45f32); 3]) * Vn([e73; 3])));
        let e79 = c.p.i[0];
        let e82 = f32::max((e79 as f32), 1.0f32);
        let e89 = pw(c, f32::abs(f32::cos(((e39 * e82) * 0.5f32))), 60.0f32);
        let e100 = pw(c, f32::abs(f32::cos((((e39 * e82) * 1.5f32) + 1.3f32))), 24.0f32);
        let e104 = v2;
        let e119 = pf(c, 9u32);
        v2 = (e104 + (Vn([((((e89 * 0.7f32) + (e100 * 0.12f32)) * f32::exp(((-e35) * 4.2f32))) * (1.0f32 - f32::exp(((-e35) * 30.0f32)))); 3]) * Vn([e119; 3])));
        let e151 = v2;
        let e158 = pf(c, 11u32);
        v2 = (e151 + (((Vn([0.35f32, 0.6f32, 1.0f32]) * Vn([((f32::exp(((-f32::abs(e34.0[1])) * 55.0f32)) * f32::exp(((-f32::abs(e34.0[0])) * 1.5f32))) + ((f32::exp(((-f32::abs(e34.0[1])) * 160.0f32)) * f32::exp(((-f32::abs(e34.0[0])) * 0.7f32))) * 0.7f32)); 3])) * Vn([e158; 3])) * Vn([0.8f32; 3])));
        let e169 = Vn([(e35 - 0.415f32), (e35 - 0.43f32), (e35 - 0.445f32)]);
        let e170 = v2;
        let e179 = pf(c, 10u32);
        v2 = (e170 + ((((((-e169) * e169) * Vn([2600.0f32; 3])).map(f32::exp) * Vn([0.4f32; 3])) * Vn([e179; 3])) * Vn([(0.55f32 + (0.45f32 * f32::cos(((e39 * 2.0f32) + 0.6f32)))); 3])));
        let e192 = v2;
        let e194 = pf3(c, 5u32);
        v2 = (e192 * e194);
        let e197 = pf2(c, 2u32);
        let e198 = v0;
        'l0: loop {
            'b0: {
                let e202 = v3;
                let e206 = c.p.i[1];
                if (e202 < e206) {
                } else {
                    break 'l0;
                }
                let e208 = v3;
                let e212 = c.p.seed;
                let e213 = rnd2(c, e208, 5i32, e212);
                let e214 = v3;
                let e225 = c.p.i[1];
                let e239 = ((0.025f32 + ((0.085f32 * e213.0[1]) * e213.0[1])) * e30);
                let e240 = v0;
                let e248 = pf(c, 14u32);
                let e251 = poly(c, (((e1 - e240) - ((e197 - e198) * Vn([(0.35f32 + ((2.1f32 * ((e214 as f32) + (0.5f32 * e213.0[0]))) / f32::max((e225 as f32), 1.0f32))); 2]))) / Vn([e239; 2])), 6.0f32, (e248 + 0.3f32));
                let e252 = px(c);
                let e253 = (e252 / e239);
                let e258 = ss(c, (0.9f32 - e253), (1.0f32 + e253), e251);
                let e263 = ss(c, 0.3f32, 1.0f32, e251);
                let e276 = fr(c, ((e213.0[0] * 0.9f32) + 0.35f32));
                let e279 = hsv(c, e276, 0.75f32, 1.0f32);
                let e281 = pf(c, 13u32);
                let e282 = mix3(c, Vn([1.0f32; 3]), e279, e281);
                let e283 = v2;
                let e292 = pf(c, 12u32);
                v2 = (e283 + (((e282 * Vn([((1.0f32 - e258) * (0.3f32 + (0.7f32 * e263))); 3])) * Vn([(0.05f32 + (0.12f32 * e213.0[0])); 3])) * Vn([e292; 3])));
            }
            let e296 = v3;
            v3 = e296.wrapping_add(1i32);
        }
        let e298 = v2;
        let e299 = v1;
        let e300 = (e298 * Vn([e299; 3]));
        return Vn([e300.0[0], e300.0[1], e300.0[2], 1.0f32]);
    }

    pub fn k_flashes(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let mut v1: i32 = (-1i32);
        let mut v2: i32 = 0i32;
        let e1 = pos(c, a0);
        let e3 = pf(c, 0u32);
        let e7 = (f32::max(e3, 0.001f32) * 1.6f32);
        let e10 = (e1 / Vn([e7; 2])).map(f32::floor);
        'l0: loop {
            'b0: {
                let e15 = v1;
                if (e15 <= 1i32) {
                } else {
                    break 'l0;
                }
                v2 = (-1i32);
                'l1: loop {
                    'b1: {
                        let e20 = v2;
                        if (e20 <= 1i32) {
                        } else {
                            break 'l1;
                        }
                        let e25 = v2;
                        let e26 = (e10.0[0] as i32).wrapping_add(e25);
                        let e29 = v1;
                        let e30 = (e10.0[1] as i32).wrapping_add(e29);
                        let e33 = c.p.seed;
                        let e34 = rnd2(c, e26, e30, e33);
                        let e37 = c.p.time;
                        let e39 = pf(c, 1u32);
                        let e44 = ((e37 * e39) + (e34.0[0] * 19.0f32));
                        let e45 = f32::floor(e44);
                        let e56 = c.p.seed;
                        let e59 = rnd2(c, e26.wrapping_mul(7i32).wrapping_add((e45 as i32)), e30.wrapping_mul(13i32).wrapping_sub((e45 as i32)), e56.wrapping_add(5u32));
                        let e69 = (((Vn([(e26 as f32), (e30 as f32)]) + Vn([0.2f32; 2])) + (e59 * Vn([0.6f32; 2]))) * Vn([e7; 2]));
                        let e71 = pf(c, 6u32);
                        let e88 = pf(c, 1u32);
                        let e94 = pf(c, 2u32);
                        let e104 = pf(c, 0u32);
                        let e108 = ((e1 - e69) / Vn([f32::max(e104, 0.001f32); 2]));
                        let e109 = e108.length();
                        let e137 = v0;
                        let e155 = pf(c, 7u32);
                        v0 = (e137 + ((f32::exp((((-((e44 - e45) / f32::max(e88, 0.001f32))) / f32::max(e94, 0.001f32)) * 3.0f32)) * (step((0.5f32 - e71), e69.0[1]) * step(((e59.0[0] * 0.5f32) + (e34.0[1] * 0.5f32)), 0.62f32))) * (((f32::exp(((-e109) * 6.0f32)) * 1.8f32) + (f32::exp((((-e109) * e109) * 45.0f32)) * 1.5f32)) + ((((f32::exp(((-f32::abs(e108.0[0])) * 26.0f32)) * f32::exp(((-f32::abs(e108.0[1])) * 3.0f32))) + (f32::exp(((-f32::abs(e108.0[1])) * 26.0f32)) * f32::exp(((-f32::abs(e108.0[0])) * 3.0f32)))) * 0.8f32) * e155))));
                    }
                    let e161 = v2;
                    v2 = e161.wrapping_add(1i32);
                }
            }
            let e164 = v1;
            v1 = e164.wrapping_add(1i32);
        }
        let e167 = pf3(c, 3u32);
        let e168 = v0;
        let e169 = (e167 * Vn([e168; 3]));
        return Vn([e169.0[0], e169.0[1], e169.0[2], 1.0f32]);
    }

    pub fn k_glow(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = pf(c, 4u32);
        let e7 = (Vn([e3.0[0], e3.0[1], e3.0[2]]) * Vn([e6; 3]));
        let e12 = at1(c, a0.0[0], a0.0[1]);
        let e15 = pf3(c, 1u32);
        let e18 = pf(c, 0u32);
        let e23 = c.p.i[0];
        let e24 = lit(c, Vn([e7.0[0], e7.0[1], e7.0[2], e3.0[3]]), ((Vn([e12.0[0], e12.0[1], e12.0[2]]) * e15) * Vn([e18; 3])), e23);
        return e24;
    }

    pub fn k_glow_dark(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = at1(c, a0.0[0], a0.0[1]);
        let e9 = pf(c, 0u32);
        let e14 = pf3(c, 1u32);
        let e28 = (Vn([e3.0[0], e3.0[1], e3.0[2]]) * (Vn([1.0f32; 3]) - ((Vn([e6.0[0], e6.0[1], e6.0[2]]) * Vn([e9; 3])) * (Vn([1.0f32; 3]) - (e14 * Vn([0.85f32; 3])))).zip3(Vn([0.0f32; 3]), Vn([1.0f32; 3]), clampf)));
        return Vn([e28.0[0], e28.0[1], e28.0[2], e3.0[3]]);
    }

    pub fn k_glow_rainbow(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = at1(c, a0.0[0], a0.0[1]);
        let e7 = Vn([e6.0[0], e6.0[1], e6.0[2]]);
        let e12 = f32::max(f32::max(e7.0[0], e7.0[1]), e7.0[2]);
        let e14 = pw(c, e12, 0.55f32);
        let e16 = pf(c, 5u32);
        let e19 = pf(c, 6u32);
        let e21 = fr(c, ((e14 * e16) + e19));
        let e23 = pf(c, 7u32);
        let e25 = hsv(c, e21, e23, 1.0f32);
        let e28 = pf(c, 4u32);
        let e29 = (Vn([e3.0[0], e3.0[1], e3.0[2]]) * Vn([e28; 3]));
        let e34 = pf3(c, 1u32);
        let e37 = pf(c, 0u32);
        let e42 = c.p.i[0];
        let e43 = lit(c, Vn([e29.0[0], e29.0[1], e29.0[2], e3.0[3]]), (((e25 * Vn([e12; 3])) * e34) * Vn([e37; 3])), e42);
        return e43;
    }

    pub fn k_glow_rings(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = at1(c, a0.0[0], a0.0[1]);
        let e7 = Vn([e6.0[0], e6.0[1], e6.0[2]]);
        let e14 = pw(c, f32::max(f32::max(e7.0[0], e7.0[1]), e7.0[2]), 0.4f32);
        let e16 = pf(c, 5u32);
        let e19 = pf(c, 6u32);
        let e22 = (((e14 * e16) + e19) * 6.2831855f32);
        let e24 = pf(c, 8u32);
        let e26 = ring1(c, (e22 - e24));
        let e27 = ring1(c, e22);
        let e29 = pf(c, 8u32);
        let e31 = ring1(c, (e22 + e29));
        let e35 = pf(c, 4u32);
        let e36 = (Vn([e3.0[0], e3.0[1], e3.0[2]]) * Vn([e35; 3]));
        let e41 = pf3(c, 1u32);
        let e44 = pf(c, 0u32);
        let e49 = c.p.i[0];
        let e50 = lit(c, Vn([e36.0[0], e36.0[1], e36.0[2], e3.0[3]]), (((e7 * Vn([e26, e27, e31])) * e41) * Vn([e44; 3])), e49);
        return e50;
    }

    pub fn k_glow_smoke(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = pos(c, a0);
        let e6 = pf(c, 5u32);
        let e12 = pf2(c, 7u32);
        let e15 = c.p.time;
        let e18 = pf(c, 5u32);
        let e23 = ((e4 / Vn([f32::max(e6, 0.001f32); 2])) + ((e12 * Vn([e15; 2])) / Vn([f32::max(e18, 0.001f32); 2])));
        let e25 = (e23 * Vn([0.7f32; 2]));
        let e28 = c.p.time;
        let e30 = pf(c, 9u32);
        let e35 = c.p.seed;
        let e38 = noise3(c, Vn([e25.0[0], e25.0[1], (e28 * e30)]), e35.wrapping_add(3u32));
        let e44 = ((e23 * Vn([0.7f32; 2])) + Vn([9.2f32, 3.3f32]));
        let e47 = c.p.time;
        let e49 = pf(c, 9u32);
        let e54 = c.p.seed;
        let e57 = noise3(c, Vn([e44.0[0], e44.0[1], (e47 * e49)]), e54.wrapping_add(4u32));
        let e61 = (e23 + (Vn([e38, e57]) * Vn([0.7f32; 2])));
        let e64 = c.p.time;
        let e66 = pf(c, 9u32);
        let e72 = c.p.i[1];
        let e75 = c.p.seed;
        let e76 = fbm(c, Vn([e61.0[0], e61.0[1], (e64 * e66)]), e72, e75);
        let e80 = pf(c, 6u32);
        let e88 = pf(c, 4u32);
        let e89 = (Vn([e3.0[0], e3.0[1], e3.0[2]]) * Vn([e88; 3]));
        let e94 = at1(c, a0.0[0], a0.0[1]);
        let e97 = pf3(c, 1u32);
        let e100 = pf(c, 0u32);
        let e106 = c.p.i[0];
        let e107 = lit(c, Vn([e89.0[0], e89.0[1], e89.0[2], e3.0[3]]), ((Vn([e94.0[0], e94.0[1], e94.0[2]]) * e97) * Vn([(e100 * f32::max((1.0f32 + ((e76 * 2.4f32) * e80)), 0.0f32)); 3])), e106);
        return e107;
    }

    pub fn k_glow_tube(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = at1(c, a0.0[0], a0.0[1]);
        let e7 = Vn([e6.0[0], e6.0[1], e6.0[2]]);
        let e10 = at2(c, a0.0[0], a0.0[1]);
        let e12 = luma(c, Vn([e10.0[0], e10.0[1], e10.0[2]]));
        let e13 = luma(c, e7);
        let e18 = pf(c, 5u32);
        let e26 = (Vn([e3.0[0], e3.0[1], e3.0[2]]) * Vn([(1.0f32 - clampf(((e12 - (e13 * 0.6f32)) * e18), 0.0f32, 0.95f32)); 3]));
        let e30 = pf3(c, 1u32);
        let e33 = pf(c, 0u32);
        let e38 = c.p.i[0];
        let e39 = lit(c, Vn([e26.0[0], e26.0[1], e26.0[2], e3.0[3]]), ((e7 * e30) * Vn([e33; 3])), e38);
        return e39;
    }

    pub fn k_glow_two(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = at1(c, a0.0[0], a0.0[1]);
        let e7 = Vn([e6.0[0], e6.0[1], e6.0[2]]);
        let e10 = pf(c, 8u32);
        let e18 = ss(c, 0.0f32, f32::max(e10, 0.001f32), f32::max(f32::max(e7.0[0], e7.0[1]), e7.0[2]));
        let e21 = pf(c, 4u32);
        let e22 = (Vn([e3.0[0], e3.0[1], e3.0[2]]) * Vn([e21; 3]));
        let e26 = pf3(c, 5u32);
        let e28 = pf3(c, 1u32);
        let e29 = mix3(c, e26, e28, e18);
        let e32 = pf(c, 0u32);
        let e37 = c.p.i[0];
        let e38 = lit(c, Vn([e22.0[0], e22.0[1], e22.0[2], e3.0[3]]), ((e7 * e29) * Vn([e32; 3])), e37);
        return e38;
    }

    pub fn k_halation(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = at1(c, a0.0[0], a0.0[1]);
        let e10 = at2(c, a0.0[0], a0.0[1]);
        let e12 = luma(c, Vn([e6.0[0], e6.0[1], e6.0[2]]));
        let e13 = luma(c, Vn([e10.0[0], e10.0[1], e10.0[2]]));
        let e15 = pf(c, 5u32);
        let e21 = pf3(c, 1u32);
        let e24 = pf(c, 0u32);
        let e29 = c.p.i[0];
        let e30 = lit(c, e3, ((e21 * Vn([f32::max((e12 - (e13 * e15)), 0.0f32); 3])) * Vn([e24; 3])), e29);
        return e30;
    }

    pub fn k_leak(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let mut v1: f32 = 0.0f32;
        let e1 = pos(c, a0);
        let e3 = pf(c, 1u32);
        let e5 = turn(c, e1, (-e3));
        let e7 = pf(c, 0u32);
        let e11 = (1.0f32 / f32::max(e7, 0.001f32));
        let e13 = pf(c, 2u32);
        let e16 = ((e5 * Vn([e11; 2])) * Vn([0.8f32; 2]));
        let e22 = c.p.seed;
        let e23 = noise3(c, Vn([e16.0[0], e16.0[1], (e13 * 0.6f32)]), e22);
        let e30 = (((e5 * Vn([e11; 2])) * Vn([0.8f32; 2])) + Vn([7.1f32, 2.9f32]));
        let e36 = c.p.seed;
        let e39 = noise3(c, Vn([e30.0[0], e30.0[1], (e13 * 0.6f32)]), e36.wrapping_add(1u32));
        let e42 = pf(c, 13u32);
        let e48 = (e5 + (((Vn([e23, e39]) * Vn([e42; 2])) * Vn([0.35f32; 2])) / Vn([e11; 2])));
        let e56 = c.p.i[1];
        if (e56 == 1i32) {
            let e67 = c.p.seed;
            let e70 = wander(c, (e13 * 0.7f32), e67.wrapping_add(2u32));
            let e85 = pw(c, (0.5f32 + (0.5f32 * f32::cos(((((((e48.0[0] * e11) * 0.8f32) + (e70 * 0.8f32)) * 6.2831855f32) * 0.5f32) + e13)))), 3.0f32);
            v0 = e85;
            let e94 = c.p.seed;
            let e97 = noise3(c, Vn([(e48.0[0] * e11), 0.0f32, (e13 * 0.5f32)]), e94.wrapping_add(8u32));
            v1 = (0.5f32 + (0.5f32 * e97));
        } else {
            let e105 = c.p.i[1];
            if (e105 == 2i32) {
                let e108 = (e48 * Vn([e11; 2]));
                let e115 = c.p.seed;
                let e116 = fbm(c, Vn([e108.0[0], e108.0[1], (e13 * 0.5f32)]), 5i32, e115);
                let e121 = c.p.seed;
                let e124 = wander(c, (e13 * 0.4f32), e121.wrapping_add(3u32));
                let e131 = c.p.seed;
                let e134 = wander(c, ((e13 * 0.4f32) + 4.0f32), e131.wrapping_add(4u32));
                let e150 = ss(c, 1.0f32, 0.0f32, (((((e5 - (Vn([e124, e134]) * Vn([0.35f32; 2]))).length() * e11) * 0.55f32) - (e116 * 0.9f32)) + 0.1f32));
                v0 = e150;
                let e153 = v0;
                let e154 = ss(c, 0.0f32, 1.0f32, e153);
                v1 = e154;
            } else {
                let e157 = ((e48 * Vn([e11; 2])) * Vn([0.55f32; 2]));
                let e164 = c.p.seed;
                let e165 = fbm(c, Vn([e157.0[0], e157.0[1], (e13 * 0.45f32)]), 3i32, e164);
                let e172 = (((e48 * Vn([e11; 2])) * Vn([0.4f32; 2])) + Vn([13.0f32, 5.0f32]));
                let e179 = c.p.seed;
                let e182 = fbm(c, Vn([e172.0[0], e172.0[1], (e13 * 0.3f32)]), 2i32, e179.wrapping_add(6u32));
                let e188 = ss(c, -0.3f32, 0.55f32, (e165 + (e182 * 0.5f32)));
                v0 = e188;
                v1 = (0.5f32 + (e182 * 1.4f32));
            }
        }
        let e197 = c.p.shape;
        let e206 = v0;
        let e210 = ss(c, 0.15f32, 1.0f32, clampf((f32::abs(e5.0[0]) / f32::max((e197 * 0.5f32), 0.5f32)), 0.0f32, 1.2f32));
        let e212 = pf(c, 3u32);
        let e213 = mix1(c, 1.0f32, e210, e212);
        v0 = (e206 * e213);
        let e216 = pf3(c, 4u32);
        let e218 = pf3(c, 7u32);
        let e221 = v1;
        let e222 = ss(c, 0.0f32, 0.55f32, e221);
        let e223 = mix3(c, e216, e218, e222);
        let e225 = pf3(c, 10u32);
        let e228 = v1;
        let e229 = ss(c, 0.5f32, 1.0f32, e228);
        let e230 = mix3(c, e223, e225, e229);
        let e231 = v0;
        let e232 = (e230 * Vn([e231; 3]));
        return Vn([e232.0[0], e232.0[1], e232.0[2], 1.0f32]);
    }

    pub fn k_light_fade(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e2 = pf2(c, 2u32);
        let e4 = pf2(c, 0u32);
        let e5 = (e2 - e4);
        let e6 = pos(c, a0);
        let e8 = pf2(c, 0u32);
        let e17 = clampf(((e6 - e8).dot(e5) / f32::max(e5.dot(e5), 1e-6f32)), 0.0f32, 1.0f32);
        let e20 = at0(c, a0.0[0], a0.0[1]);
        return (e20 * Vn([(1.0f32 - ((e17 * e17) * (3.0f32 - (2.0f32 * e17)))); 4]));
    }

    pub fn k_neon(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e2 = pf(c, 6u32);
        let e3 = edge_gap(c, e2);
        let e8 = Vn([(a0.0[0] as f32), (a0.0[1] as f32)]);
        let e13 = bil0(c, Vn([(e8.0[0] - e3), e8.0[1]]));
        let e14 = Vn([e13.0[0], e13.0[1], e13.0[2]]);
        let e19 = bil0(c, Vn([(e8.0[0] + e3), e8.0[1]]));
        let e20 = Vn([e19.0[0], e19.0[1], e19.0[2]]);
        let e25 = bil0(c, Vn([e8.0[0], (e8.0[1] - e3)]));
        let e26 = Vn([e25.0[0], e25.0[1], e25.0[2]]);
        let e31 = bil0(c, Vn([e8.0[0], (e8.0[1] + e3)]));
        let e32 = Vn([e31.0[0], e31.0[1], e31.0[2]]);
        let e40 = (((e20 - e14) * (e20 - e14)) + ((e32 - e26) * (e32 - e26))).map(f32::sqrt);
        let e47 = pf(c, 5u32);
        let e52 = pf(c, 0u32);
        let e56 = at0(c, a0.0[0], a0.0[1]);
        let e68 = pf3(c, 1u32);
        let e70 = pf(c, 4u32);
        let e71 = mix3(c, e68, (Vn([e56.0[0], e56.0[1], e56.0[2]]) / Vn([f32::max(f32::max(f32::max(e56.0[0], e56.0[1]), e56.0[2]), 0.08f32); 3])), e70);
        let e76 = pf(c, 6u32);
        let e77 = edge_thin(c, e76);
        let e78 = ((e71 * Vn([f32::min((f32::max((f32::max(f32::max(e40.0[0], e40.0[1]), e40.0[2]) - e47), 0.0f32) * e52), 1.5f32); 3])) * Vn([e77; 3]));
        return Vn([e78.0[0], e78.0[1], e78.0[2], e56.0[3]]);
    }

    pub fn k_points(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e7 = at1(c, a0.0[0], a0.0[1]);
        let e10 = pf(c, 0u32);
        let e17 = pf(c, 1u32);
        let e18 = ((Vn([e3.0[0], e3.0[1], e3.0[2]]) - (Vn([e7.0[0], e7.0[1], e7.0[2]]) * Vn([e10; 3]))).zip(Vn([0.0f32; 3]), f32::max) * Vn([e17; 3]));
        return Vn([e18.0[0], e18.0[1], e18.0[2], e3.0[3]]);
    }

    pub fn k_rays(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let mut v1: f32 = 0.0f32;
        let mut v2: i32 = 0i32;
        let mut v3: f32 = 1.0f32;
        let e1 = pos(c, a0);
        let e3 = pf2(c, 0u32);
        let e4 = (e1 - e3);
        let e8 = c.p.i[1];
        'l0: loop {
            'b0: {
                let e16 = v2;
                if (e16 < e8) {
                } else {
                    break 'l0;
                }
                let e18 = v2;
                let e23 = (((e18 as f32) + 0.5f32) / (e8 as f32));
                let e27 = pf(c, 3u32);
                let e28 = pw(c, (1.0f32 - e23), e27);
                let e29 = v0;
                let e31 = pf(c, 2u32);
                let e36 = pic0(c, (e1 - (e4 * Vn([(e23 * e31); 2]))), 0i32);
                v0 = (e29 + (Vn([e36.0[0], e36.0[1], e36.0[2]]) * Vn([e28; 3])));
                let e40 = v1;
                v1 = (e40 + e28);
            }
            let e43 = v2;
            v2 = e43.wrapping_add(1i32);
        }
        let e48 = pf(c, 4u32);
        if (e48 > 0.0f32) {
            let e51 = angle(c, e4);
            let e55 = pf(c, 5u32);
            let e56 = ((e51 / 6.2831855f32) * e55);
            let e59 = c.p.time;
            let e61 = pf(c, 6u32);
            let e62 = (e59 * e61);
            let e68 = c.p.seed;
            let e69 = wander(c, ((e56 * 3.0f32) + e62), e68);
            let e79 = c.p.seed;
            let e82 = wander(c, ((e56 * 7.0f32) - (e62 * 1.7f32)), e79.wrapping_add(5u32));
            let e88 = angle(c, e4);
            let e94 = ss(c, 0.0f32, 0.06f32, (0.5f32 - f32::abs((e88 / 6.2831855f32))));
            let e97 = pf(c, 4u32);
            v3 = f32::max((1.0f32 + ((((e69 * 0.6f32) + (e82 * 0.4f32)) * e94) * e97)), 0.0f32);
        }
        let e104 = pf3(c, 7u32);
        let e106 = pf3(c, 10u32);
        let e110 = ss(c, 0.0f32, 0.9f32, e4.length());
        let e111 = mix3(c, e104, e106, e110);
        let e112 = v0;
        let e113 = v1;
        let e119 = v3;
        let e120 = (((e112 / Vn([f32::max(e113, 0.0001f32); 3])) * e111) * Vn([e119; 3]));
        return Vn([e120.0[0], e120.0[1], e120.0[2], 1.0f32]);
    }

    pub fn k_relight(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = pos(c, a0);
        let e9 = at1(c, a0.0[0].wrapping_sub(1i32), a0.0[1]);
        let e11 = luma(c, Vn([e9.0[0], e9.0[1], e9.0[2]]));
        let e16 = at1(c, a0.0[0].wrapping_add(1i32), a0.0[1]);
        let e18 = luma(c, Vn([e16.0[0], e16.0[1], e16.0[2]]));
        let e23 = at1(c, a0.0[0], a0.0[1].wrapping_sub(1i32));
        let e25 = luma(c, Vn([e23.0[0], e23.0[1], e23.0[2]]));
        let e30 = at1(c, a0.0[0], a0.0[1].wrapping_add(1i32));
        let e32 = luma(c, Vn([e30.0[0], e30.0[1], e30.0[2]]));
        let e34 = pf(c, 3u32);
        let e39 = c.p.h;
        let e43 = (((e34 * 0.5f32) * (e39 as f32)) * 0.06f32);
        let e50 = Vn([((e11 - e18) * e43), ((e25 - e32) * e43), 1.0f32]).normalize();
        let e52 = pf2(c, 0u32);
        let e53 = (e52 - e4);
        let e55 = pf(c, 2u32);
        let e56 = Vn([e53.0[0], e53.0[1], e55]);
        let e57 = e56.length();
        let e61 = (e56 / Vn([f32::max(e57, 0.0001f32); 3]));
        let e75 = pf(c, 7u32);
        let e76 = pw(c, f32::max(e50.dot((e61 + Vn([0.0f32, 0.0f32, 1.0f32])).normalize()), 0.0f32), e75);
        let e78 = pf(c, 6u32);
        let e81 = pf(c, 11u32);
        let e85 = sq(c, (e57 / f32::max(e81, 0.001f32)));
        let e89 = (1.0f32 / (1.0f32 + e85));
        let e91 = pf(c, 5u32);
        let e94 = pf3(c, 8u32);
        let e96 = pf(c, 4u32);
        let e104 = pf3(c, 8u32);
        let e107 = pf(c, 4u32);
        let e112 = ((Vn([e3.0[0], e3.0[1], e3.0[2]]) * (Vn([e91; 3]) + (e94 * Vn([((f32::max(e50.dot(e61), 0.0f32) * e96) * e89); 3])))) + (e104 * Vn([((((e76 * e78) * e89) * e107) * e3.0[3]); 3])));
        return Vn([e112.0[0], e112.0[1], e112.0[2], e3.0[3]]);
    }

    pub fn k_rim(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e7 = at1(c, a0.0[0], a0.0[1]);
        let e15 = pf3(c, 1u32);
        let e17 = pf(c, 0u32);
        let e22 = (Vn([e3.0[0], e3.0[1], e3.0[2]]) + (e15 * Vn([((clampf((e3.0[3] - e7.0[3]), 0.0f32, 1.0f32) * e17) * e3.0[3]); 3])));
        return Vn([e22.0[0], e22.0[1], e22.0[2], e3.0[3]]);
    }

    pub fn k_shadow(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = pos(c, a0);
        let e6 = pf2(c, 0u32);
        let e9 = pic1(c, (e4 - e6), 0i32);
        let e12 = pf(c, 2u32);
        let e16 = clampf((e9.0[3] * e12), 0.0f32, 1.0f32);
        let e18 = pf3(c, 3u32);
        let e19 = (e18 * Vn([e16; 3]));
        let e20 = Vn([e19.0[0], e19.0[1], e19.0[2], e16]);
        let e24 = c.p.i[0];
        if (e24 == 1i32) {
            return e20;
        }
        return (e3 + (e20 * Vn([(1.0f32 - clampf(e3.0[3], 0.0f32, 1.0f32)); 4])));
    }

    pub fn k_spot(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = pos(c, a0);
        let e6 = pf2(c, 2u32);
        let e8 = pf2(c, 0u32);
        let e9 = (e6 - e8);
        let e12 = f32::max(e9.length(), 0.0001f32);
        let e14 = (e9 / Vn([e12; 2]));
        let e16 = pf2(c, 2u32);
        let e17 = (e4 - e16);
        let e24 = Vn([e17.dot(e14), e17.dot(Vn([(-e14.0[1]), e14.0[0]]))]);
        let e26 = pf(c, 4u32);
        let e28 = f32::max(e26, 0.0001f32);
        let e31 = pf(c, 12u32);
        let e38 = pf(c, 5u32);
        let e45 = pf(c, 5u32);
        let e50 = px(c);
        let e53 = ss(c, (1.0f32 - clampf(e38, 0.0f32, 1.0f32)), ((1.0f32 + (e45 * 0.35f32)) + (e50 / e28)), (Vn([(e24.0[0] * e31), e24.0[1]]).length() / e28));
        let e57 = pf2(c, 0u32);
        let e58 = (e4 - e57);
        let e60 = (e58.dot(e14) / e12);
        let e71 = mix1(c, 0.012f32, e28, clampf(e60, 0.0f32, 1.2f32));
        let e75 = sq(c, (f32::abs(e58.dot(Vn([(-e14.0[1]), e14.0[0]]))) / f32::max(e71, 0.0001f32)));
        let e82 = ss(c, 0.0f32, 0.04f32, e60);
        let e86 = ss(c, 0.75f32, 1.05f32, e60);
        v0 = (((f32::exp(((-e75) * 1.8f32)) * e82) * (1.0f32 - e86)) / (0.55f32 + (e60 * 1.6f32)));
        let e97 = pf(c, 13u32);
        if (e97 > 0.0f32) {
            let e104 = c.p.time;
            let e109 = c.p.time;
            let e114 = ((e4 * Vn([5.0f32; 2])) + Vn([(e104 * 0.12f32), ((-e109) * 0.07f32)]));
            let e117 = c.p.time;
            let e124 = c.p.seed;
            let e125 = fbm(c, Vn([e114.0[0], e114.0[1], (e117 * 0.2f32)]), 4i32, e124);
            let e126 = v0;
            let e130 = pf(c, 13u32);
            v0 = (e126 * f32::max((1.0f32 + ((e125 * 2.0f32) * e130)), 0.0f32));
        }
        let e138 = pf(c, 7u32);
        let e141 = pf3(c, 8u32);
        let e143 = pf(c, 6u32);
        let e148 = (Vn([e3.0[0], e3.0[1], e3.0[2]]) * (Vn([e138; 3]) + (e141 * Vn([((1.0f32 - e53) * e143); 3]))));
        let e152 = pf3(c, 8u32);
        let e153 = v0;
        let e155 = pf(c, 11u32);
        let e158 = pf(c, 6u32);
        let e168 = lit(c, Vn([e148.0[0], e148.0[1], e148.0[2], e3.0[3]]), ((e152 * Vn([(((e153 * e155) * e158) * 0.5f32); 3])) * Vn([f32::max(e3.0[3], 0.0f32); 3])), 1i32);
        return e168;
    }

    pub fn k_star(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let mut v1: i32 = 0i32;
        let mut v2: f32 = 0.0f32;
        let mut v3: i32 = 0i32;
        let e3 = c.p.w0;
        let e7 = c.p.h0;
        let e9 = Vn([(e3 as f32), (e7 as f32)]);
        let e18 = Vn([((a0.0[0] as f32) + 0.5f32), ((a0.0[1] as f32) + 0.5f32)]);
        let e25 = c.p.i[0];
        let e29 = c.p.i[1];
        'l0: loop {
            'b0: {
                let e32 = v1;
                if (e32 < e25) {
                } else {
                    break 'l0;
                }
                let e35 = pf(c, 1u32);
                let e36 = v1;
                let e42 = (e35 + (((e36 as f32) * 3.1415927f32) / (e25 as f32)));
                let e44 = pf(c, 0u32);
                v2 = e44;
                let e46 = v1;
                if ((e46 & 1i32) == 1i32) {
                    let e51 = v2;
                    let e53 = pf(c, 4u32);
                    v2 = (e51 * e53);
                }
                let e58 = v2;
                let e59 = (Vn([f32::cos(e42), f32::sin(e42)]) * Vn([e58; 2]));
                v3 = 0i32;
                'l1: loop {
                    'b1: {
                        let e62 = v3;
                        if (e62 < e29) {
                        } else {
                            break 'l1;
                        }
                        let e64 = v3;
                        let e69 = (((e64 as f32) + 0.5f32) / (e29 as f32));
                        let e73 = pf(c, 2u32);
                        let e74 = pw(c, (1.0f32 - e69), e73);
                        let e83 = hsv(c, (0.72f32 - (e69 * 0.72f32)), 0.9f32, 1.35f32);
                        let e85 = pf(c, 3u32);
                        let e86 = mix3(c, Vn([1.0f32; 3]), e83, e85);
                        let e91 = pic0_uv(c, ((e18 + (e59 * Vn([e69; 2]))) / e9), 0i32);
                        let e97 = pic0_uv(c, ((e18 - (e59 * Vn([e69; 2]))) / e9), 0i32);
                        let e100 = v0;
                        v0 = (e100 + (((Vn([e91.0[0], e91.0[1], e91.0[2]]) + Vn([e97.0[0], e97.0[1], e97.0[2]])) * e86) * Vn([e74; 3])));
                    }
                    let e105 = v3;
                    v3 = e105.wrapping_add(1i32);
                }
            }
            let e108 = v1;
            v1 = e108.wrapping_add(1i32);
        }
        let e110 = v0;
        let e114 = (e110 * Vn([(7.5f32 / (e29 as f32)); 3]));
        let e125 = (e114 / Vn([(1.0f32 + (0.45f32 * f32::max(f32::max(e114.0[0], e114.0[1]), e114.0[2]))); 3]));
        return Vn([e125.0[0], e125.0[1], e125.0[2], 1.0f32]);
    }

    #[inline]
    pub fn poly(c: &C<'_>, a0: Vn<f32, 2>, a1: f32, a2: f32) -> f32 {
        let e3 = angle(c, a0);
        let e4 = (e3 + a2);
        let e6 = (3.1415927f32 / a1);
        return ((a0.length() * f32::cos(((e4 - ((2.0f32 * e6) * f32::floor((e4 / (2.0f32 * e6))))) - e6))) / f32::cos(e6));
    }

    #[inline]
    pub fn ring1(c: &C<'_>, a0: f32) -> f32 {
        let e7 = pf(c, 7u32);
        let e8 = pw(c, (0.5f32 + (0.5f32 * f32::cos(a0))), e7);
        return e8;
    }

}

mod make {
    use super::shared::*;
    use super::*;
    #[inline]
    pub fn dyed(c: &C<'_>, a0: f32) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let e4 = pf(c, 5u32);
        let e10 = clampf((((a0 - 0.5f32) * e4) + 0.5f32), 0.0f32, 1.0f32);
        let e12 = pf3(c, 6u32);
        let e14 = pf3(c, 9u32);
        let e15 = mix3(c, e12, e14, e10);
        v0 = e15;
        let e20 = c.p.i[3];
        if (e20 == 3i32) {
            let e24 = pf3(c, 6u32);
            let e26 = pf3(c, 9u32);
            let e29 = ss(c, 0.0f32, 0.5f32, e10);
            let e30 = mix3(c, e24, e26, e29);
            let e32 = pf3(c, 12u32);
            let e35 = ss(c, 0.5f32, 1.0f32, e10);
            let e36 = mix3(c, e30, e32, e35);
            v0 = e36;
        }
        let e37 = v0;
        let e39 = pf(c, 4u32);
        let e41 = seen(c, (e37 * Vn([e39; 3])), e10);
        return e41;
    }

    #[inline]
    pub fn glowing(c: &C<'_>, a0: Vn<f32, 3>) -> Vn<f32, 4> {
        let e2 = pf(c, 4u32);
        let e6 = (a0 * Vn([e2; 3])).zip(Vn([0.0f32; 3]), f32::max);
        return Vn([e6.0[0], e6.0[1], e6.0[2], clampf(f32::max(f32::max(e6.0[0], e6.0[1]), e6.0[2]), 0.0f32, 1.0f32)]);
    }

    #[inline]
    pub fn halo1(c: &C<'_>, a0: Vn<f32, 2>) -> f32 {
        let e1 = a0.map(f32::floor);
        let e2 = (a0 - e1);
        let e5 = (Vn([1.0f32; 2]) - e2);
        let e19 = (((Vn([4.0f32; 2]) - ((e2 * e2) * Vn([6.0f32; 2]))) + (((e2 * e2) * e2) * Vn([3.0f32; 2]))) / Vn([6.0f32; 2]));
        let e25 = ((((e5 * e5) * e5) / Vn([6.0f32; 2])) + e19);
        let e28 = (Vn([1.0f32; 2]) - e25);
        let e33 = ((e1 + (e19 / e25)) - Vn([1.0f32; 2]));
        let e43 = ((e1 + ((((e2 * e2) * e2) / Vn([6.0f32; 2])) / e28)) + Vn([1.0f32; 2]));
        let e44 = bil1(c, e33);
        let e51 = bil1(c, Vn([e43.0[0], e33.0[1]]));
        let e59 = bil1(c, Vn([e33.0[0], e43.0[1]]));
        let e63 = bil1(c, e43);
        return ((((e44.0[0] * e25.0[0]) + (e51.0[0] * e28.0[0])) * e25.0[1]) + (((e59.0[0] * e25.0[0]) + (e63.0[0] * e28.0[0])) * e28.0[1]));
    }

    #[inline]
    pub fn halo2(c: &C<'_>, a0: Vn<f32, 2>) -> f32 {
        let e1 = a0.map(f32::floor);
        let e2 = (a0 - e1);
        let e5 = (Vn([1.0f32; 2]) - e2);
        let e19 = (((Vn([4.0f32; 2]) - ((e2 * e2) * Vn([6.0f32; 2]))) + (((e2 * e2) * e2) * Vn([3.0f32; 2]))) / Vn([6.0f32; 2]));
        let e25 = ((((e5 * e5) * e5) / Vn([6.0f32; 2])) + e19);
        let e28 = (Vn([1.0f32; 2]) - e25);
        let e33 = ((e1 + (e19 / e25)) - Vn([1.0f32; 2]));
        let e43 = ((e1 + ((((e2 * e2) * e2) / Vn([6.0f32; 2])) / e28)) + Vn([1.0f32; 2]));
        let e44 = bil2(c, e33);
        let e51 = bil2(c, Vn([e43.0[0], e33.0[1]]));
        let e59 = bil2(c, Vn([e33.0[0], e43.0[1]]));
        let e63 = bil2(c, e43);
        return ((((e44.0[0] * e25.0[0]) + (e51.0[0] * e28.0[0])) * e25.0[1]) + (((e59.0[0] * e25.0[0]) + (e63.0[0] * e28.0[0])) * e28.0[1]));
    }

    pub fn k_arc_edges(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e1 = pos(c, a0);
        let e4 = c.p.time;
        let e6 = pf(c, 22u32);
        let e7 = (e4 * e6);
        let e8 = f32::floor(e7);
        let e10 = pf(c, 21u32);
        let e11 = (e1 * Vn([e10; 2]));
        let e18 = c.p.seed;
        let e19 = fbm(c, Vn([e11.0[0], e11.0[1], (e8 * 3.7f32)]), 4i32, e18);
        let e21 = pf(c, 21u32);
        let e26 = ((e1 * Vn([e21; 2])) + Vn([4.0f32, 7.0f32]));
        let e33 = c.p.seed;
        let e36 = fbm(c, Vn([e26.0[0], e26.0[1], (e8 * 3.7f32)]), 4i32, e33.wrapping_add(1u32));
        let e39 = pf(c, 20u32);
        let e40 = (Vn([e19, e36]) * Vn([e39; 2]));
        let e43 = pic1(c, (e1 + e40), 0i32);
        let e44 = e43.0[0];
        let e49 = pf(c, 21u32);
        let e54 = pf2(c, 16u32);
        let e58 = ((((e1 + (e40 * Vn([0.6f32; 2]))) * Vn([e49; 2])) * Vn([1.3f32; 2])) + ((e54 * Vn([e7; 2])) * Vn([0.1f32; 2])));
        let e65 = c.p.seed;
        let e68 = ridges(c, Vn([e58.0[0], e58.0[1], (e8 * 2.1f32)]), 3i32, e65.wrapping_add(5u32));
        let e73 = pf(c, 18u32);
        let e82 = ss(c, 0.01f32, 0.25f32, e44);
        let e90 = pf(c, 19u32);
        let e99 = ss(c, 0.0f32, 0.4f32, e44);
        let e105 = c.p.seed;
        let e108 = wander(c, (e7 * 1.3f32), e105.wrapping_add(8u32));
        let e114 = pf3(c, 6u32);
        let e119 = pf3(c, 9u32);
        let e125 = glowing(c, ((((e114 * Vn([((f32::exp((((-(1.0f32 - e68)) / f32::max(e73, 0.001f32)) * 0.8f32)) * e82) * 1.6f32); 3])) * Vn([1.6f32; 3])) + ((e119 * Vn([(f32::exp((((-(1.0f32 - e68)) / f32::max(e90, 0.001f32)) * 0.6f32)) * e99); 3])) * Vn([0.8f32; 3]))) * Vn([(0.7f32 + (0.3f32 * e108)); 3])));
        return e125;
    }

    pub fn k_aurora(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let mut v1: i32 = 0i32;
        let e1 = pos(c, a0);
        let e3 = pf2(c, 0u32);
        let e4 = (e1 - e3);
        let e9 = pf(c, 2u32);
        'l0: loop {
            'b0: {
                let e16 = v1;
                let e20 = c.p.i[0];
                if (e16 < e20) {
                } else {
                    break 'l0;
                }
                let e22 = v1;
                let e23 = (e22 as f32);
                let e33 = (((e4.0[0] * (1.0f32 / f32::max(e9, 0.001f32))) * (0.8f32 + (0.25f32 * e23))) + (e23 * 3.7f32));
                let e37 = pf(c, 15u32);
                let e43 = c.p.seed;
                let e44 = noise3(c, Vn([(e33 * 0.7f32), e23, (e37 * 0.15f32)]), e43);
                let e52 = pf(c, 15u32);
                let e58 = c.p.seed;
                let e61 = noise3(c, Vn([(e33 * 1.9f32), (e23 + 4.0f32), (e52 * 0.25f32)]), e58.wrapping_add(2u32));
                let e66 = pf(c, 20u32);
                let e67 = (((e44 * 0.5f32) + (e61 * 0.2f32)) * e66);
                let e69 = pf(c, 21u32);
                let e79 = pf(c, 18u32);
                let e83 = pf(c, 15u32);
                let e89 = c.p.seed;
                let e92 = noise3(c, Vn([(e33 * 0.4f32), e23, (e83 * 0.1f32)]), e89.wrapping_add(5u32));
                let e100 = ((((e69 + (e67 * 0.4f32)) - (e23 * 0.05f32)) - e4.0[1]) / f32::max((e79 * (0.7f32 + (0.25f32 * e92))), 0.001f32));
                let e110 = pf(c, 15u32);
                let e116 = c.p.seed;
                let e119 = noise3(c, Vn([((e33 * 9.0f32) + (e67 * 5.0f32)), (e100 * 0.4f32), (e110 * 0.5f32)]), e116.wrapping_add(9u32));
                let e125 = pw(c, (0.5f32 + (0.5f32 * e119)), 1.5f32);
                let e133 = pf(c, 19u32);
                let e134 = mix1(c, 1.0f32, (0.45f32 + ((0.55f32 * e125) * 2.0f32)), e133);
                let e137 = ss(c, -0.05f32, 0.1f32, e100);
                let e153 = pf(c, 15u32);
                let e159 = c.p.seed;
                let e162 = noise3(c, Vn([(e33 * 0.5f32), (3.0f32 + e23), (e153 * 0.2f32)]), e159.wrapping_add(11u32));
                let e169 = pf3(c, 6u32);
                let e171 = pf3(c, 9u32);
                let e173 = pf3(c, 12u32);
                let e176 = ss(c, 0.3f32, 0.9f32, e100);
                let e177 = mix3(c, e171, e173, e176);
                let e180 = ss(c, 0.05f32, 0.45f32, e100);
                let e181 = mix3(c, e169, e177, e180);
                let e182 = v0;
                v0 = (e182 + ((e181 * Vn([f32::max(((((e137 * f32::exp(((-f32::max(e100, 0.0f32)) * 1.9f32))) * 1.35f32) * e134) * (0.6f32 + (0.4f32 * e162))), 0.0f32); 3])) / Vn([(1.0f32 + (0.35f32 * e23)); 3])));
            }
            let e194 = v1;
            v1 = e194.wrapping_add(1i32);
        }
        let e196 = v0;
        let e197 = glowing(c, e196);
        return e197;
    }

    pub fn k_beam(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e1 = pos(c, a0);
        let e3 = pf2(c, 0u32);
        let e5 = pf2(c, 16u32);
        let e8 = pf(c, 22u32);
        let e13 = (e3 + ((e5 - e3) * Vn([clampf(e8, 0.0f32, 1.0f32); 2])));
        let e14 = (e13 - e3);
        let e17 = f32::max(e14.length(), 1e-5f32);
        let e25 = clampf(((e1 - e3).dot((e14 / Vn([e17; 2]))) / e17), 0.0f32, 1.0f32);
        let e29 = ((e1 - e3) - (e14 * Vn([e25; 2]))).length();
        let e31 = pf(c, 21u32);
        let e34 = (1.0f32 - (e31 * e25));
        let e36 = pf(c, 23u32);
        let e43 = pf(c, 15u32);
        let e48 = pf(c, 15u32);
        let e53 = c.p.seed;
        let e54 = noise3(c, Vn([(((e25 * e17) * 14.0f32) - (e43 * 6.0f32)), 0.0f32, e48]), e53);
        let e59 = pf(c, 20u32);
        let e63 = pf(c, 15u32);
        let e68 = c.p.seed;
        let e71 = wander(c, (e63 * 18.0f32), e68.wrapping_add(3u32));
        let e80 = pf(c, 18u32);
        let e85 = f32::max(((e80 * 0.5f32) * e34), 0.0001f32);
        let e87 = sq(c, (e29 / e85));
        let e94 = pf(c, 19u32);
        let e99 = mix1(c, 1.0f32, e34, 0.6f32);
        let e100 = (f32::max(e94, 0.0001f32) * e99);
        let e124 = sq(c, ((e1 - e13).length() / ((e85 * 2.2f32) + (e100 * 0.35f32))));
        let e131 = ((f32::exp((-e124)) * 0.9f32) * step(0.01f32, e17));
        let e134 = ss(c, 0.0f32, 0.02f32, e17);
        let e136 = pf3(c, 6u32);
        let e144 = pf3(c, 9u32);
        let e152 = glowing(c, ((((e136 * Vn([(((f32::exp(((-e87) * 1.3f32)) * (1.0f32 + ((e36 * 0.6f32) * e54))) * 1.3f32) + (e131 * 0.6f32)); 3])) + (e144 * Vn([(((f32::exp((((-e29) / e100) * 1.6f32)) * 0.65f32) + (f32::exp((((-e29) / e100) * 0.45f32)) * 0.12f32)) + (e131 * 0.8f32)); 3]))) * Vn([(1.0f32 - ((e59 * 0.45f32) * (0.5f32 + (0.5f32 * e71)))); 3])) * Vn([e134; 3])));
        return e152;
    }

    pub fn k_bolt(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let mut v1: i32 = 0i32;
        let mut v2: i32 = 0i32;
        let mut v3: Vn<f32, 4> = Vn([0.0f32; 4]);
        let mut v4: i32 = 0i32;
        let e2 = pf2(c, 0u32);
        let e9 = pf2(c, 2u32);
        let e11 = (e2 + (Vn([(a0.0[0] as f32), (a0.0[1] as f32)]) * e9));
        let e13 = pf(c, 4u32);
        let e15 = pf(c, 4u32);
        let e16 = (e13 * e15);
        let e22 = c.p.i[0];
        'l0: loop {
            'b0: {
                let e29 = v1;
                if (e29 < idiv(e22.wrapping_add(7i32), 8i32)) {
                } else {
                    break 'l0;
                }
                let e32 = v1;
                let e35 = at0(c, 10i32, e32.wrapping_mul(8i32));
                if ((e11 - Vn([e35.0[0], e35.0[1]])).length() < (e35.0[2] + (3.1f32 * f32::sqrt(((e35.0[3] * e35.0[3]) + e16))))) {
                    let e49 = v1;
                    let e57 = c.p.i[0];
                    let e59 = v1;
                    v2 = e59.wrapping_mul(8i32);
                    'l1: loop {
                        'b1: {
                            let e63 = v2;
                            if (e63 < i32_min(e49.wrapping_mul(8i32).wrapping_add(8i32), e57)) {
                            } else {
                                break 'l1;
                            }
                            let e66 = v2;
                            let e67 = at0(c, 0i32, e66);
                            if ((e11 - Vn([e67.0[0], e67.0[1]])).length() < (e67.0[2] + (3.1f32 * f32::sqrt(((e67.0[3] * e67.0[3]) + e16))))) {
                                let e82 = v2;
                                let e83 = at0(c, 1i32, e82);
                                v3 = e83;
                                v4 = 2i32;
                                'l2: loop {
                                    'b2: {
                                        let e87 = v4;
                                        if (e87 < 10i32) {
                                        } else {
                                            break 'l2;
                                        }
                                        let e90 = v4;
                                        let e91 = v2;
                                        let e92 = at0(c, e90, e91);
                                        let e94 = v3;
                                        let e96 = (Vn([e92.0[0], e92.0[1]]) - Vn([e94.0[0], e94.0[1]]));
                                        let e97 = v3;
                                        let e107 = clampf(((e11 - Vn([e97.0[0], e97.0[1]])).dot(e96) / f32::max(e96.dot(e96), 1e-12f32)), 0.0f32, 1.0f32);
                                        let e108 = v3;
                                        let e112 = ((e11 - Vn([e108.0[0], e108.0[1]])) - (e96 * Vn([e107; 2])));
                                        let e114 = v3.0[2];
                                        let e116 = mix1(c, e114, e92.0[2], e107);
                                        let e118 = ((e116 * e116) + e16);
                                        let e119 = v0;
                                        let e121 = v3.0[3];
                                        let e123 = mix1(c, e121, e92.0[3], e107);
                                        v0 = f32::max(e119, (((e123 * e116) * f32::exp(((-e112.dot(e112)) / e118))) / f32::sqrt(e118)));
                                        v3 = e92;
                                    }
                                    let e134 = v4;
                                    v4 = e134.wrapping_add(1i32);
                                }
                            }
                        }
                        let e137 = v2;
                        v2 = e137.wrapping_add(1i32);
                    }
                }
            }
            let e140 = v1;
            v1 = e140.wrapping_add(1i32);
        }
        let e142 = v0;
        let e143 = v0;
        let e144 = v0;
        let e145 = v0;
        return Vn([e142, e143, e144, e145]);
    }

    pub fn k_bolt_light(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e1 = pos(c, a0);
        let e4 = at0(c, a0.0[0], a0.0[1]);
        let e5 = e4.0[0];
        let e7 = pf2(c, 8u32);
        let e10 = pf(c, 10u32);
        let e12 = halo1(c, ((e1 - e7) * Vn([e10; 2])));
        let e14 = pf(c, 11u32);
        let e17 = pf2(c, 12u32);
        let e20 = pf(c, 14u32);
        let e22 = halo2(c, ((e1 - e17) * Vn([e20; 2])));
        let e24 = pf(c, 15u32);
        let e27 = pf3(c, 1u32);
        let e32 = pf3(c, 4u32);
        let e40 = pf(c, 0u32);
        let e44 = (((e27 * Vn([(e5 * 1.25f32); 3])) + (e32 * Vn([(((e12 * e14) + (e22 * e24)) + (e5 * 0.3f32)); 3]))) * Vn([e40; 3])).zip(Vn([0.0f32; 3]), f32::max);
        return Vn([e44.0[0], e44.0[1], e44.0[2], clampf(f32::max(f32::max(e44.0[0], e44.0[1]), e44.0[2]), 0.0f32, 1.0f32)]);
    }

    pub fn k_caustics(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 2> = Vn([0.0f32; 2]);
        let mut v1: f32 = 1.0f32;
        let mut v2: i32 = 0i32;
        let e1 = place(c, a0);
        let e3 = pf(c, 15u32);
        let e5 = (e3 * 0.6f32);
        v0 = e1;
        'l0: loop {
            'b0: {
                let e11 = v2;
                if (e11 < 4i32) {
                } else {
                    break 'l0;
                }
                let e14 = v2;
                let e17 = ((e14 as f32) + 1.0f32);
                let e26 = v0.0[0];
                let e35 = v0.0[1];
                let e45 = v0.0[1];
                let e54 = v0.0[0];
                v0 = ((e1 * Vn([2.2f32; 2])) + Vn([(f32::cos(((e5 * (1.0f32 - (0.35f32 * e17))) - e26)) + f32::sin(((e5 * (1.0f32 - (0.35f32 * e17))) + e35))), (f32::sin(((e5 * (1.0f32 - (0.35f32 * e17))) - e45)) + f32::cos(((e5 * (1.0f32 - (0.35f32 * e17))) + e54)))]));
                let e64 = v0.0[0];
                let e75 = v0.0[1];
                let e83 = v1;
                v1 = (e83 + ((1.0f32 / f32::max(((Vn([((e1.0[0] * 2.2f32) / f32::max(f32::abs(f32::sin((e64 + e5))), 0.04f32)), ((e1.0[1] * 2.2f32) / f32::max(f32::abs(f32::cos((e75 + e5))), 0.04f32))]).length() * 0.28f32) + 0.12f32), 0.05f32)) * 0.12f32));
            }
            let e97 = v2;
            v2 = e97.wrapping_add(1i32);
        }
        let e100 = (e1 * Vn([1.4f32; 2]));
        let e106 = c.p.seed;
        let e107 = noise3(c, Vn([e100.0[0], e100.0[1], (e5 * 0.5f32)]), e106);
        let e113 = ((e1 * Vn([1.4f32; 2])) + Vn([5.0f32, 9.0f32]));
        let e119 = c.p.seed;
        let e122 = noise3(c, Vn([e113.0[0], e113.0[1], (e5 * 0.5f32)]), e119.wrapping_add(1u32));
        let e123 = Vn([e107, e122]);
        let e134 = c.p.seed;
        let e137 = cells(c, ((e1 * Vn([1.5f32; 2])) + (e123 * Vn([0.55f32; 2]))), 0.9f32, (e5 * 1.4f32), e134.wrapping_add(2u32));
        let e152 = c.p.seed;
        let e155 = cells(c, (((e1 * Vn([1.9f32; 2])) - (e123 * Vn([0.45f32; 2]))) + Vn([3.3f32, 1.7f32])), 0.9f32, (e5 * 1.7f32), e152.wrapping_add(3u32));
        let e167 = pf(c, 18u32);
        let e168 = pw(c, (1.0f32 - clampf(((e137.0[1] - e137.0[0]) * 1.5f32), 0.0f32, 1.0f32)), e167);
        let e180 = pf(c, 18u32);
        let e181 = pw(c, (1.0f32 - clampf(((e155.0[1] - e155.0[0]) * 1.5f32), 0.0f32, 1.0f32)), e180);
        let e187 = v1;
        let e194 = dyed(c, clampf((((e168 + (e181 * 0.7f32)) * 0.75f32) + (0.02f32 * e187)), 0.0f32, 1.0f32));
        return e194;
    }

    pub fn k_cells(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let e1 = place(c, a0);
        let e3 = pf(c, 18u32);
        let e5 = pf(c, 15u32);
        let e8 = c.p.seed;
        let e9 = cells(c, e1, e3, e5, e8);
        v0 = (1.0f32 - clampf(e9.0[0], 0.0f32, 1.0f32));
        let e20 = c.p.i[2];
        if (e20 == 1i32) {
            let e24 = pf(c, 19u32);
            let e28 = pf(c, 19u32);
            let e36 = ss(c, (e24 * 0.5f32), ((e28 * 0.5f32) + 0.05f32), (e9.0[1] - e9.0[0]));
            v0 = (1.0f32 - e36);
        } else {
            let e42 = c.p.i[2];
            if (e42 == 2i32) {
                let e51 = pf(c, 19u32);
                let e55 = pf(c, 19u32);
                let e63 = ss(c, (e51 * 0.4f32), ((e55 * 0.4f32) + 0.07f32), (e9.0[1] - e9.0[0]));
                v0 = ((0.25f32 + (0.75f32 * e9.0[2])) * e63);
            } else {
                let e68 = c.p.i[2];
                if (e68 == 3i32) {
                    let e76 = pf(c, 18u32);
                    let e78 = pf(c, 15u32);
                    let e81 = c.p.seed;
                    let e82 = cells(c, (e1 + Vn([0.02f32, 0.0f32])), e76, e78, e81);
                    v0 = ((0.5f32 + (((((e82.0[1] - e82.0[0]) - e9.0[1]) + e9.0[0]) / 0.02f32) * 0.35f32)) + ((e9.0[2] - 0.5f32) * 0.3f32));
                }
            }
        }
        let e101 = v0;
        let e102 = dyed(c, e101);
        return e102;
    }

    pub fn k_cloud_sky(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let e1 = pos(c, a0);
        let e3 = pf2(c, 0u32);
        let e6 = pf(c, 3u32);
        let e8 = turn(c, (e1 - e3), (-e6));
        let e10 = pf(c, 18u32);
        let e15 = f32::max(f32::abs((e10 - e8.0[1])), 0.004f32);
        let e19 = pf(c, 19u32);
        let e23 = pf(c, 2u32);
        let e31 = pf2(c, 16u32);
        let e33 = pf(c, 15u32);
        let e35 = (((Vn([(e8.0[0] / e15), (e19 / e15)]) / Vn([f32::max(e23, 0.001f32); 2])) * Vn([0.5f32; 2])) + (e31 * Vn([e33; 2])));
        let e38 = ss(c, 0.0f32, 0.22f32, e15);
        let e40 = pf(c, 15u32);
        let e47 = c.p.i[0];
        let e50 = c.p.seed;
        let e51 = fbm(c, Vn([e35.0[0], e35.0[1], (e40 * 0.2f32)]), e47, e50);
        let e55 = pf(c, 5u32);
        let e61 = clampf((((e51 * 0.8f32) * e55) + 0.5f32), 0.0f32, 1.0f32);
        let e63 = pf3(c, 6u32);
        let e65 = pf3(c, 9u32);
        let e72 = mix3(c, e63, e65, ((e61 * e61) * (3.0f32 - (2.0f32 * e61))));
        v0 = e72;
        let e75 = pf3(c, 12u32);
        let e76 = v0;
        let e77 = mix3(c, e75, e76, e38);
        v0 = e77;
        let e78 = v0;
        let e80 = pf(c, 4u32);
        let e82 = seen(c, (e78 * Vn([e80; 3])), e61);
        return e82;
    }

    pub fn k_cloud_storm(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e1 = place(c, a0);
        let e2 = e1.length();
        let e4 = pf(c, 18u32);
        let e11 = pf(c, 15u32);
        let e15 = turn(c, e1, ((e4 / ((e2 * 1.4f32) + 0.25f32)) + (e11 * 0.6f32)));
        let e17 = (e15 * Vn([0.9f32; 2]));
        let e19 = pf(c, 15u32);
        let e25 = c.p.seed;
        let e28 = noise3(c, Vn([e17.0[0], e17.0[1], (e19 * 0.2f32)]), e25.wrapping_add(7u32));
        let e34 = ((e15 * Vn([0.9f32; 2])) + Vn([3.0f32, 8.0f32]));
        let e36 = pf(c, 15u32);
        let e42 = c.p.seed;
        let e45 = noise3(c, Vn([e34.0[0], e34.0[1], (e36 * 0.2f32)]), e42.wrapping_add(8u32));
        let e49 = (e15 + (Vn([e28, e45]) * Vn([0.5f32; 2])));
        let e51 = pf(c, 15u32);
        let e58 = c.p.i[0];
        let e61 = c.p.seed;
        let e62 = fbm(c, Vn([e49.0[0], e49.0[1], (e51 * 0.15f32)]), e58, e61);
        let e64 = pf(c, 19u32);
        let e68 = pf(c, 19u32);
        let e73 = ss(c, (e64 * 0.5f32), ((e68 * 1.5f32) + 0.05f32), e2);
        let e84 = dyed(c, (((0.5f32 + (e62 * 0.8f32)) * e73) + ((1.0f32 - e73) * 0.02f32)));
        return e84;
    }

    pub fn k_clouds(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e1 = place(c, a0);
        let e3 = pf2(c, 16u32);
        let e5 = pf(c, 15u32);
        let e7 = (e1 + (e3 * Vn([e5; 2])));
        let e9 = pf(c, 15u32);
        let e11 = (e9 * 0.25f32);
        let e13 = (e7 * Vn([0.7f32; 2]));
        let e17 = c.p.seed;
        let e20 = noise3(c, Vn([e13.0[0], e13.0[1], e11]), e17.wrapping_add(21u32));
        let e26 = ((e7 * Vn([0.7f32; 2])) + Vn([4.1f32, 8.3f32]));
        let e30 = c.p.seed;
        let e33 = noise3(c, Vn([e26.0[0], e26.0[1], e11]), e30.wrapping_add(22u32));
        let e34 = Vn([e20, e33]);
        let e38 = pf(c, 18u32);
        let e40 = (e7 + ((e34 * Vn([0.45f32; 2])) * Vn([e38; 2])));
        let e45 = c.p.i[0];
        let e48 = c.p.seed;
        let e49 = fbm(c, Vn([e40.0[0], e40.0[1], e11]), e45, e48);
        let e54 = ((e7 * Vn([0.8f32; 2])) + (e34 * Vn([0.3f32; 2])));
        let e59 = c.p.i[0];
        let e62 = c.p.seed;
        let e65 = ridges(c, Vn([e54.0[0], e54.0[1], e11]), e59, e62.wrapping_add(40u32));
        let e75 = pf(c, 18u32);
        let e78 = dyed(c, ((0.5f32 + (e49 * 0.75f32)) + (((e65 - 0.62f32) * 0.5f32) * e75)));
        return e78;
    }

    pub fn k_clouds_color(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e1 = place(c, a0);
        let e3 = pf2(c, 16u32);
        let e5 = pf(c, 15u32);
        let e7 = (e1 + (e3 * Vn([e5; 2])));
        let e9 = pf(c, 15u32);
        let e11 = (e9 * 0.25f32);
        let e16 = c.p.i[0];
        let e19 = c.p.seed;
        let e20 = fbm(c, Vn([e7.0[0], e7.0[1], e11]), e16, e19);
        let e28 = (e7 + Vn([5.2f32, 1.3f32]));
        let e33 = c.p.i[0];
        let e36 = c.p.seed;
        let e39 = fbm(c, Vn([e28.0[0], e28.0[1], e11]), e33, e36.wrapping_add(301u32));
        let e47 = (e7 + Vn([9.7f32, 8.1f32]));
        let e52 = c.p.i[0];
        let e55 = c.p.seed;
        let e58 = fbm(c, Vn([e47.0[0], e47.0[1], e11]), e52, e55.wrapping_add(601u32));
        let e68 = pf(c, 5u32);
        let e77 = (((Vn([(0.5f32 + (e20 * 0.8f32)), (0.5f32 + (e39 * 0.8f32)), (0.5f32 + (e58 * 0.8f32))]) - Vn([0.5f32; 3])) * Vn([e68; 3])) + Vn([0.5f32; 3])).zip3(Vn([0.0f32; 3]), Vn([1.0f32; 3]), clampf);
        let e79 = pf3(c, 6u32);
        let e83 = pf3(c, 9u32);
        let e88 = pf3(c, 12u32);
        let e93 = pf(c, 4u32);
        let e103 = seen(c, ((((e79 * Vn([e77.0[0]; 3])) + (e83 * Vn([e77.0[1]; 3]))) + (e88 * Vn([e77.0[2]; 3]))) * Vn([e93; 3])), clampf(f32::max(f32::max(e77.0[0], e77.0[1]), e77.0[2]), 0.0f32, 1.0f32));
        return e103;
    }

    pub fn k_clouds_hue(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e1 = place(c, a0);
        let e3 = pf2(c, 16u32);
        let e5 = pf(c, 15u32);
        let e7 = (e1 + (e3 * Vn([e5; 2])));
        let e9 = pf(c, 15u32);
        let e11 = (e9 * 0.3f32);
        let e13 = (e7 * Vn([0.8f32; 2]));
        let e18 = c.p.seed;
        let e21 = fbm(c, Vn([e13.0[0], e13.0[1], e11]), 3i32, e18.wrapping_add(9u32));
        let e26 = (e7 + (Vn([e21, (-e21)]) * Vn([0.8f32; 2])));
        let e31 = c.p.i[0];
        let e34 = c.p.seed;
        let e35 = fbm(c, Vn([e26.0[0], e26.0[1], e11]), e31, e34);
        let e37 = pf(c, 18u32);
        let e40 = pf(c, 15u32);
        let e44 = fr(c, ((e35 * e37) + (e40 * 0.12f32)));
        let e46 = pf(c, 19u32);
        let e48 = hsv(c, e44, e46, 1.0f32);
        let e50 = pf(c, 18u32);
        let e62 = pf(c, 5u32);
        let e63 = contrast3(c, (e48 * Vn([(0.65f32 + (0.35f32 * f32::cos((((e35 * e50) + e21) * 6.2831855f32)))); 3])), e62);
        let e70 = pf(c, 4u32);
        let e77 = seen(c, (e63.zip3(Vn([0.0f32; 3]), Vn([2.0f32; 3]), clampf) * Vn([e70; 3])), clampf((0.5f32 + e35), 0.0f32, 1.0f32));
        return e77;
    }

    pub fn k_contours(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e1 = place(c, a0);
        let e3 = pf(c, 15u32);
        let e5 = (e3 * 0.15f32);
        let e6 = px(c);
        let e8 = pf(c, 2u32);
        let e11 = (e6 / f32::max(e8, 0.0001f32));
        let e16 = c.p.i[0];
        let e19 = c.p.seed;
        let e20 = fbm(c, Vn([e1.0[0], e1.0[1], e5]), e16, e19);
        let e22 = pf(c, 18u32);
        let e23 = (e20 * e22);
        let e26 = (e1 + Vn([e11, 0.0f32]));
        let e31 = c.p.i[0];
        let e34 = c.p.seed;
        let e35 = fbm(c, Vn([e26.0[0], e26.0[1], e5]), e31, e34);
        let e37 = pf(c, 18u32);
        let e41 = (e1 + Vn([0.0f32, e11]));
        let e46 = c.p.i[0];
        let e49 = c.p.seed;
        let e50 = fbm(c, Vn([e41.0[0], e41.0[1], e5]), e46, e49);
        let e52 = pf(c, 18u32);
        let e60 = line1(c, e23);
        let e63 = pf(c, 19u32);
        let e68 = stroke(c, (e60 / f32::max(Vn([((e35 * e37) - e23), ((e50 * e52) - e23)]).length(), 1e-5f32)), ((e63 * 0.02f32) / e11), 0.75f32);
        let e69 = dyed(c, e68);
        return e69;
    }

    pub fn k_dust(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let mut v1: i32 = 0i32;
        let mut v2: i32 = 0i32;
        let mut v3: i32 = 0i32;
        let e1 = pos(c, a0);
        'l0: loop {
            'b0: {
                let e6 = v1;
                let e10 = c.p.i[0];
                if (e6 < e10) {
                } else {
                    break 'l0;
                }
                let e12 = v1;
                let e13 = (e12 as f32);
                let e15 = pf(c, 2u32);
                let e24 = pf2(c, 19u32);
                let e26 = pf(c, 15u32);
                let e44 = (((e1 - ((e24 * Vn([e26; 2])) / Vn([(1.0f32 + (0.5f32 * e13)); 2]))) / Vn([f32::max(((e15 * (1.0f32 + (0.8f32 * e13))) * 9.0f32), 0.001f32); 2])) + Vn([(e13 * 7.3f32), (e13 * 3.1f32)]));
                let e45 = e44.map(f32::floor);
                v2 = (-1i32);
                'l1: loop {
                    'b1: {
                        let e48 = v2;
                        if (e48 <= 1i32) {
                        } else {
                            break 'l1;
                        }
                        v3 = (-1i32);
                        'l2: loop {
                            'b2: {
                                let e53 = v3;
                                if (e53 <= 1i32) {
                                } else {
                                    break 'l2;
                                }
                                let e58 = v3;
                                let e59 = (e45.0[0] as i32).wrapping_add(e58);
                                let e62 = v2;
                                let e63 = (e45.0[1] as i32).wrapping_add(e62);
                                let e66 = c.p.seed;
                                let e67 = v1;
                                let e72 = rnd2(c, e59, e63, e66.wrapping_add((e67 as u32).wrapping_mul(41u32)));
                                let e75 = c.p.seed;
                                let e76 = v1;
                                let e83 = rnd2(c, e59, e63, e75.wrapping_add((e76 as u32).wrapping_mul(83u32)).wrapping_add(1u32));
                                let e86 = pf(c, 18u32);
                                if (e72.0[0] < e86) {
                                    let e90 = (e72.0[1] * 6.2831855f32);
                                    let e101 = pf(c, 15u32);
                                    let e109 = pf(c, 15u32);
                                    let e123 = pf(c, 21u32);
                                    let e128 = (1.0f32 + ((e123 * e13) * 1.4f32));
                                    let e139 = pf(c, 15u32);
                                    let e154 = v0;
                                    let e157 = ss(c, 0.2f32, 1.0f32, ((e44 - (((Vn([(e59 as f32), (e63 as f32)]) + Vn([0.25f32; 2])) + (e83 * Vn([0.5f32; 2]))) + (Vn([f32::sin(((e101 * (0.4f32 + e72.0[1])) + e90)), f32::cos(((e109 * (0.3f32 + e72.0[0])) + (e90 * 1.7f32)))]) * Vn([0.2f32; 2])))).length() / ((0.03f32 + (0.035f32 * e83.0[0])) * e128)));
                                    v0 = (e154 + (((1.0f32 - e157) * (0.55f32 + (0.45f32 * f32::sin(((e139 * (1.0f32 + (2.5f32 * e72.0[1]))) + (e90 * 3.0f32)))))) / (e128 * 1.2f32)));
                                }
                            }
                            let e166 = v3;
                            v3 = e166.wrapping_add(1i32);
                        }
                    }
                    let e169 = v2;
                    v2 = e169.wrapping_add(1i32);
                }
            }
            let e172 = v1;
            v1 = e172.wrapping_add(1i32);
        }
        let e175 = pf3(c, 9u32);
        let e176 = v0;
        let e178 = glowing(c, (e175 * Vn([e176; 3])));
        return e178;
    }

    pub fn k_fall(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let mut v1: i32 = 0i32;
        let mut v2: Vn<f32, 2> = Vn([0.0f32; 2]);
        let e1 = pos(c, a0);
        let e3 = pf(c, 16u32);
        let e5 = turn(c, e1, (-e3));
        'l0: loop {
            'b0: {
                let e10 = v1;
                let e14 = c.p.i[0];
                if (e10 < e14) {
                } else {
                    break 'l0;
                }
                let e16 = v1;
                let e17 = (e16 as f32);
                let e19 = pf(c, 2u32);
                let e28 = ((f32::max(e19, 0.001f32) * (2.2f32 - (e17 * 0.32f32))) * 3.0f32);
                let e30 = pf(c, 19u32);
                let e36 = pf(c, 15u32);
                let e38 = pf(c, 17u32);
                let e49 = pf(c, 15u32);
                let e56 = pf(c, 20u32);
                v2 = Vn([(((e5.0[0] + ((f32::sin((((e5.0[1] * 2.0f32) + (e49 * 0.8f32)) + e17)) * e56) * 0.06f32)) / e28) + (e17 * 5.1f32)), ((e5.0[1] - ((e36 * e38) * (1.0f32 - (e17 * 0.13f32)))) / (e28 * (1.0f32 + (e30 * 7.0f32))))]);
                let e73 = v2.0[0];
                let e76 = v2.0[0];
                let e78 = v2.0[1];
                let e82 = c.p.seed;
                let e83 = v1;
                let e88 = rnd1(c, (f32::floor(e73) as i32), e82.wrapping_add((e83 as u32).wrapping_mul(3u32)));
                v2 = Vn([e76, (e78 - (e88 * 7.0f32))]);
                let e93 = v2;
                let e94 = e93.map(f32::floor);
                let e101 = c.p.seed;
                let e102 = v1;
                let e107 = rnd2(c, (e94.0[0] as i32), (e94.0[1] as i32), e101.wrapping_add((e102 as u32).wrapping_mul(47u32)));
                let e110 = pf(c, 18u32);
                if (e107.0[0] < e110) {
                    let e118 = c.p.seed;
                    let e119 = v1;
                    let e126 = rnd2(c, (e94.0[0] as i32), (e94.0[1] as i32), e118.wrapping_add((e119 as u32).wrapping_mul(59u32)).wrapping_add(1u32));
                    let e128 = v2.0[0];
                    let e140 = v2.0[1];
                    let e145 = Vn([((((e128 - e94.0[0]) - 0.25f32) - (e126.0[0] * 0.5f32)) * 1.0f32), ((e140 - e94.0[1]) - 0.5f32)]);
                    let e150 = (0.05f32 + (0.07f32 * e107.0[1]));
                    let e155 = ss(c, e150, (e150 * 0.2f32), f32::abs(e145.0[0]));
                    let e160 = ss(c, 0.45f32, 0.1f32, f32::abs(e145.0[1]));
                    let e172 = ss(c, (e150 * 1.6f32), (e150 * 0.3f32), Vn([e145.0[0], (e145.0[1] * 1.0f32)]).length());
                    let e173 = v0;
                    let e177 = pf(c, 19u32);
                    let e178 = ss(c, 0.0f32, 0.25f32, e177);
                    let e179 = mix1(c, e172, (e155 * e160), e178);
                    v0 = (e173 + ((e179 * (0.35f32 + (0.65f32 * e107.0[1]))) / (1.0f32 + (e17 * 0.35f32))));
                }
            }
            let e193 = v1;
            v1 = e193.wrapping_add(1i32);
        }
        let e196 = pf3(c, 9u32);
        let e197 = v0;
        let e199 = glowing(c, (e196 * Vn([e197; 3])));
        return e199;
    }

    pub fn k_fibers(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e1 = place(c, a0);
        let e3 = pf(c, 15u32);
        let e8 = (e1 + Vn([0.0f32, (e3 * 0.2f32)]));
        let e10 = (e8 * Vn([0.35f32; 2]));
        let e15 = c.p.seed;
        let e16 = noise3(c, Vn([e10.0[0], e10.0[1], 2.0f32]), e15);
        let e18 = (e8 * Vn([0.35f32; 2]));
        let e23 = c.p.seed;
        let e26 = noise3(c, Vn([e18.0[0], e18.0[1], 8.0f32]), e23.wrapping_add(9u32));
        let e30 = (e8 + (Vn([e16, e26]) * Vn([1.6f32; 2])));
        let e38 = pf(c, 18u32);
        let e39 = mix1(c, 9.0f32, 0.7f32, e38);
        let e45 = c.p.seed;
        let e48 = noise3(c, Vn([(e30.0[0] * 9.0f32), (e30.0[1] * e39), 0.0f32]), e45.wrapping_add(1u32));
        let e50 = (e8 * Vn([6.0f32; 2]));
        let e56 = c.p.i[0];
        let e59 = c.p.seed;
        let e62 = fbm(c, Vn([e50.0[0], e50.0[1], 1.0f32]), e56, e59.wrapping_add(2u32));
        let e70 = dyed(c, ((0.5f32 + (e48 * 0.38f32)) + (e62 * 0.3f32)));
        return e70;
    }

    pub fn k_flux(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e1 = place(c, a0);
        let e3 = pf(c, 15u32);
        let e5 = (e3 * 0.2f32);
        let e10 = c.p.seed;
        let e11 = fbm(c, Vn([e1.0[0], e1.0[1], e5]), 4i32, e10);
        let e15 = (e1 + Vn([5.2f32, 1.3f32]));
        let e20 = c.p.seed;
        let e23 = fbm(c, Vn([e15.0[0], e15.0[1], e5]), 4i32, e20.wrapping_add(1u32));
        let e26 = pf(c, 18u32);
        let e27 = (Vn([e11, e23]) * Vn([e26; 2]));
        let e34 = ((e1 + (e27 * Vn([2.2f32; 2]))) + Vn([1.7f32, 9.2f32]));
        let e39 = c.p.seed;
        let e42 = fbm(c, Vn([e34.0[0], e34.0[1], e5]), 4i32, e39.wrapping_add(2u32));
        let e49 = ((e1 + (e27 * Vn([2.2f32; 2]))) + Vn([8.3f32, 2.8f32]));
        let e54 = c.p.seed;
        let e57 = fbm(c, Vn([e49.0[0], e49.0[1], e5]), 4i32, e54.wrapping_add(3u32));
        let e60 = pf(c, 18u32);
        let e61 = (Vn([e42, e57]) * Vn([e60; 2]));
        let e64 = (e1 + (e61 * Vn([2.2f32; 2])));
        let e69 = c.p.i[0];
        let e72 = c.p.seed;
        let e75 = fbm(c, Vn([e64.0[0], e64.0[1], e5]), e69, e72.wrapping_add(4u32));
        let e86 = dyed(c, ((0.5f32 + (e75 * 0.9f32)) + ((e27.0[0] + e61.0[1]) * 0.25f32)));
        return e86;
    }

    pub fn k_gradient(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let e1 = pos(c, a0);
        let e3 = pf2(c, 2u32);
        let e5 = pf2(c, 0u32);
        let e6 = (e3 - e5);
        let e8 = pf2(c, 0u32);
        let e9 = (e1 - e8);
        let e12 = f32::max(e6.length(), 0.0001f32);
        v0 = (e9.dot(e6) / (e12 * e12));
        let e20 = c.p.i[2];
        if (e20 == 1i32) {
            v0 = (e9.length() / e12);
        } else {
            let e28 = c.p.i[2];
            if (e28 == 2i32) {
                let e31 = angle(c, e9);
                let e32 = angle(c, e6);
                let e38 = fr(c, (((e31 - e32) / 6.2831855f32) + 1.0f32));
                v0 = e38;
            } else {
                let e42 = c.p.i[2];
                if (e42 == 3i32) {
                    let e45 = angle(c, e6);
                    let e47 = turn(c, e9, (-e45));
                    v0 = ((f32::abs(e47.0[0]) + f32::abs(e47.0[1])) / e12);
                } else {
                    let e57 = c.p.i[2];
                    if (e57 == 4i32) {
                        let e60 = v0;
                        v0 = f32::abs(e60);
                    }
                }
            }
        }
        let e63 = pf(c, 21u32);
        if (e63 > 1.0f32) {
            let e66 = v0;
            let e68 = pf(c, 21u32);
            let e71 = md(c, (e66 * e68), 2.0f32);
            v0 = (1.0f32 - f32::abs((e71 - 1.0f32)));
        }
        let e77 = v0;
        v0 = clampf(e77, 0.0f32, 1.0f32);
        let e81 = full_pixel(c, a0);
        let e82 = v0;
        let e86 = rnd(c, e81.0[0], e81.0[1], 7u32);
        let e90 = pf(c, 20u32);
        v0 = clampf((e82 + ((e86 - 0.5f32) * e90)), 0.0f32, 1.0f32);
        let e96 = v0;
        let e97 = v0;
        let e98 = v0;
        let e100 = v0;
        let e107 = pf(c, 19u32);
        let e108 = mix1(c, e96, ((e97 * e98) * (3.0f32 - (2.0f32 * e100))), e107);
        v0 = e108;
        let e110 = pf3(c, 6u32);
        let e112 = pf(c, 9u32);
        let e113 = Vn([e110.0[0], e110.0[1], e110.0[2], e112]);
        let e115 = pf3(c, 10u32);
        let e117 = pf(c, 13u32);
        let e118 = Vn([e115.0[0], e115.0[1], e115.0[2], e117]);
        let e120 = pf3(c, 14u32);
        let e122 = pf(c, 17u32);
        let e127 = c.p.i[3];
        if (e127 == 3i32) {
            let e131 = pf(c, 18u32);
            let e134 = clampf(e131, 0.001f32, 0.999f32);
            let e135 = v0;
            if (e135 < e134) {
                let e137 = v0;
                let e139 = mix4(c, e113, e118, (e137 / e134));
                return e139;
            }
            let e140 = v0;
            let e145 = mix4(c, e118, Vn([e120.0[0], e120.0[1], e120.0[2], e122]), ((e140 - e134) / (1.0f32 - e134)));
            return e145;
        }
        let e146 = v0;
        let e147 = mix4(c, e113, e118, e146);
        return e147;
    }

    pub fn k_grid(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 2> = Vn([0.0f32; 2]);
        let mut v1: f32 = 0.0f32;
        let mut v2: f32 = 1.0f32;
        let mut v3: f32 = 1.0f32;
        let mut v4: Vn<f32, 2> = Vn([0.0f32; 2]);
        let mut v5: f32 = 0.0f32;
        let mut v6: bool = false;
        let mut v7: bool = false;
        let e1 = place(c, a0);
        v0 = e1;
        let e3 = px(c);
        let e5 = pf(c, 2u32);
        v1 = (e3 / f32::max(e5, 0.0001f32));
        let e15 = c.p.i[2];
        if (e15 == 3i32) {
            let e18 = pos(c, a0);
            let e20 = pf2(c, 0u32);
            let e23 = pf(c, 3u32);
            let e25 = turn(c, (e18 - e20), (-e23));
            let e28 = pf(c, 21u32);
            let e29 = (e25.0[1] - e28);
            if (e29 < 0.004f32) {
                let e33 = dyed(c, 0.0f32);
                let e37 = c.p.i[1];
                return (e33 * Vn([step(0.5f32, (1i32.wrapping_sub(i32_min(e37, 1i32)) as f32)); 4]));
            }
            let e52 = pf(c, 2u32);
            let e60 = pf(c, 15u32);
            v0 = (((Vn([(e25.0[0] / e29), (0.5f32 / e29)]) / Vn([f32::max(e52, 0.0001f32); 2])) * Vn([0.35f32; 2])) + Vn([0.0f32, e60]));
            let e64 = px(c);
            let e66 = pf(c, 2u32);
            v1 = ((((e64 / f32::max(e66, 0.0001f32)) * 0.35f32) / e29) * (1.0f32 + (0.5f32 / e29)));
            let e80 = ss(c, 0.0f32, 0.18f32, e29);
            v2 = e80;
        }
        let e86 = c.p.i[2];
        if (e86 == 2i32) {
            let e91 = Vn([1.0f32, 1.7320508f32]);
            let e92 = v0;
            let e93 = v0;
            let e100 = (e92 - (((e93 / e91).map(f32::floor) + Vn([0.5f32; 2])) * e91));
            let e101 = v0;
            let e102 = v0;
            let e112 = (e101 - ((((e102 / e91) - Vn([0.5f32; 2])).map(f32::floor) + Vn([1.0f32; 2])) * e91));
            v4 = e100;
            if (e112.dot(e112) < e100.dot(e100)) {
                v4 = e112;
            }
            let e117 = v4;
            let e118 = e117.map(f32::abs);
            v3 = (0.5f32 - f32::max(e118.0[0], ((e118.0[0] * 0.5f32) + (e118.0[1] * 0.8660254f32))));
        } else {
            let e133 = c.p.i[2];
            if (e133 == 1i32) {
                let e137 = v0.0[1];
                let e140 = line1(c, (e137 / 0.8660254f32));
                let e142 = v0.0[0];
                let e144 = v0.0[1];
                let e148 = line1(c, (e142 + (e144 * 0.57735026f32)));
                let e150 = v0.0[0];
                let e152 = v0.0[1];
                let e156 = line1(c, (e150 - (e152 * 0.57735026f32)));
                v3 = f32::min(f32::min(e140, e148), e156);
            } else {
                let e160 = v0.0[0];
                let e161 = line1(c, e160);
                let e163 = v0.0[1];
                let e164 = line1(c, e163);
                v3 = f32::min(e161, e164);
            }
        }
        let e167 = pf(c, 18u32);
        let e169 = (e167 * 0.5f32);
        let e171 = pf(c, 19u32);
        let e174 = v1;
        let e177 = f32::max((e171 * 0.5f32), (e174 * 0.75f32));
        let e178 = v3;
        let e179 = stroke(c, e178, e169, e177);
        v5 = e179;
        let e182 = pf(c, 20u32);
        if (e182 > 1.5f32) {
            let e190 = c.p.i[2];
            v6 = (e190 != 2i32);
        } else {
            v6 = false;
        }
        let e194 = v6;
        if e194 {
            let e200 = c.p.i[2];
            v7 = (e200 != 1i32);
        } else {
            v7 = false;
        }
        let e204 = v7;
        if e204 {
            let e206 = v0.0[0];
            let e208 = pf(c, 20u32);
            let e210 = line1(c, (e206 / e208));
            let e212 = v0.0[1];
            let e214 = pf(c, 20u32);
            let e216 = line1(c, (e212 / e214));
            let e219 = pf(c, 20u32);
            let e221 = v5;
            let e224 = stroke(c, (f32::min(e210, e216) * e219), e169, e177);
            v5 = f32::max((e221 * 0.55f32), e224);
        }
        let e226 = v5;
        let e227 = v2;
        let e229 = dyed(c, (e226 * e227));
        return e229;
    }

    pub fn k_grunge(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e1 = place(c, a0);
        let e3 = (e1 * Vn([1.5f32; 2]));
        let e8 = c.p.seed;
        let e9 = noise3(c, Vn([e3.0[0], e3.0[1], 1.0f32]), e8);
        let e11 = (e1 * Vn([1.5f32; 2]));
        let e16 = c.p.seed;
        let e19 = noise3(c, Vn([e11.0[0], e11.0[1], 5.0f32]), e16.wrapping_add(1u32));
        let e25 = (e1 + (Vn([e9, e19]) * Vn([0.6f32; 2])));
        let e31 = c.p.i[0];
        let e34 = c.p.seed;
        let e37 = fbm(c, Vn([e25.0[0], e25.0[1], 0.0f32]), e31, e34.wrapping_add(2u32));
        let e38 = ss(c, -0.05f32, 0.5f32, e37);
        let e40 = (e1 * Vn([3.0f32; 2]));
        let e46 = c.p.seed;
        let e49 = ridges(c, Vn([e40.0[0], e40.0[1], 2.0f32]), 3i32, e46.wrapping_add(3u32));
        let e56 = (e1 * Vn([0.3f32; 2]));
        let e61 = c.p.seed;
        let e64 = noise3(c, Vn([e56.0[0], e56.0[1], 8.0f32]), e61.wrapping_add(4u32));
        let e69 = turn(c, e1, (0.5f32 + (0.4f32 * e64)));
        let e80 = c.p.seed;
        let e83 = noise3(c, Vn([(e69.0[0] * 26.0f32), (e69.0[1] * 0.6f32), 3.0f32]), e80.wrapping_add(5u32));
        let e96 = c.p.seed;
        let e99 = noise3(c, Vn([((e69.0[1] * 34.0f32) + 9.0f32), (e69.0[0] * 0.8f32), 6.0f32]), e96.wrapping_add(6u32));
        let e102 = ss(c, 0.5f32, 0.62f32, e83);
        let e105 = ss(c, 0.56f32, 0.66f32, e99);
        let e113 = c.p.seed;
        let e116 = cells(c, (e1 * Vn([14.0f32; 2])), 0.95f32, 0.0f32, e113.wrapping_add(7u32));
        let e122 = ss(c, (0.3f32 * e116.0[2]), 0.0f32, e116.0[0]);
        let e128 = pf(c, 18u32);
        let e133 = pf(c, 19u32);
        let e139 = pf(c, 20u32);
        let e145 = dyed(c, clampf(((((e38 * (0.6f32 + (0.4f32 * e49))) * e128) + ((f32::min((e102 + e105), 1.0f32) * e133) * 0.8f32)) + ((e122 * step(0.6f32, e116.0[2])) * e139)), 0.0f32, 1.0f32));
        return e145;
    }

    pub fn k_moire(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let mut v1: f32 = 0.0f32;
        let e1 = pos(c, a0);
        let e3 = pf2(c, 0u32);
        let e7 = pf(c, 18u32);
        v0 = ((e1 - e3).length() * e7);
        let e11 = pf2(c, 16u32);
        let e15 = pf(c, 18u32);
        let e18 = pf(c, 19u32);
        v1 = (((e1 - e11).length() * e15) * e18);
        let e24 = c.p.i[2];
        if (e24 == 1i32) {
            let e28 = pf2(c, 0u32);
            let e31 = pf(c, 3u32);
            let e33 = turn(c, (e1 - e28), (-e31));
            let e36 = pf(c, 18u32);
            v0 = (e33.0[0] * e36);
            let e39 = pf2(c, 16u32);
            let e42 = pf(c, 3u32);
            let e45 = pf(c, 20u32);
            let e47 = turn(c, (e1 - e39), ((-e42) - e45));
            let e50 = pf(c, 18u32);
            let e53 = pf(c, 19u32);
            v1 = ((e47.0[0] * e50) * e53);
        }
        let e55 = v0;
        let e57 = pf(c, 15u32);
        let e62 = v1;
        let e64 = pf(c, 15u32);
        let e74 = dyed(c, (0.5f32 + (0.5f32 * (f32::sin(((e55 - e57) * 6.2831855f32)) * f32::sin(((e62 + e64) * 6.2831855f32))))));
        return e74;
    }

    pub fn k_moon(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let mut v1: Vn<f32, 2> = Vn([0.0f32; 2]);
        let mut v2: i32 = 0i32;
        let e1 = pos(c, a0);
        let e3 = pf2(c, 0u32);
        let e6 = pf(c, 2u32);
        let e12 = pf(c, 3u32);
        let e14 = turn(c, ((e1 - e3) / Vn([f32::max(e6, 0.0001f32); 2])), (-e12));
        let e15 = e14.length();
        let e16 = px(c);
        let e18 = pf(c, 2u32);
        let e21 = (e16 / f32::max(e18, 0.0001f32));
        let e26 = ss(c, (1.0f32 - e21), (1.0f32 + e21), e15);
        let e28 = (1.0f32 - e26);
        let e30 = f32::min(e15, 0.9995f32);
        let e35 = ((e14 / Vn([f32::max(e15, 1e-5f32); 2])) * Vn([e30; 2]));
        let e40 = Vn([e35.0[0], e35.0[1], f32::sqrt((1.0f32 - (e30 * e30)))]);
        let e43 = (Vn([e40.0[0], e40.0[1]]) * Vn([1.6f32; 2]));
        let e49 = pf(c, 15u32);
        let e55 = (Vn([e43.0[0], e43.0[1], (e40.0[2] * 1.6f32)]) + Vn([(e49 * 0.03f32), 0.0f32, 0.0f32]));
        let e63 = c.p.seed;
        let e64 = fbm(c, (e55 * Vn([0.9f32; 3])), 5i32, e63);
        let e65 = ss(c, -0.02f32, 0.32f32, e64);
        let e71 = c.p.seed;
        let e74 = fbm(c, (e55 * Vn([5.0f32; 3])), 4i32, e71.wrapping_add(1u32));
        'l0: loop {
            'b0: {
                let e81 = v2;
                if (e81 < 3i32) {
                } else {
                    break 'l0;
                }
                let e84 = v2;
                let e104 = c.p.seed;
                let e105 = v2;
                let e112 = cells(c, (Vn([f32::atan2(e40.0[0], e40.0[2]), f32::asin(clampf(e40.0[1], -1.0f32, 1.0f32))]) * Vn([(2.4f32 + ((e84 as f32) * 3.3f32)); 2])), 0.8f32, 0.0f32, e104.wrapping_add((e105 as u32).wrapping_mul(7u32)).wrapping_add(3u32));
                let e119 = (e112.0[0] / (0.18f32 + (0.16f32 * e112.0[2])));
                let e124 = sq(c, ((e119 - 1.0f32) * 4.0f32));
                let e129 = ss(c, 0.6f32, 1.0f32, e119);
                let e135 = v0;
                let e140 = v2;
                v0 = (e135 + (((f32::exp((-e124)) - (0.55f32 * (1.0f32 - e129))) * step(0.35f32, e112.0[2])) / (1.0f32 + (0.6f32 * (e140 as f32)))));
            }
            let e149 = v2;
            v2 = e149.wrapping_add(1i32);
        }
        let e152 = pf(c, 17u32);
        let e156 = ((1.0f32 - e152) * 3.1415927f32);
        let e161 = pf(c, 16u32);
        let e162 = turn(c, Vn([f32::sin(e156), 0.0f32]), e161);
        let e182 = pf(c, 18u32);
        let e191 = v0;
        let e195 = pf(c, 19u32);
        let e199 = pf3(c, 9u32);
        let e204 = pw(c, f32::max(Vn([(e40.0[0] + (e74 * 0.06f32)), (e40.0[1] + (e74 * 0.04f32)), e40.0[2]]).normalize().dot(Vn([e162.0[0], e162.0[1], f32::cos(e156)])), 0.0f32), 0.75f32);
        let e206 = pf(c, 21u32);
        let e212 = pf(c, 17u32);
        let e214 = pf(c, 20u32);
        let e215 = (e212 * e214);
        let e241 = pf3(c, 12u32);
        let e248 = pf(c, 4u32);
        let e249 = (((((e199 * Vn([f32::max((((0.82f32 - ((0.38f32 * e65) * e182)) * (1.0f32 + (e74 * 0.22f32))) + ((e191 * 0.16f32) * e195)), 0.05f32); 3])) * Vn([(e204 + (e206 * 0.06f32)); 3])) * Vn([e28; 3])) + ((e241 * Vn([(((f32::exp(((-f32::max((e15 - 1.0f32), 0.0f32)) * 5.0f32)) * 0.28f32) * e215) + ((f32::exp(((-f32::max((e15 - 1.0f32), 0.0f32)) * 1.5f32)) * 0.08f32) * e215)); 3])) * Vn([(1.0f32 - e28); 3]))) * Vn([e248; 3]));
        return Vn([e249.0[0], e249.0[1], e249.0[2], f32::max(e28, clampf(f32::max(f32::max(e249.0[0], e249.0[1]), e249.0[2]), 0.0f32, 1.0f32))]);
    }

    pub fn k_muzzle(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let mut v1: u32 = 0u32;
        let e2 = pf(c, 19u32);
        v0 = e2;
        let e6 = c.p.seed;
        v1 = e6;
        let e9 = pf(c, 20u32);
        if (e9 > 0.0f32) {
            let e13 = pf(c, 15u32);
            let e15 = pf(c, 20u32);
            let e16 = (e13 * e15);
            let e17 = fr(c, e16);
            v0 = clampf((e17 * 2.2f32), 0.0f32, 1.0f32);
            let e25 = c.p.seed;
            v1 = e25.wrapping_add(((f32::floor(e16) as i32) as u32).wrapping_mul(97u32));
        }
        let e32 = pos(c, a0);
        let e34 = pf2(c, 0u32);
        let e37 = pf(c, 3u32);
        let e39 = turn(c, (e32 - e34), (-e37));
        let e41 = pf(c, 2u32);
        let e45 = (e39 / Vn([f32::max(e41, 0.0001f32); 2]));
        let e48 = v0;
        let e49 = ss(c, 0.0f32, 1.0f32, e48);
        let e51 = (1.0f32 - e49);
        let e52 = v0;
        let e56 = (0.65f32 + (0.5f32 * e52));
        let e58 = (e45.length() / e56);
        let e59 = angle(c, e45);
        let e62 = v0;
        let e67 = ((e45 * Vn([3.2f32; 2])) - Vn([(e62 * 1.5f32), 0.0f32]));
        let e68 = v1;
        let e74 = v1;
        let e75 = fbm(c, Vn([e67.0[0], e67.0[1], (imod(e68, 97u32) as f32)]), 4i32, e74);
        let e77 = (e45.0[0] / e56);
        let e87 = sq(c, ((e45.0[1] / e56) / (0.1f32 + (f32::max(e77, 0.0f32) * 0.34f32))));
        let e92 = ss(c, -0.06f32, 0.1f32, e77);
        let e102 = pf(c, 16u32);
        let e111 = v1;
        let e114 = wander(c, (e59 * 1.6f32), e111.wrapping_add(2u32));
        let e124 = pw(c, (0.5f32 + (0.5f32 * f32::cos(((e59 * f32::max(f32::floor((e102 + 0.5f32)), 2.0f32)) + (e114 * 1.6f32))))), 5.0f32);
        let e135 = sq(c, ((f32::abs(e59) - 1.15f32) * 4.5f32));
        let e149 = pf(c, 17u32);
        let e166 = (f32::max(((((((f32::exp((-e87)) * e92) * f32::exp(((-f32::max(e77, 0.0f32)) * 1.55f32))) * 1.5f32) + ((e124 * f32::exp(((-e58) * 2.4f32))) * 0.75f32)) + ((f32::exp((-e135)) * f32::exp(((-e58) * 2.2f32))) * e149)) * (0.75f32 + (e75 * 1.3f32))), 0.0f32) * e51);
        let e168 = pf3(c, 6u32);
        let e172 = pf3(c, 12u32);
        let e174 = pf3(c, 9u32);
        let e178 = mix3(c, e172, e174, clampf(e166, 0.0f32, 1.0f32));
        let e184 = pf3(c, 9u32);
        let e195 = pf(c, 4u32);
        let e199 = (((((e168 * Vn([(f32::exp((((-e58) * e58) * 22.0f32)) * 2.0f32); 3])) * Vn([e51; 3])) + ((e178 * Vn([e166; 3])) * Vn([1.6f32; 3]))) + (((e184 * Vn([f32::exp(((-e58) * 1.7f32)); 3])) * Vn([0.25f32; 3])) * Vn([e51; 3]))) * Vn([e195; 3])).zip(Vn([0.0f32; 3]), f32::max);
        let e210 = pf(c, 18u32);
        let e214 = v0;
        let e215 = ss(c, 0.1f32, 0.8f32, e214);
        let e219 = v0;
        let e220 = ss(c, 0.85f32, 1.0f32, e219);
        let e226 = clampf(((((f32::max((0.25f32 + e75), 0.0f32) * f32::exp(((-e58) * 1.3f32))) * e210) * e215) * (1.0f32 - e220)), 0.0f32, 0.9f32);
        let e230 = (e199 + (Vn([0.32f32; 3]) * Vn([e226; 3])));
        return Vn([e230.0[0], e230.0[1], e230.0[2], clampf(f32::max(f32::max(f32::max(e199.0[0], e199.0[1]), e199.0[2]), e226), 0.0f32, 1.0f32)]);
    }

    pub fn k_nebula(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let e1 = place(c, a0);
        let e3 = pf2(c, 16u32);
        let e5 = pf(c, 15u32);
        let e7 = (e1 + (e3 * Vn([e5; 2])));
        let e9 = pf(c, 15u32);
        let e11 = (e9 * 0.12f32);
        let e13 = (e7 * Vn([0.6f32; 2]));
        let e18 = c.p.seed;
        let e21 = fbm(c, Vn([e13.0[0], e13.0[1], e11]), 3i32, e18.wrapping_add(3u32));
        let e27 = ((e7 * Vn([0.6f32; 2])) + Vn([7.7f32, 2.2f32]));
        let e32 = c.p.seed;
        let e35 = fbm(c, Vn([e27.0[0], e27.0[1], e11]), 3i32, e32.wrapping_add(4u32));
        let e36 = Vn([e21, e35]);
        let e39 = (e7 + (e36 * Vn([1.1f32; 2])));
        let e44 = c.p.i[0];
        let e47 = c.p.seed;
        let e48 = fbm(c, Vn([e39.0[0], e39.0[1], e11]), e44, e47);
        let e55 = clampf((0.55f32 + (e48 * 1.1f32)), 0.0f32, 1.0f32);
        let e60 = ((e7 * Vn([1.3f32; 2])) + (e36 * Vn([0.8f32; 2])));
        let e65 = c.p.i[0];
        let e68 = c.p.seed;
        let e71 = ridges(c, Vn([e60.0[0], e60.0[1], e11]), e65, e68.wrapping_add(50u32));
        let e81 = pw(c, clampf((0.6f32 + (e36.0[0] * 1.2f32)), 0.0f32, 1.0f32), 3.0f32);
        let e83 = pf3(c, 6u32);
        let e85 = pf3(c, 9u32);
        let e88 = ss(c, 0.25f32, 0.85f32, e55);
        let e89 = mix3(c, e83, e85, e88);
        v0 = (e89 * Vn([(0.25f32 + (e55 * 0.9f32)); 3]));
        let e96 = v0;
        let e98 = pf3(c, 12u32);
        let e102 = mix3(c, e96, (e98 * Vn([1.25f32; 3])), (e81 * e71));
        v0 = e102;
        let e103 = pos(c, a0);
        let e110 = c.p.seed;
        let e113 = cells(c, (e103 * Vn([42.0f32; 2])), 0.9f32, 0.0f32, e110.wrapping_add(80u32));
        let e117 = ss(c, 0.13f32, 0.0f32, e113.0[0]);
        let e119 = pw(c, e117, 3.0f32);
        let e125 = pf(c, 18u32);
        let e127 = v0;
        let e129 = pf(c, 5u32);
        let e130 = contrast3(c, e127, e129);
        v0 = (e130 + Vn([(((e119 * step(0.55f32, e113.0[2])) * e125) * (1.0f32 - (e55 * 0.6f32))); 3]));
        let e138 = v0;
        let e143 = pf(c, 4u32);
        let e145 = seen(c, (e138.zip(Vn([0.0f32; 3]), f32::max) * Vn([e143; 3])), e55);
        return e145;
    }

    pub fn k_nerves(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e1 = place(c, a0);
        let e3 = (e1 * Vn([1.6f32; 2]));
        let e5 = pf(c, 15u32);
        let e11 = c.p.seed;
        let e12 = noise3(c, Vn([e3.0[0], e3.0[1], (e5 * 0.2f32)]), e11);
        let e18 = ((e1 * Vn([1.6f32; 2])) + Vn([7.0f32, 3.0f32]));
        let e20 = pf(c, 15u32);
        let e26 = c.p.seed;
        let e29 = noise3(c, Vn([e18.0[0], e18.0[1], (e20 * 0.2f32)]), e26.wrapping_add(1u32));
        let e32 = (Vn([e12, e29]) * Vn([0.45f32; 2]));
        let e36 = pf(c, 15u32);
        let e41 = c.p.seed;
        let e44 = cells(c, (e1 + e32), 0.95f32, (e36 * 0.5f32), e41.wrapping_add(2u32));
        let e50 = pf(c, 15u32);
        let e55 = c.p.seed;
        let e58 = cells(c, ((e1 * Vn([2.3f32; 2])) - e32), 0.95f32, (e50 * 0.7f32), e55.wrapping_add(3u32));
        let e64 = pf(c, 18u32);
        let e76 = pf(c, 18u32);
        let e93 = pf(c, 19u32);
        let e95 = pf(c, 15u32);
        let e120 = dyed(c, clampf(((((f32::exp((((-(e44.0[1] - e44.0[0])) / f32::max(e64, 0.001f32)) * 2.2f32)) * (1.0f32 + (e93 * f32::sin((((e95 * 4.0f32) - (e44.0[0] * 9.0f32)) + (e44.0[2] * 6.2831855f32)))))) + (f32::exp((((-(e58.0[1] - e58.0[0])) / f32::max(e76, 0.001f32)) * 3.2f32)) * 0.4f32)) + (f32::exp((((-e44.0[0]) * e44.0[0]) * 26.0f32)) * 1.3f32)) * 0.8f32), 0.0f32, 1.0f32));
        return e120;
    }

    pub fn k_paint(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 2> = Vn([0.0f32; 2]);
        let mut v1: i32 = 0i32;
        let e1 = place(c, a0);
        v0 = e1;
        'l0: loop {
            'b0: {
                let e5 = v1;
                if (e5 < 4i32) {
                } else {
                    break 'l0;
                }
                let e8 = v0;
                let e10 = (e8 * Vn([0.55f32; 2]));
                let e12 = pf(c, 15u32);
                let e18 = c.p.seed;
                let e19 = noise3(c, Vn([e10.0[0], e10.0[1], (e12 * 0.1f32)]), e18);
                let e21 = (e19 * 6.2831855f32);
                let e22 = v0;
                let e29 = pf(c, 18u32);
                v0 = (e22 + ((Vn([f32::cos(e21), f32::sin(e21)]) * Vn([0.12f32; 2])) * Vn([e29; 2])));
            }
            let e33 = v1;
            v1 = e33.wrapping_add(1i32);
        }
        let e35 = v0;
        let e37 = (e35 * Vn([0.9f32; 2]));
        let e39 = pf(c, 15u32);
        let e46 = c.p.seed;
        let e49 = fbm(c, Vn([e37.0[0], e37.0[1], (e39 * 0.1f32)]), 3i32, e46.wrapping_add(2u32));
        let e55 = (e1 * Vn([0.55f32; 2]));
        let e57 = pf(c, 15u32);
        let e63 = c.p.seed;
        let e64 = noise3(c, Vn([e55.0[0], e55.0[1], (e57 * 0.1f32)]), e63);
        let e68 = turn(c, e1, (-(e64 * 6.2831855f32)));
        let e79 = c.p.seed;
        let e82 = noise3(c, Vn([(e68.0[0] * 1.5f32), (e68.0[1] * 16.0f32), 0.0f32]), e79.wrapping_add(3u32));
        let e86 = pf(c, 19u32);
        let e89 = dyed(c, ((0.5f32 + (e49 * 0.9f32)) + ((e82 * 0.16f32) * e86)));
        return e89;
    }

    pub fn k_plasma(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let e1 = place(c, a0);
        let e3 = pf(c, 15u32);
        v0 = (f32::sin(((e1.0[0] * 2.0f32) + e3)) + f32::sin((((e1.0[1] * 2.0f32) + e3) * 0.9f32)));
        let e18 = v0;
        let e25 = pf(c, 18u32);
        v0 = (e18 + f32::sin(((((e1.0[0] + e1.0[1]) * 1.4f32) * e25) + (e3 * 0.7f32))));
        let e43 = (e1 + Vn([(f32::sin((e3 * 0.33f32)) * 1.2f32), (f32::cos((e3 * 0.5f32)) * 1.2f32)]));
        let e44 = v0;
        let e49 = pf(c, 18u32);
        v0 = (e44 + f32::sin((f32::sqrt((((e43.dot(e43) * 5.0f32) * e49) + 1.0f32)) + e3)));
        let e57 = v0;
        let e68 = dyed(c, (0.5f32 + (0.5f32 * f32::sin(((e57 * 1.25f32) + (e3 * 0.3f32))))));
        return e68;
    }

    pub fn k_relief(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let mut v1: Vn<f32, 2> = Vn([0.0f32; 2]);
        let mut v2: i32 = 0i32;
        let e1 = place(c, a0);
        let e2 = px(c);
        let e4 = pf(c, 2u32);
        let e9 = f32::max((e2 / f32::max(e4, 0.0001f32)), 0.012f32);
        'l0: loop {
            'b0: {
                let e17 = v2;
                if (e17 < 3i32) {
                } else {
                    break 'l0;
                }
                let e20 = v2;
                let e24 = v2;
                let e31 = relief1(c, (e1 + (Vn([(imod(e20, 2i32) as f32), (idiv(e24, 2i32) as f32)]) * Vn([e9; 2]))));
                let e32 = v2;
                if (e32 == 0i32) {
                    v0 = e31;
                } else {
                    let e35 = v2;
                    if (e35 == 1i32) {
                        let e39 = v0;
                        v1.0[0] = ((e31 - e39) / e9);
                    } else {
                        let e43 = v0;
                        v1.0[1] = ((e31 - e43) / e9);
                    }
                }
            }
            let e47 = v2;
            v2 = e47.wrapping_add(1i32);
        }
        let e49 = v1;
        let e52 = pf(c, 18u32);
        let e53 = ((-e49) * Vn([e52; 2]));
        let e58 = pf(c, 16u32);
        let e61 = pf(c, 16u32);
        let e64 = pf(c, 17u32);
        let e75 = v0;
        let e81 = dyed(c, (((clampf(Vn([e53.0[0], e53.0[1], 1.0f32]).normalize().dot(Vn([f32::cos(e58), f32::sin(e61), f32::max(e64, 0.05f32)]).normalize()), 0.0f32, 1.0f32) * 0.8f32) + (e75 * 0.15f32)) + 0.05f32));
        return e81;
    }

    pub fn k_shape(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let e1 = pos(c, a0);
        let e3 = pf2(c, 0u32);
        let e6 = pf(c, 3u32);
        let e8 = turn(c, (e1 - e3), (-e6));
        let e10 = pf(c, 2u32);
        let e12 = f32::max(e10, 0.0001f32);
        let e15 = pf(c, 16u32);
        let e22 = (Vn([(e8.0[0] / f32::max(e15, 0.001f32)), e8.0[1]]) / Vn([e12; 2]));
        v0 = (e22.length() - 1.0f32);
        let e30 = c.p.i[0];
        let e33 = f32::max((e30 as f32), 3.0f32);
        let e37 = c.p.i[2];
        if (e37 == 1i32) {
            let e41 = pf(c, 17u32);
            let e46 = (e22.map(f32::abs) - Vn([(1.0f32 - e41); 2]));
            v0 = ((e46.zip(Vn([0.0f32; 2]), f32::max).length() + f32::min(f32::max(e46.0[0], e46.0[1]), 0.0f32)) - e41);
        } else {
            let e61 = c.p.i[2];
            if (e61 == 2i32) {
                let e64 = poly1(c, e22, e33);
                let e67 = pf(c, 17u32);
                let e68 = mix1(c, e64, e22.length(), e67);
                v0 = (e68 - 1.0f32);
            } else {
                let e74 = c.p.i[2];
                if (e74 == 3i32) {
                    let e78 = pf(c, 20u32);
                    let e81 = star1(c, e22, e33, f32::max(e78, 0.05f32));
                    v0 = (e81 - 1.0f32);
                } else {
                    let e87 = c.p.i[2];
                    if (e87 == 4i32) {
                        let e98 = (Vn([f32::abs(e22.0[0]), ((-e22.0[1]) + 0.12f32)]) * Vn([1.05f32; 2]));
                        v0 = (f32::sqrt(f32::max((((e98.0[0] * e98.0[0]) + (e98.0[1] * e98.0[1])) - (((0.75f32 * e98.0[0]) * e98.0[1]) * 1.35f32)), 0.0f32)) - 0.86f32);
                    } else {
                        let e122 = c.p.i[2];
                        if (e122 == 5i32) {
                            let e125 = e22.map(f32::abs);
                            let e128 = pf(c, 20u32);
                            let e141 = pf(c, 20u32);
                            v0 = f32::min(f32::max((e125.0[0] - f32::max(e128, 0.05f32)), (e125.0[1] - 1.0f32)), f32::max((e125.0[0] - 1.0f32), (e125.0[1] - f32::max(e141, 0.05f32))));
                        } else {
                            let e150 = c.p.i[2];
                            if (e150 == 6i32) {
                                let e157 = pf(c, 20u32);
                                let e165 = pf(c, 20u32);
                                v0 = (f32::abs(((e22.length() - 1.0f32) + (f32::max(e157, 0.02f32) * 0.5f32))) - (f32::max(e165, 0.02f32) * 0.5f32));
                            }
                        }
                    }
                }
            }
        }
        let e172 = pf(c, 19u32);
        if (e172 > 0.0f32) {
            let e175 = v0;
            let e177 = pf(c, 19u32);
            let e183 = pf(c, 19u32);
            v0 = (f32::abs((e175 + (e177 * 0.5f32))) - (e183 * 0.5f32));
        }
        let e188 = pf(c, 18u32);
        let e189 = px(c);
        let e193 = f32::max(e188, ((e189 / e12) * 0.75f32));
        let e195 = v0;
        let e196 = ss(c, (-e193), e193, e195);
        let e200 = pf(c, 12u32);
        let e202 = pf3(c, 9u32);
        let e203 = (e202 * Vn([e200; 3]));
        let e206 = pf3(c, 6u32);
        let e209 = mix4(c, Vn([e203.0[0], e203.0[1], e203.0[2], e200]), Vn([e206.0[0], e206.0[1], e206.0[2], 1.0f32]), (1.0f32 - e196));
        return e209;
    }

    pub fn k_silk(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e1 = place(c, a0);
        let e3 = pf(c, 15u32);
        let e7 = (e1 * Vn([0.6f32; 2]));
        let e12 = c.p.seed;
        let e13 = fbm(c, Vn([e7.0[0], e7.0[1], (e3 * 0.2f32)]), 3i32, e12);
        let e15 = pf(c, 18u32);
        let e16 = (e13 * e15);
        let e25 = pf(c, 19u32);
        let e27 = (((e1.0[0] * 1.2f32) + (e16 * 2.6f32)) + (e1.0[1] * e25));
        let e55 = pw(c, (0.5f32 + (0.5f32 * f32::cos((((e27 * 6.2831855f32) * 1.0f32) + 0.9f32)))), 6.0f32);
        let e66 = dyed(c, (((0.42f32 + (f32::sin((e27 * 6.2831855f32)) * 0.2f32)) + (f32::sin((((((e27 * 1.7f32) + (e1.0[1] * 0.3f32)) + e16) * 6.2831855f32) + 1.2f32)) * 0.12f32)) + (e55 * 0.35f32)));
        return e66;
    }

    pub fn k_sparkles(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let mut v1: i32 = 0i32;
        let mut v2: i32 = 0i32;
        let mut v3: i32 = 0i32;
        let e1 = pos(c, a0);
        'l0: loop {
            'b0: {
                let e7 = v1;
                let e11 = c.p.i[0];
                if (e7 < e11) {
                } else {
                    break 'l0;
                }
                let e13 = v1;
                let e14 = (e13 as f32);
                let e16 = pf(c, 2u32);
                let e27 = pf2(c, 22u32);
                let e29 = pf(c, 15u32);
                let e39 = (((e1 - (e27 * Vn([e29; 2]))) / Vn([((f32::max(e16, 0.001f32) * (1.0f32 - (0.22f32 * e14))) * 4.0f32); 2])) + Vn([(e14 * 3.9f32), (e14 * 8.2f32)]));
                let e40 = e39.map(f32::floor);
                v2 = (-1i32);
                'l1: loop {
                    'b1: {
                        let e43 = v2;
                        if (e43 <= 1i32) {
                        } else {
                            break 'l1;
                        }
                        v3 = (-1i32);
                        'l2: loop {
                            'b2: {
                                let e48 = v3;
                                if (e48 <= 1i32) {
                                } else {
                                    break 'l2;
                                }
                                let e53 = v3;
                                let e54 = (e40.0[0] as i32).wrapping_add(e53);
                                let e57 = v2;
                                let e58 = (e40.0[1] as i32).wrapping_add(e57);
                                let e61 = c.p.seed;
                                let e62 = v1;
                                let e67 = rnd2(c, e54, e58, e61.wrapping_add((e62 as u32).wrapping_mul(29u32)));
                                let e69 = pf(c, 15u32);
                                let e71 = pf(c, 19u32);
                                let e76 = ((e69 * e71) + (e67.0[0] * 13.0f32));
                                let e77 = f32::floor(e76);
                                let e88 = c.p.seed;
                                let e89 = v1;
                                let e96 = rnd2(c, e54.wrapping_add((e77 as i32).wrapping_mul(17i32)), e58.wrapping_sub((e77 as i32).wrapping_mul(5i32)), e88.wrapping_add((e89 as u32).wrapping_mul(61u32)).wrapping_add(2u32));
                                let e99 = pf(c, 18u32);
                                if (e96.0[0] < e99) {
                                    let e118 = pw(c, f32::sin(((e76 - e77) * 3.1415927f32)), 2.0f32);
                                    let e121 = ((e39 - ((Vn([(e54 as f32), (e58 as f32)]) + Vn([0.25f32; 2])) + (Vn([e67.0[1], e96.0[1]]) * Vn([0.5f32; 2])))) * Vn([4.0f32; 2]));
                                    let e122 = e121.length();
                                    let e128 = turn(c, e121, (0.35f32 * (e67.0[0] - 0.5f32)));
                                    let e130 = pf(c, 20u32);
                                    let e132 = f32::max(e130, 0.05f32);
                                    let e177 = pf3(c, 9u32);
                                    let e181 = hsv(c, e67.0[1], 0.75f32, 1.15f32);
                                    let e183 = pf(c, 21u32);
                                    let e184 = mix3(c, e177, e181, e183);
                                    let e185 = v0;
                                    v0 = (e185 + (((e184 * Vn([(((f32::exp((((-e122) * e122) * 120.0f32)) * 1.4f32) + (f32::exp(((-e122) * 12.0f32)) * 0.35f32)) + (((f32::exp(((-f32::abs(e128.0[0])) * 30.0f32)) * f32::exp((((-f32::abs(e128.0[1])) * 4.0f32) / e132))) + (f32::exp(((-f32::abs(e128.0[1])) * 30.0f32)) * f32::exp((((-f32::abs(e128.0[0])) * 4.0f32) / e132)))) * 0.75f32)); 3])) * Vn([e118; 3])) * Vn([(0.4f32 + (0.6f32 * e96.0[1])); 3])));
                                }
                            }
                            let e199 = v3;
                            v3 = e199.wrapping_add(1i32);
                        }
                    }
                    let e202 = v2;
                    v2 = e202.wrapping_add(1i32);
                }
            }
            let e205 = v1;
            v1 = e205.wrapping_add(1i32);
        }
        let e207 = v0;
        let e208 = glowing(c, e207);
        return e208;
    }

    pub fn k_spiral(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e1 = place(c, a0);
        let e3 = angle(c, e1);
        let e7 = pf(c, 18u32);
        let e18 = pf(c, 19u32);
        let e22 = pf(c, 15u32);
        let e23 = ((((e3 / 6.2831855f32) * f32::max(f32::floor((e7 + 0.5f32)), 1.0f32)) + (f32::ln(f32::max(e1.length(), 0.0001f32)) * e18)) - e22);
        let e27 = c.p.i[2];
        if (e27 == 1i32) {
            let e37 = dyed(c, (0.5f32 + (0.5f32 * f32::cos((e23 * 6.2831855f32)))));
            return e37;
        }
        let e38 = fr(c, e23);
        let e40 = pf(c, 20u32);
        let e42 = hsv(c, e38, e40, 1.0f32);
        let e44 = pf(c, 5u32);
        let e45 = contrast3(c, e42, e44);
        let e52 = pf(c, 4u32);
        let e53 = (e45.zip3(Vn([0.0f32; 3]), Vn([2.0f32; 3]), clampf) * Vn([e52; 3]));
        return Vn([e53.0[0], e53.0[1], e53.0[2], 1.0f32]);
    }

    pub fn k_spots(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let mut v1: f32 = 1.0f32;
        let mut v2: Vn<f32, 2> = Vn([0.0f32; 2]);
        let e1 = place(c, a0);
        let e3 = pf(c, 15u32);
        let e6 = (e1 + Vn([e3, 0.0f32]));
        let e8 = pf(c, 18u32);
        v0 = (e8 * 0.5f32);
        let e17 = c.p.i[2];
        if (e17 == 2i32) {
            let e22 = pf(c, 15u32);
            let e25 = c.p.seed;
            let e26 = cells(c, e6, 0.85f32, e22, e25);
            let e27 = v0;
            let e29 = pf(c, 19u32);
            v0 = (e27 * (1.0f32 - (e29 * e26.0[2])));
            v1 = e26.0[0];
        } else {
            v2 = e6;
            let e40 = c.p.i[2];
            if (e40 == 1i32) {
                let e47 = md(c, f32::floor(e6.0[1]), 2.0f32);
                v2 = Vn([(e6.0[0] + (0.5f32 * e47)), e6.0[1]]);
            }
            let e53 = v2;
            let e54 = e53.map(f32::floor);
            let e55 = v0;
            let e57 = pf(c, 19u32);
            let e64 = c.p.seed;
            let e65 = rnd(c, (e54.0[0] as i32), (e54.0[1] as i32), e64);
            v0 = (e55 * (1.0f32 - (e57 * e65)));
            let e70 = v2;
            v1 = ((e70 - e54) - Vn([0.5f32; 2])).length();
        }
        let e77 = pf(c, 20u32);
        let e80 = px(c);
        let e82 = pf(c, 2u32);
        let e86 = f32::max((e77 * 0.5f32), (e80 / f32::max(e82, 0.0001f32)));
        let e87 = v0;
        let e89 = v0;
        let e91 = v1;
        let e92 = ss(c, (e87 - e86), (e89 + e86), e91);
        let e95 = dyed(c, (1.0f32 - e92));
        return e95;
    }

    pub fn k_stars(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let mut v1: i32 = 0i32;
        let mut v2: f32 = 0.0f32;
        let e1 = pos(c, a0);
        let e3 = pf2(c, 0u32);
        let e6 = pf(c, 3u32);
        let e9 = pf(c, 15u32);
        let e13 = turn(c, (e1 - e3), ((-e6) - (e9 * 0.01f32)));
        'l0: loop {
            'b0: {
                let e19 = v1;
                if (e19 < 3i32) {
                } else {
                    break 'l0;
                }
                let e22 = v1;
                let e30 = pf(c, 2u32);
                let e34 = ((e13 * Vn([(14.0f32 + (17.0f32 * (e22 as f32))); 2])) / Vn([f32::max(e30, 0.001f32); 2]));
                let e35 = e34.map(f32::floor);
                let e42 = c.p.seed;
                let e43 = v1;
                let e48 = rnd2(c, (e35.0[0] as i32), (e35.0[1] as i32), e42.wrapping_add((e43 as u32).wrapping_mul(19u32)));
                let e55 = c.p.seed;
                let e56 = v1;
                let e63 = rnd2(c, (e35.0[0] as i32), (e35.0[1] as i32), e55.wrapping_add((e56 as u32).wrapping_mul(23u32)).wrapping_add(5u32));
                let e66 = pf(c, 18u32);
                if (e48.0[0] < e66) {
                    let e76 = pf(c, 2u32);
                    let e79 = ((((e34 - e35) - Vn([0.2f32; 2])) - (e63 * Vn([0.6f32; 2]))) * Vn([f32::max(e76, 0.001f32); 2]));
                    let e82 = pw(c, e48.0[1], 5.0f32);
                    let e84 = (e82 + 0.07f32);
                    let e87 = pf(c, 19u32);
                    let e90 = c.p.time;
                    let e114 = sq(c, (e79.length() / (0.03f32 + (0.05f32 * e84))));
                    v2 = (f32::exp((-e114)) * (0.35f32 + (e84 * 1.3f32)));
                    let e150 = v2;
                    let e152 = pf(c, 22u32);
                    v2 = (e150 + ((((f32::exp(((-f32::abs(e79.0[0])) * 60.0f32)) * f32::exp(((-f32::abs(e79.0[1])) * 6.0f32))) + (f32::exp(((-f32::abs(e79.0[1])) * 60.0f32)) * f32::exp(((-f32::abs(e79.0[0])) * 6.0f32)))) * step((1.0f32 - (e152 * 0.15f32)), e48.0[1])) * 0.8f32));
                    let e163 = v0;
                    let e173 = mix3(c, Vn([0.75f32, 0.85f32, 1.0f32]), Vn([1.0f32, 0.86f32, 0.7f32]), e63.0[1]);
                    let e174 = v2;
                    v0 = (e163 + ((e173 * Vn([e174; 3])) * Vn([(1.0f32 - (e87 * (0.5f32 + (0.5f32 * f32::sin(((e90 * (2.0f32 + (6.0f32 * e63.0[0]))) + (e48.0[1] * 60.0f32))))))); 3])));
                }
            }
            let e179 = v1;
            v1 = e179.wrapping_add(1i32);
        }
        let e182 = pf(c, 21u32);
        let e184 = turn(c, e13, (-e182));
        let e189 = (e184 * Vn([1.3f32; 2]));
        let e194 = c.p.seed;
        let e197 = noise3(c, Vn([e189.0[0], e189.0[1], 2.0f32]), e194.wrapping_add(60u32));
        let e201 = sq(c, ((e184.0[1] * 3.2f32) + (e197 * 0.6f32)));
        let e205 = (e184 * Vn([3.5f32; 2]));
        let e211 = c.p.seed;
        let e214 = fbm(c, Vn([e205.0[0], e205.0[1], 4.0f32]), 5i32, e211.wrapping_add(61u32));
        let e221 = (f32::exp((-e201)) * f32::max((0.35f32 + (e214 * 1.2f32)), 0.0f32));
        let e223 = pf3(c, 6u32);
        let e225 = pf3(c, 12u32);
        let e228 = pf(c, 20u32);
        let e234 = pf3(c, 9u32);
        let e235 = v0;
        let e239 = pf(c, 4u32);
        let e240 = (((e223 + (((e225 * Vn([e221; 3])) * Vn([e228; 3])) * Vn([0.5f32; 3]))) + (e234 * e235)) * Vn([e239; 3]));
        let e244 = c.p.i[1];
        if (e244 == 0i32) {
            return Vn([e240.0[0], e240.0[1], e240.0[2], 1.0f32]);
        }
        let e250 = pf3(c, 9u32);
        let e251 = v0;
        let e254 = pf3(c, 12u32);
        let e257 = pf(c, 20u32);
        let e262 = glowing(c, ((e250 * e251) + (((e254 * Vn([e221; 3])) * Vn([e257; 3])) * Vn([0.5f32; 3]))));
        return e262;
    }

    pub fn k_tiling(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 2> = Vn([0.0f32; 2]);
        let mut v1: Vn<f32, 2> = Vn([0.0f32; 2]);
        let e1 = place(c, a0);
        let e3 = pf(c, 18u32);
        v0 = Vn([f32::max(e3, 0.1f32), 1.0f32]);
        v1 = e1;
        let e13 = c.p.i[2];
        if (e13 == 2i32) {
            let e17 = turn(c, e1, 0.7853982f32);
            let e18 = e17.map(f32::floor);
            let e23 = md(c, (e18.0[0] + e18.0[1]), 4.0f32);
            let e24 = (e23 as i32);
            v1 = e17;
            if (e24 < 2i32) {
                let e52 = tile_done(c, Vn([((e17.0[0] - (e18.0[0] - ((e24 & 1i32) as f32))) - 1.0f32), ((e17.0[1] - e18.0[1]) - 0.5f32)]), Vn([2.0f32, 1.0f32]), (e18.0[0] as i32).wrapping_sub((e24 & 1i32)), (e18.0[1] as i32));
                return e52;
            }
            let e80 = tile_done(c, Vn([((e17.0[0] - e18.0[0]) - 0.5f32), ((e17.0[1] - (e18.0[1] - ((e24 & 1i32) as f32))) - 1.0f32)]), Vn([1.0f32, 2.0f32]), (e18.0[0] as i32).wrapping_add(400i32), (e18.0[1] as i32).wrapping_sub((e24 & 1i32)));
            return e80;
        }
        let e84 = c.p.i[2];
        if (e84 == 1i32) {
            let e89 = v0.0[0];
            let e95 = md(c, f32::floor(e1.0[1]), 2.0f32);
            v1 = Vn([(e1.0[0] + ((0.5f32 * e89) * e95)), e1.0[1]]);
        }
        let e100 = v1;
        let e101 = v0;
        let e103 = (e100 / e101).map(f32::floor);
        let e104 = v1;
        let e108 = v0;
        let e111 = v0;
        let e116 = tile_done(c, (e104 - ((e103 + Vn([0.5f32; 2])) * e108)), e111, (e103.0[0] as i32), (e103.0[1] as i32));
        return e116;
    }

    pub fn k_veins(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e1 = place(c, a0);
        let e3 = (e1 * Vn([0.7f32; 2]));
        let e5 = pf(c, 15u32);
        let e12 = c.p.seed;
        let e15 = fbm(c, Vn([e3.0[0], e3.0[1], (e5 * 0.1f32)]), 3i32, e12.wrapping_add(8u32));
        let e20 = (e1 + (Vn([e15, (-e15)]) * Vn([0.6f32; 2])));
        let e22 = pf(c, 15u32);
        let e29 = c.p.i[0];
        let e32 = c.p.seed;
        let e33 = ridges(c, Vn([e20.0[0], e20.0[1], (e22 * 0.15f32)]), e29, e32);
        let e35 = pf(c, 18u32);
        let e36 = pw(c, e33, e35);
        let e39 = dyed(c, (e36 * 1.1f32));
        return e39;
    }

    pub fn k_weave(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let mut v1: f32 = 0.0f32;
        let e1 = place(c, a0);
        let e2 = e1.map(f32::floor);
        let e6 = ((e1 - e2) - Vn([0.5f32; 2]));
        let e11 = md(c, (e2.0[0] + e2.0[1]), 2.0f32);
        v0 = e11;
        let e16 = c.p.i[2];
        if (e16 == 1i32) {
            let e23 = md(c, (e2.0[0] + e2.0[1]), 4.0f32);
            v0 = step(1.5f32, e23);
        } else {
            let e29 = c.p.i[2];
            if (e29 == 2i32) {
                let e42 = md(c, (f32::floor((e2.0[0] * 0.5f32)) + f32::floor((e2.0[1] * 0.5f32))), 2.0f32);
                v0 = e42;
            }
        }
        let e46 = pf(c, 18u32);
        let e47 = mix1(c, 0.36f32, 0.5f32, e46);
        let e52 = ss(c, e47, (e47 - 0.12f32), f32::abs(e6.0[1]));
        let e72 = ((e52 * (0.62f32 + (0.38f32 * f32::cos((e6.0[0] * 3.1415927f32))))) * f32::cos((clampf((e6.0[1] / f32::max(e47, 0.1f32)), -1.0f32, 1.0f32) * 1.2f32)));
        let e77 = ss(c, e47, (e47 - 0.12f32), f32::abs(e6.0[0]));
        let e97 = ((e77 * (0.62f32 + (0.38f32 * f32::cos((e6.0[1] * 3.1415927f32))))) * f32::cos((clampf((e6.0[0] / f32::max(e47, 0.1f32)), -1.0f32, 1.0f32) * 1.2f32)));
        let e99 = pf(c, 19u32);
        let e106 = c.p.seed;
        let e107 = rnd1(c, (e2.0[1] as i32), e106);
        let e112 = pf(c, 19u32);
        let e125 = c.p.seed;
        let e128 = noise3(c, Vn([(e1.0[0] * 0.6f32), (e2.0[1] * 3.1f32), 0.0f32]), e125.wrapping_add(2u32));
        let e130 = ((1.0f32 - ((e99 * 0.5f32) * e107)) - ((e112 * 0.2f32) * e128));
        let e132 = pf(c, 19u32);
        let e139 = c.p.seed;
        let e142 = rnd1(c, (e2.0[0] as i32), e139.wrapping_add(5u32));
        let e147 = pf(c, 19u32);
        let e160 = c.p.seed;
        let e163 = noise3(c, Vn([(e1.0[1] * 0.6f32), (e2.0[0] * 3.1f32), 5.0f32]), e160.wrapping_add(2u32));
        let e165 = ((0.82f32 - ((e132 * 0.5f32) * e142)) - ((e147 * 0.2f32) * e163));
        v1 = f32::max((e72 * e130), ((e97 * e165) * 0.55f32));
        let e172 = v0;
        if (e172 < 0.5f32) {
            v1 = f32::max((e97 * e165), ((e72 * e130) * 0.55f32));
        }
        let e180 = v1;
        let e181 = dyed(c, e180);
        return e181;
    }

    #[inline]
    pub fn line1(c: &C<'_>, a0: f32) -> f32 {
        let e3 = fr(c, (a0 + 0.5f32));
        return f32::abs((e3 - 0.5f32));
    }

    #[inline]
    pub fn place(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 2> {
        let e1 = pos(c, a0);
        let e3 = pf2(c, 0u32);
        let e6 = pf(c, 3u32);
        let e8 = turn(c, (e1 - e3), (-e6));
        let e10 = pf(c, 2u32);
        return (e8 / Vn([f32::max(e10, 0.0001f32); 2]));
    }

    #[inline]
    pub fn poly1(c: &C<'_>, a0: Vn<f32, 2>, a1: f32) -> f32 {
        let e6 = angle(c, Vn([(-a0.0[1]), a0.0[0]]));
        let e8 = (3.1415927f32 / a1);
        return ((a0.length() * f32::cos(((e6 - ((2.0f32 * e8) * f32::floor((e6 / (2.0f32 * e8))))) - e8))) / f32::cos(e8));
    }

    #[inline]
    pub fn relief1(c: &C<'_>, a0: Vn<f32, 2>) -> f32 {
        let e4 = c.p.i[2];
        if (e4 == 1i32) {
            let e8 = (a0 * Vn([1.2f32; 2]));
            let e14 = c.p.i[0];
            let e17 = c.p.seed;
            let e18 = ridges(c, Vn([e8.0[0], e8.0[1], 0.0f32]), e14, e17);
            return (e18 * 0.5f32);
        }
        let e24 = c.p.i[2];
        if (e24 == 2i32) {
            let e33 = c.p.seed;
            let e34 = cells(c, (a0 * Vn([1.6f32; 2])), 0.9f32, 0.0f32, e33);
            return (e34.0[0] * 0.5f32);
        }
        let e41 = c.p.i[2];
        if (e41 == 3i32) {
            let e45 = (a0 * Vn([5.0f32; 2]));
            let e51 = c.p.i[0];
            let e54 = c.p.seed;
            let e55 = fbm(c, Vn([e45.0[0], e45.0[1], 0.0f32]), e51, e54);
            let e59 = (a0 * Vn([1.1f32; 2]));
            let e64 = c.p.seed;
            let e67 = noise3(c, Vn([e59.0[0], e59.0[1], 4.0f32]), e64.wrapping_add(5u32));
            return ((e55 * 0.12f32) + (e67 * 0.05f32));
        }
        let e76 = c.p.i[0];
        let e79 = c.p.seed;
        let e80 = fbm(c, Vn([a0.0[0], a0.0[1], 0.0f32]), e76, e79);
        return (e80 * 0.5f32);
    }

    #[inline]
    pub fn seen(c: &C<'_>, a0: Vn<f32, 3>, a1: f32) -> Vn<f32, 4> {
        let e5 = c.p.i[1];
        if (e5 == 1i32) {
            return Vn([a0.0[0], a0.0[1], a0.0[2], clampf(f32::max(f32::max(a0.0[0], a0.0[1]), a0.0[2]), 0.0f32, 1.0f32)]);
        }
        let e20 = c.p.i[1];
        if (e20 == 2i32) {
            let e23 = (a0 * Vn([a1; 3]));
            return Vn([e23.0[0], e23.0[1], e23.0[2], a1]);
        }
        return Vn([a0.0[0], a0.0[1], a0.0[2], 1.0f32]);
    }

    #[inline]
    pub fn star1(c: &C<'_>, a0: Vn<f32, 2>, a1: f32, a2: f32) -> f32 {
        let e4 = (3.1415927f32 / a1);
        let e9 = angle(c, Vn([(-a0.0[1]), a0.0[0]]));
        let e20 = f32::abs((e9 - ((2.0f32 * e4) * f32::floor(((e9 / (2.0f32 * e4)) + 0.5f32)))));
        let e24 = (Vn([f32::cos(e4), f32::sin(e4)]) * Vn([a2; 2]));
        let e28 = (e24 - Vn([1.0f32, 0.0f32]));
        return (a0.length() / f32::max((e24.0[1] / f32::max(((f32::cos(e20) * e28.0[1]) - (f32::sin(e20) * e28.0[0])), 1e-5f32)), 0.0001f32));
    }

    #[inline]
    pub fn tile_done(c: &C<'_>, a0: Vn<f32, 2>, a1: Vn<f32, 2>, a2: i32, a3: i32) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let e16 = f32::min(((a1.0[0] * 0.5f32) - f32::abs(a0.0[0])), ((a1.0[1] * 0.5f32) - f32::abs(a0.0[1])));
        let e17 = px(c);
        let e19 = pf(c, 2u32);
        let e22 = (e17 / f32::max(e19, 0.0001f32));
        let e24 = pf(c, 19u32);
        let e26 = (e24 * 0.5f32);
        let e29 = ss(c, (e26 - e22), (e26 + e22), e16);
        let e31 = pf(c, 20u32);
        let e35 = ss(c, e26, (e26 + f32::max(e31, 0.001f32)), e16);
        v0 = (((-a0.0[1]) / a1.0[1]) - (a0.0[0] / a1.0[0]));
        let e47 = v0;
        let e51 = pf(c, 20u32);
        v0 = ((signf(e47) * (1.0f32 - e35)) * step(0.002f32, e51));
        let e57 = c.p.seed;
        let e58 = rnd(c, a2, a3, e57);
        let e62 = pf(c, 21u32);
        let e68 = v0;
        let e73 = dyed(c, (((0.72f32 + (((e58 - 0.5f32) * e62) * 0.6f32)) + (e68 * 0.25f32)) * e29));
        return e73;
    }

}

mod style {
    use super::shared::*;
    use super::*;
    #[inline]
    pub fn band1(c: &C<'_>, a0: f32) -> f32 {
        let e2 = pf(c, 0u32);
        let e5 = pf(c, 1u32);
        let e15 = pf(c, 2u32);
        let e21 = f32::max((0.5f32 - (e15 * 0.5f32)), 0.01f32);
        let e26 = ss(c, (0.5f32 - e21), (0.5f32 + e21), (0.5f32 + (0.5f32 * f32::cos((((a0 * e2) - e5) * 6.2831855f32)))));
        return e26;
    }

    #[inline]
    pub fn block_color(c: &C<'_>, a0: Vn<f32, 2>, a1: Vn<f32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 4> = Vn([0.0f32; 4]);
        let mut v1: i32 = 0i32;
        let mut v2: i32 = 0i32;
        'l0: loop {
            'b0: {
                let e7 = v1;
                if (e7 < 4i32) {
                } else {
                    break 'l0;
                }
                v2 = 0i32;
                'l1: loop {
                    'b1: {
                        let e12 = v2;
                        if (e12 < 4i32) {
                        } else {
                            break 'l1;
                        }
                        let e15 = v2;
                        let e17 = v1;
                        let e29 = v0;
                        let e33 = pic0(c, (a0 + ((((Vn([(e15 as f32), (e17 as f32)]) + Vn([0.5f32; 2])) / Vn([4.0f32; 2])) - Vn([0.5f32; 2])) * a1)), 1i32);
                        v0 = (e29 + e33);
                    }
                    let e36 = v2;
                    v2 = e36.wrapping_add(1i32);
                }
            }
            let e39 = v1;
            v1 = e39.wrapping_add(1i32);
        }
        let e41 = v0;
        return (e41 * Vn([0.0625f32; 4]));
    }

    #[inline]
    pub fn dct_w(c: &C<'_>, a0: f32, a1: f32) -> f32 {
        let mut v0: f32 = 0.5f32;
        if (a1 < 0.5f32) {
            v0 = 0.35355338f32;
        }
        let e17 = v0;
        return (f32::cos((((((2.0f32 * a0) + 1.0f32) * a1) * 3.1415927f32) / 16.0f32)) * e17);
    }

    #[inline]
    pub fn framed(c: &C<'_>, a0: Vn<f32, 4>, a1: Vn<f32, 2>, a2: f32, a3: f32) -> Vn<f32, 4> {
        let e5 = pf(c, 2u32);
        let e7 = (e5 * 0.5f32);
        let e10 = ss(c, (e7 - a3), (e7 + a3), a2);
        let e12 = pf(c, 2u32);
        let e21 = pf(c, 6u32);
        let e27 = ss(c, e7, (e7 + 0.12f32), a2);
        let e37 = pf(c, 6u32);
        let e47 = (Vn([a0.0[0], a0.0[1], a0.0[2]]) * Vn([f32::max((1.0f32 + (((((-a1.0[1]) - a1.0[0]) * e21) * 0.7f32) + ((((1.0f32 - e27) * signf(((-a1.0[1]) - a1.0[0]))) * e37) * 0.25f32))), 0.0f32); 3]));
        let e51 = pf3(c, 3u32);
        let e53 = back(c, e51, a0.0[3]);
        let e57 = mix4(c, e53, Vn([e47.0[0], e47.0[1], e47.0[2], a0.0[3]]), clampf((e10 + step(e12, 0.0f32)), 0.0f32, 1.0f32));
        return e57;
    }

    #[inline]
    pub fn grain1(c: &C<'_>, a0: Vn<f32, 2>, a1: u32) -> f32 {
        let e2 = a0.map(f32::floor);
        let e4 = (e2.0[0] as i32);
        let e6 = (e2.0[1] as i32);
        let e7 = rnd(c, e4, e6, a1);
        let e10 = rnd(c, e4, e6, a1.wrapping_add(7919u32));
        let e13 = ((e7 + e10) - 1.0f32);
        let e15 = pf(c, 1u32);
        if (e15 <= 1.0f32) {
            return e13;
        }
        let e18 = (a0 - e2);
        let e19 = rnd(c, e4, e6, a1);
        let e22 = rnd(c, e4, e6, a1.wrapping_add(7919u32));
        let e28 = rnd(c, e4.wrapping_add(1i32), e6, a1);
        let e33 = rnd(c, e4.wrapping_add(1i32), e6, a1.wrapping_add(7919u32));
        let e39 = rnd(c, e4, e6.wrapping_add(1i32), a1);
        let e44 = rnd(c, e4, e6.wrapping_add(1i32), a1.wrapping_add(7919u32));
        let e52 = rnd(c, e4.wrapping_add(1i32), e6.wrapping_add(1i32), a1);
        let e59 = rnd(c, e4.wrapping_add(1i32), e6.wrapping_add(1i32), a1.wrapping_add(7919u32));
        let e64 = fade(c, e18.0[0]);
        let e66 = fade(c, e18.0[1]);
        let e67 = Vn([e64, e66]);
        let e69 = mix1(c, ((e19 + e22) - 1.0f32), ((e28 + e33) - 1.0f32), e67.0[0]);
        let e71 = mix1(c, ((e39 + e44) - 1.0f32), ((e52 + e59) - 1.0f32), e67.0[0]);
        let e73 = mix1(c, e69, e71, e67.0[1]);
        let e77 = pf(c, 6u32);
        let e78 = mix1(c, e13, (e73 * 1.5f32), e77);
        return e78;
    }

    pub fn k_aerial(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let mut v1: Vn<f32, 4> = Vn([0.0f32; 4]);
        let mut v2: Vn<f32, 3> = Vn([0.0f32; 3]);
        let e1 = uv_of(c, a0);
        let e2 = full_pixel(c, a0);
        let e5 = c.p.seed;
        let e8 = c.p.time;
        let e20 = e5.wrapping_add((i32_max((f32::floor(((e8 * 30.0f32) + 0.001f32)) as i32), 0i32) as u32).wrapping_mul(11u32));
        let e30 = c.p.time;
        let e35 = c.p.seed;
        let e38 = wander(c, (e30 * 1.3f32), e35.wrapping_add(2u32));
        let e43 = ss(c, 0.35f32, 0.9f32, ((e38 * 0.5f32) + 0.5f32));
        let e44 = rnd1(c, (f32::floor((e1.0[1] * 480.0f32)) as i32), e20);
        let e50 = pf(c, 2u32);
        v0 = ((((e44 - 0.5f32) * 0.012f32) * e50) * (0.4f32 + (e43 * 1.6f32)));
        let e58 = v0;
        let e64 = c.p.time;
        let e70 = c.p.seed;
        let e73 = wander(c, ((e1.0[1] * 5.0f32) + (e64 * 7.0f32)), e70.wrapping_add(3u32));
        let e77 = pf(c, 2u32);
        v0 = (e58 + (((e73 * 0.012f32) * e77) * e43));
        let e84 = c.p.time;
        let e86 = pf(c, 1u32);
        let e89 = fr(c, (e1.0[1] + (e84 * e86)));
        let e91 = v0;
        let e93 = Vn([(e1.0[0] + e91), e89]);
        let e95 = pic0_uv(c, e93, 1i32);
        v1 = e95;
        let e102 = pic0_uv(c, (e93 - Vn([0.022f32, 0.0f32])), 1i32);
        let e103 = v1;
        let e105 = pf(c, 3u32);
        let e108 = mix4(c, e103, e102, (0.32f32 * e105));
        v1 = e108;
        let e110 = pf(c, 1u32);
        let e120 = v1;
        v2 = (Vn([e120.0[0], e120.0[1], e120.0[2]]) * Vn([step((0.02f32 * step(0.001f32, f32::abs(e110))), f32::min(e89, (1.0f32 - e89))); 3]));
        let e124 = v2;
        let e125 = luma(c, e124);
        let e126 = v2;
        let e128 = v2;
        let e131 = c.p.time;
        let e136 = c.p.seed;
        let e139 = wander(c, (e131 * 3.0f32), e136.wrapping_add(5u32));
        let e143 = pf(c, 6u32);
        let e145 = hue_turn(c, e128, ((e139 * 1.5f32) * e143));
        let e149 = pf(c, 6u32);
        let e158 = pf(c, 6u32);
        let e161 = mix3(c, e126, (Vn([e125; 3]) + ((e145 - Vn([e125; 3])) * Vn([(1.0f32 - ((0.7f32 * e149) * e43)); 3]))), f32::min(e158, 1.0f32));
        v2 = e161;
        let e170 = rnd(c, idiv(e2.0[0], 2i32), idiv(e2.0[1], 2i32), e20.wrapping_add(4u32));
        let e173 = v2;
        let e177 = pf(c, 0u32);
        let e185 = specks(c, 2.0f32);
        v2 = (e173 + Vn([(((((e170 - 0.5f32) * 0.55f32) * e177) * (0.5f32 + (e43 * 0.8f32))) * e185); 3]));
        let e189 = v2;
        let e195 = c.p.time;
        let e205 = pf(c, 4u32);
        v2 = (e189 * Vn([(1.0f32 + ((f32::sin((((e1.0[1] * 2.0f32) - (e195 * 0.8f32)) * 6.2831855f32)) * 0.16f32) * e205)); 3]));
        let e225 = c.p.h;
        let e229 = ss(c, 1.6f32, 3.2f32, ((e225 as f32) / 240.0f32));
        let e230 = mix1(c, 0.5f32, (0.5f32 + (0.5f32 * f32::cos(((e1.0[1] * 240.0f32) * 6.2831855f32)))), e229);
        let e231 = v2;
        let e233 = pf(c, 5u32);
        v2 = (e231 * Vn([(1.0f32 - ((e233 * 0.5f32) * e230)); 3]));
        let e240 = v2;
        let e243 = e240.zip(Vn([0.0f32; 3]), f32::max);
        let e245 = v1.0[3];
        return Vn([e243.0[0], e243.0[1], e243.0[2], e245]);
    }

    pub fn k_band(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = at1(c, a0.0[0], a0.0[1]);
        let e10 = at2(c, a0.0[0], a0.0[1]);
        let e14 = pf(c, 0u32);
        let e15 = ((Vn([e6.0[0], e6.0[1], e6.0[2]]) - Vn([e10.0[0], e10.0[1], e10.0[2]])) * Vn([e14; 3]));
        let e16 = luma(c, e15);
        let e19 = pf(c, 1u32);
        let e20 = mix3(c, Vn([e16; 3]), e15, e19);
        let e27 = pf(c, 2u32);
        let e28 = mix3(c, Vn([(0.5f32 * e3.0[3]); 3]), Vn([e3.0[0], e3.0[1], e3.0[2]]), e27);
        let e32 = (e28 + e20).zip(Vn([0.0f32; 3]), f32::max);
        return Vn([e32.0[0], e32.0[1], e32.0[2], e3.0[3]]);
    }

    pub fn k_cartoon(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let mut v1: Vn<f32, 3> = Vn([0.0f32; 3]);
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = at1(c, a0.0[0], a0.0[1]);
        let e7 = straight(c, e6);
        let e8 = luma(c, e7);
        let e10 = pf(c, 0u32);
        let e14 = f32::max((e10 - 1.0f32), 1.0f32);
        v0 = e8;
        let e19 = c.p.i[0];
        if (e19 == 0i32) {
            let e22 = (e8 * e14);
            let e23 = f32::floor(e22);
            let e25 = pf(c, 1u32);
            let e30 = (clampf(e25, 0.02f32, 1.0f32) * 0.5f32);
            let e36 = ss(c, (0.5f32 - e30), (0.5f32 + e30), (e22 - e23));
            v0 = ((e23 + e36) / e14);
        }
        let e39 = v0;
        let e45 = pf(c, 7u32);
        let e46 = saturate3(c, (e7 * Vn([(e39 / f32::max(e8, 0.02f32)); 3])), e45);
        v1 = e46;
        let e50 = at2(c, a0.0[0], a0.0[1]);
        let e53 = pf(c, 3u32);
        let e55 = pf(c, 3u32);
        let e58 = ss(c, e53, (e55 + 0.18f32), e50.0[0]);
        let e60 = pf(c, 2u32);
        let e62 = v1;
        let e64 = pf3(c, 4u32);
        let e68 = mix3(c, e62, e64, clampf((e58 * e60), 0.0f32, 1.0f32));
        v1 = e68;
        let e69 = v1;
        let e71 = back(c, e69, e3.0[3]);
        return e71;
    }

    pub fn k_dct_columns(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let mut v1: i32 = 0i32;
        let mut v2: Vn<f32, 3> = Vn([0.0f32; 3]);
        let mut v3: bool = false;
        let e5 = idiv(a0.0[1], 8i32).wrapping_mul(8i32);
        let e10 = idiv(a0.0[0], 8i32).wrapping_mul(8i32);
        let e13 = (a0.0[1].wrapping_sub(e5) as f32);
        let e16 = (a0.0[0].wrapping_sub(e10) as f32);
        'l0: loop {
            'b0: {
                let e22 = v1;
                if (e22 < 8i32) {
                } else {
                    break 'l0;
                }
                let e25 = v0;
                let e27 = v1;
                let e29 = at0(c, a0.0[0], e5.wrapping_add(e27));
                let e31 = v1;
                let e33 = dct_w(c, (e31 as f32), e13);
                v0 = (e25 + (Vn([e29.0[0], e29.0[1], e29.0[2]]) * Vn([e33; 3])));
            }
            let e37 = v1;
            v1 = e37.wrapping_add(1i32);
        }
        let e40 = pf(c, 0u32);
        let e43 = pf(c, 3u32);
        let e47 = (e40 * (1.0f32 + ((e16 + e13) * e43)));
        let e49 = pf(c, 1u32);
        let e52 = pf(c, 1u32);
        let e54 = Vn([e47, (e47 * e49), (e47 * e52)]);
        let e55 = v0;
        v2 = (((e55 / e54) + Vn([0.5f32; 3])).map(f32::floor) * e54);
        let e69 = c.p.seed;
        let e70 = rnd2(c, idiv(e10, 8i32), idiv(e5, 8i32), e69);
        let e73 = pf(c, 2u32);
        if (e70.0[0] < e73) {
            v3 = ((e16 + e13) < 2.5f32);
        } else {
            v3 = false;
        }
        let e81 = v3;
        if e81 {
            let e82 = v2;
            let e88 = c.p.seed;
            let e91 = rnd(c, e10, e5, e88.wrapping_add(1u32));
            let e96 = c.p.seed;
            let e99 = rnd(c, e10, e5, e96.wrapping_add(2u32));
            v2 = (e82 + ((Vn([(e70.0[1] - 0.5f32), (e91 - 0.5f32), (e99 - 0.5f32)]) * Vn([2.2f32; 3])) / Vn([((1.0f32 + e16) + e13); 3])));
        }
        let e111 = v2;
        let e114 = at0(c, a0.0[0], a0.0[1]);
        return Vn([e111.0[0], e111.0[1], e111.0[2], e114.0[3]]);
    }

    pub fn k_dct_rows(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let mut v1: i32 = 0i32;
        let e5 = idiv(a0.0[0], 8i32).wrapping_mul(8i32);
        'l0: loop {
            'b0: {
                let e14 = v1;
                if (e14 < 8i32) {
                } else {
                    break 'l0;
                }
                let e17 = v1;
                let e20 = at0(c, e5.wrapping_add(e17), a0.0[1]);
                let e22 = luma(c, Vn([e20.0[0], e20.0[1], e20.0[2]]));
                let e25 = luma(c, Vn([e20.0[0], e20.0[1], e20.0[2]]));
                let e29 = luma(c, Vn([e20.0[0], e20.0[1], e20.0[2]]));
                let e32 = v0;
                let e33 = v1;
                let e35 = dct_w(c, (e33 as f32), (a0.0[0].wrapping_sub(e5) as f32));
                v0 = (e32 + (Vn([e22, (e20.0[2] - e25), (e20.0[0] - e29)]) * Vn([e35; 3])));
            }
            let e39 = v1;
            v1 = e39.wrapping_add(1i32);
        }
        let e41 = v0;
        let e44 = at0(c, a0.0[0], a0.0[1]);
        return Vn([e41.0[0], e41.0[1], e41.0[2], e44.0[3]]);
    }

    pub fn k_emboss(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 4> = Vn([0.0f32; 4]);
        let mut v1: Vn<f32, 3> = Vn([0.0f32; 3]);
        let e1 = pos(c, a0);
        let e6 = at1(c, a0.0[0].wrapping_sub(1i32), a0.0[1]);
        let e8 = luma(c, Vn([e6.0[0], e6.0[1], e6.0[2]]));
        let e13 = at1(c, a0.0[0].wrapping_add(1i32), a0.0[1]);
        let e15 = luma(c, Vn([e13.0[0], e13.0[1], e13.0[2]]));
        let e20 = at1(c, a0.0[0], a0.0[1].wrapping_sub(1i32));
        let e22 = luma(c, Vn([e20.0[0], e20.0[1], e20.0[2]]));
        let e27 = at1(c, a0.0[0], a0.0[1].wrapping_add(1i32));
        let e29 = luma(c, Vn([e27.0[0], e27.0[1], e27.0[2]]));
        let e31 = pf(c, 2u32);
        let e36 = c.p.h;
        let e44 = (Vn([(e8 - e15), (e22 - e29)]) * Vn([(((e31 * 0.5f32) * (e36 as f32)) * 0.06f32); 2]));
        let e47 = Vn([e44.0[0], e44.0[1], 1.0f32]).normalize();
        let e49 = pf(c, 0u32);
        let e52 = pf(c, 0u32);
        let e55 = pf(c, 1u32);
        let e59 = Vn([f32::cos(e49), f32::sin(e52), f32::max(e55, 0.05f32)]).normalize();
        let e75 = pf(c, 5u32);
        let e76 = pw(c, f32::max(e47.dot((e59 + Vn([0.0f32, 0.0f32, 1.0f32])).normalize()), 0.0f32), e75);
        let e78 = pf(c, 4u32);
        let e82 = at0(c, a0.0[0], a0.0[1]);
        v0 = e82;
        let e87 = c.p.i[1];
        if (e87 == 2i32) {
            let e92 = pf(c, 9u32);
            let e93 = (Vn([e47.0[0], e47.0[1]]) * Vn([e92; 2]));
            let e95 = pf(c, 10u32);
            let e103 = c.p.i[0];
            let e104 = pic0(c, (e1 - (e93 * Vn([(1.0f32 + e95); 2]))), e103);
            let e109 = c.p.i[0];
            let e110 = pic0(c, (e1 - e93), e109);
            let e112 = pf(c, 10u32);
            let e120 = c.p.i[0];
            let e121 = pic0(c, (e1 - (e93 * Vn([(1.0f32 - e112); 2]))), e120);
            v0 = Vn([e104.0[0], e110.0[1], e121.0[2], f32::max(f32::max(e104.0[3], e110.0[3]), e121.0[3])]);
        }
        let e132 = v0.0[3];
        let e136 = v0;
        let e139 = pf(c, 3u32);
        let e140 = mix3(c, Vn([(0.5f32 * e132); 3]), Vn([e136.0[0], e136.0[1], e136.0[2]]), e139);
        v1 = e140;
        let e145 = c.p.i[1];
        if (e145 == 1i32) {
            let e155 = v0;
            let e157 = luma(c, Vn([e155.0[0], e155.0[1], e155.0[2]]));
            let e165 = (0.5f32 + (0.5f32 * f32::sin((((e47.0[0] * 5.0f32) + (e47.0[1] * 8.0f32)) + (e157 * 2.5f32)))));
            let e167 = v0.0[3];
            let e170 = v0;
            let e176 = pf(c, 3u32);
            let e177 = mix3(c, Vn([(e165 * e167); 3]), (Vn([e170.0[0], e170.0[1], e170.0[2]]) * Vn([(0.35f32 + e165); 3])), e176);
            v1 = e177;
        }
        let e178 = v1;
        let e183 = pf3(c, 6u32);
        let e185 = v0.0[3];
        let e188 = ((e178 * Vn([f32::max((e47.dot(e59) / f32::max(e59.0[2], 0.05f32)), 0.0f32); 3])) + (e183 * Vn([((e76 * e78) * e185); 3])));
        let e190 = v0.0[3];
        return Vn([e188.0[0], e188.0[1], e188.0[2], e190]);
    }

    pub fn k_film(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let mut v1: f32 = 0.0f32;
        let mut v2: i32 = 0i32;
        let e2 = pf(c, 9u32);
        let e7 = c.p.time;
        let e12 = (f32::floor(((e7 * f32::max(e2, 1.0f32)) + 0.001f32)) as i32);
        let e15 = c.p.seed;
        let e21 = e15.wrapping_add((i32_max(e12, 0i32) as u32).wrapping_mul(57u32));
        let e24 = c.p.time;
        let e29 = c.p.seed;
        let e32 = wander(c, (e24 * 2.6f32), e29.wrapping_add(1u32));
        let e35 = c.p.seed;
        let e38 = rnd1(c, e12, e35.wrapping_add(2u32));
        let e46 = c.p.time;
        let e51 = c.p.seed;
        let e54 = wander(c, (e46 * 3.4f32), e51.wrapping_add(3u32));
        let e59 = c.p.seed;
        let e62 = rnd1(c, e12, e59.wrapping_add(4u32));
        let e68 = pf(c, 3u32);
        let e70 = pos(c, a0);
        let e73 = pic0(c, (e70 + (Vn([(e32 + ((e38 - 0.5f32) * 0.8f32)), ((e54 * 0.7f32) + (e62 - 0.5f32))]) * Vn([e68; 2]))), 1i32);
        v0 = Vn([e73.0[0], e73.0[1], e73.0[2]]);
        let e76 = full_pixel(c, a0);
        let e77 = v0;
        let e80 = c.p.seed;
        let e83 = rnd1(c, e12, e80.wrapping_add(5u32));
        let e90 = c.p.time;
        let e95 = c.p.seed;
        let e98 = wander(c, (e90 * 5.0f32), e95.wrapping_add(6u32));
        let e103 = pf(c, 2u32);
        v0 = (e77 * Vn([(1.0f32 + ((((e83 - 0.5f32) * 0.28f32) + (e98 * 0.06f32)) * e103)); 3]));
        let e108 = v0;
        let e109 = luma(c, e108);
        let e110 = v0;
        let e125 = pf(c, 6u32);
        let e126 = mix3(c, e110, (((Vn([e109; 3]) * Vn([1.06f32, 0.98f32, 0.86f32])) * Vn([0.9f32; 3])) + Vn([0.06f32, 0.045f32, 0.03f32])), e125);
        v0 = e126;
        let e135 = rnd(c, idiv(e76.0[0], 2i32), idiv(e76.0[1], 2i32), e21.wrapping_add(7u32));
        let e141 = specks(c, 2.0f32);
        let e147 = rnd(c, e76.0[0], e76.0[1], e21.wrapping_add(8u32));
        let e153 = specks(c, 1.0f32);
        let e156 = v0;
        let e160 = pf(c, 4u32);
        v0 = (e156 + Vn([(((((((e135 - 0.5f32) * 0.6f32) * e141) + (((e147 - 0.5f32) * 0.4f32) * e153)) * 0.22f32) * e160) * (1.1f32 - (clampf(e109, 0.0f32, 1.0f32) * 0.6f32))); 3]));
        'l0: loop {
            'b0: {
                let e176 = v2;
                if (e176 < 4i32) {
                } else {
                    break 'l0;
                }
                let e179 = v2;
                let e184 = idiv(e12, 5i32.wrapping_add(e179.wrapping_mul(4i32)));
                let e185 = v2;
                let e188 = c.p.seed;
                let e191 = rnd2(c, e184, e185, e188.wrapping_add(11u32));
                let e194 = v2;
                let e198 = c.p.seed;
                let e201 = rnd1(c, e184.wrapping_mul(3i32).wrapping_add(e194), e198.wrapping_add(13u32));
                let e203 = pf(c, 0u32);
                if (e201 < (e203 * 0.65f32)) {
                    let e212 = c.p.shape;
                    let e214 = v2;
                    let e220 = c.p.seed;
                    let e223 = rnd1(c, e12.wrapping_add(e214.wrapping_mul(50i32)), e220.wrapping_add(14u32));
                    let e234 = c.p.seed;
                    let e235 = v2;
                    let e242 = wander(c, (e70.0[1] * 1.5f32), e234.wrapping_add((e235 as u32)).wrapping_add((i32_max(e184, 0i32) as u32)));
                    let e250 = (0.0005f32 + (0.0011f32 * e191.0[1]));
                    let e258 = v2;
                    let e263 = c.p.seed;
                    let e264 = noise3(c, Vn([(e70.0[1] * 3.0f32), (e184 as f32), (e258 as f32)]), e263);
                    let e269 = v1;
                    let e270 = px(c);
                    let e274 = ss(c, e250, (e250 + (e270 * 1.2f32)), f32::abs((e70.0[0] - ((((e191.0[0] - 0.5f32) * e212) + ((e223 - 0.5f32) * 0.006f32)) + (e242 * 0.002f32)))));
                    v1 = (e269 + (((1.0f32 - e274) * step(0.3f32, (e264 + 0.75f32))) * ((e191.0[1] * 1.6f32) - 0.45f32)));
                }
            }
            let e286 = v2;
            v2 = e286.wrapping_add(1i32);
        }
        let e288 = v0;
        let e289 = v1;
        v0 = (e288 + Vn([(e289 * 0.5f32); 3]));
        let e296 = (e70 * Vn([11.0f32; 2])).map(f32::floor);
        let e303 = rnd2(c, (e296.0[0] as i32), (e296.0[1] as i32), e21.wrapping_add(21u32));
        let e306 = pf(c, 1u32);
        if (e303.0[0] < (e306 * 0.07f32)) {
            let e316 = rnd2(c, (e296.0[0] as i32), (e296.0[1] as i32), e21.wrapping_add(22u32));
            let e333 = ((e70 - (((e296 + Vn([0.2f32; 2])) + (e316 * Vn([0.6f32; 2]))) / Vn([11.0f32; 2]))) / Vn([(0.0022f32 + (0.006f32 * e303.0[1])); 2]));
            let e336 = noise3(c, Vn([e333.0[0], e333.0[1], 1.0f32]), e21);
            let e339 = noise3(c, Vn([e333.0[0], e333.0[1], 5.0f32]), e21);
            let e348 = v0;
            let e357 = ss(c, 0.7f32, 1.1f32, (e333 * (Vn([1.0f32; 2]) + (Vn([0.5f32; 2]) * Vn([e336, e339])))).length());
            let e362 = mix3(c, e348, Vn([(step(0.6f32, e316.0[0]) * 0.9f32); 3]), ((1.0f32 - e357) * 0.85f32));
            v0 = e362;
        }
        let e365 = c.p.seed;
        let e368 = rnd1(c, e12, e365.wrapping_add(31u32));
        let e370 = pf(c, 7u32);
        if (e368 < (e370 * 0.5f32)) {
            let e377 = c.p.seed;
            let e380 = rnd2(c, e12, 3i32, e377.wrapping_add(32u32));
            let e386 = c.p.shape;
            let e400 = turn(c, (e70 - Vn([(((e380.0[0] - 0.5f32) * e386) * 0.9f32), ((e380.0[1] - 0.5f32) * 0.9f32)])), (e380.0[0] * 9.0f32));
            let e414 = px(c);
            let e420 = ss(c, 0.0009f32, (0.0009f32 + (e414 * 1.3f32)), f32::abs((e400.0[1] - (f32::sin(((e400.0[0] * 22.0f32) + (e380.0[1] * 9.0f32))) * 0.016f32))));
            let e427 = ss(c, 0.05f32, 0.06f32, f32::abs(e400.0[0]));
            let e431 = v0;
            v0 = (e431 * Vn([(1.0f32 - (((1.0f32 - e420) * (1.0f32 - e427)) * 0.85f32)); 3]));
        }
        let e440 = (e70 * Vn([1.6f32; 2]));
        let e450 = c.p.seed;
        let e453 = fbm(c, Vn([e440.0[0], e440.0[1], ((idiv(e12, 3i32) as f32) * 1.7f32)]), 3i32, e450.wrapping_add(41u32));
        let e454 = ss(c, 0.2f32, 0.6f32, e453);
        let e456 = pf(c, 5u32);
        let e458 = v0;
        let e459 = v0;
        let e470 = mix3(c, e458, ((e459 * Vn([1.12f32, 0.92f32, 0.7f32])) + Vn([0.05f32, 0.02f32, 0.0f32])), (e454 * e456));
        v0 = e470;
        let e474 = c.p.shape;
        let e481 = v0;
        let e483 = pf(c, 8u32);
        let e486 = ss(c, 0.28f32, 0.72f32, Vn([(e70.0[0] / f32::max(e474, 1.0f32)), e70.0[1]]).length());
        v0 = (e481 * Vn([(1.0f32 - (e483 * e486)); 3]));
        let e491 = v0;
        let e494 = e491.zip(Vn([0.0f32; 3]), f32::max);
        return Vn([e494.0[0], e494.0[1], e494.0[2], e73.0[3]]);
    }

    pub fn k_glitch(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 2> = Vn([0.0f32; 2]);
        let mut v1: Vn<f32, 4> = Vn([0.0f32; 4]);
        let e1 = pos(c, a0);
        let e4 = c.p.time;
        let e6 = pf(c, 2u32);
        let e17 = c.p.seed;
        let e20 = e17.wrapping_add((i32_max((f32::floor(((e4 * e6) + 0.001f32)) as i32), 0i32) as u32).wrapping_mul(911u32));
        let e22 = pf(c, 1u32);
        let e24 = f32::max(e22, 0.001f32);
        let e26 = pf(c, 0u32);
        let e28 = rnd1(c, 7i32, e20);
        let e33 = (e26 * (0.45f32 + (0.55f32 * e28)));
        let e40 = rnd1(c, 3i32, e20.wrapping_add(1u32));
        let e48 = (f32::floor((e1.0[1] / ((e24 * 0.5f32) * (0.4f32 + (1.6f32 * e40))))) as i32);
        let e52 = rnd2(c, e48, 11i32, e20.wrapping_add(2u32));
        let e60 = pf(c, 4u32);
        if (e52.0[0] < ((e33 * 0.4f32) * e60)) {
            v0 = Vn([(((e52.0[1] - 0.5f32) * 0.55f32) * e33), 0.0f32]);
        }
        let e73 = Vn([(e24 * 2.0f32), e24]);
        let e75 = (e1 / e73).map(f32::floor);
        let e82 = rnd2(c, (e75.0[0] as i32), (e75.0[1] as i32), e20.wrapping_add(3u32));
        let e89 = rnd(c, (e75.0[0] as i32), (e75.0[1] as i32), e20.wrapping_add(4u32));
        let e93 = pf(c, 5u32);
        if (e89 < ((e33 * 0.22f32) * e93)) {
            let e96 = v0;
            v0 = (e96 + ((((e82 - Vn([0.5f32; 2])) * Vn([5.0f32; 2])).map(f32::floor) * e73) * Vn([1.0f32, 0.5f32])));
        }
        let e110 = pf(c, 3u32);
        let e116 = rnd1(c, e48, e20.wrapping_add(5u32));
        let e119 = (((e110 * e33) * 0.03f32) * (0.4f32 + e116));
        let e120 = v0;
        let e126 = pic0(c, ((e1 + e120) + Vn([e119, 0.0f32])), 3i32);
        let e127 = v0;
        let e130 = pic0(c, (e1 + e127), 3i32);
        let e131 = v0;
        let e137 = pic0(c, ((e1 + e131) - Vn([e119, 0.0f32])), 3i32);
        v1 = Vn([e126.0[0], e130.0[1], e137.0[2], f32::max(f32::max(e126.0[3], e130.0[3]), e137.0[3])]);
        let e151 = pf(c, 6u32);
        if (e89 > (1.0f32 - ((e33 * 0.1f32) * e151))) {
            let e159 = f32::floor((e82.0[0] * 4.0f32));
            if (e159 < 1.0f32) {
                let e163 = v1.0[1];
                let e165 = v1.0[0];
                let e167 = v1.0[2];
                let e169 = v1.0[3];
                v1 = Vn([e163, e165, e167, e169]);
            } else {
                if (e159 < 2.0f32) {
                    let e173 = v1;
                    let e179 = (Vn([e173.0[0], e173.0[1], e173.0[2]]) * Vn([0.1f32, 1.2f32, 0.9f32]));
                    let e181 = v1.0[3];
                    v1 = Vn([e179.0[0], e179.0[1], e179.0[2], e181]);
                } else {
                    if (e159 < 3.0f32) {
                        let e185 = v1;
                        let e195 = (((Vn([e185.0[0], e185.0[1], e185.0[2]]) * Vn([3.0f32; 3])) + Vn([0.5f32; 3])).map(f32::floor) / Vn([3.0f32; 3]));
                        let e197 = v1.0[3];
                        v1 = Vn([e195.0[0], e195.0[1], e195.0[2], e197]);
                    } else {
                        let e200 = v1.0[3];
                        let e202 = v1;
                        let e207 = (Vn([e200; 3]) - Vn([e202.0[0], e202.0[1], e202.0[2]])).zip(Vn([0.0f32; 3]), f32::max);
                        let e209 = v1.0[3];
                        v1 = Vn([e207.0[0], e207.0[1], e207.0[2], e209]);
                    }
                }
            }
        }
        let e211 = v1;
        return e211;
    }

    pub fn k_grain(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: u32 = 0u32;
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = full_pixel(c, a0);
        let e7 = c.p.seed;
        v0 = e7;
        let e10 = pf(c, 7u32);
        if (e10 > 0.0f32) {
            let e13 = v0;
            let e16 = c.p.time;
            let e18 = pf(c, 7u32);
            v0 = e13.wrapping_add((i32_max((f32::floor(((e16 * e18) + 0.001f32)) as i32), 0i32) as u32).wrapping_mul(977u32));
        }
        let e39 = pf(c, 1u32);
        let e43 = ((Vn([(e4.0[0] as f32), (e4.0[1] as f32)]) + Vn([0.01f32; 2])) / Vn([f32::max(e39, 0.5f32); 2]));
        let e44 = straight(c, e3);
        let e45 = luma(c, e44);
        let e47 = pf(c, 3u32);
        let e49 = pf(c, 4u32);
        let e52 = ss(c, 0.0f32, 0.5f32, e45);
        let e53 = mix1(c, e47, e49, e52);
        let e55 = pf(c, 5u32);
        let e58 = ss(c, 0.5f32, 1.0f32, e45);
        let e59 = mix1(c, e53, e55, e58);
        let e61 = pf(c, 0u32);
        let e64 = pf(c, 1u32);
        let e67 = specks(c, f32::max(e64, 1.0f32));
        let e69 = v0;
        let e70 = grain1(c, e43, e69);
        let e75 = v0;
        let e78 = grain1(c, (e43 + Vn([71.0f32, 3.0f32])), e75.wrapping_add(1u32));
        let e83 = v0;
        let e86 = grain1(c, (e43 + Vn([13.0f32, 57.0f32])), e83.wrapping_add(2u32));
        let e90 = pf(c, 2u32);
        let e91 = mix3(c, Vn([e70; 3]), Vn([e78, e70, e86]), e90);
        let e99 = (Vn([e3.0[0], e3.0[1], e3.0[2]]) + (e91 * Vn([(((e59 * e61) * e67) * e3.0[3]); 3]))).zip(Vn([0.0f32; 3]), f32::max);
        return Vn([e99.0[0], e99.0[1], e99.0[2], e3.0[3]]);
    }

    pub fn k_halftone(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = pos(c, a0);
        let e8 = c.p.i[0];
        if (e8 == 0i32) {
            let e11 = straight(c, e3);
            let e13 = tone(c, e11, 0u32);
            let e15 = pf3(c, 3u32);
            let e17 = pf3(c, 6u32);
            let e19 = pf(c, 1u32);
            let e20 = screen1(c, e4, e19, e13);
            let e21 = mix3(c, e15, e17, e20);
            let e23 = back(c, e21, e3.0[3]);
            return e23;
        }
        let e24 = straight(c, e3);
        let e29 = pf(c, 9u32);
        let e38 = (((e24 - Vn([0.5f32; 3])) * Vn([e29; 3])) + Vn([0.5f32; 3])).zip3(Vn([0.0f32; 3]), Vn([1.0f32; 3]), clampf);
        let e40 = pf(c, 1u32);
        let e44 = screen1(c, e4, (e40 + 0.2618f32), e38.0[0]);
        let e46 = pf(c, 1u32);
        let e50 = screen1(c, e4, (e46 + 1.309f32), e38.0[1]);
        let e52 = pf(c, 1u32);
        let e54 = screen1(c, e4, e52, e38.0[2]);
        let e58 = c.p.i[0];
        if (e58 == 1i32) {
            let e63 = back(c, Vn([e44, e50, e54]), e3.0[3]);
            return e63;
        }
        let e72 = pf(c, 1u32);
        let e79 = screen1(c, e4, (e72 + 0.7854f32), (1.0f32 - ((1.0f32 - f32::max(f32::max(e38.0[0], e38.0[1]), e38.0[2])) * 0.85f32)));
        let e81 = pf3(c, 6u32);
        let e84 = mix1(c, 0.05f32, 1.0f32, e44);
        let e87 = mix1(c, 0.08f32, 1.0f32, e50);
        let e90 = mix1(c, 0.12f32, 1.0f32, e54);
        let e94 = pf3(c, 3u32);
        let e95 = mix3(c, e94, (e81 * Vn([e84, e87, e90])), e79);
        let e97 = back(c, e95, e3.0[3]);
        return e97;
    }

    pub fn k_hatch(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let mut v1: i32 = 0i32;
        let mut v2: f32 = 0.0f32;
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = straight(c, e3);
        let e5 = luma(c, e4);
        let e8 = clampf(e5, 0.0f32, 1.0f32);
        let e9 = pos(c, a0);
        let e11 = pf(c, 0u32);
        let e13 = f32::max(e11, 0.0001f32);
        let e15 = (e9 * Vn([6.0f32; 2]));
        let e20 = c.p.seed;
        let e21 = noise3(c, Vn([e15.0[0], e15.0[1], 0.0f32]), e20);
        let e23 = (e9 * Vn([6.0f32; 2]));
        let e28 = c.p.seed;
        let e31 = noise3(c, Vn([e23.0[0], e23.0[1], 3.0f32]), e28.wrapping_add(1u32));
        let e34 = pf(c, 10u32);
        let e39 = px(c);
        let e40 = (e39 / e13);
        'l0: loop {
            'b0: {
                let e45 = v1;
                let e49 = c.p.i[0];
                if (e45 < e49) {
                } else {
                    break 'l0;
                }
                let e53 = pf(c, 2u32);
                let e55 = v1;
                let e62 = c.p.i[0];
                let e66 = turn(c, (e9 + (((Vn([e21, e31]) * Vn([e34; 2])) * Vn([e13; 2])) * Vn([0.7f32; 2]))), ((-e53) - (((e55 as f32) * 3.1415927f32) / (e62 as f32))));
                let e69 = fr(c, (e66.0[1] / e13));
                let e78 = pf(c, 1u32);
                v2 = (((1.0f32 - e8) * e78) * 1.15f32);
                let e86 = c.p.i[1];
                if (e86 == 0i32) {
                    let e89 = v1;
                    let e96 = c.p.i[0];
                    let e100 = (1.0f32 - (((e89 as f32) + 0.6f32) / (e96 as f32)));
                    let e105 = ss(c, (e100 + 0.08f32), (e100 - 0.22f32), e8);
                    let e107 = pf(c, 1u32);
                    v2 = (e105 * e107);
                }
                let e109 = v2;
                let e112 = v2;
                let e114 = v2;
                let e116 = ss(c, (e112 - e40), (e114 + e40), (f32::abs((e69 - 0.5f32)) * 2.0f32));
                let e123 = c.p.h;
                let e126 = ss(c, 1.2f32, 2.4f32, (e13 * (e123 as f32)));
                let e127 = mix1(c, f32::min(e109, 1.0f32), (1.0f32 - e116), e126);
                let e128 = v0;
                let e131 = v2;
                v0 = (1.0f32 - ((1.0f32 - e128) * (1.0f32 - (e127 * step(0.015f32, e131)))));
            }
            let e141 = v1;
            v1 = e141.wrapping_add(1i32);
        }
        let e145 = c.p.scale;
        let e148 = edge_gap(c, (e145 * 1.5f32));
        let e149 = sobel0(c, a0, e148);
        let e151 = v0;
        let e155 = pf(c, 11u32);
        let e157 = ss(c, 0.12f32, 0.4f32, (e149.length() * e155));
        let e160 = c.p.scale;
        let e163 = edge_thin(c, (e160 * 1.5f32));
        v0 = f32::max(e151, (e157 * e163));
        let e167 = pf3(c, 3u32);
        let e171 = pf(c, 9u32);
        let e172 = mix3(c, e167, (e4 * Vn([0.55f32; 3])), e171);
        let e174 = pf3(c, 6u32);
        let e175 = v0;
        let e179 = mix3(c, e174, e172, clampf(e175, 0.0f32, 1.0f32));
        let e181 = back(c, e179, e3.0[3]);
        return e181;
    }

    pub fn k_idct_columns(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let mut v1: i32 = 0i32;
        let e5 = idiv(a0.0[1], 8i32).wrapping_mul(8i32);
        'l0: loop {
            'b0: {
                let e14 = v1;
                if (e14 < 8i32) {
                } else {
                    break 'l0;
                }
                let e17 = v0;
                let e19 = v1;
                let e21 = at0(c, a0.0[0], e5.wrapping_add(e19));
                let e23 = v1;
                let e25 = dct_w(c, (a0.0[1].wrapping_sub(e5) as f32), (e23 as f32));
                v0 = (e17 + (Vn([e21.0[0], e21.0[1], e21.0[2]]) * Vn([e25; 3])));
            }
            let e29 = v1;
            v1 = e29.wrapping_add(1i32);
        }
        let e31 = v0;
        let e34 = at0(c, a0.0[0], a0.0[1]);
        return Vn([e31.0[0], e31.0[1], e31.0[2], e34.0[3]]);
    }

    pub fn k_idct_rows(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let mut v1: i32 = 0i32;
        let e5 = idiv(a0.0[0], 8i32).wrapping_mul(8i32);
        'l0: loop {
            'b0: {
                let e14 = v1;
                if (e14 < 8i32) {
                } else {
                    break 'l0;
                }
                let e17 = v0;
                let e18 = v1;
                let e21 = at0(c, e5.wrapping_add(e18), a0.0[1]);
                let e23 = v1;
                let e25 = dct_w(c, (a0.0[0].wrapping_sub(e5) as f32), (e23 as f32));
                v0 = (e17 + (Vn([e21.0[0], e21.0[1], e21.0[2]]) * Vn([e25; 3])));
            }
            let e29 = v1;
            v1 = e29.wrapping_add(1i32);
        }
        let e32 = v0.0[0];
        let e34 = v0.0[2];
        let e35 = (e32 + e34);
        let e37 = v0.0[0];
        let e39 = v0.0[1];
        let e40 = (e37 + e39);
        let e42 = v0.0[0];
        let e54 = Vn([e35, (((e42 - (0.2126f32 * e35)) - (0.0722f32 * e40)) / 0.7152f32), e40]).zip(Vn([0.0f32; 3]), f32::max);
        let e57 = at0(c, a0.0[0], a0.0[1]);
        return Vn([e54.0[0], e54.0[1], e54.0[2], e57.0[3]]);
    }

    pub fn k_impasto(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e8 = at1(c, a0.0[0].wrapping_add(1i32), a0.0[1]);
        let e10 = luma(c, Vn([e8.0[0], e8.0[1], e8.0[2]]));
        let e15 = at1(c, a0.0[0].wrapping_sub(1i32), a0.0[1]);
        let e17 = luma(c, Vn([e15.0[0], e15.0[1], e15.0[2]]));
        let e23 = at1(c, a0.0[0], a0.0[1].wrapping_add(1i32));
        let e25 = luma(c, Vn([e23.0[0], e23.0[1], e23.0[2]]));
        let e30 = at1(c, a0.0[0], a0.0[1].wrapping_sub(1i32));
        let e32 = luma(c, Vn([e30.0[0], e30.0[1], e30.0[2]]));
        let e36 = angle(c, Vn([(-(e25 - e32)), (e10 - e17)]));
        let e37 = pos(c, a0);
        let e39 = pf(c, 0u32);
        let e43 = (1.0f32 / f32::max(e39, 0.0001f32));
        let e45 = turn(c, e37, (-e36));
        let e58 = c.p.seed;
        let e59 = noise3(c, Vn([((e45.0[0] * e43) * 0.15f32), (e45.0[1] * e43), 0.0f32]), e58);
        let e71 = c.p.seed;
        let e72 = noise3(c, Vn([((e45.0[0] * e43) * 0.15f32), ((e45.0[1] + (0.35f32 / e43)) * e43), 0.0f32]), e71);
        let e75 = pf(c, 2u32);
        let e93 = pf(c, 0u32);
        let e96 = c.p.h;
        let e99 = ss(c, 1.0f32, 3.0f32, (e93 * (e96 as f32)));
        let e101 = pf(c, 1u32);
        let e106 = pf(c, 3u32);
        let e117 = (Vn([e3.0[0], e3.0[1], e3.0[2]]) * Vn([f32::max((1.0f32 + ((((((e72 - e59) * f32::cos((e36 - e75))) * e101) * 0.9f32) + (((f32::sin(((e37.0[0] * e43) * 1.6f32)) * f32::sin(((e37.0[1] * e43) * 1.6f32))) * e106) * 0.12f32)) * e99)), 0.0f32); 3]));
        return Vn([e117.0[0], e117.0[1], e117.0[2], e3.0[3]]);
    }

    pub fn k_ink(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = at2(c, a0.0[0], a0.0[1]);
        let e8 = luma(c, Vn([e6.0[0], e6.0[1], e6.0[2]]));
        let e11 = at1(c, a0.0[0], a0.0[1]);
        let e13 = luma(c, Vn([e11.0[0], e11.0[1], e11.0[2]]));
        let e16 = pf(c, 0u32);
        let e19 = pf(c, 1u32);
        let e21 = pf(c, 1u32);
        let e23 = pf(c, 2u32);
        let e27 = ss(c, e19, (e21 + f32::max(e23, 0.005f32)), ((e8 - e13) * e16));
        v0 = e27;
        let e33 = at1(c, a0.0[0], a0.0[1]);
        let e35 = luma(c, Vn([e33.0[0], e33.0[1], e33.0[2]]));
        let e36 = ss(c, 0.12f32, 0.02f32, e35);
        let e39 = v0;
        let e41 = pf(c, 0u32);
        v0 = f32::max(e39, ((e36 * 0.8f32) * step(0.001f32, e41)));
        let e46 = full_pixel(c, a0);
        let e51 = c.p.seed;
        let e52 = rnd(c, e46.0[0], e46.0[1], e51);
        let e56 = pf(c, 10u32);
        let e63 = pf3(c, 6u32);
        let e64 = straight(c, e3);
        let e66 = pf(c, 9u32);
        let e67 = mix3(c, e63, e64, e66);
        let e70 = pf3(c, 3u32);
        let e71 = v0;
        let e72 = mix3(c, (e67 * Vn([(1.0f32 + (((e52 - 0.5f32) * e56) * 0.3f32)); 3])), e70, e71);
        let e74 = back(c, e72, e3.0[3]);
        return e74;
    }

    pub fn k_lamps(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 4> = Vn([0.0f32; 4]);
        let mut v1: i32 = 0i32;
        let mut v2: i32 = 0i32;
        let mut v3: Vn<f32, 3> = Vn([0.0f32; 3]);
        let e1 = pos(c, a0);
        let e3 = pf(c, 0u32);
        let e4 = px(c);
        let e7 = f32::max(e3, (e4 * 2.0f32));
        let e13 = ((e1 / Vn([e7; 2])).map(f32::floor) + Vn([0.5f32; 2]));
        'l0: loop {
            'b0: {
                let e22 = v1;
                if (e22 < 3i32) {
                } else {
                    break 'l0;
                }
                v2 = 0i32;
                'l1: loop {
                    'b1: {
                        let e27 = v2;
                        if (e27 < 3i32) {
                        } else {
                            break 'l1;
                        }
                        let e30 = v0;
                        let e31 = v2;
                        let e33 = v1;
                        let e44 = pic0(c, ((e13 + ((Vn([(e31 as f32), (e33 as f32)]) - Vn([1.0f32; 2])) * Vn([0.28f32; 2]))) * Vn([e7; 2])), 1i32);
                        v0 = (e30 + e44);
                    }
                    let e47 = v2;
                    v2 = e47.wrapping_add(1i32);
                }
            }
            let e50 = v1;
            v1 = e50.wrapping_add(1i32);
        }
        let e52 = v0;
        v0 = (e52 * Vn([0.11111111f32; 4]));
        let e55 = v0;
        v3 = Vn([e55.0[0], e55.0[1], e55.0[2]]);
        let e59 = pf(c, 6u32);
        if (e59 > 1.0f32) {
            let e62 = v3;
            let e64 = pf(c, 6u32);
            let e71 = pf(c, 6u32);
            v3 = (((e62 * Vn([e64; 3])) + Vn([0.5f32; 3])).map(f32::floor) / Vn([e71; 3]));
        }
        let e76 = (((e1 / Vn([e7; 2])) - e13).length() * 2.0f32);
        let e77 = px(c);
        let e80 = ((e77 / e7) * 2.0f32);
        let e82 = pf(c, 1u32);
        let e85 = pf(c, 1u32);
        let e87 = ss(c, (e82 - e80), (e85 + e80), e76);
        let e89 = (1.0f32 - e87);
        let e96 = pf(c, 2u32);
        let e98 = v3;
        let e107 = pf3(c, 3u32);
        let e109 = v0.0[3];
        let e114 = (((e107 * Vn([e109; 3])) * Vn([(1.0f32 - e89); 3])) + (e98 * Vn([((e89 * (1.15f32 - (0.4f32 * e76))) + (f32::exp((((-e76) * e76) * 2.5f32)) * e96)); 3])));
        let e116 = v0.0[3];
        return Vn([e114.0[0], e114.0[1], e114.0[2], e116]);
    }

    pub fn k_misprint(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e1 = pos(c, a0);
        let e3 = pf2(c, 0u32);
        let e6 = pic0(c, (e1 + e3), 1i32);
        let e8 = pf2(c, 2u32);
        let e11 = pic0(c, (e1 + e8), 1i32);
        let e13 = pf2(c, 4u32);
        let e16 = pic0(c, (e1 + e13), 1i32);
        let e17 = full_pixel(c, a0);
        let e19 = (e1 * Vn([90.0f32; 2]));
        let e24 = c.p.seed;
        let e25 = noise3(c, Vn([e19.0[0], e19.0[1], 0.0f32]), e24);
        let e32 = c.p.seed;
        let e33 = rnd(c, e17.0[0], e17.0[1], e32);
        let e39 = specks(c, 1.0f32);
        let e53 = pf(c, 6u32);
        let e58 = mix3(c, Vn([e6.0[0], e11.0[1], e16.0[2]]), Vn([1.0f32; 3]), clampf((((((e25 * 0.5f32) + (((e33 - 0.5f32) * 0.5f32) * e39)) * 0.5f32) + 0.12f32) * e53), 0.0f32, 1.0f32));
        let e60 = pf3(c, 7u32);
        let e65 = ((e58 * e60) * Vn([f32::max(e6.0[3], 1e-6f32); 3]));
        return Vn([e65.0[0], e65.0[1], e65.0[2], f32::max(f32::max(e6.0[3], e11.0[3]), e16.0[3])]);
    }

    pub fn k_mosaic(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 2> = Vn([0.0f32; 2]);
        let mut v1: Vn<f32, 2> = Vn([0.0f32; 2]);
        let mut v2: f32 = 64.0f32;
        let mut v3: i32 = (-1i32);
        let mut v4: i32 = 0i32;
        let mut v5: Vn<f32, 2> = Vn([0.0f32; 2]);
        let mut v6: f32 = 0.0f32;
        let mut v7: Vn<f32, 2> = Vn([0.0f32; 2]);
        let e1 = pos(c, a0);
        let e3 = pf(c, 0u32);
        let e5 = pf(c, 1u32);
        let e7 = px(c);
        let e12 = pf(c, 0u32);
        let e13 = px(c);
        let e17 = Vn([f32::max((e3 * e5), (e7 * 1.5f32)), f32::max(e12, (e13 * 1.5f32))]);
        let e18 = px(c);
        let e20 = (e18 / e17.0[1]);
        let e24 = c.p.i[1];
        if (e24 == 4i32) {
            let e29 = Vn([1.0f32, 1.7320508f32]);
            let e30 = (e1 / e17);
            let e37 = (e30 - (((e30 / e29).map(f32::floor) + Vn([0.5f32; 2])) * e29));
            let e47 = (e30 - ((((e30 / e29) - Vn([0.5f32; 2])).map(f32::floor) + Vn([1.0f32; 2])) * e29));
            v0 = e37;
            if (e47.dot(e47) < e37.dot(e37)) {
                v0 = e47;
            }
            let e52 = v0;
            let e53 = e52.map(f32::abs);
            let e65 = v0;
            let e70 = block_color(c, ((e30 - e65) * e17), (e17 * Vn([0.7f32; 2])));
            let e71 = v0;
            let e72 = framed(c, e70, e71, (0.5f32 - f32::max(e53.0[0], ((e53.0[0] * 0.5f32) + (e53.0[1] * 0.8660254f32)))), e20);
            return e72;
        }
        let e76 = c.p.i[1];
        if (e76 == 3i32) {
            let e79 = (e1 / e17);
            let e84 = c.p.seed;
            let e85 = cells(c, e79, 0.75f32, 0.0f32, e84);
            let e89 = (e79 - Vn([0.5f32; 2])).map(f32::floor);
            'l0: loop {
                'b0: {
                    let e97 = v3;
                    if (e97 <= 2i32) {
                    } else {
                        break 'l0;
                    }
                    v4 = (-1i32);
                    'l1: loop {
                        'b1: {
                            let e102 = v4;
                            if (e102 <= 2i32) {
                            } else {
                                break 'l1;
                            }
                            let e107 = v4;
                            let e108 = (e89.0[0] as i32).wrapping_add(e107);
                            let e111 = v3;
                            let e112 = (e89.0[1] as i32).wrapping_add(e111);
                            let e115 = c.p.seed;
                            let e116 = hash2(c, e108, e112, e115);
                            let e117 = unit(c, e116);
                            let e119 = (e117 * 6.2831855f32);
                            let e122 = hash(c, e116.wrapping_add(625341585u32));
                            let e123 = unit(c, e122);
                            let e126 = hash(c, e116.wrapping_add(1821285621u32));
                            let e127 = unit(c, e126);
                            let e151 = (Vn([((e108 as f32) + 0.5f32), ((e112 as f32) + 0.5f32)]) + ((((Vn([e123, e127]) - Vn([0.5f32; 2])) * Vn([0.72f32; 2])) + (Vn([f32::cos(e119), f32::sin((e119 * 1.7f32))]) * Vn([0.18f32; 2]))) * Vn([0.75f32; 2])));
                            let e153 = (e151 - e79).length();
                            let e154 = v2;
                            if (e153 < e154) {
                                v2 = e153;
                                v1 = e151;
                            }
                        }
                        let e157 = v4;
                        v4 = e157.wrapping_add(1i32);
                    }
                }
                let e160 = v3;
                v3 = e160.wrapping_add(1i32);
            }
            let e162 = v1;
            let e166 = block_color(c, (e162 * e17), (e17 * Vn([0.5f32; 2])));
            let e167 = v1;
            let e174 = framed(c, e166, (e79 - e167), ((e85.0[1] - e85.0[0]) * 0.5f32), e20);
            return e174;
        }
        v5 = (e1 / e17);
        let e180 = c.p.i[1];
        if (e180 == 2i32) {
            let e183 = v5;
            let e185 = turn(c, e183, 0.7853982f32);
            v5 = e185;
        }
        let e186 = v5;
        let e190 = (e186.map(f32::floor) + Vn([0.5f32; 2]));
        let e191 = v5;
        let e192 = (e191 - e190);
        v6 = (0.5f32 - f32::max(f32::abs(e192.0[0]), f32::abs(e192.0[1])));
        let e204 = c.p.i[1];
        if (e204 == 1i32) {
            v6 = (0.5f32 - e192.length());
        }
        v7 = e190;
        let e214 = c.p.i[1];
        if (e214 == 2i32) {
            let e218 = turn(c, e190, -0.7853982f32);
            v7 = e218;
        }
        let e219 = v7;
        let e223 = block_color(c, (e219 * e17), (e17 * Vn([0.8f32; 2])));
        let e227 = c.p.i[1];
        if (e227 == 1i32) {
            let e231 = pf(c, 2u32);
            let e236 = pf(c, 2u32);
            let e240 = v6;
            let e241 = ss(c, ((e231 * 0.5f32) - e20), ((e236 * 0.5f32) + e20), e240);
            let e243 = pf3(c, 3u32);
            let e246 = at0(c, a0.0[0], a0.0[1]);
            let e248 = back(c, e243, e246.0[3]);
            let e255 = pf(c, 6u32);
            let e262 = mix4(c, e248, (e223 * Vn([(1.0f32 + ((((e192.0[1] * -1.0f32) - e192.0[0]) * e255) * 0.6f32)); 4])), e241);
            return e262;
        }
        let e263 = v6;
        let e264 = framed(c, e223, e192, e263, e20);
        return e264;
    }

    pub fn k_mosh(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 2> = Vn([0.0f32; 2]);
        let mut v1: i32 = 0i32;
        let e1 = pos(c, a0);
        let e3 = pf(c, 1u32);
        let e8 = c.p.time;
        let e10 = pf(c, 3u32);
        let e11 = (e8 * e10);
        'l0: loop {
            'b0: {
                let e17 = v1;
                if (e17 < 2i32) {
                } else {
                    break 'l0;
                }
                let e20 = v1;
                let e24 = (f32::max(e3, 0.001f32) * (1.0f32 + (e20 as f32)));
                let e27 = (e1 / Vn([e24; 2])).map(f32::floor);
                let e34 = c.p.seed;
                let e35 = v1;
                let e38 = rnd(c, (e27.0[0] as i32), (e27.0[1] as i32), e34.wrapping_add((e35 as u32)));
                let e42 = f32::floor((e11 + (e38 * 3.0f32)));
                let e57 = c.p.seed;
                let e60 = v1;
                let e65 = rnd2(c, (e27.0[0] as i32).wrapping_add((e42 as i32).wrapping_mul(3i32)), (e27.0[1] as i32).wrapping_sub((e42 as i32).wrapping_mul(7i32)), e57.wrapping_add(5u32).wrapping_add((e60 as u32).wrapping_mul(3u32)));
                let e74 = c.p.seed;
                let e77 = rnd(c, (e27.0[0] as i32), (e27.0[1] as i32).wrapping_add((e42 as i32)), e74.wrapping_add(9u32));
                let e79 = pf(c, 2u32);
                let e84 = ((e27 + Vn([0.5f32; 2])) * Vn([e24; 2]));
                let e89 = pic1(c, (e84 + Vn([e24, 0.0f32])), 1i32);
                let e91 = luma(c, Vn([e89.0[0], e89.0[1], e89.0[2]]));
                let e96 = pic1(c, (e84 - Vn([e24, 0.0f32])), 1i32);
                let e98 = luma(c, Vn([e96.0[0], e96.0[1], e96.0[2]]));
                let e104 = pic1(c, (e84 + Vn([0.0f32, e24])), 1i32);
                let e106 = luma(c, Vn([e104.0[0], e104.0[1], e104.0[2]]));
                let e111 = pic1(c, (e84 - Vn([0.0f32, e24])), 1i32);
                let e113 = luma(c, Vn([e111.0[0], e111.0[1], e111.0[2]]));
                let e119 = pf(c, 4u32);
                let e121 = fr(c, e11);
                let e122 = v0;
                let e129 = pf(c, 4u32);
                let e130 = mix1(c, 1.0f32, 0.25f32, e129);
                let e134 = pf(c, 0u32);
                v0 = (e122 + ((((((e65 - Vn([0.5f32; 2])) * Vn([e130; 2])) + ((Vn([(e91 - e98), (e106 - e113)]) * Vn([2.0f32; 2])) * Vn([e119; 2]))) * Vn([e134; 2])) * Vn([step(e77, e79); 2])) * Vn([(0.35f32 + (0.65f32 * e121)); 2])));
            }
            let e144 = v1;
            v1 = e144.wrapping_add(1i32);
        }
        let e146 = v0;
        let e149 = pic0(c, (e1 - e146), 1i32);
        return e149;
    }

    pub fn k_oil(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 4> = Vn([0.0f32; 4]);
        let mut v1: f32 = 0.0f32;
        let mut v2: i32 = 0i32;
        let mut v3: Vn<f32, 4> = Vn([0.0f32; 4]);
        let mut v4: Vn<f32, 3> = Vn([0.0f32; 3]);
        let mut v5: f32 = 0.0f32;
        let mut v6: i32 = 0i32;
        let mut v7: i32 = 0i32;
        let mut v8: Vn<f32, 2> = Vn([0.0f32; 2]);
        let e5 = at1(c, a0.0[0].wrapping_add(1i32), a0.0[1]);
        let e7 = luma(c, Vn([e5.0[0], e5.0[1], e5.0[2]]));
        let e12 = at1(c, a0.0[0].wrapping_sub(1i32), a0.0[1]);
        let e14 = luma(c, Vn([e12.0[0], e12.0[1], e12.0[2]]));
        let e15 = (e7 - e14);
        let e20 = at1(c, a0.0[0], a0.0[1].wrapping_add(1i32));
        let e22 = luma(c, Vn([e20.0[0], e20.0[1], e20.0[2]]));
        let e27 = at1(c, a0.0[0], a0.0[1].wrapping_sub(1i32));
        let e29 = luma(c, Vn([e27.0[0], e27.0[1], e27.0[2]]));
        let e30 = (e22 - e29);
        let e33 = angle(c, Vn([(-e30), e15]));
        let e35 = pf(c, 2u32);
        let e40 = ss(c, 0.0f32, 0.02f32, Vn([e15, e30]).length());
        let e43 = (1.0f32 + (e35 * e40));
        let e47 = c.p.i[0];
        'l0: loop {
            'b0: {
                let e55 = v2;
                if (e55 < 8i32) {
                } else {
                    break 'l0;
                }
                let e58 = v2;
                v3 = Vn([0.0f32; 4]);
                v4 = Vn([0.0f32; 3]);
                v5 = 0.0f32;
                v6 = 0i32;
                'l1: loop {
                    'b1: {
                        let e74 = v6;
                        if (e74 < e47) {
                        } else {
                            break 'l1;
                        }
                        v7 = (-1i32);
                        'l2: loop {
                            'b2: {
                                let e78 = v7;
                                if (e78 <= 1i32) {
                                } else {
                                    break 'l2;
                                }
                                let e81 = v6;
                                let e88 = pf(c, 0u32);
                                let e90 = v7;
                                let e94 = ((((e58 as f32) * 6.2831855f32) / 8.0f32) + ((e90 as f32) * 0.26f32));
                                v8 = (Vn([(f32::cos(e94) * e43), (f32::sin(e94) / e43)]) * Vn([((((e81 as f32) + 0.5f32) / (e47 as f32)) * e88); 2]));
                                let e102 = v8;
                                let e103 = turn(c, e102, e33);
                                v8 = e103;
                                let e106 = v8.0[0];
                                let e114 = v8.0[1];
                                let e120 = at0(c, a0.0[0].wrapping_add((f32::floor((e106 + 0.5f32)) as i32)), a0.0[1].wrapping_add((f32::floor((e114 + 0.5f32)) as i32)));
                                let e121 = v3;
                                v3 = (e121 + e120);
                                let e123 = v4;
                                v4 = (e123 + (Vn([e120.0[0], e120.0[1], e120.0[2]]) * Vn([e120.0[0], e120.0[1], e120.0[2]])));
                                let e128 = v5;
                                v5 = (e128 + 1.0f32);
                            }
                            let e132 = v7;
                            v7 = e132.wrapping_add(1i32);
                        }
                    }
                    let e135 = v6;
                    v6 = e135.wrapping_add(1i32);
                }
                let e137 = v3;
                let e138 = v5;
                v3 = (e137 * Vn([(1.0f32 / e138); 4]));
                let e142 = v4;
                let e143 = v5;
                let e146 = v3;
                let e148 = v3;
                let e152 = ((e142 / Vn([e143; 3])) - (Vn([e146.0[0], e146.0[1], e146.0[2]]) * Vn([e148.0[0], e148.0[1], e148.0[2]]))).map(f32::abs);
                let e154 = pf(c, 1u32);
                let e167 = pw(c, ((255.0f32 * e154) * ((f32::sqrt(e152.0[0]) + f32::sqrt(e152.0[1])) + f32::sqrt(e152.0[2]))), 4.0f32);
                let e171 = (1.0f32 / (1.0f32 + e167));
                let e172 = v0;
                let e173 = v3;
                v0 = (e172 + (e173 * Vn([e171; 4])));
                let e176 = v1;
                v1 = (e176 + e171);
            }
            let e179 = v2;
            v2 = e179.wrapping_add(1i32);
        }
        let e181 = v0;
        let e182 = v1;
        return (e181 * Vn([(1.0f32 / f32::max(e182, 1e-9f32)); 4]));
    }

    pub fn k_outline(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let mut v1: Vn<f32, 3> = Vn([0.0f32; 3]);
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e5 = pf(c, 13u32);
        let e6 = edge_gap(c, e5);
        let e7 = sobel0(c, a0, e6);
        let e10 = pf(c, 0u32);
        let e13 = pf(c, 1u32);
        let e15 = pf(c, 1u32);
        let e17 = pf(c, 2u32);
        let e21 = ss(c, e13, (e15 + f32::max(e17, 0.01f32)), (e7.length() * e10));
        v0 = e21;
        let e24 = pf(c, 12u32);
        if (e24 > 0.0f32) {
            let e33 = pf(c, 11u32);
            let e36 = pf(c, 11u32);
            let e41 = v0;
            let e44 = pw(c, f32::abs((e7 / Vn([f32::max(e7.length(), 1e-5f32); 2])).dot(Vn([f32::cos(e33), f32::sin(e36)]))), 6.0f32);
            let e46 = pf(c, 12u32);
            let e47 = mix1(c, 1.0f32, e44, e46);
            v0 = (e41 * e47);
        }
        let e50 = pf3(c, 3u32);
        v1 = e50;
        let e52 = straight(c, e3);
        let e53 = v1;
        let e64 = pf(c, 10u32);
        let e65 = mix3(c, e53, (e52 / Vn([f32::max(f32::max(f32::max(e52.0[0], e52.0[1]), e52.0[2]), 0.05f32); 3])), e64);
        v1 = e65;
        let e69 = c.p.i[1];
        if (e69 == 1i32) {
            let e72 = angle(c, e7);
            let e77 = fr(c, ((e72 / 6.2831855f32) + 0.5f32));
            let e80 = hsv(c, e77, 0.9f32, 1.0f32);
            v1 = e80;
        }
        let e82 = pf3(c, 6u32);
        let e84 = pf(c, 9u32);
        let e85 = mix3(c, e82, e52, e84);
        let e86 = v1;
        let e87 = v0;
        let e89 = pf(c, 13u32);
        let e90 = edge_thin(c, e89);
        let e92 = mix3(c, e85, e86, (e87 * e90));
        let e94 = back(c, e92, e3.0[3]);
        return e94;
    }

    pub fn k_pencil(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = straight(c, e3);
        let e6 = luma(c, Vn([e3.0[0], e3.0[1], e3.0[2]]));
        let e9 = at1(c, a0.0[0], a0.0[1]);
        let e11 = luma(c, Vn([e9.0[0], e9.0[1], e9.0[2]]));
        let e21 = pw(c, (1.0f32 - clampf((e6 / f32::max(e11, 0.03f32)), 0.0f32, 1.0f32)), 0.8f32);
        let e23 = pf(c, 0u32);
        let e35 = pf(c, 0u32);
        v0 = (((e21 * e23) * 2.2f32) + (((1.0f32 - clampf(e6, 0.0f32, 1.0f32)) * 0.35f32) * e35));
        let e39 = pos(c, a0);
        let e41 = pf(c, 2u32);
        let e43 = turn(c, e39, (-e41));
        let e45 = pf(c, 1u32);
        let e49 = (e43 / Vn([f32::max(e45, 0.0001f32); 2]));
        let e58 = c.p.seed;
        let e59 = noise3(c, Vn([(e49.0[0] * 0.12f32), e49.0[1], 0.0f32]), e58);
        let e72 = c.p.seed;
        let e75 = noise3(c, Vn([(e49.0[0] * 0.35f32), (e49.0[1] * 2.7f32), 4.0f32]), e72.wrapping_add(1u32));
        let e80 = pf(c, 1u32);
        let e86 = ((e39 / Vn([f32::max(e80, 0.0001f32); 2])) * Vn([1.7f32; 2]));
        let e91 = c.p.seed;
        let e94 = noise3(c, Vn([e86.0[0], e86.0[1], 9.0f32]), e91.wrapping_add(2u32));
        let e95 = v0;
        let e104 = pf(c, 10u32);
        let e106 = v0;
        v0 = clampf(((e95 * (1.0f32 + (((e59 * 0.7f32) + (e75 * 0.5f32)) * 0.9f32))) - (((e94 * 0.22f32) * e104) * (1.0f32 - e106))), 0.0f32, 1.0f32);
        let e115 = pf3(c, 3u32);
        let e119 = pf(c, 9u32);
        let e120 = mix3(c, e115, (e4 * Vn([0.85f32; 3])), e119);
        let e122 = pf3(c, 6u32);
        let e126 = pf(c, 10u32);
        let e131 = v0;
        let e132 = mix3(c, (e122 * Vn([(1.0f32 + ((e94 * 0.05f32) * e126)); 3])), e120, e131);
        let e134 = back(c, e132, e3.0[3]);
        return e134;
    }

    pub fn k_psyche(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = straight(c, e3);
        let e5 = luma(c, e4);
        let e6 = pos(c, a0);
        let e8 = pf(c, 6u32);
        let e10 = turn(c, e6, (-e8));
        let e12 = pf(c, 0u32);
        let e16 = (e10 / Vn([f32::max(e12, 0.001f32); 2]));
        let e22 = c.p.i[1];
        if (e22 == 1i32) {
            let e32 = pf(c, 2u32);
            let e38 = c.p.seed;
            let e39 = wander(c, ((e16.0[1] * 0.7f32) + (e32 * 0.4f32)), e38);
            v0 = ((e16.0[0] * 0.5f32) + (e39 * 0.25f32));
        } else {
            let e44 = (e16 * Vn([0.6f32; 2]));
            let e46 = pf(c, 2u32);
            let e53 = c.p.seed;
            let e54 = fbm(c, Vn([e44.0[0], e44.0[1], (e46 * 0.3f32)]), 3i32, e53);
            v0 = (e54 * 0.8f32);
        }
        let e57 = v0;
        let e59 = pf(c, 1u32);
        let e62 = pf(c, 5u32);
        let e66 = pf(c, 2u32);
        let e68 = fr(c, (((e57 * e59) + (e5 * e62)) + e66));
        let e70 = pf(c, 3u32);
        let e78 = hsv(c, e68, e70, clampf((0.2f32 + (e5 * 1.1f32)), 0.0f32, 1.3f32));
        let e80 = pf(c, 4u32);
        let e81 = mix3(c, e4, e78, e80);
        let e83 = back(c, e81, e3.0[3]);
        return e83;
    }

    pub fn k_scatter(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 2> = Vn([0.0f32; 2]);
        let e1 = full_pixel(c, a0);
        let e3 = pf(c, 1u32);
        let e6 = i32_max((e3 as i32), 1i32);
        let e9 = c.p.time;
        let e11 = pf(c, 2u32);
        let e26 = c.p.seed;
        let e30 = rnd2(c, idiv(e1.0[0], e6), idiv(e1.0[1], e6), e26.wrapping_add((i32_max((f32::floor(((e9 * e11) + 0.001f32)) as i32), 0i32) as u32).wrapping_mul(131u32)));
        let e33 = (e30.0[0] * 6.2831855f32);
        let e41 = pf(c, 0u32);
        v0 = ((Vn([f32::cos(e33), f32::sin(e33)]) * Vn([f32::sqrt(e30.0[1]); 2])) * Vn([e41; 2]));
        let e45 = pf(c, 3u32);
        let e48 = pf(c, 3u32);
        let e58 = pf(c, 0u32);
        let e60 = v0;
        let e61 = v0;
        let e64 = pf(c, 4u32);
        v0 = (e60 + ((((Vn([f32::cos(e45), f32::sin(e48)]) * Vn([((e30.0[0] * 2.0f32) - 1.0f32); 2])) * Vn([e58; 2])) - e61) * Vn([e64; 2])));
        let e67 = pos(c, a0);
        let e68 = v0;
        let e73 = c.p.i[0];
        let e74 = pic0(c, (e67 + e68), e73);
        return e74;
    }

    pub fn k_sort(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: bool = false;
        let mut v1: Vn<i32, 2> = Vn([1i32, 0i32]);
        let mut v2: i32 = 0i32;
        let mut v3: i32 = 1i32;
        let mut v4: bool = false;
        let mut v5: bool = false;
        let mut v6: bool = false;
        let mut v7: i32 = 0i32;
        let mut v8: i32 = 1i32;
        let mut v9: bool = false;
        let mut v10: bool = false;
        let mut v11: bool = false;
        let mut v12: i32 = 0i32;
        let mut v13: f32 = 0.0f32;
        let mut v14: f32 = 0.0f32;
        let mut v15: i32 = 0i32;
        let mut v16: i32 = 0i32;
        let mut v17: i32 = 0i32;
        let mut v18: Vn<f32, 4> = Vn([0.0f32; 4]);
        let mut v19: f32 = 9.0f32;
        let mut v20: i32 = 0i32;
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = sort_key(c, e3);
        let e6 = pf(c, 0u32);
        if (!(e4 < e6)) {
            let e12 = pf(c, 1u32);
            v0 = (e4 > e12);
        } else {
            v0 = true;
        }
        let e15 = v0;
        if e15 {
            return e3;
        }
        let e23 = c.p.i[0];
        if (e23 == 1i32) {
            v1 = Vn([0i32, 1i32]);
        }
        let e32 = c.p.i[2];
        let e34 = pf(c, 2u32);
        let e39 = i32_max(((e34 / (e32 as f32)) as i32), 1i32);
        'l0: loop {
            'b0: {
                let e44 = v3;
                if (e44 <= e32) {
                } else {
                    break 'l0;
                }
                let e48 = v1.0[0];
                let e49 = v3;
                let e55 = v1.0[1];
                let e56 = v3;
                let e60 = at0(c, a0.0[0].wrapping_sub(e48.wrapping_mul(e49).wrapping_mul(e39)), a0.0[1].wrapping_sub(e55.wrapping_mul(e56).wrapping_mul(e39)));
                let e61 = sort_key(c, e60);
                let e63 = pf(c, 0u32);
                if (!(e61 < e63)) {
                    let e69 = pf(c, 1u32);
                    v4 = (e61 > e69);
                } else {
                    v4 = true;
                }
                let e72 = v4;
                if (!e72) {
                    let e78 = v1.0[0];
                    let e79 = v3;
                    v5 = (a0.0[0].wrapping_sub(e78.wrapping_mul(e79).wrapping_mul(e39)) < 0i32);
                } else {
                    v5 = true;
                }
                let e86 = v5;
                if (!e86) {
                    let e92 = v1.0[1];
                    let e93 = v3;
                    v6 = (a0.0[1].wrapping_sub(e92.wrapping_mul(e93).wrapping_mul(e39)) < 0i32);
                } else {
                    v6 = true;
                }
                let e100 = v6;
                if e100 {
                    break 'l0;
                }
                let e101 = v3;
                v2 = e101;
            }
            let e103 = v3;
            v3 = e103.wrapping_add(1i32);
        }
        'l1: loop {
            'b1: {
                let e109 = v8;
                if (e109 <= e32) {
                } else {
                    break 'l1;
                }
                let e113 = v1.0[0];
                let e114 = v8;
                let e120 = v1.0[1];
                let e121 = v8;
                let e125 = at0(c, a0.0[0].wrapping_add(e113.wrapping_mul(e114).wrapping_mul(e39)), a0.0[1].wrapping_add(e120.wrapping_mul(e121).wrapping_mul(e39)));
                let e126 = sort_key(c, e125);
                let e128 = pf(c, 0u32);
                if (!(e126 < e128)) {
                    let e134 = pf(c, 1u32);
                    v9 = (e126 > e134);
                } else {
                    v9 = true;
                }
                let e137 = v9;
                if (!e137) {
                    let e143 = v1.0[0];
                    let e144 = v8;
                    let e150 = c.p.w0;
                    v10 = (a0.0[0].wrapping_add(e143.wrapping_mul(e144).wrapping_mul(e39)) >= (e150 as i32));
                } else {
                    v10 = true;
                }
                let e154 = v10;
                if (!e154) {
                    let e160 = v1.0[1];
                    let e161 = v8;
                    let e167 = c.p.h0;
                    v11 = (a0.0[1].wrapping_add(e160.wrapping_mul(e161).wrapping_mul(e39)) >= (e167 as i32));
                } else {
                    v11 = true;
                }
                let e171 = v11;
                if e171 {
                    break 'l1;
                }
                let e172 = v8;
                v7 = e172;
            }
            let e174 = v8;
            v8 = e174.wrapping_add(1i32);
        }
        let e176 = v2;
        let e177 = v7;
        if (e176.wrapping_add(e177).wrapping_add(1i32) < 2i32) {
            return e3;
        }
        let e183 = v2;
        v12 = e183;
        let e188 = c.p.i[1];
        if (e188 == 1i32) {
            let e191 = v7;
            v12 = e191;
        }
        let e193 = pf(c, 0u32);
        v13 = e193;
        let e196 = pf(c, 1u32);
        v14 = f32::min(e196, 1.2f32);
        'l2: loop {
            'b2: {
                let e202 = v15;
                if (e202 < 7i32) {
                } else {
                    break 'l2;
                }
                let e205 = v13;
                let e206 = v14;
                let e209 = ((e205 + e206) * 0.5f32);
                v16 = 0i32;
                let e212 = v2;
                v17 = e212.wrapping_neg();
                'l3: loop {
                    'b3: {
                        let e215 = v17;
                        let e216 = v7;
                        if (e215 <= e216) {
                        } else {
                            break 'l3;
                        }
                        let e220 = v1.0[0];
                        let e221 = v17;
                        let e227 = v1.0[1];
                        let e228 = v17;
                        let e232 = at0(c, a0.0[0].wrapping_add(e220.wrapping_mul(e221).wrapping_mul(e39)), a0.0[1].wrapping_add(e227.wrapping_mul(e228).wrapping_mul(e39)));
                        let e233 = sort_key(c, e232);
                        if (e233 < e209) {
                            let e235 = v16;
                            v16 = e235.wrapping_add(1i32);
                        }
                    }
                    let e239 = v17;
                    v17 = e239.wrapping_add(1i32);
                }
                let e241 = v16;
                let e242 = v12;
                if (e241 <= e242) {
                    v13 = e209;
                } else {
                    v14 = e209;
                }
            }
            let e245 = v15;
            v15 = e245.wrapping_add(1i32);
        }
        let e247 = v13;
        let e248 = v14;
        v18 = e3;
        let e255 = v2;
        v20 = e255.wrapping_neg();
        'l4: loop {
            'b4: {
                let e258 = v20;
                let e259 = v7;
                if (e258 <= e259) {
                } else {
                    break 'l4;
                }
                let e263 = v1.0[0];
                let e264 = v20;
                let e270 = v1.0[1];
                let e271 = v20;
                let e275 = at0(c, a0.0[0].wrapping_add(e263.wrapping_mul(e264).wrapping_mul(e39)), a0.0[1].wrapping_add(e270.wrapping_mul(e271).wrapping_mul(e39)));
                let e276 = sort_key(c, e275);
                let e278 = f32::abs((e276 - ((e247 + e248) * 0.5f32)));
                let e279 = v19;
                if (e278 < e279) {
                    v19 = e278;
                    v18 = e275;
                }
            }
            let e282 = v20;
            v20 = e282.wrapping_add(1i32);
        }
        let e284 = v18;
        return e284;
    }

    pub fn k_tape(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let mut v1: Vn<f32, 3> = Vn([0.0f32; 3]);
        let e1 = uv_of(c, a0);
        let e2 = full_pixel(c, a0);
        let e5 = c.p.time;
        let e13 = c.p.seed;
        let e20 = e13.wrapping_add((i32_max((f32::floor(((e5 * 29.97f32) + 0.001f32)) as i32), 0i32) as u32).wrapping_mul(7u32));
        let e24 = f32::floor((e1.0[1] * 240.0f32));
        let e30 = c.p.time;
        let e36 = c.p.seed;
        let e37 = wander(c, ((e1.0[1] * 22.0f32) + (e30 * 4.0f32)), e36);
        let e45 = c.p.time;
        let e51 = c.p.seed;
        let e54 = wander(c, ((e1.0[1] * 3.0f32) - (e45 * 1.4f32)), e51.wrapping_add(1u32));
        let e59 = rnd1(c, (e24 as i32), e20);
        let e66 = pf(c, 1u32);
        v0 = ((((e37 * 0.0009f32) + (e54 * 0.0016f32)) + ((e59 - 0.5f32) * 0.0016f32)) * e66);
        let e70 = pf(c, 3u32);
        let e77 = ss(c, (1.0f32 - (0.055f32 * e70)), 1.0f32, e1.0[1]);
        let e79 = pf(c, 3u32);
        let e82 = (e77 * step(0.001f32, e79));
        let e83 = v0;
        let e87 = rnd1(c, (e24 as i32), e20.wrapping_add(2u32));
        let e95 = c.p.time;
        let e100 = c.p.seed;
        let e103 = wander(c, (e95 * 2.0f32), e100.wrapping_add(4u32));
        v0 = (e83 + ((e82 * (0.03f32 + (0.02f32 * e87))) * (1.0f32 + (e103 * 0.5f32))));
        let e112 = c.p.time;
        let e119 = c.p.seed;
        let e120 = rnd1(c, 3i32, e119);
        let e122 = fr(c, (((-e112) * 0.11f32) + e120));
        let e127 = sq(c, ((e1.0[1] - e122) * 26.0f32));
        let e131 = pf(c, 2u32);
        let e132 = (f32::exp((-e127)) * e131);
        let e133 = v0;
        let e137 = rnd1(c, (e24 as i32), e20.wrapping_add(3u32));
        v0 = (e133 + ((e132 * (e137 - 0.5f32)) * 0.06f32));
        let e145 = v0;
        let e148 = Vn([(e1.0[0] + e145), e1.0[1]]);
        let e150 = pic0_uv(c, e148, 1i32);
        let e152 = pic1_uv(c, e148, 1i32);
        let e154 = pf(c, 0u32);
        let e157 = c.p.shape;
        let e163 = pic1_uv(c, (e148 - Vn([(e154 / e157), 0.0f32])), 1i32);
        let e165 = pf(c, 5u32);
        let e166 = mix4(c, e150, e152, e165);
        let e168 = luma(c, Vn([e166.0[0], e166.0[1], e166.0[2]]));
        let e171 = luma(c, Vn([e163.0[0], e163.0[1], e163.0[2]]));
        v1 = (Vn([e168; 3]) + ((Vn([e163.0[0], e163.0[1], e163.0[2]]) - Vn([e171; 3])) * Vn([1.05f32; 3])));
        let e179 = v1;
        let e186 = rnd(c, idiv(e2.0[0], 2i32), e2.0[1], e20.wrapping_add(9u32));
        let e192 = pf(c, 4u32);
        let e195 = specks(c, 1.4f32);
        v1 = (e179 + Vn([((((e186 - 0.5f32) * 0.14f32) * e192) * e195); 3]));
        let e207 = rnd2(c, idiv(e2.0[0], 9i32), idiv(e2.0[1], 2i32), e20.wrapping_add(10u32));
        let e214 = pf(c, 4u32);
        let e215 = (((e207 - Vn([0.5f32; 2])) * Vn([0.1f32; 2])) * Vn([e214; 2]));
        let e216 = v1;
        v1 = (e216 + Vn([e215.0[0], 0.0f32, e215.0[1]]));
        let e230 = rnd(c, idiv(e2.0[0], 5i32), idiv(e2.0[1], 2i32), e20.wrapping_add(11u32));
        let e239 = v1;
        let e247 = mix3(c, e239, Vn([0.85f32; 3]), (clampf((step(0.72f32, e230) * ((e132 * 0.9f32) + (e82 * 0.7f32))), 0.0f32, 1.0f32) * 0.7f32));
        v1 = e247;
        let e249 = pf(c, 7u32);
        let e257 = rnd1(c, (e24 as i32), e20.wrapping_add(12u32));
        let e265 = rnd1(c, (e24 as i32), e20.wrapping_add(13u32));
        let e267 = fr(c, ((e1.0[0] * 1.7f32) + e265));
        let e271 = v1;
        let e274 = mix3(c, e271, Vn([0.95f32; 3]), (step((1.0f32 - (0.004f32 * e249)), e257) * step(e267, 0.25f32)));
        v1 = e274;
        let e275 = v1;
        let e278 = v1;
        let e283 = pf(c, 6u32);
        let e284 = mix3(c, e275, (Vn([0.1f32; 3]) + (e278 * Vn([0.82f32; 3]))), e283);
        v1 = e284;
        let e285 = v1;
        let e287 = pf(c, 6u32);
        let e292 = saturate3(c, e285, (1.0f32 - (0.3f32 * e287)));
        v1 = e292;
        let e293 = v1;
        let e296 = e293.zip(Vn([0.0f32; 3]), f32::max);
        return Vn([e296.0[0], e296.0[1], e296.0[2], f32::max(e150.0[3], e152.0[3])]);
    }

    pub fn k_tube(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 2> = Vn([0.0f32; 2]);
        let mut v1: Vn<f32, 3> = Vn([0.0f32; 3]);
        let e1 = pos(c, a0);
        v0 = e1;
        let e4 = pf(c, 9u32);
        let e7 = v0;
        let e8 = v0;
        let e9 = v0;
        v0 = (e7 * Vn([(1.0f32 + ((e4 * 0.35f32) * e8.dot(e9))); 2]));
        let e15 = v0;
        let e17 = pic0(c, e15, 0i32);
        let e18 = v0;
        let e19 = uv_at(c, e18);
        let e21 = luma(c, Vn([e17.0[0], e17.0[1], e17.0[2]]));
        let e24 = pf3(c, 5u32);
        let e27 = pf(c, 8u32);
        let e28 = mix3(c, Vn([e17.0[0], e17.0[1], e17.0[2]]), (e24 * Vn([e21; 3])), e27);
        v1 = e28;
        let e32 = pf(c, 0u32);
        let e36 = c.p.time;
        let e38 = pf(c, 3u32);
        let e41 = fr(c, ((e19.0[1] * e32) - (e36 * e38)));
        let e54 = pf(c, 2u32);
        let e55 = mix1(c, 0.35f32, 2.2f32, e54);
        let e56 = pw(c, (0.5f32 + (0.5f32 * f32::cos(((e41 - 0.5f32) * 6.2831855f32)))), e55);
        let e63 = mix1(c, e56, 1.0f32, clampf((e21 * 0.45f32), 0.0f32, 0.6f32));
        let e68 = c.p.h;
        let e71 = pf(c, 0u32);
        let e75 = ss(c, 1.6f32, 3.2f32, ((e68 as f32) / f32::max(e71, 1.0f32)));
        let e76 = v1;
        let e81 = mix1(c, 0.72f32, (e63 * 1.25f32), e75);
        let e83 = pf(c, 1u32);
        let e84 = mix1(c, 1.0f32, e81, e83);
        v1 = (e76 * Vn([e84; 3]));
        let e87 = pf(c, 4u32);
        if (e87 > 0.0f32) {
            let e92 = pf(c, 0u32);
            let e96 = c.p.shape;
            let e100 = fr(c, (((e19.0[0] * e92) * e96) * 0.75f32));
            let e102 = (e100 * 3.0f32);
            let e108 = ss(c, 1.0f32, 0.4f32, f32::abs((e102 - 0.5f32)));
            let e114 = ss(c, 1.0f32, 0.4f32, f32::abs((e102 - 1.5f32)));
            let e120 = ss(c, 1.0f32, 0.4f32, f32::abs((e102 - 2.5f32)));
            let e126 = c.p.w;
            let e129 = pf(c, 0u32);
            let e132 = c.p.shape;
            let e139 = ss(c, 2.0f32, 4.0f32, ((e126 as f32) / f32::max(((e129 * e132) * 0.75f32), 1.0f32)));
            let e140 = v1;
            let e150 = mix3(c, Vn([0.8f32; 3]), ((Vn([e108, e114, e120]) * Vn([1.7f32; 3])) + Vn([0.15f32; 3])), e139);
            let e152 = pf(c, 4u32);
            let e153 = mix3(c, Vn([1.0f32; 3]), e150, e152);
            v1 = (e140 * e153);
        }
        let e158 = v0.0[0];
        let e161 = c.p.shape;
        let e164 = v0.0[1];
        let e169 = ss(c, 0.35f32, 0.95f32, (Vn([(e158 / e161), e164]) * Vn([1.35f32; 2])).length());
        let e171 = pf(c, 10u32);
        let e175 = v1;
        let e176 = (e175 * Vn([(1.0f32 - (e169 * e171)); 3]));
        return Vn([e176.0[0], e176.0[1], e176.0[2], e17.0[3]]);
    }

    pub fn k_vignette(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = pos(c, a0);
        let e6 = pf2(c, 0u32);
        let e7 = (e4 - e6);
        let e12 = c.p.shape;
        let e22 = pf(c, 5u32);
        let e23 = mix1(c, (Vn([(e7.0[0] / f32::max(e12, 0.001f32)), e7.0[1]]).length() * 1.2f32), e7.length(), e22);
        let e25 = pf(c, 2u32);
        let e27 = pf(c, 2u32);
        let e29 = pf(c, 3u32);
        let e33 = ss(c, e25, (e27 + f32::max(e29, 0.001f32)), e23);
        let e35 = pf(c, 4u32);
        v0 = (e33 * e35);
        let e41 = c.p.i[0];
        if (e41 == 1i32) {
            let e45 = pf(c, 2u32);
            let e47 = pf(c, 2u32);
            let e49 = pf(c, 3u32);
            let e53 = ss(c, e45, (e47 + f32::max(e49, 0.001f32)), e23);
            let e57 = pf(c, 4u32);
            v0 = ((1.0f32 - e53) * e57);
        }
        let e59 = straight(c, e3);
        let e60 = luma(c, e59);
        let e61 = v0;
        let e63 = pf(c, 9u32);
        let e66 = ss(c, 0.6f32, 1.0f32, e60);
        v0 = (e61 * (1.0f32 - (e63 * e66)));
        let e73 = pf3(c, 6u32);
        let e76 = v0;
        let e80 = mix3(c, Vn([e3.0[0], e3.0[1], e3.0[2]]), (e73 * Vn([e3.0[3]; 3])), clampf(e76, 0.0f32, 1.0f32));
        return Vn([e80.0[0], e80.0[1], e80.0[2], e3.0[3]]);
    }

    pub fn k_wash(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = at1(c, a0.0[0], a0.0[1]);
        let e7 = straight(c, e6);
        let e10 = at2(c, a0.0[0], a0.0[1]);
        let e11 = straight(c, e10);
        let e14 = pos(c, a0);
        let e16 = pf(c, 4u32);
        let e20 = (1.0f32 / f32::max(e16, 0.001f32));
        let e21 = (e14 * Vn([e20; 2]));
        let e27 = c.p.seed;
        let e28 = fbm(c, Vn([e21.0[0], e21.0[1], 0.0f32]), 4i32, e27);
        let e31 = ((e14 * Vn([e20; 2])) * Vn([14.0f32; 2]));
        let e37 = c.p.seed;
        let e40 = fbm(c, Vn([e31.0[0], e31.0[1], 3.0f32]), 2i32, e37.wrapping_add(1u32));
        let e42 = pf(c, 1u32);
        let e50 = ss(c, 0.04f32, 0.22f32, (e7 - e11).length());
        let e52 = pf(c, 0u32);
        let e58 = pf(c, 2u32);
        let e68 = pw3(c, e7.zip(Vn([0.004f32; 3]), f32::max), f32::max((((1.0f32 + ((e28 * e42) * 0.9f32)) + ((e50 * e52) * 0.8f32)) + ((e40 * e58) * 0.25f32)), 0.2f32));
        let e72 = pf(c, 3u32);
        let e75 = luma(c, e68);
        let e81 = mix3(c, e68, Vn([1.0f32; 3]), ((e72 * 0.5f32) * (1.0f32 - (e75 * 0.4f32))));
        let e83 = back(c, e81, e3.0[3]);
        return e83;
    }

    pub fn k_zebra_light(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e4 = straight(c, e3);
        let e8 = c.p.i[1];
        if (e8 == 1i32) {
            let e12 = band1(c, e4.0[0]);
            let e14 = band1(c, e4.0[1]);
            let e16 = band1(c, e4.0[2]);
            let e19 = pf(c, 9u32);
            let e20 = mix3(c, e4, Vn([e12, e14, e16]), e19);
            let e22 = back(c, e20, e3.0[3]);
            return e22;
        }
        let e24 = pf3(c, 3u32);
        let e26 = pf3(c, 6u32);
        let e27 = luma(c, e4);
        let e28 = band1(c, e27);
        let e29 = mix3(c, e24, e26, e28);
        let e31 = pf(c, 9u32);
        let e32 = mix3(c, e4, e29, e31);
        let e34 = back(c, e32, e3.0[3]);
        return e34;
    }

    #[inline]
    pub fn screen1(c: &C<'_>, a0: Vn<f32, 2>, a1: f32, a2: f32) -> f32 {
        let mut v0: f32 = 0.0f32;
        let e4 = pf(c, 0u32);
        let e6 = f32::max(e4, 0.0001f32);
        let e8 = turn(c, a0, (-a1));
        let e10 = (e8 / Vn([e6; 2]));
        let e16 = c.p.i[1];
        if (e16 == 1i32) {
            let e20 = fr(c, e10.0[1]);
            v0 = (f32::abs((e20 - 0.5f32)) * 2.0f32);
        } else {
            let e29 = c.p.i[1];
            if (e29 == 2i32) {
                let e39 = (((e10 - e10.map(f32::floor)) - Vn([0.5f32; 2])).map(f32::abs) * Vn([2.0f32; 2]));
                v0 = f32::max(e39.0[0], e39.0[1]);
            } else {
                let e46 = c.p.i[1];
                if (e46 == 3i32) {
                    let e50 = pf2(c, 10u32);
                    let e54 = fr(c, ((a0 - e50).length() / e6));
                    v0 = (f32::abs((e54 - 0.5f32)) * 2.0f32);
                } else {
                    let e64 = ((e10 - e10.map(f32::floor)) - Vn([0.5f32; 2]));
                    v0 = (0.5f32 + (0.25f32 * (f32::cos((e64.0[0] * 6.2831855f32)) + f32::cos((e64.0[1] * 6.2831855f32)))));
                    let e78 = v0;
                    v0 = (1.0f32 - e78);
                }
            }
        }
        let e82 = pf(c, 2u32);
        let e89 = px(c);
        let e93 = ((f32::max((1.0f32 - e82), 0.0f32) * 0.5f32) + ((e89 / e6) * 1.6f32));
        let e95 = (1.0f32 - a2);
        let e98 = v0;
        let e99 = ss(c, (e95 - e93), (e95 + e93), e98);
        return (e99 * step(0.004f32, a2));
    }

    #[inline]
    pub fn sobel0(c: &C<'_>, a0: Vn<i32, 2>, a1: f32) -> Vn<f32, 2> {
        let e6 = Vn([(a0.0[0] as f32), (a0.0[1] as f32)]);
        let e11 = bil0(c, (e6 + Vn([(-a1), (-a1)])));
        let e13 = luma(c, Vn([e11.0[0], e11.0[1], e11.0[2]]));
        let e18 = bil0(c, (e6 + Vn([0.0f32, (-a1)])));
        let e20 = luma(c, Vn([e18.0[0], e18.0[1], e18.0[2]]));
        let e24 = bil0(c, (e6 + Vn([a1, (-a1)])));
        let e26 = luma(c, Vn([e24.0[0], e24.0[1], e24.0[2]]));
        let e31 = bil0(c, (e6 + Vn([(-a1), 0.0f32])));
        let e33 = luma(c, Vn([e31.0[0], e31.0[1], e31.0[2]]));
        let e37 = bil0(c, (e6 + Vn([a1, 0.0f32])));
        let e39 = luma(c, Vn([e37.0[0], e37.0[1], e37.0[2]]));
        let e43 = bil0(c, (e6 + Vn([(-a1), a1])));
        let e45 = luma(c, Vn([e43.0[0], e43.0[1], e43.0[2]]));
        let e49 = bil0(c, (e6 + Vn([0.0f32, a1])));
        let e51 = luma(c, Vn([e49.0[0], e49.0[1], e49.0[2]]));
        let e54 = bil0(c, (e6 + Vn([a1, a1])));
        let e56 = luma(c, Vn([e54.0[0], e54.0[1], e54.0[2]]));
        return (Vn([(((((e26 + (2.0f32 * e39)) + e56) - e13) - (2.0f32 * e33)) - e45), (((((e45 + (2.0f32 * e51)) + e56) - e13) - (2.0f32 * e20)) - e26)]) * Vn([0.25f32; 2]));
    }

    #[inline]
    pub fn sort_key(c: &C<'_>, a0: Vn<f32, 4>) -> f32 {
        let e1 = straight(c, a0);
        let e2 = luma(c, e1);
        return e2;
    }

    #[inline]
    pub fn tone(c: &C<'_>, a0: Vn<f32, 3>, a1: u32) -> f32 {
        let e2 = luma(c, a0);
        let e6 = pf(c, 9u32);
        return clampf((((e2 - 0.5f32) * e6) + 0.5f32), 0.0f32, 1.0f32);
    }

}

mod time {
    use super::shared::*;
    use super::*;
    pub fn k_calm(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = at1(c, a0.0[0], a0.0[1]);
        let e9 = at2(c, a0.0[0], a0.0[1]);
        let e11 = pf(c, 0u32);
        let e12 = sq(c, e11);
        let e16 = (-0.5f32 / f32::max(e12, 1e-6f32));
        let e19 = (Vn([e6.0[0], e6.0[1], e6.0[2]]) - Vn([e3.0[0], e3.0[1], e3.0[2]]));
        let e22 = (Vn([e9.0[0], e9.0[1], e9.0[2]]) - Vn([e3.0[0], e3.0[1], e3.0[2]]));
        let e27 = pf(c, 1u32);
        let e28 = (f32::exp((e19.dot(e19) * e16)) * e27);
        let e33 = pf(c, 1u32);
        let e34 = (f32::exp((e22.dot(e22) * e16)) * e33);
        return (((e3 + (e6 * Vn([e28; 4]))) + (e9 * Vn([e34; 4]))) * Vn([(1.0f32 / ((1.0f32 + e28) + e34)); 4]));
    }

    pub fn k_comb(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 4> = Vn([0.0f32; 4]);
        let mut v1: bool = false;
        let mut v2: bool = false;
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e10 = c.p.i[0];
        if ((a0.0[1] & 1i32) == e10) {
            return e3;
        }
        let e16 = at0(c, a0.0[0], a0.0[1].wrapping_sub(1i32));
        let e21 = at0(c, a0.0[0], a0.0[1].wrapping_add(1i32));
        let e28 = at0(c, a0.0[0].wrapping_sub(1i32), a0.0[1].wrapping_sub(1i32));
        let e35 = at0(c, a0.0[0].wrapping_add(1i32), a0.0[1].wrapping_add(1i32));
        let e42 = at0(c, a0.0[0].wrapping_add(1i32), a0.0[1].wrapping_sub(1i32));
        let e49 = at0(c, a0.0[0].wrapping_sub(1i32), a0.0[1].wrapping_add(1i32));
        let e51 = luma(c, Vn([e16.0[0], e16.0[1], e16.0[2]]));
        let e53 = luma(c, Vn([e21.0[0], e21.0[1], e21.0[2]]));
        let e55 = f32::abs((e51 - e53));
        let e57 = luma(c, Vn([e28.0[0], e28.0[1], e28.0[2]]));
        let e59 = luma(c, Vn([e35.0[0], e35.0[1], e35.0[2]]));
        let e61 = f32::abs((e57 - e59));
        let e63 = luma(c, Vn([e42.0[0], e42.0[1], e42.0[2]]));
        let e65 = luma(c, Vn([e49.0[0], e49.0[1], e49.0[2]]));
        let e67 = f32::abs((e63 - e65));
        v0 = ((e16 + e21) * Vn([0.5f32; 4]));
        if (e61 < (e55 * 0.8f32)) {
            v1 = (e61 <= e67);
        } else {
            v1 = false;
        }
        let e79 = v1;
        if e79 {
            v0 = ((e28 + e35) * Vn([0.5f32; 4]));
        } else {
            if (e67 < (e55 * 0.8f32)) {
                v2 = (e67 < e61);
            } else {
                v2 = false;
            }
            let e90 = v2;
            if e90 {
                v0 = ((e42 + e49) * Vn([0.5f32; 4]));
            }
        }
        let e97 = c.p.i[1];
        if (e97 == 1i32) {
            let e100 = v0;
            return e100;
        }
        let e103 = at1(c, a0.0[0], a0.0[1]);
        let e108 = at1(c, a0.0[0], a0.0[1].wrapping_sub(1i32));
        let e113 = at1(c, a0.0[0], a0.0[1].wrapping_add(1i32));
        let e115 = luma(c, Vn([e3.0[0], e3.0[1], e3.0[2]]));
        let e117 = luma(c, Vn([e103.0[0], e103.0[1], e103.0[2]]));
        let e121 = luma(c, Vn([e16.0[0], e16.0[1], e16.0[2]]));
        let e123 = luma(c, Vn([e108.0[0], e108.0[1], e108.0[2]]));
        let e127 = luma(c, Vn([e21.0[0], e21.0[1], e21.0[2]]));
        let e129 = luma(c, Vn([e113.0[0], e113.0[1], e113.0[2]]));
        let e133 = v0;
        let e135 = pf(c, 0u32);
        let e139 = pf(c, 0u32);
        let e145 = ss(c, (e135 * 0.5f32), ((e139 * 1.5f32) + 0.0001f32), f32::max(f32::abs((e115 - e117)), f32::max(f32::abs((e121 - e123)), f32::abs((e127 - e129)))));
        let e146 = mix4(c, e3, e133, e145);
        return e146;
    }

    pub fn k_drift(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 2> = Vn([0.0f32; 2]);
        let e1 = pos(c, a0);
        let e3 = pf2(c, 4u32);
        let e6 = pf2(c, 0u32);
        let e8 = pf2(c, 0u32);
        let e11 = pf(c, 3u32);
        let e13 = turn(c, ((e1 - e3) - e8), (-e11));
        let e15 = pf(c, 2u32);
        v0 = (e6 + (e13 / Vn([f32::max(e15, 0.001f32); 2])));
        let e23 = pf(c, 6u32);
        if (e23 > 0.0f32) {
            let e27 = pf(c, 7u32);
            let e31 = (1.0f32 / f32::max(e27, 0.001f32));
            let e32 = v0;
            let e33 = v0;
            let e34 = (e33 * Vn([e31; 2]));
            let e37 = c.p.time;
            let e44 = c.p.seed;
            let e45 = fbm(c, Vn([e34.0[0], e34.0[1], (e37 * 0.3f32)]), 3i32, e44);
            let e46 = v0;
            let e51 = ((e46 * Vn([e31; 2])) + Vn([5.0f32, 9.0f32]));
            let e54 = c.p.time;
            let e61 = c.p.seed;
            let e64 = fbm(c, Vn([e51.0[0], e51.0[1], (e54 * 0.3f32)]), 3i32, e61.wrapping_add(1u32));
            let e67 = pf(c, 6u32);
            v0 = (e32 + (Vn([e45, e64]) * Vn([e67; 2])));
        }
        let e71 = pf(c, 8u32);
        if (e71 != 0.0f32) {
            let e74 = v0;
            let e80 = pic0(c, (e74 - Vn([0.01f32, 0.0f32])), 1i32);
            let e82 = luma(c, Vn([e80.0[0], e80.0[1], e80.0[2]]));
            let e83 = v0;
            let e89 = pic0(c, (e83 + Vn([0.01f32, 0.0f32])), 1i32);
            let e91 = luma(c, Vn([e89.0[0], e89.0[1], e89.0[2]]));
            let e92 = v0;
            let e98 = pic0(c, (e92 - Vn([0.0f32, 0.01f32])), 1i32);
            let e100 = luma(c, Vn([e98.0[0], e98.0[1], e98.0[2]]));
            let e101 = v0;
            let e107 = pic0(c, (e101 + Vn([0.0f32, 0.01f32])), 1i32);
            let e109 = luma(c, Vn([e107.0[0], e107.0[1], e107.0[2]]));
            let e110 = v0;
            let e115 = pf(c, 8u32);
            v0 = (e110 + (Vn([(e91 - e82), (e109 - e100)]) * Vn([e115; 2])));
        }
        let e118 = v0;
        let e120 = pic0(c, e118, 0i32);
        return e120;
    }

    pub fn k_flicker(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e5 = pf(c, 5u32);
        let e9 = pf(c, 5u32);
        let e12 = straight(c, e3);
        let e13 = luma(c, e12);
        let e15 = pf(c, 5u32);
        let e19 = ss(c, (e5 - 0.1f32), (e9 + 0.1f32), (e13 + step(e15, 0.0f32)));
        let e23 = pf3(c, 1u32);
        let e25 = pf(c, 4u32);
        let e27 = pf(c, 0u32);
        let e36 = (Vn([e3.0[0], e3.0[1], e3.0[2]]) * (Vn([1.0f32; 3]) + (e23 * Vn([((e25 * e27) * e19); 3]))).zip(Vn([0.0f32; 3]), f32::max));
        return Vn([e36.0[0], e36.0[1], e36.0[2], e3.0[3]]);
    }

    pub fn k_frames(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 1.0f32;
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e5 = pf(c, 0u32);
        let e9 = at1(c, a0.0[0], a0.0[1]);
        let e11 = pf(c, 1u32);
        let e15 = at2(c, a0.0[0], a0.0[1]);
        let e17 = pf(c, 2u32);
        let e22 = pf(c, 3u32);
        if (e22 > 0.5f32) {
            let e26 = pf(c, 0u32);
            let e28 = pf(c, 1u32);
            let e31 = pf(c, 2u32);
            v0 = (1.0f32 / f32::max(((e26 + e28) + e31), 1e-5f32));
        }
        let e39 = v0;
        return ((((e3 * Vn([e5; 4])) + (e9 * Vn([e11; 4]))) + (e15 * Vn([e17; 4]))) * Vn([e39; 4]));
    }

    pub fn k_mean(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 4> = Vn([0.0f32; 4]);
        let mut v1: i32 = 0i32;
        let mut v2: i32 = 0i32;
        'l0: loop {
            'b0: {
                let e6 = v1;
                let e10 = c.p.i[1];
                if (e6 < e10) {
                } else {
                    break 'l0;
                }
                v2 = 0i32;
                'l1: loop {
                    'b1: {
                        let e14 = v2;
                        let e18 = c.p.i[0];
                        if (e14 < e18) {
                        } else {
                            break 'l1;
                        }
                        let e20 = v2;
                        let e27 = c.p.i[0];
                        let e30 = v1;
                        let e37 = c.p.i[1];
                        let e41 = v0;
                        let e43 = pic0_uv(c, Vn([(((e20 as f32) + 0.5f32) / (e27 as f32)), (((e30 as f32) + 0.5f32) / (e37 as f32))]), 1i32);
                        v0 = (e41 + e43);
                    }
                    let e46 = v2;
                    v2 = e46.wrapping_add(1i32);
                }
            }
            let e49 = v1;
            v1 = e49.wrapping_add(1i32);
        }
        let e51 = v0;
        let e55 = c.p.i[0];
        let e59 = c.p.i[1];
        return (e51 * Vn([(1.0f32 / (e55.wrapping_mul(e59) as f32)); 4]));
    }

    pub fn k_mean3(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e2 = pf(c, 0u32);
        let e4 = pf(c, 1u32);
        let e7 = pf(c, 2u32);
        let e15 = at0(c, 0i32, 0i32);
        let e17 = pf(c, 0u32);
        let e21 = at1(c, 0i32, 0i32);
        let e23 = pf(c, 1u32);
        let e28 = at2(c, 0i32, 0i32);
        let e30 = pf(c, 2u32);
        return ((((e15 * Vn([e17; 4])) + (e21 * Vn([e23; 4]))) + (e28 * Vn([e30; 4]))) * Vn([(1.0f32 / f32::max(((e2 + e4) + e7), 1e-5f32)); 4]));
    }

    pub fn k_motion(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = at1(c, a0.0[0], a0.0[1]);
        let e10 = (Vn([e3.0[0], e3.0[1], e3.0[2]]) - Vn([e6.0[0], e6.0[1], e6.0[2]])).map(f32::abs);
        let e17 = pf(c, 1u32);
        let e20 = pf(c, 0u32);
        let e24 = clampf(((f32::max(f32::max(e10.0[0], e10.0[1]), e10.0[2]) - e17) * e20), 0.0f32, 1.0f32);
        let e26 = pf3(c, 2u32);
        let e27 = straight(c, e3);
        let e29 = pf(c, 5u32);
        let e30 = mix3(c, e26, e27, e29);
        let e32 = straight(c, e3);
        let e34 = pf(c, 6u32);
        let e41 = (((e30 * Vn([e24; 3])) + ((e32 * Vn([e34; 3])) * Vn([(1.0f32 - e24); 3]))) * Vn([e3.0[3]; 3]));
        return Vn([e41.0[0], e41.0[1], e41.0[2], e3.0[3]]);
    }

    pub fn k_repair(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = at1(c, a0.0[0], a0.0[1]);
        let e9 = at2(c, a0.0[0], a0.0[1]);
        let e15 = pf(c, 0u32);
        let e16 = mix4(c, e3, e3.zip(e6, f32::min).zip(e3.zip(e6, f32::max).zip(e9, f32::min), f32::max), e15);
        return e16;
    }

    pub fn k_rgb_time(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = at1(c, a0.0[0], a0.0[1]);
        let e9 = at2(c, a0.0[0], a0.0[1]);
        return Vn([e3.0[0], e6.0[1], e9.0[2], f32::max(f32::max(e3.0[3], e6.0[3]), e9.0[3])]);
    }

    pub fn k_slit(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let e1 = pos(c, a0);
        let e3 = pf(c, 2u32);
        let e5 = turn(c, e1, (-e3));
        let e9 = c.p.shape;
        v0 = ((e5.0[0] / f32::max(e9, 1.0f32)) + 0.5f32);
        let e19 = c.p.i[0];
        if (e19 == 1i32) {
            let e23 = pf2(c, 4u32);
            v0 = ((e1 - e23).length() * 1.3f32);
        } else {
            let e31 = c.p.i[0];
            if (e31 == 2i32) {
                let e36 = at2(c, a0.0[0], a0.0[1]);
                let e38 = luma(c, Vn([e36.0[0], e36.0[1], e36.0[2]]));
                v0 = clampf(e38, 0.0f32, 1.0f32);
            } else {
                let e45 = c.p.i[0];
                if (e45 == 3i32) {
                    let e49 = (e1 * Vn([2.0f32; 2]));
                    let e55 = c.p.seed;
                    let e56 = fbm(c, Vn([e49.0[0], e49.0[1], 0.0f32]), 3i32, e55);
                    v0 = (0.5f32 + (e56 * 0.8f32));
                }
            }
        }
        let e61 = v0;
        let e66 = pf(c, 0u32);
        let e69 = pf(c, 1u32);
        let e74 = pf(c, 3u32);
        let e78 = (f32::max(e74, 0.02f32) * 0.5f32);
        let e83 = ss(c, (0.5f32 - e78), (0.5f32 + e78), ((clampf(e61, 0.0f32, 1.0f32) - e66) / f32::max(e69, 0.0001f32)));
        let e86 = at0(c, a0.0[0], a0.0[1]);
        let e89 = at1(c, a0.0[0], a0.0[1]);
        let e90 = mix4(c, e86, e89, e83);
        return e90;
    }

    pub fn k_steady(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 3> = Vn([0.0f32; 3]);
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = at1(c, 0i32, 0i32);
        let e7 = Vn([e6.0[0], e6.0[1], e6.0[2]]);
        let e10 = at2(c, 0i32, 0i32);
        let e11 = Vn([e10.0[0], e10.0[1], e10.0[2]]);
        let e12 = luma(c, e11);
        let e13 = luma(c, e7);
        v0 = Vn([(e12 / f32::max(e13, 0.001f32)); 3]);
        let e22 = c.p.i[0];
        if (e22 == 1i32) {
            v0 = (e11 / e7.zip(Vn([0.001f32; 3]), f32::max));
        }
        let e29 = v0;
        v0 = e29.zip3(Vn([0.4f32; 3]), Vn([2.5f32; 3]), clampf);
        let e38 = v0;
        let e40 = pf(c, 0u32);
        let e41 = mix3(c, Vn([1.0f32; 3]), e38, e40);
        let e42 = (Vn([e3.0[0], e3.0[1], e3.0[2]]) * e41);
        return Vn([e42.0[0], e42.0[1], e42.0[2], e3.0[3]]);
    }

    pub fn k_trail(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e6 = at1(c, a0.0[0], a0.0[1]);
        let e8 = pf(c, 4u32);
        let e12 = pf(c, 4u32);
        let e16 = luma(c, Vn([e6.0[0], e6.0[1], e6.0[2]]));
        let e18 = pf(c, 4u32);
        let e22 = ss(c, (e8 - 0.08f32), (e12 + 0.08f32), (e16 + step(e18, 0.0f32)));
        let e25 = pf3(c, 1u32);
        let e26 = (Vn([e6.0[0], e6.0[1], e6.0[2]]) * e25);
        let e30 = pf(c, 0u32);
        let e32 = (Vn([e26.0[0], e26.0[1], e26.0[2], e6.0[3]]) * Vn([(e30 * e22); 4]));
        let e36 = c.p.i[0];
        if (e36 == 1i32) {
            let e41 = (Vn([e3.0[0], e3.0[1], e3.0[2]]) + Vn([e32.0[0], e32.0[1], e32.0[2]]));
            return Vn([e41.0[0], e41.0[1], e41.0[2], (e3.0[3] + (e32.0[3] * (1.0f32 - e3.0[3])))]);
        }
        let e53 = c.p.i[0];
        if (e53 == 2i32) {
            let e68 = (Vn([e3.0[0], e3.0[1], e3.0[2]]) + (Vn([e32.0[0], e32.0[1], e32.0[2]]) * (Vn([1.0f32; 3]) - Vn([e3.0[0], e3.0[1], e3.0[2]])).zip3(Vn([0.0f32; 3]), Vn([1.0f32; 3]), clampf)));
            return Vn([e68.0[0], e68.0[1], e68.0[2], (e3.0[3] + (e32.0[3] * (1.0f32 - e3.0[3])))]);
        }
        let e80 = c.p.i[0];
        if (e80 == 3i32) {
            return (e3 + (e32 * Vn([(1.0f32 - e3.0[3]); 4])));
        }
        let e91 = c.p.i[0];
        if (e91 == 4i32) {
            let e96 = pf3(c, 1u32);
            let e97 = (Vn([e6.0[0], e6.0[1], e6.0[2]]) * e96);
            let e101 = pf(c, 0u32);
            let e102 = mix4(c, e3, Vn([e97.0[0], e97.0[1], e97.0[2], e6.0[3]]), e101);
            let e105 = Vn([e3.0[0], e3.0[1], e3.0[2]]).zip(Vn([e102.0[0], e102.0[1], e102.0[2]]), f32::min);
            return Vn([e105.0[0], e105.0[1], e105.0[2], f32::max(e3.0[3], e32.0[3])]);
        }
        return e3.zip(e32, f32::max);
    }

}

mod wipe {
    use super::shared::*;
    use super::*;
    pub fn k_burn(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 4> = Vn([0.0f32; 4]);
        let e1 = pos(c, a0);
        let e3 = pf(c, 1u32);
        let e7 = (e1 / Vn([f32::max(e3, 0.001f32); 2]));
        let e13 = c.p.seed;
        let e14 = fbm(c, Vn([e7.0[0], e7.0[1], 0.0f32]), 5i32, e13);
        let e17 = c.p.shape;
        let e19 = pf(c, 9u32);
        let e24 = pf(c, 9u32);
        let e31 = pf(c, 9u32);
        let e33 = turn(c, e1, (-e31));
        let e48 = pf(c, 10u32);
        let e49 = mix1(c, (0.5f32 + (e14 * 0.75f32)), ((((e33.0[0] / (0.5f32 * ((e17 * f32::abs(f32::cos(e19))) + f32::abs(f32::sin(e24))))) + 1.0f32) * 0.5f32) + (e14 * 0.28f32)), e48);
        let e51 = pf(c, 2u32);
        let e53 = f32::max(e51, 0.001f32);
        let e55 = pf(c, 0u32);
        let e66 = ((((e55 * (1.4f32 + (e53 * 4.0f32))) - 0.2f32) - (e53 * 2.0f32)) - e49);
        let e71 = sq(c, ((e66 + e53) / (e53 * 2.5f32)));
        let e75 = pf(c, 11u32);
        let e78 = (e1 * Vn([14.0f32; 2]));
        let e80 = pf(c, 0u32);
        let e86 = c.p.seed;
        let e89 = noise3(c, Vn([e78.0[0], e78.0[1], (e80 * 9.0f32)]), e86.wrapping_add(1u32));
        let e95 = ((e1 * Vn([14.0f32; 2])) + Vn([3.0f32, 8.0f32]));
        let e97 = pf(c, 0u32);
        let e103 = c.p.seed;
        let e106 = noise3(c, Vn([e95.0[0], e95.0[1], (e97 * 9.0f32)]), e103.wrapping_add(2u32));
        let e113 = pic0(c, (e1 + ((Vn([e89, e106]) * Vn([(f32::exp((-e71)) * e75); 2])) * Vn([0.02f32; 2]))), 1i32);
        let e116 = at1(c, a0.0[0], a0.0[1]);
        let e123 = ss(c, ((-e53) * 2.6f32), ((-e53) * 0.3f32), e66);
        let e129 = ss(c, ((-e53) * 0.15f32), (e53 * 0.15f32), e66);
        let e131 = pf3(c, 6u32);
        let e138 = mix4(c, e113, Vn([e131.0[0], e131.0[1], e131.0[2], f32::max(e113.0[3], 0.0f32)]), (e123 * 0.92f32));
        v0 = e138;
        let e146 = sq(c, ((e66 + (e53 * 0.35f32)) / (e53 * 0.5f32)));
        let e150 = (e1 * Vn([30.0f32; 2]));
        let e152 = pf(c, 0u32);
        let e158 = c.p.seed;
        let e161 = noise3(c, Vn([e150.0[0], e150.0[1], (e152 * 14.0f32)]), e158.wrapping_add(4u32));
        let e167 = v0;
        let e169 = pf3(c, 3u32);
        let e176 = lit(c, e167, ((e169 * Vn([f32::max((f32::exp((-e146)) * (0.7f32 + (0.6f32 * e161))), 0.0f32); 3])) * Vn([1.8f32; 3])), 0i32);
        v0 = e176;
        let e177 = v0;
        let e178 = mix4(c, e177, e116, e129);
        let e190 = pf3(c, 3u32);
        let e193 = lit(c, e178, (e190 * Vn([((f32::exp((((-f32::max(e66, 0.0f32)) / e53) * 1.4f32)) * e129) * 0.5f32); 3])), 0i32);
        return e193;
    }

    pub fn k_card(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 4> = Vn([0.0f32; 4]);
        let mut v1: i32 = 0i32;
        let mut v2: i32 = 0i32;
        let mut v3: Vn<f32, 4> = Vn([0.0f32; 4]);
        'l0: loop {
            'b0: {
                let e6 = v1;
                if (e6 < 2i32) {
                } else {
                    break 'l0;
                }
                v2 = 0i32;
                'l1: loop {
                    'b1: {
                        let e11 = v2;
                        if (e11 < 2i32) {
                        } else {
                            break 'l1;
                        }
                        let e14 = v2;
                        let e16 = v1;
                        let e27 = pos(c, a0);
                        let e28 = px(c);
                        let e30 = (e27 + ((((Vn([(e14 as f32), (e16 as f32)]) + Vn([0.5f32; 2])) * Vn([0.5f32; 2])) - Vn([0.5f32; 2])) * Vn([e28; 2])));
                        let e32 = pf(c, 6u32);
                        let e36 = pf(c, 7u32);
                        let e41 = pf(c, 8u32);
                        let e42 = (((e32 * e30.0[0]) + (e36 * e30.0[1])) + e41);
                        if (e42 > 1e-5f32) {
                            let e46 = pf(c, 0u32);
                            let e50 = pf(c, 1u32);
                            let e55 = pf(c, 2u32);
                            let e58 = pf(c, 3u32);
                            let e62 = pf(c, 4u32);
                            let e67 = pf(c, 5u32);
                            let e71 = (Vn([(((e46 * e30.0[0]) + (e50 * e30.0[1])) + e55), (((e58 * e30.0[0]) + (e62 * e30.0[1])) + e67)]) / Vn([e42; 2]));
                            let e75 = c.p.i[0];
                            if (e75 == 1i32) {
                                let e78 = v0;
                                let e80 = pic1(c, e71, 0i32);
                                v0 = (e78 + e80);
                            } else {
                                let e82 = v0;
                                let e84 = pic0(c, e71, 0i32);
                                v0 = (e82 + e84);
                            }
                        }
                    }
                    let e87 = v2;
                    v2 = e87.wrapping_add(1i32);
                }
            }
            let e90 = v1;
            v1 = e90.wrapping_add(1i32);
        }
        let e92 = v0;
        let e95 = pf(c, 9u32);
        let e98 = (Vn([e92.0[0], e92.0[1], e92.0[2]]) * Vn([(0.25f32 * e95); 3]));
        let e100 = v0.0[3];
        let e103 = Vn([e98.0[0], e98.0[1], e98.0[2], (e100 * 0.25f32)]);
        let e107 = c.p.i[1];
        if (e107 == 2i32) {
            return e103;
        }
        let e112 = at1(c, a0.0[0], a0.0[1]);
        v3 = e112;
        let e117 = c.p.i[0];
        if (e117 == 1i32) {
            let e122 = at0(c, a0.0[0], a0.0[1]);
            v3 = e122;
        }
        let e123 = v3;
        let e126 = pf(c, 10u32);
        let e129 = (Vn([e123.0[0], e123.0[1], e123.0[2]]) * Vn([(1.0f32 - e126); 3]));
        let e131 = v3.0[3];
        return (e103 + (Vn([e129.0[0], e129.0[1], e129.0[2], e131]) * Vn([(1.0f32 - e103.0[3]); 4])));
    }

    pub fn k_doors(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let mut v1: f32 = 0.0f32;
        let mut v2: f32 = 1.0f32;
        let mut v3: bool = false;
        let e1 = pos(c, a0);
        let e3 = pf(c, 1u32);
        let e5 = turn(c, e1, (-e3));
        let e8 = c.p.shape;
        let e10 = pf(c, 1u32);
        let e15 = pf(c, 1u32);
        let e20 = (0.5f32 * ((e8 * f32::abs(f32::cos(e10))) + f32::abs(f32::sin(e15))));
        let e22 = pf(c, 0u32);
        v0 = f32::abs(e5.0[0]);
        v1 = e5.0[1];
        let e37 = c.p.i[1];
        if (e37 == 0i32) {
            let e40 = v0;
            v0 = (e40 - (e20 * e22));
        } else {
            let e46 = c.p.i[1];
            if (e46 == 2i32) {
                let e49 = v0;
                v0 = (e20 - ((e20 - e49) / f32::max((1.0f32 - e22), 0.001f32)));
            } else {
                let e60 = ((e22 * 3.1415927f32) * 0.5f32);
                let e61 = v0;
                let e62 = (e20 - e61);
                let e68 = (f32::cos(e60) - ((e62 * f32::sin(e60)) * 0.8f32));
                let e71 = (e62 / f32::max(e68, 0.001f32));
                if (!(e68 <= 0.001f32)) {
                    v3 = (e71 > e20);
                } else {
                    v3 = true;
                }
                let e79 = v3;
                if e79 {
                    let e82 = at1(c, a0.0[0], a0.0[1]);
                    return e82;
                }
                v1 = (e5.0[1] * (1.0f32 + ((e71 * f32::sin(e60)) * 0.8f32)));
                v0 = (e20 - e71);
                let e93 = pf(c, 2u32);
                v2 = (1.0f32 - ((e93 * e22) * 0.8f32));
            }
        }
        let e99 = v0;
        if (e99 < 0.0f32) {
            let e104 = at1(c, a0.0[0], a0.0[1]);
            return e104;
        }
        let e105 = v0;
        let e107 = v1;
        let e110 = pf(c, 1u32);
        let e111 = turn(c, Vn([(signf((e5.0[0] + 1e-6f32)) * e105), e107]), e110);
        let e113 = pic0(c, e111, 0i32);
        let e114 = v0;
        let e115 = px(c);
        let e123 = v2;
        let e124 = (Vn([e113.0[0], e113.0[1], e113.0[2]]) * Vn([e123; 3]));
        let e127 = (Vn([e124.0[0], e124.0[1], e124.0[2], e113.0[3]]) * Vn([clampf(((e114 / e115) + 0.5f32), 0.0f32, 1.0f32); 4]));
        let e130 = at1(c, a0.0[0], a0.0[1]);
        return (e127 + (e130 * Vn([(1.0f32 - e127.0[3]); 4])));
    }

    pub fn k_flip_tiles(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: i32 = 0i32;
        let mut v1: Vn<f32, 2> = Vn([0.0f32; 2]);
        let mut v2: Vn<f32, 4> = Vn([0.0f32; 4]);
        let e2 = pf(c, 1u32);
        let e6 = (1.0f32 / f32::max(e2, 1.0f32));
        let e7 = pos(c, a0);
        let e9 = (e7 / Vn([e6; 2]));
        let e10 = e9.map(f32::floor);
        let e14 = ((e9 - e10) - Vn([0.5f32; 2]));
        let e18 = ((e10 + Vn([0.5f32; 2])) * Vn([e6; 2]));
        let e25 = c.p.seed;
        let e26 = rnd(c, (e10.0[0] as i32), (e10.0[1] as i32), e25);
        let e29 = c.p.shape;
        let e31 = pf(c, 3u32);
        let e36 = pf(c, 3u32);
        let e44 = pf(c, 3u32);
        let e46 = turn(c, e18, (-e44));
        let e54 = pf(c, 2u32);
        let e55 = mix1(c, (((e46.0[0] / ((0.5f32 * ((e29 * f32::abs(f32::cos(e31))) + f32::abs(f32::sin(e36)))) + e6)) + 1.0f32) * 0.5f32), e26, e54);
        let e57 = pf(c, 0u32);
        let e65 = clampf(((e57 * 2.2f32) - (e55 * 1.2f32)), 0.0f32, 1.0f32);
        let e73 = (((e65 * e65) * (3.0f32 - (2.0f32 * e65))) * 3.1415927f32);
        let e77 = c.p.i[1];
        v0 = e77;
        let e79 = v0;
        if (e79 == 2i32) {
            let e88 = c.p.seed;
            let e91 = rnd(c, (e10.0[0] as i32), (e10.0[1] as i32), e88.wrapping_add(3u32));
            v0 = (step(0.5f32, e91) as i32);
        }
        let e95 = f32::cos(e73);
        let e96 = f32::sin(e73);
        v1 = e14;
        let e98 = v0;
        if (e98 == 0i32) {
            v1 = Vn([(e14.0[0] / f32::max(f32::abs(e95), 0.001f32)), (e14.0[1] * (1.0f32 + ((e96 * e14.0[0]) * 0.5f32)))]);
        } else {
            v1 = Vn([(e14.0[0] * (1.0f32 + ((e96 * e14.0[1]) * 0.5f32))), (e14.0[1] / f32::max(f32::abs(e95), 0.001f32))]);
        }
        let e130 = v1.0[0];
        let e133 = v1.0[1];
        let e138 = px(c);
        let e146 = v1;
        let e148 = (e18 + (e146 * Vn([e6; 2])));
        let e150 = pic0(c, e148, 1i32);
        v2 = e150;
        if (e95 < 0.0f32) {
            let e155 = pic1(c, e148, 1i32);
            v2 = e155;
        }
        let e167 = mix1(c, (0.45f32 + (0.55f32 * f32::abs(e95))), 1.0f32, (step(e65, 0.0f32) + step(1.0f32, e65)));
        let e169 = pf(c, 4u32);
        let e170 = Vn([e169; 3]);
        let e173 = v2;
        let e175 = (Vn([e173.0[0], e173.0[1], e173.0[2]]) * Vn([e167; 3]));
        let e177 = v2.0[3];
        let e179 = mix4(c, Vn([e170.0[0], e170.0[1], e170.0[2], 1.0f32]), Vn([e175.0[0], e175.0[1], e175.0[2], e177]), clampf((((0.5f32 - f32::max(f32::abs(e130), f32::abs(e133))) / (e138 / e6)) + 0.5f32), 0.0f32, 1.0f32));
        return e179;
    }

    pub fn k_slide(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 4> = Vn([0.0f32; 4]);
        let e1 = pos(c, a0);
        let e3 = pf2(c, 1u32);
        let e6 = c.p.shape;
        let e12 = ((e6 * f32::abs(e3.0[0])) + f32::abs(e3.0[1]));
        let e14 = pf(c, 0u32);
        let e16 = pf(c, 3u32);
        let e17 = (e12 + e16);
        let e21 = c.p.i[1];
        if (e21 == 1i32) {
            let e30 = pic1(c, (e1 + (e3 * Vn([(e12 * (1.0f32 - e14)); 2]))), 0i32);
            let e33 = at0(c, a0.0[0], a0.0[1]);
            return (e30 + (e33 * Vn([(1.0f32 - e30.0[3]); 4])));
        }
        let e42 = c.p.i[1];
        if (e42 == 2i32) {
            let e49 = pic0(c, (e1 - (e3 * Vn([(e12 * e14); 2]))), 0i32);
            let e52 = at1(c, a0.0[0], a0.0[1]);
            return (e49 + (e52 * Vn([(1.0f32 - e49.0[3]); 4])));
        }
        let e64 = c.p.i[0];
        let e65 = pic0(c, (e1 - (e3 * Vn([(e17 * e14); 2]))), e64);
        let e74 = c.p.i[0];
        let e75 = pic1(c, (e1 + (e3 * Vn([(e17 * (1.0f32 - e14)); 2]))), e74);
        let e82 = (e1.dot(e3) - (((e17 * e14) - (e12 * 0.5f32)) - e16));
        let e86 = px(c);
        let e93 = mix4(c, e75, e65, clampf((((e82 - (e16 * 0.5f32)) / e86) + 0.5f32), 0.0f32, 1.0f32));
        v0 = e93;
        if (e16 > 0.0f32) {
            let e104 = px(c);
            let e111 = v0;
            let e113 = pf3(c, 4u32);
            let e116 = mix4(c, e111, Vn([e113.0[0], e113.0[1], e113.0[2], 1.0f32]), clampf(((((e16 * 0.5f32) - f32::abs((e82 - (e16 * 0.5f32)))) / e104) + 0.5f32), 0.0f32, 1.0f32));
            v0 = e116;
        }
        let e117 = v0;
        return e117;
    }

    pub fn k_strip_slide(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 1.0f32;
        let e1 = pos(c, a0);
        let e3 = pf(c, 2u32);
        let e5 = turn(c, e1, (-e3));
        let e7 = pf(c, 1u32);
        let e11 = (1.0f32 / f32::max(e7, 1.0f32));
        let e16 = f32::floor(((e5.0[1] / e11) + 0.0001f32));
        let e25 = c.p.seed;
        let e26 = rnd1(c, (e16 as i32), e25);
        let e30 = pf(c, 0u32);
        let e32 = pf(c, 3u32);
        let e37 = pf(c, 3u32);
        let e43 = ss(c, 0.0f32, 1.0f32, clampf(((e30 * (1.0f32 + e32)) - (e26 * e37)), 0.0f32, 1.0f32));
        let e46 = c.p.shape;
        let e48 = pf(c, 2u32);
        let e53 = pf(c, 2u32);
        let e60 = c.p.i[1];
        if (e60 == 3i32) {
            let e65 = f32::cos((e43 * 3.1415927f32));
            let e69 = ((((e5.0[1] / e11) - e16) - 0.5f32) / f32::max(f32::abs(e65), 0.001f32));
            if (f32::abs(e69) > 0.5f32) {
                return Vn([0.0f32, 0.0f32, 0.0f32, 1.0f32]);
            }
            let e85 = pf(c, 2u32);
            let e86 = turn(c, Vn([e5.0[0], (((e16 + 0.5f32) + e69) * e11)]), e85);
            let e91 = (0.35f32 + (0.65f32 * f32::abs(e65)));
            if (e65 > 0.0f32) {
                let e95 = pic0(c, e86, 1i32);
                let e97 = (Vn([e95.0[0], e95.0[1], e95.0[2]]) * Vn([e91; 3]));
                return Vn([e97.0[0], e97.0[1], e97.0[2], e95.0[3]]);
            }
            let e101 = pic1(c, e86, 1i32);
            let e103 = (Vn([e101.0[0], e101.0[1], e101.0[2]]) * Vn([e91; 3]));
            return Vn([e103.0[0], e103.0[1], e103.0[2], e101.0[3]]);
        }
        let e111 = c.p.i[1];
        if (e111 == 0i32) {
            let e115 = md(c, e16, 2.0f32);
            v0 = (1.0f32 - (2.0f32 * e115));
        } else {
            let e123 = c.p.i[1];
            if (e123 == 2i32) {
                v0 = signf((e26 - 0.5f32));
            }
        }
        let e130 = v0;
        let e137 = pf(c, 2u32);
        let e138 = turn(c, Vn([(e5.0[0] - ((e130 * ((e46 * f32::abs(f32::cos(e48))) + f32::abs(f32::sin(e53)))) * e43)), e5.0[1]]), e137);
        let e140 = pic0(c, e138, 0i32);
        let e143 = at1(c, a0.0[0], a0.0[1]);
        return (e140 + (e143 * Vn([(1.0f32 - e140.0[3]); 4])));
    }

    pub fn k_swap(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e2 = pf(c, 1u32);
        let e4 = pf(c, 2u32);
        let e7 = pf(c, 1u32);
        let e9 = pf(c, 2u32);
        let e14 = pf(c, 0u32);
        let e15 = ss(c, (e2 - e4), ((e7 + e9) + 0.0001f32), e14);
        let e18 = at0(c, a0.0[0], a0.0[1]);
        let e21 = at1(c, a0.0[0], a0.0[1]);
        let e22 = mix4(c, e18, e21, e15);
        return e22;
    }

    pub fn k_water(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e1 = pos(c, a0);
        let e3 = pf2(c, 1u32);
        let e4 = (e1 - e3);
        let e5 = e4.length();
        let e12 = c.p.shape;
        let e16 = pf(c, 1u32);
        let e20 = pf(c, 2u32);
        let e27 = pf(c, 5u32);
        let e32 = pf(c, 0u32);
        let e35 = pf(c, 5u32);
        let e36 = ((e32 * (Vn([((e12 * 0.5f32) + f32::abs(e16)), (0.5f32 + f32::abs(e20))]).length() + (e27 * 2.0f32))) - e35);
        let e39 = pf(c, 5u32);
        let e42 = ((e5 - e36) / f32::max(e39, 0.001f32));
        let e50 = pf(c, 4u32);
        let e59 = pf(c, 3u32);
        let e60 = ((f32::sin((((e5 - e36) / f32::max(e50, 0.001f32)) * 6.2831855f32)) * f32::exp((((-e42) * e42) * 2.0f32))) * e59);
        let e62 = (e1 + ((e4 / Vn([f32::max(e5, 1e-5f32); 2])) * Vn([e60; 2])));
        let e65 = ss(c, 0.35f32, -0.35f32, e42);
        let e67 = pic0(c, e62, 2i32);
        let e69 = pic1(c, e62, 2i32);
        let e70 = mix4(c, e67, e69, e65);
        let e74 = pf(c, 0u32);
        let e75 = ss(c, 0.0f32, 0.05f32, e74);
        let e79 = pf(c, 0u32);
        let e80 = ss(c, 1.0f32, 0.9f32, e79);
        let e84 = pf(c, 3u32);
        let e93 = (Vn([e70.0[0], e70.0[1], e70.0[2]]) * Vn([(1.0f32 + (((e60 / f32::max(e84, 0.0001f32)) * 0.22f32) * (e75 * e80))); 3]));
        return Vn([e93.0[0], e93.0[1], e93.0[2], e70.0[3]]);
    }

    pub fn k_wipe_bars(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let e1 = wipe_at(c, a0);
        let e3 = pf(c, 10u32);
        let e7 = (e1 / Vn([f32::max(e3, 0.001f32); 2]));
        let e10 = c.p.shape;
        let e12 = pf(c, 7u32);
        let e17 = pf(c, 7u32);
        let e23 = wipe_at(c, a0);
        let e34 = fr(c, e7.0[0]);
        v0 = e34;
        let e39 = c.p.i[1];
        if (e39 == 1i32) {
            let e48 = md(c, (f32::floor(e7.0[0]) + f32::floor(e7.0[1])), 2.0f32);
            let e50 = fr(c, e7.0[0]);
            v0 = ((e50 * 0.5f32) + (e48 * 0.5f32));
        } else {
            let e59 = c.p.i[1];
            if (e59 == 2i32) {
                let e63 = fr(c, e7.0[0]);
                let e65 = pf(c, 12u32);
                let e73 = mix1(c, e63, clampf((((e23.0[0] / (0.5f32 * ((e10 * f32::abs(f32::cos(e12))) + f32::abs(f32::sin(e17))))) + 1.0f32) * 0.5f32), 0.0f32, 1.0f32), ((clampf(e65, 0.0f32, 1.0f32) * 0.8f32) + 0.1f32));
                v0 = e73;
            } else {
                let e77 = c.p.i[1];
                if (e77 == 3i32) {
                    let e86 = md(c, (f32::floor(e7.0[0]) + f32::floor(e7.0[1])), 2.0f32);
                    let e88 = fr(c, e7.0[0]);
                    v0 = (f32::abs((e88 - 0.5f32)) * 2.0f32);
                    if (e86 > 0.5f32) {
                        let e97 = fr(c, e7.0[1]);
                        v0 = (f32::abs((e97 - 0.5f32)) * 2.0f32);
                    }
                } else {
                    let e106 = c.p.i[1];
                    if (e106 == 4i32) {
                        let e114 = c.p.seed;
                        let e115 = rnd1(c, (f32::floor(e7.0[0]) as i32), e114);
                        let e118 = wipe_at(c, a0);
                        v0 = ((e115 * 0.6f32) + (clampf((e118.0[1] + 0.5f32), 0.0f32, 1.0f32) * 0.4f32));
                    }
                }
            }
        }
        let e128 = v0;
        let e129 = wiped(c, a0, e128);
        return e129;
    }

    pub fn k_wipe_cells(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let e1 = wipe_at(c, a0);
        let e3 = pf(c, 10u32);
        let e10 = pf(c, 0u32);
        let e12 = pf(c, 13u32);
        let e16 = c.p.seed;
        let e17 = cells(c, (e1 / Vn([f32::max(e3, 0.001f32); 2])), 0.95f32, (e10 * e12), e16);
        v0 = clampf((e17.0[0] / 0.95f32), 0.0f32, 1.0f32);
        let e28 = c.p.i[1];
        if (e28 == 1i32) {
            v0 = clampf(((e17.0[1] - e17.0[0]) * 1.4f32), 0.0f32, 1.0f32);
        } else {
            let e42 = c.p.i[1];
            if (e42 == 2i32) {
                v0 = e17.0[2];
            } else {
                let e49 = c.p.i[1];
                if (e49 == 3i32) {
                    let e52 = wipe_at(c, a0);
                    let e54 = reach(c);
                    v0 = clampf(((((e52.length() / e54) * 0.7f32) + ((e17.0[1] - e17.0[0]) * 0.25f32)) + (e17.0[2] * 0.15f32)), 0.0f32, 1.0f32);
                }
            }
        }
        let e71 = v0;
        let e72 = wiped(c, a0, e71);
        return e72;
    }

    pub fn k_wipe_clock(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let e1 = wipe_at(c, a0);
        let e5 = c.p.i[1];
        let e13 = angle(c, Vn([(-e1.0[1]), e1.0[0]]));
        let e19 = fr(c, (((e13 / 6.2831855f32) * (i32_max(e5, 1i32) as f32)) + 1.0f32));
        v0 = e19;
        let e24 = c.p.i[0];
        if (e24 == 1i32) {
            let e27 = v0;
            v0 = f32::abs(((e27 * 2.0f32) - 1.0f32));
            let e33 = v0;
            v0 = (1.0f32 - e33);
        }
        let e39 = c.p.i[3];
        if (e39 == 1i32) {
            let e43 = pf(c, 14u32);
            let e45 = f32::max(e43, 1.0f32);
            let e47 = reach(c);
            let e50 = v0;
            let e55 = v0;
            v0 = ((f32::floor(((((e1.length() / e47) * e45) - e50) + 1.0f32)) + e55) / (e45 + 1.0f32));
            let e60 = v0;
            v0 = clampf(e60, 0.0f32, 1.0f32);
        }
        let e64 = v0;
        let e65 = wiped(c, a0, e64);
        return e65;
    }

    pub fn k_wipe_dots(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 2> = Vn([0.0f32; 2]);
        let mut v1: f32 = 0.0f32;
        let mut v2: Vn<f32, 2> = Vn([0.0f32; 2]);
        let mut v3: f32 = 0.0f32;
        let e2 = pf(c, 10u32);
        let e4 = f32::max(e2, 0.001f32);
        let e5 = wipe_at(c, a0);
        v0 = (e5 / Vn([e4; 2]));
        let e11 = v0;
        v2 = e11;
        let e16 = c.p.i[1];
        if (e16 == 2i32) {
            let e19 = v0;
            let e24 = c.p.seed;
            let e25 = cells(c, e19, 0.85f32, 0.0f32, e24);
            v1 = (e25.0[0] / 0.95f32);
        } else {
            let e32 = c.p.i[1];
            if (e32 == 1i32) {
                let e36 = v0.0[0];
                let e38 = v0.0[1];
                let e41 = md(c, f32::floor(e38), 2.0f32);
                let e46 = v0.0[1];
                v0 = Vn([(e36 + (0.5f32 * e41)), e46]);
            }
            let e48 = v0;
            v2 = (e48.map(f32::floor) + Vn([0.5f32; 2]));
            let e53 = v0;
            let e54 = v2;
            v1 = ((e53 - e54).length() / 0.72f32);
        }
        let e61 = c.p.shape;
        let e67 = v2.0[0];
        v3 = clampf(((((e67 * e4) / (0.5f32 * (e61 + 1.0f32))) * 0.5f32) + 0.5f32), 0.0f32, 1.0f32);
        let e81 = c.p.i[1];
        if (e81 == 3i32) {
            let e84 = v2;
            let e87 = reach(c);
            v3 = clampf(((e84.length() * e4) / e87), 0.0f32, 1.0f32);
        }
        let e93 = pf(c, 14u32);
        let e96 = clampf(e93, 0.0f32, 0.95f32);
        let e97 = v1;
        let e104 = v3;
        let e107 = wiped(c, a0, ((clampf(e97, 0.0f32, 1.0f32) * (1.0f32 - e96)) + (e104 * e96)));
        return e107;
    }

    pub fn k_wipe_line(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let e1 = pos(c, a0);
        let e3 = pf(c, 7u32);
        let e5 = turn(c, e1, (-e3));
        let e8 = c.p.shape;
        let e10 = pf(c, 7u32);
        let e15 = pf(c, 7u32);
        let e20 = (0.5f32 * ((e8 * f32::abs(f32::cos(e10))) + f32::abs(f32::sin(e15))));
        let e23 = c.p.shape;
        let e25 = pf(c, 7u32);
        let e30 = pf(c, 7u32);
        let e38 = pf(c, 10u32);
        let e43 = pf(c, 0u32);
        let e45 = pf(c, 13u32);
        let e50 = c.p.seed;
        let e51 = wander(c, ((e5.0[1] / f32::max(e38, 0.001f32)) + (e43 * e45)), e50);
        let e53 = pf(c, 12u32);
        v0 = (((e5.0[0] / e20) + 1.0f32) * 0.5f32);
        let e67 = c.p.i[1];
        if (e67 == 1i32) {
            v0 = (f32::abs(e5.0[0]) / e20);
        } else {
            let e76 = c.p.i[1];
            if (e76 == 2i32) {
                v0 = f32::max((((e5.0[0] / e20) + 1.0f32) * 0.5f32), (((e5.0[1] / (0.5f32 * ((e23 * f32::abs(f32::sin(e25))) + f32::abs(f32::cos(e30))))) + 1.0f32) * 0.5f32));
            }
        }
        let e92 = v0;
        let e94 = wiped(c, a0, (e92 + ((e51 * e53) * 0.25f32)));
        return e94;
    }

    pub fn k_wipe_luma(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let e3 = at0(c, a0.0[0], a0.0[1]);
        let e5 = luma(c, Vn([e3.0[0], e3.0[1], e3.0[2]]));
        v0 = e5;
        let e10 = c.p.i[1];
        if (e10 == 1i32) {
            let e15 = at1(c, a0.0[0], a0.0[1]);
            let e17 = luma(c, Vn([e15.0[0], e15.0[1], e15.0[2]]));
            v0 = e17;
        } else {
            let e21 = c.p.i[1];
            if (e21 == 2i32) {
                let e24 = v0;
                let e27 = at1(c, a0.0[0], a0.0[1]);
                let e29 = luma(c, Vn([e27.0[0], e27.0[1], e27.0[2]]));
                v0 = (0.5f32 * (e24 + e29));
            }
        }
        let e36 = c.p.i[0];
        if (e36 == 1i32) {
            let e41 = at2(c, a0.0[0], a0.0[1]);
            let e43 = luma(c, Vn([e41.0[0], e41.0[1], e41.0[2]]));
            v0 = e43;
        }
        let e44 = v0;
        let e50 = wiped(c, a0, (1.0f32 - clampf(e44, 0.0f32, 1.0f32)));
        return e50;
    }

    pub fn k_wipe_noise(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let e1 = wipe_at(c, a0);
        let e3 = pf(c, 10u32);
        let e7 = (e1 / Vn([f32::max(e3, 0.001f32); 2]));
        let e9 = pf(c, 0u32);
        let e11 = pf(c, 13u32);
        let e12 = (e9 * e11);
        let e18 = c.p.i[1];
        if (e18 == 1i32) {
            let e24 = c.p.seed;
            let e25 = noise3(c, Vn([e7.0[0], e7.0[1], e12]), e24);
            v0 = (e25 * 0.9f32);
        } else {
            let e31 = c.p.i[1];
            if (e31 == 2i32) {
                let e34 = full_pixel(c, a0);
                let e36 = pf(c, 10u32);
                let e41 = i32_max(((e36 * 120.0f32) as i32), 1i32);
                let e48 = c.p.seed;
                let e49 = rnd(c, idiv(e34.0[0], e41), idiv(e34.0[1], e41), e48);
                v0 = (e49 - 0.5f32);
            } else {
                let e55 = c.p.i[1];
                if (e55 == 3i32) {
                    let e59 = (e7 * Vn([0.8f32; 2]));
                    let e64 = c.p.seed;
                    let e67 = fbm(c, Vn([e59.0[0], e59.0[1], e12]), 3i32, e64.wrapping_add(1u32));
                    let e73 = ((e7 * Vn([0.8f32; 2])) + Vn([3.1f32, 7.4f32]));
                    let e78 = c.p.seed;
                    let e81 = fbm(c, Vn([e73.0[0], e73.0[1], e12]), 3i32, e78.wrapping_add(2u32));
                    let e85 = (e7 + (Vn([e67, e81]) * Vn([1.9f32; 2])));
                    let e90 = c.p.i[0];
                    let e93 = c.p.seed;
                    let e94 = fbm(c, Vn([e85.0[0], e85.0[1], e12]), e90, e93);
                    v0 = (e94 * 1.0f32);
                } else {
                    let e100 = c.p.i[1];
                    if (e100 == 4i32) {
                        let e107 = c.p.i[0];
                        let e110 = c.p.seed;
                        let e111 = ridges(c, Vn([e7.0[0], e7.0[1], e12]), e107, e110);
                        v0 = ((e111 - 0.6f32) * 1.1f32);
                    } else {
                        let e119 = c.p.i[1];
                        if (e119 == 5i32) {
                            let e131 = c.p.i[0];
                            let e134 = c.p.seed;
                            let e135 = fbm(c, Vn([e7.0[0], (e7.0[1] + (e12 * 2.0f32)), e12]), e131, e134);
                            let e140 = pf(c, 10u32);
                            v0 = ((e135 * 0.9f32) + ((e7.0[1] * f32::max(e140, 0.001f32)) * 0.35f32));
                        } else {
                            let e151 = c.p.i[0];
                            let e154 = c.p.seed;
                            let e155 = fbm(c, Vn([e7.0[0], e7.0[1], e12]), e151, e154);
                            v0 = (e155 * 0.95f32);
                        }
                    }
                }
            }
        }
        let e160 = c.p.shape;
        let e162 = pf(c, 7u32);
        let e167 = pf(c, 7u32);
        let e173 = wipe_at(c, a0);
        let e184 = pf(c, 14u32);
        let e188 = v0;
        let e193 = v0;
        let e197 = mix1(c, (0.5f32 + (e188 * 0.62f32)), (clampf((((e173.0[0] / (0.5f32 * ((e160 * f32::abs(f32::cos(e162))) + f32::abs(f32::sin(e167))))) + 1.0f32) * 0.5f32), 0.0f32, 1.0f32) + (e193 * 0.25f32)), clampf(e184, 0.0f32, 1.0f32));
        let e198 = wiped(c, a0, e197);
        return e198;
    }

    pub fn k_wipe_rings(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let e1 = wipe_at(c, a0);
        let e2 = e1.length();
        let e4 = pf(c, 10u32);
        let e9 = pf(c, 0u32);
        let e11 = pf(c, 13u32);
        let e14 = fr(c, ((e2 / f32::max(e4, 0.001f32)) - (e9 * e11)));
        let e15 = reach(c);
        let e16 = (e2 / e15);
        let e18 = pf(c, 12u32);
        let e21 = clampf(e18, 0.0f32, 1.0f32);
        let e25 = c.p.i[1];
        if (e25 == 1i32) {
            let e29 = pf(c, 10u32);
            let e44 = mix1(c, e16, (0.5f32 + (0.5f32 * f32::sin(((e2 / f32::max(e29, 0.001f32)) * 6.2831855f32)))), (0.25f32 + (e21 * 0.35f32)));
            let e45 = wiped(c, a0, e44);
            return e45;
        }
        let e50 = mix1(c, e14, e16, (0.35f32 + (e21 * 0.5f32)));
        let e51 = wiped(c, a0, e50);
        return e51;
    }

    pub fn k_wipe_shape(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: Vn<f32, 2> = Vn([0.0f32; 2]);
        let mut v1: f32 = 0.0f32;
        let mut v2: f32 = 0.0f32;
        let e1 = wipe_at(c, a0);
        v0 = e1;
        let e3 = v0;
        let e5 = pf(c, 0u32);
        let e7 = pf(c, 13u32);
        let e9 = turn(c, e3, (e5 * e7));
        v0 = e9;
        let e11 = v0.0[0];
        let e13 = pf(c, 11u32);
        let e18 = v0.0[1];
        v0 = Vn([(e11 / f32::max(e13, 0.05f32)), e18]);
        let e20 = v0;
        v1 = e20.length();
        let e23 = reach(c);
        let e25 = pf(c, 11u32);
        v2 = (e23 / f32::min(f32::max(e25, 0.05f32), 1.0f32));
        let e35 = c.p.i[1];
        if (e35 == 1i32) {
            let e39 = v0.0[0];
            let e42 = v0.0[1];
            v1 = f32::max(f32::abs(e39), f32::abs(e42));
        } else {
            let e48 = c.p.i[1];
            if (e48 == 2i32) {
                let e52 = v0.0[0];
                let e55 = v0.0[1];
                v1 = (f32::abs(e52) + f32::abs(e55));
                let e58 = v2;
                v2 = (e58 * 1.42f32);
            } else {
                let e64 = c.p.i[1];
                if (e64 == 3i32) {
                    let e68 = pf(c, 14u32);
                    let e75 = (3.1415927f32 / f32::max(f32::floor((e68 + 0.5f32)), 3.0f32));
                    let e77 = v0.0[1];
                    let e80 = v0.0[0];
                    let e82 = angle(c, Vn([(-e77), e80]));
                    let e93 = f32::abs((e82 - ((2.0f32 * e75) * f32::floor(((e82 / (2.0f32 * e75)) + 0.5f32)))));
                    let e95 = pf(c, 15u32);
                    let e97 = f32::max(e95, 0.1f32);
                    let e101 = (Vn([f32::cos(e75), f32::sin(e75)]) * Vn([e97; 2]));
                    let e105 = (e101 - Vn([1.0f32, 0.0f32]));
                    let e106 = v0;
                    v1 = (e106.length() / f32::max((e101.0[1] / f32::max(((f32::cos(e93) * e105.0[1]) - (f32::sin(e93) * e105.0[0])), 1e-5f32)), 0.0001f32));
                    let e122 = v2;
                    v2 = (e122 / e97);
                } else {
                    let e127 = c.p.i[1];
                    if (e127 == 4i32) {
                        let e131 = v0.0[0];
                        let e134 = v0.0[1];
                        let e138 = Vn([f32::abs(e131), ((-e134) + 0.04f32)]);
                        v1 = f32::sqrt(f32::max((((e138.0[0] * e138.0[0]) + (e138.0[1] * e138.0[1])) - ((1.0f32 * e138.0[0]) * e138.0[1])), 0.0f32));
                        let e155 = v2;
                        v2 = (e155 * 1.25f32);
                    } else {
                        let e161 = c.p.i[1];
                        if (e161 == 5i32) {
                            let e165 = pf(c, 14u32);
                            let e172 = v0.0[1];
                            let e175 = v0.0[0];
                            let e177 = angle(c, Vn([(-e172), e175]));
                            let e179 = (3.1415927f32 / f32::max(f32::floor((e165 + 0.5f32)), 3.0f32));
                            let e189 = v0;
                            v1 = ((e189.length() * f32::cos(((e177 - ((2.0f32 * e179) * f32::floor((e177 / (2.0f32 * e179))))) - e179))) / f32::cos(e179));
                            let e195 = v2;
                            v2 = (e195 / f32::cos(e179));
                        }
                    }
                }
            }
        }
        let e198 = v1;
        let e199 = v2;
        let e201 = wiped(c, a0, (e198 / e199));
        return e201;
    }

    pub fn k_wipe_tiles(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let e2 = pf(c, 10u32);
        let e4 = f32::max(e2, 0.001f32);
        let e6 = pf(c, 11u32);
        let e10 = Vn([(e4 * f32::max(e6, 0.05f32)), e4]);
        let e11 = pos(c, a0);
        let e12 = (e11 / e10);
        let e13 = e12.map(f32::floor);
        let e17 = ((e12 - e13) - Vn([0.5f32; 2]));
        let e24 = c.p.seed;
        let e25 = rnd(c, (e13.0[0] as i32), (e13.0[1] as i32), e24);
        let e29 = ((e13 + Vn([0.5f32; 2])) * e10);
        v0 = e25;
        let e34 = c.p.i[1];
        if (e34 == 1i32) {
            let e39 = c.p.shape;
            let e41 = pf(c, 7u32);
            let e46 = pf(c, 7u32);
            let e54 = pf(c, 7u32);
            let e56 = turn(c, e29, (-e54));
            let e64 = pf(c, 12u32);
            let e67 = mix1(c, (((e56.0[0] / ((0.5f32 * ((e39 * f32::abs(f32::cos(e41))) + f32::abs(f32::sin(e46)))) + e4)) + 1.0f32) * 0.5f32), e25, (e64 * 0.5f32));
            v0 = e67;
        } else {
            let e71 = c.p.i[1];
            if (e71 == 2i32) {
                let e75 = pf2(c, 8u32);
                let e78 = reach(c);
                let e81 = pf(c, 12u32);
                let e84 = mix1(c, ((e29 - e75).length() / e78), e25, (e81 * 0.5f32));
                v0 = e84;
            } else {
                let e88 = c.p.i[1];
                if (e88 == 3i32) {
                    let e92 = pf2(c, 8u32);
                    let e94 = angle(c, (e29 - e92));
                    let e99 = fr(c, ((e94 / 6.2831855f32) + 1.0f32));
                    let e101 = pf2(c, 8u32);
                    let e107 = fr(c, (e99 + ((e29 - e101).length() * 1.5f32)));
                    let e109 = pf(c, 12u32);
                    let e112 = mix1(c, e107, e25, (e109 * 0.5f32));
                    v0 = e112;
                }
            }
        }
        let e121 = pf(c, 14u32);
        let e124 = clampf(e121, 0.0f32, 0.9f32);
        let e125 = v0;
        let e131 = wiped(c, a0, ((e125 * (1.0f32 - e124)) + ((f32::max(f32::abs(e17.0[0]), f32::abs(e17.0[1])) * 2.0f32) * e124)));
        return e131;
    }

    pub fn k_wipe_waves(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let e1 = wipe_at(c, a0);
        let e3 = pf(c, 10u32);
        let e7 = (e1 / Vn([f32::max(e3, 0.001f32); 2]));
        let e9 = pf(c, 0u32);
        let e11 = pf(c, 13u32);
        let e12 = (e9 * e11);
        let e18 = c.p.i[1];
        if (e18 == 1i32) {
            v0 = (f32::sin((e7.length() * 6.2831855f32)) * f32::sin(((e7 + Vn([0.8f32, 0.3f32])).length() * 6.2831855f32)));
        } else {
            v0 = ((((f32::sin(((e7.0[0] * 2.1f32) + e12)) + f32::sin(((e7.0[1] * 1.7f32) - e12))) + f32::sin(((e7.0[0] + e7.0[1]) * 1.3f32))) + f32::sin(((e7.length() * 2.4f32) - e12))) * 0.25f32);
        }
        let e60 = v0;
        let e65 = wiped(c, a0, (0.5f32 + (0.5f32 * e60)));
        return e65;
    }

    #[inline]
    pub fn reach(c: &C<'_>) -> f32 {
        let e2 = c.p.shape;
        let e6 = pf(c, 8u32);
        let e10 = pf(c, 9u32);
        return Vn([((e2 * 0.5f32) + f32::abs(e6)), (0.5f32 + f32::abs(e10))]).length();
    }

    #[inline]
    pub fn wipe_at(c: &C<'_>, a0: Vn<i32, 2>) -> Vn<f32, 2> {
        let e1 = pos(c, a0);
        let e3 = pf2(c, 8u32);
        let e6 = pf(c, 7u32);
        let e8 = turn(c, (e1 - e3), (-e6));
        return e8;
    }

    #[inline]
    pub fn wiped(c: &C<'_>, a0: Vn<i32, 2>, a1: f32) -> Vn<f32, 4> {
        let mut v0: f32 = 0.0f32;
        let mut v1: Vn<f32, 4> = Vn([0.0f32; 4]);
        let e4 = at0(c, a0.0[0], a0.0[1]);
        let e7 = at1(c, a0.0[0], a0.0[1]);
        v0 = clampf(a1, 0.0f32, 1.0f32);
        let e15 = c.p.i[2];
        if (e15 == 1i32) {
            let e18 = v0;
            v0 = (1.0f32 - e18);
        }
        let e22 = pf(c, 1u32);
        let e24 = f32::max(e22, 0.003f32);
        let e26 = pf(c, 0u32);
        let e30 = v0;
        let e31 = ((e26 * (1.0f32 + e24)) - e30);
        let e33 = ss(c, 0.0f32, e24, e31);
        let e34 = mix4(c, e4, e7, e33);
        v1 = e34;
        let e37 = pf(c, 6u32);
        if (e37 > 0.0f32) {
            let e44 = pf(c, 2u32);
            let e47 = ((e31 - (e24 * 0.5f32)) / f32::max(e44, 0.001f32));
            let e51 = pf(c, 0u32);
            let e52 = ss(c, 0.0f32, 0.06f32, e51);
            let e56 = pf(c, 0u32);
            let e57 = ss(c, 1.0f32, 0.94f32, e56);
            let e59 = v1;
            let e61 = pf3(c, 3u32);
            let e68 = pf(c, 6u32);
            let e75 = lit(c, e59, (e61 * Vn([(((f32::exp((((-e47) * e47) * 3.0f32)) * e68) * (e52 * e57)) * 1.4f32); 3])), 0i32);
            v1 = e75;
        }
        let e76 = v1;
        return e76;
    }

}

