//! SVG's styling: colors (CSS Color's keywords, `#rgb`, `#rrggbb` and their
//! alpha forms, `rgb()`, `rgba()`, `hsl()`, `hsla()`), lengths in their
//! units, `style` declarations, and the document's style sheets with the
//! selectors drawing programs write — a type, `*`, classes and an id, alone
//! or together, in groups (`a, .b`) — ordered by specificity.

use super::path::Scanner;
use super::xml::Element;

/// A color, straight: red, green, blue, alpha (0–1, display-encoded).
pub(super) type Rgba = [f32; 4];

/// A CSS color; `None` when it is not one.
pub(super) fn color(s: &str) -> Option<Rgba> {
    let s = s.trim();
    let lower = s.to_ascii_lowercase();
    if let Some(hex) = lower.strip_prefix('#') {
        let v: Vec<u8> = hex
            .chars()
            .map(|c| c.to_digit(16).map(|d| d as u8))
            .collect::<Option<Vec<u8>>>()?;
        let (rgb, a) = match v.len() {
            3 => ([v[0] * 17, v[1] * 17, v[2] * 17], 255),
            4 => ([v[0] * 17, v[1] * 17, v[2] * 17], v[3] * 17),
            6 => ([v[0] * 16 + v[1], v[2] * 16 + v[3], v[4] * 16 + v[5]], 255),
            8 => (
                [v[0] * 16 + v[1], v[2] * 16 + v[3], v[4] * 16 + v[5]],
                v[6] * 16 + v[7],
            ),
            _ => return None,
        };
        return Some(rgba8(rgb, a));
    }
    if let Some(open) = lower.find('(') {
        let name = lower[..open].trim();
        let close = lower.rfind(')')?;
        let inner = &lower[open + 1..close];
        // `rgb(1 2 3 / 50%)` and `rgb(1, 2, 3, 0.5)` alike.
        let parts: Vec<&str> = inner
            .split(|c: char| c == ',' || c == '/' || c.is_whitespace())
            .filter(|p| !p.is_empty())
            .collect();
        let alpha = |i: usize| parts.get(i).map_or(Some(1.0), |a| fraction(a));
        return match name {
            "rgb" | "rgba" if parts.len() >= 3 => {
                let ch = |i: usize| -> Option<f32> {
                    let t = parts[i];
                    match t.strip_suffix('%') {
                        Some(pc) => pc.parse::<f32>().ok().map(|v| v / 100.0),
                        None => t.parse::<f32>().ok().map(|v| v / 255.0),
                    }
                    .map(|v| v.clamp(0.0, 1.0))
                };
                Some([ch(0)?, ch(1)?, ch(2)?, alpha(3)?])
            }
            "hsl" | "hsla" if parts.len() >= 3 => {
                let h = parts[0].trim_end_matches("deg").parse::<f32>().ok()?;
                let s = parts[1].trim_end_matches('%').parse::<f32>().ok()? / 100.0;
                let l = parts[2].trim_end_matches('%').parse::<f32>().ok()? / 100.0;
                let [r, g, b] = hsl(h, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0));
                Some([r, g, b, alpha(3)?])
            }
            _ => None,
        };
    }
    if lower == "transparent" {
        return Some([0.0; 4]);
    }
    named(&lower).map(|rgb| rgba8(rgb, 255))
}

fn rgba8([r, g, b]: [u8; 3], a: u8) -> Rgba {
    [
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        f32::from(a) / 255.0,
    ]
}

/// A number or a percentage, as a fraction of one (an opacity).
pub(super) fn fraction(s: &str) -> Option<f32> {
    let s = s.trim();
    match s.strip_suffix('%') {
        Some(pc) => pc.trim().parse::<f32>().ok().map(|v| v / 100.0),
        None => s.parse::<f32>().ok(),
    }
    .filter(|v| v.is_finite())
    .map(|v| v.clamp(0.0, 1.0))
}

