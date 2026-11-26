//! The Model Context Protocol: an agent's program (its client) starts
//! `bananav mcp` and they talk JSON-RPC 2.0 over its standard input and
//! output, a message a line. BananaV answers the handshake, lists its
//! tools and runs them on a [`Backend`]: a BananaV of its own without a
//! window, or the open window's (`--live`). What it writes goes nowhere
//! else: the program's other output is kept off the line
//! (`bv_platform::protocol_output`).

use std::io::{BufRead, Write};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use serde_json::{Map, Value, json};

use super::{Backend, Outcome};

/// The protocol's revisions BananaV speaks, the newest first: a client's
/// own when it is one of them, else the newest.
const VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// JSON-RPC's codes.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// What an agent is told at the handshake: how BananaV's tools work.
const INSTRUCTIONS: &str = "BananaV is a video editor. Its tools edit one project: \
the open window's (live) or one of this server's own (headless: open_project or \
new_project first, save_project at the end). Times are seconds from the project's \
start (decimals allowed), snapped to frames where an edit needs it; tracks are \
numbered from 0 at the top; events, media and markers have ids the list tools give. \
Each tool that edits is one undo step (undo/redo). Effects are found with \
list_available_effects and their parameters with get_effect; a parameter set with \
`at` makes a keyframe there. project_json and patch_project read and edit the whole \
project model (RFC 6901 pointers, RFC 6902 patches) for anything the other tools do \
not cover. preview_frame shows a frame as a picture. render starts a render \
(render_status follows it). In the live window, saving, rendering and opening wait \
for the user's yes.";

/// A server's state between messages.
#[derive(Debug, Default)]
struct Session {
    /// The revision agreed at the handshake.
    version: Option<&'static str>,
}

/// Serves the protocol on `input` and `out` until the client closes its
/// end. Between messages, while the backend has work going on (a render),
/// it is given time every tenth of a second.
pub(crate) fn serve(
    backend: &mut dyn Backend,
    input: Box<dyn BufRead + Send>,
    out: &mut dyn Write,
) -> std::io::Result<()> {
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::Builder::new()
        .name("mcp-input".into())
        .spawn(move || {
            for line in input.lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        })?;
    let mut session = Session::default();
    loop {
        let line = match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => line,
            Err(RecvTimeoutError::Timeout) => {
                backend.idle();
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => break,
        };
        if let Some(reply) = answer(backend, &mut session, line.trim()) {
            out.write_all(reply.as_bytes())?;
            out.write_all(b"\n")?;
            out.flush()?;
        }
    }
    Ok(())
}

/// The answer to a line: a message or a batch of them (`None` for
/// notifications only).
fn answer(backend: &mut dyn Backend, session: &mut Session, line: &str) -> Option<String> {
    if line.is_empty() {
        return None;
    }
    let message: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return Some(error(Value::Null, PARSE_ERROR, &e.to_string()).to_string()),
    };
    match message {
        Value::Array(batch) => {
            if batch.is_empty() {
                return Some(error(Value::Null, INVALID_REQUEST, "empty batch").to_string());
            }
            let replies: Vec<Value> = batch
                .into_iter()
                .filter_map(|m| one(backend, session, m))
                .collect();
            (!replies.is_empty()).then(|| Value::Array(replies).to_string())
        }
        m => one(backend, session, m).map(|r| r.to_string()),
    }
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn result(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

/// One message's answer: none for a notification or a response.
fn one(backend: &mut dyn Backend, session: &mut Session, message: Value) -> Option<Value> {
    let Value::Object(m) = message else {
        return Some(error(Value::Null, INVALID_REQUEST, "not an object"));
    };
    let id = m.get("id").cloned();
    let Some(method) = m.get("method").and_then(Value::as_str) else {
        // A response to something BananaV never asks: nothing to do.
        return match id {
            Some(_) if m.contains_key("result") || m.contains_key("error") => None,
            id => Some(error(
                id.unwrap_or(Value::Null),
                INVALID_REQUEST,
                "no method",
            )),
        };
    };
    let params = m.get("params").cloned().unwrap_or(Value::Null);
    // Notifications (initialized, cancelled…): BananaV needs none.
    let id = id?;
    Some(match method {
        "initialize" => {
            let asked = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let version = VERSIONS
                .iter()
                .copied()
                .find(|v| *v == asked)
                .unwrap_or(VERSIONS[0]);
            session.version = Some(version);
            result(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {
                        "name": "bananav",
                        "title": "BananaV",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                    "instructions": INSTRUCTIONS,
                }),
            )
        }
        "ping" => result(id, json!({})),
        "tools/list" => result(id, json!({"tools": super::tools::list(backend.live())})),
        "tools/call" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(error(id, INVALID_PARAMS, "no tool name"));
            };
            if !super::tools::exists(name, backend.live()) {
                return Some(error(id, INVALID_PARAMS, &format!("unknown tool: {name}")));
            }
            let args = match params.get("arguments") {
                None | Some(Value::Null) => Map::new(),
                Some(Value::Object(a)) => a.clone(),
                Some(_) => return Some(error(id, INVALID_PARAMS, "arguments must be an object")),
            };
            result(id, call_result(backend.call(name, &args)))
        }
        // Nothing of these is offered, but clients may ask.
        "resources/list" => result(id, json!({"resources": []})),
        "resources/templates/list" => result(id, json!({"resourceTemplates": []})),
        "prompts/list" => result(id, json!({"prompts": []})),
        other => error(id, METHOD_NOT_FOUND, &format!("unknown method: {other}")),
    })
}

