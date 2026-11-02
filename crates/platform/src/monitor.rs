//! What a long job needs from the OS: how much processor and memory this
//! process uses and how busy the disks are (the render progress window's
//! Performance section), its progress on the taskbar button, a sound when it
//! ends and shutting the computer down afterwards.

use std::time::Instant;

/// Processor, memory and disk use, each sampled between calls.
#[derive(Debug)]
pub struct Usage {
    last_cpu: Option<(Instant, f64)>,
    disk: imp::DiskQuery,
    /// Opened on the first [`Usage::gpu`].
    gpu: Option<imp::GpuQuery>,
}

impl Default for Usage {
    fn default() -> Usage {
        Usage::new()
    }
}

impl Usage {
    pub fn new() -> Usage {
        Usage {
            last_cpu: None,
            disk: imp::DiskQuery::new(),
            gpu: None,
        }
    }

    /// This process's share of the processor since the last call, in
    /// percent of all its cores (`None` on the first call).
    pub fn cpu(&mut self) -> Option<f64> {
        let now = Instant::now();
        let busy = imp::process_cpu_seconds()?;
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get()) as f64;
        let share = self.last_cpu.and_then(|(at, before)| {
            let wall = now.duration_since(at).as_secs_f64();
            (wall > 0.0).then(|| ((busy - before) / (wall * cores) * 100.0).clamp(0.0, 100.0))
        });
        self.last_cpu = Some((now, busy));
        share
    }

    /// Memory this process uses (its working set), in bytes.
    pub fn memory(&self) -> Option<u64> {
        imp::process_memory()
    }

    /// How busy the disks were since the last call, in percent of the time
    /// (`None` on the first call).
    pub fn disk(&mut self) -> Option<f64> {
        self.disk.sample()
    }

    /// How busy this process kept the graphics cards since the last call:
    /// its busiest engine (3D, video decoding, copying…), in percent of the
    /// time, as the system's Task Manager shows it (`None` on the first
    /// call, or without the system's counters).
    pub fn gpu(&mut self) -> Option<f64> {
        self.gpu.get_or_insert_with(imp::GpuQuery::new).sample()
    }
}

/// Shows `progress` (0–1) on the taskbar button of this process's window
/// titled `title`; `None` takes it away.
pub fn taskbar_progress(title: &str, progress: Option<f32>) {
    imp::taskbar_progress(title, progress);
}

/// Plays the system's notification sound (a long job ended).
pub fn notify_sound() {
    imp::notify_sound();
}

