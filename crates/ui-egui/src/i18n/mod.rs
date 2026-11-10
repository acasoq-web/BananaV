//! Interface languages.
//!
//! English is BananaV's main language: every text in the source code is
//! written in English, and that is what the interface shows by default.
//! Other languages are translation catalogs keyed by the English text
//! (see [`pt_br`]); a text missing from a catalog shows in English.
//!
//! - [`tr`] translates a text: `ui.button(tr("Save"))`.
//! - [`trf!`] translates a template and fills its `{}` placeholders:
//!   `trf!("Saved to {}", path.display())`. A translation may reorder the
//!   values with `{0}`, `{1}`…; `{:.1}` keeps its precision.
//! - [`trn!`] picks the form of a text by a count:
//!   `trn!(n, "{} file imported.", "{} files imported.", n)`. Each language
//!   has as many forms as its grammar needs (the CLDR plural rules).
//! - [`trc`] is for the few words whose translation depends on where they
//!   appear (the `End` key is "Terminar", the `End` column is "Fim").
//!
//! # Adding a language
//!
//! 1. Copy `pt_br.rs` to a module named after the language's tag (`es.rs`
//!    for Spanish), translate its texts and fill in its [`Locale`]: tag, own
//!    name, number separators, plural rule and forms.
//! 2. Add its line to the `languages!` list below.
//! 3. `cargo xtask i18n` lists, for every language, the texts it still
//!    lacks and the entries no text uses any more. Once a language is
//!    complete, mark it `complete: true`: the tests then fail when a text
//!    is added without its translation.
//!
//! Brazilian Portuguese is complete. Simplified Chinese, Russian, Spanish
//! (Spain), German, Japanese, French, Polish and Korean are partial, and
//! since 2026-11-10 Turkish, Traditional Chinese, Spanish (Latin America),
//! Thai, Ukrainian, Italian, Czech, Hungarian, Portuguese (Portugal) and
//! Swedish too: the texts people need most to find their way (menus, the
//! main windows, the timeline, the render dialog); Preferences marks them
//! so, and they are not kept up with every new text. Their Asian and Thai
//! letters come from the computer's fonts (`interface_fonts`).

use std::collections::HashMap;
use std::fmt::{Display, Write as _};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

mod cs;
mod de;
mod es_419;
mod es_es;
mod fr;
mod hu;
mod it;
mod ja;
mod ko;
mod pl;
mod pt_br;
mod pt_pt;
mod ru;
mod sv;
mod th;
mod tr_tr;
mod uk;
mod zh_cn;
mod zh_tw;

#[cfg(test)]
mod coverage;

/// The plural categories of the Unicode CLDR: the forms a word takes by
/// the number it counts. English has `One` ("1 file") and `Other` ("2
/// files"); other languages use more of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plural {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

/// What the interface needs to know of a language: its texts, how it
/// writes numbers and how it counts.
#[derive(Debug)]
pub struct Locale {
    /// BCP 47 tag, as saved in the settings ("pt-BR").
    pub code: &'static str,
    /// The language's own name, as Preferences lists it.
    pub native_name: &'static str,
    pub decimal_separator: char,
    pub thousands_separator: char,
    /// The plural category of a whole number.
    pub plural: fn(u64) -> Plural,
    /// The categories the language uses, in the order of the forms of
    /// [`Locale::plurals`].
    pub plural_forms: &'static [Plural],
    /// Translations keyed by the English text, sorted by lowercase key.
    pub texts: &'static [(&'static str, &'static str)],
    /// Words translated by where they appear: (context, English, text).
    pub context: &'static [(&'static str, &'static str, &'static str)],
    /// Texts that change with a count: (English one, English other, the
    /// forms of `plural_forms`).
    pub plurals: &'static [(&'static str, &'static str, &'static [&'static str])],
    /// Every text is translated: the tests fail when one is missing.
    pub complete: bool,
}

