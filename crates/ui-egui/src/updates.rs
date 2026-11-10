//! Updates. BananaV asks GitHub for its latest public release as it starts
//! (Options › Preferences › General: "Check for updates when BananaV
//! starts") and when Help › Check for Software Updates… is chosen. A newer
//! one opens a window: Update, Cancel, Don't Show Again (no more checks as
//! BananaV starts). Update, on Windows with BananaV installed by its
//! installer, downloads the release's installer, checks its SHA-256
//! against the one GitHub gives, and runs it as BananaV closes; elsewhere
//! (macOS, Linux, a copy run from a folder of its own) it opens the
//! release's page. Requests go through the system's `curl` (Windows 10 and
//! later, macOS and Linux have it): BananaV carries no network library.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Deserialize;

use crate::app::BananaApp;
use crate::egui::{self, RichText, vec2};
use crate::i18n::tr;

/// The public repository's latest release (drafts and pre-releases left
/// out by GitHub).
const LATEST: &str = "https://api.github.com/repos/acasoq-web/BananaV/releases/latest";
/// What a release's pages and files must start with.
const RELEASES: &str = "https://github.com/acasoq-web/BananaV/releases/";

/// A release's version: its numbers and its pre-release part ("alpha.1").
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Version {
    numbers: [u64; 3],
    pre: Option<String>,
}

impl Version {
    /// "0.3.0", "v0.3.0", "0.1.0-ultra-alpha.2".
    pub(crate) fn parse(s: &str) -> Option<Version> {
        let s = s.trim().trim_start_matches(['v', 'V']);
        let (core, pre) = match s.split_once('-') {
            Some((c, p)) => (c, Some(p.to_string())),
            None => (s, None),
        };
        let mut numbers = [0u64; 3];
        let parts: Vec<&str> = core.split('.').collect();
        if parts.is_empty() || parts.len() > 3 {
            return None;
        }
        for (n, p) in numbers.iter_mut().zip(&parts) {
            *n = p.parse().ok()?;
        }
        Some(Version { numbers, pre })
    }

    /// This program's.
    pub(crate) fn current() -> Version {
        Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or(Version {
            numbers: [0, 0, 0],
            pre: None,
        })
    }

    /// Whether it comes after `other` (a release after its pre-releases;
    /// pre-releases of the same numbers by their text).
    pub(crate) fn newer_than(&self, other: &Version) -> bool {
        match self.numbers.cmp(&other.numbers) {
            std::cmp::Ordering::Greater => true,
            std::cmp::Ordering::Less => false,
            std::cmp::Ordering::Equal => match (&self.pre, &other.pre) {
                (None, Some(_)) => true,
                (Some(a), Some(b)) => a > b,
                _ => false,
            },
        }
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let [a, b, c] = self.numbers;
        write!(f, "{a}.{b}.{c}")?;
        if let Some(p) = &self.pre {
            write!(f, "-{p}")?;
        }
        Ok(())
    }
}

/// A release BananaV can update to.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Release {
    pub version: Version,
    /// Its page on GitHub.
    pub page: String,
    /// This system's installer, when there is one to run.
    pub setup: Option<Asset>,
}

/// A file of a release.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// Its SHA-256, as GitHub gives it.
    pub sha256: Option<[u8; 32]>,
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    size: u64,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

/// The end of this system's installer's name ("-windows-x64-setup.exe");
/// none where BananaV has no installer of its own to run.
fn setup_suffix() -> Option<&'static str> {
    if !cfg!(windows) {
        return None;
    }
    Some(if cfg!(target_arch = "aarch64") {
        "-windows-arm64-setup.exe"
    } else {
        "-windows-x64-setup.exe"
    })
}

/// A digest's text ("sha256:…") as bytes.
fn parse_sha256(digest: &str) -> Option<[u8; 32]> {
    let hex = digest.strip_prefix("sha256:")?;
    if hex.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    Some(out)
}

