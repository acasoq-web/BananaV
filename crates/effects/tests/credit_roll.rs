//! Credit Roll through BananaV's OpenFX host, as any host runs it: the
//! Position box and the settings each effect uses, and the credits and
//! properties read from and written to files as soon as one is picked.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use bv_ofx::{Animation, Binary, ChangeReason, Context, Instance, Plugin, Setup, Value};

const CREDIT_ROLL: &str = "org.bananav.CreditRoll";

fn plugin() -> &'static Arc<Plugin> {
    static PLUGINS: OnceLock<Vec<Arc<Plugin>>> = OnceLock::new();
    PLUGINS
        .get_or_init(|| {
            let bin = Binary::built_in(
                bv_effects::BINARY,
                bv_effects::number_of_plugins,
                bv_effects::get_plugin,
                None,
            );
            let (plugins, failures) = bin.plugins().expect("BananaV's effects load");
            assert!(failures.is_empty(), "{failures:?}");
            plugins
        })
        .iter()
        .find(|p| p.info.identifier == CREDIT_ROLL)
        .expect(CREDIT_ROLL)
}

fn instance() -> Instance {
    let setup = Setup {
        frame_rate: 30.0,
        duration: 300.0,
        size: (1920.0, 1080.0),
        pixel_aspect: 1.0,
        interactive: true,
    };
    Instance::new(plugin(), Context::Generator, &setup).expect("an instance")
}

fn set(inst: &Instance, name: &str, v: Value) {
    inst.set_values(&[(name.into(), Animation::constant(v))]);
    inst.changed(&[name.into()], ChangeReason::User, 0.0);
}

fn value(inst: &Instance, name: &str) -> Value {
    inst.values()
        .into_iter()
        .find(|(n, _)| n == name)
        .expect(name)
        .1
        .value
}

fn info(inst: &Instance, name: &str) -> bv_ofx::ParamInfo {
    inst.param_infos()
        .into_iter()
        .find(|p| p.name == name)
        .expect(name)
}

/// A folder of the test's own for its files.
fn folder(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("bananav-credit-roll-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a folder");
    dir
}

#[test]
fn the_position_box_and_the_files_are_described() {
    let context = plugin().info.context(Context::Generator).unwrap();
    let param = |name: &str| context.params.iter().find(|p| p.name == name).expect(name);
    // Left, Right and Top under Position, which has VEGAS's box.
    assert!(param("position").interact.is_some());
    for name in ["left", "right", "top"] {
        assert_eq!(param(name).parent, "position");
    }
    assert_eq!(param("top").default.num(0), 0.1);
    // Files to read and files to write.
    for (name, write) in [
        ("importText", false),
        ("importProperties", false),
        ("exportText", true),
        ("exportProperties", true),
    ] {
        let p = param(name);
        assert_eq!(p.string_mode, "OfxParamStringIsFilePath");
        assert_eq!(p.file_to_write, write, "{name}");
        assert_eq!(p.parent, "files");
        assert!(!p.animates);
    }
}

#[test]
fn each_effect_shows_what_it_uses() {
    let inst = instance();
    inst.editing(true);
    // Scrolling credits: the direction; the top greyed.
    assert!(!info(&inst, "direction").secret);
    assert!(info(&inst, "in").secret && info(&inst, "out").secret && info(&inst, "display").secret);
    assert!(!info(&inst, "top").enabled);
    // A timed sequence: its ways in and out, how many lines, its top.
    set(&inst, "effect", Value::Num(vec![1.0]));
    assert!(info(&inst, "direction").secret);
    assert!(!info(&inst, "in").secret && !info(&inst, "display").secret);
    assert!(info(&inst, "top").enabled);
}

#[test]
fn credits_come_from_text_files_and_go_to_them() {
    let dir = folder("text");
    let inst = instance();
    inst.editing(true);
    // Windows' code page, as Notepad saved VEGAS's credits for years: in
    // place of the sample credits.
    let file = dir.join("credits.txt");
    std::fs::write(&file, b" Cast\r\nThe Hero\tCaf\xE9\r\n// a note\r\n").unwrap();
    set(&inst, "importText", Value::Text(file.display().to_string()));
    assert_eq!(
        value(&inst, "text").text(),
        " Cast\nThe Hero\tCafé\n// a note"
    );
    // The path goes back to empty: the same file can be picked again,
    // and then follows the credits there are.
    assert_eq!(value(&inst, "importText").text(), "");
    set(&inst, "importText", Value::Text(file.display().to_string()));
    assert_eq!(
        value(&inst, "text").text(),
        " Cast\nThe Hero\tCafé\n// a note\n Cast\nThe Hero\tCafé\n// a note"
    );
    // Exported as UTF-8 with Windows' line ends, ".txt" added.
    set(&inst, "text", Value::Text(" Thanks\nEveryone".into()));
    let out = dir.join("exported");
    set(&inst, "exportText", Value::Text(out.display().to_string()));
    assert_eq!(
        std::fs::read_to_string(dir.join("exported.txt")).unwrap(),
        " Thanks\r\nEveryone"
    );
    assert_eq!(value(&inst, "exportText").text(), "");
    assert!(inst.persistent_message().is_none());
    // A file that cannot be read says so, until one works.
    set(
        &inst,
        "importText",
        Value::Text(dir.join("missing.txt").display().to_string()),
    );
    let m = inst.persistent_message().expect("a message");
    assert_eq!(m.text, "The credits text file could not be read.");
    set(&inst, "importText", Value::Text(file.display().to_string()));
    assert!(inst.persistent_message().is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn properties_go_to_files_and_come_back() {
    let dir = folder("properties");
    let a = instance();
    a.editing(true);
    set(&a, "effect", Value::Num(vec![1.0]));
    set(&a, "left", Value::Num(vec![0.25]));
    set(&a, "background", Value::Num(vec![0.0, 0.0, 0.5, 1.0]));
    set(&a, "titleSize", Value::Num(vec![60.0]));
    set(&a, "itemBold", Value::Num(vec![1.0]));
    set(&a, "text", Value::Text(" Mine".into()));
    let file = dir.join("look.xml");
    set(
        &a,
        "exportProperties",
        Value::Text(file.display().to_string()),
    );
    let xml = std::fs::read_to_string(&file).unwrap();
    assert!(
        xml.contains("<Property name=\"left\" value=\"0.25\"/>"),
        "{xml}"
    );
    assert!(!xml.contains("Mine"), "the credits stay out");
    // Another roll takes them, its credits kept.
    let b = instance();
    b.editing(true);
    set(&b, "text", Value::Text(" Theirs".into()));
    set(
        &b,
        "importProperties",
        Value::Text(file.display().to_string()),
    );
    assert_eq!(value(&b, "effect").num(0), 1.0);
    assert_eq!(value(&b, "left").num(0), 0.25);
    assert_eq!(value(&b, "titleSize").num(0), 60.0);
    assert_eq!(value(&b, "itemBold").num(0), 1.0);
    assert_eq!(value(&b, "background").num(2), 0.5);
    assert_eq!(value(&b, "text").text(), " Theirs");
    // The timed sequence's settings show at once.
    assert!(info(&b, "direction").secret && info(&b, "top").enabled);
    // A file of something else says so.
    let other = dir.join("other.xml");
    std::fs::write(&other, "<?xml version=\"1.0\"?><Nothing/>").unwrap();
    set(
        &b,
        "importProperties",
        Value::Text(other.display().to_string()),
    );
    let m = b.persistent_message().expect("a message");
    assert_eq!(m.text, "The file holds no credit roll properties.");
    let _ = std::fs::remove_dir_all(&dir);
}
