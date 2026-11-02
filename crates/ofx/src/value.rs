//! Parameter values and their animation: what a plug-in's parameters hold,
//! at one instant and over time.

use bv_core::curve::CurvePoint;
use serde::{Deserialize, Serialize};

/// The types of parameters OpenFX defines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ParamType {
    Integer,
    Double,
    Boolean,
    Choice,
    /// A choice whose value is the option's text (OpenFX 1.5).
    StrChoice,
    Rgba,
    Rgb,
    Double2D,
    Integer2D,
    Double3D,
    Integer3D,
    String,
    /// Text only the plug-in understands.
    Custom,
    /// Bytes only the plug-in understands (OpenFX 1.5).
    Bytes,
    /// Groups other parameters (a section).
    Group,
    /// A page of the plug-in's interface.
    Page,
    PushButton,
    /// Curves (a color curve, a falloff…).
    Parametric,
}

impl ParamType {
    pub const ALL: [ParamType; 18] = [
        ParamType::Integer,
        ParamType::Double,
        ParamType::Boolean,
        ParamType::Choice,
        ParamType::StrChoice,
        ParamType::Rgba,
        ParamType::Rgb,
        ParamType::Double2D,
        ParamType::Integer2D,
        ParamType::Double3D,
        ParamType::Integer3D,
        ParamType::String,
        ParamType::Custom,
        ParamType::Bytes,
        ParamType::Group,
        ParamType::Page,
        ParamType::PushButton,
        ParamType::Parametric,
    ];

    /// The specification's name (`kOfxParamTypeDouble`…).
    pub fn ofx_name(self) -> &'static std::ffi::CStr {
        use bv_ofx_sys::names::*;
        match self {
            ParamType::Integer => kOfxParamTypeInteger,
            ParamType::Double => kOfxParamTypeDouble,
            ParamType::Boolean => kOfxParamTypeBoolean,
            ParamType::Choice => kOfxParamTypeChoice,
            ParamType::StrChoice => kOfxParamTypeStrChoice,
            ParamType::Rgba => kOfxParamTypeRGBA,
            ParamType::Rgb => kOfxParamTypeRGB,
            ParamType::Double2D => kOfxParamTypeDouble2D,
            ParamType::Integer2D => kOfxParamTypeInteger2D,
            ParamType::Double3D => kOfxParamTypeDouble3D,
            ParamType::Integer3D => kOfxParamTypeInteger3D,
            ParamType::String => kOfxParamTypeString,
            ParamType::Custom => kOfxParamTypeCustom,
            ParamType::Bytes => kOfxParamTypeBytes,
            ParamType::Group => kOfxParamTypeGroup,
            ParamType::Page => kOfxParamTypePage,
            ParamType::PushButton => kOfxParamTypePushButton,
            ParamType::Parametric => kOfxParamTypeParametric,
        }
    }

    pub fn from_ofx_name(name: &std::ffi::CStr) -> Option<ParamType> {
        ParamType::ALL.into_iter().find(|t| t.ofx_name() == name)
    }

    /// How many numbers it holds (0 for text and the others).
    pub fn dimension(self) -> usize {
        match self {
            ParamType::Integer | ParamType::Double | ParamType::Boolean | ParamType::Choice => 1,
            ParamType::Rgba => 4,
            ParamType::Rgb | ParamType::Double3D | ParamType::Integer3D => 3,
            ParamType::Double2D | ParamType::Integer2D => 2,
            _ => 0,
        }
    }

    /// Whether its numbers are whole.
    pub fn is_int(self) -> bool {
        matches!(
            self,
            ParamType::Integer
                | ParamType::Boolean
                | ParamType::Choice
                | ParamType::Integer2D
                | ParamType::Integer3D
        )
    }

    /// Whether it holds text.
    pub fn is_text(self) -> bool {
        matches!(
            self,
            ParamType::String | ParamType::Custom | ParamType::StrChoice
        )
    }

    /// Whether it holds a value at all (groups, pages and buttons do not).
    pub fn has_value(self) -> bool {
        !matches!(
            self,
            ParamType::Group | ParamType::Page | ParamType::PushButton
        )
    }

    /// Whether its value changes in steps over time (never in between).
    pub fn steps(self) -> bool {
        matches!(self, ParamType::Boolean | ParamType::Choice)
            || !self.has_value()
            || self.is_text()
    }

    /// Whether a parameter of this type animates unless its plug-in says
    /// otherwise (as the specification's reference host).
    pub fn animates_by_default(self) -> bool {
        !matches!(
            self,
            ParamType::Custom | ParamType::String | ParamType::Boolean | ParamType::Choice
        ) && self.has_value()
    }
}

