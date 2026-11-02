//! The analyses an effect's button asks for (BananaV's
//! `kBananaVParamPropAnalysis`, `ofx::AnalysisAsk`: Video Stabilization's
//! Analyze Motion, Brightness and Contrast's Current Clip…) on the
//! interface's side: started in the engine on the effect's event or media,
//! their progress in the status bar with Cancel, and what was found kept
//! in the effect's hidden parameter, with what the effect sets in turn
//! (one undo step); the features a motion analysis followed kept for this
//! session (Show Points).

use bv_timeline::scene::FxOwner;
use bv_timeline::{PluginParam, PluginValue};

use crate::app::BananaApp;
use crate::egui;
use crate::fx_window::FxTarget;
use crate::i18n::tr;

impl BananaApp {
    /// Each frame: an analysis asked for started, one finished kept.
    pub(crate) fn follow_motion_analysis(&mut self) {
        if let Some(ask) = self.ofx.take_analysis() {
            match ask.key.owner {
                FxOwner::Event(_) | FxOwner::Media(_) => {
                    if self.engine.start_motion_analysis(
                        ask.key.owner,
                        ask.kind,
                        ask.accurate,
                        ask.tuning,
                        ask.setup,
                    ) {
                        self.analysis = Some((ask, 0.0));
                    }
                }
                _ => self.notify(match ask.kind {
                    bv_engine::AnalysisKind::Histogram
                    | bv_engine::AnalysisKind::Histograms
                    | bv_engine::AnalysisKind::ColorHistograms => {
                        tr("A clip is measured for an event's or a media's effects.")
                    }
                    _ => tr("Motion is analyzed for an event's or a media's effects."),
                }),
            }
        }
        if self.analysis.is_none() {
            return;
        }
        let Some(p) = self.engine.motion_analysis() else {
            self.analysis = None;
            return;
        };
        if !p.finished {
            if let Some((_, done)) = &mut self.analysis {
                *done = p.done as f32 / p.total.max(1) as f32;
            }
            return;
        }
        let Some((ask, _)) = self.analysis.take() else {
            return;
        };
        let Some(text) = p.motion else {
            return;
        };
        let key = ask.key;
        // Show Points draws them (an event's effect, this session only).
        if let Some(points) = p.points
            && matches!(key.owner, FxOwner::Event(_))
        {
            self.stabilization_points.insert(key.clone(), points);
        }
        let result = PluginParam {
            value: PluginValue::Text(text),
            keys: Vec::new(),
        };
        // What the effect sets in turn (Brightness and Contrast: itself).
        let target = match key.owner {
            FxOwner::Event(c) => Some(FxTarget::Clip(c)),
            FxOwner::Media(m) => Some(FxTarget::Media(m, false)),
            _ => None,
        };
        let before = target.and_then(|t| {
            t.chain(self.project())
                .and_then(|chain| chain.get(key.index).cloned())
                .and_then(|e| e.plugin.map(|f| *f))
                .filter(|f| f.id == key.id)
        });
        let more = match (target, before) {
            (Some(target), Some(before)) => {
                let mut after = before.clone();
                after.params.insert(ask.param.clone(), result.clone());
                let place = crate::ofx::place_of(target, self.project(), self.cursor);
                let editor = crate::ofx::EditorKey {
                    owner: crate::ofx::EditorOwner::Fx(target),
                    index: key.index,
                    id: key.id.clone(),
                };
                crate::ofx::response(
                    &mut self.ofx,
                    editor,
                    (&before, &after),
                    std::slice::from_ref(&ask.param),
                    &place,
                )
            }
            _ => Default::default(),
        };
        self.modify(|p| {
            let effects = match key.owner {
                FxOwner::Event(c) => p.clip_mut(c).map(|c| &mut c.effects),
                FxOwner::Media(m) => p
                    .media
                    .iter_mut()
                    .find(|i| i.id == m)
                    .map(|i| &mut i.effects),
                _ => None,
            };
            if let Some(fx) = effects
                .and_then(|e| e.get_mut(key.index))
                .and_then(|e| e.plugin.as_deref_mut())
                .filter(|f| f.id == key.id)
            {
                fx.params.insert(ask.param.clone(), result);
                fx.params.extend(more);
            }
        });
    }
}

/// The analysis's progress in the status bar, with Cancel.
pub(crate) fn status(app: &mut BananaApp, ui: &mut egui::Ui) {
    let Some((_, done)) = app.analysis else {
        return;
    };
    if ui.button(tr("Cancel")).clicked() {
        app.engine.cancel_motion_analysis();
        app.analysis = None;
        return;
    }
    ui.add(
        egui::ProgressBar::new(done)
            .desired_width(200.0)
            .show_percentage(),
    );
    ui.label(match app.analysis.as_ref().map(|(ask, _)| ask.kind) {
        Some(bv_engine::AnalysisKind::Histogram) => tr("Measuring the clip's brightness…"),
        Some(bv_engine::AnalysisKind::Histograms) => tr("Measuring each frame's brightness…"),
        Some(bv_engine::AnalysisKind::ColorHistograms) => tr("Counting each frame's histograms…"),
        _ => tr("Analyzing motion…"),
    });
    ui.separator();
}

