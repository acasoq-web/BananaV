//! The Mask group of VEGAS's Glint and Rays: where the effect shows — a
//! rectangle, a bar, a wipe or an ellipse — and how (on the effect, on
//! its source, or the mask alone). The shapes are BananaV's; the card's
//! `vm_mask` (the
//! shaders' prelude) works them out the same way.

use crate::kit::Vals;

/// The group's specs, after an effect's own: `$types` the shapes in the
/// effect's order.
macro_rules! with_mask {
    ([$($own:expr),* $(,)?], $types:expr) => {
        &[
            $($own,)*
            Spec::Group(c"mask", c"Mask"),
            Spec::Choice(
                c"maskMode",
                c"Mask mode",
                &[c"Disabled", c"Effect", c"Source", c"Show mask only"],
                0,
                c"Where the mask works: not at all, on the effect, on the lights it starts from, or shown alone.",
            ),
            Spec::Choice(c"maskType", c"Mask type", $types, 0, c"The mask's shape."),
            Spec::Number(
                c"maskProgress",
                c"Progress",
                50.0,
                0.0,
                100.0,
                c"How far the wipe has gone, or how big the shape is (50: its size), in percent.",
            ),
            Spec::Number(
                c"maskSoftness",
                c"Softness",
                0.0,
                0.0,
                100.0,
                c"How softly the mask's edge goes over, in percent.",
            ),
            Spec::Number(
                c"maskWidth",
                c"Width",
                50.0,
                0.0,
                100.0,
                c"The shape's width, in percent of the frame's.",
            ),
            Spec::Number(
                c"maskHeight",
                c"Height",
                50.0,
                0.0,
                100.0,
                c"The shape's height (the bar's thickness), in percent of the frame's.",
            ),
            Spec::Number(
                c"maskCornerRadius",
                c"Corner radius",
                0.0,
                0.0,
                100.0,
                c"The rectangle's corners rounded, in percent.",
            ),
            Spec::Point(c"maskPosition", c"Position", [0.5, 0.5], c"The mask's middle."),
            Spec::Number(
                c"maskFalloff",
                c"Falloff",
                0.0,
                0.0,
                100.0,
                c"How far in from its edge the mask fades, in percent.",
            ),
            Spec::Number(c"maskAngle", c"Angle", 0.0, 0.0, 360.0, c"The mask turned, in degrees."),
            Spec::Number(
                c"maskStrength",
                c"Strength",
                100.0,
                0.0,
                100.0,
                c"How much the mask holds the effect back, in percent.",
            ),
            Spec::Number(
                c"maskLeadingRegion",
                c"Leading region",
                0.0,
                0.0,
                100.0,
                c"A softer edge outside the mask, in percent.",
            ),
            Spec::Number(
                c"maskTrailingRegion",
                c"Trailing region",
                0.0,
                0.0,
                100.0,
                c"A softer edge inside the mask, in percent.",
            ),
            Spec::GroupEnd,
        ]
    };
}
pub(crate) use with_mask;

/// The mask's settings: its mode (0 Disabled, 1 Effect, 2 Source, 3 Show
/// mask only) and the shape's numbers, as the card's `vm_mask` reads them:
/// the shape (0 rectangle, 1 bar, 2 wipe, 3 ellipse), Progress, Softness,
/// Width, Height, Corner radius (0–1), the middle (0–1, rows from the
/// top), Falloff (0–1), Angle (radians), Strength, Leading and Trailing
/// regions (0–1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Mask {
    pub(crate) mode: u32,
    pub(crate) numbers: [f32; 13],
}

impl Mask {
    /// The settings `s` name, the effect's shapes in `order` (BananaV's
    /// number for each of the list's).
    pub(crate) fn of(s: &Vals, order: [u32; 4]) -> Mask {
        let pc = |name| (s.f(name).clamp(0.0, 100.0) * 0.01) as f32;
        let [cx, cy] = s.point(c"maskPosition");
        let kind = order[s.i(c"maskType").clamp(0, 3) as usize];
        Mask {
            mode: s.i(c"maskMode").clamp(0, 3) as u32,
            numbers: [
                kind as f32,
                pc(c"maskProgress"),
                pc(c"maskSoftness"),
                pc(c"maskWidth"),
                pc(c"maskHeight"),
                pc(c"maskCornerRadius"),
                cx as f32,
                cy as f32,
                pc(c"maskFalloff"),
                s.f(c"maskAngle").to_radians() as f32,
                pc(c"maskStrength"),
                pc(c"maskLeadingRegion"),
                pc(c"maskTrailingRegion"),
            ],
        }
    }