/// Asks the system to shut the computer down in `seconds`, with `message`
/// shown meanwhile (`shutdown /a` cancels it). Returns whether it was asked.
pub fn shutdown_in(seconds: u32, message: &str) -> bool {
    if cfg!(target_os = "linux") {
        // In whole minutes (`shutdown -c` cancels it).
        let minutes = seconds.div_ceil(60);
        return std::process::Command::new("shutdown")
            .args(["-P", &format!("+{minutes}"), message])
            .status()
            .is_ok_and(|s| s.success());
    }
    if !cfg!(windows) {
        return false;
    }
    std::process::Command::new("shutdown")
        .args(["/s", "/t", &seconds.to_string(), "/c", message])
        .spawn()
        .is_ok()
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod imp {
    use windows::Win32::Foundation::{FILETIME, HWND};
    use windows::Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC};
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    };
    use windows::Win32::System::Performance::{
        PDH_FMT_COUNTERVALUE, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER,
        PDH_HQUERY, PDH_MORE_DATA, PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData,
        PdhGetFormattedCounterArrayW, PdhGetFormattedCounterValue, PdhOpenQueryW,
    };
    use windows::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    use windows::Win32::UI::Shell::{ITaskbarList3, TBPF_NOPROGRESS, TBPF_NORMAL, TaskbarList};
    use windows::core::PCWSTR;
    use windows::core::w;

    fn seconds(t: FILETIME) -> f64 {
        (((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64) as f64 / 1e7
    }

    pub fn process_cpu_seconds() -> Option<f64> {
        let (mut c, mut e, mut k, mut u) = Default::default();
        // SAFETY: the pseudo handle of this process; four valid FILETIMEs.
        unsafe { GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) }.ok()?;
        Some(seconds(k) + seconds(u))
    }

    pub fn process_memory() -> Option<u64> {
        let mut m = PROCESS_MEMORY_COUNTERS {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            ..Default::default()
        };
        // SAFETY: `m` is a valid, writable structure of the size given.
        let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut m, m.cb) };
        ok.as_bool().then_some(m.WorkingSetSize as u64)
    }

    /// The disks' idle time, through a performance counter query.
    #[derive(Debug)]
    pub struct DiskQuery {
        query: Option<(PDH_HQUERY, PDH_HCOUNTER)>,
        primed: bool,
    }

    impl DiskQuery {
        pub fn new() -> DiskQuery {
            let mut query = PDH_HQUERY::default();
            let mut counter = PDH_HCOUNTER::default();
            // SAFETY: valid out-pointers; the counter path is a static string.
            let ok = unsafe {
                PdhOpenQueryW(None, 0, &mut query) == 0
                    && PdhAddEnglishCounterW(
                        query,
                        w!("\\PhysicalDisk(_Total)\\% Idle Time"),
                        0,
                        &mut counter,
                    ) == 0
            };
            DiskQuery {
                query: ok.then_some((query, counter)),
                primed: false,
            }
        }

        pub fn sample(&mut self) -> Option<f64> {
            let (query, counter) = self.query?;
            // SAFETY: the query and counter are open until drop.
            if unsafe { PdhCollectQueryData(query) } != 0 {
                return None;
            }
            if !self.primed {
                // A rate needs two samples.
                self.primed = true;
                return None;
            }
            let mut value = PDH_FMT_COUNTERVALUE::default();
            // SAFETY: as above; `value` is a valid, writable structure.
            let r =
                unsafe { PdhGetFormattedCounterValue(counter, PDH_FMT_DOUBLE, None, &mut value) };
            if r != 0 {
                return None;
            }
            // SAFETY: PDH_FMT_DOUBLE fills the double of the union.
            let idle = unsafe { value.Anonymous.doubleValue };
            Some((100.0 - idle).clamp(0.0, 100.0))
        }
    }

    impl Drop for DiskQuery {
        fn drop(&mut self) {
            if let Some((query, _)) = self.query.take() {
                // SAFETY: the query was opened by `new` and is closed once.
                unsafe { PdhCloseQuery(query) };
            }
        }
    }

    /// This process's engines of the graphics cards, through the "GPU
    /// Engine" counters (an instance per engine it used, named
    /// `pid_<pid>_luid_<card>_phys_<n>_eng_<n>_engtype_<kind>`).
    #[derive(Debug)]
    pub struct GpuQuery {
        query: Option<(PDH_HQUERY, PDH_HCOUNTER)>,
        primed: bool,
    }

    impl GpuQuery {
        pub fn new() -> GpuQuery {
            let path = format!(
                "\\GPU Engine(pid_{}_*)\\Utilization Percentage",
                std::process::id()
            );
            let wide: Vec<u16> = path.encode_utf16().chain([0]).collect();
            let mut query = PDH_HQUERY::default();
            let mut counter = PDH_HCOUNTER::default();
            // SAFETY: valid out-pointers; `wide` is NUL-terminated and lives
            // through the call.
            let opened = unsafe { PdhOpenQueryW(None, 0, &mut query) } == 0;
            let added = opened
                && unsafe { PdhAddEnglishCounterW(query, PCWSTR(wide.as_ptr()), 0, &mut counter) }
                    == 0;
            if opened && !added {
                // SAFETY: opened just above, closed once.
                unsafe { PdhCloseQuery(query) };
            }
            GpuQuery {
                query: added.then_some((query, counter)),
                primed: false,
            }
        }

        pub fn sample(&mut self) -> Option<f64> {
            let (query, counter) = self.query?;
            // SAFETY: the query and counter are open until drop.
            if unsafe { PdhCollectQueryData(query) } != 0 {
                return None;
            }
            if !self.primed {
                // A rate needs two samples.
                self.primed = true;
                return None;
            }
            let (mut size, mut count) = (0u32, 0u32);
            // SAFETY: asks for the size of the instances' values.
            let r = unsafe {
                PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, None)
            };
            if r != PDH_MORE_DATA || count == 0 {
                // No engine used yet.
                return Some(0.0);
            }
            // Room for the items and their names, aligned for the items.
            let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
            let items = buffer.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
            // SAFETY: `buffer` holds `size` bytes, aligned to 8.
            let r = unsafe {
                PdhGetFormattedCounterArrayW(
                    counter,
                    PDH_FMT_DOUBLE,
                    &mut size,
                    &mut count,
                    Some(items),
                )
            };
            if r != 0 {
                return None;
            }
            // SAFETY: PDH wrote `count` items at the start of `buffer`.
            let items = unsafe { std::slice::from_raw_parts(items, count as usize) };
            let mut engines: Vec<(String, f64)> = Vec::new();
            for item in items {
                // SAFETY: the names are NUL-terminated strings inside
                // `buffer`.
                let name = unsafe { item.szName.to_string() }.unwrap_or_default();
                if item.FmtValue.CStatus > 1 {
                    continue;
                }
                // SAFETY: PDH_FMT_DOUBLE fills the double of the union.
                let value = unsafe { item.FmtValue.Anonymous.doubleValue };
                super::add_engine(&mut engines, &name, value);
            }
            Some(super::busiest(&engines))
        }
    }

    impl Drop for GpuQuery {
        fn drop(&mut self) {
            if let Some((query, _)) = self.query.take() {
                // SAFETY: the query was opened by `new` and is closed once.
                unsafe { PdhCloseQuery(query) };
            }
        }
    }

    pub fn taskbar_progress(title: &str, progress: Option<f32>) {
        let Some(hwnd) = super::super::find_window(title) else {
            return;
        };
        let hwnd = HWND(hwnd as *mut _);
        // SAFETY: COM on this thread (S_FALSE/RPC_E_CHANGED_MODE: it already
        // has it), then calls on a live window of this process.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let Ok(bar) =
                CoCreateInstance::<_, ITaskbarList3>(&TaskbarList, None, CLSCTX_INPROC_SERVER)
            else {
                return;
            };
            if bar.HrInit().is_err() {
                return;
            }
            match progress {
                Some(p) => {
                    let _ = bar.SetProgressState(hwnd, TBPF_NORMAL);
                    let _ = bar.SetProgressValue(hwnd, (p.clamp(0.0, 1.0) * 1000.0) as u64, 1000);
                }
                None => {
                    let _ = bar.SetProgressState(hwnd, TBPF_NOPROGRESS);
                }
            }
        }
    }

    pub fn notify_sound() {
        // SAFETY: a system sound alias, played asynchronously.
        let _ = unsafe { PlaySoundW(w!("SystemAsterisk"), None, SND_ALIAS | SND_ASYNC) };
    }
}