/// The release GitHub's answer `json` describes, with this system's
/// installer (`suffix`) when it has one; none for drafts, pre-releases and
/// pages or files outside BananaV's releases.
fn parse_release(json: &str, suffix: Option<&str>) -> Result<Release, String> {
    let r: ApiRelease =
        serde_json::from_str(json).map_err(|e| format!("an answer not understood ({e})"))?;
    if r.draft || r.prerelease {
        return Err("no release".into());
    }
    let version = Version::parse(&r.tag_name)
        .ok_or_else(|| format!("a version not understood ({})", r.tag_name))?;
    if !r.html_url.starts_with(RELEASES) {
        return Err("a page outside BananaV's releases".into());
    }
    let setup = suffix.and_then(|suffix| {
        r.assets
            .iter()
            .find(|a| a.name.ends_with(suffix) && a.browser_download_url.starts_with(RELEASES))
            .map(|a| Asset {
                name: a.name.clone(),
                url: a.browser_download_url.clone(),
                size: a.size,
                sha256: a.digest.as_deref().and_then(parse_sha256),
            })
    });
    Ok(Release {
        version,
        page: r.html_url,
        setup,
    })
}

/// The system's `curl`, its console window hidden on Windows.
fn curl() -> std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        let mut c =
            std::process::Command::new(PathBuf::from(root).join("System32").join("curl.exe"));
        const NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(NO_WINDOW);
        c
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("/usr/bin/curl")
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        std::process::Command::new("curl")
    }
}

/// Asks GitHub for the latest release.
fn fetch_latest() -> Result<Release, String> {
    let agent = format!("BananaV/{}", env!("CARGO_PKG_VERSION"));
    let out = curl()
        .args(["-fsSL", "--max-time", "20", "-A", &agent])
        .args(["-H", "Accept: application/vnd.github+json", LATEST])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("curl could not run ({e})"))?;
    if !out.status.success() {
        return Err(format!(
            "no answer from GitHub (curl {})",
            out.status.code().unwrap_or(-1)
        ));
    }
    parse_release(&String::from_utf8_lossy(&out.stdout), setup_suffix())
}

/// Whether BananaV was installed by its installer (its uninstaller lies
/// beside it): its updates can be installed the same way.
fn installed() -> bool {
    cfg!(windows)
        && std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("unins000.exe")))
            .is_some_and(|u| u.is_file())
}

/// SHA-256 (FIPS 180-4).
#[allow(clippy::needless_range_loop)]
pub(crate) fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let (whole, rest) = data.as_chunks::<64>();
    let mut tail = rest.to_vec();
    tail.push(0x80);
    while tail.len() % 64 != 56 {
        tail.push(0);
    }
    tail.extend_from_slice(&((data.len() as u64).wrapping_mul(8)).to_be_bytes());
    let blocks = whole.iter().chain(tail.as_chunks::<64>().0);
    for block in blocks {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[4 * i],
                block[4 * i + 1],
                block[4 * i + 2],
                block[4 * i + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (x, v) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *x = x.wrapping_add(v);
        }
    }
    let mut out = [0u8; 32];
    for (o, v) in out.as_chunks_mut::<4>().0.iter_mut().zip(h) {
        *o = v.to_be_bytes();
    }
    out
}

/// Whether the file at `path` is `asset` (its size, and its SHA-256 when
/// GitHub gives one).
fn matches(path: &Path, asset: &Asset) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    bytes.len() as u64 == asset.size && asset.sha256.is_none_or(|s| sha256(&bytes) == s)
}

/// Downloads `asset` to `dest`, the bytes so far in `done`, stopping when
/// `cancel` is set; checked once in.
fn download(
    asset: &Asset,
    dest: &Path,
    done: &AtomicU64,
    cancel: &AtomicBool,
) -> Result<(), String> {
    if matches(dest, asset) {
        done.store(asset.size, Ordering::Relaxed);
        return Ok(());
    }
    let part = dest.with_extension("part");
    let _ = std::fs::remove_file(&part);
    let mut child = curl()
        .args(["-fsSL", "--max-time", "3600", "-o"])
        .arg(&part)
        .arg(&asset.url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("curl could not run ({e})"))?;
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(&part);
            return Err("cancelled".into());
        }
        if let Ok(m) = std::fs::metadata(&part) {
            done.store(m.len(), Ordering::Relaxed);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    };
    if !status.success() {
        let _ = std::fs::remove_file(&part);
        return Err(format!(
            "the download failed (curl {})",
            status.code().unwrap_or(-1)
        ));
    }
    if !matches(&part, asset) {
        let _ = std::fs::remove_file(&part);
        return Err("the file downloaded is damaged (its checksum differs from GitHub's)".into());
    }
    std::fs::rename(&part, dest).map_err(|e| e.to_string())
}