/// A parameter's value at one instant.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// The numbers of a numeric parameter (a color's channels, a point's
    /// coordinates; integers, booleans and choices as whole numbers).
    Num(Vec<f64>),
    Text(String),
    Bytes(Vec<u8>),
    /// A parametric parameter's curves: each a list of control points
    /// (position, value and, set by the user, handles).
    Curves(Vec<Vec<CurvePoint>>),
    /// Groups, pages and buttons.
    None,
}

impl Value {
    /// The `i`th number (0 when there is none).
    pub fn num(&self, i: usize) -> f64 {
        match self {
            Value::Num(v) => v.get(i).copied().unwrap_or(0.0),
            _ => 0.0,
        }
    }

    pub fn text(&self) -> &str {
        match self {
            Value::Text(s) => s,
            _ => "",
        }
    }

    /// A value of `ty`'s shape: zero, empty or the type's nothing.
    pub fn zero(ty: ParamType) -> Value {
        match ty {
            t if t.is_text() => Value::Text(String::new()),
            ParamType::Bytes => Value::Bytes(Vec::new()),
            ParamType::Parametric => Value::Curves(Vec::new()),
            t if t.has_value() => Value::Num(vec![0.0; t.dimension()]),
            _ => Value::None,
        }
    }
}

/// How the value goes from a key to the next (VEGAS's keyframe types).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Interp {
    Linear,
    /// Changes fast, then settles.
    Fast,
    /// Changes slowly at first.
    Slow,
    #[default]
    Smooth,
    /// The S turned around: fast at both ends, slow in the middle.
    Sharp,
    /// Keeps the key's value until the next key.
    Hold,
    /// A cubic Bezier curve through the keys' handles (`Key::handles`),
    /// which move together, equal and opposite (VEGAS's "Bezier curve
    /// (locked tangents)")...
    Bezier,
    /// ...or apart ("split tangents"). Both draw the same curve.
    BezierSplit,
}

impl Interp {
    /// How far (0–1) the value has gone from a key to the next at `t`
    /// (0–1) of the way between them.
    pub fn eval(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Interp::Linear => t,
            Interp::Fast => 1.0 - (1.0 - t) * (1.0 - t),
            Interp::Slow => t * t,
            Interp::Smooth => t * t * (3.0 - 2.0 * t),
            // The smoothstep's inverse, exact at the ends.
            Interp::Sharp if t <= 0.0 || t >= 1.0 => t,
            Interp::Sharp => 0.5 - ((1.0 - 2.0 * t).asin() / 3.0).sin(),
            Interp::Hold => 0.0,
            // Without handles, a straight line.
            Interp::Bezier | Interp::BezierSplit => t,
        }
    }

    pub fn is_bezier(self) -> bool {
        matches!(self, Interp::Bezier | Interp::BezierSplit)
    }

    /// The slopes (in parts of the value's change over the time between
    /// the keys) at which the value leaves a key and reaches the next:
    /// how a key's curve meets a Bezier key's. Sharp's are steep, not
    /// upright.
    pub fn slopes(self) -> (f64, f64) {
        match self {
            Interp::Fast => (2.0, 0.0),
            Interp::Slow => (0.0, 2.0),
            Interp::Smooth | Interp::Hold => (0.0, 0.0),
            Interp::Sharp => (3.0, 3.0),
            Interp::Linear | Interp::Bezier | Interp::BezierSplit => (1.0, 1.0),
        }
    }
}

/// A Bezier key's handles for one of its numbers: where each is from the
/// key, `[frames, value]` -- `before` toward the key before (its time at
/// most 0), `after` toward the next (at least 0).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Handles {
    pub before: [f64; 2],
    pub after: [f64; 2],
}

/// A key: the value at `time` (frames, the effect's time).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Key {
    pub time: f64,
    pub value: Value,
    /// How the value goes from this key to the next.
    #[serde(default)]
    pub interp: Interp,
    /// A Bezier key's handles, one for each number (none: they follow the
    /// keys around it, `Animation::handles`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub handles: Vec<Handles>,
}

/// A parameter's value over time: `value` while it has no keys; from its
/// keys otherwise.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Animation {
    pub value: Value,
    /// Sorted by time, no two at the same time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<Key>,
}

/// Keys closer than this (frames) are at the same time.
pub const SAME_TIME: f64 = 1e-6;

