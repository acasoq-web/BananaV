//! A title's video objects (`VTS_01_1.VOB`…): an MPEG-2 program stream of
//! 2048-byte packs cut into VOBUs — each a group of pictures, starting
//! with a navigation pack (its PCI and DSI) — with the sound as linear
//! PCM. The navigation packs are filled in once every VOBU's place is
//! known (`finish`).

use std::io::{self, Seek, SeekFrom, Write};

/// A sector: a pack.
pub const SECTOR: usize = 2048;

/// The mux rate: DVD's 10.08 Mbit/s, in units of 50 bytes a second.
const MUX_RATE: u32 = 25_200;

/// Where the first frame shows (90 kHz).
pub const START: u64 = 45_000;

/// Linear PCM's frames: 1/600 s (80 samples at 48 kHz).
const LPCM_FRAME: usize = 80;

/// A VOBU as written: where it starts (sectors from the title's first
/// VOB sector), how long it is, when it shows and its cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Vobu {
    pub sector: u32,
    pub sectors: u32,
    /// Its first and last-plus-one frame's times (90 kHz).
    pub start: u64,
    pub end: u64,
    /// Where its first picture (the I-frame) ends, in sectors after the
    /// navigation pack.
    pub first_ref_end: u32,
    /// Its cell (from 1).
    pub cell: u8,
    /// The SCR of its navigation pack (90 kHz).
    pub scr: u64,
}

/// What a title's video objects hold, for its IFO.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Title {
    pub vobus: Vec<Vobu>,
    /// Sectors written.
    pub sectors: u32,
    /// The frame rate: NTSC's (30000/1001) or PAL's (25).
    pub ntsc: bool,
}

/// The video objects being written.
#[derive(Debug)]
pub struct VobWriter<W: Write + Seek> {
    out: W,
    ntsc: bool,
    sector: u32,
    /// The system clock (27 MHz).
    scr: u64,
    vobus: Vec<Vobu>,
    /// The cell the next VOBU starts.
    cell: u8,
    new_cell: bool,
    /// Sound not written yet (big-endian 16-bit stereo samples), and the
    /// first one's time.
    pcm: Vec<u8>,
    pcm_samples: u64,
    rate: u32,
    /// How long after the title's start the sound starts (90 kHz).
    sound_delay: u64,
}

impl<W: Write + Seek> VobWriter<W> {
    /// Video objects of NTSC (else PAL) frames, the sound at `rate` Hz.
    pub fn new(out: W, ntsc: bool, rate: u32) -> VobWriter<W> {
        VobWriter {
            out,
            ntsc,
            sector: 0,
            scr: 0,
            vobus: Vec::new(),
            cell: 0,
            new_cell: true,
            pcm: Vec::new(),
            pcm_samples: 0,
            rate,
            sound_delay: 0,
        }
    }

    /// The sound starts `ticks` (90 kHz) after the title's start: as late
    /// as its first picture shows (B-pictures' reordering).
    pub fn delay_sound(&mut self, ticks: u64) {
        self.sound_delay = ticks;
    }

    /// The next VOBU starts a new cell (a chapter).
    pub fn chapter(&mut self) {
        self.new_cell = true;
    }

    /// A coded picture decoded at `dts` and shown at `pts` for `duration`
    /// (90 kHz from the title's start; in the order they are decoded); a
    /// keyframe begins a VOBU. The first picture must be one.
    pub fn video(
        &mut self,
        data: &[u8],
        pts: u64,
        dts: u64,
        duration: u64,
        keyframe: bool,
    ) -> io::Result<()> {
        let (pts, dts) = (pts + START, dts + START);
        if keyframe || self.vobus.is_empty() {
            self.flush_audio(dts)?;
            if self.new_cell || self.vobus.is_empty() {
                self.cell += 1;
                self.new_cell = false;
            }
            let scr = self.next_scr(dts);
            self.vobus.push(Vobu {
                sector: self.sector,
                sectors: 0,
                start: pts,
                end: pts,
                first_ref_end: 0,
                cell: self.cell,
                scr: scr / 300,
            });
            // Filled in by `finish`.
            self.write_pack(scr, &[0u8; SECTOR - 14])?;
        }
        self.write_pes(0xE0, None, data, Some((pts, dts)))?;
        let last = self.vobus.last_mut().expect("a VOBU");
        if keyframe {
            last.first_ref_end = self.sector - last.sector - 1;
        }
        // Its last picture to show (B-pictures come after anchors shown
        // later).
        last.end = last.end.max(pts + duration);
        // Sound written as the pictures go.
        self.flush_audio(dts + duration)?;
        Ok(())
    }

