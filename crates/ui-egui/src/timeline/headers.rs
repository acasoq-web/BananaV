//! Track headers, laid out like VEGAS's: a colored strip with the More
//! button (⋯), the track number and its kind; the name (scribble strip),
//! the bus button (audio, with busses), Mute and Solo; the level (video)
//! or volume, pan and meter (audio), and the multipurpose slider (audio,
//! with busses or assignable effects chains: a send's level).
//! Double-click a slider: back to its default; double-click its value: type
//! it; Ctrl+drag: fine steps. A click selects the track and gives the keys
//! to the track list.

use crate::i18n::tr;
use bv_timeline::{Output, Project, Send, SendTo, Track, TrackId, TrackKind};

use super::{Layout, TimelineView};
use crate::app::BananaApp;
use crate::automation::TrackControl;
use crate::egui::{self, Align2, Color32, FontId, Key, Pos2, Rect, Sense, Stroke, pos2, vec2};
use crate::menus::{item, item_enabled, keyed_item, mark_item, sub, toggle_item};
use crate::settings::HeaderButton;
use crate::theme;

/// The colored strip (the track's color, Options › Preferences › Display).
pub(super) const STRIP_W: f32 = 22.0;
/// The audio meter on the right edge.
const METER_W: f32 = 26.0;
/// What a header's second row holds as the track list narrows, as VEGAS's:
/// the sliders alone (the audio's volume alone, then with the pan), their
/// values from 200 pixels (the audio's from 230: VEGAS 23 shows none at
/// 215, the user's prints, and the video's level there), their names
/// (Vol:, Pan:, the two sharing the row) from 400.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    Volume,
    Both,
    Values,
    Named,
}

impl Tier {
    fn of(width: f32, kind: TrackKind) -> Tier {
        let values = match kind {
            TrackKind::Video => 200.0,
            TrackKind::Audio => 230.0,
        };
        match width {
            w if w >= 400.0 => Tier::Named,
            w if w >= values => Tier::Values,
            w if w >= 120.0 => Tier::Both,
            _ => Tier::Volume,
        }
    }
}

/// A place for a button in a header: one of Edit Visible Button Set's
/// (Min/Max is two), or one of the track's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Spot {
    Button(HeaderButton),
    Minimize,
    Maximize,
    /// Audio, while the project has busses: where the track plays.
    Bus,
    /// A track minimized alone: Restore Track Height (` brings the others
    /// back, not it).
    Restore,
    /// An armed audio track's Arm for Record, to disarm it, when the set
    /// has no Arm For Record.
    Armed,
}

/// A video header's top row from the right, after Mute, Solo and the
/// track's own buttons: VEGAS 23's, whose row reads Maximize, Minimize,
/// Bypass Motion Blur, Color Grading, Track Motion, Track FX, Automation
/// Settings, Mute and Solo (the user's prints).
const VIDEO_TOP: [Spot; 7] = [
    Spot::Button(HeaderButton::Automation),
    Spot::Button(HeaderButton::TrackFx),
    Spot::Button(HeaderButton::TrackMotion),
    Spot::Button(HeaderButton::ColorGrading),
    Spot::Button(HeaderButton::BypassMotionBlur),
    Spot::Minimize,
    Spot::Maximize,
];
/// An audio header's, after the video's (no print of VEGAS's yet).
const AUDIO_TOP: [Spot; 9] = [
    Spot::Button(HeaderButton::Automation),
    Spot::Button(HeaderButton::TrackFx),
    Spot::Button(HeaderButton::ArmForRecord),
    Spot::Button(HeaderButton::InvertPhase),
    Spot::Button(HeaderButton::RecordInput),
    Spot::Button(HeaderButton::DuckController),
    Spot::Button(HeaderButton::DuckListener),
    Spot::Minimize,
    Spot::Maximize,
];
/// A video header's second row's end from the right: Compositing Mode,
/// Make Compositing Parent and Make Compositing Child, as VEGAS's.
const VIDEO_SECOND: [Spot; 3] = [
    Spot::Button(HeaderButton::MakeChild),
    Spot::Button(HeaderButton::MakeParent),
    Spot::Button(HeaderButton::CompositingMode),
];
/// A top row button's width (Automation Settings' with its menu's arrow)
/// and a second row one's, measured on VEGAS's.
const CELL_W: f32 = 24.0;
const AUTOMATION_W: f32 = 34.0;
const SECOND_W: f32 = 20.0;

/// What decides a header's buttons.
pub(super) struct ButtonsFor<'a> {
    pub kind: TrackKind,
    /// Edit Visible Button Set's for the kind.
    pub set: &'a [HeaderButton],
    /// The track's own, before the set's after Mute.
    pub extras: &'a [Spot],
    /// An adjustment track shows Track FX whatever the set says, as
    /// VEGAS's.
    pub adjustment: bool,
}

impl ButtonsFor<'_> {
    fn wants(&self, s: Spot) -> bool {
        match s {
            Spot::Button(b) => {
                self.set.contains(&b) || (b == HeaderButton::TrackFx && self.adjustment)
            }
            Spot::Minimize | Spot::Maximize => self.set.contains(&HeaderButton::MinMax),
            Spot::Bus | Spot::Restore | Spot::Armed => true,
        }
    }

    /// Where its buttons go, as VEGAS lays them out: the top row's from
    /// `right` leftward, its middle `top`, each where it fits right of
    /// `left` — the first that does not ends the row, so they go from the
    /// left as the list narrows —; a video track's compositing buttons the
    /// same way from the end of its second row (`second`: its middle and
    /// end, when it shows).
    pub fn lay_out(
        &self,
        left: f32,
        right: f32,
        top: f32,
        second: Option<(f32, f32)>,
    ) -> Vec<(Spot, Rect)> {
        let mut out = Vec::new();
        let order: &[Spot] = match self.kind {
            TrackKind::Video => &VIDEO_TOP,
            TrackKind::Audio => &AUDIO_TOP,
        };
        let first = [
            Spot::Button(HeaderButton::Solo),
            Spot::Button(HeaderButton::Mute),
        ];
        let mut x = right;
        for &s in first.iter().chain(self.extras).chain(order) {
            if !self.wants(s) {
                continue;
            }
            let w = if s == Spot::Button(HeaderButton::Automation) {
                AUTOMATION_W
            } else {
                CELL_W
            };
            if x - w < left {
                break;
            }
            out.push((
                s,
                Rect::from_min_max(pos2(x - w, top - 9.5), pos2(x, top + 9.5)),
            ));
            x -= w;
        }
        if let (TrackKind::Video, Some((y, end))) = (self.kind, second) {
            let mut x = end;
            for s in VIDEO_SECOND {
                if !self.wants(s) {
                    continue;
                }
                if x - SECOND_W < left {
                    break;
                }
                out.push((
                    s,
                    Rect::from_min_max(pos2(x - SECOND_W, y - 9.0), pos2(x, y + 9.0)),
                ));
                x -= SECOND_W;
            }
        }
        out
    }
}

pub(super) const MUTE_ON: Color32 = Color32::from_rgb(0xE0, 0x6C, 0x4F);

/// Highest volume of the faders: +12 dB.
pub(super) const MAX_GAIN: f32 = 3.981_072;

/// The name shown in the scribble strip: empty while the track has the
/// name BananaV gave it ("Video 1"), like VEGAS's unnamed tracks.
pub(crate) fn scribble_name(t: &Track) -> String {
    let auto = ["Video ", "Audio "].iter().any(|p| {
        t.name
            .strip_prefix(p)
            .is_some_and(|n| n.parse::<u32>().is_ok())
    });
    if auto { String::new() } else { t.name.clone() }
}

/// A header drag in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TrackDrag {
    /// The header itself: the track moves to where it is dropped.
    Reorder(TrackId),
    /// A track group's header: its tracks move together.
    Group(bv_timeline::TrackGroupId),
    /// The header's bottom edge: the track's height, from `top` (screen y
    /// of the track's top when the drag started) to the pointer.
    Resize { track: TrackId, top: f32 },
}

/// Where a dragged track would land: its index among the project's tracks,
/// the screen y of the gap it would fill and the group whose tracks it
/// would be among (a gap inside a group's rows, the pointer above its
/// bottom). A collapsed group is one place, before or after it.
fn drop_slot(
    l: &Layout,
    project: &Project,
    y: f32,
) -> Option<(usize, f32, Option<bv_timeline::TrackGroupId>)> {
    // The places in order: track rows and group header rows, each with the
    // first and the end of the tracks it stands for.
    let mut places: Vec<(f32, f32, usize, usize)> = Vec::new();
    for g in &l.groups {
        let members = project.track_group_members(g.group);
        let (Some(first), Some(last)) = (members.first(), members.last()) else {
            continue;
        };
        let (Some(a), Some(b)) = (project.track_index(*first), project.track_index(*last)) else {
            continue;
        };
        let end = if g.collapsed { b + 1 } else { a };
        places.push((g.top, super::groups::GROUP_H, a, end));
    }
    for row in &l.rows {
        if let Some(i) = project.track_index(row.track) {
            places.push((row.top, row.height, i, i + 1));
        }
    }
    places.sort_by(|a, b| a.0.total_cmp(&b.0));
    let last = places.last()?;
    let (index, gap) = places
        .iter()
        .find(|p| y < p.0 + p.1 / 2.0)
        .map_or((last.3, last.0 + last.1), |p| (p.2, p.0));
    let into = l
        .groups
        .iter()
        .find(|g| !g.collapsed && gap > g.top && gap <= g.bottom && y < g.bottom)
        .map(|g| g.group);
    Some((index, gap, into))
}

/// A header value that can be typed (double-click on it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeaderValue {
    /// Video level, in percent.
    Level,
    /// Audio volume, in dB.
    Volume,
    /// Audio pan: "50 L", "30 R", "center" or a signed percentage.
    Pan,
}

/// A typed volume: dB from −96 to +12; "−inf" (or less than −96) is silence.
pub(super) fn parse_volume(text: &str) -> Option<f32> {
    let t = text.trim().to_lowercase();
    if t.contains("inf") || t.contains('∞') {
        return Some(0.0);
    }
    let db = crate::fmt::parse(t.trim_end_matches("db").trim())? as f32;
    if db <= -96.0 {
        Some(0.0)
    } else {
        Some(10f32.powf(db.min(12.0) / 20.0))
    }
}

/// A typed pan: "50 L" / "50 E" (left), "30 R" / "30 D" (right), "center",
/// or a percentage (negative is left). From −1 to 1.
pub(super) fn parse_pan(text: &str) -> Option<f32> {
    let t = text.trim().to_lowercase().replace('%', "");
    let t = t.trim();
    if t.is_empty() || ["c", "center", "centro"].contains(&t) {
        return Some(0.0);
    }
    let (number, sign) = if let Some(n) = t.strip_suffix(['l', 'e']) {
        (n, -1.0)
    } else if let Some(n) = t.strip_suffix(['r', 'd']) {
        (n, 1.0)
    } else {
        (t, 1.0)
    };
    let v = crate::fmt::parse(number.trim())? as f32 * sign;
    Some((v / 100.0).clamp(-1.0, 1.0))
}

/// Double-click on a value's text opens a box to type it; Enter or a
/// click elsewhere applies it, Esc cancels.
#[allow(clippy::too_many_arguments)]
fn value_text(
    view: &mut TimelineView,
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    track: TrackId,
    which: HeaderValue,
    rect: Rect,
    current: String,
) {
    let editing = view
        .value_edit
        .as_ref()
        .is_some_and(|(t, w, _)| *t == track && *w == which);
    if !editing {
        let id = egui::Id::new(("valor-cabecalho", track.0, which as u8));
        let resp = ui.interact(rect, id, Sense::click());
        if resp.double_clicked() {
            view.value_edit = Some((track, which, current));
        }
        resp.on_hover_text(tr("Double-click to type the value"));
        return;
    }
    let Some((_, _, text)) = view.value_edit.as_mut() else {
        return;
    };
    let field = Rect::from_min_size(
        pos2(rect.left() - 2.0, rect.center().y - 9.0),
        vec2(rect.width().max(58.0) + 4.0, 18.0),
    );
    let resp = ui.put(
        field,
        egui::TextEdit::singleline(text)
            .id(egui::Id::new((
                "valor-cabecalho-texto",
                track.0,
                which as u8,
            )))
            .font(FontId::proportional(11.0))
            .margin(vec2(3.0, 1.0)),
    );
    if !resp.has_focus() && !resp.lost_focus() {
        resp.request_focus();
    }
    let cancel = ui.input(|i| i.key_pressed(Key::Escape));
    if resp.lost_focus() || cancel {
        let text = view
            .value_edit
            .take()
            .map(|(_, _, t)| t)
            .unwrap_or_default();
        if cancel {
            return;
        }
        let value = match which {
            HeaderValue::Level => {
                crate::fmt::parse(&text).map(|p| (p as f32 / 100.0).clamp(0.0, 1.0))
            }
            HeaderValue::Volume => parse_volume(&text),
            HeaderValue::Pan => parse_pan(&text),
        };
        if let Some(v) = value {
            let control = match which {
                HeaderValue::Level => TrackControl::Level,
                HeaderValue::Volume => TrackControl::Volume,
                HeaderValue::Pan => TrackControl::Pan,
            };
            let key = egui::Id::new(("valor-digitado", track.0, which as u8));
            app.set_controls(track, control, v, key);
            app.release_controls(track, control);
        }
    }
}

