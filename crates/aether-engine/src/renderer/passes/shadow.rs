//! Cascaded Shadow Map (CSM) pass.
//!
//! Renders scene depth from the directional light's perspective into a
//! layered depth texture. Each layer corresponds to one cascade covering
//! a subset of the camera frustum.
//!
//! ## Known Pitfalls
//! - **Depth-only rendering**: No fragment shader needed; use `fragment: None`.
//!   GPU derives depth from `@builtin(position)`. Manually writing
//!   `@builtin(frag_depth)` is error-prone (e.g., returning 0.0).
//! - **Per-object draw order**: Do NOT use `queue.write_buffer` inside the
//!   render pass to update per-object uniforms; Metal may serve stale data.
//!   Pre-upload all instance data to a dynamic uniform buffer before the
//!   render pass, then switch via `set_bind_group(offset)` + draw per batch.

use crate::asset::mesh::{InstanceData, Vertex};
use crate::renderer::extract::RenderBatch;
use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{InitContext, Pass, PassSignature, ResHandle};
use crate::renderer::resource::*;
use crate::renderer::resource_table::ResourceTable;
use crate::terrain::{ChunkInstanceData, TerrainGeometry};
use glam::{Mat4, Vec4Swizzles};
use std::sync::{Arc, RwLock};

/// Number of cascades.
pub const CASCADE_COUNT: usize = 4;
/// Fixed resolution for each cascade depth map.
pub const SHADOW_MAP_SIZE: u32 = 4096;

/// Per-cascade light-space matrix.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CascadeUniform {
    /// Combined light view-projection matrix for this cascade.
    pub light_view_proj: [[f32; 4]; 4],
}

/// CPU-side cascade data.
#[derive(Clone, Copy, Debug)]
pub struct Cascade {
    /// Light view-projection matrix.
    pub view_proj: Mat4,
    /// Far split distance in view space.
    pub split_depth: f32,
}

/// Cascaded shadow map pass.
pub struct ShadowPass {
    device: wgpu::Device,
    pipeline: wgpu::RenderPipeline,
    cascade_uniform_buffer: wgpu::Buffer,
    cascade_bind_group: wgpu::BindGroup,
    /// Per-instance transform vertex buffer.
    instance_buffer: wgpu::Buffer,
    instance_buffer_capacity: usize,
    shadow_depth_handle: Option<ResHandle<ShadowDepth>>,
    cascade_views: Vec<wgpu::TextureView>,
    batches: Arc<[RenderBatch]>,
    cascades: [Cascade; CASCADE_COUNT],
    terrain_geometry: Option<Arc<RwLock<TerrainGeometry>>>,
}