/// English (CLDR "en"): one for 1, other for everything else.
fn english_plural(n: u64) -> Plural {
    if n == 1 { Plural::One } else { Plural::Other }
}

/// English is the source text: it has no catalog.
const ENGLISH: Locale = Locale {
    code: "en",
    native_name: "English",
    decimal_separator: '.',
    thousands_separator: ',',
    plural: english_plural,
    plural_forms: &[Plural::One, Plural::Other],
    texts: &[],
    context: &[],
    plurals: &[],
    complete: true,
};

/// Declares the interface languages: each line is a [`Language`] variant
/// and its [`Locale`].
macro_rules! languages {
    ($($(#[$attr:meta])* $variant:ident => $locale:expr,)+) => {
        /// A language of the interface. Saved in the settings by its tag.
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
        pub enum Language {
            $($(#[$attr])* $variant,)+
        }

        impl Language {
            /// Every language, in the order Preferences lists them.
            pub const ALL: &'static [Language] = &[$(Language::$variant),+];

            /// Its texts, number separators and plural rule.
            pub fn locale(self) -> &'static Locale {
                match self {
                    $(Language::$variant => &$locale,)+
                }
            }
        }
    };
}

languages! {
    /// The main language: the texts of the source code.
    #[default]
    English => ENGLISH,
    PortugueseBrazil => pt_br::LOCALE,
    // Partial: the essentials (menus, the main windows, the timeline, the
    // render dialog); the rest shows in English.
    ChineseSimplified => zh_cn::LOCALE,
    Russian => ru::LOCALE,
    SpanishSpain => es_es::LOCALE,
    German => de::LOCALE,
    Japanese => ja::LOCALE,
    French => fr::LOCALE,
    Polish => pl::LOCALE,
    Korean => ko::LOCALE,
    Turkish => tr_tr::LOCALE,
    ChineseTraditional => zh_tw::LOCALE,
    SpanishLatinAmerica => es_419::LOCALE,
    Thai => th::LOCALE,
    Ukrainian => uk::LOCALE,
    Italian => it::LOCALE,
    Czech => cs::LOCALE,
    Hungarian => hu::LOCALE,
    PortuguesePortugal => pt_pt::LOCALE,
    Swedish => sv::LOCALE,
}

impl Language {
    /// The language's own name, as listed in Preferences.
    pub fn native_name(self) -> &'static str {
        self.locale().native_name
    }

    /// BCP 47 tag.
    pub fn code(self) -> &'static str {
        self.locale().code
    }

    /// The language whose tag is `code`.
    pub fn from_code(code: &str) -> Option<Language> {
        Language::ALL
            .iter()
            .copied()
            .find(|l| l.code().eq_ignore_ascii_case(code))
    }

    /// Decimal separator of numbers shown to the user.
    pub fn decimal_separator(self) -> char {
        self.locale().decimal_separator
    }

    /// Separator of thousands in whole numbers ("48,000" / "48.000").
    pub fn thousands_separator(self) -> char {
        self.locale().thousands_separator
    }

    /// Whether only part of the interface is translated (the rest shows in
    /// English).
    pub fn partial(self) -> bool {
        !self.locale().complete
    }

    fn index(self) -> usize {
        self as usize
    }
}

impl serde::Serialize for Language {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.code())
    }
}

impl<'de> serde::Deserialize<'de> for Language {
    /// A language this version does not have (settings written by a newer
    /// one) reads as English instead of failing the whole file.
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Language, D::Error> {
        let code = <String as serde::Deserialize>::deserialize(d)?;
        Ok(Language::from_code(&code).unwrap_or_default())
    }
}

/// A language's texts, indexed.
struct Catalog {
    texts: HashMap<&'static str, &'static str>,
    plurals: HashMap<(&'static str, &'static str), &'static [&'static str]>,
}

