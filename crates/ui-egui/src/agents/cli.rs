//! The command line: `bananav-cli <command>` (on Windows; `bananav
//! <command>` elsewhere, and from programs that start it with pipes). Its
//! results are JSON on the standard output, for programs and agents, and
//! its help; what else a person reads — progress, errors, notes — goes to
//! the standard error, translated. Exit codes: 0 done, 1 a tool failed,
//! 2 the command was not understood.

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Map, Value, json};

use super::link::Live;
use super::{Backend, Headless, Outcome};
use crate::i18n::tr;

/// The commands, for the help and to tell them from a file to open.
const COMMANDS: [&str; 12] = [
    "help",
    "version",
    "info",
    "render",
    "frame",
    "templates",
    "effects",
    "tools",
    "call",
    "run",
    "mcp",
    "--version",
];

/// Whether `args` (the program's, without its name) ask for the command
/// line: `--cli` first (what `bananav-cli` passes), or one of the
/// commands that is not a file there.
pub(crate) fn wanted(args: &[String]) -> bool {
    match args.first().map(String::as_str) {
        Some("--cli") => true,
        Some("--help" | "-h") => true,
        Some(first) => COMMANDS.contains(&first) && !std::path::Path::new(first).exists(),
        None => false,
    }
}

/// Runs the command line on `args` (the program's, without its name):
/// the exit code.
pub(crate) fn main(args: Vec<String>) -> i32 {
    let mut args = args;
    if args.first().is_some_and(|a| a == "--cli") {
        args.remove(0);
    }
    crate::settings::UiSettings::apply_language_only();
    match run(args) {
        Ok(()) => 0,
        Err(Failure::Usage(message)) => {
            eprintln!("{message}");
            eprintln!("{}", tr("Run \"bananav-cli help\" for the commands."));
            2
        }
        Err(Failure::Tool(message)) => {
            eprintln!("{message}");
            1
        }
    }
}

#[derive(Debug)]
enum Failure {
    /// The command was not understood.
    Usage(String),
    /// A tool failed.
    Tool(String),
}

/// A command's words: the positional ones and the options (`--name
/// value`, or `--name` alone for a switch).
#[derive(Debug, Default)]
struct Words {
    plain: Vec<String>,
    options: Vec<(String, Option<String>)>,
}

/// Options that take no value.
const SWITCHES: [&str; 3] = ["--live", "--overwrite", "--json"];

impl Words {
    fn of(args: &[String]) -> Words {
        let mut w = Words::default();
        let mut i = 0;
        while i < args.len() {
            let a = &args[i];
            if let Some(name) = a.strip_prefix("--") {
                if let Some((n, v)) = name.split_once('=') {
                    w.options.push((format!("--{n}"), Some(v.to_string())));
                } else if SWITCHES.contains(&a.as_str()) {
                    w.options.push((a.clone(), None));
                } else if a == "--save" {
                    // A file after it, unless another option comes.
                    let next = args.get(i + 1).filter(|n| !n.starts_with("--"));
                    w.options.push((a.clone(), next.cloned()));
                    if next.is_some() {
                        i += 1;
                    }
                } else {
                    w.options.push((a.clone(), args.get(i + 1).cloned()));
                    i += 1;
                }
            } else {
                w.plain.push(a.clone());
            }
            i += 1;
        }
        w
    }

    fn has(&self, name: &str) -> bool {
        self.options.iter().any(|(n, _)| n == name)
    }

    fn value(&self, name: &str) -> Result<Option<&str>, Failure> {
        match self.options.iter().find(|(n, _)| n == name) {
            None => Ok(None),
            Some((_, Some(v))) => Ok(Some(v)),
            Some((_, None)) => Err(Failure::Usage(trf!("{} needs a value.", name))),
        }
    }