/// Linux: the kernel's counters (`/proc`): this process's processor time
/// and memory, the disks' busy time, and the graphics engines' time this
/// process used (the DRM drivers' `fdinfo`).
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
mod imp {
    use std::time::Instant;

    fn sysconf(name: libc::c_int) -> Option<f64> {
        // SAFETY: a plain query of a system constant.
        let v = unsafe { libc::sysconf(name) };
        (v > 0).then_some(v as f64)
    }

    pub fn process_cpu_seconds() -> Option<f64> {
        let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
        // After the command name (in parentheses, maybe with spaces):
        // state, …, utime and stime (the line's 14th and 15th fields).
        let rest = stat.get(stat.rfind(')')? + 2..)?;
        let f: Vec<&str> = rest.split_whitespace().collect();
        let ticks = f.get(11)?.parse::<f64>().ok()? + f.get(12)?.parse::<f64>().ok()?;
        Some(ticks / sysconf(libc::_SC_CLK_TCK)?)
    }

    pub fn process_memory() -> Option<u64> {
        let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
        let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
        Some(pages * sysconf(libc::_SC_PAGESIZE)? as u64)
    }

    /// Counters that grow with busy time: between two samples, the busiest
    /// one's share of the time passed (`unit` counts per second).
    #[derive(Debug)]
    struct Busy {
        last: Option<(Instant, Vec<(String, f64)>)>,
        unit: f64,
    }

