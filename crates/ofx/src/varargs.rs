//! Receiving C's variadic calls (`...`) in stable Rust.
//!
//! The parameter suite's value functions and the message suite are
//! variadic: a plug-in passes as many values as the parameter has, of its
//! type. Stable Rust cannot define a variadic function, so the host defines
//! ordinary ones whose arguments sit where each platform's calling
//! convention puts a variadic call's values, and reads them in order:
//!
//! - 64-bit Windows (x64 and ARM64): every argument takes the next 64-bit
//!   slot (a register, then the stack). A `double` among the variadic
//!   values is in its integer slot as well (x64 copies it there, ARM64 puts
//!   every argument of a variadic call in integer registers, named doubles
//!   included): slots are read as integers, doubles from their bits.
//! - Apple's chips (macOS on ARM64): the named arguments take their
//!   registers as usual, and every variadic value goes on the stack, each
//!   in a 64-bit slot of its own (an `int` in the slot's low half, a
//!   `double` as its bits). The function takes integer arguments until the
//!   eight integer registers are full; the ones after them are the stack's
//!   slots, read as on Windows.
//! - Linux on x86-64 and AArch64, and Intel Macs (System V): integers and
//!   pointers take the next integer register, doubles the next
//!   floating-point one, whatever their order: integer slots and
//!   floating-point slots are read apart.
//!
//! Slots a call did not fill hold whatever was there (a register, the
//! caller's stack) and are never read: a parameter's type says how many
//! values to read. The test plug-ins (`bv-ofx-testplug`) call these
//! functions through real variadic function pointers, so the tests check
//! the convention on every platform the CI builds.
#![allow(unsafe_code)]

/// How many integer (or, on Windows and Apple's chips, any) slots a
/// function reads.
pub(crate) const SLOTS: usize = 8;

/// A variadic call's values, read in order.
#[derive(Debug)]
pub(crate) struct Va {
    ints: [u64; SLOTS],
    next_int: usize,
    #[cfg(not(any(windows, all(target_vendor = "apple", target_arch = "aarch64"))))]
    floats: [f64; SLOTS],
    #[cfg(not(any(windows, all(target_vendor = "apple", target_arch = "aarch64"))))]
    next_float: usize,
}

impl Va {
    #[cfg(any(windows, all(target_vendor = "apple", target_arch = "aarch64")))]
    pub(crate) fn new(ints: [u64; SLOTS]) -> Va {
        Va { ints, next_int: 0 }
    }

    #[cfg(not(any(windows, all(target_vendor = "apple", target_arch = "aarch64"))))]
    pub(crate) fn new(ints: [u64; SLOTS], floats: [f64; SLOTS]) -> Va {
        Va {
            ints,
            next_int: 0,
            floats,
            next_float: 0,
        }
    }

    /// The next integer slot (zero past the last).
    pub(crate) fn word(&mut self) -> u64 {
        let v = self.ints.get(self.next_int).copied().unwrap_or(0);
        self.next_int += 1;
        v
    }

    /// The next value, a pointer.
    pub(crate) fn ptr<T>(&mut self) -> *mut T {
        self.word() as usize as *mut T
    }

    /// The next value, an `int` (C passes it in the low half of a slot).
    pub(crate) fn int(&mut self) -> i32 {
        self.word() as u32 as i32
    }

    /// The next value, a `double`.
    pub(crate) fn double(&mut self) -> f64 {
        #[cfg(any(windows, all(target_vendor = "apple", target_arch = "aarch64")))]
        {
            f64::from_bits(self.word())
        }
        #[cfg(not(any(windows, all(target_vendor = "apple", target_arch = "aarch64"))))]
        {
            let v = self.floats.get(self.next_float).copied().unwrap_or(0.0);
            self.next_float += 1;
            v
        }
    }
}

