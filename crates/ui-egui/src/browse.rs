//! The keys of the windows that list things (Explorer, Project Media and
//! the plug-in windows), while one has them (it was clicked last): a
//! letter or digit picks the next item whose name starts with it — letters
//! typed less than a second apart are the start of one name, the same one
//! again goes to the next with it —, and the arrows, Home, End, Page Up
//! and Page Down move the pick. Taken from the input before anything else
//! reads it, so the timeline's shortcuts on those keys wait until it has
//! the keys again.

use crate::dock::Tab;
use crate::egui::{self, Event, Key};

/// The windows that take them.
pub(crate) const TABS: [Tab; 6] = [
    Tab::Explorer,
    Tab::ProjectMedia,
    Tab::Generators,
    Tab::Transitions,
    Tab::VideoFx,
    Tab::Compositors,
];

/// How long between two letters for them to be one name's start, in
/// seconds.
const TOGETHER: f64 = 1.0;

/// A move of the pick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Move {
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
}

/// What a list window was given this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Keys {
    /// The start of a name to find (lowercase); empty when nothing was
    /// typed now.
    pub find: String,
    pub moves: Vec<Move>,
}

impl Keys {
    pub fn is_empty(&self) -> bool {
        self.find.is_empty() && self.moves.is_empty()
    }
}

/// The keys taken, for the window that has them.
#[derive(Debug, Default)]
pub(crate) struct Browse {
    tab: Option<Tab>,
    keys: Keys,
    /// What was typed lately, and when last.
    typed: String,
    last: f64,
}

impl Browse {
    /// Forgets this frame's keys.
    pub fn clear(&mut self) {
        self.tab = None;
        self.keys = Keys::default();
    }

    /// Takes the keys from `ctx`'s input when `tab` has them and lists
    /// things.
    pub fn take_input(&mut self, ctx: &egui::Context, tab: Option<Tab>) {
        self.clear();
        let Some(tab) = tab.filter(|t| TABS.contains(t)) else {
            return;
        };
        let (now, events) = ctx.input_mut(|i| {
            let mods = i.modifiers;
            let mut taken = Vec::new();
            // Plain keys (Shift for capitals) only: Ctrl's and Alt's are
            // shortcuts.
            let plain = !mods.ctrl && !mods.command && !mods.alt;
            if plain {
                i.events.retain(|e| match e {
                    Event::Text(t) if t.chars().all(|c| c.is_alphanumeric()) => {
                        taken.push(e.clone());
                        false
                    }
                    Event::Key {
                        key,
                        pressed,
                        modifiers,
                        ..
                    } if !modifiers.ctrl && !modifiers.command && !modifiers.alt => {
                        let moves = to_move(*key).is_some() && !modifiers.shift;
                        if moves || is_typed(*key) {
                            if *pressed {
                                taken.push(e.clone());
                            }
                            false
                        } else {
                            true
                        }
                    }
                    _ => true,
                });
            }
            (i.time, taken)
        });
        let mut typed = String::new();
        let mut moves = Vec::new();
        for e in events {
            match e {
                Event::Text(t) => typed.push_str(&t.to_lowercase()),
                Event::Key { key, .. } => moves.extend(to_move(key)),
                _ => {}
            }
        }
        if !typed.is_empty() {
            if now - self.last > TOGETHER {
                self.typed.clear();
            }
            self.typed.push_str(&typed);
            self.last = now;
        }
        self.tab = Some(tab);
        self.keys = Keys {
            find: if typed.is_empty() {
                String::new()
            } else {
                self.typed.clone()
            },
            moves,
        };
    }

    /// The keys `tab` was given this frame (once).
    pub fn take(&mut self, tab: Tab) -> Keys {
        if self.tab == Some(tab) {
            std::mem::take(&mut self.keys)
        } else {
            Keys::default()
        }
    }
}

fn to_move(key: Key) -> Option<Move> {
    Some(match key {
        Key::ArrowUp => Move::Up,
        Key::ArrowDown => Move::Down,
        Key::ArrowLeft => Move::Left,
        Key::ArrowRight => Move::Right,
        Key::Home => Move::Home,
        Key::End => Move::End,
        Key::PageUp => Move::PageUp,
        Key::PageDown => Move::PageDown,
        _ => return None,
    })
}

/// A letter's or a digit's key (what typing a name presses).
fn is_typed(key: Key) -> bool {
    let name = key.name();
    name.len() == 1 && name.chars().all(|c| c.is_ascii_alphanumeric())
}

/// The item `find` picks among `names` (in their order), the pick being
/// `current`: the next one whose name starts with it, after the current
/// when one letter was typed (or the same one again: the next with it),
/// from the current when more (it may still fit). When no name starts so,
/// the letters typed last start one anew ("bbc": "c"). `None` when none
/// fits.
pub(crate) fn find<'a>(
    names: impl IntoIterator<Item = &'a str>,
    current: Option<usize>,
    find: &str,
) -> Option<usize> {
    let names: Vec<String> = names.into_iter().map(str::to_lowercase).collect();
    let chars: Vec<char> = find.chars().collect();
    (0..chars.len()).find_map(|k| {
        let tail: String = chars[k..].iter().collect();
        starting(&names, current, &tail)
    })
}

