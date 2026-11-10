//! Main menu bar and toolbar. Menu items come from [`crate::commands`], so
//! the label, shortcut and state shown here are the ones the keyboard uses.

use crate::i18n::tr;
use bv_timeline::{ChannelMap, EnvelopeKind, RippleScope};

use crate::app::actions::LayoutPreset;
use crate::app::more_actions::Script;
use crate::app::{BananaApp, Pending, Tool};
use crate::commands::Cmd;
use crate::dock::Tab;
use crate::egui::{self, Atom, Button, Id, Ui, vec2};
use crate::icons::{self, Icon};
use crate::theme;
use crate::timefmt::{GridSpacing, RulerFormat};

const ICON: f32 = 16.0;

/// A menu item for `c`, with its icon (or check mark) column and shortcut.
pub(crate) fn cmd(ui: &mut Ui, app: &mut BananaApp, c: Cmd) {
    let label = c.label(app);
    cmd_as(ui, app, c, &label);
}

/// [`cmd`] under another label: a shortcut menu's wording ("Insert Video
/// Track" where the Insert menu says "Video Track").
pub(crate) fn cmd_as(ui: &mut Ui, app: &mut BananaApp, c: Cmd, label: &str) {
    let enabled = c.enabled(app);
    let checked = c.checked(app);
    let id = Id::new(("menu-icone", format!("{c:?}")));
    // The keys of the keyboard map in use (two-key ones too: "Alt+D, A").
    let shortcut = app.keymap.text_for(crate::keymap::Action::Cmd(c));
    let button = Button::new((Atom::custom(id, vec2(ICON, ICON)), label)).shortcut_text(shortcut);
    let resp = ui.add_enabled_ui(enabled, |ui| button.atom_ui(ui)).inner;
    if let Some(r) = resp.rect(id) {
        match (checked, c.icon()) {
            (Some(on), _) => crate::widgets::paint_check_box(ui, r, on, c.is_radio(), enabled),
            (_, Some(icon)) => {
                let color = if !enabled {
                    theme::text_disabled()
                } else if matches!(c, Cmd::Delete) {
                    theme::error()
                } else {
                    theme::text()
                };
                icons::paint(ui.painter(), r.shrink(1.0), icon, color);
            }
            _ => {}
        }
    }
    if resp.response.clicked() {
        c.run(app);
    }
}

/// A menu item (lined up with the others' icon column); clicked or not.
pub(crate) fn item(ui: &mut Ui, label: &str) -> bool {
    let id = Id::new(("menu-item", label));
    Button::new((Atom::custom(id, vec2(ICON, ICON)), label))
        .atom_ui(ui)
        .response
        .clicked()
}

/// A menu item with `icon` in the icons' column (or none) and its keys,
/// greyed out unless `enabled`; clicked or not.
pub(crate) fn icon_item(
    ui: &mut Ui,
    label: &str,
    icon: Option<Icon>,
    keys: &str,
    enabled: bool,
) -> bool {
    let id = Id::new(("menu-icon-item", label));
    let button = Button::new((Atom::custom(id, vec2(ICON, ICON)), label)).shortcut_text(keys);
    let resp = ui.add_enabled_ui(enabled, |ui| button.atom_ui(ui)).inner;
    if let (Some(r), Some(icon)) = (resp.rect(id), icon) {
        let color = if enabled {
            theme::text()
        } else {
            theme::text_disabled()
        };
        icons::paint(ui.painter(), r.shrink(1.0), icon, color);
    }
    resp.response.clicked()
}

/// A menu item lined up with the others (their icons' column), greyed out
/// unless `enabled`.
pub(crate) fn item_enabled(ui: &mut Ui, label: &str, enabled: bool) -> egui::Response {
    let id = Id::new(("menu-item-enabled", label));
    let button = Button::new((Atom::custom(id, vec2(ICON, ICON)), label));
    ui.add_enabled_ui(enabled, |ui| button.atom_ui(ui))
        .inner
        .response
}

/// A menu's default item, in bold as VEGAS's; clicked or not.
pub(crate) fn strong_item(ui: &mut Ui, label: &str) -> bool {
    let id = Id::new(("menu-item-strong", label));
    Button::new((
        Atom::custom(id, vec2(ICON, ICON)),
        egui::RichText::new(label).strong(),
    ))
    .atom_ui(ui)
    .response
    .clicked()
}

/// A switch as VEGAS's menus show it: a check mark in the icons' column
/// while it is on, nothing while it is off; clicked or not.
pub(crate) fn mark_item(ui: &mut Ui, label: &str, on: bool) -> bool {
    let id = Id::new(("menu-mark", label));
    let resp = Button::new((Atom::custom(id, vec2(ICON, ICON)), label)).atom_ui(ui);
    if on && let Some(r) = resp.rect(id) {
        let c = r.center();
        ui.painter().add(egui::Shape::line(
            vec![
                egui::pos2(c.x - 4.5, c.y),
                egui::pos2(c.x - 1.5, c.y + 3.0),
                egui::pos2(c.x + 4.5, c.y - 3.5),
            ],
            egui::Stroke::new(1.8, theme::text()),
        ));
    }
    resp.response.clicked()
}

