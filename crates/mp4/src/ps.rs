//! MPEG-2 program streams (ISO/IEC 13818-1): an MPEG-2 video stream and an
//! MPEG audio stream in packs of 2048 bytes — a DVD's sector — each a pack
//! header and one PES packet (padded to fill it), the first with the
//! system header; the system clock follows the data, at most half a second
//! ahead of when it is decoded. MPEG-1 system streams (ISO/IEC 11172-1,
//! `PsWriter::mpeg1`) are alike, their pack and packet headers MPEG-1's
//! and their packs Video CD's 2324 bytes.

use std::io::{self, Write};

/// A pack's size: a DVD's sector.
pub const PACK: usize = 2048;

/// The video stream's id, and the first MPEG audio stream's.
pub const VIDEO_ID: u8 = 0xE0;
pub const AUDIO_ID: u8 = 0xC0;
/// Private stream 1: linear PCM's (substream 0xA0, as DVD's).
pub const PRIVATE_ID: u8 = 0xBD;
const PADDING_ID: u8 = 0xBE;

/// The pack header's size (MPEG-2, no stuffing).
const PACK_HEADER: usize = 14;

/// Linear PCM's bytes a pack: whole stereo samples that fit after the
/// headers (pack 14, PES 14 with its time, substream 7).
const PCM_PACK: usize = (PACK - PACK_HEADER - 14 - 7) / 4 * 4;

/// How far ahead of its decoding a pack may arrive (90 kHz).
const AHEAD: u64 = 45_000;

/// Where the first frame shows (90 kHz): room for the first packs.
pub const START: u64 = 45_000;

/// A program stream being written.
#[derive(Debug)]
pub struct PsWriter<W: Write> {
    out: W,
    /// The mux rate, in units of 50 bytes a second.
    rate: u32,
    audio: bool,
    /// The audio is linear PCM at this rate (16-bit stereo, big-endian).
    pcm: Option<u32>,
    packs: u64,
    /// The last pack's system clock (27 MHz).
    scr: u64,
    /// MPEG audio frames waiting to fill a pack, and the first one's time.
    pending: Vec<u8>,
    pending_pts: Option<u64>,
    written: u64,
    /// An MPEG-1 system stream, and its packs' size.
    mpeg1: bool,
    pack: usize,
}

impl<W: Write> PsWriter<W> {
    /// A stream carrying `bitrate` bits a second at most (video and audio
    /// together; the packs' overhead is added), with an audio stream when
    /// `audio`.
    pub fn new(out: W, bitrate: u32, audio: bool) -> PsWriter<W> {
        let bytes = bitrate as u64 / 8 * 105 / 100;
        PsWriter {
            out,
            rate: (bytes.div_ceil(50) as u32).clamp(1, (1 << 22) - 1),
            audio,
            pcm: None,
            packs: 0,
            scr: 0,
            pending: Vec::new(),
            pending_pts: None,
            written: 0,
            mpeg1: false,
            pack: PACK,
        }
    }

    /// An MPEG-1 system stream like [`new`](Self::new)'s, in packs of
    /// `pack` bytes (Video CD's 2324).
    pub fn mpeg1(out: W, bitrate: u32, audio: bool, pack: usize) -> PsWriter<W> {
        let mut w = PsWriter::new(out, bitrate, audio);
        w.mpeg1 = true;
        w.pack = pack.max(256);
        w
    }

    /// The pack header's size.
    fn header_len(&self) -> usize {
        if self.mpeg1 { 12 } else { PACK_HEADER }
    }

    /// A stream like [`new`](Self::new)'s whose sound is linear PCM at
    /// `sample_rate` Hz, 16-bit stereo, in private stream 1.
    pub fn with_pcm(out: W, bitrate: u32, sample_rate: u32) -> PsWriter<W> {
        let mut w = PsWriter::new(out, bitrate, true);
        w.pcm = Some(sample_rate.max(1));
        w
    }

    /// Bytes written so far.
    pub fn bytes_written(&self) -> u64 {
        self.written
    }

    /// A video frame (or field pair) decoded at `dts` and shown at `pts`
    /// (90 kHz, from the stream's start: `START` is added).
    pub fn write_video(&mut self, data: &[u8], pts: u64, dts: u64) -> io::Result<()> {
        self.write_pes(VIDEO_ID, data, Some((pts + START, dts + START)))
    }

    /// MPEG audio frames that start at `pts` (90 kHz, from the stream's
    /// start): gathered until they fill a pack.
    pub fn write_audio(&mut self, data: &[u8], pts: u64) -> io::Result<()> {
        if self.pending.is_empty() {
            self.pending_pts = Some(pts + START);
        }
        self.pending.extend_from_slice(data);
        if self.pending.len() >= self.pack - self.header_len() - 14 {
            self.flush_audio()?;
        }
        Ok(())
    }