fn hsl(h: f32, s: f32, l: f32) -> [f32; 3] {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    [r + m, g + m, b + m]
}

/// CSS Color's named colors (CSS Color Module, 6.1), sorted.
const NAMES: &[(&str, [u8; 3])] = &[
    ("aliceblue", [240, 248, 255]),
    ("antiquewhite", [250, 235, 215]),
    ("aqua", [0, 255, 255]),
    ("aquamarine", [127, 255, 212]),
    ("azure", [240, 255, 255]),
    ("beige", [245, 245, 220]),
    ("bisque", [255, 228, 196]),
    ("black", [0, 0, 0]),
    ("blanchedalmond", [255, 235, 205]),
    ("blue", [0, 0, 255]),
    ("blueviolet", [138, 43, 226]),
    ("brown", [165, 42, 42]),
    ("burlywood", [222, 184, 135]),
    ("cadetblue", [95, 158, 160]),
    ("chartreuse", [127, 255, 0]),
    ("chocolate", [210, 105, 30]),
    ("coral", [255, 127, 80]),
    ("cornflowerblue", [100, 149, 237]),
    ("cornsilk", [255, 248, 220]),
    ("crimson", [220, 20, 60]),
    ("cyan", [0, 255, 255]),
    ("darkblue", [0, 0, 139]),
    ("darkcyan", [0, 139, 139]),
    ("darkgoldenrod", [184, 134, 11]),
    ("darkgray", [169, 169, 169]),
    ("darkgreen", [0, 100, 0]),
    ("darkgrey", [169, 169, 169]),
    ("darkkhaki", [189, 183, 107]),
    ("darkmagenta", [139, 0, 139]),
    ("darkolivegreen", [85, 107, 47]),
    ("darkorange", [255, 140, 0]),
    ("darkorchid", [153, 50, 204]),
    ("darkred", [139, 0, 0]),
    ("darksalmon", [233, 150, 122]),
    ("darkseagreen", [143, 188, 143]),
    ("darkslateblue", [72, 61, 139]),
    ("darkslategray", [47, 79, 79]),
    ("darkslategrey", [47, 79, 79]),
    ("darkturquoise", [0, 206, 209]),
    ("darkviolet", [148, 0, 211]),
    ("deeppink", [255, 20, 147]),
    ("deepskyblue", [0, 191, 255]),
    ("dimgray", [105, 105, 105]),
    ("dimgrey", [105, 105, 105]),
    ("dodgerblue", [30, 144, 255]),
    ("firebrick", [178, 34, 34]),
    ("floralwhite", [255, 250, 240]),
    ("forestgreen", [34, 139, 34]),
    ("fuchsia", [255, 0, 255]),
    ("gainsboro", [220, 220, 220]),
    ("ghostwhite", [248, 248, 255]),
    ("gold", [255, 215, 0]),
    ("goldenrod", [218, 165, 32]),
    ("gray", [128, 128, 128]),
    ("green", [0, 128, 0]),
    ("greenyellow", [173, 255, 47]),
    ("grey", [128, 128, 128]),
    ("honeydew", [240, 255, 240]),
    ("hotpink", [255, 105, 180]),
    ("indianred", [205, 92, 92]),
    ("indigo", [75, 0, 130]),
    ("ivory", [255, 255, 240]),
    ("khaki", [240, 230, 140]),
    ("lavender", [230, 230, 250]),
    ("lavenderblush", [255, 240, 245]),
    ("lawngreen", [124, 252, 0]),
    ("lemonchiffon", [255, 250, 205]),
    ("lightblue", [173, 216, 230]),
    ("lightcoral", [240, 128, 128]),
    ("lightcyan", [224, 255, 255]),
    ("lightgoldenrodyellow", [250, 250, 210]),
    ("lightgray", [211, 211, 211]),
    ("lightgreen", [144, 238, 144]),
    ("lightgrey", [211, 211, 211]),
    ("lightpink", [255, 182, 193]),
    ("lightsalmon", [255, 160, 122]),
    ("lightseagreen", [32, 178, 170]),
    ("lightskyblue", [135, 206, 250]),
    ("lightslategray", [119, 136, 153]),
    ("lightslategrey", [119, 136, 153]),
    ("lightsteelblue", [176, 196, 222]),
    ("lightyellow", [255, 255, 224]),
    ("lime", [0, 255, 0]),
    ("limegreen", [50, 205, 50]),
    ("linen", [250, 240, 230]),
    ("magenta", [255, 0, 255]),
    ("maroon", [128, 0, 0]),
    ("mediumaquamarine", [102, 205, 170]),
    ("mediumblue", [0, 0, 205]),
    ("mediumorchid", [186, 85, 211]),
    ("mediumpurple", [147, 112, 219]),
    ("mediumseagreen", [60, 179, 113]),
    ("mediumslateblue", [123, 104, 238]),
    ("mediumspringgreen", [0, 250, 154]),
    ("mediumturquoise", [72, 209, 204]),
    ("mediumvioletred", [199, 21, 133]),
    ("midnightblue", [25, 25, 112]),
    ("mintcream", [245, 255, 250]),
    ("mistyrose", [255, 228, 225]),
    ("moccasin", [255, 228, 181]),
    ("navajowhite", [255, 222, 173]),
    ("navy", [0, 0, 128]),
    ("oldlace", [253, 245, 230]),
    ("olive", [128, 128, 0]),
    ("olivedrab", [107, 142, 35]),
    ("orange", [255, 165, 0]),
    ("orangered", [255, 69, 0]),
    ("orchid", [218, 112, 214]),
    ("palegoldenrod", [238, 232, 170]),
    ("palegreen", [152, 251, 152]),
    ("paleturquoise", [175, 238, 238]),
    ("palevioletred", [219, 112, 147]),
    ("papayawhip", [255, 239, 213]),
    ("peachpuff", [255, 218, 185]),
    ("peru", [205, 133, 63]),
    ("pink", [255, 192, 203]),
    ("plum", [221, 160, 221]),
    ("powderblue", [176, 224, 230]),
    ("purple", [128, 0, 128]),
    ("rebeccapurple", [102, 51, 153]),
    ("red", [255, 0, 0]),
    ("rosybrown", [188, 143, 143]),
    ("royalblue", [65, 105, 225]),
    ("saddlebrown", [139, 69, 19]),
    ("salmon", [250, 128, 114]),
    ("sandybrown", [244, 164, 96]),
    ("seagreen", [46, 139, 87]),
    ("seashell", [255, 245, 238]),
    ("sienna", [160, 82, 45]),
    ("silver", [192, 192, 192]),
    ("skyblue", [135, 206, 235]),
    ("slateblue", [106, 90, 205]),
    ("slategray", [112, 128, 144]),
    ("slategrey", [112, 128, 144]),
    ("snow", [255, 250, 250]),
    ("springgreen", [0, 255, 127]),
    ("steelblue", [70, 130, 180]),
    ("tan", [210, 180, 140]),
    ("teal", [0, 128, 128]),
    ("thistle", [216, 191, 216]),
    ("tomato", [255, 99, 71]),
    ("turquoise", [64, 224, 208]),
    ("violet", [238, 130, 238]),
    ("wheat", [245, 222, 179]),
    ("white", [255, 255, 255]),
    ("whitesmoke", [245, 245, 245]),
    ("yellow", [255, 255, 0]),
    ("yellowgreen", [154, 205, 50]),
];

