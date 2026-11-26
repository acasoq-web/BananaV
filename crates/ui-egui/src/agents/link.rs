//! The open window's line to agents (Options › BananaV Mods › AI agents):
//! a listener on the computer's own network (127.0.0.1, on a port the
//! system picks) whose port and key go into `agents.json` in the user's
//! settings folder (only that user reads it). A client gives the key first,
//! then a call a line — `{"id", "tool", "arguments"}` — each answered by a
//! line: `{"id", "ok": true, "value", "image"}` or `{"id", "ok": false,
//! "error"}`. The calls wait in a queue the window takes between its frames
//! ([`Host::take`]), so they run where the user's own edits do.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use serde_json::{Map, Value, json};

use super::{Backend, Outcome, Output};

/// A call from an agent, and where its outcome goes.
#[derive(Debug)]
pub(crate) struct Call {
    pub tool: String,
    pub args: Map<String, Value>,
    pub reply: mpsc::Sender<Outcome>,
}

/// The file telling clients where the window listens: beside the user's
/// settings (the tests' in a folder of their own).
fn address_file() -> Option<PathBuf> {
    if cfg!(test) {
        return Some(
            std::env::temp_dir().join(format!("bv-agents-test-{}.json", std::process::id())),
        );
    }
    Some(
        bv_platform::config_dir()?
            .join("BananaV")
            .join("agents.json"),
    )
}

/// A key no one else can guess: the system's random keys of four hashers.
fn new_key() -> String {
    use std::hash::{BuildHasher, Hasher};
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    (0..4u64)
        .map(|i| {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u64(i);
            h.write_u128(now);
            h.write_u32(std::process::id());
            format!("{:016x}", h.finish())
        })
        .collect()
}

/// Two keys alike, compared all the way (no early end to time).
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

/// The window's listener while agents are allowed.
#[derive(Debug)]
pub(crate) struct Host {
    calls: mpsc::Receiver<Call>,
    stop: Arc<AtomicBool>,
    clients: Arc<AtomicUsize>,
    port: u16,
    file: Option<PathBuf>,
}

impl Host {
    /// Listens, and writes where; `wake` makes the window take a call.
    pub fn start(wake: impl Fn() + Send + Sync + 'static) -> std::io::Result<Host> {
        Host::start_at(wake, address_file())
    }

    /// Listens, writing where into `file`.
    pub(crate) fn start_at(
        wake: impl Fn() + Send + Sync + 'static,
        file: Option<PathBuf>,
    ) -> std::io::Result<Host> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let port = listener.local_addr()?.port();
        let key = Arc::new(new_key());
        if let Some(f) = &file {
            let text = json!({
                "port": port,
                "key": key.as_str(),
                "pid": std::process::id(),
                "version": env!("CARGO_PKG_VERSION"),
            })
            .to_string();
            write_private(f, &text)?;
        }
        let (tx, calls) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let clients = Arc::new(AtomicUsize::new(0));
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
        let (s, c) = (Arc::clone(&stop), Arc::clone(&clients));
        std::thread::Builder::new()
            .name("agents-listener".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    if s.load(Ordering::Relaxed) {
                        break;
                    }
                    let Ok(stream) = stream else { continue };
                    let (key, tx, wake, s, c) = (
                        Arc::clone(&key),
                        tx.clone(),
                        Arc::clone(&wake),
                        Arc::clone(&s),
                        Arc::clone(&c),
                    );
                    let _ = std::thread::Builder::new()
                        .name("agents-client".into())
                        .spawn(move || {
                            c.fetch_add(1, Ordering::Relaxed);
                            serve_client(stream, &key, &tx, &*wake, &s);
                            c.fetch_sub(1, Ordering::Relaxed);
                            wake();
                        });
                }
            })?;
        Ok(Host {
            calls,
            stop,
            clients,
            port,
            file,
        })
    }

    /// The next call waiting.
    pub fn take(&self) -> Option<Call> {
        self.calls.try_recv().ok()
    }

    /// How many clients are connected now.
    pub fn clients(&self) -> usize {
        self.clients.load(Ordering::Relaxed)
    }

    #[cfg(test)]
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // The listener waits on a connection: one, to end it.
        let _ = TcpStream::connect_timeout(
            &SocketAddr::from((Ipv4Addr::LOCALHOST, self.port)),
            Duration::from_millis(200),
        );
        // The file, unless another window's is there now.
        if let Some(f) = &self.file
            && read_address(f).is_some_and(|(port, _)| port == self.port)
        {
            let _ = std::fs::remove_file(f);
        }
    }
}

/// `text` into `path`, readable by this user alone.
fn write_private(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(text.as_bytes())
}

fn read_address(path: &std::path::Path) -> Option<(u16, String)> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    let port = u16::try_from(v.get("port")?.as_u64()?).ok()?;
    Some((port, v.get("key")?.as_str()?.to_string()))
}