/// What a slider shows its value with.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Thumb {
    /// A knob on the line (level, volume).
    Knob,
    /// A triangle above the line (pan).
    Triangle,
}

/// A VEGAS-style slider: a thin line with a knob or a triangle at
/// `pos` (0–1). Dragging moves it (Ctrl or Shift: fine steps); a
/// double-click returns `Some(default)`. Returns the new position, and
/// whether it was let go; `automation`: an automation control (its thumb
/// marked).
#[allow(clippy::too_many_arguments)]
pub(super) fn slider(
    ui: &egui::Ui,
    rect: Rect,
    id: egui::Id,
    pos: f32,
    default: f32,
    thumb: Thumb,
    tip: &str,
    automation: bool,
) -> (Option<f32>, bool) {
    let (out, released, _) = slider_response(ui, rect, id, pos, default, thumb, tip, automation);
    (out, released)
}

/// [`slider`], and its response.
#[allow(clippy::too_many_arguments)]
fn slider_response(
    ui: &egui::Ui,
    rect: Rect,
    id: egui::Id,
    pos: f32,
    default: f32,
    thumb: Thumb,
    tip: &str,
    automation: bool,
) -> (Option<f32>, bool, egui::Response) {
    let resp = ui.interact(rect, id, Sense::click_and_drag());
    let painter = ui.painter();
    let y = rect.center().y + if thumb == Thumb::Triangle { 3.0 } else { 0.0 };
    let line = Rect::from_min_max(pos2(rect.left(), y - 1.0), pos2(rect.right(), y + 1.0));
    painter.rect_filled(line, 1.0, Color32::from_gray(0x8A));
    let x = rect.left() + rect.width() * pos.clamp(0.0, 1.0);
    let hot = resp.hovered() || resp.dragged();
    match thumb {
        Thumb::Knob => {
            let knob = Rect::from_center_size(pos2(x, y), vec2(9.0, 13.0));
            crate::widgets::paint_knob(painter, knob, hot);
            if automation {
                painter.circle_filled(knob.center(), 2.5, theme::banana());
            }
        }
        Thumb::Triangle => {
            let c = match (automation, hot) {
                (true, _) => theme::banana(),
                (false, true) => theme::text_strong(),
                (false, false) => theme::text(),
            };
            painter.add(egui::Shape::convex_polygon(
                vec![
                    pos2(x - 5.0, y - 8.0),
                    pos2(x + 5.0, y - 8.0),
                    pos2(x, y - 2.0),
                ],
                c,
                Stroke::NONE,
            ));
        }
    }
    let mut out = None;
    if resp.dragged() {
        let fine = if ui.input(|i| i.modifiers.ctrl || i.modifiers.shift) {
            0.1
        } else {
            1.0
        };
        let d = resp.drag_delta().x / rect.width() * fine;
        if d != 0.0 {
            out = Some((pos + d).clamp(0.0, 1.0));
        }
    }
    if resp.double_clicked() {
        out = Some(default);
    }
    // The joystick moves the slider under the pointer.
    if out.is_none()
        && let Some(v) = crate::widgets::joystick_push(ui, &resp)
        && v.x != 0.0
    {
        out = Some((pos + v.x * 0.5).clamp(0.0, 1.0));
    }
    let released = resp.drag_stopped() || resp.double_clicked();
    (out, released, resp.on_hover_text(tip))
}

/// A header's button of Edit Visible Button Set (not Mute, Solo or
/// Automation Settings, drawn by `ui` and `automation_button`): its icon
/// (or letters), lit while its setting is on, greyed where it cannot act;
/// clicked, its More menu command — Compositing Mode and Record Input
/// open their menus.
fn header_button(
    ui: &egui::Ui,
    view: &mut TimelineView,
    app: &mut BananaApp,
    project: &Project,
    track: &Track,
    spot: Spot,
    rect: Rect,
) {
    use super::header_icons as glyph;
    use crate::icons::Icon;
    use HeaderButton as B;
    let tid = track.id;
    let id = egui::Id::new(("cabecalho", tid.0, "botao", format!("{spot:?}")));
    let name = match spot {
        Spot::Button(b) => b.name(),
        Spot::Minimize => tr("Minimize"),
        Spot::Maximize => tr("Maximize"),
        Spot::Bus | Spot::Restore | Spot::Armed => return,
    };
    let mode = track.composite_mode;
    let (on, enabled) = match spot {
        Spot::Button(B::BypassMotionBlur) => (track.bypass_motion_blur, true),
        Spot::Button(B::TrackMotion) => (track.motion.is_some(), !track.adjustment),
        Spot::Button(B::ArmForRecord) => (track.armed, true),
        Spot::Button(B::InvertPhase) => (track.invert_phase, true),
        Spot::Button(B::TrackFx) => (!track.effects.is_empty(), true),
        Spot::Button(B::CompositingMode) => (mode != bv_timeline::CompositeMode::SourceAlpha, true),
        Spot::Button(B::MakeParent) => (false, track.composite_depth > 0),
        Spot::Button(B::MakeChild) => (false, project.can_make_child(tid)),
        Spot::Button(B::RecordInput) => (track.record_input.is_some(), true),
        Spot::Button(B::DuckController) => (track.ducking.is_some(), true),
        Spot::Button(B::DuckListener) => (track.duck_listener, true),
        _ => (false, true),
    };
    let resp = ui.interact(
        rect,
        id,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name));
    let painter = ui.painter();
    let back = rect.shrink2(vec2(2.0, 0.0));
    if on {
        painter.rect_filled(back, 2.0, theme::tab_on());
    } else if resp.hovered() && enabled {
        painter.rect_filled(back, 2.0, theme::hover());
    }
    let color = if enabled {
        theme::text()
    } else {
        theme::text_disabled()
    };
    let icon = Rect::from_center_size(rect.center(), vec2(14.0, 14.0));
    match spot {
        Spot::Button(B::BypassMotionBlur) => glyph::bypass_motion_blur(painter, icon, color),
        Spot::Button(B::TrackMotion) => glyph::track_motion(painter, icon, color),
        Spot::Button(B::ColorGrading) => glyph::color_grading(painter, icon, color),
        Spot::Button(B::CompositingMode) => {
            let mark = match mode {
                bv_timeline::CompositeMode::SourceAlpha => "α",
                bv_timeline::CompositeMode::ThreeDSourceAlpha => "3D",
                _ => "",
            };
            glyph::compositing_mode(painter, icon, color, mark);
        }
        Spot::Button(B::MakeParent) => glyph::make_parent(painter, icon, color),
        Spot::Button(B::MakeChild) => glyph::make_child(painter, icon, color),
        Spot::Minimize => glyph::minimize(painter, icon, color),
        Spot::Maximize => glyph::maximize(painter, icon, color),
        Spot::Button(B::TrackFx) => crate::icons::paint(painter, icon.expand(1.0), Icon::Fx, color),
        Spot::Button(B::ArmForRecord) => crate::icons::paint(painter, icon, Icon::Record, color),
        Spot::Button(B::InvertPhase) => crate::icons::paint(painter, icon, Icon::Phase, color),
        _ => {
            let letters = match spot {
                Spot::Button(B::RecordInput) => "In",
                Spot::Button(B::DuckController) => "DC",
                _ => "DL",
            };
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                letters,
                FontId::proportional(11.0),
                color,
            );
        }
    }
    let resp = match spot {
        Spot::Button(B::CompositingMode) => {
            resp.on_hover_text(trf!("Compositing Mode: {}", tr(mode.name())))
        }
        _ => resp.on_hover_text(name),
    };
    // The ones with choices open their menu; the others act.
    match spot {
        Spot::Button(B::CompositingMode) => {
            egui::Popup::menu(&resp).show(|ui| compositing_menu(ui, app, project, tid));
        }
        Spot::Button(B::RecordInput) => {
            egui::Popup::menu(&resp).show(|ui| record_input_menu(ui, app, project, tid));
        }
        _ if resp.clicked() => match spot {
            Spot::Button(B::BypassMotionBlur) => app.toggle_bypass_motion_blur(tid),
            Spot::Button(B::TrackMotion) => app.open_track_motion(tid, false),
            Spot::Button(B::ArmForRecord) => app.toggle_arm(tid),
            Spot::Button(B::InvertPhase) => invert_phase(app, track),
            Spot::Button(B::TrackFx) => app.clip_windows.track_fx = Some(tid),
            Spot::Button(B::MakeParent) => {
                let family = ui.input(|i| i.modifiers.command);
                app.edit(|p| p.make_compositing_parent(tid, family));
            }
            Spot::Button(B::MakeChild) => {
                let family = ui.input(|i| i.modifiers.command);
                app.edit(|p| p.make_compositing_child(tid, family));
            }
            Spot::Minimize => view.minimize_track(tid),
            Spot::Maximize => view.maximize_track(project, tid),
            Spot::Button(B::ColorGrading) => {
                app.selected_track = Some(tid);
                crate::tabs::color::open_at(app, crate::tabs::color::Level::Track);
            }
            Spot::Button(B::DuckController) => app.toggle_duck_controller(tid),
            Spot::Button(B::DuckListener) => app.toggle_duck_listener(tid),
            _ => {}
        },
        _ => {}
    }
}

/// Automation Settings, as VEGAS's: its icon (the mode's, as the Mixing
/// Console's) turns the track's automation controls on or off — lit
/// while they are —, its arrow opens the menu of the modes.
fn automation_button(ui: &egui::Ui, app: &mut BananaApp, track: &Track, rect: Rect) {
    let tid = track.id;
    let auto = track.automation;
    let id = egui::Id::new(("cabecalho", tid.0, "automacao"));
    let icon_r = Rect::from_min_size(rect.min, vec2(CELL_W, rect.height()));
    let arrow_r = Rect::from_min_max(pos2(icon_r.right(), rect.top()), rect.max);
    let icon = ui.interact(icon_r, id, Sense::click());
    let arrow = ui.interact(arrow_r, id.with("modo"), Sense::click());
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&arrow));
    let lit = auto.controls || open;
    let painter = ui.painter();
    let back = rect.shrink2(vec2(2.0, 0.0));
    if lit {
        painter.rect_filled(back, 2.0, theme::selection());
    } else if icon.hovered() || arrow.hovered() {
        painter.rect_filled(back, 2.0, theme::hover());
    }
    crate::tabs::console::paint_mode_icon(
        painter,
        Rect::from_center_size(icon_r.center(), vec2(16.0, 16.0)),
        auto.mode,
        lit.then_some(Color32::WHITE),
    );
    let color = if lit { Color32::WHITE } else { theme::text() };
    super::header_icons::menu_arrow(
        painter,
        pos2(arrow_r.center().x - 1.0, rect.center().y),
        color,
    );
    icon.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            tr("Activate Automation Controls"),
        )
    });
    arrow.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tr("Automation Settings"))
    });
    let name = crate::commands::automation_mode_name(auto.mode);
    if icon
        .on_hover_text(trf!("Automation Settings: {}", name))
        .clicked()
    {
        app.toggle_automation_controls(tid);
    }
    let arrow = arrow.on_hover_text(tr("Automation Settings"));
    egui::Popup::menu(&arrow).show(|ui| {
        for m in bv_timeline::AutomationMode::ALL {
            if crate::tabs::console::mode_item(ui, m, m == auto.mode).clicked() {
                app.set_automation_mode(tid, m);
                ui.close();
            }
        }
    });
}

/// Video effects (a preset's, a filter package's chain) let go on a video
/// track's header, VEGAS's way to add track effects, or on an adjustment
/// track: added to its chain, its Video Track FX window shown.
pub(super) fn drop_track_effects(app: &mut BananaApp, track: TrackId, drag: crate::app::MediaDrag) {
    let chain = match drag {
        crate::app::MediaDrag::Effect(e) => vec![e],
        crate::app::MediaDrag::Package(_, chain) => chain,
        _ => return,
    };
    crate::tabs::effects::apply_to_track(app, track, &chain);
    app.selected_track = Some(track);
    app.clip_windows.track_fx = Some(track);
}

/// Invert Track Phase: the selected audio tracks with `track`, when it is
/// one of them.
fn invert_phase(app: &mut BananaApp, track: &Track) {
    let v = !track.invert_phase;
    let mut ids = app.track_selection();
    if !ids.contains(&track.id) {
        ids = vec![track.id];
    }
    app.modify(|p| {
        for t in ids {
            if let Some(t) = p.track_by_id_mut(t).filter(|t| t.kind == TrackKind::Audio) {
                t.invert_phase = v;
            }
        }
    });
}

/// An output meter's shortcut menu, as VEGAS's: Reset Clip, the display
/// scale, Use Vertical Meters and Show Output Meters.
fn meter_menu(ui: &mut egui::Ui, app: &mut BananaApp, tid: TrackId) {
    if ui.button(tr("Reset Clip")).clicked() {
        if let Some(m) = app.track_meters.get_mut(&tid) {
            m.reset_clip();
        }
        ui.close();
    }
    ui.separator();
    let now = app.settings.track_meter_range;
    for r in crate::settings::MeterRange::ALL {
        let label = trf!("{} dB", crate::fmt::num(r.floor().abs() as f64, 1));
        if crate::widgets::radio(ui, now == r, &label).clicked() {
            app.settings.track_meter_range = r;
            ui.close();
        }
    }
    ui.separator();
    if toggle_item(
        ui,
        tr("Use Vertical Meters"),
        !app.settings.track_meters_across,
    ) {
        app.settings.track_meters_across ^= true;
        ui.close();
    }
    if toggle_item(ui, tr("Show Output Meters"), true) {
        app.settings.hide_track_meters = true;
        ui.close();
    }
}

