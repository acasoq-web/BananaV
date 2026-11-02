//! Mixing Console (View › Window): a channel strip per audio track, per
//! assignable effects chain and per audio bus, and the master strip.
//!
//! As VEGAS's: a toolbar (the project's sound format; Views — the Channel
//! List and the View Controls; Properties and Layout — the channels and
//! regions shown, their labels, the fader ticks, the channel width and the
//! meters; Downmix and Dim; inserting tracks, chains and busses), the
//! Channel List (a checkbox per channel; a click picks it, Ctrl and Shift
//! pick several for ganged edits: their faders, pans, mutes and solos move
//! together) and the View
//! Controls (the channel types and the control regions shown), and each
//! strip's regions: Insert FX (its effects as buttons: a click shows one,
//! the right-click shows, bypasses, removes or replaces it or loads a
//! preset; + adds one), Sends (a track's to busses and chains, a bus's to
//! chains, from before or after the volume; a chain's input level), I/O (a
//! track's recording input; the output: the Master or a bus, never in a
//! circle), VU Meters (VU or PPM, `vu_meters.rs`), Meters (peaks) and
//! Faders (automation, mute, solo, phase, arm, pan, the fader — its value
//! typed with a double-click, the mouse wheel — and a bus's or chain's
//! effects before or after its volume), the name at the bottom (a bus or
//! chain renamed with a double-click or F2, deleted with Delete or its
//! menu). A channel dragged by its name moves to another place (a track
//! in the project, a bus or chain among its kind); the Sends region's
//! bottom divider shows more or fewer sends (the mouse wheel scrolls
//! them). The keys of the MixingConsole keyboard context change what it
//! shows through [`run`]; a right-click on the console does too.

use crate::i18n::tr;
use bv_engine::meters::{MeterKind, MeterOf};
use bv_timeline::{
    BusId, Effect, EffectKind, Output, Project, Send, SendTo, Strip, TrackId, TrackKind,
};

use crate::app::BananaApp;
use crate::automation::{AutoOwner, TrackControl};
use crate::commands::Cmd;
use crate::egui::{self, Align2, Color32, FontId, Rect, RichText, Sense, Stroke, pos2, vec2};
use crate::fx_window::FxTarget;
use crate::icons::Icon;
use crate::keymap::Extra;
use crate::theme;
use crate::vu_meters::VuOptions;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChannelWidth {
    Narrow,
    #[default]
    Default,
    Wide,
}

impl ChannelWidth {
    const ALL: [ChannelWidth; 3] = [
        ChannelWidth::Narrow,
        ChannelWidth::Default,
        ChannelWidth::Wide,
    ];

    /// A strip's width (VEGAS's default strips are 82 points wide, a line
    /// between them).
    fn px(self) -> f32 {
        match self {
            ChannelWidth::Narrow => 68.0,
            ChannelWidth::Default => 82.0,
            ChannelWidth::Wide => 120.0,
        }
    }

    fn label(self) -> &'static str {
        match self {
            ChannelWidth::Narrow => tr("Narrow"),
            ChannelWidth::Default => tr("Default"),
            ChannelWidth::Wide => tr("Wide"),
        }
    }
}

/// A strip's control regions, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    InsertFx,
    Sends,
    Io,
    VuMeters,
    Meters,
    Faders,
}

impl Region {
    const ALL: [Region; 6] = [
        Region::InsertFx,
        Region::Sends,
        Region::Io,
        Region::VuMeters,
        Region::Meters,
        Region::Faders,
    ];

    /// Its name: the View Controls' buttons, its label in the strips.
    fn label(self) -> &'static str {
        match self {
            Region::InsertFx => tr("Insert FX"),
            Region::Sends => tr("Sends"),
            Region::Io => tr("I/O"),
            Region::VuMeters => tr("VU Meters"),
            Region::Meters => tr("Meters"),
            Region::Faders => tr("Faders"),
        }
    }

    /// Its line in Show Control Regions.
    fn menu_label(self) -> &'static str {
        match self {
            Region::InsertFx => tr("Insert FX Control Region"),
            Region::Sends => tr("Sends Control Region"),
            Region::Io => tr("I/O Control Region"),
            Region::VuMeters => tr("VU Meters Control Region"),
            Region::Meters => tr("Peak Meters Control Region"),
            Region::Faders => tr("Faders Control Region"),
        }
    }
}

/// Keyboard commands of the MixingConsole context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleOp {
    Width(ChannelWidth),
    /// Hides the selected track's channel.
    HideChannel,
    /// Shows every channel type and the hidden channels again.
    ShowAllChannels,
    ToggleTracks,
    ToggleMaster,
    /// The busses' channels (U) and the assignable effects chains' (E).
    ToggleBusses,
    /// The input busses' channels.
    ToggleInputs,
    ToggleFx,
    /// Every control region shown.
    ShowAllRegions,
    ToggleRegion(Region),
    /// Views: the Channel List (Shift+C) and the View Controls (Shift+R).
    ToggleChannelList,
    ToggleViewControls,
    /// Show Control Region Labels (Shift+L), Show Fader Ticks (Shift+T).
    ToggleLabels,
    ToggleTicks,
    /// Deletes the selected bus or chain (else the selected track).
    Delete,
    /// Renames the selected bus or chain (else the selected track).
    Rename,
}

/// What the console shows.
#[derive(Clone, Debug)]
pub struct ConsoleView {
    pub width: ChannelWidth,
    pub show_tracks: bool,
    pub show_master: bool,
    /// The Preview Bus's channel: the level media previews play at, and
    /// their meters.
    pub show_preview: bool,
    pub show_busses: bool,
    /// The input busses' channels.
    pub show_inputs: bool,
    pub show_fx: bool,
    /// The control regions shown (as [`Region::ALL`]).
    pub regions: [bool; 6],
    pub labels: bool,
    pub ticks: bool,
    pub channel_list: bool,
    pub view_controls: bool,
    pub hidden: Vec<TrackId>,
    pub hidden_strips: Vec<Strip>,
    /// The bus or chain picked (its name clicked): Delete and F2 act on it.
    pub selected: Option<Strip>,
    /// The busses and chains picked with Ctrl and Shift (the selected one
    /// among them): ganged edits move them together, as the tracks picked
    /// do (`BananaApp::track_selection`).
    pub picks: Vec<Strip>,
    /// The Sends region's rows, its bottom divider dragged (none: as many
    /// as there are sends), and the first one shown.
    pub send_rows: Option<usize>,
    pub send_first: usize,
    /// The divider's drag: its rows when it began, how far it went.
    send_drag: Option<(usize, f32)>,
    /// A channel being dragged by its name, and where each channel's strip
    /// was drawn.
    dragging: Option<Of>,
    strip_rects: Vec<(Of, Rect)>,
    /// A bus's or chain's name being typed.
    pub renaming: Option<(Strip, String)>,
    /// An input bus's name being typed.
    pub renaming_input: Option<(bv_timeline::InputBusId, String)>,
    /// A fader's value being typed.
    pub typing: Option<(MeterOf, String)>,
    /// The VU/PPM meters: their kind, labels, peaks, held peaks and
    /// valleys, Vintage Mode, and the channels whose meters are hidden.
    pub vu_kind: MeterKind,
    pub vu_labels: bool,
    pub vu_peaks: bool,
    pub hold_peaks: bool,
    pub hold_valleys: bool,
    pub vintage: bool,
    pub vu_hidden: Vec<MeterOf>,
    /// The peak meters' scale (of `METER_SCALES`).
    pub meter_scale: usize,
    /// The left pane's width and the Channel List's height (their
    /// splitters dragged).
    pub pane_w: f32,
    pub list_h: f32,
    /// The Master picked (its strip blue).
    pub master_picked: bool,
}

impl Default for ConsoleView {
    fn default() -> ConsoleView {
        ConsoleView {
            width: ChannelWidth::Default,
            show_tracks: true,
            show_master: true,
            show_preview: false,
            show_busses: true,
            show_inputs: true,
            show_fx: true,
            // VEGAS 23's: the I/O and the Faders (their meters beside the
            // faders); Insert FX, Sends and the meters apart hidden.
            regions: [false, false, true, false, false, true],
            labels: false,
            ticks: true,
            channel_list: true,
            view_controls: true,
            hidden: Vec::new(),
            hidden_strips: Vec::new(),
            selected: None,
            picks: Vec::new(),
            send_rows: None,
            send_first: 0,
            send_drag: None,
            dragging: None,
            strip_rects: Vec::new(),
            renaming: None,
            renaming_input: None,
            typing: None,
            vu_kind: MeterKind::TraditionalVu,
            vu_labels: true,
            vu_peaks: false,
            hold_peaks: true,
            hold_valleys: false,
            vintage: false,
            vu_hidden: Vec::new(),
            meter_scale: DEFAULT_SCALE,
            pane_w: PANE_W,
            list_h: LIST_H,
            master_picked: false,
        }
    }
}

impl ConsoleView {
    pub fn shows(&self, r: Region) -> bool {
        let i = Region::ALL.iter().position(|x| *x == r).unwrap_or(0);
        self.regions[i]
    }

    fn toggle(&mut self, r: Region) {
        let i = Region::ALL.iter().position(|x| *x == r).unwrap_or(0);
        self.regions[i] = !self.regions[i];
    }

    fn vu_options(&self) -> VuOptions {
        VuOptions {
            kind: self.vu_kind,
            labels: self.vu_labels,
            peaks: self.vu_peaks,
            hold_peaks: self.hold_peaks,
            hold_valleys: self.hold_valleys,
        }
    }

    /// The VU/PPM meters the mixer must compute while the console shows
    /// them.
    pub(crate) fn vu_wanted(&self) -> Option<MeterKind> {
        self.shows(Region::VuMeters).then_some(self.vu_kind)
    }
}

/// Runs a Mixing Console keyboard command.
pub(crate) fn run(app: &mut BananaApp, op: ConsoleOp) {
    let selected = app.selected_track.filter(|&id| {
        app.project()
            .track(id)
            .is_some_and(|t| t.kind == TrackKind::Audio)
    });
    let strip = app
        .console
        .selected
        .filter(|s| app.project().strip_channel(*s).is_some());
    match (op, strip) {
        (ConsoleOp::Delete, Some(s)) => {
            app.modify(|p| p.remove_strip(s));
            app.console.selected = None;
            return;
        }
        (ConsoleOp::Delete, None) => return app.run_extra(Extra::DeleteTrack),
        (ConsoleOp::Rename, Some(s)) => {
            let name = strip_name(app.project(), s);
            app.console.renaming = Some((s, name));
            return;
        }
        (ConsoleOp::Rename, None) => return app.run_extra(Extra::RenameTrack),
        _ => {}
    }
    let v = &mut app.console;
    match op {
        ConsoleOp::Width(w) => v.width = w,
        ConsoleOp::HideChannel => {
            if let Some(id) = selected
                && !v.hidden.contains(&id)
            {
                v.hidden.push(id);
            }
        }
        ConsoleOp::ShowAllChannels => {
            v.show_tracks = true;
            v.show_master = true;
            v.show_preview = true;
            v.show_busses = true;
            v.show_inputs = true;
            v.show_fx = true;
            v.hidden.clear();
            v.hidden_strips.clear();
        }
        ConsoleOp::ToggleTracks => v.show_tracks = !v.show_tracks,
        ConsoleOp::ToggleMaster => v.show_master = !v.show_master,
        ConsoleOp::ToggleBusses => v.show_busses = !v.show_busses,
        ConsoleOp::ToggleInputs => v.show_inputs = !v.show_inputs,
        ConsoleOp::ToggleFx => v.show_fx = !v.show_fx,
        ConsoleOp::ShowAllRegions => v.regions = [true; 6],
        ConsoleOp::ToggleRegion(r) => v.toggle(r),
        ConsoleOp::ToggleChannelList => v.channel_list = !v.channel_list,
        ConsoleOp::ToggleViewControls => v.view_controls = !v.view_controls,
        ConsoleOp::ToggleLabels => v.labels = !v.labels,
        ConsoleOp::ToggleTicks => v.ticks = !v.ticks,
        ConsoleOp::Delete | ConsoleOp::Rename => {}
    }
}

/// The busses and chains picked: those picked with Ctrl and Shift while
/// they hold the selected one, else the selected one.
fn strip_selection(v: &ConsoleView) -> Vec<Strip> {
    match v.selected {
        Some(s) if v.picks.contains(&s) => v.picks.clone(),
        Some(s) => vec![s],
        None => Vec::new(),
    }
}

/// A click on bus or chain `s`: with Shift the channels from the selected
/// one to it, with Ctrl it added or taken away, else it alone.
fn pick_strip(app: &mut BananaApp, s: Strip, shift: bool, ctrl: bool) {
    let all = app.project().strips();
    let v = &mut app.console;
    v.master_picked = false;
    if shift
        && let Some(from) = v.selected.and_then(|x| all.iter().position(|y| *y == x))
        && let Some(to) = all.iter().position(|y| *y == s)
    {
        v.picks = all[from.min(to)..=from.max(to)].to_vec();
        return;
    }
    if ctrl {
        let mut picks = strip_selection(v);
        match picks.iter().position(|x| *x == s) {
            Some(i) if picks.len() > 1 => {
                picks.remove(i);
                v.selected = picks.first().copied();
            }
            Some(_) => {}
            None => {
                picks.push(s);
                v.selected = Some(s);
            }
        }
        v.picks = picks;
        return;
    }
    v.selected = Some(s);
    v.picks = vec![s];
}

/// The tracks a change to track `id`'s strip goes to: the tracks picked
/// when it is among them, else it alone.
fn ganged_tracks(app: &BananaApp, id: TrackId) -> Vec<TrackId> {
    let sel = app.track_selection();
    if sel.contains(&id) { sel } else { vec![id] }
}

/// [`ganged_tracks`] of a bus or chain.
fn ganged_strips(app: &BananaApp, s: Strip) -> Vec<Strip> {
    let sel = strip_selection(&app.console);
    if sel.contains(&s) { sel } else { vec![s] }
}

/// Where an item at `from` goes, dropped before or after the one at `to`
/// (its place once taken out).
fn place(from: usize, to: usize, after: bool) -> usize {
    let at = if after { to + 1 } else { to };
    if from < at { at - 1 } else { at }
}

/// Channel `from` dropped before or after channel `to`: a track moves in
/// the project, a bus or chain among its kind.
fn move_channel(app: &mut BananaApp, from: Of, to: Of, after: bool) {
    match (from, to) {
        (Of::Track(a), Of::Track(b)) => {
            let p = app.project();
            let (Some(i), Some(j)) = (p.track_index(a), p.track_index(b)) else {
                return;
            };
            app.edit(|p| p.move_track_to(a, place(i, j, after)));
        }
        (Of::Strip(a), Of::Strip(b)) => {
            let kind: Vec<Strip> = app
                .project()
                .strips()
                .into_iter()
                .filter(|s| std::mem::discriminant(s) == std::mem::discriminant(&a))
                .collect();
            let (Some(i), Some(j)) = (
                kind.iter().position(|s| *s == a),
                kind.iter().position(|s| *s == b),
            ) else {
                return;
            };
            app.modify(|p| p.move_strip_to(a, place(i, j, after)));
        }
        _ => {}
    }
}

/// Whether channels `a` and `b` are of a kind (one can take the other's
/// place).
fn same_kind(a: Of, b: Of) -> bool {
    match (a, b) {
        (Of::Track(_), Of::Track(_)) => true,
        (Of::Strip(x), Of::Strip(y)) => std::mem::discriminant(&x) == std::mem::discriminant(&y),
        _ => false,
    }
}

