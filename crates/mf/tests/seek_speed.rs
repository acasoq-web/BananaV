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

/// Asked for further on than one after another (an event sped up, faster
/// playback), and then in any other way, the frames are the ones that come
/// one by one: `BV_SEEK_FILE=<video> cargo test --release -p bv-mf --test
/// seek_speed strides -- --ignored --nocapture`.
#[test]
#[ignore = "needs a file: BV_SEEK_FILE"]
fn strides_give_the_same_frames() {
    let Ok(path) = std::env::var("BV_SEEK_FILE") else {
        return;
    };
    let hash = |f: &bv_core::VideoFrame| -> (i64, u64) {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for p in &f.planes {
            match &p.data {
                bv_core::PlaneData::U8(v) => v.hash(&mut h),
                bv_core::PlaneData::U16(v) => v.hash(&mut h),
            }
        }
        (f.pts.0, h.finish())
    };
    let mut d = bv_mf::VideoDecoder::open(&path).expect("opens");
    let rate = d.info().frame_rate;
    let count = 1200;
    let one_by_one: Vec<(i64, u64)> = (0..count)
        .map(|n| hash(&d.frame_at(rate.frame_start(n)).expect("a frame")))
        .collect();
    for stride in [2, 3, 4, 7, 16, 40, 90] {
        let mut d = bv_mf::VideoDecoder::open(&path).expect("opens");
        let start = std::time::Instant::now();
        let mut n = 5;
        let mut asked = 0;
        while n < count {
            let f = hash(&d.frame_at(rate.frame_start(n)).expect("a frame"));
            assert_eq!(f, one_by_one[n as usize], "frame {n}, every {stride}");
            n += stride;
            asked += 1;
        }
        // Back a little, on by one, and the same again.
        for back in [1, 2, 9, 8, 7, 0, 1] {
            let n = count - 1 - back;
            let f = hash(&d.frame_at(rate.frame_start(n)).expect("a frame"));
            assert_eq!(f, one_by_one[n as usize], "back to {n}");
        }
        println!(
            "every {stride}: {:.2} ms a frame",
            start.elapsed().as_secs_f64() * 1e3 / f64::from(asked)
        );
    }
}
