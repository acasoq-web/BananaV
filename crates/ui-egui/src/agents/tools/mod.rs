//! The tools: each a name, a title, a description and its arguments' JSON
//! Schema (what MCP's `tools/list` gives an agent and `bananav-cli tools`
//! prints), and what it does to a [`BananaApp`]. Arguments are checked
//! against the schema before a tool runs (missing, unknown or of the wrong
//! kind: the agent is told which and why).

mod edit;
mod effects;
mod output;
mod project;
mod timeline;

use bv_core::{FrameRate, Tick};
use bv_timeline::{Clip, MediaInfo, MediaSource, Project, Track, TrackKind};
use serde_json::{Map, Value, json};

use super::Outcome;
use crate::app::BananaApp;
use crate::i18n::tr;

/// What a tool does, for the agent's program (its hints) and the window
/// (what waits for the user's yes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Only reads.
    Read,
    /// Edits the project: one undo step.
    Edit,
    /// Takes something out of the project (an undo brings it back).
    Remove,
    /// Writes a file: in the window, only with the user's yes.
    Write,
    /// Puts another project in the place of the open one: in the window,
    /// only with the user's yes.
    Replace,
    /// The window's own (playback): only on the open window.
    Window,
}

/// A tool.
#[derive(Debug)]
pub(crate) struct Tool {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// Its arguments: JSON Schema's `properties` (each with its `type`).
    pub properties: fn() -> Value,
    /// The arguments it cannot do without.
    pub required: &'static [&'static str],
    pub kind: Kind,
    pub run: fn(&mut BananaApp, &Args<'_>) -> Outcome,
}

/// Every tool, in the order they are listed.
fn all() -> &'static [Tool] {
    static ALL: std::sync::OnceLock<Vec<Tool>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let mut v = Vec::new();
        v.extend(project::tools());
        v.extend(timeline::tools());
        v.extend(effects::tools());
        v.extend(output::tools());
        v.extend(edit::tools());
        v
    })
}

fn find(name: &str, live: bool) -> Option<&'static Tool> {
    all()
        .iter()
        .find(|t| t.name == name && (live || t.kind != Kind::Window))
}

/// Whether there is a tool `name` (on the open window: `live`).
pub(crate) fn exists(name: &str, live: bool) -> bool {
    find(name, live).is_some()
}

/// The tools' definitions as MCP lists them.
pub(crate) fn list(live: bool) -> Vec<Value> {
    all()
        .iter()
        .filter(|t| live || t.kind != Kind::Window)
        .map(|t| {
            let mut schema = json!({
                "type": "object",
                "properties": (t.properties)(),
                "additionalProperties": false,
            });
            if !t.required.is_empty() {
                schema["required"] = json!(t.required);
            }
            json!({
                "name": t.name,
                "title": t.title,
                "description": t.description,
                "inputSchema": schema,
                "annotations": {
                    "title": t.title,
                    "readOnlyHint": t.kind == Kind::Read,
                    "destructiveHint": matches!(t.kind, Kind::Remove | Kind::Replace | Kind::Write),
                    "idempotentHint": t.kind == Kind::Read,
                    "openWorldHint": false,
                },
            })
        })
        .collect()
}

/// What the window asks the user before tool `name` runs with `args`
/// (`None`: it runs at once).
pub(crate) fn question(app: &BananaApp, name: &str, args: &Map<String, Value>) -> Option<String> {
    let tool = find(name, true)?;
    let path = args.get("path").and_then(Value::as_str).unwrap_or_default();
    match tool.kind {
        Kind::Write if name == "render" => {
            Some(trf!("An AI agent asks to render the project to {}.", path))
        }
        Kind::Write if name == "save_project" && path.is_empty() => {
            Some(tr("An AI agent asks to save the project.").to_string())
        }
        Kind::Write => Some(trf!("An AI agent asks to write {}.", path)),
        Kind::Replace => {
            let what = if name == "new_project" {
                tr("An AI agent asks to start a new project in place of this one.").to_string()
            } else {
                trf!(
                    "An AI agent asks to open {} in place of this project.",
                    path
                )
            };
            Some(if app.is_dirty() {
                format!("{what}\n{}", tr("The changes not saved are lost."))
            } else {
                what
            })
        }
        _ => None,
    }
}

