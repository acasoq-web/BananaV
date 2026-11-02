//! The decimal separator of the interface's language, for numbers BananaV
//! writes into pictures (a timecode's seconds): VEGAS's follow the
//! computer's language the same way. The interface sets it when its
//! language changes.

use std::sync::atomic::{AtomicBool, Ordering};

static COMMA: AtomicBool = AtomicBool::new(false);

/// Numbers are written with `separator` (`,` or `.`) from now on.
pub fn set_decimal_separator(separator: char) {
    COMMA.store(separator == ',', Ordering::Relaxed);
}

/// The separator numbers are written with: `.` unless the interface's
/// language asks for `,`.
pub fn decimal_separator() -> char {
    if COMMA.load(Ordering::Relaxed) {
        ','
    } else {
        '.'
    }
}