/// Arm for Record: a circle, filled red when the track is armed.
fn arm_button(ui: &egui::Ui, rect: Rect, id: egui::Id, on: bool) -> bool {
    let resp = ui.interact(rect, id, Sense::click());
    let painter = ui.painter();
    if resp.hovered() {
        painter.rect_filled(rect, 2.0, theme::hover());
    }
    let red = Color32::from_rgb(0xE5, 0x39, 0x35);
    if on {
        painter.circle_filled(rect.center(), 5.0, red);
    } else {
        painter.circle_stroke(rect.center(), 4.5, Stroke::new(1.5, theme::text_dim()));
    }
    resp.on_hover_text(tr("Arm for Record")).clicked()
}

/// "M" or "S" in the top row: plain letters, filled when on.
pub(super) fn letter_button(
    ui: &egui::Ui,
    rect: Rect,
    id: egui::Id,
    on: bool,
    letter: &str,
    on_color: Color32,
    tip: &str,
) -> bool {
    let resp = ui.interact(rect, id, Sense::click());
    let painter = ui.painter();
    if on {
        painter.rect_filled(rect, 2.0, on_color);
    } else if resp.hovered() {
        painter.rect_filled(rect, 2.0, theme::hover());
    }
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        letter,
        FontId::proportional(13.5),
        if on { Color32::BLACK } else { theme::text() },
    );
    resp.on_hover_text(tip).clicked()
}

/// The small kind mark next to the track number: a screen or a note.
fn paint_kind(painter: &egui::Painter, at: Pos2, kind: TrackKind, color: Color32) {
    let st = Stroke::new(1.0, color);
    match kind {
        TrackKind::Video => {
            let screen = Rect::from_min_size(at, vec2(8.0, 6.0));
            painter.rect_stroke(screen, 0.0, st, egui::StrokeKind::Inside);
            painter.line_segment(
                [pos2(at.x + 2.0, at.y + 8.0), pos2(at.x + 6.0, at.y + 8.0)],
                st,
            );
        }
        TrackKind::Audio => {
            painter.circle_filled(pos2(at.x + 2.5, at.y + 6.5), 2.0, color);
            painter.line_segment([pos2(at.x + 4.3, at.y + 6.5), pos2(at.x + 4.3, at.y)], st);
            painter.line_segment([pos2(at.x + 4.3, at.y), pos2(at.x + 7.5, at.y + 2.0)], st);
        }
    }
}

/// The three dots of the More button.
fn paint_dots(painter: &egui::Painter, center: Pos2, color: Color32) {
    for dx in [-4.0, 0.0, 4.0] {
        painter.circle_filled(pos2(center.x + dx, center.y), 1.3, color);
    }
}

/// The height of the row an audio track's meter takes across its header
/// (`across`, the meters' Use Vertical Meters off), when there is room.
fn across_row(r: Rect, across: bool) -> f32 {
    if across && r.height() >= 44.0 {
        14.0
    } else {
        0.0
    }
}

/// Where an audio track's output meter is in its header (`r`): under the
/// top row when there is room, its peak over it, as VEGAS's; `across`, the
/// meters' Use Vertical Meters off, in a row under the name, after "Out",
/// its peak right of it.
pub(crate) fn meter_rect(r: Rect, across: bool) -> Rect {
    if across_row(r, across) > 0.0 {
        return Rect::from_min_max(
            pos2(r.left() + STRIP_W + 30.0, r.top() + 23.0),
            pos2(r.right() - 40.0, r.top() + 33.0),
        );
    }
    let top = if r.height() >= 56.0 { 18.0 } else { 3.0 };
    Rect::from_min_max(
        pos2(r.right() - METER_W + 3.0, r.top() + top),
        pos2(r.right() - 4.0, r.bottom() - 5.0),
    )
}

/// Whether `pos` is over an audio track's output meter (the mouse wheel
/// there changes the meters' range, not the tracks' scroll).
pub(crate) fn over_meter(l: &Layout, pos: Pos2, across: bool) -> bool {
    l.headers.contains(pos)
        && l.rows.iter().any(|row| {
            row.kind == TrackKind::Audio
                && meter_rect(row.rect(l.headers.left(), l.headers.right()), across).contains(pos)
        })
}

/// The audio track's output meter across its header (VEGAS's horizontal
/// meters): each channel's bar from the left — green, amber from −12 dB,
/// red from −3 —, its peak held as a line, the scale's numbers along it.
pub(crate) fn paint_meter_across(
    ui: &egui::Ui,
    rect: Rect,
    meter: Option<&crate::tabs::master::MeterState>,
    range: crate::settings::MeterRange,
) {
    let painter = ui.painter();
    let floor = range.floor();
    painter.rect_filled(rect.expand(1.0), 0.0, Color32::from_gray(0x14));
    let x_of = |db: f32| rect.left() + (1.0 - (db / floor).clamp(0.0, 1.0)) * rect.width();
    let h = (rect.height() - 1.0) / 2.0;
    let zones = [
        (floor, -12.0, Color32::from_rgb(0x3C, 0xE0, 0x2C)),
        (-12.0, -3.0, Color32::from_rgb(0xF0, 0xC0, 0x20)),
        (-3.0, 0.0, Color32::from_rgb(0xF0, 0x30, 0x20)),
    ];
    for i in 0..2 {
        let top = rect.top() + i as f32 * (h + 1.0);
        let (level, hold) = meter.map_or((-96.0, -96.0), |m| (m.level(i), m.hold(i)));
        for (from, to, color) in zones {
            if level > from {
                let lit =
                    Rect::from_min_max(pos2(x_of(from), top), pos2(x_of(level.min(to)), top + h));
                painter.rect_filled(lit, 0.0, color);
            }
        }
        if hold > floor {
            painter.vline(
                x_of(hold.min(0.0)),
                top..=top + h,
                Stroke::new(1.0, Color32::from_gray(0xE6)),
            );
        }
    }
    let ink = Color32::from_gray(0x60);
    let step = range.step();
    let mut db = step;
    while (db as f32) < -floor - 0.5 {
        let x = x_of(-(db as f32));
        if x - rect.left() > 8.0 {
            painter.text(
                pos2(x, rect.center().y),
                Align2::CENTER_CENTER,
                db.to_string(),
                FontId::proportional(8.5),
                ink,
            );
        }
        db += step;
    }
}

/// The audio track's output meter, as VEGAS's: framed, dim red, amber and
/// green down its height (the Master Bus's colors), each channel (L, R)
/// lit up to its level with its peak held as a line, and its scale on the
/// right edge — numbers every 9 dB and ticks every 3 down to 42 dB below
/// 0, or one of the other ranges the mouse wheel goes through.
pub(crate) fn paint_meter(
    ui: &egui::Ui,
    rect: Rect,
    meter: Option<&crate::tabs::master::MeterState>,
    range: crate::settings::MeterRange,
) {
    paint_meter_scaled(
        ui,
        rect,
        meter,
        None,
        range.floor(),
        range.step(),
        range.tick(),
    );
}

/// [`paint_meter`] down to `floor` dB, numbered every `step` dB (or a
/// multiple, where they would crowd) with ticks every `tick` (or a
/// multiple) — a dot by each number, a dash between, as VEGAS's —: a bar
/// for each of the meter's channels, or channel `only` alone (the Mixing
/// Console's Meters region, a meter for each). The console's meters have
/// scales of their own.
pub(crate) fn paint_meter_scaled(
    ui: &egui::Ui,
    rect: Rect,
    meter: Option<&crate::tabs::master::MeterState>,
    only: Option<usize>,
    floor: f32,
    step: u32,
    tick: u32,
) {
    use crate::tabs::master::meter_gradient;
    let painter = ui.painter();
    painter.rect_filled(rect.expand(1.0), 0.0, Color32::from_gray(0x14));
    meter_gradient(painter, rect, 0.0, floor, false, floor);
    let channels: Vec<usize> = match only {
        Some(i) => vec![i],
        None => (0..meter.map_or(2, |m| m.channels().clamp(1, 8))).collect(),
    };
    let n = channels.len();
    let w = (rect.width() - (n - 1) as f32) / n as f32;
    let depth = |db: f32| rect.top() + (db / floor).clamp(0.0, 1.0) * rect.height();
    for (k, &i) in channels.iter().enumerate() {
        let bar = Rect::from_min_size(
            pos2(rect.left() + k as f32 * (w + 1.0), rect.top()),
            vec2(w, rect.height()),
        );
        let (level, hold) = meter.map_or((-96.0, -96.0), |m| (m.level(i), m.hold(i)));
        if level > floor {
            meter_gradient(painter, bar, level.min(0.0), floor, true, floor);
        }
        if hold > floor {
            painter.hline(
                bar.x_range(),
                depth(hold.min(0.0)),
                Stroke::new(1.0, Color32::from_gray(0xE6)),
            );
        }
    }
    // The scale: the numbers right-aligned against their dots (fewer when
    // they would crowd), dashes between them.
    if rect.height() >= 28.0 {
        let grey = Color32::from_gray(0x78);
        let px = rect.height() / -floor;
        let every = step * (10.0 / (step as f32 * px)).ceil().max(1.0) as u32;
        let tick = tick * (3.0 / (tick as f32 * px)).ceil().max(1.0) as u32;
        let font = if rect.width() >= 20.0 { 10.5 } else { 9.0 };
        // Ticks this small: a mesh, or they feather away.
        let mut ticks = egui::Mesh::default();
        let mut db = tick;
        while (db as f32) < -floor - 0.5 {
            let y = depth(-(db as f32)).round();
            let numbered = db.is_multiple_of(every);
            let w = if numbered { 1.0 } else { 3.0 };
            ticks.add_colored_rect(
                Rect::from_min_size(pos2(rect.right() - w, y), vec2(w, 1.0)),
                grey,
            );
            if numbered && rect.width() >= 12.0 {
                painter.text(
                    pos2(rect.right() - 3.0, y + 0.5),
                    Align2::RIGHT_CENTER,
                    db.to_string(),
                    FontId::proportional(font),
                    grey,
                );
            }
            db += tick;
        }
        painter.add(egui::Shape::mesh(ticks));
    }
}

/// The highest peak an audio track's meter holds ("-0.5"), shown over the
/// meter while there is one.
fn meter_peak(meter: Option<&crate::tabs::master::MeterState>) -> Option<f32> {
    let m = meter?;
    let peak = (0..2).map(|i| m.hold(i)).fold(f32::MIN, f32::max);
    (peak > -96.0).then_some(peak)
}

