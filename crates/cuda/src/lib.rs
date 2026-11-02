//! NVIDIA's CUDA driver, loaded at run time from the computer's driver
//! (`nvcuda.dll` from the system's folder, `libcuda.so.1`) where it has
//! one: its devices, memory on them, copies, and memory another graphics
//! interface made and shares with it (a Direct3D 12 buffer of the
//! graphics card's compositor). OpenFX 1.5's CUDA render hands plug-ins
//! pictures in such memory, so they never leave the card. Nothing of
//! NVIDIA's ships with BananaV: the calls are the driver API's, declared
//! here from NVIDIA's documentation.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_uint, c_void};
use std::sync::OnceLock;

type CuResult = c_int;
type CuDevice = c_int;
type CuContext = *mut c_void;
type CuExternalMemory = *mut c_void;

/// An address in a device's memory.
pub type DevicePtr = u64;

const SUCCESS: CuResult = 0;
/// `CU_EXTERNAL_MEMORY_HANDLE_TYPE_D3D12_RESOURCE`.
#[cfg(windows)]
const HANDLE_D3D12_RESOURCE: c_uint = 5;
/// `CUDA_EXTERNAL_MEMORY_DEDICATED`: a committed resource of its own.
#[cfg(windows)]
const MEMORY_DEDICATED: c_uint = 1;

/// `CUDA_EXTERNAL_MEMORY_HANDLE_DESC` (its handle union as Windows' pair).
#[cfg_attr(not(windows), allow(dead_code))]
#[repr(C)]
struct ExternalMemoryHandleDesc {
    kind: c_uint,
    handle: *mut c_void,
    name: *const c_void,
    size: u64,
    flags: c_uint,
    reserved: [c_uint; 16],
}

/// `CUDA_EXTERNAL_MEMORY_BUFFER_DESC`.
#[cfg_attr(not(windows), allow(dead_code))]
#[repr(C)]
struct ExternalMemoryBufferDesc {
    offset: u64,
    size: u64,
    flags: c_uint,
    reserved: [c_uint; 16],
}

/// A call the driver refused, and its result code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CudaError {
    pub call: &'static str,
    pub code: i32,
}

impl std::fmt::Display for CudaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = api()
            .and_then(|a| a.get_error_name)
            .and_then(|get| {
                let mut s: *const c_char = std::ptr::null();
                // SAFETY: the driver's own call; it points `s` at a static
                // string.
                let ok = unsafe { get(self.code, &mut s) } == SUCCESS && !s.is_null();
                // SAFETY: as above.
                ok.then(|| unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned())
            })
            .unwrap_or_default();
        write!(f, "{} failed: CUDA error {} {name}", self.call, self.code)
    }
}

impl std::error::Error for CudaError {}

fn check(call: &'static str, code: CuResult) -> Result<(), CudaError> {
    if code == SUCCESS {
        Ok(())
    } else {
        Err(CudaError { call, code })
    }
}

