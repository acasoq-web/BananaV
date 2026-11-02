//! MPEG-2 video (ISO/IEC 13818-2) written by BananaV: Main profile, 4:2:0,
//! 8 bits, frame pictures, I, P and B pictures (`Settings::b_frames`
//! between two anchors, held until the anchor after them is coded: the
//! pictures come out in the order they are decoded, each with its place in
//! display order; a GOP's last frame is an anchor, so GOPs stay closed),
//! closed GOPs each opened by a sequence header, the default
//! quantiser matrices, a quantiser a picture from a simple rate control,
//! and motion found by a diamond search at whole pixels. MPEG-1 video
//! (ISO/IEC 11172-2, `Settings::mpeg1`) is the same pictures without
//! MPEG-2's extensions: the pixels' shape in the sequence header, escapes
//! of 8 or 16 bits, levels within ±255, each coefficient made odd (its
//! mismatch control) and BT.601's colours, which its decoders take.

mod bits;
mod dct;
mod tables;

use bits::Bits;
use tables::{
    ADDRESS_ESCAPE, ADDRESS_INCREMENT, DC_SIZE_CHROMA, DC_SIZE_LUMA, EOB, ESCAPE, INTRA_MATRIX,
    MOTION_CODE, ZIGZAG,
};

/// How a stream is coded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub width: u32,
    pub height: u32,
    /// Frames a second, as a fraction.
    pub rate: (u32, u32),
    /// Bits a second: the average aimed at, and the most.
    pub bitrate: u32,
    pub max_bitrate: u32,
    /// Frames from one I-picture to the next.
    pub gop: u32,
    /// The display's shape: 1 square pixels, 2 4:3, 3 16:9, 4 2.21:1.
    pub aspect: u8,
    /// Interlaced frames and whether the top field comes first (`None`:
    /// progressive).
    pub top_field_first: Option<bool>,
    /// A sequence of interlaced video whatever its frames are, as
    /// DVD-Video's must be.
    pub interlaced_sequence: bool,
    /// MPEG-1 video (ISO/IEC 11172-2) instead, as Video CDs carry.
    pub mpeg1: bool,
    /// B-pictures between two anchors (I or P); 0: I and P only, coded in
    /// the order they show.
    pub b_frames: u32,
}

/// A coded picture: its bytes (with the sequence and GOP headers before
/// an I-picture).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub data: Vec<u8>,
    pub keyframe: bool,
    /// Its frame's place in display order (from the stream's first).
    pub display: u64,
}

/// A picture's coding type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    I,
    P,
    B,
}

/// A picture's planes, padded to whole macroblocks.
#[derive(Clone, Debug)]
struct Frame {
    y: Vec<u8>,
    cb: Vec<u8>,
    cr: Vec<u8>,
    /// The padded luma's size.
    w: usize,
    h: usize,
}

impl Frame {
    fn at(&self, plane: usize, x: usize, y: usize) -> u8 {
        match plane {
            0 => self.y[y * self.w + x],
            1 => self.cb[y * (self.w / 2) + x],
            _ => self.cr[y * (self.w / 2) + x],
        }
    }
}

/// A stream being coded.
#[derive(Debug)]
pub struct Encoder {
    s: Settings,
    mbw: usize,
    mbh: usize,
    /// The last anchor (I or P) as decoders rebuild it, and the one
    /// before it: B-pictures between them predict from both.
    reference: Option<Frame>,
    previous: Option<Frame>,
    /// Frames given (display order), and since the GOP's first.
    frames: u64,
    in_gop: u32,
    /// The GOP's first frame (temporal references count from it).
    gop_start: u64,
    /// Frames waiting to be B-pictures: their places and pictures.
    held: Vec<(u64, Frame)>,
    /// The quantisers (`quantiser_scale_code`, as numbers) the rate
    /// control holds for I-, P- and B-pictures.
    q_intra: f32,
    q_inter: f32,
    q_b: f32,
}

/// The frame_rate_code and its rate.
const RATES: [(u32, f64); 8] = [
    (1, 24_000.0 / 1001.0),
    (2, 24.0),
    (3, 25.0),
    (4, 30_000.0 / 1001.0),
    (5, 30.0),
    (6, 50.0),
    (7, 60_000.0 / 1001.0),
    (8, 60.0),
];

impl Encoder {
    pub fn new(s: Settings) -> Encoder {
        Encoder {
            s,
            mbw: (s.width as usize).div_ceil(16).max(1),
            mbh: (s.height as usize).div_ceil(16).max(1),
            reference: None,
            previous: None,
            frames: 0,
            in_gop: 0,
            gop_start: 0,
            held: Vec::new(),
            q_intra: 5.0,
            q_inter: 7.0,
            q_b: 9.0,
        }
    }

    fn fps(&self) -> f64 {
        self.s.rate.0 as f64 / self.s.rate.1.max(1) as f64
    }

    /// The bits a picture of each kind aims at: I, P, B.
    fn targets(&self) -> (f64, f64, f64) {
        let avg = self.s.bitrate as f64 / self.fps().max(1.0);
        let gop = self.s.gop.max(1) as f64;
        if gop <= 1.0 {
            return (avg, avg, avg);
        }
        let intra = 3.0 * avg;
        let inter = ((gop * avg - intra) / (gop - 1.0)).max(avg * 0.3);
        if self.s.b_frames == 0 {
            return (intra, inter, inter);
        }
        // B-pictures cost less than P-pictures: the rest shared so.
        let m = self.s.b_frames as f64 + 1.0;
        let p = inter * m / (1.0 + 0.6 * (m - 1.0));
        (intra, p, 0.6 * p)
    }

