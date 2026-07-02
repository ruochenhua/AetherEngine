//! Pipeline and resource creation for the volumetric cloud pass.

use super::shader::SHADER;
use super::textures::{create_texture_2d, create_texture_3d};
use super::types::{CloudUniform, NOISE_SIZE};
use super::VolumetricCloudPass;
use crate::scene::config::CloudQuality;
use glam::{IVec3, Vec3};
use std::borrow::Cow;
use std::mem::size_of;
use wgpu::util::DeviceExt;

/// Noise texture dimensions for a volumetric cloud quality preset.
struct NoiseSizes {
    worley: u32,
    perlin_worley: u32,
    curl: u32,
    weather: u32,
}

impl From<&CloudQuality> for NoiseSizes {
    fn from(quality: &CloudQuality) -> Self {
        match quality {
            CloudQuality::Low => Self {
                worley: 64,
                perlin_worley: 64,
                curl: 16,
                weather: 32,
            },
            CloudQuality::Medium => Self {
                worley: 128,
                perlin_worley: 128,
                curl: 16,
                weather: 64,
            },
            CloudQuality::High => Self {
                worley: 192,
                perlin_worley: 192,
                curl: 32,
                weather: 128,
            },
        }
    }
}

impl VolumetricCloudPass {
    /// Build a cloud pass without uploading the initial noise texture.
    pub(super) fn new_without_upload(device: &wgpu::Device, quality: &CloudQuality) -> Self {
        let output_format = wgpu::TextureFormat::Rgba16Float;
        let shader_source = SHADER;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Volumetric Cloud Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_source)),
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Cloud Uniform Buffer"),
            size: size_of::<CloudUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Cloud Uniform Bind Group Layout"),
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

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Cloud Uniform Bind Group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Cloud Texture Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Depth,
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
                            view_dimension: wgpu::TextureViewDimension::D3,
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
            });

        let noise_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Cloud Noise Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D3,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D3,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D3,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Volumetric Cloud Pipeline Layout"),
            bind_group_layouts: &[
                Some(&uniform_bind_group_layout),
                Some(&texture_bind_group_layout),
                Some(&noise_bind_group_layout),
            ],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Volumetric Cloud Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: size_of::<[f32; 2]>() as wgpu::BufferAddress,
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
                    format: output_format,
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

        let quad_vertices: [[f32; 2]; 6] = [
            [-1.0, -1.0],
            [1.0, -1.0],
            [1.0, 1.0],
            [-1.0, -1.0],
            [1.0, 1.0],
            [-1.0, 1.0],
        ];
        let quad_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cloud Quad Vtx"),
            contents: bytemuck::cast_slice(&quad_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let (noise_texture, noise_view) = create_texture_3d(
            device,
            NOISE_SIZE,
            wgpu::TextureFormat::R8Unorm,
            "Cloud Noise Texture",
        );

        let sizes = NoiseSizes::from(quality);
        let worley_size = sizes.worley;
        let perlin_worley_size = sizes.perlin_worley;
        let curl_size = sizes.curl;
        let weather_size = sizes.weather;

        let (worley_texture, worley_view) = create_texture_3d(
            device,
            worley_size,
            wgpu::TextureFormat::R8Unorm,
            "Cloud Worley Texture",
        );
        let (perlin_worley_texture, perlin_worley_view) = create_texture_3d(
            device,
            perlin_worley_size,
            wgpu::TextureFormat::R8Unorm,
            "Cloud Perlin-Worley Texture",
        );
        let (curl_texture, curl_view) = create_texture_3d(
            device,
            curl_size,
            wgpu::TextureFormat::Rg8Snorm,
            "Cloud Curl Texture",
        );
        let (weather_texture, weather_view) = create_texture_2d(
            device,
            weather_size,
            wgpu::TextureFormat::R8Unorm,
            "Cloud Weather Texture",
        );

        let noise_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Cloud Noise Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            ..Default::default()
        });

        let multi_noise_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Cloud Multi-Noise Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            ..Default::default()
        });

        let noise_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Cloud Noise Bind Group"),
            layout: &noise_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&worley_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&perlin_worley_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&curl_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&weather_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&multi_noise_sampler),
                },
            ],
        });

        Self {
            pipeline,
            uniform_buffer,
            uniform_bind_group,
            texture_bind_group_layout,
            texture_bind_group: None,
            noise_bind_group_layout,
            noise_bind_group,
            quad_vertex_buffer,
            quad_vertex_count: 6,
            noise_texture,
            noise_view,
            noise_sampler,
            noise_data: generate_cloud_noise_data(NOISE_SIZE),
            noise_uploaded: false,
            worley_texture,
            worley_view,
            perlin_worley_texture,
            perlin_worley_view,
            curl_texture,
            curl_view,
            weather_texture,
            weather_view,
            multi_noise_sampler,
            worley_data: crate::renderer::clouds::worley::worley_noise_3d(worley_size),
            perlin_worley_data: crate::renderer::clouds::perlin_worley::perlin_worley_noise_3d(
                perlin_worley_size,
            ),
            curl_data: crate::renderer::clouds::curl::curl_noise_3d(curl_size),
            weather_data: crate::renderer::clouds::weather::generate_weather_map_2d(weather_size),
            multi_noise_uploaded: false,
            depth_handle: None,
            cloud_color_handle: None,
            has_clouds: false,
            time: 0.0,
        }
    }
}

/// Generate cloud noise data without uploading to the GPU.
fn generate_cloud_noise_data(size: u32) -> Vec<u8> {
    let mut data = vec![0u8; (size * size * size) as usize];
    for z in 0..size {
        for y in 0..size {
            for x in 0..size {
                let p = Vec3::new(x as f32, y as f32, z as f32) / size as f32;
                let value = fbm_value_noise(p, 4, 2.0, 0.5);
                let idx = ((z * size * size) + (y * size) + x) as usize;
                data[idx] = (value.clamp(0.0, 1.0) * 255.0) as u8;
            }
        }
    }
    data
}

fn fbm_value_noise(p: Vec3, octaves: u32, lacunarity: f32, gain: f32) -> f32 {
    let mut total = 0.0;
    let mut amplitude = 0.5;
    let mut frequency = 1.0;
    let mut max_value = 0.0;
    for _ in 0..octaves {
        total += amplitude * trilinear_value_noise(p * frequency);
        max_value += amplitude;
        amplitude *= gain;
        frequency *= lacunarity;
    }
    total / max_value
}

fn trilinear_value_noise(p: Vec3) -> f32 {
    let i = p.floor().as_ivec3();
    let f = p.fract();
    let u = f * f * (3.0 - 2.0 * f);
    let mut value = 0.0;
    for z in 0..2 {
        for y in 0..2 {
            for x in 0..2 {
                let corner = i + IVec3::new(x, y, z);
                let hash = lattice_hash(corner);
                let wx = if x == 0 { 1.0 - u.x } else { u.x };
                let wy = if y == 0 { 1.0 - u.y } else { u.y };
                let wz = if z == 0 { 1.0 - u.z } else { u.z };
                value += hash * wx * wy * wz;
            }
        }
    }
    value
}

fn lattice_hash(p: IVec3) -> f32 {
    let mut n =
        p.x.wrapping_mul(374761393) ^ p.y.wrapping_mul(668265263) ^ p.z.wrapping_mul(2086444801);
    n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    n = n ^ (n >> 16);
    (n as f32 / u32::MAX as f32).clamp(0.0, 1.0)
}
