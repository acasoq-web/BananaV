//! The VST 3 preset file (`.vstpreset`), the one every VST 3 host reads, as
//! the SDK's documentation lays it out (all numbers little-endian):
//!
//! ```text
//! "VST3"  version (i32, 1)  class id (32 ASCII hexadecimal digits)
//! where the chunk list is (i64, from the file's start)
//! the chunks' data, one after another
//! "List"  how many chunks (i32)
//! each:   its id (4 ASCII bytes)  where its data is (i64)  how long (i64)
//! ```
//!
//! BananaV writes the component's state (`Comp`), the controller's own
//! (`Cont`) when it has one, and what the preset is (`Info`, the meta
//! information XML); it reads any file of that layout, taking only its
//! states. Presets go in the standard folder, by vendor and plug-in
//! ([`folder`]): every host finds them there.

use std::path::{Path, PathBuf};

/// The file's extension.
pub const EXTENSION: &str = "vstpreset";

const HEADER: &[u8; 4] = b"VST3";
const VERSION: i32 = 1;
const LIST: &[u8; 4] = b"List";
const COMPONENT: &[u8; 4] = b"Comp";
const CONTROLLER: &[u8; 4] = b"Cont";
const INFO: &[u8; 4] = b"Info";
/// The header's length: "VST3", the version, the class id, the list's
/// place.
const HEADER_LEN: usize = 4 + 4 + 32 + 8;

/// A preset read from a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preset {
    /// The effect it is for (32 hexadecimal digits, upper case).
    pub class_id: String,
    pub component: Vec<u8>,
    /// Empty when the file has none.
    pub controller: Vec<u8>,
}

impl Preset {
    /// Whether it is for effect `class_id` (as [`crate::ClassInfo::id`]
    /// writes it).
    pub fn is_for(&self, class_id: &str) -> bool {
        self.class_id.eq_ignore_ascii_case(class_id.trim())
    }
}

/// What the `Info` chunk says of the preset.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Info {
    /// The plug-in's name ("PlugInName").
    pub plugin: String,
    /// Its categories, "Fx|Delay" ("PlugInCategory").
    pub category: String,
    /// The preset's name ("Name"): the file's, without its extension.
    pub name: String,
}

