//! Tools › Selectively Prerender Video: the parts of the project that need
//! it drawn, one after another, into files the preview then reads (see
//! `bv_timeline::prerender`).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bv_core::{Tick, TimeRange};
use bv_timeline::{MediaInfo, Prerender, Project};

use crate::render::{FileFormat, RenderJob, RenderOptions, RenderSettings, VideoParams};

/// The file type of prerendered parts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PrerenderFormat {
    /// H.264 in MP4: decoded by the graphics card, plays the most smoothly.
    #[default]
    H264,
    /// ProRes 422 in MOV: the best quality, larger files.
    ProRes,
}

/// How to prerender.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PrerenderOptions {
    pub format: PrerenderFormat,
    /// "Stretch video to fill output frame size (do not letterbox)".
    pub stretch: bool,
    /// "Use project output rotation setting".
    pub rotate: bool,
}

/// How prerendering goes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PrerenderProgress {
    /// Parts done, of all.
    pub parts_done: usize,
    pub parts: usize,
    /// Frames of the part being drawn.
    pub frames_done: u64,
    pub frames: u64,
    pub finished: bool,
    pub error: Option<String>,
}

pub(crate) struct PrerenderJob {
    project: Arc<Project>,
    options: PrerenderOptions,
    queue: VecDeque<(TimeRange, PathBuf)>,
    current: Option<(TimeRange, PathBuf, RenderJob)>,
    /// The parts drawn, for the project to keep.
    made: Vec<Prerender>,
    parts: usize,
    error: Option<String>,
}

/// The name of a part's file: the project, what it shows and where.
fn part_file(folder: &Path, p: &Project, range: TimeRange, options: PrerenderOptions) -> PathBuf {
    let frame = p.settings.frame_rate.frame_at(range.start);
    let ext = match options.format {
        PrerenderFormat::H264 => "mp4",
        PrerenderFormat::ProRes => "mov",
    };
    folder.join(format!(
        "prerender_{:016x}_{frame:07}.{ext}",
        p.fingerprint(range)
    ))
}

impl crate::Engine {
    /// Starts prerendering the parts of `range` that need it into `folder`.
    /// The number of parts (0: nothing needs it).
    pub fn prerender(
        &mut self,
        range: TimeRange,
        folder: &Path,
        options: PrerenderOptions,
    ) -> usize {
        let parts = self.project.prerender_parts(range);
        if parts.is_empty() {
            return 0;
        }
        let _ = std::fs::create_dir_all(folder);
        let queue: VecDeque<_> = parts
            .iter()
            .map(|&r| (r, part_file(folder, &self.project, r, options)))
            .collect();
        self.prerender = Some(PrerenderJob {
            project: self.project.clone(),
            options,
            parts: queue.len(),
            queue,
            current: None,
            made: Vec::new(),
            error: None,
        });
        parts.len()
    }