/// A CSS color keyword's color.
fn named(name: &str) -> Option<[u8; 3]> {
    NAMES
        .binary_search_by(|(n, _)| (*n).cmp(name))
        .ok()
        .map(|i| NAMES[i].1)
}

/// A length in user units: a number and its unit (`px`, `pt`, `pc`, `mm`,
/// `cm`, `in`, `em`, `ex`, `%` of `percent_of`); `None` when it is not one.
pub(super) fn length(s: &str, percent_of: f64) -> Option<f64> {
    let mut sc = Scanner::new(s.trim());
    let v = sc.number()?;
    let unit = sc.rest().trim();
    let k = match unit {
        "" | "px" => 1.0,
        "pt" => 4.0 / 3.0,
        "pc" => 16.0,
        "mm" => 96.0 / 25.4,
        "cm" => 96.0 / 2.54,
        "in" => 96.0,
        // The default font's size.
        "em" | "rem" => 16.0,
        "ex" => 8.0,
        "%" => percent_of / 100.0,
        _ => return None,
    };
    Some(v * k)
}

/// A `style` attribute's declarations: (property, value), in order.
pub(super) fn declarations(s: &str) -> Vec<(String, String)> {
    s.split(';')
        .filter_map(|d| {
            let (k, v) = d.split_once(':')?;
            let v = v.trim().trim_end_matches("!important").trim();
            let k = k.trim().to_ascii_lowercase();
            (!k.is_empty() && !v.is_empty()).then(|| (k, v.to_string()))
        })
        .collect()
}

