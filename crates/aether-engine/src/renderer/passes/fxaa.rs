//! FXAA Pass — Fast Approximate Anti-Aliasing
//!
//! Final post-process step before debug overlay. Detects high-contrast edges
//! in the tone-mapped LDR image and blends along edge directions to reduce
//! geometric aliasing.
//!
//! Pipeline position: ToneMappingPass → FXAAPass → DebugLinePass

use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{InitContext, Pass, PassSignature, ResHandle};
use crate::renderer::resource::*;
use crate::renderer::resource_table::ResourceTable;
use std::borrow::Cow;
use wgpu::util::DeviceExt;

mod shaders;
#[cfg(test)]
mod tests;

/// FXAA quality preset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FxaaQuality {
    /// Low quality — faster, less smoothing.
    Low,
    /// Medium quality — balanced.
    Medium,
    /// High quality — more edge detection passes.
    #[default]
    High,
}

/// Uniforms for FXAA shader.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct FXAAUniforms {
    edge_threshold: f32,
    edge_threshold_min: f32,
    subpixel_quality: f32,
    enabled: u32,
}

/// FXAA Pass implementation.
pub struct FXAAPass {
    pipeline: wgpu::RenderPipeline,
    quad_vertex_buffer: wgpu::Buffer,
    quad_vertex_count: u32,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    input_handle: Option<ResHandle<FxaaInput>>,
    texture_bind_group: Option<wgpu::BindGroup>,
    sampler: wgpu::Sampler,
    quality: FxaaQuality,
    edge_threshold: Option<f32>,
    enabled: bool,
    surface_format: wgpu::TextureFormat,
}

impl Pass for FXAAPass {
    fn name(&self) -> &str {
        "FXAA"
    }

    fn signature(&self) -> PassSignature {
        PassSignature::new("FXAA")
            .read::<FxaaInput>()
            .write::<Swapchain>(self.surface_format)
    }

    fn init(ctx: &InitContext) -> Self {
        Self::new(ctx.device, ctx.surface_format)
    }

    fn resolve(&mut self, device: &wgpu::Device, resources: &ResourceTable) {
        self.input_handle = Some(resources.handle::<FxaaInput>());
        let input_view = resources.get(self.input_handle.unwrap());

        self.texture_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("FXAA Texture Bind Group"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(input_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        }));
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        self.set_enabled(frame.config.fxaa_enabled);
        self.set_quality(frame.config.fxaa_quality);
        self.set_edge_threshold(frame.config.fxaa_edge_threshold);
        self.update_uniforms_with_queue(frame.queue);
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        _resources: &ResourceTable,
        surface_view: &wgpu::TextureView,
    ) {
        let texture_bg = self
            .texture_bind_group
            .as_ref()
            .expect("FXAAPass: resolve not called");

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("FXAA Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: surface_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, texture_bg, &[]);
        pass.set_bind_group(1, &self.uniform_bind_group, &[]);
        pass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
        pass.draw(0..self.quad_vertex_count, 0..1);
    }
}

impl FXAAPass {
    /// Create a new FXAA pass.
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let shader_source = shaders::FXAA_SHADER_SRC;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("FXAA Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_source)),
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("FXAA Texture BGL"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("FXAA Uniform BGL"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("FXAA Pipeline Layout"),
            bind_group_layouts: &[
                Some(&texture_bind_group_layout),
                Some(&uniform_bind_group_layout),
            ],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("FXAA Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: wgpu::VertexFormat::Float32x2,
                    }],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("FXAA Uniform Buffer"),
            size: std::mem::size_of::<FXAAUniforms>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("FXAA Uniform BG"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let quad_vertices: [[f32; 2]; 6] = [
            [-1.0, -1.0],
            [1.0, -1.0],
            [1.0, 1.0],
            [-1.0, -1.0],
            [1.0, 1.0],
            [-1.0, 1.0],
        ];

        let quad_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("FXAA Quad Vtx"),
            contents: bytemuck::cast_slice(&quad_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("FXAA Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            pipeline,
            quad_vertex_buffer,
            quad_vertex_count: 6,
            uniform_buffer,
            uniform_bind_group,
            texture_bind_group_layout,
            input_handle: None,
            texture_bind_group: None,
            sampler,
            quality: FxaaQuality::default(),
            edge_threshold: None,
            enabled: true,
            surface_format,
        }
    }

    /// Set FXAA quality preset.
    pub fn set_quality(&mut self, quality: FxaaQuality) {
        self.quality = quality;
        // Reset custom threshold so the preset takes effect.
        self.edge_threshold = None;
    }

    /// Set a custom edge threshold. Pass `None` to fall back to the current
    /// quality preset's default.
    pub fn set_edge_threshold(&mut self, threshold: Option<f32>) {
        self.edge_threshold = threshold;
    }

    /// Enable/disable FXAA.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn quality_defaults(quality: FxaaQuality) -> (f32, f32, f32) {
        match quality {
            FxaaQuality::Low => (0.063, 0.0, 0.75),
            FxaaQuality::Medium => (0.031, 0.0, 0.5),
            FxaaQuality::High => (0.016, 0.0, 0.25),
        }
    }

    /// Update uniform buffer with a queue reference.
    pub fn update_uniforms_with_queue(&self, queue: &wgpu::Queue) {
        let (default_threshold, edge_threshold_min, subpixel_quality) =
            Self::quality_defaults(self.quality);
        let edge_threshold = self.edge_threshold.unwrap_or(default_threshold);

        let uniforms = FXAAUniforms {
            edge_threshold,
            edge_threshold_min,
            subpixel_quality,
            enabled: if self.enabled { 1 } else { 0 },
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
    }
}
