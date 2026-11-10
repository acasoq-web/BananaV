//! BananaV's own icons, drawn as vector paths (sharp at any scale).
//!
//! They share one drawing system, so that they read as a set:
//!
//! - A grid of 24 units. Paths stay inside 3–21; strokes are 2 units wide
//!   with round ends and joins, so they reach 2–22 (`glyphs.rs` has the
//!   paths, in SVG's path syntax).
//! - A body (a page, a folder, a speaker) is its outline over a soft tint
//!   of the same color; what acts (play, record, the plus of a "new") is
//!   solid with rounded corners; what is only a thing, a plain outline.
//! - Corners of containers are 2.5 units round, small parts 1.
//! - Only the caller's color is used (so hover, selected and disabled
//!   states work on every icon), except for the red of Record and the few
//!   marks that stand for something gone or live.

mod glyphs;
mod logo;
mod path;

pub use self::logo::paint as paint_logo;

use std::f32::consts::{FRAC_PI_2, PI};
use std::sync::OnceLock;

use crate::egui::{Color32, Mesh, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

use self::path::Sub;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    New,
    Open,
    Save,
    Render,
    Settings,
    Cut,
    Copy,
    Paste,
    Undo,
    Redo,
    Play,
    Pause,
    Stop,
    Record,
    Loop,
    PlayFromStart,
    GoStart,
    GoEnd,
    PrevFrame,
    NextFrame,
    EditTool,
    EnvelopeTool,
    SelectTool,
    ZoomTool,
    Delete,
    TrimStart,
    TrimEnd,
    Split,
    Lock,
    Marker,
    Region,
    Snap,
    Ripple,
    Crossfade,
    Grid,
    Camera,
    /// A camera with a down arrow, to save a capture (not used yet).
    #[allow(dead_code)]
    CameraSave,
    Folder,
    File,
    FileAudio,
    FileImage,
    FileVideo,
    Up,
    Back,
    Forward,
    Refresh,
    Plus,
    Fx,
    Speaker,
    Group,
    PanCrop,
    More,
    // Timeline toolbar buttons (Options › Customize Timeline Toolbar).
    Trim,
    SmartSplit,
    LockEnvelopes,
    IgnoreGroups,
    ColorGrading,
    ParentTimeline,
    NestedTimeline,
    NewNestedTimeline,
    NewAudioTrack,
    NewVideoTrack,
    AdjustmentTrack,
    VolumeEnvelope,
    PanEnvelope,
    Bus,
    InputBus,
    AssignableFx,
    BypassFx,
    QuickFade,
    Text,
    MuteAudio,
    MuteVideo,
    EdgeFrames,
    MediaMarkers,
    SnapGrid,
    SnapMarkers,
    SnapEvents,
    Quantize,
    SlipTakes,
    MotionTracking,
    DetectScenes,
    // Main toolbar buttons (Options › Customize Toolbar).
    SaveAs,
    SaveIncremental,
    Midi,
    Metronome,
    Disc,
    Import,
    Dock,
    Slideshow,
    Speech,
    Sparkle,
    AudioSync,
    Script,
    /// File › Properties › Video: Match Media Video Settings.
    MatchMedia,
    /// The color picker's tool that takes a color from the screen.
    Eyedropper,
    /// A picker that waits for a click on the preview: a sight.
    Crosshair,
    /// Show Limits on Scopes: two levels and an arrow between them.
    Limits,
    /// Video Preview on External Monitor: a display on its stand.
    ExternalMonitor,
    /// A plug-in marked as a favorite, and one that is not.
    StarFilled,
    Star,
    /// The windows' Views menu.
    Views,
    /// A media generator, a transition (the plug-in windows' lists).
    Generator,
    Transition,
    /// Plays media when it is clicked (Project Media, Explorer).
    AutoPreview,
    /// Project Media: Remove All Unused Media, Capture Video, Swap Media.
    RemoveUnused,
    Capture,
    Swap,
    /// A strip of film (the media bins of All Media and Media By Type).
    FilmStrip,
    /// Video Preview: split screen, 360° and HDR views; the status line's
    /// project, preview, frame and display.
    SplitScreen,
    View360,
    Hdr,
    PreviewSize,
    FrameNumber,
    DisplaySize,
    /// Master Bus: downmix, dim, the Mixing Console, the bus itself and
    /// its routing.
    Downmix,
    Dim,
    MixingConsole,
    BusSelect,
    Routing,
    /// The welcome screen: Getting Started, Language, its channels and the
    /// shapes of a new project.
    Mortarboard,
    Globe,
    /// Source code (BananaV's GitHub).
    Code,
    People,
    HeartBubble,
    ShapeWide,
    ShapeFull,
    ShapePortrait,
    ShapeSquare,
    ShapeScope,
    /// Video Scopes: Update Scopes While Playing.
    ScopesWhilePlaying,
    /// The Mixing Console's Automation Settings: an envelope's line and
    /// points.
    Automation,
    /// A text's lines lined up on the left, in the middle, on the right.
    AlignLeft,
    AlignCenter,
    AlignRight,
    /// Transfer Text Properties: a brush passing a look on.
    Transfer,
    /// Replace Plug-In: f, x and a plus.
    FxAdd,
    /// Match Event Length: a frame stretched both ways.
    FitLength,
    /// A plug-in's help: a question mark in a ring.
    Help,
    /// Open VST Bank and Save VST Bank As: two folders, a disk over a page.
    OpenBank,
    SaveBank,
    /// Invert Track Phase: two waves crossing.
    Phase,
}