/// What a background job leaves for the window.
type Slot<T> = Arc<Mutex<Option<T>>>;

/// Where the window is.
#[derive(Debug)]
enum State {
    /// Asking GitHub.
    Checking(Slot<Result<Release, String>>),
    Available(Release),
    UpToDate,
    Failed(String),
    Downloading {
        release: Release,
        done: Arc<AtomicU64>,
        cancel: Arc<AtomicBool>,
        result: Slot<Result<PathBuf, String>>,
    },
    /// Downloaded: its installer runs as BananaV closes.
    Ready(Release),
}

/// What a button of the window asks for.
enum Act {
    Update(Release),
    DontShowAgain,
    Exit,
}

/// The updates' window (none shows while BananaV checks as it starts, nor
/// when it finds nothing then).
#[derive(Debug)]
pub struct UpdateDialog {
    /// Asked for from the Help menu: says what it found, whatever it is.
    manual: bool,
    state: State,
}

impl UpdateDialog {
    fn start(manual: bool) -> UpdateDialog {
        let slot: Slot<Result<Release, String>> = Arc::default();
        let into = slot.clone();
        let _ = std::thread::Builder::new()
            .name("bv-updates".into())
            .spawn(move || {
                let found = fetch_latest();
                if let Ok(mut s) = into.lock() {
                    *s = Some(found);
                }
            });
        UpdateDialog {
            manual,
            state: State::Checking(slot),
        }
    }

    /// The window as it shows `version` found (screen tests).
    pub(crate) fn found(version: &str) -> UpdateDialog {
        UpdateDialog {
            manual: true,
            state: State::Available(Release {
                version: Version::parse(version).unwrap_or_else(Version::current),
                page: format!("{RELEASES}tag/v{version}"),
                setup: None,
            }),
        }
    }

    /// Takes what the background jobs left.
    fn poll(&mut self) {
        let next = match &self.state {
            State::Checking(slot) => match slot.lock().ok().and_then(|mut s| s.take()) {
                Some(Ok(r)) if r.version.newer_than(&Version::current()) => {
                    Some(State::Available(r))
                }
                Some(Ok(_)) => Some(State::UpToDate),
                Some(Err(e)) => Some(State::Failed(e)),
                None => None,
            },
            State::Downloading {
                release, result, ..
            } => match result.lock().ok().and_then(|mut s| s.take()) {
                Some(Ok(_)) => Some(State::Ready(release.clone())),
                Some(Err(e)) => Some(State::Failed(e)),
                None => None,
            },
            _ => None,
        };
        if let Some(n) = next {
            self.state = n;
        }
    }

    /// Update: the installer downloaded (on Windows, installed by it), or
    /// the release's page opened.
    fn update(&mut self, ctx: &egui::Context, release: Release) {
        match release.setup.clone().filter(|_| installed()) {
            Some(asset) => {
                let done = Arc::new(AtomicU64::new(0));
                let cancel = Arc::new(AtomicBool::new(false));
                let result: Slot<Result<PathBuf, String>> = Arc::default();
                let dest = std::env::temp_dir().join(&asset.name);
                let (d, c, r) = (done.clone(), cancel.clone(), result.clone());
                let repaint = ctx.clone();
                let _ = std::thread::Builder::new()
                    .name("bv-update-download".into())
                    .spawn(move || {
                        let got = download(&asset, &dest, &d, &c).map(|()| dest);
                        if let Ok(mut s) = r.lock() {
                            *s = Some(got);
                        }
                        repaint.request_repaint();
                    });
                self.state = State::Downloading {
                    release,
                    done,
                    cancel,
                    result,
                };
            }
            None => {
                ctx.open_url(egui::OpenUrl::new_tab(&release.page));
                self.state = State::Ready(release);
            }
        }
    }