/// The driver's entry points (the 64-bit ones, `_v2`, where it has two).
struct Api {
    init: unsafe extern "system" fn(c_uint) -> CuResult,
    device_get_count: unsafe extern "system" fn(*mut c_int) -> CuResult,
    device_get: unsafe extern "system" fn(*mut CuDevice, c_int) -> CuResult,
    device_get_name: unsafe extern "system" fn(*mut c_char, c_int, CuDevice) -> CuResult,
    device_get_luid:
        Option<unsafe extern "system" fn(*mut c_char, *mut c_uint, CuDevice) -> CuResult>,
    primary_ctx_retain: unsafe extern "system" fn(*mut CuContext, CuDevice) -> CuResult,
    ctx_push: unsafe extern "system" fn(CuContext) -> CuResult,
    ctx_pop: unsafe extern "system" fn(*mut CuContext) -> CuResult,
    ctx_synchronize: unsafe extern "system" fn() -> CuResult,
    mem_alloc: unsafe extern "system" fn(*mut DevicePtr, usize) -> CuResult,
    mem_free: unsafe extern "system" fn(DevicePtr) -> CuResult,
    memcpy_htod: unsafe extern "system" fn(DevicePtr, *const c_void, usize) -> CuResult,
    memcpy_dtoh: unsafe extern "system" fn(*mut c_void, DevicePtr, usize) -> CuResult,
    memcpy_dtod: unsafe extern "system" fn(DevicePtr, DevicePtr, usize) -> CuResult,
    // Memory other interfaces share: only Direct3D 12's so far.
    #[cfg_attr(not(windows), allow(dead_code))]
    import_external_memory: Option<
        unsafe extern "system" fn(
            *mut CuExternalMemory,
            *const ExternalMemoryHandleDesc,
        ) -> CuResult,
    >,
    #[cfg_attr(not(windows), allow(dead_code))]
    external_memory_get_mapped_buffer: Option<
        unsafe extern "system" fn(
            *mut DevicePtr,
            CuExternalMemory,
            *const ExternalMemoryBufferDesc,
        ) -> CuResult,
    >,
    destroy_external_memory: Option<unsafe extern "system" fn(CuExternalMemory) -> CuResult>,
    get_error_name: Option<unsafe extern "system" fn(CuResult, *mut *const c_char) -> CuResult>,
    /// Kept loaded: the entry points live in it.
    _library: libloading::Library,
}

/// The driver's library, from the system's own folder only on Windows (a
/// file of that name beside BananaV is never loaded).
#[cfg(windows)]
fn open_library() -> Option<libloading::Library> {
    use libloading::os::windows::{LOAD_LIBRARY_SEARCH_SYSTEM32, Library};
    // SAFETY: NVIDIA's driver library; loading it runs no plug-in code.
    unsafe { Library::load_with_flags("nvcuda.dll", LOAD_LIBRARY_SEARCH_SYSTEM32) }
        .ok()
        .map(libloading::Library::from)
}

#[cfg(not(windows))]
fn open_library() -> Option<libloading::Library> {
    // SAFETY: NVIDIA's driver library.
    unsafe { libloading::Library::new("libcuda.so.1") }.ok()
}

/// The entry point `name` (ending in a zero) of `library`.
///
/// # Safety
/// `T` is the entry point's signature.
unsafe fn entry<T: Copy>(library: &libloading::Library, name: &[u8]) -> Option<T> {
    // SAFETY: as the caller promises.
    unsafe { library.get::<T>(name) }.ok().map(|s| *s)
}

impl Api {
    fn load() -> Option<Api> {
        let l = open_library()?;
        // SAFETY: the driver's entry points, with the signatures NVIDIA
        // documents; the library stays loaded while they are used.
        unsafe {
            Some(Api {
                init: entry(&l, b"cuInit\0")?,
                device_get_count: entry(&l, b"cuDeviceGetCount\0")?,
                device_get: entry(&l, b"cuDeviceGet\0")?,
                device_get_name: entry(&l, b"cuDeviceGetName\0")?,
                device_get_luid: entry(&l, b"cuDeviceGetLuid\0"),
                primary_ctx_retain: entry(&l, b"cuDevicePrimaryCtxRetain\0")?,
                ctx_push: entry(&l, b"cuCtxPushCurrent_v2\0")?,
                ctx_pop: entry(&l, b"cuCtxPopCurrent_v2\0")?,
                ctx_synchronize: entry(&l, b"cuCtxSynchronize\0")?,
                mem_alloc: entry(&l, b"cuMemAlloc_v2\0")?,
                mem_free: entry(&l, b"cuMemFree_v2\0")?,
                memcpy_htod: entry(&l, b"cuMemcpyHtoD_v2\0")?,
                memcpy_dtoh: entry(&l, b"cuMemcpyDtoH_v2\0")?,
                memcpy_dtod: entry(&l, b"cuMemcpyDtoD_v2\0")?,
                import_external_memory: entry(&l, b"cuImportExternalMemory\0"),
                external_memory_get_mapped_buffer: entry(&l, b"cuExternalMemoryGetMappedBuffer\0"),
                destroy_external_memory: entry(&l, b"cuDestroyExternalMemory\0"),
                get_error_name: entry(&l, b"cuGetErrorName\0"),
                _library: l,
            })
        }
    }
}

