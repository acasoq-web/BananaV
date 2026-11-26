//! The kernels run on the processor, and what reads the pictures for them
//! (`kernels/prelude.wgsl`'s `at…` and `bil…`, written by hand).

use crate::generated::ENTRIES;
use crate::graph::{Graph, Params, Slot};
use crate::math::{V2, V4, Vn};

/// A picture read: rows from the top, premultiplied.
#[derive(Clone, Copy, Debug)]
pub struct Tex<'a> {
    pub data: &'a [[f32; 4]],
    pub w: i32,
    pub h: i32,
}

impl Tex<'_> {
    /// Pixel (x, y), the edges held.
    #[inline(always)]
    fn at(&self, x: i32, y: i32) -> V4 {
        let x = x.min(self.w - 1).max(0);
        let y = y.min(self.h - 1).max(0);
        // Never outside: the size is the data's.
        match self.data.get(y as usize * self.w as usize + x as usize) {
            Some(c) => Vn(*c),
            None => Vn([0.0; 4]),
        }
    }

    /// Between pixels (their middles at whole numbers), the edges held.
    #[inline(always)]
    fn bilinear(&self, v: V2) -> V4 {
        let x = v.0[0].max(-4.0).min(self.w as f32 + 4.0);
        let y = v.0[1].max(-4.0).min(self.h as f32 + 4.0);
        let (bx, by) = (x.floor(), y.floor());
        let (tx, ty) = (x - bx, y - by);
        let (ix, iy) = (bx as i32, by as i32);
        let a = self.at(ix, iy).0;
        let b = self.at(ix + 1, iy).0;
        let c = self.at(ix, iy + 1).0;
        let d = self.at(ix + 1, iy + 1).0;
        Vn(std::array::from_fn(|i| {
            let low = a[i] + (b[i] - a[i]) * tx;
            let high = c[i] + (d[i] - c[i]) * tx;
            low + (high - low) * ty
        }))
    }
}

/// What a kernel gets: its numbers and the pictures it reads.
#[derive(Debug)]
pub struct C<'a> {
    pub p: &'a Params,
    pub src: [Tex<'a>; 3],
}

macro_rules! native {
    ($($at:ident $bil:ident $n:literal),*) => {
        $(
            #[inline(always)]
            pub fn $at(c: &C<'_>, x: i32, y: i32) -> V4 {
                c.src[$n].at(x, y)
            }

            #[inline(always)]
            pub fn $bil(c: &C<'_>, v: V2) -> V4 {
                c.src[$n].bilinear(v)
            }
        )*
    };
}

native!(at0 bil0 0, at1 bil1 1, at2 bil2 2);

/// A picture: rows from the top, premultiplied.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Picture {
    pub w: u32,
    pub h: u32,
    pub data: Vec<[f32; 4]>,
}

impl Picture {
    pub fn new(w: u32, h: u32) -> Picture {
        Picture {
            w,
            h,
            data: vec![[0.0; 4]; w as usize * h as usize],
        }
    }

    fn tex(&self) -> Tex<'_> {
        Tex {
            data: &self.data,
            w: self.w as i32,
            h: self.h as i32,
        }
    }
}

/// How the rows are shared out: runs the function for each row's number
/// and pixels (`usize` pixels a row), on as many threads as there are.
pub type Rows<'a> = &'a dyn Fn(&mut [[f32; 4]], usize, &(dyn Fn(usize, &mut [[f32; 4]]) + Sync));

/// The rows one after another.
pub fn in_turn(data: &mut [[f32; 4]], width: usize, f: &(dyn Fn(usize, &mut [[f32; 4]]) + Sync)) {
    for (y, row) in data.chunks_mut(width.max(1)).enumerate() {
        f(y, row);
    }
}

/// Draws `graph` from `inputs` (its first pictures).
pub fn run(graph: &Graph, inputs: &[&Picture], rows: Rows<'_>) -> Picture {
    let empty = Picture::new(1, 1);
    let mut drawn: Vec<Option<Picture>> = vec![None; graph.slots as usize];
    for d in &graph.data {
        if let Some(slot) = drawn.get_mut(d.slot.0 as usize) {
            *slot = Some(Picture {
                w: d.size[0],
                h: d.size[1],
                data: d.pixels.to_vec(),
            });
        }
    }
    let last = graph.last_reads();
    let mut spare: Vec<Vec<[f32; 4]>> = Vec::new();
    for (n, pass) in graph.passes.iter().enumerate() {
        let (w, h) = (pass.params.w, pass.params.h);
        let mut data = spare.pop().unwrap_or_default();
        data.clear();
        data.resize(w as usize * h as usize, [0.0; 4]);
        {
            let picture = |s: Slot| -> &Picture {
                let i = s.0 as usize;
                match drawn.get(i).and_then(Option::as_ref) {
                    Some(p) => p,
                    None => inputs.get(i).copied().unwrap_or(&empty),
                }
            };
            let c = C {
                p: &pass.params,
                src: pass.inputs.map(|s| picture(s).tex()),
            };
            if let Some(e) = ENTRIES.get(pass.kernel.0 as usize) {
                let run = e.run;
                rows(&mut data, w as usize, &|y, row| {
                    for (x, px) in row.iter_mut().enumerate() {
                        *px = run(&c, Vn([x as i32, y as i32])).0;
                    }
                });
            }
        }
        if let Some(d) = drawn.get_mut(pass.out.0 as usize) {
            *d = Some(Picture { w, h, data });
        }
        // What no later pass reads is drawn over.
        for (i, slot) in drawn.iter_mut().enumerate() {
            let kept = i == graph.out.0 as usize || i == pass.out.0 as usize;
            if !kept
                && last[i].is_none_or(|l| l <= n)
                && let Some(p) = slot.take()
            {
                spare.push(p.data);
            }
        }
    }
    let out = graph.out.0 as usize;
    match drawn.get_mut(out).and_then(Option::take) {
        Some(p) => p,
        None => inputs.get(out).map_or(empty, |p| (*p).clone()),
    }
}
