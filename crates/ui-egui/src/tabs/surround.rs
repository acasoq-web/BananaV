//! The Surround Panner (View › Window › Surround Panner, Ctrl+Alt+3), as
//! VEGAS's Help describes and pictures it: in 5.1 projects it pans the
//! selected audio track — or a bus or a chain, from its bus track or the
//! Mixing Console — in the room (`bv_timeline::surround`). A bar with the
//! owner's badge, the move mode (Move Freely, Left/Right Only, Front/Back
//! Only) and LFE; the room: the five speakers (a click mutes one,
//! Ctrl+click solos it), a shading at each as loud as it plays — its area
//! the speaker's gain, an arc at the gain's radius, the level written —,
//! the grid where the left-to-right and the front-to-back ratios are 6, 0
//! and −6 dB (with the pan type and the Center's trim), the pan point
//! and, with keyframes, its path; the pan type written under it; the
//! Center's fader (hidden while the center is muted) and, with two
//! keyframes or more, Smoothness. The point is dragged (Shift: on a line
//! through the middle; Alt: at its distance from it; Alt+Shift: on the
//! largest circle; Ctrl: fine), double-clicked back to the middle,
//! double-clicked elsewhere to come there, moved by the arrows, Page Up
//! and Down and the wheel, put by the numeric keypad and pushed by the
//! joystick; its menu sets the pan type and LFE Only. With a track's
//! automation controls it sets the Surround Pan Keyframe at the cursor
//! (at the playback position while Touch or Latch records), else the
//! trim. The shapes and proportions are the Help's picture's, measured
//! (`surroundpannertrim.gif`); the track header's, the bus tracks' and the
//! Mixing Console's small panners are [`mini_panner`].

use bv_core::Tick;
use bv_timeline::surround::{C, L, LFE, LS, R, RS};
use bv_timeline::{
    AutomationMode, PanMode, Project, Strip, SurroundOwner, SurroundPan, TrackId, TrackKind,
};

use crate::app::BananaApp;
use crate::egui::{
    self, Align2, Color32, FontId, Key, Pos2, Rect, Sense, Shape, Stroke, pos2, vec2,
};
use crate::i18n::tr;
use crate::theme;

/// How the pan point may move (the bar's button cycles them).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum MoveMode {
    #[default]
    Freely,
    LeftRight,
    FrontBack,
}

impl MoveMode {
    fn next(self) -> MoveMode {
        match self {
            MoveMode::Freely => MoveMode::LeftRight,
            MoveMode::LeftRight => MoveMode::FrontBack,
            MoveMode::FrontBack => MoveMode::Freely,
        }
    }

    fn label(self) -> &'static str {
        match self {
            MoveMode::Freely => tr("Move Freely"),
            MoveMode::LeftRight => tr("Move Left/Right Only"),
            MoveMode::FrontBack => tr("Move Front/Back Only"),
        }
    }
}

/// The window's state.
#[derive(Debug, Default)]
pub(crate) struct SurroundView {
    /// Whose panner it shows (a small panner double-clicked, a bus
    /// track's); picking another track shows that one's.
    pub target: Option<SurroundOwner>,
    /// The track selected when the target was last followed.
    seen_track: Option<TrackId>,
    pub move_mode: MoveMode,
    /// A drag of the pan point: where it and the pointer started.
    drag: Option<([f32; 2], Pos2)>,
    /// What is left of the wheel's notches.
    wheel: f32,
}

/// The field's colors, after the Help's picture.
const SHADE: Color32 = Color32::from_gray(102);
const SHADE_IN: Color32 = Color32::from_gray(92);
const ARC: Color32 = Color32::from_gray(161);
const GRID: Color32 = Color32::from_gray(64);
const LABEL: Color32 = Color32::from_gray(232);
const SPEAKER: Color32 = Color32::from_gray(240);
const POINT: Color32 = Color32::from_rgb(16, 115, 104);
const POINT_EDGE: Color32 = Color32::from_rgb(213, 214, 214);
/// The point as an automation control: a pink cog.
const COG: Color32 = Color32::from_rgb(239, 140, 151);
const COG_EDGE: Color32 = Color32::from_rgb(166, 102, 109);
/// The path's dots, from its start's red-orange to its end's orange-yellow.
const PATH_FROM: Color32 = Color32::from_rgb(255, 102, 0);
const PATH_TO: Color32 = Color32::from_rgb(255, 177, 0);

/// A step of the arrows and the wheel; with Ctrl a tenth of it.
const STEP: f32 = 0.05;

/// The field (a square) and the room in it: the speakers at its corners
/// and the middle of its front edge, the pan point's −1 to 1 each way.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Room {
    pub field: Rect,
    pub room: Rect,
}

impl Room {
    pub fn new(field: Rect) -> Room {
        // The picture's field is 228 pixels, its room 202.
        let inset = field.width().min(field.height()) * 0.057;
        Room {
            field,
            room: field.shrink(inset),
        }
    }

    /// Where `[x, y]` is (y forward, up the screen).
    pub fn at(&self, [x, y]: [f32; 2]) -> Pos2 {
        let c = self.room.center();
        pos2(
            c.x + x * self.room.width() / 2.0,
            c.y - y * self.room.height() / 2.0,
        )
    }

    /// The point at `p`, within the room.
    pub fn point(&self, p: Pos2) -> [f32; 2] {
        let c = self.room.center();
        [
            ((p.x - c.x) / (self.room.width() / 2.0)).clamp(-1.0, 1.0),
            (-(p.y - c.y) / (self.room.height() / 2.0)).clamp(-1.0, 1.0),
        ]
    }

    /// A speaker's place in the room (5.1's channel order; not the LFE).
    fn speaker(&self, c: usize) -> [f32; 2] {
        match c {
            L => [-1.0, 1.0],
            R => [1.0, 1.0],
            C => [0.0, 1.0],
            LS => [-1.0, -1.0],
            _ => [1.0, -1.0],
        }
    }

    /// Where a speaker's icon is: outside the room's corner (above its
    /// front edge for the center).
    fn icon(&self, c: usize) -> Pos2 {
        let [x, y] = self.speaker(c);
        let d = self.room.left() - self.field.left();
        let at = self.at([x, y]);
        if c == C {
            at - vec2(0.0, d * 0.62)
        } else {
            at + vec2(x, -y) * d * 0.45
        }
    }
}

/// The speakers drawn in the room.
const SPEAKERS: [usize; 5] = [L, C, R, LS, RS];

/// A gain in dB as the field writes it ("−6,0"; under −30 dB "−∞", as
/// the Help's pictures write speakers barely fed).
fn db_text(g: f32) -> String {
    if g <= 0.031_622_8 {
        "−∞".into()
    } else {
        crate::fmt::num((20.0 * g.log10()) as f64, 1)
    }
}

