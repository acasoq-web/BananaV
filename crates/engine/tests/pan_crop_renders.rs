//! BananaV's Pan/Crop measured against another program's renders: the projects
//! of the folder `BV_PANCROP_SAMPLES` (its `vegas/geometria.veg`, with the
//! frames it rendered in `vegas/geometria`, and its `mascaras.veg`, frames in
//! `vegas/mascaras`), read and drawn here frame by frame.
//!
//! `BV_PANCROP_SAMPLES=<folder> cargo test -p bv-engine --test pan_crop_renders -- --ignored --nocapture`
//!
//! `BV_PANCROP_OUT=<folder>` saves each case's worst frame, doubled up with
//! its difference.

#![allow(clippy::print_stdout)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bv_core::Tick;
use bv_engine::Engine;
use bv_timeline::veg;
use bv_timeline::{MediaInfo, Project, ProjectSettings};

/// The project of a `.veg`, its media found where it says.
fn project(path: &Path) -> Project {
    let doc = veg::read(&std::fs::read(path).expect("the project")).expect("reads");
    let found: Vec<Option<MediaInfo>> = doc
        .media
        .iter()
        .map(|m| Engine::probe_file(&m.path).ok())
        .collect();
    assert!(
        found.iter().all(Option::is_some),
        "media missing: {:?}",
        doc.media
    );
    veg::build(&doc, &found, &ProjectSettings::default(), &|_, _| None).project
}

/// The frame at `t`, over black, RGB.
fn frame(engine: &mut Engine, t: Tick, (w, h): (u32, u32)) -> Vec<u8> {
    engine.request_preview(t, w, h);
    let start = Instant::now();
    loop {
        if let Some(f) = engine.preview_frame()
            && f.exact
            && f.time == t
            && (f.width, f.height) == (w, h)
        {
            return f
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| {
                    let a = u32::from(p[3]);
                    [0, 1, 2].map(|c| ((u32::from(p[c]) * a + 127) / 255) as u8)
                })
                .collect();
        }
        assert!(start.elapsed() < Duration::from_secs(120), "drawn in time");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn rendered(folder: &Path, f: usize) -> Option<(Vec<u8>, u32, u32)> {
    let bytes = std::fs::read(folder.join(format!("quadros_{f:06}.png"))).ok()?;
    let image = bv_png::decode(&bytes).expect("a picture");
    let rgba = image.to_rgba8();
    let rgb = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    Some((rgb, image.width, image.height))
}

#[derive(Default)]
struct Tally {
    frames: u32,
    sum: u64,
    values: u64,
    worst: u8,
    /// Pixels black in one picture and not in the other.
    swapped: u64,
    /// Pixels more than two levels off, and the same.
    far: u64,
    same: u64,
}

/// A project's events one by one against the frames rendered from it.
fn compare(project: &Project, folder: &Path, save: Option<&Path>, tag: &str) -> Vec<Tally> {
    let mut engine = Engine::new(Arc::new(|| {}));
    engine.set_project(project);
    let rate = project.settings.frame_rate;
    let size = (project.settings.width, project.settings.height);
    let mut clips: Vec<_> = project
        .clips()
        .map(|(_, c)| (c.start, c.duration))
        .collect();
    clips.sort_by_key(|c| c.0);
    let mut out = Vec::new();
    for (index, (start, duration)) in clips.into_iter().enumerate() {
        let mut tally = Tally::default();
        let first = rate.frame_at(start);
        let last = rate.frame_at(start + duration);
        let mut worst_frame = (0u64, Vec::new(), Vec::new());
        for f in first..last {
            let Some((real, w, h)) = rendered(folder, f as usize) else {
                continue;
            };
            assert_eq!((w, h), size);
            let ours = frame(&mut engine, rate.frame_start(f), size);
            tally.frames += 1;
            let mut sum = 0u64;
            for (a, b) in real.as_chunks::<3>().0.iter().zip(ours.as_chunks::<3>().0) {
                let mut far = false;
                let mut same = true;
                for k in 0..3 {
                    let d = a[k].abs_diff(b[k]);
                    sum += u64::from(d);
                    tally.values += 1;
                    tally.worst = tally.worst.max(d);
                    far |= d > 2;
                    same &= d == 0;
                }
                tally.far += u64::from(far);
                tally.same += u64::from(same);
                let black = |p: &[u8]| p.iter().all(|&v| v < 8);
                tally.swapped += u64::from(black(a) != black(b));
            }
            tally.sum += sum;
            if std::env::var_os("BV_PANCROP_FRAMES").is_some() {
                println!(
                    "    event {index} frame {f}: {:.3}",
                    sum as f64 / real.len() as f64
                );
            }
            if sum >= worst_frame.0 {
                worst_frame = (sum, real, ours);
            }
        }
        if let (Some(dir), true) = (save, tally.frames > 0) {
            let (_, real, ours) = &worst_frame;
            let mut rgba = Vec::with_capacity(real.len() / 3 * 4);
            for (a, b) in real.as_chunks::<3>().0.iter().zip(ours.as_chunks::<3>().0) {
                for k in 0..3 {
                    rgba.push(a[k].abs_diff(b[k]).saturating_mul(8));
                }
                rgba.push(255);
            }
            let png = bv_png::encode_rgba8(size.0, size.1, &rgba, 6);
            let _ = std::fs::create_dir_all(dir);
            let _ = std::fs::write(dir.join(format!("{tag}_{index:02}.png")), png);
        }
        out.push(tally);
    }
    out
}

#[test]
#[ignore = "needs the folder of renders"]
fn draws_as_its_renders_show() {
    let Some(dir) = std::env::var_os("BV_PANCROP_SAMPLES").map(PathBuf::from) else {
        println!("BV_PANCROP_SAMPLES not set");
        return;
    };
    let save = std::env::var_os("BV_PANCROP_OUT").map(PathBuf::from);
    // The picture's cases.
    let p = project(&dir.join("vegas").join("geometria.veg"));
    let tallies = compare(
        &p,
        &dir.join("vegas").join("geometria"),
        save.as_deref(),
        "geometry",
    );
    println!("geometry: event, frames, mean, worst, swapped, far pixels");
    let mut worst_mean = 0.0f64;
    let mut swapped = 0;
    for (i, t) in tallies.iter().enumerate() {
        let mean = t.sum as f64 / t.values.max(1) as f64;
        println!(
            "  {i:2} {:3} {mean:.3} {:3} {:5} {:6}",
            t.frames, t.worst, t.swapped, t.far
        );
        worst_mean = worst_mean.max(mean);
        swapped += t.swapped;
    }
    println!("geometry: worst mean {worst_mean:.3}, swapped {swapped}");
    // The masks' cases: on a white picture, the mask itself.
    let p = project(&dir.join("mascaras.veg"));
    let tallies = compare(
        &p,
        &dir.join("vegas").join("mascaras"),
        save.as_deref(),
        "mask",
    );
    println!("masks: event, frames, mean, worst, pixels the same (of 230400 each)");
    let mut exact = true;
    for (i, t) in tallies.iter().enumerate() {
        let mean = t.sum as f64 / t.values.max(1) as f64;
        println!(
            "  {i:2} {:3} {mean:.4} {:3} {:8}",
            t.frames, t.worst, t.same
        );
        exact &= t.worst == 0;
    }
    println!("masks: all exact: {exact}");
    assert!(worst_mean < 1.0, "the picture lands where it did");
    assert!(exact, "the masks come out the same");
}