    /// One frame given as NV12 (the `width` × `height` luma, then the
    /// chroma's Cb and Cr interleaved): an I-picture when its GOP begins
    /// (or `key` asks), a P-picture once `b_frames` frames wait before it
    /// — they then B-pictures —, else it waits. The pictures ready, in the
    /// order they are decoded.
    pub fn encode(&mut self, nv12: &[u8], key: bool) -> Vec<Picture> {
        let cur = self.load(nv12);
        let n = self.frames;
        self.frames += 1;
        let mut out = Vec::new();
        if key || self.reference.is_none() || self.in_gop >= self.s.gop.max(1) {
            // The GOP ends with the frames waiting, its last one an anchor.
            self.release(&mut out);
            self.in_gop = 0;
            self.gop_start = n;
            out.push(self.code(&cur, n, Kind::I));
        } else if self.held.len() as u32 >= self.s.b_frames {
            out.push(self.code(&cur, n, Kind::P));
            for (d, f) in std::mem::take(&mut self.held) {
                out.push(self.code(&f, d, Kind::B));
            }
        } else {
            self.held.push((n, cur));
        }
        self.in_gop += 1;
        out
    }

    /// The frames still waiting, at the stream's end.
    pub fn finish(&mut self) -> Vec<Picture> {
        let mut out = Vec::new();
        self.release(&mut out);
        out
    }

    /// The frames waiting coded: the last as a P-picture, the others as
    /// B-pictures between it and the anchor before.
    fn release(&mut self, out: &mut Vec<Picture>) {
        let Some((d, last)) = self.held.pop() else {
            return;
        };
        out.push(self.code(&last, d, Kind::P));
        for (d, f) in std::mem::take(&mut self.held) {
            out.push(self.code(&f, d, Kind::B));
        }
    }

    /// Frame `display` coded as a picture of `kind`.
    fn code(&mut self, cur: &Frame, display: u64, kind: Kind) -> Picture {
        let mut b = Bits::default();
        if kind == Kind::I {
            self.sequence_headers(&mut b);
            self.gop_header(&mut b, display);
        }
        let q = match kind {
            Kind::I => self.q_intra,
            Kind::P => self.q_inter,
            Kind::B => self.q_b,
        };
        let qcode = q.round().clamp(1.0, 31.0) as u32;
        self.picture_header(&mut b, kind, (display - self.gop_start) as u32);
        let before = b.len();
        let recon = if kind == Kind::B {
            self.b_slices(&mut b, cur, qcode);
            None
        } else {
            Some(self.slices(&mut b, cur, kind == Kind::I, qcode))
        };
        let spent = (b.len() - before) as f64;
        // The rate control: the quantiser follows the bits spent.
        let (ti, tp, tb) = self.targets();
        let (target, q) = match kind {
            Kind::I => (ti, &mut self.q_intra),
            Kind::P => (tp, &mut self.q_inter),
            Kind::B => (tb, &mut self.q_b),
        };
        *q = (*q * (spent / target.max(1.0)).powf(0.6) as f32).clamp(1.0, 31.0);
        if let Some(recon) = recon {
            self.previous = self.reference.replace(recon);
        }
        Picture {
            data: b.finish(),
            keyframe: kind == Kind::I,
            display,
        }
    }

    /// The NV12 frame as padded planes.
    fn load(&self, nv12: &[u8]) -> Frame {
        let (sw, sh) = (self.s.width as usize, self.s.height as usize);
        let (w, h) = (self.mbw * 16, self.mbh * 16);
        let mut y = vec![0u8; w * h];
        for row in 0..h {
            let sy = row.min(sh - 1);
            for col in 0..w {
                let sx = col.min(sw - 1);
                y[row * w + col] = nv12.get(sy * sw + sx).copied().unwrap_or(16);
            }
        }
        let (cw, ch) = (w / 2, h / 2);
        let (scw, sch) = (sw / 2, sh / 2);
        let uv = &nv12[(sw * sh).min(nv12.len())..];
        let mut cb = vec![128u8; cw * ch];
        let mut cr = vec![128u8; cw * ch];
        for row in 0..ch {
            let sy = row.min(sch.max(1) - 1);
            for col in 0..cw {
                let sx = col.min(scw.max(1) - 1);
                let i = (sy * scw + sx) * 2;
                cb[row * cw + col] = uv.get(i).copied().unwrap_or(128);
                cr[row * cw + col] = uv.get(i + 1).copied().unwrap_or(128);
            }
        }
        let mut f = Frame { y, cb, cr, w, h };
        if self.s.mpeg1 {
            to_bt601(&mut f);
        }
        f
    }

    fn sequence_headers(&self, b: &mut Bits) {
        let s = &self.s;
        let fps = self.fps();
        let code = RATES
            .iter()
            .min_by(|a, c| (a.1 - fps).abs().total_cmp(&(c.1 - fps).abs()))
            .map_or(4, |r| r.0);
        let rate = s.max_bitrate.max(s.bitrate).div_ceil(400).max(1);
        if s.mpeg1 {
            self.mpeg1_sequence_header(b, code, rate);
            return;
        }
        // DVD's buffer: 112 × 16 kbit.
        let vbv = 112u32;
        b.start(0xB3);
        b.put(s.width & 0xFFF, 12);
        b.put(s.height & 0xFFF, 12);
        b.put(s.aspect.clamp(1, 4) as u32, 4);
        b.put(code, 4);
        b.put(rate & 0x3FFFF, 18);
        b.put(1, 1);
        b.put(vbv & 0x3FF, 10);
        b.put(0, 3);
        // The sequence extension: Main profile, Main level up to standard
        // definition, High level beyond; 4:2:0.
        let level = if s.width > 1440 || s.height > 1152 {
            4
        } else if s.width > 720 || s.height > 576 {
            6
        } else {
            8
        };
        b.start(0xB5);
        b.put(1, 4);
        b.put((4 << 4) | level, 8);
        let progressive = s.top_field_first.is_none() && !s.interlaced_sequence;
        b.put(u32::from(progressive), 1);
        b.put(1, 2);
        b.put(s.width >> 12, 2);
        b.put(s.height >> 12, 2);
        b.put(rate >> 18, 12);
        b.put(1, 1);
        b.put(vbv >> 10, 8);
        b.put(0, 1);
        b.put(0, 2);
        b.put(0, 5);
        // The sequence display extension: BT.709's colours.
        b.start(0xB5);
        b.put(2, 4);
        b.put(5, 3);
        b.put(1, 1);
        b.put(1, 8);
        b.put(1, 8);
        b.put(1, 8);
        b.put(s.width & 0x3FFF, 14);
        b.put(1, 1);
        b.put(s.height & 0x3FFF, 14);
    }

