//! The encoders Windows has: Media Foundation transforms (MFTs) for each
//! compressed format, in software or on a graphics card (NVIDIA NVENC, AMD
//! AMF/VCE, Intel Quick Sync), listed once per process.

use std::sync::OnceLock;

use windows::Win32::Media::MediaFoundation::*;
use windows::core::GUID;

use crate::imp::init;
use crate::{Codec, EncoderInfo, Vendor};

fn subtype(codec: Codec) -> (GUID, GUID) {
    match codec {
        Codec::H264 => (MFT_CATEGORY_VIDEO_ENCODER, MFVideoFormat_H264),
        Codec::Hevc => (MFT_CATEGORY_VIDEO_ENCODER, MFVideoFormat_HEVC),
        Codec::Av1 => (MFT_CATEGORY_VIDEO_ENCODER, MFVideoFormat_AV1),
        Codec::Vp9 => (MFT_CATEGORY_VIDEO_ENCODER, MFVideoFormat_VP90),
        Codec::Wmv => (MFT_CATEGORY_VIDEO_ENCODER, MFVideoFormat_WMV3),
        Codec::Aac => (MFT_CATEGORY_AUDIO_ENCODER, MFAudioFormat_AAC),
        Codec::Mp3 => (MFT_CATEGORY_AUDIO_ENCODER, MFAudioFormat_MP3),
        Codec::Ac3 => (MFT_CATEGORY_AUDIO_ENCODER, MFAudioFormat_Dolby_AC3),
        Codec::Mp2 => (MFT_CATEGORY_AUDIO_ENCODER, MFAudioFormat_MPEG),
        Codec::Wma => (MFT_CATEGORY_AUDIO_ENCODER, MFAudioFormat_WMAudioV8),
        Codec::WmaLossless => (MFT_CATEGORY_AUDIO_ENCODER, MFAudioFormat_WMAudio_Lossless),
        Codec::Flac => (MFT_CATEGORY_AUDIO_ENCODER, MFAudioFormat_FLAC),
        Codec::Alac => (MFT_CATEGORY_AUDIO_ENCODER, MFAudioFormat_ALAC),
        Codec::Opus => (MFT_CATEGORY_AUDIO_ENCODER, MFAudioFormat_Opus),
    }
}

/// A string attribute of an activation object.
pub(crate) fn string_attr(a: &IMFActivate, key: &GUID) -> Option<String> {
    unsafe {
        let len = a.GetStringLength(key).ok()?;
        let mut buf = vec![0u16; len as usize + 1];
        a.GetString(key, &mut buf, None).ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// The MFTs that encode to `codec`: hardware first, in the order Windows
/// ranks them.
pub(crate) fn activates(codec: Codec, hardware: bool) -> Vec<IMFActivate> {
    if init().is_err() {
        return Vec::new();
    }
    let (category, sub) = subtype(codec);
    let major = if category == MFT_CATEGORY_VIDEO_ENCODER {
        MFMediaType_Video
    } else {
        MFMediaType_Audio
    };
    let output = MFT_REGISTER_TYPE_INFO {
        guidMajorType: major,
        guidSubtype: sub,
    };
    let flags = if hardware {
        MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER
    } else {
        MFT_ENUM_FLAG_SYNCMFT
            | MFT_ENUM_FLAG_ASYNCMFT
            | MFT_ENUM_FLAG_LOCALMFT
            | MFT_ENUM_FLAG_SORTANDFILTER
    };
    unsafe {
        let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
        let mut count = 0u32;
        if MFTEnumEx(category, flags, None, Some(&output), &mut list, &mut count).is_err()
            || list.is_null()
        {
            return Vec::new();
        }
        let items = std::slice::from_raw_parts_mut(list, count as usize);
        let out: Vec<IMFActivate> = items.iter_mut().filter_map(Option::take).collect();
        windows::Win32::System::Com::CoTaskMemFree(Some(list as *const _));
        out
    }
}

pub(crate) fn vendor_of(id: &str, name: &str) -> Vendor {
    let id = id.to_ascii_uppercase();
    let name = name.to_ascii_lowercase();
    if id.contains("VEN_10DE") || name.contains("nvidia") {
        Vendor::Nvidia
    } else if id.contains("VEN_1002") || id.contains("VEN_1022") || name.contains("amd") {
        Vendor::Amd
    } else if id.contains("VEN_8086") || name.contains("intel") || name.contains("quick sync") {
        Vendor::Intel
    } else if ["VEN_QCOM", "VEN_4D4F4351", "VEN_5143"]
        .iter()
        .any(|q| id.contains(q))
        || name.contains("qualcomm")
        || name.contains("adreno")
    {
        Vendor::Qualcomm
    } else if id.contains("VEN_") {
        Vendor::OtherHardware
    } else {
        Vendor::Software
    }
}

fn list() -> Vec<EncoderInfo> {
    let mut out = Vec::new();
    for codec in Codec::ALL {
        for hardware in [true, false] {
            for (index, a) in activates(codec, hardware).iter().enumerate() {
                let name = string_attr(a, &MFT_FRIENDLY_NAME_Attribute).unwrap_or_default();
                let id = if hardware {
                    string_attr(a, &MFT_ENUM_HARDWARE_VENDOR_ID_Attribute).unwrap_or_default()
                } else {
                    String::new()
                };
                let vendor = if hardware {
                    match vendor_of(&id, &name) {
                        Vendor::Software => Vendor::OtherHardware,
                        v => v,
                    }
                } else {
                    Vendor::Software
                };
                // A software MFT is listed again by the second pass when it
                // is also async: keep one.
                if out
                    .iter()
                    .any(|e: &EncoderInfo| e.codec == codec && e.name == name)
                {
                    continue;
                }
                out.push(EncoderInfo {
                    codec,
                    vendor,
                    name,
                    hardware,
                    index,
                });
            }
        }
    }
    out
}

/// Every encoder of the system (listed on the first call).
pub fn encoders() -> &'static [EncoderInfo] {
    static LIST: OnceLock<Vec<EncoderInfo>> = OnceLock::new();
    LIST.get_or_init(|| {
        // Listed on a thread of its own: COM stays out of the caller's
        // thread (the interface's).
        std::thread::spawn(list).join().unwrap_or_default()
    })
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "lists this computer's encoders"]
    fn print_encoders() {
        for e in super::encoders() {
            eprintln!("{e:?}");
        }
    }
}