/// A channel dragged by its name: a line where it would go; let go over
/// another channel of its kind, it moves there.
fn drag_channels(ui: &mut egui::Ui, app: &mut BananaApp) {
    let Some(from) = app.console.dragging else {
        return;
    };
    let released = ui.input(|i| i.pointer.any_released());
    if let Some(pos) = ui.input(|i| i.pointer.latest_pos()) {
        let target = app
            .console
            .strip_rects
            .iter()
            .copied()
            .find(|(of, r)| r.x_range().contains(pos.x) && same_kind(from, *of));
        if let Some((to, r)) = target
            && to != from
        {
            let after = pos.x > r.center().x;
            let x = if after {
                r.right() + 1.5
            } else {
                r.left() - 1.5
            };
            ui.painter()
                .vline(x, r.y_range(), Stroke::new(3.0, theme::banana()));
            if released {
                move_channel(app, from, to, after);
            }
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    if released {
        app.console.dragging = None;
    }
}

/// A bus's or chain's name as shown.
pub(crate) fn strip_name(p: &Project, s: Strip) -> String {
    match s {
        Strip::Bus(id) => crate::i18n::bus_name(p, id),
        Strip::Chain(id) => crate::i18n::chain_name(p, id),
    }
}

/// The faders' range (dB), a send's and an effect's row, the meters'
/// regions.
const MIN_DB: f32 = -60.0;
const MAX_DB: f32 = 12.0;
const SEND_ROW: f32 = 18.0;
const FX_ROW: f32 = 19.0;
const METERS_H: f32 = 116.0;
const LABEL_H: f32 = 13.0;
/// The Sends region's bottom divider.
const DIVIDER_H: f32 = 6.0;
/// The left pane's width at first and its bounds, the dark gap between it
/// and the strips (its splitter), the Channel List's height at first and
/// its bounds.
const PANE_W: f32 = 180.0;
const PANE_MIN: f32 = 120.0;
const PANE_MAX: f32 = 480.0;
const PANE_GAP: f32 = 5.0;
const LIST_H: f32 = 120.0;
const LIST_MIN: f32 = 40.0;
const LIST_MAX: f32 = 600.0;
/// The Meters region's height: a meter for each channel, as VEGAS's.
const PEAKS_H: f32 = 82.0;
/// VEGAS's colors for automation: Read's blue (and the fx button's dot),
/// and the red of the automation controls, Latch and the arm button.
const READ_BLUE: Color32 = Color32::from_rgb(0x69, 0x8A, 0xFF);
const AUTOMATION_RED: Color32 = Color32::from_rgb(0xFF, 0x7B, 0x5F);
/// A strip's rows, as VEGAS 23's: its header (its badge and its kind), an
/// I/O row, the automation row, the pan (its line and its value), a row of
/// the button column, the value under the fader, the name at its foot.
const HEADER_H: f32 = 24.0;
const IO_ROW: f32 = 23.0;
const AUTO_ROW: f32 = 26.0;
const PAN_H: f32 = 40.0;
const BUTTON_H: f32 = 22.0;
const VALUE_H: f32 = 22.0;
const NAME_H: f32 = 22.0;
/// The button column's width and the meter's beside the fader.
const BUTTONS_W: f32 = 22.0;
const SIDE_METER_W: f32 = 20.0;
/// Room under the strips for the horizontal scroll bar.
const SCROLL_ROOM: f32 = 12.0;
/// The console's meters' scales: their floor (dB), numbers every and
/// ticks every (dB). VEGAS's console counts 90 dB numbered every 6; the
/// mouse wheel over a meter goes through them.
const METER_SCALES: [(f32, u32, u32); 4] =
    [(-24.0, 3, 3), (-48.0, 6, 3), (-90.0, 6, 3), (-120.0, 12, 6)];
const DEFAULT_SCALE: usize = 2;

fn gain_to_db(g: f32) -> f32 {
    if g <= 1e-4 { MIN_DB } else { 20.0 * g.log10() }
}

fn db_to_gain(db: f32) -> f32 {
    if db <= MIN_DB + 0.5 {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}

/// A level as the strips write it: "−∞" at the bottom.
fn level_text(db: f32) -> String {
    if db <= MIN_DB + 0.05 {
        "−∞".to_string()
    } else {
        crate::fmt::signed(db as f64, 1)
    }
}

/// A typed level: dB, or "-inf" (at or under −60: silence).
fn parse_db(text: &str) -> Option<f32> {
    let t = text.trim().to_lowercase();
    if t.contains("inf") || t.contains('∞') {
        return Some(MIN_DB);
    }
    let db = crate::fmt::parse(t.trim_end_matches("db").trim())? as f32;
    Some(db.clamp(MIN_DB, MAX_DB))
}

/// A channel strip's owner.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Of {
    Track(TrackId),
    Strip(Strip),
}

impl Of {
    fn meter(self) -> MeterOf {
        match self {
            Of::Track(id) => MeterOf::Track(id),
            Of::Strip(s) => MeterOf::Strip(s),
        }
    }

    fn fx_target(self) -> FxTarget {
        match self {
            Of::Track(id) => FxTarget::Track(id),
            Of::Strip(Strip::Bus(id)) => FxTarget::Bus(id),
            Of::Strip(Strip::Chain(id)) => FxTarget::Chain(id),
        }
    }
}

/// The sizes every strip shares, so their rows line up.
struct Sizes {
    width: f32,
    /// A strip's whole height.
    height: f32,
    insert_fx: f32,
    sends: f32,
    /// The send rows shown and how many there are.
    send_rows: usize,
    send_total: usize,
    io: f32,
    /// The I/O region's rows: two when a strip has an input (a track's
    /// recording input, an input bus's source), the output in the last.
    io_rows: usize,
    fader: f32,
}

/// A strip's badge: a track's number in its color, a bus's letter, an
/// icon, or none (the Preview's).
#[derive(Clone, Copy)]
enum Badge {
    Number(usize, Color32),
    Letter(char),
    Icon(Icon),
    None,
}

/// A bus's badge (its letter) or a chain's (its icon).
fn strip_badge(p: &Project, s: Strip) -> Badge {
    match s {
        Strip::Bus(id) => Badge::Letter(bv_timeline::busses::bus_letter(
            p.bus_index(id).unwrap_or(0),
        )),
        Strip::Chain(_) => Badge::Icon(Icon::AssignableFx),
    }
}

/// Paints `badge` in `rect` (its text `size` points), with VEGAS's mark of
/// the picked channel in its corner.
fn paint_badge(painter: &egui::Painter, rect: Rect, badge: Badge, size: f32, picked: bool) {
    let mark = |color: Color32| {
        if picked {
            painter.rect_filled(
                Rect::from_min_size(rect.min + vec2(1.0, 1.0), vec2(3.0, 3.0)),
                0.0,
                color,
            );
        }
    };
    match badge {
        Badge::Number(n, color) => {
            painter.rect_filled(rect, 0.0, color);
            painter.text(
                rect.right_center() - vec2(3.0, 0.0),
                Align2::RIGHT_CENTER,
                n.to_string(),
                FontId::proportional(size),
                Color32::BLACK,
            );
            mark(Color32::WHITE);
        }
        Badge::Letter(c) => {
            painter.rect(
                rect,
                0.0,
                theme::panel_dark().lerp_to_gamma(theme::panel(), 0.5),
                Stroke::new(1.0, theme::panel_dark()),
                egui::StrokeKind::Inside,
            );
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                c.to_string(),
                FontId::proportional(size + 2.0),
                theme::text_strong(),
            );
            mark(Color32::BLACK);
        }
        Badge::Icon(icon) => crate::icons::paint(
            painter,
            Rect::from_center_size(rect.center(), vec2(rect.height(), rect.height())),
            icon,
            theme::text(),
        ),
        Badge::None => {}
    }
}

pub fn ui(ui: &mut egui::Ui, app: &mut BananaApp) {
    app.console_drawn = true;
    toolbar(ui, app);
    let view = app.console.clone();
    let p = app.project();
    let tracks: Vec<TrackId> = p
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Audio && view.show_tracks && !view.hidden.contains(&t.id))
        .map(|t| t.id)
        .collect();
    let mut strips = Vec::new();
    if view.show_fx {
        strips.extend(p.fx_chains.iter().map(|c| Strip::Chain(c.id)));
    }
    if view.show_busses {
        strips.extend(p.busses.iter().map(|b| Strip::Bus(b.id)));
    }
    // The input busses' strips.
    let inputs: Vec<bv_timeline::InputBusId> = if view.show_inputs {
        p.input_busses.iter().map(|b| b.id).collect()
    } else {
        Vec::new()
    };

    strips.retain(|s| !view.hidden_strips.contains(s));
    // The regions' heights: as many rows as the strip that needs most.
    let mut fx_rows = 1;
    for id in &tracks {
        fx_rows = fx_rows.max(p.track(*id).map_or(0, |t| t.effects.len()) + 1);
    }
    for s in &strips {
        fx_rows = fx_rows.max(p.strip_channel(*s).map_or(0, |c| c.effects.len()) + 1);
    }
    for id in &inputs {
        fx_rows = fx_rows_of_input(p, *id, fx_rows);
    }
    if view.show_master {
        fx_rows = fx_rows.max(p.master_fx.len() + 1);
    }
    let mut send_rows = 0;
    if !tracks.is_empty() {
        send_rows = p.busses.len() + p.fx_chains.len();
    }
    for s in &strips {
        if let Strip::Bus(id) = s {
            send_rows = send_rows.max(bus_send_targets(p, *id).len());
        }
    }
    if strips.iter().any(|s| matches!(s, Strip::Chain(_))) {
        send_rows = send_rows.max(1);
    }
    // An input bus's sends: to the chains.
    if !inputs.is_empty() {
        send_rows = send_rows.max(p.fx_chains.len());
    }
    let io_rows = if tracks.is_empty() && inputs.is_empty() {
        1
    } else {
        2
    };
    let label = |r: Region| {
        if view.labels && view.shows(r) {
            LABEL_H
        } else {
            0.0
        }
    };
    // The header, the foot and the lines under each part.
    let mut taken =
        HEADER_H + 1.0 + 1.0 + NAME_H + Region::ALL.iter().map(|r| label(*r)).sum::<f32>();
    let insert_fx = fx_rows.min(8) as f32 * FX_ROW + 4.0;
    // As many send rows as the divider leaves (all of them at first), the
    // divider under them.
    let shown_sends = view
        .send_rows
        .map_or(send_rows, |n| n.clamp(1, send_rows.max(1)));
    let sends = if send_rows > 0 {
        shown_sends as f32 * SEND_ROW + DIVIDER_H
    } else {
        0.0
    };
    let io = io_rows as f32 * IO_ROW;
    for (r, h) in [
        (Region::InsertFx, insert_fx),
        (Region::Sends, sends),
        (Region::Io, io),
        (Region::VuMeters, METERS_H),
        (Region::Meters, PEAKS_H),
    ] {
        if view.shows(r) {
            taken += h + 1.0;
        }
    }
    let faders = view.shows(Region::Faders);
    if faders {
        taken += AUTO_ROW + PAN_H + VALUE_H;
    }
    let room = (ui.available_height() - SCROLL_ROOM).max(120.0);
    let fader = if faders {
        (room - taken).max(80.0)
    } else {
        0.0
    };
    let sizes = Sizes {
        width: view.width.px(),
        height: (taken + fader).max(room),
        insert_fx,
        sends,
        send_rows: shown_sends,
        send_total: send_rows,
        io,
        io_rows,
        fader,
    };
    app.console.strip_rects.clear();
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        if view.channel_list || view.view_controls {
            let h = ui.available_height();
            left_pane(ui, app, &view, h);
        }
        // Right-click on the console: the same view options as the keys
        // (under the strips, which take their own clicks).
        let bg = ui.interact(
            ui.available_rect_before_wrap(),
            ui.id().with("console-menu"),
            egui::Sense::click(),
        );
        bg.context_menu(|ui| layout_menu(ui, app));
        // Strips taller or wider than the window scroll.
        egui::ScrollArea::both()
            .id_salt("console-strips")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    // The Preview channel first, as VEGAS's.
                    if view.show_preview {
                        preview_strip(ui, app, &sizes, &view);
                    }
                    for id in tracks {
                        strip_ui(ui, app, Of::Track(id), &sizes, &view);
                    }
                    for s in strips {
                        strip_ui(ui, app, Of::Strip(s), &sizes, &view);
                    }
                    for id in inputs {
                        input_bus_strip(ui, app, id, &sizes, &view);
                    }
                    if view.show_master {
                        master_strip(ui, app, &sizes, &view);
                    }
                });
                drag_channels(ui, app);
            });
    });
}

/// The left pane, as VEGAS's: the Channel List over the View Controls, a
/// dark gap between it and the strips. The gap dragged makes the pane
/// wider or narrower, the band between the list and the controls the list
/// taller or shorter.
fn left_pane(ui: &mut egui::Ui, app: &mut BananaApp, view: &ConsoleView, height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(view.pane_w + PANE_GAP, height), Sense::hover());
    let body = Rect::from_min_size(rect.min, vec2(view.pane_w, height));
    let gap = Rect::from_min_max(pos2(body.right(), rect.top()), rect.max);
    let split = ui
        .interact(gap, ui.id().with("console-pane-split"), Sense::drag())
        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
    if split.dragged() {
        let w = app.console.pane_w + split.drag_delta().x;
        app.console.pane_w = w.clamp(PANE_MIN, PANE_MAX);
    }
    let painter = ui.painter();
    painter.rect_filled(body, 0.0, theme::panel());
    painter.rect_filled(gap, 0.0, splitter_color(&split));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("console-panes")
            .max_rect(body.shrink(4.0))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    egui::ScrollArea::vertical()
        .id_salt("console-panes")
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            if view.channel_list {
                channel_list(ui, app, view.list_h);
            }
            if view.channel_list && view.view_controls {
                ui.add_space(4.0);
                let (r, band) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 5.0), Sense::drag());
                let band = band.on_hover_cursor(egui::CursorIcon::ResizeVertical);
                if band.dragged() {
                    let h = app.console.list_h + band.drag_delta().y;
                    app.console.list_h = h.clamp(LIST_MIN, LIST_MAX);
                }
                ui.painter().rect_filled(r, 0.0, splitter_color(&band));
                ui.add_space(16.0);
            }
            if view.view_controls {
                view_controls(ui, app);
            }
        });
}

/// A splitter's color: dark, lit under the pointer and while dragged.
fn splitter_color(r: &egui::Response) -> Color32 {
    if r.hovered() || r.dragged() {
        theme::banana_dim()
    } else {
        theme::panel_dark()
    }
}

/// The console's toolbar, as VEGAS's: its name and the project's sound
/// format on the left; on the right Views, Properties and Layout, Downmix
/// and Dim, and inserting a track, a chain, a bus or an input bus — with
/// their texts when they fit, their icons alone otherwise.
fn toolbar(ui: &mut egui::Ui, app: &mut BananaApp) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(4.0);
        let s = app.project().settings;
        ui.label(
            RichText::new(format!("{}:", tr("Mixing Console")))
                .strong()
                .size(12.0),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new(trf!(
                "{} Hz; {} Bit",
                crate::fmt::grouped(s.sample_rate.0 as u64),
                s.bit_depth
            ))
            .size(12.0),
        );
        let inserts = [
            (Cmd::InsertAudioTrack, tr("Insert Audio Track")),
            (Cmd::InsertAssignableFx, tr("Insert Assignable FX…")),
            (Cmd::InsertAudioBus, tr("Insert Bus")),
            (Cmd::InsertInputBus, tr("Insert Input Bus")),
        ];
        let font = FontId::proportional(12.0);
        let texts: f32 = inserts
            .iter()
            .map(|(_, t)| {
                ui.painter()
                    .layout_no_wrap(t.to_string(), font.clone(), theme::text())
                    .size()
                    .x
                    + 32.0
            })
            .sum();
        let labels = ui.available_width() > texts + 160.0;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(4.0);
            for (c, text) in inserts.into_iter().rev() {
                let icon = c.icon().unwrap_or(Icon::InputBus);
                if insert_button(ui, icon, text, labels, c.enabled(app)).clicked() {
                    c.run(app);
                }
            }
            crate::widgets::toolbar_separator(ui);
            let dim = bv_engine::Engine::dim();
            if crate::widgets::icon_toggle(ui, Icon::Dim, dim, tr("Dim Output")).clicked() {
                Cmd::Dim.run(app);
            }
            let on = Cmd::Downmix.checked(app).unwrap_or(false);
            if crate::widgets::icon_toggle(ui, Icon::Downmix, on, tr("Downmix Output")).clicked() {
                Cmd::Downmix.run(app);
            }
            crate::widgets::toolbar_separator(ui);
            let more = arrow_button(ui, tr("Properties and Layout"));
            egui::Popup::menu(&more).show(|ui| layout_menu(ui, app));
            let props =
                crate::widgets::icon_button(ui, Icon::Settings, tr("Properties and Layout"));
            if props.clicked() {
                app.open_project_audio_settings();
            }
            props.context_menu(|ui| layout_menu(ui, app));
            let arrow = arrow_button(ui, tr("Views"));
            egui::Popup::menu(&arrow).show(|ui| views_menu(ui, app));
            let views = crate::widgets::icon_button(ui, Icon::Views, tr("Views"));
            egui::Popup::menu(&views).show(|ui| views_menu(ui, app));
        });
    });
}

/// Views: the Channel List and the View Controls shown or hidden.
fn views_menu(ui: &mut egui::Ui, app: &mut BananaApp) {
    let v = &mut app.console;
    crate::widgets::checkbox(ui, &mut v.channel_list, tr("Channel List"));
    crate::widgets::checkbox(ui, &mut v.view_controls, tr("View Controls"));
}

/// A toolbar button: its icon and, with `label`, its text after it.
fn insert_button(
    ui: &mut egui::Ui,
    icon: Icon,
    text: &str,
    label: bool,
    enabled: bool,
) -> egui::Response {
    let color = if enabled {
        theme::text()
    } else {
        theme::text_disabled()
    };
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), FontId::proportional(12.0), color);
    let w = if label {
        24.0 + galley.size().x + 8.0
    } else {
        26.0
    };
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 24.0), sense);
    let painter = ui.painter();
    if resp.hovered() && enabled {
        painter.rect_filled(rect, 3.0, theme::hover());
    }
    let icon_r =
        Rect::from_center_size(pos2(rect.left() + 13.0, rect.center().y), vec2(16.0, 16.0));
    crate::icons::paint(painter, icon_r, icon, color);
    if label {
        let at = pos2(rect.left() + 24.0, rect.center().y - galley.size().y / 2.0);
        painter.galley(at, galley, color);
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, text));
    if label {
        resp
    } else {
        resp.on_hover_text(text)
    }
}

/// A toolbar's arrow: the menu of the button beside it.
fn arrow_button(ui: &mut egui::Ui, tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(13.0, 24.0), Sense::click());
    let painter = ui.painter();
    if resp.hovered() {
        painter.rect_filled(rect, 3.0, theme::hover());
    }
    let c = rect.center();
    painter.add(egui::Shape::convex_polygon(
        vec![
            c + vec2(-3.5, -1.5),
            c + vec2(3.5, -1.5),
            c + vec2(0.0, 2.5),
        ],
        theme::text(),
        Stroke::NONE,
    ));
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tip));
    resp.on_hover_text(tip)
}

