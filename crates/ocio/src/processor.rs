//! Processors: a configuration's conversions (color space to color space,
//! or to a display through one of its views, looks on the way) made into
//! steps. They are worked out exactly, never baked into a lattice: near
//! AP1's edge, where ACES's output clips, a 65³ lattice misses by up to
//! half the range (and by 1⁄120 for soft colors).

use crate::builtins;
use crate::config::{ColorSpace, Config, Error, Kind, Reference, Result, Transform};
use crate::luts;
use crate::math::{self, M3};
use crate::ops::{Camera, Negatives, Op};

/// A conversion: its steps, in order.
#[derive(Clone, Debug, Default)]
pub struct Processor {
    ops: Vec<Op>,
}

impl Processor {
    pub fn ops(&self) -> &[Op] {
        &self.ops
    }

    pub fn is_identity(&self) -> bool {
        self.ops.is_empty()
    }

    pub fn apply(&self, c: [f64; 3]) -> [f64; 3] {
        self.ops.iter().fold(c, |c, op| op.apply(c))
    }

    pub fn apply_f32(&self, c: [f32; 3]) -> [f32; 3] {
        self.apply(c.map(f64::from)).map(|v| v as f32)
    }

    /// When every step is a matrix: them as one.
    pub fn matrix(&self) -> Option<(M3, [f64; 3])> {
        let mut m = math::IDENTITY;
        let mut o = [0.0; 3];
        for op in &self.ops {
            let Op::Matrix(a, b) = op else { return None };
            m = math::mul(a, &m);
            let ao = math::apply(a, o);
            o = [ao[0] + b[0], ao[1] + b[1], ao[2] + b[2]];
        }
        Some((m, o))
    }

    /// This, then `next`.
    pub fn then(mut self, next: Processor) -> Processor {
        self.ops.extend(next.ops);
        self
    }

    /// The conversion undone, when every step can be.
    pub fn inverse(&self) -> Option<Processor> {
        let ops = self
            .ops
            .iter()
            .rev()
            .map(Op::inverse)
            .collect::<Option<Vec<_>>>()?;
        Some(Processor { ops })
    }
}

/// Where a conversion's colors are, as it is built.
struct Builder<'a> {
    config: &'a Config,
    ops: Vec<Op>,
    depth: usize,
}

impl Config {
    /// From color space `src` to `dst` (names, aliases or roles).
    pub fn processor(&self, src: &str, dst: &str) -> Result<Processor> {
        let mut b = Builder::new(self);
        b.colorspaces(src, dst)?;
        Ok(b.finish())
    }

    /// From color space `src` to display `display` through its view
    /// `view`; `looks` (a list like "+a, -b"), when given, instead of the
    /// view's own.
    pub fn display_processor(
        &self,
        src: &str,
        display: &str,
        view: &str,
        looks: Option<&str>,
    ) -> Result<Processor> {
        let mut b = Builder::new(self);
        b.display_view(src, display, view, looks)?;
        Ok(b.finish())
    }

    /// A transform on its own (its references resolved here).
    pub fn transform_processor(&self, t: &Transform) -> Result<Processor> {
        let mut b = Builder::new(self);
        b.transform(t, false)?;
        Ok(b.finish())
    }

    fn space(&self, name: &str) -> Result<&ColorSpace> {
        self.colorspace(name)
            .ok_or_else(|| Error::Unknown(format!("color space {name}")))
    }
}

