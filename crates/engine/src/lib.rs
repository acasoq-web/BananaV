//! Engine do BananaV: liga o modelo do projeto à saída de áudio e ao monitor.
//!
//! - **Prévia**: uma thread compõe o quadro pedido pela interface e o entrega
//!   em RGBA 8 bits, sem travar a interface.
//! - **Playback**: o relógio de áudio é o mestre. Uma thread mistura o áudio à
//!   frente num buffer; a saída de áudio consome o buffer e conta as amostras
//!   tocadas; a posição do playback é derivada dessa contagem, e o vídeo segue.

mod accel;
mod audio_sync;
mod beat_detection;
mod capture;
mod ducking;
mod live;
mod loudness;
mod media_preview;
mod metronome;
mod motion_analysis;
mod motion_tracking;
mod nested;
mod nonrealtime;
mod playback;
pub mod plugins;
mod prerender;
mod preview;
mod proxy;
mod realtime;
mod record;
mod render;
mod resample;
mod sample;
mod scenes;
mod source_view;
mod thumbs;
mod track_mix;

use std::sync::Arc;

use bv_core::Tick;
use bv_timeline::Project;

pub use accel::{DecodeChoice, GpuChoice, GpuStatus, gpu_adapters};
pub use audio_sync::{AudioSyncProgress, SyncOutcome};
pub use beat_detection::BeatProgress;
pub use bv_dvd::write_image as write_dvd_image;
/// The Color Grading effect's parameters, its wheels' points and its
/// curves' text, for the Color Grading window.
pub use bv_effects::color_grading;
pub use bv_effects::color_grading::{
    APPLE_LOG_TO_LINEAR, APPLE_LOG_TO_REC709, Picker as ColorGradingPicker,
};
pub use bv_effects::stabilization;
pub use bv_gpu::{Adapter as GpuAdapter, AdapterKind as GpuKind};
pub use bv_media::{AUDIO_EXTENSIONS, IMAGE_EXTENSIONS, MediaError, VIDEO_EXTENSIONS};
pub use bv_mixer::Downmix;
pub use bv_mixer::beats::{Beat, kept as beats_kept, threshold as beat_threshold};
pub use bv_mixer::loudness::{LiveLoudness, LoudnessReport};
pub use bv_mixer::meters;
pub use bv_mixer::modern_eq;
pub use bv_track as track;
pub use capture::{
    CameraMode, Capture, CaptureAudioCodec, CaptureDefaults, CaptureQuality, CaptureSource,
    CaptureVideoCodec, Captured, SourceKind, SourceLive, audio_sources, camera_modes,
    camera_sources, display_sources, fps_text, window_sources,
};
pub use ducking::DuckProgress;
pub use live::LiveRoute;
pub use loudness::LoudnessProgress;
pub use metronome::{Click, Clicks};
pub use motion_analysis::{AnalysisKind, AnalysisProgress, FeatureTracks, MotionSetup};
pub use motion_tracking::{TrackDirection, TrackRequest, TrackingProgress};
pub use nonrealtime::{NrtFormat, NrtProgress, trim_sound};
pub use playback::{AudioStatus, PlayOptions};
pub use prerender::{PrerenderFormat, PrerenderOptions, PrerenderProgress};
pub use preview::{Channel, PreviewFrame, PreviewLook};
pub use proxy::{ProxyDone, ProxyProgress, decodes_in_time, larger_than_4k, proxy_path};
pub use realtime::{RealTimeDone, RealTimeProgress};
pub use record::{InputDevice, RecordTake, Recorded, RecordingWave, input_devices};
pub use render::{
    AudioParams, Availability, Category, Encoder, FileFormat, ProResProfile, RateMode,
    RenderOptions, RenderProgress, RenderSettings, VideoParams, sequence_path,
};
pub use sample::{
    compositor_sample, effect_chain_cube, effect_sample, file_picture, frame_picture,
    night_picture, ripple_map, sample_picture, transition_sample,
};
pub use scenes::SceneProgress;
pub use source_view::SourcePicture;
pub use thumbs::{Thumbnail, WaveChunk, Waveform};

use bv_core::TimeRange;
pub use bv_media::{
    FileDescription, FileReader, MarkerStore, MediaStamps, TrackFacts, can_save_markers,
    describe_file, forget_local_copies, is_project, markers_path, nested_projects,
    psd_layer_blends, psd_layers, read_markers, read_project, same_file, save_markers,
    stamps as media_stamps, use_local_copy,
};
pub use preview::BuildProgress;

/// The size of the preview's frames in `max_w` × `max_h`: the output (turned
/// by its rotation), never larger than it.
fn preview_size(s: &bv_timeline::ProjectSettings, max_w: u32, max_h: u32) -> (u32, u32) {
    let (ow, oh) = s.output_size();
    let scale = (max_w as f32 / ow as f32)
        .min(max_h as f32 / oh as f32)
        .min(1.0);
    (
        ((ow as f32 * scale).round() as u32).max(2),
        ((oh as f32 * scale).round() as u32).max(2),
    )
}

/// A side of Expanded Edit Mode's split preview: the project it draws (one
/// event alone), made for a project generation.
#[derive(Clone)]
struct SplitSide {
    generation: u64,
    clip: bv_timeline::ClipId,
    project: Arc<Project>,
    /// Tells the projects of a side apart (the renderer keeps frames by it).
    number: u64,
}

/// `project` with only event `clip`, shown whatever the mute and solo
/// switches say, without its fades: what the event itself shows.
fn alone(project: &Project, clip: bv_timeline::ClipId) -> Project {
    let mut p = project.clone();
    p.prerenders.clear();
    p.mute_all_video = false;
    for t in &mut p.tracks {
        t.clips.retain(|c| c.id == clip);
        t.muted = false;
        t.solo = false;
        for c in &mut t.clips {
            c.muted = false;
            c.fade_in = Tick::ZERO;
            c.fade_out = Tick::ZERO;
        }
    }
    p
}

/// Chamado quando há algo novo para mostrar (quadro renderizado, fim do playback).
pub type RepaintFn = Arc<dyn Fn() + Send + Sync>;