/// Properties and Layout's menu (and the console's right-click): Audio
/// Properties, the channels and control regions shown, their labels, the
/// fader ticks, the channel width and the meters' layout.
fn layout_menu(ui: &mut egui::Ui, app: &mut BananaApp) {
    if ui.button(tr("Audio Properties…")).clicked() {
        app.open_project_audio_settings();
        ui.close();
    }
    ui.separator();
    ui.menu_button(tr("Show Channels"), |ui| {
        if ui.button(tr("Show All Channels")).clicked() {
            run(app, ConsoleOp::ShowAllChannels);
            ui.close();
        }
        ui.separator();
        let v = &mut app.console;
        crate::widgets::checkbox(ui, &mut v.show_preview, tr("Preview Bus"));
        crate::widgets::checkbox(ui, &mut v.show_master, tr("Master Bus"));
        crate::widgets::checkbox(ui, &mut v.show_fx, tr("Assignable FX Busses"));
        crate::widgets::checkbox(ui, &mut v.show_inputs, tr("Input Busses"));
        crate::widgets::checkbox(ui, &mut v.show_busses, tr("Audio Busses"));
        crate::widgets::checkbox(ui, &mut v.show_tracks, tr("Audio Tracks"));
    });
    ui.menu_button(tr("Show Control Regions"), |ui| {
        if ui.button(tr("Show All Control Regions")).clicked() {
            run(app, ConsoleOp::ShowAllRegions);
            ui.close();
        }
        ui.separator();
        for r in [
            Region::Faders,
            Region::Meters,
            Region::VuMeters,
            Region::Io,
            Region::Sends,
            Region::InsertFx,
        ] {
            let mut on = app.console.shows(r);
            if crate::widgets::checkbox(ui, &mut on, r.menu_label()).clicked() {
                run(app, ConsoleOp::ToggleRegion(r));
            }
        }
    });
    let v = &mut app.console;
    crate::widgets::checkbox(ui, &mut v.labels, tr("Show Control Region Labels"));
    crate::widgets::checkbox(ui, &mut v.ticks, tr("Show Fader Ticks"));
    ui.menu_button(tr("Channel Width"), |ui| {
        for w in ChannelWidth::ALL {
            if crate::widgets::radio(ui, app.console.width == w, w.label()).clicked() {
                run(app, ConsoleOp::Width(w));
                ui.close();
            }
        }
    });
    ui.menu_button(tr("Meter Layout"), |ui| vu_menu(ui, app, None));
    ui.separator();
    let can_hide = app.selected_track.is_some();
    if ui
        .add_enabled(can_hide, egui::Button::new(tr("Hide Channel")))
        .clicked()
    {
        run(app, ConsoleOp::HideChannel);
        ui.close();
    }
}

/// The VU/PPM meters' menu (a meter's right-click, Meter Layout): show or
/// hide the channel's meter, Reset Clip, the kinds, labels, peaks, held
/// peaks and valleys, Vintage Mode.
fn vu_menu(ui: &mut egui::Ui, app: &mut BananaApp, of: Option<MeterOf>) {
    if let Some(of) = of {
        let mut shown = !app.console.vu_hidden.contains(&of);
        if crate::widgets::checkbox(ui, &mut shown, tr("Show VU/PPM")).clicked() {
            let hidden = &mut app.console.vu_hidden;
            if shown {
                hidden.retain(|h| *h != of);
            } else {
                hidden.push(of);
            }
        }
    }
    if ui.button(tr("Reset Clip")).clicked() {
        app.vu.reset_clips();
        ui.close();
    }
    ui.separator();
    for k in MeterKind::ALL {
        if crate::widgets::radio(ui, app.console.vu_kind == k, tr(k.name())).clicked() {
            app.console.vu_kind = k;
            ui.close();
        }
    }
    ui.separator();
    let v = &mut app.console;
    crate::widgets::checkbox(ui, &mut v.vu_labels, tr("Show Labels"));
    crate::widgets::checkbox(ui, &mut v.vu_peaks, tr("Show Peaks"));
    crate::widgets::checkbox(ui, &mut v.hold_peaks, tr("Hold Peaks"));
    crate::widgets::checkbox(ui, &mut v.hold_valleys, tr("Hold Valleys"));
    crate::widgets::checkbox(ui, &mut v.vintage, tr("Vintage Mode"));
}

/// The Channel List: a checkbox per channel (shown or hidden), its badge
/// and its name — blue while picked, as VEGAS's —, `height` tall; a click
/// on a name picks the channel.
fn channel_list(ui: &mut egui::Ui, app: &mut BananaApp, height: f32) {
    ui.spacing_mut().item_spacing = vec2(4.0, 1.0);
    egui::ScrollArea::vertical()
        .id_salt("console-channel-list")
        .min_scrolled_height(height)
        .max_height(height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let p = app.project();
            let tracks: Vec<(TrackId, String, Badge)> = p
                .tracks
                .iter()
                .enumerate()
                .filter(|(_, t)| t.kind == TrackKind::Audio)
                .map(|(i, t)| {
                    let badge = Badge::Number(i + 1, app.settings.prefs.color_of(t));
                    (t.id, crate::i18n::track_name(t), badge)
                })
                .collect();
            let strips: Vec<(Strip, String, Badge)> = p
                .strips()
                .into_iter()
                .map(|s| (s, strip_name(p, s), strip_badge(p, s)))
                .collect();
            let picked_tracks = app.track_selection();
            let picked_strips = strip_selection(&app.console);
            let master_picked = app.console.master_picked;
            let tracks_shown = app.console.selected.is_none() && !master_picked;
            let m = ui.input(|i| i.modifiers);
            for (id, name, badge) in tracks {
                let mut shown = !app.console.hidden.contains(&id);
                let picked = tracks_shown && picked_tracks.contains(&id);
                let (toggled, r) = list_row(ui, &mut shown, badge, &name, picked);
                if toggled {
                    let h = &mut app.console.hidden;
                    if shown {
                        h.retain(|x| *x != id);
                    } else {
                        h.push(id);
                    }
                }
                if r.clicked() {
                    pick_or_drag(ui, app, Of::Track(id), &r);
                }
            }
            for (s, name, badge) in strips {
                let mut shown = !app.console.hidden_strips.contains(&s);
                let (toggled, r) =
                    list_row(ui, &mut shown, badge, &name, picked_strips.contains(&s));
                if toggled {
                    let h = &mut app.console.hidden_strips;
                    if shown {
                        h.retain(|x| *x != s);
                    } else {
                        h.push(s);
                    }
                }
                if r.clicked() {
                    pick_strip(app, s, m.shift, m.command);
                }
            }
            let mut shown = app.console.show_master;
            let (toggled, r) = list_row(
                ui,
                &mut shown,
                Badge::Icon(Icon::BusSelect),
                tr("Master"),
                master_picked,
            );
            if toggled {
                app.console.show_master = shown;
            }
            if r.clicked() {
                pick_master(app);
            }
        });
}

/// A Channel List row: its checkbox, its badge and its name, blue while
/// picked. Whether the checkbox was clicked, and the name's response.
fn list_row(
    ui: &mut egui::Ui,
    shown: &mut bool,
    badge: Badge,
    name: &str,
    picked: bool,
) -> (bool, egui::Response) {
    ui.horizontal(|ui| {
        let toggled = crate::widgets::checkbox(ui, shown, "").clicked();
        let (r, _) = ui.allocate_exact_size(vec2(16.0, 14.0), Sense::hover());
        paint_badge(ui.painter(), r, badge, 9.0, false);
        let font = FontId::proportional(13.0);
        let galley = ui
            .painter()
            .layout_no_wrap(name.to_string(), font, theme::text());
        let w = (galley.size().x + 6.0).min(ui.available_width().max(1.0));
        let (rect, resp) = ui.allocate_exact_size(vec2(w, 16.0), Sense::click());
        if picked {
            ui.painter().rect_filled(rect, 0.0, theme::selection());
        } else if resp.hovered() {
            ui.painter().rect_filled(rect, 0.0, theme::hover());
        }
        crate::widgets::fit_text(
            ui.painter(),
            pos2(rect.left() + 3.0, rect.center().y),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(13.0),
            if picked {
                Color32::WHITE
            } else {
                theme::text()
            },
            w - 4.0,
        );
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
        (toggled, resp)
    })
    .inner
}

/// The View Controls, as VEGAS 23's: Show All (lit while everything shows;
/// it shows every kind of channel and both regions), the kinds of channels
/// shown, the Meters and Faders regions (every region in Properties and
/// Layout › Show Control Regions and on the console's right-click), the
/// channel width.
fn view_controls(ui: &mut egui::Ui, app: &mut BananaApp) {
    ui.vertical_centered(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        // As wide as the longest text (other languages' are longer).
        let texts = [
            tr("Show All"),
            tr("Audio Tracks"),
            tr("Audio Busses"),
            tr("Input Busses"),
            tr("Assignable FX"),
            tr("Master Bus"),
            tr("Preview Bus"),
            Region::Meters.label(),
            Region::Faders.label(),
        ];
        let widest = texts
            .iter()
            .map(|t| {
                ui.painter()
                    .layout_no_wrap(t.to_string(), FontId::proportional(12.0), theme::text())
                    .size()
                    .x
            })
            .fold(0.0, f32::max);
        let w = (widest + 14.0).clamp(92.0, ui.available_width().max(92.0));
        let all = all_shown(&app.console);
        if view_button(ui, w, tr("Show All"), Some(all)).clicked() {
            show_all(app);
        }
        let v = &mut app.console;
        for (on, label) in [
            (&mut v.show_tracks, tr("Audio Tracks")),
            (&mut v.show_busses, tr("Audio Busses")),
            (&mut v.show_inputs, tr("Input Busses")),
            (&mut v.show_fx, tr("Assignable FX")),
            (&mut v.show_master, tr("Master Bus")),
            (&mut v.show_preview, tr("Preview Bus")),
        ] {
            if view_button(ui, w, label, Some(*on)).clicked() {
                *on = !*on;
            }
        }
        ui.add_space(18.0);
        for r in [Region::Meters, Region::Faders] {
            if view_button(ui, w, r.label(), Some(app.console.shows(r))).clicked() {
                run(app, ConsoleOp::ToggleRegion(r));
            }
        }
        ui.add_space(14.0);
        width_control(ui, app);
    });
}

/// Whether the View Controls show everything: every kind of channel, none
/// hidden, the Meters and the Faders.
fn all_shown(v: &ConsoleView) -> bool {
    v.show_tracks
        && v.show_busses
        && v.show_inputs
        && v.show_fx
        && v.show_master
        && v.show_preview
        && v.hidden.is_empty()
        && v.hidden_strips.is_empty()
        && v.shows(Region::Meters)
        && v.shows(Region::Faders)
}

/// The View Controls' Show All: every channel, the I/O, the Meters and the
/// Faders.
pub(crate) fn show_all(app: &mut BananaApp) {
    run(app, ConsoleOp::ShowAllChannels);
    for r in [Region::Io, Region::Meters, Region::Faders] {
        if !app.console.shows(r) {
            run(app, ConsoleOp::ToggleRegion(r));
        }
    }
}

/// A View Controls button `width` wide: blue while what it shows is shown
/// (`on`; none for Show All).
fn view_button(ui: &mut egui::Ui, width: f32, text: &str, on: Option<bool>) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 20.0), Sense::click());
    let lit = on == Some(true);
    let base = if lit {
        theme::selection()
    } else {
        ui.visuals().widgets.inactive.bg_fill
    };
    let fill = if resp.hovered() {
        base.lerp_to_gamma(Color32::WHITE, 0.12)
    } else {
        base
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, fill);
    crate::widgets::fit_text(
        painter,
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(12.0),
        if lit { Color32::WHITE } else { theme::text() },
        rect.width() - 6.0,
    );
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, text));
    resp
}

/// The channel width, as VEGAS's zoom: narrower, a mark on a line where the
/// width is (a click on the line picks one), wider.
fn width_control(ui: &mut egui::Ui, app: &mut BananaApp) {
    let at = ChannelWidth::ALL
        .iter()
        .position(|w| *w == app.console.width)
        .unwrap_or(1);
    let (rect, _) = ui.allocate_exact_size(vec2(100.0, 20.0), Sense::hover());
    let minus = Rect::from_center_size(pos2(rect.left() + 9.0, rect.center().y), vec2(18.0, 18.0));
    let plus = Rect::from_center_size(pos2(rect.right() - 9.0, rect.center().y), vec2(18.0, 18.0));
    for (r, wider, tip, to) in [
        (minus, false, tr("Narrower channels"), at.checked_sub(1)),
        (plus, true, tr("Wider channels"), (at < 2).then_some(at + 1)),
    ] {
        let resp = ui.interact(r, ui.id().with(("console-width", wider)), Sense::click());
        let color = if to.is_none() {
            theme::text_disabled()
        } else if resp.hovered() {
            theme::text_strong()
        } else {
            theme::text()
        };
        // A magnifier with − or +.
        let painter = ui.painter();
        let c = r.center() - vec2(1.5, 1.5);
        let stroke = Stroke::new(1.2, color);
        painter.circle_stroke(c, 5.5, stroke);
        painter.hline(c.x - 3.0..=c.x + 3.0, c.y, stroke);
        if wider {
            painter.vline(c.x, c.y - 3.0..=c.y + 3.0, stroke);
        }
        painter.line_segment(
            [c + vec2(4.0, 4.0), c + vec2(7.5, 7.5)],
            Stroke::new(1.6, color),
        );
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, to.is_some(), tip));
        if resp.on_hover_text(tip).clicked()
            && let Some(i) = to
        {
            app.console.width = ChannelWidth::ALL[i];
        }
    }
    let line = Rect::from_min_max(
        pos2(minus.right() + 8.0, rect.top()),
        pos2(plus.left() - 8.0, rect.bottom()),
    );
    let resp = ui.interact(line, ui.id().with("console-width-line"), Sense::click());
    if resp.clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        let i = ((p.x - line.left()) / line.width() * 2.0)
            .round()
            .clamp(0.0, 2.0);
        app.console.width = ChannelWidth::ALL[i as usize];
    }
    let now = ChannelWidth::ALL
        .iter()
        .position(|w| *w == app.console.width)
        .unwrap_or(1);
    let painter = ui.painter();
    let y = line.center().y + 4.0;
    painter.hline(line.x_range(), y, Stroke::new(2.0, theme::panel_dark()));
    let x = line.left() + line.width() * now as f32 / 2.0;
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(x - 5.0, y - 8.0),
            pos2(x + 5.0, y - 8.0),
            pos2(x, y - 1.0),
        ],
        theme::text(),
        Stroke::NONE,
    ));
    resp.on_hover_text(format!(
        "{}: {}",
        tr("Channel Width"),
        app.console.width.label()
    ));
}

/// A region of a strip, `h` tall: its label over it when labels show, a
/// line under it.
fn region(
    ui: &mut egui::Ui,
    view: &ConsoleView,
    r: Region,
    h: f32,
    add: impl FnOnce(&mut egui::Ui, Rect),
) {
    if !view.shows(r) {
        return;
    }
    let width = ui.max_rect().width();
    if view.labels {
        region_label(ui, r);
    }
    let (rect, _) = ui.allocate_exact_size(vec2(width, h), Sense::hover());
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Center)),
    );
    add(&mut child, rect);
    rule(ui);
}

/// A region's label (Show Control Region Labels).
fn region_label(ui: &mut egui::Ui, r: Region) {
    let width = ui.max_rect().width();
    let (lr, _) = ui.allocate_exact_size(vec2(width, LABEL_H), Sense::hover());
    ui.painter().text(
        lr.left_center() + vec2(4.0, 0.0),
        Align2::LEFT_CENTER,
        r.label(),
        FontId::proportional(9.5),
        theme::text_dim(),
    );
}

/// A strip's column, as VEGAS's: its background (blue while picked) — a
/// click on it picks the channel —, a dark line on its right, `add`
/// laying its rows top to bottom. Its rect and the background's response.
fn column(
    ui: &mut egui::Ui,
    id: egui::Id,
    width: f32,
    height: f32,
    selected: bool,
    add: impl FnOnce(&mut egui::Ui),
) -> (Rect, egui::Response) {
    let (rect, resp) = ui.allocate_exact_size(vec2(width + 1.0, height), Sense::click());
    let body = Rect::from_min_size(rect.min, vec2(width, height));
    let fill = if selected {
        theme::header_selected_focused()
    } else {
        theme::panel()
    };
    ui.painter().rect_filled(body, 0.0, fill);
    ui.painter().vline(
        rect.right() - 0.5,
        rect.y_range(),
        Stroke::new(1.0, theme::panel_dark()),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(id)
            .max_rect(body)
            .layout(egui::Layout::top_down(egui::Align::Center)),
    );
    child.spacing_mut().item_spacing = vec2(0.0, 0.0);
    child.spacing_mut().slider_width = width - 8.0;
    add(&mut child);
    (rect, resp)
}

/// A dark line across a strip, between its parts.
fn rule(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.max_rect().width(), 1.0), Sense::hover());
    ui.painter().rect_filled(r, 0.0, theme::panel_dark());
}

/// An empty row `h` tall (what a strip lacks, so the rows line up).
fn blank_row(ui: &mut egui::Ui, h: f32) {
    ui.allocate_exact_size(vec2(ui.max_rect().width(), h), Sense::hover());
}

