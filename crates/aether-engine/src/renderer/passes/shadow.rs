//! Shadow Map Pass — depth-only pass from light perspective.
//! Uses dynamic uniform offsets for per-object model matrices.

use crate::asset::mesh::Vertex;
use crate::renderer::frame::RenderFrame;
use crate::renderer::pass::{Pass, PassSignature, ResHandle};
use crate::renderer::renderable::Renderable;
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

/// Per-object model matrix for shadow rendering (256-byte aligned for dynamic offset).
#[repr(C, align(256))]
#[derive(Clone, Copy, Debug)]
pub struct ShadowObjectUniform {
    /// World-space model matrix.
    pub model: [[f32; 4]; 4],
    /// Padding to 256 bytes for dynamic uniform offset alignment.
    pub _pad: [u8; 192], // fill to 256 bytes
}
unsafe impl bytemuck::Pod for ShadowObjectUniform {}
unsafe impl bytemuck::Zeroable for ShadowObjectUniform {}

/// Shadow map pass — renders depth from the directional light's perspective.
pub struct ShadowPass {
    pipeline: wgpu::RenderPipeline,
    light_vp_buffer: wgpu::Buffer,
    light_vp_bind_group: wgpu::BindGroup,
    object_buffer: wgpu::Buffer,
    object_bind_group: wgpu::BindGroup,
    shadow_depth_handle: Option<ResHandle<ShadowDepth>>,
    renderables: Vec<Renderable>,
    light_view_proj: Mat4,
}

impl Pass for ShadowPass {
    fn name(&self) -> &str { "Shadow" }
    fn signature(&self) -> PassSignature {
        PassSignature::new("Shadow").write::<ShadowDepth>("shadow_depth", wgpu::TextureFormat::Depth32Float)
    }
    fn init(device: &wgpu::Device) -> Self { Self::new(device) }
    fn resolve(&mut self, _device: &wgpu::Device, resources: &ResourceTable) {
        self.shadow_depth_handle = Some(resources.handle::<ShadowDepth>("shadow_depth"));
    }

    fn apply_frame(&mut self, frame: &RenderFrame) {
        self.renderables = frame.renderables.to_vec();
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

        // Upload all model matrices
        let obj_size = std::mem::size_of::<ShadowObjectUniform>() as wgpu::BufferAddress;
        let mut data: Vec<u8> =
            Vec::with_capacity(self.renderables.len() * obj_size as usize);
        for r in &self.renderables {
            data.extend_from_slice(bytemuck::cast_slice(&[ShadowObjectUniform {
                model: r.transform.to_cols_array_2d(),
                _pad: [0u8; 192],
            }]));
        }
        if !data.is_empty() {
            frame.queue.write_buffer(&self.object_buffer, 0, &data);
        }
    }

    fn execute(&self, encoder: &mut wgpu::CommandEncoder, resources: &ResourceTable, _surface_view: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Shadow"), color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: resources.get(self.shadow_depth_handle.unwrap()),
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None, occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.light_vp_bind_group, &[]);

        for (i, r) in self.renderables.iter().enumerate() {
            let obj_size = std::mem::size_of::<ShadowObjectUniform>() as wgpu::BufferAddress;
            let offset = i as u32 * obj_size as u32;
            pass.set_bind_group(1, &self.object_bind_group, &[offset]);
            pass.set_vertex_buffer(0, r.mesh.vertex_buffer.slice(..));
            if let Some(ref ib) = r.mesh.index_buffer {
                pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..r.mesh.index_count, 0, 0..1);
            } else {
                pass.draw(0..r.mesh.vertex_count, 0..1);
            }
        }
    }
}

const MAX_OBJECTS: usize = 256;

impl ShadowPass {
    /// Create a new shadow pass.
    pub fn new(device: &wgpu::Device) -> Self {
        let src = r#"
struct VertexInput { @location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>, @location(3) tangent: vec4<f32>, };
struct VertexOutput { @builtin(position) clip_position: vec4<f32>, };
struct LightVP { light_view_proj: mat4x4<f32>, };
@group(0) @binding(0) var<uniform> lvp: LightVP;
struct Obj { model: mat4x4<f32>, };
@group(1) @binding(0) var<uniform> obj: Obj;
@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = lvp.light_view_proj * obj.model * vec4<f32>(in.position, 1.0);
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
        let obj_size = std::mem::size_of::<ShadowObjectUniform>() as u64;
        let obj_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("S Obj BGL"), entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0, visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: Some(std::num::NonZeroU64::new(obj_size).unwrap()) },
                count: None,
            }],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shadow PL"), bind_group_layouts: &[&vp_bgl, &obj_bgl], push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Shadow Pipeline"), layout: Some(&pl),
            vertex: wgpu::VertexState { module: &shader, entry_point: "vs_main", buffers: &[Vertex::desc()] },
            fragment: None,
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, strip_index_format: None, front_face: wgpu::FrontFace::Ccw, cull_mode: Some(wgpu::Face::Back), polygon_mode: wgpu::PolygonMode::Fill, unclipped_depth: false, conservative: false },
            depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::Less, stencil: wgpu::StencilState::default(), bias: wgpu::DepthBiasState { constant: 0, slope_scale: 0.0, clamp: 0.0 } }),
            multisample: wgpu::MultisampleState::default(), multiview: None,
        });

        let vp_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("S VP"), size: std::mem::size_of::<LightSpaceUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let vp_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("S VP BG"), layout: &vp_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: vp_buf.as_entire_binding() }],
        });

        let obj_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("S Obj"), size: MAX_OBJECTS as u64 * obj_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let obj_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("S Obj BG"), layout: &obj_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &obj_buf, offset: 0, size: Some(std::num::NonZeroU64::new(obj_size).unwrap()) }) }],
        });

        Self { pipeline, light_vp_buffer: vp_buf, light_vp_bind_group: vp_bg, object_buffer: obj_buf, object_bind_group: obj_bg, shadow_depth_handle: None, renderables: Vec::new(), light_view_proj: Mat4::IDENTITY }
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