/// Runs tool `name` with `args` on `app` (the open window's: `live`).
pub(crate) fn call(
    app: &mut BananaApp,
    name: &str,
    args: &Map<String, Value>,
    live: bool,
) -> Outcome {
    let Some(tool) = find(name, live) else {
        return Err(trf!("Unknown tool: {}", name));
    };
    check(tool, args)?;
    let args = Args { map: args };
    (tool.run)(app, &args)
}

/// `args` against `tool`'s schema: every required one there, none
/// unknown, each of its type.
fn check(tool: &Tool, args: &Map<String, Value>) -> Result<(), String> {
    let properties = (tool.properties)();
    let known = properties.as_object().cloned().unwrap_or_default();
    for (name, value) in args {
        let Some(schema) = known.get(name) else {
            let names: Vec<&str> = known.keys().map(String::as_str).collect();
            return Err(if names.is_empty() {
                trf!("Unknown argument \"{}\": the tool takes none.", name)
            } else {
                trf!(
                    "Unknown argument \"{}\" (the tool takes: {}).",
                    name,
                    names.join(", ")
                )
            });
        };
        if value.is_null() {
            continue;
        }
        let fits = |kind: &str| match kind {
            "number" => value.is_number(),
            "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
            "string" => value.is_string(),
            "boolean" => value.is_boolean(),
            "array" => value.is_array(),
            "object" => value.is_object(),
            _ => true,
        };
        let ok = match schema.get("type") {
            Some(Value::String(k)) => fits(k),
            Some(Value::Array(ks)) => ks.iter().filter_map(Value::as_str).any(fits),
            _ => true,
        };
        if !ok {
            return Err(trf!(
                "The argument \"{}\" is not of its type ({}).",
                name,
                schema["type"]
            ));
        }
        if let Some(Value::Array(options)) = schema.get("enum")
            && !options.contains(value)
        {
            let list: Vec<String> = options.iter().map(Value::to_string).collect();
            return Err(trf!(
                "The argument \"{}\" must be one of: {}.",
                name,
                list.join(", ")
            ));
        }
    }
    // Unknown ones first: a misspelt name is said as such.
    for name in tool.required {
        if args.get(*name).is_none_or(Value::is_null) {
            return Err(trf!("The argument \"{}\" is missing.", name));
        }
    }
    Ok(())
}

/// A tool's arguments.
#[derive(Debug)]
pub(crate) struct Args<'a> {
    map: &'a Map<String, Value>,
}

impl Args<'static> {
    /// No arguments.
    pub fn empty() -> Args<'static> {
        static EMPTY: std::sync::LazyLock<Map<String, Value>> = std::sync::LazyLock::new(Map::new);
        Args { map: &EMPTY }
    }
}