/// A strip's header, as VEGAS 23's: its badge and its kind ("Audio",
/// "Bus"…), or its name alone (the Preview's).
fn header(ui: &mut egui::Ui, badge: Badge, kind: &str, picked: bool) -> egui::Response {
    let w = ui.max_rect().width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, HEADER_H), Sense::click_and_drag());
    let painter = ui.painter();
    let b = Rect::from_min_size(
        pos2(rect.left() + 5.0, rect.center().y - 8.0),
        vec2(20.0, 16.0),
    );
    let text_x = match badge {
        Badge::None => rect.left() + 5.0,
        _ => {
            paint_badge(painter, b, badge, 11.0, picked);
            b.right() + 6.0
        }
    };
    crate::widgets::fit_text(
        painter,
        pos2(text_x, rect.center().y),
        Align2::LEFT_CENTER,
        kind,
        FontId::proportional(12.0),
        theme::text(),
        rect.right() - text_x - 4.0,
    );
    resp
}

/// Down to a strip's foot: the room left, then the line over its name.
fn to_foot(ui: &mut egui::Ui) {
    let rest = ui.available_height() - NAME_H - 1.0;
    if rest > 0.0 {
        ui.add_space(rest);
    }
    rule(ui);
}

/// The name at a strip's foot, as VEGAS's channel label (the Preview's on
/// a dark ground, `dark`).
fn name_row(ui: &mut egui::Ui, name: &str, tip: &str, dark: bool) -> egui::Response {
    let w = ui.max_rect().width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, NAME_H), Sense::click_and_drag());
    if dark {
        ui.painter().rect_filled(rect, 0.0, theme::panel_dark());
    }
    crate::widgets::fit_text(
        ui.painter(),
        pos2(rect.left() + 5.0, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(12.0),
        theme::text(),
        w - 8.0,
    );
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, name));
    resp.on_hover_text(tip)
}

/// A click on a strip's header, name or ground picks its channel (Ctrl and
/// Shift: several), a drag moves it.
fn pick_or_drag(ui: &egui::Ui, app: &mut BananaApp, of: Of, r: &egui::Response) {
    if r.clicked() {
        let m = ui.input(|i| i.modifiers);
        match of {
            Of::Track(id) => {
                app.pick_track(id, m.shift, m.command);
                app.console.selected = None;
                app.console.master_picked = false;
            }
            Of::Strip(s) => pick_strip(app, s, m.shift, m.command),
        }
    }
    if r.drag_started() {
        app.console.dragging = Some(of);
    }
}

/// The Master picked (its strip and its line in the Channel List blue).
fn pick_master(app: &mut BananaApp) {
    app.console.master_picked = true;
    app.console.selected = None;
    app.console.picks.clear();
}

/// An I/O row, as VEGAS's: what it names, centered (cut short with "…");
/// a click opens its menu, the row blue meanwhile.
fn io_row(ui: &mut egui::Ui, text: &str, tip: &str, menu: impl FnOnce(&mut egui::Ui)) {
    let w = ui.max_rect().width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, IO_ROW), Sense::click());
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&resp));
    let painter = ui.painter();
    let lit = rect.shrink2(vec2(4.0, 2.0));
    if open {
        painter.rect_filled(lit, 0.0, theme::selection());
    } else if resp.hovered() {
        painter.rect_filled(lit, 0.0, theme::hover());
    }
    crate::widgets::fit_text(
        painter,
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(12.0),
        if open { Color32::WHITE } else { theme::text() },
        w - 10.0,
    );
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, text));
    let resp = resp.on_hover_text(format!("{tip}: {text}"));
    egui::Popup::menu(&resp).show(menu);
}

/// An I/O row only naming where it plays (the Preview's device).
fn io_text(ui: &mut egui::Ui, text: &str, tip: &str) {
    let w = ui.max_rect().width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, IO_ROW), Sense::hover());
    crate::widgets::fit_text(
        ui.painter(),
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(12.0),
        theme::text(),
        w - 10.0,
    );
    resp.on_hover_text(format!("{tip}: {text}"));
}

/// The automation modes as the strips name them.
fn automation_short(m: bv_timeline::AutomationMode) -> &'static str {
    match m {
        bv_timeline::AutomationMode::Off => tr("Off"),
        bv_timeline::AutomationMode::Read => tr("Read"),
        bv_timeline::AutomationMode::Touch => tr("Touch"),
        bv_timeline::AutomationMode::Latch => tr("Latch"),
    }
}

/// The automation row, as VEGAS 23's: a button of its mode's icon, the
/// mode and the arrow of its menu — blue while Automation Settings are on
/// (a click on the icon turns them on or off) or the menu is open. Whether
/// the icon was clicked, and the mode picked.
fn automation_row(
    ui: &mut egui::Ui,
    controls: bool,
    mode: bv_timeline::AutomationMode,
    enabled: bool,
) -> (bool, Option<bv_timeline::AutomationMode>) {
    let w = ui.max_rect().width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, AUTO_ROW), Sense::hover());
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let button = rect.shrink2(vec2(4.0, 4.0));
    let icon_r = Rect::from_min_size(button.min, vec2(20.0, button.height()));
    let menu_r = Rect::from_min_max(pos2(icon_r.right(), button.top()), button.max);
    let icon = ui.interact(icon_r, ui.id().with("automation-on"), sense);
    let menu = ui.interact(menu_r, ui.id().with("automation-mode"), sense);
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&menu));
    let lit = enabled && (controls || open);
    let painter = ui.painter();
    if lit {
        painter.rect_filled(button, 0.0, theme::selection());
    } else if enabled && (icon.hovered() || menu.hovered()) {
        painter.rect_filled(button, 0.0, theme::hover());
    }
    let color = match (enabled, lit) {
        (false, _) => theme::text_disabled(),
        (true, true) => Color32::WHITE,
        (true, false) => theme::text(),
    };
    paint_mode_icon(
        painter,
        Rect::from_center_size(icon_r.center(), vec2(16.0, 16.0)),
        mode,
        (lit || !enabled).then_some(color),
    );
    let text = crate::widgets::fit_text(
        painter,
        pos2(menu_r.left() + 2.0, rect.center().y),
        Align2::LEFT_CENTER,
        automation_short(mode),
        FontId::proportional(12.0),
        color,
        (menu_r.width() - 15.0).max(1.0),
    );
    let a = pos2(text.right() + 6.0, rect.center().y);
    painter.add(egui::Shape::convex_polygon(
        vec![
            a + vec2(-3.5, -1.5),
            a + vec2(3.5, -1.5),
            a + vec2(0.0, 2.5),
        ],
        color,
        Stroke::NONE,
    ));
    let name = crate::commands::automation_mode_name(mode);
    icon.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tr("Automation Settings"))
    });
    menu.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name));
    if !enabled {
        icon.on_hover_text(tr("Coming soon to BananaV"));
        menu.on_hover_text(tr("Coming soon to BananaV"));
        return (false, None);
    }
    let icon = icon.on_hover_text(trf!("Automation Settings: {}", name));
    let menu = menu.on_hover_text(tr("Automation mode"));
    let mut picked = None;
    egui::Popup::menu(&menu).show(|ui| {
        for m in bv_timeline::AutomationMode::ALL {
            if mode_item(ui, m, m == mode).clicked() {
                picked = Some(m);
                ui.close();
            }
        }
    });
    (icon.clicked(), picked)
}

/// An automation mode's color, as VEGAS's icons show it: Off grey, Read
/// blue, Touch light, Latch red.
fn mode_color(m: bv_timeline::AutomationMode) -> Color32 {
    match m {
        bv_timeline::AutomationMode::Off => theme::text_dim(),
        bv_timeline::AutomationMode::Read => READ_BLUE,
        bv_timeline::AutomationMode::Touch => theme::text(),
        bv_timeline::AutomationMode::Latch => AUTOMATION_RED,
    }
}

/// An automation mode's icon in its color (or `over`'s), Off crossed out.
pub(crate) fn paint_mode_icon(
    painter: &egui::Painter,
    rect: Rect,
    m: bv_timeline::AutomationMode,
    over: Option<Color32>,
) {
    let color = over.unwrap_or_else(|| mode_color(m));
    crate::icons::paint(painter, rect, Icon::Automation, color);
    if m == bv_timeline::AutomationMode::Off {
        painter.line_segment(
            [
                rect.right_top() + vec2(-2.0, 2.0),
                rect.left_bottom() + vec2(2.0, -2.0),
            ],
            Stroke::new(1.5, color),
        );
    }
}

/// An Automation Settings item, as VEGAS's menu: its mode's icon and name
/// (the mode now in strong text).
pub(crate) fn mode_item(
    ui: &mut egui::Ui,
    m: bv_timeline::AutomationMode,
    now: bool,
) -> egui::Response {
    let name = crate::commands::automation_mode_name(m);
    let font = FontId::proportional(13.0);
    let galley = ui
        .painter()
        .layout_no_wrap(name.to_string(), font.clone(), theme::text());
    let w = (galley.size().x + 40.0).max(ui.available_width());
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 22.0), Sense::click());
    let painter = ui.painter();
    if resp.hovered() {
        painter.rect_filled(rect, 0.0, theme::selection());
    }
    paint_mode_icon(
        painter,
        Rect::from_center_size(pos2(rect.left() + 12.0, rect.center().y), vec2(16.0, 16.0)),
        m,
        None,
    );
    let color = if resp.hovered() {
        Color32::WHITE
    } else if now {
        theme::text_strong()
    } else {
        theme::text()
    };
    painter.text(
        pos2(rect.left() + 28.0, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        font,
        color,
    );
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, now, name));
    resp
}

/// A pan as the strips write it: "Center", "50% L", "50% R".
fn pan_label(pan: f32) -> String {
    if pan.abs() < 0.005 {
        tr("Center").to_string()
    } else if pan < 0.0 {
        trf!("{}% L", (-pan * 100.0).round())
    } else {
        trf!("{}% R", (pan * 100.0).round())
    }
}

/// The pan, as VEGAS's console draws it: a line, a mark over it where the
/// sound is (red while it is an automation control, `automated`), its
/// value under it; dragged (Ctrl: fine steps), a double-click centers it.
/// The pan it was moved to.
fn pan_row(ui: &mut egui::Ui, pan: f32, automated: bool) -> (Option<f32>, egui::Response) {
    let w = ui.max_rect().width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, PAN_H), Sense::click_and_drag());
    let line = Rect::from_min_max(
        pos2(rect.left() + 6.0, rect.top() + 12.0),
        pos2(rect.right() - 6.0, rect.top() + 15.0),
    );
    let (from, to) = (line.left() + 3.0, line.right() - 3.0);
    let mut out = None;
    if resp.dragged() {
        let fine = if ui.input(|i| i.modifiers.ctrl) {
            0.1
        } else {
            1.0
        };
        let d = resp.drag_delta().x / (to - from) * 2.0 * fine;
        if d != 0.0 {
            out = Some((pan + d).clamp(-1.0, 1.0));
        }
    }
    if resp.double_clicked() {
        out = Some(0.0);
    }
    let shown = out.unwrap_or(pan);
    let painter = ui.painter();
    painter.rect_filled(line, 0.0, theme::panel_dark());
    let x = from + (shown + 1.0) / 2.0 * (to - from);
    let hot = resp.hovered() || resp.dragged();
    let color = match (automated, hot) {
        (true, false) => AUTOMATION_RED,
        (true, true) => AUTOMATION_RED.lerp_to_gamma(Color32::WHITE, 0.3),
        (false, true) => Color32::WHITE,
        (false, false) => Color32::from_gray(0xE6),
    };
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(x - 5.5, line.top() - 7.0),
            pos2(x + 5.5, line.top() - 7.0),
            pos2(x, line.top() - 0.5),
        ],
        color,
        Stroke::NONE,
    ));
    painter.text(
        pos2(rect.center().x, rect.top() + 29.0),
        Align2::CENTER_CENTER,
        pan_label(shown),
        FontId::proportional(10.5),
        theme::text(),
    );
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Slider, true, tr("Pan")));
    (out, resp.on_hover_text(tr("Pan (double-click: center)")))
}

/// A 5.1 project: a strip's small surround panner in its pan's place.
fn surround_row(ui: &mut egui::Ui, app: &mut BananaApp, owner: bv_timeline::SurroundOwner) {
    let w = ui.max_rect().width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, PAN_H), Sense::hover());
    let pad = Rect::from_center_size(rect.center(), vec2(PAN_H - 4.0, PAN_H - 4.0));
    crate::tabs::surround::mini_panner(
        ui,
        app,
        pad,
        owner,
        egui::Id::new(("console-surround", owner)),
        tr("Surround Panner (double-click: open; Ctrl+drag: fine)"),
    );
}

/// The part under the pan: the button column, the fader and, while the
/// Meters region is hidden, a meter beside it — as VEGAS's.
struct Block {
    buttons: Rect,
    fader: Rect,
    meter: Option<Rect>,
}

impl Block {
    /// The button column's `i`th cell.
    fn cell(&self, i: usize) -> Rect {
        Rect::from_min_size(
            pos2(
                self.buttons.left(),
                self.buttons.top() + i as f32 * BUTTON_H,
            ),
            vec2(BUTTONS_W, BUTTON_H),
        )
    }
}

/// The part under the pan: the button column (none for the Preview,
/// `buttons` off), the fader and, while the Meters region is hidden, a
/// meter beside it — as VEGAS's.
fn block(ui: &mut egui::Ui, h: f32, side_meter: bool, buttons: bool) -> Block {
    let w = ui.max_rect().width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let column_w = if buttons { BUTTONS_W } else { 0.0 };
    let buttons = Rect::from_min_size(pos2(rect.left() + 3.0, rect.top()), vec2(column_w, h));
    // The meter from below the fader's top to its foot, as VEGAS's.
    let meter = side_meter.then(|| {
        Rect::from_min_max(
            pos2(rect.right() - 4.0 - SIDE_METER_W, rect.top() + 22.0),
            pos2(rect.right() - 4.0, rect.bottom() - 8.0),
        )
    });
    let right = meter.map_or(rect.right() - 2.0, |m| m.left());
    let x = ((buttons.right() + right) / 2.0).round();
    let fader = Rect::from_center_size(pos2(x, rect.center().y), vec2(26.0, h));
    Block {
        buttons,
        fader,
        meter,
    }
}

/// A letter of the button column ("M", "S"), filled with `lit` while on.
#[allow(clippy::too_many_arguments)]
fn letter_cell(
    ui: &mut egui::Ui,
    rect: Rect,
    id: egui::Id,
    text: &str,
    on: bool,
    lit: Color32,
    enabled: bool,
    tip: &str,
) -> egui::Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let resp = ui.interact(rect, id, sense);
    let painter = ui.painter();
    let r = rect.shrink(1.0);
    if on {
        painter.rect_filled(r, 2.0, lit);
    } else if resp.hovered() && enabled {
        painter.rect_filled(r, 2.0, theme::hover());
    }
    let color = match (on, enabled) {
        (true, _) => Color32::BLACK,
        (false, true) => theme::text(),
        (false, false) => theme::text_disabled(),
    };
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(15.0),
        color,
    );
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tip));
    resp.on_hover_text(tip)
}

/// The column's fx: a track's as VEGAS 23's ("fx" with a blue dot), a
/// bus's, chain's or the Master's in bold ("fx", `bold`); lit while there
/// are effects. A click opens their window.
fn fx_cell(
    ui: &mut egui::Ui,
    rect: Rect,
    id: egui::Id,
    has_fx: bool,
    bold: bool,
    tip: &str,
) -> egui::Response {
    let resp = ui.interact(rect, id, Sense::click());
    let painter = ui.painter();
    if resp.hovered() {
        painter.rect_filled(rect.shrink(1.0), 2.0, theme::hover());
    }
    let color = if has_fx {
        theme::banana()
    } else {
        theme::text()
    };
    if bold {
        // Bold: the text twice, half a point apart.
        for dx in [0.0, 0.6] {
            painter.text(
                pos2(rect.left() + 3.0 + dx, rect.center().y),
                Align2::LEFT_CENTER,
                "fx",
                FontId::proportional(16.0),
                color,
            );
        }
    } else {
        painter.text(
            pos2(rect.left() + 3.0, rect.center().y - 1.0),
            Align2::LEFT_CENTER,
            "fx",
            FontId::proportional(12.0),
            color,
        );
        painter.circle_filled(
            pos2(rect.left() + 15.5, rect.center().y + 3.0),
            3.5,
            READ_BLUE,
        );
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tip));
    resp.on_hover_text(tip)
}

/// The column's Arm for Record, as VEGAS 23's: a red dot (on a highlight
/// while armed).
fn arm_cell(ui: &mut egui::Ui, rect: Rect, id: egui::Id, armed: bool, tip: &str) -> egui::Response {
    let resp = ui.interact(rect, id, Sense::click());
    let painter = ui.painter();
    if armed {
        painter.rect_filled(rect.shrink(1.0), 2.0, theme::tab_on());
    } else if resp.hovered() {
        painter.rect_filled(rect.shrink(1.0), 2.0, theme::hover());
    }
    painter.circle_filled(rect.center(), 6.0, AUTOMATION_RED);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tip));
    resp.on_hover_text(tip)
}

/// The mute while it is an automation control, as VEGAS 23 draws it: a
/// red dot with a "no" sign (on a highlight while muted).
fn mute_automation_cell(
    ui: &mut egui::Ui,
    rect: Rect,
    id: egui::Id,
    on: bool,
    tip: &str,
) -> egui::Response {
    let resp = ui.interact(rect, id, Sense::click());
    let painter = ui.painter();
    if on {
        painter.rect_filled(rect.shrink(1.0), 2.0, theme::tab_on());
    } else if resp.hovered() {
        painter.rect_filled(rect.shrink(1.0), 2.0, theme::hover());
    }
    let dot = pos2(rect.left() + 8.5, rect.center().y - 2.0);
    painter.circle_filled(dot, 5.0, AUTOMATION_RED);
    let no = dot + vec2(6.0, 5.0);
    let ground = if on { theme::tab_on() } else { theme::panel() };
    painter.circle_filled(no, 4.2, ground);
    let stroke = Stroke::new(1.1, theme::text());
    painter.circle_stroke(no, 3.2, stroke);
    painter.line_segment([no + vec2(-2.2, 2.2), no + vec2(2.2, -2.2)], stroke);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tip));
    resp.on_hover_text(tip)
}