pub struct Engine {
    project: Arc<Project>,
    generation: u64,
    preview: preview::PreviewRenderer,
    playback: Option<playback::Playback>,
    /// A media playing alone through the preview bus.
    media_preview: Option<media_preview::MediaPreview>,
    render: Option<render::RenderJob>,
    trimmer: preview::PreviewRenderer,
    /// The Trimmer's project (one event of its media) and the key of what
    /// it was made of: made again only when that changes.
    trimmer_project: std::sync::Mutex<Option<(u64, Arc<Project>)>>,
    /// Multicamera editing's view: every take in tiles.
    multicam: preview::PreviewRenderer,
    thumbs: thumbs::Thumbs,
    /// The Pan/Crop window's picture, decoded on its own.
    source_view: source_view::SourceView,
    scenes: Option<scenes::SceneJob>,
    audio_sync: Option<audio_sync::AudioSyncJob>,
    beats: Option<beat_detection::BeatJob>,
    ducking: Option<ducking::DuckJob>,
    nonrealtime: Option<nonrealtime::NrtJob>,
    loudness: Option<loudness::LoudnessJob>,
    repaint: RepaintFn,
    audio_status: AudioStatus,
    /// The audio output kept open between playbacks (see `hold_audio`).
    audio_out: Option<Arc<playback::Output>>,
    /// When opening it last failed: not tried again for a while.
    audio_failed: Option<std::time::Instant>,
    /// Proxies being made.
    proxy_jobs: Vec<proxy::ProxyJob>,
    /// Tools › Selectively Prerender Video going on.
    prerender: Option<prerender::PrerenderJob>,
    /// Recording into the armed tracks.
    recorder: Option<record::Recorder>,
    /// Live input: monitoring and input busses heard (`live.rs`).
    live: Option<live::Live>,
    live_error: Option<String>,
    /// File › Real-Time Render going on.
    real_time: Option<realtime::RealTimeRender>,
    /// The prerendered parts still valid, for the project generation.
    prerendered: (Option<u64>, Vec<bv_core::TimeRange>),
    /// Expanded Edit Mode's split preview: a renderer for each side of the
    /// edit point (made when first asked for), the project each draws (its
    /// event alone: the project generation, the event, the project and its
    /// own number).
    split: Option<Box<[preview::PreviewRenderer; 2]>>,
    split_projects: [Option<SplitSide>; 2],
    split_count: u64,
    /// Motion tracking going on (`motion_tracking.rs`).
    tracking: Option<motion_tracking::Tracking>,
    /// Video Stabilization's motion analysis going on (`motion_analysis.rs`).
    analysis: Option<motion_analysis::Analysis>,
    /// The graphics card compositing video, if any.
    accel: Arc<accel::Accel>,
    /// How the Video Preview draws its frames (its quality, Split-Screen
    /// View, the channel shown).
    preview_look: PreviewLook,
    /// The project with its video effects bypassed, for Split-Screen View,
    /// and the generation it was made for.
    bypassed: std::sync::Mutex<Option<(u64, Arc<Project>)>>,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("generation", &self.generation)
            .field("playing", &self.is_playing())
            .finish()
    }
}

impl Engine {
    pub fn new(repaint: RepaintFn) -> Engine {
        plugins::register_own();
        nested::register();
        let accel = accel::Accel::new(repaint.clone());
        Engine {
            project: Arc::new(Project::default()),
            generation: 0,
            preview: preview::PreviewRenderer::new(repaint.clone(), accel.clone()),
            playback: None,
            media_preview: None,
            render: None,
            trimmer: preview::PreviewRenderer::new(repaint.clone(), accel.clone()),
            trimmer_project: std::sync::Mutex::new(None),
            multicam: preview::PreviewRenderer::new(repaint.clone(), accel.clone()),
            thumbs: thumbs::Thumbs::new(repaint.clone()),
            source_view: source_view::SourceView::new(repaint.clone()),
            scenes: None,
            audio_sync: None,
            beats: None,
            ducking: None,
            nonrealtime: None,
            loudness: None,
            repaint,
            audio_status: AudioStatus::NotStarted,
            audio_out: None,
            audio_failed: None,
            proxy_jobs: Vec::new(),
            prerender: None,
            recorder: None,
            live: None,
            live_error: None,
            real_time: None,
            prerendered: (None, Vec::new()),
            split: None,
            split_projects: [None, None],
            split_count: 0,
            tracking: None,
            analysis: None,
            accel,
            preview_look: PreviewLook::default(),
            bypassed: std::sync::Mutex::new(None),
        }
    }

    /// How the Video Preview draws its frames from now on.
    pub fn set_preview_look(&mut self, look: PreviewLook) {
        self.preview_look = look;
    }

    /// What the preview's frames are drawn with: the look and, for
    /// Split-Screen View, the project with its video effects bypassed.
    fn drawing(&self) -> preview::Drawing {
        let look = self.preview_look;
        let bypassed = look.bypass.map(|_| {
            let mut b = self.bypassed.lock().expect("bypassed project");
            match &*b {
                Some((g, p)) if *g == self.generation => p.clone(),
                _ => {
                    let p = Arc::new(self.project.video_fx_bypassed());
                    *b = Some((self.generation, p.clone()));
                    p
                }
            }
        });
        preview::Drawing { look, bypassed }
    }

    /// The generation the preview's frames are told apart by: the
    /// project's, and the look they are drawn with.
    fn preview_generation(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let look = self.preview_look;
        if look == PreviewLook::default() {
            return self.generation;
        }
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.generation.hash(&mut h);
        look.quality.hash(&mut h);
        look.bypass.map(|a| a.map(f32::to_bits)).hash(&mut h);
        look.channel.hash(&mut h);
        // Never a plain generation.
        h.finish() | 1 << 63
    }

    /// Which graphics card composites video (the processor until one is
    /// chosen). The card opens in the background.
    pub fn set_gpu(&self, choice: GpuChoice) {
        self.accel.set(choice);
    }

    /// What the graphics card compositing video is doing.
    pub fn gpu_status(&self) -> GpuStatus {
        self.accel.status()
    }

