//! BananaV's effects in another program's OFX host: Natron (free, from
//! natrongithub.github.io), whose renderer runs a Python script. With
//! `BV_NATRON` naming Natron's `bin` folder, `cargo test -p bv-effects
//! --test natron -- --ignored` builds `BananaV.ofx`, puts it in a bundle of
//! a folder of its own and has `NatronRenderer` crop its checkerboard with
//! it (it found that hosts launch exactly the threads asked for), bend
//! a constant's colors with Color Curves, redden a grey's midtones with
//! Color Corrector, cut a circle with Cookie Cutter that Layer
//! Dimensionality gives a shadow, pixelate its checkerboard, swap a
//! constant's red and blue through LUT Filter and take a warm light's cast
//! away from a warm gray with White Balance.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

mod common;

/// Natron's script: a checkerboard, BananaV's Crop keeping its right half
/// (VEGAS's Crop Left Half: half the width, its middle at 0.75), written
/// as a PNG.
const SCRIPT: &str = r#"
import NatronEngine
app = NatronEngine.natron.getActiveInstance()
checker = app.createNode("net.sf.openfx.CheckerBoardPlugin")
crop = app.createNode("org.bananav.Crop")
crop.connectInput(0, checker)
crop.getParam("xCrop").setValue(0.5)
crop.getParam("location").setValue(0.75, 0)
crop.getParam("location").setValue(0.5, 1)
writer = app.createNode("fr.inria.openfx.WriteOIIO")
writer.connectInput(0, crop)
writer.getParam("filename").setValue("OUT/crop_#.png")
app.render(writer, 1, 1)
"#;

/// Natron's script: a white constant through BananaV's Color Curves with
/// its red curve flat at 0 (cyan comes out).
const CURVES_SCRIPT: &str = r#"
import NatronEngine
app = NatronEngine.natron.getActiveInstance()
white = app.createNode("net.sf.openfx.ConstantPlugin")
for i in range(4):
    white.getParam("color").setValue(1.0, i)
curves = app.createNode("org.bananav.ColorCurves")
curves.connectInput(0, white)
red = curves.getParam("curves")
red.deleteAllControlPoints(0)
red.addControlPoint(0, 0.0, 0.0)
red.addControlPoint(0, 1.0, 0.0)
writer = app.createNode("fr.inria.openfx.WriteOIIO")
writer.connectInput(0, curves)
writer.getParam("filename").setValue("OUT/curves_#.png")
app.render(writer, 1, 1)
"#;

/// Natron's script: a white constant through BananaV's Bézier Masking, its
/// default mask: the rectangle in the middle, half the frame each way.
const MASK_SCRIPT: &str = r#"
import NatronEngine
app = NatronEngine.natron.getActiveInstance()
white = app.createNode("net.sf.openfx.ConstantPlugin")
for i in range(4):
    white.getParam("color").setValue(1.0, i)
mask = app.createNode("org.bananav.BezierMasking")
mask.connectInput(0, white)
writer = app.createNode("fr.inria.openfx.WriteOIIO")
writer.connectInput(0, mask)
writer.getParam("filename").setValue("OUT/mask_#.png")
app.render(writer, 1, 1)
"#;

/// Natron's script: a middle grey through BananaV's Color Corrector with
/// VEGAS's Red Midtones (its Mid wheel at red, half way out). Natron has
/// no draw suite: the wheel is the two numbers.
const CORRECTOR_SCRIPT: &str = r#"
import NatronEngine
app = NatronEngine.natron.getActiveInstance()
grey = app.createNode("net.sf.openfx.ConstantPlugin")
for i in range(3):
    grey.getParam("color").setValue(0.5, i)
grey.getParam("color").setValue(1.0, 3)
cc = app.createNode("org.bananav.ColorCorrector")
cc.connectInput(0, grey)
cc.getParam("mid").setValue(103.5, 0)
cc.getParam("mid").setValue(0.5, 1)
writer = app.createNode("fr.inria.openfx.WriteOIIO")
writer.connectInput(0, cc)
writer.getParam("filename").setValue("OUT/corrector_#.png")
app.render(writer, 1, 1)
"#;

/// Natron's script: a white constant cut by BananaV's Cookie Cutter (its
/// circle) and given a shadow by its Layer Dimensionality, dropped away
/// from the light, far.
const LAYER_SCRIPT: &str = r#"
import NatronEngine
app = NatronEngine.natron.getActiveInstance()
white = app.createNode("net.sf.openfx.ConstantPlugin")
for i in range(4):
    white.getParam("color").setValue(1.0, i)
cut = app.createNode("org.bananav.CookieCutter")
cut.connectInput(0, white)
layer = app.createNode("org.bananav.LayerDimensionality")
layer.connectInput(0, cut)
layer.getParam("shadowType").setValue(1)
layer.getParam("shadowHeight").setValue(0.5)
writer = app.createNode("fr.inria.openfx.WriteOIIO")
writer.connectInput(0, layer)
writer.getParam("filename").setValue("OUT/layer_#.png")
app.render(writer, 1, 1)
"#;