/// An icon of the button column, on a highlight while on.
#[allow(clippy::too_many_arguments)]
fn icon_cell(
    ui: &mut egui::Ui,
    rect: Rect,
    id: egui::Id,
    icon: Icon,
    on: bool,
    enabled: bool,
    tip: &str,
) -> egui::Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let resp = ui.interact(rect, id, sense);
    let painter = ui.painter();
    let r = rect.shrink(1.0);
    if on {
        painter.rect_filled(r, 2.0, theme::tab_on());
    } else if resp.hovered() && enabled {
        painter.rect_filled(r, 2.0, theme::hover());
    }
    let color = match (on, enabled) {
        (_, false) => theme::text_disabled(),
        (true, true) => theme::text_strong(),
        (false, true) => theme::text(),
    };
    crate::icons::paint(
        painter,
        Rect::from_center_size(rect.center(), vec2(16.0, 16.0)),
        icon,
        color,
    );
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tip));
    resp.on_hover_text(tip)
}

/// The column's Pre/Post Fader Insert FX, as VEGAS's: "fx" before a
/// fader while the effects come before the volume, after it otherwise.
fn fx_order_cell(ui: &mut egui::Ui, rect: Rect, id: egui::Id, post: bool) -> egui::Response {
    let resp = ui.interact(rect, id, Sense::click());
    let painter = ui.painter();
    if resp.hovered() {
        painter.rect_filled(rect.shrink(1.0), 2.0, theme::hover());
    }
    let color = theme::text();
    let fx_w = 11.0;
    let (fader_x, fx_x) = if post {
        (rect.left() + 5.5, rect.left() + 9.0)
    } else {
        (rect.left() + 3.5 + fx_w + 2.5, rect.left() + 3.0)
    };
    painter.vline(
        fader_x,
        rect.top() + 4.0..=rect.bottom() - 4.0,
        Stroke::new(1.0, color),
    );
    painter.circle_filled(pos2(fader_x, rect.center().y), 2.2, color);
    painter.text(
        pos2(fx_x, rect.center().y),
        Align2::LEFT_CENTER,
        "fx",
        FontId::proportional(11.0),
        color,
    );
    let tip = if post {
        tr("Post Fader Insert FX: the effects after the volume")
    } else {
        tr("Pre Fader Insert FX: the effects before the volume")
    };
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tip));
    resp.on_hover_text(tip)
}

/// A console meter in `rect`, at the console's scale.
fn meter_at(
    ui: &mut egui::Ui,
    rect: Rect,
    id: egui::Id,
    meter: Option<&crate::tabs::master::MeterState>,
    scale: usize,
) -> egui::Response {
    let (floor, step, tick) = METER_SCALES[scale.min(METER_SCALES.len() - 1)];
    crate::timeline::paint_meter_scaled(ui, rect, meter, None, floor, step, tick);
    ui.interact(rect, id, Sense::click())
}

/// The Meters region, as VEGAS's: a meter for each channel side by side,
/// each with its scale.
fn peaks_at(
    ui: &mut egui::Ui,
    rect: Rect,
    id: egui::Id,
    meter: Option<&crate::tabs::master::MeterState>,
    scale: usize,
) -> egui::Response {
    let (floor, step, tick) = METER_SCALES[scale.min(METER_SCALES.len() - 1)];
    let n = meter.map_or(2, |m| m.channels().clamp(1, 8));
    let area = Rect::from_min_max(
        pos2(rect.left() + 8.0, rect.top() + 8.0),
        pos2(rect.right() - 10.0, rect.bottom() - 8.0),
    );
    let gap = 3.0;
    let w = (area.width() - gap * (n - 1) as f32) / n as f32;
    for i in 0..n {
        let lane = Rect::from_min_size(
            pos2(area.left() + i as f32 * (w + gap), area.top()),
            vec2(w, area.height()),
        );
        crate::timeline::paint_meter_scaled(ui, lane, meter, Some(i), floor, step, tick);
    }
    ui.interact(area, id, Sense::click())
}

/// The mouse wheel over a console meter: wider scales toward the user,
/// closer ones away from them.
fn roll_scale(ui: &egui::Ui, app: &mut BananaApp, resp: &egui::Response) {
    if !resp.hovered() {
        return;
    }
    let (notches, _) = ui.input(|i| crate::widgets::wheel_notches(&i.events));
    if notches.abs() >= 1.0 {
        let i = app.console.meter_scale as i32 - notches.signum() as i32;
        app.console.meter_scale = i.clamp(0, METER_SCALES.len() as i32 - 1) as usize;
        ui.ctx()
            .input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
    }
}

/// A level as the strips write it under the fader: "0,0", "-4,6"; "−∞" at
/// the bottom.
fn value_text(db: f32) -> String {
    if db <= MIN_DB + 0.05 {
        "−∞".to_string()
    } else if db.abs() < 0.05 {
        crate::fmt::num(0.0, 1)
    } else {
        crate::fmt::num(db as f64, 1)
    }
}

/// A track's, a chain's or a bus's strip, as VEGAS 23's: its header, the
/// regions shown, its name at the foot; a click anywhere picks it.
fn strip_ui(ui: &mut egui::Ui, app: &mut BananaApp, of: Of, sizes: &Sizes, view: &ConsoleView) {
    let selected = match of {
        Of::Track(id) => {
            app.console.selected.is_none()
                && !app.console.master_picked
                && app.track_selection().contains(&id)
        }
        Of::Strip(s) => strip_selection(&app.console).contains(&s),
    };
    let p = app.project();
    let badge = match of {
        Of::Track(id) => {
            let Some(t) = p.track(id) else {
                return;
            };
            let n = p.track_index(id).map_or(0, |i| i + 1);
            Badge::Number(n, app.settings.prefs.color_of(t))
        }
        Of::Strip(s) => {
            if p.strip_channel(s).is_none() {
                return;
            }
            strip_badge(p, s)
        }
    };
    let kind = match of {
        Of::Track(_) => tr("Audio"),
        Of::Strip(Strip::Bus(_)) => tr("Bus"),
        Of::Strip(Strip::Chain(_)) => tr("Assignable FX"),
    };
    let w = sizes.width;
    let base = egui::Id::new(("console-strip", format!("{of:?}")));
    let (rect, ground) = column(ui, base, w, sizes.height, selected, |ui| {
        let head = header(ui, badge, kind, selected).on_hover_text(tr("Drag to move the channel"));
        pick_or_drag(ui, app, of, &head);
        rule(ui);
        let target = of.fx_target();
        region(ui, view, Region::InsertFx, sizes.insert_fx, |ui, _| {
            ui.add_space(2.0);
            insert_fx(ui, app, target, w - 6.0)
        });
        region(ui, view, Region::Sends, sizes.sends, |ui, rect| {
            let rect = rect.shrink2(vec2(3.0, 0.0));
            sends_region(ui, app, of, rect, sizes);
            send_divider(ui, app, of, rect, sizes);
        });
        region(ui, view, Region::Io, sizes.io, |ui, _| {
            io_region(ui, app, of, sizes)
        });
        region(ui, view, Region::VuMeters, METERS_H, |ui, rect| {
            vu_region(ui, app, of.meter(), rect, view)
        });
        region(ui, view, Region::Meters, PEAKS_H, |ui, rect| {
            peak_meter(ui, app, of, rect)
        });
        if view.shows(Region::Faders) {
            faders_region(ui, app, of, sizes, view, base);
        }
        to_foot(ui);
        match of {
            Of::Track(id) => track_name_ui(ui, app, id),
            Of::Strip(s) => strip_name_ui(ui, app, s),
        }
    });
    pick_or_drag(ui, app, of, &ground);
    app.console.strip_rects.push((of, rect));
}

/// The Insert FX rows an input bus needs, with `rows` so far.
fn fx_rows_of_input(p: &Project, id: bv_timeline::InputBusId, rows: usize) -> usize {
    rows.max(p.input_bus(id).map_or(0, |b| b.effects.len()) + 1)
}

/// An input bus's strip (VEGAS's Help, "Input bus channel strips"): its
/// Insert FX (always before its fader), its Input Source and Output in
/// the I/O region, its level's meters, its pan, mute and fader, and its
/// name (a double-click renames it; the right-click deletes it).
fn input_bus_strip(
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    id: bv_timeline::InputBusId,
    sizes: &Sizes,
    view: &ConsoleView,
) {
    let Some(bus) = app.project().input_bus(id).cloned() else {
        return;
    };
    let w = sizes.width;
    let base = egui::Id::new(("console-input-bus", id.0));
    // What its input plays now, held as a meter's.
    let level = app.input_bus_levels.remove(&id).unwrap_or([0.0; 2]);
    let mut meter = crate::tabs::master::MeterState::default();
    meter.update(Some(level.to_vec()));
    let letter = bv_timeline::busses::bus_letter(app.project().input_bus_index(id).unwrap_or(0));
    column(ui, base, w, sizes.height, false, |ui| {
        header(ui, Badge::Letter(letter), tr("Input Bus"), false);
        rule(ui);
        region(ui, view, Region::InsertFx, sizes.insert_fx, |ui, _| {
            ui.add_space(2.0);
            insert_fx(ui, app, FxTarget::InputBus(id), w - 6.0)
        });
        region(ui, view, Region::Sends, sizes.sends, |ui, rect| {
            input_sends(ui, app, id, rect.shrink2(vec2(3.0, 0.0)), sizes);
        });
        region(ui, view, Region::Io, sizes.io, |ui, _| {
            // Input Source: a device's channels.
            let source = crate::recording::input_label(&bus.input);
            io_row(ui, &source, tr("Input Source"), |ui| {
                for d in app.input_devices() {
                    let device = if d.default {
                        String::new()
                    } else {
                        d.name.clone()
                    };
                    let n = d.channels.max(1);
                    ui.menu_button(d.name.clone(), |ui| {
                        let pairs = (0..n.saturating_sub(1)).step_by(2).map(|first| (first, 2));
                        let singles = (0..n).map(|first| (first, 1));
                        for (first, channels) in pairs.chain(singles) {
                            let input = bv_timeline::RecordInput {
                                device: device.clone(),
                                first,
                                channels,
                                bus: None,
                            };
                            let on = bus.input == input;
                            let label = crate::recording::input_label(&input);
                            if ui.selectable_label(on, label).clicked() {
                                app.modify(|p| {
                                    if let Some(b) = p.input_bus_mut(id) {
                                        b.input = input.clone();
                                    }
                                });
                                ui.close();
                            }
                        }
                    });
                }
            });
            // Output: the Master, or off (heard only through a track
            // recording it).
            let out = if bus.to_master {
                tr("Master")
            } else {
                tr("Off")
            };
            io_row(ui, out, tr("Output"), |ui| {
                for (to, label) in [(true, tr("Master")), (false, tr("Off"))] {
                    if ui.selectable_label(bus.to_master == to, label).clicked() {
                        app.modify(|p| {
                            if let Some(b) = p.input_bus_mut(id) {
                                b.to_master = to;
                            }
                        });
                        ui.close();
                    }
                }
            });
        });
        region(ui, view, Region::VuMeters, METERS_H, |_, _| {});
        region(ui, view, Region::Meters, PEAKS_H, |ui, rect| {
            let r = peaks_at(
                ui,
                rect,
                base.with("peaks"),
                Some(&meter),
                app.console.meter_scale,
            );
            roll_scale(ui, app, &r);
        });
        if view.shows(Region::Faders) {
            if view.labels {
                region_label(ui, Region::Faders);
            }
            // No automation.
            blank_row(ui, AUTO_ROW);
            let (moved, _) = pan_row(ui, bus.pan, false);
            if let Some(pan) = moved {
                app.modify_merged(base.with("pan"), |p| {
                    if let Some(b) = p.input_bus_mut(id) {
                        b.pan = pan;
                    }
                });
            }
            let b = block(ui, sizes.fader, !view.shows(Region::Meters), true);
            let muted = bus.muted;
            if letter_cell(
                ui,
                b.cell(0),
                base.with("mute"),
                "M",
                muted,
                theme::error(),
                true,
                tr("Mute"),
            )
            .clicked()
            {
                app.modify(|p| {
                    if let Some(b) = p.input_bus_mut(id) {
                        b.muted = !muted;
                    }
                });
            }
            let has_fx = !bus.effects.is_empty();
            if fx_cell(
                ui,
                b.cell(1),
                base.with("fx"),
                has_fx,
                true,
                tr("Input Bus FX"),
            )
            .clicked()
            {
                app.clip_windows.fx_target = Some(FxTarget::InputBus(id));
            }
            let (moved, _, r) = fader_at(
                ui,
                b.fader,
                base.with("fader"),
                bus.gain_db,
                1,
                view.ticks,
                None,
            );
            if let Some(db) = moved {
                app.modify_merged(r.id, |p| {
                    if let Some(b) = p.input_bus_mut(id) {
                        b.gain_db = db;
                    }
                });
            }
            if let Some(m) = b.meter {
                let r = meter_at(
                    ui,
                    m,
                    base.with("meter"),
                    Some(&meter),
                    app.console.meter_scale,
                );
                roll_scale(ui, app, &r);
            }
            value_label(ui, bus.gain_db, b.fader.center().x);
        }
        to_foot(ui);
        // Its name: double-click renames it, the menu deletes it.
        match app.console.renaming_input.clone() {
            Some((r, mut text)) if r == id => {
                let (rect, _) = ui.allocate_exact_size(vec2(w, NAME_H), Sense::hover());
                let edit = ui.put(
                    rect.shrink2(vec2(3.0, 1.0)),
                    egui::TextEdit::singleline(&mut text),
                );
                edit.request_focus();
                if edit.lost_focus() {
                    app.console.renaming_input = None;
                    let name = text.trim().to_string();
                    app.modify(|p| {
                        if let Some(b) = p.input_bus_mut(id) {
                            b.name = name;
                        }
                    });
                } else {
                    app.console.renaming_input = Some((id, text));
                }
            }
            _ => {
                let name = crate::i18n::input_bus_name(app.project(), id);
                let label = name_row(ui, &name, tr("Double-click to rename"), false);
                if label.double_clicked() {
                    app.console.renaming_input = Some((
                        id,
                        app.project()
                            .input_bus(id)
                            .map_or(String::new(), |b| b.name.clone()),
                    ));
                }
                label.context_menu(|ui| {
                    if ui.button(tr("Delete")).clicked() {
                        app.modify(|p| {
                            p.remove_input_bus(id);
                        });
                        ui.close();
                    }
                });
            }
        }
    });
}

/// An input bus's Sends region: a mini fader for each assignable effects
/// chain (after the bus's volume, or before it from the right-click).
fn input_sends(
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    id: bv_timeline::InputBusId,
    rect: Rect,
    sizes: &Sizes,
) {
    let p = app.project();
    let Some(bus) = p.input_bus(id) else {
        return;
    };
    let rows: Vec<(Send, String)> = p
        .fx_chains
        .iter()
        .map(|c| {
            let to = SendTo::Fx(c.id);
            let send = bus
                .sends
                .iter()
                .find(|s| s.to == to)
                .copied()
                .unwrap_or_else(|| Send::new(to));
            (send, crate::i18n::chain_name(p, c.id))
        })
        .collect();
    let first = app
        .console
        .send_first
        .min(sizes.send_total.saturating_sub(sizes.send_rows));
    for (i, (send, name)) in rows.into_iter().enumerate() {
        if i < first || i >= first + sizes.send_rows {
            continue;
        }
        let row = Rect::from_min_size(
            pos2(rect.left(), rect.top() + (i - first) as f32 * SEND_ROW),
            vec2(rect.width(), SEND_ROW - 3.0),
        );
        let key = egui::Id::new(("console-input-send", id.0, format!("{:?}", send.to)));
        let db = if send.gain_db <= -96.0 {
            MIN_DB
        } else {
            send.gain_db
        };
        let (moved, resp) = mini_fader(ui, row, key, &name, db, send.pre_volume);
        if let Some(db) = moved {
            let mut s = send;
            s.gain_db = if db <= MIN_DB + 0.05 {
                f32::NEG_INFINITY
            } else {
                db
            };
            app.modify_merged(key, |p| p.set_input_bus_send(id, s));
        }
        let tip = if send.pre_volume {
            tr("Send (Pre Volume); right-click: Pre or Post Volume")
        } else {
            tr("Send (Post Volume); right-click: Pre or Post Volume")
        };
        resp.on_hover_text(tip).context_menu(|ui| {
            for (label, pre) in [(tr("Pre Volume"), true), (tr("Post Volume"), false)] {
                if crate::widgets::radio(ui, send.pre_volume == pre, label).clicked() {
                    app.modify(|p| {
                        p.set_input_bus_send(
                            id,
                            Send {
                                pre_volume: pre,
                                ..send
                            },
                        )
                    });
                    ui.close();
                }
            }
        });
    }
}