/// Defines `unsafe extern "C" fn $name` that receives a variadic call
/// whose named arguments are the pointers `$p` then the doubles `$d`, and
/// passes them with the variadic values ([`Va`]) to `$imp`.
macro_rules! variadic {
    ($(#[$meta:meta])* fn $name:ident($($p:ident),+ $(; $($d:ident),+)?) => $imp:path) => {
        #[cfg(all(windows, target_arch = "x86_64"))]
        $(#[$meta])*
        #[allow(clippy::too_many_arguments)]
        pub(crate) unsafe extern "C" fn $name(
            $($p: *mut ::std::ffi::c_void,)+
            $($($d: f64,)+)?
            s0: u64, s1: u64, s2: u64, s3: u64, s4: u64, s5: u64, s6: u64, s7: u64,
        ) -> ::bv_ofx_sys::OfxStatus {
            let va = $crate::varargs::Va::new([s0, s1, s2, s3, s4, s5, s6, s7]);
            // SAFETY: the plug-in's arguments, as `$imp` expects them.
            unsafe { $imp($($p,)+ $($($d,)+)? va) }
        }

        #[cfg(all(windows, not(target_arch = "x86_64")))]
        $(#[$meta])*
        #[allow(clippy::too_many_arguments)]
        pub(crate) unsafe extern "C" fn $name(
            $($p: u64,)+
            $($($d: u64,)+)?
            s0: u64, s1: u64, s2: u64, s3: u64, s4: u64, s5: u64, s6: u64, s7: u64,
        ) -> ::bv_ofx_sys::OfxStatus {
            let va = $crate::varargs::Va::new([s0, s1, s2, s3, s4, s5, s6, s7]);
            // SAFETY: the plug-in's arguments, as `$imp` expects them.
            unsafe {
                $imp(
                    $($p as usize as *mut ::std::ffi::c_void,)+
                    $($(f64::from_bits($d),)+)?
                    va,
                )
            }
        }

        #[cfg(all(target_vendor = "apple", target_arch = "aarch64"))]
        $(#[$meta])*
        #[allow(clippy::too_many_arguments)]
        pub(crate) unsafe extern "C" fn $name(
            $($p: *mut ::std::ffi::c_void,)+
            $($($d: f64,)+)?
            r0: u64, r1: u64, r2: u64, r3: u64, r4: u64, r5: u64, r6: u64, r7: u64,
            s0: u64, s1: u64, s2: u64, s3: u64, s4: u64, s5: u64, s6: u64, s7: u64,
        ) -> ::bv_ofx_sys::OfxStatus {
            // The named pointers and the first `r…` fill the eight integer
            // registers; the arguments after them are the stack's first
            // slots, where the call's values are.
            let named = [$(stringify!($p)),+].len();
            let all = [r0, r1, r2, r3, r4, r5, r6, r7, s0, s1, s2, s3, s4, s5, s6, s7];
            let mut slots = [0; $crate::varargs::SLOTS];
            slots.copy_from_slice(&all[8 - named..][..$crate::varargs::SLOTS]);
            let va = $crate::varargs::Va::new(slots);
            // SAFETY: the plug-in's arguments, as `$imp` expects them.
            unsafe { $imp($($p,)+ $($($d,)+)? va) }
        }

        #[cfg(not(any(windows, all(target_vendor = "apple", target_arch = "aarch64"))))]
        $(#[$meta])*
        #[allow(clippy::too_many_arguments)]
        pub(crate) unsafe extern "C" fn $name(
            $($p: *mut ::std::ffi::c_void,)+
            $($($d: f64,)+)?
            i0: u64, i1: u64, i2: u64, i3: u64, i4: u64, i5: u64, i6: u64, i7: u64,
            f0: f64, f1: f64, f2: f64, f3: f64, f4: f64, f5: f64, f6: f64, f7: f64,
        ) -> ::bv_ofx_sys::OfxStatus {
            let va = $crate::varargs::Va::new(
                [i0, i1, i2, i3, i4, i5, i6, i7],
                [f0, f1, f2, f3, f4, f5, f6, f7],
            );
            // SAFETY: the plug-in's arguments, as `$imp` expects them.
            unsafe { $imp($($p,)+ $($($d,)+)? va) }
        }
    };
}

pub(crate) use variadic;

/// A function defined with [`variadic!`] as the variadic type a suite's
/// table holds.
///
/// # Safety
/// `f` is a function from [`variadic!`] whose named arguments match `G`'s.
pub(crate) unsafe fn as_variadic<G: Copy>(f: *const ()) -> G {
    assert_eq!(size_of::<G>(), size_of::<*const ()>());
    // SAFETY: both are function pointers; the calling conventions agree, as
    // the module explains.
    unsafe { std::mem::transmute_copy(&f) }
}

#[cfg(test)]
mod tests {
    use std::ffi::c_void;

    use bv_ofx_sys::OfxStatus;

    use super::*;

    /// What the test function saw.
    static SEEN: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

    unsafe fn record(h: *mut c_void, t: f64, mut va: Va) -> OfxStatus {
        let line = format!(
            "{} {t} {} {} {}",
            h as usize,
            va.int(),
            va.double(),
            // SAFETY: the test passes a C string.
            unsafe { std::ffi::CStr::from_ptr(va.ptr::<std::ffi::c_char>()) }.to_string_lossy()
        );
        SEEN.lock().unwrap().push(line);
        0
    }

    variadic!(fn received(h; t) => record);

    #[test]
    fn variadic_calls_arrive_in_order() {
        type Variadic = unsafe extern "C" fn(*mut c_void, f64, ...) -> OfxStatus;
        // SAFETY: `received` takes a pointer and a double before the `...`.
        let f: Variadic = unsafe { as_variadic(received as *const ()) };
        // SAFETY: a real variadic call, as a plug-in makes it.
        let status = unsafe { f(7 as *mut c_void, 2.5, -3i32, 0.25f64, c"text".as_ptr()) };
        assert_eq!(status, 0);
        assert_eq!(
            SEEN.lock().unwrap().pop().as_deref(),
            Some("7 2.5 -3 0.25 text")
        );
    }
}