pub(crate) fn ui(
    view: &mut TimelineView,
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    l: &Layout,
    project: &Project,
) {
    let painter = ui.painter().with_clip_rect(l.headers);
    // Below the tracks the timeline's own dark, as VEGAS's track list.
    painter.rect_filled(l.headers, 0.0, theme::bg());
    let picked = app.track_selection();
    for row in &l.rows {
        let Some(track) = project.track(row.track) else {
            continue;
        };
        let index = project.track_index(row.track).unwrap_or(0);
        // Without its Surround Pan Keyframes row (drawn by surround_keys);
        // in a group, after the grouping lane.
        let mut r = row.body(l.headers.left(), l.headers.right());
        if row.group.is_some() {
            r.min.x += super::groups::LANE_W;
        }
        if !r.intersects(l.headers) {
            continue;
        }
        let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(l.headers));
        cui.set_clip_rect(l.headers);
        let ui = &mut cui;
        let id = |s: &str| egui::Id::new(("cabecalho", track.id.0, s));
        // The whole header first: the controls drawn after it are on top.
        // Dragging it moves the track, like VEGAS's track list.
        let body = ui.interact(r, id("corpo"), Sense::click_and_drag());
        if body.drag_started() {
            view.track_drag = Some(TrackDrag::Reorder(track.id));
        }

        let painter = ui.painter().clone();
        let selected = picked.contains(&track.id);
        let fill = match (selected, app.key_focus == crate::keymap::Context::TrackList) {
            (true, true) => theme::header_selected_focused(),
            (true, false) => theme::header_selected(),
            _ => theme::panel(),
        };
        painter.rect_filled(r.shrink2(vec2(0.0, 0.5)), 0.0, fill);
        let strip = Rect::from_min_size(r.min, vec2(STRIP_W, r.height() - 1.0));
        painter.rect_filled(strip, 0.0, app.settings.prefs.color_of(track));
        // A compositing child: a bracket for each level it is in.
        for k in 0..track.composite_depth {
            let x = strip.right() + 3.5 + 5.0 * k as f32;
            let stroke = Stroke::new(1.0, theme::text_dim());
            painter.line_segment([pos2(x, r.top() + 2.0), pos2(x, r.bottom() - 3.0)], stroke);
            painter.line_segment(
                [pos2(x, r.bottom() - 3.0), pos2(x + 3.0, r.bottom() - 3.0)],
                stroke,
            );
        }
        painter.line_segment(
            [r.left_bottom(), r.right_bottom()],
            Stroke::new(1.0, theme::panel_dark()),
        );
        // A video effect dragged over a video track's header: the track's
        // own when let go, as VEGAS's; the header outlined meanwhile.
        if row.kind == TrackKind::Video
            && let Some(
                drag @ (crate::app::MediaDrag::Effect(_) | crate::app::MediaDrag::Package(..)),
            ) = app.media_drag.clone()
            && ui
                .input(|i| i.pointer.hover_pos())
                .is_some_and(|p| r.contains(p))
        {
            painter.rect_stroke(
                r.shrink(1.0),
                2.0,
                Stroke::new(2.5, theme::snap()),
                egui::StrokeKind::Inside,
            );
            if ui.input(|i| i.pointer.any_released()) {
                app.media_drag = None;
                drop_track_effects(app, track.id, drag);
            }
        }

        // The strip: More (⋯), the number and the kind; More blue while
        // its menu is open, as VEGAS's.
        let dots = Rect::from_min_size(strip.min, vec2(STRIP_W, 14.0));
        let more = ui.interact(dots, id("mais"), Sense::click());
        if egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&more)) {
            painter.rect_filled(dots, 0.0, theme::selection());
        } else if more.hovered() {
            painter.rect_filled(dots, 0.0, Color32::from_white_alpha(40));
        }
        paint_dots(&painter, dots.center(), Color32::WHITE);
        if r.height() >= 30.0 {
            painter.text(
                pos2(strip.left() + 3.0, r.top() + 15.0),
                Align2::LEFT_TOP,
                (index + 1).to_string(),
                FontId::proportional(13.0),
                Color32::WHITE,
            );
            if track.adjustment {
                painter.text(
                    pos2(strip.left() + 16.0, r.top() + 24.0),
                    Align2::CENTER_CENTER,
                    "fx",
                    FontId::proportional(10.0),
                    Color32::WHITE,
                );
            } else {
                paint_kind(
                    &painter,
                    pos2(strip.left() + 12.0, r.top() + 20.0),
                    row.kind,
                    Color32::WHITE,
                );
            }
        }
        more.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tr("More")));
        let more = more.on_hover_text(tr("More"));
        let tid = track.id;
        // A track list dragged narrow: only the color strip, its number
        // and More, with every command.
        if r.width() < 64.0 {
            egui::Popup::menu(&more).show(|ui| more_menu(ui, view, app, project, track, &[]));
            if body.secondary_clicked() {
                view.context = super::lanes::ContextTarget::Track(track.id);
            }
            body.context_menu(|ui| header_menu(ui, view, app, project, track, &[]));
            continue;
        }

        // Top row: the name, then the buttons Edit Visible Button Set shows,
        // Mute and Solo last (left of the meter on audio), from the right as
        // VEGAS's, each where it fits — they go from the left as the list
        // narrows (`ButtonsFor::lay_out`) —; More has the others' commands.
        // Narrower, as VEGAS's, the meter and the multipurpose slider give
        // way, then the sliders' names and values (`Tier`).
        let meters = !app.settings.hide_track_meters && r.width() >= 190.0;
        let tier = Tier::of(r.width(), row.kind);
        // The meters across: a row under the name, the rows below it down.
        let across = app.settings.track_meters_across;
        let out_h = if row.kind == TrackKind::Audio && meters {
            across_row(r, across)
        } else {
            0.0
        };
        let right = match row.kind {
            TrackKind::Audio if meters && out_h == 0.0 => r.right() - METER_W - 4.0,
            _ => r.right() - 6.0,
        };
        let top = r.top() + 4.0;
        let set = app.settings.header_buttons.of(row.kind);
        // The track's own: the bus button (audio, while the project has
        // busses: where the track plays), Restore Track Height (minimized
        // alone, as VEGAS's: ` brings the others back, not it) and Arm for
        // Record while armed (from More or Ctrl+Alt+R), to be disarmed.
        let mut extras = Vec::new();
        if row.kind == TrackKind::Audio && !project.busses.is_empty() {
            extras.push(Spot::Bus);
        }
        if view.track_heights.get(&tid) == Some(&super::MIN_TRACK_H) {
            extras.push(Spot::Restore);
        }
        if row.kind == TrackKind::Audio && track.armed && !set.contains(&HeaderButton::ArmForRecord)
        {
            extras.push(Spot::Armed);
        }
        let second_row = r.height() >= 44.0 + out_h;
        let buttons_right = match row.kind {
            TrackKind::Audio if meters && out_h == 0.0 => meter_rect(r, across).left() - 2.0,
            _ => r.right() - 3.5,
        };
        let buttons = ButtonsFor {
            kind: row.kind,
            set: &set,
            extras: &extras,
            adjustment: track.adjustment,
        }
        .lay_out(
            strip.right() + 4.0,
            buttons_right,
            top + 8.5,
            second_row.then_some((r.top() + 36.0 + out_h, r.right() - 6.5)),
        );
        let shown: Vec<Spot> = buttons.iter().map(|(s, _)| *s).collect();
        egui::Popup::menu(&more).show(|ui| more_menu(ui, view, app, project, track, &shown));
        let in_top = |cell: &Rect| cell.top() < top + 17.0;
        for &(spot, cell) in &buttons {
            let button = Rect::from_center_size(cell.center(), vec2(18.0, 17.0));
            match spot {
                // Mute: with Automation Settings an automation control (the
                // mute envelope; with automation off, the whole track).
                Spot::Button(HeaderButton::Mute) => {
                    let owner = crate::automation::AutoOwner::Track(tid);
                    let auto_mute = track.automation.controls;
                    let muted = if auto_mute {
                        app.owner_muted(owner)
                    } else {
                        track.muted
                    };
                    let clicked = letter_button(
                        ui,
                        button,
                        id("mute"),
                        muted,
                        "M",
                        MUTE_ON,
                        &control_tip(
                            tr("Mute Track (Ctrl+click: this track alone; on a muted one, none)"),
                            track,
                        ),
                    );
                    if auto_mute {
                        crate::widgets::mark_automation(ui, button);
                    }
                    if clicked {
                        if ui.input(|i| i.modifiers.ctrl || i.modifiers.command) {
                            app.modify(|p| p.mute_alone(tid));
                        } else {
                            toggle_mute(app, track, id("mute"));
                        }
                    }
                }
                Spot::Button(HeaderButton::Solo) => {
                    if letter_button(
                        ui,
                        button,
                        id("solo"),
                        track.solo,
                        "S",
                        theme::banana(),
                        tr("Solo (Ctrl+click: this track alone; on a soloed one, none)"),
                    ) {
                        if ui.input(|i| i.modifiers.ctrl || i.modifiers.command) {
                            app.modify(|p| p.solo_alone(tid));
                        } else {
                            toggle_solo(app, track);
                        }
                    }
                }
                Spot::Button(HeaderButton::Automation) => automation_button(ui, app, track, cell),
                Spot::Bus => bus_button(ui, app, project, track, button, id("barramento")),
                Spot::Restore => {
                    if letter_button(
                        ui,
                        button,
                        id("restaurar-altura"),
                        false,
                        "=",
                        theme::text(),
                        tr("Restore Track Height"),
                    ) {
                        view.restore_track(tid);
                    }
                }
                Spot::Armed => {
                    if arm_button(ui, button, id("armar"), true) {
                        app.toggle_arm(tid);
                    }
                }
                _ => header_button(ui, view, app, project, track, spot, cell),
            }
        }
        // The name: the room the top row's buttons leave (while it is
        // renamed, some room over them).
        let name_right = buttons
            .iter()
            .filter(|(_, c)| in_top(c))
            .map(|(_, c)| c.left() - 2.0)
            .fold(right, f32::min);
        let name_rect = Rect::from_min_max(
            pos2(strip.right() + 4.0, top),
            pos2(name_right.max(strip.right() + 4.0), top + 17.0),
        );
        if view.renaming.as_ref().is_some_and(|(t, _)| *t == track.id) {
            let field = Rect::from_min_max(
                name_rect.min,
                pos2(
                    name_rect
                        .right()
                        .max(name_rect.left() + 90.0)
                        .min(r.right() - 4.0),
                    name_rect.bottom(),
                ),
            );
            rename_field(view, ui, app, field, track.id, id("nome"));
        } else if name_rect.width() > 8.0 {
            crate::widgets::fit_text(
                &painter,
                pos2(name_rect.left() + 2.0, name_rect.center().y),
                Align2::LEFT_CENTER,
                scribble_name(track),
                FontId::proportional(12.5),
                if track.muted {
                    theme::text_dim()
                } else {
                    theme::text()
                },
                name_rect.width() - 2.0,
            );
        }
        // The second row's buttons (a video track's compositing ones) at its
        // end: its slider before them.
        let second_left = buttons
            .iter()
            .filter(|(_, c)| !in_top(c))
            .map(|(_, c)| c.left())
            .reduce(f32::min);

        // Second row: level (video) or volume and pan (audio).
        let mut send_right = right;
        if r.height() >= 44.0 + out_h {
            let y = r.top() + 36.0 + out_h;
            let x0 = strip.right() + 6.0;
            let font = FontId::proportional(11.0);
            match row.kind {
                TrackKind::Video => {
                    let auto = track.automation.controls;
                    let level = app.control_value(tid, TrackControl::Level);
                    let pct = level * 100.0;
                    let end = second_left.map_or(right - 4.0, |x| x - 12.0);
                    // Its value before it, from the Values tier, while the
                    // slider keeps room (VEGAS's compositing buttons take
                    // it); a slider too short goes.
                    let label = trf!("Level: {} %", crate::fmt::num(pct as f64, 1));
                    let label_w = painter
                        .layout_no_wrap(label.clone(), font.clone(), theme::text())
                        .size()
                        .x;
                    let from = if tier >= Tier::Values && x0 + label_w + 60.0 <= end {
                        let text = painter.text(
                            pos2(x0, y),
                            Align2::LEFT_CENTER,
                            label,
                            font.clone(),
                            theme::text(),
                        );
                        value_text(
                            view,
                            ui,
                            app,
                            tid,
                            HeaderValue::Level,
                            text,
                            crate::fmt::num(pct as f64, 1),
                        );
                        text.right() + 10.0
                    } else {
                        x0
                    };
                    if end - from >= 30.0 {
                        let s = Rect::from_min_max(pos2(from, y - 8.0), pos2(end, y + 8.0));
                        let key = id("nivel");
                        let (moved, released) = slider(
                            ui,
                            s,
                            key,
                            level,
                            1.0,
                            Thumb::Knob,
                            &control_tip(tr("Level (double-click: 100 %; Ctrl+drag: fine)"), track),
                            auto,
                        );
                        if let Some(v) = moved {
                            app.set_controls(tid, TrackControl::Level, v, key);
                        }
                        if released {
                            app.release_controls(tid, TrackControl::Level);
                        }
                    }
                    // With Automation Settings, the Fade slider (fade to
                    // color) below, as VEGAS's.
                    if auto && r.height() >= 62.0 {
                        let fy = y + 20.0;
                        let fade = app.control_value(tid, TrackControl::Fade);
                        let from = if tier >= Tier::Values {
                            let label = trf!(
                                "Fade: {} %",
                                crate::fmt::num((fade * 100.0).round() as f64, 0)
                            );
                            painter
                                .text(
                                    pos2(x0, fy),
                                    Align2::LEFT_CENTER,
                                    label,
                                    font,
                                    theme::text(),
                                )
                                .right()
                                + 10.0
                        } else {
                            x0
                        };
                        let s =
                            Rect::from_min_max(pos2(from, fy - 8.0), pos2(right - 4.0, fy + 8.0));
                        let key = id("fade");
                        let (moved, released) = slider(
                            ui,
                            s,
                            key,
                            (fade + 1.0) / 2.0,
                            0.5,
                            Thumb::Knob,
                            &control_tip(
                                tr(
                                    "Fade to the top (right) or bottom (left) color (double-click: none)",
                                ),
                                track,
                            ),
                            true,
                        );
                        if let Some(v) = moved {
                            app.set_controls(tid, TrackControl::Fade, v * 2.0 - 1.0, key);
                        }
                        if released {
                            app.release_controls(tid, TrackControl::Fade);
                        }
                    }
                }
                TrackKind::Audio => {
                    let auto = track.automation.controls;
                    let volume = app.control_value(tid, TrackControl::Volume);
                    let db = if volume <= 1e-4 {
                        None
                    } else {
                        Some(20.0 * volume.log10())
                    };
                    let label = match db {
                        None => "−∞ dB".to_string(),
                        Some(db) => format!("{} dB", crate::fmt::num(db as f64, 1)),
                    };
                    let typed = db.map_or("-inf".to_string(), |db| crate::fmt::num(db as f64, 1));
                    // Named, volume and pan share the row half and half.
                    let middle = (x0 + right) / 2.0;
                    let owner = bv_timeline::SurroundOwner::Track(tid);
                    let surround = project.surround_pans(owner);
                    let pan_too = !surround && tier >= Tier::Both;
                    // Volume: a fader taper with 0 dB near the middle.
                    let vol = match tier {
                        Tier::Named => {
                            let name = painter.text(
                                pos2(x0, y),
                                Align2::LEFT_CENTER,
                                tr("Vol:"),
                                font.clone(),
                                theme::text(),
                            );
                            let at = x0 + (name.width() + 12.0).max(46.0);
                            let text = painter.text(
                                pos2(at, y),
                                Align2::LEFT_CENTER,
                                label,
                                font.clone(),
                                theme::text(),
                            );
                            value_text(view, ui, app, tid, HeaderValue::Volume, text, typed);
                            let left = text.right().max(at + 36.0) + 10.0;
                            let end = if surround { right - 4.0 } else { middle - 16.0 };
                            Rect::from_min_max(
                                pos2(left, y - 8.0),
                                pos2(end.max(left + 30.0), y + 8.0),
                            )
                        }
                        Tier::Values => {
                            let text = painter.text(
                                pos2(x0, y),
                                Align2::LEFT_CENTER,
                                label,
                                font.clone(),
                                theme::text(),
                            );
                            value_text(view, ui, app, tid, HeaderValue::Volume, text, typed);
                            Rect::from_min_size(pos2(x0 + 44.0, y - 8.0), vec2(38.0, 16.0))
                        }
                        // Sliders alone: the volume's half the row when the
                        // pan has the other half.
                        _ if pan_too => {
                            Rect::from_min_max(pos2(x0, y - 8.0), pos2(middle - 4.0, y + 8.0))
                        }
                        _ => Rect::from_min_max(pos2(x0, y - 8.0), pos2(right - 4.0, y + 8.0)),
                    };
                    let key = id("nivel");
                    let pos = (volume / MAX_GAIN).max(0.0).sqrt();
                    let unity = (1.0 / MAX_GAIN).sqrt();
                    let (moved, released) = slider(
                        ui,
                        vol,
                        key,
                        pos,
                        unity,
                        Thumb::Knob,
                        &control_tip(tr("Volume (double-click: 0 dB; Ctrl+drag: fine)"), track),
                        auto,
                    );
                    if let Some(v) = moved {
                        app.set_controls(tid, TrackControl::Volume, v * v * MAX_GAIN, key);
                    }
                    if released {
                        app.release_controls(tid, TrackControl::Volume);
                    }
                    // A 5.1 project: the small surround panner instead,
                    // at the header's right.
                    if surround && tier >= Tier::Values {
                        let top = y - 9.0;
                        let side = (r.bottom() - 3.0 - top)
                            .min(right - 2.0 - (vol.right() + 10.0))
                            .min(36.0);
                        if side >= 14.0 {
                            let rect = Rect::from_min_size(
                                pos2(right - 2.0 - side, top),
                                vec2(side, side),
                            );
                            send_right = rect.left() - 6.0;
                            crate::tabs::surround::mini_panner(
                                ui,
                                app,
                                rect,
                                owner,
                                id("surround"),
                                &control_tip(
                                    tr("Surround Panner (double-click: open; Ctrl+drag: fine)"),
                                    track,
                                ),
                            );
                        }
                    } else if pan_too {
                        let pan = app.control_value(tid, TrackControl::Pan);
                        let label = if pan.abs() < 0.005 {
                            tr("Center").to_string()
                        } else if pan < 0.0 {
                            trf!("{}% L", (-pan * 100.0).round())
                        } else {
                            trf!("{}% R", (pan * 100.0).round())
                        };
                        let typed = crate::fmt::num((pan * 100.0).round() as f64, 0);
                        let p_rect = match tier {
                            Tier::Named => {
                                let name = painter.text(
                                    pos2(middle, y),
                                    Align2::LEFT_CENTER,
                                    tr("Pan:"),
                                    font.clone(),
                                    theme::text(),
                                );
                                let at = middle + (name.width() + 12.0).max(46.0);
                                let text = painter.text(
                                    pos2(at, y),
                                    Align2::LEFT_CENTER,
                                    label,
                                    font,
                                    theme::text(),
                                );
                                value_text(view, ui, app, tid, HeaderValue::Pan, text, typed);
                                let left = text.right().max(at + 36.0) + 10.0;
                                Rect::from_min_max(
                                    pos2(left, y - 8.0),
                                    pos2((right - 2.0).max(left + 30.0), y + 8.0),
                                )
                            }
                            Tier::Values => {
                                let px = vol.right() + 10.0;
                                let text = painter.text(
                                    pos2(px, y),
                                    Align2::LEFT_CENTER,
                                    label,
                                    font,
                                    theme::text(),
                                );
                                value_text(view, ui, app, tid, HeaderValue::Pan, text, typed);
                                Rect::from_min_max(
                                    pos2(px + 44.0, y - 8.0),
                                    pos2((right - 2.0).max(px + 74.0), y + 8.0),
                                )
                            }
                            _ => Rect::from_min_max(
                                pos2(middle + 4.0, y - 8.0),
                                pos2(right - 2.0, y + 8.0),
                            ),
                        };
                        let pkey = id("pan");
                        let (moved, released) = slider(
                            ui,
                            p_rect,
                            pkey,
                            (pan + 1.0) / 2.0,
                            0.5,
                            Thumb::Triangle,
                            &control_tip(tr("Pan (double-click: center; Ctrl+drag: fine)"), track),
                            auto,
                        );
                        if let Some(v) = moved {
                            app.set_controls(tid, TrackControl::Pan, v * 2.0 - 1.0, pkey);
                        }
                        if released {
                            app.release_controls(tid, TrackControl::Pan);
                        }
                    }
                }
            }
        }
        // Third row (audio, room enough, busses or chains): the
        // multipurpose slider.
        if row.kind == TrackKind::Audio
            && r.width() >= 190.0
            && r.height() >= 66.0 + out_h
            && !(project.busses.is_empty() && project.fx_chains.is_empty())
        {
            send_row(
                view,
                ui,
                app,
                project,
                track,
                r.translate(vec2(0.0, out_h)),
                strip.right() + 6.0,
                send_right,
            );
        }
        if row.kind == TrackKind::Audio && meters {
            let meter = app.track_meters.get(&track.id);
            let m = meter_rect(r, across);
            // The mouse wheel over it: the meters' range, as VEGAS's.
            let over = ui.interact(m, id("medidor"), Sense::click());
            if over.hovered() {
                // A range a notch (a smooth wheel's bits add up).
                let (notches, _) = ui.input(|i| crate::widgets::wheel_notches(&i.events));
                view.wheel_rest += notches;
                let n = view.wheel_rest.trunc();
                view.wheel_rest -= n;
                if n != 0.0 {
                    let range = &mut app.settings.track_meter_range;
                    *range = range.rolled(n as i32);
                }
            }
            let over = over.on_hover_text(tr("Output Meter"));
            if out_h > 0.0 {
                // "Out", the meter, the peak right of it.
                ui.painter().text(
                    pos2(strip.right() + 6.0, m.center().y),
                    Align2::LEFT_CENTER,
                    crate::i18n::trc("meter", "Out"),
                    FontId::proportional(11.0),
                    theme::text(),
                );
                paint_meter_across(ui, m, meter, app.settings.track_meter_range);
                if let Some(peak) = meter_peak(meter) {
                    let color = if peak >= 0.0 {
                        Color32::from_rgb(0xFF, 0x40, 0x30)
                    } else {
                        theme::text()
                    };
                    ui.painter().text(
                        pos2(r.right() - 6.0, m.center().y),
                        Align2::RIGHT_CENTER,
                        crate::fmt::num(peak.min(99.0) as f64, 1),
                        FontId::proportional(10.0),
                        color,
                    );
                }
            } else {
                paint_meter(ui, m, meter, app.settings.track_meter_range);
            }
            if out_h == 0.0
                && m.top() - r.top() > 3.0
                && let Some(peak) = meter_peak(meter)
            {
                let text = crate::fmt::num(peak.min(99.0) as f64, 1);
                let font = FontId::proportional(10.0);
                if peak >= 0.0 {
                    // Over 0 dB: white on a red box the meter's width.
                    let bx = Rect::from_min_max(
                        pos2(m.left(), r.top() + 3.0),
                        pos2(m.right(), r.top() + 16.0),
                    );
                    ui.painter()
                        .rect_filled(bx, 0.0, Color32::from_rgb(0xFF, 0, 0));
                    ui.painter().text(
                        bx.center(),
                        Align2::CENTER_CENTER,
                        text,
                        font,
                        Color32::from_gray(0xF0),
                    );
                } else {
                    ui.painter().text(
                        pos2(m.right(), r.top() + 9.0),
                        Align2::RIGHT_CENTER,
                        text,
                        font,
                        theme::text(),
                    );
                }
            }
            // Its own shortcut menu (the header's elsewhere).
            let tid = track.id;
            over.context_menu(|ui| meter_menu(ui, app, tid));
        }

        // Double-click on the name renames; right-click opens the track menu.
        if body.double_clicked()
            && body
                .interact_pointer_pos()
                .is_some_and(|p| name_rect.contains(p))
        {
            view.renaming = Some((track.id, scribble_name(track)));
        }
        if body.secondary_clicked() {
            view.context = super::lanes::ContextTarget::Track(track.id);
        }
        body.context_menu(|ui| header_menu(ui, view, app, project, track, &shown));
    }
    // Below the tracks: VEGAS's menu for opening and inserting tracks.
    let bottom = l.rows.last().map_or(l.headers.top(), |r| r.top + r.height);
    if bottom < l.headers.bottom() {
        let empty = Rect::from_min_max(pos2(l.headers.left(), bottom), l.headers.max);
        let resp = ui.interact(empty, egui::Id::new("cabecalhos-vazio"), Sense::click());
        resp.context_menu(|ui| super::shortcut_menus::empty_menu(ui, app));
    }
}

