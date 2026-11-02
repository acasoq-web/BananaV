//! Import against ffmpeg: a file of each container and codec BananaV
//! imports is made by ffmpeg and read through `bv_media` — the right frame
//! at each time (seeking back and forth, against ffmpeg's frames), the
//! sound where it was made (against the tone itself), stills at their size. Formats nothing decodes are
//! refused with their name (Windows; FFmpeg reads them on Linux).
//! Run with `cargo test -p bv-media --test importar -- --ignored --nocapture`.

#![allow(clippy::print_stdout)]
#![cfg(any(windows, all(target_os = "linux", not(bv_no_ffmpeg))))]

use std::path::PathBuf;
use std::process::Command;

use bv_core::Tick;
use bv_media::MediaError;

fn dir() -> PathBuf {
    let d =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/generated/import-chirp");
    std::fs::create_dir_all(&d).unwrap();
    d
}

const VIDEO: &str = "testsrc2=size=320x240:rate=25:duration=2";
/// A gliding tone: no two moments alike, so a sound running late shows.
const AUDIO: &str = "aevalsrc=0.5*sin(2*PI*(200+300*t)*t):s=48000:d=2";

/// Makes `name` with ffmpeg from a moving test pattern and a tone (once).
fn make(name: &str, inputs: &[&str], args: &[&str]) -> String {
    let p = dir().join(name);
    if !p.exists() {
        let mut c = Command::new("ffmpeg");
        c.args(["-v", "error", "-y"]);
        for i in inputs {
            c.args(["-f", "lavfi", "-i", i]);
        }
        let st = c.args(args).arg(&p).status().expect("ffmpeg on the PATH");
        assert!(st.success(), "ffmpeg made {name}");
    }
    p.to_string_lossy().to_string()
}

/// ffmpeg's frames as 8-bit RGB, each once.
fn ffmpeg_frames(path: &str) -> Vec<u8> {
    Command::new("ffmpeg")
        .args(["-v", "error", "-i", path, "-fps_mode", "passthrough"])
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .unwrap()
        .stdout
}

/// Checks that the frame shown at each of a set of times (out of order,
/// then in order) is ffmpeg's frame of that time.
fn check_frames(path: &str) {
    let info = bv_media::probe_file(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let v = info.video.expect("video");
    let (w, h) = (v.width as usize, v.height as usize);
    let frames = ffmpeg_frames(path);
    let len = w * h * 3;
    let count = frames.len() / len;
    assert!(count > 10, "{path}: ffmpeg frames");
    let mut r = bv_media::open_video(&info).unwrap();
    let mut order = vec![count / 2, 1, count - 1, 3, count / 3];
    order.extend(0..12);
    for n in order {
        let t = v.frame_rate.frame_start(n as i64) + Tick(v.frame_rate.frame_duration().0 / 4);
        let img = r
            .frame_at(t)
            .unwrap_or_else(|e| panic!("{path} frame {n}: {e}"));
        assert_eq!((img.width as usize, img.height as usize), (w, h), "{path}");
        // Ours back to 8-bit (colors may differ by the gamut conversion of
        // SD video: the best match tells which frame it is).
        let ours: Vec<f32> = img
            .data
            .iter()
            .flat_map(|p| {
                [0, 1, 2].map(|c| bv_color::transfer::linear_to_bt709(p[c].clamp(0.0, 1.0)) * 255.0)
            })
            .collect();
        let error = |m: usize| -> f64 {
            let theirs = &frames[m * len..(m + 1) * len];
            ours.iter()
                .zip(theirs)
                .map(|(a, b)| (*a as f64 - *b as f64).powi(2))
                .sum::<f64>()
        };
        let best = (n.saturating_sub(2)..=(n + 2).min(count - 1))
            .min_by(|a, b| error(*a).total_cmp(&error(*b)))
            .unwrap();
        assert_eq!(best, n, "{path}: the frame at {n} is ffmpeg's {best}");
    }
}

/// ffmpeg's decode of the file's first channel, as floats.
fn ffmpeg_audio(path: &str) -> Vec<f32> {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i", path, "-map", "0:a:0"])
        .args(["-af", "pan=mono|c0=c0", "-f", "f32le", "-"])
        .output()
        .unwrap()
        .stdout;
    out.as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect()
}

/// The gliding tone of `AUDIO`, as made.
fn source(rate: u32, n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let t = i as f64 / rate as f64;
            (0.5 * (2.0 * std::f64::consts::PI * (200.0 + 300.0 * t) * t).sin()) as f32
        })
        .collect()
}

