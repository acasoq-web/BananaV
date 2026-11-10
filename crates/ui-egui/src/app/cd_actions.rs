//! Actions of the audio CD layout and of command markers (the Insert and
//! Tools menus, Project Media's Add as CD Track, the cursor's steps from
//! track to track and index to index) and the Marker Tool's keys.

use crate::i18n::tr;
use bv_core::{Tick, TimeRange};
use bv_timeline::cd::CD_PAUSE;
use bv_timeline::{MarkerId, MediaId, TrackKind};

use super::BananaApp;

impl BananaApp {
    /// Insert › Audio CD Track Region (N): a track over the time selection.
    pub(crate) fn insert_cd_track(&mut self) {
        let Some(r) = self
            .project()
            .loop_region
            .filter(|r| r.duration > Tick::ZERO)
        else {
            self.notify(tr(
                "Make a time selection (loop region) to turn into a CD track.",
            ));
            return;
        };
        if self.modify(|p| p.add_cd_track(r)).is_none() {
            self.notify(tr(
                "A CD track cannot overlap another, and a disc holds 99 tracks.",
            ));
        }
    }

    /// Insert › Audio CD Track Index (Shift+N): an index marker at the
    /// cursor, inside a track.
    pub(crate) fn insert_cd_index(&mut self) {
        let at = self.mark_time();
        if self.modify(|p| p.add_cd_index(at)).is_none() {
            self.notify(tr(
                "An index goes inside a CD track, after its start (99 at most).",
            ));
        }
    }

    /// Tools › Lay Out Audio CD from Events.
    pub(crate) fn lay_out_cd(&mut self) {
        let n = self.modify(|p| p.lay_out_cd_from_events());
        self.notify(trn!(
            n,
            "{} CD track laid out.",
            "{} CD tracks laid out.",
            n
        ));
    }

    /// Project Media's Add as CD Track: each media's sound at the cursor on
    /// an audio track, one after the other with the two seconds of silence
    /// a disc wants between them (and before the first), each a CD track
    /// named after its file.
    pub(crate) fn add_as_cd_track(&mut self, media: &[MediaId]) {
        let first_track = self.project().cd_tracks.is_empty();
        let mut at = self.cursor;
        if first_track {
            at = at.max(CD_PAUSE);
        }
        let new = self.settings.prefs.new_events();
        self.modify(|p| {
            let track = p
                .tracks
                .iter()
                .find(|t| t.kind == TrackKind::Audio)
                .map(|t| t.id)
                .unwrap_or_else(|| p.add_track(TrackKind::Audio));
            for &m in media {
                let Some(info) = p.media(m).cloned() else {
                    continue;
                };
                if info.audio.is_none() {
                    continue;
                }
                let Ok(ids) = p.insert_media_as(m, at, None, Some(track), &new) else {
                    continue;
                };
                let end = ids
                    .iter()
                    .filter_map(|&id| p.clip(id).map(|c| c.end()))
                    .max()
                    .unwrap_or(at);
                if let Some(cd) = p.add_cd_track(TimeRange::from_bounds(at, end)) {
                    let name = info.name.clone();
                    p.set_cd_track(cd, |t| t.name = name);
                }
                at = end + CD_PAUSE;
            }
        });
    }

    /// The cursor to the next (`dir` 1) or previous (−1) CD track, or with
    /// `indices` to the next track or index marker.
    pub(crate) fn cd_step(&mut self, indices: bool, dir: i8) {
        let p = self.project();
        let mut points: Vec<Tick> = p.cd_tracks.iter().map(|t| t.range.start).collect();
        if indices {
            points.extend(p.cd_indices.iter().map(|i| i.time));
        }
        points.sort();
        let c = self.cursor;
        let to = if dir > 0 {
            points.into_iter().find(|&t| t > c)
        } else {
            points.into_iter().rev().find(|&t| t < c)
        };
        if let Some(t) = to {
            self.seek(t);
        }
    }

    /// Insert › Command…: the Command Properties dialog for a command at
    /// `at`.
    pub(crate) fn open_command_dialog(&mut self, at: Tick) {
        self.menu_dialogs.more.command = Some(crate::command_ui::CommandDialog::new(self, at));
    }

    /// The Command Properties dialog for command marker `id`.
    pub(crate) fn edit_command(&mut self, id: MarkerId) {
        if let Some(d) = crate::command_ui::CommandDialog::edit(self, id) {
            self.menu_dialogs.more.command = Some(d);
        }
    }