/// A menu item with a check box showing `on`; clicked or not.
pub(crate) fn toggle_item(ui: &mut Ui, label: &str, on: bool) -> bool {
    let id = Id::new(("menu-toggle", label));
    let resp = Button::new((Atom::custom(id, vec2(ICON, ICON)), label)).atom_ui(ui);
    if let Some(r) = resp.rect(id) {
        crate::widgets::paint_check_box(ui, r, on, false, true);
    }
    resp.response.clicked()
}

/// A menu item with a check box (or a radio button) showing `on` and its
/// keys; clicked or not.
pub(crate) fn check_item(ui: &mut Ui, label: &str, on: bool, radio: bool, keys: &str) -> bool {
    let id = ui.make_persistent_id(("menu-check", label));
    let resp = Button::new((Atom::custom(id, vec2(ICON, ICON)), label))
        .shortcut_text(keys)
        .atom_ui(ui);
    if let Some(r) = resp.rect(id) {
        crate::widgets::paint_check_box(ui, r, on, radio, true);
    }
    resp.response.clicked()
}

/// A menu item with its keys; clicked or not.
pub(crate) fn keyed_item(ui: &mut Ui, label: &str, keys: &str) -> bool {
    let id = ui.make_persistent_id(("menu-keyed", label));
    Button::new((Atom::custom(id, vec2(ICON, ICON)), label))
        .shortcut_text(keys)
        .atom_ui(ui)
        .response
        .clicked()
}

/// A submenu with a radio button showing `on` in the items' column.
pub(crate) fn radio_sub(ui: &mut Ui, label: &str, on: bool, content: impl FnOnce(&mut Ui)) {
    let id = ui.make_persistent_id(("radio-submenu", label));
    let button = Button::new((Atom::custom(id, vec2(ICON, ICON)), label))
        .right_text(egui::containers::menu::SubMenuButton::RIGHT_ARROW);
    let (resp, _) = egui::containers::menu::SubMenuButton::from_button(button).ui(ui, content);
    // The column of the radio buttons: the button's padding in.
    let left = resp.rect.left() + ui.spacing().button_padding.x;
    let r = egui::Rect::from_min_size(
        egui::pos2(left, resp.rect.center().y - ICON / 2.0),
        vec2(ICON, ICON),
    );
    crate::widgets::paint_check_box(ui, r, on, true, true);
}

/// A greyed-out item for a feature that is not available yet.
pub(crate) fn soon(ui: &mut Ui, label: &str, shortcut: &str) {
    let id = Id::new(("menu-em-breve", label));
    ui.add_enabled_ui(false, |ui| {
        Button::new((Atom::custom(id, vec2(ICON, ICON)), label))
            .shortcut_text(shortcut)
            .atom_ui(ui)
    })
    .response
    .on_disabled_hover_text(tr("Coming soon to BananaV"));
}

/// A greyed-out toggle item for a feature that is not available yet (the
/// convention CLAUDE.md names, kept for the next one: every toggle that
/// used it now works).
#[allow(dead_code)]
pub(crate) fn soon_toggle(ui: &mut Ui, label: &str, shortcut: &str) {
    let id = Id::new(("menu-em-breve", label));
    let resp = ui.add_enabled_ui(false, |ui| {
        Button::new((Atom::custom(id, vec2(ICON, ICON)), label))
            .shortcut_text(shortcut)
            .atom_ui(ui)
    });
    if let Some(r) = resp.inner.rect(id) {
        crate::widgets::paint_check_box(ui, r, false, false, false);
    }
    resp.response
        .on_disabled_hover_text(tr("Coming soon to BananaV"));
}

/// A submenu whose label lines up with the items (empty icon column).
pub(crate) fn sub(ui: &mut Ui, label: &str, enabled: bool, content: impl FnOnce(&mut Ui)) {
    let id = Id::new(("submenu", label));
    let button = Button::new((Atom::custom(id, vec2(ICON, ICON)), label))
        .right_text(egui::containers::menu::SubMenuButton::RIGHT_ARROW);
    ui.add_enabled_ui(enabled, |ui| {
        egui::containers::menu::SubMenuButton::from_button(button).ui(ui, content);
    });
}

