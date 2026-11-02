//! The Pan/Crop window's picture: an event's media at the window's needle,
//! decoded on a thread of its own by readers of its own, at the size the
//! window's workspace shows it — as VEGAS's window does, whatever the Video
//! Preview shows (Sync Cursor off, its quality, proxies, its effects) and
//! however long its frames take. Only the latest picture asked for counts:
//! while its reader seeks (a long-GOP recording has a keyframe every few
//! seconds) the pictures on the way show, as the preview's scrubbing does,
//! and the exact one follows.

use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use bv_core::{ImageRgbaF32, Tick, TransferCharacteristic};
use bv_media::{Ask, Picture, Want, reduction};
use bv_render::VideoPool;
use bv_timeline::MediaInfo;

use crate::RepaintFn;

/// An event's media as the Pan/Crop window shows it.
#[derive(Debug)]
pub struct SourcePicture {
    /// The event (its `ClipId`'s number).
    pub clip: u64,
    /// The media's time asked for.
    pub time: Tick,
    /// Whether it is that time's picture (`false`: one on the way to it,
    /// its reader still seeking).
    pub exact: bool,
    pub width: u32,
    pub height: u32,
    /// RGBA, 8 bits, encoded in BT.709: shared by the pictures of the
    /// same pixels (a still's at every time).
    pub rgba: Arc<Vec<u8>>,
}

/// How long a picture is waited for before one on the way to it shows (and
/// a newer request is looked at).
const PATIENCE: Duration = Duration::from_millis(30);

/// A picture asked for.
#[derive(Clone, PartialEq)]
struct Request {
    clip: u64,
    info: MediaInfo,
    time: Tick,
    /// The size it shows at, in the readers' steps (the media's own size
    /// halved 0 to 3 times).
    min_size: (u32, u32),
    /// The event's length (generated media's effect duration).
    span: Tick,
    video_levels: bool,
}

#[derive(Default)]
struct State {
    /// The latest picture asked for, and its number.
    asked: Option<Request>,
    serial: u64,
    /// The number of the last request whose exact picture is shown.
    done: u64,
    shown: Option<Arc<SourcePicture>>,
    /// No window shows: the readers go.
    close: bool,
    quit: bool,
}

type Shared = Arc<(Mutex<State>, Condvar)>;

fn lock(shared: &Shared) -> MutexGuard<'_, State> {
    shared.0.lock().unwrap_or_else(|e| e.into_inner())
}

pub(crate) struct SourceView {
    shared: Shared,
    repaint: RepaintFn,
    /// Started with the first picture asked for.
    thread: OnceLock<JoinHandle<()>>,
}

impl SourceView {
    pub(crate) fn new(repaint: RepaintFn) -> SourceView {
        SourceView {
            shared: Arc::new((Mutex::new(State::default()), Condvar::new())),
            repaint,
            thread: OnceLock::new(),
        }
    }

    /// Event `clip`'s media `info` at media time `time`, shown `size` big
    /// (pixels): asked for when it is not the latest asked already. The
    /// latest picture of the event ready (the one asked for, or an earlier
    /// one while it is decoded).
    pub(crate) fn picture(
        &self,
        clip: u64,
        info: &MediaInfo,
        time: Tick,
        span: Tick,
        size: (u32, u32),
        video_levels: bool,
    ) -> Option<Arc<SourcePicture>> {
        let min_size = match info.video {
            Some(v) => {
                let k = reduction(v.width, v.height, size.0.max(1), size.1.max(1));
                ((v.width / k).max(1), (v.height / k).max(1))
            }
            None => size,
        };
        let shown = {
            let mut st = lock(&self.shared);
            st.close = false;
            // Asked every frame of the window: the media copied only when
            // the request changes.
            let same = st.asked.as_ref().is_some_and(|r| {
                (r.clip, r.time, r.min_size, r.span, r.video_levels)
                    == (clip, time, min_size, span, video_levels)
                    && r.info == *info
            });
            if !same {
                st.asked = Some(Request {
                    clip,
                    info: info.clone(),
                    time,
                    min_size,
                    span,
                    video_levels,
                });
                st.serial += 1;
                self.shared.1.notify_all();
            }
            st.shown.clone()
        };
        self.thread.get_or_init(|| {
            let (shared, repaint) = (self.shared.clone(), self.repaint.clone());
            std::thread::Builder::new()
                .name("bv-pan-crop-source".into())
                .spawn(move || run(&shared, &repaint))
                .expect("the Pan/Crop window's reader thread")
        });
        shown.filter(|s| s.clip == clip)
    }