/// Checks that the sound read (in pieces, as playback reads) is the tone
/// as made, late by `delay` samples everywhere: the codec's delay where
/// the file does not say it (AC-3 and MP2 in MPEG streams, AAC in MPEG-TS),
/// none where it does (MP4 edit lists, MP3's LAME tag, Opus' pre-skip).
fn check_audio(path: &str, delay: i64) {
    let info = bv_media::probe_file(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let a = info.audio.expect("audio");
    // The tone is mono (Windows' MPEG audio decoder gives it twice).
    assert!(a.channels <= 2, "{path}: {} channels", a.channels);
    let rate = a.sample_rate.0;
    let mut r = bv_media::open_audio(&info).unwrap();
    let n = rate as usize * 3 / 2;
    let mut got = Vec::with_capacity(n);
    let mut at = 0;
    while at < n {
        let k = 4410.min(n - at);
        let b = r.read(at as i64, k).unwrap();
        got.extend_from_slice(&b.channels[0]);
        at += k;
    }
    let src = source(rate, n);
    // ffmpeg's own decode of the file (first channel), asked for when the
    // tone came out of the encoder too far from the one made: then ours
    // must be ffmpeg's (FFmpeg 6's AAC encoder is less faithful than 8's).
    let mut theirs: Option<Vec<f32>> = None;
    // Where the tone is in four windows: the same delay all along.
    for w0 in [4000usize, 20000, 45000, 65000] {
        let error = |lag: i64| -> f64 {
            (w0..w0 + 4000)
                .map(|i| (got[(i as i64 + lag) as usize] as f64 - src[i] as f64).powi(2))
                .sum()
        };
        let best = (-1500..1500i64)
            .min_by(|x, y| error(*x).total_cmp(&error(*y)))
            .unwrap();
        let signal: f64 = (w0..w0 + 4000).map(|i| (src[i] as f64).powi(2)).sum();
        let db = 10.0 * (signal / error(best).max(1e-20)).log10();
        assert_eq!(
            best, delay,
            "{path}: the tone at {w0} is {best} samples late"
        );
        if db <= 25.0 {
            // The tool may start the sound elsewhere (it shifts negative
            // times): its decode at the shift that fits best.
            let theirs = theirs.get_or_insert_with(|| ffmpeg_audio(path));
            let at = |i: i64| theirs.get(i.max(0) as usize).copied().unwrap_or(0.0) as f64;
            let from = (w0 as i64 + delay) as usize;
            let off = |shift: i64| -> f64 {
                (from..from + 4000)
                    .map(|i| (got[i] as f64 - at(i as i64 + shift)).powi(2))
                    .sum()
            };
            let shift = (-2100..2100i64)
                .min_by(|x, y| off(*x).total_cmp(&off(*y)))
                .unwrap();
            let same = 10.0 * (signal / off(shift).max(1e-20)).log10();
            assert!(
                same > 50.0,
                "{path}: {db:.1} dB from the tone at {w0}, {same:.1} dB from ffmpeg's decode"
            );
        }
    }
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    println!("{name:28} audio {delay} samples late, as it should");
}

#[test]
#[ignore = "needs ffmpeg and Windows' decoders"]
fn video_containers_and_codecs() {
    let h264 = ["-c:v", "libx264", "-pix_fmt", "yuv420p"];
    // Name, ffmpeg's options, the audio codec's delay (see `check_audio`).
    let cases: Vec<(&str, Vec<&str>, usize)> = vec![
        ("h264_aac.mp4", [&h264[..], &["-c:a", "aac"]].concat(), 0),
        (
            "hevc_aac.mp4",
            vec![
                "-c:v", "libx265", "-pix_fmt", "yuv420p", "-tag:v", "hvc1", "-c:a", "aac",
            ],
            0,
        ),
        (
            "h264_pcm.mov",
            [&h264[..], &["-c:a", "pcm_s16le"]].concat(),
            0,
        ),
        (
            "prores_pcm.mov",
            vec!["-c:v", "prores_ks", "-profile:v", "2", "-c:a", "pcm_s24le"],
            0,
        ),
        ("h264_aac.mkv", [&h264[..], &["-c:a", "aac"]].concat(), 1024),
        ("h264_ac3.mkv", [&h264[..], &["-c:a", "ac3"]].concat(), 256),
        (
            "vp9_opus.webm",
            vec![
                "-c:v",
                "libvpx-vp9",
                "-deadline",
                "realtime",
                "-c:a",
                "libopus",
            ],
            0,
        ),
        (
            "av1_opus.webm",
            vec!["-c:v", "libaom-av1", "-cpu-used", "8", "-c:a", "libopus"],
            0,
        ),
        (
            "avchd.m2ts",
            [
                &h264[..],
                &[
                    "-g",
                    "12",
                    "-c:a",
                    "ac3",
                    "-f",
                    "mpegts",
                    "-mpegts_m2ts_mode",
                    "1",
                ],
            ]
            .concat(),
            256,
        ),
        (
            "h264_aac.ts",
            [&h264[..], &["-c:a", "aac", "-f", "mpegts"]].concat(),
            1024,
        ),
        (
            "mpeg2_mp2.mpg",
            vec![
                "-c:v",
                "mpeg2video",
                "-b:v",
                "4M",
                "-c:a",
                "mp2",
                "-f",
                "mpeg",
            ],
            481,
        ),
        (
            "mpeg2_ac3.vob",
            vec![
                "-c:v",
                "mpeg2video",
                "-b:v",
                "4M",
                "-c:a",
                "ac3",
                "-f",
                "vob",
            ],
            256,
        ),
        (
            "wmv2_wma.wmv",
            vec!["-c:v", "wmv2", "-b:v", "1M", "-c:a", "wmav2"],
            0,
        ),
        (
            "mjpeg_pcm.avi",
            vec![
                "-c:v",
                "mjpeg",
                "-q:v",
                "3",
                "-pix_fmt",
                "yuvj420p",
                "-c:a",
                "pcm_s16le",
            ],
            0,
        ),
    ];
    for (name, args, delay) in &cases {
        let p = make(name, &[VIDEO, AUDIO], args);
        check_frames(&p);
        // FFmpeg (Linux) applies Matroska's codec delay (the encoder's,
        // written as a time before zero), which Windows leaves in, rounded to
        // the file's milliseconds: AAC's 1024 samples as 21 ms, AC-3's 256 as
        // 5 ms, 16 samples left either way; Opus' 312 (6.5 ms) as 7 ms in
        // WebM, half a millisecond early.
        let delay = match () {
            _ if cfg!(windows) => *delay as i64,
            _ if name.ends_with(".mkv") => 16,
            _ if name.ends_with(".webm") => -24,
            _ => *delay as i64,
        };
        check_audio(&p, delay);
        println!("{name:28} frames right");
    }
}

#[test]
#[ignore = "needs ffmpeg and Windows' decoders"]
fn audio_files() {
    let cases: &[(&str, &[&str], usize)] = &[
        ("pcm.wav", &["-c:a", "pcm_s24le"], 0),
        ("pcm.w64", &["-c:a", "pcm_s16le"], 0),
        ("pcm.aiff", &["-c:a", "pcm_s16be"], 0),
        ("pcm.au", &["-c:a", "pcm_s16be"], 0),
        ("pcm.caf", &["-c:a", "pcm_s16le"], 0),
        ("mulaw.wav", &["-c:a", "pcm_mulaw"], 0),
        ("adpcm.wav", &["-c:a", "adpcm_ms"], 0),
        ("audio.mp3", &["-c:a", "libmp3lame"], 0),
        ("audio.m4a", &["-c:a", "aac"], 0),
        ("alac.m4a", &["-c:a", "alac"], 0),
        ("audio.flac", &["-c:a", "flac"], 0),
        ("vorbis.ogg", &["-c:a", "libvorbis"], 0),
        ("audio.opus", &["-c:a", "libopus"], 0),
        ("audio.wma", &["-c:a", "wmav2"], 0),
        ("audio.ac3", &["-c:a", "ac3"], 256),
    ];
    for (name, args, delay) in cases {
        let p = make(name, &[AUDIO], args);
        check_audio(&p, *delay as i64);
    }
}

#[test]
#[ignore = "needs ffmpeg and Windows' image extensions"]
fn pictures() {
    let still = "testsrc2=size=160x90:rate=1:duration=1";
    let cases: &[(&str, &[&str])] = &[
        ("photo.jpg", &["-frames:v", "1", "-q:v", "2"]),
        ("picture.bmp", &["-frames:v", "1"]),
        ("picture.tif", &["-frames:v", "1", "-pix_fmt", "rgb24"]),
        ("picture.tga", &["-frames:v", "1"]),
        (
            "picture.webp",
            &["-frames:v", "1", "-c:v", "libwebp", "-lossless", "1"],
        ),
        ("picture.png", &["-frames:v", "1"]),
        ("picture.dpx", &["-frames:v", "1"]),
        ("picture.exr", &["-frames:v", "1", "-pix_fmt", "gbrpf32le"]),
    ];
    for (name, args) in cases {
        let p = make(name, &[still], args);
        let info = bv_media::probe_file(&p).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(info.still, "{name}");
        let v = info.video.unwrap();
        assert_eq!((v.width, v.height), (160, 90), "{name}");
        let mut r = bv_media::open_video(&info).unwrap();
        let full = r.frame_at(Tick::ZERO).unwrap();
        assert_eq!((full.width, full.height), (160, 90), "{name}");
        // A preview asks for less: half the size comes.
        let half = r.frame_at_scaled(Tick::ZERO, 70, 40).unwrap();
        assert_eq!((half.width, half.height), (80, 45), "{name}: reduced");
        println!("{name:28} 160x90 still");
    }
    // An animated GIF plays: 10 frames of a tenth of a second.
    let p = make(
        "animated.gif",
        &["testsrc2=size=160x90:rate=10:duration=1"],
        &[],
    );
    let info = bv_media::probe_file(&p).unwrap();
    assert!(!info.still);
    assert_eq!(info.duration, Tick::from_seconds(1));
    let v = info.video.unwrap();
    assert_eq!((v.frame_rate.num, v.frame_rate.den), (10, 1));
    let mut r = bv_media::open_video(&info).unwrap();
    let a = r.frame_at(Tick::from_seconds_f64(0.05)).unwrap();
    let b = r.frame_at(Tick::from_seconds_f64(0.55)).unwrap();
    assert_ne!(a.data, b.data, "the animation moves");
    println!("animated.gif                 plays");
}

#[test]
#[ignore = "needs ffmpeg"]
fn refused_with_their_names() {
    let dnxhr = make(
        "dnxhr.mov",
        &[VIDEO],
        &[
            "-c:v",
            "dnxhd",
            "-profile:v",
            "dnxhr_lb",
            "-pix_fmt",
            "yuv422p",
        ],
    );
    let dts = make("audio.dts", &[AUDIO], &["-c:a", "dca", "-strict", "-2"]);
    let flv = make("video.flv", &[VIDEO], &["-c:v", "flv"]);
    if !cfg!(windows) {
        // FFmpeg reads them all.
        check_frames(&dnxhr);
        check_frames(&flv);
        let info = bv_media::probe_file(&dts).unwrap_or_else(|e| panic!("{dts}: {e}"));
        assert!(info.audio.is_some(), "DTS sound");
        println!("DNxHR, DTS and FLV read");
        return;
    }
    let refused = |p: &str| bv_media::probe_file(p).expect_err(p);
    assert_eq!(
        refused(&dnxhr),
        MediaError::UnsupportedCodec {
            codec: "DNxHR".into(),
            video: true
        }
    );
    assert_eq!(
        refused(&dts),
        MediaError::UnsupportedCodec {
            codec: "DTS".into(),
            video: false
        }
    );
    assert_eq!(refused(&flv), MediaError::UnsupportedFile("flv".into()));
}

#[test]
#[ignore = "needs ffmpeg"]
fn wide_pixels_are_read() {
    // HDV-like: 1440 × 1080 with 4 : 3 pixels, and DV widescreen in MPEG-2.
    for (name, args, want) in [
        (
            "anamorphic_h264.mp4",
            vec![
                "-vf",
                "scale=1440:1080,setsar=4/3",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
            ],
            (4, 3),
        ),
        (
            // MPEG-2 keeps 16:9 as its own pixel shape for 720 × 480: 32:27.
            "anamorphic_mpeg2.mpg",
            vec![
                "-vf",
                "scale=720:480,setsar=40/33",
                "-c:v",
                "mpeg2video",
                "-c:a",
                "mp2",
                "-f",
                "mpeg",
            ],
            (32, 27),
        ),
    ] {
        let path = make(name, &[VIDEO, AUDIO], &args);
        let info = bv_media::probe_file(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let v = info.video.expect("video");
        let par = v.pixel_aspect;
        assert!(
            (par.ratio() - want.0 as f32 / want.1 as f32).abs() < 0.01,
            "{name}: {par:?}"
        );
    }
}
