//! SVG pictures (W3C's Scalable Vector Graphics 1.1), drawn by BananaV: a
//! document's shapes — paths, rectangles, circles, ellipses, lines,
//! polylines and polygons — filled and stroked (dashes, caps, joins), in
//! solid colors and linear and radial gradients, through groups, `use`,
//! symbols and nested drawings with their transforms, with group opacity,
//! clip paths and masks; styled by attributes, `style` and the document's
//! style sheet; the pictures it holds (`<image>`) as the caller reads
//! them. Gzipped documents (`.svgz`) too. Text, filters, patterns and
//! markers are not drawn yet.
//!
//! A drawing has no pixels of its own: it is drawn as big as fits 3840
//! pixels on its longer side and 8.3 million in all — a 4K frame's —, its
//! own proportions kept, so that it stays sharp however big the project
//! shows it.

mod path;
mod raster;
mod render;
mod style;
mod xml;

use super::{Picture, ReadError, Result, pixel_count};

/// The longest side a drawing is drawn at, in pixels.
const LONGEST: f64 = 3840.0;
/// The most pixels a drawing is drawn with (3840 × 2160).
const MOST: f64 = 3840.0 * 2160.0;

/// Whether the file starting with `head` is an SVG document: XML text
/// whose first element is `<svg>`.
pub(super) fn is_svg(head: &[u8]) -> bool {
    let head = &head[..head.len().min(1 << 16)];
    let Some(text) = text_start(head) else {
        return false;
    };
    let t = text.trim_start();
    (t.starts_with("<?xml")
        || t.starts_with("<!--")
        || t.starts_with("<!DOCTYPE")
        || t.starts_with("<svg"))
        && xml::root_tag(t).is_some()
}

/// The UTF-8 text at the start of `head` (a character cut at its end
/// left out).
fn text_start(head: &[u8]) -> Option<&str> {
    let head = head.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(head);
    match std::str::from_utf8(head) {
        Ok(t) => Some(t),
        Err(e) if e.valid_up_to() + 4 >= head.len() => {
            std::str::from_utf8(&head[..e.valid_up_to()]).ok()
        }
        Err(_) => None,
    }
}

/// The size a drawing of intrinsic size `w` × `h` is drawn at.
pub(super) fn drawn_size(w: f64, h: f64) -> (u32, u32) {
    let (w, h) = (w.max(1e-3), h.max(1e-3));
    let k = (LONGEST / w.max(h)).min((MOST / (w * h)).sqrt());
    let side = |v: f64| (v * k).round().clamp(1.0, LONGEST) as u32;
    (side(w), side(h))
}

/// The size an SVG document starting with `head` is drawn at; `None`
/// when its root element is not all there.
pub(super) fn dimensions(head: &[u8]) -> Option<(u32, u32)> {
    let root = xml::root_tag(text_start(head)?)?;
    let (w, h, _) = render::intrinsic(&root);
    Some(drawn_size(w, h))
}

/// A gzipped document unpacked (RFC 1952); the data as it is otherwise.
fn gunzip(data: &[u8]) -> Result<std::borrow::Cow<'_, [u8]>> {
    if !data.starts_with(&[0x1f, 0x8b]) {
        return Ok(std::borrow::Cow::Borrowed(data));
    }
    let bad = || ReadError::Corrupt("gzip header".into());
    if data.len() < 18 || data[2] != 8 {
        return Err(bad());
    }
    let flags = data[3];
    let mut at = 10usize;
    if flags & 4 != 0 {
        let xlen = usize::from(u16::from_le_bytes([
            *data.get(at).ok_or_else(bad)?,
            *data.get(at + 1).ok_or_else(bad)?,
        ]));
        at += 2 + xlen;
    }
    for bit in [8u8, 16] {
        if flags & bit != 0 {
            let end = data
                .get(at..)
                .and_then(|d| d.iter().position(|&b| b == 0))
                .ok_or_else(bad)?;
            at += end + 1;
        }
    }
    if flags & 2 != 0 {
        at += 2;
    }
    let body = data.get(at..).ok_or_else(bad)?;
    // A drawing of more than 256 MB of text is not one.
    bv_deflate::inflate_limited(body, 256 << 20)
        .map(std::borrow::Cow::Owned)
        .map_err(|e| ReadError::Corrupt(format!("gzip: {e:?}")))
}

