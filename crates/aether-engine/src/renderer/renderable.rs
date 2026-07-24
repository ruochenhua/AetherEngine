//! GPU uniform types shared across passes.
//!
//! Extracted from `passes/gbuffer.rs` because `MaterialUniform`,
//! `ObjectUniform`, and `ViewProjUniform` are consumed by multiple modules
//! (GBufferPass, ShadowPass, SceneLoader, Scheduler). Living in a single
//! pass module was a leakage.

/// Per-object uniform (material only; model matrix now from instance buffer).
#[repr(C, align(256))]
#[derive(Clone, Copy, Debug)]
pub struct ObjectUniform {
    /// Albedo color [r, g, b, a].
    pub albedo: [f32; 4],
    /// Surface roughness (0 = mirror, 1 = matte).
    pub roughness: f32,
    /// Metalness (0 = dielectric, 1 = metal).
    pub metallic: f32,
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
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MaterialUniform {
    /// Albedo color [r, g, b, a].
    pub albedo: [f32; 4],
    /// Surface roughness (0 = mirror, 1 = matte).
    pub roughness: f32,
    /// Metalness (0 = dielectric, 1 = metal).
    pub metallic: f32,
    /// Padding to 16-byte alignment.
    pub _pad: [f32; 2],
    /// Optional albedo texture handle ID (0 = none).
    pub albedo_texture_id: u64,
}

impl Default for MaterialUniform {
    fn default() -> Self {
        Self {
            albedo: [0.8, 0.3, 0.2, 1.0],
            roughness: 0.5,
            metallic: 0.0,
            _pad: [0.0, 0.0],
            albedo_texture_id: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, offset_of, size_of};

    /// Layout guard: `ObjectUniform` is mirrored by the WGSL
    /// `ObjectData { albedo: vec4<f32>, roughness: f32, metallic: f32 }`
    /// (e.g. passes/water_reflection.rs) and is indexed via dynamic uniform
    /// offsets, so its stride must stay aligned to 256 bytes.
    #[test]
    fn object_uniform_matches_gpu_layout() {
        assert_eq!(
            size_of::<ObjectUniform>(),
            256,
            "ObjectUniform stride must stay 256 bytes (dynamic uniform offset alignment)"
        );
        assert_eq!(
            align_of::<ObjectUniform>(),
            256,
            "ObjectUniform must keep its repr(C, align(256)) alignment"
        );
        // Field offsets pinned by the WGSL ObjectData mirror.
        assert_eq!(
            offset_of!(ObjectUniform, albedo),
            0,
            "albedo must map to the vec4<f32> at offset 0"
        );
        assert_eq!(
            offset_of!(ObjectUniform, roughness),
            16,
            "roughness must directly follow the albedo vec4"
        );
        assert_eq!(
            offset_of!(ObjectUniform, metallic),
            20,
            "metallic must directly follow roughness"
        );
    }
}