/// The master strip, as VEGAS 23's: its header, its Insert FX, the output
/// device's pair it plays on (the Audio Device Selector) in the I/O
/// region, its meters; mute, solo, its effects and their order, the lock,
/// its pair of faders with their values and its meter (in 5.1 the
/// Surround Master's channel faders over them); "Master" at its foot. A
/// click picks it.
fn master_strip(ui: &mut egui::Ui, app: &mut BananaApp, sizes: &Sizes, view: &ConsoleView) {
    let surround = app.project().settings.channels == bv_core::ChannelLayout::Surround51;
    let w = if surround {
        sizes.width.max(112.0)
    } else {
        sizes.width
    };
    let base = egui::Id::new("console-master");
    let picked = app.console.master_picked;
    let (_, ground) = column(ui, base, w, sizes.height, picked, |ui| {
        if header(ui, Badge::Icon(Icon::BusSelect), tr("Master"), picked).clicked() {
            pick_master(app);
        }
        rule(ui);
        region(ui, view, Region::InsertFx, sizes.insert_fx, |ui, _| {
            ui.add_space(2.0);
            insert_fx(ui, app, FxTarget::Master, w - 6.0)
        });
        region(ui, view, Region::Sends, sizes.sends, |_, _| {});
        region(ui, view, Region::Io, sizes.io, |ui, _| {
            if sizes.io_rows > 1 {
                blank_row(ui, IO_ROW);
            }
            let pair = app.project().master.device_pairs[0];
            let name = crate::i18n::output_name(app.project(), Output::Hardware(pair));
            io_row(ui, &name, tr("Audio Device Selector"), |ui| {
                crate::tabs::master::device_menu(ui, app)
            });
        });
        region(ui, view, Region::VuMeters, METERS_H, |ui, rect| {
            vu_region(ui, app, MeterOf::Master, rect, view)
        });
        region(ui, view, Region::Meters, PEAKS_H, |ui, rect| {
            let r = peaks_at(
                ui,
                rect,
                base.with("peaks"),
                Some(&app.meters),
                app.console.meter_scale,
            )
            .on_hover_text(tr("Click to clear the clip lights"));
            if r.clicked() {
                app.meters.reset_clip();
            }
            roll_scale(ui, app, &r);
        });
        if view.shows(Region::Faders) {
            if view.labels {
                region_label(ui, Region::Faders);
            }
            // Automation of the Master to come.
            automation_row(ui, false, bv_timeline::AutomationMode::Read, false);
            // No pan: in 5.1 the Surround Master's faders, the main
            // fader that much shorter.
            let mut fader_h = sizes.fader;
            if surround {
                const SURROUND_H: f32 = 104.0;
                let (rect, _) = ui.allocate_exact_size(vec2(w, SURROUND_H), Sense::hover());
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect.shrink2(vec2(4.0, 0.0)))
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                crate::tabs::master::surround_faders(&mut child, app);
                fader_h = (fader_h + PAN_H - SURROUND_H).max(80.0);
            } else {
                blank_row(ui, PAN_H);
            }
            let b = block(ui, fader_h, !view.shows(Region::Meters), true);
            let master = app.project().master;
            if letter_cell(
                ui,
                b.cell(0),
                base.with("mute"),
                "M",
                master.muted,
                theme::error(),
                true,
                tr("Mute the master"),
            )
            .clicked()
            {
                app.modify(|p| p.master.muted = !master.muted);
            }
            letter_cell(
                ui,
                b.cell(1),
                base.with("solo"),
                "S",
                false,
                theme::banana(),
                false,
                tr("Coming soon to BananaV"),
            );
            // Its effects: the Plug-In Chooser while it has none; their
            // order before or after its volume.
            let has_fx = !app.project().master_fx.is_empty();
            let fx = fx_cell(
                ui,
                b.cell(2),
                base.with("fx"),
                has_fx,
                true,
                tr("Master Bus FX"),
            );
            if fx.clicked() {
                app.clip_windows.fx_target = Some(FxTarget::Master);
            }
            let post = app.project().master_fx_post_fader;
            fx.context_menu(|ui| {
                let mut on = post;
                if crate::widgets::checkbox(ui, &mut on, tr("Post Volume")).clicked() {
                    app.modify(|p| p.master_fx_post_fader = on);
                    ui.close();
                }
            });
            if fx_order_cell(ui, b.cell(3), base.with("order"), post).clicked() {
                app.modify(|p| p.master_fx_post_fader = !post);
            }
            // The lock at the fader's foot: in 5.1 the channel faders
            // together or one by one.
            let lock = Rect::from_min_size(
                pos2(b.buttons.left(), b.fader.bottom() - BUTTON_H - 4.0),
                vec2(BUTTONS_W, BUTTON_H),
            );
            if surround {
                let locked = !master.unlocked;
                if icon_cell(
                    ui,
                    lock,
                    base.with("lock"),
                    Icon::Lock,
                    locked,
                    true,
                    tr("Lock Fader"),
                )
                .clicked()
                {
                    app.modify(|p| p.master.unlocked = locked);
                }
            } else {
                icon_cell(
                    ui,
                    lock,
                    base.with("lock"),
                    Icon::Lock,
                    false,
                    false,
                    tr("Coming soon to BananaV"),
                );
            }
            let (moved, _, r) = fader_at(
                ui,
                b.fader,
                base.with("fader"),
                master.gain_db,
                2,
                view.ticks,
                None,
            );
            if let Some(db) = moved {
                let db = if db <= MIN_DB + 0.5 { -96.0 } else { db };
                app.modify_merged(r.id, |p| p.master.gain_db = db);
            }
            // Its volume before the effects (Pre FX) or after them (Post FX).
            r.context_menu(|ui| {
                for (label, pre) in [(tr("Pre FX"), true), (tr("Post FX"), false)] {
                    if crate::widgets::radio(ui, post == pre, label).clicked() {
                        app.modify(|p| p.master_fx_post_fader = pre);
                        ui.close();
                    }
                }
            });
            if let Some(m) = b.meter {
                let r = meter_at(
                    ui,
                    m,
                    base.with("meter"),
                    Some(&app.meters),
                    app.console.meter_scale,
                )
                .on_hover_text(tr("Click to clear the clip lights"));
                if r.clicked() {
                    app.meters.reset_clip();
                }
                roll_scale(ui, app, &r);
            }
            value_row(
                ui,
                app,
                MeterOf::Master,
                master.gain_db.max(MIN_DB),
                b.fader.center().x,
                true,
            );
        }
        to_foot(ui);
        if name_row(ui, tr("Master"), tr("Master"), false).clicked() {
            pick_master(app);
        }
    });
    if ground.clicked() {
        pick_master(app);
    }
}

/// The Preview Bus's channel (VEGAS's Show Channels › Preview Bus), as
/// VEGAS 23's: its name, the device previews play on, its meters, its
/// fader (the level media previews play at) and its value, its name again
/// at the foot on a dark ground.
fn preview_strip(ui: &mut egui::Ui, app: &mut BananaApp, sizes: &Sizes, view: &ConsoleView) {
    let base = egui::Id::new("console-preview");
    column(ui, base, sizes.width, sizes.height, false, |ui| {
        header(ui, Badge::None, tr("Preview"), false);
        rule(ui);
        // The Insert FX, Sends and VU meters regions: empty for previews.
        region(ui, view, Region::InsertFx, sizes.insert_fx, |_, _| {});
        region(ui, view, Region::Sends, sizes.sends, |_, _| {});
        region(ui, view, Region::Io, sizes.io, |ui, _| {
            if sizes.io_rows > 1 {
                blank_row(ui, IO_ROW);
            }
            let name = crate::i18n::output_name(app.project(), Output::Hardware(0));
            io_text(ui, &name, tr("Output"));
        });
        region(ui, view, Region::VuMeters, METERS_H, |_, _| {});
        region(ui, view, Region::Meters, PEAKS_H, |ui, rect| {
            let r = peaks_at(
                ui,
                rect,
                base.with("peaks"),
                Some(&app.previews.meters),
                app.console.meter_scale,
            );
            roll_scale(ui, app, &r);
        });
        if view.shows(Region::Faders) {
            if view.labels {
                region_label(ui, Region::Faders);
            }
            blank_row(ui, AUTO_ROW);
            blank_row(ui, PAN_H);
            let b = block(ui, sizes.fader, !view.shows(Region::Meters), false);
            let (moved, released, _) = fader_at(
                ui,
                b.fader,
                base.with("fader"),
                app.settings.preview_gain_db,
                1,
                view.ticks,
                None,
            );
            if let Some(db) = moved {
                let db = if db <= MIN_DB + 0.5 { -96.0 } else { db };
                app.settings.preview_gain_db = db;
                app.engine.set_preview_gain(db);
            }
            if released {
                let _ = app.settings.save();
            }
            if let Some(m) = b.meter {
                let r = meter_at(
                    ui,
                    m,
                    base.with("meter"),
                    Some(&app.previews.meters),
                    app.console.meter_scale,
                );
                roll_scale(ui, app, &r);
            }
            value_label(ui, app.settings.preview_gain_db, b.fader.center().x);
        }
        to_foot(ui);
        name_row(ui, tr("Preview"), tr("Preview Bus"), true);
    });
}

/// The Insert FX region: each effect a button (red when bypassed: a click
/// shows it, the right-click shows, bypasses, removes or replaces it or
/// loads a preset), and + to add one.
fn insert_fx(ui: &mut egui::Ui, app: &mut BananaApp, target: FxTarget, width: f32) {
    let Some(chain) = target.chain(app.project()) else {
        return;
    };
    ui.spacing_mut().item_spacing.y = 1.0;
    for (i, e) in chain.iter().enumerate() {
        let name = crate::ofx::fx_name(e);
        let text = RichText::new(&name).size(11.0).color(if e.enabled {
            theme::text()
        } else {
            theme::error()
        });
        let r = ui
            .add_sized(
                vec2(width - 2.0, FX_ROW - 1.0),
                egui::Button::new(text).truncate(),
            )
            .on_hover_text(&name);
        if r.clicked() {
            app.clip_windows.fx_node = Some((target, i));
        }
        r.context_menu(|ui| effect_menu(ui, app, target, &chain, i));
    }
    let add = ui
        .add_sized(vec2(width - 2.0, FX_ROW - 1.0), egui::Button::new("+"))
        .on_hover_text(tr("Add a plug-in"));
    egui::Popup::menu(&add).show(|ui| {
        add_menu(ui, |kind| {
            let mut c = chain.clone();
            c.push(Effect::new(kind));
            let n = c.len() - 1;
            app.modify(|p| target.set_chain(p, c));
            app.clip_windows.fx_node = Some((target, n));
        })
    });
}

/// BananaV's audio plug-ins by type (EQ, Dynamics, Reverbs…): one picked.
fn add_menu(ui: &mut egui::Ui, mut pick: impl FnMut(EffectKind)) {
    let mut categories: Vec<&'static str> = Vec::new();
    for k in EffectKind::AUDIO {
        if !categories.contains(&k.category()) {
            categories.push(k.category());
        }
    }
    for c in categories {
        ui.menu_button(tr(c), |ui| {
            for k in EffectKind::AUDIO.into_iter().filter(|k| k.category() == c) {
                if ui.button(tr(k.name())).clicked() {
                    pick(k);
                    ui.close();
                }
            }
        });
    }
}

/// The presets of BananaV's own effect `kind`: (Default), BananaV's, then
/// the user's — their names and parameters (the Mixing Console's menu, a
/// Mackie Control's V-Pot 6).
pub(crate) fn effect_presets(
    app: &mut BananaApp,
    kind: bv_timeline::EffectKind,
) -> Vec<(String, Vec<f32>)> {
    let mut presets: Vec<(String, Vec<f32>)> = vec![(
        tr("(Default)").to_string(),
        Effect::new(kind).params.clone(),
    )];
    for (n, pairs) in crate::presets::builtin(kind) {
        let mut x = Effect::new(kind);
        for (j, v) in pairs {
            x.set_param(j, v);
        }
        presets.push((n.to_string(), x.params.clone()));
    }
    presets.extend(app.clip_windows.presets().user(kind).iter().cloned());
    presets
}

/// An effect's right-click: Show, Bypass, Remove, its presets, and the
/// plug-ins to replace it with.
fn effect_menu(
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    target: FxTarget,
    chain: &[Effect],
    i: usize,
) {
    let Some(e) = chain.get(i) else {
        return;
    };
    let name = crate::ofx::fx_name(e);
    if ui.button(trf!("Show {}", name)).clicked() {
        app.clip_windows.fx_node = Some((target, i));
        ui.close();
    }
    let mut bypassed = !e.enabled;
    if crate::widgets::checkbox(ui, &mut bypassed, trf!("Bypass {}", name)).clicked() {
        let mut c = chain.to_vec();
        c[i].enabled = !bypassed;
        app.modify(|p| target.set_chain(p, c));
        ui.close();
    }
    if ui.button(trf!("Remove {}", name)).clicked() {
        let mut c = chain.to_vec();
        c.remove(i);
        app.modify(|p| target.set_chain(p, c));
        ui.close();
    }
    // Presets: BananaV's own effects' (a plug-in's are in its window).
    let own = e.plugin.is_none() && e.vst.is_none();
    ui.add_enabled_ui(own, |ui| {
        ui.menu_button(tr("Presets"), |ui| {
            let current = e.params.clone();
            for (n, params) in effect_presets(app, e.kind) {
                let on = params == current;
                if crate::widgets::radio(ui, on, n).clicked() {
                    let mut c = chain.to_vec();
                    for (j, v) in params.iter().enumerate() {
                        c[i].set_param(j, *v);
                    }
                    app.modify(|p| target.set_chain(p, c));
                    ui.close();
                }
            }
        });
    });
    ui.separator();
    // Another plug-in in its place.
    add_menu(ui, |kind| {
        let mut c = chain.to_vec();
        c[i] = Effect::new(kind);
        app.modify(|p| target.set_chain(p, c));
    });
}

/// A strip's sends: a track's to each bus and chain, a bus's to each
/// chain; a chain's input level.
fn sends_region(ui: &mut egui::Ui, app: &mut BananaApp, of: Of, rect: Rect, sizes: &Sizes) {
    let p = app.project();
    let row = |i: usize| {
        Rect::from_min_size(
            pos2(rect.left(), rect.top() + i as f32 * SEND_ROW),
            vec2(rect.width(), SEND_ROW - 3.0),
        )
    };
    if let Of::Strip(Strip::Chain(id)) = of {
        let Some(input) = p.fx_chain(id).map(|c| c.input_db) else {
            return;
        };
        let key = egui::Id::new(("console-input", id.0));
        let (moved, resp) = mini_fader(ui, row(0), key, tr("Input"), input, false);
        if let Some(db) = moved {
            let db = if db.is_finite() { db } else { -96.0 };
            app.modify_merged(key, |p| {
                if let Some(c) = p.fx_chain_mut(id) {
                    c.input_db = db;
                }
            });
        }
        resp.on_hover_text(tr("Input level of the effects chain"));
        return;
    }
    let (sends, targets): (Vec<Send>, Vec<SendTo>) = match of {
        Of::Track(id) => (
            p.track(id).map(|t| t.sends.clone()).unwrap_or_default(),
            p.busses
                .iter()
                .map(|b| SendTo::Bus(b.id))
                .chain(p.fx_chains.iter().map(|c| SendTo::Fx(c.id)))
                .collect(),
        ),
        Of::Strip(Strip::Bus(id)) => (
            p.bus(id).map(|b| b.sends.clone()).unwrap_or_default(),
            bus_send_targets(p, id),
        ),
        Of::Strip(Strip::Chain(_)) => return,
    };
    let names: Vec<String> = targets
        .iter()
        .map(|to| match *to {
            SendTo::Bus(b) => crate::i18n::bus_name(p, b),
            SendTo::Fx(c) => crate::i18n::chain_name(p, c),
        })
        .collect();
    let first = app
        .console
        .send_first
        .min(sizes.send_total.saturating_sub(sizes.send_rows));
    for (i, (to, name)) in targets.into_iter().zip(names).enumerate() {
        if i < first || i >= first + sizes.send_rows {
            continue;
        }
        let send = sends
            .iter()
            .find(|s| s.to == to)
            .copied()
            .unwrap_or_else(|| match of {
                Of::Strip(_) => Send::of_bus(to),
                Of::Track(_) => Send::new(to),
            });
        let owner = match of {
            Of::Track(id) => id.0,
            Of::Strip(Strip::Bus(id)) => 1_000_000 + id.0 as u64,
            Of::Strip(Strip::Chain(_)) => 0,
        };
        let key = egui::Id::new(("console-send", owner, format!("{to:?}")));
        let db = if send.gain_db <= -96.0 {
            MIN_DB
        } else {
            send.gain_db
        };
        let (moved, resp) = mini_fader(ui, row(i - first), key, &name, db, send.pre_volume);
        let set = move |p: &mut Project, s: Send| match of {
            Of::Track(id) => {
                p.set_send(id, s);
            }
            Of::Strip(Strip::Bus(id)) => {
                p.set_bus_send(id, s);
            }
            Of::Strip(Strip::Chain(_)) => {}
        };
        if let Some(db) = moved {
            let mut s = send;
            s.gain_db = if db <= MIN_DB + 0.05 {
                f32::NEG_INFINITY
            } else {
                db
            };
            app.modify_merged(key, |p| set(p, s));
        }
        let tip = if send.pre_volume {
            tr("Send (Pre Volume); right-click: Pre or Post Volume")
        } else {
            tr("Send (Post Volume); right-click: Pre or Post Volume")
        };
        let resp = resp.on_hover_text(tip);
        resp.context_menu(|ui| {
            for (label, pre) in [(tr("Pre Volume"), true), (tr("Post Volume"), false)] {
                if crate::widgets::radio(ui, send.pre_volume == pre, label).clicked() {
                    app.modify(|p| {
                        set(
                            p,
                            Send {
                                pre_volume: pre,
                                ..send
                            },
                        )
                    });
                    ui.close();
                }
            }
            if matches!(of, Of::Track(_)) {
                ui.separator();
                let mut link = send.link_pan;
                if crate::widgets::checkbox(ui, &mut link, tr("Link to Main Track Pan")).clicked() {
                    app.modify(|p| {
                        set(
                            p,
                            Send {
                                link_pan: link,
                                ..send
                            },
                        )
                    });
                    ui.close();
                }
            }
        });
    }
}

