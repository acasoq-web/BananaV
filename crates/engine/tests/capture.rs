//! Screen recording: a display recorded into a video file of the size and
//! length asked for (skipped where there is no desktop to read).

use std::time::Duration;

use bv_core::{FrameRate, Tick};
use bv_engine::{Capture, CaptureDefaults, Engine};

#[test]
fn a_display_records_into_a_video_file() {
    let d = CaptureDefaults::default();
    let Some(mut source) = bv_engine::display_sources(&d, &|n| format!("Screen {n}"))
        .into_iter()
        .next()
    else {
        return;
    };
    // A session without a desktop (a service) has nothing to read.
    if bv_platform::grab_area(0, 0, 16, 16).is_none() {
        return;
    }
    source.width = 320;
    source.height = 180;
    source.rate = FrameRate::new(10, 1);
    let dir = std::env::temp_dir().join(format!("bv-capture-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut capture = Capture::new();
    capture.set_sources(vec![source]);
    capture.start(&dir).unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    let (files, errors) = capture.stop();
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(files.len(), 1);
    assert!(files[0].video);
    let info = Engine::probe_file(&files[0].path.to_string_lossy()).unwrap();
    let v = info.video.expect("a picture");
    assert_eq!((v.width, v.height), (320, 180));
    assert!(
        info.duration >= Tick::from_seconds_f64(1.0),
        "{}",
        info.duration.as_seconds_f64()
    );
    capture.close();
    let _ = std::fs::remove_dir_all(&dir);
}
