//! Buffers the compositor shares with CUDA, so plug-ins that render with
//! CUDA (OpenFX 1.5's CUDA render) draw on the frame's pictures where they
//! are: on Windows a Direct3D 12 buffer made shareable, wrapped for wgpu
//! and imported into CUDA through its NT handle. Elsewhere there are none
//! yet, and those plug-ins get their pictures through the processor.
#![allow(unsafe_code)]

/// A buffer of the card that wgpu and CUDA both use.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) struct Shared {
    /// CUDA's view of it, let go of first.
    pub cuda: bv_cuda::External,
    pub buffer: wgpu::Buffer,
    pub bytes: u64,
}

/// The CUDA device that is `device`'s graphics card, when it is CUDA's
/// first one — the device plug-ins render on unless told otherwise.
#[cfg(windows)]
pub(crate) fn cuda_device(device: &wgpu::Device) -> Option<&'static bv_cuda::Device> {
    if !bv_cuda::installed() {
        return None;
    }
    // SAFETY: the device's Direct3D 12 side, only asked for its adapter.
    let hal = unsafe { device.as_hal::<wgpu::hal::api::Dx12>() }?;
    // SAFETY: a plain query of the device.
    let luid = unsafe { hal.raw_device().GetAdapterLuid() };
    let mut bytes = [0u8; 8];
    bytes[..4].copy_from_slice(&luid.LowPart.to_le_bytes());
    bytes[4..].copy_from_slice(&luid.HighPart.to_le_bytes());
    bv_cuda::Device::with_luid(bytes).filter(|d| d.ordinal() == 0)
}

#[cfg(not(windows))]
pub(crate) fn cuda_device(_device: &wgpu::Device) -> Option<&'static bv_cuda::Device> {
    None
}

/// A buffer of `bytes` both `device` and `cuda` (the same card) use.
#[cfg(windows)]
pub(crate) fn make(
    device: &wgpu::Device,
    cuda: &'static bv_cuda::Device,
    bytes: u64,
) -> Option<Shared> {
    use wgpu::hal::api::Dx12;
    use windows::Win32::Foundation::{CloseHandle, GENERIC_ALL};
    use windows::Win32::Graphics::Direct3D12::{
        D3D12_HEAP_FLAG_SHARED, D3D12_HEAP_PROPERTIES, D3D12_HEAP_TYPE_DEFAULT,
        D3D12_RESOURCE_DESC, D3D12_RESOURCE_DIMENSION_BUFFER,
        D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS, D3D12_RESOURCE_STATE_COMMON,
        D3D12_TEXTURE_LAYOUT_ROW_MAJOR, ID3D12Resource,
    };
    use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC};

    let desc = D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
        Alignment: 0,
        Width: bytes,
        Height: 1,
        DepthOrArraySize: 1,
        MipLevels: 1,
        Format: DXGI_FORMAT_UNKNOWN,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        // Kernels write it (wgpu's storage buffers).
        Flags: D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
    };
    let heap = D3D12_HEAP_PROPERTIES {
        Type: D3D12_HEAP_TYPE_DEFAULT,
        ..Default::default()
    };
    let (resource, cuda) = {
        // SAFETY: the device's Direct3D 12 side, used only to make the
        // resource; the guard is let go of before wgpu wraps it.
        let hal = unsafe { device.as_hal::<Dx12>() }?;
        let raw = hal.raw_device();
        let mut resource: Option<ID3D12Resource> = None;
        // SAFETY: a buffer's description, a place for the resource.
        unsafe {
            raw.CreateCommittedResource(
                &heap,
                D3D12_HEAP_FLAG_SHARED,
                &desc,
                D3D12_RESOURCE_STATE_COMMON,
                None,
                &mut resource,
            )
        }
        .ok()?;
        let resource = resource?;
        // SAFETY: the resource just made, shared for this process.
        let handle = unsafe {
            raw.CreateSharedHandle(
                &resource,
                None,
                GENERIC_ALL.0,
                windows::core::PCWSTR::null(),
            )
        }
        .ok()?;
        // SAFETY: a plain query of the device.
        let size = unsafe { raw.GetResourceAllocationInfo(0, &[desc]) }.SizeInBytes;
        // SAFETY: the handle of that resource, its allocation's size.
        let cuda = unsafe { cuda.import_d3d12_resource(handle.0, size) };
        // SAFETY: CUDA holds the memory now; the handle is not needed.
        let _ = unsafe { CloseHandle(handle) };
        (resource, cuda.ok()?)
    };
    // SAFETY: a buffer of `bytes` of this device, as described below.
    let buffer = unsafe {
        let hal_buffer = wgpu::hal::dx12::Device::buffer_from_raw(resource, bytes);
        device.create_buffer_from_hal::<Dx12>(
            hal_buffer,
            &wgpu::BufferDescriptor {
                label: Some("shared with CUDA"),
                size: bytes,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        )
    };
    Some(Shared {
        cuda,
        buffer,
        bytes,
    })
}

#[cfg(not(windows))]
pub(crate) fn make(
    _device: &wgpu::Device,
    _cuda: &'static bv_cuda::Device,
    _bytes: u64,
) -> Option<Shared> {
    None
}
