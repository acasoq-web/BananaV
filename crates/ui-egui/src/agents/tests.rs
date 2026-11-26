//! The tools on a BananaV without a window, the MCP server on them, and
//! the open window's line: its calls run between frames, what writes
//! waits for the user's yes.

use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

use super::{Backend, Headless, Outcome};

fn args(v: Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}

fn call(h: &mut Headless, tool: &str, a: Value) -> Outcome {
    h.call(tool, &args(a))
}

fn ok(h: &mut Headless, tool: &str, a: Value) -> Value {
    match call(h, tool, a) {
        Ok(o) => o.value,
        Err(e) => panic!("{tool}: {e}"),
    }
}

fn demo() -> Headless {
    crate::i18n::set_language(crate::i18n::Language::English);
    let mut h = Headless::new();
    h.app.load_demo();
    h.pump();
    h
}

/// Tracks, events, markers, envelopes, undo and redo, and the model as
/// JSON, read and patched.
#[test]
fn the_tools_edit_a_project() {
    let mut h = demo();
    let info = ok(&mut h, "project_info", json!({"detail": true}));
    let tracks = info["tracks"].as_u64().expect("tracks");
    assert!(tracks >= 2, "{info}");
    assert_eq!(
        info["track_list"].as_array().map(Vec::len),
        Some(tracks as usize)
    );
    // A track at the top, named.
    let t = ok(
        &mut h,
        "add_track",
        json!({"kind": "video", "index": 0, "name": "Titles"}),
    );
    assert_eq!(t["index"], 0);
    assert_eq!(t["name"], "Titles");
    assert_eq!(
        ok(&mut h, "list_tracks", json!({}))["tracks"]
            .as_array()
            .map(Vec::len),
        Some(tracks as usize + 1)
    );
    let t = ok(
        &mut h,
        "set_track",
        json!({"track": 0, "muted": true, "level": 0.5}),
    );
    assert_eq!(t["muted"], true);
    assert_eq!(t["level"], 0.5);
    // The demo's first event: moved, trimmed, split, the piece deleted.
    let events = ok(&mut h, "list_events", json!({}))["events"].clone();
    let first = events[0].clone();
    let id = first["id"].as_u64().expect("an id");
    let start = first["start"].as_f64().expect("a start");
    let moved = ok(
        &mut h,
        "move_event",
        json!({"event": id, "start": start + 1.0}),
    );
    assert!(
        (moved["start"].as_f64().unwrap() - (start + 1.0)).abs() < 0.05,
        "{moved}"
    );
    let end = moved["end"].as_f64().unwrap();
    let trimmed = ok(&mut h, "trim_event", json!({"event": id, "end": end - 0.5}));
    assert!(
        (trimmed["end"].as_f64().unwrap() - (end - 0.5)).abs() < 0.05,
        "{trimmed}"
    );
    let middle = (trimmed["start"].as_f64().unwrap() + trimmed["end"].as_f64().unwrap()) / 2.0;
    let split = ok(&mut h, "split_event", json!({"event": id, "at": middle}));
    let piece = split["new_events"][0]["id"].as_u64().expect("the piece");
    let changed = ok(
        &mut h,
        "set_event",
        json!({"event": id, "fade_in": 0.25, "fade_in_curve": "linear"}),
    );
    assert!(
        (changed["fade_in"].as_f64().unwrap() - 0.25).abs() < 0.05,
        "{changed}"
    );
    ok(&mut h, "delete_events", json!({"events": [piece]}));
    assert!(call(&mut h, "set_event", json!({"event": piece, "muted": true})).is_err());
    // Undone and redone.
    let undone = ok(&mut h, "undo", json!({}));
    assert!(undone["undone"].is_string(), "{undone}");
    assert!(call(&mut h, "set_event", json!({"event": piece, "muted": true})).is_ok());
    ok(&mut h, "undo", json!({}));
    ok(&mut h, "redo", json!({}));
    // Markers and regions.
    let m = ok(&mut h, "add_marker", json!({"at": 1.0, "name": "Here"}));
    ok(
        &mut h,
        "add_region",
        json!({"start": 2.0, "end": 3.0, "name": "Part"}),
    );
    let marks = ok(&mut h, "list_markers", json!({}));
    assert!(
        marks["markers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["name"] == "Here")
    );
    assert!(
        marks["regions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["name"] == "Part")
    );
    ok(&mut h, "delete_marker", json!({"marker": m["id"]}));
    // An envelope on the video track of the event.
    let track = first["track"].as_u64().unwrap() + 1;
    let kind = ok(&mut h, "list_tracks", json!({}))["tracks"][track as usize]["kind"].clone();
    let env = if kind == "video" {
        "composite_level"
    } else {
        "volume"
    };
    let e = ok(
        &mut h,
        "set_envelope",
        json!({"track": track, "kind": env, "points": [{"at": 0.0, "value": 1.0}, {"at": 2.0, "value": 0.5, "curve": "linear"}]}),
    );
    let points = e["envelopes"][0]["points"]
        .as_array()
        .expect("points")
        .len();
    assert!(points >= 2, "{e}");
    // The model as JSON: read, patched, and a patch that breaks it refused.
    let width = ok(
        &mut h,
        "project_json",
        json!({"pointer": "/settings/width"}),
    )["value"]
        .clone();
    assert_eq!(width, info["settings"]["width"]);
    ok(
        &mut h,
        "patch_project",
        json!({"patch": [{"op": "replace", "path": "/tracks/0/name", "value": "Patched"}]}),
    );
    assert_eq!(
        ok(&mut h, "list_tracks", json!({}))["tracks"][0]["name"],
        "Patched"
    );
    let before = h.app.history.revision();
    assert!(
        call(
            &mut h,
            "patch_project",
            json!({"patch": [{"op": "replace", "path": "/next_id", "value": 0}]})
        )
        .is_err()
    );
    assert_eq!(h.app.history.revision(), before, "nothing changed");
    // Checked arguments.
    let e = call(&mut h, "add_marker", json!({"time": 1.0})).unwrap_err();
    assert!(e.contains("time"), "{e}");
    assert!(call(&mut h, "play", json!({})).is_err(), "the window's own");
    // A new project.
    let p = ok(
        &mut h,
        "new_project",
        json!({"width": 1280, "height": 720, "frame_rate": 29.97}),
    );
    assert_eq!(p["settings"]["width"], 1280);
    // NTSC's exact rate, 30000/1001.
    assert!((p["settings"]["frame_rate"].as_f64().unwrap() - 29.97).abs() < 0.001);
    assert_eq!(p["dirty"], false);
}

/// An effect added to an event, its parameters read, set and keyframed; a
/// generated title placed and its text changed; a frame seen.
#[test]
fn the_tools_set_effects_and_see_frames() {
    let mut h = demo();
    let video = ok(
        &mut h,
        "list_available_effects",
        json!({"kind": "video", "search": "blur"}),
    );
    let id = video["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|e| {
            e["id"]
                .as_str()
                .filter(|i| i.starts_with("org.bananav."))
                .map(str::to_string)
        })
        .expect("one of BananaV's blurs");
    let events = ok(&mut h, "list_events", json!({}))["events"].clone();
    let event = events
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            let t = e["track"].as_u64().unwrap() as usize;
            h.app.project().tracks[t].kind == bv_timeline::TrackKind::Video
        })
        .expect("a video event")
        .clone();
    let ev = event["id"].as_u64().unwrap();
    let added = ok(&mut h, "add_effect", json!({"event": ev, "effect": id}));
    let index = added["index"].as_u64().unwrap();
    let param = added["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["type"] == "number" && p["min"].is_number() && p["max"].is_number())
        .expect("a number to set")
        .clone();
    let name = param["name"].as_str().unwrap();
    let (lo, hi) = (
        param["min"].as_f64().unwrap(),
        param["max"].as_f64().unwrap(),
    );
    let mid = (lo + hi) / 2.0;
    let set = ok(
        &mut h,
        "set_effect",
        json!({"event": ev, "index": index, "values": {name: mid}}),
    );
    let got = set["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .unwrap()["value"]
        .as_f64()
        .unwrap();
    assert!((got - mid).abs() < 1e-6, "{got} {mid}");
    // Keyframed at the event's middle: two keyframes, from its start.
    let at = (event["start"].as_f64().unwrap() + event["end"].as_f64().unwrap()) / 2.0;
    let keyed = ok(
        &mut h,
        "set_effect",
        json!({"event": ev, "index": index, "values": {name: hi}, "at": at}),
    );
    let p = keyed["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .unwrap()
        .clone();
    assert_eq!(p["keyframes"].as_array().map(Vec::len), Some(2), "{p}");
    // Animated: a value without `at` is refused until its keyframes go.
    assert!(
        call(
            &mut h,
            "set_effect",
            json!({"event": ev, "index": index, "values": {name: lo}})
        )
        .is_err()
    );
    ok(
        &mut h,
        "set_effect",
        json!({"event": ev, "index": index, "values": {name: lo}, "clear_keyframes": true}),
    );
    let chain = ok(&mut h, "list_effects", json!({"event": ev}));
    assert_eq!(
        chain["effects"].as_array().map(Vec::len),
        Some(index as usize + 1)
    );
    ok(
        &mut h,
        "remove_effect",
        json!({"event": ev, "index": index}),
    );
    // Titles & Text on a track of its own.
    ok(&mut h, "add_track", json!({"kind": "video", "index": 0}));
    let title = ok(
        &mut h,
        "add_generated_media",
        json!({"generator": "org.bananav.TitlesText", "track": 0, "at": 0.0, "length": 3.0, "values": {"text": "Hello"}}),
    );
    let t = title["events"][0]["id"]
        .as_u64()
        .expect("the title's event");
    let g = ok(&mut h, "get_generated_media", json!({"event": t}));
    let text = g["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "text")
        .unwrap()["value"]
        .clone();
    assert_eq!(text, "Hello");
    let g = ok(
        &mut h,
        "set_generated_media",
        json!({"event": t, "values": {"text": "Bye"}}),
    );
    let text = g["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "text")
        .unwrap()["value"]
        .clone();
    assert_eq!(text, "Bye");
    // A frame, as a picture.
    let out = call(&mut h, "preview_frame", json!({"at": 1.0, "width": 320})).expect("a frame");
    let png = out.image.expect("a picture");
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(out.value["width"], 320);
    let templates = ok(&mut h, "list_render_templates", json!({"search": "wav"}));
    assert!(!templates["templates"].as_array().unwrap().is_empty());
}

/// A render started, followed until done: its file there.
#[test]
fn a_render_goes_to_its_file() {
    let mut h = demo();
    let dir = std::env::temp_dir().join(format!("bv-agents-render-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("sound.wav");
    let _ = std::fs::remove_file(&file);
    let started = ok(
        &mut h,
        "render",
        json!({"path": file.to_string_lossy(), "from": 0.0, "to": 1.0}),
    );
    assert!(
        started["template"].as_str().unwrap().starts_with("wav"),
        "{started}"
    );
    let t0 = Instant::now();
    let status = loop {
        h.idle();
        let s = ok(&mut h, "render_status", json!({}));
        if !matches!(s["state"].as_str(), Some("starting" | "running")) {
            break s;
        }
        assert!(t0.elapsed() < Duration::from_secs(60), "the render ends");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(status["state"], "done", "{status}");
    assert!(file.exists());
    assert!(
        call(&mut h, "render", json!({"path": file.to_string_lossy()})).is_err(),
        "not over it"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The MCP server: the handshake, the tools listed, a call that edits.
#[test]
fn an_mcp_client_edits_a_project() {
    let mut h = demo();
    let lines = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"add_marker","arguments":{"at":2.5,"name":"From MCP"}}}"#,
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"preview_frame","arguments":{"width":64}}}"#,
    ];
    let input = Box::new(std::io::Cursor::new(lines.join("\n").into_bytes()));
    let mut out = Vec::new();
    super::mcp::serve(&mut h, input, &mut out).expect("served");
    let replies: Vec<Value> = String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(replies.len(), 4);
    let tools = replies[1]["result"]["tools"].as_array().unwrap();
    assert!(tools.len() > 40, "{}", tools.len());
    assert!(
        !tools.iter().any(|t| t["name"] == "play"),
        "headless: no playback"
    );
    assert_eq!(replies[2]["result"]["isError"], false);
    assert!(h.app.project().markers.iter().any(|m| m.name == "From MCP"));
    let content = replies[3]["result"]["content"].as_array().unwrap();
    assert!(
        content
            .iter()
            .any(|c| c["type"] == "image" && c["mimeType"] == "image/png")
    );
}

/// The open window with agents allowed: a client's call runs between its
/// frames; one that writes waits for the user's yes, and a no is the
/// agent's answer.
#[test]
fn the_window_runs_agents_calls_and_asks_first() {
    use crate::app::BananaApp;
    crate::i18n::set_language(crate::i18n::Language::English);
    let dir = std::env::temp_dir().join(format!("bv-agents-window-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let address = dir.join("agents.json");
    let ctx = crate::egui::Context::default();
    let mut app = BananaApp::new(ctx.clone());
    app.welcome_for_review("closed");
    app.settings = app.settings.unsaved();
    app.load_demo();
    app.agents.address = Some(address.clone());
    app.settings.mods.agents = true;
    let frame = |app: &mut BananaApp| {
        let raw = crate::egui::RawInput {
            screen_rect: Some(crate::egui::Rect::from_min_size(
                crate::egui::Pos2::ZERO,
                crate::egui::vec2(1600.0, 960.0),
            )),
            focused: false,
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| app.frame(ui));
        out.textures_delta.clear();
    };
    frame(&mut app);
    assert!(app.agents_listening().0, "listening");
    // A call that edits: done in a frame.
    let file = address.clone();
    let client = std::thread::spawn(move || {
        let mut live = super::link::Live::at(file);
        let marker = live.call("add_marker", &args(json!({"at": 1.0, "name": "Live"})));
        let save = live.call(
            "save_project",
            &args(json!({"path": "C:/nowhere/x.bvproj"})),
        );
        (marker, save)
    });
    let t0 = Instant::now();
    while app.agent_asks() == 0 {
        frame(&mut app);
        assert!(
            t0.elapsed() < Duration::from_secs(20),
            "the client's calls came"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(app.project().markers.iter().any(|m| m.name == "Live"));
    // Saving waits for the user: a no.
    app.agent_answer(false);
    let (marker, save) = client.join().expect("the client");
    assert!(marker.is_ok());
    assert_eq!(save.unwrap_err(), "The user said no.");
    // Agents off: the window stops listening.
    app.settings.mods.agents = false;
    frame(&mut app);
    assert!(!app.agents_listening().0);
    assert!(!address.exists());
    let _ = std::fs::remove_dir_all(&dir);
}