/// [`find`] for the whole of `find`.
fn starting(names: &[String], current: Option<usize>, find: &str) -> Option<usize> {
    if find.is_empty() || names.is_empty() {
        return None;
    }
    let current = current.filter(|&c| c < names.len());
    let first = find.chars().next()?;
    let again = find.chars().all(|c| c == first);
    let (start, prefix) = if again {
        // "a", or "aaa": the next with an "a" (unless a name starts "aaa").
        let whole = current.is_some_and(|c| names[c].starts_with(find)) && find.len() > 1;
        if whole {
            (current.unwrap_or(0), find.to_string())
        } else {
            (current.map_or(0, |c| c + 1), first.to_string())
        }
    } else {
        (current.unwrap_or(0), find.to_string())
    };
    let n = names.len();
    (0..n)
        .map(|k| (start + k) % n)
        .find(|&i| names[i].starts_with(&prefix))
}

/// Where `moves` take the pick `current` in `len` items laid in rows of
/// `columns` (1: a list, where Left and Right do nothing), `page` rows to
/// a page. `None` when nothing is listed.
pub(crate) fn step(
    len: usize,
    current: Option<usize>,
    moves: &[Move],
    columns: usize,
    page: usize,
) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let columns = columns.max(1);
    let mut at = current.map(|c| c.min(len - 1));
    for m in moves {
        let here = at.unwrap_or(0) as isize;
        let (columns_i, page_i) = (columns as isize, (page.max(1) * columns) as isize);
        let to = match (at, m) {
            // Nothing picked yet: the first one, wherever it goes.
            (None, Move::End) => len as isize - 1,
            (None, _) => 0,
            (_, Move::Up) => here - columns_i,
            (_, Move::Down) => here + columns_i,
            (_, Move::Left) if columns > 1 => here - 1,
            (_, Move::Right) if columns > 1 => here + 1,
            (_, Move::Left | Move::Right) => here,
            (_, Move::Home) => 0,
            (_, Move::End) => len as isize - 1,
            (_, Move::PageUp) => here - page_i,
            (_, Move::PageDown) => here + page_i,
        };
        // Up from the first row and down from the last stay in place, as
        // a grid's; a page goes as far as there is.
        let to = match m {
            Move::Up | Move::Down if to < 0 || to >= len as isize => here,
            _ => to.clamp(0, len as isize - 1),
        };
        at = Some(to as usize);
    }
    at
}

/// The scroll offset at which row `i`, `row_h` high, of a list scrolled
/// to `offset` in a view `view_h` high is seen whole.
pub(crate) fn reveal(i: usize, row_h: f32, view_h: f32, offset: f32) -> f32 {
    let top = i as f32 * row_h;
    let bottom = top + row_h;
    if top < offset {
        top
    } else if bottom > offset + view_h {
        (bottom - view_h).max(0.0)
    } else {
        offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAMES: [&str; 6] = [
        "Add Noise",
        "Black and White",
        "Blur",
        "Border",
        "Brightness",
        "Glow",
    ];

    #[test]
    fn letters_find_names_and_the_same_one_goes_on() {
        let f = |current, typed: &str| find(NAMES, current, typed);
        assert_eq!(f(None, "b"), Some(1));
        assert_eq!(f(Some(1), "b"), Some(2), "the next with a b");
        assert_eq!(f(Some(4), "b"), Some(1), "round again");
        assert_eq!(f(Some(1), "bb"), Some(2), "b again");
        assert_eq!(f(Some(0), "bl"), Some(1));
        assert_eq!(f(Some(1), "blu"), Some(2));
        assert_eq!(f(Some(2), "bo"), Some(3));
        assert_eq!(f(Some(0), "g"), Some(5));
        assert_eq!(f(Some(0), "z"), None);
        assert_eq!(f(Some(2), "bbg"), Some(5), "no name: the last letter anew");
        assert_eq!(f(Some(2), "bbgl"), Some(5), "and those after it");
        assert_eq!(f(Some(0), ""), None);
    }

    #[test]
    fn arrows_move_in_lists_and_grids() {
        use Move::*;
        // A list.
        assert_eq!(step(5, None, &[Down], 1, 3), Some(0));
        assert_eq!(step(5, Some(0), &[Down, Down], 1, 3), Some(2));
        assert_eq!(step(5, Some(0), &[Up], 1, 3), Some(0));
        assert_eq!(step(5, Some(4), &[Down], 1, 3), Some(4));
        assert_eq!(step(5, Some(2), &[Left, Right], 1, 3), Some(2));
        assert_eq!(step(5, Some(2), &[End], 1, 3), Some(4));
        assert_eq!(step(5, Some(2), &[Home], 1, 3), Some(0));
        assert_eq!(step(10, Some(1), &[PageDown], 1, 3), Some(4));
        assert_eq!(step(10, Some(8), &[PageDown], 1, 3), Some(9));
        // A grid of three.
        assert_eq!(step(8, Some(1), &[Down], 3, 2), Some(4));
        assert_eq!(step(8, Some(6), &[Down], 3, 2), Some(6));
        assert_eq!(step(8, Some(4), &[Right, Right], 3, 2), Some(6));
        assert_eq!(step(8, Some(0), &[Left], 3, 2), Some(0));
        assert_eq!(step(0, None, &[Down], 3, 2), None);
    }

    #[test]
    fn rows_scroll_into_view() {
        assert_eq!(reveal(0, 20.0, 100.0, 40.0), 0.0);
        assert_eq!(reveal(3, 20.0, 100.0, 40.0), 40.0);
        assert_eq!(reveal(7, 20.0, 100.0, 40.0), 60.0);
    }
}
