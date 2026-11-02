//! Detecção de cenas: procura cortes dentro de um clipe comparando o
//! histograma de luminância de quadros vizinhos (em baixa resolução).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use bv_core::{FrameRate, ImageRgbaF32, Tick};
use bv_timeline::{Clip, ClipId, MediaId, MediaInfo};

use crate::RepaintFn;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SceneProgress {
    pub clip: Option<ClipId>,
    /// A media's part looked through instead of an event (Project Media's
    /// and the Trimmer's Detect Scenes): its cuts are in its time.
    pub media: Option<MediaId>,
    pub done: u64,
    pub total: u64,
    pub finished: bool,
    /// Instantes da timeline onde começam cenas novas.
    pub cuts: Vec<Tick>,
    pub error: Option<String>,
}

const BINS: usize = 32;

fn histogram(img: &ImageRgbaF32) -> [f32; BINS] {
    let mut h = [0f32; BINS];
    for px in &img.data {
        let y = 0.2126 * px[0] + 0.7152 * px[1] + 0.0722 * px[2];
        // Espaço perceptual: cortes entre cenas escuras também aparecem.
        let v = y.max(0.0).powf(1.0 / 2.2).min(0.9999);
        h[(v * BINS as f32) as usize] += 1.0;
    }
    let n = img.data.len().max(1) as f32;
    h.map(|x| x / n)
}

/// Diferença entre histogramas normalizados: 0 = iguais, 1 = disjuntos.
fn distance(a: &[f32; BINS], b: &[f32; BINS]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum::<f32>() / 2.0
}

pub(crate) struct SceneJob {
    progress: Arc<Mutex<SceneProgress>>,
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl SceneJob {
    pub(crate) fn start(
        info: MediaInfo,
        clip: Clip,
        rate: FrameRate,
        threshold: f32,
        repaint: RepaintFn,
    ) -> SceneJob {
        let first = rate.frame_at(clip.start);
        let last = rate.frame_at(clip.end() - Tick(1));
        let progress = Arc::new(Mutex::new(SceneProgress {
            clip: Some(clip.id),
            total: (last - first + 1).max(1) as u64,
            ..SceneProgress::default()
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let thread = {
            let progress = progress.clone();
            let cancel = cancel.clone();
            std::thread::Builder::new()
                .name("bv-cenas".into())
                .spawn(move || {
                    let result = (|| -> Result<Vec<Tick>, String> {
                        let mut reader = bv_media::open_video(&info).map_err(|e| e.to_string())?;
                        let mut prev: Option<[f32; BINS]> = None;
                        let mut cuts = Vec::new();
                        let mut since_cut = 0;
                        for (i, f) in (first..=last).enumerate() {
                            if cancel.load(Ordering::Relaxed) {
                                break;
                            }
                            let t = rate.frame_start(f).max(clip.start);
                            let img = reader
                                .frame_at_scaled(clip.source_time(t), 96, 54)
                                .map_err(|e| e.to_string())?;
                            let h = histogram(&img);
                            if let Some(p) = &prev
                                && distance(p, &h) > threshold
                                && since_cut >= 8
                                && t > clip.start
                            {
                                cuts.push(t);
                                since_cut = 0;
                            }
                            since_cut += 1;
                            prev = Some(h);
                            if i % 10 == 0 {
                                progress.lock().expect("progresso").done = i as u64;
                                repaint();
                            }
                        }
                        Ok(cuts)
                    })();
                    let mut p = progress.lock().expect("progresso");
                    p.finished = true;
                    p.done = p.total;
                    match result {
                        Ok(c) => p.cuts = c,
                        Err(e) => p.error = Some(e),
                    }
                    drop(p);
                    repaint();
                })
                .expect("não foi possível criar a thread de detecção de cenas")
        };
        SceneJob {
            progress,
            cancel,
            thread: Some(thread),
        }
    }

    pub(crate) fn progress(&self) -> SceneProgress {
        self.progress.lock().expect("progresso").clone()
    }

    /// The job looks through media `id`, not an event.
    pub(crate) fn of_media(self, id: MediaId) -> SceneJob {
        {
            let mut p = self.progress.lock().expect("progresso");
            p.clip = None;
            p.media = Some(id);
        }
        self
    }

    pub(crate) fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for SceneJob {
    fn drop(&mut self) {
        self.cancel();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distancia_de_histogramas() {
        let black = ImageRgbaF32::filled(4, 4, [0.0, 0.0, 0.0, 1.0]);
        let white = ImageRgbaF32::filled(4, 4, [1.0, 1.0, 1.0, 1.0]);
        let (hb, hw) = (histogram(&black), histogram(&white));
        assert_eq!(distance(&hb, &hb), 0.0);
        assert!((distance(&hb, &hw) - 1.0).abs() < 1e-6);
    }

    /// Mídia sem cortes não gera divisões. (O caso com cortes de verdade
    /// está no teste de integração com vídeo, em `tests/exportar.rs`.)
    #[test]
    fn midia_continua_nao_tem_cortes() {
        use bv_timeline::{Generator, Project};
        let p = Project::default();
        let info = MediaInfo::generated(Generator::ColorBars, &p.settings);
        let clip = Clip::new(ClipId(1), info.id, Tick::ZERO, Tick::from_seconds(1));
        let job = SceneJob::start(info, clip, FrameRate::FPS_30, 0.3, Arc::new(|| {}));
        loop {
            let pr = job.progress();
            if pr.finished {
                assert!(pr.error.is_none(), "{:?}", pr.error);
                assert!(pr.cuts.is_empty(), "barras de cor não têm cortes");
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}