    /// How prerendering goes (it goes on through these calls), and the
    /// parts done, once it finishes, for the project to keep.
    pub fn prerender_progress(&mut self) -> Option<(PrerenderProgress, Vec<Prerender>)> {
        let repaint = self.repaint.clone();
        let accel = self.accel.clone();
        let job = self.prerender.as_mut()?;
        // The part being drawn: done?
        if let Some((range, file, render)) = &job.current {
            let p = render.progress();
            if !p.finished {
                return Some((
                    PrerenderProgress {
                        parts_done: job.made.len(),
                        parts: job.parts,
                        frames_done: p.done,
                        frames: p.total,
                        finished: false,
                        error: None,
                    },
                    Vec::new(),
                ));
            }
            match p.error {
                Some(e) => {
                    job.error = Some(e);
                    job.queue.clear();
                }
                None if p.cancelled => job.queue.clear(),
                None => job.made.push(Prerender {
                    range: *range,
                    file: file.to_string_lossy().to_string(),
                    fingerprint: job.project.fingerprint(*range),
                }),
            }
            job.current = None;
        }
        // The next part.
        if let Some((range, file)) = job.queue.pop_front() {
            let s = &job.project.settings;
            let (width, height) = if job.options.rotate {
                s.output_size()
            } else {
                (s.width, s.height)
            };
            let format = match job.options.format {
                PrerenderFormat::H264 => FileFormat::AvcMp4,
                PrerenderFormat::ProRes => FileFormat::ProRes,
            };
            let settings = RenderSettings {
                format,
                path: file.clone(),
                range,
                video: Some(VideoParams {
                    width,
                    height,
                    frame_rate: s.frame_rate,
                    // Generous: these stand in for the project itself.
                    bitrate: (width as f64 * height as f64 * s.frame_rate.as_f64() * 0.4) as u32,
                    keyframe_interval: (s.frame_rate.as_f64() / 2.0).round().max(1.0) as u32,
                    ..VideoParams::default()
                }),
                audio: None,
                options: RenderOptions {
                    stretch: job.options.stretch,
                    use_rotation: job.options.rotate,
                    ..RenderOptions::default()
                },
                project_file: None,
            };
            let render = RenderJob::start(job.project.clone(), settings, repaint, accel);
            job.current = Some((range, file, render));
            return Some((
                PrerenderProgress {
                    parts_done: job.made.len(),
                    parts: job.parts,
                    ..PrerenderProgress::default()
                },
                Vec::new(),
            ));
        }
        // All done.
        let job = self.prerender.take()?;
        Some((
            PrerenderProgress {
                parts_done: job.made.len(),
                parts: job.parts,
                finished: true,
                error: job.error,
                ..PrerenderProgress::default()
            },
            job.made,
        ))
    }

    /// Whether parts are being prerendered.
    pub fn is_prerendering(&self) -> bool {
        self.prerender.is_some()
    }

    /// Where the project's prerendered parts still show what it does: the
    /// preview plays them from their files, and the timeline marks them.
    pub fn prerendered(&mut self) -> &[TimeRange] {
        if self.prerendered.0 != Some(self.generation) {
            let p = &self.project;
            let valid = p
                .prerenders
                .iter()
                .filter(|part| {
                    p.fingerprint(part.range) == part.fingerprint && Path::new(&part.file).exists()
                })
                .map(|part| part.range)
                .collect();
            self.prerendered = (Some(self.generation), valid);
        }
        &self.prerendered.1
    }

    /// Stops prerendering (the parts done stay).
    pub fn cancel_prerender(&mut self) {
        if let Some(job) = &mut self.prerender {
            job.queue.clear();
            if let Some((_, _, render)) = &job.current {
                render.cancel();
            }
        }
    }
}

/// The project a prerendered part's file plays as: the file alone, where
/// the part is.
pub(crate) fn part_project(project: &Project, part: &Prerender) -> Option<Project> {
    let mut info: MediaInfo = bv_media::probe_file(&part.file).ok()?;
    let mut settings = project.settings;
    // The file holds the frames as they left ACES's view: shown as they
    // are, not through it again.
    settings.color.view = None;
    let v = info.video?;
    if (v.width, v.height) != (settings.width, settings.height) {
        // Drawn turned by the output rotation: shown as it is.
        settings.width = v.width;
        settings.height = v.height;
        settings.rotation = bv_timeline::OutputRotation::default();
    }
    let mut p = Project::new(settings);
    let track = p.tracks[0].id;
    info.effects.clear();
    let id = p.add_media(info);
    let clip = p
        .insert_media_range(
            id,
            part.range.start,
            Tick::ZERO,
            part.range.duration,
            Some(track),
            None,
        )
        .ok()?[0];
    // An id of its own: the preview's readers are kept by media.
    let own = bv_timeline::MediaId(
        part.file.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
            (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)
        }) | 1 << 63,
    );
    p.media[0].id = own;
    p.clip_mut(clip)?.media = own;
    Some(p)
}