fn catalog(lang: Language) -> &'static Catalog {
    static CATALOGS: OnceLock<Vec<Catalog>> = OnceLock::new();
    let all = CATALOGS.get_or_init(|| {
        Language::ALL
            .iter()
            .map(|l| {
                let locale = l.locale();
                Catalog {
                    texts: locale.texts.iter().copied().collect(),
                    plurals: locale
                        .plurals
                        .iter()
                        .map(|(one, other, forms)| ((*one, *other), *forms))
                        .collect(),
                }
            })
            .collect()
    });
    &all[lang.index()]
}

static LANGUAGE: AtomicU8 = AtomicU8::new(0);

/// The language the interface is showing.
pub fn language() -> Language {
    Language::ALL
        .get(LANGUAGE.load(Ordering::Relaxed) as usize)
        .copied()
        .unwrap_or_default()
}

/// Switches the interface language; the next frame is drawn in it.
pub fn set_language(lang: Language) {
    LANGUAGE.store(lang.index() as u8, Ordering::Relaxed);
    // Numbers written into pictures (a timecode's) follow it too.
    bv_core::locale::set_decimal_separator(lang.decimal_separator());
}

/// `text` (English) in the interface language.
pub fn tr(text: &str) -> &str {
    system_keys(translate(language(), text))
}

/// `text` with the keys named as this system names them: on macOS Cmd for
/// Ctrl (BananaV's Ctrl is the Command key there) and Option for Alt
/// ("Ctrl+drag: fine" reads "Cmd+drag: fine"); elsewhere as it is.
pub fn system_keys(text: &str) -> &str {
    if !cfg!(target_os = "macos") || !(text.contains("Ctrl") || text.contains("Alt")) {
        return text;
    }
    static MAC: std::sync::Mutex<Option<HashMap<String, &'static str>>> =
        std::sync::Mutex::new(None);
    let Ok(mut known) = MAC.lock() else {
        return text;
    };
    let known = known.get_or_insert_with(HashMap::new);
    if let Some(t) = known.get(text) {
        return t;
    }
    // The catalog's texts are few: each one changed is kept for good.
    let changed: &'static str = Box::leak(mac_key_names(text).into_boxed_str());
    known.insert(text.to_string(), changed);
    changed
}

