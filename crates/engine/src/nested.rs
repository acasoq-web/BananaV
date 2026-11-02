//! Nested projects' pictures and sound (`bv_media`'s nested readers): the
//! inner project drawn on the processor — with its own media pool and
//! plug-in effects, over a transparent background, so a lower third keeps
//! what shows through it — and its master bus mixed and folded down to
//! stereo, as the outer project reads them (VEGAS builds an audio proxy of
//! a nested project; here its sound is mixed live, so there is nothing to
//! build or rebuild). Its prerendered video (Selectively Prerender Video)
//! is used where it still shows what the project does, as VEGAS's Help
//! says ("Nesting projects").

use std::sync::Arc;

use bv_core::{AudioBuffer, ChannelLayout, ImageRgbaF32, SampleRate, Tick};
use bv_media::{AudioReader, NestedReaders, Result, VideoReader};
use bv_mixer::{AudioPool, Downmix};
use bv_render::VideoPool;
use bv_timeline::Project;
use bv_timeline::scene::Plugins;

use crate::plugins::{OfxEffects, Purpose};

/// Lets `bv_media` read nested projects through the readers here.
pub(crate) fn register() {
    bv_media::set_nested_readers(NestedReaders {
        video: open_video,
        audio: open_audio,
    });
}

fn open_video(project: Project) -> Result<Box<dyn VideoReader>> {
    Ok(Box::new(NestedVideo {
        project,
        pool: VideoPool::new(),
        plugins: OfxEffects::new(Purpose::File),
        parts: std::collections::HashMap::new(),
    }))
}

fn open_audio(project: Project) -> Result<Box<dyn AudioReader>> {
    Ok(Box::new(NestedAudio {
        project,
        pool: AudioPool::new(),
    }))
}

/// A nested project's pictures.
struct NestedVideo {
    project: Project,
    pool: VideoPool,
    plugins: Arc<OfxEffects>,
    /// Its prerendered parts (by their place): the project playing each,
    /// when it still shows what the project does and its file is there.
    parts: std::collections::HashMap<usize, Option<Arc<Project>>>,
}

/// The size to draw a `w` × `h` output at so it still covers `min_w` ×
/// `min_h` (never larger than the output; even sides).
fn covering((w, h): (u32, u32), min_w: u32, min_h: u32) -> (u32, u32) {
    if min_w == 0 || min_h == 0 || min_w >= w || min_h >= h {
        return (w, h);
    }
    let k = (min_w as f64 / w as f64).max(min_h as f64 / h as f64);
    let side = |v: u32| (((v as f64 * k - 1e-6).ceil() as u32).max(2).div_ceil(2) * 2).min(v);
    (side(w), side(h))
}

impl NestedVideo {
    /// The project playing the prerendered part that shows `t`, when one
    /// still shows what the project does.
    fn prerendered(&mut self, t: Tick) -> Option<Arc<Project>> {
        let project = &self.project;
        let (i, part) = project
            .prerenders
            .iter()
            .enumerate()
            .find(|(_, p)| p.range.contains(t))?;
        self.parts
            .entry(i)
            .or_insert_with(|| {
                let valid = project.fingerprint(part.range) == part.fingerprint
                    && std::path::Path::new(&part.file).exists();
                valid
                    .then(|| crate::prerender::part_project(project, part).map(Arc::new))
                    .flatten()
            })
            .clone()
    }

    /// The frame at `t`, `size` large: the video bus's motion blur (and
    /// supersampling) as the project asks — or its prerendered part's.
    fn draw(&mut self, t: Tick, size: (u32, u32)) -> ImageRgbaF32 {
        if let Some(part) = self.prerendered(t) {
            return bv_render::render_output_with(
                &part,
                t,
                size,
                &mut self.pool,
                [0.0; 4],
                None,
                Some(Plugins(&*self.plugins)),
            );
        }
        let samples = self
            .project
            .motion_blur_at(t)
            .unwrap_or_else(|| vec![(t, 1.0)]);
        let mut out: Option<ImageRgbaF32> = None;
        for (at, w) in samples {
            let img = bv_render::render_output_with(
                &self.project,
                at,
                size,
                &mut self.pool,
                [0.0; 4],
                None,
                Some(Plugins(&*self.plugins)),
            );
            match &mut out {
                None if w == 1.0 => out = Some(img),
                None => {
                    let mut first = img;
                    for px in &mut first.data {
                        *px = px.map(|v| v * w);
                    }
                    out = Some(first);
                }
                Some(sum) => {
                    for (s, px) in sum.data.iter_mut().zip(&img.data) {
                        for c in 0..4 {
                            s[c] += px[c] * w;
                        }
                    }
                }
            }
        }
        out.unwrap_or_else(|| ImageRgbaF32::transparent(size.0, size.1))
    }
}

