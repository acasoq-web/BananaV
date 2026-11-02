//! VST 3's objects as COM's: interface pointers to a plug-in's objects
//! ([`ComPtr`], counting references), and objects of the host's own that
//! plug-ins hold ([`Own`] of a [`HostObject`]): their tables first, then
//! their count of references and their data.

#![allow(unsafe_code)]

use std::marker::PhantomData;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU32, Ordering};

use bv_vst3_sys::{FUnknownVtbl, Raw, TResult, Tuid, iid, result};

/// A reference to an interface of a plug-in's object: released when
/// dropped. `V` is the interface's table, beginning with FUnknown's.
#[derive(Debug)]
pub(crate) struct ComPtr<V> {
    ptr: NonNull<std::ffi::c_void>,
    _table: PhantomData<*const V>,
}

impl<V> ComPtr<V> {
    /// Takes over a reference the plug-in handed out (`None` for null).
    ///
    /// # Safety
    /// `raw` is null or an interface pointer whose table is a `V`.
    pub(crate) unsafe fn from_raw(raw: Raw) -> Option<ComPtr<V>> {
        NonNull::new(raw).map(|ptr| ComPtr {
            ptr,
            _table: PhantomData,
        })
    }

    pub(crate) fn raw(&self) -> Raw {
        self.ptr.as_ptr()
    }

    /// The interface's table.
    pub(crate) fn vt(&self) -> &V {
        // SAFETY: an interface pointer points at the pointer to its table,
        // a `V` (`from_raw`'s contract), alive while the object is.
        unsafe { &**(self.ptr.as_ptr() as *const *const V) }
    }

    fn unknown(&self) -> &FUnknownVtbl {
        // SAFETY: every table begins with FUnknown's.
        unsafe { &**(self.ptr.as_ptr() as *const *const FUnknownVtbl) }
    }

    /// Another interface of the object, `W` its table.
    pub(crate) fn query<W>(&self, id: &Tuid) -> Option<ComPtr<W>> {
        let mut out: Raw = std::ptr::null_mut();
        // SAFETY: the plug-in's queryInterface fills `out` with an
        // interface pointer (a reference of its own) when it answers.
        let r = unsafe { (self.unknown().query_interface)(self.raw(), id, &mut out) };
        if r == result::OK {
            // SAFETY: the object says `out` is the interface `id`, whose
            // table the caller names as `W`.
            unsafe { ComPtr::from_raw(out) }
        } else {
            None
        }
    }
}

impl<V> Clone for ComPtr<V> {
    fn clone(&self) -> ComPtr<V> {
        // SAFETY: a live interface pointer.
        unsafe { (self.unknown().add_ref)(self.raw()) };
        ComPtr {
            ptr: self.ptr,
            _table: PhantomData,
        }
    }
}

impl<V> Drop for ComPtr<V> {
    fn drop(&mut self) {
        // SAFETY: the reference this pointer holds, given back once.
        unsafe { (self.unknown().release)(self.raw()) };
    }
}

/// The size of a table's pointer: an object's tables are this far apart.
const PTR: usize = std::mem::size_of::<*const ()>();

/// An object of the host's that plug-ins hold. It begins with its tables'
/// pointers (`#[repr(C)]`), FUnknown's answers on the first one.
pub(crate) trait HostObject: Sized + Send + Sync + 'static {
    /// The table that answers interface `id` (its index among the
    /// object's tables).
    fn table(id: &Tuid) -> Option<usize>;
    fn refs(&self) -> &AtomicU32;
}

/// FUnknown's functions for table `K` of objects `T` (a table's functions
/// get the pointer to that table, `K` pointers into the object).
pub(crate) unsafe extern "system" fn query<T: HostObject, const K: usize>(
    this: Raw,
    id: *const Tuid,
    obj: *mut Raw,
) -> TResult {
    if this.is_null() || id.is_null() || obj.is_null() {
        return result::INVALID_ARGUMENT;
    }
    // SAFETY: `this` is table `K` of a `T` (only `T`'s tables hold these
    // functions); `id` points at 16 bytes and `obj` at a pointer.
    unsafe {
        let base = this.cast::<u8>().sub(K * PTR).cast::<T>();
        let id = &*id;
        let table = if *id == iid::FUNKNOWN {
            Some(0)
        } else {
            T::table(id)
        };
        match table {
            Some(i) => {
                (*base).refs().fetch_add(1, Ordering::AcqRel);
                *obj = base.cast::<u8>().add(i * PTR).cast();
                result::OK
            }
            None => {
                *obj = std::ptr::null_mut();
                result::NO_INTERFACE
            }
        }
    }
}

pub(crate) unsafe extern "system" fn add_ref<T: HostObject, const K: usize>(this: Raw) -> u32 {
    // SAFETY: as `query`.
    unsafe {
        let base = this.cast::<u8>().sub(K * PTR).cast::<T>();
        (*base).refs().fetch_add(1, Ordering::AcqRel) + 1
    }
}

pub(crate) unsafe extern "system" fn release<T: HostObject, const K: usize>(this: Raw) -> u32 {
    // SAFETY: as `query`; the last reference frees the object, made by
    // `Own::new` as a box.
    unsafe {
        let base = this.cast::<u8>().sub(K * PTR).cast::<T>();
        let left = (*base).refs().fetch_sub(1, Ordering::AcqRel) - 1;
        if left == 0 {
            drop(Box::from_raw(base));
        }
        left
    }
}

/// FUnknown's table for table `K` of objects `T`.
pub(crate) const fn unknown<T: HostObject, const K: usize>() -> FUnknownVtbl {
    FUnknownVtbl {
        query_interface: query::<T, K>,
        add_ref: add_ref::<T, K>,
        release: release::<T, K>,
    }
}

/// The host's reference to one of its objects (it begins with one).
#[derive(Debug)]
pub(crate) struct Own<T: HostObject>(NonNull<T>);

// SAFETY: host objects are `Send + Sync` (their data behind locks).
unsafe impl<T: HostObject> Send for Own<T> {}
// SAFETY: as above.
unsafe impl<T: HostObject> Sync for Own<T> {}

impl<T: HostObject> Own<T> {
    /// A new object, its count of references at one (the host's).
    pub(crate) fn new(obj: T) -> Own<T> {
        obj.refs().store(1, Ordering::Release);
        Own(NonNull::from(Box::leak(Box::new(obj))))
    }

    pub(crate) fn get(&self) -> &T {
        // SAFETY: the host's reference keeps the object alive.
        unsafe { self.0.as_ref() }
    }

    /// An interface pointer to table `k`, for a call: the plug-in counts
    /// its own reference if it keeps it.
    pub(crate) fn raw(&self, k: usize) -> Raw {
        // SAFETY: the object's first fields are its tables' pointers.
        unsafe { self.0.as_ptr().cast::<u8>().add(k * PTR).cast() }
    }
}

impl<T: HostObject> Drop for Own<T> {
    fn drop(&mut self) {
        let left = self.get().refs().fetch_sub(1, Ordering::AcqRel) - 1;
        if left == 0 {
            // SAFETY: the last reference: the box `new` made.
            unsafe { drop(Box::from_raw(self.0.as_ptr())) };
        }
    }
}