    /// MPEG-1's sequence header: the pixels' shape, the buffer, and the
    /// constrained parameters flag where the stream keeps within them
    /// (Video CD's do).
    fn mpeg1_sequence_header(&self, b: &mut Bits, code: u32, rate: u32) {
        let s = &self.s;
        let mbs = self.mbw * self.mbh;
        let constrained = s.width <= 768
            && s.height <= 576
            && mbs <= 396
            && mbs as f64 * self.fps() <= 9900.0
            && rate * 400 <= 1_856_000;
        // Video CD's buffer: 20 × 16 kbit.
        let vbv = if constrained { 20 } else { 112 };
        b.start(0xB3);
        b.put(s.width & 0xFFF, 12);
        b.put(s.height & 0xFFF, 12);
        b.put(pel_aspect(s), 4);
        b.put(code, 4);
        // All ones would say a variable rate.
        b.put(rate.min(0x3FFFE), 18);
        b.put(1, 1);
        b.put(vbv, 10);
        b.put(u32::from(constrained), 1);
        b.put(0, 1);
        b.put(0, 1);
    }

    /// The largest level a coefficient takes: MPEG-1's escapes reach 255.
    fn max_level(&self) -> i32 {
        if self.s.mpeg1 { 255 } else { 2047 }
    }

    /// The GOP header: its first frame's time, closed.
    fn gop_header(&self, b: &mut Bits, first: u64) {
        let fps = self.fps().round().max(1.0) as u64;
        let f = first;
        let secs = f / fps;
        b.start(0xB8);
        b.put(0, 1);
        b.put((secs / 3600 % 24) as u32, 5);
        b.put((secs / 60 % 60) as u32, 6);
        b.put(1, 1);
        b.put((secs % 60) as u32, 6);
        b.put((f % fps) as u32, 6);
        // Closed, not broken.
        b.put(1, 1);
        b.put(0, 1);
    }

    /// The picture header (`temporal`: its place in the GOP) and MPEG-2's
    /// picture coding extension.
    fn picture_header(&self, b: &mut Bits, kind: Kind, temporal: u32) {
        b.start(0x00);
        b.put(temporal % 1024, 10);
        b.put(
            match kind {
                Kind::I => 1,
                Kind::P => 2,
                Kind::B => 3,
            },
            3,
        );
        b.put(0xFFFF, 16);
        // MPEG-1's forward and backward f_codes; MPEG-2's are in the
        // extension.
        let f = if self.s.mpeg1 { F_CODE } else { 7 };
        if kind != Kind::I {
            b.put(0, 1);
            b.put(f, 3);
        }
        if kind == Kind::B {
            b.put(0, 1);
            b.put(f, 3);
        }
        b.put(0, 1);
        if self.s.mpeg1 {
            return;
        }
        // The picture coding extension.
        b.start(0xB5);
        b.put(8, 4);
        let forward = if kind == Kind::I { 15 } else { F_CODE };
        let backward = if kind == Kind::B { F_CODE } else { 15 };
        b.put(forward, 4);
        b.put(forward, 4);
        b.put(backward, 4);
        b.put(backward, 4);
        b.put(0, 2);
        b.put(3, 2);
        b.put(u32::from(self.s.top_field_first.unwrap_or(false)), 1);
        b.put(1, 1);
        b.put(0, 1);
        b.put(0, 1);
        b.put(0, 1);
        b.put(0, 1);
        b.put(0, 1);
        let progressive = u32::from(self.s.top_field_first.is_none());
        b.put(progressive, 1);
        b.put(progressive, 1);
        b.put(0, 1);
    }

    /// The picture's slices (one a row of macroblocks); the picture as a
    /// decoder rebuilds it.
    fn slices(&self, b: &mut Bits, cur: &Frame, intra: bool, qcode: u32) -> Frame {
        let qscale = 2 * qcode as i32;
        let mut recon = Frame {
            y: vec![0; cur.y.len()],
            cb: vec![0; cur.cb.len()],
            cr: vec![0; cur.cr.len()],
            w: cur.w,
            h: cur.h,
        };
        let mut row_mvs: Vec<(i32, i32)> = vec![(0, 0); self.mbw];
        for row in 0..self.mbh {
            b.start((row + 1).min(0xAF) as u8);
            b.put(qcode, 5);
            b.put(0, 1);
            let mut dc_pred = [128i32; 3];
            let mut pmv = (0i32, 0i32);
            let mut skipped = 0usize;
            let mut left_mv = (0i32, 0i32);
            for (col, above) in row_mvs.iter_mut().enumerate() {
                let mb = Mb { col, row };
                let last = col + 1 == self.mbw;
                if intra {
                    address_increment(b, skipped + 1);
                    b.code("1");
                    self.intra_mb(b, cur, &mut recon, mb, qscale, &mut dc_pred);
                    continue;
                }
                let reference = self.reference.as_ref().expect("a reference");
                let mv = search(
                    cur,
                    reference,
                    mb,
                    &[(0, 0), left_mv, *above, pmv_full(pmv)],
                );
                left_mv = mv.0;
                *above = mv.0;
                if mv.1 > intra_cost(cur, mb) + 512 {
                    // Intra in a P-picture.
                    address_increment(b, skipped + 1);
                    skipped = 0;
                    b.code("00011");
                    self.intra_mb(b, cur, &mut recon, mb, qscale, &mut dc_pred);
                    pmv = (0, 0);
                    continue;
                }
                let (dx, dy) = mv.0;
                let pred = predict(reference, mb, dx, dy);
                let coded = self.inter_blocks(cur, &pred, mb, qscale);
                let cbp = coded_pattern(&coded);
                dc_pred = [128; 3];
                if (dx, dy) == (0, 0) && cbp == 0 && col != 0 && !last {
                    // Skipped: the reference as it is.
                    skipped += 1;
                    pmv = (0, 0);
                    put_prediction(&mut recon, &pred, mb, None, self.s.mpeg1);
                    continue;
                }
                address_increment(b, skipped + 1);
                skipped = 0;
                b.code(if cbp != 0 { "1" } else { "001" });
                let mv_half = (2 * dx, 2 * dy);
                motion_component(b, mv_half.0 - pmv.0);
                motion_component(b, mv_half.1 - pmv.1);
                pmv = mv_half;
                if cbp != 0 {
                    b.code(tables::coded_block_pattern(cbp));
                    for (i, blk) in coded.iter().enumerate() {
                        if cbp & (1 << (5 - i)) != 0 {
                            coefficients(b, blk, self.s.mpeg1);
                        }
                    }
                }
                put_prediction(&mut recon, &pred, mb, Some((&coded, qscale)), self.s.mpeg1);
            }
        }
        recon
    }