impl Animation {
    pub fn constant(value: Value) -> Animation {
        Animation {
            value,
            keys: Vec::new(),
        }
    }

    pub fn is_animated(&self) -> bool {
        !self.keys.is_empty()
    }

    /// The value at `time` (frames). Numbers of whole types are rounded;
    /// text, booleans and choices go in steps.
    pub fn at(&self, time: f64, ty: ParamType) -> Value {
        let (Some(first), Some(last)) = (self.keys.first(), self.keys.last()) else {
            return self.value.clone();
        };
        if time <= first.time {
            return first.value.clone();
        }
        if time >= last.time {
            return last.value.clone();
        }
        let i = self.keys.partition_point(|k| k.time <= time);
        let (a, b) = (&self.keys[i - 1], &self.keys[i]);
        if ty.steps() || a.interp == Interp::Hold {
            return a.value.clone();
        }
        let span = (b.time - a.time).max(SAME_TIME);
        let k = a.interp.eval((time - a.time) / span);
        match (&a.value, &b.value) {
            // A Bezier key on either side: each number on its curve.
            (Value::Num(x), Value::Num(y)) if a.interp.is_bezier() || b.interp.is_bezier() => {
                Value::Num(
                    (0..x.len().min(y.len()))
                        .map(|d| {
                            let v = self.bezier_at(i - 1, d, time);
                            if ty.is_int() { v.round() } else { v }
                        })
                        .collect(),
                )
            }
            (Value::Num(x), Value::Num(y)) => Value::Num(
                x.iter()
                    .zip(y)
                    .map(|(x, y)| {
                        let v = x + (y - x) * k;
                        if ty.is_int() { v.round() } else { v }
                    })
                    .collect(),
            ),
            // Curves of as many points move from one key's to the next's.
            (Value::Curves(x), Value::Curves(y)) if x.len() == y.len() => x
                .iter()
                .zip(y)
                .map(|(p, q)| bv_core::curve::mix(p, q, k))
                .collect::<Option<Vec<_>>>()
                .map_or_else(|| a.value.clone(), Value::Curves),
            _ => a.value.clone(),
        }
    }

    /// Sets the value at `time`: a key there (replacing one at the same
    /// time) keeping `interp` for a new key.
    pub fn set_key(&mut self, time: f64, value: Value, interp: Interp) {
        match self
            .keys
            .iter_mut()
            .find(|k| (k.time - time).abs() < SAME_TIME)
        {
            Some(k) => k.value = value,
            None => {
                let i = self.keys.partition_point(|k| k.time < time);
                self.keys.insert(
                    i,
                    Key {
                        time,
                        value,
                        interp,
                        handles: Vec::new(),
                    },
                );
            }
        }
    }

    /// Key `i`'s handles for number `d`: its own, or (none set) ones that
    /// pass it smoothly -- the slope from the key before to the key after,
    /// a third of the way to each.
    pub fn handles(&self, i: usize, d: usize) -> Handles {
        let k = &self.keys[i];
        if let Some(h) = k.handles.get(d) {
            return *h;
        }
        let prev = i.checked_sub(1).map(|j| &self.keys[j]);
        let next = self.keys.get(i + 1);
        let slope =
            |a: &Key, b: &Key| (b.value.num(d) - a.value.num(d)) / (b.time - a.time).max(SAME_TIME);
        // The ends as long as their other side; a lone key's flat.
        let (before, after, m) = match (prev, next) {
            (Some(p), Some(n)) => (k.time - p.time, n.time - k.time, slope(p, n)),
            (Some(p), None) => (k.time - p.time, k.time - p.time, slope(p, k)),
            (None, Some(n)) => (n.time - k.time, n.time - k.time, slope(k, n)),
            (None, None) => (30.0, 30.0, 0.0),
        };
        let (before, after) = (before / 3.0, after / 3.0);
        Handles {
            before: [-before, -before * m],
            after: [after, after * m],
        }
    }