    fn seconds(&self, name: &str) -> Result<Option<f64>, Failure> {
        self.value(name)?
            .map(|v| {
                v.parse::<f64>()
                    .ok()
                    .filter(|s| s.is_finite() && *s >= 0.0)
                    .ok_or_else(|| Failure::Usage(trf!("{} takes seconds: {}", name, v)))
            })
            .transpose()
    }

    /// Options not among `known`: a typing mistake.
    fn check(&self, known: &[&str]) -> Result<(), Failure> {
        match self
            .options
            .iter()
            .find(|(n, _)| !known.contains(&n.as_str()))
        {
            Some((n, _)) => Err(Failure::Usage(trf!("Unknown option: {}", n))),
            None => Ok(()),
        }
    }

    fn plain(&self, i: usize, what: &str) -> Result<&str, Failure> {
        self.plain
            .get(i)
            .map(String::as_str)
            .ok_or_else(|| Failure::Usage(trf!("Missing: {}", what)))
    }
}

/// A result printed as JSON on the standard output.
fn print(value: &Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(
        out,
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_default()
    );
    let _ = out.flush();
}

fn args_of(pairs: &[(&str, Value)]) -> Map<String, Value> {
    pairs
        .iter()
        .filter(|(_, v)| !v.is_null())
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect()
}

/// Where the tools run: a BananaV of the command's own (with `project`
/// opened in it), or the open window's (`--live`).
fn backend(live: bool, project: Option<&str>) -> Result<Box<dyn Backend>, Failure> {
    let mut b: Box<dyn Backend> = if live {
        Box::new(Live::new())
    } else {
        Box::new(Headless::new())
    };
    if let Some(p) = project {
        let path = absolute(p);
        call(&mut *b, "open_project", args_of(&[("path", json!(path))]))?;
    }
    Ok(b)
}

/// A path as the program will look for it: from the folder the command
/// runs in.
fn absolute(path: &str) -> String {
    let p = PathBuf::from(path);
    let p = if p.is_absolute() {
        p
    } else {
        std::env::current_dir().map(|d| d.join(&p)).unwrap_or(p)
    };
    p.to_string_lossy().into_owned()
}

fn call(b: &mut dyn Backend, tool: &str, args: Map<String, Value>) -> Result<Value, Failure> {
    outcome(b.call(tool, &args))
}

fn outcome(o: Outcome) -> Result<Value, Failure> {
    o.map(|out| out.value).map_err(Failure::Tool)
}

/// Saves when asked (`--save` with or without a file), else says the
/// edits were left.
fn finish(b: &mut dyn Backend, words: &Words, project: Option<&str>) -> Result<(), Failure> {
    if words.has("--save") {
        let to = words
            .options
            .iter()
            .find(|(n, _)| n == "--save")
            .and_then(|(_, v)| v.as_deref())
            .or(project)
            .ok_or_else(|| {
                Failure::Usage(tr("--save needs a file: no project was opened.").into())
            })?;
        call(
            b,
            "save_project",
            args_of(&[("path", json!(absolute(to))), ("overwrite", json!(true))]),
        )?;
    } else if !b.live()
        && call(b, "project_info", Map::new())?
            .get("dirty")
            .and_then(Value::as_bool)
            == Some(true)
    {
        eprintln!(
            "{}",
            tr("The changes were not saved: add --save to keep them.")
        );
    }
    Ok(())
}

