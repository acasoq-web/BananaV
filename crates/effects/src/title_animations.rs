//! Titles & Text's animations: the Animation list's moves of the text, its
//! lines, words or letters over the media's length, as VEGAS's titles move
//! them.
//!
//! Each animation is a template ([`Animation`]): the text block's pivot,
//! its fixed tilt and effects, attributes given to some lines, words and
//! letters by their place in the text (their character index, as the
//! template's own text had them), staggers that copy one element's
//! attributes to the elements after it and delay their rows, and rows of
//! keys that set a property of an element at each moment. Time is the part
//! of the generated media played (`u`, 0 at its start, 1 at its end).
//!
//! - [`motion`]: a row's value at a moment — its curve through the keys,
//!   the waves added to it (a damped spring, shrinking hops, a jitter) —
//!   and how big a blur a row's number asks for.
//! - [`scene`]: the user's text as lines, words and letters; the template
//!   applied to it; the rows played at a moment: each letter's place, turn,
//!   scale, slant and opacity, and the elements whose effects need a
//!   picture of their own.
//! - [`draw`]: that moment drawn: the letters and their outlines, each
//!   element's blur and glow on its own picture, the block's shadow.
//!
//! "None" draws as Titles & Text always has ([`crate::titles::picture`]).

use std::ffi::CStr;

mod data;
pub(crate) mod draw;
pub(crate) mod motion;
pub(crate) mod scene;

/// The Animation list: each animation's name as a `.veg` keeps it, and
/// as the list shows it. "None" first: the text stays still.
pub(crate) const ANIMATIONS: [(&CStr, &CStr); 51] = [
    (c"_None", c"None"),
    (c"_Title01", c"Title01"),
    (c"_Title02", c"Title02"),
    (c"_Title03", c"Title03"),
    (c"_Title04", c"Title04"),
    (c"_Title05", c"Title05"),
    (c"_Title06", c"Title06"),
    (c"_Title07", c"Title07"),
    (c"_Title08", c"Title08"),
    (c"_Title09", c"Title09"),
    (c"_Title10", c"Title10"),
    (c"_Title11", c"Title11"),
    (c"_Title12", c"Title12"),
    (c"_Title13", c"Title13"),
    (c"_Title14", c"Title14"),
    (c"_Title15", c"Title15"),
    (c"_Title16", c"Title16"),
    (c"_Title17", c"Title17"),
    (c"_Title18", c"Title18"),
    (c"_Title19", c"Title19"),
    (c"_Title20", c"Title20"),
    (c"_Title21", c"Title21"),
    (c"_Title22", c"Title22"),
    (c"_Title23", c"Title23"),
    (c"_Title24", c"Title24"),
    (c"_Title25", c"Title25"),
    (c"_Action_Flip", c"Action Flip"),
    (c"_Bounce", c"Bounce"),
    (c"_Coming_at_You", c"Coming at You"),
    (c"_Double_Flash_Glow", c"Double Flash Glow"),
    (c"_Drop_split", c"Drop Split"),
    (c"_Dropping_words", c"Dropping Words"),
    (c"_Earthquake", c"Earthquake"),
    (c"_Fall_Down", c"Fall Down"),
    (c"_Float_and_pop", c"Float and Pop"),
    (c"_Fly_in_from_Right", c"Fly in from Right"),
    (c"_Fly_in", c"Fly In"),
    (c"_Jump", c"Jump"),
    (c"_Menace", c"Menace"),
    (c"_Popup", c"Popup"),
    (c"_Rolling_Glow_and_Enlarge", c"Rolling Glow and Enlarge"),
    (c"_Rough_Day", c"Rough Day"),
    (c"_Scroll", c"Scroll"),
    (c"_Scroll_Left", c"Scroll Left"),
    (c"_Slide_Down", c"Slide Down"),
    (c"_Slide_Left", c"Slide Left"),
    (c"_Slide_Right", c"Slide Right"),
    (c"_Slide_Up", c"Slide Up"),
    (c"_Slide", c"Slide"),
    (c"_Speedy", c"Speedy"),
    (c"_Twist_In", c"Twist In"),
];