fn file_menu(ui: &mut Ui, app: &mut BananaApp) {
    cmd(ui, app, Cmd::New);
    cmd(ui, app, Cmd::Welcome);
    cmd(ui, app, Cmd::Open);
    cmd(ui, app, Cmd::Close);
    ui.separator();
    cmd(ui, app, Cmd::Save);
    cmd(ui, app, Cmd::SaveAs);
    cmd(ui, app, Cmd::CopyMediaWithProject);
    cmd(ui, app, Cmd::SaveIncremental);
    ui.separator();
    cmd(ui, app, Cmd::Render);
    cmd(ui, app, Cmd::RealTimeRender);
    ui.separator();
    sub(ui, tr("Import"), true, |ui| {
        cmd(ui, app, Cmd::ImportMedia);
        cmd(ui, app, Cmd::ImportImageSequence);
        cmd(ui, app, Cmd::ImportProjectMedia);
        soon(ui, "Premiere/After Effects (*.prproj)…", "");
        cmd(ui, app, Cmd::ImportXml);
        cmd(ui, app, Cmd::ImportFcpxml);
        cmd(ui, app, Cmd::ImportEdl);
        cmd(ui, app, Cmd::ImportOtio);
        cmd(ui, app, Cmd::ImportVeg);
        cmd(ui, app, Cmd::ImportDvdCamcorder);
        cmd(ui, app, Cmd::ImportBwf);
        cmd(ui, app, Cmd::ImportClosedCaptioning);
    });
    sub(ui, tr("Export"), true, |ui| {
        cmd(ui, app, Cmd::ExportProject);
        cmd(ui, app, Cmd::ExportArchive);
        soon(ui, "Premiere/After Effects (*.prproj)…", "");
        cmd(ui, app, Cmd::ExportXml);
        cmd(ui, app, Cmd::ExportFcpxml);
        cmd(ui, app, Cmd::ExportEdl);
        cmd(ui, app, Cmd::ExportOtio);
        cmd(ui, app, Cmd::ExportVeg);
    });
    if item(ui, tr("Screen Capture…")) {
        app.open_screen_capture();
    }
    soon(ui, tr("Extract Audio from CD…"), "");
    ui.separator();
    cmd(ui, app, Cmd::Properties);
    let prefs = &app.settings.prefs;
    if prefs.recent_list && !app.settings.recent.is_empty() {
        let count = prefs.recent_count as usize;
        ui.separator();
        for (i, path) in app
            .settings
            .recent
            .clone()
            .into_iter()
            .take(count)
            .enumerate()
        {
            let name = path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().to_string(),
            );
            let id = Id::new(("recente", i));
            let r = Button::new((
                Atom::custom(id, vec2(ICON, ICON)),
                format!("{} {name}", i + 1),
            ))
            .atom_ui(ui)
            .response
            .on_hover_text(path.display().to_string());
            if r.clicked() {
                app.request(Pending::OpenPath(path));
            }
        }
    }
    ui.separator();
    cmd(ui, app, Cmd::Exit);
}

fn edit_menu(ui: &mut Ui, app: &mut BananaApp) {
    cmd(ui, app, Cmd::Undo);
    cmd(ui, app, Cmd::Redo);
    ui.separator();
    cmd(ui, app, Cmd::Cut);
    cmd(ui, app, Cmd::Copy);
    cmd(ui, app, Cmd::Paste);
    cmd(ui, app, Cmd::PasteRepeat);
    cmd(ui, app, Cmd::PasteInsert);
    cmd(ui, app, Cmd::PasteAttributes);
    cmd(ui, app, Cmd::PasteAttributesSelective);
    ui.separator();
    cmd(ui, app, Cmd::Delete);
    cmd(ui, app, Cmd::Trim);
    cmd(ui, app, Cmd::TrimStart);
    cmd(ui, app, Cmd::TrimEnd);
    cmd(ui, app, Cmd::Split);
    cmd(ui, app, Cmd::SmartSplit);
    cmd(ui, app, Cmd::Rejoin);
    cmd(ui, app, Cmd::CloseGaps);
    ui.separator();
    cmd(ui, app, Cmd::ResizeSelectedAdjustments);
    cmd(ui, app, Cmd::ResizeAllAdjustments);
    ui.separator();
    sub(ui, tr("Navigate"), true, |ui| {
        for c in [
            Cmd::GoTo,
            Cmd::ProjectStart,
            Cmd::ProjectEnd,
            Cmd::SelectionStartGo,
            Cmd::SelectionEndGo,
            Cmd::GridLeft,
            Cmd::GridRight,
            Cmd::PixelLeft,
            Cmd::PixelRight,
            Cmd::FrameLeft,
            Cmd::FrameRight,
        ] {
            cmd(ui, app, c);
        }
        ui.separator();
        for c in [Cmd::MarkerLeft, Cmd::MarkerRight, Cmd::SelectToMarker] {
            cmd(ui, app, c);
        }
        ui.separator();
        for c in [
            Cmd::EditPointLeft,
            Cmd::EditPointRight,
            Cmd::SelectedEventLeft,
            Cmd::SelectedEventRight,
        ] {
            cmd(ui, app, c);
        }
        ui.separator();
        cmd(ui, app, Cmd::CursorToPlayhead);
        cmd(ui, app, Cmd::CenterOnCursor);
    });
    sub(ui, tr("Post-Edit Ripple"), true, |ui| {
        cmd(ui, app, Cmd::Ripple(RippleScope::AffectedTracks));
        cmd(ui, app, Cmd::Ripple(RippleScope::AffectedTracksAndMarkers));
        cmd(ui, app, Cmd::Ripple(RippleScope::All));
    });
    sub(ui, tr("Select"), true, |ui| {
        cmd(ui, app, Cmd::SelectAll);
        cmd(ui, app, Cmd::SelectAfterCursor);
        ui.separator();
        cmd(ui, app, Cmd::SelectEventStart);
        cmd(ui, app, Cmd::SelectEventEnd);
        cmd(ui, app, Cmd::ExpandedEditMode);
        cmd(ui, app, Cmd::ExitEditMode);
        ui.separator();
        cmd(ui, app, Cmd::LoopPlayback);
        ui.separator();
        soon(ui, tr("Zoom Selection"), "");
        cmd(ui, app, Cmd::SelectionToView);
        cmd(ui, app, Cmd::SelectionToProject);
        cmd(ui, app, Cmd::SelectLoopRegion);
        ui.separator();
        cmd(ui, app, Cmd::SetSelectionStart);
        cmd(ui, app, Cmd::SetSelectionEnd);
        cmd(ui, app, Cmd::SelectionPrevEvent);
        cmd(ui, app, Cmd::SelectionNextEvent);
        cmd(ui, app, Cmd::RestoreSelection);
    });
    sub(ui, tr("Editing Tool"), true, |ui| {
        cmd(ui, app, Cmd::UseTool(Tool::Edit));
        cmd(ui, app, Cmd::UseTool(Tool::Shuffle));
        for t in [Tool::Slip, Tool::Slide, Tool::Stretch, Tool::SplitTrim] {
            cmd(ui, app, Cmd::UseTool(t));
        }
        ui.separator();
        for t in [Tool::Envelope, Tool::Select, Tool::Zoom] {
            cmd(ui, app, Cmd::UseTool(t));
        }
        ui.separator();
        cmd(ui, app, Cmd::NextTool);
        cmd(ui, app, Cmd::PrevTool);
    });
    sub(ui, tr("Extensions"), false, |_| {});
    ui.separator();
    sub(ui, tr("Switches"), true, |ui| {
        switches_items(ui, app, None, false)
    });
    sub(ui, tr("Take"), true, |ui| {
        let clip = app.primary_clip();
        take_items(ui, app, clip);
    });
    sub(ui, tr("Group"), true, |ui| group_items(ui, app));
    sub(ui, tr("Stream"), false, |_| {});
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
    ui.separator();
    cmd(ui, app, Cmd::UndoAll);
    cmd(ui, app, Cmd::ClearHistory);
}