    /// A B-picture's slices: each macroblock predicted from the anchor
    /// before (forward), the one after (backward) or both (their mean),
    /// whichever is nearest, or intra; none skipped. B-pictures are no
    /// picture's reference: nothing is rebuilt.
    fn b_slices(&self, b: &mut Bits, cur: &Frame, qcode: u32) {
        let qscale = 2 * qcode as i32;
        let fwd = self.previous.as_ref().expect("an anchor before");
        let bwd = self.reference.as_ref().expect("an anchor after");
        // Where intra macroblocks are rebuilt; nothing reads it.
        let mut scratch = Frame {
            y: vec![0; cur.y.len()],
            cb: vec![0; cur.cb.len()],
            cr: vec![0; cur.cr.len()],
            w: cur.w,
            h: cur.h,
        };
        let mut above = vec![((0i32, 0i32), (0i32, 0i32)); self.mbw];
        for row in 0..self.mbh {
            b.start((row + 1).min(0xAF) as u8);
            b.put(qcode, 5);
            b.put(0, 1);
            let mut dc_pred = [128i32; 3];
            let (mut pmv_f, mut pmv_b) = ((0i32, 0i32), (0i32, 0i32));
            let (mut left_f, mut left_b) = ((0i32, 0i32), (0i32, 0i32));
            for (col, up) in above.iter_mut().enumerate() {
                let mb = Mb { col, row };
                let (vf, sf) = search(cur, fwd, mb, &[(0, 0), left_f, up.0, pmv_full(pmv_f)]);
                let (vb, sb) = search(cur, bwd, mb, &[(0, 0), left_b, up.1, pmv_full(pmv_b)]);
                (left_f, left_b) = (vf, vb);
                *up = (vf, vb);
                let pf = predict(fwd, mb, vf.0, vf.1);
                let pb = predict(bwd, mb, vb.0, vb.1);
                let pi = mean(&pf, &pb);
                let si = sad_prediction(cur, &pi, mb);
                // 1 forward, 2 backward, 3 both.
                let (mode, best, pred) = [(1u8, sf, &pf), (2, sb, &pb), (3, si, &pi)]
                    .into_iter()
                    .min_by_key(|m| m.1)
                    .expect("three predictions");
                address_increment(b, 1);
                if best > intra_cost(cur, mb) + 512 {
                    b.code("00011");
                    self.intra_mb(b, cur, &mut scratch, mb, qscale, &mut dc_pred);
                    (pmv_f, pmv_b) = ((0, 0), (0, 0));
                    continue;
                }
                dc_pred = [128; 3];
                let coded = self.inter_blocks(cur, pred, mb, qscale);
                let cbp = coded_pattern(&coded);
                b.code(match (mode, cbp != 0) {
                    (3, false) => "10",
                    (3, true) => "11",
                    (2, false) => "010",
                    (2, true) => "011",
                    (_, false) => "0010",
                    (_, true) => "0011",
                });
                if mode & 1 != 0 {
                    let v = (2 * vf.0, 2 * vf.1);
                    motion_component(b, v.0 - pmv_f.0);
                    motion_component(b, v.1 - pmv_f.1);
                    pmv_f = v;
                }
                if mode & 2 != 0 {
                    let v = (2 * vb.0, 2 * vb.1);
                    motion_component(b, v.0 - pmv_b.0);
                    motion_component(b, v.1 - pmv_b.1);
                    pmv_b = v;
                }
                if cbp != 0 {
                    b.code(tables::coded_block_pattern(cbp));
                    for (i, blk) in coded.iter().enumerate() {
                        if cbp & (1 << (5 - i)) != 0 {
                            coefficients(b, blk, self.s.mpeg1);
                        }
                    }
                }
            }
        }
    }