impl VideoReader for NestedVideo {
    fn frame_at(&mut self, t: Tick) -> Result<Arc<ImageRgbaF32>> {
        self.frame_at_scaled(t, 0, 0)
    }

    fn frame_at_scaled(&mut self, t: Tick, min_w: u32, min_h: u32) -> Result<Arc<ImageRgbaF32>> {
        let size = covering(self.project.settings.output_size(), min_w, min_h);
        Ok(Arc::new(self.draw(t, size)))
    }
}

/// A nested project's sound: its master bus, in stereo.
struct NestedAudio {
    project: Project,
    pool: AudioPool,
}

impl AudioReader for NestedAudio {
    fn sample_rate(&self) -> SampleRate {
        self.project.settings.sample_rate
    }

    fn channels(&self) -> usize {
        2
    }

    fn read(&mut self, start: i64, frames: usize) -> Result<AudioBuffer> {
        let mut buf = bv_mixer::mix(&self.project, start, frames, &mut self.pool);
        bv_mixer::downmix(&mut buf.channels, buf.layout, Downmix::Stereo);
        match buf.channels.len() {
            0 => buf.channels = vec![vec![0.0; frames]; 2],
            1 => buf.channels.push(buf.channels[0].clone()),
            _ => buf.channels.truncate(2),
        }
        buf.layout = ChannelLayout::Stereo;
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A part prerendered (a blue picture standing in for its file) shows
    /// instead of the project (red) while the project still shows what it
    /// did then; once it changes, the project again.
    #[test]
    fn a_nested_project_plays_its_prerendered_parts() {
        use bv_core::TimeRange;
        use bv_timeline::{Generator, MediaInfo, ProjectSettings};
        let dir = std::env::temp_dir().join(format!("bv-nested-prerender-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("part.png");
        let blue: Vec<u8> = [0u8, 0, 255, 255].repeat(64 * 36);
        std::fs::write(&file, bv_png::encode_rgba8(64, 36, &blue, 1)).unwrap();
        let mut settings = ProjectSettings::default();
        (settings.width, settings.height) = (64, 36);
        let mut p = Project::new(settings);
        let v = p.tracks[0].id;
        let red = p.add_media(MediaInfo::generated(
            Generator::SolidColor {
                rgba: [1.0, 0.0, 0.0, 1.0],
            },
            &p.settings,
        ));
        let c = p.insert_media(red, Tick::ZERO, Some(v), None).unwrap()[0];
        let range = TimeRange::new(Tick::ZERO, Tick::from_seconds(1));
        p.prerenders.push(bv_timeline::Prerender {
            range,
            file: file.to_string_lossy().to_string(),
            fingerprint: p.fingerprint(range),
        });
        let middle = |p: Project| {
            let mut reader = open_video(p).unwrap();
            let img = reader.frame_at(Tick::from_seconds_f64(0.5)).unwrap();
            img.data[(18 * 64 + 32) as usize]
        };
        let shown = middle(p.clone());
        assert!(shown[2] > 0.5 && shown[0] < 0.1, "{shown:?}");
        // An edit there: the project itself.
        p.clip_mut(c).unwrap().level = 0.5;
        let shown = middle(p);
        assert!(shown[0] > 0.1 && shown[2] < 0.1, "{shown:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn previews_draw_a_nested_project_just_large_enough() {
        assert_eq!(covering((1920, 1080), 0, 0), (1920, 1080));
        assert_eq!(covering((1920, 1080), 480, 270), (480, 270));
        // The larger of the two scales, rounded up to even sides.
        assert_eq!(covering((1920, 1080), 481, 100), (482, 272));
        assert_eq!(covering((1920, 1080), 4000, 3000), (1920, 1080));
    }
}