/// The Switches: Edit › Switches (`kind` `None`), or an event's shortcut
/// menu's (`event`: its wording) for its kind.
pub(crate) fn switches_items(
    ui: &mut Ui,
    app: &mut BananaApp,
    kind: Option<bv_timeline::TrackKind>,
    event: bool,
) {
    use bv_timeline::TrackKind;
    let video = kind != Some(TrackKind::Audio);
    let audio = kind != Some(TrackKind::Video);
    cmd(ui, app, Cmd::Mute);
    cmd(ui, app, Cmd::Lock);
    ui.separator();
    if event {
        // One of three past the media's end, as VEGAS's event menu shows.
        let looped = app.all_selected(None, |c| c.looped).unwrap_or(false);
        if video {
            let hold = !looped
                && app
                    .all_selected(Some(TrackKind::Video), |c| c.hold_last)
                    .unwrap_or(false);
            if check_item(ui, tr("Hold last frame"), hold, true, "") {
                app.set_event_end(false);
            }
        }
        if check_item(ui, tr("Loop event"), looped, true, "") {
            app.set_event_end(true);
        }
        if video {
            let all = app
                .all_selected(Some(TrackKind::Video), |c| c.all_frames)
                .unwrap_or(false);
            if check_item(ui, tr("Trim event to include all frames"), all, true, "") {
                app.set_all_frames(true);
            }
        }
    } else {
        cmd(ui, app, Cmd::HoldLastFrame);
        cmd(ui, app, Cmd::Loop);
        cmd(ui, app, Cmd::AllFrames);
    }
    if audio {
        ui.separator();
        cmd(ui, app, Cmd::InvertPhase);
        cmd(ui, app, Cmd::Normalize);
        cmd(ui, app, Cmd::AutoNormalize);
    }
    if video {
        ui.separator();
        cmd(ui, app, Cmd::MaintainAspect);
        cmd(ui, app, Cmd::ReduceFlicker);
        ui.separator();
        cmd(ui, app, Cmd::Resample(None));
        cmd(
            ui,
            app,
            Cmd::Resample(Some(bv_timeline::ResampleMode::FrameBlend)),
        );
        cmd(
            ui,
            app,
            Cmd::Resample(Some(bv_timeline::ResampleMode::OpticalFlow)),
        );
        cmd(
            ui,
            app,
            Cmd::Resample(Some(bv_timeline::ResampleMode::Disable)),
        );
    }
}

/// The Take submenu: VEGAS's take commands, then event `clip`'s takes, the
/// active one checked (a click plays another).
pub(crate) fn take_items(ui: &mut Ui, app: &mut BananaApp, clip: Option<bv_timeline::ClipId>) {
    cmd(ui, app, Cmd::TakeRenameActive);
    cmd(ui, app, Cmd::TakeChooseActive);
    ui.separator();
    cmd(ui, app, Cmd::TakeNext);
    cmd(ui, app, Cmd::TakePrevious);
    ui.separator();
    cmd(ui, app, Cmd::TakeDeleteActive);
    cmd(ui, app, Cmd::TakeDelete);
    let Some(clip) = clip else {
        return;
    };
    let (names, active) = crate::takes::take_names(app, clip);
    if names.is_empty() {
        return;
    }
    ui.separator();
    for (k, name) in names.iter().enumerate() {
        if check_item(ui, name, k == active, true, "") {
            app.choose_take(clip, k);
            ui.close();
        }
    }
}

