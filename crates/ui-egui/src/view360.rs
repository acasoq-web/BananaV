//! The Video Preview's 360° View (VEGAS's 360° mode): a 360° project's
//! frame — the whole sphere around the camera, laid out
//! equirectangularly, longitude across and latitude down — seen through
//! a camera looking one way, as 360° players show it. Dragging turns the
//! camera, the mouse wheel zooms (narrows what it sees) and a double-click
//! looks ahead again.

use bv_core::Tick;
use bv_timeline::Generator;

use crate::app::BananaApp;
use crate::egui::{self, Color32, Rect, TextureId, pos2};

/// How wide a view starts, in degrees across.
const DEFAULT_FOV: f32 = 90.0;
/// How narrow and how wide it can get.
const MIN_FOV: f32 = 20.0;
const MAX_FOV: f32 = 150.0;

/// Where the camera looks — degrees right of the frame's middle (yaw) and
/// up from its horizon (pitch) — and how wide it sees, in degrees across.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Look {
    pub yaw: f32,
    pub pitch: f32,
    pub fov: f32,
}

impl Default for Look {
    fn default() -> Look {
        Look {
            yaw: 0.0,
            pitch: 0.0,
            fov: DEFAULT_FOV,
        }
    }
}

impl Look {
    /// The frame's point (0–1 across and down) seen at `(sx, sy)` of a view
    /// `aspect` times as wide as it is high, counted −1 to 1 from its left
    /// and from its bottom.
    pub(crate) fn frame_point(&self, aspect: f32, sx: f32, sy: f32) -> (f32, f32) {
        let (sin_yaw, cos_yaw) = self.yaw.to_radians().sin_cos();
        let (sin_pitch, cos_pitch) = self.pitch.to_radians().sin_cos();
        let across = (self.fov.to_radians() / 2.0).tan();
        let down = across / aspect.max(1e-3);
        // The camera's axes: ahead, to its right and to its top.
        let ahead = [sin_yaw * cos_pitch, sin_pitch, cos_yaw * cos_pitch];
        let right = [cos_yaw, 0.0, -sin_yaw];
        let top = [-sin_yaw * sin_pitch, cos_pitch, -cos_yaw * sin_pitch];
        let d: [f32; 3] =
            std::array::from_fn(|i| ahead[i] + sx * across * right[i] + sy * down * top[i]);
        let longitude = d[0].atan2(d[2]);
        let latitude = d[1].atan2(d[0].hypot(d[2]));
        (
            0.5 + longitude / std::f32::consts::TAU,
            0.5 - latitude / std::f32::consts::PI,
        )
    }

    /// A drag of `delta` points over a view `width` points wide: the picture
    /// follows the pointer.
    pub(crate) fn drag(&mut self, delta: egui::Vec2, width: f32) {
        // The degrees a point spans in the middle of the view.
        let per_point = (2.0 * (self.fov.to_radians() / 2.0).tan() / width.max(1.0)).to_degrees();
        self.yaw = (self.yaw - delta.x * per_point + 180.0).rem_euclid(360.0) - 180.0;
        self.pitch = (self.pitch + delta.y * per_point).clamp(-90.0, 90.0);
    }

    /// The mouse wheel's `notches` toward the screen zoom in: each narrows
    /// the view by a tenth.
    pub(crate) fn zoom(&mut self, notches: f32) {
        self.fov = (self.fov * 0.9f32.powf(notches)).clamp(MIN_FOV, MAX_FOV);
    }
}

/// Draws the camera's view of the frame `texture` in `rect`: a grid whose
/// corners show the frame where the camera sees them. The texture must
/// repeat across (`TextureOptions::LINEAR_REPEAT`): cells over the frame's
/// left and right edges read past its right edge.
pub(crate) fn paint(painter: &egui::Painter, rect: Rect, texture: TextureId, look: &Look) {
    const ACROSS: usize = 64;
    const DOWN: usize = 40;
    let aspect = rect.width() / rect.height().max(1.0);
    let at = |i: usize, j: usize| {
        let sx = 2.0 * i as f32 / ACROSS as f32 - 1.0;
        let sy = 1.0 - 2.0 * j as f32 / DOWN as f32;
        look.frame_point(aspect, sx, sy)
    };
    let points: Vec<(f32, f32)> = (0..=DOWN)
        .flat_map(|j| (0..=ACROSS).map(move |i| at(i, j)))
        .collect();
    let mut mesh = egui::Mesh::with_texture(texture);
    for j in 0..DOWN {
        for i in 0..ACROSS {
            let corners = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)];
            let mut uv = corners.map(|(a, b)| points[b * (ACROSS + 1) + a]);
            let low = uv.iter().map(|p| p.0).fold(f32::MAX, f32::min);
            let high = uv.iter().map(|p| p.0).fold(f32::MIN, f32::max);
            if high - low > 0.5 {
                for p in &mut uv {
                    if p.0 < 0.5 {
                        p.0 += 1.0;
                    }
                }
            }
            let base = mesh.vertices.len() as u32;
            for ((a, b), (u, v)) in corners.into_iter().zip(uv) {
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: pos2(
                        rect.left() + rect.width() * a as f32 / ACROSS as f32,
                        rect.top() + rect.height() * b as f32 / DOWN as f32,
                    ),
                    uv: pos2(u, v),
                    color: Color32::WHITE,
                });
            }
            mesh.add_triangle(base, base + 1, base + 2);
            mesh.add_triangle(base, base + 2, base + 3);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