    /// Number `d` at `time` between key `i` and the next, on a cubic
    /// Bezier curve over time and value: from each key along its handle
    /// where it is a Bezier key, otherwise a third of the way along the
    /// slope its curve gives it there.
    fn bezier_at(&self, i: usize, d: usize, time: f64) -> f64 {
        let (a, b) = (&self.keys[i], &self.keys[i + 1]);
        let span = (b.time - a.time).max(SAME_TIME);
        let (y0, y3) = (a.value.num(d), b.value.num(d));
        let (s0, s1) = a.interp.slopes();
        let out = if a.interp.is_bezier() {
            self.handles(i, d).after
        } else {
            [span / 3.0, s0 * (y3 - y0) / 3.0]
        };
        let into = if b.interp.is_bezier() {
            self.handles(i + 1, d).before
        } else {
            [-span / 3.0, -s1 * (y3 - y0) / 3.0]
        };
        // Handles that point the right way in time and reach no further
        // than the other key (their slopes kept): the curve never turns
        // back in time.
        let fit = |[t, v]: [f64; 2]| {
            if t.abs() > span {
                let f = span / t.abs();
                [t * f, v * f]
            } else {
                [t, v]
            }
        };
        let out = fit([out[0].max(0.0), out[1]]);
        let into = fit([into[0].min(0.0), into[1]]);
        let x = [a.time, a.time + out[0], b.time + into[0], b.time];
        let y = [y0, y0 + out[1], y3 + into[1], y3];
        let cubic = |p: &[f64; 4], s: f64| {
            let r = 1.0 - s;
            r * r * r * p[0] + 3.0 * r * r * s * p[1] + 3.0 * r * s * s * p[2] + s * s * s * p[3]
        };
        // Where on the curve `time` is: its time only grows along it.
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..40 {
            let s = 0.5 * (lo + hi);
            if cubic(&x, s) < time {
                lo = s;
            } else {
                hi = s;
            }
        }
        cubic(&y, 0.5 * (lo + hi))
    }

    /// Removes the key at `time`; whether there was one.
    pub fn delete_key(&mut self, time: f64) -> bool {
        let before = self.keys.len();
        self.keys.retain(|k| (k.time - time).abs() >= SAME_TIME);
        self.keys.len() != before
    }

    /// The derivative of a numeric value at `time`, per number.
    pub fn derivative(&self, time: f64, ty: ParamType) -> Vec<f64> {
        const H: f64 = 1e-3;
        let (a, b) = (self.at(time - H, ty), self.at(time + H, ty));
        (0..ty.dimension())
            .map(|i| (b.num(i) - a.num(i)) / (2.0 * H))
            .collect()
    }

    /// The integral of a numeric value from `t1` to `t2`, per number
    /// (Simpson's rule, exact for constants and lines).
    pub fn integral(&self, t1: f64, t2: f64, ty: ParamType) -> Vec<f64> {
        let n = (((t2 - t1).abs() * 8.0).ceil() as usize).clamp(2, 8192) & !1;
        let h = (t2 - t1) / n as f64;
        (0..ty.dimension())
            .map(|d| {
                let f = |i: usize| self.at(t1 + h * i as f64, ty).num(d);
                let mut sum = f(0) + f(n);
                for i in 1..n {
                    sum += f(i) * if i % 2 == 1 { 4.0 } else { 2.0 };
                }
                sum * h / 3.0
            })
            .collect()
    }
}