/// Audio: the bus button, as VEGAS's: the Master's mark, or the letter of
/// the bus the track plays into; its menu routes it to the Master or a
/// bus.
fn bus_button(
    ui: &egui::Ui,
    app: &mut BananaApp,
    project: &Project,
    track: &Track,
    rect: Rect,
    id: egui::Id,
) {
    let resp = ui.interact(rect, id, Sense::click());
    let painter = ui.painter();
    if resp.hovered() {
        painter.rect_filled(rect, 2.0, theme::hover());
    }
    let name = |o: Output| crate::i18n::output_name(project, o);
    match track.output {
        Output::Master | Output::Hardware(_) => crate::icons::paint(
            painter,
            rect.shrink(2.0),
            crate::icons::Icon::BusSelect,
            theme::text(),
        ),
        Output::Bus(b) => {
            let letter = project
                .bus_index(b)
                .map_or('?', bv_timeline::busses::bus_letter);
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                letter.to_string(),
                FontId::proportional(13.5),
                theme::banana(),
            );
        }
    }
    let resp = resp.on_hover_text(trf!("Bus: {}", name(track.output)));
    let (tid, output) = (track.id, track.output);
    let outputs: Vec<Output> = std::iter::once(Output::Master)
        .chain(project.busses.iter().map(|b| Output::Bus(b.id)))
        .collect();
    egui::Popup::menu(&resp).show(|ui| {
        for o in outputs {
            if crate::widgets::radio(ui, o == output, name(o)).clicked() {
                app.modify(|p| p.set_track_output(tid, o));
                ui.close();
            }
        }
    });
}

/// Audio: the multipurpose slider, as VEGAS's: its label picks the bus or
/// assignable effects chain it sends to, the slider sets the send's level
/// (double-click: 0 dB; Ctrl+drag: fine steps), and a right-click takes
/// it from before or after the track's volume, panned as the track or
/// not.
#[allow(clippy::too_many_arguments)]
fn send_row(
    view: &mut TimelineView,
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    project: &Project,
    track: &Track,
    r: Rect,
    x0: f32,
    right: f32,
) {
    let targets: Vec<SendTo> = project
        .busses
        .iter()
        .map(|b| SendTo::Bus(b.id))
        .chain(project.fx_chains.iter().map(|c| SendTo::Fx(c.id)))
        .collect();
    let Some(&first) = targets.first() else {
        return;
    };
    let tid = track.id;
    let to = view
        .send_targets
        .get(&tid)
        .copied()
        .filter(|t| targets.contains(t))
        .unwrap_or(first);
    let send = track
        .sends
        .iter()
        .find(|s| s.to == to)
        .copied()
        .unwrap_or_else(|| Send::new(to));
    let name = |to: SendTo| match to {
        SendTo::Bus(b) => crate::i18n::bus_name(project, b),
        SendTo::Fx(c) => crate::i18n::chain_name(project, c),
    };
    let y = r.top() + 58.0;
    let font = FontId::proportional(11.0);
    // The label: the target (yellow from before the volume), its menu the
    // others; a right-click, the send's options.
    let label = Rect::from_min_size(pos2(x0 - 2.0, y - 8.0), vec2(48.0, 16.0));
    let lr = ui.interact(
        label,
        egui::Id::new(("cabecalho", tid.0, "envio-alvo")),
        Sense::click(),
    );
    let painter = ui.painter();
    if lr.hovered() {
        painter.rect_filled(label, 2.0, theme::hover());
    }
    let color = if send.pre_volume {
        theme::banana()
    } else {
        theme::text()
    };
    crate::widgets::fit_text(
        painter,
        pos2(label.left() + 2.0, y),
        Align2::LEFT_CENTER,
        name(to),
        font.clone(),
        color,
        label.width() - 12.0,
    );
    painter.text(
        pos2(label.right() - 2.0, y),
        Align2::RIGHT_CENTER,
        "▾",
        FontId::proportional(9.0),
        theme::text_dim(),
    );
    let lr = lr.on_hover_text(tr(
        "Multipurpose slider: the bus or assignable FX it sends to",
    ));
    egui::Popup::menu(&lr).show(|ui| {
        for t in &targets {
            if crate::widgets::radio(ui, *t == to, name(*t)).clicked() {
                view.send_targets.insert(tid, *t);
                ui.close();
            }
        }
    });
    lr.context_menu(|ui| send_options(ui, app, tid, send));
    // The level (with Automation Settings, the send's automation), and
    // the slider.
    let control = TrackControl::Send(to);
    let auto = track.automation.controls;
    let gain = app.control_value(tid, control);
    let text = if gain <= 1e-4 {
        "−∞ dB".to_string()
    } else {
        format!("{} dB", crate::fmt::num((20.0 * gain.log10()) as f64, 1))
    };
    let text = painter.text(
        pos2(label.right() + 4.0, y),
        Align2::LEFT_CENTER,
        text,
        font,
        theme::text(),
    );
    let s_rect = Rect::from_min_max(
        pos2(text.left() + 46.0, y - 8.0),
        pos2((right - 4.0).max(text.left() + 76.0), y + 8.0),
    );
    let key = egui::Id::new(("cabecalho", tid.0, "envio"));
    let tip = if send.pre_volume {
        tr(
            "Send level, from before the volume (double-click: 0 dB; Ctrl+drag: fine; right-click: options)",
        )
    } else {
        tr(
            "Send level, from after the volume (double-click: 0 dB; Ctrl+drag: fine; right-click: options)",
        )
    };
    let tip = control_tip(tip, track);
    let pos = (gain / MAX_GAIN).max(0.0).sqrt();
    let unity = (1.0 / MAX_GAIN).sqrt();
    let (moved, released, resp) =
        slider_response(ui, s_rect, key, pos, unity, Thumb::Knob, &tip, auto);
    if let Some(v) = moved {
        app.set_controls(tid, control, v * v * MAX_GAIN, key);
    }
    if released {
        app.release_controls(tid, control);
    }
    resp.context_menu(|ui| send_options(ui, app, tid, send));
}

