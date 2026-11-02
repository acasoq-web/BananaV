//! Video proxies in the interface: Project Media › Create Video Proxy, the
//! automatic ones for media larger than 4K (Options › Preferences › Video),
//! the proxies made before found again, and the progress of those being
//! made.

use std::path::PathBuf;

use bv_engine::Engine;
use bv_timeline::{MediaId, MediaInfo};

use crate::app::BananaApp;
use crate::settings::AutoProxies;

impl BananaApp {
    /// Where proxies go when a media file's own folder takes no files.
    fn proxy_folder(&self) -> PathBuf {
        self.settings.prefs.temp_dir().join("Proxies")
    }

    /// Project Media › Create Video Proxy.
    pub(crate) fn create_proxy(&mut self, id: MediaId) {
        let Some(info) = self.project().media(id).cloned() else {
            return;
        };
        let folder = self.proxy_folder();
        if self.engine.create_proxy(&info, &folder) {
            self.notify(trf!(
                "Making the proxy of {}…",
                crate::i18n::media_name(&info)
            ));
        }
    }

    /// Project Media › Delete Video Proxy.
    pub(crate) fn delete_proxy(&mut self, id: MediaId) {
        if let Some(info) = self.project().media(id) {
            Engine::delete_proxy(info);
            self.engine.refresh_thumbnails();
        }
    }

    /// A media file just brought in: its proxy made before, or (Options ›
    /// Preferences › Video) a new one when it is larger than 4K — always
    /// (On), or when this computer does not decode it in time (Auto).
    pub(crate) fn proxy_for_new_media(&mut self, id: MediaId) {
        let Some(info) = self.project().media(id).cloned() else {
            return;
        };
        if Engine::find_proxy(&info, &self.proxy_folder()) || !bv_engine::larger_than_4k(&info) {
            return;
        }
        match self.settings.prefs.auto_proxies {
            AutoProxies::Off => {}
            AutoProxies::On => self.create_proxy(id),
            AutoProxies::Auto => {
                // Timing the decoder takes a moment: off the interface.
                let job = std::thread::spawn(move || bv_engine::decodes_in_time(&info));
                self.proxy_checks.push((id, job));
            }
        }
    }

    /// The proxies made before of a project just opened.
    pub(crate) fn find_proxies(&mut self) {
        let folder = self.proxy_folder();
        let media: Vec<MediaInfo> = self.project().media.clone();
        for info in &media {
            Engine::find_proxy(info, &folder);
        }
    }

    /// The proxies being made: progress in the status bar, and a word when
    /// one is done.
    pub(crate) fn follow_proxies(&mut self, ctx: &crate::egui::Context) {
        let (checked, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.proxy_checks)
            .into_iter()
            .partition(|(_, job)| job.is_finished());
        self.proxy_checks = waiting;
        for (id, job) in checked {
            if job.join().is_ok_and(|fast| !fast) {
                self.create_proxy(id);
            }
        }
        let (going, done) = self.engine.proxies();
        let name = |app: &BananaApp, original: &str| {
            app.project()
                .media
                .iter()
                .find(|m| matches!(&m.source, bv_timeline::MediaSource::File { path } if path == original))
                .map_or_else(|| original.to_string(), crate::i18n::media_name)
        };
        if let Some(p) = going.first() {
            let percent = (p.done * 100).checked_div(p.total).unwrap_or(0);
            let text = trf!(
                "Making the proxy of {}: {}%",
                name(self, &p.original),
                percent
            );
            self.notify(text);
        }
        for (original, result) in done {
            let media = name(self, &original);
            match result {
                Ok(()) => {
                    self.notify(trf!("The proxy of {} is ready: previews read it.", media));
                    self.engine.refresh_thumbnails();
                }
                Err(e) => self.report_error(trf!("Could not make the proxy of {}: {}", media, e)),
            }
        }
        if !going.is_empty() || !self.proxy_checks.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
}

/// Whether a media file can have a proxy (a video file).
pub(crate) fn can_have_proxy(info: &MediaInfo) -> bool {
    matches!(info.source, bv_timeline::MediaSource::File { .. })
        && info.video.is_some()
        && !info.still
        && info.sequence.is_none()
}
