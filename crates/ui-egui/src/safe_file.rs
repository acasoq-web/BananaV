//! Files that hold someone's work, written whole or not at all.
//!
//! `std::fs::write` empties the file before it writes: a full disk, a
//! drive unplugged or BananaV closed half-way leaves it empty or cut
//! short — the project gone at the moment of saving it, the settings back
//! to their defaults at the next start. These write beside the file first
//! and put the result in its place in one step, so the file holds either
//! what it held or all of the new contents.

use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// `path` with `.{ext}` after its whole name (`Film.bvproj.part`), in the
/// same folder — the same disk, so that a rename puts it in place.
fn beside(path: &Path, ext: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(ext);
    path.with_file_name(name)
}

/// Where the contents `path` had before the last [`save`] are kept.
pub(crate) fn backup_of(path: &Path) -> PathBuf {
    beside(path, "bak")
}

/// Writes a settings file: whole or not at all. Not waited for on the
/// disk (settings are written often, from the interface's thread): safe
/// from BananaV being closed half-way, which is what happens to them.
pub(crate) fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    replace(path, |f| f.write_all(bytes), false, false)
}

/// Saves a project (or another file of someone's work): whole or not at
/// all, on the disk before it takes the file's place, and what the file
/// held before kept beside it as `<name>.bak` — the project as it was
/// saved the time before.
pub(crate) fn save(path: &Path, bytes: &[u8]) -> io::Result<()> {
    replace(path, |f| f.write_all(bytes), true, true)
}

/// The settings kept in `path` (`None` when there is no file), and where
/// a file that could not be read went: one cut short, or written by a
/// newer BananaV with something this one does not know, is moved aside as
/// `<name>.damaged` — the defaults saved next would otherwise replace for
/// good what can still be mended or read by the BananaV that wrote it.
pub(crate) fn load_json<T: serde::de::DeserializeOwned>(
    path: &Path,
) -> (Option<T>, Option<PathBuf>) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return (None, None);
    };
    match serde_json::from_str(&text) {
        Ok(kept) => (Some(kept), None),
        Err(_) => {
            let aside = beside(path, "damaged");
            let _ = std::fs::remove_file(&aside);
            let moved = std::fs::rename(path, &aside).is_ok();
            (None, moved.then_some(aside))
        }
    }
}

/// `fill` writes the new contents into a file beside `path`, which then
/// takes its place. With `durable`, the contents are on the disk first;
/// with `keep`, the old contents stay as [`backup_of`] `path`.
fn replace(
    path: &Path,
    fill: impl FnOnce(&mut File) -> io::Result<()>,
    durable: bool,
    keep: bool,
) -> io::Result<()> {
    let part = beside(path, "part");
    let mut file = match File::create(&part) {
        Ok(f) => f,
        // A folder that lets its files be changed but no file be made:
        // written over, as there is no other way.
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied && path.is_file() => {
            return fill(&mut File::create(path)?);
        }
        Err(e) => return Err(e),
    };
    let written = fill(&mut file).and_then(|()| {
        if durable {
            file.sync_all()
        } else {
            file.flush()
        }
    });
    drop(file);
    if let Err(e) = written {
        // The file itself was never touched.
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    if keep && path.is_file() {
        // A copy, not a rename: the file is never missing. Without room
        // for it the save goes on.
        let _ = std::fs::copy(path, backup_of(path));
    }
    match std::fs::rename(&part, path) {
        Ok(()) => Ok(()),
        // Another program holds the file open in a way that keeps it from
        // being replaced (some backup and antivirus tools): its contents
        // are complete beside it, copied over it instead.
        Err(_) => {
            let copied = std::fs::copy(&part, path).map(|_| ());
            let _ = std::fs::remove_file(&part);
            copied
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("bananav-safe-file-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a folder");
        dir
    }

    #[test]
    fn a_file_is_written_and_then_replaced_whole() {
        let dir = folder("whole");
        let path = dir.join("Film.bvproj");
        write(&path, b"one").expect("written");
        assert_eq!(std::fs::read(&path).expect("read"), b"one");
        write(&path, b"two, longer").expect("written");
        assert_eq!(std::fs::read(&path).expect("read"), b"two, longer");
        // Nothing left beside it, and settings keep no older copy.
        assert!(!beside(&path, "part").exists());
        assert!(!backup_of(&path).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_save_keeps_what_the_file_held_before() {
        let dir = folder("keeps");
        let path = dir.join("Film.bvproj");
        save(&path, b"first").expect("saved");
        // A new file has nothing to keep.
        assert!(!backup_of(&path).exists());
        save(&path, b"second").expect("saved");
        assert_eq!(std::fs::read(&path).expect("read"), b"second");
        assert_eq!(backup_of(&path), dir.join("Film.bvproj.bak"));
        assert_eq!(std::fs::read(backup_of(&path)).expect("read"), b"first");
        save(&path, b"third").expect("saved");
        assert_eq!(std::fs::read(backup_of(&path)).expect("read"), b"second");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_write_that_fails_half_way_leaves_the_file_as_it_was() {
        let dir = folder("fails");
        let path = dir.join("Film.bvproj");
        save(&path, b"the project as it was saved").expect("saved");
        // The disk fills up after the first bytes.
        let failed = replace(
            &path,
            |f| {
                f.write_all(b"half of the n")?;
                Err(io::Error::other("no room left"))
            },
            true,
            true,
        );
        assert!(failed.is_err());
        assert_eq!(
            std::fs::read(&path).expect("read"),
            b"the project as it was saved"
        );
        assert!(!beside(&path, "part").exists(), "nothing left beside it");
        // No copy was made of a save that never happened.
        assert!(!backup_of(&path).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_that_do_not_parse_are_set_aside_not_lost() {
        let dir = folder("damaged");
        let path = dir.join("interface.json");
        // No file: nothing kept, nothing set aside.
        let (kept, aside) = load_json::<Vec<u32>>(&path);
        assert!(kept.is_none() && aside.is_none());
        write(&path, b"[1, 2, 3]").expect("written");
        let (kept, aside) = load_json::<Vec<u32>>(&path);
        assert_eq!(kept, Some(vec![1, 2, 3]));
        assert!(aside.is_none());
        // Cut short: the defaults are used, and the file is still there to
        // be mended, under another name.
        write(&path, b"[1, 2").expect("written");
        let (kept, aside) = load_json::<Vec<u32>>(&path);
        assert!(kept.is_none());
        let aside = aside.expect("set aside");
        assert_eq!(aside, dir.join("interface.json.damaged"));
        assert_eq!(std::fs::read(&aside).expect("read"), b"[1, 2");
        assert!(!path.exists(), "the next save starts a new file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_that_is_not_there_is_an_error() {
        let dir = folder("missing");
        let path = dir.join("none").join("Film.bvproj");
        assert!(save(&path, b"x").is_err());
        assert!(write(&path, b"x").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
