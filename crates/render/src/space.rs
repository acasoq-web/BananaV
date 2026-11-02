//! 3D compositing on the processor: tracks' pictures on boxes in space
//! (`bv_timeline::space`) drawn together — at each pixel the boxes the
//! camera's ray meets, far to near, each over the ones behind it — so boxes
//! that cross intersect.

use bv_core::ImageRgbaF32;
use bv_timeline::space::Plane;

/// The most boxes a pixel shows (more are left out).
const MOST: usize = 16;

/// `layers` (pictures of the frame's size, premultiplied, each on its
/// plane) as the camera of a `w` × `h` frame sees them.
pub(crate) fn draw(layers: &[(ImageRgbaF32, Plane)], w: u32, h: u32) -> ImageRgbaF32 {
    let mut out = ImageRgbaF32::transparent(w, h);
    let (wf, hf) = (w as f32, h as f32);
    crate::par::for_each_pixel(&mut out.data, w as usize, |x, y, px| {
        let (cx, cy) = (x as f32 + 0.5 - wf / 2.0, hf / 2.0 - (y as f32 + 0.5));
        let mut hits = [(0.0f32, [0.0f32; 4]); MOST];
        let mut n = 0;
        for (img, plane) in layers {
            if n == MOST {
                break;
            }
            if let Some((t, a, b)) = plane.hit(cx, cy, wf) {
                let c = sample(img, a, b);
                if c[3] > 0.0 {
                    hits[n] = (t, c);
                    n += 1;
                }
            }
        }
        // Far to near.
        hits[..n].sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut acc = [0.0f32; 4];
        for &(_, c) in &hits[..n] {
            acc = crate::over(c, acc);
        }
        *px = acc;
    });
    out
}

/// The picture's color at (`a`, `b`) (0–1 across and down), bilinear; its
/// edges fade out over half a pixel.
fn sample(img: &ImageRgbaF32, a: f32, b: f32) -> [f32; 4] {
    let (w, h) = (img.width as i64, img.height as i64);
    if w == 0 || h == 0 {
        return [0.0; 4];
    }
    let x = a * w as f32 - 0.5;
    let y = b * h as f32 - 0.5;
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let at = |xi: i64, yi: i64| -> [f32; 4] {
        if xi < 0 || yi < 0 || xi >= w || yi >= h {
            return [0.0; 4];
        }
        img.data[(yi * w + xi) as usize]
    };
    let (x0, y0) = (x0 as i64, y0 as i64);
    let (p00, p10, p01, p11) = (
        at(x0, y0),
        at(x0 + 1, y0),
        at(x0, y0 + 1),
        at(x0 + 1, y0 + 1),
    );
    std::array::from_fn(|c| {
        let top = p00[c] + (p10[c] - p00[c]) * fx;
        let bottom = p01[c] + (p11[c] - p01[c]) * fx;
        top + (bottom - top) * fy
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_timeline::space::Box3d;

    fn solid(w: u32, h: u32, c: [f32; 4]) -> ImageRgbaF32 {
        ImageRgbaF32::filled(w, h, c)
    }

    #[test]
    fn a_box_at_rest_shows_its_picture_as_it_is() {
        let red = solid(64, 36, [1.0, 0.0, 0.0, 1.0]);
        let plane = Box3d::default().plane(64.0, 36.0);
        let out = draw(&[(red, plane)], 64, 36);
        let px = out.data[18 * 64 + 32];
        assert!((px[0] - 1.0).abs() < 1e-4 && px[3] > 0.999, "{px:?}");
    }

    #[test]
    fn crossing_boxes_intersect() {
        // Two boxes turned opposite ways about y: each is in front on one
        // side of the frame's center.
        let red = solid(64, 36, [1.0, 0.0, 0.0, 1.0]);
        let blue = solid(64, 36, [0.0, 0.0, 1.0, 1.0]);
        let turned = |deg: f32| {
            Box3d {
                orientation: [0.0, deg, 0.0],
                ..Box3d::default()
            }
            .plane(64.0, 36.0)
        };
        let out = draw(&[(red, turned(30.0)), (blue, turned(-30.0))], 64, 36);
        let left = out.data[18 * 64 + 12];
        let right = out.data[18 * 64 + 52];
        assert!(left[0] > 0.9 || left[2] > 0.9);
        // The other color on the other side.
        assert!((left[0] > 0.9) != (right[0] > 0.9), "{left:?} {right:?}");
    }

    #[test]
    fn a_farther_box_leaves_the_edges_empty() {
        let white = solid(64, 36, [1.0; 4]);
        let far = Box3d {
            z: 1.0,
            ..Box3d::default()
        }
        .plane(64.0, 36.0);
        let out = draw(&[(white, far)], 64, 36);
        assert_eq!(out.data[0][3], 0.0);
        assert!(out.data[18 * 64 + 32][3] > 0.99);
    }
}
