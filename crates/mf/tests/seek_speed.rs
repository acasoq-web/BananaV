//! How long a file's frames take to come: in order (frames a second) and
//! after seeks to times far from a keyframe — what the preview waits for
//! when playback starts at a random spot of a long-GOP recording (a 4K
//! trailer with a keyframe every 8 s decodes hundreds of frames first).
//! On the processor and on the graphics card. Run with a file:
//! `BV_SEEK_FILE=<path> cargo test -p bv-mf --test seek_speed -- --ignored --nocapture`.

#![allow(clippy::print_stdout)]

use std::time::Instant;

use bv_core::Tick;
use bv_mf::{HardwareDecoding, VideoDecoder};

#[test]
#[ignore = "needs a file: BV_SEEK_FILE"]
fn seeks_and_frames_a_second() {
    let Ok(path) = std::env::var("BV_SEEK_FILE") else {
        return;
    };
    for h in [HardwareDecoding::Auto, HardwareDecoding::Off] {
        bv_mf::set_hardware_decoding(h.clone());
        let mut d = VideoDecoder::open(&path).expect("opens");
        let rate = d.info().frame_rate;
        let start = Instant::now();
        let n = 90;
        for i in 0..n {
            d.frame_at(rate.frame_start(i)).expect("a frame");
        }
        let fps = n as f64 / start.elapsed().as_secs_f64();
        println!("{h:?}: {fps:.1} frames a second in order");
        for s in [12.0, 25.0, 45.0, 70.0, 28.5, 29.0, 36.0] {
            let start = Instant::now();
            d.frame_at(Tick::from_seconds_f64(s)).expect("a frame");
            println!(
                "{h:?}: frame at {s} s after {} ms",
                start.elapsed().as_millis()
            );
        }
    }
}