    /// The Marker Tool's keys while it is on, as VEGAS's Help lists them:
    /// ← and → to the previous or next tag of the active bar (Shift
    /// extending the pick), Delete removing the picked ones.
    pub(crate) fn marker_tool_keys(&mut self, ctx: &crate::egui::Context) {
        use crate::egui::{Key, Modifiers};
        if !self.timeline.marker_tool.active {
            return;
        }
        let (left, right, delete, shift) = ctx.input_mut(|i| {
            let shift = i.modifiers.shift;
            let m = if shift {
                Modifiers::SHIFT
            } else {
                Modifiers::NONE
            };
            (
                i.consume_key(m, Key::ArrowLeft),
                i.consume_key(m, Key::ArrowRight),
                i.consume_key(Modifiers::NONE, Key::Delete),
                shift,
            )
        });
        for (go, dir) in [(left, -1), (right, 1)] {
            if go {
                let p = self.project().clone();
                if let Some(t) = self.timeline.marker_tool.step(&p, self.cursor, dir, shift) {
                    self.seek(t);
                }
            }
        }
        if delete {
            let picks: Vec<MarkerId> = self.timeline.marker_tool.picks.iter().copied().collect();
            if !picks.is_empty() {
                self.modify(|p| {
                    for id in &picks {
                        p.remove_marker(*id);
                    }
                });
                self.timeline.marker_tool.picks.clear();
                self.timeline.marker_tool.last = None;
            }
        }
    }

    /// Media let go on the timeline, laid out as the right-drag menu
    /// says (`streams`: video, audio): end to end from `at` on `track`,
    /// each on its own track from there down, or as takes of `clip` (or of
    /// the first one added).
    pub(crate) fn add_dropped(
        &mut self,
        ids: Vec<bv_timeline::MediaId>,
        at: Tick,
        track: Option<bv_timeline::TrackId>,
        clip: Option<bv_timeline::ClipId>,
        arrange: crate::timeline::Arrange,
        (video, audio): (bool, bool),
    ) {
        if !ids.is_empty() {
            // Every event placed (and layer made): one undo step.
            self.as_one_undo(|app| {
                app.add_dropped_now(ids, at, track, clip, arrange, (video, audio))
            });
        }
    }

    fn add_dropped_now(
        &mut self,
        ids: Vec<bv_timeline::MediaId>,
        at: Tick,
        track: Option<bv_timeline::TrackId>,
        clip: Option<bv_timeline::ClipId>,
        arrange: crate::timeline::Arrange,
        (video, audio): (bool, bool),
    ) {
        use crate::timeline::Arrange;
        match arrange {
            Arrange::AcrossTime => {
                let new = self.settings.prefs.new_events();
                let lengths: Vec<Tick> = ids
                    .iter()
                    .map(|&id| {
                        let p = self.project();
                        p.media(id)
                            .cloned()
                            .or_else(|| p.subclip_info(id))
                            .map_or(Tick::ZERO, |m| new.length(&m))
                    })
                    .collect();
                let overlap = crate::timeline::added_overlap_for(self);
                let starts = crate::timeline::added_starts(at, &lengths, overlap);
                for (id, start) in ids.into_iter().zip(starts) {
                    self.place_media_streams(id, start, None, track, video, audio);
                }
            }
            Arrange::AcrossTracks => {
                // A layered Photoshop document alone: its layers, the top
                // one on the first track (VEGAS's Help).
                let ids = match ids.as_slice() {
                    [one] => self.psd_layer_media(*one).unwrap_or(ids),
                    _ => ids,
                };
                let blends = self.psd_blend_modes(&ids);
                // Each on the next track of its kinds, made when missing;
                // let go below the tracks, each on new tracks of its own.
                let below = self.drop_tracks.is_some();
                for (k, id) in ids.into_iter().enumerate() {
                    let kinds = [(video, TrackKind::Video), (audio, TrackKind::Audio)];
                    let mut target = track;
                    if below {
                        self.drop_tracks = Some(Default::default());
                    }
                    for (on, kind) in kinds {
                        if !on || below {
                            continue;
                        }
                        let first = track
                            .and_then(|t| self.project().track(t))
                            .filter(|t| t.kind == kind)
                            .map(|t| t.id);
                        let t = self.nth_track_from(kind, first, k);
                        if first.is_some() || target.is_none() {
                            target = Some(t);
                        }
                    }
                    self.place_media_streams(id, at, None, target, video, audio);
                    // A layer's blend mode: its track's compositing mode,
                    // when the track has nothing else.
                    if let Some(&mode) = blends.get(&id) {
                        let placed: Vec<bv_timeline::ClipId> =
                            self.selection.iter().copied().collect();
                        self.history.amend(|p| {
                            for c in &placed {
                                let Some(tr) = p.clip_track(*c).map(|t| t.id) else {
                                    continue;
                                };
                                if let Some(t) = p.tracks.iter_mut().find(|t| t.id == tr)
                                    && t.kind == TrackKind::Video
                                    && t.clips.len() == 1
                                {
                                    t.composite_mode = mode;
                                }
                            }
                        });
                    }
                }
            }
            Arrange::AsTakes => {
                let (c, others) = match clip {
                    Some(c) => (c, ids),
                    None => {
                        self.place_media_streams(ids[0], at, None, track, video, audio);
                        let Some(&placed) = self.selection.iter().next() else {
                            return;
                        };
                        (placed, ids[1..].to_vec())
                    }
                };
                for id in others {
                    self.add_take(c, id);
                }
            }
        }
    }