/// The pan type as the field writes it: the picture's "Add Channels (0 dB
/// Center)".
fn mode_text(mode: PanMode) -> &'static str {
    match mode {
        PanMode::AddChannels => tr("Add Channels (0 dB Center)"),
        _ => tr(mode.name()),
    }
}

/// Each speaker's gain for a sound the same on both channels.
fn speaker_gains(sp: &SurroundPan, mode: PanMode, point: [f32; 2]) -> [f32; 6] {
    sp.gains(mode, point).map(|g| g[0] + g[1])
}

/// Where the grid's lines are: the left-right places where the left
/// speakers are 6, 0 and −6 dB louder than the right ones, and the
/// front-back places where the front row's power is 6, 0 and −6 dB over the
/// back row's (with the Center's trim; the speakers neither muted nor
/// soloed).
fn grid_lines(sp: &SurroundPan, mode: PanMode) -> (Vec<f32>, Vec<f32>) {
    let plain = SurroundPan {
        center_db: sp.center_db,
        ..SurroundPan::default()
    };
    let lr = |x: f32| {
        let g = speaker_gains(&plain, mode, [x, 0.0]);
        ratio_db(g[L] + g[LS], g[R] + g[RS])
    };
    let fb = |y: f32| {
        let g = speaker_gains(&plain, mode, [0.0, y]);
        let front = g[L] * g[L] + g[C] * g[C] + g[R] * g[R];
        let back = g[LS] * g[LS] + g[RS] * g[RS];
        if back <= 1e-12 {
            f32::INFINITY
        } else if front <= 1e-12 {
            f32::NEG_INFINITY
        } else {
            10.0 * (front / back).log10()
        }
    };
    // Each ratio falls from left to right (rises from back to front);
    // its 6 dB a factor of two.
    let six = 20.0 * 2f32.log10();
    let xs = [six, 0.0, -six]
        .iter()
        .filter_map(|&db| solve(|x| -lr(x), -db))
        .collect();
    let ys = [six, 0.0, -six]
        .iter()
        .filter_map(|&db| solve(fb, db))
        .collect();
    (xs, ys)
}

fn ratio_db(a: f32, b: f32) -> f32 {
    if b <= 1e-9 {
        f32::INFINITY
    } else if a <= 1e-9 {
        f32::NEG_INFINITY
    } else {
        20.0 * (a / b).log10()
    }
}

/// Where rising `f` on −1…1 reaches `target` (bisection), if it does.
fn solve(f: impl Fn(f32) -> f32, target: f32) -> Option<f32> {
    let (mut a, mut b) = (-1.0f32, 1.0f32);
    if f(a) > target || f(b) < target {
        return None;
    }
    for _ in 0..40 {
        let m = 0.5 * (a + b);
        if f(m) < target {
            a = m;
        } else {
            b = m;
        }
    }
    Some(0.5 * (a + b))
}

/// What the field shows of a panner.
pub(crate) struct Shown<'a> {
    pub sp: &'a SurroundPan,
    pub mode: PanMode,
    /// The pan point now.
    pub point: [f32; 2],
    /// Its keyframes' path (two or more).
    pub path: Vec<[f32; 2]>,
    /// The point picked (the keys move it).
    pub focused: bool,
}

/// Paints the room: the field, the grid, the speakers and their shading,
/// the path and the pan point; in the small panners (`small`) only the
/// field, the speakers' marks and the point. `enabled`: something is
/// panned.
pub(crate) fn paint_room(
    painter: &egui::Painter,
    room: &Room,
    shown: Option<&Shown<'_>>,
    small: bool,
    automation: bool,
) {
    let field = room.field;
    painter.rect_filled(field, 0.0, theme::panel_dark());
    painter.rect_stroke(
        field,
        0.0,
        Stroke::new(if small { 1.0 } else { 2.0 }, Color32::from_gray(35)),
        egui::StrokeKind::Outside,
    );
    let Some(s) = shown else {
        for c in SPEAKERS {
            paint_speaker(painter, room, c, theme::text_disabled(), small);
        }
        return;
    };
    let gains = speaker_gains(s.sp, s.mode, s.point);
    if small {
        let mid = room.room.center();
        let stroke = Stroke::new(1.0, GRID);
        painter.line_segment(
            [
                pos2(room.room.left(), mid.y),
                pos2(room.room.right(), mid.y),
            ],
            stroke,
        );
        painter.line_segment(
            [
                pos2(mid.x, room.room.top()),
                pos2(mid.x, room.room.bottom()),
            ],
            stroke,
        );
    } else {
        // The grid: dashed, 3 pixels on and 3 off.
        let (xs, ys) = grid_lines(s.sp, s.mode);
        let stroke = Stroke::new(1.0, GRID);
        for x in xs {
            let px = room.at([x, 0.0]).x;
            painter.extend(Shape::dashed_line(
                &[pos2(px, room.room.top()), pos2(px, room.room.bottom())],
                stroke,
                3.0,
                3.0,
            ));
        }
        for y in ys {
            let py = room.at([0.0, y]).y;
            painter.extend(Shape::dashed_line(
                &[pos2(room.room.left(), py), pos2(room.room.right(), py)],
                stroke,
                3.0,
                3.0,
            ));
        }
        // Each speaker's shading: its area the gain (the radius its
        // square root), the arc at the gain's radius, the level.
        let full = room.room.width() * 0.28;
        for c in SPEAKERS {
            let g = gains[c];
            if g <= 1e-5 {
                continue;
            }
            let center = room.at(room.speaker(c));
            let (from, to) = sweep(c);
            let outer = full * g.sqrt().min(1.6);
            let inner = full * g.min(2.5);
            painter.add(pie(center, outer, from, to, SHADE));
            painter.add(pie(center, inner.min(outer), from, to, SHADE_IN));
            painter.add(Shape::line(
                arc(center, inner, from, to),
                Stroke::new(1.0, ARC),
            ));
        }
        let font = FontId::proportional((room.room.width() * 0.05).clamp(9.0, 12.0));
        let d = room.room.width() * 0.07;
        for c in SPEAKERS {
            if s.sp.lfe_only || s.sp.mute[c] {
                continue;
            }
            let [x, y] = room.speaker(c);
            let at = room.at([x, y]) + vec2(-x * d, y * d * 0.72);
            painter.text(
                at,
                Align2::CENTER_CENTER,
                db_text(gains[c]),
                font.clone(),
                LABEL,
            );
        }
        // LFE Only: everything into the LFE, its level in the middle.
        if s.sp.lfe_only {
            let mid = room.room.center();
            painter.text(
                mid - vec2(0.0, 9.0),
                Align2::CENTER_CENTER,
                "LFE",
                FontId::proportional(18.0),
                Color32::from_gray(150),
            );
            painter.text(
                mid + vec2(0.0, 10.0),
                Align2::CENTER_CENTER,
                db_text(gains[LFE]),
                font.clone(),
                LABEL,
            );
        }
        // The pan type, under the room's middle.
        painter.text(
            pos2(room.room.center().x, room.room.bottom() - 4.0),
            Align2::CENTER_BOTTOM,
            mode_text(s.mode),
            FontId::proportional((room.room.width() * 0.055).clamp(9.0, 12.0)),
            LABEL,
        );
    }
    let soloed = s.sp.solo.iter().any(|v| *v);
    for c in SPEAKERS {
        let color = if s.sp.solo[c] {
            theme::banana()
        } else if s.sp.mute[c] || (soloed && !s.sp.solo[c]) || s.sp.lfe_only {
            Color32::from_gray(90)
        } else {
            SPEAKER
        };
        paint_speaker(painter, room, c, color, small);
    }
    // The keyframes' path: a trail of small rings at even steps of time,
    // red-orange where it starts, orange-yellow where it ends.
    if !small && s.path.len() >= 2 {
        let n = s.path.len() - 1;
        for (i, p) in s.path.iter().enumerate() {
            let t = i as f32 / n as f32;
            let color = Color32::from_rgb(
                255,
                (PATH_FROM.g() as f32 + (PATH_TO.g() as f32 - PATH_FROM.g() as f32) * t) as u8,
                0,
            );
            painter.circle_stroke(room.at(*p), 2.0, Stroke::new(1.0, color));
        }
    }
    if s.sp.lfe_only {
        return;
    }
    // The pan point: a diamond for its trim, a cog as an automation
    // control (the Help's pictures).
    let at = room.at(s.point);
    let h = if small {
        (field.width() * 0.09).clamp(3.0, 5.0)
    } else {
        8.0
    };
    if automation {
        paint_cog(painter, at, h * 0.8);
    } else {
        let diamond = vec![
            at - vec2(0.0, h),
            at + vec2(h, 0.0),
            at + vec2(0.0, h),
            at - vec2(h, 0.0),
        ];
        painter.add(Shape::convex_polygon(
            diamond,
            POINT,
            Stroke::new(1.5, POINT_EDGE),
        ));
    }
    if s.focused {
        painter.circle_stroke(at, h + 4.0, Stroke::new(1.0, POINT_EDGE));
    }
}

