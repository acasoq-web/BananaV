//! The graphics cards that decode and encode video on Linux. Each card has
//! a render node (`/dev/dri/renderD128`…); FFmpeg drives it through VA-API
//! (AMD and Intel cards, and NVIDIA ones with a VA-API driver) or, on
//! NVIDIA cards, NVIDIA's own decoder (NVDEC, through CUDA).

use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::{Arc, Mutex, OnceLock};

use ffmpeg_sys_next as ff;

use super::BufferRef;
use crate::{HardwareDecoding, Vendor};

/// A graphics card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Card {
    /// "NVIDIA GeForce RTX 3070": the model, as the system's PCI database
    /// names it.
    pub(crate) name: String,
    pub(crate) vendor: Vendor,
    /// Its render node.
    pub(crate) node: PathBuf,
    /// Its own memory in bytes, for the order (NVIDIA's driver does not say
    /// it: such cards count as the largest).
    memory: u64,
}

/// The computer's graphics cards, the fastest first: those with the most
/// memory of their own.
pub(crate) fn cards() -> &'static [Card] {
    static CARDS: OnceLock<Vec<Card>> = OnceLock::new();
    CARDS.get_or_init(list)
}

fn list() -> Vec<Card> {
    let Ok(dir) = std::fs::read_dir("/dev/dri") else {
        return Vec::new();
    };
    let mut nodes: Vec<PathBuf> = dir
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("renderD"))
        })
        .collect();
    nodes.sort();
    let names = PciNames::load();
    let mut cards: Vec<Card> = nodes
        .into_iter()
        .filter_map(|node| {
            let name = node.file_name()?.to_str()?.to_string();
            let dev = Path::new("/sys/class/drm").join(name).join("device");
            let hex = |f: &str| {
                let s = std::fs::read_to_string(dev.join(f)).ok()?;
                u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok()
            };
            let vendor_id = hex("vendor")?;
            let device_id = hex("device").unwrap_or(0);
            let vendor = match vendor_id {
                0x10de => Vendor::Nvidia,
                0x1002 | 0x1022 => Vendor::Amd,
                0x8086 => Vendor::Intel,
                0x5143 | 0x17cb => Vendor::Qualcomm,
                _ => Vendor::OtherHardware,
            };
            let memory = std::fs::read_to_string(dev.join("mem_info_vram_total"))
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(if vendor == Vendor::Nvidia {
                    u64::MAX
                } else {
                    0
                });
            Some(Card {
                name: names.card(vendor_id, device_id, vendor),
                vendor,
                node,
                memory,
            })
        })
        .collect();
    cards.sort_by_key(|c| std::cmp::Reverse(c.memory));
    cards
}

/// The system's database of PCI devices (`pci.ids`), for the cards' names.
struct PciNames(String);

impl PciNames {
    fn load() -> PciNames {
        let places = [
            "/usr/share/hwdata/pci.ids",
            "/usr/share/misc/pci.ids",
            "/usr/share/pci.ids",
        ];
        PciNames(
            places
                .iter()
                .find_map(|p| std::fs::read_to_string(p).ok())
                .unwrap_or_default(),
        )
    }

    /// The device's name in the database ("GA104 [GeForce RTX 3070]").
    fn device(&self, vendor: u32, device: u32) -> Option<&str> {
        let v = format!("{vendor:04x}");
        let d = format!("\t{device:04x}");
        let mut in_vendor = false;
        for line in self.0.lines() {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            if !line.starts_with('\t') {
                in_vendor = line.starts_with(&v) && line.as_bytes().get(4) == Some(&b' ');
                continue;
            }
            if in_vendor && line.starts_with(&d) && line.as_bytes().get(5) == Some(&b' ') {
                return Some(line[5..].trim());
            }
        }
        None
    }

    /// "NVIDIA GeForce RTX 3070": the maker and the model's name (the part
    /// in brackets, when the database gives one).
    fn card(&self, vendor_id: u32, device_id: u32, vendor: Vendor) -> String {
        let maker = match vendor {
            Vendor::Nvidia => "NVIDIA",
            Vendor::Amd => "AMD",
            Vendor::Intel => "Intel",
            Vendor::Qualcomm => "Qualcomm",
            _ => "",
        };
        let model = self
            .device(vendor_id, device_id)
            .map(|n| match (n.find('['), n.rfind(']')) {
                (Some(a), Some(b)) if b > a + 1 => &n[a + 1..b],
                _ => n,
            });
        match (maker, model) {
            ("", Some(m)) => m.to_string(),
            (k, Some(m)) => format!("{k} {m}"),
            ("", None) => format!("{vendor_id:04x}:{device_id:04x}"),
            (k, None) => format!("{k} {device_id:04x}"),
        }
    }
}