/// Reads an SVG document (gzipped too); `images` reads the pictures it
/// holds by their `href` (a `data:` URI's bytes, a file next to it…).
pub(super) fn read(data: &[u8], images: &dyn Fn(&str) -> Option<Picture>) -> Result<Picture> {
    let data = gunzip(data)?;
    let text = text_start(&data).ok_or_else(|| ReadError::Corrupt("not UTF-8 text".into()))?;
    let root = xml::parse(text).map_err(ReadError::Corrupt)?;
    if root.name != "svg" {
        return Err(ReadError::Corrupt("not an SVG document".into()));
    }
    let (w, h, _) = render::intrinsic(&root);
    let (pw, ph) = drawn_size(w, h);
    pixel_count(pw as usize, ph as usize)?;
    let doc = render::Doc::new(&root, images);
    let canvas = doc.render(pw, ph);
    let mut rgba = Vec::with_capacity(canvas.px.len() * 4);
    for c in &canvas.px {
        let a = c[3].clamp(0.0, 1.0);
        let un = |v: f32| {
            if a > 0.0 {
                ((v / a).clamp(0.0, 1.0) * 255.0).round() as u8
            } else {
                0
            }
        };
        rgba.extend_from_slice(&[un(c[0]), un(c[1]), un(c[2]), (a * 255.0).round() as u8]);
    }
    Ok(Picture::rgba8(pw, ph, rgba))
}

/// The bytes of a `data:` URI (base64, or percent-encoded); `None` for any
/// other reference.
pub fn data_uri(href: &str) -> Option<Vec<u8>> {
    let rest = href.trim().strip_prefix("data:")?;
    let comma = rest.find(',')?;
    let (meta, payload) = (&rest[..comma], &rest[comma + 1..]);
    if meta.ends_with(";base64") {
        base64(payload)
    } else {
        let mut out = Vec::with_capacity(payload.len());
        let b = payload.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'%'
                && i + 2 < b.len()
                && let Ok(v) = u8::from_str_radix(&payload[i + 1..i + 3], 16)
            {
                out.push(v);
                i += 3;
                continue;
            }
            out.push(b[i]);
            i += 1;
        }
        Some(out)
    }
}

