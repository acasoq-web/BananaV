//! Video proxies: made from a recording by the system's H.264 encoder,
//! found again, read by previews below full size, never by renders. Needs
//! ffmpeg: `cargo test -p bv-engine --test proxies -- --ignored`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bv_core::Tick;
use bv_media::{Ask, VideoPool, Want};

fn source() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/generated/proxy");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("recording_1080p.mp4");
    if !p.exists() {
        let st = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg("testsrc2=size=1920x1080:rate=30:duration=2")
            .args(["-c:v", "libx264", "-pix_fmt", "yuv420p"])
            .arg(&p)
            .status()
            .expect("ffmpeg on the PATH");
        assert!(st.success());
    }
    p
}

#[test]
#[ignore = "needs ffmpeg"]
fn a_proxy_is_made_found_and_read_by_previews() {
    let path = source();
    let original = path.to_string_lossy().to_string();
    let proxy = bv_engine::proxy_path(&original);
    let _ = std::fs::remove_file(&proxy);
    bv_media::forget_proxy(&original);
    let mut info = bv_engine::Engine::probe_file(&original).expect("probe");
    info.id = bv_timeline::MediaId(1);
    let mut engine = bv_engine::Engine::new(Arc::new(|| {}));
    let fallback = std::env::temp_dir().join("bananav-proxy-test");
    assert!(engine.create_proxy(&info, &fallback));
    let start = Instant::now();
    let done = loop {
        let (_, done) = engine.proxies();
        if let Some(d) = done.into_iter().next() {
            break d;
        }
        assert!(start.elapsed() < Duration::from_secs(120), "made in time");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(done, (original.clone(), Ok(())));
    assert!(proxy.exists(), "next to the recording");
    let made = bv_engine::Engine::probe_file(&proxy.to_string_lossy()).expect("a video");
    let v = made.video.unwrap();
    assert_eq!((v.width, v.height), (1280, 720));
    assert!((made.duration.as_seconds_f64() - 2.0).abs() < 0.1);

    // Found again in another session.
    bv_media::forget_proxy(&original);
    assert!(bv_engine::Engine::find_proxy(&info, &fallback));

    // A preview pool reads the proxy, a render's reads the recording.
    let ask = |pool: &mut VideoPool| {
        pool.begin_frame();
        let asked = Ask {
            time: Tick::from_seconds(1),
            min_size: (1280, 720),
            want: Want::Linear,
            span: None,
            video_levels: false,
            eight_bit: false,
            encoded: true,
        };
        match pool.picture(1, &info, asked) {
            Ok(bv_media::Picture::Linear(img)) => img.width,
            Ok(_) => panic!("not linear"),
            Err(e) => panic!("{e}"),
        }
    };
    let mut preview = VideoPool::new();
    preview.set_proxies(true);
    assert_eq!(ask(&mut preview), 1280, "the proxy");
    let mut render = VideoPool::new();
    assert_eq!(ask(&mut render), 1920, "the recording");

    bv_engine::Engine::delete_proxy(&info);
    assert!(!proxy.exists());
}