fn api() -> Option<&'static Api> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    API.get_or_init(Api::load).as_ref()
}

/// Whether the computer has NVIDIA's driver with CUDA: its library loads.
/// Cheap; the driver is not started.
pub fn installed() -> bool {
    api().is_some()
}

/// A CUDA device. Its context is the device's primary one — the one
/// CUDA's runtime, which plug-ins use, takes too — retained the first time
/// it is needed and kept.
pub struct Device {
    ordinal: i32,
    handle: CuDevice,
    name: String,
    luid: Option<[u8; 8]>,
    context: OnceLock<Result<usize, CudaError>>,
}

impl std::fmt::Debug for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Device")
            .field("ordinal", &self.ordinal)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// The computer's CUDA devices (the driver started the first time), none
/// without CUDA.
pub fn devices() -> &'static [Device] {
    static LIST: OnceLock<Vec<Device>> = OnceLock::new();
    LIST.get_or_init(|| {
        let Some(api) = api() else {
            return Vec::new();
        };
        let mut count = 0;
        // SAFETY: the driver's calls, with places for their answers.
        if unsafe { (api.init)(0) } != SUCCESS
            || unsafe { (api.device_get_count)(&mut count) } != SUCCESS
        {
            return Vec::new();
        }
        (0..count)
            .filter_map(|ordinal| {
                let mut handle = 0;
                // SAFETY: as above.
                if unsafe { (api.device_get)(&mut handle, ordinal) } != SUCCESS {
                    return None;
                }
                let mut name = [0 as c_char; 256];
                // SAFETY: room for the name, as told.
                let named = unsafe { (api.device_get_name)(name.as_mut_ptr(), 256, handle) };
                let name = if named == SUCCESS {
                    // SAFETY: the driver ends it with a zero within the room.
                    unsafe { CStr::from_ptr(name.as_ptr()) }
                        .to_string_lossy()
                        .into_owned()
                } else {
                    String::new()
                };
                let luid = api.device_get_luid.and_then(|get| {
                    let mut luid = [0 as c_char; 8];
                    let mut mask = 0;
                    // SAFETY: eight bytes, as the call writes.
                    let ok = unsafe { get(luid.as_mut_ptr(), &mut mask, handle) } == SUCCESS;
                    ok.then(|| luid.map(|b| b as u8))
                });
                Some(Device {
                    ordinal,
                    handle,
                    name,
                    luid,
                    context: OnceLock::new(),
                })
            })
            .collect()
    })
}