/// The Group submenu.
pub(crate) fn group_items(ui: &mut Ui, app: &mut BananaApp) {
    cmd(ui, app, Cmd::GroupNew);
    ui.separator();
    cmd(ui, app, Cmd::GroupRemove);
    cmd(ui, app, Cmd::GroupClear);
    cmd(ui, app, Cmd::GroupSelectAll);
    ui.separator();
    cmd(ui, app, Cmd::GroupCut);
    cmd(ui, app, Cmd::GroupCopy);
    cmd(ui, app, Cmd::GroupDelete);
}

fn view_menu(ui: &mut Ui, app: &mut BananaApp) {
    cmd(ui, app, Cmd::Toolbar);
    cmd(ui, app, Cmd::StatusBar);
    ui.separator();
    sub(ui, tr("Window Layouts"), true, |ui| {
        cmd(ui, app, Cmd::OrganizeLayouts);
        cmd(ui, app, Cmd::SaveLayout);
        cmd(ui, app, Cmd::SaveLayoutAs);
        cmd(ui, app, Cmd::ReloadLayout);
        ui.separator();
        for p in LayoutPreset::ALL {
            cmd(ui, app, Cmd::Layout(p));
        }
        ui.separator();
        for n in 0..10 {
            cmd(ui, app, Cmd::LayoutSlot(n));
        }
    });
    ui.separator();
    cmd(ui, app, Cmd::FocusTimeline);
    sub(ui, tr("Window"), true, |ui| {
        for tab in [
            Tab::Explorer,
            Tab::Trimmer,
            Tab::Master,
            Tab::Preview,
            Tab::ProjectMedia,
            Tab::Notes,
            Tab::Transitions,
            Tab::VideoFx,
            Tab::Generators,
        ] {
            cmd(ui, app, Cmd::Window(tab));
        }
        cmd(ui, app, Cmd::Window(Tab::Compositors));
        cmd(ui, app, Cmd::Window(Tab::PluginManager));
        cmd(ui, app, Cmd::Window(Tab::Scopes));
        cmd(ui, app, Cmd::Window(Tab::SurroundPanner));
        cmd(ui, app, Cmd::Window(Tab::EditDetails));
        cmd(ui, app, Cmd::Window(Tab::MixingConsole));
        cmd(ui, app, Cmd::DeviceExplorer);
        cmd(ui, app, Cmd::Window(Tab::LoudnessMeters));
        cmd(ui, app, Cmd::Window(Tab::ColorGrading));
        cmd(ui, app, Cmd::Window(Tab::Properties));
        cmd(ui, app, Cmd::Window(Tab::Transform));
    });
    sub(ui, tr("Extensions"), false, |_| {});
    ui.separator();
    cmd(ui, app, Cmd::PreviewFader);
    ui.separator();
    cmd(ui, app, Cmd::AudioBusTracks);
    cmd(ui, app, Cmd::VideoBusTrack);
    ui.separator();
    sub(ui, tr("Event Media Markers"), true, |ui| {
        cmd(ui, app, Cmd::EventMediaMarkers);
        cmd(ui, app, Cmd::EventMediaMarkerLabels);
    });
    cmd(ui, app, Cmd::ActiveTakeInfo);
    cmd(ui, app, Cmd::Waveforms);
    cmd(ui, app, Cmd::EventButtons);
    cmd(ui, app, Cmd::FadeDurations);
    cmd(ui, app, Cmd::EdgeFrames);
    cmd(ui, app, Cmd::EventHeaders);
    cmd(ui, app, Cmd::EventDuration);
    ui.separator();
    sub(ui, tr("Audio Envelopes"), true, |ui| {
        cmd(ui, app, Cmd::ShowEnvelopeType(EnvelopeKind::Mute));
        cmd(ui, app, Cmd::ShowEnvelopeType(EnvelopeKind::Volume));
        cmd(ui, app, Cmd::ShowEnvelopeType(EnvelopeKind::Pan));
        cmd(ui, app, Cmd::ShowSurroundKeyframes);
    });
    sub(ui, tr("Video Envelopes"), true, |ui| {
        cmd(ui, app, Cmd::ShowEnvelopeType(EnvelopeKind::Mute));
        cmd(ui, app, Cmd::ShowEnvelopeType(EnvelopeKind::CompositeLevel));
        cmd(ui, app, Cmd::ShowEnvelopeType(EnvelopeKind::FadeToColor));
        cmd(ui, app, Cmd::ShowTrackKeyframes);
        cmd(ui, app, Cmd::ShowEnvelopeType(EnvelopeKind::MotionBlur));
        cmd(ui, app, Cmd::ShowEnvelopeType(EnvelopeKind::Supersampling));
        ui.separator();
        cmd(ui, app, Cmd::ShowEnvelopeType(EnvelopeKind::Velocity));
        cmd(
            ui,
            app,
            Cmd::ShowEnvelopeType(EnvelopeKind::TransitionProgress),
        );
    });
    cmd(ui, app, Cmd::ShowEnvelopes);
    cmd(ui, app, Cmd::ShowOfxEnvelopes);
    ui.separator();
    cmd(ui, app, Cmd::MinimizeTracks);
    cmd(ui, app, Cmd::RebuildPeaks);
}