/// The Sends region's bottom divider: dragged, more or fewer sends show;
/// the mouse wheel over the region scrolls them.
fn send_divider(ui: &mut egui::Ui, app: &mut BananaApp, of: Of, rect: Rect, sizes: &Sizes) {
    if sizes.send_total == 0 {
        return;
    }
    let line = Rect::from_min_max(pos2(rect.left(), rect.bottom() - DIVIDER_H), rect.max);
    let r = ui
        .interact(
            line,
            egui::Id::new(("send-divider", format!("{of:?}"))),
            Sense::drag(),
        )
        .on_hover_cursor(egui::CursorIcon::ResizeVertical)
        .on_hover_text(tr("Drag to show more or fewer sends"));
    let color = if r.hovered() || r.dragged() {
        theme::text()
    } else {
        theme::text_dim()
    };
    ui.painter()
        .hline(line.x_range(), line.center().y, Stroke::new(1.0, color));
    let v = &mut app.console;
    if r.drag_started() {
        v.send_drag = Some((sizes.send_rows, 0.0));
    }
    if r.dragged()
        && let Some((start, moved)) = v.send_drag.as_mut()
    {
        *moved += r.drag_delta().y;
        let rows = (*start as f32 + *moved / SEND_ROW).round();
        v.send_rows = Some(rows.clamp(1.0, sizes.send_total as f32) as usize);
    }
    if r.drag_stopped() {
        v.send_drag = None;
    }
    if sizes.send_total > sizes.send_rows && ui.rect_contains_pointer(rect) {
        let (notches, _) = ui.input(|i| crate::widgets::wheel_notches(&i.events));
        if notches != 0.0 {
            let max = (sizes.send_total - sizes.send_rows) as f32;
            let first = v.send_first as f32 - notches.signum();
            v.send_first = first.clamp(0.0, max) as usize;
            ui.input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
        }
    }
}

/// A send's (or a chain's input's) mini fader, as VEGAS's console draws
/// them: `name` and the level over a bar filled to it (yellow when taken
/// before the volume). Dragging sets it (Ctrl: fine steps), a double-click
/// 0 dB. Returns the level dragged to (dB; −∞ at the left end).
fn mini_fader(
    ui: &mut egui::Ui,
    rect: Rect,
    id: egui::Id,
    name: &str,
    db: f32,
    pre: bool,
) -> (Option<f32>, egui::Response) {
    let resp = ui.interact(rect, id, Sense::click_and_drag());
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, theme::panel());
    let pos = if db <= MIN_DB + 0.05 {
        0.0
    } else {
        crate::widgets::db_to_pos(db, MIN_DB, MAX_DB)
    };
    let fill = Rect::from_min_max(
        rect.min,
        pos2(rect.left() + rect.width() * pos, rect.bottom()),
    );
    let color = if pre {
        theme::banana_dim()
    } else {
        theme::selection()
    };
    painter.rect_filled(fill, 2.0, color);
    let font = FontId::proportional(10.0);
    let value = painter.text(
        pos2(rect.right() - 3.0, rect.center().y),
        Align2::RIGHT_CENTER,
        level_text(db),
        font.clone(),
        theme::text(),
    );
    crate::widgets::fit_text(
        painter,
        pos2(rect.left() + 3.0, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        font,
        theme::text(),
        (value.left() - rect.left() - 6.0).max(1.0),
    );
    let mut out = None;
    if resp.dragged() {
        let fine = if ui.input(|i| i.modifiers.ctrl) {
            0.1
        } else {
            1.0
        };
        let d = resp.drag_delta().x / rect.width() * fine;
        if d != 0.0 {
            let p = (pos + d).clamp(0.0, 1.0);
            out = Some(if p <= 0.0 {
                f32::NEG_INFINITY
            } else {
                crate::widgets::pos_to_db(p, MIN_DB, MAX_DB)
            });
        }
    }
    if resp.double_clicked() {
        out = Some(0.0);
    }
    (out, resp)
}

/// The I/O region, as VEGAS's: a track's recording input over where the
/// strip plays (a bus's or chain's output in the lower row too, so the
/// outputs line up).
fn io_region(ui: &mut egui::Ui, app: &mut BananaApp, of: Of, sizes: &Sizes) {
    if sizes.io_rows > 1 {
        match of {
            Of::Track(id) => {
                let input = app
                    .project()
                    .track(id)
                    .and_then(|t| t.record_input.clone())
                    .map_or_else(
                        || tr("Default").to_string(),
                        |i| crate::recording::input_label(&i),
                    );
                io_row(ui, &input, tr("Recording input"), |ui| {
                    let project = app.project().clone();
                    crate::timeline::record_input_menu(ui, app, &project, id);
                });
            }
            Of::Strip(_) => blank_row(ui, IO_ROW),
        }
    }
    let output = match of {
        Of::Track(id) => app.project().track(id).map(|t| t.output),
        Of::Strip(s) => app.project().strip_channel(s).map(|c| c.output),
    };
    match output {
        Some(output) => output_row(ui, app, of, output),
        None => blank_row(ui, IO_ROW),
    }
}

/// What a bus's Sends offer: the chains that do not play back into it,
/// then the busses on hardware outputs.
fn bus_send_targets(p: &Project, id: BusId) -> Vec<SendTo> {
    p.fx_chains
        .iter()
        .map(|c| SendTo::Fx(c.id))
        .chain(p.busses.iter().map(|b| SendTo::Bus(b.id)))
        .filter(|to| p.bus_can_send_to(id, *to))
        .collect()
}

/// Where a strip plays: a row naming it, its menu the Master and the
/// busses (those that would play back into a bus or a chain greyed), and
/// for a bus the output device's pairs of channels.
fn output_row(ui: &mut egui::Ui, app: &mut BananaApp, of: Of, output: Output) {
    let pairs = match of {
        Of::Strip(Strip::Bus(_)) => app.engine.hardware_outputs(),
        _ => 0,
    };
    let p = app.project();
    let name = |o: Output| crate::i18n::output_name(p, o);
    let label = name(output);
    let choices: Vec<(Output, String, bool)> = std::iter::once(Output::Master)
        .chain(p.busses.iter().map(|b| Output::Bus(b.id)))
        .chain((0..pairs).map(Output::Hardware))
        .map(|o| {
            let fits = match of {
                Of::Track(_) => true,
                Of::Strip(Strip::Bus(id)) => p.bus_can_route(id, o),
                Of::Strip(Strip::Chain(id)) => p.chain_can_route(id, o),
            };
            (o, name(o), fits)
        })
        .collect();
    io_row(ui, &label, tr("Output"), |ui| {
        for (o, text, fits) in choices {
            let picked = ui
                .add_enabled_ui(fits, |ui| crate::widgets::radio(ui, o == output, text))
                .inner;
            if picked.clicked() {
                app.modify(|p| match of {
                    Of::Track(id) => p.set_track_output(id, o),
                    Of::Strip(Strip::Bus(id)) => p.set_bus_output(id, o),
                    Of::Strip(Strip::Chain(id)) => p.set_chain_output(id, o),
                });
                ui.close();
            }
        }
    });
}

/// The VU Meters region: the strip's VU/PPM meter (hidden for that
/// channel from its menu); the right-click is the meters' menu, the wheel
/// goes through the kinds.
fn vu_region(ui: &mut egui::Ui, app: &mut BananaApp, of: MeterOf, rect: Rect, view: &ConsoleView) {
    let resp = ui.interact(
        rect,
        ui.id().with(("vu", format!("{of:?}"))),
        Sense::click(),
    );
    if !view.vu_hidden.contains(&of) {
        crate::vu_meters::paint(
            ui,
            rect.shrink2(vec2(4.0, 0.0)),
            app.vu.get(of),
            &view.vu_options(),
        );
    }
    if resp.hovered() {
        let (notches, _) = ui.input(|i| crate::widgets::wheel_notches(&i.events));
        if notches.abs() >= 1.0 {
            let at = MeterKind::ALL
                .iter()
                .position(|k| *k == app.console.vu_kind)
                .unwrap_or(0) as i32;
            let n = (at - notches.signum() as i32).rem_euclid(MeterKind::ALL.len() as i32);
            app.console.vu_kind = MeterKind::ALL[n as usize];
        }
    }
    resp.on_hover_text(tr(app.console.vu_kind.name()))
        .context_menu(|ui| vu_menu(ui, app, Some(of)));
}

/// The Meters region: the strip's meters at the console's scale (the
/// wheel changes it).
fn peak_meter(ui: &mut egui::Ui, app: &mut BananaApp, of: Of, rect: Rect) {
    let meter = match of {
        Of::Track(id) => app.track_meters.get(&id),
        Of::Strip(s) => app.strip_meters.get(&s),
    };
    let r = peaks_at(
        ui,
        rect,
        ui.id().with("peak-meter"),
        meter,
        app.console.meter_scale,
    );
    roll_scale(ui, app, &r);
}

/// The Faders region, as VEGAS 23's: the automation row, the pan (in 5.1
/// the small surround panner), then the button column — a track's mute,
/// solo, effects, phase and arm (its right-click: the Input Monitor
/// Mode); a bus's or chain's mute, solo, effects and their order —, the
/// fader with a meter beside it when the Meters region is hidden, and the
/// fader's value under it. With Automation Settings on, the pan's mark,
/// the mute and the fader's knob show they are automation controls.
fn faders_region(
    ui: &mut egui::Ui,
    app: &mut BananaApp,
    of: Of,
    sizes: &Sizes,
    view: &ConsoleView,
    base: egui::Id,
) {
    if view.labels {
        region_label(ui, Region::Faders);
    }
    let side = !view.shows(Region::Meters);
    let p = app.project();
    match of {
        Of::Track(id) => {
            let Some(t) = p.track(id) else {
                return;
            };
            let (auto, muted, solo, phase, armed, has_fx, monitor) = (
                t.automation,
                t.muted,
                t.solo,
                t.invert_phase,
                t.armed,
                !t.effects.is_empty(),
                t.input_monitor,
            );
            let gang = ganged_tracks(app, id);
            let (toggle, mode) = automation_row(ui, auto.controls, auto.mode, true);
            if toggle {
                app.toggle_automation_controls(id);
            }
            if let Some(m) = mode {
                app.set_automation_mode(id, m);
            }
            // Pan through the track's controls (trims, or automation as
            // its mode says); in 5.1 the small surround panner.
            let owner = bv_timeline::SurroundOwner::Track(id);
            if app.project().surround_pans(owner) {
                surround_row(ui, app, owner);
            } else {
                let pan = app.control_value(id, TrackControl::Pan);
                let (moved, r) = pan_row(ui, pan, auto.controls);
                let mut new_pan = moved.unwrap_or(pan);
                if let Some(v) = crate::widgets::joystick_push(ui, &r) {
                    new_pan = (new_pan + v.x).clamp(-1.0, 1.0);
                }
                r.context_menu(|ui| crate::timeline::pan_type_items(ui, app, id));
                // The tracks picked move together.
                if new_pan != pan {
                    app.set_controls(id, TrackControl::Pan, new_pan, r.id);
                }
                if r.drag_stopped() || r.double_clicked() {
                    app.release_controls(id, TrackControl::Pan);
                }
            }
            let b = block(ui, sizes.fader, side, true);
            // With Automation Settings the mute is an automation control.
            let owner = AutoOwner::Track(id);
            let mute = if auto.controls {
                mute_automation_cell(
                    ui,
                    b.cell(0),
                    base.with("mute"),
                    app.owner_muted(owner),
                    tr("Mute Track"),
                )
            } else {
                letter_cell(
                    ui,
                    b.cell(0),
                    base.with("mute"),
                    "M",
                    muted,
                    theme::error(),
                    true,
                    tr("Mute Track"),
                )
            };
            if mute.clicked() {
                if auto.controls {
                    app.click_mute(owner, mute.id);
                } else {
                    app.edit(|p| gang.iter().try_for_each(|t| p.set_track_muted(*t, !muted)));
                }
            }
            if letter_cell(
                ui,
                b.cell(1),
                base.with("solo"),
                "S",
                solo,
                theme::banana(),
                true,
                "Solo",
            )
            .clicked()
            {
                app.edit(|p| gang.iter().try_for_each(|t| p.set_track_solo(*t, !solo)));
            }
            if fx_cell(
                ui,
                b.cell(2),
                base.with("fx"),
                has_fx,
                false,
                tr("Audio Track FX"),
            )
            .clicked()
            {
                app.clip_windows.track_fx = Some(id);
            }
            if icon_cell(
                ui,
                b.cell(3),
                base.with("phase"),
                Icon::Phase,
                phase,
                true,
                tr("Invert Track Phase"),
            )
            .clicked()
            {
                app.modify(|p| {
                    for t in &gang {
                        if let Some(t) = p.track_by_id_mut(*t) {
                            t.invert_phase = !phase;
                        }
                    }
                });
            }
            let arm = arm_cell(ui, b.cell(4), base.with("arm"), armed, tr("Arm for Record"));
            if arm.clicked() {
                app.toggle_arm(id);
            }
            // Its right-click: the Input Monitor Mode — what it records
            // heard through its effects, always or (Auto) while stopped
            // and recording.
            use bv_timeline::InputMonitor;
            arm.context_menu(|ui| {
                ui.label(RichText::new(tr("Input Monitor Mode")).color(theme::text_dim()));
                for (m, label) in [
                    (InputMonitor::Off, tr("Input Monitoring Off")),
                    (InputMonitor::On, tr("Input Monitoring On")),
                    (InputMonitor::Auto, tr("Input Monitoring Auto")),
                ] {
                    if crate::widgets::radio(ui, monitor == m, label).clicked() {
                        app.modify(|p| {
                            for t in &gang {
                                if let Some(t) = p.track_by_id_mut(*t) {
                                    t.input_monitor = m;
                                }
                            }
                        });
                        ui.close();
                    }
                }
            });
            // The volume through the track's controls; the tracks picked
            // by as many dB.
            let volume = app.control_value(id, TrackControl::Volume);
            let (moved, released, r) = fader_at(
                ui,
                b.fader,
                base.with("fader"),
                gain_to_db(volume),
                1,
                view.ticks,
                auto.controls.then_some(AUTOMATION_RED),
            );
            if let Some(db) = moved {
                app.set_controls(id, TrackControl::Volume, db_to_gain(db), r.id);
            }
            if released {
                app.release_controls(id, TrackControl::Volume);
            }
            if let Some(m) = b.meter {
                let r = meter_at(
                    ui,
                    m,
                    base.with("meter"),
                    app.track_meters.get(&id),
                    app.console.meter_scale,
                );
                roll_scale(ui, app, &r);
            }
            let shown = if volume <= 0.0 {
                MIN_DB
            } else {
                gain_to_db(volume)
            };
            value_row(
                ui,
                app,
                MeterOf::Track(id),
                shown,
                b.fader.center().x,
                false,
            );
        }
        Of::Strip(s) => {
            let Some(ch) = p.strip_channel(s) else {
                return;
            };
            let (auto, solo, post, has_fx) = (
                ch.automation,
                ch.solo,
                ch.fx_post_fader,
                !ch.effects.is_empty(),
            );
            let owner = AutoOwner::Strip(s);
            let gang = ganged_strips(app, s);
            let (toggle, mode) = automation_row(ui, auto.controls, auto.mode, true);
            if toggle {
                app.toggle_owner_automation_controls(owner);
            }
            if let Some(m) = mode {
                app.set_owner_automation_mode(owner, m);
            }
            // Pan: a trim, or automation as its mode says; the others
            // picked by as much.
            let surround = bv_timeline::SurroundOwner::Strip(s);
            if app.project().surround_pans(surround) {
                surround_row(ui, app, surround);
            } else {
                let pan = app.owner_value(owner, TrackControl::Pan);
                let (moved, r) = pan_row(ui, pan, auto.controls);
                if let Some(new_pan) = moved {
                    for o in &gang {
                        let o = AutoOwner::Strip(*o);
                        let v = if o == owner {
                            new_pan
                        } else {
                            (app.owner_value(o, TrackControl::Pan) + new_pan - pan).clamp(-1.0, 1.0)
                        };
                        app.set_owner_control(o, TrackControl::Pan, v, r.id);
                    }
                }
                if r.drag_stopped() || r.double_clicked() {
                    for o in &gang {
                        app.release_owner_control(AutoOwner::Strip(*o), TrackControl::Pan);
                    }
                }
            }
            let b = block(ui, sizes.fader, side, true);
            let tips = match s {
                Strip::Bus(_) => (tr("Mute Bus"), tr("Solo: mutes the busses not soloed")),
                Strip::Chain(_) => (
                    tr("Mute Assignable FX"),
                    tr("Solo: mutes the busses and chains not soloed"),
                ),
            };
            let muted = app.owner_muted(owner);
            let mute = if auto.controls {
                mute_automation_cell(ui, b.cell(0), base.with("mute"), muted, tips.0)
            } else {
                letter_cell(
                    ui,
                    b.cell(0),
                    base.with("mute"),
                    "M",
                    muted,
                    theme::error(),
                    true,
                    tips.0,
                )
            };
            if mute.clicked() {
                // The others picked follow it.
                for o in &gang {
                    let o = AutoOwner::Strip(*o);
                    if app.owner_muted(o) == muted {
                        app.click_mute(o, mute.id);
                    }
                }
            }
            if letter_cell(
                ui,
                b.cell(1),
                base.with("solo"),
                "S",
                solo,
                theme::banana(),
                true,
                tips.1,
            )
            .clicked()
            {
                app.modify(|p| {
                    for s in &gang {
                        if let Some(c) = p.strip_channel_mut(*s) {
                            c.solo = !solo;
                        }
                    }
                });
            }
            let (tip, target) = match s {
                Strip::Bus(id) => (tr("Bus FX"), FxTarget::Bus(id)),
                Strip::Chain(id) => (tr("Assignable FX"), FxTarget::Chain(id)),
            };
            if fx_cell(ui, b.cell(2), base.with("fx"), has_fx, true, tip).clicked() {
                app.clip_windows.fx_target = Some(target);
            }
            // Its effects before its volume or after it.
            if fx_order_cell(ui, b.cell(3), base.with("order"), post).clicked() {
                app.modify(|p| {
                    if let Some(c) = p.strip_channel_mut(s) {
                        c.fx_post_fader = !post;
                    }
                });
            }
            let gain_db = gain_to_db(app.owner_value(owner, TrackControl::Volume));
            let (moved, released, r) = fader_at(
                ui,
                b.fader,
                base.with("fader"),
                gain_db,
                1,
                view.ticks,
                auto.controls.then_some(AUTOMATION_RED),
            );
            let key = r.id;
            if let Some(db) = moved {
                let g = if db <= MIN_DB + 0.5 {
                    0.0
                } else {
                    db_to_gain(db)
                };
                // The others picked by as many dB.
                let before = app.owner_value(owner, TrackControl::Volume);
                for o in &gang {
                    let o = AutoOwner::Strip(*o);
                    let v = if o == owner {
                        g
                    } else if before > 1e-6 {
                        (app.owner_value(o, TrackControl::Volume) * g / before)
                            .clamp(0.0, bv_timeline::envelope::MAX_VOLUME)
                    } else {
                        g
                    };
                    app.set_owner_control(o, TrackControl::Volume, v, key);
                }
            }
            if released {
                for o in &gang {
                    app.release_owner_control(AutoOwner::Strip(*o), TrackControl::Volume);
                }
            }
            if let Some(m) = b.meter {
                let r = meter_at(
                    ui,
                    m,
                    base.with("meter"),
                    app.strip_meters.get(&s),
                    app.console.meter_scale,
                );
                roll_scale(ui, app, &r);
            }
            value_row(
                ui,
                app,
                MeterOf::Strip(s),
                gain_db.max(MIN_DB),
                b.fader.center().x,
                false,
            );
        }
    }
}