/// The automation point: a pink cog of radius `r` (its eight teeth
/// included).
fn paint_cog(painter: &egui::Painter, at: Pos2, r: f32) {
    let body = r * 0.72;
    for k in 0..8 {
        let a = k as f32 * std::f32::consts::FRAC_PI_4;
        let dir = vec2(a.cos(), a.sin());
        let side = vec2(-dir.y, dir.x) * (r * 0.18).max(0.8);
        painter.add(Shape::convex_polygon(
            vec![
                at + dir * body * 0.8 - side,
                at + dir * r - side,
                at + dir * r + side,
                at + dir * body * 0.8 + side,
            ],
            COG,
            Stroke::NONE,
        ));
    }
    painter.circle(at, body, COG, Stroke::new(1.0, COG_EDGE));
    painter.circle_filled(at, (body * 0.35).max(0.8), COG_EDGE);
}

/// The angles (screen, clockwise from the right) a speaker's shading spans:
/// into the room.
fn sweep(c: usize) -> (f32, f32) {
    use std::f32::consts::{FRAC_PI_2, PI};
    match c {
        L => (0.0, FRAC_PI_2),
        C => (0.0, PI),
        R => (FRAC_PI_2, PI),
        LS => (1.5 * PI, 2.0 * PI),
        _ => (PI, 1.5 * PI),
    }
}

fn arc(center: Pos2, r: f32, from: f32, to: f32) -> Vec<Pos2> {
    let n = ((to - from) * r / 3.0).ceil().clamp(6.0, 96.0) as usize;
    (0..=n)
        .map(|i| {
            let a = from + (to - from) * i as f32 / n as f32;
            center + vec2(a.cos(), a.sin()) * r
        })
        .collect()
}

fn pie(center: Pos2, r: f32, from: f32, to: f32, fill: Color32) -> Shape {
    let mut pts = vec![center];
    pts.extend(arc(center, r, from, to));
    Shape::convex_polygon(pts, fill, Stroke::NONE)
}

/// A speaker's icon: a box and its cone, turned toward the room's middle.
fn paint_speaker(painter: &egui::Painter, room: &Room, c: usize, color: Color32, small: bool) {
    let at = room.icon(c);
    let [x, y] = room.speaker(c);
    // Toward the middle (screen: y down).
    let dir = if c == C {
        vec2(0.0, 1.0)
    } else {
        vec2(-x, y).normalized()
    };
    let side = vec2(-dir.y, dir.x);
    let s = if small {
        (room.field.width() * 0.035).clamp(1.5, 3.0)
    } else {
        (room.field.width() * 0.028).clamp(3.0, 6.5)
    };
    if small {
        painter.circle_filled(at, s, color);
        return;
    }
    // The box behind, the cone opening toward the room.
    let back = at - dir * s * 0.9;
    let body = vec![
        back - side * s * 0.55,
        back + side * s * 0.55,
        at + side * s * 0.55,
        at - side * s * 0.55,
    ];
    painter.add(Shape::convex_polygon(body, color, Stroke::NONE));
    let cone = vec![
        at - side * s * 0.55,
        at + side * s * 0.55,
        at + dir * s * 1.1 + side * s * 1.3,
        at + dir * s * 1.1 - side * s * 1.3,
    ];
    painter.add(Shape::convex_polygon(cone, color, Stroke::NONE));
}

impl BananaApp {
    /// Whose panner the Surround Panner shows: picking another track shows
    /// that one's; `None` when it is not an audio track, a bus or a chain.
    pub(crate) fn surround_target(&mut self) -> Option<SurroundOwner> {
        if self.selected_track != self.surround_view.seen_track {
            self.surround_view.seen_track = self.selected_track;
            if let Some(t) = self.selected_track {
                self.surround_view.target = Some(SurroundOwner::Track(t));
            }
        }
        let p = self.project();
        self.surround_view.target.filter(|o| match *o {
            SurroundOwner::Track(t) => p.track(t).is_some_and(|t| t.kind == TrackKind::Audio),
            SurroundOwner::Strip(s) => p.strip_channel(s).is_some(),
        })
    }

    /// Opens the Surround Panner on `owner`'s panner.
    pub(crate) fn open_surround_panner(&mut self, owner: SurroundOwner) {
        self.surround_view.target = Some(owner);
        self.surround_view.seen_track = self.selected_track;
        self.show_tab(crate::dock::Tab::SurroundPanner, false);
    }
}

impl BananaApp {
    /// Insert › Audio Envelopes › Surround Pan Keyframes' track: the
    /// current audio track, when it pans in the room.
    pub(crate) fn surround_keys_track(&self) -> Option<TrackId> {
        let t = self.current_track(TrackKind::Audio)?;
        self.project()
            .surround_pans(SurroundOwner::Track(t))
            .then_some(t)
    }

