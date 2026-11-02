//! Media bins: folders of Project Media that sort the project's media
//! without moving the files (VEGAS's custom bins). A bin can hold bins;
//! media can be in several bins, and stays in the project when it leaves
//! one.

use serde::{Deserialize, Serialize};

use crate::edit::EditError;
use crate::model::{MediaId, Project};

type Result<T> = std::result::Result<T, EditError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BinId(pub u64);

/// A bin: its name, the bin it is in (`None`: at the top), its media.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MediaBin {
    pub id: BinId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<BinId>,
    #[serde(default)]
    pub media: Vec<MediaId>,
}

impl Project {
    pub fn bin(&self, id: BinId) -> Option<&MediaBin> {
        self.bins.iter().find(|b| b.id == id)
    }

    fn bin_mut(&mut self, id: BinId) -> Result<&mut MediaBin> {
        self.bins
            .iter_mut()
            .find(|b| b.id == id)
            .ok_or(EditError::UnknownBin(id))
    }

    /// The bins in `parent` (`None`: at the top), by name.
    pub fn bins_in(&self, parent: Option<BinId>) -> Vec<&MediaBin> {
        let mut v: Vec<&MediaBin> = self.bins.iter().filter(|b| b.parent == parent).collect();
        v.sort_by_key(|b| b.name.to_lowercase());
        v
    }

    /// A new, empty bin in `parent`.
    pub fn add_bin(&mut self, parent: Option<BinId>, name: &str) -> Result<BinId> {
        if let Some(p) = parent {
            self.bin_mut(p)?;
        }
        let id = BinId(self.alloc_id());
        self.bins.push(MediaBin {
            id,
            name: name.to_string(),
            parent,
            media: Vec::new(),
        });
        Ok(id)
    }

    pub fn rename_bin(&mut self, id: BinId, name: &str) -> Result<()> {
        self.bin_mut(id)?.name = name.to_string();
        Ok(())
    }

    /// Deletes the bin and the bins in it; their media stay in the
    /// project.
    pub fn remove_bin(&mut self, id: BinId) -> Result<()> {
        self.bin_mut(id)?;
        let mut gone = vec![id];
        let mut i = 0;
        while i < gone.len() {
            let inside: Vec<BinId> = self
                .bins
                .iter()
                .filter(|b| b.parent == Some(gone[i]))
                .map(|b| b.id)
                .collect();
            gone.extend(inside);
            i += 1;
        }
        self.bins.retain(|b| !gone.contains(&b.id));
        Ok(())
    }

    /// Puts media into the bin (once).
    pub fn add_to_bin(&mut self, id: BinId, media: MediaId) -> Result<()> {
        if self.media(media).is_none() {
            return Err(EditError::UnknownMedia(media));
        }
        let bin = self.bin_mut(id)?;
        if !bin.media.contains(&media) {
            bin.media.push(media);
        }
        Ok(())
    }

    /// Takes media out of the bin; it stays in the project.
    pub fn remove_from_bin(&mut self, id: BinId, media: MediaId) -> Result<()> {
        self.bin_mut(id)?.media.retain(|&m| m != media);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Generator, MediaInfo};

    fn project_with_media() -> (Project, MediaId) {
        let mut p = Project::default();
        let m = p.add_media(MediaInfo::generated(Generator::ColorBars, &p.settings));
        (p, m)
    }

    #[test]
    fn bins_hold_media_and_bins() {
        let (mut p, m) = project_with_media();
        let shots = p.add_bin(None, "Shots").unwrap();
        let day1 = p.add_bin(Some(shots), "Day 1").unwrap();
        let b_roll = p.add_bin(None, "B-roll").unwrap();
        assert_eq!(
            p.bins_in(None)
                .iter()
                .map(|b| b.name.as_str())
                .collect::<Vec<_>>(),
            ["B-roll", "Shots"]
        );
        assert_eq!(p.bins_in(Some(shots))[0].id, day1);
        p.add_to_bin(day1, m).unwrap();
        p.add_to_bin(day1, m).unwrap();
        p.add_to_bin(b_roll, m).unwrap();
        assert_eq!(p.bin(day1).unwrap().media, [m], "once");
        // Out of a bin: still in the project and in the other bin.
        p.remove_from_bin(day1, m).unwrap();
        assert!(p.media(m).is_some());
        assert_eq!(p.bin(b_roll).unwrap().media, [m]);
        p.rename_bin(b_roll, "Inserts").unwrap();
        assert_eq!(p.bin(b_roll).unwrap().name, "Inserts");
        // A bin goes with the bins in it.
        p.remove_bin(shots).unwrap();
        assert!(p.bin(day1).is_none() && p.bin(shots).is_none());
        assert!(p.bin(b_roll).is_some());
        assert!(p.add_bin(Some(shots), "x").is_err(), "no such bin");
    }

    #[test]
    fn media_removed_from_the_project_leaves_its_bins_and_bins_are_saved() {
        let (mut p, m) = project_with_media();
        let b = p.add_bin(None, "Shots").unwrap();
        p.add_to_bin(b, m).unwrap();
        let json = p.to_json();
        let back = Project::from_json(&json).unwrap();
        assert_eq!(back.bin(b).unwrap().media, [m]);
        p.remove_media(m).unwrap();
        assert!(p.bin(b).unwrap().media.is_empty());
        // Projects without bins stay as they were written.
        let plain = Project::default().to_json();
        assert!(!plain.contains("\"bins\""));
    }
}
