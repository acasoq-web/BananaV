//! A DVD-Video disc's information files: the video manager's
//! (`VIDEO_TS.IFO`, with a first-play program chain jumping to the title)
//! and the title set's (`VTS_01_0.IFO`: one title, a program chain whose
//! programs are the chapters, each a cell, its time map, cell and VOBU
//! address tables). Their backups (`.BUP`) are the same bytes.

use crate::vob::{SECTOR, Title, bcd_time};

/// The picture's shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aspect {
    Standard,
    Wide,
}

fn put16(b: &mut [u8], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_be_bytes());
}

fn put32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_be_bytes());
}

/// MPEG-2 video's attributes: the standard (NTSC or PAL), the shape (a
/// wide one letterboxed on 4:3 sets), 720 pixels wide.
fn video_attributes(ntsc: bool, aspect: Aspect) -> [u8; 2] {
    let standard = if ntsc { 0 } else { 0x10 };
    let shape = match aspect {
        Aspect::Standard => 0x00,
        // 16:9, automatic pan and scan not allowed (letterbox allowed).
        Aspect::Wide => 0x0C | 0x02,
    };
    [0x40 | standard | shape, 0x00]
}

/// Linear PCM's attributes: 16-bit, 48 kHz, stereo.
const LPCM_ATTRIBUTES: [u8; 8] = [0x80, 0x01, 0, 0, 0, 0, 0, 0];

/// The sectors a table of `bytes` takes.
fn sectors(bytes: usize) -> u32 {
    bytes.div_ceil(SECTOR).max(1) as u32
}

/// A cell: the chapter its VOBUs make.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cell {
    first: usize,
    last: usize,
}

fn cells(t: &Title) -> Vec<Cell> {
    let mut out: Vec<Cell> = Vec::new();
    for (i, v) in t.vobus.iter().enumerate() {
        match out.last_mut() {
            Some(c) if t.vobus[c.first].cell == v.cell => c.last = i,
            _ => out.push(Cell { first: i, last: i }),
        }
    }
    out
}

/// A cell's or the title's playback time, BCD.
fn duration(t: &Title, first: usize, last: usize) -> u32 {
    bcd_time(t.vobus[last].end - t.vobus[first].start, t.ntsc)
}