impl BananaApp {
    /// For the screen tests: a 360° project (2:1, 360 Output on) of a
    /// checkerboard, looked at through the 360° View up and to the right.
    pub fn view_360_for_review(&mut self) {
        self.modify(|p| {
            p.settings.width = 2048;
            p.settings.height = 1024;
            p.settings.output_360 = true;
        });
        self.add_generator(Generator::Checkerboard { size: 64 }, Tick::ZERO, None);
        self.preview.view_360 = true;
        self.preview.look = Look {
            yaw: 30.0,
            pitch: 25.0,
            fov: 100.0,
        };
        self.seek(Tick::from_seconds(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4
    }

    #[test]
    fn the_view_s_middle_is_where_the_camera_looks() {
        let look = |yaw, pitch| Look {
            yaw,
            pitch,
            fov: 90.0,
        };
        assert!(near(look(0.0, 0.0).frame_point(2.0, 0.0, 0.0), (0.5, 0.5)));
        // A quarter turn right is three quarters across the frame, left a
        // quarter; half a turn is its left and right edges.
        assert!(near(
            look(90.0, 0.0).frame_point(2.0, 0.0, 0.0),
            (0.75, 0.5)
        ));
        assert!(near(
            look(-90.0, 0.0).frame_point(2.0, 0.0, 0.0),
            (0.25, 0.5)
        ));
        let (u, _) = look(180.0, 0.0).frame_point(2.0, 0.0, 0.0);
        assert!(u.abs() < 1e-4 || (u - 1.0).abs() < 1e-4, "{u}");
        // Looking up is toward the frame's top; straight up, its top edge.
        assert!(near(
            look(0.0, 45.0).frame_point(2.0, 0.0, 0.0),
            (0.5, 0.25)
        ));
        assert!(look(0.0, 90.0).frame_point(2.0, 0.0, 0.0).1 < 1e-4);
    }

    #[test]
    fn the_view_s_edges_are_half_its_field_of_view_away() {
        let ahead = Look::default();
        // 90° across: the right edge is 45° right, an eighth of the frame.
        assert!(near(ahead.frame_point(2.0, 1.0, 0.0), (0.625, 0.5)));
        assert!(near(ahead.frame_point(2.0, -1.0, 0.0), (0.375, 0.5)));
        // Twice as wide as high: the top edge is atan(tan 45° ⁄ 2) up.
        let up = (0.5f32).atan();
        let (_, v) = ahead.frame_point(2.0, 0.0, 1.0);
        assert!((v - (0.5 - up / std::f32::consts::PI)).abs() < 1e-4, "{v}");
    }

    #[test]
    fn dragging_moves_the_picture_with_the_pointer() {
        let mut look = Look::default();
        // To the right: the camera turns left; down: it looks up.
        look.drag(egui::vec2(50.0, 0.0), 500.0);
        assert!(look.yaw < 0.0 && look.pitch == 0.0, "{look:?}");
        look.drag(egui::vec2(0.0, 50.0), 500.0);
        assert!(look.pitch > 0.0, "{look:?}");
        // Never past straight up; around and around across.
        look.drag(egui::vec2(0.0, 100_000.0), 500.0);
        assert_eq!(look.pitch, 90.0);
        look.drag(egui::vec2(-100_000.0, 0.0), 500.0);
        assert!((-180.0..180.0).contains(&look.yaw), "{look:?}");
    }

    #[test]
    fn the_wheel_zooms_within_limits() {
        let mut look = Look::default();
        look.zoom(1.0);
        assert!((look.fov - 81.0).abs() < 1e-3, "{}", look.fov);
        look.zoom(-2.0);
        assert!((look.fov - 100.0).abs() < 1e-3, "{}", look.fov);
        look.zoom(100.0);
        assert_eq!(look.fov, MIN_FOV);
        look.zoom(-100.0);
        assert_eq!(look.fov, MAX_FOV);
    }
}
