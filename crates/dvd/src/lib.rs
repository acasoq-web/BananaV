//! DVD-Video discs, from the format's public descriptions: a title's video
//! objects (`vob`: MPEG-2 video and linear PCM sound in VOBUs, each
//! opened by a navigation pack) and the disc's information files (`ifo`:
//! the video manager's, playing the title first, and the title set's, its
//! chapters as cells), written into a `VIDEO_TS` folder; and the disc's
//! image (`image`: UDF 1.02 bridged with ISO 9660, the files in the
//! order the information files count them).

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

pub mod ifo;
pub mod image;
pub mod vob;

pub use ifo::{Aspect, chapters, vmg_ifo, vts_ifo};
pub use image::write_image;
pub use vob::{SECTOR, START, Title, VobWriter, Vobu};

/// The largest VOB file: 1 GiB.
pub const VOB_MAX: u64 = 1 << 30;

/// The title's video objects as `dir` holds them (`VTS_01_1.VOB`, written
/// whole) cut into files of at most 1 GiB, and the information files and
/// their backups written beside them.
pub fn finish_folder(dir: &Path, title: &Title, aspect: Aspect) -> io::Result<()> {
    split_vob(dir)?;
    let vts = vts_ifo(title, aspect);
    let vmg = vmg_ifo(chapters(title), &vts[0x100..0x400], title.ntsc, aspect);
    for (name, bytes) in [
        ("VIDEO_TS.IFO", &vmg),
        ("VIDEO_TS.BUP", &vmg),
        ("VTS_01_0.IFO", &vts),
        ("VTS_01_0.BUP", &vts),
    ] {
        std::fs::write(dir.join(name), bytes)?;
    }
    Ok(())
}

/// `VTS_01_1.VOB` cut at every gibibyte into `VTS_01_2.VOB`…
fn split_vob(dir: &Path) -> io::Result<()> {
    let first = dir.join("VTS_01_1.VOB");
    let len = std::fs::metadata(&first)?.len();
    if len <= VOB_MAX {
        return Ok(());
    }
    let mut src = File::open(&first)?;
    let mut k = 2;
    let mut at = VOB_MAX;
    while at < len {
        src.seek(SeekFrom::Start(at))?;
        let mut out = File::create(dir.join(format!("VTS_01_{k}.VOB")))?;
        let n = io::copy(&mut (&mut src).take(VOB_MAX), &mut out)?;
        out.flush()?;
        at += n.max(1);
        k += 1;
    }
    drop(src);
    OpenOptions::new()
        .write(true)
        .open(&first)?
        .set_len(VOB_MAX)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_gets_its_information_files() {
        let dir = std::env::temp_dir().join(format!("bv-dvd-folder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let vob = File::create(dir.join("VTS_01_1.VOB")).unwrap();
        let mut w = VobWriter::new(vob, false, 48_000);
        for n in 0..25u64 {
            w.audio(&[0i16; 2 * 1920]).unwrap();
            w.video(&[1u8; 4000], n * 3600, n * 3600, 3600, n % 12 == 0)
                .unwrap();
        }
        let (_, title) = w.finish().unwrap();
        finish_folder(&dir, &title, Aspect::Standard).unwrap();
        for name in [
            "VIDEO_TS.IFO",
            "VIDEO_TS.BUP",
            "VTS_01_0.IFO",
            "VTS_01_0.BUP",
        ] {
            assert!(dir.join(name).is_file(), "{name}");
        }
        let vob = std::fs::metadata(dir.join("VTS_01_1.VOB")).unwrap().len();
        assert_eq!(vob, title.sectors as u64 * SECTOR as u64);
        assert_eq!(
            std::fs::read(dir.join("VTS_01_0.IFO")).unwrap(),
            std::fs::read(dir.join("VTS_01_0.BUP")).unwrap()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
