//! A transition's settings, as VEGAS's: the icon at the right of a
//! crossfade's label on the timeline (`timeline/lanes.rs`'
//! `transition_labels`) opens the Video Event FX window on the transition
//! (`FxTarget::Transition`): its plug-in alone in the chain — its check box
//! switching it off, Remove taking it out —, its presets, its settings in
//! one to three columns and their keyframes.

use bv_timeline::ClipId;

use crate::app::BananaApp;
use crate::fx_window::FxTarget;

impl BananaApp {
    /// The FX window on the transition event `clip` comes in with (`out`:
    /// the one it goes out with).
    pub(crate) fn open_transition_window(&mut self, clip: ClipId, out: bool) {
        self.clip_windows.fx_target = Some(FxTarget::Transition(clip, out));
    }

    /// For review: the first crossfade of the first video track given
    /// 3D Fly In/Out, its FX window open when `open`. Whether there was
    /// one.
    pub fn transition_for_review(&mut self, open: bool) -> bool {
        self.transition_plugin_for_review("org.bananav.FlyInOut3D", None, open)
    }

    /// For review: the first crossfade of the first video track given the
    /// transition plug-in `id` at its factory preset `preset` (none: its
    /// defaults), its FX window open when `open`. Whether there was one.
    pub fn transition_plugin_for_review(
        &mut self,
        id: &str,
        preset: Option<&str>,
        open: bool,
    ) -> bool {
        let Some(info) = crate::ofx::transitions()
            .into_iter()
            .find(|p| p.identifier == id)
        else {
            return false;
        };
        let mut fx = crate::ofx::plugin_fx(&info);
        if let Some(preset) = preset {
            fx.params = crate::presets::builtin_plugin(id)
                .into_iter()
                .find(|(n, _)| n == preset)
                .map(|(_, v)| v)
                .unwrap_or_default();
        }
        let p = self.project();
        let Some(track) = p
            .tracks
            .iter()
            .find(|t| t.kind == bv_timeline::TrackKind::Video)
        else {
            return false;
        };
        let Some(clip) = track
            .clips
            .iter()
            .find(|b| {
                track
                    .clips
                    .iter()
                    .any(|a| a.id != b.id && a.start < b.start && a.end() > b.start)
            })
            .map(|b| b.id)
        else {
            return false;
        };
        self.modify(|p| {
            if let Some(c) = p.clip_mut(clip) {
                c.transition_fx = Some(Box::new(fx));
            }
        });
        if open {
            self.open_transition_window(clip, false);
        }
        true
    }
}
