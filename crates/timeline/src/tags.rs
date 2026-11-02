//! Media tags and searches, as VEGAS's Help describes them ("Using the
//! Project Media window", "Sorting your media with bins"): words given to
//! Project Media's items to find them by — each tag a bin of its own
//! under Tagged Media —, and Search Media Bins' conditions, whose results
//! can be kept as a bin or as a smart bin, a search kept and run again
//! whenever it is shown.

use serde::{Deserialize, Serialize};

use crate::bins::BinId;
use crate::model::{MediaId, MediaInfo, MediaSource, Project};

/// A media's tags.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MediaTags {
    pub media: MediaId,
    pub tags: Vec<String>,
}

/// What a search condition looks at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SearchField {
    Name,
    /// The file's path (generated media have none).
    Path,
    /// Video, audio or image ([`MediaKind`]'s names).
    Type,
    Tag,
    /// Seconds.
    Length,
    Width,
    Height,
    FrameRate,
    SampleRate,
    Channels,
    TapeName,
}

impl SearchField {
    pub const ALL: [SearchField; 11] = [
        SearchField::Name,
        SearchField::Path,
        SearchField::Type,
        SearchField::Tag,
        SearchField::Length,
        SearchField::Width,
        SearchField::Height,
        SearchField::FrameRate,
        SearchField::SampleRate,
        SearchField::Channels,
        SearchField::TapeName,
    ];

    /// Its English name (the interface translates it).
    pub fn name(self) -> &'static str {
        match self {
            SearchField::Name => "Name",
            SearchField::Path => "Path",
            SearchField::Type => "Type",
            SearchField::Tag => "Tag",
            SearchField::Length => "Length (seconds)",
            SearchField::Width => "Width",
            SearchField::Height => "Height",
            SearchField::FrameRate => "Frame Rate",
            SearchField::SampleRate => "Sample Rate",
            SearchField::Channels => "Channels",
            SearchField::TapeName => "Tape Name",
        }
    }

    /// It holds a number (compared as one), else text.
    pub fn numeric(self) -> bool {
        matches!(
            self,
            SearchField::Length
                | SearchField::Width
                | SearchField::Height
                | SearchField::FrameRate
                | SearchField::SampleRate
                | SearchField::Channels
        )
    }

    /// The comparisons that suit it.
    pub fn ops(self) -> &'static [SearchOp] {
        use SearchOp::*;
        match self {
            _ if self.numeric() => &[Is, IsNot, Less, Greater],
            SearchField::Type | SearchField::Tag => &[Is, IsNot],
            _ => &[Contains, DoesNotContain, Is, IsNot, StartsWith, EndsWith],
        }
    }
}

/// How a condition compares (text without regard to case).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SearchOp {
    Contains,
    DoesNotContain,
    Is,
    IsNot,
    StartsWith,
    EndsWith,
    Less,
    Greater,
}

impl SearchOp {
    /// Its English name (the interface translates it).
    pub fn name(self) -> &'static str {
        match self {
            SearchOp::Contains => "contains",
            SearchOp::DoesNotContain => "does not contain",
            SearchOp::Is => "is",
            SearchOp::IsNot => "is not",
            SearchOp::StartsWith => "starts with",
            SearchOp::EndsWith => "ends with",
            SearchOp::Less => "is less than",
            SearchOp::Greater => "is greater than",
        }
    }
}

/// A media's kind, as Media By Type sorts them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MediaKind {
    Video,
    Audio,
    Image,
}

impl MediaKind {
    pub fn of(m: &MediaInfo) -> MediaKind {
        if m.still {
            MediaKind::Image
        } else if m.video.is_some() {
            MediaKind::Video
        } else {
            MediaKind::Audio
        }
    }

    /// Its English name, the one a Type condition takes.
    pub fn name(self) -> &'static str {
        match self {
            MediaKind::Video => "Video",
            MediaKind::Audio => "Audio",
            MediaKind::Image => "Image",
        }
    }
}

/// One condition: `field` compared by `op` with `value`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchCondition {
    pub field: SearchField,
    pub op: SearchOp,
    pub value: String,
}

impl SearchCondition {
    pub fn new(field: SearchField, op: SearchOp, value: &str) -> SearchCondition {
        SearchCondition {
            field,
            op,
            value: value.to_string(),
        }
    }
}