    /// The mask at `u`, `v` (0–1, rows from the top) of a frame `aspect`
    /// times as wide as high: 1 where the effect shows whole, as the card's
    /// `vm_mask` works it out.
    pub(crate) fn at(&self, u: f32, v: f32, aspect: f32) -> f32 {
        mask_at(&self.numbers, u, v, aspect)
    }
}

/// [`Mask::at`] from the numbers alone (the card's `vm_mask`, in floats
/// as the card's).
pub(crate) fn mask_at(n: &[f32; 13], u: f32, v: f32, aspect: f32) -> f32 {
    let [
        kind,
        progress,
        soft,
        w,
        h,
        corner,
        cx,
        cy,
        falloff,
        angle,
        strength,
        lead,
        trail,
    ] = *n;
    let (px, py) = ((u - cx) * aspect, v - cy);
    let (c, s) = (angle.cos(), angle.sin());
    let (rx, ry) = (c * px + s * py, c * py - s * px);
    // The shapes' size by Progress, its size at 50.
    let grow = progress * 2.0;
    let d = if kind < 0.5 {
        let (hx, hy) = (w * 0.5 * aspect * grow, h * 0.5 * grow);
        let r = corner * hx.min(hy);
        let (qx, qy) = (rx.abs() - (hx - r), ry.abs() - (hy - r));
        qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r
    } else if kind < 1.5 {
        ry.abs() - h * 0.5 * grow
    } else if kind < 2.5 {
        let reach = (c.abs() * aspect + s.abs()) * 0.5;
        rx - (progress * 2.0 - 1.0) * reach
    } else {
        let (hx, hy) = (
            (w * 0.5 * aspect * grow).max(1e-6),
            (h * 0.5 * grow).max(1e-6),
        );
        ((rx / hx).hypot(ry / hy) - 1.0) * hx.min(hy)
    };
    let (ahead, behind) = (soft * 0.25 + lead * 0.5, soft * 0.25 + trail * 0.5);
    let mut m = if ahead + behind > 1e-6 {
        ((ahead - d) / (ahead + behind)).clamp(0.0, 1.0)
    } else if d <= 0.0 {
        1.0
    } else {
        0.0
    };
    if falloff > 0.0 {
        m *= (-d / (falloff * 0.5)).clamp(0.0, 1.0);
    }
    1.0 - strength * (1.0 - m)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask(kind: f32) -> [f32; 13] {
        [
            kind, 0.5, 0.0, 0.5, 0.5, 0.0, 0.5, 0.5, 0.0, 0.0, 1.0, 0.0, 0.0,
        ]
    }

    #[test]
    fn the_shapes_hold_their_insides() {
        // A rectangle and an ellipse half the frame, in its middle.
        for kind in [0.0, 3.0] {
            let n = mask(kind);
            assert_eq!(mask_at(&n, 0.5, 0.5, 16.0 / 9.0), 1.0);
            assert_eq!(mask_at(&n, 0.05, 0.5, 16.0 / 9.0), 0.0);
        }
        // A bar across, half the frame thick.
        let bar = mask(1.0);
        assert_eq!(mask_at(&bar, 0.02, 0.5, 1.0), 1.0);
        assert_eq!(mask_at(&bar, 0.5, 0.9, 1.0), 0.0);
        // A wipe halfway: the left half.
        let wipe = mask(2.0);
        assert_eq!(mask_at(&wipe, 0.3, 0.5, 1.0), 1.0);
        assert_eq!(mask_at(&wipe, 0.7, 0.5, 1.0), 0.0);
        // Strength 0: no mask.
        let mut weak = mask(0.0);
        weak[10] = 0.0;
        assert_eq!(mask_at(&weak, 0.05, 0.5, 1.0), 1.0);
    }
}