fn run(args: Vec<String>) -> Result<(), Failure> {
    let Some((command, rest)) = args.split_first() else {
        help();
        return Ok(());
    };
    let words = Words::of(rest);
    match command.as_str() {
        "help" | "--help" | "-h" => {
            help();
            Ok(())
        }
        "version" | "--version" => {
            print(&json!({"name": "BananaV", "version": env!("CARGO_PKG_VERSION")}));
            Ok(())
        }
        "tools" => {
            words.check(&["--live"])?;
            print(&Value::Array(super::tools::list(words.has("--live"))));
            Ok(())
        }
        "templates" => {
            words.check(&["--live"])?;
            let mut b = backend(words.has("--live"), None)?;
            print(&call(&mut *b, "list_render_templates", Map::new())?);
            Ok(())
        }
        "effects" => {
            words.check(&["--kind", "--search", "--live"])?;
            let mut b = backend(words.has("--live"), None)?;
            let args = args_of(&[
                (
                    "kind",
                    words.value("--kind")?.map_or(Value::Null, |k| json!(k)),
                ),
                (
                    "search",
                    words.value("--search")?.map_or(Value::Null, |k| json!(k)),
                ),
            ]);
            print(&call(&mut *b, "list_available_effects", args)?);
            Ok(())
        }
        "info" => {
            words.check(&["--live"])?;
            let live = words.has("--live");
            let project = if live {
                None
            } else {
                Some(words.plain(0, tr("the project"))?)
            };
            let mut b = backend(live, project)?;
            print(&call(
                &mut *b,
                "project_info",
                args_of(&[("detail", json!(true))]),
            )?);
            Ok(())
        }
        "frame" => {
            words.check(&["--at", "--width", "--overwrite"])?;
            let project = words.plain(0, tr("the project"))?;
            let out = words.plain(1, tr("the picture's file"))?;
            let mut b = backend(false, Some(project))?;
            let width = words
                .value("--width")?
                .map(|w| {
                    w.parse::<u64>()
                        .ok()
                        .filter(|w| *w > 0)
                        .ok_or_else(|| Failure::Usage(trf!("--width takes pixels: {}", w)))
                })
                .transpose()?;
            let args = args_of(&[
                ("path", json!(absolute(out))),
                (
                    "at",
                    words.seconds("--at")?.map_or(Value::Null, |s| json!(s)),
                ),
                ("width", width.map_or(Value::Null, |w| json!(w))),
                ("overwrite", json!(words.has("--overwrite"))),
            ]);
            print(&call(&mut *b, "export_frame", args)?);
            Ok(())
        }
        "render" => {
            words.check(&["--template", "--from", "--to", "--overwrite"])?;
            let project = words.plain(0, tr("the project"))?;
            let out = words.plain(1, tr("the file to render to"))?;
            let mut b = backend(false, Some(project))?;
            let args = args_of(&[
                ("path", json!(absolute(out))),
                (
                    "template",
                    words.value("--template")?.map_or(Value::Null, |t| json!(t)),
                ),
                (
                    "from",
                    words.seconds("--from")?.map_or(Value::Null, |s| json!(s)),
                ),
                (
                    "to",
                    words.seconds("--to")?.map_or(Value::Null, |s| json!(s)),
                ),
                ("overwrite", json!(words.has("--overwrite"))),
            ]);
            call(&mut *b, "render", args)?;
            let status = wait_for_render(&mut *b)?;
            print(&status);
            Ok(())
        }
        "call" => {
            words.check(&["--args", "--args-file", "--save", "--live"])?;
            let tool = words.plain(0, tr("the tool's name"))?;
            let live = words.has("--live");
            let project = words.plain.get(1).map(String::as_str);
            let args = match (words.value("--args")?, words.value("--args-file")?) {
                (Some(_), Some(_)) => {
                    return Err(Failure::Usage(
                        tr("Give --args or --args-file, not both.").into(),
                    ));
                }
                (Some(text), None) => parse_object(text)?,
                (None, Some(file)) => parse_object(
                    &std::fs::read_to_string(file)
                        .map_err(|e| Failure::Usage(trf!("{}: {}", file, e)))?,
                )?,
                (None, None) => Map::new(),
            };
            if !super::tools::exists(tool, live) {
                return Err(Failure::Usage(trf!("Unknown tool: {}", tool)));
            }
            let mut b = backend(live, project)?;
            let value = outcome(b.call(tool, &args))?;
            print(&value);
            finish(&mut *b, &words, project)
        }
        "run" => {
            words.check(&["--save", "--live"])?;
            let script = words.plain(0, tr("the script's file"))?;
            let text = std::fs::read_to_string(script)
                .map_err(|e| Failure::Usage(trf!("{}: {}", script, e)))?;
            let steps: Value = serde_json::from_str(&text)
                .map_err(|e| Failure::Usage(trf!("{}: {}", script, e)))?;
            let Value::Array(steps) = steps else {
                return Err(Failure::Usage(tr(
                    "A script is a JSON array of steps: [{\"tool\": \"…\", \"arguments\": {…}}, …]",
                )
                .into()));
            };
            let live = words.has("--live");
            let project = words.plain.get(1).map(String::as_str);
            let mut b = backend(live, project)?;
            let mut results = Vec::new();
            for (n, step) in steps.iter().enumerate() {
                let tool = step.get("tool").and_then(Value::as_str).unwrap_or_default();
                if !super::tools::exists(tool, live) {
                    return Err(Failure::Usage(trf!(
                        "Step {}: unknown tool \"{}\".",
                        n + 1,
                        tool
                    )));
                }
                let args = step
                    .get("arguments")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                match b.call(tool, &args) {
                    Ok(out) => results.push(out.value),
                    Err(e) => {
                        print(&Value::Array(results));
                        return Err(Failure::Tool(trf!("Step {} ({}): {}", n + 1, tool, e)));
                    }
                }
            }
            print(&Value::Array(results));
            finish(&mut *b, &words, project)
        }
        "mcp" => {
            words.check(&["--live"])?;
            serve_mcp(words.has("--live")).map_err(|e| Failure::Tool(e.to_string()))
        }
        other => Err(Failure::Usage(trf!("Unknown command: {}", other))),
    }
}

