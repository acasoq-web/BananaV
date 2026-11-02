//! A media's pictures as its Media Properties say to read them (VEGAS's
//! Rotation and Color range's Limited): a reader around the file's own
//! that turns its pictures — a quarter turn swaps their width and height,
//! which the media's info already has swapped — and takes Y′CbCr the file
//! calls full range as 16–235. Readers of such media give their pictures in
//! linear light once turned (the graphics card gets them so).

use std::sync::Arc;

use bv_core::{ColorRange, ImageRgbaF32, Tick, VideoFrame};
use bv_timeline::VideoReading;

use crate::{Onward, Picture, Result, VideoReader, Want};

/// A reader of a media whose Rotation or Color range is set.
pub(crate) struct Reading {
    inner: Box<dyn VideoReader>,
    /// Quarter turns clockwise.
    quarters: u8,
    limited: bool,
    video_levels: bool,
    /// The last picture turned: the one it came from, and it.
    last: Option<(usize, Arc<ImageRgbaF32>)>,
}

impl Reading {
    /// `inner` read as `reading` says; as it is when that is nothing.
    pub(crate) fn around(
        inner: Box<dyn VideoReader>,
        reading: VideoReading,
    ) -> Box<dyn VideoReader> {
        let quarters = reading.rotation.quarters() % 4;
        if quarters == 0 && !reading.limited {
            return inner;
        }
        Box::new(Reading {
            inner,
            quarters,
            limited: reading.limited,
            video_levels: false,
            last: None,
        })
    }

    /// The inner reader's size for a picture shown `w` × `h`.
    fn inner_size(&self, w: u32, h: u32) -> (u32, u32) {
        if self.quarters % 2 == 1 {
            (h, w)
        } else {
            (w, h)
        }
    }

    /// `img` turned, the last one turned kept.
    fn turned(&mut self, img: Arc<ImageRgbaF32>) -> Arc<ImageRgbaF32> {
        if self.quarters == 0 {
            return img;
        }
        let key = Arc::as_ptr(&img) as usize;
        if let Some((k, done)) = &self.last
            && *k == key
        {
            return done.clone();
        }
        let done = Arc::new(turn(&img, self.quarters));
        self.last = Some((key, done.clone()));
        done
    }
}

/// `f` with Limited's range when it says full range.
fn limited_frame(f: Arc<VideoFrame>) -> Arc<VideoFrame> {
    if f.color.range == ColorRange::Limited {
        return f;
    }
    let mut g = (*f).clone();
    g.color.range = ColorRange::Limited;
    Arc::new(g)
}

/// A picture as read: Limited's range, then turned.
fn fix(p: Picture, quarters: u8, limited: bool, levels: bool, w: u32, h: u32) -> Picture {
    let p = match p {
        Picture::Decoded(f) if limited => Picture::Decoded(limited_frame(f)),
        p => p,
    };
    if quarters == 0 {
        return p;
    }
    let img = match p {
        Picture::Linear(img) => img,
        Picture::Decoded(f) => Arc::new(bv_color::frame_to_linear_levels(
            &f,
            crate::reduction(f.width, f.height, w, h),
            levels,
        )),
    };
    Picture::Linear(Arc::new(turn(&img, quarters)))
}

impl VideoReader for Reading {
    fn frame_at(&mut self, t: Tick) -> Result<Arc<ImageRgbaF32>> {
        self.frame_at_scaled(t, u32::MAX, u32::MAX)
    }

    fn frame_at_scaled(&mut self, t: Tick, min_w: u32, min_h: u32) -> Result<Arc<ImageRgbaF32>> {
        let (w, h) = self.inner_size(min_w, min_h);
        let img = if self.limited {
            match self.inner.picture_at(t, w, h)? {
                Picture::Linear(img) => img,
                Picture::Decoded(f) => {
                    let f = limited_frame(f);
                    Arc::new(bv_color::frame_to_linear_levels(
                        &f,
                        crate::reduction(f.width, f.height, w, h),
                        self.video_levels,
                    ))
                }
            }
        } else {
            self.inner.frame_at_scaled(t, w, h)?
        };
        Ok(self.turned(img))
    }

