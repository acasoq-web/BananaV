//! For measuring BananaV's own effects against another program's
//! pictures: a picture (`BV_FRAMES_PICTURE`) drawn by the engine as the
//! Video Preview shows it (8 bits, BT.709) into `BV_FRAMES_OUT`, without
//! effects (`none.png`) and through
//! - each factory preset of the effects `BV_FRAMES_EFFECTS` names (plug-in
//!   ids, commas between), as `<effect>_<preset>.png`; or
//! - the scenarios of the file `BV_FRAMES_SCENARIOS`, a line each: name,
//!   plug-in id, preset (`-`: none) and values over it (`name=1.5,0.2;…`),
//!   tabs between, as `<name>.png`.
//!
//! The project is a new one's (`BV_FRAMES_FORMAT=32`: 32-bit floating
//! point, linear; `BV_FRAMES_VIEW=sRGB/ACES`: under ACES, through that
//! display's view; `BV_FRAMES_GAMMA=video`: 32-bit with the 2.222
//! compositing gamma; `BV_FRAMES_MEDIA_SPACE`: the picture's color space
//! under ACES, as Media Properties lists it), `BV_FRAMES_SIZE` big
//! (`1920x1080` by default). `BV_FRAMES_BINARIES` (paths, `;` between)
//! loads other makers' plug-in binaries beside BananaV's own, to draw
//! them too (the libraries they need on `PATH`).
//! `cargo test --release -p bv-engine --test effect_frames -- --ignored`

#![allow(clippy::print_stdout)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bv_core::Tick;
use bv_engine::Engine;
use bv_engine::plugins::{PresetValue, own_presets};
use bv_timeline::{
    AcesDisplay, AcesLook, AcesSpace, AcesView, ColorSpace, CompositingGamma, Effect, PixelFormat,
    PluginFx, PluginParam, PluginValue, Project, ProjectSettings, ViewTransform,
};