    /// Whether that track's keyframes row is shown.
    pub(crate) fn surround_keys_shown(&self) -> bool {
        self.surround_keys_track()
            .and_then(|t| self.project().surround(SurroundOwner::Track(t)))
            .is_some_and(|s| s.keyframes)
    }

    /// Inserts or removes that track's keyframes row.
    pub(crate) fn toggle_surround_keyframes(&mut self) {
        if let Some(t) = self.surround_keys_track() {
            toggle_keyframes(self, SurroundOwner::Track(t));
        }
    }
}

impl BananaApp {
    /// The demo in 5.1: its audio track panned in the room — three Surround
    /// Pan Keyframes, Smoothness, the center 3 dB down, its row shown —, a
    /// second audio track through a bus panned toward the back left (its
    /// keyframes row too), the bus tracks shown and the Surround Panner
    /// open on the first track, the cursor between two keyframes (screen
    /// tests).
    pub fn surround_for_review(&mut self) -> bool {
        use bv_timeline::Output;
        let Some(audio) = self
            .project()
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Audio)
            .map(|t| t.id)
        else {
            return false;
        };
        let s = Tick::from_seconds;
        let made = self.modify(|p| {
            p.settings.channels = bv_core::ChannelLayout::Surround51;
            let sp = p.surround_mut(SurroundOwner::Track(audio))?;
            sp.insert_keyframes();
            sp.set_key(s(0), [-0.6, 0.6]);
            sp.set_key(s(3), [0.55, 0.25]);
            sp.set_key(s(6), [0.15, -0.7]);
            sp.smoothness = 0.5;
            sp.center_db = -3.0;
            let second = p.add_track(TrackKind::Audio);
            let bus = p.add_bus()?;
            p.set_track_output(second, Output::Bus(bus));
            let bp = p.surround_mut(SurroundOwner::Strip(Strip::Bus(bus)))?;
            bp.x = -0.7;
            bp.y = -0.5;
            bp.insert_keyframes();
            bp.set_key(s(5), [0.3, -0.2]);
            p.master.channel_db = [0.0, 0.0, -3.0, -6.0, -1.5, -1.5];
            Some(())
        });
        self.meters_for_review(vec![0.5, 0.45, 0.3, 0.1, 0.25, 0.2]);
        self.timeline.show_surround_keys = true;
        self.timeline.show_bus_tracks = true;
        self.selected_track = Some(audio);
        self.seek(Tick::from_seconds_f64(1.5));
        self.open_surround_panner(SurroundOwner::Track(audio));
        made.is_some()
    }
}

/// Inserts `owner`'s keyframes row (a keyframe at the start holding the
/// panning) and shows the rows, or removes it (the panning at its start
/// kept as the trim).
pub(crate) fn toggle_keyframes(app: &mut BananaApp, owner: SurroundOwner) {
    let on = app.project().surround(owner).is_some_and(|s| s.keyframes);
    app.modify(|p| {
        if let Some(s) = p.surround_mut(owner) {
            if on {
                s.remove_keyframes();
            } else {
                s.insert_keyframes();
            }
        }
    });
    if !on {
        app.timeline.show_surround_keys = true;
    }
}

/// Whether moving `owner`'s point sets keyframes: its automation controls
/// (its automation not off); a bus's or a chain's keyframes row too.
fn keyframes_edited(project: &Project, owner: SurroundOwner) -> bool {
    match owner {
        SurroundOwner::Track(t) => project
            .track(t)
            .is_some_and(|t| t.automation.controls && t.automation.mode != AutomationMode::Off),
        SurroundOwner::Strip(s) => project.strip_channel(s).is_some_and(|c| {
            c.automation.mode != AutomationMode::Off
                && (c.automation.controls || project.surround(owner).is_some_and(|s| s.keyframes))
        }),
    }
}

/// Whether `owner`'s keyframes play (its automation off: not).
fn keyframes_play(project: &Project, owner: SurroundOwner) -> bool {
    match owner {
        SurroundOwner::Track(t) => project
            .track(t)
            .is_some_and(|t| t.automation.mode != AutomationMode::Off),
        SurroundOwner::Strip(s) => project
            .strip_channel(s)
            .is_some_and(|c| c.automation.mode != AutomationMode::Off),
    }
}

/// `owner`'s pan point at `t`, as it plays.
pub(crate) fn point_now(project: &Project, owner: SurroundOwner, t: Tick) -> [f32; 2] {
    match project.surround(owner) {
        None => [0.0, 0.0],
        Some(sp) if keyframes_play(project, owner) => sp.point_at(t),
        Some(sp) => sp.trim_point(),
    }
}

/// A Surround Panner's point recorded while playing back, as automation
/// is recorded (VEGAS's Help, "Recording track envelope and keyframe
/// automation"): in Touch while it is held, in Latch until playback
/// stops — whose, the point, the drag's undo, held or not, and the last
/// time a keyframe went to.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SurroundWrite {
    owner: SurroundOwner,
    point: [f32; 2],
    key: egui::Id,
    held: bool,
    last: Tick,
}

/// `owner`'s point let go (a drag ended, a key or the wheel done): in
/// Touch its recording stops, in Latch it goes on.
pub(crate) fn release_surround(app: &mut BananaApp, owner: SurroundOwner) {
    if let Some(w) = &mut app.surround_write
        && w.owner == owner
    {
        w.held = false;
    }
}

impl BananaApp {
    /// Each frame while playing back: the Surround Panner's point being
    /// recorded writes on — held in Touch, until playback stops in Latch
    /// — a keyframe at most every tenth of a second, those it passes over
    /// replaced.
    pub(crate) fn follow_surround_write(&mut self) {
        let Some(w) = self.surround_write else {
            return;
        };
        let p = self.project();
        let mode = match w.owner {
            SurroundOwner::Track(t) => p.track(t).map(|t| t.automation.mode),
            SurroundOwner::Strip(s) => p.strip_channel(s).map(|c| c.automation.mode),
        };
        let at = self.playhead.filter(|_| self.is_playing());
        let (Some(mode), Some(at)) = (mode, at) else {
            self.surround_write = None;
            return;
        };
        if !mode.writes() || !(w.held || mode == AutomationMode::Latch) {
            self.surround_write = None;
            return;
        }
        if at >= w.last && at - w.last < Tick::from_seconds_f64(0.1) {
            return;
        }
        set_point(self, w.owner, w.point, w.key);
        if let Some(n) = &mut self.surround_write {
            n.held = w.held;
        }
    }
}

/// The time a keyframe set now goes to: the cursor, or the playback
/// position while a track's Touch or Latch records (then true).
fn key_time(app: &BananaApp, owner: SurroundOwner) -> (Tick, bool) {
    let writes = match owner {
        SurroundOwner::Track(t) => app
            .project()
            .track(t)
            .is_some_and(|t| t.automation.mode.writes()),
        SurroundOwner::Strip(s) => app
            .project()
            .strip_channel(s)
            .is_some_and(|c| c.automation.mode.writes()),
    };
    match app.playhead.filter(|_| writes && app.is_playing()) {
        Some(at) => (at, true),
        None => (app.cursor, false),
    }
}

