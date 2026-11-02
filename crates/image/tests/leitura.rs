//! Every reader against ffmpeg's: files made by ffmpeg (and by Pillow for
//! what ffmpeg does not write: progressive and CMYK JPEG, EXIF orientation,
//! animated GIF with disposal, RLE TGA…) are read by BananaV and by ffmpeg,
//! and the pictures compared.
//! Run with `cargo test -p bv-image --test leitura -- --ignored --nocapture`
//! (needs `ffmpeg` and Python with Pillow on the PATH).

#![allow(clippy::print_stdout)]

use std::path::{Path, PathBuf};
use std::process::Command;

use bv_image::read::{
    Gif, GifPlayer, Picture, PictureData, dimensions, read, read_tga, tga_dimensions,
};

fn dir() -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/generated/leitura");
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Makes `name` with ffmpeg from a test pattern (once).
fn ffmpeg(name: &str, args: &[&str]) -> PathBuf {
    let p = dir().join(name);
    if !p.exists() {
        let st = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg("testsrc2=size=97x61:rate=1:duration=1,format=rgba")
            .args(["-frames:v", "1"])
            .args(args)
            .arg(&p)
            .status()
            .expect("ffmpeg on the PATH");
        assert!(st.success(), "ffmpeg made {name}");
    }
    p
}

/// Makes `name` with a Python (Pillow) script that saves to `out`.
fn pillow(name: &str, script: &str) -> PathBuf {
    let p = dir().join(name);
    if !p.exists() {
        let code = format!(
            "from PIL import Image, ImageDraw\nout = r'{}'\n{script}",
            p.display()
        );
        let st = Command::new("python")
            .args(["-c", &code])
            .status()
            .expect("python on the PATH");
        assert!(st.success(), "Pillow made {name}");
    }
    p
}

/// How ffmpeg gives its decode: the depth of ours.
#[derive(Clone, Copy, PartialEq)]
enum Depth {
    Eight,
    Sixteen,
    Float,
}

/// ffmpeg's decode as RGBA in 0–1 (all frames one after the other).
fn reference(path: &Path, depth: Depth) -> Vec<f32> {
    let o = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        // Every frame once (not repeated to a constant rate).
        .args(["-fps_mode", "passthrough", "-f", "rawvideo", "-pix_fmt"])
        .arg(match depth {
            Depth::Eight => "rgba",
            Depth::Sixteen => "rgba64le",
            Depth::Float => "gbrapf32le",
        })
        .arg("-")
        .output()
        .expect("ffmpeg");
    if depth == Depth::Eight {
        return o.stdout.iter().map(|&v| v as f32 / 255.0).collect();
    }
    if depth == Depth::Float {
        // Planar G, B, R, A per frame: back to RGBA.
        let v: Vec<f32> = o
            .stdout
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();
        let n = v.len() / 4;
        (0..n)
            .flat_map(|i| [v[2 * n + i], v[i], v[n + i], v[3 * n + i]])
            .collect()
    } else {
        o.stdout
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes(*b) as f32 / 65535.0)
            .collect()
    }
}

fn values(p: &Picture) -> Vec<f32> {
    match &p.data {
        PictureData::Rgba8(d) => d.iter().map(|&v| v as f32 / 255.0).collect(),
        PictureData::Rgba16(d) => d.iter().map(|&v| v as f32 / 65535.0).collect(),
        PictureData::RgbaF32(d) => d.clone(),
    }
}

/// PSNR of the color channels (alpha compared apart), in dB.
fn psnr(a: &[f32], b: &[f32]) -> f64 {
    assert_eq!(a.len(), b.len(), "sizes differ");
    let mut se = 0.0f64;
    let mut n = 0usize;
    for (pa, pb) in a.chunks(4).zip(b.chunks(4)) {
        for c in 0..3 {
            se += ((pa[c] - pb[c]) as f64).powi(2);
            n += 1;
        }
    }
    if se == 0.0 {
        return f64::INFINITY;
    }
    10.0 * (1.0 / (se / n as f64)).log10()
}

fn max_alpha_error(a: &[f32], b: &[f32]) -> f32 {
    a.chunks(4)
        .zip(b.chunks(4))
        .map(|(x, y)| (x[3] - y[3]).abs())
        .fold(0.0, f32::max)
}