pub use bv_core::curve::curve_at;

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(points: &[(f64, f64, Interp)]) -> Animation {
        let mut a = Animation::constant(Value::Num(vec![0.0]));
        for &(t, v, i) in points {
            a.set_key(t, Value::Num(vec![v]), i);
        }
        a
    }

    #[test]
    fn values_between_keys() {
        let a = keys(&[(0.0, 0.0, Interp::Linear), (10.0, 100.0, Interp::Linear)]);
        assert_eq!(a.at(-5.0, ParamType::Double).num(0), 0.0);
        assert_eq!(a.at(2.5, ParamType::Double).num(0), 25.0);
        assert_eq!(a.at(20.0, ParamType::Double).num(0), 100.0);
        // Whole numbers round; steps hold.
        assert_eq!(a.at(2.6, ParamType::Integer).num(0), 26.0);
        assert_eq!(a.at(9.0, ParamType::Choice).num(0), 0.0);
        let held = keys(&[(0.0, 0.0, Interp::Hold), (10.0, 100.0, Interp::Linear)]);
        assert_eq!(held.at(9.9, ParamType::Double).num(0), 0.0);
        let smooth = keys(&[(0.0, 0.0, Interp::Smooth), (10.0, 100.0, Interp::Linear)]);
        assert!((smooth.at(5.0, ParamType::Double).num(0) - 50.0).abs() < 1e-9);
        assert!(smooth.at(2.0, ParamType::Double).num(0) < 20.0);
    }

    #[test]
    fn bezier_keys_follow_their_handles() {
        let mut a = keys(&[
            (0.0, 0.0, Interp::Bezier),
            (10.0, 10.0, Interp::Bezier),
            (20.0, 0.0, Interp::Linear),
        ]);
        let at = |a: &Animation, t: f64| a.at(t, ParamType::Double).num(0);
        // Without handles of their own they pass the keys smoothly: the
        // middle one flat (its neighbours level), a peak there.
        assert!((at(&a, 10.0) - 10.0).abs() < 1e-9);
        assert!(at(&a, 9.0) < 10.0 && at(&a, 11.0) < 10.0);
        assert!((at(&a, 9.0) - at(&a, 11.0)).abs() < 1e-6);
        // Flat handles a third of the way: as Smooth.
        a.keys[0].handles = vec![Handles {
            before: [-1.0, 0.0],
            after: [10.0 / 3.0, 0.0],
        }];
        a.keys[1].handles = vec![Handles {
            before: [-10.0 / 3.0, 0.0],
            after: [10.0 / 3.0, 0.0],
        }];
        for t in [1.0, 2.5, 5.0, 7.5, 9.0] {
            let f = t / 10.0;
            let smooth = 10.0 * f * f * (3.0 - 2.0 * f);
            assert!((at(&a, t) - smooth).abs() < 1e-6, "{t}");
        }
        // A handle reaching past the next key: shortened, its slope kept;
        // the value still grows with time.
        a.keys[0].handles[0].after = [40.0, 40.0];
        let mut last = at(&a, 0.0);
        for k in 1..=100 {
            let v = at(&a, k as f64 / 10.0);
            assert!(v >= last - 1e-9 && v.is_finite(), "{k}");
            last = v;
        }
        // The segment after a Bezier key into a Linear one, and a Linear
        // one into it: straight where both are lines.
        let mut lines = keys(&[(0.0, 0.0, Interp::Linear), (10.0, 10.0, Interp::Bezier)]);
        lines.keys[1].handles = vec![Handles {
            before: [-2.0, -2.0],
            after: [2.0, 2.0],
        }];
        assert!((at(&lines, 5.0) - 5.0).abs() < 1e-6);
        // Whole numbers round.
        assert_eq!(a.at(3.3, ParamType::Integer).num(0), at(&a, 3.3).round());
    }

    #[test]
    fn keys_replace_and_delete() {
        let mut a = keys(&[(5.0, 1.0, Interp::Linear), (1.0, 2.0, Interp::Linear)]);
        assert_eq!(
            a.keys.iter().map(|k| k.time).collect::<Vec<_>>(),
            [1.0, 5.0]
        );
        a.set_key(5.0, Value::Num(vec![9.0]), Interp::Linear);
        assert_eq!(a.keys.len(), 2);
        assert!(a.delete_key(1.0));
        assert!(!a.delete_key(1.0));
    }

    #[test]
    fn derivative_and_integral() {
        let a = keys(&[(0.0, 0.0, Interp::Linear), (10.0, 20.0, Interp::Linear)]);
        assert!((a.derivative(5.0, ParamType::Double)[0] - 2.0).abs() < 1e-6);
        // The area under 2t from 0 to 10.
        assert!((a.integral(0.0, 10.0, ParamType::Double)[0] - 100.0).abs() < 1e-6);
    }

    #[test]
    fn curves_pass_through_their_points_without_overshooting() {
        let pts = [
            CurvePoint::new(0.0, 0.0),
            CurvePoint::new(0.5, 0.8),
            CurvePoint::new(1.0, 1.0),
        ];
        assert_eq!(curve_at(&pts, 0.5), 0.8);
        for i in 0..=100 {
            let v = curve_at(&pts, i as f64 / 100.0);
            assert!((0.0..=1.0).contains(&v), "{v}");
        }
        assert_eq!(curve_at(&pts, 2.0), 1.0);
    }

    #[test]
    fn curves_move_from_key_to_key() {
        let curve = |y: f64| {
            Value::Curves(vec![vec![
                CurvePoint::new(0.0, 0.0),
                CurvePoint::new(1.0, y),
            ]])
        };
        let mut a = Animation::constant(curve(1.0));
        a.set_key(0.0, curve(1.0), Interp::Linear);
        a.set_key(10.0, curve(0.5), Interp::Linear);
        let Value::Curves(c) = a.at(5.0, ParamType::Parametric) else {
            panic!("curves");
        };
        assert!((c[0][1].y - 0.75).abs() < 1e-9);
        // Of different points, they hold until the next key.
        a.set_key(
            10.0,
            Value::Curves(vec![bv_core::curve::IDENTITY[..1].to_vec()]),
            Interp::Linear,
        );
        assert_eq!(a.at(5.0, ParamType::Parametric), curve(1.0));
    }
}
