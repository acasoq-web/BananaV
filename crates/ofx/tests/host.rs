//! The host with the test plug-ins (`bv-ofx-testplug`): built into this
//! test, then loaded as a real binary from a bundle.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use bv_core::ImageRgbaF32;
use bv_ofx::{
    Animation, Binary, ChangeReason, ClipSource, Components, Context, Depth, DevicePicture, Format,
    Instance, Interp, Key, Mark, ParamType, PenAction, Plugin, Premult, RenderRequest, Setup,
    Severity, Value, View,
};

fn plugins() -> &'static [Arc<Plugin>] {
    static PLUGINS: OnceLock<Vec<Arc<Plugin>>> = OnceLock::new();
    PLUGINS.get_or_init(|| {
        let bin = Binary::built_in(
            "testplug",
            bv_ofx_testplug::OfxGetNumberOfPlugins,
            bv_ofx_testplug::OfxGetPlugin,
            None,
        );
        let (plugins, failures) = bin.plugins().expect("the test plug-ins load");
        assert!(failures.is_empty(), "{failures:?}");
        plugins
    })
}

fn plugin(id: &str) -> &'static Arc<Plugin> {
    plugins()
        .iter()
        .find(|p| p.info.identifier == id)
        .unwrap_or_else(|| panic!("{id}"))
}

/// A still picture as a clip.
struct Still(Arc<ImageRgbaF32>);

impl ClipSource for Still {
    fn picture(&self, _time: f64) -> Option<Arc<ImageRgbaF32>> {
        Some(Arc::clone(&self.0))
    }

    fn full_size(&self) -> (f64, f64) {
        (self.0.width as f64, self.0.height as f64)
    }
}

fn filled(w: u32, h: u32, px: [f32; 4]) -> Arc<dyn ClipSource> {
    Arc::new(Still(Arc::new(ImageRgbaF32::filled(w, h, px))))
}

fn setup(w: f64, h: f64) -> Setup {
    Setup {
        frame_rate: 30.0,
        duration: 100.0,
        size: (w, h),
        pixel_aspect: 1.0,
        interactive: true,
    }
}

fn request(time: f64, w: u32, h: u32) -> RenderRequest {
    RenderRequest {
        time,
        width: w,
        height: h,
        scale: 1.0,
        draft: false,
        interactive: true,
        sequential: false,
        timeline: None,
    }
}

fn close(a: [f32; 4], b: [f32; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5)
}

#[test]
fn the_test_plugins_describe_themselves() {
    let ids: Vec<_> = plugins()
        .iter()
        .map(|p| p.info.identifier.as_str())
        .collect();
    assert_eq!(
        ids,
        [
            "org.bananav.test.Invert",
            "org.bananav.test.Gradient",
            "org.bananav.test.Wipe",
            "org.bananav.test.Echo"
        ]
    );
    let invert = &plugin("org.bananav.test.Invert").info;
    assert_eq!(invert.label, "Invert (test)");
    assert_eq!(invert.grouping, "BananaV Tests");
    assert_eq!(invert.version, [1, 2]);
    assert_eq!(invert.contexts, [Context::Filter, Context::General]);
    let filter = invert.context(Context::Filter).unwrap();
    let clips: Vec<_> = filter.clips.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(clips, ["Source", "Output"]);
    let names: Vec<_> = filter.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "amount",
            "channels",
            "tint",
            "center",
            "steps",
            "note",
            "advanced",
            "invertAlpha",
            "reset",
            "curve"
        ]
    );
    let p = |n: &str| filter.params.iter().find(|p| p.name == n).unwrap();
    assert_eq!(p("amount").default, Value::Num(vec![1.0]));
    assert_eq!(p("amount").label, "Amount");
    assert_eq!(p("amount").display_max, vec![1.0]);
    assert_eq!(p("channels").options, ["All", "Red", "Green", "Blue"]);
    assert_eq!(p("tint").ty, ParamType::Rgba);
    assert_eq!(p("steps").default, Value::Num(vec![3.0]));
    assert_eq!(p("note").default, Value::Text("hello".into()));
    assert_eq!(p("invertAlpha").parent, "advanced");
    assert!(p("amount").animates && !p("channels").animates);
    assert_eq!(
        p("curve").default,
        Value::Curves(vec![bv_core::curve::IDENTITY.to_vec()])
    );
    let wipe = &plugin("org.bananav.test.Wipe").info;
    assert_eq!(wipe.contexts, [Context::Transition]);
}