/// The preview of `p` at its start, `w` × `h` RGBA.
fn frame(p: &Project, (w, h): (u32, u32)) -> Vec<u8> {
    let mut engine = Engine::new(Arc::new(|| {}));
    engine.set_project(p);
    engine.request_preview(Tick::ZERO, w, h);
    let start = Instant::now();
    loop {
        if let Some(f) = engine.preview_frame()
            && f.exact
            && (f.width, f.height) == (w, h)
        {
            return f.rgba.clone();
        }
        assert!(start.elapsed() < Duration::from_secs(60), "drawn in time");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn file_name(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// Effect `id` with `preset`'s values (if any), then `values`.
fn effect(id: &str, preset: Option<&str>, values: &[(String, Vec<f64>)]) -> Effect {
    let mut params = BTreeMap::new();
    let mut set = |k: &str, value: PluginValue| {
        params.insert(
            k.to_string(),
            PluginParam {
                value,
                keys: Vec::new(),
            },
        );
    };
    if let Some(name) = preset {
        let p = own_presets(id)
            .into_iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("{id} has no preset {name}"));
        for (k, value) in p.values {
            set(
                k,
                match value {
                    PresetValue::Num(n) => PluginValue::Num(n),
                    PresetValue::Curves(c) => PluginValue::Curves(c),
                    PresetValue::Text(t) => PluginValue::Text(t),
                },
            );
        }
    }
    for (k, v) in values {
        set(k, PluginValue::Num(v.clone()));
    }
    Effect::plugin(PluginFx {
        id: id.into(),
        version: [1, 0],
        name: id.into(),
        params,
    })
}

/// A scenarios file's line: name, effect, preset, values.
fn scenario(line: &str) -> (String, Effect) {
    let cols: Vec<&str> = line.split('\t').collect();
    assert!(cols.len() >= 3, "name, plug-in id and preset: {line}");
    let preset = (cols[2] != "-").then_some(cols[2]);
    let values: Vec<(String, Vec<f64>)> = cols
        .get(3)
        .unwrap_or(&"")
        .split(';')
        .filter(|s| !s.trim().is_empty())
        .map(|kv| {
            let (k, v) = kv.split_once('=').expect("name=values");
            let nums = v
                .split(',')
                .map(|n| n.trim().parse().expect("a number"))
                .collect();
            (k.trim().to_string(), nums)
        })
        .collect();
    (cols[0].to_string(), effect(cols[1], preset, &values))
}

#[test]
#[ignore = "writes frames for measuring effects against pictures"]
fn effect_frames() {
    let (Ok(picture), Ok(out)) = (
        std::env::var("BV_FRAMES_PICTURE"),
        std::env::var("BV_FRAMES_OUT"),
    ) else {
        println!("BV_FRAMES_PICTURE and BV_FRAMES_OUT are not set: nothing to do");
        return;
    };
    std::fs::create_dir_all(&out).expect("the output folder");
    let binaries = std::env::var("BV_FRAMES_BINARIES").unwrap_or_default();
    for path in binaries.split(';').filter(|p| !p.is_empty()) {
        let added = bv_ofx::Binary::load(std::path::Path::new(path))
            .and_then(|b| bv_ofx::registry().add_built_in(b));
        if let Err(e) = added {
            println!("{path}: {e}");
        }
    }
    let size = std::env::var("BV_FRAMES_SIZE")
        .ok()
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or((1920, 1080));
    let mut settings = ProjectSettings {
        width: size.0,
        height: size.1,
        ..ProjectSettings::default()
    };
    if std::env::var("BV_FRAMES_FORMAT").as_deref() == Ok("32") {
        settings.pixel_format = PixelFormat::Float32Full;
    }
    if std::env::var("BV_FRAMES_GAMMA").as_deref() == Ok("video") {
        settings.pixel_format = PixelFormat::Float32Full;
        settings.compositing_gamma = CompositingGamma::Video;
    }
    // ACES: `BV_FRAMES_VIEW` a display and view ("sRGB/ACES"), in the
    // working space `BV_FRAMES_ACES_SPACE` names (ACES2065-1 by default).
    if let Ok(view) = std::env::var("BV_FRAMES_VIEW") {
        let (display, kind) = view.split_once('/').unwrap_or((&view, "ACES"));
        settings.pixel_format = PixelFormat::Float32Full;
        settings.color.view = Some(ViewTransform {
            display: *AcesDisplay::ALL
                .iter()
                .find(|d| d.name() == display)
                .expect("a display"),
            view: *AcesView::ALL
                .iter()
                .find(|v| v.name() == kind)
                .expect("a view"),
        });
        if let Ok(look) = std::env::var("BV_FRAMES_LOOK") {
            settings.color.look = *AcesLook::ALL
                .iter()
                .find(|l| l.name() == Some(look.as_str()))
                .expect("a look");
        }
        if let Ok(space) = std::env::var("BV_FRAMES_ACES_SPACE") {
            settings.color.space = *AcesSpace::ALL
                .iter()
                .find(|s| s.name() == space)
                .expect("an ACES space");
        }
    }
    let mut base = Project::new(settings);
    let mut info = Engine::probe_file(&picture).expect("the picture");
    if let Ok(space) = std::env::var("BV_FRAMES_MEDIA_SPACE") {
        info.color_space = Some(
            *ColorSpace::ALL
                .iter()
                .find(|c| c.name() == space)
                .expect("a color space"),
        );
    }
    let m = base.add_media(info);
    let v = base.tracks[0].id;
    let clip = base.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
    let save = |name: &str, rgba: &[u8]| {
        let png = bv_png::encode_rgba8(size.0, size.1, rgba, 6);
        let path = std::path::Path::new(&out).join(format!("{}.png", file_name(name)));
        std::fs::write(&path, png).expect("written");
        println!("{}", path.display());
    };
    save("none", &frame(&base, size));
    let mut jobs: Vec<(String, Effect)> = Vec::new();
    if let Ok(file) = std::env::var("BV_FRAMES_SCENARIOS") {
        let text = std::fs::read_to_string(&file).expect("the scenarios");
        jobs.extend(
            text.lines()
                .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
                .map(scenario),
        );
    }
    let effects = std::env::var("BV_FRAMES_EFFECTS").unwrap_or_default();
    for id in effects.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let short = id.rsplit('.').next().unwrap_or(id);
        for preset in own_presets(id) {
            jobs.push((
                format!("{short}_{}", preset.name),
                effect(id, Some(preset.name), &[]),
            ));
        }
    }
    for (name, fx) in jobs {
        let mut p = base.clone();
        p.clip_mut(clip).unwrap().effects = vec![fx];
        save(&name, &frame(&p, size));
    }
    // Plug-ins close what they started.
    bv_ofx::unload_all(Duration::from_secs(5));
}
