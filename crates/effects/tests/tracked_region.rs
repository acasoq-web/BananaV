//! What Motion Tracking imports from BananaV's effects (VEGAS's import
//! menu): the corners of a Picture In Picture's picture and of a Bézier
//! Masking's mask, 0–1 of the frame from its top left.

use bv_effects::accel::{Value, Values};
use bv_effects::tracked_region;

fn near(a: [[f64; 2]; 4], b: [[f64; 2]; 4]) -> bool {
    a.iter()
        .flatten()
        .zip(b.iter().flatten())
        .all(|(x, y)| (x - y).abs() < 1e-9)
}

fn values(v: &[(&str, &[f64])]) -> Vec<(String, Value)> {
    v.iter()
        .map(|(n, x)| (n.to_string(), Value::Num(x.to_vec())))
        .collect()
}

#[test]
fn a_picture_in_picture_gives_its_picture() {
    // Half the frame, in its middle: from a quarter to three quarters.
    let v = values(&[("location", &[0.5, 0.5]), ("scaleX", &[0.5])]);
    let q = tracked_region("org.bananav.PictureInPicture", Values(&v), 16.0 / 9.0, 0)
        .expect("a region");
    assert!(
        near(q, [[0.25, 0.25], [0.75, 0.25], [0.75, 0.75], [0.25, 0.75]]),
        "{q:?}"
    );
}

#[test]
fn a_bezier_mask_gives_its_box() {
    // A rectangle 0.4 × 0.2 of the frame, its middle 0.3 across and 0.6
    // up: 0.1–0.5 across, 0.3–0.5 down from the top.
    let v = values(&[
        ("width1", &[0.4]),
        ("height1", &[0.2]),
        ("location1", &[0.3, 0.6]),
    ]);
    let q =
        tracked_region("org.bananav.BezierMasking", Values(&v), 16.0 / 9.0, 1).expect("a region");
    assert!(
        near(q, [[0.1, 0.3], [0.5, 0.3], [0.5, 0.5], [0.1, 0.5]]),
        "{q:?}"
    );
    // The second mask is off at first; other effects give none.
    assert!(tracked_region("org.bananav.BezierMasking", Values(&v), 16.0 / 9.0, 2).is_none());
    assert!(tracked_region("org.bananav.Crop", Values(&v), 16.0 / 9.0, 1).is_none());
}