    /// Stereo samples (interleaved) that follow the ones given before.
    pub fn audio(&mut self, samples: &[i16]) -> io::Result<()> {
        for s in samples {
            self.pcm.extend_from_slice(&s.to_be_bytes());
        }
        Ok(())
    }

    /// The sound written up to `until` (90 kHz), in packs of whole
    /// LPCM frames.
    fn flush_audio(&mut self, until: u64) -> io::Result<()> {
        // Up to six frames a pack (1920 bytes).
        let frame_bytes = LPCM_FRAME * 4;
        loop {
            let time = START + self.sound_delay + self.pcm_samples * 90_000 / self.rate as u64;
            if time >= until || self.pcm.len() < frame_bytes {
                return Ok(());
            }
            let n = (self.pcm.len() / frame_bytes).min(6) * frame_bytes;
            let chunk: Vec<u8> = self.pcm.drain(..n).collect();
            let frames = (chunk.len() / frame_bytes) as u8;
            // LPCM's header: the frames that start here, where the first
            // one does, then no emphasis, 16-bit 48 kHz stereo, no range
            // control.
            let header = [0xA0, frames, 0x00, 0x04, 0x00, 0x01, 0x80];
            self.write_pes(0xBD, Some(&header), &chunk, Some((time, time)))?;
            self.pcm_samples += (chunk.len() / 4) as u64;
        }
    }

    /// The system clock of the next pack: the mux rate's pace, no sooner
    /// than half a second before `decode` (90 kHz).
    fn next_scr(&mut self, decode: u64) -> u64 {
        let step = SECTOR as u64 * 27_000_000 / (MUX_RATE as u64 * 50);
        let soonest = decode.saturating_sub(45_000) * 300;
        let scr = if self.sector == 0 {
            0
        } else {
            (self.scr + step).max(soonest)
        };
        self.scr = scr;
        scr
    }

    /// A pack: its header (and a navigation pack's system header) and
    /// `body`.
    fn write_pack(&mut self, scr: u64, body: &[u8]) -> io::Result<()> {
        let mut pack = Vec::with_capacity(SECTOR);
        pack_header(&mut pack, scr);
        pack.extend_from_slice(body);
        debug_assert_eq!(pack.len(), SECTOR);
        self.out.write_all(&pack)?;
        self.sector += 1;
        Ok(())
    }

