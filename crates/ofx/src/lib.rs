//! BananaV's OpenFX host: it finds OFX image effect plug-ins (filters,
//! transitions, generators), describes them, and runs them on BananaV's
//! pictures.
//!
//! - [`plugin`]: loading binaries and the plug-ins in them;
//! - [`instance`]: an effect applied somewhere, rendering and answering
//!   changes;
//! - [`param`], [`props`], [`effect`], [`suites`]: the specification's
//!   objects and suites, as plug-ins see them.

pub mod effect;
#[cfg(windows)]
mod gl;
mod guard;
pub mod info;
pub mod instance;
pub mod interact;
pub mod param;
pub mod pixels;
pub mod plugin;
pub mod props;
pub mod registry;
pub mod scan;
pub mod suites;
pub mod templates;
pub mod value;

mod printf;
mod varargs;

pub use effect::ClipSource;

/// Gives plug-ins the program's main window as the host's
/// `kOfxPropHostOSHandle` (its HWND on Windows): the parent of the dialogs
/// they open themselves, which then stay in front of it.
pub fn set_host_window(handle: isize) {
    suites::host().props.set_ptr(
        bv_ofx_sys::names::kOfxPropHostOSHandle,
        handle as *const std::ffi::c_void,
    );
}

/// Whether overlays that draw with OpenGL themselves (OpenFX's first kind)
/// can be shown here: on Windows, with the graphics card's OpenGL.
pub fn opengl_overlays() -> bool {
    #[cfg(windows)]
    return gl::available();
    #[cfg(not(windows))]
    false
}
pub use guard::{Crash, take_crashes};

/// The stack a thread that calls plug-ins should have, as hosts give
/// their render threads: plug-ins' code may recurse deeply, and Windows'
/// main thread has 1 MB. It is only reserved address space until used.
pub const PLUGIN_STACK: usize = 16 << 20;
pub use info::{ClipInfo, Context, ContextInfo, Error, ParamInfo, PluginInfo};
pub use instance::{ChangeReason, Instance, PenAnswer, RenderRequest, Setup};
pub use interact::{Mark, Pen, PenAction, View, take_redraw};
pub use pixels::{Components, Depth, DevicePicture, Format, Premult};
pub use plugin::{Binary, Failure, Plugin};
pub use registry::{registry, unload_all};
pub use suites::{Message, Severity, missing_suites, recent_messages};
pub use value::{Animation, Handles, Interp, Key, ParamType, Value};