    /// An intra macroblock's six blocks, written and rebuilt.
    fn intra_mb(
        &self,
        b: &mut Bits,
        cur: &Frame,
        recon: &mut Frame,
        mb: Mb,
        qscale: i32,
        dc_pred: &mut [i32; 3],
    ) {
        let max = self.max_level() as f32;
        for (i, (plane, bx, by)) in mb.blocks().into_iter().enumerate() {
            let px: [f32; 64] =
                std::array::from_fn(|k| cur.at(plane, bx + k % 8, by + k / 8) as f32);
            let f = dct::forward(&px);
            let mut levels = [0i32; 64];
            levels[0] = (f[0] / 8.0).round().clamp(0.0, 255.0) as i32;
            for k in 1..64 {
                let w = INTRA_MATRIX[k] as f32;
                levels[k] = (16.0 * f[k] / (w * qscale as f32)).round().clamp(-max, max) as i32;
            }
            let c = if i < 4 { 0 } else { i - 3 };
            let diff = levels[0] - dc_pred[c];
            dc_pred[c] = levels[0];
            dc_differential(b, diff, c == 0);
            ac_coefficients(b, &levels, self.s.mpeg1);
            let back = dct::inverse(&dct::dequantise(
                &levels,
                &INTRA_MATRIX,
                qscale,
                true,
                self.s.mpeg1,
            ));
            for (k, &r) in back.iter().enumerate() {
                put(recon, plane, bx + k % 8, by + k / 8, r.clamp(0, 255) as u8);
            }
        }
    }

    /// A non-intra macroblock's six blocks of levels (raster order).
    fn inter_blocks(&self, cur: &Frame, pred: &Prediction, mb: Mb, qscale: i32) -> [[i32; 64]; 6] {
        let mut out = [[0i32; 64]; 6];
        for (i, (plane, bx, by)) in mb.blocks().into_iter().enumerate() {
            let (ox, oy) = mb.origin(plane);
            let res: [f32; 64] = std::array::from_fn(|k| {
                let (x, y) = (bx + k % 8, by + k / 8);
                cur.at(plane, x, y) as f32 - pred.at(plane, x - ox, y - oy) as f32
            });
            let f = dct::forward(&res);
            for k in 0..64 {
                // Flat matrix 16: the level whose rebuilt value is nearest,
                // under a dead zone of one step.
                let m = f[k].abs() / qscale as f32;
                let level = m.floor().min(self.max_level() as f32) as i32;
                out[i][k] = if f[k] < 0.0 { -level } else { level };
            }
        }
        out
    }
}

/// BT.709's Y′, Pb and Pr (0–1, ±0.5) as BT.601's.
fn bt709_to_601(y: f32, pb: f32, pr: f32) -> [f32; 3] {
    let r = y + 1.5748 * pr;
    let b = y + 1.8556 * pb;
    let g = (y - 0.2126 * r - 0.0722 * b) / 0.7152;
    let y601 = 0.299 * r + 0.587 * g + 0.114 * b;
    [y601, (b - y601) / 1.772, (r - y601) / 1.402]
}

/// The frame's BT.709 colours (the render's) made BT.601's, which MPEG-1
/// decoders take, its sequence saying nothing of them: each chroma sample
/// by itself (BT.601's Pb and Pr do not depend on the luma), each luma
/// sample moved by the chroma over it; in 8-bit video levels.
fn to_bt601(f: &mut Frame) {
    let cw = f.w / 2;
    let unit = |v: u8, mid: f32, span: f32| (v as f32 - mid) / span;
    let byte = |v: f32| v.round().clamp(0.0, 255.0) as u8;
    let old: Vec<(f32, f32)> =
        f.cb.iter()
            .zip(&f.cr)
            .map(|(&cb, &cr)| (unit(cb, 128.0, 224.0), unit(cr, 128.0, 224.0)))
            .collect();
    for (i, v) in f.y.iter_mut().enumerate() {
        let (x, y) = (i % f.w, i / f.w);
        let (pb, pr) = old[(y / 2) * cw + x / 2];
        let [y601, _, _] = bt709_to_601(unit(*v, 16.0, 219.0), pb, pr);
        *v = byte(16.0 + 219.0 * y601);
    }
    for (i, &(pb, pr)) in old.iter().enumerate() {
        let [_, pb601, pr601] = bt709_to_601(0.5, pb, pr);
        f.cb[i] = byte(128.0 + 224.0 * pb601);
        f.cr[i] = byte(128.0 + 224.0 * pr601);
    }
}

/// MPEG-1's pel_aspect_ratio codes and their pixels' shapes (height ÷
/// width).
const PEL_ASPECT: [(u32, f64); 14] = [
    (1, 1.0),
    (2, 0.6735),
    (3, 0.7031),
    (4, 0.7615),
    (5, 0.8055),
    (6, 0.8437),
    (7, 0.8935),
    (8, 0.9157),
    (9, 0.9815),
    (10, 1.0255),
    (11, 1.0695),
    (12, 1.0950),
    (13, 1.1575),
    (14, 1.2015),
];

/// MPEG-1's pel_aspect_ratio: the code of the pixels that show the frame
/// at `aspect`'s display shape (Video CD's 352 × 240 at 4:3: 12).
fn pel_aspect(s: &Settings) -> u32 {
    let frame = s.width as f64 / s.height.max(1) as f64;
    let display = match s.aspect {
        2 => 4.0 / 3.0,
        3 => 16.0 / 9.0,
        4 => 2.21,
        _ => frame,
    };
    let shape = frame / display;
    PEL_ASPECT
        .iter()
        .min_by(|a, b| (a.1 - shape).abs().total_cmp(&(b.1 - shape).abs()))
        .map_or(1, |p| p.0)
}

/// `f_code` of P-pictures: vectors up to ±32 half pixels.
const F_CODE: u32 = 2;

/// The motion search's reach, in whole pixels.
const REACH: i32 = 15;

#[derive(Clone, Copy, Debug)]
struct Mb {
    col: usize,
    row: usize,
}

impl Mb {
    /// Its blocks: (plane, x, y) of Y0–Y3, Cb, Cr.
    fn blocks(self) -> [(usize, usize, usize); 6] {
        let (x, y) = (self.col * 16, self.row * 16);
        let (cx, cy) = (self.col * 8, self.row * 8);
        [
            (0, x, y),
            (0, x + 8, y),
            (0, x, y + 8),
            (0, x + 8, y + 8),
            (1, cx, cy),
            (2, cx, cy),
        ]
    }