/// A selector BananaV matches: an element's type (or none, `*`), its
/// classes and its id, all of them together.
#[derive(Clone, Debug, PartialEq)]
struct Selector {
    tag: Option<String>,
    classes: Vec<String>,
    id: Option<String>,
}

impl Selector {
    fn parse(s: &str) -> Option<Selector> {
        let s = s.trim();
        // Combinators, attributes and pseudo-classes: not matched.
        if s.is_empty() || s.contains(|c: char| c.is_whitespace() || "[]:>+~()".contains(c)) {
            return None;
        }
        let mut sel = Selector {
            tag: None,
            classes: Vec::new(),
            id: None,
        };
        let mut rest = s;
        let first = rest.find(['.', '#']).unwrap_or(rest.len());
        let tag = &rest[..first];
        if !tag.is_empty() && tag != "*" {
            sel.tag = Some(tag.to_string());
        }
        rest = &rest[first..];
        while !rest.is_empty() {
            let kind = rest.as_bytes()[0];
            let body = &rest[1..];
            let end = body.find(['.', '#']).unwrap_or(body.len());
            let name = body[..end].to_string();
            if name.is_empty() {
                return None;
            }
            if kind == b'.' {
                sel.classes.push(name);
            } else {
                sel.id = Some(name);
            }
            rest = &body[end..];
        }
        Some(sel)
    }

    fn specificity(&self) -> (u32, u32, u32) {
        (
            u32::from(self.id.is_some()),
            self.classes.len() as u32,
            u32::from(self.tag.is_some()),
        )
    }

    fn matches(&self, e: &Element) -> bool {
        if self.tag.as_ref().is_some_and(|t| *t != e.name) {
            return false;
        }
        if let Some(id) = &self.id
            && e.attr("id") != Some(id.as_str())
        {
            return false;
        }
        if !self.classes.is_empty() {
            let mine: Vec<&str> = e.attr("class").unwrap_or("").split_whitespace().collect();
            if !self.classes.iter().all(|c| mine.contains(&c.as_str())) {
                return false;
            }
        }
        true
    }
}

/// A rule of a style sheet: its selector, how it ranks and its
/// declarations.
#[derive(Clone, Debug)]
pub(super) struct Rule {
    selector: Selector,
    specificity: (u32, u32, u32),
    order: usize,
    declarations: Vec<(String, String)>,
}

