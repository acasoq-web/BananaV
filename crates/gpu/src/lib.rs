//! Video compositing on the graphics card, with wgpu: Direct3D 12, Vulkan,
//! Metal or OpenGL, whichever the card and the system have, so it runs on
//! NVIDIA, AMD and Intel cards alike.
//!
//! It draws the same frames as `bv-render`, the processor's compositor and
//! the reference: from the same [`FrameScene`](bv_timeline::scene::FrameScene),
//! with the same math kernel for kernel (`shaders/*.wgsl`), and the tests
//! compare the two. Pictures go up as decoded (Y′CbCr planes) and are
//! converted on the card; the frame comes back in 8 bits for the screen or
//! in linear light for files.

mod compositor;
mod kernels;
mod shared;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

pub use compositor::{Compositor, Output, Rendered, draws};

/// This thread gives way to the others: the work it does can wait.
#[allow(unsafe_code)]
fn lower_priority() {
    #[cfg(windows)]
    // SAFETY: GetCurrentThread's handle always stands for this thread.
    unsafe {
        use windows::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
        };
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
}

/// Why the graphics card could not do it (the processor then does).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GpuError {
    /// No graphics card that can do it (or none by that name).
    NoAdapter,
    /// The card refused: its message.
    Failed(String),
    /// The card was lost (driver restarted, card removed).
    Lost,
}

impl std::fmt::Display for GpuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GpuError::NoAdapter => f.write_str("no graphics card can do it"),
            GpuError::Failed(e) => write!(f, "the graphics card failed: {e}"),
            GpuError::Lost => f.write_str("the graphics card was lost"),
        }
    }
}

impl std::error::Error for GpuError {}

/// What kind of graphics card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterKind {
    /// A card of its own (the fastest, usually).
    Discrete,
    /// Built into the processor.
    Integrated,
    Other,
}

/// A graphics card the compositor can use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Adapter {
    /// As the driver names it ("NVIDIA GeForce RTX 4060").
    pub name: String,
    pub kind: AdapterKind,
    /// The graphics interface used ("Dx12", "Vulkan"…).
    pub backend: String,
}

const BACKENDS: wgpu::Backends = wgpu::Backends::DX12
    .union(wgpu::Backends::VULKAN)
    .union(wgpu::Backends::METAL)
    .union(wgpu::Backends::GL);

/// The interfaces tried: all, unless `WGPU_BACKEND` names some.
fn backends() -> wgpu::Backends {
    wgpu::Backends::from_env()
        .filter(|b| !b.is_empty())
        .unwrap_or(BACKENDS)
}

fn instance() -> wgpu::Instance {
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    // `WGPU_BACKEND=vulkan` (dx12, metal, gl) forces an interface, to
    // look into a card's problems.
    desc.backends = backends();
    wgpu::Instance::new(desc)
}

/// The interfaces in the order they are preferred for one card.
fn backend_rank(b: wgpu::Backend) -> u8 {
    match b {
        wgpu::Backend::Dx12 | wgpu::Backend::Metal => 0,
        wgpu::Backend::Vulkan => 1,
        _ => 2,
    }
}

fn kind_of(t: wgpu::DeviceType) -> AdapterKind {
    match t {
        wgpu::DeviceType::DiscreteGpu => AdapterKind::Discrete,
        wgpu::DeviceType::IntegratedGpu => AdapterKind::Integrated,
        _ => AdapterKind::Other,
    }
}

/// Whether the card can run the compositor: compute shaders, enough
/// buffers per kernel, and not a software renderer (slower than
/// BananaV's own processor path).
fn usable(a: &wgpu::Adapter) -> bool {
    let info = a.get_info();
    let caps = a.get_downlevel_capabilities();
    let l = a.limits();
    info.device_type != wgpu::DeviceType::Cpu
        && caps.flags.contains(wgpu::DownlevelFlags::COMPUTE_SHADERS)
        && l.max_storage_buffers_per_shader_stage >= 3
        && l.max_compute_invocations_per_workgroup >= 256
        && l.max_compute_workgroup_size_x >= 64
        && l.max_compute_workgroup_size_y >= 16
}

/// The usable cards, each once (through its preferred interface), the
/// best first: cards of their own, then built-in ones.
fn usable_adapters(instance: &wgpu::Instance) -> Vec<wgpu::Adapter> {
    let backends = backends();
    let mut all: Vec<wgpu::Adapter> = pollster::block_on(instance.enumerate_adapters(backends))
        .into_iter()
        .filter(usable)
        .collect();
    all.sort_by_key(|a| {
        let i = a.get_info();
        let kind = match kind_of(i.device_type) {
            AdapterKind::Discrete => 0,
            AdapterKind::Integrated => 1,
            AdapterKind::Other => 2,
        };
        (kind, backend_rank(i.backend))
    });
    let mut seen: Vec<(u32, u32, String)> = Vec::new();
    all.retain(|a| {
        let i = a.get_info();
        let key = (i.vendor, i.device, i.name.clone());
        if seen.contains(&key) {
            false
        } else {
            seen.push(key);
            true
        }
    });
    all
}

