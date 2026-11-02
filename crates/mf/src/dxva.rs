//! Decoding on the graphics card: Windows' decoders (H.264, HEVC, VP9, AV1,
//! MPEG-2, VC-1) given a Direct3D 11 device decode with the card's video
//! engine (DXVA) — NVIDIA's NVDEC, AMD's VCN, Intel's Quick Sync alike —
//! and fall back to the processor for what the card cannot decode.

use std::sync::{Arc, Mutex};

use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
    IDXGIAdapter1, IDXGIFactory1, IDXGIFactory6,
};
use windows::Win32::Media::MediaFoundation::*;
use windows::core::Interface;

use crate::imp::win;
use crate::{Error, HardwareDecoding, Result};

/// A Direct3D 11 device that decoders share.
pub(crate) struct DecodeDevice {
    pub manager: IMFDXGIDeviceManager,
    /// Kept alive with the manager.
    _device: ID3D11Device,
}

// SAFETY: the device is multithread-protected and Media Foundation's device
// manager is free-threaded; decoders on several threads share them, as
// Media Foundation's documentation allows.
unsafe impl Send for DecodeDevice {}
// SAFETY: as above.
unsafe impl Sync for DecodeDevice {}

fn name_of(a: &IDXGIAdapter1) -> Option<(String, bool)> {
    let d = unsafe { a.GetDesc1() }.ok()?;
    let len = d
        .Description
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(d.Description.len());
    let name = String::from_utf16_lossy(&d.Description[..len])
        .trim()
        .to_string();
    let software = d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0;
    Some((name, software))
}

/// The computer's graphics cards (not the software renderer), the fastest
/// first.
fn hardware_adapters() -> Vec<(String, IDXGIAdapter1)> {
    let mut out: Vec<(String, IDXGIAdapter1)> = Vec::new();
    unsafe {
        let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else {
            return out;
        };
        let mut push = |a: IDXGIAdapter1| {
            if let Some((name, software)) = name_of(&a)
                && !software
                && !out.iter().any(|(n, _)| *n == name)
            {
                out.push((name, a));
            }
        };
        if let Ok(f6) = factory.cast::<IDXGIFactory6>() {
            for i in 0.. {
                match f6.EnumAdapterByGpuPreference::<IDXGIAdapter1>(
                    i,
                    DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
                ) {
                    Ok(a) => push(a),
                    Err(_) => break,
                }
            }
        }
        for i in 0.. {
            match factory.EnumAdapters1(i) {
                Ok(a) => push(a),
                Err(_) => break,
            }
        }
    }
    out
}

/// The graphics cards that can decode, the fastest first.
pub(crate) fn adapters() -> Vec<String> {
    hardware_adapters().into_iter().map(|(n, _)| n).collect()
}

/// Devices made so far, by card (made once: decoders share them).
static DEVICES: Mutex<Vec<(String, Arc<DecodeDevice>)>> = Mutex::new(Vec::new());

/// The device decoders use under `setting` (`None`: decoding stays on the
/// processor).
pub(crate) fn device(setting: &HardwareDecoding) -> Option<Arc<DecodeDevice>> {
    let list = hardware_adapters();
    let (name, adapter) = match setting {
        HardwareDecoding::Off => return None,
        HardwareDecoding::Auto => list.into_iter().next()?,
        HardwareDecoding::Card(n) => list.into_iter().find(|(name, _)| name == n)?,
    };
    let mut devices = DEVICES.lock().ok()?;
    if let Some((_, d)) = devices.iter().find(|(n, _)| *n == name) {
        return Some(d.clone());
    }
    let d = Arc::new(make(&adapter).ok()?);
    devices.push((name, d.clone()));
    Some(d)
}

fn make(adapter: &IDXGIAdapter1) -> Result<DecodeDevice> {
    unsafe {
        let mut device = None;
        D3D11CreateDevice(
            adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )
        .map_err(win)?;
        let device: ID3D11Device = device.ok_or(Error::Unavailable)?;
        // Decoders on several threads use it.
        let _ = device
            .cast::<ID3D11Multithread>()
            .map_err(win)?
            .SetMultithreadProtected(true);
        let mut token = 0u32;
        let mut manager = None;
        MFCreateDXGIDeviceManager(&mut token, &mut manager).map_err(win)?;
        let manager: IMFDXGIDeviceManager = manager.ok_or(Error::Unavailable)?;
        manager.ResetDevice(&device, token).map_err(win)?;
        Ok(DecodeDevice {
            manager,
            _device: device,
        })
    }
}