/// Moves `owner`'s pan point to `point` (`key`: one undo for a drag): the
/// keyframe at the cursor when it automates (the row inserted when it had
/// none), else the trim (what its keyframes leave there, taken off).
pub(crate) fn set_point(app: &mut BananaApp, owner: SurroundOwner, point: [f32; 2], key: egui::Id) {
    let point = [point[0].clamp(-1.0, 1.0), point[1].clamp(-1.0, 1.0)];
    let keyed = keyframes_edited(app.project(), owner);
    let plays = keyframes_play(app.project(), owner);
    let (at, writing) = key_time(app, owner);
    let shown = app.display_time();
    // Recording: the keyframes between the last one written and now go.
    let since = app
        .surround_write
        .filter(|w| writing && w.owner == owner)
        .map(|w| w.last);
    app.modify_merged(key, |p| {
        let Some(sp) = p.surround_mut(owner) else {
            return;
        };
        if keyed {
            if sp.keys.is_empty() {
                sp.insert_keyframes();
            }
            if let Some(since) = since {
                sp.keys.retain(|k| k.at <= since || k.at >= at);
            }
            sp.set_point_at(at, point);
        } else {
            let k = if plays {
                sp.keyed_at(shown)
            } else {
                [0.0, 0.0]
            };
            sp.x = (point[0] - k[0]).clamp(-2.0, 2.0);
            sp.y = (point[1] - k[1]).clamp(-2.0, 2.0);
        }
    });
    if keyed && writing {
        app.surround_write = Some(SurroundWrite {
            owner,
            point,
            key,
            held: true,
            last: at,
        });
    }
}

/// Changes `owner`'s panner with `f` (made when it had none).
fn change(app: &mut BananaApp, owner: SurroundOwner, f: impl FnOnce(&mut SurroundPan)) {
    app.modify(|p| {
        if let Some(sp) = p.surround_mut(owner) {
            f(sp);
        }
    });
}

/// The Surround Panner window.
pub fn ui(ui: &mut egui::Ui, app: &mut BananaApp) {
    let area = ui.available_rect_before_wrap();
    ui.allocate_rect(area, Sense::hover());
    let painter = ui.painter_at(area);
    painter.rect_filled(area, 0.0, theme::panel_dark());
    let owner = app.surround_target();
    let pans = owner.is_some_and(|o| app.project().surround_pans(o));
    // The bar: the badge, the move mode, LFE.
    let bar = Rect::from_min_size(area.min, vec2(area.width(), 30.0));
    painter.rect_filled(bar, 0.0, theme::panel());
    let sp = owner
        .and_then(|o| app.project().surround(o).cloned())
        .unwrap_or_default();
    let badge = Rect::from_min_size(bar.min + vec2(0.0, 5.0), vec2(34.0, 20.0));
    if let Some(o) = owner {
        paint_badge(&painter, app, o, badge);
    }
    let mode_rect =
        Rect::from_min_size(pos2(badge.right() + 6.0, bar.top() + 5.0), vec2(20.0, 20.0));
    let move_mode = app.surround_view.move_mode;
    let resp = ui
        .interact(mode_rect, ui.id().with("surround-move"), Sense::click())
        .on_hover_text(move_mode.label());
    if resp.hovered() {
        painter.rect_filled(mode_rect, 2.0, theme::hover());
    }
    paint_move_mode(&painter, mode_rect, move_mode, theme::text_strong());
    if resp.clicked() {
        app.surround_view.move_mode = move_mode.next();
    }
    let lfe_rect = Rect::from_min_size(
        pos2(mode_rect.right() + 6.0, bar.top() + 8.0),
        vec2(22.0, 14.0),
    );
    let resp = ui
        .interact(lfe_rect, ui.id().with("surround-lfe"), Sense::click())
        .on_hover_text(tr("LFE Only: everything into the low-frequency channel"));
    let lfe_on = sp.lfe_only;
    if lfe_on {
        painter.rect_filled(lfe_rect, 2.0, theme::banana_dim());
    } else if resp.hovered() {
        painter.rect_filled(lfe_rect, 2.0, theme::hover());
    }
    painter.rect_stroke(
        lfe_rect.shrink(2.0),
        1.0,
        Stroke::new(1.0, theme::text_strong()),
        egui::StrokeKind::Inside,
    );
    painter.text(
        lfe_rect.center(),
        Align2::CENTER_CENTER,
        "LFE",
        FontId::proportional(8.0),
        theme::text_strong(),
    );
    if resp.clicked()
        && let Some(o) = owner.filter(|_| pans)
    {
        change(app, o, |s| s.lfe_only = !lfe_on);
    }

    // Under the room: Center and, with two keyframes or more, Smoothness.
    let smooth = pans && sp.keys.len() >= 2;
    let rows = if smooth { 2.0 } else { 1.0 };
    let bottom_h = 8.0 + 22.0 * rows;
    let space = Rect::from_min_max(
        pos2(area.left() + 4.0, bar.bottom() + 2.0),
        pos2(area.right() - 4.0, area.bottom() - bottom_h),
    );
    let side = space.width().min(space.height()).max(60.0);
    let field = Rect::from_center_size(
        pos2(space.center().x, space.top() + side / 2.0),
        vec2(side, side),
    );
    let room = Room::new(field);
    let time = app.display_time();
    let id = ui.id().with("surround-room");
    let resp = ui.interact(field, id, Sense::click_and_drag());
    let shown = owner.filter(|_| pans).map(|o| {
        let p = app.project();
        let point = point_now(p, o, time);
        let path = if keyframes_play(p, o) {
            path_of(&sp)
        } else {
            Vec::new()
        };
        (point, path, p.surround_pan_mode(o))
    });
    let automation = owner.is_some_and(|o| keyframes_edited(app.project(), o));
    match &shown {
        Some((point, path, mode)) => {
            let s = Shown {
                sp: &sp,
                mode: *mode,
                point: *point,
                path: path.clone(),
                focused: resp.has_focus(),
            };
            paint_room(&painter, &room, Some(&s), false, automation);
        }
        None => {
            paint_room(&painter, &room, None, false, false);
            let hint = if owner.is_none() {
                tr("Select an audio track, a bus or an assignable FX chain to pan")
            } else if app.project().settings.channels != bv_core::ChannelLayout::Surround51 {
                tr("Surround panning is for 5.1 projects (File › Properties › Audio)")
            } else {
                tr("Only what plays into the Master bus is panned in the room")
            };
            crate::widgets::fit_text(
                &painter,
                field.center(),
                Align2::CENTER_CENTER,
                hint,
                FontId::proportional(11.0),
                theme::text_dim(),
                field.width() - 12.0,
            );
        }
    }
    if let (Some(o), Some((point, ..))) = (owner, shown.as_ref()) {
        room_input(ui, app, o, &room, &resp, *point, &sp, id);
    }

    // The Center's fader (not while the center is muted) and Smoothness.
    let mut y = field.bottom() + 8.0;
    if pans && let Some(o) = owner {
        if !sp.mute[C] {
            let row = Rect::from_min_size(pos2(field.left(), y), vec2(field.width(), 18.0));
            let g = bv_mixer_gain(sp.center_db);
            let pos = (g / MAX_GAIN).max(0.0).sqrt();
            let unity = (1.0 / MAX_GAIN).sqrt();
            let text = if sp.center_db <= -96.0 {
                "−∞ dB".to_string()
            } else {
                format!("{} dB", crate::fmt::num(sp.center_db as f64, 1))
            };
            if let Some(v) = labeled_slider(
                ui,
                &painter,
                row,
                tr("Center:"),
                &text,
                pos,
                unity,
                ui.id().with("surround-center"),
            ) {
                let g = v * v * MAX_GAIN;
                let db = if g <= 1e-5 { -96.0 } else { 20.0 * g.log10() };
                app.modify_merged(ui.id().with("surround-center"), |p| {
                    if let Some(s) = p.surround_mut(o) {
                        s.center_db = db;
                    }
                });
            }
            y += 22.0;
        }
        if smooth {
            let row = Rect::from_min_size(pos2(field.left(), y), vec2(field.width(), 18.0));
            let text = format!(
                "{} %",
                crate::fmt::num((sp.smoothness * 100.0).round() as f64, 0)
            );
            if let Some(v) = labeled_slider(
                ui,
                &painter,
                row,
                tr("Smoothness:"),
                &text,
                sp.smoothness,
                0.0,
                ui.id().with("surround-smooth"),
            ) {
                app.modify_merged(ui.id().with("surround-smooth"), |p| {
                    if let Some(s) = p.surround_mut(o) {
                        s.smoothness = v.clamp(0.0, 1.0);
                    }
                });
            }
        }
    }
}

