//! The VST 3 host against the test plug-ins (`bv-vst3-testplug`): linked
//! into the tests, and built and loaded from a bundle as a vendor ships
//! one.

// The editor's test makes a window of its own (Windows' functions).
#![allow(unsafe_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(windows)]
use std::sync::Mutex;

use bv_vst3::{Instance, Module};
use bv_vst3_testplug as plug;

fn module() -> Arc<Module> {
    bv_vst3::linked("testplug", plug::GetPluginFactory).expect("the test plug-ins")
}

fn instance(name: &str, controller: bool) -> Instance {
    let m = module();
    let class = m
        .effects()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no {name}"))
        .clone();
    Instance::new(&m, &class, controller).expect("it starts")
}

fn tone(n: usize) -> Vec<f32> {
    (0..n).map(|i| (i as f32 * 0.05).sin() * 0.5).collect()
}

#[test]
fn a_module_lists_its_effects() {
    let m = module();
    assert_eq!(m.vendor(), "BananaV Tests");
    assert_eq!(m.classes().len(), 3);
    let names: Vec<&str> = m.effects().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["BananaV Test Gain", "BananaV Test Polarity"]);
    let gain = m.effect("").expect("the first effect");
    assert_eq!(gain.name, "BananaV Test Gain");
    // "BANANAVTESTGAIN1".
    assert_eq!(gain.id(), "42414E414E4156544553544741494E31");
    assert_eq!(m.effect(&gain.id()).map(|c| c.cid), Some(plug::GAIN));
    assert_eq!(gain.sub_categories, "Fx|Tools");
    assert!(m.effect("00000000000000000000000000000000").is_none());
}

#[test]
fn the_gain_processes_and_follows_its_parameters() {
    let mut g = instance("BananaV Test Gain", true);
    let params = g.params();
    assert_eq!(params.len(), 3);
    assert_eq!(params[0].title, "Gain");
    assert_eq!(params[0].units, "dB");
    assert!(params[1].bypass());
    assert!(!params[2].shown());
    assert_eq!(g.text(plug::GAIN_ID, 0.25).as_deref(), Some("-6.0"));
    g.setup(48_000.0, 512, 2).expect("stereo");
    let x = tone(300);
    let mut ch = vec![x.clone(), x.clone()];
    g.process(&mut ch);
    assert_eq!(ch[0], x, "unity at the middle");
    g.set(plug::GAIN_ID, 0.25);
    assert_eq!(g.value(plug::GAIN_ID), Some(0.25));
    let mut ch = vec![x.clone(), x.clone()];
    g.process(&mut ch);
    for (a, b) in ch[1].iter().zip(&x) {
        assert!((a - b * 0.5).abs() < 1e-6);
    }
    // Its bypass.
    g.set(plug::BYPASS_ID, 1.0);
    let mut ch = vec![x.clone(), x.clone()];
    g.process(&mut ch);
    assert_eq!(ch[0], x);
    // Its state comes back in another instance.
    let state = g.state().expect("a state");
    assert_eq!(state.len(), 16);
    let mut other = instance("BananaV Test Gain", true);
    assert!(other.set_state(&state, &[]));
    assert_eq!(other.value(plug::GAIN_ID), Some(0.25));
    assert_eq!(other.value(plug::BYPASS_ID), Some(1.0));
}

#[test]
fn queued_changes_reach_the_state_without_sound() {
    let mut g = instance("BananaV Test Gain", false);
    g.setup(44_100.0, 256, 2).unwrap();
    g.queue(plug::GAIN_ID, 0.1);
    g.flush();
    let state = g.state().unwrap();
    assert_eq!(f64::from_le_bytes(state[..8].try_into().unwrap()), 0.1);
    // Long blocks go through in pieces.
    g.setup(44_100.0, 64, 1).expect("mono");
    let x = tone(1000);
    let mut ch = vec![x.clone()];
    g.process(&mut ch);
    for (a, b) in ch[0].iter().zip(&x) {
        assert!((a - b * 0.2).abs() < 1e-6);
    }
}