/// A send's options (the multipurpose slider's right-click): from before
/// or after the volume; panned as the track or from the middle.
fn send_options(ui: &mut egui::Ui, app: &mut BananaApp, tid: TrackId, send: Send) {
    for (label, pre) in [(tr("Pre Volume"), true), (tr("Post Volume"), false)] {
        if crate::widgets::radio(ui, send.pre_volume == pre, label).clicked() {
            app.modify(|p| {
                p.set_send(
                    tid,
                    Send {
                        pre_volume: pre,
                        ..send
                    },
                )
            });
            ui.close();
        }
    }
    ui.separator();
    let mut link = send.link_pan;
    if crate::widgets::checkbox(ui, &mut link, tr("Link to Main Track Pan")).clicked() {
        app.modify(|p| {
            p.set_send(
                tid,
                Send {
                    link_pan: link,
                    ..send
                },
            )
        });
        ui.close();
    }
}

/// A track header's shortcut menu, as VEGAS's for its kind: More (the ⋯
/// menu), renaming, the tracks to insert, its recording and automation
/// (audio), envelopes, switches, fade colors (video), its group and color,
/// the output meters (audio).
fn header_menu(
    ui: &mut egui::Ui,
    view: &mut TimelineView,
    app: &mut BananaApp,
    project: &Project,
    track: &Track,
    shown: &[Spot],
) {
    let (tid, kind) = (track.id, track.kind);
    // The track commands act on the track the menu is for.
    app.selected_track = Some(tid);
    ui.set_min_width(260.0);
    sub(ui, tr("More"), true, |ui| {
        more_menu(ui, view, app, project, track, shown)
    });
    ui.separator();
    if keyed_item(ui, tr("Rename"), "F2") {
        view.renaming = Some((tid, scribble_name(track)));
    }
    ui.separator();
    // Above this track, as VEGAS's.
    match kind {
        TrackKind::Video => {
            if keyed_item(ui, tr("Insert Video Track"), "Ctrl+Shift+Q") {
                app.insert_track_above(kind, tid);
            }
            if keyed_item(ui, tr("Insert Adjustment Track"), "Ctrl+Alt+Shift+Q") {
                app.insert_adjustment_track(Some(tid));
            }
        }
        TrackKind::Audio => {
            if keyed_item(ui, tr("Insert Audio Track"), "Ctrl+Q") {
                app.insert_track_above(kind, tid);
            }
        }
    }
    if item(ui, tr("Duplicate Track")) {
        app.duplicate_track(tid);
    }
    if item(ui, tr("Delete Track")) {
        app.request_delete_track(tid);
    }
    ui.separator();
    if kind == TrackKind::Audio {
        if toggle_item(ui, tr("Arm for Record"), track.armed) {
            app.toggle_arm(tid);
        }
        sub(ui, tr("Input"), true, |ui| {
            record_input_menu(ui, app, project, tid)
        });
        sub(ui, tr("Output"), false, |_| {});
        sub(ui, tr("Automation"), true, |ui| {
            automation_items(ui, app, project, tid)
        });
        sub(ui, tr("Pan Type"), true, |ui| pan_type_items(ui, app, tid));
        ui.separator();
    }
    sub(ui, tr("Insert/Remove Envelope"), true, |ui| {
        super::shortcut_menus::envelope_items(ui, app, kind)
    });
    if kind == TrackKind::Audio {
        sub(ui, tr("FX Automation Envelopes"), true, |ui| {
            app.fx_automation_items(ui, bv_timeline::EnvelopeOwner::Track(tid))
        });
    }
    ui.separator();
    sub(ui, tr("Switches"), true, |ui| {
        if toggle_item(ui, tr("Mute"), track.muted) {
            let v = !track.muted;
            app.edit(|p| p.set_track_muted(tid, v));
        }
        if toggle_item(ui, "Solo", track.solo) {
            let v = !track.solo;
            app.edit(|p| p.set_track_solo(tid, v));
        }
    });
    if kind == TrackKind::Video {
        sub(ui, tr("Fade Colors"), true, |ui| {
            super::shortcut_menus::fade_color_items(ui, app, tid)
        });
        if toggle_item(ui, tr("Expand Track Layers"), view.layers.contains(&tid)) {
            view.toggle_layers(tid);
            ui.close();
        }
        // Its keyframes area: its plug-in effects' and Track Motion's
        // keyframes (with View › Show OFX Effect Envelopes (Beta)).
        if toggle_item(
            ui,
            tr("Expand Track Keyframes"),
            view.track_keys.contains_key(&tid),
        ) {
            if view.track_keys.remove(&tid).is_none() {
                view.track_keys.insert(tid, 0);
                view.show_ofx_envelopes = true;
                view.show_track_keys = true;
            }
            ui.close();
        }
    }
    ui.separator();
    if item(ui, tr("Set Default Track Properties…")) {
        app.open_track_defaults(tid);
    }
    ui.separator();
    sub(ui, tr("Track Group"), true, |ui| {
        super::groups::track_group_items(ui, app)
    });
    // One of the eight colors of Options › Preferences › Display.
    sub(ui, tr("Track Display Color"), true, |ui| {
        let now = track.color_index();
        for i in 0..bv_timeline::TRACK_PALETTE as usize {
            let color = app.settings.prefs.track_color(i);
            let name = crate::settings::track_color_name(i);
            if color_item(ui, color, name, now == i).clicked() {
                app.modify(|p| {
                    let _ = p.set_track_color(tid, Some(i as u8));
                });
                ui.close();
            }
        }
    });
    if kind == TrackKind::Audio {
        ui.separator();
        let vertical = !app.settings.track_meters_across;
        if toggle_item(ui, tr("Use Vertical Meters"), vertical) {
            app.settings.track_meters_across = vertical;
        }
        let shown = !app.settings.hide_track_meters;
        if toggle_item(ui, tr("Show Output Meters"), shown) {
            // Kept with the settings, saved on leaving.
            app.settings.hide_track_meters = shown;
        }
    }
}

/// The Pan Type submenu (the track header's, the Mixing Console's pan):
/// how the track's pan and its pan envelope place the sound.
pub(crate) fn pan_type_items(ui: &mut egui::Ui, app: &mut BananaApp, tid: TrackId) {
    let now = app.project().track(tid).map(|t| t.pan_mode);
    for mode in bv_timeline::PanMode::ALL {
        if crate::widgets::radio(ui, now == Some(mode), tr(mode.name())).clicked() {
            app.modify(|p| {
                if let Some(t) = p.track_by_id_mut(tid) {
                    t.pan_mode = mode;
                }
            });
            ui.close();
        }
    }
}

/// A line of the Track Color menu: the color's swatch and its name.
fn color_item(ui: &mut egui::Ui, color: Color32, name: &str, selected: bool) -> egui::Response {
    let h = ui.spacing().interact_size.y;
    let font = egui::TextStyle::Button.resolve(ui.style());
    let galley = ui
        .painter()
        .layout_no_wrap(name.to_string(), font, theme::text());
    let w = (galley.size().x + 40.0).max(ui.available_width());
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    let painter = ui.painter();
    if resp.hovered() {
        painter.rect_filled(rect, 2.0, theme::hover());
    } else if selected {
        painter.rect_filled(rect, 2.0, theme::header_selected());
    }
    let swatch = Rect::from_min_size(
        pos2(rect.left() + 4.0, rect.center().y - 6.0),
        vec2(18.0, 12.0),
    );
    painter.rect_filled(swatch, 2.0, color);
    painter.rect_stroke(
        swatch,
        2.0,
        Stroke::new(1.0, theme::separator()),
        egui::StrokeKind::Inside,
    );
    let at = pos2(
        swatch.right() + 8.0,
        rect.center().y - galley.size().y / 2.0,
    );
    painter.galley(at, galley, theme::text());
    resp.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, name)
    });
    resp
}

/// The bottom edges of the headers, over every header (the next track's
/// header would otherwise take the press): dragging one resizes its track.
fn edges_ui(view: &mut TimelineView, ui: &egui::Ui, l: &Layout) {
    let clip = ui.clip_rect().intersect(l.headers.expand2(vec2(0.0, 3.0)));
    for row in &l.rows {
        let r = row.rect(l.headers.left(), l.headers.right());
        let edge = Rect::from_min_max(
            pos2(r.left(), r.bottom() - 3.0),
            pos2(r.right(), r.bottom() + 3.0),
        );
        if !edge.intersects(clip) {
            continue;
        }
        let id = egui::Id::new(("cabecalho", row.track.0, "borda"));
        let resp = ui.interact(edge.intersect(clip), id, Sense::drag());
        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        if resp.drag_started() {
            // The track's own height: its keyframes row apart.
            view.track_drag = Some(TrackDrag::Resize {
                track: row.track,
                top: r.top() + row.keys,
            });
        }
        resp.on_hover_text(tr("Drag to change the track's height"));
    }
}

/// Height `height` for track `track`, whose header's edge is dragged: for
/// all the tracks selected with it, as VEGAS's.
fn resize_tracks(view: &mut TimelineView, app: &BananaApp, track: TrackId, height: f32) {
    let picked = app.track_selection();
    let tracks = if picked.contains(&track) {
        picked
    } else {
        vec![track]
    };
    for t in tracks {
        view.set_track_height(t, height);
    }
}

/// A track header or its edge being dragged: the track's new height while
/// resizing; while reordering, the line where it will land, and the move
/// when it is dropped.
pub(crate) fn drag_ui(view: &mut TimelineView, ui: &egui::Ui, app: &mut BananaApp, l: &Layout) {
    edges_ui(view, ui, l);
    let Some(drag) = view.track_drag else {
        return;
    };
    let (pointer, down) = ui.input(|i| (i.pointer.interact_pos(), i.pointer.primary_down()));
    // A track carried near or past the list's top or bottom: the tracks
    // scroll, as VEGAS's.
    let pointer = match drag {
        TrackDrag::Reorder(_) | TrackDrag::Group(_) if down => {
            pointer.map(|p| pos2(p.x, view.edge_scroll_y(ui, l, p.y)))
        }
        _ => pointer,
    };
    match drag {
        TrackDrag::Resize { track, top } => {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
            if let Some(p) = pointer {
                resize_tracks(view, app, track, p.y - top);
            }
        }
        TrackDrag::Group(group) => {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            let project = app.project().clone();
            if let Some((index, y, _)) = pointer.and_then(|p| drop_slot(l, &project, p.y)) {
                // Never inside another group: after it.
                let index = l
                    .groups
                    .iter()
                    .filter(|g| g.group != group)
                    .find_map(|g| {
                        let m = project.track_group_members(g.group);
                        let a = project.track_index(*m.first()?)?;
                        let b = project.track_index(*m.last()?)?;
                        (index > a && index <= b).then_some(b + 1)
                    })
                    .unwrap_or(index);
                let x0 = l.headers.left();
                let x1 = l.lanes.right();
                let painter = ui.painter().with_clip_rect(
                    Rect::from_min_max(pos2(x0, l.headers.top()), pos2(x1, l.headers.bottom()))
                        .expand2(vec2(0.0, 2.0)),
                );
                painter.line_segment(
                    [pos2(x0, y), pos2(x1, y)],
                    Stroke::new(3.0, theme::banana()),
                );
                if !down {
                    app.modify(|p| p.move_track_group_to(group, index));
                }
            }
        }
        TrackDrag::Reorder(track) => {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            let project = app.project().clone();
            if let Some((index, y, into)) = pointer.and_then(|p| drop_slot(l, &project, p.y)) {
                let x0 = l.headers.left();
                let x1 = l.lanes.right();
                let painter = ui.painter().with_clip_rect(
                    Rect::from_min_max(pos2(x0, l.headers.top()), pos2(x1, l.headers.bottom()))
                        .expand2(vec2(0.0, 2.0)),
                );
                painter.line_segment(
                    [pos2(x0, y), pos2(x1, y)],
                    Stroke::new(3.0, theme::banana()),
                );
                if !down {
                    let from = app.project().track_index(track);
                    // Dropped below itself: one place less once it is taken
                    // out.
                    let to = match from {
                        Some(f) if f < index => index - 1,
                        _ => index,
                    };
                    // Into a group's rows it joins the group; out of them it
                    // leaves its group.
                    let now = project.track(track).and_then(|t| t.group);
                    if from != Some(to) || now != into {
                        app.modify(|p| {
                            let _ = p.move_track_to(track, to);
                            p.set_track_group(track, into);
                        });
                    }
                }
            }
        }
    }
    if !down {
        view.track_drag = None;
    }
}