impl<'a> Args<'a> {
    /// The argument `k`, when given (a null counts as not).
    pub fn get(&self, k: &str) -> Option<&'a Value> {
        self.map.get(k).filter(|v| !v.is_null())
    }

    pub fn has(&self, k: &str) -> bool {
        self.get(k).is_some()
    }

    fn missing(k: &str) -> String {
        trf!("The argument \"{}\" is missing.", k)
    }

    pub fn opt_str(&self, k: &str) -> Result<Option<&'a str>, String> {
        self.get(k)
            .map(|v| {
                v.as_str()
                    .ok_or_else(|| trf!("The argument \"{}\" must be text.", k))
            })
            .transpose()
    }

    pub fn str(&self, k: &str) -> Result<&'a str, String> {
        self.opt_str(k)?.ok_or_else(|| Self::missing(k))
    }

    pub fn opt_f64(&self, k: &str) -> Result<Option<f64>, String> {
        self.get(k)
            .map(|v| {
                v.as_f64()
                    .filter(|x| x.is_finite())
                    .ok_or_else(|| trf!("The argument \"{}\" must be a number.", k))
            })
            .transpose()
    }

    pub fn opt_u64(&self, k: &str) -> Result<Option<u64>, String> {
        self.get(k)
            .map(|v| {
                v.as_u64().ok_or_else(|| {
                    trf!("The argument \"{}\" must be a whole number, 0 or more.", k)
                })
            })
            .transpose()
    }

    pub fn u64(&self, k: &str) -> Result<u64, String> {
        self.opt_u64(k)?.ok_or_else(|| Self::missing(k))
    }

    pub fn opt_bool(&self, k: &str) -> Result<Option<bool>, String> {
        self.get(k)
            .map(|v| {
                v.as_bool()
                    .ok_or_else(|| trf!("The argument \"{}\" must be true or false.", k))
            })
            .transpose()
    }

    pub fn bool_or(&self, k: &str, default: bool) -> Result<bool, String> {
        Ok(self.opt_bool(k)?.unwrap_or(default))
    }

    /// A time in seconds (0 or more).
    pub fn opt_time(&self, k: &str) -> Result<Option<Tick>, String> {
        self.opt_f64(k)?
            .map(|s| {
                (0.0..1e9)
                    .contains(&s)
                    .then(|| Tick::from_seconds_f64(s))
                    .ok_or_else(|| {
                        trf!(
                            "The argument \"{}\" must be a time in seconds, 0 or more.",
                            k
                        )
                    })
            })
            .transpose()
    }

    pub fn time(&self, k: &str) -> Result<Tick, String> {
        self.opt_time(k)?.ok_or_else(|| Self::missing(k))
    }

    /// One id or a list of them.
    pub fn ids(&self, k: &str) -> Result<Vec<u64>, String> {
        let bad = || trf!("The argument \"{}\" must be an id or a list of ids.", k);
        match self.get(k) {
            None => Err(Self::missing(k)),
            Some(Value::Array(a)) => a.iter().map(|v| v.as_u64().ok_or_else(bad)).collect(),
            Some(v) => v.as_u64().map(|i| vec![i]).ok_or_else(bad),
        }
    }
}

/// A time for the agent: seconds, to a millionth.
pub(super) fn secs(t: Tick) -> f64 {
    (t.as_seconds_f64() * 1e6).round() / 1e6
}

/// A frame rate as a number of frames a second.
pub(super) fn fps(r: FrameRate) -> f64 {
    (f64::from(r.num) / f64::from(r.den.max(1)) * 1e6).round() / 1e6
}

/// Track `index` (0 at the top).
pub(super) fn track_at(p: &Project, index: u64) -> Result<&Track, String> {
    usize::try_from(index)
        .ok()
        .and_then(|i| p.tracks.get(i))
        .ok_or_else(|| {
            trf!(
                "There is no track {} (the project has {}).",
                index,
                p.tracks.len()
            )
        })
}

/// Event `id`.
pub(super) fn clip_of(p: &Project, id: u64) -> Result<&Clip, String> {
    p.clip(bv_timeline::ClipId(id))
        .ok_or_else(|| trf!("There is no event {}.", id))
}

/// Media `id`.
pub(super) fn media_of(p: &Project, id: u64) -> Result<&MediaInfo, String> {
    p.media(bv_timeline::MediaId(id))
        .ok_or_else(|| trf!("There is no media {}.", id))
}

pub(super) fn kind_name(k: TrackKind) -> &'static str {
    match k {
        TrackKind::Video => "video",
        TrackKind::Audio => "audio",
    }
}

/// A track for the agent.
pub(super) fn track_json(index: usize, t: &Track) -> Value {
    json!({
        "index": index,
        "id": t.id.0,
        "kind": kind_name(t.kind),
        "name": t.name,
        "muted": t.muted,
        "solo": t.solo,
        "level": t.level,
        "pan": t.pan,
        "composite_mode": t.composite_mode.name(),
        "adjustment": t.adjustment,
        "events": t.clips.len(),
        "effects": t.effects.iter().map(effects::effect_name).collect::<Vec<_>>(),
    })
}