#[test]
fn an_instance_inverts_its_source_over_time() {
    let inst = Instance::new(
        plugin("org.bananav.test.Invert"),
        Context::Filter,
        &setup(8.0, 4.0),
    )
    .expect("an instance");
    inst.connect("Source", filled(8, 4, [0.2, 0.4, 0.6, 1.0]));
    let out = inst.render(&request(0.0, 8, 4)).expect("a frame");
    assert_eq!((out.width, out.height), (8, 4));
    assert!(
        close(out.pixel(3, 2), [0.8, 0.6, 0.4, 1.0]),
        "{:?}",
        out.pixel(3, 2)
    );
    // The amount from half to whole over ten frames.
    let anim = Animation {
        value: Value::Num(vec![1.0]),
        keys: vec![
            Key {
                time: 0.0,
                value: Value::Num(vec![0.5]),
                interp: Interp::Linear,
                handles: Vec::new(),
            },
            Key {
                time: 10.0,
                value: Value::Num(vec![1.0]),
                interp: Interp::Linear,
                handles: Vec::new(),
            },
        ],
    };
    inst.set_values(&[("amount".into(), anim)]);
    let half = inst.render(&request(0.0, 8, 4)).unwrap();
    assert!(
        close(half.pixel(0, 0), [0.5, 0.5, 0.5, 1.0]),
        "{:?}",
        half.pixel(0, 0)
    );
    let whole = inst.render(&request(10.0, 8, 4)).unwrap();
    assert!(close(whole.pixel(7, 3), [0.8, 0.6, 0.4, 1.0]));
}

#[test]
fn changes_go_both_ways() {
    let inst = Instance::new(
        plugin("org.bananav.test.Invert"),
        Context::Filter,
        &setup(4.0, 4.0),
    )
    .unwrap();
    // The user sets the amount to 0: the plug-in turns the tint off.
    inst.set_values(&[("amount".into(), Animation::constant(Value::Num(vec![0.0])))]);
    let changed = inst.changed(&["amount".into()], ChangeReason::User, 0.0);
    assert!(changed.is_empty(), "{changed:?}");
    let tint = |i: &Instance| {
        i.param_infos()
            .into_iter()
            .find(|p| p.name == "tint")
            .unwrap()
            .enabled
    };
    assert!(!tint(&inst));
    // Reset: the plug-in sets the amount and the note itself.
    let mut changed = inst.changed(&["reset".into()], ChangeReason::User, 0.0);
    changed.sort();
    assert_eq!(changed, ["amount", "note"]);
    let values = inst.values();
    let value = |n: &str| values.iter().find(|(k, _)| k == n).unwrap().1.value.clone();
    assert_eq!(value("amount"), Value::Num(vec![1.0]));
    assert_eq!(value("note"), Value::Text("reset".into()));
    // Its own change came back to it: the tint is on again.
    assert!(tint(&inst));
    // Buttons and groups keep nothing.
    assert!(values.iter().all(|(n, _)| n != "reset" && n != "advanced"));
}

#[test]
fn nothing_to_do_is_an_identity_and_almost_nothing_a_message() {
    let inst = Instance::new(
        plugin("org.bananav.test.Invert"),
        Context::Filter,
        &setup(2.0, 2.0),
    )
    .unwrap();
    let src = Arc::new(ImageRgbaF32::filled(2, 2, [0.1, 0.2, 0.3, 0.5]));
    inst.connect("Source", Arc::new(Still(Arc::clone(&src))));
    inst.set_values(&[("amount".into(), Animation::constant(Value::Num(vec![0.0])))]);
    let same = inst.render(&request(0.0, 2, 2)).unwrap();
    assert_eq!(same, *src);
    inst.set_values(&[(
        "amount".into(),
        Animation::constant(Value::Num(vec![0.005])),
    )]);
    inst.render(&request(0.0, 2, 2)).unwrap();
    let m = inst.persistent_message().expect("a warning");
    assert_eq!(m.severity, Severity::Warning);
    assert_eq!(m.text, "Amount 0.005 inverts almost nothing");
    inst.set_values(&[("amount".into(), Animation::constant(Value::Num(vec![1.0])))]);
    inst.render(&request(0.0, 2, 2)).unwrap();
    assert_eq!(inst.persistent_message(), None);
}