/// A card opened for FFmpeg's codecs.
pub(crate) struct Device {
    pub(crate) buffer: BufferRef,
    /// The layout of its pictures (VA-API or CUDA surfaces).
    pub(crate) pix_fmt: ff::AVPixelFormat,
    /// Its place in [`cards`].
    pub(crate) card: usize,
}

impl std::fmt::Debug for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Device")
            .field("card", &cards().get(self.card).map(|c| &c.name))
            .field("pix_fmt", &self.pix_fmt)
            .finish()
    }
}

/// How a card is driven.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Api {
    Vaapi,
    Cuda,
}

fn create(api: Api, card: usize) -> Option<Device> {
    let c = cards().get(card)?;
    let (kind, pix_fmt, name) = match api {
        Api::Vaapi => (
            ff::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI,
            ff::AVPixelFormat::AV_PIX_FMT_VAAPI,
            c.node.to_str()?.to_string(),
        ),
        // CUDA numbers NVIDIA's cards among themselves.
        Api::Cuda => (
            ff::AVHWDeviceType::AV_HWDEVICE_TYPE_CUDA,
            ff::AVPixelFormat::AV_PIX_FMT_CUDA,
            cards()[..card]
                .iter()
                .filter(|k| k.vendor == Vendor::Nvidia)
                .count()
                .to_string(),
        ),
    };
    let name = CString::new(name).ok()?;
    super::init();
    let mut ctx = ptr::null_mut();
    // SAFETY: FFmpeg fills `ctx` (left null on failure).
    let r =
        unsafe { ff::av_hwdevice_ctx_create(&mut ctx, kind, name.as_ptr(), ptr::null_mut(), 0) };
    (r >= 0 && !ctx.is_null()).then(|| Device {
        buffer: BufferRef(ctx),
        pix_fmt,
        card,
    })
}

/// Card `card` driven by `api`, opened once per process (`None` when its
/// driver lacks it).
pub(crate) fn open(api: Api, card: usize) -> Option<Arc<Device>> {
    type Opened = Vec<((Api, usize), Option<Arc<Device>>)>;
    static OPENED: Mutex<Opened> = Mutex::new(Vec::new());
    let mut opened = OPENED.lock().ok()?;
    if let Some((_, d)) = opened.iter().find(|(k, _)| *k == (api, card)) {
        return d.clone();
    }
    let d = create(api, card).map(Arc::new);
    opened.push(((api, card), d.clone()));
    d
}

/// Card `card` for decoding: NVIDIA's decoder on NVIDIA cards (VA-API when
/// CUDA is missing), VA-API on the others.
fn decoding(card: usize) -> Option<Arc<Device>> {
    if cards().get(card)?.vendor == Vendor::Nvidia
        && let Some(d) = open(Api::Cuda, card)
    {
        return Some(d);
    }
    open(Api::Vaapi, card)
}

/// The card decoders opened now decode on, by the setting (`None`: the
/// processor).
pub(crate) fn device(setting: &HardwareDecoding) -> Option<Arc<Device>> {
    match setting {
        HardwareDecoding::Off => None,
        HardwareDecoding::Auto => (0..cards().len()).find_map(decoding),
        HardwareDecoding::Card(name) => decoding(cards().iter().position(|c| &c.name == name)?),
    }
}

/// The cards that decode video, the fastest first.
pub(crate) fn adapters() -> Vec<String> {
    (0..cards().len())
        .filter(|&i| decoding(i).is_some())
        .map(|i| cards()[i].name.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cards_are_named_from_the_pci_database() {
        let db = PciNames(
            "# comment\n10de  NVIDIA Corporation\n\t2484  GA104 [GeForce RTX 3070]\n\t\t1043 87c0  ASUS\n\t1f82  TU117 [GeForce GTX 1650]\n1002  Advanced Micro Devices, Inc. [AMD/ATI]\n\t73ff  Navi 23 [Radeon RX 6600/6600 XT/6600M]\n8086  Intel Corporation\n\t46a6  Alder Lake-P GT2\n"
                .into(),
        );
        assert_eq!(
            db.card(0x10de, 0x2484, Vendor::Nvidia),
            "NVIDIA GeForce RTX 3070"
        );
        assert_eq!(
            db.card(0x1002, 0x73ff, Vendor::Amd),
            "AMD Radeon RX 6600/6600 XT/6600M"
        );
        assert_eq!(
            db.card(0x8086, 0x46a6, Vendor::Intel),
            "Intel Alder Lake-P GT2"
        );
        assert_eq!(db.card(0x8086, 0x1234, Vendor::Intel), "Intel 1234");
        assert_eq!(db.card(0x1af4, 0x1050, Vendor::OtherHardware), "1af4:1050");
    }

    #[test]
    #[ignore = "lists this computer's graphics cards"]
    fn print_cards() {
        for c in cards() {
            eprintln!(
                "{c:?}: decodes {}",
                device(&HardwareDecoding::Card(c.name.clone())).is_some()
            );
        }
    }
}
