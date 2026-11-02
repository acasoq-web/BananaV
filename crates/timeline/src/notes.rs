//! Project Notes, as VEGAS 2026's Help describes them: notes left in a
//! project — each at a timeline time, in a color and with a label (to sort
//! them), its text, when it was written, resolved or still to address.
//! The window is `ui-egui`'s `tabs/notes.rs`. Projects saved with the old
//! single text (`Project::notes`) get it as a first note when they open.

use bv_core::Tick;
use serde::{Deserialize, Serialize};

use crate::model::Project;

/// A note's colors (its header bar), as the window's lists offer them.
pub const NOTE_COLORS: [[u8; 3]; 8] = [
    [61, 120, 146],
    [168, 64, 64],
    [196, 120, 48],
    [180, 160, 48],
    [72, 140, 72],
    [64, 96, 176],
    [128, 80, 160],
    [110, 110, 110],
];

/// A project note.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectNote {
    pub id: u32,
    /// Where in the timeline it is about.
    pub time: Tick,
    #[serde(default)]
    pub text: String,
    /// Its label ("Untitled" when empty) and color (an index of
    /// [`NOTE_COLORS`]).
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub color: u8,
    /// When it was written: seconds since 1970 (UTC), and the clock's
    /// offset from UTC then, in seconds.
    #[serde(default)]
    pub created: i64,
    #[serde(default)]
    pub utc_offset: i64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub resolved: bool,
}

impl Project {
    /// Add New Note: a note at `time`, with `label` and `color`, written at
    /// `created` (seconds since 1970, UTC, and the clock's offset); its id.
    pub fn add_note(&mut self, time: Tick, label: &str, color: u8, created: (i64, i64)) -> u32 {
        let id = self
            .project_notes
            .iter()
            .map(|n| n.id + 1)
            .max()
            .unwrap_or(1);
        self.project_notes.push(ProjectNote {
            id,
            time: time.max(Tick::ZERO),
            text: String::new(),
            label: label.to_string(),
            color: color % NOTE_COLORS.len() as u8,
            created: created.0,
            utc_offset: created.1,
            resolved: false,
        });
        id
    }

    pub fn note_mut(&mut self, id: u32) -> Option<&mut ProjectNote> {
        self.project_notes.iter_mut().find(|n| n.id == id)
    }

    pub fn remove_note(&mut self, id: u32) {
        self.project_notes.retain(|n| n.id != id);
    }

    /// The old single text of Project Notes, as the first note (at the
    /// start, untitled).
    pub(crate) fn upgrade_notes(&mut self) {
        if self.notes.trim().is_empty() || !self.project_notes.is_empty() {
            return;
        }
        let text = std::mem::take(&mut self.notes);
        let id = self.add_note(Tick::ZERO, "", 0, (0, 0));
        if let Some(n) = self.note_mut(id) {
            n.text = text;
        }
    }
}

/// `created` (seconds since 1970, UTC) at `offset` seconds from UTC, as
/// the civil date and time: (year, month 1–12, day, weekday 0 = Sunday,
/// hour, minute, second).
pub fn civil_time(created: i64, offset: i64) -> (i64, u32, u32, u32, u32, u32, u32) {
    let t = created + offset;
    let days = t.div_euclid(86_400);
    let secs = t.rem_euclid(86_400);
    let (y, m, d) = bv_core::date::civil_from_days(days);
    // 1970-01-01 was a Thursday.
    let weekday = (days + 4).rem_euclid(7) as u32;
    (
        y,
        m,
        d,
        weekday,
        (secs / 3600) as u32,
        (secs % 3600 / 60) as u32,
        (secs % 60) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ProjectSettings;

    /// Notes are added at a time with a label and a color, changed and
    /// removed; an old project's text becomes the first note.
    #[test]
    fn notes_come_and_go() {
        let mut p = Project::new(ProjectSettings::default());
        let a = p.add_note(Tick::from_seconds(3), "Mix", 9, (1_667_827_201, -3 * 3600));
        assert_eq!(p.project_notes[0].color, 1, "wrapped");
        p.note_mut(a).unwrap().resolved = true;
        let b = p.add_note(Tick::from_seconds(-1), "", 0, (0, 0));
        assert_eq!(p.project_notes[1].time, Tick::ZERO);
        p.remove_note(a);
        assert_eq!(p.project_notes.len(), 1);
        assert_eq!(p.project_notes[0].id, b);
        let mut old = Project::new(ProjectSettings::default());
        old.notes = "Call the client".into();
        let back = Project::from_json(&old.to_json()).unwrap();
        assert_eq!(back.project_notes.len(), 1);
        assert_eq!(back.project_notes[0].text, "Call the client");
        assert!(back.notes.is_empty());
    }

    /// The Help's picture's date: Monday, November 7, 2022 1:20:01 PM.
    #[test]
    fn civil_times() {
        // 2022-11-07 13:20:01 UTC.
        let (y, m, d, wd, h, mi, s) = civil_time(1_667_827_201, 0);
        assert_eq!((y, m, d, wd, h, mi, s), (2022, 11, 7, 1, 13, 20, 1));
        // Three hours behind: 10:20:01 the same day.
        assert_eq!(civil_time(1_667_827_201, -3 * 3600).4, 10);
    }
}