#[test]
fn channels_tint_and_alpha() {
    let inst = Instance::new(
        plugin("org.bananav.test.Invert"),
        Context::General,
        &setup(1.0, 1.0),
    )
    .unwrap();
    inst.connect("Source", filled(1, 1, [0.25, 0.25, 0.25, 0.5]));
    inst.set_values(&[
        (
            "channels".into(),
            Animation::constant(Value::Num(vec![1.0])),
        ),
        (
            "tint".into(),
            Animation::constant(Value::Num(vec![0.5, 1.0, 1.0, 1.0])),
        ),
        (
            "invertAlpha".into(),
            Animation::constant(Value::Num(vec![1.0])),
        ),
    ]);
    let px = inst.render(&request(0.0, 1, 1)).unwrap().pixel(0, 0);
    // Red only (premultiplied: 0.5 − 0.25), halved; alpha inverted.
    assert!(close(px, [0.125, 0.25, 0.25, 0.5]), "{px:?}");
}

#[test]
fn a_generator_in_bytes() {
    let inst = Instance::new(
        plugin("org.bananav.test.Gradient"),
        Context::Generator,
        &setup(4.0, 8.0),
    )
    .unwrap();
    let out = inst.render(&request(0.0, 4, 8)).unwrap();
    // BananaV's top row is the plug-in's last: its top color.
    assert_eq!(out.pixel(0, 0), [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(out.pixel(3, 7), [0.0, 0.0, 1.0, 1.0]);
    let mid = out.pixel(1, 4)[0];
    assert!(mid > 0.3 && mid < 0.6, "{mid}");
}

#[test]
fn a_transition_wipes() {
    let inst = Instance::new(
        plugin("org.bananav.test.Wipe"),
        Context::Transition,
        &setup(8.0, 2.0),
    )
    .unwrap();
    inst.connect("SourceFrom", filled(8, 2, [0.0, 0.0, 0.0, 1.0]));
    inst.connect("SourceTo", filled(8, 2, [1.0; 4]));
    inst.set_values(&[(
        "Transition".into(),
        Animation::constant(Value::Num(vec![0.25])),
    )]);
    let out = inst.render(&request(0.0, 8, 2)).unwrap();
    assert_eq!(out.pixel(1, 0), [1.0; 4]);
    assert_eq!(out.pixel(2, 1), [0.0, 0.0, 0.0, 1.0]);
}

#[test]
fn an_overlay_draws_its_cross_and_the_pen_moves_the_center() {
    let inst = Instance::new(
        plugin("org.bananav.test.Invert"),
        Context::Filter,
        &setup(200.0, 100.0),
    )
    .unwrap();
    assert!(inst.has_overlay());
    // The center's default was normalised (the middle): now canonical.
    let center = |inst: &Instance| {
        inst.values()
            .into_iter()
            .find(|(n, _)| n == "center")
            .map(|(_, a)| a.value)
    };
    assert_eq!(center(&inst), Some(Value::Num(vec![100.0, 50.0])));
    let view = View {
        time: 0.0,
        render_scale: 1.0,
        pixel_scale: [1.0, 1.0],
        background: [0.0; 3],
        viewport: [200, 100],
        to_frame: None,
    };
    let marks = inst.overlay_draw(&view);
    let Some(Mark::Lines { points, .. }) = marks.first() else {
        panic!("{marks:?}");
    };
    assert_eq!(points[0], [88.0, 50.0]);
    assert!(
        marks
            .iter()
            .any(|m| matches!(m, Mark::Text { text, .. } if text == "Center"))
    );
    // Far from the cross, the pen is not the overlay's.
    let far = inst.overlay_pen(PenAction::Down, &view, [10.0, 10.0], [10, 90], 1.0);
    assert!(!far.taken);
    // On it, the overlay takes the pen; moving it moves the center.
    assert!(
        inst.overlay_pen(PenAction::Down, &view, [101.0, 49.0], [101, 51], 1.0)
            .taken
    );
    let moved = inst.overlay_pen(PenAction::Motion, &view, [150.0, 75.0], [150, 25], 1.0);
    assert!(moved.taken);
    assert_eq!(moved.changed, ["center"]);
    assert!(
        inst.overlay_pen(PenAction::Up, &view, [150.0, 75.0], [150, 25], 1.0)
            .taken
    );
    assert_eq!(center(&inst), Some(Value::Num(vec![150.0, 75.0])));
    // Drawn again: the cross where the center went, in the active color.
    let marks = inst.overlay_draw(&view);
    let Some(Mark::Lines { points, .. }) = marks.first() else {
        panic!("{marks:?}");
    };
    assert_eq!(points[0], [138.0, 75.0]);
}

#[test]
fn a_plugin_made_in_the_next_context_when_one_fails() {
    let echo = plugin("org.bananav.test.Echo");
    assert_eq!(echo.info.contexts, [Context::Filter, Context::General]);
    assert!(Instance::new(echo, Context::Filter, &setup(4.0, 4.0)).is_err());
    let inst =
        Instance::new_in(echo, &[Context::Filter, Context::General], &setup(4.0, 4.0)).unwrap();
    assert_eq!(inst.context(), Context::General);
    // Only its own contexts are tried.
    assert!(Instance::new_in(echo, &[Context::Transition], &setup(4.0, 4.0)).is_err());
}

#[test]
fn a_plugin_without_its_source_fails_without_crashing() {
    let inst = Instance::new(
        plugin("org.bananav.test.Invert"),
        Context::Filter,
        &setup(2.0, 2.0),
    )
    .unwrap();
    let e = inst.render(&request(0.0, 2, 2)).unwrap_err();
    assert_eq!(e.to_string(), "Render answered kOfxStatFailed");
}

/// The test plug-ins built as a library, as a vendor ships them.
fn built_library() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = root.join("target").join("ofx-testplug");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = std::process::Command::new(cargo)
        .current_dir(&root)
        .args(["build", "-q", "-p", "bv-ofx-testplug", "--target-dir"])
        .arg(&target)
        .status()
        .expect("cargo runs");
    assert!(status.success());
    let name = if cfg!(windows) {
        "bv_ofx_testplug.dll"
    } else {
        "libbv_ofx_testplug.so"
    };
    target.join("debug").join(name)
}

#[test]
fn the_library_loads_from_a_bundle() {
    let lib = built_library();
    let dir = std::env::temp_dir().join(format!("bananav-ofx-{}", std::process::id()));
    let arch = bv_ofx::plugin::architectures()[0];
    let contents = dir
        .join("Tests")
        .join("BananaV Test.ofx.bundle")
        .join("Contents")
        .join(arch);
    std::fs::create_dir_all(&contents).unwrap();
    std::fs::copy(&lib, contents.join("BananaV Test.ofx")).unwrap();
    let bundles = bv_ofx::plugin::find_bundles(&dir);
    assert_eq!(bundles.len(), 1);
    let binaries = bv_ofx::plugin::bundle_binaries(&bundles[0]);
    assert_eq!(binaries.len(), 1);
    let bin = Binary::load(&binaries[0]).expect("it loads");
    let (plugins, failures) = bin.plugins().unwrap();
    assert!(failures.is_empty());
    assert_eq!(plugins.len(), 4);
    let inst = Instance::new(&plugins[0], Context::Filter, &setup(2.0, 2.0)).unwrap();
    inst.connect("Source", filled(2, 2, [0.0, 0.0, 0.0, 1.0]));
    let out = inst.render(&request(0.0, 2, 2)).unwrap();
    assert_eq!(out.pixel(1, 1), [1.0, 1.0, 1.0, 1.0]);
    drop(inst);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The test library in a bundle of its own folder; the folder and the
/// binary.
fn bundle(tag: &str) -> (PathBuf, PathBuf) {
    let lib = built_library();
    let dir = std::env::temp_dir().join(format!("bananav-ofx-{tag}-{}", std::process::id()));
    let arch = bv_ofx::plugin::architectures()[0];
    let contents = dir.join("Test.ofx.bundle").join("Contents").join(arch);
    std::fs::create_dir_all(&contents).unwrap();
    let binary = contents.join("Test.ofx");
    std::fs::copy(&lib, &binary).unwrap();
    (dir, binary)
}

fn scanner(env: &[(&str, &str)]) -> bv_ofx::scan::Scanner {
    bv_ofx::scan::Scanner {
        exe: PathBuf::from(env!("CARGO_BIN_EXE_bv-ofx-scan")),
        args: Vec::new(),
        library_folders: Vec::new(),
        env: env
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        timeout: std::time::Duration::from_secs(60),
    }
}

#[test]
fn plug_ins_are_described_in_a_process_of_their_own() {
    let (dir, binary) = bundle("scan");
    let scan = scanner(&[]).describe(&binary);
    let plugins = scan.result.expect("described");
    assert_eq!(plugins.len(), 4);
    assert_eq!(plugins[0].label, "Invert (test)");
    // One that crashes while describing itself only costs its process.
    let crashed = scanner(&[("BV_OFX_TESTPLUG_CRASH", "describe")]).describe(&binary);
    assert!(
        matches!(crashed.result, Err(bv_ofx::Error::Crashed(_))),
        "{:?}",
        crashed.result
    );
    // The cache describes a binary once, until it changes.
    let mut cache = bv_ofx::scan::Cache::default();
    let mut described = 0;
    let changed = cache.refresh(std::slice::from_ref(&dir), |f| {
        described += 1;
        scanner(&[]).describe(f)
    });
    assert_eq!((changed, described, cache.plugins().len()), (true, 1, 4));
    let changed = cache.refresh(std::slice::from_ref(&dir), |_| unreachable!("cached"));
    assert_eq!((changed, cache.plugins().len()), (false, 4));
    // A crash is no answer to keep: the next refresh describes it again.
    let mut cache = bv_ofx::scan::Cache::default();
    cache.refresh(std::slice::from_ref(&dir), |f| {
        scanner(&[("BV_OFX_TESTPLUG_CRASH", "describe")]).describe(f)
    });
    assert!(cache.plugins().is_empty());
    let changed = cache.refresh(std::slice::from_ref(&dir), |f| scanner(&[]).describe(f));
    assert_eq!((changed, cache.plugins().len()), (true, 4));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_plug_in_is_tried_in_a_process_of_its_own() {
    let (dir, binary) = bundle("try");
    let out = dir.join("pictures");
    std::fs::create_dir_all(&out).unwrap();
    let trial = scanner(&[]).try_plugin(&binary, 0, &out);
    assert_eq!(trial.identifier, "org.bananav.test.Invert");
    assert!(trial.passed(), "{:?}", trial.steps);
    let steps: Vec<_> = trial.steps.iter().map(|s| s.what.as_str()).collect();
    assert_eq!(
        steps,
        [
            "create (Filter)",
            "render defaults",
            "draw its overlay (2 marks)",
            "change 2 parameters",
            "render changed"
        ]
    );
    let first = bv_ofx::scan::read_raw(&trial.pictures[0]).expect("a picture");
    // The card's first bar (75 % gray) inverted.
    let px = first.pixel(10, 10);
    assert!((px[0] - 0.25).abs() < 1e-5, "{px:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// An overlay of OpenFX's first kind draws with OpenGL itself: in the
/// host's offscreen context, set to canonical coordinates, its drawing
/// comes back as a picture of the view (where the graphics card has the
/// OpenGL for it: elsewhere it draws nothing).
#[cfg(windows)]
#[test]
fn an_overlay_drawing_with_opengl_comes_back_as_a_picture() {
    let bin = Binary::built_in(
        "glplug",
        bv_ofx_testplug::OfxGetNumberOfPluginsGl,
        bv_ofx_testplug::OfxGetPluginGl,
        None,
    );
    let (plugins, _) = bin.plugins().expect("the OpenGL plug-in loads");
    let inst = Instance::new(&plugins[0], Context::Filter, &setup(200.0, 100.0)).expect("made");
    assert!(inst.has_overlay());
    let view = View {
        time: 0.0,
        render_scale: 1.0,
        pixel_scale: [1.0, 1.0],
        background: [0.0; 3],
        viewport: [200, 100],
        // Canonical pixels, y up, to the view's 0–1, y down.
        to_frame: Some([[1.0 / 200.0, 0.0, 0.0], [0.0, -1.0 / 100.0, 1.0]]),
    };
    let marks = inst.overlay_draw(&view);
    if !bv_ofx::opengl_overlays() {
        eprintln!("no OpenGL for overlays here: it drew nothing");
        assert!(marks.is_empty(), "{marks:?}");
        return;
    }
    eprintln!("drawn with the graphics card's OpenGL");
    let [
        Mark::Picture {
            width,
            height,
            rgba,
        },
    ] = marks.as_slice()
    else {
        panic!("{marks:?}");
    };
    assert_eq!((*width, *height), (200, 100));
    let at = |x: usize, y: usize| &rgba[(y * 200 + x) * 4..][..4];
    // The cross's middle, canonical (100, 50): the view's (100, 50) from
    // the top. Its arms reach 10 pixels out, nothing beyond.
    assert_eq!(at(100, 50), [255, 255, 255, 255]);
    assert_eq!(at(92, 50)[3], 255);
    assert_eq!(at(100, 42)[3], 255);
    assert_eq!(at(100, 70)[3], 0);
    assert_eq!(at(10, 10)[3], 0);
    // Drawn again, the same (the plug-in's state does not pile up).
    assert_eq!(inst.overlay_draw(&view), marks);
}

/// A clip whose picture is in the graphics card's memory already (a render
/// on the card).
struct OnCard {
    memory: bv_cuda::Memory,
    w: usize,
    h: usize,
}

impl ClipSource for OnCard {
    fn picture(&self, _time: f64) -> Option<Arc<ImageRgbaF32>> {
        None
    }

    fn full_size(&self) -> (f64, f64) {
        (self.w as f64, self.h as f64)
    }

    fn on_card(&self, _time: f64) -> Option<DevicePicture> {
        Some(DevicePicture::borrowed(
            self.memory.address(),
            self.w,
            self.h,
            self.w * 16,
            FLOATS,
        ))
    }
}

const FLOATS: Format = Format {
    depth: Depth::Float,
    components: Components::Rgba,
};

/// A plug-in that renders with CUDA (OpenFX 1.5's CUDA render) gets its
/// pictures in the graphics card's memory where the host has them there,
/// and draws into the card's output; one that only renders so gets the
/// processor's pictures sent there and its own brought back; asked to
/// render on the processor, one that also can does. Where the computer has
/// CUDA (nothing is checked elsewhere).
#[test]
#[allow(unsafe_code)]
fn plug_ins_rendering_with_cuda_get_their_pictures_on_the_card() {
    use std::sync::atomic::Ordering;
    let Some(device) = bv_cuda::devices().first() else {
        eprintln!("no CUDA here");
        return;
    };
    let bin = Binary::built_in(
        "cudaplug",
        bv_ofx_testplug::OfxGetNumberOfPluginsCuda,
        bv_ofx_testplug::OfxGetPluginCuda,
        None,
    );
    let (plugins, failures) = bin.plugins().expect("the CUDA plug-ins load");
    assert!(failures.is_empty(), "{failures:?}");
    let (w, h) = (24usize, 10usize);
    let mut picture = ImageRgbaF32::transparent(w as u32, h as u32);
    for y in 0..h {
        for x in 0..w {
            let a = 0.5 + 0.5 * (y as f32 / h as f32);
            *picture.pixel_mut(x as u32, y as u32) = [0.25 * a, (x as f32 / w as f32) * a, 0.0, a];
        }
    }
    let inverted = |p: [f32; 4]| [p[3] - p[0], p[3] - p[1], p[3] - p[2], p[3]];
    let picture = Arc::new(picture);
    let check = |out: &ImageRgbaF32| {
        for y in 0..h as u32 {
            for x in 0..w as u32 {
                let want = inverted(picture.pixel(x, y));
                assert!(
                    close(out.pixel(x, y), want),
                    "({x}, {y}): {:?}",
                    out.pixel(x, y)
                );
            }
        }
    };
    let invert = plugins
        .iter()
        .find(|p| p.info.identifier == "org.bananav.test.CudaInvert")
        .expect("CudaInvert");
    let inst = Instance::new(invert, Context::Filter, &setup(w as f64, h as f64)).expect("made");
    assert!(inst.renders_with_cuda());
    // On the card: the source's picture there already, the output there.
    let up = DevicePicture::upload(device, &picture, FLOATS).expect("sent");
    let source = device.alloc(up.len()).expect("memory");
    // SAFETY: both the device's memory, as big as the picture.
    unsafe { device.copy(source.address(), up.address, up.len()) }.expect("copied");
    let output = device.alloc(w * h * 16).expect("memory");
    let before = bv_ofx_testplug::CUDA_RENDERS.load(Ordering::Relaxed);
    let on_card: Arc<dyn ClipSource> = Arc::new(OnCard {
        memory: source,
        w,
        h,
    });
    let target = DevicePicture::borrowed(output.address(), w, h, w * 16, FLOATS);
    inst.render_on_card(
        &[("Source", on_card)],
        &[],
        &request(0.0, w as u32, h as u32),
        device,
        target,
    )
    .expect("rendered on the card");
    assert_eq!(
        bv_ofx_testplug::CUDA_RENDERS.load(Ordering::Relaxed),
        before + 1
    );
    let mut bytes = vec![0u8; w * h * 16];
    // SAFETY: the output's memory, of that size.
    unsafe { device.download(&mut bytes, output.address()) }.expect("back");
    // SAFETY: OpenFX's layout of the picture just drawn.
    let back = unsafe {
        bv_ofx::pixels::from_ofx(
            bytes.as_ptr(),
            w,
            h,
            (w * 16) as isize,
            FLOATS,
            Premult::PreMultiplied,
        )
    };
    check(&back);
    // On the processor, a plug-in that also can renders there.
    let source: Arc<dyn ClipSource> = Arc::new(Still(Arc::clone(&picture)));
    let drawn = inst
        .render_with(
            &[("Source", Arc::clone(&source))],
            &[],
            &request(0.0, w as u32, h as u32),
        )
        .expect("rendered on the processor");
    check(&drawn);
    assert_eq!(
        bv_ofx_testplug::CUDA_RENDERS.load(Ordering::Relaxed),
        before + 1
    );
    // One that only renders with CUDA goes through the card.
    let only = plugins
        .iter()
        .find(|p| p.info.identifier == "org.bananav.test.CudaOnlyInvert")
        .expect("CudaOnlyInvert");
    let inst = Instance::new(only, Context::Filter, &setup(w as f64, h as f64)).expect("made");
    let drawn = inst
        .render_with(
            &[("Source", source)],
            &[],
            &request(0.0, w as u32, h as u32),
        )
        .expect("rendered through the card");
    check(&drawn);
    assert_eq!(
        bv_ofx_testplug::CUDA_RENDERS.load(Ordering::Relaxed),
        before + 2
    );
}

/// A plug-in loads alone, by its number (a binary of hundreds takes
/// seconds to describe them all), and hears the unload action once.
#[test]
fn a_plug_in_loads_alone_and_hears_it_is_done_with_once() {
    use std::sync::atomic::Ordering;
    let bin = Binary::built_in(
        "testplug-alone",
        bv_ofx_testplug::OfxGetNumberOfPlugins,
        bv_ofx_testplug::OfxGetPlugin,
        None,
    );
    let echo = bin.plugin(3).expect("Echo loads alone");
    assert_eq!(echo.info.identifier, "org.bananav.test.Echo");
    assert!(bin.plugin(4).is_err(), "there are four");
    let before = bv_ofx_testplug::ECHO_UNLOADS.load(Ordering::Relaxed);
    echo.unload();
    echo.unload();
    assert_eq!(
        bv_ofx_testplug::ECHO_UNLOADS.load(Ordering::Relaxed),
        before + 1
    );
}

/// A plug-in that crashes as it renders — on the render's thread, or on a
/// thread of the multi-thread suite — does not take the host with it: the
/// render fails, its binary is switched off (its next calls fail at once),
/// and the crash is told. Windows only (the guard is structured exception
/// handling).
#[cfg(windows)]
#[test]
fn a_plug_in_that_crashes_is_switched_off_not_the_host() {
    for on_thread in [false, true] {
        let bin = Binary::built_in(
            "crashplug",
            bv_ofx_testplug::OfxGetNumberOfPluginsCrash,
            bv_ofx_testplug::OfxGetPluginCrash,
            None,
        );
        let (plugins, _) = bin.plugins().expect("the crash plug-in loads");
        let crash = &plugins[0];
        let inst = Instance::new(crash, Context::Filter, &setup(16.0, 16.0)).expect("made");
        if on_thread {
            inst.set_values(&[("where".into(), Animation::constant(Value::Num(vec![1.0])))]);
        }
        inst.connect("Source", filled(16, 16, [0.5, 0.5, 0.5, 1.0]));
        let rendered = inst.render(&request(0.0, 16, 16));
        assert!(rendered.is_err(), "the crash came back as a failure");
        assert!(bin.crashed(), "switched off");
        let crashes = bv_ofx::take_crashes();
        assert!(
            crashes.iter().any(|c| c.binary == Path::new("crashplug")),
            "{crashes:?}"
        );
        // Its next render fails at once, without calling it.
        assert!(inst.render(&request(0.0, 16, 16)).is_err());
    }
}