    /// The top left corner of its area in `plane`.
    fn origin(self, plane: usize) -> (usize, usize) {
        if plane == 0 {
            (self.col * 16, self.row * 16)
        } else {
            (self.col * 8, self.row * 8)
        }
    }
}

/// A macroblock's prediction: 16×16 luma, 8×8 of each chroma.
struct Prediction {
    y: [u8; 256],
    cb: [u8; 64],
    cr: [u8; 64],
}

impl Prediction {
    fn at(&self, plane: usize, x: usize, y: usize) -> u8 {
        match plane {
            0 => self.y[y * 16 + x],
            1 => self.cb[y * 8 + x],
            _ => self.cr[y * 8 + x],
        }
    }
}

fn put(f: &mut Frame, plane: usize, x: usize, y: usize, v: u8) {
    let cw = f.w / 2;
    match plane {
        0 => f.y[y * f.w + x] = v,
        1 => f.cb[y * cw + x] = v,
        _ => f.cr[y * cw + x] = v,
    }
}

/// The macroblock `mb` of the reference moved by (`dx`, `dy`) whole
/// pixels, its chroma by half that, in half pixels, as decoders form it.
fn predict(r: &Frame, mb: Mb, dx: i32, dy: i32) -> Prediction {
    let (x0, y0) = ((mb.col * 16) as i32 + dx, (mb.row * 16) as i32 + dy);
    let y = std::array::from_fn(|k| r.y[(y0 as usize + k / 16) * r.w + x0 as usize + k % 16]);
    // The chroma's vector: half the luma's (in half pixels), truncated.
    let (vx, vy) = ((2 * dx) / 2, (2 * dy) / 2);
    let (ix, hx) = (vx >> 1, vx & 1);
    let (iy, hy) = (vy >> 1, vy & 1);
    let cw = r.w / 2;
    let (cx0, cy0) = ((mb.col * 8) as i32 + ix, (mb.row * 8) as i32 + iy);
    let chroma = |plane: &[u8]| -> [u8; 64] {
        std::array::from_fn(|k| {
            let (x, y) = (cx0 + (k % 8) as i32, cy0 + (k / 8) as i32);
            let at = |xx: i32, yy: i32| plane[yy as usize * cw + xx as usize] as u32;
            let a = at(x, y);
            match (hx, hy) {
                (0, 0) => a as u8,
                (1, 0) => ((a + at(x + 1, y) + 1) >> 1) as u8,
                (0, _) => ((a + at(x, y + 1) + 1) >> 1) as u8,
                _ => ((a + at(x + 1, y) + at(x, y + 1) + at(x + 1, y + 1) + 2) >> 2) as u8,
            }
        })
    };
    Prediction {
        y,
        cb: chroma(&r.cb),
        cr: chroma(&r.cr),
    }
}

/// The macroblock rebuilt: its prediction, plus its residual's levels
/// (at `qscale`) where given.
fn put_prediction(
    f: &mut Frame,
    pred: &Prediction,
    mb: Mb,
    coded: Option<(&[[i32; 64]; 6], i32)>,
    mpeg1: bool,
) {
    let flat = [16u8; 64];
    for (i, (plane, bx, by)) in mb.blocks().into_iter().enumerate() {
        let (ox, oy) = mb.origin(plane);
        let residual = coded
            .filter(|(c, _)| c[i].iter().any(|&v| v != 0))
            .map(|(c, q)| dct::inverse(&dct::dequantise(&c[i], &flat, q, false, mpeg1)));
        for k in 0..64 {
            let (x, y) = (bx + k % 8, by + k / 8);
            let p = pred.at(plane, x - ox, y - oy) as i32;
            let v = p + residual.map_or(0, |r| r[k]);
            put(f, plane, x, y, v.clamp(0, 255) as u8);
        }
    }
}

/// The coded_block_pattern of a macroblock's levels: a bit for each block
/// with any.
fn coded_pattern(coded: &[[i32; 64]; 6]) -> u8 {
    coded.iter().enumerate().fold(0u8, |c, (i, blk)| {
        if blk.iter().any(|&v| v != 0) {
            c | (1 << (5 - i))
        } else {
            c
        }
    })
}

/// Two predictions' mean, as decoders form a bidirectional one.
fn mean(a: &Prediction, b: &Prediction) -> Prediction {
    let avg = |x: u8, y: u8| ((x as u16 + y as u16 + 1) >> 1) as u8;
    Prediction {
        y: std::array::from_fn(|k| avg(a.y[k], b.y[k])),
        cb: std::array::from_fn(|k| avg(a.cb[k], b.cb[k])),
        cr: std::array::from_fn(|k| avg(a.cr[k], b.cr[k])),
    }
}

/// The sum of absolute differences of the macroblock's luma and a
/// prediction's.
fn sad_prediction(cur: &Frame, p: &Prediction, mb: Mb) -> u32 {
    let (x0, y0) = (mb.col * 16, mb.row * 16);
    (0..256)
        .map(|k| {
            let c = cur.y[(y0 + k / 16) * cur.w + x0 + k % 16] as i32;
            (c - p.y[k] as i32).unsigned_abs()
        })
        .sum()
}

/// The sum of absolute differences of the macroblock's luma and the
/// reference's moved by (`dx`, `dy`).
fn sad(cur: &Frame, r: &Frame, mb: Mb, dx: i32, dy: i32, best: u32) -> u32 {
    let (x0, y0) = (mb.col * 16, mb.row * 16);
    let (rx, ry) = ((x0 as i32 + dx) as usize, (y0 as i32 + dy) as usize);
    let mut s = 0u32;
    for y in 0..16 {
        let c = &cur.y[(y0 + y) * cur.w + x0..][..16];
        let p = &r.y[(ry + y) * r.w + rx..][..16];
        for x in 0..16 {
            s += (c[x] as i32 - p[x] as i32).unsigned_abs();
        }
        if s >= best {
            return s;
        }
    }
    s
}

