//! AI agents and the command line: BananaV's editing for programs.
//!
//! The same tools ([`tools`]: open and save projects, import media, tracks,
//! events, effects and their parameters, transitions, markers, envelopes,
//! the model as JSON, a frame as a picture, renders, the keyboard map's
//! commands) run on a [`BananaApp`] two ways:
//!
//! - **headless** ([`Headless`]): a BananaV of their own without a window,
//!   for the command line (`bananav-cli call`, `render`, `info`…, [`cli`])
//!   and for an agent's MCP client that starts `bananav mcp` ([`mcp`]);
//! - **live** ([`link`]): the open window's project, when Options ›
//!   BananaV Mods allows agents — `bananav mcp --live` and `bananav-cli
//!   call --live` pass the calls to it over the computer's own network
//!   (127.0.0.1, with a key only this user can read), and the window runs
//!   them between its frames, saving, rendering or opening another
//!   project only with the user's yes.
//!
//! Each tool that edits is one undo step. Tool names, their arguments and
//! their descriptions are the protocol's (English, like the keyboard map's
//! command ids); what a person reads — the command line's help, progress
//! and errors — is translated.

pub(crate) mod cli;
pub(crate) mod link;
pub(crate) mod mcp;
mod patch;
#[cfg(test)]
mod tests;
pub(crate) mod tools;

use serde_json::{Map, Value};

use crate::app::BananaApp;
use crate::egui;

/// What a tool gives back.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Output {
    /// For the agent (as JSON) and the command line.
    pub value: Value,
    /// A picture it shows (PNG): MCP's image content.
    pub image: Option<Vec<u8>>,
}

impl Output {
    pub fn value(value: Value) -> Output {
        Output { value, image: None }
    }
}

/// A tool's result, or what went wrong (for a person: translated).
pub(crate) type Outcome = Result<Output, String>;

/// Where tools run: a BananaV of their own or the open window.
pub(crate) trait Backend {
    /// Runs tool `name` with `args`.
    fn call(&mut self, name: &str, args: &Map<String, Value>) -> Outcome;
    /// Whether the open window's project is the one edited.
    fn live(&self) -> bool;
    /// Time given between calls (a render going on gets on with it).
    fn idle(&mut self) {}
}

/// A BananaV without a window: its interface runs frames nobody sees, so
/// everything works as in the program (jobs finish, effects load), and
/// nothing of the user's is touched — no settings saved, no session, no
/// autosave, no update check.
#[derive(Debug)]
pub(crate) struct Headless {
    ctx: egui::Context,
    app: Box<BananaApp>,
}

impl Headless {
    pub fn new() -> Headless {
        let ctx = egui::Context::default();
        let mut app = Box::new(BananaApp::new(ctx.clone()));
        app.agent_headless();
        let mut h = Headless { ctx, app };
        h.pump();
        h
    }

    /// One frame of the interface, unseen.
    pub fn pump(&mut self) {
        // Not focused: the keyboard map never reads the computer's keys.
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1600.0, 960.0),
            )),
            focused: false,
            ..Default::default()
        };
        let app = &mut self.app;
        let mut out = self.ctx.run_ui(raw, |ui| app.frame(ui));
        out.textures_delta.clear();
    }
}

impl Backend for Headless {
    fn call(&mut self, name: &str, args: &Map<String, Value>) -> Outcome {
        let out = tools::call(&mut self.app, name, args, false);
        self.pump();
        out
    }

    fn live(&self) -> bool {
        false
    }

    fn idle(&mut self) {
        if self.app.agent_busy() {
            self.pump();
        }
    }
}

/// Base64 (RFC 4648, with padding): MCP's pictures.
pub(crate) fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for k in 0..4 {
            if k <= chunk.len() {
                out.push(char::from(ABC[((n >> (18 - 6 * k)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Base64 read back (the live window's pictures).
pub(crate) fn unbase64(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let bytes = text.trim_end_matches('=').as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut n = 0u32;
        for (k, &c) in chunk.iter().enumerate() {
            n |= value(c)? << (18 - 6 * k);
        }
        let len = chunk.len().checked_sub(1)?;
        for k in 0..len {
            out.push((n >> (16 - 8 * k)) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod base64_tests {
    #[test]
    fn base64_goes_there_and_back() {
        for (raw, coded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(super::base64(raw.as_bytes()), coded);
            assert_eq!(super::unbase64(coded).expect("decodes"), raw.as_bytes());
        }
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(super::unbase64(&super::base64(&all)).expect("decodes"), all);
        assert!(super::unbase64("Zm9v!").is_none());
    }
}