#[test]
fn a_controller_apart_is_connected_and_hears_the_component() {
    let mut p = instance("BananaV Test Polarity", true);
    // The component's message, made by the host, reached the controller.
    assert_eq!(p.value(plug::HELLO_ID), Some(0.3));
    assert_eq!(p.params().iter().filter(|i| i.shown()).count(), 1);
    // Stereo only: one channel goes in on both sides, out as their mean.
    p.setup(48_000.0, 512, 1).unwrap();
    p.set(plug::INVERT_ID, 1.0);
    let x = tone(100);
    let mut ch = vec![x.clone()];
    p.process(&mut ch);
    for (a, b) in ch[0].iter().zip(&x) {
        assert!((a + b).abs() < 1e-6);
    }
    // Five channels: the plug-in's two work, the others pass.
    let mut five = vec![x.clone(); 5];
    p.process(&mut five);
    assert!((five[0][10] + x[10]).abs() < 1e-6);
    assert_eq!(five[4], x);
    // The controller reads the component's state.
    let state = p.state().unwrap();
    let other = {
        let mut o = instance("BananaV Test Polarity", true);
        o.set_state(&state, &[]);
        o
    };
    assert_eq!(other.value(plug::INVERT_ID), Some(1.0));
    assert_eq!(
        other.text(plug::INVERT_ID, 1.0).as_deref(),
        Some("Inverted")
    );
}

#[cfg(windows)]
mod window {
    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn CreateWindowExW(
            ex: u32,
            class: *const u16,
            name: *const u16,
            style: u32,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            parent: isize,
            menu: isize,
            instance: isize,
            param: *mut std::ffi::c_void,
        ) -> isize;
        pub fn DestroyWindow(hwnd: isize) -> i32;
    }
}

/// The gain's editor in a window of the test's (never shown): it asks
/// its frame for another size once attached, and keeps to its smallest.
#[cfg(windows)]
#[test]
fn an_editor_lives_in_the_window_it_is_given() {
    let mut g = instance("BananaV Test Gain", true);
    let ed = g.editor().expect("an editor");
    assert!(ed.supports(bv_vst3::PLATFORM_HWND));
    assert!(!ed.supports(bv_vst3::PLATFORM_X11));
    assert_eq!(ed.size(), Some(plug::EDITOR_START));
    assert!(ed.resizable());
    let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
    // SAFETY: a hidden window of the test's own.
    let parent = unsafe {
        window::CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            400,
            300,
            0,
            0,
            0,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(parent, 0);
    let asked = Arc::new(Mutex::new(Vec::new()));
    let note = Arc::clone(&asked);
    // SAFETY: the window lives until after the editor is detached.
    let ok = unsafe {
        ed.attach(
            parent as *mut std::ffi::c_void,
            bv_vst3::PLATFORM_HWND,
            Box::new(move |w, h| note.lock().unwrap().push((w, h))),
        )
    };
    assert!(ok);
    assert_eq!(*asked.lock().unwrap(), [plug::EDITOR_ASKED]);
    assert_eq!(ed.size(), Some(plug::EDITOR_ASKED));
    assert_eq!(ed.resized(100, 50), (200, 80));
    ed.detach();
    g.close_editor();
    // SAFETY: the test's window.
    unsafe { window::DestroyWindow(parent) };
}

/// Builds the test plug-ins and puts them in `folder` as a VST 3 bundle;
/// the bundle's path.
fn bundle_in(folder: &Path) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = root.join("target").join("vst3-testplug");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = std::process::Command::new(cargo)
        .current_dir(&root)
        .args(["build", "-q", "-p", "bv-vst3-testplug", "--target-dir"])
        .arg(&target)
        .status()
        .expect("cargo runs");
    assert!(status.success());
    let built = target.join("debug").join(if cfg!(windows) {
        "bv_vst3_testplug.dll"
    } else {
        "libbv_vst3_testplug.so"
    });
    let bundle = folder.join("BananaV Tests.vst3");
    let library = bv_vst3::library_of(&{
        std::fs::create_dir_all(&bundle).unwrap();
        bundle.clone()
    });
    std::fs::create_dir_all(library.parent().unwrap()).unwrap();
    std::fs::copy(&built, &library).unwrap();
    bundle
}

#[test]
fn a_bundle_loads_as_a_vendor_ships_it() {
    let dir = std::env::temp_dir().join(format!("bananav-vst3-bundle-{}", std::process::id()));
    let bundle = bundle_in(&dir);
    let m = bv_vst3::load(&bundle).expect("it loads");
    assert_eq!(m.effects().count(), 2);
    // Loaded once.
    assert!(Arc::ptr_eq(&m, &bv_vst3::load(&bundle).unwrap()));
    let class = m.effect("").unwrap().clone();
    let mut g = Instance::new(&m, &class, true).unwrap();
    g.setup(48_000.0, 128, 2).unwrap();
    g.set(plug::GAIN_ID, 0.0);
    let mut ch = vec![tone(64), tone(64)];
    g.process(&mut ch);
    assert!(ch[0].iter().all(|&s| s == 0.0));
    drop(g);
    // The library stays loaded: the folder cannot go on Windows.
    let _ = std::fs::remove_dir_all(&dir);
    // A folder that is no module.
    assert!(bv_vst3::load(&dir.join("missing.vst3")).is_err());
}
