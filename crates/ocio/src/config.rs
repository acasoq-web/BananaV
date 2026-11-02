//! An OpenColorIO configuration (profile versions 1 and 2): its color
//! spaces (scene- and display-referred), roles, displays and their views,
//! view transforms, looks and named transforms, read from its YAML.

use std::path::PathBuf;

use crate::yaml::{self, Node};

/// Why a configuration could not be read or used.
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    Yaml(yaml::Error),
    /// Something it says is not what OpenColorIO reads (the key and why).
    Invalid(String),
    /// A name it does not have (a color space, display, view or look).
    Unknown(String),
    /// A transform BananaV does not do (yet).
    Unsupported(String),
    /// A file a transform reads.
    File(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Yaml(e) => write!(f, "{e}"),
            Error::Invalid(s) => write!(f, "invalid: {s}"),
            Error::Unknown(s) => write!(f, "unknown: {s}"),
            Error::Unsupported(s) => write!(f, "not supported: {s}"),
            Error::File(s) => write!(f, "file: {s}"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// A transform as a configuration writes it.
#[derive(Clone, Debug, PartialEq)]
pub struct Transform {
    pub kind: Kind,
    /// Written `direction: inverse`.
    pub inverse: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Group(Vec<Transform>),
    /// Rows of the 4 × 4 matrix, then the offset (RGBA).
    Matrix {
        matrix: [f64; 16],
        offset: [f64; 4],
    },
    /// `value` per channel; `style`: basicFwd, basicMirrorFwd, …
    Exponent {
        value: [f64; 4],
        style: String,
    },
    ExponentWithLinear {
        gamma: [f64; 4],
        offset: [f64; 4],
        mirror: bool,
    },
    Log {
        base: f64,
    },
    LogAffine {
        base: f64,
        log_side_slope: [f64; 3],
        log_side_offset: [f64; 3],
        lin_side_slope: [f64; 3],
        lin_side_offset: [f64; 3],
        /// LogCameraTransform's.
        lin_side_break: Option<[f64; 3]>,
        linear_slope: Option<[f64; 3]>,
    },
    Cdl {
        slope: [f64; 3],
        offset: [f64; 3],
        power: [f64; 3],
        sat: f64,
        clamp: bool,
    },
    Range {
        min_in: Option<f64>,
        max_in: Option<f64>,
        min_out: Option<f64>,
        max_out: Option<f64>,
        clamp: bool,
    },
    File {
        src: String,
        interpolation: String,
    },
    Builtin {
        style: String,
    },
    FixedFunction {
        style: String,
        params: Vec<f64>,
    },
    ColorSpace {
        src: String,
        dst: String,
    },
    DisplayView {
        src: String,
        display: String,
        view: String,
        looks_bypass: bool,
    },
    Look {
        src: String,
        dst: String,
        looks: String,
    },
    ExposureContrast {
        exposure: f64,
        contrast: f64,
        gamma: f64,
        pivot: f64,
        style: String,
    },
    /// Version 1's allocation (how GPUs should sample): nothing to colors.
    Allocation,
}

/// Which reference a color space is described against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reference {
    Scene,
    Display,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColorSpace {
    pub name: String,
    pub aliases: Vec<String>,
    pub family: String,
    pub description: String,
    pub categories: Vec<String>,
    pub encoding: String,
    pub is_data: bool,
    pub reference: Reference,
    /// To the reference (`to_scene_reference`, `to_display_reference`,
    /// v1's `to_reference`) and from it.
    pub to_reference: Option<Transform>,
    pub from_reference: Option<Transform>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub name: String,
    /// Version 2's: a view transform and the display color space.
    pub view_transform: Option<String>,
    /// The display color space (v2) or the color space (v1).
    pub colorspace: String,
    pub looks: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Display {
    pub name: String,
    pub views: Vec<View>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ViewTransform {
    pub name: String,
    pub description: String,
    pub reference: Reference,
    pub to_reference: Option<Transform>,
    pub from_reference: Option<Transform>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub name: String,
    pub process_space: String,
    pub description: String,
    pub transform: Option<Transform>,
    pub inverse_transform: Option<Transform>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NamedTransform {
    pub name: String,
    pub aliases: Vec<String>,
    pub transform: Option<Transform>,
    pub inverse_transform: Option<Transform>,
}

/// A configuration.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub version: f64,
    pub name: String,
    pub description: String,
    /// Where its files' paths start: the configuration's folder.
    pub working_dir: Option<PathBuf>,
    pub search_path: Vec<String>,
    pub environment: Vec<(String, String)>,
    pub luma: [f64; 3],
    pub roles: Vec<(String, String)>,
    pub colorspaces: Vec<ColorSpace>,
    pub displays: Vec<Display>,
    pub active_displays: Vec<String>,
    pub active_views: Vec<String>,
    pub inactive_colorspaces: Vec<String>,
    pub view_transforms: Vec<ViewTransform>,
    pub default_view_transform: Option<String>,
    pub looks: Vec<Look>,
    pub named_transforms: Vec<NamedTransform>,
}

fn invalid<T>(what: impl Into<String>) -> Result<T> {
    Err(Error::Invalid(what.into()))
}

fn text(n: Option<&Node>) -> String {
    n.and_then(Node::str).unwrap_or("").trim_end().to_string()
}

fn texts(n: Option<&Node>) -> Vec<String> {
    let v = n.and_then(Node::strs).unwrap_or_default();
    // A single scalar may list several, separated by commas.
    v.iter()
        .flat_map(|s| s.split(','))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn num(n: Option<&Node>, default: f64) -> Result<f64> {
    match n {
        None => Ok(default),
        Some(n) => n
            .f64()
            .ok_or_else(|| Error::Invalid(format!("not a number: {:?}", n.str()))),
    }
}

/// One number for every channel, or one each.
fn per_channel<const N: usize>(n: Option<&Node>, default: f64) -> Result<[f64; N]> {
    match n {
        None => Ok([default; N]),
        Some(n) => {
            if let Some(v) = n.f64() {
                return Ok([v; N]);
            }
            let v = n
                .f64s()
                .ok_or_else(|| Error::Invalid("expected numbers".into()))?;
            if v.len() >= N {
                Ok(std::array::from_fn(|i| v[i]))
            } else if v.len() == 3 && N == 4 {
                Ok(std::array::from_fn(|i| if i < 3 { v[i] } else { default }))
            } else {
                invalid(format!("expected {N} numbers, found {}", v.len()))
            }
        }
    }
}

/// A transform node.
pub fn transform(n: &Node) -> Result<Transform> {
    let tag = n.tag.as_deref().unwrap_or("");
    let inverse = matches!(text(n.get("direction")).as_str(), "inverse");
    let kind = match tag {
        "GroupTransform" => {
            let children = n.get("children").and_then(Node::seq).unwrap_or(&[]);
            Kind::Group(children.iter().map(transform).collect::<Result<_>>()?)
        }
        "MatrixTransform" => {
            let mut matrix = [0.0; 16];
            for i in 0..4 {
                matrix[i * 5] = 1.0;
            }
            if let Some(m) = n.get("matrix") {
                let v = m.f64s().ok_or_else(|| Error::Invalid("matrix".into()))?;
                if v.len() != 16 {
                    return invalid("a matrix has 16 numbers");
                }
                matrix.copy_from_slice(&v);
            }
            Kind::Matrix {
                matrix,
                offset: per_channel(n.get("offset"), 0.0)?,
            }
        }
        "ExponentTransform" => Kind::Exponent {
            value: per_channel(n.get("value").or(n.get("gamma")), 1.0)?,
            style: n
                .get("style")
                .and_then(Node::str)
                .unwrap_or("basicFwd")
                .to_string(),
        },
        "ExponentWithLinearTransform" => Kind::ExponentWithLinear {
            gamma: per_channel(n.get("gamma"), 1.0)?,
            offset: per_channel(n.get("offset"), 0.0)?,
            mirror: text(n.get("style")) == "mirror",
        },
        "LogTransform" => Kind::Log {
            base: num(n.get("base"), 2.0)?,
        },
        "LogAffineTransform" | "LogCameraTransform" => Kind::LogAffine {
            base: num(n.get("base"), 2.0)?,
            log_side_slope: per_channel(n.get("log_side_slope"), 1.0)?,
            log_side_offset: per_channel(n.get("log_side_offset"), 0.0)?,
            lin_side_slope: per_channel(n.get("lin_side_slope"), 1.0)?,
            lin_side_offset: per_channel(n.get("lin_side_offset"), 0.0)?,
            lin_side_break: match n.get("lin_side_break") {
                Some(b) => Some(per_channel(Some(b), 0.0)?),
                None if tag == "LogCameraTransform" => {
                    return invalid("a LogCameraTransform needs lin_side_break");
                }
                None => None,
            },
            linear_slope: match n.get("linear_slope") {
                Some(s) => Some(per_channel(Some(s), 1.0)?),
                None => None,
            },
        },
        "CDLTransform" => Kind::Cdl {
            slope: per_channel(n.get("slope"), 1.0)?,
            offset: per_channel(n.get("offset"), 0.0)?,
            power: per_channel(n.get("power"), 1.0)?,
            sat: num(n.get("sat").or(n.get("saturation")), 1.0)?,
            clamp: !text(n.get("style")).starts_with("noClamp"),
        },
        "RangeTransform" => {
            let opt = |k: &str| -> Result<Option<f64>> {
                match n.get(k) {
                    Some(v) => num(Some(v), 0.0).map(Some),
                    None => Ok(None),
                }
            };
            Kind::Range {
                min_in: opt("min_in_value")?,
                max_in: opt("max_in_value")?,
                min_out: opt("min_out_value")?,
                max_out: opt("max_out_value")?,
                clamp: text(n.get("style")) != "noClamp",
            }
        }
        "FileTransform" => Kind::File {
            src: text(n.get("src")),
            interpolation: text(n.get("interpolation")),
        },
        "BuiltinTransform" => Kind::Builtin {
            style: text(n.get("style")),
        },
        "FixedFunctionTransform" => Kind::FixedFunction {
            style: text(n.get("style")),
            params: n.get("params").and_then(Node::f64s).unwrap_or_default(),
        },
        "ColorSpaceTransform" => Kind::ColorSpace {
            src: text(n.get("src")),
            dst: text(n.get("dst")),
        },
        "DisplayViewTransform" => Kind::DisplayView {
            src: text(n.get("src")),
            display: text(n.get("display")),
            view: text(n.get("view")),
            looks_bypass: n.get("looks_bypass").and_then(Node::bool).unwrap_or(false),
        },
        "LookTransform" => Kind::Look {
            src: text(n.get("src")),
            dst: text(n.get("dst")),
            looks: text(n.get("looks")),
        },
        "ExposureContrastTransform" => Kind::ExposureContrast {
            exposure: num(n.get("exposure"), 0.0)?,
            contrast: num(n.get("contrast"), 1.0)?,
            gamma: num(n.get("gamma"), 1.0)?,
            pivot: num(n.get("pivot"), 0.18)?,
            style: text(n.get("style")),
        },
        "AllocationTransform" => Kind::Allocation,
        other => return Err(Error::Unsupported(format!("transform {other}"))),
    };
    Ok(Transform { kind, inverse })
}

fn optional_transform(n: Option<&Node>) -> Result<Option<Transform>> {
    match n {
        Some(n) if !n.is_null() => transform(n).map(Some),
        _ => Ok(None),
    }
}

fn colorspace(n: &Node, display: bool) -> Result<ColorSpace> {
    let name = text(n.get("name"));
    if name.is_empty() {
        return invalid("a color space without a name");
    }
    let (to, from, reference) = if let Some(t) = n.get("to_display_reference") {
        (Some(t), n.get("from_display_reference"), Reference::Display)
    } else if n.get("from_display_reference").is_some() {
        (None, n.get("from_display_reference"), Reference::Display)
    } else if display {
        (
            n.get("to_display_reference"),
            n.get("from_display_reference"),
            Reference::Display,
        )
    } else {
        (
            n.get("to_scene_reference").or(n.get("to_reference")),
            n.get("from_scene_reference").or(n.get("from_reference")),
            Reference::Scene,
        )
    };
    Ok(ColorSpace {
        name,
        aliases: texts(n.get("aliases")),
        family: text(n.get("family")),
        description: text(n.get("description")),
        categories: texts(n.get("categories")),
        encoding: text(n.get("encoding")),
        is_data: n.get("isdata").and_then(Node::bool).unwrap_or(false),
        reference,
        to_reference: optional_transform(to)?,
        from_reference: optional_transform(from)?,
    })
}

fn view(n: &Node) -> View {
    View {
        name: text(n.get("name")),
        view_transform: n
            .get("view_transform")
            .and_then(Node::str)
            .map(str::to_string),
        colorspace: text(n.get("display_colorspace").or(n.get("colorspace"))),
        looks: text(n.get("looks").or(n.get("look"))),
        description: text(n.get("description")),
    }
}

impl Config {
    /// Reads a configuration's text; `working_dir`: its folder (files'
    /// paths start there).
    pub fn parse(source: &str, working_dir: Option<PathBuf>) -> Result<Config> {
        let doc = yaml::parse(source).map_err(Error::Yaml)?;
        if doc.map().is_none() {
            return invalid("a configuration is a mapping");
        }
        let version = num(doc.get("ocio_profile_version"), 1.0)?;
        let mut colorspaces = Vec::new();
        for n in doc.get("colorspaces").and_then(Node::seq).unwrap_or(&[]) {
            colorspaces.push(colorspace(n, false)?);
        }
        for n in doc
            .get("display_colorspaces")
            .and_then(Node::seq)
            .unwrap_or(&[])
        {
            colorspaces.push(colorspace(n, true)?);
        }
        let shared: Vec<View> = doc
            .get("shared_views")
            .and_then(Node::seq)
            .unwrap_or(&[])
            .iter()
            .map(view)
            .collect();
        let mut displays = Vec::new();
        for (name, list) in doc.get("displays").and_then(Node::map).unwrap_or(&[]) {
            let mut views = Vec::new();
            for v in list.seq().unwrap_or(&[]) {
                if v.tag.as_deref() == Some("Views") {
                    for wanted in texts(Some(v)) {
                        if let Some(s) = shared.iter().find(|s| s.name == wanted) {
                            let mut s = s.clone();
                            if s.colorspace == "<USE_DISPLAY_NAME>" {
                                s.colorspace = name.clone();
                            }
                            views.push(s);
                        }
                    }
                } else {
                    views.push(view(v));
                }
            }
            displays.push(Display {
                name: name.clone(),
                views,
            });
        }
        let mut view_transforms = Vec::new();
        for n in doc
            .get("view_transforms")
            .and_then(Node::seq)
            .unwrap_or(&[])
        {
            let (to, from, reference) = if n.get("to_display_reference").is_some()
                || n.get("from_display_reference").is_some()
            {
                (
                    n.get("to_display_reference"),
                    n.get("from_display_reference"),
                    Reference::Display,
                )
            } else {
                (
                    n.get("to_scene_reference").or(n.get("to_reference")),
                    n.get("from_scene_reference").or(n.get("from_reference")),
                    Reference::Scene,
                )
            };
            view_transforms.push(ViewTransform {
                name: text(n.get("name")),
                description: text(n.get("description")),
                reference,
                to_reference: optional_transform(to)?,
                from_reference: optional_transform(from)?,
            });
        }
        let mut looks = Vec::new();
        for n in doc.get("looks").and_then(Node::seq).unwrap_or(&[]) {
            looks.push(Look {
                name: text(n.get("name")),
                process_space: text(n.get("process_space")),
                description: text(n.get("description")),
                transform: optional_transform(n.get("transform"))?,
                inverse_transform: optional_transform(n.get("inverse_transform"))?,
            });
        }
        let mut named_transforms = Vec::new();
        for n in doc
            .get("named_transforms")
            .and_then(Node::seq)
            .unwrap_or(&[])
        {
            named_transforms.push(NamedTransform {
                name: text(n.get("name")),
                aliases: texts(n.get("aliases")),
                transform: optional_transform(n.get("transform").or(n.get("encoding")))?,
                inverse_transform: optional_transform(n.get("inverse_transform"))?,
            });
        }
        let roles = doc
            .get("roles")
            .and_then(Node::map)
            .unwrap_or(&[])
            .iter()
            .map(|(k, v)| (k.clone(), text(Some(v))))
            .collect();
        let environment = doc
            .get("environment")
            .and_then(Node::map)
            .unwrap_or(&[])
            .iter()
            .map(|(k, v)| (k.clone(), text(Some(v))))
            .collect();
        let search_path = match doc.get("search_path") {
            Some(n) if n.seq().is_some() => texts(Some(n)),
            Some(n) => text(Some(n))
                .split([':', ';'])
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
            None => Vec::new(),
        };
        let luma = match doc.get("luma").and_then(Node::f64s) {
            Some(v) if v.len() == 3 => [v[0], v[1], v[2]],
            _ => [0.2126, 0.7152, 0.0722],
        };
        Ok(Config {
            version,
            name: text(doc.get("name")),
            description: text(doc.get("description")),
            working_dir,
            search_path,
            environment,
            luma,
            roles,
            colorspaces,
            displays,
            active_displays: texts(doc.get("active_displays")),
            active_views: texts(doc.get("active_views")),
            inactive_colorspaces: texts(doc.get("inactive_colorspaces")),
            view_transforms,
            default_view_transform: doc
                .get("default_view_transform")
                .and_then(Node::str)
                .map(str::to_string),
            looks,
            named_transforms,
        })
    }

    /// Reads a configuration file.
    pub fn read(path: &std::path::Path) -> Result<Config> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error::File(format!("{}: {e}", path.display())))?;
        Config::parse(&text, path.parent().map(PathBuf::from))
    }

    /// A color space by its name, an alias or a role.
    pub fn colorspace(&self, name: &str) -> Option<&ColorSpace> {
        let name = self
            .roles
            .iter()
            .find(|(r, _)| r.eq_ignore_ascii_case(name))
            .map_or(name, |(_, cs)| cs.as_str());
        // As OpenColorIO: names first, then aliases, case ignored.
        self.colorspaces
            .iter()
            .find(|c| c.name == name)
            .or_else(|| {
                self.colorspaces
                    .iter()
                    .find(|c| c.name.eq_ignore_ascii_case(name))
            })
            .or_else(|| {
                self.colorspaces
                    .iter()
                    .find(|c| c.aliases.iter().any(|a| a.eq_ignore_ascii_case(name)))
            })
    }

    pub fn display(&self, name: &str) -> Option<&Display> {
        self.displays.iter().find(|d| d.name == name)
    }

    pub fn view_transform(&self, name: &str) -> Option<&ViewTransform> {
        self.view_transforms.iter().find(|v| v.name == name)
    }

    pub fn look(&self, name: &str) -> Option<&Look> {
        self.looks.iter().find(|l| l.name == name)
    }

    /// The displays to offer: the active ones (all when it names none), in
    /// its order.
    pub fn shown_displays(&self) -> Vec<&Display> {
        let all = self.displays.iter();
        if self.active_displays.is_empty() {
            all.collect()
        } else {
            self.active_displays
                .iter()
                .filter_map(|n| self.display(n))
                .collect()
        }
    }

    /// A display's views to offer: the active ones (all when it names
    /// none).
    pub fn shown_views<'a>(&self, display: &'a Display) -> Vec<&'a View> {
        if self.active_views.is_empty() {
            display.views.iter().collect()
        } else {
            let mut v: Vec<&View> = display
                .views
                .iter()
                .filter(|v| self.active_views.iter().any(|a| a == &v.name))
                .collect();
            v.sort_by_key(|x| {
                self.active_views
                    .iter()
                    .position(|a| a == &x.name)
                    .unwrap_or(usize::MAX)
            });
            v
        }
    }

    /// The color spaces to offer: neither inactive nor data, scene's then
    /// display's.
    pub fn shown_colorspaces(&self) -> Vec<&ColorSpace> {
        self.colorspaces
            .iter()
            .filter(|c| !self.inactive_colorspaces.iter().any(|i| i == &c.name))
            .collect()
    }

    /// A path a transform names, made whole: environment variables
    /// (`$NAME`, `${NAME}`) put in, then looked for where the search path
    /// says, from the configuration's folder.
    pub fn resolve(&self, src: &str) -> Result<PathBuf> {
        let expanded = self.expand(src);
        let p = PathBuf::from(&expanded);
        if p.is_absolute() {
            return if p.exists() {
                Ok(p)
            } else {
                Err(Error::File(format!("{} is missing", p.display())))
            };
        }
        let base = self.working_dir.clone().unwrap_or_default();
        let mut tried = Vec::new();
        let dirs: Vec<String> = if self.search_path.is_empty() {
            vec![String::new()]
        } else {
            self.search_path.clone()
        };
        for d in dirs {
            let d = PathBuf::from(self.expand(&d));
            let dir = if d.is_absolute() { d } else { base.join(d) };
            let candidate = dir.join(&expanded);
            if candidate.exists() {
                return Ok(candidate);
            }
            tried.push(candidate);
        }
        Err(Error::File(format!(
            "{expanded} is not in {}",
            tried
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )))
    }

    fn expand(&self, s: &str) -> String {
        let mut out = String::new();
        let mut rest = s;
        while let Some(i) = rest.find('$') {
            out.push_str(&rest[..i]);
            let after = &rest[i + 1..];
            let (name, len) = if let Some(inner) = after.strip_prefix('{') {
                let end = inner.find('}').unwrap_or(inner.len());
                (&inner[..end], end + 2)
            } else {
                let end = after
                    .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .unwrap_or(after.len());
                (&after[..end], end)
            };
            let value = self
                .environment
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
                .or_else(|| std::env::var(name).ok())
                .unwrap_or_default();
            out.push_str(&value);
            rest = &after[len.min(after.len())..];
        }
        out.push_str(rest);
        out
    }
}