impl Pass for ShadowPass {
    fn name(&self) -> &str {
        "Shadow"
    }
    fn signature(&self) -> PassSignature {
        PassSignature::new("Shadow").write_array::<ShadowDepth>(
            wgpu::TextureFormat::Depth32Float,
            SHADOW_MAP_SIZE,
            SHADOW_MAP_SIZE,
            CASCADE_COUNT as u32,
        )
    }
    fn init(ctx: &InitContext) -> Self {
        Self::new(ctx.device)
    }
    fn resolve(&mut self, _device: &wgpu::Device, resources: &ResourceTable) {
        self.shadow_depth_handle = Some(resources.handle::<ShadowDepth>());
        let texture = resources
            .texture(self.shadow_depth_handle.unwrap())
            .expect("ShadowDepth must own its texture");
        self.cascade_views.clear();
        for i in 0..CASCADE_COUNT {
            self.cascade_views
                .push(texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some(&format!("Shadow Cascade {}", i)),
                    format: Some(wgpu::TextureFormat::Depth32Float),
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    aspect: wgpu::TextureAspect::All,
                    base_mip_level: 0,
                    mip_level_count: Some(1),
                    base_array_layer: i as u32,
                    array_layer_count: Some(1),
                    ..Default::default()
                }));
        }
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        self.batches = frame.batches.clone();
        self.terrain_geometry = frame.terrain_geometry.clone();
        let light_dir = glam::Vec3::from_array(frame.lighting.light.direction).normalize();

        self.cascades = compute_cascades(frame, &light_dir);

        let aligned = Self::aligned_cascade_uniform_size();
        for (i, cascade) in self.cascades.iter().enumerate() {
            let uniform = CascadeUniform {
                light_view_proj: cascade.view_proj.to_cols_array_2d(),
            };
            frame.queue.write_buffer(
                &self.cascade_uniform_buffer,
                (i * aligned) as u64,
                bytemuck::cast_slice(&[uniform]),
            );
        }

        // Upload instance data
        let total_instances: usize = self.batches.iter().map(|b| b.instances.len()).sum();
        if total_instances > self.instance_buffer_capacity {
            let new_capacity = total_instances.max(256);
            self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Shadow Instance Buf"),
                size: (new_capacity * std::mem::size_of::<InstanceData>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.instance_buffer_capacity = new_capacity;
        }
        let mut instance_data: Vec<u8> =
            Vec::with_capacity(total_instances * std::mem::size_of::<InstanceData>());
        for batch in self.batches.iter() {
            instance_data.extend_from_slice(bytemuck::cast_slice(&batch.instances));
        }
        if !instance_data.is_empty() {
            frame
                .queue
                .write_buffer(&self.instance_buffer, 0, &instance_data);
        }
    }

    fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        _resources: &ResourceTable,
        _surface_view: &wgpu::TextureView,
    ) {
        let aligned_size = Self::aligned_cascade_uniform_size() as u32;
        for (i, view) in self.cascade_views.iter().enumerate() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(&format!("Shadow Cascade {}", i)),
                color_attachments: &[],
                multiview_mask: None,
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.cascade_bind_group, &[(i as u32 * aligned_size)]);

            let mut instance_offset = 0usize;
            for batch in self.batches.iter() {
                let instance_count = batch.instances.len() as u32;
                if instance_count == 0 || batch.mesh.vertex_count == 0 {
                    continue;
                }

                pass.set_vertex_buffer(0, batch.mesh.vertex_buffer.slice(..));
                let instance_byte_start =
                    (instance_offset * std::mem::size_of::<InstanceData>()) as wgpu::BufferAddress;
                let instance_byte_end = instance_byte_start
                    + (batch.instances.len() * std::mem::size_of::<InstanceData>())
                        as wgpu::BufferAddress;
                pass.set_vertex_buffer(
                    1,
                    self.instance_buffer
                        .slice(instance_byte_start..instance_byte_end),
                );
                if let Some(ref ib) = batch.mesh.index_buffer {
                    pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    let start = batch.mesh.index_offset;
                    let end = start + batch.mesh.index_count;
                    pass.draw_indexed(start..end, 0, 0..instance_count);
                } else {
                    pass.draw(0..batch.mesh.vertex_count, 0..instance_count);
                }
                instance_offset += batch.instances.len();
            }

            // Render terrain chunks into the shadow map so terrain casts shadows.
            if let Some(terrain_geometry) = &self.terrain_geometry {
                let terrain = terrain_geometry.read().unwrap();
                let chunks = terrain.chunks();
                let chunk_meshes = terrain.chunk_meshes();
                let instance_buffer = terrain.instance_buffer();
                pass.set_vertex_buffer(1, instance_buffer.slice(..));
                for (chunk_index, chunk) in chunks.iter().enumerate() {
                    let lod_mesh = &chunk_meshes[chunk_index][chunk.lod as usize];
                    let instance_start =
                        (chunk_index * std::mem::size_of::<ChunkInstanceData>())
                            as wgpu::BufferAddress;
                    let instance_end =
                        instance_start + std::mem::size_of::<ChunkInstanceData>()
                            as wgpu::BufferAddress;
                    pass.set_vertex_buffer(0, lod_mesh.vertex_buffer.slice(..));
                    pass.set_vertex_buffer(1, instance_buffer.slice(instance_start..instance_end));
                    if let Some(ref ib) = lod_mesh.index_buffer {
                        pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..lod_mesh.index_count, 0, 0..1);
                    } else {
                        pass.draw(0..lod_mesh.vertex_count, 0..1);
                    }
                }
            }
        }
    }
}