/// The menus, in the title bar (`title_bar.rs`), spaced as VEGAS's; the
/// right edge of the last.
pub fn menu_bar(ui: &mut Ui, app: &mut BananaApp) -> f32 {
    let style = |style: &mut egui::Style| {
        egui::containers::menu::menu_style(style);
        style.spacing.button_padding = vec2(5.0, 2.0);
    };
    egui::MenuBar::new()
        .style(style)
        .ui(ui, |ui| {
            ui.menu_button(tr("File"), |ui| file_menu(ui, app));
            ui.menu_button(tr("Edit"), |ui| edit_menu(ui, app));
            ui.menu_button(tr("View"), |ui| view_menu(ui, app));
            ui.menu_button(tr("Insert"), |ui| insert_menu(ui, app));
            ui.menu_button(tr("Tools"), |ui| tools_menu(ui, app));
            ui.menu_button(tr("Options"), |ui| options_menu(ui, app));
            ui.menu_button(tr("Help"), |ui| help_menu(ui, app))
                .response
                .rect
                .right()
        })
        .inner
}

fn insert_menu(ui: &mut Ui, app: &mut BananaApp) {
    sub(ui, tr("Audio Envelopes"), true, |ui| {
        cmd(ui, app, Cmd::AudioEnvelope(EnvelopeKind::Mute));
        cmd(ui, app, Cmd::AudioEnvelope(EnvelopeKind::Volume));
        cmd(ui, app, Cmd::AudioEnvelope(EnvelopeKind::Pan));
        cmd(ui, app, Cmd::SurroundKeyframes);
        ui.separator();
        cmd(ui, app, Cmd::FxAutomation);
    });
    sub(ui, tr("Video Envelopes"), true, |ui| {
        cmd(ui, app, Cmd::VideoMuteEnvelope);
        cmd(ui, app, Cmd::VideoEnvelope(EnvelopeKind::CompositeLevel));
        cmd(ui, app, Cmd::VideoEnvelope(EnvelopeKind::FadeToColor));
        ui.separator();
        cmd(ui, app, Cmd::VideoEnvelope(EnvelopeKind::Velocity));
        cmd(
            ui,
            app,
            Cmd::VideoEnvelope(EnvelopeKind::TransitionProgress),
        );
    });
    ui.separator();
    cmd(ui, app, Cmd::InsertAudioTrack);
    cmd(ui, app, Cmd::InsertVideoTrack);
    cmd(ui, app, Cmd::InsertAdjustmentTrack);
    ui.separator();
    cmd(ui, app, Cmd::InsertAudioBus);
    cmd(ui, app, Cmd::InsertInputBus);
    cmd(ui, app, Cmd::InsertAssignableFx);
    ui.separator();
    cmd(ui, app, Cmd::SubtitlesFromFile);
    cmd(ui, app, Cmd::SubtitlesFromRegions);
    cmd(ui, app, Cmd::AddMissingStream);
    cmd(ui, app, Cmd::EmptyEvent);
    cmd(ui, app, Cmd::InsertAdjustmentEvent);
    cmd(ui, app, Cmd::InsertTextMedia);
    let id = Id::new("menu-midia-gerada");
    if Button::new((Atom::custom(id, vec2(ICON, ICON)), tr("Generated Media…")))
        .atom_ui(ui)
        .response
        .clicked()
    {
        app.show_tab(Tab::Generators, false);
    }
    cmd(ui, app, Cmd::Slideshow);
    ui.separator();
    cmd(ui, app, Cmd::InsertTime);
    ui.separator();
    cmd(ui, app, Cmd::InsertMarker);
    cmd(ui, app, Cmd::InsertRegion);
    ui.separator();
    cmd(ui, app, Cmd::InsertCdTrack);
    cmd(ui, app, Cmd::InsertCdIndex);
    ui.separator();
    cmd(ui, app, Cmd::InsertCommand);
}