    /// A layered Photoshop document's layers as media (made once each, the
    /// document's name and the layer's), the top layer first; `None` for
    /// other media.
    pub(crate) fn psd_layer_media(&mut self, media: MediaId) -> Option<Vec<MediaId>> {
        let info = self.project().media(media)?.clone();
        let bv_timeline::MediaSource::File { path } = &info.source else {
            return None;
        };
        if info.layer.is_some() {
            return None;
        }
        let names = bv_engine::psd_layers(path);
        if names.is_empty() {
            return None;
        }
        let mut ids = Vec::with_capacity(names.len());
        for (k, name) in names.iter().enumerate().rev() {
            ids.push(self.psd_layer_of(&info, k as u16, name));
        }
        Some(ids)
    }

    /// The compositing modes of `ids`' Photoshop layers whose blend modes
    /// BananaV has a mode like (Normal left out).
    fn psd_blend_modes(
        &self,
        ids: &[MediaId],
    ) -> std::collections::HashMap<MediaId, bv_timeline::CompositeMode> {
        let mut read: std::collections::HashMap<String, Vec<[u8; 4]>> = Default::default();
        let mut modes = std::collections::HashMap::new();
        for &id in ids {
            let Some(m) = self.project().media(id) else {
                continue;
            };
            let (bv_timeline::MediaSource::File { path }, Some(k)) = (&m.source, m.layer) else {
                continue;
            };
            let blends = read
                .entry(path.clone())
                .or_insert_with(|| bv_engine::psd_layer_blends(path));
            let mode = blends.get(k as usize).map_or_else(Default::default, |&b| {
                bv_timeline::CompositeMode::from_photoshop(b)
            });
            if !mode.is_default() {
                modes.insert(id, mode);
            }
        }
        modes
    }

    /// The media for layer `k` (`name`) of `info`'s document: the one the
    /// project has, or a new one.
    pub(crate) fn psd_layer_of(
        &mut self,
        info: &bv_timeline::MediaInfo,
        k: u16,
        name: &str,
    ) -> MediaId {
        let same_file = |m: &bv_timeline::MediaInfo| m.source == info.source && m.layer == Some(k);
        if let Some(m) = self.project().media.iter().find(|m| same_file(m)) {
            return m.id;
        }
        let mut layer = info.clone();
        layer.layer = Some(k);
        layer.name = format!("{}: {}", crate::i18n::media_name(info), name);
        layer.markers.clear();
        self.modify(|p| p.add_media(layer))
    }

    /// The `k`-th track of `kind` from `from` (its first one when `None`)
    /// down, made at the bottom when there are not that many.
    fn nth_track_from(
        &mut self,
        kind: TrackKind,
        from: Option<bv_timeline::TrackId>,
        k: usize,
    ) -> bv_timeline::TrackId {
        let list: Vec<bv_timeline::TrackId> = self
            .project()
            .tracks
            .iter()
            .filter(|t| t.kind == kind && !t.adjustment)
            .map(|t| t.id)
            .collect();
        let start = from
            .and_then(|f| list.iter().position(|&t| t == f))
            .unwrap_or(0);
        if let Some(&t) = list.get(start + k) {
            return t;
        }
        let missing = start + k + 1 - list.len();
        let mut last = list.last().copied();
        for _ in 0..missing {
            last = Some(self.modify(|p| p.add_track(kind)));
        }
        last.expect("a track was made")
    }

