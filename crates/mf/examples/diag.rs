//! Diagnóstico: compara quadros da MF com os do ffmpeg e mede o áudio.
#![allow(clippy::print_stdout)]
use bv_core::{PlaneData, Tick};

fn planes(f: &bv_core::VideoFrame) -> Vec<u8> {
    let mut got = Vec::new();
    for p in &f.planes {
        let PlaneData::U8(d) = &p.data else { panic!() };
        for row in 0..p.height {
            got.extend_from_slice(&d[row * p.stride..row * p.stride + p.width]);
        }
    }
    got
}

fn main() {
    let path = std::env::args().nth(1).expect("arquivo");
    let dir = std::env::args().nth(2).expect("pasta com quadro_N.yuv");
    let mut dec = bv_mf::VideoDecoder::open(&path).unwrap();
    println!("{:?}", dec.info());
    for n in [0i64, 1, 44, 45, 46] {
        let t = bv_core::FrameRate::FPS_30.frame_start(n) + Tick(1000);
        let f = dec.frame_at(t).unwrap();
        let got = planes(&f);
        print!(
            "pedido {n} → pts {:.4}s cor {:?}: ",
            f.pts.as_seconds_f64(),
            f.color
        );
        for m in [n - 1, n, n + 1] {
            if let Ok(exp) = std::fs::read(format!("{dir}/ref_{m}.yuv")) {
                let diff = got.iter().zip(&exp).filter(|(a, b)| a != b).count();
                let maxd = got
                    .iter()
                    .zip(&exp)
                    .map(|(a, b)| (*a as i32 - *b as i32).abs())
                    .max()
                    .unwrap_or(0);
                print!("[vs {m}: {diff} dif, máx {maxd}] ");
            }
        }
        println!();
    }
    let mut a = bv_mf::AudioDecoder::open(&path).unwrap();
    for start in [0i64, 4800, 48000, 96000] {
        let b = a.read(start, 4800).unwrap();
        let pk = b.channels[0].iter().fold(0f32, |m, s| m.max(s.abs()));
        println!(
            "áudio em {start}: pico {pk:.4} ({} canais)",
            b.channels.len()
        );
    }
}