impl Device {
    /// The device whose adapter is `luid` (Windows' locally unique
    /// identifier of a graphics card, its bytes in memory's order).
    pub fn with_luid(luid: [u8; 8]) -> Option<&'static Device> {
        devices().iter().find(|d| d.luid == Some(luid))
    }

    /// CUDA's `ordinal`th device.
    pub fn with_ordinal(ordinal: i32) -> Option<&'static Device> {
        devices().iter().find(|d| d.ordinal == ordinal)
    }

    pub fn ordinal(&self) -> i32 {
        self.ordinal
    }

    /// As the driver names it ("NVIDIA GeForce RTX 4070").
    pub fn name(&self) -> &str {
        &self.name
    }

    fn api(&self) -> &'static Api {
        api().expect("a device comes from the driver")
    }

    fn context(&self) -> Result<CuContext, CudaError> {
        self.context
            .get_or_init(|| {
                let mut ctx: CuContext = std::ptr::null_mut();
                // SAFETY: the driver's call, with a place for the context.
                let code = unsafe { (self.api().primary_ctx_retain)(&mut ctx, self.handle) };
                check("cuDevicePrimaryCtxRetain", code).map(|()| ctx as usize)
            })
            .clone()
            .map(|c| c as CuContext)
    }

    /// Makes the device's context current on this thread until the guard
    /// is dropped: CUDA's calls, and a plug-in rendering with CUDA, find
    /// it there.
    pub fn enter(&self) -> Result<Current<'_>, CudaError> {
        let ctx = self.context()?;
        // SAFETY: the device's retained context.
        check("cuCtxPushCurrent", unsafe { (self.api().ctx_push)(ctx) })?;
        Ok(Current { device: self })
    }

    /// Waits for everything the device was asked to do.
    pub fn synchronize(&self) -> Result<(), CudaError> {
        let _current = self.enter()?;
        // SAFETY: the driver's call, its context current.
        check("cuCtxSynchronize", unsafe {
            (self.api().ctx_synchronize)()
        })
    }

    /// `bytes` of the device's memory.
    pub fn alloc(&'static self, bytes: usize) -> Result<Memory, CudaError> {
        let _current = self.enter()?;
        let mut address = 0;
        // SAFETY: the driver's call, its context current.
        check("cuMemAlloc", unsafe {
            (self.api().mem_alloc)(&mut address, bytes.max(1))
        })?;
        Ok(Memory {
            device: self,
            address,
            bytes,
        })
    }

    /// `from` copied to the device's memory at `to`.
    ///
    /// # Safety
    /// `to` is the device's memory, `from.len()` bytes of it not used
    /// meanwhile.
    pub unsafe fn upload(&self, to: DevicePtr, from: &[u8]) -> Result<(), CudaError> {
        let _current = self.enter()?;
        // SAFETY: as the caller promises; `from` is readable.
        check("cuMemcpyHtoD", unsafe {
            (self.api().memcpy_htod)(to, from.as_ptr().cast(), from.len())
        })
    }

    /// The device's memory at `from` copied into `to`.
    ///
    /// # Safety
    /// `from` is the device's memory, `to.len()` bytes of it.
    pub unsafe fn download(&self, to: &mut [u8], from: DevicePtr) -> Result<(), CudaError> {
        let _current = self.enter()?;
        // SAFETY: as the caller promises; `to` is writable.
        check("cuMemcpyDtoH", unsafe {
            (self.api().memcpy_dtoh)(to.as_mut_ptr().cast(), from, to.len())
        })
    }

    /// `bytes` of the device's memory copied from `from` to `to`.
    ///
    /// # Safety
    /// Both are the device's memory, `bytes` of each, apart.
    pub unsafe fn copy(
        &self,
        to: DevicePtr,
        from: DevicePtr,
        bytes: usize,
    ) -> Result<(), CudaError> {
        let _current = self.enter()?;
        // SAFETY: as the caller promises.
        check("cuMemcpyDtoD", unsafe {
            (self.api().memcpy_dtod)(to, from, bytes)
        })
    }

    /// A Direct3D 12 committed resource (a buffer) of the same graphics
    /// card, shared through the NT handle `handle`, seen as memory of the
    /// device: `bytes` is its allocation's size. The handle may be closed
    /// once imported.
    ///
    /// # Safety
    /// `handle` is a shared handle of such a resource, `bytes` its size.
    #[cfg(windows)]
    pub unsafe fn import_d3d12_resource(
        &'static self,
        handle: *mut c_void,
        bytes: u64,
    ) -> Result<External, CudaError> {
        let api = self.api();
        let missing = CudaError {
            call: "cuImportExternalMemory",
            code: -1,
        };
        let (Some(import), Some(map), Some(destroy)) = (
            api.import_external_memory,
            api.external_memory_get_mapped_buffer,
            api.destroy_external_memory,
        ) else {
            return Err(missing);
        };
        let _current = self.enter()?;
        let desc = ExternalMemoryHandleDesc {
            kind: HANDLE_D3D12_RESOURCE,
            handle,
            name: std::ptr::null(),
            size: bytes,
            flags: MEMORY_DEDICATED,
            reserved: [0; 16],
        };
        let mut memory: CuExternalMemory = std::ptr::null_mut();
        // SAFETY: as the caller promises; the description is complete.
        check("cuImportExternalMemory", unsafe {
            import(&mut memory, &desc)
        })?;
        let buffer = ExternalMemoryBufferDesc {
            offset: 0,
            size: bytes,
            flags: 0,
            reserved: [0; 16],
        };
        let mut address = 0;
        // SAFETY: the memory just imported, mapped whole.
        if let Err(e) = check("cuExternalMemoryGetMappedBuffer", unsafe {
            map(&mut address, memory, &buffer)
        }) {
            // SAFETY: imported above, mapped nowhere.
            unsafe { destroy(memory) };
            return Err(e);
        }
        Ok(External {
            device: self,
            memory: memory as usize,
            address,
            bytes,
        })
    }
}