fn tools_menu(ui: &mut Ui, app: &mut BananaApp) {
    sub(ui, tr("Audio"), true, |ui| {
        cmd(ui, app, Cmd::OpenInAudioEditor);
        cmd(ui, app, Cmd::OpenCopyInAudioEditor);
        cmd(ui, app, Cmd::EditSourceProject);
        ui.separator();
        cmd(ui, app, Cmd::AudioEventFx);
        cmd(ui, app, Cmd::NonRealTimeFx);
        ui.separator();
        cmd(ui, app, Cmd::AudioTrackFx);
        ui.separator();
        // The project's assignable effects chains, one to delete.
        let chains: Vec<_> = app.project().fx_chains.iter().map(|c| c.id).collect();
        sub(ui, tr("Remove Assignable FX"), !chains.is_empty(), |ui| {
            for id in chains {
                let name = crate::i18n::chain_name(app.project(), id);
                if ui.button(name).clicked() {
                    app.modify(|p| p.remove_fx_chain(id));
                    ui.close();
                }
            }
        });
    });
    sub(ui, tr("Video"), true, |ui| {
        cmd(ui, app, Cmd::MediaFx);
        ui.separator();
        cmd(ui, app, Cmd::PanCropEvent);
        cmd(ui, app, Cmd::VideoEventFx);
        cmd(ui, app, Cmd::MotionTracking);
        ui.separator();
        cmd(ui, app, Cmd::VideoTrackFx);
        cmd(ui, app, Cmd::TrackMotion);
        ui.separator();
        cmd(ui, app, Cmd::VideoOutputFx);
    });
    ui.separator();
    cmd(ui, app, Cmd::RenderToNewTrack);
    cmd(ui, app, Cmd::PrerenderVideo);
    cmd(ui, app, Cmd::CleanUpPrerender);
    cmd(ui, app, Cmd::BuildRamPreview);
    cmd(ui, app, Cmd::ClearRamPreview);
    ui.separator();
    soon(ui, tr("AI Auto Reframe"), "");
    ui.separator();
    cmd(ui, app, Cmd::LayOutCd);
    sub(ui, tr("Burn Disc"), true, |ui| {
        cmd(ui, app, Cmd::BurnTaoCd);
        cmd(ui, app, Cmd::BurnDaoCd);
        cmd(ui, app, Cmd::BurnDvd);
    });
    cmd(ui, app, Cmd::LoudnessLog);
    ui.separator();
    cmd(ui, app, Cmd::AdvancedSave);
    ui.separator();
    sub(ui, "Scripting", true, |ui| {
        soon(ui, tr("Run Script…"), "");
        soon(ui, tr("Rescan Script Menu Folder"), "");
        ui.separator();
        cmd(ui, app, Cmd::RunScript(Script::ExportChapters));
        cmd(ui, app, Cmd::RunScript(Script::ExportCaptionsDvd));
        cmd(ui, app, Cmd::RunScript(Script::ExportCaptionsQuickTime));
        cmd(ui, app, Cmd::RunScript(Script::ExportCaptionsSami));
        cmd(ui, app, Cmd::RunScript(Script::ExportCaptionsSubRip));
        cmd(ui, app, Cmd::ExportEdl);
        cmd(ui, app, Cmd::RunScript(Script::ExportRegionsAsSubtitles));
        cmd(ui, app, Cmd::RunScript(Script::GroupVideoAndAudio));
        cmd(ui, app, Cmd::RunScript(Script::PromoteMediaCaptions));
        cmd(ui, app, Cmd::RunScript(Script::RemoveLetterboxing));
        cmd(ui, app, Cmd::RunScript(Script::StereoToMono));
    });
    sub(ui, tr("Extensions"), false, |_| {});
    sub(ui, tr("Multicamera"), true, |ui| {
        cmd(ui, app, Cmd::MulticamSyncAudio);
        ui.separator();
        cmd(ui, app, Cmd::MulticamSyncTimecode);
        cmd(ui, app, Cmd::MulticamSyncDate);
        cmd(ui, app, Cmd::MulticamCreate);
        cmd(ui, app, Cmd::MulticamExpand);
        ui.separator();
        cmd(ui, app, Cmd::MulticamEditing);
        cmd(ui, app, Cmd::MulticamAudioWithVideo);
        cmd(ui, app, Cmd::MulticamFullFrame);
    });
    ui.separator();
    cmd(ui, app, Cmd::CleanProjectMedia);
    // Nested projects' sound is mixed live here: this reads them again.
    let nested = app.has_nested_projects();
    if ui
        .add_enabled_ui(nested, |ui| item(ui, tr("Rebuild Offline Nested Audio")))
        .inner
    {
        app.rebuild_nested_audio();
    }
}