/// The rules of a style sheet (at-rules — `@media`, `@font-face`… — left
/// out).
pub(super) fn sheet(css: &str, rules: &mut Vec<Rule>) {
    // Comments out.
    let mut text = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(at) = rest.find("/*") {
        text.push_str(&rest[..at]);
        rest = rest[at + 2..]
            .find("*/")
            .map_or("", |e| &rest[at + 2 + e + 2..]);
    }
    text.push_str(rest);
    let mut rest = text.as_str();
    while let Some(open) = rest.find('{') {
        let head = rest[..open].trim();
        // The block, its nested braces (an at-rule's) counted.
        let mut depth = 0usize;
        let mut close = None;
        for (i, c) in rest[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(open + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close = close.unwrap_or(rest.len());
        let body = &rest[open + 1..close.max(open + 1)];
        // An at-rule ending with `;` before the block (`@import …;`).
        let head = head.rsplit(';').next().unwrap_or(head).trim();
        if !head.starts_with('@') {
            let decls = declarations(body);
            for s in head.split(',') {
                if let Some(selector) = Selector::parse(s) {
                    rules.push(Rule {
                        specificity: selector.specificity(),
                        selector,
                        order: rules.len(),
                        declarations: decls.clone(),
                    });
                }
            }
        }
        rest = rest.get(close + 1..).unwrap_or("");
    }
}

/// The declarations of the rules that match `e`, the strongest last (by
/// specificity, then by order).
pub(super) fn matching<'a>(rules: &'a [Rule], e: &Element) -> Vec<&'a (String, String)> {
    let mut hits: Vec<&Rule> = rules.iter().filter(|r| r.selector.matches(e)).collect();
    hits.sort_by_key(|r| (r.specificity, r.order));
    hits.iter().flat_map(|r| r.declarations.iter()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_in_their_forms() {
        assert_eq!(color("#f00"), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(
            color("#00ff0080").map(|c| (c[1], (c[3] * 255.0).round())),
            Some((1.0, 128.0))
        );
        assert_eq!(color("rgb(255, 0, 0)"), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(color("rgba(0,0,255,.5)"), Some([0.0, 0.0, 1.0, 0.5]));
        assert_eq!(
            color("rgb(100%, 0%, 0% / 25%)"),
            Some([1.0, 0.0, 0.0, 0.25])
        );
        let g = color("hsl(120, 100%, 25%)").expect("hsl");
        assert!((g[1] - 0.5).abs() < 1e-6 && g[0] == 0.0);
        assert_eq!(
            color("RebeccaPurple").map(|c| (c[0] * 255.0).round()),
            Some(102.0)
        );
        assert_eq!(color("transparent"), Some([0.0; 4]));
        assert_eq!(color("nonsense"), None);
    }

    #[test]
    fn the_color_names_are_sorted() {
        // `named` searches them by halves.
        assert!(NAMES.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(NAMES.len(), 148);
        for name in ["aliceblue", "yellowgreen", "lightgoldenrodyellow", "grey"] {
            assert!(named(name).is_some(), "{name}");
        }
    }

    #[test]
    fn lengths_in_their_units() {
        assert_eq!(length("10", 0.0), Some(10.0));
        assert_eq!(length("1in", 0.0), Some(96.0));
        assert_eq!(length("50%", 200.0), Some(100.0));
        assert_eq!(length("3pt", 0.0), Some(4.0));
        assert_eq!(length("2 furlongs", 0.0), None);
    }

    #[test]
    fn a_sheet_matches_by_specificity_then_order() {
        let mut rules = Vec::new();
        sheet(
            "/* c */ @media print { rect { fill: blue } }\n\
             rect { fill: red; stroke: black }\n\
             .a, #x { fill: green }\n\
             rect.a { fill: yellow }\n\
             g rect { fill: gray }",
            &mut rules,
        );
        let e = super::super::xml::parse(r#"<rect class="a b" id="x"/>"#).expect("an element");
        let decls: Vec<&str> = matching(&rules, &e)
            .iter()
            .filter(|(k, _)| k == "fill")
            .map(|(_, v)| v.as_str())
            .collect();
        // Weakest first: rect, .a, rect.a, #x.
        assert_eq!(decls, ["red", "green", "yellow", "green"]);
    }
}
