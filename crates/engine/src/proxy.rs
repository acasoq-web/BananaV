//! Proxies: smaller H.264 copies of large media files (more than 4K, or too
//! heavy to decode in time) that previews read instead (Project Media ›
//! Create Video Proxy; Options › Preferences › Video: "Automatically create
//! video proxies for greater than 4K resolution media"). Renders always
//! read the files themselves.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use bv_core::{Tick, TimeRange};
use bv_timeline::{MediaInfo, MediaSource, Project, VideoStreamInfo};

use crate::render::{FileFormat, RenderJob, RenderOptions, RenderSettings, VideoParams};

/// A proxy being made.
pub(crate) struct ProxyJob {
    original: String,
    target: PathBuf,
    part: PathBuf,
    job: RenderJob,
}

/// How a proxy being made goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProxyProgress {
    /// The media file it is of.
    pub original: String,
    pub done: u64,
    pub total: u64,
}

/// A proxy finished: of which media file, or why it could not be made.
pub type ProxyDone = (String, Result<(), String>);

/// Where a media file's proxy goes: next to it (`clip.mp4.bvproxy.mp4`).
pub fn proxy_path(original: &str) -> PathBuf {
    PathBuf::from(format!("{original}.bvproxy.mp4"))
}

/// The same name in `folder` (when the media's own folder takes no files).
fn proxy_in(folder: &Path, original: &str) -> PathBuf {
    let name = Path::new(original)
        .file_name()
        .map_or_else(|| "media".into(), |n| n.to_string_lossy().to_string());
    // Two files of one name in different folders get different proxies.
    let hash = original.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)
    });
    folder.join(format!("{name}.{hash:016x}.bvproxy.mp4"))
}

/// A proxy's frame size: the media's shape, no taller than 720 lines.
pub(crate) fn proxy_size(v: &VideoStreamInfo) -> (u32, u32) {
    let scale = (720.0 / v.height as f32).min(1.0);
    let even = |x: f32| ((x.round() as u32) & !1).max(2);
    (even(v.width as f32 * scale), even(v.height as f32 * scale))
}

/// Whether a media file is larger than 4K.
pub fn larger_than_4k(info: &MediaInfo) -> bool {
    info.video
        .is_some_and(|v| v.width as u64 * v.height as u64 > 3840 * 2160)
}

/// Whether this computer decodes the media's frames as fast as they play
/// (a few frames are timed; blocking).
pub fn decodes_in_time(info: &MediaInfo) -> bool {
    let Some(v) = info.video else {
        return true;
    };
    let Ok(mut r) = bv_media::open_video(info) else {
        return true;
    };
    let rate = v.frame_rate;
    // The first frame opens the decoder: not counted.
    let _ = r.frame_at(Tick::ZERO);
    let start = Instant::now();
    let n = 8;
    for k in 1..=n {
        let _ = r.frame_at(rate.frame_start(k));
    }
    let per_frame = start.elapsed().as_secs_f64() / n as f64;
    per_frame < rate.frame_duration().as_seconds_f64() * 0.8
}