    impl Busy {
        fn sample(&mut self, now: Vec<(String, f64)>) -> Option<f64> {
            let at = Instant::now();
            let share = self.last.as_ref().and_then(|(t, before)| {
                let wall = at.duration_since(*t).as_secs_f64() * self.unit;
                (wall > 0.0).then(|| {
                    now.iter()
                        .filter_map(|(k, v)| {
                            let b = before.iter().find(|(bk, _)| bk == k)?.1;
                            Some((v - b).max(0.0) / wall * 100.0)
                        })
                        .fold(0.0, f64::max)
                        .clamp(0.0, 100.0)
                })
            });
            self.last = Some((at, now));
            share
        }
    }

    /// Each whole disk's milliseconds spent on input and output
    /// (`/proc/diskstats`' 13th field).
    fn disk_times() -> Vec<(String, f64)> {
        let Ok(text) = std::fs::read_to_string("/proc/diskstats") else {
            return Vec::new();
        };
        text.lines()
            .filter_map(|l| {
                let f: Vec<&str> = l.split_whitespace().collect();
                let name = *f.get(2)?;
                let whole = std::path::Path::new("/sys/block").join(name).exists();
                let virt = ["loop", "ram", "zram"].iter().any(|p| name.starts_with(p));
                (whole && !virt).then(|| Some((name.to_string(), f.get(12)?.parse().ok()?)))?
            })
            .collect()
    }

    #[derive(Debug)]
    pub struct DiskQuery(Busy);

    impl DiskQuery {
        pub fn new() -> DiskQuery {
            DiskQuery(Busy {
                last: None,
                unit: 1000.0,
            })
        }

        pub fn sample(&mut self) -> Option<f64> {
            let times = disk_times();
            if times.is_empty() {
                return None;
            }
            self.0.sample(times)
        }
    }