/// The title set's IFO for `title` (its VOB sectors start right after
/// it).
pub fn vts_ifo(title: &Title, aspect: Aspect) -> Vec<u8> {
    let cells = cells(title);
    let n = title.vobus.len();
    // The tables, each from a sector of its own.
    let ptt_len = 12 + 4 * cells.len();
    let programs = cells.len();
    let map_len = programs + programs % 2;
    let pgc_len = 0xEC + map_len + 24 * cells.len() + 4 * cells.len();
    let pgcit_len = 16 + pgc_len;
    // The time map: a VOBU every `unit` seconds, at most 2048.
    let total = title
        .vobus
        .last()
        .map_or(0, |v| v.end - title.vobus[0].start);
    let unit = (total / 90_000 / 2048 + 1).max(1);
    let entries = (total / 90_000 / unit) as usize;
    let tmap_len = 12 + 4 + 4 * entries;
    let c_adt_len = 8 + 12 * cells.len();
    let admap_len = 4 + 4 * n;
    let ptt_at = 1;
    let pgcit_at = ptt_at + sectors(ptt_len);
    let tmap_at = pgcit_at + sectors(pgcit_len);
    let c_adt_at = tmap_at + sectors(tmap_len);
    let admap_at = c_adt_at + sectors(c_adt_len);
    let ifo_sectors = admap_at + sectors(admap_len);
    let mut b = vec![0u8; ifo_sectors as usize * SECTOR];

    // VTSI_MAT.
    b[0..12].copy_from_slice(b"DVDVIDEO-VTS");
    put32(&mut b, 0x0C, 2 * ifo_sectors + title.sectors - 1);
    put32(&mut b, 0x1C, ifo_sectors - 1);
    put16(&mut b, 0x20, 0x0011);
    put32(&mut b, 0x80, 0x3FF);
    put32(&mut b, 0xC4, ifo_sectors);
    put32(&mut b, 0xC8, ptt_at);
    put32(&mut b, 0xCC, pgcit_at);
    put32(&mut b, 0xD4, tmap_at);
    put32(&mut b, 0xE0, c_adt_at);
    put32(&mut b, 0xE4, admap_at);
    let video = video_attributes(title.ntsc, aspect);
    b[0x100..0x102].copy_from_slice(&video);
    b[0x200..0x202].copy_from_slice(&video);
    put16(&mut b, 0x202, 1);
    b[0x204..0x20C].copy_from_slice(&LPCM_ATTRIBUTES);

    // VTS_PTT_SRPT: one title, its chapters the program chain's programs.
    let at = ptt_at as usize * SECTOR;
    put16(&mut b, at, 1);
    put32(&mut b, at + 4, ptt_len as u32 - 1);
    put32(&mut b, at + 8, 12);
    for k in 0..cells.len() {
        put16(&mut b, at + 12 + 4 * k, 1);
        put16(&mut b, at + 14 + 4 * k, k as u16 + 1);
    }

    // VTS_PGCIT: the title's program chain.
    let at = pgcit_at as usize * SECTOR;
    put16(&mut b, at, 1);
    put32(&mut b, at + 4, pgcit_len as u32 - 1);
    b[at + 8] = 0x81;
    put32(&mut b, at + 12, 16);
    let pgc = at + 16;
    b[pgc + 2] = programs as u8;
    b[pgc + 3] = cells.len() as u8;
    put32(&mut b, pgc + 4, duration(title, 0, n - 1));
    // Audio stream 0 available.
    put16(&mut b, pgc + 0x0C, 0x8000);
    let map_at = 0xEC;
    let play_at = map_at + map_len;
    let pos_at = play_at + 24 * cells.len();
    put16(&mut b, pgc + 0xE6, map_at as u16);
    put16(&mut b, pgc + 0xE8, play_at as u16);
    put16(&mut b, pgc + 0xEA, pos_at as u16);
    for (k, c) in cells.iter().enumerate() {
        b[pgc + map_at + k] = k as u8 + 1;
        let cp = pgc + play_at + 24 * k;
        put32(&mut b, cp + 4, duration(title, c.first, c.last));
        let (first, last) = (&title.vobus[c.first], &title.vobus[c.last]);
        put32(&mut b, cp + 8, first.sector);
        put32(&mut b, cp + 16, last.sector);
        put32(&mut b, cp + 20, last.sector + last.sectors - 1);
        let ps = pgc + pos_at + 4 * k;
        put16(&mut b, ps, 1);
        b[ps + 3] = k as u8 + 1;
    }

    // VTS_TMAPTI: the VOBU at each `unit` seconds.
    let at = tmap_at as usize * SECTOR;
    put16(&mut b, at, 1);
    put32(&mut b, at + 4, tmap_len as u32 - 1);
    put32(&mut b, at + 8, 12);
    b[at + 12] = unit as u8;
    put16(&mut b, at + 14, entries as u16);
    for e in 0..entries {
        let time = title.vobus[0].start + (e as u64 + 1) * unit * 90_000;
        let v = title
            .vobus
            .iter()
            .rev()
            .find(|v| v.start <= time)
            .map_or(0, |v| v.sector);
        put32(&mut b, at + 16 + 4 * e, v);
    }

    // VTS_C_ADT: each cell's sectors.
    let at = c_adt_at as usize * SECTOR;
    put16(&mut b, at, 1);
    put32(&mut b, at + 4, c_adt_len as u32 - 1);
    for (k, c) in cells.iter().enumerate() {
        let e = at + 8 + 12 * k;
        put16(&mut b, e, 1);
        b[e + 2] = k as u8 + 1;
        let last = &title.vobus[c.last];
        put32(&mut b, e + 4, title.vobus[c.first].sector);
        put32(&mut b, e + 8, last.sector + last.sectors - 1);
    }

    // VTS_VOBU_ADMAP: every VOBU's first sector.
    let at = admap_at as usize * SECTOR;
    put32(&mut b, at, admap_len as u32 - 1);
    for (k, v) in title.vobus.iter().enumerate() {
        put32(&mut b, at + 4 + 4 * k, v.sector);
    }
    b
}