/// A slider's tip, saying when it is an automation control and how its
/// track's automation goes.
fn control_tip(tip: &str, track: &Track) -> String {
    automation_tip(tip, track.automation)
}

/// `tip`, and the automation control's mode when it is one.
pub(super) fn automation_tip(tip: &str, auto: bv_timeline::TrackAutomation) -> String {
    if auto.controls {
        trf!(
            "{} · automation control: {}",
            tip,
            crate::commands::automation_mode_name(auto.mode)
        )
    } else {
        tip.to_string()
    }
}

/// Activate Automation Controls, and the Automation Settings submenu.
fn automation_items(ui: &mut egui::Ui, app: &mut BananaApp, project: &Project, id: TrackId) {
    let auto = project.track(id).map(|t| t.automation).unwrap_or_default();
    if mark_item(ui, tr("Activate Automation Controls"), auto.controls) {
        app.toggle_automation_controls(id);
    }
    sub(ui, tr("Automation Settings"), true, |ui| {
        for m in bv_timeline::AutomationMode::ALL {
            if mark_item(ui, crate::commands::automation_mode_name(m), auto.mode == m) {
                app.set_automation_mode(id, m);
            }
        }
    });
}

/// The compositing modes as radio items, `now` the track's (`parent`:
/// its Parent Composite Mode's); Custom runs a compositor plug-in (the
/// Compositors window's).
fn composite_modes(
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    id: TrackId,
    now: bv_timeline::CompositeMode,
    parent: bool,
) {
    for m in bv_timeline::CompositeMode::MENU {
        if crate::widgets::radio(ui, now == m, tr(m.name())).clicked() {
            app.modify(|p| {
                if let Some(t) = p.track_by_id_mut(id) {
                    if parent {
                        t.parent_mode = m;
                    } else {
                        t.composite_mode = m;
                    }
                }
            });
            ui.close();
        }
    }
    ui.separator();
    let custom = bv_timeline::CompositeMode::Custom;
    if crate::widgets::radio(ui, now == custom, tr("Custom")).clicked() {
        app.choose_custom_compositing(id, parent);
        ui.close();
    }
}

/// A compositing parent's Parent Composite Mode submenu.
fn parent_mode_sub(ui: &mut egui::Ui, app: &mut BananaApp, project: &Project, id: TrackId) {
    if let Some(t) = project
        .track(id)
        .filter(|_| project.is_compositing_parent(id))
    {
        let now = t.parent_mode;
        sub(ui, tr("Parent Composite Mode"), true, |ui| {
            composite_modes(ui, app, id, now, true)
        });
    }
}

/// The Compositing Mode button's menu: its modes (a parent's Parent
/// Composite Mode first).
fn compositing_menu(ui: &mut egui::Ui, app: &mut BananaApp, project: &Project, id: TrackId) {
    let Some(now) = project.track(id).map(|t| t.composite_mode) else {
        return;
    };
    if project.is_compositing_parent(id) {
        parent_mode_sub(ui, app, project, id);
        ui.separator();
    }
    composite_modes(ui, app, id, now, false);
}

/// Mute's click without Ctrl: the mute envelope's with Automation
/// Settings on, else the whole track's.
fn toggle_mute(app: &mut BananaApp, track: &Track, key: egui::Id) {
    if track.automation.controls {
        app.click_mute(crate::automation::AutoOwner::Track(track.id), key);
    } else {
        let (tid, v) = (track.id, !track.muted);
        app.edit(|p| p.set_track_muted(tid, v));
    }
}

/// Solo's click without Ctrl.
fn toggle_solo(app: &mut BananaApp, track: &Track) {
    let (tid, v) = (track.id, !track.solo);
    app.edit(|p| p.set_track_solo(tid, v));
}

/// The More (⋯) menu, as VEGAS's: the commands of the buttons the header
/// does not show (`shown`: those it does, hidden by Edit Visible Button
/// Set or for want of room), then Edit Visible Button Set.
fn more_menu(
    ui: &mut egui::Ui,
    view: &mut TimelineView,
    app: &mut BananaApp,
    project: &Project,
    track: &Track,
    shown: &[Spot],
) {
    let hidden = |s: Spot| !shown.contains(&s);
    match track.kind {
        TrackKind::Video => video_more_menu(ui, view, app, project, track, &hidden),
        TrackKind::Audio => audio_more_menu(ui, view, app, project, track, &hidden),
    }
    ui.separator();
    if item(ui, tr("Edit Visible Button Set…")) {
        app.open_header_buttons(track.kind);
    }
}

/// More's Mute and Solo, when the header does not show them.
fn mute_solo_items(
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    track: &Track,
    hidden: &dyn Fn(Spot) -> bool,
) {
    if hidden(Spot::Button(HeaderButton::Mute)) && mark_item(ui, tr("Mute"), track.muted) {
        let key = egui::Id::new(("cabecalho", track.id.0, "mute"));
        toggle_mute(app, track, key);
    }
    if hidden(Spot::Button(HeaderButton::Solo)) && mark_item(ui, tr("Solo"), track.solo) {
        toggle_solo(app, track);
    }
}

/// More's Minimize and Maximize, when the header does not show them.
fn height_items(
    ui: &mut egui::Ui,
    view: &mut TimelineView,
    project: &Project,
    id: TrackId,
    hidden: &dyn Fn(Spot) -> bool,
) {
    if hidden(Spot::Minimize) && item(ui, tr("Minimize")) {
        view.minimize_track(id);
    }
    if hidden(Spot::Maximize) && item(ui, tr("Maximize")) {
        view.maximize_track(project, id);
    }
}

/// The video track's More (⋯) menu, in VEGAS's order (the user's prints:
/// Bypass Motion Blur, Track Motion, Activate Automation Controls,
/// Automation Settings, Compositing Mode, Make Compositing Parent and
/// Child, Minimize, Maximize, Color Grading).
fn video_more_menu(
    ui: &mut egui::Ui,
    view: &mut TimelineView,
    app: &mut BananaApp,
    project: &Project,
    track: &Track,
    hidden: &dyn Fn(Spot) -> bool,
) {
    use HeaderButton as B;
    let id = track.id;
    if hidden(Spot::Button(B::BypassMotionBlur))
        && mark_item(ui, tr("Bypass Motion Blur"), track.bypass_motion_blur)
    {
        app.toggle_bypass_motion_blur(id);
        ui.close();
    }
    if hidden(Spot::Button(B::TrackMotion))
        && item_enabled(ui, tr("Track Motion"), !track.adjustment).clicked()
    {
        app.open_track_motion(id, false);
        ui.close();
    }
    // A compositing parent moves its whole group.
    if project.is_compositing_parent(id)
        && item_enabled(ui, tr("Parent Motion"), !track.adjustment).clicked()
    {
        app.open_track_motion(id, true);
        ui.close();
    }
    if hidden(Spot::Button(B::TrackFx)) && item(ui, tr("Track FX")) {
        app.clip_windows.track_fx = Some(id);
    }
    if hidden(Spot::Button(B::Automation)) {
        automation_items(ui, app, project, id);
    }
    mute_solo_items(ui, app, track, hidden);
    parent_mode_sub(ui, app, project, id);
    if hidden(Spot::Button(B::CompositingMode)) {
        let now = track.composite_mode;
        sub(ui, tr("Compositing Mode"), true, |ui| {
            composite_modes(ui, app, id, now, false)
        });
    }
    let family = ui.input(|i| i.modifiers.command);
    if hidden(Spot::Button(B::MakeParent))
        && item_enabled(ui, tr("Make Compositing Parent"), track.composite_depth > 0).clicked()
    {
        app.edit(|p| p.make_compositing_parent(id, family));
        ui.close();
    }
    if hidden(Spot::Button(B::MakeChild))
        && item_enabled(ui, tr("Make Compositing Child"), project.can_make_child(id))
            .on_hover_text(tr("Ctrl: with its children"))
            .clicked()
    {
        app.edit(|p| p.make_compositing_child(id, family));
        ui.close();
    }
    height_items(ui, view, project, id, hidden);
    if hidden(Spot::Button(B::ColorGrading)) && item(ui, tr("Color Grading")) {
        app.selected_track = Some(id);
        crate::tabs::color::open_at(app, crate::tabs::color::Level::Track);
    }
}

/// The audio track's More (⋯) menu, in VEGAS's order.
fn audio_more_menu(
    ui: &mut egui::Ui,
    view: &mut TimelineView,
    app: &mut BananaApp,
    project: &Project,
    track: &Track,
    hidden: &dyn Fn(Spot) -> bool,
) {
    use HeaderButton as B;
    let id = track.id;
    if hidden(Spot::Button(B::ArmForRecord))
        && hidden(Spot::Armed)
        && mark_item(ui, tr("Arm For Record"), track.armed)
    {
        app.toggle_arm(id);
    }
    if hidden(Spot::Button(B::InvertPhase))
        && mark_item(ui, tr("Invert Track Phase"), track.invert_phase)
    {
        invert_phase(app, track);
    }
    if hidden(Spot::Button(B::Automation)) {
        automation_items(ui, app, project, id);
    }
    if hidden(Spot::Button(B::TrackFx)) && item(ui, tr("Track FX")) {
        app.clip_windows.track_fx = Some(id);
    }
    mute_solo_items(ui, app, track, hidden);
    sub(ui, tr("Output Bus"), false, |_| {});
    if hidden(Spot::Button(B::RecordInput)) {
        sub(ui, tr("Record Input"), true, |ui| {
            record_input_menu(ui, app, project, id)
        });
    }
    height_items(ui, view, project, id, hidden);
    // Auto Ducking: this track's sound lowering others', or lowered.
    let controller = track.ducking.is_some();
    if hidden(Spot::Button(B::DuckController))
        && mark_item(ui, tr("Auto Ducking Controller"), controller)
    {
        app.toggle_duck_controller(id);
    }
    if controller && item(ui, tr("Auto Ducking Controls…")) {
        app.duck_dialog = Some(crate::ducking_ui::DuckTarget::Track(id));
    }
    if hidden(Spot::Button(B::DuckListener))
        && mark_item(ui, tr("Auto Ducking Listener"), track.duck_listener)
    {
        app.toggle_duck_listener(id);
    }
}

/// Record Input: each input device, with its stereo pairs and its single
/// channels; monitoring the input is to come.
pub(crate) fn record_input_menu(
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    project: &Project,
    id: TrackId,
) {
    use bv_timeline::RecordInput;
    let current = project
        .track(id)
        .and_then(|t| t.record_input.clone())
        .unwrap_or_default();
    let devices = app.input_devices();
    if devices.is_empty() {
        ui.add_enabled(false, egui::Button::new(tr("No audio input device")));
    }
    for d in &devices {
        // The system's default device is kept as "the default": it follows
        // the system's choice.
        let device = if d.default {
            String::new()
        } else {
            d.name.clone()
        };
        let label = if d.default {
            trf!("{} (default)", d.name)
        } else {
            d.name.clone()
        };
        sub(ui, &label, true, |ui| {
            let n = d.channels.max(1);
            let pairs = (0..n.saturating_sub(1))
                .step_by(2)
                .map(|first| RecordInput {
                    device: device.clone(),
                    first,
                    channels: 2,
                    bus: None,
                });
            let singles = (0..n).map(|first| RecordInput {
                device: device.clone(),
                first,
                channels: 1,
                bus: None,
            });
            for input in pairs.chain(singles) {
                let on = current == input;
                if toggle_item(ui, &crate::recording::input_label(&input), on) {
                    app.set_record_input(id, input);
                }
            }
        });
    }
    // An input bus: its input through its effects.
    let busses: Vec<(bv_timeline::InputBusId, String)> = project
        .input_busses
        .iter()
        .map(|b| (b.id, crate::i18n::input_bus_name(project, b.id)))
        .collect();
    sub(ui, tr("Input Busses"), !busses.is_empty(), |ui| {
        for (b, name) in &busses {
            if toggle_item(ui, name, current.bus == Some(*b)) {
                app.set_record_input(
                    id,
                    RecordInput {
                        bus: Some(*b),
                        ..RecordInput::default()
                    },
                );
            }
        }
    });
    ui.separator();
    // Input monitoring: what it records heard through its effects.
    use bv_timeline::InputMonitor;
    let monitor = project
        .track(id)
        .map_or(InputMonitor::Off, |t| t.input_monitor);
    for (m, label) in [
        (InputMonitor::Off, tr("Input Monitoring Off")),
        (InputMonitor::On, tr("Input Monitoring On")),
        (InputMonitor::Auto, tr("Input Monitoring Auto")),
    ] {
        if toggle_item(ui, label, monitor == m) {
            app.modify(|p| {
                if let Some(t) = p.track_by_id_mut(id) {
                    t.input_monitor = m;
                }
            });
        }
    }
}