    /// The nanoseconds each graphics engine worked for this process, per
    /// card (`drm-engine-render: 1234 ns` in the open device's `fdinfo`),
    /// divided among the engines of a kind.
    fn gpu_times() -> Vec<(String, f64)> {
        let Ok(dir) = std::fs::read_dir("/proc/self/fdinfo") else {
            return Vec::new();
        };
        let mut seen = std::collections::HashSet::new();
        let mut engines: Vec<(String, f64)> = Vec::new();
        for e in dir.flatten() {
            let Ok(text) = std::fs::read_to_string(e.path()) else {
                continue;
            };
            let field = |k: &str| {
                text.lines()
                    .find_map(|l| l.strip_prefix(k))
                    .map(|v| v.trim().to_string())
            };
            let Some(client) = field("drm-client-id:") else {
                continue;
            };
            let card = field("drm-pdev:").unwrap_or_default();
            // A client shows in each of its open files: once.
            if !seen.insert((card.clone(), client)) {
                continue;
            }
            for l in text.lines() {
                let Some(rest) = l.strip_prefix("drm-engine-") else {
                    continue;
                };
                let Some((name, value)) = rest.split_once(':') else {
                    continue;
                };
                if name.starts_with("capacity-") {
                    continue;
                }
                let ns: f64 = value
                    .trim()
                    .trim_end_matches("ns")
                    .trim()
                    .parse()
                    .unwrap_or(0.0);
                let capacity = field(&format!("drm-engine-capacity-{name}:"))
                    .and_then(|c| c.parse::<f64>().ok())
                    .unwrap_or(1.0)
                    .max(1.0);
                let key = format!("{card}/{name}");
                match engines.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, v)) => *v += ns / capacity,
                    None => engines.push((key, ns / capacity)),
                }
            }
        }
        engines
    }

    #[derive(Debug)]
    pub struct GpuQuery(Busy);

    impl GpuQuery {
        pub fn new() -> GpuQuery {
            GpuQuery(Busy {
                last: None,
                unit: 1e9,
            })
        }

        pub fn sample(&mut self) -> Option<f64> {
            let times = gpu_times();
            if times.is_empty() {
                return None;
            }
            self.0.sample(times)
        }
    }

    /// Linux desktops have no standard progress on their task bars.
    pub fn taskbar_progress(_title: &str, _progress: Option<f32>) {}

    /// The desktop's "complete" sound, through whichever player there is.
    pub fn notify_sound() {
        use std::process::{Command, Stdio};
        let sound = "/usr/share/sounds/freedesktop/stereo/complete.oga";
        let tries: [(&str, &[&str]); 3] = [
            ("canberra-gtk-play", &["--id", "complete"]),
            ("paplay", &[sound]),
            ("pw-play", &[sound]),
        ];
        for (program, args) in tries {
            let child = Command::new(program)
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            if let Ok(mut child) = child {
                // Waited for elsewhere, so it does not linger.
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                return;
            }
        }
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod imp {
    pub fn process_cpu_seconds() -> Option<f64> {
        None
    }

    pub fn process_memory() -> Option<u64> {
        None
    }

    #[derive(Debug)]
    pub struct DiskQuery;

    impl DiskQuery {
        pub fn new() -> DiskQuery {
            DiskQuery
        }

        pub fn sample(&mut self) -> Option<f64> {
            None
        }
    }

    #[derive(Debug)]
    pub struct GpuQuery;

    impl GpuQuery {
        pub fn new() -> GpuQuery {
            GpuQuery
        }

        pub fn sample(&mut self) -> Option<f64> {
            None
        }
    }

    pub fn taskbar_progress(_title: &str, _progress: Option<f32>) {}

    pub fn notify_sound() {}
}

/// Adds an instance's share to its engine's (`…_luid_<card>_phys_<n>_eng_<n>`):
/// an engine can show in several instances.
#[cfg_attr(not(windows), allow(dead_code))]
fn add_engine(engines: &mut Vec<(String, f64)>, instance: &str, share: f64) {
    let start = instance.find("luid_").unwrap_or(0);
    let end = instance.find("_engtype").unwrap_or(instance.len());
    let key = instance.get(start..end).unwrap_or(instance);
    match engines.iter_mut().find(|(k, _)| k == key) {
        Some((_, v)) => *v += share,
        None => engines.push((key.to_string(), share)),
    }
}

/// The busiest engine's share, within 0–100 %.
#[cfg_attr(not(windows), allow(dead_code))]
fn busiest(engines: &[(String, f64)]) -> f64 {
    engines
        .iter()
        .map(|(_, v)| *v)
        .fold(0.0, f64::max)
        .clamp(0.0, 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_graphics_card_share_is_its_busiest_engine() {
        let mut engines = Vec::new();
        let pid = "pid_42_luid_0x00000000_0x0000F6AE_phys_0";
        add_engine(&mut engines, &format!("{pid}_eng_0_engtype_3D"), 12.5);
        add_engine(&mut engines, &format!("{pid}_eng_0_engtype_3D"), 7.5);
        add_engine(
            &mut engines,
            &format!("{pid}_eng_3_engtype_VideoDecode"),
            15.0,
        );
        assert_eq!(engines.len(), 2);
        assert_eq!(busiest(&engines), 20.0);
        assert_eq!(busiest(&[]), 0.0);
    }

    #[test]
    fn usage_is_measured_on_windows() {
        let mut u = Usage::new();
        assert!(u.cpu().is_none(), "the first call has nothing to compare");
        // Some work, then a share.
        let mut x = 0u64;
        for i in 0..5_000_000u64 {
            x = x.wrapping_add(i * i);
        }
        std::hint::black_box(x);
        std::thread::sleep(std::time::Duration::from_millis(50));
        if cfg!(any(windows, target_os = "linux")) {
            let cpu = u.cpu().expect("processor share");
            assert!((0.0..=100.0).contains(&cpu), "{cpu}");
            assert!(u.memory().is_some_and(|m| m > 1 << 20));
            // The graphics card's counters may be missing (old systems).
            assert!(u.gpu().is_none(), "the first call has nothing to compare");
            if let Some(gpu) = u.gpu() {
                assert!((0.0..=100.0).contains(&gpu), "{gpu}");
            }
        }
    }
}

#[cfg(test)]
mod disk_tests {
    #[test]
    #[ignore = "prints this computer's disk activity"]
    fn disk_activity() {
        let mut u = super::Usage::new();
        assert!(u.disk().is_none());
        for _ in 0..3 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            eprintln!(
                "disk {:?} cpu {:?} memory {:?}",
                u.disk(),
                u.cpu(),
                u.memory()
            );
        }
    }
}
