//! Banana Split's kernels (`bv-banana`'s `kernels/*.wgsl`) on the card: a
//! shader module per family, a pipeline per kernel, all bound alike — the
//! parameters, three pictures read and one written.

use std::sync::OnceLock;

use bv_banana::graph::{Kernel, Params};
use wgpu::util::DeviceExt;

/// The pack's kernels, each compiled as it is first used (or by
/// [`crate::Gpu::warm_up`], after BananaV's own).
pub(crate) struct Kernels {
    group: wgpu::BindGroupLayout,
    layout: wgpu::PipelineLayout,
    modules: Vec<OnceLock<wgpu::ShaderModule>>,
    pipelines: Vec<OnceLock<wgpu::ComputePipeline>>,
}

impl Kernels {
    pub fn new(device: &wgpu::Device) -> Kernels {
        let storage = |binding: u32, read_only: bool| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let entries = [
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            storage(1, true),
            storage(2, true),
            storage(3, true),
            storage(4, false),
        ];
        let group = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("banana split"),
            entries: &entries,
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("banana split"),
            bind_group_layouts: &[Some(&group)],
            immediate_size: 0,
        });
        Kernels {
            group,
            layout,
            modules: bv_banana::families().map(|_| OnceLock::new()).collect(),
            pipelines: bv_banana::kernels().map(|_| OnceLock::new()).collect(),
        }
    }

    /// A kernel, compiled.
    pub fn pipeline(&self, device: &wgpu::Device, k: Kernel) -> Option<&wgpu::ComputePipeline> {
        let cell = self.pipelines.get(k.0 as usize)?;
        let (family, entry) = bv_banana::kernel(k)?;
        let module = self.modules.get(family)?.get_or_init(|| {
            let (name, source) = bv_banana::family(family).unwrap_or(("", String::new()));
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(&format!("banana split {name}")),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            })
        });
        Some(cell.get_or_init(|| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(&entry),
                layout: Some(&self.layout),
                module,
                entry_point: Some(&entry),
                compilation_options: Default::default(),
                cache: None,
            })
        }))
    }

    /// Records kernel `k`: `inputs` read, `out` written.
    pub fn run(
        &self,
        device: &wgpu::Device,
        enc: &mut wgpu::CommandEncoder,
        k: Kernel,
        params: &Params,
        inputs: [&wgpu::Buffer; 3],
        out: &wgpu::Buffer,
    ) {
        let Some(pipeline) = self.pipeline(device, k) else {
            return;
        };
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("banana split parameters"),
            contents: bytemuck::bytes_of(params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let buffers = [&uniform, inputs[0], inputs[1], inputs[2], out];
        let entries: Vec<wgpu::BindGroupEntry<'_>> = buffers
            .iter()
            .enumerate()
            .map(|(n, b)| wgpu::BindGroupEntry {
                binding: n as u32,
                resource: b.as_entire_binding(),
            })
            .collect();
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("banana split"),
            layout: &self.group,
            entries: &entries,
        });
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(bv_banana::kernel_name(k)),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(params.w.div_ceil(16), params.h.div_ceil(16), 1);
    }
}

#[cfg(test)]
mod tests {
    /// Every family is valid in the shading language with every kernel's
    /// entry point, and Metal can say it, checked without a graphics card.
    #[test]
    fn the_kernels_are_valid_and_translate_to_metal() {
        use naga::back::msl;
        for (name, source) in bv_banana::families() {
            let module = naga::front::wgsl::parse_str(&source)
                .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(&source)));
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                msl::supported_capabilities(),
            )
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
            let options = msl::Options {
                lang_version: (2, 3),
                fake_missing_bindings: true,
                ..Default::default()
            };
            let (text, _) =
                msl::write_string(&module, &info, &options, &msl::PipelineOptions::default())
                    .unwrap_or_else(|e| panic!("{name} in Metal: {e:?}"));
            assert!(text.contains("kernel void"), "{name}");
        }
        for k in bv_banana::kernels() {
            let (family, entry) = bv_banana::kernel(k).expect("a kernel");
            let (_, source) = bv_banana::family(family).expect("its family");
            assert!(source.contains(&format!("fn {entry}(")), "{entry}");
        }
    }
}
