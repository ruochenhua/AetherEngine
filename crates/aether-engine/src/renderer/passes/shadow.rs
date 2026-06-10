//! Shadow Map Pass — depth-only pass from light perspective.
//! Uses a vertex buffer for per-instance model matrices (GPU instancing).

use crate::asset::mesh::{InstanceData, Vertex};
use crate::renderer::extract::RenderBatch;
use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{Pass, PassSignature, ResHandle};
use crate::renderer::resource::*;
use crate::renderer::resource_table::ResourceTable;
use glam::Mat4;

/// Light-space uniform: view-projection matrix from the light's perspective.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightSpaceUniform {
    /// Combined light view-projection matrix.
    pub light_view_proj: [[f32; 4]; 4],
}

/// Fixed resolution for the shadow depth map.
pub const SHADOW_MAP_SIZE: u32 = 2048;

/// Shadow map pass — renders depth from the directional light's perspective.
pub struct ShadowPass {
    device: wgpu::Device,
    pipeline: wgpu::RenderPipeline,
    light_vp_buffer: wgpu::Buffer,
    light_vp_bind_group: wgpu::BindGroup,
    /// Per-instance transform vertex buffer.
    instance_buffer: wgpu::Buffer,
    instance_buffer_capacity: usize,
    shadow_depth_handle: Option<ResHandle<ShadowDepth>>,
    batches: Vec<RenderBatch>,
    light_view_proj: Mat4,
}

impl Pass for ShadowPass {
    fn name(&self) -> &str { "Shadow" }
    fn signature(&self) -> PassSignature {
        PassSignature::new("Shadow")
            .write_sized::<ShadowDepth>("shadow_depth", wgpu::TextureFormat::Depth32Float, SHADOW_MAP_SIZE, SHADOW_MAP_SIZE)
    }
    fn init(device: &wgpu::Device) -> Self { Self::new(device) }
    fn resolve(&mut self, _device: &wgpu::Device, resources: &ResourceTable) {
        self.shadow_depth_handle = Some(resources.handle::<ShadowDepth>("shadow_depth"));
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        self.batches = frame.batches.to_vec();
        let light_dir =
            glam::Vec3::from_array(frame.lighting.light.direction).normalize();
        self.light_view_proj = compute_light_space_matrix(&light_dir);

        // Upload light VP
        let vp = LightSpaceUniform {
            light_view_proj: self.light_view_proj.to_cols_array_2d(),
        };
        frame
            .queue
            .write_buffer(&self.light_vp_buffer, 0, bytemuck::cast_slice(&[vp]));

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
        let mut instance_data: Vec<u8> = Vec::with_capacity(total_instances * std::mem::size_of::<InstanceData>());
        for batch in &self.batches {
            instance_data.extend_from_slice(bytemuck::cast_slice(&batch.instances));
        }
        if !instance_data.is_empty() {
            frame.queue.write_buffer(&self.instance_buffer, 0, &instance_data);
        }
    }

    fn execute(&self, encoder: &mut wgpu::CommandEncoder, resources: &ResourceTable, _surface_view: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Shadow"), color_attachments: &[],
            multiview_mask: None,
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: resources.get(self.shadow_depth_handle.unwrap()),
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None, occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.light_vp_bind_group, &[]);

        let mut instance_offset = 0usize;
        for batch in &self.batches {
            let instance_count = batch.instances.len() as u32;
            pass.set_vertex_buffer(0, batch.mesh.vertex_buffer.slice(..));
            let instance_byte_start = (instance_offset * std::mem::size_of::<InstanceData>()) as wgpu::BufferAddress;
            let instance_byte_end = instance_byte_start + (batch.instances.len() * std::mem::size_of::<InstanceData>()) as wgpu::BufferAddress;
            pass.set_vertex_buffer(1, self.instance_buffer.slice(instance_byte_start..instance_byte_end));
            if let Some(ref ib) = batch.mesh.index_buffer {
                pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..batch.mesh.index_count, 0, 0..instance_count);
            } else {
                pass.draw(0..batch.mesh.vertex_count, 0..instance_count);
            }
            instance_offset += batch.instances.len();
        }
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl ShadowPass {
    /// Create a new shadow pass.
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
            label: Some("Shadow Shader"), source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(src)),
        });

        let vp_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("S VP BGL"), entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0, visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });

        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shadow PL"), bind_group_layouts: &[Some(&vp_bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Shadow Pipeline"), layout: Some(&pl),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_main"), compilation_options: Default::default(), buffers: &[Vertex::desc(), InstanceData::instance_desc()] },
            fragment: None,
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, strip_index_format: None, front_face: wgpu::FrontFace::Ccw, cull_mode: Some(wgpu::Face::Back), polygon_mode: wgpu::PolygonMode::Fill, unclipped_depth: false, conservative: false },
            depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: Some(true), depth_compare: Some(wgpu::CompareFunction::Less), stencil: wgpu::StencilState::default(), bias: wgpu::DepthBiasState { constant: 0, slope_scale: 0.0, clamp: 0.0 } }),
            multisample: wgpu::MultisampleState::default(), multiview_mask: None,
            cache: None,
        });

        let vp_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("S VP"), size: std::mem::size_of::<LightSpaceUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let vp_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("S VP BG"), layout: &vp_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: vp_buf.as_entire_binding() }],
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
            pipeline, light_vp_buffer: vp_buf, light_vp_bind_group: vp_bg,
            instance_buffer,
            instance_buffer_capacity: initial_instance_capacity,
            shadow_depth_handle: None, batches: Vec::new(), light_view_proj: Mat4::IDENTITY 
        }
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

/// Compute a WGPU-compatible orthographic light-space matrix.
///
/// The matrix maps world-space positions to clip space (z ∈ [0, 1])
/// compatible with wgpu/Vulkan depth buffer conventions.
pub fn compute_light_space_matrix(light_direction: &glam::Vec3) -> Mat4 {
    let center = glam::Vec3::ZERO;
    let half = 20.0;
    let light_pos = center - *light_direction * half;
    let up = if light_direction.x.abs() < 0.001 && light_direction.z.abs() < 0.001 { glam::Vec3::X } else { glam::Vec3::Y };
    let view = Mat4::look_at_rh(light_pos, center, up);
    let proj = ortho_wgpu(-half, half, -half, half, 0.01, 40.0);
    proj * view
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn compute_works() {
        let m = compute_light_space_matrix(&glam::Vec3::new(0.5, -1.0, 0.3).normalize());
        for c in 0..4 { for r in 0..4 { assert!(!m.col(c)[r].is_nan()); } }
    }
}