impl BananaApp {
    /// The demo's first video event through Dual Fish Eye Stitching, its
    /// Video Event FX window open on it (screen tests).
    pub fn stitching_for_review(&mut self) -> bool {
        let Some(info) = bv_ofx::registry()
            .known()
            .into_iter()
            .find(|i| i.identifier == "org.bananav.DualFishEyeStitching")
        else {
            return false;
        };
        let Some(id) = self
            .project()
            .tracks
            .iter()
            .find(|t| t.kind == bv_timeline::TrackKind::Video)
            .and_then(|t| t.clips.first())
            .map(|c| c.id)
        else {
            return false;
        };
        let fx = crate::ofx::plugin_fx(&info);
        self.modify(|p| {
            if let Some(c) = p.clip_mut(id) {
                c.effects.push(bv_timeline::Effect::plugin(fx));
            }
        });
        self.select_clip(id, false);
        self.seek(bv_core::Tick::from_seconds(1));
        self.open_clip_window("fx");
        true
    }

    /// The demo's first video event with Video Stabilization in `view`
    /// (0 Basic, 1 Professional, 2 Expert), nothing analyzed yet, its Video
    /// Event FX window open on it (screen tests).
    pub fn video_stabilization_plain_for_review(&mut self, view: f64) -> bool {
        let Some(info) = bv_ofx::registry()
            .known()
            .into_iter()
            .find(|i| i.identifier == "org.bananav.VideoStabilization")
        else {
            return false;
        };
        let Some(id) = self
            .project()
            .tracks
            .iter()
            .find(|t| t.kind == bv_timeline::TrackKind::Video)
            .and_then(|t| t.clips.first())
            .map(|c| c.id)
        else {
            return false;
        };
        let mut fx = crate::ofx::plugin_fx(&info);
        fx.params.insert(
            "views".into(),
            PluginParam {
                value: PluginValue::Num(vec![view]),
                keys: Vec::new(),
            },
        );
        self.modify(|p| {
            if let Some(c) = p.clip_mut(id) {
                c.effects.push(bv_timeline::Effect::plugin(fx));
            }
        });
        self.select_clip(id, false);
        self.seek(bv_core::Tick::from_seconds(1));
        self.open_clip_window("fx");
        true
    }

    /// The demo's first video event with Video Stabilization at Professional
    /// view, its motion analyzed (a small shake), and its Video Event FX
    /// window open on it (screen tests).
    pub fn video_stabilization_for_review(&mut self) -> bool {
        let Some(info) = bv_ofx::registry()
            .known()
            .into_iter()
            .find(|i| i.identifier == "org.bananav.VideoStabilization")
        else {
            return false;
        };
        let Some(id) = self
            .project()
            .tracks
            .iter()
            .find(|t| t.kind == bv_timeline::TrackKind::Video)
            .and_then(|t| t.clips.first())
            .map(|c| c.id)
        else {
            return false;
        };
        let steps: Vec<bv_engine::stabilization::Step> = (0..30)
            .map(|i| {
                let dx = if i % 2 == 0 { 0.004 } else { -0.004 };
                bv_engine::stabilization::Step::similar(dx, 0.0, 0.0, 1.0)
            })
            .collect();
        let mut fx = crate::ofx::plugin_fx(&info);
        let num = |v: f64| PluginParam {
            value: PluginValue::Num(vec![v]),
            keys: Vec::new(),
        };
        fx.params.insert("views".into(), num(1.0));
        fx.params.insert("hasMotionData".into(), num(1.0));
        fx.params.insert(
            "motionData".into(),
            PluginParam {
                value: PluginValue::Text(bv_engine::stabilization::motion_text(&steps)),
                keys: Vec::new(),
            },
        );
        self.modify(|p| {
            if let Some(c) = p.clip_mut(id) {
                c.effects.push(bv_timeline::Effect::plugin(fx));
            }
        });
        self.select_clip(id, false);
        self.open_clip_window("fx");
        true
    }

    /// [`Self::video_stabilization_for_review`] in its Expert view with Show
    /// Points on: features drifting right over thirty frames, those in the
    /// left half followed for ten frames, the others for three.
    pub fn video_stabilization_expert_for_review(&mut self) -> bool {
        if !self.video_stabilization_for_review() {
            return false;
        }
        let Some((id, index)) = self.project().clips().find_map(|(_, c)| {
            c.effects
                .iter()
                .position(|e| {
                    e.plugin
                        .as_deref()
                        .is_some_and(|f| f.id == "org.bananav.VideoStabilization")
                })
                .map(|i| (c.id, i))
        }) else {
            return false;
        };
        self.modify(|p| {
            if let Some(fx) = p
                .clip_mut(id)
                .and_then(|c| c.effects.get_mut(index))
                .and_then(|e| e.plugin.as_deref_mut())
            {
                for (name, v) in [("views", 2.0), ("showFeatures", 1.0)] {
                    fx.params.insert(
                        name.into(),
                        PluginParam {
                            value: PluginValue::Num(vec![v]),
                            keys: Vec::new(),
                        },
                    );
                }
            }
        });
        let frames = (0..30)
            .map(|f| {
                let mut v = Vec::new();
                for gy in 0..6 {
                    for gx in 0..10 {
                        // Left half: followed ten frames; right: three.
                        let life = if gx < 5 { 10 } else { 3 };
                        if f % life == life - 1 {
                            continue;
                        }
                        let x = 30.0 + gx as f32 * 44.0 + (f % life) as f32 * 2.0;
                        let y = 30.0 + gy as f32 * 40.0;
                        v.push(([x, y], [x + 2.0, y]));
                    }
                }
                v
            })
            .collect();
        let key = bv_engine::plugins::EffectKey {
            owner: FxOwner::Event(id),
            index,
            id: "org.bananav.VideoStabilization".into(),
        };
        // The cursor three frames into the event.
        if let Some(start) = self.project().clip(id).map(|c| c.start) {
            self.cursor = start + self.frame_rate().frame_start(3);
        }
        self.stabilization_points.insert(
            key,
            std::sync::Arc::new(bv_engine::FeatureTracks {
                size: (480, 270),
                frames,
            }),
        );
        true
    }
}