    /// Linear PCM samples (16-bit stereo, big-endian) that start at `pts`
    /// (90 kHz, from the stream's start): in packs of whole samples.
    pub fn write_pcm(&mut self, data: &[u8], pts: u64) -> io::Result<()> {
        if self.pending.is_empty() {
            self.pending_pts = Some(pts + START);
        }
        self.pending.extend_from_slice(data);
        while self.pending.len() >= PCM_PACK {
            self.flush_pcm(PCM_PACK)?;
        }
        Ok(())
    }

    /// `n` bytes of the PCM gathered in one PES packet, its header DVD's:
    /// the 1/600 s frames that start there, 16-bit, 48 kHz, stereo.
    fn flush_pcm(&mut self, n: usize) -> io::Result<()> {
        let rate = self.pcm.unwrap_or(48_000) as u64;
        let data: Vec<u8> = self.pending.drain(..n.min(self.pending.len())).collect();
        let pts = self.pending_pts.unwrap_or(START);
        let frames = (data.len() / 4 * 600 / rate as usize).clamp(1, 255) as u8;
        let header = [0xA0, frames, 0x00, 0x04, 0x00, 0x01, 0x80];
        let mut body = header.to_vec();
        body.extend_from_slice(&data);
        self.write_pes(PRIVATE_ID, &body, Some((pts, pts)))?;
        self.pending_pts = Some(pts + (data.len() as u64 / 4) * 90_000 / rate);
        Ok(())
    }