/// Natron's script: its checkerboard (squares 64 pixels big) through
/// BananaV's Pixelate, nearly all the way each way: cells far bigger than
/// the squares, each of one color.
const PIXELATE_SCRIPT: &str = r#"
import NatronEngine
app = NatronEngine.natron.getActiveInstance()
checker = app.createNode("net.sf.openfx.CheckerBoardPlugin")
pix = app.createNode("org.bananav.Pixelate")
pix.connectInput(0, checker)
pix.getParam("horizontal").setValue(0.99)
pix.getParam("vertical").setValue(0.99)
writer = app.createNode("fr.inria.openfx.WriteOIIO")
writer.connectInput(0, pix)
writer.getParam("filename").setValue("OUT/pixelate_#.png")
app.render(writer, 1, 1)
"#;

/// Natron's script: a red constant through BananaV's LUT Filter with a
/// table that swaps red and blue (`LUTFILE`).
const LUT_SCRIPT: &str = r#"
import NatronEngine
app = NatronEngine.natron.getActiveInstance()
red = app.createNode("net.sf.openfx.ConstantPlugin")
for i, v in enumerate([1.0, 0.0, 0.0, 1.0]):
    red.getParam("color").setValue(v, i)
lut = app.createNode("org.bananav.LutFilter")
lut.connectInput(0, red)
lut.getParam("file").setValue("LUTFILE")
writer = app.createNode("fr.inria.openfx.WriteOIIO")
writer.connectInput(0, lut)
writer.getParam("filename").setValue("OUT/lut_#.png")
app.render(writer, 1, 1)
"#;

/// Natron's script: a warm gray through BananaV's White Balance, set as
/// picking that same color sets it (its temperature, tint and amount).
const WHITE_BALANCE_SCRIPT: &str = r#"
import NatronEngine
app = NatronEngine.natron.getActiveInstance()
warm = app.createNode("net.sf.openfx.ConstantPlugin")
for i, v in enumerate([0.6, 0.45, 0.3, 1.0]):
    warm.getParam("color").setValue(v, i)
wb = app.createNode("org.bananav.WhiteBalance")
wb.connectInput(0, warm)
for i, v in enumerate([0.6, 0.45, 0.3]):
    wb.getParam("neutral").setValue(v, i)
wb.getParam("temperature").setValue(3143.843224597883)
wb.getParam("tint").setValue(-33.12102139343967)
wb.getParam("amount").setValue(0.5261149522623656)
writer = app.createNode("fr.inria.openfx.WriteOIIO")
writer.connectInput(0, wb)
writer.getParam("filename").setValue("OUT/wb_#.png")
app.render(writer, 1, 1)
"#;

/// Natron's script: an orange frame with a blue rectangle through
/// BananaV's White Balance with Equalize Histogram (each channel stretched
/// over the frame, red and green held under blue).
const EQUALIZE_SCRIPT: &str = r#"
import NatronEngine
app = NatronEngine.natron.getActiveInstance()
rect = app.createNode("net.sf.openfx.Rectangle")
for i, v in enumerate([0.9, 0.5, 0.1, 1.0]):
    rect.getParam("color0").setValue(v, i)
for i, v in enumerate([0.2, 0.4, 0.9, 1.0]):
    rect.getParam("color1").setValue(v, i)
rect.getParam("softness").setValue(0.0)
rect.getParam("bottomLeft").setValue(480.0, 0)
rect.getParam("bottomLeft").setValue(270.0, 1)
rect.getParam("size").setValue(960.0, 0)
rect.getParam("size").setValue(540.0, 1)
wb = app.createNode("org.bananav.WhiteBalance")
wb.connectInput(0, rect)
wb.getParam("equalize").setValue(True)
writer = app.createNode("fr.inria.openfx.WriteOIIO")
writer.connectInput(0, wb)
writer.getParam("filename").setValue("OUT/equalize_#.png")
app.render(writer, 1, 1)
"#;

/// Natron's script: an orange through BananaV's HSL Adjust turned half
/// way round the hues.
const HSL_SCRIPT: &str = r#"
import NatronEngine
app = NatronEngine.natron.getActiveInstance()
orange = app.createNode("net.sf.openfx.ConstantPlugin")
for i, v in enumerate([0.9, 0.5, 0.1, 1.0]):
    orange.getParam("color").setValue(v, i)
hsl = app.createNode("org.bananav.HslAdjust")
hsl.connectInput(0, orange)
hsl.getParam("hue").setValue(0.5)
writer = app.createNode("fr.inria.openfx.WriteOIIO")
writer.connectInput(0, hsl)
writer.getParam("filename").setValue("OUT/hsl_#.png")
app.render(writer, 1, 1)
"#;

/// Natron's script: an orange frame through BananaV's Picture In Picture
/// at its defaults (half the frame, in its middle) and pinned in Free Form.
const PIP_SCRIPT: &str = r#"
import NatronEngine
app = NatronEngine.natron.getActiveInstance()
orange = app.createNode("net.sf.openfx.ConstantPlugin")
for i, v in enumerate([0.9, 0.5, 0.1, 1.0]):
    orange.getParam("color").setValue(v, i)