    /// No Pan/Crop window shows: its readers and picture go.
    pub(crate) fn close(&self) {
        let mut st = lock(&self.shared);
        if st.asked.is_some() || st.shown.is_some() {
            st.asked = None;
            st.shown = None;
            st.close = true;
            self.shared.1.notify_all();
        }
    }
}

impl Drop for SourceView {
    fn drop(&mut self) {
        lock(&self.shared).quit = true;
        self.shared.1.notify_all();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// The thread: each latest request's picture, pictures on the way first
/// while its reader seeks.
fn run(shared: &Shared, repaint: &RepaintFn) {
    let mut pool = VideoPool::new();
    // The picture shown last and its pixels, not converted again when it
    // comes back (a still's, the same at every time).
    let mut last: Option<(Picture, Arc<Vec<u8>>, u32, u32)> = None;
    loop {
        let (request, serial) = {
            let mut st = lock(shared);
            loop {
                if st.quit {
                    return;
                }
                if std::mem::take(&mut st.close) {
                    pool.clear();
                    last = None;
                }
                if let Some(r) = st.asked.as_ref().filter(|_| st.done < st.serial) {
                    break (r.clone(), st.serial);
                }
                st = shared.1.wait(st).unwrap_or_else(|e| e.into_inner());
            }
        };
        let ask = Ask {
            time: request.time,
            min_size: request.min_size,
            want: Want::Linear,
            span: Some(request.span),
            video_levels: request.video_levels,
        };
        pool.begin_frame();
        pool.set_patience(Some(Instant::now() + PATIENCE));
        let got = pool.picture(request.clip, &request.info, ask).ok();
        let exact = !pool.took_on_the_way();
        let made = got.map(|p| {
            let (rgba, width, height) = match &last {
                Some((l, rgba, w, h)) if same(l, &p) => (rgba.clone(), *w, *h),
                _ => {
                    let (rgba, w, h) = display(&p, request.min_size, request.video_levels);
                    (Arc::new(rgba), w, h)
                }
            };
            last = Some((p, rgba.clone(), width, height));
            SourcePicture {
                clip: request.clip,
                time: request.time,
                exact,
                width,
                height,
                rgba,
            }
        });
        let changed = {
            let mut st = lock(shared);
            // A picture that cannot be read is not asked for again.
            if (exact || made.is_none()) && st.serial == serial {
                st.done = serial;
            }
            match made {
                Some(m)
                    if st.shown.as_ref().is_none_or(|s| {
                        (s.clip, s.time, s.exact) != (m.clip, m.time, m.exact)
                            || !Arc::ptr_eq(&s.rgba, &m.rgba)
                    }) =>
                {
                    st.shown = Some(Arc::new(m));
                    true
                }
                _ => false,
            }
        };
        if changed {
            repaint();
        }
    }
}

/// Whether two pictures are the same one.
fn same(a: &Picture, b: &Picture) -> bool {
    match (a, b) {
        (Picture::Linear(a), Picture::Linear(b)) => Arc::ptr_eq(a, b),
        (Picture::Decoded(a), Picture::Decoded(b)) => Arc::ptr_eq(a, b),
        _ => false,
    }
}

/// `picture` as the window shows it — display values, at least `min_size`
/// big, brought down by whole steps (each pixel the mean of those it
/// covers) while it is twice that — and its size.
fn display(picture: &Picture, min_size: (u32, u32), video_levels: bool) -> (Vec<u8>, u32, u32) {
    let step = |w: u32, h: u32| {
        (w / min_size.0.max(1))
            .min(h / min_size.1.max(1))
            .clamp(1, 16)
    };
    let linear = match picture {
        Picture::Decoded(f) => bv_color::frame_to_linear_levels(
            f,
            reduction(f.width, f.height, min_size.0, min_size.1),
            video_levels,
        ),
        Picture::Linear(img) => shrunk(img, step(img.width, img.height)),
    };
    (
        bv_color::linear_to_display_rgba8(&linear, TransferCharacteristic::Bt709),
        linear.width,
        linear.height,
    )
}

/// `img` brought down `k` times each way, each pixel the mean of the `k` ×
/// `k` it covers.
fn shrunk(img: &ImageRgbaF32, k: u32) -> ImageRgbaF32 {
    if k <= 1 {
        return img.clone();
    }
    let (w, h) = (img.width as usize, img.height as usize);
    let k = k as usize;
    let (ow, oh) = ((w / k).max(1), (h / k).max(1));
    let mut out = ImageRgbaF32::transparent(ow as u32, oh as u32);
    let n = (k * k) as f32;
    for oy in 0..oh {
        for ox in 0..ow {
            let mut sum = [0.0f32; 4];
            for y in oy * k..((oy + 1) * k).min(h) {
                for p in &img.data[y * w + ox * k..y * w + ((ox + 1) * k).min(w)] {
                    for (s, v) in sum.iter_mut().zip(p) {
                        *s += v;
                    }
                }
            }
            out.data[oy * ow + ox] = sum.map(|s| s / n);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bv_timeline::{Generator, ProjectSettings};

    /// The picture asked for comes at the size it shows at (in the
    /// readers' steps), of the event and time asked; a newer request
    /// brings its own; closed, nothing is shown.
    #[test]
    fn the_window_s_picture_comes_at_its_size() {
        let settings = ProjectSettings::default();
        let info = MediaInfo::generated(Generator::ColorBars, &settings);
        let view = SourceView::new(Arc::new(|| {}));
        let wait = |time: Tick| {
            let start = Instant::now();
            loop {
                if let Some(p) = view.picture(
                    7,
                    &info,
                    time,
                    Tick::from_seconds_f64(5.0),
                    (300, 170),
                    false,
                ) && p.time == time
                    && p.exact
                {
                    break p;
                }
                assert!(start.elapsed() < Duration::from_secs(10), "a picture");
                std::thread::sleep(Duration::from_millis(2));
            }
        };
        let p = wait(Tick::ZERO);
        let v = info.video.unwrap();
        assert!(
            p.width >= 300 && p.width < 600 && p.width <= v.width,
            "{}",
            p.width
        );
        assert_eq!(p.rgba.len(), (p.width * p.height * 4) as usize);
        assert_eq!(p.clip, 7);
        // A still: the same pixels at another time, not converted again.
        let later = Tick::from_seconds_f64(1.0);
        assert!(Arc::ptr_eq(&wait(later).rgba, &p.rgba));
        view.close();
        assert!(lock(&view.shared).shown.is_none());
    }

    /// Brought down, each pixel is the mean of those it covers.
    #[test]
    fn shrinking_averages() {
        let mut img = ImageRgbaF32::transparent(4, 2);
        for (i, p) in img.data.iter_mut().enumerate() {
            *p = [i as f32, 0.0, 0.0, 1.0];
        }
        let s = shrunk(&img, 2);
        assert_eq!((s.width, s.height), (2, 1));
        assert_eq!(s.data[0], [(0.0 + 1.0 + 4.0 + 5.0) / 4.0, 0.0, 0.0, 1.0]);
        assert_eq!(s.data[1], [(2.0 + 3.0 + 6.0 + 7.0) / 4.0, 0.0, 0.0, 1.0]);
    }
}