/// The faders' top: +12 dB, as the track headers'.
const MAX_GAIN: f32 = 3.981_072;

fn bv_mixer_gain(db: f32) -> f32 {
    if db <= -96.0 {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}

/// The keyframes' path (the point their curves and Smoothness give, the
/// trim added) at even steps of time — a tenth of a second, at most 400
/// steps —, with two keyframes or more.
fn path_of(sp: &SurroundPan) -> Vec<[f32; 2]> {
    if sp.keys.len() < 2 {
        return Vec::new();
    }
    let (a, b) = (sp.keys[0].at, sp.keys[sp.keys.len() - 1].at);
    let span = (b - a).0.max(1);
    let n = ((span as f64 / Tick::from_seconds_f64(0.1).0 as f64).ceil() as usize).clamp(8, 400);
    (0..=n)
        .map(|i| sp.point_at(Tick(a.0 + (span as f64 * i as f64 / n as f64) as i64)))
        .collect()
}

/// The owner's badge: a track's number on its color and a speaker, a
/// bus's letter, a chain's "FX".
fn paint_badge(painter: &egui::Painter, app: &BananaApp, owner: SurroundOwner, r: Rect) {
    let p = app.project();
    let (fill, text) = match owner {
        SurroundOwner::Track(t) => {
            let index = p.tracks.iter().position(|x| x.id == t).unwrap_or(0);
            let color = p
                .track(t)
                .map_or(theme::track_audio(), |t| app.settings.prefs.color_of(t));
            (color, (index + 1).to_string())
        }
        SurroundOwner::Strip(Strip::Bus(b)) => {
            let i = p.busses.iter().position(|x| x.id == b).unwrap_or(0);
            (
                theme::banana_dim(),
                bv_timeline::busses::bus_letter(i).to_string(),
            )
        }
        SurroundOwner::Strip(Strip::Chain(_)) => (theme::banana_dim(), "FX".to_string()),
    };
    painter.rect_filled(r, 0.0, fill);
    painter.text(
        r.left_top() + vec2(3.0, 1.0),
        Align2::LEFT_TOP,
        text,
        FontId::proportional(12.0),
        Color32::WHITE,
    );
    crate::icons::paint(
        painter,
        Rect::from_min_size(r.left_top() + vec2(13.0, 7.0), vec2(12.0, 12.0)),
        crate::icons::Icon::Speaker,
        Color32::WHITE,
    );
}

/// The move mode's button: four arrows, or two.
fn paint_move_mode(painter: &egui::Painter, r: Rect, mode: MoveMode, color: Color32) {
    let c = r.center();
    let s = 6.5;
    let stroke = Stroke::new(1.5, color);
    let arrow = |dir: egui::Vec2| {
        let tip = c + dir * s;
        let side = vec2(-dir.y, dir.x);
        painter.line_segment([c, tip], stroke);
        painter.add(Shape::convex_polygon(
            vec![
                tip + dir * 1.5,
                tip - dir * 2.0 + side * 2.5,
                tip - dir * 2.0 - side * 2.5,
            ],
            color,
            Stroke::NONE,
        ));
    };
    if mode != MoveMode::FrontBack {
        arrow(vec2(1.0, 0.0));
        arrow(vec2(-1.0, 0.0));
    }
    if mode != MoveMode::LeftRight {
        arrow(vec2(0.0, 1.0));
        arrow(vec2(0.0, -1.0));
    }
}

/// A row of the window's bottom: its label in a box, a slider, its value
/// in a box (Ctrl: fine; double-click: `default`). The new position.
#[allow(clippy::too_many_arguments)]
fn labeled_slider(
    ui: &egui::Ui,
    painter: &egui::Painter,
    row: Rect,
    label: &str,
    value: &str,
    pos: f32,
    default: f32,
    id: egui::Id,
) -> Option<f32> {
    let font = FontId::proportional(11.0);
    let label_w = row.width() * 0.3;
    let value_w = row.width() * 0.22;
    let lbox = Rect::from_min_size(row.min, vec2(label_w, row.height()));
    let vbox = Rect::from_min_max(pos2(row.right() - value_w, row.top()), row.max);
    for b in [lbox, vbox] {
        painter.rect_filled(b, 0.0, theme::panel());
        painter.line_segment(
            [b.left_top(), b.right_top()],
            Stroke::new(1.0, Color32::from_gray(130)),
        );
    }
    crate::widgets::fit_text(
        painter,
        pos2(lbox.left() + 3.0, lbox.center().y),
        Align2::LEFT_CENTER,
        label,
        font.clone(),
        theme::text(),
        lbox.width() - 4.0,
    );
    painter.text(
        pos2(vbox.right() - 3.0, vbox.center().y),
        Align2::RIGHT_CENTER,
        value,
        font,
        theme::text(),
    );
    let track = Rect::from_min_max(
        pos2(lbox.right() + 10.0, row.top()),
        pos2(vbox.left() - 10.0, row.bottom()),
    );
    let resp = ui.interact(track, id, Sense::click_and_drag());
    let y = track.center().y;
    painter.line_segment(
        [pos2(track.left(), y), pos2(track.right(), y)],
        Stroke::new(1.0, Color32::from_gray(130)),
    );
    let x = track.left() + track.width() * pos.clamp(0.0, 1.0);
    crate::widgets::paint_knob(
        painter,
        Rect::from_center_size(pos2(x, y), vec2(9.0, 13.0)),
        resp.hovered() || resp.dragged(),
    );
    if resp.double_clicked() {
        return Some(default);
    }
    if resp.dragged() {
        let fine = if ui.input(|i| i.modifiers.ctrl) {
            0.1
        } else {
            1.0
        };
        let d = resp.drag_delta().x / track.width().max(1.0) * fine;
        if d != 0.0 {
            return Some((pos + d).clamp(0.0, 1.0));
        }
    }
    None
}

/// The room's mouse, keys, wheel and joystick (`point`: the pan point now).
#[allow(clippy::too_many_arguments)]
fn room_input(
    ui: &egui::Ui,
    app: &mut BananaApp,
    owner: SurroundOwner,
    room: &Room,
    resp: &egui::Response,
    point: [f32; 2],
    sp: &SurroundPan,
    key: egui::Id,
) {
    let modifiers = ui.input(|i| i.modifiers);
    let at = room.at(point);
    let near_point = |p: Pos2| p.distance(at) <= 10.0;
    let speaker_at = |p: Pos2| {
        SPEAKERS
            .into_iter()
            .find(|&c| p.distance(room.icon(c)) <= 9.0)
    };
    let pointer = resp.interact_pointer_pos();
    // A speaker: a click mutes or unmutes it, with Ctrl solos it.
    if resp.clicked()
        && let Some(c) = pointer.and_then(speaker_at)
    {
        if modifiers.ctrl {
            change(app, owner, |s| s.solo[c] = !s.solo[c]);
        } else {
            change(app, owner, |s| s.mute[c] = !s.mute[c]);
        }
        return;
    }
    if resp.drag_started() {
        app.surround_view.drag = pointer.filter(|p| near_point(*p)).map(|p| (point, p));
        if app.surround_view.drag.is_some() {
            resp.request_focus();
        }
    }
    if resp.clicked() && pointer.is_some_and(near_point) {
        resp.request_focus();
    }
    let mode = app.surround_view.move_mode;
    let mut new = None;
    if resp.dragged()
        && let (Some((from, p0)), Some(p)) = (app.surround_view.drag, pointer)
    {
        let fine = if modifiers.ctrl { 0.1 } else { 1.0 };
        let d = (p - p0) * fine;
        let mut q = [
            from[0] + d.x / (room.room.width() / 2.0),
            from[1] - d.y / (room.room.height() / 2.0),
        ];
        if mode == MoveMode::Freely {
            q = constrained(q, from, modifiers);
        }
        new = Some(q);
    }
    if resp.drag_stopped() {
        app.surround_view.drag = None;
        release_surround(app, owner);
    }
    // Double-click: on the point, back to the middle; elsewhere, there.
    if resp.double_clicked()
        && let Some(p) = pointer.filter(|p| speaker_at(*p).is_none())
    {
        new = Some(if near_point(p) {
            [0.0, 0.0]
        } else {
            room.point(p)
        });
    }
    // The wheel: forward and back; Shift: left and right; Alt: around the
    // middle at its distance (Alt+Shift: on the largest circle).
    if resp.hovered() {
        let (notches, _) = ui.input(|i| crate::widgets::wheel_notches(&i.events));
        app.surround_view.wheel += notches;
        let n = app.surround_view.wheel.trunc();
        app.surround_view.wheel -= n;
        if n != 0.0 {
            let step = if modifiers.ctrl { STEP * 0.1 } else { STEP };
            new = Some(if modifiers.alt {
                let r = if modifiers.shift {
                    1.0
                } else {
                    (point[0].hypot(point[1])).max(1e-3)
                };
                let a = point[1].atan2(point[0]) - n * 5f32.to_radians() * step / STEP;
                [r * a.cos(), r * a.sin()]
            } else if modifiers.shift {
                [point[0] + n * step, point[1]]
            } else {
                [point[0], point[1] + n * step]
            });
        }
    }
    // The keys, while the point is picked.
    if resp.has_focus() {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                resp.id,
                egui::EventFilter {
                    tab: false,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: false,
                },
            )
        });
        if let Some(q) = keys_move(ui, point, modifiers) {
            new = Some(q);
        }
    }
    // The joystick under the pointer.
    if new.is_none()
        && let Some(v) = crate::widgets::joystick_push(ui, resp)
    {
        new = Some([point[0] + v.x, point[1] + v.y]);
    }
    if let Some(mut q) = new {
        match mode {
            MoveMode::LeftRight => q[1] = point[1],
            MoveMode::FrontBack => q[0] = point[0],
            MoveMode::Freely => {}
        }
        if q != point {
            set_point(app, owner, q, key);
            // Keys, the wheel and the joystick are not held.
            if !resp.dragged() {
                release_surround(app, owner);
            }
        }
    }
    // Its menu: the pan type (a track's) and LFE Only.
    let lfe = sp.lfe_only;
    resp.context_menu(|ui| {
        if let SurroundOwner::Track(t) = owner {
            crate::menus::sub(ui, tr("Pan Type"), true, |ui| {
                crate::timeline::pan_type_items(ui, app, t);
            });
            ui.separator();
        }
        if crate::widgets::radio(ui, !lfe, tr("Surround Pan")).clicked() {
            change(app, owner, |s| s.lfe_only = false);
            ui.close();
        }
        if crate::widgets::radio(ui, lfe, tr("LFE Only")).clicked() {
            change(app, owner, |s| s.lfe_only = true);
            ui.close();
        }
    });
}