/// An event for the agent.
pub(super) fn event_json(p: &Project, track: usize, c: &Clip) -> Value {
    let name = p.media(c.media).map(|m| m.name.clone()).unwrap_or_default();
    json!({
        "id": c.id.0,
        "track": track,
        "media": c.media.0,
        "name": name,
        "start": secs(c.start),
        "end": secs(c.end()),
        "length": secs(c.duration),
        "media_offset": secs(c.source_in),
        "rate": c.rate,
        "level": c.level,
        "muted": c.muted,
        "locked": c.locked,
        "fade_in": secs(c.fade_in),
        "fade_out": secs(c.fade_out),
        "group": c.group.map(|g| g.0),
        "effects": c.effects.iter().map(effects::effect_name).collect::<Vec<_>>(),
        "takes": c.takes.len().max(1),
    })
}

/// A media for the agent.
pub(super) fn media_json(m: &MediaInfo) -> Value {
    let path = match &m.source {
        MediaSource::File { path } => Value::String(path.clone()),
        _ => Value::Null,
    };
    json!({
        "id": m.id.0,
        "name": m.name,
        "path": path,
        "generated": matches!(m.source, MediaSource::Generated(_)),
        "length": secs(m.duration),
        "still": m.still,
        "video": m.video.map(|v| json!({
            "width": v.width,
            "height": v.height,
            "frame_rate": fps(v.frame_rate),
        })),
        "audio": m.audio.map(|a| json!({
            "sample_rate": serde_json::to_value(a.sample_rate).unwrap_or(Value::Null),
            "channels": a.channels,
        })),
        "effects": m.effects.iter().map(effects::effect_name).collect::<Vec<_>>(),
    })
}

/// A property of a tool's schema.
pub(super) fn arg(kind: &str, description: &str) -> Value {
    json!({"type": kind, "description": description})
}

/// A property whose value is one of `options`.
pub(super) fn choice(options: &[&str], description: &str) -> Value {
    json!({"type": "string", "enum": options, "description": description})
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every tool has a unique name, a description and a schema whose
    /// required arguments are among its properties.
    #[test]
    fn the_tools_describe_themselves() {
        let mut names = std::collections::BTreeSet::new();
        for t in all() {
            assert!(names.insert(t.name), "{} twice", t.name);
            assert!(
                t.name.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
                "{}",
                t.name
            );
            assert!(
                !t.description.is_empty() && !t.title.is_empty(),
                "{}",
                t.name
            );
            let props = (t.properties)();
            let props = props.as_object().expect("properties");
            for r in t.required {
                assert!(props.contains_key(*r), "{}: {r}", t.name);
            }
            for (k, v) in props {
                assert!(v.get("type").is_some(), "{}.{k}", t.name);
                assert!(v.get("description").is_some(), "{}.{k}", t.name);
            }
        }
        assert!(list(false).len() < list(true).len());
    }

    #[test]
    fn arguments_are_checked_against_the_schema() {
        let tool = find("add_marker", false).expect("add_marker");
        let ok: Map<String, Value> =
            serde_json::from_value(json!({"at": 1.5, "name": "A"})).unwrap();
        assert!(check(tool, &ok).is_ok());
        let missing: Map<String, Value> = serde_json::from_value(json!({"name": "A"})).unwrap();
        assert!(check(tool, &missing).is_err());
        let unknown: Map<String, Value> =
            serde_json::from_value(json!({"at": 1, "colour": 2})).unwrap();
        assert!(check(tool, &unknown).unwrap_err().contains("colour"));
        let wrong: Map<String, Value> = serde_json::from_value(json!({"at": "soon"})).unwrap();
        assert!(check(tool, &wrong).is_err());
    }
}