impl<'a> Builder<'a> {
    fn new(config: &'a Config) -> Builder<'a> {
        Builder {
            config,
            ops: Vec::new(),
            depth: 0,
        }
    }

    fn finish(self) -> Processor {
        Processor {
            ops: optimize(self.ops),
        }
    }

    /// `t`'s steps (undone when `inverse`, its own direction counted).
    fn transform(&mut self, t: &Transform, inverse: bool) -> Result<()> {
        self.depth += 1;
        if self.depth > 64 {
            return Err(Error::Invalid(
                "transforms refer to each other in a loop".into(),
            ));
        }
        let inv = t.inverse != inverse;
        let r = self.kind(&t.kind, inv);
        self.depth -= 1;
        r
    }

    fn push(&mut self, ops: Vec<Op>, inverse: bool, what: &str) -> Result<()> {
        if inverse {
            for op in ops.iter().rev() {
                let i = op
                    .inverse()
                    .ok_or_else(|| Error::Unsupported(format!("{what} backwards")))?;
                self.ops.push(i);
            }
        } else {
            self.ops.extend(ops);
        }
        Ok(())
    }

    fn kind(&mut self, k: &Kind, inv: bool) -> Result<()> {
        match k {
            Kind::Group(children) => {
                if inv {
                    for c in children.iter().rev() {
                        self.transform(c, true)?;
                    }
                } else {
                    for c in children {
                        self.transform(c, false)?;
                    }
                }
                Ok(())
            }
            Kind::Matrix { matrix, offset } => {
                let (m, _) = math::of_rows16(matrix);
                let op = Op::Matrix(m, [offset[0], offset[1], offset[2]]);
                self.push(vec![op], inv, "a matrix")
            }
            Kind::Exponent { value, style } => {
                let neg = if style.contains("Mirror") || style.contains("mirror") {
                    Negatives::Mirror
                } else if style.contains("Pass") || style.contains("pass") {
                    Negatives::PassThrough
                } else {
                    Negatives::Clamp
                };
                let rev = style.ends_with("Rev");
                let op = Op::Power([value[0], value[1], value[2]], neg);
                self.push(vec![op], inv != rev, "an exponent")
            }
            Kind::ExponentWithLinear {
                gamma,
                offset,
                mirror: _,
            } => {
                if gamma[0] != gamma[1]
                    || gamma[1] != gamma[2]
                    || offset[0] != offset[1]
                    || offset[1] != offset[2]
                {
                    return Err(Error::Unsupported(
                        "an ExponentWithLinearTransform different per channel".into(),
                    ));
                }
                let op = Op::Curve(
                    crate::curves::Curve::MonCurve {
                        gamma: gamma[0],
                        offset: offset[0],
                    },
                    true,
                );
                self.push(vec![op], inv, "an exponent")
            }
            Kind::Log { base } => {
                let op = Op::Log {
                    base: *base,
                    inverse: false,
                };
                self.push(vec![op], inv, "a log")
            }
            Kind::LogAffine {
                base,
                log_side_slope,
                log_side_offset,
                lin_side_slope,
                lin_side_offset,
                lin_side_break,
                linear_slope,
            } => {
                let op = Op::LogAffine {
                    base: *base,
                    log_slope: *log_side_slope,
                    log_offset: *log_side_offset,
                    lin_slope: *lin_side_slope,
                    lin_offset: *lin_side_offset,
                    camera: lin_side_break.map(|lin_break| Camera {
                        lin_break,
                        linear_slope: *linear_slope,
                    }),
                    inverse: false,
                };
                self.push(vec![op], inv, "a log")
            }
            Kind::Cdl {
                slope,
                offset,
                power,
                sat,
                clamp,
            } => {
                let op = Op::Cdl {
                    slope: *slope,
                    offset: *offset,
                    power: *power,
                    sat: *sat,
                    clamp: *clamp,
                    inverse: false,
                };
                self.push(vec![op], inv, "a CDL")
            }
            Kind::Range {
                min_in,
                max_in,
                min_out,
                max_out,
                clamp,
            } => {
                let (scale, offset) = match (min_in, max_in, min_out, max_out) {
                    (Some(a), Some(b), Some(c), Some(d)) if b != a => {
                        let s = (d - c) / (b - a);
                        (s, c - s * a)
                    }
                    (Some(a), _, Some(c), _) => (1.0, c - a),
                    (_, Some(b), _, Some(d)) => (1.0, d - b),
                    _ => (1.0, 0.0),
                };
                let op = Op::Range {
                    scale,
                    offset,
                    lo: if *clamp { *min_out } else { None },
                    hi: if *clamp { *max_out } else { None },
                };
                self.push(vec![op], inv, "a range")
            }
            Kind::File { src, interpolation } => {
                let path = self.config.resolve(src)?;
                let ops = luts::load(&path, interpolation)?;
                self.push(ops, inv, src)
            }
            Kind::Builtin { style } => {
                let ops = builtins::ops(style)
                    .ok_or_else(|| Error::Unsupported(format!("builtin {style}")))?;
                self.push(ops, inv, style)
            }
            Kind::FixedFunction { style, params } => {
                let op = match style.as_str() {
                    "ACES_GamutComp13" | "ACES_GamutComp13_Fwd" | "GamutComp13" => {
                        let defaults = [1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2];
                        if !params.is_empty()
                            && params
                                .iter()
                                .zip(defaults)
                                .any(|(a, b)| (a - b).abs() > 1e-6)
                        {
                            return Err(Error::Unsupported(
                                "gamut compression with other parameters".into(),
                            ));
                        }
                        Op::GamutCompress(false)
                    }
                    "RGB_TO_HSV" => Op::RgbToHsv(false),
                    "HSV_TO_RGB" => Op::RgbToHsv(true),
                    "XYZ_TO_xyY" => Op::XyzToXyy(false),
                    "xyY_TO_XYZ" => Op::XyzToXyy(true),
                    other => return Err(Error::Unsupported(format!("fixed function {other}"))),
                };
                self.push(vec![op], inv, style)
            }
            Kind::ColorSpace { src, dst } => {
                if inv {
                    self.colorspaces(dst, src)
                } else {
                    self.colorspaces(src, dst)
                }
            }
            Kind::DisplayView {
                src,
                display,
                view,
                looks_bypass,
            } => {
                let looks = looks_bypass.then_some("");
                if !inv {
                    return self.display_view(src, display, view, looks);
                }
                let mut sub = Builder::new(self.config);
                sub.depth = self.depth;
                sub.display_view(src, display, view, looks)?;
                let ops = sub.ops;
                self.push(ops, true, "a display's view")
            }
            Kind::Look { src, dst, looks } => {
                let mut sub = Builder::new(self.config);
                sub.depth = self.depth;
                let at = sub.looks(src, looks)?;
                sub.colorspaces(&at, dst)?;
                let ops = sub.ops;
                self.push(ops, inv, "looks")
            }
            Kind::ExposureContrast {
                exposure,
                contrast,
                gamma: _,
                pivot,
                style,
            } => {
                if style.starts_with("video") || style.starts_with("log") {
                    return Err(Error::Unsupported(format!(
                        "exposure and contrast, {style}"
                    )));
                }
                // Linear: (c · 2^exposure ⁄ pivot)^contrast · pivot.
                let gain = 2f64.powf(*exposure) / pivot;
                let ops = vec![
                    Op::Matrix(
                        [[gain, 0.0, 0.0], [0.0, gain, 0.0], [0.0, 0.0, gain]],
                        [0.0; 3],
                    ),
                    Op::Power([*contrast; 3], Negatives::PassThrough),
                    Op::Matrix(
                        [[*pivot, 0.0, 0.0], [0.0, *pivot, 0.0], [0.0, 0.0, *pivot]],
                        [0.0; 3],
                    ),
                ];
                self.push(ops, inv, "exposure and contrast")
            }
            Kind::Allocation => Ok(()),
        }
    }

    /// Into `cs`'s reference.
    fn enter_reference_of(&mut self, cs: &ColorSpace) -> Result<()> {
        if let Some(t) = &cs.to_reference {
            self.transform(t, false)
        } else if let Some(t) = &cs.from_reference {
            self.transform(t, true)
        } else {
            Ok(())
        }
    }

    /// From the reference into `cs`.
    fn leave_reference_into(&mut self, cs: &ColorSpace) -> Result<()> {
        if let Some(t) = &cs.from_reference {
            self.transform(t, false)
        } else if let Some(t) = &cs.to_reference {
            self.transform(t, true)
        } else {
            Ok(())
        }
    }

    /// The scene reference into the display reference (`back`: the other
    /// way) through the configuration's default view transform.
    fn default_view(&mut self, back: bool) -> Result<()> {
        let c = self.config;
        let vt = c
            .default_view_transform
            .as_deref()
            .and_then(|n| c.view_transform(n))
            .or_else(|| {
                c.view_transforms
                    .iter()
                    .find(|v| v.reference == Reference::Scene)
            })
            .ok_or_else(|| {
                Error::Invalid("no view transform between the scene and the display".into())
            })?
            .clone();
        let (fwd, rev) = (vt.from_reference.as_ref(), vt.to_reference.as_ref());
        match (back, fwd, rev) {
            (false, Some(t), _) => self.transform(t, false),
            (false, None, Some(t)) => self.transform(t, true),
            (true, _, Some(t)) => self.transform(t, false),
            (true, Some(t), None) => self.transform(t, true),
            _ => Ok(()),
        }
    }

    fn colorspaces(&mut self, src: &str, dst: &str) -> Result<()> {
        let c = self.config;
        let s = c.space(src)?.clone();
        let d = c.space(dst)?.clone();
        if s.name == d.name || s.is_data || d.is_data {
            return Ok(());
        }
        self.enter_reference_of(&s)?;
        match (s.reference, d.reference) {
            (Reference::Scene, Reference::Display) => self.default_view(false)?,
            (Reference::Display, Reference::Scene) => self.default_view(true)?,
            _ => {}
        }
        self.leave_reference_into(&d)
    }

    /// Looks `list` ("a, +b, -c") from color space `src`: where the colors
    /// end (the last look's process space).
    fn looks(&mut self, src: &str, list: &str) -> Result<String> {
        let mut at = src.to_string();
        for item in list
            .split([',', ':'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let (name, backwards) = if let Some(n) = item.strip_prefix('-') {
                (n.trim(), true)
            } else {
                (item.trim_start_matches('+').trim(), false)
            };
            let look = self
                .config
                .look(name)
                .ok_or_else(|| Error::Unknown(format!("look {name}")))?
                .clone();
            let process = if look.process_space.is_empty() {
                at.clone()
            } else {
                look.process_space.clone()
            };
            self.colorspaces(&at, &process)?;
            match (backwards, &look.transform, &look.inverse_transform) {
                (false, Some(t), _) => self.transform(t, false)?,
                (false, None, Some(t)) => self.transform(t, true)?,
                (true, _, Some(t)) => self.transform(t, false)?,
                (true, Some(t), None) => self.transform(t, true)?,
                _ => {}
            }
            at = process;
        }
        Ok(at)
    }

    fn display_view(
        &mut self,
        src: &str,
        display: &str,
        view: &str,
        looks: Option<&str>,
    ) -> Result<()> {
        let c = self.config;
        let d = c
            .display(display)
            .ok_or_else(|| Error::Unknown(format!("display {display}")))?;
        let v = d
            .views
            .iter()
            .find(|v| v.name == view)
            .ok_or_else(|| Error::Unknown(format!("view {view} of {display}")))?
            .clone();
        let s = c.space(src)?;
        if s.is_data {
            return Ok(());
        }
        let list = looks.map_or(v.looks.clone(), str::to_string);
        let at = self.looks(src, &list)?;
        let Some(vt_name) = &v.view_transform else {
            // Version 1: the view is a color space.
            return self.colorspaces(&at, &v.colorspace);
        };
        let vt = c
            .view_transform(vt_name)
            .ok_or_else(|| Error::Unknown(format!("view transform {vt_name}")))?
            .clone();
        let target = c.space(&v.colorspace)?.clone();
        let here = c.space(&at)?.clone();
        // Into the view transform's reference…
        match vt.reference {
            Reference::Scene => {
                self.enter_reference_of(&here)?;
                if here.reference == Reference::Display {
                    self.default_view(true)?;
                }
            }
            Reference::Display => {
                self.enter_reference_of(&here)?;
                if here.reference == Reference::Scene {
                    self.default_view(false)?;
                }
            }
        }
        // …through it to the display reference…
        match (&vt.from_reference, &vt.to_reference) {
            (Some(t), _) => self.transform(t, false)?,
            (None, Some(t)) => self.transform(t, true)?,
            _ => {}
        }
        // …and into the display's color space.
        if target.reference == Reference::Scene {
            self.default_view(true)?;
        }
        self.leave_reference_into(&target)
    }
}

/// Neighbouring matrices made one.
fn optimize(ops: Vec<Op>) -> Vec<Op> {
    let mut out: Vec<Op> = Vec::with_capacity(ops.len());
    for op in ops {
        if let (Op::Matrix(b, bo), Some(Op::Matrix(a, ao))) = (&op, out.last()) {
            let m = math::mul(b, a);
            let t = math::apply(b, *ao);
            let o = [t[0] + bo[0], t[1] + bo[1], t[2] + bo[2]];
            out.pop();
            out.push(Op::Matrix(m, o));
            continue;
        }
        out.push(op);
    }
    // A matrix that does nothing is no step.
    out.retain(|op| {
        !matches!(op, Op::Matrix(m, o)
            if (0..3).all(|i| (0..3).all(|j| (m[i][j] - if i == j { 1.0 } else { 0.0 }).abs() < 1e-12))
                && o.iter().all(|v| v.abs() < 1e-12))
    });
    out
}
