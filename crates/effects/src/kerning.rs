//! The fonts' own kerning: how much nearer or farther apart two letters
//! sit than their advances say. It comes from the OpenType `GPOS` table's
//! `kern` feature — its pair adjustments, glyph by glyph or class by class,
//! for the Latin script (else the default one, else the first) — or, in
//! fonts without one, from the older `kern` table.

use skrifa::GlyphId;
use skrifa::raw::tables::gpos::{Gpos, PairPos, PositionSubtables};
use skrifa::raw::tables::kern::{Kern, SubtableKind};
use skrifa::raw::types::Tag;
use skrifa::raw::{FontRef, TableProvider};

/// A face's kerning pairs.
pub(crate) struct Kerning<'a> {
    gpos: Option<Gpos<'a>>,
    /// The lookups of the `GPOS` table's `kern` feature, in order.
    lookups: Vec<u16>,
    /// The `kern` table, read when there are no lookups.
    kern: Option<Kern<'a>>,
}

impl<'a> Kerning<'a> {
    /// `font`'s kerning.
    pub(crate) fn of(font: &FontRef<'a>) -> Kerning<'a> {
        Kerning::from_tables(font.gpos().ok(), font.kern().ok())
    }

    fn from_tables(gpos: Option<Gpos<'a>>, kern: Option<Kern<'a>>) -> Kerning<'a> {
        let lookups = gpos.as_ref().map(kern_lookups).unwrap_or_default();
        Kerning {
            gpos,
            lookups,
            kern,
        }
    }

    /// How much farther the pen moves after `left` when `right` follows,
    /// in the font's units (negative: nearer).
    pub(crate) fn pair(&self, left: GlyphId, right: GlyphId) -> i32 {
        if let Some(gpos) = self.gpos.as_ref().filter(|_| !self.lookups.is_empty()) {
            let Ok(list) = gpos.lookup_list() else {
                return 0;
            };
            // Each lookup adds its own; in a lookup the first subtable that
            // has the pair gives it.
            return self
                .lookups
                .iter()
                .filter_map(|&i| list.lookups().get(i as usize).ok())
                .filter_map(|lookup| match lookup.subtables() {
                    Ok(PositionSubtables::Pair(subtables)) => subtables
                        .iter()
                        .flatten()
                        .find_map(|pair| pair_value(&pair, left, right)),
                    _ => None,
                })
                .sum();
        }
        let Some(kern) = &self.kern else {
            return 0;
        };
        kern.subtables()
            .flatten()
            .filter(|t| t.is_horizontal() && !t.is_cross_stream() && !t.is_variable())
            .filter_map(|t| match t.kind().ok()? {
                SubtableKind::Format0(t) => t.kerning(left, right),
                SubtableKind::Format2(t) => t.kerning(left, right),
                SubtableKind::Format3(t) => t.kerning(left, right),
                SubtableKind::Format1(_) => None,
            })
            .sum()
    }
}

/// The lookups of `gpos`'s `kern` feature for the Latin script, else the
/// default one, else the first (its default language's, else its first
/// language's).
fn kern_lookups(gpos: &Gpos<'_>) -> Vec<u16> {
    let (Ok(scripts), Ok(features)) = (gpos.script_list(), gpos.feature_list()) else {
        return Vec::new();
    };
    let records = scripts.script_records();
    let record = [Tag::new(b"latn"), Tag::new(b"DFLT")]
        .iter()
        .find_map(|&tag| records.iter().find(|r| r.script_tag() == tag))
        .or(records.first());
    let Some(script) = record.and_then(|r| r.script(scripts.offset_data()).ok()) else {
        return Vec::new();
    };
    let lang = script.default_lang_sys().and_then(Result::ok).or_else(|| {
        let r = script.lang_sys_records().first()?;
        r.lang_sys(script.offset_data()).ok()
    });
    let Some(lang) = lang else {
        return Vec::new();
    };
    let mut lookups: Vec<u16> = Vec::new();
    for i in lang.feature_indices() {
        let Some(record) = features.feature_records().get(i.get() as usize) else {
            continue;
        };
        if record.feature_tag() != Tag::new(b"kern") {
            continue;
        }
        if let Ok(feature) = record.feature(features.offset_data()) {
            lookups.extend(feature.lookup_list_indices().iter().map(|l| l.get()));
        }
    }
    lookups.sort_unstable();
    lookups.dedup();
    lookups
}

