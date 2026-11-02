//! The timeline's shortcut menus, as VEGAS's: an empty part of a track's
//! (its edit commands, tracks, envelopes, fade colors and the events to
//! insert), an event's (its media, FX, edits, switches, takes, groups,
//! streams; an audio event's its channels too) and the empty space below
//! the tracks' (opening and inserting tracks).

use bv_timeline::{ChannelMap, ClipId, EnvelopeKind, MediaSource, TrackId, TrackKind};

use crate::app::BananaApp;
use crate::commands::Cmd;
use crate::dock::Tab;
use crate::egui::Ui;
use crate::i18n::tr;
use crate::icons::Icon;
use crate::menus::{check_item, cmd, cmd_as, icon_item, item, sub};

/// Right-click on an empty part of `track`.
pub(super) fn track_menu(ui: &mut Ui, app: &mut BananaApp, track: TrackId) {
    let Some(kind) = app.project().track(track).map(|t| t.kind) else {
        ui.close();
        return;
    };
    // The track commands act on the track the menu is for.
    app.selected_track = Some(track);
    ui.set_min_width(260.0);
    cmd(ui, app, Cmd::Paste);
    ui.separator();
    for c in [
        Cmd::Delete,
        Cmd::Trim,
        Cmd::TrimStart,
        Cmd::TrimEnd,
        Cmd::Split,
    ] {
        cmd(ui, app, c);
    }
    cmd_as(ui, app, Cmd::Rejoin, tr("Event Heal"));
    cmd(ui, app, Cmd::CloseGaps);
    ui.separator();
    cmd(ui, app, Cmd::SelectAfterCursor);
    ui.separator();
    // Above this track, as VEGAS's.
    match kind {
        TrackKind::Video => {
            if crate::menus::keyed_item(ui, tr("Insert Video Track"), "Ctrl+Shift+Q") {
                app.insert_track_above(kind, track);
            }
            if crate::menus::keyed_item(ui, tr("Insert Adjustment Track"), "Ctrl+Alt+Shift+Q") {
                app.insert_adjustment_track(Some(track));
            }
        }
        TrackKind::Audio => {
            if crate::menus::keyed_item(ui, tr("Insert Audio Track"), "Ctrl+Q") {
                app.insert_track_above(kind, track);
            }
        }
    }
    if item(ui, tr("Duplicate Track")) {
        app.duplicate_track(track);
    }
    if item(ui, tr("Delete Track")) {
        app.request_delete_track(track);
    }
    ui.separator();
    sub(ui, tr("Insert/Remove Envelope"), true, |ui| {
        envelope_items(ui, app, kind)
    });
    if kind == TrackKind::Video {
        ui.separator();
        sub(ui, tr("Fade Colors"), true, |ui| {
            fade_color_items(ui, app, track)
        });
    }
    ui.separator();
    cmd_as(ui, app, Cmd::EmptyEvent, tr("Insert Empty Event"));
    if kind == TrackKind::Video {
        cmd_as(
            ui,
            app,
            Cmd::InsertAdjustmentEvent,
            tr("Insert Adjustment Event"),
        );
        cmd_as(ui, app, Cmd::InsertTextMedia, tr("Insert Text Media…"));
    }
    if item(ui, tr("Insert Generated Media…")) {
        app.show_tab(Tab::Generators, false);
    }
    if kind == TrackKind::Video {
        cmd_as(ui, app, Cmd::Slideshow, tr("Insert Slideshow…"));
    }
}