    fn flush_audio(&mut self) -> io::Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        if self.pcm.is_some() {
            let n = self.pending.len();
            return self.flush_pcm(n);
        }
        let data = std::mem::take(&mut self.pending);
        let pts = self.pending_pts.take().unwrap_or(START);
        self.write_pes(AUDIO_ID, &data, Some((pts, pts)))
    }

    /// Ends the stream (the audio still gathered, the end code); gives the
    /// writer back.
    pub fn finish(mut self) -> io::Result<W> {
        self.flush_audio()?;
        self.out.write_all(&[0, 0, 1, 0xB9])?;
        self.written += 4;
        self.out.flush()?;
        Ok(self.out)
    }

    /// `data` of stream `id` in as many packs as it takes, the first with
    /// its times.
    fn write_pes(&mut self, id: u8, data: &[u8], times: Option<(u64, u64)>) -> io::Result<()> {
        let mut rest = data;
        let mut first = true;
        let decode = times.map_or(0, |(_, dts)| dts);
        while first || !rest.is_empty() {
            let stamp = if first { times } else { None };
            let mut pack = Vec::with_capacity(self.pack);
            self.pack_header(&mut pack, decode);
            if self.packs == 0 {
                self.system_header(&mut pack);
            }
            let ts_len = match stamp {
                Some((pts, dts)) if pts != dts => 10,
                Some(_) => 5,
                // MPEG-1's byte saying there is none.
                None if self.mpeg1 => 1,
                None => 0,
            };
            // The packet's fixed bytes: its start code and length, and
            // MPEG-2's flags.
            let fixed = if self.mpeg1 { 6 } else { 9 };
            let room = self.pack - pack.len() - fixed - ts_len;
            let take = rest.len().min(room);
            let left = room - take;
            // What is left over: a padding packet, or stuffing in the
            // header where it is too small for one.
            let (stuffing, padding) = if left >= 6 { (0, left) } else { (left, 0) };
            pack.extend_from_slice(&[0, 0, 1, id]);
            if self.mpeg1 {
                // The stuffing first, then the times.
                pack.extend_from_slice(&((stuffing + ts_len + take) as u16).to_be_bytes());
                pack.extend(std::iter::repeat_n(0xFF, stuffing));
            } else {
                let header_len = ts_len + stuffing;
                let pes_len = 3 + header_len + take;
                pack.extend_from_slice(&(pes_len as u16).to_be_bytes());
                let aligned = if first && id == VIDEO_ID { 0x04 } else { 0 };
                pack.push(0x80 | aligned);
                pack.push(match ts_len {
                    10 => 0xC0,
                    5 => 0x80,
                    _ => 0,
                });
                pack.push(header_len as u8);
            }
            match stamp {
                Some((pts, dts)) if pts != dts => {
                    timestamp(&mut pack, 0x3, pts);
                    timestamp(&mut pack, 0x1, dts);
                }
                Some((pts, _)) => timestamp(&mut pack, 0x2, pts),
                None if self.mpeg1 => pack.push(0x0F),
                None => {}
            }
            if !self.mpeg1 {
                pack.extend(std::iter::repeat_n(0xFF, stuffing));
            }
            pack.extend_from_slice(&rest[..take]);
            if padding > 0 {
                pack.extend_from_slice(&[0, 0, 1, PADDING_ID]);
                pack.extend_from_slice(&((padding - 6) as u16).to_be_bytes());
                pack.extend(std::iter::repeat_n(0xFF, padding - 6));
            }
            debug_assert_eq!(pack.len(), self.pack);
            self.out.write_all(&pack)?;
            self.written += self.pack as u64;
            self.packs += 1;
            rest = &rest[take..];
            first = false;
        }
        Ok(())
    }

    /// A pack header; its clock follows the packs at the mux rate, and
    /// reaches what `decode` (90 kHz) carries no sooner than `AHEAD`
    /// before it.
    fn pack_header(&mut self, out: &mut Vec<u8>, decode: u64) {
        let step = self.pack as u64 * 27_000_000 / (self.rate as u64 * 50);
        let soonest = decode.saturating_sub(AHEAD) * 300;
        let scr = if self.packs == 0 {
            0
        } else {
            (self.scr + step).max(soonest)
        };
        self.scr = scr;
        let (base, ext) = (scr / 300, scr % 300);
        out.extend_from_slice(&[0, 0, 1, 0xBA]);
        if self.mpeg1 {
            // "0010", the clock at 90 kHz, the mux rate.
            timestamp(out, 0x2, base);
            let r = self.rate;
            out.push(0x80 | ((r >> 15) as u8 & 0x7F));
            out.push((r >> 7) as u8);
            out.push(((r << 1) as u8 & 0xFE) | 0x01);
            return;
        }
        out.push(0x44 | ((base >> 27) as u8 & 0x38) | ((base >> 28) as u8 & 0x03));
        out.push((base >> 20) as u8);
        out.push(((base >> 12) as u8 & 0xF8) | 0x04 | ((base >> 13) as u8 & 0x03));
        out.push((base >> 5) as u8);
        out.push(((base << 3) as u8 & 0xF8) | 0x04 | ((ext >> 7) as u8 & 0x03));
        out.push(((ext << 1) as u8 & 0xFE) | 0x01);
        let r = self.rate;
        out.push((r >> 14) as u8);
        out.push((r >> 6) as u8);
        out.push(((r << 2) as u8 & 0xFC) | 0x03);
        out.push(0xF8);
    }

    /// The system header: the streams and their decoders' buffers (the
    /// video's 232 KiB, the audio's 4 KiB, as DVD's).
    fn system_header(&self, out: &mut Vec<u8>) {
        let audio = if self.pcm.is_some() {
            PRIVATE_ID
        } else {
            AUDIO_ID
        };
        // Video CD's video buffer: 46 KiB.
        let video = if self.mpeg1 { 46 } else { 232 };
        let streams: &[(u8, u8, u16)] = if self.audio {
            &[(VIDEO_ID, 1, video), (audio, 0, 32)]
        } else {
            &[(VIDEO_ID, 1, video)]
        };
        let len = 6 + 3 * streams.len();
        out.extend_from_slice(&[0, 0, 1, 0xBB]);
        out.extend_from_slice(&(len as u16).to_be_bytes());
        let r = self.rate;
        out.push(0x80 | ((r >> 15) as u8 & 0x7F));
        out.push((r >> 7) as u8);
        out.push(((r << 1) as u8 & 0xFE) | 0x01);
        out.push(u8::from(self.audio) << 2);
        out.push(0xE0 | 0x01);
        // MPEG-1's reserved byte; MPEG-2's packet rate restriction off.
        out.push(if self.mpeg1 { 0xFF } else { 0x7F });
        for &(id, scale, size) in streams {
            out.push(id);
            out.push(0xC0 | (scale << 5) | ((size >> 8) as u8 & 0x1F));
            out.push(size as u8);
        }
    }
}

/// A PES timestamp: its 4-bit prefix and 33 bits with marker bits.
fn timestamp(out: &mut Vec<u8>, prefix: u8, t: u64) {
    out.push((prefix << 4) | ((t >> 29) as u8 & 0x0E) | 0x01);
    out.push((t >> 22) as u8);
    out.push(((t >> 14) as u8 & 0xFE) | 0x01);
    out.push((t >> 7) as u8);
    out.push(((t << 1) as u8 & 0xFE) | 0x01);
}

