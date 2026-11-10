//! The interface's fonts: egui's own (Latin, Greek, Cyrillic) and, behind
//! them, the computer's Chinese, Japanese and Korean faces (Traditional
//! Chinese's for its interface), one with many other scripts and one with
//! Thai's — so what is typed in any language (a caption, a marker's name,
//! a file's) and the Asian languages' interface show their letters instead
//! of boxes. The faces' files are big (tens of megabytes
//! together): they are read on a thread of their own and added when they
//! are in, except for an Asian interface language, whose first frame needs
//! them.

use crate::egui;
use crate::i18n::Language;

/// Which language's way Han characters are drawn in first.
fn first_for(lang: Language) -> bv_engine::plugins::Cjk {
    use bv_engine::plugins::Cjk;
    match lang {
        Language::Japanese => Cjk::Japanese,
        Language::Korean => Cjk::Korean,
        Language::ChineseTraditional => Cjk::ChineseTraditional,
        _ => Cjk::Chinese,
    }
}

/// Whether `lang`'s own texts need the computer's faces.
fn needs_them(lang: Language) -> bool {
    matches!(
        lang,
        Language::ChineseSimplified
            | Language::ChineseTraditional
            | Language::Japanese
            | Language::Korean
            | Language::Thai
    )
}

/// The faces of `first`'s order read from their files.
fn read(first: bv_engine::plugins::Cjk) -> Vec<(String, egui::FontData)> {
    bv_engine::plugins::interface_fallbacks(first)
        .into_iter()
        .filter_map(|(path, index)| {
            let bytes = std::fs::read(&path).ok()?;
            let mut data = egui::FontData::from_owned(bytes);
            data.index = index;
            let name = format!("bv-system:{}:{index}", path.display());
            Some((name, data))
        })
        .collect()
}

fn add(ctx: &egui::Context, faces: Vec<(String, egui::FontData)>) {
    use egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
    for (name, data) in faces {
        ctx.add_font(FontInsert::new(
            &name,
            data,
            vec![
                InsertFontFamily {
                    family: egui::FontFamily::Proportional,
                    priority: FontPriority::Lowest,
                },
                InsertFontFamily {
                    family: egui::FontFamily::Monospace,
                    priority: FontPriority::Lowest,
                },
            ],
        ));
    }
    ctx.request_repaint();
}

/// [`install`] at once, whatever the language.
pub(crate) fn install_now(ctx: &egui::Context, lang: Language) {
    add(ctx, read(first_for(lang)));
}

/// Adds the computer's faces behind egui's, in the order `lang` wants:
/// at once when its texts need them, else from a thread of their own.
pub(crate) fn install(ctx: &egui::Context, lang: Language) {
    let first = first_for(lang);
    if needs_them(lang) {
        add(ctx, read(first));
        return;
    }
    let ctx = ctx.clone();
    let _ = std::thread::Builder::new()
        .name("bv-fonts".into())
        .spawn(move || add(&ctx, read(first)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asian_languages_draw_han_their_own_way_first() {
        use bv_engine::plugins::Cjk;
        assert_eq!(first_for(Language::Japanese), Cjk::Japanese);
        assert_eq!(first_for(Language::Korean), Cjk::Korean);
        assert_eq!(first_for(Language::ChineseSimplified), Cjk::Chinese);
        assert_eq!(
            first_for(Language::ChineseTraditional),
            Cjk::ChineseTraditional
        );
        assert!(needs_them(Language::Thai));
        assert_eq!(first_for(Language::English), Cjk::Chinese);
        assert!(needs_them(Language::Japanese));
        assert!(!needs_them(Language::German));
    }

    /// egui's own fonts draw every European language's letters: the Asian
    /// ones need the computer's (as do a few symbols).
    #[test]
    fn european_languages_have_their_letters() {
        let ctx = egui::Context::default();
        ctx.begin_pass(egui::RawInput::default());
        for &lang in Language::ALL {
            if needs_them(lang) {
                continue;
            }
            let l = lang.locale();
            let sample: String = std::iter::once(l.native_name)
                .chain(l.texts.iter().map(|(_, t)| *t))
                .collect();
            ctx.fonts_mut(|f| {
                let font = egui::FontId::proportional(13.0);
                for c in sample.chars().filter(|c| c.is_alphabetic()) {
                    assert!(
                        f.has_glyph(&font, c),
                        "{}: no letter for {c:?} ({:#06x})",
                        l.code,
                        c as u32
                    );
                }
            });
        }
        ctx.end_pass().textures_delta.clear();
    }

    /// The computer's faces cover the Asian languages' texts (where it has
    /// them: Windows always does).
    #[test]
    #[cfg(windows)]
    fn windows_faces_cover_the_asian_languages() {
        for lang in [
            Language::ChineseSimplified,
            Language::Japanese,
            Language::Korean,
        ] {
            let ctx = egui::Context::default();
            add(&ctx, read(first_for(lang)));
            ctx.begin_pass(egui::RawInput::default());
            let l = lang.locale();
            let sample: String = std::iter::once(l.native_name)
                .chain(l.texts.iter().map(|(_, t)| *t))
                .collect();
            ctx.fonts_mut(|f| {
                let font = egui::FontId::proportional(13.0);
                for c in sample.chars().filter(|c| !c.is_whitespace()) {
                    assert!(
                        f.has_glyph(&font, c),
                        "{}: no letter for {c:?} ({:#06x})",
                        l.code,
                        c as u32
                    );
                }
            });
            ctx.end_pass().textures_delta.clear();
        }
    }
}