    /// Whether video is decoded on the graphics card (Windows' decoders
    /// driving the card's video engine) or on the processor.
    pub fn set_hardware_decoding(&self, choice: DecodeChoice) {
        self.accel.set_decoding(&choice);
    }

    /// The graphics cards that can decode video, the fastest first.
    pub fn decode_adapters() -> Vec<String> {
        bv_mf::decode_adapters()
    }

    /// The most threads the processor's compositor uses (Options ›
    /// Preferences › Video, "Maximum number of rendering threads").
    pub fn set_render_threads(n: usize) {
        bv_render::set_threads(n);
    }

    /// Informa o estado atual do projeto. Barato quando nada mudou.
    pub fn set_project(&mut self, project: &Project) {
        if *self.project == *project {
            return;
        }
        self.project = Arc::new(project.clone());
        self.generation += 1;
        if let Some(p) = &self.playback {
            p.set_project(self.project.clone());
        }
    }

    /// Descreve um arquivo de mídia para importá-lo no projeto.
    pub fn probe_file(path: &str) -> Result<bv_timeline::MediaInfo, MediaError> {
        bv_media::probe_file(path)
    }

    /// Describes `count` numbered pictures from `first` as one video.
    pub fn probe_sequence(
        first: &str,
        count: u32,
        rate: bv_core::FrameRate,
    ) -> Result<bv_timeline::MediaInfo, MediaError> {
        bv_media::probe_sequence(first, count, rate)
    }

    /// How many numbered pictures follow `first` without a gap.
    pub fn sequence_length(first: &str) -> u32 {
        bv_media::sequence_length(first)
    }

    /// Recording start of a Broadcast Wave file, if it has one.
    pub fn bwf_start(path: &str) -> Option<Tick> {
        bv_media::bwf_start(path)
    }

    pub fn project(&self) -> &Arc<Project> {
        &self.project
    }

    /// Começa a tocar a partir de `from`.
    pub fn play(&mut self, from: Tick, options: PlayOptions) {
        self.stop();
        self.media_preview = None;
        let held = self.audio_out.clone();
        let p = playback::Playback::start(self.project.clone(), from, options, held);
        self.audio_status = p.audio_status();
        self.playback = Some(p);
    }

    /// Keeps the audio output open between playbacks (playback starts at
    /// once), or lets it go as soon as nothing plays. BananaV holds it while
    /// it is the active program (Options › Preferences › General: "Close
    /// audio and MIDI ports when not the active application").
    pub fn hold_audio(&mut self, hold: bool) {
        if !hold {
            if !self.is_playing() {
                self.audio_out = None;
            }
            return;
        }
        if self.audio_out.as_ref().is_some_and(|o| !o.alive()) {
            self.audio_out = None;
        }
        let retry = self
            .audio_failed
            .is_none_or(|t| t.elapsed() > std::time::Duration::from_secs(10));
        if self.audio_out.is_none() && retry {
            match playback::Output::open(self.project.settings.sample_rate.0) {
                Ok(o) => {
                    self.audio_status = o.status();
                    self.audio_out = Some(Arc::new(o));
                    self.audio_failed = None;
                }
                Err(e) => {
                    playback::note_audio_error(format!(
                        "The audio output could not be opened: {e}"
                    ));
                    self.audio_failed = Some(std::time::Instant::now());
                }
            }
        }
    }

    /// The audio output's failures since the last call (opening it, the
    /// device failing while open), oldest first, in English: for the
    /// interface to note down.
    pub fn take_audio_errors(&self) -> Vec<String> {
        playback::take_audio_errors()
    }

    /// Options › Preferences › Audio Device: the output device, by name
    /// (`None`: the system's default, followed when it changes). A change
    /// opens it at the next playback.
    pub fn set_audio_device(&mut self, device: Option<String>) {
        if playback::output_device() == device {
            return;
        }
        playback::set_output_device(device);
        if !self.is_playing() {
            self.audio_out = None;
            self.audio_failed = None;
        }
    }