/// One client: its key, then its calls one by one, each waiting for the
/// window's outcome.
fn serve_client(
    stream: TcpStream,
    key: &str,
    calls: &mpsc::Sender<Call>,
    wake: &(dyn Fn() + Send + Sync),
    stop: &AtomicBool,
) {
    let Ok(mut out) = stream.try_clone() else {
        return;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let mut input = BufReader::new(stream);
    let mut line = String::new();
    if input.read_line(&mut line).is_err() {
        return;
    }
    let given = serde_json::from_str::<Value>(line.trim())
        .ok()
        .and_then(|v| v.get("key").and_then(Value::as_str).map(str::to_string));
    if !given.is_some_and(|k| same(&k, key)) {
        let _ = writeln!(out, "{}", json!({"ok": false, "error": "wrong key"}));
        return;
    }
    let _ = writeln!(out, "{}", json!({"ok": true}));
    let _ = input.get_ref().set_read_timeout(None);
    loop {
        line.clear();
        match input.read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let Ok(request) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let tool = request
            .get("tool")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let args = request
            .get("arguments")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let (reply, answer) = mpsc::channel();
        let outcome = if calls.send(Call { tool, args, reply }).is_ok() {
            wake();
            answer
                .recv()
                .unwrap_or_else(|_| Err(tr("BananaV closed before answering.").into()))
        } else {
            Err(tr("BananaV closed before answering.").into())
        };
        let message = match outcome {
            Ok(o) => json!({
                "id": id,
                "ok": true,
                "value": o.value,
                "image": o.image.as_deref().map(super::base64),
            }),
            Err(e) => json!({"id": id, "ok": false, "error": e}),
        };
        if writeln!(out, "{message}").is_err() {
            return;
        }
    }
}

use crate::i18n::tr;

/// The client's side: calls passed to the open window.
#[derive(Debug, Default)]
pub(crate) struct Live {
    line: Option<(BufReader<TcpStream>, TcpStream)>,
    next: u64,
    /// Where the window's address is (tests: their own).
    file: Option<PathBuf>,
}

impl Live {
    pub fn new() -> Live {
        Live {
            file: address_file(),
            ..Live::default()
        }
    }

    #[cfg(test)]
    pub fn at(file: PathBuf) -> Live {
        Live {
            file: Some(file),
            ..Live::default()
        }
    }

    fn not_open() -> String {
        tr("BananaV is not open, or does not allow AI agents: turn on Options › BananaV Mods › AI agents in its window.")
            .into()
    }

    fn connect(&mut self) -> Result<(), String> {
        let (port, key) = self
            .file
            .as_deref()
            .and_then(read_address)
            .ok_or_else(Self::not_open)?;
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        let stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))
            .map_err(|_| Self::not_open())?;
        let mut out = stream.try_clone().map_err(|e| e.to_string())?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
        let mut input = BufReader::new(stream);
        writeln!(out, "{}", json!({"key": key})).map_err(|_| Self::not_open())?;
        let mut line = String::new();
        input.read_line(&mut line).map_err(|_| Self::not_open())?;
        let welcome: Value = serde_json::from_str(line.trim()).map_err(|_| Self::not_open())?;
        if welcome.get("ok") != Some(&Value::Bool(true)) {
            return Err(Self::not_open());
        }
        // A call may wait for the user's yes for long.
        let _ = input.get_ref().set_read_timeout(None);
        self.line = Some((input, out));
        Ok(())
    }

    fn exchange(&mut self, tool: &str, args: &Map<String, Value>) -> std::io::Result<Outcome> {
        let (input, out) = self.line.as_mut().ok_or(std::io::ErrorKind::NotConnected)?;
        self.next += 1;
        let request = json!({"id": self.next, "tool": tool, "arguments": args});
        writeln!(out, "{request}")?;
        out.flush()?;
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        let answer: Value = serde_json::from_str(line.trim())?;
        Ok(if answer.get("ok") == Some(&Value::Bool(true)) {
            Ok(Output {
                value: answer.get("value").cloned().unwrap_or(Value::Null),
                image: answer
                    .get("image")
                    .and_then(Value::as_str)
                    .and_then(super::unbase64),
            })
        } else {
            Err(answer
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string())
        })
    }
}

impl Backend for Live {
    fn call(&mut self, name: &str, args: &Map<String, Value>) -> Outcome {
        // Once more on a fresh line when the old one broke (the window
        // closed and opened again, or agents turned off and on).
        for _ in 0..2 {
            if self.line.is_none() {
                self.connect()?;
            }
            match self.exchange(name, args) {
                Ok(outcome) => return outcome,
                Err(_) => self.line = None,
            }
        }
        Err(Self::not_open())
    }

    fn live(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A client with the key gets its calls to the window's queue and the
    /// outcomes back; one without it gets nothing through; the address
    /// goes when the window stops listening.
    #[test]
    fn calls_go_to_the_window_and_back() {
        let dir = std::env::temp_dir().join(format!("bv-agents-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("agents.json");
        let host = Host::start_at(|| {}, Some(file.clone())).expect("listens");
        let wrong = dir.join("wrong.json");
        write_private(
            &wrong,
            &json!({"port": host.port(), "key": "0"}).to_string(),
        )
        .expect("written");
        assert!(Live::at(wrong).call("project_info", &Map::new()).is_err());
        let worker = std::thread::spawn(move || {
            let call = loop {
                if let Some(c) = host.take() {
                    break c;
                }
                std::thread::sleep(Duration::from_millis(5));
            };
            assert_eq!(call.tool, "add_marker");
            assert_eq!(call.args["name"], "Here");
            let _ = call.reply.send(Ok(Output {
                value: json!({"id": 7}),
                image: Some(vec![1, 2, 3]),
            }));
            host
        });
        let mut client = Live::at(file.clone());
        let mut args = Map::new();
        args.insert("name".into(), json!("Here"));
        let out = client.call("add_marker", &args).expect("an outcome");
        assert_eq!(out.value["id"], 7);
        assert_eq!(out.image.as_deref(), Some(&[1u8, 2, 3][..]));
        let host = worker.join().expect("the window");
        assert!(host.clients() >= 1, "the client still connected");
        drop(host);
        assert!(!file.exists(), "the address goes with the window");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
