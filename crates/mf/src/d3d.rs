//! A Direct3D 11 device on a chosen graphics card, for the hardware
//! encoders that only work with one (AMD's): the encoder gets the device
//! through a DXGI device manager and the frames as textures of it.

use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_NV12, DXGI_FORMAT_P010, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory1};
use windows::Win32::Media::MediaFoundation::*;
use windows::core::Interface;

use crate::imp::win;
use crate::{Error, Result, Vendor};

/// PCI vendor ids.
fn pci_id(v: Vendor) -> Option<u32> {
    match v {
        Vendor::Nvidia => Some(0x10DE),
        Vendor::Amd => Some(0x1002),
        Vendor::Intel => Some(0x8086),
        // "QCOM": how Windows names Adreno's maker.
        Vendor::Qualcomm => Some(0x4D4F_4351),
        _ => None,
    }
}

pub(crate) struct Device {
    pub manager: IMFDXGIDeviceManager,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    /// The CPU-writable texture frames are copied through.
    staging: Option<ID3D11Texture2D>,
}

impl Device {
    /// A device on the first graphics card of `vendor`.
    pub(crate) fn new(vendor: Vendor) -> Result<Device> {
        let want = pci_id(vendor).ok_or(Error::Unavailable)?;
        unsafe {
            let factory: IDXGIFactory1 = CreateDXGIFactory1().map_err(win)?;
            let mut adapter: Option<IDXGIAdapter1> = None;
            for i in 0.. {
                let Ok(a) = factory.EnumAdapters1(i) else {
                    break;
                };
                if a.GetDesc1().map_err(win)?.VendorId == want {
                    adapter = Some(a);
                    break;
                }
            }
            let adapter = adapter.ok_or_else(|| Error::Windows(format!("no {vendor:?} card")))?;
            let mut device = None;
            let mut context = None;
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_VIDEO_SUPPORT | D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .map_err(win)?;
            let device: ID3D11Device = device.ok_or(Error::Unavailable)?;
            let context = context.ok_or(Error::Unavailable)?;
            // The encoder uses the device from its own threads.
            if let Ok(mt) = device.cast::<ID3D11Multithread>() {
                let _ = mt.SetMultithreadProtected(true);
            }
            let mut token = 0u32;
            let mut manager = None;
            MFCreateDXGIDeviceManager(&mut token, &mut manager).map_err(win)?;
            let manager: IMFDXGIDeviceManager = manager.ok_or(Error::Unavailable)?;
            manager.ResetDevice(&device, token).map_err(win)?;
            Ok(Device {
                manager,
                device,
                context,
                staging: None,
            })
        }
    }

    fn texture(
        &self,
        width: u32,
        height: u32,
        format: DXGI_FORMAT,
        staging: bool,
    ) -> Result<ID3D11Texture2D> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: if staging {
                D3D11_USAGE_STAGING
            } else {
                D3D11_USAGE_DEFAULT
            },
            BindFlags: if staging {
                0
            } else {
                D3D11_BIND_RENDER_TARGET.0 as u32
            },
            CPUAccessFlags: if staging {
                D3D11_CPU_ACCESS_WRITE.0 as u32
            } else {
                0
            },
            MiscFlags: 0,
        };
        let mut tex = None;
        unsafe {
            self.device
                .CreateTexture2D(&desc, None, Some(&mut tex))
                .map_err(win)?;
        }
        tex.ok_or(Error::Unavailable)
    }

    /// A media buffer holding `data` (NV12 or P010: `height` rows of luma,
    /// then the chroma rows) as a texture of the device.
    pub(crate) fn upload(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        ten_bit: bool,
    ) -> Result<IMFMediaBuffer> {
        let format = if ten_bit {
            DXGI_FORMAT_P010
        } else {
            DXGI_FORMAT_NV12
        };
        let row = (width * if ten_bit { 2 } else { 1 }) as usize;
        if self.staging.is_none() {
            self.staging = Some(self.texture(width, height, format, true)?);
        }
        let staging = self.staging.clone().ok_or(Error::Unavailable)?;
        // A new texture per frame: the encoder may still be reading the last.
        let frame = self.texture(width, height, format, false)?;
        unsafe {
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(&staging, 0, D3D11_MAP_WRITE, 0, Some(&mut mapped))
                .map_err(win)?;
            let pitch = mapped.RowPitch as usize;
            let dst = mapped.pData as *mut u8;
            // Luma rows, then the interleaved chroma rows right after them.
            let rows = height as usize + height as usize / 2;
            for r in 0..rows {
                let src = &data[r * row..(r + 1) * row];
                std::ptr::copy_nonoverlapping(src.as_ptr(), dst.add(r * pitch), row);
            }
            self.context.Unmap(&staging, 0);
            self.context.CopyResource(&frame, &staging);
            let buffer =
                MFCreateDXGISurfaceBuffer(&ID3D11Texture2D::IID, &frame, 0, false).map_err(win)?;
            if let Ok(b2) = buffer.cast::<IMF2DBuffer>()
                && let Ok(n) = b2.GetContiguousLength()
            {
                let _ = buffer.SetCurrentLength(n);
            }
            Ok(buffer)
        }
    }
}