/// A track's envelopes to insert or remove (the selected track's), by its
/// kind.
pub(super) fn envelope_items(ui: &mut Ui, app: &mut BananaApp, kind: TrackKind) {
    match kind {
        TrackKind::Video => {
            cmd_as(ui, app, Cmd::VideoMuteEnvelope, tr("Mute"));
            cmd_as(
                ui,
                app,
                Cmd::VideoEnvelope(EnvelopeKind::CompositeLevel),
                tr("Composite Level"),
            );
            cmd_as(
                ui,
                app,
                Cmd::VideoEnvelope(EnvelopeKind::FadeToColor),
                tr("Fade to Color"),
            );
        }
        TrackKind::Audio => {
            cmd(ui, app, Cmd::AudioEnvelope(EnvelopeKind::Mute));
            cmd(ui, app, Cmd::AudioEnvelope(EnvelopeKind::Volume));
            cmd(ui, app, Cmd::AudioEnvelope(EnvelopeKind::Pan));
            if app.surround_keys_track().is_some() {
                cmd(ui, app, Cmd::SurroundKeyframes);
            }
            // Its sends' (VEGAS's bus envelopes): one for each bus and
            // chain, named by it.
            let p = app.project();
            let targets: Vec<(bv_timeline::SendTo, String)> = p
                .busses
                .iter()
                .map(|b| {
                    (
                        bv_timeline::SendTo::Bus(b.id),
                        crate::i18n::bus_name(p, b.id),
                    )
                })
                .chain(p.fx_chains.iter().map(|c| {
                    (
                        bv_timeline::SendTo::Fx(c.id),
                        crate::i18n::chain_name(p, c.id),
                    )
                }))
                .collect();
            if !targets.is_empty() {
                ui.separator();
            }
            for (to, name) in targets {
                cmd_as(ui, app, Cmd::AudioEnvelope(EnvelopeKind::Send(to)), &name);
            }
        }
    }
}

/// The colors video track `track`'s Fade to Color envelope fades to: Top…
/// and Bottom… open the color picker.
pub(super) fn fade_color_items(ui: &mut Ui, app: &mut BananaApp, track: TrackId) {
    let (name, colors) = app.project().track(track).map_or_else(
        || (String::new(), bv_timeline::scene::DEFAULT_FADE_COLORS),
        |t| {
            (
                crate::i18n::track_name(t),
                t.fade_colors
                    .unwrap_or(bv_timeline::scene::DEFAULT_FADE_COLORS),
            )
        },
    );
    for (i, label) in [tr("Top…"), tr("Bottom…")].into_iter().enumerate() {
        if item(ui, label) {
            let title = trf!("{} – {}", name, label.trim_end_matches('…'));
            app.track_color = Some((
                Some(track),
                i,
                crate::color_picker::ColorPicker::new(title, colors[i]),
            ));
        }
    }
}

/// The colors the video bus's Fade to Color envelope fades to (its bus
/// track's Fade Colors): Top… and Bottom… open the color picker.
pub(super) fn bus_fade_color_items(ui: &mut Ui, app: &mut BananaApp) {
    let colors = app
        .project()
        .video_bus
        .fade_colors
        .unwrap_or(bv_timeline::scene::DEFAULT_FADE_COLORS);
    for (i, label) in [tr("Top…"), tr("Bottom…")].into_iter().enumerate() {
        if item(ui, label) {
            let title = trf!("{} – {}", tr("Video Output"), label.trim_end_matches('…'));
            app.track_color = Some((
                None,
                i,
                crate::color_picker::ColorPicker::new(title, colors[i]),
            ));
        }
    }
}

/// Right-click below the tracks, in the timeline or in the track list.
pub(super) fn empty_menu(ui: &mut Ui, app: &mut BananaApp) {
    ui.set_min_width(260.0);
    cmd(ui, app, Cmd::Open);
    cmd_as(ui, app, Cmd::InsertAudioTrack, tr("Insert Audio Track"));
    cmd_as(ui, app, Cmd::InsertVideoTrack, tr("Insert Video Track"));
    cmd_as(
        ui,
        app,
        Cmd::InsertAdjustmentTrack,
        tr("Insert Adjustment Track"),
    );
    ui.separator();
    cmd(ui, app, Cmd::Preferences);
}