    /// Draws the window; `false` once it is closed.
    pub fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) -> bool {
        self.poll();
        let busy = matches!(self.state, State::Checking(_) | State::Downloading { .. });
        if busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
        // As BananaV starts, only a newer release shows a window.
        if !self.manual {
            match &self.state {
                State::Checking(_) => return true,
                State::UpToDate | State::Failed(_) => return false,
                _ => {}
            }
        }
        let (mut open, mut close) = (true, false);
        let mut action: Option<Act> = None;
        let ready_here = matches!(&self.state, State::Ready(r) if r.setup.is_some() && installed());
        crate::floating::Window::new(tr("BananaV Update"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_min_width(380.0);
                ui.spacing_mut().item_spacing.y = 8.0;
                let button = |ui: &mut egui::Ui, text: &str| {
                    ui.add(egui::Button::new(text).min_size(vec2(80.0, 0.0)))
                };
                match &self.state {
                    State::Checking(_) => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(tr("Looking for a newer BananaV…"));
                        });
                        close = button(ui, tr("Cancel")).clicked();
                    }
                    State::Available(r) => {
                        ui.label(
                            RichText::new(trf!(
                                "BananaV {} is available (you have {}).",
                                r.version,
                                Version::current()
                            ))
                            .strong(),
                        );
                        if ui.link(tr("What's new in this version")).clicked() {
                            ctx.open_url(egui::OpenUrl::new_tab(&r.page));
                        }
                        let installs = r.setup.is_some() && installed();
                        ui.horizontal(|ui| {
                            let tip = if installs {
                                tr("Downloads its installer, which opens as BananaV closes.")
                            } else {
                                tr("Opens its download page.")
                            };
                            if button(ui, tr("Update")).on_hover_text(tip).clicked() {
                                action = Some(Act::Update(r.clone()));
                            }
                            close |= button(ui, tr("Cancel")).clicked();
                            if button(ui, tr("Don't Show Again"))
                                .on_hover_text(tr(
                                    "BananaV stops looking for updates as it starts (Options › Preferences › General turns it back on; Help › Check for Software Updates… looks at any time).",
                                ))
                                .clicked()
                            {
                                action = Some(Act::DontShowAgain);
                                close = true;
                            }
                        });
                    }
                    State::UpToDate => {
                        ui.label(trf!(
                            "You have the latest BananaV ({}).",
                            Version::current()
                        ));
                        close = button(ui, "OK").clicked();
                    }
                    State::Failed(e) => {
                        ui.label(trf!("BananaV could not look for updates: {}.", e));
                        close = button(ui, "OK").clicked();
                    }
                    State::Downloading {
                        release,
                        done,
                        cancel,
                        ..
                    } => {
                        let size = release.setup.as_ref().map_or(0, |a| a.size).max(1);
                        let got = done.load(Ordering::Relaxed).min(size);
                        ui.label(trf!("Downloading BananaV {}…", release.version));
                        ui.add(
                            egui::ProgressBar::new(got as f32 / size as f32)
                                .desired_width(360.0)
                                .text(format!(
                                    "{} / {}",
                                    crate::fmt::bytes(got as f64),
                                    crate::fmt::bytes(size as f64)
                                )),
                        );
                        if button(ui, tr("Cancel")).clicked() {
                            cancel.store(true, Ordering::Relaxed);
                            close = true;
                        }
                    }
                    State::Ready(r) => {
                        if ready_here {
                            ui.label(trf!(
                                "BananaV {} is downloaded: its installer opens as BananaV closes.",
                                r.version
                            ));
                            ui.horizontal(|ui| {
                                if button(ui, tr("Install Now")).clicked() {
                                    action = Some(Act::Exit);
                                    close = true;
                                }
                                close |= button(ui, tr("Later")).clicked();
                            });
                        } else {
                            ui.label(tr(
                                "The download page opened in your browser: install the new version from there.",
                            ));
                            close = button(ui, "OK").clicked();
                        }
                    }
                }
            });
        match action {
            Some(Act::Update(r)) => self.update(ctx, r),
            Some(Act::DontShowAgain) => {
                app.settings.prefs.check_updates = false;
                let _ = app.settings.save();
            }
            Some(Act::Exit) => app.request(crate::app::Pending::Exit),
            None => {}
        }
        // Downloaded: installed as BananaV closes, whatever is pressed.
        if let State::Ready(r) = &self.state
            && ready_here
            && let Some(asset) = &r.setup
        {
            app.install_on_exit = Some(std::env::temp_dir().join(&asset.name));
        }
        if !open && let State::Downloading { cancel, .. } = &self.state {
            cancel.store(true, Ordering::Relaxed);
        }
        open && !close
    }
}