impl ShadowPass {
    /// Create a new cascaded shadow pass.
    pub fn new(device: &wgpu::Device) -> Self {
        let src = r#"
struct VertexInput { @location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>, @location(3) tangent: vec4<f32>, };
struct InstanceInput {
    @location(4) model_matrix_0: vec4<f32>,
    @location(5) model_matrix_1: vec4<f32>,
    @location(6) model_matrix_2: vec4<f32>,
    @location(7) model_matrix_3: vec4<f32>,
    @location(8) entity_id: u32,
};
struct VertexOutput { @builtin(position) clip_position: vec4<f32>, };
struct LightVP { light_view_proj: mat4x4<f32>, };
@group(0) @binding(0) var<uniform> lvp: LightVP;
@vertex
fn vs_main(in: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    let model = mat4x4<f32>(instance.model_matrix_0, instance.model_matrix_1, instance.model_matrix_2, instance.model_matrix_3);
    out.clip_position = lvp.light_view_proj * model * vec4<f32>(in.position, 1.0);
    return out;
}
"#;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shadow Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(src)),
        });

        let vp_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("S VP BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shadow PL"),
            bind_group_layouts: &[Some(&vp_bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Shadow Pipeline"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Vertex::desc(), InstanceData::instance_desc()],
            },
            fragment: None,
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 0,
                    slope_scale: 0.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let aligned_size = Self::aligned_cascade_uniform_size();
        let vp_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("S VP"),
            size: (CASCADE_COUNT as u64) * aligned_size as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let vp_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("S VP BG"),
            layout: &vp_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &vp_buf,
                    offset: 0,
                    size: Some(
                        std::num::NonZeroU64::new(std::mem::size_of::<CascadeUniform>() as u64)
                            .unwrap(),
                    ),
                }),
            }],
        });

        let initial_instance_capacity = 256;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Shadow Instance Buf"),
            size: (initial_instance_capacity * std::mem::size_of::<InstanceData>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            device: device.clone(),
            pipeline,
            cascade_uniform_buffer: vp_buf,
            cascade_bind_group: vp_bg,
            instance_buffer,
            instance_buffer_capacity: initial_instance_capacity,
            shadow_depth_handle: None,
            cascade_views: Vec::new(),
            batches: Arc::from([]),
            cascades: [Cascade {
                view_proj: Mat4::IDENTITY,
                split_depth: 0.0,
            }; CASCADE_COUNT],
            terrain_geometry: None,
        }
    }

    /// Return the CPU-side cascade data computed for the current frame.
    pub fn cascades(&self) -> &[Cascade; CASCADE_COUNT] {
        &self.cascades
    }

    fn aligned_cascade_uniform_size() -> usize {
        let size = std::mem::size_of::<CascadeUniform>();
        let alignment = 256; // wgpu uniform dynamic offset alignment
        size.div_ceil(alignment) * alignment
    }
}