/// Right-click on event `clip` (selected by then).
pub(super) fn event_menu(ui: &mut Ui, app: &mut BananaApp, clip: ClipId) {
    let p = app.project();
    let (Some(c), Some(kind)) = (p.clip(clip).cloned(), p.clip_track(clip).map(|t| t.kind)) else {
        ui.close();
        return;
    };
    let media = p.media(c.media).cloned();
    let video = kind == TrackKind::Video;
    ui.set_min_width(280.0);
    if item(ui, tr("Open in Trimmer")) {
        app.trimmer.open(c.media);
        app.show_tab(Tab::Trimmer, false);
    }
    // An event of a subclip's part: its parent media in the Trimmer, the
    // event's part selected (VEGAS greys it for other events).
    let part = event_part(&c, media.as_ref());
    let of_subclip =
        app.project().subclips.iter().any(|s| {
            s.parent == c.media && s.range.start <= part.start && part.end() <= s.range.end()
        });
    if icon_item(ui, tr("Open Parent Media in Trimmer"), None, "", of_subclip) {
        app.trimmer.open_part(c.media, part);
        app.show_tab(Tab::Trimmer, false);
    }
    // On an Auto Ducking controller: its loudness read again, its own
    // reductions.
    let controller = app
        .project()
        .clip_track(clip)
        .is_some_and(|t| t.ducking.is_some());
    if controller && item(ui, tr("Re-Analyze Auto Ducking")) {
        app.reanalyse_ducking(clip);
    }
    if video {
        if item(ui, tr("Select in Project Media list")) {
            app.select_in_project_media(c.media);
        }
    } else {
        cmd(ui, app, Cmd::OpenInAudioEditor);
        cmd(ui, app, Cmd::OpenCopyInAudioEditor);
    }
    cmd(ui, app, Cmd::EditSourceProject);
    cmd(ui, app, Cmd::CreateNestedTimeline);
    cmd(ui, app, Cmd::OpenNestedTimeline);
    // An audio event's: the media list last, then its beats.
    if !video && item(ui, tr("Select in Project Media list")) {
        app.select_in_project_media(c.media);
    }
    ui.separator();
    if video {
        cmd(ui, app, Cmd::MediaFx);
        ui.separator();
        cmd(ui, app, Cmd::PanCropEvent);
        cmd(ui, app, Cmd::VideoEventFx);
        cmd(ui, app, Cmd::MotionTracking);
    } else {
        // Its media's beats as markers, its tempo the project's.
        if item(ui, tr("Beat Detection")) {
            app.beat_detection(c.media);
        }
        if item(ui, tr("Tempo Detection")) {
            app.tempo_detection(c.media);
        }
        ui.separator();
        cmd(ui, app, Cmd::AudioEventFx);
        cmd(ui, app, Cmd::NonRealTimeFx);
    }
    ui.separator();
    cmd(ui, app, Cmd::Cut);
    cmd(ui, app, Cmd::Copy);
    cmd(ui, app, Cmd::PasteAttributes);
    cmd_as(
        ui,
        app,
        Cmd::PasteAttributesSelective,
        tr("Selectively Paste Event Attributes"),
    );
    ui.separator();
    for c in [Cmd::Delete, Cmd::Trim, Cmd::TrimStart, Cmd::TrimEnd] {
        cmd(ui, app, c);
    }
    if video
        && icon_item(
            ui,
            tr("Detect Scenes and Split"),
            Some(Icon::DetectScenes),
            "",
            true,
        )
    {
        app.clip_windows.scenes = Some((clip, 0.5, false));
    }
    cmd(ui, app, Cmd::Split);
    if video {
        cmd(ui, app, Cmd::SmartSplit);
    }
    cmd_as(ui, app, Cmd::Rejoin, tr("Event Heal"));
    cmd(ui, app, Cmd::CloseGaps);
    if video {
        ui.separator();
        let adjustment = media.as_ref().is_some_and(|m| m.is_adjustment());
        if icon_item(
            ui,
            tr("Resize Adjustment Event to Project"),
            None,
            "",
            adjustment,
        ) {
            app.edit(|p| p.resize_adjustment_to_project(clip));
            ui.close();
        }
    }
    ui.separator();
    cmd_as(
        ui,
        app,
        Cmd::AddMissingStream,
        tr("Add Missing Stream for Selected Event"),
    );
    if item(ui, tr("Create Subclip")) {
        app.open_subclip_dialog(c.media, event_part(&c, media.as_ref()));
    }
    cmd(ui, app, Cmd::Reverse);
    ui.separator();
    if item(ui, tr("Select Events to End")) {
        app.select_events_to_end();
    }
    cmd(ui, app, Cmd::SelectAfterCursor);
    // VEGAS's audio events have no envelopes of their own here.
    if video {
        sub(ui, tr("Insert/Remove Envelope"), true, |ui| {
            let velocity = app.has_event_envelope(EnvelopeKind::Velocity);
            if check_item(ui, tr("Velocity"), velocity == Some(true), false, "") {
                app.toggle_event_envelopes(EnvelopeKind::Velocity);
                ui.close();
            }
            // A whole event frozen by an older BananaV: unfrozen here.
            if c.freeze.is_some() {
                if check_item(ui, tr("Freeze Frame at Cursor"), true, false, "") {
                    app.edit(|p| p.set_clip_freeze(clip, None));
                    ui.close();
                }
            } else {
                cmd(ui, app, Cmd::FreezeFrameAtCursor);
            }
            let progress = app.has_event_envelope(EnvelopeKind::TransitionProgress);
            let clicked = ui
                .add_enabled_ui(progress.is_some(), |ui| {
                    check_item(
                        ui,
                        tr("Transition Progress"),
                        progress == Some(true),
                        false,
                        "",
                    )
                })
                .inner;
            if clicked {
                app.toggle_event_envelopes(EnvelopeKind::TransitionProgress);
                ui.close();
            }
        });
    }
    ui.separator();
    sub(ui, tr("Switches"), true, |ui| {
        crate::menus::switches_items(ui, app, Some(kind), true);
    });
    sub(ui, tr("Take"), true, |ui| {
        crate::menus::take_items(ui, app, Some(clip));
    });
    sub(ui, tr("Group"), true, |ui| {
        crate::menus::group_items(ui, app)
    });
    sub(ui, tr("Stream"), true, |ui| {
        // A layered Photoshop document's event: the composite or a layer.
        if video
            && let Some(m) = media.as_ref()
            && psd_stream_items(ui, app, clip, m)
        {
            return;
        }
        let streams = media.as_ref().map_or(1, |m| {
            if video {
                usize::from(m.video.is_some()).max(1)
            } else {
                m.audio_stream_count().max(1)
            }
        });
        for s in 0..streams {
            let on = video || c.audio_stream as usize == s;
            if check_item(ui, &trf!("Stream {}", s + 1), on, true, "") && !video {
                let ids = app.selected();
                let s = s as u16;
                app.modify(|p| {
                    for id in ids {
                        let audio = p.clip_track(id).is_some_and(|t| t.kind == TrackKind::Audio);
                        if let Some(c) = p.clip_mut(id).filter(|_| audio) {
                            c.audio_stream = s;
                        }
                    }
                });
            }
        }
    });
    if !video {
        sub(ui, tr("Channels"), true, |ui| {
            for m in [
                ChannelMap::Both,
                ChannelMap::LeftOnly,
                ChannelMap::RightOnly,
                ChannelMap::Combine,
                ChannelMap::Swap,
            ] {
                cmd(ui, app, Cmd::Channels(m));
            }
        });
    }
    // How far its media is from its other part's (the timeline's badge).
    let offset = super::lanes::sync_offsets(app.project())
        .get(&clip)
        .copied();
    sub(ui, tr("Synchronize"), true, |ui| {
        let on = offset.is_some();
        if icon_item(ui, tr("By Moving"), None, "", on)
            && let Some(d) = offset
        {
            app.edit(|p| p.sync_by_moving(clip, d));
        }
        if icon_item(ui, tr("By Slipping"), None, "", on)
            && let Some(d) = offset
        {
            app.edit(|p| p.sync_by_slipping(clip, d));
        }
    });
    // Sync links: the selected events follow this one (not the other way);
    // its own group's are already tied to it.
    let p = app.project();
    let group = p.group_members(clip);
    let others: Vec<ClipId> = app
        .selection
        .iter()
        .copied()
        .filter(|c| !group.contains(c))
        .collect();
    let linked_here: Vec<ClipId> = others
        .iter()
        .copied()
        .filter(|&c| {
            p.clip(c)
                .and_then(|c| c.sync_link)
                .is_some_and(|l| l.to == clip)
        })
        .collect();
    let own = p.clip(clip).and_then(|c| c.sync_link);
    let followers: Vec<ClipId> = p
        .clips()
        .filter(|(_, c)| c.sync_link.is_some_and(|l| l.to == clip))
        .map(|(_, c)| c.id)
        .collect();
    if icon_item(
        ui,
        tr("Create Sync Link with Selected Events"),
        None,
        "",
        !others.is_empty(),
    ) {
        app.modify(|p| {
            let Some(at) = p.clip(clip).map(|c| c.start) else {
                return;
            };
            for &id in &others {
                if let Some(c) = p.clip_mut(id) {
                    c.sync_link = Some(bv_timeline::SyncLink {
                        to: clip,
                        offset: c.start - at,
                    });
                }
            }
        });
        ui.close();
    }
    sub(ui, tr("Sync Link"), true, |ui| {
        if icon_item(
            ui,
            tr("Select All"),
            None,
            "",
            own.is_some() || !followers.is_empty(),
        ) {
            // The event followed and all that follow it.
            let head = own.map_or(clip, |l| l.to);
            let p = app.project();
            let all: Vec<ClipId> = std::iter::once(head)
                .chain(
                    p.clips()
                        .filter(|(_, c)| c.sync_link.is_some_and(|l| l.to == head))
                        .map(|(_, c)| c.id),
                )
                .collect();
            app.selection = all.into_iter().collect();
            ui.close();
        }
        if icon_item(ui, tr("Unlink"), None, "", own.is_some()) {
            app.modify(|p| {
                if let Some(c) = p.clip_mut(clip) {
                    c.sync_link = None;
                }
            });
            ui.close();
        }
        // The selected events that follow this one: let go.
        if icon_item(
            ui,
            tr("Remove Sync Link from Selected Events"),
            None,
            "",
            !linked_here.is_empty(),
        ) {
            app.modify(|p| {
                for &id in &linked_here {
                    if let Some(c) = p.clip_mut(id) {
                        c.sync_link = None;
                    }
                }
            });
            ui.close();
        }
    });
    ui.separator();
    let generated = media
        .as_ref()
        .is_some_and(|m| matches!(m.source, MediaSource::Generated(_)));
    if generated && item(ui, tr("Generated Media…")) {
        app.open_generated_media(clip);
    }
    if item(ui, tr("Properties…")) {
        app.dialogs.event_properties =
            crate::event_properties::EventPropertiesDialog::new(app, clip);
    }
}