/// A drag's constraints with Move Freely: Shift on the line through the
/// middle and where the drag started, Alt at the distance it started at,
/// Alt+Shift on the largest circle.
fn constrained(q: [f32; 2], from: [f32; 2], m: egui::Modifiers) -> [f32; 2] {
    let len = q[0].hypot(q[1]);
    if m.alt {
        let r = if m.shift { 1.0 } else { from[0].hypot(from[1]) };
        if len < 1e-6 {
            return q;
        }
        return [q[0] / len * r, q[1] / len * r];
    }
    if m.shift {
        let fl = from[0].hypot(from[1]);
        if fl < 1e-6 {
            return q;
        }
        let (ux, uy) = (from[0] / fl, from[1] / fl);
        let along = q[0] * ux + q[1] * uy;
        return [ux * along, uy * along];
    }
    q
}

/// The keys that move a picked point: the arrows and Page Up and Down
/// (with Shift left and right), Ctrl in fine steps; the numeric keypad
/// puts it at a corner, an edge or the middle (with Ctrl, 1, 3, 7 and 9 at
/// the largest circle's corners).
fn keys_move(ui: &egui::Ui, point: [f32; 2], m: egui::Modifiers) -> Option<[f32; 2]> {
    let step = if m.ctrl { STEP * 0.1 } else { STEP };
    let page = step * 4.0;
    let pressed = |k: Key| ui.input(|i| i.key_pressed(k));
    let [x, y] = point;
    if pressed(Key::ArrowUp) {
        return Some([x, y + step]);
    }
    if pressed(Key::ArrowDown) {
        return Some([x, y - step]);
    }
    if pressed(Key::ArrowLeft) {
        return Some([x - step, y]);
    }
    if pressed(Key::ArrowRight) {
        return Some([x + step, y]);
    }
    if pressed(Key::PageUp) {
        return Some(if m.shift {
            [x + page, y]
        } else {
            [x, y + page]
        });
    }
    if pressed(Key::PageDown) {
        return Some(if m.shift {
            [x - page, y]
        } else {
            [x, y - page]
        });
    }
    let pads = [
        (Key::Num1, '1', [-1.0, -1.0]),
        (Key::Num2, '2', [0.0, -1.0]),
        (Key::Num3, '3', [1.0, -1.0]),
        (Key::Num4, '4', [-1.0, 0.0]),
        (Key::Num5, '5', [0.0, 0.0]),
        (Key::Num6, '6', [1.0, 0.0]),
        (Key::Num7, '7', [-1.0, 1.0]),
        (Key::Num8, '8', [0.0, 1.0]),
        (Key::Num9, '9', [1.0, 1.0]),
    ];
    for (k, c, [px, py]) in pads {
        if pressed(k) && bv_platform::numpad_down(c) {
            let corner = px != 0.0 && py != 0.0;
            let s = if m.ctrl && corner {
                std::f32::consts::FRAC_1_SQRT_2
            } else {
                1.0
            };
            return Some([px * s, py * s]);
        }
    }
    None
}

