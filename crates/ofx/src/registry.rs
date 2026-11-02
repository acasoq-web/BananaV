//! The plug-ins BananaV knows: what scans found and the plug-ins built
//! into the program, loaded into this process when first used.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, RwLock};

use crate::info::{Context, Error, PluginInfo};
use crate::plugin::{Binary, Plugin};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// A binary loaded here (or why it failed) and its plug-ins loaded so far,
/// by number: each loads when first used, once — the first who needs it
/// loads it and others wait for it alone, never under the registry's lock
/// (a plug-in describing itself may take a while: lookups of others go
/// on meanwhile).
#[derive(Debug, Default)]
struct Loaded {
    binary: Once<Arc<Binary>>,
    plugins: HashMap<u32, Once<Arc<Plugin>>>,
}

/// Something loaded once (or why it failed to).
type Once<T> = Arc<OnceLock<Result<T, Error>>>;

/// A cell holding `value` already.
fn holding<T>(value: T) -> Arc<OnceLock<T>> {
    let cell = OnceLock::new();
    let _ = cell.set(value);
    Arc::new(cell)
}

/// The known plug-ins and the binaries loaded for them.
#[derive(Debug, Default)]
pub struct Registry {
    built_in: RwLock<Vec<Arc<PluginInfo>>>,
    scanned: RwLock<Vec<Arc<PluginInfo>>>,
    /// Every known plug-in, worked out again when either list changes:
    /// lookups run every frame, for every effect drawn, and one binary of
    /// hundreds of plug-ins (Sapphire's) is megabytes of descriptions, so
    /// they are shared, never copied.
    known: RwLock<Known>,
    /// Binaries loaded here, and their plug-ins (or why they failed).
    loaded: Mutex<HashMap<PathBuf, Loaded>>,
    /// Bumps whenever the known plug-ins change.
    generation: AtomicU64,
}

/// The known plug-ins — the built-in ones, then the scanned ones — and by
/// identifier.
#[derive(Debug, Default)]
struct Known {
    list: Vec<Arc<PluginInfo>>,
    by_id: HashMap<String, Arc<PluginInfo>>,
}

fn read<T>(l: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    l.read().unwrap_or_else(|e| e.into_inner())
}

fn write<T>(l: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    l.write().unwrap_or_else(|e| e.into_inner())
}

/// The program's registry.
pub fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(Registry::default)
}

impl Registry {
    /// The plug-ins scans found (their binaries load when first used).
    pub fn set_scanned(&self, infos: Vec<PluginInfo>) {
        *write(&self.scanned) = infos.into_iter().map(Arc::new).collect();
        self.changed();
    }

    /// The known plug-ins worked out again from the lists.
    fn changed(&self) {
        let built_in = read(&self.built_in);
        let scanned = read(&self.scanned);
        let mut ids = HashSet::new();
        let list: Vec<Arc<PluginInfo>> = built_in
            .iter()
            .chain(scanned.iter())
            .filter(|p| ids.insert(p.identifier.as_str()))
            .cloned()
            .collect();
        let by_id = list
            .iter()
            .map(|p| (p.identifier.clone(), Arc::clone(p)))
            .collect();
        drop(ids);
        *write(&self.known) = Known { list, by_id };
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    /// Adds plug-ins built into the program (described now).
    pub fn add_built_in(&self, binary: Arc<Binary>) -> Result<(), Error> {
        let (plugins, _) = binary.plugins()?;
        let infos: Vec<Arc<PluginInfo>> =
            plugins.iter().map(|p| Arc::new(p.info.clone())).collect();
        let plugins = plugins
            .into_iter()
            .map(|p| (p.info.index, holding(Ok(p))))
            .collect();
        lock(&self.loaded).insert(
            binary.path.clone(),
            Loaded {
                binary: holding(Ok(binary)),
                plugins,
            },
        );
        let mut built_in = write(&self.built_in);
        built_in.retain(|k| !infos.iter().any(|i| i.identifier == k.identifier));
        built_in.extend(infos);
        drop(built_in);
        self.changed();
        Ok(())
    }

    /// Changes whenever the known plug-ins do.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    /// Every known plug-in: the built-in ones, then the scanned ones.
    pub fn known(&self) -> Vec<Arc<PluginInfo>> {
        read(&self.known).list.clone()
    }

    /// The known plug-ins that run in `context`.
    pub fn in_context(&self, context: Context) -> Vec<Arc<PluginInfo>> {
        read(&self.known)
            .list
            .iter()
            .filter(|p| p.contexts.contains(&context))
            .cloned()
            .collect()
    }

    /// The known plug-in `id`.
    pub fn info(&self, id: &str) -> Option<Arc<PluginInfo>> {
        read(&self.known).by_id.get(id).cloned()
    }

    /// The plug-in `id`, loaded here (alone, its binary too) when it was
    /// not.
    pub fn plugin(&self, id: &str) -> Result<Arc<Plugin>, Error> {
        let info = self
            .info(id)
            .ok_or_else(|| Error::Other(format!("no plug-in {id}")))?;
        let (binary, plugin) = {
            let mut loaded = lock(&self.loaded);
            let loaded = loaded.entry(info.file.clone()).or_default();
            let plugin = loaded.plugins.entry(info.index).or_default();
            (Arc::clone(&loaded.binary), Arc::clone(plugin))
        };
        let binary = binary.get_or_init(|| Binary::load(&info.file)).clone()?;
        match plugin.get_or_init(|| binary.plugin(info.index)) {
            Ok(p) if p.info.identifier == id => Ok(Arc::clone(p)),
            Ok(_) => Err(Error::Other(format!("{id} is no longer in its binary"))),
            Err(e) => Err(e.clone()),
        }
    }

    /// Tells every plug-in loaded here it is done with ([`Plugin::unload`]):
    /// BananaV is closing, its windows and engine gone.
    pub fn unload_all(&self) {
        let plugins: Vec<Arc<Plugin>> = lock(&self.loaded)
            .values()
            .flat_map(|l| l.plugins.values())
            .filter_map(|p| p.get()?.as_ref().ok().cloned())
            .collect();
        for p in plugins {
            p.unload();
        }
    }
}

/// Unloads every plug-in loaded ([`Registry::unload_all`]) on a thread of
/// its own, waiting `patience` at most: a plug-in that hangs there does not
/// keep BananaV from closing.
pub fn unload_all(patience: std::time::Duration) {
    let (done, wait) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("bv-ofx-unload".into())
        .stack_size(crate::PLUGIN_STACK)
        .spawn(move || {
            registry().unload_all();
            let _ = done.send(());
        });
    if spawned.is_ok() {
        let _ = wait.recv_timeout(patience);
    }
}