/// Whether (`dx`, `dy`) keeps the macroblock inside the reference.
fn inside(r: &Frame, mb: Mb, dx: i32, dy: i32) -> bool {
    let (x, y) = ((mb.col * 16) as i32 + dx, (mb.row * 16) as i32 + dy);
    dx.abs() <= REACH
        && dy.abs() <= REACH
        && x >= 0
        && y >= 0
        && x + 16 <= r.w as i32
        && y + 16 <= r.h as i32
}

/// The motion of the macroblock from the reference: the best of the
/// candidates, then a diamond search around it; with its SAD.
fn search(cur: &Frame, r: &Frame, mb: Mb, candidates: &[(i32, i32)]) -> ((i32, i32), u32) {
    let mut best = (0, 0);
    let mut best_sad = sad(cur, r, mb, 0, 0, u32::MAX);
    for &(dx, dy) in candidates {
        if inside(r, mb, dx, dy) {
            let s = sad(cur, r, mb, dx, dy, best_sad);
            if s < best_sad {
                best_sad = s;
                best = (dx, dy);
            }
        }
    }
    for step in [4, 2, 1] {
        loop {
            let mut moved = false;
            for (sx, sy) in [(step, 0), (-step, 0), (0, step), (0, -step)] {
                let (dx, dy) = (best.0 + sx, best.1 + sy);
                if inside(r, mb, dx, dy) {
                    let s = sad(cur, r, mb, dx, dy, best_sad);
                    if s < best_sad {
                        best_sad = s;
                        best = (dx, dy);
                        moved = true;
                    }
                }
            }
            if !moved {
                break;
            }
        }
    }
    (best, best_sad)
}

/// The predicted vector (half pixels) as whole pixels.
fn pmv_full(p: (i32, i32)) -> (i32, i32) {
    (p.0 / 2, p.1 / 2)
}

/// How costly the macroblock's luma would be as intra: its spread around
/// its mean.
fn intra_cost(cur: &Frame, mb: Mb) -> u32 {
    let (x0, y0) = (mb.col * 16, mb.row * 16);
    let mut sum = 0u32;
    for y in 0..16 {
        for x in 0..16 {
            sum += cur.y[(y0 + y) * cur.w + x0 + x] as u32;
        }
    }
    let mean = (sum / 256) as i32;
    let mut s = 0u32;
    for y in 0..16 {
        for x in 0..16 {
            s += (cur.y[(y0 + y) * cur.w + x0 + x] as i32 - mean).unsigned_abs();
        }
    }
    s
}

/// macroblock_address_increment, with escapes past 33.
fn address_increment(b: &mut Bits, mut n: usize) {
    while n > 33 {
        b.code(ADDRESS_ESCAPE);
        n -= 33;
    }
    b.code(ADDRESS_INCREMENT[n.max(1) - 1]);
}

/// One component of a motion vector's difference from its prediction
/// (half pixels), as `motion_code` and `motion_residual` at `F_CODE`.
fn motion_component(b: &mut Bits, delta: i32) {
    let f = 1 << (F_CODE - 1);
    let range = 32 * f;
    let mut d = delta;
    if d < -range / 2 {
        d += range;
    } else if d >= range / 2 {
        d -= range;
    }
    if d == 0 {
        b.code(MOTION_CODE[0]);
        return;
    }
    let a = d.unsigned_abs() - 1;
    let code = (a / f as u32 + 1) as usize;
    b.code(MOTION_CODE[code.min(16)]);
    b.put(u32::from(d < 0), 1);
    b.put(a % f as u32, F_CODE - 1);
}

/// An intra block's DC: its size, then its difference's bits.
fn dc_differential(b: &mut Bits, diff: i32, luma: bool) {
    let size = 32 - diff.unsigned_abs().leading_zeros();
    let table = if luma { &DC_SIZE_LUMA } else { &DC_SIZE_CHROMA };
    b.code(table[size as usize]);
    if size > 0 {
        let bits = if diff > 0 {
            diff as u32
        } else {
            (diff + (1 << size) - 1) as u32
        };
        b.put(bits, size);
    }
}

/// A coefficient (`run` zeros before it), escaped where Table B-14 has
/// no code for it (MPEG-1's escape: the level in 8 bits, or 16 past
/// ±127); `first` writes (0, ±1) as a non-intra block's first.
fn coefficient(b: &mut Bits, run: usize, level: i32, first: bool, mpeg1: bool) {
    let mag = level.unsigned_abs();
    if first && run == 0 && mag == 1 {
        b.code("1");
        b.put(u32::from(level < 0), 1);
        return;
    }
    match tables::coefficient(run, mag) {
        Some(c) => {
            b.code(c);
            b.put(u32::from(level < 0), 1);
        }
        None => {
            b.code(ESCAPE);
            b.put(run as u32, 6);
            if !mpeg1 {
                b.put((level as u32) & 0xFFF, 12);
            } else if mag < 128 {
                b.put((level as u32) & 0xFF, 8);
            } else {
                b.put(if level < 0 { 0x80 } else { 0 }, 8);
                b.put((level as u32) & 0xFF, 8);
            }
        }
    }
}

/// An intra block's AC coefficients in zigzag order, then the end.
fn ac_coefficients(b: &mut Bits, levels: &[i32; 64], mpeg1: bool) {
    let mut run = 0;
    for &i in &ZIGZAG[1..] {
        let v = levels[i];
        if v == 0 {
            run += 1;
        } else {
            coefficient(b, run, v, false, mpeg1);
            run = 0;
        }
    }
    b.code(EOB);
}