pip = app.createNode("org.bananav.PictureInPicture")
pip.connectInput(0, orange)
writer = app.createNode("fr.inria.openfx.WriteOIIO")
writer.connectInput(0, pip)
writer.getParam("filename").setValue("OUT/pip_#.png")
app.render(writer, 1, 1)
pip.getParam("mode").setValue(2)
pip.getParam("topRight").setValue(1.0, 0)
pip.getParam("topRight").setValue(1.0, 1)
writer.getParam("filename").setValue("OUT/pinned_#.png")
app.render(writer, 1, 1)
"#;

/// Natron's script: a strong sky blue through BananaV's Saturation Adjust
/// with High at -1 (the strongest colors gray).
const SATURATION_SCRIPT: &str = r#"
import NatronEngine
app = NatronEngine.natron.getActiveInstance()
sky = app.createNode("net.sf.openfx.ConstantPlugin")
for i, v in enumerate([0.0, 0.9, 1.0, 1.0]):
    sky.getParam("color").setValue(v, i)
saturation = app.createNode("org.bananav.SaturationAdjust")
saturation.connectInput(0, sky)
saturation.getParam("high").setValue(-1.0)
writer = app.createNode("fr.inria.openfx.WriteOIIO")
writer.connectInput(0, saturation)
writer.getParam("filename").setValue("OUT/saturation_#.png")
app.render(writer, 1, 1)
"#;

/// Natron's script: a middle grey through BananaV's Color Balance with
/// VEGAS's Red Midtones (red 0.1 in the middle tones, the luminosity kept).
const BALANCE_SCRIPT: &str = r#"
import NatronEngine
app = NatronEngine.natron.getActiveInstance()
grey = app.createNode("net.sf.openfx.ConstantPlugin")
for i, v in enumerate([0.5, 0.5, 0.5, 1.0]):
    grey.getParam("color").setValue(v, i)
balance = app.createNode("org.bananav.ColorBalance")
balance.connectInput(0, grey)
balance.getParam("red").setValue(0.1)
writer = app.createNode("fr.inria.openfx.WriteOIIO")
writer.connectInput(0, balance)
writer.getParam("filename").setValue("OUT/balance_#.png")
app.render(writer, 1, 1)
"#;

/// Natron's script: a red through BananaV's Color Corrector (Secondary)
/// with VEGAS's Invert Color (its hue turned half way round).
const SECONDARY_SCRIPT: &str = r#"
import NatronEngine
app = NatronEngine.natron.getActiveInstance()
red = app.createNode("net.sf.openfx.ConstantPlugin")
for i, v in enumerate([0.8, 0.3, 0.3, 1.0]):
    red.getParam("color").setValue(v, i)
secondary = app.createNode("org.bananav.ColorCorrectorSecondary")
secondary.connectInput(0, red)
secondary.getParam("rotateHue").setValue(180.0)
writer = app.createNode("fr.inria.openfx.WriteOIIO")
writer.connectInput(0, secondary)
writer.getParam("filename").setValue("OUT/secondary_#.png")
app.render(writer, 1, 1)
"#;

/// Natron's script: an orange through BananaV's Border, a solid white
/// frame a quarter of the half width wide.
const BORDER_SCRIPT: &str = r#"
import NatronEngine
app = NatronEngine.natron.getActiveInstance()
orange = app.createNode("net.sf.openfx.ConstantPlugin")
for i, v in enumerate([0.9, 0.5, 0.1, 1.0]):
    orange.getParam("color").setValue(v, i)
border = app.createNode("org.bananav.Border")
border.connectInput(0, orange)
border.getParam("type").setValue(2)
border.getParam("size").setValue(0.25)
writer = app.createNode("fr.inria.openfx.WriteOIIO")
writer.connectInput(0, border)
writer.getParam("filename").setValue("OUT/border_#.png")
app.render(writer, 1, 1)
"#;

/// Natron's script: a blue portrait strip (a third of the frame, the rest
/// transparent) through BananaV's Black Bar Fill.
const FILL_SCRIPT: &str = r#"
import NatronEngine
app = NatronEngine.natron.getActiveInstance()
rect = app.createNode("net.sf.openfx.Rectangle")
for i, v in enumerate([0.0, 0.0, 0.0, 0.0]):
    rect.getParam("color0").setValue(v, i)
for i, v in enumerate([0.2, 0.4, 0.9, 1.0]):
    rect.getParam("color1").setValue(v, i)
rect.getParam("softness").setValue(0.0)
rect.getParam("bottomLeft").setValue(640.0, 0)
rect.getParam("bottomLeft").setValue(0.0, 1)
rect.getParam("size").setValue(640.0, 0)
rect.getParam("size").setValue(1080.0, 1)
fill = app.createNode("org.bananav.BlackBarFill")
fill.connectInput(0, rect)
writer = app.createNode("fr.inria.openfx.WriteOIIO")
writer.connectInput(0, fill)
writer.getParam("filename").setValue("OUT/fill_#.png")
app.render(writer, 1, 1)
"#;

