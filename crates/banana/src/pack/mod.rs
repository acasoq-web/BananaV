//! The pack's effects, by group.

mod blur;
mod color;
mod dissolves;
mod distort;
mod generate;
mod light;
mod motion;
mod stylize;
mod time;
mod wipes;

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::fx::Fx;

/// Every effect, always in the same order.
pub fn all() -> &'static [Fx] {
    static ALL: OnceLock<Vec<Fx>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut v = color::list();
        v.extend(blur::list());
        v.extend(distort::list());
        v.extend(light::list());
        v.extend(generate::list());
        v.extend(stylize::list());
        v.extend(time::list());
        v.extend(dissolves::list());
        v.extend(wipes::list());
        v.extend(motion::list());
        v
    })
}

/// The effect whose identifier ends with `short`.
pub fn find(short: &str) -> Option<(usize, &'static Fx)> {
    static INDEX: OnceLock<HashMap<&'static str, usize>> = OnceLock::new();
    let index = INDEX.get_or_init(|| all().iter().enumerate().map(|(i, f)| (f.id, i)).collect());
    let i = *index.get(short)?;
    Some((i, &all()[i]))
}