/// The part of its media an event plays (its media time from its start to
/// its end, within the media).
fn event_part(c: &bv_timeline::Clip, media: Option<&bv_timeline::MediaInfo>) -> bv_core::TimeRange {
    let start = c.source_in.max(bv_core::Tick::ZERO);
    let end = c.source_in + c.to_source(c.duration);
    let end = media.map_or(end, |m| if m.still { end } else { end.min(m.duration) });
    bv_core::TimeRange::from_bounds(start, end.max(start))
}

/// An event of a layered Photoshop document: its Stream menu lists the
/// composite and each layer (VEGAS's Help); picking one makes the event
/// show it. `false` for other media.
fn psd_stream_items(
    ui: &mut Ui,
    app: &mut BananaApp,
    clip: ClipId,
    m: &bv_timeline::MediaInfo,
) -> bool {
    let bv_timeline::MediaSource::File { path } = &m.source else {
        return false;
    };
    let names = bv_engine::psd_layers(path);
    if names.is_empty() {
        return false;
    }
    // The document's composite: a media of it without a layer.
    let composite = app
        .project()
        .media
        .iter()
        .find(|o| o.source == m.source && o.layer.is_none())
        .map(|o| o.id);
    if check_item(ui, tr("Composite"), m.layer.is_none(), true, "")
        && let Some(id) = composite
    {
        app.modify(|p| {
            if let Some(c) = p.clip_mut(clip) {
                c.media = id;
            }
        });
    }
    let base = app
        .project()
        .media
        .iter()
        .find(|o| o.source == m.source && o.layer.is_none())
        .cloned()
        .unwrap_or_else(|| m.clone());
    for (k, name) in names.iter().enumerate().rev() {
        let on = m.layer == Some(k as u16);
        if check_item(ui, name, on, true, "") && !on {
            let id = app.psd_layer_of(&base, k as u16, name);
            app.modify(|p| {
                if let Some(c) = p.clip_mut(clip) {
                    c.media = id;
                }
            });
        }
    }
    true
}
