//! The VST 2 host against the test plug-in (`bv-vst2-testplug`): linked
//! into the tests; built as a library and looked at in a process of its
//! own; and, on Windows, built for 32-bit x86 and run in BananaV's 32-bit
//! bridge (built here too).

// The editor's test makes a window of its own (Windows' functions).
#![allow(unsafe_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(windows)]
use std::sync::Mutex;

use bv_vst2::{Instance, Module, Transport};
use bv_vst2_testplug as plug;

fn module() -> Arc<Module> {
    bv_vst2::linked("testplug2", plug::VSTPluginMain).expect("the test plug-in")
}

fn instance() -> Instance {
    Instance::of(&module(), 0).expect("it starts")
}

fn tone(n: usize) -> Vec<f32> {
    (0..n).map(|i| (i as f32 * 0.05).sin() * 0.5).collect()
}

fn scaled(a: &[f32], b: &[f32], k: f32) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y * k).abs() < 1e-6)
}

#[test]
fn a_plug_in_describes_itself_and_processes() {
    let mut p = instance();
    let d = p.describe();
    assert_eq!(d.name, "BananaV Test Gain 2");
    assert_eq!(d.vendor, "BananaV Tests");
    assert_eq!((d.unique_id, d.version), (plug::UNIQUE_ID, 1000));
    assert_eq!((d.inputs, d.outputs, d.params, d.programs), (2, 2, 4, 3));
    assert!(d.chunks);
    assert_eq!(bv_vst2::class_id(d.unique_id, 0), "42764732");
    let params = p.params();
    assert_eq!(params[0].name, "Gain");
    assert_eq!(params[0].label, "dB");
    assert_eq!(p.display(plug::GAIN), "0.0");
    p.setup(48_000.0, 64, 2).expect("stereo");
    let x = tone(300);
    let mut ch = vec![x.clone(), x.clone()];
    p.process(&mut ch);
    assert_eq!(ch[0], x, "unity in the middle");
    p.set(plug::GAIN, 0.25);
    assert_eq!(p.value(plug::GAIN), Some(0.25));
    assert_eq!(p.display(plug::GAIN), "-6.0");
    let mut ch = vec![x.clone(), x.clone()];
    p.process(&mut ch);
    assert!(scaled(&ch[1], &x, 0.5));
    // One channel through a stereo plug-in: on both sides, out as their
    // mean.
    let mut mono = vec![x.clone()];
    p.process(&mut mono);
    assert!(scaled(&mono[0], &x, 0.5));
    assert_eq!(p.tail(), Some(plug::TAIL as u32));
    assert_eq!(p.latency(), 0);
}

#[test]
fn programs_states_and_preset_files() {
    let mut p = instance();
    assert_eq!(p.programs(), ["Unity", "Half", "Silent"]);
    assert_eq!(p.program(), Some(0));
    assert!(p.set_program(1));
    assert_eq!(p.program(), Some(1));
    assert_eq!(p.value(plug::GAIN), Some(0.25));
    assert!(!p.set_program(3));
    // Its state (its chunk) comes back in another instance.
    p.set(plug::BYPASS, 1.0);
    let state = p.state().expect("a state");
    let mut q = instance();
    assert!(q.set_state(&state));
    assert_eq!(q.program(), Some(1));
    assert_eq!(q.value(plug::GAIN), Some(0.25));
    assert_eq!(q.value(plug::BYPASS), Some(1.0));
    // The program as a preset file, read back by another.
    let file = p.preset_file().expect("a preset file");
    let preset = bv_vst2::preset::read(&file).expect("it reads");
    assert!(preset.is_for(plug::UNIQUE_ID));
    assert!(!preset.is_bank());
    let mut r = instance();
    assert!(r.set_state(&file));
    assert_eq!(r.value(plug::BYPASS), Some(1.0));
}