impl crate::Engine {
    /// Starts making the proxy of a video file (next to it, or in
    /// `fallback` when its folder takes no files). `false` when the media
    /// is not a video file or its proxy is being made already.
    pub fn create_proxy(&mut self, info: &MediaInfo, fallback: &Path) -> bool {
        let (MediaSource::File { path }, Some(v), None) = (&info.source, info.video, info.sequence)
        else {
            return false;
        };
        if info.still || self.proxy_jobs.iter().any(|j| j.original == *path) {
            return false;
        }
        let mut target = proxy_path(path);
        let writable = target
            .parent()
            .is_some_and(|dir| std::fs::File::create(dir.join(".bananav-proxy-probe")).is_ok());
        if let Some(dir) = target.parent() {
            let _ = std::fs::remove_file(dir.join(".bananav-proxy-probe"));
        }
        if !writable {
            let _ = std::fs::create_dir_all(fallback);
            target = proxy_in(fallback, path);
        }
        let part = target.with_extension("part.mp4");
        let (width, height) = proxy_size(&v);
        let mut settings = self.project.settings;
        settings.width = v.width;
        settings.height = v.height;
        settings.frame_rate = v.frame_rate;
        settings.pixel_aspect = v.pixel_aspect;
        let mut p = Project::new(settings);
        let track = p.tracks[0].id;
        let mut media = info.clone();
        media.effects.clear();
        let id = p.add_media(media);
        if p.insert_media_range(id, Tick::ZERO, Tick::ZERO, info.duration, Some(track), None)
            .is_err()
        {
            return false;
        }
        let render = RenderSettings {
            format: FileFormat::AvcMp4,
            path: part.clone(),
            range: TimeRange::new(Tick::ZERO, info.duration),
            video: Some(VideoParams {
                width,
                height,
                frame_rate: v.frame_rate,
                // Light, and a keyframe every half second: quick to seek in.
                bitrate: (width as f64 * height as f64 * v.frame_rate.as_f64() * 0.08) as u32,
                keyframe_interval: (v.frame_rate.as_f64() / 2.0).round().max(1.0) as u32,
                ..VideoParams::default()
            }),
            audio: None,
            options: RenderOptions::default(),
            project_file: None,
        };
        let job = RenderJob::start(
            Arc::new(p),
            render,
            self.repaint.clone(),
            self.accel.clone(),
        );
        self.proxy_jobs.push(ProxyJob {
            original: path.clone(),
            target,
            part,
            job,
        });
        true
    }

    /// The proxies being made, and those finished since the last call (each
    /// done one is read by previews from now on).
    pub fn proxies(&mut self) -> (Vec<ProxyProgress>, Vec<ProxyDone>) {
        let mut going = Vec::new();
        let mut done = Vec::new();
        let mut kept = Vec::new();
        for j in std::mem::take(&mut self.proxy_jobs) {
            let p = j.job.progress();
            if !p.finished {
                going.push(ProxyProgress {
                    original: j.original.clone(),
                    done: p.done,
                    total: p.total,
                });
                kept.push(j);
                continue;
            }
            let result = match p.error {
                Some(e) => Err(e),
                None if p.cancelled => Err("cancelled".into()),
                None => std::fs::rename(&j.part, &j.target)
                    .map_err(|e| e.to_string())
                    .map(|()| {
                        bv_media::use_proxy(&j.original, &j.target.to_string_lossy());
                    }),
            };
            if result.is_err() {
                let _ = std::fs::remove_file(&j.part);
            }
            done.push((j.original, result));
        }
        self.proxy_jobs = kept;
        (going, done)
    }

    /// Finds a media file's proxy made before (next to it or in
    /// `fallback`, newer than the file) and has previews read it.
    pub fn find_proxy(info: &MediaInfo, fallback: &Path) -> bool {
        let MediaSource::File { path } = &info.source else {
            return false;
        };
        let changed = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        let Some(media) = changed(Path::new(path)) else {
            return false;
        };
        let found = [proxy_path(path), proxy_in(fallback, path)]
            .into_iter()
            .find(|p| changed(p).is_some_and(|t| t >= media));
        match found {
            Some(p) => {
                bv_media::use_proxy(path, &p.to_string_lossy());
                true
            }
            None => false,
        }
    }

    /// Deletes a media file's proxy (previews read the file again).
    pub fn delete_proxy(info: &MediaInfo) {
        let MediaSource::File { path } = &info.source else {
            return;
        };
        if let Some(p) = bv_media::proxy_of(path) {
            let _ = std::fs::remove_file(p);
        }
        bv_media::forget_proxy(path);
    }

    /// Whether a media file has a proxy previews read.
    pub fn has_proxy(info: &MediaInfo) -> bool {
        matches!(&info.source, MediaSource::File { path } if bv_media::proxy_of(path).is_some())
    }
}
