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
    assert_eq!(m.classes().len(), 4);
    let names: Vec<&str> = m.effects().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "BananaV Test Gain",
            "BananaV Test Polarity",
            "BananaV Test Programs"
        ]
    );
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
    use std::env::consts::{DLL_PREFIX, DLL_SUFFIX};
    let built = target
        .join("debug")
        .join(format!("{DLL_PREFIX}bv_vst3_testplug{DLL_SUFFIX}"));
    let bundle = folder.join("BananaV Tests.vst3");
    std::fs::create_dir_all(&bundle).unwrap();
    mac_info_plist(&bundle);
    // On a Mac, Core Foundation is not asked before the executable is in:
    // it keeps a bundle as it first saw it for as long as the process runs.
    let library = if cfg!(target_os = "macos") {
        bundle.join("Contents").join("MacOS").join("BananaV Tests")
    } else {
        bv_vst3::library_of(&bundle)
    };
    std::fs::create_dir_all(library.parent().unwrap()).unwrap();
    std::fs::copy(&built, &library).unwrap();
    bundle
}

/// A Mac bundle's Info.plist naming its executable, as vendors ship one
/// (Core Foundation finds the executable through it); nothing elsewhere.
fn mac_info_plist(bundle: &Path) {
    if !cfg!(target_os = "macos") {
        return;
    }
    let contents = bundle.join("Contents");
    std::fs::create_dir_all(&contents).unwrap();
    let name = bundle.file_stem().unwrap().to_string_lossy();
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>{name}</string>
    <key>CFBundleIdentifier</key>
    <string>org.bananav.tests.vst3</string>
    <key>CFBundlePackageType</key>
    <string>BNDL</string>
</dict>
</plist>
"#
    );
    std::fs::write(contents.join("Info.plist"), plist).unwrap();
}

