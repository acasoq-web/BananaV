//! A DVD-Video folder (Tools › Burn Disc › DVD renders one before burning
//! it): BananaV's MPEG-2 pictures (`bv_mpeg2`, DVD's GOPs) and the mix as
//! linear PCM in BananaV's video objects (`bv_dvd`), a chapter at each
//! marker, then the information files; `VIDEO_TS` and an empty `AUDIO_TS`
//! under the settings' path.

use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use bv_dvd::{Aspect, VobWriter};
use bv_timeline::Project;

use super::{Output, RenderSettings};

pub(super) struct DvdOut {
    vob: Option<VobWriter<BufWriter<File>>>,
    video: bv_mpeg2::Encoder,
    rate: bv_core::FrameRate,
    aspect: Aspect,
    /// The frames that begin chapters (the markers').
    chapters: Vec<u64>,
    /// Pictures written: the next one's decoding time.
    coded: u64,
    folder: PathBuf,
}

pub(super) fn open(project: &Project, s: &RenderSettings) -> Result<Box<dyn Output>, String> {
    let mut v = s.video.ok_or("no video settings")?;
    let ntsc = v.height == 480;
    let folder = s.path.join("VIDEO_TS");
    std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    std::fs::create_dir_all(s.path.join("AUDIO_TS"))
        .map_err(|e| format!("{}: {e}", s.path.display()))?;
    // DVD's limits: 9.8 Mbit/s with the sound's 1.5.
    v.max_bitrate = v.max_bitrate.clamp(v.bitrate.min(8_000_000), 8_000_000);
    v.bitrate = v.bitrate.min(v.max_bitrate);
    // DVD's GOPs: at most 18 fields NTSC, 15 PAL.
    // The disc's shape: the Burn DVD dialog's, or the project's.
    let wide = s
        .options
        .dvd_wide
        .unwrap_or_else(|| super::mpeg::project_aspect(project) == 3);
    let gop = if ntsc { 15 } else { 12 };
    let video = super::mpeg::video_encoder(project, &v, gop, Some(if wide { 3 } else { 2 }), false);
    let aspect = if wide { Aspect::Wide } else { Aspect::Standard };
    let chapters = project
        .markers
        .iter()
        .filter(|m| m.time > s.range.start && m.time < s.range.end())
        .map(|m| v.frame_rate.frame_at(m.time - s.range.start).max(0) as u64)
        .collect();
    let file = File::create(folder.join("VTS_01_1.VOB"))
        .map_err(|e| format!("{}: {e}", folder.display()))?;
    let rate = s.audio.map_or(48_000, |a| a.sample_rate);
    let mut vob = VobWriter::new(BufWriter::with_capacity(1 << 20, file), ntsc, rate);
    // The pictures show a frame after their places (B-pictures): the
    // sound as late.
    let frame = v.frame_rate.den as u64 * 90_000 / v.frame_rate.num.max(1) as u64;
    vob.delay_sound(frame);
    Ok(Box::new(DvdOut {
        vob: Some(vob),
        video,
        rate: v.frame_rate,
        aspect,
        chapters,
        coded: 0,
        folder,
    }))
}

impl DvdOut {
    /// Frame `f`'s time (90 kHz).
    fn at(&self, f: u64) -> u64 {
        f * self.rate.den as u64 * 90_000 / self.rate.num.max(1) as u64
    }

    /// Coded pictures into the video objects, timed as `mpeg.rs` times
    /// them; a chapter's I-picture opens its cell.
    fn write_pictures(&mut self, pictures: Vec<bv_mpeg2::Picture>) -> Result<(), String> {
        for p in pictures {
            let pts = self.at(p.display + 1);
            let duration = self.at(p.display + 2) - pts;
            let dts = self.at(self.coded);
            self.coded += 1;
            let chapter = p.keyframe && self.chapters.contains(&p.display);
            let vob = self.vob.as_mut().ok_or("closed")?;
            if chapter {
                vob.chapter();
            }
            vob.video(&p.data, pts, dts, duration, p.keyframe)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

impl Output for DvdOut {
    fn packed_420(&self) -> Option<bool> {
        Some(false)
    }

    fn video_packed(&mut self, data: &[u8], n: u64) -> Result<(), String> {
        let pictures = self.video.encode(data, self.chapters.contains(&n));
        self.write_pictures(pictures)
    }

    fn audio(&mut self, planar: &[Vec<f32>]) -> Result<(), String> {
        let n = planar.first().map_or(0, Vec::len);
        let mut pcm = Vec::with_capacity(2 * n);
        for i in 0..n {
            // Stereo: a mono mix goes to both sides.
            let l = planar[0][i];
            let r = planar.get(1).map_or(l, |c| c[i]);
            for v in [l, r] {
                pcm.push((v.clamp(-1.0, 1.0) * 32_767.0).round() as i16);
            }
        }
        let vob = self.vob.as_mut().ok_or("closed")?;
        vob.audio(&pcm).map_err(|e| e.to_string())
    }

    fn encoder_name(&self) -> String {
        "BananaV MPEG-2 (DVD-Video)".into()
    }

    fn bytes(&self) -> u64 {
        0
    }

    fn finish(mut self: Box<Self>) -> Result<Vec<PathBuf>, String> {
        let rest = self.video.finish();
        self.write_pictures(rest)?;
        let vob = self.vob.take().ok_or("closed")?;
        let (_, title) = vob.finish().map_err(|e| e.to_string())?;
        if title.vobus.is_empty() {
            return Err("no frames to put on the disc".into());
        }
        bv_dvd::finish_folder(&self.folder, &title, self.aspect).map_err(|e| e.to_string())?;
        Ok(vec![self.folder.clone()])
    }
}
