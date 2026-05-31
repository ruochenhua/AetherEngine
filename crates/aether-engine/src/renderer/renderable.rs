//! Renderable types shared across passes.
//!
//! Extracted from `passes/gbuffer.rs` because `Renderable`, `MaterialUniform`,
//! `ObjectUniform`, and `ViewProjUniform` are consumed by multiple modules
//! (GBufferPass, ShadowPass, SceneLoader, Scheduler). Living in a single
//! pass module was a leakage.

use crate::asset::mesh::GpuMesh;
use glam::Mat4;
use std::sync::Arc;

/// Per-object uniform (model + material), 256-byte aligned for dynamic offset.
#[repr(C, align(256))]
#[derive(Clone, Copy, Debug)]
pub struct ObjectUniform {
    /// Model matrix (column-major).
    pub model: [[f32; 4]; 4],
    /// Albedo color [r, g, b, a].
    pub albedo: [f32; 4],
    /// Surface roughness (0 = mirror, 1 = matte).
    pub roughness: f32,
    /// Metalness (0 = dielectric, 1 = metal).
    pub metallic: f32,
    /// Padding to 256 bytes.
    pub _pad: [u8; 168],
}

// Safety: ObjectUniform is #[repr(C, align(256))] with no invalid bit patterns
unsafe impl bytemuck::Pod for ObjectUniform {}
unsafe impl bytemuck::Zeroable for ObjectUniform {}

/// View-projection uniform (shared across draw calls).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ViewProjUniform {
    /// View matrix (column-major).
    pub view: [[f32; 4]; 4],
    /// Projection matrix (column-major).
    pub proj: [[f32; 4]; 4],
}

/// PBR material parameters for a renderable object.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MaterialUniform {
    /// Albedo color [r, g, b, a].
    pub albedo: [f32; 4],
    /// Surface roughness (0 = mirror, 1 = matte).
    pub roughness: f32,
    /// Metalness (0 = dielectric, 1 = metal).
    pub metallic: f32,
    /// Padding to 16-byte alignment.
    pub _pad: [f32; 2],
}

impl Default for MaterialUniform {
    fn default() -> Self {
        Self {
            albedo: [0.8, 0.3, 0.2, 1.0],
            roughness: 0.5,
            metallic: 0.0,
            _pad: [0.0, 0.0],
        }
    }
}

/// A renderable object — mesh + transform + material.
#[derive(Clone)]
pub struct Renderable {
    /// GPU mesh (shared ownership).
    pub mesh: Arc<GpuMesh>,
    /// Model matrix (world transform).
    pub transform: Mat4,
    /// PBR material parameters.
    pub material: MaterialUniform,
}