    /// `data` of stream `id` (with a substream header `sub`) in packs, the
    /// first with its times (shown, decoded).
    fn write_pes(
        &mut self,
        id: u8,
        sub: Option<&[u8]>,
        data: &[u8],
        times: Option<(u64, u64)>,
    ) -> io::Result<()> {
        let sub = sub.unwrap_or(&[]);
        let mut rest = data;
        let mut first = true;
        let decode = times.map_or(START, |(_, dts)| dts);
        while first || !rest.is_empty() {
            let stamp = if first { times } else { None };
            let ts_len = match stamp {
                Some((pts, dts)) if pts != dts => 10,
                Some(_) => 5,
                None => 0,
            };
            let room = SECTOR - 14 - 9 - ts_len - sub.len();
            let take = rest.len().min(room);
            let left = room - take;
            let (stuffing, padding) = if left >= 6 { (0, left) } else { (left, 0) };
            let mut body = Vec::with_capacity(SECTOR - 14);
            body.extend_from_slice(&[0, 0, 1, id]);
            body.extend_from_slice(
                &((3 + ts_len + stuffing + sub.len() + take) as u16).to_be_bytes(),
            );
            body.push(if first && id == 0xE0 { 0x84 } else { 0x80 });
            body.push(match ts_len {
                10 => 0xC0,
                5 => 0x80,
                _ => 0,
            });
            body.push((ts_len + stuffing) as u8);
            match stamp {
                Some((pts, dts)) if pts != dts => {
                    timestamp(&mut body, 0x3, pts);
                    timestamp(&mut body, 0x1, dts);
                }
                Some((pts, _)) => timestamp(&mut body, 0x2, pts),
                None => {}
            }
            body.extend(std::iter::repeat_n(0xFF, stuffing));
            body.extend_from_slice(sub);
            body.extend_from_slice(&rest[..take]);
            if padding > 0 {
                body.extend_from_slice(&[0, 0, 1, 0xBE]);
                body.extend_from_slice(&((padding - 6) as u16).to_be_bytes());
                body.extend(std::iter::repeat_n(0xFF, padding - 6));
            }
            let scr = self.next_scr(decode);
            self.write_pack(scr, &body)?;
            rest = &rest[take..];
            first = false;
        }
        Ok(())
    }

    /// The last sound, then every navigation pack filled in; what was
    /// written, for the IFO.
    pub fn finish(mut self) -> io::Result<(W, Title)> {
        let end = self
            .vobus
            .last()
            .map_or(START, |v| v.end)
            .saturating_add(90_000);
        // Sound past the last frame is dropped; whole frames before it go.
        self.flush_audio(end)?;
        let total = self.sector;
        let mut vobus = self.vobus.clone();
        for i in 0..vobus.len() {
            let next = vobus.get(i + 1).map_or(total, |v| v.sector);
            vobus[i].sectors = next - vobus[i].sector;
        }
        for (i, v) in vobus.iter().enumerate() {
            let nav = nav_pack(&vobus, i, self.ntsc);
            self.out
                .seek(SeekFrom::Start(v.sector as u64 * SECTOR as u64 + 14))?;
            self.out.write_all(&nav)?;
        }
        self.out
            .seek(SeekFrom::Start(total as u64 * SECTOR as u64))?;
        self.out.flush()?;
        Ok((
            self.out,
            Title {
                vobus,
                sectors: total,
                ntsc: self.ntsc,
            },
        ))
    }
}

/// An MPEG-2 pack header at the mux rate.
fn pack_header(out: &mut Vec<u8>, scr: u64) {
    let (base, ext) = (scr / 300, scr % 300);
    out.extend_from_slice(&[0, 0, 1, 0xBA]);
    out.push(0x44 | ((base >> 27) as u8 & 0x38) | ((base >> 28) as u8 & 0x03));
    out.push((base >> 20) as u8);
    out.push(((base >> 12) as u8 & 0xF8) | 0x04 | ((base >> 13) as u8 & 0x03));
    out.push((base >> 5) as u8);
    out.push(((base << 3) as u8 & 0xF8) | 0x04 | ((ext >> 7) as u8 & 0x03));
    out.push(((ext << 1) as u8 & 0xFE) | 0x01);
    out.push((MUX_RATE >> 14) as u8);
    out.push((MUX_RATE >> 6) as u8);
    out.push(((MUX_RATE << 2) as u8 & 0xFC) | 0x03);
    out.push(0xF8);
}

/// A PES timestamp: its prefix and 33 bits with marker bits.
fn timestamp(out: &mut Vec<u8>, prefix: u8, t: u64) {
    out.push((prefix << 4) | ((t >> 29) as u8 & 0x0E) | 0x01);
    out.push((t >> 22) as u8);
    out.push(((t >> 14) as u8 & 0xFE) | 0x01);
    out.push((t >> 7) as u8);
    out.push(((t << 1) as u8 & 0xFE) | 0x01);
}