/// `text` safe inside an XML attribute.
fn xml_attribute(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

/// The meta information XML of `info`.
fn info_xml(info: &Info) -> String {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<MetaInfo>\n");
    let mut attr = |id: &str, value: &str| {
        if !value.is_empty() {
            xml.push_str(&format!(
                "\t<Attr id=\"{id}\" value=\"{}\" type=\"string\" flags=\"writeProtected\"/>\n",
                xml_attribute(value)
            ));
        }
    };
    attr("MediaType", "VstPreset");
    attr("PlugInName", &info.plugin);
    attr("PlugInCategory", &info.category);
    attr("Name", &info.name);
    xml.push_str("</MetaInfo>\n");
    xml
}

/// A preset file for effect `class_id` (32 hexadecimal digits) with its
/// component's state and, when not empty, its controller's.
pub fn write(class_id: &str, component: &[u8], controller: &[u8], info: &Info) -> Vec<u8> {
    let mut id = [b'0'; 32];
    for (d, c) in id.iter_mut().zip(class_id.trim().bytes()) {
        *d = c.to_ascii_uppercase();
    }
    let info = info_xml(info);
    let mut chunks: Vec<(&[u8; 4], &[u8])> = vec![(COMPONENT, component)];
    if !controller.is_empty() {
        chunks.push((CONTROLLER, controller));
    }
    chunks.push((INFO, info.as_bytes()));
    let data: usize = chunks.iter().map(|(_, d)| d.len()).sum();
    let mut out = Vec::with_capacity(HEADER_LEN + data + 8 + chunks.len() * 20);
    out.extend_from_slice(HEADER);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&id);
    let list_at = (HEADER_LEN + data) as i64;
    out.extend_from_slice(&list_at.to_le_bytes());
    let mut places = Vec::with_capacity(chunks.len());
    for (_, d) in &chunks {
        places.push((out.len() as i64, d.len() as i64));
        out.extend_from_slice(d);
    }
    out.extend_from_slice(LIST);
    out.extend_from_slice(&(chunks.len() as i32).to_le_bytes());
    for ((tag, _), (at, len)) in chunks.iter().zip(places) {
        out.extend_from_slice(*tag);
        out.extend_from_slice(&at.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
    }
    out
}

fn i64_at(b: &[u8], at: usize) -> Option<i64> {
    Some(i64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

fn i32_at(b: &[u8], at: usize) -> Option<i32> {
    Some(i32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// The preset in `bytes`; `None` when they are no preset file or hold no
/// component state.
pub fn read(bytes: &[u8]) -> Option<Preset> {
    if bytes.get(..4)? != HEADER {
        return None;
    }
    let id = std::str::from_utf8(bytes.get(8..40)?).ok()?;
    if !id.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let list = usize::try_from(i64_at(bytes, 40)?).ok()?;
    if bytes.get(list..list + 4)? != LIST {
        return None;
    }
    let count = usize::try_from(i32_at(bytes, list + 4)?).ok()?;
    let mut preset = Preset {
        class_id: id.to_ascii_uppercase(),
        component: Vec::new(),
        controller: Vec::new(),
    };
    for k in 0..count.min(1024) {
        let entry = list + 8 + k * 20;
        let tag = bytes.get(entry..entry + 4)?;
        let at = usize::try_from(i64_at(bytes, entry + 4)?).ok()?;
        let len = usize::try_from(i64_at(bytes, entry + 12)?).ok()?;
        let data = bytes.get(at..at.checked_add(len)?)?;
        if tag == COMPONENT {
            preset.component = data.to_vec();
        } else if tag == CONTROLLER {
            preset.controller = data.to_vec();
        }
    }
    (!preset.component.is_empty()).then_some(preset)
}

/// `name` made a folder's or a file's name: what systems refuse in one
/// replaced.
pub fn file_name(name: &str) -> String {
    let clean: String = name
        .trim()
        .chars()
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim_end_matches(['.', ' ']).to_string();
    if clean.is_empty() {
        "_".to_string()
    } else {
        clean
    }
}

/// The standard folder of `vendor`'s `plugin`'s presets under `base`
/// (`Documents\VST3 Presets` on Windows, `~/.vst3/presets` on Linux).
pub fn folder(base: &Path, vendor: &str, plugin: &str) -> PathBuf {
    let vendor = if vendor.trim().is_empty() {
        "Unknown"
    } else {
        vendor
    };
    base.join(file_name(vendor)).join(file_name(plugin))
}

/// The preset files in `dir`, by name.
pub fn list(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_file()
                        && p.extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION))
                })
                .collect()
        })
        .unwrap_or_default();
    found.sort_by_key(|p| p.file_stem().map(|s| s.to_string_lossy().to_lowercase()));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "42414E414E4156544553544741494E31";

    #[test]
    fn a_preset_goes_and_comes_back() {
        let info = Info {
            plugin: "Gain".into(),
            category: "Fx".into(),
            name: "Loud & clear".into(),
        };
        let bytes = write(
            &ID.to_lowercase(),
            b"state of the component",
            b"view",
            &info,
        );
        // The layout every host reads.
        assert_eq!(&bytes[..4], b"VST3");
        assert_eq!(i32_at(&bytes, 4), Some(1));
        assert_eq!(&bytes[8..40], ID.as_bytes());
        let list = i64_at(&bytes, 40).unwrap() as usize;
        assert_eq!(&bytes[list..list + 4], b"List");
        assert_eq!(i32_at(&bytes, list + 4), Some(3));
        assert_eq!(&bytes[list + 8..list + 12], b"Comp");
        assert_eq!(i64_at(&bytes, list + 12), Some(48));
        let xml = String::from_utf8_lossy(&bytes);
        assert!(xml.contains("<Attr id=\"Name\" value=\"Loud &amp; clear\""));
        let back = read(&bytes).unwrap();
        assert!(back.is_for(ID));
        assert_eq!(back.component, b"state of the component");
        assert_eq!(back.controller, b"view");
        // No controller state: no chunk for it.
        let bytes = write(ID, b"c", b"", &Info::default());
        let back = read(&bytes).unwrap();
        assert!(back.controller.is_empty());
        assert_eq!(back.component, b"c");
    }

    #[test]
    fn what_is_no_preset_is_refused() {
        assert_eq!(read(b""), None);
        assert_eq!(read(b"VST2 and the rest"), None);
        let mut bytes = write(ID, b"state", b"", &Info::default());
        // Its list's place pointing past the end.
        bytes[40] = 0xFF;
        assert_eq!(read(&bytes), None);
        // No component state.
        assert_eq!(read(&write(ID, b"", b"", &Info::default())), None);
    }

    #[test]
    fn folders_are_named_as_the_standard_says() {
        let base = Path::new("presets");
        assert_eq!(
            folder(base, "Makers: \"Pro\"", "Echo/Delay"),
            base.join("Makers_ _Pro_").join("Echo_Delay")
        );
        assert_eq!(folder(base, " ", "Gain"), base.join("Unknown").join("Gain"));
        assert_eq!(file_name("Bright. "), "Bright");
    }
}