fn describe(a: &wgpu::Adapter) -> Adapter {
    let i = a.get_info();
    Adapter {
        name: i.name.trim().to_string(),
        kind: kind_of(i.device_type),
        backend: format!("{:?}", i.backend),
    }
}

/// The graphics cards the compositor can use, the best first (looked up
/// once).
pub fn adapters() -> Vec<Adapter> {
    static LIST: OnceLock<Vec<Adapter>> = OnceLock::new();
    LIST.get_or_init(|| usable_adapters(&instance()).iter().map(describe).collect())
        .clone()
}

/// A graphics card opened for compositing: shared by every compositor.
pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter: Adapter,
    kernels: kernels::Kernels,
    lost: Arc<AtomicBool>,
    /// The first error not caught by a frame (shown when one fails).
    stray: Arc<Mutex<Option<String>>>,
    /// The CUDA device that is this card, if plug-ins may draw there.
    cuda: OnceLock<Option<&'static bv_cuda::Device>>,
}

impl std::fmt::Debug for Gpu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gpu")
            .field("adapter", &self.adapter)
            .finish()
    }
}

impl Gpu {
    /// Opens the card named `name` (as [`adapters`] lists it), or the best
    /// one.
    pub fn open(name: Option<&str>) -> Result<Arc<Gpu>, GpuError> {
        let instance = instance();
        let list = usable_adapters(&instance);
        let adapter = match name {
            Some(n) => list.into_iter().find(|a| describe(a).name == n),
            None => list.into_iter().next(),
        }
        .ok_or(GpuError::NoAdapter)?;
        let limits = adapter.limits();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("BananaV compositor"),
            required_features: wgpu::Features::empty(),
            // The card's own limits: frames of 4K and more need large
            // buffers.
            required_limits: limits,
            ..Default::default()
        }))
        .map_err(|e| GpuError::Failed(e.to_string()))?;
        let lost = Arc::new(AtomicBool::new(false));
        let stray = Arc::new(Mutex::new(None));
        {
            let lost = lost.clone();
            device.set_device_lost_callback(move |_, _| lost.store(true, Ordering::Relaxed));
            let stray = stray.clone();
            device.on_uncaptured_error(Arc::new(move |e| {
                if let Ok(mut s) = stray.lock() {
                    s.get_or_insert_with(|| e.to_string());
                }
            }));
        }
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let kernels = kernels::Kernels::new(&device);
        if let Some(e) = pollster::block_on(scope.pop()) {
            return Err(GpuError::Failed(e.to_string()));
        }
        Ok(Arc::new(Gpu {
            adapter: describe(&adapter),
            device,
            queue,
            kernels,
            lost,
            stray,
            cuda: OnceLock::new(),
        }))
    }

    /// Compiles every kernel now (they are compiled the first time they
    /// run otherwise), on a few threads of low priority: frames draw on
    /// meanwhile, each waiting only for the kernels it uses, and later
    /// ones find them ready. The first kernel the card refuses, if any.
    /// `BV_GPU_TRACE=1` prints each one's time.
    pub fn warm_up(&self) -> Result<(), GpuError> {
        let trace = std::env::var_os("BV_GPU_TRACE").is_some();
        let next = AtomicUsize::new(0);
        let work = || -> Result<(), GpuError> {
            lower_priority();
            while let Some(&k) = kernels::K::ALL.get(next.fetch_add(1, Ordering::Relaxed)) {
                let start = std::time::Instant::now();
                let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
                self.kernels.pipeline(&self.device, k);
                if let Some(e) = pollster::block_on(scope.pop()) {
                    return Err(GpuError::Failed(format!("{}: {e}", k.entry())));
                }
                if trace {
                    eprintln!("kernel {}: {} ms", k.entry(), start.elapsed().as_millis());
                }
            }
            Ok(())
        };
        let threads = std::thread::available_parallelism().map_or(1, |n| (n.get() / 3).clamp(1, 4));
        let start = std::time::Instant::now();
        let done = std::thread::scope(|s| {
            let workers: Vec<_> = (0..threads).map(|_| s.spawn(work)).collect();
            workers
                .into_iter()
                .try_for_each(|w| w.join().unwrap_or(Ok(())))
        });
        if trace {
            eprintln!("every kernel in {} ms", start.elapsed().as_millis());
        }
        done
    }

    /// The card in use.
    pub fn adapter(&self) -> &Adapter {
        &self.adapter
    }

    /// The CUDA device that is this card, when plug-ins that render with
    /// CUDA can draw on its pictures there (Direct3D 12 shares its buffers
    /// with CUDA; it is CUDA's first device). Looked up the first time.
    pub fn cuda(&self) -> Option<&'static bv_cuda::Device> {
        *self.cuda.get_or_init(|| shared::cuda_device(&self.device))
    }

    /// Whether the card was lost: a new one must be opened.
    pub fn is_lost(&self) -> bool {
        self.lost.load(Ordering::Relaxed)
    }

    /// An error the card reported outside a frame's watch, if any.
    fn take_stray(&self) -> Option<String> {
        self.stray.lock().ok()?.take()
    }

    /// The largest storage buffer the card takes (bytes).
    fn max_buffer(&self) -> u64 {
        let l = self.device.limits();
        l.max_storage_buffer_binding_size.min(l.max_buffer_size)
    }
}