/// The name of no animation.
pub(crate) const NONE: &CStr = c"_None";

/// The animation of that name (none for "None" or a name it does not
/// know: the text stays still).
pub(crate) fn find(name: &str) -> Option<&'static Animation> {
    data::ALL.iter().copied().find(|a| a.name == name)
}

/// An animation: the template the text takes and its rows.
#[derive(Debug)]
pub(crate) struct Animation {
    /// Its name as the list keeps it.
    pub name: &'static str,
    pub block: Template,
    /// Attributes for the lines, words and letters at these places of the
    /// text (a later one over an earlier one on the same element).
    pub nodes: &'static [Node],
    pub staggers: &'static [Stagger],
    pub rows: &'static [Row],
}

/// What an animation keeps of its own text block: where it turns about,
/// its fixed tilt and slant, its effects, and how much of it shows. (Its
/// place, scale and tracking are the settings'.)
#[derive(Debug)]
pub(crate) struct Template {
    pub pivot: Pivot,
    /// Radians, clockwise.
    pub rotation: f64,
    /// Across per down, and down per across.
    pub shear: [f64; 2],
    pub opacity: f64,
    pub effects: Effects,
    /// Lines never closer than the font's own distance (Line spacing under
    /// 1 counts as 1).
    pub spacing_floor: bool,
    /// The letter at the edge of the part shown fades in by how much of it
    /// is in (else it shows whole once more than half of it is).
    pub fade_selection: bool,
    /// The part of the letters shown: from, to (0–1 of them all).
    pub selection: [f64; 2],
    /// A curve the letters run along instead of their lines.
    pub path: Option<&'static Path>,
}

/// An element's effects: a blur (across, down) and a yellow glow, each on
/// or off, their sizes parts of the frame's width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Effects {
    pub blur_on: bool,
    pub blur: [f64; 2],
    pub glow_on: bool,
    pub glow: f64,
}

/// A line's, word's or letter's attributes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Attrs {
    /// Units of a twentieth of the frame's height, right and up.
    pub offset: [f64; 2],
    pub scale: [f64; 2],
    /// Across per down (tops lean left for more).
    pub shear: f64,
    /// Radians, clockwise.
    pub rotation: f64,
    pub pivot: Pivot,
    pub opacity: f64,
    /// None: no effects at all (rows on them do nothing).
    pub effects: Option<Effects>,
    /// A line's: added after each of its letters, in units.
    pub tracking: f64,
    /// A letter's: it and the rest of its line moved across, in ems.
    pub kerning: f64,
}

impl Attrs {
    /// An element as it is without an animation.
    pub const PLAIN: Attrs = Attrs {
        offset: [0.0, 0.0],
        scale: [1.0, 1.0],
        shear: 0.0,
        rotation: 0.0,
        pivot: Pivot::At(Across::Middle, Down::Middle),
        opacity: 1.0,
        effects: None,
        tracking: 0.0,
        kerning: 0.0,
    };
}

/// Attributes for the element of the text at a character's place.
#[derive(Debug)]
pub(crate) struct Node {
    pub level: Level,
    /// The character's place in the text, each line's end counting one.
    pub index: usize,
    pub attrs: Attrs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Level {
    Line,
    Word,
    Char,
}

/// The point of its box an element turns and scales about.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Pivot {
    At(Across, Down),
    /// Parts of its width (from its left) and of its height (from its top).
    Part(f64, f64),
}

/// Across an element's box: its middle or its right edge (the
/// animations' pivots are no others).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Across {
    Middle,
    Far,
}

/// Down an element's box: its middle or its baseline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Down {
    Middle,
    Baseline,
}

