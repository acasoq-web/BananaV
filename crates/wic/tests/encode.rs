//! HEIC and JPEG XR through the Windows Imaging Component.
//! Run with `cargo test -p bv-wic --test encode -- --ignored --nocapture`.

#![cfg(windows)]

use bv_wic::{Container, can_write, encode};

#[test]
#[ignore = "needs the Windows Imaging Component and its extensions"]
fn heic_and_jpeg_xr() {
    let (w, h) = (64u32, 48u32);
    let mut rgba = Vec::new();
    for y in 0..h {
        for x in 0..w {
            rgba.extend_from_slice(&[(x * 4) as u8, (y * 5) as u8, 128, 200]);
        }
    }
    let d = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/generated");
    std::fs::create_dir_all(&d).unwrap();
    for (c, name, magic) in [
        (Container::JpegXr, "wic.jxr", &b"II\xBC"[..]),
        (Container::Heif, "wic.heic", &b"ftyp"[..]),
    ] {
        eprintln!("{c:?}: can write {}", can_write(c));
        if !can_write(c) {
            continue;
        }
        let bytes = encode(c, w, h, &rgba, true, 90).unwrap();
        std::fs::write(d.join(name), &bytes).unwrap();
        eprintln!("{name}: {} bytes", bytes.len());
        assert!(
            bytes.windows(magic.len()).take(16).any(|win| win == magic),
            "{name} starts with {:?}",
            &bytes[..12.min(bytes.len())]
        );
    }
}

/// Every picture format Windows reads here, against ffmpeg's decode (WebP,
/// AVIF) or what was written (HEIC, JPEG XR).
#[test]
#[ignore = "needs the Windows Imaging Component, its extensions and ffmpeg"]
fn decoders() {
    let d =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/generated/wic");
    std::fs::create_dir_all(&d).unwrap();
    let make = |name: &str, args: &[&str]| {
        let p = d.join(name);
        if !p.exists() {
            let st = std::process::Command::new("ffmpeg")
                .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
                .arg("testsrc2=size=96x64:rate=1:duration=1,format=rgba")
                .args(["-frames:v", "1"])
                .args(args)
                .arg(&p)
                .status()
                .expect("ffmpeg");
            assert!(st.success());
        }
        p
    };
    let reference = |p: &std::path::Path| -> Vec<u8> {
        std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(p)
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"])
            .output()
            .unwrap()
            .stdout
    };
    for (name, args, min_db) in [
        (
            "lossless.webp",
            &["-c:v", "libwebp", "-lossless", "1"][..],
            99.0,
        ),
        // 4:2:0 upsampled another way than ffmpeg: a sanity check.
        (
            "lossy.webp",
            &["-c:v", "libwebp", "-quality", "90"][..],
            20.0,
        ),
        ("icon.ico", &[][..], 99.0),
    ] {
        let p = make(name, args);
        let pic = bv_wic::decode(p.to_str().unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
        let want = reference(&p);
        let got: Vec<u8> = pic.rgba.iter().map(|v| (v >> 8) as u8).collect();
        assert_eq!(got.len(), want.len(), "{name}: size");
        let se: f64 = got
            .chunks(4)
            .zip(want.chunks(4))
            .map(|(a, b)| {
                (0..3)
                    .map(|c| (a[c] as f64 - b[c] as f64).powi(2))
                    .sum::<f64>()
            })
            .sum();
        let db = if se == 0.0 {
            f64::INFINITY
        } else {
            10.0 * (255.0f64.powi(2) / (se / (got.len() / 4 * 3) as f64)).log10()
        };
        eprintln!("{name}: {}x{} {db:.2} dB", pic.width, pic.height);
        assert!(db >= min_db, "{name}: {db:.2} dB");
    }
    // What the encoders wrote comes back.
    let (w, h) = (64u32, 48u32);
    let mut rgba = Vec::new();
    for y in 0..h {
        for x in 0..w {
            rgba.extend_from_slice(&[(x * 4) as u8, (y * 5) as u8, 128, 255]);
        }
    }
    for (c, name) in [
        (Container::JpegXr, "back.jxr"),
        (Container::Heif, "back.heic"),
    ] {
        if !can_write(c) {
            continue;
        }
        let p = d.join(name);
        std::fs::write(&p, encode(c, w, h, &rgba, false, 95).unwrap()).unwrap();
        let pic = bv_wic::decode(p.to_str().unwrap()).unwrap();
        assert_eq!((pic.width, pic.height), (w, h), "{name}");
        let err: f64 = pic
            .rgba
            .iter()
            .zip(&rgba)
            .map(|(a, b)| ((a >> 8) as f64 - *b as f64).abs())
            .sum::<f64>()
            / rgba.len() as f64;
        eprintln!("{name}: mean error {err:.2}");
        assert!(err < 4.0, "{name}: mean error {err}");
    }
}