    /// The output devices of this computer, the system's default first
    /// (name, whether it is the default).
    pub fn output_devices() -> Vec<(String, bool)> {
        use cpal::traits::{DeviceTrait, HostTrait};
        let host = cpal::default_host();
        let name_of = |d: &cpal::Device| d.description().ok().map(|d| d.name().to_string());
        let default = host.default_output_device().and_then(|d| name_of(&d));
        let mut out: Vec<(String, bool)> = host
            .output_devices()
            .map(|devices| {
                devices
                    .filter_map(|d| name_of(&d))
                    .map(|n| {
                        let is_default = default.as_deref() == Some(n.as_str());
                        (n, is_default)
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.sort_by_key(|(_, d)| !d);
        out
    }

    /// Options › Preferences › Audio Device: "Playback buffering" and,
    /// with "Enable track buffering", "Track buffering" (seconds).
    pub fn set_audio_buffering(playback_s: f32, track_s: Option<f32>) {
        let secs = |s: f32| std::time::Duration::from_secs_f32(s.clamp(0.01, 2.0));
        playback::set_buffering(secs(playback_s), track_s.map(secs));
    }

    /// Options › Metronome: on with `clicks`, or off.
    pub fn set_metronome(clicks: Option<Clicks>) {
        playback::set_metronome(clicks);
    }

    /// The Master Bus's Downmix Output: what playback folds the mix down
    /// to (renders never).
    pub fn set_downmix(d: Downmix) {
        playback::set_downmix(d);
    }

    pub fn downmix() -> Downmix {
        playback::downmix()
    }

    /// The Master Bus's Dim Output: playback 20 dB down (renders never).
    pub fn set_dim(on: bool) {
        playback::set_dim(on);
    }

    /// The Loudness Meters' readings: since playback last started or the
    /// meters were reset (`None` before anything played).
    pub fn loudness() -> Option<LiveLoudness> {
        playback::loudness()
    }

    /// Reset Meter Measurement.
    pub fn reset_loudness() {
        playback::reset_loudness();
    }

    /// Reset Clip (the true peaks' over-the-limit mark).
    pub fn reset_true_peak_clip() {
        playback::reset_true_peak_clip();
    }

    /// True Peak Blocking Filter (from the next playback or reset).
    pub fn set_true_peak_blocking(on: bool) {
        playback::set_true_peak_blocking(on);
    }

    pub fn dim() -> bool {
        playback::dim()
    }

    /// Follows the system's default audio output when it changed (while
    /// nothing plays), unless a device is chosen.
    pub fn follow_default_audio(&mut self) {
        if playback::output_device().is_some() {
            return;
        }
        if !self.is_playing()
            && let Some(o) = &self.audio_out
            && playback::Output::default_name().is_some_and(|n| n != o.name)
        {
            self.audio_out = None;
        }
    }

    /// Whether bypassed audio effects keep running, unheard (turning them
    /// on again then joins without a break).
    pub fn set_keep_bypassed_fx(keep: bool) {
        bv_mixer::set_keep_bypassed(keep);
    }

    /// Whether the graphics card draws frames under ACES (Options ›
    /// Preferences › General: "Use GPU processing for ACES color
    /// management"); off, the processor does.
    pub fn set_aces_on_gpu(on: bool) {
        accel::set_aces_on_gpu(on);
    }

    /// The length of the quick fades at audio event edges (Options ›
    /// Preferences › Editing: "Quick fade length for audio events").
    pub fn set_quick_fade(length: Tick) {
        bv_mixer::set_quick_fade(length);
    }

    /// Whether tracks' pre-volume sends go silent with the track's mute
    /// (Options › Preferences › Audio: "Track prefader sends listen to
    /// mute"); else they play on, for cue mixes.
    pub fn set_prefader_sends_listen_to_mute(listen: bool) {
        bv_mixer::set_prefader_sends_listen_to_mute(listen);
    }

    /// Whether waveforms of media files are kept in peak files next to
    /// them (`.bvpk`), and whether new ones are hidden.
    pub fn set_peak_files(write: bool, hide: bool) {
        thumbs::peak_files::set(write, hide);
    }

    /// Deletes the peak files of a media file (Rebuild Audio Peaks).
    pub fn delete_peak_files(media: &str) {
        thumbs::peak_files::delete(media);
    }

    /// Closes the media files the previews and thumbnails keep open, so
    /// that other programs can change them; they open again when needed.
    pub fn release_media(&self) {
        self.preview.release();
        self.trimmer.release();
        self.thumbs.release();
        if let Some(split) = &self.split {
            split.iter().for_each(preview::PreviewRenderer::release);
        }
    }

    /// Expanded Edit Mode's split preview: `sides[0]`, the event before the
    /// edit point at its last frame; `sides[1]`, the event after it at its
    /// first. Each is drawn alone, without its fades, fitting `max_w` ×
    /// `max_h`.
    pub fn request_edit_point(
        &mut self,
        sides: [Option<(bv_timeline::ClipId, Tick)>; 2],
        max_w: u32,
        max_h: u32,
    ) {
        let s = &self.project.settings;
        let (w, h) = preview_size(s, max_w, max_h);
        let rate = s.frame_rate;
        let (repaint, accel) = (self.repaint.clone(), self.accel.clone());
        let split = self.split.get_or_insert_with(|| {
            Box::new([
                preview::PreviewRenderer::new(repaint.clone(), accel.clone()),
                preview::PreviewRenderer::new(repaint, accel),
            ])
        });
        for (i, side) in sides.into_iter().enumerate() {
            let Some((clip, t)) = side else {
                continue;
            };
            let current = self.split_projects[i]
                .as_ref()
                .filter(|s| s.generation == self.generation && s.clip == clip);
            let side = match current {
                Some(side) => side.clone(),
                None => {
                    self.split_count += 1;
                    let side = SplitSide {
                        generation: self.generation,
                        clip,
                        project: Arc::new(alone(&self.project, clip)),
                        number: self.split_count,
                    };
                    self.split_projects[i] = Some(side.clone());
                    side
                }
            };
            split[i].request(side.project, side.number, rate.snap_floor(t), (w, h), None);
        }
    }

    /// The frames of the split preview, as they are drawn.
    pub fn edit_point_frames(&self) -> [Option<Arc<PreviewFrame>>; 2] {
        match &self.split {
            Some(split) => [split[0].latest(), split[1].latest()],
            None => [None, None],
        }
    }

    /// Para o playback e devolve onde ele estava.
    pub fn stop(&mut self) -> Option<Tick> {
        self.playback.take().map(|p| p.position())
    }

    pub fn is_playing(&self) -> bool {
        self.playback.as_ref().is_some_and(|p| !p.finished())
    }

    /// Posição atual do playback (relógio de áudio), se estiver tocando.
    pub fn position(&self) -> Option<Tick> {
        self.playback.as_ref().map(|p| p.position())
    }

    /// Se o playback chegou ao fim do projeto sozinho, encerra e devolve o fim.
    pub fn poll_finished(&mut self) -> Option<Tick> {
        if self.playback.as_ref().is_some_and(|p| p.finished()) {
            return self.stop();
        }
        None
    }

    /// Picos de áudio (por canal, linear) desde a última chamada.
    pub fn take_peaks(&self) -> Option<Vec<f32>> {
        self.playback.as_ref().and_then(|p| p.take_peaks())
    }

    /// Peak per channel (linear) of each track heard since the last call:
    /// the meters in the track headers.
    pub fn take_track_peaks(&self) -> Vec<(bv_timeline::TrackId, Vec<f32>)> {
        self.playback
            .as_ref()
            .map_or_else(Vec::new, |p| p.take_track_peaks())
    }

    /// The Color Grading effect's grade with `values` (its parameters at a
    /// time; those not given at their defaults) as a `.cube` file's text,
    /// `size` points a side: the Color Grading window's Export LUT.
    pub fn color_grading_cube(values: &[(String, bv_ofx::Value)], size: usize) -> String {
        bv_effects::color_grading::cube(&effect_values(values), size)
    }

    /// What a picker of the Color Grading window sets, from the colors
    /// picked on the preview (straight RGB, the effect's input there: a
    /// click's few pixels, or a region's when `region`) and the effect's
    /// `values` at the cursor: the parameters' new values by name.
    pub fn color_grading_pick(
        values: &[(String, bv_ofx::Value)],
        picker: ColorGradingPicker,
        colors: &[[f32; 3]],
        region: bool,
    ) -> Vec<(String, bv_ofx::Value)> {
        use bv_effects::accel::Value;
        bv_effects::color_grading::pick(&effect_values(values), picker, colors, region)
            .into_iter()
            .map(|(name, v)| {
                let v = match v {
                    Value::Num(n) => bv_ofx::Value::Num(n),
                    Value::Curves(c) => bv_ofx::Value::Curves(c),
                    Value::Text(t) => bv_ofx::Value::Text(t),
                };
                (name, v)
            })
            .collect()
    }

    /// The region Motion Tracking imports from effect `id` with `values`
    /// (its values at a time) on a frame `aspect` wide for 1 high: the
    /// corners of a Picture In Picture's picture or of a Bézier Masking's
    /// mask `mask`, 0–1 of the frame from its top left.
    pub fn tracked_region(
        id: &str,
        values: &[(String, bv_ofx::Value)],
        aspect: f64,
        mask: usize,
    ) -> Option<[[f64; 2]; 4]> {
        let values = effect_values(values);
        bv_effects::tracked_region(id, bv_effects::accel::Values(&values), aspect, mask)
    }

    /// The latest VU/PPM reading (a sine's amplitude per channel) of each
    /// meter heard since the last call: the Mixing Console's VU Meters.
    pub fn take_vu_levels(&self) -> Vec<(meters::MeterOf, Vec<f32>)> {
        self.playback
            .as_ref()
            .map_or_else(Vec::new, |p| p.take_vu_levels())
    }

    /// Which VU/PPM meters the mixer computes (`None`: none) and whether
    /// their needles are vintage.
    pub fn set_vu_meters(kind: Option<meters::MeterKind>, vintage: bool) {
        meters::set_vu_meters(kind, vintage);
    }

    /// Peak per channel (linear) of each audio bus and assignable effects
    /// chain heard since the last call: their meters in the Mixing
    /// Console.
    pub fn take_strip_peaks(&self) -> Vec<(bv_timeline::Strip, Vec<f32>)> {
        self.playback
            .as_ref()
            .map_or_else(Vec::new, |p| p.take_strip_peaks())
    }

    /// How many pairs of channels the output device has: its hardware
    /// outputs for busses (1/2, 3/4…; one while none is open).
    pub fn hardware_outputs(&self) -> u8 {
        match &self.audio_status {
            AudioStatus::Device { channels, .. } => (channels / 2).clamp(1, 255) as u8,
            _ => 1,
        }
    }

    pub fn audio_status(&self) -> &AudioStatus {
        &self.audio_status
    }

    /// Miniatura do quadro `t` da mídia (altura `height`), se já estiver pronta.
    pub fn thumbnail(
        &self,
        info: &bv_timeline::MediaInfo,
        t: Tick,
        height: u32,
    ) -> Option<Arc<Thumbnail>> {
        self.thumbs.thumbnail(info, t, height)
    }

    /// The Pan/Crop window's picture of event `clip` (its media `info` at
    /// media time `time`, `size` pixels big on the screen; `span`, the
    /// event's length): decoded by readers of its own at that size,
    /// whatever the Video Preview shows. The latest one ready of the event
    /// — the one asked for, or an earlier one while it is decoded.
    pub fn source_picture(
        &self,
        clip: bv_timeline::ClipId,
        info: &bv_timeline::MediaInfo,
        time: Tick,
        span: Tick,
        size: (u32, u32),
    ) -> Option<Arc<SourcePicture>> {
        let levels = self.project.settings.pixel_format.is_video_levels();
        self.source_view
            .picture(clip.0, info, time, span, size, levels)
    }

    /// No Pan/Crop window shows: its readers go.
    pub fn close_source_view(&self) {
        self.source_view.close();
    }

    /// Forma de onda da mídia, se já estiver pronta.
    /// One second (`index`) of a media's audio stream, sample by sample, for
    /// the waveform of a zoomed-in timeline (`None` while it is read).
    pub fn wave_chunk(
        &self,
        info: &bv_timeline::MediaInfo,
        stream: u16,
        index: i64,
    ) -> Option<Arc<WaveChunk>> {
        self.thumbs.wave_chunk(info, stream, index)
    }

    /// Waveform of the media's audio stream `stream` (0 = the first).
    pub fn waveform(&self, info: &bv_timeline::MediaInfo, stream: u16) -> Option<Arc<Waveform>> {
        self.thumbs.waveform(info, stream)
    }

    /// Recalcula miniaturas e formas de onda (ex.: arquivo alterado no disco).
    pub fn refresh_thumbnails(&self) {
        self.thumbs.clear();
    }

    /// Pede o quadro `t` de uma mídia isolada (Unidade de corte).
    pub fn request_trimmer(&self, info: &bv_timeline::MediaInfo, t: Tick, max_w: u32, max_h: u32) {
        let Some(v) = info.video else {
            return;
        };
        let mut settings = self.project.settings;
        settings.width = v.width;
        settings.height = v.height;
        settings.frame_rate = v.frame_rate;
        // What the picture is made of — the media, how it is read, its
        // effects, the project's settings — names its frames: another
        // media, or the same turned, draws anew.
        let generation = {
            use std::hash::{Hash, Hasher};
            let mut hs = std::collections::hash_map::DefaultHasher::new();
            format!("{info:?}{settings:?}").hash(&mut hs);
            hs.finish()
        };
        // Its project is made once for that (each frame of a playback asks).
        let project = {
            let mut kept = self
                .trimmer_project
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            match kept.as_ref() {
                Some((key, p)) if *key == generation => p.clone(),
                _ => {
                    let mut p = Project::new(settings);
                    let track = p.tracks[0].id;
                    let id = p.add_media(info.clone());
                    let _ = p.insert_media_range(
                        id,
                        Tick::ZERO,
                        Tick::ZERO,
                        info.duration,
                        Some(track),
                        None,
                    );
                    let p = Arc::new(p);
                    *kept = Some((generation, p.clone()));
                    p
                }
            }
        };
        let scale = (max_w as f32 / v.width as f32)
            .min(max_h as f32 / v.height as f32)
            .min(1.0);
        let w = ((v.width as f32 * scale).round() as u32).max(2);
        let h = ((v.height as f32 * scale).round() as u32).max(2);
        let rate = v.frame_rate;
        let t = rate.snap_floor(t);
        // While its preview plays, the next frames are drawn ahead, as the
        // timeline's are while it plays.
        let playing = self
            .media_preview
            .as_ref()
            .is_some_and(|p| !p.playback.finished() && p.plays(info));
        let ahead = playing.then(|| rate.frame_start(rate.frame_at(t) + 1));
        self.trimmer.request(project, generation, t, (w, h), ahead);
    }

    pub fn trimmer_frame(&self) -> Option<Arc<PreviewFrame>> {
        self.trimmer.latest()
    }

    /// Asks for multicamera editing's view: `project` (the takes laid out
    /// in tiles) at `t`, fitted in `max_w` × `max_h`.
    pub fn request_multicam(&self, project: Project, t: Tick, max_w: u32, max_h: u32) {
        let s = project.settings;
        let scale = (max_w as f32 / s.width.max(1) as f32)
            .min(max_h as f32 / s.height.max(1) as f32)
            .min(1.0);
        let w = ((s.width as f32 * scale).round() as u32).max(2);
        let h = ((s.height as f32 * scale).round() as u32).max(2);
        // The tiles' layout and media: another layout is another picture.
        let generation = {
            use std::hash::{Hash, Hasher};
            let mut hs = std::collections::hash_map::DefaultHasher::new();
            for c in project.clips().map(|(_, c)| c) {
                (c.media.0, c.start.0, c.source_in.0, c.duration.0).hash(&mut hs);
            }
            for m in &project.media {
                format!("{:?}", m.source).hash(&mut hs);
            }
            hs.finish()
        };
        self.multicam.request(
            Arc::new(project),
            generation,
            s.frame_rate.snap_floor(t),
            (w, h),
            None,
        );
    }

    pub fn multicam_frame(&self) -> Option<Arc<PreviewFrame>> {
        self.multicam.latest()
    }

    /// Procura cortes de cena dentro do clipe (em segundo plano).
    /// `sensitivity` de 0 (só cortes óbvios) a 1 (qualquer mudança).
    pub fn start_scene_detection(&mut self, clip: bv_timeline::ClipId, sensitivity: f32) -> bool {
        let p = &self.project;
        let Some(c) = p.clip(clip).cloned() else {
            return false;
        };
        let Some(info) = p.media(c.media).cloned().filter(|m| m.video.is_some()) else {
            return false;
        };
        let threshold = 0.6 - 0.5 * sensitivity.clamp(0.0, 1.0);
        self.scenes = Some(scenes::SceneJob::start(
            info,
            c,
            p.settings.frame_rate,
            threshold,
            self.repaint.clone(),
        ));
        true
    }

    /// Looks for scene cuts in `part` of media `info`, in the background
    /// (Project Media's Detect Scenes and Add to Timeline and Detect Scenes
    /// and Create Subclips, the Trimmer's from its cursor): the cuts come in
    /// the media's time, [`SceneProgress::media`] naming it. `false` when it
    /// has no picture or the part is empty.
    pub fn start_media_scene_detection(
        &mut self,
        info: bv_timeline::MediaInfo,
        part: bv_core::TimeRange,
        sensitivity: f32,
    ) -> bool {
        let Some(video) = info.video else {
            return false;
        };
        if part.duration <= Tick::ZERO {
            return false;
        }
        // An event over the part where it lies in the media: its times are
        // the media's.
        let mut clip =
            bv_timeline::Clip::new(bv_timeline::ClipId(0), info.id, part.start, part.duration);
        clip.source_in = part.start;
        let threshold = 0.6 - 0.5 * sensitivity.clamp(0.0, 1.0);
        let id = info.id;
        self.scenes = Some(
            scenes::SceneJob::start(
                info,
                clip,
                video.frame_rate,
                threshold,
                self.repaint.clone(),
            )
            .of_media(id),
        );
        true
    }

    pub fn scene_progress(&self) -> Option<SceneProgress> {
        self.scenes.as_ref().map(|j| j.progress())
    }

    pub fn cancel_scene_detection(&mut self) {
        self.scenes = None;
    }

    /// Lines events up by their sound (Tools › Multicamera › Sync Mixed
    /// Audio to Align Events), in the background: `clips[0]` is the target,
    /// which stays, and how far each other one moves comes in
    /// [`audio_sync_progress`](Engine::audio_sync_progress). Events without
    /// sound are left out; `false` when the target has none or nothing
    /// else does.
    pub fn start_audio_sync(&mut self, clips: &[bv_timeline::ClipId]) -> bool {
        let p = &self.project;
        let source = |&id: &bv_timeline::ClipId| {
            let c = p.clip(id)?;
            let info = p.media(c.media)?;
            info.audio_stream(c.audio_stream)?;
            Some(audio_sync::SyncSource {
                clip: id,
                info: info.clone(),
                stream: c.audio_stream,
                start: c.start,
                source_in: c.source_in,
                duration: c.duration,
                rate: c.rate,
            })
        };
        let Some(target) = clips.first().and_then(source) else {
            return false;
        };
        let mut sources = vec![target];
        sources.extend(clips[1..].iter().filter_map(source));
        if sources.len() < 2 {
            return false;
        }
        self.audio_sync = Some(audio_sync::AudioSyncJob::start(
            sources,
            self.repaint.clone(),
        ));
        true
    }

    pub fn audio_sync_progress(&self) -> Option<AudioSyncProgress> {
        self.audio_sync.as_ref().map(|j| j.progress())
    }

    /// Stops the synchronization (and forgets a finished one).
    pub fn cancel_audio_sync(&mut self) {
        self.audio_sync = None;
    }

    /// Finds the beats and the tempo of `media`'s sound in the background
    /// (Beat Detection, Tempo Detection): see
    /// [`beat_progress`](Engine::beat_progress). `false` when it has none.
    pub fn start_beat_detection(&mut self, media: bv_timeline::MediaId) -> bool {
        let Some(info) = self.project.media(media) else {
            return false;
        };
        if info.audio_stream(0).is_none() {
            return false;
        }
        self.beats = Some(beat_detection::BeatJob::start(
            info.clone(),
            0,
            self.repaint.clone(),
        ));
        true
    }

    pub fn beat_progress(&self) -> Option<BeatProgress> {
        self.beats.as_ref().map(|j| j.progress())
    }

    /// Stops the beat detection (and forgets a finished one).
    pub fn cancel_beat_detection(&mut self) {
        self.beats = None;
    }

    /// Reads the loudness of events `clips` (an Auto Ducking controller's)
    /// in the background: see [`ducking_progress`](Engine::ducking_progress).
    /// Events that play no sound are left out; `false` when none does.
    pub fn start_ducking_analysis(&mut self, clips: &[bv_timeline::ClipId]) -> bool {
        let p = &self.project;
        let sources: Vec<ducking::DuckSource> = clips
            .iter()
            .filter_map(|&id| {
                let c = p.clip(id)?;
                let info = p.media(c.media)?;
                info.audio_stream(c.audio_stream)?;
                let (a, b) = (c.source_time(c.start), c.source_time(c.end()));
                let around = Tick::from_seconds_f64(ducking::AROUND);
                let from = (a.min(b) - around).max(Tick::ZERO);
                let to = (a.max(b) + around).min(info.duration.max(a.max(b)));
                (to > from).then(|| ducking::DuckSource {
                    clip: id,
                    info: info.clone(),
                    stream: c.audio_stream,
                    from,
                    to,
                })
            })
            .collect();
        if sources.is_empty() {
            return false;
        }
        self.ducking = Some(ducking::DuckJob::start(sources, self.repaint.clone()));
        true
    }

    pub fn ducking_progress(&self) -> Option<DuckProgress> {
        self.ducking.as_ref().map(|j| j.progress())
    }

    /// Stops the analysis (and forgets a finished one).
    pub fn cancel_ducking_analysis(&mut self) {
        self.ducking = None;
    }

    /// Renders the sound event `clip` plays through `effects` into a WAV
    /// file at `path`, in the background (Tools › Audio › Apply
    /// Non-Real-Time Event FX): see
    /// [`nonrealtime_progress`](Engine::nonrealtime_progress). `false` when
    /// the event plays no sound, or plays it backward or frozen.
    pub fn start_nonrealtime_fx(
        &mut self,
        clip: bv_timeline::ClipId,
        effects: Vec<bv_timeline::Effect>,
        path: std::path::PathBuf,
        format: NrtFormat,
    ) -> bool {
        let p = &self.project;
        let Some(c) = p.clip(clip) else {
            return false;
        };
        if c.reverse.is_some() || c.freeze.is_some() {
            return false;
        }
        let Some(info) = p.media(c.media) else {
            return false;
        };
        if info.audio_stream(c.audio_stream).is_none() {
            return false;
        }
        let (from, to) = (c.source_time(c.start), c.source_time(c.end()));
        if to <= from {
            return false;
        }
        self.nonrealtime = Some(nonrealtime::NrtJob::start(
            nonrealtime::NrtSource {
                clip,
                info: info.clone(),
                stream: c.audio_stream,
                from,
                to,
                effects,
                path,
                format,
                tempo: p.settings.tempo,
            },
            self.repaint.clone(),
        ));
        true
    }

    pub fn nonrealtime_progress(&self) -> Option<NrtProgress> {
        self.nonrealtime.as_ref().map(|j| j.progress())
    }

    /// Stops the rendering (its file goes) and forgets a finished one.
    pub fn cancel_nonrealtime_fx(&mut self) {
        self.nonrealtime = None;
    }

    /// Measures the loudness of `range` (or the whole project) in the
    /// background. Returns `false` when there is nothing to measure.
    pub fn start_loudness_scan(&mut self, range: Option<bv_core::TimeRange>) -> bool {
        let end = self.project.duration();
        let range = range.unwrap_or(bv_core::TimeRange::new(Tick::ZERO, end));
        if range.duration <= Tick::ZERO {
            return false;
        }
        self.loudness = Some(loudness::LoudnessJob::start(
            self.project.clone(),
            range,
            self.repaint.clone(),
        ));
        true
    }

    pub fn loudness_progress(&self) -> Option<LoudnessProgress> {
        self.loudness.as_ref().map(|j| j.progress())
    }

    pub fn cancel_loudness_scan(&mut self) {
        self.loudness = None;
    }

    /// The project a file was rendered from, when the renderer kept its
    /// path (Render's "Save project as path reference in rendered file":
    /// an MPEG-4 family file's or a WAV file's): Edit Source Project.
    pub fn project_reference(path: &std::path::Path) -> Option<std::path::PathBuf> {
        // A nested project is its own source.
        if bv_media::is_project(&path.to_string_lossy()) {
            return Some(path.to_path_buf());
        }
        let mut f = std::fs::File::open(path).ok()?;
        let text = match bv_wav::read_project_reference(&mut f) {
            Ok(found) => found,
            Err(_) => bv_mp4::read::project_reference(&mut f),
        }?;
        Some(std::path::PathBuf::from(text))
    }

    /// Starts rendering to a file in the background (replacing an earlier
    /// render).
    pub fn start_render(&mut self, settings: RenderSettings) {
        self.render = Some(render::RenderJob::start(
            self.project.clone(),
            settings,
            self.repaint.clone(),
            self.accel.clone(),
        ));
    }

    pub fn render_progress(&self) -> Option<RenderProgress> {
        self.render.as_ref().map(|e| e.progress())
    }

    /// Whether the render shows its frames in the preview while it runs.
    pub fn show_render_video(&self, on: bool) {
        if let Some(r) = &self.render {
            r.show_video(on);
        }
    }

    /// The last frame of the render, while it shows them.
    pub fn render_frame(&self) -> Option<Arc<PreviewFrame>> {
        self.render.as_ref().and_then(|r| r.shown_frame())
    }

    pub fn cancel_render(&self) {
        if let Some(e) = &self.render {
            e.cancel();
        }
    }

    /// Forgets a finished render.
    pub fn clear_render(&mut self) {
        if self.render_progress().is_some_and(|p| p.finished) {
            self.render = None;
        }
    }

    /// The frame at `t` as the Video Preview's Copy Snapshot to Clipboard
    /// and Save Snapshot to File take it, as VEGAS's do at Best (Full): the
    /// output's whole size, its pictures scaled at Best, every effect —
    /// plug-ins too —, the video bus's motion blur and the output's effects,
    /// drawn as a render draws it (on the graphics card when there is one;
    /// never from a prerendered part or a proxy), over `background`. RGBA, 8
    /// bits, BT.709 as the preview shows it, the alpha straight; `None` when
    /// it could not be drawn.
    pub fn snapshot(&self, t: Tick, background: [f32; 4]) -> Option<(u32, u32, Vec<u8>)> {
        let project = self.project.clone();
        let accel = self.accel.clone();
        let size = project.settings.output_size();
        let t = project.settings.frame_rate.snap_floor(t);
        // Plug-ins run on a thread with the stack they may need.
        let img = std::thread::scope(|s| {
            std::thread::Builder::new()
                .name("snapshot".into())
                .stack_size(bv_ofx::PLUGIN_STACK)
                .spawn_scoped(s, move || {
                    let mut painter = accel::Painter::new(accel, plugins::Purpose::File);
                    let mut pool = bv_render::VideoPool::new();
                    let best = Some(bv_timeline::RenderQuality::Best);
                    painter.linear_as(&project, t, size, &mut pool, background, best)
                })
                .ok()?
                .join()
                .ok()
        })?;
        let tc = bv_core::TransferCharacteristic::Bt709;
        let rgba = bv_color::linear_to_display_rgba8_straight(&img, tc);
        Some((img.width, img.height, rgba))
    }

    /// A media's frame at `t` (media time), its own size, as a PNG: the
    /// Trimmer's Save Snapshot to File.
    pub fn media_snapshot_png(info: &bv_timeline::MediaInfo, t: Tick) -> Option<Vec<u8>> {
        let v = info.video?;
        let mut pool = bv_media::VideoPool::new();
        let img = pool.frame_scaled(info, t, v.width, v.height).ok()?;
        let rgba = bv_color::linear_to_display_rgba8(&img, bv_core::TransferCharacteristic::Bt709);
        Some(bv_png::encode_rgb8_from_rgba(
            img.width, img.height, &rgba, 6,
        ))
    }

    /// Save Snapshot to File's PNG: [`Engine::snapshot`] with the frame's
    /// transparency, as VEGAS's files keep it (where nothing covers the
    /// frame, or an effect leaves it clear).
    pub fn snapshot_png(&self, t: Tick) -> Option<Vec<u8>> {
        let (w, h, rgba) = self.snapshot(t, [0.0; 4])?;
        Some(bv_png::encode_rgba8(w, h, &rgba, 6))
    }

    /// Pede o quadro de `t` para o monitor, cabendo em `max_w` × `max_h`.
    pub fn request_preview(&self, t: Tick, max_w: u32, max_h: u32) {
        let s = &self.project.settings;
        let (w, h) = preview_size(s, max_w, max_h);
        let t = s.frame_rate.snap_floor(t);
        // While playing forward, the next frame is drawn ahead.
        let ahead = self
            .playback
            .as_ref()
            .filter(|p| !p.finished() && p.rate() > 0.0)
            .map(|p| {
                let d = s.frame_rate.frame_duration();
                let step = Tick((d.0 as f64 * p.rate()).round() as i64).max(d);
                s.frame_rate.snap_floor(t + step)
            });
        self.preview.request_with(
            self.project.clone(),
            self.preview_generation(),
            t,
            (w, h),
            ahead,
            self.drawing(),
        );
    }

    /// O quadro mais recente que ficou pronto.
    pub fn preview_frame(&self) -> Option<Arc<PreviewFrame>> {
        self.preview.latest()
    }

    /// How much memory the preview keeps drawn frames in, to show and play
    /// them again at once (Options › Preferences › Video: "Dynamic RAM
    /// Preview max"; 0 keeps none).
    pub fn set_ram_preview(&self, bytes: usize) {
        self.preview.set_cache_bytes(bytes);
    }

    /// Tools › Build Dynamic RAM Preview: draws every frame of `range` at
    /// the preview's size (`max_w` × `max_h` at most) into that memory, so
    /// that it plays at full rate.
    pub fn build_ram_preview(&self, range: TimeRange, max_w: u32, max_h: u32) {
        let s = &self.project.settings;
        let (w, h) = preview_size(s, max_w, max_h);
        let rate = s.frame_rate;
        let first = rate.frame_at(range.start);
        let last = rate.frame_at(range.end() - Tick(1));
        let times: Vec<Tick> = (first..=last).map(|k| rate.frame_start(k)).collect();
        self.preview.build_with(
            self.project.clone(),
            self.preview_generation(),
            times,
            (w, h),
            self.drawing(),
        );
    }

    /// How the Dynamic RAM Preview build goes.
    pub fn ram_preview_progress(&self) -> Option<preview::BuildProgress> {
        self.preview.build_progress()
    }

    /// Tools › Clear Dynamic RAM Preview.
    pub fn clear_ram_preview(&self) {
        self.preview.clear_cache();
    }
}

/// An OFX host's values as BananaV's own effects read them (texts,
/// numbers and curves).
fn effect_values(values: &[(String, bv_ofx::Value)]) -> Vec<(String, bv_effects::accel::Value)> {
    use bv_effects::accel::Value;
    values
        .iter()
        .filter_map(|(name, v)| {
            let v = match v {
                bv_ofx::Value::Num(n) => Value::Num(n.clone()),
                bv_ofx::Value::Curves(c) => Value::Curves(c.clone()),
                bv_ofx::Value::Text(t) => Value::Text(t.clone()),
                bv_ofx::Value::Bytes(_) | bv_ofx::Value::None => return None,
            };
            Some((name.clone(), v))
        })
        .collect()
}
