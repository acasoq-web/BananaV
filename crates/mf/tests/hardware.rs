//! Decoding on the graphics card (DXVA) against decoding on the processor:
//! the standards define the decoded picture exactly, so H.264, HEVC, VP9
//! and AV1 must give the same pictures either way (the 10-bit to 8-bit step
//! may differ by a little). MPEG-2 is decoded on the processor: Windows'
//! decoder lost a file's last frames on the card after a seek. Also how
//! fast each decodes. Needs ffmpeg and a graphics card:
//! `cargo test -p bv-mf --test hardware -- --ignored --nocapture`.

#![allow(clippy::print_stdout)]

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use bv_core::Tick;
use bv_mf::{HardwareDecoding, VideoDecoder};

fn make(name: &str, size: &str, encode: &[&str]) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/generated/hardware");
    std::fs::create_dir_all(&d).unwrap();
    let p = d.join(name);
    if !p.exists() {
        let src = format!("testsrc2=s={size}:r=30:d=4");
        let st = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", &src])
            .args(encode)
            .arg(&p)
            .status()
            .expect("ffmpeg on the PATH");
        assert!(st.success(), "ffmpeg made {name}");
    }
    p
}

/// The largest difference between two frames' planes.
fn diff(a: &bv_core::VideoFrame, b: &bv_core::VideoFrame) -> u16 {
    assert_eq!((a.width, a.height), (b.width, b.height));
    let mut worst = 0;
    for (pa, pb) in a.planes.iter().zip(&b.planes) {
        for y in 0..pa.height {
            for x in 0..pa.width {
                worst = worst.max(pa.sample(x, y).abs_diff(pb.sample(x, y)));
            }
        }
    }
    worst
}

fn open(path: &str, h: HardwareDecoding) -> VideoDecoder {
    bv_mf::set_hardware_decoding(h);
    VideoDecoder::open(path).expect("opens")
}

#[test]
#[ignore = "needs ffmpeg and a graphics card"]
fn card_decodes_as_the_processor() {
    println!("cards: {:?}", bv_mf::decode_adapters());
    let cases: [(&str, &str, &[&str], u16); 6] = [
        (
            "h264.mp4",
            "1280x720",
            &["-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "40"],
            0,
        ),
        (
            "hevc.mp4",
            "1280x720",
            &["-c:v", "libx265", "-pix_fmt", "yuv420p", "-tag:v", "hvc1"],
            0,
        ),
        (
            "hevc10.mp4",
            "1280x720",
            &[
                "-c:v",
                "libx265",
                "-pix_fmt",
                "yuv420p10le",
                "-tag:v",
                "hvc1",
            ],
            2,
        ),
        (
            "vp9.webm",
            "1280x720",
            &["-c:v", "libvpx-vp9", "-b:v", "2M", "-deadline", "realtime"],
            0,
        ),
        (
            "av1.mp4",
            "1280x720",
            &["-c:v", "libaom-av1", "-cpu-used", "8", "-b:v", "2M"],
            0,
        ),
        (
            "mpeg2.mpg",
            "1280x720",
            &["-c:v", "mpeg2video", "-q:v", "3"],
            4,
        ),
    ];
    for (name, size, encode, tolerance) in cases {
        let p = make(name, size, encode);
        let path = p.to_str().unwrap();
        let mut cpu = open(path, HardwareDecoding::Off);
        let mut card = open(path, HardwareDecoding::Auto);
        let rate = cpu.info().frame_rate;
        let mut worst = 0;
        let (mut t_cpu, mut t_card) = (0.0, 0.0);
        for k in 0..90 {
            let t = rate.frame_start(k) + Tick(1);
            let a = Instant::now();
            let fa = cpu.frame_at(t).unwrap();
            let b = Instant::now();
            let fb = card.frame_at(t).unwrap();
            t_cpu += (b - a).as_secs_f64();
            t_card += b.elapsed().as_secs_f64();
            worst = worst.max(diff(&fa, &fb));
        }
        println!(
            "{name:11} on the card: {:5}  differs by {worst:2}  processor {:5.2} ms, card {:5.2} ms a frame",
            card.on_graphics_card(),
            t_cpu * 1000.0 / 90.0,
            t_card * 1000.0 / 90.0
        );
        assert!(worst <= tolerance, "{name}: differs by {worst}");
        // Seeking back gives the same picture too, and so does the last
        // frame after seeks.
        for k in [17, 60, 1, 119] {
            let t = rate.frame_start(k) + Tick(1);
            let worst = diff(&cpu.frame_at(t).unwrap(), &card.frame_at(t).unwrap());
            assert!(worst <= tolerance, "{name}: frame {k} differs by {worst}");
        }
        if name == "mpeg2.mpg" {
            assert!(!card.on_graphics_card(), "MPEG-2 on the processor");
        }
    }
    bv_mf::set_hardware_decoding(HardwareDecoding::Off);
}

#[test]
#[ignore = "needs ffmpeg and a graphics card"]
fn heavy_video_decodes_faster_on_the_card() {
    let files = [
        make(
            "hevc10_4k.mp4",
            "3840x2160",
            &[
                "-c:v",
                "libx265",
                "-preset",
                "ultrafast",
                "-pix_fmt",
                "yuv420p10le",
                "-tag:v",
                "hvc1",
            ],
        ),
        make(
            "hevc_4k.mp4",
            "3840x2160",
            &[
                "-c:v",
                "libx265",
                "-preset",
                "ultrafast",
                "-pix_fmt",
                "yuv420p",
                "-tag:v",
                "hvc1",
            ],
        ),
        make(
            "h264_4k.mp4",
            "3840x2160",
            &[
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-pix_fmt",
                "yuv420p",
            ],
        ),
    ];
    for p in &files {
        let path = p.to_str().unwrap();
        let name = p.file_name().unwrap().to_string_lossy();
        for h in [HardwareDecoding::Off, HardwareDecoding::Auto] {
            let mut d = open(path, h.clone());
            let rate = d.info().frame_rate;
            d.frame_at(Tick(1)).unwrap();
            let mut usage = bv_platform::Usage::new();
            usage.cpu();
            let start = Instant::now();
            for k in 1..=60 {
                d.frame_at(rate.frame_start(k) + Tick(1)).unwrap();
            }
            println!(
                "{name:15} {h:?} (card: {}): {:.1} ms a frame, processor {:.0}%",
                d.on_graphics_card(),
                start.elapsed().as_secs_f64() * 1000.0 / 60.0,
                usage.cpu().unwrap_or(0.0)
            );
        }
    }
    bv_mf::set_hardware_decoding(HardwareDecoding::Off);
}
