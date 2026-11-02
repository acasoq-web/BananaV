//! Custom compositing, as VEGAS's: a video track's Compositing Mode, or a
//! compositing parent's Parent Composite Mode, set to Custom runs a
//! compositor plug-in (`Track::compositor`, `parent_compositor`), picked in
//! the Compositors window (`tabs/compositors.rs`); its settings show in a
//! window of their own.

use bv_timeline::{CompositeMode, PluginFx, TrackId, TrackKind};

use crate::app::BananaApp;
use crate::dock::Tab;
use crate::egui;
use crate::i18n::tr;

/// The window of the settings of `track`'s compositor: its Compositing
/// Mode's, or (`parent`) its Parent Composite Mode's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CompositorWindow {
    pub track: TrackId,
    pub parent: bool,
}

impl BananaApp {
    /// Custom picked in a track's compositing menu: with a compositor, the
    /// track composites through it and its settings open; without one, the
    /// Compositors window opens to pick one for it.
    pub(crate) fn choose_custom_compositing(&mut self, track: TrackId, parent: bool) {
        let has = self.project().track(track).is_some_and(|t| {
            if parent {
                t.parent_compositor.is_some()
            } else {
                t.compositor.is_some()
            }
        });
        if has {
            self.modify(|p| {
                if let Some(t) = p.track_by_id_mut(track) {
                    if parent {
                        t.parent_mode = CompositeMode::Custom;
                    } else {
                        t.composite_mode = CompositeMode::Custom;
                    }
                }
            });
            self.compositor_window = Some(CompositorWindow { track, parent });
        } else {
            self.compositor_pick = Some((track, parent));
            self.show_tab(Tab::Compositors, false);
            self.notify(tr(
                "Pick a compositor in the Compositors window: double-click one of its presets.",
            ));
        }
    }

    /// The track (and which of its modes: `true` its Parent Composite
    /// Mode) a compositor picked in the Compositors window goes to: the
    /// one whose Custom mode asked for one, else the selected video track
    /// (the selected event's).
    pub(crate) fn compositor_target(&self) -> Option<(TrackId, bool)> {
        let p = self.project();
        let video = |id: TrackId| p.track(id).is_some_and(|t| t.kind == TrackKind::Video);
        if let Some((track, parent)) = self.compositor_pick
            && video(track)
        {
            return Some((track, parent));
        }
        let track = self.selected_track.or_else(|| {
            self.primary_clip()
                .and_then(|c| p.clip_track(c).map(|t| t.id))
        })?;
        video(track).then_some((track, false))
    }

    /// `fx` as `track`'s compositor (`parent`: its Parent Composite Mode's):
    /// the mode Custom, its settings open.
    pub(crate) fn apply_compositor(&mut self, track: TrackId, parent: bool, fx: PluginFx) {
        self.modify(|p| {
            if let Some(t) = p.track_by_id_mut(track) {
                if parent {
                    t.parent_mode = CompositeMode::Custom;
                    t.parent_compositor = Some(Box::new(fx));
                } else {
                    t.composite_mode = CompositeMode::Custom;
                    t.compositor = Some(Box::new(fx));
                }
            }
        });
        self.compositor_pick = None;
        self.compositor_window = Some(CompositorWindow { track, parent });
    }

    /// The compositor's settings window, while it is open.
    pub(crate) fn compositor_window(&mut self, ctx: &egui::Context) {
        let Some(w) = self.compositor_window else {
            return;
        };
        let p = self.project();
        let Some(track) = p.track(w.track) else {
            self.compositor_window = None;
            return;
        };
        let fx = if w.parent {
            track.parent_compositor.as_deref().cloned()
        } else {
            track.compositor.as_deref().cloned()
        };
        let Some(fx) = fx else {
            self.compositor_window = None;
            return;
        };
        let name = crate::i18n::track_name(track);
        let title = if w.parent {
            trf!("Parent Composite Mode: {}", name)
        } else {
            trf!("Compositing Mode: {}", name)
        };
        let place =
            crate::ofx::place_of(crate::fx_window::FxTarget::Track(w.track), p, self.cursor);
        let mut open = true;
        crate::floating::Window::new(title)
            .id(egui::Id::new(("compositor", w.track.0, w.parent)))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                let key = crate::ofx::EditorKey {
                    owner: crate::ofx::EditorOwner::Compositor(w.track, w.parent),
                    index: 0,
                    id: fx.id.clone(),
                };
                if let Some(new) = crate::ofx::params(ui, &mut self.ofx, key, &fx, &place, 1) {
                    let id = egui::Id::new(("compositor-params", w.track.0, w.parent));
                    self.modify_merged(id, |p| {
                        if let Some(t) = p.track_by_id_mut(w.track) {
                            let slot = if w.parent {
                                &mut t.parent_compositor
                            } else {
                                &mut t.compositor
                            };
                            *slot = Some(Box::new(new));
                        }
                    });
                }
            });
        if !open {
            self.compositor_window = None;
        }
    }
}