/// Where a search looks: all the media, or a bin and the bins in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchIn {
    All,
    Bin(BinId),
}

/// Search Media Bins' search: its conditions, all of them to be met or
/// any one, and where it looks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MediaSearch {
    pub conditions: Vec<SearchCondition>,
    /// All the conditions must be met (else any one).
    pub all: bool,
    pub within: SearchIn,
}

impl Default for MediaSearch {
    fn default() -> MediaSearch {
        MediaSearch {
            conditions: vec![SearchCondition::new(
                SearchField::Name,
                SearchOp::Contains,
                "",
            )],
            all: true,
            within: SearchIn::All,
        }
    }
}

/// A smart bin: a search kept under a name, its media whatever matches
/// when it is shown.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SmartBin {
    pub id: BinId,
    pub name: String,
    pub search: MediaSearch,
}

/// `a` and `b` the same tag: their letters in any case, the spaces at
/// their ends left out.
fn same_tag(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// A number typed as a condition's value ("1,5" too).
fn number(text: &str) -> Option<f64> {
    text.trim().replace(',', ".").parse().ok()
}

impl SearchCondition {
    /// Whether `m`, tagged `tags`, meets it.
    fn meets(&self, m: &MediaInfo, tags: &[String]) -> bool {
        let field = self.field;
        if field.numeric() {
            let value = match field {
                SearchField::Length => Some(m.duration.as_seconds_f64()),
                SearchField::Width => m.video.map(|v| f64::from(v.width)),
                SearchField::Height => m.video.map(|v| f64::from(v.height)),
                SearchField::FrameRate => m.video.map(|v| v.frame_rate.as_f64()),
                SearchField::SampleRate => m.audio.map(|a| f64::from(a.sample_rate.0)),
                SearchField::Channels => m.audio.map(|a| f64::from(a.channels)),
                _ => None,
            };
            let (Some(v), Some(want)) = (value, number(&self.value)) else {
                // What has no such number meets only "is not".
                return self.op == SearchOp::IsNot;
            };
            // Equal to the hundredth: rates and lengths are not round.
            let equal = (v - want).abs() < 0.005;
            return match self.op {
                SearchOp::Is => equal,
                SearchOp::IsNot => !equal,
                SearchOp::Less => v < want && !equal,
                SearchOp::Greater => v > want && !equal,
                _ => false,
            };
        }
        let want = self.value.trim().to_lowercase();
        if field == SearchField::Tag {
            let has = tags.iter().any(|t| same_tag(t, &want));
            return match self.op {
                SearchOp::IsNot => !has,
                _ => has,
            };
        }
        let text = match field {
            SearchField::Name => m.name.clone(),
            SearchField::Path => match &m.source {
                MediaSource::File { path } => path.clone(),
                MediaSource::Generated(_) => String::new(),
            },
            SearchField::Type => MediaKind::of(m).name().to_string(),
            SearchField::TapeName => m.reel.tape_name.clone(),
            _ => String::new(),
        }
        .to_lowercase();
        match self.op {
            SearchOp::Contains => text.contains(&want),
            SearchOp::DoesNotContain => !text.contains(&want),
            SearchOp::Is => text == want,
            SearchOp::IsNot => text != want,
            SearchOp::StartsWith => text.starts_with(&want),
            SearchOp::EndsWith => text.ends_with(&want),
            SearchOp::Less | SearchOp::Greater => false,
        }
    }
}

impl Project {
    /// The tags of `media` (a media or a subclip).
    pub fn tags_of(&self, media: MediaId) -> &[String] {
        self.media_tags
            .iter()
            .find(|t| t.media == media)
            .map_or(&[], |t| t.tags.as_slice())
    }

    /// Tags `media` with `tag` (its spaces at the ends left out); whether
    /// it was not tagged so before (in any case).
    pub fn add_tag(&mut self, media: MediaId, tag: &str) -> bool {
        let tag = tag.trim();
        if tag.is_empty() || self.tags_of(media).iter().any(|t| same_tag(t, tag)) {
            return false;
        }
        match self.media_tags.iter_mut().find(|t| t.media == media) {
            Some(t) => t.tags.push(tag.to_string()),
            None => self.media_tags.push(MediaTags {
                media,
                tags: vec![tag.to_string()],
            }),
        }
        true
    }

    /// Takes tag `tag` (in any case) off `media`.
    pub fn remove_tag(&mut self, media: MediaId, tag: &str) {
        for t in self.media_tags.iter_mut().filter(|t| t.media == media) {
            t.tags.retain(|x| !same_tag(x, tag));
        }
        self.media_tags.retain(|t| !t.tags.is_empty());
    }

    /// Every tag given to a media, each once (as first written), by name:
    /// Tagged Media's bins.
    pub fn all_tags(&self) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for t in self.media_tags.iter().flat_map(|t| &t.tags) {
            if !v.iter().any(|x| same_tag(x, t)) {
                v.push(t.clone());
            }
        }
        v.sort_by_key(|t| t.to_lowercase());
        v
    }

    /// The media tagged `tag`, in the project's order.
    pub fn media_tagged(&self, tag: &str) -> Vec<MediaId> {
        self.searchable()
            .into_iter()
            .filter(|m| self.tags_of(m.id).iter().any(|t| same_tag(t, tag)))
            .map(|m| m.id)
            .collect()
    }

    /// Project Media's items: each media, then its subclips.
    fn searchable(&self) -> Vec<MediaInfo> {
        self.media
            .iter()
            .flat_map(|m| {
                std::iter::once(m.clone()).chain(
                    self.subclips
                        .iter()
                        .filter(|s| s.parent == m.id)
                        .filter_map(|s| self.subclip_info(s.id)),
                )
            })
            .collect()
    }

    /// The media in bin `bin` and the bins in it.
    fn in_bin(&self, bin: BinId) -> Vec<MediaId> {
        let mut bins = vec![bin];
        let mut i = 0;
        while i < bins.len() {
            let inside: Vec<BinId> = self
                .bins
                .iter()
                .filter(|b| b.parent == Some(bins[i]))
                .map(|b| b.id)
                .collect();
            bins.extend(inside);
            i += 1;
        }
        self.bins
            .iter()
            .filter(|b| bins.contains(&b.id))
            .flat_map(|b| b.media.iter().copied())
            .collect()
    }

    /// The media `search` finds, in the project's order. No conditions
    /// find everything where it looks.
    pub fn search_media(&self, search: &MediaSearch) -> Vec<MediaId> {
        let within = match search.within {
            SearchIn::All => None,
            SearchIn::Bin(b) => Some(self.in_bin(b)),
        };
        self.searchable()
            .into_iter()
            .filter(|m| within.as_ref().is_none_or(|w| w.contains(&m.id)))
            .filter(|m| {
                let tags = self.tags_of(m.id);
                let mut met = search.conditions.iter().map(|c| c.meets(m, tags));
                if search.conditions.is_empty() {
                    true
                } else if search.all {
                    met.all(|x| x)
                } else {
                    met.any(|x| x)
                }
            })
            .map(|m| m.id)
            .collect()
    }

    /// Keeps `search` as a smart bin named `name`.
    pub fn add_smart_bin(&mut self, name: &str, search: MediaSearch) -> BinId {
        let id = BinId(self.alloc_id());
        self.smart_bins.push(SmartBin {
            id,
            name: name.to_string(),
            search,
        });
        id
    }

    pub fn smart_bin(&self, id: BinId) -> Option<&SmartBin> {
        self.smart_bins.iter().find(|b| b.id == id)
    }

    pub fn smart_bin_mut(&mut self, id: BinId) -> Option<&mut SmartBin> {
        self.smart_bins.iter_mut().find(|b| b.id == id)
    }

    pub fn remove_smart_bin(&mut self, id: BinId) {
        self.smart_bins.retain(|b| b.id != id);
    }

    /// Keeps what `search` finds now as a new bin named `name` (in the
    /// bin it looked in, else at the top).
    pub fn save_search_as_bin(&mut self, name: &str, search: &MediaSearch) -> BinId {
        let found = self.search_media(search);
        let parent = match search.within {
            SearchIn::Bin(b) if self.bin(b).is_some() => Some(b),
            _ => None,
        };
        let id = self
            .add_bin(parent, name)
            .or_else(|_| self.add_bin(None, name))
            .expect("a bin at the top");
        for m in found {
            let _ = self.add_to_bin(id, m);
        }
        id
    }

    /// A media (or subclip) leaving the project takes its tags with it.
    pub(crate) fn forget_tags(&mut self, media: MediaId) {
        self.media_tags.retain(|t| t.media != media);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Generator, ProjectSettings};
    use bv_core::Tick;

    fn project() -> (Project, MediaId, MediaId) {
        let mut p = Project::default();
        let s = ProjectSettings::default();
        let mut bars = MediaInfo::generated(Generator::ColorBars, &s);
        bars.name = "Beach bars".into();
        bars.duration = Tick::from_seconds(10);
        let bars = p.add_media(bars);
        let mut tone = MediaInfo::generated(
            Generator::TestTone {
                frequency_hz: 440.0,
                level_db: -6.0,
            },
            &s,
        );
        tone.name = "Tone".into();
        tone.duration = Tick::from_seconds(3);
        let tone = p.add_media(tone);
        (p, bars, tone)
    }

    #[test]
    fn tags_are_words_given_to_media_each_once() {
        let (mut p, bars, tone) = project();
        assert!(p.add_tag(bars, " Friends "));
        assert!(!p.add_tag(bars, "friends"), "the same tag in another case");
        assert!(!p.add_tag(bars, "  "));
        p.add_tag(bars, "Dresden");
        p.add_tag(tone, "anne");
        p.add_tag(tone, "FRIENDS");
        assert_eq!(p.tags_of(bars), ["Friends", "Dresden"]);
        assert_eq!(p.all_tags(), ["anne", "Dresden", "Friends"]);
        assert_eq!(p.media_tagged("friends"), [bars, tone]);
        p.remove_tag(bars, "FRIENDS");
        assert_eq!(p.tags_of(bars), ["Dresden"]);
        // A media leaving the project takes its tags along.
        p.remove_media(tone).unwrap();
        assert_eq!(p.all_tags(), ["Dresden"]);
    }

    #[test]
    fn searches_meet_all_or_any_of_their_conditions() {
        use SearchField::*;
        use SearchOp::*;
        let (mut p, bars, tone) = project();
        p.add_tag(tone, "Voice");
        let search = |conditions: Vec<SearchCondition>, all| MediaSearch {
            conditions,
            all,
            within: SearchIn::All,
        };
        let c = SearchCondition::new;
        assert_eq!(
            p.search_media(&search(vec![c(Name, Contains, "BEACH")], true)),
            [bars]
        );
        assert_eq!(
            p.search_media(&search(vec![c(Type, Is, "audio")], true)),
            [tone]
        );
        assert_eq!(
            p.search_media(&search(vec![c(Tag, Is, "voice")], true)),
            [tone]
        );
        assert_eq!(
            p.search_media(&search(vec![c(Tag, IsNot, "voice")], true)),
            [bars]
        );
        assert_eq!(
            p.search_media(&search(vec![c(Length, Greater, "9,5")], true)),
            [bars]
        );
        let width = p.media(bars).unwrap().video.unwrap().width.to_string();
        assert_eq!(
            p.search_media(&search(vec![c(Width, Is, &width)], true)),
            [bars]
        );
        let two = vec![c(Name, StartsWith, "beach"), c(Tag, Is, "Voice")];
        assert!(p.search_media(&search(two.clone(), true)).is_empty());
        assert_eq!(p.search_media(&search(two, false)), [bars, tone]);
        // In a bin: only its media (and its bins').
        let bin = p.add_bin(None, "Sounds").unwrap();
        let inner = p.add_bin(Some(bin), "Voices").unwrap();
        p.add_to_bin(inner, tone).unwrap();
        let within = MediaSearch {
            within: SearchIn::Bin(bin),
            ..MediaSearch::default()
        };
        assert_eq!(p.search_media(&within), [tone]);
        // Kept as a bin: what it found then; as a smart bin: the search.
        let found = p.save_search_as_bin("Found", &within);
        assert_eq!(p.bin(found).unwrap().media, [tone]);
        assert_eq!(p.bin(found).unwrap().parent, Some(bin));
        let smart = p.add_smart_bin("Beach", search(vec![c(Name, Contains, "beach")], true));
        assert_eq!(
            p.search_media(&p.smart_bin(smart).unwrap().search.clone()),
            [bars]
        );
        p.remove_smart_bin(smart);
        assert!(p.smart_bins.is_empty());
    }
}