/// A non-intra block's coefficients in zigzag order, then the end.
fn coefficients(b: &mut Bits, levels: &[i32; 64], mpeg1: bool) {
    let mut run = 0;
    let mut first = true;
    for &i in &ZIGZAG {
        let v = levels[i];
        if v == 0 {
            run += 1;
        } else {
            coefficient(b, run, v, first, mpeg1);
            first = false;
            run = 0;
        }
    }
    b.code(EOB);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            width: 64,
            height: 48,
            rate: (25, 1),
            bitrate: 2_000_000,
            max_bitrate: 4_000_000,
            gop: 12,
            aspect: 2,
            top_field_first: None,
            interlaced_sequence: false,
            mpeg1: false,
            b_frames: 0,
        }
    }

    /// A frame of a gradient with a square that moves.
    fn frame(n: usize) -> Vec<u8> {
        let (w, h) = (64, 48);
        let mut f = vec![0u8; w * h + w * h / 2];
        for y in 0..h {
            for x in 0..w {
                let inside = (x + 64 - n * 2) % 64 < 16 && (16..32).contains(&y);
                f[y * w + x] = if inside { 220 } else { (x * 2 + y) as u8 + 16 };
            }
        }
        for i in 0..w * h / 4 {
            f[w * h + 2 * i] = 100;
            f[w * h + 2 * i + 1] = 150;
        }
        f
    }

    #[test]
    fn a_gop_opens_with_its_headers_and_ps_follow() {
        let mut e = Encoder::new(settings());
        let first = e.encode(&frame(0), false).remove(0);
        assert!(first.keyframe);
        assert_eq!(&first.data[..4], [0, 0, 1, 0xB3]);
        // The size: 64 × 48, 4:3, 25 frames a second.
        assert_eq!(&first.data[4..8], [0x04, 0x00, 0x30, 0x23]);
        let next = e.encode(&frame(1), false).remove(0);
        assert!(!next.keyframe);
        assert_eq!(&next.data[..4], [0, 0, 1, 0x00]);
        // A P-picture of a small move is much smaller than the I.
        assert!(
            next.data.len() < first.data.len(),
            "{} {}",
            next.data.len(),
            first.data.len()
        );
        // Every slice is there: 3 rows.
        let slices = next
            .data
            .windows(4)
            .filter(|w| w[..3] == [0, 0, 1] && (1..=3).contains(&w[3]))
            .count();
        assert_eq!(slices, 3);
    }

    #[test]
    fn keys_and_gops_make_i_pictures() {
        let mut e = Encoder::new(Settings {
            gop: 3,
            ..settings()
        });
        let kinds: Vec<bool> = (0..7)
            .map(|n| e.encode(&frame(n), n == 5)[0].keyframe)
            .collect();
        assert_eq!(kinds, [true, false, false, true, false, true, false]);
    }

    #[test]
    fn an_mpeg1_stream_has_no_extensions() {
        let mut e = Encoder::new(Settings {
            mpeg1: true,
            ..settings()
        });
        let mut data = Vec::new();
        for n in 0..3 {
            data.extend(e.encode(&frame(n), false).into_iter().flat_map(|p| p.data));
        }
        assert!(
            !data.windows(4).any(|w| w == [0, 0, 1, 0xB5]),
            "no extensions"
        );
        // 64 × 48 at 4:3: square pixels (1), 25 frames a second (3).
        assert_eq!(&data[..8], [0, 0, 1, 0xB3, 0x04, 0x00, 0x30, 0x13]);
        // A P-picture's forward f_code in its header.
        let p = data
            .windows(6)
            .position(|w| w[..4] == [0, 0, 1, 0] && (w[5] >> 3) & 7 == 2)
            .expect("a P-picture");
        let header = u64::from_be_bytes(data[p + 4..p + 12].try_into().unwrap());
        assert_eq!((header >> (64 - 10 - 3 - 16 - 1 - 3)) & 7, F_CODE as u64);
        // Video CD's pixels.
        let vcd = |height, aspect| {
            pel_aspect(&Settings {
                width: 352,
                height,
                aspect,
                ..settings()
            })
        };
        assert_eq!((vcd(240, 2), vcd(288, 2)), (12, 8));
    }

    #[test]
    fn b_pictures_come_after_the_anchor_they_need() {
        let mut e = Encoder::new(Settings {
            b_frames: 2,
            gop: 7,
            ..settings()
        });
        let mut coded = Vec::new();
        for n in 0..9 {
            coded.extend(e.encode(&frame(n), false));
        }
        coded.extend(e.finish());
        // Each picture's type and temporal reference (from its header) and
        // its frame.
        let kinds: Vec<(u8, u32, u64)> = coded
            .iter()
            .map(|p| {
                let at = p.data.windows(4).position(|w| w == [0, 0, 1, 0]).unwrap();
                let (a, b) = (p.data[at + 4], p.data[at + 5]);
                ((b >> 3) & 7, ((a as u32) << 2) | (b as u32 >> 6), p.display)
            })
            .collect();
        assert_eq!(
            kinds,
            [
                (1, 0, 0),
                (2, 3, 3),
                (3, 1, 1),
                (3, 2, 2),
                (2, 6, 6),
                (3, 4, 4),
                (3, 5, 5),
                (1, 0, 7),
                (2, 1, 8)
            ]
        );
        // B-pictures of a small move cost less than the I-picture.
        assert!(coded[2].data.len() < coded[0].data.len() / 2);
    }

    #[test]
    fn mpeg1_escapes_take_8_or_16_bits() {
        let mut b = Bits::default();
        coefficient(&mut b, 0, 200, false, true);
        // The escape (000001), run 0, then 0x00 and 200.
        assert_eq!(b.finish(), [0x04, 0x00, 0x0C, 0x80]);
        let mut b = Bits::default();
        coefficient(&mut b, 0, -100, false, true);
        // -100 in 8 bits: 0x9C.
        assert_eq!(b.finish(), [0x04, 0x09, 0xC0]);
    }

    #[test]
    fn dc_differences_take_their_sizes() {
        let mut b = Bits::default();
        dc_differential(&mut b, -3, true);
        // Size 2 ("01"), then -3 + 3 = 0 in two bits.
        assert_eq!(b.finish(), [0b0100_0000]);
    }
}