/// The timeline's place and tempo reach the plug-in; a ramp's points cut
/// the block where they fall.
#[test]
fn the_timeline_and_ramps_reach_the_plug_in() {
    let mut p = instance();
    p.setup(48_000.0, 512, 2).unwrap();
    p.set_transport(Transport {
        playing: true,
        tempo: 90.0,
        ..Transport::default()
    });
    p.locate(96_000);
    let mut ch = vec![tone(256), tone(256)];
    p.process(&mut ch);
    assert!((p.value(plug::TEMPO_SEEN).unwrap() - 0.09).abs() < 1e-6);
    assert!((p.value(plug::PLACE_SEEN).unwrap() - 0.0096).abs() < 1e-6);
    // Gain: unity, half from sample 100, nothing from 150.
    p.queue_ramp(plug::GAIN, &[(0, 0.5), (100, 0.25), (150, 0.0)]);
    let x = tone(200);
    let mut ch = vec![x.clone(), x.clone()];
    p.process(&mut ch);
    for (i, (a, b)) in ch[0].iter().zip(&x).enumerate() {
        let k = match i {
            0..100 => 1.0,
            100..150 => 0.5,
            _ => 0.0,
        };
        assert!((a - b * k).abs() < 1e-6, "sample {i}");
    }
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

/// The editor in a window of the test's (never shown): it asks for
/// another size once open.
#[cfg(windows)]
#[test]
fn an_editor_lives_in_the_window_it_is_given() {
    let mut p = instance();
    assert!(p.has_editor());
    assert_eq!(
        p.editor_size(),
        Some((
            i32::from(plug::EDITOR_START.0),
            i32::from(plug::EDITOR_START.1)
        ))
    );
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
    // SAFETY: the window lives until after the editor is closed.
    let ok = unsafe {
        p.open_editor(
            parent as *mut std::ffi::c_void,
            Box::new(move |w, h| note.lock().unwrap().push((w, h))),
            "Gain",
        )
    };
    assert!(ok);
    assert_eq!(*asked.lock().unwrap(), [plug::EDITOR_ASKED]);
    assert_eq!(p.editor_size(), Some(plug::EDITOR_ASKED));
    assert!(p.follow_editor());
    p.close_editor();
    assert!(!p.editor_open());
    // SAFETY: the test's window.
    unsafe { window::DestroyWindow(parent) };
}

/// A plug-in that crashes as it processes: switched off, the sound as it
/// came, the crash told. Windows only (structured exception handling).
#[cfg(all(windows, target_env = "msvc"))]
#[test]
fn a_plug_in_that_crashes_is_switched_off_not_the_host() {
    let m = bv_vst2::linked("crashplug2", plug::VSTPluginMainCrash).unwrap();
    let mut c = Instance::of(&m, 0).expect("made");
    c.setup(48_000.0, 128, 2).unwrap();
    let x = tone(300);
    let mut ch = vec![x.clone(), x.clone()];
    c.process(&mut ch);
    assert_eq!(ch[0], x, "the sound went through");
    assert!(c.crashed() && m.crashed());
    let crashes = bv_vst2::take_crashes();
    let crash = crashes
        .iter()
        .find(|k| k.module == Path::new("<linked>/crashplug2"))
        .unwrap_or_else(|| panic!("{crashes:?}"));
    assert_eq!(crash.doing, "process");
    assert_eq!(crash.code, 0xC000_0005);
    assert!(c.params().is_empty());
    drop(c);
    assert!(matches!(Instance::of(&m, 0), Err(bv_vst2::Error::Crashed)));
    // Its neighbours go on.
    let mut g = instance();
    g.setup(48_000.0, 128, 2).unwrap();
    assert!(!g.crashed());
}

/// Builds `package` (for `target`, or this computer) into a folder of its
/// own; the folder of its build's files.
fn build(package: &str, target: Option<&str>) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join("target").join("vst2-tests");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut cmd = std::process::Command::new(cargo);
    cmd.current_dir(&root)
        .args(["build", "-q", "-p", package, "--target-dir"])
        .arg(&dir);
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    assert!(cmd.status().expect("cargo runs").success(), "{package}");
    match target {
        Some(t) => dir.join(t).join("debug"),
        None => dir.join("debug"),
    }
}

