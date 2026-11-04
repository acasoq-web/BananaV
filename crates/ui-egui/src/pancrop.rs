//! A box of the Pan/Crop and Track Motion workspaces as they draw and drag
//! it: its middle, its size and its angle, in numbers the screen's geometry
//! works with.

use bv_timeline::pancrop::PanCropKey;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    /// Centro, em pixels da mídia.
    pub cx: f32,
    pub cy: f32,
    /// Tamanho, em pixels da mídia.
    pub w: f32,
    pub h: f32,
    /// Ângulo do retângulo sobre a mídia, em graus (sentido horário na tela).
    pub angle_deg: f32,
}

pub(crate) fn rot(v: (f32, f32), deg: f32) -> (f32, f32) {
    let (s, c) = deg.to_radians().sin_cos();
    (v.0 * c - v.1 * s, v.0 * s + v.1 * c)
}

impl Viewport {
    /// A Pan/Crop's box (its mirrors aside: its size counts from them).
    pub fn of(key: &PanCropKey) -> Viewport {
        Viewport {
            cx: key.center[0] as f32,
            cy: key.center[1] as f32,
            w: key.width.abs() as f32,
            h: key.height.abs() as f32,
            angle_deg: key.angle.to_degrees() as f32,
        }
    }

    /// `key` with what makes this box differ from `was`: its size, with the
    /// mirrors it has, its middle, its pivot following when it was there,
    /// its angle. What did not change stays exactly as it was.
    pub fn onto(&self, was: &Viewport, key: &PanCropKey) -> PanCropKey {
        let mut k = *key;
        if self.w != was.w {
            k.width = f64::from(self.w).copysign(key.width);
        }
        if self.h != was.h {
            k.height = f64::from(self.h).copysign(key.height);
        }
        if self.cx != was.cx {
            k.center[0] = f64::from(self.cx);
        }
        if self.cy != was.cy {
            k.center[1] = f64::from(self.cy);
        }
        if self.angle_deg != was.angle_deg {
            k.angle = f64::from(self.angle_deg).to_radians();
        }
        if key.pivot == key.center {
            k.pivot = k.center;
        }
        k
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEDIA: [f64; 2] = [1280.0, 720.0];

    #[test]
    fn the_whole_media_is_its_box() {
        let v = Viewport::of(&PanCropKey::whole(MEDIA));
        assert_eq!((v.w, v.h), (1280.0, 720.0));
        assert_eq!((v.cx, v.cy), (640.0, 360.0));
        assert_eq!(v.angle_deg, 0.0);
    }

    #[test]
    fn mirrors_stay_out_of_its_size_and_come_back() {
        let k = PanCropKey {
            width: -640.0,
            ..PanCropKey::whole(MEDIA)
        };
        let v = Viewport::of(&k);
        assert_eq!(v.w, 640.0);
        let mut w = v;
        w.w = 320.0;
        assert_eq!(w.onto(&v, &k).width, -320.0);
    }

    #[test]
    fn only_what_changed_changes() {
        let k = PanCropKey {
            center: [100.123_456_789, 200.0],
            pivot: [7.0, 8.0],
            ..PanCropKey::whole(MEDIA)
        };
        let v = Viewport::of(&k);
        assert_eq!(v.onto(&v, &k), k);
        let mut w = v;
        w.cy = 210.0;
        let moved = w.onto(&v, &k);
        assert_eq!(moved.center, [100.123_456_789, 210.0]);
        assert_eq!(moved.pivot, [7.0, 8.0]);
    }

    #[test]
    fn a_pivot_in_the_middle_follows_it() {
        let k = PanCropKey::whole(MEDIA);
        let v = Viewport::of(&k);
        let mut w = v;
        w.cx = 700.0;
        assert_eq!(w.onto(&v, &k).pivot, [700.0, 360.0]);
    }
}
