//! Water pass pipeline and mesh creation.
//!
//! Creates the render pipeline, bind group layouts, uniform buffer,
//! and the subdivided water plane mesh used by [`WaterPass`].

use super::WaterPass;
use super::WaterUniform;
use crate::asset::mesh::{CpuMesh, GpuMesh, Vertex};
use std::borrow::Cow;
use std::mem::size_of;
use std::sync::Arc;

impl WaterPass {
    /// Create a new water pass.
    pub fn new(device: &wgpu::Device) -> Self {
        let output_format = wgpu::TextureFormat::Rgba16Float;
        let shader_source = r#"
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

struct WaterUniform {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    water_color: vec4<f32>,
    deep_color: vec4<f32>,
    wave_direction: vec2<f32>,
    wave_amplitude: f32,
    wave_wavelength: f32,
    wave_speed: f32,
    wave_steepness: f32,
    time: f32,
    level: f32,
    fresnel_power: f32,
    refraction_scale: f32,
    reflectivity: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    _pad3: f32,
    _pad4: f32,
    _pad5: f32,
    _pad6: f32,
    _pad7: f32,
    _pad8: f32,
};

@group(0) @binding(0) var<uniform> water: WaterUniform;
@group(1) @binding(0) var scene_color: texture_2d<f32>;
@group(1) @binding(1) var reflection_texture: texture_2d<f32>;
@group(1) @binding(2) var tex_sampler: sampler;

const PI: f32 = 3.14159265359;

fn gerstner_displacement(position: vec2<f32>, time: f32) -> vec3<f32> {
    var dir = normalize(water.wave_direction);
    if (length(dir) < 0.0001) {
        dir = vec2<f32>(1.0, 0.0);
    }
    let w = 2.0 * PI / max(water.wave_wavelength, 0.001);
    let phi = water.wave_speed * w * time;
    let q = water.wave_steepness;
    let theta = w * dot(dir, position) + phi;
    let c = cos(theta);
    let s = sin(theta);
    let amp = water.wave_amplitude;
    return vec3<f32>(
        q * amp * dir.x * c,
        amp * s,
        q * amp * dir.y * c
    );
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let base_pos = in.position;
    let displaced = base_pos + gerstner_displacement(base_pos.xz, water.time);
    let world_pos = vec3<f32>(displaced.x, displaced.y + water.level, displaced.z);
    out.clip_position = water.view_proj * vec4<f32>(world_pos, 1.0);
    out.world_pos = world_pos;
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(scene_color, 0));
    let screen_uv = in.clip_position.xy / dims;

    // Reconstruct surface normal from position derivatives.
    let ddx = dpdx(in.world_pos);
    let ddy = dpdy(in.world_pos);
    let normal = normalize(cross(ddx, ddy));

    let view_dir = normalize(water.camera_pos.xyz - in.world_pos);
    let n_dot_v = max(dot(normal, view_dir), 0.0);
    let fresnel = pow(1.0 - n_dot_v, water.fresnel_power);

    // Refraction: sample the lit scene behind the water surface, distorted by the normal.
    let refract_uv = clamp(
        screen_uv + normal.xz * water.refraction_scale,
        vec2<f32>(0.0),
        vec2<f32>(1.0)
    );
    let refract_color = textureSample(scene_color, tex_sampler, refract_uv).rgb;
    let water_tint = mix(water.water_color, water.deep_color, 0.5).rgb;
    let refracted = refract_color * water_tint;

    // Reflection: sample SSR reflection texture, also distorted slightly.
    let reflect_uv = clamp(
        screen_uv + normal.xz * water.refraction_scale * 0.5,
        vec2<f32>(0.0),
        vec2<f32>(1.0)
    );
    let reflection = textureSample(reflection_texture, tex_sampler, reflect_uv).rgb;

    let final_color = mix(refracted, reflection, fresnel * water.reflectivity);
    let alpha = mix(0.6, 0.95, fresnel);

    return vec4<f32>(final_color, alpha);
}
"#;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Water Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_source)),
        });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Water Uniform Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let texture_bind_group_layout = create_texture_bind_group_layout(device);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Water Pipeline Layout"),
            bind_group_layouts: &[
                Some(&uniform_bind_group_layout),
                Some(&texture_bind_group_layout),
            ],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Water Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Vertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: output_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Water Uniform Buffer"),
            size: size_of::<WaterUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Water Uniform Bind Group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let mesh = Arc::new(GpuMesh::from_cpu(device, &create_water_plane(128, 256.0)));

        Self {
            pipeline,
            uniform_buffer,
            uniform_bind_group,
            texture_bind_group: None,
            mesh,
            scene_color_handle: None,
            reflection_handle: None,
            depth_handle: None,
            water_color_handle: None,
            has_water: false,
            time: 0.0,
        }
    }
}

/// Create the texture bind group layout shared by the water pipeline and
/// the per-frame texture bind group.
pub(super) fn create_texture_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Water Texture Bind Group Layout"),
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
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

/// Create a subdivided XZ-plane CPU mesh centered at the origin.
fn create_water_plane(subdivisions: u32, extent: f32) -> CpuMesh {
    let segments = subdivisions.max(1);
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();

    for z in 0..=segments {
        for x in 0..=segments {
            let u = x as f32 / segments as f32;
            let v = z as f32 / segments as f32;
            let px = (u - 0.5) * extent;
            let pz = (v - 0.5) * extent;
            positions.push([px, 0.0, pz]);
            normals.push([0.0, 1.0, 0.0]);
            uvs.push([u * 4.0, v * 4.0]);
        }
    }

    for z in 0..segments {
        for x in 0..segments {
            let i0 = z * (segments + 1) + x;
            let i1 = i0 + 1;
            let i2 = (z + 1) * (segments + 1) + x;
            let i3 = i2 + 1;
            indices.push(i0);
            indices.push(i2);
            indices.push(i1);
            indices.push(i1);
            indices.push(i2);
            indices.push(i3);
        }
    }

    CpuMesh {
        positions,
        normals,
        uvs,
        tangents: Vec::new(),
        indices,
    }
}
