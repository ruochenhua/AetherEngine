//! GPU uniform types shared across passes.
//!
//! Extracted from `passes/gbuffer.rs` because `MaterialUniform`,
//! `ObjectUniform`, and `ViewProjUniform` are consumed by multiple modules
//! (GBufferPass, ShadowPass, SceneLoader, Scheduler). Living in a single
//! pass module was a leakage.

#[cfg(test)]
#[path = "renderable_tests.rs"]
mod tests;

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
    /// Whether the object bypasses deferred lighting.
    pub unlit: u32,
    /// Normal map strength.
    pub normal_scale: f32,
    /// Occlusion contribution.
    pub occlusion_strength: f32,
    /// Emissive color multiplier.
    pub emissive: [f32; 3],
    /// Emissive intensity.
    pub emissive_intensity: f32,
    /// Whether a normal texture is bound.
    pub normal_present: u32,
    /// Whether an ORM texture is bound.
    pub orm_present: u32,
    /// Whether an emissive texture is bound.
    pub emissive_present: u32,
    /// Padding for WGSL uniform alignment.
    pub _material_pad: u32,
}

// Safety: ObjectUniform is #[repr(C, align(256))] with no invalid bit patterns
unsafe impl bytemuck::Pod for ObjectUniform {}
unsafe impl bytemuck::Zeroable for ObjectUniform {}

impl ObjectUniform {
    /// Build the GPU-facing object data from the extracted material adapter.
    pub fn from_material(material: &MaterialUniform) -> Self {
        Self {
            albedo: material.albedo,
            roughness: material.roughness,
            metallic: material.metallic,
            unlit: material.unlit,
            normal_scale: material.normal_scale,
            occlusion_strength: material.occlusion_strength,
            emissive: material.emissive,
            emissive_intensity: material.emissive_intensity,
            normal_present: u32::from(material.normal_texture_id != 0),
            orm_present: u32::from(material.orm_texture_id != 0),
            emissive_present: u32::from(material.emissive_texture_id != 0),
            _material_pad: 0,
        }
    }
}

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
    /// Whether the object bypasses deferred lighting.
    pub unlit: u32,
    /// Padding to 16-byte alignment.
    pub _pad: u32,
    /// Optional albedo texture handle ID (0 = none).
    pub albedo_texture_id: u64,
    /// Normal map strength.
    pub normal_scale: f32,
    /// Occlusion contribution.
    pub occlusion_strength: f32,
    /// Emissive color multiplier.
    pub emissive: [f32; 3],
    /// Emissive intensity.
    pub emissive_intensity: f32,
    /// Optional normal texture handle ID (0 = none).
    pub normal_texture_id: u64,
    /// Optional ORM texture handle ID (0 = none).
    pub orm_texture_id: u64,
    /// Optional emissive texture handle ID (0 = none).
    pub emissive_texture_id: u64,
}

impl MaterialUniform {
    /// Adapt one resolved material to the legacy ECS component consumed by the renderer.
    pub fn from_resolution(resolution: &crate::scene::material::MaterialResolution) -> Self {
        let material = &resolution.material;
        Self {
            albedo: material.base_color,
            roughness: material.roughness,
            metallic: material.metallic,
            unlit: u32::from(resolution.legacy_unlit),
            _pad: 0,
            albedo_texture_id: material.albedo.as_ref().map_or(0, |handle| handle.id()),
            normal_scale: material.normal_scale,
            occlusion_strength: material.occlusion_strength,
            emissive: material.emissive,
            emissive_intensity: material.emissive_intensity,
            normal_texture_id: material.normal.as_ref().map_or(0, |handle| handle.id()),
            orm_texture_id: material.orm.as_ref().map_or(0, |handle| handle.id()),
            emissive_texture_id: material
                .emissive_texture
                .as_ref()
                .map_or(0, |handle| handle.id()),
        }
    }
}

impl Default for MaterialUniform {
    fn default() -> Self {
        Self {
            albedo: [0.8, 0.3, 0.2, 1.0],
            roughness: 0.5,
            metallic: 0.0,
            unlit: 0,
            _pad: 0,
            albedo_texture_id: 0,
            normal_scale: 1.0,
            occlusion_strength: 1.0,
            emissive: [0.0; 3],
            emissive_intensity: 1.0,
            normal_texture_id: 0,
            orm_texture_id: 0,
            emissive_texture_id: 0,
        }
    }
}
