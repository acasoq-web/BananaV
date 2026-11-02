//! Mede leitura de vídeo (decodificar + converter) em sequência, como no playback.
#![allow(clippy::print_stdout)]
use std::time::Instant;

use bv_core::Tick;

fn main() {
    let path = std::env::args().nth(1).expect("arquivo");
    let info = bv_media::probe_file(&path).expect("probe");
    println!("{:?} {:?} {}", info.video, info.audio, info.duration);
    let mut r = bv_media::open_video(&info).expect("vídeo");
    let rate = info.video.unwrap().frame_rate;
    let mut start = 30;
    for (label, w, h) in [
        ("cheia", u32::MAX, u32::MAX),
        ("prévia 960", 960, 540),
        ("prévia 640", 640, 360),
    ] {
        let t0 = Instant::now();
        let n = 90;
        for i in 0..n {
            let _ = r
                .frame_at_scaled(rate.frame_start(start + i), w, h)
                .unwrap();
        }
        start += n;
        let ms = t0.elapsed().as_secs_f64() * 1000.0 / n as f64;
        println!("{label}: {ms:.2} ms/quadro ({:.0} fps)", 1000.0 / ms);
    }
    let t0 = Instant::now();
    let _ = r.frame_at(Tick::from_seconds(1)).unwrap();
    println!(
        "busca para 1 s: {:.1} ms",
        t0.elapsed().as_secs_f64() * 1000.0
    );
}