/// Base64 (RFC 4648), white space and padding allowed.
fn base64(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut acc = 0u32;
    let mut bits = 0u32;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            c if c.is_ascii_whitespace() => continue,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read::PictureData;

    fn px(p: &Picture, x: u32, y: u32) -> [u8; 4] {
        let PictureData::Rgba8(d) = &p.data else {
            panic!("8 bits")
        };
        let i = ((y * p.width + x) * 4) as usize;
        [d[i], d[i + 1], d[i + 2], d[i + 3]]
    }

    fn no_images(_: &str) -> Option<Picture> {
        None
    }

    /// A small drawing read: its own size made 3840 wide, its shapes where
    /// it puts them.
    fn read_at(svg: &str) -> Picture {
        read(svg.as_bytes(), &no_images).expect("a drawing")
    }

    #[test]
    fn its_size_fits_a_4k_frame() {
        assert_eq!(drawn_size(100.0, 50.0), (3840, 1920));
        assert_eq!(drawn_size(1920.0, 1080.0), (3840, 2160));
        // Square: its pixels' count bounds it.
        assert_eq!(drawn_size(24.0, 24.0), (2880, 2880));
        let doc = br#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100"><rect/></svg>"#;
        assert!(is_svg(doc));
        assert_eq!(dimensions(doc), Some((3840, 1920)));
        assert!(!is_svg(b"<html><body/></html>"));
        assert!(!is_svg(b"\x89PNG"));
    }

    #[test]
    fn shapes_fill_and_stroke_where_they_are() {
        // 100 × 50 drawn 38.4 times bigger.
        let p = read_at(
            r##"<svg width="100" height="50">
                 <rect x="10" y="10" width="30" height="30" fill="#ff0000"/>
                 <circle cx="75" cy="25" r="15" fill="none" stroke="blue" stroke-width="4"/>
               </svg>"##,
        );
        assert_eq!((p.width, p.height), (3840, 1920));
        let k = 38.4;
        let at = |x: f64, y: f64| px(&p, (x * k) as u32, (y * k) as u32);
        assert_eq!(at(25.0, 25.0), [255, 0, 0, 255], "inside the square");
        assert_eq!(at(5.0, 5.0)[3], 0, "outside: transparent");
        assert_eq!(at(75.0, 10.0), [0, 0, 255, 255], "on the ring");
        assert_eq!(at(75.0, 25.0)[3], 0, "the ring's middle, not filled");
    }

    #[test]
    fn styles_groups_transforms_and_use() {
        let p = read_at(
            r##"<svg viewBox="0 0 100 50">
                 <style>.a { fill: #00ff00 } #b { fill: rgb(0,0,255) }</style>
                 <defs><rect id="r" width="10" height="10"/></defs>
                 <g fill="red" transform="translate(50 0)">
                   <use href="#r" x="5" y="5" class="a"/>
                   <rect x="30" y="5" width="10" height="10" id="b" style="fill: yellow"/>
                 </g>
                 <rect x="5" y="30" width="10" height="10" fill="red" opacity="0.5"/>
               </svg>"##,
        );
        let k = 38.4;
        let at = |x: f64, y: f64| px(&p, (x * k) as u32, (y * k) as u32);
        // The sheet's class over the group's attribute.
        assert_eq!(at(60.0, 10.0), [0, 255, 0, 255]);
        // `style` over the sheet's id rule.
        assert_eq!(at(85.0, 10.0), [255, 255, 0, 255]);
        // Half opaque.
        let half = at(10.0, 35.0);
        assert_eq!(&half[..3], &[255, 0, 0]);
        assert!((i32::from(half[3]) - 128).abs() <= 1, "{half:?}");
    }

    #[test]
    fn gradients_clip_paths_and_masks() {
        let p = read_at(
            r##"<svg viewBox="0 0 100 100">
                 <defs>
                   <linearGradient id="g"><stop offset="0" stop-color="#000"/><stop offset="1" stop-color="#fff"/></linearGradient>
                   <clipPath id="c"><rect x="0" y="0" width="50" height="100"/></clipPath>
                   <mask id="m"><rect x="0" y="0" width="100" height="100" fill="white"/><rect x="0" y="50" width="100" height="50" fill="black"/></mask>
                 </defs>
                 <rect width="100" height="40" fill="url(#g)"/>
                 <rect y="40" width="100" height="10" fill="red" clip-path="url(#c)"/>
                 <rect y="45" width="100" height="10" fill="blue" mask="url(#m)"/>
               </svg>"##,
        );
        // 100 × 100: 2880 square, 28.8 a unit.
        let k = 28.8;
        let at = |x: f64, y: f64| px(&p, (x * k) as u32, (y * k) as u32);
        let left = at(5.0, 20.0)[0];
        let mid = at(50.0, 20.0)[0];
        let right = at(95.0, 20.0)[0];
        assert!(
            left < 30 && right > 225 && (110..=145).contains(&mid),
            "{left} {mid} {right}"
        );
        assert_eq!(at(25.0, 42.0), [255, 0, 0, 255], "inside the clip");
        assert_eq!(at(75.0, 42.0)[3], 0, "clipped away");
        assert_eq!(at(50.0, 47.0)[2], 255, "the mask's white half");
        assert_eq!(at(50.0, 53.0)[3], 0, "the mask's black half");
    }

    /// A hostile document — `use` references multiplying ten times a
    /// level, sixty levels deep — ends drawing within its budget instead
    /// of taking forever.
    #[test]
    fn a_document_reusing_itself_too_much_stops() {
        let mut svg = String::from(
            r#"<svg width="10" height="10"><defs><rect id="l0" width="1" height="1"/>"#,
        );
        for i in 1..60 {
            svg.push_str(&format!(r#"<g id="l{i}">"#));
            for _ in 0..10 {
                svg.push_str(&format!(r##"<use href="#l{}"/>"##, i - 1));
            }
            svg.push_str("</g>");
        }
        svg.push_str(r##"</defs><use href="#l59"/></svg>"##);
        let t = std::time::Instant::now();
        let p = read(svg.as_bytes(), &no_images).expect("drawn, cut short");
        assert_eq!((p.width, p.height), (2880, 2880));
        assert!(
            t.elapsed() < std::time::Duration::from_secs(60),
            "{:?}",
            t.elapsed()
        );
    }

    #[test]
    fn gzipped_documents_and_embedded_pictures() {
        assert_eq!(base64("SGVsbG8="), Some(b"Hello".to_vec()));
        assert_eq!(data_uri("data:text/plain,a%20b"), Some(b"a b".to_vec()));
        assert_eq!(
            data_uri("data:image/png;base64,SGVsbG8="),
            Some(b"Hello".to_vec())
        );
        assert_eq!(data_uri("picture.png"), None);
        let svg =
            br##"<svg width="10" height="10"><rect width="10" height="10" fill="#08f"/></svg>"##;
        let packed = bv_deflate::deflate(svg, 6);
        let mut gz = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 255];
        gz.extend_from_slice(&packed);
        gz.extend_from_slice(&bv_deflate::crc32(svg).to_le_bytes());
        gz.extend_from_slice(&(svg.len() as u32).to_le_bytes());
        let p = read(&gz, &no_images).expect("gzipped");
        assert_eq!(px(&p, 100, 100), [0, 0x88, 0xff, 255]);
        // A picture it holds, read by the caller.
        let red = |_: &str| Some(Picture::rgba8(1, 1, vec![255, 0, 0, 255]));
        let doc =
            br#"<svg width="10" height="10"><image href="x.png" width="10" height="10"/></svg>"#;
        let p = read(doc, &red).expect("with a picture");
        assert_eq!(px(&p, 1000, 1000), [255, 0, 0, 255]);
    }
}