/// A PES timestamp read back (`at` its first byte).
pub fn read_timestamp(b: &[u8]) -> Option<u64> {
    let b = b.get(..5)?;
    Some(
        ((b[0] as u64 >> 1) & 0x07) << 30
            | (b[1] as u64) << 22
            | ((b[2] as u64) >> 1) << 15
            | (b[3] as u64) << 7
            | (b[4] as u64) >> 1,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each pack's PES packets: (stream id, timestamps, payload).
    fn packets(stream: &[u8]) -> Vec<(u8, Option<u64>, Vec<u8>)> {
        let mut out = Vec::new();
        for pack in stream.chunks(PACK) {
            if pack.len() < PACK {
                assert_eq!(pack, [0, 0, 1, 0xB9]);
                continue;
            }
            assert_eq!(&pack[..4], [0, 0, 1, 0xBA]);
            let mut at = PACK_HEADER;
            while at + 6 <= pack.len() {
                assert_eq!(&pack[at..at + 3], [0, 0, 1]);
                let id = pack[at + 3];
                let len = u16::from_be_bytes([pack[at + 4], pack[at + 5]]) as usize;
                let body = &pack[at + 6..at + 6 + len];
                if id == VIDEO_ID || id == AUDIO_ID {
                    let hl = body[2] as usize;
                    let pts = (body[1] & 0x80 != 0).then(|| read_timestamp(&body[3..]).unwrap());
                    out.push((id, pts, body[3 + hl..].to_vec()));
                }
                at += 6 + len;
            }
            assert_eq!(at, PACK);
        }
        out
    }

    #[test]
    fn frames_go_into_whole_packs_with_their_times() {
        let mut w = PsWriter::new(Vec::new(), 8_000_000, true);
        let big: Vec<u8> = (0..5000u32).map(|i| i as u8).collect();
        w.write_video(&big, 3003, 3003).unwrap();
        w.write_audio(&[1, 2, 3], 2160).unwrap();
        w.write_video(&[9; 10], 6006, 6006).unwrap();
        let stream = w.finish().unwrap();
        assert_eq!((stream.len() - 4) % PACK, 0);
        let p = packets(&stream);
        // The first frame over three packs, its time in the first.
        let video: Vec<u8> = p
            .iter()
            .filter(|(id, ..)| *id == VIDEO_ID)
            .take(3)
            .flat_map(|(_, _, d)| d.clone())
            .collect();
        assert_eq!(video, big);
        assert_eq!(p[0].1, Some(3003 + START));
        assert_eq!(p[1].1, None);
        // The audio waits for the end, its time kept.
        let audio = p.iter().find(|(id, ..)| *id == AUDIO_ID).unwrap();
        assert_eq!(audio.1, Some(2160 + START));
        assert_eq!(audio.2, [1, 2, 3]);
        // The system header in the first pack only.
        assert_eq!(&stream[PACK_HEADER..PACK_HEADER + 4], [0, 0, 1, 0xBB]);
        assert_ne!(
            &stream[PACK + PACK_HEADER..PACK + PACK_HEADER + 4],
            [0, 0, 1, 0xBB]
        );
    }

    #[test]
    fn mpeg1_packs_take_their_size_and_headers() {
        let mut w = PsWriter::mpeg1(Vec::new(), 1_400_000, true, 2324);
        w.write_video(&[7; 3000], 3003, 3003).unwrap();
        w.write_audio(&[1; 10], 0).unwrap();
        let s = w.finish().unwrap();
        assert_eq!((s.len() - 4) % 2324, 0);
        for pack in s[..s.len() - 4].chunks(2324) {
            assert_eq!(&pack[..4], [0, 0, 1, 0xBA]);
            assert_eq!(pack[4] & 0xF1, 0x21, "MPEG-1's pack header");
        }
        // The system header (6 + 2 streams), then the first frame's packet:
        // its time right after its length.
        assert_eq!(&s[12..16], [0, 0, 1, 0xBB]);
        assert_eq!(&s[30..34], [0, 0, 1, VIDEO_ID]);
        assert_eq!(s[36] & 0xF1, 0x21);
        assert_eq!(read_timestamp(&s[36..]), Some(3003 + START));
        // The rest of the frame: no time.
        assert_eq!(&s[2324 + 12..2324 + 16], [0, 0, 1, VIDEO_ID]);
        assert_eq!(s[2324 + 18], 0x0F);
    }

    #[test]
    fn timestamps_round_trip() {
        for t in [0u64, 1, 90_000, (1 << 33) - 1] {
            let mut b = Vec::new();
            timestamp(&mut b, 2, t);
            assert_eq!(read_timestamp(&b), Some(t));
        }
    }
}