#[test]
fn a_bundle_loads_as_a_vendor_ships_it() {
    let dir = std::env::temp_dir().join(format!("bananav-vst3-bundle-{}", std::process::id()));
    let bundle = bundle_in(&dir);
    let m = bv_vst3::load(&bundle).expect("it loads");
    assert_eq!(m.effects().count(), 3);
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

/// The value of the 8 bytes at `at` of a state.
fn f64_at(state: &[u8], at: usize) -> f64 {
    f64::from_le_bytes(state[at..at + 8].try_into().unwrap())
}

fn i64_at(state: &[u8], at: usize) -> i64 {
    i64::from_le_bytes(state[at..at + 8].try_into().unwrap())
}

#[test]
fn programs_come_from_the_plug_ins_program_list() {
    let mut p = instance("BananaV Test Programs", true);
    assert_eq!(p.programs(), ["Unity", "Half", "Silent"]);
    assert_eq!(p.program(), Some(0));
    p.setup(48_000.0, 512, 2).unwrap();
    assert!(p.set_program(1));
    assert_eq!(p.program(), Some(1));
    // The program brought its gain, to the controller and the processor.
    assert_eq!(p.value(plug::GAIN_ID), Some(0.25));
    let x = tone(200);
    let mut ch = vec![x.clone(), x.clone()];
    p.process(&mut ch);
    for (a, b) in ch[0].iter().zip(&x) {
        assert!((a - b * 0.5).abs() < 1e-6);
    }
    assert!(!p.set_program(3), "no fourth program");
    // The program is in its state.
    let state = p.state().unwrap();
    let other = {
        let mut o = instance("BananaV Test Programs", true);
        assert!(o.set_state(&state, &[]));
        o
    };
    assert_eq!(other.program(), Some(1));
    // An effect without programs has none.
    let mut g = instance("BananaV Test Gain", true);
    assert!(g.programs().is_empty());
    assert_eq!(g.program(), None);
    assert!(!g.set_program(0));
}

#[test]
fn tails_are_told() {
    let p = instance("BananaV Test Programs", false);
    assert_eq!(p.tail(), Some(plug::TAIL));
    assert_eq!(instance("BananaV Test Gain", false).tail(), Some(0));
    assert_eq!(p.latency(), 0);
}

/// A ramp's points reach the plug-in at their samples, across the blocks
/// a long call is cut into.
#[test]
fn ramps_change_parameters_at_their_samples() {
    let mut g = instance("BananaV Test Gain", false);
    g.setup(48_000.0, 64, 1).unwrap();
    g.queue_ramp(plug::GAIN_ID, &[(0, 0.5), (100, 0.25), (150, 0.0)]);
    let x = tone(200);
    let mut ch = vec![x.clone()];
    g.process(&mut ch);
    for (i, (a, b)) in ch[0].iter().zip(&x).enumerate() {
        let k = match i {
            0..100 => 1.0,
            100..150 => 0.5,
            _ => 0.0,
        };
        assert!((a - b * k).abs() < 1e-6, "sample {i}: {a} for {}", b * k);
    }
    // Spent: the next call keeps the last value.
    let mut ch = vec![x.clone()];
    g.process(&mut ch);
    assert!(ch[0].iter().all(|&s| s == 0.0));
}

#[test]
fn the_timeline_place_and_tempo_reach_the_plug_in() {
    use bv_vst3_sys::context;
    let mut p = instance("BananaV Test Programs", false);
    p.setup(48_000.0, 512, 2).unwrap();
    p.set_transport(bv_vst3::Transport {
        playing: true,
        tempo: 90.0,
        numerator: 3,
        denominator: 4,
        cycle: None,
    });
    // One second in.
    p.locate(48_000);
    let mut ch = vec![tone(256), tone(256)];
    p.process(&mut ch);
    let state = p.state().unwrap();
    assert_eq!(f64_at(&state, 24), 90.0, "tempo");
    assert!((f64_at(&state, 32) - 1.5).abs() < 1e-9, "quarter notes");
    assert_eq!(i64_at(&state, 40), 48_000, "samples");
    let flags = i64_at(&state, 48) as u32;
    for f in [
        context::PLAYING,
        context::TEMPO_VALID,
        context::PROJECT_TIME_MUSIC_VALID,
        context::TIME_SIG_VALID,
        context::BAR_POSITION_VALID,
        context::CONT_TIME_VALID,
    ] {
        assert_ne!(flags & f, 0, "flag {f:#x}");
    }
    // The next block follows on.
    let mut ch = vec![tone(256), tone(256)];
    p.process(&mut ch);
    assert_eq!(i64_at(&p.state().unwrap(), 40), 48_256);
}

/// A preset file of BananaV's comes back in another instance.
#[test]
fn a_preset_file_brings_the_plug_in_back() {
    let mut p = instance("BananaV Test Programs", true);
    assert!(p.set_program(2));
    let class = p.class().clone();
    let bytes = bv_vst3::preset::write(
        &class.id(),
        &p.state().unwrap(),
        &p.controller_state().unwrap_or_default(),
        &bv_vst3::preset::Info {
            plugin: class.name.clone(),
            category: class.sub_categories.clone(),
            name: "Quiet".into(),
        },
    );
    let preset = bv_vst3::preset::read(&bytes).expect("a preset");
    assert!(preset.is_for(&class.id()));
    let mut other = instance("BananaV Test Programs", true);
    assert!(other.set_state(&preset.component, &preset.controller));
    assert_eq!(other.program(), Some(2));
    // Not for another effect.
    assert!(!preset.is_for(&bv_vst3_sys::uid_to_hex(&plug::GAIN)));
}

/// A plug-in that crashes as it processes: the crash comes back as a
/// failure, the sound goes through as it came, its module is switched off
/// (never called again, even to end) and the crash is told. Windows only
/// (the guard is structured exception handling).
#[cfg(all(windows, target_env = "msvc"))]
#[test]
fn a_plug_in_that_crashes_is_switched_off_not_the_host() {
    let m = bv_vst3::linked("crashplug", plug::GetPluginFactoryCrash).expect("its module");
    let class = m.effects().next().expect("its effect").clone();
    assert_eq!(class.name, "BananaV Test Crash");
    let mut c = Instance::new(&m, &class, true).expect("made");
    c.setup(48_000.0, 128, 2).expect("set up");
    let x = tone(300);
    let mut ch = vec![x.clone(), x.clone()];
    c.process(&mut ch);
    assert_eq!(ch[0], x, "the sound went through");
    assert!(c.crashed() && m.crashed(), "switched off");
    let crashes = bv_vst3::take_crashes();
    let crash = crashes
        .iter()
        .find(|k| k.module == Path::new("<linked>/crashplug"))
        .unwrap_or_else(|| panic!("{crashes:?}"));
    assert_eq!(crash.doing, "process");
    assert_eq!(crash.code, 0xC000_0005, "an access violation");
    assert_eq!(crash.effect, "BananaV Test Crash");
    // Not called again: no parameters, the sound untouched.
    assert!(c.params().is_empty());
    let mut ch = vec![x.clone(), x.clone()];
    c.process(&mut ch);
    assert_eq!(ch[1], x);
    drop(c);
    assert!(matches!(
        Instance::new(&m, &class, false),
        Err(bv_vst3::Error::Crashed)
    ));
    // Its neighbours go on.
    let mut g = instance("BananaV Test Gain", false);
    g.setup(48_000.0, 128, 2).unwrap();
    assert!(!g.crashed());
}

fn scanner(env: &[(&str, &str)]) -> bv_vst3::scan::Scanner {
    bv_vst3::scan::Scanner {
        exe: PathBuf::from(env!("CARGO_BIN_EXE_bv-vst3-scan")),
        args: Vec::new(),
        bridge: None,
        env: env
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        timeout: std::time::Duration::from_secs(120),
    }
}

/// Modules looked at in a process of their own: their effects tried and
/// described; one that crashes there costs that process only.
#[test]
fn modules_are_looked_at_in_a_process_of_their_own() {
    use bv_vst3::scan::Failure;
    let dir = std::env::temp_dir().join(format!("bananav-vst3-scan-{}", std::process::id()));
    let bundle = bundle_in(&dir);
    let scan = scanner(&[]).describe(&bundle);
    assert_eq!(scan.vendor, "BananaV Tests");
    let effects = scan.result.expect("described");
    let names: Vec<&str> = effects.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "BananaV Test Gain",
            "BananaV Test Polarity",
            "BananaV Test Programs"
        ]
    );
    assert!(effects.iter().all(|e| e.works()), "{effects:?}");
    let (gain, polarity, programs) = (&effects[0], &effects[1], &effects[2]);
    assert_eq!(gain.class_id, "42414E414E4156544553544741494E31");
    assert_eq!((gain.inputs, gain.outputs), (2, 2));
    assert_eq!((gain.params, gain.programs), (2, 0));
    assert_eq!(gain.sub_categories, "Fx|Tools");
    assert_eq!(gain.version, "1.0.0");
    assert_eq!(polarity.params, 1);
    assert_eq!((programs.params, programs.programs), (3, 3));
    assert_eq!(programs.tail, Some(plug::TAIL));
    // Crashing as they process: told for each effect (the guard caught
    // it in the child), or the child's crash where there is no guard.
    let crashed = scanner(&[("BV_VST3_TESTPLUG_CRASH", "process")]).describe(&bundle);
    if cfg!(any(all(windows, target_env = "msvc"), target_os = "macos")) {
        let effects = crashed.result.expect("the child lived");
        let error = effects[0].error.as_deref().unwrap_or_default();
        assert!(error.contains("crashed in process"), "{error}");
        assert!(effects.iter().all(|e| !e.works()));
    } else {
        assert!(matches!(crashed.result, Err(Failure::Crashed(_))));
    }
    // Aborting as one starts: the child's crash; where the guard catches
    // an abort too (macOS), the child lives and tells it.
    let ended = scanner(&[("BV_VST3_TESTPLUG_CRASH", "start")]).describe(&bundle);
    let told = match &ended.result {
        Err(Failure::Crashed(_)) => true,
        Err(Failure::Refused(why)) => cfg!(target_os = "macos") && why.contains("crashed"),
        Ok(effects) => {
            cfg!(target_os = "macos")
                && effects
                    .iter()
                    .all(|e| e.error.as_deref().is_some_and(|w| w.contains("crashed")))
        }
        _ => false,
    };
    assert!(told, "{:?}", ended.result);
    // In the cache: what worked is kept.
    let mut cache = bv_vst3::scan::Cache::default();
    let describe = |m: &Path| scanner(&[]).describe(m);
    cache.refresh(std::slice::from_ref(&bundle), 2, describe, |_, _| {});
    assert_eq!(cache.effects().count(), 3);
    assert!(cache.known(&bundle).is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A 32-bit module in BananaV's 32-bit bridge: looked at there (waiting
/// for the bridge without one), run there — its parameters and their
/// texts, ramps, programs, states, the timeline's place and tempo —; a
/// crash there ends the bridge only, the sound going through. Windows x64
/// only.
#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
fn a_32_bit_module_runs_in_the_bridge() {
    use bv_vst3::Bridged;
    use bv_vst3::scan::Failure;
    const X86: &str = "i686-pc-windows-msvc";
    // Rust's 32-bit Windows target, to build them with.
    let sysroot = std::process::Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    if !Path::new(&sysroot).join("lib/rustlib").join(X86).is_dir() {
        eprintln!("left out: no {X86} target (rustup target add {X86})");
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = root.join("target").join("vst3-testplug");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = std::process::Command::new(cargo)
        .current_dir(&root)
        .args([
            "build",
            "-q",
            "-p",
            "bv-vst3-testplug",
            "-p",
            "bananav-bridge32",
        ])
        .args(["--target", X86, "--target-dir"])
        .arg(&target)
        .status()
        .expect("cargo runs");
    assert!(status.success());
    let built = target.join(X86).join("debug");
    let bridge = built.join(bv_bridge::BRIDGE_EXE);
    // A bundle with its 32-bit library alone, as older plug-ins ship.
    let dir = std::env::temp_dir().join(format!("bananav-vst3-bridge-{}", std::process::id()));
    let bundle = dir.join("BananaV Tests.vst3");
    std::fs::create_dir_all(&bundle).unwrap();
    let library = bv_vst3::library_32(&bundle);
    std::fs::create_dir_all(library.parent().unwrap()).unwrap();
    std::fs::copy(built.join("bv_vst3_testplug.dll"), &library).unwrap();
    assert!(bv_vst3::is_32bit(&bundle));
    // Looked at by the bridge; without one, it waits for it.
    let mut s = scanner(&[]);
    assert_eq!(s.describe(&bundle).result, Err(Failure::NeedsBridge));
    s.bridge = Some(bridge.clone());
    let scan = s.describe(&bundle);
    assert!(scan.bits32);
    assert_eq!(scan.vendor, "BananaV Tests");
    let effects = scan.result.expect("described by the bridge");
    let names: Vec<&str> = effects.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "BananaV Test Gain",
            "BananaV Test Polarity",
            "BananaV Test Programs"
        ]
    );
    assert!(effects.iter().all(|e| e.works()), "{effects:?}");
    assert_eq!((effects[2].params, effects[2].programs), (3, 3));
    // Run there: its parameters, their texts, the sound.
    let mut g = Bridged::spawn_with(&bridge, &bundle, "", true, &[]).expect("in the bridge");
    assert_eq!(g.class().name, "BananaV Test Gain");
    assert_eq!(g.class().id(), "42414E414E4156544553544741494E31");
    assert!(g.has_editor());
    let params = g.params();
    assert_eq!(params.len(), 3);
    assert_eq!(
        (params[0].title.as_str(), params[0].units.as_str()),
        ("Gain", "dB")
    );
    assert!(params[1].bypass());
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
    // Ramps reach it at their samples.
    g.queue_ramp(plug::GAIN_ID, &[(0, 0.5), (100, 0.25), (150, 0.0)]);
    let y = tone(200);
    let mut ch = vec![y.clone(), y.clone()];
    g.process(&mut ch);
    for (i, (a, b)) in ch[0].iter().zip(&y).enumerate() {
        let k = match i {
            0..100 => 1.0,
            100..150 => 0.5,
            _ => 0.0,
        };
        assert!((a - b * k).abs() < 1e-6, "sample {i}: {a} for {}", b * k);
    }
    // Its editor inside a window of this process (never shown), as the FX
    // window shows it: made in the bridge, put inside ours; the size it
    // asked for comes back, and it closes.
    bv_bridge::windows_here();
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
    assert!(g.open_editor("Gain", Some(parent)), "inside our window");
    assert!(g.editor_open());
    assert!(g.poll_editor(), "still open");
    assert_eq!(g.editor_size(), Some(plug::EDITOR_ASKED));
    g.close_editor();
    assert!(!g.editor_open());
    // SAFETY: the test's window, its child gone with the editor.
    unsafe { window::DestroyWindow(parent) };
    assert!(!g.crashed());
    drop(g);
    // Its programs, its state, the timeline's place and tempo.
    let programs = effects[2].class_id.clone();
    let mut p = Bridged::spawn_with(&bridge, &bundle, &programs, true, &[]).unwrap();
    assert_eq!(p.programs(), ["Unity", "Half", "Silent"]);
    assert_eq!(p.program(), Some(0));
    assert_eq!(p.tail(), Some(plug::TAIL));
    assert_eq!(p.latency(), 0);
    p.setup(48_000.0, 512, 2).unwrap();
    assert!(p.set_program(1));
    assert_eq!(p.program(), Some(1));
    assert_eq!(p.value(plug::GAIN_ID), Some(0.25));
    p.set_transport(bv_vst3::Transport {
        playing: true,
        tempo: 90.0,
        numerator: 3,
        denominator: 4,
        cycle: None,
    });
    p.locate(48_000);
    let mut ch = vec![tone(256), tone(256)];
    p.process(&mut ch);
    let state = p.state().expect("its state");
    assert_eq!(f64_at(&state, 24), 90.0, "tempo");
    assert!((f64_at(&state, 32) - 1.5).abs() < 1e-9, "quarter notes");
    assert_eq!(i64_at(&state, 40), 48_000, "samples");
    let mut q = Bridged::spawn_with(&bridge, &bundle, &programs, true, &[]).unwrap();
    assert!(q.set_state(&state, &[]));
    assert_eq!(q.program(), Some(1));
    drop((p, q));
    // Ending the bridge as it starts: no plug-in.
    let start = [("BV_VST3_TESTPLUG_CRASH".to_string(), "start".to_string())];
    assert!(matches!(
        Bridged::spawn_with(&bridge, &bundle, "", false, &start),
        Err(bv_vst3::Error::Bridge(_))
    ));
    // A crash ends its bridge only: the sound goes through, it is told.
    let env = [("BV_VST3_TESTPLUG_CRASH".to_string(), "process".to_string())];
    let mut c = Bridged::spawn_with(&bridge, &bundle, "", false, &env).unwrap();
    c.setup(48_000.0, 256, 2).unwrap();
    let mut ch = vec![x.clone(), x.clone()];
    c.process(&mut ch);
    assert_eq!(ch[0], x, "the sound went through");
    assert!(c.crashed());
    let crashes = bv_vst3::take_crashes();
    let crash = crashes
        .iter()
        .find(|k| k.module == bundle)
        .unwrap_or_else(|| panic!("{crashes:?}"));
    assert_eq!((crash.doing.as_str(), crash.code), ("process", 0xC000_0005));
    assert_eq!(crash.effect, "BananaV Test Gain");
    assert!(crash.at.ends_with("(in the 32-bit bridge)"), "{}", crash.at);
    drop(c);
    let _ = std::fs::remove_dir_all(&dir);
}