/// The fader in `rect`, its ticks left of it when they show, its knob
/// `tint`ed (red while it is an automation control); the mouse wheel
/// moves it (0.5 dB a notch, Ctrl 0.1). The level dragged to (dB),
/// whether it was let go, its response.
fn fader_at(
    ui: &mut egui::Ui,
    rect: Rect,
    id: egui::Id,
    db: f32,
    squares: u8,
    ticks: bool,
    tint: Option<Color32>,
) -> (Option<f32>, bool, egui::Response) {
    if ticks {
        let track = Rect::from_center_size(rect.center(), vec2(4.0, rect.height() - 16.0));
        let x = rect.center().x.round();
        // Ticks this small: a mesh, or they feather away.
        let mut mesh = egui::Mesh::default();
        for t in [
            12.0, 6.0, 0.0, -6.0, -12.0, -18.0, -24.0, -36.0, -48.0, -60.0,
        ] {
            let y = (track.bottom()
                - crate::widgets::db_to_pos(t, MIN_DB, MAX_DB) * track.height())
            .round();
            let len = if t == 0.0 { 6.0 } else { 4.0 };
            mesh.add_colored_rect(
                Rect::from_min_size(pos2(x - 6.0 - len, y), vec2(len, 1.0)),
                Color32::from_gray(0x68),
            );
        }
        ui.painter().add(egui::Shape::mesh(mesh));
    }
    let mut v = db.max(MIN_DB);
    let r = crate::widgets::vertical_fader_in(ui, rect, id, &mut v, MIN_DB, MAX_DB, squares, tint);
    let mut moved = r.changed().then_some(v);
    let mut released = r.drag_stopped() || r.double_clicked();
    if r.hovered() {
        let (notches, ctrl) = ui.input(|i| crate::widgets::wheel_notches(&i.events));
        if notches != 0.0 {
            let step = if ctrl { 0.1 } else { 0.5 };
            moved = Some((v + notches * step).clamp(MIN_DB, MAX_DB));
            released = true;
            ui.ctx()
                .input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
        }
    }
    (moved, released, r)
}

/// The fader's value under it at `x` ("0,0"; with `pair` twice, the
/// Master's pair): a double-click types it ("-6", "-inf").
fn value_row(ui: &mut egui::Ui, app: &mut BananaApp, of: MeterOf, db: f32, x: f32, pair: bool) {
    let w = ui.max_rect().width();
    let (row, _) = ui.allocate_exact_size(vec2(w, VALUE_H), Sense::hover());
    let width = if pair { 62.0 } else { 44.0 };
    let text_r = Rect::from_center_size(pos2(x, row.center().y), vec2(width, VALUE_H - 2.0));
    let typing = app.console.typing.as_ref().is_some_and(|(o, _)| *o == of);
    if !typing {
        let resp = ui.interact(
            text_r,
            egui::Id::new(("console-value", format!("{of:?}"))),
            Sense::click(),
        );
        let text = value_text(db);
        let painter = ui.painter();
        let font = FontId::proportional(11.0);
        let xs: &[f32] = if pair { &[-15.0, 15.0] } else { &[0.0] };
        for dx in xs {
            painter.text(
                pos2(x + dx, row.center().y),
                Align2::CENTER_CENTER,
                &text,
                font.clone(),
                theme::text(),
            );
        }
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &text));
        if resp
            .on_hover_text(tr("Double-click to type the value"))
            .double_clicked()
        {
            app.console.typing = Some((of, crate::fmt::num(db as f64, 1)));
        }
        return;
    }
    let Some((_, text)) = app.console.typing.as_mut() else {
        return;
    };
    let resp = ui.put(
        text_r.expand2(vec2(10.0, 0.0)),
        egui::TextEdit::singleline(text),
    );
    if !resp.has_focus() && !resp.lost_focus() {
        resp.request_focus();
    }
    let cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));
    if resp.lost_focus() || cancel {
        let text = app
            .console
            .typing
            .take()
            .map(|(_, t)| t)
            .unwrap_or_default();
        if cancel {
            return;
        }
        let Some(db) = parse_db(&text) else {
            return;
        };
        match of {
            MeterOf::Track(id) => {
                let key = egui::Id::new(("console-typed", id.0));
                app.set_control(id, TrackControl::Volume, db_to_gain(db), key);
                app.release_control(id, TrackControl::Volume);
            }
            MeterOf::Strip(s) => {
                let db = if db <= MIN_DB { -96.0 } else { db };
                app.modify(|p| {
                    if let Some(c) = p.strip_channel_mut(s) {
                        c.gain_db = db;
                    }
                });
            }
            MeterOf::Master => {
                let db = if db <= MIN_DB { -96.0 } else { db };
                app.modify(|p| p.master.gain_db = db);
            }
        }
    }
}

/// A fader's value under it at `x`, only shown (the preview's, an input
/// bus's).
fn value_label(ui: &mut egui::Ui, db: f32, x: f32) {
    let w = ui.max_rect().width();
    let (row, _) = ui.allocate_exact_size(vec2(w, VALUE_H), Sense::hover());
    ui.painter().text(
        pos2(x, row.center().y),
        Align2::CENTER_CENTER,
        value_text(db.max(MIN_DB)),
        FontId::proportional(11.0),
        theme::text(),
    );
}

/// A track's name at the foot of its strip — none while it has the name
/// BananaV gave it, as VEGAS's unnamed tracks —: a click picks the track,
/// a drag moves it.
fn track_name_ui(ui: &mut egui::Ui, app: &mut BananaApp, id: TrackId) {
    let Some(t) = app.project().track(id) else {
        return;
    };
    let name = if given_name(t) {
        String::new()
    } else {
        crate::i18n::track_name(t)
    };
    let r = name_row(ui, &name, tr("Drag to move the channel"), false);
    pick_or_drag(ui, app, Of::Track(id), &r);
}

/// Whether a track has the name BananaV gives new ones ("Audio 3"), or
/// none.
fn given_name(t: &bv_timeline::Track) -> bool {
    let prefix = match t.kind {
        TrackKind::Video => "Video ",
        TrackKind::Audio => "Audio ",
    };
    t.name.trim().is_empty()
        || t.name
            .strip_prefix(prefix)
            .is_some_and(|n| n.parse::<u32>().is_ok())
}

/// A bus's or chain's name at the foot of its strip: a click picks it, a
/// double-click (or F2) types a new one, its menu renames or deletes it.
fn strip_name_ui(ui: &mut egui::Ui, app: &mut BananaApp, s: Strip) {
    let name = strip_name(app.project(), s);
    if let Some((r, text)) = app.console.renaming.as_mut()
        && *r == s
    {
        let w = ui.max_rect().width();
        let (rect, _) = ui.allocate_exact_size(vec2(w, NAME_H), Sense::hover());
        let resp = ui.put(
            rect.shrink2(vec2(3.0, 1.0)),
            egui::TextEdit::singleline(text),
        );
        if !resp.has_focus() && !resp.lost_focus() {
            resp.request_focus();
        }
        let cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if resp.lost_focus() || cancel {
            let typed = app
                .console
                .renaming
                .take()
                .map(|(_, t)| t.trim().to_string())
                .unwrap_or_default();
            if !cancel && typed != name {
                // The default name typed back (or none): the default again.
                let default = match s {
                    Strip::Bus(id) => crate::i18n::bus_default_name(app.project(), id),
                    Strip::Chain(id) => crate::i18n::chain_default_name(app.project(), id),
                };
                let typed = if typed == default {
                    String::new()
                } else {
                    typed
                };
                app.modify(|p| {
                    if let Some(c) = p.strip_channel_mut(s) {
                        c.name = typed;
                    }
                });
            }
        }
        return;
    }
    let r = name_row(
        ui,
        &name,
        tr("Double-click to rename; drag to move the channel"),
        false,
    );
    pick_or_drag(ui, app, Of::Strip(s), &r);
    if r.double_clicked() {
        app.console.selected = Some(s);
        app.console.renaming = Some((s, name.clone()));
    }
    r.context_menu(|ui| {
        if ui.button(tr("Rename")).clicked() {
            app.console.selected = Some(s);
            app.console.renaming = Some((s, name.clone()));
            ui.close();
        }
        if ui.button(tr("Delete")).clicked() {
            app.modify(|p| p.remove_strip(s));
            app.console.selected = None;
            ui.close();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_commands_change_the_view() {
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        let audio = app
            .project()
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Audio)
            .map(|t| t.id);
        run(&mut app, ConsoleOp::Width(ChannelWidth::Narrow));
        assert_eq!(app.console.width, ChannelWidth::Narrow);
        run(&mut app, ConsoleOp::ToggleRegion(Region::Faders));
        assert!(!app.console.shows(Region::Faders));
        run(&mut app, ConsoleOp::ShowAllRegions);
        assert!(app.console.shows(Region::Faders) && app.console.shows(Region::VuMeters));
        assert_eq!(app.console.vu_wanted(), Some(MeterKind::TraditionalVu));
        // Hiding needs a selected audio track.
        run(&mut app, ConsoleOp::HideChannel);
        assert!(app.console.hidden.is_empty());
        app.selected_track = audio;
        run(&mut app, ConsoleOp::HideChannel);
        assert_eq!(app.console.hidden, audio.into_iter().collect::<Vec<_>>());
        run(&mut app, ConsoleOp::ToggleMaster);
        assert!(!app.console.show_master);
        run(&mut app, ConsoleOp::ToggleBusses);
        run(&mut app, ConsoleOp::ToggleFx);
        assert!(!app.console.show_busses && !app.console.show_fx);
        run(&mut app, ConsoleOp::ShowAllChannels);
        assert!(app.console.hidden.is_empty() && app.console.show_master);
        assert!(app.console.show_busses && app.console.show_fx);
        for op in [
            ConsoleOp::ToggleChannelList,
            ConsoleOp::ToggleViewControls,
            ConsoleOp::ToggleLabels,
            ConsoleOp::ToggleTicks,
        ] {
            run(&mut app, op);
        }
        let v = &app.console;
        assert!(!v.channel_list && !v.view_controls && v.labels && !v.ticks);
    }

    /// A bus picked in the console: F2 renames it, Delete deletes it — its
    /// track going to the bus before it.
    #[test]
    fn the_picked_bus_is_renamed_and_deleted() {
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        Cmd::InsertAudioBus.run(&mut app);
        Cmd::InsertAudioBus.run(&mut app);
        let (a, b) = (app.project().busses[0].id, app.project().busses[1].id);
        let audio = app
            .project()
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Audio)
            .map(|t| t.id)
            .unwrap();
        app.modify(|p| p.set_track_output(audio, Output::Bus(b)));
        app.console.selected = Some(Strip::Bus(b));
        run(&mut app, ConsoleOp::Rename);
        assert_eq!(
            app.console.renaming,
            Some((Strip::Bus(b), crate::i18n::bus_name(app.project(), b)))
        );
        run(&mut app, ConsoleOp::Delete);
        assert_eq!(app.project().busses.len(), 1);
        assert_eq!(app.project().track(audio).unwrap().output, Output::Bus(a));
        // Undone in one step.
        app.undo();
        assert_eq!(app.project().busses.len(), 2);
    }

    /// A bus on a hardware output: the other busses' Sends offer it (a
    /// send from after their volume at first), never itself.
    #[test]
    fn busses_send_to_a_bus_on_a_hardware_output() {
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        Cmd::InsertAudioBus.run(&mut app);
        Cmd::InsertAudioBus.run(&mut app);
        let (a, b) = (app.project().busses[0].id, app.project().busses[1].id);
        assert!(bus_send_targets(app.project(), a).is_empty());
        assert!(app.engine.hardware_outputs() >= 1);
        app.modify(|p| p.set_bus_output(b, Output::Hardware(0)));
        assert_eq!(bus_send_targets(app.project(), a), vec![SendTo::Bus(b)]);
        assert!(bus_send_targets(app.project(), b).is_empty());
        assert!(!Send::of_bus(SendTo::Bus(b)).pre_volume);
    }

    /// Channels dragged by their names: a track moves in the project, a
    /// bus among the busses; faders of picked channels move together.
    #[test]
    fn channels_move_and_picked_ones_move_together() {
        let mut app = BananaApp::new(egui::Context::default());
        app.load_demo();
        Cmd::InsertAudioBus.run(&mut app);
        Cmd::InsertAudioBus.run(&mut app);
        Cmd::InsertAudioTrack.run(&mut app);
        let audio: Vec<TrackId> = app
            .project()
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Audio)
            .map(|t| t.id)
            .collect();
        assert!(audio.len() >= 2, "two audio tracks");
        let (a, b) = (audio[0], audio[1]);
        let index = |app: &BananaApp, t| app.project().track_index(t).unwrap();
        move_channel(&mut app, Of::Track(a), Of::Track(b), true);
        assert_eq!(index(&app, a), index(&app, b) + 1);
        move_channel(&mut app, Of::Track(a), Of::Track(b), false);
        assert_eq!(index(&app, a) + 1, index(&app, b));
        let busses: Vec<_> = app.project().busses.iter().map(|x| x.id).collect();
        move_channel(
            &mut app,
            Of::Strip(Strip::Bus(busses[0])),
            Of::Strip(Strip::Bus(busses[1])),
            true,
        );
        assert_eq!(app.project().busses[1].id, busses[0]);
        assert_eq!(place(0, 2, false), 1);
        assert_eq!(place(2, 0, true), 1);
        // Two busses picked: Ctrl adds, Shift spans, a click alone.
        pick_strip(&mut app, Strip::Bus(busses[1]), false, false);
        pick_strip(&mut app, Strip::Bus(busses[0]), false, true);
        assert_eq!(strip_selection(&app.console).len(), 2);
        assert_eq!(ganged_strips(&app, Strip::Bus(busses[0])).len(), 2);
        pick_strip(&mut app, Strip::Bus(busses[0]), false, false);
        assert_eq!(strip_selection(&app.console), vec![Strip::Bus(busses[0])]);
        // Two tracks picked: their volumes move by as many dB.
        app.pick_track(a, false, false);
        app.pick_track(b, false, true);
        assert_eq!(ganged_tracks(&app, a).len(), 2);
        let key = egui::Id::new("test-fader");
        app.set_controls(a, TrackControl::Volume, 0.5, key);
        app.release_controls(a, TrackControl::Volume);
        let vb = app.control_value(b, TrackControl::Volume);
        assert!((vb - 0.5).abs() < 1e-4, "{vb}");
    }

    #[test]
    fn typed_levels() {
        assert_eq!(parse_db("-6"), Some(-6.0));
        assert_eq!(parse_db("-6,5 dB"), Some(-6.5));
        assert_eq!(parse_db("-inf"), Some(MIN_DB));
        assert_eq!(parse_db("40"), Some(MAX_DB));
        assert_eq!(parse_db("x"), None);
    }
}