/// Runs the installer downloaded, BananaV closing.
pub(crate) fn run_installer(setup: &Path) {
    let _ = std::process::Command::new(setup).spawn();
}

impl BananaApp {
    /// Help › Check for Software Updates… (`manual`), and the check as
    /// BananaV starts.
    pub(crate) fn check_updates(&mut self, manual: bool) {
        if !manual && self.menu_dialogs.more.updates.is_some() {
            return;
        }
        self.menu_dialogs.more.updates = Some(UpdateDialog::start(manual));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn sha256_gives_the_standards_digests() {
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&sha256(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        // A million a's: many blocks.
        assert_eq!(
            hex(&sha256(&vec![b'a'; 1_000_000])),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn versions_compare_as_releases_do() {
        let v = |s| Version::parse(s).unwrap();
        assert!(v("v0.3.0").newer_than(&v("0.2.0")));
        assert!(v("0.2.1").newer_than(&v("0.2.0")));
        assert!(v("1.0").newer_than(&v("0.9.9")));
        assert!(!v("0.2.0").newer_than(&v("0.2.0")));
        assert!(!v("0.1.0").newer_than(&v("0.2.0")));
        // A release after its pre-releases.
        assert!(v("0.1.0").newer_than(&v("0.1.0-ultra-alpha.2")));
        assert!(!v("0.1.0-ultra-alpha.2").newer_than(&v("0.1.0")));
        assert_eq!(v("v0.1.0-ultra-alpha.2").to_string(), "0.1.0-ultra-alpha.2");
        assert!(Version::parse("not a version").is_none());
    }

    #[test]
    fn githubs_answer_gives_the_release_and_this_systems_installer() {
        let digest = "0719c06b2262e1195f4be754b5981b23ce19e5e5d7bdbdb523e3981a2ae828d1";
        let json = format!(
            r#"{{"tag_name":"v0.3.0","html_url":"https://github.com/acasoq-web/BananaV/releases/tag/v0.3.0",
            "draft":false,"prerelease":false,"assets":[
            {{"name":"BananaV-0.3.0-windows-arm64-setup.exe","size":5,"browser_download_url":"https://github.com/acasoq-web/BananaV/releases/download/v0.3.0/BananaV-0.3.0-windows-arm64-setup.exe"}},
            {{"name":"BananaV-0.3.0-windows-x64-setup.exe","size":14266508,"digest":"sha256:{digest}",
              "browser_download_url":"https://github.com/acasoq-web/BananaV/releases/download/v0.3.0/BananaV-0.3.0-windows-x64-setup.exe"}}]}}"#
        );
        let r = parse_release(&json, Some("-windows-x64-setup.exe")).unwrap();
        assert_eq!(r.version.to_string(), "0.3.0");
        let a = r.setup.unwrap();
        assert_eq!(a.name, "BananaV-0.3.0-windows-x64-setup.exe");
        assert_eq!(a.size, 14_266_508);
        assert_eq!(hex(&a.sha256.unwrap()), digest);
        // No installer for this system: the page alone.
        let r = parse_release(&json, None).unwrap();
        assert!(r.setup.is_none());
        // Files elsewhere are not taken; pre-releases are not offered.
        let elsewhere = json.replace(
            "https://github.com/acasoq-web/BananaV/releases/download/v0.3.0/BananaV-0.3.0-windows-x64",
            "https://example.com/x64",
        );
        let r = parse_release(&elsewhere, Some("-windows-x64-setup.exe")).unwrap();
        assert!(r.setup.is_none());
        let pre = json.replace(r#""prerelease":false"#, r#""prerelease":true"#);
        assert!(parse_release(&pre, None).is_err());
        assert!(parse_release("{}", None).is_err());
    }

    #[test]
    fn a_file_matches_its_asset_by_size_and_checksum() {
        let dir = std::env::temp_dir().join(format!("bv-update-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("setup.exe");
        std::fs::write(&path, b"abc").unwrap();
        let mut asset = Asset {
            name: "setup.exe".into(),
            url: String::new(),
            size: 3,
            sha256: Some(sha256(b"abc")),
        };
        assert!(matches(&path, &asset));
        asset.sha256 = Some(sha256(b"abd"));
        assert!(!matches(&path, &asset));
        asset.sha256 = None;
        assert!(matches(&path, &asset));
        asset.size = 4;
        assert!(!matches(&path, &asset));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