/// Natron's script: a dark gray frame, its right half lighter, through
/// BananaV's Unsharp Mask.
const UNSHARP_SCRIPT: &str = r#"
import NatronEngine
app = NatronEngine.natron.getActiveInstance()
rect = app.createNode("net.sf.openfx.Rectangle")
for i, v in enumerate([0.2, 0.2, 0.2, 1.0]):
    rect.getParam("color0").setValue(v, i)
for i, v in enumerate([0.6, 0.6, 0.6, 1.0]):
    rect.getParam("color1").setValue(v, i)
rect.getParam("softness").setValue(0.0)
rect.getParam("bottomLeft").setValue(960.0, 0)
rect.getParam("bottomLeft").setValue(0.0, 1)
rect.getParam("size").setValue(960.0, 0)
rect.getParam("size").setValue(1080.0, 1)
unsharp = app.createNode("org.bananav.UnsharpMask")
unsharp.connectInput(0, rect)
unsharp.getParam("amount").setValue(1.0)
unsharp.getParam("radius").setValue(0.05)
writer = app.createNode("fr.inria.openfx.WriteOIIO")
writer.connectInput(0, unsharp)
writer.getParam("filename").setValue("OUT/unsharp_#.png")
app.render(writer, 1, 1)
"#;

/// Natron's script: a gray frame through BananaV's Add Noise as it comes.
const NOISE_SCRIPT: &str = r#"
import NatronEngine
app = NatronEngine.natron.getActiveInstance()
gray = app.createNode("net.sf.openfx.ConstantPlugin")
for i, v in enumerate([0.2, 0.2, 0.2, 1.0]):
    gray.getParam("color").setValue(v, i)
noise = app.createNode("org.bananav.AddNoise")
noise.connectInput(0, gray)
writer = app.createNode("fr.inria.openfx.WriteOIIO")
writer.connectInput(0, noise)
writer.getParam("filename").setValue("OUT/noise_#.png")
app.render(writer, 1, 1)
"#;

/// Natron's script: a gray frame through BananaV's Timecode as it comes.
const TIMECODE_SCRIPT: &str = r#"
import NatronEngine
app = NatronEngine.natron.getActiveInstance()
gray = app.createNode("net.sf.openfx.ConstantPlugin")
for i, v in enumerate([0.2, 0.2, 0.2, 1.0]):
    gray.getParam("color").setValue(v, i)
timecode = app.createNode("org.bananav.Timecode")
timecode.connectInput(0, gray)
writer = app.createNode("fr.inria.openfx.WriteOIIO")
writer.connectInput(0, timecode)
writer.getParam("filename").setValue("OUT/timecode_#.png")
app.render(writer, 25, 25)
"#;

