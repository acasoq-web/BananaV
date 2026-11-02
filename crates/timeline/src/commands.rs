//! Command markers (Insert › Command), as VEGAS's Help describes them:
//! metadata commands on the timeline's command bar — a command (one of
//! [`COMMANDS`] or one typed in), the argument it passes (a web address,
//! a caption's text), a comment for whoever edits the project — that
//! players of streaming files act on when playback reaches them. BananaV
//! keeps them in the project, on the command bar and in Edit Details'
//! Commands; writing them into rendered files is still to come.

use bv_core::Tick;
use serde::{Deserialize, Serialize};

use crate::model::{MarkerId, Project};

/// The commands the Command Properties dialog offers (VEGAS's list; any
/// other text is a custom command).
pub const COMMANDS: [&str; 9] = [
    "URL",
    "Text",
    "WMClosedCaption",
    "WMTextBodyText",
    "WMTextHeadline",
    "Scott EOM",
    "Scott Cue In",
    "608CC1",
    "608CC3",
];

/// A command marker.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommandMarker {
    pub id: MarkerId,
    /// When it happens.
    pub time: Tick,
    pub command: String,
    /// What the command is given (its argument).
    #[serde(default)]
    pub parameter: String,
    /// A reminder of what it is for, like a marker's name.
    #[serde(default)]
    pub comment: String,
}

impl Project {
    /// A command marker at `time`; its id.
    pub fn add_command(
        &mut self,
        time: Tick,
        command: &str,
        parameter: &str,
        comment: &str,
    ) -> MarkerId {
        let id = MarkerId(self.alloc_id());
        self.commands.push(CommandMarker {
            id,
            time: time.max(Tick::ZERO),
            command: command.to_string(),
            parameter: parameter.to_string(),
            comment: comment.to_string(),
        });
        self.sort_commands();
        id
    }

    pub fn command(&self, id: MarkerId) -> Option<&CommandMarker> {
        self.commands.iter().find(|c| c.id == id)
    }

    /// Changes a command marker, its place included.
    pub fn set_command(&mut self, id: MarkerId, edit: impl FnOnce(&mut CommandMarker)) {
        if let Some(c) = self.commands.iter_mut().find(|c| c.id == id) {
            edit(c);
            c.time = c.time.max(Tick::ZERO);
            c.id = id;
        }
        self.sort_commands();
    }

    pub(crate) fn sort_commands(&mut self) {
        self.commands.sort_by_key(|c| (c.time, c.id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_stay_in_time_order() {
        let mut p = Project::default();
        let late = p.add_command(Tick::from_seconds(8), "URL", "https://example.com", "");
        let early = p.add_command(Tick::from_seconds(2), "Text", "Hello", "greeting");
        assert_eq!(p.commands[0].id, early);
        p.set_command(early, |c| c.time = Tick::from_seconds(9));
        assert_eq!(p.commands[0].id, late, "moved after it");
        assert_eq!(p.command(early).unwrap().comment, "greeting");
        p.remove_marker(late);
        assert_eq!(p.commands.len(), 1, "removed like any marker");
    }

    #[test]
    fn ripple_edits_carry_commands_with_the_markers() {
        let mut p = Project::default();
        let c = p.add_command(Tick::from_seconds(5), "Text", "", "");
        p.insert_space(Tick::from_seconds(1), Tick::from_seconds(2), &[], true);
        assert_eq!(p.command(c).unwrap().time, Tick::from_seconds(7));
        p.delete_range(
            bv_core::TimeRange::new(Tick::from_seconds(6), Tick::from_seconds(2)),
            &[],
            true,
        );
        assert!(p.command(c).is_none(), "inside the removed time");
    }
}