    /// Timeline time `text` names in the ruler's format.
    pub(crate) fn parse_ruler_text(&self, text: &str) -> Option<Tick> {
        crate::timefmt::parse_position(self.project(), text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::egui;

    #[test]
    fn media_added_as_cd_tracks_leave_two_seconds_between_them() {
        let mut app = BananaApp::new(egui::Context::default());
        let tone = |app: &mut BananaApp| {
            let s = app.project().settings;
            app.modify(|p| {
                p.add_media(bv_timeline::MediaInfo::generated(
                    bv_timeline::Generator::TestTone {
                        frequency_hz: 440.0,
                        level_db: -6.0,
                    },
                    &s,
                ))
            })
        };
        let (a, b) = (tone(&mut app), tone(&mut app));
        app.add_as_cd_track(&[a, b]);
        let p = app.project();
        assert_eq!(p.cd_tracks.len(), 2);
        assert_eq!(
            p.cd_tracks[0].range.start, CD_PAUSE,
            "two seconds before the first"
        );
        assert_eq!(
            p.cd_tracks[1].range.start - p.cd_tracks[0].range.end(),
            CD_PAUSE,
            "and between them"
        );
        app.seek(Tick::ZERO);
        app.cd_step(false, 1);
        assert_eq!(app.cursor, CD_PAUSE);
        app.cd_step(false, 1);
        assert_eq!(app.cursor, app.project().cd_tracks[1].range.start);
    }

    #[test]
    fn dropped_media_lay_out_as_the_menu_says() {
        use crate::timeline::Arrange;
        let s = Tick::from_seconds;
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        // Two pictures (color bars, gradient: video only) and a tone.
        let media: Vec<MediaId> = app.project().media.iter().map(|m| m.id).collect();
        let (bars, gradient, tone) = (media[0], media[1], media[3]);
        let at = s(60);
        let count = |app: &BananaApp| app.project().clips().count();
        let before = count(&app);
        app.add_dropped(
            vec![bars, gradient],
            at,
            None,
            None,
            Arrange::AcrossTime,
            (true, true),
        );
        let mut starts: Vec<Tick> = app
            .project()
            .clips()
            .filter(|(_, c)| c.start >= at)
            .map(|(_, c)| c.start)
            .collect();
        starts.sort();
        assert_eq!(count(&app), before + 2);
        assert_eq!(starts[0], at);
        assert!(starts[1] > at, "end to end: {starts:?}");
        // Across tracks: the same time, two video tracks.
        let at2 = s(120);
        app.add_dropped(
            vec![bars, gradient],
            at2,
            None,
            None,
            Arrange::AcrossTracks,
            (true, true),
        );
        let tracks: Vec<_> = app
            .project()
            .clips()
            .filter(|(_, c)| c.start == at2)
            .map(|(t, _)| t.id)
            .collect();
        assert_eq!(tracks.len(), 2);
        assert_ne!(tracks[0], tracks[1], "each on its own track");
        // As takes: one event, the other a take of it.
        let at3 = s(180);
        app.add_dropped(
            vec![bars, gradient],
            at3,
            None,
            None,
            Arrange::AsTakes,
            (true, true),
        );
        let ev: Vec<_> = app
            .project()
            .clips()
            .filter(|(_, c)| c.start == at3)
            .map(|(_, c)| c.take_count())
            .collect();
        assert_eq!(ev, vec![2], "one event with two takes");
        // Audio only of a tone: only sound.
        let at4 = s(240);
        app.add_dropped(
            vec![tone],
            at4,
            None,
            None,
            Arrange::AcrossTime,
            (false, true),
        );
        assert!(
            app.project()
                .clips()
                .filter(|(_, c)| c.start == at4)
                .all(|(t, _)| t.kind == TrackKind::Audio)
        );
    }

    /// A test layer: its name, its rectangle (top, left, bottom, right) and its
    /// red, green, blue and alpha planes.
    type TestLayer<'a> = (&'a str, [i32; 4], [&'a [u8]; 4]);

    /// A 2×2 RGB Photoshop document with two layers, "Bg" (red) and
    /// "Dot" (one green pixel, blended with Screen), raw.
    fn layered_psd() -> Vec<u8> {
        let mut d = b"8BPS".to_vec();
        d.extend_from_slice(&1u16.to_be_bytes());
        d.extend_from_slice(&[0; 6]);
        d.extend_from_slice(&3u16.to_be_bytes());
        d.extend_from_slice(&2u32.to_be_bytes());
        d.extend_from_slice(&2u32.to_be_bytes());
        d.extend_from_slice(&8u16.to_be_bytes());
        d.extend_from_slice(&3u16.to_be_bytes());
        d.extend_from_slice(&0u32.to_be_bytes());
        d.extend_from_slice(&0u32.to_be_bytes());
        let layers: [TestLayer<'_>; 2] = [
            ("Bg", [0, 0, 2, 2], [&[255; 4], &[0; 4], &[0; 4], &[255; 4]]),
            ("Dot", [0, 1, 1, 2], [&[0], &[255], &[0], &[255]]),
        ];
        let (mut records, mut data) = (Vec::new(), Vec::new());
        for (name, rect, planes) in layers {
            for v in rect {
                records.extend_from_slice(&v.to_be_bytes());
            }
            records.extend_from_slice(&4u16.to_be_bytes());
            for (id, p) in [0i16, 1, 2, -1].into_iter().zip(planes) {
                records.extend_from_slice(&id.to_be_bytes());
                records.extend_from_slice(&(2 + p.len() as u32).to_be_bytes());
                data.extend_from_slice(&0u16.to_be_bytes());
                data.extend_from_slice(p);
            }
            records.extend_from_slice(b"8BIM");
            records.extend_from_slice(if name == "Dot" { b"scrn" } else { b"norm" });
            records.extend_from_slice(&[255, 0, 0, 0]);
            let padded = (1 + name.len()).div_ceil(4) * 4;
            records.extend_from_slice(&(8 + padded as u32).to_be_bytes());
            records.extend_from_slice(&0u32.to_be_bytes());
            records.extend_from_slice(&0u32.to_be_bytes());
            let mut pascal = vec![name.len() as u8];
            pascal.extend_from_slice(name.as_bytes());
            pascal.resize(padded, 0);
            records.extend_from_slice(&pascal);
        }
        let mut info = 2i16.to_be_bytes().to_vec();
        info.extend_from_slice(&records);
        info.extend_from_slice(&data);
        d.extend_from_slice(&((4 + info.len() + 4) as u32).to_be_bytes());
        d.extend_from_slice(&(info.len() as u32).to_be_bytes());
        d.extend_from_slice(&info);
        d.extend_from_slice(&0u32.to_be_bytes());
        d.extend_from_slice(&0u16.to_be_bytes());
        d.extend_from_slice(&[255, 0, 255, 255, 0, 255, 0, 0, 0, 0, 0, 0]);
        d
    }

    #[test]
    fn a_layered_psd_goes_across_tracks_a_layer_each() {
        use crate::timeline::Arrange;
        let dir = std::env::temp_dir().join(format!("bv-psd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Poster.psd");
        std::fs::write(&path, layered_psd()).unwrap();
        let mut app = BananaApp::new(egui::Context::default());
        let ids = app.import_paths(std::slice::from_ref(&path));
        let doc = ids[0];
        let at = Tick::from_seconds(5);
        app.add_dropped(
            vec![doc],
            at,
            None,
            None,
            Arrange::AcrossTracks,
            (true, true),
        );
        let p = app.project();
        let mut placed: Vec<(usize, Option<u16>)> = p
            .clips()
            .filter(|(_, c)| c.start == at)
            .map(|(t, c)| {
                (
                    p.track_index(t.id).unwrap(),
                    p.media(c.media).unwrap().layer,
                )
            })
            .collect();
        placed.sort();
        assert_eq!(
            placed.iter().map(|x| x.1).collect::<Vec<_>>(),
            vec![Some(1), Some(0)],
            "the top layer (Dot) on the upper track, Bg below"
        );
        let names: Vec<String> = p
            .media
            .iter()
            .filter(|m| m.layer.is_some())
            .map(|m| m.name.clone())
            .collect();
        assert!(names.iter().any(|n| n.ends_with(": Dot")), "{names:?}");
        // Each layer's blend mode on its track.
        let mode_of = |layer: u16| {
            p.clips()
                .find(|(_, c)| p.media(c.media).unwrap().layer == Some(layer))
                .map(|(t, _)| t.composite_mode)
        };
        assert_eq!(mode_of(1), Some(bv_timeline::CompositeMode::Screen));
        assert_eq!(mode_of(0), Some(bv_timeline::CompositeMode::SourceAlpha));
        // The same layer asked again: the same media.
        let info = p.media(doc).unwrap().clone();
        let again = app.psd_layer_of(&info, 1, "Dot");
        assert_eq!(
            app.project()
                .media
                .iter()
                .filter(|m| m.layer == Some(1))
                .count(),
            1
        );
        assert_eq!(app.project().media(again).unwrap().layer, Some(1));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
