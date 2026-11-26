//! What an effect draws: kernels run one after another on pictures, the
//! same steps for the graphics card (`bv-gpu`'s `compositor/banana.rs`) and
//! the processor ([`crate::cpu`]).

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};

/// One of the kernels (`generated.rs`' `k::…`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Kernel(pub u16);

/// How many rows of numbers a kernel takes.
pub const ROWS: usize = 12;

/// What a kernel reads besides its pictures, as the shaders' `Params`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct Params {
    /// The size written.
    pub w: u32,
    pub h: u32,
    /// The sizes of the pictures read.
    pub w0: u32,
    pub h0: u32,
    pub w1: u32,
    pub h1: u32,
    pub w2: u32,
    pub h2: u32,
    /// Pixels drawn for each of the full frame's (a preview's are fewer).
    pub scale: f32,
    /// The frame's width over its height, as shown.
    pub shape: f32,
    /// Seconds from the effect's start.
    pub time: f32,
    pub seed: u32,
    pub i: [i32; 4],
    pub f: [[f32; 4]; ROWS],
}

impl Params {
    pub const ZERO: Params = Params {
        w: 0,
        h: 0,
        w0: 0,
        h0: 0,
        w1: 0,
        h1: 0,
        w2: 0,
        h2: 0,
        scale: 1.0,
        shape: 1.0,
        time: 0.0,
        seed: 0,
        i: [0; 4],
        f: [[0.0; 4]; ROWS],
    };
}

/// A picture of the graph: 0 the effect's (a transition's first), 1 a
/// transition's second, then the other frames it reads, then what its
/// steps draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Slot(pub u16);

/// A kernel run on pictures.
#[derive(Clone, Debug, PartialEq)]
pub struct Pass {
    pub kernel: Kernel,
    /// The pictures read (the first again for those a kernel does not
    /// read).
    pub inputs: [Slot; 3],
    /// Where it draws, and how big.
    pub out: Slot,
    pub params: Params,
}

/// A picture worked out with the steps (a path's places), for kernels to
/// read: never drawn over.
#[derive(Clone, Debug, PartialEq)]
pub struct Data {
    pub slot: Slot,
    pub size: [u32; 2],
    pub pixels: Arc<[[f32; 4]]>,
}

/// An effect's steps.
#[derive(Clone, Debug, PartialEq)]
pub struct Graph {
    /// How many pictures it gets.
    pub inputs: u16,
    /// How many pictures there are in all.
    pub slots: u16,
    pub data: Vec<Data>,
    pub passes: Vec<Pass>,
    /// What it comes to.
    pub out: Slot,
}

impl Graph {
    /// For each picture, the last pass reading it (`None`: none does).
    pub fn last_reads(&self) -> Vec<Option<usize>> {
        let mut last = vec![None; self.slots as usize];
        for (n, p) in self.passes.iter().enumerate() {
            for s in p.inputs {
                if let Some(l) = last.get_mut(s.0 as usize) {
                    *l = Some(n);
                }
            }
        }
        last
    }
}
