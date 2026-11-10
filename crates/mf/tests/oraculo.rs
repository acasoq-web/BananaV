//! Compara a decodificação da Media Foundation com a do ffmpeg (oráculo).
//! Precisa do `ffmpeg` no PATH; rodar com `cargo test -p bv-mf -- --ignored`.

#![cfg(any(
    windows,
    all(any(target_os = "linux", target_os = "macos"), not(bv_no_ffmpeg))
))]

use std::path::PathBuf;
use std::process::Command;

use bv_core::{PlaneData, Tick};

fn media_dir() -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/generated");
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn ffmpeg(args: &[&str]) {
    let st = Command::new("ffmpeg")
        .args(["-v", "error", "-y"])
        .args(args)
        .status()
        .expect("ffmpeg no PATH");
    assert!(st.success(), "ffmpeg {args:?}");
}

/// MP4 de 3 s, 640×360, 30 fps, H.264 High + AAC estéreo 48 kHz.
fn sample_mp4() -> PathBuf {
    let p = media_dir().join("teste_h264_aac.mp4");
    if !p.exists() {
        ffmpeg(&[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=s=640x360:r=30:d=3",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=1000:sample_rate=48000:d=3",
            "-c:v",
            "libx264",
            "-profile:v",
            "high",
            "-pix_fmt",
            "yuv420p",
            "-g",
            "30",
            "-bf",
            "2",
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            "-ac",
            "2",
            p.to_str().unwrap(),
        ]);
    }
    p
}

#[test]
#[ignore = "precisa do ffmpeg"]
fn probe_le_streams_e_duracao() {
    let p = bv_mf::probe(sample_mp4().to_str().unwrap()).unwrap();
    let v = p.video.unwrap();
    assert_eq!((v.width, v.height), (640, 360));
    assert_eq!(v.frame_rate.as_f64().round(), 30.0);
    let a = p.audio.unwrap();
    assert_eq!(a.sample_rate.0, 48000);
    assert!(
        (p.duration.as_seconds_f64() - 3.0).abs() < 0.1,
        "{}",
        p.duration
    );
}

#[test]
#[ignore = "precisa do ffmpeg"]
fn quadros_iguais_aos_do_ffmpeg() {
    let path = sample_mp4();
    let mut dec = bv_mf::VideoDecoder::open(path.to_str().unwrap()).unwrap();
    // Quadros fora de ordem para exercitar busca e decodificação adiante.
    for frame_no in [45i64, 0, 1, 2, 60, 89, 30, 31] {
        let t = bv_core::FrameRate::FPS_30.frame_start(frame_no) + Tick(1000);
        let f = dec.frame_at(t).unwrap();
        let raw = media_dir().join(format!("quadro_{frame_no}.yuv"));
        ffmpeg(&[
            "-i",
            path.to_str().unwrap(),
            "-vf",
            &format!("select=eq(n\\,{frame_no})"),
            "-frames:v",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "yuv420p",
            raw.to_str().unwrap(),
        ]);
        let expected = std::fs::read(&raw).unwrap();
        let mut got = Vec::new();
        for p in &f.planes {
            let PlaneData::U8(d) = &p.data else {
                panic!("8 bits")
            };
            for row in 0..p.height {
                got.extend_from_slice(&d[row * p.stride..row * p.stride + p.width]);
            }
        }
        assert_eq!(got.len(), expected.len(), "quadro {frame_no}");
        let diff = got.iter().zip(&expected).filter(|(a, b)| a != b).count();
        // Decodificadores H.264 são bit-exatos: nenhum pixel deve diferir.
        assert_eq!(diff, 0, "quadro {frame_no}: {diff} amostras diferentes");
    }
}

#[test]
#[ignore = "precisa do ffmpeg"]
fn audio_tem_o_tom_e_acesso_aleatorio() {
    let path = sample_mp4();
    let mut dec = bv_mf::AudioDecoder::open(path.to_str().unwrap()).unwrap();
    assert_eq!(dec.info().sample_rate.0, 48000);
    let mid = dec.read(48000, 4800).unwrap();
    let peak = mid.channels[0].iter().fold(0f32, |m, s| m.max(s.abs()));
    // Senoide de amplitude 1/8, mono → estéreo pelo ffmpeg a −3 dB: ≈ 0,088.
    assert!((peak - 0.0884).abs() < 0.01, "pico {peak}");
    // Ler de novo o mesmo trecho (depois de ir para outro lugar) dá o mesmo.
    let _ = dec.read(120_000, 1000).unwrap();
    let again = dec.read(48000, 4800).unwrap();
    let max_diff = mid.channels[0]
        .iter()
        .zip(&again.channels[0])
        .fold(0f32, |m, (a, b)| m.max((a - b).abs()));
    assert!(max_diff < 1e-4, "diferença {max_diff}");
}