/// A time as DVD's BCD: hours, minutes, seconds and frames, the frame
/// rate in the frames' top bits (11: 30, 01: 25).
pub fn bcd_time(ticks: u64, ntsc: bool) -> u32 {
    let bcd = |v: u64| (((v / 10) << 4) | (v % 10)) as u32;
    let fps = if ntsc { 30 } else { 25 };
    let secs = ticks / 90_000;
    let frames = (ticks % 90_000) * fps / 90_000;
    let rate = if ntsc { 0xC0 } else { 0x40 };
    (bcd(secs / 3600 % 100) << 24)
        | (bcd(secs / 60 % 60) << 16)
        | (bcd(secs % 60) << 8)
        | (bcd(frames) | rate)
}

/// The VOBU search information's offset to the VOBU `to` from `from`
/// (sectors; the top bit: it holds video), or "none".
fn sri(vobus: &[Vobu], from: usize, to: Option<usize>) -> u32 {
    match to {
        Some(t) if t != from => {
            let d = vobus[t].sector.abs_diff(vobus[from].sector);
            0x8000_0000 | d
        }
        _ => 0x3FFF_FFFF,
    }
}

/// The VOBU of the same cell that starts `secs` (half-second steps:
/// `halves`) after (or before, when negative) VOBU `i`.
fn vobu_at(vobus: &[Vobu], i: usize, halves: i64) -> Option<usize> {
    let cell = vobus[i].cell;
    let target = vobus[i].start as i64 + halves * 45_000;
    let same: Vec<usize> = (0..vobus.len())
        .filter(|&k| vobus[k].cell == cell)
        .collect();
    if halves > 0 {
        same.into_iter()
            .filter(|&k| k > i && vobus[k].start as i64 >= target)
            .min_by_key(|&k| vobus[k].start)
    } else {
        same.into_iter()
            .filter(|&k| k < i && vobus[k].start as i64 <= target)
            .max_by_key(|&k| vobus[k].start)
    }
}