/// A small panner (a track header's, a bus track's, a Mixing Console
/// strip's) in `rect`: the room, its speakers' marks and the point —
/// yellow-edged as an automation control; dragged anywhere it moves the
/// point (Ctrl: fine), a double-click opens the Surround Panner on it, the
/// joystick pushes it, its menu sets the pan type and LFE Only.
pub(crate) fn mini_panner(
    ui: &egui::Ui,
    app: &mut BananaApp,
    rect: Rect,
    owner: SurroundOwner,
    id: egui::Id,
    tip: &str,
) -> egui::Response {
    let resp = ui.interact(rect, id, Sense::click_and_drag());
    let room = Room::new(rect);
    let time = app.display_time();
    let p = app.project();
    let sp = p.surround(owner).cloned().unwrap_or_default();
    let point = point_now(p, owner, time);
    let mode = p.surround_pan_mode(owner);
    let automation = keyframes_edited(p, owner);
    let shown = Shown {
        sp: &sp,
        mode,
        point,
        path: Vec::new(),
        focused: false,
    };
    paint_room(ui.painter(), &room, Some(&shown), true, automation);
    if resp.hovered() {
        ui.painter().rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0, theme::text_dim()),
            egui::StrokeKind::Outside,
        );
    }
    let mut new = None;
    if resp.dragged() {
        let fine = if ui.input(|i| i.modifiers.ctrl) {
            0.1
        } else {
            1.0
        };
        let d = resp.drag_delta() * fine;
        if d != egui::Vec2::ZERO {
            new = Some([
                point[0] + d.x / (room.room.width() / 2.0),
                point[1] - d.y / (room.room.height() / 2.0),
            ]);
        }
    }
    if new.is_none()
        && let Some(v) = crate::widgets::joystick_push(ui, &resp)
    {
        new = Some([point[0] + v.x, point[1] + v.y]);
    }
    if let Some(q) = new {
        set_point(app, owner, q, id);
        if !resp.dragged() {
            release_surround(app, owner);
        }
    }
    if resp.drag_stopped() {
        release_surround(app, owner);
    }
    if resp.double_clicked() {
        app.open_surround_panner(owner);
    }
    let lfe = sp.lfe_only;
    resp.context_menu(|ui| {
        if let SurroundOwner::Track(t) = owner {
            crate::menus::sub(ui, tr("Pan Type"), true, |ui| {
                crate::timeline::pan_type_items(ui, app, t);
            });
            ui.separator();
        }
        if crate::widgets::radio(ui, !lfe, tr("Surround Pan")).clicked() {
            change(app, owner, |s| s.lfe_only = false);
            ui.close();
        }
        if crate::widgets::radio(ui, lfe, tr("LFE Only")).clicked() {
            change(app, owner, |s| s.lfe_only = true);
            ui.close();
        }
    });
    resp.on_hover_text(tip)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The grid where Add Channels' left-to-right ratio is 6 dB lies a
    /// third of the way out (as the Help's picture draws it); the ratios'
    /// zero lines move back with the Center's trim down.
    #[test]
    fn the_grid_follows_the_pan_type_and_the_center() {
        let sp = SurroundPan::default();
        let (xs, ys) = grid_lines(&sp, PanMode::AddChannels);
        assert_eq!(xs.len(), 3);
        assert!((xs[0] + 1.0 / 3.0).abs() < 1e-3, "{xs:?}");
        assert!(xs[1].abs() < 1e-3 && (xs[2] - 1.0 / 3.0).abs() < 1e-3);
        assert_eq!(ys.len(), 3);
        assert!(ys[0] > ys[1] && ys[1] > ys[2], "{ys:?}");
        let quiet = SurroundPan {
            center_db: -12.0,
            ..SurroundPan::default()
        };
        let (_, low) = grid_lines(&quiet, PanMode::AddChannels);
        assert!(low[1] > ys[1], "{low:?} {ys:?}");
    }

    /// The room maps the point both ways; Shift keeps the line through the
    /// middle, Alt the distance, Alt+Shift the largest circle.
    #[test]
    fn the_room_and_the_drag_constraints() {
        let room = Room::new(Rect::from_min_size(pos2(10.0, 20.0), vec2(200.0, 200.0)));
        let p = room.at([0.5, -0.25]);
        let q = room.point(p);
        assert!((q[0] - 0.5).abs() < 1e-5 && (q[1] + 0.25).abs() < 1e-5);
        assert!(room.at([0.0, 1.0]).y < room.at([0.0, -1.0]).y);
        let shift = egui::Modifiers::SHIFT;
        let c = constrained([0.9, 0.1], [0.5, 0.5], shift);
        assert!((c[0] - c[1]).abs() < 1e-5);
        let alt = egui::Modifiers::ALT;
        let c = constrained([0.9, 0.0], [0.3, 0.4], alt);
        assert!((c[0].hypot(c[1]) - 0.5).abs() < 1e-5);
        let c = constrained([0.2, 0.0], [0.3, 0.4], alt | shift);
        assert!((c[0] - 1.0).abs() < 1e-5);
    }
}