/// Pillow's decode (libjpeg's, for JPEG: ffmpeg's chroma upsampling is
/// not libjpeg's) as RGBA in 0–1.
fn pillow_reference(path: &Path) -> Vec<f32> {
    let code = format!(
        "import sys
from PIL import Image
sys.stdout.buffer.write(Image.open(r'{}').convert('RGBA').tobytes())",
        path.display()
    );
    let o = Command::new("python")
        .args(["-c", &code])
        .output()
        .expect("python");
    o.stdout.iter().map(|&v| v as f32 / 255.0).collect()
}

/// Reads a JPEG and compares with libjpeg (through Pillow).
fn check_jpeg(path: &Path, min_db: f64) {
    let name = path.file_name().unwrap().to_string_lossy();
    let data = std::fs::read(path).unwrap();
    let pic = read(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert_eq!(
        dimensions(&data),
        Some((pic.width, pic.height)),
        "{name}: header size"
    );
    let got = values(&pic);
    let want = pillow_reference(path);
    let db = psnr(&got, &want[..got.len().min(want.len())]);
    println!(
        "{name:28} {}x{} {db:>7.2} dB (libjpeg)",
        pic.width, pic.height
    );
    assert!(db >= min_db, "{name}: {db:.2} dB < {min_db}");
}

/// Like [`check`] for an 8-bit file read at 16 bits: both at 8 bits.
fn check8(path: &Path) {
    let name = path.file_name().unwrap().to_string_lossy();
    let pic = read(&std::fs::read(path).unwrap()).unwrap();
    let got: Vec<f32> = values(&pic)
        .iter()
        .map(|v| (v * 255.0).round() / 255.0)
        .collect();
    let want = reference(path, Depth::Eight);
    let db = psnr(&got, &want);
    println!(
        "{name:28} {}x{} {db:>7.2} dB (8 bits)",
        pic.width, pic.height
    );
    assert!(db.is_infinite(), "{name}: {db:.2} dB");
}

/// Reads `path` (TGA by name) and compares with ffmpeg: at least `min_db`.
fn check(path: &Path, min_db: f64) {
    let data = std::fs::read(path).unwrap();
    let name = path.file_name().unwrap().to_string_lossy();
    let pic = if name.ends_with(".tga") {
        read_tga(&data)
    } else {
        read(&data)
    }
    .unwrap_or_else(|e| panic!("{name}: {e}"));
    let header = if name.ends_with(".tga") {
        tga_dimensions(&data)
    } else {
        dimensions(&data)
    };
    assert_eq!(header, Some((pic.width, pic.height)), "{name}: header size");
    let depth = match pic.data {
        PictureData::Rgba8(_) => Depth::Eight,
        PictureData::Rgba16(_) => Depth::Sixteen,
        PictureData::RgbaF32(_) => Depth::Float,
    };
    let got = values(&pic);
    let want = reference(path, depth);
    let want = &want[..got.len().min(want.len())];
    let db = psnr(&got, want);
    let alpha = max_alpha_error(&got, want);
    println!(
        "{name:28} {}x{} {:>7.2} dB, alpha error {alpha:.4}",
        pic.width, pic.height, db
    );
    assert!(db >= min_db, "{name}: {db:.2} dB < {min_db}");
    assert!(alpha <= 0.01, "{name}: alpha differs by {alpha}");
}

#[test]
#[ignore = "needs ffmpeg and Pillow"]
fn bmp() {
    check(&ffmpeg("rgb24.bmp", &["-pix_fmt", "bgr24"]), 99.0);
    check(&ffmpeg("bgra.bmp", &["-pix_fmt", "bgra"]), 99.0);
    // 5 and 6-bit channels widened with rounding (ffmpeg: bit copies).
    check(&ffmpeg("rgb565.bmp", &["-pix_fmt", "rgb565"]), 50.0);
    check(&ffmpeg("rgb555.bmp", &["-pix_fmt", "rgb555"]), 50.0);
    check(&ffmpeg("pal8.bmp", &["-pix_fmt", "pal8"]), 99.0);
    check(&ffmpeg("gray.bmp", &["-pix_fmt", "gray"]), 99.0);
    check(&ffmpeg("mono.bmp", &["-pix_fmt", "monob"]), 99.0);
    check(
        &pillow(
            "pal4_rle.bmp",
            "im = Image.radial_gradient('L').resize((61, 37)).quantize(16)\nim.save(out, compression=2)",
        ),
        99.0,
    );
}

#[test]
#[ignore = "needs ffmpeg and Pillow"]
fn jpeg() {
    check_jpeg(
        &ffmpeg("base420.jpg", &["-pix_fmt", "yuvj420p", "-q:v", "2"]),
        50.0,
    );
    check_jpeg(
        &ffmpeg("base422.jpg", &["-pix_fmt", "yuvj422p", "-q:v", "2"]),
        50.0,
    );
    check_jpeg(
        &ffmpeg("base444.jpg", &["-pix_fmt", "yuvj444p", "-q:v", "2"]),
        50.0,
    );
    check_jpeg(
        &ffmpeg("gray.jpg", &["-pix_fmt", "gray", "-q:v", "2"]),
        50.0,
    );
    let photo = "im = Image.radial_gradient('L').resize((99, 67)).convert('RGB')\nd = ImageDraw.Draw(im)\nd.rectangle((10, 10, 40, 30), fill=(200, 30, 60))\nd.ellipse((50, 20, 90, 60), fill=(20, 160, 90))\n";
    check_jpeg(
        &pillow(
            "progressive.jpg",
            &format!("{photo}im.save(out, quality=92, progressive=True)"),
        ),
        50.0,
    );
    check_jpeg(
        &pillow(
            "progressive_444.jpg",
            &format!("{photo}im.save(out, quality=95, progressive=True, subsampling=0)"),
        ),
        50.0,
    );
    check_jpeg(
        &pillow(
            "restart.jpg",
            &format!("{photo}im.save(out, quality=90, restart_marker_blocks=3)"),
        ),
        50.0,
    );
    check(
        &pillow(
            "cmyk.jpg",
            &format!("{photo}im.convert('CMYK').save(out, quality=95)"),
        ),
        30.0,
    );
}

/// A photo taken sideways: EXIF orientation 6 turns it upright.
#[test]
#[ignore = "needs Pillow"]
fn jpeg_orientation() {
    let path = pillow(
        "oriented.jpg",
        "im = Image.new('RGB', (40, 20), (255, 0, 0))\nImageDraw.Draw(im).rectangle((0, 0, 9, 19), fill=(0, 0, 255))\nexif = Image.Exif()\nexif[274] = 6\nim.save(out, quality=95, exif=exif.tobytes())",
    );
    let data = std::fs::read(path).unwrap();
    let pic = read(&data).unwrap();
    // 40×20 stored, turned 90° clockwise to show: 20×40, blue on top.
    assert_eq!((pic.width, pic.height), (20, 40));
    assert_eq!(dimensions(&data), Some((20, 40)));
    let PictureData::Rgba8(px) = &pic.data else {
        panic!()
    };
    let at = |x: usize, y: usize| &px[(y * 20 + x) * 4..(y * 20 + x) * 4 + 3];
    assert!(at(10, 2)[2] > 200 && at(10, 2)[0] < 60, "top is blue");
    assert!(at(10, 37)[0] > 200 && at(10, 37)[2] < 60, "bottom is red");
}

#[test]
#[ignore = "needs ffmpeg"]
fn tiff() {
    check(&ffmpeg("plain.tif", &["-pix_fmt", "rgb24"]), 99.0);
    check(&ffmpeg("rgba.tif", &["-pix_fmt", "rgba"]), 99.0);
    check(&ffmpeg("rgb48.tif", &["-pix_fmt", "rgb48le"]), 99.0);
    check(&ffmpeg("rgba64.tif", &["-pix_fmt", "rgba64le"]), 99.0);
    check(&ffmpeg("gray.tif", &["-pix_fmt", "gray"]), 99.0);
    check(&ffmpeg("gray16.tif", &["-pix_fmt", "gray16le"]), 99.0);
    check(&ffmpeg("pal8.tif", &["-pix_fmt", "pal8"]), 99.0);
    check(&ffmpeg("mono.tif", &["-pix_fmt", "monob"]), 99.0);
    // Y′CbCr (ffmpeg's TIFF of video): chroma widened its own way.
    check(&ffmpeg("ycbcr420.tif", &["-pix_fmt", "yuv420p"]), 22.0);
    check(&ffmpeg("ycbcr422.tif", &["-pix_fmt", "yuv422p"]), 22.0);
    check(&ffmpeg("ycbcr444.tif", &["-pix_fmt", "yuv444p"]), 40.0);
    for algo in ["lzw", "deflate", "packbits"] {
        check(
            &ffmpeg(
                &format!("{algo}.tif"),
                &["-pix_fmt", "rgb24", "-compression_algo", algo],
            ),
            99.0,
        );
        check(
            &ffmpeg(
                &format!("{algo}48.tif"),
                &["-pix_fmt", "rgb48le", "-compression_algo", algo],
            ),
            99.0,
        );
    }
    check(
        &pillow(
            "predictor.tif",
            "im = Image.radial_gradient('L').resize((61, 37)).convert('RGB')\nim.save(out, compression='tiff_lzw', tiffinfo={317: 2})",
        ),
        99.0,
    );
    check(
        &pillow(
            "tiled.tif",
            "im = Image.radial_gradient('L').resize((61, 37)).convert('RGB')\nim.save(out, compression='tiff_deflate', tile=(16, 16))",
        ),
        99.0,
    );
}

#[test]
#[ignore = "needs ffmpeg and Pillow"]
fn tga() {
    check(&ffmpeg("rle.tga", &["-pix_fmt", "bgr24"]), 99.0);
    check(
        &ffmpeg("raw.tga", &["-pix_fmt", "bgr24", "-rle", "0"]),
        99.0,
    );
    check(&ffmpeg("bgra.tga", &["-pix_fmt", "bgra"]), 99.0);
    check(&ffmpeg("gray.tga", &["-pix_fmt", "gray"]), 99.0);
    check(&ffmpeg("rgb555.tga", &["-pix_fmt", "rgb555le"]), 50.0);
    check(
        &pillow(
            "top_down.tga",
            "im = Image.radial_gradient('L').resize((61, 37)).convert('RGBA')\nim.save(out, rle=True, orientation=1)",
        ),
        99.0,
    );
    check(
        &pillow(
            "pal.tga",
            "im = Image.radial_gradient('L').resize((61, 37)).quantize(64)\nim.save(out)",
        ),
        99.0,
    );
}

#[test]
#[ignore = "needs ffmpeg"]
fn dpx() {
    check(&ffmpeg("ten.dpx", &["-pix_fmt", "gbrp10le"]), 99.0);
    check(&ffmpeg("twelve.dpx", &["-pix_fmt", "gbrp12le"]), 99.0);
    check(&ffmpeg("sixteen.dpx", &["-pix_fmt", "rgb48le"]), 99.0);
    check(&ffmpeg("rgba16.dpx", &["-pix_fmt", "rgba64le"]), 99.0);
    // ffmpeg widens 8 bits to 16 its own way: compared at 8 bits.
    check8(&ffmpeg("eight.dpx", &["-pix_fmt", "rgb24"]));
    check(&ffmpeg("gray.dpx", &["-pix_fmt", "gray16le"]), 99.0);
}

#[test]
#[ignore = "needs ffmpeg"]
fn exr() {
    for (name, comp, fmt) in [
        ("none_half.exr", "none", "half"),
        ("rle_half.exr", "rle", "half"),
        ("zip1_float.exr", "zip1", "float"),
        ("zip16_half.exr", "zip16", "half"),
    ] {
        check(
            &ffmpeg(
                name,
                &[
                    "-pix_fmt",
                    "gbrapf32le",
                    "-compression",
                    comp,
                    "-format",
                    fmt,
                ],
            ),
            // ffmpeg's float output is a hair off (alpha 1 comes as
            // 0.9995): about 75 dB.
            70.0,
        );
    }
    check(
        &ffmpeg(
            "rgb_zip16.exr",
            &["-pix_fmt", "gbrpf32le", "-compression", "zip16"],
        ),
        70.0,
    );
}

/// GIF: every frame of an animation with transparency and the disposal
/// methods, as ffmpeg plays it.
#[test]
#[ignore = "needs ffmpeg and Pillow"]
fn gif() {
    check(&ffmpeg("still.gif", &[]), 99.0);
    let path = pillow(
        "animated.gif",
        r#"frames = []
for i in range(6):
    im = Image.new('RGBA', (48, 32), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    d.ellipse((i * 6, 4, i * 6 + 14, 18), fill=(255, 40 * i, 0, 255))
    d.rectangle((0, 24, 8 * i, 31), fill=(0, 120, 255, 255))
    frames.append(im)
frames[0].save(out, save_all=True, append_images=frames[1:], duration=[40, 60, 80, 40, 40, 100], disposal=[1, 2, 2, 1, 3, 2], loop=0, transparency=0)
"#,
    );
    let data = std::fs::read(&path).unwrap();
    let gif = Gif::parse(&data).unwrap();
    assert_eq!(gif.frames.len(), 6);
    assert_eq!(gif.duration(), 36, "hundredths");
    let want = reference(&path, Depth::Eight);
    let frame_len = (gif.width * gif.height * 4) as usize;
    let mut player = GifPlayer::new(&gif);
    // In order, then backwards (from the frames kept).
    for k in (0..6).chain((0..6).rev()) {
        let got: Vec<f32> = player
            .frame(&gif, k)
            .iter()
            .map(|&v| v as f32 / 255.0)
            .collect();
        // Where both are opaque the colors match; transparency matches.
        let w = &want[k * frame_len..(k + 1) * frame_len];
        let mut wrong = 0;
        for (i, (a, b)) in got.chunks(4).zip(w.chunks(4)).enumerate() {
            let alpha_differs = (a[3] - b[3]).abs() > 0.01;
            let color_differs = a[3] > 0.0 && (0..3).any(|c| (a[c] - b[c]).abs() > 0.01);
            if (alpha_differs || color_differs) && wrong < 5 {
                println!(
                    "  ({}, {}): ours {a:?} ffmpeg {b:?}",
                    i % gif.width as usize,
                    i / gif.width as usize
                );
            }
            wrong += usize::from(alpha_differs || color_differs);
        }
        println!("animated.gif frame {k}: {wrong} pixels differ");
        assert_eq!(wrong, 0, "frame {k}");
    }
}

/// Photoshop documents built from the specification, read by ffmpeg too.
#[test]
#[ignore = "needs ffmpeg"]
fn psd() {
    // RGB, 8 and 16 bits, raw and RLE (PackBits rows).
    for (depth, rle) in [(8u16, false), (8, true), (16, false), (16, true)] {
        let (w, h) = (23usize, 9usize);
        let bytes = depth as usize / 8;
        let mut planes = Vec::new();
        for c in 0..3usize {
            let mut p = Vec::new();
            for y in 0..h {
                for x in 0..w {
                    let v = ((x * 11 + y * 23 + c * 70) % 256) as u16;
                    let v = if bytes == 2 { v * 257 } else { v };
                    p.extend_from_slice(&v.to_be_bytes()[2 - bytes..]);
                }
            }
            planes.push(p);
        }
        let mut d = b"8BPS".to_vec();
        d.extend_from_slice(&1u16.to_be_bytes());
        d.extend_from_slice(&[0; 6]);
        d.extend_from_slice(&3u16.to_be_bytes());
        d.extend_from_slice(&(h as u32).to_be_bytes());
        d.extend_from_slice(&(w as u32).to_be_bytes());
        d.extend_from_slice(&depth.to_be_bytes());
        d.extend_from_slice(&3u16.to_be_bytes());
        d.extend_from_slice(&[0; 12]);
        d.extend_from_slice(&u16::from(rle).to_be_bytes());
        if rle {
            // Literal runs only: valid PackBits.
            let row = w * bytes;
            let packed_row = |r: &[u8]| -> Vec<u8> {
                let mut o = Vec::new();
                for chunk in r.chunks(128) {
                    o.push((chunk.len() - 1) as u8);
                    o.extend_from_slice(chunk);
                }
                o
            };
            let rows: Vec<Vec<u8>> = planes
                .iter()
                .flat_map(|p| p.chunks(row).map(packed_row).collect::<Vec<_>>())
                .collect();
            for r in &rows {
                d.extend_from_slice(&(r.len() as u16).to_be_bytes());
            }
            for r in &rows {
                d.extend_from_slice(r);
            }
        } else {
            for p in &planes {
                d.extend_from_slice(p);
            }
        }
        let path = dir().join(format!("doc{depth}{}.psd", if rle { "_rle" } else { "" }));
        std::fs::write(&path, &d).unwrap();
        check(&path, 99.0);
    }
}

/// OpenEXR files of every codec Blender writes (in `testdata`): each one
/// BananaV reads matches ffmpeg; the others say so.
#[test]
#[ignore = "needs ffmpeg"]
fn exr_codecs() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/exr");
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    names.sort();
    for path in names {
        let data = std::fs::read(&path).unwrap();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        match read(&data) {
            Ok(pic) => {
                // ffmpeg clips half floats at 1 (the pattern goes to 2):
                // both compared clipped.
                let clip =
                    |v: Vec<f32>| -> Vec<f32> { v.iter().map(|v| v.clamp(0.0, 1.0)).collect() };
                let got = clip(values(&pic));
                let want = clip(reference(&path, Depth::Float));
                let db = psnr(&got, &want);
                println!("{name:28} {}x{} {db:>7.2} dB", pic.width, pic.height);
                assert!(db >= 70.0, "{name}: {db:.2} dB");
            }
            Err(e) => println!("{name:28} {e}"),
        }
    }
}