fn options_menu(ui: &mut Ui, app: &mut BananaApp) {
    sub(ui, tr("Frame Quantization"), true, |ui| {
        cmd(ui, app, Cmd::Quantize);
        cmd(ui, app, Cmd::ShowUnquantized);
    });
    ui.separator();
    cmd(ui, app, Cmd::Snapping);
    cmd(ui, app, Cmd::SnapGrid);
    cmd(ui, app, Cmd::SnapMarkers);
    cmd(ui, app, Cmd::SnapEvents);
    ui.separator();
    sub(ui, tr("Grid Spacing"), true, |ui| {
        cmd(ui, app, Cmd::Grid(GridSpacing::RulerMarks));
        ui.separator();
        for g in [
            GridSpacing::Minutes,
            GridSpacing::Seconds,
            GridSpacing::Milliseconds,
            GridSpacing::Frames,
        ] {
            cmd(ui, app, Cmd::Grid(g));
        }
        ui.separator();
        for g in GridSpacing::MUSICAL {
            cmd(ui, app, Cmd::Grid(g));
        }
    });
    sub(ui, tr("Ruler Time Format"), true, |ui| {
        for r in RulerFormat::ALL {
            cmd(ui, app, Cmd::Ruler(r));
        }
        ui.separator();
        cmd(ui, app, Cmd::SetTimeAtCursor);
        cmd(ui, app, Cmd::SetProjectTime);
        cmd(ui, app, Cmd::SetProjectTempo);
    });
    ui.separator();
    cmd(ui, app, Cmd::AutoCrossfade);
    cmd(ui, app, Cmd::QuickFades);
    sub(ui, tr("Ripple"), true, |ui| {
        cmd(ui, app, Cmd::AutoRipple);
        ui.separator();
        cmd(ui, app, Cmd::RippleMode(RippleScope::AffectedTracks));
        cmd(
            ui,
            app,
            Cmd::RippleMode(RippleScope::AffectedTracksAndMarkers),
        );
        cmd(ui, app, Cmd::RippleMode(RippleScope::All));
    });
    cmd(ui, app, Cmd::LockEnvelopes);
    cmd(ui, app, Cmd::IgnoreGrouping);
    cmd(ui, app, Cmd::SlipAllTakes);
    ui.separator();
    cmd(ui, app, Cmd::LoopPlayback);
    cmd(ui, app, Cmd::Metronome);
    sub(ui, tr("Loudness Meters"), true, |ui| {
        crate::tabs::loudness::options_items(ui, app);
    });
    ui.separator();
    cmd(ui, app, Cmd::BypassAudioFx);
    ui.separator();
    cmd(ui, app, Cmd::MuteAllAudio);
    cmd(ui, app, Cmd::MuteAllVideo);
    ui.separator();
    sub(ui, tr("Timecode"), true, |ui| {
        cmd(ui, app, Cmd::GenerateMtc);
        cmd(ui, app, Cmd::GenerateClock);
        cmd(ui, app, Cmd::TriggerMtc);
    });
    ui.separator();
    cmd(ui, app, Cmd::ExternalControl);
    ui.separator();
    cmd(ui, app, Cmd::CustomizeToolbar);
    cmd(ui, app, Cmd::CustomizeTimelineToolbar);
    cmd(ui, app, Cmd::CustomizeKeyboard);
    cmd(ui, app, Cmd::EnvelopeAppearance);
    ui.separator();
    cmd(ui, app, Cmd::ExportPreferences);
    cmd(ui, app, Cmd::ImportPreferences);
    cmd(ui, app, Cmd::Preferences);
    // As VEGAS's: with Shift held as the menu opens, "Internal…" at its
    // end (no command, key or toolbar button has it).
    if shift_at_opening(ui, Id::new("options-menu-shift")) && item(ui, tr("Internal…")) {
        app.menu_dialogs.more.internal = Some(crate::internal_prefs::InternalDialog::new(app));
        ui.close();
    }
}

/// Whether Shift was held when the menu `id` opened: read on its first
/// pass, kept while it stays open (letting Shift go changes nothing).
fn shift_at_opening(ui: &Ui, id: Id) -> bool {
    let pass = ui.ctx().cumulative_pass_nr();
    let kept = ui
        .ctx()
        .data(|d| d.get_temp::<(u64, bool)>(id))
        .filter(|(last, _)| *last + 1 >= pass)
        .map(|(_, shift)| shift);
    let shift = kept.unwrap_or_else(|| ui.input(|i| i.modifiers.shift));
    ui.ctx().data_mut(|d| d.insert_temp(id, (pass, shift)));
    shift
}

fn help_menu(ui: &mut Ui, app: &mut BananaApp) {
    cmd(ui, app, Cmd::HelpContents);
    soon(ui, tr("What's This?"), "Shift+F1");
    cmd(ui, app, Cmd::KeyboardShortcuts);
    soon(ui, tr("Download the BananaV User Manual"), "");
    ui.separator();
    cmd(ui, app, Cmd::CheckUpdates);
    soon(ui, tr("Deactivate Software on This Computer…"), "");
    ui.separator();
    soon(ui, tr("BananaV Home Page"), "");
    soon(ui, tr("BananaV Community…"), "");
    cmd(ui, app, Cmd::About);
    ui.separator();
    let id = Id::new("menu-demo");
    if Button::new((Atom::custom(id, vec2(ICON, ICON)), tr("Open Demo Project")))
        .atom_ui(ui)
        .response
        .clicked()
    {
        app.request(Pending::Demo);
    }
}

pub fn toolbar(ui: &mut Ui, app: &mut BananaApp) {
    // Right-click on the toolbar: customize it, like VEGAS.
    let bg = ui.interact(
        ui.available_rect_before_wrap(),
        Id::new("barra-principal"),
        egui::Sense::click(),
    );
    bg.context_menu(|ui| {
        if ui.button(tr("Customize Toolbar…")).clicked() {
            Cmd::CustomizeToolbar.run(app);
            ui.close();
        }
    });
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        // The buttons of Options › Customize Toolbar.
        crate::toolbar_buttons::paint(ui, app, crate::toolbar_buttons::ToolbarKind::Main);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            icons::paint_logo(
                ui.painter(),
                egui::Rect::from_center_size(
                    ui.cursor().right_center() - egui::vec2(14.0, 0.0),
                    egui::vec2(20.0, 20.0),
                ),
            );
            ui.add_space(26.0);
            ui.label(
                egui::RichText::new("BananaV")
                    .strong()
                    .color(crate::theme::banana()),
            );
        });
    });
}