/// The video manager's IFO for one title set of `chapters` chapters
/// whose IFO is `vts_ifo` bytes, its attributes `attributes` (the title
/// set IFO's 0x100–0x3FF).
pub fn vmg_ifo(chapters: u16, vts_attributes: &[u8], ntsc: bool, aspect: Aspect) -> Vec<u8> {
    const SECTORS: u32 = 3;
    let mut b = vec![0u8; SECTORS as usize * SECTOR];
    b[0..12].copy_from_slice(b"DVDVIDEO-VMG");
    // No menus: the IFO and its backup.
    put32(&mut b, 0x0C, 2 * SECTORS - 1);
    put32(&mut b, 0x1C, SECTORS - 1);
    put16(&mut b, 0x20, 0x0011);
    put16(&mut b, 0x26, 1);
    put16(&mut b, 0x28, 1);
    b[0x2A] = 1;
    put16(&mut b, 0x3E, 1);
    b[0x40..0x47].copy_from_slice(b"BananaV");
    put32(&mut b, 0x80, 0x3FF);
    put32(&mut b, 0x84, 0x400);
    put32(&mut b, 0xC4, 1);
    put32(&mut b, 0xD0, 2);
    b[0x100..0x102].copy_from_slice(&video_attributes(ntsc, aspect));
    // The first-play program chain: its pre-command jumps to title 1.
    let fp = 0x400;
    put16(&mut b, fp + 0xE4, 0xEC);
    let cmd = fp + 0xEC;
    put16(&mut b, cmd, 1);
    put16(&mut b, cmd + 6, 8 + 8 - 1);
    b[cmd + 8..cmd + 16].copy_from_slice(&[0x30, 0x02, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00]);

    // TT_SRPT: the title.
    let at = SECTOR;
    put16(&mut b, at, 1);
    put32(&mut b, at + 4, 8 + 12 - 1);
    b[at + 8] = 0x3C;
    b[at + 9] = 1;
    put16(&mut b, at + 10, chapters);
    b[at + 14] = 1;
    b[at + 15] = 1;
    // The title set starts after the manager's IFO and its backup.
    put32(&mut b, at + 16, 2 * SECTORS);

    // VTS_ATRT: the title set's attributes.
    let at = 2 * SECTOR;
    let atr_len = 8 + 0x300;
    put16(&mut b, at, 1);
    put32(&mut b, at + 4, (12 + atr_len) as u32 - 1);
    put32(&mut b, at + 8, 12);
    put32(&mut b, at + 12, atr_len as u32 - 1);
    let take = vts_attributes.len().min(0x300);
    b[at + 20..at + 20 + take].copy_from_slice(&vts_attributes[..take]);
    b
}

/// The number of chapters (cells) of a title.
pub fn chapters(title: &Title) -> u16 {
    cells(title).len() as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vob::Vobu;

    fn title() -> Title {
        let v = |sector, sectors, start: u64, cell| Vobu {
            sector,
            sectors,
            start,
            end: start + 45_045,
            first_ref_end: 3,
            cell,
            scr: 0,
        };
        Title {
            vobus: vec![
                v(0, 10, 45_000, 1),
                v(10, 12, 90_045, 1),
                v(22, 8, 135_090, 2),
            ],
            sectors: 30,
            ntsc: true,
        }
    }

    fn get32(b: &[u8], at: usize) -> u32 {
        u32::from_be_bytes(b[at..at + 4].try_into().unwrap())
    }

    #[test]
    fn the_title_set_ifo_points_at_its_tables_and_cells() {
        let t = title();
        let ifo = vts_ifo(&t, Aspect::Wide);
        assert_eq!(&ifo[0..12], b"DVDVIDEO-VTS");
        let ifo_sectors = (ifo.len() / SECTOR) as u32;
        assert_eq!(get32(&ifo, 0x1C), ifo_sectors - 1);
        assert_eq!(get32(&ifo, 0xC4), ifo_sectors);
        assert_eq!(get32(&ifo, 0x0C), 2 * ifo_sectors + 30 - 1);
        // The cell address table: two cells, the second from sector 22.
        let c_adt = get32(&ifo, 0xE0) as usize * SECTOR;
        assert_eq!(get32(&ifo, c_adt + 8 + 12 + 4), 22);
        assert_eq!(get32(&ifo, c_adt + 8 + 12 + 8), 29);
        // The VOBU map: every VOBU.
        let admap = get32(&ifo, 0xE4) as usize * SECTOR;
        assert_eq!(get32(&ifo, admap), 4 + 4 * 3 - 1);
        assert_eq!(get32(&ifo, admap + 8), 10);
        // The program chain: two programs, two cells.
        let pgc = get32(&ifo, 0xCC) as usize * SECTOR + 16;
        assert_eq!((ifo[pgc + 2], ifo[pgc + 3]), (2, 2));
        assert_eq!(ifo[0x200], 0x4E, "MPEG-2 NTSC 16:9");
        assert_eq!(chapters(&t), 2);
    }

    #[test]
    fn the_video_manager_plays_the_title_first() {
        let t = title();
        let vts = vts_ifo(&t, Aspect::Standard);
        let vmg = vmg_ifo(chapters(&t), &vts[0x100..0x400], true, Aspect::Standard);
        assert_eq!(&vmg[0..12], b"DVDVIDEO-VMG");
        assert_eq!(&vmg[0x400 + 0xEC + 8..0x400 + 0xEC + 10], [0x30, 0x02]);
        assert_eq!(u16::from_be_bytes([vmg[SECTOR + 10], vmg[SECTOR + 11]]), 2);
        assert_eq!(get32(&vmg, SECTOR + 16), 6);
        // The title set's attributes copied.
        assert_eq!(vmg[2 * SECTOR + 20 + 0x100], vts[0x200]);
    }
}