/// A device's context current on this thread (see [`Device::enter`]).
#[derive(Debug)]
pub struct Current<'a> {
    device: &'a Device,
}

impl Drop for Current<'_> {
    fn drop(&mut self) {
        let mut ctx: CuContext = std::ptr::null_mut();
        // SAFETY: pops the context this guard pushed.
        unsafe { (self.device.api().ctx_pop)(&mut ctx) };
    }
}

/// Memory on a device, freed when dropped.
#[derive(Debug)]
pub struct Memory {
    device: &'static Device,
    address: DevicePtr,
    bytes: usize,
}

impl Memory {
    pub fn address(&self) -> DevicePtr {
        self.address
    }

    pub fn len(&self) -> usize {
        self.bytes
    }

    pub fn is_empty(&self) -> bool {
        self.bytes == 0
    }

    pub fn device(&self) -> &'static Device {
        self.device
    }
}

impl Drop for Memory {
    fn drop(&mut self) {
        if let Ok(_current) = self.device.enter() {
            // SAFETY: memory this owns, no longer used.
            unsafe { (self.device.api().mem_free)(self.address) };
        }
    }
}

/// Memory another graphics interface made, seen by a device: its address
/// is valid while this lives.
#[derive(Debug)]
pub struct External {
    device: &'static Device,
    memory: usize,
    address: DevicePtr,
    bytes: u64,
}

impl External {
    pub fn address(&self) -> DevicePtr {
        self.address
    }

    pub fn len(&self) -> u64 {
        self.bytes
    }

    pub fn is_empty(&self) -> bool {
        self.bytes == 0
    }

    pub fn device(&self) -> &'static Device {
        self.device
    }
}

impl Drop for External {
    fn drop(&mut self) {
        let api = self.device.api();
        if let (Ok(_current), Some(destroy)) = (self.device.enter(), api.destroy_external_memory) {
            // SAFETY: the mapping and the memory this owns, no longer used.
            unsafe {
                (api.mem_free)(self.address);
                destroy(self.memory as CuExternalMemory);
            }
        }
    }
}

// SAFETY: the driver's handles are usable from any thread (the driver API
// is thread-safe); the context is made current where it is used.
unsafe impl Send for Device {}
// SAFETY: as above.
unsafe impl Sync for Device {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bytes go to a device, are copied there and come back, where the
    /// computer has CUDA (nothing is checked elsewhere).
    #[test]
    fn memory_goes_to_the_device_and_comes_back() {
        let Some(device) = devices().first() else {
            return;
        };
        let bytes: Vec<u8> = (0..4096u32).map(|i| (i * 7 % 251) as u8).collect();
        let a = device.alloc(bytes.len()).expect("memory");
        let b = device.alloc(bytes.len()).expect("memory");
        let mut back = vec![0u8; bytes.len()];
        // SAFETY: the memory just allocated, of that size.
        unsafe {
            device.upload(a.address(), &bytes).expect("up");
            device
                .copy(b.address(), a.address(), bytes.len())
                .expect("copied");
            device.download(&mut back, b.address()).expect("down");
        }
        assert_eq!(back, bytes);
        assert!(!device.name().is_empty());
        assert_eq!(
            Device::with_ordinal(device.ordinal()).map(|d| d.ordinal()),
            Some(0)
        );
    }
}