/// `pair`'s adjustment of the pen after `left` before `right`: none when
/// the subtable does not have them (a class subtable has every glyph it
/// covers on the left).
fn pair_value(pair: &PairPos<'_>, left: GlyphId, right: GlyphId) -> Option<i32> {
    match pair {
        PairPos::Format1(t) => {
            let index = t.coverage().ok()?.get(left)?;
            let set = t.pair_sets().get(index as usize).ok()?;
            set.pair_value_records()
                .iter()
                .flatten()
                .find(|r| r.second_glyph().to_u32() == right.to_u32())
                .map(|r| r.value_record1().x_advance().unwrap_or(0).into())
        }
        PairPos::Format2(t) => {
            t.coverage().ok()?.get(left)?;
            let c1 = t.class_def1().ok()?.get(left);
            let c2 = t.class_def2().ok()?.get(right);
            if c1 >= t.class1_count() || c2 >= t.class2_count() {
                return None;
            }
            let record = t.class1_records().get(c1 as usize).ok()?;
            let value = record.class2_records().get(c2 as usize).ok()?;
            Some(value.value_record1().x_advance().unwrap_or(0).into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skrifa::raw::{FontData, FontRead};

    fn be(v: &mut Vec<u8>, words: &[i32]) {
        for &w in words {
            v.extend((w as u16).to_be_bytes());
        }
    }

    /// A `GPOS` table whose Latin `kern` feature has one pair lookup: A V
    /// (glyphs 1 and 2) −80 glyph by glyph, then T (3) before o or e (4,
    /// 5) −60 class by class.
    fn gpos() -> Vec<u8> {
        let mut t = Vec::new();
        // Header: version 1.0, then the script, feature and lookup lists.
        be(&mut t, &[1, 0, 10, 30, 44]);
        // Script list: latn, its default language with feature 0.
        be(&mut t, &[1]);
        t.extend(b"latn");
        be(&mut t, &[8, 4, 0, 0, 0xFFFF, 1, 0]);
        // Feature list: kern, with lookup 0.
        be(&mut t, &[1]);
        t.extend(b"kern");
        be(&mut t, &[8, 0, 1, 0]);
        // Lookup list: one pair lookup with two subtables.
        assert_eq!(t.len(), 44);
        be(&mut t, &[1, 4, 2, 0, 2, 10, 34]);
        // Pair pos 1: coverage A, its pair set A V −80.
        be(&mut t, &[1, 18, 4, 0, 1, 12]);
        be(&mut t, &[1, 2, -80]);
        be(&mut t, &[1, 1, 1]);
        // Pair pos 2: coverage T; T class 1; o and e class 1; T before
        // class 1, −60.
        be(&mut t, &[2, 24, 4, 0, 30, 38, 2, 2]);
        be(&mut t, &[0, 0, 0, -60]);
        be(&mut t, &[1, 1, 3]);
        be(&mut t, &[1, 3, 1, 1]);
        be(&mut t, &[1, 4, 2, 1, 1]);
        t
    }

    #[test]
    fn gpos_pairs_kern_glyph_by_glyph_and_class_by_class() {
        let bytes = gpos();
        let gpos = Gpos::read(FontData::new(&bytes)).expect("a GPOS table");
        let k = Kerning::from_tables(Some(gpos), None);
        assert_eq!(k.lookups, vec![0]);
        let g = GlyphId::new;
        assert_eq!(k.pair(g(1), g(2)), -80);
        assert_eq!(k.pair(g(2), g(1)), 0);
        assert_eq!(k.pair(g(3), g(4)), -60);
        assert_eq!(k.pair(g(3), g(5)), -60);
        assert_eq!(k.pair(g(3), g(1)), 0);
        assert_eq!(k.pair(g(1), g(4)), 0);
    }

    #[test]
    fn the_old_kern_table_kerns_fonts_without_gpos() {
        // Version 0, one horizontal format 0 subtable: A V −70, T o −50.
        let mut t = Vec::new();
        be(&mut t, &[0, 1]);
        be(&mut t, &[0, 14 + 12, 0x0001, 2, 12, 1, 0]);
        be(&mut t, &[1, 2, -70, 3, 4, -50]);
        let kern = Kern::read(FontData::new(&t)).expect("a kern table");
        let k = Kerning::from_tables(None, Some(kern));
        let g = GlyphId::new;
        assert_eq!(k.pair(g(1), g(2)), -70);
        assert_eq!(k.pair(g(3), g(4)), -50);
        assert_eq!(k.pair(g(2), g(1)), 0);
    }

    #[test]
    #[cfg(windows)]
    fn arial_kerns_a_v() {
        let Some((face, _, _)) = crate::fonts::find("Arial", false, false) else {
            return;
        };
        let data = crate::fonts::data(&face.path).expect("Arial's file");
        let font = FontRef::from_index(&data, face.index).expect("Arial");
        let map = skrifa::MetadataProvider::charmap(&font);
        let [a, v] = ['A', 'V'].map(|c| map.map(c).expect("a glyph"));
        assert!(Kerning::of(&font).pair(a, v) < 0);
    }
}