/// An element of the text: the block, a line, a line's word, a word's
/// letter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Element {
    Block,
    Line(usize),
    Word(usize, usize),
    Char(usize, usize, usize),
}

/// The elements a stagger runs over, from its own element on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum List {
    /// The block's lines.
    Lines,
    /// Its line's words.
    LineWords,
    /// The block's words.
    Words,
    /// The block's letters.
    Chars,
}

/// One element's attributes copied to the elements after it in a list,
/// and the rows under it played for each of them, each a little later:
/// `duration` (a part of the media) times `shape` at its rank (0 for the
/// first, 1 for the last; the rank itself without a shape).
#[derive(Debug)]
pub(crate) struct Stagger {
    pub at: Element,
    pub list: List,
    pub duration: f64,
    pub shape: Option<&'static [Key]>,
}

/// What a row sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Property {
    OffsetX,
    OffsetY,
    ScaleX,
    ScaleY,
    Rotation,
    ShearX,
    Opacity,
    BlurAcross,
    BlurDown,
    Glow,
    /// The block's tracking, in units.
    Tracking,
    /// Along the path, in its lengths.
    PathPosition,
    SelectionLeft,
    SelectionRight,
}

/// A property of an element over time: the curve through its keys, and
/// waves added to it; under a stagger (an index into the animation's
/// staggers), played for each element of the stagger's list.
#[derive(Debug)]
pub(crate) struct Row {
    pub at: Element,
    pub property: Property,
    pub stagger: Option<usize>,
    pub keys: &'static [Key],
    pub waves: &'static [Wave],
}

/// A key: a moment (a part of the media), its value, and how the curve
/// goes on from it to the next key.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Key {
    pub at: f64,
    pub value: f64,
    pub ease: Ease,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Ease {
    Linear,
    /// Slow at first (a square).
    Slow,
    /// Fast at first.
    Fast,
    /// Slow at both ends.
    Smooth,
    /// A smooth curve through the neighbouring keys.
    Auto,
    /// Its own slopes, arriving and leaving (values per segment).
    Tangents(f64, f64),
}

/// A move added to a row from `start` for `length` (parts of the media),
/// `scale` times its shape's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Wave {
    pub start: f64,
    pub length: f64,
    pub scale: f64,
    pub shape: Shape,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Shape {
    /// A damped spring's wobble about 0, dipping to −2 × `amplitude`.
    Spring {
        amplitude: f64,
        frequency: f64,
        damping: f64,
    },
    /// `loops` hops each `decay` as high and as long as the one before,
    /// begun `inner` of a hop in; `settle`: 0 where it begins.
    Hops {
        loops: f64,
        decay: f64,
        inner: f64,
        settle: bool,
    },
    /// A smooth jitter: `octaves` layers, each twice as fast and half as
    /// big as the one before.
    Jitter {
        frequency: f64,
        octaves: u32,
        gradients: &'static [f64; 128],
    },
}

/// A curve of cubic pieces through its points, in units: across from the
/// block's left, down from each line's baseline.
#[derive(Debug)]
pub(crate) struct Path {
    pub points: &'static [PathPoint],
    /// Past its end the letters come back at its start.
    pub wrap: bool,
    pub position: f64,
}

/// A point of a path and its handles: each `[in, at, out]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PathPoint {
    pub x: [f64; 3],
    pub y: [f64; 3],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_animation_of_the_list_has_its_moves() {
        for (name, _) in &ANIMATIONS[1..] {
            let name = name.to_str().unwrap();
            let a = find(name).unwrap_or_else(|| panic!("{name}"));
            assert!(!a.rows.is_empty(), "{name}");
            for r in a.rows {
                assert!(!r.keys.is_empty(), "{name}");
                assert!(r.stagger.is_none_or(|s| s < a.staggers.len()), "{name}");
            }
        }
        assert!(find("_None").is_none());
        assert!(find("_Nothing").is_none());
        assert_eq!(data::ALL.len(), ANIMATIONS.len() - 1);
    }
}
