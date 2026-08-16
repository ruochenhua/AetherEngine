//! God Ray Pass — volumetric light shafts.
//!
//! A full-screen pass that runs after SSR and before water/composite. It ray
//! marches from each screen pixel toward the sun's screen-space position,
//! samples the G-Buffer depth to detect occlusion, and writes a separate
//! `GodRayColor` overlay. The composite pass adds this overlay on top of the

use crate::renderer::pass::ResHandle;
use crate::renderer::resource::{GDepth, GodRayColor};

mod pass;
mod pipeline;
mod shaders;

/// GPU uniform data for the god ray shader.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GodRayUniform {
    /// View-projection matrix.
    pub view_proj: glam::Mat4,
    /// Inverse view-projection matrix.
    pub inv_view_proj: glam::Mat4,
    /// Camera world-space position (xyz, w unused).
    pub camera_pos: glam::Vec4,
    /// Direction toward the sun (xyz, w unused).
    pub sun_direction: glam::Vec4,
    /// x=samples, y=density, z=decay, w=weight.
    pub params: glam::Vec4,
    /// Final exposure multiplier.
    pub exposure: f32,
    /// Padding to 16-byte alignment.
    pub _pad: [f32; 3],
}

impl Default for GodRayUniform {
    fn default() -> Self {
        Self {
            view_proj: glam::Mat4::IDENTITY,
            inv_view_proj: glam::Mat4::IDENTITY,
            camera_pos: glam::Vec4::ZERO,
            sun_direction: glam::Vec4::new(0.0, 0.2, -1.0, 0.0),
            params: glam::Vec4::new(64.0, 0.5, 0.95, 0.5),
            exposure: 0.3,
            _pad: [0.0; 3],
        }
    }
}

/// God ray render pass.
pub struct GodRayPass {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    texture_bind_group: Option<wgpu::BindGroup>,
    quad_vertex_buffer: wgpu::Buffer,
    quad_vertex_count: u32,
    depth_handle: Option<ResHandle<GDepth>>,
    god_ray_color_handle: Option<ResHandle<GodRayColor>>,
    has_god_ray: bool,
}
#[cfg(test)]
mod tests;