/// Compute wgpu-compatible orthographic projection (z ∈ [0, 1]).
fn ortho_wgpu(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> Mat4 {
    let mut m = Mat4::IDENTITY;
    m.col_mut(0)[0] = 2.0 / (right - left);
    m.col_mut(1)[1] = 2.0 / (top - bottom);
    m.col_mut(2)[2] = -1.0 / (far - near);
    m.col_mut(3)[0] = -(right + left) / (right - left);
    m.col_mut(3)[1] = -(top + bottom) / (top - bottom);
    m.col_mut(3)[2] = -near / (far - near);
    m
}

/// Compute the camera frustum corners for a view-space depth slice.
///
/// `cam_near`/`cam_far` are the camera's clip planes, and `slice_near`/
/// `slice_far` are the positive view-space distances that bound the slice.
/// The projection matrix must map z into [0, 1] (wgpu perspective).
fn frustum_corners(
    view: &Mat4,
    proj: &Mat4,
    cam_near: f32,
    cam_far: f32,
    slice_near: f32,
    slice_far: f32,
) -> [glam::Vec3; 8] {
    let inv_view_proj = (proj.mul_mat4(view)).inverse();
    let mut corners = [glam::Vec3::ZERO; 8];
    let mut i = 0;
    // perspective_rh with z ∈ [0, 1] maps view distance d to
    // z_ndc = far * (d - near) / (d * (far - near)).
    for d in [slice_near, slice_far] {
        let z_ndc = cam_far * (d - cam_near) / (d * (cam_far - cam_near));
        for y in [-1.0f32, 1.0] {
            for x in [-1.0f32, 1.0] {
                let clip = glam::Vec4::new(x, y, z_ndc, 1.0);
                let world = inv_view_proj * clip;
                corners[i] = world.xyz() / world.w;
                i += 1;
            }
        }
    }
    corners
}

/// Compute practical split distances for cascaded shadow maps.
fn split_distances(near: f32, far: f32, count: usize, lambda: f32) -> Vec<f32> {
    let mut splits = Vec::with_capacity(count);
    for i in 1..=count {
        let t = i as f32 / count as f32;
        let log = near * (far / near).powf(t);
        let uniform = near + (far - near) * t;
        splits.push(lambda * log + (1.0 - lambda) * uniform);
    }
    splits
}

/// Compute all cascades for the current frame.
pub fn compute_cascades(frame: &RenderFrame, light_dir: &glam::Vec3) -> [Cascade; CASCADE_COUNT] {
    let camera = &frame.camera;
    let view = camera.view_matrix();
    let proj = camera.projection_matrix(frame.aspect);

    let near = camera.near;
    let far = camera.far;
    let splits = split_distances(near, far, CASCADE_COUNT, 0.75);

    let mut cascades = [Cascade {
        view_proj: Mat4::IDENTITY,
        split_depth: 0.0,
    }; CASCADE_COUNT];

    let mut prev_far = near;
    for (i, split) in splits.iter().enumerate() {
        let cascade_far = *split;
        let corners = frustum_corners(&view, &proj, near, far, prev_far, cascade_far);
        cascades[i] = compute_cascade(light_dir, &corners, cascade_far, far);
        prev_far = cascade_far;
    }

    cascades
}

/// Compute a single cascade matrix from world-space frustum corners.
///
/// All corners lie inside the resulting orthographic cube. The near/far
/// planes are extended by `cam_far` to capture shadow casters far outside
/// the cascade frustum — essential for low-angle directional lights.
/// XY bounds are snapped to texel boundaries to prevent cascade seams.
fn compute_cascade(
    light_dir: &glam::Vec3,
    corners: &[glam::Vec3],
    split_depth: f32,
    cam_far: f32,
) -> Cascade {
    // Pick 'up' vector orthogonal to light direction.
    let up = if light_dir.x.abs() < 0.001 && light_dir.z.abs() < 0.001 {
        glam::Vec3::X
    } else {
        glam::Vec3::Y
    };

    // Light-space view: look along light_dir. Position at origin — only
    // the rotation matters for AABB computation.
    let light_view = Mat4::look_at_rh(glam::Vec3::ZERO, *light_dir, up);

    // Transform frustum corners to light space and compute AABB.
    let mut min_ls = glam::Vec3::splat(f32::MAX);
    let mut max_ls = glam::Vec3::splat(f32::MIN);
    for corner in corners {
        let ls = light_view.transform_point3a(glam::Vec3A::from(*corner));
        min_ls = min_ls.min(ls.into());
        max_ls = max_ls.max(ls.into());
    }

    // Extend depth: push near-z back by the camera far plane to capture
    // all potential occluders, even those far outside the cascade frustum.
    // This is critical for low-angle directional lights where shadows can
    // extend many times the cascade depth range.
    // max_ls.z is extended by half the far plane to capture tall casters
    // above the frustum that are closer to the light source.
    min_ls.z -= cam_far;
    max_ls.z += cam_far * 0.5;

    // Expand XY margins by 2 pixels to avoid edge clipping.
    let texel_size = (max_ls.x - min_ls.x) / SHADOW_MAP_SIZE as f32;
    min_ls.x -= texel_size * 2.0;
    max_ls.x += texel_size * 2.0;
    min_ls.y -= texel_size * 2.0;
    max_ls.y += texel_size * 2.0;
    // Snap to texel boundaries to prevent cascade seams.
    let snap = |v: f32| (v / texel_size).floor() * texel_size;
    min_ls.x = snap(min_ls.x);
    max_ls.x = snap(max_ls.x);
    min_ls.y = snap(min_ls.y);
    max_ls.y = snap(max_ls.y);

    let proj = ortho_wgpu(min_ls.x, max_ls.x, min_ls.y, max_ls.y, min_ls.z, max_ls.z);

    Cascade {
        view_proj: proj * light_view,
        split_depth,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::camera::FlyCamera;
    use crate::renderer::frame::{FrameConfig, RenderFrame};
    use crate::renderer::light::LightingUniforms;

    fn headless_queue() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("need adapter");
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("need device")
    }

    fn build_frame<'a>(
        camera: &'a FlyCamera,
        lighting: &'a LightingUniforms,
        queue: &'a wgpu::Queue,
        optional: &'a crate::renderer::extract::OptionalPassData,
        config: &'a FrameConfig,
        texture_cache: &'a crate::asset::texture_cache::GpuTextureCache,
        asset_manager: &'a crate::asset::AssetManager,
    ) -> RenderFrame<'a> {
        RenderFrame {
            camera,
            aspect: 16.0 / 9.0,
            batches: std::sync::Arc::from([]),
            lighting,
            queue,
            delta_time: 0.0,
            config,
            optional,
            terrain_geometry: None,
            texture_cache,
            asset_manager,
        }
    }

    #[test]
    fn compute_cascades_produces_valid_matrices() {
        let camera = FlyCamera::default();
        let lighting = LightingUniforms::default();
        let (device, queue) = headless_queue();
        let texture_cache = crate::asset::texture_cache::GpuTextureCache::new(&device, &queue);
        let asset_manager = crate::asset::AssetManager::new();
        let optional = crate::renderer::extract::OptionalPassData::default();
        let config = FrameConfig::default();
        let frame = build_frame(
            &camera,
            &lighting,
            &queue,
            &optional,
            &config,
            &texture_cache,
            &asset_manager,
        );
        let light_dir = glam::Vec3::new(0.5, -1.0, 0.3).normalize();
        let cascades = compute_cascades(&frame, &light_dir);
        for cascade in &cascades {
            for c in 0..4 {
                for r in 0..4 {
                    assert!(!cascade.view_proj.col(c)[r].is_nan());
                }
            }
            assert!(cascade.split_depth > 0.0);
        }
        assert!(cascades[0].split_depth <= cascades[1].split_depth);
        assert!(cascades[1].split_depth <= cascades[2].split_depth);
        drop(device);
    }

    #[test]
    fn split_distances_increase_monotonically() {
        let splits = split_distances(0.1, 100.0, 3, 0.5);
        assert_eq!(splits.len(), 3);
        assert!(splits[0] < splits[1]);
        assert!(splits[1] < splits[2]);
        assert!(splits[2] <= 100.0);
    }

    #[test]
    fn cascade_matrix_contains_scene_points() {
        let camera = FlyCamera::default();
        let lighting = LightingUniforms::default();
        let (device, queue) = headless_queue();
        let texture_cache = crate::asset::texture_cache::GpuTextureCache::new(&device, &queue);
        let asset_manager = crate::asset::AssetManager::new();
        let optional = crate::renderer::extract::OptionalPassData::default();
        let config = FrameConfig::default();
        let frame = build_frame(
            &camera,
            &lighting,
            &queue,
            &optional,
            &config,
            &texture_cache,
            &asset_manager,
        );
        let light_dir = glam::Vec3::new(-0.6, -1.0, -0.4).normalize();
        let cascades = compute_cascades(&frame, &light_dir);

        // Test that a few world-space points map inside the first cascade's clip space.
        let test_points = [
            glam::Vec3::new(0.0, 0.0, 0.0),
            glam::Vec3::new(2.0, 0.0, 4.0),
            glam::Vec3::new(-8.0, 1.5, -8.0),
        ];
        for p in test_points {
            let clip = cascades[0].view_proj * glam::Vec4::from((p, 1.0));
            let ndc = clip.xyz() / clip.w;
            assert!(
                ndc.x >= -1.0
                    && ndc.x <= 1.0
                    && ndc.y >= -1.0
                    && ndc.y <= 1.0
                    && ndc.z >= 0.0
                    && ndc.z <= 1.0,
                "point {:?} outside cascade 0 clip space: {:?}",
                p,
                ndc
            );
        }
        drop(device);
    }

    #[test]
    fn shadow_pass_stores_terrain_geometry() {
        let camera = FlyCamera::default();
        let lighting = LightingUniforms::default();
        let (device, queue) = headless_queue();
        let texture_cache = crate::asset::texture_cache::GpuTextureCache::new(&device, &queue);
        let asset_manager = crate::asset::AssetManager::new();
        let optional = crate::renderer::extract::OptionalPassData::default();
        let config = FrameConfig::default();
        let mut terrain_geom = crate::terrain::TerrainGeometry::new(&device);
        let terrain = crate::ecs::components::Terrain {
            source: crate::scene::TerrainSource::Procedural {
                seed: 0,
                frequency: 0.05,
                amplitude: 32.0,
            },
            geometry: crate::scene::TerrainGeometry::default(),
            material: crate::asset::terrain_material::TerrainMaterial::default(),
            splatmap_path: None,
            layer_configs: Vec::new(),
        };
        terrain_geom.update(&device, &queue, &camera, 16.0 / 9.0, &terrain);

        let frame = RenderFrame {
            camera: &camera,
            aspect: 16.0 / 9.0,
            batches: std::sync::Arc::from([]),
            lighting: &lighting,
            queue: &queue,
            delta_time: 0.0,
            config: &config,
            optional: &optional,
            terrain_geometry: Some(std::sync::Arc::new(std::sync::RwLock::new(terrain_geom))),
            texture_cache: &texture_cache,
            asset_manager: &asset_manager,
        };

        let mut pass = ShadowPass::new(&device);
        pass.apply_frame(&frame);
        assert!(pass.terrain_geometry.is_some());
        assert!(!pass.terrain_geometry.unwrap().read().unwrap().chunks().is_empty());
        drop(device);
    }
}