/// The grid the paths are drawn on.
const GRID: f32 = 24.0;
/// The stroke's width in grid units, and the least it is in pixels (thinner
/// lines turn grey).
const STROKE: f32 = 1.5;
const LEAST_STROKE: f32 = 1.1;
/// How much of the icon's color the tint of a body has, at the top of the
/// icon and at its foot: the light comes from above.
const SOFT_TOP: f32 = 0.34;
const SOFT_FOOT: f32 = 0.08;
/// How much of its strength a secondary stroke has.
const DIM: f32 = 0.5;
/// The outline of a solid shape, in grid units: it makes its corners round.
const SOLID_EDGE: f32 = 1.2;
/// The radius, in grid units, that sharp corners of the paths are rounded to
/// (of solid shapes, which also have their outline: less).
const CORNER: f32 = 1.0;
const SOLID_CORNER: f32 = 0.35;
/// Where a stroke turns by more than about 35° it is cut for a round join.
const SHARP_COS: f32 = 0.82;
/// Dashes and their gaps, in grid units.
const DASH: f32 = 2.0;
const GAP: f32 = 2.2;
/// Under this many pixels across, an icon is drawn on the pixel grid: lines
/// one pixel wide through pixel centers, as a hand-made small icon is.
const HINTED_BELOW: f32 = 17.0;
/// From this many pixels across, the fine details of an icon are drawn.
const FINE_FROM: f32 = 20.0;
/// The red of a recording, as the interface uses it elsewhere.
const RED: Color32 = Color32::from_rgb(0xE0, 0x4F, 0x45);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fill {
    None,
    /// The icon's color at a fraction of its strength.
    Soft,
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edge {
    None,
    Line,
    Dashed,
    /// The thin outline that rounds the corners of solid shapes.
    Thin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ink {
    /// The color the caller gave.
    Icon,
    Red,
}

/// A shape of an icon: its paths, how they are filled and outlined.
#[derive(Clone, Debug)]
struct Layer {
    fill: Fill,
    edge: Edge,
    ink: Ink,
    /// Left out of small icons, where it would only blur them.
    fine: bool,
    /// A supporting detail, drawn at half strength.
    dim: bool,
    subs: Vec<Sub>,
}

const COUNT: usize = Icon::Phase as usize + 1;

/// Each icon's layers, made the first time it is drawn.
static LAYERS: [OnceLock<Vec<Layer>>; COUNT] = [const { OnceLock::new() }; COUNT];

/// Draws the icon inside `rect` with the given color.
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    // Options › Preferences › Display › "Icon color strength".
    let color = crate::theme::icon_color(color);
    let s = rect.width().min(rect.height());
    let ppp = painter.ctx().pixels_per_point();
    let pixels = s * ppp;
    if !pixels.is_finite() || pixels < 4.0 {
        return;
    }
    let hinted = pixels < HINTED_BELOW;
    let detailed = pixels >= FINE_FROM;
    let unit = s / GRID;
    // The icon's corner on a whole pixel, so edges of the same weight look
    // the same wherever the icon is drawn.
    let snap = |v: f32| (v * ppp).round() / ppp;
    let corner = rect.center() - vec2(s, s) / 2.0;
    let corner = pos2(snap(corner.x), snap(corner.y));
    // Small icons: every point to the middle of a pixel, and lines of one.
    let center = |v: f32| ((v * ppp).floor() + 0.5) / ppp;
    let to_px = |p: Pos2| {
        let (x, y) = (corner.x + p.x * unit, corner.y + p.y * unit);
        if hinted {
            pos2(center(x), center(y))
        } else {
            pos2(x, y)
        }
    };
    let width = if hinted {
        1.0 / ppp
    } else {
        (STROKE * unit).max(LEAST_STROKE / ppp)
    };
    let thin = if hinted {
        1.0 / ppp
    } else {
        (SOLID_EDGE * unit).max(1.0 / ppp)
    };
    let round = !hinted;
    let red = crate::theme::icon_color(RED);
    for layer in LAYERS[icon as usize].get_or_init(|| glyphs::layers(icon)) {
        if layer.fine && !detailed {
            continue;
        }
        let ink = match layer.ink {
            Ink::Icon => color,
            Ink::Red => red,
        };
        let ink = if layer.dim {
            ink.gamma_multiply(DIM)
        } else {
            ink
        };
        let (top, foot) = (corner.y, corner.y + s);
        let tint = |p: Pos2| {
            let t = ((p.y - top) / (foot - top)).clamp(0.0, 1.0);
            ink.gamma_multiply(SOFT_TOP + (SOFT_FOOT - SOFT_TOP) * t)
        };
        for sub in &layer.subs {
            let corner_radius = if layer.edge == Edge::Thin {
                SOLID_CORNER
            } else {
                CORNER
            };
            let mut points = sub.flatten(&to_px);
            if !hinted {
                points = path::round_corners(&points, sub.closed, corner_radius * unit);
            }
            let (points, closed) = tidy(points, sub.closed, ppp, hinted);
            if let [dot] = points[..] {
                // Squeezed to a point by the pixel grid: its pixel.
                if layer.fill == Fill::Full || layer.edge != Edge::None {
                    painter.rect_filled(
                        Rect::from_center_size(dot, vec2(1.0, 1.0) / ppp),
                        0.0,
                        ink,
                    );
                }
                continue;
            }
            if points.len() < 2 {
                continue;
            }
            match layer.fill {
                Fill::None => {}
                Fill::Soft => fill(painter, &points, &tint),
                Fill::Full => fill(painter, &points, &|_| ink),
            }
            match layer.edge {
                Edge::None => {}
                Edge::Line => stroke(painter, &points, closed, Stroke::new(width, ink), round),
                Edge::Thin => stroke(painter, &points, closed, Stroke::new(thin, ink), false),
                Edge::Dashed => {
                    let (dash, gap) = if hinted {
                        ((DASH * unit).max(2.0 / ppp), (GAP * unit).max(1.5 / ppp))
                    } else {
                        (DASH * unit, GAP * unit)
                    };
                    let mut line = points;
                    if closed {
                        line.push(line[0]);
                    }
                    painter.extend(Shape::dashed_line(
                        &line,
                        Stroke::new(width, ink),
                        dash,
                        gap,
                    ));
                }
            }
        }
    }
}

/// A path without what the pixel grid made degenerate: points that repeat,
/// spikes where the path goes back along itself (egui's strokes cannot join
/// those), a shape squeezed to a point, and a closed shape squeezed flat,
/// which is the line it has become. (Icons that are not on the pixel grid
/// keep their shapes but for exact repeats and exact turns back.)
fn tidy(mut points: Vec<Pos2>, mut closed: bool, ppp: f32, hinted: bool) -> (Vec<Pos2>, bool) {
    // What is nearer than this is the same point: a tenth of a pixel on the grid.
    let near = if hinted { 0.1 / ppp } else { 1e-3 };
    // The cosine under which the path counts as going back on itself.
    let turn_back = if hinted { -0.97 } else { -0.9999 };
    let dedup = |points: &mut Vec<Pos2>, closed: bool| {
        points.dedup_by(|b, a| (*b - *a).length() < near);
        while closed && points.len() > 1 && (points[0] - points[points.len() - 1]).length() < near {
            points.pop();
        }
    };
    points.retain(|p| p.is_finite());
    dedup(&mut points, closed);
    while let Some(i) = spike(&points, closed, turn_back) {
        points.remove(i);
        dedup(&mut points, closed);
    }
    if hinted && !points.is_empty() {
        let bounds = Rect::from_points(&points);
        if bounds.width().max(bounds.height()) < 1.2 / ppp {
            points = vec![bounds.center()];
        }
    }
    if closed && points.len() >= 3 {
        let (a, b) = (points[0], points[1]);
        let flat = points
            .iter()
            .all(|p| ((b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)).abs() < 1e-3);
        if flat {
            let along = |p: Pos2| (p - a).dot(b - a);
            let (mut low, mut high) = (a, a);
            for &p in &points {
                if along(p) < along(low) {
                    low = p;
                }
                if along(p) > along(high) {
                    high = p;
                }
            }
            points = vec![low, high];
        }
    }
    if points.len() <= 2 {
        closed = false;
    }
    (points, closed)
}

/// The first point where the path turns back on itself (the cosine of the
/// angle between the pieces is under `turn_back`), if any.
fn spike(points: &[Pos2], closed: bool, turn_back: f32) -> Option<usize> {
    let n = points.len();
    if n < 3 {
        return None;
    }
    (0..n).find(|&i| {
        if !closed && (i == 0 || i + 1 == n) {
            return false;
        }
        let u = points[i] - points[(i + n - 1) % n];
        let v = points[(i + 1) % n] - points[i];
        u.dot(v) < turn_back * u.length() * v.length()
    })
}

/// Fills a polygon (concave ones too), each corner in the color `shade` gives.
fn fill(painter: &Painter, points: &[Pos2], shade: &dyn Fn(Pos2) -> Color32) {
    let mut mesh = Mesh::default();
    for &p in points {
        mesh.colored_vertex(p, shade(p));
    }
    mesh.indices = path::triangulate(points);
    painter.add(Shape::mesh(mesh));
}

/// A stroke with round ends and round joins. Where the path turns sharply,
/// at a point that is not rounded already, it is cut in two and each piece
/// gets a round end: the two round ends make the join. (Small icons, on the
/// pixel grid, have plain strokes.)
fn stroke(painter: &Painter, points: &[Pos2], closed: bool, stroke: Stroke, round: bool) {
    let n = points.len();
    if !round {
        painter.add(if closed {
            Shape::closed_line(points.to_vec(), stroke)
        } else {
            Shape::line(points.to_vec(), stroke)
        });
        return;
    }
    let sharp = |i: usize| {
        let before = (points[i] - points[(i + n - 1) % n]).normalized();
        let after = (points[(i + 1) % n] - points[i]).normalized();
        before.dot(after) < SHARP_COS
    };
    let cuts: Vec<usize> = (0..n)
        .filter(|&i| (closed || (i > 0 && i + 1 < n)) && sharp(i))
        .collect();
    if closed && cuts.is_empty() {
        painter.add(Shape::closed_line(points.to_vec(), stroke));
        return;
    }
    // The pieces, from one cut to the next: a closed path goes round to its
    // first cut, an open one has its two ends besides.
    let mut starts = cuts;
    if !closed {
        starts.insert(0, 0);
    }
    for (k, &from) in starts.iter().enumerate() {
        let to = match starts.get(k + 1) {
            Some(&next) => next,
            None if closed => starts[0] + n,
            None => n - 1,
        };
        let piece: Vec<Pos2> = (from..=to).map(|i| points[i % n]).collect();
        if piece.len() > 1 {
            line(painter, &piece, stroke);
        }
    }
}

/// An open stroke: the line with soft edges and a round end at each end.
fn line(painter: &Painter, points: &[Pos2], stroke: Stroke) {
    let feather = 1.0 / painter.ctx().pixels_per_point();
    let (inner, outer) = (
        (stroke.width - feather) / 2.0,
        (stroke.width + feather) / 2.0,
    );
    let n = points.len();
    // Four vertices at each point: the soft edge, the solid edge, the other
    // solid edge and the other soft edge. The ends are bare: the caps join
    // them exactly, so a color that lets what is under it show is not drawn
    // twice.
    let normal_of = |i: usize| (points[i + 1] - points[i]).normalized().rot90();
    let mut mesh = Mesh::default();
    for (i, &point) in points.iter().enumerate() {
        let normal = if i == 0 {
            normal_of(0)
        } else if i == n - 1 {
            normal_of(n - 2)
        } else {
            // Mitered, so that the line keeps its width where it bends.
            let mean = (normal_of(i - 1) + normal_of(i)) / 2.0;
            mean / mean.length_sq().max(0.25)
        };
        mesh.colored_vertex(point + normal * outer, Color32::TRANSPARENT);
        mesh.colored_vertex(point + normal * inner, stroke.color);
        mesh.colored_vertex(point - normal * inner, stroke.color);
        mesh.colored_vertex(point - normal * outer, Color32::TRANSPARENT);
    }
    for i in 0..n as u32 - 1 {
        for k in 0..3 {
            let (a, b) = (4 * i + k, 4 * (i + 1) + k);
            mesh.add_triangle(a, a + 1, b);
            mesh.add_triangle(a + 1, b + 1, b);
        }
    }
    painter.add(Shape::mesh(mesh));
    cap(painter, points[0], points[1], stroke);
    cap(painter, points[n - 1], points[n - 2], stroke);
}

/// A round end for a stroke that ends at `end`, coming from `from`: the half
/// disc beyond the end, with the soft edge strokes have. It is a mesh of its
/// own that stays clear of the stroke, so that a color that lets what is
/// under it show is not drawn twice.
fn cap(painter: &Painter, end: Pos2, from: Pos2, stroke: Stroke) {
    const STEPS: u32 = 8;
    let feather = 1.0 / painter.ctx().pixels_per_point();
    let out = (end - from).normalized();
    let heading = out.y.atan2(out.x);
    let (inner, outer) = (
        (stroke.width - feather) / 2.0,
        (stroke.width + feather) / 2.0,
    );
    let mut mesh = Mesh::default();
    mesh.colored_vertex(end, stroke.color);
    for k in 0..=STEPS {
        let angle = heading - FRAC_PI_2 + PI * k as f32 / STEPS as f32;
        let dir = vec2(angle.cos(), angle.sin());
        mesh.colored_vertex(end + dir * inner, stroke.color);
        mesh.colored_vertex(end + dir * outer, Color32::TRANSPARENT);
    }
    for k in 0..STEPS {
        let (a, b, c, d) = (1 + 2 * k, 2 + 2 * k, 3 + 2 * k, 4 + 2 * k);
        mesh.add_triangle(0, a, c);
        mesh.add_triangle(a, b, d);
        mesh.add_triangle(a, d, c);
    }
    painter.add(Shape::mesh(mesh));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_is_drawn() {
        for (i, icon) in glyphs::ALL.iter().enumerate() {
            assert_eq!(*icon as usize, i, "{icon:?} is out of order in ALL");
            assert!(!glyphs::layers(*icon).is_empty(), "{icon:?} has no shape");
        }
        assert_eq!(glyphs::ALL.len(), COUNT);
    }

    #[test]
    fn every_icon_stays_on_its_grid() {
        let same = |p: Pos2| p;
        for icon in glyphs::ALL {
            for layer in glyphs::layers(icon) {
                for sub in &layer.subs {
                    for p in sub.flatten(&same) {
                        assert!(
                            (0.5..=GRID - 0.5).contains(&p.x) && (0.5..=GRID - 0.5).contains(&p.y),
                            "{icon:?} has a point outside the grid: {p:?}"
                        );
                    }
                }
            }
        }
    }

    /// Every icon draws at any size, scale and color: shapes with finite
    /// points, and none at all where there is no room.
    #[test]
    fn every_icon_paints_at_any_size_and_scale() {
        use crate::egui::{Context, RawInput, epaint::Primitive};
        let colors = [
            Color32::WHITE,
            Color32::from_rgba_unmultiplied(255, 255, 255, 90),
        ];
        for ppp in [1.0, 1.5, 2.0] {
            let ctx = Context::default();
            ctx.set_pixels_per_point(ppp);
            for size in [3.0, 8.0, 14.0, 16.0, 18.0, 20.0, 24.0, 33.0, 64.0, 200.0] {
                let rect = Rect::from_min_size(pos2(10.3, 7.7), vec2(size, size));
                for color in colors {
                    let mut out = ctx.run_ui(RawInput::default(), |ui| {
                        for icon in glyphs::ALL {
                            paint(ui.painter(), rect, icon, color);
                        }
                    });
                    out.textures_delta.clear();
                    let painted = ctx.tessellate(out.shapes, ppp);
                    assert_eq!(painted.is_empty(), size * ppp < 4.0, "{size} px at {ppp}");
                    for primitive in painted {
                        let Primitive::Mesh(mesh) = primitive.primitive else {
                            continue;
                        };
                        assert!(mesh.vertices.iter().all(|v| v.pos.is_finite()));
                        assert!(
                            mesh.indices
                                .iter()
                                .all(|&i| (i as usize) < mesh.vertices.len())
                        );
                    }
                }
            }
        }
    }
}
