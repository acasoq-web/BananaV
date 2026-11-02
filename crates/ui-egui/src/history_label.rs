//! Names an edit by comparing the project before and after it, for the
//! "Undo …" / "Redo …" menu items (like VEGAS shows "Undo Delete Event").

use crate::i18n::tr;
use std::collections::HashMap;

use bv_timeline::{Clip, ClipId, Project};

/// Short pt-BR description of what changed from `before` to `after`.
pub fn describe(before: &Project, after: &Project) -> &'static str {
    if before.tracks.len() != after.tracks.len() {
        return if after.tracks.len() > before.tracks.len() {
            tr("Insert Track")
        } else {
            tr("Delete Track")
        };
    }
    // Only the ruler format (Options › Ruler Format).
    let mut ruler_only = after.settings;
    ruler_only.ruler_format = before.settings.ruler_format;
    if ruler_only == before.settings && before.settings != after.settings {
        return tr("Ruler Format");
    }
    if before.settings != after.settings
        || before.prerender_folder != after.prerender_folder
        || before.recorded_folder != after.recorded_folder
        || before.summary != after.summary
        || before.audio_cd != after.audio_cd
    {
        return tr("Project Properties");
    }
    let old: HashMap<ClipId, &Clip> = before.clips().map(|(_, c)| (c.id, c)).collect();
    let new: HashMap<ClipId, &Clip> = after.clips().map(|(_, c)| (c.id, c)).collect();
    let added = new.keys().filter(|id| !old.contains_key(id)).count();
    let removed = old.keys().filter(|id| !new.contains_key(id)).count();
    match (added, removed) {
        (0, 0) => {}
        (_, 0) => {
            // A split leaves the left half in place with a shorter duration.
            let split = new.values().any(|c| {
                old.get(&c.id)
                    .is_some_and(|o| o.start == c.start && c.duration < o.duration)
            });
            return if split {
                tr("Split Event")
            } else {
                tr("Add Event")
            };
        }
        (0, _) => {
            let joined = new.values().any(|c| {
                old.get(&c.id)
                    .is_some_and(|o| o.start == c.start && c.duration > o.duration)
            });
            return if joined {
                tr("Rejoin Events")
            } else {
                tr("Delete Event")
            };
        }
        _ => return tr("Edit Events"),
    }
    for (id, c) in &new {
        let Some(o) = old.get(id) else { continue };
        if o == c {
            continue;
        }
        let label = if o.transform_keys != c.transform_keys {
            tr("Event Pan/Crop Position Keyframe")
        } else if o.transform != c.transform {
            tr("Event Pan/Crop")
        } else if o.effects != c.effects {
            tr("Event FX")
        } else if o.start != c.start && o.duration == c.duration && o.source_in == c.source_in {
            tr("Move Event")
        } else if o.start == c.start && o.duration == c.duration && o.source_in != c.source_in {
            tr("Slip Event Media")
        } else if o.start != c.start || o.duration != c.duration {
            tr("Trim Event")
        } else if o.fade_in != c.fade_in
            || o.fade_out != c.fade_out
            || o.fade_in_curve != c.fade_in_curve
            || o.fade_out_curve != c.fade_out_curve
        {
            tr("Event Fade")
        } else if o.level != c.level {
            tr("Event Level")
        } else if o.rate != c.rate {
            tr("Event Playback Rate")
        } else if o.transition != c.transition {
            tr("Event Transition")
        } else if o.group != c.group {
            tr("Group Events")
        } else if o.muted != c.muted || o.locked != c.locked {
            tr("Event Switches")
        } else if o.channels != c.channels {
            tr("Event Channels")
        } else {
            tr("Event Properties")
        };
        return label;
    }
    for (o, t) in before.tracks.iter().zip(&after.tracks) {
        if o.id != t.id {
            return tr("Move Track");
        }
        if o.effects != t.effects {
            return tr("Track FX");
        }
        if o != t {
            return tr("Track Properties");
        }
    }
    if before.media != after.media {
        return if after.media.len() > before.media.len() {
            tr("Add Media")
        } else if after.media.len() < before.media.len() {
            tr("Remove Media")
        } else if before
            .media
            .iter()
            .zip(&after.media)
            .any(|(a, b)| a.markers != b.markers)
        {
            tr("Media Markers")
        } else {
            tr("Media FX")
        };
    }
    if before.markers != after.markers {
        return tr("Marker");
    }
    if before.regions != after.regions {
        return tr("Region");
    }
    if before.loop_region != after.loop_region {
        return tr("Time Selection");
    }
    if before.master != after.master {
        return tr("Master Bus");
    }
    if before.notes != after.notes || before.project_notes != after.project_notes {
        return tr("Project Notes");
    }
    tr("Editing")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_core::Tick;
    use bv_timeline::{Generator, MediaInfo};

    #[test]
    fn names_common_edits() {
        let mut p = Project::default();
        let v = p.tracks[0].id;
        let m = p.add_media(MediaInfo::generated(Generator::ColorBars, &p.settings));
        let before = p.clone();
        let id = p.insert_media(m, Tick::ZERO, Some(v), None).unwrap()[0];
        assert_eq!(describe(&before, &p), "Add Event");
        let before = p.clone();
        p.split(id, Tick::from_seconds(1)).unwrap();
        assert_eq!(describe(&before, &p), "Split Event");
        let before = p.clone();
        p.move_clip(id, Tick::from_seconds(2), None).unwrap();
        assert_eq!(describe(&before, &p), "Move Event");
        let before = p.clone();
        p.delete_clips(&[id], false).unwrap();
        assert_eq!(describe(&before, &p), "Delete Event");
    }
}
