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
        let shader_source = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@location(0) pos: vec2<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(pos, 0.0, 1.0);
    out.uv = vec2<f32>(pos.x * 0.5 + 0.5, 0.5 - pos.y * 0.5);
    return out;
}

struct FXAAUniforms {
    edge_threshold: f32,
    edge_threshold_min: f32,
    subpixel_quality: f32,
    enabled: u32,
};

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var tex_sampler: sampler;

@group(1) @binding(0) var<uniform> uniforms: FXAAUniforms;

fn luma(rgb: vec3<f32>) -> f32 {
    return dot(rgb, vec3<f32>(0.299, 0.587, 0.114));
}

fn fxaa(uv: vec2<f32>) -> vec3<f32> {
    let texel_size = 1.0 / vec2<f32>(textureDimensions(input_texture));
    let uv_n = uv + vec2<f32>(0.0, texel_size.y);
    let uv_s = uv - vec2<f32>(0.0, texel_size.y);
    let uv_e = uv + vec2<f32>(texel_size.x, 0.0);
    let uv_w = uv - vec2<f32>(texel_size.x, 0.0);

    let rgb_c = textureSample(input_texture, tex_sampler, uv).rgb;
    let rgb_n = textureSample(input_texture, tex_sampler, uv_n).rgb;
    let rgb_s = textureSample(input_texture, tex_sampler, uv_s).rgb;
    let rgb_e = textureSample(input_texture, tex_sampler, uv_e).rgb;
    let rgb_w = textureSample(input_texture, tex_sampler, uv_w).rgb;

    let luma_c = luma(rgb_c);
    let luma_n = luma(rgb_n);
    let luma_s = luma(rgb_s);
    let luma_e = luma(rgb_e);
    let luma_w = luma(rgb_w);

    let luma_min = min(luma_c, min(min(luma_n, luma_s), min(luma_e, luma_w)));
    let luma_max = max(luma_c, max(max(luma_n, luma_s), max(luma_e, luma_w)));

    let luma_range = luma_max - luma_min;

    if (luma_range < max(uniforms.edge_threshold_min, luma_max * uniforms.edge_threshold)) {
        return rgb_c;
    }

    let luma_nw = luma(textureSample(input_texture, tex_sampler, uv + vec2<f32>(-texel_size.x, texel_size.y)).rgb);
    let luma_ne = luma(textureSample(input_texture, tex_sampler, uv + vec2<f32>( texel_size.x, texel_size.y)).rgb);
    let luma_sw = luma(textureSample(input_texture, tex_sampler, uv + vec2<f32>(-texel_size.x, -texel_size.y)).rgb);
    let luma_se = luma(textureSample(input_texture, tex_sampler, uv + vec2<f32>( texel_size.x, -texel_size.y)).rgb);

    let luma_ns = luma_n + luma_s;
    let luma_ew = luma_e + luma_w;
    let luma_nesw = luma_ne + luma_sw;
    let luma_nwse = luma_nw + luma_se;

    let edge_horizontal = abs(-2.0 * luma_n + luma_nesw)
                        + abs(-2.0 * luma_c + luma_nwse) * 2.0
                        + abs(-2.0 * luma_s + luma_nesw);
    let edge_vertical   = abs(-2.0 * luma_e + luma_nwse)
                        + abs(-2.0 * luma_c + luma_nesw) * 2.0
                        + abs(-2.0 * luma_w + luma_nwse);

    let is_horizontal = edge_horizontal >= edge_vertical;

    let luma1 = select(luma_e, luma_n, is_horizontal);
    let luma2 = select(luma_w, luma_s, is_horizontal);
    let gradient1 = luma1 - luma_c;
    let gradient2 = luma2 - luma_c;

    let is_1_steepest = abs(gradient1) >= abs(gradient2);
    let gradient_scaled = 0.25 * max(abs(gradient1), abs(gradient2));

    let step_length = select(texel_size.x, texel_size.y, is_horizontal);
    var uv_offset = vec2<f32>(0.0);
    if (is_horizontal) {
        uv_offset.y = step_length * 0.5;
    } else {
        uv_offset.x = step_length * 0.5;
    }

    var luma_local_average = 0.0;
    if (is_1_steepest) {
        uv_offset = -uv_offset;
        luma_local_average = 0.5 * (luma1 + luma_c);
    } else {
        luma_local_average = 0.5 * (luma2 + luma_c);
    }

    var uv_p = uv + uv_offset;
    var uv_m = uv - uv_offset;

    var luma_end_p = 0.0;
    var luma_end_m = 0.0;
    var done_p = false;
    var done_m = false;

    // Search along edge direction (simplified: 4 steps)
    for (var i: i32 = 0; i < 4; i = i + 1) {
        if (!done_p) {
            luma_end_p = luma(textureSample(input_texture, tex_sampler, uv_p).rgb);
            done_p = abs(luma_end_p - luma_local_average) >= gradient_scaled;
            if (!done_p) {
                uv_p += uv_offset;
            }
        }
        if (!done_m) {
            luma_end_m = luma(textureSample(input_texture, tex_sampler, uv_m).rgb);
            done_m = abs(luma_end_m - luma_local_average) >= gradient_scaled;
            if (!done_m) {
                uv_m -= uv_offset;
            }
        }
    }

    let distance_p = select(abs(uv_p.x - uv.x), abs(uv_p.y - uv.y), is_horizontal);
    let distance_m = select(abs(uv_m.x - uv.x), abs(uv_m.y - uv.y), is_horizontal);

    let edge_thickness = distance_p + distance_m;
    // Standard FXAA 3.11 offset: positive → shift toward positive, negative → toward negative
    let pixel_offset = 0.5 * (distance_m - distance_p) / edge_thickness;

    let luma_average = (1.0 / 12.0) * (2.0 * (luma_n + luma_s + luma_e + luma_w) + luma_nw + luma_ne + luma_sw + luma_se);
    let subpixel_offset = abs(luma_average - luma_c) / luma_range;
    let subpixel_offset_clamped = max(-2.0 * subpixel_offset + 1.0, 0.0);
    let subpixel_offset_final = subpixel_offset_clamped * subpixel_offset_clamped * uniforms.subpixel_quality;

    // Pick the larger magnitude; preserve pixel_offset sign for subpixel case
    let final_offset = select(
        sign(pixel_offset) * subpixel_offset_final,
        pixel_offset,
        abs(pixel_offset) >= subpixel_offset_final,
    );

    var uv_final = uv;
    if (is_horizontal) {
        uv_final.y += final_offset * step_length;
    } else {
        uv_final.x += final_offset * step_length;
    }

    return textureSample(input_texture, tex_sampler, uv_final).rgb;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if (uniforms.enabled == 0u) {
        return vec4<f32>(textureSample(input_texture, tex_sampler, in.uv).rgb, 1.0);
    }
    return vec4<f32>(fxaa(in.uv), 1.0);
}
"#;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn headless_device() -> wgpu::Device {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("need adapter");
        let (device, _) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .expect("need device");
        device
    }

    #[test]
    fn signature_declares_reads() {
        let device = headless_device();
        let pass = FXAAPass::new(&device, wgpu::TextureFormat::Bgra8UnormSrgb);
        let sig = pass.signature();
        assert_eq!(sig.name, "FXAA");
        assert_eq!(sig.reads.len(), 1);
        assert_eq!(sig.writes.len(), 1);
    }

    #[test]
    fn init_creates_resources() {
        let _pass = FXAAPass::new(&headless_device(), wgpu::TextureFormat::Bgra8UnormSrgb);
    }

    #[test]
    fn default_quality_is_high() {
        let device = headless_device();
        let pass = FXAAPass::new(&device, wgpu::TextureFormat::Bgra8UnormSrgb);
        assert_eq!(pass.quality, FxaaQuality::High);
    }

    #[test]
    fn quality_can_be_changed() {
        let device = headless_device();
        let mut pass = FXAAPass::new(&device, wgpu::TextureFormat::Bgra8UnormSrgb);
        pass.set_quality(FxaaQuality::Low);
        assert_eq!(pass.quality, FxaaQuality::Low);
        pass.set_quality(FxaaQuality::Medium);
        assert_eq!(pass.quality, FxaaQuality::Medium);
    }
}