/// The name being edited: Enter or a click elsewhere keeps it, Esc cancels.
/// An empty name gives the track back its default one.
fn rename_field(
    view: &mut TimelineView,
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    rect: Rect,
    track: TrackId,
    id: egui::Id,
) {
    let Some((_, text)) = view.renaming.as_mut() else {
        return;
    };
    let resp = ui.put(
        rect,
        egui::TextEdit::singleline(text)
            .id(id)
            .font(FontId::proportional(12.5))
            .margin(vec2(3.0, 1.0)),
    );
    if !resp.has_focus() && !resp.lost_focus() {
        resp.request_focus();
    }
    let cancel = ui.input(|i| i.key_pressed(Key::Escape));
    if resp.lost_focus() || cancel {
        let name = view
            .renaming
            .take()
            .map(|(_, n)| n.trim().to_string())
            .unwrap_or_default();
        let Some(t) = app.project().track(track) else {
            return;
        };
        if cancel || name == scribble_name(t) {
            return;
        }
        let name = if name.is_empty() {
            default_name(app.project(), track)
        } else {
            name
        };
        app.modify(|p| {
            if let Some(t) = p.track_by_id_mut(track) {
                t.name = name;
            }
        });
    }
}

/// "Video 2": the name BananaV gives a track by its kind and position.
fn default_name(project: &Project, id: TrackId) -> String {
    let Some(t) = project.track(id) else {
        return String::new();
    };
    let n = project
        .tracks
        .iter()
        .filter(|o| o.kind == t.kind)
        .position(|o| o.id == id)
        .map_or(1, |i| i + 1);
    match t.kind {
        TrackKind::Video => format!("Video {n}"),
        TrackKind::Audio => format!("Audio {n}"),
    }
}

impl BananaApp {
    /// Every audio track's meter lit at `peaks` (linear, a channel each),
    /// for screen tests.
    /// The meters across the headers (Use Vertical Meters off), on
    /// settings never saved (screen tests).
    pub fn track_meters_across_for_review(&mut self) {
        self.settings = crate::settings::UiSettings::in_memory();
        self.settings.track_meters_across = true;
    }

    pub fn track_meters_for_review(&mut self, peaks: Vec<f32>) {
        let audio: Vec<TrackId> = self
            .project()
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Audio)
            .map(|t| t.id)
            .collect();
        for id in audio {
            self.track_meters
                .entry(id)
                .or_default()
                .update(Some(peaks.clone()));
        }
    }

    /// Where the first audio track's output meter is (the timeline as last
    /// drawn), the settings swapped for ones never saved since the mouse
    /// wheel there changes them, for screen tests.
    pub fn track_meter_wheel_for_review(&mut self) -> Option<Pos2> {
        self.settings = crate::settings::UiSettings::in_memory();
        let view = &self.timeline;
        let l = view.layout(view.last_rect, self.project());
        let row = l.rows.iter().find(|r| r.kind == TrackKind::Audio)?;
        let across = self.settings.track_meters_across;
        Some(meter_rect(row.rect(l.headers.left(), l.headers.right()), across).center())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A video header of `width` (its color strip too) with every button,
    /// laid out as `ui` does: the buttons it shows, top row then second.
    fn shown_at(width: f32, set: &[HeaderButton]) -> Vec<Spot> {
        let left = STRIP_W + 4.0;
        ButtonsFor {
            kind: TrackKind::Video,
            set,
            extras: &[],
            adjustment: false,
        }
        .lay_out(left, width - 3.5, 12.5, Some((36.0, width - 6.5)))
        .into_iter()
        .map(|(s, _)| s)
        .collect()
    }

    /// Every button, as the user's VEGAS 23 prints show them: in a wide
    /// list all of them; at 215 pixels Maximize and Minimize gone; at 123
    /// only Automation Settings, Mute and Solo left on the top row; the
    /// compositing buttons at the second row's end each time.
    #[test]
    fn the_buttons_go_from_the_left_as_the_list_narrows() {
        use HeaderButton as B;
        let all = HeaderButton::VIDEO;
        let second = [
            Spot::Button(B::MakeChild),
            Spot::Button(B::MakeParent),
            Spot::Button(B::CompositingMode),
        ];
        let mut wide = vec![
            Spot::Button(B::Solo),
            Spot::Button(B::Mute),
            Spot::Button(B::Automation),
            Spot::Button(B::TrackFx),
            Spot::Button(B::TrackMotion),
            Spot::Button(B::ColorGrading),
            Spot::Button(B::BypassMotionBlur),
            Spot::Minimize,
            Spot::Maximize,
        ];
        wide.extend(second);
        assert_eq!(shown_at(1098.0, &all), wide);
        let mut mid = wide[..7].to_vec();
        mid.extend(second);
        assert_eq!(shown_at(215.0, &all), mid);
        let mut narrow = wide[..3].to_vec();
        narrow.extend(second);
        assert_eq!(shown_at(123.0, &all), narrow);
        // VEGAS's at first: Mute and Solo alone.
        assert_eq!(
            shown_at(215.0, &HeaderButton::DEFAULT),
            [Spot::Button(B::Solo), Spot::Button(B::Mute)]
        );
        // Hidden ones leave no gap: Color Grading next to Mute.
        assert_eq!(
            shown_at(400.0, &[B::Mute, B::ColorGrading]),
            [Spot::Button(B::Mute), Spot::Button(B::ColorGrading)]
        );
    }

    /// The top row's buttons where VEGAS's are (the user's prints, from the
    /// header's right edge): Solo's middle 15.5 pixels in, a button every
    /// 24, Automation Settings 34 with its arrow; the compositing buttons
    /// every 20, Make Compositing Child's middle 16.5 in.
    #[test]
    fn the_buttons_sit_where_vegas_puts_them() {
        let cells = ButtonsFor {
            kind: TrackKind::Video,
            set: &HeaderButton::VIDEO,
            extras: &[],
            adjustment: false,
        }
        .lay_out(26.0, 1098.0 - 3.5, 12.5, Some((36.0, 1098.0 - 6.5)));
        let middle = |s: Spot| {
            1098.0
                - cells
                    .iter()
                    .find(|(x, _)| *x == s)
                    .map(|(_, c)| c.center().x)
                    .unwrap()
        };
        use HeaderButton as B;
        assert!((middle(Spot::Button(B::Solo)) - 15.5).abs() < 0.01);
        assert!((middle(Spot::Button(B::Mute)) - 39.5).abs() < 0.01);
        assert!((middle(Spot::Button(B::TrackFx)) - 97.5).abs() < 0.01);
        assert!((middle(Spot::Maximize) - 217.5).abs() < 0.01);
        assert!((middle(Spot::Button(B::MakeChild)) - 16.5).abs() < 0.01);
        assert!((middle(Spot::Button(B::CompositingMode)) - 56.5).abs() < 0.01);
    }

    /// An adjustment track shows Track FX whatever the set says; the
    /// track's own buttons come after Mute.
    #[test]
    fn an_adjustment_track_shows_its_effects() {
        use HeaderButton as B;
        let spots: Vec<Spot> = ButtonsFor {
            kind: TrackKind::Video,
            set: &HeaderButton::DEFAULT,
            extras: &[Spot::Restore],
            adjustment: true,
        }
        .lay_out(26.0, 400.0, 12.5, None)
        .into_iter()
        .map(|(s, _)| s)
        .collect();
        assert_eq!(
            spots,
            [
                Spot::Button(B::Solo),
                Spot::Button(B::Mute),
                Spot::Restore,
                Spot::Button(B::TrackFx),
            ]
        );
    }

    #[test]
    fn default_names_stay_out_of_the_scribble_strip() {
        let mut p = Project::default();
        let v = p.tracks[0].id;
        assert_eq!(scribble_name(p.track(v).unwrap()), "");
        p.track_by_id_mut(v).unwrap().name = "Interview".into();
        assert_eq!(scribble_name(p.track(v).unwrap()), "Interview");
        assert_eq!(default_name(&p, v), "Video 1");
    }

    #[test]
    fn minimize_and_maximize_toggle() {
        let p = Project::default();
        let (v, a) = (&p.tracks[0], p.tracks[1].id);
        let mut view = TimelineView::default();
        let normal = view.height_of(v);
        view.minimize_track(v.id);
        assert_eq!(view.height_of(v), super::super::MIN_TRACK_H);
        view.minimize_track(v.id);
        assert_eq!(view.height_of(v), normal, "again: back to the default");
        view.maximize_track(&p, v.id);
        assert_eq!(view.height_of(v), view.lanes_height);
        assert!(!view.track_heights.contains_key(&a), "only that track");
    }

    /// The edge of one of several tracks selected (Shift) sizes them all
    /// alike, as VEGAS's; a track not among them alone.
    #[test]
    fn the_selected_tracks_take_one_height() {
        let mut app = BananaApp::new(egui::Context::default());
        let (a1, a2, a3) = app.modify(|p| {
            (
                p.add_track(TrackKind::Audio),
                p.add_track(TrackKind::Audio),
                p.add_track(TrackKind::Audio),
            )
        });
        app.pick_track(a1, false, false);
        app.pick_track(a2, true, false);
        let mut view = TimelineView::default();
        resize_tracks(&mut view, &app, a2, 120.0);
        assert_eq!(view.track_heights.get(&a1), Some(&120.0));
        assert_eq!(view.track_heights.get(&a2), Some(&120.0));
        assert_eq!(view.track_heights.get(&a3), None);
        resize_tracks(&mut view, &app, a3, 90.0);
        assert_eq!(view.track_heights.get(&a3), Some(&90.0));
        assert_eq!(
            view.track_heights.get(&a1),
            Some(&120.0),
            "a track apart alone"
        );
    }

    #[test]
    fn typed_values() {
        let db = |t: &str| parse_volume(t).map(|g| (20.0 * g.log10() * 10.0).round() / 10.0);
        assert_eq!(db("-6"), Some(-6.0));
        assert_eq!(db("3,5 dB"), Some(3.5));
        assert_eq!(db("20"), Some(12.0), "+12 dB at most");
        assert_eq!(parse_volume("-inf"), Some(0.0));
        assert_eq!(parse_volume("x"), None);
        assert_eq!(parse_pan("Center"), Some(0.0));
        assert_eq!(parse_pan("50 L"), Some(-0.5));
        assert_eq!(parse_pan("30% R"), Some(0.3));
        assert_eq!(parse_pan("40 E"), Some(-0.4));
        assert_eq!(parse_pan("-25"), Some(-0.25));
        assert_eq!(parse_pan("200"), Some(1.0));
    }

    #[test]
    fn volume_taper_puts_unity_near_the_middle() {
        let unity = (1.0 / MAX_GAIN).sqrt();
        assert!((unity - 0.5).abs() < 0.01, "{unity}");
        let gain = |pos: f32| pos * pos * MAX_GAIN;
        assert!((gain(1.0) - MAX_GAIN).abs() < 1e-6);
        assert!((20.0 * gain(unity).log10()).abs() < 1e-3);
    }

    /// A video and an audio track picked in the track list, then a
    /// right-click on one of them: they stay picked and the menu's Delete
    /// Track deletes them both, as VEGAS's, in one undo.
    #[test]
    fn delete_track_deletes_the_tracks_picked() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut app = BananaApp::new(egui::Context::default());
        app.welcome_for_review("closed");
        app.load_demo();
        let tracks = app.project().tracks.len();
        let (video, audio) = (app.project().tracks[0].id, app.project().tracks[1].id);
        let mut h = Harness::builder()
            .with_size(egui::vec2(1600.0, 960.0))
            .build_ui_state(
                |ui, app: &mut BananaApp| {
                    crate::apply_theme(ui.ctx());
                    app.frame(ui);
                },
                app,
            );
        h.run_steps(4);
        h.state_mut().pick_track(video, false, false);
        h.state_mut().pick_track(audio, false, true);
        let at = h
            .state()
            .track_header_spot_for_review(1)
            .expect("its header");
        h.event(egui::Event::PointerMoved(at));
        h.step();
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            h.step();
        }
        h.step();
        assert_eq!(
            h.state().track_selection(),
            vec![video, audio],
            "still picked"
        );
        let item = h
            .get_all_by_label("Delete Track")
            .last()
            .expect("the menu's Delete Track")
            .rect()
            .center();
        h.event(egui::Event::PointerMoved(item));
        h.step();
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos: item,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            h.step();
        }
        h.step();
        let p = h.state().project();
        assert!(p.track(video).is_none() && p.track(audio).is_none());
        assert_eq!(p.tracks.len(), tracks - 2);
        h.state_mut().undo();
        assert_eq!(h.state().project().tracks.len(), tracks, "one undo");
    }
}