/// A tool's outcome as the protocol's call result: its value as text (and
/// as structured content when it is an object), its picture, or its error.
pub(crate) fn call_result(outcome: Outcome) -> Value {
    match outcome {
        Ok(out) => {
            let text = serde_json::to_string_pretty(&out.value).unwrap_or_default();
            let mut content = vec![json!({"type": "text", "text": text})];
            if let Some(png) = &out.image {
                content.push(json!({
                    "type": "image",
                    "data": super::base64(png),
                    "mimeType": "image/png",
                }));
            }
            let mut r = json!({"content": content, "isError": false});
            if out.value.is_object() {
                r["structuredContent"] = out.value;
            }
            r
        }
        Err(message) => json!({
            "content": [{"type": "text", "text": message}],
            "isError": true,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::Output;

    /// A backend answering every tool with its name and arguments.
    #[derive(Debug, Default)]
    struct Echo {
        calls: usize,
    }

    impl Backend for Echo {
        fn call(&mut self, name: &str, args: &Map<String, Value>) -> Outcome {
            self.calls += 1;
            if name == "undo" {
                return Err("nothing to undo".into());
            }
            Ok(Output::value(json!({"tool": name, "args": args})))
        }

        fn live(&self) -> bool {
            false
        }
    }

    fn talk(lines: &[&str]) -> (Vec<Value>, Echo) {
        let mut echo = Echo::default();
        let input = Box::new(std::io::Cursor::new(lines.join("\n").into_bytes()));
        let mut out = Vec::new();
        serve(&mut echo, input, &mut out).expect("served");
        let replies = String::from_utf8(out)
            .expect("text")
            .lines()
            .map(|l| serde_json::from_str(l).expect("a JSON line"))
            .collect();
        (replies, echo)
    }

    /// The handshake agrees on the client's revision (else the newest),
    /// notifications get no answer, the tools are listed and run, and a
    /// tool's failure is a result marked as an error, not a protocol one.
    #[test]
    fn a_client_talks_to_bananav() {
        let (replies, echo) = talk(&[
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"project_info","arguments":{}}}"#,
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"undo"}}"#,
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"no_such_tool"}}"#,
            r#"{"jsonrpc":"2.0","id":6,"method":"bogus"}"#,
            r#"not json"#,
            r#"{"jsonrpc":"2.0","id":7,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}"#,
        ]);
        assert_eq!(replies.len(), 8, "{replies:?}");
        assert_eq!(replies[0]["id"], 1);
        assert_eq!(replies[0]["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(replies[0]["result"]["serverInfo"]["name"], "bananav");
        assert!(replies[0]["result"]["capabilities"]["tools"].is_object());
        let tools = replies[1]["result"]["tools"].as_array().expect("tools");
        assert!(tools.iter().any(|t| t["name"] == "project_info"));
        assert!(tools.iter().all(|t| t["inputSchema"]["type"] == "object"));
        assert_eq!(replies[2]["result"]["isError"], false);
        assert_eq!(
            replies[2]["result"]["structuredContent"]["tool"],
            "project_info"
        );
        assert_eq!(replies[3]["result"]["isError"], true);
        assert_eq!(
            replies[3]["result"]["content"][0]["text"],
            "nothing to undo"
        );
        assert_eq!(replies[4]["error"]["code"], INVALID_PARAMS);
        assert_eq!(replies[5]["error"]["code"], METHOD_NOT_FOUND);
        assert_eq!(replies[6]["error"]["code"], PARSE_ERROR);
        assert_eq!(replies[6]["id"], Value::Null);
        assert_eq!(replies[7]["result"]["protocolVersion"], VERSIONS[0]);
        assert_eq!(echo.calls, 2);
    }

    /// A batch gets a batch back, without the notifications' answers.
    #[test]
    fn a_batch_is_answered_as_one() {
        let (replies, _) = talk(&[
            r#"[{"jsonrpc":"2.0","id":"a","method":"ping"},{"jsonrpc":"2.0","method":"notifications/initialized"},{"jsonrpc":"2.0","id":"b","method":"ping"}]"#,
        ]);
        let batch = replies[0].as_array().expect("a batch");
        assert_eq!(batch.len(), 2);
        assert_eq!(batch[0]["id"], "a");
        assert_eq!(batch[1]["id"], "b");
    }
}
