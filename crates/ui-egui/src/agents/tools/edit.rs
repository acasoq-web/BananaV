//! Undo and redo, the whole model as JSON (read and patched), and the
//! keyboard map's commands.

use bv_timeline::Project;
use serde_json::{Value, json};

use super::{Args, Kind, Tool, arg};
use crate::agents::{Outcome, Output, patch};
use crate::app::BananaApp;
use crate::i18n::tr;

fn done(v: Value) -> Outcome {
    Ok(Output::value(v))
}

fn undo(app: &mut BananaApp, _: &Args<'_>) -> Outcome {
    if !app.history.can_undo() {
        return Err(tr("There is nothing to undo.").into());
    }
    let what = app.undo_label().map(|l| tr(l).to_string());
    app.undo();
    done(json!({"undone": what, "can_undo": app.history.can_undo()}))
}

fn redo(app: &mut BananaApp, _: &Args<'_>) -> Outcome {
    if !app.history.can_redo() {
        return Err(tr("There is nothing to redo.").into());
    }
    let what = app.redo_label().map(|l| tr(l).to_string());
    app.redo();
    done(json!({"redone": what, "can_redo": app.history.can_redo()}))
}

/// The project's model as JSON.
fn model(app: &BananaApp) -> Result<Value, String> {
    serde_json::from_str(&app.project().to_json()).map_err(|e| e.to_string())
}

fn project_json(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let doc = model(app)?;
    let pointer = a.opt_str("pointer")?.unwrap_or("");
    let part = patch::get(&doc, pointer).map_err(|e| trf!("The pointer finds nothing: {}", e))?;
    done(json!({"pointer": pointer, "value": part}))
}

fn patch_project(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let Some(ops) = a.get("patch") else {
        return Err(trf!("The argument \"{}\" is missing.", "patch"));
    };
    let mut doc = model(app)?;
    patch::apply(&mut doc, ops).map_err(|e| trf!("The patch did not apply: {}", e))?;
    let new = Project::from_json(&doc.to_string())
        .map_err(|e| trf!("The patched project is not one: {}", e))?;
    new.check_invariants()
        .map_err(|e| trf!("The patched project does not hold together: {}", e))?;
    let count = ops.as_array().map_or(0, Vec::len);
    app.agent_modify(|p| *p = new);
    app.prune_selection();
    done(json!({"applied": count, "undo": app.undo_label().map(|l| tr(l).to_string())}))
}

/// The keyboard map's commands an agent can run (none still to come), each
/// once.
fn commands() -> Vec<&'static crate::keymap::KeyCommand> {
    let mut seen = std::collections::BTreeSet::new();
    let reg = crate::keymap::registry();
    // The window's own first, as a key press in it finds them.
    reg.iter()
        .filter(|c| c.context == crate::keymap::Context::Global)
        .chain(reg.iter())
        .filter(|c| !matches!(c.action, crate::keymap::Action::Soon))
        .filter(|c| seen.insert(c.id.as_str()))
        .collect()
}

fn list_commands(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let search = a.opt_str("search")?.map(str::to_lowercase);
    let list: Vec<Value> = commands()
        .into_iter()
        .filter_map(|c| {
            let name = match c.action {
                crate::keymap::Action::Cmd(cmd) => cmd.label(app),
                _ => String::new(),
            };
            let keep = search
                .as_ref()
                .is_none_or(|q| c.id.to_lowercase().contains(q) || name.to_lowercase().contains(q));
            keep.then(|| {
                json!({
                    "id": c.id,
                    "name": name,
                    "keys": c.defaults.iter().map(|b| b.label()).collect::<Vec<_>>(),
                })
            })
        })
        .collect();
    done(json!({"commands": list}))
}

fn run_command(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let id = a.str("id")?;
    let c = commands()
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| trf!("There is no command {} (list_commands gives them).", id))?;
    match c.action {
        crate::keymap::Action::Cmd(cmd) => {
            if !cmd.enabled(app) {
                return Err(trf!("The command {} cannot run now.", id));
            }
            cmd.run(app);
        }
        crate::keymap::Action::Extra(e) => app.run_extra(e),
        crate::keymap::Action::Soon => {}
    }
    done(json!({"ran": id}))
}

pub(super) fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "undo",
            title: "Undo",
            description: "Undoes the last edit (the agent's or the user's).",
            properties: || json!({}),
            required: &[],
            kind: Kind::Edit,
            run: undo,
        },
        Tool {
            name: "redo",
            title: "Redo",
            description: "Redoes the last edit undone.",
            properties: || json!({}),
            required: &[],
            kind: Kind::Edit,
            run: redo,
        },
        Tool {
            name: "project_json",
            title: "Read the project's model",
            description: "The project's whole model as JSON, or the part a JSON Pointer (RFC 6901) names: \"/settings\", \"/tracks/0\", \"/tracks/0/clips/2/pan_crop\", \"/media/1\"… Times in it are ticks: 254016000000 a second.",
            properties: || json!({"pointer": arg("string", "A JSON Pointer (\"\" or nothing: all of it).")}),
            required: &[],
            kind: Kind::Read,
            run: project_json,
        },
        Tool {
            name: "patch_project",
            title: "Patch the project's model",
            description: "Changes the project's model with a JSON Patch (RFC 6902: add, remove, replace, move, copy, test), for what no other tool does: every setting of tracks, events, Pan/Crop, Track Motion, masks, busses… All of it applies as one undo step, or none of it when the project it makes does not hold together (ids unique and below next_id, events in order and inside their media…). Times are ticks: 254016000000 a second.",
            properties: || json!({"patch": {"type": "array", "items": {"type": "object"}, "description": "The operations, in order."}}),
            required: &["patch"],
            kind: Kind::Edit,
            run: patch_project,
        },
        Tool {
            name: "list_commands",
            title: "List the window's commands",
            description: "The commands of the window's menus and keyboard map (as Options › Customize Keyboard lists them): id, name, default keys. run_command runs them in the open window.",
            properties: || json!({"search": arg("string", "Only those whose id or name has this text.")}),
            required: &[],
            kind: Kind::Read,
            run: list_commands,
        },
        Tool {
            name: "run_command",
            title: "Run a window's command",
            description: "Runs a command of the open window's menus (an id from list_commands, e.g. \"Edit.Undo\", \"View.Window.Mixer\"), as its menu item or key does: on the selection and the cursor. Commands that ask for a file open the window's dialog for the user.",
            properties: || json!({"id": arg("string", "The command's id.")}),
            required: &["id"],
            kind: Kind::Window,
            run: run_command,
        },
    ]
}