    fn set_span(&mut self, span: Option<Tick>) {
        self.inner.set_span(span);
    }

    fn set_video_levels(&mut self, on: bool) {
        self.video_levels = on;
        self.inner.set_video_levels(on);
    }

    fn set_alpha(&mut self, alpha: bv_timeline::AlphaChannel, background: [u8; 3]) {
        self.inner.set_alpha(alpha, background);
    }

    fn picture_at(&mut self, t: Tick, min_w: u32, min_h: u32) -> Result<Picture> {
        if self.quarters != 0 {
            return self.frame_at_scaled(t, min_w, min_h).map(Picture::Linear);
        }
        let (w, h) = self.inner_size(min_w, min_h);
        let p = self.inner.picture_at(t, w, h)?;
        Ok(fix(p, 0, self.limited, self.video_levels, w, h))
    }

    fn picture_on_the_way(
        &mut self,
        t: Tick,
        min_w: u32,
        min_h: u32,
        want: Want,
        on: &mut dyn FnMut(Option<Picture>) -> Onward,
    ) -> Result<Picture> {
        let (w, h) = self.inner_size(min_w, min_h);
        let (quarters, limited, levels) = (self.quarters, self.limited, self.video_levels);
        // Turned pictures are in linear light; Limited's are converted here.
        let inner_want = if quarters != 0 || limited {
            Want::Decoded
        } else {
            want
        };
        let p = self
            .inner
            .picture_on_the_way(t, w, h, inner_want, &mut |p| {
                on(p.map(|p| fix(p, quarters, limited, levels, w, h)))
            })?;
        let p = fix(p, quarters, limited, levels, w, h);
        Ok(match p {
            Picture::Decoded(f) if want == Want::Linear => {
                Picture::Linear(Arc::new(bv_color::frame_to_linear_levels(
                    &f,
                    crate::reduction(f.width, f.height, w, h),
                    levels,
                )))
            }
            p => p,
        })
    }
}

/// `img` turned `quarters` quarter turns clockwise.
pub(crate) fn turn(img: &ImageRgbaF32, quarters: u8) -> ImageRgbaF32 {
    let (w, h) = (img.width as usize, img.height as usize);
    let quarters = quarters % 4;
    let (ow, oh) = if quarters % 2 == 1 { (h, w) } else { (w, h) };
    let mut out = ImageRgbaF32::transparent(ow as u32, oh as u32);
    bv_color::par_rows(&mut out.data, ow, |oy, row| {
        for (ox, px) in row.iter_mut().enumerate() {
            // The source pixel that lands at (ox, oy).
            let (x, y) = match quarters {
                1 => (oy, h - 1 - ox),
                2 => (w - 1 - ox, h - 1 - oy),
                3 => (w - 1 - oy, ox),
                _ => (ox, oy),
            };
            *px = img.data[y * w + x];
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 3 × 2 picture turned: a quarter clockwise puts its bottom left
    /// pixel at the top left, its top left at the top right.
    #[test]
    fn turning_moves_the_corners() {
        let mut img = ImageRgbaF32::transparent(3, 2);
        for (i, p) in img.data.iter_mut().enumerate() {
            *p = [i as f32, 0.0, 0.0, 1.0];
        }
        let q = turn(&img, 1);
        assert_eq!((q.width, q.height), (2, 3));
        // Clockwise: the bottom row (3, 4, 5) becomes the left column.
        let v: Vec<f32> = q.data.iter().map(|p| p[0]).collect();
        assert_eq!(v, [3.0, 0.0, 4.0, 1.0, 5.0, 2.0]);
        let h = turn(&img, 2);
        let v: Vec<f32> = h.data.iter().map(|p| p[0]).collect();
        assert_eq!(v, [5.0, 4.0, 3.0, 2.0, 1.0, 0.0]);
        let c = turn(&img, 3);
        let v: Vec<f32> = c.data.iter().map(|p| p[0]).collect();
        assert_eq!(v, [2.0, 5.0, 1.0, 4.0, 0.0, 3.0]);
        // Four quarters: as it was.
        let back = turn(&turn(&q, 1), 2);
        assert_eq!(back.data, img.data);
    }
}