fn parse_object(text: &str) -> Result<Map<String, Value>, Failure> {
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(m)) => Ok(m),
        Ok(_) => Err(Failure::Usage(
            tr("The arguments are a JSON object: {\"name\": value, …}").into(),
        )),
        Err(e) => Err(Failure::Usage(trf!("The arguments are not JSON: {}", e))),
    }
}

/// A render followed until it ends, its progress on the standard error:
/// its last status.
fn wait_for_render(b: &mut dyn Backend) -> Result<Value, Failure> {
    let mut shown = None;
    loop {
        b.idle();
        // The command's own BananaV: its render is the last one.
        let status = call(b, "render_status", Map::new())?;
        let state = status
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let percent = status
            .get("progress")
            .and_then(Value::as_f64)
            .map_or(0, |p| (p * 100.0).floor() as i64);
        if shown != Some(percent) {
            eprint!("\r{}", trf!("Rendering… {} %", percent));
            shown = Some(percent);
        }
        match state {
            "done" => {
                eprintln!();
                return Ok(status);
            }
            "failed" | "cancelled" => {
                eprintln!();
                let why = status
                    .get("error")
                    .and_then(Value::as_str)
                    .map_or_else(|| tr("The render stopped.").to_string(), str::to_string);
                return Err(Failure::Tool(why));
            }
            _ => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

/// MCP on the standard input and output, the output kept for its
/// messages alone.
fn serve_mcp(live: bool) -> std::io::Result<()> {
    let mut out: Box<dyn Write> = match bv_platform::protocol_output() {
        Ok(f) => Box::new(f),
        Err(_) => Box::new(std::io::stdout()),
    };
    let input = Box::new(std::io::BufReader::new(std::io::stdin()));
    let mut b: Box<dyn Backend> = if live {
        Box::new(Live::new())
    } else {
        Box::new(Headless::new())
    };
    super::mcp::serve(&mut *b, input, &mut *out)
}

fn help() {
    let lines = [
        tr("BananaV's command line, for people and AI agents. Results come as JSON."),
        "",
        tr("Usage: bananav-cli <command> [arguments]"),
        "",
        tr("Commands:"),
        tr(
            "  info <project>                  What a project has: settings, tracks, events, media.",
        ),
        tr(
            "  render <project> <file>         Renders it. --template <name>, --from <s>, --to <s>, --overwrite.",
        ),
        tr(
            "  frame <project> <file.png>      A frame as a picture. --at <s>, --width <pixels>, --overwrite.",
        ),
        tr("  templates                       The render templates."),
        tr(
            "  effects                         The effects there are. --kind video|audio|transition|generator, --search <text>.",
        ),
        tr("  tools                           Every tool and its arguments (JSON Schema)."),
        tr(
            "  call <tool> [<project>]         Runs a tool. --args <JSON> or --args-file <file>, --save [<file>].",
        ),
        tr(
            "  run <script.json> [<project>]   Runs a JSON list of {\"tool\", \"arguments\"} in order. --save [<file>].",
        ),
        tr(
            "  mcp                             Serves the Model Context Protocol on the standard input and output.",
        ),
        tr("  help, version"),
        "",
        tr("--live: on the project open in BananaV's window (Options › BananaV Mods › AI agents)."),
        tr(
            "Times are in seconds. Exit codes: 0 done, 1 a tool failed, 2 the command was not understood.",
        ),
    ];
    // Asked for: on the standard output, as other programs' help.
    let mut out = std::io::stdout().lock();
    for l in lines {
        let _ = writeln!(out, "{l}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(a: &[&str]) -> Words {
        Words::of(&a.iter().map(|s| (*s).to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn options_and_words_are_told_apart() {
        let w = words(&[
            "p.veg",
            "--at",
            "2.5",
            "--overwrite",
            "out.png",
            "--width=640",
        ]);
        assert_eq!(w.plain, ["p.veg", "out.png"]);
        assert_eq!(w.seconds("--at").ok().flatten(), Some(2.5));
        assert!(w.has("--overwrite"));
        assert_eq!(w.value("--width").ok().flatten(), Some("640"));
        assert!(w.check(&["--at", "--overwrite", "--width"]).is_ok());
        assert!(w.check(&["--at"]).is_err());
        // --save takes a file only when one follows.
        let w = words(&["add_track", "--save", "--live"]);
        assert!(w.options.contains(&("--save".to_string(), None)));
        assert!(w.has("--live"));
        let w = words(&["add_track", "--save", "out.bvproj"]);
        assert_eq!(w.value("--save").ok().flatten(), Some("out.bvproj"));
        assert!(words(&["--at", "-1"]).seconds("--at").is_err());
    }

    #[test]
    fn the_command_line_is_told_from_a_file_to_open() {
        let a = |v: &[&str]| v.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        assert!(wanted(&a(&["--cli"])));
        assert!(wanted(&a(&["mcp"])));
        assert!(wanted(&a(&["render", "p.veg", "o.mp4"])));
        assert!(!wanted(&a(&["project.veg"])));
        assert!(!wanted(&a(&[])));
    }

    /// `render`: started, followed to its end, the file written.
    #[test]
    fn a_render_from_the_command_line_writes_its_file() {
        crate::i18n::set_language(crate::i18n::Language::English);
        let dir = std::env::temp_dir().join(format!("bv-cli-render-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let project = dir.join("demo.bvproj").to_string_lossy().into_owned();
        let file = dir.join("sound.wav");
        let _ = std::fs::remove_file(&file);
        let mut h = Headless::new();
        h.app.load_demo();
        h.pump();
        let saved = h.call(
            "save_project",
            &args_of(&[("path", json!(project)), ("overwrite", json!(true))]),
        );
        assert!(saved.is_ok(), "{:?}", saved.err());
        drop(h);
        let done = run(vec![
            "render".into(),
            project,
            file.to_string_lossy().into_owned(),
            "--to".into(),
            "1".into(),
        ]);
        assert!(done.is_ok(), "{done:?}");
        assert!(file.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