/// The Mac's names of the keys in `text`: the words "Ctrl" and "Alt" (not
/// within other words) become "Cmd" and "Option".
pub fn mac_key_names(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    let mut rest = text;
    let mut before: Option<char> = None;
    while !rest.is_empty() {
        let word = [("Ctrl", "Cmd"), ("Alt", "Option")]
            .into_iter()
            .find(|(w, _)| {
                rest.starts_with(w)
                    && before.is_none_or(|c| !c.is_alphanumeric())
                    && rest[w.len()..]
                        .chars()
                        .next()
                        .is_none_or(|c| !c.is_alphanumeric())
            });
        if let Some((w, mac)) = word {
            out.push_str(mac);
            rest = &rest[w.len()..];
            before = w.chars().last();
            continue;
        }
        let c = rest.chars().next().unwrap_or_default();
        out.push(c);
        before = Some(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// `text` in `lang`; English when the catalog does not have it.
pub fn translate(lang: Language, text: &str) -> &str {
    catalog(lang).texts.get(text).copied().unwrap_or(text)
}

/// Like [`tr`], for a word whose translation depends on `context`.
pub fn trc<'a>(context: &str, text: &'a str) -> &'a str {
    let lang = language();
    lang.locale()
        .context
        .iter()
        .find(|(ctx, en, _)| *ctx == context && *en == text)
        .map_or_else(|| translate(lang, text), |(_, _, t)| *t)
}

/// The form of a text that counts `n`, in `lang`: `one` and `other` are the
/// English forms (used where the catalog lacks the text).
pub fn plural_in<'a>(lang: Language, n: u64, one: &'a str, other: &'a str) -> &'a str {
    let locale = lang.locale();
    match catalog(lang).plurals.get(&(one, other)) {
        Some(forms) => {
            let category = (locale.plural)(n);
            let i = locale
                .plural_forms
                .iter()
                .position(|c| *c == category)
                .unwrap_or(locale.plural_forms.len().saturating_sub(1));
            forms.get(i).or(forms.last()).copied().unwrap_or(other)
        }
        None if english_plural(n) == Plural::One => one,
        None => other,
    }
}

/// [`plural_in`] the interface language.
pub fn plural<'a>(n: u64, one: &'a str, other: &'a str) -> &'a str {
    plural_in(language(), n, one, other)
}

/// Fills a translated template: `{}` takes the next value, `{N}` the value
/// `N`, `{:.P}` / `{N:.P}` apply a precision, `{{` and `}}` are braces.
pub fn fill(template: &str, args: &[&dyn Display]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut next = 0;
    let mut rest = template;
    while let Some(i) = rest.find(['{', '}']) {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        if tail.starts_with("{{") || tail.starts_with("}}") {
            out.push_str(&tail[..1]);
            rest = &tail[2..];
            continue;
        }
        let Some(end) = tail.find('}').filter(|_| tail.starts_with('{')) else {
            out.push_str(&tail[..1]);
            rest = &tail[1..];
            continue;
        };
        let inner = &tail[1..end];
        let (index, spec) = inner.split_once(':').unwrap_or((inner, ""));
        let index = if index.is_empty() {
            next += 1;
            next - 1
        } else {
            index.parse().unwrap_or(usize::MAX)
        };
        match args.get(index) {
            Some(arg) => match spec.strip_prefix('.').and_then(|p| p.parse::<usize>().ok()) {
                Some(prec) => {
                    let _ = write!(out, "{arg:.prec$}");
                }
                None => {
                    let _ = write!(out, "{arg}");
                }
            },
            None => out.push_str(&tail[..=end]),
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Translates a template with [`tr`] and fills it with [`fill`].
macro_rules! trf {
    ($template:literal $(, $arg:expr)* $(,)?) => {
        $crate::i18n::fill(
            $crate::i18n::tr($template),
            &[$(&$arg as &dyn ::std::fmt::Display),*],
        )
    };
}

/// Picks the form of a text by a count and fills it like [`trf!`]:
/// `trn!(n, "{} file imported.", "{} files imported.", n)`.
macro_rules! trn {
    ($n:expr, $one:literal, $other:literal $(, $arg:expr)* $(,)?) => {
        $crate::i18n::fill(
            $crate::i18n::plural(($n) as u64, $one, $other),
            &[$(&$arg as &dyn ::std::fmt::Display),*],
        )
    };
}

/// Name of a generated media in the interface language ("Tone 440 Hz").
pub fn generator_name(g: &bv_timeline::Generator) -> String {
    match g {
        bv_timeline::Generator::TestTone { frequency_hz, .. } => {
            trf!("Tone {} Hz", crate::fmt::num(*frequency_hz as f64, 0))
        }
        // A plug-in's own name.
        bv_timeline::Generator::Plugin(_) => g.display_name(),
        other => tr(&other.display_name()).to_string(),
    }
}

/// Display name of a media. Generated media keep their default name in
/// English in the project file; that name is shown translated.
pub fn media_name(m: &bv_timeline::MediaInfo) -> String {
    match &m.source {
        bv_timeline::MediaSource::Generated(g) if m.name == g.display_name() => generator_name(g),
        _ => m.name.clone(),
    }
}

/// Display name of a track: the default names ("Video 1", "Audio 2") are
/// stored in English and shown in the interface language.
/// An audio bus's name: the user's, else "Bus A"… by its place.
pub fn bus_name(p: &bv_timeline::Project, id: bv_timeline::BusId) -> String {
    match p.bus(id) {
        Some(b) if !b.channel.name.is_empty() => b.channel.name.clone(),
        _ => bus_default_name(p, id),
    }
}

/// Where a track or a strip plays, named: the Master, a bus, or a pair
/// of the output device's channels ("Hardware Out 3/4").
pub fn output_name(p: &bv_timeline::Project, o: bv_timeline::Output) -> String {
    match o {
        bv_timeline::Output::Master => tr("Master").to_string(),
        bv_timeline::Output::Bus(b) => bus_name(p, b),
        bv_timeline::Output::Hardware(k) => {
            let first = u32::from(k) * 2 + 1;
            trf!("Hardware Out {}/{}", first, first + 1)
        }
    }
}

/// An input bus's name: the user's, else "Input Bus A"… by its place.
pub fn input_bus_name(p: &bv_timeline::Project, id: bv_timeline::InputBusId) -> String {
    match p.input_bus(id) {
        Some(b) if !b.name.is_empty() => b.name.clone(),
        _ => p.input_bus_index(id).map_or_else(String::new, |i| {
            trf!("Input Bus {}", bv_timeline::busses::bus_letter(i))
        }),
    }
}

/// An audio bus's name when the user gives none: "Bus A"… by its place.
pub fn bus_default_name(p: &bv_timeline::Project, id: bv_timeline::BusId) -> String {
    p.bus_index(id).map_or_else(String::new, |i| {
        trf!("Bus {}", bv_timeline::busses::bus_letter(i))
    })
}

/// An assignable effects chain's name: the user's, else its effects'
/// ("Reverb", "Delay (Echo) + Reverb"), else "FX 1"… by its place.
pub fn chain_name(p: &bv_timeline::Project, id: bv_timeline::FxChainId) -> String {
    match p.fx_chain(id) {
        Some(c) if !c.channel.name.is_empty() => c.channel.name.clone(),
        _ => chain_default_name(p, id),
    }
}

/// An assignable effects chain's name when the user gives none.
pub fn chain_default_name(p: &bv_timeline::Project, id: bv_timeline::FxChainId) -> String {
    let Some((i, c)) = p.fx_chains.iter().enumerate().find(|(_, c)| c.id == id) else {
        return String::new();
    };
    if c.channel.effects.is_empty() {
        return trf!("FX {}", i + 1);
    }
    c.channel
        .effects
        .iter()
        .map(crate::ofx::fx_name)
        .collect::<Vec<_>>()
        .join(" + ")
}

pub fn track_name(t: &bv_timeline::Track) -> String {
    if t.adjustment
        && let Some(n) = t.name.strip_prefix("Adjustment ")
        && n.parse::<u32>().is_ok()
    {
        return trf!("Adjustment {}", n);
    }
    let prefix = match t.kind {
        bv_timeline::TrackKind::Video => "Video ",
        bv_timeline::TrackKind::Audio => "Audio ",
    };
    match t.name.strip_prefix(prefix) {
        Some(n) if n.parse::<u32>().is_ok() => match t.kind {
            bv_timeline::TrackKind::Video => trf!("Video {}", n),
            bv_timeline::TrackKind::Audio => trf!("Audio {}", n),
        },
        _ => t.name.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_the_source_text() {
        assert_eq!(translate(Language::English, "Save"), "Save");
        assert_eq!(
            translate(Language::English, "Not in any catalog"),
            "Not in any catalog"
        );
    }

    #[test]
    fn portuguese_comes_from_the_catalog() {
        assert_eq!(translate(Language::PortugueseBrazil, "Save"), "Salvar");
        assert_eq!(
            translate(Language::PortugueseBrazil, "Customize Keyboard…"),
            "Personalizar teclado…"
        );
        // Missing texts fall back to English.
        assert_eq!(
            translate(Language::PortugueseBrazil, "Not in any catalog"),
            "Not in any catalog"
        );
    }

    #[test]
    fn catalogs_are_sorted_unique_and_templates_match() {
        let count = |s: &str| s.matches('{').count() - 2 * s.matches("{{").count();
        for lang in Language::ALL {
            let l = lang.locale();
            let code = l.code;
            let mut seen = std::collections::HashSet::new();
            for (en, t) in l.texts {
                assert!(seen.insert(*en), "{code}: {en:?} is listed twice");
                assert_eq!(
                    count(en),
                    count(t),
                    "{code}: {en:?} and {t:?} take different values"
                );
            }
            for w in l.texts.windows(2) {
                assert!(
                    w[0].0.to_lowercase() <= w[1].0.to_lowercase(),
                    "{code}: {:?} comes before {:?}",
                    w[1].0,
                    w[0].0
                );
            }
            for (one, other, forms) in l.plurals {
                assert_eq!(
                    forms.len(),
                    l.plural_forms.len(),
                    "{code}: {other:?} needs one form per plural category"
                );
                for f in *forms {
                    assert_eq!(count(one), count(f), "{code}: {f:?} takes other values");
                }
                assert_eq!(count(one), count(other), "{one:?} and {other:?}");
            }
            for (_, en, t) in l.context {
                assert_eq!(count(en), count(t), "{code}: {en:?} and {t:?}");
            }
            // Every category the rule gives has a form.
            for n in 0..1000 {
                let c = (l.plural)(n);
                assert!(
                    l.plural_forms.contains(&c),
                    "{code}: {n} is {c:?}, which has no form"
                );
            }
            assert_eq!(Language::from_code(code), Some(*lang));
        }
    }

    #[test]
    fn counts_pick_the_form() {
        let (one, other) = ("{} file", "{} files");
        assert_eq!(plural_in(Language::English, 1, one, other), "{} file");
        assert_eq!(plural_in(Language::English, 0, one, other), "{} files");
        assert_eq!(plural_in(Language::English, 2, one, other), "{} files");
        // Portuguese counts 0 and 1 as singular.
        let pt = |n| plural_in(Language::PortugueseBrazil, n, one, other);
        assert_eq!(pt(0), "{} arquivo");
        assert_eq!(pt(1), "{} arquivo");
        assert_eq!(pt(2), "{} arquivos");
        // A text the catalog lacks keeps the English forms.
        assert_eq!(
            plural_in(Language::PortugueseBrazil, 3, "{} thing", "{} things"),
            "{} things"
        );
    }

    #[test]
    fn languages_are_saved_by_tag() {
        let json = serde_json::to_string(&Language::PortugueseBrazil).unwrap();
        assert_eq!(json, "\"pt-BR\"");
        let back: Language = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Language::PortugueseBrazil);
        // A language this version does not have reads as English.
        let unknown: Language = serde_json::from_str("\"xx-YY\"").unwrap();
        assert_eq!(unknown, Language::English);
    }

    #[test]
    fn fill_places_values() {
        assert_eq!(fill("{} of {}", &[&1, &"two"]), "1 of two");
        assert_eq!(fill("{1} before {0}", &[&"a", &"b"]), "b before a");
        assert_eq!(fill("{:.1} dB", &[&-3.04]), "-3.0 dB");
        assert_eq!(fill("{{literal}} {}", &[&7]), "{literal} 7");
        assert_eq!(fill("missing {}", &[]), "missing {}");
    }

    #[test]
    fn templates_follow_the_catalog() {
        let pt = translate(Language::PortugueseBrazil, "Saved to {}");
        assert_eq!(fill(pt, &[&"x.srt"]), "Salvo em x.srt");
    }
}

#[cfg(test)]
mod key_name_tests {
    use super::mac_key_names;

    #[test]
    fn the_mac_names_its_keys_its_own_way() {
        assert_eq!(
            mac_key_names("Ctrl+drag: fine; Alt+D, A"),
            "Cmd+drag: fine; Option+D, A"
        );
        assert_eq!(
            mac_key_names("(Ctrl: finer) Shift+Alt: round"),
            "(Cmd: finer) Shift+Option: round"
        );
        // Words that only begin so stay.
        assert_eq!(mac_key_names("Altura e Alternate"), "Altura e Alternate");
        assert_eq!(mac_key_names("Ctrls"), "Ctrls");
        assert_eq!(mac_key_names("Ctrl"), "Cmd");
    }
}
