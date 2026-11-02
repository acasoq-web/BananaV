//! Media previews' side of the interface (VEGAS's "Previewing media
//! files"): Start Preview, Stop Preview and Auto Preview in Project Media
//! and the Explorer, the Trimmer's Play, Pause and Stop — a media plays
//! alone through the preview bus (`Engine::start_media_preview`), its
//! picture in the Trimmer, its level on the Master Bus window's Preview
//! fader (View › Mixer Preview Fader).

use crate::i18n::tr;
use bv_core::Tick;
use bv_timeline::{MediaId, MediaInfo};

use crate::app::BananaApp;
use crate::tabs::master::MeterState;

/// The preview playing.
#[derive(Clone, Debug)]
pub(crate) struct MediaPreview {
    pub info: MediaInfo,
    /// The project's media it is (`None`: a file of the Explorer).
    pub media: Option<MediaId>,
    /// Where it started (the Trimmer's Stop goes back there).
    pub from: Tick,
}

/// What the previews keep between frames.
#[derive(Debug, Default)]
pub(crate) struct PreviewState {
    pub playing: Option<MediaPreview>,
    /// The Preview fader's meter.
    pub meters: MeterState,
}

impl BananaApp {
    /// Plays media `media` of the project from `from` (its time) through
    /// the preview bus.
    pub(crate) fn preview_media(&mut self, media: MediaId, from: Tick) {
        let Some(info) = self.project().media(media).cloned() else {
            return;
        };
        self.start_preview(info, Some(media), from);
    }

    /// Plays `items` (media, or their parts) one after another through the
    /// preview bus: a storyboard's Start Preview.
    pub(crate) fn start_sequence_preview(
        &mut self,
        items: Vec<(MediaInfo, Option<bv_core::TimeRange>)>,
    ) {
        let Some(first) = items.first().map(|(i, _)| i.clone()) else {
            return;
        };
        if self.is_playing() {
            self.stop();
        }
        let gain = self.settings.preview_gain_db;
        self.engine.start_sequence_preview(&items, gain);
        self.previews.playing = Some(MediaPreview {
            info: first,
            media: None,
            from: Tick::ZERO,
        });
    }

    /// Plays a file of the Explorer through the preview bus.
    pub(crate) fn preview_file(&mut self, path: &std::path::Path) {
        match self.probe_cached(path) {
            Some(info) => self.start_preview(info, None, Tick::ZERO),
            None => self.notify(tr("This file cannot be previewed.")),
        }
    }

    pub(crate) fn start_preview(&mut self, info: MediaInfo, media: Option<MediaId>, from: Tick) {
        if self.is_playing() {
            self.stop();
        }
        let gain = self.settings.preview_gain_db;
        self.engine.start_media_preview(&info, from, gain);
        self.previews.playing = Some(MediaPreview { info, media, from });
    }

    /// Stop Preview; `back`: the Trimmer's cursor returns to where the
    /// preview started (its Stop), else it stays (Pause).
    pub(crate) fn stop_preview(&mut self, back: bool) {
        let at = self.engine.stop_media_preview();
        if let Some(p) = self.previews.playing.take()
            && p.media.is_some()
            && p.media == self.trimmer.media_id()
        {
            self.trimmer
                .set_cursor_file(if back { p.from } else { at.unwrap_or(p.from) });
        }
    }

    pub(crate) fn previewing(&self) -> bool {
        self.previews.playing.is_some()
    }

    /// Each frame: the Trimmer's cursor follows the preview of its media
    /// (with Loop Playback, back to the loop region's start past its end),
    /// the Preview fader's meter its peaks; a preview played to its end
    /// stops.
    pub(crate) fn follow_preview(&mut self) {
        self.previews
            .meters
            .update(self.engine.take_preview_peaks());
        let Some(p) = &self.previews.playing else {
            return;
        };
        match self.engine.media_preview_position() {
            Some(t) => {
                if p.media.is_some() && p.media == self.trimmer.media_id() {
                    self.trimmer.set_cursor_file(t);
                    let at = self.trimmer.marks_cursor();
                    if self.loop_enabled
                        && let Some(back) = self.trimmer.loop_back(at)
                    {
                        crate::tabs::trimmer::play_again_from(self, back);
                    }
                }
            }
            None => {
                self.engine.stop_media_preview();
                self.previews.playing = None;
            }
        }
    }
}

impl BananaApp {
    /// View › Mixer Preview Fader on, the Preview fader at −6 dB and its
    /// meter showing a preview's peaks; Project Media's Auto Preview on
    /// (screen tests; the settings in memory).
    pub fn preview_fader_for_review(&mut self) {
        self.settings = crate::settings::UiSettings::in_memory();
        self.settings.preview_fader = true;
        self.settings.preview_gain_db = -6.0;
        self.settings.media_auto_preview = true;
        self.previews.meters.update(Some(vec![0.35, 0.3]));
        self.show_tab(crate::dock::Tab::ProjectMedia, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::egui;

    #[test]
    fn stop_takes_the_trimmers_cursor_back_and_pause_leaves_it() {
        let s = Tick::from_seconds;
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        let id = app.project().media[0].id;
        let info = app.project().media[0].clone();
        app.trimmer.open(id);
        for (back, expected) in [(true, s(1)), (false, s(1))] {
            app.trimmer.set_cursor(s(3));
            app.previews.playing = Some(MediaPreview {
                info: info.clone(),
                media: Some(id),
                from: s(1),
            });
            app.stop_preview(back);
            assert!(!app.previewing());
            // Without a playback the engine knows no position: where the
            // preview began.
            assert_eq!(app.trimmer.marks_cursor(), expected);
        }
    }
}