/// VOBU `i`'s navigation pack after its pack header: the system header,
/// the PCI and the DSI.
fn nav_pack(vobus: &[Vobu], i: usize, ntsc: bool) -> Vec<u8> {
    let v = &vobus[i];
    let mut out = Vec::with_capacity(SECTOR - 14);
    // The system header: DVD's streams and buffers.
    out.extend_from_slice(&[0, 0, 1, 0xBB, 0x00, 0x12]);
    out.extend_from_slice(&[0x80, 0xC4, 0xE1, 0x04, 0xE1, 0xFF]);
    out.extend_from_slice(&[
        0xB9, 0xE0, 0xE8, 0xB8, 0xC0, 0x20, 0xBD, 0xE0, 0x3A, 0xBF, 0xE0, 0x02,
    ]);
    // The cell's elapsed time at this VOBU.
    let cell_start = vobus
        .iter()
        .find(|o| o.cell == v.cell)
        .map_or(v.start, |o| o.start);
    let elapsed = bcd_time(v.start - cell_start, ntsc);
    // PCI.
    let mut pci = vec![0u8; 980];
    pci[0] = 0x00;
    let p = &mut pci[1..];
    p[0..4].copy_from_slice(&v.sector.to_be_bytes());
    p[0x0C..0x10].copy_from_slice(&(v.start as u32).to_be_bytes());
    p[0x10..0x14].copy_from_slice(&(v.end as u32).to_be_bytes());
    p[0x18..0x1C].copy_from_slice(&elapsed.to_be_bytes());
    out.extend_from_slice(&[0, 0, 1, 0xBF, 0x03, 0xD4]);
    out.extend_from_slice(&pci);
    // DSI.
    let mut dsi = vec![0u8; 1018];
    dsi[0] = 0x01;
    let d = &mut dsi[1..];
    d[0..4].copy_from_slice(&(v.scr as u32).to_be_bytes());
    d[4..8].copy_from_slice(&v.sector.to_be_bytes());
    d[8..12].copy_from_slice(&(v.sectors.saturating_sub(1)).to_be_bytes());
    d[12..16].copy_from_slice(&v.first_ref_end.to_be_bytes());
    d[0x18..0x1A].copy_from_slice(&1u16.to_be_bytes());
    d[0x1B] = v.cell;
    d[0x1C..0x20].copy_from_slice(&elapsed.to_be_bytes());
    // The VOBU search information: the next VOBU with video, those 120,
    // 60, 30, 10, 7.5 … 0.5 seconds ahead, the next and the previous,
    // those behind, and the previous one with video.
    let ahead = [
        240i64, 120, 60, 20, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1,
    ];
    let next = (i + 1 < vobus.len() && vobus[i + 1].cell == v.cell).then_some(i + 1);
    let prev = i.checked_sub(1).filter(|&p| vobus[p].cell == v.cell);
    let mut entries = vec![sri(vobus, i, next)];
    entries.extend(ahead.iter().map(|&h| sri(vobus, i, vobu_at(vobus, i, h))));
    entries.push(sri(vobus, i, next));
    entries.push(sri(vobus, i, prev));
    entries.extend(
        ahead
            .iter()
            .rev()
            .map(|&h| sri(vobus, i, vobu_at(vobus, i, -h))),
    );
    entries.push(sri(vobus, i, prev));
    for (k, e) in entries.iter().enumerate() {
        let at = 0xEA + 4 * k;
        d[at..at + 4].copy_from_slice(&e.to_be_bytes());
    }
    out.extend_from_slice(&[0, 0, 1, 0xBF, 0x03, 0xFA]);
    out.extend_from_slice(&dsi);
    debug_assert_eq!(out.len(), SECTOR - 14);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn vobus_start_with_navigation_packs_that_know_their_places() {
        let mut w = VobWriter::new(Cursor::new(Vec::new()), true, 48_000);
        let frame = 3003;
        for n in 0..30u64 {
            let key = n % 15 == 0;
            if n == 15 {
                w.chapter();
            }
            w.audio(&vec![0i16; 2 * 1601]).unwrap();
            w.video(&vec![n as u8; 3000], n * frame, n * frame, frame, key)
                .unwrap();
        }
        let (out, title) = w.finish().unwrap();
        let bytes = out.into_inner();
        assert_eq!(bytes.len(), title.sectors as usize * SECTOR);
        assert_eq!(title.vobus.len(), 2);
        let (a, b) = (title.vobus[0], title.vobus[1]);
        assert_eq!((a.cell, b.cell), (1, 2));
        assert_eq!(a.sectors, b.sector);
        // Each navigation pack: its sector, the VOBU's end.
        for v in &title.vobus {
            let nav = &bytes[v.sector as usize * SECTOR..][..SECTOR];
            assert_eq!(&nav[14..18], [0, 0, 1, 0xBB]);
            let pci = &nav[14 + 24 + 7..];
            assert_eq!(u32::from_be_bytes(pci[0..4].try_into().unwrap()), v.sector);
            let dsi = &nav[14 + 24 + 6 + 980 + 7..];
            let ea = u32::from_be_bytes(dsi[8..12].try_into().unwrap());
            assert_eq!(ea, v.sectors - 1);
        }
        // Every pack a pack.
        assert!(bytes.chunks(SECTOR).all(|p| p[..4] == [0, 0, 1, 0xBA]));
        // The sound went in too.
        let lpcm = bytes
            .chunks(SECTOR)
            .filter(|p| p[14..18] == [0, 0, 1, 0xBD])
            .count();
        assert!(lpcm > 10, "{lpcm}");
    }

    #[test]
    fn times_are_bcd_with_their_rate() {
        assert_eq!(bcd_time(90_000 * 3725 + 45_000, true), 0x0102_0515 | 0xC0);
        assert_eq!(bcd_time(0, false), 0x40);
    }
}