/// The test plug-in's library copied into a folder of its own.
fn plug_in_file(built: &Path, tag: &str) -> PathBuf {
    let name = if cfg!(windows) {
        "bv_vst2_testplug.dll"
    } else {
        "libbv_vst2_testplug.so"
    };
    let dir = std::env::temp_dir().join(format!("bananav-vst2-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(name);
    std::fs::copy(built.join(name), &file).unwrap();
    file
}

fn scanner(env: &[(&str, &str)], bridge: Option<PathBuf>) -> bv_vst2::scan::Scanner {
    bv_vst2::scan::Scanner {
        exe: PathBuf::from(env!("CARGO_BIN_EXE_bv-vst2-scan")),
        bridge,
        env: env
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        timeout: std::time::Duration::from_secs(120),
    }
}

/// Libraries looked at in a process of their own: the plug-in tried and
/// described; one that crashes there costs that process only.
#[test]
fn libraries_are_looked_at_in_a_process_of_their_own() {
    use bv_vst2::scan::Failure;
    let file = plug_in_file(&build("bv-vst2-testplug", None), "scan");
    let scan = scanner(&[], None).describe(&file);
    assert_eq!(scan.machine, Some(bv_vst2::this_machine()));
    let effects = scan.result.expect("described");
    assert_eq!(effects.len(), 1);
    let e = &effects[0];
    assert!(e.works(), "{e:?}");
    assert!(e.is_effect());
    assert_eq!(e.name, "BananaV Test Gain 2");
    assert_eq!((e.params, e.programs, e.inputs, e.outputs), (4, 3, 2, 2));
    assert_eq!(e.tail, Some(plug::TAIL as u32));
    let crashed = scanner(&[("BV_VST2_TESTPLUG_CRASH", "process")], None).describe(&file);
    if cfg!(all(windows, target_env = "msvc")) {
        let effects = crashed.result.expect("the child lived");
        let error = effects[0].error.as_deref().unwrap_or_default();
        assert!(error.contains("crashed in process"), "{error}");
    } else {
        assert!(matches!(crashed.result, Err(Failure::Crashed(_))));
    }
    let ended = scanner(&[("BV_VST2_TESTPLUG_CRASH", "start")], None).describe(&file);
    assert!(
        matches!(ended.result, Err(Failure::Crashed(_))),
        "{:?}",
        ended.result
    );
    let _ = std::fs::remove_dir_all(file.parent().unwrap());
}

/// A 32-bit plug-in in BananaV's 32-bit bridge: looked at, run, its
/// programs and states; a crash there ends the bridge only, the sound
/// going through. Windows x64 only.
#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
fn a_32_bit_plug_in_runs_in_the_bridge() {
    // Rust's 32-bit Windows target, to build them with.
    let sysroot = std::process::Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    if !Path::new(&sysroot)
        .join("lib/rustlib/i686-pc-windows-msvc")
        .is_dir()
    {
        eprintln!(
            "left out: no i686-pc-windows-msvc target (rustup target add i686-pc-windows-msvc)"
        );
        return;
    }
    let built = build("bv-vst2-testplug", Some("i686-pc-windows-msvc"));
    let bridge =
        build("bananav-bridge32", Some("i686-pc-windows-msvc")).join(bv_vst2::bridge::BRIDGE_EXE);
    let file = plug_in_file(&built, "bridge");
    assert_eq!(bv_vst2::machine(&file), Some(bv_vst2::Machine::X86));
    // Looked at by the bridge.
    let scan = scanner(&[], Some(bridge.clone())).describe(&file);
    let effects = scan.result.expect("described by the bridge");
    assert_eq!(effects[0].name, "BananaV Test Gain 2");
    assert!(effects[0].works(), "{:?}", effects[0]);
    // Run there.
    let mut p = Instance::bridged_with(&bridge, &file, 0, &[]).expect("in the bridge");
    assert!(p.bridged());
    assert_eq!(p.describe().params, 4);
    assert_eq!(p.params()[1].name, "Bypass");
    p.setup(48_000.0, 256, 2).expect("set up");
    p.set_transport(Transport {
        tempo: 140.0,
        ..Transport::default()
    });
    let x = tone(600);
    p.queue(plug::GAIN, 0.25);
    let mut ch = vec![x.clone(), x.clone()];
    p.process(&mut ch);
    assert!(scaled(&ch[0], &x, 0.5), "half through the bridge");
    assert!((p.value(plug::TEMPO_SEEN).unwrap() - 0.14).abs() < 1e-6);
    assert!(p.set_program(2));
    assert_eq!(p.programs()[2], "Silent");
    let state = p.state().expect("its state");
    let mut q = Instance::bridged_with(&bridge, &file, 0, &[]).unwrap();
    assert!(q.set_state(&state));
    assert_eq!(q.program(), Some(2));
    assert_eq!(q.display(plug::GAIN), "-oo");
    drop((p, q));
    // A crash ends its bridge only: the sound goes through, it is told.
    let env = [("BV_VST2_TESTPLUG_CRASH".to_string(), "process".to_string())];
    let mut c = Instance::bridged_with(&bridge, &file, 0, &env).unwrap();
    c.setup(48_000.0, 256, 2).unwrap();
    let mut ch = vec![x.clone(), x.clone()];
    c.process(&mut ch);
    assert_eq!(ch[0], x);
    assert!(c.crashed());
    let crashes = bv_vst2::take_crashes();
    let crash = crashes
        .iter()
        .find(|k| k.module == file)
        .unwrap_or_else(|| panic!("{crashes:?}"));
    assert_eq!((crash.doing.as_str(), crash.code), ("process", 0xC000_0005));
    assert!(crash.at.ends_with("(in the 32-bit bridge)"), "{}", crash.at);
    drop(c);
    let _ = std::fs::remove_dir_all(file.parent().unwrap());
}