/// Runs Natron's renderer on `script` in `dir`, BananaV's bundle in a
/// folder of its own; the folder the frames went to.
fn natron(bin: &OsStr, dir: &Path, name: &str, script: &str) -> PathBuf {
    let plugins = dir.join("plugins");
    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    common::bundle_in(&plugins);
    let file = dir.join(format!("{name}.py"));
    let out_text = out.display().to_string().replace('\\', "/");
    std::fs::write(&file, script.replace("OUT", &out_text)).unwrap();
    let renderer = Path::new(bin).join(if cfg!(windows) {
        "NatronRenderer.exe"
    } else {
        "NatronRenderer"
    });
    let mut child = Command::new(renderer)
        .arg("-t")
        .arg(&file)
        .env("OFX_PLUGIN_PATH", &plugins)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Natron runs");
    let start = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > Duration::from_secs(300) {
            let _ = child.kill();
            panic!("Natron did not finish");
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    out
}

#[test]
#[ignore = "needs Natron (BV_NATRON: its bin folder)"]
fn natron_crops_with_bananavs_crop() {
    let Some(bin) = std::env::var_os("BV_NATRON") else {
        eprintln!("BV_NATRON is not set: nothing to run");
        return;
    };
    let dir = std::env::temp_dir().join(format!("bananav-natron-{}", std::process::id()));
    let out = natron(&bin, &dir, "crop", SCRIPT);
    let png = std::fs::read(out.join("crop_1.png")).expect("Natron wrote the frame");
    let img = bv_png::decode(&png).unwrap();
    let rgba = img.to_rgba8();
    let alpha = |x: u32, y: u32| rgba[((y * img.width + x) * 4 + 3) as usize];
    let (w, h) = (img.width, img.height);
    // The left half went, the right half stayed.
    assert_eq!(alpha(w / 4, h / 2), 0);
    assert_eq!(alpha(w * 3 / 4, h / 2), 255);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "needs Natron (BV_NATRON: its bin folder)"]
fn natron_masks_with_bananavs_bezier_masking() {
    let Some(bin) = std::env::var_os("BV_NATRON") else {
        eprintln!("BV_NATRON is not set: nothing to run");
        return;
    };
    let dir = std::env::temp_dir().join(format!("bananav-natron-mask-{}", std::process::id()));
    let out = natron(&bin, &dir, "mask", MASK_SCRIPT);
    let png = std::fs::read(out.join("mask_1.png")).expect("Natron wrote the frame");
    let img = bv_png::decode(&png).unwrap();
    let rgba = img.to_rgba8();
    let alpha = |x: u32, y: u32| rgba[((y * img.width + x) * 4 + 3) as usize];
    let (w, h) = (img.width, img.height);
    // The middle kept, the corners cut out.
    assert_eq!(alpha(w / 2, h / 2), 255);
    assert_eq!(alpha(w / 10, h / 10), 0);
    assert_eq!(alpha(w * 9 / 10, h * 9 / 10), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "needs Natron (BV_NATRON: its bin folder)"]
fn natron_bends_colors_with_bananavs_color_curves() {
    let Some(bin) = std::env::var_os("BV_NATRON") else {
        eprintln!("BV_NATRON is not set: nothing to run");
        return;
    };
    let dir = std::env::temp_dir().join(format!("bananav-natron-curves-{}", std::process::id()));
    let out = natron(&bin, &dir, "curves", CURVES_SCRIPT);
    let png = std::fs::read(out.join("curves_1.png")).expect("Natron wrote the frame");
    let img = bv_png::decode(&png).unwrap();
    let rgba = img.to_rgba8();
    let i = ((img.height / 2 * img.width + img.width / 2) * 4) as usize;
    // No red left; green and blue kept.
    assert_eq!([rgba[i], rgba[i + 1], rgba[i + 2]], [0, 255, 255]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "needs Natron (BV_NATRON: its bin folder)"]
fn natron_reddens_midtones_with_bananavs_color_corrector() {
    let Some(bin) = std::env::var_os("BV_NATRON") else {
        eprintln!("BV_NATRON is not set: nothing to run");
        return;
    };
    let dir = std::env::temp_dir().join(format!("bananav-natron-corrector-{}", std::process::id()));
    let out = natron(&bin, &dir, "corrector", CORRECTOR_SCRIPT);
    let png = std::fs::read(out.join("corrector_1.png")).expect("Natron wrote the frame");
    let img = bv_png::decode(&png).unwrap();
    let rgba = img.to_rgba8();
    let i = ((img.height / 2 * img.width + img.width / 2) * 4) as usize;
    let [r, g, b] = [rgba[i], rgba[i + 1], rgba[i + 2]].map(i32::from);
    // Red up, green and blue down alike.
    assert!(r > g + 15 && (g - b).abs() <= 2, "{r} {g} {b}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "needs Natron (BV_NATRON: its bin folder)"]
fn natron_cuts_a_circle_and_shades_it_with_bananavs_effects() {
    let Some(bin) = std::env::var_os("BV_NATRON") else {
        eprintln!("BV_NATRON is not set: nothing to run");
        return;
    };
    let dir = std::env::temp_dir().join(format!("bananav-natron-layer-{}", std::process::id()));
    let out = natron(&bin, &dir, "layer", LAYER_SCRIPT);
    let png = std::fs::read(out.join("layer_1.png")).expect("Natron wrote the frame");
    let img = bv_png::decode(&png).unwrap();
    let rgba = img.to_rgba8();
    let (w, h) = (img.width as f64, img.height as f64);
    let at = |x: f64, y: f64| {
        let i = ((y as u32 * img.width + x as u32) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    };
    // The circle's middle: white.
    assert_eq!(at(w / 2.0, h / 2.0), [255, 255, 255, 255]);
    // Its top left corner, away from the shadow: nothing.
    assert_eq!(at(4.0, 4.0)[3], 0);
    // Just past the circle, down and right: its shadow, black.
    let r = 0.25 * w.hypot(h) + 40.0;
    let k = std::f64::consts::FRAC_1_SQRT_2;
    let shade = at(w / 2.0 + r * k, h / 2.0 + r * k);
    assert!(shade[3] > 200 && shade[0] < 20, "{shade:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "needs Natron (BV_NATRON: its bin folder)"]
fn natron_pixelates_with_bananavs_pixelate() {
    let Some(bin) = std::env::var_os("BV_NATRON") else {
        eprintln!("BV_NATRON is not set: nothing to run");
        return;
    };
    let dir = std::env::temp_dir().join(format!("bananav-natron-pixelate-{}", std::process::id()));
    let out = natron(&bin, &dir, "pixelate", PIXELATE_SCRIPT);
    let png = std::fs::read(out.join("pixelate_1.png")).expect("Natron wrote the frame");
    let img = bv_png::decode(&png).unwrap();
    let rgba = img.to_rgba8();
    let at = |x: u32, y: u32| {
        let i = ((y * img.width + x) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    };
    // Squares apart in the checkerboard, one cell here: one color, the
    // squares' average (grey).
    let corner = at(10, 10);
    assert_eq!(corner, at(100, 10));
    assert_eq!(corner, at(10, 100));
    assert_eq!(corner, at(100, 100));
    // The next cell down (from 180 pixels on) holds other parts of the
    // squares (across, each cell holds as much of both colors: Natron
    // centers its squares).
    assert_ne!(corner, at(10, 250));
    let [r, g, b, _] = corner.map(i32::from);
    assert!(
        (r - g).abs() <= 2 && (g - b).abs() <= 2 && r > 40 && r < 220,
        "{corner:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "needs Natron (BV_NATRON: its bin folder)"]
fn natron_swaps_colors_with_bananavs_lut_filter() {
    let Some(bin) = std::env::var_os("BV_NATRON") else {
        eprintln!("BV_NATRON is not set: nothing to run");
        return;
    };
    let dir = std::env::temp_dir().join(format!("bananav-natron-lut-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // Two points each way, red and blue swapped.
    let mut cube = String::from("LUT_3D_SIZE 2\n");
    for b in 0..2 {
        for g in 0..2 {
            for r in 0..2 {
                cube += &format!("{b} {g} {r}\n");
            }
        }
    }
    let file = dir.join("swap.cube");
    std::fs::write(&file, cube).unwrap();
    let path = file.display().to_string().replace('\\', "/");
    let out = natron(&bin, &dir, "lut", &LUT_SCRIPT.replace("LUTFILE", &path));
    let png = std::fs::read(out.join("lut_1.png")).expect("Natron wrote the frame");
    let img = bv_png::decode(&png).unwrap();
    let rgba = img.to_rgba8();
    let i = ((img.height / 2 * img.width + img.width / 2) * 4) as usize;
    assert_eq!([rgba[i], rgba[i + 1], rgba[i + 2]], [0, 0, 255]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "needs Natron (BV_NATRON: its bin folder)"]
fn natron_takes_a_cast_away_with_bananavs_white_balance() {
    let Some(bin) = std::env::var_os("BV_NATRON") else {
        eprintln!("BV_NATRON is not set: nothing to run");
        return;
    };
    let dir = std::env::temp_dir().join(format!("bananav-natron-wb-{}", std::process::id()));
    let out = natron(&bin, &dir, "wb", WHITE_BALANCE_SCRIPT);
    let png = std::fs::read(out.join("wb_1.png")).expect("Natron wrote the frame");
    let img = bv_png::decode(&png).unwrap();
    let rgba = img.to_rgba8();
    let i = ((img.height / 2 * img.width + img.width / 2) * 4) as usize;
    let [r, g, b] = [rgba[i], rgba[i + 1], rgba[i + 2]].map(i32::from);
    // Gray (Natron's values are linear: the picture's own numbers still).
    assert!((r - g).abs() <= 3 && (g - b).abs() <= 3, "{r} {g} {b}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "needs Natron (BV_NATRON: its bin folder)"]
fn natron_equalizes_with_bananavs_white_balance() {
    let Some(bin) = std::env::var_os("BV_NATRON") else {
        eprintln!("BV_NATRON is not set: nothing to run");
        return;
    };
    let dir = std::env::temp_dir().join(format!("bananav-natron-eq-{}", std::process::id()));
    let out = natron(&bin, &dir, "equalize", EQUALIZE_SCRIPT);
    let png = std::fs::read(out.join("equalize_1.png")).expect("Natron wrote the frame");
    let img = bv_png::decode(&png).unwrap();
    let rgba = img.to_rgba8();
    let at = |x: u32, y: u32| {
        let i = ((y * img.width + x) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2]]
    };
    // The whole frame's levels: the orange went black (its blue the
    // lowest), the blue rectangle pure blue (its red and green the
    // lowest).
    assert_eq!(at(2, 2), [0, 0, 0]);
    assert_eq!(at(img.width / 2, img.height / 2), [0, 0, 255]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "needs Natron (BV_NATRON: its bin folder)"]
fn natron_turns_hues_with_bananavs_hsl_adjust() {
    let Some(bin) = std::env::var_os("BV_NATRON") else {
        eprintln!("BV_NATRON is not set: nothing to run");
        return;
    };
    let dir = std::env::temp_dir().join(format!("bananav-natron-hsl-{}", std::process::id()));
    let out = natron(&bin, &dir, "hsl", HSL_SCRIPT);
    let png = std::fs::read(out.join("hsl_1.png")).expect("Natron wrote the frame");
    let img = bv_png::decode(&png).unwrap();
    let rgba = img.to_rgba8();
    let i = ((img.height / 2 * img.width + img.width / 2) * 4) as usize;
    let [r, g, b] = [rgba[i], rgba[i + 1], rgba[i + 2]];
    // The orange's opposite: blue (0.1, 0.5, 0.9 in Natron's linear values).
    assert!(b > g && g > r, "{r} {g} {b}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "needs Natron (BV_NATRON: its bin folder)"]
fn natron_shrinks_and_pins_with_bananavs_picture_in_picture() {
    let Some(bin) = std::env::var_os("BV_NATRON") else {
        eprintln!("BV_NATRON is not set: nothing to run");
        return;
    };
    let dir = std::env::temp_dir().join(format!("bananav-natron-pip-{}", std::process::id()));
    let out = natron(&bin, &dir, "pip", PIP_SCRIPT);
    let alpha_at = |file: &str, fx: f64, fy: f64| {
        let png = std::fs::read(out.join(file)).expect("Natron wrote the frame");
        let img = bv_png::decode(&png).unwrap();
        let rgba = img.to_rgba8();
        let (x, y) = (
            (fx * img.width as f64) as u32,
            ((1.0 - fy) * img.height as f64) as u32,
        );
        rgba[((y * img.width + x) * 4 + 3) as usize]
    };
    // Half the frame in its middle: transparent around.
    assert_eq!(alpha_at("pip_1.png", 0.5, 0.5), 255);
    assert_eq!(alpha_at("pip_1.png", 0.1, 0.1), 0);
    assert_eq!(alpha_at("pip_1.png", 0.9, 0.9), 0);
    // Its top right corner pinned to the frame's: the picture reaches it.
    assert_eq!(alpha_at("pinned_1.png", 0.96, 0.94), 255);
    assert_eq!(alpha_at("pinned_1.png", 0.1, 0.1), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "needs Natron (BV_NATRON: its bin folder)"]
fn natron_grays_the_strongest_colors_with_bananavs_saturation_adjust() {
    let Some(bin) = std::env::var_os("BV_NATRON") else {
        eprintln!("BV_NATRON is not set: nothing to run");
        return;
    };
    let dir =
        std::env::temp_dir().join(format!("bananav-natron-saturation-{}", std::process::id()));
    let out = natron(&bin, &dir, "saturation", SATURATION_SCRIPT);
    let png = std::fs::read(out.join("saturation_1.png")).expect("Natron wrote the frame");
    let img = bv_png::decode(&png).unwrap();
    let rgba = img.to_rgba8();
    let i = ((img.height / 2 * img.width + img.width / 2) * 4) as usize;
    let [r, g, b] = [rgba[i], rgba[i + 1], rgba[i + 2]].map(i32::from);
    // Gray at its luma (0.642 in Natron's linear values).
    assert!(
        (r - g).abs() <= 1 && (g - b).abs() <= 1 && r > 150,
        "{r} {g} {b}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "needs Natron (BV_NATRON: its bin folder)"]
fn natron_reddens_greys_with_bananavs_color_balance() {
    let Some(bin) = std::env::var_os("BV_NATRON") else {
        eprintln!("BV_NATRON is not set: nothing to run");
        return;
    };
    let dir = std::env::temp_dir().join(format!("bananav-natron-balance-{}", std::process::id()));
    let out = natron(&bin, &dir, "balance", BALANCE_SCRIPT);
    let png = std::fs::read(out.join("balance_1.png")).expect("Natron wrote the frame");
    let img = bv_png::decode(&png).unwrap();
    let rgba = img.to_rgba8();
    let i = ((img.height / 2 * img.width + img.width / 2) * 4) as usize;
    let [r, g, b] = [rgba[i], rgba[i + 1], rgba[i + 2]].map(i32::from);
    // Red up, green and blue down alike.
    assert!(r > g + 15 && (g - b).abs() <= 1, "{r} {g} {b}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "needs Natron (BV_NATRON: its bin folder)"]
fn natron_turns_red_cyan_with_bananavs_color_corrector_secondary() {
    let Some(bin) = std::env::var_os("BV_NATRON") else {
        eprintln!("BV_NATRON is not set: nothing to run");
        return;
    };
    let dir = std::env::temp_dir().join(format!("bananav-natron-secondary-{}", std::process::id()));
    let out = natron(&bin, &dir, "secondary", SECONDARY_SCRIPT);
    let png = std::fs::read(out.join("secondary_1.png")).expect("Natron wrote the frame");
    let img = bv_png::decode(&png).unwrap();
    let rgba = img.to_rgba8();
    let i = ((img.height / 2 * img.width + img.width / 2) * 4) as usize;
    let [r, g, b] = [rgba[i], rgba[i + 1], rgba[i + 2]].map(i32::from);
    // Across from red: cyan, green and blue alike above red.
    assert!(g > r + 15 && (g - b).abs() <= 2, "{r} {g} {b}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "needs Natron (BV_NATRON: its bin folder)"]
fn natron_frames_with_bananavs_border() {
    let Some(bin) = std::env::var_os("BV_NATRON") else {
        eprintln!("BV_NATRON is not set: nothing to run");
        return;
    };
    let dir = std::env::temp_dir().join(format!("bananav-natron-border-{}", std::process::id()));
    let out = natron(&bin, &dir, "border", BORDER_SCRIPT);
    let png = std::fs::read(out.join("border_1.png")).expect("Natron wrote the frame");
    let img = bv_png::decode(&png).unwrap();
    let rgba = img.to_rgba8();
    let at = |x: u32, y: u32| {
        let i = ((y * img.width + x) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2]]
    };
    // The white frame on the edges, the orange inside.
    assert_eq!(at(2, img.height / 2), [255, 255, 255]);
    assert_eq!(at(img.width / 2, 2), [255, 255, 255]);
    let middle = at(img.width / 2, img.height / 2);
    assert!(middle[0] > middle[1] && middle[1] > middle[2], "{middle:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "needs Natron (BV_NATRON: its bin folder)"]
fn natron_fills_the_bars_with_bananavs_black_bar_fill() {
    let Some(bin) = std::env::var_os("BV_NATRON") else {
        eprintln!("BV_NATRON is not set: nothing to run");
        return;
    };
    let dir = std::env::temp_dir().join(format!("bananav-natron-fill-{}", std::process::id()));
    let out = natron(&bin, &dir, "fill", FILL_SCRIPT);
    let png = std::fs::read(out.join("fill_1.png")).expect("Natron wrote the frame");
    let img = bv_png::decode(&png).unwrap();
    let rgba = img.to_rgba8();
    // The bars hold the strip enlarged: blue, solid, out to the edges.
    let i = ((img.height / 2 * img.width + 2) * 4) as usize;
    let [r, g, b, a] = [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]];
    assert!(a == 255 && b > g && g > r, "{r} {g} {b} {a}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "needs Natron (BV_NATRON: its bin folder)"]
fn natron_sharpens_with_bananavs_unsharp_mask() {
    let Some(bin) = std::env::var_os("BV_NATRON") else {
        eprintln!("BV_NATRON is not set: nothing to run");
        return;
    };
    let dir = std::env::temp_dir().join(format!("bananav-natron-unsharp-{}", std::process::id()));
    let out = natron(&bin, &dir, "unsharp", UNSHARP_SCRIPT);
    let png = std::fs::read(out.join("unsharp_1.png")).expect("Natron wrote the frame");
    let img = bv_png::decode(&png).unwrap();
    let rgba = img.to_rgba8();
    let at = |x: u32| rgba[((img.height / 2 * img.width + x) * 4) as usize] as i32;
    let (half, w) = (img.width / 2, img.width);
    // Either side of the edge pulled apart; far from it, as it was.
    assert!(
        at(half + 2) > at(w * 3 / 4) + 10,
        "{} {}",
        at(half + 2),
        at(w * 3 / 4)
    );
    assert!(
        at(half - 3) + 10 < at(w / 4),
        "{} {}",
        at(half - 3),
        at(w / 4)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "needs Natron (BV_NATRON: its bin folder)"]
fn natron_adds_noise_with_bananavs_add_noise() {
    let Some(bin) = std::env::var_os("BV_NATRON") else {
        eprintln!("BV_NATRON is not set: nothing to run");
        return;
    };
    let dir = std::env::temp_dir().join(format!("bananav-natron-noise-{}", std::process::id()));
    let out = natron(&bin, &dir, "noise", NOISE_SCRIPT);
    let png = std::fs::read(out.join("noise_1.png")).expect("Natron wrote the frame");
    let img = bv_png::decode(&png).unwrap();
    let rgba = img.to_rgba8();
    let px = |x: u32| {
        let i = ((img.height / 2 * img.width + x) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2]]
    };
    // Gray specks, each pixel its own.
    let row: Vec<[u8; 3]> = (100..200).map(px).collect();
    assert!(row.iter().all(|p| p[0] == p[1] && p[1] == p[2]), "{row:?}");
    let distinct = row.windows(2).filter(|w| w[0] != w[1]).count();
    assert!(distinct > 80, "{distinct}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "needs Natron (BV_NATRON: its bin folder)"]
fn natron_writes_the_time_with_bananavs_timecode() {
    let Some(bin) = std::env::var_os("BV_NATRON") else {
        eprintln!("BV_NATRON is not set: nothing to run");
        return;
    };
    let dir = std::env::temp_dir().join(format!("bananav-natron-timecode-{}", std::process::id()));
    let out = natron(&bin, &dir, "timecode", TIMECODE_SCRIPT);
    let png = std::fs::read(out.join("timecode_25.png")).expect("Natron wrote the frame");
    let img = bv_png::decode(&png).unwrap();
    let rgba = img.to_rgba8();
    let (w, h) = (img.width, img.height);
    let px = |x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2]]
    };
    // A black box with white letters in the lower right; the gray elsewhere.
    let region: Vec<[u8; 3]> = (h * 82 / 100..h * 90 / 100)
        .flat_map(|y| (w * 65 / 100..w * 89 / 100).map(move |x| (x, y)))
        .map(|(x, y)| px(x, y))
        .collect();
    assert!(region.iter().any(|p| p == &[0, 0, 0]), "no box");
    assert!(region.iter().any(|p| p[0] > 240), "no letters");
    let corner = px(w / 10, h / 10);
    assert!(corner[0] > 100 && corner[0] < 140, "{corner:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
